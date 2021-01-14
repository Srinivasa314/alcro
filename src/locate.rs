#[cfg(target_os = "windows")]
use std::env::var;
use std::path::Path;
pub use tinyfiledialogs;
use tinyfiledialogs::{message_box_yes_no, MessageBoxIcon, YesNo};

#[cfg(target_os = "macos")]
const PATHS: &[&str] = &[
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
];

#[cfg(target_os = "linux")]
const PATHS: &[&str] = &[
    "/usr/bin/google-chrome-stable",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/snap/bin/chromium",
];

#[cfg(target_family = "unix")]
fn paths() -> &'static [&'static str] {
    PATHS
}

#[cfg(target_os = "windows")]
fn paths() -> [String; 7] {
    [
        var("LocalAppData").unwrap() + "/Google/Chrome/Application/chrome.exe",
        var("ProgramFiles").unwrap() + "/Google/Chrome/Application/chrome.exe",
        var("ProgramFiles(x86)").unwrap() + "/Google/Chrome/Application/chrome.exe",
        var("LocalAppData").unwrap() + "/Chromium/Application/chrome.exe",
        var("ProgramFiles").unwrap() + "/Chromium/Application/chrome.exe",
        var("ProgramFiles(x86)").unwrap() + "/Chromium/Application/chrome.exe",
        var("ProgramFiles(x86)").unwrap() + "/Microsoft/Edge/Application/msedge.exe",
    ]
}

#[derive(Debug, thiserror::Error)]
pub enum LocateChromeError {
    #[error("An installation of chrome/chromium could not be found")]
    ChromeNotInstalledError,
    #[error("Download chrome prompt could not be displayed: {0}")]
    PromptError(#[from] PromptError),
}

pub fn locate_chrome() -> Result<String, LocateChromeError> {
    for path in paths().iter() {
        if Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }
    prompt_download()?;
    Err(LocateChromeError::ChromeNotInstalledError)
}

use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("Cannot display dialog: {0}")]
    IOError(#[from] std::io::Error),
}

fn prompt_download() -> Result<(), PromptError> {
    let title = "Chrome not found";
    let text =
        "No Chrome/Chromium installation was found. Would you like to download and install it now?";

    if message_box_yes_no(title, text, MessageBoxIcon::Question, YesNo::Yes) == YesNo::No {
        return Ok(());
    }

    let url = "https://www.google.com/chrome/";

    #[cfg(target_os = "linux")]
    Command::new("xdg-open").arg(url).spawn()?;
    #[cfg(target_os = "macos")]
    Command::new("open").arg(url).spawn()?;
    #[cfg(target_os = "windows")]
    Command::new("cmd")
        .arg("/c")
        .arg("start")
        .arg(url)
        .spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_locate_chrome() {
        assert!(locate_chrome().is_ok())
    }
}
