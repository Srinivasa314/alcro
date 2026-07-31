use super::{Chrome, JSObject, JSResult, LoadEvent, PipeReader, Window};
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
            let target = pmsg["params"]["targetId"].as_str().unwrap_or("");
            let destroyed = c.windows.iter().find_map(|e| {
                e.value()
                    .upgrade()
                    .filter(|w| w.target == target)
                    .map(|w| (e.key().clone(), w))
            });
            if let Some((session, window)) = destroyed {
                c.windows.remove(&session);
                let _ = window.closed_tx.send(true);
                // Fail this window's in-flight commands: their nested
                // responses will never arrive now that the target is gone.
                let stale: Vec<i32> = c
                    .pending
                    .iter()
                    .filter(|e| e.value().0 == session)
                    .map(|e| *e.key())
                    .collect();
                for id in stale {
                    if let Some((_, (_, reschan))) = c.pending.remove(&id) {
                        let _ = reschan.send(Err(window_closed_error()));
                    }
                }
            }
            // Prune windows whose handles were dropped, then exit once no
            // window is left (and none is being created): the browser
            // process is no longer needed.
            c.windows.retain(|_, w| w.upgrade().is_some());
            if c.windows.is_empty() && c.windows_in_creation.load(Ordering::SeqCst) == 0 {
                c.kill_process();
                break;
            }
        } else if pmsg["method"] == "Target.receivedMessageFromTarget" {
            let params = &pmsg["params"];
            let session = params["sessionId"].as_str().unwrap_or("");
            let window = c.windows.get(session).and_then(|w| w.upgrade());

            let message = params["message"]
                .as_str()
                .expect("message should be a string");
            let res: JSObject = serde_json::from_str(message).expect("Invalid JSON");

            if res["id"] == JSObject::Null && res["method"] == "Page.loadEventFired" {
                if let Some(window) = window {
                    let _ = window.load_send.send(LoadEvent::Loaded);
                }
            } else if res["id"] == JSObject::Null && res["method"] == "Page.frameNavigated" {
                let frame = &res["params"]["frame"];
                if frame["parentId"] == JSObject::Null {
                    if let Some(window) = window {
                        let _ = window.load_send.send(LoadEvent::Navigated(
                            frame["loaderId"].as_str().unwrap_or("").to_string(),
                        ));
                    }
                }
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
                if let Some(window) = window {
                    binding_called(
                        window,
                        res["params"]["name"].as_str().expect("Expected string"),
                        payload,
                        res["params"]["executionContextId"]
                            .as_i64()
                            .expect("Expected i64"),
                    );
                }
                continue;
            } else if res["id"].is_i64() {
                let res_id = res["id"].as_i64().expect("Expected i64") as i32;

                if let Some((_, (_, reschan))) = c.pending.remove(&res_id) {
                    send_result(reschan, &res);
                }
            }
        } else if pmsg["id"].is_i64() {
            // Top level messages are responses to browser level commands, or
            // acknowledgements of Target.sendMessageToTarget (whose ids live
            // in `pending` and are resolved by the nested response instead —
            // unless the browser reports an error for them).
            let res_id = pmsg["id"].as_i64().expect("Expected i64") as i32;
            if let Some((_, reschan)) = c.pending_browser.remove(&res_id) {
                send_result(reschan, &pmsg);
            } else if pmsg["error"] != JSObject::Null {
                if let Some((_, (_, reschan))) = c.pending.remove(&res_id) {
                    let _ = reschan.send(Err(pmsg["error"]["message"].clone()));
                }
            }
        }
    }
    // The browser is gone: fail all in-flight and future requests and mark
    // every window closed instead of leaving their callers waiting forever.
    c.closed.store(true, Ordering::Relaxed);
    c.pending.clear();
    c.pending_browser.clear();
    for e in c.windows.iter() {
        if let Some(window) = e.value().upgrade() {
            let _ = window.closed_tx.send(true);
        }
    }
    c.windows.clear();
}

pub async fn send(w: &Arc<Window>, method: &str, params: &JSObject) -> JSResult {
    let c = &w.chrome;
    if c.closed.load(Ordering::Relaxed) {
        return Err(browser_closed_error());
    }
    if w.is_closed() {
        return Err(window_closed_error());
    }
    let id = c.id.fetch_add(1, Ordering::Relaxed) + 1;
    let json_msg = json!({
        "id":id,
        "method":method,
        "params":params
    });
    let (s, r) = oneshot::channel();
    c.pending.insert(id, (w.session.clone(), s));

    let message = json!({
        "id":id,
        "method":"Target.sendMessageToTarget",
        "params":json!({
            "message":json_msg.to_string(),
            "sessionId":w.session
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

pub async fn send_browser(c: &Arc<Chrome>, method: &str, params: &JSObject) -> JSResult {
    if c.closed.load(Ordering::Relaxed) {
        return Err(browser_closed_error());
    }
    let id = c.id.fetch_add(1, Ordering::Relaxed) + 1;
    let (s, r) = oneshot::channel();
    c.pending_browser.insert(id, s);

    let message = json!({
        "id":id,
        "method":method,
        "params":params
    })
    .to_string();

    if let Err(e) = c.psend.lock().await.write(message).await {
        c.pending_browser.remove(&id);
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

fn window_closed_error() -> JSObject {
    JSObject::String("Window has been closed".to_string())
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

fn binding_called(w: Arc<Window>, name: &str, payload: JSObject, context_id: i64) {
    let binding = w.bindings.get(name).map(|b| Arc::clone(&*b));
    if let Some(binding) = binding {
        let args = payload["args"].as_array().cloned().unwrap_or_default();
        // The future is created inline (cheap for an async fn: no user code
        // runs until it is polled) and then driven on its own task so that
        // bindings never block the message loop.
        let fut = binding(args);
        tokio::spawn(async move {
            let result = fut.await;
            complete_binding(w, payload, context_id, result).await;
        });
    }
}

async fn complete_binding(w: Arc<Window>, payload: JSObject, context_id: i64, result: JSResult) {
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
        &w,
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
