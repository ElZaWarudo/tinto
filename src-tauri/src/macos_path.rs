//! Apps opened from the Dock or Finder get launchd's PATH
//! (`/usr/bin:/bin:/usr/sbin:/sbin`), not the one the user's shell builds, so
//! agents, npm and Gitleaks installed with Homebrew, nvm or their own
//! installers are not found. At startup Tinto takes the PATH the user's login
//! shell gives, as a terminal would see it.

use std::{
    env,
    ffi::{OsStr, OsString},
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MARKER: &str = "__TINTO_PATH__";
const SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// Puts the login shell's PATH before the inherited one when Tinto was not
/// started from a terminal. Must run before any other thread starts: it
/// changes the process environment.
pub(crate) fn adopt_login_shell_path() {
    // A terminal already gave its PATH, maybe with changes of its session.
    if env::var_os("TERM").is_some() {
        return;
    }
    let Some(shell_path) = login_shell_path() else {
        return;
    };
    let inherited = env::var_os("PATH").unwrap_or_default();
    if let Ok(path) = merge_paths(&shell_path, &inherited) {
        env::set_var("PATH", path);
    }
}

fn login_shell_path() -> Option<OsString> {
    let shell = env::var_os("SHELL")
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| OsString::from("/bin/zsh"));
    // Interactive too: nvm and most installers set PATH up in ~/.zshrc.
    let mut child = Command::new(shell)
        .arg("-ilc")
        .arg(format!("printf '{MARKER}%s{MARKER}' \"$PATH\""))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + SHELL_TIMEOUT;
    while child.try_wait().ok()?.is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut output = String::new();
    child.stdout.take()?.read_to_string(&mut output).ok()?;
    marked_path(&output).map(OsString::from)
}

/// The PATH between the markers; whatever the shell's startup files print
/// around it is ignored.
fn marked_path(output: &str) -> Option<&str> {
    let (_, rest) = output.split_once(MARKER)?;
    let (path, _) = rest.split_once(MARKER)?;
    (!path.is_empty()).then_some(path)
}

fn merge_paths(first: &OsStr, then: &OsStr) -> Result<OsString, env::JoinPathsError> {
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in env::split_paths(first).chain(env::split_paths(then)) {
        if !entry.as_os_str().is_empty() && !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    env::join_paths(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_path_between_the_markers() {
        let output = format!("Welcome back!\n{MARKER}/opt/homebrew/bin:/usr/bin{MARKER}bye");
        assert_eq!(marked_path(&output), Some("/opt/homebrew/bin:/usr/bin"));
    }

    #[test]
    fn a_shell_that_gave_no_path_gives_none() {
        assert_eq!(marked_path("zsh: command not found: printf"), None);
        assert_eq!(marked_path(&format!("{MARKER}{MARKER}")), None);
    }

    #[test]
    fn the_shell_path_goes_first_without_repeating_entries() {
        let merged = merge_paths(
            OsStr::new("/Users/me/.local/bin:/opt/homebrew/bin:/usr/bin"),
            OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin"),
        )
        .unwrap();
        assert_eq!(
            merged,
            OsString::from("/Users/me/.local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin")
        );
    }
}
