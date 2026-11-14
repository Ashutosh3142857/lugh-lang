// A Model Context Protocol server (stdio, JSON-RPC 2.0) that lets any MCP client
// write, check and dry-run Lugh programs. Programs run in the sandbox: adapters
// act on a simulated world behind the real policy gate, so nothing real is touched.
import { readFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { failed } from "./runtime.mjs";

const VERSION = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version;
const REFERENCE = readFileSync(new URL("../REFERENCE.md", import.meta.url), "utf8");
const PROTOCOLS = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const code = { type: "string", description: "A complete Lugh program, starting with `agent <Name>`." };
const safe = { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false };
export const TOOLS = [
  {
    name: "lugh_reference",
    title: "Lugh language reference",
    description: "Syntax and semantics of Lugh, a programming language for AI agents where permissions (policy), budgets and human approval are part of the code. Read this before writing a program.",
    inputSchema: { type: "object", properties: {} },
    annotations: safe,
  },
  {
    name: "lugh_check",
    title: "Check a Lugh program",
    description: "Compile a Lugh program without running it. Reports syntax errors, unknown functions and adapters, and counts goals, tests, routes and pages.",
    inputSchema: { type: "object", properties: { code }, required: ["code"] },
    annotations: safe,
  },
  {
    name: "lugh_run",
    title: "Dry-run a Lugh program in the sandbox",
    description: "Run a Lugh program's goals in a sandbox. fs, http, mcp and shell act on a simulated world (see lugh_sandbox), but every call passes the real policy gate and budgets. Returns the output and a policy log of every action and its decision (allowed, approved, declined, denied). Use it to show a person exactly what an agent would do before it runs for real.",
    inputSchema: {
      type: "object",
      properties: {
        code,
        approvals: { type: "string", enum: ["approve", "decline"], description: "How to answer `ask human` and policy approvals. Default approve." },
      },
      required: ["code"],
    },
    annotations: safe,
  },
  {
    name: "lugh_test",
    title: "Run a Lugh program's tests",
    description: "Run the `test` blocks of a Lugh program in the sandbox and report passes and failures.",
    inputSchema: { type: "object", properties: { code }, required: ["code"] },
    annotations: safe,
  },
  {
    name: "lugh_sandbox",
    title: "Describe the sandbox",
    description: "List the simulated files, HTTP endpoints, MCP servers (with each tool's effect) and shell commands that programs run by lugh_run can use.",
    inputSchema: { type: "object", properties: {} },
    annotations: safe,
  },
];

function callTool(run, name, args = {}) {
  const need = () => {
    if (typeof args.code !== "string" || !args.code.trim()) throw new Error("`code` is required: a Lugh program starting with `agent <Name>`");
    return args.code;
  };
  switch (name) {
    case "lugh_reference": return { text: REFERENCE, isError: false };
    case "lugh_sandbox": return { text: run("", "sandbox"), isError: false };
    case "lugh_check": { const t = run(need(), "check"); return { text: t, isError: failed(t) }; }
    case "lugh_test": { const t = run(need(), "test"); return { text: t, isError: failed(t) }; }
    case "lugh_run": {
      const t = run(need(), "run", { decline: args.approvals === "decline" });
      // A denial or failed goal is a result to show, not a tool error.
      return { text: t, isError: false };
    }
    default: throw Object.assign(new Error(`Unknown tool: ${name}`), { code: -32602 });
  }
}

/** Handle one JSON-RPC message; returns the response object, or null for notifications. */
export function handle(run, msg) {
  const reply = result => ({ jsonrpc: "2.0", id: msg.id, result });
  const error = (code, message) => ({ jsonrpc: "2.0", id: msg.id ?? null, error: { code, message } });
  if (msg?.jsonrpc !== "2.0" || typeof msg.method !== "string") return error(-32600, "Invalid Request");
  const isNotification = !("id" in msg);
  switch (msg.method) {
    case "initialize": {
      const asked = msg.params?.protocolVersion;
      return reply({
        protocolVersion: PROTOCOLS.includes(asked) ? asked : PROTOCOLS[0],
        capabilities: { tools: { listChanged: false } },
        serverInfo: { name: "lugh", title: "Lugh", version: VERSION },
        instructions: "Lugh is a programming language for AI agents: a program declares what it may touch (use), its limits (budget) and which actions need a human (policy). Call lugh_reference first, write the program, check it with lugh_check, then dry-run it with lugh_run and show the person the policy log before running it for real with the Lugh CLI.",
      });
    }
    case "ping": return isNotification ? null : reply({});
    case "tools/list": return reply({ tools: TOOLS });
    case "tools/call": {
      try {
        const { text, isError } = callTool(run, msg.params?.name, msg.params?.arguments);
        return reply({ content: [{ type: "text", text }], isError });
      } catch (e) {
        if (e.code === -32602) return error(-32602, e.message);
        return reply({ content: [{ type: "text", text: String(e.message || e) }], isError: true });
      }
    }
    default:
      return isNotification ? null : error(-32601, `Method not found: ${msg.method}`);
  }
}

/** Serve over stdio: one JSON-RPC message per line. */
export function serve(run, input = process.stdin, output = process.stdout) {
  const rl = createInterface({ input, crlfDelay: Infinity });
  rl.on("line", line => {
    if (!line.trim()) return;
    let msg;
    try { msg = JSON.parse(line); } catch { output.write(JSON.stringify({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "Parse error" } }) + "\n"); return; }
    const res = Array.isArray(msg) ? msg.map(m => handle(run, m)).filter(Boolean) : handle(run, msg);
    if (res && (!Array.isArray(res) || res.length)) output.write(JSON.stringify(res) + "\n");
  });
  return rl;
}
