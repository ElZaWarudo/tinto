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
    DeliveryAccess, DeliveryAgent, DeliveryApprovalStatus, DeliveryJob, DeliveryJobStatus,
    DeliveryLeaseState, DeliveryRepoSettings, DeliveryResultState, DeliveryRung, DeliverySettings,
    DeliveryTask,
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
/// exit=0 sleep=0`.
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
        #[cfg(target_os = "windows")]
        {
            let mut lines = vec!["@echo off".to_string()];
            lines.push(format!("type \"{}\"", events_file.display()));
            if let Some(path) = &touch {
                lines.push(format!("echo changed>\"{}\"", path.display()));
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
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut lines = vec![format!("cat '{}'", events_file.display())];
            if let Some(path) = &touch {
                lines.push(format!("echo changed > '{}'", path.display()));
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
                checks: Vec::new(),
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
    };
    let claude = h
        .service
        .dispatch(new_job(DeliveryAgent::Claude, DeliveryAccess::Workspace))
        .unwrap();
    assert_eq!(claude.model, None);
    assert_eq!(claude.allowed_commands, vec!["npm test".to_string()]);
    h.wait(&claude.id);
    let full = h
        .service
        .dispatch(new_job(DeliveryAgent::Claude, DeliveryAccess::Full))
        .unwrap();
    assert!(full.allowed_commands.is_empty());
    h.wait(&full.id);
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
        .release_lease("qa", "browser closed, data restored")
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
        .complete_approval(&pr.id, true, "https://example.test/pr/1")
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
        .create_run(h.repo.path().to_path_buf(), "Batch", Some("coord-a".into()))
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
