//! What the phone may do (docs/contracts/mobile-link.md §4): five actions,
//! each the same service call the desktop panel makes, as the user. Any
//! other request, or an extra field, does not parse.

use serde::Deserialize;
use serde_json::Value;

use crate::delivery::model::DeliveryApprovalStatus;
use crate::delivery::qa_problems::QaProblemChoice;
use crate::delivery::service::DeliveryService;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    AnswerDecision {
        decision_id: String,
        answer: String,
    },
    AcceptRecommended {
        task_id: String,
    },
    DecideApproval {
        approval_id: String,
        approve: bool,
        digest: String,
    },
    SendQaProblems {
        task_id: String,
        choices: Vec<Choice>,
    },
    SetRunPaused {
        run_id: String,
        paused: bool,
    },
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub problem_id: String,
    pub solution: String,
}

pub fn parse(action: &Value) -> Result<Action, String> {
    Action::deserialize(action).map_err(|_| "El móvil no puede hacer eso.".to_string())
}

/// Runs the action as the user; the error is for the user to read.
pub fn run(service: &DeliveryService, action: Action) -> Result<(), String> {
    let done = match action {
        Action::AnswerDecision {
            decision_id,
            answer,
        } => service
            .answer_decision(&decision_id, &answer, "user")
            .map(drop),
        Action::AcceptRecommended { task_id } => {
            service.accept_recommended(&task_id, "user").map(drop)
        }
        Action::DecideApproval {
            approval_id,
            approve,
            digest,
        } => {
            if approve {
                let approval = service
                    .approval(&approval_id)
                    .map_err(|error| error.message)?;
                approvable(&approval, &digest)?;
            }
            service
                .decide_approval(&approval_id, approve, None)
                .map(drop)
        }
        Action::SendQaProblems { task_id, choices } => {
            let choices: Vec<QaProblemChoice> = choices
                .into_iter()
                .map(|choice| QaProblemChoice {
                    problem_id: choice.problem_id,
                    solution: choice.solution,
                })
                .collect();
            service.send_qa_problems(&task_id, &choices).map(drop)
        }
        Action::SetRunPaused { run_id, paused } => {
            service.set_run_paused(&run_id, paused).map(drop)
        }
    };
    done.map_err(|error| error.message)
}

/// The phone approves only the text it showed, and only a text it showed
/// whole.
fn approvable(
    approval: &crate::delivery::model::DeliveryApproval,
    digest: &str,
) -> Result<(), String> {
    if approval.status != DeliveryApprovalStatus::Pending {
        return Ok(()); // The service says why it cannot be decided.
    }
    let view = super::snapshot::approval(approval);
    if view.digest != digest {
        return Err("El texto cambió desde que lo viste: revísalo de nuevo.".into());
    }
    if view.title.chars().count() > super::snapshot::APPROVAL_TEXT
        || view.body.chars().count() > super::snapshot::APPROVAL_TEXT
        || view
            .commits
            .iter()
            .any(|c| c.chars().count() > super::snapshot::APPROVAL_TEXT)
    {
        return Err("El texto es demasiado largo para el móvil: apruébalo en Tinto.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobile_link::snapshot::tests::sample_approval;
    use serde_json::json;

    #[test]
    fn only_the_allowlisted_actions_with_their_exact_fields_parse() {
        assert_eq!(
            parse(&json!({ "action": "set_run_paused", "run_id": "r1", "paused": true })),
            Ok(Action::SetRunPaused {
                run_id: "r1".into(),
                paused: true
            })
        );
        for refused in [
            json!({ "action": "delivery_dispatch_job", "task_id": "t1" }),
            json!({ "action": "set_run_paused", "run_id": "r1", "paused": true, "repo": "/" }),
            json!({ "action": "answer_decision", "decision_id": "d1" }),
            json!({ "action": "send_qa_problems", "task_id": "t1",
                    "choices": [{ "problem_id": "p", "solution": "s", "command": "rm" }] }),
            json!("answer_decision"),
        ] {
            assert!(parse(&refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn an_approval_needs_the_digest_of_the_whole_text_the_phone_showed() {
        let shown = sample_approval("a1", "cuerpo");
        let digest = crate::mobile_link::snapshot::approval(&shown).digest;
        assert_eq!(approvable(&shown, &digest), Ok(()));
        let mut changed = shown.clone();
        changed.body = "otro cuerpo".into();
        assert!(approvable(&changed, &digest).is_err());
        let long = sample_approval("a1", &"b".repeat(20_000));
        let digest = crate::mobile_link::snapshot::approval(&long).digest;
        assert!(approvable(&long, &digest).is_err());
    }
}
