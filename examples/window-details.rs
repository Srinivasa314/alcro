#![windows_subsystem = "windows"]
use alcro::{Content, JSObject, JSResult, UIBuilder, WindowState, UI};
use serde_json::to_value;
#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref ui: UI = UIBuilder::new()
        .content(Content::Html(include_str!("window-details.html")))
        .run();
}

fn window_details(_: &[JSObject]) -> JSResult {
    let bounds = ui.bounds().unwrap();
    Ok(to_value(bounds).unwrap())
}

//Toggles between maximized and normal
fn toggle(_: &[JSObject]) -> JSResult {
    let state = ui.bounds().unwrap().window_state;
    if state == WindowState::Maximized {
        ui.set_bounds(WindowState::Normal.to_bounds()).unwrap();
    } else if state == WindowState::Normal {
        ui.set_bounds(WindowState::Maximized.to_bounds()).unwrap();
    }
    Ok(JSObject::Null)
}

fn main() {
    ui.bind("windowDetails", window_details).unwrap();
    ui.bind("toggle", toggle).unwrap();
    ui.eval("printDetails()").unwrap();
    ui.wait_finish();
}
