use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "windows")]
use crate::windows_process::hide_console;

use super::AgentConsoleError;

pub const ALLOWED_AGENTS: &[&str] = &["claude", "codex", "opencode"];

pub fn resolve_agent_binary(agent_type: &str) -> Result<PathBuf, AgentConsoleError> {
    resolve_agent_binary_with(agent_type, |binary| which::which(binary))
}

pub fn resolve_agent_binary_with<F>(
    agent_type: &str,
    finder: F,
) -> Result<PathBuf, AgentConsoleError>
where
    F: FnOnce(&str) -> Result<PathBuf, which::Error>,
{
    validate_agent_type(agent_type)?;
    let binary_path =
        finder(agent_type).map_err(|_| binary_not_found_error(agent_type.to_string()))?;
    if !binary_path.is_file() {
        return Err(binary_not_found_error(agent_type.to_string()));
    }
    Ok(binary_path)
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

pub fn wsl_agent_binary_available(
    distro: &str,
    agent_type: &str,
) -> Result<bool, AgentConsoleError> {
    validate_agent_type(agent_type)?;
    wsl_command_available(distro, agent_type)
}

pub fn ensure_wsl_agent_binary(distro: &str, agent_type: &str) -> Result<(), AgentConsoleError> {
    if wsl_agent_binary_available(distro, agent_type)? {
        Ok(())
    } else {
        Err(binary_not_found_error(agent_type.to_string()))
    }
}

#[cfg(target_os = "windows")]
fn wsl_command_available(distro: &str, agent_type: &str) -> Result<bool, AgentConsoleError> {
    let argv = build_wsl_command_available_argv(distro, agent_type)?;
    let Some((program, args)) = argv.split_first() else {
        return Err(AgentConsoleError::new(
            "wsl_spawn_failed",
            "comando WSL vacio",
        ));
    };
    let mut command = Command::new(program);
    let output = hide_console(command.args(args))
        .output()
        .map_err(map_wsl_spawn_error)?;
    Ok(output.status.success() && !output.stdout.is_empty())
}

#[cfg(not(target_os = "windows"))]
fn wsl_command_available(_distro: &str, _agent_type: &str) -> Result<bool, AgentConsoleError> {
    Err(AgentConsoleError::new(
        "missing_wsl",
        "WSL solo esta disponible en Windows",
    ))
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn build_wsl_command_available_argv(
    distro: &str,
    agent_type: &str,
) -> Result<Vec<String>, AgentConsoleError> {
    if distro.trim().is_empty() {
        return Err(AgentConsoleError::new(
            "missing_distro",
            "no se configuro la distro WSL",
        ));
    }
    validate_agent_type(agent_type)?;
    Ok(vec![
        "wsl.exe".into(),
        "-d".into(),
        distro.to_string(),
        "--exec".into(),
        "bash".into(),
        "-lc".into(),
        "export PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH\"; command -v -- \"$1\"".into(),
        "tinto-agent-console-check".into(),
        agent_type.to_string(),
    ])
}

#[cfg(target_os = "windows")]
fn map_wsl_spawn_error(error: std::io::Error) -> AgentConsoleError {
    if error.kind() == std::io::ErrorKind::NotFound {
        return AgentConsoleError::new("missing_wsl", "no se encontro wsl.exe");
    }
    AgentConsoleError::new("wsl_spawn_failed", "no se pudo iniciar WSL")
}

fn binary_not_found_error(agent_type: String) -> AgentConsoleError {
    AgentConsoleError::new(
        "binary_not_found",
        format!("no se encontro el binario '{agent_type}' en PATH"),
    )
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
    fn builds_wsl_command_available_check_without_interpolation() {
        let argv = build_wsl_command_available_argv("Ubuntu", "codex").unwrap();

        assert_eq!(&argv[..5], ["wsl.exe", "-d", "Ubuntu", "--exec", "bash"]);
        assert_eq!(
            argv[6],
            "export PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH\"; command -v -- \"$1\""
        );
        assert_eq!(argv[8], "codex");
    }

    #[test]
    fn wsl_available_check_rejects_unsupported_agent_before_spawn() {
        let error = build_wsl_command_available_argv("Ubuntu", "powershell").unwrap_err();

        assert_eq!(error.category, "unsupported_agent");
    }
}
