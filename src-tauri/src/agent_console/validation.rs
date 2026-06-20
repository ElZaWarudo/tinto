use std::path::PathBuf;

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
}
