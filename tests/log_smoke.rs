use alcro::{Content, LogOutput, UIBuilder};

#[test]
fn test_log_to_file() {
    let logfile = std::env::temp_dir().join("alcro_log_smoke.txt");
    let _ = std::fs::remove_file(&logfile);

    let ui = UIBuilder::new()
        .content(Content::Html("<html><body></body></html>"))
        .custom_args(&["--headless"])
        .log_output(LogOutput::File(logfile.clone()))
        .run()
        .expect("Unable to launch");
    ui.eval("console.log('hello from js')").unwrap();

    let mut contents = String::new();
    for _ in 0..50 {
        contents = std::fs::read_to_string(&logfile).unwrap_or_default();
        if contents.contains("hello from js") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(contents.contains("hello from js"), "log file: {contents}");
    let _ = std::fs::remove_file(&logfile);
}
