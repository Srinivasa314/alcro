pub struct PipeReader {
    pipe: std::io::BufReader<std::fs::File>,
}

impl PipeReader {
    pub fn new(f: std::fs::File) -> Self {
        Self {
            pipe: std::io::BufReader::new(f),
        }
    }

    pub fn read(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        use std::io::BufRead;
        let mut bytes_to_read = vec![];
        self.pipe.read_until(0, &mut bytes_to_read)?;
        bytes_to_read.pop();
        Ok(String::from_utf8(bytes_to_read)?)
    }
}

pub struct PipeWriter {
    pipe: std::fs::File,
}

impl PipeWriter {
    pub fn new(f: std::fs::File) -> Self {
        Self { pipe: f }
    }

    pub fn write(&mut self, message: &str) -> Result<usize, Box<dyn std::error::Error>> {
        use std::io::Write;
        Ok(self
            .pipe
            .write(std::ffi::CString::new(message)?.as_bytes())?)
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
