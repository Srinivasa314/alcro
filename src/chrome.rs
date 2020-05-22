use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{bounded, Sender};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use websocket::{sync::stream::TcpStream, ClientBuilder};

mod devtools;
use devtools::{readloop, recv_msg_from_ws, send, send_msg_to_ws};

/// A JS object
pub type JSObject = serde_json::Value;
/// The result of a JS function.
///
/// The Err variant will be returned if
/// * There is an exception
/// * An error type is returned
pub type JSResult = Result<JSObject, JSObject>;
/// The function type used in `UI::bind`.
/// It takes a list of JS objects and returns a `JSResult`.
type BindingFunc = Arc<dyn Fn(&[JSObject]) -> JSResult + Sync + Send>;

pub struct Chrome {
    id: AtomicI32,
    wssend: Mutex<websocket::sender::Writer<TcpStream>>,
    wsrecv: Mutex<websocket::receiver::Reader<TcpStream>>,
    target: String,
    session: String,
    pending: Mutex<HashMap<i32, Sender<JSResult>>>,
    window: AtomicI32,
    kill_send: Sender<()>,
    done: AtomicBool,
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
    pub fn new_with_args(chrome_binary: &str, args: Vec<String>) -> Arc<Chrome> {
        let (kill_send, kill_recv) = bounded(1);

        let mut c_cmd = Command::new(chrome_binary)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args(&args)
            .spawn()
            .expect("Cannot spawn chrome");

        let stderr = BufReader::new(c_cmd.stderr.take().unwrap());

        let re = regex::Regex::new(r"^DevTools listening on (ws://.*?)$").unwrap();

        let mut ws_url = String::new();
        for line in stderr.lines() {
            match re.captures(&line.expect("Unable to read line from stderr")) {
                None => continue,
                Some(cap) => {
                    ws_url = cap[1].to_string();
                    break;
                }
            }
        }

        let client = ClientBuilder::new(&ws_url)
            .unwrap()
            .connect_insecure()
            .unwrap();
        let (receiver, sender) = client.split().unwrap();

        let mut c = Chrome {
            id: AtomicI32::new(2),
            wsrecv: Mutex::new(receiver),
            wssend: Mutex::new(sender),
            target: String::new(),
            session: String::new(),
            window: AtomicI32::new(0),
            done: AtomicBool::new(false),
            kill_send,
            pending: Mutex::new(HashMap::new()),
            bindings: Mutex::new(HashMap::new()),
        };

        c.target = c.find_target();
        c.start_session();
        c.session = c.start_session();

        let c_arc = Arc::new(c);
        let c_arc_clone = c_arc.clone();

        std::thread::spawn(move || loop {
            if c_cmd.try_wait().expect("Error in waiting").is_some() {
                c_arc_clone.done.store(true, Ordering::SeqCst);
                break;
            } else if kill_recv.try_recv().is_ok() {
                if let Ok(ws) = c_arc_clone.wssend.lock() {
                    let _ = ws.shutdown_all();
                }
                let _ = c_cmd.kill();
                c_arc_clone.done.store(true, Ordering::SeqCst);
                break;
            }
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
            send(Arc::clone(&c_arc), method, args).unwrap();
        }

        if !args.contains(&"--headless".to_string()) {
            let win_id = get_window_for_target(Arc::clone(&c_arc)).unwrap();
            Arc::clone(&c_arc).window.store(win_id, Ordering::SeqCst);
        }
        c_arc
    }

    fn find_target(&self) -> String {
        send_msg_to_ws(
            &self.wssend,
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
            let wsmsg: JSObject =
                serde_json::from_str(&recv_msg_from_ws(&self.wsrecv).unwrap()).unwrap();
            if wsmsg["method"] == "Target.targetCreated" {
                let params = &wsmsg["params"];
                if params["targetInfo"]["type"] == "page" {
                    return params["targetInfo"]["targetId"]
                        .as_str()
                        .unwrap()
                        .to_string();
                }
            }
        }
    }

    fn start_session(&self) -> String {
        send_msg_to_ws(
            &self.wssend,
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
            loop {
                let wsmsg: JSObject =
                    serde_json::from_str(&recv_msg_from_ws(&self.wsrecv).unwrap()).unwrap();
                if wsmsg["id"] == 1 {
                    if wsmsg["error"] != JSObject::Null {
                        panic!(wsmsg["error"].to_string())
                    }
                    let session = &wsmsg["result"];
                    return session["sessionId"].as_str().unwrap().to_string();
                }
            }
        }
    }

    pub fn kill(&self) {
        if !self.done() {
            self.kill_send.send(()).expect("Receiver end closed");
        }
    }

    pub fn done(&self) -> bool {
        return self.done.load(Ordering::SeqCst);
    }

    pub fn wait_finish(&self) {
        while !self.done() {}
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
        Ok(v) => Ok(v["windowId"].as_i64().unwrap() as i32),
        Err(e) => Err(e),
    }
}

pub fn load(c: Arc<Chrome>, url: &str) -> JSResult {
    return send(Arc::clone(&c), "Page.navigate", &json!({ "url": url }));
}

pub fn eval(c: Arc<Chrome>, expr: &str) -> JSResult {
    return send(
        c,
        "Runtime.evaluate",
        &json!({
            "expression": expr, "awaitPromise": true, "returnByValue": true
        }),
    );
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
            "windowId": c.window.load(Ordering::SeqCst)
        }),
    ) {
        Err(e) => Err(e),
        Ok(result) => {
            let ret: Bounds = serde_json::from_value(result["bounds"].clone()).unwrap();
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
    return eval(Arc::clone(&c), &script);
}
