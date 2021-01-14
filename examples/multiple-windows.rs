#![windows_subsystem = "windows"]

use std::sync::{atomic::AtomicI32, Arc};

use alcro::{Content, UIBuilder, UI};
use serde_json::to_value;

fn new_window() -> Result<Arc<UI>, Box<dyn std::error::Error>> {
    let ui = Arc::new(
        UIBuilder::new()
            .content(Content::Html(include_str!("./multiple-windows.html")))
            .run()?,
    );
    Ok(ui)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = Arc::new(AtomicI32::new(0));
    let ui1 = new_window()?;
    let ui2 = new_window()?;

    ui1.bind("increment", {
        let count = count.clone();
        let ui2 = Arc::downgrade(&ui2);
        move |_| {
            let c = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ui2.upgrade()
                .unwrap()
                .eval(&format!(
                    "document.getElementById('count').innerText='Count: {}'",
                    c + 1
                ))
                .map_err(|e| e.to_string())?;
            Ok(to_value(c + 1).unwrap())
        }
    })
    .map_err(|e| e.to_string())?;

    ui1.bind("decrement", {
        let count = count.clone();
        let ui2 = Arc::downgrade(&ui2);
        move |_| {
            let c = count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            ui2.upgrade()
                .unwrap()
                .eval(&format!(
                    "document.getElementById('count').innerText='Count: {}'",
                    c - 1
                ))
                .map_err(|e| e.to_string())?;
            Ok(to_value(c - 1).unwrap())
        }
    })
    .map_err(|e| e.to_string())?;

    ui2.bind("increment", {
        let count = count.clone();
        let ui1 = Arc::downgrade(&ui1);
        move |_| {
            let c = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ui1.upgrade()
                .unwrap()
                .eval(&format!(
                    "document.getElementById('count').innerText='Count: {}'",
                    c + 1
                ))
                .map_err(|e| e.to_string())?;
            Ok(to_value(c + 1).unwrap())
        }
    })
    .map_err(|e| e.to_string())?;

    ui2.bind("decrement", {
        let count = count.clone();
        let ui1 = Arc::downgrade(&ui1);
        move |_| {
            let c = count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            ui1.upgrade()
                .unwrap()
                .eval(&format!(
                    "document.getElementById('count').innerText='Count: {}'",
                    c - 1
                ))
                .map_err(|e| e.to_string())?;
            Ok(to_value(c - 1).unwrap())
        }
    })
    .map_err(|e| e.to_string())?;

    ui1.wait_finish();
    ui2.wait_finish();
    Ok(())
}
