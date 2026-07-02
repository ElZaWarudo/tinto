#[cfg(target_os = "windows")]
use std::collections::HashMap;
#[cfg(any(target_os = "windows", test))]
use std::collections::HashSet;
use std::io::Read;
#[cfg(target_os = "windows")]
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::{Child, ChildStdin, Command, Stdio};
#[cfg(target_os = "windows")]
use std::sync::mpsc;
#[cfg(target_os = "windows")]
use std::sync::Arc;
#[cfg(any(target_os = "windows", test))]
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
#[cfg(target_os = "windows")]
use std::time::Instant;

#[cfg(target_os = "windows")]
use crate::windows_process::hide_console;

use super::protocol::{
    encode_agent_request, encode_request, parse_agent_response_line, parse_response_line,
    AgentError, AgentErrorCategory, AgentRequest, AgentResponse, HandshakeRequest,
    HandshakeResponse, AGENT_VERSION,
};

pub const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
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
            "bash",
            [
                "-lc".to_string(),
                "exec cargo run --manifest-path \"$1\" --bin tinto-agent".to_string(),
                "tinto-agent-dev-source".to_string(),
                format!("{root}/src-tauri/Cargo.toml"),
            ],
        )
    }

    pub fn managed_wsl_agent() -> Self {
        Self::new(
            "bash",
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
        "--exec".to_string(),
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
    request_ubuntu_dev_source_with_timeout(request, DEFAULT_REQUEST_TIMEOUT)
}

pub fn request_ubuntu_agent(request: &AgentRequest) -> Result<AgentResponse, AgentError> {
    request_ubuntu_agent_with_timeout(request, DEFAULT_REQUEST_TIMEOUT)
}

pub fn request_wsl_agent(
    distro: &str,
    request: &AgentRequest,
) -> Result<AgentResponse, AgentError> {
    request_wsl_agent_with_timeout(distro, request, DEFAULT_REQUEST_TIMEOUT)
}

pub fn request_wsl_agent_with_timeout(
    distro: &str,
    request: &AgentRequest,
    timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    let distro = distro.trim();
    if distro.is_empty() {
        return Err(AgentError::new(
            AgentErrorCategory::MissingDistro,
            "no se configuro la distro WSL",
        ));
    }
    match packaged_agent_host_path() {
        Ok(agent_path) => {
            ensure_packaged_agent_installed(distro, &agent_path)?;
            let config = WslLaunchConfig::new(distro, AgentCommand::managed_wsl_agent());
            request_ubuntu_agent_config(&config, request, timeout)
        }
        Err(_error) if dev_source_fallback_enabled() => {
            request_wsl_dev_source_with_timeout(distro, request, timeout)
        }
        Err(error) => Err(error),
    }
}

pub fn request_ubuntu_agent_with_timeout(
    request: &AgentRequest,
    timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    request_wsl_agent_with_timeout(DEFAULT_SMOKE_DISTRO, request, timeout)
}

pub fn request_ubuntu_dev_source_with_timeout(
    request: &AgentRequest,
    timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    let config = ubuntu_dev_source_from_host()?;
    request_ubuntu_dev_source_config(&config, request, timeout)
}

fn request_wsl_dev_source_with_timeout(
    distro: &str,
    request: &AgentRequest,
    timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    let mut config = ubuntu_dev_source_from_host()?;
    config.distro = distro.to_string();
    request_ubuntu_dev_source_config(&config, request, timeout)
}

#[cfg(target_os = "windows")]
fn request_ubuntu_dev_source_config(
    config: &WslLaunchConfig,
    request: &AgentRequest,
    timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    request_with_persistent_agent(config, request, timeout)
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
    request_with_persistent_agent(config, request, timeout)
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

    let mut invalid_candidate = None;
    for candidate in packaged_agent_candidates(&exe_dir) {
        if candidate.is_file() {
            if is_linux_agent_artifact(&candidate) {
                return Ok(candidate);
            }
            invalid_candidate.get_or_insert(candidate);
        }
    }
    if let Some(candidate) = invalid_candidate {
        return Err(non_linux_packaged_agent_error(&candidate));
    }
    Err(missing_packaged_agent_error())
}

fn validate_packaged_agent_path(path: PathBuf) -> Result<PathBuf, AgentError> {
    if !path.is_file() {
        return Err(AgentError::new(
            AgentErrorCategory::MissingAgent,
            format!(
                "no se encontro el binario Linux de tinto-agent en {}",
                path.display()
            ),
        ));
    }

    if is_linux_agent_artifact(&path) {
        Ok(path)
    } else {
        Err(non_linux_packaged_agent_error(&path))
    }
}

fn is_linux_agent_artifact(path: &Path) -> bool {
    let mut magic = [0_u8; 4];
    match std::fs::File::open(path).and_then(|mut file| file.read_exact(&mut magic)) {
        Ok(()) => magic == *b"\x7FELF",
        Err(_) => false,
    }
}

fn non_linux_packaged_agent_error(path: &Path) -> AgentError {
    AgentError::new(
        AgentErrorCategory::MissingAgent,
        format!(
            "el binario configurado para tinto-agent WSL no es un ejecutable Linux ELF: {}",
            path.display()
        ),
    )
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

#[cfg(any(target_os = "windows", test))]
fn packaged_agent_install_cache() -> &'static Mutex<HashSet<String>> {
    static CACHE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashSet::new()))
}

#[cfg(any(target_os = "windows", test))]
fn packaged_agent_install_cache_key(distro: &str, source: &Path) -> String {
    format!(
        "{}\n{}\n{}\n{}",
        AGENT_VERSION,
        distro.trim(),
        source.to_string_lossy(),
        packaged_agent_source_fingerprint(source)
    )
}

#[cfg(any(target_os = "windows", test))]
fn packaged_agent_source_fingerprint(source: &Path) -> String {
    let Ok(metadata) = std::fs::metadata(source) else {
        return "missing".to_string();
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| format!("{}.{}", duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or_else(|| "unknown".to_string());
    format!("{}:{modified}", metadata.len())
}

#[cfg(target_os = "windows")]
fn ensure_packaged_agent_installed(distro: &str, source: &Path) -> Result<(), AgentError> {
    let key = packaged_agent_install_cache_key(distro, source);
    if packaged_agent_install_cache()
        .lock()
        .map(|cache| cache.contains(&key))
        .unwrap_or(false)
    {
        return Ok(());
    }

    install_packaged_agent(distro, source)?;
    if let Ok(mut cache) = packaged_agent_install_cache().lock() {
        cache.insert(key);
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn ensure_packaged_agent_installed(distro: &str, source: &Path) -> Result<(), AgentError> {
    install_packaged_agent(distro, source)
}

#[cfg(target_os = "windows")]
fn install_packaged_agent(distro: &str, source: &Path) -> Result<(), AgentError> {
    let argv = build_packaged_agent_install_argv(distro, source)?;
    let Some((program, args)) = argv.split_first() else {
        return Err(AgentError::new(
            AgentErrorCategory::SpawnFailed,
            "comando de instalacion WSL vacio",
        ));
    };
    if !source.is_file() {
        return Err(AgentError::new(
            AgentErrorCategory::MissingAgent,
            "no se pudo abrir el binario Linux empaquetado de tinto-agent",
        ));
    }

    let mut command = Command::new(program);
    let status = hide_console(
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
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
fn build_packaged_agent_install_argv(
    distro: &str,
    source: &Path,
) -> Result<Vec<String>, AgentError> {
    if distro.trim().is_empty() {
        return Err(AgentError::new(
            AgentErrorCategory::MissingDistro,
            "no se configuro la distro WSL",
        ));
    }
    let source = windows_path_to_wsl_mount(source)?;
    Ok(vec![
        "wsl.exe".to_string(),
        "-d".to_string(),
        distro.to_string(),
        "--exec".to_string(),
        "bash".to_string(),
        "-lc".to_string(),
        format!(
            "set -eu; install_dir=\"$HOME/.local/share/tinto/agents/{AGENT_VERSION}\"; dest=\"$install_dir/tinto-agent\"; mkdir -p \"$install_dir\"; if [ -f \"$dest\" ] && cmp -s -- \"$1\" \"$dest\"; then chmod 700 \"$dest\"; else tmp=\"$install_dir/.tinto-agent.$$.$RANDOM.tmp\"; trap 'rm -f \"$tmp\"' EXIT; cp -- \"$1\" \"$tmp\"; chmod 700 \"$tmp\"; mv -f -- \"$tmp\" \"$dest\"; trap - EXIT; fi"
        ),
        "tinto-agent-install".to_string(),
        source,
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
struct PooledAgent {
    child: Mutex<Child>,
    io: Mutex<()>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<mpsc::Receiver<Result<String, AgentError>>>,
}

#[cfg(target_os = "windows")]
fn pooled_agents() -> &'static Mutex<HashMap<String, Arc<PooledAgent>>> {
    static POOL: OnceLock<Mutex<HashMap<String, Arc<PooledAgent>>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(target_os = "windows")]
fn pooled_agent_key(argv: &[String]) -> String {
    argv.join("\0")
}

#[cfg(target_os = "windows")]
fn request_with_persistent_agent(
    config: &WslLaunchConfig,
    request: &AgentRequest,
    timeout: Duration,
) -> Result<AgentResponse, AgentError> {
    let argv = build_wsl_argv(config)?;
    let key = pooled_agent_key(&argv);
    let request_line = encode_agent_request(request)?;
    for attempt in 0..2 {
        let agent = pooled_agent(&key, &argv)?;
        match agent.exchange(&request_line, timeout) {
            Ok(line) => match parse_agent_response_line(&line) {
                Ok(response) => {
                    let retry = attempt == 0
                        && request_is_retry_safe(request)
                        && response_indicates_stale_agent(&response);
                    if retry {
                        drop_pooled_agent(&key, Some(&agent));
                        continue;
                    }
                    return Ok(response);
                }
                Err(error) => {
                    drop_pooled_agent(&key, Some(&agent));
                    return Err(error);
                }
            },
            Err(error) => {
                let retry = error.category == AgentErrorCategory::ChildExit
                    && attempt == 0
                    && request_is_retry_safe(request);
                drop_pooled_agent(&key, Some(&agent));
                if retry {
                    continue;
                }
                return Err(error);
            }
        }
    }
    Err(AgentError::new(
        AgentErrorCategory::ChildExit,
        "el agente WSL cerro stdout",
    ))
}

#[cfg(any(target_os = "windows", test))]
fn request_is_retry_safe(request: &AgentRequest) -> bool {
    matches!(
        request,
        AgentRequest::Handshake { .. }
            | AgentRequest::RepoSnapshot { .. }
            | AgentRequest::RepoSnapshotWithFsEvents { .. }
            | AgentRequest::ListDirectory { .. }
            | AgentRequest::WorktreeDiff { .. }
            | AgentRequest::CommitDiff { .. }
            | AgentRequest::CommitLog { .. }
            | AgentRequest::Blob { .. }
            | AgentRequest::FileContent { .. }
            | AgentRequest::MediaContent { .. }
            | AgentRequest::RepoTree { .. }
            | AgentRequest::GitReviewSummary { .. }
            | AgentRequest::GitleaksSetupStatus { .. }
            | AgentRequest::CreateGitleaksConfig { .. }
            | AgentRequest::CreateAgentsMdConfig { .. }
            | AgentRequest::AgentBinaryAvailable { .. }
            | AgentRequest::AgentCheckpointScan { .. }
    )
}

#[cfg(any(target_os = "windows", test))]
fn response_indicates_stale_agent(response: &AgentResponse) -> bool {
    matches!(
        response,
        AgentResponse::Error { category, message }
            if category == "malformed_response" && message == "mensaje invalido"
    )
}

#[cfg(target_os = "windows")]
fn pooled_agent(key: &str, argv: &[String]) -> Result<Arc<PooledAgent>, AgentError> {
    if let Ok(pool) = pooled_agents().lock() {
        if let Some(agent) = pool.get(key) {
            return Ok(Arc::clone(agent));
        }
    }

    let agent = Arc::new(start_pooled_agent(argv)?);
    let mut pool = pooled_agents()
        .lock()
        .map_err(|_| AgentError::new(AgentErrorCategory::ChildExit, "pool WSL bloqueado"))?;
    if let Some(existing) = pool.get(key) {
        agent.kill();
        return Ok(Arc::clone(existing));
    }
    pool.insert(key.to_string(), Arc::clone(&agent));
    Ok(agent)
}

#[cfg(target_os = "windows")]
fn start_pooled_agent(argv: &[String]) -> Result<PooledAgent, AgentError> {
    let Some((program, args)) = argv.split_first() else {
        return Err(AgentError::new(
            AgentErrorCategory::SpawnFailed,
            "comando WSL vacio",
        ));
    };

    let mut command = Command::new(program);
    let mut child = hide_console(
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .spawn()
    .map_err(|error| map_spawn_error(program, error))?;

    let stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AgentError::new(
                AgentErrorCategory::SpawnFailed,
                "stdin no disponible",
            ));
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AgentError::new(
                AgentErrorCategory::SpawnFailed,
                "stdout no disponible",
            ));
        }
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stdout);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    let _ = tx.send(Err(AgentError::new(
                        AgentErrorCategory::ChildExit,
                        "el agente WSL cerro stdout",
                    )));
                    return;
                }
                Ok(_) => {
                    if tx.send(Ok(line)).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    let _ = tx.send(Err(AgentError::new(
                        AgentErrorCategory::ChildExit,
                        "stdout cerrado",
                    )));
                    return;
                }
            }
        }
    });

    Ok(PooledAgent {
        child: Mutex::new(child),
        io: Mutex::new(()),
        stdin: Mutex::new(stdin),
        stdout: Mutex::new(rx),
    })
}

#[cfg(target_os = "windows")]
impl PooledAgent {
    fn exchange(&self, request_line: &str, timeout: Duration) -> Result<String, AgentError> {
        let _io = self
            .io
            .lock()
            .map_err(|_| AgentError::new(AgentErrorCategory::ChildExit, "agente bloqueado"))?;
        {
            let mut child = self
                .child
                .lock()
                .map_err(|_| AgentError::new(AgentErrorCategory::ChildExit, "agente bloqueado"))?;
            if let Some(status) = child.try_wait().map_err(|_| {
                AgentError::new(
                    AgentErrorCategory::ChildExit,
                    "no se pudo observar el agente",
                )
            })? {
                return Err(AgentError::new(
                    AgentErrorCategory::ChildExit,
                    format!("el agente WSL termino con {status}"),
                ));
            }
        }

        {
            let mut stdin = self
                .stdin
                .lock()
                .map_err(|_| AgentError::new(AgentErrorCategory::ChildExit, "stdin bloqueado"))?;
            stdin
                .write_all(request_line.as_bytes())
                .and_then(|_| stdin.flush())
                .map_err(|_| AgentError::new(AgentErrorCategory::ChildExit, "stdin cerrado"))?;
        }

        let stdout = self
            .stdout
            .lock()
            .map_err(|_| AgentError::new(AgentErrorCategory::ChildExit, "stdout bloqueado"))?;
        match stdout.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(AgentError::new(
                AgentErrorCategory::Timeout,
                "timeout esperando respuesta del agente WSL",
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(AgentError::new(
                AgentErrorCategory::ChildExit,
                "el agente WSL cerro la conexion",
            )),
        }
    }

    fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(target_os = "windows")]
fn drop_pooled_agent(key: &str, agent: Option<&Arc<PooledAgent>>) {
    if let Ok(mut pool) = pooled_agents().lock() {
        pool.remove(key);
    }
    if let Some(agent) = agent {
        agent.kill();
    }
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

        let mut command = Command::new(program);
        let mut child = hide_console(
            command
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null()),
        )
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
                "--exec",
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

        assert_eq!(&argv[..5], ["wsl.exe", "-d", "Ubuntu", "--exec", "bash"]);
        assert_eq!(argv[5], "-lc");
        assert!(argv[6].contains("cargo run"));
        assert_eq!(argv[7], "tinto-agent-dev-source");
        assert_eq!(argv[8], "/home/mayor/tinto/src-tauri/Cargo.toml");
    }

    #[test]
    fn managed_agent_command_uses_versioned_home_path() {
        let config = WslLaunchConfig::ubuntu_managed_agent();
        let argv = build_wsl_argv(&config).expect("argv");

        assert_eq!(&argv[..5], ["wsl.exe", "-d", "Ubuntu", "--exec", "bash"]);
        assert_eq!(argv[5], "-lc");
        assert!(argv[6].contains("$HOME/.local/share/tinto/agents/"));
        assert!(argv[6].contains("/tinto-agent"));
        assert!(argv[6].starts_with("exec "));
    }

    #[test]
    fn packaged_agent_install_command_copies_host_agent_from_wsl_mount() {
        let argv = build_packaged_agent_install_argv(
            "Ubuntu",
            Path::new("C:\\Program Files\\Tinto\\tinto-agent-linux-x86_64"),
        )
        .expect("argv");

        assert_eq!(&argv[..5], ["wsl.exe", "-d", "Ubuntu", "--exec", "bash"]);
        assert_eq!(argv[5], "-lc");
        assert!(argv[6].contains("mkdir -p \"$install_dir\""));
        assert!(argv[6].contains("dest=\"$install_dir/tinto-agent\""));
        assert!(argv[6].contains("cmp -s -- \"$1\" \"$dest\""));
        assert!(argv[6].contains("tmp=\"$install_dir/.tinto-agent.$$.$RANDOM.tmp\""));
        assert!(argv[6].contains("cp -- \"$1\" \"$tmp\""));
        assert!(argv[6].contains("mv -f -- \"$tmp\" \"$dest\""));
        assert!(argv[6].contains("chmod 700 \"$dest\""));
        assert!(!argv[6].contains("exit 0"));
        assert!(argv[6].contains("$HOME/.local/share/tinto/agents/"));
        assert_eq!(argv[7], "tinto-agent-install");
        assert_eq!(
            argv[8],
            "/mnt/c/Program Files/Tinto/tinto-agent-linux-x86_64"
        );
    }

    #[test]
    fn packaged_agent_install_cache_keys_by_version_distro_and_source() {
        let source = Path::new("C:\\Program Files\\Tinto\\tinto-agent-linux-x86_64");
        let key = packaged_agent_install_cache_key("Ubuntu", source);
        let same = packaged_agent_install_cache_key(" Ubuntu ", source);
        let other_distro = packaged_agent_install_cache_key("Debian", source);
        let other_source = packaged_agent_install_cache_key(
            "Ubuntu",
            Path::new("C:\\Program Files\\Tinto\\other-agent"),
        );

        assert_eq!(key, same);
        assert_ne!(key, other_distro);
        assert_ne!(key, other_source);

        let cache = packaged_agent_install_cache();
        let mut cache = cache.lock().expect("cache lock");
        cache.clear();
        assert!(cache.insert(key.clone()));
        assert!(cache.contains(&same));
        cache.clear();
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
    fn agent_requests_have_more_room_than_startup() {
        assert_eq!(DEFAULT_STARTUP_TIMEOUT, Duration::from_secs(10));
        assert_eq!(DEFAULT_REQUEST_TIMEOUT, Duration::from_secs(30));
        assert!(DEFAULT_REQUEST_TIMEOUT > DEFAULT_STARTUP_TIMEOUT);
    }

    #[test]
    fn packaged_agent_path_accepts_linux_elf_artifact() {
        let path = temp_agent_path("elf");
        std::fs::write(&path, b"\x7FELFplaceholder").expect("write artifact");

        let validated = validate_packaged_agent_path(path.clone()).expect("valid artifact");

        assert_eq!(validated, path);
        let _ = std::fs::remove_file(validated);
    }

    #[test]
    fn packaged_agent_path_rejects_non_linux_binary() {
        let path = temp_agent_path("pe");
        std::fs::write(&path, b"MZplaceholder").expect("write artifact");

        let error = validate_packaged_agent_path(path.clone()).expect_err("invalid artifact");

        assert_eq!(error.category, AgentErrorCategory::MissingAgent);
        assert!(error.message.contains("Linux ELF"));
        let _ = std::fs::remove_file(path);
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
    fn child_exit_retry_is_limited_to_read_only_requests() {
        let read_only = AgentRequest::RepoSnapshotWithFsEvents {
            protocol_version: PROTOCOL_VERSION,
            repos: vec!["/home/me/repo".into()],
            subscriptions: Vec::new(),
            fs_watch: Vec::new(),
            scope: crate::wsl_agent::protocol::RepoSnapshotScope::Everything,
        };
        let availability = AgentRequest::AgentBinaryAvailable {
            protocol_version: PROTOCOL_VERSION,
            agent_type: "codex".into(),
        };
        let mutating_delete = AgentRequest::DeleteFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: "/home/me/repo".into(),
            allowed_repos: vec!["/home/me/repo".into()],
            sources: vec!["old.txt".into()],
        };
        let idempotent_agents_md = AgentRequest::CreateAgentsMdConfig {
            protocol_version: PROTOCOL_VERSION,
            repo: "/home/me/repo".into(),
            allowed_repos: vec!["/home/me/repo".into()],
        };
        let mutating_checkpoint = AgentRequest::AgentCheckpointCreate {
            protocol_version: PROTOCOL_VERSION,
            repo: "/home/me/repo".into(),
            allowed_repos: vec!["/home/me/repo".into()],
            session_id: "sess".into(),
            created_at_ms: 1,
        };
        let mutating_checkpoint_file = AgentRequest::AgentCheckpointRevertFile {
            protocol_version: PROTOCOL_VERSION,
            allowed_repos: vec!["/home/me/repo".into()],
            checkpoint: crate::agent_console::checkpoint::CheckpointRecord {
                contract: crate::bus::contract::AgentSessionCheckpoint {
                    checkpoint_type: crate::bus::contract::AgentSessionCheckpointType::FsSnapshot,
                    git_hash: None,
                    snapshot_files: Vec::new(),
                },
                repo: "/home/me/repo".into(),
                session_id: "sess".into(),
                checkpoint_dir: "/tmp/tinto-checkpoint".into(),
                created_at_ms: 1,
            },
            path: "src/a.rs".into(),
        };
        let mutating_worktree = AgentRequest::CreateGitWorktree {
            protocol_version: PROTOCOL_VERSION,
            repo: "/home/me/repo".into(),
            allowed_repos: vec!["/home/me/repo".into()],
            session_id: "sess".into(),
        };

        assert!(request_is_retry_safe(&read_only));
        assert!(request_is_retry_safe(&availability));
        assert!(request_is_retry_safe(&idempotent_agents_md));
        assert!(!request_is_retry_safe(&mutating_delete));
        assert!(!request_is_retry_safe(&mutating_checkpoint));
        assert!(!request_is_retry_safe(&mutating_checkpoint_file));
        assert!(!request_is_retry_safe(&mutating_worktree));
    }

    #[test]
    fn stale_agent_error_is_detected_for_pool_restart() {
        assert!(response_indicates_stale_agent(&AgentResponse::Error {
            category: "malformed_response".into(),
            message: "mensaje invalido".into(),
        }));
        assert!(!response_indicates_stale_agent(&AgentResponse::Error {
            category: "repo-not-allowed".into(),
            message: "el repo no pertenece al workbench activo".into(),
        }));
    }

    #[test]
    fn windows_path_translation_uses_wsl_mount_shape() {
        let translated = windows_path_to_wsl_mount(std::path::Path::new("C:\\Users\\Mayor\\tinto"))
            .expect("translate");

        assert_eq!(translated, "/mnt/c/Users/Mayor/tinto");
    }

    fn temp_agent_path(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "tinto-agent-test-{}-{}-{label}",
            std::process::id(),
            nanos
        ))
    }
}
