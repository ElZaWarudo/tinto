//! Where each task is, derived from what Tinto records about it (jobs,
//! approvals, decisions, resources, its PR) and never stored: one state
//! machine every surface reads, so the view, the notifications and the
//! coordinator agree on what a task waits for and what comes next.
//!
//! The phase is the most urgent fact about the task, in this order: what
//! waits on the user, what runs, what waits its turn, what broke, then the
//! delivery ladder and the next stage. Each phase says who it waits on.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use super::coordination::{JIRA_AFTER_MERGE, JIRA_AFTER_PR};
use super::model::{
    DeliveryAgent, DeliveryApproval, DeliveryApprovalStatus, DeliveryBlocker, DeliveryBlockerKind,
    DeliveryDecision, DeliveryDecisionStatus, DeliveryIssuePackage, DeliveryJob, DeliveryJobStatus,
    DeliveryLease, DeliveryLeaseState, DeliveryQueueWait, DeliveryResultState, DeliveryRound,
    DeliveryRun, DeliveryRung, DeliverySettingsProposal, DeliverySplitIssue, DeliveryTask,
};
use super::prs::{CiState, DeliveryPr};
use super::service::{CHECKS_ROLE, SETUP_ROLE};

/// The built-in stages, in flow order.
pub const STAGES: [&str; 4] = ["tests", "implementation", "review", "qa"];

/// The delivery ladder: Jira comes twice, once the PR is open and once merged.
pub const STEPS: [(DeliveryRung, Option<&str>); 6] = [
    (DeliveryRung::Commit, None),
    (DeliveryRung::Push, None),
    (DeliveryRung::Pr, None),
    (DeliveryRung::Jira, Some(JIRA_AFTER_PR)),
    (DeliveryRung::Merge, None),
    (DeliveryRung::Jira, Some(JIRA_AFTER_MERGE)),
];

/// Who has to act for the task to move.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Waiter {
    User,
    Coordinator,
    /// An agent is working on it.
    Agent,
    /// Its job waits for capacity or an exclusive resource.
    Queue,
    /// People review its PR.
    Reviewers,
    /// Another task or a decision in Tinto it is blocked by.
    Dependency,
    /// Something outside Tinto it is blocked by: someone else's PR, a note.
    External,
    Nobody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Phase {
    /// The user answers its decisions.
    Decisions {
        count: usize,
    },
    /// The user approves a delivery step.
    Approval {
        rung: DeliveryRung,
        stage: Option<String>,
    },
    /// An approved PR or Jira step the coordinator still has to report.
    Record {
        rung: DeliveryRung,
        stage: Option<String>,
    },
    /// A done step git or GitHub no longer bear out.
    Drifted {
        rung: DeliveryRung,
        stage: Option<String>,
    },
    Running {
        job_id: String,
        role: String,
    },
    /// Paused by the user; it holds the task until resumed or cancelled.
    Paused {
        job_id: String,
        role: String,
    },
    /// Its job waits because the user paused the batch.
    BatchPaused {
        job_id: String,
        role: String,
    },
    /// Its job waits for a resource left in quarantine.
    WaitsQuarantine {
        job_id: String,
        lease: String,
    },
    /// One of its setup commands failed: no stage starts until it is
    /// retried.
    SetupFailed {
        job_id: String,
        command: String,
    },
    /// Its job needs the repo's environment, which does not answer: it
    /// starts once Tinto's periodic "Entorno" check passes.
    WaitsEnvironment {
        job_id: String,
        role: String,
        problem: String,
    },
    Queued {
        job_id: String,
        role: String,
        lease: Option<String>,
        /// Its place in the resource's queue, or in the capacity queue.
        position: usize,
        /// Why it has not started, by the scheduler's rules.
        reason: Option<String>,
        since_ms: Option<u64>,
    },
    /// A job of this task left the resource in quarantine.
    Quarantined {
        lease: String,
    },
    /// The settings change this task asked for waits on the user.
    SettingsProposal,
    /// Blocked until its open blockers clear (see the task's `blockers`).
    Blocked {
        open: usize,
    },
    /// Its last blocker cleared (by the user, or it resolved) and the
    /// coordinator has not acted on it since.
    Unblocked {
        cleared: String,
        /// `user` or `tinto`.
        by: String,
        at_ms: u64,
    },
    /// Its latest stage failed or was blocked.
    Broken {
        job_id: String,
        role: String,
        attempt: u32,
        /// Ended with a `blocked` result rather than failing.
        blocked: bool,
    },
    Delivered,
    /// Its PR is followed on GitHub, with the CI of its head.
    Pr {
        number: u64,
        ci: Option<CiState>,
    },
    /// A sync's merge is in progress and its conflicts are resolved (or
    /// work waits on it): the commit that concludes it comes before any
    /// other stage. `waiting_round` is the round waiting for it, if any.
    WaitsMergeCommit {
        waiting_round: Option<u32>,
    },
    /// Stages of these rounds changed the code while a sync's merge was
    /// in progress: the user decides how to commit it.
    MergeMixed {
        mixed_rounds: Vec<u32>,
    },
    /// Its PR was merged on GitHub with the CI of its head not green; a
    /// round fixes it on the base.
    MergedFailingCi {
        number: u64,
        why: String,
    },
    NextStage {
        role: String,
    },
    NextStep {
        rung: DeliveryRung,
        stage: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryTaskPhase {
    #[serde(flatten)]
    pub phase: Phase,
    pub waits_on: Waiter,
    /// The stage to run next, when one is due (whatever the phase).
    pub next_stage: Option<String>,
    /// The delivery steps this task climbs, in order, when they are not the
    /// whole ladder (a middle package of a split issue); empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<DeliveryStep>,
    /// What one of its queued jobs waits for looks stuck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// The round the task is in, while work sent back on it is not done
    /// again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<CurrentRound>,
}

/// A task's open round and where it is in it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurrentRound {
    #[serde(flatten)]
    pub round: DeliveryRound,
    /// The stage (`implementation`…) or delivery step (`commit`, `push`)
    /// the round is at; empty when it waits on something else.
    pub at: String,
}

/// One rung of a task's ladder (Jira with its moment).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryStep {
    pub rung: DeliveryRung,
    pub stage: Option<String>,
}

/// Everything the phase of a task depends on, as the overview has it.
pub struct Facts<'a> {
    pub runs: &'a [DeliveryRun],
    /// Every job, oldest first: queue positions are across tasks.
    pub jobs: &'a [DeliveryJob],
    pub leases: &'a [DeliveryLease],
    pub approvals: &'a [DeliveryApproval],
    pub decisions: &'a [DeliveryDecision],
    pub settings_proposals: &'a [DeliverySettingsProposal],
    pub prs: &'a [DeliveryPr],
    pub rounds: &'a [DeliveryRound],
    /// Why each queued job has not started.
    pub queued: &'a BTreeMap<String, DeliveryQueueWait>,
    /// Tasks whose worktree has a merge in progress, as git says.
    pub merging: &'a HashSet<String>,
    /// Batches whose coordinator may release a quarantined resource:
    /// nothing of theirs waits on the user's answer.
    pub releasable_runs: &'a HashSet<String>,
}

fn is_open(job: &DeliveryJob) -> bool {
    job.status.is_open()
}

/// Neither stale nor undone.
fn is_live(job: &DeliveryJob) -> bool {
    job.result_state != Some(DeliveryResultState::Stale) && job.undone_at_ms.is_none()
}

fn is_current(job: &DeliveryJob) -> bool {
    is_live(job) && job.result_state != Some(DeliveryResultState::Invalid)
}

fn result_is(job: &DeliveryJob, status: &str) -> bool {
    job.result
        .as_ref()
        .is_some_and(|result| result.status == status)
}

/// Settled without running it again: its work was done another way.
fn is_resolved(job: &DeliveryJob) -> bool {
    job.resolved_at_ms.is_some()
}

fn is_passed(job: &DeliveryJob) -> bool {
    (is_resolved(job) && is_live(job))
        || (job.status == DeliveryJobStatus::Finished && is_current(job) && result_is(job, "pass"))
}

/// Ended without a usable result, and not settled since.
pub fn is_broken(job: &DeliveryJob) -> bool {
    !is_resolved(job)
        && (matches!(
            job.status,
            DeliveryJobStatus::Failed | DeliveryJobStatus::Interrupted
        ) || job.result_state == Some(DeliveryResultState::Invalid)
            || (job.status == DeliveryJobStatus::Finished && result_is(job, "blocked")))
}

fn stage_order(role: &str) -> Option<usize> {
    STAGES.iter().position(|stage| *stage == role)
}

/// A result later work on an earlier stage left behind: new tests after
/// the implementation, new code after its review or QA.
fn is_superseded(job: &DeliveryJob, jobs: &[&DeliveryJob]) -> bool {
    let Some(order) = stage_order(&job.role).filter(|order| *order > 0) else {
        return false;
    };
    let Some(at) = jobs.iter().position(|other| other.id == job.id) else {
        return false;
    };
    jobs[at + 1..].iter().any(|later| {
        stage_order(&later.role).is_some_and(|later_order| later_order < order)
            && later.undone_at_ms.is_none()
            && !later.changes.is_empty()
    })
}

/// Findings a later stage answered: one after it passed (the tests' failing
/// cases, made to pass by the implementation).
pub fn is_answered(job: &DeliveryJob, jobs: &[&DeliveryJob]) -> bool {
    job.status == DeliveryJobStatus::Finished
        && result_is(job, "findings")
        && jobs.iter().any(|later| {
            later.created_at_ms > job.created_at_ms
                && stage_order(&later.role) > stage_order(&job.role)
                && is_passed(later)
        })
}

/// Ended with findings nobody has accepted (see `resolve_job`).
pub fn has_open_findings(job: &DeliveryJob) -> bool {
    job.status == DeliveryJobStatus::Finished && result_is(job, "findings") && !is_resolved(job)
}

/// Why the task's code cannot be committed yet: its latest stage's findings
/// were neither answered by a later stage nor accepted, or one of Tinto's
/// checks fails on it.
pub fn commit_blocker(jobs: &[&DeliveryJob]) -> Option<String> {
    let agent_jobs: Vec<&DeliveryJob> = jobs
        .iter()
        .filter(|job| job.agent != DeliveryAgent::Shell && is_live(job))
        .copied()
        .collect();
    if let Some(latest) = agent_jobs.last() {
        if has_open_findings(latest) && !is_answered(latest, &agent_jobs) {
            let count = latest
                .result
                .as_ref()
                .map_or(0, |result| result.findings.len());
            return Some(format!(
                "{} terminó con {count} hallazgo{} sin atender",
                latest.role,
                if count == 1 { "" } else { "s" }
            ));
        }
    }
    let failed: Vec<String> = current_checks(jobs)
        .iter()
        .filter(|check| check.status == DeliveryJobStatus::Failed)
        .map(|check| format!("`{}`", check.prompt))
        .collect();
    (!failed.is_empty()).then(|| format!("fallan comprobaciones de Tinto: {}", failed.join(", ")))
}

/// The latest stage that changed the task's code.
pub fn last_change<'a>(jobs: &[&'a DeliveryJob]) -> Option<&'a DeliveryJob> {
    jobs.iter()
        .rev()
        .find(|job| job.agent != DeliveryAgent::Shell && is_live(job) && !job.changes.is_empty())
        .copied()
}

/// Tinto's checks of the code the latest change left: the latest run of
/// each check since that change, oldest first.
pub fn current_checks<'a>(jobs: &[&'a DeliveryJob]) -> Vec<&'a DeliveryJob> {
    let since = last_change(jobs).map_or(0, |job| job.created_at_ms);
    let mut latest: Vec<&DeliveryJob> = Vec::new();
    for job in jobs.iter().filter(|job| {
        job.agent == DeliveryAgent::Shell && job.role == CHECKS_ROLE && job.created_at_ms > since
    }) {
        match latest.iter_mut().find(|other| other.prompt == job.prompt) {
            Some(other) => *other = job,
            None => latest.push(job),
        }
    }
    latest
}

/// The sync whose merge is still in progress: it stopped on conflicts and
/// no commit concluded it since. `None` once git has no merge in progress
/// (`in_progress` false), whoever concluded it.
pub fn open_merge<'a>(
    approvals: &[&'a DeliveryApproval],
    in_progress: bool,
) -> Option<&'a DeliveryApproval> {
    if !in_progress {
        return None;
    }
    let sync = latest_step(approvals, DeliveryRung::Sync, None).filter(|sync| {
        sync.status == DeliveryApprovalStatus::Executed && !sync.conflicts.is_empty()
    })?;
    let at = sync.executed_at_ms.unwrap_or(0);
    // Concluded by a commit step, or outside Tinto (the sync then records
    // the commit that did).
    let concluded = !sync.created.is_empty()
        || approvals.iter().any(|approval| {
            approval.rung == DeliveryRung::Commit
                && approval.status == DeliveryApprovalStatus::Executed
                && approval.executed_at_ms.unwrap_or(0) > at
        });
    (!concluded).then_some(sync)
}

/// The stages that resolve a sync's conflicts: implementation, then its
/// review (Tinto's checks run between them).
pub const MERGE_STAGES: [&str; 2] = ["implementation", "review"];

/// Whether a round belongs to resolving the merge of `sync`: the sync's own
/// round, or findings and failing checks of the stages run since it.
pub fn resolves_merge(
    round: &DeliveryRound,
    sync: &DeliveryApproval,
    jobs: &[&DeliveryJob],
) -> bool {
    let at = sync.executed_at_ms.unwrap_or(0);
    (round.kind == "sync" && round.source.as_deref() == Some(sync.id.as_str()))
        || (matches!(round.kind.as_str(), "review" | "checks")
            && jobs.iter().any(|job| {
                Some(job.id.as_str()) == round.source.as_deref() && job.created_at_ms > at
            }))
}

/// The merge's conflicts are resolved and verified: since the sync an
/// implementation passed, every one of the task's checks passed on it and
/// a review after it passed, with nothing running.
pub fn merge_resolved(checks: &[String], jobs: &[&DeliveryJob], sync: &DeliveryApproval) -> bool {
    let at = sync.executed_at_ms.unwrap_or(0);
    if jobs.iter().any(|job| is_open(job)) {
        return false;
    }
    let agent: Vec<&DeliveryJob> = jobs
        .iter()
        .filter(|job| job.agent != DeliveryAgent::Shell && is_live(job) && job.created_at_ms > at)
        .copied()
        .collect();
    let Some(fix) = agent.iter().rposition(|job| job.role == "implementation") else {
        return false;
    };
    if !is_passed(agent[fix]) {
        return false;
    }
    let reviewed = agent[fix + 1..]
        .last()
        .is_some_and(|job| job.role == "review" && is_passed(job));
    let current = current_checks(jobs);
    let checked = checks.iter().all(|command| {
        current
            .iter()
            .any(|check| &check.prompt == command && check.status == DeliveryJobStatus::Finished)
    });
    reviewed && checked
}

/// Rounds that are not part of the merge but whose stages changed the code
/// while it was in progress (from before Tinto kept them apart): their
/// changes can only be committed together with the merge now.
pub fn mixed_into_merge<'a>(
    rounds: &[&'a DeliveryRound],
    jobs: &[&DeliveryJob],
    sync: &DeliveryApproval,
) -> Vec<&'a DeliveryRound> {
    let at = sync.executed_at_ms.unwrap_or(0);
    rounds
        .iter()
        // A round waiting for the merge commit had no stage of its own.
        .filter(|round| {
            round.opened_at_ms >= at
                && !round.waits_merge_commit
                && !resolves_merge(round, sync, jobs)
        })
        .filter(|round| {
            jobs.iter().any(|job| {
                job.agent != DeliveryAgent::Shell
                    && is_live(job)
                    && !job.changes.is_empty()
                    && job.created_at_ms >= round.opened_at_ms
                    && round
                        .closed_at_ms
                        .is_none_or(|closed| job.created_at_ms <= closed)
            })
        })
        .copied()
        .collect()
}

/// The latest run of each of the task's setup commands, in its order. A
/// command that never ran has none.
pub fn current_setup<'a>(setup: &[String], jobs: &[&'a DeliveryJob]) -> Vec<&'a DeliveryJob> {
    setup
        .iter()
        .filter_map(|command| {
            jobs.iter()
                .rev()
                .find(|job| {
                    job.agent == DeliveryAgent::Shell
                        && job.role == SETUP_ROLE
                        && &job.prompt == command
                })
                .copied()
        })
        .collect()
}

/// The paused job no later job resumes.
fn paused_job<'a>(jobs: &[&'a DeliveryJob]) -> Option<&'a DeliveryJob> {
    let resumed: HashSet<&str> = jobs
        .iter()
        .filter_map(|job| job.resumes.as_deref())
        .collect();
    jobs.iter()
        .find(|job| job.status == DeliveryJobStatus::Paused && !resumed.contains(job.id.as_str()))
        .copied()
}

fn step_matches(approval: &DeliveryApproval, rung: DeliveryRung, stage: Option<&str>) -> bool {
    approval.rung == rung
        && match stage {
            None => true,
            Some(stage) => approval.stage.as_deref().unwrap_or(JIRA_AFTER_PR) == stage,
        }
}

fn latest_step<'a>(
    approvals: &[&'a DeliveryApproval],
    rung: DeliveryRung,
    stage: Option<&str>,
) -> Option<&'a DeliveryApproval> {
    approvals
        .iter()
        .rev()
        .find(|approval| step_matches(approval, rung, stage))
        .copied()
}

/// Whether the step was done: its latest request executed.
pub fn step_done(approvals: &[&DeliveryApproval], rung: DeliveryRung) -> bool {
    latest_step(approvals, rung, None)
        .is_some_and(|step| step.status == DeliveryApprovalStatus::Executed)
}

/// The ladder the task climbs. A middle package of a split issue ends at
/// its merge: the issue closes in Jira with its last package.
pub fn steps_for(task: &DeliveryTask) -> Vec<(DeliveryRung, Option<&'static str>)> {
    let middle = task
        .package
        .as_ref()
        .is_some_and(|package| !package.is_last());
    STEPS
        .into_iter()
        .filter(|(rung, stage)| {
            !(middle && *rung == DeliveryRung::Jira && *stage == Some(JIRA_AFTER_MERGE))
        })
        .collect()
}

/// Every step of the task's ladder done: the task is delivered.
pub fn is_delivered(task: &DeliveryTask, approvals: &[&DeliveryApproval]) -> bool {
    next_step(task, approvals).is_none()
}

/// The first ladder step not done; `None` once every step is.
fn next_step(
    task: &DeliveryTask,
    approvals: &[&DeliveryApproval],
) -> Option<(DeliveryRung, Option<&'static str>)> {
    steps_for(task).into_iter().find(|(rung, stage)| {
        latest_step(approvals, *rung, *stage)
            .is_none_or(|step| step.status != DeliveryApprovalStatus::Executed)
    })
}

fn stage_of(approval: &DeliveryApproval) -> Option<String> {
    (approval.rung == DeliveryRung::Jira).then(|| {
        approval
            .stage
            .clone()
            .unwrap_or_else(|| JIRA_AFTER_PR.into())
    })
}

/// The stage to run next: none while a stage or Tinto's checks run, wait
/// or are paused, while the task's setup has not passed, nor once the task
/// was committed after its last stage; implementation while a sync step's
/// merge waits on its conflicts; else retry a broken one, go back to
/// implementation after findings or a failed check, or the first built-in
/// stage without a passing (or answered) attempt that still stands.
pub fn next_stage(
    setup: &[String],
    jobs: &[&DeliveryJob],
    approvals: &[&DeliveryApproval],
) -> Option<String> {
    if paused_job(jobs).is_some() {
        return None;
    }
    if current_setup(setup, jobs)
        .iter()
        .any(|job| job.status != DeliveryJobStatus::Finished)
    {
        return None;
    }
    let agent_jobs: Vec<&DeliveryJob> = jobs
        .iter()
        .filter(|job| job.agent != DeliveryAgent::Shell && is_live(job))
        .copied()
        .collect();
    let checks = current_checks(jobs);
    if agent_jobs.iter().chain(&checks).any(|job| is_open(job)) {
        return None;
    }
    let latest = agent_jobs.last();
    let last_end = latest.and_then(|job| job.ended_at_ms).unwrap_or(0);
    let committed_after = |at: u64| {
        approvals.iter().any(|approval| {
            approval.rung == DeliveryRung::Commit
                && approval.status == DeliveryApprovalStatus::Executed
                && approval.executed_at_ms.unwrap_or(0) > at
        })
    };
    // A sync stopped on conflicts: an implementation job resolves them,
    // unless one already ran since or a commit concluded the merge.
    let conflicted = latest_step(approvals, DeliveryRung::Sync, None)
        .filter(|sync| {
            sync.status == DeliveryApprovalStatus::Executed && !sync.conflicts.is_empty()
        })
        .and_then(|sync| sync.executed_at_ms)
        .is_some_and(|at| at > last_end && !committed_after(at));
    if conflicted {
        return Some("implementation".into());
    }
    if committed_after(last_end) {
        return None;
    }
    if let Some(latest) = latest {
        if is_broken(latest) {
            return Some(latest.role.clone());
        }
        if has_open_findings(latest) {
            return Some("implementation".into());
        }
    }
    if checks
        .iter()
        .any(|job| job.status == DeliveryJobStatus::Failed)
    {
        return Some("implementation".into());
    }
    STAGES
        .iter()
        .find(|stage| {
            let job = agent_jobs
                .iter()
                .rev()
                .find(|job| job.role == **stage)
                .copied();
            match job {
                None => true,
                Some(job) => {
                    !(is_passed(job) || is_answered(job, &agent_jobs))
                        || is_superseded(job, &agent_jobs)
                }
            }
        })
        .map(|stage| stage.to_string())
}

/// Each issue delivered in packages, across the packages Tinto has of it:
/// the live tasks (with their phases) and the archived ones, all delivered.
pub fn split_issues(
    tasks: &[DeliveryTask],
    archived: &[DeliveryTask],
    phases: &std::collections::BTreeMap<String, DeliveryTaskPhase>,
    facts: &Facts<'_>,
) -> Vec<DeliverySplitIssue> {
    let mut issues: Vec<DeliverySplitIssue> = Vec::new();
    for (task, live) in tasks
        .iter()
        .map(|task| (task, true))
        .chain(archived.iter().map(|task| (task, false)))
    {
        let Some(package) = &task.package else {
            continue;
        };
        let repo = super::tasks::plain_path(&task.repo);
        let delivered = !live
            || phases
                .get(&task.id)
                .is_some_and(|phase| phase.phase == Phase::Delivered);
        let approvals: Vec<&DeliveryApproval> = facts
            .approvals
            .iter()
            .filter(|approval| approval.task_id == task.id)
            .collect();
        let merged = delivered
            || step_done(&approvals, DeliveryRung::Merge)
            || facts
                .prs
                .iter()
                .any(|pr| pr.task_id == task.id && pr.state == "merged");
        let entry = DeliveryIssuePackage {
            task_id: task.id.clone(),
            index: package.index,
            title: package.title.clone(),
            merged,
            delivered,
            archived: !live,
        };
        match issues
            .iter_mut()
            .find(|issue| issue.repo == repo && issue.key.eq_ignore_ascii_case(&task.key))
        {
            Some(issue) => issue.packages.push(entry),
            None => issues.push(DeliverySplitIssue {
                repo,
                key: task.key.clone(),
                of: package.of,
                packages: vec![entry],
                delivered: false,
            }),
        }
    }
    for issue in &mut issues {
        issue.packages.sort_by_key(|package| package.index);
        issue.delivered = (1..=issue.of).all(|index| {
            issue
                .packages
                .iter()
                .any(|package| package.index == index && package.delivered)
        });
    }
    // Only issues with something live: an all-archived one is history.
    issues.retain(|issue| issue.packages.iter().any(|package| !package.archived));
    issues
}

/// Who a followed PR waits on: the user when it has news or trouble, the
/// coordinator once approved or merged (the next step), else its reviewers.
/// Past the brainstorm the coordinator takes the news and trouble too.
fn pr_waits_on(pr: &DeliveryPr, unattended: bool) -> Waiter {
    let trouble = pr.state == "closed"
        || pr.new_reviews + pr.new_comments > 0
        || pr.checks.failing > 0
        || pr.conflicts
        || pr.review_decision.as_deref() == Some("changes_requested");
    // Every reviewer who must approve it has.
    let approved = pr.is_approved();
    if trouble && unattended {
        Waiter::Coordinator
    } else if trouble {
        Waiter::User
    } else if pr.state == "merged" || (approved && pr.checks.pending == 0 && !pr.draft) {
        Waiter::Coordinator
    } else {
        Waiter::Reviewers
    }
}

/// The delivery step a round still has to do again: its commit, then its
/// push when the branch was published before it opened.
pub fn round_step(round: &DeliveryRound, approvals: &[&DeliveryApproval]) -> Option<DeliveryRung> {
    let done_since = |rung: DeliveryRung| {
        approvals.iter().any(|approval| {
            approval.rung == rung
                && approval.status == DeliveryApprovalStatus::Executed
                && approval.executed_at_ms.unwrap_or(0) > round.opened_at_ms
        })
    };
    if !done_since(DeliveryRung::Commit) {
        Some(DeliveryRung::Commit)
    } else if round.after_push && !done_since(DeliveryRung::Push) {
        Some(DeliveryRung::Push)
    } else {
        None
    }
}

pub fn task_phase(task: &DeliveryTask, facts: &Facts<'_>) -> DeliveryTaskPhase {
    let open = facts
        .rounds
        .iter()
        .rev()
        .find(|round| round.task_id == task.id && round.is_current());
    let mut phase = phase_in_round(task, facts, open);
    phase.round = open.map(|round| CurrentRound {
        round: round.clone(),
        at: round_at(&phase),
    });
    phase
}

/// The stage or delivery step a phase is at.
fn round_at(phase: &DeliveryTaskPhase) -> String {
    match &phase.phase {
        Phase::Running { role, .. }
        | Phase::Paused { role, .. }
        | Phase::BatchPaused { role, .. }
        | Phase::WaitsEnvironment { role, .. }
        | Phase::Queued { role, .. }
        | Phase::Broken { role, .. }
        | Phase::NextStage { role } => role.clone(),
        Phase::Approval { rung, .. }
        | Phase::Record { rung, .. }
        | Phase::Drifted { rung, .. }
        | Phase::NextStep { rung, .. } => format!("{rung:?}").to_ascii_lowercase(),
        _ => phase.next_stage.clone().unwrap_or_default(),
    }
}

fn phase_in_round(
    task: &DeliveryTask,
    facts: &Facts<'_>,
    round: Option<&DeliveryRound>,
) -> DeliveryTaskPhase {
    let jobs: Vec<&DeliveryJob> = facts
        .jobs
        .iter()
        .filter(|job| job.task_id == task.id)
        .collect();
    // A reply to a reviewer is outside the ladder and holds nothing.
    let approvals: Vec<&DeliveryApproval> = facts
        .approvals
        .iter()
        .filter(|approval| approval.task_id == task.id && approval.rung != DeliveryRung::PrReply)
        .collect();
    // Past its run's brainstorm, what the coordinator can act on does not
    // wait for the user.
    let unattended = facts
        .runs
        .iter()
        .any(|run| Some(&run.id) == task.run_id.as_ref() && run.unattended);
    let merge = open_merge(&approvals, facts.merging.contains(&task.id));
    let mut next = next_stage(&task.setup, &jobs, &approvals);
    // While a merge is in progress only the stages resolving it run, and
    // none once they passed: the merge is committed first.
    if let Some(sync) = merge {
        if merge_resolved(&task.checks, &jobs, sync) {
            next = None;
        } else if next.as_deref().is_some_and(|role| role != "implementation") {
            next = Some("review".to_string());
        }
    }
    let resolving = merge.is_some() && next.is_some();
    let ladder = steps_for(task);
    let steps: Vec<DeliveryStep> = if ladder.len() == STEPS.len() {
        Vec::new()
    } else {
        ladder
            .into_iter()
            .map(|(rung, stage)| DeliveryStep {
                rung,
                stage: stage.map(str::to_string),
            })
            .collect()
    };
    let warning = jobs
        .iter()
        .find_map(|job| facts.queued.get(&job.id)?.warning.clone());
    let phase = |phase: Phase, waits_on: Waiter| DeliveryTaskPhase {
        phase,
        waits_on,
        next_stage: next.clone(),
        steps: steps.clone(),
        round: None,
        warning: warning.clone(),
    };

    let decisions = facts
        .decisions
        .iter()
        .filter(|decision| {
            decision.task_id == task.id && decision.status == DeliveryDecisionStatus::Pending
        })
        .count();
    if decisions > 0 {
        return phase(Phase::Decisions { count: decisions }, Waiter::User);
    }
    if let Some(step) = approvals
        .iter()
        .find(|approval| approval.status == DeliveryApprovalStatus::Pending)
    {
        return phase(
            Phase::Approval {
                rung: step.rung,
                stage: stage_of(step),
            },
            Waiter::User,
        );
    }
    if let Some(step) = approvals
        .iter()
        .find(|approval| approval.status == DeliveryApprovalStatus::Approved)
    {
        return phase(
            Phase::Record {
                rung: step.rung,
                stage: stage_of(step),
            },
            Waiter::Coordinator,
        );
    }
    if let Some(step) = approvals.iter().find(|approval| {
        approval.status == DeliveryApprovalStatus::Drifted
            && latest_step(&approvals, approval.rung, stage_of(approval).as_deref())
                .is_some_and(|latest| latest.id == approval.id)
    }) {
        // A push is put right by asking for it again, which the
        // coordinator does, and so are the PR and Jira steps past the
        // brainstorm; the other steps wait on the user.
        let waits_on = match step.rung {
            DeliveryRung::Push => Waiter::Coordinator,
            DeliveryRung::Pr | DeliveryRung::Jira if unattended => Waiter::Coordinator,
            _ => Waiter::User,
        };
        return phase(
            Phase::Drifted {
                rung: step.rung,
                stage: stage_of(step),
            },
            waits_on,
        );
    }
    if let Some(job) = jobs.iter().find(|job| {
        matches!(
            job.status,
            DeliveryJobStatus::Pending | DeliveryJobStatus::Started
        )
    }) {
        return phase(
            Phase::Running {
                job_id: job.id.clone(),
                role: job.role.clone(),
            },
            Waiter::Agent,
        );
    }
    if let Some(job) = paused_job(&jobs) {
        return phase(
            Phase::Paused {
                job_id: job.id.clone(),
                role: job.role.clone(),
            },
            Waiter::User,
        );
    }
    if let Some(job) = current_setup(&task.setup, &jobs)
        .into_iter()
        .find(|job| !job.status.is_open() && job.status != DeliveryJobStatus::Finished)
    {
        return phase(
            Phase::SetupFailed {
                job_id: job.id.clone(),
                command: job.prompt.clone(),
            },
            Waiter::Coordinator,
        );
    }
    if let Some(job) = jobs
        .iter()
        .find(|job| job.status == DeliveryJobStatus::Queued)
    {
        if let Some(problem) = &job.waits_environment {
            return phase(
                Phase::WaitsEnvironment {
                    job_id: job.id.clone(),
                    role: job.role.clone(),
                    problem: problem.clone(),
                },
                Waiter::User,
            );
        }
        let run_paused = facts
            .runs
            .iter()
            .any(|run| Some(&run.id) == task.run_id.as_ref() && run.paused);
        if run_paused {
            return phase(
                Phase::BatchPaused {
                    job_id: job.id.clone(),
                    role: job.role.clone(),
                },
                Waiter::User,
            );
        }
        let lease = facts
            .leases
            .iter()
            .find(|lease| lease.queue.iter().any(|waiter| waiter.job_id == job.id));
        // The resource it asks for, also while the scheduler holds it back
        // for something else first (capacity, its task, the environment).
        let wanted = lease.or_else(|| {
            let name = job.lease.as_deref()?;
            facts.leases.iter().find(|lease| lease.name == name)
        });
        // A later attempt of the stage that left it there takes it over.
        let takes_over = |lease: &DeliveryLease| {
            lease
                .holder_job_id
                .as_deref()
                .and_then(|holder| facts.jobs.iter().find(|other| other.id == holder))
                .is_some_and(|holder| holder.task_id == job.task_id && holder.role == job.role)
        };
        if let Some(lease) = wanted
            .filter(|lease| lease.state == DeliveryLeaseState::Quarantined && !takes_over(lease))
        {
            // Once nothing of the batch waits on the user, the coordinator
            // checks the environment and releases it.
            let releases = match task.run_id.as_ref() {
                Some(run) if facts.releasable_runs.contains(run) => Waiter::Coordinator,
                _ => Waiter::User,
            };
            return phase(
                Phase::WaitsQuarantine {
                    job_id: job.id.clone(),
                    lease: lease.name.clone(),
                },
                releases,
            );
        }
        let position = match lease {
            Some(lease) => {
                lease
                    .queue
                    .iter()
                    .position(|waiter| waiter.job_id == job.id)
                    .unwrap_or(0)
                    + 1
            }
            None => {
                let mut queue: Vec<&DeliveryJob> = facts
                    .jobs
                    .iter()
                    .filter(|other| other.status == DeliveryJobStatus::Queued)
                    .filter(|other| {
                        !facts
                            .leases
                            .iter()
                            .any(|lease| lease.queue.iter().any(|waiter| waiter.job_id == other.id))
                    })
                    .collect();
                queue.sort_by_key(|other| other.created_at_ms);
                queue
                    .iter()
                    .position(|other| other.id == job.id)
                    .map_or(0, |at| at + 1)
            }
        };
        let wait = facts.queued.get(&job.id);
        return phase(
            Phase::Queued {
                job_id: job.id.clone(),
                role: job.role.clone(),
                lease: lease.map(|lease| lease.name.clone()),
                position,
                reason: wait.map(|wait| wait.reason.clone()),
                since_ms: wait.map(|wait| wait.since_ms),
            },
            Waiter::Queue,
        );
    }
    if let Some(lease) = facts.leases.iter().find(|lease| {
        lease.state == DeliveryLeaseState::Quarantined
            && lease.holder_task_id.as_deref() == Some(task.id.as_str())
    }) {
        return phase(
            Phase::Quarantined {
                lease: lease.name.clone(),
            },
            Waiter::User,
        );
    }
    if facts
        .settings_proposals
        .iter()
        .any(|proposal| proposal.task_id == task.id)
    {
        return phase(Phase::SettingsProposal, Waiter::User);
    }
    let open: Vec<&DeliveryBlocker> = task.blockers.iter().filter(|b| b.is_open()).collect();
    if !open.is_empty() {
        let inside = open.iter().any(|blocker| {
            matches!(
                blocker.kind,
                DeliveryBlockerKind::Task { .. } | DeliveryBlockerKind::Decision { .. }
            )
        });
        return phase(
            Phase::Blocked { open: open.len() },
            if inside {
                Waiter::Dependency
            } else {
                Waiter::External
            },
        );
    }
    if let Some(unblock) = &task.unblocked {
        return phase(
            Phase::Unblocked {
                cleared: unblock.cleared.clone(),
                by: unblock.by.clone(),
                at_ms: unblock.at_ms,
            },
            Waiter::Coordinator,
        );
    }
    let latest = jobs
        .iter()
        .rev()
        .find(|job| job.agent != DeliveryAgent::Shell && is_live(job));
    if let Some(job) = latest.filter(|job| is_broken(job)) {
        return phase(
            Phase::Broken {
                job_id: job.id.clone(),
                role: job.role.clone(),
                attempt: job.attempt,
                blocked: job.status == DeliveryJobStatus::Finished,
            },
            Waiter::Coordinator,
        );
    }
    if let Some(sync) = merge {
        let task_rounds: Vec<&DeliveryRound> = facts
            .rounds
            .iter()
            .filter(|round| round.task_id == task.id)
            .collect();
        let mixed = mixed_into_merge(&task_rounds, &jobs, sync);
        if !mixed.is_empty() {
            return phase(
                Phase::MergeMixed {
                    mixed_rounds: mixed.iter().map(|round| round.number).collect(),
                },
                Waiter::User,
            );
        }
        if !resolving {
            let waiting = task_rounds
                .iter()
                .rev()
                .find(|round| round.is_open() && round.waits_merge_commit)
                .map(|round| round.number);
            return phase(
                Phase::WaitsMergeCommit {
                    waiting_round: waiting,
                },
                Waiter::Coordinator,
            );
        }
    }
    if let Some(pr) = facts
        .prs
        .iter()
        .find(|pr| pr.task_id == task.id && round.is_some_and(|round| round.kind == "ci"))
    {
        if let Some(why) = &pr.merged_failing_ci {
            return phase(
                Phase::MergedFailingCi {
                    number: pr.number,
                    why: why.clone(),
                },
                Waiter::Coordinator,
            );
        }
    }
    let step = next_step(task, &approvals);
    let Some((rung, stage)) = step else {
        return phase(Phase::Delivered, Waiter::Nobody);
    };
    // Work sent back comes before the PR's wait: the round's stages, then
    // its commit and push.
    if let Some(round) = round {
        if let Some(role) = next.clone() {
            return phase(Phase::NextStage { role }, Waiter::Coordinator);
        }
        if let Some(rung) = round_step(round, &approvals) {
            return phase(Phase::NextStep { rung, stage: None }, Waiter::Coordinator);
        }
    }
    if let Some(pr) = facts.prs.iter().find(|pr| pr.task_id == task.id) {
        return phase(
            Phase::Pr {
                number: pr.number,
                ci: pr.ci,
            },
            pr_waits_on(pr, unattended),
        );
    }
    if let Some(role) = next.clone() {
        return phase(Phase::NextStage { role }, Waiter::Coordinator);
    }
    phase(
        Phase::NextStep {
            rung,
            stage: stage.map(str::to_string),
        },
        Waiter::Coordinator,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::adapters::tests::{sample_job, sample_task};
    use crate::delivery::model::DeliveryJobResult;

    fn job(id: &str, role: &str, status: DeliveryJobStatus, result: Option<&str>) -> DeliveryJob {
        let mut job = sample_job(DeliveryAgent::Codex);
        job.id = id.into();
        job.role = role.into();
        job.status = status;
        job.created_at_ms = id.trim_start_matches('j').parse().unwrap_or(0);
        job.ended_at_ms = (!status.is_open()).then_some(job.created_at_ms * 1000 + 500);
        job.result_state = Some(DeliveryResultState::Accepted);
        job.result = result.map(|status| DeliveryJobResult {
            status: status.into(),
            summary: String::new(),
            changed_paths: Vec::new(),
            checks: Vec::new(),
            findings: vec!["x".into()],
            handoff: String::new(),
            blocked_by: String::new(),
        });
        job
    }

    fn step(rung: &str, status: &str, executed_at_ms: Option<u64>) -> DeliveryApproval {
        serde_json::from_value(serde_json::json!({
            "id": format!("a-{rung}-{status}"), "task_id": "t1", "rung": rung, "title": "",
            "body": "", "status": status, "requested_by": "coord", "requested_at_ms": 1,
            "decided_at_ms": null, "executed_at_ms": executed_at_ms, "outcome": null,
        }))
        .unwrap()
    }

    struct World {
        jobs: Vec<DeliveryJob>,
        approvals: Vec<DeliveryApproval>,
        decisions: Vec<DeliveryDecision>,
        leases: Vec<DeliveryLease>,
        runs: Vec<DeliveryRun>,
        prs: Vec<DeliveryPr>,
        proposals: Vec<DeliverySettingsProposal>,
        rounds: Vec<DeliveryRound>,
        merging: HashSet<String>,
        releasable_runs: HashSet<String>,
    }

    impl World {
        fn new(jobs: Vec<DeliveryJob>) -> Self {
            World {
                jobs,
                approvals: Vec::new(),
                decisions: Vec::new(),
                leases: Vec::new(),
                runs: Vec::new(),
                prs: Vec::new(),
                proposals: Vec::new(),
                rounds: Vec::new(),
                merging: HashSet::new(),
                releasable_runs: HashSet::new(),
            }
        }

        fn phase(&self) -> DeliveryTaskPhase {
            task_phase(
                &sample_task(),
                &Facts {
                    runs: &self.runs,
                    jobs: &self.jobs,
                    leases: &self.leases,
                    approvals: &self.approvals,
                    decisions: &self.decisions,
                    settings_proposals: &self.proposals,
                    prs: &self.prs,
                    rounds: &self.rounds,
                    queued: &BTreeMap::new(),
                    merging: &self.merging,
                    releasable_runs: &self.releasable_runs,
                },
            )
        }
    }

    use DeliveryJobStatus::{Finished, Paused, Queued, Started};

    #[test]
    fn a_new_task_waits_on_the_coordinator_for_its_tests() {
        let phase = World::new(Vec::new()).phase();
        assert_eq!(
            phase.phase,
            Phase::NextStage {
                role: "tests".into()
            }
        );
        assert_eq!(phase.waits_on, Waiter::Coordinator);
    }

    #[test]
    fn what_waits_on_the_user_comes_before_what_runs() {
        let mut world = World::new(vec![job("j1", "implementation", Started, None)]);
        world.approvals.push(step("commit", "pending", None));
        let phase = world.phase();
        assert_eq!(phase.waits_on, Waiter::User);
        assert!(matches!(
            phase.phase,
            Phase::Approval {
                rung: DeliveryRung::Commit,
                ..
            }
        ));
        world.approvals[0].status = DeliveryApprovalStatus::Approved;
        assert_eq!(
            world.phase().waits_on,
            Waiter::Coordinator,
            "a step to record"
        );
        world.approvals.clear();
        let running = world.phase();
        assert_eq!(running.waits_on, Waiter::Agent);
        assert_eq!(running.next_stage, None, "no stage while one runs");
    }

    #[test]
    fn a_running_stage_offers_no_other_even_with_earlier_findings() {
        let world = World::new(vec![
            job("j1", "tests", Finished, Some("findings")),
            job("j2", "implementation", Finished, Some("pass")),
            job("j3", "review", Started, None),
        ]);
        assert_eq!(world.phase().next_stage, None);
    }

    #[test]
    fn answered_findings_count_as_done_and_findings_send_back_to_implementation() {
        let answered = World::new(vec![
            job("j1", "tests", Finished, Some("findings")),
            job("j2", "implementation", Finished, Some("pass")),
            job("j3", "review", Finished, Some("pass")),
        ]);
        assert_eq!(
            answered.phase().phase,
            Phase::NextStage { role: "qa".into() }
        );
        let reviewed = World::new(vec![
            job("j1", "tests", Finished, Some("pass")),
            job("j2", "implementation", Finished, Some("pass")),
            job("j3", "review", Finished, Some("findings")),
        ]);
        assert_eq!(
            reviewed.phase().next_stage.as_deref(),
            Some("implementation")
        );
    }

    #[test]
    fn a_broken_stage_is_retried_by_the_coordinator() {
        let world = World::new(vec![job("j1", "tests", Finished, Some("blocked"))]);
        let phase = world.phase();
        assert_eq!(phase.waits_on, Waiter::Coordinator);
        assert!(matches!(phase.phase, Phase::Broken { blocked: true, .. }));
        assert_eq!(phase.next_stage.as_deref(), Some("tests"));
    }

    #[test]
    fn a_task_committed_after_its_stages_moves_on_the_ladder() {
        let mut world = World::new(vec![
            job("j1", "tests", Finished, Some("findings")),
            job("j2", "implementation", Finished, Some("pass")),
            job("j3", "review", Finished, Some("pass")),
        ]);
        world
            .approvals
            .push(step("commit", "executed", Some(10_000)));
        let phase = world.phase();
        assert_eq!(phase.next_stage, None);
        assert_eq!(
            phase.phase,
            Phase::NextStep {
                rung: DeliveryRung::Push,
                stage: None
            }
        );
    }

    #[test]
    fn pauses_and_queues_say_who_holds_them() {
        let paused = World::new(vec![job("j1", "implementation", Paused, None)]);
        assert_eq!(paused.phase().waits_on, Waiter::User);
        assert_eq!(paused.phase().next_stage, None);

        let mut queued = World::new(vec![job("j1", "qa", Queued, None)]);
        assert_eq!(queued.phase().waits_on, Waiter::Queue);
        let mut run: DeliveryRun = serde_json::from_value(serde_json::json!({
            "id": "r1", "repo": "/r", "title": "B", "status": "active", "generation": 1,
            "owner": null, "created_at_ms": 1, "updated_at_ms": 1, "paused": true,
        }))
        .unwrap();
        queued.runs.push(run.clone());
        let mut task = sample_task();
        task.run_id = Some("r1".into());
        let facts = Facts {
            runs: &queued.runs,
            jobs: &queued.jobs,
            leases: &[],
            approvals: &[],
            decisions: &[],
            settings_proposals: &[],
            prs: &[],
            rounds: &[],
            queued: &BTreeMap::new(),
            merging: &HashSet::new(),
            releasable_runs: &HashSet::new(),
        };
        assert!(matches!(
            task_phase(&task, &facts).phase,
            Phase::BatchPaused { .. }
        ));
        run.paused = false;
    }

    fn changed(mut job: DeliveryJob) -> DeliveryJob {
        job.changes = vec![crate::delivery::model::DeliveryChange {
            path: "src/app.ts".into(),
            kind: "modified".into(),
        }];
        job
    }

    #[test]
    fn stale_results_do_not_count_and_new_tests_ask_for_the_implementation_again() {
        let mut stale = job("j1", "tests", Finished, Some("pass"));
        stale.result_state = Some(DeliveryResultState::Stale);
        assert_eq!(
            World::new(vec![stale]).phase().next_stage.as_deref(),
            Some("tests")
        );
        let redone = World::new(vec![
            changed(job("j1", "tests", Finished, Some("pass"))),
            job("j2", "implementation", Finished, Some("pass")),
            changed(job("j3", "tests", Finished, Some("pass"))),
        ]);
        assert_eq!(redone.phase().next_stage.as_deref(), Some("implementation"));
    }

    #[test]
    fn new_code_asks_for_the_review_again_unless_it_was_undone() {
        let fix = changed(job("j4", "implementation", Finished, Some("pass")));
        let jobs = |fix: DeliveryJob| {
            vec![
                job("j1", "tests", Finished, Some("pass")),
                job("j2", "implementation", Finished, Some("pass")),
                job("j3", "review", Finished, Some("pass")),
                fix,
            ]
        };
        assert_eq!(
            World::new(jobs(fix.clone())).phase().next_stage.as_deref(),
            Some("review")
        );
        let mut undone = fix;
        undone.undone_at_ms = Some(5);
        assert_eq!(
            World::new(jobs(undone)).phase().next_stage.as_deref(),
            Some("qa")
        );
    }

    #[test]
    fn work_after_the_commit_brings_the_stages_back() {
        let mut world = World::new(vec![
            job("j1", "tests", Finished, Some("pass")),
            job("j2", "implementation", Finished, Some("pass")),
            job("j3", "review", Finished, Some("pass")),
        ]);
        world
            .approvals
            .push(step("commit", "executed", Some(4_000)));
        assert_eq!(world.phase().next_stage, None);
        let mut later = changed(job("j5", "implementation", Finished, Some("pass")));
        later.ended_at_ms = Some(9_000);
        world.jobs.push(later);
        assert_eq!(world.phase().next_stage.as_deref(), Some("review"));
    }

    #[test]
    fn every_stage_passed_leads_to_the_commit() {
        let world = World::new(vec![
            job("j1", "tests", Finished, Some("pass")),
            job("j2", "implementation", Finished, Some("pass")),
            job("j3", "review", Finished, Some("pass")),
            job("j4", "qa", Finished, Some("pass")),
        ]);
        let phase = world.phase();
        assert_eq!(phase.next_stage, None);
        assert_eq!(
            phase.phase,
            Phase::NextStep {
                rung: DeliveryRung::Commit,
                stage: None
            }
        );
    }

    #[test]
    fn a_failed_stage_is_broken_even_with_an_invalid_result() {
        let mut failed = job("j2", "implementation", DeliveryJobStatus::Failed, None);
        failed.attempt = 2;
        failed.result_state = Some(DeliveryResultState::Invalid);
        let phase = World::new(vec![job("j1", "tests", Finished, Some("pass")), failed]).phase();
        assert_eq!(
            phase.phase,
            Phase::Broken {
                job_id: "j2".into(),
                role: "implementation".into(),
                attempt: 2,
                blocked: false
            }
        );
    }

    fn proposal(task_id: &str) -> DeliverySettingsProposal {
        serde_json::from_value(serde_json::json!({
            "id": format!("p-{task_id}"), "repo": "/r", "task_id": task_id, "run_id": "r1",
            "reason": "", "changes": {}, "status": "pending", "requested_by": "coord",
            "requested_at_ms": 1, "decided_by": null, "decided_at_ms": null,
        }))
        .unwrap()
    }

    #[test]
    fn a_settings_proposal_holds_only_the_task_that_asked_once_its_agents_stop() {
        let mut world = World::new(Vec::new());
        world.proposals.push(proposal("t1"));
        assert_eq!(world.phase().phase, Phase::SettingsProposal);
        world.proposals = vec![proposal("t2")];
        assert_ne!(world.phase().phase, Phase::SettingsProposal);
        world.proposals = vec![proposal("t1")];
        world.jobs.push(job("j1", "qa", Started, None));
        assert_eq!(world.phase().waits_on, Waiter::Agent);
    }

    fn lease(state: DeliveryLeaseState, holder: Option<&str>, queue: &[&str]) -> DeliveryLease {
        let queue: Vec<serde_json::Value> = queue
            .iter()
            .map(|id| serde_json::json!({"job_id": id, "task_id": "t1", "requested_at_ms": 1}))
            .collect();
        serde_json::from_value(serde_json::json!({
            "name": "qa", "state": state, "generation": 1, "holder_job_id": "x",
            "holder_task_id": holder, "acquired_at_ms": 1, "note": null, "queue": queue,
        }))
        .unwrap()
    }

    #[test]
    fn queues_say_their_place_and_quarantine_holds_only_when_nothing_runs() {
        let mut waiting = job("j1", "qa", Queued, None);
        waiting.lease = Some("qa".into());
        let mut world = World::new(vec![waiting]);
        world
            .leases
            .push(lease(DeliveryLeaseState::Active, Some("t9"), &["j1"]));
        assert!(matches!(
            world.phase().phase,
            Phase::Queued { position: 1, .. }
        ));
        world.leases = vec![lease(DeliveryLeaseState::Quarantined, Some("t9"), &["j1"])];
        assert!(matches!(world.phase().phase, Phase::WaitsQuarantine { .. }));

        let mut ahead = job("j1", "tests", Queued, None);
        ahead.task_id = "t2".into();
        let capacity = World::new(vec![ahead, job("j2", "tests", Queued, None)]);
        assert!(matches!(
            capacity.phase().phase,
            Phase::Queued { position: 2, .. }
        ));

        let mut left = World::new(Vec::new());
        left.leases
            .push(lease(DeliveryLeaseState::Quarantined, Some("t1"), &[]));
        assert_eq!(
            left.phase().phase,
            Phase::Quarantined { lease: "qa".into() }
        );
        left.jobs.push(job("j1", "implementation", Started, None));
        assert_eq!(left.phase().waits_on, Waiter::Agent);
    }

    #[test]
    fn decisions_come_before_anything_else() {
        let mut world = World::new(Vec::new());
        world.approvals.push(step("commit", "pending", None));
        world.decisions.push(
            serde_json::from_value(serde_json::json!({
                "id": "d1", "task_id": "t1", "kind": "choice", "question": "?", "detail": "",
                "options": [], "text": "", "command": null, "undo": "", "status": "pending",
                "answer": null, "requested_by": "coord", "requested_at_ms": 1,
                "decided_by": null, "decided_at_ms": null,
            }))
            .unwrap(),
        );
        assert_eq!(world.phase().phase, Phase::Decisions { count: 1 });
        world.decisions[0].status = DeliveryDecisionStatus::Answered;
        world.approvals.clear();
        assert_eq!(
            world.phase().phase,
            Phase::NextStage {
                role: "tests".into()
            }
        );
    }

    fn pr(over: serde_json::Value) -> DeliveryPr {
        let mut value = serde_json::json!({
            "task_id": "t1", "url": "https://github.com/o/r/pull/5", "repo": "o/r",
            "number": 5, "state": "open", "draft": false, "conflicts": false,
            "review_decision": "review_required",
            "checks": {"passing": 2, "failing": 0, "pending": 0},
            "new_reviews": 0, "new_comments": 0, "new_authors": [], "latest_activity": null,
            "seen_until": null, "checked_at_ms": 1, "error": null,
        });
        value
            .as_object_mut()
            .unwrap()
            .extend(over.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_pr_waits_on_whoever_has_to_act_on_it() {
        let mut world = World::new(vec![job("j1", "qa", Finished, Some("pass"))]);
        for rung in ["commit", "push", "pr"] {
            world.approvals.push(step(rung, "executed", Some(9_000)));
        }
        let mut waits = |over: serde_json::Value| {
            world.prs = vec![pr(over)];
            (world.phase().phase, world.phase().waits_on)
        };
        assert_eq!(
            waits(serde_json::json!({})),
            (
                Phase::Pr {
                    number: 5,
                    ci: None
                },
                Waiter::Reviewers
            )
        );
        assert_eq!(
            waits(serde_json::json!({"new_comments": 1})).1,
            Waiter::User
        );
        assert_eq!(
            waits(serde_json::json!({"conflicts": true})).1,
            Waiter::User
        );
        assert_eq!(
            waits(serde_json::json!({"review_decision": "approved"})).1,
            Waiter::Coordinator
        );
        assert_eq!(
            waits(serde_json::json!({
                "review_decision": "approved",
                "checks": {"passing": 1, "failing": 0, "pending": 1},
            }))
            .1,
            Waiter::Reviewers
        );
        assert_eq!(
            waits(serde_json::json!({"state": "merged"})).1,
            Waiter::Coordinator
        );
        // Past the brainstorm the coordinator takes news and trouble.
        for trouble in [
            serde_json::json!({"new_comments": 1}),
            serde_json::json!({"conflicts": true}),
            serde_json::json!({"review_decision": "changes_requested"}),
        ] {
            assert_eq!(pr_waits_on(&pr(trouble), true), Waiter::Coordinator);
        }
        assert_eq!(
            pr_waits_on(&pr(serde_json::json!({})), true),
            Waiter::Reviewers
        );
    }

    #[test]
    fn a_delivered_task_waits_on_nobody() {
        let mut world = World::new(vec![job("j1", "qa", Finished, Some("pass"))]);
        for (rung, stage) in STEPS {
            let mut done = step(&format!("{rung:?}").to_lowercase(), "executed", Some(1));
            done.stage = stage.map(str::to_string);
            done.id = format!("{rung:?}-{stage:?}");
            world.approvals.push(done);
        }
        let phase = world.phase();
        assert_eq!(phase.phase, Phase::Delivered);
        assert_eq!(phase.waits_on, Waiter::Nobody);
    }
}
