use alcro::UI;

#[test]
fn test_ui() {
    let _ui = UI::new("data:text/html,<html>Close Me!</html>", "", 480, 320, &[]);
}

#[test]
fn test_ui_close() {
    let ui = UI::new("data:text/html,<html>I will close in 3 seconds.You can close if you want.</html>", "", 480, 320, &[]);
    std::thread::sleep(std::time::Duration::from_secs(3));
    ui.close();
}