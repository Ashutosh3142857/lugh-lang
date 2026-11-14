# Lugh reference (for writing programs)

A Lugh program is indented by two spaces and starts with `agent <Name>`.

```lugh
agent RefundDesk
  use fs, mcp                      # adapters the agent may use; anything else is unreachable
  budget time 30s, calls 12        # limits enforced by the runtime
  policy
    allow read                     # runs without asking
    ask human before submit, send  # pauses for a yes/no
    deny delete                    # refused before the request is sent

  goal "Refund damaged orders"     # a plain-language objective, then its steps
    let orders = csv.parse(fs.read "orders.csv")
    let damaged = filter o in orders where o.status == "damaged"
    each o in damaged
      mcp.call "payments" "refund" {order_id: o.order_id, amount: o.amount}
    log "Refunded {count(damaged)} orders"

  test "nothing to refund in an empty list"
    expect count(filter o in [] where o.status == "damaged") == 0
```

## Effects and the policy gate

Every adapter call has one effect: `read`, `write`, `send`, `submit`, `spend`, `delete` or `execute`.
Before each call the runtime checks the time budget, the call budget, `deny`, `allow`,
and otherwise asks a human. Effects not listed in `policy` need approval. A denial or
budget overrun cannot be caught by `attempt`.

| Adapter | Verbs → effect |
| --- | --- |
| `fs` | `read`, `list` → read; `write`, `append` → write; `delete` → delete |
| `http` | `get` → read, `post` → send, `put` → write, `delete` → delete |
| `shell` | `run` → execute |
| `db` | `query` → read, `exec` → write (SQLite) |
| `mcp` | `tools` → read; `call` → from the tool's MCP annotations: read-only → read, destructive → delete, otherwise submit |
| `llm` | `think` |

Adapter calls: `fs.read "path" as json`, `http.get "https://…" as json`,
`mcp.call "server" "tool" {arg: value}`, `mcp.tools "server"`, `shell.run "cmd"`.
Conversions: `as json`, `as text`, `as lines`, `as number`.

## Statements

| Statement | Meaning |
| --- | --- |
| `let x = <expr>` | bind a value |
| `when <cond>` / `otherwise` | conditional (each block indented) |
| `each x in <list>` [`parallel N`] | loop |
| `let x = attempt` … `otherwise` … | fallback when the block fails |
| `let x = attempt auto` | each indented line is a route, tried best-first by learned success |
| `let x = together` | run indented lines in parallel, returns a list |
| `let x = consensus [min K]` | accept a value most indented sources agree on |
| `refine x with <expr> until <cond> [rounds N]` | re-derive until a check passes |
| `ensure <cond> else stop "msg"` / `else ask human "msg"` | verify |
| `ask human "question" -> ok` | approval, binds true/false |
| `remember "key" = <expr> [keep 7d]`, `recall "key"`, `recall like "query"` | memory |
| `send console "#channel" "message"` | deliver a message (effect: send) |
| `log <expr>`, `stop "msg"`, `expect <cond>` (in tests) | output, end, assert |

## Expressions

Literals `42`, `"text"`, `true`, `none`, `[1, 2]`, `{a: 1}`; field access `x.a`, `xs.0`;
`+ - * / < > <= >= == != and or not` (`and`/`or` return an operand).
Collections: `filter x in xs where …`, `map x in xs to …`, `sort xs by f [desc]`,
`group xs by f`, `pick lowest price from xs`, `count xs`.
Reasoning: `attend to xs by "query" [top N]`, `point to "query" in text` (verbatim span
or `none`), `pairs a from X, b from Y where …`, `simplest of xs`,
`novelty x [against xs]`, `think "prompt" with data [samples N]`.

Strings interpolate pure expressions: `"Total {sum(xs)}"`. String literals and adapter
calls are not allowed inside `{}`; bind them with `let` first.

## Standard library (pure, no effects)

`sum avg min max round abs upper lower trim split join replace contains unique first
last range keys values now today uuid number`, plus modules `text.*` (regex `find`,
`find_all`, `sub`, `matches`), `list.*`, `math.*`, `stats.*` (`mean median stdev
percentile`), `time.*` (`add diff weekday start_of format parse`), `json.*`,
`csv.parse`/`csv.text`, `crypto.*`, `encode.*`, `schema.check`/`schema.valid`
(`{age: "int 18..120", email: "email"}`), `template.render`.
Full list: https://github.com/Ashutosh3142857/lugh-lang/blob/main/docs/STDLIB.md

## Web apps

Top-level `route GET "/api/x"` blocks (with `request`, `respond [status] value`) and
`page "/" title "…"` blocks (`heading`, `text`, `table from`, `stat from`, `form to`,
`button`, `approvals`). Serve with `lugh app.lugh --serve 8080`.

## Running

- Sandbox (no install): `npx lugh-lang run program.lugh` or the MCP tools `lugh_check`,
  `lugh_run`, `lugh_test`. Adapters act on a simulated world; see `lugh_sandbox`.
- For real: install Lugh Core, then `lugh program.lugh` (asks for approval in the
  terminal), `lugh program.lugh --check` (what it can touch), `lugh plan "<goal>"`.

Spec: https://github.com/Ashutosh3142857/lugh-lang/blob/main/docs/SPEC.md
