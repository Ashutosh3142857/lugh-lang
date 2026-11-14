// MCP protocol tests. With LUGH_STUB=1 they use a stand-in runtime (no lugh.wasm
// needed); in CI they run against the real WebAssembly build.
import assert from "node:assert";
import { PassThrough } from "node:stream";
import { handle, serve, TOOLS } from "../lib/mcp.mjs";
import { load } from "../lib/runtime.mjs";

const stub = (src, mode, opts = {}) =>
  mode === "sandbox" ? "Files: fares.json …" :
  mode === "check" ? (src.startsWith("agent ") ? "✅ compiles: 1 goal(s)" : "❌ a Lugh program must start with: agent <Name>") :
  mode === "test" ? "  ✓ t\n\n1 passed, 0 failed" :
  (opts.decline ? "… you said no\n✗ submit  you said no" : "… you said yes\n✓ submit  asked you, approved");
const run = process.env.LUGH_STUB ? stub : await load();
const call = (name, args, id = 3) => handle(run, { jsonrpc: "2.0", id, method: "tools/call", params: { name, arguments: args } });

// initialize negotiates a supported protocol version
const init = handle(run, { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "t", version: "1" } } });
assert.equal(init.result.protocolVersion, "2025-06-18");
assert.equal(init.result.serverInfo.name, "lugh");
assert.ok(init.result.capabilities.tools);
assert.equal(handle(run, { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "1999-01-01" } }).result.protocolVersion, "2025-11-25");
assert.equal(handle(run, { jsonrpc: "2.0", method: "notifications/initialized" }), null);

// tools/list: five tools, all read-only
const list = handle(run, { jsonrpc: "2.0", id: 2, method: "tools/list" }).result.tools;
assert.deepEqual(list.map(t => t.name), ["lugh_reference", "lugh_check", "lugh_run", "lugh_test", "lugh_sandbox"]);
assert.ok(list.every(t => t.annotations.readOnlyHint && t.inputSchema.type === "object"));
assert.equal(TOOLS.length, 5);

const program = 'agent A\n  use mcp\n  policy\n    allow read\n    ask human before submit\n    deny delete\n  goal "g"\n    mcp.call "notes" "add_note" {title: "T", body: "B"}\n';
assert.match(call("lugh_reference").result.content[0].text, /policy/);
assert.match(call("lugh_sandbox").result.content[0].text, /fares\.json/);
assert.match(call("lugh_check", { code: program }).result.content[0].text, /compiles/);
assert.equal(call("lugh_check", { code: "not lugh" }).result.isError, true);
assert.match(call("lugh_run", { code: program }).result.content[0].text, /asked you, approved/);
assert.match(call("lugh_run", { code: program, approvals: "decline" }).result.content[0].text, /you said no/);
assert.match(call("lugh_test", { code: 'agent A\n  test "t"\n    expect 1 == 1\n' }).result.content[0].text, /1 passed, 0 failed/);
assert.equal(call("lugh_run", {}).result.isError, true);
assert.equal(call("no_such_tool", {}).error.code, -32602);
assert.equal(handle(run, { jsonrpc: "2.0", id: 9, method: "nope" }).error.code, -32601);
assert.equal(handle(run, { jsonrpc: "2.0", id: 10, method: "ping" }).result && true, true);

// real stdio round trip
const input = new PassThrough(), output = new PassThrough();
serve(run, input, output);
const lines = [];
output.on("data", d => lines.push(...d.toString().trim().split("\n")));
input.write(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list" }) + "\n");
input.write("{not json\n");
await new Promise(r => setTimeout(r, 50));
assert.equal(JSON.parse(lines[0]).result.tools.length, 5);
assert.equal(JSON.parse(lines[1]).error.code, -32700);

if (!process.env.LUGH_STUB) {
  // Against the real build: the policy gate runs before simulated calls.
  const out = call("lugh_run", { code: program + '  goal "d"\n    mcp.call "notes" "delete_note" {title: "T"}\n' }).result.content[0].text;
  assert.match(out, /denied by policy/);
}
console.log(`✓ MCP server: initialize, tools/list, ${list.length} tools, errors and stdio${process.env.LUGH_STUB ? " (stub runtime)" : ""}`);
