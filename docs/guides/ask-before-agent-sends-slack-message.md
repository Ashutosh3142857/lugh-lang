---
title: Make an AI agent ask before it sends a Slack message
description: Slack MCP servers let an agent post in your name. Keep reading free and make every message, reply and scheduled message wait for your yes.
slug: ask-before-agent-sends-slack-message
---

# Make an AI agent ask before it sends a Slack message

A Slack MCP server is great for "summarise what I missed in #eng". The same server also has a tool that posts messages as you. An agent that misreads a thread, or that reads a message containing instructions aimed at it, can reply to a customer channel before you notice.

The rule most people actually want is simple: **read anything, send nothing without asking.**

## Set it up

```bash
npx lugh-lang protect
```

`protect` finds your Slack MCP server (Slack's own, `korotovsky/slack-mcp-server`, or the older `@modelcontextprotocol/server-slack`) in Claude Code, Cursor, VS Code, Claude Desktop or Windsurf and wraps it with the `slack` policy. It shows the change first and `--undo` reverts it.

Or wrap one server by hand:

```bash
npx -y lugh-lang guard --policy slack -- npx -y @modelcontextprotocol/server-slack
```

## What the `slack` policy does

| Tool calls | Result |
|---|---|
| list channels, read history and threads, search, look up users | run |
| send a message, reply in a thread, schedule a message | ask you first |
| add or remove a reaction, mark as read | run |
| create or update canvases and user groups | ask you first |
| anything with "delete" in the name | refused |

"Ask" uses your client's own approval prompt (MCP elicitation) where the client supports it. Where it doesn't, the call is held back and the agent is told it needs approval, so nothing is sent silently.

Every decision, including the ones you approved, is written to `~/.lugh/guard-audit.jsonl`.

## Tighten or loosen it

The policy is a few lines of text. To let the agent post freely in one channel but ask everywhere else:

```lugh
  tools
    "*post_message*" when channel_id matches "^C0123ABCD$" is write
    "*post_message*" is send
```

Put the specific rule first; the first match wins. Save it as `slack.lugh` and use `--policy ./slack.lugh`.
