use super::{BindingFunc, Chrome, JSObject, JSResult, PipeReader};
use serde_json::json;
use std::sync::{atomic::Ordering, Arc};
use tokio::sync::oneshot;

pub async fn readloop(c: Arc<Chrome>, mut precv: PipeReader) {
    loop {
        let pmsg = match precv.read().await {
            Ok(msg) => msg,
            Err(_) => break,
        };
        if pmsg.is_empty() {
            break;
        }
        let pmsg: JSObject = serde_json::from_str(&pmsg).expect("Invalid JSON");

        if pmsg["method"] == "Target.targetDestroyed" {
            #[cfg(target_family = "unix")]
            if pmsg["params"]["targetId"] == c.target {
                let _ = super::os::kill_proc(c.pid);
                break;
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

            if res["id"] == JSObject::Null && res["method"] == "Page.loadEventFired" {
                let _ = c.load_send.try_send(());
            } else if res["id"] == JSObject::Null && res["method"] == "Runtime.consoleAPICalled"
                || res["method"] == "Runtime.exceptionThrown"
            {
                c.log(&res);
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

                if let Some((_, reschan)) = c.pending.remove(&res_id) {
                    send_result(reschan, &res);
                }
            }
        }
    }
    // The browser is gone: fail all in-flight and future requests instead of
    // leaving their callers waiting forever.
    c.closed.store(true, Ordering::Relaxed);
    c.pending.clear();
}

pub async fn send(c: Arc<Chrome>, method: &str, params: &JSObject) -> JSResult {
    if c.closed.load(Ordering::Relaxed) {
        return Err(browser_closed_error());
    }
    let id = c.id.fetch_add(1, Ordering::Relaxed) + 1;
    let json_msg = json!({
        "id":id,
        "method":method,
        "params":params
    });
    let (s, r) = oneshot::channel();
    c.pending.insert(id, s);

    let message = json!({
        "id":id,
        "method":"Target.sendMessageToTarget",
        "params":json!({
            "message":json_msg.to_string(),
            "sessionId":c.session
        })
    })
    .to_string();

    if let Err(e) = c.psend.lock().await.write(message).await {
        c.pending.remove(&id);
        return Err(JSObject::String(format!("Unable to write to pipe: {}", e)));
    }

    match r.await {
        Ok(result) => result,
        Err(_) => Err(browser_closed_error()),
    }
}

fn browser_closed_error() -> JSObject {
    JSObject::String("Browser has been closed".to_string())
}

fn send_result(reschan: oneshot::Sender<JSResult>, res: &JSObject) {
    let result = if res["error"]["message"] != JSObject::Null {
        Err(res["error"]["message"].clone())
    } else if res["result"]["exceptionDetails"]["exception"]["value"] != JSObject::Null {
        Err(res["result"]["exceptionDetails"]["exception"]["value"].clone())
    } else if res["result"]["result"]["type"] == "object"
        && res["result"]["result"]["subtype"] == "error"
    {
        Err(res["result"]["result"]["description"].clone())
    } else if res["result"]["result"]["type"] != JSObject::Null {
        Ok(res["result"]["result"]["value"].clone())
    } else {
        Ok(res["result"].clone())
    };
    let _ = reschan.send(result);
}

fn binding_called(c: Arc<Chrome>, name: &str, payload: JSObject, context_id: i64) {
    let binding: Option<BindingFunc> = c.bindings.get(name).map(|b| Arc::clone(&*b));
    if let Some(binding) = binding {
        let args = payload["args"].as_array().cloned().unwrap_or_default();
        // The future is created inline (cheap for an async fn: no user code
        // runs until it is polled) and then driven on its own task so that
        // bindings never block the message loop.
        let fut = binding(args);
        tokio::spawn(async move {
            let result = fut.await;
            complete_binding(c, payload, context_id, result).await;
        });
    }
}

async fn complete_binding(c: Arc<Chrome>, payload: JSObject, context_id: i64, result: JSResult) {
    let (r, e) = match result {
        Ok(x) => (x.to_string(), r#""""#.to_string()),
        Err(e) => ("".to_string(), e.to_string()),
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
        name = payload["name"].as_str().expect("Expected string"),
        seq = payload["seq"].as_i64().expect("Expected i64"),
        result = r,
        error = e
    );

    if let Err(e) = send(
        c,
        "Runtime.evaluate",
        &json!({
            "expression":expr,
            "contextId":context_id
        }),
    )
    .await
    {
        eprintln!("{}", e);
    }
}
