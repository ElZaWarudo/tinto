//! Decisions: what the coordinator needs the user to settle before a task's
//! stages run (behaviour choices, user-facing texts, QA permissions).
//!
//! - The coordinator asks; the user answers in Tinto. Answers are final:
//!   changing one means a new contract version, not an edit.
//! - While a task has pending decisions, no stage can be dispatched on it.
//! - Answered decisions go into every later job's instructions, and an
//!   allowed permission's command joins the commands its QA job may run.
//! - "Accept recommended" settles choices and texts, never permissions.
//! - What a permission lets QA run cannot change after the user allows it:
//!   scripts agents can edit are frozen when the coordinator asks (the user
//!   reads that text and jobs run that text), and a command that runs the
//!   task's own code is allowed for the worktree content of that moment.

use super::model::{
    DeliveryDecision, DeliveryDecisionKind, DeliveryDecisionOption, DeliveryDecisionStatus,
    DeliveryFrozenScript, DeliveryTask,
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
    /// `permission`: absolute paths of the scripts the command runs.
    pub scripts: Vec<String>,
    /// `permission`: folders outside the worktree QA may read and list.
    pub read_dirs: Vec<String>,
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
            for dir in &decision.read_dirs {
                let dir = norm(dir.trim());
                let inside = dir
                    .trim_end_matches('/')
                    .split('/')
                    .skip(1)
                    .any(|part| !part.is_empty());
                if !is_absolute(&dir) || !inside {
                    return Err(invalid(format!(
                        "read_dirs: {dir} debe ser la ruta absoluta de una carpeta concreta"
                    )));
                }
            }
        }
    }
    Ok(())
}

const SCRIPT_EXTENSIONS: [&str; 11] = [
    ".sh", ".bash", ".ps1", ".py", ".js", ".mjs", ".cjs", ".ts", ".cmd", ".bat", ".rb",
];
const MAX_SCRIPT_BYTES: u64 = 256 * 1024;

/// One spelling for Windows, WSL mounts and slashes.
fn norm(text: &str) -> String {
    let text = text.replace('\\', "/").to_lowercase();
    match text.strip_prefix("/mnt/") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b'/') => {
            format!("{}:{}", &rest[..1], &rest[1..])
        }
        _ => text,
    }
}

fn is_absolute(path: &str) -> bool {
    path.starts_with('/') || path.as_bytes().get(1) == Some(&b':')
}

/// Whether `text` names `path` or something inside it (not a sibling whose
/// name merely starts the same, like `repo-wt` for `repo`).
fn names(text: &str, path: &str) -> bool {
    text.match_indices(path).any(|(at, _)| {
        !text
            .as_bytes()
            .get(at + path.len())
            .is_some_and(|next| next.is_ascii_alphanumeric() || matches!(next, b'-' | b'_' | b'.'))
    })
}

/// What Tinto works out about a permission before the user sees it.
#[derive(Debug, Default)]
struct Frozen {
    scripts: Vec<DeliveryFrozenScript>,
    runs_candidate: bool,
    unfrozen: Vec<String>,
}

/// Reads a script where the task lives (a WSL repo's files are reached
/// through the distro's network share).
fn read_script(path: &str, task: &DeliveryTask) -> Result<String, DeliveryError> {
    let file = match task.distro.as_deref() {
        Some(distro) if path.starts_with('/') => std::path::PathBuf::from(format!(
            r"\\wsl.localhost\{distro}{}",
            path.replace('/', "\\")
        )),
        _ => std::path::PathBuf::from(path),
    };
    let unreadable =
        |reason: String| invalid(format!("no se pudo leer el script {path}: {reason}"));
    let size = std::fs::metadata(&file)
        .map_err(|error| unreadable(error.to_string()))?
        .len();
    if size > MAX_SCRIPT_BYTES {
        return Err(unreadable(
            "es demasiado grande para congelarlo".to_string(),
        ));
    }
    std::fs::read_to_string(&file).map_err(|error| unreadable(error.to_string()))
}

/// Freezes the scripts a permission declares and checks what its command
/// runs. Agents can edit the repo, its worktrees and `.agent` after the user
/// allows the permission, so a command may only name files there when they
/// are declared scripts (frozen) or belong to the task's worktree (the
/// permission then holds for one candidate).
fn freeze(decision: &NewDecision, task: &DeliveryTask) -> Result<Frozen, DeliveryError> {
    let mut frozen = Frozen::default();
    if decision.kind != DeliveryDecisionKind::Permission {
        return Ok(frozen);
    }
    let worktree = norm(&plain_path(&task.worktree));
    let editable_roots = [
        Some(norm(&plain_path(&task.repo))),
        task.worktree.parent().map(|root| norm(&plain_path(root))),
    ];
    let editable = |text: &str| {
        text.contains(".agent/")
            || editable_roots
                .iter()
                .flatten()
                .any(|root| names(text, root))
    };
    for path in &decision.scripts {
        let path = path.trim();
        if !is_absolute(path) {
            return Err(invalid(format!(
                "scripts: {path} debe ser una ruta absoluta"
            )));
        }
        let name = |path: &str| norm(path).rsplit('/').next().map(str::to_string);
        if frozen
            .scripts
            .iter()
            .any(|other| name(&other.path) == name(path))
        {
            return Err(invalid(format!(
                "scripts: hay dos archivos llamados igual que {path}; se copian a una misma carpeta"
            )));
        }
        frozen.scripts.push(DeliveryFrozenScript {
            path: path.to_string(),
            content: read_script(path, task)?,
        });
    }
    let declared: Vec<String> = frozen
        .scripts
        .iter()
        .map(|script| norm(&script.path))
        .collect();
    for token in decision
        .command
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
    {
        let token = token.trim_matches(|c| c == '"' || c == '\'');
        let text = norm(token);
        if let Some(index) = declared.iter().position(|path| *path == text) {
            // Keep the command's spelling: jobs replace it with the copy.
            frozen.scripts[index].path = token.to_string();
        } else if names(&text, &worktree) {
            frozen.runs_candidate = true;
        } else if editable(&text) {
            return Err(invalid(format!(
                "el comando ejecuta {token}, que los agentes pueden editar después de que el usuario lo permita: decláralo en `scripts` para que Tinto lo congele y el usuario vea su contenido"
            )));
        } else if SCRIPT_EXTENSIONS.iter().any(|ext| text.ends_with(ext)) && !is_absolute(token) {
            return Err(invalid(format!(
                "el comando ejecuta {token} por ruta relativa (se resolvería dentro del worktree): usa su ruta absoluta y decláralo en `scripts`"
            )));
        }
    }
    for script in &frozen.scripts {
        let words = script
            .content
            .split(|c: char| c.is_whitespace() || "\"'=;()<>|&,`".contains(c));
        for word in words.filter(|word| !word.is_empty()) {
            let text = norm(word);
            if declared.contains(&text) {
                continue;
            }
            if names(&text, &worktree) {
                frozen.runs_candidate = true;
            } else if editable(&text) && !frozen.unfrozen.iter().any(|seen| norm(seen) == text) {
                frozen.unfrozen.push(word.to_string());
            }
        }
    }
    Ok(frozen)
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
            let mut note = format!("{} → {verdict}", decision.question);
            if let Some(command) = decision.command.as_deref() {
                note.push_str(&format!(" (command: {command})"));
            }
            if !decision.read_dirs.is_empty() {
                note.push_str(&format!(
                    " (read and list: {})",
                    decision.read_dirs.join(", ")
                ));
            }
            note
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
        // The batch is all or nothing, and the refusal names every invalid
        // item, so a coordinator never believes a valid one was asked.
        let mut frozen = Vec::new();
        let mut refused = Vec::new();
        for (index, decision) in decisions.iter().enumerate() {
            match validate(decision).and_then(|_| freeze(decision, &task)) {
                Ok(checked) => frozen.push(checked),
                Err(error) => refused.push(format!(
                    "#{} «{}»: {}",
                    index + 1,
                    decision.question.trim(),
                    error.message
                )),
            }
        }
        if !refused.is_empty() {
            let valid = decisions.len() - refused.len();
            let rest = match valid {
                0 => String::new(),
                _ => format!(
                    " Las otras {valid} son válidas pero tampoco se crearon: corrige y reenvía el lote completo."
                ),
            };
            return Err(invalid(format!(
                "no se creó ninguna decisión de este lote de {}. Inválidas: {}.{rest}",
                decisions.len(),
                refused.join("; ")
            )));
        }
        let now = now_ms();
        let created: Vec<DeliveryDecision> = decisions
            .into_iter()
            .zip(frozen)
            .map(|(decision, frozen)| DeliveryDecision {
                scripts: frozen.scripts,
                read_dirs: decision
                    .read_dirs
                    .iter()
                    .map(|dir| dir.trim().to_string())
                    .collect(),
                runs_candidate: frozen.runs_candidate,
                unfrozen: frozen.unfrozen,
                candidate: None,
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
        if decision.runs_candidate && answer == ALLOWED {
            decision.candidate = Some(self.worktree_content(&decision)?);
        }
        decision.status = DeliveryDecisionStatus::Answered;
        decision.answer = Some(answer.to_string());
        self.record_answer(decision, decided_by, "decision_answered")
    }

    /// The user allows again a permission that runs the task's code, for the
    /// worktree as it is now (it had changed since they allowed it).
    pub fn renew_permission(
        &self,
        decision_id: &str,
        decided_by: &str,
    ) -> Result<DeliveryDecision, DeliveryError> {
        let mut decision = self
            .store()?
            .decision(decision_id)?
            .ok_or_else(|| DeliveryError::not_found("la decisión", decision_id))?;
        if !decision.runs_candidate || decision.answer.as_deref() != Some(ALLOWED) {
            return Err(invalid(
                "solo se renueva un permiso permitido que ejecuta el código de la tarea",
            ));
        }
        decision.candidate = Some(self.worktree_content(&decision)?);
        self.record_answer(decision, decided_by, "permission_renewed")
    }

    /// The tree id of the task's worktree as it is now.
    fn worktree_content(&self, decision: &DeliveryDecision) -> Result<String, DeliveryError> {
        let task = self.task(&decision.task_id)?;
        Ok(self
            .snapshot(
                &task,
                &format!("decision-{}-{}", decision.id, now_ms()),
                None,
            )?
            .tree)
    }

    fn record_answer(
        &self,
        mut decision: DeliveryDecision,
        decided_by: &str,
        event: &str,
    ) -> Result<DeliveryDecision, DeliveryError> {
        decision.decided_by = Some(decided_by.to_string());
        decision.decided_at_ms = Some(now_ms());
        let store = self.store()?;
        store.put_decision(&decision)?;
        store.record_event(now_ms(), event, Some(&decision.task_id), None, &decision.id)?;
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
