//! # Alcro
//!
//! Alcro is a library to create desktop apps using rust and modern web technologies.
//! It uses the existing chrome installation for the UI.
//!
//! # Example
//!
//! ```no_run
//! #![windows_subsystem = "windows"]
//! use alcro::{UIBuilder, Content};
//! use serde_json::to_value;
//!
//! let ui = UIBuilder::new().content(Content::Html("<html><body>Close Me!</body></html>")).run().expect("Unable to launch");
//! assert_eq!(ui.eval("document.body.innerText").unwrap(), "Close Me!");
//!
//! //Expose rust function to js
//! ui.bind("product",|args| {
//!     let mut product = 1;
//!     for arg in args {
//!         match arg.as_i64() {
//!             Some(i) => product*=i,
//!             None => return Err(to_value("Not number").unwrap())
//!         }
//!     }
//!     Ok(to_value(product).unwrap())
//! }).expect("Unable to bind function");
//!
//! assert_eq!(ui.eval("(async () => await product(1,2,3))();").unwrap(), 6);
//! assert!(ui.eval("(async () => await product(1,2,'hi'))();").is_err());
//! ui.wait_finish();
//! ```
//!
//! To change the path of the browser launched set the ALCRO_BROWSER_PATH environment variable. Only Chromium based browsers work.
//!

mod chrome;
#[cfg(target_family = "windows")]
use chrome::close_handle;
use chrome::{bind, bounds, close, eval, load, load_css, load_js, set_bounds, Chrome};
pub use chrome::{Bounds, JSError, JSObject, JSResult, WindowState};
mod locate;
pub use locate::tinyfiledialogs as dialog;
use locate::{locate_chrome, LocateChromeError};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

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

/// The browser window
pub struct UI {
    chrome: Arc<Chrome>,
    _tmpdir: Option<tempdir::TempDir>,
    waited: AtomicBool,
}

/// Error in launching a UI window
#[derive(Debug, thiserror::Error)]
pub enum UILaunchError {
    /// Cannot create temporary directory
    #[error("Cannot create temporary directory: {0}")]
    TempDirectoryCreationError(#[from] std::io::Error),
    /// The path specified by ALCRO_BROWSER_PATH does not exist
    #[error("The path {0} specified by ALCRO_BROWSER_PATH does not exist")]
    BrowserPathInvalid(String),
    /// Error in locating chrome
    #[error("Error in locating chrome: {0}")]
    LocateChromeError(#[from] LocateChromeError),
    /// Error when initializing chrome
    #[error("Error when initializing chrome: {0}")]
    ChromeInitError(#[from] JSError),
}

impl UI {
    fn new(
        url: &str,
        dir: Option<&std::path::Path>,
        width: i32,
        height: i32,
        custom_args: &[&str],
    ) -> Result<UI, UILaunchError> {
        let _tmpdir;
        let dir = match dir {
            Some(dir) => {
                _tmpdir = None;
                dir
            }
            None => {
                _tmpdir = Some(tempdir::TempDir::new("alcro")?);
                _tmpdir.as_ref().unwrap().path()
            }
        };

        let mut args = Vec::from(DEFAULT_CHROME_ARGS);
        let user_data_dir_arg = format!("--user-data-dir={}", dir.to_str().unwrap());
        args.push(&user_data_dir_arg);
        let window_size_arg = format!("--window-size={},{}", width, height);
        args.push(&window_size_arg);
        for arg in custom_args {
            args.push(arg)
        }
        args.push("--remote-debugging-pipe");

        let app_arg;
        if custom_args.contains(&"--headless") {
            args.push(url);
        } else {
            app_arg = format!("--app={}", url);
            args.push(&app_arg);
        }
        let chrome_path = match std::env::var("ALCRO_BROWSER_PATH") {
            Ok(path) => {
                if std::fs::metadata(&path).is_ok() {
                    path
                } else {
                    return Err(UILaunchError::BrowserPathInvalid(path));
                }
            }
            Err(_) => locate_chrome()?,
        };
        let chrome = Chrome::new_with_args(&chrome_path, &args)?;
        Ok(UI {
            chrome,
            _tmpdir,
            waited: AtomicBool::new(false),
        })
    }

    /// Returns true if the browser is closed
    pub fn done(&self) -> bool {
        self.chrome.done()
    }

    /// Wait for the browser to be closed
    pub fn wait_finish(&self) {
        self.chrome.wait_finish();
        self.waited.store(true, Ordering::Relaxed);
    }

    /// Close the browser window
    pub fn close(&self) {
        close(self.chrome.clone())
    }

    /// Load content in the browser. It returns Err if it fails.
    pub fn load(&self, content: Content) -> Result<(), JSError> {
        let html: String;
        let url = match content {
            Content::Url(u) => u,
            Content::Html(h) => {
                html = format!("data:text/html,{}", h);
                &html
            }
        };
        load(self.chrome.clone(), url)
    }

    /// Bind a rust function so that JS code can use it. It returns Err if it fails.
    /// The rust function will be executed in a new thread and can be called asynchronously from Javascript
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the function
    /// * `f` - The function. It should take a list of `JSObject` as argument and return a `JSResult`
    ///
    /// # Examples
    ///
    /// ```
    /// #![windows_subsystem = "windows"]
    /// use alcro::UIBuilder;
    /// use serde_json::to_value;
    ///
    /// let ui = UIBuilder::new().custom_args(&["--headless"]).run().expect("Unable to launch");
    /// ui.bind("add",|args| {
    ///     let mut sum = 0;
    ///     for arg in args {
    ///         match arg.as_i64() {
    ///             Some(i) => sum+=i,
    ///             None => return Err(to_value("Not number").unwrap())
    ///         }
    ///     }
    ///     Ok(to_value(sum).unwrap())
    /// }).expect("Unable to bind function");
    /// assert_eq!(ui.eval("(async () => await add(1,2,3))();").unwrap(), 6);
    /// assert!(ui.eval("(async () => await add(1,2,'hi'))();").is_err());
    /// ```
    pub fn bind<F>(&self, name: &str, f: F) -> Result<(), JSError>
    where
        F: Fn(&[JSObject]) -> JSResult + Sync + Send + 'static,
    {
        bind(self.chrome.clone(), name, Arc::new(f))
    }

    /// Evaluates js code and returns the result.
    ///
    /// # Examples
    ///
    /// ```
    /// #![windows_subsystem = "windows"]
    /// use alcro::UIBuilder;
    /// let ui = UIBuilder::new().custom_args(&["--headless"]).run().expect("Unable to launch");
    /// assert_eq!(ui.eval("1+1").unwrap(), 2);
    /// assert_eq!(ui.eval("'Hello'+' World'").unwrap(), "Hello World");
    /// assert!(ui.eval("xfgch").is_err());
    /// ```

    pub fn eval(&self, js: &str) -> JSResult {
        eval(self.chrome.clone(), js)
    }

    /// Evaluates js code and adds functions before document loads. Loaded js is unloaded on reload.
    ///
    /// # Arguments
    ///
    /// * `script` - Javascript that should be loaded
    ///
    /// # Examples
    ///
    /// ```
    /// #![windows_subsystem = "windows"]
    /// use alcro::UIBuilder;
    /// let ui = UIBuilder::new().custom_args(&["--headless"]).run().expect("Unable to launch");
    /// ui.load_js("function loadedFunction() { return 'This function was loaded from rust'; }").expect("Unable to load js");
    /// assert_eq!(ui.eval("loadedFunction()").unwrap(), "This function was loaded from rust");
    /// ```

    pub fn load_js(&self, script: &str) -> Result<(), JSError> {
        load_js(self.chrome.clone(), script)
    }

    /// Loads CSS into current window. Loaded CSS is unloaded on reload.
    ///
    /// # Arguments
    ///
    /// * `css` - CSS that should be loaded
    ///
    /// # Examples
    ///
    /// ```
    /// #![windows_subsystem = "windows"]
    /// use alcro::UIBuilder;
    /// let ui = UIBuilder::new().custom_args(&["--headless"]).run().expect("Unable to launch");
    /// ui.load_css("body {display: none;}").expect("Unable to load css");
    /// ```

    pub fn load_css(&self, css: &str) -> Result<(), JSError> {
        load_css(self.chrome.clone(), css)
    }

    /// It changes the size, position or state of the browser window specified by the `Bounds` struct. It returns Err if it fails.
    ///
    /// To change the window state alone use `WindowState::to_bounds()`
    pub fn set_bounds(&self, b: Bounds) -> Result<(), JSError> {
        set_bounds(self.chrome.clone(), b)
    }

    /// It gets the size, position and state of the browser window. It returns Err if it fails.
    pub fn bounds(&self) -> Result<Bounds, JSObject> {
        bounds(self.chrome.clone())
    }
}

/// Closes the browser window
impl Drop for UI {
    fn drop(&mut self) {
        if !self.waited.load(Ordering::Relaxed) && !self.done() {
            self.close();
            self.wait_finish();
        }
        #[cfg(target_family = "windows")]
        close_handle(self.chrome.clone());
    }
}

/// Specifies the type of content shown by the browser
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Content<'a> {
    /// The URL
    Url(&'a str),
    /// HTML text
    Html(&'a str),
}

/// Builder for constructing a UI instance.
pub struct UIBuilder<'a> {
    content: Content<'a>,
    dir: Option<&'a std::path::Path>,
    width: i32,
    height: i32,
    custom_args: &'a [&'a str],
}

impl<'a> Default for UIBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> UIBuilder<'a> {
    /// Default UI
    pub fn new() -> Self {
        UIBuilder {
            content: Content::Html(""),
            dir: None,
            width: 800,
            height: 600,
            custom_args: &[],
        }
    }

    /// Return the UI instance. It returns the Err variant if any error occurs.
    pub fn run(&self) -> Result<UI, UILaunchError> {
        let html: String;
        let url = match self.content {
            Content::Url(u) => u,
            Content::Html(h) => {
                html = format!("data:text/html,{}", h);
                &html
            }
        };
        UI::new(url, self.dir, self.width, self.height, self.custom_args)
    }

    /// Set the content (url or html text)
    pub fn content(&mut self, content: Content<'a>) -> &mut Self {
        self.content = content;
        self
    }

    /// Set the user data directory. By default it is a temporary directory.
    pub fn user_data_dir(&mut self, dir: &'a std::path::Path) -> &mut Self {
        self.dir = Some(dir);
        self
    }

    /// Set the window size
    pub fn size(&mut self, width: i32, height: i32) -> &mut Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Add custom arguments to spawn chrome with
    pub fn custom_args(&mut self, custom_args: &'a [&'a str]) -> &mut Self {
        self.custom_args = custom_args;
        self
    }
}
