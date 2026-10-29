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
//!   branch (jobs cannot run git merge). It runs as it is asked, without the
//!   user: it publishes nothing. Conflicts leave the merge in progress for an
//!   implementation job, and a commit step concludes it.
//! - Once the coordinator ends a run's brainstorm, its commit and push rungs
//!   are approved as they are asked; the PR and Jira rungs still wait.

use super::model::{
    DeliveryApproval, DeliveryApprovalStatus, DeliveryBlockerKind, DeliveryCommit,
    DeliveryDecisionKind, DeliveryDecisionOption, DeliveryDecisionStatus, DeliveryJob,
    DeliveryMergeConflicts, DeliveryRound, DeliveryRun, DeliveryRung,
    DeliverySettingsProposalStatus, DeliveryTask, NewBlocker, SINGLE_COMMIT_MAX_FILES,
    SINGLE_COMMIT_MAX_LINES,
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
    /// `pr_reply` only: the review comment answered, or the PR for a
    /// general comment.
    pub comment_url: Option<String>,
    /// `pr_reply` only: resolve the review thread once replied.
    pub resolve_thread: bool,
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

/// The question Tinto asks when other work got into a merge in progress,
/// and its two answers.
pub const MIXED_QUESTION: &str =
    "Hay cambios de otras rondas mezclados con el merge en curso: ¿cómo se confirman?";
pub const MIXED_ONE_COMMIT: &str = "Un solo commit de merge con todo";
pub const MIXED_SPLIT: &str = "Separarlos a mano";
/// The blocker the user clears once the split by hand is done; Tinto then
/// runs the checks again.
pub const MIXED_SPLIT_NOTE: &str =
    "Separa a mano el merge de los cambios de otras rondas (commits aparte) y quita este bloqueo";

/// What the mixed rounds changed, for the merge commit's message.
fn mixed_body(mixed: &[&DeliveryRound], jobs: &[&DeliveryJob]) -> String {
    let mut lines = vec!["Incluye también cambios hechos durante el merge:".to_string()];
    for round in mixed {
        let mut paths: Vec<String> = jobs
            .iter()
            .filter(|job| job.created_at_ms >= round.opened_at_ms)
            .filter(|job| {
                round
                    .closed_at_ms
                    .is_none_or(|closed| job.created_at_ms <= closed)
            })
            .flat_map(|job| job.changes.iter().map(|change| change.path.clone()))
            .collect();
        paths.sort();
        paths.dedup();
        lines.push(format!(
            "- Ronda {} ({}): {}",
            round.number,
            round.cause,
            paths.join(", ")
        ));
    }
    lines.join(
        "
",
    )
}

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
        head: None,
        author: None,
        notice: None,
        corrections: Vec::new(),
        comment_url: None,
        resolve_thread: false,
        images: Vec::new(),
        thread: None,
        style_examples: Vec::new(),
        fix_commits: Vec::new(),
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
        let store = self.store()?;
        // Nothing before it concerns its coordinator.
        let start = store.last_event_seq()?;
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
            coordinator_cursor: start,
            returned_cursor: start,
            notified_cursor: 0,
            notified_at_ms: None,
        };
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

    /// What of the batch still waits on the user's answer, if anything: a
    /// task's decision or a settings proposal. Nothing past the brainstorm.
    pub(super) fn brainstorm_waits(
        &self,
        run: &DeliveryRun,
    ) -> Result<Option<String>, DeliveryError> {
        if run.unattended {
            return Ok(None);
        }
        let tasks = self.store()?.tasks()?;
        for task in tasks
            .iter()
            .filter(|task| task.run_id.as_deref() == Some(run.id.as_str()))
        {
            if self
                .task_decisions(&task.id)?
                .iter()
                .any(|decision| decision.status == DeliveryDecisionStatus::Pending)
            {
                return Ok(Some(format!("{} tiene decisiones sin responder", task.key)));
            }
        }
        let proposing = self
            .repo_settings_proposals(&run.repo)?
            .iter()
            .any(|proposal| {
                proposal.run_id == run.id
                    && proposal.status == DeliverySettingsProposalStatus::Pending
            });
        Ok(proposing.then(|| "hay una propuesta de ajustes sin decidir".to_string()))
    }

    /// The coordinator ends the batch's brainstorm once the user has
    /// answered everything it asked, so nothing is left for the stages to
    /// wait on. From then on the batch runs without stopping until a PR.
    pub fn end_brainstorm(&self, run_id: &str) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.active_run(run_id)?;
        if let Some(why) = self.brainstorm_waits(&run)? {
            return Err(DeliveryError::new(
                "brainstorm_open",
                format!("el brainstorm sigue: {why}"),
            ));
        }
        run.unattended = true;
        run.updated_at_ms = now_ms();
        self.save_run(&run, "run_policy", "unattended")?;
        self.take_allowed_steps(run_id)?;
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

    /// The steps the task's repo lets Tinto take on its own.
    pub(super) fn autonomy(
        &self,
        task: &DeliveryTask,
    ) -> Result<super::model::DeliveryAutonomy, DeliveryError> {
        Ok(self.store()?.repo_settings(&task.repo)?.autonomy)
    }

    /// Whether Tinto takes this step of the task on its own now: past the
    /// brainstorm, when the repo lets it.
    pub(super) fn takes_on_its_own(
        &self,
        task: &DeliveryTask,
        step: impl FnOnce(&super::model::DeliveryAutonomy) -> bool,
    ) -> Result<bool, DeliveryError> {
        Ok(self.unattended(task)? && step(&self.autonomy(task)?))
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
        let reply = request.rung == DeliveryRung::PrReply;
        if request.title.trim().is_empty() && !planned && !reply {
            return Err(DeliveryError::new(
                "invalid_approval",
                "la aprobación necesita el texto exacto (título o mensaje)",
            ));
        }
        // A reply names the comment it answers, which the user reads with
        // it: a review comment with its diff, or the PR for a general one.
        let (comment_url, thread) = if reply {
            let url = request.comment_url.as_deref().unwrap_or_default().trim();
            if request.body.trim().is_empty() {
                return Err(DeliveryError::new(
                    "invalid_approval",
                    "la respuesta necesita su texto exacto en body",
                ));
            }
            if let Some(label) = super::prs::status_label(&request.body) {
                return Err(DeliveryError::new(
                    "reply_status_label",
                    format!(
                        "la respuesta empieza con la etiqueta «{label}» y la regla es no abrir con etiquetas de estado (Fixed, Done, Addressed, Resolved): empieza por lo que hace el código ahora, en presente, como en style_examples"
                    ),
                ));
            }
            let thread = if super::threads::parse_comment_url(url).is_some() {
                let reference = super::threads::NewReference {
                    comment_url: Some(url.to_string()),
                    ..Default::default()
                };
                self.resolve_references(&task, &[reference]).pop()
            } else if super::prs::parse_pr_url(url).is_some() {
                None
            } else {
                return Err(DeliveryError::new(
                    "invalid_approval",
                    "comment_url tiene que ser un comentario de revisión (…/pull/<n>#discussion_r<id>) o el PR, para un comentario general",
                ));
            };
            if request.resolve_thread {
                match thread.as_ref().filter(|thread| thread.error.is_none()) {
                    Some(thread) => self.check_fix_pushed(&task, &thread.path)?,
                    None if thread.is_some() => {
                        return Err(DeliveryError::new(
                            "fix_unpushed",
                            "no se pudo leer el comentario para comprobar que su arreglo está publicado: pide la respuesta sin resolve_thread",
                        ))
                    }
                    None => {}
                }
            }
            (Some(url.to_string()), thread)
        } else {
            (None, None)
        };
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
                    && approval.comment_url == comment_url
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
        // The coordinator commits only settled work: findings are fixed or
        // accepted first. The user may commit as they judge.
        if request.rung == DeliveryRung::Commit && request.requested_by != "user" {
            let jobs = self.store()?.task_jobs(&task.id)?;
            let jobs: Vec<&super::model::DeliveryJob> = jobs.iter().collect();
            if let Some(why) = super::phase::commit_blocker(&task.checks, &jobs) {
                return Err(DeliveryError::new(
                    "stages_unsettled",
                    format!(
                        "{why}: arréglalos con una etapa de implementation, o acéptalos con resolve_job y una nota que diga por qué no bloquean; después pide el commit"
                    ),
                ));
            }
        }
        let mut title = request.title.trim().to_string();
        let mut body = request.body.clone();
        let mut commits = Vec::new();
        let mut left_out = Vec::new();
        let mut single_commit_reason = None;
        let mut notice = None;
        let mut style_examples = Vec::new();
        let mut fix_commits = Vec::new();
        let merge_head = match request.rung {
            DeliveryRung::Commit => tasks::merge_head(place, &task.worktree),
            _ => None,
        };
        // The user sees who the commits will be by, as git's config says:
        // Tinto never sets an identity of its own.
        let author = match request.rung {
            DeliveryRung::Commit => {
                let author = tasks::author(place, &task.worktree)?;
                notice = author_notice(
                    &author,
                    &tasks::recent_authors(place, &task.worktree, &task.base_commit),
                );
                Some(author)
            }
            _ => None,
        };
        // A push publishes the commit the user saw when it was asked.
        let head = match request.rung {
            DeliveryRung::Push => tasks::head(place, &task.worktree).ok(),
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
            DeliveryRung::PrReply => {
                title = match thread.as_ref().and_then(|thread| thread.comment.as_ref()) {
                    Some(comment) => format!("Respuesta a {}", comment.author),
                    None => "Respuesta en el PR".to_string(),
                };
                notice = super::prs::unanswered_notice(thread.as_ref(), &body);
                style_examples = self.style_examples(&task);
                fix_commits = self.fix_commits(&task, comment_url.as_deref(), thread.as_ref());
            }
            _ => {}
        }
        // A repo whose pushes start no CI: the messages Tinto writes say so,
        // in the exact text the user approves.
        if matches!(
            request.rung,
            DeliveryRung::Commit | DeliveryRung::Sync | DeliveryRung::Merge
        ) {
            let settings = self.store()?.repo_settings(&task.repo)?;
            for commit in &mut commits {
                commit.message = settings.ci_message(&commit.message);
            }
            if request.rung != DeliveryRung::Commit {
                body = settings.ci_message(&body).trim_start().to_string();
            }
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
            head,
            author,
            notice,
            corrections: Vec::new(),
            resolve_thread: reply && request.resolve_thread,
            // The QA screenshots its text shows where it names them.
            images: match request.rung {
                DeliveryRung::Pr | DeliveryRung::PrReply => {
                    self.named_screenshots(&task.id, &body)?
                }
                _ => Vec::new(),
            },
            comment_url,
            thread,
            style_examples,
            fix_commits,
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
        // A sync only brings the base into the local branch, publishing
        // nothing, and Tinto refuses it on a dirty worktree or a running
        // job: it runs as it is asked, brainstorm or not.
        // A restack rewrites the branch's history: it waits like a commit.
        let unattended = (approval.rung == DeliveryRung::Sync && task.restack_from.is_none())
            || self.unattended(&task)?;
        if unattended && self.step_allowed(&task, &approval)? {
            return self.decide_approval(&approval.id, true, None);
        }
        Ok(approval)
    }

    /// Whether the repo lets Tinto take this step on its own past the
    /// brainstorm (by default commit, sync, push and the Jira steps it runs
    /// itself; opening the PR, merging it and replies wait for the user).
    fn step_allowed(
        &self,
        task: &DeliveryTask,
        approval: &DeliveryApproval,
    ) -> Result<bool, DeliveryError> {
        let autonomy = self.autonomy(task)?;
        Ok(match approval.rung {
            DeliveryRung::Commit => autonomy.commit,
            DeliveryRung::Sync => autonomy.sync,
            DeliveryRung::Push => autonomy.push,
            DeliveryRung::Pr => autonomy.pr,
            DeliveryRung::Merge => autonomy.merge,
            DeliveryRung::Jira => {
                autonomy.jira
                    && (!approval.jira_statuses.is_empty()
                        || (approval.jira_link_only && self.jira_site(&task.repo)?.is_some()))
            }
            DeliveryRung::PrReply => autonomy.pr_reply,
        })
    }

    /// Takes the batch's waiting steps the repo now lets Tinto take on its
    /// own: its brainstorm just ended, or the user changed what Tinto does
    /// by itself. A step requested before waits for the user otherwise.
    pub fn take_allowed_steps(&self, run_id: &str) -> Result<(), DeliveryError> {
        if !self.run(run_id)?.unattended {
            return Ok(());
        }
        let waiting: Vec<DeliveryApproval> = self
            .store()?
            .approvals()?
            .into_iter()
            .filter(|approval| approval.status == DeliveryApprovalStatus::Pending)
            .collect();
        for approval in waiting {
            let task = self.task(&approval.task_id)?;
            if task.run_id.as_deref() == Some(run_id) && self.step_allowed(&task, &approval)? {
                self.decide_approval(&approval.id, true, None)?;
            }
        }
        Ok(())
    }

    pub fn approval(&self, approval_id: &str) -> Result<DeliveryApproval, DeliveryError> {
        self.store()?
            .approval(approval_id)?
            .ok_or_else(|| DeliveryError::not_found("la aprobación", approval_id))
    }

    /// The texts of a commit, PR or merge step as the user edited them
    /// before approving: a commit plan's messages in order, else its title
    /// and body. They are the ones Tinto uses.
    pub fn set_step_text(
        &self,
        approval_id: &str,
        title: Option<String>,
        body: Option<String>,
        messages: Option<Vec<String>>,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        let editable = matches!(
            approval.rung,
            DeliveryRung::Commit | DeliveryRung::Pr | DeliveryRung::Merge
        );
        if !editable || approval.status != DeliveryApprovalStatus::Pending {
            return Err(DeliveryError::new(
                "invalid_approval",
                "solo se editan los textos de un commit, PR o fusión que aún no se aprobó",
            ));
        }
        let empty = |text: &str| {
            DeliveryError::new("invalid_approval", format!("{text} no puede quedar vacío"))
        };
        if let Some(messages) = messages {
            if messages.len() != approval.commits.len() {
                return Err(DeliveryError::new(
                    "invalid_approval",
                    "hay que dar un mensaje por cada commit del plan",
                ));
            }
            if messages.iter().any(|message| message.trim().is_empty()) {
                return Err(empty("el mensaje de un commit"));
            }
            for (commit, message) in approval.commits.iter_mut().zip(messages) {
                commit.message = message.trim().to_string();
            }
            if let [only] = approval.commits.as_slice() {
                approval.title = subject(&only.message).to_string();
            }
        }
        if let Some(title) = title {
            if title.trim().is_empty() {
                return Err(empty("el título"));
            }
            approval.title = title.trim().to_string();
        }
        if let Some(body) = body {
            approval.body = body.trim().to_string();
        }
        self.save_approval(&approval, "step_text_edited")?;
        Ok(approval)
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
            match self.open_approved_pr(&mut approval) {
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
        // Posted once, never retried: a failure says why, and the
        // coordinator asks again if it still makes sense.
        if approval.rung == DeliveryRung::PrReply {
            let posted = self.post_reply(&approval);
            approval.executed_at_ms = Some(now_ms());
            match posted {
                Ok((url, notice)) => {
                    approval.status = DeliveryApprovalStatus::Executed;
                    approval.outcome = Some(url);
                    approval.notice = notice;
                }
                Err(error) => {
                    approval.status = DeliveryApprovalStatus::Failed;
                    approval.outcome = Some(error.message);
                    approval.notice = None;
                }
            }
        }
        if matches!(
            approval.rung,
            DeliveryRung::Commit | DeliveryRung::Sync | DeliveryRung::Push
        ) {
            let task = self.task(&approval.task_id)?;
            let executed = match approval.rung {
                DeliveryRung::Commit => run_commit_plan(&task, &mut approval),
                DeliveryRung::Sync => self.run_sync(&task, &mut approval),
                _ if self.commits_lost(&task)? => Err(DeliveryError::new(
                    "commit_missing",
                    "No se publicó: los commits que hizo Tinto ya no están en la rama. Vuelve a preparar el commit.",
                )),
                _ => self.run_push(&task, &mut approval),
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
            // New commits on an open PR: its CI must run on them.
            if approval.rung == DeliveryRung::Push
                && approval.status == DeliveryApprovalStatus::Executed
            {
                if let Some(pr) = self.task_pr(&task.id)?.filter(|pr| pr.is_open()) {
                    approval.notice = self.label_for_ci(&task, &pr.url, true);
                    self.expect_ci(&task)?;
                }
            }
        }
        self.save_approval(&approval, "approval_approved")?;
        // A sync that merged cleanly changed the code no stage made: Tinto's
        // checks verify it, not an agent.
        if approval.rung == DeliveryRung::Sync
            && approval.status == DeliveryApprovalStatus::Executed
            && approval.conflicts.is_empty()
            && !approval.created.is_empty()
            && !self.same_tree(&approval.task_id, approval.before.as_deref())?
        {
            self.queue_task_checks(&approval.task_id, None)?;
        }
        if let Some(found) = opened {
            let task = self.task(&approval.task_id)?;
            self.adopt_pr(&task, &found)?;
            approval = self.approval(&approval.id)?;
        }
        // Read only once the step is saved as done, so the read does not
        // take Tinto's own merge for one made outside it.
        if approval.rung == DeliveryRung::Merge {
            let _ = self.refresh_pr(&approval.task_id, std::time::Duration::ZERO);
            if approval.status == DeliveryApprovalStatus::Executed {
                self.delete_merged_branch(&self.task(&approval.task_id)?)?;
            }
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
        let found = self.open_approved_pr(&mut approval)?;
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

    /// Past the brainstorm a PR with conflicts brings its base in without
    /// waiting for anyone: Tinto asks the sync itself, which runs as it is
    /// asked, and the conflicts it leaves go to an implementation job. When
    /// it cannot start now (a job running, a merge in progress, changes not
    /// committed) the coordinator sees the conflicts on the PR.
    pub(super) fn sync_conflicted_pr(
        &self,
        pr: &super::prs::DeliveryPr,
    ) -> Result<(), DeliveryError> {
        if !pr.is_open() || !pr.conflicts {
            return Ok(());
        }
        self.auto_sync(&self.task(&pr.task_id)?)
    }

    /// Once a PR of the batch merges, every other open PR of the repo into
    /// the same base brings it in at once, while the conflicts are few and
    /// fresh, instead of when its own PR is ready.
    pub(super) fn sync_after_merge(
        &self,
        merged: &super::prs::DeliveryPr,
    ) -> Result<(), DeliveryError> {
        let Some(base) = merged.base.as_deref() else {
            return Ok(());
        };
        let repo = self.task(&merged.task_id)?.repo;
        let store = self.store()?;
        let open: Vec<DeliveryTask> = store
            .tasks()?
            .into_iter()
            .filter(|task| task.id != merged.task_id && task.repo == repo)
            .filter(|task| {
                store
                    .pr(&task.id)
                    .ok()
                    .flatten()
                    .is_some_and(|pr| pr.is_open() && pr.base.as_deref() == Some(base))
            })
            .collect();
        drop(store);
        for task in open {
            self.auto_sync(&task)?;
        }
        Ok(())
    }

    /// Asks the sync of the task's base, which runs as it is asked; one that
    /// cannot start now is only logged.
    fn auto_sync(&self, task: &DeliveryTask) -> Result<(), DeliveryError> {
        let base = self.sync_base(task)?;
        let title = match task.restack_from {
            Some(_) => format!("Restack {} onto {base}", task.branch),
            None => format!("Merge branch '{base}' into {}", task.branch),
        };
        let request = NewApproval {
            task_id: task.id.clone(),
            rung: DeliveryRung::Sync,
            title,
            body: String::new(),
            requested_by: super::decisions::AUTO.to_string(),
            commits: Vec::new(),
            merge_method: None,
            stage: None,
            jira_statuses: Vec::new(),
            leave_out: Vec::new(),
            single_commit_reason: None,
            comment_url: None,
            resolve_thread: false,
        };
        if let Err(error) = self.request_approval(request) {
            eprintln!("tinto: delivery did not sync {}: {error}", task.key);
        }
        Ok(())
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
        // A restack is due while the branch still has the rewritten base's
        // commits; one done outside Tinto is not asked again.
        if let Some(from) = &task.restack_from {
            if tasks::on_branch(place, &task.worktree, from) {
                return Ok((name, commit));
            }
            let mut done = task.clone();
            done.restack_from = None;
            self.store()?.put_task(&done)?;
        }
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
        let task = &self.task(&task.id)?;
        let place = Place::of(task.distro.as_deref());
        approval.merge_head = Some(commit.clone());
        if let Some(from) = task.restack_from.clone() {
            return self.run_restack(task, approval, &name, &from);
        }
        let message = match approval.body.trim() {
            "" => approval.title.clone(),
            body => format!("{}\n\n{body}", approval.title),
        };
        match tasks::merge_base(place, &task.worktree, &commit, &message)? {
            tasks::Merged::Commit(merge) => {
                approval.created = vec![merge.clone()];
                Ok(format!(
                    "{name} ({}) fusionada en la rama: commit de merge {}{}",
                    short(&commit),
                    short(&merge),
                    submodules_note(place, &task.worktree)
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

    /// The sync whose merge is still in progress in the task's worktree.
    /// A sync whose merge git no longer has in progress, its merged commit on
    /// the branch: someone committed the merge outside Tinto. The sync
    /// records that commit, so nothing waits for a commit step to conclude
    /// it any more.
    fn record_merge_concluded_outside(&self, task: &DeliveryTask) -> Result<(), DeliveryError> {
        let approvals: Vec<DeliveryApproval> = self
            .store()?
            .approvals()?
            .into_iter()
            .filter(|approval| approval.task_id == task.id)
            .collect();
        let refs: Vec<&DeliveryApproval> = approvals.iter().collect();
        let Some(mut sync) = super::phase::open_merge(&refs, true).cloned() else {
            return Ok(());
        };
        let place = Place::of(task.distro.as_deref());
        let merged = sync
            .merge_head
            .as_deref()
            .is_some_and(|commit| tasks::on_branch(place, &task.worktree, commit));
        let Ok(head) = tasks::head(place, &task.worktree) else {
            return Ok(());
        };
        if !merged {
            return Ok(());
        }
        sync.created = vec![head];
        self.save_approval(&sync, "merge_concluded_outside")?;
        // Its round ends with that commit, as with Tinto's.
        let store = self.store()?;
        for mut round in store.task_rounds(&task.id)? {
            if round.kind != "sync"
                || round.source.as_deref() != Some(sync.id.as_str())
                || !round.is_open()
            {
                continue;
            }
            round.closed_at_ms = Some(now_ms());
            round.outcome = Some("merge confirmado fuera de Tinto".to_string());
            store.put_round(&round)?;
            store.record_event(
                now_ms(),
                "round_closed",
                Some(&task.id),
                None,
                &super::rounds::round_text(&round),
            )?;
        }
        drop(store);
        // The commit Tinto asked to conclude it has nothing left to commit.
        for asked in approvals.iter().filter(|approval| {
            approval.rung == DeliveryRung::Commit
                && approval.status == DeliveryApprovalStatus::Pending
                && approval.requested_by == "auto"
        }) {
            self.withdraw_approval(&asked.id, "el merge se confirmó fuera de Tinto")?;
        }
        Ok(())
    }

    fn task_open_merge(
        &self,
        task: &DeliveryTask,
    ) -> Result<Option<DeliveryApproval>, DeliveryError> {
        let approvals: Vec<DeliveryApproval> = self
            .store()?
            .approvals()?
            .into_iter()
            .filter(|approval| approval.task_id == task.id)
            .collect();
        let refs: Vec<&DeliveryApproval> = approvals.iter().collect();
        let place = Place::of(task.distro.as_deref());
        let in_progress = tasks::merge_head(place, &task.worktree).is_some();
        Ok(super::phase::open_merge(&refs, in_progress).cloned())
    }

    /// While a sync's merge is in progress only its implementation and its
    /// review run, and nothing once they passed: a commit concludes a merge
    /// whole, so anything else done now could never be committed apart.
    pub(super) fn check_merge_stage(
        &self,
        task: &DeliveryTask,
        role: &str,
    ) -> Result<(), DeliveryError> {
        let Some(sync) = self.task_open_merge(task)? else {
            return Ok(());
        };
        let jobs = self.store()?.task_jobs(&task.id)?;
        let jobs: Vec<&DeliveryJob> = jobs.iter().collect();
        let resolving = super::phase::MERGE_STAGES.contains(&role)
            && !super::phase::merge_resolved(&task.checks, &jobs, &sync);
        if resolving {
            return Ok(());
        }
        Err(DeliveryError::new(
            "merge_in_progress",
            format!(
                "hay un merge en curso ({}): solo implementation y review lo resuelven. Confirma antes el merge con el paso commit (Tinto lo pide en cuanto sus etapas pasan); {role} corre después, encima del commit de merge",
                sync.title
            ),
        ))
    }

    /// After a job ended or a decision was answered: once the stages that
    /// resolve a merge passed, Tinto asks the commit that concludes it; a
    /// merge mixed with other work asks the user how to commit it; a merge
    /// concluded outside Tinto lets the rounds waiting for it go on.
    pub(super) fn conclude_merge(&self, task_id: &str) -> Result<(), DeliveryError> {
        let task = self.task(task_id)?;
        let Some(sync) = self.task_open_merge(&task)? else {
            self.record_merge_concluded_outside(&task)?;
            let store = self.store()?;
            return super::rounds::release_waiting(&store, task_id);
        };
        let store = self.store()?;
        let jobs = store.task_jobs(task_id)?;
        let rounds = store.task_rounds(task_id)?;
        let decisions = store.task_decisions(task_id)?;
        let asking = store.approvals()?.into_iter().any(|approval| {
            approval.task_id == task_id
                && approval.rung == DeliveryRung::Commit
                && matches!(
                    approval.status,
                    DeliveryApprovalStatus::Pending | DeliveryApprovalStatus::Approved
                )
        });
        drop(store);
        if asking {
            return Ok(());
        }
        let job_refs: Vec<&DeliveryJob> = jobs.iter().collect();
        let round_refs: Vec<&DeliveryRound> = rounds.iter().collect();
        let mixed = super::phase::mixed_into_merge(&round_refs, &job_refs, &sync);
        let body = if mixed.is_empty() {
            if !super::phase::merge_resolved(&task.checks, &job_refs, &sync) {
                return Ok(());
            }
            String::new()
        } else {
            let asked = decisions
                .iter()
                .find(|decision| decision.question == MIXED_QUESTION);
            let Some(asked) = asked else {
                self.ask_mixed_merge(&task, &mixed, &job_refs)?;
                return Ok(());
            };
            match asked.answer.as_deref() {
                Some(MIXED_ONE_COMMIT) => mixed_body(&mixed, &job_refs),
                Some(MIXED_SPLIT) => {
                    let noted = task.blockers.iter().any(|blocker| {
                        matches!(&blocker.kind, DeliveryBlockerKind::Note { text } if text == MIXED_SPLIT_NOTE)
                    });
                    if !noted {
                        self.add_blocker(
                            task_id,
                            NewBlocker::Note {
                                text: MIXED_SPLIT_NOTE.to_string(),
                            },
                        )?;
                    }
                    return Ok(());
                }
                _ => return Ok(()),
            }
        };
        let request = NewApproval {
            task_id: task.id.clone(),
            rung: DeliveryRung::Commit,
            title: sync.title.clone(),
            body,
            requested_by: super::decisions::AUTO.to_string(),
            commits: Vec::new(),
            merge_method: None,
            stage: None,
            jira_statuses: Vec::new(),
            leave_out: Vec::new(),
            single_commit_reason: None,
            comment_url: None,
            resolve_thread: false,
        };
        if let Err(error) = self.request_approval(request) {
            eprintln!(
                "tinto: delivery did not ask the merge commit of {}: {error}",
                task.key
            );
        }
        Ok(())
    }

    /// Asks the user how to commit a merge other work got mixed into.
    fn ask_mixed_merge(
        &self,
        task: &DeliveryTask,
        mixed: &[&DeliveryRound],
        jobs: &[&DeliveryJob],
    ) -> Result<(), DeliveryError> {
        let rounds: Vec<String> = mixed
            .iter()
            .map(|round| format!("ronda {} ({})", round.number, round.cause))
            .collect();
        let option = |label: &str, consequence: String| DeliveryDecisionOption {
            label: label.to_string(),
            consequence,
            recommended: false,
        };
        self.request_decisions(
            &task.id,
            vec![super::decisions::NewDecision {
                kind: DeliveryDecisionKind::Choice,
                question: MIXED_QUESTION.to_string(),
                detail: format!(
                    "Cambios hechos durante el merge en {}:\n{}",
                    rounds.join("; "),
                    mixed_body(mixed, jobs)
                ),
                references: Vec::new(),
                options: vec![
                    option(
                        MIXED_ONE_COMMIT,
                        "Un commit concluye el merge con todo; su mensaje lista qué cambios vinieron de la revisión.".to_string(),
                    ),
                    option(
                        MIXED_SPLIT,
                        "Separas a mano el merge de esos cambios; después una etapa verifica el resultado y las comprobaciones de Tinto vuelven a correr.".to_string(),
                    ),
                ],
                text: String::new(),
                command: None,
                undo: String::new(),
                scripts: Vec::new(),
                read_dirs: Vec::new(),
                write_dirs: Vec::new(),
                clients: Vec::new(),
                full_access: false,
                blocking: true,
            }],
            super::decisions::AUTO,
        )?;
        Ok(())
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
        let Some(synced) = synced.filter(|base| !tasks::on_branch(place, &task.worktree, base))
        else {
            return Ok(());
        };
        // Rebuilt on the PR's current base (a restack after the base merged
        // rewritten): the old synced commit is gone for good reason.
        let base = self.sync_base(task)?;
        let current = tasks::base_tip(place, &task.worktree, &base, true)
            .or_else(|_| tasks::base_tip(place, &task.worktree, &base, false));
        if current.is_ok_and(|(_, commit)| tasks::on_branch(place, &task.worktree, &commit)) {
            return Ok(());
        }
        Err(DeliveryError::new(
            "base_unmerged",
            format!(
                "la base que se sincronizó ({}) ya no está en la rama, ni la base actual del PR ({base}): sincroniza de nuevo antes de publicar",
                short(&synced)
            ),
        ))
    }

    /// Whether the task's HEAD has the same content as `commit`.
    fn same_tree(&self, task_id: &str, commit: Option<&str>) -> Result<bool, DeliveryError> {
        let task = self.task(task_id)?;
        let place = Place::of(task.distro.as_deref());
        let Some(commit) = commit else {
            return Ok(false);
        };
        let tree = tasks::tree(place, &task.worktree, commit);
        Ok(tree.is_some() && tree == tasks::tree(place, &task.worktree, "HEAD"))
    }

    /// Publishes the branch as approved: the HEAD the push was asked for, or
    /// past it only commits Tinto's own steps (sync, commit) added since,
    /// else `head_moved`. A branch Tinto restacked since its last push
    /// replaces the remote one only while that is still what Tinto
    /// published (`--force-with-lease`). A remote that already ends at HEAD
    /// (rewritten outside Tinto from this worktree) is adopted as published,
    /// and the checks run again when its content is not what was published.
    fn run_push(
        &self,
        task: &DeliveryTask,
        approval: &mut DeliveryApproval,
    ) -> Result<String, DeliveryError> {
        // Right before publishing: the base synced is still merged.
        self.check_base_merged(task)?;
        let place = Place::of(task.distro.as_deref());
        let head = tasks::head(place, &task.worktree)?;
        let approvals: Vec<DeliveryApproval> = self
            .store()?
            .approvals()?
            .into_iter()
            .filter(|other| other.task_id == task.id && other.id != approval.id)
            .collect();
        let mut added = String::new();
        if let Some(asked) = approval.head.clone().filter(|asked| *asked != head) {
            let since: Vec<String> = if tasks::on_branch(place, &task.worktree, &asked) {
                tasks::git(
                    place,
                    &task.worktree,
                    &["rev-list", "--first-parent", &format!("{asked}..HEAD")],
                )
                .map(|list| list.lines().map(str::to_string).collect())
                .unwrap_or_default()
            } else {
                Vec::new()
            };
            let own = |commit: &String| {
                approvals.iter().any(|step| {
                    matches!(step.rung, DeliveryRung::Commit | DeliveryRung::Sync)
                        && step.status.was_done()
                        && step.created.contains(commit)
                })
            };
            if since.is_empty() || !since.iter().all(own) {
                return Err(DeliveryError::new(
                    "head_moved",
                    format!(
                        "No se publicó (head_moved): la rama ya no está en {} como cuando se pidió el push, y lo nuevo no son pasos de Tinto. Vuelve a pedir el push para publicar {}.",
                        short(&asked),
                        short(&head)
                    ),
                ));
            }
            added = format!(
                ", con lo que Tinto añadió después de pedirlo ({})",
                shorts(&since)
            );
        }
        let last_push = approvals
            .iter()
            .rev()
            .find(|step| step.rung == DeliveryRung::Push && step.status.was_done());
        let published = last_push.and_then(|push| push.tip.clone());
        let remote = tasks::remote_tip(place, &task.repo, &task.branch)
            .ok()
            .flatten();
        approval.tip = Some(head.clone());
        if remote.as_deref() == Some(head.as_str()) {
            let same = published.as_deref().is_some_and(|published| {
                let tree = tasks::tree(place, &task.worktree, published);
                tree.is_some() && tree == tasks::tree(place, &task.worktree, &head)
            });
            if !same && published.is_some() {
                self.queue_task_checks(&task.id, None)?;
            }
            return Ok(format!(
                "La rama remota ya estaba en {}: Tinto la adopta como publicada{}",
                short(&head),
                if same || published.is_none() {
                    ""
                } else {
                    "; su contenido no es el publicado, así que vuelven a correr las comprobaciones"
                }
            ));
        }
        let restacked = approvals.iter().any(|step| {
            step.rung == DeliveryRung::Sync
                && step.status == DeliveryApprovalStatus::Executed
                && step.before.is_some()
                && step.executed_at_ms >= last_push.and_then(|push| push.executed_at_ms)
        });
        let lease = published
            .as_deref()
            .filter(|tip| restacked && !tasks::on_branch(place, &task.worktree, tip));
        let upstream = tasks::push_branch(place, &task.worktree, &task.branch, lease)?;
        Ok(match lease {
            Some(_) => {
                format!("Publicado en {upstream} (rama restackeada, con --force-with-lease){added}")
            }
            None => format!("Publicado en {upstream}{added}"),
        })
    }

    /// The sync of a task whose base merged rewritten: its own commits move
    /// onto the base (`rebase --onto <base> <from>`) in the worktree. Nothing
    /// is published; the push step replaces the remote branch.
    fn run_restack(
        &self,
        task: &DeliveryTask,
        approval: &mut DeliveryApproval,
        name: &str,
        from: &str,
    ) -> Result<String, DeliveryError> {
        let place = Place::of(task.distro.as_deref());
        let before = tasks::head(place, &task.worktree)?;
        let onto = approval.merge_head.clone().unwrap_or_default();
        if let Err(files) = tasks::restack(place, &task.worktree, &onto, from)? {
            return Err(DeliveryError::new(
                "restack_conflicts",
                format!(
                    "El restack sobre {name} tiene conflictos en: {}. La rama quedó como estaba: rebásala a mano (rebase --onto {name} {}) y publícala con el paso push.",
                    files.join(", "),
                    short(from)
                ),
            ));
        }
        approval.before = Some(before.clone());
        approval.created = tasks::git(
            place,
            &task.worktree,
            &["rev-list", "--reverse", &format!("{onto}..HEAD")],
        )?
        .lines()
        .map(str::to_string)
        .collect();
        let mut done = task.clone();
        done.restack_from = None;
        done.updated_at_ms = now_ms();
        self.store()?.put_task(&done)?;
        let content = if self.same_tree(&task.id, Some(&before))? {
            "el contenido no cambió: no hace falta repetir las comprobaciones"
        } else {
            "el contenido cambió: Tinto repite las comprobaciones"
        };
        Ok(format!(
            "Rama rebasada sobre {name} ({}) sin los commits de la base fusionada ({}): {} commit(s) propios; {content}. Publícala con el paso push, que reemplaza la rama remota con --force-with-lease.{}",
            short(&onto),
            short(from),
            approval.created.len(),
            submodules_note(place, &task.worktree)
        ))
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
        // The commits were put right outside Tinto with the same content (a
        // merge's second parent restored, a message reworded): the step
        // stands on the new ones. There is nothing to commit again.
        if let Some(rewritten) =
            rewritten(place, &task.worktree, &created, approval.before.as_deref())
        {
            approval.created = rewritten;
            self.save_approval(&approval, "commit_rewritten")?;
            return Ok(false);
        }
        // Committed and published outside Tinto (a merge redone and pushed by
        // hand), with nothing left to commit: the step stands on what the
        // branch has, and asking a commit again would find nothing.
        let head = tasks::head(place, &task.worktree)?;
        let clean = tasks::worktree_state(place, &task.worktree, &task.base_commit)
            .is_ok_and(|state| !state.dirty);
        if clean
            && tasks::merge_head(place, &task.worktree).is_none()
            && tasks::published(place, &task.worktree, &head)
        {
            approval.created = vec![head];
            self.save_approval(&approval, "commit_rewritten")?;
            return Ok(false);
        }
        // Another reader (the view, a wait) may have found it first.
        let still_done = self
            .store()?
            .approval(&approval.id)?
            .is_some_and(|stored| stored.status == DeliveryApprovalStatus::Executed);
        if !still_done {
            return Ok(true);
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
    /// have what Tinto published, at its end or behind commits pushed since
    /// (it moved ahead). Once the PR is merged GitHub deletes the branch, so
    /// nothing is checked; a remote that cannot be read says nothing either.
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
            None => Some("La rama remota ya no existe".to_string()),
            Some(tip)
                if published.as_ref().is_some_and(|published| {
                    *published != tip
                        && !tasks::remote_has(place, &task.repo, &task.branch, published, &tip)
                }) =>
            {
                // Rewritten outside Tinto from this worktree (re-authored,
                // restacked): the local branch is the remote one.
                let local = tasks::head(place, &task.worktree).ok();
                let published_tree = published
                    .as_deref()
                    .and_then(|published| tasks::tree(place, &task.repo, published));
                if local.as_deref() != Some(tip.as_str()) {
                    Some("La rama remota cambió fuera de Tinto".to_string())
                } else if published_tree.is_some()
                    && published_tree == tasks::tree(place, &task.repo, &tip)
                {
                    // Tinto's commits stand on their rewritten copies.
                    self.commits_lost(task)?;
                    return self.adopt_push(push, &tip);
                } else {
                    Some(format!(
                        "La rama remota cambió fuera de Tinto: es la rama local ({}), con otro contenido que lo publicado; al pedir el push de nuevo Tinto la adopta y repite las comprobaciones",
                        short(&tip)
                    ))
                }
            }
            Some(_) => None,
        };
        self.set_drift(push, drift)
    }

    /// The remote branch was rewritten outside Tinto with the content Tinto
    /// published, and the worktree has it: a pure history rewrite. The push
    /// stands on the new tip, with nothing to approve or check again.
    fn adopt_push(&self, mut push: DeliveryApproval, tip: &str) -> Result<(), DeliveryError> {
        push.outcome = Some(format!(
            "La rama remota se reescribió fuera de Tinto con el mismo contenido: Tinto adopta {} como publicada",
            short(tip)
        ));
        push.tip = Some(tip.to_string());
        push.status = DeliveryApprovalStatus::Executed;
        push.notice = None;
        self.save_approval(&push, "step_adopted")
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
            // A sync's merge committed by hand, its round still open.
            let store = self.store()?;
            if let Some(round) = super::rounds::open_of(&store, &task.id)? {
                drop(store);
                self.record_merge_concluded_outside(&task)?;
                self.settle_rerun_checks(&task, round)?;
            }
        }
        Ok(())
    }

    /// Closes a checks round whose failing checks passed when run again
    /// (after a setup step, say) while no stage of it changed the code, once
    /// the worktree is clean and, when the branch was published before it
    /// opened, equal to what was published: there is nothing to commit or
    /// push, and the task goes back to its PR.
    fn settle_rerun_checks(
        &self,
        task: &DeliveryTask,
        mut round: super::model::DeliveryRound,
    ) -> Result<(), DeliveryError> {
        if round.kind != "checks" || round.waits_merge_commit || task.checks.is_empty() {
            return Ok(());
        }
        {
            let jobs = self.store()?.task_jobs(&task.id)?;
            let changed = jobs.iter().any(|job| {
                job.agent != super::model::DeliveryAgent::Shell
                    && job.created_at_ms >= round.opened_at_ms
                    && job.undone_at_ms.is_none()
                    && !job.changes.is_empty()
            });
            let refs: Vec<&super::model::DeliveryJob> = jobs.iter().collect();
            let current = super::phase::current_checks(&task.checks, &refs);
            let passed = task.checks.iter().all(|command| {
                current.iter().any(|check| {
                    &check.prompt == command
                        && check.status == super::model::DeliveryJobStatus::Finished
                })
            });
            if changed || !passed || jobs.iter().any(|job| job.status.is_open()) {
                return Ok(());
            }
        }
        let place = Place::of(task.distro.as_deref());
        let Ok(state) = tasks::worktree_state(place, &task.worktree, &task.base_commit) else {
            return Ok(());
        };
        let published = !round.after_push || (state.has_upstream && state.unpublished == 0);
        if state.dirty || !published || tasks::merge_head(place, &task.worktree).is_some() {
            return Ok(());
        }
        let store = self.store()?;
        // Another reader may have closed it first.
        if super::rounds::open_of(&store, &task.id)?.map(|open| open.id) != Some(round.id.clone()) {
            return Ok(());
        }
        round.closed_at_ms = Some(now_ms());
        round.outcome = Some("pasaron al repetirlas; nada que publicar".to_string());
        store.put_round(&round)?;
        store.record_event(
            now_ms(),
            "round_closed",
            Some(&task.id),
            None,
            &super::rounds::round_text(&round),
        )?;
        drop(store);
        self.notify();
        Ok(())
    }

    /// A merge needs the followed PR open, approved (unless the repo merges
    /// without a review), without conflicts and with its checks finished and
    /// green. GitHub still enforces its own rules.
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
        } else if !pr.missing_approvals().is_empty() {
            Some(format!(
                "al PR #{} le falta la aprobación de {}: aprueban todos los revisores elegidos, salvo los que quites",
                pr.number,
                pr.missing_approvals().join(", ")
            ))
        } else if !pr
            .review_allows_merge(self.store()?.repo_settings(&task.repo)?.pr_without_reviewer)
        {
            Some(format!(
                "el PR #{} no tiene una revisión aprobada",
                pr.number
            ))
        } else if task.restack_from.is_some() {
            Some(format!(
                "el PR #{} aún lleva los commits de la base que se fusionó reescrita: pide el paso sync (restack) y publica la rama antes de fusionar",
                pr.number
            ))
        } else if pr.conflicts {
            Some(format!("el PR #{} tiene conflictos con su base", pr.number))
        } else {
            self.ci_refusal(task, &pr)
        };
        if let Some(message) = blocker {
            return Err(DeliveryError::new("pr_not_ready", message));
        }
        // CI passed on a branch without the base's latest commits: what
        // merges was never tested together.
        let place = Place::of(task.distro.as_deref());
        let base = self.sync_base(task)?;
        // Offline, the base as last fetched; unknown, nothing to compare.
        let tip = tasks::base_tip(place, &task.worktree, &base, true)
            .or_else(|_| tasks::base_tip(place, &task.worktree, &base, false));
        let Ok((name, commit)) = tip else {
            return Ok(());
        };
        if !tasks::on_branch(place, &task.worktree, &commit) {
            return Err(DeliveryError::new(
                "base_moved",
                format!(
                    "{name} avanzó ({}) desde la última CI en verde: sincroniza la rama, publícala y espera a que la CI pase de nuevo",
                    short(&commit)
                ),
            ));
        }
        Ok(())
    }

    /// Why the CI on the PR's head does not let it merge. A repo without
    /// workflows may have no check at all, nor a task the coordinator said
    /// needs none; what checks it has still count.
    pub(super) fn ci_refusal(
        &self,
        task: &DeliveryTask,
        pr: &super::prs::DeliveryPr,
    ) -> Option<String> {
        if pr.checks.state() == super::prs::CiState::NotRun
            && self.ci_not_needed(task, pr).is_some()
        {
            return None;
        }
        let place = Place::of(task.distro.as_deref());
        pr.ci_refusal(super::prs::ci_label(place, &task.worktree).as_deref())
    }

    /// Why CI that never ran is not needed on the task's PR: the
    /// coordinator said so, the repo has no workflows, its pushes skip CI
    /// (`ci_on_push: skip`), or no workflow runs for the files it changes.
    pub(super) fn ci_not_needed(
        &self,
        task: &DeliveryTask,
        pr: &super::prs::DeliveryPr,
    ) -> Option<String> {
        let place = Place::of(task.distro.as_deref());
        if task.skip_ci {
            return Some("la tarea no necesita CI (needs_ci: false)".to_string());
        }
        if !super::prs::has_ci(place, &task.worktree) {
            return Some("el repositorio no tiene workflows de CI".to_string());
        }
        let skips = self
            .store()
            .and_then(|store| store.repo_settings(&task.repo))
            .is_ok_and(|settings| settings.ci_on_push == super::model::DeliveryCiOnPush::Skip);
        if skips {
            return Some("los push de Tinto saltan la CI (ci_on_push: skip)".to_string());
        }
        let base = pr.base.clone().unwrap_or_else(|| task.base_ref.clone());
        super::prs::ci_skipped_by_paths(place, &task.worktree, &base, &task.branch)
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
        let method = approval.merge_method.as_deref().unwrap_or("squash");
        // The PR as it is now: the step may have waited while it moved.
        let task = self.task(&approval.task_id)?;
        self.refresh_pr(&task.id, std::time::Duration::ZERO)?;
        self.merge_prerequisites(&task, Some(method))?;
        let pr = self.task_pr(&approval.task_id)?.ok_or_else(|| {
            DeliveryError::new("pr_missing", "la tarea ya no tiene un PR de GitHub")
        })?;
        // Merged without CI having run: the outcome says why it was not needed.
        let no_ci = (pr.checks.state() == super::prs::CiState::NotRun)
            .then(|| self.ci_not_needed(&task, &pr))
            .flatten();
        // GitHub closes the PRs that go into a branch it deletes: those are
        // pointed at the merge target first (`delete_merged_branch`).
        let stacked = !self.stacked_prs(&task)?.is_empty();
        self.github_client()
            .merge(&pr.url, method, &approval.title, &approval.body, !stacked)?;
        let mut outcome = match stacked {
            false => format!(
                "PR #{} fusionado ({method}) y rama {} borrada en GitHub",
                pr.number, task.branch
            ),
            true => format!(
                "PR #{} fusionado ({method}); la rama {} se borra en GitHub en cuanto los PRs que van a ella apunten a {}",
                pr.number,
                task.branch,
                pr.base.as_deref().unwrap_or(&task.base_ref)
            ),
        };
        if let Some(why) = no_ci {
            outcome.push_str(&format!("; sin CI: {why}"));
        }
        Ok(outcome)
    }

    /// The open PRs of the batch that go into the task's branch.
    fn stacked_prs(
        &self,
        task: &DeliveryTask,
    ) -> Result<Vec<super::prs::DeliveryPr>, DeliveryError> {
        let store = self.store()?;
        let mut stacked = Vec::new();
        for other in store.tasks()? {
            if other.id == task.id || other.repo != task.repo {
                continue;
            }
            if let Some(pr) = store
                .pr(&other.id)?
                .filter(|pr| pr.is_open() && pr.base.as_deref() == Some(task.branch.as_str()))
            {
                stacked.push(pr);
            }
        }
        Ok(stacked)
    }

    /// After Tinto merged the task's PR keeping its branch: the PRs that go
    /// into it are pointed where it merged, then the branch is deleted.
    fn delete_merged_branch(&self, task: &DeliveryTask) -> Result<(), DeliveryError> {
        if self.stacked_prs(task)?.is_empty() {
            return Ok(());
        }
        self.retarget_packages()?;
        if !self.stacked_prs(task)?.is_empty() {
            eprintln!("tinto: delivery kept {}: PRs still go into it", task.branch);
            return Ok(());
        }
        let Some(repo) = super::prs::task_github_repo(task) else {
            return Ok(());
        };
        if let Err(error) = self.github_client().delete_branch(&repo, &task.branch) {
            eprintln!("tinto: delivery could not delete {}: {error}", task.branch);
        }
        Ok(())
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
        let detail = match (approval.status, approval.notice.as_deref()) {
            // Why it drifted and what puts it right.
            (DeliveryApprovalStatus::Drifted, Some(why)) if approval.rung == DeliveryRung::Push => {
                format!(
                    "{step}: {why}; revisa la rama y vuelve a pedir el paso push para publicarla"
                )
            }
            (DeliveryApprovalStatus::Drifted, Some(why)) => format!("{step}: {why}"),
            _ => match approval.outcome.as_deref().unwrap_or_default() {
                "" => step,
                outcome => format!("{step}: {outcome}"),
            },
        };
        store.record_event(now_ms(), event, Some(&approval.task_id), None, &detail)?;
        super::rounds::after_step(&store, approval)?;
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
    if let Some(shown) = &approval.author {
        let now = tasks::author(place, &task.worktree)?;
        if now != *shown {
            return Err(DeliveryError::new(
                "author_changed",
                format!("No se creó ningún commit: se aprobó con el autor {shown} y git usaría ahora {now}. Prepáralo de nuevo."),
            ));
        }
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

/// The commits that stand for `created` on the branch when they were
/// rewritten outside Tinto with the same content: HEAD has the tree of the
/// last of them. They are the branch's commits since `before` when that is
/// still on it (first parents only), else HEAD alone. A rewrite that changed
/// the content (an amend with other files, a rebase on a newer base) is not
/// one: the commit is prepared again.
fn rewritten(
    place: Place<'_>,
    worktree: &std::path::Path,
    created: &[String],
    before: Option<&str>,
) -> Option<Vec<String>> {
    let last = created.last()?;
    let tree = |commit: &str| {
        tasks::git(
            place,
            worktree,
            &["rev-parse", &format!("{commit}^{{tree}}")],
        )
        .ok()
    };
    if tree("HEAD")? != tree(last)? {
        return None;
    }
    let since = before
        .filter(|before| tasks::on_branch(place, worktree, before))
        .and_then(|before| {
            tasks::git(
                place,
                worktree,
                &[
                    "rev-list",
                    "--reverse",
                    "--first-parent",
                    &format!("{before}..HEAD"),
                ],
            )
            .ok()
        })
        .map(|list| list.lines().map(str::to_string).collect::<Vec<_>>())
        .filter(|list| !list.is_empty());
    match since {
        Some(commits) => Some(commits),
        None => tasks::head(place, worktree).ok().map(|head| vec![head]),
    }
}

/// A warning when the commits' author signs none of the base's latest
/// commits: a wrong identity is cheap to fix before the commit and costly
/// after (rewriting published history).
fn author_notice(author: &str, recent: &[String]) -> Option<String> {
    let email = author.rsplit_once('<')?.1.trim_end_matches('>');
    if recent.is_empty() || recent.iter().any(|known| known.eq_ignore_ascii_case(email)) {
        return None;
    }
    Some(format!(
        "Los commits se harán como {author}, que no firma ninguno de los últimos commits de la base (los firman {}). Si no es tu identidad, corrige user.name y user.email (git config) en el repositorio y pide el commit de nuevo",
        recent.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
    ))
}

/// Checks out the submodules at the commits the branch now records, after
/// Tinto moved it (sync, restack), and says so in the step's outcome. A
/// failure is said too: the next job tries again.
fn submodules_note(place: Place<'_>, worktree: &std::path::Path) -> String {
    match tasks::init_submodules(place, worktree, true) {
        Ok(Some(note)) => format!(". {note}"),
        Ok(None) => String::new(),
        Err(error) => format!(". {}", error.message),
    }
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
        DeliveryRung::Merge | DeliveryRung::Jira | DeliveryRung::Sync | DeliveryRung::PrReply => {}
    }
    Ok(())
}
