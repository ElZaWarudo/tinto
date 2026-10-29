//! Engine tests with real git repositories and real processes. A fake
//! launcher replays recorded agent output, so no agent CLI is needed.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use super::adapters::{JobLaunch, JobPaths};
use super::coordination::NewApproval;
use super::model::{
    DeliveryAccess, DeliveryAgent, DeliveryApprovalStatus, DeliveryDecisionKind,
    DeliveryDecisionOption, DeliveryJob, DeliveryJobStatus, DeliveryLeaseState,
    DeliveryRepoSettings, DeliveryResultState, DeliveryRung, DeliverySettings,
    DeliverySettingsChanges, DeliverySettingsProposalStatus, DeliveryStageAgent, DeliveryTask,
};
use super::service::{DeliveryService, Launcher, NewJob, NewPackage, NewTask};
use super::store::DeliveryStore;
use super::tasks::plain_path;
use super::tasks::test_support::{git, repo_with_commit};
use crate::agent_console::checkpoint::CheckpointConfig;

const CODEX_OK: &str = include_str!("fixtures/codex-ok.jsonl");
const CODEX_FAILED: &str = include_str!("fixtures/codex-failed.jsonl");
const CODEX_LIMIT: &str = include_str!("fixtures/codex-limit.jsonl");
const CLAUDE_OK: &str = include_str!("fixtures/claude-ok.jsonl");
/// How often a test looks again at what it waits for.
const POLL: Duration = Duration::from_millis(20);

/// Prompt directives for the fake agent: `events=codex-ok touch=a.txt
/// exit=0 sleep=0`; `env=NAME` writes that variable's value to `env.txt`.
fn fake_launcher() -> Launcher {
    Arc::new(|job: &DeliveryJob, task: &DeliveryTask, paths: &JobPaths| {
        let spec: HashMap<&str, &str> = job
            .prompt
            .split_whitespace()
            .filter_map(|part| part.split_once('='))
            .collect();
        let events = match spec.get("events").copied() {
            Some("codex-ok") => CODEX_OK,
            Some("codex-failed") => CODEX_FAILED,
            Some("codex-limit") => CODEX_LIMIT,
            Some("claude-ok") => CLAUDE_OK,
            _ => "",
        };
        std::fs::create_dir_all(&paths.dir).unwrap();
        let events_file = paths.dir.join("fake-events.jsonl");
        std::fs::write(&events_file, events).unwrap();
        let worktree = PathBuf::from(plain_path(&task.worktree));
        let sleep: u32 = spec.get("sleep").and_then(|v| v.parse().ok()).unwrap_or(0);
        let exit: i32 = spec.get("exit").and_then(|v| v.parse().ok()).unwrap_or(0);
        let touch = spec.get("touch").map(|name| worktree.join(name));
        // `record=1`: a QA job that writes its visual changes record;
        // `record=problems`: one that lists two problems; `record=guide`:
        // one with the guide's header and verdict table. `record_as=-x.md`
        // writes it as `<KEY>-x.md` instead (`.html`: a page of its own).
        if let Some(record) = spec.get("record") {
            let problems = match *record {
                "problems" => "**Problems:**\n- P1: The toast covers Retry on mobile.\n- P2: The menu is cut off.\n\n**Suggested solutions:**\n- P1: Move the toast up.\n- P2: Let the menu scroll.\n",
                "guide" => "| Test environment | |\n| --- | --- |\n| Date | 2026-10-22 |\n\n**Problems:** None\n\n## Verdicts\n\n| Item | Verdict |\n| --- | --- |\n| 1. Menú | pass |\n",
                _ => "**Problems:** None\n",
            };
            let name = format!("{}{}", task.key, spec.get("record_as").unwrap_or(&".md"));
            let text = match name.ends_with(".html") {
                true => format!("<h1>{} by the agent</h1>", task.key),
                false => format!(
                    "# {} — Visual changes\n\n## 1. Menú\n\n{problems}",
                    task.key
                ),
            };
            std::fs::create_dir_all(&paths.qa_output).unwrap();
            std::fs::write(paths.qa_output.join(name), text).unwrap();
        }
        #[cfg(target_os = "windows")]
        {
            let mut lines = vec!["@echo off".to_string()];
            lines.push(format!("type \"{}\"", events_file.display()));
            if let Some(path) = &touch {
                lines.push(format!("echo changed>\"{}\"", path.display()));
            }
            if let Some(name) = spec.get("env") {
                lines.push(format!(
                    "echo %{name}%>\"{}\"",
                    worktree.join("env.txt").display()
                ));
            }
            // `server=1`: a server it starts in the background and leaves.
            if spec.contains_key("server") {
                lines.push("start \"\" /b ping -n 300 127.0.0.1 >NUL".to_string());
            }
            if sleep > 0 {
                lines.push(format!("ping -n {} 127.0.0.1 >NUL", sleep + 1));
            }
            lines.push(format!("exit /b {exit}"));
            let script = paths.dir.join("fake.cmd");
            std::fs::write(&script, lines.join("\r\n")).unwrap();
            Ok(JobLaunch {
                program: PathBuf::from("cmd.exe"),
                args: vec!["/D".into(), "/C".into(), script.display().to_string()],
                cwd: Some(worktree),
                stdin: String::new(),
                provider_session_id: None,
                wsl_job: None,
                note: None,
                files: Vec::new(),
                env: Vec::new(),
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut lines = vec![format!("cat '{}'", events_file.display())];
            if let Some(path) = &touch {
                lines.push(format!("echo changed > '{}'", path.display()));
            }
            if let Some(name) = spec.get("env") {
                lines.push(format!(
                    "echo \"${name}\" > '{}'",
                    worktree.join("env.txt").display()
                ));
            }
            // `children=1`: commands it leaves running, one in its process
            // group and one in its own, as Codex runs them.
            if spec.contains_key("children") {
                let dir = paths.dir.display();
                lines.push(format!("sleep 300 & echo $! > '{dir}/same.pid'"));
                lines.push(format!(
                    "perl -e 'setpgrp(0, 0); exec \"sleep\", \"300\"' & echo $! > '{dir}/own.pid'"
                ));
            }
            if spec.contains_key("server") {
                lines.push("sleep 300 &".to_string());
            }
            if sleep > 0 {
                lines.push(format!("exec sleep {sleep}"));
            }
            lines.push(format!("exit {exit}"));
            let script = paths.dir.join("fake.sh");
            std::fs::write(&script, lines.join("\n")).unwrap();
            Ok(JobLaunch {
                program: PathBuf::from("sh"),
                args: vec![script.display().to_string()],
                cwd: Some(worktree),
                stdin: String::new(),
                provider_session_id: None,
                wsl_job: None,
                note: None,
                files: Vec::new(),
                env: Vec::new(),
            })
        }
    })
}

struct Harness {
    _home_lock: std::sync::RwLockReadGuard<'static, ()>,
    service: DeliveryService,
    repo: tempfile::TempDir,
    _worktrees: tempfile::TempDir,
    _jobs: tempfile::TempDir,
}

fn harness_with_store(store: DeliveryStore) -> Harness {
    let home_lock = crate::HOME_ENV_LOCK
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let repo = repo_with_commit();
    let worktrees = tempfile::tempdir().unwrap();
    let jobs = tempfile::tempdir().unwrap();
    store
        .set_repo_settings(
            repo.path(),
            &DeliveryRepoSettings {
                worktree_root: Some(worktrees.path().to_path_buf()),
                bootstrap: None,
                default_base: None,
                ..Default::default()
            },
        )
        .unwrap();
    let jobs_dir = jobs.path().to_path_buf();
    let service = DeliveryService::with_parts(
        store,
        fake_launcher(),
        Arc::new(move |id: &str| Ok(JobPaths::in_dir(jobs_dir.join(id)))),
        CheckpointConfig::default(),
    )
    .unwrap();
    Harness {
        _home_lock: home_lock,
        service,
        repo,
        _worktrees: worktrees,
        _jobs: jobs,
    }
}

fn harness() -> Harness {
    harness_with_store(DeliveryStore::open_in_memory().unwrap())
}

impl Harness {
    fn task(&self, key: &str) -> DeliveryTask {
        self.service
            .create_task(NewTask {
                repo: self.repo.path().to_path_buf(),
                distro: None,
                key: key.to_string(),
                title: format!("Task {key}"),
                read_dirs: Vec::new(),
                base: None,
                branch: None,
                run_id: None,
                package: None,
            })
            .unwrap()
    }

    fn dispatch(&self, task: &DeliveryTask, role: &str, prompt: &str) -> DeliveryJob {
        self.service
            .dispatch(self.new_job(task, role, prompt))
            .unwrap()
    }

    fn new_job(&self, task: &DeliveryTask, role: &str, prompt: &str) -> NewJob {
        NewJob {
            task_id: task.id.clone(),
            role: role.to_string(),
            agent: DeliveryAgent::Codex,
            model: None,
            access: DeliveryAccess::Workspace,
            prompt: prompt.to_string(),
            writes: None,
            lease: None,
            timeout_minutes: None,
            reasoning: None,
            platform: None,
            read_dirs: Vec::new(),
            stage_default: false,
            needs_environment: false,
        }
    }

    fn wait(&self, job_id: &str) -> DeliveryJob {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let job = self.service.job(job_id).unwrap();
            if !job.status.is_open() {
                return job;
            }
            assert!(
                Instant::now() < deadline,
                "job did not end: {:?}",
                job.status
            );
            std::thread::sleep(POLL);
        }
    }

    fn wait_status(&self, job_id: &str, status: DeliveryJobStatus) -> DeliveryJob {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let job = self.service.job(job_id).unwrap();
            if job.status == status {
                return job;
            }
            assert!(
                Instant::now() < deadline,
                "job stayed {:?}, expected {status:?}",
                job.status
            );
            std::thread::sleep(POLL);
        }
    }
}

#[test]
fn a_job_snapshots_runs_and_its_result_is_accepted_and_undoable() {
    let h = harness();
    let task = h.task("K-1");
    let worktree = PathBuf::from(plain_path(&task.worktree));
    assert_eq!(
        std::fs::read_to_string(worktree.join("README.md"))
            .unwrap()
            .trim_end(),
        "base"
    );

    let job = h.dispatch(&task, "tests", "events=codex-ok touch=made.txt");
    let job = h.wait(&job.id);

    assert_eq!(job.status, DeliveryJobStatus::Finished, "{:?}", job.error);
    assert_eq!(job.result_state, Some(DeliveryResultState::Accepted));
    assert_eq!(job.result.as_ref().unwrap().status, "pass");
    assert_eq!(
        job.provider_session_id.as_deref(),
        Some("01a116a2-cd4d-7e31-a43d-b697759df531")
    );
    assert_ne!(job.start_candidate, job.end_candidate);
    assert_eq!(job.changes.len(), 1);
    assert_eq!(job.changes[0].path, "made.txt");
    assert_eq!(job.changes[0].kind, "created");
    let log = h.service.job_log(&job.id, 0).unwrap();
    assert!(log.entries.iter().any(|entry| entry.kind == "command"));

    let undone = h.service.undo(&job.id).unwrap();
    assert!(undone.undone_at_ms.is_some());
    assert!(!worktree.join("made.txt").exists());
    assert!(h.service.undo(&job.id).is_err());
}

#[test]
fn dispatch_fills_the_codex_default_model_and_claude_allowed_commands() {
    let h = harness();
    let task = h.task("K-9");
    h.service
        .set_model_catalog(
            DeliveryAgent::Codex,
            Some(" gpt-6-astra ".into()),
            vec!["gpt-6-astra".into()],
        )
        .unwrap();
    let codex = h.dispatch(&task, "tests", "events=codex-ok");
    assert_eq!(codex.model.as_deref(), Some("gpt-6-astra"));
    assert!(codex.allowed_commands.is_empty());
    h.wait(&codex.id);

    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.checks = vec!["npm test".into()];
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    let new_job = |agent, access| NewJob {
        task_id: task.id.clone(),
        role: "review".into(),
        agent,
        model: None,
        access,
        prompt: "events=claude-ok".into(),
        writes: None,
        lease: None,
        timeout_minutes: None,
        reasoning: None,
        platform: None,
        read_dirs: Vec::new(),
        stage_default: false,
        needs_environment: false,
    };
    let claude = h
        .service
        .dispatch(new_job(DeliveryAgent::Claude, DeliveryAccess::Workspace))
        .unwrap();
    assert_eq!(claude.model, None);
    assert_eq!(claude.allowed_commands, vec!["npm test".to_string()]);
    // Once it runs, the job says the model its session reported.
    let ran = h.wait(&claude.id);
    assert_eq!(ran.session_model.as_deref(), Some("claude"));
    assert_eq!(ran.model, None, "a retry still asks for the default");
    let full = h
        .service
        .dispatch(new_job(DeliveryAgent::Claude, DeliveryAccess::Full))
        .unwrap();
    assert!(full.allowed_commands.is_empty());
    h.wait(&full.id);
}

#[test]
fn only_qa_jobs_get_the_qa_commands_and_the_browser() {
    let h = harness();
    let task = h.task("K-10");
    let settings = DeliveryRepoSettings {
        checks: vec!["npm test".into()],
        qa_commands: vec!["agentos skills install".into()],
        qa_browser: true,
        ..h.service
            .store()
            .unwrap()
            .repo_settings(h.repo.path())
            .unwrap()
    };
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    let new_job = |role: &str| NewJob {
        task_id: task.id.clone(),
        role: role.into(),
        agent: DeliveryAgent::Claude,
        model: None,
        access: DeliveryAccess::Workspace,
        prompt: "events=claude-ok".into(),
        writes: None,
        lease: None,
        timeout_minutes: None,
        reasoning: None,
        platform: None,
        read_dirs: Vec::new(),
        stage_default: false,
        needs_environment: false,
    };
    let review = h.service.dispatch(new_job("review")).unwrap();
    assert_eq!(review.allowed_commands, vec!["npm test".to_string()]);
    assert!(!review.qa_browser);
    h.wait(&review.id);
    let qa = h.service.dispatch(new_job("qa")).unwrap();
    assert_eq!(
        qa.allowed_commands,
        vec!["npm test".to_string(), "agentos skills install".to_string()]
    );
    assert!(qa.qa_browser);
    assert_eq!(qa.lease.as_deref(), Some("qa"));
    h.wait(&qa.id);
}

#[test]
fn a_repo_is_the_same_with_or_without_the_verbatim_prefix() {
    let store = DeliveryStore::open_in_memory().unwrap();
    let settings = DeliveryRepoSettings {
        checks: vec!["npm test".into()],
        ..Default::default()
    };
    store
        .set_repo_settings(std::path::Path::new(r"\\?\C:\work\repo"), &settings)
        .unwrap();
    assert_eq!(
        store
            .repo_settings(std::path::Path::new(r"C:\work\repo"))
            .unwrap(),
        settings
    );
}

#[test]
fn a_read_only_job_that_changes_files_is_invalid() {
    let h = harness();
    let task = h.task("K-2");
    let job = h.dispatch(&task, "review", "events=codex-ok touch=sneaky.txt");
    let job = h.wait(&job.id);
    assert!(!job.writes);
    assert_eq!(job.status, DeliveryJobStatus::Finished);
    assert_eq!(job.result_state, Some(DeliveryResultState::Invalid));
}

#[test]
fn a_failed_agent_reports_its_reason_and_a_refused_model_is_not_dispatched_again() {
    let h = harness();
    let task = h.task("K-3");
    let with_model = |prompt: &str| NewJob {
        model: Some("gpt-6.1-sol".into()),
        ..h.new_job(&task, "implementation", prompt)
    };
    let job = h
        .service
        .dispatch(with_model("events=codex-failed exit=1"))
        .unwrap();
    let job = h.wait(&job.id);
    assert_eq!(job.status, DeliveryJobStatus::Failed);
    // A model the account cannot use is said plainly, with the provider's words.
    assert_eq!(
        job.error.as_deref(),
        Some("El modelo de la etapa no está disponible para esta cuenta de Codex: cámbialo en Ajustes. (The 'gpt-6.1-sol' model is not supported when using Codex with a ChatGPT account.)")
    );
    assert_eq!(job.exit_code, Some(1));
    let again = eventually(|| h.service.dispatch(with_model("events=codex-ok")).err());
    assert_eq!(again.category, "model_unavailable");
    // The catalog seen anew: it may be dispatched again.
    h.service
        .set_model_catalog(DeliveryAgent::Codex, None, vec!["gpt-6.1-sol".into()])
        .unwrap();
    assert!(h.service.dispatch(with_model("events=codex-ok")).is_ok());
}

#[test]
fn a_job_its_usage_limit_stopped_runs_again_later() {
    let h = harness();
    let task = h.task("K-3B");
    let job = h.dispatch(&task, "implementation", "events=codex-limit exit=1");
    assert_eq!(h.wait(&job.id).status, DeliveryJobStatus::Failed);
    let retry = eventually(|| {
        h.service
            .overview()
            .unwrap()
            .jobs
            .into_iter()
            .find(|other| other.retry_of.as_deref() == Some(job.id.as_str()))
    });
    assert_eq!(retry.task_id, task.id);
    let stopped = h.service.job(&job.id).unwrap();
    assert!(
        stopped
            .error
            .as_deref()
            .unwrap()
            .ends_with("Se reintentó tras el límite de uso."),
        "{:?}",
        stopped.error
    );
}

#[test]
fn a_finished_process_without_a_result_fails() {
    let h = harness();
    let task = h.task("K-4");
    let job = h.wait(&h.dispatch(&task, "tests", "events=none").id);
    assert_eq!(job.status, DeliveryJobStatus::Failed);
    assert!(job.error.unwrap().contains("resultado"));
}

#[test]
fn capacity_and_one_job_per_task_are_respected() {
    let h = harness();
    h.service
        .update_settings(DeliverySettings { capacity: 1 })
        .unwrap();
    let a = h.task("K-5");
    let b = h.task("K-6");
    let first = h.dispatch(&a, "tests", "sleep=30");
    let second_same_task = h.dispatch(&a, "implementation", "events=codex-ok");
    let other_task = h.dispatch(&b, "tests", "events=codex-ok");
    h.wait_status(&first.id, DeliveryJobStatus::Started);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        h.service.job(&second_same_task.id).unwrap().status,
        DeliveryJobStatus::Queued
    );
    assert_eq!(
        h.service.job(&other_task.id).unwrap().status,
        DeliveryJobStatus::Queued
    );

    h.service.cancel(&first.id).unwrap();
    let first = h.wait(&first.id);
    assert_eq!(first.status, DeliveryJobStatus::Cancelled);
    // FIFO: the task's next job was queued before the other task's.
    let next = h.wait(&second_same_task.id);
    assert_eq!(next.status, DeliveryJobStatus::Finished);
    assert_eq!(h.wait(&other_task.id).status, DeliveryJobStatus::Finished);
}

#[test]
fn waiting_on_one_issue_never_leaves_the_rest_of_the_batch_idle() {
    let h = harness();
    h.service
        .update_settings(DeliverySettings { capacity: 3 })
        .unwrap();
    let run = h
        .service
        .create_run(h.repo.path().to_path_buf(), "B", Some("coord".into()), None)
        .unwrap();
    let task = |key: &str| {
        h.service
            .create_task(NewTask {
                repo: h.repo.path().to_path_buf(),
                distro: None,
                key: key.into(),
                title: "x".into(),
                read_dirs: Vec::new(),
                base: None,
                branch: None,
                run_id: Some(run.id.clone()),
                package: None,
            })
            .unwrap()
    };
    let busy = task("I-1");
    let idle = task("I-2");
    let asking = task("I-3");
    h.service
        .request_decisions(&asking.id, vec![choice("¿Aviso?", Some(0))], "coord")
        .unwrap();
    let slow = h.dispatch(&busy, "tests", "sleep=30");
    let wait = |extra: serde_json::Value| {
        let mut args = json!({"job_ids": [slow.id], "timeout_seconds": 1});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "wait_job", "arguments": args}}),
        )
        .unwrap()["result"]["structuredContent"]
            .clone()
    };
    // It does not wait while an issue could run: only I-2, since I-3 waits
    // on the user and I-1 runs.
    let first = wait(json!({}));
    assert_eq!(first["waited"], false, "{first}");
    let keys: Vec<&str> = first["idle"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["I-2"]);
    // Its phase says what it waits for: its first stage.
    assert_eq!(first["idle"][0]["phase"]["kind"], "next_stage");
    assert_eq!(first["idle"][0]["phase"]["role"], "tests");
    assert_eq!(first["idle"][0]["phase"]["waits_on"], "coordinator");
    assert_eq!(first["spare_capacity"], 2);
    // Told the idle issue waits on something else, it waits, still saying so.
    let waited = wait(json!({"idle_ok": true}));
    assert_eq!(waited["done"], false);
    assert!(waited.get("waited").is_none(), "{waited}");
    assert_eq!(waited["idle"].as_array().unwrap().len(), 1);
    // Once it runs, nothing is idle.
    let other = h.dispatch(&idle, "tests", "sleep=30");
    assert_eq!(wait(json!({}))["idle"], json!([]));
    for job in [&slow, &other] {
        h.service.cancel(&job.id).unwrap();
        h.wait(&job.id);
    }
}

#[test]
fn waiting_on_a_batch_returns_the_first_job_that_ends() {
    let h = harness();
    h.service
        .update_settings(DeliverySettings { capacity: 2 })
        .unwrap();
    let slow = h.dispatch(&h.task("K-50"), "tests", "sleep=30");
    let fast = h.dispatch(&h.task("K-51"), "tests", "events=codex-ok");
    let waited = super::mcp::handle_message(
        &h.service,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "wait_job", "arguments": {"job_ids": [slow.id, fast.id], "timeout_seconds": 60}}}),
    )
    .unwrap()["result"]["structuredContent"]
        .clone();
    assert_eq!(waited["done"], true, "{waited}");
    assert_eq!(waited["job"]["id"], fast.id.as_str());
    assert_eq!(waited["ended"].as_array().unwrap().len(), 1);
    assert_eq!(waited["open"], json!([slow.id]));
    h.service.cancel(&slow.id).unwrap();
    h.wait(&slow.id);
}

#[test]
fn an_agent_missing_on_this_machine_gives_way_to_the_other_never_to_wsl() {
    let h = harness();
    let task = h.task("L-1");
    let new_job = |agent, reasoning: &str| NewJob {
        task_id: task.id.clone(),
        role: "tests".into(),
        agent,
        model: Some("opus".into()),
        access: DeliveryAccess::Workspace,
        prompt: "events=codex-ok".into(),
        writes: None,
        lease: None,
        timeout_minutes: None,
        reasoning: Some(reasoning.into()),
        platform: None,
        read_dirs: Vec::new(),
        stage_default: false,
        needs_environment: false,
    };
    // Claude Code is not installed: Codex runs it, with its own model.
    h.service
        .set_installed(Arc::new(|agent| agent == DeliveryAgent::Codex));
    let swapped = h
        .service
        .dispatch(new_job(DeliveryAgent::Claude, "max"))
        .unwrap();
    assert_eq!(swapped.agent, DeliveryAgent::Codex);
    assert_ne!(swapped.model.as_deref(), Some("opus"));
    assert_eq!(swapped.reasoning, None, "Codex has no `max`");
    assert_eq!(
        swapped.substituted.as_deref(),
        Some("Claude Code no está instalado en este equipo: corrió con Codex.")
    );
    h.wait(&swapped.id);
    // And the other way round, keeping a level both know.
    h.service
        .set_installed(Arc::new(|agent| agent == DeliveryAgent::Claude));
    let other = h
        .service
        .dispatch(new_job(DeliveryAgent::Codex, "high"))
        .unwrap();
    assert_eq!(other.agent, DeliveryAgent::Claude);
    assert_eq!(other.reasoning.as_deref(), Some("high"));
    h.wait(&other.id);
    // With neither, nothing is dispatched.
    h.service.set_installed(Arc::new(|_| false));
    assert_eq!(
        h.service
            .dispatch(new_job(DeliveryAgent::Codex, "high"))
            .unwrap_err()
            .category,
        "binary_not_found"
    );
}

#[test]
fn a_qa_visual_record_becomes_a_page_the_user_opens() {
    let h = harness();
    let task = h.task("V-1");
    let qa = h.dispatch(&task, "qa", "events=codex-ok record=1");
    let qa = h.wait(&qa.id);
    let page = qa.visual_record.expect("a rendered page");
    assert!(page.ends_with("V-1.html"), "{page}");
    let html = std::fs::read_to_string(&page).unwrap();
    assert!(html.contains("<h1>V-1 — Visual changes</h1>"));
    assert_eq!(qa.visual_record_error, None);
    assert!(qa.visual_record_source.unwrap().ends_with("V-1.md"));
    // It lacks the guide's header and verdicts, and says so.
    assert_eq!(qa.visual_record_warnings.len(), 2);
    // Without a record, nothing is rendered, and the job says why.
    let plain = h.dispatch(&task, "qa", "events=codex-ok");
    let plain = h.wait(&plain.id);
    assert_eq!(plain.visual_record, None);
    assert_eq!(
        plain.visual_record_error.as_deref(),
        Some("el trabajo no escribió V-1.md en su carpeta de QA")
    );
    // One that follows the guide has nothing to warn about.
    let guided = h.dispatch(&task, "qa", "events=codex-ok record=guide");
    let guided = h.wait(&guided.id);
    assert!(guided.visual_record.is_some());
    assert_eq!(guided.visual_record_warnings, Vec::<String>::new());
    // Other jobs have no record to miss.
    let tests = h.wait(&h.dispatch(&task, "tests", "events=codex-ok").id);
    assert_eq!(tests.visual_record_error, None);
}

#[test]
fn a_qa_record_copied_after_the_job_ended_shows_when_read_or_regenerated() {
    let h = harness();
    let task = h.task("R-1");
    let qa = h.wait(&h.dispatch(&task, "qa", "events=codex-ok").id);
    assert!(qa.visual_record_error.is_some());
    let folder = (h.service.inner.paths_for)(&qa.id).unwrap().qa_output;
    std::fs::create_dir_all(&folder).unwrap();
    let write = |name: &str, secs: u64| {
        let path = folder.join(name);
        std::fs::write(
            &path,
            "# R-1 — Visual changes\n\n## 1. Menú\n\n**Problems:**\n- P1: The menu is cut off.\n",
        )
        .unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
    };
    write("R-1_notes.md", 100);
    write("R-1-matrix.md", 200);
    // The overview renders it, and only once.
    let shown = |overview: super::model::DeliveryOverview| {
        overview
            .jobs
            .into_iter()
            .find(|job| job.id == qa.id)
            .unwrap()
    };
    let read = shown(h.service.overview().unwrap());
    assert!(read.visual_record.unwrap().ends_with("R-1-matrix.html"));
    assert_eq!(read.visual_record_error, None);
    assert_eq!(read.visual_record_extras, ["R-1_notes.md"]);
    assert_eq!(h.service.task_qa_problems(&task.id).unwrap().len(), 1);
    shown(h.service.overview().unwrap());

    // `<KEY>.md` copied in later: read_job finds it.
    std::fs::copy(folder.join("R-1-matrix.md"), folder.join("R-1.md")).unwrap();
    let read = h.service.job(&qa.id).unwrap();
    assert!(read
        .visual_record
        .as_deref()
        .unwrap()
        .ends_with("R-1-matrix.html"));
    let mut cleared = read.clone();
    cleared.visual_record = None;
    cleared.visual_record_source = None;
    h.service.store().unwrap().put_job(&cleared).unwrap();
    let read = h.service.read_job(&qa.id).unwrap();
    assert!(read.visual_record.unwrap().ends_with("R-1.html"));
    assert_eq!(read.visual_record_extras, ["R-1-matrix.md", "R-1_notes.md"]);

    // "Regenerar registro" renders it again; its problems are not repeated.
    let again = h.service.regenerate_visual_record(&qa.id).unwrap();
    assert!(again.visual_record.unwrap().ends_with("R-1.html"));
    assert_eq!(h.service.task_qa_problems(&task.id).unwrap().len(), 1);
    let tests = h.wait(&h.dispatch(&task, "tests", "events=codex-ok").id);
    assert!(h.service.regenerate_visual_record(&tests.id).is_err());
}

#[test]
fn a_running_job_shows_what_it_does_and_what_it_used() {
    let h = harness();
    let task = h.task("O-1");
    let job = h.dispatch(&task, "tests", "events=codex-ok");
    let job = h.wait(&job.id);
    assert!(job.activity_at_ms.is_some());
    assert!(job.activity.is_some());
    assert_eq!(
        job.usage,
        Some(super::model::DeliveryUsage {
            input_tokens: 42723 - 21120,
            cached_tokens: 21120,
            output_tokens: 226,
            cost_micro_usd: None,
        })
    );
    // Once it ends, nothing runs and it says where its time went.
    assert_eq!(job.current_command, None);
    let timing = job.timing.expect("timing");
    assert_eq!(
        timing.slowest[0].command,
        "Set-Content greeting.txt 'hello from a delivery job.'"
    );
    assert_eq!(timing.slowest[0].runs, 1);
    assert_eq!(timing.model_ms, None);
}

#[test]
fn a_paused_stage_holds_its_task_and_resource_until_it_resumes_in_its_session() {
    let h = harness();
    let task = h.task("P-1");
    let running = h.dispatch(&task, "qa", "events=codex-ok sleep=30");
    h.wait_status(&running.id, DeliveryJobStatus::Started);
    // Its session is known once the agent starts it.
    let deadline = Instant::now() + Duration::from_secs(30);
    while h
        .service
        .job(&running.id)
        .unwrap()
        .provider_session_id
        .is_none()
    {
        assert!(Instant::now() < deadline, "no session recorded");
        std::thread::sleep(POLL);
    }
    h.service.pause(&running.id).unwrap();
    let paused = h.wait_status(&running.id, DeliveryJobStatus::Paused);
    assert_eq!(paused.error, None);
    // It keeps QA and its task: nothing else starts there.
    let lease = qa_lease(&h);
    assert_eq!(lease.state, DeliveryLeaseState::Active);
    assert_eq!(lease.holder_job_id.as_deref(), Some(paused.id.as_str()));
    let refused = h
        .service
        .dispatch(NewJob {
            task_id: task.id.clone(),
            role: "review".into(),
            agent: DeliveryAgent::Codex,
            model: None,
            access: DeliveryAccess::Workspace,
            prompt: "events=codex-ok".into(),
            writes: None,
            lease: None,
            timeout_minutes: None,
            reasoning: None,
            platform: None,
            read_dirs: Vec::new(),
            stage_default: false,
            needs_environment: false,
        })
        .unwrap_err();
    assert_eq!(refused.category, "job_paused");

    // Resuming continues the same session and takes QA over.
    let resumed = h.service.resume(&paused.id).unwrap();
    assert_eq!(resumed.resumes.as_deref(), Some(paused.id.as_str()));
    assert_eq!(resumed.provider_session_id, paused.provider_session_id);
    h.wait_status(&resumed.id, DeliveryJobStatus::Started);
    assert_eq!(
        qa_lease(&h).holder_job_id.as_deref(),
        Some(resumed.id.as_str())
    );
    assert_eq!(
        h.service.resume(&paused.id).unwrap_err().category,
        "job_not_paused"
    );
    h.service.cancel(&resumed.id).unwrap();
    h.wait(&resumed.id);
}

#[test]
fn a_paused_stage_can_be_given_up_and_a_paused_batch_starts_nothing() {
    let h = harness();
    let run = h
        .service
        .create_run(h.repo.path().to_path_buf(), "B", None, None)
        .unwrap();
    let task = h
        .service
        .create_task(NewTask {
            repo: h.repo.path().to_path_buf(),
            distro: None,
            key: "P-2".into(),
            title: "x".into(),
            read_dirs: Vec::new(),
            base: None,
            branch: None,
            run_id: Some(run.id.clone()),
            package: None,
        })
        .unwrap();
    let running = h.dispatch(&task, "tests", "events=codex-ok sleep=30");
    h.wait_status(&running.id, DeliveryJobStatus::Started);
    h.service.pause(&running.id).unwrap();
    let paused = h.wait_status(&running.id, DeliveryJobStatus::Paused);
    let given_up = h.service.cancel(&paused.id).unwrap();
    assert_eq!(given_up.status, DeliveryJobStatus::Cancelled);

    h.service.set_run_paused(&run.id, true).unwrap();
    let waiting = h.dispatch(&task, "tests", "events=codex-ok");
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        h.service.job(&waiting.id).unwrap().status,
        DeliveryJobStatus::Queued
    );
    h.service.set_run_paused(&run.id, false).unwrap();
    assert_eq!(h.wait(&waiting.id).status, DeliveryJobStatus::Finished);
}

#[test]
fn a_batch_closed_to_new_tasks_refuses_them_until_the_user_allows_them() {
    let h = harness();
    let run = h
        .service
        .create_run(h.repo.path().to_path_buf(), "B", None, None)
        .unwrap();
    let new_task = |key: &str| NewTask {
        repo: h.repo.path().to_path_buf(),
        distro: None,
        key: key.into(),
        title: "x".into(),
        read_dirs: Vec::new(),
        base: None,
        branch: None,
        run_id: Some(run.id.clone()),
        package: None,
    };
    h.service.create_task(new_task("N-1")).unwrap();

    assert!(
        h.service
            .set_run_no_new_tasks(&run.id, true)
            .unwrap()
            .no_new_tasks
    );
    let refused = h.service.create_task(new_task("N-2")).unwrap_err();
    assert_eq!(refused.category, "run_no_new_tasks");
    let events = h.service.store().unwrap().events_after(0).unwrap();
    assert!(events
        .iter()
        .any(|event| event.kind == "run_policy" && event.detail.ends_with("no_new_tasks")));

    h.service.set_run_no_new_tasks(&run.id, false).unwrap();
    h.service.create_task(new_task("N-2")).unwrap();
}

#[test]
fn the_qa_resource_is_exclusive_fifo_and_quarantined_after_a_crash() {
    let h = harness();
    let a = h.task("K-7");
    let b = h.task("K-8");
    let crashing = h.dispatch(&a, "qa", "events=none exit=3");
    let waiting = h.dispatch(&b, "qa", "events=codex-ok");
    assert_eq!(crashing.lease.as_deref(), Some("qa"));

    let crashing = h.wait(&crashing.id);
    assert_eq!(crashing.status, DeliveryJobStatus::Failed);
    std::thread::sleep(Duration::from_millis(300));
    let lease = h
        .service
        .overview()
        .unwrap()
        .leases
        .into_iter()
        .find(|lease| lease.name == "qa")
        .unwrap();
    assert_eq!(lease.state, DeliveryLeaseState::Quarantined);
    assert_eq!(
        h.service.job(&waiting.id).unwrap().status,
        DeliveryJobStatus::Queued
    );
    assert_eq!(lease.queue.len(), 1);

    let released = h
        .service
        .release_lease("qa", "browser closed, data restored", "el usuario")
        .unwrap();
    assert_eq!(released.state, DeliveryLeaseState::Free);
    let waiting = h.wait(&waiting.id);
    assert_eq!(waiting.status, DeliveryJobStatus::Finished);
    let lease = h.service.overview().unwrap().leases.remove(0);
    assert_eq!(lease.state, DeliveryLeaseState::Free);
}

#[test]
fn results_from_superseded_attempts_or_older_contracts_are_stale() {
    let h = harness();
    let task = h.task("K-9");
    let first = h.dispatch(&task, "tests", "events=codex-ok sleep=2");
    let second = h.dispatch(&task, "tests", "events=codex-ok");
    assert_eq!(second.attempt, 2);
    let first = h.wait(&first.id);
    assert_eq!(first.result_state, Some(DeliveryResultState::Stale));
    assert_eq!(
        h.wait(&second.id).result_state,
        Some(DeliveryResultState::Accepted)
    );

    let review = h.dispatch(&task, "review", "events=codex-ok sleep=2");
    h.wait_status(&review.id, DeliveryJobStatus::Started);
    h.service
        .update_task(&task.id, None, Some(2), None, true)
        .unwrap();
    let review = h.wait(&review.id);
    assert_eq!(review.result_state, Some(DeliveryResultState::Stale));
    assert!(h
        .service
        .update_task(&task.id, None, Some(1), None, true)
        .is_err());
}

/// Tinto closes and leaves its jobs running, as an update does.
fn close_tinto(service: &DeliveryService) {
    service.close();
    // Its followers notice on their next look.
    std::thread::sleep(Duration::from_millis(600));
}

/// Kills a job's process while Tinto is closed, as a reboot would.
fn kill_job(service: &DeliveryService, job_id: &str) {
    let job = service.job(job_id).unwrap();
    let pid = job.pid.unwrap();
    let _ = crate::agent_console::pty::kill_process_tree(pid);
    wait_gone(&job);
}

fn wait_gone(job: &DeliveryJob) {
    let (pid, started) = (job.pid.unwrap(), job.process_started.as_deref().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    while super::job_process::alive(pid, started) {
        assert!(Instant::now() < deadline, "the job's process kept running");
        std::thread::sleep(POLL);
    }
}

/// Tinto starting again on the same state and job folders.
fn reopen(path: &std::path::Path, h: &Harness) -> DeliveryService {
    let jobs_dir = h._jobs.path().to_path_buf();
    DeliveryService::with_parts(
        DeliveryStore::open(path).unwrap(),
        fake_launcher(),
        Arc::new(move |id: &str| Ok(JobPaths::in_dir(jobs_dir.join(id)))),
        CheckpointConfig::default(),
    )
    .unwrap()
}

fn wait_on(service: &DeliveryService, job_id: &str) -> DeliveryJob {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let job = service.job(job_id).unwrap();
        if !job.status.is_open() {
            return job;
        }
        assert!(
            Instant::now() < deadline,
            "job did not end: {:?}",
            job.status
        );
        std::thread::sleep(POLL);
    }
}

#[test]
fn a_job_still_running_after_a_restart_is_adopted_and_ends_the_normal_way() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.sqlite");
    let h = harness_with_store(DeliveryStore::open(&path).unwrap());
    let task = h.task("K-20");
    let job = h.dispatch(&task, "tests", "events=codex-ok touch=a.txt sleep=4");
    let started = h.wait_status(&job.id, DeliveryJobStatus::Started);
    assert!(started.process_started.is_some());
    close_tinto(&h.service);

    let restarted = reopen(&path, &h);
    let adopted = restarted.job(&job.id).unwrap();
    assert_eq!(adopted.status, DeliveryJobStatus::Started);
    assert!(adopted.error.is_none());
    restarted.follow_running();
    let done = wait_on(&restarted, &job.id);
    assert_eq!(done.status, DeliveryJobStatus::Finished, "{:?}", done.error);
    assert_eq!(done.result_state, Some(DeliveryResultState::Accepted));
    // What it wrote while Tinto was closed is read too.
    assert!(done.usage.is_some());
    assert_eq!(
        done.provider_session_id.as_deref(),
        Some("01a116a2-cd4d-7e31-a43d-b697759df531")
    );
    assert!(
        done.changes.iter().any(|change| change.path == "a.txt"),
        "{:?}",
        done.changes
    );
}

#[test]
fn a_job_whose_agent_delivered_while_tinto_was_closed_ends_with_its_result() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.sqlite");
    let h = harness_with_store(DeliveryStore::open(&path).unwrap());
    let task = h.task("K-21");
    let job = h.dispatch(&task, "tests", "events=codex-ok sleep=1");
    h.wait_status(&job.id, DeliveryJobStatus::Started);
    close_tinto(&h.service);
    wait_gone(&h.service.job(&job.id).unwrap());

    let restarted = reopen(&path, &h);
    restarted.follow_running();
    let done = wait_on(&restarted, &job.id);
    assert_eq!(done.status, DeliveryJobStatus::Finished, "{:?}", done.error);
    assert_eq!(done.result_state, Some(DeliveryResultState::Accepted));
}

#[test]
fn a_check_adopted_after_a_restart_ends_with_the_code_it_left_or_runs_again() {
    for left in [Some("1"), None] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("delivery.sqlite");
        let h = harness_with_store(DeliveryStore::open(&path).unwrap());
        let task = h.task("K-23");
        let mut check = h.new_job(&task, "checks", "sleep=4 exit=0");
        check.agent = DeliveryAgent::Shell;
        let job = h.service.dispatch(check).unwrap();
        h.wait_status(&job.id, DeliveryJobStatus::Started);
        close_tinto(&h.service);
        // What the command's launch leaves in its folder as it ends; the
        // fake launcher's command does not.
        if let Some(code) = left {
            let file = h
                ._jobs
                .path()
                .join(&job.id)
                .join(super::adapters::EXIT_FILE);
            std::fs::write(file, code).unwrap();
        }

        let restarted = reopen(&path, &h);
        restarted.follow_running();
        let done = wait_on(&restarted, &job.id);
        let rounds = restarted.store().unwrap().task_rounds(&task.id).unwrap();
        match left {
            Some(_) => {
                assert_eq!(done.status, DeliveryJobStatus::Failed);
                assert_eq!(done.exit_code, Some(1));
                assert_eq!(rounds.len(), 1, "a failing check opens its round");
            }
            // Its end is unknown: not a failure, it runs again.
            None => {
                assert_eq!(
                    done.status,
                    DeliveryJobStatus::Interrupted,
                    "{:?}",
                    done.error
                );
                assert!(rounds.is_empty(), "{rounds:?}");
                let jobs = restarted.overview().unwrap().jobs;
                let retry = jobs
                    .iter()
                    .find(|again| again.retry_of.as_deref() == Some(job.id.as_str()))
                    .expect("queued again");
                assert_eq!(
                    wait_on(&restarted, &retry.id).status,
                    DeliveryJobStatus::Finished
                );
            }
        }
    }
}

#[test]
fn an_adopted_job_can_be_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.sqlite");
    let h = harness_with_store(DeliveryStore::open(&path).unwrap());
    let task = h.task("K-22");
    let job = h.dispatch(&task, "tests", "sleep=60");
    h.wait_status(&job.id, DeliveryJobStatus::Started);
    close_tinto(&h.service);

    let restarted = reopen(&path, &h);
    restarted.follow_running();
    restarted.cancel(&job.id).unwrap();
    let cancelled = wait_on(&restarted, &job.id);
    assert_eq!(cancelled.status, DeliveryJobStatus::Cancelled);
    wait_gone(&cancelled);
}

#[test]
fn a_job_that_died_while_tinto_was_closed_is_interrupted_and_quarantines_its_resource() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.sqlite");
    let h = harness_with_store(DeliveryStore::open(&path).unwrap());
    let task = h.task("K-10");
    let job = h.dispatch(&task, "qa", "sleep=30");
    h.wait_status(&job.id, DeliveryJobStatus::Started);
    close_tinto(&h.service);
    kill_job(&h.service, &job.id);

    let restarted = reopen(&path, &h);
    restarted.follow_running();
    let interrupted = restarted.job(&job.id).unwrap();
    assert_eq!(interrupted.status, DeliveryJobStatus::Interrupted);
    let lease = restarted.overview().unwrap().leases.remove(0);
    assert_eq!(lease.state, DeliveryLeaseState::Quarantined);
}

#[test]
fn retry_on_restart_queues_again_once_the_jobs_safe_to_repeat_and_tells_the_coordinator() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.sqlite");
    let h = harness_with_store(DeliveryStore::open(&path).unwrap());
    let store = h.service.store().unwrap();
    let settings = DeliveryRepoSettings {
        retry_on_restart: true,
        ..store.repo_settings(h.repo.path()).unwrap()
    };
    store.set_repo_settings(h.repo.path(), &settings).unwrap();
    drop(store);
    h.service
        .update_settings(DeliverySettings { capacity: 3 })
        .unwrap();
    let run = h
        .service
        .create_run(h.repo.path().to_path_buf(), "B", Some("coord".into()), None)
        .unwrap();
    let task = |key: &str| {
        h.service
            .create_task(NewTask {
                repo: h.repo.path().to_path_buf(),
                distro: None,
                key: key.into(),
                title: "x".into(),
                read_dirs: Vec::new(),
                base: None,
                branch: None,
                run_id: Some(run.id.clone()),
                package: None,
            })
            .unwrap()
    };
    let (coding, testing) = (task("R-1"), task("R-2"));
    let implementation = h.dispatch(&coding, "implementation", "sleep=30");
    // QA holds the `qa` resource, which a restart leaves in quarantine.
    let qa = h.dispatch(&testing, "qa", "sleep=30");
    h.wait_status(&implementation.id, DeliveryJobStatus::Started);
    h.wait_status(&qa.id, DeliveryJobStatus::Started);
    let cursor = h.service.store().unwrap().last_event_seq().unwrap();
    close_tinto(&h.service);
    kill_job(&h.service, &implementation.id);
    kill_job(&h.service, &qa.id);

    let jobs = tempfile::tempdir().unwrap();
    let jobs_dir = jobs.path().to_path_buf();
    let restart = || {
        let jobs_dir = jobs_dir.clone();
        DeliveryService::with_parts(
            DeliveryStore::open(&path).unwrap(),
            fake_launcher(),
            Arc::new(move |id: &str| Ok(JobPaths::in_dir(jobs_dir.join(id)))),
            CheckpointConfig::default(),
        )
        .unwrap()
    };
    let restarted = restart();
    restarted.retry_interrupted();
    let interrupted = restarted.job(&implementation.id).unwrap();
    assert_eq!(interrupted.status, DeliveryJobStatus::Interrupted);
    assert!(
        interrupted
            .error
            .as_deref()
            .unwrap()
            .ends_with("Se reintentó al reiniciar Tinto."),
        "{:?}",
        interrupted.error
    );
    let jobs_now = restarted.overview().unwrap().jobs;
    let retry = jobs_now
        .iter()
        .find(|job| job.retry_of.as_deref() == Some(implementation.id.as_str()))
        .expect("the implementation is queued again");
    assert_eq!(retry.task_id, coding.id);
    assert_eq!(retry.prompt, implementation.prompt);
    assert_eq!(retry.attempt, 2);
    let skipped = restarted.job(&qa.id).unwrap();
    assert_eq!(skipped.status, DeliveryJobStatus::Interrupted);
    assert!(
        skipped
            .error
            .as_deref()
            .unwrap()
            .contains("No se reintenta")
            && skipped.error.as_deref().unwrap().contains("cuarentena"),
        "{:?}",
        skipped.error
    );
    assert!(!jobs_now
        .iter()
        .any(|job| job.task_id == testing.id && job.id != qa.id));

    let waited = restarted
        .wait_run(
            std::slice::from_ref(&run.id),
            Some(cursor),
            Duration::from_secs(1),
            true,
        )
        .unwrap();
    let texts: Vec<&str> = waited["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["text"].as_str())
        .collect();
    assert!(
        texts
            .iter()
            .any(|text| text.starts_with("R-1: Tinto reintentó la etapa implementation")),
        "{texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text.starts_with("R-2: la etapa qa se interrumpió")
                && text.contains("no se reintenta")),
        "{texts:?}"
    );

    // The retry itself is cut off by another restart: not repeated again.
    let deadline = Instant::now() + Duration::from_secs(60);
    while restarted.job(&retry.id).unwrap().status != DeliveryJobStatus::Started {
        assert!(Instant::now() < deadline, "the retry did not start");
        std::thread::sleep(POLL);
    }
    close_tinto(&restarted);
    kill_job(&restarted, &retry.id);
    let again = restart();
    again.retry_interrupted();
    let cut = again.job(&retry.id).unwrap();
    assert_eq!(cut.status, DeliveryJobStatus::Interrupted);
    assert!(cut
        .error
        .as_deref()
        .unwrap()
        .contains("ya era el reintento"));
    assert!(!again
        .overview()
        .unwrap()
        .jobs
        .iter()
        .any(|job| job.retry_of.as_deref() == Some(retry.id.as_str())));

    for service in [&h.service, &restarted] {
        for job in service.overview().unwrap().jobs {
            let _ = service.cancel(&job.id);
        }
    }
}

#[test]
fn without_retry_on_restart_an_interrupted_agent_job_stays_but_checks_run_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.sqlite");
    let h = harness_with_store(DeliveryStore::open(&path).unwrap());
    let task = h.task("R-3");
    let job = h.dispatch(&task, "implementation", "sleep=30");
    h.wait_status(&job.id, DeliveryJobStatus::Started);
    // A check cut off with it: commands only, so it runs again regardless.
    let mut checks = super::adapters::tests::sample_job(DeliveryAgent::Shell);
    checks.id = "checks-1".into();
    checks.task_id = task.id.clone();
    checks.role = "checks".into();
    checks.prompt = "exit=0".into();
    checks.status = DeliveryJobStatus::Started;
    h.service.store().unwrap().put_job(&checks).unwrap();
    close_tinto(&h.service);
    kill_job(&h.service, &job.id);

    let restarted = DeliveryService::with_parts(
        DeliveryStore::open(&path).unwrap(),
        fake_launcher(),
        Arc::new(|id: &str| Ok(JobPaths::in_dir(std::env::temp_dir().join(id)))),
        CheckpointConfig::default(),
    )
    .unwrap();
    restarted.retry_interrupted();
    let jobs = restarted.overview().unwrap().jobs;
    assert_eq!(
        restarted.job(&job.id).unwrap().error.as_deref(),
        Some("Tinto se cerró mientras el trabajo corría.")
    );
    assert!(!jobs
        .iter()
        .any(|again| again.retry_of.as_deref() == Some(job.id.as_str())));
    assert!(jobs
        .iter()
        .any(|again| again.retry_of.as_deref() == Some("checks-1")));
    let _ = h.service.cancel(&job.id);
}

#[test]
fn removing_a_task_protects_unsaved_work() {
    let h = harness();
    let task = h.task("K-11");
    let worktree = PathBuf::from(plain_path(&task.worktree));
    std::fs::write(worktree.join("draft.txt"), "wip").unwrap();
    let error = h.service.remove_task(&task.id, false).unwrap_err();
    assert_eq!(error.category, "task_unsaved");
    assert!(worktree.exists());
    h.service.remove_task(&task.id, true).unwrap();
    assert!(!worktree.exists());
    let branch = format!("refs/heads/{}", task.branch);
    assert!(
        super::tasks::git(
            super::tasks::Place::Native,
            h.repo.path(),
            &["rev-parse", "--verify", "--quiet", &branch]
        )
        .is_err(),
        "the local branch goes with the worktree"
    );
    assert!(h.service.overview().unwrap().tasks.is_empty());
    // The same key can be started again once the task is gone.
    h.task("K-11");
}

#[test]
fn a_task_whose_removal_failed_halfway_can_be_removed() {
    let h = harness();
    let task = h.task("K-7");
    let folder = PathBuf::from(plain_path(&task.worktree));
    git(
        h.repo.path(),
        &["worktree", "remove", "--force", &plain_path(&folder)],
    );
    std::fs::create_dir(&folder).unwrap();
    h.service.remove_task(&task.id, false).unwrap();
    assert!(!folder.exists());
}

#[test]
fn a_task_whose_worktree_was_deleted_by_hand_can_be_removed() {
    let h = harness();
    let task = h.task("K-8");
    let folder = PathBuf::from(plain_path(&task.worktree));
    std::fs::remove_dir_all(&folder).unwrap();
    h.service.remove_task(&task.id, false).unwrap();
    assert!(h.service.overview().unwrap().tasks.is_empty());
    let listed = std::process::Command::new("git")
        .arg("-C")
        .arg(h.repo.path())
        .args(["worktree", "list"])
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&listed.stdout).contains("K-8"));
}

#[test]
fn a_task_key_is_unique_per_repository() {
    let h = harness();
    h.task("K-12");
    let error = h
        .service
        .create_task(NewTask {
            repo: h.repo.path().to_path_buf(),
            distro: None,
            key: "k-12".to_string(),
            title: "again".to_string(),
            read_dirs: Vec::new(),
            base: None,
            branch: Some("other".to_string()),
            run_id: None,
            package: None,
        })
        .unwrap_err();
    assert_eq!(error.category, "task_exists");
}

#[test]
fn approvals_climb_one_rung_at_a_time_and_tinto_runs_commit_and_push() {
    let h = harness();
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare"]);
    git(
        h.repo.path(),
        &["remote", "add", "origin", &plain_path(remote.path())],
    );
    let task = h.task("K-13");
    let worktree = PathBuf::from(plain_path(&task.worktree));
    let request = |rung, title: &str| NewApproval {
        task_id: task.id.clone(),
        rung,
        title: title.to_string(),
        body: String::new(),
        requested_by: "user".to_string(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: None,
        stage: None,
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    };

    assert_eq!(
        h.service
            .request_approval(request(DeliveryRung::Commit, "Add feature"))
            .unwrap_err()
            .category,
        "nothing_to_commit"
    );
    assert_eq!(
        h.service
            .request_approval(request(DeliveryRung::Pr, "PR"))
            .unwrap_err()
            .category,
        "not_pushed"
    );
    std::fs::write(worktree.join("feature.txt"), "x").unwrap();
    let commit = h
        .service
        .request_approval(request(DeliveryRung::Commit, "Add feature"))
        .unwrap();
    assert!(h
        .service
        .request_approval(request(DeliveryRung::Commit, "again"))
        .is_err());
    let commit = h.service.decide_approval(&commit.id, true, None).unwrap();
    assert_eq!(
        commit.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        commit.outcome
    );
    assert!(h.service.decide_approval(&commit.id, true, None).is_err());

    let push = h
        .service
        .request_approval(request(DeliveryRung::Push, "Publish"))
        .unwrap();
    let push = h.service.decide_approval(&push.id, true, None).unwrap();
    assert_eq!(
        push.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        push.outcome
    );

    let pr = h
        .service
        .request_approval(request(DeliveryRung::Pr, "Add feature"))
        .unwrap();
    let pr = h.service.decide_approval(&pr.id, true, None).unwrap();
    // Tinto does not open PRs; the coordinator reports back.
    assert_eq!(pr.status, DeliveryApprovalStatus::Approved);
    let pr = h
        .service
        .complete_approval(&pr.id, true, "https://github.com/o/r/pull/1")
        .unwrap();
    assert_eq!(pr.status, DeliveryApprovalStatus::Executed);

    let jira = h
        .service
        .request_approval(request(DeliveryRung::Jira, "Move K-13 to review"))
        .unwrap();
    let jira = h
        .service
        .decide_approval(&jira.id, false, Some("not yet".into()))
        .unwrap();
    assert_eq!(jira.status, DeliveryApprovalStatus::Rejected);
}

#[test]
fn a_replaced_coordinator_is_fenced() {
    let h = harness();
    let run = h
        .service
        .create_run(
            h.repo.path().to_path_buf(),
            "Batch",
            Some("coord-a".into()),
            None,
        )
        .unwrap();
    assert_eq!(run.generation, 1);
    assert_eq!(
        h.service
            .acquire_run(&run.id, "coord-b")
            .unwrap_err()
            .category,
        "run_locked"
    );
    assert!(h
        .service
        .takeover_run(&run.id, "coord-b", Some(5), "x")
        .is_err());
    let taken = h
        .service
        .takeover_run(
            &run.id,
            "coord-b",
            Some(1),
            "the user confirmed coord-a stopped",
        )
        .unwrap();
    assert_eq!(taken.generation, 2);
    assert_eq!(
        h.service
            .verify_run(&run.id, "coord-a", 1)
            .unwrap_err()
            .category,
        "fenced"
    );
    h.service.verify_run(&run.id, "coord-b", 2).unwrap();
    let closed = h.service.close_run(&run.id).unwrap();
    assert_eq!(
        h.service
            .verify_run(&closed.id, "coord-b", 2)
            .unwrap_err()
            .category,
        "run_closed"
    );
}

#[test]
fn the_coordinator_api_speaks_mcp_and_fences_writes() {
    let h = harness();
    let call = |id: u64, method: &str, params: serde_json::Value| {
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        )
        .unwrap()
    };
    let init = call(1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(super::mcp::handle_message(
        &h.service,
        &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
    )
    .is_none());
    let tools = call(2, "tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"dispatch_job") && names.contains(&"wait_job"));

    let repo = plain_path(h.repo.path());
    let run = call(
        3,
        "tools/call",
        json!({"name": "create_run", "arguments": {"repo": repo, "title": "Batch", "owner": "coord"}}),
    );
    let run_id = run["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let stale = call(
        4,
        "tools/call",
        json!({"name": "create_task", "arguments": {"run_id": run_id, "owner": "coord", "generation": 9, "repo": repo, "key": "M-1", "title": "x"}}),
    );
    assert_eq!(stale["result"]["isError"], true);
    let task = call(
        5,
        "tools/call",
        json!({"name": "create_task", "arguments": {"run_id": run_id, "owner": "coord", "generation": 1, "repo": repo, "key": "M-1", "title": "x"}}),
    );
    let task_id = task["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let job = call(
        6,
        "tools/call",
        json!({"name": "dispatch_job", "arguments": {"run_id": run_id, "owner": "coord", "generation": 1, "task_id": task_id, "role": "tests", "agent": "codex", "prompt": "events=codex-ok"}}),
    );
    let job_id = job["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(job["result"]["structuredContent"]["access"], "workspace");
    let waited = call(
        7,
        "tools/call",
        json!({"name": "wait_job", "arguments": {"job_id": job_id, "timeout_seconds": 60}}),
    );
    assert_eq!(waited["result"]["structuredContent"]["done"], true);
    assert_eq!(
        waited["result"]["structuredContent"]["job"]["result_state"], "accepted",
        "{waited}"
    );
    // Without an agent, the stage runs with what the user fixed for it.
    let review = json!({"name": "dispatch_job", "arguments": {"run_id": run_id, "owner": "coord", "generation": 1, "task_id": task_id, "role": "review", "prompt": "events=codex-ok"}});
    let unset = call(9, "tools/call", review.clone());
    assert_eq!(unset["result"]["isError"], true, "{unset}");
    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.stages.insert(
        "review".into(),
        DeliveryStageAgent {
            agent: DeliveryAgent::Codex,
            model: Some("gpt-6-sol".into()),
            reasoning: Some("high".into()),
            timeout_minutes: None,
        },
    );
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    let fixed = call(10, "tools/call", review.clone())["result"]["structuredContent"].clone();
    assert_eq!(fixed["agent"], "codex", "{fixed}");
    assert_eq!(fixed["model"], "gpt-6-sol");
    assert_eq!(fixed["reasoning"], "high");
    h.wait(fixed["id"].as_str().unwrap());
    // A retry follows the stage as the user left it after a model failed,
    // even when the attempt it repeats ran with a model the coordinator chose.
    let set_stage_model = |model: &str| {
        let mut settings = settings.clone();
        settings.stages.get_mut("review").unwrap().model = Some(model.into());
        h.service
            .store()
            .unwrap()
            .set_repo_settings(h.repo.path(), &settings)
            .unwrap();
    };
    let retry = |id: u64, job: &serde_json::Value| {
        call(
            id,
            "tools/call",
            json!({"name": "retry_job", "arguments": {"run_id": run_id, "owner": "coord", "generation": 1, "job_id": job["id"]}}),
        )["result"]["structuredContent"]
            .clone()
    };
    set_stage_model("gpt-6-pro");
    let retried = retry(11, &fixed);
    assert_eq!(retried["model"], "gpt-6-pro", "{retried}");
    assert_eq!(retried["stage_default"], true);
    h.wait(retried["id"].as_str().unwrap());
    let mut chosen = review.clone();
    chosen["arguments"]["model"] = json!("gpt-5.5");
    let chosen = call(12, "tools/call", chosen)["result"]["structuredContent"].clone();
    h.wait(chosen["id"].as_str().unwrap());
    set_stage_model("gpt-6-mini");
    let retried = retry(13, &chosen);
    assert_eq!(retried["model"], "gpt-6-mini", "{retried}");
    assert_eq!(retried["stage_default"], true);
    h.wait(retried["id"].as_str().unwrap());
    // Folders to read must be inside the main checkout or the worktree.
    let mut outside = review.clone();
    outside["arguments"]["read_dirs"] = json!([plain_path(&std::env::temp_dir())]);
    let refused = call(14, "tools/call", outside);
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    assert!(refused.to_string().contains("read_dirs"), "{refused}");
    let unknown = call(8, "nope", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);
}

#[test]
fn bootstrap_runs_as_a_shell_job_in_new_worktrees() {
    let h = harness();
    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    let install = if cfg!(target_os = "windows") {
        "echo ready>bootstrapped.txt"
    } else {
        "echo ready > bootstrapped.txt"
    };
    settings.bootstrap = Some(format!("echo started && {install}"));
    settings.setup = vec![install.to_string(), "touch=deps.txt".to_string()];
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    let task = h.task("K-14");
    let bootstrap = h
        .service
        .overview()
        .unwrap()
        .jobs
        .into_iter()
        .find(|job| job.task_id == task.id)
        .unwrap();
    assert_eq!(bootstrap.agent, DeliveryAgent::Shell);
    assert_eq!(bootstrap.role, "bootstrap");
    assert_eq!(h.wait(&bootstrap.id).status, DeliveryJobStatus::Finished);
    // A setup command the bootstrap already ran is not run again.
    let task = h
        .service
        .set_task_setup(&task.id, vec![install.into(), "touch=deps.txt".into()])
        .unwrap();
    assert_eq!(task.setup, ["touch=deps.txt"]);
}

#[test]
fn a_job_reads_existing_folders_of_the_main_checkout_or_its_worktree_only() {
    let h = harness();
    let task = h.task("R-1");
    let contracts = task.repo.join("docs").join("contracts");
    std::fs::create_dir_all(&contracts).unwrap();
    let dispatch = |read_dirs: Vec<String>| {
        h.service.dispatch(NewJob {
            task_id: task.id.clone(),
            role: "implementation".into(),
            agent: DeliveryAgent::Claude,
            model: None,
            access: DeliveryAccess::Workspace,
            prompt: "events=claude-ok".into(),
            writes: None,
            lease: None,
            timeout_minutes: None,
            reasoning: None,
            platform: None,
            read_dirs,
            stage_default: false,
            needs_environment: false,
        })
    };
    let job = dispatch(vec![plain_path(&contracts), plain_path(&task.worktree)]).unwrap();
    assert_eq!(
        job.read_dirs,
        [plain_path(&contracts), plain_path(&task.worktree)]
    );
    h.wait(&job.id);
    let elsewhere = tempfile::tempdir().unwrap();
    let missing = task.repo.join("docs").join("nope");
    let escape = format!("{}/docs/../..", plain_path(&task.repo));
    for dir in [
        plain_path(elsewhere.path()),
        plain_path(&missing),
        escape,
        "docs".into(),
    ] {
        assert_eq!(
            dispatch(vec![dir.clone()]).unwrap_err().category,
            "invalid_read_dir",
            "{dir}"
        );
    }
}

#[test]
fn jobs_run_with_the_repos_environment_for_checks() {
    let h = harness();
    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.check_env.insert(
        "TEST_DB_MONGO_URL".into(),
        "mongodb://localhost:27017/agos{task}".into(),
    );
    settings
        .check_env
        .insert("TEST_DB_NAME".into(), "tinto_test".into());
    settings
        .check_env
        .insert("TEST_DB_RUN".into(), "agos{task}_{job}".into());
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    let task = h.task("AGOS-474");
    let job = h.dispatch(&task, "tests", "events=codex-ok env=TEST_DB_MONGO_URL");
    // `{task}` gives each task its own value, such as its own test database.
    assert_eq!(
        job.check_env["TEST_DB_MONGO_URL"],
        "mongodb://localhost:27017/agosagos_474"
    );
    assert_eq!(job.check_env["TEST_DB_NAME"], "tinto_test");
    // `{job}` gives each job its own: a check and an agent never share it.
    assert_eq!(
        job.check_env["TEST_DB_RUN"],
        format!("agosagos_474_{}", &job.id[..8])
    );
    h.wait(&job.id);
    let seen = std::fs::read_to_string(task.worktree.join("env.txt")).unwrap();
    assert_eq!(seen.trim(), "mongodb://localhost:27017/agosagos_474");
}

#[test]
fn check_variables_are_names_a_shell_can_export() {
    let clean = |name: &str| {
        super::service::clean_check_env([(name.to_string(), " v ".to_string())].into())
    };
    assert_eq!(
        clean(" TEST_DB_MONGO_URL ").unwrap(),
        [("TEST_DB_MONGO_URL".to_string(), "v".to_string())].into()
    );
    for name in ["", "1DB", "TEST-DB", "A B", "X;rm"] {
        assert_eq!(
            clean(name).unwrap_err().category,
            "invalid_check_env",
            "{name}"
        );
    }
}

/// A Delivery tool called as a coordinator calls it: the reply's `result`.
fn tool(h: &Harness, name: &str, arguments: serde_json::Value) -> serde_json::Value {
    super::mcp::handle_message(
        &h.service,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
    )
    .unwrap()["result"]
        .clone()
}

/// A run held by `coord` at generation 1, and the fence its writes carry.
fn coordinated_run(h: &Harness) -> serde_json::Value {
    let run = tool(
        h,
        "create_run",
        json!({"repo": plain_path(h.repo.path()), "title": "B", "owner": "coord"}),
    );
    json!({"run_id": run["structuredContent"]["id"], "owner": "coord", "generation": 1})
}

fn with(fence: &serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    let mut arguments = fence.clone();
    for (key, value) in extra.as_object().unwrap() {
        arguments[key] = value.clone();
    }
    arguments
}

#[test]
fn a_tasks_planning_folders_reach_every_job_and_are_checked_when_set() {
    let h = harness();
    let repo = plain_path(h.repo.path());
    let issue = h.repo.path().join(".agent/backlog-delivery/issues/P-1");
    let context = h.repo.path().join(".agent/backlog-delivery/context");
    std::fs::create_dir_all(&issue).unwrap();
    std::fs::create_dir_all(&context).unwrap();
    let (issue, context) = (plain_path(&issue), plain_path(&context));
    let fence = coordinated_run(&h);
    let new_task = |read_dirs: serde_json::Value| {
        tool(
            &h,
            "create_task",
            with(
                &fence,
                json!({"repo": repo, "key": "P-1", "title": "x", "read_dirs": read_dirs}),
            ),
        )
    };
    // A folder outside the main checkout is refused before anything is made.
    let outside = new_task(json!([plain_path(&std::env::temp_dir())]));
    assert_eq!(outside["isError"], true, "{outside}");
    assert!(h.service.overview().unwrap().tasks.is_empty());

    let created = new_task(json!([issue, context]));
    assert_eq!(
        created["structuredContent"]["read_dirs"],
        json!([issue, context]),
        "{created}"
    );
    let task_id = created["structuredContent"]["id"].clone();
    // Every job of the task reads them, a writing stage too, unasked.
    let job = tool(
        &h,
        "dispatch_job",
        with(
            &fence,
            json!({"task_id": task_id, "role": "implementation", "agent": "codex", "prompt": "events=codex-ok"}),
        ),
    );
    assert_eq!(
        job["structuredContent"]["read_dirs"],
        json!([issue, context]),
        "{job}"
    );
    h.wait(job["structuredContent"]["id"].as_str().unwrap());

    // set_task_state replaces them, checked the same way.
    let refused = tool(
        &h,
        "set_task_state",
        with(&fence, json!({"task_id": task_id, "read_dirs": ["docs"]})),
    );
    assert_eq!(refused["isError"], true, "{refused}");
    let set = tool(
        &h,
        "set_task_state",
        with(&fence, json!({"task_id": task_id, "read_dirs": [issue]})),
    );
    assert_eq!(
        set["structuredContent"]["read_dirs"],
        json!([issue]),
        "{set}"
    );
    assert_eq!(set["structuredContent"]["state"], "intake");
}

#[test]
fn a_stages_time_limit_applies_when_its_dispatch_gives_none_and_a_retry_can_change_it() {
    let h = harness();
    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.stages.insert(
        "tests".into(),
        DeliveryStageAgent {
            agent: DeliveryAgent::Codex,
            model: None,
            reasoning: None,
            timeout_minutes: Some(120),
        },
    );
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    let task = h.task("L-1");
    let dispatch = |role: &str, timeout_minutes: Option<u32>| {
        let job = h
            .service
            .dispatch(NewJob {
                task_id: task.id.clone(),
                role: role.into(),
                agent: DeliveryAgent::Codex,
                model: None,
                access: DeliveryAccess::Workspace,
                prompt: "events=codex-ok".into(),
                writes: None,
                lease: None,
                timeout_minutes,
                reasoning: None,
                platform: None,
                read_dirs: Vec::new(),
                stage_default: false,
                needs_environment: false,
            })
            .unwrap();
        h.wait(&job.id)
    };
    assert_eq!(dispatch("tests", None).timeout_minutes, 120);
    let given = dispatch("tests", Some(7));
    assert_eq!(given.timeout_minutes, 7);
    assert_eq!(dispatch("implementation", None).timeout_minutes, 60);
    // Scoping reads the code and writes nothing.
    let explore = dispatch("explore", None);
    assert!(!explore.writes);
    assert_eq!(explore.timeout_minutes, 30);
    assert_eq!(explore.result_state, Some(DeliveryResultState::Accepted));

    let longer = h.service.retry(&given.id, Some(200)).unwrap();
    assert_eq!(longer.timeout_minutes, 200);
    h.wait(&longer.id);
    let same = h.service.retry(&given.id, None).unwrap();
    assert_eq!(same.timeout_minutes, 7);
    h.wait(&same.id);
}

#[test]
fn a_job_that_ran_out_of_time_or_was_cut_off_goes_on_in_its_session() {
    let h = harness();
    let task = h.task("C-1");
    let first = h.wait(&h.dispatch(&task, "implementation", "events=codex-ok").id);
    assert!(first.provider_session_id.is_some());
    // A job that finished is retried, not continued.
    assert_eq!(
        h.service
            .continue_job(&first.id, None)
            .unwrap_err()
            .category,
        "job_not_continuable"
    );
    // As if its time had run out.
    let mut stopped = first.clone();
    stopped.status = DeliveryJobStatus::Failed;
    stopped.error = Some("Se agotó el tiempo máximo de 60 minutos.".into());
    h.service.store().unwrap().put_job(&stopped).unwrap();
    let continued = h.service.continue_job(&stopped.id, Some(120)).unwrap();
    assert_eq!(continued.resumes.as_deref(), Some(stopped.id.as_str()));
    assert_eq!(continued.provider_session_id, stopped.provider_session_id);
    assert_eq!(continued.prompt, stopped.prompt);
    assert_eq!((continued.attempt, continued.timeout_minutes), (2, 120));
    let note = continued.resume_note.clone().unwrap();
    assert!(
        note.contains("se agotó su tiempo máximo de 60 minutos")
            && note.contains("El worktree conserva los cambios"),
        "{note}"
    );
    h.wait(&continued.id);
    // Only the latest attempt of the stage goes on.
    assert_eq!(
        h.service
            .continue_job(&stopped.id, None)
            .unwrap_err()
            .category,
        "job_superseded"
    );

    // Cut off by a restart: the same, with its time limit kept.
    let mut cut = h.wait(&h.dispatch(&task, "review", "events=codex-ok").id);
    cut.status = DeliveryJobStatus::Interrupted;
    cut.error = Some("Tinto se cerró mientras el trabajo corría.".into());
    h.service.store().unwrap().put_job(&cut).unwrap();
    let continued = h.service.continue_job(&cut.id, None).unwrap();
    assert_eq!(continued.timeout_minutes, cut.timeout_minutes);
    assert!(continued
        .resume_note
        .as_deref()
        .unwrap()
        .contains("Tinto se cerró"));
    h.wait(&continued.id);

    // An agent that never started its session has none to go on in.
    let mut silent = h.wait(&h.dispatch(&task, "tests", "events=none").id);
    silent.status = DeliveryJobStatus::Interrupted;
    silent.provider_session_id = Some("never-started".into());
    h.service.store().unwrap().put_job(&silent).unwrap();
    assert_eq!(
        h.service
            .continue_job(&silent.id, None)
            .unwrap_err()
            .category,
        "session_unknown"
    );
}

#[test]
fn models_are_checked_against_the_catalog_tinto_last_saw() {
    let h = harness();
    // Before any catalog is seen, nothing is refused.
    h.service
        .check_model(DeliveryAgent::Codex, "gpt-9")
        .unwrap();
    h.service
        .set_model_catalog(
            DeliveryAgent::Codex,
            Some("gpt-6.1-sol".into()),
            vec!["gpt-6.1-sol".into(), " gpt-6-astra ".into(), "".into()],
        )
        .unwrap();
    h.service
        .check_model(DeliveryAgent::Codex, "gpt-6-astra")
        .unwrap();
    let refused = h
        .service
        .check_model(DeliveryAgent::Codex, "gpt-9")
        .unwrap_err();
    assert_eq!(refused.category, "model_unavailable");
    assert!(
        refused.message.ends_with("gpt-6.1-sol, gpt-6-astra"),
        "{}",
        refused.message
    );
    // Muse Code has its own catalog; Claude Code's models are a fixed list.
    h.service
        .set_model_catalog(DeliveryAgent::Muse, None, vec!["muse-spark-1.3".into()])
        .unwrap();
    assert_eq!(
        h.service
            .check_model(DeliveryAgent::Muse, "gpt-6-astra")
            .unwrap_err()
            .category,
        "model_unavailable"
    );
    h.service
        .check_model(DeliveryAgent::Claude, "opus")
        .unwrap();

    // A coordinator naming a model the account lacks is told the valid ones.
    let fence = coordinated_run(&h);
    let task = tool(
        &h,
        "create_task",
        with(
            &fence,
            json!({"repo": plain_path(h.repo.path()), "key": "M-2", "title": "x"}),
        ),
    );
    let dispatched = tool(
        &h,
        "dispatch_job",
        with(
            &fence,
            json!({"task_id": task["structuredContent"]["id"], "role": "tests", "agent": "codex", "model": "gpt-9", "prompt": "events=codex-ok"}),
        ),
    );
    assert_eq!(dispatched["isError"], true, "{dispatched}");
    let text = dispatched["content"][0]["text"].as_str().unwrap();
    assert!(
        text.starts_with("model_unavailable") && text.contains("gpt-6-astra"),
        "{text}"
    );
}

#[test]
fn a_coordinator_proposes_variables_for_checks_and_the_user_approves_them() {
    let h = harness();
    let fence = coordinated_run(&h);
    let task = tool(
        &h,
        "create_task",
        with(
            &fence,
            json!({"repo": plain_path(h.repo.path()), "key": "V-1", "title": "x"}),
        ),
    );
    let proposed = tool(
        &h,
        "propose_settings",
        with(
            &fence,
            json!({"task_id": task["structuredContent"]["id"], "reason": "una base de datos por tarea", "check_env": {"TEST_DB_MONGO_URL": "mongodb://127.0.0.1:27419/agos{task}"}}),
        ),
    );
    let proposal = &proposed["structuredContent"];
    assert_eq!(proposal["status"], "pending", "{proposed}");
    let changes: DeliverySettingsChanges =
        serde_json::from_value(proposal["changes"].clone()).unwrap();
    h.service
        .approve_settings_proposal(proposal["id"].as_str().unwrap(), changes, "user")
        .unwrap();
    let settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    assert_eq!(
        settings.check_env["TEST_DB_MONGO_URL"],
        "mongodb://127.0.0.1:27419/agos{task}"
    );
}

fn choice(question: &str, recommended: Option<usize>) -> super::decisions::NewDecision {
    super::decisions::NewDecision {
        kind: DeliveryDecisionKind::Choice,
        question: question.into(),
        detail: "flag --dir".into(),
        references: Vec::new(),
        options: ["Sí", "No"]
            .iter()
            .enumerate()
            .map(|(index, label)| DeliveryDecisionOption {
                label: label.to_string(),
                consequence: format!("consecuencia {label}"),
                recommended: recommended == Some(index),
            })
            .collect(),
        text: String::new(),
        command: None,
        undo: String::new(),
        scripts: Vec::new(),
        read_dirs: Vec::new(),
        write_dirs: Vec::new(),
        clients: Vec::new(),
        full_access: false,
        blocking: true,
    }
}

fn permission(command: &str) -> super::decisions::NewDecision {
    super::decisions::NewDecision {
        kind: DeliveryDecisionKind::Permission,
        question: "¿Instalar en tu ~/.claude real?".into(),
        detail: String::new(),
        references: Vec::new(),
        options: Vec::new(),
        text: String::new(),
        command: Some(command.into()),
        undo: "agentos skills uninstall demo".into(),
        scripts: Vec::new(),
        read_dirs: Vec::new(),
        write_dirs: Vec::new(),
        clients: Vec::new(),
        full_access: false,
        blocking: true,
    }
}

#[test]
fn decisions_are_validated() {
    let h = harness();
    let task = h.task("D-1");
    let mut one_option = choice("¿Codex?", Some(0));
    one_option.options.truncate(1);
    let mut two_recommended = choice("¿Codex?", Some(0));
    two_recommended.options[1].recommended = true;
    let mut no_undo = permission("agentos skills install");
    no_undo.undo.clear();
    let mut empty_text = choice("Texto del aviso", None);
    empty_text.kind = DeliveryDecisionKind::Text;
    for bad in [one_option, two_recommended, no_undo, empty_text] {
        let error = h
            .service
            .request_decisions(&task.id, vec![bad], "coord")
            .unwrap_err();
        assert_eq!(error.category, "invalid_decision");
    }
    assert!(h.service.task_decisions(&task.id).unwrap().is_empty());
}

#[test]
fn a_choice_answered_in_the_users_own_words_reaches_the_jobs() {
    let h = harness();
    let task = h.task("D-3");
    let asked = h
        .service
        .request_decisions(
            &task.id,
            vec![choice("¿Instalamos también para Codex?", Some(0))],
            "coord",
        )
        .unwrap();
    assert!(h
        .service
        .answer_decision(&asked[0].id, " ", "user")
        .is_err());
    h.service
        .answer_decision(&asked[0].id, "Solo si ya está instalado", "user")
        .unwrap();
    let tests = h.dispatch(&task, "tests", "events=codex-ok");
    assert!(tests.decisions[0].ends_with("→ Solo si ya está instalado"));
    h.wait(&tests.id);
}

#[test]
fn pending_decisions_block_stages_and_answers_reach_the_jobs() {
    let h = harness();
    let task = h.task("D-2");
    let mut text = choice("Texto del aviso", None);
    text.kind = DeliveryDecisionKind::Text;
    text.options.clear();
    text.text = "Instalado en la carpeta final".into();
    let asked = h
        .service
        .request_decisions(
            &task.id,
            vec![
                choice("¿Instalamos también para Codex?", Some(0)),
                text,
                permission("agentos skills install"),
            ],
            "coord",
        )
        .unwrap();
    assert_eq!(asked.len(), 3);
    let blocked = h
        .service
        .dispatch(NewJob {
            task_id: task.id.clone(),
            role: "tests".into(),
            agent: DeliveryAgent::Codex,
            model: None,
            access: DeliveryAccess::Workspace,
            prompt: "events=codex-ok".into(),
            writes: None,
            lease: None,
            timeout_minutes: None,
            reasoning: None,
            platform: None,
            read_dirs: Vec::new(),
            stage_default: false,
            needs_environment: false,
        })
        .unwrap_err();
    assert_eq!(blocked.category, "decisions_pending");

    // Recommended choices and texts are accepted; the permission waits.
    let accepted = h.service.accept_recommended(&task.id, "user").unwrap();
    assert_eq!(accepted.len(), 2);
    assert_eq!(accepted[0].answer.as_deref(), Some("Sí"));
    assert_eq!(
        accepted[1].answer.as_deref(),
        Some("Instalado en la carpeta final")
    );
    assert_eq!(accepted[0].decided_by.as_deref(), Some("user"));
    let permission_id = asked[2].id.clone();
    assert_eq!(
        h.service
            .answer_decision(&permission_id, "maybe", "user")
            .unwrap_err()
            .category,
        "invalid_decision"
    );
    h.service
        .answer_decision(&permission_id, super::decisions::ALLOWED, "user")
        .unwrap();
    assert_eq!(
        h.service
            .answer_decision(&permission_id, super::decisions::DENIED, "user")
            .unwrap_err()
            .category,
        "decision_answered"
    );

    let tests = h.dispatch(&task, "tests", "events=codex-ok");
    assert_eq!(tests.decisions.len(), 3);
    assert!(tests.decisions[0].ends_with("→ Sí"));
    assert!(tests.decisions[2].contains("allowed (command: agentos skills install)"));
    assert!(tests.allowed_commands.is_empty());
    h.wait(&tests.id);
    let qa = h
        .service
        .dispatch(NewJob {
            task_id: task.id.clone(),
            role: "qa".into(),
            agent: DeliveryAgent::Claude,
            model: None,
            access: DeliveryAccess::Workspace,
            prompt: "events=claude-ok".into(),
            writes: None,
            lease: None,
            timeout_minutes: None,
            reasoning: None,
            platform: None,
            read_dirs: Vec::new(),
            stage_default: false,
            needs_environment: false,
        })
        .unwrap();
    assert!(qa
        .allowed_commands
        .contains(&"agentos skills install".to_string()));
    h.wait(&qa.id);
    assert_eq!(h.service.overview().unwrap().decisions.len(), 3);
}

#[test]
fn a_coordinator_asks_for_decisions_over_mcp() {
    let h = harness();
    let call = |id: u64, name: &str, arguments: serde_json::Value| {
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
        )
        .unwrap()["result"]
            .clone()
    };
    let repo = plain_path(h.repo.path());
    let run = call(
        1,
        "create_run",
        json!({"repo": repo, "title": "B", "owner": "coord"}),
    );
    let run_id = run["structuredContent"]["id"].as_str().unwrap().to_string();
    let task = call(
        2,
        "create_task",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "repo": repo, "key": "D-3", "title": "x"}),
    );
    let task_id = task["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let asked = call(
        3,
        "request_decisions",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "task_id": task_id, "decisions": [
            {"kind": "choice", "question": "¿Codex también?", "options": [
                {"label": "Sí", "consequence": "Instala en los dos", "recommended": true},
                {"label": "No", "consequence": "Solo Claude"}
            ]},
            {"kind": "permission", "question": "¿Escribir en ~/.claude?", "undo": "borrar la carpeta", "command": "agentos skills install", "read_dirs": ["C:/Users/me/.claude/skills"]}
        ]}),
    );
    assert_eq!(
        asked["structuredContent"]["result"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "{asked}"
    );
    let bad = call(
        4,
        "request_decisions",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "task_id": task_id, "decisions": [{"kind": "vote", "question": "?"}]}),
    );
    assert_eq!(bad["isError"], true);
    assert_eq!(
        asked["structuredContent"]["result"][1]["read_dirs"],
        json!(["C:/Users/me/.claude/skills"])
    );
    // One invalid item: nothing is created and the error names it.
    let mixed = call(
        6,
        "request_decisions",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "task_id": task_id, "decisions": [
            {"kind": "text", "question": "Texto del aviso", "text": "Instalado"},
            {"kind": "permission", "question": "¿Ejecutar el script?", "undo": "nada", "command": "bash qa/run.sh"}
        ]}),
    );
    assert_eq!(mixed["isError"], true);
    let reason = mixed["content"][0]["text"].as_str().unwrap();
    assert!(reason.contains("#2 «¿Ejecutar el script?»"), "{reason}");
    assert!(reason.contains("Las otras 1 son válidas"), "{reason}");
    let platform = call(
        7,
        "dispatch_job",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "task_id": task_id, "role": "qa", "agent": "claude", "prompt": "x", "platform": "mac"}),
    );
    assert_eq!(platform["isError"], true);
    assert!(platform["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("plataforma desconocida"));
    let read = call(5, "read_decisions", json!({"task_id": task_id}));
    assert_eq!(read["structuredContent"]["result"][0]["status"], "pending");
    assert_eq!(read["structuredContent"]["result"][1]["kind"], "permission");
}

#[test]
fn a_permission_cannot_run_files_agents_can_edit() {
    let h = harness();
    let task = h.task("D-4");
    let repo = plain_path(h.repo.path());
    let worktree = plain_path(&task.worktree);
    let other_worktree = plain_path(&task.worktree.with_file_name("D-40"));
    let refused = [
        format!("bash {repo}/.agent/backlog-delivery/issues/D-4/qa/run.sh"),
        format!("bash {other_worktree}/scripts/install.sh"),
        "bash qa/agentos-qa.sh".to_string(),
        "python ./check.py".to_string(),
        format!("node {}/tools/run.js", repo.replace('\\', "/")),
    ];
    for command in refused {
        let error = h
            .service
            .request_decisions(&task.id, vec![permission(&command)], "coord")
            .unwrap_err();
        assert_eq!(error.category, "invalid_decision", "{command}");
    }
    let asked = h
        .service
        .request_decisions(
            &task.id,
            vec![
                permission("agentos skills install team/reviewer --target codex"),
                // The task's own code: allowed, for one candidate.
                permission(&format!("node {worktree}/cli/index.js")),
            ],
            "coord",
        )
        .unwrap();
    assert!(!asked[0].runs_candidate);
    assert!(asked[1].runs_candidate);

    let mut outside = permission("agentos skills list");
    outside.read_dirs = vec!["C:/".into()];
    assert!(h
        .service
        .request_decisions(&task.id, vec![outside.clone()], "coord")
        .is_err());
    outside.read_dirs = vec!["skills".into()];
    assert!(h
        .service
        .request_decisions(&task.id, vec![outside], "coord")
        .is_err());
}

fn claude_qa(task: &DeliveryTask, prompt: &str) -> NewJob {
    NewJob {
        task_id: task.id.clone(),
        role: "qa".into(),
        agent: DeliveryAgent::Claude,
        model: None,
        access: DeliveryAccess::Workspace,
        prompt: prompt.into(),
        writes: None,
        lease: None,
        timeout_minutes: None,
        reasoning: None,
        platform: None,
        read_dirs: Vec::new(),
        stage_default: false,
        needs_environment: false,
    }
}

fn qa_lease(h: &Harness) -> super::model::DeliveryLease {
    h.service
        .overview()
        .unwrap()
        .leases
        .into_iter()
        .find(|lease| lease.name == "qa")
        .unwrap()
}

#[test]
fn a_permission_freezes_its_scripts_and_holds_for_one_candidate() {
    let h = harness();
    let task = h.task("D-5");
    let slashes = |path: &std::path::Path| plain_path(path).replace('\\', "/");
    let qa_dir = h.repo.path().join(".agent").join("qa");
    std::fs::create_dir_all(&qa_dir).unwrap();
    let script = qa_dir.join("run.sh");
    let approved = format!(
        "WT={}\n. \"{}/.agent/env/qa.env\"\nexec node \"$WT/cli/index.js\" \"$@\"\n",
        slashes(&task.worktree),
        slashes(h.repo.path()),
    );
    std::fs::write(&script, &approved).unwrap();
    let skills = tempfile::tempdir().unwrap();

    let mut request = permission(&format!("bash {}", slashes(&script)));
    request.scripts = vec![plain_path(&script)];
    request.read_dirs = vec![plain_path(skills.path())];
    let asked = h
        .service
        .request_decisions(&task.id, vec![request], "coord")
        .unwrap()
        .remove(0);
    // What the user reads is what was frozen, with what it leaves unfrozen.
    assert_eq!(asked.scripts[0].content, approved);
    assert_eq!(asked.scripts[0].path, slashes(&script));
    assert!(asked.runs_candidate);
    assert_eq!(asked.unfrozen.len(), 1, "{:?}", asked.unfrozen);
    assert!(asked.unfrozen[0].ends_with("/.agent/env/qa.env"));

    // An agent rewrites the script after the user allows it.
    let allowed = h
        .service
        .answer_decision(&asked.id, super::decisions::ALLOWED, "user")
        .unwrap();
    assert!(allowed.candidate.is_some());
    std::fs::write(&script, "rm -rf ~\n").unwrap();

    let qa = h
        .service
        .dispatch(claude_qa(&task, "events=claude-ok"))
        .unwrap();
    assert_eq!(qa.read_dirs, vec![plain_path(skills.path())]);
    let qa = h.wait(&qa.id);
    assert_eq!(qa.status, DeliveryJobStatus::Finished, "{:?}", qa.error);
    assert!(qa.ran_on.is_some());
    let copy = &qa.frozen_scripts[0];
    assert_eq!(copy.original, slashes(&script));
    assert_eq!(std::fs::read_to_string(&copy.copy).unwrap(), approved);
    let prompt = super::adapters::job_prompt(
        &qa,
        &task,
        &super::adapters::JobPaths::in_dir(PathBuf::from("jobs")),
        false,
        &[],
    );
    assert!(
        prompt.contains(&format!(
            "The repo's checks: bash {}.",
            copy.copy.replace('\\', "/")
        )),
        "{prompt}"
    );

    // The task's code changes: the permission no longer covers it, and the
    // job that never started leaves the QA resource free.
    std::fs::write(task.worktree.join("cli.js"), "changed\n").unwrap();
    let stale = h
        .service
        .dispatch(claude_qa(&task, "events=claude-ok"))
        .unwrap();
    let stale = h.wait(&stale.id);
    assert_eq!(stale.status, DeliveryJobStatus::Failed);
    assert!(stale.error.unwrap().contains("Vuelve a permitirlo"));
    assert_eq!(qa_lease(&h).state, DeliveryLeaseState::Free);

    // Only the user's renewal makes it valid for the new content.
    let renewed = h.service.renew_permissions(&task.id, "user").unwrap();
    assert_eq!(renewed.len(), 1);
    assert_ne!(renewed[0].candidate, allowed.candidate);
    let again = h
        .service
        .dispatch(claude_qa(&task, "events=claude-ok"))
        .unwrap();
    assert_eq!(h.wait(&again.id).status, DeliveryJobStatus::Finished);
    // The job that never started still says which code it found, so the
    // view can tell the permission no longer covers it.
    assert!(stale.start_candidate.is_some());
    let other = h.task("D-5b");
    assert_eq!(
        h.service
            .renew_permissions(&other.id, "user")
            .unwrap_err()
            .category,
        "invalid_decision"
    );
}

#[test]
fn past_the_brainstorm_qa_neither_waits_on_a_stale_permission_nor_on_a_quarantine() {
    let h = harness();
    let run = h
        .service
        .create_run(h.repo.path().to_path_buf(), "B", Some("coord".into()), None)
        .unwrap();
    let task = h
        .service
        .create_task(NewTask {
            repo: h.repo.path().to_path_buf(),
            distro: None,
            key: "D-8".into(),
            title: "x".into(),
            read_dirs: Vec::new(),
            base: None,
            branch: None,
            run_id: Some(run.id.clone()),
            package: None,
        })
        .unwrap();
    let release = |note: &str| {
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "release_lease", "arguments": {"run_id": run.id, "owner": "coord", "generation": 1, "name": "qa", "note": note}}}),
        )
        .unwrap()["result"]
            .clone()
    };

    // A QA job of the run crashes and leaves the resource quarantined;
    // while the user has the batch's questions to answer, only they free it.
    let crashing = h.dispatch(&task, "qa", "events=none exit=3");
    h.wait(&crashing.id);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(qa_lease(&h).state, DeliveryLeaseState::Quarantined);
    let asked = h
        .service
        .request_decisions(&task.id, vec![choice("¿Codex también?", Some(0))], "coord")
        .unwrap();
    let refused = release("limpio");
    assert_eq!(refused["isError"], true);
    assert!(
        refused["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("D-8 tiene decisiones sin responder"),
        "{refused}"
    );

    // Answered: the coordinator checks and frees it, though nobody called
    // end_brainstorm.
    h.service
        .answer_decision(&asked[0].id, "Sí", "user")
        .unwrap();
    assert_eq!(release(" ")["isError"], true, "it says what it checked");
    let released = release("sin procesos ni datos de prueba");
    assert_eq!(released["structuredContent"]["state"], "free", "{released}");
    assert!(released["structuredContent"]["note"]
        .as_str()
        .unwrap()
        .starts_with("Liberado por el coordinador coord"));
    // What it found reaches the user.
    let events = h.service.store().unwrap().events_after(0).unwrap();
    assert!(events.iter().any(|event| event.kind == "lease_released"
        && event.detail.contains("sin procesos ni datos de prueba")));
    h.service.end_brainstorm(&run.id).unwrap();

    // A permission that runs the task's code is allowed again for the code
    // QA finds, instead of failing it as stale.
    let cli = plain_path(&task.worktree.join("cli.js")).replace('\\', "/");
    std::fs::write(task.worktree.join("cli.js"), "v1\n").unwrap();
    let allowed = h
        .service
        .request_decisions(&task.id, vec![permission(&format!("node {cli}"))], "coord")
        .unwrap()
        .remove(0);
    assert!(allowed.runs_candidate);
    assert_eq!(allowed.decided_by.as_deref(), Some(super::decisions::AUTO));
    // Another size: written within the same moment as v1, a same-size file
    // can look unchanged to git's stat cache, and the code would not move.
    std::fs::write(task.worktree.join("cli.js"), "version 2\n").unwrap();
    let qa = h
        .service
        .dispatch(claude_qa(&task, "events=claude-ok"))
        .unwrap();
    let qa = h.wait(&qa.id);
    assert_eq!(qa.status, DeliveryJobStatus::Finished, "{:?}", qa.error);
    let renewed = h
        .service
        .task_decisions(&task.id)
        .unwrap()
        .into_iter()
        .find(|decision| decision.id == allowed.id)
        .unwrap();
    assert_ne!(renewed.candidate, allowed.candidate);
    assert_eq!(renewed.decided_by.as_deref(), Some(super::decisions::AUTO));
}

#[test]
fn a_pending_decision_can_be_withdrawn_and_an_answered_one_cannot() {
    let h = harness();
    let task = h.task("D-7");
    let asked = h
        .service
        .request_decisions(
            &task.id,
            vec![
                permission("agentos skills list"),
                permission("agentos skills install"),
            ],
            "coord",
        )
        .unwrap();
    h.service.withdraw_decision(&asked[0].id).unwrap();
    assert_eq!(h.service.task_decisions(&task.id).unwrap().len(), 1);
    h.service
        .answer_decision(&asked[1].id, super::decisions::ALLOWED, "user")
        .unwrap();
    assert_eq!(
        h.service
            .withdraw_decision(&asked[1].id)
            .unwrap_err()
            .category,
        "decision_answered"
    );
    // Nothing is pending any more: stages can run.
    h.service
        .dispatch(claude_qa(&task, "events=claude-ok"))
        .unwrap();
}

#[test]
fn a_batch_with_an_invalid_decision_says_which_and_creates_none() {
    let h = harness();
    let task = h.task("D-6");
    let mut no_undo = permission("agentos skills install");
    no_undo.undo.clear();
    no_undo.question = "¿Instalar sin deshacer?".into();
    let unfrozen = permission(&format!(
        "bash {}/.agent/qa/run.sh",
        plain_path(h.repo.path())
    ));
    let error = h
        .service
        .request_decisions(
            &task.id,
            vec![choice("¿Codex también?", Some(0)), no_undo, unfrozen],
            "coord",
        )
        .unwrap_err();
    assert_eq!(error.category, "invalid_decision");
    assert!(error.message.contains("no se creó ninguna decisión"));
    assert!(error.message.contains("#2 «¿Instalar sin deshacer?»"));
    assert!(error
        .message
        .contains("#3 «¿Instalar en tu ~/.claude real?»"));
    assert!(error.message.contains("decláralo en `scripts`"));
    assert!(error.message.contains("Las otras 1 son válidas"));
    assert!(!error.message.contains("#1"));
    assert!(h.service.task_decisions(&task.id).unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn cancelling_a_job_stops_what_it_started() {
    use std::process::Command;

    let ps = |args: &[&str]| -> String {
        Command::new("ps")
            .args(args)
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let h = harness();
    let task = h.task("D-8");
    let job = h.dispatch(&task, "tests", "events=none children=1 sleep=60");
    h.wait_status(&job.id, DeliveryJobStatus::Started);
    let dir = h._jobs.path().join(&job.id);
    let started = |name: &str| -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(pid) = std::fs::read_to_string(dir.join(name)) {
                let pid = pid.trim().to_string();
                if !pid.is_empty() {
                    return pid;
                }
            }
            assert!(Instant::now() < deadline, "{name} never appeared");
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let commands = [started("same.pid"), started("own.pid")];
    let deadline = Instant::now() + Duration::from_secs(10);
    while ps(&["-o", "pgid=", "-p", &commands[1]]) != commands[1] {
        assert!(
            Instant::now() < deadline,
            "the command never left the job's group"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    h.service.cancel(&job.id).unwrap();
    assert_eq!(h.wait(&job.id).status, DeliveryJobStatus::Cancelled);

    // Gone, or a zombie waiting for init to reap it.
    let deadline = Instant::now() + Duration::from_secs(5);
    while commands.iter().any(|pid| {
        let stat = ps(&["-o", "stat=", "-p", pid]);
        !stat.is_empty() && !stat.starts_with('Z')
    }) {
        assert!(Instant::now() < deadline, "a command outlived the cancel");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A task of `run`, as a coordinator creates it.
fn task_in(h: &Harness, run_id: &str, key: &str) -> DeliveryTask {
    h.service
        .create_task(NewTask {
            repo: h.repo.path().to_path_buf(),
            distro: None,
            key: key.into(),
            title: key.into(),
            read_dirs: Vec::new(),
            base: None,
            branch: None,
            run_id: Some(run_id.into()),
            package: None,
        })
        .unwrap()
}

/// Package `index` of 2 of issue `key`.
fn package(
    h: &Harness,
    key: &str,
    index: u32,
    builds_on: Option<u32>,
) -> Result<DeliveryTask, super::DeliveryError> {
    h.service.create_task(NewTask {
        repo: h.repo.path().to_path_buf(),
        distro: None,
        key: key.to_string(),
        title: format!("Task {key}"),
        read_dirs: Vec::new(),
        base: None,
        branch: None,
        run_id: None,
        package: Some(NewPackage {
            index,
            of: 2,
            title: format!("Parte {index}"),
            builds_on,
            until: None,
        }),
    })
}

#[test]
fn a_package_without_work_of_its_own_detaches_onto_the_base() {
    let h = harness();
    let first = package(&h, "K-61", 1, None).unwrap();
    let first_tree = PathBuf::from(plain_path(&first.worktree));
    std::fs::write(first_tree.join("backend.txt"), "api\n").unwrap();
    git(&first_tree, &["add", "."]);
    git(&first_tree, &["commit", "-q", "-m", "backend"]);
    let second = package(&h, "K-61", 2, Some(1)).unwrap();
    let second_tree = PathBuf::from(plain_path(&second.worktree));
    assert!(second_tree.join("backend.txt").exists());

    // Work of its own on package 1's code: detaching would lose it.
    std::fs::write(second_tree.join("ui.txt"), "ui\n").unwrap();
    assert_eq!(
        h.service.detach_package(&second.id).unwrap_err().category,
        "package_detach_refused"
    );
    std::fs::remove_file(second_tree.join("ui.txt")).unwrap();

    let detached = h.service.detach_package(&second.id).unwrap();
    assert_eq!(detached.base_ref, "main");
    assert_eq!(detached.package.unwrap().builds_on, None);
    assert!(detached.blockers.iter().all(|blocker| !blocker.is_open()));
    assert!(!second_tree.join("backend.txt").exists());
    assert_eq!(
        in_worktree(&second, &["rev-parse", "HEAD"]),
        in_worktree(&second, &["rev-parse", "main"])
    );
}

#[test]
fn a_package_starts_from_the_one_it_builds_on_and_waits_for_its_pr() {
    let h = harness();
    let first = package(&h, "K-60", 1, None).unwrap();
    assert!(first.worktree.ends_with("K-60-1"), "{:?}", first.worktree);
    assert_eq!(first.branch, "delivery/k-60-1");
    assert_eq!(first.base_ref, "main");
    // One task per package: the same package twice, or the issue as a
    // whole next to its packages, is refused.
    assert_eq!(
        package(&h, "K-60", 1, None).unwrap_err().category,
        "task_exists"
    );
    let whole = h.service.create_task(NewTask {
        repo: h.repo.path().to_path_buf(),
        distro: None,
        key: "K-60".into(),
        title: "Task K-60".into(),
        read_dirs: Vec::new(),
        base: None,
        branch: None,
        run_id: None,
        package: None,
    });
    assert_eq!(whole.unwrap_err().category, "task_exists");

    let second = package(&h, "K-60", 2, Some(1)).unwrap();
    assert_eq!(second.base_ref, first.branch);
    assert_eq!(
        second.package.as_ref().unwrap().builds_on.as_deref(),
        Some(first.id.as_str())
    );
    assert_eq!(second.blocked_reason, "espera al paquete 1");
    // Blocked on package 1, so not the coordinator's to move.
    let phase = h.service.overview().unwrap().phases[&second.id].clone();
    assert_eq!(phase.waits_on, super::phase::Waiter::Dependency);
    let refused = h.service.dispatch(NewJob {
        task_id: second.id.clone(),
        role: "tests".into(),
        agent: DeliveryAgent::Codex,
        model: None,
        access: DeliveryAccess::Workspace,
        prompt: "events=codex-ok".into(),
        writes: None,
        lease: None,
        timeout_minutes: None,
        reasoning: None,
        platform: None,
        read_dirs: Vec::new(),
        stage_default: false,
        needs_environment: false,
    });
    assert_eq!(refused.unwrap_err().category, "task_blocked");

    // Package 1 commits its work and opens its PR; package 2's first stage
    // starts from that work.
    let first_tree = PathBuf::from(plain_path(&first.worktree));
    std::fs::write(first_tree.join("backend.txt"), "api\n").unwrap();
    git(&first_tree, &["add", "."]);
    git(&first_tree, &["commit", "-q", "-m", "backend"]);
    put_step(&h, &first, "k60-pr", "pr", "executed", "PR abierto");
    h.service.settle_blockers().unwrap();
    let job = h.dispatch(&second, "tests", "events=codex-ok");
    assert_eq!(h.wait(&job.id).status, DeliveryJobStatus::Finished);
    let second_tree = PathBuf::from(plain_path(&second.worktree));
    assert!(second_tree.join("backend.txt").exists());
    assert_ne!(
        h.service.task(&second.id).unwrap().base_commit,
        second.base_commit
    );

    let issues = h.service.overview().unwrap().issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].of, 2);
    assert_eq!(
        issues[0]
            .packages
            .iter()
            .map(|package| package.task_id.as_str())
            .collect::<Vec<_>>(),
        [first.id.as_str(), second.id.as_str()]
    );
    assert!(!issues[0].delivered);
}

#[test]
fn a_task_created_as_a_whole_issue_becomes_its_first_package() {
    let h = harness();
    let whole = h.service.create_task(NewTask {
        repo: h.repo.path().to_path_buf(),
        distro: None,
        key: "K-61".into(),
        title: "Task K-61".into(),
        read_dirs: Vec::new(),
        base: None,
        branch: None,
        run_id: None,
        package: None,
    });
    let whole = whole.unwrap();
    let as_package = |index: u32, builds_on: Option<u32>| NewPackage {
        index,
        of: 2,
        title: format!("Parte {index}"),
        builds_on,
        until: None,
    };
    // An existing task keeps its base: it cannot build on another.
    let refused = h
        .service
        .set_task_package(&whole.id, &as_package(2, Some(1)));
    assert_eq!(refused.unwrap_err().category, "invalid_package");

    let first = h
        .service
        .set_task_package(&whole.id, &as_package(1, None))
        .unwrap();
    assert_eq!(
        first.package.as_ref().map(|p| (p.index, p.of)),
        Some((1, 2))
    );
    assert_eq!(first.worktree, whole.worktree);
    assert_eq!(first.base_ref, whole.base_ref);
    let again = h.service.set_task_package(&whole.id, &as_package(1, None));
    assert_eq!(again.unwrap_err().category, "invalid_package");

    // The issue's next package can now follow it.
    let second = package(&h, "K-61", 2, Some(1)).unwrap();
    assert_eq!(second.base_ref, first.branch);
    let issues = h.service.overview().unwrap().issues;
    assert_eq!(
        issues[0]
            .packages
            .iter()
            .map(|package| package.task_id.as_str())
            .collect::<Vec<_>>(),
        [first.id.as_str(), second.id.as_str()]
    );
}

#[test]
fn a_split_issue_moves_in_jira_once_and_closes_with_its_last_package() {
    let h = harness();
    let first = package(&h, "K-61", 1, None).unwrap();
    let last = package(&h, "K-61", 2, None).unwrap();
    let jira = |task: &DeliveryTask, stage: &str| NewApproval {
        task_id: task.id.clone(),
        rung: DeliveryRung::Jira,
        title: "Jira".into(),
        body: String::new(),
        requested_by: "coord".into(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: None,
        stage: Some(stage.into()),
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    };
    // A middle package's ladder ends at its merge; the last climbs the whole
    // one, which the phase leaves unsaid.
    let steps = |task: &DeliveryTask| h.service.overview().unwrap().phases[&task.id].steps.len();
    assert_eq!((steps(&first), steps(&last)), (5, 0));
    assert_eq!(
        h.service
            .request_approval(jira(&first, "after_merge"))
            .unwrap_err()
            .category,
        "invalid_approval"
    );
    // Package 1 moves the issue: package 2 then only links its PR.
    let mut moved = h
        .service
        .request_approval(jira(&first, "after_pr"))
        .unwrap();
    assert!(!moved.jira_link_only);
    moved.status = DeliveryApprovalStatus::Executed;
    moved.executed_at_ms = Some(3);
    h.service.store().unwrap().put_approval(&moved).unwrap();
    assert!(
        h.service
            .request_approval(jira(&last, "after_pr"))
            .unwrap()
            .jira_link_only
    );
    // The issue closes with the last package, once every package merged.
    assert_eq!(
        h.service
            .request_approval(jira(&last, "after_merge"))
            .unwrap_err()
            .category,
        "packages_unmerged"
    );
    for rung in ["commit", "push", "pr", "merge"] {
        put_step(
            &h,
            &first,
            &format!("k61-{rung}"),
            rung,
            "executed",
            "hecho",
        );
    }
    h.service
        .request_approval(jira(&last, "after_merge"))
        .unwrap();
    let overview = h.service.overview().unwrap();
    assert_eq!(
        overview.phases[&first.id].phase,
        super::phase::Phase::Delivered
    );
    assert!(!overview.issues[0].delivered);
}

#[test]
fn a_package_merges_after_the_one_it_builds_on_into_that_ones_base() {
    let h = harness();
    github_origin(&h);
    let github = FakeGitHub::new(
        json!({
            "state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "APPROVED",
            "statusCheckRollup": [], "reviews": [], "comments": [],
            "baseRefName": "delivery/k-62-1"
        }),
        json!([]),
    );
    h.service.set_github(github.clone());
    let first = package(&h, "K-62", 1, None).unwrap();
    let second = package(&h, "K-62", 2, Some(1)).unwrap();
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/51").unwrap();
    h.service.link_pr(&second.id, &reference).unwrap();
    h.service.refresh_pr(&second.id, Duration::ZERO).unwrap();
    let merge = NewApproval {
        task_id: second.id.clone(),
        rung: DeliveryRung::Merge,
        title: "K-62: UI (#51)".into(),
        body: String::new(),
        requested_by: "coord".into(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: Some("merge".into()),
        stage: None,
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    };
    // Its PR goes into package 1's branch: merging it now would land there.
    let refused = h.service.request_approval(merge.clone()).unwrap_err();
    assert_eq!(refused.category, "pr_not_ready");
    assert!(
        refused.message.contains("va a la rama de K-62 · 1/2"),
        "{}",
        refused.message
    );

    // Package 1 merges: package 2's PR goes where package 1 went.
    put_step(&h, &first, "k62-merge", "merge", "executed", "fusionado");
    assert_eq!(h.service.retarget_packages().unwrap(), 1);
    assert!(github
        .actions
        .lock()
        .unwrap()
        .contains(&"base https://github.com/o/r/pull/51 main".to_string()));
    assert_eq!(h.service.task(&second.id).unwrap().base_ref, "main");
    assert_eq!(h.service.retarget_packages().unwrap(), 0);
    let events = h.service.store().unwrap().events_after(0).unwrap();
    assert!(events.iter().any(|event| event.kind == "package_retargeted"
        && event.task_id.as_deref() == Some(second.id.as_str())));
    h.service.request_approval(merge).unwrap();
}

fn executed_step(task_id: &str, rung: &str) -> super::model::DeliveryApproval {
    serde_json::from_value(json!({
        "id": format!("{task_id}-{rung}"), "task_id": task_id, "rung": rung, "title": "",
        "body": "", "status": "executed", "requested_by": "coord", "requested_at_ms": 1,
        "decided_at_ms": 2, "executed_at_ms": 3, "outcome": null,
    }))
    .unwrap()
}

#[test]
fn a_task_blocked_on_another_tasks_merge_waits_until_it_merges() {
    use super::model::NewBlocker;
    use super::phase::Waiter;

    let h = harness();
    let run = h
        .service
        .create_run(h.repo.path().to_path_buf(), "B", Some("coord".into()), None)
        .unwrap();
    let blocked = task_in(&h, &run.id, "AGOS-139");
    let rag = task_in(&h, &run.id, "AGOS-580");
    let phase = |task: &DeliveryTask| h.service.overview().unwrap().phases[&task.id].clone();
    let idle = || -> Vec<String> {
        h.service
            .idle_tasks(&run.id)
            .unwrap()
            .iter()
            .map(|task| task["key"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    assert!(idle().contains(&"AGOS-139".to_string()));

    let task = h
        .service
        .set_blockers(
            &blocked.id,
            vec![NewBlocker::Task {
                task: "agos-580".into(),
                until: None,
            }],
            "needs the per-project RAG",
        )
        .unwrap();
    assert_eq!(task.blockers[0].status, "sin PR");
    assert_eq!(phase(&blocked).waits_on, Waiter::Dependency);
    assert!(!idle().contains(&"AGOS-139".to_string()));
    let refused = h
        .service
        .dispatch(NewJob {
            task_id: blocked.id.clone(),
            role: "tests".into(),
            agent: DeliveryAgent::Codex,
            model: None,
            access: DeliveryAccess::Workspace,
            prompt: "events=none".into(),
            writes: None,
            lease: None,
            timeout_minutes: None,
            reasoning: None,
            platform: None,
            read_dirs: Vec::new(),
            stage_default: false,
            needs_environment: false,
        })
        .unwrap_err();
    assert_eq!(refused.category, "task_blocked");
    assert!(
        refused.message.contains("AGOS-580 (fusionado)"),
        "{}",
        refused.message
    );

    // Its PR opening is not enough; the merge is.
    h.service
        .store()
        .unwrap()
        .put_approval(&executed_step(&rag.id, "pr"))
        .unwrap();
    assert_eq!(h.service.settle_blockers().unwrap(), 0);
    assert_eq!(
        h.service.task(&blocked.id).unwrap().blockers[0].status,
        "PR abierto"
    );
    h.service
        .store()
        .unwrap()
        .put_approval(&executed_step(&rag.id, "merge"))
        .unwrap();
    assert_eq!(h.service.settle_blockers().unwrap(), 1);

    let (events, _) = h.service.coordinator_events(&run.id, 0).unwrap();
    let cleared = events
        .iter()
        .find(|event| event["kind"] == "blocker_cleared")
        .expect("a blocker_cleared event");
    assert_eq!(cleared["key"], "AGOS-139");
    assert_eq!(phase(&blocked).waits_on, Waiter::Coordinator);
    assert!(idle().contains(&"AGOS-139".to_string()));
}

#[test]
fn a_task_blocked_on_an_outside_pr_clears_once_github_says_merged() {
    use super::model::NewBlocker;
    use super::phase::Waiter;

    let h = harness();
    let github = FakeGitHub::new(json!({"state": "OPEN"}), json!([]));
    h.service.set_github(github.clone());
    let task = h.task("AGOS-463");
    h.service
        .set_blockers(
            &task.id,
            vec![NewBlocker::Pr {
                url: "https://github.com/AIRInstitute/agentos/pull/305".into(),
                until: None,
            }],
            "",
        )
        .unwrap();
    assert_eq!(
        h.service.overview().unwrap().phases[&task.id].waits_on,
        Waiter::External
    );

    h.service.refresh_blocking_prs().unwrap();
    let still = h.service.task(&task.id).unwrap();
    assert!(still.blockers[0].is_open());
    assert_eq!(still.blockers[0].status, "abierto");

    *github.snapshot.lock().unwrap() = json!({"state": "MERGED"});
    h.service.refresh_blocking_prs().unwrap();
    let done = h.service.task(&task.id).unwrap();
    assert!(!done.blockers[0].is_open());
    assert_eq!(done.blockers[0].status, "fusionado");
}

#[test]
fn blockers_survive_a_takeover_and_the_user_can_override_them() {
    use super::model::NewBlocker;

    let h = harness();
    let run = h
        .service
        .create_run(h.repo.path().to_path_buf(), "B", Some("coord".into()), None)
        .unwrap();
    let task = task_in(&h, &run.id, "AGOS-1");
    h.service
        .set_blockers(
            &task.id,
            vec![NewBlocker::Note {
                text: "waiting for the design (v2)".into(),
            }],
            "",
        )
        .unwrap();
    h.service
        .takeover_run(&run.id, "coord-2", None, "the user asked")
        .unwrap();
    // Read back from the store, as after a restart.
    let stored = h.service.store().unwrap().task(&task.id).unwrap().unwrap();
    assert!(stored.blockers[0].is_open());

    let cleared = h.service.clear_blockers(&task.id, None, true).unwrap();
    assert!(!cleared.blockers[0].is_open());
    let (events, _) = h.service.coordinator_events(&run.id, 0).unwrap();
    assert!(events
        .iter()
        .any(|event| event["kind"] == "blockers_changed"));
}

#[test]
fn a_state_label_says_how_long_it_may_be() {
    let h = harness();
    let task = h.task("AGOS-2");
    let label = "blocked: waiting for AGOS-580 (project RAG)";
    let error = h
        .service
        .update_task(&task.id, Some(label.into()), None, None, false)
        .unwrap_err();
    assert!(
        error.message.contains("de 1 a 40 caracteres"),
        "{}",
        error.message
    );
    assert!(error.message.contains("tiene 43"), "{}", error.message);
    let short = h
        .service
        .update_task(
            &task.id,
            Some("blocked (AGOS-580)".into()),
            None,
            None,
            false,
        )
        .unwrap();
    assert_eq!(short.state, "blocked (AGOS-580)");
}

#[test]
fn a_block_named_only_in_the_label_points_to_set_task_blocked() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("AGOS-3");
    let labelled = coordinator.write(
        "set_task_state",
        json!({"task_id": task_id, "state": "blocked: waiting for AGOS-580"}),
    );
    assert!(labelled["structuredContent"]["notice"]
        .as_str()
        .is_some_and(|notice| notice.contains("set_task_blocked")));
    let plain = coordinator.write(
        "set_task_state",
        json!({"task_id": task_id, "state": "implementation"}),
    );
    assert!(plain["structuredContent"]["notice"].is_null());
}

#[test]
fn cancelling_qa_quarantines_its_resource_only_if_it_acted() {
    let h = harness();
    let task = h.task("D-7");
    // Cancelled before the agent ran anything: the resource is free.
    let idle = h.dispatch(&task, "qa", "events=none sleep=30");
    h.wait_status(&idle.id, DeliveryJobStatus::Started);
    h.service.cancel(&idle.id).unwrap();
    assert_eq!(h.wait(&idle.id).status, DeliveryJobStatus::Cancelled);
    assert_eq!(qa_lease(&h).state, DeliveryLeaseState::Free);

    // Cancelled after it ran commands: the user checks before releasing.
    let busy = h.dispatch(&task, "qa", "events=codex-ok sleep=30");
    h.wait_status(&busy.id, DeliveryJobStatus::Started);
    std::thread::sleep(Duration::from_millis(1500));
    h.service.cancel(&busy.id).unwrap();
    assert_eq!(h.wait(&busy.id).status, DeliveryJobStatus::Cancelled);
    let lease = qa_lease(&h);
    assert_eq!(lease.state, DeliveryLeaseState::Quarantined);
    let note = lease.note.unwrap();
    assert!(note.contains("de D-7 se canceló"), "{note}");
    assert!(note.contains("pulsa Liberar"), "{note}");
}

#[test]
fn coordinators_read_what_the_user_configured() {
    let h = harness();
    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.qa_browser = true;
    settings.qa_commands = vec!["agentos skills install".into()];
    settings.qa_environment = "Windows, ámbito personal; Claude Code y Codex".into();
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    let run = h
        .service
        .create_run(h.repo.path().to_path_buf(), "Lote", None, Some(true))
        .unwrap();
    let task = h
        .service
        .create_task(NewTask {
            repo: h.repo.path().to_path_buf(),
            distro: None,
            key: "D-5".into(),
            title: "x".into(),
            read_dirs: Vec::new(),
            base: None,
            branch: None,
            run_id: Some(run.id.clone()),
            package: None,
        })
        .unwrap();
    let read = super::mcp::handle_message(
        &h.service,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "read_settings", "arguments": {"task_id": task.id}}}),
    )
    .unwrap()["result"]["structuredContent"]
        .clone();
    assert_eq!(read["qa_browser"], true);
    assert_eq!(read["qa_commands"][0], "agentos skills install");
    assert_eq!(
        read["qa_environment"],
        "Windows, ámbito personal; Claude Code y Codex"
    );
    assert_eq!(read["run"]["qa_jira_comment"], true);

    let changed = h.service.set_run_qa_jira_comment(&run.id, false).unwrap();
    assert_eq!(changed.qa_jira_comment, Some(false));
    let unset = h
        .service
        .create_run(
            h.repo.path().to_path_buf(),
            "Otro",
            Some("coord".into()),
            None,
        )
        .unwrap();
    assert_eq!(unset.qa_jira_comment, None);

    let qa = h
        .service
        .dispatch(NewJob {
            task_id: task.id.clone(),
            role: "qa".into(),
            agent: DeliveryAgent::Codex,
            model: None,
            access: DeliveryAccess::Workspace,
            prompt: "events=codex-ok".into(),
            writes: None,
            lease: None,
            timeout_minutes: None,
            reasoning: None,
            platform: None,
            read_dirs: Vec::new(),
            stage_default: false,
            needs_environment: false,
        })
        .unwrap();
    assert_eq!(
        qa.qa_environment,
        "Windows, ámbito personal; Claude Code y Codex"
    );
    h.wait(&qa.id);
    let tests = h.dispatch(&task, "tests", "events=codex-ok");
    assert!(tests.qa_environment.is_empty());
    h.wait(&tests.id);
}

#[test]
fn a_permission_names_the_folders_qa_may_write_in() {
    let h = harness();
    let task = h.task("D-8");
    let home = crate::runtime_paths::user_home_dir().unwrap();

    // The whole home, a drive and a relative path are not "a folder".
    for refused in [
        plain_path(&home),
        plain_path(home.parent().unwrap()),
        "C:/".to_string(),
        "skills".to_string(),
    ] {
        let mut request = permission("agentos skills install demo");
        request.write_dirs = vec![refused.clone()];
        let error = h
            .service
            .request_decisions(&task.id, vec![request], "coord")
            .unwrap_err();
        assert!(error.message.contains("write_dirs"), "{refused}: {error}");
    }
    let mut unknown = permission("agentos skills install demo");
    unknown.clients = vec!["cursor".into()];
    let error = h
        .service
        .request_decisions(&task.id, vec![unknown], "coord")
        .unwrap_err();
    assert!(error.message.contains("cliente desconocido «cursor»"));

    // A concrete folder, which does not exist yet.
    let outside = tempfile::tempdir().unwrap();
    let skills = plain_path(&outside.path().join("skills"));
    let mut request = permission("agentos skills install demo");
    request.write_dirs = vec![skills.clone()];
    let asked = h
        .service
        .request_decisions(&task.id, vec![request], "coord")
        .unwrap()
        .remove(0);
    assert_eq!(asked.write_dirs, vec![skills.clone()]);
    assert!(asked.warnings.is_empty());
    h.service
        .answer_decision(&asked.id, super::decisions::ALLOWED, "user")
        .unwrap();

    // Other stages never get it; the QA job does, with Codex too, and the
    // folder is there when it starts.
    let tests = h.dispatch(&task, "tests", "events=codex-ok");
    assert!(tests.write_dirs.is_empty());
    assert!(tests.decisions[0].contains(&format!("(may write in: {skills})")));
    h.wait(&tests.id);
    assert!(!std::path::Path::new(&skills).exists());
    let qa = h.dispatch(&task, "qa", "events=codex-ok");
    assert_eq!(qa.write_dirs, vec![skills.clone()]);
    assert_eq!(qa.allowed_commands, vec!["agentos skills install demo"]);
    assert_eq!(h.wait(&qa.id).status, DeliveryJobStatus::Finished);
    assert!(std::path::Path::new(&skills).is_dir());
}

#[test]
fn a_permission_to_start_a_real_client_shows_what_the_client_writes() {
    let h = harness();
    let task = h.task("D-9");
    let folder = |client: &str| plain_path(&super::decisions::client_home(client).unwrap());
    let mut request = permission("node C:/qa/client-skills.mjs");
    request.clients = vec!["claude".into(), "codex".into()];
    let asked = h
        .service
        .request_decisions(&task.id, vec![request], "coord")
        .unwrap()
        .remove(0);
    assert_eq!(asked.clients, ["claude", "codex"]);
    assert_eq!(asked.write_dirs, [folder("claude"), folder("codex")]);
    // Codex keeps its own folder from its sandbox: said now, not in QA.
    #[cfg(windows)]
    {
        assert_eq!(asked.warnings.len(), 1, "{:?}", asked.warnings);
        assert!(asked.warnings[0].contains(&folder("codex")));
        assert!(asked.warnings[0].contains("no puede arrancar"));
    }

    let mut only_claude = permission("node C:/qa/client-skills.mjs");
    only_claude.clients = vec!["claude".into()];
    let asked = h
        .service
        .request_decisions(&task.id, vec![only_claude], "coord")
        .unwrap()
        .remove(0);
    assert_eq!(asked.write_dirs, [folder("claude")]);
    assert!(asked.warnings.is_empty());
}

#[test]
fn a_permission_is_warned_when_its_command_cannot_start_in_the_codex_sandbox() {
    let h = harness();
    let task = h.task("D-10");
    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.checks = vec!["npm test".into()];
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    // A machine where bash starts in no Codex sandbox.
    let tried = Arc::new(std::sync::Mutex::new(Vec::<Vec<String>>::new()));
    let record = tried.clone();
    h.service.set_sandbox_planner(Arc::new(move |_task, programs| {
        record.lock().unwrap().push(programs.to_vec());
        super::adapters::SandboxPlan {
            unelevated: true,
            note: None,
            obstacles: programs
                .iter()
                .filter(|program| *program == "bash")
                .map(|program| format!("{program} no puede ejecutarse en ningún sandbox de Codex en este equipo"))
                .collect(),
        }
    }));
    let asked = h
        .service
        .request_decisions(
            &task.id,
            vec![
                permission("bash C:/qa/run.sh"),
                permission("node C:/qa/run.mjs"),
                choice("¿Codex también?", Some(0)),
            ],
            "coord",
        )
        .unwrap();
    assert_eq!(
        asked[0].warnings,
        ["Con Codex como agente de QA: bash no puede ejecutarse en ningún sandbox de Codex en este equipo."]
    );
    assert!(asked[1].warnings.is_empty());
    assert!(asked[2].warnings.is_empty());
    // Each command is tried with the repo's checks, as the QA job will run.
    assert_eq!(*tried.lock().unwrap(), [["npm", "bash"], ["npm", "node"]]);
}

#[test]
fn a_coordinator_runs_qa_with_full_access_only_when_the_user_allowed_it() {
    let h = harness();
    // A machine whose Codex sandbox cannot start bash.
    h.service
        .set_sandbox_planner(Arc::new(|_task, _programs| super::adapters::SandboxPlan {
            obstacles: vec!["bash no puede ejecutarse".into()],
            ..Default::default()
        }));
    let call = |name: &str, arguments: serde_json::Value| {
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
        )
        .unwrap()["result"]
            .clone()
    };
    let repo = plain_path(h.repo.path());
    let run = call(
        "create_run",
        json!({"repo": repo, "title": "B", "owner": "coord"}),
    );
    let run_id = run["structuredContent"]["id"].as_str().unwrap().to_string();
    let fenced = |extra: serde_json::Value| {
        let mut arguments = json!({"run_id": run_id, "owner": "coord", "generation": 1});
        for (key, value) in extra.as_object().unwrap() {
            arguments[key] = value.clone();
        }
        arguments
    };
    let task = call(
        "create_task",
        fenced(json!({"repo": repo, "key": "D-11", "title": "x"})),
    );
    let task_id = task["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let dispatch = |role: &str| {
        let job = call(
            "dispatch_job",
            fenced(
                json!({"task_id": task_id, "role": role, "agent": "codex", "prompt": "events=codex-ok"}),
            ),
        );
        let id = job["structuredContent"]["id"].as_str().unwrap().to_string();
        h.wait(&id)
    };

    // Asking is not having: while the user has not answered nothing runs,
    // and a refusal leaves QA with workspace access.
    let ask = || {
        let asked = call(
            "request_decisions",
            fenced(json!({"task_id": task_id, "decisions": [{
                "kind": "permission", "question": "¿QA con acceso completo?",
                "undo": "nada que deshacer", "command": "bash C:/qa/run.sh login", "full_access": true
            }]})),
        );
        let decision = &asked["structuredContent"]["result"][0];
        assert_eq!(decision["full_access"], true, "{asked}");
        // No sandbox, so nothing to warn about the command.
        assert_eq!(decision["warnings"], json!([]));
        decision["id"].as_str().unwrap().to_string()
    };
    let refused = ask();
    h.service
        .answer_decision(&refused, super::decisions::DENIED, "user")
        .unwrap();
    assert_eq!(dispatch("qa").access, DeliveryAccess::Workspace);

    let allowed = ask();
    h.service
        .answer_decision(&allowed, super::decisions::ALLOWED, "user")
        .unwrap();
    let qa = dispatch(" QA ");
    assert_eq!(qa.access, DeliveryAccess::Full);
    assert!(qa
        .decisions
        .iter()
        .any(|note| note.contains("(QA with full access)")));
    // Only QA: every other stage stays in its sandbox.
    assert_eq!(dispatch("review").access, DeliveryAccess::Workspace);
    assert_eq!(dispatch("implementation").access, DeliveryAccess::Workspace);
}

fn commit(message: &str, paths: &[&str]) -> super::model::DeliveryCommit {
    super::model::DeliveryCommit {
        message: message.to_string(),
        paths: paths.iter().map(|path| path.to_string()).collect(),
    }
}

fn commit_plan(
    task: &DeliveryTask,
    commits: Vec<super::model::DeliveryCommit>,
    leave_out: &[&str],
) -> NewApproval {
    NewApproval {
        task_id: task.id.clone(),
        rung: DeliveryRung::Commit,
        title: String::new(),
        body: String::new(),
        requested_by: "coord".to_string(),
        commits,
        leave_out: leave_out.iter().map(|path| path.to_string()).collect(),
        single_commit_reason: None,
        merge_method: None,
        stage: None,
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    }
}

/// Runs git in a task's worktree and returns its output.
fn in_worktree(task: &DeliveryTask, args: &[&str]) -> String {
    super::tasks::git(super::tasks::Place::Native, &task.worktree, args).unwrap()
}

fn changed(task: &DeliveryTask) -> Vec<String> {
    super::tasks::changed_files(super::tasks::Place::Native, &task.worktree).unwrap()
}

fn write_files(task: &DeliveryTask, names: &[&str]) {
    for name in names {
        let file = task.worktree.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, format!("{name}\n")).unwrap();
    }
}

#[test]
fn a_commit_plan_is_refused_unless_every_changed_file_has_one_place() {
    let h = harness();
    let task = h.task("C-1");
    write_files(&task, &["a.txt", "b.txt", "docs/c.md"]);
    let draft = h.service.commit_draft(&task.id).unwrap();
    assert_eq!(draft.files, ["a.txt", "b.txt", "docs/c.md"]);
    assert_eq!(draft.recent, ["init"]);

    let refused = |plan: NewApproval| {
        let error = h.service.request_approval(plan).unwrap_err();
        assert_eq!(error.category, "invalid_commit_plan", "{error}");
        error.message
    };
    let all = ["a.txt", "b.txt", "docs/c.md"];
    // A file that does not exist (or has no changes).
    let message = refused(commit_plan(
        &task,
        vec![
            commit("feat: a", &all),
            commit("docs: x", &["docs/nope.md"]),
        ],
        &[],
    ));
    assert!(message.contains("docs/nope.md (el commit 2) no existe o no tiene cambios"));
    // The same file in two commits, or in a commit and left out.
    let message = refused(commit_plan(
        &task,
        vec![commit("feat: a", &all), commit("docs: c", &["docs/c.md"])],
        &["a.txt"],
    ));
    assert!(message.contains("docs/c.md está en el commit 1 y en el commit 2"));
    assert!(message.contains("a.txt está en los que se dejan fuera y en el commit 1"));
    // A changed file nobody placed.
    let message = refused(commit_plan(
        &task,
        vec![commit("feat: a", &["a.txt"])],
        &["b.txt"],
    ));
    assert!(message.contains("sin asignar: docs/c.md"));
    // A commit without message or without files.
    let message = refused(commit_plan(
        &task,
        vec![commit("  ", &all), commit("docs: c", &[])],
        &[],
    ));
    assert!(message.contains("el commit 1 no tiene mensaje"));
    assert!(message.contains("el commit 2 no incluye archivos"));
    assert!(h.service.overview().unwrap().approvals.is_empty());
}

#[test]
fn a_commit_plan_creates_its_commits_in_order_and_can_be_undone() {
    let h = harness();
    let task = h.task("C-2");
    let base = in_worktree(&task, &["rev-parse", "HEAD"]);
    std::fs::remove_file(task.worktree.join("README.md")).unwrap();
    write_files(
        &task,
        &["cli/a.ts", "cli/a.test.ts", "docs/c.md", "scratch.log"],
    );
    let body = "docs(cli): explain a - C-2\n\nWhy it changed.\nSecond line.";
    let asked = h
        .service
        .request_approval(commit_plan(
            &task,
            vec![
                commit("feat(cli): add a", &[r"cli\a.ts", "./README.md"]),
                commit("test(cli): cover a", &["cli/a.test.ts"]),
                commit(body, &["docs/c.md"]),
            ],
            &["scratch.log"],
        ))
        .unwrap();
    // What the user approves: the whole plan, and who prepared it.
    assert_eq!(asked.title, "3 commits");
    assert_eq!(asked.requested_by, "coord");
    assert_eq!(asked.commits[0].paths, ["cli/a.ts", "README.md"]);
    assert_eq!(asked.left_out, ["scratch.log"]);
    // The user may reword a message before approving: Tinto uses theirs.
    h.service
        .set_step_text(
            &asked.id,
            None,
            None,
            Some(vec![
                "feat(cli): add the a command".into(),
                "test(cli): cover a".into(),
                body.into(),
            ]),
        )
        .unwrap();

    let done = h.service.decide_approval(&asked.id, true, None).unwrap();
    assert_eq!(
        done.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        done.outcome
    );
    assert_eq!(done.created.len(), 3);
    assert_eq!(done.before.as_deref(), Some(base.as_str()));
    assert!(done.outcome.as_deref().unwrap().starts_with("3 commits ("));
    // Those commits, in that order, with those exact texts and files.
    assert_eq!(
        in_worktree(&task, &["log", "--format=%s", &format!("{base}..HEAD")]),
        "docs(cli): explain a - C-2\ntest(cli): cover a\nfeat(cli): add the a command"
    );
    assert_eq!(in_worktree(&task, &["log", "-1", "--format=%B"]), body);
    assert_eq!(in_worktree(&task, &["rev-parse", "HEAD"]), done.created[2]);
    let files = |commit: &str| in_worktree(&task, &["show", "--name-only", "--format=", commit]);
    assert_eq!(files(&done.created[0]), "README.md\ncli/a.ts");
    assert_eq!(files(&done.created[1]), "cli/a.test.ts");
    // Nothing is left but what the plan left out on purpose.
    assert_eq!(changed(&task), ["scratch.log"]);

    // Undone: every change is back in the worktree and the step is open.
    let undone = h.service.undo_commit(&asked.id).unwrap();
    assert_eq!(undone.status, DeliveryApprovalStatus::Undone);
    assert_eq!(in_worktree(&task, &["rev-parse", "HEAD"]), base);
    assert_eq!(
        changed(&task),
        [
            "README.md",
            "cli/a.test.ts",
            "cli/a.ts",
            "docs/c.md",
            "scratch.log"
        ]
    );
    assert!(!task.worktree.join("README.md").exists());
    assert_eq!(
        h.service.undo_commit(&asked.id).unwrap_err().category,
        "undo_unavailable"
    );
    let mut everything = commit_plan(
        &task,
        vec![commit(
            "feat: everything",
            &[
                "README.md",
                "cli/a.test.ts",
                "cli/a.ts",
                "docs/c.md",
                "scratch.log",
            ],
        )],
        &[],
    );
    everything.single_commit_reason = Some("the files only make sense together".into());
    let again = h.service.request_approval(everything).unwrap();
    assert_eq!(again.title, "feat: everything");
}

#[test]
fn a_commit_plan_stops_at_the_commit_that_fails_and_says_how_the_branch_is_left() {
    let h = harness();
    let task = h.task("C-3");
    let base = in_worktree(&task, &["rev-parse", "HEAD"]);
    // The repository refuses some messages, as a commit-msg hook would.
    let hook = h.repo.path().join(".git").join("hooks").join("commit-msg");
    std::fs::write(&hook, "#!/bin/sh\nif grep -q WIP \"$1\"; then exit 1; fi\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    write_files(&task, &["a.txt", "b.txt", "c.txt"]);
    let asked = h
        .service
        .request_approval(commit_plan(
            &task,
            vec![
                commit("feat: a", &["a.txt"]),
                commit("WIP b", &["b.txt"]),
                commit("feat: c", &["c.txt"]),
            ],
            &[],
        ))
        .unwrap();
    let failed = h.service.decide_approval(&asked.id, true, None).unwrap();
    assert_eq!(failed.status, DeliveryApprovalStatus::Failed);
    let outcome = failed.outcome.unwrap();
    assert!(
        outcome.starts_with("El commit 2 de 3 («WIP b») falló"),
        "{outcome}"
    );
    assert!(
        outcome.contains("Los 1 anteriores quedaron hechos"),
        "{outcome}"
    );
    // The first commit stands; the rest is in the worktree, nothing staged.
    assert_eq!(failed.created.len(), 1);
    assert_eq!(
        in_worktree(&task, &["rev-parse", "HEAD"]),
        failed.created[0]
    );
    assert_eq!(changed(&task), ["b.txt", "c.txt"]);
    assert_eq!(in_worktree(&task, &["diff", "--cached", "--name-only"]), "");
    // And the part that was done can be taken back too.
    h.service.undo_commit(&asked.id).unwrap();
    assert_eq!(in_worktree(&task, &["rev-parse", "HEAD"]), base);
    assert_eq!(changed(&task), ["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn a_commit_plan_is_not_run_if_the_worktree_changed_since_it_was_prepared() {
    let h = harness();
    let task = h.task("C-4");
    let base = in_worktree(&task, &["rev-parse", "HEAD"]);
    write_files(&task, &["a.txt"]);
    let asked = h
        .service
        .request_approval(commit_plan(&task, vec![commit("feat: a", &["a.txt"])], &[]))
        .unwrap();
    write_files(&task, &["late.txt"]);
    let failed = h.service.decide_approval(&asked.id, true, None).unwrap();
    assert_eq!(failed.status, DeliveryApprovalStatus::Failed);
    let outcome = failed.outcome.unwrap();
    assert!(outcome.starts_with("No se creó ningún commit"), "{outcome}");
    assert!(outcome.contains("sin asignar: late.txt"), "{outcome}");
    assert!(failed.created.is_empty());
    assert_eq!(in_worktree(&task, &["rev-parse", "HEAD"]), base);
}

#[test]
fn a_commit_step_whose_commits_left_the_branch_goes_back_to_pending() {
    let h = harness();
    let task = h.task("C-5");
    write_files(&task, &["a.txt"]);
    // A step recorded before plans existed: only "Commit <hash>".
    let asked = h
        .service
        .request_approval(NewApproval {
            task_id: task.id.clone(),
            rung: DeliveryRung::Commit,
            title: "C-5: add a".into(),
            body: String::new(),
            requested_by: "user".into(),
            commits: Vec::new(),
            leave_out: Vec::new(),
            single_commit_reason: None,
            merge_method: None,
            stage: None,
            jira_statuses: Vec::new(),
            comment_url: None,
            resolve_thread: false,
        })
        .unwrap();
    let mut done = h.service.decide_approval(&asked.id, true, None).unwrap();
    assert_eq!(done.status, DeliveryApprovalStatus::Executed);
    assert_eq!(
        in_worktree(&task, &["log", "-1", "--format=%B"]),
        "C-5: add a"
    );
    done.created.clear();
    done.before = None;
    h.service.store().unwrap().put_approval(&done).unwrap();
    let latest = || h.service.approval(&asked.id).unwrap();
    h.service.overview().unwrap();
    assert_eq!(latest().status, DeliveryApprovalStatus::Executed);

    // Someone takes the commit back by hand.
    git(&task.worktree, &["reset", "-q", "--mixed", "HEAD~1"]);
    let overview = h.service.overview().unwrap();
    let step = overview
        .approvals
        .iter()
        .find(|a| a.id == asked.id)
        .unwrap();
    assert_eq!(step.status, DeliveryApprovalStatus::Undone);
    assert!(step
        .outcome
        .as_deref()
        .unwrap()
        .contains("ya no están en la rama"));
    assert_eq!(changed(&task), ["a.txt"]);

    // A commit reworded outside (amend, same content) still stands, on the
    // new commit: there is nothing to commit again.
    let asked = h
        .service
        .request_approval(commit_plan(&task, vec![commit("feat: a", &["a.txt"])], &[]))
        .unwrap();
    h.service.decide_approval(&asked.id, true, None).unwrap();
    git(
        &task.worktree,
        &["commit", "-q", "--amend", "-m", "feat: a, reworded"],
    );
    h.service.overview().unwrap();
    let reworded = h.service.approval(&asked.id).unwrap();
    assert_eq!(reworded.status, DeliveryApprovalStatus::Executed);
    assert_eq!(
        reworded.created,
        [in_worktree(&task, &["rev-parse", "HEAD"])]
    );

    // One rewritten with other content is gone, and nothing is pushed.
    std::fs::write(PathBuf::from(plain_path(&task.worktree)).join("b.txt"), "b").unwrap();
    git(&task.worktree, &["add", "b.txt"]);
    git(
        &task.worktree,
        &["commit", "-q", "--amend", "-m", "feat: a and b"],
    );
    let push = h
        .service
        .request_approval(NewApproval {
            task_id: task.id.clone(),
            rung: DeliveryRung::Push,
            title: "Publish".into(),
            body: String::new(),
            requested_by: "user".into(),
            commits: Vec::new(),
            leave_out: Vec::new(),
            single_commit_reason: None,
            merge_method: None,
            stage: None,
            jira_statuses: Vec::new(),
            comment_url: None,
            resolve_thread: false,
        })
        .unwrap_err();
    assert_eq!(push.category, "commit_missing");
    assert_eq!(
        h.service.approval(&asked.id).unwrap().status,
        DeliveryApprovalStatus::Undone
    );
}

/// `main` gains a rename and a new file, then the task's branch gets a
/// commit of its own touching `README.md` the way `main` does when
/// `conflict` is set.
fn diverge_from_main(h: &Harness, task: &DeliveryTask, conflict: bool) {
    let repo = h.repo.path();
    git(repo, &["mv", "README.md", "GUIDE.md"]);
    std::fs::write(repo.join("base.txt"), "base\n").unwrap();
    if conflict {
        std::fs::write(repo.join("shared.txt"), "main\n").unwrap();
    }
    git(repo, &["add", "--all"]);
    git(repo, &["commit", "-q", "-m", "main moves on"]);
    write_files(task, &["a.txt"]);
    if conflict {
        std::fs::write(task.worktree.join("shared.txt"), "task\n").unwrap();
    }
    git(&task.worktree, &["add", "--all"]);
    git(&task.worktree, &["commit", "-q", "-m", "task work"]);
}

/// Starts merging `main` into the task's branch without concluding it, as a
/// coordinator did in its shell; true when git stopped on conflicts.
fn start_merge(task: &DeliveryTask) -> bool {
    !std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-C"])
        .arg(&task.worktree)
        .args(["merge", "--no-ff", "--no-commit", "-q", "main"])
        .output()
        .unwrap()
        .status
        .success()
}

fn parents(task: &DeliveryTask) -> Vec<String> {
    in_worktree(task, &["rev-list", "--parents", "-1", "HEAD"])
        .split_whitespace()
        .skip(1)
        .map(str::to_string)
        .collect()
}

#[test]
fn a_commit_step_concludes_a_merge_in_progress_with_both_parents() {
    let h = harness();
    let task = h.task("M-1");
    diverge_from_main(&h, &task, false);
    let head = in_worktree(&task, &["rev-parse", "HEAD"]);
    let main = in_worktree(&task, &["rev-parse", "main"]);
    assert!(!start_merge(&task));
    // Both sides of the rename are changed files.
    assert_eq!(changed(&task), ["GUIDE.md", "README.md", "base.txt"]);

    // A merge is one commit with everything: neither a split plan nor files
    // left out.
    let refused = |plan: NewApproval| h.service.request_approval(plan).unwrap_err().message;
    assert!(refused(commit_plan(
        &task,
        vec![
            commit("merge a", &["GUIDE.md"]),
            commit("merge b", &["base.txt"])
        ],
        &[],
    ))
    .contains("un único commit de merge"));
    assert!(refused(commit_plan(
        &task,
        vec![commit("Merge main", &[])],
        &["base.txt"]
    ))
    .contains("no deja archivos fuera"));

    // Its paths are not needed, renames included.
    let asked = h
        .service
        .request_approval(commit_plan(
            &task,
            vec![commit("Merge branch 'main' into delivery/m-1", &[])],
            &[],
        ))
        .unwrap();
    assert_eq!(asked.merge_head.as_deref(), Some(main.as_str()));
    assert_eq!(asked.title, "Merge branch 'main' into delivery/m-1");
    let done = h.service.decide_approval(&asked.id, true, None).unwrap();
    assert_eq!(
        done.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        done.outcome
    );
    assert_eq!(parents(&task), [head.clone(), main.clone()]);
    assert_eq!(done.created, [in_worktree(&task, &["rev-parse", "HEAD"])]);
    assert_eq!(
        in_worktree(&task, &["rev-list", "--count", "HEAD..main"]),
        "0"
    );
    assert!(changed(&task).is_empty());
    assert!(task.worktree.join("GUIDE.md").exists());
    assert!(!task.worktree.join("README.md").exists());

    // Undone, the branch is back on its own commit.
    h.service.undo_commit(&asked.id).unwrap();
    assert_eq!(in_worktree(&task, &["rev-parse", "HEAD"]), head);
}

#[test]
fn a_merge_commit_waits_for_its_conflicts_and_a_plan_from_before_the_merge_is_not_run() {
    let h = harness();
    let task = h.task("M-2");
    diverge_from_main(&h, &task, true);
    let head = in_worktree(&task, &["rev-parse", "HEAD"]);
    let main = in_worktree(&task, &["rev-parse", "main"]);
    write_files(&task, &["late.txt"]);
    // Prepared before the merge started.
    let stale = h
        .service
        .request_approval(commit_plan(
            &task,
            vec![commit("feat: late", &["late.txt"])],
            &[],
        ))
        .unwrap();
    std::fs::remove_file(task.worktree.join("late.txt")).unwrap();
    assert!(start_merge(&task));
    let failed = h.service.decide_approval(&stale.id, true, None).unwrap();
    assert_eq!(failed.status, DeliveryApprovalStatus::Failed);
    assert!(failed
        .outcome
        .unwrap()
        .contains("hay un merge en curso que este plan no contemplaba"));
    // The merge is still there for the step that concludes it.
    assert!(super::tasks::merge_head(super::tasks::Place::Native, &task.worktree).is_some());

    let merge = || commit_plan(&task, vec![commit("Merge main", &[])], &[]);
    let error = h.service.request_approval(merge()).unwrap_err();
    assert_eq!(error.category, "merge_conflicts");
    assert!(error.message.contains("shared.txt"), "{error}");

    // The job resolves the file without staging it.
    std::fs::write(task.worktree.join("shared.txt"), "task and main\n").unwrap();
    let asked = h.service.request_approval(merge()).unwrap();
    let done = h.service.decide_approval(&asked.id, true, None).unwrap();
    assert_eq!(
        done.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        done.outcome
    );
    assert_eq!(parents(&task), [head, main]);
    assert_eq!(
        std::fs::read_to_string(task.worktree.join("shared.txt")).unwrap(),
        "task and main\n"
    );
    assert!(changed(&task).is_empty());
}

fn step(task: &DeliveryTask, rung: DeliveryRung, title: &str) -> NewApproval {
    NewApproval {
        task_id: task.id.clone(),
        rung,
        title: title.into(),
        body: String::new(),
        requested_by: "user".into(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: None,
        stage: None,
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    }
}

#[test]
fn a_sync_step_merges_the_fetched_base_and_the_push_checks_it_is_still_merged() {
    let h = harness();
    let origin = tempfile::tempdir().unwrap();
    git(origin.path(), &["init", "-q", "--bare", "-b", "main"]);
    let origin_path = plain_path(origin.path());
    git(h.repo.path(), &["remote", "add", "origin", &origin_path]);
    let task = h.task("S-1");
    diverge_from_main(&h, &task, false);
    // The base moves on in origin only: the sync has to fetch it.
    git(h.repo.path(), &["push", "-q", "origin", "main"]);
    git(
        h.repo.path(),
        &["update-ref", "refs/remotes/origin/main", "HEAD~1"],
    );
    let head = in_worktree(&task, &["rev-parse", "HEAD"]);
    let main = in_worktree(&task, &["rev-parse", "main"]);

    write_files(&task, &["wip.txt"]);
    let dirty = h
        .service
        .request_approval(step(&task, DeliveryRung::Sync, "Merge main"))
        .unwrap_err();
    assert_eq!(dirty.category, "worktree_dirty");
    std::fs::remove_file(task.worktree.join("wip.txt")).unwrap();

    let message = "Merge remote-tracking branch 'origin/main' into delivery/s-1";
    // A sync runs as it is asked: nobody approves it.
    let done = h
        .service
        .request_approval(step(&task, DeliveryRung::Sync, message))
        .unwrap();
    assert_eq!(
        done.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        done.outcome
    );
    assert!(done.conflicts.is_empty());
    assert_eq!(done.merge_head.as_deref(), Some(main.as_str()));
    assert_eq!(parents(&task), [head.clone(), main.clone()]);
    assert_eq!(done.created, [in_worktree(&task, &["rev-parse", "HEAD"])]);
    assert_eq!(in_worktree(&task, &["log", "-1", "--format=%B"]), message);
    assert!(changed(&task).is_empty());
    // Nothing more to bring in.
    assert_eq!(
        h.service
            .request_approval(step(&task, DeliveryRung::Sync, "Merge main"))
            .unwrap_err()
            .category,
        "base_merged"
    );

    let push = h
        .service
        .request_approval(step(&task, DeliveryRung::Push, "Publish"))
        .unwrap();
    let pushed = h.service.decide_approval(&push.id, true, None).unwrap();
    assert_eq!(
        pushed.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        pushed.outcome
    );

    // A branch rebuilt without the merge is not published.
    git(&task.worktree, &["reset", "-q", "--hard", &head]);
    write_files(&task, &["more.txt"]);
    git(&task.worktree, &["add", "--all"]);
    git(&task.worktree, &["commit", "-q", "-m", "more"]);
    let refused = h
        .service
        .request_approval(step(&task, DeliveryRung::Push, "Publish"))
        .unwrap_err();
    assert_eq!(refused.category, "base_unmerged", "{refused}");
}

#[test]
fn a_sync_with_conflicts_goes_back_to_implementation_and_a_commit_concludes_it() {
    let h = harness();
    let task = h.task("S-2");
    diverge_from_main(&h, &task, true);
    let head = in_worktree(&task, &["rev-parse", "HEAD"]);
    let main = in_worktree(&task, &["rev-parse", "main"]);
    // A sync runs as it is asked: nobody approves it.
    let done = h
        .service
        .request_approval(step(&task, DeliveryRung::Sync, "Merge main"))
        .unwrap();
    assert_eq!(
        done.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        done.outcome
    );
    assert_eq!(done.conflicts, ["shared.txt"]);
    assert!(done.created.is_empty());
    assert!(done.outcome.unwrap().contains("shared.txt"));
    // The merge stays in progress, with the base's rename in it.
    assert!(super::tasks::merge_head(super::tasks::Place::Native, &task.worktree).is_some());
    assert!(task.worktree.join("GUIDE.md").exists());
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(phase.next_stage.as_deref(), Some("implementation"));
    // Nothing is published while it lasts, and no second sync starts.
    for (rung, category) in [
        (DeliveryRung::Push, "merge_in_progress"),
        (DeliveryRung::Sync, "merge_in_progress"),
    ] {
        let error = h
            .service
            .request_approval(step(&task, rung, "x"))
            .unwrap_err();
        assert_eq!(error.category, category, "{error}");
    }

    // The implementation job is told what to resolve.
    let job = h.dispatch(&task, "implementation", "events=codex-ok");
    let conflicts = job.merge_conflicts.clone().unwrap();
    // Resolving a merge gets more time than a stage's usual hour.
    assert_eq!(job.timeout_minutes, 120);
    assert_eq!(conflicts.base, "main");
    assert_eq!(conflicts.files, ["shared.txt"]);
    let paths = super::adapters::JobPaths::in_dir(h.repo.path().join("job"));
    let prompt = super::adapters::job_prompt(&job, &task, &paths, false, &[]);
    assert!(
        prompt.contains("Tinto is merging main into this branch and the merge stopped on conflicts in: shared.txt"),
        "{prompt}"
    );
    h.wait(&job.id);
    // It resolves the file; the commit step concludes the merge.
    std::fs::write(task.worktree.join("shared.txt"), "task and main\n").unwrap();
    let commit = h
        .service
        .request_approval(commit_plan(&task, vec![commit("Merge main", &[])], &[]))
        .unwrap();
    let concluded = h.service.decide_approval(&commit.id, true, None).unwrap();
    assert_eq!(
        concluded.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        concluded.outcome
    );
    assert_eq!(parents(&task), [head, main]);
    assert!(h
        .service
        .request_approval(step(&task, DeliveryRung::Push, "Publish"))
        .is_ok());
}

#[test]
fn a_commit_whose_parents_alone_were_fixed_outside_tinto_stays_done() {
    let h = harness();
    let task = h.task("M-3");
    diverge_from_main(&h, &task, false);
    let main = in_worktree(&task, &["rev-parse", "main"]);
    // What happened before Tinto knew merges: the merge's changes committed
    // with one parent.
    git(&task.worktree, &["merge", "--squash", "-q", "main"]);
    let mut plan = commit_plan(
        &task,
        vec![commit("Merge main", &["GUIDE.md", "README.md", "base.txt"])],
        &[],
    );
    plan.single_commit_reason = Some("main's changes".into());
    let asked = h.service.request_approval(plan).unwrap();
    let done = h.service.decide_approval(&asked.id, true, None).unwrap();
    assert_eq!(done.status, DeliveryApprovalStatus::Executed);
    let single = done.created[0].clone();
    assert_eq!(parents(&task).len(), 1);

    // The coordinator restores the second parent by hand, same tree.
    let tree = format!("{single}^{{tree}}");
    let first = format!("{single}^");
    let fixed = in_worktree(
        &task,
        &[
            "commit-tree",
            &tree,
            "-p",
            &first,
            "-p",
            &main,
            "-m",
            "Merge main",
        ],
    );
    git(&task.worktree, &["reset", "-q", "--soft", &fixed]);
    h.service.overview().unwrap();
    let step = h.service.approval(&asked.id).unwrap();
    assert_eq!(step.status, DeliveryApprovalStatus::Executed);
    assert_eq!(step.created, std::slice::from_ref(&fixed));
    // Nothing stops the push.
    let push = h.service.request_approval(NewApproval {
        task_id: task.id.clone(),
        rung: DeliveryRung::Push,
        title: "Publish".into(),
        body: String::new(),
        requested_by: "user".into(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: None,
        stage: None,
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    });
    assert!(push.is_ok(), "{push:?}");

    // A different tree is a rewrite: the step is undone.
    let other = in_worktree(
        &task,
        &["commit-tree", "HEAD~1^{tree}", "-p", &first, "-m", "x"],
    );
    git(&task.worktree, &["reset", "-q", "--soft", &other]);
    h.service.overview().unwrap();
    assert_eq!(
        h.service.approval(&asked.id).unwrap().status,
        DeliveryApprovalStatus::Undone
    );
}

#[test]
fn a_coordinator_sends_a_commit_plan_over_mcp() {
    let h = harness();
    let call = |name: &str, arguments: serde_json::Value| {
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
        )
        .unwrap()["result"]
            .clone()
    };
    let repo = plain_path(h.repo.path());
    let run = call(
        "create_run",
        json!({"repo": repo, "title": "B", "owner": "coord"}),
    );
    let run_id = run["structuredContent"]["id"].as_str().unwrap().to_string();
    let task = call(
        "create_task",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "repo": repo, "key": "C-6", "title": "x"}),
    );
    let task_id = task["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let worktree = PathBuf::from(task["structuredContent"]["worktree"].as_str().unwrap());
    std::fs::write(worktree.join("a.txt"), "a").unwrap();
    std::fs::write(worktree.join("b.txt"), "b").unwrap();
    let request = |commits: serde_json::Value| {
        call(
            "request_approval",
            json!({"run_id": run_id, "owner": "coord", "generation": 1, "task_id": task_id, "rung": "commit", "commits": commits}),
        )
    };
    // The same validation as in the view.
    let bad = request(json!([{"message": "feat: a", "paths": ["a.txt", "nope.txt"]}]));
    assert_eq!(bad["isError"], true);
    let reason = bad["content"][0]["text"].as_str().unwrap();
    assert!(
        reason.contains("nope.txt (el commit 1) no existe"),
        "{reason}"
    );
    assert!(reason.contains("sin asignar: b.txt"), "{reason}");
    assert_eq!(request(json!([{"message": "feat: a"}]))["isError"], true);

    let asked = request(json!([
        {"message": "feat: a", "paths": ["a.txt"]},
        {"message": "feat: b", "paths": ["b.txt"]}
    ]));
    let approval = &asked["structuredContent"];
    assert_eq!(approval["requested_by"], "coord", "{asked}");
    assert_eq!(approval["commits"][1]["message"], "feat: b");
    let id = approval["id"].as_str().unwrap();
    h.service.decide_approval(id, true, None).unwrap();
    let read = call("read_approval", json!({"approval_id": id}));
    assert_eq!(read["structuredContent"]["status"], "executed");
    assert_eq!(
        read["structuredContent"]["created"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

/// GitHub as the PR watch sees it, with a counter of feed requests.
struct FakeGitHub {
    snapshot: std::sync::Mutex<serde_json::Value>,
    feed_calls: std::sync::atomic::AtomicUsize,
    mentioned: Vec<String>,
    merged: std::sync::Mutex<Vec<String>>,
    /// `<url> <logins>` of every review request.
    reviews: std::sync::Mutex<Vec<String>>,
    /// What `gh pr list --head <branch>` answers, for any branch.
    branch_prs: std::sync::Mutex<serde_json::Value>,
    /// `create <base> <head>`, `edit <url>`, `close <url>`, `draft <url>`.
    actions: std::sync::Mutex<Vec<String>>,
    /// What `gh api …/pulls/<n>/comments` answers.
    review_comments: std::sync::Mutex<serde_json::Value>,
    /// The repo's conversation comments and its PRs' review summaries.
    conversation: std::sync::Mutex<serde_json::Value>,
    review_bodies: std::sync::Mutex<serde_json::Value>,
}

impl FakeGitHub {
    fn new(snapshot: serde_json::Value, branch_prs: serde_json::Value) -> Arc<Self> {
        Arc::new(Self {
            snapshot: std::sync::Mutex::new(snapshot),
            feed_calls: std::sync::atomic::AtomicUsize::new(0),
            mentioned: Vec::new(),
            merged: std::sync::Mutex::new(Vec::new()),
            reviews: std::sync::Mutex::new(Vec::new()),
            branch_prs: std::sync::Mutex::new(branch_prs),
            actions: std::sync::Mutex::new(Vec::new()),
            review_comments: std::sync::Mutex::new(json!([])),
            conversation: std::sync::Mutex::new(json!([])),
            review_bodies: std::sync::Mutex::new(json!([])),
        })
    }
}

impl super::prs::GitHub for FakeGitHub {
    fn pr(&self, _url: &str) -> Result<serde_json::Value, super::DeliveryError> {
        Ok(self.snapshot.lock().unwrap().clone())
    }
    fn me(&self) -> Result<String, super::DeliveryError> {
        Ok("me".into())
    }
    fn branch_prs(
        &self,
        repo: &str,
        _branch: &str,
    ) -> Result<serde_json::Value, super::DeliveryError> {
        assert_eq!(repo, "o/r", "the repo comes from origin's URL");
        Ok(self.branch_prs.lock().unwrap().clone())
    }
    fn assignees(&self, _repo: &str) -> Result<Vec<String>, super::DeliveryError> {
        Ok(vec!["me".into(), "Luis".into(), "ana".into()])
    }
    fn request_reviewers(&self, url: &str, logins: &[String]) -> Result<(), super::DeliveryError> {
        self.reviews
            .lock()
            .unwrap()
            .push(format!("{url} {}", logins.join(",")));
        Ok(())
    }
    fn remove_reviewers(&self, url: &str, logins: &[String]) -> Result<(), super::DeliveryError> {
        self.reviews
            .lock()
            .unwrap()
            .push(format!("remove {url} {}", logins.join(",")));
        Ok(())
    }
    fn merge(
        &self,
        url: &str,
        method: &str,
        subject: &str,
        _body: &str,
        delete_branch: bool,
    ) -> Result<(), super::DeliveryError> {
        self.merged.lock().unwrap().push(match delete_branch {
            true => format!("{url} {method} {subject}"),
            false => format!("{url} {method} {subject} (branch kept)"),
        });
        let mut snapshot = self.snapshot.lock().unwrap();
        snapshot["state"] = json!("MERGED");
        Ok(())
    }
    fn create_pr(
        &self,
        repo: &str,
        base: Option<&str>,
        head: &str,
        title: &str,
        body: &str,
    ) -> Result<String, super::DeliveryError> {
        assert_eq!(repo, "o/r");
        self.actions
            .lock()
            .unwrap()
            .push(format!("create {} {head}", base.unwrap_or("-")));
        let mut snapshot = self.snapshot.lock().unwrap();
        snapshot["state"] = json!("OPEN");
        snapshot["title"] = json!(title);
        snapshot["body"] = json!(body);
        *self.branch_prs.lock().unwrap() = json!([open_pr(50, "OPEN")]);
        Ok("Creating pull request...\nhttps://github.com/o/r/pull/50".into())
    }
    fn edit_pr(&self, url: &str, title: &str, body: &str) -> Result<(), super::DeliveryError> {
        self.actions.lock().unwrap().push(format!("edit {url}"));
        let mut snapshot = self.snapshot.lock().unwrap();
        snapshot["title"] = json!(title);
        snapshot["body"] = json!(body);
        Ok(())
    }
    fn close_pr(&self, url: &str) -> Result<(), super::DeliveryError> {
        self.actions.lock().unwrap().push(format!("close {url}"));
        self.snapshot.lock().unwrap()["state"] = json!("CLOSED");
        Ok(())
    }
    fn set_pr_base(&self, url: &str, base: &str) -> Result<(), super::DeliveryError> {
        self.actions
            .lock()
            .unwrap()
            .push(format!("base {url} {base}"));
        self.snapshot.lock().unwrap()["baseRefName"] = json!(base);
        Ok(())
    }
    fn pr_review_comments(
        &self,
        repo: &str,
        number: u64,
    ) -> Result<serde_json::Value, super::DeliveryError> {
        assert_eq!((repo, number), ("o/r", 299));
        Ok(self.review_comments.lock().unwrap().clone())
    }
    fn repo_review_comments(
        &self,
        repo: &str,
        page: u32,
    ) -> Result<serde_json::Value, super::DeliveryError> {
        assert_eq!(repo, "o/r");
        if page > 1 {
            return Ok(json!([]));
        }
        let mut comments = self.review_comments.lock().unwrap().clone();
        comments.as_array_mut().unwrap().reverse();
        Ok(comments)
    }
    fn repo_conversation_comments(
        &self,
        _repo: &str,
        page: u32,
    ) -> Result<serde_json::Value, super::DeliveryError> {
        Ok(match page {
            1 => self.conversation.lock().unwrap().clone(),
            _ => json!([]),
        })
    }
    fn repo_review_bodies(&self, _repo: &str) -> Result<serde_json::Value, super::DeliveryError> {
        Ok(self.review_bodies.lock().unwrap().clone())
    }
    fn reply_to_review_comment(
        &self,
        repo: &str,
        number: u64,
        comment_id: u64,
        body: &str,
    ) -> Result<String, super::DeliveryError> {
        assert_eq!((repo, number), ("o/r", 299));
        self.actions
            .lock()
            .unwrap()
            .push(format!("reply {comment_id} {body}"));
        let mut comments = self.review_comments.lock().unwrap();
        let id = 9000 + comments.as_array().unwrap().len() as u64;
        let url = format!("https://github.com/o/r/pull/299#discussion_r{id}");
        comments.as_array_mut().unwrap().push(json!({
            "id": id, "in_reply_to_id": comment_id, "user": {"login": "me"},
            "body": body, "html_url": url
        }));
        Ok(url)
    }
    fn comment_on_pr(&self, url: &str, body: &str) -> Result<String, super::DeliveryError> {
        self.actions
            .lock()
            .unwrap()
            .push(format!("comment {url} {body}"));
        Ok(format!("{url}#issuecomment-1"))
    }
    fn resolve_review_thread(
        &self,
        _repo: &str,
        _number: u64,
        comment_id: u64,
    ) -> Result<(), super::DeliveryError> {
        self.actions
            .lock()
            .unwrap()
            .push(format!("resolve {comment_id}"));
        Ok(())
    }
    fn pr_to_draft(&self, url: &str) -> Result<(), super::DeliveryError> {
        self.actions.lock().unwrap().push(format!("draft {url}"));
        self.snapshot.lock().unwrap()["isDraft"] = json!(true);
        Ok(())
    }
    fn upload_file(
        &self,
        repo: &str,
        branch: &str,
        path: &str,
        _bytes: &[u8],
        _message: &str,
    ) -> Result<String, super::DeliveryError> {
        self.actions
            .lock()
            .unwrap()
            .push(format!("upload {repo} {branch} {path}"));
        Ok("c0ffee1".into())
    }
    fn delete_branch(&self, repo: &str, branch: &str) -> Result<(), super::DeliveryError> {
        self.actions
            .lock()
            .unwrap()
            .push(format!("delete-branch {repo} {branch}"));
        Ok(())
    }
    fn add_label(&self, url: &str, label: &str) -> Result<(), super::DeliveryError> {
        self.actions
            .lock()
            .unwrap()
            .push(format!("label {url} {label}"));
        self.snapshot.lock().unwrap()["labels"] = json!([{ "name": label }]);
        Ok(())
    }
    fn remove_label(&self, url: &str, label: &str) -> Result<(), super::DeliveryError> {
        self.actions
            .lock()
            .unwrap()
            .push(format!("unlabel {url} {label}"));
        self.snapshot.lock().unwrap()["labels"] = json!([]);
        Ok(())
    }
    fn feed(&self, _etag: Option<&str>) -> Result<super::prs::Feed, super::DeliveryError> {
        self.feed_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(super::prs::Feed::Changed {
            etag: Some("e1".into()),
            poll_seconds: 60,
            prs: self.mentioned.clone(),
        })
    }
}

#[test]
fn a_linked_pr_is_followed_from_the_feed_and_its_news_is_seen() {
    let h = harness();
    let task = h.task("P-1");
    let review = |login: &str, at: &str| json!({"author": {"login": login}, "state": "COMMENTED", "submittedAt": at});
    let github = std::sync::Arc::new(FakeGitHub {
        snapshot: std::sync::Mutex::new(json!({
            "state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "REVIEW_REQUIRED",
            "statusCheckRollup": [{"name": "ci", "status": "COMPLETED", "conclusion": "SUCCESS"}],
            "reviews": [review("bot", "2026-10-14T08:00:00Z")], "comments": []
        })),
        feed_calls: std::sync::atomic::AtomicUsize::new(0),
        mentioned: vec!["o/r#5".into()],
        merged: std::sync::Mutex::new(Vec::new()),
        reviews: std::sync::Mutex::new(Vec::new()),
        branch_prs: std::sync::Mutex::new(json!([])),
        actions: std::sync::Mutex::new(Vec::new()),
        review_comments: std::sync::Mutex::new(json!([])),
        conversation: std::sync::Mutex::new(json!([])),
        review_bodies: std::sync::Mutex::new(json!([])),
    });
    h.service.set_github(github.clone());

    // Nothing linked: the watch asks GitHub nothing.
    let mut etag = None;
    h.service.watch_prs_once(&mut etag);
    assert_eq!(
        github.feed_calls.load(std::sync::atomic::Ordering::SeqCst),
        0
    );

    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/5").unwrap();
    h.service.link_pr(&task.id, &reference).unwrap();
    let pr = h
        .service
        .refresh_pr(&task.id, Duration::ZERO)
        .unwrap()
        .unwrap();
    assert_eq!(pr.state, "open");
    assert_eq!(
        pr.new_reviews, 0,
        "what was there before linking is not news"
    );

    *github.snapshot.lock().unwrap() = json!({
        "state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "CHANGES_REQUESTED",
        "statusCheckRollup": [{"name": "ci", "status": "COMPLETED", "conclusion": "FAILURE"}],
        "reviews": [review("bot", "2026-10-14T08:00:00Z"), review("ana", "2026-10-14T09:00:00Z")],
        "comments": [{"author": {"login": "me"}, "createdAt": "2026-10-14T09:05:00Z"}]
    });
    // The feed mentions the PR: it is read again.
    h.service.watch_prs_once(&mut etag);
    assert_eq!(etag.as_deref(), Some("e1"));
    let pr = h.service.task_pr(&task.id).unwrap().unwrap();
    assert_eq!((pr.new_reviews, pr.new_comments), (1, 0));
    assert_eq!(pr.new_authors, vec!["ana".to_string()]);
    assert_eq!(pr.checks.failing, 1);
    assert_eq!(h.service.overview().unwrap().prs.len(), 1);

    // A stage on the task is the round that answers it.
    let job = h.dispatch(&task, "implementation", "events=codex-ok");
    let pr = h.service.task_pr(&task.id).unwrap().unwrap();
    assert_eq!(pr.new_reviews, 0);
    assert_eq!(pr.seen_until.as_deref(), Some("2026-10-14T09:00:00Z"));
    h.wait(&job.id);

    let read = super::mcp::handle_message(
        &h.service,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "read_pr", "arguments": {"task_id": task.id}}}),
    )
    .unwrap()["result"]["structuredContent"]
        .clone();
    assert_eq!(read["review_decision"], "changes_requested");
    assert_eq!(read["number"], 5);
}

#[test]
fn the_merge_step_merges_the_followed_pr_and_deletes_its_branch() {
    let h = harness();
    let task = h.task("P-2");
    let github = std::sync::Arc::new(FakeGitHub {
        snapshot: std::sync::Mutex::new(json!({
            "state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "",
            "statusCheckRollup": [{"name": "ci", "status": "COMPLETED", "conclusion": "FAILURE"}],
            "reviews": [], "comments": []
        })),
        feed_calls: std::sync::atomic::AtomicUsize::new(0),
        mentioned: Vec::new(),
        merged: std::sync::Mutex::new(Vec::new()),
        reviews: std::sync::Mutex::new(Vec::new()),
        branch_prs: std::sync::Mutex::new(json!([])),
        actions: std::sync::Mutex::new(Vec::new()),
        review_comments: std::sync::Mutex::new(json!([])),
        conversation: std::sync::Mutex::new(json!([])),
        review_bodies: std::sync::Mutex::new(json!([])),
    });
    h.service.set_github(github.clone());
    let merge = |method: Option<&str>| NewApproval {
        task_id: task.id.clone(),
        rung: DeliveryRung::Merge,
        title: "P-2: paginar (#5)".into(),
        body: String::new(),
        requested_by: "user".into(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: method.map(str::to_string),
        stage: None,
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    };
    assert_eq!(
        h.service
            .request_approval(merge(Some("squash")))
            .unwrap_err()
            .category,
        "pr_missing"
    );
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/5").unwrap();
    h.service.link_pr(&task.id, &reference).unwrap();
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    assert_eq!(
        h.service
            .request_approval(merge(None))
            .unwrap_err()
            .category,
        "invalid_approval"
    );
    assert_eq!(
        h.service
            .request_approval(merge(Some("squash")))
            .unwrap_err()
            .category,
        "pr_not_ready",
        "failing checks block the merge"
    );

    github.snapshot.lock().unwrap()["statusCheckRollup"] =
        json!([{"name": "ci", "status": "COMPLETED", "conclusion": "SUCCESS"}]);
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    assert_eq!(
        h.service
            .request_approval(merge(Some("squash")))
            .unwrap_err()
            .category,
        "pr_not_ready",
        "a PR nobody approved does not merge"
    );
    // Unless its repo merges without a review.
    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.pr_without_reviewer = true;
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    let asked = h.service.request_approval(merge(Some("squash"))).unwrap();
    assert_eq!(asked.merge_method.as_deref(), Some("squash"));
    // The user may reword the merge commit before approving.
    h.service
        .set_step_text(&asked.id, Some("P-2: paginate (#5)".into()), None, None)
        .unwrap();
    let done = h.service.decide_approval(&asked.id, true, None).unwrap();
    assert_eq!(
        done.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        done.outcome
    );
    assert!(done.outcome.unwrap().contains("fusionado (squash)"));
    assert_eq!(
        github.merged.lock().unwrap().as_slice(),
        ["https://github.com/o/r/pull/5 squash P-2: paginate (#5)".to_string()]
    );
    assert_eq!(
        h.service.task_pr(&task.id).unwrap().unwrap().state,
        "merged"
    );
}

#[test]
fn pr_steps_done_before_following_are_linked_when_the_watch_starts() {
    let h = harness();
    let task = h.task("P-3");
    let approval: super::model::DeliveryApproval = serde_json::from_value(json!({
        "id": "a-pr", "task_id": task.id, "rung": "pr", "title": "PR", "body": "",
        "status": "executed", "requested_by": "coord", "requested_at_ms": 1,
        "decided_at_ms": 2, "executed_at_ms": 3,
        "outcome": "PR abierto: https://github.com/o/r/pull/9."
    }))
    .unwrap();
    h.service.store().unwrap().put_approval(&approval).unwrap();
    assert_eq!(h.service.link_recorded_prs().unwrap(), 1);
    assert_eq!(h.service.task_pr(&task.id).unwrap().unwrap().number, 9);
    assert_eq!(h.service.link_recorded_prs().unwrap(), 0, "linked once");
}

/// A delivery step as an older Tinto (or the coordinator) left it.
fn put_step(h: &Harness, task: &DeliveryTask, id: &str, rung: &str, status: &str, outcome: &str) {
    let step: super::model::DeliveryApproval = serde_json::from_value(json!({
        "id": id, "task_id": task.id, "rung": rung, "title": rung, "body": "",
        "status": status, "requested_by": "coord", "requested_at_ms": 1,
        "decided_at_ms": 2, "executed_at_ms": 3, "outcome": outcome
    }))
    .unwrap();
    h.service.store().unwrap().put_approval(&step).unwrap();
}

/// `origin` on GitHub as `o/r`.
fn github_origin(h: &Harness) {
    git(
        h.repo.path(),
        &["remote", "add", "origin", "https://github.com/o/r.git"],
    );
}

fn open_pr(number: u64, state: &str) -> serde_json::Value {
    json!({"number": number, "url": format!("https://github.com/o/r/pull/{number}"), "state": state, "headRefOid": "abc"})
}

#[test]
fn a_pushed_task_without_a_pr_url_finds_its_pr_by_branch_at_startup() {
    let h = harness();
    github_origin(&h);
    let task = h.task("AGOS-455");
    put_step(&h, &task, "a-push", "push", "executed", "Publicado");
    put_step(&h, &task, "a-pr", "pr", "executed", "PR abierto y aprobado");
    let github = FakeGitHub::new(
        json!({"state": "OPEN"}),
        json!([
            open_pr(270, "CLOSED"),
            open_pr(279, "OPEN"),
            open_pr(281, "CLOSED")
        ]),
    );
    h.service.set_github(github);

    assert_eq!(h.service.link_recorded_prs().unwrap(), 1);
    let pr = h.service.task_pr(&task.id).unwrap().unwrap();
    assert_eq!(
        pr.number, 279,
        "the open PR wins over more recent closed ones"
    );
    let step = h.service.approval("a-pr").unwrap();
    assert_eq!(step.status, DeliveryApprovalStatus::Executed);
    assert_eq!(step.notice, None);
}

#[test]
fn the_pr_step_is_done_only_with_a_pr_tinto_can_follow() {
    let h = harness();
    github_origin(&h);
    let task = h.task("P-4");
    let github = FakeGitHub::new(
        json!({"state": "OPEN", "title": "pr", "body": ""}),
        json!([]),
    );
    h.service.set_github(github.clone());
    put_step(&h, &task, "a-pr", "pr", "approved", "");

    let error = h
        .service
        .complete_approval("a-pr", true, "abierto")
        .unwrap_err();
    assert_eq!(
        error.message,
        format!("No encuentro un PR de la rama {}. Pega su URL", task.branch)
    );
    assert!(h.service.task_pr(&task.id).unwrap().is_none());

    // Without a URL, the branch's PR is accepted.
    *github.branch_prs.lock().unwrap() = json!([open_pr(12, "OPEN")]);
    let done = h.service.complete_approval("a-pr", true, "").unwrap();
    assert_eq!(done.status, DeliveryApprovalStatus::Executed);
    assert_eq!(
        done.outcome.as_deref(),
        Some("https://github.com/o/r/pull/12")
    );
    assert_eq!(h.service.task_pr(&task.id).unwrap().unwrap().number, 12);
}

#[test]
fn the_branchs_pr_wins_over_a_different_recorded_url() {
    let h = harness();
    github_origin(&h);
    let task = h.task("P-5");
    h.service.set_github(FakeGitHub::new(
        json!({"state": "OPEN", "title": "pr", "body": ""}),
        json!([open_pr(7, "OPEN")]),
    ));
    put_step(&h, &task, "a-pr", "pr", "approved", "");
    let done = h
        .service
        .complete_approval("a-pr", true, "https://github.com/o/r/pull/3")
        .unwrap();
    assert_eq!(h.service.task_pr(&task.id).unwrap().unwrap().number, 7);
    assert_eq!(
        done.notice.as_deref(),
        Some("La URL anotada no es el PR de esta rama")
    );
    assert_eq!(
        done.outcome.as_deref(),
        Some("https://github.com/o/r/pull/3"),
        "what was recorded is kept"
    );
}

#[test]
fn a_pr_merged_outside_tinto_makes_the_merge_step_done_and_a_closed_one_drifts() {
    let h = harness();
    let task = h.task("P-6");
    put_step(
        &h,
        &task,
        "a-pr",
        "pr",
        "executed",
        "https://github.com/o/r/pull/8",
    );
    let github = FakeGitHub::new(
        json!({"state": "CLOSED", "mergedBy": null, "mergedAt": "", "mergeCommit": null}),
        json!([]),
    );
    h.service.set_github(github.clone());
    h.service.link_recorded_prs().unwrap();
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    let step = h.service.approval("a-pr").unwrap();
    assert_eq!(step.status, DeliveryApprovalStatus::Drifted);
    assert_eq!(
        step.notice.as_deref(),
        Some("El PR #8 se cerró sin fusionar")
    );

    *github.snapshot.lock().unwrap() = json!({
        "state": "MERGED", "mergedBy": {"login": "ana"},
        "mergedAt": "2026-10-14T09:30:00Z", "mergeCommit": {"oid": "1a2b3c4d5e6f"}
    });
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    let step = h.service.approval("a-pr").unwrap();
    assert_eq!(
        step.status,
        DeliveryApprovalStatus::Executed,
        "it matches again"
    );
    assert_eq!(step.notice, None);
    let merge = h
        .service
        .latest_approval(&task.id, DeliveryRung::Merge)
        .unwrap()
        .unwrap();
    assert_eq!(merge.status, DeliveryApprovalStatus::Executed);
    assert_eq!(merge.requested_by, "github");
    assert_eq!(
        merge.outcome.as_deref(),
        Some("Fusionado en GitHub por ana el 2026-10-14 (commit 1a2b3c4)")
    );
    // Read again: nothing new is recorded, and no merge is offered.
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    let merges = h
        .service
        .overview()
        .unwrap()
        .approvals
        .into_iter()
        .filter(|approval| approval.rung == DeliveryRung::Merge)
        .count();
    assert_eq!(merges, 1);
    let read = super::mcp::handle_message(
        &h.service,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "read_pr", "arguments": {"task_id": task.id}}}),
    )
    .unwrap()["result"]["structuredContent"]
        .clone();
    assert_eq!(read["merged_by"], "ana");
}

#[test]
fn a_push_drifts_when_the_remote_branch_is_gone_or_moved_and_is_asked_again() {
    let h = harness();
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare"]);
    git(
        h.repo.path(),
        &["remote", "add", "origin", &plain_path(remote.path())],
    );
    let task = h.task("P-7");
    let worktree = PathBuf::from(plain_path(&task.worktree));
    std::fs::write(worktree.join("feature.txt"), "x").unwrap();
    let request = |rung, title: &str| NewApproval {
        task_id: task.id.clone(),
        rung,
        title: title.to_string(),
        body: String::new(),
        requested_by: "user".to_string(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: None,
        stage: None,
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    };
    let commit = h
        .service
        .request_approval(request(DeliveryRung::Commit, "Add feature"))
        .unwrap();
    h.service.decide_approval(&commit.id, true, None).unwrap();
    let push = h
        .service
        .request_approval(request(DeliveryRung::Push, "Publish"))
        .unwrap();
    let push = h.service.decide_approval(&push.id, true, None).unwrap();
    assert_eq!(push.status, DeliveryApprovalStatus::Executed);
    assert!(push.tip.is_some());
    h.service.check_task(&task.id).unwrap();
    assert_eq!(
        h.service.approval(&push.id).unwrap().status,
        DeliveryApprovalStatus::Executed
    );

    let branch = format!("refs/heads/{}", task.branch);
    git(remote.path(), &["update-ref", "-d", &branch]);
    h.service.check_task(&task.id).unwrap();
    let drifted = h.service.approval(&push.id).unwrap();
    assert_eq!(drifted.status, DeliveryApprovalStatus::Drifted);
    assert_eq!(
        drifted.notice.as_deref(),
        Some("La rama remota ya no existe")
    );

    git(remote.path(), &["update-ref", &branch, &task.base_commit]);
    h.service.check_task(&task.id).unwrap();
    let drifted = h.service.approval(&push.id).unwrap();
    assert_eq!(drifted.status, DeliveryApprovalStatus::Drifted);
    assert_eq!(
        drifted.notice.as_deref(),
        Some("La rama remota cambió fuera de Tinto")
    );

    // "Volver a pedir el push": nothing new to publish, and it is allowed.
    let again = h
        .service
        .request_approval(request(DeliveryRung::Push, "Publish again"))
        .unwrap();
    let again = h.service.decide_approval(&again.id, true, None).unwrap();
    assert_eq!(
        again.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        again.outcome
    );
    h.service.check_task(&task.id).unwrap();
    assert_eq!(
        h.service.approval(&again.id).unwrap().status,
        DeliveryApprovalStatus::Executed
    );
}

#[test]
fn a_corrected_result_keeps_the_previous_one_needs_a_reason_and_links_the_pr() {
    let h = harness();
    let task = h.task("P-8");
    put_step(&h, &task, "a-push", "push", "executed", "Publicado");
    put_step(&h, &task, "a-pr", "pr", "executed", "PR abierto");
    put_step(
        &h,
        &task,
        "a-jira",
        "jira",
        "executed",
        "Movida a In Review",
    );

    let refused = |id: &str, reason: &str| {
        h.service
            .correct_approval(id, "https://github.com/o/r/pull/279", reason)
            .unwrap_err()
            .category
    };
    assert_eq!(refused("a-pr", "  "), "invalid_correction");
    assert_eq!(refused("a-push", "otra rama"), "invalid_correction");

    let corrected = h
        .service
        .correct_approval(
            "a-pr",
            "https://github.com/o/r/pull/279",
            "La URL estaba en el paso Jira",
        )
        .unwrap();
    assert_eq!(
        corrected.outcome.as_deref(),
        Some("https://github.com/o/r/pull/279")
    );
    assert_eq!(corrected.corrections.len(), 1);
    assert_eq!(
        corrected.corrections[0].previous.as_deref(),
        Some("PR abierto")
    );
    assert_eq!(
        corrected.corrections[0].reason,
        "La URL estaba en el paso Jira"
    );
    assert_eq!(h.service.task_pr(&task.id).unwrap().unwrap().number, 279);

    let jira = h
        .service
        .correct_approval(
            "a-jira",
            "Movida a In Review y comentada",
            "Faltaba el comentario",
        )
        .unwrap();
    assert_eq!(
        jira.corrections[0].previous.as_deref(),
        Some("Movida a In Review")
    );
}

#[test]
fn jira_is_asked_after_the_pr_and_after_the_merge_and_an_older_one_is_the_prs() {
    let h = harness();
    let task = h.task("AGOS-455");
    // One Jira step done before merge existed, as AGOS-455 has.
    put_step(
        &h,
        &task,
        "a-pr",
        "pr",
        "executed",
        "https://github.com/o/r/pull/279",
    );
    put_step(&h, &task, "a-jira", "jira", "executed", "In Review");
    h.service.stage_legacy_jira().unwrap();
    assert_eq!(
        h.service.approval("a-jira").unwrap().stage.as_deref(),
        Some("after_pr")
    );

    let jira = |stage: Option<&str>| NewApproval {
        task_id: task.id.clone(),
        rung: DeliveryRung::Jira,
        title: "Cerrar AGOS-455".into(),
        body: String::new(),
        requested_by: "user".into(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: None,
        stage: stage.map(str::to_string),
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    };
    let next = h.service.request_approval(jira(None)).unwrap();
    assert_eq!(
        next.stage.as_deref(),
        Some("after_merge"),
        "the one missing"
    );
    assert_eq!(
        h.service.request_approval(jira(None)).unwrap_err().category,
        "approval_exists"
    );
    assert_eq!(
        h.service
            .request_approval(jira(Some("later")))
            .unwrap_err()
            .category,
        "invalid_approval"
    );
}

#[test]
fn the_merge_method_proposed_follows_how_the_base_merges_its_prs() {
    let h = harness();
    let squashed = h.task("M-1");
    assert_eq!(h.service.merge_method(&squashed.id).unwrap(), "squash");

    let repo = h.repo.path();
    for number in 1..=2 {
        let branch = format!("feature-{number}");
        git(repo, &["checkout", "-q", "-b", &branch, "main"]);
        std::fs::write(repo.join(format!("f{number}.txt")), "x").unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-q", "-m", "feat: x"]);
        git(repo, &["checkout", "-q", "main"]);
        let message = format!("Merge pull request #{number} from o/{branch}");
        git(repo, &["merge", "-q", "--no-ff", "-m", &message, &branch]);
    }
    std::fs::write(repo.join("g.txt"), "x").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "fix: y (#3)"]);
    let merged = h.task("M-2");
    assert_eq!(h.service.merge_method(&merged.id).unwrap(), "merge");
}

#[test]
fn a_pr_step_done_without_a_push_in_tinto_still_finds_the_branchs_pr() {
    let h = harness();
    github_origin(&h);
    let task = h.task("P-9");
    put_step(&h, &task, "a-pr", "pr", "executed", "PR abierto");
    h.service.set_github(FakeGitHub::new(
        json!({"state": "OPEN"}),
        json!([open_pr(31, "OPEN")]),
    ));
    let pr = h.service.check_task(&task.id).unwrap().unwrap();
    assert_eq!(pr.number, 31);
}

#[test]
fn the_startup_check_of_every_task_finishes() {
    let h = harness();
    let task = h.task("P-10");
    put_step(&h, &task, "a-push", "push", "executed", "Publicado");
    put_step(
        &h,
        &task,
        "a-pr",
        "pr",
        "executed",
        "https://github.com/o/r/pull/4",
    );
    h.service
        .set_github(FakeGitHub::new(json!({"state": "MERGED"}), json!([])));
    let service = h.service.clone();
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || done.send(service.check_tasks()).unwrap());
    // A store lock held across the loop used to hang Tinto at startup.
    finished
        .recv_timeout(Duration::from_secs(30))
        .expect("the startup check hung")
        .unwrap();
    assert!(h.service.task_pr(&task.id).unwrap().is_some());
}

#[test]
fn the_user_asks_for_a_review_again_or_takes_a_request_back_on_the_open_pr() {
    let h = harness();
    let task = h.task("P-12");
    let github = FakeGitHub::new(
        json!({"state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "REVIEW_REQUIRED",
               "reviewRequests": [{"login": "Luis"}, {"name": "core-team"}],
               "statusCheckRollup": [], "reviews": [], "comments": []}),
        json!([]),
    );
    h.service.set_github(github.clone());
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/41").unwrap();
    h.service.link_pr(&task.id, &reference).unwrap();
    let pr = h
        .service
        .refresh_pr(&task.id, Duration::ZERO)
        .unwrap()
        .unwrap();
    assert_eq!(
        pr.requested_reviewers,
        ["Luis".to_string()],
        "teams are left out"
    );

    h.service
        .change_pr_reviewers(&task.id, vec!["@ana".into()], vec![" Luis ".into()])
        .unwrap();
    assert_eq!(
        github.reviews.lock().unwrap().as_slice(),
        [
            "remove https://github.com/o/r/pull/41 Luis".to_string(),
            "https://github.com/o/r/pull/41 ana".to_string()
        ]
    );

    github.snapshot.lock().unwrap()["state"] = json!("MERGED");
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    assert_eq!(
        h.service
            .change_pr_reviewers(&task.id, vec!["ana".into()], vec![])
            .unwrap_err()
            .category,
        "pr_not_open"
    );
}

#[test]
fn the_chosen_reviewers_are_asked_once_the_pr_is_linked_and_merging_needs_an_approval() {
    let h = harness();
    github_origin(&h);
    let task = h.task("P-11");
    let github = FakeGitHub::new(
        json!({"state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "",
               "statusCheckRollup": [], "reviews": [
                   {"author": {"login": "ana"}, "state": "COMMENTED", "submittedAt": "2026-10-14T08:00:00Z"}
               ], "comments": []}),
        json!([open_pr(40, "OPEN")]),
    );
    h.service.set_github(github.clone());
    assert_eq!(
        h.service.pr_reviewers(&task.id).unwrap(),
        vec!["ana".to_string(), "Luis".to_string()],
        "the user is not offered as their own reviewer"
    );

    put_step(&h, &task, "a-pr", "pr", "pending", "");
    h.service
        .set_pr_reviewers("a-pr", vec!["@ana".into(), " Luis ".into()])
        .unwrap();
    let done = h.service.decide_approval("a-pr", true, None).unwrap();
    assert_eq!(done.status, DeliveryApprovalStatus::Executed);
    assert_eq!(
        github.actions.lock().unwrap().as_slice(),
        ["edit https://github.com/o/r/pull/40".to_string()],
        "the branch's open PR gets the approved text"
    );
    assert!(done.reviewers_requested);
    assert_eq!(
        github.reviews.lock().unwrap().as_slice(),
        ["https://github.com/o/r/pull/40 ana,Luis".to_string()]
    );
    assert!(h
        .service
        .set_pr_reviewers("a-pr", vec!["ana".into()])
        .is_err());

    let merge = || NewApproval {
        task_id: task.id.clone(),
        rung: DeliveryRung::Merge,
        title: "Merge".into(),
        body: String::new(),
        requested_by: "user".into(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: Some("merge".into()),
        stage: None,
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    };
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    let refused = h.service.request_approval(merge()).unwrap_err();
    assert_eq!(refused.category, "pr_not_ready");
    assert!(
        refused
            .message
            .contains("le falta la aprobación de ana, Luis"),
        "{}",
        refused.message
    );

    // A repo without required reviews: its latest verdict counts, and every
    // reviewer chosen must approve.
    github.snapshot.lock().unwrap()["reviews"] = json!([
        {"author": {"login": "ana"}, "state": "CHANGES_REQUESTED", "submittedAt": "2026-10-14T08:00:00Z"},
        {"author": {"login": "ana"}, "state": "APPROVED", "submittedAt": "2026-10-14T09:00:00Z"}
    ]);
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    assert_eq!(
        h.service.task_pr(&task.id).unwrap().unwrap().approved_by,
        vec!["ana".to_string()]
    );
    let refused = h.service.request_approval(merge()).unwrap_err();
    assert!(
        refused.message.contains("aprobación de Luis"),
        "{}",
        refused.message
    );
    github.snapshot.lock().unwrap()["reviews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"author": {"login": "Luis"}, "state": "APPROVED", "submittedAt": "2026-10-14T10:00:00Z"}));
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    h.service.request_approval(merge()).unwrap();
}

/// Jira with one issue, its workflow, and a log of the writes.
struct FakeJira {
    status: std::sync::Mutex<String>,
    /// (from, transition id, transition name, to)
    workflow: Vec<(&'static str, &'static str, &'static str, &'static str)>,
    writes: std::sync::Mutex<Vec<String>>,
}

impl super::jira::Jira for FakeJira {
    fn call(
        &self,
        site: &super::jira::JiraSite,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, super::DeliveryError> {
        assert_eq!(site.host, "https://jira.test");
        assert_eq!(site.token, "secret");
        let mut status = self.status.lock().unwrap();
        match (method, path) {
            ("GET", "/rest/api/2/issue/AGOS-455?fields=status") => {
                Ok(json!({"fields": {"status": {"name": *status}}}))
            }
            ("GET", "/rest/api/2/issue/AGOS-455?fields=parent") => Ok(json!({
                "fields": {"parent": {"key": "AGOS-400", "fields": {"summary": "Facturación"}}}
            })),
            ("GET", "/rest/api/2/issue/AGOS-455/transitions") => Ok(json!({
                "transitions": self.workflow.iter()
                    .filter(|(from, ..)| *from == status.as_str())
                    .map(|(_, id, name, to)| json!({"id": id, "name": name, "to": {"name": to}}))
                    .collect::<Vec<_>>()
            })),
            ("POST", "/rest/api/2/issue/AGOS-455/transitions") => {
                let id = body.unwrap()["transition"]["id"].as_str().unwrap();
                let (_, _, _, to) = self
                    .workflow
                    .iter()
                    .find(|(from, tid, ..)| *from == status.as_str() && *tid == id)
                    .unwrap();
                *status = to.to_string();
                self.writes.lock().unwrap().push(format!("transition {id}"));
                Ok(serde_json::Value::Null)
            }
            ("POST", "/rest/api/2/issue/AGOS-455/remotelink") => {
                self.writes.lock().unwrap().push(format!(
                    "link {}",
                    body.unwrap()["globalId"].as_str().unwrap()
                ));
                Ok(json!({"id": 1}))
            }
            ("GET", path) if path.starts_with("/rest/api/2/issue/AGOS-455?fields=summary,") => {
                assert!(path.ends_with("&expand=renderedFields"), "{path}");
                Ok(json!({
                    "key": "AGOS-455",
                    "fields": {
                        "summary": "Exportar facturas",
                        "status": {"name": *status},
                        "issuetype": {"name": "Historia"},
                        "priority": {"name": "Alta"},
                        "assignee": {"displayName": "Ana"},
                        "reporter": null,
                        "labels": ["facturas"],
                        "parent": {"key": "AGOS-400", "fields": {"summary": "Facturación"}},
                        "issuelinks": [
                            {"type": {"inward": "is blocked by", "outward": "blocks"},
                             "inwardIssue": {"key": "AGOS-12", "fields": {"summary": "API", "status": {"name": "Cerrada"}}}}
                        ],
                        "comment": {"comments": [
                            {"author": {"displayName": "Luis"}, "created": "2026-10-20T10:00:00.000+0200", "body": "*hecho*"}
                        ]}
                    },
                    "renderedFields": {
                        "description": "<p>Exporta <b>CSV</b></p>",
                        "comment": {"comments": [{"body": "<p><b>hecho</b></p>"}]}
                    }
                }))
            }
            ("GET", path) if path.starts_with("/rest/api/2/issue/AGOS-404?") => Err(
                super::DeliveryError::new("jira_failed", "Jira respondió 404 Not Found"),
            ),
            other => panic!("unexpected Jira call {other:?}"),
        }
    }
}

#[test]
fn tinto_reads_a_tasks_issue_from_the_repos_jira() {
    let h = harness();
    let task = h.task("AGOS-455");
    let missing = h.task("AGOS-404");
    h.service.set_jira(Arc::new(FakeJira {
        status: std::sync::Mutex::new("En ejecución".into()),
        workflow: Vec::new(),
        writes: std::sync::Mutex::new(Vec::new()),
    }));

    let unset = h.service.issue(&task.id).unwrap_err();
    assert_eq!(unset.category, "jira_not_configured");

    let store = h.service.store().unwrap();
    let mut settings = store.repo_settings(h.repo.path()).unwrap();
    settings.jira = Some(super::model::DeliveryJiraSettings {
        host: "https://jira.test".into(),
        project: "AGOS".into(),
        after_pr: Vec::new(),
        after_merge: Vec::new(),
        has_token: false,
    });
    store.set_repo_settings(h.repo.path(), &settings).unwrap();
    drop(store);
    // A host without a token cannot read it either.
    assert_eq!(
        h.service.issue(&task.id).unwrap_err().category,
        "jira_not_configured"
    );
    h.service.set_jira_token(h.repo.path(), "secret").unwrap();

    // The list shows each task's parent: read once in the background; an
    // issue Jira refuses stays unread, to try again.
    h.service.read_parents().unwrap();
    let read = h.service.task(&task.id).unwrap();
    assert_eq!(read.parent.unwrap().key, "AGOS-400");
    assert!(read.parent_read_at_ms.is_some());
    assert_eq!(h.service.task(&missing.id).unwrap().parent_read_at_ms, None);

    let issue = h.service.issue(&task.id).unwrap();
    assert_eq!(
        serde_json::to_value(&issue).unwrap(),
        json!({
            "key": "AGOS-455",
            "url": "https://jira.test/browse/AGOS-455",
            "summary": "Exportar facturas",
            "status": "En ejecución",
            "type": "Historia",
            "priority": "Alta",
            "assignee": "Ana",
            "reporter": null,
            "labels": ["facturas"],
            "parent": {"key": "AGOS-400", "summary": "Facturación"},
            "links": [{"type": "is blocked by", "key": "AGOS-12", "summary": "API", "status": "Cerrada"}],
            "description_html": "<p>Exporta <b>CSV</b></p>",
            "comments": [{"author": "Luis", "created": "2026-10-20T10:00:00.000+0200", "body_html": "<p><b>hecho</b></p>"}]
        })
    );

    // What Jira refuses reaches the user as Jira said it.
    let refused = h.service.issue(&missing.id).unwrap_err();
    assert_eq!(refused.category, "jira_failed");
    assert!(refused.message.contains("404"), "{}", refused.message);
}

#[test]
fn tinto_runs_the_jira_steps_and_verifies_the_status_when_the_repo_has_jira() {
    let h = harness();
    let task = h.task("AGOS-455");
    let jira = Arc::new(FakeJira {
        status: std::sync::Mutex::new("Por hacer".into()),
        workflow: vec![
            ("Por hacer", "11", "Start Work", "En ejecución"),
            ("En ejecución", "151", "Submit for Review", "In Review"),
            ("In Review", "161", "Close", "Cerrada"),
        ],
        writes: std::sync::Mutex::new(Vec::new()),
    });
    h.service.set_jira(jira.clone());
    let jira_step = |stage: &str, statuses: Vec<String>| NewApproval {
        task_id: task.id.clone(),
        rung: DeliveryRung::Jira,
        title: "AGOS-455".into(),
        body: String::new(),
        requested_by: "user".into(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: None,
        stage: Some(stage.into()),
        jira_statuses: statuses,
        comment_url: None,
        resolve_thread: false,
    };

    // Without Jira in Tinto the coordinator does it, and must say what it did.
    let manual = h
        .service
        .request_approval(jira_step("after_pr", Vec::new()))
        .unwrap();
    assert!(manual.jira_statuses.is_empty());
    let manual = h.service.decide_approval(&manual.id, true, None).unwrap();
    assert_eq!(manual.status, DeliveryApprovalStatus::Approved);
    assert_eq!(
        h.service
            .complete_approval(&manual.id, true, " ")
            .unwrap_err()
            .category,
        "invalid_outcome"
    );
    h.service
        .complete_approval(&manual.id, false, "aún no")
        .unwrap();

    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.jira = Some(super::model::DeliveryJiraSettings {
        host: "https://jira.test".into(),
        project: "AGOS".into(),
        after_pr: vec!["En ejecución".into(), "In Review".into()],
        after_merge: vec!["Cerrada".into()],
        has_token: false,
    });
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    h.service.set_jira_token(h.repo.path(), "secret").unwrap();
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/279").unwrap();
    h.service.link_pr(&task.id, &reference).unwrap();

    let review = h
        .service
        .request_approval(jira_step("after_pr", Vec::new()))
        .unwrap();
    assert_eq!(
        review.jira_statuses,
        vec!["En ejecución", "In Review"],
        "the repo's"
    );
    let review = h.service.decide_approval(&review.id, true, None).unwrap();
    assert_eq!(
        review.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        review.outcome
    );
    assert_eq!(
        review.outcome.as_deref(),
        Some("Enlace al PR #279. Por hacer → En ejecución (Start Work). En ejecución → In Review (Submit for Review). Estado verificado en AGOS-455: In Review")
    );

    // A status the workflow cannot reach from here fails, saying so.
    let wrong = h
        .service
        .request_approval(jira_step("after_merge", vec!["Hecha".into()]))
        .unwrap();
    let wrong = h.service.decide_approval(&wrong.id, true, None).unwrap();
    assert_eq!(wrong.status, DeliveryApprovalStatus::Failed);
    let reason = wrong.outcome.unwrap();
    assert!(
        reason.contains("Desde «In Review» no hay transición a «Hecha» (se puede ir a: Cerrada)"),
        "{reason}"
    );
    assert!(reason.contains("No se hizo nada en Jira"), "{reason}");

    let close = h
        .service
        .request_approval(jira_step("after_merge", Vec::new()))
        .unwrap();
    let close = h.service.decide_approval(&close.id, true, None).unwrap();
    assert_eq!(
        close.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        close.outcome
    );
    assert_eq!(*jira.status.lock().unwrap(), "Cerrada");
    assert_eq!(
        jira.writes.lock().unwrap().as_slice(),
        [
            "link github-pr=https://github.com/o/r/pull/279",
            "transition 11",
            "transition 151",
            "transition 161"
        ]
    );
}

#[test]
fn a_delivered_task_is_archived_without_its_worktree_and_comes_back_from_its_pr() {
    let h = harness();
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare"]);
    git(
        h.repo.path(),
        &["remote", "add", "origin", &plain_path(remote.path())],
    );
    let task = h.task("P-12");
    let worktree = PathBuf::from(plain_path(&task.worktree));
    std::fs::write(worktree.join("feature.txt"), "x").unwrap();
    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-q", "-m", "feat: x"]);
    let head = super::tasks::head(super::tasks::Place::Native, &task.worktree).unwrap();
    // GitHub keeps the PR's head after its branch is deleted on merge.
    git(
        &worktree,
        &["push", "-q", "origin", "HEAD:refs/pull/5/head"],
    );
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/5").unwrap();
    h.service.link_pr(&task.id, &reference).unwrap();

    assert_eq!(
        h.service.archive_task(&task.id).unwrap_err().category,
        "task_not_delivered"
    );
    for (id, rung, stage) in [
        ("a1", "commit", None),
        ("a2", "push", None),
        ("a3", "pr", None),
        ("a4", "jira", Some("after_pr")),
        ("a5", "merge", None),
        ("a6", "jira", Some("after_merge")),
    ] {
        let step: super::model::DeliveryApproval = serde_json::from_value(json!({
            "id": id, "task_id": task.id, "rung": rung, "stage": stage, "title": rung,
            "body": "", "status": "executed", "requested_by": "user", "requested_at_ms": 1,
            "decided_at_ms": 2, "executed_at_ms": 3, "outcome": "hecho"
        }))
        .unwrap();
        h.service.store().unwrap().put_approval(&step).unwrap();
    }
    h.service.archive_task(&task.id).unwrap();
    assert!(!worktree.exists(), "the worktree goes");
    let branch = format!("refs/heads/{}", task.branch);
    assert!(
        super::tasks::git(
            super::tasks::Place::Native,
            h.repo.path(),
            &["rev-parse", "--verify", "--quiet", &branch]
        )
        .is_err(),
        "and the local branch"
    );
    assert!(h.service.overview().unwrap().tasks.is_empty());

    let found = |query: &str| {
        h.service
            .archived_tasks(query)
            .unwrap()
            .into_iter()
            .map(|archived| archived.task.key)
            .collect::<Vec<_>>()
    };
    assert_eq!(found("#5"), vec!["P-12"]);
    assert_eq!(found("p-12"), vec!["P-12"]);
    assert!(found("6").is_empty());

    let back = h.service.unarchive_task(&task.id).unwrap();
    assert_eq!(back.archived_at_ms, None);
    assert_eq!(
        super::tasks::head(super::tasks::Place::Native, &task.worktree).unwrap(),
        head,
        "the worktree is back at the PR's head"
    );
    assert_eq!(h.service.overview().unwrap().tasks.len(), 1);
    assert!(found("").is_empty());
}

#[test]
fn capacity_is_automatic_fixed_up_to_eight_or_unlimited() {
    use super::model::{auto_capacity_limit, UNLIMITED_CAPACITY};
    // One per physical core (half the logical ones), less one for checks.
    assert_eq!(auto_capacity_limit(8), 3);
    assert_eq!(auto_capacity_limit(32), 8);
    assert_eq!(auto_capacity_limit(2), 2);
    let limit = auto_capacity_limit(std::thread::available_parallelism().unwrap().get());
    let automatic = DeliverySettings { capacity: 0 };
    assert_eq!(automatic.effective_capacity(0), 1);
    assert_eq!(automatic.effective_capacity(2), 2);
    assert_eq!(automatic.effective_capacity(12), limit);
    assert_eq!(DeliverySettings { capacity: 5 }.effective_capacity(1), 5);

    let h = harness();
    h.service.update_settings(automatic).unwrap();
    for key in ["C-1", "C-2", "C-3", "C-4"] {
        h.task(key);
    }
    let overview = h.service.overview().unwrap();
    assert_eq!(
        (overview.settings.capacity, overview.capacity),
        (0, 4.min(limit))
    );

    for (asked, kept) in [(20, 8), (UNLIMITED_CAPACITY, UNLIMITED_CAPACITY)] {
        h.service
            .update_settings(DeliverySettings { capacity: asked })
            .unwrap();
        let overview = h.service.overview().unwrap();
        assert_eq!(
            (overview.settings.capacity, overview.capacity),
            (kept, kept)
        );
    }
}

#[test]
fn every_tool_result_is_an_object_for_mcp_clients() {
    let h = harness();
    let task = h.task("S-1");
    let call = |name: &str| {
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": {"task_id": task.id}}}),
        )
        .unwrap()["result"]["structuredContent"]
            .clone()
    };
    // A list and a null go under `result`; objects stay as they are.
    assert_eq!(call("read_decisions"), json!({"result": []}));
    assert_eq!(call("read_pr"), json!({"result": null}));
    assert!(call("read_settings")["repo"].is_string());
}

#[test]
fn the_coordinators_own_calls_do_not_block_and_the_user_can_change_them() {
    let h = harness();
    let task = h.task("D-9");
    let mut own = choice("¿Formato del manifiesto?", Some(0));
    own.blocking = false;
    let mut no_recommendation = choice("¿Formato?", None);
    no_recommendation.blocking = false;
    let mut permission = permission("agentos skills install");
    permission.blocking = false;
    for refused in [no_recommendation, permission] {
        assert_eq!(
            h.service
                .request_decisions(&task.id, vec![refused], "coord")
                .unwrap_err()
                .category,
            "invalid_decision"
        );
    }
    let asked = h
        .service
        .request_decisions(
            &task.id,
            vec![own, choice("¿Mostrar el aviso?", Some(1))],
            "coord",
        )
        .unwrap();
    let (own, users) = (&asked[0], &asked[1]);
    assert_eq!(own.status, super::model::DeliveryDecisionStatus::Answered);
    assert_eq!(own.answer.as_deref(), Some("Sí"), "its recommendation");
    assert_eq!(own.decided_by.as_deref(), Some("coord"));
    assert_eq!(users.status, super::model::DeliveryDecisionStatus::Pending);

    // Only the user's question holds the stages.
    h.service.answer_decision(&users.id, "No", "user").unwrap();
    let job = h.dispatch(&task, "tests", "events=codex-ok");
    h.wait(&job.id);

    let changed = h.service.change_decision(&own.id, "No").unwrap();
    assert_eq!(
        (changed.answer.as_deref(), changed.decided_by.as_deref()),
        (Some("No"), Some("user"))
    );
    assert_eq!(
        h.service
            .change_decision(&users.id, "Sí")
            .unwrap_err()
            .category,
        "decision_answered",
        "the user's own answers stay final"
    );
    // Besides its options, a choice takes the user's own words; not nothing.
    let own_words = h
        .service
        .change_decision(&own.id, "Solo en Claude Code, por ahora")
        .unwrap();
    assert_eq!(
        own_words.answer.as_deref(),
        Some("Solo en Claude Code, por ahora")
    );
    assert!(h.service.change_decision(&own.id, "  ").is_err());
}

#[test]
fn after_the_brainstorm_decisions_settle_as_they_are_asked() {
    let h = harness();
    let call = |name: &str, arguments: serde_json::Value| {
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
        )
        .unwrap()["result"]
            .clone()
    };
    let repo = plain_path(h.repo.path());
    let run = call(
        "create_run",
        json!({"repo": repo, "title": "B", "owner": "coord"}),
    );
    let run_id = run["structuredContent"]["id"].as_str().unwrap().to_string();
    let task = call(
        "create_task",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "repo": repo, "key": "D-5", "title": "x"}),
    );
    let task_id = task["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let fenced = |extra: serde_json::Value| {
        let mut args =
            json!({"run_id": run_id, "owner": "coord", "generation": 1, "task_id": task_id});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        args
    };
    let ask = |decisions: serde_json::Value| {
        call("request_decisions", fenced(json!({"decisions": decisions})))
    };
    let answers = |result: serde_json::Value| {
        result["structuredContent"]["result"]
            .as_array()
            .unwrap()
            .iter()
            .map(|decision| {
                (
                    decision["answer"].as_str().map(str::to_string),
                    decision["decided_by"].as_str().map(str::to_string),
                )
            })
            .collect::<Vec<_>>()
    };
    let recommended = json!({"kind": "choice", "question": "¿Aviso?", "options": [
        {"label": "Sí", "consequence": "a"},
        {"label": "No", "consequence": "b", "recommended": true}]});
    let unrecommended = json!({"kind": "choice", "question": "¿Sin recomendación?", "options": [
        {"label": "A", "consequence": "a"}, {"label": "B", "consequence": "b"}]});
    let permission = json!({"kind": "permission", "question": "¿Instalar?", "undo": "desinstalar", "command": "agentos skills install"});

    // During the brainstorm the user answers, and it cannot end before.
    let asked = ask(json!([recommended, unrecommended]));
    assert_eq!(answers(asked.clone()), vec![(None, None), (None, None)]);
    let open = call("end_brainstorm", fenced(json!({})));
    assert_eq!(open["isError"], true);
    assert!(open["content"][0]["text"].as_str().unwrap().contains("D-5"));
    for decision in asked["structuredContent"]["result"].as_array().unwrap() {
        h.service
            .answer_decision(decision["id"].as_str().unwrap(), "Sí", "user")
            .or_else(|_| {
                h.service
                    .answer_decision(decision["id"].as_str().unwrap(), "A", "user")
            })
            .unwrap();
    }
    // A settings proposal also keeps it open until the user decides it.
    let propose = |command: &str| {
        call(
            "propose_settings",
            fenced(json!({"reason": "tests", "checks_add": [command]})),
        )["structuredContent"]
            .clone()
    };
    let proposed = propose("npm test");
    assert_eq!(proposed["status"], "pending", "{proposed}");
    assert_eq!(call("end_brainstorm", fenced(json!({})))["isError"], true);
    h.service
        .reject_settings_proposal(proposed["id"].as_str().unwrap(), "user")
        .unwrap();
    let ended = call("end_brainstorm", fenced(json!({})));
    assert_eq!(ended["structuredContent"]["unattended"], true, "{ended}");
    let approved = propose("npm run lint");
    assert_eq!(approved["status"], "approved", "{approved}");
    assert_eq!(approved["decided_by"], super::decisions::AUTO);

    // From then on nothing waits for the user before the PR.
    let refused = ask(json!([recommended, unrecommended]));
    assert_eq!(refused["isError"], true);
    assert!(refused["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("recomienda una opción"));
    let auto = Some(super::decisions::AUTO.to_string());
    assert_eq!(
        answers(ask(json!([
            recommended,
            {"kind": "text", "question": "Mensaje", "text": "Hecho."},
            permission
        ]))),
        vec![
            (Some("No".into()), auto.clone()),
            (Some("Hecho.".into()), auto.clone()),
            (Some(super::decisions::ALLOWED.into()), auto.clone()),
        ]
    );
    let worktree = PathBuf::from(task["structuredContent"]["worktree"].as_str().unwrap());
    std::fs::write(worktree.join("a.txt"), "a").unwrap();
    let commit = call(
        "request_approval",
        fenced(json!({"rung": "commit", "commits": [{"message": "feat: a", "paths": ["a.txt"]}]})),
    );
    assert_eq!(
        commit["structuredContent"]["status"], "executed",
        "{commit}"
    );

    // A repo can keep any step for the user, even past the brainstorm.
    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.autonomy.permissions = false;
    settings.autonomy.commit = false;
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    assert_eq!(answers(ask(json!([permission]))), vec![(None, None)]);
    std::fs::write(worktree.join("b.txt"), "b").unwrap();
    let held = call(
        "request_approval",
        fenced(json!({"rung": "commit", "commits": [{"message": "feat: b", "paths": ["b.txt"]}]})),
    );
    assert_eq!(held["structuredContent"]["status"], "pending", "{held}");
}

#[test]
fn a_coordinator_accepts_the_recommended_decisions_but_never_a_permission() {
    let h = harness();
    let call = |name: &str, arguments: serde_json::Value| {
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
        )
        .unwrap()["result"]
            .clone()
    };
    let repo = plain_path(h.repo.path());
    let run = call(
        "create_run",
        json!({"repo": repo, "title": "B", "owner": "coord"}),
    );
    let run_id = run["structuredContent"]["id"].as_str().unwrap().to_string();
    let task = call(
        "create_task",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "repo": repo, "key": "D-4", "title": "x"}),
    );
    let task_id = task["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let fenced = |extra: serde_json::Value| {
        let mut args =
            json!({"run_id": run_id, "owner": "coord", "generation": 1, "task_id": task_id});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        args
    };
    call(
        "request_decisions",
        fenced(json!({"decisions": [
            {"kind": "choice", "question": "¿Aviso?", "options": [
                {"label": "Sí", "consequence": "a", "recommended": true},
                {"label": "No", "consequence": "b"}]},
            {"kind": "choice", "question": "¿Sin recomendación?", "options": [
                {"label": "A", "consequence": "a"}, {"label": "B", "consequence": "b"}]},
            {"kind": "text", "question": "Mensaje", "text": "Hecho."},
            {"kind": "permission", "question": "¿Instalar?", "undo": "desinstalar", "command": "agentos skills install"}
        ]})),
    );
    let accepted = call("accept_recommended", fenced(json!({})));
    let accepted = accepted["structuredContent"]["result"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        accepted.len(),
        2,
        "the choice with a recommendation and the text"
    );
    assert!(accepted
        .iter()
        .all(|decision| decision["decided_by"] == "coord"));
    let pending: Vec<String> = h
        .service
        .task_decisions(&task_id)
        .unwrap()
        .into_iter()
        .filter(|decision| decision.status == super::model::DeliveryDecisionStatus::Pending)
        .map(|decision| decision.question)
        .collect();
    assert_eq!(pending, vec!["¿Sin recomendación?", "¿Instalar?"]);
    // A replaced coordinator cannot do it.
    let stale = call(
        "accept_recommended",
        json!({"run_id": run_id, "owner": "other", "generation": 1, "task_id": task_id}),
    );
    assert_eq!(stale["isError"], true);
}

#[test]
fn a_coordinator_proposes_settings_and_the_next_job_runs_what_the_user_approved() {
    let h = harness();
    let call = |id: u64, name: &str, arguments: serde_json::Value| {
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
        )
        .unwrap()["result"]
            .clone()
    };
    let repo = plain_path(h.repo.path());
    let run = call(
        1,
        "create_run",
        json!({"repo": repo, "title": "B", "owner": "coord"}),
    );
    let run_id = run["structuredContent"]["id"].as_str().unwrap().to_string();
    let task = call(
        2,
        "create_task",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "repo": repo, "key": "AGOS-474", "title": "x"}),
    );
    let task_id = task["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let commands = [
        "npm --prefix services/memories test",
        "npm --prefix services/workflows test",
        "npm --prefix services/session-executor test",
        "npm --prefix services/rag test",
        "npm --prefix api-gateway test",
        "npm --prefix frontend test",
        "npm --prefix frontend run preflight",
    ];
    let reason = "AGOS-474 necesita correr los tests de los servicios y del frontend";

    // A replaced coordinator cannot propose.
    let stale = call(
        3,
        "propose_settings",
        json!({"run_id": run_id, "owner": "coord", "generation": 0, "task_id": task_id, "reason": reason, "checks_add": commands}),
    );
    assert_eq!(stale["isError"], true, "{stale}");
    assert!(stale.to_string().contains("fenced"), "{stale}");

    // An invalid item refuses the whole proposal and is named.
    let bad = call(
        4,
        "propose_settings",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "task_id": task_id, "reason": reason, "checks_add": ["npm --prefix frontend test", "bash scripts/qa.sh"]}),
    );
    assert_eq!(bad["isError"], true, "{bad}");
    assert!(bad.to_string().contains("scripts/qa.sh"), "{bad}");

    let proposed = call(
        5,
        "propose_settings",
        json!({"run_id": run_id, "owner": "coord", "generation": 1, "repo": repo, "reason": reason, "checks_add": commands}),
    );
    let proposal = &proposed["structuredContent"];
    assert_eq!(proposal["status"], "pending", "{proposed}");
    assert_eq!(proposal["task_id"], task_id.as_str());
    let proposal_id = proposal["id"].as_str().unwrap().to_string();
    assert_eq!(h.service.overview().unwrap().settings_proposals.len(), 1);
    let before = call(6, "read_settings", json!({"task_id": task_id}));
    assert_eq!(
        before["structuredContent"]["proposals"]["pending"][0]["id"],
        proposal_id.as_str()
    );
    assert_eq!(
        before["structuredContent"]["checks"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // The user approves all but one.
    let accepted = DeliverySettingsChanges {
        checks_add: commands[..6].iter().map(|item| item.to_string()).collect(),
        ..Default::default()
    };
    h.service
        .approve_settings_proposal(&proposal_id, accepted.clone(), "user")
        .unwrap();
    let again = h
        .service
        .approve_settings_proposal(&proposal_id, accepted, "user")
        .unwrap_err();
    assert_eq!(again.category, "settings_proposal_decided");
    assert!(h.service.overview().unwrap().settings_proposals.is_empty());

    let after = call(7, "read_settings", json!({"task_id": task_id}));
    let settings = &after["structuredContent"];
    assert_eq!(settings["checks"].as_array().unwrap().len(), 6, "{after}");
    let recent = &settings["proposals"]["recent"][0];
    assert_eq!(recent["status"], "approved");
    assert_eq!(recent["decided_by"], "user");
    assert_eq!(
        recent["approved"]["checks_add"].as_array().unwrap().len(),
        6
    );

    let job = h
        .service
        .dispatch(NewJob {
            task_id: task_id.clone(),
            role: "tests".into(),
            agent: DeliveryAgent::Claude,
            model: None,
            access: DeliveryAccess::Workspace,
            prompt: "events=claude-ok".into(),
            writes: None,
            lease: None,
            timeout_minutes: None,
            reasoning: None,
            platform: None,
            read_dirs: Vec::new(),
            stage_default: false,
            needs_environment: false,
        })
        .unwrap();
    assert!(job
        .allowed_commands
        .contains(&"npm --prefix services/memories test".to_string()));
    assert!(!job
        .allowed_commands
        .contains(&"npm --prefix frontend run preflight".to_string()));
    h.wait(&job.id);
}

#[test]
fn the_user_can_reject_a_settings_proposal() {
    let h = harness();
    let run = h
        .service
        .create_run(h.repo.path().to_path_buf(), "B", Some("coord".into()), None)
        .unwrap();
    let mut task = h.task("K-1");
    task.run_id = Some(run.id.clone());
    let proposal = h
        .service
        .propose_settings(
            &run.id,
            &task,
            DeliverySettingsChanges {
                qa_browser: Some(true),
                ..Default::default()
            },
            "La QA necesita navegador",
            "coord",
        )
        .unwrap();
    let rejected = h
        .service
        .reject_settings_proposal(&proposal.id, "user")
        .unwrap();
    assert_eq!(rejected.status, DeliverySettingsProposalStatus::Rejected);
    assert!(
        !h.service
            .store()
            .unwrap()
            .repo_settings(h.repo.path())
            .unwrap()
            .qa_browser
    );
}

/// A coordinator over MCP, on a run it holds as `coord`, generation 1.
struct Coordinator<'a> {
    h: &'a Harness,
    run_id: String,
}

impl<'a> Coordinator<'a> {
    fn start(h: &'a Harness) -> Self {
        let mut coordinator = Self {
            h,
            run_id: String::new(),
        };
        let repo = plain_path(h.repo.path());
        let run = coordinator.call(
            "create_run",
            json!({"repo": repo, "title": "B", "owner": "coord"}),
        );
        coordinator.run_id = run["structuredContent"]["id"].as_str().unwrap().to_string();
        coordinator
    }

    fn call(&self, name: &str, arguments: serde_json::Value) -> serde_json::Value {
        super::mcp::handle_message(
            &self.h.service,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
        )
        .unwrap()["result"]
            .clone()
    }

    /// A tool call on the run, fenced as its owner.
    fn write(&self, name: &str, mut arguments: serde_json::Value) -> serde_json::Value {
        arguments["run_id"] = json!(self.run_id);
        arguments["owner"] = json!("coord");
        arguments["generation"] = json!(1);
        self.call(name, arguments)
    }

    /// A task of the run and its worktree.
    fn task(&self, key: &str) -> (String, PathBuf) {
        let repo = plain_path(self.h.repo.path());
        let task = self.write(
            "create_task",
            json!({"repo": repo, "key": key, "title": "x"}),
        );
        (
            task["structuredContent"]["id"]
                .as_str()
                .unwrap()
                .to_string(),
            PathBuf::from(task["structuredContent"]["worktree"].as_str().unwrap()),
        )
    }
}

fn error_text(result: &serde_json::Value) -> String {
    assert_eq!(result["isError"], true, "{result}");
    result["content"][0]["text"].as_str().unwrap().to_string()
}

#[test]
fn a_pending_approval_can_be_withdrawn_with_a_reason_but_not_a_decided_one() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, worktree) = coordinator.task("W-1");
    std::fs::write(worktree.join("a.txt"), "a").unwrap();
    let request = |message: &str| {
        coordinator.write(
            "request_approval",
            json!({"task_id": task_id, "rung": "commit", "commits": [{"message": message, "paths": ["a.txt"]}]}),
        )
    };
    let first = request("feat: a, badly worded");
    let first_id = first["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(error_text(&request("feat: a")).contains("ya hay una aprobación abierta"));

    let no_reason = coordinator.write(
        "withdraw_approval",
        json!({"approval_id": first_id, "reason": " "}),
    );
    assert!(error_text(&no_reason).contains("falta reason"));
    let withdrawn = coordinator.write(
        "withdraw_approval",
        json!({"approval_id": first_id, "reason": "the message did not follow the template"}),
    );
    assert_eq!(
        withdrawn["structuredContent"]["withdrawn"],
        first_id.as_str()
    );
    assert!(h.service.approval(&first_id).is_err());
    assert_eq!(
        h.service
            .store()
            .unwrap()
            .event_details("approval_withdrawn")
            .unwrap(),
        ["commit: the message did not follow the template".to_string()]
    );

    // The corrected request takes its place; once decided it cannot go.
    let second = request("feat: a");
    let second_id = second["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    h.service.decide_approval(&second_id, true, None).unwrap();
    let refused = coordinator.write(
        "withdraw_approval",
        json!({"approval_id": second_id, "reason": "too late"}),
    );
    assert!(error_text(&refused).contains("ya tiene una decisión del usuario"));
}

#[test]
fn a_commit_step_needs_a_plan_and_one_commit_across_areas_says_why() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, worktree) = coordinator.task("S-1");
    std::fs::create_dir_all(worktree.join("src")).unwrap();
    std::fs::create_dir_all(worktree.join("tests")).unwrap();
    std::fs::write(worktree.join("src/a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(worktree.join("tests/a.rs"), "#[test]\nfn a() {}\n").unwrap();
    std::fs::write(worktree.join("NOTES.md"), "# a\n").unwrap();
    let paths = json!(["src/a.rs", "tests/a.rs", "NOTES.md"]);
    let request = |extra: serde_json::Value| {
        let mut arguments = json!({"task_id": task_id, "rung": "commit", "title": "feat: a"});
        for (key, value) in extra.as_object().unwrap() {
            arguments[key] = value.clone();
        }
        coordinator.write("request_approval", arguments)
    };

    let unplanned = error_text(&request(json!({})));
    assert!(
        unplanned.contains("necesita su plan en commits"),
        "{unplanned}"
    );

    let one = json!([{"message": "feat: a", "paths": paths}]);
    let unjustified = error_text(&request(json!({ "commits": one })));
    assert!(
        unjustified.contains("mezcla code, docs, tests"),
        "{unjustified}"
    );
    assert!(
        unjustified.contains("single_commit_reason"),
        "{unjustified}"
    );
    assert!(unjustified.contains("commits atómicos"), "{unjustified}");

    let justified = request(
        json!({"commits": one, "single_commit_reason": "the doc only describes this test"}),
    );
    let approval = &justified["structuredContent"];
    assert_eq!(
        approval["single_commit_reason"], "the doc only describes this test",
        "{justified}"
    );
    assert_eq!(approval["status"], "pending");
}

#[test]
fn a_small_single_commit_in_one_area_needs_no_reason_and_the_limits_are_the_repos() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, worktree) = coordinator.task("S-2");
    let lines: String = (0..12).map(|line| format!("line {line}\n")).collect();
    std::fs::write(worktree.join("a.rs"), &lines).unwrap();
    std::fs::write(worktree.join("b.rs"), "b\n").unwrap();
    let request = || {
        coordinator.write(
            "request_approval",
            json!({"task_id": task_id, "rung": "commit", "commits": [{"message": "feat: a", "paths": ["a.rs", "b.rs"]}]}),
        )
    };
    let asked = request();
    let id = asked["structuredContent"]["id"]
        .as_str()
        .expect("accepted")
        .to_string();
    h.service
        .withdraw_approval(&id, "try the repo's limit")
        .unwrap();

    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.single_commit_max_lines = Some(10);
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    let refused = error_text(&request());
    assert!(
        refused.contains("cambia 13 líneas (más de 10)"),
        "{refused}"
    );
}

#[test]
fn approving_the_pr_step_opens_the_pr_with_the_approved_text() {
    let h = harness();
    github_origin(&h);
    let task = h.task("O-1");
    let github = FakeGitHub::new(json!({}), json!([]));
    h.service.set_github(github.clone());
    let step: super::model::DeliveryApproval = serde_json::from_value(json!({
        "id": "a-pr", "task_id": task.id, "rung": "pr", "title": "feat: open it",
        "body": "## Summary\n\nWhat was approved.", "status": "pending",
        "requested_by": "coord", "requested_at_ms": 1
    }))
    .unwrap();
    h.service.store().unwrap().put_approval(&step).unwrap();

    let done = h.service.decide_approval("a-pr", true, None).unwrap();
    assert_eq!(done.status, DeliveryApprovalStatus::Executed);
    assert_eq!(
        done.outcome.as_deref(),
        Some("https://github.com/o/r/pull/50")
    );
    assert_eq!(done.notice, None);
    let base = task.base_ref.trim_start_matches("origin/");
    assert_eq!(
        github.actions.lock().unwrap().as_slice(),
        [format!("create {base} {}", task.branch)]
    );
    let published = github.snapshot.lock().unwrap().clone();
    assert_eq!(published["title"], "feat: open it");
    assert_eq!(published["body"], "## Summary\n\nWhat was approved.");
    assert_eq!(h.service.task_pr(&task.id).unwrap().unwrap().number, 50);
}

#[test]
fn a_pr_reported_by_the_coordinator_must_say_what_was_approved() {
    let h = harness();
    github_origin(&h);
    let task = h.task("O-2");
    let step: super::model::DeliveryApproval = serde_json::from_value(json!({
        "id": "a-pr", "task_id": task.id, "rung": "pr", "title": "feat: approved",
        "body": "Approved body.\nSecond line.", "status": "pending",
        "requested_by": "coord", "requested_at_ms": 1
    }))
    .unwrap();
    h.service.store().unwrap().put_approval(&step).unwrap();
    // GitHub out of reach: the step waits for the coordinator and says so.
    let approved = h.service.decide_approval("a-pr", true, None).unwrap();
    assert_eq!(approved.status, DeliveryApprovalStatus::Approved);
    assert!(approved
        .notice
        .as_deref()
        .is_some_and(|notice| notice.starts_with("Tinto no pudo abrir el PR")));

    // The coordinator opened it with other text.
    let github = FakeGitHub::new(
        json!({"state": "OPEN", "title": "feat: approved", "body": "Approved body.\nAnother line."}),
        json!([open_pr(8, "OPEN")]),
    );
    h.service.set_github(github.clone());
    let refused = h.service.complete_approval("a-pr", true, "").unwrap_err();
    assert_eq!(refused.category, "pr_mismatch");
    assert!(
        refused.message.starts_with(super::prs::PUBLISHED_MISMATCH),
        "{}",
        refused.message
    );
    assert!(
        refused
            .message
            .contains("línea 2: publicado «Another line.», aprobado «Second line.»"),
        "{}",
        refused.message
    );
    assert!(h.service.task_pr(&task.id).unwrap().is_none());

    // Once the PR says what was approved (line endings aside), it is done.
    github.snapshot.lock().unwrap()["body"] = json!("Approved body.\r\nSecond line.\r\n");
    let done = h.service.complete_approval("a-pr", true, "").unwrap();
    assert_eq!(done.status, DeliveryApprovalStatus::Executed);
    assert_eq!(done.notice, None);
}

#[test]
fn a_pr_found_on_github_with_other_text_is_marked() {
    let h = harness();
    github_origin(&h);
    let task = h.task("O-3");
    put_step(&h, &task, "a-push", "push", "executed", "Publicado");
    put_step(&h, &task, "a-pr", "pr", "approved", "");
    h.service.set_github(FakeGitHub::new(
        json!({"state": "OPEN", "title": "something else", "body": ""}),
        json!([open_pr(9, "OPEN")]),
    ));
    h.service.find_task_pr(&task.id).unwrap();
    let step = h.service.approval("a-pr").unwrap();
    assert_eq!(step.status, DeliveryApprovalStatus::Executed);
    assert!(step
        .notice
        .as_deref()
        .is_some_and(|notice| notice.starts_with(super::prs::PUBLISHED_MISMATCH)));
}

#[test]
fn the_user_can_close_or_draft_the_pr_of_a_stopped_delivery() {
    let h = harness();
    github_origin(&h);
    let task = h.task("O-4");
    let github = FakeGitHub::new(
        json!({"state": "OPEN", "title": "pr", "body": ""}),
        json!([open_pr(11, "OPEN")]),
    );
    h.service.set_github(github.clone());
    put_step(&h, &task, "a-push", "push", "executed", "Publicado");
    h.service.find_task_pr(&task.id).unwrap();
    h.service.task_pr_to_draft(&task.id).unwrap();
    h.service.close_task_pr(&task.id).unwrap();
    assert_eq!(
        github.actions.lock().unwrap().as_slice(),
        [
            "draft https://github.com/o/r/pull/11".to_string(),
            "close https://github.com/o/r/pull/11".to_string()
        ]
    );
}

#[test]
fn an_approved_pr_step_without_a_pr_is_opened_when_the_user_asks() {
    let h = harness();
    github_origin(&h);
    let task = h.task("O-5");
    // Approved before Tinto opened PRs itself, and nobody opened it.
    put_step(&h, &task, "a-pr", "pr", "approved", "");
    let github = FakeGitHub::new(json!({}), json!([]));
    h.service.set_github(github.clone());
    let done = h.service.open_pr_step("a-pr").unwrap();
    assert_eq!(done.status, DeliveryApprovalStatus::Executed);
    assert_eq!(h.service.task_pr(&task.id).unwrap().unwrap().number, 50);
    assert_eq!(github.actions.lock().unwrap().len(), 1);
    // Only an approved PR step without its PR.
    assert_eq!(
        h.service.open_pr_step("a-pr").unwrap_err().category,
        "approval_not_approved"
    );
}

impl Coordinator<'_> {
    /// A tool call from the Agents conversation `session`.
    fn write_from(
        &self,
        session: &str,
        name: &str,
        mut arguments: serde_json::Value,
    ) -> serde_json::Value {
        arguments["run_id"] = json!(self.run_id);
        arguments["owner"] = json!("coord");
        arguments["generation"] = json!(1);
        super::mcp::handle_message_from(
            &self.h.service,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
            Some(session),
        )
        .unwrap()["result"]
            .clone()
    }
}

#[test]
fn wait_run_wakes_on_the_users_decision_even_with_no_job_running() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("F-1");
    let task = h.service.task(&task_id).unwrap();
    put_step(&h, &task, "a-push", "push", "pending", "");

    let now = coordinator.write("wait_run", json!({}));
    let cursor = now["structuredContent"]["cursor"].as_u64().expect("cursor");
    assert_eq!(now["structuredContent"]["events"], json!([]));

    // The user rejects the step while the coordinator waits.
    let service = h.service.clone();
    let user = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        service.decide_approval("a-push", false, None).unwrap();
    });
    let started = Instant::now();
    let woken = coordinator.write(
        "wait_run",
        json!({"cursor": cursor, "timeout_seconds": 20, "idle_ok": true}),
    );
    user.join().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "it wakes on the change, not at the timeout"
    );
    let reply = &woken["structuredContent"];
    let events = reply["events"].as_array().expect("events");
    assert_eq!(events.len(), 1, "{reply}");
    assert_eq!(events[0]["kind"], "approval_rejected");
    assert_eq!(events[0]["text"], "F-1: el usuario rechazó el paso push");
    let next = reply["cursor"].as_u64().unwrap();
    assert!(next > cursor);

    // Nothing new: it waits out the timeout and says so.
    let quiet = coordinator.write(
        "wait_run",
        json!({"cursor": next, "timeout_seconds": 1, "idle_ok": true}),
    );
    assert_eq!(quiet["structuredContent"]["events"], json!([]));
    assert_eq!(quiet["structuredContent"]["waited"], true);
}

#[test]
fn wait_run_leaves_out_what_the_coordinator_asked_and_other_batches() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, worktree) = coordinator.task("F-2");
    let cursor = coordinator.write("wait_run", json!({}))["structuredContent"]["cursor"]
        .as_u64()
        .unwrap();
    // The coordinator's own request, and a change in another batch.
    std::fs::write(worktree.join("a.txt"), "a").unwrap();
    coordinator.write(
        "request_approval",
        json!({"task_id": task_id, "rung": "commit", "commits": [{"message": "feat: a", "paths": ["a.txt"]}]}),
    );
    let other = h.task("OTHER-1");
    put_step(&h, &other, "o-push", "push", "pending", "");
    h.service.decide_approval("o-push", false, None).unwrap();

    let (events, _) = h
        .service
        .coordinator_events(&coordinator.run_id, cursor)
        .unwrap();
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn the_coordinators_conversation_is_woken_once_with_what_it_has_not_seen() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("F-3");
    let task = h.service.task(&task_id).unwrap();
    put_step(&h, &task, "a-push", "push", "pending", "");
    h.service.decide_approval("a-push", false, None).unwrap();

    // Its tool calls name its conversation; what happened before it is not news.
    coordinator.write_from("session-1", "wait_run", json!({}));
    let run = h.service.run(&coordinator.run_id).unwrap();
    assert_eq!(run.coordinator_session.as_deref(), Some("session-1"));
    let later = super::now_ms() + super::feed::WAKE_QUIET_MS + 1;
    assert!(h.service.pending_wakeups(later).unwrap().is_empty());

    h.service.set_run_paused(&coordinator.run_id, true).unwrap();
    // Still settling: nothing yet.
    assert!(h
        .service
        .pending_wakeups(super::now_ms())
        .unwrap()
        .is_empty());
    let later = super::now_ms() + super::feed::WAKE_QUIET_MS + 1;
    let wakeups = h.service.pending_wakeups(later).unwrap();
    assert_eq!(wakeups.len(), 1);
    let wakeup = &wakeups[0];
    assert_eq!(wakeup.session_id, "session-1");
    assert!(
        wakeup.text.contains("- el usuario pausó el lote"),
        "{}",
        wakeup.text
    );
    assert!(wakeup.text.contains("Llama a wait_run"), "{}", wakeup.text);

    // Once sent, it is not sent again.
    h.service
        .mark_coordinator_notified(&wakeup.runs[0].0, wakeup.runs[0].1)
        .unwrap();
    assert!(h.service.pending_wakeups(later).unwrap().is_empty());
    assert!(h
        .service
        .run(&coordinator.run_id)
        .unwrap()
        .notified_at_ms
        .is_some());
}

/// A wait_run argument for each batch, as its owner `coord` at generation 1.
fn held(coordinators: &[&Coordinator]) -> serde_json::Value {
    json!(coordinators
        .iter()
        .map(|c| json!({"run_id": c.run_id, "owner": "coord", "generation": 1}))
        .collect::<Vec<_>>())
}

fn wait_from(h: &Harness, session: &str, arguments: serde_json::Value) -> serde_json::Value {
    super::mcp::handle_message_from(
        &h.service,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "wait_run", "arguments": arguments}}),
        Some(session),
    )
    .unwrap()["result"]
        .clone()
}

#[test]
fn one_wait_run_covers_every_batch_its_conversation_coordinates() {
    let h = harness();
    let first = Coordinator::start(&h);
    let second = Coordinator::start(&h);
    first.task("M-1");
    second.task("M-2");
    let now = wait_from(&h, "both", json!({"runs": held(&[&first, &second])}));
    let cursor = now["structuredContent"]["cursor"].as_u64().expect("cursor");
    for run_id in [&first.run_id, &second.run_id] {
        let run = h.service.run(run_id).unwrap();
        assert_eq!(run.coordinator_session.as_deref(), Some("both"));
        assert_eq!(run.generation, 1, "ownership stays per batch");
    }

    // A change in the second batch wakes the one wait, naming its batch.
    let service = h.service.clone();
    let second_id = second.run_id.clone();
    let user = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        service.set_run_paused(&second_id, true).unwrap();
    });
    let started = Instant::now();
    let woken = wait_from(
        &h,
        "both",
        json!({"runs": held(&[&first, &second]), "cursor": cursor, "timeout_seconds": 20, "idle_ok": true}),
    );
    user.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(10));
    let reply = &woken["structuredContent"];
    let events = reply["events"].as_array().expect("events");
    assert_eq!(events.len(), 1, "{reply}");
    assert_eq!(events[0]["run_id"], json!(second.run_id));
    assert_eq!(events[0]["text"], "el usuario pausó el lote");
    // Both batches were handed the new cursor: nothing to wake them for.
    let next = reply["cursor"].as_u64().unwrap();
    for run_id in [&first.run_id, &second.run_id] {
        assert_eq!(h.service.run(run_id).unwrap().returned_cursor, next);
    }
    let later = super::now_ms() + super::feed::WAKE_QUIET_MS + 1;
    assert!(h.service.pending_wakeups(later).unwrap().is_empty());
}

#[test]
fn a_wait_on_several_batches_is_fenced_per_batch() {
    let h = harness();
    let first = Coordinator::start(&h);
    let second = Coordinator::start(&h);
    // Another coordinator takes the second batch.
    second.call(
        "takeover_run",
        json!({"run_id": second.run_id, "owner": "other"}),
    );
    let refused = wait_from(&h, "both", json!({"runs": held(&[&first, &second])}));
    let text = error_text(&refused);
    assert!(text.contains(&format!("lote {}", second.run_id)), "{text}");
    // The batch it still holds goes on.
    let alone = wait_from(&h, "both", json!({"runs": held(&[&first])}));
    assert!(alone["structuredContent"]["cursor"].is_u64(), "{alone}");
    let missing = wait_from(&h, "both", json!({"runs": [{"run_id": first.run_id}]}));
    assert!(error_text(&missing).contains("generation"));
}

#[test]
fn a_conversation_with_several_batches_is_woken_once_for_all_of_them() {
    let h = harness();
    let first = Coordinator::start(&h);
    let second = Coordinator::start(&h);
    wait_from(&h, "both", json!({"runs": held(&[&first, &second])}));
    h.service.set_run_paused(&first.run_id, true).unwrap();
    h.service.set_run_paused(&second.run_id, true).unwrap();
    let later = super::now_ms() + super::feed::WAKE_QUIET_MS + 1;
    let wakeups = h.service.pending_wakeups(later).unwrap();
    assert_eq!(wakeups.len(), 1, "{wakeups:?}");
    let wakeup = &wakeups[0];
    assert_eq!(wakeup.session_id, "both");
    let mut woken: Vec<&str> = wakeup.runs.iter().map(|(id, _)| id.as_str()).collect();
    woken.sort_unstable();
    let mut expected = vec![first.run_id.as_str(), second.run_id.as_str()];
    expected.sort_unstable();
    assert_eq!(woken, expected);
    assert!(wakeup.text.contains("`runs`"), "{}", wakeup.text);
    assert!(
        wakeup.text.contains(&first.run_id) && wakeup.text.contains(&second.run_id),
        "{}",
        wakeup.text
    );

    for (run_id, cursor) in &wakeup.runs {
        h.service
            .mark_coordinator_notified(run_id, *cursor)
            .unwrap();
    }
    assert!(h.service.pending_wakeups(later).unwrap().is_empty());
}

#[test]
fn the_handover_names_every_batch_and_one_wait_for_all() {
    let h = harness();
    let first = h.service.run(&Coordinator::start(&h).run_id).unwrap();
    let second = h.service.run(&Coordinator::start(&h).run_id).unwrap();
    let one = super::feed::handover_text(std::slice::from_ref(&first));
    assert!(one.contains(&format!("run_id {}", first.id)), "{one}");
    assert!(!one.contains("`runs`"), "{one}");
    let both = super::feed::handover_text(&[first.clone(), second.clone()]);
    assert!(both.contains("coordinar 2 lotes"), "{both}");
    assert!(
        both.contains(&format!("run_id {}", first.id))
            && both.contains(&format!("run_id {}", second.id)),
        "{both}"
    );
    assert!(both.contains("wait_run sin cursor con `runs`"), "{both}");
}

#[test]
fn a_coordinator_inside_wait_run_is_not_woken() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    coordinator.task("F-4");
    coordinator.write_from("session-2", "wait_run", json!({}));
    // Counted as inside wait_run the way the wait counts it, for as long as
    // the test needs: a real wait in a thread could time out on a busy
    // machine before the change was checked.
    let _inside =
        super::feed::Waiting::start(&h.service, std::slice::from_ref(&coordinator.run_id));
    h.service.set_run_paused(&coordinator.run_id, true).unwrap();
    let later = super::now_ms() + super::feed::WAKE_QUIET_MS + 1;
    // The wait hands it the change; no message is needed.
    let pending = h.service.pending_wakeups(later).unwrap();
    assert!(pending.is_empty(), "{pending:?}");
}

#[test]
fn approving_a_step_tinto_does_not_run_asks_for_no_outcome() {
    let h = harness();
    let task = h.task("J-1");
    // A Jira step without the repo's Jira in Tinto, at both stages: the
    // coordinator runs it and reports it.
    for (id, stage) in [("a-jira-pr", "after_pr"), ("a-jira-merge", "after_merge")] {
        let step: super::model::DeliveryApproval = serde_json::from_value(json!({
            "id": id, "task_id": task.id, "rung": "jira", "title": "Jira", "body": "",
            "status": "pending", "requested_by": "coord", "requested_at_ms": 1, "stage": stage
        }))
        .unwrap();
        h.service.store().unwrap().put_approval(&step).unwrap();
        let approved = h.service.decide_approval(id, true, None).unwrap();
        assert_eq!(approved.status, DeliveryApprovalStatus::Approved, "{stage}");
        assert_eq!(approved.outcome, None, "{stage}");
    }
    let done = h
        .service
        .complete_approval("a-jira-pr", true, "AGOS-474 → In Review")
        .unwrap();
    assert_eq!(done.status, DeliveryApprovalStatus::Executed);
    assert_eq!(done.outcome.as_deref(), Some("AGOS-474 → In Review"));
    let failed = h
        .service
        .complete_approval("a-jira-merge", false, "Jira no respondió")
        .unwrap();
    assert_eq!(failed.status, DeliveryApprovalStatus::Failed);
}

#[test]
fn a_coordinator_the_user_asks_takes_the_batch_and_the_old_one_is_told() {
    let h = harness();
    let old = Coordinator::start(&h);
    old.task("H-1");
    let cursor = old.write_from("old-session", "wait_run", json!({}))["structuredContent"]
        ["cursor"]
        .as_u64()
        .unwrap();

    // Another conversation is asked to take it: no generation needed.
    let locked = old.call("acquire_run", json!({"run_id": old.run_id, "owner": "new"}));
    assert!(error_text(&locked).contains("takeover_run"));
    let taken = super::mcp::handle_message_from(
        &h.service,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "takeover_run",
            "arguments": {"run_id": old.run_id, "owner": "new"}}}),
        Some("new-session"),
    )
    .unwrap()["result"]
        .clone();
    let run = &taken["structuredContent"];
    assert_eq!(run["owner"], "new", "{taken}");
    assert_eq!(run["generation"], 2);
    assert_eq!(run["coordinator_session"], "new-session");

    // The old one learns it from its wait, and its writes are refused.
    let (events, _) = h.service.coordinator_events(&old.run_id, cursor).unwrap();
    assert!(events
        .iter()
        .any(|event| event["text"] == "otro coordinador tomó el lote"));
    let refused = old.write("wait_run", json!({}));
    assert!(error_text(&refused).contains("deja este lote"));
}

#[test]
fn a_reply_to_a_reviewer_waits_for_the_user_and_is_posted_once_in_its_thread() {
    use super::phase::{Phase, Waiter};
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("R-2");
    let github = FakeGitHub::new(json!({}), json!([]));
    *github.review_comments.lock().unwrap() = json!([
        {"id": 2105, "path": "cli/src/commands.ts", "line": 1092, "position": 14,
         "html_url": "https://github.com/o/r/pull/299#discussion_r2105",
         "user": {"login": "Sn4pe"}, "created_at": "2026-10-19T10:00:00Z",
         "body": "With `--force`, a folder the CLI did not install is deleted.",
         "diff_hunk": "@@ -979,1 +1091,1 @@ install\n-  old();\n+  installIntoDir(force);"},
        {"id": 2107, "in_reply_to_id": 2105, "user": {"login": "Sn4pe"}, "body": "Any news?",
         "html_url": "https://github.com/o/r/pull/299#discussion_r2107"},
        {"id": 2200, "path": "a.ts", "line": 3, "position": 2, "user": {"login": "ana"},
         "body": "typo", "diff_hunk": "@@ -1,1 +1,1 @@\n+x"}
    ]);
    h.service.set_github(github.clone());
    // The fix is published, so the threads can be resolved.
    let task = h.service.task(&task_id).unwrap();
    let remote = format!("refs/remotes/origin/{}", task.branch);
    git(
        &PathBuf::from(plain_path(&task.worktree)),
        &["update-ref", &remote, "HEAD"],
    );
    // Past the brainstorm a reply still waits for the user.
    h.service.end_brainstorm(&coordinator.run_id).unwrap();
    let ask = |url: &str, body: &str| {
        coordinator.write(
            "request_approval",
            json!({"task_id": task_id, "rung": "pr_reply", "comment_url": url,
                   "body": body, "resolve_thread": true}),
        )
    };

    let asked = ask(
        "https://github.com/o/r/pull/299#discussion_r2107",
        "The CLI now deletes only folders it installed (abc123).",
    );
    let reply = &asked["structuredContent"];
    assert_eq!(reply["status"], "pending", "{asked}");
    assert_eq!(reply["title"], "Respuesta a Sn4pe");
    assert_eq!(reply["thread"]["comment"]["body"], "Any news?");
    let reply_id = reply["id"].as_str().unwrap().to_string();
    // One open reply per comment; another comment can have its own.
    assert!(error_text(&ask(
        "https://github.com/o/r/pull/299#discussion_r2107",
        "again"
    ))
    .contains("ya hay una aprobación abierta"));
    let general = ask("https://github.com/o/r/pull/299", "Thanks for the review!");
    let general_id = general["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        general["structuredContent"]["thread"],
        serde_json::Value::Null
    );
    let gone = ask(
        "https://github.com/o/r/pull/299#discussion_r2200",
        "The typo is gone.",
    );
    let gone_id = gone["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(error_text(&ask("https://github.com/o/r/issues/3", "x")).contains("comment_url"));

    // Pending replies hold neither the phase nor the stages.
    let phase = h.service.overview().unwrap().phases[&task_id].clone();
    assert!(
        !matches!(phase.phase, Phase::Approval { .. }),
        "{:?}",
        phase.phase
    );
    assert_ne!(phase.waits_on, Waiter::User);

    // The user's edited text is what is posted, in the thread's first
    // comment, and the thread is resolved. Approved in a batch, the call
    // returns at once and the post follows in the background.
    h.service
        .approve_replies(
            vec![super::prs::ApprovedReply {
                approval_id: reply_id.clone(),
                body:
                    "The CLI now deletes only folders it installed. A new test checks it (abc123)."
                        .into(),
                images: Vec::new(),
            }],
            Vec::new(),
        )
        .unwrap();
    let posted = eventually(|| {
        let reply = h.service.approval(&reply_id).unwrap();
        (reply.status != DeliveryApprovalStatus::Pending).then_some(reply)
    });
    assert_eq!(posted.status, DeliveryApprovalStatus::Executed);
    assert_eq!(
        posted.body,
        "The CLI now deletes only folders it installed. A new test checks it (abc123)."
    );
    let url = posted.outcome.clone().unwrap();
    assert!(url.contains("#discussion_r9"), "{url}");
    assert_eq!(
        github.actions.lock().unwrap().as_slice(),
        [
            "reply 2105 The CLI now deletes only folders it installed. A new test checks it (abc123).".to_string(),
            "resolve 2105".to_string()
        ]
    );

    // Asked and approved again with the same text: the reply already there
    // is taken as it, nothing is posted twice.
    let again = ask(
        "https://github.com/o/r/pull/299#discussion_r2105",
        "The CLI now deletes only folders it installed. A new test checks it (abc123).",
    );
    let again = h
        .service
        .decide_approval(
            again["structuredContent"]["id"].as_str().unwrap(),
            true,
            None,
        )
        .unwrap();
    assert_eq!(again.status, DeliveryApprovalStatus::Executed);
    assert_eq!(again.outcome.as_deref(), Some(url.as_str()));
    assert_eq!(
        github
            .actions
            .lock()
            .unwrap()
            .iter()
            .filter(|action| action.starts_with("reply"))
            .count(),
        1
    );

    // A general comment goes on the PR.
    let commented = h.service.decide_approval(&general_id, true, None).unwrap();
    assert_eq!(
        commented.outcome.as_deref(),
        Some("https://github.com/o/r/pull/299#issuecomment-1")
    );

    // A comment deleted meanwhile fails the step with why, without retrying.
    github
        .review_comments
        .lock()
        .unwrap()
        .as_array_mut()
        .unwrap()
        .retain(|comment| comment["id"] != 2200);
    let failed = h.service.decide_approval(&gone_id, true, None).unwrap();
    assert_eq!(failed.status, DeliveryApprovalStatus::Failed);
    assert!(failed.outcome.unwrap().contains("ya no existe"));
    assert_eq!(github.actions.lock().unwrap().len(), 4);
}

#[test]
fn a_reply_comes_with_the_users_style_and_is_refused_with_a_label_or_an_unpushed_fix() {
    let h = harness();
    github_origin(&h);
    let coordinator = Coordinator::start(&h);
    let (task_id, worktree) = coordinator.task("R-3");
    let github = FakeGitHub::new(json!({}), json!([]));
    let mut comments = vec![json!(
        {"id": 3000, "path": "README.md", "line": 1, "position": 1,
         "html_url": "https://github.com/o/r/pull/299#discussion_r3000",
         "user": {"login": "Sn4pe"}, "created_at": "2026-10-19T10:00:00Z",
         "body": "Is the dialog shown again after leaving? Also, typo here.",
         "diff_hunk": "@@ -1,1 +1,1 @@\n+base"}
    )];
    // Oldest first: six replies of the user, one of someone else and a
    // comment of the user's that starts a thread.
    for n in 1..=6 {
        comments.push(json!({"id": 3000 + n, "in_reply_to_id": 3000,
            "user": {"login": "me"}, "body": format!("The leave dialog now says {n}.")}));
    }
    comments
        .push(json!({"id": 3010, "in_reply_to_id": 3000, "user": {"login": "ana"}, "body": "ok"}));
    comments.push(
        json!({"id": 3011, "user": {"login": "me"}, "body": "a review comment", "path": "a.ts"}),
    );
    *github.review_comments.lock().unwrap() = json!(comments);
    h.service.set_github(github);
    let ask = |body: &str, resolve: bool| {
        coordinator.write(
            "request_approval",
            json!({"task_id": task_id, "rung": "pr_reply",
                   "comment_url": "https://github.com/o/r/pull/299#discussion_r3000",
                   "body": body, "resolve_thread": resolve}),
        )
    };
    let withdraw = |reply: &serde_json::Value| {
        h.service
            .withdraw_approval(reply["structuredContent"]["id"].as_str().unwrap(), "again")
            .unwrap();
    };

    // A status label opening the reply is refused, naming the rule.
    for body in [
        "Fixed: the typo.",
        "Done.",
        "**Addressed** in 4f54085",
        "resolved",
    ] {
        let refused = error_text(&ask(body, false));
        assert!(refused.contains("etiquetas de estado"), "{body}: {refused}");
    }

    // The user's five latest replies come with it, newest first, and an
    // unanswered question is warned about.
    let asked = ask("The typo is gone. Tests cover it.", false);
    let reply = &asked["structuredContent"];
    assert_eq!(
        reply["style_examples"],
        json!([6, 5, 4, 3, 2].map(|n| format!("The leave dialog now says {n}.")))
    );
    let notice = reply["notice"].as_str().unwrap_or_default();
    assert!(
        notice.contains("«Is the dialog shown again after leaving?»"),
        "{asked}"
    );
    // Answered once edited: the warning goes.
    let id = reply["id"].as_str().unwrap();
    let edited = h
        .service
        .set_reply_body(id, "The dialog is not shown again after leaving.")
        .unwrap();
    assert_eq!(edited.notice, None);
    withdraw(&asked);

    // Resolving waits for the fix to be on the remote branch.
    let task = h.service.task(&task_id).unwrap();
    let remote = format!("refs/remotes/origin/{}", task.branch);
    let refused = error_text(&ask("The dialog now closes once.", true));
    assert!(refused.contains("no está publicada"), "{refused}");
    git(&worktree, &["update-ref", &remote, "HEAD"]);
    std::fs::write(worktree.join("README.md"), "fixed\n").unwrap();
    let refused = error_text(&ask("The dialog now closes once.", true));
    assert!(
        refused.contains("README.md tiene cambios sin commit"),
        "{refused}"
    );
    git(&worktree, &["commit", "-qam", "fix"]);
    let refused = error_text(&ask("The dialog now closes once.", true));
    assert!(
        refused.contains("que cambia README.md, no está en origin/"),
        "{refused}"
    );
    git(&worktree, &["update-ref", &remote, "HEAD"]);
    let asked = ask("The dialog now closes once.", true);
    assert_eq!(
        asked["structuredContent"]["resolve_thread"], true,
        "{asked}"
    );
    // The commit that changed the commented file is the fix, linked in the PR.
    let fix = &asked["structuredContent"]["fix_commits"];
    assert_eq!(fix.as_array().unwrap().len(), 1, "{asked}");
    assert_eq!(fix[0]["subject"], "fix");
    assert!(fix[0]["url"]
        .as_str()
        .unwrap()
        .starts_with("https://github.com/o/r/pull/299/commits/"));
}

#[test]
fn a_decision_shows_the_review_comment_and_the_code_it_is_about() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, worktree) = coordinator.task("R-1");
    let github = FakeGitHub::new(json!({}), json!([]));
    *github.review_comments.lock().unwrap() = json!([
        {"id": 2105, "path": "cli/src/commands.ts", "line": 1092, "position": 14,
         "html_url": "https://github.com/o/r/pull/299#discussion_r2105",
         "user": {"login": "Sn4pe"}, "created_at": "2026-10-19T10:00:00Z",
         "body": "With `--force`, a folder the CLI did not install is deleted.",
         "diff_hunk": "@@ -979,2 +1091,2 @@ install\n-  // Replace only\n-  old();\n+  // Replace\n+  installIntoDir(force);"},
        {"id": 2106, "in_reply_to_id": 2105, "user": {"login": "me"}, "body": "ok"},
        {"id": 2200, "path": "cli/src/commands.ts", "line": null, "original_line": 1071,
         "position": null, "user": {"login": "Sn4pe"}, "body": "outdated", "diff_hunk": "@@ -1,1 +1,1 @@\n+x"}
    ]);
    h.service.set_github(github);
    // The worktree: two lines the task added to a file of its base.
    std::fs::write(worktree.join("README.md"), "base\nadded\nmore\n").unwrap();

    let asked = coordinator.write(
        "request_decisions",
        json!({"task_id": task_id, "decisions": [{
            "kind": "choice", "question": "Which folders may --force replace?",
            "options": [{"label": "Only copies of this skill", "recommended": true}, {"label": "Any"}],
            "references": [
                {"comment_url": "https://github.com/o/r/pull/299#discussion_r2105", "note": "first comment"},
                {"comment_url": "https://github.com/o/r/pull/299#discussion_r2200"},
                {"path": "README.md", "start_line": 1, "end_line": 3},
                {"path": "../outside.ts", "start_line": 1},
                {"comment_url": "https://github.com/o/r/pull/299#discussion_r9"}
            ]
        }]}),
    );
    let threads = asked["structuredContent"]["result"][0]["references"]
        .as_array()
        .unwrap_or_else(|| panic!("{asked}"))
        .clone();
    assert_eq!(threads.len(), 5);

    let review = &threads[0];
    assert_eq!(review["kind"], "pr_comment");
    assert_eq!(review["path"], "cli/src/commands.ts");
    assert_eq!(review["line"], 1092);
    assert_eq!(review["outdated"], false);
    assert_eq!(review["note"], "first comment");
    assert_eq!(review["comment"]["author"], "Sn4pe");
    assert_eq!(review["comment"]["replies"], 1);
    let lines = review["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[3]["new"], 1092);
    assert_eq!(lines[3]["target"], true);
    assert_eq!(lines[0]["kind"], "del");

    assert_eq!(threads[1]["outdated"], true);
    assert_eq!(threads[1]["line"], 1071);

    let code = &threads[2];
    assert_eq!(code["kind"], "code");
    let kinds: Vec<_> = code["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| {
            (
                line["new"].as_u64().unwrap(),
                line["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            (1, "ctx".to_string()),
            (2, "add".to_string()),
            (3, "add".to_string())
        ]
    );

    // What cannot be read says why instead of showing made-up code.
    assert!(threads[3]["error"].as_str().unwrap().contains("relativa"));
    assert_eq!(threads[4]["error"], "el comentario ya no existe en GitHub");
}

#[test]
fn qa_problems_wait_for_the_user_and_reach_the_coordinator_and_the_next_qa() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("Q-1");
    let task = h.service.task(&task_id).unwrap();
    let qa = h.dispatch(&task, "qa", "events=codex-ok record=problems");
    h.wait(&qa.id);

    let problems = h.service.task_qa_problems(&task_id).unwrap();
    assert_eq!(problems.len(), 2);
    assert_eq!(problems[0].number, 1);
    assert_eq!(problems[0].label, "P1");
    assert_eq!(problems[0].suggestions, ["Move the toast up."]);
    assert_eq!(
        problems[0].status,
        super::model::DeliveryQaProblemStatus::Open
    );

    // The user dismisses one, reports another and sends two to fix.
    h.service
        .dismiss_qa_problem(&problems[1].id, "the menu is meant to be cut")
        .unwrap();
    let own = h
        .service
        .report_qa_problem(&task_id, "The header hides the mocks", "npm test")
        .unwrap();
    assert_eq!(own.number, 3);
    let cursor = coordinator.write("wait_run", json!({}))["structuredContent"]["cursor"]
        .as_u64()
        .unwrap();
    let choices = [
        super::qa_problems::QaProblemChoice {
            problem_id: problems[0].id.clone(),
            solution: "Move the toast up.".into(),
        },
        super::qa_problems::QaProblemChoice {
            problem_id: own.id.clone(),
            solution: "Two columns, real and mocked".into(),
        },
    ];
    h.service.send_qa_problems(&task_id, &choices).unwrap();
    assert!(
        h.service.send_qa_problems(&task_id, &choices).is_err(),
        "sent once"
    );

    // The coordinator hears it and reads exactly what to fix.
    let (events, _) = h
        .service
        .coordinator_events(&coordinator.run_id, cursor)
        .unwrap();
    // And the round they open, with what to fix.
    assert_eq!(events.len(), 2);
    assert!(events[0]["text"].as_str().unwrap().contains("(#1, #3)"));
    assert_eq!(events[1]["kind"], "round_opened");
    assert!(events[1]["text"]
        .as_str()
        .unwrap()
        .contains("Ronda 2 · 2 problemas de QA para corregir: 1) #1 P1:"));
    let read = coordinator.call("read_qa_problems", json!({"task_id": task_id}));
    let listed = read["structuredContent"]["result"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(listed[0]["status"], "sent");
    assert_eq!(listed[0]["solution"], "Move the toast up.");
    assert_eq!(listed[1]["status"], "dismissed");
    let updated = coordinator.write(
        "update_qa_problems",
        json!({"task_id": task_id, "updates": [{"problem_id": problems[0].id, "status": "fixing", "note": "implementation 3"}]}),
    );
    assert_eq!(
        updated["structuredContent"]["result"][0]["status"], "fixing",
        "{updated}"
    );
    let refused = coordinator.write(
        "update_qa_problems",
        json!({"task_id": task_id, "updates": [{"problem_id": problems[1].id, "status": "verified"}]}),
    );
    assert!(error_text(&refused).contains("#2"));

    // The next QA is told what to check again and what not to report.
    let followup = h.service.qa_followup(&task_id).unwrap();
    assert_eq!(followup.len(), 3, "{followup:?}");
    assert!(followup[0].starts_with("Check #1 again"));
    assert!(followup
        .iter()
        .any(|line| line.contains("Do not report «The menu is cut off.»")));
    let record = "# Q-1 — Visual changes\n\n## 1. Menú\n\n**Problems:**\n- P1: The toast covers Retry on mobile.\n- P2: The menu is cut off.\n";
    h.service
        .record_qa_problems(&task, "qa-2", 2, record)
        .unwrap();
    // Its record brings back neither the dismissed problem nor duplicates
    // of what was open.
    let after = h.service.task_qa_problems(&task_id).unwrap();
    let open: Vec<_> = after
        .iter()
        .filter(|problem| problem.status == super::model::DeliveryQaProblemStatus::Open)
        .map(|problem| (problem.number, problem.text.as_str()))
        .collect();
    assert_eq!(open, [(4, "The toast covers Retry on mobile.")]);
}

#[test]
fn past_the_brainstorm_qa_problems_go_to_the_coordinator_with_qas_fix() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("Q-9");
    h.service.end_brainstorm(&coordinator.run_id).unwrap();
    let task = h.service.task(&task_id).unwrap();
    let qa = h.dispatch(&task, "qa", "events=codex-ok record=problems");
    h.wait(&qa.id);

    let sent: Vec<_> = h
        .service
        .task_qa_problems(&task_id)
        .unwrap()
        .into_iter()
        .map(|problem| (problem.status, problem.solution.unwrap_or_default()))
        .collect();
    let status = super::model::DeliveryQaProblemStatus::Sent;
    assert_eq!(
        sent,
        [
            (status, "Move the toast up.".to_string()),
            (status, "Let the menu scroll.".to_string())
        ]
    );
    let round = h.service.overview().unwrap().phases[&task_id].round.clone();
    assert!(round.is_some(), "a round opens for them");
}

#[test]
fn past_the_brainstorm_the_jira_steps_tinto_runs_go_through() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("AGOS-455");
    let jira = Arc::new(FakeJira {
        status: std::sync::Mutex::new("Por hacer".into()),
        workflow: vec![("Por hacer", "151", "Submit for Review", "In Review")],
        writes: std::sync::Mutex::new(Vec::new()),
    });
    h.service.set_jira(jira.clone());
    h.service.end_brainstorm(&coordinator.run_id).unwrap();
    let ask = || {
        coordinator.write(
            "request_approval",
            json!({"task_id": task_id, "rung": "jira", "stage": "after_pr", "title": "AGOS-455"}),
        )["structuredContent"]
            .clone()
    };

    // Without Jira in Tinto the coordinator runs it, so the user approves it.
    let manual = ask();
    assert_eq!(manual["status"], "pending", "{manual}");
    h.service
        .withdraw_approval(manual["id"].as_str().unwrap(), "Jira set")
        .unwrap();

    let mut settings = h
        .service
        .store()
        .unwrap()
        .repo_settings(h.repo.path())
        .unwrap();
    settings.jira = Some(super::model::DeliveryJiraSettings {
        host: "https://jira.test".into(),
        project: "AGOS".into(),
        after_pr: vec!["In Review".into()],
        after_merge: vec!["Cerrada".into()],
        has_token: false,
    });
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    h.service.set_jira_token(h.repo.path(), "secret").unwrap();
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/279").unwrap();
    h.service.link_pr(&task_id, &reference).unwrap();

    let done = ask();
    assert_eq!(done["status"], "executed", "{done}");
    assert_eq!(*jira.status.lock().unwrap(), "In Review");
}

#[test]
fn a_pr_with_conflicts_brings_its_base_in_without_asking() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("S-9");
    let task = h.service.task(&task_id).unwrap();
    diverge_from_main(&h, &task, true);
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/281").unwrap();
    h.service.link_pr(&task_id, &reference).unwrap();
    let mut pr = h.service.task_pr(&task_id).unwrap().unwrap();
    pr.state = "open".into();
    pr.conflicts = true;
    let syncs = || {
        h.service
            .store()
            .unwrap()
            .approvals()
            .unwrap()
            .into_iter()
            .filter(|approval| approval.rung == DeliveryRung::Sync)
            .collect::<Vec<_>>()
    };

    // Brainstorm or not: a sync publishes nothing, so nobody approves it.
    h.service.sync_conflicted_pr(&pr).unwrap();
    let [sync] = syncs().try_into().unwrap();
    assert_eq!(
        sync.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        sync.outcome
    );
    assert_eq!(sync.requested_by, "auto");
    assert_eq!(
        sync.title,
        format!("Merge branch 'main' into {}", task.branch)
    );
    assert_eq!(sync.conflicts, ["shared.txt"]);
    let phase = h.service.overview().unwrap().phases[&task_id].clone();
    assert_eq!(phase.next_stage.as_deref(), Some("implementation"));
    // Read again while the merge is in progress: no second sync.
    h.service.sync_conflicted_pr(&pr).unwrap();
    assert_eq!(syncs().len(), 1);
}

#[test]
fn a_merged_pr_brings_its_base_into_the_other_open_prs_and_checks_verify_the_merge() {
    let h = harness();
    set_repo_checks(&h, &["exit=0"]);
    let coordinator = Coordinator::start(&h);
    let (merged_id, _) = coordinator.task("S-10");
    let (open_id, _) = coordinator.task("S-11");
    let open = h.service.task(&open_id).unwrap();
    h.service
        .set_task_checks(&open_id, vec!["exit=0".into()])
        .unwrap();
    diverge_from_main(&h, &open, false);
    let pr = |task_id: &str, number: u32, state: &str| {
        let reference =
            super::prs::parse_pr_url(&format!("https://github.com/o/r/pull/{number}")).unwrap();
        // Stored as GitHub showed it, without the background read a link starts.
        let mut pr = super::prs::DeliveryPr::linked(task_id, &reference);
        pr.state = state.into();
        pr.base = Some("main".into());
        h.service.store().unwrap().put_pr(&pr).unwrap();
        pr
    };
    let merged = pr(&merged_id, 290, "merged");
    pr(&open_id, 291, "open");
    h.service.end_brainstorm(&coordinator.run_id).unwrap();

    h.service.sync_after_merge(&merged).unwrap();
    let store = h.service.store().unwrap();
    let sync = store
        .approvals()
        .unwrap()
        .into_iter()
        .find(|approval| approval.rung == DeliveryRung::Sync)
        .expect("the open PR's task syncs");
    assert_eq!(sync.task_id, open_id);
    assert_eq!(
        sync.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        sync.outcome
    );
    assert!(sync.conflicts.is_empty());
    // The clean merge is verified by Tinto's checks, not by a stage.
    let checks: Vec<String> = store
        .task_jobs(&open_id)
        .unwrap()
        .into_iter()
        .filter(|job| job.role == "checks")
        .map(|job| job.prompt)
        .collect();
    assert_eq!(checks, ["exit=0"]);
}

#[test]
fn a_qa_record_from_before_tinto_read_problems_is_read_at_startup() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("Q-1");
    let task = h.service.task(&task_id).unwrap();
    let qa = h.dispatch(&task, "qa", "events=codex-ok record=problems");
    h.wait(&qa.id);
    // A later QA whose record an older Tinto rendered without reading it.
    let mut older = h.service.store().unwrap().job(&qa.id).unwrap().unwrap();
    assert!(older.visual_record.is_some());
    older.id = "qa-before-update".to_string();
    older.attempt += 1;
    older.created_at_ms += 1;
    h.service.store().unwrap().put_job(&older).unwrap();

    h.service.record_missed_qa_problems().unwrap();
    h.service.record_missed_qa_problems().unwrap();
    let open: Vec<_> = h
        .service
        .task_qa_problems(&task_id)
        .unwrap()
        .into_iter()
        .filter(|problem| problem.status == super::model::DeliveryQaProblemStatus::Open)
        .map(|problem| (problem.job_id.unwrap(), problem.number))
        .collect();
    assert_eq!(
        open,
        [
            ("qa-before-update".to_string(), 3),
            ("qa-before-update".to_string(), 4)
        ],
        "read once, in place of the earlier QA's"
    );
}

#[test]
fn the_overview_says_what_a_coordinator_that_is_not_waiting_has_not_read() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("F-1");
    let task = h.service.task(&task_id).unwrap();
    put_step(&h, &task, "a-push", "push", "pending", "");
    let cursor = coordinator.write("wait_run", json!({}))["structuredContent"]["cursor"]
        .as_u64()
        .unwrap();
    let state = |h: &Harness| h.service.overview().unwrap().coordinators[0].clone();
    assert_eq!(state(&h).unread, 0);

    // The user decides while the coordinator is not waiting: nobody reads it.
    h.service.decide_approval("a-push", false, None).unwrap();
    let stalled = state(&h);
    assert_eq!(stalled.run_id, coordinator.run_id);
    assert!(!stalled.waiting);
    assert_eq!(stalled.unread, 1);
    assert!(stalled.unread_since_ms.is_some());

    // Once it reads it with wait_run, there is nothing left.
    coordinator.write(
        "wait_run",
        json!({"cursor": cursor, "timeout_seconds": 1, "idle_ok": true}),
    );
    assert_eq!(state(&h).unread, 0);
}

#[test]
fn a_qa_problem_is_dismissed_without_saying_why() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("Q-2");
    let reported = h
        .service
        .report_qa_problem(&task_id, "The header is too tall", "home")
        .unwrap();
    let dismissed = h.service.dismiss_qa_problem(&reported.id, "  ").unwrap();
    assert_eq!(dismissed.reason, None);
    assert_eq!(
        h.service.qa_followup(&task_id).unwrap(),
        ["Do not report «The header is too tall» again: the user decided it is not a problem."]
    );
}

/// A batch the size of a real one: delivered and active tasks, jobs with
/// long prompts, ~80 allowed commands and long results, redone steps.
fn busy_batch(h: &Harness) -> Vec<DeliveryTask> {
    use super::model::{DeliveryApproval, DeliveryDecision};

    let run = h
        .service
        .create_run(
            h.repo.path().to_path_buf(),
            "Lote",
            Some("coord".into()),
            None,
        )
        .unwrap();
    let long = |what: &str, chars: usize| what.repeat(chars / what.len() + 1)[..chars].to_string();
    let commands: Vec<String> = (0..80)
        .map(|n| format!("npm run test -- src/module{n}/feature.test.ts"))
        .collect();
    let files: Vec<String> = (0..15)
        .map(|file| format!("src/feature/file{file}.ts"))
        .collect();
    let mut tasks = Vec::new();
    let mut at = 1_000u64;
    for n in 0..12 {
        let task = task_in(h, &run.id, &format!("AGOS-{n}"));
        // Taken after the task exists: creating one needs the store too.
        let store = h.service.store().unwrap();
        let roles = [
            "tests",
            "implementation",
            "review",
            "implementation",
            "review",
            "qa",
            "qa",
        ];
        for (index, role) in roles.iter().enumerate() {
            at += 1;
            let job: DeliveryJob = serde_json::from_value(json!({
                "id": format!("job-{n}-{index}"), "task_id": task.id, "role": role,
                "attempt": 1, "agent": "claude", "access": "workspace",
                "prompt": long("Implementa la incidencia siguiendo el contrato. ", 2400),
                "contract_version": 1, "writes": true, "timeout_minutes": 60,
                "status": "finished", "created_at_ms": at, "started_at_ms": at, "ended_at_ms": at,
                "allowed_commands": commands,
                "decisions": vec![long("Decisión contestada por el usuario. ", 280); 10],
                "changes": files.iter().map(|path| json!({"path": path, "kind": "modified"})).collect::<Vec<_>>(),
                "result": {
                    "status": "pass", "summary": long("Cambios hechos y verificados. ", 800),
                    "changed_paths": files,
                    "checks": vec![json!({"command": "npm test", "result": long("ok ", 150)}); 6],
                    "findings": vec![long("Hallazgo de la revisión. ", 170); 8],
                    "handoff": long("Siguiente paso. ", 650), "blocked_by": "none",
                },
            }))
            .unwrap();
            store.put_job(&job).unwrap();
        }
        for index in 0..5 {
            let decision: DeliveryDecision = serde_json::from_value(json!({
                "id": format!("decision-{n}-{index}"), "task_id": task.id, "kind": "choice",
                "question": long("¿Qué hacemos con el aviso? ", 90),
                "detail": long("Detalle técnico de la decisión. ", 320),
                "options": [
                    {"label": "Sí", "consequence": long("Consecuencia. ", 140), "recommended": true},
                    {"label": "No", "consequence": long("Consecuencia. ", 140), "recommended": false},
                ],
                "status": "answered", "answer": "Sí", "requested_by": "coord",
                "requested_at_ms": at, "decided_by": "user", "decided_at_ms": at,
            }))
            .unwrap();
            store.put_decision(&decision).unwrap();
        }
        // The first six are delivered; every task redid its commit step.
        let mut steps = vec![("commit", None), ("commit", None), ("push", None)];
        if n < 6 {
            steps.extend([
                ("pr", None),
                ("jira", Some("after_pr")),
                ("merge", None),
                ("jira", Some("after_merge")),
            ]);
        }
        for (index, (rung, stage)) in steps.into_iter().enumerate() {
            at += 1;
            let approval: DeliveryApproval = serde_json::from_value(json!({
                "id": format!("approval-{n}-{index}"), "task_id": task.id, "rung": rung, "stage": stage,
                "title": "feat: la incidencia", "body": long("Cuerpo del paso. ", 500),
                "status": "executed", "requested_by": "coord", "requested_at_ms": at,
                "decided_at_ms": at, "executed_at_ms": at, "outcome": "hecho",
                "commits": [{"message": "feat: la incidencia", "paths": files}],
            }))
            .unwrap();
            store.put_approval(&approval).unwrap();
        }
        tasks.push(task);
    }
    tasks
}

fn mcp_tool(h: &Harness, name: &str, arguments: serde_json::Value) -> serde_json::Value {
    super::mcp::handle_message(
        &h.service,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
    )
    .unwrap()["result"]["structuredContent"]
        .clone()
}

#[test]
fn the_coordinator_reads_a_compact_overview_and_jobs_without_their_instructions() {
    let h = harness();
    let tasks = busy_batch(&h);
    let (delivered, active) = (&tasks[0], &tasks[6]);
    let full = mcp_tool(&h, "delivery_overview", json!({"compact": false}));
    let compact = mcp_tool(&h, "delivery_overview", json!({}));
    let (full_size, compact_size) = (full.to_string().len(), compact.to_string().len());
    println!("delivery_overview: {full_size} bytes in full, {compact_size} compact");
    assert!(
        compact_size * 5 < full_size,
        "{full_size} -> {compact_size}"
    );
    assert_eq!(compact["phases"][&delivered.id]["kind"], "delivered");
    assert_eq!(full["jobs"][0]["prompt"].as_str().unwrap().len(), 2400);

    let find = |list: &str, id: &str| -> serde_json::Value {
        compact[list]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == id)
            .unwrap()
            .clone()
    };
    // Ended jobs of a delivered task, and those a later one of the same
    // stage replaced, are only counted.
    let listed = |id: &str| {
        compact["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|job| job["id"] == id)
    };
    assert!(!listed("job-0-2") && !listed("job-6-2"));
    assert_eq!(compact["older_jobs"][&delivered.id], 7);
    assert_eq!(compact["older_jobs"][&active.id], 3);
    // The latest review: what it is, without instructions or long text.
    let current = find("jobs", "job-6-4");
    assert_eq!(current["role"], "review");
    for key in [
        "prompt",
        "allowed_commands",
        "decisions",
        "changes",
        "check_env",
    ] {
        assert!(current.get(key).is_none(), "{key}: {current}");
    }
    assert_eq!(
        current["result"],
        json!({"status": "pass", "summary": current["result"]["summary"], "blocked_by": "none"})
    );
    assert!(current["result"]["summary"]
        .as_str()
        .unwrap()
        .ends_with('…'));
    // A redone commit step keeps only the id of the one it replaced.
    let replaced = find("approvals", "approval-6-0");
    assert_eq!(replaced["status"], "executed");
    assert!(replaced.get("body").is_none());
    assert!(find("approvals", "approval-6-1").get("body").is_some());
    // A delivered task's steps and answered decisions: settled.
    assert!(find("approvals", "approval-0-6").get("body").is_none());
    assert!(find("decisions", "decision-6-0").get("question").is_none());

    // read_job leaves the instructions out unless asked.
    let read = mcp_tool(&h, "read_job", json!({"job_id": "job-6-2"}));
    assert!(read.get("prompt").is_none() && read.get("allowed_commands").is_none());
    assert_eq!(read["result"]["findings"].as_array().unwrap().len(), 8);
    assert_eq!(read["task_id"], active.id.as_str());
    let whole = mcp_tool(&h, "read_job", json!({"job_id": "job-6-2", "full": true}));
    assert_eq!(whole["allowed_commands"].as_array().unwrap().len(), 80);
    assert!(whole["prompt"].as_str().is_some());
}

#[test]
fn the_overview_narrows_to_a_batch_or_task_leaves_closed_batches_out_and_sends_rounds_once() {
    let h = harness();
    let run = |title: &str| {
        h.service
            .create_run(
                h.repo.path().to_path_buf(),
                title,
                Some("coord".into()),
                None,
            )
            .unwrap()
    };
    let (open, other, closed) = (run("Abierto"), run("Otro"), run("Cerrado"));
    let (first, second) = (task_in(&h, &open.id, "K-1"), task_in(&h, &open.id, "K-2"));
    let elsewhere = task_in(&h, &other.id, "K-3");
    let old = task_in(&h, &closed.id, "K-4");
    h.service.close_run(&closed.id).unwrap();
    let items: Vec<String> = (0..3).map(|n| format!("Arregla el punto {n}")).collect();
    h.service
        .store()
        .unwrap()
        .put_round(&super::model::DeliveryRound {
            id: "round-1".into(),
            task_id: first.id.clone(),
            number: 2,
            kind: "review".into(),
            short: "3 hallazgos".into(),
            cause: "La revisión pidió cambios".into(),
            items: items.clone(),
            source: None,
            opened_at_ms: super::now_ms(),
            after_push: false,
            closed_at_ms: None,
            outcome: None,
            waits_merge_commit: false,
        })
        .unwrap();
    let ids = |value: &serde_json::Value, list: &str| -> Vec<String> {
        value[list]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap().to_string())
            .collect()
    };

    // By default every batch but the closed one.
    let all = mcp_tool(&h, "delivery_overview", json!({}));
    assert_eq!(ids(&all, "runs"), [open.id.clone(), other.id.clone()]);
    assert!(all["phases"].get(&old.id).is_none());
    assert!(all["phases"].get(&elsewhere.id).is_some());
    // The round's items come once; the phase names it.
    assert_eq!(
        all["phases"][&first.id]["round"],
        json!({"id": "round-1", "number": 2, "kind": "review", "at": all["phases"][&first.id]["round"]["at"]})
    );
    assert_eq!(ids(&all, "rounds"), ["round-1"]);
    assert_eq!(all["rounds"][0]["items"], json!(items));
    let closed_too = mcp_tool(&h, "delivery_overview", json!({"closed": true}));
    assert!(closed_too["phases"].get(&old.id).is_some());

    // One batch, one task, some parts.
    let batch = mcp_tool(&h, "delivery_overview", json!({"run_id": open.id}));
    assert_eq!(ids(&batch, "tasks"), [first.id.clone(), second.id.clone()]);
    let task = mcp_tool(
        &h,
        "delivery_overview",
        json!({"task_id": second.id, "fields": ["tasks", "phases", "runs"]}),
    );
    assert_eq!(ids(&task, "tasks"), std::slice::from_ref(&second.id));
    assert_eq!(ids(&task, "runs"), std::slice::from_ref(&open.id));
    let mut parts: Vec<&String> = task.as_object().unwrap().keys().collect();
    parts.sort();
    assert_eq!(parts, ["phases", "runs", "tasks"]);
}

#[test]
fn the_coordinator_is_not_told_of_its_own_task_edits_but_is_of_the_users() {
    let h = harness();
    let run = h
        .service
        .create_run(
            h.repo.path().to_path_buf(),
            "Lote",
            Some("coord".into()),
            None,
        )
        .unwrap();
    let task = task_in(&h, &run.id, "AGOS-7");
    super::mcp::handle_message_from(
        &h.service,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "set_task_state", "arguments": {
            "run_id": run.id, "owner": "coord", "generation": run.generation,
            "task_id": task.id, "state": "en revisión", "contract_version": 2,
        }}}),
        Some("session-1"),
    )
    .unwrap();
    assert_eq!(h.service.task(&task.id).unwrap().state, "en revisión");
    let (events, _) = h.service.coordinator_events(&run.id, 0).unwrap();
    assert_eq!(events, Vec::<serde_json::Value>::new());
    let later = super::now_ms() + super::feed::WAKE_QUIET_MS + 1;
    assert!(h.service.pending_wakeups(later).unwrap().is_empty());

    h.service
        .update_task(&task.id, Some("esperando diseño".into()), None, None, true)
        .unwrap();
    let (events, _) = h.service.coordinator_events(&run.id, 0).unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["kind"], "task_updated");
    assert_eq!(
        events[0]["text"],
        "AGOS-7: el usuario editó la tarea: estado «esperando diseño»"
    );
    // Quiet long enough counting from the edit, not from before it.
    let later = super::now_ms() + super::feed::WAKE_QUIET_MS + 1;
    let wakeups = h.service.pending_wakeups(later).unwrap();
    assert_eq!(wakeups.len(), 1);
    assert!(wakeups[0].text.contains("el usuario editó la tarea"));
}

#[test]
#[ignore]
fn zz_measure_live_copy() {
    let path = std::env::var("LIVE_DB").unwrap();
    let h = harness_with_store(DeliveryStore::open(std::path::Path::new(&path)).unwrap());
    let full = mcp_tool(&h, "delivery_overview", json!({"compact": false}));
    let compact = mcp_tool(&h, "delivery_overview", json!({}));
    println!(
        "LIVE full={} compact={}",
        full.to_string().len(),
        compact.to_string().len()
    );
    let pretty = |value: &serde_json::Value| serde_json::to_string_pretty(value).unwrap().len();
    println!(
        "LIVE pretty full={} compact={}",
        pretty(&full),
        pretty(&compact)
    );
    for key in [
        "jobs",
        "approvals",
        "decisions",
        "tasks",
        "phases",
        "prs",
        "qa_problems",
        "settings_proposals",
        "runs",
    ] {
        println!(
            "LIVE {key}: {} -> {}",
            full[key].to_string().len(),
            compact[key].to_string().len()
        );
    }
}

#[test]
#[ignore]
fn zz_measure_replies() {
    let reply = |h: &Harness, compact: bool| {
        super::mcp::handle_message(
            &h.service,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "delivery_overview", "arguments": {"compact": compact}}}),
        )
        .unwrap()
        .to_string()
        .len()
    };
    let h = harness();
    busy_batch(&h);
    println!(
        "FIXTURE reply full={} compact={}",
        reply(&h, false),
        reply(&h, true)
    );
    if let Ok(path) = std::env::var("LIVE_DB") {
        let h = harness_with_store(DeliveryStore::open(std::path::Path::new(&path)).unwrap());
        println!(
            "LIVE reply full={} compact={}",
            reply(&h, false),
            reply(&h, true)
        );
    }
}

fn set_repo_checks(h: &Harness, checks: &[&str]) {
    let store = h.service.store().unwrap();
    let mut settings = store.repo_settings(h.repo.path()).unwrap();
    settings.checks = checks.iter().map(|check| check.to_string()).collect();
    store.set_repo_settings(h.repo.path(), &settings).unwrap();
}

/// The task's check jobs, once there are `count` and all ended.
fn ended_checks(h: &Harness, task: &DeliveryTask, count: usize) -> Vec<DeliveryJob> {
    ended_jobs(h, task, "checks", count)
}

/// The task's jobs of `role`, once there are `count` and all ended.
fn ended_jobs(h: &Harness, task: &DeliveryTask, role: &str, count: usize) -> Vec<DeliveryJob> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let checks: Vec<DeliveryJob> = h
            .service
            .store()
            .unwrap()
            .task_jobs(&task.id)
            .unwrap()
            .into_iter()
            .filter(|job| job.role == role)
            .collect();
        if checks.len() == count && checks.iter().all(|job| !job.status.is_open()) {
            return checks;
        }
        let statuses: Vec<_> = checks.iter().map(|job| job.status).collect();
        assert!(
            Instant::now() < deadline,
            "{role} jobs did not end: {statuses:?}"
        );
        std::thread::sleep(POLL);
    }
}

#[test]
fn tinto_runs_the_task_checks_after_a_change_and_sends_a_failure_back_to_implementation() {
    let h = harness();
    set_repo_checks(&h, &["exit=0", "exit=1 events=codex-failed"]);
    let task = h.task("K-50");
    let error = h
        .service
        .set_task_checks(&task.id, vec!["rm -rf /".into()])
        .unwrap_err();
    assert_eq!(error.category, "unknown_check");
    let task = h
        .service
        .set_task_checks(
            &task.id,
            vec![
                " exit=0 ".into(),
                "exit=1 events=codex-failed".into(),
                "exit=0".into(),
            ],
        )
        .unwrap();
    assert_eq!(task.checks, ["exit=0", "exit=1 events=codex-failed"]);

    // The tests stage's new tests fail until the implementation: no checks.
    let tests = h.dispatch(&task, "tests", "events=codex-ok touch=t.txt");
    h.wait(&tests.id);
    let implementation = h.dispatch(&task, "implementation", "events=codex-ok touch=code.txt");
    let implementation = h.wait(&implementation.id);
    assert!(implementation.task_checks[1].starts_with("While you work, run only the tests"));
    let checks = ended_checks(&h, &task, 2);
    assert!(checks
        .iter()
        .all(|check| check.created_at_ms >= implementation.ended_at_ms.unwrap()));
    assert_eq!(checks[0].prompt, "exit=0");
    assert_eq!(checks[0].status, DeliveryJobStatus::Finished);
    assert_eq!(checks[1].status, DeliveryJobStatus::Failed);
    // Each check counts its own attempts.
    assert!(checks.iter().all(|check| check.attempt == 1));

    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(phase.next_stage.as_deref(), Some("implementation"));
    // The coordinator hears of them once, when the last one ended.
    let events = h.service.store().unwrap().events_after(0).unwrap();
    let ended: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "checks_ended")
        .collect();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].detail, "fallaron `exit=1 events=codex-failed`");
    assert!(!events.iter().any(|event| event.kind == "job_ended"
        && checks
            .iter()
            .any(|check| event.job_id.as_ref() == Some(&check.id))));

    // The fix starts from that code and reads why it failed.
    let fix = h.dispatch(&task, "implementation", "events=codex-ok");
    let note = h.wait(&fix.id).task_checks.join("\n");
    assert!(
        note.contains("These failed on the code you start from:"),
        "{note}"
    );
    assert!(note.contains("- `exit=1 events=codex-failed`"), "{note}");
    assert!(note.contains("is not supported when using Codex"), "{note}");
    assert!(!note.contains("- `exit=0`"), "{note}");
}

#[test]
fn a_stage_runs_again_only_the_checks_its_changes_reach() {
    let h = harness();
    for dir in ["web", "api"] {
        std::fs::create_dir_all(h.repo.path().join(dir)).unwrap();
        std::fs::write(h.repo.path().join(dir).join("keep.txt"), "keep\n").unwrap();
    }
    git(h.repo.path(), &["add", "--all"]);
    git(h.repo.path(), &["commit", "-q", "-m", "packages"]);
    let (web, api) = (
        "exit=0 npm --prefix web test",
        "exit=0 npm --prefix api test",
    );
    set_repo_checks(&h, &[web, api]);
    let task = h.task("K-51");
    h.service
        .set_task_checks(&task.id, vec![web.into(), api.into()])
        .unwrap();

    // The first change runs every check once.
    let first = h.dispatch(&task, "implementation", "events=codex-ok touch=web/a.txt");
    h.wait(&first.id);
    ended_checks(&h, &task, 2);
    // Then a change in one package runs that package's check only…
    let second = h.dispatch(&task, "implementation", "events=codex-ok touch=web/b.txt");
    h.wait(&second.id);
    assert_eq!(ended_checks(&h, &task, 3)[2].prompt, web);
    // …and one outside every package's folder runs them all.
    let root = h.dispatch(&task, "implementation", "events=codex-ok touch=root.txt");
    h.wait(&root.id);
    ended_checks(&h, &task, 5);
    let jobs = h.service.store().unwrap().task_jobs(&task.id).unwrap();
    let jobs: Vec<&DeliveryJob> = jobs.iter().collect();
    let current = super::phase::current_checks(&jobs);
    assert_eq!(current.len(), 2);
    assert!(current
        .iter()
        .all(|check| check.created_at_ms > root.created_at_ms));
}

#[test]
fn a_writing_stage_runs_the_format_and_lint_checks_before_it_ends() {
    let h = harness();
    set_repo_checks(&h, &["npm test", "npm run lint"]);
    let task = h.task("K-50");
    h.service
        .set_task_checks(&task.id, vec!["npm test".into(), "npm run lint".into()])
        .unwrap();
    let implementation = h.dispatch(&task, "implementation", "events=codex-ok");
    let told = h.wait(&implementation.id).task_checks;
    assert!(
        told.contains(&"Before you finish, run the format and lint checks and fix what they report: `npm run lint`.".to_string()),
        "{told:?}"
    );
}

#[test]
fn a_stage_after_passing_checks_is_told_not_to_run_them_again() {
    let h = harness();
    set_repo_checks(&h, &["exit=0"]);
    let task = h.task("K-51");
    h.service
        .set_task_checks(&task.id, vec!["exit=0".into()])
        .unwrap();
    let tests = h.dispatch(&task, "tests", "events=codex-ok touch=t.txt");
    h.wait(&tests.id);
    let implementation = h.dispatch(&task, "implementation", "events=codex-ok touch=code.txt");
    h.wait(&implementation.id);
    assert_eq!(
        ended_checks(&h, &task, 1)[0].status,
        DeliveryJobStatus::Finished
    );
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(phase.next_stage.as_deref(), Some("review"));

    let review = h.dispatch(&task, "review", "events=codex-ok");
    assert_eq!(
        h.wait(&review.id).task_checks,
        [
            "Tinto runs this task's checks itself on what each stage changes: `exit=0`.",
            "Do not run these checks, builds, linters, type checkers or test suites to review: Tinto runs them on every stage's result and sends failures back. Review by reading the change; run a test only to confirm a specific doubt.",
            "They all passed on the code you start from: do not run them again.",
        ]
    );
    let events = h.service.store().unwrap().events_after(0).unwrap();
    assert!(events
        .iter()
        .any(|event| event.kind == "checks_ended" && event.detail == "pasaron"));
}

fn set_repo_setup(h: &Harness, setup: &[&str]) {
    let store = h.service.store().unwrap();
    let mut settings = store.repo_settings(h.repo.path()).unwrap();
    settings.setup = setup.iter().map(|command| command.to_string()).collect();
    store.set_repo_settings(h.repo.path(), &settings).unwrap();
}

#[test]
fn setup_runs_before_any_stage_and_a_failed_command_holds_them_until_retried() {
    use super::phase::{Phase, Waiter};
    let h = harness();
    set_repo_setup(&h, &["touch=deps.txt", "exit=1 events=codex-failed"]);
    let task = h.task("K-60");
    let error = h
        .service
        .set_task_setup(&task.id, vec!["npm ci".into()])
        .unwrap_err();
    assert_eq!(error.category, "unknown_setup");
    let task = h
        .service
        .set_task_setup(
            &task.id,
            vec![
                "touch=deps.txt".into(),
                " exit=1 events=codex-failed ".into(),
            ],
        )
        .unwrap();
    assert_eq!(task.setup, ["touch=deps.txt", "exit=1 events=codex-failed"]);
    // A stage dispatched meanwhile waits for the setup.
    let tests = h.dispatch(&task, "tests", "events=codex-ok touch=t.txt");
    let setup = ended_jobs(&h, &task, "setup", 2);
    assert_eq!(setup[0].status, DeliveryJobStatus::Finished);
    assert_eq!(setup[1].status, DeliveryJobStatus::Failed);
    assert!(task.worktree.join("deps.txt").is_file());
    assert_eq!(
        h.service.job(&tests.id).unwrap().status,
        DeliveryJobStatus::Queued
    );
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(
        phase.phase,
        Phase::SetupFailed {
            job_id: setup[1].id.clone(),
            command: "exit=1 events=codex-failed".into(),
        }
    );
    assert_eq!(phase.waits_on, Waiter::Coordinator);
    assert_eq!(phase.next_stage, None);
    // The coordinator hears of it once, when the last command ended.
    let events = h.service.store().unwrap().events_after(0).unwrap();
    let ended: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "setup_ended")
        .collect();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].detail, "fallaron `exit=1 events=codex-failed`");

    // A retry is that command's second attempt; the stage still waits.
    let retry = h.service.retry(&setup[1].id, None).unwrap();
    assert_eq!(retry.attempt, 2);
    assert_eq!(h.wait(&retry.id).status, DeliveryJobStatus::Failed);
    assert_eq!(
        h.service.job(&tests.id).unwrap().status,
        DeliveryJobStatus::Queued
    );
    // Without that command the stage starts; what passed does not run again.
    h.service
        .set_task_setup(&task.id, vec!["touch=deps.txt".into()])
        .unwrap();
    assert_eq!(h.wait(&tests.id).status, DeliveryJobStatus::Finished);
    assert_eq!(ended_jobs(&h, &task, "setup", 3).len(), 3);
}

#[test]
fn jobs_that_need_the_environment_wait_while_it_fails_and_start_once_it_answers() {
    use super::environment::DeliveryEnvironmentCheck;
    use super::phase::{Phase, Waiter};
    use std::sync::atomic::{AtomicBool, Ordering};
    let h = harness();
    let up = Arc::new(AtomicBool::new(false));
    let answers = up.clone();
    h.service.set_environment_check(Arc::new(move |_, _, _| {
        let ok = answers.load(Ordering::SeqCst);
        vec![DeliveryEnvironmentCheck {
            name: "Docker".into(),
            ok,
            output: if ok {
                "Docker 27 responde."
            } else {
                "no responde"
            }
            .into(),
        }]
    }));
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("E-1");
    let checked = coordinator.call("check_environment", json!({"task_id": task_id}));
    assert_eq!(checked["structuredContent"]["ok"], false, "{checked}");
    assert_eq!(checked["structuredContent"]["checks"][0]["name"], "Docker");
    let unknown = coordinator.write(
        "set_task_state",
        json!({"task_id": task_id, "setup": ["npm ci"]}),
    );
    assert!(error_text(&unknown).contains("comandos de preparación"));
    let task = h.service.task(&task_id).unwrap();

    // QA waits instead of failing, and says why.
    let qa = h.dispatch(&task, "qa", "events=codex-ok");
    let deadline = Instant::now() + Duration::from_secs(30);
    let waiting = loop {
        let job = h.service.job(&qa.id).unwrap();
        if job.waits_environment.is_some() {
            break job;
        }
        assert!(Instant::now() < deadline, "QA did not wait: {job:?}");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(waiting.status, DeliveryJobStatus::Queued);
    assert!(waiting.needs_environment);
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(
        phase.phase,
        Phase::WaitsEnvironment {
            job_id: qa.id.clone(),
            role: "qa".into(),
            problem: "Docker: no responde".into(),
        }
    );
    assert_eq!(phase.waits_on, Waiter::User);
    // A job that does not need it runs meanwhile.
    let other = h.task("E-2");
    let plain = h.dispatch(&other, "implementation", "events=codex-ok");
    assert_eq!(h.wait(&plain.id).status, DeliveryJobStatus::Finished);
    assert_eq!(
        h.service.job(&qa.id).unwrap().status,
        DeliveryJobStatus::Queued
    );

    // Once it answers, the waiting job starts.
    up.store(true, Ordering::SeqCst);
    h.service
        .check_environment(&task.repo, task.distro.as_deref())
        .unwrap();
    let qa = h.wait(&qa.id);
    assert_eq!(qa.status, DeliveryJobStatus::Finished);
    assert_eq!(qa.waits_environment, None);
    let events = h.service.store().unwrap().events_after(0).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == "job_waits_environment")
            .count(),
        1
    );
}

#[test]
fn a_broken_stage_is_settled_with_a_note_and_the_task_moves_on() {
    use super::phase::{Phase, Waiter};
    let h = harness();
    let fence = coordinated_run(&h);
    let created = tool(
        &h,
        "create_task",
        with(
            &fence,
            json!({"repo": plain_path(h.repo.path()), "key": "R-1", "title": "x"}),
        ),
    );
    let task = h
        .service
        .task(created["structuredContent"]["id"].as_str().unwrap())
        .unwrap();
    let job = h.dispatch(&task, "implementation", "events=codex-failed touch=a.txt");
    let job = h.wait(&job.id);
    assert_eq!(job.status, DeliveryJobStatus::Failed);
    let phase = || h.service.overview().unwrap().phases[&task.id].clone();
    assert!(matches!(phase().phase, Phase::Broken { .. }));
    assert_eq!(phase().next_stage.as_deref(), Some("implementation"));

    // A note is required, in one line.
    let resolve = |note: &str| {
        tool(
            &h,
            "resolve_job",
            with(&fence, json!({"job_id": job.id, "note": note})),
        )
    };
    assert_eq!(resolve("  ")["isError"], true);
    let resolved = resolve("Otra sesión terminó\nla implementación")["structuredContent"].clone();
    assert_eq!(
        resolved["resolved_note"],
        "Otra sesión terminó la implementación"
    );
    // The alert clears and the stage counts as passed: the task goes on.
    assert!(!matches!(phase().phase, Phase::Broken { .. }));
    assert_eq!(phase().waits_on, Waiter::Coordinator);
    assert_eq!(phase().next_stage.as_deref(), Some("tests"));
    let job = h.service.job(&job.id).unwrap();
    assert!(job.resolved_at_ms.is_some());
    assert_eq!(job.changes.len(), 1, "what it left stays");
    // Only once, and only for a broken stage.
    assert_eq!(resolve("otra vez")["isError"], true);
    let store = h.service.store().unwrap();
    assert_eq!(
        store.event_details("job_resolved").unwrap(),
        ["Otra sesión terminó la implementación"]
    );
    drop(store);

    // The user settles too; that one wakes the coordinator.
    let again = h.dispatch(&task, "implementation", "events=codex-failed");
    h.wait(&again.id);
    assert!(matches!(phase().phase, Phase::Broken { .. }));
    let err = h.service.resolve(&again.id, "", "user").unwrap_err();
    assert_eq!(err.category, "invalid_note");
    h.service
        .resolve(&again.id, "Lo arreglé a mano", "user")
        .unwrap();
    assert!(!matches!(phase().phase, Phase::Broken { .. }));
    assert_eq!(
        h.service
            .store()
            .unwrap()
            .event_details("job_resolved_by_user")
            .unwrap(),
        ["Lo arreglé a mano"]
    );
}

#[test]
fn a_drifted_push_clears_once_the_remote_branch_has_what_tinto_published() {
    use super::phase::{Phase, Waiter};
    let h = harness();
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare"]);
    git(
        h.repo.path(),
        &["remote", "add", "origin", &plain_path(remote.path())],
    );
    let task = h.task("P-9");
    let worktree = PathBuf::from(plain_path(&task.worktree));
    std::fs::write(worktree.join("feature.txt"), "x").unwrap();
    let request = |rung, title: &str| NewApproval {
        task_id: task.id.clone(),
        rung,
        title: title.to_string(),
        body: String::new(),
        requested_by: "user".to_string(),
        commits: Vec::new(),
        leave_out: Vec::new(),
        single_commit_reason: None,
        merge_method: None,
        stage: None,
        jira_statuses: Vec::new(),
        comment_url: None,
        resolve_thread: false,
    };
    let commit = h
        .service
        .request_approval(request(DeliveryRung::Commit, "Add feature"))
        .unwrap();
    h.service.decide_approval(&commit.id, true, None).unwrap();
    let push = h
        .service
        .request_approval(request(DeliveryRung::Push, "Publish"))
        .unwrap();
    let push = h.service.decide_approval(&push.id, true, None).unwrap();
    assert_eq!(push.status, DeliveryApprovalStatus::Executed);
    let status = || h.service.approval(&push.id).unwrap().status;

    // The remote branch goes back: drifted, and the coordinator is told
    // what puts it right.
    let branch = format!("refs/heads/{}", task.branch);
    git(remote.path(), &["update-ref", &branch, &task.base_commit]);
    h.service.check_pushes(true).unwrap();
    assert_eq!(status(), DeliveryApprovalStatus::Drifted);
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert!(matches!(phase.phase, Phase::Drifted { .. }));
    assert_eq!(phase.waits_on, Waiter::Coordinator);
    let detail = h
        .service
        .store()
        .unwrap()
        .event_details("step_drifted")
        .unwrap();
    assert!(
        detail[0].contains("La rama remota cambió fuera de Tinto")
            && detail[0].contains("vuelve a pedir el paso push"),
        "{detail:?}"
    );

    // Put right outside Tinto with one more commit on top: the remote has
    // what Tinto published, so the push matches again on the next pass.
    std::fs::write(worktree.join("more.txt"), "y").unwrap();
    git(&worktree, &["add", "more.txt"]);
    git(&worktree, &["commit", "-q", "-m", "more"]);
    git(&worktree, &["push", "-q", "origin", &task.branch]);
    h.service.check_pushes(false).unwrap();
    assert_eq!(status(), DeliveryApprovalStatus::Executed);
    assert!(!matches!(
        h.service.overview().unwrap().phases[&task.id].phase,
        Phase::Drifted { .. }
    ));

    // Someone else pushes ahead from another clone: Tinto fetches what it
    // does not know and the push still matches.
    let other = tempfile::tempdir().unwrap();
    git(
        other.path(),
        &[
            "clone",
            "-q",
            "--branch",
            &task.branch,
            &plain_path(remote.path()),
            ".",
        ],
    );
    std::fs::write(other.path().join("theirs.txt"), "z").unwrap();
    git(other.path(), &["add", "theirs.txt"]);
    git(other.path(), &["commit", "-q", "-m", "theirs"]);
    git(other.path(), &["push", "-q", "origin", &task.branch]);
    h.service.check_pushes(true).unwrap();
    assert_eq!(status(), DeliveryApprovalStatus::Executed);

    // A rewrite that drops it is a drift again.
    git(&worktree, &["reset", "-q", "--hard", &task.base_commit]);
    std::fs::write(worktree.join("other.txt"), "w").unwrap();
    git(&worktree, &["add", "other.txt"]);
    git(&worktree, &["commit", "-q", "-m", "rewritten"]);
    git(&worktree, &["push", "-q", "-f", "origin", &task.branch]);
    h.service.check_pushes(true).unwrap();
    assert_eq!(status(), DeliveryApprovalStatus::Drifted);
}

#[test]
fn a_commit_waits_until_the_qa_findings_are_fixed_or_accepted() {
    let h = harness();
    let task = h.task("QF-1");
    write_files(&task, &["a.txt"]);
    // The stages ran: QA ended with findings nobody answered.
    for (index, (role, status)) in [("implementation", "pass"), ("qa", "findings")]
        .into_iter()
        .enumerate()
    {
        let job: DeliveryJob = serde_json::from_value(json!({
            "id": format!("qf-{index}"), "task_id": task.id, "role": role,
            "attempt": 1, "agent": "claude", "access": "workspace", "prompt": "x",
            "contract_version": 1, "writes": role != "qa", "timeout_minutes": 60,
            "status": "finished", "created_at_ms": index + 1, "started_at_ms": index + 1,
            "ended_at_ms": index + 1, "result_state": "accepted",
            "result": {
                "status": status, "summary": "s", "changed_paths": [], "checks": [],
                "findings": if status == "findings" { vec!["El aviso tapa el botón", "Falta el texto"] } else { vec![] },
                "handoff": "", "blocked_by": "none",
            },
        }))
        .unwrap();
        h.service.store().unwrap().put_job(&job).unwrap();
    }
    let plan = || commit_plan(&task, vec![commit("feat: a", &["a.txt"])], &[]);
    let error = h.service.request_approval(plan()).unwrap_err();
    assert_eq!(error.category, "stages_unsettled", "{error}");
    assert!(
        error
            .message
            .contains("qa terminó con 2 hallazgos sin atender"),
        "{error}"
    );
    assert!(error.message.contains("resolve_job"), "{error}");
    // The user may still commit as they judge.
    let mut by_user = plan();
    by_user.requested_by = "user".into();
    assert!(!matches!(
        h.service.request_approval(by_user),
        Err(error) if error.category == "stages_unsettled"
    ));

    // Accepted as they are, with a reason: the commit can be asked for.
    let accepted = h
        .service
        .resolve(
            "qf-1",
            "Observaciones de estilo; no bloquean la incidencia",
            "coordinator",
        )
        .unwrap();
    assert!(accepted.resolved_at_ms.is_some());
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_ne!(phase.next_stage.as_deref(), Some("implementation"));
    for approval in h.service.overview().unwrap().approvals {
        let _ = h.service.withdraw_approval(&approval.id, "otra prueba");
    }
    assert!(!matches!(
        h.service.request_approval(plan()),
        Err(error) if error.category == "stages_unsettled"
    ));
}

// ---- rounds ----

/// A stage of the task as it ended, written to the store the way `finish`
/// leaves it, rounds included.
fn put_stage(
    h: &Harness,
    task: &DeliveryTask,
    id: &str,
    role: &str,
    status: &str,
    findings: &[&str],
    changed: bool,
) -> DeliveryJob {
    // Each stage after the one before it, and after any round it opened.
    std::thread::sleep(Duration::from_millis(5));
    let at = super::now_ms();
    // Started before it ended, like a real job: the round it opens at its
    // end is never one it worked on, even within the same millisecond.
    let job: DeliveryJob = serde_json::from_value(json!({
        "id": id, "task_id": task.id, "role": role, "attempt": 1, "agent": "claude",
        "access": "workspace", "prompt": "x", "contract_version": 1,
        "writes": matches!(role, "tests" | "implementation"), "timeout_minutes": 60,
        "status": "finished", "created_at_ms": at - 1, "started_at_ms": at - 1, "ended_at_ms": at,
        "result_state": "accepted",
        "changes": if changed { json!([{"path": "a.txt", "kind": "modified"}]) } else { json!([]) },
        "result": {
            "status": status, "summary": "s", "changed_paths": [], "checks": [],
            "findings": findings, "handoff": "", "blocked_by": "none",
        },
    }))
    .unwrap();
    let store = h.service.store().unwrap();
    store.put_job(&job).unwrap();
    super::rounds::after_job(&store, &job).unwrap();
    drop(store);
    std::thread::sleep(Duration::from_millis(5));
    job
}

/// A delivery step done now, saved the way Tinto saves it.
fn step_done_now(h: &Harness, task: &DeliveryTask, id: &str, rung: &str) {
    let now = super::now_ms();
    let step: super::model::DeliveryApproval = serde_json::from_value(json!({
        "id": id, "task_id": task.id, "rung": rung, "title": rung, "body": "",
        "status": "executed", "requested_by": "coord", "requested_at_ms": now,
        "decided_at_ms": now, "executed_at_ms": now, "outcome": "hecho",
    }))
    .unwrap();
    h.service
        .save_approval(&step, "approval_completed")
        .unwrap();
}

fn round_now(h: &Harness, task: &DeliveryTask) -> Option<super::phase::CurrentRound> {
    h.service.overview().unwrap().phases[&task.id].round.clone()
}

#[test]
fn each_send_back_opens_a_round_that_closes_once_its_work_is_committed() {
    use super::phase::Phase;
    let h = harness();
    let task = h.task("R-1");
    // The tests' failing cases are part of the initial delivery.
    put_stage(
        &h,
        &task,
        "t1",
        "tests",
        "findings",
        &["falta el caso vacío"],
        true,
    );
    put_stage(&h, &task, "i1", "implementation", "pass", &[], true);
    assert!(round_now(&h, &task).is_none());

    put_stage(
        &h,
        &task,
        "r1",
        "review",
        "findings",
        &["Falta validar la fecha", "El nombre confunde"],
        false,
    );
    let round = round_now(&h, &task).expect("review findings open a round");
    assert_eq!(round.round.number, 2);
    assert_eq!(round.round.kind, "review");
    assert_eq!(round.round.cause, "2 hallazgos de revisión");
    assert_eq!(
        round.round.items,
        ["Falta validar la fecha", "El nombre confunde"]
    );
    assert_eq!(round.at, "implementation");
    assert!(!round.round.after_push);
    let opened = h
        .service
        .store()
        .unwrap()
        .event_details("round_opened")
        .unwrap();
    assert_eq!(
        opened,
        ["Ronda 2 · 2 hallazgos de revisión: 1) Falta validar la fecha; 2) El nombre confunde"]
    );
    // The stage dispatched in it is told what to address.
    let note = super::rounds::note(&h.service.store().unwrap(), &task.id).unwrap();
    assert_eq!(
        note,
        [
            "This is round 2 of the task: its work came back because of this: 2 hallazgos de revisión.",
            "What this round has to address:",
            "- Falta validar la fecha",
            "- El nombre confunde",
        ]
    );

    // Fixed and reviewed, then QA sends it back: round 3, round 2 corrected.
    put_stage(&h, &task, "i2", "implementation", "pass", &[], true);
    put_stage(&h, &task, "r2", "review", "pass", &[], false);
    put_stage(
        &h,
        &task,
        "q1",
        "qa",
        "findings",
        &["El aviso tapa el botón"],
        false,
    );
    let rounds = h.service.overview().unwrap().rounds;
    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[0].outcome.as_deref(), Some("corregidos"));
    assert!(rounds[0].closed_at_ms.is_some());
    let round = round_now(&h, &task).unwrap();
    assert_eq!((round.round.number, round.round.kind.as_str()), (3, "qa"));
    assert_eq!(round.round.cause, "1 hallazgo de QA");

    // Its stages pass again: the round still wants its commit.
    put_stage(&h, &task, "i3", "implementation", "pass", &[], true);
    put_stage(&h, &task, "r3", "review", "pass", &[], false);
    put_stage(&h, &task, "q2", "qa", "pass", &[], false);
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(
        phase.phase,
        Phase::NextStep {
            rung: DeliveryRung::Commit,
            stage: None
        }
    );
    assert_eq!(phase.round.unwrap().at, "commit");
    step_done_now(&h, &task, "c1", "commit");
    assert!(round_now(&h, &task).is_none());
    let rounds = h.service.overview().unwrap().rounds;
    assert_eq!(rounds[1].outcome.as_deref(), Some("corregidos"));
    let closed = h
        .service
        .store()
        .unwrap()
        .event_details("round_closed")
        .unwrap();
    assert_eq!(closed.len(), 2);
}

#[test]
fn qa_problems_sent_to_fix_refine_the_round_their_qa_opened() {
    let h = harness();
    let task = h.task("R-2");
    put_stage(&h, &task, "i1", "implementation", "pass", &[], true);
    put_stage(&h, &task, "q1", "qa", "findings", &["P1", "P2"], false);
    let problem: super::model::DeliveryQaProblem = serde_json::from_value(json!({
        "id": "p1", "task_id": task.id, "job_id": "q1", "attempt": 1, "number": 1,
        "label": "P1", "text": "El aviso tapa el botón", "status": "open",
        "created_at_ms": 1, "updated_at_ms": 1,
    }))
    .unwrap();
    h.service.store().unwrap().put_qa_problem(&problem).unwrap();
    h.service
        .send_qa_problems(
            &task.id,
            &[super::qa_problems::QaProblemChoice {
                problem_id: "p1".into(),
                solution: "Subir el aviso".into(),
            }],
        )
        .unwrap();
    let round = round_now(&h, &task).unwrap();
    assert_eq!(round.round.number, 2, "the same round, not a new one");
    assert_eq!(round.round.kind, "qa_problems");
    assert_eq!(round.round.cause, "1 problema de QA para corregir");
    assert_eq!(
        round.round.items,
        ["#1 P1: El aviso tapa el botón → Subir el aviso"]
    );
    assert_eq!(h.service.overview().unwrap().rounds.len(), 1);
    let kinds = h.service.store().unwrap().event_kinds().unwrap();
    assert!(kinds.contains(&"round_changed".to_string()), "{kinds:?}");
}

#[test]
fn accepted_findings_close_their_round() {
    let h = harness();
    let task = h.task("R-3");
    put_stage(&h, &task, "i1", "implementation", "pass", &[], true);
    put_stage(
        &h,
        &task,
        "q1",
        "qa",
        "findings",
        &["Un detalle de estilo"],
        false,
    );
    assert!(round_now(&h, &task).is_some());
    h.service
        .resolve("q1", "Estilo; no bloquea la incidencia", "coordinator")
        .unwrap();
    assert!(round_now(&h, &task).is_none());
    let rounds = h.service.overview().unwrap().rounds;
    assert_eq!(rounds[0].outcome.as_deref(), Some("aceptados"));
}

#[test]
fn a_failing_check_opens_a_round_the_fix_is_told_about() {
    let h = harness();
    set_repo_checks(&h, &["exit=1"]);
    let task = h.task("R-4");
    h.service
        .set_task_checks(&task.id, vec!["exit=1".into()])
        .unwrap();
    let implementation = h.dispatch(&task, "implementation", "events=codex-ok touch=code.txt");
    h.wait(&implementation.id);
    assert_eq!(
        ended_checks(&h, &task, 1)[0].status,
        DeliveryJobStatus::Failed
    );
    // There as soon as the check is seen ended.
    let round = round_now(&h, &task).expect("a failing check opens a round");
    assert_eq!(round.round.kind, "checks");
    assert_eq!(
        round.round.cause,
        "Fallan comprobaciones de Tinto: `exit=1`"
    );
    assert_eq!(round.at, "implementation");

    let fix = h.dispatch(&task, "implementation", "events=codex-ok");
    let fix = h.wait(&fix.id);
    assert_eq!(
        fix.round[0],
        "This is round 2 of the task: its work came back because of this: Fallan comprobaciones de Tinto: `exit=1`."
    );
    // It changed nothing, so the check still fails on the code: same round.
    let round = round_now(&h, &task).unwrap();
    assert_eq!(
        (round.round.number, round.at.as_str()),
        (2, "implementation")
    );
    assert_eq!(h.service.overview().unwrap().rounds.len(), 1);
}

#[test]
fn a_round_whose_stages_pass_without_changes_has_nothing_to_publish() {
    let h = harness();
    let task = h.task("R-8");
    put_stage(&h, &task, "t1", "tests", "pass", &[], true);
    put_stage(&h, &task, "i1", "implementation", "pass", &[], true);
    put_stage(
        &h,
        &task,
        "r1",
        "review",
        "findings",
        &["¿Falta un caso?"],
        false,
    );
    // The implementation finds it already covered; review and QA agree.
    put_stage(&h, &task, "i2", "implementation", "pass", &[], false);
    put_stage(&h, &task, "r2", "review", "pass", &[], false);
    assert!(round_now(&h, &task).is_some(), "QA has not run yet");
    put_stage(&h, &task, "q1", "qa", "pass", &[], false);
    assert!(round_now(&h, &task).is_none());
    let rounds = h.service.overview().unwrap().rounds;
    assert_eq!(
        rounds[0].outcome.as_deref(),
        Some("sin cambios que publicar")
    );
}

#[test]
fn changes_asked_on_the_pr_open_a_round_that_repeats_commit_and_push() {
    use super::phase::Phase;
    let h = harness();
    let task = h.task("R-5");
    for (rung, id) in [("commit", "c0"), ("push", "p0"), ("pr", "pr0")] {
        put_step(&h, &task, id, rung, "executed", "hecho");
    }
    put_stage(&h, &task, "t1", "tests", "pass", &[], true);
    put_stage(&h, &task, "i1", "implementation", "pass", &[], true);
    put_stage(&h, &task, "r1", "review", "pass", &[], false);
    put_stage(&h, &task, "q1", "qa", "pass", &[], false);
    let pr: super::prs::DeliveryPr = serde_json::from_value(json!({
        "task_id": task.id, "url": "https://github.com/o/r/pull/308", "repo": "o/r",
        "number": 308, "state": "open", "draft": false, "conflicts": false,
        "review_decision": "changes_requested",
        "checks": {"passing": 1, "failing": 0, "pending": 0},
        "new_reviews": 1, "new_comments": 1, "new_authors": ["ana"],
        "news": ["ana pidió cambios: Renombra el campo", "ana: Falta un test"],
        "latest_activity": "2026-10-22T10:00:00Z", "seen_until": "2026-10-21T00:00:00Z",
        "checked_at_ms": 1, "error": null, "changes_requested_at_ms": super::now_ms(),
    }))
    .unwrap();
    h.service.store().unwrap().put_pr(&pr).unwrap();
    assert_eq!(
        h.service.overview().unwrap().phases[&task.id].phase,
        Phase::Pr {
            number: 308,
            ci: None
        }
    );

    // The coordinator sends the work back: the stage answers the PR.
    let fix = h.dispatch(&task, "implementation", "events=codex-ok touch=fix.txt");
    let round = round_now(&h, &task).expect("a writing stage on the PR's news opens a round");
    assert_eq!(round.round.kind, "pr");
    assert_eq!(
        round.round.cause,
        "Cambios pedidos en el PR #308 por ana (2 comentarios)"
    );
    assert_eq!(
        round.round.items,
        ["ana pidió cambios: Renombra el campo", "ana: Falta un test"]
    );
    assert!(round.round.after_push);
    let fix = h.wait(&fix.id);
    assert!(fix.round.contains(&"- ana: Falta un test".to_string()));
    assert_eq!(
        h.service.task_pr(&task.id).unwrap().unwrap().news,
        Vec::<String>::new(),
        "seen once a stage answers it"
    );

    // Its review comes before the PR's wait, then its commit and its push.
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(
        phase.phase,
        Phase::NextStage {
            role: "review".into()
        }
    );
    put_stage(&h, &task, "r2", "review", "pass", &[], false);
    put_stage(&h, &task, "q2", "qa", "pass", &[], false);
    let phase = |h: &Harness| h.service.overview().unwrap().phases[&task.id].clone();
    assert!(matches!(
        phase(&h).phase,
        Phase::NextStep {
            rung: DeliveryRung::Commit,
            ..
        }
    ));
    step_done_now(&h, &task, "c1", "commit");
    assert_eq!(phase(&h).round.unwrap().at, "push");
    step_done_now(&h, &task, "p1", "push");
    assert!(phase(&h).round.is_none());
    assert_eq!(
        phase(&h).phase,
        Phase::Pr {
            number: 308,
            ci: None
        }
    );
    assert_eq!(
        h.service.overview().unwrap().rounds[0].outcome.as_deref(),
        Some("cambios publicados")
    );

    // The same changes requested do not open another round.
    let again = h.dispatch(&task, "tests", "events=codex-ok");
    h.wait(&again.id);
    assert_eq!(h.service.overview().unwrap().rounds.len(), 1);
}

#[test]
fn a_sync_with_conflicts_opens_a_round_once() {
    let h = harness();
    let task = h.task("R-6");
    let mut sync = executed_step(&task.id, "sync");
    sync.executed_at_ms = Some(super::now_ms());
    sync.conflicts = vec!["src/app.ts".into()];
    h.service.save_approval(&sync, "approval_approved").unwrap();
    h.service.save_approval(&sync, "step_matches").unwrap();
    let round = round_now(&h, &task).unwrap();
    assert_eq!(round.round.kind, "sync");
    assert_eq!(
        round.round.cause,
        "Conflictos al sincronizar con la base en 1 archivo"
    );
    assert_eq!(round.round.items, ["src/app.ts"]);
    assert_eq!(h.service.overview().unwrap().rounds.len(), 1);
}

#[test]
fn rounds_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.sqlite");
    let h = harness_with_store(DeliveryStore::open(&path).unwrap());
    let task = h.task("R-7");
    put_stage(&h, &task, "i1", "implementation", "pass", &[], true);
    put_stage(
        &h,
        &task,
        "r1",
        "review",
        "findings",
        &["Falta validar"],
        false,
    );
    let before = round_now(&h, &task).unwrap();
    close_tinto(&h.service);

    let service = reopen(&path, &h);
    let after = service.overview().unwrap().phases[&task.id].round.clone();
    assert_eq!(after, Some(before));
    service.close();
}

/// The structured reply of a wait_run as the run's owner.
fn wait(coordinator: &Coordinator, arguments: serde_json::Value) -> serde_json::Value {
    coordinator.write("wait_run", arguments)["structuredContent"].clone()
}

fn idle_kinds(reply: &serde_json::Value) -> Vec<(String, String)> {
    reply["idle"]
        .as_array()
        .expect("idle")
        .iter()
        .map(|task| {
            (
                task["key"].as_str().unwrap_or_default().to_string(),
                task["phase"]["kind"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

#[test]
fn a_blocker_the_user_clears_waits_on_the_coordinator_until_it_acts() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("AGOS-463");
    let blocked = coordinator.write(
        "set_task_blocked",
        json!({"task_id": task_id, "reason": "Waiting for the test databases",
               "blockers": [{"type": "note", "text": "las bases de pruebas"}]}),
    );
    assert_eq!(blocked["isError"], json!(null), "{blocked}");
    let cursor = wait(&coordinator, json!({}))["cursor"].as_u64().unwrap();

    // The user clears it while the coordinator is not inside wait_run.
    let cleared = h.service.clear_blockers(&task_id, None, true).unwrap();
    assert_eq!(
        cleared.blocked_reason, "",
        "the reason goes with the blockers"
    );
    let unblock = cleared
        .unblocked
        .clone()
        .expect("it waits on the coordinator");
    assert_eq!(unblock.by, "user");
    assert_eq!(unblock.cleared, "las bases de pruebas");
    assert_eq!(unblock.reason, "Waiting for the test databases");

    // With or without a cursor, the next waits list it until it is handled.
    let unblocked = vec![("AGOS-463".to_string(), "unblocked".to_string())];
    let with_cursor = wait(
        &coordinator,
        json!({"cursor": cursor, "timeout_seconds": 5}),
    );
    assert_eq!(idle_kinds(&with_cursor), unblocked, "{with_cursor}");
    assert_eq!(with_cursor["idle"][0]["phase"]["by"], "user");
    assert_eq!(
        with_cursor["idle"][0]["phase"]["cleared"],
        "las bases de pruebas"
    );
    assert_eq!(with_cursor["idle"][0]["phase"]["waits_on"], "coordinator");
    let next = with_cursor["cursor"].as_u64().unwrap();
    let quiet = wait(&coordinator, json!({"cursor": next, "timeout_seconds": 5}));
    assert_eq!(quiet["waited"], false, "it does not wait: {quiet}");
    assert_eq!(idle_kinds(&quiet), unblocked);
    assert_eq!(idle_kinds(&wait(&coordinator, json!({}))), unblocked);
    let phase = &h.service.overview().unwrap().phases[&task_id];
    assert!(matches!(phase.phase, super::phase::Phase::Unblocked { .. }));
    let held = coordinator.call(
        "acquire_run",
        json!({"run_id": coordinator.run_id, "owner": "coord"}),
    );
    let pending = held["structuredContent"]["pending"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(pending.len(), 1, "{pending:?}");
    assert_eq!(pending[0]["kind"], "unblocked");
    assert!(pending[0]["text"]
        .as_str()
        .unwrap()
        .contains("las bases de pruebas"));

    // Acting on the task settles it.
    coordinator.write(
        "set_task_state",
        json!({"task_id": task_id, "state": "implementation"}),
    );
    assert_eq!(h.service.task(&task_id).unwrap().unblocked, None);
    let after = wait(&coordinator, json!({}));
    assert!(
        !idle_kinds(&after)
            .iter()
            .any(|(_, kind)| kind == "unblocked"),
        "{after}"
    );
}

#[test]
fn a_blocker_that_resolves_waits_on_the_coordinator_until_it_dispatches() {
    use super::model::NewBlocker;

    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (rag, _) = coordinator.task("AGOS-580");
    let (task_id, _) = coordinator.task("AGOS-434");
    h.service
        .set_blockers(
            &task_id,
            vec![NewBlocker::Task {
                task: "AGOS-580".into(),
                until: None,
            }],
            "needs the RAG",
        )
        .unwrap();
    h.service
        .store()
        .unwrap()
        .put_approval(&executed_step(&rag, "merge"))
        .unwrap();
    assert_eq!(h.service.settle_blockers().unwrap(), 1);
    let task = h.service.task(&task_id).unwrap();
    assert_eq!(task.blocked_reason, "");
    assert_eq!(
        task.unblocked.as_ref().map(|unblock| unblock.by.as_str()),
        Some("tinto")
    );

    let dispatched = coordinator.write(
        "dispatch_job",
        json!({"task_id": task_id, "role": "tests", "agent": "codex", "prompt": "events=codex-ok"}),
    );
    assert_eq!(dispatched["isError"], json!(null), "{dispatched}");
    assert_eq!(h.service.task(&task_id).unwrap().unblocked, None);
}

#[test]
fn a_refused_dispatch_leaves_the_cleared_blocker_pending() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("AGOS-1");
    coordinator.write(
        "set_task_blocked",
        json!({"task_id": task_id, "blockers": [{"type": "note", "text": "x"}]}),
    );
    h.service.clear_blockers(&task_id, None, true).unwrap();
    let refused = coordinator.write(
        "dispatch_job",
        json!({"task_id": task_id, "role": "tests", "prompt": "x"}),
    );
    assert_eq!(refused["isError"], true, "no agent fixed for the stage");
    assert!(h.service.task(&task_id).unwrap().unblocked.is_some());
}

#[test]
fn clearing_every_blocker_empties_the_reason() {
    use super::model::NewBlocker;

    let h = harness();
    let task = h.task("AGOS-468");
    let note = |text: &str| NewBlocker::Note { text: text.into() };
    let blocked = h
        .service
        .set_blockers(
            &task.id,
            vec![note("las bases"), note("el diseño")],
            "Waiting for the test databases",
        )
        .unwrap();
    // One left open keeps it.
    let first = blocked.blockers[0].id.clone();
    let one = h
        .service
        .clear_blockers(&task.id, Some(&[first]), false)
        .unwrap();
    assert_eq!(one.blocked_reason, "Waiting for the test databases");
    // The coordinator clearing the last one leaves nothing pending.
    let none = h.service.clear_blockers(&task.id, None, false).unwrap();
    assert_eq!(none.blocked_reason, "");
    assert_eq!(none.unblocked, None);

    // A reason stored before blockers dropped theirs goes too.
    let mut stale = h.service.task(&task.id).unwrap();
    stale.blocked_reason = "Waiting for the test databases".into();
    h.service.store().unwrap().put_task(&stale).unwrap();
    h.service.settle_blockers().unwrap();
    assert_eq!(h.service.task(&task.id).unwrap().blocked_reason, "");
}

#[test]
fn changes_handed_to_an_interrupted_coordinator_come_again() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("AGOS-7");
    let start = wait(&coordinator, json!({}))["cursor"].as_u64().unwrap();
    let step: super::model::DeliveryApproval = serde_json::from_value(json!({
        "id": "a-jira", "task_id": task_id, "rung": "jira", "title": "Jira", "body": "",
        "status": "pending", "requested_by": "coord", "requested_at_ms": 1, "stage": "after_pr"
    }))
    .unwrap();
    h.service.store().unwrap().put_approval(&step).unwrap();
    h.service.decide_approval("a-jira", true, None).unwrap();

    // The wait hands the approval over, then the coordinator is interrupted.
    let handed = wait(
        &coordinator,
        json!({"cursor": start, "timeout_seconds": 5, "idle_ok": true}),
    );
    let events = handed["events"].as_array().unwrap().clone();
    assert_eq!(events.len(), 1, "{handed}");
    assert_eq!(events[0]["kind"], "approval_approved");
    assert_eq!(events[0]["redelivered"], json!(null));
    let cursor = handed["cursor"].as_u64().unwrap();
    let run = h.service.run(&coordinator.run_id).unwrap();
    assert_eq!(
        run.coordinator_cursor, start,
        "only handed, not acknowledged"
    );
    assert_eq!(run.returned_cursor, cursor);

    // A new wait without cursor, or taking the run, gets it again.
    let again = wait(&coordinator, json!({}));
    let events = again["events"].as_array().unwrap().clone();
    assert_eq!(events.len(), 1, "{again}");
    assert_eq!(events[0]["seq"], handed["events"][0]["seq"]);
    assert_eq!(events[0]["redelivered"], true);
    let taken = coordinator.call(
        "takeover_run",
        json!({"run_id": coordinator.run_id, "owner": "coord"}),
    );
    let pending = taken["structuredContent"]["pending"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(pending.len(), 1, "{taken}");
    assert_eq!(pending[0]["kind"], "approval_approved");

    // After the takeover, the wait without cursor hands the approval again
    // but not the takeover itself, which is news only for the replaced one.
    let generation = taken["structuredContent"]["generation"].as_u64().unwrap();
    let owned = |extra: serde_json::Value| {
        let mut arguments =
            json!({"run_id": coordinator.run_id, "owner": "coord", "generation": generation});
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        coordinator.call("wait_run", arguments)["structuredContent"].clone()
    };
    let resumed = owned(json!({}));
    let kinds: Vec<&str> = resumed["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["kind"].as_str())
        .collect();
    assert_eq!(kinds, vec!["approval_approved"], "{resumed}");

    // Waiting from its cursor acknowledges it: nothing comes again.
    let latest = resumed["cursor"].as_u64().unwrap();
    owned(json!({"cursor": latest, "timeout_seconds": 1, "idle_ok": true}));
    assert_eq!(
        h.service
            .run(&coordinator.run_id)
            .unwrap()
            .coordinator_cursor,
        latest
    );
    let fresh = owned(json!({}));
    assert_eq!(fresh["events"], json!([]), "{fresh}");
    let none = coordinator.call(
        "acquire_run",
        json!({"run_id": coordinator.run_id, "owner": "coord"}),
    );
    assert_eq!(none["structuredContent"]["pending"], json!([]));
}

#[test]
fn a_queued_job_says_what_holds_it() {
    use super::adapters::tests::sample_job;

    let h = harness();
    let task = h.task("AGOS-468");
    let other = h.task("AGOS-469");
    let now = super::now_ms();
    let mut checks = sample_job(DeliveryAgent::Shell);
    checks.id = "checks-1".into();
    checks.task_id = task.id.clone();
    checks.role = "checks".into();
    checks.prompt = "npm test".into();
    checks.status = DeliveryJobStatus::Started;
    checks.timeout_minutes = 30;
    checks.created_at_ms = now - 50 * 60_000;
    checks.started_at_ms = Some(now - 50 * 60_000);
    let mut queued = sample_job(DeliveryAgent::Codex);
    queued.id = "impl-1".into();
    queued.task_id = task.id.clone();
    queued.role = "implementation".into();
    queued.created_at_ms = now - 40 * 60_000;
    // QA of another task waits for the qa resource a finished job kept.
    let mut holder = sample_job(DeliveryAgent::Codex);
    holder.id = "qa-old".into();
    holder.task_id = other.id.clone();
    holder.role = "qa".into();
    holder.status = DeliveryJobStatus::Finished;
    holder.created_at_ms = now - 30 * 60_000;
    let mut qa = sample_job(DeliveryAgent::Codex);
    qa.id = "qa-2".into();
    qa.task_id = other.id.clone();
    qa.role = "qa".into();
    qa.lease = Some("qa".into());
    qa.created_at_ms = now - 20 * 60_000;
    {
        let store = h.service.store().unwrap();
        for job in [&checks, &queued, &holder, &qa] {
            store.put_job(job).unwrap();
        }
        let lease: super::model::DeliveryLease = serde_json::from_value(json!({
            "name": "qa", "state": "active", "generation": 1, "holder_job_id": "qa-old",
            "holder_task_id": other.id, "acquired_at_ms": 1, "note": null, "queue": [],
        }))
        .unwrap();
        store.put_lease(&lease).unwrap();
    }

    let overview = h.service.overview().unwrap();
    let wait = &overview.queued["impl-1"];
    assert_eq!(
        wait.reason,
        "espera a que terminen las comprobaciones (trabajo checks-1) de esta tarea"
    );
    assert_eq!(wait.since_ms, queued.created_at_ms);
    let stuck = wait
        .warning
        .as_deref()
        .expect("the checks run past their limit");
    assert!(
        stuck.contains("checks-1") && stuck.contains("30 min"),
        "{stuck}"
    );
    // The task runs its checks, and its phase warns that they look stuck.
    let phase = &overview.phases[&task.id];
    assert!(matches!(phase.phase, super::phase::Phase::Running { .. }));
    assert_eq!(phase.warning.as_deref(), Some(stuck));

    let qa_wait = &overview.queued["qa-2"];
    assert_eq!(
        qa_wait.reason,
        "espera el recurso qa, que tiene el trabajo qa-old"
    );
    assert!(qa_wait
        .warning
        .as_deref()
        .is_some_and(|warning| warning.contains("ya terminó")));
    match &overview.phases[&other.id].phase {
        super::phase::Phase::Queued { reason, .. } => {
            assert_eq!(reason.as_deref(), Some(qa_wait.reason.as_str()));
        }
        phase => panic!("{phase:?}"),
    }

    let read = mcp_tool(&h, "read_job", json!({"job_id": "impl-1"}));
    assert_eq!(read["queued_reason"], json!(wait.reason));
    assert_eq!(read["queued_since_ms"], json!(queued.created_at_ms));
    assert!(read["queued_warning"].is_string());
    let running = mcp_tool(&h, "read_job", json!({"job_id": "checks-1"}));
    assert_eq!(running.get("queued_reason"), None);
}

#[test]
fn tintos_checks_run_two_at_a_time_across_tasks() {
    use super::adapters::tests::sample_job;
    let h = harness();
    let (first, second, third) = (h.task("K-1"), h.task("K-2"), h.task("K-3"));
    let now = super::now_ms();
    let check = |id: &str, task: &DeliveryTask, status: DeliveryJobStatus| {
        let mut job = sample_job(DeliveryAgent::Shell);
        job.id = id.into();
        job.task_id = task.id.clone();
        job.role = "checks".into();
        job.prompt = "npm run typecheck".into();
        job.status = status;
        job.created_at_ms = now;
        job.started_at_ms = (status == DeliveryJobStatus::Started).then_some(now);
        job.pid = None;
        job
    };
    // An agent of the second task may run beside the first task's check.
    let mut agent = sample_job(DeliveryAgent::Codex);
    agent.id = "review-2".into();
    agent.task_id = second.id.clone();
    agent.role = "review".into();
    agent.created_at_ms = now - 1;
    {
        let store = h.service.store().unwrap();
        store
            .put_job(&check("check-1", &first, DeliveryJobStatus::Started))
            .unwrap();
        store.put_job(&agent).unwrap();
    }
    let overview = h.service.overview().unwrap();
    assert_eq!(overview.queued["review-2"].reason, "empieza ahora");
    h.service.cancel("review-2").unwrap();

    h.service
        .store()
        .unwrap()
        .put_job(&check("check-2", &second, DeliveryJobStatus::Queued))
        .unwrap();
    let overview = h.service.overview().unwrap();
    assert_eq!(overview.queued["check-2"].reason, "empieza ahora");
    h.service
        .store()
        .unwrap()
        .put_job(&check("check-2", &second, DeliveryJobStatus::Started))
        .unwrap();
    h.service
        .store()
        .unwrap()
        .put_job(&check("check-3", &third, DeliveryJobStatus::Queued))
        .unwrap();
    let overview = h.service.overview().unwrap();
    assert_eq!(
        overview.queued["check-3"].reason,
        "espera a que termine otra comprobación (trabajo check-1): corren 2 a la vez para no repartirse los núcleos"
    );
}

#[test]
fn a_job_cut_off_by_a_restart_goes_on_in_its_session_when_retried() {
    let h = harness();
    let task = h.task("C-9");
    let mut cut = h.wait(&h.dispatch(&task, "implementation", "events=codex-ok").id);
    assert!(cut.provider_session_id.is_some());
    cut.status = DeliveryJobStatus::Interrupted;
    cut.error = Some("Tinto se cerró mientras el trabajo corría.".into());
    h.service.store().unwrap().put_job(&cut).unwrap();
    h.service
        .retry_after_restart(&cut.id, "al reiniciar Tinto")
        .unwrap();
    let retry = h
        .service
        .overview()
        .unwrap()
        .jobs
        .into_iter()
        .find(|job| job.retry_of.as_deref() == Some(cut.id.as_str()))
        .expect("queued again");
    assert_eq!(retry.resumes.as_deref(), Some(cut.id.as_str()));
    assert_eq!(retry.provider_session_id, cut.provider_session_id);
    assert!(retry.resume_note.unwrap().contains("Tinto se cerró"));
    h.wait(&retry.id);

    // One that never started its session starts over.
    let mut silent = h.wait(&h.dispatch(&task, "review", "events=none").id);
    silent.status = DeliveryJobStatus::Interrupted;
    silent.provider_session_id = Some("never-started".into());
    h.service.store().unwrap().put_job(&silent).unwrap();
    h.service
        .retry_after_restart(&silent.id, "al reiniciar Tinto")
        .unwrap();
    let again = h
        .service
        .overview()
        .unwrap()
        .jobs
        .into_iter()
        .find(|job| job.retry_of.as_deref() == Some(silent.id.as_str()))
        .expect("queued again");
    assert_eq!(again.resumes, None);
    h.wait(&again.id);
}

#[test]
fn the_brainstorm_ends_once_every_task_of_the_run_is_being_implemented() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (first, _) = coordinator.task("B-1");
    let (second, _) = coordinator.task("B-2");
    let unattended = || h.service.run(&coordinator.run_id).unwrap().unattended;
    let implement = |task_id: &str| {
        let task = h.service.task(task_id).unwrap();
        let job = h.dispatch(&task, "implementation", "events=codex-ok");
        h.wait(&job.id);
    };
    implement(&first);
    assert!(!unattended(), "B-2 is not implemented yet");
    // A decision still open keeps it going.
    coordinator.write(
        "request_decisions",
        json!({"task_id": second, "decisions": [
            {"kind": "text", "question": "Button text?", "text": "Save"}
        ]}),
    );
    let pending = h.service.task_decisions(&second).unwrap();
    let task = h.service.task(&second).unwrap();
    assert!(h
        .service
        .dispatch(NewJob {
            task_id: task.id.clone(),
            role: "implementation".into(),
            agent: DeliveryAgent::Codex,
            model: None,
            access: DeliveryAccess::Workspace,
            prompt: "events=codex-ok".into(),
            writes: None,
            lease: None,
            timeout_minutes: None,
            reasoning: None,
            platform: None,
            read_dirs: Vec::new(),
            stage_default: false,
            needs_environment: false,
        })
        .is_err());
    assert!(!unattended());
    h.service
        .answer_decision(&pending[0].id, "Save", "user")
        .unwrap();
    // A task waiting for another one does not keep the brainstorm going.
    let (third, _) = coordinator.task("B-3");
    h.service
        .set_blockers(
            &third,
            vec![super::model::NewBlocker::Task {
                task: "B-2".into(),
                until: None,
            }],
            "builds on B-2",
        )
        .unwrap();
    // A step asked during the brainstorm waits for the user…
    let first_task = h.service.task(&first).unwrap();
    write_files(&first_task, &["done.txt"]);
    git(&first_task.worktree, &["add", "--all"]);
    git(&first_task.worktree, &["commit", "-q", "-m", "done"]);
    let push = h
        .service
        .request_approval(step(&first_task, DeliveryRung::Push, "Publish"))
        .unwrap();
    assert_eq!(push.status, DeliveryApprovalStatus::Pending);
    implement(&second);
    assert!(unattended());
    // …until it ends: then Tinto takes it, as the repo lets it.
    assert_ne!(
        h.service.approval(&push.id).unwrap().status,
        DeliveryApprovalStatus::Pending
    );
}

/// A workflow on main that runs CI only once a PR gets the `Ready` label,
/// as agentos does.
fn commit_ready_workflow(h: &Harness) {
    let dir = h.repo.path().join(".github/workflows");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("frontend.yml"),
        "on:\n  pull_request:\n    types: [ opened, labeled ]\njobs:\n  ci:\n    if: github.event_name == 'push' || (github.event.action == 'labeled' &&\n      github.event.label.name == 'Ready')\n",
    )
    .unwrap();
    git(h.repo.path(), &["add", "--all"]);
    git(h.repo.path(), &["commit", "-q", "-m", "ci"]);
}

fn merge_step(task: &DeliveryTask) -> NewApproval {
    NewApproval {
        merge_method: Some("squash".into()),
        requested_by: "user".into(),
        ..step(task, DeliveryRung::Merge, "Merge")
    }
}

/// What a job dispatched now would get: started, or why not.
fn try_dispatch(
    h: &Harness,
    task: &DeliveryTask,
    role: &str,
) -> Result<DeliveryJob, super::DeliveryError> {
    h.service.dispatch(NewJob {
        task_id: task.id.clone(),
        role: role.to_string(),
        agent: DeliveryAgent::Codex,
        model: None,
        access: DeliveryAccess::Workspace,
        prompt: "events=codex-ok".to_string(),
        writes: None,
        lease: None,
        timeout_minutes: None,
        reasoning: None,
        platform: None,
        read_dirs: Vec::new(),
        stage_default: false,
        needs_environment: false,
    })
}

/// Polls `check` until it gives something, for what Tinto does right
/// after a job ends.
fn eventually<T>(mut check: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some(found) = check() {
            return found;
        }
        assert!(Instant::now() < deadline, "it never happened");
        std::thread::sleep(POLL);
    }
}

#[test]
fn the_merge_needs_green_ci_on_the_head_and_a_base_that_has_not_moved() {
    let h = harness();
    commit_ready_workflow(&h);
    let task = h.task("CI-1");
    let github = FakeGitHub::new(
        json!({
            "state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "APPROVED",
            "headRefOid": "abc1234def", "reviews": [], "comments": [], "statusCheckRollup": []
        }),
        json!([]),
    );
    h.service.set_github(github.clone());
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/7").unwrap();
    h.service.link_pr(&task.id, &reference).unwrap();
    let refused = |rollup: serde_json::Value| {
        github.snapshot.lock().unwrap()["statusCheckRollup"] = rollup;
        h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
        h.service.request_approval(merge_step(&task)).err()
    };
    let check = |name: &str, status: &str, conclusion: &str| json!({"workflowName": "frontend", "name": name, "status": status, "conclusion": conclusion});
    let red = refused(json!([check("ci", "COMPLETED", "FAILURE")])).unwrap();
    assert_eq!(red.category, "pr_not_ready");
    assert_eq!(red.message, "el PR #7 tiene checks fallando en abc1234");
    let pending = refused(json!([check("ci", "IN_PROGRESS", "")])).unwrap();
    assert_eq!(pending.message, "el PR #7 tiene checks en curso en abc1234");
    let not_run = "la CI no se ejecutó en abc1234: añade la etiqueta Ready";
    assert_eq!(refused(json!([])).unwrap().message, not_run);
    // A workflow whose paths the PR touches but that skipped every job did
    // not run.
    assert_eq!(
        refused(json!([check("ci", "COMPLETED", "SKIPPED")]))
            .unwrap()
            .message,
        not_run
    );
    assert_eq!(
        h.service.overview().unwrap().phases[&task.id].phase,
        super::phase::Phase::Pr {
            number: 7,
            ci: Some(super::prs::CiState::NotRun)
        }
    );
    // A change the coordinator said needs no CI merges without it; checks
    // that did run still count.
    h.service.set_task_needs_ci(&task.id, false).unwrap();
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    assert_eq!(
        h.service.overview().unwrap().phases[&task.id].phase,
        super::phase::Phase::Pr {
            number: 7,
            ci: None
        }
    );
    assert!(refused(json!([])).is_none());
    let asked = h
        .service
        .store()
        .unwrap()
        .approvals()
        .unwrap()
        .pop()
        .unwrap();
    h.service.withdraw_approval(&asked.id, "again").unwrap();
    let red = refused(json!([check("ci", "COMPLETED", "FAILURE")])).unwrap();
    assert_eq!(red.message, "el PR #7 tiene checks fallando en abc1234");
    h.service.set_task_needs_ci(&task.id, true).unwrap();
    // Green, beside a job of the same workflow that runs only on main and
    // a workflow that skipped by its own condition.
    let green = json!([
        check("ci", "COMPLETED", "SUCCESS"),
        check("deploy", "COMPLETED", "SKIPPED"),
        {"workflowName": "PR Agent", "name": "PR Agent review", "status": "COMPLETED", "conclusion": "SKIPPED"}
    ]);
    assert!(refused(green.clone()).is_none());
    let asked = h
        .service
        .store()
        .unwrap()
        .approvals()
        .unwrap()
        .pop()
        .unwrap();
    h.service.withdraw_approval(&asked.id, "again").unwrap();

    // The base moved after the green run: sync and a new green run first.
    std::fs::write(h.repo.path().join("later.txt"), "later\n").unwrap();
    git(h.repo.path(), &["add", "--all"]);
    git(h.repo.path(), &["commit", "-q", "-m", "main moves on"]);
    let moved = refused(green).unwrap();
    assert_eq!(moved.category, "base_moved", "{moved}");
}

#[test]
fn the_pr_and_push_steps_put_the_ci_label_on_and_a_push_starts_ci_again() {
    let h = harness();
    commit_ready_workflow(&h);
    github_origin(&h);
    let origin = tempfile::tempdir().unwrap();
    git(origin.path(), &["init", "-q", "--bare", "-b", "main"]);
    let origin_path = plain_path(origin.path());
    git(
        h.repo.path(),
        &["remote", "set-url", "--push", "origin", &origin_path],
    );
    let task = h.task("CI-2");
    let github = FakeGitHub::new(
        json!({"state": "OPEN", "mergeable": "MERGEABLE", "statusCheckRollup": [], "reviews": [], "comments": [], "labels": []}),
        json!([]),
    );
    h.service.set_github(github.clone());
    let url = "https://github.com/o/r/pull/8";
    let reference = super::prs::parse_pr_url(url).unwrap();
    h.service.link_pr(&task.id, &reference).unwrap();
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();

    // A task that needs no CI gets no label.
    let docs_only = h.service.set_task_needs_ci(&task.id, false).unwrap();
    assert_eq!(h.service.label_for_ci(&docs_only, url, false), None);
    assert!(github.actions.lock().unwrap().is_empty());
    let task = h.service.set_task_needs_ci(&task.id, true).unwrap();

    // Opening the PR: the label goes on.
    assert_eq!(h.service.label_for_ci(&task, url, false), None);
    assert_eq!(
        github.actions.lock().unwrap().as_slice(),
        [format!("label {url} Ready")]
    );
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    github.actions.lock().unwrap().clear();

    // A push: off and on again, which is what makes CI run on the new head.
    write_files(&task, &["more.txt"]);
    git(&task.worktree, &["add", "--all"]);
    git(&task.worktree, &["commit", "-q", "-m", "more"]);
    let push = h
        .service
        .request_approval(step(&task, DeliveryRung::Push, "Publish"))
        .unwrap();
    let pushed = h.service.decide_approval(&push.id, true, None).unwrap();
    assert_eq!(
        pushed.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        pushed.outcome
    );
    assert_eq!(pushed.notice, None);
    assert_eq!(
        github.actions.lock().unwrap().as_slice(),
        [format!("unlabel {url} Ready"), format!("label {url} Ready")]
    );
}

#[test]
fn a_pr_merged_on_github_opens_a_ci_round_only_while_a_check_fails() {
    let h = harness();
    let task = h.task("CI-3");
    // Passed, beside a workflow that skipped by its own condition.
    let passed = json!([
        {"workflowName": "api", "name": "ci", "status": "COMPLETED", "conclusion": "SUCCESS"},
        {"workflowName": "PR Agent", "name": "PR Agent review", "status": "COMPLETED", "conclusion": "SKIPPED"}
    ]);
    let github = FakeGitHub::new(
        json!({
            "state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "APPROVED",
            "headRefOid": "fedcba98", "baseRefName": "main", "reviews": [], "comments": [],
            "statusCheckRollup": passed.clone()
        }),
        json!([]),
    );
    h.service.set_github(github.clone());
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/9").unwrap();
    h.service.link_pr(&task.id, &reference).unwrap();
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    github.snapshot.lock().unwrap()["state"] = json!("MERGED");
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    let rounds = || h.service.store().unwrap().task_rounds(&task.id).unwrap();

    // Merged with CI ok: the merge is done and nothing needs fixing.
    let merge = h
        .service
        .latest_approval(&task.id, DeliveryRung::Merge)
        .unwrap()
        .unwrap();
    assert_eq!(merge.status, DeliveryApprovalStatus::Executed);
    let pr = h.service.task_pr(&task.id).unwrap().unwrap();
    assert_eq!(pr.merged_failing_ci, None);
    assert!(rounds().is_empty());
    let clean = h.service.overview().unwrap().phases[&task.id].phase.clone();

    // GitHub shows a check on that head failing.
    github.snapshot.lock().unwrap()["statusCheckRollup"] = json!([{"workflowName": "api", "name": "ci", "status": "COMPLETED", "conclusion": "FAILURE"}]);
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();

    let why = "el PR #9 tiene checks fallando en fedcba9";
    let pr = h.service.task_pr(&task.id).unwrap().unwrap();
    assert_eq!(pr.merged_failing_ci.as_deref(), Some(why));
    let events = h.service.store().unwrap().events_after(0).unwrap();
    assert!(events
        .iter()
        .any(|event| event.kind == "pr_merged_failing_ci"));
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(
        phase.phase,
        super::phase::Phase::MergedFailingCi {
            number: 9,
            why: why.into()
        }
    );
    let round = phase.round.expect("a round to fix it");
    assert_eq!(round.round.kind, "ci");
    assert!(
        round.round.cause.contains("arréglalo en main"),
        "{}",
        round.round.cause
    );
    // Read again: still one round.
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    assert_eq!(rounds().len(), 1);

    // Recorded as failing, but GitHub no longer shows a failure (its re-run
    // passed, or it was a skip once counted against it): taken back.
    github.snapshot.lock().unwrap()["statusCheckRollup"] = passed;
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    let pr = h.service.task_pr(&task.id).unwrap().unwrap();
    assert_eq!(pr.merged_failing_ci, None);
    let round = rounds().pop().unwrap();
    assert!(!round.is_open());
    assert_eq!(round.outcome.as_deref(), Some("CI passed; no fix needed"));
    // Where a clean merge leaves it.
    assert_eq!(h.service.overview().unwrap().phases[&task.id].phase, clean);
}

/// A task whose sync stopped on conflicts in `shared.txt`, with `exit=0`
/// as its check.
fn conflicted_merge(h: &Harness, key: &str) -> DeliveryTask {
    set_repo_checks(h, &["exit=0"]);
    let task = h.task(key);
    h.service
        .set_task_checks(&task.id, vec!["exit=0".into()])
        .unwrap();
    diverge_from_main(h, &task, true);
    // A sync runs as it is asked: nobody approves it.
    let done = h
        .service
        .request_approval(step(&task, DeliveryRung::Sync, "Merge main"))
        .unwrap();
    assert_eq!(done.conflicts, ["shared.txt"]);
    h.service.task(&task.id).unwrap()
}

/// The commit Tinto asked last for the task.
fn merge_commit_asked(h: &Harness, task: &DeliveryTask) -> super::model::DeliveryApproval {
    eventually(|| {
        h.service
            .store()
            .unwrap()
            .approvals()
            .unwrap()
            .into_iter()
            .rev()
            .find(|approval| {
                approval.task_id == task.id
                    && approval.rung == DeliveryRung::Commit
                    && approval.status == DeliveryApprovalStatus::Pending
            })
    })
}

/// The merge's own stages: the implementation that resolves the conflict,
/// its checks and its review.
fn resolve_merge(h: &Harness, task: &DeliveryTask) -> DeliveryJob {
    let fix = h.dispatch(task, "implementation", "events=codex-ok touch=resolved.txt");
    std::fs::write(task.worktree.join("shared.txt"), "task and main\n").unwrap();
    h.wait(&fix.id);
    ended_checks(h, task, 1);
    let review = h.dispatch(task, "review", "events=codex-ok");
    h.wait(&review.id);
    fix
}

#[test]
fn during_a_merge_only_its_stages_run_and_tinto_asks_its_commit_once_they_pass() {
    let h = harness();
    let task = conflicted_merge(&h, "MC-1");
    for role in ["qa", "tests"] {
        let refused = try_dispatch(&h, &task, role).unwrap_err();
        assert_eq!(refused.category, "merge_in_progress", "{role}: {refused}");
    }
    resolve_merge(&h, &task);
    let commit = merge_commit_asked(&h, &task);
    assert_eq!(commit.requested_by, "auto");
    assert_eq!(commit.title, "Merge main");
    // Its stages passed: nothing runs before the commit, not even them.
    for role in ["implementation", "review", "qa"] {
        assert_eq!(
            try_dispatch(&h, &task, role).unwrap_err().category,
            "merge_in_progress",
            "{role}"
        );
    }
    let concluded = h.service.decide_approval(&commit.id, true, None).unwrap();
    assert_eq!(
        concluded.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        concluded.outcome
    );
    assert!(super::tasks::merge_head(super::tasks::Place::Native, &task.worktree).is_none());
    let qa = try_dispatch(&h, &task, "qa").unwrap();
    h.wait(&qa.id);
}

#[test]
fn a_merge_committed_and_pushed_by_hand_closes_its_round_and_asks_no_commit() {
    let h = harness();
    let task = conflicted_merge(&h, "MC-0");
    resolve_merge(&h, &task);
    merge_commit_asked(&h, &task);
    // The user concludes the merge and publishes it outside Tinto.
    let origin = tempfile::tempdir().unwrap();
    git(origin.path(), &["init", "-q", "--bare", "-b", "main"]);
    git(
        h.repo.path(),
        &["remote", "add", "origin", &plain_path(origin.path())],
    );
    git(&task.worktree, &["add", "--all"]);
    git(&task.worktree, &["commit", "-q", "--no-edit"]);
    git(&task.worktree, &["push", "-q", "-u", "origin", "HEAD"]);

    h.service.overview().unwrap();
    let store = h.service.store().unwrap();
    let rounds = store.task_rounds(&task.id).unwrap();
    assert!(rounds.iter().all(|round| !round.is_open()), "{rounds:?}");
    assert_eq!(
        rounds.last().unwrap().outcome.as_deref(),
        Some("merge confirmado fuera de Tinto")
    );
    assert!(!store.approvals().unwrap().iter().any(|approval| {
        approval.task_id == task.id
            && approval.rung == DeliveryRung::Commit
            && approval.status == DeliveryApprovalStatus::Pending
    }));
}

#[test]
fn a_round_opened_during_a_merge_waits_for_its_commit_and_runs_on_top_of_it() {
    let h = harness();
    let task = conflicted_merge(&h, "MC-2");
    super::rounds::open_round(
        &h.service.store().unwrap(),
        &task.id,
        super::rounds::NewRound {
            kind: "qa_problems",
            short: "1 problema de QA".into(),
            cause: "1 problema de QA para corregir".into(),
            items: vec!["#1 P1: the toast covers Retry → move it up".into()],
            source: None,
        },
    )
    .unwrap();
    let rounds = || h.service.store().unwrap().task_rounds(&task.id).unwrap();
    let waiting = rounds()
        .into_iter()
        .find(|round| round.kind == "qa_problems")
        .unwrap();
    assert!(waiting.waits_merge_commit && waiting.is_open());
    // The merge's own round stays the one its stages work in.
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(phase.round.unwrap().round.kind, "sync");
    let fix = resolve_merge(&h, &task);
    assert!(!fix.round.join("\n").contains("toast"), "{:?}", fix.round);

    // While the commit is not there, the task says what it waits for.
    let commit = merge_commit_asked(&h, &task);
    h.service.withdraw_approval(&commit.id, "later").unwrap();
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(
        phase.phase,
        super::phase::Phase::WaitsMergeCommit {
            waiting_round: Some(waiting.number)
        }
    );
    assert_eq!(phase.next_stage, None);

    h.service.conclude_merge(&task.id).unwrap();
    let commit = merge_commit_asked(&h, &task);
    let concluded = h.service.decide_approval(&commit.id, true, None).unwrap();
    assert_eq!(
        concluded.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        concluded.outcome
    );
    let released = rounds()
        .into_iter()
        .find(|round| round.id == waiting.id)
        .unwrap();
    assert!(!released.waits_merge_commit && released.is_open());
    assert!(released.opened_at_ms >= concluded.executed_at_ms.unwrap());
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(phase.round.unwrap().round.kind, "qa_problems");
    // Its fix is told about it, on top of the merge commit.
    let fix = h.dispatch(&task, "implementation", "events=codex-ok touch=toast.txt");
    assert!(fix.round.join("\n").contains("toast"), "{:?}", fix.round);
    h.wait(&fix.id);
}

#[test]
fn a_merge_other_work_got_into_asks_the_user_how_to_commit_it() {
    let h = harness();
    let task = conflicted_merge(&h, "MC-3");
    // As before Tinto kept them apart: a PR round worked during the merge.
    let round = super::model::DeliveryRound {
        id: "legacy".into(),
        task_id: task.id.clone(),
        number: 3,
        kind: "pr".into(),
        short: "PR".into(),
        cause: "Cambios pedidos en el PR #5".into(),
        items: Vec::new(),
        source: None,
        opened_at_ms: super::now_ms(),
        after_push: true,
        closed_at_ms: None,
        outcome: None,
        waits_merge_commit: false,
    };
    h.service.store().unwrap().put_round(&round).unwrap();
    let fix = h.dispatch(&task, "implementation", "events=codex-ok touch=review.txt");
    std::fs::write(task.worktree.join("shared.txt"), "task and main\n").unwrap();
    h.wait(&fix.id);
    let asked = eventually(|| {
        h.service
            .store()
            .unwrap()
            .task_decisions(&task.id)
            .unwrap()
            .into_iter()
            .find(|decision| decision.question == super::coordination::MIXED_QUESTION)
    });
    assert!(asked.detail.contains("review.txt"), "{}", asked.detail);
    let labels: Vec<&str> = asked
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            super::coordination::MIXED_ONE_COMMIT,
            super::coordination::MIXED_SPLIT
        ]
    );
    h.service
        .answer_decision(&asked.id, super::coordination::MIXED_ONE_COMMIT, "user")
        .unwrap();
    let commit = merge_commit_asked(&h, &task);
    assert!(
        commit
            .body
            .contains("Ronda 3 (Cambios pedidos en el PR #5): review.txt")
            || commit.commits[0]
                .message
                .contains("Ronda 3 (Cambios pedidos en el PR #5): review.txt"),
        "{commit:?}"
    );
}

#[test]
fn a_reply_shows_the_qa_screenshots_the_user_chose_from_a_branch_deleted_with_the_pr() {
    let h = harness();
    github_origin(&h);
    let coordinator = Coordinator::start(&h);
    let (task_id, _) = coordinator.task("R-9");
    let task = h.service.task(&task_id).unwrap();
    let github = FakeGitHub::new(
        json!({"state": "OPEN", "mergeable": "MERGEABLE", "statusCheckRollup": [], "reviews": [], "comments": []}),
        json!([]),
    );
    *github.review_comments.lock().unwrap() = json!([
        {"id": 3100, "path": "README.md", "line": 1, "position": 1,
         "html_url": "https://github.com/o/r/pull/299#discussion_r3100",
         "user": {"login": "Sn4pe"}, "created_at": "2026-10-19T10:00:00Z",
         "body": "The menu wraps on mobile.", "diff_hunk": "@@ -1,1 +1,1 @@
    +base"}
    ]);
    h.service.set_github(github.clone());
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/299").unwrap();
    h.service.link_pr(&task_id, &reference).unwrap();
    h.service.refresh_pr(&task_id, Duration::ZERO).unwrap();

    // What a QA of the task saved.
    let qa = h.dispatch(&task, "qa", "events=codex-ok");
    h.wait(&qa.id);
    let folder = (h.service.inner.paths_for)(&qa.id).unwrap().qa_output;
    std::fs::create_dir_all(&folder).unwrap();
    let shot = "R-9-1-menu-mobile.png";
    std::fs::write(folder.join(shot), b"png").unwrap();
    std::fs::write(folder.join("R-9.md"), "# R-9").unwrap();
    let shots = h.service.qa_screenshots(&task_id).unwrap();
    let files: Vec<&str> = shots.iter().map(|shot| shot.file.as_str()).collect();
    assert_eq!(files, [shot]);
    assert!(shots[0].data_uri.starts_with("data:image/png;base64,"));

    let asked = coordinator.write(
        "request_approval",
        json!({"task_id": task_id, "rung": "pr_reply",
               "comment_url": "https://github.com/o/r/pull/299#discussion_r3100",
               "body": "The menu keeps each option on one line now."}),
    );
    let id = asked["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let image = |file: &str| super::model::DeliveryReplyImage {
        job_id: qa.id.clone(),
        file: file.to_string(),
    };
    // Only the task's QA screenshots.
    assert!(h
        .service
        .set_reply_images(&id, vec![image("other.png")])
        .is_err());
    h.service.set_reply_images(&id, vec![image(shot)]).unwrap();
    let posted = h.service.decide_approval(&id, true, None).unwrap();
    assert_eq!(
        posted.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        posted.outcome
    );
    let short = qa.id.split('-').next().unwrap();
    let path = format!("R-9/{short}/{shot}");
    let branch = task.branch.replace('/', "-");
    let actions = github.actions.lock().unwrap().clone();
    assert!(
        actions.contains(&format!("upload o/r tinto/qa-evidence/{branch} {path}")),
        "{actions:?}"
    );
    let reply = actions
        .iter()
        .find(|action| action.starts_with("reply 3100"))
        .unwrap();
    assert!(
        reply.ends_with(&format!(
            "The menu keeps each option on one line now.

![{shot}](https://github.com/o/r/blob/c0ffee1/{path}?raw=true)"
        )),
        "{reply}"
    );

    // Merged: the branch with the screenshots goes.
    github.snapshot.lock().unwrap()["state"] = json!("MERGED");
    h.service.refresh_pr(&task_id, Duration::ZERO).unwrap();
    assert!(github
        .actions
        .lock()
        .unwrap()
        .contains(&format!("delete-branch o/r tinto/qa-evidence/{branch}")));
}

#[test]
fn the_watch_reads_a_red_pr_on_every_pass_and_any_open_pr_every_two_minutes() {
    let h = harness();
    let task = h.task("W-1");
    let rollup = |conclusion: &str| json!([{"workflowName": "api", "name": "ci", "status": "COMPLETED", "conclusion": conclusion}]);
    let github = FakeGitHub::new(
        json!({"state": "OPEN", "mergeable": "MERGEABLE", "reviews": [], "comments": [],
               "statusCheckRollup": rollup("FAILURE")}),
        json!([]),
    );
    h.service.set_github(github.clone());
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/12").unwrap();
    h.service.link_pr(&task.id, &reference).unwrap();
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    let pr = || h.service.task_pr(&task.id).unwrap().unwrap();
    assert_eq!(pr().checks.failing, 1);

    // CI ran again and passed: no notification tells, the next pass sees it.
    github.snapshot.lock().unwrap()["statusCheckRollup"] = rollup("SUCCESS");
    let mut etag = None;
    h.service.watch_prs_once(&mut etag);
    assert_eq!(pr().checks.failing, 0);
    assert_eq!(pr().checks.passing, 1);

    // Green and just read: left alone.
    github.snapshot.lock().unwrap()["comments"] = json!([{"author": {"login": "ana"}, "createdAt": "2099-01-01T00:00:00Z", "body": "one more thing"}]);
    h.service.watch_prs_once(&mut etag);
    assert_eq!(pr().new_comments, 0);
    // Read two minutes ago: read again, and the comment is there.
    let mut old = pr();
    old.checked_at_ms = Some(super::now_ms() - 3 * 60_000);
    h.service.store().unwrap().put_pr(&old).unwrap();
    h.service.watch_prs_once(&mut etag);
    assert_eq!(pr().new_comments, 1);
}

#[test]
fn the_pr_step_shows_each_qa_screenshot_where_its_description_names_it() {
    let h = harness();
    github_origin(&h);
    let task = h.task("O-9");
    let github = FakeGitHub::new(json!({}), json!([]));
    h.service.set_github(github.clone());
    let qa = h.dispatch(&task, "qa", "events=codex-ok");
    h.wait(&qa.id);
    let folder = (h.service.inner.paths_for)(&qa.id).unwrap().qa_output;
    std::fs::create_dir_all(&folder).unwrap();
    let (named, dropped, chosen) = (
        "O-9-1-menu-desktop.png",
        "O-9-2-toast-desktop.png",
        "O-9-3-menu-mobile.png",
    );
    for shot in [named, dropped, chosen] {
        std::fs::write(folder.join(shot), b"png").unwrap();
    }
    let body = format!(
        "## How to test\n\n1. Open the menu.\n\n![The wider menu](qa:{named})\n\n2. Save.\n\n![The toast](qa:{dropped})\n\n## Notes\n\nNone."
    );
    // The coordinator names them; one no QA of the task has is refused.
    let images = h.service.named_screenshots(&task.id, &body).unwrap();
    assert_eq!(
        images
            .iter()
            .map(|image| image.file.as_str())
            .collect::<Vec<_>>(),
        [named, dropped]
    );
    let unknown = h
        .service
        .named_screenshots(&task.id, "![x](qa:O-9-9-nope.png)")
        .unwrap_err();
    assert!(unknown.message.contains(named), "{}", unknown.message);
    let step: super::model::DeliveryApproval = serde_json::from_value(json!({
        "id": "a-pr", "task_id": task.id, "rung": "pr", "title": "feat: menu",
        "body": body, "status": "pending", "images": images,
        "requested_by": "coord", "requested_at_ms": 1
    }))
    .unwrap();
    h.service.store().unwrap().put_approval(&step).unwrap();
    // The user takes one out and adds another.
    let image = |file: &str| super::model::DeliveryReplyImage {
        job_id: qa.id.clone(),
        file: file.into(),
    };
    h.service
        .set_reply_images("a-pr", vec![image(named), image(chosen)])
        .unwrap();

    let done = h.service.decide_approval("a-pr", true, None).unwrap();
    assert_eq!(
        done.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        done.notice
    );
    let short = qa.id.split('-').next().unwrap();
    let url =
        |shot: &str| format!("https://github.com/o/r/blob/c0ffee1/O-9/{short}/{shot}?raw=true");
    let branch = task.branch.replace('/', "-");
    // The screenshots go up before the PR opens with them.
    assert_eq!(
        github.actions.lock().unwrap()[0],
        format!("upload o/r tinto/qa-evidence/{branch} O-9/{short}/{named}")
    );
    // The named one under its step, the taken-out one gone, the added one
    // at the end of "How to test".
    let shown = format!(
        "## How to test\n\n1. Open the menu.\n\n![The wider menu]({})\n\n2. Save.\n\n![{chosen}]({})\n\n## Notes\n\nNone.",
        url(named),
        url(chosen)
    );
    assert_eq!(github.snapshot.lock().unwrap()["body"], shown);
    // The step holds the text GitHub has.
    assert_eq!(done.body, shown);
}

#[test]
fn a_quarantine_holds_other_tasks_qa_while_the_stage_that_left_it_goes_on() {
    use super::adapters::tests::sample_job;
    use super::phase::{Phase, Waiter};
    let h = harness();
    let coordinator = Coordinator::start(&h);
    let (left_id, _) = coordinator.task("Q-10");
    let (other_id, _) = coordinator.task("Q-11");
    let now = super::now_ms();
    let qa = |id: &str, task_id: &str, status: DeliveryJobStatus, at: u64| {
        let mut job = sample_job(DeliveryAgent::Codex);
        job.id = id.into();
        job.task_id = task_id.into();
        job.role = "qa".into();
        job.lease = Some("qa".into());
        job.status = status;
        job.created_at_ms = at;
        job.started_at_ms = (status != DeliveryJobStatus::Queued).then_some(at);
        job.ended_at_ms = (status == DeliveryJobStatus::Failed).then_some(at + 1);
        job.pid = None;
        job
    };
    {
        let store = h.service.store().unwrap();
        store
            .put_job(&qa("qa-1", &left_id, DeliveryJobStatus::Failed, now - 10))
            .unwrap();
        store
            .put_job(&qa(
                "qa-other",
                &other_id,
                DeliveryJobStatus::Queued,
                now - 5,
            ))
            .unwrap();
        store
            .put_lease(&super::model::DeliveryLease {
                name: "qa".into(),
                state: DeliveryLeaseState::Quarantined,
                generation: 1,
                holder_job_id: Some("qa-1".into()),
                holder_task_id: Some(left_id.clone()),
                acquired_at_ms: Some(now - 10),
                note: Some(
                    "Al terminar seguían en marcha, y Tinto los paró: node.exe (pid 9).".into(),
                ),
                // As when the scheduler held the job back for something else.
                queue: Vec::new(),
                last_task_id: Some(left_id.clone()),
                prepared: None,
            })
            .unwrap();
    }

    // The other task waits on it, and the coordinator, with nothing of the
    // batch waiting on the user, is the one to release it.
    let overview = h.service.overview().unwrap();
    let phase = &overview.phases[&other_id];
    assert_eq!(
        phase.phase,
        Phase::WaitsQuarantine {
            job_id: "qa-other".into(),
            lease: "qa".into()
        }
    );
    assert_eq!(phase.waits_on, Waiter::Coordinator);
    let woken = coordinator.write("wait_run", json!({}));
    let open = &woken["structuredContent"]["open_jobs"];
    let waiting = open
        .as_array()
        .unwrap()
        .iter()
        .find(|job| job["job_id"] == "qa-other")
        .unwrap_or_else(|| panic!("{woken}"));
    assert_eq!(waiting["waits_quarantine"], "qa");
    assert_eq!(waiting["waits_on"], "coordinator");
    assert!(waiting["quarantine_note"]
        .as_str()
        .unwrap()
        .contains("node.exe"));

    // The stage that left it, continued, takes it over at once; the other
    // task still waits.
    h.service
        .store()
        .unwrap()
        .put_job(&qa("qa-2", &left_id, DeliveryJobStatus::Queued, now - 1))
        .unwrap();
    let overview = h.service.overview().unwrap();
    assert_eq!(overview.queued["qa-2"].reason, "empieza ahora");
    assert!(
        overview.queued["qa-other"].reason.contains("cuarentena")
            || overview.queued["qa-other"].reason.contains("qa-2"),
        "{}",
        overview.queued["qa-other"].reason
    );
}

#[test]
fn a_qa_job_that_leaves_a_server_running_ends_with_it_stopped_and_named_in_the_quarantine() {
    let h = harness();
    let task = h.task("Q-12");
    let job = h.dispatch(&task, "qa", "events=none server=1 exit=3");
    assert_eq!(h.wait(&job.id).status, DeliveryJobStatus::Failed);
    let lease = eventually(|| {
        Some(qa_lease(&h)).filter(|lease| lease.state == DeliveryLeaseState::Quarantined)
    });
    let server = if cfg!(windows) { "ping" } else { "sleep" };
    let note = lease.note.unwrap_or_default();
    assert!(note.to_lowercase().contains(server), "{note}");
    assert!(note.contains("Tinto los paró"), "{note}");
    let events = h.service.store().unwrap().events_after(0).unwrap();
    assert!(events
        .iter()
        .any(|event| event.kind == "job_leftovers"
            && event.job_id.as_deref() == Some(job.id.as_str())));
    // Nothing of the job runs any more.
    #[cfg(target_os = "windows")]
    {
        let left = crate::windows_process::KillOnCloseJob::open_named(&format!(
            "Local\tinto-delivery-{}",
            job.id
        ));
        let deadline = Instant::now() + Duration::from_secs(10);
        while left
            .as_ref()
            .is_some_and(|tree| !tree.processes().is_empty())
        {
            assert!(Instant::now() < deadline, "the server kept running");
            std::thread::sleep(POLL);
        }
    }
}

#[test]
fn a_pr_merges_only_once_every_chosen_reviewer_approved_or_was_removed() {
    let h = harness();
    let task = h.task("RV-1");
    let github = FakeGitHub::new(
        json!({
            "state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "APPROVED",
            "statusCheckRollup": [{"name": "ci", "status": "COMPLETED", "conclusion": "SUCCESS"}],
            "reviews": [{"author": {"login": "ana"}, "state": "APPROVED", "submittedAt": "2026-10-27T08:00:00Z"}],
            "comments": []
        }),
        json!([]),
    );
    h.service.set_github(github.clone());
    let reference = super::prs::parse_pr_url("https://github.com/o/r/pull/14").unwrap();
    h.service.link_pr(&task.id, &reference).unwrap();
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    h.service
        .change_pr_reviewers(&task.id, vec!["ana".into(), "luis".into()], Vec::new())
        .unwrap();

    // GitHub says approved, but luis, chosen to review it, has not.
    let refused = h.service.request_approval(merge_step(&task)).unwrap_err();
    assert_eq!(refused.category, "pr_not_ready");
    assert!(
        refused.message.contains("le falta la aprobación de luis"),
        "{}",
        refused.message
    );
    let phase = h.service.overview().unwrap().phases[&task.id].clone();
    assert_eq!(phase.waits_on, super::phase::Waiter::Reviewers);

    // The user takes him off: what was asked is approved.
    h.service
        .change_pr_reviewers(&task.id, Vec::new(), vec!["luis".into()])
        .unwrap();
    assert!(h.service.request_approval(merge_step(&task)).is_ok());
}

#[cfg(target_os = "windows")]
#[test]
fn a_check_the_system_killed_runs_again_once_before_it_sends_the_work_back() {
    // 0x40010004: what Windows gives a process it terminated.
    let killed = "exit=1073807364";
    let h = harness();
    set_repo_checks(&h, &[killed]);
    let task = h.task("K-70");
    h.service
        .set_task_checks(&task.id, vec![killed.into()])
        .unwrap();
    let fix = h.dispatch(&task, "implementation", "events=codex-ok touch=code.txt");
    h.wait(&fix.id);
    // Killed once: it runs again rather than sending the work back.
    let runs = ended_checks(&h, &task, 2);
    assert!(runs
        .iter()
        .all(|run| run.status == DeliveryJobStatus::Failed));
    // Killed again: now it does.
    let round = eventually(|| round_now(&h, &task));
    assert_eq!(round.round.kind, "checks");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(ended_checks(&h, &task, 2).len(), 2, "only one retry");
}

#[test]
fn a_free_qa_slot_goes_first_to_the_task_its_stack_was_built_for() {
    use super::adapters::tests::sample_job;
    let h = harness();
    let (stack, other) = (h.task("ST-1"), h.task("ST-2"));
    let now = super::now_ms();
    let qa = |id: &str, task: &DeliveryTask, at: u64| {
        let mut job = sample_job(DeliveryAgent::Codex);
        job.id = id.into();
        job.task_id = task.id.clone();
        job.role = "qa".into();
        job.lease = Some("qa".into());
        job.status = DeliveryJobStatus::Queued;
        job.created_at_ms = at;
        job.started_at_ms = None;
        job
    };
    {
        let store = h.service.store().unwrap();
        // The other task asked first; the stack is still on ST-1's branch.
        store.put_job(&qa("qa-other", &other, now - 5)).unwrap();
        store.put_job(&qa("qa-stack", &stack, now - 1)).unwrap();
        store
            .put_lease(&super::model::DeliveryLease {
                name: "qa".into(),
                state: DeliveryLeaseState::Free,
                generation: 2,
                holder_job_id: None,
                holder_task_id: None,
                acquired_at_ms: None,
                note: None,
                queue: Vec::new(),
                last_task_id: Some(stack.id.clone()),
                prepared: None,
            })
            .unwrap();
    }
    let queued = h.service.overview().unwrap().queued;
    assert_eq!(queued["qa-stack"].reason, "empieza ahora");
    assert_ne!(queued["qa-other"].reason, "empieza ahora");
    // Without a job of ST-1 waiting, nobody waits for it: the other starts.
    h.service.cancel("qa-stack").unwrap();
    eventually(|| {
        (h.service.job("qa-other").unwrap().status != DeliveryJobStatus::Queued).then_some(())
    });
}

/// A local app for QA stack tests: `/` loads `/m.js`, which imports
/// `/dep.js`. It records the paths asked for.
fn qa_app() -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = asked.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            let _ = BufReader::new(&stream).read_line(&mut line);
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            let body = match path.as_str() {
                "/" => r#"<html><script type="module" src="/m.js"></script></html>"#,
                "/m.js" => r#"import "/dep.js";"#,
                _ => "export {};",
            };
            seen.lock().unwrap().push(path);
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (url, asked)
}

#[test]
fn browser_qa_starts_on_a_prepared_stack_beside_a_command_line_qa() {
    let h = harness();
    let (url, asked) = qa_app();
    let built = h.repo.path().join("built.txt");
    let store = h.service.store().unwrap();
    let mut settings = store.repo_settings(h.repo.path()).unwrap();
    settings.qa_browser = true;
    settings.qa_stack = Some(super::model::DeliveryQaStack {
        name: "dev".into(),
        up: format!("echo {{services}}>> \"{}\"", plain_path(&built)),
        services: [
            ("frontend".to_string(), vec!["web".to_string()]),
            ("api".to_string(), vec!["api".to_string()]),
        ]
        .into(),
        ready_url: url,
        warm_paths: vec!["/".into()],
        ..Default::default()
    });
    store.set_repo_settings(h.repo.path(), &settings).unwrap();
    drop(store);
    let task = h.task("S-1");
    let other = h.task("S-2");
    let worktree = PathBuf::from(plain_path(&task.worktree));
    std::fs::create_dir_all(worktree.join("frontend")).unwrap();
    std::fs::write(worktree.join("frontend").join("page.vue"), "x").unwrap();

    let browser = h.dispatch(&task, "qa", "events=codex-ok sleep=3");
    let cli = h
        .service
        .dispatch(NewJob {
            task_id: other.id.clone(),
            role: "qa".into(),
            agent: DeliveryAgent::Codex,
            model: None,
            access: DeliveryAccess::Workspace,
            prompt: "events=codex-ok sleep=3".into(),
            writes: None,
            lease: Some("qa:cli:s2".into()),
            timeout_minutes: None,
            reasoning: None,
            platform: None,
            read_dirs: Vec::new(),
            stage_default: false,
            needs_environment: false,
        })
        .unwrap();
    assert_eq!(browser.lease.as_deref(), Some("qa:browser:dev"));
    assert!(!cli.qa_browser && cli.qa_stack.is_none());
    // Each holds its own environment's resource: both run at once.
    h.wait_status(&browser.id, DeliveryJobStatus::Started);
    assert_eq!(
        h.service.job(&cli.id).unwrap().status,
        DeliveryJobStatus::Started
    );

    let browser = h.wait(&browser.id);
    let prepare = browser.prepare.clone().unwrap();
    // Only the service of the changed folder was rebuilt, and the warm-up
    // followed the page's imports.
    assert_eq!(prepare.services, ["web"]);
    assert!(!prepare.reused);
    assert_eq!(std::fs::read_to_string(&built).unwrap().trim(), "web");
    assert!(asked.lock().unwrap().contains(&"/dep.js".to_string()));
    assert!(browser.timing.unwrap().prepare_ms.is_some());

    // The same worktree again: nothing is rebuilt.
    let again = h.wait(&h.dispatch(&task, "qa", "events=codex-ok").id);
    assert!(again.prepare.unwrap().reused);
    assert_eq!(std::fs::read_to_string(&built).unwrap().trim(), "web");
}

#[test]
fn a_qa_that_ends_without_its_record_leaves_a_stub_and_its_changes_undone() {
    let h = harness();
    let task = h.task("S-3");
    let qa = h.wait(&h.dispatch(&task, "qa", "events=none touch=README.md").id);
    let worktree = PathBuf::from(plain_path(&task.worktree));
    assert_eq!(
        std::fs::read_to_string(worktree.join("README.md"))
            .unwrap()
            .replace('\r', ""),
        "base\n"
    );
    assert_eq!(qa.reverted, ["README.md"]);
    assert!(qa.changes.is_empty());
    let source = qa.visual_record_source.unwrap();
    assert!(source.ends_with("S-3.md"), "{source}");
    assert!(
        qa.visual_record_warnings.is_empty(),
        "{:?}",
        qa.visual_record_warnings
    );
    assert!(std::fs::read_to_string(source)
        .unwrap()
        .contains("not verifiable: out of time"));
}

#[test]
fn guides_turn_their_reviewers_comments_into_rules_the_user_approves() {
    use super::model::{DeliveryGuideKind, DeliveryGuidePerson};
    let h = harness();
    github_origin(&h);
    let github = FakeGitHub::new(json!({}), json!([]));
    let comment = |id: u64, login: &str, body: &str| {
        json!({
            "id": id, "user": {"login": login}, "body": body,
            "path": "frontend/menu.vue", "line": 4, "diff_hunk": "@@ -1 +1 @@\n-a\n+b",
            "html_url": format!("https://github.com/o/r/pull/7#discussion_r{id}"),
            "_links": {"pull_request": {"href": "https://api.github.com/repos/o/r/pulls/7"}},
            "created_at": "2026-10-01T10:00:00Z"
        })
    };
    *github.review_comments.lock().unwrap() = json!([
        comment(1, "ana", "Usa el token de 4px, no 6px"),
        comment(2, "bob", "lgtm"),
        comment(3, "Luis", "El error pasa por el handler común"),
    ]);
    h.service.set_github(github.clone());
    let repo = h.repo.path();
    assert_eq!(
        h.service
            .collect_guide_comments(repo, None)
            .unwrap_err()
            .category,
        "invalid_guide",
        "nothing to collect before a guide is chosen"
    );
    let person = |login: &str, kind| DeliveryGuidePerson {
        login: login.into(),
        kinds: vec![kind],
    };
    let store = h.service.store().unwrap();
    let mut settings = store.repo_settings(repo).unwrap();
    settings.guides = vec![
        person("ana", DeliveryGuideKind::Visual),
        person("luis", DeliveryGuideKind::Technical),
    ];
    store.set_repo_settings(repo, &settings).unwrap();
    drop(store);
    // Only the guides' comments, each once however often it is read.
    h.service.collect_guide_comments(repo, None).unwrap();
    let view = h.service.collect_guide_comments(repo, None).unwrap();
    assert_eq!((view.comments, view.pending), (2, 2));

    // Distilling takes a batch's coordinator, who hears of it.
    assert_eq!(
        h.service.request_guide(repo).unwrap_err().category,
        "invalid_guide"
    );
    let coordinator = Coordinator::start(&h);
    h.service.request_guide(repo).unwrap();
    let woken = coordinator.write("wait_run", json!({}));
    assert!(
        woken["structuredContent"]["events"]
            .to_string()
            .contains("read_guide_comments"),
        "{woken}"
    );
    let read = coordinator.call("read_guide_comments", json!({"run_id": coordinator.run_id}))
        ["structuredContent"]
        .clone();
    assert_eq!(read["comments"].as_array().unwrap().len(), 2, "{read}");
    // Tinto's own numbers, in the order collected.
    assert_eq!(read["through"], 2);
    let stale = coordinator.write("propose_guide", json!({"through": 9, "rules": []}));
    assert_eq!(stale["isError"], true, "{stale}");
    // A stage no job has would leave the rule out of every job.
    let unknown = coordinator.write(
        "propose_guide",
        json!({"through": 2, "rules": [{"kind": "visual", "rule": "x", "stages": ["QA"]}]}),
    );
    assert_eq!(unknown["isError"], true, "{unknown}");
    coordinator.write(
        "propose_guide",
        json!({"through": 2, "note": "Primeras reglas", "rules": [
            {"kind": "visual", "rule": "El espaciado sale de los tokens de 4px", "applies_to": ["frontend/"], "seen": 1},
            {"kind": "technical", "rule": "Los errores pasan por el handler común", "stages": ["review"]}
        ]}),
    );
    let view = h.service.guide_view(repo).unwrap();
    let proposed: Vec<String> = view
        .proposal
        .unwrap()
        .rules
        .into_iter()
        .map(|rule| rule.id)
        .collect();
    assert_eq!(proposed, ["V-1", "T-1"]);
    assert!(
        view.rules.is_empty(),
        "nothing changes until the user approves"
    );

    let view = h.service.decide_guide(repo, true).unwrap();
    assert_eq!(view.rules.len(), 2);
    assert_eq!((view.pending, view.proposal), (0, None));
    let read = coordinator.call("read_guide_comments", json!({"run_id": coordinator.run_id}));
    assert_eq!(
        read["structuredContent"]["comments"],
        json!([]),
        "distilled comments are not read again"
    );

    // A job gets the approved rules for its stage.
    let task = h.task("AGOS-1");
    let job = h.dispatch(&task, "implementation", "events=codex-ok");
    assert_eq!(
        job.guide_rules,
        ["V-1: El espaciado sale de los tokens de 4px (in frontend/)"]
    );
    h.wait(&job.id);

    // While Tinto follows PRs it collects on its own what the guides write in
    // a review's summary or a PR's conversation (not an issue's, nor an empty
    // approval), and asks the coordinator to distill the new comments.
    let written = |id: u64, login: &str, body: &str, url: &str, at: &str| json!({"id": id, "user": {"login": login}, "body": body, "html_url": url, "created_at": at});
    *github.conversation.lock().unwrap() = json!([
        written(
            40,
            "ana",
            "Los vacíos dicen qué hacer",
            "https://github.com/o/r/pull/7#issuecomment-40",
            "2026-10-03T10:00:00Z"
        ),
        written(
            41,
            "ana",
            "Una idea para el roadmap",
            "https://github.com/o/r/issues/8#issuecomment-41",
            "2026-10-03T11:00:00Z"
        ),
    ]);
    *github.review_bodies.lock().unwrap() = json!([
        written(
            9,
            "luis",
            "Falta manejar el timeout",
            "https://github.com/o/r/pull/7#pullrequestreview-9",
            "2026-10-02T10:00:00Z"
        ),
        written(
            10,
            "luis",
            "",
            "https://github.com/o/r/pull/7#pullrequestreview-10",
            "2026-10-04T10:00:00Z"
        ),
    ]);
    h.service.watch_guides().unwrap();
    assert_eq!(
        h.service.guide_view(repo).unwrap().pending,
        0,
        "collected at most every half hour"
    );
    {
        let store = h.service.store().unwrap();
        let mut guide = store.guide(repo).unwrap();
        guide.collected_at_ms = Some(0);
        store.put_guide(repo, &guide).unwrap();
    }
    h.service.watch_guides().unwrap();
    let woken = coordinator.write("wait_run", json!({}));
    assert!(
        woken["structuredContent"]["events"]
            .to_string()
            .contains("read_guide_comments"),
        "{woken}"
    );
    let read = coordinator.call("read_guide_comments", json!({"run_id": coordinator.run_id}))
        ["structuredContent"]
        .clone();
    let texts: Vec<&str> = read["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|comment| comment["text"].as_str().unwrap())
        .collect();
    assert_eq!(
        texts,
        ["Falta manejar el timeout", "Los vacíos dicen qué hacer"]
    );
    assert_eq!(read["through"], 4);
}

/// A bare `origin` with `main` pushed, for tests that publish.
fn bare_origin(h: &Harness) -> tempfile::TempDir {
    let origin = tempfile::tempdir().unwrap();
    git(origin.path(), &["init", "-q", "--bare", "-b", "main"]);
    git(
        h.repo.path(),
        &["remote", "add", "origin", &plain_path(origin.path())],
    );
    git(h.repo.path(), &["push", "-q", "origin", "main"]);
    origin
}

/// A task created from another task's branch.
fn task_on(h: &Harness, key: &str, base: &DeliveryTask) -> DeliveryTask {
    h.service
        .create_task(NewTask {
            repo: h.repo.path().to_path_buf(),
            distro: None,
            key: key.into(),
            title: key.into(),
            read_dirs: Vec::new(),
            base: Some(base.branch.clone()),
            branch: None,
            run_id: None,
            package: None,
        })
        .unwrap()
}

/// Commits everything changed in the worktree outside Tinto.
fn commit_outside(task: &DeliveryTask, message: &str) {
    git(&task.worktree, &["add", "--all"]);
    git(&task.worktree, &["commit", "-q", "-m", message]);
}

/// The task's commit and push steps, approved by the user.
fn commit_and_push(
    h: &Harness,
    task: &DeliveryTask,
    title: &str,
) -> super::model::DeliveryApproval {
    let commit = h
        .service
        .request_approval(step(task, DeliveryRung::Commit, title))
        .unwrap();
    h.service.decide_approval(&commit.id, true, None).unwrap();
    let push = h
        .service
        .request_approval(step(task, DeliveryRung::Push, "Publish"))
        .unwrap();
    let push = h.service.decide_approval(&push.id, true, None).unwrap();
    assert_eq!(
        push.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        push.outcome
    );
    push
}

fn latest_push(h: &Harness, task: &DeliveryTask) -> super::model::DeliveryApproval {
    h.service
        .store()
        .unwrap()
        .approvals()
        .unwrap()
        .into_iter()
        .rev()
        .find(|step| step.task_id == task.id && step.rung == DeliveryRung::Push)
        .unwrap()
}

#[test]
fn a_branch_rewritten_outside_tinto_is_adopted_when_the_worktree_has_it() {
    let h = harness();
    let _origin = bare_origin(&h);
    let first = h.task("RW-1");
    write_files(&first, &["one.txt"]);
    commit_outside(&first, "one");
    git(&first.worktree, &["push", "-q", "origin", &first.branch]);
    let second = task_on(&h, "RW-2", &first);
    write_files(&second, &["two.txt"]);
    commit_and_push(&h, &second, "Add two");

    // Re-authored outside Tinto, same content: adopted with a notice, no
    // push to approve.
    git(
        &second.worktree,
        &[
            "-c",
            "user.email=o@o",
            "commit",
            "-q",
            "--amend",
            "--no-edit",
            "--reset-author",
        ],
    );
    git(
        &second.worktree,
        &["push", "-q", "-f", "origin", &second.branch],
    );
    h.service.check_pushes(true).unwrap();
    let push = latest_push(&h, &second);
    assert_eq!(
        push.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        push.notice
    );
    assert!(
        push.outcome.as_deref().unwrap().contains("adopta"),
        "{:?}",
        push.outcome
    );
    assert_eq!(push.tip, Some(in_worktree(&second, &["rev-parse", "HEAD"])));

    // It syncs the first branch, which then merges into main rewritten
    // (with main moved on): restacked onto main outside Tinto and pushed.
    write_files(&first, &["one2.txt"]);
    commit_outside(&first, "one more");
    git(&first.worktree, &["push", "-q", "origin", &first.branch]);
    let sync = h
        .service
        .request_approval(step(&second, DeliveryRung::Sync, "Merge first"))
        .unwrap();
    assert_eq!(
        sync.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        sync.outcome
    );
    let push = h
        .service
        .request_approval(step(&second, DeliveryRung::Push, "Publish"))
        .unwrap();
    h.service.decide_approval(&push.id, true, None).unwrap();
    let repo = h.repo.path();
    for name in ["one.txt", "one2.txt", "other.txt"] {
        std::fs::write(repo.join(name), format!("{name}\n")).unwrap();
    }
    git(repo, &["add", "--all"]);
    git(repo, &["commit", "-q", "-m", "first, squashed (#1)"]);
    git(repo, &["push", "-q", "origin", "main"]);
    let mut moved = h.service.task(&second.id).unwrap();
    moved.base_ref = "main".into();
    h.service.store().unwrap().put_task(&moved).unwrap();
    git(&second.worktree, &["fetch", "-q", "origin"]);
    let old_base = format!("origin/{}", first.branch);
    git(
        &second.worktree,
        &["rebase", "-q", "--onto", "origin/main", &old_base],
    );
    git(
        &second.worktree,
        &["push", "-q", "-f", "origin", &second.branch],
    );

    // Other content than published: it stays drifted until a push adopts
    // it, which the synced commit no longer in the branch does not refuse:
    // the PR's current base is in it.
    h.service.check_pushes(true).unwrap();
    let drifted = latest_push(&h, &second);
    assert_eq!(drifted.status, DeliveryApprovalStatus::Drifted);
    assert!(drifted
        .notice
        .as_deref()
        .unwrap()
        .contains("otro contenido"));
    let again = h
        .service
        .request_approval(step(&second, DeliveryRung::Push, "Adopt"))
        .unwrap();
    let again = h.service.decide_approval(&again.id, true, None).unwrap();
    assert_eq!(
        again.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        again.outcome
    );
    assert!(
        again.outcome.as_deref().unwrap().contains("adopta"),
        "{:?}",
        again.outcome
    );
}

#[test]
fn a_task_on_another_tasks_branch_follows_it_and_is_restacked_when_it_merges_squashed() {
    let h = harness();
    let origin = bare_origin(&h);
    let first = h.task("RS-1");
    write_files(&first, &["one.txt"]);
    commit_outside(&first, "one");
    let second = task_on(&h, "RS-2", &first);

    // Not a package, yet it starts each stage from the first's latest work
    // until it has its own.
    write_files(&first, &["one2.txt"]);
    commit_outside(&first, "one more");
    let job = h.dispatch(&second, "tests", "events=codex-ok");
    assert_eq!(h.wait(&job.id).status, DeliveryJobStatus::Finished);
    assert!(second.worktree.join("one2.txt").exists());
    git(&first.worktree, &["push", "-q", "origin", &first.branch]);
    write_files(&second, &["two.txt"]);
    let published = commit_and_push(&h, &second, "Add two").tip.unwrap();

    // The first merges into main squashed: the second's base moves there
    // and it is marked for a restack.
    let repo = h.repo.path();
    for name in ["one.txt", "one2.txt"] {
        std::fs::write(repo.join(name), format!("{name}\n")).unwrap();
    }
    git(repo, &["add", "--all"]);
    git(repo, &["commit", "-q", "-m", "first (#1)"]);
    git(repo, &["push", "-q", "origin", "main"]);
    let mut merge = executed_step(&first.id, "merge");
    merge.merge_method = Some("squash".into());
    h.service.store().unwrap().put_approval(&merge).unwrap();
    assert_eq!(h.service.retarget_packages().unwrap(), 1);
    let moved = h.service.task(&second.id).unwrap();
    assert_eq!(moved.base_ref, "main");
    let first_tip = in_worktree(&first, &["rev-parse", "HEAD"]);
    assert_eq!(moved.restack_from.as_deref(), Some(first_tip.as_str()));

    // The restack waits for the user, then keeps only its own commit.
    let restack = h
        .service
        .request_approval(step(&second, DeliveryRung::Sync, "Restack"))
        .unwrap();
    assert_eq!(restack.status, DeliveryApprovalStatus::Pending);
    let restack = h.service.decide_approval(&restack.id, true, None).unwrap();
    assert_eq!(
        restack.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        restack.outcome
    );
    assert_eq!(
        in_worktree(&second, &["rev-parse", "HEAD^"]),
        in_worktree(&second, &["rev-parse", "origin/main"])
    );
    assert_eq!(h.service.task(&second.id).unwrap().restack_from, None);

    // Its push replaces the remote branch while it is what Tinto published.
    let push = h
        .service
        .request_approval(step(&second, DeliveryRung::Push, "Publish"))
        .unwrap();
    let push = h.service.decide_approval(&push.id, true, None).unwrap();
    assert_eq!(
        push.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        push.outcome
    );
    assert!(push
        .outcome
        .as_deref()
        .unwrap()
        .contains("force-with-lease"));
    let remote = super::tasks::git(
        super::tasks::Place::Native,
        origin.path(),
        &["rev-parse", &format!("refs/heads/{}", second.branch)],
    )
    .unwrap();
    assert_eq!(remote, in_worktree(&second, &["rev-parse", "HEAD"]));
    assert_ne!(remote, published);
}

#[test]
fn merging_a_pr_others_go_into_keeps_its_branch_until_they_point_at_the_merge_target() {
    let h = harness();
    github_origin(&h);
    let github = FakeGitHub::new(
        json!({
            "state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "APPROVED",
            "statusCheckRollup": [], "reviews": [], "comments": [],
            "baseRefName": "delivery/k-63-1"
        }),
        json!([]),
    );
    h.service.set_github(github.clone());
    let first = package(&h, "K-63", 1, None).unwrap();
    let second = package(&h, "K-63", 2, Some(1)).unwrap();
    for (task, number) in [(&first, 50), (&second, 51)] {
        let url = format!("https://github.com/o/r/pull/{number}");
        let reference = super::prs::parse_pr_url(&url).unwrap();
        h.service.link_pr(&task.id, &reference).unwrap();
        h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    }
    let mut merge = step(&first, DeliveryRung::Merge, "K-63: API (#50)");
    merge.merge_method = Some("squash".into());
    let merge = h.service.request_approval(merge).unwrap();
    let merged = h.service.decide_approval(&merge.id, true, None).unwrap();
    assert_eq!(
        merged.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        merged.outcome
    );

    assert_eq!(
        github.merged.lock().unwrap().as_slice(),
        ["https://github.com/o/r/pull/50 squash K-63: API (#50) (branch kept)"]
    );
    let actions = github.actions.lock().unwrap().clone();
    let at = |action: &str| actions.iter().position(|done| done == action);
    let retargeted = at("base https://github.com/o/r/pull/51 main").expect("retargeted");
    let deleted = at("delete-branch o/r delivery/k-63-1").expect("deleted");
    assert!(retargeted < deleted, "{actions:?}");
    // Squashed: the second still carries the first's commits as they were.
    assert!(h.service.task(&second.id).unwrap().restack_from.is_some());
}

#[test]
fn a_push_publishes_the_head_it_was_asked_for_or_only_tintos_steps_after_it() {
    let h = harness();
    let _origin = bare_origin(&h);
    let task = h.task("HM-1");
    let commit = |name: &str| {
        write_files(&task, &[name]);
        let commit = h
            .service
            .request_approval(step(&task, DeliveryRung::Commit, name))
            .unwrap();
        h.service.decide_approval(&commit.id, true, None).unwrap();
    };
    commit("a.txt");
    let push = h
        .service
        .request_approval(step(&task, DeliveryRung::Push, "Publish"))
        .unwrap();
    assert_eq!(push.head, Some(in_worktree(&task, &["rev-parse", "HEAD"])));

    // A commit step Tinto ran before the approval goes with it.
    commit("b.txt");
    let pushed = h.service.decide_approval(&push.id, true, None).unwrap();
    assert_eq!(
        pushed.status,
        DeliveryApprovalStatus::Executed,
        "{:?}",
        pushed.outcome
    );
    assert!(pushed.outcome.as_deref().unwrap().contains("Tinto añadió"));
    assert_eq!(pushed.tip, Some(in_worktree(&task, &["rev-parse", "HEAD"])));

    // A commit made outside Tinto meanwhile is not published.
    commit("c.txt");
    let push = h
        .service
        .request_approval(step(&task, DeliveryRung::Push, "Publish"))
        .unwrap();
    write_files(&task, &["d.txt"]);
    commit_outside(&task, "outside");
    let refused = h.service.decide_approval(&push.id, true, None).unwrap();
    assert_eq!(refused.status, DeliveryApprovalStatus::Failed);
    assert!(
        refused.outcome.as_deref().unwrap().contains("head_moved"),
        "{:?}",
        refused.outcome
    );
}

#[test]
fn a_commit_shows_its_author_and_warns_when_the_base_history_has_another() {
    let h = harness();
    let task = h.task("AU-1");
    write_files(&task, &["a.txt"]);
    let asked = h
        .service
        .request_approval(step(&task, DeliveryRung::Commit, "Add a"))
        .unwrap();
    assert_eq!(asked.author.as_deref(), Some("t <t@t>"));
    assert_eq!(asked.notice, None);
    h.service
        .withdraw_approval(&asked.id, "other author")
        .unwrap();

    git(
        &task.worktree,
        &["config", "user.email", "me@elsewhere.org"],
    );
    let asked = h
        .service
        .request_approval(step(&task, DeliveryRung::Commit, "Add a"))
        .unwrap();
    assert_eq!(asked.author.as_deref(), Some("t <me@elsewhere.org>"));
    assert!(
        asked.notice.as_deref().unwrap().contains("t@t"),
        "{:?}",
        asked.notice
    );

    // Approved, it commits as the author shown or not at all.
    git(&task.worktree, &["config", "user.email", "t@t"]);
    let done = h.service.decide_approval(&asked.id, true, None).unwrap();
    assert_eq!(done.status, DeliveryApprovalStatus::Failed);
    assert!(
        done.outcome.as_deref().unwrap().contains("autor"),
        "{:?}",
        done.outcome
    );
    assert!(!changed(&task).is_empty());
}
