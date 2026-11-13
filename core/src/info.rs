//! Value helpers and information-theoretic operators (MDL / Kolmogorov).

use flate2::{write::ZlibEncoder, Compression};
use serde_json::Value;
use std::collections::HashSet;
use std::io::Write;

/// Text form of a value: strings as-is, numbers without a trailing ".0", everything else as JSON.
pub fn txt(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        Value::Null => "none".into(),
        other => canonical(other),
    }
}

/// Canonical JSON text: sorted keys, ", " and ": " separators. Matches the Python
/// runtime byte for byte, so compression-based scores agree across runtimes.
pub fn canonical(v: &Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &Value, out: &mut String) {
    match v {
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_canonical(x, out);
            }
            out.push(']');
        }
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&Value::String(k.clone()).to_string());
                out.push_str(": ");
                write_canonical(&o[k], out);
            }
            out.push('}');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// Description length in bytes: a computable upper bound on Kolmogorov complexity.
pub fn k(s: &str) -> usize {
    let mut e = ZlibEncoder::new(Vec::with_capacity(s.len() / 2 + 16), Compression::best());
    let _ = e.write_all(s.as_bytes());
    e.finish().map(|b| b.len()).unwrap_or(s.len())
}

/// Normalized compression distance: 0 = same information, ~1 = unrelated.
pub fn ncd(a: &str, b: &str) -> f64 {
    let (ka, kb) = (k(a), k(b));
    let kab = k(&format!("{a} {b}"));
    (kab as f64 - ka.min(kb) as f64) / (ka.max(kb).max(1) as f64)
}

pub fn tokens(s: &str) -> HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// Attention-style score: token overlap blended with compression similarity.
pub fn relevance(query: &str, item: &str) -> f64 {
    let q = tokens(query);
    let overlap = q.intersection(&tokens(item)).count() as f64 / q.len().max(1) as f64;
    0.7 * overlap + 0.3 * (1.0 - ncd(query, item).min(1.0))
}

pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

pub fn val_eq(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// Splits text into sentences / lines, keeping each as a verbatim slice.
pub fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let b = text.as_bytes();
    for i in 0..b.len() {
        let end_here = b[i] == b'\n'
            || ((b[i] == b'.' || b[i] == b'!' || b[i] == b'?')
                && (i + 1 == b.len() || b[i + 1].is_ascii_whitespace()));
        if end_here {
            let stop = if b[i] == b'\n' { i } else { i + 1 };
            let s = text[start..stop].trim();
            if !s.is_empty() {
                out.push(s);
            }
            start = i + 1;
        }
    }
    let s = text[start.min(text.len())..].trim();
    if !s.is_empty() {
        out.push(s);
    }
    out
}
