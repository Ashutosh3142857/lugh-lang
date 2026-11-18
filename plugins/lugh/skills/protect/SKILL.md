---
name: protect
description: Put every MCP server the user already has (Claude Code, Claude Desktop, Cursor, VS Code, Windsurf) behind Lugh guard, so reads run, risky actions ask first and destructive ones are refused. Use when the user wants to lock down, secure, restrict or audit their MCP servers or tools, or asks how to stop an agent deleting or changing things.
---

# Protect the user's MCP servers

`npx lugh-lang protect` finds the user's MCP configs and wraps each local server
in Lugh guard with a fitting policy (filesystem, github, supabase, stripe, or a
default that allows reads, asks before writes, sends, spends and commands, and
refuses deletes). It changes only those server entries, backs each file up first,
and can be undone.

1. Show the plan without changing anything:
   ```bash
   npx -y lugh-lang protect
   ```
   It lists every server it found, which policy each would get, and which it
   skips (remote servers, servers already guarded, Lugh's own server).
2. Show the user that list and ask whether to apply it. Do not continue without
   a clear yes: this rewrites their MCP configuration files.
3. With their yes:
   ```bash
   npx -y lugh-lang protect --yes
   ```
4. Tell them to restart their MCP clients (in Claude Code, start a new session),
   that decisions are logged to `~/.lugh/guard-audit.jsonl`, and that
   `npx -y lugh-lang protect --undo` puts everything back.

To change what a server may do, copy its policy from
https://github.com/Ashutosh3142857/lugh-lang/tree/main/npm/policies, edit it, and
point that server's `--policy` at the file. Policy syntax:
https://github.com/Ashutosh3142857/lugh-lang/blob/main/docs/GUARD.md
