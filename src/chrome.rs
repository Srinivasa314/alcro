use std::{
    fmt::Display,
    sync::{
        atomic::{AtomicBool, AtomicI32, Ordering},
        Arc,
    },
};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{mpsc, oneshot, Mutex};

mod devtools;
use devtools::{readloop, send};
mod os;
#[cfg(target_family = "windows")]
use os::close_process_handle;
use os::{exited, kill_proc, new_process, wait_proc, PipeReader, PipeWriter, Process};

/// A JS object. It is an alias for `serde_json::Value`. See it's documentation for how to use it.
pub type JSObject = serde_json::Value;
/// The result of a JS function.
///
/// The Err variant will be returned if
/// * There is an exception
/// * An error type is returned
pub type JSResult = Result<JSObject, JSObject>;

/// An error from chrome in JSON format
#[derive(Debug)]
pub struct JSError(JSObject);
impl JSError {
    pub fn source(self) -> JSObject {
        self.0
    }
}
impl std::error::Error for JSError {}
impl Display for JSError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl From<JSObject> for JSError {
    fn from(o: JSObject) -> Self {
        Self(o)
    }
}

trait ToResultOfJSError {
    fn to_result_of_jserror(self) -> Result<(), JSError>;
}
impl ToResultOfJSError for JSResult {
    fn to_result_of_jserror(self) -> Result<(), JSError> {
        match self {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

pub type BindingFuture = std::pin::Pin<Box<dyn std::future::Future<Output = JSResult> + Send>>;
pub type BindingFunc = Arc<dyn Fn(Vec<JSObject>) -> BindingFuture + Sync + Send>;

/// Where to log the browser's console messages and uncaught exceptions.
///
/// By default they are not logged.
#[derive(Debug, Clone)]
pub enum LogOutput {
    /// Log to standard output
    Stdout,
    /// Log to standard error
    Stderr,
    /// Log to the given file
    File(std::path::PathBuf),
}

pub enum LogSink {
    Stdout,
    Stderr,
    File(std::sync::Mutex<std::fs::File>),
}

pub struct Chrome {
    id: AtomicI32,
    #[cfg(target_family = "unix")]
    pid: Process,
    #[cfg(target_family = "windows")]
    pid: usize,
    psend: Mutex<PipeWriter>,
    target: String,
    session: String,
    pending: dashmap::DashMap<i32, oneshot::Sender<JSResult>>,
    window: AtomicI32,
    bindings: dashmap::DashMap<String, BindingFunc>,
    load_send: mpsc::Sender<()>,
    load_recv: Mutex<mpsc::Receiver<()>>,
    log_sink: Option<LogSink>,
    closed: AtomicBool,
}

/// A struct that stores the size, position and window state of the browser window.

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct Bounds {
    /// x coordinate of the window
    pub left: i32,
    /// y coordinate of the window
    pub top: i32,
    /// width of the window
    pub width: i32,
    /// height of the window
    pub height: i32,
    pub window_state: WindowState,
}

/// The state of the window
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum WindowState {
    Normal,
    Maximized,
    Minimized,
    Fullscreen,
}

impl WindowState {
    /// Convert to Bounds struct
    pub fn to_bounds(self) -> Bounds {
        Bounds {
            height: 0,
            width: 0,
            top: 0,
            left: 0,
            window_state: self,
        }
    }
}

impl Chrome {
    pub async fn new_with_args(
        chrome_binary: &str,
        args: &[&str],
        url: &str,
        log_sink: Option<LogSink>,
    ) -> Result<Arc<Chrome>, JSError> {
        let (pid, read_file, write_file) =
            new_process(chrome_binary, &args).expect("Unable to launch chrome");
        let mut precv = PipeReader::new(read_file).expect("Unable to open browser pipe");
        let mut psend = PipeWriter::new(write_file).expect("Unable to open browser pipe");

        let target = find_target(&mut psend, &mut precv).await;
        let session = start_session(&mut psend, &mut precv, &target).await?;

        let (load_send, load_recv) = mpsc::channel(1);

        let c_arc = Arc::new(Chrome {
            id: AtomicI32::new(2),
            psend: Mutex::new(psend),
            target,
            session,
            window: AtomicI32::new(0),
            pending: dashmap::DashMap::new(),
            bindings: dashmap::DashMap::new(),
            load_send,
            load_recv: Mutex::new(load_recv),
            log_sink,
            closed: AtomicBool::new(false),
            #[cfg(target_family = "windows")]
            pid: pid as usize,
            #[cfg(target_family = "unix")]
            pid,
        });

        tokio::spawn(readloop(Arc::clone(&c_arc), precv));

        for (method, params) in [
            ("Page.enable", JSObject::Null),
            (
                "Target.setAutoAttach",
                json!({"autoAttach": true, "waitForDebuggerOnStart": false}),
            ),
            ("Network.enable", JSObject::Null),
            ("Runtime.enable", JSObject::Null),
            ("Security.enable", JSObject::Null),
            ("Performance.enable", JSObject::Null),
            ("Log.enable", JSObject::Null),
            ("DOM.enable", JSObject::Null),
            ("CSS.enable", JSObject::Null),
        ]
        .iter()
        {
            send(Arc::clone(&c_arc), method, params).await?;
        }

        if !args.contains(&"--headless") {
            let win_id = get_window_for_target(Arc::clone(&c_arc)).await?;
            c_arc.window.store(win_id, Ordering::Relaxed);
        }

        load(Arc::clone(&c_arc), url).await?;
        Ok(c_arc)
    }

    fn log(&self, msg: &JSObject) {
        use std::io::Write;
        match &self.log_sink {
            None => {}
            Some(LogSink::Stdout) => println!("Message: {}", msg),
            Some(LogSink::Stderr) => eprintln!("Message: {}", msg),
            Some(LogSink::File(f)) => {
                let mut f = f.lock().expect("Unable to lock");
                let _ = writeln!(f, "Message: {}", msg);
            }
        }
    }

    pub fn done(&self) -> bool {
        exited(self.pid as Process).expect("Error in getting process state")
    }

    pub async fn wait_finish(&self) {
        let pid = self.pid;
        tokio::task::spawn_blocking(move || wait_proc(pid as Process))
            .await
            .expect("Wait task panicked")
            .expect("Error in waiting for process")
    }

    /// Synchronous best-effort kill of the browser process, for use in Drop.
    pub fn kill(&self) {
        let _ = kill_proc(self.pid as Process);
        let _ = wait_proc(self.pid as Process);
    }
}

async fn find_target(psend: &mut PipeWriter, precv: &mut PipeReader) -> String {
    psend
        .write(
            json!(
            {
            "id": 0,
            "method": "Target.setDiscoverTargets",
            "params": { "discover": true }
            }
            )
            .to_string(),
        )
        .await
        .expect("Unable to write to pipe");

    loop {
        let pmsg: JSObject =
            serde_json::from_str(&precv.read().await.expect("Unable to read from pipe"))
                .expect("Invalid JSON");
        if pmsg["method"] == "Target.targetCreated" {
            let params = &pmsg["params"];
            if params["targetInfo"]["type"] == "page" {
                return params["targetInfo"]["targetId"]
                    .as_str()
                    .expect("Value not of string datatype")
                    .to_string();
            }
        }
    }
}

async fn start_session(
    psend: &mut PipeWriter,
    precv: &mut PipeReader,
    target: &str,
) -> Result<String, JSError> {
    psend
        .write(
            json!(
            {
            "id": 1,
            "method": "Target.attachToTarget",
            "params": {"targetId": target}
            }
            )
            .to_string(),
        )
        .await
        .expect("Unable to write to pipe");

    loop {
        let pmsg: JSObject =
            serde_json::from_str(&precv.read().await.expect("Unable to read from pipe"))
                .expect("Invalid JSON");
        if pmsg["id"] == 1 {
            if pmsg["error"] != JSObject::Null {
                return Err(pmsg["error"].clone().into());
            }
            let session = &pmsg["result"];
            return Ok(session["sessionId"]
                .as_str()
                .expect("Value not of string datatype")
                .to_string());
        }
    }
}

async fn get_window_for_target(c: Arc<Chrome>) -> Result<i32, JSObject> {
    match send(
        Arc::clone(&c),
        "Browser.getWindowForTarget",
        &json!({
            "targetId": c.target
        }),
    )
    .await
    {
        Ok(v) => Ok(v["windowId"].as_i64().expect("Value not i64") as i32),
        Err(e) => Err(e),
    }
}

pub async fn load(c: Arc<Chrome>, url: &str) -> Result<(), JSError> {
    let mut load_recv = c.load_recv.lock().await;
    while load_recv.try_recv().is_ok() {}
    send(Arc::clone(&c), "Page.navigate", &json!({ "url": url }))
        .await
        .to_result_of_jserror()?;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(30), load_recv.recv()).await;
    Ok(())
}

pub async fn eval(c: Arc<Chrome>, expr: &str) -> JSResult {
    send(
        c,
        "Runtime.evaluate",
        &json!({
            "expression": expr, "awaitPromise": true, "returnByValue": true
        }),
    )
    .await
}

pub async fn set_bounds(c: Arc<Chrome>, b: Bounds) -> Result<(), JSError> {
    let param = json!({
        "windowId": c.window,
        "bounds": if b.window_state != WindowState::Normal {
            json!({
                "windowState":b.window_state
            })
        }else {
            serde_json::to_value(b).unwrap()
        }
    });
    send(c, "Browser.setWindowBounds", &param)
        .await
        .to_result_of_jserror()
}

pub async fn bounds(c: Arc<Chrome>) -> Result<Bounds, JSObject> {
    match send(
        Arc::clone(&c),
        "Browser.getWindowBounds",
        &json!({
            "windowId": c.window.load(Ordering::Relaxed)
        }),
    )
    .await
    {
        Err(e) => Err(e),
        Ok(result) => {
            let ret: Bounds = serde_json::from_value(result["bounds"].clone())
                .expect("Value not of bounds datatype");
            Ok(ret)
        }
    }
}

pub async fn load_js(c: Arc<Chrome>, script: &str) -> Result<(), JSError> {
    if let Err(e) = send(
        Arc::clone(&c),
        "Page.addScriptToEvaluateOnNewDocument",
        &json!({ "source": script }),
    )
    .await
    {
        return Err(e.into());
    }
    eval(Arc::clone(&c), &script).await.to_result_of_jserror()
}

pub async fn load_css(c: Arc<Chrome>, css: &str) -> Result<(), JSError> {
    let frame_tree = match send(
        Arc::clone(&c),
        "Page.getFrameTree",
        &json!({ "targetId": c.target }),
    )
    .await
    {
        Ok(ft) => ft,
        Err(e) => return Err(e.into()),
    };
    let frame_id = frame_tree["frameTree"]["frame"]["id"].as_str().unwrap();
    let style_sheet = match send(
        Arc::clone(&c),
        "CSS.createStyleSheet",
        &json!({ "frameId": frame_id }),
    )
    .await
    {
        Ok(ss) => ss,
        Err(e) => return Err(e.into()),
    };
    let style_sheet_id = style_sheet["styleSheetId"].as_str().unwrap();
    send(
        Arc::clone(&c),
        "CSS.setStyleSheetText",
        &json!({ "styleSheetId": style_sheet_id, "text": css }),
    )
    .await
    .to_result_of_jserror()
}

pub async fn bind(c: Arc<Chrome>, name: &str, f: BindingFunc) -> Result<(), JSError> {
    c.bindings.insert(name.to_string(), f);

    if let Err(e) = send(
        Arc::clone(&c),
        "Runtime.addBinding",
        &json!({ "name": name }),
    )
    .await
    {
        return Err(e.into());
    }

    let script = format!(
        r"(()=>{{
        const bindingName = '{name}';
        const binding = window[bindingName];
        window[bindingName] = async (...args) => {{
            const me = window[bindingName];
            let errors = me['errors'];
            let callbacks = me['callbacks'];
            if (!callbacks) {{
                callbacks = new Map();
                me['callbacks'] = callbacks;
            }}
            if (!errors) {{
                errors = new Map();
                me['errors'] = errors;
            }}
            const seq = (me['lastSeq'] || 0) + 1;
            me['lastSeq'] = seq;
            const promise = new Promise((resolve, reject) => {{
                callbacks.set(seq, resolve);
                errors.set(seq, reject);
            }});
            binding(JSON.stringify({{name: bindingName, seq, args}}));
            return promise;
        }}}})();
   ",
        name = name
    );

    if let Err(e) = send(
        Arc::clone(&c),
        "Page.addScriptToEvaluateOnNewDocument",
        &json!({ "source": script }),
    )
    .await
    {
        return Err(e.into());
    }
    eval(Arc::clone(&c), &script).await.to_result_of_jserror()
}

pub async fn close(c: Arc<Chrome>) {
    if let Err(e) = send(c, "Browser.close", &json!({})).await {
        eprintln!("{}", e);
    }
}

#[cfg(target_family = "windows")]
pub fn close_handle(c: Arc<Chrome>) {
    close_process_handle(c.pid as Process).expect("Unable to close handle")
}
