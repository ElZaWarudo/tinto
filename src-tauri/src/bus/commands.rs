//! Comandos `invoke` del contrato del bus (ver
//! `docs/contracts/bus-contract.md`). Las lecturas pesadas (diffs, log,
//! blob, contenido, árbol) NO pasan por la task del bus: corren en
//! `spawn_blocking` abriendo `Git2Engine` directamente o leyendo el FS, con
//! guardas de tamaño/binario y validación de path. Los proxies de estado
//! (snapshot, suscripción, retry) hablan con la task vía `BusHandle`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Serialize;
use tauri::State;

use super::contract::{
    ContentEncoding, FileContent, GitleaksInstallResult, GitleaksSetupStatus, RepoTree,
    SubscriptionTarget, TreeEntry, WorkbenchSnapshot, FILE_CONTENT_MAX_BYTES,
    MEDIA_CONTENT_MAX_BYTES, REPO_TREE_MAX_ENTRIES,
};
use super::secret_scan;
use super::{BusHandle, RepoResolveError, ResolvedRepo};
use crate::git::{
    CommitInfo, DiffHunk, DiffLine, DiffLineKind, FileDiff, Git2Engine, GitEngine, GitError,
};
use crate::workbench::RepoSource;
use crate::wsl_agent::launcher::request_wsl_agent;
use crate::wsl_agent::protocol::{AgentRequest, AgentResponse, PROTOCOL_VERSION};

pub(crate) const GITLEAKS_TEMPLATE: &str = r#"# .gitleaks.toml
title = "Tinto local scan policy"

[extend]
useDefault = true

[allowlist]
paths = [
  '(?i)(?:^|[/\\])(?:\.git|node_modules|dist|build|target|\.next)(?:[/\\]|$)',
]
"#;

const AGENTS_MD_START: &str = "<!-- tinto-iade:start -->";
const AGENTS_MD_END: &str = "<!-- tinto-iade:end -->";
pub(crate) const AGENTS_MD_SECTION: &str = r#"<!-- tinto-iade:start -->
## Tinto IADE

Tinto is an IADE (Integrated Agentic Development Environment). When a turn is complete, notify the Tinto host so Agent Lens can close the turn and update checkpoints accurately.

At the end of each turn, run:

```sh
printf '%s\n' "$TINTO_TURN_DONE_MARKER"
```
<!-- tinto-iade:end -->
"#;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RepoFetchPreview {
    pub remote: String,
    pub host: String,
    pub sanitized_url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RepoFetchResult {
    pub remote: String,
    pub host: String,
    pub fetched_at_ms: u64,
}

#[tauri::command]
pub fn get_gitleaks_setup_status() -> Result<GitleaksSetupStatus, CommandError> {
    Ok(gitleaks_setup_status())
}

#[tauri::command]
pub async fn install_gitleaks() -> Result<GitleaksInstallResult, CommandError> {
    let install_outcome = tokio::task::spawn_blocking(secret_scan::install_gitleaks)
        .await
        .map_err(|_| CommandError::new("internal", "la tarea de instalación falló"))?;
    let status = gitleaks_setup_status();
    let method = install_outcome.method.map(|method| method.to_string());
    let message = install_outcome.message;

    Ok(GitleaksInstallResult {
        installed: status.installed,
        version: status.version,
        binary_path: status.binary_path,
        method,
        message,
    })
}

#[tauri::command]
pub async fn get_repo_gitleaks_setup_status(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
) -> Result<GitleaksSetupStatus, CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => Ok(gitleaks_setup_status()),
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::GitleaksSetupStatus {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                    },
                )? {
                    AgentResponse::GitleaksSetupStatus { status } => Ok(status),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

#[tauri::command]
pub async fn install_repo_gitleaks(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
) -> Result<GitleaksInstallResult, CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => install_gitleaks().await,
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::InstallGitleaks {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                    },
                )? {
                    AgentResponse::GitleaksInstallResult { result } => Ok(result),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

#[tauri::command]
pub async fn create_repo_gitleaks_config(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
) -> Result<(), CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo_abs = resolved.path;
            blocking(move || write_repo_gitleaks_config(&repo_abs)).await
        }
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::CreateGitleaksConfig {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                    },
                )? {
                    AgentResponse::Unit => Ok(()),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

#[tauri::command]
pub async fn create_repo_agents_md_config(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
) -> Result<(), CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo_abs = resolved.path;
            blocking(move || write_repo_agents_md_config(&repo_abs)).await
        }
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::CreateAgentsMdConfig {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                    },
                )? {
                    AgentResponse::Unit => Ok(()),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

#[tauri::command]
pub async fn get_repo_fetch_preview(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
) -> Result<RepoFetchPreview, CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo_abs = resolved.path;
            blocking(move || repo_fetch_preview(&repo_abs, None)).await
        }
        RepoSource::Wsl => Err(CommandError::new(
            "unsupported-repo-source",
            "fetch opt-in solo está disponible para repos locales",
        )),
    }
}

#[tauri::command]
pub async fn fetch_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    remote: String,
    confirmed_host: String,
    user_consent: bool,
) -> Result<RepoFetchResult, CommandError> {
    if !user_consent {
        return Err(CommandError::new(
            "user-consent-required",
            "fetch requiere confirmación explícita del usuario",
        ));
    }
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo_abs = resolved.path;
            blocking(move || fetch_repo_local(&repo_abs, &remote, &confirmed_host)).await
        }
        RepoSource::Wsl => Err(CommandError::new(
            "unsupported-repo-source",
            "fetch opt-in solo está disponible para repos locales",
        )),
    }
}

pub(crate) fn gitleaks_setup_status() -> GitleaksSetupStatus {
    let Some(path) = secret_scan::gitleaks_binary_path() else {
        return GitleaksSetupStatus {
            installed: false,
            version: None,
            binary_path: None,
        };
    };

    let version = Command::new(&path)
        .arg("version")
        .output()
        .ok()
        .and_then(|output| {
            if !output.status.success() {
                return None;
            }
            let raw = str::from_utf8(&output.stdout).ok()?;
            raw.lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_string())
        });

    GitleaksSetupStatus {
        installed: true,
        version,
        binary_path: Some(path.to_string_lossy().to_string()),
    }
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn repo_fetch_preview(
    repo_abs: &Path,
    remote_override: Option<&str>,
) -> Result<RepoFetchPreview, CommandError> {
    let repo = git2::Repository::open(repo_abs).map_err(GitError::Internal)?;
    let remote = remote_override
        .map(str::to_string)
        .or_else(|| current_fetch_remote(&repo).ok())
        .unwrap_or_else(|| "origin".to_string());
    if remote.trim().is_empty() || remote == "." {
        return Err(CommandError::new(
            "remote-without-host",
            "el upstream local no requiere fetch de red",
        ));
    }
    let git_remote = repo.find_remote(&remote).map_err(|_| {
        CommandError::new(
            "remote-not-found",
            format!("remote no encontrado: {}", safe_error_fragment(&remote)),
        )
    })?;
    let url = git_remote
        .url()
        .ok_or_else(|| CommandError::new("remote-url-missing", "remote sin URL"))?;
    let host = remote_host(url).ok_or_else(|| {
        CommandError::new(
            "remote-without-host",
            "remote sin host verificable para fetch opt-in",
        )
    })?;
    Ok(RepoFetchPreview {
        remote,
        host,
        sanitized_url: sanitize_remote_url(url),
    })
}

fn current_fetch_remote(repo: &git2::Repository) -> Result<String, CommandError> {
    let head = repo.head().map_err(GitError::Internal)?;
    let branch = head
        .shorthand()
        .ok_or_else(|| CommandError::new("detached-head", "HEAD detached no tiene upstream"))?;
    let config = repo.config().map_err(GitError::Internal)?;
    config
        .get_string(&format!("branch.{branch}.remote"))
        .or_else(|_| Ok("origin".to_string()))
}

pub(crate) fn fetch_repo_local(
    repo_abs: &Path,
    remote: &str,
    confirmed_host: &str,
) -> Result<RepoFetchResult, CommandError> {
    let preview = repo_fetch_preview(repo_abs, Some(remote))?;
    if preview.host != confirmed_host {
        return Err(CommandError::new(
            "host-confirmation-mismatch",
            "el host confirmado no coincide con el remote actual",
        ));
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(repo_abs)
        .arg("fetch")
        .arg("--prune")
        .arg(&preview.remote)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|_| CommandError::new("git-unavailable", "git no está disponible"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(fetch_command_error(&stderr));
    }

    Ok(RepoFetchResult {
        remote: preview.remote,
        host: preview.host,
        fetched_at_ms: now_epoch_ms(),
    })
}

fn fetch_command_error(stderr: &str) -> CommandError {
    let lower = stderr.to_lowercase();
    let category = if lower.contains("host key verification failed")
        || lower.contains("certificate")
        || lower.contains("ssl")
    {
        "host-not-verified"
    } else if lower.contains("could not read username")
        || lower.contains("terminal prompts disabled")
    {
        "credential-missing"
    } else if lower.contains("authentication failed") || lower.contains("permission denied") {
        "auth-rejected"
    } else if lower.contains("could not resolve host")
        || lower.contains("failed to connect")
        || lower.contains("network")
    {
        "network-unreachable"
    } else {
        "fetch-failed"
    };
    CommandError::new(category, safe_error_fragment(stderr))
}

fn safe_error_fragment(input: &str) -> String {
    let sanitized = sanitize_remote_url(input);
    let mut lines = sanitized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    lines
        .next()
        .unwrap_or("fetch falló")
        .chars()
        .take(240)
        .collect()
}

pub(crate) fn sanitize_remote_url(input: &str) -> String {
    if let Some((scheme, rest)) = input.split_once("://") {
        let (authority, tail) = match rest.find(['/', '\n', '\r', ' ', '\t']) {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, ""),
        };
        let host = authority
            .rsplit_once('@')
            .map(|(_, host)| host)
            .unwrap_or(authority);
        return format!("{scheme}://{host}{tail}");
    }

    let first = input.split_whitespace().next().unwrap_or(input);
    if let Some((left, right)) = first.split_once(':') {
        if !left.contains('/') {
            let host = left.rsplit_once('@').map(|(_, host)| host).unwrap_or(left);
            return input.replacen(first, &format!("{host}:{right}"), 1);
        }
    }
    input.to_string()
}

pub(crate) fn remote_host(url: &str) -> Option<String> {
    if let Some((_scheme, rest)) = url.split_once("://") {
        let authority = rest.split(['/', '\n', '\r', ' ', '\t']).next()?.trim();
        let host_port = authority
            .rsplit_once('@')
            .map(|(_, host)| host)
            .unwrap_or(authority);
        return host_from_authority(host_port);
    }

    let first = url.split_whitespace().next().unwrap_or(url);
    let (left, _path) = first.split_once(':')?;
    if left.contains('/') {
        return None;
    }
    let host = left.rsplit_once('@').map(|(_, host)| host).unwrap_or(left);
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn host_from_authority(authority: &str) -> Option<String> {
    if authority.is_empty() {
        return None;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split_once(']').map(|(host, _)| host.to_string());
    }
    let host = authority.split(':').next().unwrap_or(authority);
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

pub(crate) fn write_repo_gitleaks_config(repo: &Path) -> Result<(), CommandError> {
    if secret_scan::has_repo_gitleaks_config(repo) {
        return Ok(());
    }
    let target = repo.join(".gitleaks.toml");
    fs::write(&target, GITLEAKS_TEMPLATE).map_err(map_gitleaks_write_error)
}

pub(crate) fn has_repo_agents_md_config(repo: &Path) -> bool {
    fs::read_to_string(repo.join("AGENTS.md"))
        .map(|content| content.contains(AGENTS_MD_START) && content.contains(AGENTS_MD_END))
        .unwrap_or(false)
}

pub(crate) fn write_repo_agents_md_config(repo: &Path) -> Result<(), CommandError> {
    let target = repo.join("AGENTS.md");
    let existing = fs::read_to_string(&target).unwrap_or_default();
    let next = if let (Some(start), Some(end)) =
        (existing.find(AGENTS_MD_START), existing.find(AGENTS_MD_END))
    {
        let section_end = end + AGENTS_MD_END.len();
        format!(
            "{}{}{}",
            &existing[..start],
            AGENTS_MD_SECTION,
            &existing[section_end..]
        )
    } else if existing.trim().is_empty() {
        AGENTS_MD_SECTION.to_string()
    } else {
        format!("{}\n\n{}", existing.trim_end(), AGENTS_MD_SECTION)
    };
    fs::write(&target, next).map_err(map_agents_md_write_error)
}

fn map_agents_md_write_error(error: io::Error) -> CommandError {
    CommandError::new(
        "agents-md-write-failed",
        format!("no se pudo crear AGENTS.md: {error}"),
    )
}

fn map_gitleaks_write_error(error: io::Error) -> CommandError {
    CommandError::new(
        "gitleaks-config-write-failed",
        format!("no se pudo crear .gitleaks.toml: {error}"),
    )
}

/// Error de comando serializado hacia el frontend (categoría + mensaje
/// seguro), patrón análogo a `WorkbenchError`.
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub category: String,
    pub message: String,
}

impl CommandError {
    pub(crate) fn new(category: &str, message: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            message: message.into(),
        }
    }
}

impl From<GitError> for CommandError {
    fn from(e: GitError) -> Self {
        // Misma categoría que el path de delta (única fuente: GitError::category).
        CommandError::new(e.category(), e.to_string())
    }
}

/// Valida que `repo` pertenezca al workbench activo (allowlist del bus) y
/// devuelve su path canónico. Las lecturas bajo demanda SOLO operan sobre
/// repos montados: `resolve_within` contiene el path *dentro* del repo, pero
/// el repo en sí debe estar acotado al workbench (si no, un frontend
/// comprometido podría leer cualquier ruta del disco con `repo=/`).
async fn resolve_read_repo(bus: &BusHandle, repo: &Path) -> Result<ResolvedRepo, CommandError> {
    bus.resolve_repo_identity(repo.to_path_buf())
        .await
        .map_err(map_repo_resolve_error)
}

pub(crate) fn map_repo_resolve_error(error: RepoResolveError) -> CommandError {
    match error {
        RepoResolveError::UnsupportedRepoSource { .. } => CommandError::new(
            "unsupported_repo_source",
            "la fuente del repo no está disponible en este entorno",
        ),
        RepoResolveError::RepositoryNotFound => {
            CommandError::new("repository-not-found", "el repo no existe")
        }
        RepoResolveError::RepoNotAllowed => CommandError::new(
            "repo-not-allowed",
            "el repo no pertenece al workbench activo",
        ),
        RepoResolveError::BusUnavailable => {
            CommandError::new("bus-unavailable", "el bus no está disponible")
        }
    }
}

/// Corre trabajo bloqueante (git/FS) fuera del runtime async.
async fn blocking<T, F>(f: F) -> Result<T, CommandError>
where
    F: FnOnce() -> Result<T, CommandError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| CommandError::new("internal", "la tarea de lectura falló"))?
}

/// Valida que `rel` (relativo al repo) quede contenido dentro de `repo`
/// tras canonicalizar — rechaza `../` que escape. Devuelve el path absoluto
/// canónico del archivo. Requiere que el archivo exista.
fn resolve_within(repo: &Path, rel: &Path) -> Result<PathBuf, CommandError> {
    let repo_canon = repo
        .canonicalize()
        .map_err(|_| CommandError::new("repository-not-found", "el repo no existe"))?;
    let joined = repo_canon.join(rel);
    let canon = joined
        .canonicalize()
        .map_err(|_| CommandError::new("not-found", "el archivo no existe"))?;
    if !canon.starts_with(&repo_canon) {
        return Err(CommandError::new(
            "path-traversal",
            "el path se sale del repositorio",
        ));
    }
    // El árbol interno de `.git` no se expone por lectura directa (misma
    // política que `list_repo_tree`): evita filtrar `.git/config` con
    // credenciales de remoto, HEAD, index, etc.
    let inside = canon.strip_prefix(&repo_canon).unwrap_or(&canon);
    if inside
        .components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new(".git"))
    {
        return Err(CommandError::new(
            "path-forbidden",
            "el directorio .git no se expone",
        ));
    }
    Ok(canon)
}

/// Construye `FileContent` desde bytes con guardas: >1 MiB se trunca; el
/// contenido no-UTF8 (binario) se codifica en base64.
pub(crate) fn file_content_from_bytes(bytes: Vec<u8>) -> FileContent {
    file_content_from_bytes_with_limit(bytes, FILE_CONTENT_MAX_BYTES)
}

fn file_content_from_bytes_with_limit(bytes: Vec<u8>, max_bytes: usize) -> FileContent {
    let truncated = bytes.len() > max_bytes;
    let slice = if truncated {
        &bytes[..max_bytes]
    } else {
        &bytes[..]
    };
    match std::str::from_utf8(slice) {
        Ok(text) => FileContent {
            encoding: ContentEncoding::Utf8,
            content: text.to_string(),
            truncated,
        },
        // Cuando el corte por truncado parte un carácter multibyte al final
        // (`error_len() == None`), el archivo SIGUE siendo texto: se conserva
        // el prefijo UTF-8 válido en vez de degradar a base64. Un byte inválido
        // real en medio (`error_len() == Some`) sí es binario.
        Err(e) if truncated && e.error_len().is_none() => {
            let valid = std::str::from_utf8(&slice[..e.valid_up_to()])
                .expect("valid_up_to garantiza UTF-8 válido");
            FileContent {
                encoding: ContentEncoding::Utf8,
                content: valid.to_string(),
                truncated,
            }
        }
        Err(_) => FileContent {
            encoding: ContentEncoding::Base64,
            content: STANDARD.encode(slice),
            truncated,
        },
    }
}

/// Media is always returned as base64 so the frontend can build stable data URLs
/// even when an ASCII-only PDF or SVG would otherwise decode as UTF-8 text.
fn media_content_from_bytes(bytes: Vec<u8>) -> FileContent {
    let truncated = bytes.len() > MEDIA_CONTENT_MAX_BYTES;
    let slice = if truncated {
        &bytes[..MEDIA_CONTENT_MAX_BYTES]
    } else {
        &bytes[..]
    };
    FileContent {
        encoding: ContentEncoding::Base64,
        content: STANDARD.encode(slice),
        truncated,
    }
}

pub(crate) fn validate_media_path(path: &Path) -> Result<(), CommandError> {
    let supported = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "avif" | "bmp" | "gif" | "ico" | "jpeg" | "jpg" | "pdf" | "png" | "svg" | "webp"
            )
        })
        .unwrap_or(false);
    if supported {
        Ok(())
    } else {
        Err(CommandError::new(
            "unsupported-media",
            "el archivo no tiene una extensión multimedia soportada",
        ))
    }
}

/// Diff sintetizado todo-añadido para un archivo untracked (el agente lo
/// acaba de crear; `worktree_diff` de git no lo incluye). Binario o >1 MiB
/// → `FileDiff` sin hunks marcado binario. `None` si no se puede leer.
pub(crate) fn synthesize_untracked_diff(repo: &Path, rel: &Path) -> Option<FileDiff> {
    let abs = repo.join(rel);
    let bytes = std::fs::read(&abs).ok()?;
    let too_big = bytes.len() > FILE_CONTENT_MAX_BYTES;
    let text = std::str::from_utf8(&bytes).ok();
    if too_big || text.is_none() {
        return Some(FileDiff {
            path: rel.to_path_buf(),
            old_path: None,
            is_binary: true,
            hunks: Vec::new(),
        });
    }
    let text = text.unwrap();
    let lines: Vec<DiffLine> = text
        .lines()
        .enumerate()
        .map(|(i, content)| DiffLine {
            kind: DiffLineKind::Added,
            content: content.to_string(),
            old_lineno: None,
            new_lineno: Some(i as u32 + 1),
        })
        .collect();
    let hunks = if lines.is_empty() {
        Vec::new()
    } else {
        vec![DiffHunk {
            old_start: 0,
            new_start: 1,
            lines,
        }]
    };
    Some(FileDiff {
        path: rel.to_path_buf(),
        old_path: None,
        is_binary: false,
        hunks,
    })
}

// ===========================================================================
// Lecturas pesadas (spawn_blocking, sin tocar la task del bus)
// ===========================================================================

#[tauri::command]
pub async fn get_worktree_diff(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
) -> Result<Vec<FileDiff>, CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo = resolved.path;
            blocking(move || Ok(Git2Engine::open(&repo)?.worktree_diff()?)).await
        }
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::WorktreeDiff {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                    },
                )? {
                    AgentResponse::WorktreeDiff { diffs } => Ok(diffs),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

#[tauri::command]
pub async fn get_commit_diff(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    commit_id: String,
) -> Result<Vec<FileDiff>, CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo = resolved.path;
            blocking(move || Ok(Git2Engine::open(&repo)?.commit_diff(&commit_id)?)).await
        }
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::CommitDiff {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                        commit_id,
                    },
                )? {
                    AgentResponse::CommitDiff { diffs } => Ok(diffs),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

#[tauri::command]
pub async fn get_commit_log(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    offset: usize,
    limit: usize,
) -> Result<Vec<CommitInfo>, CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo = resolved.path;
            blocking(move || Ok(Git2Engine::open(&repo)?.log(offset, limit)?)).await
        }
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::CommitLog {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                        offset,
                        limit,
                    },
                )? {
                    AgentResponse::CommitLog { commits } => Ok(commits),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

#[tauri::command]
pub async fn get_blob(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    commit_id: String,
    path: PathBuf,
) -> Result<FileContent, CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo = resolved.path;
            blocking(move || {
                let bytes = Git2Engine::open(&repo)?.blob_at(&commit_id, &path)?;
                Ok(file_content_from_bytes(bytes))
            })
            .await
        }
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::Blob {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                        commit_id,
                        path,
                    },
                )? {
                    AgentResponse::Blob { content } => Ok(content),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

#[tauri::command]
pub async fn get_file_content(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    path: PathBuf,
) -> Result<FileContent, CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo = resolved.path;
            blocking(move || {
                let abs = resolve_within(&repo, &path)?;
                read_file_content_bounded(&abs)
            })
            .await
        }
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::FileContent {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                        path,
                    },
                )? {
                    AgentResponse::FileContent { content } => Ok(content),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

#[tauri::command]
pub async fn get_media_content(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    path: PathBuf,
) -> Result<FileContent, CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo = resolved.path;
            blocking(move || {
                let abs = resolve_within(&repo, &path)?;
                validate_media_path(&path)?;
                read_media_content_bounded(&abs)
            })
            .await
        }
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::MediaContent {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                        path,
                    },
                )? {
                    AgentResponse::MediaContent { content } => Ok(content),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

#[tauri::command]
pub async fn list_repo_tree(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
) -> Result<RepoTree, CommandError> {
    let resolved = resolve_read_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo = resolved.path;
            blocking(move || list_repo_tree_capped(&repo, REPO_TREE_MAX_ENTRIES)).await
        }
        RepoSource::Wsl => {
            blocking(move || {
                match wsl_request(
                    resolved.distro,
                    AgentRequest::RepoTree {
                        protocol_version: PROTOCOL_VERSION,
                        repo: resolved.path,
                        allowed_repos: resolved.wsl_repos,
                    },
                )? {
                    AgentResponse::RepoTree { tree } => Ok(tree),
                    response => Err(unexpected_wsl_response(response)),
                }
            })
            .await
        }
    }
}

pub(crate) fn wsl_request(
    distro: Option<String>,
    request: AgentRequest,
) -> Result<AgentResponse, CommandError> {
    let distro =
        distro.ok_or_else(|| CommandError::new("missing_distro", "repo WSL sin distro"))?;
    match request_wsl_agent(&distro, &request).map_err(map_wsl_agent_error)? {
        AgentResponse::Error { category, message } => Err(CommandError::new(&category, message)),
        response => Ok(response),
    }
}

fn map_wsl_agent_error(error: crate::wsl_agent::protocol::AgentError) -> CommandError {
    CommandError::new(error.safe_category(), error.message)
}

fn unexpected_wsl_response(_response: AgentResponse) -> CommandError {
    CommandError::new("malformed_response", "respuesta inesperada del agente WSL")
}

/// Lee un archivo regular con asignación acotada: rechaza no-regulares
/// (FIFO/dispositivos/dir, que bloquearían o no tienen sentido) y lee a lo
/// sumo `FILE_CONTENT_MAX_BYTES + 1` bytes, de modo que la memoria queda
/// acotada sin importar el tamaño real del archivo.
pub(crate) fn read_file_content_bounded(abs: &Path) -> Result<FileContent, CommandError> {
    read_regular_file_bounded(
        abs,
        FILE_CONTENT_MAX_BYTES,
        file_content_from_bytes_with_limit,
    )
}

pub(crate) fn read_media_content_bounded(abs: &Path) -> Result<FileContent, CommandError> {
    read_regular_file_bounded(abs, MEDIA_CONTENT_MAX_BYTES, |bytes, _| {
        media_content_from_bytes(bytes)
    })
}

fn read_regular_file_bounded(
    abs: &Path,
    max_bytes: usize,
    encode: fn(Vec<u8>, usize) -> FileContent,
) -> Result<FileContent, CommandError> {
    use std::io::Read;
    let meta = std::fs::metadata(abs)
        .map_err(|e| CommandError::new("io", format!("no se pudo leer el archivo: {e}")))?;
    if !meta.file_type().is_file() {
        return Err(CommandError::new(
            "not-a-file",
            "el path no es un archivo regular",
        ));
    }
    let file = std::fs::File::open(abs)
        .map_err(|e| CommandError::new("io", format!("no se pudo abrir el archivo: {e}")))?;
    let mut buf = Vec::new();
    file.take(max_bytes as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| CommandError::new("io", format!("no se pudo leer el archivo: {e}")))?;
    Ok(encode(buf, max_bytes))
}

/// Walk del árbol respetando `.gitignore`, excluyendo `.git`, con tope de
/// entradas (parametrizado para test del branch de truncado).
pub(crate) fn list_repo_tree_capped(repo: &Path, cap: usize) -> Result<RepoTree, CommandError> {
    {
        let repo_canon = repo
            .canonicalize()
            .map_err(|_| CommandError::new("repository-not-found", "el repo no existe"))?;
        let mut entries = Vec::new();
        let mut truncated = false;
        // Walk respetando .gitignore (mismo crate que el clasificador).
        for result in ignore::WalkBuilder::new(&repo_canon).hidden(false).build() {
            let Ok(entry) = result else { continue };
            let path = entry.path();
            // Saltar el root y siempre el árbol interno de .git.
            let Ok(rel) = path.strip_prefix(&repo_canon) else {
                continue;
            };
            if rel.as_os_str().is_empty() {
                continue;
            }
            if rel
                .components()
                .next()
                .is_some_and(|c| c.as_os_str() == ".git")
            {
                continue;
            }
            if entries.len() >= cap {
                truncated = true;
                break;
            }
            entries.push(TreeEntry {
                path: repo_relative_tree_path(rel),
                is_dir: entry.file_type().is_some_and(|t| t.is_dir()),
            });
        }
        Ok(RepoTree { entries, truncated })
    }
}

fn repo_relative_tree_path(rel: &Path) -> String {
    rel.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

// ===========================================================================
// Proxies de estado (vía la task del bus)
// ===========================================================================

#[tauri::command]
pub async fn get_workbench_snapshot(
    bus: State<'_, BusHandle>,
) -> Result<WorkbenchSnapshot, CommandError> {
    bus.snapshot()
        .await
        .ok_or_else(|| CommandError::new("bus-unavailable", "el bus no está disponible"))
}

#[tauri::command]
pub async fn set_subscriptions(
    bus: State<'_, BusHandle>,
    targets: Vec<SubscriptionTarget>,
) -> Result<(), CommandError> {
    if bus.subscribe(targets).await {
        Ok(())
    } else {
        Err(CommandError::new(
            "bus-unavailable",
            "el bus no está disponible",
        ))
    }
}

#[tauri::command]
pub async fn retry_repo(bus: State<'_, BusHandle>, repo: PathBuf) -> Result<(), CommandError> {
    if bus.retry_repo(repo).await {
        Ok(())
    } else {
        Err(CommandError::new(
            "bus-unavailable",
            "el bus no está disponible",
        ))
    }
}

#[tauri::command]
pub async fn forget_repo(bus: State<'_, BusHandle>, repo: PathBuf) -> Result<(), CommandError> {
    if bus.forget_repo(repo).await {
        Ok(())
    } else {
        Err(CommandError::new(
            "bus-unavailable",
            "el bus no está disponible",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_fixtures::TempRepo;

    #[test]
    fn file_content_utf8_y_binario() {
        let utf8 = file_content_from_bytes(b"hola\nmundo".to_vec());
        assert!(matches!(utf8.encoding, ContentEncoding::Utf8));
        assert_eq!(utf8.content, "hola\nmundo");
        assert!(!utf8.truncated);

        let bin = file_content_from_bytes(vec![0u8, 159, 146, 150]);
        assert!(matches!(bin.encoding, ContentEncoding::Base64));
    }

    #[test]
    fn file_content_trunca_sobre_el_limite() {
        let big = vec![b'a'; FILE_CONTENT_MAX_BYTES + 100];
        let fc = file_content_from_bytes(big);
        assert!(fc.truncated);
        assert_eq!(fc.content.len(), FILE_CONTENT_MAX_BYTES);
    }

    #[test]
    fn media_content_siempre_base64_y_usa_limite_multimedia() {
        let ascii_pdf = b"%PDF-1.7\n%%EOF".to_vec();
        let fc = media_content_from_bytes(ascii_pdf);
        assert!(matches!(fc.encoding, ContentEncoding::Base64));
        assert!(!fc.truncated);
        assert_eq!(fc.content, STANDARD.encode(b"%PDF-1.7\n%%EOF"));

        let big = vec![b'a'; MEDIA_CONTENT_MAX_BYTES + 50];
        let fc = media_content_from_bytes(big);
        assert!(fc.truncated);
        assert_eq!(
            fc.content,
            STANDARD.encode(vec![b'a'; MEDIA_CONTENT_MAX_BYTES])
        );
    }

    #[test]
    fn agents_md_config_writer_creates_iade_section() {
        let repo = tempfile::tempdir().unwrap();

        write_repo_agents_md_config(repo.path()).unwrap();

        let content = std::fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
        assert!(content.contains("Integrated Agentic Development Environment"));
        assert!(content.contains("TINTO_TURN_DONE_MARKER"));
        assert!(has_repo_agents_md_config(repo.path()));
    }

    #[test]
    fn agents_md_config_writer_replaces_existing_tinto_section() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("AGENTS.md"),
            "Project instructions\n\n<!-- tinto-iade:start -->\nold\n<!-- tinto-iade:end -->\n",
        )
        .unwrap();

        write_repo_agents_md_config(repo.path()).unwrap();

        let content = std::fs::read_to_string(repo.path().join("AGENTS.md")).unwrap();
        assert!(content.starts_with("Project instructions"));
        assert!(!content.contains("\nold\n"));
        assert_eq!(content.matches("<!-- tinto-iade:start -->").count(), 1);
    }

    #[test]
    fn repo_fetch_preview_sanitizes_remote_credentials_and_resolves_host() {
        let repo_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(repo_dir.path()).unwrap();
        repo.remote("origin", "https://user:secret@github.com/acme/repo.git")
            .unwrap();

        let preview = repo_fetch_preview(repo_dir.path(), Some("origin")).unwrap();

        assert_eq!(preview.remote, "origin");
        assert_eq!(preview.host, "github.com");
        assert_eq!(preview.sanitized_url, "https://github.com/acme/repo.git");
        assert!(!preview.sanitized_url.contains("secret"));
    }

    #[test]
    fn repo_fetch_helpers_support_scp_style_remotes_without_userinfo() {
        assert_eq!(
            remote_host("git@github.com:acme/repo.git"),
            Some("github.com".to_string())
        );
        assert_eq!(
            sanitize_remote_url("git@github.com:acme/repo.git"),
            "github.com:acme/repo.git"
        );
    }

    #[test]
    fn fetch_command_error_classifies_and_sanitizes_credential_failures() {
        let err = fetch_command_error(
            "fatal: could not read Username for 'https://token@github.com/acme/repo.git': terminal prompts disabled",
        );

        assert_eq!(err.category, "credential-missing");
        assert!(!err.message.contains("token@"));
        assert!(err.message.contains("https://github.com/acme/repo.git"));
    }

    #[test]
    fn fetch_repo_local_fails_closed_when_confirmed_host_drifted() {
        let repo_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(repo_dir.path()).unwrap();
        repo.remote("origin", "https://github.com/acme/repo.git")
            .unwrap();

        let err =
            fetch_repo_local(repo_dir.path(), "origin", "gitlab.com").expect_err("host mismatch");

        assert_eq!(err.category, "host-confirmation-mismatch");
    }

    #[test]
    fn validate_media_path_acepta_solo_pdf_e_imagenes_soportadas() {
        assert!(validate_media_path(Path::new("docs/spec.PDF")).is_ok());
        assert!(validate_media_path(Path::new("assets/logo.png")).is_ok());
        assert!(validate_media_path(Path::new("assets/icon.svg")).is_ok());
        let err = validate_media_path(Path::new("src/main.rs")).expect_err("no media");
        assert_eq!(err.category, "unsupported-media");
    }

    #[test]
    fn unsupported_repo_resolve_error_maps_to_safe_category() {
        let err = map_repo_resolve_error(RepoResolveError::UnsupportedRepoSource {
            source: crate::workbench::RepoSource::Wsl,
        });

        assert_eq!(err.category, "unsupported_repo_source");
        assert_eq!(
            err.message,
            "la fuente del repo no está disponible en este entorno"
        );
        assert!(!err.message.contains("backend local"));
        assert!(!err.message.contains("/home/me/proyecto"));
    }

    #[test]
    fn resolve_within_rechaza_traversal() {
        let repo = TempRepo::with_initial_commit();
        // base.txt existe y queda dentro.
        assert!(resolve_within(repo.path(), Path::new("base.txt")).is_ok());
        // Target que EXISTE fuera del repo: canonicalize() tiene éxito, así que
        // solo el guard `starts_with` puede rechazarlo → prueba que el branch
        // de seguridad efectivamente dispara (no el de not-found).
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("secret.txt");
        std::fs::write(&outside_file, "x").unwrap();
        let err = resolve_within(repo.path(), &outside_file).expect_err("debe rechazar el escape");
        assert_eq!(
            err.category, "path-traversal",
            "debe ser el guard starts_with, no not-found"
        );
        // Target inexistente: branch not-found, pinned por separado.
        let err = resolve_within(repo.path(), Path::new("no/existe.txt"))
            .expect_err("inexistente debe fallar");
        assert_eq!(err.category, "not-found");
    }

    #[cfg(unix)]
    #[test]
    fn resolve_within_rechaza_symlink_que_escapa() {
        use std::os::unix::fs::symlink;
        let repo = TempRepo::with_initial_commit();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "x").unwrap();
        symlink(outside.path().join("secret"), repo.path().join("link")).unwrap();
        // canonicalize() resuelve el symlink fuera del repo → debe rechazarse.
        let err = resolve_within(repo.path(), Path::new("link")).expect_err("symlink escape");
        assert_eq!(err.category, "path-traversal");
    }

    #[test]
    fn resolve_within_rechaza_punto_git() {
        let repo = TempRepo::with_initial_commit();
        // `.git/config` existe y queda dentro del repo, pero no se expone.
        let err =
            resolve_within(repo.path(), Path::new(".git/config")).expect_err(".git no se expone");
        assert_eq!(err.category, "path-forbidden");
    }

    #[test]
    fn file_content_trunca_en_frontera_utf8_sigue_siendo_texto() {
        // Un carácter multibyte (é = 2 bytes) que cruza el corte del truncado:
        // el archivo SIGUE siendo texto, no debe degradar a base64.
        let mut bytes = vec![b'a'; FILE_CONTENT_MAX_BYTES - 1];
        bytes.extend_from_slice("é".as_bytes());
        bytes.extend(std::iter::repeat_n(b'b', 50));
        let fc = file_content_from_bytes(bytes);
        assert!(fc.truncated);
        assert!(
            matches!(fc.encoding, ContentEncoding::Utf8),
            "frontera UTF-8 no debe forzar base64"
        );
        assert_eq!(fc.content.len(), FILE_CONTENT_MAX_BYTES - 1);
    }

    #[test]
    fn read_file_content_acota_y_rechaza_no_regulares() {
        let repo = TempRepo::with_initial_commit();
        // Archivo > límite: leído acotado y marcado truncado.
        let big = "x".repeat(FILE_CONTENT_MAX_BYTES + 500);
        repo.write("grande.txt", &big);
        let fc = read_file_content_bounded(&repo.path().join("grande.txt")).expect("lee");
        assert!(fc.truncated);
        assert_eq!(fc.content.len(), FILE_CONTENT_MAX_BYTES);
        // Un directorio no es archivo regular → rechazado sin colgarse.
        let err = read_file_content_bounded(repo.path()).expect_err("dir no es archivo");
        assert_eq!(err.category, "not-a-file");
    }

    #[test]
    fn read_media_content_lee_binarios_mayores_a_un_mebibyte() {
        let repo = TempRepo::with_initial_commit();
        let bytes = vec![7u8; FILE_CONTENT_MAX_BYTES + 128];
        std::fs::write(repo.path().join("image.png"), &bytes).unwrap();
        let fc = read_media_content_bounded(&repo.path().join("image.png")).expect("lee media");
        assert!(matches!(fc.encoding, ContentEncoding::Base64));
        assert!(!fc.truncated);
        assert_eq!(fc.content, STANDARD.encode(bytes));
    }

    #[test]
    fn synthesize_untracked_diff_todo_anadido() {
        let repo = TempRepo::with_initial_commit();
        repo.write("nuevo.txt", "a\nb\nc\n");
        let diff = synthesize_untracked_diff(repo.path(), Path::new("nuevo.txt")).expect("diff");
        assert!(!diff.is_binary);
        assert_eq!(diff.hunks.len(), 1);
        assert_eq!(diff.hunks[0].lines.len(), 3);
        assert!(diff.hunks[0]
            .lines
            .iter()
            .all(|l| matches!(l.kind, DiffLineKind::Added)));
    }

    #[test]
    fn list_repo_tree_excluye_git_y_respeta_gitignore() {
        let repo = TempRepo::with_initial_commit();
        repo.write(".gitignore", "target/\n");
        repo.write("src/main.rs", "fn main() {}");
        repo.write("target/out.o", "obj");

        let tree = list_repo_tree_capped(repo.path(), REPO_TREE_MAX_ENTRIES).expect("tree");
        let paths: Vec<&str> = tree.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"src/main.rs"));
        assert!(paths.contains(&"base.txt"));
        assert!(
            paths.iter().all(|p| !p.contains('\\')),
            "paths del árbol deben usar separadores POSIX"
        );
        assert!(
            !paths.iter().any(|p| p.starts_with(".git/")),
            "sin .git interno"
        );
        assert!(
            !paths.iter().any(|p| p.starts_with("target/")),
            "target gitignoreado"
        );
        assert!(!tree.truncated);
    }

    #[test]
    fn list_repo_tree_marca_truncated_al_alcanzar_el_cap() {
        let repo = TempRepo::with_initial_commit();
        for i in 0..5 {
            repo.write(&format!("f{i}.txt"), "x");
        }
        let tree = list_repo_tree_capped(repo.path(), 2).expect("tree");
        assert!(tree.truncated, "debe marcar truncado al llegar al cap");
        assert_eq!(
            tree.entries.len(),
            2,
            "el cap es exclusivo de la entrada que rompe"
        );
    }
}
