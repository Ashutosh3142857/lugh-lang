# Lugh Language Specification — v0.2

## Program structure

A program is one `agent` block. Indentation (spaces) defines blocks. Comments start with `#`.

```
agent <Name>
  use <adapter>, <adapter>, ...
  budget <limit> <value>, ...
  policy
    allow <effect>, ...
    ask human before <effect>, ...
    deny <effect>, ...
  goal "<objective>"
    <statements>
```

A program may declare several goals; they run in order.

## Header

| Keyword | Description |
| --- | --- |
| `agent` | Names the agent. Every audit record carries this name. |
| `use` | Adapters the agent may call. Unlisted adapters are unreachable. |
| `budget` | `time` (`s`, `m`, `h`), `calls` (number of effects). Defaults: 120 s, 25 calls. `spend` and `tokens` are recorded in v0.1, metered from v0.5. |
| `policy` | Assigns effect types to `allow`, `ask human before` or `deny`. Unlisted effects require approval. |
| `goal` | A plain-language objective followed by its steps. |

## Statements

| Statement | Description |
| --- | --- |
| `let x = <expr>` | Bind a value. |
| `let x = attempt` / `otherwise` | Run the indented block; on failure run the `otherwise` block. |
| `let x = together` | Run each indented line in parallel; returns a list. |
| `ensure <cond> else stop "<msg>"` | Verify; stop the goal if false. |
| `ensure <cond> else ask human "<msg>"` | Verify; ask whether to continue if false. |
| `when <cond>` / `otherwise` | Conditional. |
| `each x in <list>` | Loop. |
| `let x = attempt auto` | Each indented line is an alternative route, tried best-first by learned score. |
| `let x = consensus [min K]` | Each indented line is an independent source; accept a value that at least K (default: majority) agree on. |
| `refine x with <expr> until <cond> [rounds N]` | Re-derive `x` until `<cond>` holds; if it never does, `x` keeps its original value. Default 3 rounds. |
| `each x in <list> parallel N` | Loop with N concurrent workers. |
| `ask human "<question>" -> x` | Pause for approval; binds `true`/`false`. |
| `remember "<key>" = <expr> [keep <n>s/m/h/d]` | Persist a value across runs, optionally expiring. |
| `send <channel> <target> <message>` | Deliver a message (effect: `send`). Channels: `console`, `file`; others via MCP from v0.2. |
| `log <expr>` | Record a note. |
| `stop "<msg>"` | End the goal cleanly. |

## Expressions

| Form | Example |
| --- | --- |
| Literals | `42`, `3.5`, `"text"`, `true`, `false`, `none`, `[1, 2]` |
| Field access | `best.price`, `fares.0.airline` |
| Operators | `+ - * /`, `< > <= >= == !=`, `and or not` |
| Adapter call | `http.get "https://…" as json` |
| Conversion | `as json`, `as text`, `as lines`, `as number` |
| Selection | `pick lowest price from fares`, `pick highest score from items` |
| Count | `count items` |
| Memory | `recall "key"`, `recall like "query"` |
| Reasoning | `think "Summarise this" with data [samples N]` |
| Relevance | `attend to items by "query" [top N]` |
| Grounded extraction | `point to "query" in text` (returns a verbatim span or `none`) |
| Relations | `pairs a from X, b from Y where <cond>` |
| Simplicity | `simplest of list` (shortest compressed description) |
| Novelty | `novelty x [against list]` (0 = already known, ~1 = new) |

Strings interpolate `{name.field}`.

## Effects

`read`, `write`, `submit`, `send`, `spend`, `execute`, `delete`. Each adapter verb declares exactly one effect.

| Adapter | Verbs → effect |
| --- | --- |
| `http` | `get` → read, `post` → send, `put` → write, `delete` → delete |
| `fs` | `read`, `list` → read; `write`, `append` → write; `delete` → delete |
| `shell` | `run` → execute |
| `browser` | `open`, `text`, `screenshot` → read; `type` → write; `click` → submit |
| `mcp` | `query` → read, `call` → send |
| `app` | `open`, `read` → read; `type` → write; `click` → submit |

## Execution model

1. Parse lines into a block tree; parse each line into an expression tree.
2. Load the header; reject unknown adapters or effects before any action.
3. For each effect: check time budget → count against call budget → deny list → allow list → otherwise ask a human. Only then call the adapter.
4. Failures inside `attempt` run the `otherwise` branch. Policy denials and budget overruns always propagate.
5. Each goal ends as `done`, `stopped` or `failed`.

## Learning between runs

`lugh_stats.json` holds per-route moment estimates for `attempt auto`: first moment of success, second moment of latency, bias-corrected as in Adam. Score = m̂ / (√v̂ + 1).

## Evaluation

`--eval` prints each goal's outcome (done 1.0, stopped 0.5, failed 0.0) and a weighted score Σ wᵢvᵢ / Σ wᵢ with wᵢ = 2^(−K(goalᵢ)/256), where K is the compressed size of the goal's source.

## Audit log

`lugh_audit.jsonl` — one JSON object per line: `goal_start`, `effect`, `attend`, `point`, `recall_like`, `route_failed`, `consensus`, `vote`, `refine`, `effect_error`, `attempt_failed`, `human`, `denied`, `rejected`, `ensure_failed`, `remember`, `think`, `sent`, `log`, `goal_done`, `goal_stopped`, `goal_failed`. Each record carries a timestamp and the agent name.

## Memory

`lugh_memory.json` — key/value store written by `remember` and read by `recall`, persisted between runs.
