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
//!   lugh program.lugh --lint     compile only; print errors as JSON (for editors)
//!   lugh playground [port]       the web playground (adapters act on a simulated world)
//!   lugh plan "<goal>" [--use fs,db,mcp] [--out plan.lugh] [--run] [--yes]
//!                                an LLM writes the program; you review it before it runs

use lugh_core::parser::{build_tree, calls_in_block};
use lugh_core::program::{compile_check, setup};
use lugh_core::runtime::{self, Flow, Runtime};
use lugh_core::{adapters, info, plan, stdlib, web};
use std::process::ExitCode;

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
    if args.get(1).map(String::as_str) == Some("playground") {
        let port = args.get(2).map(String::as_str).unwrap_or("8000");
        let host = if args.iter().any(|a| a == "--public") {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        };
        const PAGE: &str = include_str!("../../playground/index.html");
        if let Err(e) = web::serve_playground(&format!("{host}:{port}"), PAGE) {
            eprintln!("❌ cannot listen on {host}:{port}: {e}");
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
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
    if flag("--lint") {
        // Machine-readable compile check for editors: {"ok": bool, "errors": [{"line", "message"}]}
        let errors = match compile_check(&src) {
            Ok(()) => vec![],
            Err(e) => {
                let (line, message) = e
                    .strip_prefix("line ")
                    .and_then(|r| r.split_once(": "))
                    .and_then(|(n, m)| n.parse::<usize>().ok().map(|n| (n, m.to_string())))
                    .unwrap_or((1, e.clone()));
                vec![serde_json::json!({"line": line, "message": message})]
            }
        };
        println!(
            "{}",
            serde_json::json!({"ok": errors.is_empty(), "errors": errors})
        );
        return ExitCode::SUCCESS;
    }
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
