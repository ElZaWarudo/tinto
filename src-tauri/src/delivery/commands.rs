//! Tauri commands for the Delivery view. Slow work (git, snapshots, pushes)
//! runs off the async runtime.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, State};

use super::coordination::{DeliveryCommitDraft, NewApproval};
use super::model::{
    DeliveryAccess, DeliveryAgent, DeliveryApproval, DeliveryCommit, DeliveryDecision,
    DeliveryJiraSettings, DeliveryJob, DeliveryJobLog, DeliveryLease, DeliveryOverview,
    DeliveryPlatform, DeliveryRepoSettings, DeliveryRun, DeliveryRung, DeliverySettings,
    DeliverySettingsChanges, DeliverySettingsProposal, DeliveryStageAgent, DeliveryTask,
};
use super::prs::DeliveryPr;
use super::service::{DeliveryService, NewJob, NewTask};
use super::tasks::plain_path;
use super::DeliveryError;
use crate::agent_console::commands::CommandError;
use crate::agent_console::AgentSessionRegistry;
use crate::bus::contract::AgentSessionStatus;
use crate::bus::BusHandle;
use crate::workbench::WorkbenchStore;

pub const EVENT_DELIVERY_CHANGED: &str = "tinto://delivery-changed";

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, DeliveryError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| CommandError::new("delivery_task_failed", error.to_string()))?
        .map_err(CommandError::from)
}

/// Off the main thread: it is read after every change while agents work,
/// and a busy main thread is a window that does not answer clicks.
#[tauri::command]
pub async fn delivery_overview(
    service: State<'_, DeliveryService>,
) -> Result<DeliveryOverview, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.overview()).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn delivery_create_task(
    service: State<'_, DeliveryService>,
    repo: PathBuf,
    distro: Option<String>,
    key: String,
    title: String,
    base: Option<String>,
    branch: Option<String>,
    run_id: Option<String>,
) -> Result<DeliveryTask, CommandError> {
    let service = service.inner().clone();
    blocking(move || {
        service.create_task(NewTask {
            repo,
            distro,
            key,
            title,
            base,
            branch,
            run_id,
        })
    })
    .await
}

#[tauri::command]
pub async fn delivery_remove_task(
    service: State<'_, DeliveryService>,
    bus: State<'_, BusHandle>,
    workbenches: State<'_, Mutex<WorkbenchStore>>,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    task_id: String,
    force: bool,
) -> Result<(), CommandError> {
    let worktree = service.task(&task_id).map_err(CommandError::from)?.worktree;
    // A live conversation keeps the folder in use, and Windows refuses to
    // delete it halfway through.
    if agents_working_in(&registry, &worktree) {
        return Err(CommandError::new(
            "agent_session_active",
            "Hay una conversación de Agents activa en este worktree. Detenla antes de eliminar la tarea.",
        ));
    }
    let service = service.inner().clone();
    blocking(move || service.remove_task(&task_id, force)).await?;
    forget_worktree(&bus, &workbenches, &worktree);
    Ok(())
}

fn agents_working_in(registry: &Mutex<AgentSessionRegistry>, worktree: &Path) -> bool {
    let Ok(mut registry) = registry.lock() else {
        return false;
    };
    let _ = registry.refresh_session_statuses();
    let target = plain_path(worktree).replace('\\', "/");
    registry.list_sessions().iter().any(|session| {
        matches!(
            session.status,
            AgentSessionStatus::Starting | AgentSessionStatus::Running
        ) && plain_path(&session.repo)
            .replace('\\', "/")
            .eq_ignore_ascii_case(&target)
    })
}

/// Drops a removed task's worktree from the workbenches, where "Abrir en
/// Agents" may have added it.
fn forget_worktree(bus: &BusHandle, workbenches: &Mutex<WorkbenchStore>, worktree: &Path) {
    let Ok(mut store) = workbenches.lock() else {
        return;
    };
    let names: Vec<String> = store
        .config()
        .workbenches
        .iter()
        .map(|workbench| workbench.name.clone())
        .collect();
    let path = worktree.to_string_lossy();
    let mut removed = false;
    for name in names {
        removed |= store.remove_repo_entry(&name, &path).unwrap_or(false);
    }
    if removed {
        if let Some(active) = store.active_workbench_runtime() {
            bus.set_workbench(active.repos);
        }
    }
}

#[tauri::command]
pub fn delivery_update_task(
    service: State<'_, DeliveryService>,
    task_id: String,
    state: Option<String>,
    contract_version: Option<u32>,
    title: Option<String>,
) -> Result<DeliveryTask, CommandError> {
    service
        .update_task(&task_id, state, contract_version, title)
        .map_err(CommandError::from)
}

/// The user blocks a task on something more.
#[tauri::command]
pub fn delivery_add_blocker(
    service: State<'_, DeliveryService>,
    task_id: String,
    blocker: super::model::NewBlocker,
) -> Result<DeliveryTask, CommandError> {
    service
        .add_blocker(&task_id, blocker)
        .map_err(CommandError::from)
}

/// The user clears some blockers, or overrides the block (`None`: all).
#[tauri::command]
pub fn delivery_clear_blockers(
    service: State<'_, DeliveryService>,
    task_id: String,
    blocker_ids: Option<Vec<String>>,
) -> Result<DeliveryTask, CommandError> {
    service
        .clear_blockers(&task_id, blocker_ids.as_deref(), true)
        .map_err(CommandError::from)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn delivery_dispatch_job(
    service: State<'_, DeliveryService>,
    task_id: String,
    role: String,
    agent: DeliveryAgent,
    model: Option<String>,
    access: Option<DeliveryAccess>,
    prompt: String,
    writes: Option<bool>,
    lease: Option<String>,
    timeout_minutes: Option<u32>,
    reasoning: Option<String>,
    platform: Option<DeliveryPlatform>,
) -> Result<DeliveryJob, CommandError> {
    service
        .dispatch(NewJob {
            platform,
            task_id,
            role,
            agent,
            model,
            access: access.unwrap_or_default(),
            prompt,
            writes,
            lease,
            timeout_minutes,
            reasoning,
        })
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_cancel_job(
    service: State<'_, DeliveryService>,
    job_id: String,
) -> Result<DeliveryJob, CommandError> {
    service.cancel(&job_id).map_err(CommandError::from)
}

/// The page of the task's Jira issue, when Tinto knows the repo's Jira host.
#[tauri::command]
pub fn delivery_issue_url(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<Option<String>, CommandError> {
    service.issue_url(&task_id).map_err(CommandError::from)
}

/// Opens a QA job's visual changes page with the system's default browser.
#[tauri::command]
pub fn delivery_open_visual_record(
    service: State<'_, DeliveryService>,
    job_id: String,
) -> Result<(), CommandError> {
    let job = service.job(&job_id).map_err(CommandError::from)?;
    let page = job
        .visual_record
        .filter(|page| std::path::Path::new(page).is_file())
        .ok_or_else(|| CommandError::new("not_found", "este trabajo no tiene registro visual"))?;
    open_with_system(&page)
}

/// Opens a web page (a Jira issue, a PR) in the system's browser: the app's
/// webview does not follow links that would open a new window.
#[tauri::command]
pub fn delivery_open_url(url: String) -> Result<(), CommandError> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(CommandError::new(
            "invalid_url",
            "solo se abren direcciones web",
        ));
    }
    open_with_system(&url)
}

/// A file or address, with what the system opens it with. No shell is
/// involved, so the text reaches the program as one argument.
fn open_with_system(target: &str) -> Result<(), CommandError> {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| CommandError::new("open_failed", error.to_string()))
}

#[tauri::command]
pub fn delivery_pause_job(
    service: State<'_, DeliveryService>,
    job_id: String,
) -> Result<DeliveryJob, CommandError> {
    service.pause(&job_id).map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_resume_job(
    service: State<'_, DeliveryService>,
    job_id: String,
) -> Result<DeliveryJob, CommandError> {
    service.resume(&job_id).map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_set_run_paused(
    service: State<'_, DeliveryService>,
    run_id: String,
    paused: bool,
) -> Result<DeliveryRun, CommandError> {
    service
        .set_run_paused(&run_id, paused)
        .map_err(CommandError::from)
}

/// The user stops or allows new tasks in a batch.
#[tauri::command]
pub fn delivery_set_run_no_new_tasks(
    service: State<'_, DeliveryService>,
    run_id: String,
    no_new_tasks: bool,
) -> Result<DeliveryRun, CommandError> {
    service
        .set_run_no_new_tasks(&run_id, no_new_tasks)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_retry_job(
    service: State<'_, DeliveryService>,
    job_id: String,
) -> Result<DeliveryJob, CommandError> {
    service.retry(&job_id).map_err(CommandError::from)
}

#[tauri::command]
pub async fn delivery_undo_job(
    service: State<'_, DeliveryService>,
    job_id: String,
) -> Result<DeliveryJob, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.undo(&job_id)).await
}

/// Off the main thread: a running job's log is read again every second or so.
#[tauri::command]
pub async fn delivery_job_log(
    service: State<'_, DeliveryService>,
    job_id: String,
    from_line: Option<usize>,
) -> Result<DeliveryJobLog, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.job_log(&job_id, from_line.unwrap_or(0))).await
}

#[tauri::command]
pub fn delivery_update_settings(
    service: State<'_, DeliveryService>,
    capacity: u32,
) -> Result<DeliverySettings, CommandError> {
    service
        .update_settings(DeliverySettings { capacity })
        .map_err(CommandError::from)
}

/// The model Codex jobs use when they do not name one: the default of the
/// account's model catalog, reported by the view.
#[tauri::command]
pub fn delivery_set_codex_model(
    service: State<'_, DeliveryService>,
    model: Option<String>,
) -> Result<(), CommandError> {
    service.set_codex_model(model).map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_repo_settings(
    service: State<'_, DeliveryService>,
    repo: PathBuf,
) -> Result<DeliveryRepoSettings, CommandError> {
    let store = service.store()?;
    let mut settings = store.repo_settings(&repo)?;
    if let Some(jira) = settings.jira.as_mut() {
        jira.has_token = store.jira_token(&repo)?.is_some();
    }
    Ok(settings)
}

/// Archives a delivered task: its worktree and local branch go.
#[tauri::command]
pub async fn delivery_archive_task(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<(), CommandError> {
    let service = service.inner().clone();
    blocking(move || service.archive_task(&task_id)).await
}

/// Archived tasks by issue key or PR number.
#[tauri::command]
pub fn delivery_archived_tasks(
    service: State<'_, DeliveryService>,
    query: String,
) -> Result<Vec<super::model::DeliveryArchivedTask>, CommandError> {
    service.archived_tasks(&query).map_err(CommandError::from)
}

/// Brings an archived task back, with its worktree.
#[tauri::command]
pub async fn delivery_unarchive_task(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<DeliveryTask, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.unarchive_task(&task_id)).await
}

/// Saves the repo's Jira token apart from its settings; empty forgets it.
#[tauri::command]
pub fn delivery_set_jira_token(
    service: State<'_, DeliveryService>,
    repo: PathBuf,
    token: String,
) -> Result<(), CommandError> {
    service
        .set_jira_token(&repo, &token)
        .map_err(CommandError::from)
}

/// "Comprobar Jira": the name of the token's owner, as Jira sees it.
#[tauri::command]
pub async fn delivery_check_jira(
    service: State<'_, DeliveryService>,
    repo: PathBuf,
) -> Result<String, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.check_jira(&repo)).await
}

#[tauri::command]
pub fn delivery_set_repo_settings(
    service: State<'_, DeliveryService>,
    repo: PathBuf,
    settings: DeliveryRepoSettings,
) -> Result<DeliveryRepoSettings, CommandError> {
    let clean = |value: Option<String>| {
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let settings = DeliveryRepoSettings {
        worktree_root: settings
            .worktree_root
            .filter(|root| !root.as_os_str().is_empty()),
        bootstrap: clean(settings.bootstrap),
        default_base: clean(settings.default_base),
        checks: settings
            .checks
            .into_iter()
            .filter_map(|check| clean(Some(check)))
            .collect(),
        qa_commands: settings
            .qa_commands
            .into_iter()
            .filter_map(|command| clean(Some(command)))
            .collect(),
        qa_browser: settings.qa_browser,
        qa_environment: settings.qa_environment.trim().to_string(),
        verification: settings.verification.trim().to_string(),
        paid_model_calls: settings.paid_model_calls,
        // Zero means no limit was set: the default applies.
        single_commit_max_files: settings.single_commit_max_files.filter(|limit| *limit > 0),
        single_commit_max_lines: settings.single_commit_max_lines.filter(|limit| *limit > 0),
        stages: settings
            .stages
            .into_iter()
            .filter(|(_, stage)| stage.agent != DeliveryAgent::Shell)
            .map(|(role, stage)| {
                let stage = DeliveryStageAgent {
                    agent: stage.agent,
                    model: clean(stage.model),
                    reasoning: clean(stage.reasoning),
                };
                (role.trim().to_ascii_lowercase(), stage)
            })
            .collect(),
        jira: match settings.jira {
            Some(jira) if !jira.host.trim().is_empty() => {
                let statuses = |list: Vec<String>| -> Vec<String> {
                    list.into_iter().filter_map(|s| clean(Some(s))).collect()
                };
                Some(DeliveryJiraSettings {
                    host: super::jira::clean_host(&jira.host)?,
                    project: jira.project.trim().to_ascii_uppercase(),
                    after_pr: statuses(jira.after_pr),
                    after_merge: statuses(jira.after_merge),
                    has_token: false,
                })
            }
            _ => None,
        },
    };
    service
        .store()
        .and_then(|store| store.set_repo_settings(&repo, &settings))
        .map(|_| settings)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_release_lease(
    service: State<'_, DeliveryService>,
    name: String,
    note: String,
) -> Result<DeliveryLease, CommandError> {
    if note.trim().is_empty() {
        return Err(CommandError::new(
            "confirmation_required",
            "describe cómo comprobaste que el entorno quedó limpio",
        ));
    }
    service
        .release_lease(&name, &note, "el usuario")
        .map_err(CommandError::from)
}

// Tauri passes each field the view sends as its own argument.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn delivery_request_approval(
    service: State<'_, DeliveryService>,
    task_id: String,
    rung: DeliveryRung,
    title: String,
    body: Option<String>,
    commits: Option<Vec<DeliveryCommit>>,
    leave_out: Option<Vec<String>>,
    merge_method: Option<String>,
    stage: Option<String>,
    jira_statuses: Option<Vec<String>>,
) -> Result<DeliveryApproval, CommandError> {
    service
        .request_approval(NewApproval {
            task_id,
            rung,
            title,
            body: body.unwrap_or_default(),
            requested_by: "user".to_string(),
            commits: commits.unwrap_or_default(),
            leave_out: leave_out.unwrap_or_default(),
            single_commit_reason: None,
            merge_method,
            stage,
            jira_statuses: jira_statuses.unwrap_or_default(),
        })
        .map_err(CommandError::from)
}

/// The merge method the base branch's history shows: `merge` or `squash`.
#[tauri::command]
pub async fn delivery_merge_method(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<String, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.merge_method(&task_id)).await
}

/// The changed files and recent commit subjects a commit plan starts from.
#[tauri::command]
pub async fn delivery_commit_draft(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<DeliveryCommitDraft, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.commit_draft(&task_id)).await
}

#[tauri::command]
pub async fn delivery_undo_commit(
    service: State<'_, DeliveryService>,
    approval_id: String,
) -> Result<DeliveryApproval, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.undo_commit(&approval_id)).await
}

#[tauri::command]
pub async fn delivery_decide_approval(
    service: State<'_, DeliveryService>,
    approval_id: String,
    approve: bool,
    note: Option<String>,
    // A PR step: who the user chose to review it.
    reviewers: Option<Vec<String>>,
) -> Result<DeliveryApproval, CommandError> {
    let service = service.inner().clone();
    blocking(move || {
        if let Some(reviewers) = reviewers.filter(|_| approve) {
            service.set_pr_reviewers(&approval_id, reviewers)?;
        }
        service.decide_approval(&approval_id, approve, note)
    })
    .await
}

/// Who can review the task's PR: the repo's assignable people on GitHub.
#[tauri::command]
pub async fn delivery_pr_reviewers(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<Vec<String>, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.pr_reviewers(&task_id)).await
}

#[tauri::command]
pub fn delivery_complete_approval(
    service: State<'_, DeliveryService>,
    approval_id: String,
    success: bool,
    outcome: String,
) -> Result<DeliveryApproval, CommandError> {
    service
        .complete_approval(&approval_id, success, &outcome)
        .map_err(CommandError::from)
}

/// The user corrects a done step's result, saying why. Not on MCP: the
/// correction is the user's, not the coordinator's.
#[tauri::command]
pub fn delivery_correct_approval(
    service: State<'_, DeliveryService>,
    approval_id: String,
    outcome: String,
    reason: String,
) -> Result<DeliveryApproval, CommandError> {
    service
        .correct_approval(&approval_id, &outcome, &reason)
        .map_err(CommandError::from)
}

/// Allowing a permission that runs the task's code snapshots the worktree.
#[tauri::command]
pub async fn delivery_answer_decision(
    service: State<'_, DeliveryService>,
    decision_id: String,
    answer: String,
) -> Result<DeliveryDecision, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.answer_decision(&decision_id, &answer, "user")).await
}

/// The user approves a coordinator's settings proposal, or the part of it
/// in `accepted`.
#[tauri::command]
pub fn delivery_approve_settings_proposal(
    service: State<'_, DeliveryService>,
    proposal_id: String,
    accepted: DeliverySettingsChanges,
) -> Result<DeliverySettingsProposal, CommandError> {
    service
        .approve_settings_proposal(&proposal_id, accepted, "user")
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_reject_settings_proposal(
    service: State<'_, DeliveryService>,
    proposal_id: String,
) -> Result<DeliverySettingsProposal, CommandError> {
    service
        .reject_settings_proposal(&proposal_id, "user")
        .map_err(CommandError::from)
}

/// The user changes what the coordinator decided on its own.
#[tauri::command]
pub fn delivery_change_decision(
    service: State<'_, DeliveryService>,
    decision_id: String,
    answer: String,
) -> Result<DeliveryDecision, CommandError> {
    service
        .change_decision(&decision_id, &answer)
        .map_err(CommandError::from)
}

/// Allows again, for the worktree as it is now, every permission of the
/// task that runs its code.
#[tauri::command]
pub async fn delivery_renew_permissions(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<Vec<DeliveryDecision>, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.renew_permissions(&task_id, "user")).await
}

#[tauri::command]
pub fn delivery_withdraw_decision(
    service: State<'_, DeliveryService>,
    decision_id: String,
) -> Result<(), CommandError> {
    service
        .withdraw_decision(&decision_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_accept_recommended(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<Vec<DeliveryDecision>, CommandError> {
    service
        .accept_recommended(&task_id, "user")
        .map_err(CommandError::from)
}

/// Reads the task's PR now, unless it was read in the last 30 seconds.
#[tauri::command]
pub fn delivery_refresh_pr(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<Option<DeliveryPr>, CommandError> {
    service
        .refresh_pr(&task_id, std::time::Duration::from_secs(30))
        .map_err(CommandError::from)
}

/// What opening a task checks: its PR is linked (found by its branch when
/// the PR step has no URL) and recently read.
#[tauri::command]
pub async fn delivery_check_task(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<Option<DeliveryPr>, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.check_task(&task_id)).await
}

/// "Buscar el PR de la rama": links the PR GitHub has for the task's branch.
#[tauri::command]
pub async fn delivery_find_pr(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<Option<DeliveryPr>, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.find_task_pr(&task_id)).await
}

/// A problem QA missed, as the user describes it.
#[tauri::command]
pub fn delivery_report_qa_problem(
    service: State<'_, DeliveryService>,
    task_id: String,
    text: String,
    place: String,
) -> Result<super::model::DeliveryQaProblem, CommandError> {
    service
        .report_qa_problem(&task_id, &text, &place)
        .map_err(CommandError::from)
}

/// The user decides a QA problem is not one.
#[tauri::command]
pub fn delivery_dismiss_qa_problem(
    service: State<'_, DeliveryService>,
    problem_id: String,
    reason: String,
) -> Result<super::model::DeliveryQaProblem, CommandError> {
    service
        .dismiss_qa_problem(&problem_id, &reason)
        .map_err(CommandError::from)
}

/// The user sends QA problems to fix, each with its solution.
#[tauri::command]
pub fn delivery_send_qa_problems(
    service: State<'_, DeliveryService>,
    task_id: String,
    choices: Vec<super::qa_problems::QaProblemChoice>,
) -> Result<Vec<super::model::DeliveryQaProblem>, CommandError> {
    service
        .send_qa_problems(&task_id, &choices)
        .map_err(CommandError::from)
}

/// Opens an approved PR step's PR on GitHub with its approved text.
#[tauri::command]
pub async fn delivery_open_pr(
    service: State<'_, DeliveryService>,
    approval_id: String,
) -> Result<DeliveryApproval, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.open_pr_step(&approval_id)).await
}

/// Closes the task's PR on GitHub without merging it, for a delivery the
/// user stopped.
#[tauri::command]
pub async fn delivery_close_pr(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<Option<DeliveryPr>, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.close_task_pr(&task_id)).await
}

/// Turns the task's PR back into a draft on GitHub.
#[tauri::command]
pub async fn delivery_pr_to_draft(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<Option<DeliveryPr>, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.task_pr_to_draft(&task_id)).await
}

#[tauri::command]
pub fn delivery_pr_seen(
    service: State<'_, DeliveryService>,
    task_id: String,
) -> Result<Option<DeliveryPr>, CommandError> {
    service.mark_pr_seen(&task_id).map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_create_run(
    service: State<'_, DeliveryService>,
    repo: PathBuf,
    title: String,
    qa_jira_comment: Option<bool>,
) -> Result<DeliveryRun, CommandError> {
    service
        .create_run(repo, &title, None, qa_jira_comment)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_set_run_qa_jira_comment(
    service: State<'_, DeliveryService>,
    run_id: String,
    post: bool,
) -> Result<DeliveryRun, CommandError> {
    service
        .set_run_qa_jira_comment(&run_id, post)
        .map_err(CommandError::from)
}

/// The user takes the run back: the coordinator's generation is fenced.
#[tauri::command]
pub fn delivery_takeover_run(
    service: State<'_, DeliveryService>,
    run_id: String,
) -> Result<DeliveryRun, CommandError> {
    service
        .takeover_run(
            &run_id,
            "user",
            None,
            "el usuario tomó el control desde Tinto",
        )
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_close_run(
    service: State<'_, DeliveryService>,
    run_id: String,
) -> Result<DeliveryRun, CommandError> {
    service.close_run(&run_id).map_err(CommandError::from)
}

/// What a task changed since its worktree was created, to open in the
/// files view.
#[derive(Debug, Clone, Serialize)]
pub struct DeliveryTaskChanges {
    /// The worktree as a repo of the active workbench.
    pub repo: PathBuf,
    pub base_ref: String,
    pub base_commit: String,
    pub files: Vec<crate::git::FileDiff>,
}

#[tauri::command]
pub async fn delivery_task_changes(
    service: State<'_, DeliveryService>,
    bus: State<'_, BusHandle>,
    workbenches: State<'_, Mutex<WorkbenchStore>>,
    task_id: String,
) -> Result<DeliveryTaskChanges, CommandError> {
    let task = service.task(&task_id).map_err(CommandError::from)?;
    if task.distro.is_some() {
        return Err(CommandError::new(
            "unsupported",
            "ver los cambios todavía no está disponible para worktrees dentro de WSL",
        ));
    }
    // Agents (often from WSL) touch files in ways that leave git's index
    // stale, and libgit2 would hash every tracked file again on each read
    // without saving it: git refreshes the index once and saves it, so both
    // Tinto's first read of the repo and the diff below stay quick.
    let refreshed = task.worktree.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        super::tasks::git(
            super::tasks::Place::Native,
            &refreshed,
            &["update-index", "-q", "--refresh"],
        )
    })
    .await;
    let resolved = crate::agent_console::commands::ensure_worktree_repo(
        &bus,
        &workbenches,
        &task.worktree,
        None,
        &task.key,
    )
    .await?;
    let worktree = task.worktree.clone();
    let base = task.base_commit.clone();
    let files = tauri::async_runtime::spawn_blocking(move || {
        crate::git::Git2Engine::open(&worktree)?.diff_against(&base)
    })
    .await
    .map_err(|error| CommandError::new("internal", error.to_string()))?
    .map_err(|error| CommandError::new("git_error", error.to_string()))?;
    Ok(DeliveryTaskChanges {
        repo: resolved.path,
        base_ref: task.base_ref,
        base_commit: task.base_commit,
        files,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct DeliveryConversation {
    pub session_id: String,
    pub repo: PathBuf,
    pub agent_type: String,
}

#[tauri::command]
pub async fn delivery_open_task_in_agents(
    app: AppHandle,
    service: State<'_, DeliveryService>,
    bus: State<'_, BusHandle>,
    workbenches: State<'_, Mutex<WorkbenchStore>>,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    task_id: String,
    agent_type: String,
) -> Result<DeliveryConversation, CommandError> {
    let task = service.task(&task_id).map_err(CommandError::from)?;
    let (session_id, repo) = crate::agent_console::commands::open_worktree_conversation(
        &app,
        &bus,
        &workbenches,
        &registry,
        &task.worktree,
        task.distro.as_deref(),
        &task.key,
        &agent_type,
        None,
    )
    .await?;
    Ok(DeliveryConversation {
        session_id,
        repo,
        agent_type,
    })
}

/// Continues a finished Codex or Claude Code job as a conversation in Agents.
#[tauri::command]
pub async fn delivery_open_job_in_agents(
    app: AppHandle,
    service: State<'_, DeliveryService>,
    bus: State<'_, BusHandle>,
    workbenches: State<'_, Mutex<WorkbenchStore>>,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    job_id: String,
) -> Result<DeliveryConversation, CommandError> {
    let job = service.job(&job_id).map_err(CommandError::from)?;
    let agent_type = match job.agent {
        DeliveryAgent::Codex => "codex",
        DeliveryAgent::Claude => "claude",
        DeliveryAgent::Muse => "muse",
        DeliveryAgent::Shell => {
            return Err(CommandError::new(
                "resume_unsupported",
                "Agents solo puede continuar trabajos de Codex, Claude Code o Muse Code",
            ))
        }
    };
    if job.status.is_open() {
        return Err(CommandError::new(
            "job_open",
            "espera a que el trabajo termine",
        ));
    }
    let conversation = job.provider_session_id.clone().ok_or_else(|| {
        CommandError::new(
            "resume_unavailable",
            "el trabajo no registró su conversación",
        )
    })?;
    let task = service.task(&job.task_id).map_err(CommandError::from)?;
    // Agents runs a Windows repo's agent on Windows; Claude keeps its
    // conversations where it ran.
    if task.distro.is_none()
        && job
            .ran_on
            .as_deref()
            .is_some_and(|on| on.starts_with("WSL"))
    {
        return Err(CommandError::new(
            "resume_unsupported",
            "este trabajo de Claude Code corrió en WSL sobre un repositorio de Windows; Agents solo puede continuarlo en Windows",
        ));
    }
    let (session_id, repo) = crate::agent_console::commands::open_worktree_conversation(
        &app,
        &bus,
        &workbenches,
        &registry,
        &task.worktree,
        task.distro.as_deref(),
        &task.key,
        agent_type,
        Some(&conversation),
    )
    .await?;
    Ok(DeliveryConversation {
        session_id,
        repo,
        agent_type: agent_type.to_string(),
    })
}
