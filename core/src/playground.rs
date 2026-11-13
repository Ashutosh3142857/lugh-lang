//! The sandboxed runner behind the web playground (in the browser via
//! WebAssembly, or on a server via `lugh playground`).
//!
//! Safety: adapters that touch the outside world (files, shell, network,
//! databases, MCP) are refused, nothing is written to disk, approvals are shown
//! but granted, and every run has a 10-second, 25-effect budget that also
//! applies to pure computation.

use crate::parser::build_tree;
use crate::program::setup;
use crate::runtime::{Env, Flow, Runtime};

pub const MAX_SOURCE: usize = 50_000;

/// Run `src` and return everything it printed. `mode`: "run", "test" or "check".
pub fn run(src: &str, mode: &str) -> String {
    if src.len() > MAX_SOURCE {
        return format!("❌ program is larger than {} KB", MAX_SOURCE / 1000);
    }
    let tree = build_tree(src);
    let Some(top) = tree.first().filter(|l| l.text.starts_with("agent ")) else {
        return "❌ a Lugh program must start with: agent <Name>".into();
    };
    let mut rt = Runtime::ephemeral();
    rt.agent = top.text[6..].trim().to_string();
    let program = match setup(&mut rt, top) {
        Ok(p) => p,
        Err(e) => return format!("❌ {e}"),
    };
    if let Some(a) = rt.adapters.iter().find(|a| a.as_str() != "llm") {
        return format!(
            "❌ `use {a}` is disabled in the playground: adapters act on real files, networks and services.\n   Install Lugh to run it: https://github.com/Ashutosh3142857/lugh-lang"
        );
    }
    rt.time_budget = rt.time_budget.min(10.0);
    rt.call_budget = rt.call_budget.min(25);

    let mut header = format!("⚡ agent '{}'", rt.agent);
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
    rt.take_output()
}
