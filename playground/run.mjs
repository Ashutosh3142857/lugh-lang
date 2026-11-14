#!/usr/bin/env node
// Run a Lugh program in the playground sandbox from the command line, with no
// install: the same WebAssembly build the web page uses. Useful for agents.
//
//   node run.mjs program.lugh            run its goals
//   node run.mjs program.lugh --test     run its test blocks
//   node run.mjs program.lugh --check    compile only
//   node run.mjs program.lugh --decline  answer every approval with no
//   node run.mjs --sandbox               describe the simulated world
//   cat program.lugh | node run.mjs -    read the program from stdin
//
// lugh.wasm is loaded from this folder, or downloaded from the published playground.
import { readFileSync, existsSync } from "node:fs";

const PAGES = "https://ashutosh3142857.github.io/lugh-lang/lugh.wasm";
const args = process.argv.slice(2);
const flag = f => args.includes(f);
const file = args.find(a => !a.startsWith("--") || a === "-");

async function wasmBytes() {
  const local = new URL("./lugh.wasm", import.meta.url);
  if (existsSync(local)) return readFileSync(local);
  const res = await fetch(PAGES);
  if (!res.ok) throw new Error(`could not download ${PAGES}: ${res.status}`);
  return new Uint8Array(await res.arrayBuffer());
}

const mode = flag("--sandbox") ? 3 : flag("--test") ? 1 : flag("--check") ? 2 : 0;
if (mode !== 3 && !file) {
  console.error("usage: node run.mjs <program.lugh | -> [--test | --check] [--decline]   or   node run.mjs --sandbox");
  process.exit(2);
}
const src = mode === 3 ? "" : file === "-" ? readFileSync(0, "utf8") : readFileSync(file, "utf8");
const { instance } = await WebAssembly.instantiate(await wasmBytes(), { env: { lugh_now_ms: () => Date.now() } });
const w = instance.exports;
const input = new TextEncoder().encode(src);
const ptr = w.lugh_alloc(Math.max(1, input.length));
new Uint8Array(w.memory.buffer, ptr, input.length).set(input);
const n = w.lugh_run(ptr, input.length, mode + (flag("--decline") ? 4 : 0));
w.lugh_free(ptr, Math.max(1, input.length));
const out = new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.lugh_output_ptr(), n).slice());
console.log(out.trimEnd());
// Exit 1 when something failed, so scripts and agents can branch on it.
process.exit(/^❌|^\s+✗|\b[1-9]\d* failed\b/m.test(out) ? 1 : 0);
