use alcro::UI;

#[test]
fn test_ui_manual_close() {
    let mut ui = UI::new("data:text/html,<html>Close Me!</html>", "", 480, 320, &[]);
    ui.wait_finish();
}

#[test]
fn test_ui_close_after_3_secs() {
    let mut ui = UI::new(
        "data:text/html,<html>I will close in 3 seconds.You can close if you want.</html>",
        "",
        480,
        320,
        &[],
    );
    std::thread::sleep(std::time::Duration::from_secs(3));
    ui.close();
}

#[test]
fn test_auto_drop() {
    let ui = UI::new(
        "data:text/html,<html>You wouldn't be able to see me!</html>",
        "",
        480,
        320,
        &[],
    );
}
