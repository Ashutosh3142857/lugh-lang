# Privacy

Lugh has no service, no accounts and no telemetry. Nothing you do with Lugh is
sent to the Lugh project.

## What runs where

- **The `lugh-lang` npm package, Lugh Core and the Claude Code plugins** run on
  your computer. `npx` downloads the package from the npm registry; that is the
  only network request Lugh itself causes.
- **`lugh-lang protect`** reads the MCP configuration files of the MCP clients on
  your computer and, after you say yes, rewrites the MCP server entries in them.
  It saves a copy of each file next to it (`<file>.lugh-backup`). It sends none
  of this anywhere.
- **`lugh-lang guard`** relays messages between your MCP client and the MCP
  servers you configured. It does not contact anything else.
- **The audit log** (`~/.lugh/guard-audit.jsonl`, or the file you choose) records
  each tool call guard decided on: the tool, its effect, the decision and up to
  300 characters of its arguments. Those arguments can contain personal data,
  such as an email address passed to a mail server. The log stays on your
  computer; delete it whenever you like.
- **The browser playground** runs in your browser against a simulated world. It
  stores nothing on a server.

The MCP servers you guard, and the model provider your client uses, have their
own privacy terms; Lugh does not change what they receive.

## Contact

Questions: open an issue at https://github.com/Ashutosh3142857/lugh-lang/issues
