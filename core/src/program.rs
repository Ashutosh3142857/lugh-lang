//! Loading a program: header (use, budget, policy, auth), goals, tests, routes,
//! pages and jobs, plus the compile-time checks shared by every front end.

use crate::adapters;
use crate::parser::{self, build_tree, calls_in_block, parse_block, parse_expr, walk, Block, Line};
use crate::runtime::Runtime;
use crate::stdlib;
use crate::web;

pub const EFFECTS: [&str; 7] = [
    "read", "write", "send", "spend", "delete", "submit", "execute",
];

pub struct Goal {
    pub text: String,
    pub body: Block,
    pub source: String,
}

pub struct Program {
    pub goals: Vec<Goal>,
    pub routes: Vec<web::Route>,
    pub pages: Vec<web::Page>,
    pub tests: Vec<Goal>,
    pub jobs: Vec<web::Job>,
    pub auth_env: Option<String>,
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

pub fn setup(rt: &mut Runtime, top: &Line) -> Result<Program, String> {
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
pub fn compile_check(src: &str) -> Result<(), String> {
    let tree = build_tree(src);
    let top = tree
        .first()
        .filter(|l| l.text.starts_with("agent "))
        .ok_or("a Lugh program must start with: agent <Name>")?;
    let mut rt = Runtime::ephemeral();
    setup(&mut rt, top).map(|_| ())
}
