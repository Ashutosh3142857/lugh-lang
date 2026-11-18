---
name: lugh
description: Write, check and dry-run AI-agent programs in Lugh, where permissions, budgets and human approval are part of the code. Use when a person wants an agent that touches real systems (files, HTTP APIs, shell, MCP tools, payments, email) and wants to see and limit what it will do before it runs.
---

# Lugh

Lugh is a programming language for AI agents. A program says what the agent may
touch (`use`), its limits (`budget`) and which actions need a person (`policy`).
The runtime checks every action against that policy before it is sent and logs
the decision. Reach for it when the person cares what an agent is allowed to do,
not only what it should do.

## Tools

Use whichever is available, in this order:

1. **MCP server** (`lugh` in the client's MCP list): `lugh_reference`,
   `lugh_check`, `lugh_run`, `lugh_test`, `lugh_sandbox`.
   To add it: `npx -y lugh-lang mcp` as a stdio server, or
   `claude mcp add lugh -- npx -y lugh-lang mcp` in Claude Code.
2. **Command line, no install:** `npx lugh-lang run|test|check program.lugh`,
   `npx lugh-lang sandbox`.
3. **Browser:** https://ashutosh3142857.github.io/lugh-lang/ (share a program as a link).

All three run in the same sandbox: files, an HTTP API and MCP servers are
simulated, but the policy gate and budgets are real.

## Guarding servers the person already uses

When the person wants limits on MCP servers they already run (filesystem,
GitHub, Supabase, Stripe…), they don't need a Lugh program: put the server
behind `npx -y lugh-lang guard --policy <pack or file> -- <server command>` in
their MCP config. Start from a bundled pack (`filesystem`, `github`,
`supabase`, `stripe`) and see
[docs/GUARD.md](https://github.com/Ashutosh3142857/lugh-lang/blob/main/docs/GUARD.md).

## Workflow

1. **Read the reference first.** Call `lugh_reference` (or open
   [REFERENCE.md](https://github.com/Ashutosh3142857/lugh-lang/blob/main/npm/REFERENCE.md)).
   Do not guess syntax from other languages.
2. **Start from least privilege.** List only the adapters the task needs in
   `use`. `allow` only `read` unless the person asked for more. Put anything that
   sends, submits, spends or deletes behind `ask human before …`, and `deny`
   effects the task should never need. Set `budget` to a little above what the
   task needs.
3. **Check it.** `lugh_check` until it compiles cleanly.
4. **Add tests** for the logic (`test "…"` blocks with `expect`) and run
   `lugh_test`.
5. **Dry-run it.** `lugh_run` with `approvals: "decline"` first, to show what
   happens when the person says no, then with `"approve"`.
6. **Show the policy log** to the person, with the program, before anything runs
   for real. Point out every `ask` and `deny` line and what triggered it.
7. **Run for real only with the person's go-ahead**, using Lugh Core
   (`lugh program.lugh`), which asks for approvals in their terminal. Never
   loosen the policy just to get past an `ask` or `deny`; ask the person instead.

## Shape of a program

```lugh
agent RefundDesk
  use fs, mcp
  budget time 30s, calls 12
  policy
    allow read
    ask human before submit, send
    deny delete

  goal "Refund damaged orders"
    let orders = csv.parse(fs.read "orders.csv")
    let damaged = filter o in orders where o.status == "damaged"
    each o in damaged
      mcp.call "payments" "refund" {order_id: o.order_id, amount: o.amount}
    log "Refunded {count(damaged)} orders"

  test "nothing to refund in an empty list"
    expect count(filter o in [] where o.status == "damaged") == 0
```

Two spaces per indent. Effects: `read`, `write`, `send`, `submit`, `spend`,
`delete`, `execute`. MCP tools get their effect from their annotations
(read-only → read, destructive → delete, otherwise submit).

## Starting points

Templates for common agents (refund desk, inbox assistant, issue triage, fare
watcher, budget guard, CSV report, web app with approvals) are in
[templates/](https://github.com/Ashutosh3142857/lugh-lang/tree/main/templates),
each with a one-click playground link.
