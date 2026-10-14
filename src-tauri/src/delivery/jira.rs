//! Jira as Tinto uses it to run a task's Jira steps: link the PR to the
//! issue and walk the issue through the statuses the step names, then
//! verify where it ended. Each repo sets its Jira (host, project and the
//! statuses for after the PR and after the merge) in Tinto; the token is
//! stored apart from the settings and never leaves Tinto.

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
}
