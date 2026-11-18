//! A simulated world for the playground, so programs that use adapters can run
//! in the browser without touching anything real.
//!
//! Every call still goes through the policy gate first, exactly as it would
//! with real adapters: only the adapter's effect is simulated. The world holds
//! a few files, an HTTP API at https://api.example.com and five MCP servers
//! (notes, github, payments, mail, database). It is created fresh for every run.

use crate::info::txt;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub const API: &str = "https://api.example.com";

pub struct Sandbox {
    files: BTreeMap<String, String>,
    notes: Vec<Value>,
    issues: Vec<Value>,
    payments: Vec<Value>,
    balance: f64,
    mail_out: Vec<Value>,
    tables: BTreeMap<String, i64>,
}

const FARES: &str = r#"[
  {"airline": "Emirates", "from": "DEL", "to": "DUB", "price": 48200, "stops": 1, "depart": "2026-12-02"},
  {"airline": "Etihad", "from": "DEL", "to": "DUB", "price": 41800, "stops": 1, "depart": "2026-12-02"},
  {"airline": "Qatar Airways", "from": "DEL", "to": "DUB", "price": 43950, "stops": 1, "depart": "2026-12-03"},
  {"airline": "IndiGo", "from": "DEL", "to": "DUB", "price": 39900, "stops": 2, "depart": "2026-12-02"},
  {"airline": "Lufthansa", "from": "DEL", "to": "DUB", "price": 52300, "stops": 1, "depart": "2026-12-04"}
]"#;

const ORDERS: &str = "order_id,customer,email,amount,status,date,reason
A-1001,Priya Shah,priya@example.com,2499,delivered,2026-11-02,
A-1002,Tom Reid,tom@example.com,899,damaged,2026-11-03,Screen cracked on arrival
A-1003,Ana Lima,ana@example.com,15999,delivered,2026-11-03,
A-1004,Wei Chen,wei@example.com,1299,late,2026-11-05,Arrived 9 days late
A-1005,Omar Ali,omar@example.com,349,damaged,2026-11-06,Box crushed
A-1006,Sara Kim,sara@example.com,7499,delivered,2026-11-07,
A-1007,Luca Rossi,luca@example.com,22999,damaged,2026-11-08,Wrong model sent
A-1008,Mia Park,mia@example.com,599,late,2026-11-09,Arrived 4 days late
A-1009,Ravi Iyer,ravi@example.com,1899,delivered,2026-11-10,
A-1010,Emma Wood,emma@example.com,4299,cancelled,2026-11-11,
";

const INBOX: &str = r#"[
  {"from": "lena@acme.example", "subject": "Lunch on Friday?", "body": "Fancy tacos? 1pm works for me."},
  {"from": "billing@cloudhost.example", "subject": "Invoice INV-2041 overdue", "body": "Invoice INV-2041 for INR 18,400 is overdue. Payment is due by 30 November 2026. Reply to this email to confirm."},
  {"from": "sam@acme.example", "subject": "Standup moved", "body": "Standup is now at 10:30 from Monday."},
  {"from": "it@acme.example", "subject": "Password expires soon", "body": "Your password expires in 3 days. Change it from the staff portal."}
]"#;

const RATES: &str = r#"{"base": "INR", "date": "2026-11-14", "rates": {"USD": 0.01184, "EUR": 0.01093, "GBP": 0.00921, "AED": 0.04349}}"#;

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Sandbox {
    pub fn new() -> Self {
        let mut files = BTreeMap::new();
        files.insert("fares.json".into(), FARES.into());
        files.insert("orders.csv".into(), ORDERS.into());
        files.insert("inbox.json".into(), INBOX.into());
        files.insert(
            "config.json".into(),
            r#"{"currency": "INR", "budget": 45000, "refund_limit": 5000, "team": "support"}"#
                .into(),
        );
        files.insert(
            "README.txt".into(),
            "Files in this folder are simulated. Writes last until the run ends.".into(),
        );
        Sandbox {
            files,
            notes: vec![
                json!({"title": "Passport", "body": "Renew before March"}),
                json!({"title": "Visa", "body": "Biometrics appointment pending"}),
            ],
            issues: vec![
                json!({"number": 41, "title": "App crashes when the fare list is empty", "labels": ["bug"], "state": "open"}),
                json!({"number": 42, "title": "Add dark mode to the inbox", "labels": ["enhancement"], "state": "open"}),
                json!({"number": 43, "title": "Crash on startup with an expired token", "labels": ["bug"], "state": "open"}),
                json!({"number": 44, "title": "Typo on the pricing page", "labels": ["docs"], "state": "open"}),
            ],
            payments: vec![],
            balance: 250000.0,
            mail_out: vec![],
            tables: BTreeMap::from([
                ("users".to_string(), 1284),
                ("orders".to_string(), 5310),
                ("sessions".to_string(), 220),
            ]),
        }
    }

    /// The MCP tools each simulated server offers, with standard MCP annotations.
    fn tools(server: &str) -> Option<Vec<Value>> {
        let t = |name: &str, desc: &str, read_only: bool, destructive: bool| {
            json!({"name": name, "description": desc,
                   "annotations": {"readOnlyHint": read_only, "destructiveHint": destructive}})
        };
        Some(match server {
            "notes" => vec![
                t("list_notes", "List all notes", true, false),
                t("add_note", "Add a note {title, body}", false, false),
                t("delete_note", "Delete a note {title}", false, true),
            ],
            "github" => vec![
                t("list_issues", "Open issues {label?}", true, false),
                t("create_issue", "Open an issue {title, body}", false, false),
                t(
                    "comment",
                    "Comment on an issue {number, body}",
                    false,
                    false,
                ),
                t("close_issue", "Close an issue {number}", false, false),
                t("delete_repo", "Delete the repository", false, true),
            ],
            "payments" => vec![
                t("get_balance", "Account balance", true, false),
                t("list_refunds", "Refunds made in this run", true, false),
                t("refund", "Refund an order {order_id, amount}", false, false),
                t("void_all", "Void every payment today", false, true),
            ],
            "mail" => vec![
                t("list_messages", "Messages in the inbox", true, false),
                t(
                    "send_email",
                    "Send an email {to, subject, body}",
                    false,
                    false,
                ),
                t("delete_all", "Delete every message", false, true),
            ],
            "database" => vec![
                t(
                    "list_tables",
                    "Tables in the production database",
                    true,
                    false,
                ),
                t("count_rows", "Count rows {table}", true, false),
                t("insert_rows", "Insert rows {table, rows}", false, false),
                t("delete_rows", "Delete rows {table, where}", false, true),
                t("drop_table", "Drop a table {table}", false, true),
            ],
            _ => return None,
        })
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

    /// Effect type of a simulated MCP tool, decided before the call like the real client.
    pub fn mcp_effect(&self, server: &str, tool: &str) -> Result<&'static str, String> {
        let tools = Self::tools(server).ok_or_else(|| {
            format!(
                "mcp: no server '{server}' in the playground (try notes, github, payments, mail or database)"
            )
        })?;
        tools
            .iter()
            .find(|t| t["name"].as_str() == Some(tool))
            .map(Self::effect_from)
            .ok_or_else(|| {
                let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
                format!(
                    "mcp: server '{server}' has no tool '{tool}' (tools: {})",
                    names.join(", ")
                )
            })
    }

    /// Run one adapter verb against the simulated world. The policy gate has already passed.
    pub fn call(&mut self, adapter: &str, method: &str, args: &[Value]) -> Result<Value, String> {
        let arg = |i: usize| {
            args.get(i)
                .ok_or_else(|| format!("{adapter}.{method} needs argument {}", i + 1))
        };
        match (adapter, method) {
            ("fs", "read") => {
                let p = clean(&txt(arg(0)?));
                self.files
                    .get(&p)
                    .cloned()
                    .map(Value::String)
                    .ok_or_else(|| format!("no file '{p}' (files: {})", self.names().join(", ")))
            }
            ("fs", "list") => Ok(Value::from(self.names())),
            ("fs", "write") => {
                let p = clean(&txt(arg(0)?));
                self.files.insert(p.clone(), as_text(arg(1)?));
                Ok(Value::String(p))
            }
            ("fs", "append") => {
                let p = clean(&txt(arg(0)?));
                let add = as_text(arg(1)?);
                let f = self.files.entry(p.clone()).or_default();
                f.push_str(&add);
                f.push('\n');
                Ok(Value::String(p))
            }
            ("fs", "delete") => {
                let p = clean(&txt(arg(0)?));
                self.files
                    .remove(&p)
                    .map(|_| Value::String(p.clone()))
                    .ok_or_else(|| format!("no file '{p}'"))
            }
            ("http", m) => self.http(m, &txt(arg(0)?), args.get(1)),
            ("shell", "run") => {
                let cmd = txt(arg(0)?);
                let cmd = cmd.trim();
                if let Some(rest) = cmd.strip_prefix("echo ") {
                    Ok(Value::String(rest.trim_matches(['"', '\'']).to_string()))
                } else if cmd == "date" {
                    Ok(Value::String("Sat Nov 14 12:00:00 UTC 2026".into()))
                } else if cmd == "ls" {
                    Ok(Value::String(self.names().join("\n")))
                } else {
                    Err(format!(
                        "the playground only simulates `echo`, `date` and `ls`, not `{cmd}`"
                    ))
                }
            }
            ("db", _) => {
                Err("db isn't simulated in the playground; install Lugh to use SQLite".into())
            }
            ("mcp", "tools") => {
                let server = txt(arg(0)?);
                let tools = Self::tools(&server).ok_or_else(|| {
                    format!("mcp: no server '{server}' in the playground (try notes, github, payments, mail or database)")
                })?;
                Ok(Value::Array(
                    tools
                        .iter()
                        .map(|t| json!({"name": t["name"], "description": t["description"], "effect": Self::effect_from(t)}))
                        .collect(),
                ))
            }
            ("mcp", "call") => {
                let server = txt(arg(0)?);
                let tool = txt(arg(1)?);
                let input = args.get(2).cloned().unwrap_or(Value::Null);
                self.mcp_call(&server, &tool, &input)
            }
            _ => Err(format!("{adapter} has no action '{method}'")),
        }
    }

    fn names(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    fn http(&mut self, method: &str, url: &str, body: Option<&Value>) -> Result<Value, String> {
        let Some(path) = url.strip_prefix(API) else {
            return Err(format!(
                "the playground only simulates {API} (no real network here); asked for {url}"
            ));
        };
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let q: Map<String, Value> = query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect();
        let body_json = |v: Value| Ok(Value::String(v.to_string()));
        match (method, path.trim_end_matches('/')) {
            ("get", "/fares") => {
                let all: Value = serde_json::from_str(FARES).unwrap();
                let mut list = all.as_array().cloned().unwrap_or_default();
                if let Some(Value::String(to)) = q.get("to") {
                    list.retain(|f| f["to"].as_str() == Some(to.as_str()));
                }
                if let Some(Value::String(max)) = q.get("max") {
                    let max: f64 = max.parse().unwrap_or(f64::MAX);
                    list.retain(|f| f["price"].as_f64().unwrap_or(0.0) <= max);
                }
                body_json(Value::Array(list))
            }
            ("get", "/rates") => Ok(Value::String(RATES.into())),
            ("get", "/orders") => {
                let rows = crate::stdlib::call("csv.parse", &[Value::String(ORDERS.into())])
                    .map_err(|e| e.to_string())?;
                body_json(rows)
            }
            ("get", p) if p.starts_with("/weather/") => {
                let city = &p["/weather/".len()..];
                let (temp, sky) = match city.to_lowercase().as_str() {
                    "dublin" => (9, "light rain"),
                    "delhi" => (24, "haze"),
                    "dubai" => (29, "clear"),
                    "london" => (11, "cloudy"),
                    _ => (18, "partly cloudy"),
                };
                body_json(json!({"city": city, "temp_c": temp, "sky": sky}))
            }
            ("get", "/slow") => Err("timed out after 30s (simulated)".into()),
            ("get", "/flaky") => Err("503 Service Unavailable (simulated)".into()),
            ("get", p) => Err(format!(
                "404 Not Found: {p} (try /fares, /rates, /orders, /weather/<city>)"
            )),
            (m, p) => body_json(
                json!({"ok": true, "method": m.to_uppercase(), "path": p, "received": body.cloned().unwrap_or(Value::Null)}),
            ),
        }
    }

    fn mcp_call(&mut self, server: &str, tool: &str, input: &Value) -> Result<Value, String> {
        let s = |k: &str| input.get(k).map(txt).unwrap_or_default();
        match (server, tool) {
            ("notes", "list_notes") => Ok(Value::Array(self.notes.clone())),
            ("notes", "add_note") => {
                let n = json!({"title": s("title"), "body": s("body")});
                self.notes.push(n.clone());
                Ok(n)
            }
            ("notes", "delete_note") => {
                let t = s("title");
                let before = self.notes.len();
                self.notes
                    .retain(|n| n["title"].as_str() != Some(t.as_str()));
                Ok(json!({"deleted": before - self.notes.len()}))
            }
            ("github", "list_issues") => {
                let label = s("label");
                Ok(Value::Array(
                    self.issues
                        .iter()
                        .filter(|i| i["state"] == "open")
                        .filter(|i| {
                            label.is_empty()
                                || i["labels"].as_array().is_some_and(|l| {
                                    l.iter().any(|x| x == &Value::String(label.clone()))
                                })
                        })
                        .cloned()
                        .collect(),
                ))
            }
            ("github", "create_issue") => {
                let n = 41 + self.issues.len() as i64;
                let issue = json!({"number": n, "title": s("title"), "body": s("body"), "labels": [], "state": "open"});
                self.issues.push(issue.clone());
                Ok(issue)
            }
            ("github", "comment") => {
                Ok(json!({"issue": input.get("number"), "comment": s("body")}))
            }
            ("github", "close_issue") => {
                let n = input.get("number").and_then(Value::as_i64).unwrap_or(-1);
                let found = self
                    .issues
                    .iter_mut()
                    .find(|i| i["number"].as_i64() == Some(n));
                match found {
                    Some(i) => {
                        i["state"] = json!("closed");
                        Ok(i.clone())
                    }
                    None => Err(format!("no issue #{n}")),
                }
            }
            ("github", "delete_repo") => Ok(json!({"deleted": true})),
            ("payments", "get_balance") => Ok(json!({"currency": "INR", "balance": self.balance})),
            ("payments", "list_refunds") => Ok(Value::Array(self.payments.clone())),
            ("payments", "refund") => {
                let amount = input.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
                if amount <= 0.0 {
                    return Err("refund needs a positive amount".into());
                }
                if amount > self.balance {
                    return Err("insufficient balance".into());
                }
                self.balance -= amount;
                let r = json!({"order_id": s("order_id"), "amount": amount, "status": "refunded"});
                self.payments.push(r.clone());
                Ok(r)
            }
            ("payments", "void_all") => Ok(json!({"voided": self.payments.len()})),
            ("mail", "list_messages") => serde_json::from_str(INBOX).map_err(|e| e.to_string()),
            ("mail", "send_email") => {
                let m = json!({"to": s("to"), "subject": s("subject"), "body": s("body"), "status": "sent (simulated)"});
                self.mail_out.push(m.clone());
                Ok(m)
            }
            ("mail", "delete_all") => Ok(json!({"deleted": 4})),
            ("database", "list_tables") => {
                Ok(Value::from(self.tables.keys().cloned().collect::<Vec<_>>()))
            }
            ("database", "count_rows") => {
                let t = s("table");
                self.tables
                    .get(&t)
                    .map(|n| json!(n))
                    .ok_or_else(|| format!("no table '{t}'"))
            }
            ("database", "insert_rows") => {
                let t = s("table");
                let n = input
                    .get("rows")
                    .and_then(Value::as_array)
                    .map_or(0, |r| r.len() as i64);
                let count = self
                    .tables
                    .get_mut(&t)
                    .ok_or_else(|| format!("no table '{t}'"))?;
                *count += n;
                Ok(json!({"inserted": n}))
            }
            ("database", "delete_rows") => {
                let t = s("table");
                let count = self
                    .tables
                    .get_mut(&t)
                    .ok_or_else(|| format!("no table '{t}'"))?;
                let gone = *count;
                *count = 0;
                Ok(json!({"deleted": gone}))
            }
            ("database", "drop_table") => {
                let t = s("table");
                self.tables
                    .remove(&t)
                    .map(|_| json!({"dropped": t}))
                    .ok_or_else(|| format!("no table '{t}'"))
            }
            _ => Err(format!("mcp: server '{server}' has no tool '{tool}'")),
        }
    }
}

fn clean(p: &str) -> String {
    p.trim_start_matches("./").to_string()
}

fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

/// A short description of the simulated world, shown by the playground.
pub fn describe() -> &'static str {
    "Files: fares.json, orders.csv, inbox.json, config.json, README.txt
HTTP:  GET https://api.example.com/fares[?to=DUB&max=45000], /rates, /orders, /weather/<city>, /flaky, /slow
       POST, PUT and DELETE to any api.example.com path echo what they receive
MCP:   notes    list_notes (read), add_note (submit), delete_note (delete)
       github   list_issues (read), create_issue, comment, close_issue (submit), delete_repo (delete)
       payments get_balance, list_refunds (read), refund (submit), void_all (delete)
       mail     list_messages (read), send_email (submit), delete_all (delete)
       database list_tables, count_rows (read), insert_rows (submit), delete_rows, drop_table (delete)
Shell: echo, date and ls only"
}
