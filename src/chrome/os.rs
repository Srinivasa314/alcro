#[derive(Debug, thiserror::Error)]
pub enum PipeReadError {
    #[error("Invalid UTF-8")]
    InvalidUtf8Error,
    #[error("Cannot read data from pipe")]
    IOError(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum PipeWriteError {
    #[error("Null character present in string")]
    NullCharacterPresent,
    #[error("Cannot write data to pipe: {0}")]
    IOError(#[from] std::io::Error),
}

/// Reads null-terminated messages from the browser pipe asynchronously.
/// Returns an empty string on EOF (browser closed the pipe).
#[cfg(target_family = "unix")]
pub struct PipeReader {
    pipe: tokio::io::BufReader<tokio::net::unix::pipe::Receiver>,
}

#[cfg(target_family = "unix")]
impl PipeReader {
    /// Must be called from within a tokio runtime.
    pub fn new(f: std::fs::File) -> std::io::Result<Self> {
        Ok(Self {
            pipe: tokio::io::BufReader::new(tokio::net::unix::pipe::Receiver::from_file(f)?),
        })
    }

    pub async fn read(&mut self) -> Result<String, PipeReadError> {
        use tokio::io::AsyncBufReadExt;
        let mut bytes_to_read = vec![];
        self.pipe.read_until(0, &mut bytes_to_read).await?;
        if bytes_to_read.last() == Some(&0) {
            bytes_to_read.pop();
        }
        String::from_utf8(bytes_to_read).map_err(|_| PipeReadError::InvalidUtf8Error)
    }
}

#[cfg(target_family = "unix")]
pub struct PipeWriter {
    pipe: tokio::net::unix::pipe::Sender,
}

#[cfg(target_family = "unix")]
impl PipeWriter {
    /// Must be called from within a tokio runtime.
    pub fn new(f: std::fs::File) -> std::io::Result<Self> {
        Ok(Self {
            pipe: tokio::net::unix::pipe::Sender::from_file(f)?,
        })
    }

    pub async fn write(&mut self, message: String) -> Result<(), PipeWriteError> {
        use tokio::io::AsyncWriteExt;
        match std::ffi::CString::new(message) {
            Ok(cstr) => Ok(self.pipe.write_all(cstr.as_bytes_with_nul()).await?),
            Err(_) => Err(PipeWriteError::NullCharacterPresent),
        }
    }
}

// Anonymous pipes on Windows do not support overlapped (async) I/O, so reads
// happen on a dedicated thread that forwards messages over a channel, and
// writes go through spawn_blocking.
#[cfg(target_family = "windows")]
pub struct PipeReader {
    rx: tokio::sync::mpsc::UnboundedReceiver<Result<String, PipeReadError>>,
}

#[cfg(target_family = "windows")]
impl PipeReader {
    pub fn new(f: std::fs::File) -> std::io::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || {
            let mut pipe = std::io::BufReader::new(f);
            loop {
                use std::io::BufRead;
                let mut bytes_to_read = vec![];
                match pipe.read_until(0, &mut bytes_to_read) {
                    Ok(_) => {
                        if bytes_to_read.is_empty() {
                            break; // EOF
                        }
                        if bytes_to_read.last() == Some(&0) {
                            bytes_to_read.pop();
                        }
                        let msg = String::from_utf8(bytes_to_read)
                            .map_err(|_| PipeReadError::InvalidUtf8Error);
                        let was_err = msg.is_err();
                        if tx.send(msg).is_err() || was_err {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.into()));
                        break;
                    }
                }
            }
        });
        Ok(Self { rx })
    }

    pub async fn read(&mut self) -> Result<String, PipeReadError> {
        match self.rx.recv().await {
            Some(msg) => msg,
            None => Ok(String::new()), // EOF
        }
    }
}

#[cfg(target_family = "windows")]
pub struct PipeWriter {
    pipe: std::sync::Arc<std::sync::Mutex<std::fs::File>>,
}

#[cfg(target_family = "windows")]
impl PipeWriter {
    pub fn new(f: std::fs::File) -> std::io::Result<Self> {
        Ok(Self {
            pipe: std::sync::Arc::new(std::sync::Mutex::new(f)),
        })
    }

    pub async fn write(&mut self, message: String) -> Result<(), PipeWriteError> {
        let cstr = match std::ffi::CString::new(message) {
            Ok(cstr) => cstr,
            Err(_) => return Err(PipeWriteError::NullCharacterPresent),
        };
        let pipe = self.pipe.clone();
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            pipe.lock()
                .expect("Unable to lock")
                .write_all(cstr.as_bytes_with_nul())
        })
        .await
        .expect("Pipe write task panicked")?;
        Ok(())
    }
}

#[cfg(target_family = "unix")]
mod process_unix;
#[cfg(target_family = "unix")]
pub use process_unix::*;

#[cfg(target_family = "windows")]
mod process_windows;
#[cfg(target_family = "windows")]
pub use process_windows::*;
