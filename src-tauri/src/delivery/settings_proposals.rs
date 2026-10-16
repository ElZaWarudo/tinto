//! Settings proposals: changes to a repo's settings that a coordinator asks
//! for and the user approves or rejects in Tinto.
//!
//! - The coordinator never writes settings: it proposes, and nothing changes
//!   until the user approves, in whole or in part.
//! - The verification commands are also what agents may run, so a proposed
//!   command is refused when it runs something agents can edit (the repo,
//!   its worktrees, `.agent`, a script by relative path) or names a path
//!   outside the repo. A script has to go through a permission decision,
//!   which freezes it.
//! - Approving changes the settings for the jobs dispatched from then on;
//!   jobs already queued or running keep what they got.
//! - Once the coordinator ends its run's brainstorm, a valid proposal is
//!   approved whole as it is made, decided by `auto`.

use std::path::Path;

use super::decisions::{is_absolute, names, norm, AUTO, SCRIPT_EXTENSIONS};
use super::model::{
    DeliveryRepoSettings, DeliverySettingsChanges, DeliverySettingsProposal,
    DeliverySettingsProposalStatus, DeliveryTask,
};
use super::service::DeliveryService;
use super::tasks::plain_path;
use super::{now_ms, DeliveryError};

fn invalid(message: impl Into<String>) -> DeliveryError {
    DeliveryError::new("invalid_settings", message)
}

fn trimmed(items: &[String]) -> Vec<String> {
    items.iter().map(|item| item.trim().to_string()).collect()
}

fn clean(changes: &DeliverySettingsChanges) -> DeliverySettingsChanges {
    DeliverySettingsChanges {
        checks_add: trimmed(&changes.checks_add),
        checks_remove: trimmed(&changes.checks_remove),
        qa_commands_add: trimmed(&changes.qa_commands_add),
        bootstrap: changes
            .bootstrap
            .as_deref()
            .map(|text| text.trim().to_string()),
        qa_browser: changes.qa_browser,
        verification: changes
            .verification
            .as_deref()
            .map(|text| text.trim().to_string()),
    }
}

fn is_empty(changes: &DeliverySettingsChanges) -> bool {
    changes.checks_add.is_empty()
        && changes.checks_remove.is_empty()
        && changes.qa_commands_add.is_empty()
        && changes.bootstrap.is_none()
        && changes.qa_browser.is_none()
        && changes.verification.is_none()
}

/// Why a command agents would run cannot be proposed, if it cannot.
fn command_problem(command: &str, task: &DeliveryTask) -> Option<String> {
    if command.is_empty() {
        return Some("está vacío".to_string());
    }
    let editable_roots = [
        Some(norm(&plain_path(&task.repo))),
        task.worktree.parent().map(|root| norm(&plain_path(root))),
    ];
    for token in command.split_whitespace() {
        let token = token.trim_matches(|c| c == '"' || c == '\'');
        let text = norm(token);
        let path = text
            .split_once('=')
            .map_or(text.as_str(), |(_, value)| value);
        if text.contains(".agent/")
            || editable_roots
                .iter()
                .flatten()
                .any(|root| names(path, root))
        {
            return Some(format!(
                "ejecuta {token}, que los agentes pueden editar; pídelo como permiso con `scripts` para que Tinto lo congele"
            ));
        }
        if is_absolute(path) || path.split('/').any(|part| part == "..") {
            return Some(format!("nombra {token}, fuera del repositorio"));
        }
        if SCRIPT_EXTENSIONS.iter().any(|ext| path.ends_with(ext)) {
            return Some(format!(
                "ejecuta {token} por ruta relativa, un script que los agentes pueden editar; pídelo como permiso con `scripts` para que Tinto lo congele"
            ));
        }
    }
    None
}

/// Every reason the changes cannot be proposed, each naming its item.
fn problems(
    changes: &DeliverySettingsChanges,
    settings: &DeliveryRepoSettings,
    task: &DeliveryTask,
) -> Vec<String> {
    let mut problems = Vec::new();
    let mut commands = |field: &str, items: &[String], current: &[String]| {
        for (index, item) in items.iter().enumerate() {
            if let Some(problem) = command_problem(item, task) {
                problems.push(format!("{field} «{item}»: {problem}"));
            } else if items[..index].contains(item) {
                problems.push(format!("{field} «{item}»: está repetido"));
            } else if current.contains(item) {
                problems.push(format!("{field} «{item}»: ya está en los ajustes"));
            }
        }
    };
    commands("checks_add", &changes.checks_add, &settings.checks);
    commands(
        "qa_commands_add",
        &changes.qa_commands_add,
        &settings.qa_commands,
    );
    if let Some(bootstrap) = &changes.bootstrap {
        if let Some(problem) = command_problem(bootstrap, task) {
            problems.push(format!("bootstrap «{bootstrap}»: {problem}"));
        }
    }
    for (index, item) in changes.checks_remove.iter().enumerate() {
        if changes.checks_remove[..index].contains(item) {
            problems.push(format!("checks_remove «{item}»: está repetido"));
        } else if !settings.checks.contains(item) {
            problems.push(format!("checks_remove «{item}»: no está en los ajustes"));
        } else if changes.checks_add.contains(item) {
            problems.push(format!("checks_remove «{item}»: también se añade"));
        }
    }
    if changes
        .verification
        .as_deref()
        .is_some_and(|text| text.is_empty())
    {
        problems.push("verification: está vacío".to_string());
    }
    problems
}

/// Whether every change in `part` is one of `whole`.
fn is_part_of(part: &DeliverySettingsChanges, whole: &DeliverySettingsChanges) -> bool {
    let within = |items: &[String], all: &[String]| items.iter().all(|item| all.contains(item));
    within(&part.checks_add, &whole.checks_add)
        && within(&part.checks_remove, &whole.checks_remove)
        && within(&part.qa_commands_add, &whole.qa_commands_add)
        && (part.bootstrap.is_none() || part.bootstrap == whole.bootstrap)
        && (part.qa_browser.is_none() || part.qa_browser == whole.qa_browser)
        && (part.verification.is_none() || part.verification == whole.verification)
}

/// The settings with the changes applied. Adding what is already there or
/// removing what is not changes nothing.
fn apply(settings: &mut DeliveryRepoSettings, changes: &DeliverySettingsChanges) {
    for item in &changes.checks_add {
        if !settings.checks.contains(item) {
            settings.checks.push(item.clone());
        }
    }
    settings
        .checks
        .retain(|item| !changes.checks_remove.contains(item));
    for item in &changes.qa_commands_add {
        if !settings.qa_commands.contains(item) {
            settings.qa_commands.push(item.clone());
        }
    }
    if let Some(bootstrap) = &changes.bootstrap {
        settings.bootstrap = Some(bootstrap.clone()).filter(|text| !text.is_empty());
    }
    if let Some(qa_browser) = changes.qa_browser {
        settings.qa_browser = qa_browser;
    }
    if let Some(verification) = &changes.verification {
        settings.verification = verification.clone();
    }
}

impl DeliveryService {
    /// The coordinator's proposal, for the repo of `task`. All or nothing:
    /// the refusal names every invalid item.
    pub fn propose_settings(
        &self,
        run_id: &str,
        task: &DeliveryTask,
        changes: DeliverySettingsChanges,
        reason: &str,
        requested_by: &str,
    ) -> Result<DeliverySettingsProposal, DeliveryError> {
        let changes = clean(&changes);
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(invalid("falta reason: una línea para el usuario"));
        }
        if is_empty(&changes) {
            return Err(invalid("la propuesta no cambia nada"));
        }
        let settings = self.store()?.repo_settings(&task.repo)?;
        let problems = problems(&changes, &settings, task);
        if !problems.is_empty() {
            return Err(invalid(format!(
                "no se creó la propuesta. Inválidos: {}",
                problems.join("; ")
            )));
        }
        let proposal = DeliverySettingsProposal {
            id: uuid::Uuid::new_v4().to_string(),
            repo: task.repo.clone(),
            task_id: task.id.clone(),
            run_id: run_id.to_string(),
            reason: reason.to_string(),
            changes,
            status: DeliverySettingsProposalStatus::Pending,
            approved: None,
            requested_by: requested_by.to_string(),
            requested_at_ms: now_ms(),
            decided_by: None,
            decided_at_ms: None,
        };
        let store = self.store()?;
        store.put_settings_proposal(&proposal)?;
        store.record_event(
            proposal.requested_at_ms,
            "settings_proposed",
            Some(&proposal.task_id),
            None,
            &proposal.reason,
        )?;
        drop(store);
        self.notify();
        // Past the brainstorm nothing waits for the user: it applies whole.
        if self.run(run_id)?.unattended {
            let changes = proposal.changes.clone();
            return self.approve_settings_proposal(&proposal.id, changes, AUTO);
        }
        Ok(proposal)
    }

    pub fn repo_settings_proposals(
        &self,
        repo: &Path,
    ) -> Result<Vec<DeliverySettingsProposal>, DeliveryError> {
        self.store()?.repo_settings_proposals(repo)
    }

    /// The user approves `accepted`: the proposal or a part of it.
    pub fn approve_settings_proposal(
        &self,
        proposal_id: &str,
        accepted: DeliverySettingsChanges,
        decided_by: &str,
    ) -> Result<DeliverySettingsProposal, DeliveryError> {
        let mut proposal = self.pending_settings_proposal(proposal_id)?;
        let accepted = clean(&accepted);
        if is_empty(&accepted) {
            return Err(invalid(
                "no hay ningún cambio elegido: elige alguno o rechaza la propuesta",
            ));
        }
        if !is_part_of(&accepted, &proposal.changes) {
            return Err(invalid("lo aprobado no forma parte de la propuesta"));
        }
        let store = self.store()?;
        let mut settings = store.repo_settings(&proposal.repo)?;
        apply(&mut settings, &accepted);
        store.set_repo_settings(&proposal.repo, &settings)?;
        proposal.approved = Some(accepted);
        self.decide_settings_proposal(
            &store,
            &mut proposal,
            DeliverySettingsProposalStatus::Approved,
            decided_by,
        )?;
        drop(store);
        self.notify();
        Ok(proposal)
    }

    pub fn reject_settings_proposal(
        &self,
        proposal_id: &str,
        decided_by: &str,
    ) -> Result<DeliverySettingsProposal, DeliveryError> {
        let mut proposal = self.pending_settings_proposal(proposal_id)?;
        let store = self.store()?;
        self.decide_settings_proposal(
            &store,
            &mut proposal,
            DeliverySettingsProposalStatus::Rejected,
            decided_by,
        )?;
        drop(store);
        self.notify();
        Ok(proposal)
    }

    fn pending_settings_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<DeliverySettingsProposal, DeliveryError> {
        let proposal = self
            .store()?
            .settings_proposal(proposal_id)?
            .ok_or_else(|| DeliveryError::not_found("la propuesta de ajustes", proposal_id))?;
        if proposal.status != DeliverySettingsProposalStatus::Pending {
            return Err(DeliveryError::new(
                "settings_proposal_decided",
                "esta propuesta de ajustes ya está decidida",
            ));
        }
        Ok(proposal)
    }

    fn decide_settings_proposal(
        &self,
        store: &super::store::DeliveryStore,
        proposal: &mut DeliverySettingsProposal,
        status: DeliverySettingsProposalStatus,
        decided_by: &str,
    ) -> Result<(), DeliveryError> {
        proposal.status = status;
        proposal.decided_by = Some(decided_by.to_string());
        proposal.decided_at_ms = Some(now_ms());
        store.put_settings_proposal(proposal)?;
        let kind = match status {
            DeliverySettingsProposalStatus::Approved => "settings_approved",
            _ => "settings_rejected",
        };
        store.record_event(
            proposal.decided_at_ms.unwrap_or_default(),
            kind,
            Some(&proposal.task_id),
            None,
            &proposal.reason,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn task() -> DeliveryTask {
        DeliveryTask {
            id: "task".into(),
            run_id: Some("run".into()),
            repo: PathBuf::from("C:/code/agentos"),
            distro: None,
            key: "AGOS-474".into(),
            title: "AGOS-474".into(),
            worktree: PathBuf::from("C:/code/agentos-wt/AGOS-474"),
            branch: "fix/AGOS-474".into(),
            base_ref: "develop".into(),
            base_commit: "abc".into(),
            state: "tests".into(),
            contract_version: 1,
            created_at_ms: 0,
            updated_at_ms: 0,
            removed_at_ms: None,
            archived_at_ms: None,
        }
    }

    fn checks(items: &[&str]) -> DeliverySettingsChanges {
        DeliverySettingsChanges {
            checks_add: items.iter().map(|item| item.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn plain_commands_inside_the_repo_are_valid() {
        let changes = checks(&[
            "npm --prefix services/memories test",
            "npm --prefix frontend run preflight",
            "cargo test --manifest-path=src-tauri/Cargo.toml",
        ]);
        assert!(problems(&changes, &DeliveryRepoSettings::default(), &task()).is_empty());
    }

    #[test]
    fn commands_agents_can_edit_or_outside_the_repo_are_refused_by_name() {
        let changes = DeliverySettingsChanges {
            checks_add: vec![
                "bash scripts/check.sh".into(),
                "node C:/code/agentos/tools/run.mjs".into(),
                "python /home/me/.agent/qa.py".into(),
                "npm --prefix ../other test".into(),
                "C:/tools/lint.exe".into(),
                "  ".into(),
            ],
            ..Default::default()
        };
        let found = problems(&clean(&changes), &DeliveryRepoSettings::default(), &task());
        assert_eq!(found.len(), 6, "{found:?}");
        assert!(found[0].contains("scripts/check.sh") && found[0].contains("ruta relativa"));
        assert!(found[1].contains("pueden editar"));
        assert!(found[2].contains("pueden editar"));
        assert!(found[3].contains("fuera del repositorio"));
        assert!(found[4].contains("fuera del repositorio"));
        assert!(found[5].contains("vacío"));
    }

    #[test]
    fn duplicates_and_removals_of_missing_commands_are_refused() {
        let settings = DeliveryRepoSettings {
            checks: vec!["npm test".into()],
            ..Default::default()
        };
        let changes = DeliverySettingsChanges {
            checks_add: vec![
                "npm test".into(),
                "npm run lint".into(),
                "npm run lint".into(),
            ],
            checks_remove: vec!["npm run build".into()],
            ..Default::default()
        };
        let found = problems(&changes, &settings, &task());
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found[0].contains("ya está en los ajustes"));
        assert!(found[1].contains("repetido"));
        assert!(found[2].contains("no está en los ajustes"));
    }

    #[test]
    fn a_part_must_come_from_the_proposal() {
        let whole = DeliverySettingsChanges {
            checks_add: vec!["a".into(), "b".into()],
            qa_browser: Some(true),
            ..Default::default()
        };
        assert!(is_part_of(&checks(&["b"]), &whole));
        assert!(!is_part_of(&checks(&["c"]), &whole));
        let other_browser = DeliverySettingsChanges {
            qa_browser: Some(false),
            ..Default::default()
        };
        assert!(!is_part_of(&other_browser, &whole));
    }

    #[test]
    fn applying_adds_removes_and_replaces() {
        let mut settings = DeliveryRepoSettings {
            checks: vec!["npm test".into(), "old".into()],
            ..Default::default()
        };
        apply(
            &mut settings,
            &DeliverySettingsChanges {
                checks_add: vec!["npm test".into(), "new".into()],
                checks_remove: vec!["old".into()],
                qa_commands_add: vec!["qa".into()],
                bootstrap: Some("npm ci".into()),
                qa_browser: Some(true),
                verification: Some("e2e".into()),
            },
        );
        assert_eq!(settings.checks, ["npm test", "new"]);
        assert_eq!(settings.qa_commands, ["qa"]);
        assert_eq!(settings.bootstrap.as_deref(), Some("npm ci"));
        assert!(settings.qa_browser);
        assert_eq!(settings.verification, "e2e");
    }
}
