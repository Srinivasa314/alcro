#![windows_subsystem = "windows"]

use std::sync::{atomic::AtomicI32, Arc};

use alcro::{Content, UIBuilder, UI};
use anyhow::Context;
use serde_json::to_value;

fn new_window() -> anyhow::Result<Arc<UI>> {
    let ui = Arc::new(
        UIBuilder::new()
            .content(Content::Html(include_str!("./multiple-windows.html")))
            .run()
            .context("Failed to create new window")?,
    );
    Ok(ui)
}

fn main() -> anyhow::Result<()> {
    let count = Arc::new(AtomicI32::new(0));
    let ui1 = new_window()?;
    let ui2 = new_window()?;

    ui1.bind("increment", {
        let count = count.clone();
        let ui2 = Arc::downgrade(&ui2);
        move |_| {
            let c = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ui2.upgrade().unwrap().eval(&format!(
                "document.getElementById('count').innerText='Count: {}'",
                c + 1
            ))?;
            Ok(to_value(c + 1).unwrap())
        }
    })?;

    ui1.bind("decrement", {
        let count = count.clone();
        let ui2 = Arc::downgrade(&ui2);
        move |_| {
            let c = count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            ui2.upgrade().unwrap().eval(&format!(
                "document.getElementById('count').innerText='Count: {}'",
                c - 1
            ))?;
            Ok(to_value(c - 1).unwrap())
        }
    })?;

    ui2.bind("increment", {
        let count = count.clone();
        let ui1 = Arc::downgrade(&ui1);
        move |_| {
            let c = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ui1.upgrade().unwrap().eval(&format!(
                "document.getElementById('count').innerText='Count: {}'",
                c + 1
            ))?;
            Ok(to_value(c + 1).unwrap())
        }
    })?;

    ui2.bind("decrement", {
        let count = count.clone();
        let ui1 = Arc::downgrade(&ui1);
        move |_| {
            let c = count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            ui1.upgrade().unwrap().eval(&format!(
                "document.getElementById('count').innerText='Count: {}'",
                c - 1
            ))?;
            Ok(to_value(c - 1).unwrap())
        }
    })?;

    ui1.wait_finish();
    ui2.wait_finish();
    Ok(())
}
