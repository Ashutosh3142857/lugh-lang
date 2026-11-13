//! Lugh Web: the backend and frontend frameworks, built into the runtime.
//!
//! Backend:  `route GET "/fares/:id"` blocks become HTTP endpoints. The request is
//!           available as `request` (method, path, params, query, body). `respond`
//!           ends the request. `ask human` waits for a decision in the web inbox.
//! Frontend: `page "/" title "…"` blocks declare a UI with a handful of elements.
//!           Pages are rendered on the server to plain HTML with a ~3 KB script:
//!           no build step, no client framework.
//! Built in: /_lugh/inbox (approvals), /_lugh/api/approvals, /_lugh/api/audit.
//!
//! The HTTP layer is a deliberately small HTTP/1.1 implementation on std::net:
//! one thread per connection, `Connection: close`, bodies up to 1 MB.

use crate::parser::{tokenize, Block, Line, Tok};
use crate::runtime::{Approvals, Env, Flow, Runtime};
use serde_json::{Map, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

const MAX_BODY: usize = 1 << 20;

pub struct Route {
    pub method: String,
    pub segments: Vec<String>,
    pub path: String,
    pub body: Block,
}

pub struct Page {
    pub path: String,
    pub title: String,
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone)]
pub enum Element {
    Heading(String),
    Text(String),
    Link {
        label: String,
        to: String,
    },
    Table {
        from: String,
        columns: Vec<String>,
    },
    List {
        from: String,
        show: Option<String>,
    },
    Stat {
        from: String,
        field: Option<String>,
        label: String,
    },
    Form {
        to: String,
        fields: Vec<String>,
        submit: String,
    },
    Button {
        label: String,
        calls: String,
    },
    Approvals,
    Audit,
}

// ───────────────────────────── declarations ─────────────────────────────

pub fn parse_route(line: &Line, adapters: &[String]) -> Result<Route, String> {
    let rest = line.text["route".len()..].trim();
    let (method, path) = rest
        .split_once(' ')
        .ok_or(format!("line {}: route <METHOD> \"/path\"", line.no))?;
    let method = method.to_uppercase();
    if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
        return Err(format!("line {}: unsupported method '{method}'", line.no));
    }
    let path = path.trim().trim_matches('"').to_string();
    if !path.starts_with('/') {
        return Err(format!("line {}: route path must start with '/'", line.no));
    }
    Ok(Route {
        segments: split_path(&path),
        body: crate::parser::parse_block(&line.children, adapters)?,
        method,
        path,
    })
}

fn strings(toks: &[Tok]) -> Vec<String> {
    toks.iter()
        .filter_map(|t| {
            if let Tok::Str(s) = t {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Names listed after `keyword` up to the next keyword: `columns a, b, c`.
fn names_after(toks: &[Tok], keyword: &str, stop: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut on = false;
    for t in toks {
        match t {
            Tok::Name(n) if n == keyword => on = true,
            Tok::Name(n) if on && stop.contains(&n.as_str()) => break,
            Tok::Name(n) if on => out.push(n.clone()),
            Tok::Op(o) if on && o == "," => {}
            _ if on => break,
            _ => {}
        }
    }
    out
}

fn string_after(toks: &[Tok], keyword: &str) -> Option<String> {
    toks.windows(2).find_map(|w| match (&w[0], &w[1]) {
        (Tok::Name(k), Tok::Str(s)) if k == keyword => Some(s.clone()),
        _ => None,
    })
}

pub fn parse_page(line: &Line) -> Result<Page, String> {
    let toks = tokenize(line.text["page".len()..].trim())?;
    let path = strings(&toks)
        .first()
        .cloned()
        .ok_or(format!("line {}: page \"/path\" title \"…\"", line.no))?;
    let title = string_after(&toks, "title").unwrap_or_else(|| path.clone());
    let mut elements = Vec::new();
    for el in &line.children {
        let toks = tokenize(&el.text)?;
        let kind = match toks.first() {
            Some(Tok::Name(n)) => n.as_str(),
            _ => return Err(format!("line {}: expected a page element", el.no)),
        };
        let first_str = strings(&toks).first().cloned();
        let need = |what: &str| format!("line {}: {kind} needs {what}", el.no);
        elements.push(match kind {
            "heading" => Element::Heading(first_str.ok_or_else(|| need("text"))?),
            "text" => Element::Text(first_str.ok_or_else(|| need("text"))?),
            "link" => Element::Link {
                label: first_str.ok_or_else(|| need("a label"))?,
                to: string_after(&toks, "to").ok_or_else(|| need("to \"/path\""))?,
            },
            "table" => Element::Table {
                from: string_after(&toks, "from").ok_or_else(|| need("from \"/api\""))?,
                columns: names_after(&toks, "columns", &[]),
            },
            "list" => Element::List {
                from: string_after(&toks, "from").ok_or_else(|| need("from \"/api\""))?,
                show: names_after(&toks, "show", &[]).into_iter().next(),
            },
            "stat" => Element::Stat {
                from: string_after(&toks, "from").ok_or_else(|| need("from \"/api\""))?,
                field: names_after(&toks, "field", &["label"]).into_iter().next(),
                label: string_after(&toks, "label").unwrap_or_default(),
            },
            "form" => Element::Form {
                to: string_after(&toks, "to").ok_or_else(|| need("to \"/api\""))?,
                fields: names_after(&toks, "fields", &["submit"]),
                submit: string_after(&toks, "submit").unwrap_or_else(|| "Submit".into()),
            },
            "button" => Element::Button {
                label: first_str.ok_or_else(|| need("a label"))?,
                calls: string_after(&toks, "calls").ok_or_else(|| need("calls \"/api\""))?,
            },
            "approvals" => Element::Approvals,
            "audit" => Element::Audit,
            other => return Err(format!("line {}: unknown page element '{other}'", el.no)),
        });
    }
    Ok(Page {
        path,
        title,
        elements,
    })
}

// ───────────────────────────── HTTP ─────────────────────────────

struct Request {
    method: String,
    path: String,
    query: Map<String, Value>,
    body: Value,
}

fn split_path(p: &str) -> Vec<String> {
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Form and query values: numbers become numbers, everything else stays text.
fn field_value(s: &str) -> Value {
    match s.parse::<f64>() {
        Ok(n) if !s.is_empty() => crate::parser::num(n),
        _ => Value::String(s.to_string()),
    }
}

fn parse_pairs(s: &str) -> Map<String, Value> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (url_decode(k), field_value(&url_decode(v)))
        })
        .collect()
}

fn read_request(stream: &TcpStream) -> Result<Request, String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().ok_or("empty request")?.to_uppercase();
    let target = parts.next().ok_or("missing path")?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let (mut length, mut json) = (0usize, false);
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).map_err(|e| e.to_string())? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = v.trim().parse().unwrap_or(0),
                "content-type" => json = v.contains("json"),
                _ => {}
            }
        }
    }
    if length > MAX_BODY {
        return Err("body too large".into());
    }
    let mut raw = vec![0u8; length];
    reader.read_exact(&mut raw).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&raw);
    let body = if text.trim().is_empty() {
        Value::Null
    } else if json {
        serde_json::from_str(&text).map_err(|e| format!("invalid JSON body: {e}"))?
    } else {
        Value::Object(parse_pairs(&text))
    };
    Ok(Request {
        method,
        path: url_decode(path),
        query: parse_pairs(query),
        body,
    })
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn send(mut stream: &TcpStream, code: u16, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        reason(code),
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn send_value(stream: &TcpStream, code: u16, v: &Value) {
    match v {
        Value::String(s) => send(stream, code, "text/plain; charset=utf-8", s.as_bytes()),
        other => send(
            stream,
            code,
            "application/json",
            other.to_string().as_bytes(),
        ),
    }
}

fn error(stream: &TcpStream, code: u16, msg: &str) {
    send_value(stream, code, &serde_json::json!({"error": msg}));
}

// ───────────────────────────── server ─────────────────────────────

pub struct Server {
    rt: Runtime,
    routes: Vec<Route>,
    pages: Vec<Page>,
    inbox: Arc<Approvals>,
}

impl Server {
    pub fn new(mut rt: Runtime, routes: Vec<Route>, pages: Vec<Page>) -> Self {
        let inbox = Arc::new(Approvals::default());
        rt.inbox = Some(inbox.clone());
        Server {
            rt,
            routes,
            pages,
            inbox,
        }
    }

    pub fn listen(self, addr: &str) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr)?;
        println!("\n🌐 Lugh serving '{}' on http://{addr}", self.rt.agent);
        for r in &self.routes {
            println!("   {:<6} {}", r.method, r.path);
        }
        for p in &self.pages {
            println!("   PAGE   {}  ({})", p.path, p.title);
        }
        println!("   INBOX  /_lugh/inbox");
        let server = Arc::new(self);
        for conn in listener.incoming() {
            let Ok(stream) = conn else { continue };
            let server = server.clone();
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                server.handle(&stream);
            });
        }
        Ok(())
    }

    fn handle(&self, stream: &TcpStream) {
        let req = match read_request(stream) {
            Ok(r) => r,
            Err(e) => return error(stream, 400, &e),
        };
        let segs = split_path(&req.path);

        // Built-in endpoints.
        match (
            req.method.as_str(),
            segs.iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
        ) {
            ("GET", ["_lugh", "inbox"]) => {
                let page = Page {
                    path: "/_lugh/inbox".into(),
                    title: "Approvals inbox".into(),
                    elements: vec![Element::Approvals, Element::Audit],
                };
                return send(
                    stream,
                    200,
                    "text/html; charset=utf-8",
                    render(&page, &self.rt.agent, &self.pages).as_bytes(),
                );
            }
            ("GET", ["_lugh", "api", "approvals"]) => {
                return send_value(stream, 200, &self.inbox.list())
            }
            ("POST", ["_lugh", "api", "approvals", id]) => {
                let approve = req
                    .body
                    .get("approve")
                    .map(crate::info::truthy)
                    .unwrap_or(false);
                return match id
                    .parse::<usize>()
                    .ok()
                    .map(|id| self.inbox.decide(id, approve))
                {
                    Some(true) => send_value(stream, 200, &serde_json::json!({"ok": true})),
                    _ => error(stream, 404, "no such pending approval"),
                };
            }
            ("GET", ["_lugh", "api", "audit"]) => return send_value(stream, 200, &tail_audit(30)),
            ("GET", ["favicon.ico"]) => return send(stream, 204, "image/x-icon", b""),
            _ => {}
        }

        if req.method == "GET" {
            if let Some(page) = self.pages.iter().find(|p| split_path(&p.path) == segs) {
                return send(
                    stream,
                    200,
                    "text/html; charset=utf-8",
                    render(page, &self.rt.agent, &self.pages).as_bytes(),
                );
            }
        }

        let mut params = Map::new();
        let route = self.routes.iter().find(|r| {
            params.clear();
            r.method == req.method
                && r.segments.len() == segs.len()
                && r.segments
                    .iter()
                    .zip(&segs)
                    .all(|(pat, got)| match pat.strip_prefix(':') {
                        Some(name) => {
                            params.insert(name.into(), field_value(got));
                            true
                        }
                        None => pat == got,
                    })
        });
        let Some(route) = route else {
            return error(
                stream,
                404,
                &format!("no route for {} {}", req.method, req.path),
            );
        };

        let rt = self.rt.fork(); // fresh budget per request; shared memory, stats, audit, inbox
        let mut env = Env::default();
        env.insert(
            "request".into(),
            serde_json::json!({"method": req.method, "path": req.path, "params": params, "query": req.query, "body": req.body}),
        );
        rt.audit(
            "request",
            serde_json::json!({"method": req.method, "path": req.path}),
        );
        let (code, body) = match rt.run_block(&route.body, &mut env) {
            Ok(v) => (200, v),
            Err(Flow::Respond(code, v)) => (code, v),
            Err(Flow::Stop(m)) => (422, serde_json::json!({"stopped": m})),
            Err(Flow::Denied(m)) => (403, serde_json::json!({"error": m})),
            Err(Flow::Budget(m)) => (429, serde_json::json!({"error": m})),
            Err(Flow::Fail(m)) => (500, serde_json::json!({"error": m})),
        };
        rt.audit(
            "response",
            serde_json::json!({"path": req.path, "status": code}),
        );
        rt.save();
        send_value(stream, code, &body);
    }
}

fn tail_audit(n: usize) -> Value {
    let text = std::fs::read_to_string("lugh_audit.jsonl").unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    Value::Array(
        lines[lines.len().saturating_sub(n)..]
            .iter()
            .rev()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect(),
    )
}

// ───────────────────────────── frontend renderer ─────────────────────────────

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render(page: &Page, app: &str, pages: &[Page]) -> String {
    let mut body = String::new();
    for el in &page.elements {
        body.push_str(&match el {
            Element::Heading(t) => format!("<h2>{}</h2>", esc(t)),
            Element::Text(t) => format!("<p>{}</p>", esc(t)),
            Element::Link { label, to } => format!("<p><a href=\"{}\">{}</a></p>", esc(to), esc(label)),
            Element::Table { from, columns } => format!(
                "<section class=\"card\"><div data-lugh=\"table\" data-from=\"{}\" data-columns=\"{}\"><p class=\"muted\">Loading…</p></div></section>",
                esc(from),
                esc(&columns.join(","))
            ),
            Element::List { from, show } => format!(
                "<section class=\"card\"><ul data-lugh=\"list\" data-from=\"{}\" data-show=\"{}\"><li class=\"muted\">Loading…</li></ul></section>",
                esc(from),
                esc(show.as_deref().unwrap_or(""))
            ),
            Element::Stat { from, field, label } => format!(
                "<section class=\"card stat\"><div class=\"stat-value\" data-lugh=\"stat\" data-from=\"{}\" data-field=\"{}\">…</div><div class=\"muted\">{}</div></section>",
                esc(from),
                esc(field.as_deref().unwrap_or("")),
                esc(label)
            ),
            Element::Form { to, fields, submit } => {
                let inputs: String = fields
                    .iter()
                    .map(|f| format!("<label>{}<input name=\"{}\" autocomplete=\"off\"></label>", esc(&title_case(f)), esc(f)))
                    .collect();
                format!(
                    "<section class=\"card\"><form data-lugh=\"form\" data-to=\"{}\">{inputs}<button type=\"submit\">{}</button><output></output></form></section>",
                    esc(to),
                    esc(submit)
                )
            }
            Element::Button { label, calls } => format!(
                "<section class=\"card\"><button data-lugh=\"button\" data-calls=\"{}\">{}</button><output></output></section>",
                esc(calls),
                esc(label)
            ),
            Element::Approvals => "<section class=\"card\"><h3>Waiting for your approval</h3><div data-lugh=\"approvals\"><p class=\"muted\">Loading…</p></div></section>".into(),
            Element::Audit => "<section class=\"card\"><h3>Recent activity</h3><div data-lugh=\"audit\"><p class=\"muted\">Loading…</p></div></section>".into(),
        });
    }
    let nav: String = pages
        .iter()
        .map(|p| {
            format!(
                "<a href=\"{}\"{}>{}</a>",
                esc(&p.path),
                if p.path == page.path {
                    " aria-current=\"page\""
                } else {
                    ""
                },
                esc(&p.title)
            )
        })
        .chain(std::iter::once(format!(
            "<a href=\"/_lugh/inbox\"{}>Inbox <span data-lugh=\"badge\"></span></a>",
            if page.path == "/_lugh/inbox" {
                " aria-current=\"page\""
            } else {
                ""
            }
        )))
        .collect();
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{title} · {app}</title><style>{CSS}</style></head><body><header><strong>{app}</strong><nav>{nav}</nav></header><main><h1>{title}</h1>{body}</main><script>{JS}</script></body></html>",
        title = esc(&page.title),
        app = esc(app),
    )
}

fn title_case(s: &str) -> String {
    let mut c = s.replace('_', " ");
    if let Some(f) = c.get_mut(0..1) {
        f.make_ascii_uppercase();
    }
    c
}

const CSS: &str = r#"
:root{--bg:#f7f7f5;--card:#fff;--ink:#1d1d1b;--muted:#6b6b66;--line:#e4e3de;--accent:#2f5d50;--ok:#2f7a4a;--bad:#a33a2c}
@media (prefers-color-scheme:dark){:root{--bg:#141413;--card:#1e1e1c;--ink:#ecebe6;--muted:#9a9993;--line:#2e2e2b;--accent:#7fb8a6;--ok:#6cc08b;--bad:#e0806f}}
*{box-sizing:border-box}body{margin:0;font:15px/1.5 system-ui,-apple-system,Segoe UI,sans-serif;background:var(--bg);color:var(--ink)}
header{display:flex;gap:24px;align-items:center;padding:14px 24px;border-bottom:1px solid var(--line);background:var(--card);flex-wrap:wrap}
nav{display:flex;gap:16px}nav a{color:var(--muted);text-decoration:none}nav a[aria-current]{color:var(--ink);font-weight:600}
main{max-width:880px;margin:0 auto;padding:24px}h1{font-size:24px;margin:8px 0 20px}h2{font-size:18px;margin:24px 0 8px}h3{font-size:15px;margin:0 0 12px}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:16px;margin:0 0 16px;overflow-x:auto}
table{border-collapse:collapse;width:100%}th,td{text-align:left;padding:8px 10px;border-bottom:1px solid var(--line)}th{color:var(--muted);font-weight:500;font-size:13px}
.muted{color:var(--muted)}.stat-value{font-size:28px;font-weight:600}
form{display:grid;gap:12px;max-width:420px}label{display:grid;gap:4px;font-size:13px;color:var(--muted)}
input{font:inherit;padding:8px 10px;border:1px solid var(--line);border-radius:6px;background:var(--bg);color:var(--ink)}
button{font:inherit;padding:8px 14px;border-radius:6px;border:1px solid var(--accent);background:var(--accent);color:var(--card);cursor:pointer;justify-self:start}
button.secondary{background:none;color:var(--ink);border-color:var(--line)}
output{display:block;margin-top:8px;font-size:13px;white-space:pre-wrap}output.ok{color:var(--ok)}output.bad{color:var(--bad)}
.approval{display:flex;justify-content:space-between;gap:12px;align-items:center;padding:10px 0;border-bottom:1px solid var(--line)}.approval div{display:flex;gap:8px}
[data-lugh=badge]:not(:empty){background:var(--bad);color:#fff;border-radius:9px;padding:0 7px;font-size:12px}
code{font-size:12px}
"#;

const JS: &str = r#"
const $=(s,r=document)=>[...r.querySelectorAll(s)];
const show=v=>v==null?"":typeof v=="object"?JSON.stringify(v):String(v);
const label=c=>c.replace(/_/g," ").replace(/^./,x=>x.toUpperCase());
const esc=s=>show(s).replace(/[&<>"]/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));
async function api(url,body){const r=await fetch(url,body===undefined?{}:{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(body)});
 const t=await r.text();let d;try{d=JSON.parse(t)}catch{d=t}return{ok:r.ok,status:r.status,data:d}}
function rows(d){return Array.isArray(d)?d:d&&typeof d=="object"?[d]:[]}
async function table(el){const{data}=await api(el.dataset.from);const rs=rows(data);
 const cols=el.dataset.columns?el.dataset.columns.split(","):Object.keys(rs[0]||{});
 el.innerHTML=rs.length?`<table><thead><tr>${cols.map(c=>`<th>${esc(label(c))}</th>`).join("")}</tr></thead><tbody>${rs.map(r=>`<tr>${cols.map(c=>`<td>${esc(r[c])}</td>`).join("")}</tr>`).join("")}</tbody></table>`:'<p class="muted">Nothing yet.</p>'}
async function list(el){const{data}=await api(el.dataset.from);const f=el.dataset.show;
 el.innerHTML=rows(data).map(r=>`<li>${esc(f?r[f]:r)}</li>`).join("")||'<li class="muted">Nothing yet.</li>'}
async function stat(el){const{data}=await api(el.dataset.from);const f=el.dataset.field;el.textContent=show(f&&data?data[f]:data)}
function result(out,res){out.className=res.ok?"ok":"bad";out.textContent=res.status+" · "+show(res.data)}
function form(el){el.addEventListener("submit",async e=>{e.preventDefault();const out=el.querySelector("output");const b={};
 new FormData(el).forEach((v,k)=>{b[k]=v!==""&&!isNaN(v)?Number(v):v});out.className="";out.textContent="Working… (may be waiting for approval in the inbox)";
 result(out,await api(el.dataset.to,b));refresh()})}
function button(el){el.addEventListener("click",async()=>{const out=el.nextElementSibling;out.textContent="Working…";result(out,await api(el.dataset.calls,{}));refresh()})}
async function approvals(el){const{data}=await api("/_lugh/api/approvals");
 el.innerHTML=data.length?data.map(a=>`<div class="approval"><span>${esc(a.question)}</span><div><button data-id="${a.id}" data-ok="1">Approve</button><button class="secondary" data-id="${a.id}" data-ok="0">Reject</button></div></div>`).join(""):'<p class="muted">Nothing is waiting.</p>';
 $("button",el).forEach(b=>b.onclick=async()=>{await api(`/_lugh/api/approvals/${b.dataset.id}`,{approve:b.dataset.ok=="1"});refresh()})}
async function audit(el){const{data}=await api("/_lugh/api/audit");
 el.innerHTML=`<table><tbody>${data.slice(0,15).map(e=>`<tr><td class="muted">${new Date(e.t*1000).toLocaleTimeString()}</td><td>${esc(e.kind)}</td><td><code>${esc(e.action||e.question||e.path||e.goal||e.value||"")}</code></td></tr>`).join("")}</tbody></table>`}
async function badge(){const{data}=await api("/_lugh/api/approvals");$("[data-lugh=badge]").forEach(b=>b.textContent=data.length||"")}
function refresh(){$("[data-lugh=table]").forEach(table);$("[data-lugh=list]").forEach(list);$("[data-lugh=stat]").forEach(stat);$("[data-lugh=approvals]").forEach(approvals);$("[data-lugh=audit]").forEach(audit);badge()}
$("[data-lugh=form]").forEach(form);$("[data-lugh=button]").forEach(button);refresh();setInterval(()=>{$("[data-lugh=approvals]").forEach(approvals);badge()},2000);
"#;
