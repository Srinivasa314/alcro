#![windows_subsystem = "windows"]
use alcro::{Content, JSError, UIBuilder};
use serde_json::to_value;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ui = UIBuilder::new()
        .content(Content::Html(include_str!("js-rust-communicate.html")))
        .run()
        .await?;

    //Rust calling JS
    assert_eq!(
        ui.eval("document.getElementById('title').innerText")
            .await
            .unwrap(),
        "JS Rust Communication"
    );
    ui.eval("document.getElementById('result').innerText='Type the file name in the input box and click the button the result will be displayed'").await.map_err(JSError::from)?;

    ui.bind("readFile", |args| async move {
        if args.is_empty() {
            return Err(to_value("File name required").unwrap());
        }
        match args[0].as_str() {
            Some(name) => match tokio::fs::read_to_string(name).await {
                Ok(result) => Ok(to_value(result).unwrap()),
                Err(_) => Err(to_value("File cannot be read").unwrap()),
            },
            None => Err(to_value("Argument should be a string").unwrap()),
        }
    })
    .await?;
    ui.wait_finish().await;
    Ok(())
}
