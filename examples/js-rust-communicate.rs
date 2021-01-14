#![windows_subsystem = "windows"]
use alcro::{Content, JSError, UIBuilder};
use serde_json::to_value;

fn main() -> anyhow::Result<()> {
    let ui = UIBuilder::new()
        .content(Content::Html(include_str!("js-rust-communicate.html")))
        .run()?;

    //Rust calling JS
    assert_eq!(
        ui.eval("document.getElementById('title').innerText")
            .unwrap(),
        "JS Rust Communication"
    );
    ui.eval("document.getElementById('result').innerText='Type the file name in the input box and click the button the result will be displayed'").map_err(|e|JSError::from(e))?;

    ui.bind("readFile", |args| {
        if args.len() == 0 {
            Err(to_value("File name required").unwrap())
        } else {
            match args[0].as_str() {
                Some(name) => match std::fs::read_to_string(name) {
                    Ok(result) => Ok(to_value(result).unwrap()),
                    Err(_) => Err(to_value("File cannot be read").unwrap()),
                },
                None => Err(to_value("Argument should be a string").unwrap()),
            }
        }
    })?;
    ui.wait_finish();
    Ok(())
}
