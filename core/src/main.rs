//! Lugh Core — native runtime for the Lugh agent programming language.
//!
//! Usage:
//!   lugh program.lugh            run, asking before effects that need approval
//!   lugh program.lugh --yes      run with auto-approval
//!   lugh program.lugh --check    list every action the program can take and its policy
//!   lugh program.lugh --eval     run and print a simplicity-weighted score
//!   lugh program.lugh --quiet    print only results and errors

mod adapters;
mod info;
mod parser;
mod runtime;

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

fn setup(rt: &mut Runtime, top: &Line) -> Result<Vec<Goal>, String> {
    let mut goal_lines = Vec::new();
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
            other => {
                return Err(format!(
                    "line {}: expected use/budget/policy/goal, got '{other}'",
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
    if goals.is_empty() {
        return Err("no goal defined".into());
    }
    Ok(goals)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
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
    let goals = match setup(&mut rt, top) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("❌ {e}");
            return ExitCode::from(2);
        }
    };

    let mut allow: Vec<_> = rt.allow.iter().cloned().collect();
    allow.sort();
    let mut deny: Vec<_> = rt.deny.iter().cloned().collect();
    deny.sort();
    println!(
        "\n⚡ Lugh Core agent '{}'  adapters={:?}  budget=({}s, {} calls)  allow={:?}  deny={:?}",
        rt.agent, rt.adapters, rt.time_budget, rt.call_budget, allow, deny
    );

    if flag("--check") {
        for g in &goals {
            let mut calls = Vec::new();
            calls_in_block(&g.body, &mut calls);
            for (no, a, m) in calls {
                let eff = adapters::effect_of(&a, &m).unwrap_or("unknown");
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

    rt.start_clock();
    let mut results = Vec::new();
    for g in &goals {
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
