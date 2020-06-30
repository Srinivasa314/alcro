use libc::*;
const BUFSIZE: usize = 1024;

pub struct PipeWriter {
    fd: c_int,
}

#[cfg(target_family = "unix")]
type Size = size_t;

#[cfg(target_family = "windows")]
type Size = c_uint;

impl PipeWriter {
    pub fn write(&mut self, mut msg: String) {
        msg.push('\0');
        unsafe {
            write(self.fd, msg.as_ptr() as *const c_void, msg.len() as Size);
        }
    }

    unsafe fn new(fd: c_int) -> Self {
        Self { fd }
    }
}

pub struct PipeReader {
    fd: c_int,
    extra_buffer: Vec<u8>,
}

impl PipeReader {
    unsafe fn new(fd: c_int) -> PipeReader {
        PipeReader {
            fd,
            extra_buffer: vec![],
        }
    }

    pub fn read(&mut self) -> String {
        let mut resbuf: [u8; BUFSIZE] = [0; BUFSIZE];
        let mut s: Vec<u8> = vec![];
        let mut nbytes = self.extra_buffer.len();

        if !self.extra_buffer.is_empty() {
            resbuf[..self.extra_buffer.len()].clone_from_slice(&self.extra_buffer);
        }

        loop {
            if self.extra_buffer.is_empty() {
                unsafe {
                    nbytes =
                        read(self.fd, resbuf.as_mut_ptr() as *mut c_void, BUFSIZE as Size) as usize;
                }
            } else {
                self.extra_buffer.clear();
            }
            if nbytes == 0 {
                break;
            }
            let mut null_found = false;
            let mut len = nbytes;
            for (i, byte) in resbuf.iter().enumerate().take(nbytes) {
                if *byte == 0 {
                    len = i;
                    null_found = true;
                    break;
                }
            }

            s.extend_from_slice(&resbuf[0..len]);
            if null_found && len + 1 < nbytes {
                self.extra_buffer = resbuf[(len + 1)..nbytes].to_vec();
            }
            if null_found {
                break;
            }
        }
        unsafe { String::from_utf8_unchecked(s) }
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
