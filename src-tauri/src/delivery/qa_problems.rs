//! The problems QA found, as the user decides on them: fix with QA's
//! suggestion or their own solution, or dismiss. Sent problems reach the
//! coordinator as one change, and the next QA checks exactly those.
//!
//! QA writes them in its visual changes record (`**Problems:**` and
//! `**Suggested solutions:**` of each change, labelled `P1`, `P2`…); Tinto
//! reads them when the QA job ends and numbers them per task (`#1`, `#2`…).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::model::{
    DeliveryJob, DeliveryJobStatus, DeliveryQaProblem, DeliveryQaProblemStatus, DeliveryTask,
};
use super::service::{DeliveryService, QA_ROLE};
use super::tasks::plain_path;
use super::visual_record;
use super::{now_ms, DeliveryError};

/// A problem as the record states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordProblem {
    /// `P3` as the record names it.
    pub label: String,
    /// The change it is about (the record's section).
    pub change: String,
    /// Where it happens.
    pub place: String,
    pub text: String,
    /// QA's proposals for it, the recommended first.
    pub suggestions: Vec<String>,
}

/// What the user sends for one problem.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct QaProblemChoice {
    pub problem_id: String,
    /// The fix: one of QA's suggestions or the user's own words.
    pub solution: String,
}

/// What the coordinator says happened to a sent problem.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct QaProblemUpdate {
    pub problem_id: String,
    pub status: DeliveryQaProblemStatus,
    #[serde(default)]
    pub note: String,
}

fn empty(text: &str) -> bool {
    let text = text.trim().trim_matches('.').to_ascii_lowercase();
    text.is_empty() || matches!(text.as_str(), "none" | "ninguno" | "ninguna" | "n/a" | "-")
}

/// The items of a labelled paragraph: its list items, or the paragraph.
fn items(text: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut loose = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let item = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| {
                let (number, rest) = line.split_once(". ")?;
                number.chars().all(|c| c.is_ascii_digit()).then_some(rest)
            });
        match item {
            Some(item) => items.push(item.trim().to_string()),
            None if !items.is_empty() => {
                let last = items.last_mut().expect("an item");
                last.push(' ');
                last.push_str(line);
            }
            None => loose.push(line.to_string()),
        }
    }
    if items.is_empty() && !loose.is_empty() {
        items.push(loose.join(" "));
    }
    items.into_iter().filter(|item| !empty(item)).collect()
}

/// `P3` and the rest of an item that starts with its label.
fn labelled(item: &str) -> Option<(String, String)> {
    let bare = item.trim_start_matches(['*', '`']);
    let digits: String = bare
        .strip_prefix('P')?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        return None;
    }
    let rest = bare[1 + digits.len()..]
        .trim_start_matches(['*', '`'])
        .trim_start()
        .trim_start_matches([':', '.', ')', '—', '-'])
        .trim();
    Some((format!("P{digits}"), rest.to_string()))
}

/// The problems of a visual changes record.
pub fn parse_record(record: &str) -> Vec<RecordProblem> {
    struct Section {
        change: String,
        place: String,
        problems: String,
        suggestions: String,
    }
    let mut sections: Vec<Section> = Vec::new();
    // Which labelled paragraph the following lines belong to.
    let mut field: Option<&str> = None;
    for line in record.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("## ") {
            let title = title.trim();
            let change = match title.split_once(". ") {
                Some((number, rest)) if number.chars().all(|c| c.is_ascii_digit()) => rest,
                _ => title,
            };
            sections.push(Section {
                change: change.to_string(),
                place: String::new(),
                problems: String::new(),
                suggestions: String::new(),
            });
            field = None;
            continue;
        }
        let Some(section) = sections.last_mut() else {
            continue;
        };
        if trimmed.starts_with('#') || trimmed.starts_with("![") || trimmed.starts_with('|') {
            if let Some(place) = trimmed
                .strip_prefix("| Where |")
                .map(|rest| rest.trim_end_matches('|').trim())
            {
                section.place = place.to_string();
            }
            field = None;
            continue;
        }
        let starts = |label: &str| trimmed.strip_prefix(label).map(str::to_string);
        if let Some(rest) = starts("**Problems:**").or_else(|| starts("**Problemas:**")) {
            field = Some("problems");
            section.problems.push_str(&rest);
            section.problems.push('\n');
            continue;
        }
        if let Some(rest) =
            starts("**Suggested solutions:**").or_else(|| starts("**Soluciones sugeridas:**"))
        {
            field = Some("suggestions");
            section.suggestions.push_str(&rest);
            section.suggestions.push('\n');
            continue;
        }
        if trimmed.starts_with("**") {
            field = None;
            continue;
        }
        match field {
            Some("problems") => {
                section.problems.push_str(line);
                section.problems.push('\n');
            }
            Some("suggestions") => {
                section.suggestions.push_str(line);
                section.suggestions.push('\n');
            }
            _ => {}
        }
    }
    let mut problems: Vec<RecordProblem> = Vec::new();
    for section in sections {
        let suggestions: Vec<(Option<String>, String)> = items(&section.suggestions)
            .into_iter()
            .map(|item| match labelled(&item) {
                Some((label, text)) => (Some(label), text),
                None => (None, item),
            })
            .collect();
        for item in items(&section.problems) {
            let (label, text) = labelled(&item)
                .unwrap_or_else(|| (format!("P{}", problems.len() + 1), item.clone()));
            // Suggestions for this problem; unlabelled ones are the change's.
            let mine: Vec<String> = suggestions
                .iter()
                .filter(|(target, _)| target.as_deref().is_none_or(|target| target == label))
                .map(|(_, text)| text.clone())
                .collect();
            problems.push(RecordProblem {
                label,
                change: section.change.clone(),
                place: section.place.clone(),
                text,
                suggestions: mine,
            });
        }
    }
    problems
}

fn same_problem(a: &str, b: &str) -> bool {
    let clean = |text: &str| {
        text.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    clean(a) == clean(b)
}

/// The job no longer runs and will not resume.
fn ended(job: &DeliveryJob) -> bool {
    !job.status.is_open() && job.status != DeliveryJobStatus::Paused
}

fn invalid(message: &str) -> DeliveryError {
    DeliveryError::new("invalid_arguments", message)
}

impl DeliveryService {
    pub fn task_qa_problems(&self, task_id: &str) -> Result<Vec<DeliveryQaProblem>, DeliveryError> {
        self.store()?.task_qa_problems(task_id)
    }

    fn qa_problem(&self, id: &str) -> Result<DeliveryQaProblem, DeliveryError> {
        self.store()?
            .qa_problem(id)?
            .ok_or_else(|| DeliveryError::not_found("el problema", id))
    }

    fn next_number(&self, task_id: &str) -> Result<u32, DeliveryError> {
        Ok(self
            .task_qa_problems(task_id)?
            .iter()
            .map(|problem| problem.number)
            .max()
            .unwrap_or(0)
            + 1)
    }

    /// The problems a QA job's record lists. What the user had not decided
    /// on from an earlier QA gives way to this one; what they dismissed is
    /// not brought back.
    pub(super) fn record_qa_problems(
        &self,
        task: &DeliveryTask,
        job_id: &str,
        attempt: u32,
        record: &str,
    ) -> Result<usize, DeliveryError> {
        let found = parse_record(record);
        let known = self.task_qa_problems(&task.id)?;
        let mut number = self.next_number(&task.id)?;
        let now = now_ms();
        let store = self.store()?;
        for mut earlier in known.iter().cloned() {
            if earlier.job_id.is_some() && earlier.status == DeliveryQaProblemStatus::Open {
                earlier.status = DeliveryQaProblemStatus::Superseded;
                earlier.updated_at_ms = now;
                store.put_qa_problem(&earlier)?;
            }
        }
        let mut added = 0;
        for problem in found {
            let dismissed = known.iter().any(|known| {
                known.status == DeliveryQaProblemStatus::Dismissed
                    && same_problem(&known.text, &problem.text)
            });
            if dismissed {
                continue;
            }
            store.put_qa_problem(&DeliveryQaProblem {
                id: uuid::Uuid::new_v4().to_string(),
                task_id: task.id.clone(),
                job_id: Some(job_id.to_string()),
                attempt: Some(attempt),
                number,
                label: problem.label,
                change: problem.change,
                place: problem.place,
                text: problem.text,
                suggestions: problem.suggestions,
                status: DeliveryQaProblemStatus::Open,
                solution: None,
                reason: None,
                notes: Vec::new(),
                created_at_ms: now,
                updated_at_ms: now,
            })?;
            number += 1;
            added += 1;
        }
        drop(store);
        self.notify();
        Ok(added)
    }

    /// Each task's latest QA record whose problems were never read (it ended
    /// under a Tinto that did not read them) is read now, at startup.
    pub(super) fn record_missed_qa_problems(&self) -> Result<(), DeliveryError> {
        let (tasks, jobs, problems) = {
            let store = self.store()?;
            (store.tasks()?, store.jobs()?, store.qa_problems()?)
        };
        for task in tasks {
            let latest = jobs
                .iter()
                .filter(|job| job.task_id == task.id && job.role == QA_ROLE)
                .max_by_key(|job| job.created_at_ms);
            let Some(job) = latest else { continue };
            let Some(page) = &job.visual_record else {
                continue;
            };
            if problems
                .iter()
                .any(|problem| problem.job_id.as_deref() == Some(job.id.as_str()))
            {
                continue;
            }
            let record = match &job.visual_record_source {
                Some(source) => PathBuf::from(source),
                None => Path::new(page).with_extension("md"),
            };
            if record.extension().is_some_and(|ext| ext == "md") {
                if let Ok(text) = std::fs::read_to_string(record) {
                    self.record_qa_problems(&task, &job.id, job.attempt, &text)?;
                }
            }
        }
        Ok(())
    }

    /// Finds the QA job's record in `qa_dir`, renders it and saves on the
    /// job its page, the file it came from, the other candidates and what
    /// it lacks of the guide; or why there is none. With `read_problems`,
    /// the problems a Markdown record lists wait for the user.
    pub(super) fn save_visual_record(
        &self,
        task: &DeliveryTask,
        job_id: &str,
        qa_dir: &Path,
        read_problems: bool,
    ) -> Result<DeliveryJob, DeliveryError> {
        let found = visual_record::find(qa_dir, &task.key);
        let text = found
            .as_ref()
            .filter(|found| found.source.extension().is_some_and(|ext| ext == "md"))
            .and_then(|found| std::fs::read_to_string(&found.source).ok());
        if let (true, Some(text)) = (read_problems, &text) {
            let attempt = self.job(job_id)?.attempt;
            self.record_qa_problems(task, job_id, attempt, text)?;
        }
        let rendered = found
            .as_ref()
            .map(|found| visual_record::page_for(&found.source));
        let store = self.store()?;
        let Some(mut job) = store.job(job_id)? else {
            return Err(DeliveryError::not_found("el trabajo", job_id));
        };
        let name = |path: &PathBuf| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        job.visual_record_source = found.as_ref().map(|found| plain_path(&found.source));
        job.visual_record_extras = found
            .as_ref()
            .map(|found| found.extras.iter().map(name).collect())
            .unwrap_or_default();
        job.visual_record_warnings = text
            .as_deref()
            .map(visual_record::missing_parts)
            .unwrap_or_default();
        (job.visual_record, job.visual_record_error) = match rendered {
            Some(Ok(page)) => (Some(plain_path(&page)), None),
            Some(Err(error)) => (None, Some(error)),
            None => (
                None,
                Some(format!(
                    "el trabajo no escribió {}.md en su carpeta de QA",
                    task.key
                )),
            ),
        };
        store.put_job(&job)?;
        Ok(job)
    }

    /// The user's "Regenerar registro": an ended QA job's record, found and
    /// rendered again, for one fixed or copied into its QA folder later.
    pub fn regenerate_visual_record(&self, job_id: &str) -> Result<DeliveryJob, DeliveryError> {
        let job = self.job(job_id)?;
        if job.role != QA_ROLE {
            return Err(invalid("solo un trabajo de QA tiene registro visual"));
        }
        if !ended(&job) {
            return Err(DeliveryError::new(
                "job_running",
                "la QA aún no terminó; su registro se genera al terminar",
            ));
        }
        let job = self.refresh_visual_record(&job)?;
        self.notify();
        Ok(job)
    }

    /// An ended QA job whose record was missing and that now has one: what
    /// read_job and the overview render before they show it.
    pub(super) fn record_appeared(&self, task: &DeliveryTask, job: &DeliveryJob) -> bool {
        if job.role != QA_ROLE || !ended(job) || job.visual_record.is_some() {
            return false;
        }
        let Ok(paths) = (self.inner.paths_for)(&job.id) else {
            return false;
        };
        visual_record::find(&paths.qa_output, &task.key)
            .is_some_and(|found| Some(plain_path(&found.source)) != job.visual_record_source)
    }

    /// A job as read_job shows it, with a record that appeared since it ended.
    pub fn read_job(&self, job_id: &str) -> Result<DeliveryJob, DeliveryError> {
        let job = self.job(job_id)?;
        match self.task(&job.task_id) {
            Ok(task) if self.record_appeared(&task, &job) => self.refresh_visual_record(&job),
            _ => Ok(job),
        }
    }

    /// Renders an ended QA job's record again. Its problems are read only
    /// when it is the task's latest QA and none were read from it yet.
    pub(super) fn refresh_visual_record(
        &self,
        job: &DeliveryJob,
    ) -> Result<DeliveryJob, DeliveryError> {
        let task = self.task(&job.task_id)?;
        let paths = (self.inner.paths_for)(&job.id)?;
        let read_problems = {
            let store = self.store()?;
            let latest = store
                .task_jobs(&task.id)?
                .into_iter()
                .filter(|other| other.role == QA_ROLE)
                .max_by_key(|other| other.created_at_ms);
            latest.is_some_and(|latest| latest.id == job.id)
                && !store
                    .task_qa_problems(&task.id)?
                    .iter()
                    .any(|problem| problem.job_id.as_deref() == Some(job.id.as_str()))
        };
        self.save_visual_record(&task, &job.id, &paths.qa_output, read_problems)
    }

    /// A problem QA missed, as the user describes it.
    pub fn report_qa_problem(
        &self,
        task_id: &str,
        text: &str,
        place: &str,
    ) -> Result<DeliveryQaProblem, DeliveryError> {
        let task = self.task(task_id)?;
        if text.trim().is_empty() {
            return Err(invalid("describe el problema"));
        }
        let now = now_ms();
        let problem = DeliveryQaProblem {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: task.id,
            job_id: None,
            attempt: None,
            number: self.next_number(task_id)?,
            label: String::new(),
            change: String::new(),
            place: place.trim().to_string(),
            text: text.trim().to_string(),
            suggestions: Vec::new(),
            status: DeliveryQaProblemStatus::Open,
            solution: None,
            reason: None,
            notes: Vec::new(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.store()?.put_qa_problem(&problem)?;
        self.notify();
        Ok(problem)
    }

    /// The user dismisses a problem: the next QA does not report it again.
    pub fn dismiss_qa_problem(
        &self,
        id: &str,
        reason: &str,
    ) -> Result<DeliveryQaProblem, DeliveryError> {
        let mut problem = self.qa_problem(id)?;
        if problem.status != DeliveryQaProblemStatus::Open {
            return Err(invalid("solo se descarta un problema que aún no se envió"));
        }
        problem.status = DeliveryQaProblemStatus::Dismissed;
        // Saying why is optional.
        problem.reason = Some(reason.trim().to_string()).filter(|reason| !reason.is_empty());
        problem.updated_at_ms = now_ms();
        self.store()?.put_qa_problem(&problem)?;
        self.notify();
        Ok(problem)
    }

    /// The user sends problems to fix, each with its solution, as one change
    /// for the coordinator.
    pub fn send_qa_problems(
        &self,
        task_id: &str,
        choices: &[QaProblemChoice],
    ) -> Result<Vec<DeliveryQaProblem>, DeliveryError> {
        if choices.is_empty() {
            return Err(invalid("elige al menos un problema"));
        }
        let mut sent = Vec::new();
        for choice in choices {
            let mut problem = self.qa_problem(&choice.problem_id)?;
            if problem.task_id != task_id {
                return Err(invalid("el problema es de otra tarea"));
            }
            if problem.status != DeliveryQaProblemStatus::Open {
                return Err(invalid(&format!(
                    "el problema #{} ya se envió o se descartó",
                    problem.number
                )));
            }
            if choice.solution.trim().is_empty() {
                return Err(invalid(&format!(
                    "di cómo arreglar el problema #{}",
                    problem.number
                )));
            }
            problem.status = DeliveryQaProblemStatus::Sent;
            problem.solution = Some(choice.solution.trim().to_string());
            problem.updated_at_ms = now_ms();
            sent.push(problem);
        }
        let numbers = sent
            .iter()
            .map(|problem| format!("#{}", problem.number))
            .collect::<Vec<_>>()
            .join(", ");
        let store = self.store()?;
        for problem in &sent {
            store.put_qa_problem(problem)?;
        }
        store.record_event(now_ms(), "qa_problems_sent", Some(task_id), None, &numbers)?;
        super::rounds::for_qa_problems(&store, task_id, &sent)?;
        drop(store);
        self.notify();
        Ok(sent)
    }

    /// The coordinator says how sent problems went: being fixed, fixed (QA
    /// checked it), or still failing.
    pub fn update_qa_problems(
        &self,
        task_id: &str,
        updates: &[QaProblemUpdate],
    ) -> Result<Vec<DeliveryQaProblem>, DeliveryError> {
        let mut updated = Vec::new();
        for update in updates {
            let mut problem = self.qa_problem(&update.problem_id)?;
            if problem.task_id != task_id {
                return Err(invalid("el problema es de otra tarea"));
            }
            let allowed = matches!(
                update.status,
                DeliveryQaProblemStatus::Fixing
                    | DeliveryQaProblemStatus::Verified
                    | DeliveryQaProblemStatus::StillFailing
            ) && matches!(
                problem.status,
                DeliveryQaProblemStatus::Sent
                    | DeliveryQaProblemStatus::Fixing
                    | DeliveryQaProblemStatus::StillFailing
            );
            if !allowed {
                return Err(invalid(&format!(
                    "el problema #{} no se puede pasar a ese estado: solo los enviados pasan a fixing, verified o still_failing",
                    problem.number
                )));
            }
            problem.status = update.status;
            if !update.note.trim().is_empty() {
                problem.notes.push(update.note.trim().to_string());
            }
            problem.updated_at_ms = now_ms();
            updated.push(problem);
        }
        let store = self.store()?;
        for problem in &updated {
            store.put_qa_problem(problem)?;
        }
        drop(store);
        self.notify();
        Ok(updated)
    }

    /// For a QA job's instructions: what to check again and what not to
    /// report.
    pub(super) fn qa_followup(&self, task_id: &str) -> Result<Vec<String>, DeliveryError> {
        Ok(self
            .task_qa_problems(task_id)?
            .into_iter()
            .filter_map(|problem| match problem.status {
                DeliveryQaProblemStatus::Sent
                | DeliveryQaProblemStatus::Fixing
                | DeliveryQaProblemStatus::StillFailing => Some(format!(
                    "Check #{} again: «{}». The fix the user chose: «{}». Report it under Problems with the same text if it still happens; otherwise say in Observations that #{} is fixed.",
                    problem.number,
                    problem.text,
                    problem.solution.unwrap_or_default(),
                    problem.number
                )),
                DeliveryQaProblemStatus::Dismissed => Some(format!(
                    "Do not report «{}» again: the user decided it is not a problem{}.",
                    problem.text,
                    problem
                        .reason
                        .map(|reason| format!(" ({reason})"))
                        .unwrap_or_default()
                )),
                _ => None,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD: &str = "# K-1 — Visual changes: errors\n\n## 1. Error toast\n\n| | |\n| --- | --- |\n| Where | Run panel → Retry |\n| Behaviour | A toast says it |\n\n**Mobile — error**\n\n![](K-1-1-error-mobile.png)\n\n**Observations:** fine on desktop\n\n**Problems:**\n- P1: The toast covers the Retry button on mobile.\n- P2: The toast stays 8 s.\n\n**Suggested solutions:**\n- P1: Move the toast to the top on narrow screens.\n- P1: Keep the error in the panel instead.\n- P2: Shorten it to 3 s.\n\n## 2. Header\n\n| | |\n| --- | --- |\n| Where | npm test output |\n\n**Problems:** The header does not say which services are mocked.\n\n**Suggested solutions:** Add (mock) next to each one.\n\n## 3. Docs\n\n**Problems:** None\n\n**Suggested solutions:** None\n";

    #[test]
    fn a_record_gives_each_problem_with_its_own_suggestions() {
        let problems = parse_record(RECORD);
        assert_eq!(problems.len(), 3, "{problems:#?}");
        assert_eq!(problems[0].label, "P1");
        assert_eq!(problems[0].change, "Error toast");
        assert_eq!(problems[0].place, "Run panel → Retry");
        assert_eq!(
            problems[0].text,
            "The toast covers the Retry button on mobile."
        );
        assert_eq!(
            problems[0].suggestions,
            [
                "Move the toast to the top on narrow screens.",
                "Keep the error in the panel instead."
            ]
        );
        assert_eq!(problems[1].suggestions, ["Shorten it to 3 s."]);
        // Unlabelled: numbered on, with the change's suggestion.
        assert_eq!(problems[2].label, "P3");
        assert_eq!(
            problems[2].text,
            "The header does not say which services are mocked."
        );
        assert_eq!(problems[2].suggestions, ["Add (mock) next to each one."]);
    }

    #[test]
    fn labels_are_read_whatever_their_punctuation() {
        assert_eq!(
            labelled("**P12** — text"),
            Some(("P12".to_string(), "text".to_string()))
        );
        assert_eq!(
            labelled("P3) text"),
            Some(("P3".to_string(), "text".to_string()))
        );
        assert_eq!(labelled("Plain text"), None);
    }
}
