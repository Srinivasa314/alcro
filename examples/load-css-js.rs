#![windows_subsystem = "windows"]
use alcro::{Content, UIBuilder};

fn  main() -> Result<(), Box<dyn std::error::Error>> {
    let index_content = include_str!("load-css-js/index.html");
    let script = include_str!("load-css-js/js/script.js");
    let css = include_str!("load-css-js/css/style.css");
    let ui = UIBuilder::new()
        .content(Content::Html(index_content))
        .run()?;

    ui.load_js(script).unwrap();
    ui.load_css(css).unwrap();
    
    ui.wait_finish();
    Ok(())
}
