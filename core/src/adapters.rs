//! Adapters: the bridge from Lugh verbs to existing infrastructure.
//! Each verb declares exactly one effect type, which the policy gate checks.

use crate::info::txt;
use serde_json::Value;
use std::io::Write;
use std::process::Command;
#[cfg(feature = "native")]
use std::time::Duration;

pub const KNOWN: [&str; 6] = ["http", "fs", "shell", "llm", "db", "mcp"];

pub fn effect_of(adapter: &str, method: &str) -> Option<&'static str> {
    Some(match (adapter, method) {
        ("http", "get") => "read",
        ("http", "post") => "send",
        ("http", "put") => "write",
        ("http", "delete") => "delete",
        ("fs", "read") | ("fs", "list") => "read",
        ("fs", "write") | ("fs", "append") => "write",
        ("fs", "delete") => "delete",
        ("shell", "run") => "execute",
        ("db", "query") => "read",
        ("db", "exec") => "write",
        ("mcp", "tools") => "read",
        // Decided per tool from its MCP annotations, just before the call.
        ("mcp", "call") => "dynamic",
        _ => return None,
    })
}

#[cfg(feature = "native")]
/// True when `url` should bypass the proxy: loopback hosts, or a match in NO_PROXY.
fn no_proxy(url: &str) -> bool {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', ':', '?'])
        .next()
        .unwrap_or("")
        .to_lowercase();
    if host == "localhost" || host == "::1" || host.starts_with("127.") || host.starts_with("[::1]")
    {
        return true;
    }
    let list = std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .unwrap_or_default();
    list.split(',')
        .map(|d| d.trim().trim_start_matches('.').to_lowercase())
        .filter(|d| !d.is_empty())
        .any(|d| d == "*" || host == d || host.ends_with(&format!(".{d}")))
}

#[cfg(feature = "native")]
/// HTTP client for `url`: honours HTTPS_PROXY, except for loopback and NO_PROXY hosts.
pub fn http_agent_for(url: &str) -> ureq::Agent {
    let mut b = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("lugh/", env!("CARGO_PKG_VERSION")));
    if !no_proxy(url) {
        if let Some(p) = std::env::var("HTTPS_PROXY")
            .ok()
            .or_else(|| std::env::var("https_proxy").ok())
        {
            if let Ok(proxy) = ureq::Proxy::new(p) {
                b = b.proxy(proxy);
            }
        }
    }
    b.build()
}

pub fn call(adapter: &str, method: &str, args: &[Value]) -> Result<Value, String> {
    let arg = |i: usize| {
        args.get(i)
            .ok_or_else(|| format!("{adapter}.{method} needs argument {}", i + 1))
    };
    match (adapter, method) {
        #[cfg(feature = "native")]
        ("http", m) => {
            let url = txt(arg(0)?);
            let req = http_agent_for(&url).request(&m.to_uppercase(), &url);
            let resp = match args.get(1) {
                Some(body) => req.send_json(body.clone()),
                None => req.call(),
            }
            .map_err(|e| e.to_string())?;
            resp.into_string()
                .map(Value::String)
                .map_err(|e| e.to_string())
        }
        ("fs", "read") => std::fs::read_to_string(txt(arg(0)?))
            .map(Value::String)
            .map_err(|e| e.to_string()),
        ("fs", "list") => {
            let mut names: Vec<String> = std::fs::read_dir(txt(arg(0)?))
                .map_err(|e| e.to_string())?
                .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
                .collect();
            names.sort();
            Ok(Value::from(names))
        }
        ("fs", "write") => {
            let p = txt(arg(0)?);
            std::fs::write(&p, as_text(arg(1)?)).map_err(|e| e.to_string())?;
            Ok(Value::String(p))
        }
        ("fs", "append") => {
            let p = txt(arg(0)?);
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&p)
                .map_err(|e| e.to_string())?;
            writeln!(f, "{}", as_text(arg(1)?)).map_err(|e| e.to_string())?;
            Ok(Value::String(p))
        }
        ("fs", "delete") => {
            let p = txt(arg(0)?);
            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
            Ok(Value::String(p))
        }
        ("shell", "run") => {
            let out = Command::new("sh")
                .arg("-c")
                .arg(txt(arg(0)?))
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(format!(
                    "shell exited {}: {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            Ok(Value::String(
                String::from_utf8_lossy(&out.stdout).trim().to_string(),
            ))
        }
        #[cfg(feature = "native")]
        ("db", m) => db(m, &txt(arg(0)?), args.get(1)),
        #[cfg(feature = "native")]
        ("mcp", "tools") => crate::mcp::tools(&txt(arg(0)?)),
        #[cfg(feature = "native")]
        ("mcp", "call") => crate::mcp::call(
            &txt(arg(0)?),
            &txt(arg(1)?),
            args.get(2).cloned().unwrap_or(Value::Null),
        ),
        #[cfg(not(feature = "native"))]
        ("http" | "db" | "mcp", _) => Err(format!(
            "{adapter} is not available in this build (browser or playground)"
        )),
        _ => Err(format!("{adapter} has no action '{method}'")),
    }
}

#[cfg(feature = "native")]
/// SQLite through the policy gate: `db.query` (read) returns rows as objects,
/// `db.exec` (write) returns the number of changed rows. Parameters are always
/// bound, never spliced into SQL. The file is LUGH_DB, or lugh.db by default.
fn db(method: &str, sql: &str, params: Option<&Value>) -> Result<Value, String> {
    use rusqlite::types::Value as Sql;
    let path = std::env::var("LUGH_DB").unwrap_or_else(|_| "lugh.db".into());
    let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let to_sql = |v: &Value| match v {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(*b as i64),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Sql::Integer(i),
            None => Sql::Real(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => Sql::Text(s.clone()),
        other => Sql::Text(other.to_string()),
    };
    let bound: Vec<Sql> = match params {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(a)) => a.iter().map(to_sql).collect(),
        Some(other) => vec![to_sql(other)],
    };
    let refs = rusqlite::params_from_iter(bound.iter());
    if method == "exec" {
        return conn
            .execute(sql, refs)
            .map(Value::from)
            .map_err(|e| e.to_string());
    }
    if method != "query" {
        return Err(format!("db has no action '{method}' (query, exec)"));
    }
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let cols: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    let rows = stmt
        .query_map(refs, |row| {
            let mut obj = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                let v = match row.get::<_, Sql>(i)? {
                    Sql::Null => Value::Null,
                    Sql::Integer(n) => Value::from(n),
                    Sql::Real(f) => crate::parser::num(f),
                    Sql::Text(t) => Value::String(t),
                    Sql::Blob(b) => Value::String(format!("<{} bytes>", b.len())),
                };
                obj.insert(c.clone(), v);
            }
            Ok(Value::Object(obj))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
        .map_err(|e| e.to_string())
}

fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn cast(v: Value, to: Option<&str>) -> Result<Value, String> {
    match to {
        None => Ok(v),
        Some("json") => match v {
            Value::String(s) => {
                serde_json::from_str(&s).map_err(|e| format!("not valid JSON: {e}"))
            }
            other => Ok(other),
        },
        Some("number") => txt(&v)
            .trim()
            .parse::<f64>()
            .map(crate::parser::num)
            .map_err(|e| e.to_string()),
        Some("text") => Ok(Value::String(as_text(&v))),
        Some("lines") => Ok(Value::from(
            txt(&v)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(String::from)
                .collect::<Vec<_>>(),
        )),
        Some(other) => Err(format!("unknown type '{other}'")),
    }
}

/// The only probabilistic step. Uses an LLM API when ANTHROPIC_API_KEY is set, else a local stub.
pub fn think(prompt: &str) -> Result<String, String> {
    let key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|_| cfg!(feature = "native"));
    let Some(key) = key else {
        let first = prompt.lines().next().unwrap_or_default();
        return Ok(format!(
            "[llm stub] would reason about: {}",
            first.chars().take(80).collect::<String>()
        ));
    };
    think_remote(prompt, &key)
}

#[cfg(not(feature = "native"))]
fn think_remote(_prompt: &str, _key: &str) -> Result<String, String> {
    Err("think needs network access, which this build does not have".into())
}

#[cfg(feature = "native")]
fn think_remote(prompt: &str, key: &str) -> Result<String, String> {
    let model = std::env::var("LUGH_MODEL").unwrap_or_else(|_| "claude-sonnet-5-5".into());
    let body = serde_json::json!({"model": model, "max_tokens": 400, "messages": [{"role": "user", "content": prompt}]});
    let url = std::env::var("LUGH_LLM_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".into());
    let resp: Value = http_agent_for(&url)
        .post(&url)
        .set("x-api-key", key)
        .set("anthropic-version", "2023-06-01")
        .send_json(body)
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    Ok(resp["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string())
}
