use libc::*;
use std::ptr::null_mut as NULL;

const BUFSIZE: usize = 16;

pub struct PipeWriter {
    fd: c_int,
}

#[cfg(target_family = "unix")]
type write_size = size_t;

#[cfg(target_family = "windows")]
type write_size = c_uint;

impl PipeWriter {
    pub fn write(&mut self, mut msg: String) {
        msg.push('\0');
        unsafe {
            write(
                self.fd,
                msg.as_ptr() as *const c_void,
                msg.len() as write_size,
            );
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

        if !self.extra_buffer.is_empty() {
            s.append(&mut self.extra_buffer);
            self.extra_buffer.clear();
        }

        loop {
            let nbytes;
            unsafe {
                nbytes = read(self.fd, resbuf.as_mut_ptr() as *mut c_void, BUFSIZE);
            }
            if nbytes == 0 {
                break;
            }
            let mut null_found = false;
            let mut len = nbytes;

            for i in 0..nbytes {
                if resbuf[i as usize] == 0 {
                    len = i;
                    null_found = true;
                    break;
                }
            }

            s.extend_from_slice(&resbuf[0..(len as usize)]);
            if null_found == true && len + 1 < nbytes {
                self.extra_buffer = resbuf[((len + 1) as usize)..(nbytes as usize)].to_vec();
            }
            if null_found {
                break;
            }
        }
        unsafe { String::from_utf8_unchecked(s) }
    }
}

#[cfg(target_family = "unix")]
type pid_t = libc::pid_t;

#[cfg(target_family = "unix")]
pub fn new_process(mut path: String, args: &mut [String]) -> (pid_t, PipeReader, PipeWriter) {
    const READ_END: usize = 0;
    const WRITE_END: usize = 1;

    let mut pipe3: [c_int; 2] = [0; 2];
    let mut pipe4: [c_int; 2] = [0; 2];

    unsafe {
        pipe(pipe3.as_mut_ptr());
        pipe(pipe4.as_mut_ptr());
    }

    let childpid: pid_t;
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
        return (childpid, readp, writep);
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

        let mut args_ptr_list = vec![path.as_ptr() as *const i8];
        args_ptr_list.append(
            &mut args
                .into_iter()
                .map(|s| s.as_ptr() as *const i8)
                .collect::<Vec<*const i8>>(),
        );
        args_ptr_list.push(NULL());

        unsafe {
            execv(path.as_ptr() as *const i8, args_ptr_list.as_ptr());
        }
        panic!("Unable to exec");
    }
}

#[cfg(target_family = "unix")]
pub fn kill_proc(p: pid_t) {
    unsafe {
        kill(p, SIGINT);
    }
}

#[cfg(target_family = "windows")]
#[repr(packed)]
struct StdioBuffer5 {
    no_fds: u32,
    flags: [u8; 5],
    handles: [HANDLE; 5],
}

#[cfg(target_family = "windows")]
const FOPEN: u8 = 0x01;
#[cfg(target_family = "windows")]
const FPIPE: u8 = 0x08;
#[cfg(target_family = "windows")]
const FDEV: u8 = 0x40;
#[cfg(target_family = "windows")]
type pid_t = HANDLE;

#[cfg(target_family = "windows")]
fn L(string: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::iter::once;
    OsStr::new(string).encode_wide().chain(once(0)).collect()
}

#[cfg(target_family = "windows")]
pub fn new_process(path: String, args: &mut [String]) -> (pid_t, PipeReader, PipeWriter) {
    use std::mem::*;
    use winapi::shared::minwindef::{TRUE, *};
    use winapi::shared::{basetsd::*, ntdef::HANDLE};
    use winapi::um::fileapi::*;
    use winapi::um::handleapi::*;
    use winapi::um::minwinbase::*;
    use winapi::um::namedpipeapi::*;
    use winapi::um::processthreadsapi::*;
    use winapi::um::winbase::*;
    use winapi::um::winnt::*;

    unsafe {
        let size_sa = size_of::<SECURITY_ATTRIBUTES>() as u32;
        let mut sa = SECURITY_ATTRIBUTES {
            nLength: size_sa,
            lpSecurityDescriptor: NULL(),
            bInheritHandle: TRUE,
        };

        let null_read = CreateFileW(
            L("null").as_mut_ptr(),
            FILE_GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &mut sa as LPSECURITY_ATTRIBUTES,
            OPEN_EXISTING,
            0,
            NULL(),
        );
        let null_write = CreateFileW(
            L("null").as_mut_ptr(),
            FILE_GENERIC_WRITE | FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &mut sa as LPSECURITY_ATTRIBUTES,
            OPEN_EXISTING,
            0,
            NULL(),
        );

        let mut readpipe3: HANDLE = NULL();
        let mut writepipe3: HANDLE = NULL();
        CreatePipe(
            &mut readpipe3 as LPHANDLE,
            &mut writepipe3 as LPHANDLE,
            &mut sa as LPSECURITY_ATTRIBUTES,
            0,
        );

        let mut readpipe4: HANDLE = NULL();
        let mut writepipe4: HANDLE = NULL();
        CreatePipe(
            &mut readpipe4 as LPHANDLE,
            &mut writepipe4 as LPHANDLE,
            &mut sa as LPSECURITY_ATTRIBUTES,
            0,
        );

        let mut startupinfo: STARTUPINFOEXW = zeroed();
        let mut processinfo: PROCESS_INFORMATION = zeroed();
        let mut attrsize: SIZE_T = Default::default();
        InitializeProcThreadAttributeList(NULL(), 1, 0, &mut attrsize as PSIZE_T);
        let mut attr_list: Vec<u8> = vec![0; attrsize];

        let mut handle_list = [null_read, null_write, null_write, readpipe3, writepipe4];
        const PROC_THREAD_ATTRIBUTE_HANDLE_LIST: usize = 0x20002;
        UpdateProcThreadAttribute(
            attr_list.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            handle_list.as_mut_ptr() as PVOID,
            size_of::<HANDLE>() * 5,
            NULL(),
            NULL(),
        );

        let mut stdio_buffer = StdioBuffer5 {
            no_fds: 5,
            flags: [
                FOPEN | FDEV,
                FOPEN | FDEV,
                FOPEN | FDEV,
                FOPEN | FPIPE,
                FOPEN | FPIPE,
            ],
            handles: [null_read, null_write, null_write, readpipe3, writepipe4],
        };

        startupinfo.lpAttributeList = attr_list.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
        startupinfo.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startupinfo.StartupInfo.cbReserved2 = size_of::<StdioBuffer5>() as u16;
        startupinfo.StartupInfo.lpReserved2 = &mut stdio_buffer as *mut StdioBuffer5 as LPBYTE;

        let mut child_cmd: String = path.clone();
        for arg in args {
            child_cmd.push_str(&format!(" {}", arg));
        }
        CreateProcessW(
            L(&path).as_mut_ptr(),
            L(&child_cmd).as_mut_ptr(),
            NULL(),
            NULL(),
            TRUE,
            EXTENDED_STARTUPINFO_PRESENT,
            NULL(),
            NULL(),
            &mut startupinfo as LPSTARTUPINFOEXW as LPSTARTUPINFOW,
            &mut processinfo as LPPROCESS_INFORMATION,
        );

        CloseHandle(processinfo.hThread);
        CloseHandle(readpipe3);
        CloseHandle(writepipe4);

        let writep = PipeWriter::new(open_osfhandle(writepipe3 as isize, O_WRONLY));
        let readp = PipeReader::new(open_osfhandle(readpipe4 as isize, O_RDONLY));
        return (processinfo.hProcess, readp, writep);
    }
}

#[cfg(target_family = "windows")]
pub fn kill_proc(pid: pid_t) {
    TerminateProcess(hproc, 0);
    CloseHandle(hproc);
}

#[cfg(target_family = "windows")]
pub fn exited(pid: pid_t) -> bool {
    return WaitForSingleObject(hproc, 0) == WAIT_OBJECT_0;
}

#[cfg(target_family = "unix")]
pub fn exited(pid: pid_t) -> bool {
    let mut status = 0;
    unsafe {
        return waitpid(pid, &mut status, WNOHANG) != 0;
    }
}
