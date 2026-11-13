//! The Lugh standard library: pure functions grouped into modules, modelled on
//! what Python and JavaScript projects reach for most often.
//!
//!   text.*      str / re           · String / RegExp
//!   list.*      itertools, dict    · lodash
//!   math.*      math               · Math
//!   stats.*     statistics         · simple-statistics
//!   time.*      datetime           · date-fns
//!   json.* csv.* json, csv         · JSON, PapaParse
//!   crypto.*    hashlib, hmac, uuid· node:crypto
//!   encode.*    base64, urllib     · btoa, encodeURIComponent
//!   schema.*    pydantic           · zod
//!   template.*  jinja2             · handlebars
//!   env()       os.environ, dotenv · process.env
//!
//! None of these touch the outside world, so they do not pass the policy gate.
//! Anything with an effect (files, network, databases) is an adapter instead.

use crate::info::{canonical, truthy, txt, val_eq};
use crate::parser::num;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

type R = Result<Value, String>;

fn f(v: &Value, what: &str) -> Result<f64, String> {
    match v {
        Value::Number(n) => n.as_f64().ok_or_else(|| format!("{what}: not a number")),
        Value::String(s) => s
            .trim()
            .parse()
            .map_err(|_| format!("{what}: '{s}' is not a number")),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        other => Err(format!("{what}: expected a number, got {}", txt(other))),
    }
}

fn list<'a>(v: &'a Value, what: &str) -> Result<&'a Vec<Value>, String> {
    v.as_array()
        .ok_or_else(|| format!("{what}: expected a list, got {}", txt(v)))
}

fn nums(v: &Value, what: &str) -> Result<Vec<f64>, String> {
    list(v, what)?.iter().map(|x| f(x, what)).collect()
}

fn s(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        other => txt(other),
    }
}

fn arg<'a>(args: &'a [Value], i: usize, name: &str) -> Result<&'a Value, String> {
    args.get(i)
        .ok_or_else(|| format!("{name} needs argument {}", i + 1))
}

fn regex(p: &str) -> Result<regex::Regex, String> {
    regex::Regex::new(p).map_err(|e| format!("invalid pattern: {e}"))
}

// ───────────────────────────── time (UTC, ISO 8601) ─────────────────────────────

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

fn now_secs() -> f64 {
    crate::clock::now()
}

/// Accepts unix seconds, "YYYY-MM-DD", "YYYY-MM-DDTHH:MM[:SS][Z|+HH:MM]".
fn to_secs(v: &Value) -> Result<f64, String> {
    if let Some(n) = v.as_f64() {
        return Ok(n);
    }
    let t = s(v);
    let t = t.trim();
    let bad = || format!("time: cannot read '{t}' (use YYYY-MM-DD or ISO 8601)");
    let num = |a: usize, b: usize| {
        t.get(a..b)
            .and_then(|x| x.parse::<i64>().ok())
            .ok_or_else(bad)
    };
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    if t.get(4..5) != Some("-")
        || t.get(7..8) != Some("-")
        || !(1..=12).contains(&mo)
        || !(1..=31).contains(&d)
    {
        return Err(bad());
    }
    let mut secs = days_from_civil(y, mo, d) * 86400;
    if t.len() > 10 {
        let (h, mi) = (num(11, 13)?, num(14, 16)?);
        let sec = if t.get(16..17) == Some(":") {
            num(17, 19)?
        } else {
            0
        };
        secs += h * 3600 + mi * 60 + sec;
        if let Some(pos) = t[16..].find(['+', '-']).map(|p| p + 16) {
            let sign = if &t[pos..pos + 1] == "+" { 1 } else { -1 };
            let oh = num(pos + 1, pos + 3)?;
            let om = t
                .get(pos + 4..pos + 6)
                .and_then(|x| x.parse::<i64>().ok())
                .unwrap_or(0);
            secs -= sign * (oh * 3600 + om * 60);
        }
    }
    Ok(secs as f64)
}

fn iso(secs: f64, date_only: bool) -> String {
    let total = secs.floor() as i64;
    let (days, rem) = (total.div_euclid(86400), total.rem_euclid(86400));
    let (y, m, d) = civil_from_days(days);
    if date_only {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            rem / 3600,
            rem % 3600 / 60,
            rem % 60
        )
    }
}

fn unit_secs(u: &str) -> Result<f64, String> {
    Ok(match u.trim_end_matches('s') {
        "second" => 1.0,
        "minute" => 60.0,
        "hour" => 3600.0,
        "day" => 86400.0,
        "week" => 604800.0,
        other => {
            return Err(format!(
                "time: unknown unit '{other}' (seconds, minutes, hours, days, weeks)"
            ))
        }
    })
}

fn is_date_only(v: &Value) -> bool {
    matches!(v, Value::String(s) if s.trim().len() == 10)
}

// ───────────────────────────── csv (RFC 4180) ─────────────────────────────

fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let (mut rows, mut row, mut field) = (Vec::new(), Vec::new(), String::new());
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    rows
}

fn csv_cell(v: &Value) -> String {
    let t = s(v);
    if t.contains([',', '"', '\n']) {
        format!("\"{}\"", t.replace('"', "\"\""))
    } else {
        t
    }
}

fn cell_value(t: &str) -> Value {
    match t.parse::<f64>() {
        Ok(n) if !t.is_empty() && !t.starts_with('0') || t == "0" || t.starts_with("0.") => num(n),
        _ => Value::String(t.to_string()),
    }
}

// ───────────────────────────── schema (pydantic / zod style) ─────────────────────────────

/// spec: {"field": "number", "nickname?": "text", "tags": "list", "price": "number 0..50000"}
fn schema_errors(v: &Value, spec: &Value, path: &str, out: &mut Vec<String>) {
    let Some(spec) = spec.as_object() else {
        out.push("schema: spec must be an object".into());
        return;
    };
    let Some(obj) = v.as_object() else {
        out.push(format!(
            "{}: expected an object",
            if path.is_empty() { "value" } else { path }
        ));
        return;
    };
    for (key, rule) in spec {
        let optional = key.ends_with('?');
        let name = key.trim_end_matches('?');
        let at = if path.is_empty() {
            name.to_string()
        } else {
            format!("{path}.{name}")
        };
        let Some(val) = obj.get(name).filter(|x| !x.is_null()) else {
            if !optional {
                out.push(format!("{at}: required"));
            }
            continue;
        };
        if rule.is_object() {
            schema_errors(val, rule, &at, out);
            continue;
        }
        let rule = s(rule);
        let mut parts = rule.split_whitespace();
        let kind = parts.next().unwrap_or("any");
        let ok = match kind {
            "number" => val.is_number(),
            "int" => val.as_f64().is_some_and(|x| x.fract() == 0.0),
            "text" => val.is_string(),
            "bool" => val.is_boolean(),
            "list" => val.is_array(),
            "object" => val.is_object(),
            "email" => val.as_str().is_some_and(|e| {
                let (u, d) = e.split_once('@').unwrap_or(("", ""));
                !u.is_empty() && d.contains('.') && !d.starts_with('.') && !d.ends_with('.')
            }),
            "date" => val.is_string() && to_secs(val).is_ok(),
            "any" => true,
            other => {
                out.push(format!("{at}: unknown type '{other}'"));
                continue;
            }
        };
        if !ok {
            out.push(format!("{at}: expected {kind}, got {}", txt(val)));
            continue;
        }
        if let Some((lo, hi)) = parts.next().and_then(|r| r.split_once("..")) {
            let size = match val {
                Value::String(t) => t.chars().count() as f64,
                Value::Array(a) => a.len() as f64,
                other => other.as_f64().unwrap_or(0.0),
            };
            if lo.parse::<f64>().is_ok_and(|lo| size < lo)
                || hi.parse::<f64>().is_ok_and(|hi| size > hi)
            {
                out.push(format!("{at}: must be within {lo}..{hi}"));
            }
        }
    }
}

// ───────────────────────────── template (mustache subset) ─────────────────────────────

fn lookup<'a>(scopes: &[&'a Value], path: &str) -> Option<&'a Value> {
    if path == "." {
        return scopes.last().copied();
    }
    for scope in scopes.iter().rev() {
        let mut v = *scope;
        let mut ok = true;
        for part in path.split('.') {
            match v
                .get(part)
                .or_else(|| part.parse::<usize>().ok().and_then(|i| v.get(i)))
            {
                Some(next) => v = next,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return Some(v);
        }
    }
    None
}

fn html_escape(t: &str) -> String {
    t.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// {{name}} escaped · {{{name}}} raw · {{#items}}…{{/items}} loop or if · {{^x}}…{{/x}} if not
fn render(tpl: &str, scopes: &mut Vec<&Value>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = tpl;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        if let Some(inner) = after.strip_prefix('{') {
            let close = inner.find("}}}").ok_or("template: unclosed {{{")?;
            out.push_str(
                &lookup(scopes, inner[..close].trim())
                    .map(s)
                    .unwrap_or_default(),
            );
            rest = &inner[close + 3..];
            continue;
        }
        let close = after.find("}}").ok_or("template: unclosed {{")?;
        let tag = after[..close].trim();
        rest = &after[close + 2..];
        if let Some(name) = tag.strip_prefix('#').or_else(|| tag.strip_prefix('^')) {
            let end_tag = format!("{{{{/{name}}}}}");
            let end = rest
                .find(&end_tag)
                .ok_or(format!("template: missing {end_tag}"))?;
            let (body, after_section) = (&rest[..end], &rest[end + end_tag.len()..]);
            let value = lookup(scopes, name);
            if tag.starts_with('^') {
                if !value.is_some_and(truthy) {
                    out.push_str(&render(body, scopes)?);
                }
            } else if let Some(Value::Array(items)) = value {
                for item in items {
                    scopes.push(item);
                    let piece = render(body, scopes);
                    scopes.pop();
                    out.push_str(&piece?);
                }
            } else if let Some(v) = value.filter(|v| truthy(v)) {
                scopes.push(v);
                let piece = render(body, scopes);
                scopes.pop();
                out.push_str(&piece?);
            }
            rest = after_section;
        } else {
            out.push_str(&html_escape(
                &lookup(scopes, tag).map(s).unwrap_or_default(),
            ));
        }
    }
    out.push_str(rest);
    Ok(out)
}

// ───────────────────────────── randomness ─────────────────────────────

fn fill_random(buf: &mut [u8]) {
    #[cfg(feature = "native")]
    getrandom::getrandom(buf).expect("system randomness unavailable");
    #[cfg(not(feature = "native"))]
    {
        // Browser build: a xorshift generator seeded from the clock. Fine for the
        // playground; not for secrets. Native builds use the OS generator.
        use std::sync::atomic::{AtomicU64, Ordering};
        static STATE: AtomicU64 = AtomicU64::new(0);
        let mut x = STATE.load(Ordering::Relaxed);
        if x == 0 {
            x = (crate::clock::now() * 1e6) as u64 | 1;
        }
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x >> 24) as u8;
        }
        STATE.store(x, Ordering::Relaxed);
    }
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    fill_random(&mut b);
    b
}

fn random_unit() -> f64 {
    (u64::from_le_bytes(random_bytes::<8>()) >> 11) as f64 / (1u64 << 53) as f64
}

// ───────────────────────────── dispatch ─────────────────────────────

/// Short names that need no module prefix.
fn alias(name: &str) -> &str {
    match name {
        "sum" => "list.sum",
        "avg" | "mean" => "stats.mean",
        "min" => "list.min",
        "max" => "list.max",
        "round" => "math.round",
        "abs" => "math.abs",
        "upper" => "text.upper",
        "lower" => "text.lower",
        "trim" => "text.trim",
        "split" => "text.split",
        "join" => "text.join",
        "replace" => "text.replace",
        "contains" => "list.contains",
        "unique" => "list.unique",
        "first" => "list.first",
        "last" => "list.last",
        "range" => "list.range",
        "keys" => "list.keys",
        "values" => "list.values",
        "now" => "time.now",
        "today" => "time.today",
        "uuid" => "crypto.uuid",
        "number" => "text.number",
        other => other,
    }
}

pub fn exists(name: &str) -> bool {
    NAMES.contains(&alias(name))
}

pub const NAMES: &[&str] = &[
    "text.upper",
    "text.lower",
    "text.trim",
    "text.title",
    "text.split",
    "text.join",
    "text.replace",
    "text.contains",
    "text.starts",
    "text.ends",
    "text.slice",
    "text.words",
    "text.lines",
    "text.pad",
    "text.repeat",
    "text.number",
    "text.matches",
    "text.find",
    "text.find_all",
    "text.sub",
    "text.length",
    "list.sum",
    "list.min",
    "list.max",
    "list.first",
    "list.last",
    "list.take",
    "list.skip",
    "list.reverse",
    "list.unique",
    "list.flatten",
    "list.zip",
    "list.range",
    "list.contains",
    "list.index",
    "list.chunk",
    "list.pluck",
    "list.concat",
    "list.keys",
    "list.values",
    "list.merge",
    "list.get",
    "list.set",
    "list.has",
    "list.count_by",
    "list.index_by",
    "list.compact",
    "math.round",
    "math.floor",
    "math.ceil",
    "math.abs",
    "math.sqrt",
    "math.pow",
    "math.log",
    "math.exp",
    "math.clamp",
    "math.random",
    "math.random_int",
    "math.pi",
    "stats.mean",
    "stats.median",
    "stats.mode",
    "stats.variance",
    "stats.stdev",
    "stats.percentile",
    "stats.min",
    "stats.max",
    "stats.sum",
    "stats.normalize",
    "time.now",
    "time.today",
    "time.unix",
    "time.parse",
    "time.format",
    "time.add",
    "time.diff",
    "time.weekday",
    "time.start_of",
    "json.parse",
    "json.text",
    "json.pretty",
    "csv.parse",
    "csv.text",
    "crypto.sha256",
    "crypto.hmac_sha256",
    "crypto.uuid",
    "crypto.random_hex",
    "crypto.equal",
    "encode.base64",
    "encode.unbase64",
    "encode.url",
    "encode.unurl",
    "encode.hex",
    "schema.check",
    "schema.valid",
    "template.render",
    "env",
    "type",
];

pub fn call(name: &str, args: &[Value]) -> R {
    let name = alias(name);
    let a = |i: usize| arg(args, i, name);
    let text = |i: usize| a(i).map(s);
    let number = |i: usize| a(i).and_then(|v| f(v, name));
    let opt_num = |i: usize, d: f64| {
        args.get(i)
            .map(|v| f(v, name))
            .transpose()
            .map(|x| x.unwrap_or(d))
    };
    Ok(match name {
        // ── text ──
        "text.upper" => Value::String(text(0)?.to_uppercase()),
        "text.lower" => Value::String(text(0)?.to_lowercase()),
        "text.trim" => Value::String(text(0)?.trim().to_string()),
        "text.title" => Value::String(
            text(0)?
                .split(' ')
                .map(|w| {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| {
                            f.to_uppercase()
                                .chain(c.flat_map(char::to_lowercase))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect::<Vec<String>>()
                .join(" "),
        ),
        "text.split" => {
            let t = text(0)?;
            let sep = args.get(1).map(s).unwrap_or_else(|| " ".into());
            Value::from(t.split(sep.as_str()).map(String::from).collect::<Vec<_>>())
        }
        "text.join" => {
            let sep = args.get(1).map(s).unwrap_or_default();
            Value::String(
                list(a(0)?, name)?
                    .iter()
                    .map(s)
                    .collect::<Vec<_>>()
                    .join(&sep),
            )
        }
        "text.replace" => Value::String(text(0)?.replace(&text(1)?, &text(2)?)),
        "text.contains" => Value::Bool(text(0)?.contains(&text(1)?)),
        "text.starts" => Value::Bool(text(0)?.starts_with(&text(1)?)),
        "text.ends" => Value::Bool(text(0)?.ends_with(&text(1)?)),
        "text.slice" => {
            let chars: Vec<char> = text(0)?.chars().collect();
            let len = chars.len() as i64;
            let idx =
                |x: f64| (if x < 0.0 { len + x as i64 } else { x as i64 }).clamp(0, len) as usize;
            let (from, to) = (idx(number(1)?), idx(opt_num(2, len as f64)?));
            Value::String(chars[from..to.max(from)].iter().collect())
        }
        "text.words" => Value::from(
            text(0)?
                .split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>(),
        ),
        "text.lines" => Value::from(text(0)?.lines().map(String::from).collect::<Vec<_>>()),
        "text.pad" => {
            let (t, width) = (text(0)?, number(1)? as usize);
            let fill = args
                .get(2)
                .map(s)
                .and_then(|x| x.chars().next())
                .unwrap_or(' ');
            let n = width.saturating_sub(t.chars().count());
            Value::String(std::iter::repeat_n(fill, n).collect::<String>() + &t)
        }
        "text.repeat" => Value::String(text(0)?.repeat(number(1)?.max(0.0) as usize)),
        "text.length" => Value::from(text(0)?.chars().count()),
        "text.number" => match text(0)?.trim().replace(',', "").parse::<f64>() {
            Ok(n) => num(n),
            Err(_) => Value::Null,
        },
        "text.matches" => Value::Bool(regex(&text(1)?)?.is_match(&text(0)?)),
        "text.find" => {
            let (t, re) = (text(0)?, regex(&text(1)?)?);
            match re.captures(&t) {
                Some(c) => Value::String(
                    c.get(1)
                        .or_else(|| c.get(0))
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default(),
                ),
                None => Value::Null,
            }
        }
        "text.find_all" => {
            let (t, re) = (text(0)?, regex(&text(1)?)?);
            Value::from(
                re.captures_iter(&t)
                    .map(|c| {
                        c.get(1)
                            .or_else(|| c.get(0))
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "text.sub" => Value::String(
            regex(&text(1)?)?
                .replace_all(&text(0)?, text(2)?.as_str())
                .into_owned(),
        ),

        // ── list / object ──
        "list.sum" | "stats.sum" => num(nums(a(0)?, name)?.iter().sum()),
        "list.min" | "list.max" | "stats.min" | "stats.max" => {
            let want_min = name.ends_with("min");
            let items: Vec<Value> = if args.len() > 1 {
                args.to_vec()
            } else {
                list(a(0)?, name)?.clone()
            };
            let pick = |x: &Value, y: &Value| match (x.as_f64(), y.as_f64()) {
                (Some(p), Some(q)) => p.total_cmp(&q),
                _ => txt(x).cmp(&txt(y)),
            };
            let best = if want_min {
                items.iter().min_by(|x, y| pick(x, y))
            } else {
                items.iter().max_by(|x, y| pick(x, y))
            };
            best.cloned().unwrap_or(Value::Null)
        }
        "list.first" => list(a(0)?, name)?.first().cloned().unwrap_or(Value::Null),
        "list.last" => list(a(0)?, name)?.last().cloned().unwrap_or(Value::Null),
        "list.take" => Value::Array(
            list(a(0)?, name)?
                .iter()
                .take(number(1)? as usize)
                .cloned()
                .collect(),
        ),
        "list.skip" => Value::Array(
            list(a(0)?, name)?
                .iter()
                .skip(number(1)? as usize)
                .cloned()
                .collect(),
        ),
        "list.reverse" => Value::Array(list(a(0)?, name)?.iter().rev().cloned().collect()),
        "list.unique" => {
            let mut seen = std::collections::HashSet::new();
            Value::Array(
                list(a(0)?, name)?
                    .iter()
                    .filter(|v| seen.insert(canonical(v)))
                    .cloned()
                    .collect(),
            )
        }
        "list.flatten" => Value::Array(
            list(a(0)?, name)?
                .iter()
                .flat_map(|v| match v {
                    Value::Array(inner) => inner.clone(),
                    other => vec![other.clone()],
                })
                .collect(),
        ),
        "list.zip" => {
            let (x, y) = (list(a(0)?, name)?, list(a(1)?, name)?);
            Value::Array(
                x.iter()
                    .zip(y)
                    .map(|(p, q)| Value::Array(vec![p.clone(), q.clone()]))
                    .collect(),
            )
        }
        "list.range" => {
            let (start, end) = if args.len() == 1 {
                (0.0, number(0)?)
            } else {
                (number(0)?, number(1)?)
            };
            let step = opt_num(2, 1.0)?;
            if step == 0.0 {
                return Err("list.range: step cannot be 0".into());
            }
            let mut out = Vec::new();
            let mut x = start;
            while (step > 0.0 && x < end) || (step < 0.0 && x > end) {
                if out.len() >= 1_000_000 {
                    return Err("list.range: more than 1,000,000 items".into());
                }
                out.push(num(x));
                x += step;
            }
            Value::Array(out)
        }
        "list.contains" => match a(0)? {
            Value::Array(items) => Value::Bool(items.iter().any(|v| val_eq(v, &args[1]))),
            Value::Object(o) => Value::Bool(o.contains_key(&text(1)?)),
            other => Value::Bool(s(other).contains(&text(1)?)),
        },
        "list.index" => {
            let target = a(1)?;
            list(a(0)?, name)?
                .iter()
                .position(|v| val_eq(v, target))
                .map(Value::from)
                .unwrap_or(Value::Null)
        }
        "list.chunk" => {
            let size = (number(1)? as usize).max(1);
            Value::Array(
                list(a(0)?, name)?
                    .chunks(size)
                    .map(|c| Value::Array(c.to_vec()))
                    .collect(),
            )
        }
        "list.pluck" => {
            let field = text(1)?;
            Value::Array(
                list(a(0)?, name)?
                    .iter()
                    .map(|v| v.get(&field).cloned().unwrap_or(Value::Null))
                    .collect(),
            )
        }
        "list.concat" => Value::Array(
            args.iter()
                .map(|v| list(v, name).cloned())
                .collect::<Result<Vec<_>, _>>()?
                .concat(),
        ),
        "list.compact" => Value::Array(
            list(a(0)?, name)?
                .iter()
                .filter(|v| truthy(v))
                .cloned()
                .collect(),
        ),
        "list.keys" => Value::from(
            a(0)?
                .as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
        ),
        "list.values" => Value::Array(
            a(0)?
                .as_object()
                .map(|o| o.values().cloned().collect())
                .unwrap_or_default(),
        ),
        "list.merge" => {
            let mut out = Map::new();
            for v in args {
                if let Value::Object(o) = v {
                    out.extend(o.clone());
                }
            }
            Value::Object(out)
        }
        "list.get" => {
            let fallback = args.get(2).cloned().unwrap_or(Value::Null);
            lookup(&[a(0)?], &text(1)?).cloned().unwrap_or(fallback)
        }
        "list.set" => {
            let mut o = a(0)?.as_object().cloned().unwrap_or_default();
            o.insert(text(1)?, a(2)?.clone());
            Value::Object(o)
        }
        "list.has" => Value::Bool(a(0)?.get(text(1)?).is_some_and(|v| !v.is_null())),
        "list.count_by" | "list.index_by" => {
            let field = text(1)?;
            let mut out = Map::new();
            for item in list(a(0)?, name)? {
                let key = item.get(&field).map(s).unwrap_or_default();
                if name == "list.count_by" {
                    let n = out.get(&key).and_then(Value::as_f64).unwrap_or(0.0) + 1.0;
                    out.insert(key, num(n));
                } else {
                    out.insert(key, item.clone());
                }
            }
            Value::Object(out)
        }

        // ── math ──
        "math.round" => {
            let p = 10f64.powi(opt_num(1, 0.0)? as i32);
            num((number(0)? * p).round() / p)
        }
        "math.floor" => num(number(0)?.floor()),
        "math.ceil" => num(number(0)?.ceil()),
        "math.abs" => num(number(0)?.abs()),
        "math.sqrt" => num(number(0)?.sqrt()),
        "math.pow" => num(number(0)?.powf(number(1)?)),
        "math.log" => num(match args.get(1) {
            Some(base) => number(0)?.log(f(base, name)?),
            None => number(0)?.ln(),
        }),
        "math.exp" => num(number(0)?.exp()),
        "math.clamp" => num(number(0)?.clamp(number(1)?, number(2)?)),
        "math.pi" => num(std::f64::consts::PI),
        "math.random" => num(random_unit()),
        "math.random_int" => {
            let (lo, hi) = (number(0)?.ceil(), number(1)?.floor());
            if hi < lo {
                return Err("math.random_int: empty range".into());
            }
            num(lo + (random_unit() * (hi - lo + 1.0)).floor())
        }

        // ── stats ──
        "stats.mean" => {
            let xs = nums(a(0)?, name)?;
            if xs.is_empty() {
                Value::Null
            } else {
                num(xs.iter().sum::<f64>() / xs.len() as f64)
            }
        }
        "stats.median" | "stats.percentile" => {
            let mut xs = nums(a(0)?, name)?;
            if xs.is_empty() {
                return Ok(Value::Null);
            }
            xs.sort_by(f64::total_cmp);
            let p = if name == "stats.median" {
                50.0
            } else {
                number(1)?.clamp(0.0, 100.0)
            };
            let rank = p / 100.0 * (xs.len() - 1) as f64;
            let (lo, hi) = (rank.floor() as usize, rank.ceil() as usize);
            num(xs[lo] + (xs[hi] - xs[lo]) * (rank - lo as f64))
        }
        "stats.mode" => {
            let items = list(a(0)?, name)?;
            let mut counts: Vec<(String, usize, &Value)> = Vec::new();
            for v in items {
                let k = canonical(v);
                match counts.iter_mut().find(|c| c.0 == k) {
                    Some(c) => c.1 += 1,
                    None => counts.push((k, 1, v)),
                }
            }
            counts
                .iter()
                .max_by_key(|c| c.1)
                .map(|c| c.2.clone())
                .unwrap_or(Value::Null)
        }
        "stats.variance" | "stats.stdev" => {
            let xs = nums(a(0)?, name)?;
            if xs.len() < 2 {
                return Ok(Value::Null);
            }
            let mean = xs.iter().sum::<f64>() / xs.len() as f64;
            let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (xs.len() - 1) as f64;
            num(if name == "stats.stdev" {
                var.sqrt()
            } else {
                var
            })
        }
        "stats.normalize" => {
            let xs = nums(a(0)?, name)?;
            let (lo, hi) = xs
                .iter()
                .fold((f64::MAX, f64::MIN), |(l, h), x| (l.min(*x), h.max(*x)));
            Value::Array(
                xs.iter()
                    .map(|x| num(if hi > lo { (x - lo) / (hi - lo) } else { 0.0 }))
                    .collect(),
            )
        }

        // ── time ──
        "time.now" => Value::String(iso(now_secs(), false)),
        "time.today" => Value::String(iso(now_secs(), true)),
        "time.unix" => num(match args.first() {
            Some(v) => to_secs(v)?,
            None => now_secs().floor(),
        }),
        "time.parse" => num(to_secs(a(0)?)?),
        "time.format" => {
            let style = args.get(1).map(s).unwrap_or_else(|| "datetime".into());
            Value::String(iso(to_secs(a(0)?)?, style == "date"))
        }
        "time.add" => {
            let v = a(0)?;
            let secs = to_secs(v)?
                + number(1)? * unit_secs(&args.get(2).map(s).unwrap_or_else(|| "days".into()))?;
            Value::String(iso(secs, is_date_only(v)))
        }
        "time.diff" => {
            let unit = unit_secs(&args.get(2).map(s).unwrap_or_else(|| "days".into()))?;
            num((to_secs(a(1)?)? - to_secs(a(0)?)?) / unit)
        }
        "time.weekday" => {
            let days = (to_secs(a(0)?)? / 86400.0).floor() as i64;
            let names = [
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
                "Monday",
                "Tuesday",
                "Wednesday",
            ];
            Value::String(names[days.rem_euclid(7) as usize].into())
        }
        "time.start_of" => {
            let v = a(0)?;
            let secs = to_secs(v)?;
            let days = (secs / 86400.0).floor() as i64;
            let (y, m, _) = civil_from_days(days);
            let start = match text(1)?.as_str() {
                "day" => days,
                "week" => days - (days + 3).rem_euclid(7),
                "month" => days_from_civil(y, m, 1),
                "year" => days_from_civil(y, 1, 1),
                other => {
                    return Err(format!(
                        "time.start_of: unknown period '{other}' (day, week, month, year)"
                    ))
                }
            };
            Value::String(iso((start * 86400) as f64, true))
        }

        // ── json / csv ──
        "json.parse" => serde_json::from_str(&text(0)?).map_err(|e| format!("json.parse: {e}"))?,
        "json.text" => Value::String(a(0)?.to_string()),
        "json.pretty" => Value::String(serde_json::to_string_pretty(a(0)?).unwrap_or_default()),
        "csv.parse" => {
            let rows = csv_rows(&text(0)?);
            let Some((header, body)) = rows.split_first() else {
                return Ok(Value::Array(vec![]));
            };
            Value::Array(
                body.iter()
                    .map(|r| {
                        Value::Object(
                            header
                                .iter()
                                .enumerate()
                                .map(|(i, h)| {
                                    (
                                        h.trim().to_string(),
                                        cell_value(r.get(i).map(String::as_str).unwrap_or("")),
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect(),
            )
        }
        "csv.text" => {
            let rows = list(a(0)?, name)?;
            let header: Vec<String> = match (args.get(1), rows.first()) {
                (Some(cols), _) => list(cols, name)?.iter().map(s).collect(),
                (None, Some(Value::Object(o))) => o.keys().cloned().collect(),
                _ => vec![],
            };
            let mut out = header
                .iter()
                .map(|h| csv_cell(&Value::String(h.clone())))
                .collect::<Vec<_>>()
                .join(",")
                + "\n";
            for r in rows {
                out += &header
                    .iter()
                    .map(|h| csv_cell(r.get(h).unwrap_or(&Value::Null)))
                    .collect::<Vec<_>>()
                    .join(",");
                out.push('\n');
            }
            Value::String(out)
        }

        // ── crypto / encode ──
        "crypto.sha256" => Value::String(hex(&Sha256::digest(text(0)?.as_bytes()))),
        "crypto.hmac_sha256" => {
            let mut mac =
                Hmac::<Sha256>::new_from_slice(text(0)?.as_bytes()).map_err(|e| e.to_string())?;
            mac.update(text(1)?.as_bytes());
            Value::String(hex(&mac.finalize().into_bytes()))
        }
        "crypto.uuid" => {
            let mut b = random_bytes::<16>();
            b[6] = (b[6] & 0x0f) | 0x40;
            b[8] = (b[8] & 0x3f) | 0x80;
            let h = hex(&b);
            Value::String(format!(
                "{}-{}-{}-{}-{}",
                &h[0..8],
                &h[8..12],
                &h[12..16],
                &h[16..20],
                &h[20..32]
            ))
        }
        "crypto.random_hex" => {
            let n = (opt_num(0, 16.0)? as usize).clamp(1, 1024);
            let mut b = vec![0u8; n];
            fill_random(&mut b);
            Value::String(hex(&b))
        }
        "crypto.equal" => {
            // Constant-time comparison, for tokens and signatures.
            let (x, y) = (text(0)?, text(1)?);
            Value::Bool(
                x.len() == y.len()
                    && x.bytes()
                        .zip(y.bytes())
                        .fold(0u8, |acc, (p, q)| acc | (p ^ q))
                        == 0,
            )
        }
        "encode.base64" => {
            Value::String(base64::engine::general_purpose::STANDARD.encode(text(0)?))
        }
        "encode.unbase64" => Value::String(
            String::from_utf8(
                base64::engine::general_purpose::STANDARD
                    .decode(text(0)?.trim())
                    .map_err(|e| format!("encode.unbase64: {e}"))?,
            )
            .map_err(|_| "encode.unbase64: result is not text")?,
        ),
        "encode.url" => Value::String(
            text(0)?
                .bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect(),
        ),
        "encode.unurl" => Value::String(crate::web::url_decode(&text(0)?)),
        "encode.hex" => Value::String(hex(text(0)?.as_bytes())),

        // ── schema / template ──
        "schema.check" | "schema.valid" => {
            let mut errors = Vec::new();
            schema_errors(a(0)?, a(1)?, "", &mut errors);
            if name == "schema.valid" {
                Value::Bool(errors.is_empty())
            } else {
                Value::from(errors)
            }
        }
        "template.render" => {
            let data = args.get(1).cloned().unwrap_or(Value::Null);
            Value::String(render(&text(0)?, &mut vec![&data])?)
        }

        // ── environment / reflection ──
        "env" => {
            static DOTENV: OnceLock<Map<String, Value>> = OnceLock::new();
            let dotenv = DOTENV.get_or_init(|| {
                std::fs::read_to_string(".env")
                    .unwrap_or_default()
                    .lines()
                    .filter_map(|l| {
                        let l = l.trim();
                        if l.starts_with('#') {
                            return None;
                        }
                        let (k, v) = l.split_once('=')?;
                        Some((
                            k.trim().to_string(),
                            Value::String(v.trim().trim_matches('"').to_string()),
                        ))
                    })
                    .collect()
            });
            let key = text(0)?;
            std::env::var(&key)
                .ok()
                .map(Value::String)
                .or_else(|| dotenv.get(&key).cloned())
                .or_else(|| args.get(1).cloned())
                .unwrap_or(Value::Null)
        }
        "type" => Value::String(
            match a(0)? {
                Value::Null => "none",
                Value::Bool(_) => "bool",
                Value::Number(_) => "number",
                Value::String(_) => "text",
                Value::Array(_) => "list",
                Value::Object(_) => "object",
            }
            .into(),
        ),
        other => {
            let close: Vec<&&str> = NAMES
                .iter()
                .filter(|n| {
                    n.ends_with(&format!(".{other}")) || n.starts_with(&format!("{other}."))
                })
                .take(4)
                .collect();
            return Err(if close.is_empty() {
                format!("unknown function '{other}' (see docs/STDLIB.md)")
            } else {
                format!(
                    "unknown function '{other}'; did you mean {}?",
                    close
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
        }
    })
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
