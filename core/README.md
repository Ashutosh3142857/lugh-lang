# Lugh Core

The native runtime for Lugh, written in Rust. It runs the same `.lugh` programs as
the Python reference runtime and gives the same results, as one self-contained binary.

```bash
cd core && cargo build --release
./target/release/lugh ../examples/research_desk.lugh --yes --eval
```

Flags: `--yes` (auto-approve), `--check` (static policy report), `--eval`
(weighted score), `--quiet`, `--test` (run `test` blocks), `--serve [port]`
(run routes, pages and scheduled jobs; add `--public` to bind all interfaces).

Tests: `cargo test --release` runs the example programs, the standard-library
suite written in Lugh, and drives the web server end to end: an approval
through the inbox, token auth and scheduled jobs.

## Design

| Stage | What happens |
| --- | --- |
| `parser.rs` | Source → indentation tree → statement and expression trees, **once**, before anything runs. Variable paths such as `fare.price` are split at compile time. |
| `runtime.rs` | Executes the trees. Every effect passes the policy gate and budget meter; memory, learned route statistics and the audit log are thread-safe. `together`, `consensus` and `each … parallel N` run on real OS threads. |
| `adapters.rs` | `http`, `fs`, `shell`, `db` (bundled SQLite, bound parameters) and the `think` LLM call. |
| `web.rs` | Backend and frontend frameworks: a small HTTP/1.1 server on `std::net` (thread per connection), route matching with `:params`, the server-side page renderer with a ~3 KB script, and the approvals inbox. No web dependencies. |
| `stdlib.rs` | The standard library: text, list, math, stats, time, json, csv, crypto, encode, schema, template, env. Pure functions; unknown names are rejected at compile time. |
| `info.rs` | Canonical JSON text and zlib description length, byte-compatible with the Python runtime, so `attend`, `novelty` and `simplest` give identical answers in both. |

Engineering choices:

- **Rust, not C.** The same machine-code speed, with memory safety. That matters for a
  runtime that acts on real systems for people.
- **No per-pair allocation** in `pairs`: items are moved into a single binding slot
  and back.
- **Zero-copy comparisons:** `a.x == b.y` borrows both values instead of cloning them.
- **Release profile:** fat LTO, one codegen unit, `panic = "abort"`, stripped symbols.

## Status

Supports the full v0.2 language, except the `browser`, `mcp` and `app` adapters;
use the Python runtime for those. Programs that `use` them are rejected with a
clear message.
