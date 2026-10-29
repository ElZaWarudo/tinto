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
        /// Its tree, found again by name.
        #[cfg(target_os = "windows")]
        tree: Option<crate::windows_process::KillOnCloseJob>,
        #[cfg(not(target_os = "windows"))]
        started: String,
    },
    /// It ended while Tinto was closed; what it started may not have.
    Ended {
        #[cfg(target_os = "windows")]
        tree: Option<crate::windows_process::KillOnCloseJob>,
    },
}

/// The name of a job's process tree, by which a restarted Tinto finds it.
#[cfg(target_os = "windows")]
fn tree_name(job_id: &str) -> String {
    format!("Local\\tinto-delivery-{job_id}")
}

impl JobProcess {
    /// Starts `command` so that closing Tinto leaves it running; its own
    /// group, so stopping the job reaches what it starts.
    pub fn spawn(command: &mut Command, job_id: &str) -> io::Result<Self> {
        #[cfg(target_os = "windows")]
        {
            let child = crate::windows_process::spawn_outliving(command)?;
            let tree = crate::windows_process::KillOnCloseJob::attach_named_outliving(
                &child,
                &tree_name(job_id),
            )
            .ok();
            Ok(Self {
                pid: child.id(),
                kind: Kind::Child { child, tree },
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::process::CommandExt;
            let _ = job_id;
            without_appimage_env(command);
            let child = command.process_group(0).spawn()?;
            Ok(Self {
                pid: child.id(),
                kind: Kind::Child { child },
            })
        }
    }

    /// The process `pid` if it still is the one that started at `started`
    /// (see [`JobProcess::started`]).
    pub fn adopt(pid: u32, started: &str, job_id: &str) -> Option<Self> {
        #[cfg(target_os = "windows")]
        {
            let handle = crate::windows_process::ProcessHandle::open(pid)?;
            (handle.creation_time()?.to_string() == started).then(|| Self {
                pid,
                kind: Kind::Adopted {
                    handle,
                    tree: crate::windows_process::KillOnCloseJob::open_named(&tree_name(job_id)),
                },
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = job_id;
            (start_time(pid)? == started).then(|| Self {
                pid,
                kind: Kind::Adopted {
                    started: started.to_string(),
                },
            })
        }
    }

    /// A job whose process ended while Tinto was closed.
    pub fn ended(pid: u32, job_id: &str) -> Self {
        #[cfg(not(target_os = "windows"))]
        let _ = job_id;
        Self {
            pid,
            kind: Kind::Ended {
                #[cfg(target_os = "windows")]
                tree: crate::windows_process::KillOnCloseJob::open_named(&tree_name(job_id)),
            },
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
            Kind::Adopted { handle, .. } => Ok(handle.exit_code()?.map(|code| Exited {
                code: Some(code as i32),
                success: Some(code == 0),
            })),
            #[cfg(not(target_os = "windows"))]
            Kind::Adopted { started } => Ok((start_time(self.pid).as_ref() != Some(started))
                .then_some(Exited {
                    code: None,
                    success: None,
                })),
            Kind::Ended { .. } => Ok(Some(Exited {
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
            Kind::Adopted { handle, tree } => {
                let _ = crate::agent_console::pty::kill_process_tree(self.pid);
                handle.terminate();
                if let Some(tree) = tree {
                    let _ = tree.kill_all();
                }
            }
            #[cfg(not(target_os = "windows"))]
            Kind::Adopted { .. } => {
                let _ = crate::agent_console::pty::kill_process_tree(self.pid);
            }
            Kind::Ended { .. } => {}
        }
    }

    /// Once the job ended, for whatever reason: ends what it left running
    /// (a server it started in the background, say) and says what that was.
    pub fn sweep(&mut self) -> Vec<String> {
        #[cfg(target_os = "windows")]
        {
            let tree = match &self.kind {
                Kind::Child { tree, .. } | Kind::Adopted { tree, .. } | Kind::Ended { tree } => {
                    tree.as_ref()
                }
            };
            let Some(tree) = tree else {
                return Vec::new();
            };
            let left = tree.processes();
            if !left.is_empty() {
                let _ = tree.kill_all();
            }
            left
        }
        #[cfg(not(target_os = "windows"))]
        {
            // Its own process group: what it started in the background is in
            // it, unless it left the group on purpose. Every process, filtered
            // by group here: `ps -g` selects a session on Linux, not a group.
            let group = self.pid.to_string();
            let listed = Command::new("ps")
                .args(["-A", "-o", "pid=,pgid=,args="])
                .output()
                .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
                .unwrap_or_default();
            let left: Vec<String> = listed
                .lines()
                .filter_map(|line| {
                    let mut fields = line.split_whitespace();
                    let pid = fields.next()?;
                    let pgid = fields.next()?;
                    (pgid == group && pid != group)
                        .then(|| format!("{} (pid {pid})", fields.collect::<Vec<_>>().join(" ")))
                })
                .collect();
            if !left.is_empty() {
                let _ = Command::new("kill")
                    .args(["-KILL", "--", &format!("-{group}")])
                    .status();
            }
            left
        }
    }
}

/// What the AppImage's runtime sets for Tinto itself: a job's `python3`
/// fails on its PYTHONHOME, and so on.
#[cfg(not(target_os = "windows"))]
const APPIMAGE_VARS: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "ARGV0",
    "OWD",
    "PYTHONHOME",
    "PYTHONPATH",
    "PERLLIB",
    "GSETTINGS_SCHEMA_DIR",
    "GDK_PIXBUF_MODULEDIR",
    "GDK_PIXBUF_MODULE_FILE",
    "GIO_MODULE_DIR",
    "QT_PLUGIN_PATH",
];

/// Under an AppImage, gives `command` the environment Tinto was started
/// with: without the runtime's variables and its mount in the search
/// paths, and with what the runtime saved as `*_ORIG` back in place.
#[cfg(not(target_os = "windows"))]
fn without_appimage_env(command: &mut Command) {
    use std::collections::HashMap;
    use std::ffi::{OsStr, OsString};
    let mut env: HashMap<OsString, OsString> = std::env::vars_os().collect();
    for (name, value) in command.get_envs() {
        match value {
            Some(value) => env.insert(name.to_os_string(), value.to_os_string()),
            None => env.remove(name),
        };
    }
    if !env.contains_key(OsStr::new("APPIMAGE")) && !env.contains_key(OsStr::new("APPDIR")) {
        return;
    }
    for name in env.keys() {
        let text = name.to_string_lossy();
        if APPIMAGE_VARS.contains(&text.as_ref()) || text.starts_with("GST_PLUGIN_") {
            command.env_remove(name);
        }
    }
    for name in ["PATH", "LD_LIBRARY_PATH", "XDG_DATA_DIRS"] {
        let Some(value) = env.get(OsStr::new(name)) else {
            continue;
        };
        let kept: Vec<_> = std::env::split_paths(value)
            .filter(|dir| !dir.to_string_lossy().starts_with("/tmp/.mount_"))
            .collect();
        match std::env::join_paths(kept) {
            Ok(kept) if !kept.is_empty() => command.env(name, kept),
            _ => command.env_remove(name),
        };
    }
    for (name, value) in &env {
        let Some(original) = name.to_str().and_then(|name| name.strip_suffix("_ORIG")) else {
            continue;
        };
        command.env_remove(name);
        match value.is_empty() {
            true => command.env_remove(original),
            false => command.env(original, value),
        };
    }
}

/// Whether `pid` is still the process that started at `started`.
pub(super) fn alive(pid: u32, started: &str) -> bool {
    let Some(mut process) = JobProcess::adopt(pid, started, "") else {
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
        let mut started = JobProcess::spawn(&mut command, "j-adopt").unwrap();
        let pid = started.pid();
        let at = started.started().expect("its start time");
        assert!(alive(pid, &at));
        assert!(!alive(pid, "1"), "another start time is another process");

        let mut adopted = JobProcess::adopt(pid, &at, "j-adopt").expect("adopted");
        assert_eq!(adopted.try_wait().unwrap(), None);
        adopted.kill_tree();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        // The first Tinto reaps its child, as the system would once it closed.
        while started.try_wait().unwrap().is_none() || alive(pid, &at) {
            assert!(std::time::Instant::now() < deadline, "it kept running");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn a_job_runs_without_the_appimage_runtimes_environment() {
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("env.txt");
        let path = std::env::var("PATH").unwrap_or_default();
        let mut command = Command::new("env");
        command
            .env("APPIMAGE", "/home/me/Tinto.AppImage")
            .env("APPDIR", "/tmp/.mount_Tinto_gdHjFm")
            .env("PYTHONHOME", "/tmp/.mount_Tinto_gdHjFm/usr/")
            .env(
                "GST_PLUGIN_SYSTEM_PATH",
                "/tmp/.mount_Tinto_gdHjFm/usr/lib/gst",
            )
            .env("PATH", format!("/tmp/.mount_Tinto_gdHjFm/usr/bin:{path}"))
            .env("LD_LIBRARY_PATH", "/tmp/.mount_Tinto_gdHjFm/usr/lib")
            .env("PYTHONPATH_ORIG", "/home/me/lib")
            .env("TEST_DB_NAME", "tinto_test")
            .stdout(std::fs::File::create(&seen).unwrap());
        let mut job = JobProcess::spawn(&mut command, "j-appimage").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while job.try_wait().unwrap().is_none() {
            assert!(std::time::Instant::now() < deadline, "env did not end");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let seen = std::fs::read_to_string(&seen).unwrap();
        let value = |name: &str| {
            seen.lines()
                .find_map(|line| line.strip_prefix(&format!("{name}=")))
                .map(str::to_string)
        };
        for gone in [
            "APPIMAGE",
            "APPDIR",
            "PYTHONHOME",
            "GST_PLUGIN_SYSTEM_PATH",
            "LD_LIBRARY_PATH",
            "PYTHONPATH_ORIG",
        ] {
            assert_eq!(value(gone), None, "{gone} reached the job");
        }
        assert_eq!(value("PATH").as_deref(), Some(path.as_str()));
        assert_eq!(value("PYTHONPATH").as_deref(), Some("/home/me/lib"));
        assert_eq!(value("TEST_DB_NAME").as_deref(), Some("tinto_test"));
    }

    /// A job that starts a server in the background and ends.
    fn detaching() -> Command {
        #[cfg(target_os = "windows")]
        {
            let mut command = Command::new("cmd");
            command.args(["/c", "start", "", "/b", "ping", "-n", "60", "127.0.0.1"]);
            command
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 60 & exit 0"]);
            command
        }
    }

    #[test]
    fn what_a_job_left_running_is_named_and_stopped_even_by_a_later_tinto() {
        let name = format!("j-sweep-{}", std::process::id());
        let mut command = detaching();
        command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut job = JobProcess::spawn(&mut command, &name).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while job.try_wait().unwrap().is_none() {
            assert!(std::time::Instant::now() < deadline, "the job did not end");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        // A Tinto that restarted finds what the job left by its name.
        let mut found = JobProcess::ended(job.pid(), &name);
        let left = found.sweep();
        let server = if cfg!(windows) { "ping" } else { "sleep" };
        assert!(
            left.iter()
                .any(|process| process.to_lowercase().contains(server)),
            "{left:?}"
        );
        // Stopped: nothing is left for the next look.
        while !job.sweep().is_empty() {
            assert!(std::time::Instant::now() < deadline, "it kept running");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
