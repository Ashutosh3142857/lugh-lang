//! The playground sandbox, tested through the library API (the same code the
//! browser build runs).

use lugh_core::playground::{run, run_with};

#[test]
fn runs_goals_and_tests() {
    let out = run("agent A\n  goal \"g\"\n    let xs = [\n      1,\n      2\n    ]\n    log \"sum={sum(xs)}\"\n", "run");
    assert!(out.contains("sum=3") && out.contains("✅ done"), "{out}");
    let out = run(
        "agent A\n  test \"t\"\n    expect 1 == 1\n  test \"f\"\n    expect 1 == 2\n",
        "test",
    );
    assert!(out.contains("1 passed, 1 failed"), "{out}");
}

#[test]
fn adapters_act_on_a_simulated_world_never_the_real_one() {
    // Reads see only the simulated files.
    let out = run(
        "agent A\n  use fs\n  policy\n    allow read\n  goal \"g\"\n    log fs.read \"/etc/hostname\"\n",
        "run",
    );
    assert!(out.contains("no file '/etc/hostname'"), "{out}");
    // Writes stay in memory: nothing appears on disk.
    let probe = "lugh_playground_probe.txt";
    let out = run(
        &format!("agent A\n  use fs\n  policy\n    allow read, write\n  goal \"g\"\n    fs.write \"{probe}\" \"x\"\n    log fs.read \"{probe}\"\n"),
        "run",
    );
    assert!(
        out.contains("📝 x") && !std::path::Path::new(probe).exists(),
        "{out}"
    );
    // No real network and no real shell.
    let out = run(
        "agent A\n  use http, shell\n  policy\n    allow read, execute\n  goal \"g\"\n    log http.get \"https://example.org/\"\n",
        "run",
    );
    assert!(
        out.contains("only simulates https://api.example.com"),
        "{out}"
    );
    let out = run(
        "agent A\n  use shell\n  policy\n    allow execute\n  goal \"g\"\n    log shell.run \"rm -rf /tmp/x\"\n",
        "run",
    );
    assert!(out.contains("only simulates `echo`"), "{out}");
    let out = run("agent A\n  use db\n  goal \"g\"\n    log 1\n", "run");
    assert!(out.contains("simulated world"), "{out}");
}

#[test]
fn the_policy_gate_runs_before_every_simulated_call() {
    let src = "agent A\n  use mcp\n  policy\n    allow read\n    ask human before submit\n    deny delete\n  goal \"add\"\n    mcp.call \"notes\" \"add_note\" {title: \"T\", body: \"B\"}\n  goal \"delete\"\n    mcp.call \"notes\" \"delete_note\" {title: \"T\"}\n";
    let out = run(src, "run");
    assert!(out.contains("asked you, approved"), "{out}");
    assert!(
        out.contains("policy denies 'delete'") && out.contains("denied by policy"),
        "{out}"
    );
    let out = run_with(src, "run", true);
    assert!(
        out.contains("human rejected") && out.contains("you said no"),
        "{out}"
    );
    assert!(!out.contains("asked you, approved"), "{out}");
}

#[test]
fn every_example_runs_and_is_embedded_in_the_page() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../playground");
    let page = std::fs::read_to_string(root.join("index.html")).unwrap();
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("examples/index.json")).unwrap())
            .unwrap();
    for it in index.as_array().unwrap() {
        let id = it["id"].as_str().unwrap();
        let src = std::fs::read_to_string(root.join(format!("examples/{id}.lugh"))).unwrap();
        assert!(
            page.contains(&src),
            "{id}: index.html is out of date, run python3 playground/sync_examples.py"
        );
        let mode = it["mode"].as_str().unwrap();
        let out = run(&src, mode);
        // Intended failures are policy denials, budgets and the deliberately failing
        // test; anything pointing at a line of the program is a bug in the example.
        assert!(
            !out.contains("❌ line") && !out.contains("unexpected"),
            "{id}: {out}"
        );
        assert!(
            !out.contains("is not defined") && !out.contains("has no part"),
            "{id}: {out}"
        );
        if mode == "run" {
            // budget_guard is meant to hit its call budget before finishing.
            let expected = if id == "budget_guard" {
                "call budget of 5 used up"
            } else {
                "✅ done"
            };
            assert!(out.contains(expected), "{id}: {out}");
        }
    }
}

#[test]
fn runaway_computation_hits_the_time_budget() {
    let started = std::time::Instant::now();
    let out = run(
        "agent A\n  budget time 1s\n  goal \"g\"\n    let a = range(100000)\n    let p = pairs x from a, y from a where x == y\n",
        "run",
    );
    assert!(out.contains("time budget"), "{out}");
    assert!(
        started.elapsed().as_secs() < 5,
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn compile_errors_are_reported() {
    assert!(
        run("agent A\n  goal \"g\"\n    log text.nope(1)\n", "check").contains("unknown function")
    );
    assert!(run("not a program", "run").contains("must start with"));
}
