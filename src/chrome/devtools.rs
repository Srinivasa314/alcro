use super::{BindingFunc, Chrome, JSObject, JSResult};
use crossbeam_channel::{bounded, Sender};
use serde_json::json;
use std::sync::{atomic::Ordering, Arc, Mutex};
use websocket::{sync::stream::TcpStream, Message, OwnedMessage};

pub fn send_msg_to_ws(ws: &Mutex<websocket::sender::Writer<TcpStream>>, message: &str) {
    ws.lock()
        .expect("Unable to lock")
        .send_message(&Message::text(message))
        .expect("Unable to send message");
}

pub fn recv_msg_from_ws(ws: &Mutex<websocket::receiver::Reader<TcpStream>>) -> Option<String> {
    match ws.lock().expect("Unable to lock").recv_message() {
        Ok(OwnedMessage::Text(t)) => Some(t),
        Ok(_) => panic!("Received non UTF8 data"),
        Err(_) => None,
    }
}

pub fn readloop(c: Arc<Chrome>) {
    loop {
        match recv_msg_from_ws(&c.wsrecv) {
            Some(wsmsg) => {
                let wsmsg: JSObject = serde_json::from_str(&wsmsg).unwrap();

                if wsmsg["method"] == "Target.targetDestroyed" {
                    if wsmsg["params"]["targetId"] == c.target {
                        c.kill();
                        return;
                    }
                } else if wsmsg["method"] == "Target.receivedMessageFromTarget" {
                    let params = &wsmsg["params"];
                    if params["sessionId"] != c.session {
                        continue;
                    }

                    let message = params["message"].as_str().unwrap();
                    let res: JSObject = serde_json::from_str(message).unwrap();

                    if res["id"] == JSObject::Null && res["method"] == "Runtime.consoleAPICalled"
                        || res["method"] == "Runtime.exceptionThrown"
                    {
                        println!("Message: {}", res);
                    } else if res["id"] == JSObject::Null
                        && res["method"] == "Runtime.bindingCalled"
                    {
                        let payload: JSObject =
                            serde_json::from_str(res["params"]["payload"].as_str().unwrap())
                                .unwrap();
                        binding_called(
                            c.clone(),
                            res["params"]["name"].as_str().unwrap(),
                            payload,
                            res["params"]["executionContextId"].as_i64(),
                        );
                        continue;
                    } else if res["id"].is_i64() {
                        let mut pending = c.pending.lock().unwrap();
                        let res_id = res["id"].as_i64().unwrap() as i32;

                        match pending.get(&res_id) {
                            None => continue,
                            Some(reschan) => {
                                send_result(reschan, &res);
                            }
                        }
                        pending.remove(&res_id);
                    }
                }
            }
            None => return,
        }
    }
}

pub fn send(c: Arc<Chrome>, method: &str, params: &JSObject) -> JSResult {
    let id = c.id.fetch_add(1, Ordering::SeqCst) + 1;
    let json_msg = json!({
        "id":id,
        "method":method,
        "params":params
    });
    let (s, r) = bounded(1);
    {
        c.pending.lock().unwrap().insert(id, s);
    };

    send_msg_to_ws(
        &c.wssend,
        &json!({
            "id":id,
            "method":"Target.sendMessageToTarget",
            "params":json!({
                "message":json_msg.to_string(),
                "sessionId":c.session
            })
        })
        .to_string(),
    );
    let res = r.recv().unwrap();
    res
}

fn send_result(reschan: &Sender<JSResult>, res: &JSObject) {
    if res["error"]["message"] != JSObject::Null {
        reschan.send(Err(res["error"]["message"].clone())).unwrap();
    } else if res["result"]["exceptionDetails"]["exception"]["value"] != JSObject::Null {
        reschan
            .send(Err(
                res["result"]["exceptionDetails"]["exception"]["value"].clone()
            ))
            .unwrap();
    } else if res["result"]["result"]["type"] == "object"
        && res["result"]["result"]["subtype"] == "error"
    {
        reschan
            .send(Err(res["result"]["result"]["description"].clone()))
            .unwrap();
    } else if res["result"]["result"]["type"] != JSObject::Null {
        reschan
            .send(Ok(res["result"]["result"]["value"].clone()))
            .unwrap();
    } else {
        reschan.send(Ok(res["result"].clone())).unwrap();
    }
}

fn binding_called(c: Arc<Chrome>, name: &str, payload: JSObject, context_id: Option<i64>) {
    let binding: Option<BindingFunc>;
    {
        let bindings = c.bindings.lock().unwrap();
        binding = match bindings.get(name) {
            Some(b) => Some(Arc::clone(b)),
            None => None,
        }
    }

    if let Some(binding) = binding {
        let c = Arc::clone(&c);
        std::thread::spawn(move || {
            let result: Result<String, String> = match binding(payload["args"].as_array().unwrap())
            {
                Err(e) => Err(e.to_string()),
                Ok(v) => Ok(v.to_string()),
            };

            let (r, e) = match result {
                Ok(x) => (x, r#""""#.to_string()),
                Err(e) => ("".to_string(), e),
            };

            let expr = format!(
                r"
                if ({error}) {{
                    window['{name}']['errors'].get({seq})({error});
                }} else {{
                    window['{name}']['callbacks'].get({seq})({result});
                }}
                window['{name}']['callbacks'].delete({seq});
                window['{name}']['errors'].delete({seq});
                ",
                name = payload["name"].as_str().unwrap(),
                seq = payload["seq"].as_i64().unwrap(),
                result = r,
                error = e
            );

            send(
                Arc::clone(&c),
                "Runtime.evaluate",
                &json!({
                    "expression":expr,
                    "contextId":context_id.unwrap()
                }),
            )
            .unwrap();
        });
    }
}
