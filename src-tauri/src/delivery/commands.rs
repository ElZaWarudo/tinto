//! Tauri commands for the Delivery view. Slow work (git, snapshots, pushes)
//! runs off the async runtime.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, State};

use super::coordination::NewApproval;
use super::model::{
    DeliveryAccess, DeliveryAgent, DeliveryApproval, DeliveryDecision, DeliveryJob, DeliveryJobLog,
    DeliveryLease, DeliveryOverview, DeliveryRepoSettings, DeliveryRun, DeliveryRung,
    DeliverySettings, DeliveryTask,
};
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

#[tauri::command]
pub fn delivery_overview(
    service: State<'_, DeliveryService>,
) -> Result<DeliveryOverview, CommandError> {
    service.overview().map_err(CommandError::from)
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
) -> Result<DeliveryJob, CommandError> {
    service
        .dispatch(NewJob {
            task_id,
            role,
            agent,
            model,
            access: access.unwrap_or_default(),
            prompt,
            writes,
            lease,
            timeout_minutes,
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

#[tauri::command]
pub fn delivery_job_log(
    service: State<'_, DeliveryService>,
    job_id: String,
    from_line: Option<usize>,
) -> Result<DeliveryJobLog, CommandError> {
    service
        .job_log(&job_id, from_line.unwrap_or(0))
        .map_err(CommandError::from)
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
    service
        .store()
        .and_then(|store| store.repo_settings(&repo))
        .map_err(CommandError::from)
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
        .release_lease(&name, &note)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn delivery_request_approval(
    service: State<'_, DeliveryService>,
    task_id: String,
    rung: DeliveryRung,
    title: String,
    body: Option<String>,
) -> Result<DeliveryApproval, CommandError> {
    service
        .request_approval(NewApproval {
            task_id,
            rung,
            title,
            body: body.unwrap_or_default(),
            requested_by: "user".to_string(),
        })
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn delivery_decide_approval(
    service: State<'_, DeliveryService>,
    approval_id: String,
    approve: bool,
    note: Option<String>,
) -> Result<DeliveryApproval, CommandError> {
    let service = service.inner().clone();
    blocking(move || service.decide_approval(&approval_id, approve, note)).await
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

#[tauri::command]
pub fn delivery_answer_decision(
    service: State<'_, DeliveryService>,
    decision_id: String,
    answer: String,
) -> Result<DeliveryDecision, CommandError> {
    service
        .answer_decision(&decision_id, &answer, "user")
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

/// Continues a finished Codex job as a conversation in Agents.
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
    if job.agent != DeliveryAgent::Codex {
        return Err(CommandError::new(
            "resume_unsupported",
            "Agents solo puede continuar trabajos de Codex",
        ));
    }
    if job.status.is_open() {
        return Err(CommandError::new(
            "job_open",
            "espera a que el trabajo termine",
        ));
    }
    let thread = job.provider_session_id.clone().ok_or_else(|| {
        CommandError::new(
            "resume_unavailable",
            "el trabajo no registró su hilo de Codex",
        )
    })?;
    let task = service.task(&job.task_id).map_err(CommandError::from)?;
    let (session_id, repo) = crate::agent_console::commands::open_worktree_conversation(
        &app,
        &bus,
        &workbenches,
        &registry,
        &task.worktree,
        task.distro.as_deref(),
        &task.key,
        "codex",
        Some(&thread),
    )
    .await?;
    Ok(DeliveryConversation {
        session_id,
        repo,
        agent_type: "codex".to_string(),
    })
}
