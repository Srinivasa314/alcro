#![windows_subsystem = "windows"]
use alcro::{Content, JSError, JSObject, UIBuilder, WindowState};
use serde_json::to_value;
use std::sync::{Arc, Weak};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ui = Arc::new(
        UIBuilder::new()
            .content(Content::Html(include_str!("window-details.html")))
            .run()
            .await?,
    );

    let ui2 = Arc::downgrade(&ui);
    ui.bind("windowDetails", move |_| {
        let ui2 = ui2.clone();
        async move {
            let ui = Weak::upgrade(&ui2).unwrap();
            let bounds = ui.bounds().await?;
            Ok(to_value(bounds).unwrap())
        }
    })
    .await?;

    let ui2 = Arc::downgrade(&ui);
    ui.bind("toggle", move |_| {
        let ui2 = ui2.clone();
        async move {
            let ui = Weak::upgrade(&ui2).unwrap();
            let state = ui.bounds().await?.window_state;
            if state == WindowState::Maximized {
                ui.set_bounds(WindowState::Normal.to_bounds())
                    .await
                    .map_err(|e| e.source())?;
            } else if state == WindowState::Normal {
                ui.set_bounds(WindowState::Maximized.to_bounds())
                    .await
                    .map_err(|e| e.source())?;
            }
            Ok(JSObject::Null)
        }
    })
    .await?;

    ui.eval("printDetails()").await.map_err(JSError::from)?;
    ui.wait_finish().await;
    Ok(())
}
