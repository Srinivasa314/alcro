#[macro_use]
extern crate serde_derive;

mod chrome;
use chrome::Chrome;
mod locate;
use locate::locate_chrome;

const DEFAULT_CHROME_ARGS: &[&str] = &[
    "--disable-background-networking",
    "--disable-background-timer-throttling",
    "--disable-backgrounding-occluded-windows",
    "--disable-breakpad",
    "--disable-client-side-phishing-detection",
    "--disable-default-apps",
    "--disable-dev-shm-usage",
    "--disable-infobars",
    "--disable-extensions",
    "--disable-features=site-per-process",
    "--disable-hang-monitor",
    "--disable-ipc-flooding-protection",
    "--disable-popup-blocking",
    "--disable-prompt-on-repost",
    "--disable-renderer-backgrounding",
    "--disable-sync",
    "--disable-translate",
    "--disable-windows10-custom-titlebar",
    "--metrics-recording-only",
    "--no-first-run",
    "--no-default-browser-check",
    "--safebrowsing-disable-auto-update",
    "--enable-automation",
    "--password-store=basic",
    "--use-mock-keychain",
];

pub struct UI {
    chrome: Chrome,
    tmpdir: Option<tempdir::TempDir>,
}

impl UI {
    pub fn new(url: &str, dir: &str, width: u32, height: u32, customArgs: &[&str]) -> UI {
        let url = if url.is_empty() {
            "data:text/html,<html></html>"
        } else {
            url
        };

        let tmpdir = if dir.is_empty() {
            let t = tempdir::TempDir::new("alcro").expect("Cannot create temp directory");
            Some(t)
        } else {
            None
        };

        let dir = if dir.is_empty() {
            tmpdir.as_ref().unwrap().path().to_str().unwrap()
        } else {
            dir
        };

        let mut args: Vec<String> = Vec::from(DEFAULT_CHROME_ARGS)
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        args.push(format!("--app={}", url));
        args.push(format!("--user-data-dir={}", dir));
        args.push(format!("--window-size={},{}", width, height));
        for arg in customArgs {
            args.push(arg.to_string())
        }
        args.push("--remote-debugging-port=0".to_string());

        let chrome = Chrome::new_with_args(&locate_chrome(), args);
        UI {
            chrome,
            tmpdir,
        }
    }

    pub fn done(&self) -> bool {
        return self.chrome.done();
    }

    pub fn wait_finish(mut self) {
        self.chrome.wait_finish();
    }

    pub fn close(mut self) {
        self.chrome.kill()
    }
}