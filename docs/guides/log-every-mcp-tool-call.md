---
title: Log every MCP tool call your AI agent makes
description: Get a plain JSONL audit log of every tool call Claude Code, Cursor or VS Code makes through MCP, including the ones that were refused or that you approved.
slug: log-every-mcp-tool-call
---

# Log every MCP tool call your AI agent makes

After an agent session it's surprisingly hard to answer "what did it actually do?". The chat shows what the model said. The tool calls are folded away, scattered across clients, and gone when the session ends.

[Lugh guard](https://github.com/Ashutosh3142857/lugh-lang) sits in front of each MCP server and writes one JSON line per decision to a single file, for every client and every server.

## Set it up

```bash
npx lugh-lang protect
```

That wraps every local MCP server in your Claude Code, Claude Desktop, Cursor, VS Code and Windsurf configs with a fitting policy, and logs every call.

If you want the log without any extra prompts or refusals, save this as `log-only.lugh`:

```lugh
agent LogOnly
  policy
    allow read, write, send, submit, spend, delete, execute
```

and wrap a server with it:

```bash
npx -y lugh-lang guard --policy ./log-only.lugh -- <your server command>
```

## What the log looks like

By default the log is `~/.lugh/guard-audit.jsonl` (change it with `--audit <file>`). Each line is one event:

```json
{"t":1795702050.6,"agent":"PostgresGuard","kind":"effect","effect":"read","action":"execute_sql"}
{"t":1795702061.2,"agent":"PostgresGuard","kind":"denied","effect":"delete","action":"execute_sql"}
{"t":1795702090.9,"agent":"PostgresGuard","kind":"human","effect":"write","action":"execute_sql","answer":"yes"}
```

The kinds are `start`, `effect` (the call went through), `denied`, `human` (you were asked, with your answer), `held` (approval needed but the client can't ask), `budget` (a time or call budget ran out) and `server_exit`.

## Useful one-liners

```bash
# everything that was refused
grep '"kind":"denied"' ~/.lugh/guard-audit.jsonl

# what you approved
grep '"kind":"human"' ~/.lugh/guard-audit.jsonl | grep '"answer":"yes"'

# calls per tool
jq -r 'select(.kind=="effect") | .action' ~/.lugh/guard-audit.jsonl | sort | uniq -c | sort -rn
```

Because it's plain JSONL you can ship it anywhere: tail it into your log stack, or attach it to an incident review.
