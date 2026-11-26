---
title: Stop Claude Code or Cursor from running DROP TABLE through an MCP server
description: An AI coding agent with a Postgres or Supabase MCP server can run any SQL it decides to. Here is how to let it read freely while DROP, TRUNCATE and DELETE never reach the database.
slug: stop-ai-agent-dropping-database
---

# Stop Claude Code or Cursor from running `DROP TABLE` through an MCP server

Connect a database MCP server to Claude Code, Cursor or VS Code and the agent gets a tool like `execute_sql` or `query`. That tool runs whatever SQL the model writes. Most of the time it's a harmless `SELECT`. Sometimes it's `DELETE FROM users` because the model misread a task, followed an instruction hidden in a web page, or decided a table looked like test data.

Client approval prompts help, but people click "always allow" after the tenth harmless query. After that nothing stands between the model and your data.

## The fix: put a policy in front of the server

[Lugh guard](https://github.com/Ashutosh3142857/lugh-lang) sits between your AI client and the MCP server. It reads every tool call, decides what it does, and only then lets it through. For SQL it looks at the statement itself:

| The agent sends | What happens |
|---|---|
| `SELECT`, `WITH … SELECT`, `SHOW`, `EXPLAIN` | runs |
| `DROP`, `TRUNCATE`, `DELETE`, `REVOKE` (anywhere in the statement, including inside a CTE) | refused; the server never receives it |
| `INSERT`, `UPDATE`, `ALTER`, `CREATE`, `GRANT`, … | your client asks you first |
| anything it can't classify | asks you first |

## Set it up in one command

```bash
npx lugh-lang protect
```

`protect` finds the MCP servers in your Claude Code, Claude Desktop, Cursor, VS Code and Windsurf configs, shows you what it will change, and wraps each local server. A Postgres server gets the `postgres` policy, Supabase gets `supabase`. It keeps a backup next to each file, and `npx lugh-lang protect --undo` puts everything back.

Your config entry ends up looking like this:

```json
"postgres": {
  "command": "npx",
  "args": ["-y", "lugh-lang", "guard", "--policy", "postgres", "--",
           "uvx", "postgres-mcp", "--access-mode=unrestricted"]
}
```

Restart the client. Nothing else changes: same server, same tools, same connection string.

## What the agent sees

When the model tries `DROP TABLE orders`, the call comes back as an error saying the policy denied it. The model usually adapts and explains that it can't do that. The database never saw the statement, so there's nothing to roll back.

Every decision is appended to `~/.lugh/guard-audit.jsonl`, one JSON line per call, so you can see afterwards what the agent tried.

## Write your own rules

The bundled policy is a short text file. Copy it and change what you need:

```lugh
agent PostgresGuard
  budget calls 200
  policy
    allow read
    ask human before write, execute, submit
    deny delete
  tools
    "execute_sql" when sql matches "\b(drop|truncate|delete|revoke)\b" is delete
    "execute_sql" when sql matches "^\s*(select|with|show|explain)\b" is read
    "execute_sql" is execute
```

Then point guard at it: `--policy ./my-db.lugh`.

## Limits worth knowing

Pattern matching on SQL is a strong speed bump, not a SQL parser. It errs on the side of asking: a column named `comment` or `set` in a `SELECT` will trigger a prompt. For anything that matters, also connect with a database role that can't write where writes aren't needed. Guard and a read-only role together cover both the honest mistake and the clever one.
