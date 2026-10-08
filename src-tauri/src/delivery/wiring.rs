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

/// Runs the stdio side of the bridge: one JSON-RPC message per line in,
/// the server's answer (if any) per line out.
pub fn proxy_stdio(url: &str, token: &str) -> i32 {
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
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = client
            .post(url)
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(line.clone())
            .send()
            .and_then(|response| response.text());
        let answer = match reply {
            Ok(body) => body,
            Err(error) => proxy_error(&line, &error.to_string()),
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
