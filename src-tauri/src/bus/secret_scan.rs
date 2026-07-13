use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{Cursor, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use serde::Deserialize;
use tar::Archive;
use uuid::Uuid;
use zip::ZipArchive;

use super::contract::SecretFinding;
use crate::git::{DiffLineKind, FileDiff, RepoStatus};

const GITLEAKS_TIMEOUT_SECONDS: u64 = 8;
const FALLBACK_RULE_ID: &str = "heuristic-possible-secret";
const GITLEAKS_FALLBACK_CANDIDATES: [&str; 2] = [".gitleaks.toml", "gitleaks.toml"];
const GITLEAKS_RELEASE_API: &str = "https://api.github.com/repos/gitleaks/gitleaks/releases/latest";
const GITLEAKS_RELEASES_LATEST: &str = "https://github.com/gitleaks/gitleaks/releases/latest";
const GITLEAKS_RELEASES_BASE: &str = "https://github.com/gitleaks/gitleaks/releases";
const GITLEAKS_USER_AGENT: &str = "tinto-addon-manager";

#[derive(Debug)]
pub(crate) struct GitleaksInstallOutcome {
    pub(crate) method: Option<&'static str>,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
struct InstallAttempt {
    method: &'static str,
    program: &'static str,
    args: &'static [&'static str],
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    TarGz,
    Zip,
}

#[derive(Debug, Clone)]
struct ManagedReleaseAsset {
    name: String,
    download_url: String,
    archive: ArchiveKind,
    version: String,
}

#[derive(Debug, Deserialize)]
struct GitleaksReportFinding {
    #[serde(alias = "File")]
    file: String,
    #[serde(alias = "StartLine")]
    start_line: Option<u32>,
    #[serde(alias = "EndLine")]
    end_line: Option<u32>,
    #[serde(alias = "Line")]
    line: Option<u32>,
    #[serde(alias = "RuleID")]
    rule_id: Option<String>,
    #[serde(alias = "Description")]
    description: Option<String>,
}

pub(crate) fn detect_secret_findings(
    repo: &Path,
    status: &RepoStatus,
    diffs: &[FileDiff],
) -> Vec<SecretFinding> {
    let changed_paths = changed_paths(status, diffs);
    if changed_paths.is_empty() {
        return Vec::new();
    }

    let config = repo_gitleaks_config(repo);
    scan_with_gitleaks(repo, status, diffs, &changed_paths, config.as_deref())
        .unwrap_or_else(|| heuristic_findings(diffs))
}

pub(crate) fn has_repo_gitleaks_config(repo: &Path) -> bool {
    repo_gitleaks_config(repo).is_some()
}

pub(crate) fn gitleaks_binary_path() -> Option<PathBuf> {
    if let Some(path) = managed_gitleaks_binary_path() {
        return Some(path);
    }
    if let Ok(path) = which::which("gitleaks") {
        return Some(path);
    }
    gitleaks_known_paths()
        .into_iter()
        .find(|path| path.is_file())
}

pub(crate) fn install_gitleaks() -> GitleaksInstallOutcome {
    if gitleaks_binary_path().is_some() {
        return GitleaksInstallOutcome {
            method: Some("already-installed"),
            message: "Gitleaks ya está disponible en el sistema.".to_string(),
        };
    }

    let mut failures = Vec::new();

    match install_managed_gitleaks() {
        Ok(binary_path) => {
            return GitleaksInstallOutcome {
                method: Some("managed-download"),
                message: format!(
                    "Gitleaks instalado por Tinto en {}.",
                    binary_path.to_string_lossy()
                ),
            };
        }
        Err(error) => failures.push(format!("managed-download: {error}")),
    }

    for attempt in gitleaks_install_attempts() {
        let (ok, detail) = run_install_attempt(&attempt);
        if gitleaks_binary_path().is_some() {
            return GitleaksInstallOutcome {
                method: Some(attempt.method),
                message: format!("Instalado con {method}: {detail}", method = attempt.method),
            };
        }
        if !ok {
            failures.push(format!("{}: {}", attempt.method, detail));
        }
    }

    GitleaksInstallOutcome {
        method: None,
        message: format!(
            "No se pudo instalar Gitleaks automáticamente. {}",
            summarize_install_failures(&failures)
        ),
    }
}

fn install_managed_gitleaks() -> Result<PathBuf, String> {
    let release = resolve_latest_gitleaks_release()?;
    let asset = pick_gitleaks_asset(&release)
        .ok_or_else(|| format!("no hay binario publicado para {}", target_descriptor()))?;
    let response = Client::builder()
        .build()
        .map_err(|error| format!("no se pudo crear el cliente HTTP: {error}"))?
        .get(&asset.download_url)
        .header("User-Agent", GITLEAKS_USER_AGENT)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("no se pudo descargar {}: {error}", asset.name))?;
    let bytes = response
        .bytes()
        .map_err(|error| format!("descarga incompleta de {}: {error}", asset.name))?;
    let target = managed_gitleaks_install_path();
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("no se pudo preparar {}: {error}", parent.display()))?;
    }
    extract_gitleaks_binary(bytes.as_ref(), asset.archive, &target)?;
    write_managed_version_file(&asset.version)?;
    Ok(target)
}

fn run_install_attempt(attempt: &InstallAttempt) -> (bool, String) {
    let output = Command::new(attempt.program).args(attempt.args).output();
    match output {
        Ok(output) => {
            let detail = if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stdout.is_empty() {
                    stderr
                } else if stderr.is_empty() {
                    stdout
                } else {
                    format!("{stdout}\n{stderr}")
                }
            } else {
                format!(
                    "No pudo ejecutar {} {}: {}",
                    attempt.program,
                    if output.status.success() {
                        "correctamente"
                    } else {
                        "con error"
                    },
                    output.status
                )
            };
            (output.status.success(), detail)
        }
        Err(error) => (
            false,
            format!("No se encontró el instalador {}: {error}", attempt.program),
        ),
    }
}

fn gitleaks_install_attempts() -> Vec<InstallAttempt> {
    #[cfg(target_os = "windows")]
    {
        vec![
            InstallAttempt {
                method: "winget",
                program: "winget",
                args: &[
                    "install",
                    "--id",
                    "Zricethezav.Gitleaks",
                    "--accept-source-agreements",
                    "--accept-package-agreements",
                    "--silent",
                ],
            },
            InstallAttempt {
                method: "choco",
                program: "choco",
                args: &["install", "gitleaks", "-y"],
            },
            InstallAttempt {
                method: "scoop",
                program: "scoop",
                args: &["install", "gitleaks"],
            },
        ]
    }

    #[cfg(target_os = "macos")]
    {
        vec![
            InstallAttempt {
                method: "brew",
                program: "brew",
                args: &["install", "gitleaks"],
            },
            InstallAttempt {
                method: "go",
                program: "go",
                args: &["install", "github.com/gitleaks/gitleaks/v8@latest"],
            },
        ]
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        vec![
            InstallAttempt {
                method: "brew",
                program: "brew",
                args: &["install", "gitleaks"],
            },
            InstallAttempt {
                method: "apt",
                program: "apt-get",
                args: &["install", "-y", "gitleaks"],
            },
            InstallAttempt {
                method: "dnf",
                program: "dnf",
                args: &["install", "-y", "gitleaks"],
            },
            InstallAttempt {
                method: "yum",
                program: "yum",
                args: &["install", "-y", "gitleaks"],
            },
            InstallAttempt {
                method: "pacman",
                program: "pacman",
                args: &["-S", "--noconfirm", "gitleaks"],
            },
            InstallAttempt {
                method: "zypper",
                program: "zypper",
                args: &["--non-interactive", "install", "gitleaks"],
            },
            InstallAttempt {
                method: "apk",
                program: "apk",
                args: &["add", "gitleaks"],
            },
            InstallAttempt {
                method: "go",
                program: "go",
                args: &["install", "github.com/gitleaks/gitleaks/v8@latest"],
            },
        ]
    }
}

fn gitleaks_known_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let binary = gitleaks_binary_name();
    candidates.push(managed_gitleaks_install_path());

    #[cfg(target_os = "windows")]
    {
        if let Some(home) = env::var_os("USERPROFILE") {
            candidates.push(PathBuf::from(home).join("go").join("bin").join(binary));
        }
        if let Some(local_appdata) = env::var_os("LOCALAPPDATA") {
            let local_appdata = PathBuf::from(local_appdata);
            candidates.push(
                local_appdata
                    .join("Programs")
                    .join("gitleaks")
                    .join("gitleaks.exe"),
            );
            candidates.push(
                local_appdata
                    .join("Programs")
                    .join("gitleaks")
                    .join("bin")
                    .join(binary),
            );
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = env::var_os("HOME") {
            candidates.push(
                PathBuf::from(home.clone())
                    .join(".cargo")
                    .join("bin")
                    .join(binary),
            );
            candidates.push(
                PathBuf::from(home.clone())
                    .join("go")
                    .join("bin")
                    .join(binary),
            );
            candidates.push(PathBuf::from(home).join(".local").join("bin").join(binary));
        }
        candidates.push(PathBuf::from("/usr/local/bin").join(binary));
        candidates.push(PathBuf::from("/usr/bin").join(binary));
    }

    candidates
}

fn managed_gitleaks_binary_path() -> Option<PathBuf> {
    let path = managed_gitleaks_install_path();
    path.is_file().then_some(path)
}

fn managed_gitleaks_root() -> PathBuf {
    if let Some(base) = crate::runtime_paths::data_local_dir() {
        return base.join("tinto").join("addons").join("gitleaks");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("tinto")
            .join("addons")
            .join("gitleaks");
    }
    PathBuf::from(".tinto").join("addons").join("gitleaks")
}

fn managed_gitleaks_install_path() -> PathBuf {
    managed_gitleaks_root()
        .join("bin")
        .join(gitleaks_binary_name())
}

fn managed_gitleaks_version_file() -> PathBuf {
    managed_gitleaks_root().join("VERSION")
}

fn write_managed_version_file(version: &str) -> Result<(), String> {
    fs::write(managed_gitleaks_version_file(), version)
        .map_err(|error| format!("no se pudo registrar la versión instalada: {error}"))
}

fn fetch_latest_gitleaks_release() -> Result<GithubRelease, String> {
    Client::builder()
        .build()
        .map_err(|error| format!("no se pudo crear el cliente HTTP: {error}"))?
        .get(GITLEAKS_RELEASE_API)
        .header("User-Agent", GITLEAKS_USER_AGENT)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("no se pudo consultar la release de Gitleaks: {error}"))?
        .json::<GithubRelease>()
        .map_err(|error| format!("respuesta inválida de releases: {error}"))
}

fn resolve_latest_gitleaks_release() -> Result<GithubRelease, String> {
    fetch_latest_gitleaks_release_html().or_else(|html_error| {
        fetch_latest_gitleaks_release()
            .map_err(|api_error| format!("{html_error} | api: {api_error}"))
    })
}

fn fetch_latest_gitleaks_release_html() -> Result<GithubRelease, String> {
    let client = Client::builder()
        .build()
        .map_err(|error| format!("no se pudo crear el cliente HTTP: {error}"))?;
    let latest = client
        .get(GITLEAKS_RELEASES_LATEST)
        .header("User-Agent", GITLEAKS_USER_AGENT)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("no se pudo abrir la release pública de Gitleaks: {error}"))?;
    let latest_url = latest.url().to_string();
    let tag = latest_url
        .rsplit('/')
        .next()
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| "no se pudo resolver el tag de la release actual".to_string())?
        .to_string();
    let assets_html = client
        .get(format!("{GITLEAKS_RELEASES_BASE}/expanded_assets/{tag}"))
        .header("User-Agent", GITLEAKS_USER_AGENT)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("no se pudo cargar la lista pública de assets: {error}"))?
        .text()
        .map_err(|error| format!("no se pudo leer la lista pública de assets: {error}"))?;
    let assets = extract_release_assets_from_html(&assets_html);
    if assets.is_empty() {
        return Err("la release pública no expone assets descargables".to_string());
    }
    Ok(GithubRelease {
        tag_name: tag,
        assets,
    })
}

fn extract_release_assets_from_html(html: &str) -> Vec<GithubReleaseAsset> {
    let mut assets = Vec::new();
    let mut seen = HashSet::new();
    let mut remaining = html;
    while let Some(start) = remaining.find("gitleaks_") {
        let candidate = &remaining[start..];
        let end = candidate
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '"' | '\'' | '<' | '>' | ')' | '(')
            })
            .unwrap_or(candidate.len());
        let name = &candidate[..end];
        remaining = &candidate[end..];
        if !name.starts_with("gitleaks_") {
            continue;
        }
        if !(name.ends_with(".tar.gz") || name.ends_with(".zip")) {
            continue;
        }
        if !seen.insert(name.to_string()) {
            continue;
        }
        let version = name
            .strip_prefix("gitleaks_")
            .and_then(|rest| rest.split('_').next())
            .unwrap_or("")
            .trim();
        if version.is_empty() {
            continue;
        }
        let tag = format!("v{version}");
        assets.push(GithubReleaseAsset {
            name: name.to_string(),
            browser_download_url: format!("{GITLEAKS_RELEASES_BASE}/download/{tag}/{name}"),
        });
    }
    assets
}

fn pick_gitleaks_asset(release: &GithubRelease) -> Option<ManagedReleaseAsset> {
    let os_tokens = target_os_tokens();
    let arch_tokens = target_arch_tokens();
    release.assets.iter().find_map(|asset| {
        let lower = asset.name.to_ascii_lowercase();
        let archive = if lower.ends_with(".tar.gz") {
            Some(ArchiveKind::TarGz)
        } else if lower.ends_with(".zip") {
            Some(ArchiveKind::Zip)
        } else {
            None
        }?;
        let os_match = os_tokens.iter().any(|token| lower.contains(token));
        let arch_match = arch_tokens.iter().any(|token| lower.contains(token));
        if !lower.contains("gitleaks") || !os_match || !arch_match {
            return None;
        }
        Some(ManagedReleaseAsset {
            name: asset.name.clone(),
            download_url: asset.browser_download_url.clone(),
            archive,
            version: infer_version_from_release(release, asset),
        })
    })
}

fn extract_gitleaks_binary(
    bytes: &[u8],
    archive: ArchiveKind,
    target: &Path,
) -> Result<(), String> {
    let data = match archive {
        ArchiveKind::TarGz => extract_binary_from_tar_gz(bytes)?,
        ArchiveKind::Zip => extract_binary_from_zip(bytes)?,
    };
    let temp_path = target.with_extension("tmp");
    if target.exists() {
        fs::remove_file(target)
            .map_err(|error| format!("no se pudo reemplazar el binario anterior: {error}"))?;
    }
    fs::write(&temp_path, data)
        .map_err(|error| format!("no se pudo escribir el binario descargado: {error}"))?;
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&temp_path)
            .map_err(|error| format!("no se pudo leer permisos temporales: {error}"))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&temp_path, permissions)
            .map_err(|error| format!("no se pudieron aplicar permisos de ejecución: {error}"))?;
    }
    fs::rename(&temp_path, target).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        format!("no se pudo activar el binario descargado: {error}")
    })?;
    Ok(())
}

fn extract_binary_from_tar_gz(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = Archive::new(decoder);
    for entry in archive
        .entries()
        .map_err(|error| format!("no se pudo leer el tarball: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("entrada de tar inválida: {error}"))?;
        let Some(name) = entry.path().ok().and_then(|path| {
            path.file_name()
                .map(|file_name| file_name.to_string_lossy().to_string())
        }) else {
            continue;
        };
        if name != gitleaks_binary_name() {
            continue;
        }
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|error| format!("no se pudo extraer el binario: {error}"))?;
        return Ok(data);
    }
    Err("el archivo descargado no contiene el ejecutable de Gitleaks".to_string())
}

fn extract_binary_from_zip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|error| format!("zip inválido para Gitleaks: {error}"))?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("entrada ZIP inválida: {error}"))?;
        let Some(name) = Path::new(file.name())
            .file_name()
            .map(|file_name| file_name.to_string_lossy().to_string())
        else {
            continue;
        };
        if name != gitleaks_binary_name() {
            continue;
        }
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|error| format!("no se pudo extraer el binario ZIP: {error}"))?;
        return Ok(data);
    }
    Err("el ZIP descargado no contiene el ejecutable de Gitleaks".to_string())
}

fn infer_version_from_release(release: &GithubRelease, asset: &GithubReleaseAsset) -> String {
    let version = release.tag_name.trim_start_matches('v').trim().to_string();
    if !version.is_empty() {
        return version;
    }
    let fallback = asset
        .name
        .trim_start_matches("gitleaks_")
        .split('_')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if fallback.is_empty() {
        "latest".to_string()
    } else {
        fallback
    }
}

fn summarize_install_failures(failures: &[String]) -> String {
    if failures.is_empty() {
        return "Sin diagnóstico adicional.".to_string();
    }
    failures
        .iter()
        .take(4)
        .cloned()
        .collect::<Vec<_>>()
        .join(" | ")
}

fn target_descriptor() -> String {
    format!("{}-{}", env::consts::OS, env::consts::ARCH)
}

fn target_os_tokens() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &["windows"]
    }
    #[cfg(target_os = "macos")]
    {
        &["darwin", "macos"]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        &["linux"]
    }
}

fn target_arch_tokens() -> &'static [&'static str] {
    #[cfg(target_arch = "x86_64")]
    {
        &["x64", "x86_64", "amd64"]
    }
    #[cfg(target_arch = "aarch64")]
    {
        &["arm64", "aarch64"]
    }
    #[cfg(target_arch = "arm")]
    {
        &["armv7", "armv6", "arm"]
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "arm")))]
    {
        &[env::consts::ARCH]
    }
}

fn gitleaks_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "gitleaks.exe"
    } else {
        "gitleaks"
    }
}

fn scan_with_gitleaks(
    repo: &Path,
    status: &RepoStatus,
    diffs: &[FileDiff],
    changed_paths: &HashSet<String>,
    config_path: Option<&Path>,
) -> Option<Vec<SecretFinding>> {
    let gitleaks = gitleaks_binary_path()?;
    let report_path = std::env::temp_dir().join(format!("tinto-gitleaks-{}.json", Uuid::new_v4()));
    let mut command = Command::new(gitleaks);
    if let Some(path) = config_path {
        command.arg("--config").arg(path);
    }
    let output = command
        .arg("dir")
        .arg("--no-banner")
        .arg("--redact=100")
        .arg("--exit-code")
        .arg("0")
        .arg("--timeout")
        .arg(GITLEAKS_TIMEOUT_SECONDS.to_string())
        .arg("--report-format")
        .arg("json")
        .arg("--report-path")
        .arg(&report_path)
        .arg(repo)
        .output()
        .ok()?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&report_path);
        return None;
    }

    let report = std::fs::read_to_string(&report_path).ok()?;
    let _ = std::fs::remove_file(&report_path);
    let report = if report.trim().is_empty() {
        "[]".to_string()
    } else {
        report
    };
    let parsed: Vec<GitleaksReportFinding> = serde_json::from_str(&report).ok()?;
    Some(filter_report_findings(
        repo,
        status,
        diffs,
        changed_paths,
        parsed,
    ))
}

fn repo_gitleaks_config(repo: &Path) -> Option<PathBuf> {
    GITLEAKS_FALLBACK_CANDIDATES.iter().find_map(|name| {
        let candidate = repo.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        None
    })
}

fn filter_report_findings(
    repo: &Path,
    status: &RepoStatus,
    diffs: &[FileDiff],
    changed_paths: &HashSet<String>,
    parsed: Vec<GitleaksReportFinding>,
) -> Vec<SecretFinding> {
    let untracked = status
        .untracked
        .iter()
        .map(|path| normalize_path(path))
        .collect::<HashSet<_>>();
    let added_lines = added_lines_by_path(diffs);
    let mut findings = Vec::new();
    let mut seen = HashSet::new();

    for finding in parsed {
        let path = normalize_report_path(repo, &finding.file);
        if !changed_paths.contains(&path) {
            continue;
        }

        let line = finding.start_line.or(finding.line).unwrap_or(1);
        let end_line = finding.end_line.unwrap_or(line);
        if !untracked.contains(&path) {
            let Some(lines) = added_lines.get(&path) else {
                continue;
            };
            if !(line..=end_line).any(|candidate| lines.contains(&candidate)) {
                continue;
            }
        }

        let rule_id = finding.rule_id.unwrap_or_else(|| "gitleaks".into());
        if !seen.insert((path.clone(), line, rule_id.clone())) {
            continue;
        }

        findings.push(SecretFinding {
            path: PathBuf::from(path),
            line,
            rule_id,
            description: finding
                .description
                .unwrap_or_else(|| "Possible secret".into()),
        });
    }

    findings.sort_by(|a, b| {
        normalize_path(&a.path)
            .cmp(&normalize_path(&b.path))
            .then(a.line.cmp(&b.line))
            .then(a.rule_id.cmp(&b.rule_id))
    });
    findings
}

pub(crate) fn heuristic_findings(diffs: &[FileDiff]) -> Vec<SecretFinding> {
    let mut findings = Vec::new();
    let mut seen = HashSet::new();

    for diff in diffs {
        for line in diff.hunks.iter().flat_map(|h| h.lines.iter()) {
            if line.kind != DiffLineKind::Added {
                continue;
            }
            let Some(line_no) = line.new_lineno else {
                continue;
            };
            if !secret_line_marker(&line.content) {
                continue;
            }
            let path = normalize_path(&diff.path);
            if !seen.insert((path.clone(), line_no)) {
                continue;
            }
            findings.push(SecretFinding {
                path: PathBuf::from(path),
                line: line_no,
                rule_id: FALLBACK_RULE_ID.into(),
                description: "Possible secret".into(),
            });
        }
    }

    findings.sort_by(|a, b| {
        normalize_path(&a.path)
            .cmp(&normalize_path(&b.path))
            .then(a.line.cmp(&b.line))
    });
    findings
}

pub(crate) fn secret_line_marker(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    (lower.contains("-----begin ") && lower.contains("private key"))
        || sensitive_assignment(&lower, "api_key")
        || sensitive_assignment(&lower, "apikey")
        || sensitive_assignment(&lower, "access_token")
        || sensitive_assignment(&lower, "auth_token")
        || sensitive_assignment(&lower, "secret")
        || sensitive_assignment(&lower, "token")
        || sensitive_assignment(&lower, "password")
        || sensitive_assignment(&lower, "private_key")
}

fn sensitive_assignment(line: &str, key: &str) -> bool {
    for separator in ["=", ":"] {
        let tight = format!("{key}{separator}");
        if let Some(index) = line.find(&tight) {
            let value = line[index + tight.len()..].trim_start();
            if !looks_like_type_annotation(value) {
                return true;
            }
        }

        let spaced = format!("{key} {separator}");
        if let Some(index) = line.find(&spaced) {
            let value = line[index + spaced.len()..].trim_start();
            if !looks_like_type_annotation(value) {
                return true;
            }
        }
    }
    false
}

fn looks_like_type_annotation(value: &str) -> bool {
    let value = value.trim_start_matches(['&', '\'', '"']);
    value.starts_with("string")
        || value.starts_with("str")
        || value.starts_with("number")
        || value.starts_with("boolean")
        || value.starts_with("bool")
        || value.starts_with("uuid")
        || value.starts_with("pathbuf")
        || value.starts_with("vec<")
        || value.starts_with("option<")
        || value.starts_with("result<")
}

fn changed_paths(status: &RepoStatus, diffs: &[FileDiff]) -> HashSet<String> {
    let mut paths = status
        .modified
        .iter()
        .chain(status.staged.iter())
        .chain(status.untracked.iter())
        .map(|path| normalize_path(path))
        .collect::<HashSet<_>>();
    for diff in diffs {
        paths.insert(normalize_path(&diff.path));
        if let Some(old_path) = &diff.old_path {
            paths.insert(normalize_path(old_path));
        }
    }
    paths
}

fn added_lines_by_path(diffs: &[FileDiff]) -> HashMap<String, HashSet<u32>> {
    let mut lines = HashMap::new();
    for diff in diffs {
        let entry = lines
            .entry(normalize_path(&diff.path))
            .or_insert_with(HashSet::new);
        for line in diff.hunks.iter().flat_map(|h| h.lines.iter()) {
            if line.kind == DiffLineKind::Added {
                if let Some(line_no) = line.new_lineno {
                    entry.insert(line_no);
                }
            }
        }
    }
    lines
}

fn normalize_report_path(repo: &Path, file: &str) -> String {
    let report_path = PathBuf::from(file);
    if report_path.is_absolute() {
        report_path
            .strip_prefix(repo)
            .map(normalize_path)
            .unwrap_or_else(|_| normalize_path(&report_path))
    } else {
        normalize_path(&report_path)
    }
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_diff(path: &str, lines: Vec<(&str, Option<u32>)>) -> FileDiff {
        FileDiff {
            path: path.into(),
            old_path: None,
            is_binary: false,
            hunks: vec![crate::git::DiffHunk {
                old_start: 1,
                new_start: 1,
                lines: lines
                    .into_iter()
                    .map(|(content, line)| crate::git::DiffLine {
                        kind: DiffLineKind::Added,
                        content: content.into(),
                        old_lineno: None,
                        new_lineno: line,
                    })
                    .collect(),
            }],
        }
    }

    #[test]
    fn heuristic_marker_ignores_type_annotations() {
        assert!(!secret_line_marker("token: string"));
        assert!(!secret_line_marker("pub token: String,"));
        assert!(!secret_line_marker("token: Option<String>"));
        assert!(secret_line_marker("token: \"abc123\""));
        assert!(secret_line_marker("api_key = \"abc123\""));
    }

    #[test]
    fn gitleaks_results_are_filtered_to_changed_lines() {
        let status = RepoStatus {
            modified: vec!["src/config.ts".into()],
            staged: Vec::new(),
            untracked: vec!["src/new.env".into()],
        };
        let diffs = vec![
            sample_diff("src/config.ts", vec![("const token = \"abc\";", Some(7))]),
            sample_diff("src/new.env", vec![("API_KEY=abc", Some(1))]),
        ];
        let report = vec![
            GitleaksReportFinding {
                file: "src/config.ts".into(),
                start_line: Some(7),
                end_line: Some(7),
                line: None,
                rule_id: Some("generic-api-key".into()),
                description: Some("Possible secret".into()),
            },
            GitleaksReportFinding {
                file: "src/config.ts".into(),
                start_line: Some(99),
                end_line: Some(99),
                line: None,
                rule_id: Some("generic-api-key".into()),
                description: Some("Possible secret".into()),
            },
            GitleaksReportFinding {
                file: "src/new.env".into(),
                start_line: Some(12),
                end_line: Some(12),
                line: None,
                rule_id: Some("generic-api-key".into()),
                description: Some("Possible secret".into()),
            },
        ];

        let findings = filter_report_findings(
            Path::new("/repo"),
            &status,
            &diffs,
            &changed_paths(&status, &diffs),
            report,
        );

        assert_eq!(findings.len(), 2);
        assert_eq!(normalize_path(&findings[0].path), "src/config.ts");
        assert_eq!(findings[0].line, 7);
        assert_eq!(normalize_path(&findings[1].path), "src/new.env");
        assert_eq!(findings[1].line, 12);
    }

    #[test]
    fn picks_linux_x64_release_asset() {
        let release = GithubRelease {
            tag_name: "v8.24.2".into(),
            assets: vec![
                GithubReleaseAsset {
                    name: "gitleaks_8.24.2_checksums.txt".into(),
                    browser_download_url: "https://example.invalid/checksums".into(),
                },
                GithubReleaseAsset {
                    name: format!(
                        "gitleaks_8.24.2_{}_{}.{}",
                        target_os_tokens()[0],
                        target_arch_tokens()[0],
                        if cfg!(target_os = "windows") {
                            "zip"
                        } else {
                            "tar.gz"
                        }
                    ),
                    browser_download_url: "https://example.invalid/archive".into(),
                },
            ],
        };

        let asset = pick_gitleaks_asset(&release).expect("asset should match current target");
        assert_eq!(asset.version, "8.24.2");
        assert!(asset.name.contains(target_os_tokens()[0]));
        assert!(target_arch_tokens()
            .iter()
            .any(|token| asset.name.contains(token)));
    }

    #[test]
    fn extracts_release_assets_from_public_html() {
        let html = r#"
            <li><a href="/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_linux_x64.tar.gz">gitleaks_8.30.1_linux_x64.tar.gz</a></li>
            <li><a href="/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_windows_x64.zip">gitleaks_8.30.1_windows_x64.zip</a></li>
        "#;

        let assets = extract_release_assets_from_html(html);

        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].name, "gitleaks_8.30.1_linux_x64.tar.gz");
        assert_eq!(
            assets[0].browser_download_url,
            "https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_linux_x64.tar.gz"
        );
    }
}
