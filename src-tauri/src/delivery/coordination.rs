//! Runs and approvals: the coordination the backlog-delivery skill used to
//! keep by hand, made deterministic.
//!
//! - A run groups tasks under one coordinator. Its lock carries a generation
//!   that increases on every takeover; a coordinator acting with an older
//!   generation is fenced.
//! - Approvals climb a ladder (commit → push → PR → Jira), one rung at a
//!   time. Approving a rung never approves the next. Tinto runs the commit
//!   and the push itself; the coordinator runs the PR and Jira steps after
//!   they are approved and reports back.
//! - The commit rung is a plan: one or more commits, each with its exact
//!   message and files. Tinto creates them as written (it adds no trailers
//!   or signatures), can undo them until they are pushed, and notices when
//!   they are no longer on the branch.

use super::model::{
    DeliveryApproval, DeliveryApprovalStatus, DeliveryCommit, DeliveryRun, DeliveryRung,
    DeliveryTask,
};
use super::service::DeliveryService;
use super::tasks::{self, Place};
use super::{now_ms, DeliveryError};

#[derive(Debug, Clone)]
pub struct NewApproval {
    pub task_id: String,
    pub rung: DeliveryRung,
    pub title: String,
    pub body: String,
    pub requested_by: String,
    /// `commit` only: the commits to create, in order. When empty, one
    /// commit with every change and `title` (plus `body`) as its message.
    pub commits: Vec<DeliveryCommit>,
    /// `commit` only: changed files the plan leaves uncommitted on purpose.
    pub leave_out: Vec<String>,
}

/// What the form to prepare a commit starts from.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct DeliveryCommitDraft {
    /// Every file with uncommitted changes.
    pub files: Vec<String>,
    /// Subjects of the branch's latest commits, to follow the repo's
    /// convention.
    pub recent: Vec<String>,
}

fn invalid_plan(message: impl Into<String>) -> DeliveryError {
    DeliveryError::new("invalid_commit_plan", message)
}

/// A path as git names it: relative to the worktree, with `/`.
fn plan_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    path.strip_prefix("./").unwrap_or(&path).to_string()
}

fn subject(message: &str) -> &str {
    message.lines().next().unwrap_or_default().trim()
}

fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

/// A commit plan is valid when every changed file is in exactly one commit
/// or left out on purpose, and nothing else is named.
fn check_plan(
    changed: &[String],
    commits: &[DeliveryCommit],
    leave_out: &[String],
) -> Result<(), DeliveryError> {
    let mut problems = Vec::new();
    // Where each file goes: a commit number, or 0 for left out.
    let mut placed: Vec<(&str, usize)> = Vec::new();
    let place = |index: usize| match index {
        0 => "los que se dejan fuera".to_string(),
        index => format!("el commit {index}"),
    };
    let groups = std::iter::once((0, leave_out)).chain(
        commits
            .iter()
            .enumerate()
            .map(|(index, commit)| (index + 1, commit.paths.as_slice())),
    );
    for (index, paths) in groups {
        for path in paths {
            if let Some((_, first)) = placed.iter().find(|(seen, _)| *seen == path) {
                problems.push(format!(
                    "{path} está en {} y en {}",
                    place(*first),
                    place(index)
                ));
            } else if !changed.contains(path) {
                problems.push(format!(
                    "{path} ({}) no existe o no tiene cambios",
                    place(index)
                ));
            } else {
                placed.push((path, index));
            }
        }
    }
    for (index, commit) in commits.iter().enumerate() {
        if commit.message.trim().is_empty() {
            problems.push(format!("el commit {} no tiene mensaje", index + 1));
        }
        if commit.paths.is_empty() {
            problems.push(format!("el commit {} no incluye archivos", index + 1));
        }
    }
    let unassigned: Vec<&str> = changed
        .iter()
        .map(String::as_str)
        .filter(|path| !placed.iter().any(|(seen, _)| seen == path))
        .collect();
    if !unassigned.is_empty() {
        problems.push(format!(
            "sin asignar: {} (inclúyelos en un commit o déjalos fuera a propósito)",
            unassigned.join(", ")
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(invalid_plan(problems.join("; ")))
    }
}

/// The commits Tinto created for a commit step. Steps from before plans
/// existed only recorded "Commit <short hash>" as their outcome.
fn created_commits(approval: &DeliveryApproval) -> Vec<String> {
    if !approval.created.is_empty() {
        return approval.created.clone();
    }
    approval
        .outcome
        .as_deref()
        .and_then(|outcome| outcome.strip_prefix("Commit "))
        .map(|commit| vec![commit.trim().to_string()])
        .unwrap_or_default()
}

impl DeliveryService {
    // ---- runs ----

    pub fn create_run(
        &self,
        repo: std::path::PathBuf,
        title: &str,
        owner: Option<String>,
        qa_jira_comment: Option<bool>,
    ) -> Result<DeliveryRun, DeliveryError> {
        let now = now_ms();
        let run = DeliveryRun {
            id: format!("run-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            repo,
            title: title.trim().to_string(),
            status: "active".to_string(),
            generation: 1,
            owner,
            created_at_ms: now,
            updated_at_ms: now,
            qa_jira_comment,
        };
        let store = self.store()?;
        store.put_run(&run)?;
        store.record_event(now, "run_created", None, None, &run.id)?;
        drop(store);
        self.notify();
        Ok(run)
    }

    pub fn run(&self, run_id: &str) -> Result<DeliveryRun, DeliveryError> {
        self.store()?
            .run(run_id)?
            .ok_or_else(|| DeliveryError::not_found("la ejecución", run_id))
    }

    /// Takes a free run, or confirms the caller already holds it.
    pub fn acquire_run(&self, run_id: &str, owner: &str) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.active_run(run_id)?;
        match run.owner.as_deref() {
            Some(current) if current != owner => Err(DeliveryError::new(
                "run_locked",
                format!(
                    "la ejecución la coordina {current} (generación {}); usa takeover solo si ese coordinador se detuvo",
                    run.generation
                ),
            )),
            Some(_) => Ok(run),
            None => {
                run.owner = Some(owner.to_string());
                run.updated_at_ms = now_ms();
                self.save_run(&run, "run_acquired", owner)?;
                Ok(run)
            }
        }
    }

    /// Replaces the coordinator. The generation increases, so anything the
    /// previous coordinator still sends is fenced.
    pub fn takeover_run(
        &self,
        run_id: &str,
        owner: &str,
        expect_generation: Option<u64>,
        reason: &str,
    ) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.active_run(run_id)?;
        if let Some(expected) = expect_generation {
            if expected != run.generation {
                return Err(DeliveryError::new(
                    "fenced",
                    format!("la ejecución ya está en la generación {}", run.generation),
                ));
            }
        }
        run.owner = Some(owner.to_string());
        run.generation += 1;
        run.updated_at_ms = now_ms();
        self.save_run(&run, "run_takeover", &format!("{owner}: {}", reason.trim()))?;
        Ok(run)
    }

    pub fn release_run(
        &self,
        run_id: &str,
        owner: &str,
        generation: u64,
    ) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.verify_run(run_id, owner, generation)?;
        run.owner = None;
        run.updated_at_ms = now_ms();
        self.save_run(&run, "run_released", owner)?;
        Ok(run)
    }

    /// The user's choice on posting QA verdicts to Jira for this batch.
    pub fn set_run_qa_jira_comment(
        &self,
        run_id: &str,
        post: bool,
    ) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.run(run_id)?;
        run.qa_jira_comment = Some(post);
        run.updated_at_ms = now_ms();
        self.save_run(
            &run,
            "run_policy",
            if post {
                "qa_jira_comment"
            } else {
                "no_qa_jira_comment"
            },
        )?;
        Ok(run)
    }

    pub fn close_run(&self, run_id: &str) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.run(run_id)?;
        run.status = "closed".to_string();
        run.owner = None;
        run.generation += 1;
        run.updated_at_ms = now_ms();
        self.save_run(&run, "run_closed", "")?;
        Ok(run)
    }

    /// Fencing check for every coordinator write.
    pub fn verify_run(
        &self,
        run_id: &str,
        owner: &str,
        generation: u64,
    ) -> Result<DeliveryRun, DeliveryError> {
        let run = self.active_run(run_id)?;
        if run.owner.as_deref() != Some(owner) || run.generation != generation {
            return Err(DeliveryError::new(
                "fenced",
                format!(
                    "la ejecución está en la generación {} coordinada por {}; esta escritura se descarta",
                    run.generation,
                    run.owner.as_deref().unwrap_or("nadie")
                ),
            ));
        }
        Ok(run)
    }

    fn active_run(&self, run_id: &str) -> Result<DeliveryRun, DeliveryError> {
        let run = self.run(run_id)?;
        if run.status != "active" {
            return Err(DeliveryError::new(
                "run_closed",
                "la ejecución está cerrada",
            ));
        }
        Ok(run)
    }

    fn save_run(&self, run: &DeliveryRun, event: &str, detail: &str) -> Result<(), DeliveryError> {
        let store = self.store()?;
        store.put_run(run)?;
        store.record_event(
            run.updated_at_ms,
            event,
            None,
            None,
            &format!("{} {detail}", run.id),
        )?;
        drop(store);
        self.notify();
        Ok(())
    }

    // ---- approvals ----

    pub fn request_approval(
        &self,
        request: NewApproval,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let task = self.task(&request.task_id)?;
        let place = Place::of(task.distro.as_deref());
        let planned = request.rung == DeliveryRung::Commit && !request.commits.is_empty();
        if request.title.trim().is_empty() && !planned {
            return Err(DeliveryError::new(
                "invalid_approval",
                "la aprobación necesita el texto exacto (título o mensaje)",
            ));
        }
        {
            let store = self.store()?;
            let open = store.approvals()?.into_iter().any(|approval| {
                approval.task_id == task.id
                    && approval.rung == request.rung
                    && matches!(
                        approval.status,
                        DeliveryApprovalStatus::Pending | DeliveryApprovalStatus::Approved
                    )
            });
            if open {
                return Err(DeliveryError::new(
                    "approval_exists",
                    "ya hay una aprobación abierta para este paso",
                ));
            }
        }
        rung_prerequisites(&task, request.rung)?;
        let mut title = request.title.trim().to_string();
        let mut body = request.body;
        let mut commits = Vec::new();
        let mut left_out = Vec::new();
        match request.rung {
            DeliveryRung::Commit => {
                let changed = tasks::changed_files(place, &task.worktree)?;
                commits = if planned {
                    request
                        .commits
                        .into_iter()
                        .map(|commit| DeliveryCommit {
                            message: commit.message.trim().to_string(),
                            paths: commit.paths.iter().map(|path| plan_path(path)).collect(),
                        })
                        .collect()
                } else {
                    vec![DeliveryCommit {
                        message: match body.trim() {
                            "" => title.clone(),
                            body => format!("{title}\n\n{body}"),
                        },
                        paths: changed.clone(),
                    }]
                };
                left_out = request
                    .leave_out
                    .iter()
                    .map(|path| plan_path(path))
                    .collect();
                check_plan(&changed, &commits, &left_out)?;
                // The plan is the exact text; the title only names the step.
                title = match commits.as_slice() {
                    [only] => subject(&only.message).to_string(),
                    many => format!("{} commits", many.len()),
                };
                body = String::new();
            }
            DeliveryRung::Push if self.commits_lost(&task)? => {
                return Err(DeliveryError::new(
                    "commit_missing",
                    "los commits que hizo Tinto ya no están en la rama; hay que volver a preparar el commit",
                ));
            }
            _ => {}
        }
        let approval = DeliveryApproval {
            commits,
            left_out,
            created: Vec::new(),
            before: None,
            id: uuid::Uuid::new_v4().to_string(),
            task_id: task.id.clone(),
            rung: request.rung,
            title,
            body,
            status: DeliveryApprovalStatus::Pending,
            requested_by: request.requested_by,
            requested_at_ms: now_ms(),
            decided_at_ms: None,
            executed_at_ms: None,
            outcome: None,
        };
        let store = self.store()?;
        store.put_approval(&approval)?;
        store.record_event(
            approval.requested_at_ms,
            "approval_requested",
            Some(&task.id),
            None,
            &format!("{:?}", approval.rung).to_ascii_lowercase(),
        )?;
        drop(store);
        self.notify();
        Ok(approval)
    }

    pub fn approval(&self, approval_id: &str) -> Result<DeliveryApproval, DeliveryError> {
        self.store()?
            .approval(approval_id)?
            .ok_or_else(|| DeliveryError::not_found("la aprobación", approval_id))
    }

    /// The user's decision. An approved commit or push runs right away.
    pub fn decide_approval(
        &self,
        approval_id: &str,
        approve: bool,
        note: Option<String>,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        if approval.status != DeliveryApprovalStatus::Pending {
            return Err(DeliveryError::new(
                "approval_decided",
                "esta aprobación ya tiene una decisión",
            ));
        }
        approval.decided_at_ms = Some(now_ms());
        if !approve {
            approval.status = DeliveryApprovalStatus::Rejected;
            approval.outcome = note.filter(|note| !note.trim().is_empty());
            self.save_approval(&approval, "approval_rejected")?;
            return Ok(approval);
        }
        approval.status = DeliveryApprovalStatus::Approved;
        if matches!(approval.rung, DeliveryRung::Commit | DeliveryRung::Push) {
            let task = self.task(&approval.task_id)?;
            let place = Place::of(task.distro.as_deref());
            let executed = match approval.rung {
                DeliveryRung::Commit => run_commit_plan(&task, &mut approval),
                _ if self.commits_lost(&task)? => Err(DeliveryError::new(
                    "commit_missing",
                    "No se publicó: los commits que hizo Tinto ya no están en la rama. Vuelve a preparar el commit.",
                )),
                _ => tasks::push_branch(place, &task.worktree, &task.branch)
                    .map(|upstream| format!("Publicado en {upstream}")),
            };
            approval.executed_at_ms = Some(now_ms());
            match executed {
                Ok(outcome) => {
                    approval.status = DeliveryApprovalStatus::Executed;
                    approval.outcome = Some(outcome);
                }
                Err(error) => {
                    approval.status = DeliveryApprovalStatus::Failed;
                    approval.outcome = Some(error.message);
                }
            }
        }
        self.save_approval(&approval, "approval_approved")?;
        Ok(approval)
    }

    /// The coordinator (or the user) reports how an approved PR or Jira step
    /// went.
    pub fn complete_approval(
        &self,
        approval_id: &str,
        success: bool,
        outcome: &str,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        if approval.status != DeliveryApprovalStatus::Approved {
            return Err(DeliveryError::new(
                "approval_not_approved",
                "solo se informa el resultado de un paso aprobado",
            ));
        }
        approval.status = if success {
            DeliveryApprovalStatus::Executed
        } else {
            DeliveryApprovalStatus::Failed
        };
        approval.executed_at_ms = Some(now_ms());
        approval.outcome = Some(outcome.trim().to_string()).filter(|text| !text.is_empty());
        self.save_approval(&approval, "approval_completed")?;
        Ok(approval)
    }

    /// What the form to prepare a task's commit starts from.
    pub fn commit_draft(&self, task_id: &str) -> Result<DeliveryCommitDraft, DeliveryError> {
        let task = self.task(task_id)?;
        let place = Place::of(task.distro.as_deref());
        Ok(DeliveryCommitDraft {
            files: tasks::changed_files(place, &task.worktree)?,
            recent: tasks::recent_subjects(place, &task.worktree, 8),
        })
    }

    /// Takes back the commits of a commit step that was not pushed: the
    /// branch returns to where it was and the changes stay in the worktree.
    pub fn undo_commit(&self, approval_id: &str) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        let created = created_commits(&approval);
        let undoable = approval.rung == DeliveryRung::Commit
            && matches!(
                approval.status,
                DeliveryApprovalStatus::Executed | DeliveryApprovalStatus::Failed
            );
        let (true, Some(last)) = (undoable, created.last()) else {
            return Err(DeliveryError::new(
                "undo_unavailable",
                "este paso no tiene commits de Tinto que deshacer",
            ));
        };
        let task = self.task(&approval.task_id)?;
        let place = Place::of(task.distro.as_deref());
        let tip = tasks::git(place, &task.worktree, &["rev-parse", last]).unwrap_or_default();
        if tip.is_empty() || tasks::head(place, &task.worktree)? != tip {
            return Err(DeliveryError::new(
                "commit_moved",
                "la rama ya no termina en los commits que hizo Tinto (hay commits posteriores o se reescribió); no se puede deshacer desde aquí",
            ));
        }
        if tasks::published(place, &task.worktree, &tip) {
            return Err(DeliveryError::new(
                "commit_published",
                "los commits ya están publicados; no se deshacen desde Tinto",
            ));
        }
        let before = approval
            .before
            .clone()
            .unwrap_or_else(|| format!("{}^", created[0]));
        tasks::uncommit_to(place, &task.worktree, &before)?;
        approval.status = DeliveryApprovalStatus::Undone;
        approval.outcome = Some(format!(
            "Deshecho: los cambios de {} volvieron al worktree, sin confirmar.",
            match created.len() {
                1 => "1 commit".to_string(),
                count => format!("{count} commits"),
            }
        ));
        self.save_approval(&approval, "commit_undone")?;
        Ok(approval)
    }

    /// Whether the commits of the task's commit step are gone from its
    /// branch (reset, amend or rebase done outside Tinto). The step then
    /// counts as undone, so the commit is prepared again before any push.
    pub(super) fn commits_lost(&self, task: &DeliveryTask) -> Result<bool, DeliveryError> {
        let latest = self
            .store()?
            .approvals()?
            .into_iter()
            .rev()
            .find(|approval| approval.task_id == task.id && approval.rung == DeliveryRung::Commit);
        let Some(mut approval) = latest else {
            return Ok(false);
        };
        let created = created_commits(&approval);
        if approval.status != DeliveryApprovalStatus::Executed || created.is_empty() {
            return Ok(false);
        }
        let place = Place::of(task.distro.as_deref());
        // A worktree git cannot read says nothing about the commits.
        if tasks::head(place, &task.worktree).is_err()
            || created
                .iter()
                .all(|commit| tasks::on_branch(place, &task.worktree, commit))
        {
            return Ok(false);
        }
        approval.status = DeliveryApprovalStatus::Undone;
        approval.outcome = Some(format!(
            "Los commits que hizo Tinto ({}) ya no están en la rama: se deshicieron o reescribieron fuera de Tinto (reset, amend o rebase). Hay que volver a preparar el commit.",
            created
                .iter()
                .map(|commit| short(commit))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        self.save_approval(&approval, "commit_missing")?;
        Ok(true)
    }

    /// Runs [`Self::commits_lost`] on every task that has a commit step done
    /// and its push still ahead, so the view never offers to push commits
    /// that are gone.
    pub(super) fn reconcile_commits(&self) -> Result<(), DeliveryError> {
        let (tasks, approvals) = {
            let store = self.store()?;
            (store.tasks()?, store.approvals()?)
        };
        for task in tasks {
            let latest = |rung: DeliveryRung| {
                approvals
                    .iter()
                    .rev()
                    .find(|approval| approval.task_id == task.id && approval.rung == rung)
            };
            let committed = latest(DeliveryRung::Commit)
                .is_some_and(|approval| approval.status == DeliveryApprovalStatus::Executed);
            let pushed = latest(DeliveryRung::Push)
                .is_some_and(|approval| approval.status == DeliveryApprovalStatus::Executed);
            if committed && !pushed {
                self.commits_lost(&task)?;
            }
        }
        Ok(())
    }

    fn save_approval(&self, approval: &DeliveryApproval, event: &str) -> Result<(), DeliveryError> {
        let store = self.store()?;
        store.put_approval(approval)?;
        store.record_event(
            now_ms(),
            event,
            Some(&approval.task_id),
            None,
            approval.outcome.as_deref().unwrap_or_default(),
        )?;
        drop(store);
        self.notify();
        Ok(())
    }
}

/// Creates the plan's commits in order, with their exact messages, and
/// stops at the first that fails. Records what it created so the step can
/// be undone, and says in the outcome how the branch was left.
fn run_commit_plan(
    task: &DeliveryTask,
    approval: &mut DeliveryApproval,
) -> Result<String, DeliveryError> {
    let place = Place::of(task.distro.as_deref());
    let changed = tasks::changed_files(place, &task.worktree)?;
    // A request from before plans existed: one commit with everything.
    if approval.commits.is_empty() {
        approval.commits = vec![DeliveryCommit {
            message: match approval.body.trim() {
                "" => approval.title.clone(),
                body => format!("{}\n\n{body}", approval.title),
            },
            paths: changed.clone(),
        }];
    }
    check_plan(&changed, &approval.commits, &approval.left_out).map_err(|error| {
        invalid_plan(format!(
            "No se creó ningún commit: el worktree cambió desde que se preparó el plan ({}). Prepáralo de nuevo.",
            error.message
        ))
    })?;
    approval.before = Some(tasks::head(place, &task.worktree)?);
    tasks::unstage_all(place, &task.worktree)?;
    let total = approval.commits.len();
    for (index, commit) in approval.commits.iter().enumerate() {
        match tasks::commit_paths(place, &task.worktree, &commit.message, &commit.paths) {
            Ok(created) => approval.created.push(created),
            Err(error) => {
                let _ = tasks::unstage_all(place, &task.worktree);
                let done = match approval.created.len() {
                    0 => "No se creó ningún commit".to_string(),
                    count => format!(
                        "Los {count} anteriores quedaron hechos ({})",
                        shorts(&approval.created)
                    ),
                };
                return Err(DeliveryError::new(
                    "commit_failed",
                    format!(
                        "El commit {} de {total} («{}») falló: {}. {done}; el resto de los cambios sigue en el worktree, sin confirmar.",
                        index + 1,
                        subject(&commit.message),
                        error.message
                    ),
                ));
            }
        }
    }
    let unexpected: Vec<String> = tasks::changed_files(place, &task.worktree)?
        .into_iter()
        .filter(|path| !approval.left_out.contains(path))
        .collect();
    let mut outcome = match total {
        1 => format!("Commit {}", shorts(&approval.created)),
        _ => format!("{total} commits ({})", shorts(&approval.created)),
    };
    if !unexpected.is_empty() {
        outcome.push_str(&format!(
            ". Quedan cambios sin confirmar que el plan no dejaba fuera: {}",
            unexpected.join(", ")
        ));
    }
    Ok(outcome)
}

fn shorts(commits: &[String]) -> String {
    commits
        .iter()
        .map(|commit| short(commit))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Each rung needs the git state the previous one leaves: something to
/// commit, commits to push, a published branch for the PR.
fn rung_prerequisites(task: &DeliveryTask, rung: DeliveryRung) -> Result<(), DeliveryError> {
    let place = Place::of(task.distro.as_deref());
    match rung {
        DeliveryRung::Commit => {
            let state = tasks::worktree_state(place, &task.worktree, &task.base_commit)?;
            if !state.dirty {
                return Err(DeliveryError::new(
                    "nothing_to_commit",
                    "no hay cambios que confirmar",
                ));
            }
        }
        DeliveryRung::Push => {
            let state = tasks::worktree_state(place, &task.worktree, &task.base_commit)?;
            if state.unpublished == 0 {
                return Err(DeliveryError::new(
                    "nothing_to_push",
                    "no hay commits por publicar",
                ));
            }
        }
        DeliveryRung::Pr => {
            let state = tasks::worktree_state(place, &task.worktree, &task.base_commit)?;
            if !state.has_upstream {
                return Err(DeliveryError::new(
                    "not_pushed",
                    "publica la rama antes de pedir el PR",
                ));
            }
        }
        DeliveryRung::Jira => {}
    }
    Ok(())
}
