use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::{env, fs, time::SystemTime};

use super::AgentConsoleError;

pub const ALLOWED_AGENTS: &[&str] = &["claude", "codex", "opencode"];

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
    path.is_file() && !is_windowsapps_codex_alias(agent_type, path)
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

#[cfg(windows)]
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
fn fallback_agent_candidates(_agent_type: &str) -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        std::fs::write(&binary, "fake").unwrap();

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
        std::fs::write(&binary, "fake").unwrap();

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
}
