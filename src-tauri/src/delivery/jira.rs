//! Jira as Tinto uses it to run a task's Jira steps: link the PR to the
//! issue and walk the issue through the statuses the step names, then
//! verify where it ended. Each repo sets its Jira (host, project and the
//! statuses for after the PR and after the merge) in Tinto; the token is
//! stored apart from the settings and never leaves Tinto.

use serde::Serialize;
use serde_json::{json, Value};

use super::service::DeliveryService;
use super::DeliveryError;

/// Where a repo's Jira is and how to reach it.
#[derive(Debug, Clone)]
pub struct JiraSite {
    /// `https://jira.example.com`, without a trailing slash.
    pub host: String,
    pub token: String,
}

/// Jira's REST API. The app uses HTTP; tests use fakes.
pub trait Jira: Send + Sync {
    /// `method` on `path` (`/rest/api/2/…`), with an optional JSON body.
    /// Answers the JSON, or `null` for an empty answer.
    fn call(
        &self,
        site: &JiraSite,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, DeliveryError>;
}

/// Jira Server/Data Center over HTTPS with a personal access token.
pub struct JiraHttp;

impl Jira for JiraHttp {
    fn call(
        &self,
        site: &JiraSite,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, DeliveryError> {
        let failed = |message: String| DeliveryError::new("jira_failed", message);
        // No redirects: a redirect would carry the token somewhere else.
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|error| failed(error.to_string()))?;
        let method = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|error| failed(error.to_string()))?;
        let mut request = client
            .request(method, format!("{}{path}", site.host))
            .bearer_auth(&site.token)
            .header("Accept", "application/json");
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .map_err(|error| failed(format!("no se pudo llegar a Jira: {error}")))?;
        let status = response.status();
        let text = response.text().unwrap_or_default();
        if !status.is_success() {
            let detail: String = text.chars().take(300).collect();
            return Err(failed(format!("Jira respondió {status}: {detail}")));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|error| failed(error.to_string()))
    }
}

/// `https://jira.example.com` from what the user typed.
pub fn clean_host(host: &str) -> Result<String, DeliveryError> {
    let host = host.trim().trim_end_matches('/');
    if !host.starts_with("https://") || host.len() <= "https://".len() {
        return Err(DeliveryError::new(
            "invalid_settings",
            "el host de Jira debe empezar por https://",
        ));
    }
    Ok(host.to_string())
}

/// What a Jira step does: link the PR (once it is open) and walk the issue
/// through `statuses`, in order. Returns what was done, ending with the
/// status Jira reports at the end. A failure says what was done before it.
pub fn run_step(
    jira: &dyn Jira,
    site: &JiraSite,
    issue: &str,
    pr: Option<(&str, u64)>,
    statuses: &[String],
) -> Result<String, DeliveryError> {
    let base = format!("/rest/api/2/issue/{issue}");
    let mut done: Vec<String> = Vec::new();
    let stop = |done: &[String], error: DeliveryError| {
        let before = match done {
            [] => "No se hizo nada en Jira".to_string(),
            done => format!("Hecho antes: {}", done.join(". ")),
        };
        DeliveryError::new(error.category, format!("{} {before}.", error.message))
    };
    if let Some((url, number)) = pr {
        // The same globalId updates the link instead of adding another.
        let link = json!({
            "globalId": format!("github-pr={url}"),
            "relationship": "Pull Request",
            "object": {"url": url, "title": format!("PR #{number}")}
        });
        jira.call(site, "POST", &format!("{base}/remotelink"), Some(&link))
            .map_err(|error| stop(&done, error))?;
        done.push(format!("Enlace al PR #{number}"));
    }
    let status = |jira: &dyn Jira| -> Result<String, DeliveryError> {
        let issue = jira.call(site, "GET", &format!("{base}?fields=status"), None)?;
        Ok(issue["fields"]["status"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    };
    for target in statuses {
        let current = status(jira).map_err(|error| stop(&done, error))?;
        if current.eq_ignore_ascii_case(target) {
            continue;
        }
        let transitions = jira
            .call(site, "GET", &format!("{base}/transitions"), None)
            .map_err(|error| stop(&done, error))?;
        let available = transitions["transitions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let to = |transition: &Value| {
            transition["to"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        };
        let Some(transition) = available
            .iter()
            .find(|t| to(t).eq_ignore_ascii_case(target))
        else {
            let reachable: Vec<String> = available.iter().map(to).collect();
            return Err(stop(
                &done,
                DeliveryError::new(
                    "jira_transition_missing",
                    format!(
                        "Desde «{current}» no hay transición a «{target}» (se puede ir a: {}).",
                        if reachable.is_empty() {
                            "ninguno".to_string()
                        } else {
                            reachable.join(", ")
                        }
                    ),
                ),
            ));
        };
        let id = transition["id"].as_str().unwrap_or_default();
        let body = json!({"transition": {"id": id}});
        jira.call(site, "POST", &format!("{base}/transitions"), Some(&body))
            .map_err(|error| stop(&done, error))?;
        done.push(format!(
            "{current} → {} ({})",
            to(transition),
            transition["name"].as_str().unwrap_or(id)
        ));
    }
    let reached = status(jira).map_err(|error| stop(&done, error))?;
    if let Some(target) = statuses.last() {
        if !reached.eq_ignore_ascii_case(target) {
            return Err(stop(
                &done,
                DeliveryError::new(
                    "jira_status_mismatch",
                    format!("{issue} quedó en «{reached}», no en «{target}»."),
                ),
            ));
        }
    }
    done.push(format!("Estado verificado en {issue}: {reached}"));
    Ok(done.join(". "))
}

/// A task's issue as Tinto shows it: the fields that matter to the work,
/// and its description and last comments as the HTML Jira renders.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryIssue {
    pub key: String,
    pub url: String,
    pub summary: String,
    pub status: String,
    #[serde(rename = "type")]
    pub issue_type: String,
    pub priority: Option<String>,
    pub assignee: Option<String>,
    pub reporter: Option<String>,
    pub labels: Vec<String>,
    pub parent: Option<DeliveryIssueParent>,
    pub links: Vec<DeliveryIssueLink>,
    pub description_html: String,
    pub comments: Vec<DeliveryIssueComment>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DeliveryIssueParent {
    pub key: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryIssueLink {
    /// How the other issue relates to this one: "blocks", "is blocked by"…
    #[serde(rename = "type")]
    pub link_type: String,
    pub key: String,
    pub summary: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryIssueComment {
    pub author: String,
    /// When it was written, as Jira gives it (ISO 8601).
    pub created: String,
    pub body_html: String,
}

/// How many comments the issue view keeps: the newest ones.
const ISSUE_COMMENTS: usize = 10;

/// The issue's parent in Jira's `fields`, if it has one.
fn parent_from(fields: &Value) -> Option<DeliveryIssueParent> {
    Some(DeliveryIssueParent {
        key: fields["parent"]["key"].as_str()?.to_string(),
        summary: fields["parent"]["fields"]["summary"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    })
}

/// The issue from Jira's answer to `GET /rest/api/2/issue/{key}` with
/// `expand=renderedFields`.
fn issue_from(key: &str, url: String, issue: &Value) -> DeliveryIssue {
    let fields = &issue["fields"];
    let rendered = &issue["renderedFields"];
    let text = |value: &Value| value.as_str().unwrap_or_default().to_string();
    let person = |value: &Value| {
        value["displayName"]
            .as_str()
            .or(value["name"].as_str())
            .map(str::to_string)
    };
    let links = fields["issuelinks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|link| {
            let (other, link_type) = if link["outwardIssue"].is_object() {
                (&link["outwardIssue"], &link["type"]["outward"])
            } else if link["inwardIssue"].is_object() {
                (&link["inwardIssue"], &link["type"]["inward"])
            } else {
                return None;
            };
            Some(DeliveryIssueLink {
                link_type: text(link_type),
                key: text(&other["key"]),
                summary: text(&other["fields"]["summary"]),
                status: text(&other["fields"]["status"]["name"]),
            })
        })
        .collect();
    let empty = Vec::new();
    let comments = fields["comment"]["comments"].as_array().unwrap_or(&empty);
    let rendered_comments = rendered["comment"]["comments"].as_array().unwrap_or(&empty);
    let comments = comments
        .iter()
        .enumerate()
        .skip(comments.len().saturating_sub(ISSUE_COMMENTS))
        .map(|(index, comment)| DeliveryIssueComment {
            author: person(&comment["author"]).unwrap_or_default(),
            created: text(&comment["created"]),
            body_html: rendered_comments
                .get(index)
                .and_then(|rendered| rendered["body"].as_str())
                .map(str::to_string)
                .unwrap_or_else(|| text(&comment["body"])),
        })
        .collect();
    DeliveryIssue {
        key: issue["key"].as_str().unwrap_or(key).to_string(),
        url,
        summary: text(&fields["summary"]),
        status: text(&fields["status"]["name"]),
        issue_type: text(&fields["issuetype"]["name"]),
        priority: fields["priority"]["name"].as_str().map(str::to_string),
        assignee: person(&fields["assignee"]),
        reporter: person(&fields["reporter"]),
        labels: fields["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|label| label.as_str().map(str::to_string))
            .collect(),
        parent: parent_from(fields),
        links,
        description_html: text(&rendered["description"]),
        comments,
    }
}

impl DeliveryService {
    pub fn set_jira(&self, jira: std::sync::Arc<dyn Jira>) {
        if let Ok(mut slot) = self.inner.jira.lock() {
            *slot = jira;
        }
    }

    pub(super) fn jira_client(&self) -> std::sync::Arc<dyn Jira> {
        self.inner
            .jira
            .lock()
            .map(|slot| slot.clone())
            .unwrap_or_else(|_| std::sync::Arc::new(JiraHttp))
    }

    /// The repo's Jira, when it is set in Tinto with its token.
    pub(super) fn jira_site(
        &self,
        repo: &std::path::Path,
    ) -> Result<Option<JiraSite>, DeliveryError> {
        let store = self.store()?;
        let Some(jira) = store.repo_settings(repo)?.jira else {
            return Ok(None);
        };
        Ok(store.jira_token(repo)?.map(|token| JiraSite {
            host: jira.host,
            token,
        }))
    }

    /// The page of the task's issue: on the repo's Jira set in Tinto, else
    /// on the host the backlog-delivery skill keeps in the checkout's
    /// `.agent/env/jira.env` (only `JIRA_HOST` is read from it).
    pub fn issue_url(&self, task_id: &str) -> Result<Option<String>, DeliveryError> {
        let task = self.task(task_id)?;
        let configured = self
            .store()?
            .repo_settings(&task.repo)?
            .jira
            .map(|jira| jira.host)
            .filter(|host| !host.trim().is_empty());
        let host = configured.or_else(|| skill_jira_host(&task.repo));
        Ok(host.map(|host| format!("{}/browse/{}", host.trim().trim_end_matches('/'), task.key)))
    }

    /// Saves the token apart from the repo's settings; empty forgets it.
    pub fn set_jira_token(&self, repo: &std::path::Path, token: &str) -> Result<(), DeliveryError> {
        self.store()?.set_jira_token(repo, token.trim())
    }

    /// "Comprobar Jira": who the token belongs to, as Jira sees it.
    pub fn check_jira(&self, repo: &std::path::Path) -> Result<String, DeliveryError> {
        let site = self.jira_site(repo)?.ok_or_else(|| {
            DeliveryError::new(
                "jira_not_configured",
                "falta el host o el token de Jira de este repositorio",
            )
        })?;
        let me = self
            .jira_client()
            .call(&site, "GET", "/rest/api/2/myself", None)?;
        Ok(me["displayName"]
            .as_str()
            .or(me["name"].as_str())
            .unwrap_or("usuario sin nombre")
            .to_string())
    }

    /// The task's issue, read from the repo's Jira set in Tinto.
    pub fn issue(&self, task_id: &str) -> Result<DeliveryIssue, DeliveryError> {
        let task = self.task(task_id)?;
        let site = self.jira_site(&task.repo)?.ok_or_else(|| {
            DeliveryError::new(
                "jira_not_configured",
                "falta el host o el token de Jira de este repositorio: ponlos en Ajustes para leer la incidencia en Tinto",
            )
        })?;
        let path = format!(
            "/rest/api/2/issue/{}?fields=summary,status,issuetype,priority,assignee,reporter,labels,description,comment,parent,issuelinks&expand=renderedFields",
            task.key
        );
        let issue = self.jira_client().call(&site, "GET", &path, None)?;
        let url = format!("{}/browse/{}", site.host, task.key);
        let issue = issue_from(&task.key, url, &issue);
        self.remember_parent(task_id, issue.parent.clone())?;
        Ok(issue)
    }

    /// Reads from Jira the parent of each task never read yet, in the repos
    /// with Jira set; one that fails is tried again on the next call.
    pub(super) fn read_parents(&self) -> Result<(), DeliveryError> {
        let tasks = self.store()?.tasks()?;
        for task in tasks {
            if task.parent_read_at_ms.is_some() || task.removed_at_ms.is_some() {
                continue;
            }
            let Some(site) = self.jira_site(&task.repo)? else {
                continue;
            };
            let path = format!("/rest/api/2/issue/{}?fields=parent", task.key);
            if let Ok(issue) = self.jira_client().call(&site, "GET", &path, None) {
                self.remember_parent(&task.id, parent_from(&issue["fields"]))?;
            }
        }
        Ok(())
    }

    fn remember_parent(
        &self,
        task_id: &str,
        parent: Option<DeliveryIssueParent>,
    ) -> Result<(), DeliveryError> {
        let store = self.store()?;
        let Some(mut task) = store.task(task_id)? else {
            return Ok(());
        };
        task.parent = parent;
        task.parent_read_at_ms = Some(super::now_ms());
        store.put_task(&task)
    }
}

/// `JIRA_HOST` from the skill's `.agent/env/jira.env` in the main checkout,
/// when it is an http(s) address. Nothing else in that file is read.
fn skill_jira_host(repo: &std::path::Path) -> Option<String> {
    let env = std::fs::read_to_string(repo.join(".agent").join("env").join("jira.env")).ok()?;
    env.lines()
        .find_map(|line| line.trim().strip_prefix("JIRA_HOST="))
        .map(|host| host.trim().trim_matches(['"', '\'']).to_string())
        .filter(|host| host.starts_with("https://") || host.starts_with("http://"))
}

#[cfg(test)]
mod issue_url_tests {
    use super::skill_jira_host;

    #[test]
    fn the_skill_env_gives_only_its_host() {
        let repo = tempfile::tempdir().unwrap();
        let env = repo.path().join(".agent").join("env");
        std::fs::create_dir_all(&env).unwrap();
        std::fs::write(
            env.join("jira.env"),
            "JIRA_API_TOKEN=secret\nJIRA_HOST=\"https://jira.example.com/\"\n",
        )
        .unwrap();
        assert_eq!(
            skill_jira_host(repo.path()).as_deref(),
            Some("https://jira.example.com/")
        );
        std::fs::write(env.join("jira.env"), "JIRA_HOST=file:///etc\n").unwrap();
        assert_eq!(skill_jira_host(repo.path()), None);
    }
}
