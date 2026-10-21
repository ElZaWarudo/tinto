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
use super::service::{DeliveryService, Launcher, NewJob, NewTask};
use super::store::DeliveryStore;
use super::tasks::plain_path;
use super::tasks::test_support::{git, repo_with_commit};
use crate::agent_console::checkpoint::CheckpointConfig;

const CODEX_OK: &str = include_str!("fixtures/codex-ok.jsonl");
const CODEX_FAILED: &str = include_str!("fixtures/codex-failed.jsonl");
const CLAUDE_OK: &str = include_str!("fixtures/claude-ok.jsonl");

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
        // `record=problems`: one that lists two problems.
        if let Some(record) = spec.get("record") {
            let problems = match *record {
                "problems" => "**Problems:**\n- P1: The toast covers Retry on mobile.\n- P2: The menu is cut off.\n\n**Suggested solutions:**\n- P1: Move the toast up.\n- P2: Let the menu scroll.\n",
                _ => "**Problems:** None\n",
            };
            std::fs::create_dir_all(&paths.qa_output).unwrap();
            std::fs::write(
                paths.qa_output.join(format!("{}.md", task.key)),
                format!(
                    "# {} — Visual changes\n\n## 1. Menú\n\n{problems}",
                    task.key
                ),
            )
            .unwrap();
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
                base: None,
                branch: None,
                run_id: None,
            })
            .unwrap()
    }

    fn dispatch(&self, task: &DeliveryTask, role: &str, prompt: &str) -> DeliveryJob {
        self.service
            .dispatch(NewJob {
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
            })
            .unwrap()
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
            std::thread::sleep(Duration::from_millis(100));
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
            std::thread::sleep(Duration::from_millis(100));
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
        .set_codex_model(Some(" gpt-6-astra ".into()))
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
fn a_failed_agent_reports_its_reason() {
    let h = harness();
    let task = h.task("K-3");
    let job = h.dispatch(&task, "implementation", "events=codex-failed exit=1");
    let job = h.wait(&job.id);
    assert_eq!(job.status, DeliveryJobStatus::Failed);
    assert_eq!(
        job.error.as_deref(),
        Some("The 'gpt-6.1-sol' model is not supported when using Codex with a ChatGPT account.")
    );
    assert_eq!(job.exit_code, Some(1));
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
                base: None,
                branch: None,
                run_id: Some(run.id.clone()),
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
    // Without a record, nothing is rendered.
    let plain = h.dispatch(&task, "qa", "events=codex-ok");
    assert_eq!(h.wait(&plain.id).visual_record, None);
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
        std::thread::sleep(Duration::from_millis(100));
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
            base: None,
            branch: None,
            run_id: Some(run.id.clone()),
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
        base: None,
        branch: None,
        run_id: Some(run.id.clone()),
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
        .update_task(&task.id, None, Some(2), None)
        .unwrap();
    let review = h.wait(&review.id);
    assert_eq!(review.result_state, Some(DeliveryResultState::Stale));
    assert!(h
        .service
        .update_task(&task.id, None, Some(1), None)
        .is_err());
}

#[test]
fn a_restart_marks_running_jobs_interrupted_and_quarantines_their_resource() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.sqlite");
    let h = harness_with_store(DeliveryStore::open(&path).unwrap());
    let task = h.task("K-10");
    let job = h.dispatch(&task, "qa", "sleep=30");
    h.wait_status(&job.id, DeliveryJobStatus::Started);

    let restarted = DeliveryService::with_parts(
        DeliveryStore::open(&path).unwrap(),
        fake_launcher(),
        Arc::new(|id: &str| Ok(JobPaths::in_dir(std::env::temp_dir().join(id)))),
        CheckpointConfig::default(),
    )
    .unwrap();
    let interrupted = restarted.job(&job.id).unwrap();
    assert_eq!(interrupted.status, DeliveryJobStatus::Interrupted);
    let lease = restarted.overview().unwrap().leases.remove(0);
    assert_eq!(lease.state, DeliveryLeaseState::Quarantined);

    let _ = h.service.cancel(&job.id);
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
                base: None,
                branch: None,
                run_id: Some(run.id.clone()),
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
        skipped.error.as_deref().unwrap().contains("No se reintenta")
            && skipped.error.as_deref().unwrap().contains("cuarentena"),
        "{:?}",
        skipped.error
    );
    assert!(!jobs_now
        .iter()
        .any(|job| job.task_id == testing.id && job.id != qa.id));

    let waited = restarted
        .wait_run(&run.id, Some(cursor), Duration::from_secs(1), true)
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
        texts.iter().any(|text| text.starts_with("R-2: la etapa qa se interrumpió")
            && text.contains("no se reintenta")),
        "{texts:?}"
    );

    // The retry itself is cut off by another restart: not repeated again.
    let deadline = Instant::now() + Duration::from_secs(60);
    while restarted.job(&retry.id).unwrap().status != DeliveryJobStatus::Started {
        assert!(Instant::now() < deadline, "the retry did not start");
        std::thread::sleep(Duration::from_millis(100));
    }
    let again = restart();
    again.retry_interrupted();
    let cut = again.job(&retry.id).unwrap();
    assert_eq!(cut.status, DeliveryJobStatus::Interrupted);
    assert!(cut.error.as_deref().unwrap().contains("ya era el reintento"));
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
fn without_retry_on_restart_an_interrupted_job_stays_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.sqlite");
    let h = harness_with_store(DeliveryStore::open(&path).unwrap());
    let task = h.task("R-3");
    let job = h.dispatch(&task, "implementation", "sleep=30");
    h.wait_status(&job.id, DeliveryJobStatus::Started);

    let restarted = DeliveryService::with_parts(
        DeliveryStore::open(&path).unwrap(),
        fake_launcher(),
        Arc::new(|id: &str| Ok(JobPaths::in_dir(std::env::temp_dir().join(id)))),
        CheckpointConfig::default(),
    )
    .unwrap();
    restarted.retry_interrupted();
    let jobs = restarted.overview().unwrap().jobs;
    assert_eq!(jobs.len(), 1);
    assert_eq!(
        jobs[0].error.as_deref(),
        Some("Tinto se cerró mientras el trabajo corría.")
    );
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
            base: None,
            branch: Some("other".to_string()),
            run_id: None,
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
    // A retry follows the stage as the user left it after a model failed;
    // a model the coordinator chose is kept.
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
    assert_eq!(retried["model"], "gpt-5.5", "{retried}");
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
    settings.bootstrap = Some(if cfg!(target_os = "windows") {
        "echo ready>bootstrapped.txt".to_string()
    } else {
        "echo ready > bootstrapped.txt".to_string()
    });
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
        "mongodb://localhost:27017/tinto_test".into(),
    );
    h.service
        .store()
        .unwrap()
        .set_repo_settings(h.repo.path(), &settings)
        .unwrap();
    let task = h.task("E-1");
    let job = h.dispatch(&task, "tests", "events=codex-ok env=TEST_DB_MONGO_URL");
    assert_eq!(job.check_env, settings.check_env);
    h.wait(&job.id);
    let seen = std::fs::read_to_string(task.worktree.join("env.txt")).unwrap();
    assert_eq!(seen.trim(), "mongodb://localhost:27017/tinto_test");
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
            base: None,
            branch: None,
            run_id: Some(run.id.clone()),
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
    // during the brainstorm only the user may free it.
    let crashing = h.dispatch(&task, "qa", "events=none exit=3");
    h.wait(&crashing.id);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(qa_lease(&h).state, DeliveryLeaseState::Quarantined);
    assert_eq!(release("limpio")["isError"], true);

    h.service.end_brainstorm(&run.id).unwrap();
    assert_eq!(release(" ")["isError"], true, "it says what it checked");
    let released = release("sin procesos ni datos de prueba");
    assert_eq!(released["structuredContent"]["state"], "free", "{released}");
    assert!(released["structuredContent"]["note"]
        .as_str()
        .unwrap()
        .starts_with("Liberado por el coordinador coord"));

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
    std::fs::write(task.worktree.join("cli.js"), "v2\n").unwrap();
    let qa = h
        .service
        .dispatch(claude_qa(&task, "events=claude-ok"))
        .unwrap();
    let qa = h.wait(&qa.id);
    assert_eq!(qa.status, DeliveryJobStatus::Finished, "{:?}", qa.error);
    let renewed = h.service.task_decisions(&task.id).unwrap().remove(0);
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
            base: None,
            branch: None,
            run_id: Some(run_id.into()),
        })
        .unwrap()
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
        .update_task(&task.id, Some(label.into()), None, None)
        .unwrap_err();
    assert!(
        error.message.contains("de 1 a 40 caracteres"),
        "{}",
        error.message
    );
    assert!(error.message.contains("tiene 43"), "{}", error.message);
    let short = h
        .service
        .update_task(&task.id, Some("blocked (AGOS-580)".into()), None, None)
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
            base: None,
            branch: None,
            run_id: Some(run.id.clone()),
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
        "docs(cli): explain a - C-2\ntest(cli): cover a\nfeat(cli): add a"
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

    // A commit rewritten outside (amend) is gone too, and nothing is pushed.
    let asked = h
        .service
        .request_approval(commit_plan(&task, vec![commit("feat: a", &["a.txt"])], &[]))
        .unwrap();
    h.service.decide_approval(&asked.id, true, None).unwrap();
    git(
        &task.worktree,
        &["commit", "-q", "--amend", "-m", "feat: a, reworded"],
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
        })
        .unwrap_err();
    assert_eq!(push.category, "commit_missing");
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
    fn merge(
        &self,
        url: &str,
        method: &str,
        subject: &str,
        _body: &str,
    ) -> Result<(), super::DeliveryError> {
        self.merged
            .lock()
            .unwrap()
            .push(format!("{url} {method} {subject}"));
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
    fn pr_review_comments(
        &self,
        repo: &str,
        number: u64,
    ) -> Result<serde_json::Value, super::DeliveryError> {
        assert_eq!((repo, number), ("o/r", 299));
        Ok(self.review_comments.lock().unwrap().clone())
    }
    fn pr_to_draft(&self, url: &str) -> Result<(), super::DeliveryError> {
        self.actions.lock().unwrap().push(format!("draft {url}"));
        self.snapshot.lock().unwrap()["isDraft"] = json!(true);
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
            "state": "OPEN", "mergeable": "MERGEABLE", "reviewDecision": "APPROVED",
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
    let asked = h.service.request_approval(merge(Some("squash"))).unwrap();
    assert_eq!(asked.merge_method.as_deref(), Some("squash"));
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
        ["https://github.com/o/r/pull/5 squash P-2: paginar (#5)".to_string()]
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
    };
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    let refused = h.service.request_approval(merge()).unwrap_err();
    assert_eq!(refused.category, "pr_not_ready");
    assert!(
        refused.message.contains("no tiene una revisión aprobada"),
        "{}",
        refused.message
    );

    // A repo without required reviews: one approval is enough.
    github.snapshot.lock().unwrap()["reviews"] = json!([
        {"author": {"login": "ana"}, "state": "CHANGES_REQUESTED", "submittedAt": "2026-10-14T08:00:00Z"},
        {"author": {"login": "ana"}, "state": "APPROVED", "submittedAt": "2026-10-14T09:00:00Z"}
    ]);
    h.service.refresh_pr(&task.id, Duration::ZERO).unwrap();
    assert_eq!(
        h.service.task_pr(&task.id).unwrap().unwrap().approved_by,
        vec!["ana".to_string()]
    );
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
            other => panic!("unexpected Jira call {other:?}"),
        }
    }
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
fn automatic_capacity_runs_one_agent_per_active_task() {
    let automatic = DeliverySettings { capacity: 0 };
    assert_eq!(automatic.effective_capacity(0), 1);
    assert_eq!(automatic.effective_capacity(5), 5);
    assert_eq!(automatic.effective_capacity(12), 8);
    assert_eq!(DeliverySettings { capacity: 2 }.effective_capacity(5), 2);

    let h = harness();
    h.service.update_settings(automatic).unwrap();
    for key in ["C-1", "C-2", "C-3", "C-4"] {
        h.task(key);
    }
    let overview = h.service.overview().unwrap();
    assert_eq!((overview.settings.capacity, overview.capacity), (0, 4));
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
    assert!(h.service.change_decision(&own.id, "Quizá").is_err());
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
        .mark_coordinator_notified(&wakeup.run_id, wakeup.cursor)
        .unwrap();
    assert!(h.service.pending_wakeups(later).unwrap().is_empty());
    assert!(h
        .service
        .run(&coordinator.run_id)
        .unwrap()
        .notified_at_ms
        .is_some());
}

#[test]
fn a_coordinator_inside_wait_run_is_not_woken() {
    let h = harness();
    let coordinator = Coordinator::start(&h);
    coordinator.task("F-4");
    coordinator.write_from("session-2", "wait_run", json!({}));
    let service = h.service.clone();
    let run_id = coordinator.run_id.clone();
    let waiting = std::thread::spawn(move || {
        service
            .wait_run(&run_id, Some(u64::MAX), Duration::from_secs(2), true)
            .unwrap()
    });
    std::thread::sleep(Duration::from_millis(300));
    h.service.set_run_paused(&coordinator.run_id, true).unwrap();
    let later = super::now_ms() + super::feed::WAKE_QUIET_MS + 1;
    // The wait hands it the change; no message is needed.
    let pending = h.service.pending_wakeups(later).unwrap();
    waiting.join().unwrap();
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
    assert_eq!(events.len(), 1);
    assert!(events[0]["text"].as_str().unwrap().contains("(#1, #3)"));
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
