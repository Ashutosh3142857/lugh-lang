# Lugh MCP server for Claude Code

Adds the Lugh MCP server (`npx -y lugh-lang mcp`): tools to read the Lugh
language reference, check a program, dry-run it in a sandbox and run its tests.
All tools are read-only and nothing real is touched.

```
/plugin install lugh-mcp --marketplace Ashutosh3142857/lugh-lang
```

Pair it with the `lugh` plugin from the same marketplace for `/lugh:protect` and
the Lugh workflow skill. Needs Node.js 18 or later. MIT licensed.
