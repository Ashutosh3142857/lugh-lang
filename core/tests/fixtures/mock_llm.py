"""Stand-in for an Anthropic-compatible Messages endpoint, for testing `lugh plan`.

The first reply is a program that does not compile (it calls an unknown
function); the second is a corrected program. Requests are saved to
REQUEST_LOG so tests can check what the planner sent.
"""
import json, os, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

BROKEN = """```lugh
agent VisaNotes
  use mcp
  goal "Summarise the notes"
    let notes = mcp.call "notes" "list_notes"
    log text.summarize(notes)
```"""

FIXED = """Here is the corrected program.
```lugh
agent VisaNotes
  use mcp
  budget time 1m, calls 10
  policy
    allow read
    ask human before submit
    deny delete
  goal "Add a reminder and summarise the visa notes"
    mcp.call "notes" "add_note" {title: "Photos", body: "Two passport photos"}
    let notes = mcp.call "notes" "list_notes"
    let titles = map n in notes to n.title
    log "You have {titles}"
```"""

class Handler(BaseHTTPRequestHandler):
    calls = 0

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        with open(os.environ.get("REQUEST_LOG", "requests.jsonl"), "a") as f:
            f.write(json.dumps(body) + "\n")
        Handler.calls += 1
        text = BROKEN if Handler.calls == 1 else FIXED
        out = json.dumps({"content": [{"type": "text", "text": text}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)

    def log_message(self, *args):
        pass

HTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
