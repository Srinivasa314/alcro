use libc::{
    c_char, pid_t, posix_spawn, posix_spawn_file_actions_adddup2, posix_spawn_file_actions_init,
    posix_spawn_file_actions_t,
};
use nix::{
    errno::Errno,
    fcntl::{open, OFlag},
    sys::{
        signal::{kill, Signal},
        stat::Mode,
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{pipe, Pid},
};
use std::{
    fs::File,
    mem,
    os::fd::AsRawFd,
    ptr::{null, null_mut as NULL},
};
pub type Process = Pid;

extern "C" {
    static environ: *const *mut c_char;
}

pub fn new_process(path: &str, args: &[&str]) -> Result<(Process, File, File), nix::Error> {
    let (pipe3_read, pipe3_write) = pipe()?;
    let (pipe4_read, pipe4_write) = pipe()?;

    let null_read = open("/dev/null", OFlag::O_RDONLY, Mode::empty())?;
    let null_write = open("/dev/null", OFlag::O_WRONLY, Mode::empty())?;

    let mut pid: pid_t = 0;
    unsafe {
        let mut file_actions: posix_spawn_file_actions_t = mem::zeroed();
        Errno::result(posix_spawn_file_actions_init(
            &mut file_actions as *mut posix_spawn_file_actions_t,
        ))?;
        Errno::result(posix_spawn_file_actions_adddup2(
            &mut file_actions,
            null_read.as_raw_fd(),
            0,
        ))?;
        Errno::result(posix_spawn_file_actions_adddup2(
            &mut file_actions,
            null_write.as_raw_fd(),
            1,
        ))?;
        Errno::result(posix_spawn_file_actions_adddup2(
            &mut file_actions,
            null_write.as_raw_fd(),
            2,
        ))?;
        Errno::result(posix_spawn_file_actions_adddup2(
            &mut file_actions,
            pipe3_read.as_raw_fd(),
            3,
        ))?;
        Errno::result(posix_spawn_file_actions_adddup2(
            &mut file_actions,
            pipe4_write.as_raw_fd(),
            4,
        ))?;

        let path = std::ffi::CString::new(path).unwrap();
        let args = args
            .iter()
            .map(|s| std::ffi::CString::new(*s).unwrap())
            .collect::<Vec<_>>();

        let mut args_ptr_list = vec![path.as_ptr() as *mut c_char];
        args_ptr_list.append(&mut args.iter().map(|s| s.as_ptr() as *mut c_char).collect());
        args_ptr_list.push(NULL());

        Errno::result(posix_spawn(
            &mut pid,
            path.as_ptr(),
            &file_actions,
            null(),
            args_ptr_list.as_ptr(),
            environ,
        ))?;
    }

    // The child's ends (pipe3_read, pipe4_write) and the null fds are closed
    // in the parent automatically when their OwnedFds drop here.
    Ok((
        Pid::from_raw(pid),
        File::from(pipe4_read),
        File::from(pipe3_write),
    ))
}

pub fn kill_proc(p: Process) -> Result<(), nix::Error> {
    kill(p, Signal::SIGTERM)
}

pub fn exited(pid: Process) -> std::result::Result<bool, nix::Error> {
    match waitpid(pid, Some(WaitPidFlag::WNOHANG))? {
        WaitStatus::Exited(_, _) => Ok(true),
        _ => Ok(false),
    }
}

pub fn wait_proc(pid: Process) -> Result<(), nix::Error> {
    waitpid(pid, None)?;
    Ok(())
}
