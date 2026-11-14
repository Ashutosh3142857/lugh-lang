// Loads Lugh Core (the WebAssembly build) and runs programs in the playground
// sandbox: adapters act on a simulated world, behind the real policy gate.
import { readFileSync, existsSync } from "node:fs";

const PUBLISHED = "https://ashutosh3142857.github.io/lugh-lang/lugh.wasm";
const MODES = { run: 0, test: 1, check: 2, sandbox: 3 };

async function wasmBytes() {
  const bundled = new URL("../lugh.wasm", import.meta.url);
  if (existsSync(bundled)) return readFileSync(bundled);
  const res = await fetch(PUBLISHED);
  if (!res.ok) throw new Error(`could not download ${PUBLISHED}: HTTP ${res.status}`);
  return new Uint8Array(await res.arrayBuffer());
}

/** Returns run(source, mode = "run" | "test" | "check" | "sandbox", { decline }) → output text. */
export async function load() {
  const { instance } = await WebAssembly.instantiate(await wasmBytes(), {
    env: { lugh_now_ms: () => Date.now() },
  });
  const w = instance.exports;
  return function run(source, mode = "run", { decline = false } = {}) {
    if (!(mode in MODES)) throw new Error(`unknown mode ${mode}`);
    const input = new TextEncoder().encode(source ?? "");
    const size = Math.max(1, input.length);
    const ptr = w.lugh_alloc(size);
    new Uint8Array(w.memory.buffer, ptr, input.length).set(input);
    const n = w.lugh_run(ptr, input.length, MODES[mode] + (decline ? 4 : 0));
    w.lugh_free(ptr, size);
    return new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.lugh_output_ptr(), n).slice());
  };
}

/** True when the output reports a failed goal, test or compile error. */
export function failed(output) {
  return /^❌|^\s+✗|\b[1-9]\d* failed\b/m.test(output);
}
