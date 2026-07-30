#![windows_subsystem = "windows"]

use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc, Weak,
};

use alcro::{Content, UIBuilder, UI};
use anyhow::Context;
use serde_json::to_value;

async fn bind_counter(
    ui: &UI,
    other: Weak<UI>,
    count: Arc<AtomicI32>,
    name: &str,
    delta: i32,
) -> anyhow::Result<()> {
    ui.bind(name, move |_| {
        let count = count.clone();
        let other = other.clone();
        async move {
            let c = count.fetch_add(delta, Ordering::Relaxed) + delta;
            other
                .upgrade()
                .unwrap()
                .eval(&format!(
                    "document.getElementById('count').innerText='Count: {}'",
                    c
                ))
                .await?;
            Ok(to_value(c).unwrap())
        }
    })
    .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let count = Arc::new(AtomicI32::new(0));
    let ui1 = Arc::new(
        UIBuilder::new()
            .content(Content::Html(include_str!("./multiple-windows.html")))
            .run()
            .await
            .context("Failed to launch browser")?,
    );
    // The second window shares the browser process of the first one
    let ui2 = Arc::new(
        ui1.new_window(Content::Html(include_str!("./multiple-windows.html")))
            .await
            .context("Failed to open second window")?,
    );

    bind_counter(&ui1, Arc::downgrade(&ui2), count.clone(), "increment", 1).await?;
    bind_counter(&ui1, Arc::downgrade(&ui2), count.clone(), "decrement", -1).await?;
    bind_counter(&ui2, Arc::downgrade(&ui1), count.clone(), "increment", 1).await?;
    bind_counter(&ui2, Arc::downgrade(&ui1), count.clone(), "decrement", -1).await?;

    ui1.wait_finish().await;
    ui2.wait_finish().await;
    Ok(())
}
