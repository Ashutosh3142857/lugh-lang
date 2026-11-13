//! Lugh Core — native runtime for the Lugh agent programming language.
//!
//! Usage:
//!   lugh program.lugh            run, asking before effects that need approval
//!   lugh program.lugh --yes      run with auto-approval
//!   lugh program.lugh --check    list every action the program can take and its policy
//!   lugh program.lugh --eval     run and print a simplicity-weighted score
//!   lugh program.lugh --quiet    print only results and errors
//!   lugh program.lugh --serve 8080   serve its routes, pages, scheduled jobs and inbox
//!   lugh program.lugh --test     run its `test` blocks
//!   lugh plan "<goal>" [--use fs,db,mcp] [--out plan.lugh] [--run] [--yes]
//!                                an LLM writes the program; you review it before it runs

mod adapters;
mod info;
mod mcp;
mod parser;
mod plan;
mod runtime;
mod stdlib;
mod web;

use parser::{build_tree, calls_in_block, parse_block, parse_expr, walk, Block, Line};
use runtime::{Flow, Runtime};
use std::process::ExitCode;

const EFFECTS: [&str; 7] = [
    "read", "write", "send", "spend", "delete", "submit", "execute",
];

struct Goal {
    text: String,
    body: Block,
    source: String,
}

struct Program {
    goals: Vec<Goal>,
    routes: Vec<web::Route>,
    pages: Vec<web::Page>,
    tests: Vec<Goal>,
    jobs: Vec<web::Job>,
    auth_env: Option<String>,
}

fn quoted(s: &str) -> Option<String> {
    match parse_expr(s.trim(), &[]) {
        Ok(parser::Expr::Str(s)) => Some(s),
        _ => None,
    }
}

fn block_source(lines: &[Line]) -> String {
    let mut src = Vec::new();
    walk(lines, &mut src);
    src.join("\n")
}

/// Compile-time check: every standard-library function a program calls must exist.
fn check_functions(body: &Block) -> Result<(), String> {
    let mut calls = Vec::new();
    calls_in_block(body, &mut calls);
    for (no, adapter, name) in calls {
        if adapter.is_empty() && !stdlib::exists(&name) {
            return Err(format!(
                "line {no}: unknown function '{name}' (see docs/STDLIB.md)"
            ));
        }
    }
    Ok(())
}

fn setup(rt: &mut Runtime, top: &Line) -> Result<Program, String> {
    let mut goal_lines = Vec::new();
    let mut route_lines = Vec::new();
    let mut pages = Vec::new();
    let mut test_lines = Vec::new();
    let mut job_lines = Vec::new();
    let mut auth_env = None;
    for node in &top.children {
        let head = node.text.split_whitespace().next().unwrap_or("");
        let rest = node.text[head.len()..].trim();
        match head {
            "use" => {
                for name in rest.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    let base = name.split_whitespace().next().unwrap();
                    if !adapters::KNOWN.contains(&base) {
                        return Err(format!(
                            "line {}: adapter '{base}' is not in Lugh Core yet (available: {}); use the Python runtime for it",
                            node.no,
                            adapters::KNOWN.join(", ")
                        ));
                    }
                    rt.adapters.push(base.to_string());
                }
            }
            "budget" => {
                for part in rest.split(',').map(str::trim) {
                    let (key, val) = part
                        .split_once(' ')
                        .ok_or(format!("line {}: budget <limit> <value>", node.no))?;
                    let val = val.trim();
                    match key {
                        "time" => {
                            let digits: String = val
                                .chars()
                                .take_while(|c| c.is_ascii_digit() || *c == '.')
                                .collect();
                            let n: f64 = digits
                                .parse()
                                .map_err(|_| format!("line {}: bad time budget", node.no))?;
                            rt.time_budget = n * match val.chars().last() {
                                Some('m') => 60.0,
                                Some('h') => 3600.0,
                                _ => 1.0,
                            };
                        }
                        "calls" => {
                            rt.call_budget = val
                                .parse()
                                .map_err(|_| format!("line {}: bad call budget", node.no))?
                        }
                        _ => {} // spend / tokens: recorded, metered in a later version
                    }
                }
            }
            "policy" => {
                for rule in &node.children {
                    let t = rule.text.as_str();
                    let (bucket, list) = if let Some(r) = t.strip_prefix("allow ") {
                        (&mut rt.allow, r)
                    } else if let Some(r) = t.strip_prefix("deny ") {
                        (&mut rt.deny, r)
                    } else if let Some(r) = t.strip_prefix("ask human before ") {
                        (&mut rt.ask, r)
                    } else {
                        return Err(format!(
                            "line {}: policy rules are allow / deny / ask human before",
                            rule.no
                        ));
                    };
                    for e in list.split(',').map(str::trim) {
                        if !EFFECTS.contains(&e) {
                            return Err(format!("line {}: unknown effect '{e}'", rule.no));
                        }
                        bucket.insert(e.to_string());
                    }
                }
            }
            "goal" => goal_lines.push(node),
            "route" => route_lines.push(node),
            "page" => pages.push(web::parse_page(node)?),
            "test" => test_lines.push(node),
            "every" => job_lines.push(node),
            "auth" => {
                // auth token env "LUGH_TOKEN"
                let name = rest
                    .strip_prefix("token env ")
                    .and_then(quoted)
                    .ok_or(format!(
                        "line {}: expected: auth token env \"VARIABLE\"",
                        node.no
                    ))?;
                auth_env = Some(name);
            }
            other => {
                return Err(format!(
                    "line {}: expected use/budget/policy/goal/route/page, got '{other}'",
                    node.no
                ))
            }
        }
    }
    let mut goals = Vec::new();
    for g in goal_lines {
        let text = match parse_expr(g.text[4..].trim(), &[]) {
            Ok(parser::Expr::Str(s)) => s,
            _ => return Err(format!("line {}: goal needs a quoted description", g.no)),
        };
        let mut src = Vec::new();
        walk(&g.children, &mut src);
        goals.push(Goal {
            text,
            body: parse_block(&g.children, &rt.adapters)?,
            source: src.join("\n"),
        });
    }
    let routes = route_lines
        .into_iter()
        .map(|r| web::parse_route(r, &rt.adapters))
        .collect::<Result<Vec<_>, _>>()?;
    let mut tests = Vec::new();
    for t in test_lines {
        let text =
            quoted(&t.text[4..]).ok_or(format!("line {}: test needs a quoted name", t.no))?;
        tests.push(Goal {
            text,
            body: parse_block(&t.children, &rt.adapters)?,
            source: block_source(&t.children),
        });
    }
    let jobs = job_lines
        .into_iter()
        .map(|j| web::parse_job(j, &rt.adapters))
        .collect::<Result<Vec<_>, _>>()?;
    let bodies = goals
        .iter()
        .chain(&tests)
        .map(|g| &g.body)
        .chain(routes.iter().map(|r| &r.body))
        .chain(jobs.iter().map(|j| &j.body));
    for body in bodies {
        check_functions(body)?;
    }
    if goals.is_empty()
        && routes.is_empty()
        && pages.is_empty()
        && tests.is_empty()
        && jobs.is_empty()
    {
        return Err("nothing to run: add a goal, route, page, job or test".into());
    }
    Ok(Program {
        goals,
        routes,
        pages,
        tests,
        jobs,
        auth_env,
    })
}

/// Compile without running: parse, load the header, check function names.
fn compile_check(src: &str) -> Result<(), String> {
    let tree = build_tree(src);
    let top = tree
        .first()
        .filter(|l| l.text.starts_with("agent "))
        .ok_or("a Lugh program must start with: agent <Name>")?;
    let mut rt = Runtime::new(true).map_err(|e| e.to_string())?;
    setup(&mut rt, top).map(|_| ())
}

fn plan_command(args: &[String]) -> ExitCode {
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| {
        args.iter()
            .position(|a| a == f)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let Some(goal) = args.iter().skip(2).find(|a| {
        !a.starts_with("--")
            && Some(*a) != value("--use").as_ref()
            && Some(*a) != value("--out").as_ref()
    }) else {
        eprintln!(
            "usage: lugh plan \"<goal>\" [--use fs,http,db,mcp] [--out plan.lugh] [--run] [--yes]"
        );
        return ExitCode::from(1);
    };
    let adapters: Vec<String> = value("--use")
        .map(|u| {
            u.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if let Some(bad) = adapters
        .iter()
        .find(|a| !adapters::KNOWN.contains(&a.as_str()))
    {
        eprintln!(
            "❌ unknown adapter '{bad}' (available: {})",
            adapters::KNOWN.join(", ")
        );
        return ExitCode::from(1);
    }
    let out = value("--out").unwrap_or_else(|| "plan.lugh".into());
    println!("\n🧭 Planning: {goal}");
    let program = match plan::generate(goal, &adapters, 3, compile_check) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = std::fs::write(&out, format!("{program}\n")) {
        eprintln!("❌ cannot write {out}: {e}");
        return ExitCode::from(1);
    }
    println!("\n📄 {out}\n");
    for line in program.lines() {
        println!("   {line}");
    }
    let me = std::env::current_exe().expect("own path");
    println!("\n🔎 What it can do:");
    let _ = std::process::Command::new(&me)
        .args([out.as_str(), "--check"])
        .status();
    if !flag("--run") {
        println!("\nReview {out}, then run it with: lugh {out}");
        return ExitCode::SUCCESS;
    }
    println!("\n🛡  Plan safety: a generated program may only read on its own; everything else asks you, and delete is denied.");
    if !flag("--yes") {
        print!("   Run this program now? [y/N] ");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let mut ans = String::new();
        let _ = std::io::stdin().read_line(&mut ans);
        if !matches!(ans.trim().to_lowercase().as_str(), "y" | "yes") {
            println!("   Not run. The program is saved in {out}.");
            return ExitCode::SUCCESS;
        }
    }
    let mut run = std::process::Command::new(&me);
    run.args([out.as_str(), "--plan-safe"]);
    if flag("--yes") {
        run.arg("--yes");
    }
    match run.status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        _ => ExitCode::from(1),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("plan") {
        return plan_command(&args);
    }
    let Some(path) = args.iter().skip(1).find(|a| !a.starts_with("--")) else {
        eprintln!("usage: lugh <program.lugh> [--yes] [--check] [--eval] [--quiet]");
        return ExitCode::from(1);
    };
    let flag = |f: &str| args.iter().any(|a| a == f);
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ cannot read {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let mut rt = match Runtime::new(flag("--yes")) {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("❌ cannot open audit log: {e}");
            return ExitCode::from(1);
        }
    };
    rt.quiet = flag("--quiet");

    let tree = build_tree(&src);
    let Some(top) = tree.first().filter(|l| l.text.starts_with("agent ")) else {
        eprintln!("❌ a Lugh program must start with: agent <Name>");
        return ExitCode::from(2);
    };
    rt.agent = top.text[6..].trim().to_string();
    let program = match setup(&mut rt, top) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ {e}");
            return ExitCode::from(2);
        }
    };
    if flag("--plan-safe") {
        // Generated programs: whatever policy the model wrote, only reads run unasked.
        rt.allow.retain(|e| e == "read");
        rt.deny.insert("delete".into());
    }

    let mut allow: Vec<_> = rt.allow.iter().cloned().collect();
    allow.sort();
    let mut deny: Vec<_> = rt.deny.iter().cloned().collect();
    deny.sort();
    println!(
        "\n⚡ Lugh Core agent '{}'  adapters={:?}  budget=({}s, {} calls)  allow={:?}  deny={:?}",
        rt.agent, rt.adapters, rt.time_budget, rt.call_budget, allow, deny
    );

    let goals = &program.goals;
    if flag("--check") {
        let bodies = goals
            .iter()
            .map(|g| &g.body)
            .chain(program.routes.iter().map(|r| &r.body));
        for body in bodies {
            let mut calls = Vec::new();
            calls_in_block(body, &mut calls);
            for (no, a, m) in calls.into_iter().filter(|c| !c.1.is_empty()) {
                let eff = adapters::effect_of(&a, &m).unwrap_or("unknown");
                if eff == "dynamic" {
                    println!("  line {no:>3}: {:<14} effect=per tool  → decided from the tool's MCP annotations at call time", format!("{a}.{m}"));
                    continue;
                }
                let rule = if rt.deny.contains(eff) {
                    "DENIED"
                } else if rt.allow.contains(eff) {
                    "allowed"
                } else {
                    "needs approval"
                };
                println!(
                    "  line {no:>3}: {:<14} effect={eff:<8} → {rule}",
                    format!("{a}.{m}")
                );
            }
        }
        println!("✅ static check passed");
        return ExitCode::SUCCESS;
    }

    if flag("--test") {
        rt.auto_yes = true;
        rt.quiet = !flag("--verbose");
        let (mut passed, mut failed) = (0, 0);
        println!();
        for t in &program.tests {
            let mut env = runtime::Env::default();
            let started = std::time::Instant::now();
            match rt.run_block(&t.body, &mut env) {
                Ok(_) | Err(Flow::Respond(..)) => {
                    passed += 1;
                    println!(
                        "  ✓ {}  ({:.1} ms)",
                        t.text,
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                }
                Err(Flow::Fail(m)) | Err(Flow::Stop(m)) | Err(Flow::Denied(m))
                | Err(Flow::Budget(m)) => {
                    failed += 1;
                    println!("  ✗ {}\n      {m}", t.text);
                }
            }
        }
        println!("\n{passed} passed, {failed} failed");
        return if failed == 0 && passed > 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }

    if let Some(i) = args.iter().position(|a| a == "--serve") {
        let port = args
            .get(i + 1)
            .filter(|p| !p.starts_with("--"))
            .map(String::as_str)
            .unwrap_or("8080");
        let host = if flag("--public") {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        };
        let token = match &program.auth_env {
            Some(var) => match stdlib::call("env", &[serde_json::Value::String(var.clone())]) {
                Ok(serde_json::Value::String(t)) if t.len() >= 16 => Some(t),
                _ => {
                    eprintln!("❌ auth is on but {var} is not set to a token of at least 16 characters; refusing to start");
                    return ExitCode::from(1);
                }
            },
            None => None,
        };
        let server = web::Server::new(rt, program.routes, program.pages, program.jobs, token);
        if let Err(e) = server.listen(&format!("{host}:{port}")) {
            eprintln!("❌ cannot listen on {host}:{port}: {e}");
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }

    rt.start_clock();
    let mut results = Vec::new();
    for g in goals {
        println!("\n🎯 goal: {}", g.text);
        rt.audit("goal_start", serde_json::json!({"goal": g.text}));
        let mut env = runtime::Env::default();
        let outcome = match rt.run_block(&g.body, &mut env) {
            Ok(_) => {
                rt.audit("goal_done", serde_json::json!({"goal": g.text}));
                println!("✅ goal complete");
                1.0
            }
            Err(Flow::Stop(m)) => {
                rt.audit(
                    "goal_stopped",
                    serde_json::json!({"goal": g.text, "reason": m}),
                );
                println!("⏹  stopped: {m}");
                0.5
            }
            Err(Flow::Respond(_, _)) => {
                rt.audit("goal_done", serde_json::json!({"goal": g.text}));
                println!("✅ goal complete");
                1.0
            }
            Err(Flow::Fail(m)) | Err(Flow::Denied(m)) | Err(Flow::Budget(m)) => {
                rt.audit(
                    "goal_failed",
                    serde_json::json!({"goal": g.text, "error": m}),
                );
                println!("❌ failed: {m}");
                0.0
            }
        };
        results.push((g.text.clone(), outcome, info::k(&g.source)));
    }
    rt.save();
    println!("\n📜 audit log → lugh_audit.jsonl   🧠 memory → lugh_memory.json   📈 route stats → lugh_stats.json");

    if flag("--eval") {
        // Simplicity-weighted success, after Legg & Hutter: weight each goal by 2^-K(goal).
        let w: Vec<(String, f64, f64)> = results
            .into_iter()
            .map(|(g, v, k)| (g, v, 2f64.powf(-(k as f64) / 256.0)))
            .collect();
        let score =
            w.iter().map(|(_, v, wt)| v * wt).sum::<f64>() / w.iter().map(|x| x.2).sum::<f64>();
        println!("\n📊 evaluation");
        for (g, v, wt) in &w {
            println!("  {v:.1}  weight {wt:.3}  {g}");
        }
        println!("  weighted score: {score:.3}  (1.0 = every goal done)");
    }
    ExitCode::SUCCESS
}
