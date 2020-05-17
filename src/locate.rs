#[cfg(target_os = "windows")]
use std::env::var;
use std::path::Path;

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
	return PATHS;
}

#[cfg(target_os = "windows")]
fn paths() -> [String; 6] {
	return [
		var("LocalAppData").unwrap() + "/Google/Chrome/Application/chrome.exe",
		var("ProgramFiles").unwrap() + "/Google/Chrome/Application/chrome.exe",
		var("ProgramFiles(x86)").unwrap() + "/Google/Chrome/Application/chrome.exe",
		var("LocalAppData").unwrap() + "/Chromium/Application/chrome.exe",
		var("ProgramFiles").unwrap() + "/Chromium/Application/chrome.exe",
		var("ProgramFiles(x86)").unwrap() + "/Chromium/Application/chrome.exe",
	];
}

pub fn locate_chrome() -> String {
	for path in paths().iter() {
		if Path::new(path).exists() {
			return path.to_string();
		}
	}
	panic!("Cannot find chrome"); //TODO:FIX
}
