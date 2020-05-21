#![windows_subsystem = "windows"]
use alcro::{Content, JSObject, JSResult, UIBuilder};
use serde_json::to_value;

fn add_one(args: &[JSObject]) -> JSResult {
    if args.len() != 1 {
        Err(to_value("One argument needed").unwrap())
    } else {
        match args[0].as_i64() {
            Some(i) => Ok(to_value(i + 1).unwrap()),
            None => Err(to_value("Not an integer").unwrap()),
        }
    }
}

fn main() {
    let ui = UIBuilder::new()
        .content(Content::Html(include_str!("js-rust-communicate.html")))
        .run();
    assert_eq!(ui.eval("1+1").unwrap(), 2); //Rust calling js
    ui.bind("addOne", add_one).unwrap();
    ui.wait_finish();
}
