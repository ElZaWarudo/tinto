use std::{
    io,
    mem::size_of,
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Child, Command},
    ptr,
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
};

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub(crate) fn hide_console(command: &mut Command) -> &mut Command {
    command.creation_flags(CREATE_NO_WINDOW)
}

pub(crate) struct KillOnCloseJob {
    handle: HANDLE,
}

impl KillOnCloseJob {
    pub(crate) fn attach(child: &Child) -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
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
