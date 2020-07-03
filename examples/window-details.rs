#![windows_subsystem = "windows"]
use alcro::{Content, JSObject, UIBuilder, WindowState};
use serde_json::to_value;
use std::sync::{Arc, Weak};

fn main() {
    let ui = Arc::new(
        UIBuilder::new()
            .content(Content::Html(include_str!("window-details.html")))
            .run(),
    );

    let ui2 = Arc::downgrade(&ui);
    ui.bind("windowDetails", move |_| {
        let ui = Weak::upgrade(&ui2).unwrap();
        let bounds = ui.bounds().unwrap();
        Ok(to_value(bounds).unwrap())
    })
    .unwrap();

    let ui2 = Arc::downgrade(&ui);
    ui.bind("toggle", move |_| {
        let ui = Weak::upgrade(&ui2).unwrap();
        let state = ui.bounds().unwrap().window_state;
        if state == WindowState::Maximized {
            ui.set_bounds(WindowState::Normal.to_bounds()).unwrap();
        } else if state == WindowState::Normal {
            ui.set_bounds(WindowState::Maximized.to_bounds()).unwrap();
        }
        Ok(JSObject::Null)
    })
    .unwrap();

    ui.eval("printDetails()").unwrap();
    ui.wait_finish();
}
