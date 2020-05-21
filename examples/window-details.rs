#![windows_subsystem = "windows"]
use alcro::{Bounds, Content, JSObject, JSResult, UIBuilder, UI};
use serde_json::json;
#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref ui: UI = UIBuilder::new()
        .content(Content::Html(include_str!("window-details.html")))
        .run();
}

fn window_details(_: &[JSObject]) -> JSResult {
    let bounds = ui.bounds().unwrap();
    Ok(json!({
        "x":bounds.left,
        "y":bounds.top,
        "height":bounds.height,
        "width":bounds.width,
        "state":bounds.window_state
    }))
}

//Toggles between maximized and normal
fn toggle(_: &[JSObject]) -> JSResult {
    let state = ui.bounds().unwrap().window_state;
    if state == "maximized" {
        ui.set_bounds(Bounds {
            window_state: "normal".to_string(),
            height: 0,
            width: 0,
            top: 0,
            left: 0,
        })
        .unwrap();
    } else if state == "normal" {
        ui.set_bounds(Bounds {
            window_state: "maximized".to_string(),
            height: 0,
            width: 0,
            top: 0,
            left: 0,
        })
        .unwrap();
    }
    Ok(JSObject::Null)
}

fn main() {
    ui.bind("windowDetails", window_details).unwrap();
    ui.bind("toggle", toggle).unwrap();
    ui.eval("printDetails()").unwrap();
    ui.wait_finish();
}
