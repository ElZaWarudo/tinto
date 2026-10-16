//! Connects conversations started in Agents to the coordinator API, so a
//! Claude Code or Codex conversation can coordinate Delivery without setup.
//!
//! CLIs on Windows reach the server over HTTP. Inside WSL (NAT networking),
//! `127.0.0.1` is not Windows, so the CLI runs this same executable as a
//! stdio MCP server ([`PROXY_FLAG`]) that forwards each message to it.

use std::io::{BufRead, Write};
use std::sync::Mutex;

use serde_json::{json, Value};

use super::model::DeliveryCoordinatorEndpoint;

pub const SERVER_NAME: &str = "tinto-delivery";
/// `tinto.exe --delivery-mcp-proxy <url> <token>` serves MCP on stdio.
pub const PROXY_FLAG: &str = "--delivery-mcp-proxy";

static ENDPOINT: Mutex<Option<DeliveryCoordinatorEndpoint>> = Mutex::new(None);

pub(super) fn publish(endpoint: &DeliveryCoordinatorEndpoint) {
    if let Ok(mut slot) = ENDPOINT.lock() {
        *slot = Some(endpoint.clone());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Server {
    Http { url: String, token: String },
    Stdio { command: String, args: Vec<String> },
}

fn server(wsl: bool) -> Option<Server> {
    let endpoint = ENDPOINT.lock().ok()?.clone()?;
    if !wsl {
        return Some(Server::Http {
            url: endpoint.url,
            token: endpoint.token,
        });
    }
    let exe = std::env::current_exe().ok()?;
    let command = crate::wsl_agent::launcher::windows_path_to_wsl_mount(&exe).ok()?;
    Some(Server::Stdio {
        command,
        args: vec![PROXY_FLAG.to_string(), endpoint.url, endpoint.token],
    })
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

fn claude_args_for(server: &Server) -> Vec<String> {
    let config = match server {
        Server::Http { url, token } => json!({
            "type": "http",
            "url": url,
            "headers": { "Authorization": format!("Bearer {token}") },
        }),
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
        Server::Http { url, token } => vec![
            format!("{}={}", key("url"), text(url)),
            format!(
                "{}={{ Authorization = {} }}",
                key("http_headers"),
                text(&format!("Bearer {token}"))
            ),
        ],
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
pub fn proxy_stdio(given: Option<(&str, &str)>) -> i32 {
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
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut target = resolve();
    let mut opened = false;
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let send = |target: &Option<Target>| {
            let target = target.as_ref()?;
            Some(
                client
                    .post(&target.url)
                    .bearer_auth(&target.token)
                    .header("Content-Type", "application/json")
                    .header("Accept", "application/json")
                    .body(line.clone())
                    .send(),
            )
        };
        let unreachable = |sent: &Option<reqwest::Result<reqwest::blocking::Response>>| {
            sent.as_ref()
                .is_none_or(|sent| sent.as_ref().is_err_and(reqwest::Error::is_connect))
        };
        let mut sent = send(&target);
        // Nobody listening there: Tinto restarted on another port, or is
        // closed. Read where it listens now; if nobody does, open Tinto,
        // once, and wait for its server, so an agent never has to start
        // Tinto some other way or be set up again.
        if unreachable(&sent) {
            target = resolve();
            sent = send(&target);
        }
        if unreachable(&sent) && !opened {
            opened = true;
            if open_tinto() {
                if let Some(found) = wait_for_server(resolve, std::time::Duration::from_secs(60)) {
                    target = Some(found);
                    sent = send(&target);
                }
            }
        }
        let answer = match sent.map(|sent| sent.and_then(|response| response.text())) {
            Some(Ok(body)) => body,
            Some(Err(error)) => proxy_error(&line, &error.to_string()),
            None => proxy_error(&line, "Tinto no ha iniciado nunca su servidor"),
        };
        if answer.trim().is_empty() {
            continue;
        }
        if writeln!(stdout, "{}", answer.trim()).is_err() || stdout.flush().is_err() {
            break;
        }
    }
    0
}

/// Starts this same executable as the Tinto app, detached from the agent
/// that runs the proxy, so closing the agent does not close Tinto.
fn open_tinto() -> bool {
    let Ok(exe) = std::env::current_exe() else {
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

    fn http() -> Server {
        Server::Http {
            url: "http://127.0.0.1:47920/mcp".into(),
            token: "t0k".into(),
        }
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
        assert_eq!(
            wait_for_server(
                || Some(moved.clone()),
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
