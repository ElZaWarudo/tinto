//! How each agent CLI runs as a one-shot job, and how its output is read.
//!
//! - Codex: `codex exec --json … --output-schema <file> -o <file> -`.
//! - Claude Code: `claude -p --output-format stream-json … --json-schema <json>`.
//! - Shell: a plain command, for bootstrap and checks.
//!
//! The job ends when the process exits; nothing here depends on turns.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::{
    DeliveryAccess, DeliveryAgent, DeliveryJob, DeliveryJobResult, DeliveryLogEntry,
    DeliveryPlanItem, DeliveryPlatform, DeliveryQuota, DeliverySlowCommand, DeliveryTask,
    DeliveryTiming, DeliveryUsage,
};
use super::service::{style_check, QA_CLI_LEASE, QA_ROLE};
use super::tasks::{git, plain_path, Place};
use super::DeliveryError;

/// The browser QA jobs get, pinned so a new release cannot change a run.
pub const PLAYWRIGHT_MCP: &str = "@playwright/mcp@0.0.82";
const BROWSER_SERVER: &str = "playwright";
/// How long a browser tool call may go without a word before the agent's
/// CLI stops it: a page that never loads would otherwise hold the job for
/// the CLI's own limit (30 minutes for Claude Code).
pub const BROWSER_TOOL_SILENCE_SECS: u32 = 180;
/// The share of a QA job's time limit after which it is told to stop
/// testing and write its record.
pub const QA_BUDGET_PERCENT: u32 = 70;

/// Shape of the result every agent job ends with; Codex and Claude both
/// enforce it on the final answer. Results of about 8 KB failed to parse
/// as Claude's StructuredOutput call, so it states a size budget; and
/// agents listed what they fixed as findings, which kept done rounds open,
/// so that goes in `addressed`.
pub const RESULT_SCHEMA: &str = r#"{
  "type": "object",
  "description": "Keep the whole result under 4000 characters: summary and each finding or addressed item one or two short sentences; any longer detail goes in handoff, at most 1500 characters.",
  "properties": {
    "status": { "type": "string", "enum": ["pass", "findings", "blocked"] },
    "summary": { "type": "string", "description": "At most three sentences." },
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
    "findings": { "type": "array", "description": "Only defects still open, one short line each: never what was fixed, addressed, verified or found fine.", "items": { "type": "string" } },
    "addressed": { "type": "array", "description": "Each item of the round or earlier finding dealt with: the item, what was done and where (file:line), one short line each.", "items": { "type": "string" } },
    "handoff": { "type": "string" },
    "blocked_by": { "type": "string", "enum": ["none", "environment", "sandbox"] }
  },
  "required": ["status", "summary", "changed_paths", "checks", "findings", "addressed", "handoff", "blocked_by"],
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
    /// Muse Code reads its prompt from a file.
    pub prompt: PathBuf,
    /// Where a QA job's browser saves screenshots, outside the worktree.
    pub qa_output: PathBuf,
    pub qa_browser_script: PathBuf,
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
            prompt: dir.join("prompt.md"),
            qa_output: dir.join("qa"),
            qa_browser_script: dir.join("qa-browser.mjs"),
            dir,
        }
    }

    pub fn prepare(&self) -> Result<(), DeliveryError> {
        std::fs::create_dir_all(&self.qa_output).map_err(DeliveryError::io)?;
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
    /// Set for jobs that run in WSL, where closing `wsl.exe` does not stop
    /// the Linux processes.
    pub wsl_job: Option<WslJob>,
    /// Something about how the job runs that its stage should say.
    pub note: Option<String>,
    /// Files the job needs in its folder, written when it starts.
    pub files: Vec<JobFile>,
    /// Variables the agent starts with (a job on this machine's filesystem;
    /// a WSL job exports them in its script).
    pub env: Vec<(String, String)>,
}

/// A file a job needs, as its path and contents.
pub type JobFile = (PathBuf, String);

/// A job running in WSL. Every process it starts inherits
/// `TINTO_DELIVERY_JOB=<job_id>`, which is how [`stop_wsl_job`] finds them,
/// including commands the agent runs in their own process groups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WslJob {
    pub distro: String,
    pub job_id: String,
}

/// A Windows-side path as the job's agent names it: `/mnt/c/…` inside WSL,
/// forward slashes otherwise (backslashes do not survive a shell).
fn agent_path(path: &str, from_wsl: bool) -> String {
    if from_wsl {
        if let Ok(mounted) = wsl_path(Path::new(path)) {
            return mounted;
        }
    }
    path.replace('\\', "/")
}

/// The job's QA folder as its agent names it.
fn qa_folder(paths: &JobPaths, from_wsl: bool) -> String {
    agent_path(&plain_path(&paths.qa_output), from_wsl)
}

/// An absolute path as Claude Code's file rules spell it: `//c/Users/…` for
/// `C:/Users/…`, `//home/…` for `/home/…`.
fn claude_rule_path(path: &str) -> String {
    match path.split_once(":/") {
        Some((drive, rest)) if drive.len() == 1 => {
            format!("//{}/{rest}", drive.to_ascii_lowercase())
        }
        _ => format!("/{path}"),
    }
}

/// The commands a job may run, naming the frozen copies of the approved
/// scripts instead of the originals.
fn job_commands(job: &DeliveryJob, from_wsl: bool) -> Vec<String> {
    job.allowed_commands
        .iter()
        .map(|command| {
            job.frozen_scripts
                .iter()
                .fold(command.clone(), |command, script| {
                    command.replace(&script.original, &agent_path(&script.copy, from_wsl))
                })
        })
        .collect()
}

/// The instructions an agent job receives: Tinto's envelope around the
/// user's or coordinator's assignment. `from_wsl` says whether it runs in
/// WSL, which changes how it names Windows paths.
pub fn job_prompt(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
    from_wsl: bool,
    obstacles: &[String],
) -> String {
    let mut lines = vec![
        "You are running as a background job in Tinto Delivery mode.".to_string(),
        format!("Task: {} — {}", task.label(), task.title),
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
    if let Some(package) = &task.package {
        let part = match package.title.trim() {
            "" => String::new(),
            title => format!(" ({title})"),
        };
        lines.push(format!(
            "This is package {} of {} of issue {}{part}: deliver only this package's part; the other packages are separate tasks with their own pull requests.",
            package.index, package.of, task.key
        ));
    }
    if !job.writes {
        lines.push("This job is read-only: do not modify, create or delete files.".to_string());
        if job.role != QA_ROLE {
            lines.push(format!(
                "You can read the main checkout at {} (for example the task's contract under .agent). Do not change it.",
                agent_path(&plain_path(&task.repo), from_wsl)
            ));
        }
    }
    if job.role == QA_ROLE {
        lines.push(format!(
            "This is the QA stage: you have network access and can read the main checkout at {} (for example the project's local QA configuration). Do not change it.",
            plain_path(&task.repo)
        ));
        lines.push(format!(
            "Your QA folder is {}. It is the one place where you can create files: write your evidence and output files there and name them in the result's checks.",
            qa_folder(paths, from_wsl)
        ));
        if !job.qa_environment.trim().is_empty() {
            lines.push(format!(
                "QA environment the user set for this repo (test there; anything beyond it is out of scope): {}",
                job.qa_environment.trim()
            ));
        }
        if let (Some(stack), Some(prepare)) = (&job.qa_stack, &job.prepare) {
            lines.push(format!(
                "Tinto prepared the QA stack «{}» before you started, and it serves this worktree at {}: {} Do not rebuild, restart or reconfigure it. If it stops answering, report what depends on it as blocked by the environment.",
                stack.name,
                stack.ready_url,
                prepare.note.trim()
            ));
        }
        if let Some(project) = job
            .lease
            .as_deref()
            .and_then(|lease| lease.strip_prefix(QA_CLI_LEASE))
        {
            lines.push(format!(
                "This is a command-line QA without a browser. Run it on its own isolated environment named «{project}» (for example a compose project of that name) and remove it when you end; do not use or change the repo's shared stack, which another QA may be testing on."
            ));
        }
        if job.qa_browser {
            lines.push(format!(
                "You have a headless browser with a fresh profile through the {BROWSER_SERVER} MCP tools. The screenshots it saves go to the job's folder, outside the worktree; name them in the result's checks. Its console logs and page snapshots are saved in your QA folder too: read them there, or with the browser_console_messages tool, and never search the disk for them."
            ));
            lines.push(format!(
                "Work the browser in short steps: browser_navigate, browser_snapshot, browser_click and browser_type, and browser_wait_for with a text to wait for and a time limit of at most 30 seconds. Do not write browser_run_code_unsafe scripts that wait on page loads or sleep: a browser call that says nothing for {} minutes is stopped. If a page does not load within a minute, check the dev server's logs once and report it as blocked by the environment instead of retrying. Check the visual changes first and the regressions last.",
                BROWSER_TOOL_SILENCE_SECS / 60
            ));
        }
        if !job.qa_followup.is_empty() || !job.round.is_empty() {
            lines.push("This QA re-checks a round: check only the items the round and the earlier problems below list, not the whole feature, unless the assignment asks for more.".to_string());
        }
        let budget = job.timeout_minutes * QA_BUDGET_PERCENT / 100;
        lines.push(format!(
            "Your time limit is {} minutes. By minute {budget}, stop testing and write `{}.md` with what you have, marking each item you did not check «not verifiable: out of time»: a QA without its record is lost. If time runs out anyway, Tinto writes a stub record from your screenshots and your task list.",
            job.timeout_minutes, task.key
        ));
        lines.push(qa_record_guide(&task.key));
        if job.qa_browser {
            lines.push(visual_record_instructions(&task.key));
        }
    }
    // The task's planning folders, which every stage reads, apart from the
    // other folders it may read.
    let (planning, other): (Vec<&String>, Vec<&String>) = job
        .read_dirs
        .iter()
        .partition(|dir| task.read_dirs.contains(dir));
    let named = |dirs: Vec<&String>| -> String {
        let dirs: Vec<String> = dirs.iter().map(|dir| agent_path(dir, from_wsl)).collect();
        dirs.join("; ")
    };
    if !planning.is_empty() {
        lines.push(format!(
            "The task's planning folders, outside the worktree (its contract and context): read what they say about this task before you start, and do not change them: {}.",
            named(planning)
        ));
    }
    if !other.is_empty() {
        lines.push(format!(
            "You may also read and list these folders outside the worktree, without changing them: {}.",
            named(other)
        ));
    }
    if !job.write_dirs.is_empty() {
        let dirs: Vec<String> = job
            .write_dirs
            .iter()
            .map(|dir| agent_path(dir, from_wsl))
            .collect();
        lines.push(format!(
            "The user lets the commands they approved write in these folders outside the worktree, and nowhere else: {}.",
            dirs.join("; ")
        ));
    }
    if !obstacles.is_empty() {
        lines.push(format!(
            "Known limits of your sandbox on this machine: {}. Do not work around them: report what depends on them as blocked by the sandbox.",
            obstacles.join("; ")
        ));
    }
    if !job.frozen_scripts.is_empty() {
        lines.push("The user approved these scripts as frozen copies. The originals may have changed since and must not be run: wherever the assignment names an original, run its copy.".to_string());
        // A script two permissions share is copied once per permission, next
        // to its siblings; the first copy is the one to name.
        let mut listed: Vec<&str> = Vec::new();
        for script in &job.frozen_scripts {
            if !listed.contains(&script.original.as_str()) {
                listed.push(&script.original);
                lines.push(format!(
                    "- {} → {}",
                    script.original,
                    agent_path(&script.copy, from_wsl)
                ));
            }
        }
    }
    if job.agent == DeliveryAgent::Claude && job.access == DeliveryAccess::Workspace {
        lines.push(format!(
            "You can run any shell command, compound ones included (pipes, `&&`, `$(…)`). Tinto refuses only these, which are its own steps or reach beyond your task: {}.",
            CLAUDE_DENIED.join(", ")
        ));
        let commands = job_commands(job, from_wsl);
        if !commands.is_empty() {
            lines.push(format!("The repo's checks: {}.", commands.join("; ")));
        }
        if job.role != QA_ROLE {
            lines.push("You have no web tools (WebFetch, WebSearch): work from the code, the dependencies on disk and the task's folders.".to_string());
        }
        lines.push("Run commands in the foreground and read their output when they end, with a time limit (`timeout`) on a slow one. Start one in the background only when it outlasts your shell's limit, and then wait for it once, not with repeated polls. To wait for a server you started, use one bounded readiness check (for example `curl --retry 30 --retry-delay 2 --retry-all-errors <url>`).".to_string());
    }
    let full = full_checks(job, task);
    if matches!(job.agent, DeliveryAgent::Claude | DeliveryAgent::Opencode) && !full.is_empty() {
        let listed: Vec<String> = full.iter().map(|check| format!("`{check}`")).collect();
        lines.push(format!(
            "Tinto refuses these checks exactly as written, since it runs them itself on your result: {}. To test what you change, run the test runner on just those files or tests (for example `npx mocha -r ts-node/register <spec files>`, `npx vitest run <files>`, `cargo test <test name>`, `pytest <files>`).",
            listed.join("; ")
        ));
    }
    if let Some(merge) = &job.merge_conflicts {
        let files = merge.files.join(", ");
        lines.push(if job.writes {
            format!(
                "Tinto is merging {} into this branch and the merge stopped on conflicts in: {files}. Resolve each one first: edit the file so it keeps what both sides meant, and remove every conflict marker (<<<<<<<, =======, >>>>>>>). Do not run git merge, git add, git commit, git reset or git checkout: the merge stays in progress and Tinto concludes it with the commit step.",
                merge.base
            )
        } else {
            format!(
                "Tinto is merging {} into this branch; it still has conflicts in: {files}.",
                merge.base
            )
        });
    }
    lines.extend(job.task_checks.iter().cloned());
    lines.extend(job.round.iter().cloned());
    if job.role == QA_ROLE && !job.qa_followup.is_empty() {
        lines.push("Problems from earlier QA the user decided on:".to_string());
        lines.extend(job.qa_followup.iter().map(|note| format!("- {note}")));
    }
    if !job.decisions.is_empty() {
        lines.push("Decisions the user made for this task (follow them):".to_string());
        lines.extend(job.decisions.iter().map(|note| format!("- {note}")));
    }
    if !job.guide_rules.is_empty() {
        lines.push(
            "Rules the repo's reviewers keep asking for (follow each where it applies):"
                .to_string(),
        );
        lines.extend(job.guide_rules.iter().map(|rule| format!("- {rule}")));
    }
    // Each finding sends the work back for one more round of fix, checks
    // and review: only what has to change earns one.
    if job.role == "review" || job.role == QA_ROLE {
        lines.push("Findings are only what must change before this merges: a defect in what the change does, a requirement of the task it misses, or a requirement it adds without a test. Put the rest in handoff, not in findings: low-severity notes, style preferences, hardening against failures the task does not ask about, and what you checked and found fine. With only those, the status is pass.".to_string());
    }
    if job.role == QA_ROLE {
        lines.push("Test what the task asks for. Failures it does not ask about (a service stopped or paused, a provider's limits) are observations in the record, not problems.".to_string());
    }
    if job.role == "review" && !job.round.is_empty() {
        lines.push("This review re-checks a round: check that its items were addressed and that the fix broke nothing near them; do not review the rest of the change again.".to_string());
    }
    lines.push(
        "Do not commit, push, open pull requests or change issue trackers: Tinto asks the user for those steps."
            .to_string(),
    );
    if !job.round.is_empty() {
        lines.push("In the result, list in `addressed` each round item you dealt with (the item, what you did, file:line). `findings` holds only defects still open: never what you fixed, addressed or verified.".to_string());
    }
    lines.push(
        "Finish with the structured result: status (pass, findings or blocked), summary, changed_paths, checks you ran with their results, findings (only defects still open), addressed (what you fixed or confirmed), and handoff notes for the next stage. Keep it short, under 4000 characters: one or two sentences per finding, the detail in handoff."
            .to_string(),
    );
    lines.push(
        "Set blocked_by to say what stopped you, if anything: `sandbox` when your own sandbox or permission rules refused something a decision above allows (name the command and the exact error in findings), `environment` when something outside them failed (a service down, a missing tool, a login), `none` otherwise."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("Assignment:".to_string());
    lines.push(job.prompt.trim().to_string());
    // Coordinators keep asking jobs for full builds and suites Tinto runs
    // anyway, often three times per stage: Tinto's word is the last one.
    if !job.task_checks.is_empty() {
        lines.push(String::new());
        lines.push(
            "If the assignment asks you to run the task's checks above in full (builds, type checkers, test suites), do not: Tinto runs them on your result. Run only the tests of what you change, and the format and lint checks Tinto asked you to run above."
                .to_string(),
        );
    }
    lines.join("\n")
}

/// Builds the command for a job. `claude_distro` is the WSL distro a Claude
/// job on a Windows repo runs in when it cannot or should not run on Windows.
pub fn launch(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
    claude_distro: Option<&str>,
) -> Result<JobLaunch, DeliveryError> {
    match job.agent {
        DeliveryAgent::Codex => {
            let native = match task.distro {
                Some(_) => None,
                None => Some(crate::agent_console::validation::resolve_agent_binary(
                    "codex",
                )?),
            };
            let plan = match (&native, job.access) {
                (Some(_), DeliveryAccess::Workspace) => {
                    codex_sandbox_plan(task, &command_programs(&job.allowed_commands))
                }
                _ => SandboxPlan::default(),
            };
            codex_launch(job, task, paths, native, &plan)
        }
        DeliveryAgent::Claude => {
            let native = match (task.distro.as_deref(), job.platform) {
                (Some(_), _) | (None, Some(DeliveryPlatform::Wsl)) => None,
                (None, _) => claude_binary(),
            };
            claude_launch(job, task, paths, native, claude_distro)
        }
        DeliveryAgent::Muse => {
            let native = match task.distro {
                Some(_) => None,
                None => Some(crate::agent_console::validation::resolve_agent_binary(
                    "muse",
                )?),
            };
            muse_launch(job, task, paths, native)
        }
        DeliveryAgent::Opencode => {
            let native = match task.distro {
                Some(_) => None,
                None => Some(crate::agent_console::validation::resolve_agent_binary(
                    "opencode",
                )?),
            };
            opencode_launch(job, task, paths, native)
        }
        DeliveryAgent::Shell => shell_launch(job, task, paths),
    }
}

/// Whether the agent's CLI is installed on this machine (outside WSL).
pub fn installed_on_windows(agent: DeliveryAgent) -> bool {
    match agent {
        DeliveryAgent::Codex => {
            crate::agent_console::validation::resolve_agent_binary("codex").is_ok()
        }
        DeliveryAgent::Claude => claude_binary().is_some(),
        DeliveryAgent::Muse => {
            crate::agent_console::validation::resolve_agent_binary("muse").is_ok()
        }
        DeliveryAgent::Opencode => {
            crate::agent_console::validation::resolve_agent_binary("opencode").is_ok()
        }
        DeliveryAgent::Shell => true,
    }
}

/// Claude Code on this machine: on PATH or, on Windows, the one the Claude
/// desktop app installed.
fn claude_binary() -> Option<PathBuf> {
    let found = crate::agent_console::validation::resolve_agent_binary("claude").ok();
    #[cfg(windows)]
    let found = found.or_else(|| {
        let appdata = std::env::var_os("APPDATA")?;
        crate::agent_console::validation::claude_desktop_binary(Path::new(&appdata))
    });
    found
}

/// The programs that commands start (`npm` for `npm test`), a quoted path
/// included, each once.
pub fn command_programs(commands: &[String]) -> Vec<String> {
    let mut programs: Vec<String> = Vec::new();
    for command in commands {
        let command = command.trim();
        let program = match command.strip_prefix('"') {
            Some(rest) => rest.split('"').next(),
            None => command.split_whitespace().next(),
        };
        if let Some(program) = program.filter(|program| !program.is_empty()) {
            if !programs.iter().any(|seen| seen == program) {
                programs.push(program.to_string());
            }
        }
    }
    programs
}

/// How a Codex job's Windows sandbox has to run for the programs it needs.
/// Codex has two: the configured one (by default "elevated", which runs as
/// another user and cannot read folders that only grant the current user,
/// such as a Node installed by nvm) and the unelevated one (the current
/// user with a restricted token, in which Git Bash cannot start).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SandboxPlan {
    /// Run in the unelevated sandbox instead of the configured one.
    pub unelevated: bool,
    /// Why, for the job's stage.
    pub note: Option<String>,
    /// What cannot run in the chosen sandbox, in the user's words.
    pub obstacles: Vec<String>,
}

/// Chooses the sandbox from the programs that failed to start in each one.
#[cfg(any(windows, test))]
fn sandbox_plan_from(configured: &[String], unelevated: &[String]) -> SandboxPlan {
    if configured.is_empty() {
        return SandboxPlan::default();
    }
    let use_unelevated = unelevated.len() < configured.len();
    let (failed, other, other_name) = if use_unelevated {
        (unelevated, configured, "configurado (elevado)")
    } else {
        (configured, unelevated, "sin elevar")
    };
    let obstacles = failed
        .iter()
        .map(|program| {
            let only_other: Vec<&str> = other
                .iter()
                .filter(|other| !failed.contains(other))
                .map(String::as_str)
                .collect();
            if other.contains(program) {
                format!("{program} no puede ejecutarse en ningún sandbox de Codex en este equipo")
            } else {
                format!(
                    "{program} solo puede ejecutarse en el sandbox {other_name} de Codex, que aquí no sirve porque en él no funciona {}",
                    only_other.join(", ")
                )
            }
        })
        .collect();
    SandboxPlan {
        unelevated: use_unelevated,
        note: use_unelevated.then(|| {
            format!(
                "sandbox de Codex sin elevar (el configurado no puede ejecutar {})",
                configured.join(", ")
            )
        }),
        obstacles,
    }
}

/// Tries each program Tinto can find on PATH in both of Codex's Windows
/// sandboxes, from the task's worktree, and plans from what fails.
pub fn codex_sandbox_plan(task: &DeliveryTask, programs: &[String]) -> SandboxPlan {
    #[cfg(windows)]
    {
        let Ok(codex) = crate::agent_console::validation::resolve_agent_binary("codex") else {
            return SandboxPlan::default();
        };
        if task.distro.is_some() {
            return SandboxPlan::default();
        }
        let programs: Vec<&String> = programs
            .iter()
            .filter(|program| which::which(program).is_ok())
            .collect();
        let fails = |unelevated: bool| -> Vec<String> {
            programs
                .iter()
                .filter(|program| {
                    let mut command = std::process::Command::new(&codex);
                    command
                        .current_dir(plain_path(&task.worktree))
                        .arg("sandbox");
                    if unelevated {
                        command.args(["-c", UNELEVATED_SANDBOX]);
                    }
                    command.arg("--");
                    // A shell has to start; anything else only has to be found.
                    let name = program.rsplit(['/', '\\']).next().unwrap_or_default();
                    if matches!(name, "bash" | "sh" | "bash.exe" | "sh.exe") {
                        command.args([program.as_str(), "-c", "exit 0"]);
                    } else {
                        command.args(["cmd", "/c", "where", program.as_str()]);
                    }
                    crate::windows_process::output_with_timeout(
                        &mut command,
                        std::time::Duration::from_secs(30),
                    )
                    .is_ok_and(|output| !output.status.success())
                })
                .map(|program| program.to_string())
                .collect()
        };
        let configured = fails(false);
        if configured.is_empty() {
            return SandboxPlan::default();
        }
        sandbox_plan_from(&configured, &fails(true))
    }
    #[cfg(not(windows))]
    {
        let _ = (task, programs);
        SandboxPlan::default()
    }
}

/// How QA records the issue's visual changes for the user: Tinto renders the
/// record to one HTML page with the screenshots embedded when the job ends.
fn visual_record_instructions(key: &str) -> String {
    format!(
        r#"When the issue changes what users see, write its visual changes record in your QA folder as `{key}.md`, with its screenshots beside it, referenced by file name. It is for the user's local review only: use user-facing language, and include no secrets, local absolute paths or personal data, and no credentials on screen.
- Record each visual change at two viewports, desktop 1280×800 and mobile 375×812, in the states that matter: dialogs, menus, warnings, errors, toasts, empty and disabled states.
- Find everything in this one pass: every changed screen at both viewports and, at mobile, every element the change touches (headers, lists, table cells, badges, menus, buttons) checked for overflow, clipping, wrapping and horizontal scroll. List every problem you find, not only the first: each further QA pass sends the work back once more and rebuilds the QA environment.
- Compare every user-facing text with the approved text, character by character, and report any difference as a problem.
- Screenshots at full resolution: at least the viewport width in pixels (ideally twice, with a device scale factor of 2), PNG or JPEG of quality 90 or more; never a scaled-down capture, a zoomed-out page or a thumbnail. Check each image's size and retake any below the viewport width. Name each one `{key}-<n>-<slug>-desktop.png` or `-mobile.png`.
- A toast that disappears before the capture: confirm its text with a DOM observer and say so in the record. Mark what still waits for a fix or a capture as pending.
- Follow this layout (screenshots one per row, never side by side; every change block has the Where/Behaviour table, the screenshots, then Observations, Problems and Suggested solutions, writing "None" when empty; separate pre-existing behaviour from what the change introduces):

# {key} — Visual changes: <short outcome>

| Test environment | |
| --- | --- |
| Date | <YYYY-MM-DD> |
| Environment | <local dev stack / staging; no secrets> |
| Branch | `<branch>` |
| Viewports | Desktop 1280×800, mobile 375×812 |

## 1. <Change name>

| | |
| --- | --- |
| Where | <navigation path to the screen or action> |
| Behaviour | <what the user sees and what happens, approved texts quoted verbatim> |

**Desktop — <state>**

![]({key}-1-<slug>-desktop.png)

**Mobile — <state>**

![]({key}-1-<slug>-mobile.png)

**Observations:** <layout notes; mark pre-existing behaviour as such>

**Problems:** <a list, one problem per item, each starting with its label P1, P2… numbered across the whole record; or "None">

**Suggested solutions:** <a list, each item starting with the label of the problem it fixes ("P1: …"); several for one problem are alternatives, the one you recommend first; or "None">

## Verdicts

| Item | Verdict |
| --- | --- |
| 1. <Change name> | pass / fail / not verifiable: <why> |

The user decides on each problem in Tinto, by its label: write each so it can be fixed on its own."#
    )
}

/// What every QA job's record holds, visual or from the command line: Tinto
/// shows it to the user and warns when its header or verdicts are missing.
fn qa_record_guide(key: &str) -> String {
    format!(
        r#"QA record guide. Every QA writes its record, which the user reviews in Tinto before the commit:
- File: `{key}.md` in your QA folder, exactly that name. Screenshots go beside it as `{key}-<n>-<slug>-desktop.png` or `-mobile.png`, linked by relative file name.
- Header table first: date, branch, environment (the stack and ports you tested on, or "command line, no UI") and, when there is a UI, the viewports.
- Visual QA: one section per change you checked (what, where, expected and seen, its screenshots), then a verdict table (each item → pass / fail / not verifiable, with why), then the problems for the user to decide: `**Problems:**` with a list labelled P1…Pn and `**Suggested solutions:**` with "P1: …" items, or "None".
- Command-line QA (no UI): one section per suite (the command, its pass and fail counts, its log file in your QA folder), then a matrix table (each acceptance criterion or preserved behaviour → the test that proves it → pass / fail / not verified, with the reason), then the problems as above.
- Never secrets: no passwords, tokens or keys in the record, and none in the command output you print or save."#
    )
}

/// What a resumed agent is told: the session already has its instructions.
const RESUME_PROMPT: &str = "El usuario pausó esta etapa y ahora la reanuda. Continúa exactamente donde lo dejaste, con las mismas instrucciones, y termina entregando el resultado con el formato pedido.";

/// Why a resumed agent goes on: the user's pause, or the note of a job that
/// stopped on its own.
fn resume_prompt(job: &DeliveryJob) -> String {
    job.resume_note
        .clone()
        .unwrap_or_else(|| RESUME_PROMPT.to_string())
}

/// The session a job continues, when it resumes a paused one.
fn resumed_session(job: &DeliveryJob) -> Result<Option<&str>, DeliveryError> {
    match (&job.resumes, job.provider_session_id.as_deref()) {
        (None, _) => Ok(None),
        (Some(_), Some(session)) => Ok(Some(session)),
        (Some(_), None) => Err(DeliveryError::new(
            "session_unknown",
            "no se sabe la sesión del agente que hay que reanudar",
        )),
    }
}

/// The `-c` override that selects Codex's unelevated Windows sandbox.
const UNELEVATED_SANDBOX: &str = r#"windows.sandbox="unelevated""#;

/// `native` is the Codex binary for a repo on this machine's filesystem;
/// `plan`, how its Windows sandbox has to run.
fn codex_launch(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
    native: Option<PathBuf>,
    plan: &SandboxPlan,
) -> Result<JobLaunch, DeliveryError> {
    let sandbox = match job.access {
        DeliveryAccess::Workspace => "workspace-write",
        DeliveryAccess::Full => "danger-full-access",
    };
    // `codex exec resume` has no `-C` or `-s`: it runs in the worktree as
    // its working directory and takes the sandbox as an override.
    let resume = resumed_session(job)?;
    let prompt = match resume {
        Some(_) => resume_prompt(job),
        None => job_prompt(job, task, paths, task.distro.is_some(), &plan.obstacles),
    };
    match task.distro.as_deref() {
        None => {
            let program = native.ok_or_else(|| {
                DeliveryError::new("binary_not_found", "no se encontró Codex en este equipo")
            })?;
            let worktree = plain_path(&task.worktree);
            let sandbox_mode = format!("sandbox_mode={}", Value::from(sandbox));
            let start: Vec<&str> = match resume {
                Some(_) => vec!["exec", "resume", "--json", "-c", &sandbox_mode],
                None => vec!["exec", "--json", "-C", &worktree, "-s", sandbox],
            };
            let mut args: Vec<String> = start
                .into_iter()
                .chain([
                    "--output-schema",
                    &plain_path(&paths.schema),
                    "-o",
                    &plain_path(&paths.result),
                ])
                .map(|value| value.to_string())
                .collect();
            if let Some(model) = job.model.as_deref() {
                args.extend(["-m".to_string(), model.to_string()]);
            }
            if let Some(effort) = job.reasoning.as_deref() {
                args.extend(["-c".to_string(), codex_reasoning(effort)]);
            }
            args.extend(codex_qa_overrides(job, false, paths)?);
            if plan.unelevated {
                args.extend(["-c".to_string(), UNELEVATED_SANDBOX.to_string()]);
            }
            args.extend(resume.map(str::to_string));
            args.push("-".to_string());
            Ok(JobLaunch {
                program,
                args,
                cwd: Some(PathBuf::from(worktree)),
                stdin: prompt,
                provider_session_id: resume.map(str::to_string),
                wsl_job: None,
                note: plan.note.clone(),
                files: Vec::new(),
                env: Vec::new(),
            })
        }
        Some(distro) => {
            let model = job
                .model
                .as_deref()
                .map(|model| format!(" -m {}", sh_quote(model)))
                .unwrap_or_default();
            let reasoning = job
                .reasoning
                .as_deref()
                .map(|effort| format!(" -c {}", sh_quote(&codex_reasoning(effort))))
                .unwrap_or_default();
            let qa: String = codex_qa_overrides(job, true, paths)?
                .iter()
                .map(|value| format!(" {}", sh_quote(value)))
                .collect();
            let start = match resume {
                Some(_) => format!(
                    "exec codex exec resume --json -c {}",
                    sh_quote(&format!("sandbox_mode={}", Value::from(sandbox)))
                ),
                None => format!(
                    "exec codex exec --json -C {} -s {sandbox}",
                    sh_quote(&task.worktree.to_string_lossy())
                ),
            };
            let session = resume
                .map(|id| format!(" {}", sh_quote(id)))
                .unwrap_or_default();
            let script = format!(
                "{start} --output-schema {} -o {}{model}{reasoning}{qa}{session} -",
                sh_quote(&wsl_path(&paths.schema)?),
                sh_quote(&wsl_path(&paths.result)?),
            );
            wsl_launch(
                distro,
                &task.worktree.to_string_lossy(),
                script,
                prompt,
                resume.map(str::to_string),
                job,
            )
        }
    }
}

/// `native` is Muse Code on this machine for a repo on its filesystem.
/// The prompt goes in a file; `--session-id` names a new session and, given
/// again, resumes it.
fn muse_launch(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
    native: Option<PathBuf>,
) -> Result<JobLaunch, DeliveryError> {
    let resume = resumed_session(job)?;
    let session_id = resume
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let from_wsl = native.is_none();
    let prompt = match resume {
        Some(_) => resume_prompt(job),
        None => job_prompt(job, task, paths, from_wsl, &[]),
    };
    std::fs::write(&paths.prompt, prompt).map_err(DeliveryError::io)?;
    let path = |path: &Path| -> Result<String, DeliveryError> {
        if from_wsl {
            wsl_path(path)
        } else {
            Ok(plain_path(path))
        }
    };
    let worktree = if from_wsl {
        task.worktree.to_string_lossy().to_string()
    } else {
        plain_path(&task.worktree)
    };
    // No one answers prompts; the workspace is trusted so Muse follows the
    // repo's own rules, as Codex reads AGENTS.md.
    let mut args: Vec<String> = [
        "exec",
        "--json",
        "--workspace",
        &worktree,
        "--output-schema",
        &path(&paths.schema)?,
        "--prompt-file",
        &path(&paths.prompt)?,
        "--approval-mode",
        "never",
        "--trust-workspace",
        "--session-id",
        &session_id,
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    if job.access == DeliveryAccess::Full {
        args.push("--yolo".to_string());
    } else if job.role == QA_ROLE {
        args.extend(["--sandbox-network".to_string(), "enabled".to_string()]);
    }
    if job.role == QA_ROLE && job.qa_browser {
        // The same headless browser Codex and Claude get, for this run only.
        let (command, server_args) = browser_server(from_wsl, paths)?;
        let server = serde_json::json!({ "command": command, "args": server_args });
        args.extend([
            "-c".to_string(),
            format!("mcpServers.{BROWSER_SERVER}={server}"),
        ]);
    }
    if let Some(model) = job.model.as_deref() {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    if let Some(effort) = job.reasoning.as_deref() {
        args.extend(["--reasoning-effort".to_string(), effort.to_string()]);
    }
    match (native, task.distro.as_deref()) {
        (Some(binary), _) => {
            let (program, prefix) = crate::agent_console::msp::muse_program(&binary);
            Ok(JobLaunch {
                program,
                args: prefix
                    .into_iter()
                    .map(|arg| arg.to_string_lossy().to_string())
                    .chain(args)
                    .collect(),
                cwd: Some(PathBuf::from(worktree)),
                stdin: String::new(),
                provider_session_id: Some(session_id),
                wsl_job: None,
                note: None,
                files: Vec::new(),
                env: Vec::new(),
            })
        }
        (None, Some(distro)) => {
            let script = std::iter::once("exec muse".to_string())
                .chain(args.iter().map(|arg| sh_quote(arg)))
                .collect::<Vec<_>>()
                .join(" ");
            wsl_launch(
                distro,
                &worktree,
                script,
                String::new(),
                Some(session_id),
                job,
            )
        }
        (None, None) => Err(DeliveryError::new(
            "binary_not_found",
            "no se encontró Muse Code en este equipo",
        )),
    }
}

/// Where OpenCode reads a job's settings from: its permission rules.
const OPENCODE_CONFIG_ENV: &str = "OPENCODE_CONFIG_CONTENT";

/// OpenCode has no flag for a result schema: its last message must be the
/// result itself, which [`read_outcome`] takes out of it.
fn opencode_result_note() -> String {
    format!(
        "Your final message must be only the structured result: one JSON object, without prose or code fences around it, that matches this JSON schema:\n{RESULT_SCHEMA}"
    )
}

/// `opencode run --format json`, the prompt on stdin. OpenCode names its
/// session, which the first event gives; a job it continues passes it back.
/// Its permission rules go in [`OPENCODE_CONFIG_ENV`] (see
/// [`opencode_permissions`]).
fn opencode_launch(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
    native: Option<PathBuf>,
) -> Result<JobLaunch, DeliveryError> {
    let resume = resumed_session(job)?;
    let from_wsl = native.is_none();
    let prompt = match resume {
        Some(_) => resume_prompt(job),
        None => format!(
            "{}\n\n{}",
            job_prompt(job, task, paths, from_wsl, &[]),
            opencode_result_note()
        ),
    };
    let worktree = if from_wsl {
        task.worktree.to_string_lossy().to_string()
    } else {
        plain_path(&task.worktree)
    };
    // OpenCode takes its project from `PWD`, not from the folder it starts
    // in: Tinto's own would make every job work on the checkout Tinto was
    // started from, with the worktree out of its reach.
    let mut args: Vec<String> = ["run", "--format", "json", "--dir", &worktree]
        .into_iter()
        .map(str::to_string)
        .collect();
    if let Some(session) = resume {
        args.extend(["--session".to_string(), session.to_string()]);
    }
    if let Some(model) = job.model.as_deref() {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    // OpenCode's reasoning effort is the model's variant.
    if let Some(effort) = job.reasoning.as_deref() {
        args.extend(["--variant".to_string(), effort.to_string()]);
    }
    let config =
        serde_json::json!({ "permission": opencode_permissions(job, task, paths, &worktree, from_wsl) })
            .to_string();
    match (native, task.distro.as_deref()) {
        (Some(program), _) => Ok(JobLaunch {
            program,
            args,
            cwd: Some(PathBuf::from(&worktree)),
            stdin: prompt,
            provider_session_id: resume.map(str::to_string),
            wsl_job: None,
            note: None,
            files: Vec::new(),
            env: vec![
                (OPENCODE_CONFIG_ENV.to_string(), config),
                ("PWD".to_string(), worktree),
            ],
        }),
        (None, Some(distro)) => {
            let script = format!(
                "export {OPENCODE_CONFIG_ENV}={}; exec opencode {}",
                sh_quote(&config),
                args.iter()
                    .map(|arg| sh_quote(arg))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            wsl_launch(
                distro,
                &worktree,
                script,
                prompt,
                resume.map(str::to_string),
                job,
            )
        }
        (None, None) => Err(DeliveryError::new(
            "binary_not_found",
            "no se encontró OpenCode en este equipo",
        )),
    }
}

/// The models OpenCode offers on this machine (`provider/model`), as
/// `opencode models` lists them.
pub fn opencode_models() -> Result<Vec<String>, DeliveryError> {
    let binary = crate::agent_console::validation::resolve_agent_binary("opencode")?;
    let output = std::process::Command::new(binary)
        .arg("models")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(DeliveryError::io)?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| line.contains('/') && !line.contains(' '))
        .map(str::to_string)
        .collect())
}

/// Commands a read-only OpenCode job may run besides the repo's checks.
const OPENCODE_READ_COMMANDS: [&str; 13] = [
    "ls",
    "cat",
    "head",
    "tail",
    "wc",
    "grep",
    "rg",
    "find",
    "pwd",
    "git status",
    "git diff",
    "git log",
    "git show",
];

/// What an OpenCode job may do, as OpenCode's permission rules: OpenCode
/// has no sandbox of its own, so these are its only fence.
///
/// - Full access: everything.
/// - A job that writes: any edit in the worktree and any command but
///   Tinto's own steps (the deny list Claude jobs get).
/// - A read-only job (review, QA, explore): no edits and only the repo's
///   checks and a few commands that read; QA may write in its QA folder.
/// - Outside the worktree only the folders it may read or, for QA, write.
///
/// Edit rules name a path from the worktree; folders outside it are also
/// named whole for `external_directory`. A rule for one path or command
/// comes after the `*` it narrows.
fn opencode_permissions(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
    worktree: &str,
    from_wsl: bool,
) -> Value {
    if job.access == DeliveryAccess::Full {
        return serde_json::json!({
            "edit": "allow", "bash": "allow", "webfetch": "allow", "external_directory": "allow",
        });
    }
    let worktree = worktree.replace('\\', "/");
    let mut edit = serde_json::Map::new();
    let mut bash = serde_json::Map::new();
    let mut outside = serde_json::Map::new();
    let rule = |map: &mut serde_json::Map<String, Value>, key: String, action: &str| {
        map.insert(key, Value::from(action));
    };
    let read_only = !job.writes;
    rule(
        &mut edit,
        "*".into(),
        if read_only { "deny" } else { "allow" },
    );
    rule(
        &mut bash,
        "*".into(),
        if read_only { "deny" } else { "allow" },
    );
    rule(&mut outside, "*".into(), "deny");
    let full = full_checks(job, task);
    if read_only {
        for command in OPENCODE_READ_COMMANDS
            .iter()
            .map(|command| command.to_string())
            .chain(job.allowed_commands.iter().cloned())
            .filter(|command| !full.contains(&command.as_str()))
        {
            // Also as `git -C <worktree> …`, as agents often write it.
            if let Some(rest) = command.strip_prefix("git ") {
                rule(&mut bash, format!("git -C {worktree} {rest} *"), "allow");
                rule(&mut bash, format!("git -C {worktree} {rest}"), "allow");
            }
            rule(&mut bash, format!("{command} *"), "allow");
            rule(&mut bash, command, "allow");
        }
    }
    for dir in job.read_dirs.iter().map(|dir| agent_path(dir, from_wsl)) {
        rule(
            &mut edit,
            format!("{}/**", relative_path(&worktree, &dir)),
            "deny",
        );
        rule(&mut outside, format!("{dir}/**"), "allow");
        rule(&mut outside, dir, "allow");
    }
    if job.role == QA_ROLE {
        let writable = [qa_folder(paths, from_wsl)]
            .into_iter()
            .chain(job.write_dirs.iter().map(|dir| agent_path(dir, from_wsl)));
        for dir in writable {
            rule(
                &mut edit,
                format!("{}/**", relative_path(&worktree, &dir)),
                "allow",
            );
            rule(&mut outside, format!("{dir}/**"), "allow");
            rule(&mut outside, dir, "allow");
        }
    }
    for command in CLAUDE_DENIED {
        rule(&mut bash, format!("{command} *"), "deny");
        rule(&mut bash, command.to_string(), "deny");
        if let Some(rest) = command.strip_prefix("git ") {
            rule(&mut bash, format!("git -C {worktree} {rest} *"), "deny");
        }
    }
    for check in full {
        rule(&mut bash, check.to_string(), "deny");
    }
    serde_json::json!({
        "edit": edit,
        "bash": bash,
        "external_directory": outside,
        "webfetch": if job.role == QA_ROLE { "allow" } else { "deny" },
    })
}

/// `path` as seen from `base`, both absolute with `/`: `../qa/K-1` for a
/// folder beside the worktree.
fn relative_path(base: &str, path: &str) -> String {
    let parts = |path: &str| -> Vec<String> {
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    };
    let (base, path) = (parts(base), parts(path));
    let common = base
        .iter()
        .zip(&path)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = vec!["..".to_string(); base.len() - common];
    relative.extend(path[common..].iter().cloned());
    relative.join("/")
}

fn claude_launch(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
    native: Option<PathBuf>,
    claude_distro: Option<&str>,
) -> Result<JobLaunch, DeliveryError> {
    // A Windows repo's job runs with the Claude Code installed on Windows;
    // WSL only when asked for (dispatch gives the job to Codex instead).
    if native.is_none() && task.distro.is_none() && job.platform != Some(DeliveryPlatform::Wsl) {
        return Err(DeliveryError::new(
            "binary_not_found",
            "Claude Code no está instalado en este equipo",
        ));
    }
    let mode = match (job.access, job.writes) {
        (DeliveryAccess::Full, _) => "bypassPermissions",
        (DeliveryAccess::Workspace, true) => "acceptEdits",
        // Without anyone to approve prompts, `default` denies edits.
        (DeliveryAccess::Workspace, false) => "default",
    };
    let from_wsl = native.is_none();
    let allowed = claude_allowed_tools(job, task, paths, from_wsl);
    let denied = claude_denied_tools(job, task, from_wsl);
    // A resumed job goes on in the paused session; a new one names its own.
    let resume = resumed_session(job)?;
    let session_id = resume
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let session_flag = if resume.is_some() {
        "--resume"
    } else {
        "--session-id"
    };
    let prompt = match resume {
        Some(_) => resume_prompt(job),
        None => job_prompt(job, task, paths, from_wsl, &[]),
    };
    if let Some(program) = native {
        let mut args: Vec<String> = [
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-mode",
            mode,
            session_flag,
            &session_id,
            "--json-schema",
            RESULT_SCHEMA,
            // None of the user's MCP servers: only those the job is given
            // (QA's browser), never Drive, Docs or Tinto's own.
            "--strict-mcp-config",
        ]
        .iter()
        .map(|value| value.to_string())
        .collect();
        if let Some(model) = job.model.as_deref() {
            args.extend(["--model".to_string(), model.to_string()]);
        }
        if let Some(effort) = job.reasoning.as_deref() {
            args.extend(["--effort".to_string(), effort.to_string()]);
        }
        let (qa, files) = claude_qa_args(
            job,
            &plain_path(&task.repo),
            &plain_path(&paths.qa_output),
            false,
            paths,
        )?;
        args.extend(qa);
        if !allowed.is_empty() {
            args.push("--allowedTools".to_string());
            args.extend(allowed);
        }
        if !denied.is_empty() {
            args.push("--disallowedTools".to_string());
            args.extend(denied);
        }
        return Ok(JobLaunch {
            program,
            args,
            cwd: Some(PathBuf::from(plain_path(&task.worktree))),
            stdin: prompt,
            provider_session_id: Some(session_id),
            wsl_job: None,
            note: None,
            files,
            env: Vec::new(),
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
    let effort = job
        .reasoning
        .as_deref()
        .map(|effort| format!(" --effort {}", sh_quote(effort)))
        .unwrap_or_default();
    let model = job
        .model
        .as_deref()
        .map(|model| format!(" --model {}", sh_quote(model)))
        .unwrap_or_default();
    let allowed = if allowed.is_empty() {
        String::new()
    } else {
        let tools: Vec<String> = allowed.iter().map(|tool| sh_quote(tool)).collect();
        format!(" --allowedTools {}", tools.join(" "))
    };
    let denied = if denied.is_empty() {
        String::new()
    } else {
        let tools: Vec<String> = denied.iter().map(|tool| sh_quote(tool)).collect();
        format!(" --disallowedTools {}", tools.join(" "))
    };
    // A Windows checkout read by git inside WSL: without Windows' line-ending
    // setting, every CRLF file would look modified.
    let line_endings = match task.distro {
        Some(_) => None,
        None => git(
            Place::Native,
            &task.worktree,
            &["config", "--get", "core.autocrlf"],
        )
        .ok()
        .filter(|value| !value.is_empty()),
    }
    .map(|value| {
        format!(
            "export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.autocrlf GIT_CONFIG_VALUE_0={}; ",
            sh_quote(&value)
        )
    })
    .unwrap_or_default();
    let main_checkout = match task.distro {
        Some(_) => task.repo.to_string_lossy().into_owned(),
        None => wsl_path(&task.repo)?,
    };
    let (qa, files) = claude_qa_args(
        job,
        &main_checkout,
        &wsl_path(&paths.qa_output)?,
        true,
        paths,
    )?;
    let qa: String = qa
        .iter()
        .map(|value| format!(" {}", sh_quote(value)))
        .collect();
    let script = format!(
        "{line_endings}exec claude -p --output-format stream-json --verbose --permission-mode {mode} {session_flag} {} --json-schema \"$(cat {})\" --strict-mcp-config{model}{effort}{qa}{allowed}{denied}",
        sh_quote(&session_id),
        sh_quote(&wsl_path(&paths.schema)?)
    );
    let mut launch = wsl_launch(&distro, &cwd, script, prompt, Some(session_id), job)?;
    launch.files = files;
    Ok(launch)
}

/// Commands a Claude job without full access may not run, whatever it
/// chains them with: Tinto's own steps (commit, push, merge) and what reaches
/// beyond the task's worktree and branch. Claude Code checks each part of a
/// compound command against them.
const CLAUDE_DENIED: [&str; 11] = [
    "git commit",
    "git push",
    "git merge",
    "git rebase",
    "git reset --hard",
    "git clean",
    "git checkout",
    "git switch",
    "npm install -g",
    "npm i -g",
    "cargo install",
];

/// What a Claude job without full access may use: any shell command (the
/// deny list in [`claude_denied_tools`] still applies). It may read the
/// folders its coordinator named; a QA job may also write in its own QA
/// folder, where its evidence goes.
fn claude_allowed_tools(
    job: &DeliveryJob,
    _task: &DeliveryTask,
    paths: &JobPaths,
    from_wsl: bool,
) -> Vec<String> {
    if job.access == DeliveryAccess::Full {
        return Vec::new();
    }
    let mut tools = vec!["Bash".to_string()];
    // Folders it may read only: a rule, not `--add-dir`, which would let a
    // job that accepts edits change them too.
    tools.extend(
        job.read_dirs
            .iter()
            .map(|dir| format!("Read({}/**)", claude_rule_path(&agent_path(dir, from_wsl)))),
    );
    if job.role == QA_ROLE {
        let writable = [qa_folder(paths, from_wsl)]
            .into_iter()
            .chain(job.write_dirs.iter().map(|dir| agent_path(dir, from_wsl)));
        tools.extend(writable.map(|dir| format!("Edit({}/**)", claude_rule_path(&dir))));
    }
    if job.qa_browser {
        tools.push(format!("mcp__{BROWSER_SERVER}"));
    }
    tools
}

/// Claude Code tools no job needs, whatever its access: they reach beyond
/// its task (schedules, notifications, remote sessions, workflows, other
/// agents' conversations, publishing, other worktrees).
const CLAUDE_UNNEEDED_TOOLS: [&str; 12] = [
    "CronCreate",
    "CronDelete",
    "CronList",
    "ScheduleWakeup",
    "PushNotification",
    "RemoteTrigger",
    "Workflow",
    "SendMessage",
    "PublishPlugin",
    "ShareOnboardingGuide",
    "EnterWorktree",
    "ExitWorktree",
];

/// The web tools, which only QA (it has network) and full access keep:
/// other stages that tried them were refused, so they do not get them.
const CLAUDE_WEB_TOOLS: [&str; 2] = ["WebFetch", "WebSearch"];

/// The task's checks a job without full access may not run as they are:
/// Tinto runs each one on the job's result, and agents that ran whole
/// suites themselves spent 10 to 14 minutes on them, past Claude Code's
/// Bash ceiling. The format and lint checks stay, since writing stages run
/// them before they end; so does any narrower run (a test file, a filter).
fn full_checks<'a>(job: &DeliveryJob, task: &'a DeliveryTask) -> Vec<&'a str> {
    if job.access == DeliveryAccess::Full {
        return Vec::new();
    }
    task.checks
        .iter()
        .map(String::as_str)
        .filter(|check| !style_check(check))
        .collect()
}

/// What a Claude job may not use: the tools no job needs and, without full
/// access, the web outside QA, Tinto's own steps (also as `git -C
/// <worktree> …`, which would otherwise slip past them) and the task's
/// checks as they are written.
fn claude_denied_tools(job: &DeliveryJob, task: &DeliveryTask, from_wsl: bool) -> Vec<String> {
    let mut tools: Vec<String> = CLAUDE_UNNEEDED_TOOLS.map(str::to_string).to_vec();
    if job.access == DeliveryAccess::Full {
        return tools;
    }
    if job.role != QA_ROLE {
        tools.extend(CLAUDE_WEB_TOOLS.map(str::to_string));
    }
    let worktree = agent_path(&plain_path(&task.worktree), from_wsl);
    tools.extend(
        CLAUDE_DENIED
            .iter()
            .flat_map(|command| {
                let mut forms = vec![command.to_string()];
                if let Some(rest) = command.strip_prefix("git ") {
                    forms.push(format!("git -C {worktree} {rest}"));
                }
                forms
            })
            .map(|command| format!("Bash({command}:*)")),
    );
    // Exactly as written, so a narrower run of the same tool stays allowed.
    tools.extend(
        full_checks(job, task)
            .into_iter()
            .map(|check| format!("Bash({check})")),
    );
    tools
}

/// Wraps Playwright MCP for QA jobs (see the file's header).
const QA_BROWSER_SCRIPT: &str = include_str!("qa_browser.mjs");

/// How the browser MCP server starts. On Windows it always runs on the
/// Windows side, also for agents in WSL: that is where Chrome is and where
/// Docker Desktop publishes the app under test, and WSL may have no Node.
fn browser_server(
    from_wsl: bool,
    paths: &JobPaths,
) -> Result<(String, Vec<String>), DeliveryError> {
    std::fs::create_dir_all(&paths.qa_output).map_err(DeliveryError::io)?;
    std::fs::write(&paths.qa_browser_script, QA_BROWSER_SCRIPT).map_err(DeliveryError::io)?;
    let args = vec![
        plain_path(&paths.qa_browser_script),
        plain_path(&paths.qa_output),
        PLAYWRIGHT_MCP.to_string(),
    ];
    if !cfg!(windows) {
        return Ok(("node".to_string(), args));
    }
    let cmd = if from_wsl {
        "/mnt/c/Windows/System32/cmd.exe"
    } else {
        "cmd"
    };
    let mut wrapped = vec!["/c".to_string(), "node".to_string()];
    wrapped.extend(args);
    Ok((cmd.to_string(), wrapped))
}

/// The `-c` override that sets Codex's reasoning effort.
fn codex_reasoning(effort: &str) -> String {
    format!("model_reasoning_effort={}", Value::from(effort))
}

/// A Claude QA job reads the main checkout, its own QA folder (to look at
/// its screenshots) and the folders the user's permissions name and, when
/// enabled, gets the browser. Paths are as the agent sees them; `from_wsl`
/// says whether it runs in WSL.
fn claude_qa_args(
    job: &DeliveryJob,
    main_checkout: &str,
    qa_output: &str,
    from_wsl: bool,
    paths: &JobPaths,
) -> Result<(Vec<String>, Vec<JobFile>), DeliveryError> {
    if job.role != QA_ROLE {
        // A read-only stage (review) reads the main checkout too: that is
        // where the contract it reviews against lives. Its permission mode
        // lets it edit nothing there.
        let args = match job.writes {
            true => Vec::new(),
            false => vec!["--add-dir".to_string(), main_checkout.to_string()],
        };
        return Ok((args, Vec::new()));
    }
    let mut args = vec![
        "--add-dir".to_string(),
        main_checkout.to_string(),
        qa_output.to_string(),
    ];
    // Folders the user's permissions name; a QA job can edit only where
    // its allowed tools say so.
    let named = job.read_dirs.iter().chain(&job.write_dirs);
    args.extend(named.map(|dir| agent_path(dir, from_wsl)));
    if job.qa_browser {
        let (command, server_args) = browser_server(from_wsl, paths)?;
        args.push("--mcp-config".to_string());
        args.push(
            serde_json::json!({
                "mcpServers": {
                    BROWSER_SERVER: {
                        "type": "stdio",
                        "command": command,
                        "args": server_args,
                        "timeout": BROWSER_TOOL_SILENCE_SECS * 1000,
                    }
                }
            })
            .to_string(),
        );
    }
    // Past its budget, every tool call tells the agent to write its record.
    let budget_ms = u64::from(job.timeout_minutes * QA_BUDGET_PERCENT / 100) * 60_000;
    let script = paths.dir.join(QA_BUDGET_SCRIPT);
    let budget = qa_budget_script(super::now_ms() + budget_ms, job.timeout_minutes, &job.id);
    let hook = format!("node \"{}\"", agent_path(&plain_path(&script), from_wsl));
    args.push("--settings".to_string());
    args.push(
        serde_json::json!({
            "hooks": { "PostToolUse": [{ "hooks": [{ "type": "command", "command": hook }] }] }
        })
        .to_string(),
    );
    Ok((args, vec![(script, budget)]))
}

/// The hook that tells a QA agent its budget is spent (see
/// [`qa_budget_script`]).
const QA_BUDGET_SCRIPT: &str = "qa-budget.mjs";

/// A Claude Code hook run after each tool call: past `deadline_ms` it adds
/// to the agent's context that it must stop testing and write its record.
fn qa_budget_script(deadline_ms: u64, timeout_minutes: u32, job_id: &str) -> String {
    let message = format!(
        "Tinto: {QA_BUDGET_PERCENT}% of this QA job's {timeout_minutes} minutes are spent. Stop testing now: write the QA record with what you have, marking each item you did not check «not verifiable: out of time», then finish with the structured result."
    );
    let output = serde_json::json!({
        "hookSpecificOutput": { "hookEventName": "PostToolUse", "additionalContext": message }
    });
    format!(
        "// Tinto QA budget for job {job_id}.\nif (Date.now() >= {deadline_ms}) console.log({});\n",
        Value::from(output.to_string())
    )
}

/// Arguments for a Codex QA job: its QA folder writable, network in the
/// sandbox and, when enabled, the browser, pre-approved because `exec` has
/// no one to ask.
fn codex_qa_overrides(
    job: &DeliveryJob,
    from_wsl: bool,
    paths: &JobPaths,
) -> Result<Vec<String>, DeliveryError> {
    if job.role != QA_ROLE {
        return Ok(Vec::new());
    }
    let mut values = vec!["sandbox_workspace_write.network_access=true".to_string()];
    if job.qa_browser {
        // `-c` values are TOML; JSON strings are valid TOML basic strings.
        let text = |value: &str| Value::from(value).to_string();
        let key = |name: &str| format!("mcp_servers.{BROWSER_SERVER}.{name}");
        let (command, args) = browser_server(from_wsl, paths)?;
        values.push(format!("{}={}", key("command"), text(&command)));
        values.push(format!(
            "{}=[{}]",
            key("args"),
            args.iter()
                .map(|arg| text(arg))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        values.push(format!(
            "{}=\"approve\"",
            key("default_tools_approval_mode")
        ));
        values.push(format!(
            "{}={BROWSER_TOOL_SILENCE_SECS}",
            key("tool_timeout_sec")
        ));
    }
    std::fs::create_dir_all(&paths.qa_output).map_err(DeliveryError::io)?;
    // Besides its QA folder, only the folders the user's permissions name
    // become writable roots of the sandbox.
    let writable = [qa_folder(paths, from_wsl)]
        .into_iter()
        .chain(job.write_dirs.iter().map(|dir| agent_path(dir, from_wsl)));
    Ok(writable
        .flat_map(|dir| ["--add-dir".to_string(), dir])
        .chain(
            values
                .into_iter()
                .flat_map(|value| ["-c".to_string(), value]),
        )
        .collect())
}

fn shell_launch(
    job: &DeliveryJob,
    task: &DeliveryTask,
    paths: &JobPaths,
) -> Result<JobLaunch, DeliveryError> {
    if let Some(distro) = task.distro.as_deref() {
        return wsl_launch(
            distro,
            &task.worktree.to_string_lossy(),
            job.prompt.clone(),
            String::new(),
            None,
            job,
        );
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
        wsl_job: None,
        note: None,
        files: Vec::new(),
        env: Vec::new(),
    })
}

/// Runs `script` in WSL with the job's marker in its environment (see
/// [`stop_wsl_job`]). The script is the last argument, passed untouched.
fn wsl_launch(
    distro: &str,
    cwd: &str,
    script: String,
    stdin: String,
    provider_session_id: Option<String>,
    job: &DeliveryJob,
) -> Result<JobLaunch, DeliveryError> {
    let wsl_job = WslJob {
        distro: distro.to_string(),
        job_id: job.id.clone(),
    };
    // Windows' environment does not cross into WSL: the repo's variables
    // for checks are exported here.
    let check_env: String = job
        .check_env
        .iter()
        .map(|(name, value)| format!(" {name}={}", sh_quote(value)))
        .collect();
    let leader = format!(
        r#"export TINTO_DELIVERY_JOB={}{check_env}; eval "$1""#,
        sh_quote(&wsl_job.job_id)
    );
    Ok(JobLaunch {
        program: PathBuf::from("wsl.exe"),
        args: vec![
            "-d".to_string(),
            distro.to_string(),
            "--cd".to_string(),
            cwd.to_string(),
            "--exec".to_string(),
            "bash".to_string(),
            "-lc".to_string(),
            leader,
            "tinto-job".to_string(),
            script,
        ],
        cwd: None,
        stdin,
        provider_session_id,
        wsl_job: Some(wsl_job),
        note: None,
        files: Vec::new(),
        env: Vec::new(),
    })
}

/// Sends `signal` (`TERM` or `KILL`) to every Linux process of a WSL job.
pub fn stop_wsl_job(job: &WslJob, signal: &str) {
    let script = format!(
        r#"for env in /proc/[0-9]*/environ; do grep -qzx {marker} "$env" 2>/dev/null || continue; pid=${{env#/proc/}}; kill -{signal} "${{pid%/environ}}" 2>/dev/null; done; true"#,
        marker = sh_quote(&format!("TINTO_DELIVERY_JOB={}", job.job_id)),
    );
    let mut command = std::process::Command::new("wsl.exe");
    command.args(["-d", &job.distro, "--exec", "bash", "-c", &script]);
    #[cfg(target_os = "windows")]
    {
        crate::windows_process::hide_console(&mut command);
        let _ = crate::windows_process::output_with_timeout(
            &mut command,
            std::time::Duration::from_secs(15),
        );
    }
    #[cfg(not(target_os = "windows"))]
    let _ = command.output();
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
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
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
    /// The whole job's use, for an agent that reports it step by step: the
    /// sum the view kept as it ran may have read lines twice after a restart.
    pub usage: Option<DeliveryUsage>,
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
        DeliveryAgent::Muse => (value.pointer("/stream/kind") == Some(&Value::from("session")))
            .then(|| value.pointer("/stream/id")?.as_str().map(str::to_string))
            .flatten(),
        // Every OpenCode event names its session.
        DeliveryAgent::Opencode => value["sessionID"].as_str().map(str::to_string),
        DeliveryAgent::Shell => None,
    }
}

/// A Muse `exec --json` record: its payload type and the payload. The
/// event itself sits in `record` or `event` for some kinds.
fn muse_record(value: &Value) -> (&str, &Value) {
    let payload = &value["payload"];
    let body = payload
        .get("record")
        .or_else(|| payload.get("event"))
        .filter(|body| body.is_object())
        .unwrap_or(payload);
    (value["payload_type"].as_str().unwrap_or_default(), body)
}

/// Muse tools that only read; any other tool may have changed something.
const MUSE_READ_TOOLS: [&str; 8] = [
    "read_file",
    "search",
    "glob",
    "list_dir",
    "read_skill",
    "web_search",
    "web_fetch",
    "write_todos",
];

/// What one output line tells about the job as it runs.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Observation {
    /// The latest thing the agent did or said, in one line.
    pub activity: Option<String>,
    /// The agent's task list, when the line rewrites it.
    pub plan: Option<Vec<DeliveryPlanItem>>,
    pub usage: Option<DeliveryUsage>,
    /// The command running now and since when, when that changed: `Some(None)`
    /// once none runs. Set from a [`JobClock`].
    pub command: Option<Option<(String, u64)>>,
}

pub fn observe(agent: DeliveryAgent, line: &str) -> Observation {
    let activity = log_entries(agent, line)
        .into_iter()
        .rev()
        .find_map(|entry| {
            let text = entry
                .text
                .lines()
                .find(|text| !text.trim().is_empty())?
                .trim();
            Some(text.chars().take(160).collect::<String>())
        });
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Observation {
            activity,
            ..Observation::default()
        };
    };
    let count = |pointer: &str| value.pointer(pointer).and_then(Value::as_u64).unwrap_or(0);
    let (plan, usage) = match agent {
        DeliveryAgent::Claude => {
            let plan = value
                .pointer("/message/content")
                .and_then(Value::as_array)
                .and_then(|content| {
                    content
                        .iter()
                        .rev()
                        .find(|block| block["type"] == "tool_use" && block["name"] == "TodoWrite")
                })
                .and_then(|block| block.pointer("/input/todos")?.as_array())
                .map(|todos| {
                    todos
                        .iter()
                        .map(|todo| DeliveryPlanItem {
                            text: todo["content"].as_str().unwrap_or_default().to_string(),
                            status: todo["status"].as_str().unwrap_or("pending").to_string(),
                        })
                        .collect()
                });
            let usage = (value["type"] == "result").then(|| DeliveryUsage {
                input_tokens: count("/usage/input_tokens")
                    + count("/usage/cache_creation_input_tokens"),
                cached_tokens: count("/usage/cache_read_input_tokens"),
                output_tokens: count("/usage/output_tokens"),
                cost_micro_usd: value["total_cost_usd"]
                    .as_f64()
                    .map(|usd| (usd * 1_000_000.0).round() as u64),
            });
            (plan, usage)
        }
        DeliveryAgent::Codex => {
            let plan = (value.pointer("/item/type") == Some(&Value::from("todo_list")))
                .then(|| value.pointer("/item/items")?.as_array())
                .flatten()
                .map(|items| {
                    items
                        .iter()
                        .map(|item| DeliveryPlanItem {
                            text: item["text"].as_str().unwrap_or_default().to_string(),
                            status: if item["completed"] == true {
                                "completed"
                            } else {
                                "pending"
                            }
                            .to_string(),
                        })
                        .collect()
                });
            let usage = (value["type"] == "turn.completed").then(|| DeliveryUsage {
                input_tokens: count("/usage/input_tokens")
                    .saturating_sub(count("/usage/cached_input_tokens")),
                cached_tokens: count("/usage/cached_input_tokens"),
                output_tokens: count("/usage/output_tokens"),
                cost_micro_usd: None,
            });
            (plan, usage)
        }
        DeliveryAgent::Muse => {
            let (kind, body) = muse_record(&value);
            let plan = (kind == "todo.snapshot.updated")
                .then(|| body["items"].as_array())
                .flatten()
                .map(|items| {
                    items
                        .iter()
                        .map(|item| DeliveryPlanItem {
                            text: item["text"].as_str().unwrap_or_default().to_string(),
                            status: item["status"].as_str().unwrap_or("pending").to_string(),
                        })
                        .collect()
                });
            (plan, None)
        }
        DeliveryAgent::Opencode => {
            let part = &value["part"];
            let plan = (value["type"] == "tool_use" && part["tool"] == "todowrite")
                .then(|| part.pointer("/state/input/todos")?.as_array())
                .flatten()
                .map(|todos| {
                    todos
                        .iter()
                        .map(|todo| DeliveryPlanItem {
                            text: todo["content"].as_str().unwrap_or_default().to_string(),
                            status: todo["status"].as_str().unwrap_or("pending").to_string(),
                        })
                        .collect()
                });
            // One step's tokens: the service adds the steps up.
            let usage = (value["type"] == "step_finish").then(|| DeliveryUsage {
                input_tokens: count("/part/tokens/input") + count("/part/tokens/cache/write"),
                cached_tokens: count("/part/tokens/cache/read"),
                output_tokens: count("/part/tokens/output") + count("/part/tokens/reasoning"),
                cost_micro_usd: part["cost"]
                    .as_f64()
                    .map(|usd| (usd * 1_000_000.0).round() as u64),
            });
            (plan, usage)
        }
        DeliveryAgent::Shell => (None, None),
    };
    Observation {
        activity,
        plan,
        usage,
        command: None,
    }
}

/// The use of its account's limit a Claude Code `rate_limit_event` reports:
/// the window nearest its limit (five hours, seven days…), with when it
/// renews. `None` for any other line.
pub fn rate_limit(agent: DeliveryAgent, line: &str) -> Option<DeliveryQuota> {
    if agent != DeliveryAgent::Claude || !line.contains("\"rate_limit_event\"") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    let info = &value["rate_limit_info"];
    let windows: Vec<(String, &Value)> = match info["unifiedWindows"].as_object() {
        Some(windows) => windows
            .iter()
            .map(|(name, window)| (name.clone(), window))
            .collect(),
        None => vec![(info["rateLimitType"].as_str()?.to_string(), info)],
    };
    windows
        .into_iter()
        .filter_map(|(window, values)| {
            Some(DeliveryQuota {
                agent,
                window,
                utilization: (values["utilization"].as_f64()? * 100.0).round() as u32,
                resets_at_ms: values["resetsAt"].as_u64()? * 1000,
                seen_at_ms: 0,
            })
        })
        .max_by_key(|quota| quota.utilization)
}

/// Claude Code's Bash stops waiting for a command after 10 minutes.
const CLAUDE_BASH_CEILING_MS: u64 = 600_000;

/// Pairs each top-level tool call's start with its end as the output
/// arrives: the view shows the one running and, once the job ends, where its
/// time went. Times are when Tinto reads each line: Codex lines carry none,
/// and a job in WSL may keep a clock apart from the view's.
#[derive(Debug, Default)]
pub struct JobClock {
    /// The calls running now, oldest first: id, command, start.
    open: Vec<(String, String, u64)>,
    /// When `open` last stopped being empty.
    busy_since: u64,
    commands_ms: u64,
    commands: HashMap<String, DeliverySlowCommand>,
    model_ms: Option<u64>,
    turns: Option<u32>,
}

impl JobClock {
    /// Reads one line, received at `at_ms`. Returns the command to show as
    /// running when it changed.
    pub fn line(
        &mut self,
        agent: DeliveryAgent,
        line: &str,
        at_ms: u64,
    ) -> Option<Option<(String, u64)>> {
        let value = serde_json::from_str::<Value>(line).ok()?;
        let before = self.running();
        match agent {
            DeliveryAgent::Claude if value["parent_tool_use_id"].is_null() => {
                let blocks = value
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten();
                match value["type"].as_str() {
                    Some("assistant") => {
                        for block in blocks.filter(|block| {
                            block["type"] == "tool_use" && block["name"] != "StructuredOutput"
                        }) {
                            let command = match block["input"]["command"].as_str() {
                                Some(command) => command.to_string(),
                                None => claude_tool_summary(block),
                            };
                            self.start(block["id"].as_str().unwrap_or_default(), &command, at_ms);
                        }
                    }
                    Some("user") => {
                        let at_ceiling = value
                            .pointer("/tool_use_result/timedOutAfterMs")
                            .and_then(Value::as_u64)
                            .is_some_and(|ms| ms >= CLAUDE_BASH_CEILING_MS);
                        for block in blocks.filter(|block| block["type"] == "tool_result") {
                            let id = block["tool_use_id"].as_str().unwrap_or_default();
                            self.end(id, at_ms, at_ceiling);
                        }
                    }
                    Some("result") => {
                        self.model_ms = value["duration_api_ms"].as_u64();
                        self.turns = value["num_turns"]
                            .as_u64()
                            .and_then(|turns| u32::try_from(turns).ok());
                    }
                    _ => {}
                }
            }
            DeliveryAgent::Codex => {
                let item = &value["item"];
                let id = item["id"].as_str().unwrap_or_default();
                match value["type"].as_str() {
                    Some("item.started") => {
                        let command = match item["type"].as_str() {
                            Some("command_execution") => {
                                without_shell(item["command"].as_str().unwrap_or_default())
                            }
                            Some("mcp_tool_call") => item["tool"].as_str().unwrap_or("mcp"),
                            other => other.unwrap_or("herramienta"),
                        };
                        self.start(id, command, at_ms);
                    }
                    Some("item.completed") => self.end(id, at_ms, false),
                    _ => {}
                }
            }
            DeliveryAgent::Muse => {
                let (kind, body) = muse_record(&value);
                let id = body["effect_id"].as_str().unwrap_or_default();
                match kind {
                    "tool_batch.effect.started" => {
                        let tool = body["tool_name"].as_str().unwrap_or("herramienta");
                        self.start(id, tool, at_ms);
                    }
                    "tool_batch.effect.terminal" => self.end(id, at_ms, false),
                    _ => {}
                }
            }
            // OpenCode tells of a tool call once it ended, with its times.
            DeliveryAgent::Opencode if value["type"] == "tool_use" => {
                let part = &value["part"];
                let id = part["callID"].as_str().unwrap_or_default();
                let time = |key: &str| part["state"]["time"][key].as_u64().unwrap_or(at_ms);
                self.start(id, &opencode_tool_summary(part), time("start"));
                self.end(id, time("end"), false);
            }
            _ => {}
        }
        let after = self.running();
        (after != before).then_some(after)
    }

    fn running(&self) -> Option<(String, u64)> {
        self.open
            .first()
            .map(|(_, command, at)| (command.clone(), *at))
    }

    fn start(&mut self, id: &str, command: &str, at_ms: u64) {
        if self.open.is_empty() {
            self.busy_since = at_ms;
        }
        // One line, so it reads in the view and repeats group together; a
        // leading `cd <worktree> &&` only hides the command.
        let command = command.split_whitespace().collect::<Vec<_>>().join(" ");
        let command = command
            .strip_prefix("cd ")
            .and_then(|rest| rest.split_once(" && "))
            .map_or(command.as_str(), |(_, rest)| rest);
        let command = command.chars().take(160).collect();
        self.open.push((id.to_string(), command, at_ms));
    }

    fn end(&mut self, id: &str, at_ms: u64, at_ceiling: bool) {
        let Some(index) = self.open.iter().position(|(open, ..)| open == id) else {
            return;
        };
        let (_, command, started) = self.open.remove(index);
        let slow = self
            .commands
            .entry(command.clone())
            .or_insert_with(|| DeliverySlowCommand {
                command,
                ..DeliverySlowCommand::default()
            });
        slow.total_ms += at_ms.saturating_sub(started);
        slow.runs += 1;
        slow.at_ceiling += u32::from(at_ceiling);
        if self.open.is_empty() {
            self.commands_ms += at_ms.saturating_sub(self.busy_since);
        }
    }

    /// Where the job's time went; the calls still running end at `at_ms`.
    pub fn timing(mut self, at_ms: u64) -> DeliveryTiming {
        let open: Vec<String> = self.open.iter().map(|(id, ..)| id.clone()).collect();
        for id in open {
            self.end(&id, at_ms, false);
        }
        let mut slowest: Vec<DeliverySlowCommand> = self.commands.into_values().collect();
        slowest.sort_by(|a, b| {
            b.total_ms
                .cmp(&a.total_ms)
                .then_with(|| a.command.cmp(&b.command))
        });
        slowest.truncate(3);
        DeliveryTiming {
            model_ms: self.model_ms,
            commands_ms: self.commands_ms,
            turns: self.turns,
            slowest,
            prepare_ms: None,
        }
    }
}

/// A command as Codex ran it, without the shell it wraps each one in
/// (`powershell.exe -Command '…'`, `bash -lc '…'`).
fn without_shell(command: &str) -> &str {
    for flag in [" -Command ", " -lc ", " -c "] {
        let Some((program, rest)) = command.split_once(flag) else {
            continue;
        };
        let name = program
            .trim_matches('"')
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(
            name.trim_end_matches(".exe"),
            "powershell" | "pwsh" | "bash" | "sh" | "zsh"
        ) {
            let rest = rest.trim();
            return ['\'', '"']
                .into_iter()
                .find_map(|quote| rest.strip_prefix(quote)?.strip_suffix(quote))
                .unwrap_or(rest);
        }
    }
    command
}

/// The model a Claude Code session says it runs, in its `init` event.
pub fn claude_init_model(line: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    (value["type"] == "system" && value["subtype"] == "init")
        .then(|| value["model"].as_str().map(str::to_string))
        .flatten()
        .filter(|model| !model.is_empty())
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
        DeliveryAgent::Muse => {
            let (kind, body) = muse_record(&value);
            match kind {
                "run.terminal.completed" => body["text"]
                    .as_str()
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| vec![entry("message", text.to_string())])
                    .unwrap_or_default(),
                "run.terminal.failed" | "run.terminal.cancelled" => vec![entry(
                    "error",
                    body["reason"]
                        .as_str()
                        .unwrap_or("el agente terminó con error")
                        .to_string(),
                )],
                "tool_batch.effect.started" => body["tool_name"]
                    .as_str()
                    .map(|tool| vec![entry("command", tool.to_string())])
                    .unwrap_or_default(),
                "tool_batch.effect.terminal" if body["outcome"]["kind"] == "failed" => {
                    let reason = body["outcome"]["reason"]
                        .as_str()
                        .and_then(|reason| reason.lines().next())
                        .unwrap_or("sin detalle");
                    vec![entry("info", format!("Falló una herramienta: {reason}"))]
                }
                _ => Vec::new(),
            }
        }
        DeliveryAgent::Opencode => {
            let part = &value["part"];
            match value["type"].as_str() {
                Some("text") => part["text"]
                    .as_str()
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| vec![entry("message", text.to_string())])
                    .unwrap_or_default(),
                Some("tool_use") if part["state"]["status"] == "error" => vec![entry(
                    "info",
                    format!(
                        "No se pudo {}: {}",
                        opencode_tool_summary(part),
                        part["state"]["error"]
                            .as_str()
                            .and_then(|error| error.lines().next())
                            .unwrap_or("sin detalle")
                    ),
                )],
                Some("tool_use") => vec![entry("command", opencode_tool_summary(part))],
                Some("error") => vec![entry("error", opencode_error(&value))],
                _ => Vec::new(),
            }
        }
        DeliveryAgent::Shell => Vec::new(),
    }
}

/// OpenCode tools that only read; any other may have changed something.
const OPENCODE_READ_TOOLS: [&str; 9] = [
    "read",
    "glob",
    "grep",
    "list",
    "todoread",
    "todowrite",
    "webfetch",
    "websearch",
    "codesearch",
];

/// An OpenCode tool call in one line: its command, else the tool and what
/// it was on.
fn opencode_tool_summary(part: &Value) -> String {
    let input = &part["state"]["input"];
    let tool = part["tool"].as_str().unwrap_or("herramienta");
    match input["command"].as_str() {
        Some(command) => command.to_string(),
        None => match input["filePath"]
            .as_str()
            .or_else(|| input["pattern"].as_str())
            .or_else(|| input["path"].as_str())
        {
            Some(detail) => format!("{tool}: {detail}"),
            None => tool.to_string(),
        },
    }
}

/// Why an OpenCode run failed, from its `error` event.
fn opencode_error(value: &Value) -> String {
    let error = &value["error"];
    error["data"]["message"]
        .as_str()
        .or_else(|| error["message"].as_str())
        .or_else(|| value["message"].as_str())
        .or_else(|| error["name"].as_str())
        .map(provider_error_reason)
        .unwrap_or_else(|| "el agente terminó con error".to_string())
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

/// Whether a job's output shows it did more than read. Its exclusive
/// resource is then in an unknown state unless the job ends cleanly.
pub fn acted(job: &DeliveryJob, events: &str) -> bool {
    let slashes = |text: &str| text.replace('\\', "/");
    if job.agent == DeliveryAgent::Shell {
        return true;
    }
    events
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .any(|value| match job.agent {
            DeliveryAgent::Shell => true,
            DeliveryAgent::Muse => {
                let (kind, body) = muse_record(&value);
                kind == "tool_batch.effect.started"
                    && !MUSE_READ_TOOLS.contains(&body["tool_name"].as_str().unwrap_or_default())
            }
            // A call its rules refused did nothing.
            DeliveryAgent::Opencode => {
                value["type"] == "tool_use"
                    && value["part"]["state"]["status"] == "completed"
                    && !OPENCODE_READ_TOOLS
                        .contains(&value["part"]["tool"].as_str().unwrap_or_default())
            }
            // Its sandbox lets it run anything inside the worktree.
            DeliveryAgent::Codex => matches!(
                value["item"]["type"].as_str(),
                Some("command_execution" | "file_change" | "mcp_tool_call")
            ),
            DeliveryAgent::Claude => value["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|block| block["type"] == "tool_use")
                .any(|block| {
                    let name = block["name"].as_str().unwrap_or_default();
                    let command = slashes(block["input"]["command"].as_str().unwrap_or_default());
                    name.starts_with("mcp__")
                        || matches!(name, "Write" | "Edit" | "NotebookEdit")
                        || (name == "Bash"
                            && (job.access == DeliveryAccess::Full
                                || job.allowed_commands.iter().any(|allowed| {
                                    command.trim_start().starts_with(&slashes(allowed))
                                })
                                || job
                                    .frozen_scripts
                                    .iter()
                                    .any(|script| command.contains(&slashes(&script.copy)))))
                }),
        })
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
            DeliveryAgent::Muse => {
                let (kind, body) = muse_record(&value);
                match kind {
                    "run.terminal.completed" => {
                        last_message = body["text"].as_str().map(str::to_string);
                    }
                    "run.terminal.failed" | "run.terminal.cancelled" => {
                        outcome.error = Some(
                            body["reason"]
                                .as_str()
                                .unwrap_or("el agente terminó con error")
                                .to_string(),
                        );
                    }
                    _ => {}
                }
            }
            DeliveryAgent::Opencode => match value["type"].as_str() {
                Some("text") => {
                    last_message = value["part"]["text"].as_str().map(str::to_string);
                }
                Some("step_finish") => {
                    if let Some(step) = observe(agent, line).usage {
                        outcome.usage = Some(match &outcome.usage {
                            Some(before) => before.plus(&step),
                            None => step,
                        });
                    }
                }
                Some("error") => outcome.error = Some(opencode_error(&value)),
                _ => {}
            },
            DeliveryAgent::Shell => {}
        }
    }
    outcome.error = outcome.error.map(|error| model_unavailable(agent, error));
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
    let mut result: DeliveryJobResult = serde_json::from_value(value.clone())
        .map_err(|error| format!("resultado con formato inválido: {error}"))?;
    settle_confirmations(&mut result);
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

/// Agents still report what they fixed or confirmed as findings ("Fixed
/// (decision 1…)", "MEDIUM tx hash: addressed…"), which kept tasks in rounds
/// that were done: such a finding, or one whose text after its label starts
/// that way, moves to `addressed`, and a `findings` result left without
/// findings is a pass.
fn settle_confirmations(result: &mut DeliveryJobResult) {
    let (confirmed, open): (Vec<String>, Vec<String>) = std::mem::take(&mut result.findings)
        .into_iter()
        .partition(|finding| confirmation(finding));
    result.findings = open;
    result.addressed.extend(confirmed);
    if result.status == "findings" && result.findings.is_empty() {
        result.status = "pass".to_string();
    }
}

/// Whether a finding says something was fixed, addressed, verified or OK.
fn confirmation(finding: &str) -> bool {
    let text = finding.trim_start_matches(|c: char| !c.is_alphanumeric());
    let after_label = text.split_once(':').map(|(_, rest)| rest.trim_start());
    [Some(text), after_label].into_iter().flatten().any(|text| {
        let word: String = text.chars().take_while(|c| c.is_alphabetic()).collect();
        let rest = text[word.len()..].trim_start();
        match word.to_lowercase().as_str() {
            "fixed" | "addressed" | "verified" => true,
            // "OK button overlaps" is a defect; "OK: …", "OK, …" are not.
            "ok" => rest.is_empty() || rest.starts_with([':', ',', '.', ';', '-', '—', '(']),
            _ => false,
        }
    })
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

/// A provider's refusal of the model asked for, said plainly: the account
/// cannot use the stage's model. Any other error stays as it is.
fn model_unavailable(agent: DeliveryAgent, error: String) -> String {
    let lower = error.to_lowercase();
    let refused = lower.contains("model")
        && [
            "not supported",
            "does not exist",
            "may not exist",
            "not have access",
            "model_not_found",
            "model not found",
        ]
        .iter()
        .any(|phrase| lower.contains(phrase));
    if !refused {
        return error;
    }
    format!(
        "{MODEL_REFUSED} {}: cámbialo en Ajustes. ({error})",
        super::service::agent_name(agent)
    )
}

/// How a job's error starts when the account refused its model.
pub(super) const MODEL_REFUSED: &str =
    "El modelo de la etapa no está disponible para esta cuenta de";

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
pub(crate) mod tests {
    use super::*;
    use crate::delivery::model::{DeliveryFrozenCopy, DeliveryJobStatus, DeliveryResultState};

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
            package: None,
            worktree: PathBuf::from("/repo-wt/K-1"),
            branch: "delivery/k-1".into(),
            base_ref: "main".into(),
            base_commit: "abc".into(),
            state: "intake".into(),
            contract_version: 1,
            read_dirs: Vec::new(),
            created_at_ms: 1,
            updated_at_ms: 1,
            removed_at_ms: None,
            archived_at_ms: None,
            blockers: Vec::new(),
            blocked_reason: String::new(),
            unblocked: None,
            checks: Vec::new(),
            setup: Vec::new(),
            parent: None,
            parent_read_at_ms: None,
            skip_ci: false,
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
            session_model: None,
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
            process_started: None,
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
            resolved_at_ms: None,
            resolved_note: None,
            allowed_commands: Vec::new(),
            qa_browser: false,
            decisions: Vec::new(),
            qa_followup: Vec::new(),
            qa_environment: String::new(),
            qa_stack: None,
            prepare: None,
            reverted: Vec::new(),
            task_checks: Vec::new(),
            round: Vec::new(),
            guide_rules: Vec::new(),
            needs_environment: false,
            waits_environment: None,
            reasoning: None,
            platform: None,
            ran_on: None,
            read_dirs: Vec::new(),
            write_dirs: Vec::new(),
            stage_default: false,
            check_env: Default::default(),
            frozen_scripts: Vec::new(),
            resumes: None,
            resume_note: None,
            merge_conflicts: None,
            activity: None,
            activity_at_ms: None,
            current_command: None,
            current_command_at_ms: None,
            timing: None,
            plan: Vec::new(),
            usage: None,
            visual_record: None,
            visual_record_error: None,
            visual_record_source: None,
            visual_record_extras: Vec::new(),
            visual_record_warnings: Vec::new(),
            substituted: None,
            retry_of: None,
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
        let error = outcome.error.expect("error");
        assert!(
            error.starts_with("El modelo de la etapa no está disponible para esta cuenta de Codex: cámbialo en Ajustes.")
                && error.contains("The 'gpt-6.1-sol' model is not supported"),
            "{error}"
        );
        assert!(outcome.result.is_none());
    }

    #[test]
    fn a_model_the_account_lacks_is_said_plainly_and_other_errors_stay() {
        let claude = r#"{"type":"result","subtype":"success","is_error":true,"result":"There's an issue with the selected model (claude-nope). It may not exist or you may not have access to it. Run --model to pick a different model."}"#;
        let error = read_outcome(DeliveryAgent::Claude, claude, None)
            .error
            .expect("error");
        assert!(
            error.contains("cuenta de Claude Code: cámbialo en Ajustes"),
            "{error}"
        );
        assert!(error.contains("claude-nope"), "{error}");
        let other = r#"{"type":"result","subtype":"success","is_error":true,"result":"API Error: 529 overloaded"}"#;
        assert_eq!(
            read_outcome(DeliveryAgent::Claude, other, None)
                .error
                .as_deref(),
            Some("API Error: 529 overloaded")
        );
    }

    // A real `muse exec --json` run the provider refused.
    const MUSE_FAILED: &str = include_str!("fixtures/muse-failed.jsonl");
    // Assembled from the record shapes Muse writes to its session logs.
    const MUSE_OK: &str = include_str!("fixtures/muse-ok.jsonl");

    // A real `opencode run --format json` run (opencode 1.19.5): a write, a
    // command its rules refused, a command, then the result as its answer.
    const OPENCODE_OK: &str = include_str!("fixtures/opencode-ok.jsonl");

    #[test]
    fn opencode_run_yields_session_result_summed_use_and_what_it_did() {
        let outcome = read_outcome(DeliveryAgent::Opencode, OPENCODE_OK, None);
        assert_eq!(
            outcome.provider_session_id.as_deref(),
            Some("ses_e7872946bffexCQDWUZfzeWg79")
        );
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.result.expect("result").changed_paths, ["a.txt"]);
        // Its three steps together: input with what it cached for later,
        // output with its reasoning.
        assert_eq!(
            outcome.usage,
            Some(DeliveryUsage {
                input_tokens: 16_733,
                cached_tokens: 38_528,
                output_tokens: 163,
                cost_micro_usd: Some(0),
            })
        );
        let entries: Vec<(String, String)> = OPENCODE_OK
            .lines()
            .flat_map(|line| log_entries(DeliveryAgent::Opencode, line))
            .map(|entry| (entry.kind, entry.text))
            .filter(|(kind, _)| kind != "message")
            .collect();
        assert_eq!(entries[0], ("command".into(), "write: /repo/a.txt".into()));
        assert!(entries[1].1.starts_with("No se pudo printf"), "{entries:?}");
        assert_eq!(entries[2], ("command".into(), "ls".into()));
        let job = sample_job(DeliveryAgent::Opencode);
        assert!(acted(&job, OPENCODE_OK));
        // Calls its rules refused did nothing.
        let refused: String = OPENCODE_OK
            .lines()
            .filter(|line| !line.contains(r#""status":"completed""#))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!acted(&job, &refused));
    }

    #[test]
    fn an_opencode_job_runs_by_rules_for_its_stage() {
        let dir = std::env::temp_dir().join(format!("tinto-opencode-{}", uuid::Uuid::new_v4()));
        let paths = JobPaths::in_dir(dir);
        let mut task = sample_task();
        task.checks = vec!["npm test".into(), "npm run lint".into()];
        let rules = |job: &DeliveryJob| {
            let launch =
                opencode_launch(job, &task, &paths, Some(PathBuf::from("opencode"))).unwrap();
            let (name, config) = &launch.env[0];
            assert_eq!(name, OPENCODE_CONFIG_ENV);
            let config: Value = serde_json::from_str(config).unwrap();
            (launch, config["permission"].clone())
        };
        let mut job = sample_job(DeliveryAgent::Opencode);
        job.model = Some("opencode/big-pickle".into());
        job.reasoning = Some("high".into());
        job.allowed_commands = vec!["npm test".into()];
        let (launch, writer) = rules(&job);
        assert_eq!(
            launch.args,
            [
                "run",
                "--format",
                "json",
                "--dir",
                "/repo-wt/K-1",
                "--model",
                "opencode/big-pickle",
                "--variant",
                "high"
            ]
        );
        // Its project is the worktree, whatever folder Tinto started in.
        assert_eq!(
            launch.env[1],
            ("PWD".to_string(), "/repo-wt/K-1".to_string())
        );
        assert!(launch.stdin.contains("Write the failing test."));
        assert!(launch
            .stdin
            .contains("Your final message must be only the structured result"));
        // A stage that writes: any edit and command but Tinto's own steps.
        assert_eq!(writer["edit"]["*"], "allow");
        assert_eq!(writer["bash"]["*"], "allow");
        assert_eq!(writer["bash"]["git push *"], "deny");
        assert_eq!(writer["bash"]["git -C /repo-wt/K-1 commit *"], "deny");
        assert_eq!(writer["external_directory"]["*"], "deny");
        // Tinto runs the suites: not as written, but lint and narrower runs.
        assert_eq!(writer["bash"]["npm test"], "deny");
        assert!(writer["bash"].get("npm run lint").is_none());

        // Review reads: no edits, only the checks and commands that read.
        job.role = "review".into();
        job.writes = false;
        job.read_dirs = vec!["/repo/.agent/K-1".into()];
        let (_, review) = rules(&job);
        assert_eq!(review["edit"]["*"], "deny");
        assert_eq!(review["bash"]["*"], "deny");
        assert_eq!(review["bash"]["npm test"], "deny");
        assert!(review["bash"].get("npm test *").is_none());
        assert_eq!(review["bash"]["git diff *"], "allow");
        assert_eq!(review["bash"]["git -C /repo-wt/K-1 diff *"], "allow");
        assert_eq!(review["bash"]["git commit *"], "deny");
        assert_eq!(review["external_directory"]["/repo/.agent/K-1/**"], "allow");
        assert_eq!(review["edit"]["../../repo/.agent/K-1/**"], "deny");
        assert_eq!(review["webfetch"], "deny");

        // QA also writes its record in its QA folder, outside the worktree.
        job.role = QA_ROLE.into();
        let (_, qa) = rules(&job);
        let folder = qa_folder(&paths, false);
        assert_eq!(
            qa["edit"][format!("{}/**", relative_path("/repo-wt/K-1", &folder))],
            "allow"
        );
        assert_eq!(qa["external_directory"][format!("{folder}/**")], "allow");
        assert_eq!(qa["webfetch"], "allow");

        // Full access, and a paused job going on in its session.
        job.access = DeliveryAccess::Full;
        job.resumes = Some("j0".into());
        job.provider_session_id = Some("ses_1".into());
        let (resumed, full) = rules(&job);
        assert_eq!(full["bash"], "allow");
        assert_eq!(full["edit"], "allow");
        assert!(resumed
            .args
            .windows(2)
            .any(|pair| pair == ["--session", "ses_1"]));
        assert_eq!(resumed.stdin, RESUME_PROMPT);
    }

    #[test]
    fn muse_success_yields_session_plan_and_result() {
        let outcome = read_outcome(DeliveryAgent::Muse, MUSE_OK, None);
        assert_eq!(
            outcome.provider_session_id.as_deref(),
            Some("01a14513-07d7-7f39-8201-b2ba63d1231c")
        );
        assert_eq!(outcome.error, None);
        let result = outcome.result.expect("result");
        assert_eq!(result.changed_paths, vec!["hello.txt".to_string()]);

        let plan = MUSE_OK
            .lines()
            .find_map(|line| observe(DeliveryAgent::Muse, line).plan)
            .expect("plan");
        assert_eq!(plan[1].text, "Create hello.txt");
        assert_eq!(plan[1].status, "in_progress");

        let commands: Vec<_> = MUSE_OK
            .lines()
            .flat_map(|line| log_entries(DeliveryAgent::Muse, line))
            .filter(|entry| entry.kind == "command")
            .map(|entry| entry.text)
            .collect();
        assert_eq!(commands, ["read_file", "write_file"]);
        assert!(acted(&sample_job(DeliveryAgent::Muse), MUSE_OK));
    }

    #[test]
    fn muse_reads_alone_do_not_count_as_acting() {
        let reads: String = MUSE_OK
            .lines()
            .filter(|line| !line.contains("write_file"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!acted(&sample_job(DeliveryAgent::Muse), &reads));
    }

    #[test]
    fn muse_failure_reports_the_provider_reason() {
        let outcome = read_outcome(DeliveryAgent::Muse, MUSE_FAILED, None);
        let error = outcome.error.expect("error");
        assert!(error.contains("Billing verification failed"), "{error}");
        assert!(outcome.result.is_none());
        let entries: Vec<_> = MUSE_FAILED
            .lines()
            .flat_map(|line| log_entries(DeliveryAgent::Muse, line))
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "error");
    }

    #[test]
    fn muse_job_names_its_session_and_reads_its_prompt_from_a_file() {
        let dir = std::env::temp_dir().join(format!("tinto-muse-job-{}", uuid::Uuid::new_v4()));
        let paths = JobPaths::in_dir(dir.clone());
        paths.prepare().unwrap();
        let mut job = sample_job(DeliveryAgent::Muse);
        job.model = Some("muse-spark-1.3".into());
        job.reasoning = Some("high".into());
        let launch =
            muse_launch(&job, &sample_task(), &paths, Some(PathBuf::from("muse"))).unwrap();
        let session = launch.provider_session_id.clone().expect("session");
        let arg = |flag: &str| {
            let at = launch.args.iter().position(|arg| arg == flag).expect(flag);
            launch.args[at + 1].clone()
        };
        assert_eq!(arg("--session-id"), session);
        assert_eq!(arg("--approval-mode"), "never");
        assert_eq!(arg("--model"), "muse-spark-1.3");
        assert_eq!(arg("--reasoning-effort"), "high");
        assert!(!launch.args.contains(&"--yolo".to_string()));
        assert!(std::fs::read_to_string(&paths.prompt)
            .unwrap()
            .contains("Write the failing test."));

        // Resuming a paused job goes on in the same session.
        job.resumes = Some("j0".into());
        job.provider_session_id = Some(session.clone());
        job.access = DeliveryAccess::Full;
        let resumed =
            muse_launch(&job, &sample_task(), &paths, Some(PathBuf::from("muse"))).unwrap();
        assert_eq!(resumed.provider_session_id, Some(session));
        assert!(resumed.args.contains(&"--yolo".to_string()));
        assert_eq!(
            std::fs::read_to_string(&paths.prompt).unwrap(),
            RESUME_PROMPT
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn muse_qa_gets_the_headless_browser_for_its_run() {
        let dir = std::env::temp_dir().join(format!("tinto-muse-qa-{}", uuid::Uuid::new_v4()));
        let paths = JobPaths::in_dir(dir.clone());
        paths.prepare().unwrap();
        let mut job = sample_job(DeliveryAgent::Muse);
        job.role = QA_ROLE.into();
        job.qa_browser = true;
        let launch =
            muse_launch(&job, &sample_task(), &paths, Some(PathBuf::from("muse"))).unwrap();
        let at = launch.args.iter().position(|arg| arg == "-c").expect("-c");
        let (key, value) = launch.args[at + 1].split_once('=').unwrap();
        assert_eq!(key, "mcpServers.playwright");
        let server: Value = serde_json::from_str(value).unwrap();
        let args = server["args"].as_array().unwrap();
        assert!(args.contains(&Value::from(PLAYWRIGHT_MCP)));
        assert!(args.contains(&Value::from(plain_path(&paths.qa_output))));
        assert!(paths.qa_browser_script.is_file());
        assert!(launch
            .args
            .windows(2)
            .any(|pair| pair == ["--sandbox-network", "enabled"]));
        std::fs::remove_dir_all(dir).unwrap();
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
    fn findings_that_confirm_a_fix_are_addressed_not_open() {
        // Seen in jobs 7c50b2b3 and da6ed9d5: confirmations kept done rounds open.
        let result = |findings: &[&str]| {
            parse_result(&serde_json::json!({
                "status": "findings", "summary": "Round done.", "findings": findings,
            }))
            .unwrap()
        };
        let done = result(&[
            "Fixed (decision 1): the hash is checked",
            "MEDIUM tx hash: addressed in src/tx.ts:40",
            "- Verified the migration runs twice",
            "OK: lint passes",
        ]);
        assert_eq!(done.status, "pass");
        assert!(done.findings.is_empty());
        assert_eq!(done.addressed.len(), 4);
        // Defects that merely start with a similar word stay open.
        let open = result(&["OK button overlaps the footer", "Fixes are missing a test"]);
        assert_eq!(open.status, "findings");
        assert_eq!(open.findings.len(), 2);
        assert!(open.addressed.is_empty());
        // The schema asks for them apart, and for a size budget.
        let schema: Value = serde_json::from_str(RESULT_SCHEMA).unwrap();
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&Value::from("addressed")));
        assert!(schema["description"].as_str().unwrap().contains("4000"));
    }

    #[test]
    fn launches_carry_sandbox_schema_and_prompt() {
        let mut task = sample_task();
        task.distro = Some("Ubuntu".into());
        let job = sample_job(DeliveryAgent::Codex);
        let paths = JobPaths::in_dir(PathBuf::from(
            r"C:\Users\me\AppData\Roaming\tinto\delivery\jobs\j1",
        ));
        let launch = codex_launch(&job, &task, &paths, None, &SandboxPlan::default()).unwrap();
        assert_eq!(launch.program, PathBuf::from("wsl.exe"));
        let script = launch.args.last().unwrap();
        assert!(script.contains("-s workspace-write"), "{script}");
        assert!(script.contains("--output-schema '/mnt/c/Users/me/AppData/Roaming/tinto/delivery/jobs/j1/result.schema.json'"), "{script}");
        assert!(script.contains(" -m 'gpt-6-astra' -"), "{script}");
        assert!(launch
            .stdin
            .contains("Assignment:\nWrite the failing test."));
        assert_eq!(
            launch.wsl_job,
            Some(WslJob {
                distro: "Ubuntu".into(),
                job_id: "j1".into()
            })
        );
        let leader = &launch.args[launch.args.len() - 3];
        assert_eq!(leader, r#"export TINTO_DELIVERY_JOB='j1'; eval "$1""#);

        let mut claude = sample_job(DeliveryAgent::Claude);
        claude.access = DeliveryAccess::Full;
        claude.writes = false;
        let launch = claude_launch(&claude, &task, &paths, None, None).unwrap();
        let script = launch.args.last().unwrap();
        assert!(
            script.contains("--permission-mode bypassPermissions"),
            "{script}"
        );
        assert!(launch.provider_session_id.is_some());
        assert!(launch.stdin.contains("This job is read-only"));
        assert!(!script.contains("--allowedTools"), "{script}");
    }

    #[test]
    fn qa_with_a_browser_is_asked_for_the_visual_changes_record() {
        let task = sample_task();
        let paths = JobPaths::in_dir(PathBuf::from(r"C:\jobs\j1"));
        let mut qa = sample_job(DeliveryAgent::Claude);
        qa.role = QA_ROLE.into();
        let prompt = job_prompt(&qa, &task, &paths, false, &[]);
        assert!(!prompt.contains("visual changes record"), "{prompt}");
        // Command-line QA still gets the record guide.
        assert!(prompt.contains("QA record guide"), "{prompt}");
        assert!(prompt.contains(&format!("File: `{}.md` in your QA folder", task.key)));
        assert!(prompt.contains("Command-line QA (no UI): one section per suite"));
        assert!(prompt.contains("a matrix table"));
        assert!(prompt.contains("Never secrets"));
        qa.qa_browser = true;
        let prompt = job_prompt(&qa, &task, &paths, false, &[]);
        assert!(
            prompt.contains(&format!("in your QA folder as `{}.md`", task.key)),
            "{prompt}"
        );
        assert!(prompt.contains("desktop 1280×800 and mobile 375×812"));
        assert!(prompt.contains("Visual QA: one section per change you checked"));
        assert!(prompt.contains("| 1. <Change name> | pass / fail / not verifiable"));
    }

    #[test]
    fn a_resumed_job_continues_its_session_with_a_short_prompt() {
        let mut task = sample_task();
        task.distro = Some("Ubuntu".into());
        let paths = JobPaths::in_dir(PathBuf::from(r"C:\jobs\j2"));
        let mut codex = sample_job(DeliveryAgent::Codex);
        codex.resumes = Some("j1".into());
        codex.provider_session_id = Some("thread-1".into());
        let launch = codex_launch(&codex, &task, &paths, None, &SandboxPlan::default()).unwrap();
        let script = launch.args.last().unwrap();
        assert!(
            script.contains(r#"exec codex exec resume --json -c 'sandbox_mode="workspace-write"'"#),
            "{script}"
        );
        assert!(!script.contains(" -C "), "{script}");
        assert!(script.ends_with(" 'thread-1' -"), "{script}");
        assert_eq!(launch.stdin, RESUME_PROMPT);
        assert_eq!(launch.provider_session_id.as_deref(), Some("thread-1"));

        let mut claude = sample_job(DeliveryAgent::Claude);
        claude.resumes = Some("j1".into());
        claude.provider_session_id = Some("session-1".into());
        let launch = claude_launch(&claude, &task, &paths, None, None).unwrap();
        let script = launch.args.last().unwrap();
        assert!(script.contains("--resume 'session-1'"), "{script}");
        assert!(!script.contains("--session-id"), "{script}");
        assert_eq!(launch.stdin, RESUME_PROMPT);
        // A job that stopped on its own goes on with why it stopped.
        claude.resume_note = Some("Se agotó su tiempo: sigue.".into());
        let launch = claude_launch(&claude, &task, &paths, None, None).unwrap();
        assert_eq!(launch.stdin, "Se agotó su tiempo: sigue.");

        claude.provider_session_id = None;
        assert!(claude_launch(&claude, &task, &paths, None, None).is_err());
    }

    #[test]
    fn every_stage_is_told_to_read_the_tasks_planning_folders() {
        let mut task = sample_task();
        let issue = "C:/repo/.agent/backlog-delivery/issues/K-1".to_string();
        task.read_dirs = vec![issue.clone()];
        let paths = JobPaths::in_dir(PathBuf::from(r"C:\jobs\j1"));
        let mut job = sample_job(DeliveryAgent::Codex);
        job.read_dirs = vec![issue.clone(), "C:/repo/docs".into()];
        assert!(job.writes);
        let prompt = job_prompt(&job, &task, &paths, false, &[]);
        assert!(
            prompt.contains(&format!(
                "The task's planning folders, outside the worktree (its contract and context): read what they say about this task before you start, and do not change them: {issue}."
            )),
            "{prompt}"
        );
        assert!(
            prompt.contains("You may also read and list these folders outside the worktree, without changing them: C:/repo/docs."),
            "{prompt}"
        );
    }

    #[test]
    fn a_claude_rate_limit_event_gives_the_window_nearest_its_limit() {
        // As the DPP batch's jobs reported it.
        let line = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1793271600,"rateLimitType":"five_hour","utilization":0.94,"overageStatus":"rejected","overageDisabledReason":"out_of_credits","isUsingOverage":false,"surpassedThreshold":0.9,"unifiedWindows":{"five_hour":{"utilization":0.94,"resetsAt":1793271600,"status":"allowed_warning","surpassedThreshold":0.9},"seven_day":{"utilization":0.37,"resetsAt":1793610000,"status":"allowed"}}},"session_id":"4b90393f"}"#;
        assert_eq!(
            rate_limit(DeliveryAgent::Claude, line),
            Some(DeliveryQuota {
                agent: DeliveryAgent::Claude,
                window: "five_hour".into(),
                utilization: 94,
                resets_at_ms: 1_793_271_600_000,
                seen_at_ms: 0,
            })
        );
        assert_eq!(
            rate_limit(DeliveryAgent::Claude, CLAUDE_OK.lines().next().unwrap()),
            None
        );
    }

    #[test]
    fn agent_events_tell_activity_plan_and_usage() {
        let todo = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"TodoWrite","input":{"todos":[{"content":"Leer el test","status":"completed"},{"content":"Arreglar el parser","status":"in_progress"}]}}]}}"#;
        let seen = observe(DeliveryAgent::Claude, todo);
        assert_eq!(seen.plan.unwrap()[1].status, "in_progress");
        assert!(seen.activity.is_some());
        let result = r#"{"type":"result","total_cost_usd":0.25,"usage":{"input_tokens":10,"cache_creation_input_tokens":5,"cache_read_input_tokens":100,"output_tokens":7}}"#;
        assert_eq!(
            observe(DeliveryAgent::Claude, result).usage,
            Some(DeliveryUsage {
                input_tokens: 15,
                cached_tokens: 100,
                output_tokens: 7,
                cost_micro_usd: Some(250_000),
            })
        );
        let list = r#"{"type":"item.updated","item":{"type":"todo_list","items":[{"text":"Leer","completed":true},{"text":"Arreglar","completed":false}]}}"#;
        let plan = observe(DeliveryAgent::Codex, list).plan.unwrap();
        assert_eq!(
            (plan[0].status.as_str(), plan[1].status.as_str()),
            ("completed", "pending")
        );
        assert_eq!(observe(DeliveryAgent::Codex, "{}"), Observation::default());
    }

    /// The running command (and since when) after a line changed it.
    type CommandChange = (usize, Option<(String, u64)>);

    /// Feeds `events` one line a second; returns the running command at
    /// each change (by line) and the clock.
    fn clocked(agent: DeliveryAgent, events: &str) -> (Vec<CommandChange>, JobClock) {
        let mut clock = JobClock::default();
        let changes = events
            .lines()
            .enumerate()
            .filter_map(|(at, line)| Some((at, clock.line(agent, line, at as u64 * 1000)?)))
            .collect();
        (changes, clock)
    }

    fn slow(command: &str, total_ms: u64, runs: u32, at_ceiling: u32) -> DeliverySlowCommand {
        DeliverySlowCommand {
            command: command.into(),
            total_ms,
            runs,
            at_ceiling,
        }
    }

    #[test]
    fn claude_tool_calls_tell_the_running_command_and_where_the_time_went() {
        let (changes, clock) = clocked(DeliveryAgent::Claude, CLAUDE_OK);
        let first = "printf 'goodbye from claude' > farewell.txt && git status --short";
        let second = "printf 'goodbye from claude' > farewell.txt";
        // The final answer is not a command.
        assert_eq!(
            changes,
            vec![
                (1, Some((first.to_string(), 1000))),
                (3, None),
                (4, Some((second.to_string(), 4000))),
                (5, None),
            ]
        );
        assert_eq!(
            clock.timing(9000),
            DeliveryTiming {
                model_ms: Some(5100),
                commands_ms: 3000,
                turns: Some(4),
                prepare_ms: None,
                slowest: vec![slow(first, 2000, 1, 0), slow(second, 1000, 1, 0)],
            }
        );
    }

    #[test]
    fn claude_commands_repeat_overlap_and_hit_the_bash_ceiling() {
        let mut clock = JobClock::default();
        let call = |id: &str, name: &str, input: &str| {
            format!(
                r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"{name}","input":{input}}}]}},"parent_tool_use_id":null}}"#
            )
        };
        let done = |id: &str, extra: &str| {
            format!(
                r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"{id}"}}]}},"parent_tool_use_id":null{extra}}}"#
            )
        };
        let preflight = r#"{"command":"npm run preflight\n  2>&1 | tail -40"}"#;
        let both = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"a","name":"Bash","input":{"command":"npm run preflight\n  2>&1 | tail -40"}},{"type":"tool_use","id":"b","name":"Read","input":{"file_path":"x.ts"}}]},"parent_tool_use_id":null}"#;
        let running = |command: &str, at: u64| Some(Some((command.to_string(), at)));
        let preflight_line = "npm run preflight 2>&1 | tail -40";
        assert_eq!(
            clock.line(DeliveryAgent::Claude, both, 0),
            running(preflight_line, 0)
        );
        // A subagent's calls are its own.
        let inner = call("c", "Bash", r#"{"command":"ls"}"#).replace(
            r#""parent_tool_use_id":null"#,
            r#""parent_tool_use_id":"b""#,
        );
        assert_eq!(clock.line(DeliveryAgent::Claude, &inner, 500), None);
        // Another call ending leaves the oldest one shown.
        assert_eq!(
            clock.line(DeliveryAgent::Claude, &done("b", ""), 1000),
            None
        );
        let ceiling = r#","tool_use_result":{"stdout":"","timedOutAfterMs":600000}"#;
        assert_eq!(
            clock.line(DeliveryAgent::Claude, &done("a", ceiling), 600_000),
            Some(None)
        );
        clock.line(
            DeliveryAgent::Claude,
            &call("d", "Bash", preflight),
            700_000,
        );
        clock.line(DeliveryAgent::Claude, &done("d", ""), 760_000);
        let test = call("e", "Bash", r#"{"command":"cd /repo-wt/K-1 && npm test"}"#);
        assert_eq!(
            clock.line(DeliveryAgent::Claude, &test, 800_000),
            running("npm test", 800_000)
        );
        // Still running when the job ends: it counts until then.
        assert_eq!(
            clock.timing(1_000_000),
            DeliveryTiming {
                model_ms: None,
                commands_ms: 600_000 + 60_000 + 200_000,
                turns: None,
                prepare_ms: None,
                slowest: vec![
                    slow(preflight_line, 660_000, 2, 1),
                    slow("npm test", 200_000, 1, 0),
                    slow("Read: x.ts", 1000, 1, 0),
                ],
            }
        );
    }

    #[test]
    fn codex_items_tell_the_running_command_without_its_shell() {
        let (changes, clock) = clocked(DeliveryAgent::Codex, CODEX_OK);
        let command = "Set-Content greeting.txt 'hello from a delivery job.'";
        assert_eq!(
            changes,
            vec![(4, Some((command.to_string(), 4000))), (5, None)]
        );
        assert_eq!(
            clock.timing(8000),
            DeliveryTiming {
                model_ms: None,
                commands_ms: 1000,
                turns: None,
                prepare_ms: None,
                slowest: vec![slow(command, 1000, 1, 0)],
            }
        );
        assert_eq!(
            without_shell(
                r#""C:\\WINDOWS\\System32\\WindowsPowerShell\\v1.0\\powershell.exe" -Command 'npm run check-lint'"#
            ),
            "npm run check-lint"
        );
        assert_eq!(without_shell("/bin/bash -lc 'npm test'"), "npm test");
        assert_eq!(
            without_shell("git -c core.quotepath=off status"),
            "git -c core.quotepath=off status"
        );
    }

    #[test]
    fn muse_tool_effects_tell_the_running_tool() {
        let (changes, clock) = clocked(DeliveryAgent::Muse, MUSE_OK);
        assert_eq!(
            changes,
            vec![
                (4, Some(("read_file".to_string(), 4000))),
                (5, None),
                (6, Some(("write_file".to_string(), 6000))),
                (7, None),
            ]
        );
        let timing = clock.timing(10_000);
        assert_eq!(timing.commands_ms, 2000);
        assert_eq!(
            timing.slowest,
            vec![
                slow("read_file", 1000, 1, 0),
                slow("write_file", 1000, 1, 0)
            ]
        );
    }

    #[test]
    fn claude_without_full_access_runs_any_command_but_tintos_steps() {
        let mut task = sample_task();
        task.distro = Some("Ubuntu".into());
        let paths = JobPaths::in_dir(PathBuf::from(
            r"C:\Users\me\AppData\Roaming\tinto\delivery\jobs\j1",
        ));
        task.checks = vec!["npm test".into(), "npm run lint".into()];
        let mut job = sample_job(DeliveryAgent::Claude);
        job.allowed_commands = vec!["npm test".into(), "cargo test".into()];
        let launch = claude_launch(&job, &task, &paths, None, None).unwrap();
        let script = launch.args.last().unwrap();
        assert!(script.contains("--permission-mode acceptEdits"), "{script}");
        // None of the user's MCP servers (Drive, Docs, Tinto's own).
        assert!(script.contains(" --strict-mcp-config"), "{script}");
        // Neither the tools that reach beyond the task nor, outside QA, the web.
        assert!(
            script.contains(" --allowedTools 'Bash' --disallowedTools 'CronCreate'"),
            "{script}"
        );
        for tool in [
            "'PushNotification'",
            "'RemoteTrigger'",
            "'Workflow'",
            "'WebFetch'",
        ] {
            assert!(script.contains(tool), "{tool}: {script}");
        }
        // Any command, compound ones too, but Tinto's own steps, also
        // spelled with `git -C <worktree>`, and the suites Tinto runs as
        // they are written: the lint check stays, as writers run it.
        assert!(
            script.contains(
                " 'Bash(git commit:*)' 'Bash(git -C /repo-wt/K-1 commit:*)' 'Bash(git push:*)'"
            ),
            "{script}"
        );
        assert!(
            script.ends_with(" 'Bash(cargo install:*)' 'Bash(npm test)'"),
            "{script}"
        );
        assert!(launch.stdin.contains(
            "Tinto refuses these checks exactly as written, since it runs them itself on your result: `npm test`. To test what you change, run the test runner on just those files"
        ));
        assert!(launch
            .stdin
            .contains("You can run any shell command, compound ones included"));
        assert!(launch.stdin.contains("refuses only these"));
        assert!(launch
            .stdin
            .contains("The repo's checks: npm test; cargo test."));
        // A stage that writes cannot reach the main checkout.
        assert!(!script.contains("--add-dir"), "{script}");

        job.writes = false;
        job.allowed_commands.clear();
        let launch = claude_launch(&job, &task, &paths, None, None).unwrap();
        let script = launch.args.last().unwrap();
        assert!(script.contains("--permission-mode default"), "{script}");
        // A read-only stage reads the main checkout, where the contract is.
        assert!(script.contains(" '--add-dir' '/repo'"), "{script}");
        assert!(launch.stdin.contains("read the main checkout at /repo "));
        assert!(
            script.contains(" --allowedTools 'Bash' --disallowedTools"),
            "{script}"
        );
        assert!(!launch.stdin.contains("The repo's checks"));

        // With full access only the tools no job needs are left out.
        job.access = DeliveryAccess::Full;
        let launch = claude_launch(&job, &task, &paths, None, None).unwrap();
        let script = launch.args.last().unwrap();
        assert!(script.contains(" --strict-mcp-config"), "{script}");
        assert!(
            script.contains("--disallowedTools 'CronCreate'"),
            "{script}"
        );
        assert!(
            !script.contains("Bash(") && !script.contains("WebFetch"),
            "{script}"
        );
        assert!(!launch.stdin.contains("Tinto refuses these checks"));
    }

    #[test]
    fn a_writing_job_reads_the_named_folders_and_gets_the_check_variables() {
        let mut task = sample_task();
        task.distro = Some("Ubuntu".into());
        let paths = JobPaths::in_dir(PathBuf::from(
            r"C:\Users\me\AppData\Roaming\tinto\delivery\jobs\j1",
        ));
        let mut job = sample_job(DeliveryAgent::Claude);
        job.read_dirs = vec!["/repo/docs/contracts".into()];
        job.check_env = [(
            "TEST_DB_MONGO_URL".to_string(),
            "mongodb://localhost:27017/test".to_string(),
        )]
        .into();
        let launch = claude_launch(&job, &task, &paths, None, None).unwrap();
        let script = launch.args.last().unwrap();
        // Read only: a rule, never a working directory it could edit.
        assert!(
            script.contains(" 'Read(//repo/docs/contracts/**)'"),
            "{script}"
        );
        assert!(!script.contains("--add-dir"), "{script}");
        assert!(launch.stdin.contains(
            "read and list these folders outside the worktree, without changing them: /repo/docs/contracts."
        ));
        // Windows' environment does not reach WSL: the script exports them.
        assert!(
            launch.args.iter().any(|arg| {
                arg.starts_with(
                "export TINTO_DELIVERY_JOB='j1' TEST_DB_MONGO_URL='mongodb://localhost:27017/test';"
            )
            }),
            "{:?}",
            launch.args
        );
    }

    #[test]
    #[cfg(windows)]
    fn qa_jobs_get_network_the_main_checkout_and_the_browser() {
        let mut task = sample_task();
        task.distro = Some("Ubuntu".into());
        task.repo = PathBuf::from("/home/me/repo");
        let dir = tempfile::tempdir().unwrap();
        let paths = JobPaths::in_dir(dir.path().join("j1"));
        let qa_output = wsl_path(&paths.qa_output).unwrap();
        let mut job = sample_job(DeliveryAgent::Claude);
        job.role = QA_ROLE.into();
        job.writes = false;
        job.qa_browser = true;
        job.allowed_commands = vec!["agentos skills install".into()];
        let launch = claude_launch(&job, &task, &paths, None, None).unwrap();
        let script = launch.args.last().unwrap();
        assert!(
            script.contains(&format!(" '--add-dir' '/home/me/repo' '{qa_output}'")),
            "{script}"
        );
        assert!(
            script.contains(r#""command":"/mnt/c/Windows/System32/cmd.exe""#),
            "{script}"
        );
        assert!(script.contains(r#""args":["/c","node","#), "{script}");
        assert!(script.contains(r#""@playwright/mcp@0.0.82"]"#), "{script}");
        // Its evidence goes in its QA folder.
        assert!(
            script.contains(&format!(
                " --allowedTools 'Bash' 'Edit(/{qa_output}/**)' 'mcp__playwright' --disallowedTools"
            )),
            "{script}"
        );
        assert!(launch
            .stdin
            .contains(&format!("Your QA folder is {qa_output}.")));
        assert!(paths.qa_output.is_dir());
        assert_eq!(
            std::fs::read_to_string(&paths.qa_browser_script).unwrap(),
            QA_BROWSER_SCRIPT
        );
        assert!(launch.stdin.contains("main checkout at /home/me/repo"));
        assert!(launch.stdin.contains("headless browser"));

        let mut codex = sample_job(DeliveryAgent::Codex);
        codex.role = QA_ROLE.into();
        codex.qa_browser = true;
        let launch = codex_launch(&codex, &task, &paths, None, &SandboxPlan::default()).unwrap();
        let script = launch.args.last().unwrap();
        assert!(
            script.contains(&format!(" '--add-dir' '{qa_output}' '-c'")),
            "{script}"
        );
        assert!(
            script.contains(" '-c' 'sandbox_workspace_write.network_access=true'"),
            "{script}"
        );
        assert!(
            script.contains(
                r#" '-c' 'mcp_servers.playwright.command="/mnt/c/Windows/System32/cmd.exe"'"#
            ),
            "{script}"
        );
        assert!(
            script.contains(r#"'mcp_servers.playwright.default_tools_approval_mode="approve"' '-c' 'mcp_servers.playwright.tool_timeout_sec=180' -"#),
            "{script}"
        );

        codex.role = "review".into();
        let launch = codex_launch(&codex, &task, &paths, None, &SandboxPlan::default()).unwrap();
        assert!(!launch.args.last().unwrap().contains("network_access"));
        assert!(!launch.stdin.contains("QA stage"));
    }

    #[test]
    #[cfg(windows)]
    fn on_windows_the_browser_starts_through_cmd_and_node() {
        let dir = tempfile::tempdir().unwrap();
        let paths = JobPaths::in_dir(dir.path().join("j1"));
        let (command, args) = browser_server(false, &paths).unwrap();
        assert_eq!(command, "cmd");
        assert_eq!(&args[..2], ["/c", "node"]);
        assert_eq!(args[2], plain_path(&paths.qa_browser_script));
        assert_eq!(args[3], plain_path(&paths.qa_output));
    }

    #[test]
    fn reasoning_reaches_both_clis() {
        let mut task = sample_task();
        task.distro = Some("Ubuntu".into());
        let paths = JobPaths::in_dir(PathBuf::from(
            r"C:\Users\me\AppData\Roaming\tinto\delivery\jobs\j1",
        ));
        let mut codex = sample_job(DeliveryAgent::Codex);
        codex.reasoning = Some("medium".into());
        let script = codex_launch(&codex, &task, &paths, None, &SandboxPlan::default())
            .unwrap()
            .args
            .pop()
            .unwrap();
        assert!(
            script.contains(r#" -c 'model_reasoning_effort="medium"' -"#),
            "{script}"
        );
        let mut claude = sample_job(DeliveryAgent::Claude);
        claude.reasoning = Some("high".into());
        let script = claude_launch(&claude, &task, &paths, None, None)
            .unwrap()
            .args
            .pop()
            .unwrap();
        assert!(script.contains(" --effort 'high'"), "{script}");
        codex.reasoning = None;
        let script = codex_launch(&codex, &task, &paths, None, &SandboxPlan::default())
            .unwrap()
            .args
            .pop()
            .unwrap();
        assert!(!script.contains("model_reasoning_effort"), "{script}");
    }

    fn windows_task() -> DeliveryTask {
        let mut task = sample_task();
        task.repo = PathBuf::from(r"C:\work\repo");
        task.worktree = PathBuf::from(r"C:\work\repo-wt\K-1");
        task
    }

    fn qa_job_with_permissions() -> DeliveryJob {
        let mut job = sample_job(DeliveryAgent::Claude);
        job.role = QA_ROLE.into();
        job.writes = false;
        job.allowed_commands = vec!["bash C:/work/repo/.agent/qa/run.sh".into()];
        job.frozen_scripts = vec![DeliveryFrozenCopy {
            original: "C:/work/repo/.agent/qa/run.sh".into(),
            copy: r"C:\cfg\jobs\j1\frozen\1\run.sh".into(),
        }];
        job.read_dirs = vec![r"C:\Users\me\.claude\skills".into()];
        job
    }

    #[test]
    fn a_claude_job_on_a_windows_repo_runs_on_windows_when_it_can() {
        let task = windows_task();
        let dir = tempfile::tempdir().unwrap();
        let paths = JobPaths::in_dir(dir.path().join("j1"));
        let claude = PathBuf::from(r"C:\apps\claude.exe");
        let job = qa_job_with_permissions();
        let launch =
            claude_launch(&job, &task, &paths, Some(claude.clone()), Some("Ubuntu")).unwrap();
        assert_eq!(launch.program, claude);
        assert_eq!(launch.wsl_job, None);
        assert_eq!(launch.cwd, Some(PathBuf::from(r"C:\work\repo-wt\K-1")));
        // The folders a permission names follow the main checkout and the
        // job's own QA folder.
        let add_dir = launch
            .args
            .iter()
            .position(|arg| arg == "--add-dir")
            .unwrap();
        assert_eq!(launch.args[add_dir + 1], r"C:\work\repo");
        assert_eq!(launch.args[add_dir + 3], "C:/Users/me/.claude/skills");
        // Any command; Tinto's steps refused, also on the worktree by path.
        let allowed = |tool: &str| launch.args.iter().any(|arg| arg == tool);
        assert!(allowed("Bash"));
        assert!(allowed("--disallowedTools"));
        assert!(allowed("Bash(git -C C:/work/repo-wt/K-1 push:*)"));
        assert_eq!(claude_rule_path("C:/cfg/jobs/j1/qa"), "//c/cfg/jobs/j1/qa");
        assert_eq!(claude_rule_path("/home/me/qa"), "//home/me/qa");
        assert!(allowed(&format!(
            "Edit({}/**)",
            claude_rule_path(&qa_folder(&paths, false))
        )));
        assert!(!launch
            .args
            .iter()
            .any(|arg| arg.contains(".agent/qa/run.sh")));
        assert!(launch
            .stdin
            .contains("- C:/work/repo/.agent/qa/run.sh → C:/cfg/jobs/j1/frozen/1/run.sh"));
        assert!(launch
            .stdin
            .contains("The repo's checks: bash C:/cfg/jobs/j1/frozen/1/run.sh."));
        assert!(launch.stdin.contains(
            "read and list these folders outside the worktree, without changing them: C:/Users/me/.claude/skills."
        ));
    }

    #[test]
    fn the_platform_of_a_claude_job_can_be_forced() {
        let task = windows_task();
        let paths = JobPaths::in_dir(PathBuf::from(r"C:\cfg\jobs\j1"));
        let mut job = qa_job_with_permissions();
        // Not installed on Windows: never a quiet move to WSL (dispatch
        // gives the job to Codex instead).
        for platform in [None, Some(DeliveryPlatform::Windows)] {
            job.platform = platform;
            let error = claude_launch(&job, &task, &paths, None, Some("Ubuntu")).unwrap_err();
            assert_eq!(error.category, "binary_not_found");
        }
        // WSL when the job asks for it.
        job.platform = Some(DeliveryPlatform::Wsl);
        let launch = claude_launch(&job, &task, &paths, None, Some("Ubuntu")).unwrap();
        assert_eq!(launch.program, PathBuf::from("wsl.exe"));
        assert_eq!(launch.wsl_job.as_ref().unwrap().distro, "Ubuntu");
        let script = launch.args.last().unwrap();
        assert!(
            script.contains("'Bash(git -C /mnt/c/work/repo-wt/K-1 push:*)'"),
            "{script}"
        );
        assert!(
            launch
                .stdin
                .contains("bash /mnt/c/cfg/jobs/j1/frozen/1/run.sh"),
            "{}",
            launch.stdin
        );
        assert!(
            script.contains(" '/mnt/c/Users/me/.claude/skills'"),
            "{script}"
        );
    }

    #[test]
    fn codex_leaves_its_elevated_sandbox_when_it_cannot_read_a_check_program() {
        let task = windows_task();
        let paths = JobPaths::in_dir(PathBuf::from(r"C:\cfg\jobs\j1"));
        let mut job = sample_job(DeliveryAgent::Codex);
        job.allowed_commands = vec![
            "npm test".into(),
            "npm run check-lint".into(),
            "cargo test".into(),
        ];
        job.allowed_commands
            .push(r#""C:/Program Files/Git/bin/bash.exe" run.sh"#.into());
        assert_eq!(
            command_programs(&job.allowed_commands),
            ["npm", "cargo", "C:/Program Files/Git/bin/bash.exe"]
        );
        let codex = PathBuf::from(r"C:\apps\codex.exe");
        let failed = |programs: &[&str]| -> Vec<String> {
            programs.iter().map(|program| program.to_string()).collect()
        };

        // Everything starts in the configured sandbox: nothing changes.
        assert_eq!(
            sandbox_plan_from(&[], &failed(&["bash"])),
            SandboxPlan::default()
        );
        // Only the unelevated one can read npm: the job moves there.
        let plan = sandbox_plan_from(&failed(&["npm"]), &[]);
        assert!(plan.unelevated && plan.obstacles.is_empty());
        let launch = codex_launch(&job, &task, &paths, Some(codex.clone()), &plan).unwrap();
        assert_eq!(launch.program, codex);
        let end = &launch.args[launch.args.len() - 3..];
        assert_eq!(end, ["-c", r#"windows.sandbox="unelevated""#, "-"]);
        assert!(launch.note.as_deref().unwrap().contains("npm"));
        assert!(!launch.stdin.contains("Known limits"));

        // No sandbox starts both: the one that fails less, and the job is
        // told what it cannot run instead of finding out by trying.
        let plan = sandbox_plan_from(&failed(&["npm", "node"]), &failed(&["bash"]));
        assert!(plan.unelevated);
        assert_eq!(plan.obstacles.len(), 1);
        assert!(
            plan.obstacles[0].starts_with("bash solo puede ejecutarse en el sandbox configurado")
        );
        assert!(plan.obstacles[0].ends_with("no funciona npm, node"));
        let launch = codex_launch(&job, &task, &paths, Some(codex.clone()), &plan).unwrap();
        assert!(launch
            .stdin
            .contains("Known limits of your sandbox on this machine: bash solo"));
        let plan = sandbox_plan_from(&failed(&["npm", "bash"]), &failed(&["bash"]));
        assert_eq!(
            plan.obstacles,
            ["bash no puede ejecutarse en ningún sandbox de Codex en este equipo"]
        );

        let launch =
            codex_launch(&job, &task, &paths, Some(codex), &SandboxPlan::default()).unwrap();
        assert!(!launch
            .args
            .iter()
            .any(|arg| arg.contains("windows.sandbox")));
        assert_eq!(launch.note, None);
    }

    #[test]
    fn folders_a_permission_makes_writable_reach_both_agents_and_nothing_else_does() {
        let task = windows_task();
        let dir = tempfile::tempdir().unwrap();
        let paths = JobPaths::in_dir(dir.path().join("j1"));
        let qa = qa_folder(&paths, false);
        let mut job = qa_job_with_permissions();
        job.write_dirs = vec![r"C:\Users\me\.agents\skills".into()];

        // Codex: its QA folder and the named folder are the only extra
        // writable roots of the sandbox.
        let mut codex = job.clone();
        codex.agent = DeliveryAgent::Codex;
        let launch = codex_launch(
            &codex,
            &task,
            &paths,
            Some(PathBuf::from("codex.exe")),
            &SandboxPlan::default(),
        )
        .unwrap();
        let roots: Vec<&String> = launch
            .args
            .iter()
            .zip(launch.args.iter().skip(1))
            .filter(|(flag, _)| *flag == "--add-dir")
            .map(|(_, dir)| dir)
            .collect();
        assert_eq!(roots, [&qa, "C:/Users/me/.agents/skills"]);
        // A folder only readable is never a writable root.
        assert!(!launch.args.iter().any(|arg| arg.contains(".claude/skills")));
        assert!(launch
            .stdin
            .contains("write in these folders outside the worktree, and nowhere else: C:/Users/me/.agents/skills."));

        // Claude Code: it may edit there, and only there besides its QA folder.
        let launch =
            claude_launch(&job, &task, &paths, Some(PathBuf::from("claude.exe")), None).unwrap();
        let edits: Vec<&String> = launch
            .args
            .iter()
            .filter(|arg| arg.starts_with("Edit("))
            .collect();
        assert_eq!(
            edits,
            [
                &format!("Edit({}/**)", claude_rule_path(&qa)),
                "Edit(//c/Users/me/.agents/skills/**)"
            ]
        );
    }

    #[test]
    fn a_result_says_what_blocked_the_job() {
        let schema: Value = serde_json::from_str(RESULT_SCHEMA).unwrap();
        assert_eq!(
            schema["properties"]["blocked_by"]["enum"],
            serde_json::json!(["none", "environment", "sandbox"])
        );
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&Value::from("blocked_by")));
        let result = parse_result(&serde_json::json!({
            "status": "blocked", "summary": "bash did not start", "blocked_by": "sandbox"
        }))
        .unwrap();
        assert_eq!(result.blocked_by, "sandbox");
        // Results from before the field existed still read.
        let old = parse_result(&serde_json::json!({"status": "pass", "summary": "ok"})).unwrap();
        assert_eq!(old.blocked_by, "");
        let prompt = job_prompt(
            &sample_job(DeliveryAgent::Codex),
            &sample_task(),
            &JobPaths::in_dir(PathBuf::from("jobs")),
            false,
            &[],
        );
        assert!(prompt.contains("Set blocked_by"));
    }

    #[test]
    fn a_job_acted_when_it_did_more_than_read() {
        let claude = |name: &str, command: &str| {
            serde_json::json!({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "name": name, "input": {"command": command}}
            ]}})
            .to_string()
        };
        let job = qa_job_with_permissions();
        let reads = [
            claude("Read", ""),
            claude("Bash", "git status --short"),
            claude("Bash", "ls C:/Users/me/.claude/skills"),
        ]
        .join("\n");
        assert!(!acted(&job, &reads));
        assert!(!acted(&job, ""));
        let ran_copy = claude("Bash", r"bash C:\cfg\jobs\j1\frozen\1\run.sh login");
        assert!(acted(&job, &format!("{reads}\n{ran_copy}")));
        assert!(acted(
            &job,
            &claude("mcp__playwright__browser_navigate", "")
        ));

        let codex = sample_job(DeliveryAgent::Codex);
        assert!(!acted(
            &codex,
            r#"{"type":"thread.started","thread_id":"t"}"#
        ));
        assert!(acted(&codex, CODEX_OK));
        assert!(acted(&sample_job(DeliveryAgent::Shell), ""));
    }

    #[test]
    fn shell_quoting_survives_single_quotes() {
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
    }
}
