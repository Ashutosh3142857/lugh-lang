# Lugh for Claude Code

Policy, budgets and human approval for the MCP servers you already use, and for
agent programs written in [Lugh](https://github.com/Ashutosh3142857/lugh-lang).

## What it adds

- **`/lugh:protect`**: puts every MCP server in your Claude Code, Claude Desktop,
  Cursor, VS Code and Windsurf configs behind Lugh guard. It shows you the plan
  and changes nothing until you say yes. Reads run, risky actions (writes, sends,
  payments, commands) ask you first, destructive ones are refused before they
  reach the server, and every decision is logged to `~/.lugh/guard-audit.jsonl`.
  `npx lugh-lang protect --undo` reverts it.
- **The Lugh skill**: the workflow for writing an agent with least-privilege
  policy, dry-running it, and showing you the policy log before it runs for real.

## Install

```
/plugin install lugh --marketplace Ashutosh3142857/lugh-lang
```

Needs Node.js 18 or later (protect runs through `npx`).

For the Lugh MCP server (check and dry-run programs from Claude), also install
`lugh-mcp` from the same marketplace, or run
`claude mcp add lugh -- npx -y lugh-lang mcp`.

## Links

- Guard mode and policy syntax: [docs/GUARD.md](https://github.com/Ashutosh3142857/lugh-lang/blob/main/docs/GUARD.md)
- Playground: https://ashutosh3142857.github.io/lugh-lang/
- License: MIT
