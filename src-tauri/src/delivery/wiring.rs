//! Connects conversations started in Agents to the coordinator API, so a
//! Claude Code or Codex conversation can coordinate Delivery without setup.
//!
//! CLIs on Windows reach the server over HTTP. Inside WSL (NAT networking),
//! `127.0.0.1` is not Windows, so the CLI runs this same executable as a
//! stdio MCP server ([`PROXY_FLAG`]) that forwards each message to it.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Value};

use super::model::DeliveryCoordinatorEndpoint;

pub const SERVER_NAME: &str = "tinto-delivery";
/// `tinto.exe --delivery-mcp-proxy <url> <token>` serves MCP on stdio.
pub const PROXY_FLAG: &str = "--delivery-mcp-proxy";
/// The proxy runs from its own copy of the executable, under this name and
/// outside the install folder: installing Tinto closes every `tinto.exe`
/// and would cut the coordinators' Delivery tools mid-call.
const PROXY_EXE: &str = if cfg!(windows) {
    "tinto-delivery-proxy.exe"
} else {
    "tinto-delivery-proxy"
};
/// The setting with the app's own executable, which the proxy opens.
pub(super) const APP_EXE_SETTING: &str = "app_exe";

static ENDPOINT: Mutex<Option<DeliveryCoordinatorEndpoint>> = Mutex::new(None);

pub(super) fn publish(endpoint: &DeliveryCoordinatorEndpoint) {
    if let Ok(mut slot) = ENDPOINT.lock() {
        *slot = Some(endpoint.clone());
    }
}

/// Header naming the Agents conversation a Delivery tool call comes from.
pub const SESSION_HEADER: &str = "X-Tinto-Agent-Session";
/// Variable that carries the conversation's id to Muse Code, whose `-c`
/// header values must come from the environment.
pub const MUSE_SESSION_ENV: &str = "TINTO_AGENT_SESSION";

thread_local! {
    static AGENT_SESSION: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Runs `spawn`, which starts the Agents conversation `session`: the
/// Delivery tool arguments built meanwhile name it, so Tinto knows which
/// conversation coordinates a batch and can wake it.
pub fn with_agent_session<T>(session: &str, spawn: impl FnOnce() -> T) -> T {
    AGENT_SESSION.with(|slot| *slot.borrow_mut() = Some(session.to_string()));
    let spawned = spawn();
    AGENT_SESSION.with(|slot| *slot.borrow_mut() = None);
    spawned
}

fn agent_session() -> Option<String> {
    AGENT_SESSION.with(|slot| slot.borrow().clone())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Server {
    Http {
        url: String,
        token: String,
        session: Option<String>,
    },
    Stdio {
        command: String,
        args: Vec<String>,
    },
}

fn server(wsl: bool) -> Option<Server> {
    let endpoint = ENDPOINT.lock().ok()?.clone()?;
    let session = agent_session();
    if !wsl {
        return Some(Server::Http {
            url: endpoint.url,
            token: endpoint.token,
            session,
        });
    }
    let exe = match endpoint.command {
        Some(command) => PathBuf::from(command),
        None => std::env::current_exe().ok()?,
    };
    let command = crate::wsl_agent::launcher::windows_path_to_wsl_mount(&exe).ok()?;
    let mut args = vec![PROXY_FLAG.to_string(), endpoint.url, endpoint.token];
    args.extend(session);
    Some(Server::Stdio { command, args })
}

/// Arguments for an interactive Claude Code conversation: the Delivery
/// tools, pre-approved. Empty when the coordinator API is not running.
pub fn claude_args(wsl: bool) -> Vec<String> {
    server(wsl)
        .map(|server| claude_args_for(&server))
        .unwrap_or_default()
}

/// `-c` overrides for a Codex conversation: the Delivery tools,
/// pre-approved. Empty when the coordinator API is not running.
pub fn codex_args(wsl: bool) -> Vec<String> {
    server(wsl)
        .map(|server| codex_args_for(&server))
        .unwrap_or_default()
}

/// Variable that carries the Delivery token to Muse Code: its `-c` values
/// refuse a literal secret and take it only as a whole `${VAR}`.
pub const MUSE_AUTH_ENV: &str = "TINTO_DELIVERY_AUTH";

/// `-c` overrides for a Muse Code conversation, and the environment they
/// read. Muse runs it without approval prompts, so the tools need no
/// pre-approval. Empty when the coordinator API is not running.
pub fn muse_args(wsl: bool) -> (Vec<String>, Vec<(&'static str, String)>) {
    server(wsl)
        .map(|server| muse_args_for(&server))
        .unwrap_or_default()
}

fn muse_args_for(server: &Server) -> (Vec<String>, Vec<(&'static str, String)>) {
    let (config, env) = match server {
        Server::Http {
            url,
            token,
            session,
        } => {
            let mut headers = json!({ "Authorization": format!("${{{MUSE_AUTH_ENV}}}") });
            let mut env = vec![(MUSE_AUTH_ENV, format!("Bearer {token}"))];
            if let Some(session) = session {
                headers[SESSION_HEADER] = json!(format!("${{{MUSE_SESSION_ENV}}}"));
                env.push((MUSE_SESSION_ENV, session.clone()));
            }
            (json!({ "url": url, "headers": headers }), env)
        }
        Server::Stdio { command, args } => {
            (json!({ "command": command, "args": args }), Vec::new())
        }
    };
    (
        vec![
            "-c".to_string(),
            format!("mcpServers.{SERVER_NAME}={config}"),
        ],
        env,
    )
}

fn claude_args_for(server: &Server) -> Vec<String> {
    let config = match server {
        Server::Http {
            url,
            token,
            session,
        } => {
            let mut headers = json!({ "Authorization": format!("Bearer {token}") });
            if let Some(session) = session {
                headers[SESSION_HEADER] = json!(session);
            }
            json!({ "type": "http", "url": url, "headers": headers })
        }
        Server::Stdio { command, args } => json!({
            "type": "stdio",
            "command": command,
            "args": args,
        }),
    };
    vec![
        "--mcp-config".to_string(),
        json!({ "mcpServers": { SERVER_NAME: config } }).to_string(),
        "--allowedTools".to_string(),
        format!("mcp__{SERVER_NAME}"),
    ]
}

fn codex_args_for(server: &Server) -> Vec<String> {
    // `-c` values are TOML; JSON strings are valid TOML basic strings.
    let text = |value: &str| Value::from(value).to_string();
    let key = |name: &str| format!("mcp_servers.{SERVER_NAME}.{name}");
    let overrides = match server {
        Server::Http {
            url,
            token,
            session,
        } => {
            let session = session
                .as_deref()
                .map(|session| format!(", {} = {}", text(SESSION_HEADER), text(session)))
                .unwrap_or_default();
            vec![
                format!("{}={}", key("url"), text(url)),
                format!(
                    "{}={{ Authorization = {}{session} }}",
                    key("http_headers"),
                    text(&format!("Bearer {token}"))
                ),
            ]
        }
        Server::Stdio { command, args } => vec![
            format!("{}={}", key("command"), text(command)),
            format!(
                "{}=[{}]",
                key("args"),
                args.iter()
                    .map(|arg| text(arg))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ],
    };
    // Agents run Codex without approval prompts, which would decline every
    // Delivery tool; the API itself keeps approvals with the user.
    let approve = format!("{}=\"approve\"", key("default_tools_approval_mode"));
    overrides
        .into_iter()
        .chain([approve])
        .flat_map(|value| ["-c".to_string(), value])
        .collect()
}

/// Puts this executable at the proxy's path in `dir` when it is not there
/// or is another version, and returns that path. A copy still running for
/// some agent cannot be replaced but can be renamed: it moves aside, and is
/// deleted on a later start, once nothing runs it.
pub fn install_proxy(dir: &Path) -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin)?;
    for entry in std::fs::read_dir(&bin)?.flatten() {
        if entry.file_name().to_string_lossy().starts_with("old-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let target = bin.join(PROXY_EXE);
    if std::fs::read(&target).ok() == Some(std::fs::read(&exe)?) {
        return Ok(target);
    }
    if target.exists() && std::fs::remove_file(&target).is_err() {
        let aside = bin.join(format!("old-{}-{PROXY_EXE}", super::now_ms()));
        std::fs::rename(&target, aside)?;
    }
    std::fs::copy(&exe, &target)?;
    Ok(target)
}

/// The app the proxy opens when Tinto is closed: the one that last started
/// the server, since the proxy itself runs from a copy.
fn app_exe() -> Option<PathBuf> {
    let store = super::store::DeliveryStore::open_default().ok()?;
    let exe = store.setting::<String>(APP_EXE_SETTING).ok()??;
    Some(PathBuf::from(exe)).filter(|exe| exe.is_file())
}

/// Where the proxy sends messages: the server's URL and its token.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    url: String,
    token: String,
}

/// Where Tinto's server listens now, as Tinto saved it when it started.
/// Its port can change between launches (the saved one may be taken or
/// reserved by Windows), so a config never has to name it.
fn saved_target() -> Option<Target> {
    let store = super::store::DeliveryStore::open_default().ok()?;
    let port = store.setting::<u16>("mcp_port").ok()??;
    let token = store.setting::<String>("mcp_token").ok()??;
    Some(Target {
        url: format!("http://127.0.0.1:{port}/mcp"),
        token,
    })
}

/// Runs the stdio side of the bridge: one JSON-RPC message per line in,
/// the server's answer (if any) per line out. `given` is the URL and token
/// of older configs, used only when Tinto has saved none.
pub fn proxy_stdio(given: Option<(&str, &str)>, session: Option<&str>) -> i32 {
    let resolve = || {
        saved_target().or_else(|| {
            given.map(|(url, token)| Target {
                url: url.to_string(),
                token: token.to_string(),
            })
        })
    };
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15 * 60))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("tinto: {error}");
            return 1;
        }
    };
    let stdout = Mutex::new(std::io::stdout());
    forward_lines(std::io::stdin().lock(), &stdout, &client, resolve, session);
    0
}

/// Forwards each line on its own thread and writes each answer as it comes:
/// a long call (a wait_run, a read GitHub is slow to answer, one the client
/// gave up on) never holds up the calls after it.
fn forward_lines<W: Write + Send>(
    input: impl BufRead,
    output: &Mutex<W>,
    client: &reqwest::blocking::Client,
    resolve: impl Fn() -> Option<Target> + Sync,
    session: Option<&str>,
) {
    let target = Mutex::new(resolve());
    let opened = Mutex::new(false);
    std::thread::scope(|scope| {
        for line in input.lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let (target, opened, resolve) = (&target, &opened, &resolve);
            scope.spawn(move || {
                let answer = forward(client, &line, session, target, opened, resolve);
                if answer.trim().is_empty() {
                    return;
                }
                let mut output = output
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let _ = writeln!(output, "{}", answer.trim()).and_then(|()| output.flush());
            });
        }
    });
}

/// Sends one message to Tinto: its answer, or the error to give the client.
fn forward(
    client: &reqwest::blocking::Client,
    line: &str,
    session: Option<&str>,
    target: &Mutex<Option<Target>>,
    opened: &Mutex<bool>,
    resolve: &(impl Fn() -> Option<Target> + Sync),
) -> String {
    let current = || {
        target
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    };
    let set = |found: Option<Target>| {
        *target
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = found;
    };
    let send = |target: &Option<Target>| {
        let target = target.as_ref()?;
        Some(
            client
                .post(&target.url)
                .bearer_auth(&target.token)
                .header(SESSION_HEADER, session.unwrap_or_default())
                .header("Content-Type", "application/json")
                .header("Accept", "application/json")
                .body(line.to_string())
                .send(),
        )
    };
    let unreachable = |sent: &Option<reqwest::Result<reqwest::blocking::Response>>| {
        sent.as_ref()
            .is_none_or(|sent| sent.as_ref().is_err_and(reqwest::Error::is_connect))
    };
    let mut sent = send(&current());
    // Nobody listening there: Tinto restarted on another port, or is
    // closed. Read where it listens now; if nobody does, open Tinto, once,
    // and wait for its server, so an agent never has to start Tinto some
    // other way or be set up again. Calls meanwhile wait for that opening.
    if unreachable(&sent) {
        set(resolve());
        sent = send(&current());
    }
    if unreachable(&sent) {
        let mut opened = opened
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !*opened {
            *opened = true;
            if open_tinto() {
                if let Some(found) = wait_for_server(resolve, std::time::Duration::from_secs(60)) {
                    set(Some(found));
                }
            }
        }
        drop(opened);
        sent = send(&current());
    }
    match sent.map(|sent| sent.and_then(|response| response.text())) {
        Some(Ok(body)) => body,
        Some(Err(error)) => proxy_error(line, &error.to_string()),
        None => proxy_error(line, "Tinto no ha iniciado nunca su servidor"),
    }
}

/// Starts this same executable as the Tinto app, detached from the agent
/// that runs the proxy, so closing the agent does not close Tinto.
fn open_tinto() -> bool {
    let Some(exe) = app_exe().or_else(|| std::env::current_exe().ok()) else {
        return false;
    };
    let mut command = std::process::Command::new(exe);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        // Out of the agent's job object when it allows it, else within it.
        command.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB);
        if command.spawn().is_ok() {
            return true;
        }
        command.creation_flags(flags);
    }
    command.spawn().is_ok()
}

/// The first target `resolve` gives that accepts connections before
/// `limit` runs out. It is asked again on every try: a starting Tinto saves
/// its port only once it listens.
fn wait_for_server(
    resolve: impl Fn() -> Option<Target>,
    limit: std::time::Duration,
) -> Option<Target> {
    let deadline = std::time::Instant::now() + limit;
    let step = std::time::Duration::from_millis(500);
    while std::time::Instant::now() < deadline {
        let listening = resolve().filter(|target| {
            let address = target
                .url
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap_or_default();
            std::net::ToSocketAddrs::to_socket_addrs(address)
                .ok()
                .and_then(|mut addresses| addresses.next())
                .is_some_and(|address| std::net::TcpStream::connect_timeout(&address, step).is_ok())
        });
        if listening.is_some() {
            return listening;
        }
        std::thread::sleep(step);
    }
    None
}

/// A JSON-RPC error for a request Tinto could not be reached for; nothing
/// for notifications.
fn proxy_error(request: &str, reason: &str) -> String {
    let id = serde_json::from_str::<Value>(request)
        .ok()
        .and_then(|message| message.get("id").cloned());
    match id {
        Some(id) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": format!("Tinto no responde: {reason}") },
        })
        .to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_proxy_runs_from_its_own_copy_kept_at_this_version() {
        let dir = tempfile::tempdir().unwrap();
        let exe = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        let copy = install_proxy(dir.path()).unwrap();
        assert_eq!(copy, dir.path().join("bin").join(PROXY_EXE));
        assert!(std::fs::read(&copy).unwrap() == exe);
        // Another version is replaced; leftovers set aside earlier go.
        std::fs::write(&copy, b"older").unwrap();
        std::fs::write(dir.path().join("bin").join("old-1-proxy"), b"x").unwrap();
        assert_eq!(install_proxy(dir.path()).unwrap(), copy);
        assert!(std::fs::read(&copy).unwrap() == exe);
        assert!(!dir.path().join("bin").join("old-1-proxy").exists());
    }

    fn http() -> Server {
        Server::Http {
            url: "http://127.0.0.1:47920/mcp".into(),
            token: "t0k".into(),
            session: None,
        }
    }

    #[test]
    fn every_runtime_names_its_conversation_to_delivery() {
        let http = Server::Http {
            url: "http://127.0.0.1:47920/mcp".into(),
            token: "t0k".into(),
            session: Some("s-1".into()),
        };
        let claude: Value = serde_json::from_str(&claude_args_for(&http)[1]).unwrap();
        assert_eq!(
            claude["mcpServers"]["tinto-delivery"]["headers"][SESSION_HEADER],
            "s-1"
        );
        let codex = codex_args_for(&http);
        assert!(
            codex[3].contains(r#""X-Tinto-Agent-Session" = "s-1""#),
            "{}",
            codex[3]
        );
        let (muse, env) = muse_args_for(&http);
        assert!(muse[1].contains(r#""X-Tinto-Agent-Session":"${TINTO_AGENT_SESSION}""#));
        assert!(env.contains(&(MUSE_SESSION_ENV, "s-1".to_string())));
        assert_eq!(
            with_agent_session("s-2", agent_session),
            Some("s-2".to_string())
        );
        assert_eq!(agent_session(), None, "only while the conversation starts");
    }

    fn stdio() -> Server {
        Server::Stdio {
            command: "/mnt/c/Program Files/Tinto/tinto.exe".into(),
            args: vec![
                PROXY_FLAG.into(),
                "http://127.0.0.1:47920/mcp".into(),
                "t0k".into(),
            ],
        }
    }

    #[test]
    fn muse_reads_the_token_from_its_environment() {
        let (args, env) = muse_args_for(&http());
        assert_eq!(args[0], "-c");
        let (key, value) = args[1].split_once('=').unwrap();
        assert_eq!(key, "mcpServers.tinto-delivery");
        let config: Value = serde_json::from_str(value).unwrap();
        assert_eq!(
            config,
            json!({"url": "http://127.0.0.1:47920/mcp", "headers": {"Authorization": "${TINTO_DELIVERY_AUTH}"}})
        );
        assert!(!args[1].contains("t0k"));
        assert_eq!(env, [(MUSE_AUTH_ENV, "Bearer t0k".to_string())]);

        let (args, env) = muse_args_for(&stdio());
        let config: Value = serde_json::from_str(args[1].split_once('=').unwrap().1).unwrap();
        assert_eq!(config["command"], "/mnt/c/Program Files/Tinto/tinto.exe");
        assert!(env.is_empty());
    }

    #[test]
    fn claude_gets_the_server_and_its_tools_pre_approved() {
        let args = claude_args_for(&http());
        assert_eq!(args[0], "--mcp-config");
        let config: Value = serde_json::from_str(&args[1]).unwrap();
        assert_eq!(
            config["mcpServers"]["tinto-delivery"],
            json!({"type": "http", "url": "http://127.0.0.1:47920/mcp", "headers": {"Authorization": "Bearer t0k"}})
        );
        assert_eq!(&args[2..], ["--allowedTools", "mcp__tinto-delivery"]);

        let args = claude_args_for(&stdio());
        let config: Value = serde_json::from_str(&args[1]).unwrap();
        assert_eq!(
            config["mcpServers"]["tinto-delivery"]["args"],
            json!([PROXY_FLAG, "http://127.0.0.1:47920/mcp", "t0k"])
        );
    }

    #[test]
    fn codex_overrides_are_toml() {
        let args = codex_args_for(&http());
        assert_eq!(
            args,
            [
                "-c",
                r#"mcp_servers.tinto-delivery.url="http://127.0.0.1:47920/mcp""#,
                "-c",
                r#"mcp_servers.tinto-delivery.http_headers={ Authorization = "Bearer t0k" }"#,
                "-c",
                r#"mcp_servers.tinto-delivery.default_tools_approval_mode="approve""#,
            ]
        );
        let args = codex_args_for(&stdio());
        assert_eq!(
            args[3],
            r#"mcp_servers.tinto-delivery.args=["--delivery-mcp-proxy", "http://127.0.0.1:47920/mcp", "t0k"]"#
        );
        for value in args.iter().skip(1).step_by(2) {
            let (_, toml_value) = value.split_once('=').unwrap();
            toml::from_str::<toml::Table>(&format!("v = {toml_value}")).unwrap();
        }
    }

    #[test]
    fn the_proxy_waits_for_a_server_that_listens_wherever_it_moved() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let moved = Target {
            url: format!("http://127.0.0.1:{port}/mcp"),
            token: "t0k".into(),
        };
        // The first read still has the old port; the next one the new.
        let reads = std::cell::Cell::new(0);
        let resolve = || {
            reads.set(reads.get() + 1);
            Some(match reads.get() {
                1 => Target {
                    url: "http://127.0.0.1:1/mcp".into(),
                    token: "t0k".into(),
                },
                _ => moved.clone(),
            })
        };
        assert_eq!(
            wait_for_server(resolve, std::time::Duration::from_secs(5)),
            Some(moved.clone())
        );
        drop(listener);
        // Nobody listens on port 1. The port just freed would not do: a test
        // running alongside may take it at once.
        let nowhere = Target {
            url: "http://127.0.0.1:1/mcp".into(),
            token: "t0k".into(),
        };
        assert_eq!(
            wait_for_server(
                || Some(nowhere.clone()),
                std::time::Duration::from_millis(600)
            ),
            None
        );
        assert_eq!(
            wait_for_server(|| None, std::time::Duration::from_millis(600)),
            None
        );
    }

    #[test]
    fn a_call_tinto_has_not_answered_does_not_hold_up_the_next() {
        use std::io::{BufReader, Read};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let target = Target {
            url: format!(
                "http://127.0.0.1:{}/mcp",
                listener.local_addr().unwrap().port()
            ),
            token: "t0k".into(),
        };
        let (release, released) = std::sync::mpsc::channel::<()>();
        let released = Mutex::new(released);
        let output = Mutex::new(Vec::<u8>::new());
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"slow\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"fast\"}\n";
        let client = reqwest::blocking::Client::new();
        std::thread::scope(|scope| {
            // Moved in: a failing assertion drops it and lets `slow` go.
            let release = release;
            let (listener, released) = (&listener, &released);
            // Tinto: it answers `slow` only once let go, `fast` at once.
            scope.spawn(move || {
                for stream in listener.incoming().take(2) {
                    let mut stream = stream.unwrap();
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut length = 0;
                    loop {
                        let mut line = String::new();
                        reader.read_line(&mut line).unwrap();
                        if line.trim().is_empty() {
                            break;
                        }
                        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).unwrap();
                    scope.spawn(move || {
                        let message: Value = serde_json::from_slice(&body).unwrap();
                        if message["method"] == "slow" {
                            let _ = released.lock().unwrap().recv();
                        }
                        let answer = json!({"jsonrpc": "2.0", "id": message["id"], "result": message["method"]}).to_string();
                        write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len()).unwrap();
                    });
                }
            });
            let written = || String::from_utf8(output.lock().unwrap().clone()).unwrap();
            let (output, client, target) = (&output, &client, &target);
            let proxy = scope.spawn(move || {
                forward_lines(
                    input.as_bytes(),
                    output,
                    client,
                    || Some(target.clone()),
                    None,
                )
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !written().contains("fast") {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the second call waited for the first"
                );
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(!written().contains("slow"));
            release.send(()).unwrap();
            proxy.join().unwrap();
            let lines: Vec<String> = written().lines().map(str::to_string).collect();
            assert_eq!(lines.len(), 2);
            assert!(lines[1].contains("slow"));
        });
    }

    #[test]
    fn unreachable_requests_get_an_error_and_notifications_nothing() {
        let error: Value = serde_json::from_str(&proxy_error(
            r#"{"jsonrpc":"2.0","id":7,"method":"x"}"#,
            "down",
        ))
        .unwrap();
        assert_eq!(error["id"], 7);
        assert!(proxy_error(r#"{"jsonrpc":"2.0","method":"n"}"#, "down").is_empty());
    }
}
