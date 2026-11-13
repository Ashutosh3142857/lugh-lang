//! End-to-end tests: run the bundled example programs through the release
//! binary and check the results that matter, line by line.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// Programs share memory/stats/audit files in the repo root, so tests run one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn clean() {
    for f in ["lugh_memory.json", "lugh_stats.json", "lugh_audit.jsonl"] {
        let _ = std::fs::remove_file(repo().join(f));
    }
}

fn run(program: &str, extra: &[&str]) -> String {
    let dir = repo();
    clean();
    let out = Command::new(env!("CARGO_BIN_EXE_lugh"))
        .arg(program)
        .arg("--yes")
        .args(extra)
        .current_dir(&dir)
        .output()
        .expect("run lugh");
    assert!(out.status.success(), "lugh exited with {}", out.status);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn has(out: &str, needle: &str) {
    assert!(out.contains(needle), "missing {needle:?} in output:\n{out}");
}

#[test]
fn research_desk_primitives() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = run("examples/research_desk.lugh", &["--eval"]);
    has(&out, "Priority: Invoice INV-2041 overdue");
    has(
        &out,
        "Grounded deadline: Payment is due by 30 November 2026.",
    );
    has(&out, "Recalled: Payment is due by 30 November 2026.");
    has(&out, "Fully paid: 1 invoice(s)");
    has(&out, "consensus 2/3");
    has(&out, "Agreed fare: Etihad at INR 41800");
    has(&out, "route succeeded: fs.read");
    has(&out, "Plan: Book the agreed fare now");
    has(&out, "weighted score: 1.000");
}

#[test]
fn flight_scout_fallback_and_approval() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = run("examples/flight_scout.lugh", &[]);
    has(&out, "trying fallback");
    has(&out, "ensured: best.price < 45000");
    has(&out, "[#travel] Booking Etihad at INR 41800");
    has(&out, "✅ goal complete");
}

#[test]
fn static_check_reports_policy() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = run("examples/fare_desk.lugh", &["--check"]);
    has(&out, "fs.append      effect=write    → allowed");
    has(&out, "static check passed");
}

fn http(port: u16, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    let code = raw[9..12].parse().unwrap();
    let body = raw
        .split_once("\r\n\r\n")
        .map(|x| x.1.to_string())
        .unwrap_or_default();
    (code, body)
}

#[test]
fn web_app_routes_pages_and_approvals() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    clean();
    let _ = std::fs::remove_file(repo().join("examples/data/bookings.log"));
    let port = 18_000 + (std::process::id() % 2000) as u16;
    let mut server = Command::new(env!("CARGO_BIN_EXE_lugh"))
        .args(["examples/fare_desk.lugh", "--serve", &port.to_string()])
        .current_dir(repo())
        .stdout(Stdio::null())
        .spawn()
        .expect("start server");
    for _ in 0..50 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let (code, best) = http(port, "GET", "/api/best", "");
    assert_eq!(code, 200);
    assert!(
        best.contains("\"price\":41800"),
        "consensus over HTTP: {best}"
    );

    assert_eq!(http(port, "GET", "/api/fares/Etihad", "").0, 200);
    assert_eq!(http(port, "GET", "/api/fares/Nobody", "").0, 404);
    assert_eq!(
        http(
            port,
            "POST",
            "/api/book",
            r#"{"airline":"Emirates","price":48200}"#
        )
        .0,
        422
    );

    // A booking waits for a human; approve it through the inbox API.
    let booking = std::thread::spawn(move || {
        http(
            port,
            "POST",
            "/api/book",
            r#"{"airline":"Etihad","price":41800}"#,
        )
    });
    let mut pending = String::new();
    for _ in 0..50 {
        pending = http(port, "GET", "/_lugh/api/approvals", "").1;
        if pending.contains("Book Etihad") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(pending.contains("\"id\":1"), "pending approvals: {pending}");
    assert_eq!(
        http(
            port,
            "POST",
            "/_lugh/api/approvals/1",
            r#"{"approve":true}"#
        )
        .0,
        200
    );
    let (code, body) = booking.join().unwrap();
    assert_eq!(code, 201, "booking: {body}");

    let (code, page) = http(port, "GET", "/", "");
    assert_eq!(code, 200);
    assert!(page.contains("<h1>Fare desk</h1>") && page.contains("data-lugh=\"table\""));
    assert_eq!(http(port, "GET", "/_lugh/inbox", "").0, 200);

    server.kill().ok();
    let _ = server.wait();
}

#[test]
fn stdlib_suite_written_in_lugh() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    clean();
    let _ = std::fs::remove_file(repo().join("lugh.db"));
    let out = Command::new(env!("CARGO_BIN_EXE_lugh"))
        .args(["examples/stdlib_test.lugh", "--test"])
        .current_dir(repo())
        .output()
        .expect("run tests");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "Lugh test suite failed:\n{text}");
    assert!(text.contains("13 passed, 0 failed"), "{text}");
}

#[test]
fn failing_lugh_test_exits_nonzero() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("lugh-neg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("neg.lugh"), "agent Neg\n  test \"fails\"\n    expect 1 == 2\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lugh")).args(["neg.lugh", "--test"]).current_dir(&dir).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("0 passed, 1 failed"));
}

#[test]
fn unknown_function_is_a_compile_error() {
    let dir = std::env::temp_dir().join(format!("lugh-typo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("typo.lugh"), "agent T\n  test \"t\"\n    expect text.uper(\"a\") == \"A\"\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lugh")).args(["typo.lugh", "--test"]).current_dir(&dir).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown function 'text.uper'"));
}

fn get(port: u16, path: &str, auth: Option<&str>) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let header = auth.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n{header}\r\n").unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    (raw[9..12].parse().unwrap(), raw.split_once("\r\n\r\n").map(|x| x.1.to_string()).unwrap_or_default())
}

#[test]
fn auth_token_and_scheduled_jobs() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("lugh-auth-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("app.lugh"),
        "agent A\n  auth token env \"TEST_LUGH_TOKEN\"\n  policy\n    allow read, write\n  every 1s\n    remember \"ticks\" = (recall \"ticks\" or 0) + 1\n  route GET \"/ticks\"\n    respond {ticks: recall \"ticks\"}\n",
    )
    .unwrap();
    let port = 20_000 + (std::process::id() % 2000) as u16;

    // Without the variable, the server must refuse to start.
    let out = Command::new(env!("CARGO_BIN_EXE_lugh")).args(["app.lugh", "--serve", &port.to_string()]).current_dir(&dir).env_remove("TEST_LUGH_TOKEN").output().unwrap();
    assert!(!out.status.success());

    let token = "abcdef0123456789abcdef";
    let mut server = Command::new(env!("CARGO_BIN_EXE_lugh"))
        .args(["app.lugh", "--serve", &port.to_string()])
        .current_dir(&dir)
        .env("TEST_LUGH_TOKEN", token)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..50 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(get(port, "/ticks", None).0, 401);
    assert_eq!(get(port, "/ticks", Some("wrong-token-0000000000")).0, 401);
    std::thread::sleep(Duration::from_millis(2500));
    let (code, body) = get(port, "/ticks", Some(token));
    server.kill().ok();
    let _ = server.wait();
    assert_eq!(code, 200);
    let ticks: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(ticks["ticks"].as_f64().unwrap_or(0.0) >= 2.0, "scheduler ticks: {body}");
}
