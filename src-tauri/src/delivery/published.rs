//! What waits for the fix to be on GitHub. AIRInstitute/agentos#373: ten
//! approved replies described fixes that were only in local commits, and
//! the review was asked again, while GitHub's PR head was still the commit
//! reviewed before (its green CI was that commit's). The reviewer saw the
//! same diff.
//!
//! So a reply that describes a fix (any but an interim one), with the
//! resolution of its thread, is posted only once the live PR head has the
//! branch's HEAD it was approved at; and a reviewer is asked to review
//! again only once the live head has every local commit, the replies are
//! posted, every thread has one and the checks passed on that head. Both
//! are checked again each time the PR is read from GitHub, so a restart, a
//! head moved on GitHub or a rewritten branch count as they are then, and
//! nothing goes out while GitHub cannot be read.

use super::model::{DeliveryApproval, DeliveryApprovalStatus, DeliveryRung, StepEvent};
use super::model::{DeliveryJob, DeliveryTask};
use super::prs::DeliveryPr;
use super::service::DeliveryService;
use super::tasks::{self, Place};
use super::{now_ms, DeliveryError};

/// How a reply approved before its fix is on GitHub reads in the panel.
pub const REPLY_HELD: &str = "Respuesta aprobada · espera a que se publique la corrección";

fn short(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// Whether `commit` is `head` or in its history, as the worktree knows
/// them: a head the worktree has never seen does not contain it.
fn contains(task: &DeliveryTask, head: &str, commit: &str) -> bool {
    let place = Place::of(task.distro.as_deref());
    head == commit
        || tasks::git(
            place,
            &task.worktree,
            &["merge-base", "--is-ancestor", commit, head],
        )
        .is_ok()
}

impl DeliveryService {
    /// Posts an approved reply once GitHub's PR head (`live_head`, read now
    /// when `None`) has its fix; until then it stays approved and its notice
    /// says what it waits for. Posted once, never retried: a failure says
    /// why, and the coordinator asks again if it still makes sense.
    pub(super) fn post_reply_once_published(
        &self,
        approval: &mut DeliveryApproval,
        live_head: Option<&str>,
    ) -> Result<(), DeliveryError> {
        if let Some(why) = self.reply_blocker(approval, live_head)? {
            approval.notice = Some(format!("{REPLY_HELD}: {why}"));
            return Ok(());
        }
        let posted = self.post_reply(approval);
        approval.executed_at_ms = Some(now_ms());
        match posted {
            Ok((url, notice)) => {
                approval.advance(StepEvent::Ran { ok: true })?;
                approval.outcome = Some(url);
                approval.notice = notice;
            }
            Err(error) => {
                approval.advance(StepEvent::Ran { ok: false })?;
                approval.outcome = Some(error.message);
                approval.notice = None;
            }
        }
        Ok(())
    }

    /// Why a reply cannot be posted yet, or `None`. Its fix is the branch's
    /// HEAD when it was first approved (kept in `head`); a branch rewritten
    /// since carries it in its new HEAD.
    fn reply_blocker(
        &self,
        approval: &mut DeliveryApproval,
        live_head: Option<&str>,
    ) -> Result<Option<String>, DeliveryError> {
        if approval.interim {
            return Ok(None);
        }
        let task = self.task(&approval.task_id)?;
        let place = Place::of(task.distro.as_deref());
        let fix = match approval.head.clone() {
            Some(fix) if contains(&task, "HEAD", &fix) => fix,
            _ => tasks::head(place, &task.worktree)?,
        };
        approval.head = Some(fix.clone());
        let live = match live_head {
            Some(head) => head.to_string(),
            None => match self.live_head(&task, approval) {
                Ok(head) => head,
                Err(error) => {
                    return Ok(Some(format!(
                        "no se pudo leer el PR en GitHub ({}): se comprueba otra vez en la próxima lectura",
                        error.message
                    )))
                }
            },
        };
        if live.is_empty() {
            return Ok(Some("GitHub no dice qué commit tiene el PR".to_string()));
        }
        if !contains(&task, &live, &fix) {
            return Ok(Some(format!(
                "el PR en GitHub está en {} y la corrección en {}{}",
                short(&live),
                short(&fix),
                self.unpublished_because(&task)
            )));
        }
        let path = approval.thread.as_ref().map(|thread| thread.path.as_str());
        if let Some(path) = path.filter(|path| !path.is_empty()) {
            let status = tasks::git(
                place,
                &task.worktree,
                &["status", "--porcelain", "--", path],
            )
            .unwrap_or_default();
            if !status.is_empty() {
                return Ok(Some(format!("{path} tiene cambios sin commit")));
            }
        }
        Ok(None)
    }

    /// The head commit of the PR a reply is on, as GitHub has it now.
    fn live_head(
        &self,
        task: &DeliveryTask,
        approval: &DeliveryApproval,
    ) -> Result<String, DeliveryError> {
        let url = match self.task_pr(&task.id)? {
            Some(pr) => pr.url,
            None => approval
                .comment_url
                .as_deref()
                .and_then(super::prs::parse_pr_url)
                .map(|pr| pr.url)
                .ok_or_else(|| {
                    DeliveryError::new("invalid_approval", "la respuesta no tiene PR")
                })?,
        };
        let snapshot = self.github().pr(&url)?;
        Ok(snapshot["headRefOid"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Why the branch is not published yet, when Tinto knows, as a clause
    /// for a blocker: a merge or restack in progress, the stages' findings
    /// or Tinto's checks failing.
    fn unpublished_because(&self, task: &DeliveryTask) -> String {
        let why = match self.check_base_merged(task) {
            Err(error) => Some(error.message),
            Ok(()) => self
                .store()
                .and_then(|store| store.task_jobs(&task.id))
                .ok()
                .and_then(|jobs| {
                    let jobs: Vec<&DeliveryJob> = jobs.iter().collect();
                    super::phase::commit_blocker(&task.checks, &jobs)
                }),
        };
        why.map(|why| format!("; no se publica todavía: {why}"))
            .unwrap_or_default()
    }

    /// A posted reply answers its reviewer.
    pub(super) fn reply_answered(&self, approval: &DeliveryApproval) -> Result<(), DeliveryError> {
        if approval.status != DeliveryApprovalStatus::Executed {
            return Ok(());
        }
        let Some(reviewer) = approval.reply_to() else {
            return Ok(());
        };
        let store = self.store()?;
        if let Some(mut pr) = store.pr(&approval.task_id)? {
            super::reviews::apply(
                &mut pr.review_states,
                reviewer,
                super::reviews::ReviewEvent::Answered,
            );
            store.put_pr(&pr)?;
        }
        Ok(())
    }

    /// Once the task's PR was read from GitHub (`live`, its answer, already
    /// on the stored PR): posts the replies held for their fix, then asks
    /// the reviewers held to review again. Runs under the PR's read lock,
    /// so two reads never post the same reply.
    pub(super) fn publish_held(
        &self,
        task_id: &str,
        live: &serde_json::Value,
    ) -> Result<(), DeliveryError> {
        let live_head = live["headRefOid"].as_str().unwrap_or_default();
        let is_held = |approval: &DeliveryApproval| {
            approval.task_id == task_id
                && approval.rung == DeliveryRung::PrReply
                && approval.status == DeliveryApprovalStatus::Approved
        };
        let held: Vec<String> = self
            .store()?
            .approvals()?
            .into_iter()
            .filter(is_held)
            .map(|approval| approval.id)
            .collect();
        // A few at a time; one failing does not stop the others, and the
        // review is asked again only after all of them.
        let posted = super::prs::at_once(&held, |id| {
            // Each reply's own claim, as a decision takes it, read again
            // under it: a reply being posted from the user's approve, or
            // withdrawn meanwhile, is not posted here as well. No lock is
            // held across the batch.
            let lock = super::service::keyed_lock(&self.inner.deciding, id);
            let _deciding = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(mut approval) = self.store()?.approval(id)?.filter(is_held) else {
                return Ok(());
            };
            let before = approval.clone();
            self.post_reply_once_published(&mut approval, Some(live_head))?;
            if approval != before {
                self.save_approval(&approval, "pr_reply_published")?;
                self.reply_answered(&approval)?;
            }
            Ok::<(), DeliveryError>(())
        });
        posted.into_iter().collect::<Result<(), _>>()?;
        self.request_held_reviews(&self.task(task_id)?)
    }

    /// Asks the reviewers held in `review_again` once nothing stops it,
    /// else keeps why on the PR.
    fn request_held_reviews(&self, task: &DeliveryTask) -> Result<(), DeliveryError> {
        let Some(pr) = self.task_pr(&task.id)? else {
            return Ok(());
        };
        if pr.review_again.is_empty() {
            return Ok(());
        }
        let blocker = match self.review_blocker(task, &pr)? {
            Some(why) => Some(why),
            None => self
                .github()
                .request_reviewers(&pr.url, &pr.review_again)
                .err()
                .map(|error| format!("GitHub no aceptó la petición: {}", error.message)),
        };
        let store = self.store()?;
        let Some(mut stored) = store.pr(&task.id)? else {
            return Ok(());
        };
        if blocker.is_none() {
            let asked = |login: &String| {
                pr.review_again
                    .iter()
                    .any(|held| held.eq_ignore_ascii_case(login))
            };
            stored.review_again.retain(|login| !asked(login));
            stored.add_reviewers(&pr.review_again);
        }
        stored.review_again_blocker = blocker;
        store.put_pr(&stored)?;
        drop(store);
        self.notify();
        Ok(())
    }

    /// Why the review cannot be asked again on `pr` (as GitHub last showed
    /// it), or `None`: the PR's head on GitHub has every local commit, with
    /// no merge left in progress, the task's replies are all posted, every
    /// thread has one, and the checks passed on that head, GitHub's and
    /// Tinto's.
    fn review_blocker(
        &self,
        task: &DeliveryTask,
        pr: &DeliveryPr,
    ) -> Result<Option<String>, DeliveryError> {
        let place = Place::of(task.distro.as_deref());
        let Some(live) = pr.head.clone().filter(|head| !head.is_empty()) else {
            return Ok(Some("GitHub no dice qué commit tiene el PR".to_string()));
        };
        let local = tasks::head(place, &task.worktree)?;
        if tasks::sync_in_progress(place, &task.worktree) || !contains(task, &live, &local) {
            return Ok(Some(format!(
                "el PR en GitHub está en {} y la rama local en {}{}",
                short(&live),
                short(&local),
                self.unpublished_because(task)
            )));
        }
        let unposted = self
            .store()?
            .approvals()?
            .iter()
            .filter(|approval| {
                approval.task_id == task.id
                    && approval.rung == DeliveryRung::PrReply
                    && matches!(
                        approval.status,
                        DeliveryApprovalStatus::Pending | DeliveryApprovalStatus::Approved
                    )
            })
            .count();
        if unposted > 0 {
            return Ok(Some(format!(
                "{unposted} respuesta(s) a la revisión sin publicar"
            )));
        }
        let open = pr.unanswered().count();
        if open > 0 {
            return Ok(Some(format!("{open} hilo(s) de revisión sin respuesta")));
        }
        if let Some(why) = self.ci_refusal(task, pr) {
            return Ok(Some(why));
        }
        let jobs = self.store()?.task_jobs(&task.id)?;
        let jobs: Vec<&DeliveryJob> = jobs.iter().collect();
        let failing = super::phase::failing_checks(&task.checks, &jobs);
        if !failing.is_empty() {
            return Ok(Some(format!(
                "fallan comprobaciones de Tinto: {}",
                failing.join(", ")
            )));
        }
        Ok(None)
    }
}
