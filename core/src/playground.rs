//! The sandboxed runner behind the web playground (in the browser via
//! WebAssembly, or on a server via `lugh playground`).
//!
//! Safety: adapters never touch the outside world. They act on a simulated one
//! (see `sandbox.rs`): a few files, an HTTP API and five MCP servers, created
//! fresh for every run. Every call still passes the policy gate first, so
//! `allow`, `ask human before` and `deny` behave exactly as they do for real.
//! Approvals are answered by a setting (yes by default), nothing is written to
//! disk, and every run has a 10-second, 25-effect budget that also applies to
//! pure computation.

use crate::parser::build_tree;
use crate::program::setup;
use crate::runtime::{Env, Flow, Runtime};

pub const MAX_SOURCE: usize = 50_000;

/// Run `src` and return everything it printed. `mode`: "run", "test", "check" or
/// "world" (describe the simulated world). Approvals are answered yes.
pub fn run(src: &str, mode: &str) -> String {
    run_with(src, mode, false)
}

/// Like `run`, but `decline` answers every approval with no.
pub fn run_with(src: &str, mode: &str, decline: bool) -> String {
    if mode == "world" {
        return crate::sandbox::describe().to_string();
    }
    if src.len() > MAX_SOURCE {
        return format!("❌ program is larger than {} KB", MAX_SOURCE / 1000);
    }
    let tree = build_tree(src);
    let Some(top) = tree.first().filter(|l| l.text.starts_with("agent ")) else {
        return "❌ a Lugh program must start with: agent <Name>".into();
    };
    let mut rt = Runtime::ephemeral();
    rt.decline = decline;
    rt.agent = top.text[6..].trim().to_string();
    let program = match setup(&mut rt, top) {
        Ok(p) => p,
        Err(e) => return format!("❌ {e}"),
    };
    let simulated = rt.adapters.iter().any(|a| a.as_str() != "llm");
    rt.time_budget = rt.time_budget.min(10.0);
    rt.call_budget = rt.call_budget.min(25);

    let mut header = format!("⚡ agent '{}'", rt.agent);
    if simulated {
        header.push_str(&format!(
            "  uses {}  (simulated world: nothing real is touched)",
            rt.adapters.join(", ")
        ));
    }
    if !program.routes.is_empty() || !program.pages.is_empty() || !program.jobs.is_empty() {
        header.push_str("  (routes, pages and jobs compile here; serve them with `lugh --serve`)");
    }
    rt.emit(&header);

    match mode {
        "check" => {
            rt.emit(&format!(
                "✅ compiles: {} goal(s), {} test(s), {} route(s), {} page(s)",
                program.goals.len(),
                program.tests.len(),
                program.routes.len(),
                program.pages.len()
            ));
        }
        "test" => {
            if program.tests.is_empty() {
                rt.emit("No `test` blocks in this program.");
            }
            let (mut passed, mut failed) = (0, 0);
            rt.quiet = true;
            for t in &program.tests {
                rt.start_clock();
                match rt.run_block(&t.body, &mut Env::default()) {
                    Ok(_) | Err(Flow::Respond(..)) => {
                        passed += 1;
                        rt.emit(&format!("  ✓ {}", t.text));
                    }
                    Err(Flow::Fail(m) | Flow::Stop(m) | Flow::Denied(m) | Flow::Budget(m)) => {
                        failed += 1;
                        rt.emit(&format!("  ✗ {}\n      {m}", t.text));
                    }
                }
            }
            rt.emit(&format!("\n{passed} passed, {failed} failed"));
        }
        _ => {
            if program.goals.is_empty() {
                rt.emit("No `goal` blocks to run. Try Test or Check.");
            }
            for g in &program.goals {
                rt.emit(&format!("\n🎯 {}", g.text));
                rt.start_clock();
                match rt.run_block(&g.body, &mut Env::default()) {
                    Ok(_) | Err(Flow::Respond(..)) => rt.emit("✅ done"),
                    Err(Flow::Stop(m)) => rt.emit(&format!("⏹  stopped: {m}")),
                    Err(Flow::Fail(m) | Flow::Denied(m) | Flow::Budget(m)) => {
                        rt.emit(&format!("❌ {m}"))
                    }
                }
            }
        }
    }
    if mode == "run" || mode.is_empty() {
        policy_report(&rt);
    }
    rt.take_output()
}

/// After a run: every effect the program attempted and what the policy did with it.
fn policy_report(rt: &Runtime) {
    let mut lines = Vec::new();
    let mut asked: Option<String> = None;
    for r in rt.audit_records() {
        let action = r["action"].as_str().unwrap_or_default().to_string();
        let effect = r["effect"].as_str().unwrap_or_default();
        match r["kind"].as_str().unwrap_or_default() {
            "human" => asked = r["question"].as_str().map(String::from),
            "effect" => {
                let was_asked = asked.take().is_some_and(|q| q.ends_with(&action));
                let how = if was_asked {
                    "asked you, approved"
                } else {
                    "allowed by policy"
                };
                lines.push(format!("  ✓ {effect:<7} {how:<20} {action}"));
            }
            "effect_error" => {
                asked = None;
                lines.push(format!(
                    "  ! {effect:<7} {:<20} {action}",
                    "allowed, then failed"
                ));
            }
            "denied" => lines.push(format!(
                "  ✗ {effect:<7} {:<20} {action}",
                "denied by policy"
            )),
            "rejected" => {
                asked = None;
                lines.push(format!("  ✗ {effect:<7} {:<20} {action}", "you said no"))
            }
            _ => {}
        }
    }
    if !lines.is_empty() {
        rt.emit("\n📋 Policy log: every action, checked before it ran");
        for l in lines {
            rt.emit(&l);
        }
    }
}
