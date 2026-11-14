// The npm package, the MCP registry entry and Lugh Core must carry the same version.
import assert from "node:assert";
import { readFileSync } from "node:fs";

const read = p => readFileSync(new URL(p, import.meta.url), "utf8");
const pkg = JSON.parse(read("../package.json"));
const server = JSON.parse(read("../server.json"));
const cargo = read("../../core/Cargo.toml").match(/^version = "([^"]+)"/m)[1];
assert.equal(pkg.version, cargo, "npm/package.json and core/Cargo.toml versions differ");
assert.equal(server.version, cargo, "npm/server.json version differs");
assert.equal(server.packages[0].version, cargo, "npm/server.json package version differs");
assert.equal(server.name, pkg.mcpName, "server.json name must equal package.json mcpName");
assert.equal(server.packages[0].identifier, pkg.name);
assert.ok(server.description.length <= 100, "the MCP registry limits descriptions to 100 characters");
console.log(`✓ versions agree: ${cargo}`);
