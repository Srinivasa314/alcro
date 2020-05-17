use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use websocket::client::ClientBuilder;
use websocket::{Message, OwnedMessage};

pub struct Chrome {
    id: i32,
    pub cmd: Option<Child>,
    pub ws: Option<Arc<Mutex<WSChannel>>>,
    target: String,
    session: String,
    window: i32,
}

pub struct WSChannel(
    pub websocket::receiver::Reader<std::net::TcpStream>,
    pub websocket::sender::Writer<std::net::TcpStream>,
);

impl Chrome {
    pub fn new_with_args(chrome_binary: &str, args: Vec<String>) -> Chrome {
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
        //TODO c.window = 0;
        c
    }

    fn find_target(&mut self) -> String {
        self.send_msg_to_ws(
            r#"
            {
            "id": 0,
            "method": "Target.setDiscoverTargets",
            "params": { "discover": true }
            }              
            "#,
        );

        loop {
            match self
                .ws
                .as_mut()
                .unwrap()
                .lock()
                .expect("Unable to lock")
                .0
                .recv_message()
                .expect("Failed to receive websocket message")
            {
                OwnedMessage::Text(t) => {
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
                _ => panic!("Received non text from websocket"),
            }
        }
    }

    fn start_session(&mut self) -> String {
        self.send_msg_to_ws(&format!(
            r#"
            {{
            "id": 1, 
            "method": "Target.attachToTarget",
            "params": {{"targetId": "{target}"}}
            }}
            "#,
            target = self.target
        ));

        loop {
            match self
                .ws
                .as_mut()
                .unwrap()
                .lock()
                .expect("Unable to lock")
                .0
                .recv_message()
                .expect("Failed to receive websocket message")
            {
                OwnedMessage::Text(t) => {
                    let session_result: SessionResult = match serde_json::from_str(&t) {
                        Ok(result) => result,
                        Err(_) => continue,
                    };
                    if session_result.id == 1 {
                        return session_result.result.session_id;
                    }
                }
                _ => panic!("Received non text from websocket"),
            }
        }
    }

    fn send_msg_to_ws(&mut self, message: &str) {
        self.ws
            .as_mut()
            .unwrap()
            .lock()
            .expect("Unable to lock")
            .1
            .send_message(&Message::text(message))
            .expect("Unable to send message");
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
