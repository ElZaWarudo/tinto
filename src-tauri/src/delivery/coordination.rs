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
    /// `merge` only: `squash`, `merge` or `rebase`.
    pub merge_method: Option<String>,
    /// `jira` only: `after_pr` or `after_merge`; the next one missing when
    /// `None`.
    pub stage: Option<String>,
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
        commits: Vec::new(),
        left_out: Vec::new(),
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
    /// task has not done (`after_pr`, then `after_merge`).
    fn jira_stage(&self, task_id: &str, asked: Option<&str>) -> Result<String, DeliveryError> {
        if let Some(asked) = asked {
            return match asked {
                JIRA_AFTER_PR | JIRA_AFTER_MERGE => Ok(asked.to_string()),
                other => Err(DeliveryError::new(
                    "invalid_approval",
                    format!("momento de Jira desconocido: {other} (after_pr o after_merge)"),
                )),
            };
        }
        let done_after_pr = self.store()?.approvals()?.iter().any(|approval| {
            approval.task_id == task_id
                && approval.rung == DeliveryRung::Jira
                && approval.stage.as_deref() == Some(JIRA_AFTER_PR)
                && approval.status == DeliveryApprovalStatus::Executed
        });
        Ok(if done_after_pr {
            JIRA_AFTER_MERGE
        } else {
            JIRA_AFTER_PR
        }
        .to_string())
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
        let stage = match request.rung {
            DeliveryRung::Jira => Some(self.jira_stage(&task.id, request.stage.as_deref())?),
            _ => None,
        };
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
            commits,
            left_out,
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
        if matches!(approval.rung, DeliveryRung::Commit | DeliveryRung::Push) {
            let task = self.task(&approval.task_id)?;
            let place = Place::of(task.distro.as_deref());
            let executed = match approval.rung {
                DeliveryRung::Commit => run_commit_plan(&task, &mut approval),
                _ if self.commits_lost(&task)? => Err(DeliveryError::new(
                    "commit_missing",
                    "No se publicó: los commits que hizo Tinto ya no están en la rama. Vuelve a preparar el commit.",
                )),
                _ => tasks::push_branch(place, &task.worktree, &task.branch).map(|upstream| {
                    approval.tip = tasks::head(place, &task.worktree).ok();
                    format!("Publicado en {upstream}")
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
        // Read only once the step is saved as done, so the read does not
        // take Tinto's own merge for one made outside it.
        if approval.rung == DeliveryRung::Merge {
            let _ = self.refresh_pr(&approval.task_id, std::time::Duration::ZERO);
        }
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
        if matches!(approval.rung, DeliveryRung::Commit | DeliveryRung::Push) {
            return refuse(
                "el commit y el push los comprueba Tinto con git; no se corrigen a mano",
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
        let blocker = if pr.state != "open" {
            Some(format!(
                "el PR #{} no está abierto ({})",
                pr.number, pr.state
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
        // Checked against the followed PR, which needs the service.
        DeliveryRung::Merge | DeliveryRung::Jira => {}
    }
    Ok(())
}
