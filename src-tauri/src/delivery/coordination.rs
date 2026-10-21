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
//!   they are no longer on the branch. While a merge is in progress the
//!   plan is one commit of the whole merge, which keeps both parents.
//! - The sync rung, asked when needed, merges the task's base into its
//!   branch (jobs cannot run git merge). Conflicts leave the merge in
//!   progress for an implementation job, and a commit step concludes it.
//! - Once the coordinator ends a run's brainstorm, its commit, sync and push
//!   rungs are approved as they are asked; the PR and Jira rungs still wait.

use super::model::{
    DeliveryApproval, DeliveryApprovalStatus, DeliveryCommit, DeliveryDecisionStatus,
    DeliveryMergeConflicts, DeliveryRun, DeliveryRung, DeliverySettingsProposalStatus,
    DeliveryTask, SINGLE_COMMIT_MAX_FILES, SINGLE_COMMIT_MAX_LINES,
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
    /// `merge` only: `squash`, `merge` or `rebase`.
    pub merge_method: Option<String>,
    /// `jira` only: `after_pr` or `after_merge`; the next one missing when
    /// `None`.
    pub stage: Option<String>,
    /// `jira` only: the statuses to walk the issue through; the repo's
    /// defaults for the stage when empty.
    pub jira_statuses: Vec<String>,
    /// `commit` only: changed files the plan leaves uncommitted on purpose.
    pub leave_out: Vec<String>,
    /// `commit` only: why a single commit is right when the change spans
    /// several areas or is large; a coordinator's plan needs it then.
    pub single_commit_reason: Option<String>,
}

/// Where a changed file belongs for splitting commits: `docs`, `tests` or
/// `code`.
fn change_area(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let in_dir = |dirs: &[&str]| lower.split('/').any(|part| dirs.contains(&part));
    if in_dir(&["docs", "doc"])
        || [".md", ".mdx", ".rst", ".adoc"]
            .iter()
            .any(|ext| name.ends_with(ext))
    {
        "docs"
    } else if in_dir(&["test", "tests", "__tests__", "spec", "e2e"])
        || [".test.", ".spec.", "_test."]
            .iter()
            .any(|part| name.contains(part))
        || name.starts_with("test_")
    {
        "tests"
    } else {
        "code"
    }
}

/// A plan of one commit is the exception: when it mixes areas or is larger
/// than the repo allows, it has to say why.
fn check_single_commit(
    paths: &[String],
    lines: u64,
    max_files: u32,
    max_lines: u32,
    reason: Option<&str>,
) -> Result<(), DeliveryError> {
    if reason.is_some_and(|reason| !reason.trim().is_empty()) {
        return Ok(());
    }
    let mut areas: Vec<&str> = paths.iter().map(|path| change_area(path)).collect();
    areas.sort_unstable();
    areas.dedup();
    let mut why = Vec::new();
    if areas.len() > 1 {
        why.push(format!("mezcla {}", areas.join(", ")));
    }
    if paths.len() > max_files as usize {
        why.push(format!(
            "tiene {} archivos (más de {max_files})",
            paths.len()
        ));
    }
    if lines > u64::from(max_lines) {
        why.push(format!("cambia {lines} líneas (más de {max_lines})"));
    }
    if why.is_empty() {
        return Ok(());
    }
    Err(DeliveryError::new(
        "single_commit_unjustified",
        format!(
            "un único commit que {} necesita single_commit_reason: la norma es dividir el cambio en commits atómicos, cada uno con su código y sus tests; si debe ir junto, di por qué",
            why.join(" y ")
        ),
    ))
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

/// The one commit that concludes a merge in progress. Its paths are not
/// asked: it takes every change (a rename would need both of its sides),
/// and they are recorded to notice if the worktree changes before it runs.
fn merge_commit(
    place: Place<'_>,
    worktree: &std::path::Path,
    request: &NewApproval,
    title: &str,
    body: &str,
) -> Result<DeliveryCommit, DeliveryError> {
    let message = match request.commits.as_slice() {
        [] => match body.trim() {
            "" => title.to_string(),
            body => format!("{title}\n\n{body}"),
        },
        [only] => only.message.trim().to_string(),
        _ => {
            return Err(invalid_plan(
                "hay un merge en curso: se confirma entero, en un único commit de merge",
            ))
        }
    };
    if message.is_empty() {
        return Err(invalid_plan("el commit de merge no tiene mensaje"));
    }
    if !request.leave_out.is_empty() {
        return Err(invalid_plan(
            "hay un merge en curso: el commit de merge lo incluye todo y no deja archivos fuera",
        ));
    }
    check_conflicts_resolved(place, worktree)?;
    Ok(DeliveryCommit {
        message,
        paths: tasks::changed_files(place, worktree)?,
    })
}

fn check_conflicts_resolved(
    place: Place<'_>,
    worktree: &std::path::Path,
) -> Result<(), DeliveryError> {
    let unresolved = tasks::unresolved_conflicts(place, worktree);
    if unresolved.is_empty() {
        return Ok(());
    }
    Err(DeliveryError::new(
        "merge_conflicts",
        format!(
            "el merge aún tiene conflictos sin resolver en: {}",
            unresolved.join(", ")
        ),
    ))
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

/// The Jira step when the PR is open (the issue goes to review).
pub const JIRA_AFTER_PR: &str = "after_pr";
/// The Jira step once the PR is merged (the issue is closed).
pub const JIRA_AFTER_MERGE: &str = "after_merge";

/// A step done outside Tinto that Tinto found on GitHub.
pub(super) fn found_step(
    task_id: &str,
    rung: DeliveryRung,
    title: String,
    outcome: String,
) -> DeliveryApproval {
    let now = now_ms();
    DeliveryApproval {
        id: uuid::Uuid::new_v4().to_string(),
        task_id: task_id.to_string(),
        rung,
        title,
        body: String::new(),
        status: DeliveryApprovalStatus::Executed,
        requested_by: "github".to_string(),
        requested_at_ms: now,
        decided_at_ms: None,
        executed_at_ms: Some(now),
        outcome: Some(outcome),
        merge_method: None,
        stage: None,
        jira_statuses: Vec::new(),
        jira_link_only: false,
        reviewers: Vec::new(),
        reviewers_requested: false,
        commits: Vec::new(),
        left_out: Vec::new(),
        single_commit_reason: None,
        merge_head: None,
        conflicts: Vec::new(),
        created: Vec::new(),
        before: None,
        tip: None,
        notice: None,
        corrections: Vec::new(),
    }
}

impl DeliveryService {
    /// The task's latest step of `rung`, whatever its status.
    pub(super) fn latest_approval(
        &self,
        task_id: &str,
        rung: DeliveryRung,
    ) -> Result<Option<DeliveryApproval>, DeliveryError> {
        Ok(self
            .store()?
            .approvals()?
            .into_iter()
            .rev()
            .find(|approval| approval.task_id == task_id && approval.rung == rung))
    }

    /// The Jira step a request is for: the one asked, else the next one the
    /// task has not done (`after_pr`, then `after_merge`). A middle package
    /// has no `after_merge`: the issue closes with its last package.
    fn jira_stage(
        &self,
        task: &DeliveryTask,
        asked: Option<&str>,
    ) -> Result<String, DeliveryError> {
        let stage = match asked {
            Some(JIRA_AFTER_PR) | Some(JIRA_AFTER_MERGE) => asked.unwrap_or_default(),
            Some(other) => {
                return Err(DeliveryError::new(
                    "invalid_approval",
                    format!("momento de Jira desconocido: {other} (after_pr o after_merge)"),
                ))
            }
            None if self.jira_done(&task.id)? => JIRA_AFTER_MERGE,
            None => JIRA_AFTER_PR,
        };
        if let Some(package) = task
            .package
            .as_ref()
            .filter(|package| stage == JIRA_AFTER_MERGE && !package.is_last())
        {
            return Err(DeliveryError::new(
                "invalid_approval",
                format!(
                    "{} es un paquete intermedio: no cierra el issue, lo cierra el paquete {}/{} cuando se fusionen todos",
                    task.label(),
                    package.of,
                    package.of
                ),
            ));
        }
        Ok(stage.to_string())
    }

    /// Whether the task's Jira step after its PR was done.
    fn jira_done(&self, task_id: &str) -> Result<bool, DeliveryError> {
        Ok(self.store()?.approvals()?.iter().any(|approval| {
            approval.task_id == task_id
                && approval.rung == DeliveryRung::Jira
                && approval.stage.as_deref() == Some(JIRA_AFTER_PR)
                && approval.status == DeliveryApprovalStatus::Executed
        }))
    }

    /// The other packages of the task's issue, archived ones included.
    pub(super) fn packages_of(
        &self,
        task: &DeliveryTask,
    ) -> Result<Vec<DeliveryTask>, DeliveryError> {
        let store = self.store()?;
        let archived = store
            .removed_tasks()?
            .into_iter()
            .filter(|other| other.archived_at_ms.is_some());
        Ok(store
            .tasks()?
            .into_iter()
            .chain(archived)
            .filter(|other| task.is_sibling(other))
            .collect())
    }

    /// Whether the task's PR was merged: by its merge step, on GitHub, or
    /// archived once delivered.
    pub(super) fn task_merged(&self, task: &DeliveryTask) -> Result<bool, DeliveryError> {
        if task.archived_at_ms.is_some() {
            return Ok(true);
        }
        let merge_done = self
            .latest_approval(&task.id, DeliveryRung::Merge)?
            .is_some_and(|merge| merge.status == DeliveryApprovalStatus::Executed);
        Ok(merge_done
            || self
                .task_pr(&task.id)?
                .is_some_and(|pr| pr.state == "merged"))
    }

    /// Jira steps from when a task had a single one get their moment: done
    /// before the merge, it was the PR's (AGOS-455 moved the issue to review
    /// before merge existed); after it, the merge's.
    pub(super) fn stage_legacy_jira(&self) -> Result<(), DeliveryError> {
        let approvals = self.store()?.approvals()?;
        for mut jira in approvals.iter().cloned() {
            if jira.rung != DeliveryRung::Jira || jira.stage.is_some() {
                continue;
            }
            let at = jira.executed_at_ms.unwrap_or(jira.requested_at_ms);
            let merged_before = approvals.iter().any(|merge| {
                merge.task_id == jira.task_id
                    && merge.rung == DeliveryRung::Merge
                    && merge.status == DeliveryApprovalStatus::Executed
                    && merge.executed_at_ms.is_some_and(|merged| merged <= at)
            });
            jira.stage = Some(
                if merged_before {
                    JIRA_AFTER_MERGE
                } else {
                    JIRA_AFTER_PR
                }
                .to_string(),
            );
            self.store()?.put_approval(&jira)?;
        }
        Ok(())
    }

    /// Every step of the task's ladder done: commit, push, PR, merge and its
    /// Jira steps.
    pub(super) fn delivered(&self, task_id: &str) -> Result<bool, DeliveryError> {
        let task = self.task(task_id)?;
        let approvals: Vec<DeliveryApproval> = self
            .store()?
            .approvals()?
            .into_iter()
            .filter(|approval| approval.task_id == task_id)
            .collect();
        let approvals: Vec<&DeliveryApproval> = approvals.iter().collect();
        Ok(super::phase::is_delivered(&task, &approvals))
    }

    /// Archives a delivered task: its worktree and local branch go (what it
    /// produced is on GitHub) and it leaves the list. Its record stays, so
    /// it can be found by issue or PR and brought back.
    pub fn archive_task(&self, task_id: &str) -> Result<(), DeliveryError> {
        self.task(task_id)?;
        if !self.delivered(task_id)? {
            return Err(DeliveryError::new(
                "task_not_delivered",
                "solo se archiva una tarea entregada: con el merge y sus pasos de Jira hechos",
            ));
        }
        self.remove_task(task_id, true)?;
        let store = self.store()?;
        let mut task = store
            .task(task_id)?
            .ok_or_else(|| DeliveryError::not_found("la tarea", task_id))?;
        let now = now_ms();
        task.archived_at_ms = Some(now);
        task.updated_at_ms = now;
        store.put_task(&task)?;
        store.record_event(now, "task_archived", Some(task_id), None, &task.key)?;
        drop(store);
        self.notify();
        Ok(())
    }

    /// Archived tasks whose issue key contains `query`, or whose PR number
    /// or URL is `query` (`#279` or `279`); all of them when it is empty.
    /// Newest first.
    pub fn archived_tasks(
        &self,
        query: &str,
    ) -> Result<Vec<super::model::DeliveryArchivedTask>, DeliveryError> {
        let query = query.trim().trim_start_matches('#').to_ascii_lowercase();
        let store = self.store()?;
        let mut found = Vec::new();
        for task in store.removed_tasks()?.into_iter().rev() {
            if task.archived_at_ms.is_none() {
                continue;
            }
            let pr = store.pr(&task.id)?;
            let matches = query.is_empty()
                || task.key.to_ascii_lowercase().contains(&query)
                || pr.as_ref().is_some_and(|pr| {
                    pr.number.to_string() == query || pr.url.to_ascii_lowercase() == query
                });
            if matches {
                found.push(super::model::DeliveryArchivedTask {
                    pr_number: pr.as_ref().map(|pr| pr.number),
                    pr_url: pr.map(|pr| pr.url),
                    task,
                });
            }
        }
        Ok(found)
    }

    /// Brings an archived task back with its worktree: from its local
    /// branch, else from its PR on GitHub (`pull/<n>/head`, kept after the
    /// branch is deleted), else from the remote branch.
    pub fn unarchive_task(&self, task_id: &str) -> Result<DeliveryTask, DeliveryError> {
        let mut task = self
            .store()?
            .task(task_id)?
            .filter(|task| task.archived_at_ms.is_some())
            .ok_or_else(|| DeliveryError::not_found("la tarea archivada", task_id))?;
        // Another package of the issue may be active; the same one may not.
        let active = self.store()?.tasks()?.into_iter().any(|other| {
            other.key == task.key
                && tasks::plain_path(&other.repo) == tasks::plain_path(&task.repo)
                && !task.is_sibling(&other)
        });
        if active {
            return Err(DeliveryError::new(
                "task_exists",
                format!("ya hay una tarea {} activa en el repositorio", task.label()),
            ));
        }
        let place = Place::of(task.distro.as_deref());
        let reference = format!("refs/heads/{}", task.branch);
        let local = tasks::git(
            place,
            &task.repo,
            &["rev-parse", "--verify", "--quiet", &reference],
        );
        if local.is_err() {
            let from = match self.task_pr(&task.id)? {
                Some(pr) => format!("pull/{}/head", pr.number),
                None => reference.clone(),
            };
            let spec = format!("{from}:{reference}");
            tasks::git(place, &task.repo, &["fetch", "origin", &spec]).map_err(|error| {
                DeliveryError::new(
                    "unarchive_failed",
                    format!(
                        "no se pudo traer la rama {} de GitHub ({from}): {}",
                        task.branch, error.message
                    ),
                )
            })?;
        }
        tasks::create_worktree(
            place,
            &task.repo,
            &task.worktree,
            &task.branch,
            &task.base_commit,
        )?;
        let now = now_ms();
        task.removed_at_ms = None;
        task.archived_at_ms = None;
        task.updated_at_ms = now;
        let store = self.store()?;
        store.put_task(&task)?;
        store.record_event(now, "task_unarchived", Some(&task.id), None, &task.key)?;
        drop(store);
        self.notify();
        Ok(task)
    }

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
            unattended: false,
            paused: false,
            no_new_tasks: false,
            coordinator_session: None,
            coordinator_cursor: 0,
            notified_cursor: 0,
            notified_at_ms: None,
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
                    "la ejecución la coordina {current} (generación {}); si el usuario te pidió coordinarla, o ese coordinador se detuvo, tómala con takeover_run",
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

    /// The user pauses or resumes the batch: while paused its queued jobs
    /// wait; the running ones go on, and each can be paused on its own.
    pub fn set_run_paused(&self, run_id: &str, paused: bool) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.run(run_id)?;
        run.paused = paused;
        run.updated_at_ms = now_ms();
        self.save_run(
            &run,
            "run_policy",
            if paused { "paused" } else { "resumed" },
        )?;
        self.tick();
        Ok(run)
    }

    /// The user stops or allows new tasks in the batch; the coordinator hears
    /// it in wait_run, and create_task refuses while it is stopped.
    pub fn set_run_no_new_tasks(
        &self,
        run_id: &str,
        no_new_tasks: bool,
    ) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.run(run_id)?;
        run.no_new_tasks = no_new_tasks;
        run.updated_at_ms = now_ms();
        self.save_run(
            &run,
            "run_policy",
            if no_new_tasks {
                "no_new_tasks"
            } else {
                "new_tasks_allowed"
            },
        )?;
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

    /// The coordinator ends the batch's brainstorm once the user has
    /// answered everything it asked, so nothing is left for the stages to
    /// wait on. From then on the batch runs without stopping until a PR.
    pub fn end_brainstorm(&self, run_id: &str) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.active_run(run_id)?;
        let tasks = self.store()?.tasks()?;
        for task in tasks
            .iter()
            .filter(|task| task.run_id.as_deref() == Some(run_id))
        {
            if self
                .task_decisions(&task.id)?
                .iter()
                .any(|decision| decision.status == DeliveryDecisionStatus::Pending)
            {
                return Err(DeliveryError::new(
                    "brainstorm_open",
                    format!(
                        "el brainstorm sigue: {} tiene decisiones sin responder",
                        task.key
                    ),
                ));
            }
        }
        if self
            .repo_settings_proposals(&run.repo)?
            .iter()
            .any(|proposal| {
                proposal.run_id == run_id
                    && proposal.status == DeliverySettingsProposalStatus::Pending
            })
        {
            return Err(DeliveryError::new(
                "brainstorm_open",
                "el brainstorm sigue: hay una propuesta de ajustes sin decidir",
            ));
        }
        run.unattended = true;
        run.updated_at_ms = now_ms();
        self.save_run(&run, "run_policy", "unattended")?;
        Ok(run)
    }

    /// The batch's tasks whose phase waits on the coordinator: a stage to
    /// run or retry, a delivery step to ask for or record. Each comes with
    /// its phase and the stage due, as the view shows them.
    pub fn idle_tasks(&self, run_id: &str) -> Result<Vec<serde_json::Value>, DeliveryError> {
        let overview = self.overview()?;
        Ok(overview
            .tasks
            .iter()
            .filter(|task| task.run_id.as_deref() == Some(run_id))
            .filter_map(|task| {
                let phase = overview.phases.get(&task.id)?;
                (phase.waits_on == super::phase::Waiter::Coordinator).then(|| {
                    serde_json::json!({
                        "task_id": task.id,
                        "key": task.label(),
                        "state": task.state,
                        "phase": phase,
                    })
                })
            })
            .collect())
    }

    /// How many more jobs can run now.
    pub fn spare_capacity(&self) -> Result<usize, DeliveryError> {
        let store = self.store()?;
        let capacity = store.settings()?.effective_capacity(store.tasks()?.len()) as usize;
        let running = store
            .jobs()?
            .iter()
            .filter(|job| job.status.is_open())
            .count();
        Ok(capacity.saturating_sub(running))
    }

    /// Whether the task's batch is past its brainstorm.
    pub(super) fn unattended(&self, task: &DeliveryTask) -> Result<bool, DeliveryError> {
        Ok(match task.run_id.as_deref() {
            Some(run_id) => self.run(run_id)?.unattended,
            None => false,
        })
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
                    "la ejecución está en la generación {} coordinada por {}; esta escritura se descarta. Si el usuario no te pidió volver a coordinarla, deja este lote",
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
        let stage = match request.rung {
            DeliveryRung::Jira => Some(self.jira_stage(&task, request.stage.as_deref())?),
            _ => None,
        };
        // A split issue moves in Jira once: the first package to open its
        // PR moves it, the others only link their PR, and the last one
        // closes it once every package has merged.
        let mut link_only = false;
        if request.rung == DeliveryRung::Jira && task.package.is_some() {
            let packages = self.packages_of(&task)?;
            if stage.as_deref() == Some(JIRA_AFTER_MERGE) {
                let of = task.package.as_ref().map_or(0, |package| package.of);
                let mut missing = Vec::new();
                for index in 1..of {
                    let merged = match packages
                        .iter()
                        .find(|other| other.package.as_ref().is_some_and(|p| p.index == index))
                    {
                        Some(other) => self.task_merged(other)?,
                        None => false,
                    };
                    if !merged {
                        missing.push(format!("{index}/{of}"));
                    }
                }
                if !missing.is_empty() {
                    return Err(DeliveryError::new(
                        "packages_unmerged",
                        format!(
                            "{} se cierra en Jira cuando se fusionen todos sus paquetes: falta {}",
                            task.key,
                            missing.join(", ")
                        ),
                    ));
                }
            } else {
                for other in &packages {
                    link_only |= self.jira_done(&other.id)?;
                }
            }
        }
        // With the repo's Jira set in Tinto, Tinto runs the step: the
        // statuses asked, else the repo's for this moment.
        let mut jira_statuses: Vec<String> = Vec::new();
        if request.rung == DeliveryRung::Jira && !link_only && self.jira_site(&task.repo)?.is_some()
        {
            jira_statuses = request
                .jira_statuses
                .iter()
                .map(|status| status.trim().to_string())
                .filter(|status| !status.is_empty())
                .collect();
            if jira_statuses.is_empty() {
                let jira = self
                    .store()?
                    .repo_settings(&task.repo)?
                    .jira
                    .unwrap_or_default();
                jira_statuses = match stage.as_deref() {
                    Some(JIRA_AFTER_MERGE) => jira.after_merge,
                    _ => jira.after_pr,
                };
            }
            if jira_statuses.is_empty() {
                return Err(DeliveryError::new(
                    "invalid_approval",
                    "di a qué estados pasa la issue (o fíjalos en Ajustes → Jira del repositorio)",
                ));
            }
        }
        {
            let store = self.store()?;
            let open = store.approvals()?.into_iter().any(|approval| {
                approval.task_id == task.id
                    && approval.rung == request.rung
                    && approval.stage == stage
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
        // A push that no longer matches the remote can be asked again with
        // nothing new to publish: publishing again is how it is put right.
        let republish = request.rung == DeliveryRung::Push
            && self
                .latest_approval(&task.id, DeliveryRung::Push)?
                .is_some_and(|push| push.status == DeliveryApprovalStatus::Drifted);
        if !republish {
            rung_prerequisites(&task, request.rung)?;
        }
        let mut title = request.title.trim().to_string();
        let mut body = request.body.clone();
        let mut commits = Vec::new();
        let mut left_out = Vec::new();
        let mut single_commit_reason = None;
        let merge_head = match request.rung {
            DeliveryRung::Commit => tasks::merge_head(place, &task.worktree),
            _ => None,
        };
        match request.rung {
            // A merge in progress is concluded whole, so the commit keeps the
            // merged branch as its second parent.
            DeliveryRung::Commit if merge_head.is_some() => {
                let commit = merge_commit(place, &task.worktree, &request, &title, &body)?;
                title = subject(&commit.message).to_string();
                body = String::new();
                commits = vec![commit];
            }
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
                // The user who prepares one commit knows what it holds; a
                // coordinator justifies it.
                if let ([only], false) = (commits.as_slice(), request.requested_by == "user") {
                    let settings = self.store()?.repo_settings(&task.repo)?;
                    check_single_commit(
                        &only.paths,
                        tasks::changed_lines(place, &task.worktree, &only.paths),
                        settings
                            .single_commit_max_files
                            .unwrap_or(SINGLE_COMMIT_MAX_FILES),
                        settings
                            .single_commit_max_lines
                            .unwrap_or(SINGLE_COMMIT_MAX_LINES),
                        request.single_commit_reason.as_deref(),
                    )?;
                    single_commit_reason = request
                        .single_commit_reason
                        .as_deref()
                        .map(str::trim)
                        .filter(|reason| !reason.is_empty())
                        .map(str::to_string);
                }
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
            DeliveryRung::Push => self.check_base_merged(&task)?,
            DeliveryRung::Sync => {
                self.sync_prerequisites(&task)?;
            }
            DeliveryRung::Merge => {
                self.merge_prerequisites(&task, request.merge_method.as_deref())?
            }
            _ => {}
        }
        let approval = DeliveryApproval {
            merge_method: request
                .merge_method
                .filter(|_| request.rung == DeliveryRung::Merge),
            stage,
            jira_statuses,
            jira_link_only: link_only,
            reviewers: Vec::new(),
            reviewers_requested: false,
            commits,
            left_out,
            single_commit_reason,
            merge_head,
            conflicts: Vec::new(),
            created: Vec::new(),
            before: None,
            tip: None,
            notice: None,
            corrections: Vec::new(),
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
        // Past the brainstorm the branch is committed and published without
        // stopping; opening the PR still waits for the user.
        if matches!(
            approval.rung,
            DeliveryRung::Commit | DeliveryRung::Sync | DeliveryRung::Push
        ) && self.unattended(&task)?
        {
            return self.decide_approval(&approval.id, true, None);
        }
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
        let tinto_runs_jira = !approval.jira_statuses.is_empty()
            || (approval.jira_link_only
                && self
                    .jira_site(&self.task(&approval.task_id)?.repo)?
                    .is_some());
        if approval.rung == DeliveryRung::Jira && tinto_runs_jira {
            let executed = self.run_jira_step(&approval);
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
        // Tinto opens the PR with the approved text. When it cannot reach
        // GitHub the step stays approved for the coordinator, whose report
        // is checked against that text.
        let mut opened = None;
        if approval.rung == DeliveryRung::Pr {
            match self.open_approved_pr(&approval) {
                Ok(found) => {
                    approval.status = DeliveryApprovalStatus::Executed;
                    approval.executed_at_ms = Some(now_ms());
                    approval.outcome = Some(found.url.clone());
                    opened = Some(found);
                }
                Err(error) => {
                    approval.notice = Some(format!(
                        "Tinto no pudo abrir el PR ({}): el coordinador lo abre con este título y cuerpo y lo informa",
                        error.message
                    ));
                }
            }
        }
        if approval.rung == DeliveryRung::Merge {
            let executed = self.merge_pr(&approval);
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
        if matches!(
            approval.rung,
            DeliveryRung::Commit | DeliveryRung::Sync | DeliveryRung::Push
        ) {
            let task = self.task(&approval.task_id)?;
            let place = Place::of(task.distro.as_deref());
            let executed = match approval.rung {
                DeliveryRung::Commit => run_commit_plan(&task, &mut approval),
                DeliveryRung::Sync => self.run_sync(&task, &mut approval),
                _ if self.commits_lost(&task)? => Err(DeliveryError::new(
                    "commit_missing",
                    "No se publicó: los commits que hizo Tinto ya no están en la rama. Vuelve a preparar el commit.",
                )),
                // Right before publishing: the base synced is still merged.
                _ => self.check_base_merged(&task).and_then(|()| {
                    tasks::push_branch(place, &task.worktree, &task.branch).map(|upstream| {
                        approval.tip = tasks::head(place, &task.worktree).ok();
                        format!("Publicado en {upstream}")
                    })
                }),
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
        if let Some(found) = opened {
            let task = self.task(&approval.task_id)?;
            self.adopt_pr(&task, &found)?;
            approval = self.approval(&approval.id)?;
        }
        // Read only once the step is saved as done, so the read does not
        // take Tinto's own merge for one made outside it.
        if approval.rung == DeliveryRung::Merge {
            let _ = self.refresh_pr(&approval.task_id, std::time::Duration::ZERO);
        }
        Ok(approval)
    }

    /// The user has Tinto open an approved PR step that is not open yet:
    /// Tinto could not reach GitHub when it was approved, or it was approved
    /// before Tinto opened PRs itself.
    pub fn open_pr_step(&self, approval_id: &str) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        if approval.rung != DeliveryRung::Pr || approval.status != DeliveryApprovalStatus::Approved
        {
            return Err(DeliveryError::new(
                "approval_not_approved",
                "solo se abre un paso PR aprobado que aún no tiene PR",
            ));
        }
        let found = self.open_approved_pr(&approval)?;
        approval.status = DeliveryApprovalStatus::Executed;
        approval.executed_at_ms = Some(now_ms());
        approval.outcome = Some(found.url.clone());
        approval.notice = None;
        self.save_approval(&approval, "approval_completed")?;
        let task = self.task(&approval.task_id)?;
        self.adopt_pr(&task, &found)?;
        self.approval(&approval.id)
    }

    /// The coordinator takes back a request the user has not answered, to
    /// ask it again with other text. The reason stays in the log.
    pub fn withdraw_approval(&self, approval_id: &str, reason: &str) -> Result<(), DeliveryError> {
        let approval = self.approval(approval_id)?;
        if approval.status != DeliveryApprovalStatus::Pending {
            return Err(DeliveryError::new(
                "approval_not_pending",
                "esta aprobación ya tiene una decisión del usuario y no se puede retirar ni sustituir; pide el paso siguiente o informa su resultado",
            ));
        }
        if reason.trim().is_empty() {
            return Err(DeliveryError::new(
                "invalid_arguments",
                "di por qué se retira la aprobación",
            ));
        }
        let rung = format!("{:?}", approval.rung).to_ascii_lowercase();
        let store = self.store()?;
        store.delete_approval(&approval.id)?;
        store.record_event(
            now_ms(),
            "approval_withdrawn",
            Some(&approval.task_id),
            None,
            &format!("{rung}: {}", reason.trim()),
        )?;
        drop(store);
        self.notify();
        Ok(())
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
        // A Jira step done by hand says what was done in Jira.
        if success && approval.rung == DeliveryRung::Jira && outcome.trim().is_empty() {
            return Err(DeliveryError::new(
                "invalid_outcome",
                "di qué se hizo en Jira (transiciones y estado final)",
            ));
        }
        // A PR step is never done without a PR Tinto can follow: the one
        // GitHub has for the branch wins over the URL in the outcome.
        let mut opened = None;
        if success && approval.rung == DeliveryRung::Pr {
            let task = self.task(&approval.task_id)?;
            let found = self
                .find_branch_pr(&task)
                .ok()
                .flatten()
                .or_else(|| super::prs::parse_pr_url(outcome))
                .ok_or_else(|| super::prs::no_branch_pr(&task.branch))?;
            // What was published has to be what the user approved.
            match self.published_difference(&found.url, &approval) {
                Ok(Some(difference)) => {
                    return Err(DeliveryError::new(
                        "pr_mismatch",
                        format!(
                            "{}: {difference}. Pon en el PR el título y el cuerpo aprobados (gh pr edit) y vuelve a informar",
                            super::prs::PUBLISHED_MISMATCH
                        ),
                    ))
                }
                Ok(None) => approval.notice = None,
                Err(_) => {
                    approval.notice = Some(
                        "No se pudo leer el PR en GitHub para comprobar que coincide con el texto aprobado"
                            .to_string(),
                    )
                }
            }
            opened = Some((task, found));
        }
        approval.status = if success {
            DeliveryApprovalStatus::Executed
        } else {
            DeliveryApprovalStatus::Failed
        };
        approval.executed_at_ms = Some(now_ms());
        approval.outcome = Some(outcome.trim().to_string())
            .filter(|text| !text.is_empty())
            .or_else(|| opened.as_ref().map(|(_, found)| found.url.clone()));
        self.save_approval(&approval, "approval_completed")?;
        if let Some((task, found)) = opened {
            self.adopt_pr(&task, &found)?;
            approval = self.approval(&approval.id)?;
        }
        Ok(approval)
    }

    /// The user corrects the result of a done step Tinto cannot check (PR,
    /// merge, Jira). The previous result is kept with the correction, and a
    /// PR step corrected with a PR's URL links that PR.
    pub fn correct_approval(
        &self,
        approval_id: &str,
        outcome: &str,
        reason: &str,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        let refuse = |message: &str| Err(DeliveryError::new("invalid_correction", message));
        if reason.trim().is_empty() {
            return refuse("di por qué se corrige el resultado");
        }
        if outcome.trim().is_empty() {
            return refuse("el resultado corregido no puede quedar vacío");
        }
        if matches!(
            approval.rung,
            DeliveryRung::Commit | DeliveryRung::Sync | DeliveryRung::Push
        ) {
            return refuse(
                "el commit, el sync y el push los comprueba Tinto con git; no se corrigen a mano",
            );
        }
        if !approval.status.was_done() {
            return refuse("solo se corrige el resultado de un paso hecho");
        }
        approval.corrections.push(super::model::DeliveryCorrection {
            previous: approval.outcome.take(),
            outcome: outcome.trim().to_string(),
            reason: reason.trim().to_string(),
            at_ms: now_ms(),
        });
        approval.outcome = Some(outcome.trim().to_string());
        let linked = match approval.rung {
            DeliveryRung::Pr => super::prs::parse_pr_url(outcome),
            _ => None,
        };
        if linked.is_some() {
            // The user's word on which PR it is.
            approval.status = DeliveryApprovalStatus::Executed;
            approval.notice = None;
        }
        self.save_approval(&approval, "approval_corrected")?;
        if let Some(pr) = linked {
            if self.task_pr(&approval.task_id)?.map(|linked| linked.url) != Some(pr.url.clone()) {
                self.link_pr(&approval.task_id, &pr)?;
            }
        }
        Ok(approval)
    }

    /// The merge method to propose: the one the base branch's history
    /// shows for its latest PRs.
    pub fn merge_method(&self, task_id: &str) -> Result<String, DeliveryError> {
        let task = self.task(task_id)?;
        let place = Place::of(task.distro.as_deref());
        Ok(tasks::base_merge_method(place, &task.repo, &task.base_ref).to_string())
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

    /// The branch the task's PR merges into: its PR's base once GitHub has
    /// it (a package retargeted), else the task's.
    fn sync_base(&self, task: &DeliveryTask) -> Result<String, DeliveryError> {
        Ok(self
            .task_pr(&task.id)?
            .and_then(|pr| pr.base)
            .unwrap_or_else(|| task.base_ref.clone()))
    }

    /// A sync starts from a clean worktree no job is changing, with no merge
    /// in progress and a base, fetched now, that is not merged yet. Returns
    /// the base's name and commit.
    fn sync_prerequisites(&self, task: &DeliveryTask) -> Result<(String, String), DeliveryError> {
        let place = Place::of(task.distro.as_deref());
        let refuse = |category: &str, message: &str| Err(DeliveryError::new(category, message));
        if tasks::merge_head(place, &task.worktree).is_some() {
            return refuse(
                "merge_in_progress",
                "ya hay un merge en curso en el worktree; se concluye con el paso commit",
            );
        }
        if tasks::worktree_state(place, &task.worktree, &task.base_commit)?.dirty {
            return refuse(
                "worktree_dirty",
                "el worktree tiene cambios sin confirmar: confírmalos antes de sincronizar",
            );
        }
        if self
            .store()?
            .task_jobs(&task.id)?
            .iter()
            .any(|job| job.status.is_open())
        {
            return refuse(
                "job_running",
                "un job de la tarea está en curso; sincroniza cuando termine",
            );
        }
        let base = self.sync_base(task)?;
        let (name, commit) = tasks::base_tip(place, &task.worktree, &base, true)?;
        if tasks::on_branch(place, &task.worktree, &commit) {
            return Err(DeliveryError::new(
                "base_merged",
                format!(
                    "la rama ya incluye {name} ({}): no hay nada que sincronizar",
                    short(&commit)
                ),
            ));
        }
        Ok((name, commit))
    }

    /// Fetches the task's base and merges it into the branch. Without
    /// conflicts the step leaves a merge commit; with them it leaves the
    /// merge in progress for an implementation job to resolve and a commit
    /// step to conclude.
    fn run_sync(
        &self,
        task: &DeliveryTask,
        approval: &mut DeliveryApproval,
    ) -> Result<String, DeliveryError> {
        let (name, commit) = self.sync_prerequisites(task)?;
        let place = Place::of(task.distro.as_deref());
        approval.merge_head = Some(commit.clone());
        let message = match approval.body.trim() {
            "" => approval.title.clone(),
            body => format!("{}\n\n{body}", approval.title),
        };
        match tasks::merge_base(place, &task.worktree, &commit, &message)? {
            tasks::Merged::Commit(merge) => {
                approval.created = vec![merge.clone()];
                Ok(format!(
                    "{name} ({}) fusionada en la rama: commit de merge {}",
                    short(&commit),
                    short(&merge)
                ))
            }
            tasks::Merged::Conflicts(files) => {
                let outcome = format!(
                    "{name} ({}) tiene conflictos con la rama en: {}. El merge queda en curso: un job de implementación los resuelve y el paso commit lo concluye.",
                    short(&commit),
                    files.join(", ")
                );
                approval.conflicts = files;
                Ok(outcome)
            }
        }
    }

    /// The conflicts a merge in progress still has in the task's worktree.
    pub(super) fn merge_conflicts(
        &self,
        task: &DeliveryTask,
    ) -> Result<Option<DeliveryMergeConflicts>, DeliveryError> {
        let place = Place::of(task.distro.as_deref());
        if tasks::merge_head(place, &task.worktree).is_none() {
            return Ok(None);
        }
        let files = tasks::unresolved_conflicts(place, &task.worktree);
        if files.is_empty() {
            return Ok(None);
        }
        let base = self.sync_base(task)?;
        let base =
            tasks::base_tip(place, &task.worktree, &base, false).map_or(base, |(name, _)| name);
        Ok(Some(DeliveryMergeConflicts { base, files }))
    }

    /// Before a push: no merge left in progress, and the base the latest
    /// sync merged still in the branch (a commit made over the merge must
    /// keep it as a parent).
    pub(super) fn check_base_merged(&self, task: &DeliveryTask) -> Result<(), DeliveryError> {
        let place = Place::of(task.distro.as_deref());
        if tasks::merge_head(place, &task.worktree).is_some() {
            return Err(DeliveryError::new(
                "merge_in_progress",
                "hay un merge en curso en el worktree: concluye con el paso commit antes de publicar",
            ));
        }
        let synced = self
            .latest_approval(&task.id, DeliveryRung::Sync)?
            .filter(|sync| sync.status == DeliveryApprovalStatus::Executed)
            .and_then(|sync| sync.merge_head);
        match synced {
            Some(base) if !tasks::on_branch(place, &task.worktree, &base) => {
                Err(DeliveryError::new(
                    "base_unmerged",
                    format!(
                        "la base que se sincronizó ({}) ya no está en la rama: sincroniza de nuevo antes de publicar",
                        short(&base)
                    ),
                ))
            }
            _ => Ok(()),
        }
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
        // Only the parents of the last commit were put right outside Tinto
        // (a merge's second parent restored): the branch has the same
        // content in the same place, so the step stands on the new commit.
        if let Some(head) = reparented(place, &task.worktree, &created) {
            approval.created = created;
            if let Some(last) = approval.created.last_mut() {
                *last = head;
            }
            self.save_approval(&approval, "commit_reparented")?;
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

    /// Checks a done push against the remote branch: it must still exist and
    /// end where Tinto published it. Once the PR is merged GitHub deletes the
    /// branch, so nothing is checked; a remote that cannot be read says
    /// nothing either.
    pub(super) fn check_push(&self, task: &DeliveryTask) -> Result<(), DeliveryError> {
        let Some(push) = self
            .latest_approval(&task.id, DeliveryRung::Push)?
            .filter(|push| push.status.was_done())
        else {
            return Ok(());
        };
        let merged = self
            .latest_approval(&task.id, DeliveryRung::Merge)?
            .is_some_and(|merge| merge.status == DeliveryApprovalStatus::Executed)
            || self
                .task_pr(&task.id)?
                .is_some_and(|pr| pr.state == "merged");
        if merged {
            return self.set_drift(push, None);
        }
        let place = Place::of(task.distro.as_deref());
        let Ok(remote) = tasks::remote_tip(place, &task.repo, &task.branch) else {
            return Ok(());
        };
        // Pushes from before Tinto kept the tip: the last one git saw.
        let tracking = format!("refs/remotes/origin/{}", task.branch);
        let published = push
            .tip
            .clone()
            .or_else(|| tasks::git(place, &task.repo, &["rev-parse", &tracking]).ok());
        let drift = match remote {
            None => Some("La rama remota ya no existe"),
            Some(tip) if published.is_some_and(|published| published != tip) => {
                Some("La rama remota cambió fuera de Tinto")
            }
            Some(_) => None,
        };
        self.set_drift(push, drift.map(str::to_string))
    }

    /// Marks a done step as no longer matching git or GitHub, with why, or
    /// as done again once what it recorded holds again.
    pub(super) fn set_drift(
        &self,
        mut step: DeliveryApproval,
        drift: Option<String>,
    ) -> Result<(), DeliveryError> {
        let before = step.clone();
        match drift {
            Some(why) => {
                step.status = DeliveryApprovalStatus::Drifted;
                step.notice = Some(why);
            }
            None if step.status == DeliveryApprovalStatus::Drifted => {
                step.status = DeliveryApprovalStatus::Executed;
                step.notice = None;
            }
            None => {}
        }
        if step == before {
            return Ok(());
        }
        let event = match step.status {
            DeliveryApprovalStatus::Drifted => "step_drifted",
            _ => "step_matches",
        };
        self.save_approval(&step, event)
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

    /// A merge needs the followed PR open, without conflicts and with its
    /// checks finished and green. GitHub still enforces its own rules.
    fn merge_prerequisites(
        &self,
        task: &DeliveryTask,
        method: Option<&str>,
    ) -> Result<(), DeliveryError> {
        if !matches!(method, Some("squash" | "merge" | "rebase")) {
            return Err(DeliveryError::new(
                "invalid_approval",
                "elige cómo fusionar: squash, merge o rebase",
            ));
        }
        self.ensure_pr_linked(task)?;
        let pr = self
            .refresh_pr(&task.id, std::time::Duration::from_secs(30))?
            .ok_or_else(|| {
                let mut error = super::prs::no_branch_pr(&task.branch);
                error.category = "pr_missing".to_string();
                error
            })?;
        // A package merged before it may have just freed this one.
        self.retarget_packages()?;
        let task = &self.task(&task.id)?;
        let blocker = if pr.state != "open" {
            Some(format!(
                "el PR #{} no está abierto ({})",
                pr.number, pr.state
            ))
        } else if let Some(why) = self.package_merge_refusal(task)? {
            Some(format!("el PR #{} {why}", pr.number))
        } else if !pr.is_approved() {
            Some(format!(
                "el PR #{} no tiene una revisión aprobada",
                pr.number
            ))
        } else if pr.conflicts {
            Some(format!("el PR #{} tiene conflictos con su base", pr.number))
        } else if pr.checks.failing > 0 {
            Some(format!("el PR #{} tiene checks fallando", pr.number))
        } else if pr.checks.pending > 0 {
            Some(format!("el PR #{} tiene checks en curso", pr.number))
        } else {
            None
        };
        match blocker {
            Some(message) => Err(DeliveryError::new("pr_not_ready", message)),
            None => Ok(()),
        }
    }

    /// Runs a Jira step on the task's issue: links its PR once it is open
    /// and walks the issue through the step's statuses, verifying the last.
    fn run_jira_step(&self, approval: &DeliveryApproval) -> Result<String, DeliveryError> {
        let task = self.task(&approval.task_id)?;
        let site = self.jira_site(&task.repo)?.ok_or_else(|| {
            DeliveryError::new(
                "jira_not_configured",
                "el Jira de este repositorio ya no tiene host o token en Tinto",
            )
        })?;
        let pr = match approval.stage.as_deref() {
            Some(JIRA_AFTER_PR) => self.task_pr(&task.id)?,
            _ => None,
        };
        super::jira::run_step(
            &*self.jira_client(),
            &site,
            &task.key,
            pr.as_ref().map(|pr| (pr.url.as_str(), pr.number)),
            &approval.jira_statuses,
        )
    }

    /// Merges the task's PR on GitHub and deletes its branch there.
    fn merge_pr(&self, approval: &DeliveryApproval) -> Result<String, DeliveryError> {
        let pr = self.task_pr(&approval.task_id)?.ok_or_else(|| {
            DeliveryError::new("pr_missing", "la tarea ya no tiene un PR de GitHub")
        })?;
        let method = approval.merge_method.as_deref().unwrap_or("squash");
        self.github_client()
            .merge(&pr.url, method, &approval.title, &approval.body)?;
        Ok(format!(
            "PR #{} fusionado ({method}) y rama {} borrada en GitHub",
            pr.number,
            self.task(&approval.task_id)
                .map(|task| task.branch)
                .unwrap_or_default()
        ))
    }

    pub(super) fn save_approval(
        &self,
        approval: &DeliveryApproval,
        event: &str,
    ) -> Result<(), DeliveryError> {
        let store = self.store()?;
        store.put_approval(approval)?;
        // The step first, so the coordinator's wake-up can name it.
        let step = format!("{:?}", approval.rung).to_ascii_lowercase();
        let detail = match approval.outcome.as_deref().unwrap_or_default() {
            "" => step,
            outcome => format!("{step}: {outcome}"),
        };
        store.record_event(now_ms(), event, Some(&approval.task_id), None, &detail)?;
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
    // Unstaging would end a merge in progress: it is concluded only by the
    // step prepared for it.
    if tasks::merge_head(place, &task.worktree) != approval.merge_head {
        return Err(invalid_plan(match approval.merge_head {
            Some(_) => "No se creó ningún commit: el merge en curso cuando se preparó el commit ya no está. Prepáralo de nuevo.",
            None => "No se creó ningún commit: hay un merge en curso que este plan no contemplaba. Prepara de nuevo el commit, que concluirá el merge.",
        }));
    }
    check_plan(&changed, &approval.commits, &approval.left_out).map_err(|error| {
        invalid_plan(format!(
            "No se creó ningún commit: el worktree cambió desde que se preparó el plan ({}). Prepáralo de nuevo.",
            error.message
        ))
    })?;
    approval.before = Some(tasks::head(place, &task.worktree)?);
    if approval.merge_head.is_some() {
        check_conflicts_resolved(place, &task.worktree)?;
        let message = &approval.commits[0].message;
        let created = tasks::commit_merge(place, &task.worktree, message).map_err(|error| {
            DeliveryError::new(
                "commit_failed",
                format!(
                    "El commit de merge falló: {}. El merge sigue en curso en el worktree.",
                    error.message
                ),
            )
        })?;
        approval.created.push(created);
        return Ok(format!("Commit de merge {}", shorts(&approval.created)));
    }
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

/// The branch's HEAD when it replaces the last of `created` with the same
/// tree on the same first parent (only its other parents differ), and the
/// rest of `created` is still on the branch.
fn reparented(place: Place<'_>, worktree: &std::path::Path, created: &[String]) -> Option<String> {
    let (last, earlier) = created.split_last()?;
    if !earlier
        .iter()
        .all(|commit| tasks::on_branch(place, worktree, commit))
    {
        return None;
    }
    // Tree, then parents.
    let shape = |commit: &str| -> Option<(String, Vec<String>)> {
        let text = tasks::git(place, worktree, &["show", "-s", "--format=%T %P", commit]).ok()?;
        let mut ids = text.split_whitespace().map(str::to_string);
        Some((ids.next()?, ids.collect()))
    };
    let (tree, parents) = shape("HEAD")?;
    let (old_tree, old_parents) = shape(last)?;
    // An amend keeps the parents: that is a rewrite, not a fix.
    let only_parents = tree == old_tree
        && parents != old_parents
        && parents.first().is_some()
        && parents.first() == old_parents.first();
    only_parents
        .then(|| tasks::head(place, worktree).ok())
        .flatten()
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
        // Checked with the service: the followed PR, or the task's jobs.
        DeliveryRung::Merge | DeliveryRung::Jira | DeliveryRung::Sync => {}
    }
    Ok(())
}
