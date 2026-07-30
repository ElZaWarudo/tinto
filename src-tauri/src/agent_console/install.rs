use std::{
    collections::HashMap,
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::bus::contract::{
    AgentInstallOutcome, AgentInstallOutcomeKind, AgentInstallPreview, AgentInstallPrivilege,
    AgentProviderSource, AgentSessionPermissionMode,
};

use super::{validation::validate_agent_type, AgentConsoleError};

pub const MAX_CAPTURE_BYTES: usize = 32 * 1024;
const ATTEMPT_TTL_MS: u64 = 10 * 60 * 1000;
const MAX_ATTEMPTS: usize = 32;
const INSTALL_TIMEOUT: Duration = Duration::from_secs(180);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct InstallRecipe {
    pub provider: &'static str,
    pub display_name: &'static str,
    pub package: &'static str,
    pub binary: &'static str,
    pub minimum_node: Option<(u64, u64, u64)>,
    pub revision: &'static str,
}

impl InstallRecipe {
    pub fn npm_args(&self) -> [&'static str; 3] {
        ["install", "-g", self.package]
    }

    pub fn display_command(&self) -> String {
        format!("npm install -g {}", self.package)
    }
}

pub fn recipe_for(provider: &str) -> Result<InstallRecipe, AgentConsoleError> {
    validate_agent_type(provider)?;
    match provider {
        "claude" => Ok(InstallRecipe {
            provider: "claude",
            display_name: "Claude Code",
            package: "@anthropic-ai/claude-code",
            binary: "claude",
            minimum_node: Some((22, 0, 0)),
            revision: "npm-v1",
        }),
        "codex" => Ok(InstallRecipe {
            provider: "codex",
            display_name: "Codex",
            package: "@openai/codex",
            binary: "codex",
            minimum_node: None,
            revision: "npm-v1",
        }),
        "kimi" => Ok(InstallRecipe {
            provider: "kimi",
            display_name: "Kimi Code",
            package: "@moonshot-ai/kimi-code",
            binary: "kimi",
            minimum_node: Some((22, 19, 0)),
            revision: "npm-v1",
        }),
        "opencode" => Ok(InstallRecipe {
            provider: "opencode",
            display_name: "OpenCode",
            package: "opencode-ai@latest",
            binary: "opencode",
            minimum_node: None,
            revision: "npm-v1",
        }),
        _ => unreachable!("provider validated above"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessLaunch {
    pub program: PathBuf,
    pub prefix_args: Vec<OsString>,
}

pub fn windows_npm_launcher_from_node(node: &Path) -> Result<ProcessLaunch, AgentConsoleError> {
    if !node.is_file()
        || !node
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("node.exe"))
    {
        return Err(AgentConsoleError::new(
            "missing_prerequisite",
            "no se pudo validar node.exe para ejecutar npm sin shell",
        ));
    }
    let npm_cli = node
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js");
    if !npm_cli.is_file() {
        return Err(AgentConsoleError::new(
            "missing_prerequisite",
            "no se pudo asociar node.exe con un npm-cli.js existente",
        ));
    }
    Ok(ProcessLaunch {
        program: node.to_path_buf(),
        prefix_args: vec![npm_cli.into_os_string()],
    })
}

fn local_npm_launcher() -> Result<ProcessLaunch, AgentConsoleError> {
    #[cfg(target_os = "windows")]
    {
        let node = which::which("node.exe").map_err(|_| {
            AgentConsoleError::new("missing_prerequisite", "Node.js no esta disponible")
        })?;
        windows_npm_launcher_from_node(&node)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let npm = which::which("npm").map_err(|_| {
            AgentConsoleError::new("missing_prerequisite", "npm no esta disponible")
        })?;
        Ok(ProcessLaunch {
            program: npm,
            prefix_args: Vec::new(),
        })
    }
}

fn runtime_launch(
    source: AgentProviderSource,
    distro: Option<&str>,
    executable: &str,
) -> Result<ProcessLaunch, AgentConsoleError> {
    match source {
        AgentProviderSource::Local if executable == "npm" => local_npm_launcher(),
        AgentProviderSource::Local => {
            let program = which::which(executable).map_err(|_| {
                AgentConsoleError::new(
                    "verification_failed",
                    format!("no se encontro {executable} despues de instalar"),
                )
            })?;
            Ok(ProcessLaunch {
                program,
                prefix_args: Vec::new(),
            })
        }
        AgentProviderSource::Wsl => {
            let distro = distro
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| AgentConsoleError::new("missing_distro", "repo WSL sin distro"))?;
            Ok(ProcessLaunch {
                program: PathBuf::from("wsl.exe"),
                prefix_args: vec![
                    "-d".into(),
                    distro.into(),
                    "--exec".into(),
                    executable.into(),
                ],
            })
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreparedInstall {
    pub repo: PathBuf,
    pub agent_type: String,
    pub permission_mode: AgentSessionPermissionMode,
    pub source: AgentProviderSource,
    pub distro: Option<String>,
    pub recipe: InstallRecipe,
}

#[derive(Debug, Clone)]
pub struct ClaimedInstall {
    pub repo: PathBuf,
    pub agent_type: String,
    pub permission_mode: AgentSessionPermissionMode,
    pub source: AgentProviderSource,
    pub distro: Option<String>,
    pub recipe: InstallRecipe,
    pub cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
enum AttemptState {
    Pending,
    Running,
    Launching,
}

#[derive(Debug, Clone)]
struct InstallAttempt {
    prepared: PreparedInstall,
    expires_at_ms: u64,
    state: AttemptState,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct AgentInstallRegistry {
    attempts: HashMap<String, InstallAttempt>,
}

impl AgentInstallRegistry {
    pub fn prepare(
        &mut self,
        prepared: PreparedInstall,
    ) -> Result<AgentInstallPreview, AgentConsoleError> {
        self.prune_expired();
        if self.attempts.len() >= MAX_ATTEMPTS {
            return Err(AgentConsoleError::new(
                "install_busy",
                "hay demasiadas instalaciones pendientes; cancela una antes de continuar",
            ));
        }
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let expires_at_ms = now_ms().saturating_add(ATTEMPT_TTL_MS);
        let recipe = prepared.recipe.clone();
        let preview = AgentInstallPreview {
            attempt_id: attempt_id.clone(),
            agent_type: prepared.agent_type.clone(),
            display_name: recipe.display_name.to_string(),
            source: prepared.source,
            distro: prepared.distro.clone(),
            installer: "npm".to_string(),
            command_display: recipe.display_command(),
            arguments: recipe.npm_args().into_iter().map(str::to_string).collect(),
            global_effect: "Instala el agente globalmente para el usuario del runtime seleccionado"
                .to_string(),
            privilege: AgentInstallPrivilege::None,
            recipe_revision: recipe.revision.to_string(),
            expires_at_ms,
        };
        self.attempts.insert(
            attempt_id,
            InstallAttempt {
                prepared,
                expires_at_ms,
                state: AttemptState::Pending,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        );
        Ok(preview)
    }

    pub fn claim(&mut self, attempt_id: &str) -> Result<ClaimedInstall, AgentConsoleError> {
        self.prune_expired();
        let attempt = self.attempts.get_mut(attempt_id).ok_or_else(|| {
            AgentConsoleError::new("install_attempt_expired", "la autorizacion ya no es valida")
        })?;
        if !matches!(attempt.state, AttemptState::Pending) {
            return Err(AgentConsoleError::new(
                "install_attempt_consumed",
                "la instalacion ya fue confirmada",
            ));
        }
        attempt.state = AttemptState::Running;
        Ok(ClaimedInstall {
            repo: attempt.prepared.repo.clone(),
            agent_type: attempt.prepared.agent_type.clone(),
            permission_mode: attempt.prepared.permission_mode,
            source: attempt.prepared.source,
            distro: attempt.prepared.distro.clone(),
            recipe: attempt.prepared.recipe.clone(),
            cancelled: Arc::clone(&attempt.cancelled),
        })
    }

    pub fn cancel(&mut self, attempt_id: &str) -> Result<(), AgentConsoleError> {
        let attempt = self.attempts.get(attempt_id).ok_or_else(|| {
            AgentConsoleError::new("install_attempt_expired", "la autorizacion ya no es valida")
        })?;
        match attempt.state {
            AttemptState::Pending => {
                attempt.cancelled.store(true, Ordering::SeqCst);
                self.attempts.remove(attempt_id);
            }
            AttemptState::Running => attempt.cancelled.store(true, Ordering::SeqCst),
            AttemptState::Launching => {
                return Err(AgentConsoleError::new(
                    "install_attempt_consumed",
                    "la instalacion ya termino y la sesion se esta iniciando",
                ));
            }
        }
        Ok(())
    }

    pub fn begin_launch(&mut self, attempt_id: &str) -> Result<bool, AgentConsoleError> {
        let attempt = self.attempts.get_mut(attempt_id).ok_or_else(|| {
            AgentConsoleError::new("install_attempt_expired", "la autorizacion ya no es valida")
        })?;
        if !matches!(attempt.state, AttemptState::Running) {
            return Err(AgentConsoleError::new(
                "install_attempt_consumed",
                "la instalacion ya fue procesada",
            ));
        }
        if attempt.cancelled.load(Ordering::SeqCst) {
            return Ok(false);
        }
        attempt.state = AttemptState::Launching;
        Ok(true)
    }

    pub fn finish(&mut self, attempt_id: &str) {
        self.attempts.remove(attempt_id);
    }

    fn prune_expired(&mut self) {
        let now = now_ms();
        self.attempts.retain(|_, attempt| {
            !matches!(attempt.state, AttemptState::Pending) || attempt.expires_at_ms > now
        });
    }
}

pub fn preflight_install(prepared: &PreparedInstall) -> Result<(), AgentConsoleError> {
    preflight_install_with(&SystemProcessRunner, &SystemRuntimeLauncher, prepared)
}

fn preflight_install_with(
    runner: &impl InstallProcessRunner,
    launcher: &impl InstallRuntimeLauncher,
    prepared: &PreparedInstall,
) -> Result<(), AgentConsoleError> {
    let npm = launcher.launch(prepared.source, prepared.distro.as_deref(), "npm")?;
    let npm_result = runner.run(
        &npm,
        &["--version".into()],
        PROBE_TIMEOUT,
        &AtomicBool::new(false),
    )?;
    if !npm_result.success {
        return Err(AgentConsoleError::new(
            "missing_prerequisite",
            "npm no esta disponible en el runtime seleccionado",
        ));
    }
    if let Some(minimum) = prepared.recipe.minimum_node {
        let node = launcher
            .launch(prepared.source, prepared.distro.as_deref(), "node")
            .map_err(|_| {
                AgentConsoleError::new(
                    "missing_prerequisite",
                    "Node.js no esta disponible en el runtime seleccionado",
                )
            })?;
        let result = runner.run(
            &node,
            &["--version".into()],
            PROBE_TIMEOUT,
            &AtomicBool::new(false),
        )?;
        let version = parse_node_version(&result.stdout).ok_or_else(|| {
            AgentConsoleError::new(
                "missing_prerequisite",
                "no se pudo validar la version de Node.js",
            )
        })?;
        if version < minimum {
            return Err(AgentConsoleError::new(
                "missing_prerequisite",
                format!(
                    "{} requiere Node.js {}.{}.{} o posterior",
                    prepared.recipe.display_name, minimum.0, minimum.1, minimum.2
                ),
            ));
        }
    }
    Ok(())
}

pub fn execute_install(claimed: &ClaimedInstall) -> AgentInstallOutcome {
    match execute_install_inner(claimed) {
        Ok((version, detail)) => AgentInstallOutcome {
            outcome: AgentInstallOutcomeKind::Verified,
            verified_version: Some(version),
            session_id: None,
            message: detail,
        },
        Err(error) => AgentInstallOutcome {
            outcome: outcome_for_category(&error.category),
            verified_version: None,
            session_id: None,
            message: error.message,
        },
    }
}

fn execute_install_inner(claimed: &ClaimedInstall) -> Result<(String, String), AgentConsoleError> {
    execute_install_inner_with(&SystemProcessRunner, &SystemRuntimeLauncher, claimed)
}

fn execute_install_inner_with(
    runner: &impl InstallProcessRunner,
    launcher: &impl InstallRuntimeLauncher,
    claimed: &ClaimedInstall,
) -> Result<(String, String), AgentConsoleError> {
    let npm = launcher.launch(claimed.source, claimed.distro.as_deref(), "npm")?;
    let args = claimed
        .recipe
        .npm_args()
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    let installed = runner.run(&npm, &args, INSTALL_TIMEOUT, &claimed.cancelled)?;
    if !installed.success {
        if is_permission_failure(&installed) {
            return Err(AgentConsoleError::new(
                "missing_prerequisite",
                safe_failure_message(
                    "npm no puede escribir en el prefijo global; configura un prefijo de usuario",
                    &installed,
                ),
            ));
        }
        return Err(AgentConsoleError::new(
            "installer_failed",
            safe_failure_message("npm no pudo instalar el agente", &installed),
        ));
    }
    let binary = launcher.launch(
        claimed.source,
        claimed.distro.as_deref(),
        claimed.recipe.binary,
    )?;
    let verified = runner.run(
        &binary,
        &[OsString::from("--version")],
        PROBE_TIMEOUT,
        &claimed.cancelled,
    )?;
    if !verified.success {
        return Err(AgentConsoleError::new(
            "verification_failed",
            safe_failure_message("el agente instalado no supero la verificacion", &verified),
        ));
    }
    let version = verified
        .stdout
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if version.is_empty() {
        return Err(AgentConsoleError::new(
            "verification_failed",
            "el agente no devolvio una version verificable",
        ));
    }
    Ok((version, "Instalacion verificada".to_string()))
}

fn is_permission_failure(result: &ProcessResult) -> bool {
    let output = format!("{}\n{}", result.stdout, result.stderr).to_ascii_lowercase();
    ["eacces", "eperm", "permission denied", "access is denied"]
        .iter()
        .any(|needle| output.contains(needle))
}

fn outcome_for_category(category: &str) -> AgentInstallOutcomeKind {
    match category {
        "missing_prerequisite" => AgentInstallOutcomeKind::MissingPrerequisite,
        "install_cancelled" => AgentInstallOutcomeKind::Cancelled,
        "install_timeout" => AgentInstallOutcomeKind::Timeout,
        "installer_spawn_failed" => AgentInstallOutcomeKind::SpawnFailed,
        "verification_failed" => AgentInstallOutcomeKind::VerificationFailed,
        "cleanup_failed" => AgentInstallOutcomeKind::CleanupFailed,
        _ => AgentInstallOutcomeKind::InstallerFailed,
    }
}

#[derive(Debug)]
struct ProcessResult {
    success: bool,
    stdout: String,
    stderr: String,
}

trait InstallProcessRunner {
    fn run(
        &self,
        launch: &ProcessLaunch,
        args: &[OsString],
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<ProcessResult, AgentConsoleError>;
}

trait InstallRuntimeLauncher {
    fn launch(
        &self,
        source: AgentProviderSource,
        distro: Option<&str>,
        executable: &str,
    ) -> Result<ProcessLaunch, AgentConsoleError>;
}

struct SystemProcessRunner;
struct SystemRuntimeLauncher;

impl InstallRuntimeLauncher for SystemRuntimeLauncher {
    fn launch(
        &self,
        source: AgentProviderSource,
        distro: Option<&str>,
        executable: &str,
    ) -> Result<ProcessLaunch, AgentConsoleError> {
        runtime_launch(source, distro, executable)
    }
}

impl InstallProcessRunner for SystemProcessRunner {
    fn run(
        &self,
        launch: &ProcessLaunch,
        args: &[OsString],
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<ProcessResult, AgentConsoleError> {
        run_process(launch, args, timeout, cancelled)
    }
}

fn run_process(
    launch: &ProcessLaunch,
    args: &[OsString],
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<ProcessResult, AgentConsoleError> {
    let mut command = Command::new(&launch.program);
    apply_install_environment(&mut command);
    command
        .args(&launch.prefix_args)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(target_os = "windows")]
    crate::windows_process::hide_console(&mut command);
    let mut child = command.spawn().map_err(|_| {
        AgentConsoleError::new("installer_spawn_failed", "no se pudo iniciar el instalador")
    })?;
    #[cfg(target_os = "windows")]
    let containment = crate::windows_process::KillOnCloseJob::attach(&child).map_err(|_| {
        let _ = child.kill();
        let _ = child.wait();
        AgentConsoleError::new(
            "cleanup_failed",
            "no se pudo contener el proceso instalador",
        )
    })?;
    let stdout = read_capped(child.stdout.take());
    let stderr = read_capped(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        if cancelled.load(Ordering::SeqCst) {
            terminate_process_tree(&mut child)?;
            let _ = child.wait();
            return Err(AgentConsoleError::new(
                "install_cancelled",
                "instalacion cancelada",
            ));
        }
        if started.elapsed() >= timeout {
            terminate_process_tree(&mut child)?;
            let _ = child.wait();
            return Err(AgentConsoleError::new(
                "install_timeout",
                "la instalacion excedio el tiempo permitido",
            ));
        }
        if let Some(status) = child.try_wait().map_err(|_| {
            AgentConsoleError::new("cleanup_failed", "no se pudo observar el instalador")
        })? {
            break status;
        }
        thread::sleep(Duration::from_millis(50));
    };
    #[cfg(target_os = "windows")]
    drop(containment);
    #[cfg(unix)]
    terminate_remaining_process_group(child.id());
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok(ProcessResult {
        success: status.success(),
        stdout: sanitize_output(&stdout),
        stderr: sanitize_output(&stderr),
    })
}

fn apply_install_environment(command: &mut Command) {
    const ALLOWED_ENVIRONMENT: &[&str] = &[
        "PATH",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USERPROFILE",
        "HOME",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "LANG",
        "LC_ALL",
    ];

    command.env_clear();
    for name in ALLOWED_ENVIRONMENT {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}

fn read_capped<R: Read + Send + 'static>(reader: Option<R>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        if let Some(mut reader) = reader {
            let mut buffer = [0_u8; 4096];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                let remaining = MAX_CAPTURE_BYTES.saturating_sub(output.len());
                output.extend_from_slice(&buffer[..read.min(remaining)]);
            }
        }
        output
    })
}

#[cfg(target_os = "windows")]
fn terminate_process_tree(child: &mut std::process::Child) -> Result<(), AgentConsoleError> {
    child
        .kill()
        .map_err(|_| AgentConsoleError::new("cleanup_failed", "no se pudo detener el instalador"))
}

#[cfg(unix)]
fn terminate_process_tree(child: &mut std::process::Child) -> Result<(), AgentConsoleError> {
    let group = format!("-{}", child.id());
    let term_sent = Command::new("kill")
        .args(["-TERM", "--", &group])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if term_sent {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    let kill_sent = Command::new("kill")
        .args(["-KILL", "--", &group])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !kill_sent && child.kill().is_err() {
        return Err(AgentConsoleError::new(
            "cleanup_failed",
            "no se pudo detener el grupo del instalador",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn terminate_remaining_process_group(process_group: u32) {
    let group = format!("-{process_group}");
    let _ = Command::new("kill")
        .args(["-KILL", "--", &group])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub fn sanitize_output(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(&raw[..raw.len().min(MAX_CAPTURE_BYTES)]);
    text.lines()
        .take(12)
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if [
                "token=",
                "npm_token",
                "_authtoken",
                "password=",
                "secret=",
                "api_key=",
                "authorization:",
                "bearer ",
            ]
            .iter()
            .any(|needle| lower.contains(needle))
                || contains_url_userinfo(&lower)
            {
                "[redacted]".to_string()
            } else {
                line.chars().take(512).collect()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn contains_url_userinfo(line: &str) -> bool {
    let Some((_, after_scheme)) = line.split_once("://") else {
        return false;
    };
    after_scheme
        .split('/')
        .next()
        .is_some_and(|authority| authority.contains('@'))
}

fn safe_failure_message(prefix: &str, result: &ProcessResult) -> String {
    let detail = if result.stderr.trim().is_empty() {
        result.stdout.trim()
    } else {
        result.stderr.trim()
    };
    if detail.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}: {detail}")
    }
}

fn parse_node_version(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.trim().trim_start_matches('v');
    let mut parts = value.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.split('-').next()?.parse().ok()?,
    ))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, sync::Mutex};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedCall {
        program: PathBuf,
        args: Vec<OsString>,
    }

    struct FakeProcessRunner {
        results: Mutex<VecDeque<Result<ProcessResult, AgentConsoleError>>>,
        calls: Mutex<Vec<RecordedCall>>,
    }

    struct FakeRuntimeLauncher;

    impl InstallRuntimeLauncher for FakeRuntimeLauncher {
        fn launch(
            &self,
            source: AgentProviderSource,
            distro: Option<&str>,
            executable: &str,
        ) -> Result<ProcessLaunch, AgentConsoleError> {
            match source {
                AgentProviderSource::Local => Ok(ProcessLaunch {
                    program: PathBuf::from(format!("fake-{executable}")),
                    prefix_args: Vec::new(),
                }),
                AgentProviderSource::Wsl => Ok(ProcessLaunch {
                    program: PathBuf::from("wsl.exe"),
                    prefix_args: vec![
                        "-d".into(),
                        distro.unwrap_or_default().into(),
                        "--exec".into(),
                        executable.into(),
                    ],
                }),
            }
        }
    }

    impl FakeProcessRunner {
        fn new(results: Vec<Result<ProcessResult, AgentConsoleError>>) -> Self {
            Self {
                results: Mutex::new(results.into()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<RecordedCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl InstallProcessRunner for FakeProcessRunner {
        fn run(
            &self,
            launch: &ProcessLaunch,
            args: &[OsString],
            _timeout: Duration,
            _cancelled: &AtomicBool,
        ) -> Result<ProcessResult, AgentConsoleError> {
            let mut full_args = launch.prefix_args.clone();
            full_args.extend_from_slice(args);
            self.calls.lock().unwrap().push(RecordedCall {
                program: launch.program.clone(),
                args: full_args,
            });
            self.results
                .lock()
                .unwrap()
                .pop_front()
                .expect("fake process result")
        }
    }

    fn process_result(success: bool, stdout: &str, stderr: &str) -> ProcessResult {
        ProcessResult {
            success,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn recipes_are_compiled_and_shell_free_for_every_provider() {
        let cases = [
            ("claude", "@anthropic-ai/claude-code"),
            ("codex", "@openai/codex"),
            ("kimi", "@moonshot-ai/kimi-code"),
            ("opencode", "opencode-ai@latest"),
        ];
        for (provider, package) in cases {
            let recipe = recipe_for(provider).unwrap();
            assert_eq!(recipe.package, package);
            assert_eq!(recipe.npm_args(), ["install", "-g", package]);
            assert!(!recipe.display_command().contains('|'));
            assert!(!recipe.display_command().contains("cmd.exe"));
            assert!(!recipe.display_command().contains("powershell"));
        }
    }

    #[test]
    fn windows_launcher_requires_a_paired_node_and_npm_cli() {
        let root = tempfile::tempdir().unwrap();
        let node = root.path().join("node.exe");
        let npm_cli = root.path().join("node_modules/npm/bin/npm-cli.js");
        std::fs::create_dir_all(npm_cli.parent().unwrap()).unwrap();
        std::fs::write(&node, b"fake").unwrap();
        std::fs::write(&npm_cli, b"fake").unwrap();
        let launcher = windows_npm_launcher_from_node(&node).unwrap();
        assert_eq!(launcher.program, node);
        assert_eq!(PathBuf::from(&launcher.prefix_args[0]), npm_cli);
    }

    #[test]
    fn single_use_attempts_reject_concurrent_or_replayed_confirmation() {
        let mut registry = AgentInstallRegistry::default();
        let preview = registry
            .prepare(PreparedInstall {
                repo: PathBuf::from("C:/repo"),
                agent_type: "codex".into(),
                permission_mode: AgentSessionPermissionMode::Workspace,
                source: AgentProviderSource::Local,
                distro: None,
                recipe: recipe_for("codex").unwrap(),
            })
            .unwrap();
        let claimed = registry.claim(&preview.attempt_id).unwrap();
        assert_eq!(claimed.agent_type, "codex");
        assert_eq!(
            registry.claim(&preview.attempt_id).unwrap_err().category,
            "install_attempt_consumed"
        );
    }

    #[test]
    fn cancellation_marks_a_claimed_attempt_for_process_cleanup() {
        let mut registry = AgentInstallRegistry::default();
        let preview = registry
            .prepare(PreparedInstall {
                repo: PathBuf::from("/repo"),
                agent_type: "kimi".into(),
                permission_mode: AgentSessionPermissionMode::Workspace,
                source: AgentProviderSource::Wsl,
                distro: Some("Ubuntu-24.04".into()),
                recipe: recipe_for("kimi").unwrap(),
            })
            .unwrap();
        let claimed = registry.claim(&preview.attempt_id).unwrap();
        registry.cancel(&preview.attempt_id).unwrap();
        assert!(claimed.cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn cancellation_wins_before_launch_and_is_rejected_after_launch_begins() {
        let mut registry = AgentInstallRegistry::default();
        let first = registry
            .prepare(PreparedInstall {
                repo: PathBuf::from("/repo"),
                agent_type: "codex".into(),
                permission_mode: AgentSessionPermissionMode::Workspace,
                source: AgentProviderSource::Local,
                distro: None,
                recipe: recipe_for("codex").unwrap(),
            })
            .unwrap();
        registry.claim(&first.attempt_id).unwrap();
        registry.cancel(&first.attempt_id).unwrap();
        assert!(!registry.begin_launch(&first.attempt_id).unwrap());

        let second = registry
            .prepare(PreparedInstall {
                repo: PathBuf::from("/repo"),
                agent_type: "codex".into(),
                permission_mode: AgentSessionPermissionMode::FullAccess,
                source: AgentProviderSource::Local,
                distro: None,
                recipe: recipe_for("codex").unwrap(),
            })
            .unwrap();
        let claimed = registry.claim(&second.attempt_id).unwrap();
        assert_eq!(
            claimed.permission_mode,
            AgentSessionPermissionMode::FullAccess
        );
        assert!(registry.begin_launch(&second.attempt_id).unwrap());
        assert_eq!(
            registry.cancel(&second.attempt_id).unwrap_err().category,
            "install_attempt_consumed"
        );
    }

    #[test]
    fn expired_attempts_cannot_be_claimed() {
        let mut registry = AgentInstallRegistry::default();
        let preview = registry
            .prepare(PreparedInstall {
                repo: PathBuf::from("/repo"),
                agent_type: "codex".into(),
                permission_mode: AgentSessionPermissionMode::Workspace,
                source: AgentProviderSource::Local,
                distro: None,
                recipe: recipe_for("codex").unwrap(),
            })
            .unwrap();
        registry
            .attempts
            .get_mut(&preview.attempt_id)
            .unwrap()
            .expires_at_ms = 0;

        assert_eq!(
            registry.claim(&preview.attempt_id).unwrap_err().category,
            "install_attempt_expired"
        );
    }

    #[test]
    fn output_sanitization_is_bounded_and_redacts_secret_like_values() {
        let raw = format!(
            "token=secret-value\nhttps://user:password@registry.example.test/pkg\n{}",
            "x".repeat(MAX_CAPTURE_BYTES * 2)
        );
        let safe = sanitize_output(raw.as_bytes());
        assert!(!safe.contains("secret-value"));
        assert!(!safe.contains("user:password"));
        assert!(safe.len() <= MAX_CAPTURE_BYTES + 64);
    }

    #[test]
    fn node_versions_are_compared_numerically() {
        assert_eq!(parse_node_version("v22.19.0\n"), Some((22, 19, 0)));
        assert!(parse_node_version("v22.9.0").unwrap() < (22, 19, 0));
    }

    #[test]
    fn fake_wsl_runner_keeps_preflight_install_and_verification_in_one_distro() {
        let prepared = PreparedInstall {
            repo: PathBuf::from("/repo"),
            agent_type: "kimi".into(),
            permission_mode: AgentSessionPermissionMode::Workspace,
            source: AgentProviderSource::Wsl,
            distro: Some("Ubuntu-24.04".into()),
            recipe: recipe_for("kimi").unwrap(),
        };
        let preflight = FakeProcessRunner::new(vec![
            Ok(process_result(true, "10.9.0", "")),
            Ok(process_result(true, "v22.19.0", "")),
        ]);
        preflight_install_with(&preflight, &FakeRuntimeLauncher, &prepared).unwrap();
        let preflight_calls = preflight.calls();
        assert_eq!(preflight_calls.len(), 2);
        for call in &preflight_calls {
            assert_eq!(call.program, PathBuf::from("wsl.exe"));
            assert_eq!(&call.args[..3], ["-d", "Ubuntu-24.04", "--exec"]);
        }
        assert_eq!(preflight_calls[0].args[3], "npm");
        assert_eq!(preflight_calls[1].args[3], "node");

        let claimed = ClaimedInstall {
            repo: prepared.repo,
            agent_type: prepared.agent_type,
            permission_mode: prepared.permission_mode,
            source: prepared.source,
            distro: prepared.distro,
            recipe: prepared.recipe,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let execution = FakeProcessRunner::new(vec![
            Ok(process_result(true, "installed", "")),
            Ok(process_result(true, "kimi 1.2.3", "")),
        ]);
        let (version, _) =
            execute_install_inner_with(&execution, &FakeRuntimeLauncher, &claimed).unwrap();
        assert_eq!(version, "kimi 1.2.3");
        let calls = execution.calls();
        assert_eq!(
            calls[0].args,
            [
                "-d",
                "Ubuntu-24.04",
                "--exec",
                "npm",
                "install",
                "-g",
                "@moonshot-ai/kimi-code"
            ]
            .map(OsString::from)
        );
        assert_eq!(
            calls[1].args,
            ["-d", "Ubuntu-24.04", "--exec", "kimi", "--version"].map(OsString::from)
        );
    }

    #[test]
    fn fake_runner_maps_global_prefix_permission_failures_to_manual_guidance() {
        let claimed = ClaimedInstall {
            repo: PathBuf::from("/repo"),
            agent_type: "codex".into(),
            permission_mode: AgentSessionPermissionMode::Workspace,
            source: AgentProviderSource::Wsl,
            distro: Some("Ubuntu".into()),
            recipe: recipe_for("codex").unwrap(),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let runner =
            FakeProcessRunner::new(vec![Ok(process_result(false, "", "npm ERR! code EACCES"))]);

        let error =
            execute_install_inner_with(&runner, &FakeRuntimeLauncher, &claimed).unwrap_err();
        assert_eq!(error.category, "missing_prerequisite");
        assert!(error.message.contains("prefijo global"));
    }

    #[test]
    fn fake_local_runner_uses_exact_compiled_argv_without_a_shell() {
        let prepared = PreparedInstall {
            repo: PathBuf::from("/repo"),
            agent_type: "codex".into(),
            permission_mode: AgentSessionPermissionMode::Workspace,
            source: AgentProviderSource::Local,
            distro: None,
            recipe: recipe_for("codex").unwrap(),
        };
        let preflight = FakeProcessRunner::new(vec![Ok(process_result(true, "10.9.0", ""))]);
        preflight_install_with(&preflight, &FakeRuntimeLauncher, &prepared).unwrap();
        assert_eq!(preflight.calls()[0].program, PathBuf::from("fake-npm"));

        let claimed = ClaimedInstall {
            repo: prepared.repo,
            agent_type: prepared.agent_type,
            permission_mode: prepared.permission_mode,
            source: prepared.source,
            distro: prepared.distro,
            recipe: prepared.recipe,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let execution = FakeProcessRunner::new(vec![
            Ok(process_result(true, "installed", "")),
            Ok(process_result(true, "codex-cli 1.2.3", "")),
        ]);
        execute_install_inner_with(&execution, &FakeRuntimeLauncher, &claimed).unwrap();
        let calls = execution.calls();
        assert_eq!(calls[0].program, PathBuf::from("fake-npm"));
        assert_eq!(
            calls[0].args,
            ["install", "-g", "@openai/codex"].map(OsString::from)
        );
        assert_eq!(calls[1].program, PathBuf::from("fake-codex"));
        assert_eq!(calls[1].args, ["--version"].map(OsString::from));
    }

    #[test]
    fn installer_environment_excludes_credentials() {
        let mut command = Command::new("unused");
        command.env("NPM_TOKEN", "secret");
        command.env("ANTHROPIC_API_KEY", "secret");

        apply_install_environment(&mut command);

        let environment = command.get_envs().collect::<Vec<_>>();
        assert!(!environment.iter().any(|(name, _)| *name == "NPM_TOKEN"));
        assert!(!environment
            .iter()
            .any(|(name, _)| *name == "ANTHROPIC_API_KEY"));
    }
}
