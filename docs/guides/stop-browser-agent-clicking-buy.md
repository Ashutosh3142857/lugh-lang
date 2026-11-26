---
title: Stop a browser agent from clicking "Buy" or "Pay"
description: Playwright MCP gives an AI agent a real browser. Let it browse, click and type, but make it ask before it submits, pays, sends, signs in or uploads.
slug: stop-browser-agent-clicking-buy
---

# Stop a browser agent from clicking "Buy" or "Pay"

With the Playwright MCP server an agent can drive a real browser: open pages, click, type, fill forms. That's what makes it useful, and it's also what makes it risky. The click that opens a product page and the click that places the order are the same tool call, `browser_click`.

Asking before every click makes the agent useless. Asking before none makes it dangerous. The useful middle is to ask before the clicks that commit something.

## Set it up

```bash
npx lugh-lang protect
```

`protect` finds `@playwright/mcp` in your Claude Code, Cursor, VS Code, Claude Desktop or Windsurf config and wraps it with the `playwright` policy. Or by hand:

```bash
npx -y lugh-lang guard --policy playwright -- npx -y @playwright/mcp@latest
```

## What the `playwright` policy does

| The agent | Result |
|---|---|
| navigates, takes snapshots and screenshots, reads console and network | runs |
| clicks, types, selects, hovers, drags | runs |
| clicks an element whose label says buy, pay, purchase, checkout, order, subscribe, donate, transfer, send, submit, post, publish, confirm, approve, accept, agree, delete, remove, sign in, log in, sign up or register | asks you |
| types with `submit: true`, or presses Enter | asks you |
| fills a form that includes password or card fields | asks you |
| accepts a browser dialog | asks you |
| uploads a file | asks you |
| runs JavaScript in the page | asks you |
| clears cookies or storage | refused |

Every decision is logged to `~/.lugh/guard-audit.jsonl`.

## Why matching on labels works, and where it doesn't

Playwright's click tool includes a human-readable description of the element the model is clicking, like "Place order button". Guard matches on that. It catches the common case well, and a model that's trying to complete a purchase will describe the button honestly because that's how it finds it.

It is still a speed bump, not a guarantee. A button labelled "Continue" can complete a checkout. So keep the agent's browser profile signed out of accounts that can spend money, and treat the label rules as a second line.
