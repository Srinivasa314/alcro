#[cfg(target_os = "linux")]
pub fn message_box(title: &str, text: &str) -> bool {
    use std::process::Command;
    let status = Command::new("zenity")
        .arg("--question")
        .arg("--title")
        .arg(title)
        .arg("--text")
        .arg(text)
        .status()
        .expect("Failed to launch zenity");
    return status.success();
}

#[cfg(target_os = "macos")]
pub fn message_box(title: &str, text: &str) -> bool {
    use std::process::Command;
    let buttons = "{\"No\", \"Yes\"}";
    let script=format!("set T to button returned of (display dialog {} with title {} buttons {} default button \"Yes\")",text,title,buttons);
    let cmd = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .unwrap();
    return String::from_utf8_lossy(&cmd.stdout) == "Yes";
}

#[cfg(target_os = "windows")]
pub fn message_box(title: &str, text: &str) -> bool {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use winapi::um::winuser::{MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO};

    let title: Vec<u16> = OsStr::new(title).encode_wide().chain(once(0)).collect();
    let text: Vec<u16> = OsStr::new(text).encode_wide().chain(once(0)).collect();
    unsafe {
        return MessageBoxW(
            null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES;
    }
}
