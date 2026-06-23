#[cfg(target_os = "windows")]
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::{Command, Stdio};
#[cfg(target_os = "windows")]
use std::sync::mpsc;
use std::time::Duration;
#[cfg(target_os = "windows")]
use std::time::Instant;

use super::protocol::{
    encode_agent_request, encode_request, parse_agent_response_line, parse_response_line,
    AgentError, AgentErrorCategory, AgentRequest, AgentResponse, HandshakeRequest,
    HandshakeResponse, AGENT_VERSION,
};

pub const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(3);
pub const DEFAULT_SMOKE_DISTRO: &str = "Ubuntu";
pub const PACKAGED_AGENT_ENV: &str = "TINTO_WSL_AGENT_LINUX_BIN";
pub const DEV_SOURCE_FALLBACK_ENV: &str = "TINTO_WSL_AGENT_ALLOW_DEV_SOURCE";
pub const DEV_SOURCE_ROOT_ENV: &str = "TINTO_WSL_AGENT_ROOT_LINUX";
const PACKAGED_AGENT_FILENAME: &str = "tinto-agent-linux-x86_64";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslLaunchConfig {
    pub distro: String,
    pub agent_command: AgentCommand,
}

pub trait HandshakeTransport {
    fn exchange(
        &self,
        argv: &[String],
        request_line: &str,
        timeout: Duration,
    ) -> Result<String, AgentError>;
}

#[cfg(target_os = "windows")]
pub struct StdCommandTransport;

impl AgentCommand {
    pub fn new(program: impl Into<String>, args: impl IntoIterator<Item = String>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().collect(),
        }
    }

    pub fn dev_source(repo_root_linux_path: impl AsRef<str>) -> Self {
        let root = repo_root_linux_path.as_ref().trim_end_matches('/');
        Self::new(
            "cargo",
            [
                "run".to_string(),
                "--manifest-path".to_string(),
                format!("{root}/src-tauri/Cargo.toml"),
                "--bin".to_string(),
                "tinto-agent".to_string(),
            ],
        )
    }

    pub fn managed_wsl_agent() -> Self {
        Self::new(
            "sh",
            [
                "-lc".to_string(),
                format!("exec \"$HOME/.local/share/tinto/agents/{AGENT_VERSION}/tinto-agent\""),
            ],
        )
    }

    fn validate(&self) -> Result<(), AgentError> {
        if self.program.trim().is_empty() {
            return Err(AgentError::new(
                AgentErrorCategory::MissingAgent,
                "no se configuro el comando del agente",
            ));
        }
        Ok(())
    }
}

impl WslLaunchConfig {
    pub fn new(distro: impl Into<String>, agent_command: AgentCommand) -> Self {
        Self {
            distro: distro.into(),
            agent_command,
        }
    }

    pub fn ubuntu_dev_source(repo_root_linux_path: impl AsRef<str>) -> Self {
        Self::new(
            DEFAULT_SMOKE_DISTRO,
            AgentCommand::dev_source(repo_root_linux_path),
        )
    }

    pub fn ubuntu_managed_agent() -> Self {
        Self::new(DEFAULT_SMOKE_DISTRO, AgentCommand::managed_wsl_agent())
    }

    fn validate(&self) -> Result<(), AgentError> {
        if self.distro.trim().is_empty() {
            return Err(AgentError::new(
                AgentErrorCategory::MissingDistro,
                "no se configuro la distro WSL",
            ));
        }
        self.agent_command.validate()
    }
}

pub fn build_wsl_argv(config: &WslLaunchConfig) -> Result<Vec<String>, AgentError> {
    config.validate()?;
    let mut argv = vec![
        "wsl.exe".to_string(),
        "-d".to_string(),
        config.distro.clone(),
        "--".to_string(),
        config.agent_command.program.clone(),
    ];
    argv.extend(config.agent_command.args.iter().cloned());
    Ok(argv)
}

pub fn handshake_with_transport<T: HandshakeTransport>(
    config: &WslLaunchConfig,
    timeout: Duration,
    transport: &T,
) -> Result<HandshakeResponse, AgentError> {
    let argv = build_wsl_argv(config)?;
    let request = encode_request(&HandshakeRequest::current(AGENT_VERSION))?;
    let response_line = transport.exchange(&argv, &request, timeout)?;
    parse_response_line(&response_line)
}

pub fn request_with_transport<T: HandshakeTransport>(
    config: &WslLaunchConfig,
    request: &AgentRequest,
    timeout: Duration,
    transport: &T,
) -> Result<AgentResponse, AgentError> {
    let argv = build_wsl_argv(config)?;
    let request = encode_agent_request(request)?;
    let response_line = transport.exchange(&argv, &request, timeout)?;
    parse_agent_response_line(&response_line)
}

pub fn request_ubuntu_dev_source(request: &AgentRequest) -> Result<AgentResponse, AgentError> {
    request_ubuntu_dev_source_with_timeout(request, DEFAULT_STARTUP_TIMEOUT)
}

pub fn request_ubuntu_agent(request: &AgentRequest) -> Result<AgentResponse, AgentError> {
    request_ubuntu_agent_with_timeout(request, DEFAULT_STARTUP_TIMEOUT)
}

pub fn request_ubuntu_agent_with_timeout(
    request: &AgentRequest,
    timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    match packaged_agent_host_path() {
        Ok(agent_path) => {
            install_packaged_agent(DEFAULT_SMOKE_DISTRO, &agent_path)?;
            let config = WslLaunchConfig::ubuntu_managed_agent();
            request_ubuntu_agent_config(&config, request, timeout)
        }
        Err(_error) if dev_source_fallback_enabled() => {
            request_ubuntu_dev_source_with_timeout(request, timeout)
        }
        Err(error) => Err(error),
    }
}

pub fn request_ubuntu_dev_source_with_timeout(
    request: &AgentRequest,
    timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    let config = ubuntu_dev_source_from_host()?;
    request_ubuntu_dev_source_config(&config, request, timeout)
}

#[cfg(target_os = "windows")]
fn request_ubuntu_dev_source_config(
    config: &WslLaunchConfig,
    request: &AgentRequest,
    timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    let transport = StdCommandTransport;
    request_with_transport(config, request, timeout, &transport)
}

#[cfg(not(target_os = "windows"))]
fn request_ubuntu_dev_source_config(
    _config: &WslLaunchConfig,
    _request: &AgentRequest,
    _timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    Err(AgentError::new(
        AgentErrorCategory::MissingWsl,
        "WSL solo esta disponible en Windows",
    ))
}

#[cfg(target_os = "windows")]
fn request_ubuntu_agent_config(
    config: &WslLaunchConfig,
    request: &AgentRequest,
    timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    let transport = StdCommandTransport;
    request_with_transport(config, request, timeout, &transport)
}

#[cfg(not(target_os = "windows"))]
fn request_ubuntu_agent_config(
    _config: &WslLaunchConfig,
    _request: &AgentRequest,
    _timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    Err(AgentError::new(
        AgentErrorCategory::MissingWsl,
        "WSL solo esta disponible en Windows",
    ))
}

pub fn ubuntu_dev_source_from_host() -> Result<WslLaunchConfig, AgentError> {
    let root = std::env::var(DEV_SOURCE_ROOT_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(Ok)
        .unwrap_or_else(default_agent_root_linux_path)?;
    Ok(WslLaunchConfig::ubuntu_dev_source(root))
}

pub fn packaged_agent_host_path() -> Result<PathBuf, AgentError> {
    if let Some(path) = std::env::var_os(PACKAGED_AGENT_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return validate_packaged_agent_path(path);
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let Some(exe_dir) = exe_dir else {
        return Err(missing_packaged_agent_error());
    };

    for candidate in packaged_agent_candidates(&exe_dir) {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(missing_packaged_agent_error())
}

fn validate_packaged_agent_path(path: PathBuf) -> Result<PathBuf, AgentError> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(AgentError::new(
            AgentErrorCategory::MissingAgent,
            format!(
                "no se encontro el binario Linux de tinto-agent en {}",
                path.display()
            ),
        ))
    }
}

fn packaged_agent_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        exe_dir.join(PACKAGED_AGENT_FILENAME),
        exe_dir.join("resources").join(PACKAGED_AGENT_FILENAME),
        exe_dir
            .join("..")
            .join("Resources")
            .join(PACKAGED_AGENT_FILENAME),
    ]
}

fn missing_packaged_agent_error() -> AgentError {
    AgentError::new(
        AgentErrorCategory::MissingAgent,
        format!(
            "no se encontro el binario Linux empaquetado de tinto-agent; configura {PACKAGED_AGENT_ENV} o habilita {DEV_SOURCE_FALLBACK_ENV}=1 para desarrollo",
        ),
    )
}

fn dev_source_fallback_enabled() -> bool {
    dev_source_fallback_value_enabled(std::env::var(DEV_SOURCE_FALLBACK_ENV).ok().as_deref())
}

fn dev_source_fallback_value_enabled(value: Option<&str>) -> bool {
    value
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn install_packaged_agent(distro: &str, source: &Path) -> Result<(), AgentError> {
    let argv = build_packaged_agent_install_argv(distro)?;
    let Some((program, args)) = argv.split_first() else {
        return Err(AgentError::new(
            AgentErrorCategory::SpawnFailed,
            "comando de instalacion WSL vacio",
        ));
    };
    let agent_file = std::fs::File::open(source).map_err(|_| {
        AgentError::new(
            AgentErrorCategory::MissingAgent,
            "no se pudo abrir el binario Linux empaquetado de tinto-agent",
        )
    })?;

    let status = Command::new(program)
        .args(args)
        .stdin(Stdio::from(agent_file))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| map_spawn_error(program, error))?;
    if status.success() {
        Ok(())
    } else {
        Err(AgentError::new(
            AgentErrorCategory::SpawnFailed,
            "no se pudo instalar tinto-agent dentro de Ubuntu WSL",
        ))
    }
}

#[cfg(not(target_os = "windows"))]
fn install_packaged_agent(_distro: &str, _source: &Path) -> Result<(), AgentError> {
    Err(AgentError::new(
        AgentErrorCategory::MissingWsl,
        "WSL solo esta disponible en Windows",
    ))
}

#[cfg(any(target_os = "windows", test))]
fn build_packaged_agent_install_argv(distro: &str) -> Result<Vec<String>, AgentError> {
    if distro.trim().is_empty() {
        return Err(AgentError::new(
            AgentErrorCategory::MissingDistro,
            "no se configuro la distro WSL",
        ));
    }
    Ok(vec![
        "wsl.exe".to_string(),
        "-d".to_string(),
        distro.to_string(),
        "--".to_string(),
        "sh".to_string(),
        "-lc".to_string(),
        format!(
            "set -eu; install_dir=\"$HOME/.local/share/tinto/agents/{AGENT_VERSION}\"; mkdir -p \"$install_dir\"; cat > \"$install_dir/tinto-agent\"; chmod 700 \"$install_dir/tinto-agent\""
        ),
    ])
}

fn default_agent_root_linux_path() -> Result<String, AgentError> {
    let cwd = std::env::current_dir().map_err(|_| {
        AgentError::new(
            AgentErrorCategory::MissingAgent,
            "no se pudo resolver el directorio actual de Tinto",
        )
    })?;
    windows_path_to_wsl_mount(&cwd)
}

pub(crate) fn windows_path_to_wsl_mount(path: &std::path::Path) -> Result<String, AgentError> {
    let text = path.to_string_lossy().replace('\\', "/");
    let Some((drive, rest)) = text.split_once(":/") else {
        return Err(AgentError::new(
            AgentErrorCategory::MissingAgent,
            "configura TINTO_WSL_AGENT_ROOT_LINUX con el checkout Linux de Tinto",
        ));
    };
    let Some(letter) = drive.chars().next() else {
        return Err(AgentError::new(
            AgentErrorCategory::MissingAgent,
            "path de Windows invalido para el agente WSL",
        ));
    };
    Ok(format!("/mnt/{}/{}", letter.to_ascii_lowercase(), rest))
}

#[cfg(target_os = "windows")]
impl HandshakeTransport for StdCommandTransport {
    fn exchange(
        &self,
        argv: &[String],
        request_line: &str,
        timeout: Duration,
    ) -> Result<String, AgentError> {
        let Some((program, args)) = argv.split_first() else {
            return Err(AgentError::new(
                AgentErrorCategory::SpawnFailed,
                "comando WSL vacio",
            ));
        };

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| map_spawn_error(program, error))?;

        {
            let mut stdin = child.stdin.take().ok_or_else(|| {
                AgentError::new(AgentErrorCategory::SpawnFailed, "stdin no disponible")
            })?;
            stdin
                .write_all(request_line.as_bytes())
                .map_err(|_| AgentError::new(AgentErrorCategory::ChildExit, "stdin cerrado"))?;
        }

        let stdout = child.stdout.take().ok_or_else(|| {
            AgentError::new(AgentErrorCategory::SpawnFailed, "stdout no disponible")
        })?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = std::io::BufReader::new(stdout)
                .read_line(&mut line)
                .map(|_| line);
            let _ = tx.send(result);
        });

        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(result) = rx.try_recv() {
                let line = result.map_err(|_| {
                    AgentError::new(AgentErrorCategory::ChildExit, "stdout cerrado")
                })?;
                let status = child.wait().map_err(|_| {
                    AgentError::new(
                        AgentErrorCategory::ChildExit,
                        "no se pudo esperar al agente",
                    )
                })?;
                if !status.success() && line.trim().is_empty() {
                    return Err(AgentError::new(
                        AgentErrorCategory::ChildExit,
                        "el agente termino sin respuesta valida",
                    ));
                }
                return Ok(line);
            }

            if let Some(status) = child.try_wait().map_err(|_| {
                AgentError::new(
                    AgentErrorCategory::ChildExit,
                    "no se pudo observar el agente",
                )
            })? {
                if !status.success() {
                    return Err(AgentError::new(
                        AgentErrorCategory::ChildExit,
                        "el agente termino antes del handshake",
                    ));
                }
            }

            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AgentError::new(
                    AgentErrorCategory::Timeout,
                    "timeout esperando handshake del agente",
                ));
            }

            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[cfg(target_os = "windows")]
fn map_spawn_error(program: &str, error: std::io::Error) -> AgentError {
    if error.kind() == std::io::ErrorKind::NotFound {
        let category = if program.eq_ignore_ascii_case("wsl.exe") {
            AgentErrorCategory::MissingWsl
        } else {
            AgentErrorCategory::MissingAgent
        };
        return AgentError::new(category, "no se encontro el ejecutable requerido");
    }
    AgentError::new(
        AgentErrorCategory::SpawnFailed,
        "no se pudo iniciar el proceso WSL",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wsl_agent::protocol::{AgentStatus, PROTOCOL_VERSION};

    struct MockTransport {
        response: Result<String, AgentError>,
        expected_argv: Vec<String>,
        expected_type: &'static str,
    }

    impl HandshakeTransport for MockTransport {
        fn exchange(
            &self,
            argv: &[String],
            request_line: &str,
            _timeout: Duration,
        ) -> Result<String, AgentError> {
            assert_eq!(argv, self.expected_argv.as_slice());
            assert!(request_line.contains(&format!(r#""type":"{}""#, self.expected_type)));
            self.response.clone()
        }
    }

    #[test]
    fn builds_wsl_command_without_shell_interpolation() {
        let config = WslLaunchConfig::new(
            "Ubuntu",
            AgentCommand::new(
                "cargo",
                [
                    "run".to_string(),
                    "--bin".to_string(),
                    "tinto-agent".to_string(),
                    "arg with spaces".to_string(),
                ],
            ),
        );

        let argv = build_wsl_argv(&config).expect("argv");

        assert_eq!(
            argv,
            vec![
                "wsl.exe",
                "-d",
                "Ubuntu",
                "--",
                "cargo",
                "run",
                "--bin",
                "tinto-agent",
                "arg with spaces"
            ]
        );
        assert!(!argv.iter().any(|arg| arg.contains("cargo run")));
    }

    #[test]
    fn dev_source_command_uses_ubuntu_and_manifest_path() {
        let config = WslLaunchConfig::ubuntu_dev_source("/home/mayor/tinto/");
        let argv = build_wsl_argv(&config).expect("argv");

        assert_eq!(&argv[..5], ["wsl.exe", "-d", "Ubuntu", "--", "cargo"]);
        assert!(argv.contains(&"--manifest-path".to_string()));
        assert!(argv.contains(&"/home/mayor/tinto/src-tauri/Cargo.toml".to_string()));
        assert!(argv.contains(&"tinto-agent".to_string()));
    }

    #[test]
    fn managed_agent_command_uses_versioned_home_path() {
        let config = WslLaunchConfig::ubuntu_managed_agent();
        let argv = build_wsl_argv(&config).expect("argv");

        assert_eq!(&argv[..5], ["wsl.exe", "-d", "Ubuntu", "--", "sh"]);
        assert_eq!(argv[5], "-lc");
        assert!(argv[6].contains("$HOME/.local/share/tinto/agents/"));
        assert!(argv[6].contains("/tinto-agent"));
        assert!(argv[6].starts_with("exec "));
    }

    #[test]
    fn packaged_agent_install_command_uses_stdin_and_versioned_home_path() {
        let argv = build_packaged_agent_install_argv("Ubuntu").expect("argv");

        assert_eq!(&argv[..5], ["wsl.exe", "-d", "Ubuntu", "--", "sh"]);
        assert_eq!(argv[5], "-lc");
        assert!(argv[6].contains("mkdir -p \"$install_dir\""));
        assert!(argv[6].contains("cat > \"$install_dir/tinto-agent\""));
        assert!(argv[6].contains("chmod 700 \"$install_dir/tinto-agent\""));
        assert!(argv[6].contains("$HOME/.local/share/tinto/agents/"));
    }

    #[test]
    fn packaged_agent_candidates_are_app_relative() {
        let base = std::path::Path::new("C:\\Program Files\\Tinto");
        let candidates = packaged_agent_candidates(base);

        assert!(candidates
            .iter()
            .any(|path| path.ends_with(PACKAGED_AGENT_FILENAME)));
        assert!(candidates.iter().any(|path| path
            .to_string_lossy()
            .replace('\\', "/")
            .contains("resources/tinto-agent-linux-x86_64")));
    }

    #[test]
    fn dev_source_fallback_requires_explicit_truthy_value() {
        assert!(dev_source_fallback_value_enabled(Some("1")));
        assert!(dev_source_fallback_value_enabled(Some("true")));
        assert!(dev_source_fallback_value_enabled(Some(" YES ")));
        assert!(!dev_source_fallback_value_enabled(None));
        assert!(!dev_source_fallback_value_enabled(Some("0")));
        assert!(!dev_source_fallback_value_enabled(Some("false")));
    }

    #[test]
    fn missing_distro_and_agent_command_are_safe_errors() {
        let no_distro = WslLaunchConfig::new("", AgentCommand::new("cargo", []));
        let error = build_wsl_argv(&no_distro).expect_err("missing distro");
        assert_eq!(error.category, AgentErrorCategory::MissingDistro);

        let no_agent = WslLaunchConfig::new("Ubuntu", AgentCommand::new("", []));
        let error = build_wsl_argv(&no_agent).expect_err("missing agent");
        assert_eq!(error.category, AgentErrorCategory::MissingAgent);
    }

    #[test]
    fn handshake_accepts_mocked_compatible_response() {
        let config = WslLaunchConfig::ubuntu_dev_source("/repo");
        let expected_argv = build_wsl_argv(&config).expect("argv");
        let response = serde_json::json!({
            "type": "handshake",
            "protocol_version": PROTOCOL_VERSION,
            "agent_version": "0.1.0",
            "status": "ok"
        })
        .to_string()
            + "\n";
        let transport = MockTransport {
            response: Ok(response),
            expected_argv,
            expected_type: "handshake",
        };

        let handshake =
            handshake_with_transport(&config, DEFAULT_STARTUP_TIMEOUT, &transport).expect("ok");

        assert_eq!(handshake.status, AgentStatus::Ok);
        assert_eq!(handshake.protocol_version, PROTOCOL_VERSION);
    }

    #[test]
    fn handshake_maps_transport_and_protocol_failures() {
        let config = WslLaunchConfig::ubuntu_dev_source("/repo");
        let expected_argv = build_wsl_argv(&config).expect("argv");
        let transport = MockTransport {
            response: Err(AgentError::new(
                AgentErrorCategory::Timeout,
                "timeout esperando handshake",
            )),
            expected_argv,
            expected_type: "handshake",
        };
        let error = handshake_with_transport(&config, DEFAULT_STARTUP_TIMEOUT, &transport)
            .expect_err("err");
        assert_eq!(error.category, AgentErrorCategory::Timeout);

        let expected_argv = build_wsl_argv(&config).expect("argv");
        let transport = MockTransport {
            response: Ok("{ nope".into()),
            expected_argv,
            expected_type: "handshake",
        };
        let error = handshake_with_transport(&config, DEFAULT_STARTUP_TIMEOUT, &transport)
            .expect_err("err");
        assert_eq!(error.category, AgentErrorCategory::MalformedResponse);
    }

    #[test]
    fn request_with_transport_returns_typed_response() {
        let config = WslLaunchConfig::ubuntu_dev_source("/repo");
        let expected_argv = build_wsl_argv(&config).expect("argv");
        let response = serde_json::json!({
            "type": "worktree_diff",
            "diffs": []
        })
        .to_string()
            + "\n";
        let transport = MockTransport {
            response: Ok(response),
            expected_argv,
            expected_type: "worktree_diff",
        };

        let request = AgentRequest::WorktreeDiff {
            protocol_version: crate::wsl_agent::protocol::PROTOCOL_VERSION,
            repo: "/home/me/repo".into(),
            allowed_repos: vec!["/home/me/repo".into()],
        };
        let response =
            request_with_transport(&config, &request, DEFAULT_STARTUP_TIMEOUT, &transport)
                .expect("response");

        assert!(matches!(response, AgentResponse::WorktreeDiff { .. }));
    }

    #[test]
    fn windows_path_translation_uses_wsl_mount_shape() {
        let translated = windows_path_to_wsl_mount(std::path::Path::new("C:\\Users\\Mayor\\tinto"))
            .expect("translate");

        assert_eq!(translated, "/mnt/c/Users/Mayor/tinto");
    }
}
