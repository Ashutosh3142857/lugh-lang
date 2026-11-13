//! MCP client: connects Lugh to any Model Context Protocol server over stdio.
//!
//! Servers are configured in `lugh.mcp.json` (or the file named by
//! LUGH_MCP_CONFIG), in the same format as Claude Desktop:
//!
//!   {"mcpServers": {"notes": {"command": "python3", "args": ["notes_server.py"], "env": {}}}}
//!
//! In a program:
//!   use mcp
//!   let tools = mcp.tools "notes"
//!   let note  = mcp.call "notes" "add_note" {title: "Visa", body: "Book biometrics"}
//!
//! Each tool's effect comes from its MCP annotations: `readOnlyHint` → read,
//! `destructiveHint: true` → delete, anything else → submit. Annotations are
//! hints from the server, so only configure servers you trust.

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const PROTOCOL: &str = "2025-06-18";
const TIMEOUT: Duration = Duration::from_secs(60);

struct Client {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
    tools: Vec<Value>,
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn config() -> Result<Map<String, Value>, String> {
    let path = std::env::var("LUGH_MCP_CONFIG").unwrap_or_else(|_| "lugh.mcp.json".into());
    let text = std::fs::read_to_string(&path)
        .map_err(|_| format!("mcp: no server config found at {path} (see docs/MCP.md)"))?;
    let v: Value =
        serde_json::from_str(&text).map_err(|e| format!("mcp: {path} is not valid JSON: {e}"))?;
    v.get("mcpServers")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| format!("mcp: {path} needs an \"mcpServers\" object"))
}

pub fn server_names() -> Vec<String> {
    config()
        .map(|c| c.keys().cloned().collect())
        .unwrap_or_default()
}

impl Client {
    fn start(name: &str) -> Result<Client, String> {
        let cfg = config()?;
        let server = cfg
            .get(name)
            .ok_or_else(|| format!("mcp: no server named '{name}' in the config"))?;
        let command = server["command"]
            .as_str()
            .ok_or_else(|| format!("mcp: server '{name}' has no command"))?;
        let mut cmd = Command::new(command);
        if let Some(args) = server["args"].as_array() {
            cmd.args(args.iter().filter_map(Value::as_str));
        }
        if let Some(env) = server["env"].as_object() {
            for (k, v) in env {
                cmd.env(k, v.as_str().unwrap_or_default());
            }
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("mcp: cannot start server '{name}' ({command}): {e}"))?;
        let stdin = child.stdin.take().ok_or("mcp: no stdin")?;
        let stdout = child.stdout.take().ok_or("mcp: no stdout")?;
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut c = Client {
            child,
            stdin,
            lines,
            next_id: 0,
            tools: vec![],
        };
        c.request(
            "initialize",
            json!({"protocolVersion": PROTOCOL, "capabilities": {}, "clientInfo": {"name": "lugh", "version": env!("CARGO_PKG_VERSION")}}),
        )?;
        c.notify("notifications/initialized")?;
        let mut cursor: Option<String> = None;
        loop {
            let params = match &cursor {
                Some(cur) => json!({"cursor": cur}),
                None => json!({}),
            };
            let page = c.request("tools/list", params)?;
            c.tools
                .extend(page["tools"].as_array().cloned().unwrap_or_default());
            match page["nextCursor"].as_str() {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        Ok(c)
    }

    fn send(&mut self, msg: &Value) -> Result<(), String> {
        writeln!(self.stdin, "{msg}")
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("mcp: server closed its input: {e}"))
    }

    fn notify(&mut self, method: &str) -> Result<(), String> {
        self.send(&json!({"jsonrpc": "2.0", "method": method}))
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let line = self
                .lines
                .recv_timeout(TIMEOUT)
                .map_err(|_| format!("mcp: no reply to {method} within {}s", TIMEOUT.as_secs()))?;
            let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                continue;
            }; // ignore non-JSON log lines
            if msg.get("method").is_some() {
                // A request or notification from the server. Lugh offers no client
                // capabilities, so politely decline requests and ignore notifications.
                if let Some(req_id) = msg.get("id") {
                    self.send(&json!({"jsonrpc": "2.0", "id": req_id, "error": {"code": -32601, "message": "not supported by lugh"}}))?;
                }
                continue;
            }
            if msg.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(err) = msg.get("error") {
                return Err(format!(
                    "mcp: {method} failed: {}",
                    err["message"].as_str().unwrap_or("unknown error")
                ));
            }
            return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

fn clients() -> &'static Mutex<HashMap<String, Client>> {
    static CLIENTS: OnceLock<Mutex<HashMap<String, Client>>> = OnceLock::new();
    CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn with_client<T>(
    server: &str,
    f: impl FnOnce(&mut Client) -> Result<T, String>,
) -> Result<T, String> {
    let mut all = clients().lock().unwrap();
    if !all.contains_key(server) {
        let c = Client::start(server)?;
        all.insert(server.to_string(), c);
    }
    let result = f(all.get_mut(server).unwrap());
    if result
        .as_ref()
        .is_err_and(|e| e.contains("closed") || e.contains("no reply"))
    {
        all.remove(server); // restart on next use
    }
    result
}

fn effect_from(tool: &Value) -> &'static str {
    let a = &tool["annotations"];
    if a["readOnlyHint"].as_bool() == Some(true) {
        "read"
    } else if a["destructiveHint"].as_bool() == Some(true) {
        "delete"
    } else {
        "submit"
    }
}

/// The effect type of one tool, used by the policy gate before the call is made.
pub fn tool_effect(server: &str, tool: &str) -> Result<&'static str, String> {
    with_client(server, |c| {
        c.tools
            .iter()
            .find(|t| t["name"].as_str() == Some(tool))
            .map(effect_from)
            .ok_or_else(|| {
                let names: Vec<&str> = c.tools.iter().filter_map(|t| t["name"].as_str()).collect();
                format!(
                    "mcp: server '{server}' has no tool '{tool}' (tools: {})",
                    names.join(", ")
                )
            })
    })
}

/// Tools a server offers: name, description, Lugh effect type and input schema.
pub fn tools(server: &str) -> Result<Value, String> {
    with_client(server, |c| {
        Ok(Value::Array(
            c.tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t["name"],
                        "description": t["description"],
                        "effect": effect_from(t),
                        "input": t["inputSchema"],
                    })
                })
                .collect(),
        ))
    })
}

pub fn call(server: &str, tool: &str, args: Value) -> Result<Value, String> {
    let args = if args.is_null() { json!({}) } else { args };
    let result = with_client(server, |c| {
        c.request("tools/call", json!({"name": tool, "arguments": args}))
    })?;
    let text: Vec<String> = result["content"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p["text"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if result["isError"].as_bool() == Some(true) {
        return Err(format!(
            "mcp: {server}.{tool} reported an error: {}",
            text.join(" ")
        ));
    }
    if let Some(structured) = result.get("structuredContent").filter(|v| !v.is_null()) {
        return Ok(structured.clone());
    }
    Ok(match text.as_slice() {
        [] => Value::Null,
        [one] => serde_json::from_str(one).unwrap_or_else(|_| Value::String(one.clone())),
        many => Value::String(many.join("\n")),
    })
}
