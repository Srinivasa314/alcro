pub struct PipeReader {
    pipe: std::io::BufReader<std::fs::File>,
}

#[derive(Debug, thiserror::Error)]
pub enum PipeReadError {
    #[error("Invalid UTF-8")]
    InvalidUtf8Error,
    #[error("Cannot read data from pipe")]
    IOError(#[from] std::io::Error),
}

impl PipeReader {
    pub fn new(f: std::fs::File) -> Self {
        Self {
            pipe: std::io::BufReader::new(f),
        }
    }

    pub fn read(&mut self) -> Result<String, PipeReadError> {
        use std::io::BufRead;
        let mut bytes_to_read = vec![];
        self.pipe.read_until(0, &mut bytes_to_read)?;
        bytes_to_read.pop();
        String::from_utf8(bytes_to_read).map_err(|_| PipeReadError::InvalidUtf8Error)
    }
}

pub struct PipeWriter {
    pipe: std::fs::File,
}

#[derive(Debug, thiserror::Error)]
pub enum PipeWriteError {
    #[error("Null character present in string")]
    NullCharacterPresent,
    #[error("Cannot write data to pipe: {0}")]
    IOError(#[from] std::io::Error),
}

impl PipeWriter {
    pub fn new(f: std::fs::File) -> Self {
        Self { pipe: f }
    }

    pub fn write(&mut self, message: String) -> Result<usize, PipeWriteError> {
        use std::io::Write;
        match std::ffi::CString::new(message) {
            Ok(cstr) => Ok(self.pipe.write(cstr.as_bytes_with_nul())?),
            Err(_) => return Err(PipeWriteError::NullCharacterPresent),
        }
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
