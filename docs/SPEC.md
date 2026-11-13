# Lugh Language Specification — v0.5

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

A program may declare several goals; they run in order. Lugh Core also accepts
`test "<name>"`, `every <n>s|m|h|d`, `route`, `page` and `auth token env "<VAR>"`
at the top level (see [STDLIB](STDLIB.md) and the Web section below).

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
| `expect <cond>` | In a `test` block: fail the test if false. |
| `stop "<msg>"` | End the goal cleanly. |

## Expressions

| Form | Example |
| --- | --- |
| Literals | `42`, `-3.5`, `"text"`, `true`, `false`, `none`, `[1, 2]`, `{airline: "Etihad", price: 41800}` |
| Field access on any value | `list.count_by(xs, "a").Etihad`, `schema.check(x, s).0` |
| Function call | `upper(s)`, `stats.median(xs)` (see [STDLIB](STDLIB.md)) |
| Collections | `filter x in xs where …`, `map x in xs to …`, `sort xs by f [desc]`, `group xs by f` |
| Field access | `best.price`, `fares.0.airline` |
| Operators | `+ - * /`, `< > <= >= == !=`, `and or not` (`and`/`or` return an operand, so `n or 0` gives a default) |
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

## Web: routes and pages (Lugh Core, v0.4)

A program may declare `route` and `page` blocks alongside goals. Run it with
`lugh program.lugh --serve 8080` (add `--public` to listen on all interfaces).

### Routes (backend)

```
route GET "/api/fares/:airline"
  let fares = fs.read "fares.json" as json
  each f in fares
    when f.airline == request.params.airline
      respond f
  respond 404 "No fare for {request.params.airline}"
```

| Item | Description |
| --- | --- |
| `route <METHOD> "<path>"` | `GET`, `POST`, `PUT`, `PATCH`, `DELETE`. Segments starting with `:` are parameters. |
| `request` | `{method, path, params, query, body}`. JSON bodies are parsed; form bodies become objects; numeric strings become numbers. |
| `respond [status] <value>` | Ends the request. Text is sent as `text/plain`, everything else as JSON. Default status 200. |
| `ensure <cond> else respond <status> <value>` | Validation in one line. |
| Falling off the end | Responds 200 with the value of the last statement. |
| `ask human` | Waits (up to 5 minutes) for a decision in the approvals inbox. No decision counts as a rejection. |

Status codes for other outcomes: `stop` → 422, policy denial → 403, budget
exceeded → 429, failure → 500. Each request gets a fresh budget; memory,
learned route statistics and the audit log are shared.

### Pages (frontend)

```
page "/" title "Fare desk"
  text "Live fares from three sources."
  stat from "/api/best" field price label "Lowest fare (INR)"
  table from "/api/fares" columns airline, depart, price
  form to "/api/book" fields airline, price submit "Request booking"
```

| Element | Renders |
| --- | --- |
| `heading "…"` / `text "…"` | A heading / a paragraph. |
| `link "…" to "/path"` | A link. |
| `table from "/api" [columns a, b]` | A table of the JSON list the route returns. |
| `list from "/api" [show field]` | A bulleted list. |
| `stat from "/api" [field f] label "…"` | One large number with a label. |
| `form to "/api" fields a, b [submit "…"]` | A form that POSTs JSON and shows the response. |
| `button "…" calls "/api"` | A button that POSTs and shows the response. |
| `approvals` / `audit` | The pending-approvals list / recent activity. |

Pages are rendered on the server to plain HTML with a ~3 KB script: no build
step, no client framework, light and dark themes.

### Built in

| Path | Purpose |
| --- | --- |
| `/_lugh/inbox` | Approvals inbox and live activity. |
| `GET /_lugh/api/approvals` | Pending approvals as JSON. |
| `POST /_lugh/api/approvals/:id` | `{"approve": true|false}`. |
| `GET /_lugh/api/audit` | The 30 most recent audit records. |

The server binds to `127.0.0.1` by default. It doesn't yet provide
authentication or TLS: put it behind a reverse proxy with auth before exposing
it to a network.
