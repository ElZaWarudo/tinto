//! The "Entorno" check of a repo's batch bar: whether what its jobs need
//! outside the worktree answers now. Docker, when the repo uses it, and the
//! verification commands the user marked as environment probes (a database
//! ping…), each run where the repo's jobs run, with a time limit.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::model::DeliveryRepoSettings;
use super::tasks::plain_path;

/// How long a probe gets before it counts as not answering.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
/// The end of a probe's output, shown on hover.
const OUTPUT_CHARS: usize = 2000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryEnvironmentCheck {
    /// `Docker` or the probe's command.
    pub name: String,
    pub ok: bool,
    /// What it printed (its end), or why it failed.
    pub output: String,
}

/// Runs the repo's checks at once and waits for all of them.
pub fn check(
    repo: &Path,
    distro: Option<&str>,
    settings: &DeliveryRepoSettings,
) -> Vec<DeliveryEnvironmentCheck> {
    check_within(repo, distro, settings, PROBE_TIMEOUT)
}

fn check_within(
    repo: &Path,
    distro: Option<&str>,
    settings: &DeliveryRepoSettings,
    timeout: Duration,
) -> Vec<DeliveryEnvironmentCheck> {
    let mut names = Vec::new();
    let mut found = Vec::new();
    if uses_docker(repo, distro, settings) {
        names.push(None);
        // A full disk hangs Docker's engine and fails every install.
        found.extend(disk(repo, distro));
    }
    // A probe whose command is no longer a verification command is not run.
    names.extend(
        settings
            .environment_probes
            .iter()
            .filter(|probe| settings.checks.contains(probe))
            .map(Some),
    );
    std::thread::scope(|scope| {
        let running: Vec<_> = names
            .into_iter()
            .map(|probe| {
                scope.spawn(move || match probe {
                    None => docker(repo, distro, timeout),
                    Some(command) => {
                        let (ok, output) = run(probe_command(repo, distro, command), timeout);
                        DeliveryEnvironmentCheck {
                            name: command.clone(),
                            ok,
                            output,
                        }
                    }
                })
            })
            .collect();
        running
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .chain(found)
            .collect()
    })
}

/// Below this much free space on the repo's disk the environment fails.
const MIN_FREE_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// The free space of the disk the repo is on, when the system says it.
fn disk(repo: &Path, distro: Option<&str>) -> Option<DeliveryEnvironmentCheck> {
    let free = free_bytes(&readable_root(repo, distro))?;
    let gb = free as f64 / (1024.0 * 1024.0 * 1024.0);
    Some(DeliveryEnvironmentCheck {
        name: "Disco".to_string(),
        ok: free >= MIN_FREE_BYTES,
        output: if free >= MIN_FREE_BYTES {
            format!("Quedan {gb:.0} GB libres.")
        } else {
            format!("Solo quedan {gb:.1} GB libres: Docker y las instalaciones fallan sin espacio. Libera espacio en el disco del repositorio.")
        },
    })
}

#[cfg(windows)]
fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut free = 0u64;
    // SAFETY: `wide` is a NUL-terminated path and `free` outlives the call.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
fn free_bytes(_path: &Path) -> Option<u64> {
    None
}

/// A compose file or Dockerfile at the repo's root, or a command of its
/// settings that calls `docker`.
fn uses_docker(repo: &Path, distro: Option<&str>, settings: &DeliveryRepoSettings) -> bool {
    const FILES: &[&str] = &[
        "compose.yaml",
        "compose.yml",
        "docker-compose.yaml",
        "docker-compose.yml",
        "Dockerfile",
    ];
    let root = readable_root(repo, distro);
    FILES.iter().any(|file| root.join(file).is_file())
        || settings
            .checks
            .iter()
            .chain(&settings.qa_commands)
            .chain(&settings.bootstrap)
            .any(|command| command.contains("docker"))
}

/// The repo as Windows reads it: a WSL repo through the distro's share.
fn readable_root(repo: &Path, distro: Option<&str>) -> PathBuf {
    let text = repo.to_string_lossy();
    match distro {
        Some(distro) if text.starts_with('/') => PathBuf::from(format!(
            r"\\wsl.localhost\{distro}{}",
            text.replace('/', "\\")
        )),
        _ => repo.to_path_buf(),
    }
}

fn docker(repo: &Path, distro: Option<&str>, timeout: Duration) -> DeliveryEnvironmentCheck {
    const INFO: &str = "docker info --format '{{.ServerVersion}}'";
    let command = match distro {
        Some(_) => probe_command(repo, distro, INFO),
        None => {
            let mut command = Command::new("docker");
            command.args(["info", "--format", "{{.ServerVersion}}"]);
            command
        }
    };
    let (ok, output) = run(command, timeout);
    DeliveryEnvironmentCheck {
        name: "Docker".to_string(),
        ok,
        output: if ok {
            format!("Docker {output} responde.")
        } else {
            output
        },
    }
}

/// The probe as a shell runs it in the repo: `bash -lc` in the distro for
/// a WSL repo, as its jobs do; `cmd.exe` on Windows.
fn probe_command(repo: &Path, distro: Option<&str>, probe: &str) -> Command {
    if let Some(distro) = distro {
        let mut command = Command::new("wsl.exe");
        command.args(["-d", distro, "--cd"]);
        command.arg(repo.to_string_lossy().as_ref());
        command.args(["--exec", "bash", "-lc", probe]);
        return command;
    }
    #[cfg(windows)]
    let command = {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("cmd.exe");
        // `/S` takes the text between the outer quotes as it is.
        command
            .args(["/D", "/S", "/C"])
            .raw_arg(format!("\"{probe}\""));
        command
    };
    #[cfg(not(windows))]
    let command = {
        let mut command = Command::new("sh");
        command.args(["-c", probe]);
        command
    };
    let mut command = command;
    command.current_dir(plain_path(repo));
    command
}

/// Runs a command for at most `timeout`: whether it succeeded and the end
/// of what it printed, or why not.
fn run(mut command: Command, timeout: Duration) -> (bool, String) {
    #[cfg(windows)]
    crate::windows_process::hide_console(&mut command);
    let spawned = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let program = command.get_program().to_string_lossy().to_string();
            return (
                false,
                format!("{program} no está instalado en este equipo."),
            );
        }
        Err(error) => return (false, format!("No se pudo ejecutar: {error}")),
    };
    // Read as it prints, so a full pipe never holds the command.
    let (sender, received) = mpsc::channel();
    for stream in [
        child
            .stdout
            .take()
            .map(|out| Box::new(out) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|err| Box::new(err) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let sender = sender.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stream.take(256 * 1024).read_to_end(&mut bytes);
            let _ = sender.send(bytes);
        });
    }
    drop(sender);
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    // What a process it started still holds open is not waited for.
    let mut printed = Vec::new();
    while let Ok(bytes) = received.recv_timeout(Duration::from_secs(1)) {
        printed.extend(bytes);
    }
    let printed = tail(String::from_utf8_lossy(&printed).trim());
    match status {
        Some(status) if status.success() => (true, printed),
        Some(status) => {
            let code = status.code().map_or("sin código".to_string(), |code| {
                format!("con código {code}")
            });
            let what = if printed.is_empty() {
                "sin salida".to_string()
            } else {
                printed
            };
            (false, format!("Salió {code}: {what}"))
        }
        None => (
            false,
            format!("No respondió en {} s. {printed}", timeout.as_secs())
                .trim()
                .to_string(),
        ),
    }
}

fn tail(text: &str) -> String {
    let count = text.chars().count();
    if count <= OUTPUT_CHARS {
        return text.to_string();
    }
    let rest: String = text.chars().skip(count - OUTPUT_CHARS).collect();
    format!("…{rest}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(checks: &[&str], probes: &[&str]) -> DeliveryRepoSettings {
        DeliveryRepoSettings {
            checks: checks.iter().map(|c| c.to_string()).collect(),
            environment_probes: probes.iter().map(|c| c.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn runs_only_the_marked_checks_and_says_how_each_went() {
        let repo = tempfile::tempdir().unwrap();
        let checks = settings(
            &["echo listo", "exit 3", "npm test"],
            &["echo listo", "exit 3", "gone"],
        );
        let results = check_within(repo.path(), None, &checks, Duration::from_secs(20));
        assert_eq!(results.len(), 2, "{results:?}");
        assert_eq!(results[0].name, "echo listo");
        assert!(results[0].ok);
        assert_eq!(results[0].output, "listo");
        assert_eq!(results[1].name, "exit 3");
        assert!(!results[1].ok);
        assert_eq!(results[1].output, "Salió con código 3: sin salida");
    }

    #[test]
    fn a_probe_that_does_not_answer_fails_in_time() {
        let repo = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let slow = "ping -n 30 127.0.0.1 > nul";
        #[cfg(not(windows))]
        let slow = "sleep 30";
        let started = Instant::now();
        let results = check_within(
            repo.path(),
            None,
            &settings(&[slow], &[slow]),
            Duration::from_millis(300),
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!results[0].ok);
        assert!(results[0].output.starts_with("No respondió"), "{results:?}");
    }

    #[test]
    fn docker_is_checked_only_when_the_repo_uses_it() {
        let repo = tempfile::tempdir().unwrap();
        assert!(!uses_docker(
            repo.path(),
            None,
            &settings(&["npm test"], &[])
        ));
        assert!(uses_docker(
            repo.path(),
            None,
            &settings(&["docker compose run tests"], &[])
        ));
        std::fs::write(repo.path().join("compose.yaml"), "services: {}\n").unwrap();
        assert!(uses_docker(repo.path(), None, &settings(&[], &[])));
    }

    #[test]
    fn a_repo_with_docker_also_has_its_disk_checked() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("compose.yaml"), "services: {}\n").unwrap();
        let results = check_within(
            repo.path(),
            None,
            &settings(&[], &[]),
            Duration::from_secs(1),
        );
        let disk = results.iter().find(|result| result.name == "Disco");
        assert_eq!(disk.is_some(), cfg!(windows), "{results:?}");
    }

    #[test]
    fn a_long_output_keeps_its_end() {
        let long = "a".repeat(OUTPUT_CHARS) + "fin";
        let kept = tail(&long);
        assert!(kept.starts_with('…') && kept.ends_with("fin"));
        assert_eq!(kept.chars().count(), OUTPUT_CHARS + 1);
    }
}
