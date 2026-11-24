// lugh-lang protect: put every local MCP server you already use behind Lugh guard.
//
//   npx lugh-lang protect            show the plan, then ask before changing anything
//   npx lugh-lang protect --yes      apply without asking
//   npx lugh-lang protect --undo     put every config back the way it was
//
// It finds MCP configs for Claude Code, Claude Desktop, Cursor, VS Code and
// Windsurf, and rewrites each local (stdio) server entry from
//   "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "~/docs"]
// to
//   "command": "npx", "args": ["-y", "lugh-lang", "guard", "--policy", "filesystem", "--",
//                              "npx", "-y", "@modelcontextprotocol/server-filesystem", "~/docs"]
// Nothing else in the file changes. A copy of each file is saved next to it
// first (<file>.lugh-backup), and --undo unwraps the entries in place, so later
// edits you made to the file are kept.

import { copyFileSync, existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";

// Where each client keeps its MCP servers. `key` is the object holding the
// servers; `projects` marks Claude Code's per-project ("local") servers.
export function configLocations({ home = homedir(), cwd = process.cwd(), platform = process.platform, env = process.env } = {}) {
  const appData = env.APPDATA || join(home, "AppData", "Roaming");
  const appSupport = platform === "darwin" ? join(home, "Library", "Application Support")
    : platform === "win32" ? appData : join(home, ".config");
  const list = [
    { client: "Claude Code (user and local)", file: join(home, ".claude.json"), key: "mcpServers", projects: true },
    { client: "Claude Code (this project)", file: join(cwd, ".mcp.json"), key: "mcpServers" },
    { client: "Claude Desktop", file: join(appSupport, "Claude", "claude_desktop_config.json"), key: "mcpServers" },
    { client: "Cursor", file: join(home, ".cursor", "mcp.json"), key: "mcpServers" },
    { client: "Cursor (this project)", file: join(cwd, ".cursor", "mcp.json"), key: "mcpServers" },
    { client: "VS Code", file: join(appSupport, "Code", "User", "mcp.json"), key: "servers" },
    { client: "VS Code (this project)", file: join(cwd, ".vscode", "mcp.json"), key: "servers" },
    { client: "Windsurf", file: join(home, ".codeium", "windsurf", "mcp_config.json"), key: "mcpServers" },
  ];
  const seen = new Set();
  return list.filter((l) => { const f = resolve(l.file); if (seen.has(f)) return false; seen.add(f); return true; });
}

// Which bundled policy fits a server, from its command line.
export function packFor(entry) {
  const s = [entry.command, ...(entry.args || [])].join(" ").toLowerCase();
  if (s.includes("server-filesystem")) return "filesystem";
  if (s.includes("github-mcp-server") || s.includes("server-github") || s.includes("githubcopilot.com/mcp")) return "github";
  if (s.includes("supabase")) return "supabase";
  if (s.includes("mcp.stripe.com") || s.includes("@stripe/mcp")) return "stripe";
  if (s.includes("@playwright/mcp") || s.includes("playwright-mcp")) return "playwright";
  if (s.includes("postgres-mcp") || s.includes("server-postgres") || s.includes("mcp-postgres") || s.includes("postgres-mcp-server")) return "postgres";
  if (s.includes("slack")) return "slack";
  if (s.includes("mcp.notion.com") || s.includes("notion-mcp-server")) return "notion";
  return "default";
}

const isLugh = (e) => [e.command, ...(e.args || [])].some((a) => typeof a === "string" && /(^|[/@])lugh-lang(@[\w.]+)?$/.test(a));

// The guard's own arguments inside a wrapped entry: [... "guard", "--policy", P, (flags), "--", cmd, ...args]
function unwrapArgs(entry) {
  const args = entry.args || [];
  const g = args.indexOf("guard");
  const sep = args.indexOf("--", g);
  if (g < 0 || sep < 0 || !args.slice(0, g).some((a) => /lugh-lang/.test(a))) return null;
  const inner = args.slice(sep + 1);
  if (!inner.length) return null;
  return { command: inner[0], args: inner.slice(1) };
}

export function wrapEntry(entry, pack, platform = process.platform) {
  const inner = [entry.command, ...(entry.args || [])];
  const guard = ["-y", "lugh-lang", "guard", "--policy", pack, "--", ...inner];
  return platform === "win32"
    ? { ...entry, command: "cmd", args: ["/c", "npx", ...guard] }
    : { ...entry, command: "npx", args: guard };
}

export function unwrapEntry(entry) {
  const inner = unwrapArgs(entry);
  return inner ? { ...entry, ...inner } : null;
}

// Every server block in one parsed config, with a path for messages.
function blocks(json, loc) {
  const out = [];
  if (json && typeof json[loc.key] === "object" && json[loc.key]) out.push({ where: "", servers: json[loc.key] });
  if (loc.projects && json && typeof json.projects === "object") {
    for (const [path, p] of Object.entries(json.projects)) {
      if (p && typeof p.mcpServers === "object" && p.mcpServers && Object.keys(p.mcpServers).length) out.push({ where: ` [${path}]`, servers: p.mcpServers });
    }
  }
  return out;
}

// Work out what would change. Returns one item per config file that has servers.
export function plan({ undo = false, platform = process.platform, ...where } = {}) {
  const files = [];
  for (const loc of configLocations({ platform, ...where })) {
    if (!existsSync(loc.file)) continue;
    let json;
    try { json = JSON.parse(readFileSync(loc.file, "utf8")); } catch (e) {
      files.push({ ...loc, error: `not valid JSON (${e.message}); left alone` });
      continue;
    }
    const changes = [];
    for (const b of blocks(json, loc)) {
      for (const [name, entry] of Object.entries(b.servers)) {
        if (!entry || typeof entry !== "object") continue;
        if (undo) {
          const un = unwrapEntry(entry);
          if (un) changes.push({ name: name + b.where, servers: b.servers, key: name, to: un, note: "guard removed" });
          continue;
        }
        if (entry.url || (entry.type && entry.type !== "stdio")) { changes.push({ name: name + b.where, skip: "remote server: guard runs local (stdio) servers; see docs/GUARD.md for mcp-remote" }); continue; }
        if (!entry.command) continue;
        if (unwrapArgs(entry)) { changes.push({ name: name + b.where, skip: "already guarded" }); continue; }
        if (isLugh(entry)) { changes.push({ name: name + b.where, skip: "Lugh's own server" }); continue; }
        const pack = packFor(entry);
        changes.push({ name: name + b.where, servers: b.servers, key: name, to: wrapEntry(entry, pack, platform), note: `guard, policy ${pack}` });
      }
    }
    if (changes.length) files.push({ ...loc, json, changes });
  }
  return files;
}

// Write the planned changes: backup first, then an atomic replace.
export function apply(files) {
  let changed = 0;
  for (const f of files) {
    const todo = (f.changes || []).filter((c) => c.to);
    if (!todo.length) continue;
    for (const c of todo) c.servers[c.key] = c.to;
    const backup = f.file + ".lugh-backup";
    copyFileSync(f.file, backup);
    const tmp = f.file + ".lugh-tmp";
    writeFileSync(tmp, JSON.stringify(f.json, null, 2) + "\n");
    renameSync(tmp, f.file);
    changed += todo.length;
  }
  return changed;
}

function describe(files, undo) {
  if (!files.length) return "No MCP servers found in Claude Code, Claude Desktop, Cursor, VS Code or Windsurf configs.";
  const lines = [];
  for (const f of files) {
    lines.push(`${f.client}: ${f.file}`);
    if (f.error) { lines.push(`  ! ${f.error}`); continue; }
    for (const c of f.changes) lines.push(c.to ? `  ✓ ${c.name}: ${c.note}` : `  · ${c.name}: ${c.skip}`);
  }
  return lines.join("\n");
}

export async function main(argv) {
  const undo = argv.includes("--undo");
  const yes = argv.includes("--yes") || argv.includes("-y");
  const files = plan({ undo });
  const count = files.flatMap((f) => f.changes || []).filter((c) => c.to).length;
  console.log(describe(files, undo));
  if (!count) {
    console.log(undo ? "\nNothing to undo." : "\nNothing to change.");
    return;
  }
  console.log(undo
    ? `\n${count} server(s) will go back to running without guard.`
    : `\n${count} server(s) will run behind Lugh guard. Reads run, risky actions ask you, destructive ones are refused.\nA copy of each file is saved as <file>.lugh-backup; undo any time with: npx lugh-lang protect --undo`);
  if (!yes) {
    if (!process.stdin.isTTY) { console.log("\nRun again with --yes to apply."); return; }
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    const answer = await new Promise((r) => rl.question("\nApply these changes? [y/N] ", r));
    rl.close();
    if (!/^y(es)?$/i.test(answer.trim())) { console.log("No changes made."); return; }
  }
  const n = apply(files);
  console.log(`\nDone: ${n} server(s) ${undo ? "unguarded" : "guarded"}. Restart your MCP clients to pick up the change.`);
  if (!undo) console.log("Every decision is logged to ~/.lugh/guard-audit.jsonl. Policies: https://github.com/Ashutosh3142857/lugh-lang/blob/main/docs/GUARD.md");
}
