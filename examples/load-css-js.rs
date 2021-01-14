#![windows_subsystem = "windows"]
use alcro::{Content, UIBuilder};

fn main() -> anyhow::Result<()> {
    let index_content = include_str!("load-css-js/index.html");
    let script = include_str!("load-css-js/js/script.js");
    let css = include_str!("load-css-js/css/style.css");
    let ui = UIBuilder::new()
        .content(Content::Html(index_content))
        .run()?;

    ui.load_js(script)?;
    ui.load_css(css)?;

    ui.wait_finish();
    Ok(())
}
