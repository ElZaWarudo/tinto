use std::{
    io,
    mem::size_of,
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Child, Command},
    ptr,
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_ACCESS_DENIED, FILETIME, HANDLE, WAIT_OBJECT_0},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicProcessIdList,
        JobObjectExtendedLimitInformation, OpenJobObjectW, QueryInformationJobObject,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
    System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, QueryFullProcessImageNameW,
        TerminateProcess, WaitForSingleObject, CREATE_BREAKAWAY_FROM_JOB, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    },
};

/// Access to a job object opened by name: list its processes, end them.
const JOB_OBJECT_QUERY: u32 = 0x0004;
const JOB_OBJECT_TERMINATE: u32 = 0x0008;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// The executable of process `pid` ("node.exe"), when it can be read.
fn image_name(pid: u32) -> Option<String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    let read = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut length)
    };
    unsafe {
        CloseHandle(handle);
    }
    (read != 0).then(|| {
        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string()
    })
}

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub(crate) fn hide_console(command: &mut Command) -> &mut Command {
    command.creation_flags(CREATE_NO_WINDOW)
}

/// Starts `command` without a console and, when Tinto itself runs inside a
/// job that would take it down on exit, outside that job.
pub(crate) fn spawn_outliving(command: &mut Command) -> io::Result<Child> {
    match command
        .creation_flags(CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
    {
        // Tinto's job does not allow it: the process stays in it.
        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
            hide_console(command).spawn()
        }
        spawned => spawned,
    }
}

/// When a process started, in 100 ns ticks: with its pid it tells that
/// process from a later one given the same pid.
pub(crate) fn creation_time(process: &impl AsRawHandle) -> Option<u64> {
    created(process.as_raw_handle() as HANDLE)
}

fn created(handle: HANDLE) -> Option<u64> {
    let mut times = [FILETIME::default(); 4];
    let [creation, exit, kernel, user] = &mut times;
    if unsafe { GetProcessTimes(handle, creation, exit, kernel, user) } == 0 {
        return None;
    }
    Some((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

/// A process opened by pid: a job Tinto started before it restarted.
pub(crate) struct ProcessHandle {
    handle: HANDLE,
}

// SAFETY: a process handle can be used and closed from any thread.
unsafe impl Send for ProcessHandle {}

impl ProcessHandle {
    pub(crate) fn open(pid: u32) -> Option<Self> {
        let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE;
        let handle = unsafe { OpenProcess(access, 0, pid) };
        (!handle.is_null()).then_some(Self { handle })
    }

    pub(crate) fn creation_time(&self) -> Option<u64> {
        created(self.handle)
    }

    /// Its exit code once it ended; `None` while it runs.
    pub(crate) fn exit_code(&self) -> io::Result<Option<u32>> {
        if unsafe { WaitForSingleObject(self.handle, 0) } != WAIT_OBJECT_0 {
            return Ok(None);
        }
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self.handle, &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(code))
    }

    pub(crate) fn terminate(&self) {
        unsafe {
            TerminateProcess(self.handle, 1);
        }
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

pub(crate) struct KillOnCloseJob {
    handle: HANDLE,
}

impl KillOnCloseJob {
    pub(crate) fn attach(child: &Child) -> io::Result<Self> {
        Self::create(child, true, None)
    }

    /// A job that only groups `child` and what it starts, to stop them all
    /// at once: closing it, as Tinto does when it exits, leaves them running.
    /// Named so that a Tinto started later finds it again with
    /// [`Self::open_named`]: the job lives on while any of its processes
    /// does, even once no Tinto has it open.
    pub(crate) fn attach_named_outliving(child: &Child, name: &str) -> io::Result<Self> {
        Self::create(child, false, Some(name))
    }

    /// The job named `name`, while any process is still in it.
    pub(crate) fn open_named(name: &str) -> Option<Self> {
        let name = wide(name);
        let handle =
            unsafe { OpenJobObjectW(JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE, 0, name.as_ptr()) };
        (!handle.is_null()).then_some(Self { handle })
    }

    /// The processes still in the job, as "node.exe (pid 1234)".
    pub(crate) fn processes(&self) -> Vec<String> {
        #[repr(C)]
        struct IdList {
            assigned: u32,
            listed: u32,
            ids: [usize; 512],
        }
        let mut list = IdList {
            assigned: 0,
            listed: 0,
            ids: [0; 512],
        };
        let read = unsafe {
            QueryInformationJobObject(
                self.handle,
                JobObjectBasicProcessIdList,
                (&raw mut list).cast(),
                size_of::<IdList>() as u32,
                ptr::null_mut(),
            )
        };
        if read == 0 {
            return Vec::new();
        }
        list.ids[..(list.listed as usize).min(list.ids.len())]
            .iter()
            .map(|&id| {
                let pid = id as u32;
                let name = image_name(pid).unwrap_or_else(|| "proceso".to_string());
                format!("{name} (pid {pid})")
            })
            .collect()
    }

    fn create(child: &Child, kill_on_close: bool, name: Option<&str>) -> io::Result<Self> {
        let name = name.map(wide);
        let name_ptr = name.as_ref().map_or(ptr::null(), |name| name.as_ptr());
        let handle = unsafe { CreateJobObjectW(ptr::null(), name_ptr) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        if kill_on_close {
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        }
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            unsafe {
                CloseHandle(handle);
            }
            return Err(io::Error::last_os_error());
        }
        let process = child.as_raw_handle() as HANDLE;
        if unsafe { AssignProcessToJobObject(handle, process) } == 0 {
            unsafe {
                CloseHandle(handle);
            }
            return Err(io::Error::last_os_error());
        }
        Ok(Self { handle })
    }

    /// Kills every process in the job (the child and its descendants)
    /// while keeping the job itself open.
    pub(crate) fn kill_all(&self) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.handle, 1) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(crate) fn terminate(mut self) -> io::Result<()> {
        let handle = self.handle;
        self.handle = ptr::null_mut();
        if unsafe { CloseHandle(handle) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

/// Bound a diagnostic child and its pipe reads; never change distro lifecycle.
pub(crate) fn output_with_timeout(
    command: &mut Command,
    timeout: std::time::Duration,
) -> io::Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Instant;
    let deadline = Instant::now() + timeout;
    let mut child = hide_console(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    for (is_stdout, stream) in [
        (true, Box::new(stdout) as Box<dyn Read + Send>),
        (false, Box::new(stderr) as Box<dyn Read + Send>),
    ] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stream
                .take(64 * 1024 + 1)
                .read_to_end(&mut bytes)
                .and_then(|_| {
                    if bytes.len() > 64 * 1024 {
                        Err(io::Error::other("WSL diagnostic output exceeded limit"))
                    } else {
                        Ok(bytes)
                    }
                });
            let _ = tx.send((is_stdout, result));
        });
    }
    drop(tx);
    let result = (|| {
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "timeout esperando comando WSL",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        for _ in 0..2 {
            let (is_stdout, bytes) = rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::TimedOut, "timeout leyendo salida WSL")
                })?;
            if is_stdout {
                stdout = bytes?;
            } else {
                stderr = bytes?;
            }
        }
        Ok(std::process::Output {
            status,
            stdout,
            stderr,
        })
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        process::Stdio,
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn kill_on_close_job_reaps_a_spawned_descendant() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("descendant.pid");
        let pid_path = pid_file.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$child = Start-Process powershell.exe -ArgumentList '-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 30' -PassThru; Set-Content -LiteralPath '{pid_path}' -Value $child.Id; Wait-Process -Id $child.Id"
        );
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut parent = hide_console(&mut command).spawn().unwrap();
        let job = KillOnCloseJob::attach(&parent).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let descendant_pid = loop {
            if let Ok(pid) = fs::read_to_string(&pid_file) {
                break pid.trim().parse::<u32>().unwrap();
            }
            assert!(
                Instant::now() < deadline,
                "descendant pid was not published"
            );
            thread::sleep(Duration::from_millis(20));
        };

        job.terminate().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let parent_done = parent.try_wait().unwrap().is_some();
            if parent_done && !process_is_running(descendant_pid) {
                break;
            }
            if Instant::now() >= deadline {
                let _ = hide_console(Command::new("taskkill").args([
                    "/F",
                    "/T",
                    "/PID",
                    &descendant_pid.to_string(),
                ]))
                .status();
                panic!("job close left a descendant running");
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn bounded_output_preserves_exit_and_bounds_stalled_child() {
        let output = output_with_timeout(
            Command::new("cmd.exe").args(["/C", "echo ready & exit /b 7"]),
            std::time::Duration::from_secs(3),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert!(String::from_utf8_lossy(&output.stdout).contains("ready"));
        let started = std::time::Instant::now();
        let error = output_with_timeout(
            Command::new("powershell.exe").args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 10",
            ]),
            std::time::Duration::from_millis(100),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    fn process_is_running(pid: u32) -> bool {
        let script = format!(
            "if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
        );
        let mut command = Command::new("powershell.exe");
        hide_console(command.args(["-NoProfile", "-NonInteractive", "-Command", &script]))
            .status()
            .is_ok_and(|status| status.success())
    }
}
