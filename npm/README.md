# lugh-lang

[Lugh](https://github.com/Ashutosh3142857/lugh-lang) is a programming language for
AI agents. A program declares which systems the agent may touch (`use`), its limits
(`budget`), and which actions need a person (`policy`). Every action is typed as read,
write, send, submit, spend, delete or execute, and the runtime checks it before it runs.

This package runs Lugh Core (compiled to WebAssembly) in a sandbox, with no install:
`fs`, `http`, `mcp` and `shell` act on a simulated world, but every call passes the
real policy gate and budgets. Use it to check what an agent would do before it does it.

```bash
npx lugh-lang run program.lugh            # run goals; prints a policy log of every action
npx lugh-lang run program.lugh --decline  # answer every approval with no
npx lugh-lang test program.lugh           # run test blocks
npx lugh-lang check program.lugh          # compile only
npx lugh-lang sandbox                     # what the simulated world contains
npx lugh-lang guard --policy <file|pack> -- <mcp server command>   # guard a real MCP server
```

Commands exit with status 1 when a goal, test or compile step fails.

## MCP server

`npx lugh-lang mcp` is a Model Context Protocol server (stdio) with five read-only
tools: `lugh_reference`, `lugh_check`, `lugh_run`, `lugh_test` and `lugh_sandbox`.
An agent can write a Lugh program for a task, check it, dry-run it, and show the
person the policy log before anything runs for real.

```json
{
  "mcpServers": {
    "lugh": { "command": "npx", "args": ["-y", "lugh-lang", "mcp"] }
  }
}
```

## Guard mode: a policy in front of any MCP server

Keep the MCP servers you already use and put Lugh's policy gate in front of them:

```bash
npx -y lugh-lang guard --policy filesystem -- npx -y @modelcontextprotocol/server-filesystem ~/projects
```

Each tool call gets an effect (from the server's annotations or your policy),
then passes the gate: budgets, deny, allow, otherwise your client asks you.
Refused calls never reach the server, and every decision goes to an audit log.
Bundled policies: `filesystem`, `github`, `supabase`, `stripe`. Details:
[docs/GUARD.md](https://github.com/Ashutosh3142857/lugh-lang/blob/main/docs/GUARD.md).

## Running for real

To act on real files, APIs and MCP servers, install Lugh Core (Rust) from the
[repository](https://github.com/Ashutosh3142857/lugh-lang). There, approvals are
asked in the terminal or a web inbox, and every action is written to an audit log.

Try it in the browser: https://ashutosh3142857.github.io/lugh-lang/

MIT licensed.
