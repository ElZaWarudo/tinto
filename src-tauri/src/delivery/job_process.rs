//! A job's process, which outlives Tinto: it starts detached, writes its
//! output straight to the job's files, and a restarted Tinto adopts it by
//! its pid once its start time says it is still the same process.

use std::io;
use std::process::{Child, Command};

/// How a job's process ended. `success` is `None` when Tinto could not
/// learn it: a process adopted after a restart on Unix, which is not
/// Tinto's child any more, or one that ended while Tinto was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Exited {
    pub code: Option<i32>,
    pub success: Option<bool>,
}

pub(super) struct JobProcess {
    pid: u32,
    kind: Kind,
}

enum Kind {
    /// Started in this session of Tinto.
    Child {
        child: Child,
        #[cfg(target_os = "windows")]
        tree: Option<crate::windows_process::KillOnCloseJob>,
    },
    /// Started before Tinto restarted.
    Adopted {
        #[cfg(target_os = "windows")]
        handle: crate::windows_process::ProcessHandle,
        #[cfg(not(target_os = "windows"))]
        started: String,
    },
    /// It ended while Tinto was closed.
    Ended,
}

impl JobProcess {
    /// Starts `command` so that closing Tinto leaves it running; its own
    /// group, so stopping the job reaches what it starts.
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(target_os = "windows")]
        {
            let child = crate::windows_process::spawn_outliving(command)?;
            let tree = crate::windows_process::KillOnCloseJob::attach_outliving(&child).ok();
            Ok(Self {
                pid: child.id(),
                kind: Kind::Child { child, tree },
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::process::CommandExt;
            let child = command.process_group(0).spawn()?;
            Ok(Self {
                pid: child.id(),
                kind: Kind::Child { child },
            })
        }
    }

    /// The process `pid` if it still is the one that started at `started`
    /// (see [`JobProcess::started`]).
    pub fn adopt(pid: u32, started: &str) -> Option<Self> {
        #[cfg(target_os = "windows")]
        {
            let handle = crate::windows_process::ProcessHandle::open(pid)?;
            (handle.creation_time()?.to_string() == started).then_some(Self {
                pid,
                kind: Kind::Adopted { handle },
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            (start_time(pid)? == started).then(|| Self {
                pid,
                kind: Kind::Adopted {
                    started: started.to_string(),
                },
            })
        }
    }

    /// A job whose process ended while Tinto was closed.
    pub fn ended(pid: u32) -> Self {
        Self {
            pid,
            kind: Kind::Ended,
        }
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// When it started, as [`JobProcess::adopt`] compares it.
    pub fn started(&self) -> Option<String> {
        match &self.kind {
            #[cfg(target_os = "windows")]
            Kind::Child { child, .. } => {
                crate::windows_process::creation_time(child).map(|at| at.to_string())
            }
            #[cfg(not(target_os = "windows"))]
            Kind::Child { .. } => start_time(self.pid),
            _ => None,
        }
    }

    pub fn try_wait(&mut self) -> io::Result<Option<Exited>> {
        match &mut self.kind {
            Kind::Child { child, .. } => Ok(child.try_wait()?.map(|status| Exited {
                code: status.code(),
                success: Some(status.success()),
            })),
            #[cfg(target_os = "windows")]
            Kind::Adopted { handle } => Ok(handle.exit_code()?.map(|code| Exited {
                code: Some(code as i32),
                success: Some(code == 0),
            })),
            #[cfg(not(target_os = "windows"))]
            Kind::Adopted { started } => Ok((start_time(self.pid).as_ref() != Some(started))
                .then_some(Exited {
                    code: None,
                    success: None,
                })),
            Kind::Ended => Ok(Some(Exited {
                code: None,
                success: None,
            })),
        }
    }

    /// Kills the process and everything it started.
    pub fn kill_tree(&mut self) {
        match &mut self.kind {
            Kind::Child {
                child,
                #[cfg(target_os = "windows")]
                tree,
            } => {
                // Killing the child alone orphans what it started, among
                // them the commands Codex runs in process groups of their own.
                #[cfg(unix)]
                let _ = crate::agent_console::pty::kill_process_tree(self.pid);
                let _ = child.kill();
                #[cfg(target_os = "windows")]
                if let Some(tree) = tree {
                    let _ = tree.kill_all();
                }
            }
            // Tinto's job object for it closed with the Tinto that started it:
            // the tree is found from its pid.
            #[cfg(target_os = "windows")]
            Kind::Adopted { handle } => {
                let _ = crate::agent_console::pty::kill_process_tree(self.pid);
                handle.terminate();
            }
            #[cfg(not(target_os = "windows"))]
            Kind::Adopted { .. } => {
                let _ = crate::agent_console::pty::kill_process_tree(self.pid);
            }
            Kind::Ended => {}
        }
    }
}

/// Whether `pid` is still the process that started at `started`.
pub(super) fn alive(pid: u32, started: &str) -> bool {
    let Some(mut process) = JobProcess::adopt(pid, started) else {
        return false;
    };
    matches!(process.try_wait(), Ok(None))
}

/// When `pid` started, `None` once it is gone (a zombie included).
#[cfg(not(target_os = "windows"))]
fn start_time(pid: u32) -> Option<String> {
    // Linux: the 22nd field of its stat, in clock ticks since boot; the
    // fields start after the command's closing parenthesis.
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
        if matches!(fields.first(), Some(&"Z") | Some(&"X")) {
            return None;
        }
        return fields.get(19).map(|ticks| ticks.to_string());
    }
    let output = Command::new("ps")
        .args(["-o", "stat=,lstart=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let (stat, started) = text.split_once(char::is_whitespace)?;
    (!stat.starts_with('Z')).then(|| started.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sleeper() -> Command {
        #[cfg(target_os = "windows")]
        {
            let mut command = Command::new("ping");
            command.args(["-n", "30", "127.0.0.1"]);
            command
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        }
    }

    #[test]
    fn a_process_is_adopted_by_pid_and_start_time_and_stopped_from_there() {
        let mut command = sleeper();
        command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut started = JobProcess::spawn(&mut command).unwrap();
        let pid = started.pid();
        let at = started.started().expect("its start time");
        assert!(alive(pid, &at));
        assert!(!alive(pid, "1"), "another start time is another process");

        let mut adopted = JobProcess::adopt(pid, &at).expect("adopted");
        assert_eq!(adopted.try_wait().unwrap(), None);
        adopted.kill_tree();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        // The first Tinto reaps its child, as the system would once it closed.
        while started.try_wait().unwrap().is_none() || alive(pid, &at) {
            assert!(std::time::Instant::now() < deadline, "it kept running");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}
