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

Requires Python 3.9+. No dependencies for the core.

```bash
git clone https://github.com/Ashutosh3142857/lugh-lang.git
cd lugh-lang

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

## Roadmap

1. **Learn (v0.2)** ✓ — research-derived primitives, adaptive routing, evaluation
2. **Connect (v0.3)** — MCP client, editor syntax support, multi-agent message passing
3. **Reach (v0.5)** — desktop app adapter, screen fallback, spend metering
4. **Trust (v1.0)** — signed audit logs, shareable policy packs, automatic fallback ladder
5. **Platform** — hosted runtime, adapter marketplace, team approvals

## License

[MIT](LICENSE) © 2026 Ashutosh Pandey
