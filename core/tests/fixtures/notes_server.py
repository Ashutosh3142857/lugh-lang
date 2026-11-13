"""A minimal MCP server over stdio, written directly against the protocol
(JSON-RPC 2.0, one message per line), used to test Lugh's MCP client.

Tools: list_notes (read-only), add_note, delete_note (destructive).
Notes live in the JSON file named by NOTES_FILE.
"""
import json, os, sys

NOTES = os.environ.get("NOTES_FILE", "notes.json")

def load():
    try:
        return json.load(open(NOTES))
    except (OSError, ValueError):
        return []

def save(notes):
    json.dump(notes, open(NOTES, "w"))

TOOLS = [
    {"name": "list_notes", "description": "List all notes",
     "inputSchema": {"type": "object", "properties": {}},
     "annotations": {"readOnlyHint": True}},
    {"name": "add_note", "description": "Add a note",
     "inputSchema": {"type": "object", "properties": {"title": {"type": "string"}, "body": {"type": "string"}},
                     "required": ["title"]},
     "annotations": {"readOnlyHint": False, "destructiveHint": False}},
    {"name": "delete_note", "description": "Delete a note by title",
     "inputSchema": {"type": "object", "properties": {"title": {"type": "string"}}, "required": ["title"]},
     "annotations": {"readOnlyHint": False, "destructiveHint": True}},
]

def text(value, error=False):
    return {"content": [{"type": "text", "text": json.dumps(value)}], "isError": error}

def call(name, args):
    notes = load()
    if name == "list_notes":
        return text(notes)
    if name == "add_note":
        if not args.get("title"):
            return text("title is required", error=True)
        note = {"title": args["title"], "body": args.get("body", "")}
        notes.append(note); save(notes)
        return text(note)
    if name == "delete_note":
        kept = [n for n in notes if n["title"] != args.get("title")]
        save(kept)
        return text({"deleted": len(notes) - len(kept)})
    raise KeyError(name)

def reply(msg_id, result=None, error=None):
    out = {"jsonrpc": "2.0", "id": msg_id}
    out["error" if error else "result"] = error or result
    sys.stdout.write(json.dumps(out) + "\n"); sys.stdout.flush()

for line in sys.stdin:
    msg = json.loads(line)
    method, msg_id = msg.get("method"), msg.get("id")
    if msg_id is None:
        continue  # notification
    if method == "initialize":
        reply(msg_id, {"protocolVersion": msg["params"]["protocolVersion"], "capabilities": {"tools": {}},
                       "serverInfo": {"name": "notes", "version": "1.0"}})
    elif method == "tools/list":
        reply(msg_id, {"tools": TOOLS})
    elif method == "tools/call":
        try:
            reply(msg_id, call(msg["params"]["name"], msg["params"].get("arguments", {})))
        except KeyError:
            reply(msg_id, error={"code": -32602, "message": "unknown tool"})
    else:
        reply(msg_id, error={"code": -32601, "message": "method not found"})
