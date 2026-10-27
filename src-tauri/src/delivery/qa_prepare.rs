//! Prepares a repo's QA stack before a browser QA job's agent starts: it
//! rebuilds only the services whose folders the task changed, waits for the
//! stack to answer and requests the pages QA visits, so the agent neither
//! rediscovers how to run the app nor waits on cold compiles. When the stack
//! already serves the task's worktree as it is, nothing is rebuilt.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::model::{DeliveryPreparedStack, DeliveryQaStack};

/// The log of a job's preparation, in its folder.
pub const PREPARE_LOG: &str = "prepare.log";
/// How long the whole preparation may take.
pub const PREPARE_LIMIT: Duration = Duration::from_secs(30 * 60);
/// How long the stack has to answer once it is up.
const READY_LIMIT: Duration = Duration::from_secs(10 * 60);
/// How long one page or module may take to compile on its first request.
const WARM_REQUEST_LIMIT: Duration = Duration::from_secs(120);
/// The most modules requested to warm the pages, the ones they import
/// included.
const WARM_MODULES: usize = 120;

/// The stack's services built from the folders the changed paths are in.
pub fn services_for(changed: &[String], map: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut services: Vec<String> = Vec::new();
    for path in changed {
        let path = path.replace('\\', "/");
        for (folder, built) in map {
            let folder = folder.replace('\\', "/");
            let inside = path == folder || path.starts_with(&format!("{folder}/"));
            if inside {
                services.extend(built.iter().cloned());
            }
        }
    }
    services.sort();
    services.dedup();
    services
}

/// Which services to rebuild for a task's worktree at `candidate`, given
/// what the stack served before: none when it already serves that worktree
/// as it is (`true`); else the task's own and those built before from any
/// worktree, which must serve this one now.
pub fn rebuild(
    task_id: &str,
    candidate: &str,
    changed: Vec<String>,
    prepared: Option<&DeliveryPreparedStack>,
) -> (Vec<String>, bool) {
    if prepared.is_some_and(|prepared| prepared.task_id == task_id && prepared.candidate == candidate) {
        return (Vec::new(), true);
    }
    let mut services = changed;
    if let Some(prepared) = prepared {
        services.extend(prepared.services.iter().cloned());
    }
    services.sort();
    services.dedup();
    (services, false)
}

/// The modules a page loads from its own server: the `src` of its scripts
/// and the `href` of its links, when they are paths on the same origin.
pub fn assets_in(html: &str) -> Vec<String> {
    let mut assets: Vec<String> = Vec::new();
    for (tag, attribute) in [("<script", "src=\""), ("<link", "href=\"")] {
        let mut rest = html;
        while let Some(at) = rest.find(tag) {
            rest = &rest[at + tag.len()..];
            let end = rest.find('>').unwrap_or(rest.len());
            let element = &rest[..end];
            let Some(value) = element
                .find(attribute)
                .map(|start| &element[start + attribute.len()..])
                .and_then(|value| value.split('"').next())
            else {
                continue;
            };
            let skipped = [".ico", ".png", ".svg", ".jpg", ".webp", ".woff", ".woff2"]
                .iter()
                .any(|ext| value.ends_with(ext));
            if value.starts_with('/') && !value.starts_with("//") && !skipped {
                if !assets.iter().any(|seen| seen == value) {
                    assets.push(value.to_string());
                }
            }
        }
    }
    assets
}

/// The modules a dev server's module imports from its own origin
/// (`from "/_nuxt/…"`, `import("/…")`): a dependency the server cannot
/// bundle hangs on these, not on the page.
pub fn imports_in(module: &str) -> Vec<String> {
    let mut imports: Vec<String> = Vec::new();
    for opening in ["from \"/", "from '/", "import \"/", "import '/", "import(\"/", "import('/"] {
        let quote = if opening.contains('\'') { '\'' } else { '"' };
        let mut rest = module;
        while let Some(at) = rest.find(opening) {
            rest = &rest[at + opening.len() - 1..];
            let value = rest.split(quote).next().unwrap_or_default();
            if !value.starts_with("//") && !imports.iter().any(|seen| seen == value) {
                imports.push(value.to_string());
            }
            rest = &rest[1..];
        }
    }
    imports
}

/// The origin of a URL (`http://localhost:8080` of `http://localhost:8080/x`).
pub fn origin(url: &str) -> &str {
    let after_scheme = url.find("://").map_or(0, |at| at + 3);
    match url[after_scheme..].find('/') {
        Some(at) => &url[..after_scheme + at],
        None => url,
    }
}

/// Appends a line to the preparation's log.
pub fn log(path: &Path, line: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

/// Waits until `url` answers (anything but a server error) or the limit
/// passes; says why it did not.
pub fn wait_ready(url: &str, cancel: &AtomicBool, until: Instant) -> Result<(), String> {
    let client = client(Duration::from_secs(10))?;
    let until = until.min(Instant::now() + READY_LIMIT);
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelado".to_string());
        }
        let last = match client.get(url).send() {
            Ok(response) if response.status().as_u16() < 500 => return Ok(()),
            Ok(response) => format!("respondió {}", response.status()),
            Err(error) => error.to_string(),
        };
        if Instant::now() >= until {
            return Err(format!("{url} no respondió a tiempo ({last})"));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Requests each page and the modules it loads, so the dev server compiles
/// them before QA visits them. A page or module that fails or does not
/// answer is a broken stack: QA would only wait on it.
pub fn warm(base: &str, paths: &[String], log_path: &Path) -> Result<usize, String> {
    let client = client(WARM_REQUEST_LIMIT)?;
    let origin = origin(base);
    let get = |url: &str| -> Result<String, String> {
        let started = Instant::now();
        let response = client
            .get(url)
            .send()
            .map_err(|error| format!("{url}: {error}"))?;
        let status = response.status();
        let body = response.text().unwrap_or_default();
        log(
            log_path,
            &format!("  {} {url} en {:.1} s", status.as_u16(), started.elapsed().as_secs_f32()),
        );
        if status.is_server_error() || status.is_client_error() {
            return Err(format!("{url} respondió {status}"));
        }
        Ok(body)
    };
    let pages: Vec<String> = match paths {
        [] => vec![base.to_string()],
        paths => paths
            .iter()
            .map(|path| match path.starts_with("http") {
                true => path.clone(),
                false => format!("{origin}/{}", path.trim_start_matches('/')),
            })
            .collect(),
    };
    // Each page, then the modules it loads and the ones they import, in
    // order, each once.
    let mut modules: Vec<String> = Vec::new();
    for page in pages {
        let html = get(&page)?;
        for asset in assets_in(&html) {
            if !modules.contains(&asset) {
                modules.push(asset);
            }
        }
    }
    let mut next = 0;
    while next < modules.len() && next < WARM_MODULES {
        let module = get(&format!("{origin}{}", modules[next]))?;
        next += 1;
        if modules[next - 1].ends_with(".css") {
            continue;
        }
        for import in imports_in(&module) {
            if !modules.contains(&import) {
                modules.push(import);
            }
        }
    }
    Ok(next)
}

fn client(timeout: Duration) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| error.to_string())
}

/// Runs one of the stack's commands to its end, its output in the log,
/// stopping it when the job is cancelled or the preparation's time is out.
pub fn run(
    mut command: Command,
    label: &str,
    log_path: &Path,
    cancel: &AtomicBool,
    until: Instant,
    job_id: &str,
) -> Result<(), String> {
    log(log_path, &format!("$ {label}"));
    let output = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|error| error.to_string())?;
    let errors = output.try_clone().map_err(|error| error.to_string())?;
    command
        .stdin(Stdio::null())
        .stdout(output)
        .stderr(errors);
    let mut process = super::job_process::JobProcess::spawn(&mut command, job_id)
        .map_err(|error| format!("no se pudo iniciar «{label}»: {error}"))?;
    loop {
        match process.try_wait() {
            Ok(Some(exited)) => {
                process.sweep();
                return match exited.success {
                    Some(true) => Ok(()),
                    _ => Err(format!(
                        "«{label}» terminó con código {}",
                        exited.code.map_or("?".to_string(), |code| code.to_string())
                    )),
                };
            }
            Ok(None) => {}
            Err(error) => return Err(error.to_string()),
        }
        let stop = if cancel.load(Ordering::SeqCst) {
            Some("cancelado".to_string())
        } else if Instant::now() >= until {
            Some(format!("«{label}» no terminó a tiempo"))
        } else {
            None
        };
        if let Some(why) = stop {
            process.kill_tree();
            process.sweep();
            return Err(why);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// `up` with the services to rebuild in place of `{services}`.
pub fn up_command(stack: &DeliveryQaStack, services: &[String]) -> String {
    stack.up.replace("{services}", &services.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> BTreeMap<String, Vec<String>> {
        BTreeMap::from([
            ("frontend".to_string(), vec!["frontend".to_string()]),
            ("api-gateway".to_string(), vec!["api-gateway".to_string()]),
            (
                "services/workflows".to_string(),
                vec!["workflows".to_string(), "scheduler".to_string()],
            ),
            ("protocol".to_string(), vec!["api-gateway".to_string(), "workflows".to_string()]),
        ])
    }

    #[test]
    fn only_the_services_of_the_changed_folders_are_rebuilt() {
        let changed = |paths: &[&str]| -> Vec<String> {
            services_for(&paths.iter().map(|p| p.to_string()).collect::<Vec<_>>(), &map())
        };
        // A frontend-only change rebuilds no backend service.
        assert_eq!(changed(&["frontend/pages/x.vue", "README.md"]), ["frontend"]);
        assert_eq!(
            changed(&["protocol/a.proto", "services/workflows/src/a.ts"]),
            ["api-gateway", "scheduler", "workflows"]
        );
        // A folder is matched whole, not by a prefix of its name.
        assert!(changed(&["frontend-legacy/a.ts", "services/workflows-old/a"]).is_empty());
    }

    #[test]
    fn a_stack_serving_the_same_worktree_is_reused_and_another_tasks_services_come_back() {
        let prepared = DeliveryPreparedStack {
            task_id: "t1".into(),
            candidate: "c1".into(),
            services: vec!["rag".into()],
        };
        assert_eq!(rebuild("t1", "c1", vec!["frontend".into()], Some(&prepared)), (vec![], true));
        // Another task's services serve its worktree: they are rebuilt too.
        assert_eq!(
            rebuild("t2", "c9", vec!["frontend".into()], Some(&prepared)),
            (vec!["frontend".to_string(), "rag".to_string()], false)
        );
        assert_eq!(
            rebuild("t2", "c9", vec!["frontend".into()], None),
            (vec!["frontend".to_string()], false)
        );
    }

    #[test]
    fn a_pages_own_modules_are_found() {
        let html = r#"<html><head><link rel="modulepreload" href="/_nuxt/entry.js"><link rel="icon" href="/favicon.ico">
<script type="module" src="/_nuxt/@vite/client"></script><script src="https://cdn.example.com/x.js"></script>
<script src="//cdn.example.com/y.js"></script><script>inline()</script></head></html>"#;
        assert_eq!(assets_in(html), ["/_nuxt/@vite/client", "/_nuxt/entry.js"]);
        let module = r#"import { a } from "/_nuxt/node_modules/.cache/vite/client/deps/vue.js?v=1";
import '/_nuxt/app.css'; const p = import("/_nuxt/pages/login.vue"); import x from "./rel.js"; import y from "//cdn/z.js";"#;
        assert_eq!(
            imports_in(module),
            [
                "/_nuxt/node_modules/.cache/vite/client/deps/vue.js?v=1",
                "/_nuxt/app.css",
                "/_nuxt/pages/login.vue"
            ]
        );
        assert_eq!(origin("http://localhost:8080/login?a=1"), "http://localhost:8080");
        assert_eq!(origin("http://localhost:8080"), "http://localhost:8080");
    }
}
