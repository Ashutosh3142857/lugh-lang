# Lugh

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
| Install | Python 3.9+ | One 2.4 MB binary, libc only |
| Adapters | http, fs, shell, browser, llm | http, fs, shell, llm |
| Web (routes, pages, inbox) | — | Yes |

Both runtimes produce identical results on the bundled examples.

### Benchmarks

Measured with `python bench/gen.py && python bench/run.py` (Linux x86-64, mean of
repeated runs; peak memory of the measured process alone):

| Workload | Python | Lugh Core | Speed-up | Memory |
| --- | --- | --- | --- | --- |
| Startup (`bench/hello.lugh`) | 80 ms, 22.3 MB | 2.9 ms, 4.3 MB | 27.7× | 5.2× less |
| 1,000,000 relational comparisons + attention over 3,000 emails (`bench/heavy.lugh`) | 2,079 ms, 26.2 MB | 271 ms, 13.7 MB | 7.7× | 1.9× less |

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
- [Lugh Core](core/README.md): the native Rust runtime

## Roadmap

1. **Learn (v0.2)** ✓ — research-derived primitives, adaptive routing, evaluation
2. **Native (v0.3)** ✓ — Lugh Core: Rust runtime, compile-once execution, benchmarks
3. **Full stack (v0.4)** ✓ — routes, pages, approvals inbox, end-to-end tests
4. **Compile (v0.5)** — bytecode VM, field access compiled to fixed slots, WebAssembly build
5. **Connect (v0.6)** — MCP client in Core, editor syntax support, multi-agent message passing
6. **Reach (v0.7)** — desktop app adapter, screen fallback, spend metering
7. **Trust (v1.0)** — authentication and TLS for served apps, signed audit logs, shareable policy packs, automatic fallback ladder
8. **Platform** — hosted runtime, adapter marketplace, team approvals

## License

[MIT](LICENSE) © 2026 Ashutosh Pandey
