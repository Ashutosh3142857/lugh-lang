// lugh-lang guard: put a Lugh policy in front of any MCP server.
//
//   npx lugh-lang guard --policy github.lugh -- npx -y @modelcontextprotocol/server-github
//
// The guard is a stdio MCP proxy. It starts the real server as a child process
// and relays every message, except that each tools/call first passes the same
// policy gate Lugh programs use:
//
//   time budget → call budget → deny → allow → otherwise ask a human
//
// A tool's effect comes from its MCP annotations (readOnlyHint → read,
// destructiveHint → delete, otherwise submit), unless the policy file's `tools`
// block names it. "Ask" uses the client's approval prompt (MCP elicitation)
// when the client supports it; otherwise the call is held back and the agent is
// told why. Every decision is appended to a JSONL audit log.

import { spawn } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { homedir } from "node:os";

export const EFFECTS = ["read", "write", "send", "submit", "spend", "delete", "execute"];

// ───────── policy file ─────────
// A Lugh header (agent, budget, policy) plus an optional guard-only `tools`
// block that sets a tool's effect by name (`*` matches anything):
//
//   agent GitHubGuard
//     budget calls 200, time 2h
//     policy
//       allow read
//       ask human before submit, write
//       deny delete
//     tools
//       "delete_*" is delete
//       "search_*" is read
export function parsePolicy(src) {
  const p = { agent: "guard", allow: new Set(), deny: new Set(), ask: new Set(), calls: Infinity, time: Infinity, tools: [] };
  let section = null;
  src.split(/\r?\n/).forEach((raw, i) => {
    const no = i + 1;
    const line = raw.replace(/\s+#.*$/, "").replace(/^\s*#.*$/, "");
    if (!line.trim()) return;
    const indent = line.length - line.trimStart().length;
    const t = line.trim();
    const fail = (m) => { throw new Error(`policy line ${no}: ${m}`); };
    if (indent === 0) {
      const m = t.match(/^(agent|guard)\s+(\S+)$/);
      if (!m) fail("expected `agent <Name>` at the top");
      p.agent = m[2];
      section = null;
      return;
    }
    if (indent === 2) {
      const [head, ...restWords] = t.split(/\s+/);
      const rest = restWords.join(" ");
      section = null;
      if (head === "budget") {
        for (const part of rest.split(",").map((s) => s.trim()).filter(Boolean)) {
          const [key, val] = part.split(/\s+/);
          if (key === "calls") {
            if (!/^\d+$/.test(val || "")) fail("bad call budget");
            p.calls = Number(val);
          } else if (key === "time") {
            const m = (val || "").match(/^(\d+(?:\.\d+)?)([smh]?)$/);
            if (!m) fail("bad time budget, e.g. 30s, 10m, 2h");
            p.time = Number(m[1]) * ({ m: 60, h: 3600 }[m[2]] || 1);
          } // spend / tokens: accepted, not metered (as in Lugh Core)
        }
      } else if (head === "policy" || head === "tools") {
        section = head;
      } else if (head === "use" || head === "goal" || head === "test") {
        section = "skip"; // a full program can double as a guard file
      } else {
        fail(`expected budget / policy / tools, got '${head}'`);
      }
      return;
    }
    if (section === "skip") return;
    if (section === "policy") {
      let bucket, list;
      if (t.startsWith("allow ")) [bucket, list] = [p.allow, t.slice(6)];
      else if (t.startsWith("deny ")) [bucket, list] = [p.deny, t.slice(5)];
      else if (t.startsWith("ask human before ")) [bucket, list] = [p.ask, t.slice(17)];
      else fail("policy rules are allow / deny / ask human before");
      for (const e of list.split(",").map((s) => s.trim())) {
        if (!EFFECTS.includes(e)) fail(`unknown effect '${e}' (effects: ${EFFECTS.join(", ")})`);
        bucket.add(e);
      }
      return;
    }
    if (section === "tools") {
      const m = t.match(/^"([^"]+)"(?:\s+when\s+([\w.]+)\s+matches\s+"((?:[^"\\]|\\.)*)")?\s+is\s+(\w+)$/);
      if (!m) fail('tool rules look like: "delete_*" is delete, or "execute_sql" when query matches "drop|truncate" is delete');
      const [, pattern, arg, argPattern, effect] = m;
      if (!EFFECTS.includes(effect)) fail(`unknown effect '${effect}'`);
      const re = new RegExp("^" + pattern.split("*").map((s) => s.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*") + "$");
      let when = null;
      if (arg) {
        try { when = { arg, re: new RegExp(argPattern.replace(/\\"/g, '"'), "i") }; } catch (e) { fail(`bad pattern: ${e.message}`); }
      }
      p.tools.push({ pattern, re, when, effect });
      return;
    }
    fail("unexpected indentation");
  });
  return p;
}

// The effect of one call: the first matching `tools` rule wins, then MCP annotations.
// A rule with `when <arg> matches "<regex>"` applies only when that argument
// (a dotted path; objects are matched as JSON) matches, case-insensitively.
export function effectOf(policy, name, tool, args = {}) {
  const rule = policy.tools.find((r) => r.re.test(name) && (!r.when || argMatches(args, r.when)));
  if (rule) return rule.effect;
  const a = (tool && tool.annotations) || {};
  if (a.readOnlyHint === true) return "read";
  if (a.destructiveHint === true) return "delete";
  return "submit";
}

// What the gate decides before asking anyone: allow, deny or ask (with a reason).
export function decide(policy, state, effect) {
  if ((Date.now() - state.started) / 1000 > policy.time) return { verdict: "budget", reason: `time budget of ${fmtTime(policy.time)} used up` };
  if (state.calls + 1 > policy.calls) return { verdict: "budget", reason: `call budget of ${policy.calls} used up` };
  if (policy.deny.has(effect)) return { verdict: "deny", reason: `policy denies '${effect}'` };
  if (policy.allow.has(effect)) return { verdict: "allow" };
  return { verdict: "ask" };
}

function argMatches(args, when) {
  let v = args;
  for (const k of when.arg.split(".")) v = v == null ? undefined : v[k];
  if (v === undefined) return false;
  return when.re.test(typeof v === "string" ? v : JSON.stringify(v));
}

function fmtTime(s) { return s >= 3600 ? `${s / 3600}h` : s >= 60 ? `${s / 60}m` : `${s}s`; }

function note(policy, effect) {
  if (policy.deny.has(effect)) return `[Lugh policy: ${effect}, denied]`;
  if (policy.allow.has(effect)) return `[Lugh policy: ${effect}, allowed]`;
  return `[Lugh policy: ${effect}, needs human approval]`;
}

// ───────── the proxy ─────────
export function guard({ policy, command, args, audit, hideDenied = false, onExit = () => {}, stdin = process.stdin, stdout = process.stdout, stderr = process.stderr, env = process.env }) {
  const child = spawn(command, args, { stdio: ["pipe", "pipe", "pipe"], env });
  const state = { started: Date.now(), calls: 0, tools: new Map(), elicitation: false, nextId: 1 };
  const pendingList = new Set(); // ids of tools/list requests we forwarded
  const waiting = new Map(); // our elicitation request id → resolve
  const ownReplies = new Map(); // our own requests to the server → handler
  const log = (kind, fields) => {
    if (!audit) return;
    try {
      mkdirSync(dirname(audit), { recursive: true });
      appendFileSync(audit, JSON.stringify({ t: Date.now() / 1000, agent: policy.agent, kind, ...fields }) + "\n");
    } catch (e) { stderr.write(`lugh guard: cannot write audit log: ${e.message}\n`); }
  };
  const toClient = (msg) => stdout.write(JSON.stringify(msg) + "\n");
  const toServer = (msg) => child.stdin.write(JSON.stringify(msg) + "\n");
  const blocked = (id, text) => toClient({ jsonrpc: "2.0", id, result: { content: [{ type: "text", text }], isError: true } });

  async function approve(question) {
    if (!state.elicitation) return null; // client cannot ask; caller decides
    const id = `lugh-guard-${state.nextId++}`;
    const answer = new Promise((resolve) => waiting.set(id, resolve));
    toClient({
      jsonrpc: "2.0", id, method: "elicitation/create",
      params: {
        message: question,
        requestedSchema: { type: "object", properties: { approve: { type: "boolean", title: "Allow this action", default: true } } },
      },
    });
    const res = await answer;
    return !!(res && res.action === "accept" && (!res.content || res.content.approve !== false));
  }

  // Ask the server for its tool list ourselves (used when a call names a tool we haven't seen).
  function refreshTools() {
    const id = `lugh-guard-list-${state.nextId++}`;
    return new Promise((resolve) => {
      ownReplies.set(id, (res) => {
        for (const tool of (res && res.tools) || []) state.tools.set(tool.name, tool);
        resolve();
      });
      toServer({ jsonrpc: "2.0", id, method: "tools/list", params: {} });
      setTimeout(() => { if (ownReplies.delete(id)) resolve(); }, 10000);
    });
  }

  async function onToolCall(msg) {
    const name = msg.params && msg.params.name;
    if (!state.tools.has(name) && state.initialized) await refreshTools();
    const effect = effectOf(policy, name, state.tools.get(name), (msg.params && msg.params.arguments) || {});
    const shown = JSON.stringify((msg.params && msg.params.arguments) || {});
    const action = `${name} ${shown.length > 300 ? shown.slice(0, 300) + "…" : shown}`;
    const d = decide(policy, state, effect);
    if (d.verdict !== "budget") state.calls++; // as in Lugh Core, every gated call counts, even a denied one
    if (d.verdict === "budget") {
      log("budget", { effect, action, reason: d.reason });
      return blocked(msg.id, `Lugh guard blocked ${name}: ${d.reason}.`);
    }
    if (d.verdict === "deny") {
      log("denied", { effect, action });
      return blocked(msg.id, `Lugh guard blocked ${name}: ${d.reason}. This action is not allowed by the policy; do not retry it another way. Tell the user what you were trying to do.`);
    }
    if (d.verdict === "ask") {
      const ok = await approve(`Lugh guard: allow ${name} (${effect})?\n${shown}`);
      if (ok === null) {
        log("held", { effect, action });
        return blocked(msg.id, `Lugh guard held ${name}: '${effect}' needs a person's approval and this client cannot show an approval prompt. Ask the user; they can allow it in the guard policy.`);
      }
      log("human", { effect, action, answer: ok ? "yes" : "no" });
      if (!ok) return blocked(msg.id, `Lugh guard: the user declined ${name}. Do not retry it.`);
    }
    log("effect", { effect, action });
    toServer(msg);
  }

  function fromClient(msg) {
    if (Array.isArray(msg)) {
      // JSON-RPC batches are not part of current MCP; never let one carry a call past the gate.
      if (msg.some((m) => m && m.method === "tools/call")) {
        for (const m of msg) if (m && m.id !== undefined) toClient({ jsonrpc: "2.0", id: m.id, error: { code: -32600, message: "Lugh guard does not accept batched tool calls" } });
        return;
      }
      return toServer(msg);
    }
    if (msg.id !== undefined && waiting.has(msg.id) && !msg.method) {
      const resolve = waiting.get(msg.id);
      waiting.delete(msg.id);
      return resolve(msg.error ? null : msg.result);
    }
    if (msg.method === "initialize") {
      state.elicitation = !!(msg.params && msg.params.capabilities && msg.params.capabilities.elicitation);
    }
    if (msg.method === "tools/list" && msg.id !== undefined) pendingList.add(msg.id);
    if (msg.method === "tools/call") return void onToolCall(msg);
    toServer(msg);
  }

  function fromServer(msg) {
    if (!Array.isArray(msg) && msg.id !== undefined && ownReplies.has(msg.id)) {
      const handle = ownReplies.get(msg.id);
      ownReplies.delete(msg.id);
      return handle(msg.result);
    }
    if (!Array.isArray(msg) && msg.result && msg.result.serverInfo) state.initialized = true;
    if (!Array.isArray(msg) && msg.id !== undefined && pendingList.has(msg.id) && msg.result && Array.isArray(msg.result.tools)) {
      pendingList.delete(msg.id);
      const tools = [];
      for (const tool of msg.result.tools) {
        state.tools.set(tool.name, tool);
        const effect = effectOf(policy, tool.name, tool);
        if (hideDenied && policy.deny.has(effect)) continue;
        tools.push({ ...tool, description: `${tool.description || ""} ${note(policy, effect)}`.trim() });
      }
      msg = { ...msg, result: { ...msg.result, tools } };
    }
    toClient(msg);
  }

  lines(stdin, fromClient, (bad) => stderr.write(`lugh guard: ignoring a non-JSON line from the client: ${bad.slice(0, 80)}\n`));
  lines(child.stdout, fromServer, (bad) => stderr.write(`lugh guard: server wrote a non-JSON line: ${bad.slice(0, 80)}\n`));
  child.stderr.on("data", (d) => stderr.write(d));
  child.on("exit", (code) => { log("server_exit", { code }); onExit(code ?? 0); });
  child.on("error", (e) => { stderr.write(`lugh guard: cannot start the server: ${e.message}\n`); onExit(1); });
  stdin.on("end", () => child.stdin.end());
  log("start", { command: [command, ...args].join(" ") });
  return child;
}

function lines(stream, onMessage, onBad) {
  let buf = "";
  stream.on("data", (chunk) => {
    buf += chunk.toString("utf8");
    let i;
    while ((i = buf.indexOf("\n")) >= 0) {
      const line = buf.slice(0, i).trim();
      buf = buf.slice(i + 1);
      if (!line) continue;
      let msg;
      try { msg = JSON.parse(line); } catch { onBad(line); continue; }
      onMessage(msg);
    }
  });
}

// A policy is a file path, or the name of a bundled pack (filesystem, github, supabase, stripe).
export const PACKS = ["filesystem", "github", "supabase", "stripe"];
export function resolvePolicy(name) {
  if (existsSync(name)) return name;
  if (PACKS.includes(name)) return fileURLToPath(new URL(`../policies/${name}.lugh`, import.meta.url));
  throw new Error(`no policy file '${name}' (bundled packs: ${PACKS.join(", ")})`);
}

// CLI entry: lugh-lang guard --policy <file> [--audit <file>] [--hide-denied] -- <command> [args...]
export function main(argv) {
  const sep = argv.indexOf("--");
  const opts = sep >= 0 ? argv.slice(0, sep) : argv;
  const cmd = sep >= 0 ? argv.slice(sep + 1) : [];
  const get = (flag) => { const i = opts.indexOf(flag); return i >= 0 ? opts[i + 1] : undefined; };
  const file = get("--policy");
  if (!file || !cmd.length) {
    process.stderr.write("usage: npx lugh-lang guard --policy <file.lugh | filesystem | github | supabase | stripe> [--audit <log.jsonl>] [--hide-denied] -- <server command> [args...]\n");
    process.exit(2);
  }
  let policy;
  try { policy = parsePolicy(readFileSync(resolvePolicy(file), "utf8")); } catch (e) { process.stderr.write(`lugh guard: ${e.message}\n`); process.exit(2); }
  const audit = get("--audit") || join(homedir(), ".lugh", "guard-audit.jsonl");
  guard({ policy, command: cmd[0], args: cmd.slice(1), audit, hideDenied: opts.includes("--hide-denied"), onExit: (code) => process.exit(code) });
}
