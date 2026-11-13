// Smoke test for the browser build: load lugh.wasm exactly as the playground page
// does and run programs through it. Run: node playground/test-wasm.mjs
import { readFileSync } from "node:fs";
import assert from "node:assert";

const bytes = readFileSync(new URL("./lugh.wasm", import.meta.url));
const { instance } = await WebAssembly.instantiate(bytes, { env: { lugh_now_ms: () => Date.now() } });
const w = instance.exports;

function run(src, mode = 0) {
  const input = new TextEncoder().encode(src);
  const ptr = w.lugh_alloc(input.length);
  new Uint8Array(w.memory.buffer, ptr, input.length).set(input);
  const n = w.lugh_run(ptr, input.length, mode);
  w.lugh_free(ptr, input.length);
  return new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.lugh_output_ptr(), n).slice());
}

const out = run('agent A\n  goal "g"\n    let xs = [\n      1, 2, 3\n    ]\n    log "sum={sum(xs)} now={text.length(now())}"\n    log crypto.sha256("abc")\n');
assert.match(out, /sum=6 now=20/, out);
assert.match(out, /ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad/, out);
assert.match(run('agent A\n  test "t"\n    expect upper("a") == "A"\n', 1), /1 passed, 0 failed/);
assert.match(run('agent A\n  use shell\n  goal "g"\n    log 1\n'), /disabled in the playground/);
assert.match(run('agent A\n  goal "g"\n    log text.nope(1)\n', 2), /unknown function/);
const t0 = Date.now();
assert.match(run('agent A\n  budget time 1s\n  goal "g"\n    let a = range(100000)\n    let p = pairs x from a, y from a where x == y\n'), /time budget/);
assert.ok(Date.now() - t0 < 5000, "budget stops runaway code in the browser too");
console.log(`✓ lugh.wasm (${(bytes.length / 1024).toFixed(0)} KB): runs goals and tests, refuses adapters, reports compile errors, enforces the time budget`);
