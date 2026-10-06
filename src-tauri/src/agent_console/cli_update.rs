//! Version check and update for installed agent CLIs.
//!
//! Each CLI is updated the way it was installed: global npm packages with
//! `npm install -g <package>@latest`, Claude Code's native installer with
//! `claude update`. CLIs managed by another app (the Codex desktop app) or
//! installed some other way only report versions. The command is always
//! derived here from fixed recipes; the UI never supplies one.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use crate::bus::contract::{
    AgentCliUpdateMethod, AgentCliUpdateOutcome, AgentCliUpdateStatus, AgentProviderSource,
};

use super::{
    install::{
        local_npm_launcher, run_process, runtime_launch, ProcessLaunch, ProcessResult,
        INSTALL_TIMEOUT, PROBE_TIMEOUT,
    },
    validation::{resolve_agent_binary, validate_agent_type},
    AgentConsoleError,
};

/// Global npm packages that provide each CLI, in lookup order. The first one
/// is also used to look up the latest version for non-npm installs.
fn npm_packages(agent_type: &str) -> &'static [&'static str] {
    match agent_type {
        "claude" => &["@anthropic-ai/claude-code"],
        "codex" => &["@openai/codex"],
        "kimi" => &["@moonshot-ai/kimi-code"],
        "opencode" => &["opencode-ai", "@opencode/cli"],
        _ => &[],
    }
}

/// Subcommand of CLIs that update themselves when not installed through npm.
fn self_update_subcommand(agent_type: &str) -> Option<&'static str> {
    match agent_type {
        "claude" => Some("update"),
        "opencode" => Some("upgrade"),
        _ => None,
    }
}

/// What a probe found, before it is turned into the public status.
#[derive(Debug, Default, PartialEq, Eq)]
struct Probe {
    installed: bool,
    binary: Option<PathBuf>,
    version_line: Option<String>,
    package: Option<String>,
    method: Option<AgentCliUpdateMethod>,
    latest: Option<String>,
}

pub fn status(
    source: AgentProviderSource,
    distro: Option<&str>,
    agent_type: &str,
) -> Result<AgentCliUpdateStatus, AgentConsoleError> {
    validate_agent_type(agent_type)?;
    let probe = match source {
        AgentProviderSource::Local => probe_local(agent_type),
        AgentProviderSource::Wsl => probe_wsl(distro, agent_type, "status")?,
    };
    Ok(status_from_probe(source, distro, agent_type, &probe))
}

pub fn update(
    source: AgentProviderSource,
    distro: Option<&str>,
    agent_type: &str,
) -> Result<AgentCliUpdateOutcome, AgentConsoleError> {
    validate_agent_type(agent_type)?;
    match source {
        AgentProviderSource::Local => update_local(agent_type),
        AgentProviderSource::Wsl => {
            let probe = probe_wsl(distro, agent_type, "update")?;
            Ok(AgentCliUpdateOutcome {
                updated: true,
                version: probe.version_line.as_deref().and_then(parse_version),
                message: "CLI actualizado".to_string(),
            })
        }
    }
}

fn status_from_probe(
    source: AgentProviderSource,
    distro: Option<&str>,
    agent_type: &str,
    probe: &Probe,
) -> AgentCliUpdateStatus {
    let installed_version = probe.version_line.as_deref().and_then(parse_version);
    let latest_version = probe.latest.as_deref().and_then(parse_version);
    let method = if probe.installed {
        probe.method.unwrap_or(AgentCliUpdateMethod::Unmanaged)
    } else {
        AgentCliUpdateMethod::Missing
    };
    let newer = match (&installed_version, &latest_version) {
        (Some(installed), Some(latest)) => version_is_newer(latest, installed),
        _ => false,
    };
    let command_display = match method {
        AgentCliUpdateMethod::Npm => probe
            .package
            .as_ref()
            .map(|package| format!("npm install -g {package}@latest")),
        AgentCliUpdateMethod::SelfUpdate => self_update_subcommand(agent_type)
            .map(|subcommand| format!("{agent_type} {subcommand}")),
        _ => None,
    };
    AgentCliUpdateStatus {
        agent_type: agent_type.to_string(),
        source,
        distro: distro.map(str::to_string),
        method,
        installed_version,
        latest_version,
        update_available: newer,
        command_display,
    }
}

// ---- Windows / host ------------------------------------------------------

fn probe_local(agent_type: &str) -> Probe {
    let Ok(binary) = resolve_agent_binary(agent_type) else {
        return Probe::default();
    };
    let version_line = run(
        &ProcessLaunch {
            program: binary.clone(),
            prefix_args: Vec::new(),
        },
        &["--version"],
    )
    .ok()
    .and_then(|output| first_line(&output.stdout));
    let npm = local_npm_launcher().ok();
    let package = npm
        .as_ref()
        .and_then(|npm| installed_npm_package(npm, agent_type));
    let method = if package.is_some() {
        AgentCliUpdateMethod::Npm
    } else {
        local_non_npm_method(agent_type, &binary)
    };
    let lookup = package.clone().or_else(|| {
        npm_packages(agent_type)
            .first()
            .map(|name| name.to_string())
    });
    let latest = match (&npm, lookup) {
        (Some(npm), Some(package)) => run(npm, &["view", &package, "version"])
            .ok()
            .filter(|output| output.success)
            .and_then(|output| first_line(&output.stdout)),
        _ => None,
    };
    Probe {
        installed: true,
        binary: Some(binary),
        version_line,
        package,
        method: Some(method),
        latest,
    }
}

fn update_local(agent_type: &str) -> Result<AgentCliUpdateOutcome, AgentConsoleError> {
    let probe = probe_local(agent_type);
    let binary = probe
        .binary
        .clone()
        .ok_or_else(|| AgentConsoleError::new("missing_agent", "el CLI no esta instalado"))?;
    let result = match (probe.method, probe.package.as_deref()) {
        (Some(AgentCliUpdateMethod::Npm), Some(package)) => run_with_timeout(
            &local_npm_launcher()?,
            &["install", "-g", &format!("{package}@latest")],
        )?,
        (Some(AgentCliUpdateMethod::SelfUpdate), _) => run_with_timeout(
            &ProcessLaunch {
                program: binary.clone(),
                prefix_args: Vec::new(),
            },
            &[self_update_subcommand(agent_type).unwrap_or("update")],
        )?,
        _ => {
            return Err(AgentConsoleError::new(
                "update_unsupported",
                "Tinto no puede actualizar esta instalacion del CLI",
            ))
        }
    };
    if !result.success {
        return Ok(AgentCliUpdateOutcome {
            updated: false,
            version: probe.version_line.as_deref().and_then(parse_version),
            message: failure_message(&result),
        });
    }
    let version = run(
        &ProcessLaunch {
            program: binary,
            prefix_args: Vec::new(),
        },
        &["--version"],
    )
    .ok()
    .and_then(|output| first_line(&output.stdout))
    .as_deref()
    .and_then(parse_version);
    Ok(AgentCliUpdateOutcome {
        updated: true,
        version,
        message: "CLI actualizado".to_string(),
    })
}

fn installed_npm_package(npm: &ProcessLaunch, agent_type: &str) -> Option<String> {
    npm_packages(agent_type).iter().find_map(|package| {
        let output = run(npm, &["ls", "-g", "--depth=0", "--json", package]).ok()?;
        npm_ls_lists(&output.stdout, package).then(|| package.to_string())
    })
}

/// Claude Code's native installer and the Codex desktop app place their CLIs
/// in recognizable folders; anything else is left to the user.
fn local_non_npm_method(agent_type: &str, binary: &Path) -> AgentCliUpdateMethod {
    let path = binary.to_string_lossy().replace('\\', "/").to_lowercase();
    match agent_type {
        "claude" if path.contains("/.local/bin/") || path.contains("/.local/share/claude/") => {
            AgentCliUpdateMethod::SelfUpdate
        }
        "codex"
            if path.contains("/openai/codex/")
                || path.contains("/windowsapps/")
                || path.contains("openai.codex") =>
        {
            AgentCliUpdateMethod::App
        }
        "opencode" => AgentCliUpdateMethod::SelfUpdate,
        _ => AgentCliUpdateMethod::Unmanaged,
    }
}

// ---- WSL ----------------------------------------------------------------

fn probe_wsl(
    distro: Option<&str>,
    agent_type: &str,
    mode: &str,
) -> Result<Probe, AgentConsoleError> {
    let launch = runtime_launch(AgentProviderSource::Wsl, distro, "bash")?;
    let script = crate::wsl_agent::shell_env::agent_cli_update_script();
    let mut args: Vec<&str> = vec!["-lc", &script, "tinto-cli-update", mode, agent_type];
    args.extend(npm_packages(agent_type));
    let result = if mode == "update" {
        run_with_timeout(&launch, &args)?
    } else {
        run(&launch, &args)?
    };
    let probe = parse_wsl_report(&result.stdout);
    if mode == "update" && !result.success {
        return Err(AgentConsoleError::new(
            "update_failed",
            failure_message(&result),
        ));
    }
    Ok(probe)
}

/// The WSL script reports `tinto:<key>=<value>` lines on stdout; installer
/// output goes to stderr so it cannot be mistaken for the report.
fn parse_wsl_report(stdout: &str) -> Probe {
    let mut probe = Probe::default();
    for line in stdout.lines() {
        let Some((key, value)) = line
            .strip_prefix("tinto:")
            .and_then(|rest| rest.split_once('='))
        else {
            continue;
        };
        let value = value.trim();
        let value_opt = (!value.is_empty()).then(|| value.to_string());
        match key {
            "path" => {
                probe.installed = value_opt.is_some();
                probe.binary = value_opt.map(PathBuf::from);
            }
            "version" => probe.version_line = value_opt,
            "package" => probe.package = value_opt,
            "latest" => probe.latest = value_opt,
            "method" => {
                probe.method = match value {
                    "npm" => Some(AgentCliUpdateMethod::Npm),
                    "self" => Some(AgentCliUpdateMethod::SelfUpdate),
                    _ => Some(AgentCliUpdateMethod::Unmanaged),
                }
            }
            _ => {}
        }
    }
    probe
}

// ---- shared helpers -------------------------------------------------------

fn run(launch: &ProcessLaunch, args: &[&str]) -> Result<ProcessResult, AgentConsoleError> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    run_process(
        launch,
        &args,
        PROBE_TIMEOUT.saturating_mul(3),
        &AtomicBool::new(false),
    )
}

fn run_with_timeout(
    launch: &ProcessLaunch,
    args: &[&str],
) -> Result<ProcessResult, AgentConsoleError> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    run_process(launch, &args, INSTALL_TIMEOUT, &AtomicBool::new(false))
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn npm_ls_lists(json: &str, package: &str) -> bool {
    json.contains(&format!("\"{package}\""))
}

fn failure_message(result: &ProcessResult) -> String {
    let last_line = |text: &str| -> Option<String> {
        text.lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .map(str::to_string)
    };
    let detail = last_line(&result.stderr)
        .or_else(|| last_line(&result.stdout))
        .unwrap_or_else(|| "el actualizador termino con error".to_string());
    format!("No se pudo actualizar: {detail}")
}

/// First `major.minor.patch` in a `--version` line, e.g. "codex-cli 0.158.0",
/// "2.1.285 (Claude Code)" or "opencode v2.0.18".
fn parse_version(text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|token| token.trim_matches('.'))
        .find(|token| {
            token.split('.').count() >= 3 && token.split('.').all(|part| !part.is_empty())
        })
        .map(str::to_string)
}

fn version_parts(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn version_is_newer(candidate: &str, current: &str) -> bool {
    version_parts(candidate) > version_parts(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions_from_each_cli_banner() {
        assert_eq!(
            parse_version("codex-cli 0.158.0").as_deref(),
            Some("0.158.0")
        );
        assert_eq!(
            parse_version("2.1.285 (Claude Code)").as_deref(),
            Some("2.1.285")
        );
        assert_eq!(parse_version("opencode v2.0.18").as_deref(), Some("2.0.18"));
        assert_eq!(parse_version("no version here"), None);
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(version_is_newer("0.160.1", "0.158.0"));
        assert!(version_is_newer("2.0.24", "2.0.18"));
        assert!(!version_is_newer("2.1.285", "2.1.285"));
        assert!(!version_is_newer("1.18.34", "2.0.18"));
    }

    #[test]
    fn wsl_report_ignores_installer_noise() {
        let probe = parse_wsl_report(
            "added 3 packages\ntinto:path=/home/me/.local/bin/claude\ntinto:version=2.1.285 (Claude Code)\ntinto:method=self\ntinto:package=\ntinto:latest=2.1.291\n",
        );
        assert!(probe.installed);
        assert_eq!(probe.method, Some(AgentCliUpdateMethod::SelfUpdate));
        assert_eq!(probe.package, None);
        let status = status_from_probe(AgentProviderSource::Wsl, Some("Ubuntu"), "claude", &probe);
        assert!(status.update_available);
        assert_eq!(status.installed_version.as_deref(), Some("2.1.285"));
        assert_eq!(status.latest_version.as_deref(), Some("2.1.291"));
        assert_eq!(status.command_display.as_deref(), Some("claude update"));
    }

    #[test]
    fn missing_or_app_managed_clis_offer_no_command() {
        let missing =
            status_from_probe(AgentProviderSource::Local, None, "kimi", &Probe::default());
        assert_eq!(missing.method, AgentCliUpdateMethod::Missing);
        assert!(!missing.update_available);

        let codex = Probe {
            installed: true,
            version_line: Some("codex-cli 0.158.0".into()),
            method: Some(AgentCliUpdateMethod::App),
            latest: Some("0.160.1".into()),
            ..Probe::default()
        };
        let status = status_from_probe(AgentProviderSource::Local, None, "codex", &codex);
        assert!(status.update_available);
        assert_eq!(status.command_display, None);
    }

    #[test]
    fn npm_status_names_the_installed_package() {
        let probe = Probe {
            installed: true,
            version_line: Some("opencode v2.0.18".into()),
            package: Some("@opencode/cli".into()),
            method: Some(AgentCliUpdateMethod::Npm),
            latest: Some("2.0.24".into()),
            ..Probe::default()
        };
        let status = status_from_probe(AgentProviderSource::Local, None, "opencode", &probe);
        assert_eq!(
            status.command_display.as_deref(),
            Some("npm install -g @opencode/cli@latest")
        );
        assert!(npm_ls_lists(
            r#"{"dependencies":{"@opencode/cli":{"version":"2.0.18"}}}"#,
            "@opencode/cli"
        ));
        assert!(!npm_ls_lists(r#"{"name":"lib"}"#, "@opencode/cli"));
    }

    #[test]
    fn recognizes_native_claude_and_app_managed_codex_on_windows() {
        assert_eq!(
            local_non_npm_method(
                "claude",
                Path::new("C:\\Users\\me\\.local\\bin\\claude.exe")
            ),
            AgentCliUpdateMethod::SelfUpdate
        );
        assert_eq!(
            local_non_npm_method(
                "codex",
                Path::new("C:\\Users\\me\\AppData\\Local\\Programs\\OpenAI\\Codex\\bin\\codex.exe")
            ),
            AgentCliUpdateMethod::App
        );
        assert_eq!(
            local_non_npm_method("opencode", Path::new("C:\\tools\\opencode.exe")),
            AgentCliUpdateMethod::SelfUpdate
        );
        assert_eq!(
            local_non_npm_method("kimi", Path::new("C:\\tools\\kimi.exe")),
            AgentCliUpdateMethod::Unmanaged
        );
    }
}
