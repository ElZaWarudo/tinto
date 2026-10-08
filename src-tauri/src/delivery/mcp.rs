//! Delivery's coordinator API: an MCP server (streamable HTTP, JSON
//! responses only) on 127.0.0.1, protected by a bearer token. A coordinator
//! agent (for example the backlog-delivery skill) uses it instead of keeping
//! run state, locks and queues by hand.
//!
//! Every write carries the coordinator's run `owner` and `generation`; a
//! coordinator that was replaced is fenced. Coordinators can only dispatch
//! workspace-access jobs and cannot approve anything: approvals are the
//! user's, in Tinto.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::coordination::NewApproval;
use super::model::{DeliveryAccess, DeliveryAgent, DeliveryCoordinatorEndpoint, DeliveryRung};
use super::service::{DeliveryService, NewJob, NewTask};
use super::DeliveryError;

const PREFERRED_PORT: u16 = 47920;
const MAX_BODY: usize = 4 * 1024 * 1024;

/// Starts the server on a background thread and publishes its endpoint in
/// the overview. The port and token persist so a coordinator's MCP config
/// keeps working across restarts.
pub fn start(service: DeliveryService) -> Result<DeliveryCoordinatorEndpoint, DeliveryError> {
    let (token, port) = {
        let store = service.store()?;
        let token: String = match store.setting::<String>("mcp_token")? {
            Some(token) => token,
            None => {
                let token = uuid::Uuid::new_v4().simple().to_string();
                store.set_setting("mcp_token", &token)?;
                token
            }
        };
        let port = store.setting::<u16>("mcp_port")?.unwrap_or(PREFERRED_PORT);
        (token, port)
    };
    let listener = TcpListener::bind(("127.0.0.1", port))
        .or_else(|_| TcpListener::bind(("127.0.0.1", 0)))
        .map_err(DeliveryError::io)?;
    let port = listener.local_addr().map_err(DeliveryError::io)?.port();
    service.store()?.set_setting("mcp_port", &port)?;
    let endpoint = DeliveryCoordinatorEndpoint {
        url: format!("http://127.0.0.1:{port}/mcp"),
        token: token.clone(),
    };
    if let Ok(mut slot) = service.inner.coordinator.lock() {
        *slot = Some(endpoint.clone());
    }
    super::wiring::publish(&endpoint);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let service = service.clone();
            let token = token.clone();
            std::thread::spawn(move || {
                let _ = serve(stream, &service, &token);
            });
        }
    });
    Ok(endpoint)
}

fn serve(mut stream: TcpStream, service: &DeliveryService, token: &str) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut content_length = 0usize;
    let mut authorized = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        let (name, value) = line.split_once(':').unwrap_or((line.as_str(), ""));
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => content_length = value.parse().unwrap_or(0),
            "authorization" => authorized = value == format!("Bearer {token}"),
            _ => {}
        }
    }
    if path != "/mcp" {
        return respond(&mut stream, 404, "Not Found", None);
    }
    if !authorized {
        return respond(&mut stream, 401, "Unauthorized", None);
    }
    if method != "POST" {
        return respond(&mut stream, 405, "Method Not Allowed", None);
    }
    if content_length > MAX_BODY {
        return respond(&mut stream, 413, "Payload Too Large", None);
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        let error = rpc_error(Value::Null, -32700, "parse error");
        return respond(&mut stream, 200, "OK", Some(&error));
    };
    match handle_message(service, &message) {
        Some(reply) => respond(&mut stream, 200, "OK", Some(&reply)),
        None => respond(&mut stream, 202, "Accepted", None),
    }
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    body: Option<&Value>,
) -> std::io::Result<()> {
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

/// One JSON-RPC message. Notifications get no reply.
pub fn handle_message(service: &DeliveryService, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned()?;
    let method = message["method"].as_str().unwrap_or_default();
    let reply = match method {
        "initialize" => Ok(json!({
            "protocolVersion": message["params"]["protocolVersion"].as_str().unwrap_or("2025-03-26"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "tinto-delivery", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Tinto Delivery: create a run, create tasks (each gets its own worktree and branch), dispatch jobs and wait for their results. Pass your run owner and generation on every write. Approvals for commit, push, PR and Jira are requested here and decided by the user in Tinto."
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => Ok(call_tool(
            service,
            message["params"]["name"].as_str().unwrap_or_default(),
            &message["params"]["arguments"],
        )),
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match reply {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, text)) => rpc_error(id, code, &text),
    })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn call_tool(service: &DeliveryService, name: &str, args: &Value) -> Value {
    match run_tool(service, name, args) {
        Ok(value) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
            "structuredContent": value,
        }),
        Err(error) => json!({
            "content": [{ "type": "text", "text": format!("{}: {}", error.category, error.message) }],
            "isError": true,
        }),
    }
}

fn run_tool(service: &DeliveryService, name: &str, args: &Value) -> Result<Value, DeliveryError> {
    let text = |key: &str| -> Result<String, DeliveryError> {
        args[key]
            .as_str()
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| DeliveryError::new("invalid_arguments", format!("falta {key}")))
    };
    let optional = |key: &str| {
        args[key]
            .as_str()
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty())
    };
    let number = |key: &str| -> Result<u64, DeliveryError> {
        args[key]
            .as_u64()
            .ok_or_else(|| DeliveryError::new("invalid_arguments", format!("falta {key}")))
    };
    // Every write is fenced by the caller's run lock.
    let fence = || -> Result<String, DeliveryError> {
        let run_id = text("run_id")?;
        service.verify_run(&run_id, &text("owner")?, number("generation")?)?;
        Ok(run_id)
    };
    let task_in_run = |run_id: &str, task_id: &str| -> Result<(), DeliveryError> {
        let task = service.task(task_id)?;
        if task.run_id.as_deref() != Some(run_id) {
            return Err(DeliveryError::new(
                "task_not_in_run",
                "la tarea no pertenece a esta ejecución",
            ));
        }
        Ok(())
    };
    match name {
        "delivery_overview" => to_json(&service.overview()?),
        "create_run" => to_json(&service.create_run(
            PathBuf::from(text("repo")?),
            &text("title")?,
            Some(text("owner")?),
        )?),
        "acquire_run" => to_json(&service.acquire_run(&text("run_id")?, &text("owner")?)?),
        "takeover_run" => to_json(&service.takeover_run(
            &text("run_id")?,
            &text("owner")?,
            Some(number("expect_generation")?),
            &text("reason")?,
        )?),
        "release_run" => to_json(&service.release_run(
            &text("run_id")?,
            &text("owner")?,
            number("generation")?,
        )?),
        "create_task" => {
            let run_id = fence()?;
            to_json(&service.create_task(NewTask {
                repo: PathBuf::from(text("repo")?),
                distro: optional("distro"),
                key: text("key")?,
                title: text("title")?,
                base: optional("base"),
                branch: optional("branch"),
                run_id: Some(run_id),
            })?)
        }
        "set_task_state" => {
            let run_id = fence()?;
            let task_id = text("task_id")?;
            task_in_run(&run_id, &task_id)?;
            to_json(&service.update_task(
                &task_id,
                optional("state"),
                args["contract_version"].as_u64().map(|value| value as u32),
                None,
            )?)
        }
        "dispatch_job" => {
            let run_id = fence()?;
            let task_id = text("task_id")?;
            task_in_run(&run_id, &task_id)?;
            let agent = match text("agent")?.as_str() {
                "codex" => DeliveryAgent::Codex,
                "claude" => DeliveryAgent::Claude,
                other => {
                    return Err(DeliveryError::new(
                        "invalid_arguments",
                        format!("agente desconocido: {other} (codex o claude)"),
                    ))
                }
            };
            to_json(&service.dispatch(NewJob {
                task_id,
                role: text("role")?,
                agent,
                model: optional("model"),
                access: DeliveryAccess::Workspace,
                prompt: text("prompt")?,
                writes: args["writes"].as_bool(),
                lease: optional("lease"),
                timeout_minutes: args["timeout_minutes"].as_u64().map(|value| value as u32),
            })?)
        }
        "cancel_job" | "retry_job" => {
            let run_id = fence()?;
            let job = service.job(&text("job_id")?)?;
            task_in_run(&run_id, &job.task_id)?;
            if name == "cancel_job" {
                to_json(&service.cancel(&job.id)?)
            } else {
                to_json(&service.retry(&job.id)?)
            }
        }
        "wait_job" => {
            let job_id = text("job_id")?;
            let timeout =
                Duration::from_secs(args["timeout_seconds"].as_u64().unwrap_or(50).clamp(1, 600));
            let deadline = Instant::now() + timeout;
            loop {
                let job = service.job(&job_id)?;
                if !job.status.is_open() {
                    return Ok(json!({ "done": true, "job": job }));
                }
                if Instant::now() >= deadline {
                    return Ok(json!({ "done": false, "job": job }));
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        "read_job" => to_json(&service.job(&text("job_id")?)?),
        "job_log" => to_json(&service.job_log(
            &text("job_id")?,
            args["from_line"].as_u64().unwrap_or(0) as usize,
        )?),
        "request_approval" => {
            let run_id = fence()?;
            let task_id = text("task_id")?;
            task_in_run(&run_id, &task_id)?;
            let rung = match text("rung")?.as_str() {
                "commit" => DeliveryRung::Commit,
                "push" => DeliveryRung::Push,
                "pr" => DeliveryRung::Pr,
                "jira" => DeliveryRung::Jira,
                other => {
                    return Err(DeliveryError::new(
                        "invalid_arguments",
                        format!("paso desconocido: {other}"),
                    ))
                }
            };
            to_json(&service.request_approval(NewApproval {
                task_id,
                rung,
                title: text("title")?,
                body: optional("body").unwrap_or_default(),
                requested_by: text("owner")?,
            })?)
        }
        "read_approval" => to_json(&service.approval(&text("approval_id")?)?),
        "complete_approval" => {
            let run_id = fence()?;
            let approval = service.approval(&text("approval_id")?)?;
            task_in_run(&run_id, &approval.task_id)?;
            to_json(&service.complete_approval(
                &approval.id,
                args["success"].as_bool().unwrap_or(false),
                &optional("outcome").unwrap_or_default(),
            )?)
        }
        _ => Err(DeliveryError::new(
            "unknown_tool",
            format!("herramienta desconocida: {name}"),
        )),
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<Value, DeliveryError> {
    serde_json::to_value(value)
        .map_err(|error| DeliveryError::new("encode_failed", error.to_string()))
}

fn tool_definitions() -> Value {
    let fenced = |extra: Value| -> Value {
        let mut properties = json!({
            "run_id": { "type": "string" },
            "owner": { "type": "string", "description": "Your coordinator id, the one that holds the run." },
            "generation": { "type": "integer", "description": "The run generation you hold; stale generations are fenced." }
        });
        let mut required = vec![json!("run_id"), json!("owner"), json!("generation")];
        if let (Some(props), Some(extra_props)) =
            (properties.as_object_mut(), extra["properties"].as_object())
        {
            for (key, value) in extra_props {
                props.insert(key.clone(), value.clone());
            }
        }
        if let Some(extra_required) = extra["required"].as_array() {
            required.extend(extra_required.iter().cloned());
        }
        json!({ "type": "object", "properties": properties, "required": required })
    };
    let schema = |properties: Value, required: Value| json!({ "type": "object", "properties": properties, "required": required });
    json!([
        { "name": "delivery_overview", "description": "Runs, tasks, jobs, exclusive resources and approvals.", "inputSchema": schema(json!({}), json!([])) },
        { "name": "create_run", "description": "Start a run you coordinate. Returns its id and generation 1.", "inputSchema": schema(json!({"repo": {"type": "string"}, "title": {"type": "string"}, "owner": {"type": "string"}}), json!(["repo", "title", "owner"])) },
        { "name": "acquire_run", "description": "Take a run nobody coordinates, or confirm you hold it.", "inputSchema": schema(json!({"run_id": {"type": "string"}, "owner": {"type": "string"}}), json!(["run_id", "owner"])) },
        { "name": "takeover_run", "description": "Replace a coordinator that stopped. Only after the user confirmed it stopped.", "inputSchema": schema(json!({"run_id": {"type": "string"}, "owner": {"type": "string"}, "expect_generation": {"type": "integer"}, "reason": {"type": "string"}}), json!(["run_id", "owner", "expect_generation", "reason"])) },
        { "name": "release_run", "description": "Release the run lock when you stop.", "inputSchema": schema(json!({"run_id": {"type": "string"}, "owner": {"type": "string"}, "generation": {"type": "integer"}}), json!(["run_id", "owner", "generation"])) },
        { "name": "create_task", "description": "Create a task: its own worktree and branch from a verified base.", "inputSchema": fenced(json!({"properties": {"repo": {"type": "string"}, "distro": {"type": "string"}, "key": {"type": "string"}, "title": {"type": "string"}, "base": {"type": "string"}, "branch": {"type": "string"}}, "required": ["repo", "key", "title"]})) },
        { "name": "set_task_state", "description": "Set a task's stage label and/or raise its contract version (older results become stale).", "inputSchema": fenced(json!({"properties": {"task_id": {"type": "string"}, "state": {"type": "string"}, "contract_version": {"type": "integer"}}, "required": ["task_id"]})) },
        { "name": "dispatch_job", "description": "Queue a background agent job in a task's worktree. Roles: tests, implementation, review (read-only), qa (read-only, holds the qa resource). Claude jobs without full access can run read-only commands plus the repo's verification commands set in Tinto; anything else is denied. The job ends with a structured result.", "inputSchema": fenced(json!({"properties": {"task_id": {"type": "string"}, "role": {"type": "string"}, "agent": {"type": "string", "enum": ["codex", "claude"]}, "prompt": {"type": "string"}, "model": {"type": "string", "description": "Model id. When omitted, Codex jobs use the default of the account's model catalog as last seen by Tinto (falling back to the CLI's configured model) and Claude jobs use the CLI's default."}, "writes": {"type": "boolean"}, "lease": {"type": "string"}, "timeout_minutes": {"type": "integer"}}, "required": ["task_id", "role", "agent", "prompt"]})) },
        { "name": "wait_job", "description": "Wait until a job ends (or the timeout passes) and return it.", "inputSchema": schema(json!({"job_id": {"type": "string"}, "timeout_seconds": {"type": "integer"}}), json!(["job_id"])) },
        { "name": "read_job", "description": "A job with its status, result, result state, candidates and changed files.", "inputSchema": schema(json!({"job_id": {"type": "string"}}), json!(["job_id"])) },
        { "name": "job_log", "description": "Readable log of a job from a raw line onwards.", "inputSchema": schema(json!({"job_id": {"type": "string"}, "from_line": {"type": "integer"}}), json!(["job_id"])) },
        { "name": "cancel_job", "description": "Cancel a queued or running job.", "inputSchema": fenced(json!({"properties": {"job_id": {"type": "string"}}, "required": ["job_id"]})) },
        { "name": "retry_job", "description": "Queue a new attempt of a finished job; the old attempt's late results become stale.", "inputSchema": fenced(json!({"properties": {"job_id": {"type": "string"}}, "required": ["job_id"]})) },
        { "name": "request_approval", "description": "Ask the user to approve one delivery rung (commit, push, pr or jira) with the exact text. Tinto runs commit and push itself once approved.", "inputSchema": fenced(json!({"properties": {"task_id": {"type": "string"}, "rung": {"type": "string", "enum": ["commit", "push", "pr", "jira"]}, "title": {"type": "string"}, "body": {"type": "string"}}, "required": ["task_id", "rung", "title"]})) },
        { "name": "read_approval", "description": "An approval's status and outcome.", "inputSchema": schema(json!({"approval_id": {"type": "string"}}), json!(["approval_id"])) },
        { "name": "complete_approval", "description": "Report how an approved PR or Jira step went after you ran it.", "inputSchema": fenced(json!({"properties": {"approval_id": {"type": "string"}, "success": {"type": "boolean"}, "outcome": {"type": "string"}}, "required": ["approval_id", "success"]})) }
    ])
}
