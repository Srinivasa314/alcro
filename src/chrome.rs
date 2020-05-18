use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use websocket::client::ClientBuilder;
use websocket::{Message, OwnedMessage};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

pub struct Chrome {
    id: i32,
    pub cmd: Option<Child>,
    pub ws: Option<Arc<Mutex<WSChannel>>>,
    target: String,
    session: String,
    window: i32,
    kill_send: mpsc::Sender<()>,
    done: Arc<AtomicBool>,
    killing_thread: Option<std::thread::JoinHandle<()>>,
}

pub struct WSChannel(
    pub websocket::receiver::Reader<std::net::TcpStream>,
    pub websocket::sender::Writer<std::net::TcpStream>,
);

fn send_msg_to_ws(ws: Arc<Mutex<WSChannel>>, message: &str) {
    ws.lock()
        .expect("Unable to lock")
        .1
        .send_message(&Message::text(message))
        .expect("Unable to send message");
}

fn recv_msg_from_ws(ws: Arc<Mutex<WSChannel>>) -> String {
    match ws
        .lock()
        .expect("Unable to lock")
        .0
        .recv_message()
        .expect("Failed to receive websocket message")
    {
        OwnedMessage::Text(t) => t,
        _ => panic!("Received non text from websocket"),
    }
}

impl Chrome {
    pub fn new_with_args(chrome_binary: &str, args: Vec<String>) -> Chrome {
        let (kill_send, kill_recv) = mpsc::channel();

        let mut c = Chrome {
            id: 2,
            cmd: Some(
                Command::new(chrome_binary)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .args(args)
                    .spawn()
                    .expect("Cannot spawn chrome"),
            ),
            ws: None,
            target: String::new(),
            session: String::new(),
            window: 0,
            done: Arc::new(AtomicBool::new(false)),
            kill_send,
            killing_thread: None,
        };

        let stderr = BufReader::new(c.cmd.as_mut().unwrap().stderr.take().unwrap());
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

        let ws = ClientBuilder::new(&ws_url).unwrap().connect_insecure();
        let (ws_send, ws_recv) = ws.expect("Websocket connection failed").split().unwrap();
        c.ws = Some(Arc::new(Mutex::new(WSChannel(ws_send, ws_recv))));

        c.target = c.find_target();
        c.start_session();
        c.session = c.start_session();
        //TODO:Remove this
        println!("Session={}", c.session);
        //TODO c.readloop... c.window = 0;
        let done_cloned = Arc::clone(&c.done);
        let mut chrome_cmd = c.cmd.take().unwrap();
        let chrome_ws = Arc::clone(&c.ws.as_ref().unwrap());

        c.killing_thread = Some(std::thread::spawn(move || loop {
            if chrome_cmd.try_wait().expect("Error in waiting").is_some() {
                done_cloned.store(true, Ordering::SeqCst);
                break;
            } else if kill_recv.try_recv().is_ok() {
                chrome_ws
                    .lock()
                    .expect("Unable to lock")
                    .0
                    .shutdown_all()
                    .expect("Unable to shutdown");
                chrome_cmd.kill().expect("Unable to kill chrome");
                done_cloned.store(true, Ordering::SeqCst);
                break;
            }
        }));
        c
    }

    fn find_target(&self) -> String {
        send_msg_to_ws(
            Arc::clone(&self.ws.as_ref().unwrap()),
            r#"
            {
            "id": 0,
            "method": "Target.setDiscoverTargets",
            "params": { "discover": true }
            }              
            "#,
        );

        loop {
            let t = recv_msg_from_ws(Arc::clone(&self.ws.as_ref().unwrap()));
            let wsresult: WSResult<TargetCreatedParams> = match serde_json::from_str(&t) {
                Ok(result) => result,
                Err(_) => continue,
            };
            if wsresult.method == "Target.targetCreated"
                && wsresult.params.target_info.r#type == "page"
            {
                return wsresult.params.target_info.target_id;
            }
        }
    }

    fn start_session(&self) -> String {
        send_msg_to_ws(
            Arc::clone(&self.ws.as_ref().unwrap()),
            &format!(
                r#"
            {{
            "id": 1, 
            "method": "Target.attachToTarget",
            "params": {{"targetId": "{target}"}}
            }}
            "#,
                target = self.target
            ),
        );

        loop {
            let t = recv_msg_from_ws(Arc::clone(&self.ws.as_ref().unwrap()));
            let session_result: SessionResult = match serde_json::from_str(&t) {
                Ok(result) => result,
                Err(_) => continue,
            };
            if session_result.id == 1 {
                return session_result.result.session_id;
            }
        }
    }

    pub fn kill(&mut self) {
        match &mut self.cmd {
            Some(proc) => {
                if let Some(ws) = &self.ws {
                    ws.lock()
                        .expect("Unable to lock")
                        .0
                        .shutdown_all()
                        .expect("Unable to shutdown");
                };
                proc.kill().expect("Chrome process not running");
            }
            None => {
                if !self.done() {
                    self.kill_send.send(()).expect("Receiver end closed");
                }
            }
        }
    }

    pub fn done(&self) -> bool {
        return self.done.load(Ordering::SeqCst);
    }

    pub fn wait_finish(&mut self) {
        match self.killing_thread.take().unwrap().join() {
            Ok(_) => (),
            Err(e) => panic!(e),
        }
    }
}

impl Drop for Chrome {
    fn drop(&mut self) {
        if !self.done() {
            self.kill();
            self.wait_finish();
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WSResult<T> {
    method: String,
    params: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetCreatedParams {
    target_info: TargetInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetInfo {
    target_id: String,
    r#type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionResult {
    id: i32,
    result: SessionId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionId {
    session_id: String,
}
