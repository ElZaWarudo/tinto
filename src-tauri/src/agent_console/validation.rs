use std::{
    env, fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::AgentConsoleError;

pub const ALLOWED_AGENTS: &[&str] = &["claude", "codex", "kimi", "opencode", "muse"];

/// Runtimes that can run without their sandbox, chosen per session.
pub fn supports_full_access(agent_type: &str) -> bool {
    agent_type.eq_ignore_ascii_case("codex") || agent_type.eq_ignore_ascii_case("muse")
}

pub fn resolve_agent_binary(agent_type: &str) -> Result<PathBuf, AgentConsoleError> {
    resolve_agent_binary_with_candidates(
        agent_type,
        |binary| which::which(binary),
        fallback_agent_candidates,
    )
}

pub fn resolve_agent_binary_with<F>(
    agent_type: &str,
    finder: F,
) -> Result<PathBuf, AgentConsoleError>
where
    F: FnOnce(&str) -> Result<PathBuf, which::Error>,
{
    resolve_agent_binary_with_candidates(agent_type, finder, |_| Vec::new())
}

fn resolve_agent_binary_with_candidates<F, C>(
    agent_type: &str,
    finder: F,
    candidates: C,
) -> Result<PathBuf, AgentConsoleError>
where
    F: FnOnce(&str) -> Result<PathBuf, which::Error>,
    C: FnOnce(&str) -> Vec<PathBuf>,
{
    validate_agent_type(agent_type)?;
    if let Ok(binary_path) = finder(agent_type) {
        if is_usable_binary(agent_type, &binary_path) {
            return Ok(binary_path);
        }
    }
    for candidate in candidates(agent_type) {
        if is_usable_binary(agent_type, &candidate) {
            return Ok(candidate);
        }
    }
    Err(binary_not_found_error(agent_type.to_string()))
}

pub fn validate_agent_type(agent_type: &str) -> Result<&'static str, AgentConsoleError> {
    ALLOWED_AGENTS
        .iter()
        .copied()
        .find(|allowed| *allowed == agent_type)
        .ok_or_else(|| {
            AgentConsoleError::new(
                "unsupported_agent",
                format!("agente no soportado: '{agent_type}'"),
            )
        })
}

fn binary_not_found_error(agent_type: String) -> AgentConsoleError {
    AgentConsoleError::new(
        "binary_not_found",
        format!("no se encontro el binario '{agent_type}' en PATH"),
    )
}

fn is_usable_binary(agent_type: &str, path: &Path) -> bool {
    path.is_file() && is_executable(path) && !is_windowsapps_codex_alias(agent_type, path)
}

/// Follows symlinks: a link to a file without the execute bit is not usable.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

fn is_windowsapps_codex_alias(agent_type: &str, path: &Path) -> bool {
    if !agent_type.eq_ignore_ascii_case("codex") {
        return false;
    }
    let mut previous_was_windowsapps = false;
    path.components().any(|component| {
        let component = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        let matches = previous_was_windowsapps && component.starts_with("openai.codex_");
        previous_was_windowsapps = component == "windowsapps";
        matches
    })
}

#[cfg(windows)]
fn fallback_agent_candidates(agent_type: &str) -> Vec<PathBuf> {
    if agent_type.eq_ignore_ascii_case("claude") {
        // Claude Code's own installer puts it in ~/.local/bin, which is not
        // always on the PATH Tinto starts with.
        let installed = env::var_os("USERPROFILE").map(|home| {
            PathBuf::from(home)
                .join(".local")
                .join("bin")
                .join("claude.exe")
        });
        return sorted_existing_candidates(installed.into_iter().collect());
    }
    if agent_type.eq_ignore_ascii_case("muse") {
        // Muse Code's installer puts its launcher in
        // %LOCALAPPDATA%\Programs\muse, which may not be on Tinto's PATH.
        let installed = env::var_os("LOCALAPPDATA").map(|local_appdata| {
            PathBuf::from(local_appdata)
                .join("Programs")
                .join("muse")
                .join("muse.cmd")
        });
        return sorted_existing_candidates(installed.into_iter().collect());
    }
    if !agent_type.eq_ignore_ascii_case("codex") {
        return Vec::new();
    }

    let mut paths = Vec::new();
    if let Some(local_appdata) = env::var_os("LOCALAPPDATA") {
        append_codex_install_candidates(&PathBuf::from(local_appdata), &mut paths);
    }
    if let Some(path) = env::var_os("PATH") {
        paths.extend(env::split_paths(&path).map(|entry| entry.join("codex.exe")));
    }

    sorted_existing_candidates(paths)
}

/// The newest Claude Code the Claude desktop app installed. It is not on
/// PATH: `<appdata>\Claude\claude-code\<version>\<build>\claude.exe`.
#[cfg(windows)]
pub fn claude_desktop_binary(appdata: &Path) -> Option<PathBuf> {
    let versions = appdata.join("Claude").join("claude-code");
    let mut paths = Vec::new();
    for version in fs::read_dir(versions).into_iter().flatten().flatten() {
        for build in fs::read_dir(version.path()).into_iter().flatten().flatten() {
            paths.push(build.path().join("claude.exe"));
        }
    }
    sorted_existing_candidates(paths).into_iter().next()
}

#[cfg(windows)]
fn append_codex_install_candidates(local_appdata: &Path, paths: &mut Vec<PathBuf>) {
    let versioned_bin = local_appdata.join("OpenAI").join("Codex").join("bin");
    if let Ok(entries) = fs::read_dir(versioned_bin) {
        paths.extend(
            entries
                .flatten()
                .map(|entry| entry.path().join("codex.exe")),
        );
    }

    paths.push(
        local_appdata
            .join("Packages")
            .join("OpenAI.Codex_2p2nqsd0c76g0")
            .join("LocalCache")
            .join("Local")
            .join("OpenAI")
            .join("Codex")
            .join("bin")
            .join("codex.exe"),
    );
}

fn sorted_existing_candidates(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut candidates = paths
        .into_iter()
        .filter_map(|path| {
            let metadata = fs::metadata(&path).ok()?;
            if !metadata.is_file() {
                return None;
            }
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some((modified, path))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(modified, _)| *modified);
    candidates.dedup_by(|(_, left), (_, right)| left == right);
    candidates.into_iter().rev().map(|(_, path)| path).collect()
}

#[cfg(not(windows))]
fn fallback_agent_candidates(agent_type: &str) -> Vec<PathBuf> {
    // Apps started from a desktop launcher do not get the PATH the shell's rc
    // files build, so the installers' own folders under $HOME may be missing.
    env::var_os("HOME")
        .map(|home| home_agent_candidates(Path::new(&home), agent_type))
        .unwrap_or_default()
}

/// Where the agents' own installers put them under `home`, in order.
#[cfg(not(windows))]
fn home_agent_candidates(home: &Path, agent_type: &str) -> Vec<PathBuf> {
    let local_bin = home.join(".local").join("bin").join(agent_type);
    match agent_type {
        "claude" => {
            let versions = fs::read_dir(home.join(".local/share/claude/versions"))
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .collect();
            let mut paths = vec![local_bin, home.join(".claude/local/claude")];
            paths.extend(sorted_existing_candidates(versions).into_iter().next());
            paths
        }
        "codex" => vec![
            local_bin,
            home.join(".codex/packages/standalone/current/bin/codex"),
            home.join(".npm-global/bin/codex"),
            home.join(".cargo/bin/codex"),
        ],
        _ => vec![local_bin],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_binary(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "fake").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn accepts_kimi_agent() {
        assert_eq!(validate_agent_type("kimi").unwrap(), "kimi");
    }

    #[test]
    fn accepts_muse_agent() {
        assert_eq!(validate_agent_type("muse").unwrap(), "muse");
    }

    #[test]
    fn rejects_agents_outside_allowlist() {
        let error = validate_agent_type("powershell").unwrap_err();

        assert_eq!(error.category, "unsupported_agent");
    }

    #[test]
    fn rejects_allowlisted_agent_when_binary_is_missing() {
        let error = resolve_agent_binary_with("codex", |_| Err(which::Error::CannotFindBinaryPath))
            .unwrap_err();

        assert_eq!(error.category, "binary_not_found");
    }

    #[test]
    fn resolves_allowlisted_agent_to_file() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("codex");
        fake_binary(&binary);

        let resolved = resolve_agent_binary_with("codex", |_| Ok(binary.clone())).unwrap();

        assert_eq!(resolved, binary);
    }

    #[test]
    fn rejects_resolved_directory_as_binary() {
        let temp = tempfile::tempdir().unwrap();
        let error =
            resolve_agent_binary_with("codex", |_| Ok(temp.path().to_path_buf())).unwrap_err();

        assert_eq!(error.category, "binary_not_found");
    }

    #[test]
    fn falls_back_when_path_resolves_to_windowsapps_codex_alias() {
        let temp = tempfile::tempdir().unwrap();
        let protected = temp
            .path()
            .join("WindowsApps")
            .join("OpenAI.Codex_1.0.0_x64__abc")
            .join("app")
            .join("resources")
            .join("codex.exe");
        std::fs::create_dir_all(protected.parent().unwrap()).unwrap();
        std::fs::write(&protected, "protected").unwrap();
        let binary = temp.path().join("codex.exe");
        fake_binary(&binary);

        let resolved = resolve_agent_binary_with_candidates(
            "codex",
            |_| Ok(protected),
            |_| vec![binary.clone()],
        )
        .unwrap();

        assert_eq!(resolved, binary);
    }

    #[test]
    fn rejects_windowsapps_codex_alias_without_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let protected = temp
            .path()
            .join("WindowsApps")
            .join("OpenAI.Codex_1.0.0_x64__abc")
            .join("app")
            .join("resources")
            .join("codex.exe");
        std::fs::create_dir_all(protected.parent().unwrap()).unwrap();
        std::fs::write(&protected, "protected").unwrap();
        let error =
            resolve_agent_binary_with_candidates("codex", |_| Ok(protected), |_| Vec::new())
                .unwrap_err();

        assert_eq!(error.category, "binary_not_found");
    }

    #[test]
    #[cfg(windows)]
    fn finds_the_claude_code_of_the_desktop_app() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(claude_desktop_binary(temp.path()), None);
        let build = temp
            .path()
            .join("Claude")
            .join("claude-code")
            .join("2.1.293")
            .join("83cb0bd7fed4");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("claude.exe"), "fake").unwrap();
        assert_eq!(
            claude_desktop_binary(temp.path()),
            Some(build.join("claude.exe"))
        );
    }

    #[cfg(not(windows))]
    fn resolve_in_home(home: &Path, agent_type: &str) -> Result<PathBuf, AgentConsoleError> {
        resolve_agent_binary_with_candidates(
            agent_type,
            |_| Err(which::Error::CannotFindBinaryPath),
            |agent| home_agent_candidates(home, agent),
        )
    }

    #[test]
    #[cfg(unix)]
    fn finds_agents_their_installers_put_under_home_when_path_lacks_them() {
        let layouts = [
            (
                "claude",
                ".local/bin/claude",
                ".local/share/claude/versions/2.1.0",
            ),
            ("claude", ".claude/local/claude", ".claude/local/claude"),
            (
                "claude",
                ".local/share/claude/versions/2.1.0",
                ".local/share/claude/versions/2.1.0",
            ),
            (
                "codex",
                ".local/bin/codex",
                ".codex/packages/standalone/current/bin/codex",
            ),
            (
                "codex",
                ".codex/packages/standalone/current/bin/codex",
                ".codex/packages/standalone/current/bin/codex",
            ),
            ("codex", ".npm-global/bin/codex", ".npm-global/bin/codex"),
            ("codex", ".cargo/bin/codex", ".cargo/bin/codex"),
            ("muse", ".local/bin/muse", ".local/bin/muse"),
            ("kimi", ".local/bin/kimi", ".local/bin/kimi"),
        ];
        for (agent, entry, target) in layouts {
            let home = tempfile::tempdir().unwrap();
            fake_binary(&home.path().join(target));
            if entry != target {
                let link = home.path().join(entry);
                fs::create_dir_all(link.parent().unwrap()).unwrap();
                std::os::unix::fs::symlink(home.path().join(target), link).unwrap();
            }

            let resolved = resolve_in_home(home.path(), agent).unwrap();

            assert_eq!(resolved, home.path().join(entry), "{agent} at ~/{entry}");
        }
    }

    #[test]
    #[cfg(unix)]
    fn picks_the_newest_claude_version() {
        let home = tempfile::tempdir().unwrap();
        let versions = home.path().join(".local/share/claude/versions");
        fake_binary(&versions.join("2.1.0"));
        fake_binary(&versions.join("2.0.9"));
        fs::File::options()
            .write(true)
            .open(versions.join("2.0.9"))
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH)
            .unwrap();

        assert_eq!(
            resolve_in_home(home.path(), "claude").unwrap(),
            versions.join("2.1.0")
        );
    }

    #[test]
    #[cfg(unix)]
    fn home_without_a_usable_agent_is_binary_not_found() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_in_home(home.path(), "claude").unwrap_err().category,
            "binary_not_found"
        );
        // A link to a file that cannot run is not an install.
        let target = home
            .path()
            .join(".codex/packages/standalone/current/bin/codex");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "not executable").unwrap();
        fs::create_dir_all(home.path().join(".local/bin")).unwrap();
        std::os::unix::fs::symlink(&target, home.path().join(".local/bin/codex")).unwrap();
        assert_eq!(
            resolve_in_home(home.path(), "codex").unwrap_err().category,
            "binary_not_found"
        );
    }
}
