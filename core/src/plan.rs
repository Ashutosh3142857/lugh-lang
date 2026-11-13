//! `lugh plan "<goal>"`: an LLM writes a Lugh program for a goal, the compiler
//! checks it (and asks the model to repair it if it doesn't compile), and a
//! person reviews the program and its policy report before anything runs.

use crate::info::txt;
use serde_json::{json, Value};

const GUIDE: &str = r#"You write programs in Lugh, a language for AI agents. Reply with ONE complete program in a ```lugh code block and nothing else.

STRUCTURE (indentation = 2 spaces per level)
agent <Name>
  use <adapters, comma separated>
  budget time 2m, calls 30
  policy
    allow read
    ask human before write, send, submit, spend, execute
    deny delete
  goal "<what this achieves>"
    <statements>

STATEMENTS
let x = <expr>
log "text with {x.field} placeholders"
ensure <cond> else stop "<why>"
when <cond>            (indented block, optional sibling `otherwise` block)
each item in <list>    (indented block)
remember "key" = <expr>      recall "key"
ask human "<question>?" -> ok      (then: when ok ...)
let x = attempt        (indented block; sibling `otherwise` block runs if it fails)
stop "<reason>"

EXPRESSIONS
numbers, "text", true, false, none, [1, 2], {key: value}, a.b.0, - + * /, == != < > <= >=, and, or, not
filter x in xs where <cond>     map x in xs to <expr>     sort xs by field [desc]     group xs by field
count(xs)  sum(xs)  avg(xs)  min(xs)  max(xs)  round(x, 2)  upper(s)  lower(s)  trim(s)  split(s, ",")  join(xs, ", ")
think "<instruction>" with <data>      (asks an LLM; returns text)
Standard library functions (call as name(args)): {STDLIB}

ADAPTERS (only these touch the outside world; each verb has an effect checked by policy)
{ADAPTERS}

RULES
- Use only the adapters listed above and only the functions listed above.
- Adapter calls take space-separated arguments, no parentheses: fs.read "notes.txt"   db.query "SELECT * FROM t WHERE id = ?" [7]   mcp.call "server" "tool" {arg: 1}
- Prefer reading and summarising; anything that writes, sends or deletes must go through policy (ask human before it).
- Keep the program short and clear. Use log to report what was found.
"#;

fn adapter_docs(adapters: &[String]) -> String {
    let mut out = Vec::new();
    for a in adapters {
        out.push(match a.as_str() {
            "fs" => "fs.read \"path\" [as json|lines]  (read) · fs.list \"dir\" (read) · fs.write \"path\" value (write) · fs.append \"path\" value (write)".to_string(),
            "http" => "http.get \"url\" [as json] (read) · http.post \"url\" {body} (send)".to_string(),
            "shell" => "shell.run \"command\" (execute)".to_string(),
            "db" => "db.query \"SQL with ? params\" [params] (read, returns rows as objects) · db.exec \"SQL\" [params] (write)".to_string(),
            "llm" => "think \"instruction\" with data (read) — requires `use llm`".to_string(),
            "mcp" => {
                let mut lines = vec!["mcp.tools \"server\" (read) · mcp.call \"server\" \"tool\" {arguments} (effect set by the tool)".to_string()];
                for server in crate::mcp::server_names() {
                    match crate::mcp::tools(&server) {
                        Ok(Value::Array(tools)) => {
                            lines.push(format!("  MCP server \"{server}\" tools:"));
                            for t in tools {
                                lines.push(format!(
                                    "    - {} [{}]: {} — arguments: {}",
                                    txt(&t["name"]),
                                    txt(&t["effect"]),
                                    t["description"].as_str().unwrap_or(""),
                                    t["input"]["properties"]
                                ));
                            }
                        }
                        Ok(_) => {}
                        Err(e) => lines.push(format!("  MCP server \"{server}\" unavailable: {e}")),
                    }
                }
                lines.join("\n")
            }
            other => format!("{other}: (unknown adapter)"),
        });
    }
    if out.is_empty() {
        "(none: the program may only compute and log)".into()
    } else {
        out.join("\n")
    }
}

fn ask_model(messages: &[Value], system: &str) -> Result<String, String> {
    let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        "lugh plan needs ANTHROPIC_API_KEY (or LUGH_LLM_URL for a compatible endpoint)"
    })?;
    let url = std::env::var("LUGH_LLM_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".into());
    let model = std::env::var("LUGH_MODEL").unwrap_or_else(|_| "claude-sonnet-5-5".into());
    let body = json!({"model": model, "max_tokens": 2500, "system": system, "messages": messages});
    let resp: Value = crate::adapters::http_agent_for(&url)
        .post(&url)
        .set("x-api-key", &key)
        .set("anthropic-version", "2023-06-01")
        .send_json(body)
        .map_err(|e| format!("model request failed: {e}"))?
        .into_json()
        .map_err(|e| format!("model reply was not JSON: {e}"))?;
    resp["content"][0]["text"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| format!("unexpected model reply: {resp}"))
}

fn extract_program(reply: &str) -> String {
    for fence in ["```lugh", "```"] {
        if let Some(start) = reply.find(fence) {
            let body = &reply[start + fence.len()..];
            if let Some(end) = body.find("```") {
                return body[..end].trim_matches('\n').to_string();
            }
        }
    }
    reply.trim().to_string()
}

/// Generate a program for `goal`. `compile` returns Err(message) if the program
/// does not compile; the model gets the message and up to `attempts` tries.
pub fn generate(
    goal: &str,
    adapters: &[String],
    attempts: usize,
    compile: impl Fn(&str) -> Result<(), String>,
) -> Result<String, String> {
    let system = GUIDE
        .replace("{STDLIB}", &crate::stdlib::NAMES.join(", "))
        .replace("{ADAPTERS}", &adapter_docs(adapters));
    let mut messages = vec![
        json!({"role": "user", "content": format!("Goal: {goal}\nAvailable adapters: {}", if adapters.is_empty() { "none".into() } else { adapters.join(", ") })}),
    ];
    let mut last_err = String::new();
    for attempt in 1..=attempts {
        let reply = ask_model(&messages, &system)?;
        let program = extract_program(&reply);
        match compile(&program) {
            Ok(()) => {
                if attempt > 1 {
                    println!(
                        "  ♻ the model repaired its program after {} compile error(s)",
                        attempt - 1
                    );
                }
                return Ok(program);
            }
            Err(e) => {
                println!("  ✗ attempt {attempt}: {e}");
                last_err = e.clone();
                messages.push(json!({"role": "assistant", "content": reply}));
                messages.push(json!({"role": "user", "content": format!("That program does not compile: {e}\nReturn the complete corrected program.")}));
            }
        }
    }
    Err(format!("the model could not produce a compiling program in {attempts} attempts; last error: {last_err}"))
}
