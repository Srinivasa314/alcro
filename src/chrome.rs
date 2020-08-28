use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{bounded, Sender};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicI32, Ordering};

mod devtools;
use devtools::{readloop, recv_msg, send, send_msg};
mod os;
#[cfg(target_family = "windows")]
use os::close_process_handle;
#[cfg(target_family = "unix")]
use os::kill_proc;
use os::{exited, new_process, wait_proc, PipeReader, PipeWriter, Process};

/// A JS object. It is an alias for `serde_json::Value`. See it's documentation for how to use it.
pub type JSObject = serde_json::Value;
/// The result of a JS function.
///
/// The Err variant will be returned if
/// * There is an exception
/// * An error type is returned
pub type JSResult = Result<JSObject, JSObject>;
type BindingFunc = Arc<dyn Fn(&[JSObject]) -> JSResult + Sync + Send>;

pub struct Chrome {
    id: AtomicI32,
    pid: usize,
    psend: Mutex<PipeWriter>,
    precv: Mutex<PipeReader>,
    target: String,
    session: String,
    kill_send: Sender<()>,
    pending: Mutex<HashMap<i32, Sender<JSResult>>>,
    window: AtomicI32,
    bindings: Mutex<HashMap<String, BindingFunc>>,
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
    pub fn new_with_args(chrome_binary: &str, mut args: &[&str]) -> Result<Arc<Chrome>, String> {
        let (pid, precv, psend) = new_process(chrome_binary, &mut args);
        let (kill_send, kill_recv) = bounded(1);

        let mut c = Chrome {
            id: AtomicI32::new(2),
            precv: Mutex::new(precv),
            psend: Mutex::new(psend),
            target: String::new(),
            session: String::new(),
            window: AtomicI32::new(0),
            pending: Mutex::new(HashMap::new()),
            bindings: Mutex::new(HashMap::new()),
            kill_send,
            pid: pid as usize,
        };

        c.target = c.find_target();
        c.session = c.start_session()?;

        let c_arc = Arc::new(c);

        #[cfg(target_family = "unix")]
        std::thread::spawn(move || {
            kill_recv.recv().unwrap();
            kill_proc(pid);
        });

        let c_arc_clone = c_arc.clone();
        std::thread::spawn(move || readloop(c_arc_clone));

        for (method, args) in [
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
        ]
        .iter()
        {
            send(Arc::clone(&c_arc), method, args).map_err(|e| e.to_string())?;
        }

        if !args.contains(&"--headless") {
            let win_id = get_window_for_target(Arc::clone(&c_arc)).map_err(|e| e.to_string())?;
            Arc::clone(&c_arc).window.store(win_id, Ordering::Relaxed);
        }
        Ok(c_arc)
    }

    fn find_target(&self) -> String {
        send_msg(
            &self.psend,
            &json!(
            {
            "id": 0,
            "method": "Target.setDiscoverTargets",
            "params": { "discover": true }
            }
            )
            .to_string(),
        );

        loop {
            let pmsg: JSObject =
                serde_json::from_str(&recv_msg(&self.precv)).expect("Invalid JSON");
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

    fn start_session(&self) -> Result<String, String> {
        send_msg(
            &self.psend,
            &json!(
            {
            "id": 1,
            "method": "Target.attachToTarget",
            "params": {"targetId": self.target}
            }
            )
            .to_string(),
        );

        loop {
            let pmsg: JSObject =
                serde_json::from_str(&recv_msg(&self.precv)).expect("Invalid JSON");
            if pmsg["id"] == 1 {
                if pmsg["error"] != JSObject::Null {
                    return Err(pmsg["error"].to_string());
                }
                let session = &pmsg["result"];
                return Ok(session["sessionId"]
                    .as_str()
                    .expect("Value not of string datatype")
                    .to_string());
            }
        }
    }

    pub fn done(&self) -> bool {
        exited(self.pid as Process)
    }

    pub fn wait_finish(&self) {
        wait_proc(self.pid as Process)
    }
}

fn get_window_for_target(c: Arc<Chrome>) -> Result<i32, JSObject> {
    match send(
        Arc::clone(&c),
        "Browser.getWindowForTarget",
        &json!({
            "targetId": c.target
        }),
    ) {
        Ok(v) => Ok(v["windowId"].as_i64().expect("Value not i64") as i32),
        Err(e) => Err(e),
    }
}

pub fn load(c: Arc<Chrome>, url: &str) -> JSResult {
    send(Arc::clone(&c), "Page.navigate", &json!({ "url": url }))
}

pub fn eval(c: Arc<Chrome>, expr: &str) -> JSResult {
    send(
        c,
        "Runtime.evaluate",
        &json!({
            "expression": expr, "awaitPromise": true, "returnByValue": true
        }),
    )
}

pub fn set_bounds(c: Arc<Chrome>, b: Bounds) -> JSResult {
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
}

pub fn bounds(c: Arc<Chrome>) -> Result<Bounds, JSObject> {
    match send(
        Arc::clone(&c),
        "Browser.getWindowBounds",
        &json!({
            "windowId": c.window.load(Ordering::Relaxed)
        }),
    ) {
        Err(e) => Err(e),
        Ok(result) => {
            let ret: Bounds = serde_json::from_value(result["bounds"].clone())
                .expect("Value not of bounds datatype");
            Ok(ret)
        }
    }
}

pub fn bind(c: Arc<Chrome>, name: &str, f: BindingFunc) -> JSResult {
    {
        let mut bindings = c.bindings.lock().unwrap();
        bindings.insert(name.to_string(), f);
    }

    if let Err(e) = send(
        Arc::clone(&c),
        "Runtime.addBinding",
        &json!({ "name": name }),
    ) {
        return Err(e);
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
    ) {
        return Err(e);
    }
    eval(Arc::clone(&c), &script)
}

pub fn close(c: Arc<Chrome>) {
    std::thread::spawn(move || {
        if let Err(e) = send(c, "Browser.close", &json!({})) {
            eprintln!("{}", e);
        }
    });
}

#[cfg(target_family = "windows")]
pub fn close_handle(c: Arc<Chrome>) {
    close_process_handle(c.pid as Process)
}
