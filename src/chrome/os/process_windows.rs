use super::{PipeReader, PipeWriter};
use libc::*;
use std::ptr::null_mut as NULL;

#[repr(packed)]
struct StdioBuffer5 {
    no_fds: u32,
    flags: [u8; 5],
    handles: [HANDLE; 5],
}

const FOPEN: u8 = 0x01;
const FPIPE: u8 = 0x08;
const FDEV: u8 = 0x40;
pub type Process = HANDLE;

use os_str_bytes::OsStrBytes;
use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStrExt;

fn l(string: &str) -> Vec<u16> {
    use std::iter::once;
    OsStr::new(string).encode_wide().chain(once(0)).collect()
}

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

pub fn new_process(path: String, args: &mut [String]) -> (Process, PipeReader, PipeWriter) {
    unsafe {
        let size_sa = size_of::<SECURITY_ATTRIBUTES>() as u32;
        let mut sa = SECURITY_ATTRIBUTES {
            nLength: size_sa,
            lpSecurityDescriptor: NULL(),
            bInheritHandle: TRUE,
        };

        let null_read = CreateFileW(
            l("NUL").as_mut_ptr(),
            FILE_GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &mut sa as LPSECURITY_ATTRIBUTES,
            OPEN_EXISTING,
            0,
            NULL(),
        );
        let null_write = CreateFileW(
            l("NUL").as_mut_ptr(),
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

        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let mut cmd_str = make_command_line(&OsString::from(&path), &args).unwrap();
        cmd_str.push(0);
        CreateProcessW(
            NULL(),
            cmd_str.as_mut_ptr(),
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
        (processinfo.hProcess, readp, writep)
    }
}

pub fn exited(pid: Process) -> bool {
    use winapi::um::synchapi::WaitForSingleObject;
    unsafe { WaitForSingleObject(pid, 0) == WAIT_OBJECT_0 }
}

pub fn wait_proc(pid: Process) {
    use winapi::um::synchapi::WaitForSingleObject;
    unsafe {
        WaitForSingleObject(pid, INFINITE);
    }
}

use std::io::{self, ErrorKind};

fn make_command_line(prog: &OsStr, args: &[OsString]) -> io::Result<Vec<u16>> {
    // Encode the command and arguments in a command line string such
    // that the spawned process may recover them using CommandLineToArgvW.
    let mut cmd: Vec<u16> = Vec::new();
    // Always quote the program name so CreateProcess doesn't interpret args as
    // part of the name if the binary wasn't found first time.
    append_arg(&mut cmd, prog, true)?;
    for arg in args {
        cmd.push(' ' as u16);
        append_arg(&mut cmd, arg, false)?;
    }
    Ok(cmd)
}
fn append_arg(cmd: &mut Vec<u16>, arg: &OsStr, force_quotes: bool) -> io::Result<()> {
    // If an argument has 0 characters then we need to quote it to ensure
    // that it actually gets passed through on the command line or otherwise
    // it will be dropped entirely when parsed on the other end.
    ensure_no_nuls(arg)?;
    let arg_bytes = &arg.to_bytes();
    let quote =
        force_quotes || arg_bytes.iter().any(|c| *c == b' ' || *c == b'\t') || arg_bytes.is_empty();
    if quote {
        cmd.push('"' as u16);
    }

    let mut backslashes: usize = 0;
    for x in arg.encode_wide() {
        if x == '\\' as u16 {
            backslashes += 1;
        } else {
            if x == '"' as u16 {
                // Add n+1 backslashes to total 2n+1 before internal '"'.
                cmd.extend((0..=backslashes).map(|_| '\\' as u16));
            }
            backslashes = 0;
        }
        cmd.push(x);
    }

    if quote {
        // Add n backslashes to total 2n before ending '"'.
        cmd.extend((0..backslashes).map(|_| '\\' as u16));
        cmd.push('"' as u16);
    }
    Ok(())
}

fn ensure_no_nuls<T: AsRef<OsStr>>(str: T) -> io::Result<T> {
    if str.as_ref().encode_wide().any(|b| b == 0) {
        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "nul byte found in provided data",
        ))
    } else {
        Ok(str)
    }
}

pub fn close_process_handle(p: Process) {
    unsafe {
        CloseHandle(p);
    }
}
