#![windows_subsystem = "windows"]
use alcro::{Content, JSObject, UIBuilder, WindowState};
use serde_json::to_value;
use std::sync::Arc;

fn main() {
    let ui = Arc::new(
        UIBuilder::new()
            .content(Content::Html(include_str!("window-details.html")))
            .run(),
    );

    let ui_cloned = ui.clone();
    ui.bind("windowDetails", move |_| {
        let bounds = ui_cloned.bounds().unwrap();
        Ok(to_value(bounds).unwrap())
    })
    .unwrap();

    let ui_cloned = ui.clone();
    ui.bind("toggle", move |_| {
        let state = ui_cloned.bounds().unwrap().window_state;
        if state == WindowState::Maximized {
            ui_cloned
                .set_bounds(WindowState::Normal.to_bounds())
                .unwrap();
        } else if state == WindowState::Normal {
            ui_cloned
                .set_bounds(WindowState::Maximized.to_bounds())
                .unwrap();
        }
        Ok(JSObject::Null)
    })
    .unwrap();

    ui.eval("printDetails()").unwrap();
    ui.wait_finish();
}
