#![windows_subsystem = "windows"]
use alcro::{Content, JSObject, UIBuilder, WindowState};
use serde_json::to_value;
use std::sync::{Arc, Weak};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ui = Arc::new(
        UIBuilder::new()
            .content(Content::Html(include_str!("window-details.html")))
            .run()?,
    );

    let ui2 = Arc::downgrade(&ui);
    ui.bind("windowDetails", move |_| {
        let ui = Weak::upgrade(&ui2).unwrap();
        let bounds = ui.bounds()?;
        Ok(to_value(bounds).unwrap())
    })
    .map_err(|e| e.to_string())?;

    let ui2 = Arc::downgrade(&ui);
    ui.bind("toggle", move |_| {
        let ui = Weak::upgrade(&ui2).unwrap();
        let state = ui.bounds()?.window_state;
        if state == WindowState::Maximized {
            ui.set_bounds(WindowState::Normal.to_bounds())?;
        } else if state == WindowState::Normal {
            ui.set_bounds(WindowState::Maximized.to_bounds())?;
        }
        Ok(JSObject::Null)
    })
    .map_err(|e| e.to_string())?;

    ui.eval("printDetails()").map_err(|e| e.to_string())?;
    ui.wait_finish();
    Ok(())
}
