use std::{
    fmt::Display,
    sync::{
        atomic::{AtomicBool, AtomicI32, Ordering},
        Arc, Weak,
    },
};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{mpsc, oneshot, watch, Mutex};

mod devtools;
use devtools::{readloop, send, send_browser};
mod os;
#[cfg(target_family = "windows")]
use os::close_process_handle;
use os::{kill_proc, new_process, wait_proc, PipeReader, PipeWriter, Process};

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

/// The browser process, shared by all of its windows.
pub struct Chrome {
    id: AtomicI32,
    #[cfg(target_family = "unix")]
    pid: Process,
    #[cfg(target_family = "windows")]
    pid: usize,
    psend: Mutex<PipeWriter>,
    pending: dashmap::DashMap<i32, oneshot::Sender<JSResult>>,
    pending_browser: dashmap::DashMap<i32, oneshot::Sender<JSResult>>,
    windows: dashmap::DashMap<String, Weak<Window>>,
    headless: bool,
    log_sink: Option<LogSink>,
    closed: AtomicBool,
    _tmpdir: Option<tempfile::TempDir>,
}

/// One browser window (a devtools target with its own session).
pub struct Window {
    chrome: Arc<Chrome>,
    target: String,
    session: String,
    window_id: AtomicI32,
    bindings: dashmap::DashMap<String, BindingFunc>,
    load_send: mpsc::Sender<()>,
    load_recv: Mutex<mpsc::Receiver<()>>,
    closed_tx: watch::Sender<bool>,
    closed_rx: watch::Receiver<bool>,
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

    fn kill_process(&self) {
        let _ = kill_proc(self.pid as Process);
    }
}

/// The browser process is killed when the last reference to it (via its
/// windows) is dropped.
impl Drop for Chrome {
    fn drop(&mut self) {
        let _ = kill_proc(self.pid as Process);
        let _ = wait_proc(self.pid as Process);
        #[cfg(target_family = "windows")]
        let _ = close_process_handle(self.pid as Process);
    }
}

impl Window {
    /// Returns true if this window has been closed
    pub fn is_closed(&self) -> bool {
        *self.closed_rx.borrow()
    }

    /// Wait until this window is closed
    pub async fn wait_closed(&self) {
        let mut rx = self.closed_rx.clone();
        let _ = rx.wait_for(|closed| *closed).await;
    }

    /// Returns true if any other window of the same browser is still open
    pub fn has_other_live_windows(&self) -> bool {
        self.chrome.windows.iter().any(|e| {
            e.key() != &self.session
                && e.value()
                    .upgrade()
                    .map_or(false, |w| !w.is_closed())
        })
    }

    /// Synchronous best-effort kill of the whole browser, for use in Drop.
    pub fn kill_browser(&self) {
        self.chrome.kill_process();
    }
}

/// Launch the browser process and return its first window.
pub async fn launch(
    chrome_binary: &str,
    args: &[&str],
    url: &str,
    log_sink: Option<LogSink>,
    tmpdir: Option<tempfile::TempDir>,
) -> Result<Arc<Window>, JSError> {
    let (pid, read_file, write_file) =
        new_process(chrome_binary, &args).expect("Unable to launch chrome");
    let mut precv = PipeReader::new(read_file).expect("Unable to open browser pipe");
    let mut psend = PipeWriter::new(write_file).expect("Unable to open browser pipe");

    let target = find_target(&mut psend, &mut precv).await;
    let session = start_session(&mut psend, &mut precv, &target).await?;

    let c_arc = Arc::new(Chrome {
        id: AtomicI32::new(2),
        psend: Mutex::new(psend),
        pending: dashmap::DashMap::new(),
        pending_browser: dashmap::DashMap::new(),
        windows: dashmap::DashMap::new(),
        headless: args.contains(&"--headless"),
        log_sink,
        closed: AtomicBool::new(false),
        _tmpdir: tmpdir,
        #[cfg(target_family = "windows")]
        pid: pid as usize,
        #[cfg(target_family = "unix")]
        pid,
    });

    let window = register_window(&c_arc, target, session);
    tokio::spawn(readloop(Arc::clone(&c_arc), precv));

    init_window(&window, url).await?;
    Ok(window)
}

/// Open another window in the same browser process.
pub async fn new_window(w: &Arc<Window>, url: &str) -> Result<Arc<Window>, JSError> {
    let c = &w.chrome;
    let mut params = json!({ "url": "about:blank" });
    if !c.headless {
        params["newWindow"] = json!(true);
    }
    let target = send_browser(c, "Target.createTarget", &params)
        .await
        .map_err(JSError::from)?["targetId"]
        .as_str()
        .expect("Value not of string datatype")
        .to_string();
    let session = send_browser(c, "Target.attachToTarget", &json!({ "targetId": target }))
        .await
        .map_err(JSError::from)?["sessionId"]
        .as_str()
        .expect("Value not of string datatype")
        .to_string();

    let window = register_window(c, target, session);
    init_window(&window, url).await?;
    Ok(window)
}

fn register_window(c: &Arc<Chrome>, target: String, session: String) -> Arc<Window> {
    let (load_send, load_recv) = mpsc::channel(1);
    let (closed_tx, closed_rx) = watch::channel(false);
    let window = Arc::new(Window {
        chrome: Arc::clone(c),
        target,
        session: session.clone(),
        window_id: AtomicI32::new(0),
        bindings: dashmap::DashMap::new(),
        load_send,
        load_recv: Mutex::new(load_recv),
        closed_tx,
        closed_rx,
    });
    c.windows.insert(session, Arc::downgrade(&window));
    window
}

/// Enable the devtools domains on a fresh session and load the initial url.
async fn init_window(w: &Arc<Window>, url: &str) -> Result<(), JSError> {
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
        send(w, method, params).await?;
    }

    if !w.chrome.headless {
        let win_id = send(
            w,
            "Browser.getWindowForTarget",
            &json!({ "targetId": w.target }),
        )
        .await
        .map_err(JSError::from)?["windowId"]
            .as_i64()
            .expect("Value not i64") as i32;
        w.window_id.store(win_id, Ordering::Relaxed);
    }

    load(w, url).await
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

pub async fn load(w: &Arc<Window>, url: &str) -> Result<(), JSError> {
    let mut load_recv = w.load_recv.lock().await;
    while load_recv.try_recv().is_ok() {}
    send(w, "Page.navigate", &json!({ "url": url }))
        .await
        .to_result_of_jserror()?;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(30), load_recv.recv()).await;
    Ok(())
}

pub async fn eval(w: &Arc<Window>, expr: &str) -> JSResult {
    send(
        w,
        "Runtime.evaluate",
        &json!({
            "expression": expr, "awaitPromise": true, "returnByValue": true
        }),
    )
    .await
}

pub async fn set_bounds(w: &Arc<Window>, b: Bounds) -> Result<(), JSError> {
    let param = json!({
        "windowId": w.window_id.load(Ordering::Relaxed),
        "bounds": if b.window_state != WindowState::Normal {
            json!({
                "windowState":b.window_state
            })
        }else {
            serde_json::to_value(b).unwrap()
        }
    });
    send(w, "Browser.setWindowBounds", &param)
        .await
        .to_result_of_jserror()
}

pub async fn bounds(w: &Arc<Window>) -> Result<Bounds, JSObject> {
    match send(
        w,
        "Browser.getWindowBounds",
        &json!({
            "windowId": w.window_id.load(Ordering::Relaxed)
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

pub async fn load_js(w: &Arc<Window>, script: &str) -> Result<(), JSError> {
    if let Err(e) = send(
        w,
        "Page.addScriptToEvaluateOnNewDocument",
        &json!({ "source": script }),
    )
    .await
    {
        return Err(e.into());
    }
    eval(w, &script).await.to_result_of_jserror()
}

pub async fn load_css(w: &Arc<Window>, css: &str) -> Result<(), JSError> {
    let frame_tree = match send(w, "Page.getFrameTree", &json!({ "targetId": w.target })).await {
        Ok(ft) => ft,
        Err(e) => return Err(e.into()),
    };
    let frame_id = frame_tree["frameTree"]["frame"]["id"].as_str().unwrap();
    let style_sheet = match send(w, "CSS.createStyleSheet", &json!({ "frameId": frame_id })).await {
        Ok(ss) => ss,
        Err(e) => return Err(e.into()),
    };
    let style_sheet_id = style_sheet["styleSheetId"].as_str().unwrap();
    send(
        w,
        "CSS.setStyleSheetText",
        &json!({ "styleSheetId": style_sheet_id, "text": css }),
    )
    .await
    .to_result_of_jserror()
}

pub async fn bind(w: &Arc<Window>, name: &str, f: BindingFunc) -> Result<(), JSError> {
    w.bindings.insert(name.to_string(), f);

    if let Err(e) = send(w, "Runtime.addBinding", &json!({ "name": name })).await {
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
        w,
        "Page.addScriptToEvaluateOnNewDocument",
        &json!({ "source": script }),
    )
    .await
    {
        return Err(e.into());
    }
    eval(w, &script).await.to_result_of_jserror()
}

/// Close this window. The browser process exits when its last window closes.
pub async fn close(w: &Arc<Window>) {
    if let Err(e) = send_browser(
        &w.chrome,
        "Target.closeTarget",
        &json!({ "targetId": w.target }),
    )
    .await
    {
        eprintln!("{}", e);
    }
}
