#!/usr/bin/env node
// lugh-lang: check and dry-run Lugh programs, or serve them to AI agents over MCP.
import { readFileSync } from "node:fs";
import { load, failed } from "../lib/runtime.mjs";

const HELP = `lugh-lang: Lugh, the programming language for AI agents

  npx lugh-lang run <file|->  [--decline]   run goals in the sandbox, with a policy log
  npx lugh-lang test <file|->               run test blocks
  npx lugh-lang check <file|->              compile only
  npx lugh-lang sandbox                     what the simulated world contains
  npx lugh-lang mcp                         MCP server (stdio) for AI agents

The sandbox simulates files, an HTTP API and MCP servers, and every call
passes the real policy gate. To act on real systems, install Lugh Core:
https://github.com/Ashutosh3142857/lugh-lang`;

const [cmd, file, ...rest] = process.argv.slice(2);
if (!cmd || cmd === "-h" || cmd === "--help") { console.log(HELP); process.exit(cmd ? 0 : 2); }
if (cmd === "--version" || cmd === "-v") {
  console.log(JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version);
  process.exit(0);
}
const run = await load();
if (cmd === "mcp") {
  const { serve } = await import("../lib/mcp.mjs");
  serve(run);
} else if (cmd === "sandbox") {
  console.log(run("", "sandbox"));
} else if (["run", "test", "check"].includes(cmd)) {
  if (!file) { console.error(`usage: npx lugh-lang ${cmd} <file|->`); process.exit(2); }
  const src = readFileSync(file === "-" ? 0 : file, "utf8");
  const out = run(src, cmd, { decline: rest.includes("--decline") });
  console.log(out.trimEnd());
  process.exit(failed(out) ? 1 : 0);
} else {
  console.error(`unknown command '${cmd}'\n\n${HELP}`);
  process.exit(2);
}
