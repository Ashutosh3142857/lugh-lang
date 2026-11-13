//! Runtime: executes parsed programs with policy, budgets, memory, learning and audit.

use crate::adapters;
use crate::info::{k, ncd, relevance, sentences, truthy, txt, val_eq};
use crate::parser::{num, Block, Expr, Fallback, Rhs, Stmt};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// FxHash: a fast, non-cryptographic hasher. Variable names are program-controlled,
/// so hash-flooding resistance is unnecessary here.
#[derive(Default, Clone, Copy)]
pub struct Fx(u64);
impl std::hash::Hasher for Fx {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut w = [0u8; 8];
            w[..chunk.len()].copy_from_slice(chunk);
            self.0 = (self.0.rotate_left(5) ^ u64::from_le_bytes(w))
                .wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }
}
pub type Env = HashMap<String, Value, std::hash::BuildHasherDefault<Fx>>;

fn compare(op: &str, x: &Value, y: &Value) -> bool {
    use std::cmp::Ordering::*;
    if op == "==" {
        return val_eq(x, y);
    }
    if op == "!=" {
        return !val_eq(x, y);
    }
    let ord = match (x.as_f64(), y.as_f64()) {
        (Some(p), Some(q)) => p.partial_cmp(&q),
        _ => Some(txt(x).cmp(&txt(y))),
    };
    match op {
        "<" => ord == Some(Less),
        ">" => ord == Some(Greater),
        "<=" => matches!(ord, Some(Less | Equal)),
        _ => matches!(ord, Some(Greater | Equal)),
    }
}

#[derive(Debug)]
pub enum Flow {
    Stop(String),
    Fail(String),
    Denied(String),
    Budget(String),
    /// `respond` inside a route: HTTP status and body.
    Respond(u16, Value),
}

/// Human approvals waiting in the web inbox (server mode).
#[derive(Default)]
pub struct Approvals {
    pub pending: Mutex<Vec<Pending>>,
    pub changed: Condvar,
    next_id: AtomicUsize,
}

#[derive(Clone)]
pub struct Pending {
    pub id: usize,
    pub question: String,
    pub created: f64,
    pub decision: Option<bool>,
}

impl Approvals {
    pub fn list(&self) -> Value {
        let p = self.pending.lock().unwrap();
        Value::Array(
            p.iter()
                .filter(|x| x.decision.is_none())
                .map(|x| serde_json::json!({"id": x.id, "question": x.question, "created": x.created}))
                .collect(),
        )
    }
    pub fn decide(&self, id: usize, approve: bool) -> bool {
        let mut p = self.pending.lock().unwrap();
        match p.iter_mut().find(|x| x.id == id && x.decision.is_none()) {
            Some(x) => {
                x.decision = Some(approve);
                self.changed.notify_all();
                true
            }
            None => false,
        }
    }
    /// Blocks until a person decides in the inbox, or the wait times out (counts as a no).
    fn wait_for(&self, question: &str, timeout: std::time::Duration) -> bool {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let mut p = self.pending.lock().unwrap();
        p.push(Pending {
            id,
            question: question.into(),
            created: now(),
            decision: None,
        });
        self.changed.notify_all();
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(d) = p.iter().find(|x| x.id == id).and_then(|x| x.decision) {
                p.retain(|x| x.id != id);
                return d;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                p.retain(|x| x.id != id);
                return false;
            }
            p = self.changed.wait_timeout(p, left).unwrap().0;
        }
    }
}
type R<T> = Result<T, Flow>;

fn fail<T>(m: impl Into<String>) -> R<T> {
    Err(Flow::Fail(m.into()))
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub struct Runtime {
    pub auto_yes: bool,
    pub agent: String,
    pub adapters: Vec<String>,
    pub allow: HashSet<String>,
    pub ask: HashSet<String>,
    pub deny: HashSet<String>,
    pub time_budget: f64,
    pub call_budget: usize,
    pub quiet: bool,
    calls: AtomicUsize,
    started: Instant,
    memory: Arc<Mutex<Map<String, Value>>>,
    stats: Arc<Mutex<Map<String, Value>>>,
    audit: Arc<Mutex<File>>,
    human: Arc<Mutex<()>>,
    /// Set in server mode: approvals go to the web inbox instead of the terminal.
    pub inbox: Option<Arc<Approvals>>,
}

const MEMORY: &str = "lugh_memory.json";
const STATS: &str = "lugh_stats.json";

fn load(path: &str) -> Map<String, Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

impl Runtime {
    pub fn new(auto_yes: bool) -> std::io::Result<Self> {
        let mut memory = load(MEMORY);
        // LSTM-style forget gate: expired entries are dropped on load.
        let t = now();
        if let Some(Value::Object(exp)) = memory.get("__expires__").cloned() {
            let mut keep = Map::new();
            for (key, until) in exp {
                if until.as_f64().unwrap_or(0.0) < t {
                    memory.remove(&key);
                } else {
                    keep.insert(key, until);
                }
            }
            memory.insert("__expires__".into(), Value::Object(keep));
        }
        Ok(Runtime {
            auto_yes,
            agent: "anonymous".into(),
            adapters: vec![],
            allow: HashSet::new(),
            ask: HashSet::new(),
            deny: HashSet::new(),
            time_budget: 120.0,
            call_budget: 25,
            quiet: false,
            calls: AtomicUsize::new(0),
            started: Instant::now(),
            memory: Arc::new(Mutex::new(memory)),
            stats: Arc::new(Mutex::new(load(STATS))),
            audit: Arc::new(Mutex::new(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("lugh_audit.jsonl")?,
            )),
            human: Arc::new(Mutex::new(())),
            inbox: None,
        })
    }

    /// A per-request runtime: same policy, memory, stats, audit and inbox; fresh budget.
    pub fn fork(&self) -> Runtime {
        Runtime {
            auto_yes: self.auto_yes,
            agent: self.agent.clone(),
            adapters: self.adapters.clone(),
            allow: self.allow.clone(),
            ask: self.ask.clone(),
            deny: self.deny.clone(),
            time_budget: self.time_budget,
            call_budget: self.call_budget,
            quiet: self.quiet,
            calls: AtomicUsize::new(0),
            started: Instant::now(),
            memory: self.memory.clone(),
            stats: self.stats.clone(),
            audit: self.audit.clone(),
            human: self.human.clone(),
            inbox: self.inbox.clone(),
        }
    }

    pub fn start_clock(&mut self) {
        self.started = Instant::now();
    }

    pub fn save(&self) {
        let _ = std::fs::write(
            MEMORY,
            serde_json::to_string_pretty(&*self.memory.lock().unwrap()).unwrap_or_default(),
        );
        let _ = std::fs::write(
            STATS,
            serde_json::to_string_pretty(&*self.stats.lock().unwrap()).unwrap_or_default(),
        );
    }

    pub fn audit(&self, kind: &str, fields: Value) {
        let mut rec = Map::new();
        rec.insert("t".into(), num((now() * 1000.0).round() / 1000.0));
        rec.insert("agent".into(), Value::String(self.agent.clone()));
        rec.insert("kind".into(), Value::String(kind.into()));
        if let Value::Object(f) = fields {
            rec.extend(f);
        }
        let _ = writeln!(self.audit.lock().unwrap(), "{}", Value::Object(rec));
    }

    pub fn say(&self, icon: &str, msg: &str) {
        if !self.quiet {
            println!("  {icon} {msg}");
        }
    }

    // ───────── policy gate: every effect passes here ─────────

    fn gate(&self, effect: &str, desc: &str) -> R<()> {
        if self.started.elapsed().as_secs_f64() > self.time_budget {
            return Err(Flow::Budget(format!(
                "time budget of {:.0}s used up",
                self.time_budget
            )));
        }
        if self.calls.fetch_add(1, Ordering::SeqCst) + 1 > self.call_budget {
            return Err(Flow::Budget(format!(
                "call budget of {} used up",
                self.call_budget
            )));
        }
        if self.deny.contains(effect) {
            self.audit(
                "denied",
                serde_json::json!({"effect": effect, "action": desc}),
            );
            return Err(Flow::Denied(format!("policy denies '{effect}': {desc}")));
        }
        if self.allow.contains(effect) {
            return Ok(());
        }
        if !self.approves(&format!("[{effect}] {desc}")) {
            self.audit(
                "rejected",
                serde_json::json!({"effect": effect, "action": desc}),
            );
            return Err(Flow::Denied(format!("human rejected: {desc}")));
        }
        Ok(())
    }

    fn approves(&self, q: &str) -> bool {
        if let (Some(inbox), false) = (&self.inbox, self.auto_yes) {
            self.say("👤", &format!("waiting for approval in the inbox → {q}"));
            let ok = inbox.wait_for(q, std::time::Duration::from_secs(300));
            self.audit("human", serde_json::json!({"question": q, "answer": if ok { "yes" } else { "no" }, "via": "inbox"}));
            return ok;
        }
        let _one_at_a_time = self.human.lock().unwrap();
        if self.auto_yes {
            self.say("👤", &format!("auto-approved → {q}"));
            self.audit(
                "human",
                serde_json::json!({"question": q, "answer": "yes(auto)"}),
            );
            return true;
        }
        print!("  👤 APPROVE? {q} [y/N] ");
        let _ = std::io::stdout().flush();
        let mut ans = String::new();
        let _ = std::io::stdin().lock().read_line(&mut ans);
        let ans = ans.trim().to_lowercase();
        self.audit("human", serde_json::json!({"question": q, "answer": ans}));
        ans == "y" || ans == "yes"
    }

    // ───────── statements ─────────

    pub fn run_block(&self, b: &Block, env: &mut Env) -> R<Value> {
        let mut last = Value::Null;
        for (no, s) in b {
            last = self.exec(s, env).map_err(|f| match f {
                Flow::Fail(m) if !m.starts_with("line ") => Flow::Fail(format!("line {no}: {m}")),
                other => other,
            })?;
        }
        Ok(last)
    }

    fn rhs(&self, r: &Rhs, env: &mut Env) -> R<Value> {
        match r {
            Rhs::Expr(e) => self.eval(e, env),
            Rhs::Attempt(body, alt) => {
                let mut scoped = env.clone();
                match self.run_block(body, &mut scoped) {
                    Ok(v) => {
                        self.say("✔", "attempt succeeded");
                        Ok(v)
                    }
                    Err(Flow::Fail(m)) => {
                        self.audit("attempt_failed", serde_json::json!({"error": m}));
                        match alt {
                            Some(alt) => {
                                self.say(
                                    "↪",
                                    &format!(
                                        "attempt failed ({}) → trying fallback",
                                        m.chars().take(70).collect::<String>()
                                    ),
                                );
                                self.run_block(alt, env)
                            }
                            None => fail(m),
                        }
                    }
                    Err(other) => Err(other),
                }
            }
            Rhs::AttemptAuto(routes) => self.attempt_auto(routes, env),
            Rhs::Together(branches) => self.together(branches, env),
            Rhs::Consensus(branches, need) => {
                let vals = self.together(branches, env)?;
                let vals = vals.as_array().cloned().unwrap_or_default();
                // Group identical values by their canonical text; remember where each group starts.
                let mut groups: Vec<(String, usize, usize)> = Vec::new(); // (key, votes, first index)
                for (i, v) in vals.iter().enumerate() {
                    let key = crate::info::canonical(v);
                    match groups.iter_mut().find(|g| g.0 == key) {
                        Some(g) => g.1 += 1,
                        None => groups.push((key, 1, i)),
                    }
                }
                let (votes, first) = groups
                    .iter()
                    .max_by_key(|g| g.1)
                    .map(|g| (g.1, g.2))
                    .unwrap_or((0, 0));
                let need = need.unwrap_or(vals.len() / 2 + 1);
                self.audit(
                    "consensus",
                    serde_json::json!({"sources": vals.len(), "votes": votes, "needed": need}),
                );
                if votes < need {
                    return fail(format!(
                        "no consensus: best value had {votes}/{} votes, needed {need}",
                        vals.len()
                    ));
                }
                self.say("🤝", &format!("consensus {votes}/{}", vals.len()));
                Ok(vals.into_iter().nth(first).unwrap_or(Value::Null))
            }
        }
    }

    fn exec(&self, s: &Stmt, env: &mut Env) -> R<Value> {
        match s {
            Stmt::Let(name, r) => {
                let v = self.rhs(r, env)?;
                env.insert(name.clone(), v.clone());
                Ok(v)
            }
            Stmt::Do(r) => self.rhs(r, env),
            Stmt::Refine {
                name,
                gen,
                cond,
                rounds,
            } => {
                let original = env.get(name).cloned().unwrap_or(Value::Null);
                if truthy(&self.eval(cond, env)?) {
                    return Ok(original);
                }
                for r in 1..=*rounds {
                    let v = self.eval(gen, env)?;
                    env.insert(name.clone(), v.clone());
                    if truthy(&self.eval(cond, env)?) {
                        self.audit(
                            "refine",
                            serde_json::json!({"name": name, "rounds": r, "accepted": true}),
                        );
                        self.say("♻", &format!("refined '{name}' in {r} round(s)"));
                        return Ok(v);
                    }
                }
                env.insert(name.clone(), original.clone()); // identity path: never end worse than we began
                self.audit(
                    "refine",
                    serde_json::json!({"name": name, "rounds": rounds, "accepted": false}),
                );
                self.say(
                    "♻",
                    &format!("refine of '{name}' did not pass in {rounds} rounds — kept original"),
                );
                Ok(original)
            }
            Stmt::Ensure { cond, src, fb } => {
                if truthy(&self.eval(cond, env)?) {
                    self.say("✔", &format!("ensured: {src}"));
                    return Ok(Value::Bool(true));
                }
                self.audit("ensure_failed", serde_json::json!({"condition": src}));
                match fb {
                    Fallback::Stop(m) => {
                        Err(Flow::Stop(self.interp(&txt(&self.eval(m, env)?), env)))
                    }
                    Fallback::Ask(m) => {
                        let q = self.interp(&txt(&self.eval(m, env)?), env);
                        if self.approves(&format!("ensure failed — continue anyway? {q}")) {
                            Ok(Value::Bool(false))
                        } else {
                            Err(Flow::Stop("human chose to stop".into()))
                        }
                    }
                    Fallback::Respond(status, m) => Err(Flow::Respond(*status, self.eval(m, env)?)),
                    Fallback::None => Err(Flow::Stop(format!("ensure failed: {src}"))),
                }
            }
            Stmt::Remember { key, expr, keep } => {
                let v = self.eval(expr, env)?;
                let mut mem = self.memory.lock().unwrap();
                mem.insert(key.clone(), v.clone());
                let exp = mem
                    .entry("__expires__")
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Value::Object(exp) = exp {
                    match keep {
                        Some(secs) => {
                            exp.insert(key.clone(), num(now() + secs));
                        }
                        None => {
                            exp.remove(key);
                        }
                    }
                }
                drop(mem);
                self.audit("remember", serde_json::json!({"key": key, "value": v}));
                self.say(
                    "🧠",
                    &match keep {
                        Some(s) => {
                            let s = *s as i64;
                            let span = match s {
                                s if s % 86400 == 0 => format!("{}d", s / 86400),
                                s if s % 3600 == 0 => format!("{}h", s / 3600),
                                s if s % 60 == 0 => format!("{}m", s / 60),
                                s => format!("{s}s"),
                            };
                            format!("remembered '{key}' for {span}")
                        }
                        None => format!("remembered '{key}'"),
                    },
                );
                Ok(v)
            }
            Stmt::Ask { q, name } => {
                let q = self.interp(&txt(&self.eval(q, env)?), env);
                let ok = Value::Bool(self.approves(&q));
                env.insert(name.clone(), ok.clone());
                Ok(ok)
            }
            Stmt::When {
                cond,
                body,
                otherwise,
            } => {
                if truthy(&self.eval(cond, env)?) {
                    self.run_block(body, env)
                } else if let Some(o) = otherwise {
                    self.run_block(o, env)
                } else {
                    Ok(Value::Null)
                }
            }
            Stmt::Each {
                var,
                list,
                parallel,
                body,
            } => {
                let items = match self.eval(list, env)? {
                    Value::Array(a) => a,
                    Value::Null => vec![],
                    other => return fail(format!("each needs a list, got {}", txt(&other))),
                };
                match parallel {
                    Some(n) if *n > 1 && items.len() > 1 => {
                        let n = (*n).min(items.len());
                        let results: Vec<R<(usize, Value)>> = std::thread::scope(|sc| {
                            let handles: Vec<_> = (0..n)
                                .map(|w| {
                                    let items = &items;
                                    let base = &*env;
                                    sc.spawn(move || {
                                        let mut out = Vec::new();
                                        for i in (w..items.len()).step_by(n) {
                                            let mut local = base.clone();
                                            local.insert(var.clone(), items[i].clone());
                                            out.push(
                                                self.run_block(body, &mut local).map(|v| (i, v)),
                                            );
                                        }
                                        out
                                    })
                                })
                                .collect();
                            handles
                                .into_iter()
                                .flat_map(|h| h.join().unwrap())
                                .collect()
                        });
                        let mut ordered = vec![Value::Null; items.len()];
                        for r in results {
                            let (i, v) = r?;
                            ordered[i] = v;
                        }
                        self.say(
                            "⇉",
                            &format!("processed {} items on {n} workers", items.len()),
                        );
                        Ok(Value::Array(ordered))
                    }
                    _ => {
                        let mut out = Vec::with_capacity(items.len());
                        for it in items {
                            env.insert(var.clone(), it);
                            out.push(self.run_block(body, env)?);
                        }
                        Ok(Value::Array(out))
                    }
                }
            }
            Stmt::Send {
                channel,
                target,
                msg,
            } => {
                let target = txt(&self.eval(target, env)?);
                let msg = self.interp(&txt(&self.eval(msg, env)?), env);
                self.gate(
                    "send",
                    &format!(
                        "send via {channel} to {target}: {}",
                        msg.chars().take(60).collect::<String>()
                    ),
                )?;
                match channel.as_str() {
                    "console" => println!("  📣 [{target}] {msg}"),
                    "file" => {
                        adapters::call("fs", "append", &[Value::String(target.clone()), Value::String(msg.clone())]).map_err(Flow::Fail)?;
                    }
                    _ => println!("  📣 [{channel}:{target}] {msg}   (simulated — connect an MCP server for real delivery)"),
                }
                self.audit(
                    "sent",
                    serde_json::json!({"channel": channel, "target": target, "message": msg}),
                );
                Ok(Value::String(msg))
            }
            Stmt::Log(e) => {
                let v = self.eval(e, env)?;
                let shown = match &v {
                    Value::String(s) => self.interp(s, env),
                    other => txt(other),
                };
                self.say("📝", &shown);
                self.audit("log", serde_json::json!({"value": shown}));
                Ok(v)
            }
            Stmt::Stop(e) => Err(Flow::Stop(self.interp(&txt(&self.eval(e, env)?), env))),
            Stmt::Expect { cond, src } => {
                if truthy(&self.eval(cond, env)?) {
                    Ok(Value::Bool(true))
                } else {
                    fail(format!("expected {src}"))
                }
            }
            Stmt::Respond { status, body } => {
                let v = self.eval(body, env)?;
                Err(Flow::Respond(*status, v))
            }
        }
    }

    fn together(&self, branches: &[Block], env: &Env) -> R<Value> {
        let results: Vec<R<Value>> = std::thread::scope(|sc| {
            let hs: Vec<_> = branches
                .iter()
                .map(|b| {
                    let mut local = env.clone();
                    sc.spawn(move || self.run_block(b, &mut local))
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        Ok(Value::Array(results.into_iter().collect::<R<Vec<_>>>()?))
    }

    // Adam-style bias-corrected moments: success rate (m) and squared latency (v).
    fn route_score(&self, key: &str) -> f64 {
        let stats = self.stats.lock().unwrap();
        let Some(s) = stats.get(key) else { return 0.5 };
        let t = s["t"].as_f64().unwrap_or(1.0);
        let m_hat = s["m"].as_f64().unwrap_or(0.0) / (1.0 - 0.9f64.powf(t));
        let v_hat = s["v"].as_f64().unwrap_or(0.0) / (1.0 - 0.999f64.powf(t));
        m_hat / (v_hat.sqrt() + 1.0)
    }

    fn route_update(&self, key: &str, ok: bool, secs: f64) {
        let mut stats = self.stats.lock().unwrap();
        let s = stats
            .entry(key)
            .or_insert_with(|| serde_json::json!({"m": 0.0, "v": 0.0, "t": 0}));
        let t = s["t"].as_f64().unwrap_or(0.0) + 1.0;
        let m = 0.9 * s["m"].as_f64().unwrap_or(0.0) + 0.1 * if ok { 1.0 } else { 0.0 };
        let v = 0.999 * s["v"].as_f64().unwrap_or(0.0) + 0.001 * secs * secs;
        *s = serde_json::json!({"m": m, "v": v, "t": t});
    }

    fn attempt_auto(&self, routes: &[(String, Block)], env: &mut Env) -> R<Value> {
        let mut order: Vec<&(String, Block)> = routes.iter().collect();
        order.sort_by(|a, b| {
            self.route_score(&b.0)
                .partial_cmp(&self.route_score(&a.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let first_word = |s: &str| s.split_whitespace().next().unwrap_or("").to_string();
        self.say(
            "🧭",
            &format!(
                "route order: {}",
                order
                    .iter()
                    .map(|r| first_word(&r.0))
                    .collect::<Vec<_>>()
                    .join(" > ")
            ),
        );
        let mut last = String::new();
        for (key, block) in order {
            let t0 = Instant::now();
            let mut scoped = env.clone();
            match self.run_block(block, &mut scoped) {
                Ok(v) => {
                    self.route_update(key, true, t0.elapsed().as_secs_f64());
                    self.say("✔", &format!("route succeeded: {}", first_word(key)));
                    return Ok(v);
                }
                Err(Flow::Fail(m)) => {
                    self.route_update(key, false, t0.elapsed().as_secs_f64());
                    self.audit(
                        "route_failed",
                        serde_json::json!({"route": key, "error": m}),
                    );
                    self.say("↪", &format!("route failed: {} → next", first_word(key)));
                    last = m;
                }
                Err(other) => return Err(other),
            }
        }
        fail(format!("all routes failed; last error: {last}"))
    }

    // ───────── expressions ─────────

    fn list(&self, e: &Expr, env: &Env) -> R<Vec<Value>> {
        match self.eval(e, env)? {
            Value::Array(a) => Ok(a),
            Value::Null => Ok(vec![]),
            other => fail(format!("expected a list, got {}", txt(&other))),
        }
    }

    pub fn eval(&self, e: &Expr, env: &Env) -> R<Value> {
        use Expr::*;
        Ok(match e {
            Lit(v) => v.clone(),
            Str(s) => Value::String(self.interp(s, env)),
            List(v) => Value::Array(v.iter().map(|x| self.eval(x, env)).collect::<R<_>>()?),
            Var(parts) => self.lookup_parts(parts, env)?,
            Not(a) => Value::Bool(!truthy(&self.eval(a, env)?)),
            // Like Python and JavaScript: `and`/`or` return an operand, so `n or 0` is a default.
            And(a, b) => {
                let x = self.eval(a, env)?;
                if truthy(&x) {
                    self.eval(b, env)?
                } else {
                    x
                }
            }
            Or(a, b) => {
                let x = self.eval(a, env)?;
                if truthy(&x) {
                    x
                } else {
                    self.eval(b, env)?
                }
            }
            Cmp(op, a, b) => {
                if let (Some(x), Some(y)) = (Self::peek(a, env), Self::peek(b, env)) {
                    return Ok(Value::Bool(compare(op, x, y)));
                }
                let (x, y) = (self.eval(a, env)?, self.eval(b, env)?);
                return Ok(Value::Bool(compare(op, &x, &y)));
            }
            Bin(op, a, b) => {
                let (x, y) = (self.eval(a, env)?, self.eval(b, env)?);
                match (op.as_str(), x.as_f64(), y.as_f64()) {
                    ("+", Some(p), Some(q)) => num(p + q),
                    ("-", Some(p), Some(q)) => num(p - q),
                    ("*", Some(p), Some(q)) => num(p * q),
                    ("/", Some(p), Some(q)) => {
                        if q == 0.0 {
                            return fail("division by zero");
                        }
                        num(p / q)
                    }
                    ("+", _, _) => Value::String(txt(&x) + &txt(&y)),
                    _ => {
                        return fail(format!(
                            "cannot apply '{op}' to {} and {}",
                            txt(&x),
                            txt(&y)
                        ))
                    }
                }
            }
            Pick { lowest, field, src } => {
                let items = self.list(src, env)?;
                let key = |v: &Value| v[field.as_str()].as_f64().unwrap_or(f64::NAN);
                let best = if *lowest {
                    items.into_iter().min_by(|a, b| key(a).total_cmp(&key(b)))
                } else {
                    items.into_iter().max_by(|a, b| key(a).total_cmp(&key(b)))
                };
                match best {
                    Some(v) => v,
                    None => return fail("pick from an empty list"),
                }
            }
            Count(a) => match self.eval(a, env)? {
                Value::Array(v) => Value::from(v.len()),
                Value::Object(o) => Value::from(o.len()),
                Value::String(s) => Value::from(s.chars().count()),
                Value::Null => Value::from(0),
                _ => Value::from(1),
            },
            Recall(k) => {
                let key = txt(&self.eval(k, env)?);
                self.memory
                    .lock()
                    .unwrap()
                    .get(&key)
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            RecallLike(q) => {
                let q = txt(&self.eval(q, env)?);
                let mem = self.memory.lock().unwrap();
                let best = mem
                    .iter()
                    .filter(|(k, _)| !k.starts_with("__"))
                    .map(|(k, v)| (relevance(&q, &format!("{k} {}", txt(v))), v))
                    .max_by(|a, b| a.0.total_cmp(&b.0));
                match best {
                    Some((score, v)) if score >= 0.15 => v.clone(),
                    _ => Value::Null,
                }
            }
            AskHuman(q) => {
                let q = txt(&self.eval(q, env)?);
                Value::Bool(self.approves(&q))
            }
            Think { prompt, ctx, n } => {
                let mut full = txt(&self.eval(prompt, env)?);
                if let Some(c) = ctx {
                    full = format!("{full}\n\nContext:\n{}", txt(&self.eval(c, env)?));
                }
                let n = self.eval(n, env)?.as_f64().unwrap_or(1.0).max(1.0) as usize;
                let mut outs = Vec::with_capacity(n);
                for _ in 0..n {
                    self.gate(
                        "read",
                        &format!("think: {}", full.chars().take(60).collect::<String>()),
                    )?;
                    outs.push(adapters::think(&full).map_err(Flow::Fail)?);
                }
                self.audit("think", serde_json::json!({"prompt": full, "samples": n}));
                let mut counts: HashMap<&String, usize> = HashMap::new();
                for o in &outs {
                    *counts.entry(o).or_default() += 1;
                }
                let (best, votes) = counts.into_iter().max_by_key(|c| c.1).unwrap();
                if n > 1 {
                    self.say("🗳", &format!("think: {votes}/{n} samples agree"));
                }
                Value::String(best.clone())
            }
            Attend { src, q, n } => {
                let items = self.list(src, env)?;
                let q = txt(&self.eval(q, env)?);
                let n = self.eval(n, env)?.as_f64().unwrap_or(3.0) as usize;
                let mut scored: Vec<(f64, Value)> = items
                    .into_iter()
                    .map(|it| (relevance(&q, &txt(&it)), it))
                    .collect();
                scored.sort_by(|a, b| b.0.total_cmp(&a.0));
                self.audit("attend", serde_json::json!({"query": q, "considered": scored.len(), "kept": n.min(scored.len())}));
                Value::Array(scored.into_iter().take(n).map(|s| s.1).collect())
            }
            Point { q, text } => {
                let q = txt(&self.eval(q, env)?);
                let text = txt(&self.eval(text, env)?);
                let best = sentences(&text)
                    .into_iter()
                    .map(|s| (relevance(&q, s), s))
                    .max_by(|a, b| a.0.total_cmp(&b.0));
                match best {
                    Some((score, span)) if score > 0.0 => {
                        debug_assert!(text.contains(span)); // grounded: always a verbatim span
                        self.audit("point", serde_json::json!({"query": q, "span": span}));
                        Value::String(span.to_string())
                    }
                    _ => Value::Null,
                }
            }
            Novelty { x, against } => {
                let x = txt(&self.eval(x, env)?);
                let refs: Vec<String> = match against {
                    Some(a) => self.list(a, env)?.iter().map(txt).collect(),
                    None => self
                        .memory
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|(k, _)| !k.starts_with("__"))
                        .map(|(_, v)| txt(v))
                        .collect(),
                };
                let d = refs.iter().map(|r| ncd(&x, r)).fold(1.0f64, f64::min);
                num((d * 1000.0).round() / 1000.0)
            }
            Simplest(a) => {
                let items = self.list(a, env)?;
                match items.into_iter().min_by_key(|v| k(&txt(v))) {
                    Some(v) => v,
                    None => return fail("simplest of an empty list"),
                }
            }
            Field(inner, key) => {
                let v = self.eval(inner, env)?;
                let got = match &v {
                    Value::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
                    Value::Object(o) => o.get(key),
                    _ => None,
                };
                got.cloned().unwrap_or(Value::Null)
            }
            Obj(fields) => {
                let mut o = serde_json::Map::new();
                for (k, v) in fields {
                    o.insert(k.clone(), self.eval(v, env)?);
                }
                Value::Object(o)
            }
            Fn { name, args } => {
                let vals: Vec<Value> = args.iter().map(|a| self.eval(a, env)).collect::<R<_>>()?;
                crate::stdlib::call(name, &vals).map_err(Flow::Fail)?
            }
            Filter { var, src, cond }
            | Map {
                var,
                src,
                out: cond,
            } => {
                let items = self.list(src, env)?;
                let keep = matches!(e, Filter { .. });
                let mut local = env.clone();
                local.insert(var.clone(), Value::Null);
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    *local.get_mut(var).unwrap() = item;
                    let v = self.eval(cond, &local)?;
                    if keep {
                        if truthy(&v) {
                            out.push(std::mem::take(local.get_mut(var).unwrap()));
                        }
                    } else {
                        out.push(v);
                    }
                }
                Value::Array(out)
            }
            Sort { src, field, desc } => {
                let mut items = self.list(src, env)?;
                items.sort_by(|x, y| {
                    let (p, q) = (&x[field.as_str()], &y[field.as_str()]);
                    let o = match (p.as_f64(), q.as_f64()) {
                        (Some(a), Some(b)) => a.total_cmp(&b),
                        _ => txt(p).cmp(&txt(q)),
                    };
                    if *desc {
                        o.reverse()
                    } else {
                        o
                    }
                });
                Value::Array(items)
            }
            Group { src, field } => {
                let mut groups = serde_json::Map::new();
                for item in self.list(src, env)? {
                    let key = txt(&item[field.as_str()]);
                    if let Value::Array(g) =
                        groups.entry(key).or_insert_with(|| Value::Array(vec![]))
                    {
                        g.push(item);
                    }
                }
                Value::Object(groups)
            }
            Pairs { a, xs, b, ys, cond } => {
                let (mut xs, mut ys) = (self.list(xs, env)?, self.list(ys, env)?);
                let mut local = env.clone();
                local.insert(a.clone(), Value::Null);
                local.insert(b.clone(), Value::Null);
                let mut out = Vec::new();
                for x in xs.iter_mut() {
                    *local.get_mut(a).unwrap() = std::mem::take(x);
                    for y in ys.iter_mut() {
                        // Move the item into the binding slot and back: no allocation per pair.
                        *local.get_mut(b).unwrap() = std::mem::take(y);
                        let hit = truthy(&self.eval(cond, &local)?);
                        *y = std::mem::take(local.get_mut(b).unwrap());
                        if hit {
                            let mut m = serde_json::Map::new();
                            m.insert(a.clone(), local[a].clone());
                            m.insert(b.clone(), y.clone());
                            out.push(Value::Object(m));
                        }
                    }
                    *x = std::mem::take(local.get_mut(a).unwrap());
                }
                Value::Array(out)
            }
            Call {
                adapter,
                method,
                args,
                cast,
            } => {
                let effect = adapters::effect_of(adapter, method)
                    .ok_or_else(|| Flow::Fail(format!("{adapter} has no action '{method}'")))?;
                let vals: Vec<Value> = args.iter().map(|a| self.eval(a, env)).collect::<R<_>>()?;
                let shown: Vec<String> = vals
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => s.chars().take(50).collect(),
                        other => other.to_string().chars().take(50).collect(),
                    })
                    .collect();
                let desc = format!("{adapter}.{method} {}", shown.join(" "));
                let effect = if effect == "dynamic" {
                    let (server, tool) = (
                        vals.first().map(txt).unwrap_or_default(),
                        vals.get(1).map(txt).unwrap_or_default(),
                    );
                    crate::mcp::tool_effect(&server, &tool).map_err(Flow::Fail)?
                } else {
                    effect
                };
                self.gate(effect, &desc)?;
                self.say("→", &desc);
                let t0 = Instant::now();
                match adapters::call(adapter, method, &vals) {
                    Ok(v) => {
                        self.audit("effect", serde_json::json!({"effect": effect, "action": desc, "ms": t0.elapsed().as_millis() as u64}));
                        adapters::cast(v, cast.as_deref()).map_err(Flow::Fail)?
                    }
                    Err(err) => {
                        self.audit(
                            "effect_error",
                            serde_json::json!({"effect": effect, "action": desc, "error": err}),
                        );
                        return fail(format!("{adapter}.{method} failed: {err}"));
                    }
                }
            }
        })
    }

    /// Borrow a variable path without cloning (fast path for comparisons).
    fn peek<'a>(e: &'a Expr, env: &'a Env) -> Option<&'a Value> {
        match e {
            Expr::Var(parts) => {
                let mut v = env.get(parts.first()?)?;
                for p in &parts[1..] {
                    v = match v {
                        Value::Array(a) => a.get(p.parse::<usize>().ok()?)?,
                        Value::Object(o) => o.get(p)?,
                        _ => return None,
                    };
                }
                Some(v)
            }
            Expr::Lit(v) => Some(v),
            _ => None,
        }
    }

    fn lookup(&self, path: &str, env: &Env) -> R<Value> {
        let parts: Vec<String> = path.split('.').map(String::from).collect();
        self.lookup_parts(&parts, env)
    }

    fn lookup_parts(&self, all: &[String], env: &Env) -> R<Value> {
        let head = all.first().map(String::as_str).unwrap_or("");
        let parts = all.iter().skip(1).map(String::as_str);
        let mut v = match env.get(head) {
            Some(v) => v,
            None if head == "memory" => {
                return Ok(Value::Object(self.memory.lock().unwrap().clone()))
            }
            None => return fail(format!("'{head}' is not defined")),
        };
        for p in parts {
            v = match v {
                Value::Array(a) => p.parse::<usize>().ok().and_then(|i| a.get(i)),
                Value::Object(o) => o.get(p),
                _ => None,
            }
            .ok_or_else(|| Flow::Fail(format!("'{}' has no part '{p}'", all.join("."))))?;
        }
        Ok(v.clone())
    }

    /// String interpolation, like f-strings and template literals: `{name.field}`
    /// or any pure expression, `{count(items)}`, `{round(total / n, 2)}`.
    /// Adapter calls are not allowed inside braces, so a message can never cause
    /// an effect. Anything that isn't a valid expression is left as written.
    pub fn interp(&self, s: &str, env: &Env) -> String {
        if !s.contains('{') {
            return s.to_string();
        }
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            // Find the matching '}', allowing nested braces such as {json.text({a: 1})}.
            let mut depth = 0usize;
            let close = after.char_indices().find_map(|(i, c)| match c {
                '{' => {
                    depth += 1;
                    None
                }
                '}' if depth == 0 => Some(i),
                '}' => {
                    depth -= 1;
                    None
                }
                _ => None,
            });
            let Some(close) = close.filter(|c| *c > 0) else {
                out.push('{');
                rest = after;
                continue;
            };
            let inner = &after[..close];
            let simple = inner.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.');
            let value = if simple {
                self.lookup(inner, env).ok()
            } else {
                crate::parser::parse_expr(inner, &[]).ok().and_then(|e| self.eval(&e, env).ok())
            };
            match value {
                Some(v) => out.push_str(&txt(&v)),
                None => out.push_str(&rest[open..open + close + 2]),
            }
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        out
    }
}
