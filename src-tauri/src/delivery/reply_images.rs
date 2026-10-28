//! QA screenshots on GitHub: in the PR's description and in replies to
//! reviewers. The user picks some of the task's QA screenshots for the PR
//! step or a reply; when it is published, Tinto pushes them to a branch of
//! the task's own on GitHub (`tinto/qa-evidence/<branch>`, with no history,
//! never merged) and the text shows them by that commit. The branch goes
//! once the PR is merged or closed.

use serde::Serialize;

use super::model::{
    DeliveryApproval, DeliveryApprovalStatus, DeliveryJobStatus, DeliveryReplyImage, DeliveryRung,
    DeliveryTask,
};
use super::prs::task_github_repo;
use super::service::{DeliveryService, QA_ROLE};
use super::DeliveryError;

/// A screenshot one of the task's QA jobs saved.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryQaScreenshot {
    pub job_id: String,
    pub attempt: u32,
    pub file: String,
    /// The image itself, for the reply editor to show.
    pub data_uri: String,
}

const IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// The branch the task's screenshots live on.
pub fn evidence_branch(task: &DeliveryTask) -> String {
    format!("tinto/qa-evidence/{}", task.branch.replace('/', "-"))
}

fn invalid(message: &str) -> DeliveryError {
    DeliveryError::new("invalid_approval", message)
}

impl DeliveryService {
    /// The screenshots of the task's ended QA jobs, newest QA first.
    pub fn qa_screenshots(
        &self,
        task_id: &str,
    ) -> Result<Vec<DeliveryQaScreenshot>, DeliveryError> {
        let mut jobs = self.store()?.task_jobs(task_id)?;
        jobs.retain(|job| job.role == QA_ROLE && job.status == DeliveryJobStatus::Finished);
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at_ms));
        let mut shots = Vec::new();
        for job in jobs {
            let folder = (self.inner.paths_for)(&job.id)?.qa_output;
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            let mut files: Vec<String> = entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .filter(|name| {
                    name.rsplit_once('.').is_some_and(|(_, ext)| {
                        IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
                    })
                })
                .collect();
            files.sort();
            for file in files {
                if let Ok(data_uri) = super::visual_record::data_uri(&file, &folder) {
                    shots.push(DeliveryQaScreenshot {
                        job_id: job.id.clone(),
                        attempt: job.attempt,
                        file,
                        data_uri,
                    });
                }
            }
        }
        Ok(shots)
    }

    /// The QA screenshots a PR description or a reply names where it shows
    /// them, `![what it shows](qa:<file>)`: each from the newest QA of the
    /// task that has it. A name no QA of the task has is refused.
    pub(super) fn named_screenshots(
        &self,
        task_id: &str,
        body: &str,
    ) -> Result<Vec<DeliveryReplyImage>, DeliveryError> {
        let named = qa_refs(body);
        if named.is_empty() {
            return Ok(Vec::new());
        }
        let available = self.qa_screenshots(task_id)?;
        let mut images: Vec<DeliveryReplyImage> = Vec::new();
        for file in named {
            let Some(shot) = available.iter().find(|shot| shot.file == file) else {
                let mut names: Vec<&str> =
                    available.iter().map(|shot| shot.file.as_str()).collect();
                names.dedup();
                return Err(invalid(&format!(
                    "{file} no es una captura de la QA de esta tarea; las que hay: {}",
                    if names.is_empty() {
                        "ninguna".to_string()
                    } else {
                        names.join(", ")
                    }
                )));
            };
            if !images.iter().any(|image| image.file == file) {
                images.push(DeliveryReplyImage {
                    job_id: shot.job_id.clone(),
                    file: file.to_string(),
                });
            }
        }
        Ok(images)
    }

    /// The QA screenshots the user chose for a PR step or a reply not
    /// approved yet.
    pub fn set_reply_images(
        &self,
        approval_id: &str,
        images: Vec<DeliveryReplyImage>,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        if !matches!(approval.rung, DeliveryRung::PrReply | DeliveryRung::Pr)
            || approval.status != DeliveryApprovalStatus::Pending
        {
            return Err(invalid(
                "solo se eligen capturas para un PR o una respuesta que aún no se aprobó",
            ));
        }
        let available = self.qa_screenshots(&approval.task_id)?;
        if let Some(missing) = images.iter().find(|image| {
            !available
                .iter()
                .any(|shot| shot.job_id == image.job_id && shot.file == image.file)
        }) {
            return Err(invalid(&format!(
                "{} no es una captura de la QA de esta tarea",
                missing.file
            )));
        }
        approval.images = images;
        self.save_approval(&approval, "pr_reply_edited")?;
        Ok(approval)
    }

    /// The text of a PR step or a reply as it is published: its images,
    /// pushed to the task's evidence branch, each shown by the commit that
    /// has it where the text names it (`qa:<file>`), else after it (in a
    /// PR's description, at the end of its "How to test" section). A
    /// screenshot it names that the user took out goes with its mention.
    pub(super) fn text_with_images(
        &self,
        approval: &DeliveryApproval,
    ) -> Result<String, DeliveryError> {
        let mut body = without_qa_refs(approval.body.trim(), &approval.images);
        if approval.images.is_empty() {
            return Ok(body);
        }
        let task = self.task(&approval.task_id)?;
        let repo = task_github_repo(&task).ok_or_else(|| {
            DeliveryError::new("github_unavailable", "el remoto origin no está en GitHub")
        })?;
        let branch = evidence_branch(&task);
        let github = self.github_client();
        let mut shown = Vec::new();
        for image in &approval.images {
            let folder = (self.inner.paths_for)(&image.job_id)?.qa_output;
            let bytes = std::fs::read(folder.join(&image.file))
                .map_err(|_| invalid(&format!("la captura {} ya no está en la QA", image.file)))?;
            let path = format!("{}/{}/{}", task.key, short_id(&image.job_id), image.file);
            // Shown already (a step asked again after a failed attempt).
            if body.contains(&path) {
                continue;
            }
            let commit = github.upload_file(
                &repo,
                &branch,
                &path,
                &bytes,
                &format!("QA de {}: {}", task.key, image.file),
            )?;
            let url = format!("https://github.com/{repo}/blob/{commit}/{path}?raw=true");
            let named = format!("(qa:{})", image.file);
            if body.contains(&named) {
                body = body.replace(&named, &format!("({url})"));
            } else {
                shown.push(format!("![{}]({url})", image.file));
            }
        }
        if shown.is_empty() {
            return Ok(body);
        }
        let images = shown.join("\n\n");
        Ok(match approval.rung {
            DeliveryRung::Pr => in_test_section(&body, &images),
            _ => format!("{body}\n\n{images}"),
        })
    }

    /// Once the task's PR is merged or closed, its evidence branch goes,
    /// if its PR or a reply showed images.
    pub(super) fn drop_evidence(&self, task: &DeliveryTask) {
        let showed = self
            .store()
            .and_then(|store| store.approvals())
            .is_ok_and(|approvals| {
                approvals.iter().any(|approval| {
                    approval.task_id == task.id
                        && matches!(approval.rung, DeliveryRung::PrReply | DeliveryRung::Pr)
                        && approval.status == DeliveryApprovalStatus::Executed
                        && !approval.images.is_empty()
                })
            });
        let Some(repo) = task_github_repo(task).filter(|_| showed) else {
            return;
        };
        if let Err(error) = self
            .github_client()
            .delete_branch(&repo, &evidence_branch(task))
        {
            eprintln!(
                "tinto: delivery could not delete the QA evidence branch of {}: {error}",
                task.key
            );
        }
    }
}

/// The files a text names as `![…](qa:<file>)`, in order.
fn qa_refs(body: &str) -> Vec<&str> {
    body.match_indices("](qa:")
        .filter_map(|(at, open)| {
            let rest = &body[at + open.len()..];
            rest.find(')').map(|end| rest[..end].trim())
        })
        .filter(|file| !file.is_empty())
        .collect()
}

/// `body` without the `![…](qa:<file>)` of files not among `images`.
fn without_qa_refs(body: &str, images: &[DeliveryReplyImage]) -> String {
    let mut text = body.to_string();
    for file in qa_refs(body) {
        if images.iter().any(|image| image.file == file) {
            continue;
        }
        let target = format!("](qa:{file})");
        while let Some(end) = text.find(&target) {
            let Some(start) = text[..end].rfind("![") else {
                break;
            };
            text.replace_range(start..end + target.len(), "");
        }
    }
    text
}

/// `images` at the end of the description's "How to test" section (in
/// English or Spanish), or at its end when it has none.
fn in_test_section(body: &str, images: &str) -> String {
    const TITLES: [&str; 6] = [
        "how to test",
        "testing",
        "test plan",
        "cómo probar",
        "como probar",
        "pruebas",
    ];
    let lines: Vec<&str> = body.lines().collect();
    let heading = |line: &str| {
        let level = line.chars().take_while(|c| *c == '#').count();
        (level > 0 && line[level..].starts_with(' '))
            .then(|| (level, line[level..].trim().to_lowercase()))
    };
    let start = lines.iter().position(|line| {
        heading(line).is_some_and(|(_, title)| TITLES.iter().any(|name| title.starts_with(name)))
    });
    let Some(start) = start else {
        return format!("{body}\n\n{images}");
    };
    let level = heading(lines[start]).map_or(2, |(level, _)| level);
    let end = lines[start + 1..]
        .iter()
        .position(|line| heading(line).is_some_and(|(other, _)| other <= level))
        .map_or(lines.len(), |at| start + 1 + at);
    let section = lines[..end].join("\n").trim_end().to_string();
    let rest = lines[end..].join("\n");
    if rest.is_empty() {
        format!("{section}\n\n{images}")
    } else {
        format!("{section}\n\n{images}\n\n{rest}")
    }
}

/// The first part of a job id, enough to keep two QA attempts apart.
fn short_id(job_id: &str) -> &str {
    job_id.split('-').next().unwrap_or(job_id)
}

#[cfg(test)]
mod tests {
    use super::in_test_section;

    #[test]
    fn screenshots_go_at_the_end_of_the_how_to_test_section() {
        let body = "## Summary\n\nA menu.\n\n## How to test\n\n1. Open it.\n\n## Notes\n\nNone.";
        assert_eq!(
            in_test_section(body, "![a](u)"),
            "## Summary\n\nA menu.\n\n## How to test\n\n1. Open it.\n\n![a](u)\n\n## Notes\n\nNone."
        );
        // Its own subheadings stay inside it.
        let nested = "## Cómo probar\n\n### Escritorio\n\nAbre.\n\n## Riesgos";
        assert_eq!(
            in_test_section(nested, "![a](u)"),
            "## Cómo probar\n\n### Escritorio\n\nAbre.\n\n![a](u)\n\n## Riesgos"
        );
        assert_eq!(
            in_test_section("## Summary\n\nA.", "![a](u)"),
            "## Summary\n\nA.\n\n![a](u)"
        );
    }
}
