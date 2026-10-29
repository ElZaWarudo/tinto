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
use super::service::{valid_env_name, DeliveryService};
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
        setup_add: trimmed(&changes.setup_add),
        setup_remove: trimmed(&changes.setup_remove),
        probes_add: trimmed(&changes.probes_add),
        bootstrap: changes
            .bootstrap
            .as_deref()
            .map(|text| text.trim().to_string()),
        qa_browser: changes.qa_browser,
        verification: changes
            .verification
            .as_deref()
            .map(|text| text.trim().to_string()),
        check_env: changes
            .check_env
            .iter()
            .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
            .collect(),
    }
}

fn is_empty(changes: &DeliverySettingsChanges) -> bool {
    changes.checks_add.is_empty()
        && changes.checks_remove.is_empty()
        && changes.qa_commands_add.is_empty()
        && changes.setup_add.is_empty()
        && changes.setup_remove.is_empty()
        && changes.probes_add.is_empty()
        && changes.bootstrap.is_none()
        && changes.qa_browser.is_none()
        && changes.verification.is_none()
        && changes.check_env.is_empty()
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
    commands("setup_add", &changes.setup_add, &settings.setup);
    commands(
        "probes_add",
        &changes.probes_add,
        &settings.environment_probes,
    );
    if let Some(bootstrap) = &changes.bootstrap {
        if let Some(problem) = command_problem(bootstrap, task) {
            problems.push(format!("bootstrap «{bootstrap}»: {problem}"));
        }
    }
    let mut removed = |field: &str, items: &[String], current: &[String], added: &[String]| {
        for (index, item) in items.iter().enumerate() {
            if items[..index].contains(item) {
                problems.push(format!("{field} «{item}»: está repetido"));
            } else if !current.contains(item) {
                problems.push(format!("{field} «{item}»: no está en los ajustes"));
            } else if added.contains(item) {
                problems.push(format!("{field} «{item}»: también se añade"));
            }
        }
    };
    removed(
        "checks_remove",
        &changes.checks_remove,
        &settings.checks,
        &changes.checks_add,
    );
    removed(
        "setup_remove",
        &changes.setup_remove,
        &settings.setup,
        &changes.setup_add,
    );
    if changes
        .verification
        .as_deref()
        .is_some_and(|text| text.is_empty())
    {
        problems.push("verification: está vacío".to_string());
    }
    for (name, value) in &changes.check_env {
        if !valid_env_name(name) {
            problems.push(format!(
                "check_env «{name}»: no es un nombre de variable (letras, dígitos y _, sin empezar por dígito)"
            ));
        } else if settings.check_env.get(name) == Some(value) {
            problems.push(format!("check_env «{name}»: ya tiene ese valor"));
        }
    }
    problems
}

/// Whether every change in `part` is one of `whole`.
fn is_part_of(part: &DeliverySettingsChanges, whole: &DeliverySettingsChanges) -> bool {
    let within = |items: &[String], all: &[String]| items.iter().all(|item| all.contains(item));
    within(&part.checks_add, &whole.checks_add)
        && within(&part.checks_remove, &whole.checks_remove)
        && within(&part.qa_commands_add, &whole.qa_commands_add)
        && within(&part.setup_add, &whole.setup_add)
        && within(&part.setup_remove, &whole.setup_remove)
        && within(&part.probes_add, &whole.probes_add)
        && (part.bootstrap.is_none() || part.bootstrap == whole.bootstrap)
        && (part.qa_browser.is_none() || part.qa_browser == whole.qa_browser)
        && (part.verification.is_none() || part.verification == whole.verification)
        && part
            .check_env
            .iter()
            .all(|(name, value)| whole.check_env.get(name) == Some(value))
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
    for item in &changes.setup_add {
        if !settings.setup.contains(item) {
            settings.setup.push(item.clone());
        }
    }
    settings
        .setup
        .retain(|item| !changes.setup_remove.contains(item));
    for item in &changes.probes_add {
        if !settings.environment_probes.contains(item) {
            settings.environment_probes.push(item.clone());
        }
        if !settings.checks.contains(item) {
            settings.checks.push(item.clone());
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
    settings.check_env.extend(changes.check_env.clone());
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
        // Past the brainstorm, when the repo lets Tinto, it applies whole.
        if self.run(run_id)?.unattended && settings.autonomy.settings_proposals {
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
            package: None,
            worktree: PathBuf::from("C:/code/agentos-wt/AGOS-474"),
            branch: "fix/AGOS-474".into(),
            base_ref: "develop".into(),
            base_commit: "abc".into(),
            state: "tests".into(),
            contract_version: 1,
            read_dirs: Vec::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
            removed_at_ms: None,
            archived_at_ms: None,
            blockers: Vec::new(),
            blocked_reason: String::new(),
            unblocked: None,
            checks: Vec::new(),
            setup: Vec::new(),
            parent: None,
            parent_read_at_ms: None,
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
    fn an_added_probe_is_a_check_too_so_it_runs() {
        let mut settings = DeliveryRepoSettings {
            checks: vec!["npm test".into()],
            ..Default::default()
        };
        let changes = DeliverySettingsChanges {
            probes_add: vec!["npm test".into(), "nc -z 127.0.0.1 27419".into()],
            ..Default::default()
        };
        apply(&mut settings, &changes);
        assert_eq!(settings.environment_probes, changes.probes_add);
        assert_eq!(settings.checks, changes.probes_add);
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
            setup: vec!["npm ci".into(), "gone".into()],
            ..Default::default()
        };
        apply(
            &mut settings,
            &DeliverySettingsChanges {
                checks_add: vec!["npm test".into(), "new".into()],
                checks_remove: vec!["old".into()],
                qa_commands_add: vec!["qa".into()],
                setup_add: vec!["make bindings".into()],
                setup_remove: vec!["gone".into()],
                probes_add: Vec::new(),
                bootstrap: Some("npm ci".into()),
                qa_browser: Some(true),
                verification: Some("e2e".into()),
                check_env: [("DB".to_string(), "mongodb://h/agos{task}".to_string())].into(),
            },
        );
        assert_eq!(settings.checks, ["npm test", "new"]);
        assert_eq!(settings.qa_commands, ["qa"]);
        assert_eq!(settings.setup, ["npm ci", "make bindings"]);
        assert_eq!(settings.bootstrap.as_deref(), Some("npm ci"));
        assert!(settings.qa_browser);
        assert_eq!(settings.verification, "e2e");
        assert_eq!(settings.check_env["DB"], "mongodb://h/agos{task}");
    }

    #[test]
    fn variables_for_checks_need_a_valid_name_and_a_new_value() {
        let settings = DeliveryRepoSettings {
            check_env: [("DB".to_string(), "mongodb://h/agos".to_string())].into(),
            ..Default::default()
        };
        let changes = DeliverySettingsChanges {
            check_env: [
                ("DB".to_string(), "mongodb://h/agos".to_string()),
                ("1DB".to_string(), "x".to_string()),
                ("TEST_DB".to_string(), "mongodb://h/agos{task}".to_string()),
            ]
            .into(),
            ..Default::default()
        };
        let found = problems(&changes, &settings, &task());
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].contains("1DB") && found[0].contains("nombre"));
        assert!(found[1].contains("«DB»") && found[1].contains("ya tiene ese valor"));
        let part = DeliverySettingsChanges {
            check_env: [("TEST_DB".to_string(), "mongodb://h/agos{task}".to_string())].into(),
            ..Default::default()
        };
        assert!(is_part_of(&part, &changes));
        let changed = DeliverySettingsChanges {
            check_env: [("TEST_DB".to_string(), "other".to_string())].into(),
            ..Default::default()
        };
        assert!(!is_part_of(&changed, &changes));
    }
}
