# Guard mode: a Lugh policy in front of any MCP server

`lugh-lang guard` puts allow / ask / deny rules, budgets and an audit log in front
of an MCP server you already use, without rewriting anything. Your client (Claude
Code, Cursor, VS Code, Claude Desktop) talks to the guard; the guard starts the
real server and passes every message through, except that each tool call must
clear the policy first.

```bash
npx -y lugh-lang guard --policy github -- docker run -i --rm -e GITHUB_PERSONAL_ACCESS_TOKEN ghcr.io/github/github-mcp-server
```

In an MCP config, the guard is the command and the real server goes after `--`:

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "lugh-lang", "guard", "--policy", "filesystem", "--",
               "npx", "-y", "@modelcontextprotocol/server-filesystem", "/Users/me/projects"]
    }
  }
}
```

In Claude Code:

```bash
claude mcp add filesystem -- npx -y lugh-lang guard --policy filesystem -- npx -y @modelcontextprotocol/server-filesystem ~/projects
```

## What happens to a tool call

1. The guard finds the call's **effect**: `read`, `write`, `send`, `submit`,
   `spend`, `delete` or `execute`. A matching rule in the policy's `tools` block
   decides it; otherwise the server's MCP annotations do (`readOnlyHint` → read,
   `destructiveHint` → delete, anything else → submit). A tool the server doesn't
   list counts as submit.
2. It checks, in order: the **time budget**, the **call budget**, **deny**,
   **allow**, and otherwise **asks a person**. These are the same rules, in the same
   order, as the policy gate inside Lugh programs. Every gated call counts toward
   the call budget, including denied ones.
3. **Ask** shows an approval prompt in your client (MCP elicitation) with the tool
   name, its effect and its arguments. If the client can't show one, the call is
   held back and the agent is told a person must approve it.
4. A refused call never reaches the server. The agent gets a tool error that says
   why, so it can tell you instead of looping.
5. Every decision is appended to the audit log (default
   `~/.lugh/guard-audit.jsonl`, or `--audit <file>`), one JSON object per line.

`tools/list` responses are annotated, so the agent sees each tool's policy up
front: `[Lugh policy: delete, denied]`. With `--hide-denied`, denied tools are
left out of the list entirely.

## Policy files

A guard policy is a Lugh header (any full Lugh program works too; its goals are
ignored), plus an optional `tools` block:

```lugh
agent SupabaseGuard
  budget calls 200, time 2h
  policy
    allow read
    ask human before write, execute, submit, spend
    deny delete
  tools
    "execute_sql" when query matches "\b(drop|truncate|delete)\b" is delete
    "execute_sql" is execute
    "delete_*" is delete
    "list_*" is read
```

- `"name" is <effect>` sets a tool's effect; `*` matches anything.
- `"name" when <argument> matches "<regex>" is <effect>` applies only when that
  argument matches (case-insensitive; dotted paths reach into objects). Rules are
  checked top to bottom and the first match wins, so put argument rules first.
- Argument rules are a speed bump, not a parser: they catch the obvious
  destructive call, not a determined one. Keep `ask` on anything that can do damage.

## Bundled policy packs

Pass a name instead of a file:

| Pack | Server | What it does |
| --- | --- | --- |
| `filesystem` | `@modelcontextprotocol/server-filesystem` | Reads run; writes, edits and moves ask; nothing is deleted |
| `github` | `github/github-mcp-server` | Reads run; writes, comments, pushes and merges ask; deleting repos and files is refused |
| `supabase` | Supabase MCP (via `mcp-remote`) | SQL that drops, truncates or deletes is refused; all other SQL, migrations, merges and deploys ask |
| `stripe` | Stripe MCP (via `mcp-remote`) | Lookups run; every write and every money movement asks |

The packs live in [`npm/policies/`](../npm/policies). Copy one to start your own.

Remote (HTTP) servers such as Stripe and Supabase are reached through
[`mcp-remote`](https://www.npmjs.com/package/mcp-remote), which turns them into a
local stdio server the guard can sit in front of:

```bash
npx -y lugh-lang guard --policy stripe -- npx -y mcp-remote https://mcp.stripe.com
```

## Limits

- Annotations are hints from the server. Only guard servers you trust, or set
  every tool's effect yourself in the `tools` block.
- The guard sees tool names and arguments, not what a tool does inside. A tool
  that can do several things (for example `stripe_api_write`) gets one effect, so
  give it the most dangerous one.
- Approval prompts need a client that supports MCP elicitation; others get the
  "held back" message instead.
