// Tests for `lugh-lang protect`: wrap every local MCP server in guard, and undo it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { plan, apply, packFor, wrapEntry, unwrapEntry } from "../lib/protect.mjs";

const BIN = join(fileURLToPath(new URL(".", import.meta.url)), "..", "bin", "lugh-lang.mjs");

function world() {
  const home = mkdtempSync(join(tmpdir(), "lugh-home-"));
  const cwd = mkdtempSync(join(tmpdir(), "lugh-proj-"));
  const claude = {
    numStartups: 12, // unrelated Claude Code state must survive
    mcpServers: {
      files: { command: "npx", args: ["-y", "@modelcontextprotocol/server-filesystem", "/srv/docs"], env: { A: "1" } },
      remote: { type: "http", url: "https://mcp.example.com/mcp" },
      lugh: { command: "npx", args: ["-y", "lugh-lang", "mcp"] },
    },
    projects: { "/work/app": { mcpServers: { db: { command: "npx", args: ["-y", "mcp-remote", "https://mcp.supabase.com/mcp?project_ref=x"] } }, allowedTools: ["x"] } },
  };
  writeFileSync(join(home, ".claude.json"), JSON.stringify(claude, null, 2));
  mkdirSync(join(home, ".config", "Claude"), { recursive: true });
  writeFileSync(join(home, ".config", "Claude", "claude_desktop_config.json"), JSON.stringify({ mcpServers: { gh: { command: "docker", args: ["run", "-i", "--rm", "ghcr.io/github/github-mcp-server"] } } }));
  mkdirSync(join(cwd, ".vscode"), { recursive: true });
  writeFileSync(join(cwd, ".vscode", "mcp.json"), JSON.stringify({ servers: { tool: { type: "stdio", command: "python3", args: ["server.py"] } } }));
  mkdirSync(join(home, ".cursor"), { recursive: true });
  writeFileSync(join(home, ".cursor", "mcp.json"), "{ not json");
  return { home, cwd };
}
const read = (f) => JSON.parse(readFileSync(f, "utf8"));

test("packs are chosen from the server's command line", () => {
  assert.equal(packFor({ command: "npx", args: ["-y", "@modelcontextprotocol/server-filesystem", "/x"] }), "filesystem");
  assert.equal(packFor({ command: "docker", args: ["run", "ghcr.io/github/github-mcp-server"] }), "github");
  assert.equal(packFor({ command: "npx", args: ["mcp-remote", "https://mcp.supabase.com/mcp"] }), "supabase");
  assert.equal(packFor({ command: "npx", args: ["mcp-remote", "https://mcp.stripe.com"] }), "stripe");
  assert.equal(packFor({ command: "python3", args: ["server.py"] }), "default");
});

test("wrap and unwrap are exact inverses, on every platform", () => {
  const e = { command: "npx", args: ["-y", "pkg", "--flag", "--"], env: { K: "v" }, cwd: "/x" };
  for (const platform of ["linux", "darwin", "win32"]) {
    const w = wrapEntry(e, "default", platform);
    assert.deepEqual(unwrapEntry(w), e);
  }
  assert.deepEqual(wrapEntry(e, "default", "linux").args.slice(0, 6), ["-y", "lugh-lang", "guard", "--policy", "default", "--"]);
  assert.equal(wrapEntry(e, "default", "win32").command, "cmd");
  assert.equal(unwrapEntry(e), null);
});

test("protect wraps every local server, skips the rest, and keeps unrelated settings", () => {
  const w = world();
  const files = plan({ ...w, platform: "linux" });
  const byClient = Object.fromEntries(files.map((f) => [f.client, f]));
  const cc = byClient["Claude Code (user and local)"].changes.map((c) => `${c.name}:${c.to ? c.note : c.skip}`);
  assert.deepEqual(cc, [
    "files:guard, policy filesystem",
    "remote:remote server: guard runs local (stdio) servers; see docs/GUARD.md for mcp-remote",
    "lugh:Lugh's own server",
    "db [/work/app]:guard, policy supabase",
  ]);
  assert.match(byClient["Cursor"].error, /not valid JSON/);
  assert.equal(apply(files), 4);

  const claude = read(join(w.home, ".claude.json"));
  assert.equal(claude.numStartups, 12);
  assert.deepEqual(claude.projects["/work/app"].allowedTools, ["x"]);
  assert.deepEqual(claude.mcpServers.files.args, ["-y", "lugh-lang", "guard", "--policy", "filesystem", "--", "npx", "-y", "@modelcontextprotocol/server-filesystem", "/srv/docs"]);
  assert.deepEqual(claude.mcpServers.files.env, { A: "1" });
  assert.deepEqual(claude.mcpServers.remote, { type: "http", url: "https://mcp.example.com/mcp" });
  assert.equal(read(join(w.home, ".config", "Claude", "claude_desktop_config.json")).mcpServers.gh.args[4], "github");
  assert.equal(read(join(w.cwd, ".vscode", "mcp.json")).servers.tool.args[4], "default");
  assert.ok(existsSync(join(w.home, ".claude.json.lugh-backup")));
  assert.equal(readFileSync(join(w.home, ".cursor", "mcp.json"), "utf8"), "{ not json");

  // running again changes nothing
  const again = plan({ ...w, platform: "linux" });
  assert.equal(again.flatMap((f) => f.changes || []).filter((c) => c.to).length, 0);
});

test("undo restores the original entries and keeps later edits", () => {
  const w = world();
  const before = read(join(w.home, ".claude.json"));
  apply(plan({ ...w, platform: "linux" }));
  const edited = read(join(w.home, ".claude.json"));
  edited.numStartups = 13; // the client kept writing to the file afterwards
  writeFileSync(join(w.home, ".claude.json"), JSON.stringify(edited));
  assert.equal(apply(plan({ ...w, undo: true, platform: "linux" })), 4);
  const after = read(join(w.home, ".claude.json"));
  assert.deepEqual(after.mcpServers, before.mcpServers);
  assert.deepEqual(after.projects, before.projects);
  assert.equal(after.numStartups, 13);
});

test("the CLI shows the plan and changes nothing without --yes", () => {
  const w = world();
  const original = readFileSync(join(w.home, ".claude.json"), "utf8");
  const out = execFileSync(process.execPath, [BIN, "protect"], { cwd: w.cwd, env: { ...process.env, HOME: w.home, XDG_CONFIG_HOME: "" }, input: "" }).toString();
  assert.match(out, /files: guard, policy filesystem/);
  assert.match(out, /Run again with --yes to apply/);
  assert.equal(readFileSync(join(w.home, ".claude.json"), "utf8"), original);
  const done = execFileSync(process.execPath, [BIN, "protect", "--yes"], { cwd: w.cwd, env: { ...process.env, HOME: w.home } }).toString();
  assert.match(done, /Done: \d+ server\(s\) guarded/);
});
