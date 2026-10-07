//! How each agent CLI runs as a one-shot job, and how its output is read.
//!
//! - Codex: `codex exec --json … --output-schema <file> -o <file> -`.
//! - Claude Code: `claude -p --output-format stream-json … --json-schema <json>`.
//! - Shell: a plain command, for bootstrap and checks.
//!
//! The job ends when the process exits; nothing here depends on turns.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::model::{
    DeliveryAccess, DeliveryAgent, DeliveryJob, DeliveryJobResult, DeliveryLogEntry, DeliveryTask,
};
use super::tasks::plain_path;
use super::DeliveryError;

/// Shape of the result every agent job ends with; Codex and Claude both
/// enforce it on the final answer.
pub const RESULT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "status": { "type": "string", "enum": ["pass", "findings", "blocked"] },
    "summary": { "type": "string" },
    "changed_paths": { "type": "array", "items": { "type": "string" } },
    "checks": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": { "command": { "type": "string" }, "result": { "type": "string" } },
        "required": ["command", "result"],
        "additionalProperties": false
      }
    },
    "findings": { "type": "array", "items": { "type": "string" } },
    "handoff": { "type": "string" }
  },
  "required": ["status", "summary", "changed_paths", "checks", "findings", "handoff"],
  "additionalProperties": false
}"#;

/// Files a job writes under `<config>/delivery/jobs/<job id>/`.
#[derive(Debug, Clone)]
pub struct JobPaths {
    pub dir: PathBuf,
    pub events: PathBuf,
    pub stderr: PathBuf,
    pub result: PathBuf,
    pub schema: PathBuf,
}

impl JobPaths {
    pub fn for_job(job_id: &str) -> Result<Self, DeliveryError> {
        let base = crate::runtime_paths::tinto_config_dir().ok_or_else(|| {
            DeliveryError::new("delivery_store_unavailable", "config directory unavailable")
        })?;
        Ok(Self::in_dir(
            base.join("delivery").join("jobs").join(job_id),
        ))
    }

    pub fn in_dir(dir: PathBuf) -> Self {
        Self {
            events: dir.join("events.jsonl"),
            stderr: dir.join("stderr.log"),
            result: dir.join("result.json"),
            schema: dir.join("result.schema.json"),
            dir,
        }
    }

    pub fn prepare(&self) -> Result<(), DeliveryError> {
        std::fs::create_dir_all(&self.dir).map_err(DeliveryError::io)?;
        std::fs::write(&self.schema, RESULT_SCHEMA).map_err(DeliveryError::io)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLaunch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub stdin: String,
    /// Known before the process starts (Claude takes it as an argument).
    pub provider_session_id: Option<String>,
}

/// The instructions an agent job receives: Tinto's envelope around the
/// user's or coordinator's assignment.
pub fn job_prompt(job: &DeliveryJob, task: &DeliveryTask) -> String {
    let mut lines = vec![
        "You are running as a background job in Tinto Delivery mode.".to_string(),
        format!("Task: {} — {}", task.key, task.title),
        format!(
            "Role: {} (attempt {}, contract v{})",
            job.role, job.attempt, job.contract_version
        ),
        format!(
            "Working directory: {} (branch {})",
            plain_path(&task.worktree),
            task.branch
        ),
        "Other agents and the user may be working in other worktrees. Do not touch anything outside this directory, and preserve changes you did not make.".to_string(),
    ];
    if !job.writes {
        lines.push("This job is read-only: do not modify, create or delete files.".to_string());
    }
    lines.push(
        "Do not commit, push, open pull requests or change issue trackers: Tinto asks the user for those steps."
            .to_string(),
    );
    lines.push(
        "Finish with the structured result: status (pass, findings or blocked), summary, changed_paths, checks you ran with their results, findings, and handoff notes for the next stage."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("Assignment:".to_string());
    lines.push(job.prompt.trim().to_string());
    lines.join("\n")
}

/// Builds the command for a job. `claude_distro` is the WSL distro used when
/// Claude Code is not installed natively.
pub fn launch(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
    claude_distro: Option<&str>,
) -> Result<JobLaunch, DeliveryError> {
    match job.agent {
        DeliveryAgent::Codex => codex_launch(job, task, paths),
        DeliveryAgent::Claude => claude_launch(job, task, paths, claude_distro),
        DeliveryAgent::Shell => shell_launch(job, task, paths),
    }
}

fn codex_launch(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
) -> Result<JobLaunch, DeliveryError> {
    let sandbox = match job.access {
        DeliveryAccess::Workspace => "workspace-write",
        DeliveryAccess::Full => "danger-full-access",
    };
    let prompt = job_prompt(job, task);
    match task.distro.as_deref() {
        None => {
            let program = crate::agent_console::validation::resolve_agent_binary("codex")?;
            let mut args: Vec<String> = [
                "exec",
                "--json",
                "-C",
                &plain_path(&task.worktree),
                "-s",
                sandbox,
                "--output-schema",
                &plain_path(&paths.schema),
                "-o",
                &plain_path(&paths.result),
            ]
            .iter()
            .map(|value| value.to_string())
            .collect();
            if let Some(model) = job.model.as_deref() {
                args.extend(["-m".to_string(), model.to_string()]);
            }
            args.push("-".to_string());
            Ok(JobLaunch {
                program,
                args,
                cwd: Some(PathBuf::from(plain_path(&task.worktree))),
                stdin: prompt,
                provider_session_id: None,
            })
        }
        Some(distro) => {
            let model = job
                .model
                .as_deref()
                .map(|model| format!(" -m {}", sh_quote(model)))
                .unwrap_or_default();
            let script = format!(
                "exec codex exec --json -C {} -s {sandbox} --output-schema {} -o {}{model} -",
                sh_quote(&task.worktree.to_string_lossy()),
                sh_quote(&wsl_path(&paths.schema)?),
                sh_quote(&wsl_path(&paths.result)?),
            );
            Ok(wsl_launch(
                distro,
                &task.worktree.to_string_lossy(),
                script,
                prompt,
                None,
            ))
        }
    }
}

fn claude_launch(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
    claude_distro: Option<&str>,
) -> Result<JobLaunch, DeliveryError> {
    let mode = match job.access {
        DeliveryAccess::Workspace => "acceptEdits",
        DeliveryAccess::Full => "bypassPermissions",
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let prompt = job_prompt(job, task);
    let native = match task.distro {
        None => crate::agent_console::validation::resolve_agent_binary("claude").ok(),
        Some(_) => None,
    };
    if let Some(program) = native {
        let mut args: Vec<String> = [
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-mode",
            mode,
            "--session-id",
            &session_id,
            "--json-schema",
            RESULT_SCHEMA,
        ]
        .iter()
        .map(|value| value.to_string())
        .collect();
        if let Some(model) = job.model.as_deref() {
            args.extend(["--model".to_string(), model.to_string()]);
        }
        return Ok(JobLaunch {
            program,
            args,
            cwd: Some(PathBuf::from(plain_path(&task.worktree))),
            stdin: prompt,
            provider_session_id: Some(session_id),
        });
    }
    let (distro, cwd) = match task.distro.as_deref() {
        Some(distro) => (distro.to_string(), task.worktree.to_string_lossy().into_owned()),
        None => (
            claude_distro
                .ok_or_else(|| {
                    DeliveryError::new(
                        "binary_not_found",
                        "Claude Code no está instalado en Windows ni hay una distro WSL para ejecutarlo",
                    )
                })?
                .to_string(),
            wsl_path(&task.worktree)?,
        ),
    };
    let model = job
        .model
        .as_deref()
        .map(|model| format!(" --model {}", sh_quote(model)))
        .unwrap_or_default();
    let script = format!(
        "exec claude -p --output-format stream-json --verbose --permission-mode {mode} --session-id {session_id} --json-schema \"$(cat {})\"{model}",
        sh_quote(&wsl_path(&paths.schema)?)
    );
    Ok(wsl_launch(&distro, &cwd, script, prompt, Some(session_id)))
}

fn shell_launch(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
) -> Result<JobLaunch, DeliveryError> {
    if let Some(distro) = task.distro.as_deref() {
        return Ok(wsl_launch(
            distro,
            &task.worktree.to_string_lossy(),
            job.prompt.clone(),
            String::new(),
            None,
        ));
    }
    // A script file avoids cmd.exe's quoting rules for the command text.
    #[cfg(target_os = "windows")]
    let (program, script, body) = (
        PathBuf::from("cmd.exe"),
        paths.dir.join("command.cmd"),
        format!("@echo off\r\n{}\r\n", job.prompt.trim()),
    );
    #[cfg(not(target_os = "windows"))]
    let (program, script, body) = (
        PathBuf::from("sh"),
        paths.dir.join("command.sh"),
        format!("{}\n", job.prompt.trim()),
    );
    std::fs::write(&script, body).map_err(DeliveryError::io)?;
    #[cfg(target_os = "windows")]
    let args = vec!["/D".to_string(), "/C".to_string(), plain_path(&script)];
    #[cfg(not(target_os = "windows"))]
    let args = vec![plain_path(&script)];
    Ok(JobLaunch {
        program,
        args,
        cwd: Some(PathBuf::from(plain_path(&task.worktree))),
        stdin: String::new(),
        provider_session_id: None,
    })
}

fn wsl_launch(
    distro: &str,
    cwd: &str,
    script: String,
    stdin: String,
    provider_session_id: Option<String>,
) -> JobLaunch {
    JobLaunch {
        program: PathBuf::from("wsl.exe"),
        args: vec![
            "-d".to_string(),
            distro.to_string(),
            "--cd".to_string(),
            cwd.to_string(),
            "--exec".to_string(),
            "bash".to_string(),
            "-lc".to_string(),
            script,
        ],
        cwd: None,
        stdin,
        provider_session_id,
    }
}

/// The WSL distro used to run Claude Code when it is not installed on
/// Windows: the first one listed (the default), skipping Docker's.
pub fn default_wsl_distro() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("wsl.exe");
        command.args(["--list", "--quiet"]);
        let output = crate::windows_process::output_with_timeout(
            &mut command,
            std::time::Duration::from_secs(5),
        )
        .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = if output.stdout.len() % 2 == 0 && output.stdout.contains(&0) {
            let units: Vec<u16> = output
                .stdout
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            String::from_utf8_lossy(&output.stdout).into_owned()
        };
        text.lines()
            .map(|line| line.trim().trim_matches('\0').to_string())
            .find(|line| !line.is_empty() && !line.starts_with("docker-desktop"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

fn wsl_path(path: &Path) -> Result<String, DeliveryError> {
    crate::wsl_agent::launcher::windows_path_to_wsl_mount(Path::new(&plain_path(path)))
        .map_err(DeliveryError::from)
}

fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

// ---- reading the output ----

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobOutcome {
    pub provider_session_id: Option<String>,
    /// Why the agent failed, when it says so.
    pub error: Option<String>,
    pub result: Option<DeliveryJobResult>,
    /// Why the final answer was not a valid result.
    pub result_error: Option<String>,
}

pub fn session_id_from_line(agent: DeliveryAgent, line: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    match agent {
        DeliveryAgent::Codex => (value["type"] == "thread.started")
            .then(|| value["thread_id"].as_str().map(str::to_string))
            .flatten(),
        DeliveryAgent::Claude => (value["type"] == "system" && value["subtype"] == "init")
            .then(|| value["session_id"].as_str().map(str::to_string))
            .flatten(),
        DeliveryAgent::Shell => None,
    }
}

/// Readable entries for one raw output line.
pub fn log_entries(agent: DeliveryAgent, line: &str) -> Vec<DeliveryLogEntry> {
    let entry = |kind: &str, text: String| DeliveryLogEntry {
        kind: kind.to_string(),
        text,
    };
    if agent == DeliveryAgent::Shell {
        return vec![entry("info", line.to_string())];
    }
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return if line.trim().is_empty() {
            Vec::new()
        } else {
            vec![entry("info", line.to_string())]
        };
    };
    let text = |pointer: &str| value.pointer(pointer).and_then(Value::as_str);
    match agent {
        DeliveryAgent::Codex => match value["type"].as_str() {
            Some("item.completed") => match text("/item/type") {
                Some("agent_message") => vec![entry(
                    "message",
                    text("/item/text").unwrap_or_default().to_string(),
                )],
                Some("command_execution") => vec![entry(
                    "command",
                    format!(
                        "{}{}",
                        text("/item/command").unwrap_or_default(),
                        value
                            .pointer("/item/exit_code")
                            .and_then(Value::as_i64)
                            .map(|code| format!("  → {code}"))
                            .unwrap_or_default()
                    ),
                )],
                Some("file_change") => {
                    let paths = value
                        .pointer("/item/changes")
                        .and_then(Value::as_array)
                        .map(|changes| {
                            changes
                                .iter()
                                .filter_map(|change| change["path"].as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    vec![entry("command", format!("Editó {paths}"))]
                }
                Some("error") => vec![entry(
                    "info",
                    text("/item/message").unwrap_or_default().to_string(),
                )],
                _ => Vec::new(),
            },
            Some("turn.failed") => vec![entry(
                "error",
                provider_error_reason(text("/error/message").unwrap_or_default()),
            )],
            Some("error") => vec![entry(
                "error",
                provider_error_reason(text("/message").unwrap_or_default()),
            )],
            _ => Vec::new(),
        },
        DeliveryAgent::Claude => match value["type"].as_str() {
            Some("assistant") => value
                .pointer("/message/content")
                .and_then(Value::as_array)
                .map(|content| {
                    content
                        .iter()
                        .filter_map(|block| match block["type"].as_str() {
                            Some("text") => block["text"]
                                .as_str()
                                .filter(|text| !text.trim().is_empty())
                                .map(|text| entry("message", text.to_string())),
                            Some("tool_use") if block["name"] != "StructuredOutput" => {
                                Some(entry("command", claude_tool_summary(block)))
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Some("system") if value["subtype"] == "permission_denied" => vec![entry(
                "error",
                format!("Permiso denegado: {}", text("/message").unwrap_or_default()),
            )],
            Some("result") if value["is_error"] == true => vec![entry(
                "error",
                text("/result")
                    .unwrap_or("el agente terminó con error")
                    .to_string(),
            )],
            _ => Vec::new(),
        },
        DeliveryAgent::Shell => Vec::new(),
    }
}

fn claude_tool_summary(block: &Value) -> String {
    let name = block["name"].as_str().unwrap_or("herramienta");
    let input = &block["input"];
    let detail = input["command"]
        .as_str()
        .or_else(|| input["file_path"].as_str())
        .or_else(|| input["pattern"].as_str())
        .or_else(|| input["path"].as_str())
        .unwrap_or_default();
    if detail.is_empty() {
        name.to_string()
    } else {
        format!("{name}: {detail}")
    }
}

/// Reads the whole output once the process has exited.
pub fn read_outcome(agent: DeliveryAgent, events: &str, result_file: Option<&str>) -> JobOutcome {
    let mut outcome = JobOutcome::default();
    if agent == DeliveryAgent::Shell {
        return outcome;
    }
    let mut last_message: Option<String> = None;
    let mut final_value: Option<Value> = None;
    for line in events.lines() {
        if outcome.provider_session_id.is_none() {
            outcome.provider_session_id = session_id_from_line(agent, line);
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match agent {
            DeliveryAgent::Codex => match value["type"].as_str() {
                Some("item.completed") if value["item"]["type"] == "agent_message" => {
                    last_message = value["item"]["text"].as_str().map(str::to_string);
                }
                Some("turn.failed") => {
                    outcome.error = value["error"]["message"]
                        .as_str()
                        .map(provider_error_reason);
                }
                Some("error") => {
                    outcome.error = value["message"].as_str().map(provider_error_reason);
                }
                _ => {}
            },
            DeliveryAgent::Claude => {
                if value["type"] == "result" {
                    if value["is_error"] == true {
                        outcome.error = Some(
                            value["result"]
                                .as_str()
                                .unwrap_or("el agente terminó con error")
                                .to_string(),
                        );
                    }
                    if value["structured_output"].is_object() {
                        final_value = Some(value["structured_output"].clone());
                    }
                    last_message = value["result"].as_str().map(str::to_string);
                }
            }
            DeliveryAgent::Shell => {}
        }
    }
    let candidate = final_value
        .or_else(|| result_file.and_then(json_object_in))
        .or_else(|| last_message.as_deref().and_then(json_object_in));
    match candidate {
        Some(value) => match parse_result(&value) {
            Ok(result) => outcome.result = Some(result),
            Err(error) => outcome.result_error = Some(error),
        },
        None => {
            outcome.result_error =
                Some("el agente no entregó el resultado estructurado".to_string())
        }
    }
    outcome
}

pub fn parse_result(value: &Value) -> Result<DeliveryJobResult, String> {
    let result: DeliveryJobResult = serde_json::from_value(value.clone())
        .map_err(|error| format!("resultado con formato inválido: {error}"))?;
    if !matches!(result.status.as_str(), "pass" | "findings" | "blocked") {
        return Err(format!(
            "estado de resultado desconocido: {}",
            result.status
        ));
    }
    if result.summary.trim().is_empty() {
        return Err("el resultado no tiene resumen".to_string());
    }
    Ok(result)
}

/// The JSON object in a final answer, tolerating code fences or prose
/// around it.
fn json_object_in(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start)
        .then(|| serde_json::from_str::<Value>(&text[start..=end]).ok())
        .flatten()
        .filter(Value::is_object)
}

/// Codex wraps API errors as JSON text; keep the human-readable message.
fn provider_error_reason(error: &str) -> String {
    serde_json::from_str::<Value>(error)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::model::{DeliveryJobStatus, DeliveryResultState};

    const CODEX_OK: &str = include_str!("fixtures/codex-ok.jsonl");
    const CODEX_FAILED: &str = include_str!("fixtures/codex-failed.jsonl");
    const CLAUDE_OK: &str = include_str!("fixtures/claude-ok.jsonl");

    pub(crate) fn sample_task() -> DeliveryTask {
        DeliveryTask {
            id: "t1".into(),
            run_id: None,
            repo: PathBuf::from("/repo"),
            distro: None,
            key: "K-1".into(),
            title: "Do the thing".into(),
            worktree: PathBuf::from("/repo-wt/K-1"),
            branch: "delivery/k-1".into(),
            base_ref: "main".into(),
            base_commit: "abc".into(),
            state: "intake".into(),
            contract_version: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
            removed_at_ms: None,
        }
    }

    pub(crate) fn sample_job(agent: DeliveryAgent) -> DeliveryJob {
        DeliveryJob {
            id: "j1".into(),
            task_id: "t1".into(),
            role: "tests".into(),
            attempt: 1,
            agent,
            model: Some("gpt-6-astra".into()),
            access: DeliveryAccess::Workspace,
            prompt: "Write the failing test.".into(),
            contract_version: 1,
            writes: true,
            lease: None,
            timeout_minutes: 30,
            status: DeliveryJobStatus::Queued,
            created_at_ms: 1,
            started_at_ms: None,
            ended_at_ms: None,
            pid: None,
            provider_session_id: None,
            exit_code: None,
            error: None,
            start_candidate: None,
            end_candidate: None,
            changes: Vec::new(),
            result: None,
            result_state: None::<DeliveryResultState>,
            result_note: None,
            undone_at_ms: None,
        }
    }

    #[test]
    fn codex_success_yields_thread_and_result() {
        let outcome = read_outcome(DeliveryAgent::Codex, CODEX_OK, None);
        assert_eq!(
            outcome.provider_session_id.as_deref(),
            Some("01a116a2-cd4d-7e31-a43d-b697759df531")
        );
        assert_eq!(outcome.error, None);
        let result = outcome.result.expect("result");
        assert_eq!(result.status, "pass");
        assert_eq!(result.changed_paths, vec!["greeting.txt".to_string()]);
    }

    #[test]
    fn codex_failure_reports_the_api_reason() {
        let outcome = read_outcome(DeliveryAgent::Codex, CODEX_FAILED, None);
        assert_eq!(
            outcome.error.as_deref(),
            Some(
                "The 'gpt-6.1-sol' model is not supported when using Codex with a ChatGPT account."
            )
        );
        assert!(outcome.result.is_none());
    }

    #[test]
    fn claude_structured_output_is_the_result() {
        let outcome = read_outcome(DeliveryAgent::Claude, CLAUDE_OK, None);
        assert_eq!(
            outcome.provider_session_id.as_deref(),
            Some("dab8d3e7-fcb8-4652-88f1-55219349b8d0")
        );
        assert_eq!(outcome.result.expect("result").status, "pass");
        let entries: Vec<_> = CLAUDE_OK
            .lines()
            .flat_map(|line| log_entries(DeliveryAgent::Claude, line))
            .collect();
        assert!(entries
            .iter()
            .any(|entry| entry.kind == "command" && entry.text.starts_with("Bash: ")));
        assert!(entries
            .iter()
            .any(|entry| entry.kind == "error" && entry.text.starts_with("Permiso denegado")));
    }

    #[test]
    fn results_are_validated() {
        assert!(parse_result(&serde_json::json!({"status": "done", "summary": "x"})).is_err());
        assert!(parse_result(&serde_json::json!({"status": "pass", "summary": " "})).is_err());
        assert_eq!(
            json_object_in("Here you go:\n```json\n{\"a\":1}\n```"),
            Some(serde_json::json!({"a": 1}))
        );
        let outcome = read_outcome(DeliveryAgent::Codex, "", Some("not json"));
        assert!(outcome.result_error.is_some());
    }

    #[test]
    fn launches_carry_sandbox_schema_and_prompt() {
        let mut task = sample_task();
        task.distro = Some("Ubuntu".into());
        let job = sample_job(DeliveryAgent::Codex);
        let paths = JobPaths::in_dir(PathBuf::from(
            r"C:\Users\me\AppData\Roaming\tinto\delivery\jobs\j1",
        ));
        let launch = codex_launch(&job, &task, &paths).unwrap();
        assert_eq!(launch.program, PathBuf::from("wsl.exe"));
        let script = launch.args.last().unwrap();
        assert!(script.contains("-s workspace-write"), "{script}");
        assert!(script.contains("--output-schema '/mnt/c/Users/me/AppData/Roaming/tinto/delivery/jobs/j1/result.schema.json'"), "{script}");
        assert!(script.contains(" -m 'gpt-6-astra' -"), "{script}");
        assert!(launch
            .stdin
            .contains("Assignment:\nWrite the failing test."));

        let mut claude = sample_job(DeliveryAgent::Claude);
        claude.access = DeliveryAccess::Full;
        claude.writes = false;
        let launch = claude_launch(&claude, &task, &paths, None).unwrap();
        let script = launch.args.last().unwrap();
        assert!(
            script.contains("--permission-mode bypassPermissions"),
            "{script}"
        );
        assert!(launch.provider_session_id.is_some());
        assert!(launch.stdin.contains("This job is read-only"));
    }

    #[test]
    fn shell_quoting_survives_single_quotes() {
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
    }
}
