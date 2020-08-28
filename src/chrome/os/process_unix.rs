use super::{PipeReader, PipeWriter};
use libc::*;
use std::ptr::null_mut as NULL;

pub type Process = libc::pid_t;

pub fn new_process(path: &str, args: &[&str]) -> (Process, PipeReader, PipeWriter) {
    const READ_END: usize = 0;
    const WRITE_END: usize = 1;

    let mut pipe3: [c_int; 2] = [0; 2];
    let mut pipe4: [c_int; 2] = [0; 2];

    unsafe {
        pipe(pipe3.as_mut_ptr());
        pipe(pipe4.as_mut_ptr());
    }

    let childpid: Process;
    unsafe {
        childpid = fork();
    }

    if childpid == -1 {
        panic!("Fork failed");
    } else if childpid != 0 {
        let readp: PipeReader;
        let writep: PipeWriter;
        unsafe {
            use std::fs::File;
            use std::os::unix::io::FromRawFd;
            close(pipe3[READ_END]);
            close(pipe4[WRITE_END]);
            writep = PipeWriter::new(File::from_raw_fd(pipe3[WRITE_END]));
            readp = PipeReader::new(File::from_raw_fd(pipe4[READ_END]));
        }
        (childpid, readp, writep)
    } else {
        unsafe {
            let dev_null_path = std::ffi::CString::new("/dev/null").unwrap();
            let null_read = open(dev_null_path.as_ptr(), O_RDONLY);
            let null_write = open(dev_null_path.as_ptr(), O_WRONLY);

            dup2(null_read, 0);
            dup2(null_write, 1);
            dup2(null_write, 2);
            dup2(pipe3[READ_END], 3);
            dup2(pipe4[WRITE_END], 4);
        }

        let path = std::ffi::CString::new(path).unwrap();
        let args = args
            .iter()
            .map(|s| std::ffi::CString::new(*s).unwrap())
            .collect::<Vec<_>>();

        let mut args_ptr_list = vec![path.as_ptr() as *const c_char];
        args_ptr_list.append(&mut args.iter().map(|s| s.as_ptr() as *const c_char).collect());
        args_ptr_list.push(NULL());

        unsafe {
            execv(path.as_ptr() as *const c_char, args_ptr_list.as_ptr());
        }
        panic!("Unable to exec");
    }
}

pub fn kill_proc(p: Process) -> std::io::Result<()> {
    unsafe {
        if kill(p, SIGTERM) == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

pub fn exited(pid: Process) -> std::io::Result<bool> {
    let mut status = 0;
    unsafe {
        match waitpid(pid, &mut status, WNOHANG) {
            0 => Ok(false),
            -1 => Err(std::io::Error::last_os_error()),
            _ => Ok(true),
        }
    }
}

pub fn wait_proc(pid: Process) -> std::io::Result<()> {
    let mut status = 0;
    unsafe {
        if waitpid(pid, &mut status, 0) == -1 {
            return Err(std::io::Error::last_os_error());
        } else {
            Ok(())
        }
    }
}
