use super::{ActiveBindingContext, BindingContext, Chrome, JSObject, JSResult};
use super::{PipeReader, PipeWriter};
use crossbeam_channel::{bounded, Sender};
use serde_json::json;
use std::sync::{atomic::Ordering, Arc, Mutex};

pub fn send_msg(p: &Mutex<PipeWriter>, message: String) {
    p.lock()
        .expect("Unable to lock")
        .write(message)
        .expect("Unable to write to pipe");
}

pub fn recv_msg(p: &Mutex<PipeReader>) -> String {
    p.lock()
        .expect("Unable to lock")
        .read()
        .expect("Unable to read from pipe")
}

pub fn readloop(c: Arc<Chrome>) {
    loop {
        let pmsg = recv_msg(&c.precv);
        if pmsg.is_empty() {
            break;
        }
        let pmsg: JSObject = serde_json::from_str(&pmsg).expect("Invalid JSON");

        if pmsg["method"] == "Target.targetDestroyed" {
            #[cfg(target_family = "unix")]
            if pmsg["params"]["targetId"] == c.target {
                let _ = c.kill_send.send(());
                return;
            }
        } else if pmsg["method"] == "Target.receivedMessageFromTarget" {
            let params = &pmsg["params"];
            if params["sessionId"] != c.session {
                continue;
            }

            let message = params["message"]
                .as_str()
                .expect("message should be a string");
            let res: JSObject = serde_json::from_str(message).expect("Invalid JSON");

            if res["id"] == JSObject::Null && res["method"] == "Runtime.consoleAPICalled"
                || res["method"] == "Runtime.exceptionThrown"
            {
                println!("Message: {}", res);
            } else if res["id"] == JSObject::Null && res["method"] == "Runtime.bindingCalled" {
                let payload: JSObject = serde_json::from_str(
                    res["params"]["payload"]
                        .as_str()
                        .expect("payload should be a string"),
                )
                .expect("Invalid JSON");
                binding_called(
                    c.clone(),
                    res["params"]["name"].as_str().expect("Expected string"),
                    payload,
                    res["params"]["executionContextId"]
                        .as_i64()
                        .expect("Expected i64"),
                );
                continue;
            } else if res["id"].is_i64() {
                let res_id = res["id"].as_i64().expect("Expected i64") as i32;

                match c.pending.get(&res_id) {
                    None => continue,
                    Some(reschan) => {
                        send_result(&*reschan, &res);
                    }
                }
                c.pending.remove(&res_id);
            }
        }
    }
}

pub fn send(c: Arc<Chrome>, method: &str, params: &JSObject) -> JSResult {
    let id = c.id.fetch_add(1, Ordering::Relaxed) + 1;
    let json_msg = json!({
        "id":id,
        "method":method,
        "params":params
    });
    let (s, r) = bounded(1);
    c.pending.insert(id, s);

    send_msg(
        &c.psend,
        json!({
            "id":id,
            "method":"Target.sendMessageToTarget",
            "params":json!({
                "message":json_msg.to_string(),
                "sessionId":c.session
            })
        })
        .to_string(),
    );
    r.recv().unwrap()
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

fn binding_called(c: Arc<Chrome>, name: &str, payload: JSObject, context_id: i64) {
    let binding = match c.bindings.get(name) {
        Some(b) => Some(Arc::clone(&*b)),
        None => None,
    };
    if let Some(binding) = binding {
        binding(BindingContext::new(ActiveBindingContext {
            chrome: c,
            payload,
            context_id,
        }))
    }
}
