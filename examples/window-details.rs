#![windows_subsystem = "windows"]
use alcro::{Content, JSError, JSObject, UIBuilder, WindowState};
use serde_json::to_value;
use std::sync::{Arc, Weak};

fn main() -> anyhow::Result<()> {
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
    })?;

    let ui2 = Arc::downgrade(&ui);
    ui.bind("toggle", move |_| {
        let ui = Weak::upgrade(&ui2).unwrap();
        let state = ui.bounds()?.window_state;
        if state == WindowState::Maximized {
            ui.set_bounds(WindowState::Normal.to_bounds())
                .map_err(|e| e.source())?;
        } else if state == WindowState::Normal {
            ui.set_bounds(WindowState::Maximized.to_bounds())
                .map_err(|e| e.source())?;
        }
        Ok(JSObject::Null)
    })?;

    ui.eval("printDetails()").map_err(|e| JSError::from(e))?;
    ui.wait_finish();
    Ok(())
}
