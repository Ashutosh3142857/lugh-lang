// Tests for `lugh-lang guard`: a Lugh policy in front of a real MCP server process.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { parsePolicy, effectOf, decide, resolvePolicy, PACKS } from "../lib/guard.mjs";

const here = fileURLToPath(new URL(".", import.meta.url));
const BIN = join(here, "..", "bin", "lugh-lang.mjs");
const SERVER = join(here, "fixtures", "fake-server.mjs");

const POLICY = `agent TestGuard
  budget calls 6
  policy
    allow read
    ask human before submit
    deny delete
  tools
    "legacy_*" is read
    "add_item" when name matches "^drop|truncate" is delete
`;

// Start guard around the fake server and talk to it like an MCP client.
function session({ policy = POLICY, elicitation = false, approve = true, extra = [] } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "lugh-guard-"));
  const file = join(dir, "policy.lugh");
  const audit = join(dir, "audit.jsonl");
  writeFileSync(file, policy);
  const p = spawn(process.execPath, [BIN, "guard", "--policy", file, "--audit", audit, ...extra, "--", process.execPath, SERVER]);
  const waiters = new Map();
  const asked = [];
  let buf = "", id = 0;
  p.stdout.on("data", (d) => {
    buf += d;
    let i;
    while ((i = buf.indexOf("\n")) >= 0) {
      const msg = JSON.parse(buf.slice(0, i));
      buf = buf.slice(i + 1);
      if (msg.method === "elicitation/create") {
        asked.push(msg.params.message);
        const result = approve ? { action: "accept", content: { approve: true } } : { action: "decline" };
        p.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: msg.id, result }) + "\n");
      } else if (waiters.has(msg.id)) {
        waiters.get(msg.id)(msg);
        waiters.delete(msg.id);
      }
    }
  });
  const send = (method, params = {}) => new Promise((resolve) => {
    const mid = ++id;
    waiters.set(mid, resolve);
    p.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: mid, method, params }) + "\n");
  });
  const call = async (name, args = {}) => (await send("tools/call", { name, arguments: args })).result;
  const text = (r) => r.content[0].text;
  const ready = send("initialize", { protocolVersion: "2025-06-18", capabilities: elicitation ? { elicitation: {} } : {}, clientInfo: { name: "test", version: "1" } });
  const close = () => { p.stdin.end(); p.kill(); };
  const records = () => readFileSync(audit, "utf8").trim().split("\n").map((l) => JSON.parse(l));
  return { ready, send, call, text, close, records, asked };
}

test("reads pass, deletes are refused before they reach the server", async () => {
  const s = session();
  await s.ready;
  assert.match(s.text(await s.call("list_items")), /^ran list_items/);
  const del = await s.call("delete_item", { id: 7 });
  assert.equal(del.isError, true);
  assert.match(s.text(del), /policy denies 'delete'/);
  assert.deepEqual(JSON.parse(s.text(await s.call("calls_seen"))), ["list_items"]);
  s.close();
});

test("tools/list descriptions carry the policy for each tool", async () => {
  const s = session();
  await s.ready;
  const { tools } = (await s.send("tools/list")).result;
  const byName = Object.fromEntries(tools.map((t) => [t.name, t.description]));
  assert.match(byName.list_items, /\[Lugh policy: read, allowed\]/);
  assert.match(byName.add_item, /\[Lugh policy: submit, needs human approval\]/);
  assert.match(byName.delete_item, /\[Lugh policy: delete, denied\]/);
  assert.match(byName.legacy_export, /\[Lugh policy: read, allowed\]/); // tools rule beats missing annotations
  s.close();
});

test("--hide-denied removes denied tools from the list", async () => {
  const s = session({ extra: ["--hide-denied"] });
  await s.ready;
  const names = (await s.send("tools/list")).result.tools.map((t) => t.name);
  assert.ok(!names.includes("delete_item"));
  assert.ok(names.includes("add_item"));
  s.close();
});

test("ask: the client's approval prompt decides, yes runs it", async () => {
  const s = session({ elicitation: true, approve: true });
  await s.ready;
  await s.send("tools/list");
  assert.match(s.text(await s.call("add_item", { name: "milk" })), /^ran add_item/);
  assert.equal(s.asked.length, 1);
  assert.match(s.asked[0], /allow add_item \(submit\)/);
  s.close();
});

test("ask: no means the call never reaches the server", async () => {
  const s = session({ elicitation: true, approve: false });
  await s.ready;
  await s.send("tools/list");
  const r = await s.call("add_item", { name: "milk" });
  assert.equal(r.isError, true);
  assert.match(s.text(r), /declined/);
  assert.deepEqual(JSON.parse(s.text(await s.call("calls_seen"))), []);
  s.close();
});

test("argument rules: a destructive payload is denied, the same tool otherwise asks", async () => {
  const s = session({ elicitation: true, approve: true });
  await s.ready;
  await s.send("tools/list");
  const bad = await s.call("add_item", { name: "DROP TABLE users" });
  assert.equal(bad.isError, true);
  assert.match(s.text(bad), /policy denies 'delete'/);
  assert.match(s.text(await s.call("add_item", { name: "milk" })), /^ran add_item/);
  assert.deepEqual(JSON.parse(s.text(await s.call("calls_seen"))), ["add_item"]);
  s.close();
});

test("ask without an approval prompt in the client: held back, not run", async () => {
  const s = session({ elicitation: false });
  await s.ready;
  await s.send("tools/list");
  const r = await s.call("add_item");
  assert.equal(r.isError, true);
  assert.match(s.text(r), /needs a person's approval/);
  assert.deepEqual(JSON.parse(s.text(await s.call("calls_seen"))), []);
  s.close();
});

test("a call before tools/list: guard looks the tool up itself", async () => {
  const s = session({ elicitation: false });
  await s.ready; // the client never listed tools
  assert.match(s.text(await s.call("list_items")), /^ran list_items/);
  assert.equal((await s.call("delete_item")).isError, true);
  s.close();
});

test("a tool the server doesn't list counts as submit and needs approval", async () => {
  const s = session({ elicitation: false });
  await s.ready;
  const r = await s.call("mystery_tool");
  assert.equal(r.isError, true);
  assert.match(s.text(r), /'submit' needs a person's approval/);
  s.close();
});

test("the call budget stops a runaway loop, denied calls count too", async () => {
  const s = session({ policy: POLICY.replace("calls 6", "calls 3") });
  await s.ready;
  await s.send("tools/list");
  await s.call("list_items");
  await s.call("delete_item");
  await s.call("list_items");
  const r = await s.call("list_items");
  assert.equal(r.isError, true);
  assert.match(s.text(r), /call budget of 3 used up/);
  s.close();
});

test("every decision is written to the audit log", async () => {
  const s = session({ elicitation: true, approve: false });
  await s.ready;
  await s.send("tools/list");
  await s.call("list_items");
  await s.call("delete_item", { id: 1 });
  await s.call("add_item");
  s.close();
  await new Promise((r) => setTimeout(r, 100));
  const kinds = s.records().map((r) => `${r.kind}${r.effect ? ":" + r.effect : ""}`);
  assert.deepEqual(kinds.filter((k) => k !== "start" && !k.startsWith("server_exit")), ["effect:read", "denied:delete", "human:submit"]);
  assert.equal(s.records()[0].agent, "TestGuard");
});

test("policy files: effects and tool patterns", () => {
  const p = parsePolicy(POLICY);
  assert.equal(p.agent, "TestGuard");
  assert.equal(p.calls, 6);
  assert.equal(effectOf(p, "legacy_export", undefined), "read");
  assert.equal(effectOf(p, "x", { annotations: { destructiveHint: true } }), "delete");
  assert.equal(effectOf(p, "x", {}), "submit");
  assert.equal(decide(p, { started: Date.now(), calls: 0 }, "delete").verdict, "deny");
  assert.equal(decide(p, { started: Date.now(), calls: 0 }, "spend").verdict, "ask");
  assert.equal(parsePolicy("agent A\n  budget time 2h\n").time, 7200);
  assert.throws(() => parsePolicy("agent A\n  policy\n    allow everything\n"), /unknown effect 'everything'/);
  assert.throws(() => parsePolicy("agent A\n  tools\n    delete_* is delete\n"), /tool rules look like/);
  const q = parsePolicy('agent A\n  tools\n    "execute_sql" when query matches "\\b(drop|truncate)\\b" is delete\n    "execute_sql" is execute\n');
  assert.equal(effectOf(q, "execute_sql", {}, { query: "select 1" }), "execute");
  assert.equal(effectOf(q, "execute_sql", {}, { query: "Drop table orders" }), "delete");
  assert.equal(effectOf(q, "execute_sql", {}, {}), "execute");
  // a full Lugh program can be used as a guard file: its goals are ignored
  const prog = parsePolicy("agent R\n  use mcp\n  policy\n    allow read\n  goal \"x\"\n    log 1\n");
  assert.ok(prog.allow.has("read"));
});

test("bundled policy packs parse and do what their comments say", () => {
  const pack = (n) => parsePolicy(readFileSync(resolvePolicy(n), "utf8"));
  for (const n of PACKS) assert.ok(pack(n).deny.has("delete"), `${n} denies delete`);
  const fs = pack("filesystem");
  assert.equal(effectOf(fs, "write_file", { annotations: { destructiveHint: true } }), "write");
  assert.equal(decide(fs, { started: Date.now(), calls: 0 }, "write").verdict, "ask");
  const gh = pack("github");
  assert.equal(effectOf(gh, "delete_repository", {}), "delete");
  assert.equal(effectOf(gh, "merge_pull_request", {}), "write");
  assert.equal(effectOf(gh, "list_issues", {}), "read");
  const sb = pack("supabase");
  assert.equal(effectOf(sb, "execute_sql", {}, { query: "select * from orders" }), "execute");
  assert.equal(effectOf(sb, "execute_sql", {}, { query: "DROP TABLE orders;" }), "delete");
  assert.equal(effectOf(sb, "execute_sql", {}, { query: "truncate users" }), "delete");
  assert.equal(effectOf(sb, "execute_sql", {}, { query: "delete from users where true" }), "delete");
  assert.equal(effectOf(sb, "apply_migration", {}, { name: "x", query: "create table t (id int)" }), "write");
  assert.equal(effectOf(sb, "list_tables", {}), "read");
  const st = pack("stripe");
  assert.equal(effectOf(st, "stripe_api_read", {}), "read");
  assert.equal(effectOf(st, "send_money", {}), "spend");
  assert.equal(decide(st, { started: Date.now(), calls: 0 }, "spend").verdict, "ask");
  assert.throws(() => resolvePolicy("nope"), /bundled packs/);
});
