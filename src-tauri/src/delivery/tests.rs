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
    DeliveryRepoSettings, DeliveryResultState, DeliveryRung, DeliverySettings, DeliveryStageAgent,
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
                note: None,
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
        commits: Vec::new(),
        leave_out: Vec::new(),
        merge_method: None,
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
    let fixed = call(10, "tools/call", review)["result"]["structuredContent"].clone();
    assert_eq!(fixed["agent"], "codex", "{fixed}");
    assert_eq!(fixed["model"], "gpt-6-sol");
    assert_eq!(fixed["reasoning"], "high");
    h.wait(fixed["id"].as_str().unwrap());
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

fn choice(question: &str, recommended: Option<usize>) -> super::decisions::NewDecision {
    super::decisions::NewDecision {
        kind: DeliveryDecisionKind::Choice,
        question: question.into(),
        detail: "flag --dir".into(),
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
    }
}

fn permission(command: &str) -> super::decisions::NewDecision {
    super::decisions::NewDecision {
        kind: DeliveryDecisionKind::Permission,
        question: "¿Instalar en tu ~/.claude real?".into(),
        detail: String::new(),
        options: Vec::new(),
        text: String::new(),
        command: Some(command.into()),
        undo: "agentos skills uninstall demo".into(),
        scripts: Vec::new(),
        read_dirs: Vec::new(),
        write_dirs: Vec::new(),
        clients: Vec::new(),
        full_access: false,
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
        asked["structuredContent"].as_array().unwrap().len(),
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
        asked["structuredContent"][1]["read_dirs"],
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
    assert_eq!(read["structuredContent"][0]["status"], "pending");
    assert_eq!(read["structuredContent"][1]["kind"], "permission");
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
            "you can run only: bash {}.",
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
        let decision = &asked["structuredContent"][0];
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
        merge_method: None,
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
    let again = h
        .service
        .request_approval(commit_plan(
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
        ))
        .unwrap();
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
            merge_method: None,
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
            merge_method: None,
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
    /// What `gh pr list --head <branch>` answers, for any branch.
    branch_prs: std::sync::Mutex<serde_json::Value>,
}

impl FakeGitHub {
    fn new(snapshot: serde_json::Value, branch_prs: serde_json::Value) -> Arc<Self> {
        Arc::new(Self {
            snapshot: std::sync::Mutex::new(snapshot),
            feed_calls: std::sync::atomic::AtomicUsize::new(0),
            mentioned: Vec::new(),
            merged: std::sync::Mutex::new(Vec::new()),
            branch_prs: std::sync::Mutex::new(branch_prs),
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
        branch_prs: std::sync::Mutex::new(json!([])),
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
        branch_prs: std::sync::Mutex::new(json!([])),
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
        merge_method: method.map(str::to_string),
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
    let github = FakeGitHub::new(json!({"state": "OPEN"}), json!([]));
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
        json!({"state": "OPEN"}),
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
        merge_method: None,
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
