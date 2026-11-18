# Lugh templates

Starting points for common agents. Each one runs as-is in the sandbox: open it in
the playground, change it, and share it as a link. To run one from a terminal:
`npx lugh-lang run <file>`.

| Agent | What it shows | Open |
| --- | --- | --- |
| [Agent tries to wipe production](../playground/examples/prod_db_incident.lugh) | An agent decides to empty a production table during a code freeze; the delete is refused and the data survives | [Playground](https://ashutosh3142857.github.io/lugh-lang/#example=prod_db_incident) |
| [Refund desk](../playground/examples/refund_desk.lugh) | Reads run on their own, every refund asks you first, and a destructive call is never allowed | [Playground](https://ashutosh3142857.github.io/lugh-lang/#example=refund_desk) |
| [Inbox assistant](../playground/examples/inbox_assistant.lugh) | Finds what matters, quotes the deadline from the source, drafts a reply that only goes out with your yes | [Playground](https://ashutosh3142857.github.io/lugh-lang/#example=inbox_assistant) |
| [GitHub issue triage](../playground/examples/issue_triage.lugh) | Ranks issues, comments on crashes, opens a tracking issue; `attempt` cannot swallow a policy denial | [Playground](https://ashutosh3142857.github.io/lugh-lang/#example=issue_triage) |
| [Notes agent over MCP](../playground/examples/notes_agent.lugh) | MCP tools get their effect from their annotations; the policy decides which may run | [Playground](https://ashutosh3142857.github.io/lugh-lang/#example=notes_agent) |
| [Fare watcher](../playground/examples/fare_watch.lugh) | Falls back when an API fails, accepts a fare only when sources agree | [Playground](https://ashutosh3142857.github.io/lugh-lang/#example=fare_watch) |
| [Budget guard](../playground/examples/budget_guard.lugh) | A call budget stops a runaway loop | [Playground](https://ashutosh3142857.github.io/lugh-lang/#example=budget_guard) |
| [CSV to Markdown report](../playground/examples/sales_report.lugh) | Parse, validate, aggregate and write a report | [Playground](https://ashutosh3142857.github.io/lugh-lang/#example=sales_report) |
| [Web app with approvals](../playground/examples/web_app.lugh) | Routes and pages in the same language, with tests | [Playground](https://ashutosh3142857.github.io/lugh-lang/#example=web_app) |

## Writing your own

1. Start with `use` listing only the adapters you need, `allow read`, and
   `ask human before` anything that sends, submits, spends or deletes.
2. `npx lugh-lang check agent.lugh`, then `npx lugh-lang run agent.lugh` to see the
   policy log.
3. When it does what you want, run it for real with Lugh Core: `lugh agent.lugh`.

For coding agents (Claude Code, Cursor and others), the [Lugh skill](../skills/lugh/SKILL.md)
and the MCP server (`npx -y lugh-lang mcp`) give the same workflow.
