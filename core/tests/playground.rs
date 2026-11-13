//! The playground sandbox, tested through the library API (the same code the
//! browser build runs).

use lugh_core::playground::run;

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
fn refuses_every_adapter_that_touches_the_outside_world() {
    for adapter in ["fs", "shell", "http", "db", "mcp"] {
        let out = run(
            &format!("agent A\n  use {adapter}\n  goal \"g\"\n    log 1\n"),
            "run",
        );
        assert!(
            out.contains("disabled in the playground"),
            "{adapter}: {out}"
        );
    }
}

#[test]
fn approvals_are_shown_not_silently_skipped() {
    let out = run(
        "agent A\n  goal \"g\"\n    ask human \"Send it?\" -> ok\n    log ok\n",
        "run",
    );
    assert!(
        out.contains("would ask you") && out.contains("Send it?"),
        "{out}"
    );
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
