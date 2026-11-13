# MCP and `lugh plan`

## Connecting to MCP servers

Lugh Core speaks the [Model Context Protocol](https://modelcontextprotocol.io)
over stdio, so a Lugh agent can use any MCP server: GitHub, Slack, Google
Drive, Notion, databases, file systems and thousands more.

Configure servers in `lugh.mcp.json` in the working directory, or point
`LUGH_MCP_CONFIG` at a file. The format is the same as Claude Desktop's, so an
existing config can be reused:

```json
{
  "mcpServers": {
    "notes": {
      "command": "python3",
      "args": ["core/tests/fixtures/notes_server.py"],
      "env": { "NOTES_FILE": "examples/data/notes.json" }
    }
  }
}
```

In a program:

```lugh
agent NotesAgent
  use mcp
  policy
    allow read
    ask human before submit
    deny delete

  goal "Capture the visa checklist"
    let tools = mcp.tools "notes"                       # name, description, effect, input
    mcp.call "notes" "add_note" {title: "Biometrics", body: "Book the appointment"}
    let notes = mcp.call "notes" "list_notes"
```

A server starts on first use and stays connected for the rest of the run.
`structuredContent` is returned as-is; a single text result is parsed as JSON
when possible; a tool error becomes a failure that `attempt … otherwise` can
catch.

### Effects come from tool annotations

| MCP annotation | Lugh effect |
| --- | --- |
| `readOnlyHint: true` | `read` |
| `destructiveHint: true` | `delete` |
| anything else | `submit` (asks a human unless allowed) |

The effect is resolved before the call is sent, so a denied tool never
reaches the server. Annotations are hints written by the server's author:
configure only servers you trust, just as you would in any MCP client.
Starting a server runs its configured command on your machine.

## `lugh plan`: the model writes, you approve

```bash
export ANTHROPIC_API_KEY=...
lugh plan "Add a passport photos reminder and list my visa notes" --use mcp --run
```

1. The planner gives the model a compact Lugh guide, the standard-library
   function names, and the adapters you allowed with `--use`. For `mcp`, it
   also gives the actual tools of every configured server, with their effects
   and arguments.
2. The reply is compiled. If it does not compile, the error goes back to the
   model, which gets up to three attempts.
3. You see the program and the `--check` report of everything it can touch.
   The program is saved to `--out` (default `plan.lugh`).
4. With `--run`, it asks before running, then runs with **plan safety**:
   whatever policy the model wrote, only `read` runs without asking, and
   `delete` is always denied.

Options: `--use fs,http,db,mcp`, `--out file.lugh`, `--run`, `--yes` (skip the
run confirmation and auto-approve; use only for testing). `LUGH_MODEL` selects
the model; `LUGH_LLM_URL` points at any Anthropic-compatible Messages endpoint.

### Model providers

`lugh plan` and `think` speak the Anthropic Messages format, so any
compatible provider works:

| Provider | Settings |
| --- | --- |
| Anthropic | `ANTHROPIC_API_KEY=sk-ant-…` (default endpoint) |
| DeepSeek | `ANTHROPIC_API_KEY=<DeepSeek key>` · `LUGH_LLM_URL=https://api.deepseek.com/anthropic/v1/messages` · `LUGH_MODEL=deepseek-flash` |
| Self-hosted or gateway | `LUGH_LLM_URL=<your /v1/messages endpoint>` |

Keep keys in the environment or a `.env` file (already in `.gitignore`), never
in a `.lugh` program or a commit.

## Tested against

- A protocol-level test server written directly from the MCP specification
  with no SDK ([`core/tests/fixtures/notes_server.py`](../core/tests/fixtures/notes_server.py)),
  covering initialization, tool listing, calls, tool errors and annotations.
- A stand-in model ([`core/tests/fixtures/mock_llm.py`](../core/tests/fixtures/mock_llm.py))
  whose first program does not compile, to test the repair loop.

To try an official server, for example the filesystem server:

```json
{"mcpServers": {"files": {"command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/folder"]}}}
```

```lugh
let tools = mcp.tools "files"
let text = mcp.call "files" "read_text_file" {path: "/path/to/folder/todo.txt"}
```
