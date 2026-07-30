//! # Alcro
//!
//! Alcro is a library to create desktop apps using rust and modern web technologies.
//! It uses the existing chrome installation for the UI. The API is async and runs on
//! the [`tokio`] runtime.
//!
//! # Example
//!
//! ```no_run
//! #![windows_subsystem = "windows"]
//! use alcro::{UIBuilder, Content};
//! use serde_json::to_value;
//!
//! #[tokio::main]
//! async fn main() {
//!     let ui = UIBuilder::new().content(Content::Html("<html><body>Close Me!</body></html>")).run().await.expect("Unable to launch");
//!     assert_eq!(ui.eval("document.body.innerText").await.unwrap(), "Close Me!");
//!
//!     //Expose rust function to js
//!     ui.bind("product", |args| async move {
//!         let mut product = 1;
//!         for arg in args {
//!             match arg.as_i64() {
//!                 Some(i) => product *= i,
//!                 None => return Err(to_value("Not number").unwrap()),
//!             }
//!         }
//!         Ok(to_value(product).unwrap())
//!     }).await.expect("Unable to bind function");
//!
//!     assert_eq!(ui.eval("(async () => await product(1,2,3))();").await.unwrap(), 6);
//!     assert!(ui.eval("(async () => await product(1,2,'hi'))();").await.is_err());
//!     ui.wait_finish().await;
//! }
//! ```
//!
//! To change the path of the browser launched set the ALCRO_BROWSER_PATH environment variable. Only Chromium based browsers work.
//!

mod chrome;
use chrome::{
    bind, bounds, close, eval, launch, load, load_css, load_js, new_window, set_bounds,
    BindingFunc, LogSink, Window,
};
pub use chrome::{Bounds, JSError, JSObject, JSResult, LogOutput, WindowState};
mod locate;
pub use locate::tinyfiledialogs as dialog;
use locate::{locate_chrome, LocateChromeError};
use std::future::Future;
use std::sync::Arc;

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
    "--password-store=basic",
    "--use-mock-keychain",
];

/// A browser window.
///
/// The browser process is shared: [`UIBuilder::run()`] launches a browser with
/// its first window, and [`UI::new_window()`] opens further windows in the
/// same browser. The process exits when its last window is closed or dropped.
pub struct UI {
    window: Arc<Window>,
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
    /// Cannot create the log file
    #[error("Cannot create log file: {0}")]
    LogFileCreationError(std::io::Error),
}

impl UI {
    async fn new(
        url: &str,
        dir: Option<&std::path::Path>,
        width: i32,
        height: i32,
        custom_args: &[&str],
        log_output: Option<&LogOutput>,
    ) -> Result<UI, UILaunchError> {
        let _tmpdir;
        let dir = match dir {
            Some(dir) => {
                _tmpdir = None;
                dir
            }
            None => {
                _tmpdir = Some(tempfile::TempDir::new()?);
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
        let log_sink = match log_output {
            None => None,
            Some(LogOutput::Stdout) => Some(LogSink::Stdout),
            Some(LogOutput::Stderr) => Some(LogSink::Stderr),
            Some(LogOutput::File(path)) => Some(LogSink::File(std::sync::Mutex::new(
                std::fs::File::create(path).map_err(UILaunchError::LogFileCreationError)?,
            ))),
        };
        let window = launch(&chrome_path, &args, url, log_sink, _tmpdir).await?;
        Ok(UI { window })
    }

    /// Open another window in the same browser process and wait for its
    /// content to load. It returns Err if it fails.
    ///
    /// # Examples
    ///
    /// ```
    /// #![windows_subsystem = "windows"]
    /// use alcro::{Content, UIBuilder};
    /// # tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async {
    /// let ui = UIBuilder::new()
    ///     .content(Content::Html("<html><body>first</body></html>"))
    ///     .custom_args(&["--headless"])
    ///     .run().await.expect("Unable to launch");
    /// let ui2 = ui.new_window(Content::Html("<html><body>second</body></html>"))
    ///     .await.expect("Unable to open window");
    /// assert_eq!(ui.eval("document.body.innerText").await.unwrap(), "first");
    /// assert_eq!(ui2.eval("document.body.innerText").await.unwrap(), "second");
    /// # });
    /// ```
    pub async fn new_window(&self, content: Content<'_>) -> Result<UI, JSError> {
        let html: String;
        let url = match content {
            Content::Url(u) => u,
            Content::Html(h) => {
                html = format!("data:text/html,{}", h);
                &html
            }
        };
        let window = new_window(&self.window, url).await?;
        Ok(UI { window })
    }

    /// Returns true if this window is closed
    pub fn done(&self) -> bool {
        self.window.is_closed()
    }

    /// Wait for this window to be closed
    pub async fn wait_finish(&self) {
        self.window.wait_closed().await;
    }

    /// Close this window gracefully. The browser process exits when its last
    /// window is closed.
    pub async fn close(&self) {
        close(&self.window).await
    }

    /// Load content in the window and wait for the page to load. It returns Err if it fails.
    pub async fn load(&self, content: Content<'_>) -> Result<(), JSError> {
        let html: String;
        let url = match content {
            Content::Url(u) => u,
            Content::Html(h) => {
                html = format!("data:text/html,{}", h);
                &html
            }
        };
        load(&self.window, url).await
    }

    /// Bind a rust function so that JS code can use it. It returns Err if it fails.
    ///
    /// The function receives the arguments by value and returns a [`Future`] for the
    /// result (generally by using an `async move` block body). Each invocation from JS
    /// runs as its own tokio task, so bindings can be called concurrently and may await
    /// freely; use [`tokio::task::spawn_blocking`] inside the binding for CPU heavy or
    /// blocking work.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the function
    /// * `f` - The function. It should take a [`Vec`] of [`JSObject`] arguments by value
    ///         and return a [`Future`] for the [`JSResult`]
    ///
    /// # Examples
    ///
    /// ```
    /// #![windows_subsystem = "windows"]
    /// use alcro::UIBuilder;
    /// use serde_json::to_value;
    ///
    /// # tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async {
    /// let ui = UIBuilder::new().custom_args(&["--headless"]).run().await.expect("Unable to launch");
    /// ui.bind("add", |args| async move {
    ///     let mut sum = 0;
    ///     for arg in args {
    ///         match arg.as_i64() {
    ///             Some(i) => sum += i,
    ///             None => return Err(to_value("Not number").unwrap()),
    ///         }
    ///     }
    ///     Ok(to_value(sum).unwrap())
    /// }).await.expect("Unable to bind function");
    /// assert_eq!(ui.eval("(async () => await add(1,2,3))();").await.unwrap(), 6);
    /// assert!(ui.eval("(async () => await add(1,2,'hi'))();").await.is_err());
    /// # });
    /// ```
    pub async fn bind<F, Fut>(&self, name: &str, f: F) -> Result<(), JSError>
    where
        F: Fn(Vec<JSObject>) -> Fut + Sync + Send + 'static,
        Fut: Future<Output = JSResult> + Send + 'static,
    {
        let func: BindingFunc = Arc::new(move |args| Box::pin(f(args)));
        bind(&self.window, name, func).await
    }

    /// Evaluates js code and returns the result.
    ///
    /// # Examples
    ///
    /// ```
    /// #![windows_subsystem = "windows"]
    /// use alcro::UIBuilder;
    /// # tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async {
    /// let ui = UIBuilder::new().custom_args(&["--headless"]).run().await.expect("Unable to launch");
    /// assert_eq!(ui.eval("1+1").await.unwrap(), 2);
    /// assert_eq!(ui.eval("'Hello'+' World'").await.unwrap(), "Hello World");
    /// assert!(ui.eval("xfgch").await.is_err());
    /// # });
    /// ```
    pub async fn eval(&self, js: &str) -> JSResult {
        eval(&self.window, js).await
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
    /// # tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async {
    /// let ui = UIBuilder::new().custom_args(&["--headless"]).run().await.expect("Unable to launch");
    /// ui.load_js("function loadedFunction() { return 'This function was loaded from rust'; }").await.expect("Unable to load js");
    /// assert_eq!(ui.eval("loadedFunction()").await.unwrap(), "This function was loaded from rust");
    /// # });
    /// ```
    pub async fn load_js(&self, script: &str) -> Result<(), JSError> {
        load_js(&self.window, script).await
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
    /// # tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async {
    /// let ui = UIBuilder::new().custom_args(&["--headless"]).run().await.expect("Unable to launch");
    /// ui.load_css("body {display: none;}").await.expect("Unable to load css");
    /// # });
    /// ```
    pub async fn load_css(&self, css: &str) -> Result<(), JSError> {
        load_css(&self.window, css).await
    }

    /// It changes the size, position or state of the browser window specified by the `Bounds` struct. It returns Err if it fails.
    ///
    /// To change the window state alone use `WindowState::to_bounds()`
    pub async fn set_bounds(&self, b: Bounds) -> Result<(), JSError> {
        set_bounds(&self.window, b).await
    }

    /// It gets the size, position and state of the browser window. It returns Err if it fails.
    pub async fn bounds(&self) -> Result<Bounds, JSObject> {
        bounds(&self.window).await
    }
}

/// Dropping a `UI` closes its window; when it is the last open window of the
/// browser, the browser process is killed instead.
///
/// Drop cannot wait for a graceful shutdown; to close the browser gracefully call
/// [`UI::close()`] and [`UI::wait_finish()`] before dropping.
impl Drop for UI {
    fn drop(&mut self) {
        if self.window.is_closed() {
            return;
        }
        if !self.window.has_other_live_windows() {
            self.window.kill_browser();
        } else if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let window = self.window.clone();
            handle.spawn(async move { close(&window).await });
        }
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
    log_output: Option<LogOutput>,
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
            log_output: None,
        }
    }

    /// Launch the browser, wait for the initial page to load and return the UI instance.
    /// It returns the Err variant if any error occurs.
    pub async fn run(&self) -> Result<UI, UILaunchError> {
        let html: String;
        let url = match self.content {
            Content::Url(u) => u,
            Content::Html(h) => {
                html = format!("data:text/html,{}", h);
                &html
            }
        };
        UI::new(
            url,
            self.dir,
            self.width,
            self.height,
            self.custom_args,
            self.log_output.as_ref(),
        )
        .await
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

    /// Log the browser's console messages and uncaught exceptions to the given
    /// destination. By default they are not logged.
    pub fn log_output(&mut self, log_output: LogOutput) -> &mut Self {
        self.log_output = Some(log_output);
        self
    }
}
