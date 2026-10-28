//! Rounds: each time something sends a task's work back (review or QA
//! findings, QA problems the user sends to fix, a failing check of Tinto,
//! changes asked on its PR, conflicts of a sync), a new round opens with its
//! cause and what it has to address, recorded as it happens. It closes once
//! that work is done again: committed (and pushed, when the branch was
//! published before), accepted with resolve_job, or found to need no change.
//! Round 1, the initial delivery, is implicit.

use super::model::{
    DeliveryAgent, DeliveryApproval, DeliveryApprovalStatus, DeliveryJob, DeliveryJobStatus,
    DeliveryQaProblem, DeliveryResultState, DeliveryRound, DeliveryRung, DeliveryTask,
};
use super::phase::{
    current_checks, has_open_findings, next_stage, open_merge, resolves_merge, round_step,
};
use super::prs::DeliveryPr;
use super::service::{CHECKS_ROLE, QA_ROLE};
use super::store::DeliveryStore;
use super::{now_ms, DeliveryError};

/// Items a job's instructions or an event list at most.
const MAX_ITEMS: usize = 20;
const ITEM_CHARS: usize = 300;

/// What opens a round.
pub(super) struct NewRound {
    pub kind: &'static str,
    pub short: String,
    pub cause: String,
    pub items: Vec<String>,
    /// The job or step it comes from: a round opens once per source.
    pub source: Option<String>,
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn line(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(ITEM_CHARS) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}

/// "Ronda 3 · Cambios pedidos en el PR #308 por ana: 1) …; 2) …".
pub(super) fn round_text(round: &DeliveryRound) -> String {
    let mut text = format!("Ronda {} · {}", round.number, round.cause);
    if !round.items.is_empty() {
        let items: Vec<String> = round
            .items
            .iter()
            .take(MAX_ITEMS)
            .enumerate()
            .map(|(at, item)| format!("{}) {item}", at + 1))
            .collect();
        text.push_str(&format!(": {}", items.join("; ")));
        if round.items.len() > MAX_ITEMS {
            text.push_str(&format!(" (y {} más)", round.items.len() - MAX_ITEMS));
        }
    }
    text
}

/// How a round that is done again ended, by what opened it.
fn done_outcome(kind: &str) -> &'static str {
    match kind {
        "checks" => "comprobaciones en verde",
        "sync" => "conflictos resueltos",
        "pr" => "cambios publicados",
        _ => "corregidos",
    }
}

fn stage_name(role: &str) -> &'static str {
    if role == QA_ROLE {
        "QA"
    } else {
        "revisión"
    }
}

/// Review or QA ended with findings: they send the work back.
fn findings_round(job: &DeliveryJob) -> Option<NewRound> {
    let sends_back = job.agent != DeliveryAgent::Shell
        && matches!(job.role.as_str(), "review" | QA_ROLE)
        && job.result_state == Some(DeliveryResultState::Accepted)
        && has_open_findings(job);
    if !sends_back {
        return None;
    }
    let findings = &job.result.as_ref()?.findings;
    let short = format!(
        "{} de {}",
        plural(findings.len(), "hallazgo", "hallazgos"),
        stage_name(&job.role)
    );
    Some(NewRound {
        kind: if job.role == QA_ROLE { "qa" } else { "review" },
        cause: short.clone(),
        short,
        items: findings.iter().map(|finding| line(finding)).collect(),
        source: Some(job.id.clone()),
    })
}

/// Every check of the task ended on the latest change and some failed.
fn checks_round(task: &DeliveryTask, job: &DeliveryJob, jobs: &[DeliveryJob]) -> Option<NewRound> {
    if job.agent != DeliveryAgent::Shell || job.role != CHECKS_ROLE {
        return None;
    }
    let jobs: Vec<&DeliveryJob> = jobs.iter().collect();
    let checks = current_checks(&jobs);
    let ended = task.checks.iter().all(|command| {
        checks
            .iter()
            .any(|check| &check.prompt == command && !check.status.is_open())
    });
    let failed: Vec<&&DeliveryJob> = checks
        .iter()
        .filter(|check| check.status == DeliveryJobStatus::Failed)
        .collect();
    if !ended || failed.is_empty() || checks.iter().any(|check| check.status.is_open()) {
        return None;
    }
    let names: Vec<String> = failed
        .iter()
        .map(|check| format!("`{}`", check.prompt))
        .collect();
    Some(NewRound {
        kind: "checks",
        short: "comprobaciones".to_string(),
        cause: format!("Fallan comprobaciones de Tinto: {}", names.join(", ")),
        items: failed
            .iter()
            .map(|check| {
                line(&format!(
                    "`{}`: {}",
                    check.prompt,
                    check.error.as_deref().unwrap_or("falló")
                ))
            })
            .collect(),
        // The last check to end: one round per run of the checks.
        source: Some(job.id.clone()),
    })
}

/// The PR's news a writing stage answers: changes asked, or others' new
/// reviews and comments.
fn pr_round(pr: &DeliveryPr, rounds: &[DeliveryRound]) -> Option<NewRound> {
    // An approval asks for no work: only what else reviewers said sends the
    // work back, and only that is addressed.
    let mut items: Vec<String> = pr
        .news
        .iter()
        .filter(|item| !item.ends_with(super::prs::APPROVED_NEWS))
        .map(|item| line(item))
        .collect();
    let news = !items.is_empty();
    // Changes asked since the latest round that answered the PR.
    let answered = rounds
        .iter()
        .filter(|round| round.kind == "pr")
        .map(|round| round.opened_at_ms)
        .max()
        .unwrap_or(0);
    let asked = pr.review_decision.as_deref() == Some("changes_requested")
        && pr.changes_requested_at_ms.is_some_and(|at| at > answered);
    if !pr.is_open() || !(news || asked) {
        return None;
    }
    let by = match pr.new_authors.as_slice() {
        [] => String::new(),
        authors => authors.join(", "),
    };
    let mut cause = match (pr.review_decision.as_deref(), by.is_empty()) {
        (Some("changes_requested"), true) => format!("Cambios pedidos en el PR #{}", pr.number),
        (Some("changes_requested"), false) => {
            format!("Cambios pedidos en el PR #{} por {by}", pr.number)
        }
        (_, true) => format!("Comentarios nuevos en el PR #{}", pr.number),
        (_, false) => format!("Comentarios nuevos en el PR #{} de {by}", pr.number),
    };
    if news {
        cause.push_str(&format!(
            " ({})",
            plural(items.len(), "comentario", "comentarios")
        ));
    } else {
        // Changes asked whose words Tinto no longer has (seen already).
        items.push(format!(
            "Lee en {} la revisión que pidió cambios y sus comentarios",
            pr.url
        ));
    }
    Some(NewRound {
        kind: "pr",
        short: "PR".to_string(),
        cause,
        items,
        source: None,
    })
}

fn latest_done(approvals: &[DeliveryApproval], rung: DeliveryRung) -> Option<&DeliveryApproval> {
    approvals
        .iter()
        .rev()
        .find(|approval| approval.rung == rung && approval.status.was_done())
}

fn task_approvals(
    store: &DeliveryStore,
    task_id: &str,
) -> Result<Vec<DeliveryApproval>, DeliveryError> {
    Ok(store
        .approvals()?
        .into_iter()
        .filter(|approval| approval.task_id == task_id)
        .collect())
}

pub(super) fn open_of(
    store: &DeliveryStore,
    task_id: &str,
) -> Result<Option<DeliveryRound>, DeliveryError> {
    Ok(store
        .task_rounds(task_id)?
        .into_iter()
        .rev()
        .find(DeliveryRound::is_current))
}

/// Opens a round on the task, unless its source already opened one. A
/// round still open that no stage worked on yet takes the new cause instead
/// when it is the same kind (QA problems refine QA's findings); otherwise it
/// gives way to the new round, which carries its items. Runs under the
/// caller's hold of the store, with what caused it.
pub(super) fn open_round(
    store: &DeliveryStore,
    task_id: &str,
    new: NewRound,
) -> Result<(), DeliveryError> {
    let rounds = store.task_rounds(task_id)?;
    if new.source.is_some() && rounds.iter().any(|round| round.source == new.source) {
        return Ok(());
    }
    let jobs = store.task_jobs(task_id)?;
    let now = now_ms();
    let number = rounds.last().map_or(2, |round| round.number + 1);
    let approvals = task_approvals(store, task_id)?;
    let approval_refs: Vec<&DeliveryApproval> = approvals.iter().collect();
    let job_refs: Vec<&DeliveryJob> = jobs.iter().collect();
    // While a sync's merge is in progress, work that is not resolving it
    // waits for the commit that concludes it: committed after it, its
    // changes stay apart from the merge.
    let merge = open_merge(&approval_refs, true);
    let probe = DeliveryRound {
        id: String::new(),
        task_id: task_id.to_string(),
        number,
        kind: new.kind.to_string(),
        short: String::new(),
        cause: String::new(),
        items: Vec::new(),
        source: new.source.clone(),
        opened_at_ms: now,
        after_push: false,
        closed_at_ms: None,
        outcome: None,
        waits_merge_commit: false,
    };
    let waits = merge.is_some_and(|sync| !resolves_merge(&probe, sync, &job_refs));
    // The sync's own round sets aside the one in course, unfinished: its
    // work goes on once the merge is committed.
    if merge.is_some() && new.kind == "sync" {
        if let Some(mut open) = rounds
            .iter()
            .rev()
            .find(|round| round.is_current())
            .cloned()
        {
            open.waits_merge_commit = true;
            store.put_round(&open)?;
            store.record_event(
                now,
                "round_changed",
                Some(task_id),
                None,
                &round_text(&open),
            )?;
        }
    }
    let items = new.items;
    let previous = rounds.iter().rev().find(|round| {
        round.is_open()
            && round.waits_merge_commit == waits
            && !(new.kind == "sync" && merge.is_some())
    });
    if let Some(mut open) = previous.cloned() {
        // A round waiting for the merge commit has had no stage of its own.
        let worked = !waits
            && jobs.iter().any(|job| {
                job.agent != DeliveryAgent::Shell && job.created_at_ms >= open.opened_at_ms
            });
        // A round no stage worked on yet takes the new cause, under its own
        // number: QA problems refine QA's findings; anything else joins what
        // it already had to address.
        let refines = open.kind == "qa" && new.kind == "qa_problems";
        if !worked {
            if refines {
                open.items = items;
            } else {
                for item in items {
                    if !open.items.contains(&item) {
                        open.items.push(item);
                    }
                }
            }
            open.kind = new.kind.to_string();
            open.short = new.short;
            open.cause = new.cause;
            open.source = new.source.or(open.source);
            store.put_round(&open)?;
            store.record_event(
                now,
                "round_changed",
                Some(task_id),
                None,
                &round_text(&open),
            )?;
            return Ok(());
        }
        let role = if open.kind == "review" {
            "review"
        } else {
            QA_ROLE
        };
        let fixed = matches!(open.kind.as_str(), "review" | "qa" | "qa_problems")
            && jobs.iter().any(|job| {
                job.role == role
                    && job.created_at_ms >= open.opened_at_ms
                    && job.status == DeliveryJobStatus::Finished
                    && job.result_state == Some(DeliveryResultState::Accepted)
                    && job
                        .result
                        .as_ref()
                        .is_some_and(|result| result.status == "pass")
            });
        open.closed_at_ms = Some(now);
        open.outcome = Some(if fixed {
            done_outcome(&open.kind).to_string()
        } else {
            format!("siguió en la ronda {number}")
        });
        store.put_round(&open)?;
        store.record_event(now, "round_closed", Some(task_id), None, &round_text(&open))?;
    }
    let after_push = latest_done(&task_approvals(store, task_id)?, DeliveryRung::Push).is_some();
    let round = DeliveryRound {
        id: uuid::Uuid::new_v4().to_string(),
        task_id: task_id.to_string(),
        number,
        kind: new.kind.to_string(),
        short: new.short,
        cause: new.cause,
        items,
        source: new.source,
        opened_at_ms: now,
        after_push,
        closed_at_ms: None,
        outcome: None,
        waits_merge_commit: waits,
    };
    store.put_round(&round)?;
    store.record_event(
        now,
        "round_opened",
        Some(task_id),
        None,
        &round_text(&round),
    )
}

/// Closes the task's open round once its work is done again: committed
/// (and pushed, when the branch was published before it opened), its
/// findings accepted, or its stages passed with nothing to commit.
pub(super) fn settle_round(store: &DeliveryStore, task_id: &str) -> Result<(), DeliveryError> {
    let Some(mut round) = open_of(store, task_id)? else {
        return Ok(());
    };
    let Some(task) = store.task(task_id)? else {
        return Ok(());
    };
    let jobs = store.task_jobs(task_id)?;
    let approvals = task_approvals(store, task_id)?;
    let job_refs: Vec<&DeliveryJob> = jobs.iter().collect();
    let approval_refs: Vec<&DeliveryApproval> = approvals.iter().collect();
    let accepted = round.source.as_deref().is_some_and(|source| {
        jobs.iter()
            .any(|job| job.id == source && job.resolved_at_ms.is_some())
    });
    // The round's own stages, Tinto's checks aside.
    let worked: Vec<&DeliveryJob> = jobs
        .iter()
        .filter(|job| {
            job.agent != DeliveryAgent::Shell
                && job.created_at_ms >= round.opened_at_ms
                && job.undone_at_ms.is_none()
                && job.result_state != Some(DeliveryResultState::Stale)
        })
        .collect();
    let settled = !jobs.iter().any(|job| job.status.is_open())
        && next_stage(&task.setup, &job_refs, &approval_refs).is_none();
    let unchanged = settled
        && worked.iter().all(|job| job.changes.is_empty())
        && worked.last().is_some_and(|job| {
            job.status == DeliveryJobStatus::Finished
                && job
                    .result
                    .as_ref()
                    .is_some_and(|result| result.status == "pass")
        });
    let outcome = if round_step(&round, &approval_refs).is_none() {
        done_outcome(&round.kind)
    } else if accepted {
        "aceptados"
    } else if unchanged {
        "sin cambios que publicar"
    } else {
        return Ok(());
    };
    round.closed_at_ms = Some(now_ms());
    round.outcome = Some(outcome.to_string());
    store.put_round(&round)?;
    store.record_event(
        now_ms(),
        "round_closed",
        Some(task_id),
        None,
        &round_text(&round),
    )
}

/// After a job ended: findings or failing checks open a round, and the
/// open one may be done.
pub(super) fn after_job(store: &DeliveryStore, job: &DeliveryJob) -> Result<(), DeliveryError> {
    let Some(task) = store.task(&job.task_id)? else {
        return Ok(());
    };
    let jobs = store.task_jobs(&task.id)?;
    if let Some(new) = findings_round(job).or_else(|| checks_round(&task, job, &jobs)) {
        open_round(store, &task.id, new)?;
    }
    settle_round(store, &task.id)
}

/// QA problems the user sent to fix, each with its solution.
pub(super) fn for_qa_problems(
    store: &DeliveryStore,
    task_id: &str,
    sent: &[DeliveryQaProblem],
) -> Result<(), DeliveryError> {
    let short = plural(sent.len(), "problema de QA", "problemas de QA");
    open_round(
        store,
        task_id,
        NewRound {
            kind: "qa_problems",
            cause: format!("{short} para corregir"),
            short,
            items: sent
                .iter()
                .map(|problem| {
                    line(&format!(
                        "#{} {}: {} → {}",
                        problem.number,
                        problem.label,
                        problem.text,
                        problem.solution.as_deref().unwrap_or_default()
                    ))
                })
                .collect(),
            source: None,
        },
    )
}

/// After a delivery step was saved: a sync that stopped on conflicts sends
/// the work back; a commit or push may do a round's work again.
pub(super) fn after_step(
    store: &DeliveryStore,
    approval: &DeliveryApproval,
) -> Result<(), DeliveryError> {
    if approval.status != DeliveryApprovalStatus::Executed {
        return Ok(());
    }
    match approval.rung {
        DeliveryRung::Sync if !approval.conflicts.is_empty() => open_round(
            store,
            &approval.task_id,
            NewRound {
                kind: "sync",
                short: "conflictos con la base".to_string(),
                cause: format!(
                    "Conflictos al sincronizar con la base en {}",
                    plural(approval.conflicts.len(), "archivo", "archivos")
                ),
                items: approval.conflicts.clone(),
                source: Some(approval.id.clone()),
            },
        ),
        DeliveryRung::Commit => {
            settle_round(store, &approval.task_id)?;
            release_waiting(store, &approval.task_id)
        }
        DeliveryRung::Push => settle_round(store, &approval.task_id),
        _ => Ok(()),
    }
}

/// Once the merge is concluded, the rounds that waited for it start: their
/// stages run on top of the merge commit, and their own commit comes after.
pub(super) fn release_waiting(store: &DeliveryStore, task_id: &str) -> Result<(), DeliveryError> {
    let approvals = task_approvals(store, task_id)?;
    let approval_refs: Vec<&DeliveryApproval> = approvals.iter().collect();
    if open_merge(&approval_refs, true).is_some() {
        return Ok(());
    }
    let after_push = latest_done(&approvals, DeliveryRung::Push).is_some();
    let rounds = store.task_rounds(task_id)?;
    if !rounds
        .iter()
        .any(|round| round.is_open() && round.waits_merge_commit)
    {
        return Ok(());
    }
    // The merge's round ends with its commit: its push goes out with the
    // next round's.
    for mut round in rounds.iter().filter(|round| round.is_current()).cloned() {
        round.closed_at_ms = Some(now_ms());
        round.outcome = Some("merge confirmado".to_string());
        store.put_round(&round)?;
        store.record_event(
            now_ms(),
            "round_closed",
            Some(task_id),
            None,
            &round_text(&round),
        )?;
    }
    for mut round in rounds
        .into_iter()
        .filter(|round| round.is_open() && round.waits_merge_commit)
    {
        round.waits_merge_commit = false;
        round.opened_at_ms = now_ms();
        round.after_push = after_push;
        store.put_round(&round)?;
        store.record_event(
            now_ms(),
            "round_changed",
            Some(task_id),
            None,
            &round_text(&round),
        )?;
    }
    Ok(())
}

/// A writing stage dispatched while the PR has news or asks for changes: it
/// answers them in a round of their own.
pub(super) fn for_pr(store: &DeliveryStore, task_id: &str) -> Result<(), DeliveryError> {
    let Some(pr) = store.pr(task_id)? else {
        return Ok(());
    };
    match pr_round(&pr, &store.task_rounds(task_id)?) {
        Some(new) => open_round(store, task_id, new),
        None => Ok(()),
    }
}

/// What a job dispatched in the task's open round is told about it.
pub(super) fn note(store: &DeliveryStore, task_id: &str) -> Result<Vec<String>, DeliveryError> {
    let Some(round) = open_of(store, task_id)? else {
        return Ok(Vec::new());
    };
    let mut lines = vec![format!(
        "This is round {} of the task: its work came back because of this: {}.",
        round.number, round.cause
    )];
    if !round.items.is_empty() {
        lines.push("What this round has to address:".to_string());
        lines.extend(
            round
                .items
                .iter()
                .take(MAX_ITEMS)
                .map(|item| format!("- {item}")),
        );
        if round.items.len() > MAX_ITEMS {
            lines.push(format!("- and {} more", round.items.len() - MAX_ITEMS));
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::pr_round;
    use crate::delivery::prs::DeliveryPr;

    fn pr(news: &[&str]) -> DeliveryPr {
        serde_json::from_value(serde_json::json!({
            "task_id": "t1", "url": "https://github.com/o/r/pull/5", "repo": "o/r",
            "number": 5, "state": "open", "draft": false, "conflicts": false,
            "review_decision": "approved", "checks": {"passing": 1, "failing": 0, "pending": 0},
            "new_reviews": news.len(), "new_comments": 0, "new_authors": ["ana"], "news": news,
            "latest_activity": null, "seen_until": null, "checked_at_ms": 1, "error": null,
        }))
        .unwrap()
    }

    #[test]
    fn an_approval_alone_sends_no_work_back() {
        assert!(pr_round(&pr(&["ana aprobó"]), &[]).is_none());
        let round = pr_round(&pr(&["ana aprobó", "ana: Falta un test"]), &[]).unwrap();
        assert_eq!(round.items, ["ana: Falta un test"]);
        // Changes asked, their words already seen: the round says where.
        let mut asked = pr(&[]);
        asked.review_decision = Some("changes_requested".into());
        asked.changes_requested_at_ms = Some(2);
        let round = pr_round(&asked, &[]).unwrap();
        assert_eq!(
            round.items,
            ["Lee en https://github.com/o/r/pull/5 la revisión que pidió cambios y sus comentarios"]
        );
    }
}
