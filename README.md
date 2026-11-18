# Lugh

[![CI](https://github.com/Ashutosh3142857/lugh-lang/actions/workflows/ci.yml/badge.svg)](https://github.com/Ashutosh3142857/lugh-lang/actions/workflows/ci.yml)
[![Playground](https://img.shields.io/badge/try-playground-2f5d50)](https://ashutosh3142857.github.io/lugh-lang/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Glama score](https://glama.ai/mcp/servers/Ashutosh3142857/lugh-lang/badges/score.svg)](https://glama.ai/mcp/servers/Ashutosh3142857/lugh-lang)

**A programming language for AI agents that work through the infrastructure that already exists** — APIs, websites, shells, files, SaaS apps and desktop software.

Lugh is named for the Irish god *Samildánach*, "skilled in all the arts". A Lugh program declares what an agent may touch, how much it may spend, and when a human must approve — and the runtime enforces all of it on every single action.

```lugh
agent FlightScout
  use http, fs, shell, llm
  budget time 2m, calls 20
  policy
    allow read, write
    ask human before send, spend, submit
    deny delete

  goal "Find the cheapest Delhi to Dublin fare under 45000 INR"
    let fares = attempt
      http.get "https://api.flights.example/search?from=DEL&to=DUB" as json
    otherwise
      fs.read "examples/fares.json" as json

    let best = pick lowest price from fares
    ensure best.price < 45000 else stop "Nothing under budget"

    ask human "Book {best.airline} at INR {best.price}?" -> ok
    when ok
      send console "#travel" "Booking {best.airline}"
```

## Try it

- **In your browser:** the [Lugh Playground](https://ashutosh3142857.github.io/lugh-lang/) runs Lugh
  compiled to WebAssembly, with nothing to install.
- **From a terminal, no install:** `npx lugh-lang run program.lugh` runs a program in
  the same sandbox and prints a policy log (`test`, `check` and `sandbox` work too).
- **For AI agents, over MCP:** `npx lugh-lang mcp` is an
  [MCP server](npm/README.md), listed in the
  [MCP Registry](https://registry.modelcontextprotocol.io/v0.1/servers?search=io.github.Ashutosh3142857/lugh)
  as `io.github.Ashutosh3142857/lugh`. An agent can read the language reference,
  write a program, check it and dry-run it, then show you the policy log:
  ```json
  { "mcpServers": { "lugh": { "command": "npx", "args": ["-y", "lugh-lang", "mcp"] } } }
  ```

  One-click install:
  [![Install in Cursor](https://cursor.com/deeplink/mcp-install-dark.svg)](https://cursor.com/en/install-mcp?name=lugh&config=eyJjb21tYW5kIjoibnB4IiwiYXJncyI6WyIteSIsImx1Z2gtbGFuZyIsIm1jcCJdfQ%3D%3D)
  [![Install in VS Code](https://img.shields.io/badge/VS_Code-Install_Lugh_MCP-0098FF?logo=visualstudiocode&logoColor=white)](https://insiders.vscode.dev/redirect/mcp/install?name=lugh&config=%7B%22command%22%3A%22npx%22%2C%22args%22%3A%5B%22-y%22%2C%22lugh-lang%22%2C%22mcp%22%5D%7D)

  In Claude Code: `claude mcp add lugh -- npx -y lugh-lang mcp`

  It is also listed on Glama, mcprush and mcpservers.org; Glama and mcprush grade servers independently:

  [![Lugh MCP server on Glama](https://glama.ai/mcp/servers/Ashutosh3142857/lugh-lang/badges/card.svg)](https://glama.ai/mcp/servers/Ashutosh3142857/lugh-lang)
  [![Lugh MCP server on mcprush](https://mcprush.com/badge/lugh-mcp/card.svg)](https://mcprush.com/to/lugh-mcp?ref=badge)

  [![Listed on mcpservers.org](https://mcpservers.org/badge.svg)](https://mcpservers.org/servers/ashutosh3142857/lugh-lang)
- **Templates:** [templates/](templates/) has starting points for common agents (refund desk,
  inbox assistant, issue triage and more), each with a one-click playground link.
- **For coding agents:** the [Lugh skill](skills/lugh/SKILL.md) teaches Claude Code and other
  agents the write, check, dry-run and show-the-policy-log workflow.
- **Usage:** downloads, traffic and stars are recorded daily and charted at
  [ashutosh3142857.github.io/lugh-lang/stats.html](https://ashutosh3142857.github.io/lugh-lang/stats.html).
- **Locally:** `lugh playground` serves the same page on http://127.0.0.1:8000.
- **In VS Code:** the [extension](editors/vscode) adds highlighting, snippets,
  live error checking and run commands.

In the playground, adapters act on a simulated world: a few files, an HTTP API
and five MCP servers (notes, GitHub, payments, mail, a database), created fresh for every
run. Nothing real is touched, but every call still passes the policy gate, so
`allow`, `ask` and `deny` behave exactly as they do when installed, and each run
ends with a policy log. Choose whether approvals are answered yes or no, share a
program as a link, or run the same sandbox from a terminal or an agent with
`npx lugh-lang`. Every run has a 10-second
budget that also stops runaway computation.

## Why a new language

Agent frameworks keep the things that matter most — permissions, spending limits, human approval, recovery from failure, an audit trail — outside the code, in config files and prompts. Lugh makes them **syntax**, so they are checked before a program runs and recorded while it runs.

| Concern | In Lugh |
| --- | --- |
| What the agent may do | `policy` block, enforced by the runtime |
| Runaway cost or loops | `budget` block: time, calls, spend |
| Human approval | `ask human` is a built-in statement |
| Brittle integrations | `attempt … otherwise` fallback chains |
| Proof of what happened | Every effect written to an audit log |

## Quick start

Python runtime: Python 3.9+, no dependencies. Native runtime: Rust 1.70+ to build.

```bash
git clone https://github.com/Ashutosh3142857/lugh-lang.git
cd lugh-lang

# Native runtime (Rust)
(cd core && cargo build --release)
./core/target/release/lugh examples/research_desk.lugh --yes --eval

# Reference runtime (Python)

python lugh.py examples/flight_scout.lugh --check   # list every action and its policy
python lugh.py examples/flight_scout.lugh           # run, approving actions as you go
python lugh.py examples/flight_scout.lugh --yes     # run with auto-approval
```

Optional:

- `pip install playwright && playwright install chromium` enables the `browser` adapter.
- Set `ANTHROPIC_API_KEY` so `think` uses a real LLM instead of the local stub (`LUGH_MODEL` selects the model).

## What's new in v0.2

Primitives drawn from the foundational ML literature ([details](docs/RESEARCH.md)):

```lugh
let urgent   = attend to inbox by "invoice overdue" top 2      # attention
let deadline = point to "payment due by date" in mail.body      # grounded extraction
remember "deadline" = deadline keep 7d                          # gated memory
let found    = recall like "northwind deadline"                 # content-addressed memory
let matched  = pairs i from invoices, p from payments where i.amount == p.amount
let fare     = consensus                                        # ensemble agreement
  pick lowest price from fs.read "a.json" as json
  pick lowest price from fs.read "b.json" as json
let quote    = attempt auto                                     # learned route order
  http.get "https://api.example.com/quote" as json
  fs.read "cache.json" as json
refine note with think "Improve this" with fare until count note > 20 rounds 2
let plan     = simplest of plans                                # minimum description length
let fresh    = novelty fare                                     # compression novelty
each m in matched parallel 4                                    # pipelined loop
```

Run the full demo:

```bash
python lugh.py examples/research_desk.lugh --yes --eval
```

## Agents that write their own programs

```bash
lugh plan "Add a passport photos reminder and list my visa notes" --use mcp --run
```

An LLM writes the Lugh program, the compiler checks it (sending any error back
for the model to fix), and you see the program and a report of everything it
can touch before it runs. Generated programs run under **plan safety**: only
reads happen without asking you, and deletes are always blocked.

Lugh connects to any [MCP](https://modelcontextprotocol.io) server (GitHub,
Slack, Drive, Notion and thousands more) with the same config format as
Claude Desktop. Each tool's effect comes from its MCP annotations, and policy
decides before the call is sent. See [MCP and plan](docs/MCP.md).

```lugh
use mcp
policy
  allow read
  ask human before submit
  deny delete
...
mcp.call "notes" "add_note" {title: "Biometrics"}   # asks you first
mcp.call "notes" "delete_note" {title: "x"}         # blocked by policy, never sent
```

## Standard library

Lugh Core ships with what most projects pull in from the Python and JavaScript
ecosystems: strings and regex, collections, maths and statistics, dates, JSON
and CSV, hashing and UUIDs, schema validation, templates, SQLite, a test runner,
scheduled jobs and token auth. See the [standard library reference](docs/STDLIB.md).

```lugh
let cheap  = filter f in fares where f.price < 44000
let ranked = sort cheap by price
let errors = schema.check(request.body, {title: "text 2..80", amount: "number 0.01..1000000"})
db.exec "INSERT INTO expenses VALUES (?, ?, ?)" [uuid(), e.title, e.amount]
let report = csv.text(db.query "SELECT * FROM expenses")

test "only cheap fares"
  expect count(cheap) == 3
```

```bash
./core/target/release/lugh examples/stdlib_test.lugh --test   # 13 passed, 0 failed
```

[`examples/ledger.lugh`](examples/ledger.lugh) puts it together: an expense
tracker with a database, validation, token auth, an hourly job, CSV export and
a web UI, in about 50 lines.

## Full-stack apps: routes, pages and a human in the loop

One `.lugh` file can be an API, a web UI and an approval workflow
([example](examples/fare_desk.lugh), [reference](docs/SPEC.md#web-routes-and-pages-lugh-core-v04)):

```lugh
route POST "/api/book"
  let b = request.body
  ensure b.price < 45000 else respond 422 "Over budget"
  ask human "Book {b.airline} at INR {b.price}?" -> ok   # waits in the web inbox
  when ok
    respond 201 b
  otherwise
    respond 403 "Rejected"

page "/" title "Fare desk"
  stat from "/api/best" field price label "Lowest fare (INR)"
  table from "/api/fares" columns airline, depart, price
  form to "/api/book" fields airline, price submit "Request booking"
```

```bash
./core/target/release/lugh examples/fare_desk.lugh --serve 8080
# open http://127.0.0.1:8080 and the approvals inbox at /_lugh/inbox
```

`ask human` inside a route pauses that request until someone clicks Approve or
Reject in the inbox. Pages are server-rendered HTML (about 6 KB each, including
styles and script) with no build step.

## Two runtimes

| | Python reference (`lugh.py`) | Lugh Core (`core/`, Rust) |
| --- | --- | --- |
| Use it for | Prototyping, the browser adapter | Production, scale, embedding |
| Install | Python 3.9+ | One 5.9 MB binary (SQLite included), libc only |
| Adapters | http, fs, shell, browser, llm | http, fs, shell, db, mcp, llm |
| Web (routes, pages, inbox, auth, jobs) | — | Yes |
| Standard library, `test` blocks, `lugh plan` | Core language only | Yes |

Both runtimes produce identical results on the bundled examples.

### Benchmarks

Measured with `python bench/gen.py && python bench/run.py` (Linux x86-64, mean of
repeated runs; peak memory of the measured process alone):

| Workload | Python | Lugh Core | Speed-up | Memory |
| --- | --- | --- | --- | --- |
| Startup (`bench/hello.lugh`) | 79 ms, 22.3 MB | 2.9 ms, 4.8 MB | 27× | 4.6× less |
| 1,000,000 relational comparisons + attention over 3,000 emails (`bench/heavy.lugh`) | 1,925 ms, 26.2 MB | 321 ms, 14.9 MB | 6.0× | 1.8× less |

Numbers vary by machine. An agent's wall-clock time is usually dominated by
network and model latency; Core's advantage is cheap startup, a small footprint
and real parallelism when many agents run at once.

## Effects and policy

Every action belongs to one of seven effect types. Policy is written against these, so reviewing an agent means understanding seven words rather than every API it calls.

| Effect | Meaning | Default when not listed |
| --- | --- | --- |
| `read` | Observes, changes nothing | ask human |
| `write` | Changes local or draft state | ask human |
| `submit` | Commits an action in another system | ask human |
| `send` | Reaches another person | ask human |
| `spend` | Moves money | ask human |
| `execute` | Runs code or commands | ask human |
| `delete` | Irreversible removal | ask human |

Anything not explicitly allowed requires approval, so a missing policy line makes an agent more cautious, never less. Policy denials and budget overruns cannot be caught by `attempt`, so an agent cannot route around its limits.

## Adapters

| Adapter | Reaches | Status |
| --- | --- | --- |
| `http` | REST / JSON APIs | Working |
| `fs` | Local files | Working |
| `shell` | Installed CLI tools | Working |
| `browser` | Any website (Playwright) | Working |
| `llm` | Reasoning via `think` (with `samples N`) | Working |
| `mcp` | SaaS apps via Model Context Protocol | Planned (v0.3) |
| `app` | Desktop software via OS accessibility APIs | Planned (v0.5) |

An adapter is one class: a map of `verb → effect` and a `call(verb, args)` method.

## Documentation

- [Language specification](docs/SPEC.md)
- [From research to runtime](docs/RESEARCH.md): how 30 foundational papers map to Lugh
- [MCP and `lugh plan`](docs/MCP.md): connecting to MCP servers; letting a model write programs you approve
- [Standard library](docs/STDLIB.md): every module, mapped to its Python and JavaScript equivalents
- [Lugh Core](core/README.md): the native Rust runtime

## Roadmap

1. **Learn (v0.2)** ✓ — research-derived primitives, adaptive routing, evaluation
2. **Native (v0.3)** ✓ — Lugh Core: Rust runtime, compile-once execution, benchmarks
3. **Full stack (v0.4)** ✓ — routes, pages, approvals inbox, end-to-end tests
4. **Standard library (v0.5)** ✓ — 100+ functions, SQLite, tests, jobs, auth, object literals
5. **Connect (v0.6)** ✓ — MCP client, `lugh plan` with self-repair and plan safety
6. **Reach (v0.7)** ✓ — browser playground (WebAssembly), VS Code extension, multi-line lists and objects, CI
7. **Compile (v0.8)** — bytecode VM, field access compiled to fixed slots
8. **Ecosystem (v0.9)** — package manager, language server, multi-agent message passing
9. **Desktop (v0.10)** — desktop app adapter, screen fallback, spend metering
10. **Trust (v1.0)** — TLS, user accounts and roles for served apps, signed audit logs, shareable policy packs, automatic fallback ladder
11. **Platform** — hosted runtime, adapter marketplace, team approvals

## Support Lugh

Lugh is free and open source. If it's useful to you, you can support its
development:

| Network | Address |
| --- | --- |
| Bitcoin | `bc1q3w9542hnh4qykqnfsjg9dcq49fhm8dtuwtydhy` |
| Ethereum | `0x15b4939bcf8c8FA80c320566c4d9D49c9078e1C2` |

Please send each coin only on its own network.

Contributions of code, examples, adapters and bug reports are just as welcome.

## License

[MIT](LICENSE) © 2026 Ashutosh Pandey
