use super::{PipeReader, PipeWriter};
use libc::*;
use std::ptr::null_mut as NULL;

pub type Process = libc::pid_t;

pub fn new_process(mut path: String, args: &mut [String]) -> (Process, PipeReader, PipeWriter) {
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
            close(pipe3[READ_END]);
            close(pipe4[WRITE_END]);
            writep = PipeWriter::new(pipe3[WRITE_END]);
            readp = PipeReader::new(pipe4[READ_END]);
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

        path.push('\0');
        for arg in args.iter_mut() {
            arg.push('\0')
        }

        let mut args_ptr_list = vec![path.as_ptr() as *const c_char];
        args_ptr_list.append(
            &mut args
                .iter_mut()
                .map(|s| s.as_ptr() as *const c_char)
                .collect::<Vec<*const c_char>>(),
        );
        args_ptr_list.push(NULL());

        unsafe {
            execv(path.as_ptr() as *const c_char, args_ptr_list.as_ptr());
        }
        panic!("Unable to exec");
    }
}

pub fn kill_proc(p: Process) {
    unsafe {
        kill(p, SIGTERM);
    }
}

pub fn exited(pid: Process) -> bool {
    let mut status = 0;
    unsafe { waitpid(pid, &mut status, WNOHANG) != 0 }
}

pub fn wait_proc(pid: Process) {
    let mut status = 0;
    unsafe {
        waitpid(pid, &mut status, 0);
    }
}
