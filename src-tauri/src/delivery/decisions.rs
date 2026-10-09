//! Decisions: what the coordinator needs the user to settle before a task's
//! stages run (behaviour choices, user-facing texts, QA permissions).
//!
//! - The coordinator asks; the user answers in Tinto. Answers are final:
//!   changing one means a new contract version, not an edit.
//! - While a task has pending decisions, no stage can be dispatched on it.
//! - Answered decisions go into every later job's instructions, and an
//!   allowed permission's command joins the commands its QA job may run.
//! - "Accept recommended" settles choices and texts, never permissions.

use super::model::{
    DeliveryDecision, DeliveryDecisionKind, DeliveryDecisionOption, DeliveryDecisionStatus,
    DeliveryTask,
};
use super::service::DeliveryService;
use super::tasks::plain_path;
use super::{now_ms, DeliveryError};

pub const ALLOWED: &str = "allowed";
pub const DENIED: &str = "denied";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDecision {
    pub kind: DeliveryDecisionKind,
    pub question: String,
    pub detail: String,
    pub options: Vec<DeliveryDecisionOption>,
    pub text: String,
    pub command: Option<String>,
    pub undo: String,
}

fn invalid(message: impl Into<String>) -> DeliveryError {
    DeliveryError::new("invalid_decision", message)
}

fn validate(decision: &NewDecision) -> Result<(), DeliveryError> {
    if decision.question.trim().is_empty() {
        return Err(invalid("cada decisión necesita una pregunta"));
    }
    match decision.kind {
        DeliveryDecisionKind::Choice => {
            if !(2..=4).contains(&decision.options.len()) {
                return Err(invalid("una elección necesita entre 2 y 4 opciones"));
            }
            if decision
                .options
                .iter()
                .any(|option| option.label.trim().is_empty())
            {
                return Err(invalid("cada opción necesita un texto"));
            }
            if decision
                .options
                .iter()
                .filter(|option| option.recommended)
                .count()
                > 1
            {
                return Err(invalid("solo una opción puede ser la recomendada"));
            }
        }
        DeliveryDecisionKind::Text => {
            if decision.text.trim().is_empty() {
                return Err(invalid("un texto a aprobar necesita el texto propuesto"));
            }
        }
        DeliveryDecisionKind::Permission => {
            if decision.undo.trim().is_empty() {
                return Err(invalid("un permiso necesita decir cómo se deshace"));
            }
        }
    }
    Ok(())
}

const SCRIPT_EXTENSIONS: [&str; 11] = [
    ".sh", ".bash", ".ps1", ".py", ".js", ".mjs", ".cjs", ".ts", ".cmd", ".bat", ".rb",
];

/// Whether a permission's command runs files agents can edit after the user
/// approves it: anything in the repo, its worktrees or `.agent`, or a script
/// given by a relative path (it resolves inside the worktree).
fn runs_agent_files(command: &str, task: &DeliveryTask) -> bool {
    // One spelling for Windows, WSL mounts and slashes.
    let norm = |text: &str| {
        let text = text.replace('\\', "/").to_lowercase();
        match text.strip_prefix("/mnt/") {
            Some(rest) if rest.as_bytes().get(1) == Some(&b'/') => {
                format!("{}:{}", &rest[..1], &rest[1..])
            }
            _ => text,
        }
    };
    let text = norm(command);
    let owned = [
        Some(norm(&plain_path(&task.repo))),
        task.worktree.parent().map(|root| norm(&plain_path(root))),
    ];
    if text.contains(".agent/")
        || owned
            .iter()
            .flatten()
            .any(|path| text.contains(path.as_str()))
    {
        return true;
    }
    text.split_whitespace().any(|token| {
        let token = token.trim_matches(|c| c == '"' || c == '\'');
        let script = SCRIPT_EXTENSIONS.iter().any(|ext| token.ends_with(ext));
        let absolute = token.starts_with('/') || token.as_bytes().get(1) == Some(&b':');
        script && !absolute
    })
}

/// How a job's instructions state an answered decision.
pub fn decision_note(decision: &DeliveryDecision) -> Option<String> {
    let answer = decision.answer.as_deref()?;
    Some(match decision.kind {
        DeliveryDecisionKind::Choice => format!("{} → {answer}", decision.question),
        DeliveryDecisionKind::Text => {
            format!(
                "{} → approved text, use it exactly: {answer}",
                decision.question
            )
        }
        DeliveryDecisionKind::Permission => {
            let verdict = if answer == ALLOWED {
                "allowed"
            } else {
                "NOT allowed"
            };
            match decision.command.as_deref() {
                Some(command) => format!("{} → {verdict} (command: {command})", decision.question),
                None => format!("{} → {verdict}", decision.question),
            }
        }
    })
}

impl DeliveryService {
    pub fn request_decisions(
        &self,
        task_id: &str,
        decisions: Vec<NewDecision>,
        requested_by: &str,
    ) -> Result<Vec<DeliveryDecision>, DeliveryError> {
        let task = self.task(task_id)?;
        if decisions.is_empty() {
            return Err(invalid("no hay decisiones que pedir"));
        }
        for decision in &decisions {
            validate(decision)?;
            if let Some(command) = decision.command.as_deref() {
                if decision.kind == DeliveryDecisionKind::Permission
                    && runs_agent_files(command, &task)
                {
                    return Err(invalid(
                        "el comando de un permiso no puede ejecutar archivos que los agentes pueden editar (el repositorio, sus worktrees o .agent): pide el comando real",
                    ));
                }
            }
        }
        let now = now_ms();
        let created: Vec<DeliveryDecision> = decisions
            .into_iter()
            .map(|decision| DeliveryDecision {
                id: uuid::Uuid::new_v4().to_string(),
                task_id: task.id.clone(),
                kind: decision.kind,
                question: decision.question.trim().to_string(),
                detail: decision.detail.trim().to_string(),
                options: decision.options,
                text: decision.text.trim().to_string(),
                command: decision
                    .command
                    .map(|command| command.trim().to_string())
                    .filter(|command| !command.is_empty()),
                undo: decision.undo.trim().to_string(),
                status: DeliveryDecisionStatus::Pending,
                answer: None,
                requested_by: requested_by.to_string(),
                requested_at_ms: now,
                decided_by: None,
                decided_at_ms: None,
            })
            .collect();
        let store = self.store()?;
        for decision in &created {
            store.put_decision(decision)?;
        }
        store.record_event(
            now,
            "decisions_requested",
            Some(&task.id),
            None,
            &created.len().to_string(),
        )?;
        drop(store);
        self.notify();
        Ok(created)
    }

    pub fn task_decisions(&self, task_id: &str) -> Result<Vec<DeliveryDecision>, DeliveryError> {
        self.store()?.task_decisions(task_id)
    }

    /// The user's answer: an option's label, the final text, or
    /// [`ALLOWED`]/[`DENIED`].
    pub fn answer_decision(
        &self,
        decision_id: &str,
        answer: &str,
        decided_by: &str,
    ) -> Result<DeliveryDecision, DeliveryError> {
        let mut decision = self
            .store()?
            .decision(decision_id)?
            .ok_or_else(|| DeliveryError::not_found("la decisión", decision_id))?;
        if decision.status != DeliveryDecisionStatus::Pending {
            return Err(DeliveryError::new(
                "decision_answered",
                "esta decisión ya tiene respuesta; para cambiarla, sube la versión del contrato y pide otra",
            ));
        }
        let answer = answer.trim();
        let valid = match decision.kind {
            DeliveryDecisionKind::Choice => {
                decision.options.iter().any(|option| option.label == answer)
            }
            DeliveryDecisionKind::Text => !answer.is_empty(),
            DeliveryDecisionKind::Permission => answer == ALLOWED || answer == DENIED,
        };
        if !valid {
            return Err(invalid("la respuesta no corresponde a esta decisión"));
        }
        decision.status = DeliveryDecisionStatus::Answered;
        decision.answer = Some(answer.to_string());
        decision.decided_by = Some(decided_by.to_string());
        decision.decided_at_ms = Some(now_ms());
        let store = self.store()?;
        store.put_decision(&decision)?;
        store.record_event(
            now_ms(),
            "decision_answered",
            Some(&decision.task_id),
            None,
            &decision.id,
        )?;
        drop(store);
        self.notify();
        Ok(decision)
    }

    /// Answers every pending choice with its recommended option and approves
    /// every pending text as proposed. Permissions stay pending.
    pub fn accept_recommended(
        &self,
        task_id: &str,
        decided_by: &str,
    ) -> Result<Vec<DeliveryDecision>, DeliveryError> {
        let pending: Vec<DeliveryDecision> = self
            .task_decisions(task_id)?
            .into_iter()
            .filter(|decision| decision.status == DeliveryDecisionStatus::Pending)
            .collect();
        let mut answered = Vec::new();
        for decision in pending {
            let answer = match decision.kind {
                DeliveryDecisionKind::Choice => decision
                    .options
                    .iter()
                    .find(|option| option.recommended)
                    .map(|option| option.label.clone()),
                DeliveryDecisionKind::Text => Some(decision.text.clone()),
                DeliveryDecisionKind::Permission => None,
            };
            if let Some(answer) = answer {
                answered.push(self.answer_decision(&decision.id, &answer, decided_by)?);
            }
        }
        Ok(answered)
    }
}
