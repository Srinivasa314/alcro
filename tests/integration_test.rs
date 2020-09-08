use alcro::{Content, UIBuilder};

#[test]
fn test_content() {
    let ui = UIBuilder::new()
        .content(Content::Html("<html><body>Close Me!</body></html>"))
        .custom_args(&["--headless"])
        .run()
        .expect("Unable to launch");
    assert_eq!(ui.eval("document.body.innerText").unwrap(), "Close Me!");

    let ui2 = UIBuilder::new()
        .content(Content::Url("https://www.google.com"))
        .custom_args(&["--headless"])
        .run()
        .expect("Unable to launch");
    assert_eq!(
        ui2.eval("window.location.href").unwrap(),
        "https://www.google.com/"
    );
}

#[test]
fn test_eval() {
    let ui = UIBuilder::new()
        .custom_args(&["--headless"])
        .run()
        .expect("Unable to launch");
    assert_eq!(ui.eval("2+2").unwrap(), 4);
    assert_eq!(ui.eval("Promise.resolve('Its Ok')").unwrap(), "Its Ok");
    assert_eq!(ui.eval("Promise.reject('ERROR')").unwrap_err(), "ERROR");
    assert_eq!(ui.eval("throw 'ERROR'").unwrap_err(), "ERROR");
    assert!(ui.eval("dtyfhgxnt*").is_err());
}
