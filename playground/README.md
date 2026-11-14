# Lugh Playground

**Live:** https://ashutosh3142857.github.io/lugh-lang/

The playground runs Lugh Core compiled to WebAssembly, entirely in the browser.
Adapters work, but on a simulated world created fresh for every run, so
programs can read files, call an HTTP API and use MCP servers without touching
anything real. Every call still goes through the policy gate first:
`allow`, `ask human before`, `deny` and budgets behave exactly as they do when
Lugh is installed. A **Policy log** at the end of each run lists every action
the program attempted and what the policy did with it.

## The simulated world

| Adapter | What's there |
| --- | --- |
| `fs` | `fares.json`, `orders.csv`, `inbox.json`, `config.json`, `README.txt`. Writes last until the run ends. |
| `http` | `GET https://api.example.com/fares[?to=DUB&max=45000]`, `/rates`, `/orders`, `/weather/<city>`, `/flaky` (fails), `/slow` (times out). `POST`, `PUT`, `DELETE` echo what they receive. |
| `mcp` | `notes`, `github`, `payments` and `mail` servers. Each tool has standard MCP annotations, so its effect is read, submit or delete. |
| `shell` | `echo`, `date` and `ls` only. |
| `db`, real network | Not available: install Lugh. |

**When asked** (Approve or Decline) sets how the playground answers approval
requests, so you can see both paths.

## Links

- `#example=refund_desk` opens an example; `#code=<base64url>` opens any program.
- Add `?run=1`, `?test=1` or `?check=1` to run it on load, and `&decline=1` to answer approvals with no.
- **Share** copies such a link for the program in the editor.

## For agents and scripts

Run any program in the same sandbox from the command line with
`npx lugh-lang run program.lugh`, or serve it to MCP clients with
`npx lugh-lang mcp` (see [npm/README.md](../npm/README.md)). Without npm, the
single-file runner below does the same (Node 18 or later):

```bash
curl -sO https://raw.githubusercontent.com/Ashutosh3142857/lugh-lang/main/playground/run.mjs
node run.mjs program.lugh            # run goals (prints output and a policy log)
node run.mjs program.lugh --test     # run test blocks
node run.mjs program.lugh --check    # compile only
node run.mjs program.lugh --decline  # answer approvals with no
node run.mjs --sandbox               # what the simulated world contains
```

`run.mjs` downloads `lugh.wasm` from the published playground, and exits with
status 1 when a goal or test fails, so an agent can check a program it wrote
before handing it to a person.

## Examples

The examples are in [`examples/`](examples/), listed in `examples/index.json`.
After editing one, run `python3 playground/sync_examples.py` to embed it in
`index.html`; the test suite checks they match and that every example runs.
