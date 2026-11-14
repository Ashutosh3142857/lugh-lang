# Working on Lugh

Guidance for coding agents and contributors changing this repository.

## Layout

| Path | What it is |
| --- | --- |
| `core/` | Lugh Core, the Rust runtime (library in `src/lib.rs`, CLI in `src/main.rs`) |
| `lugh.py` | The Python reference runtime (core language and v0.2 primitives) |
| `examples/` | Example programs; several are run by the test suite |
| `docs/` | Specification, standard library, MCP and research notes |
| `playground/` | The browser playground page and its WebAssembly smoke test |
| `editors/vscode/` | The VS Code extension and its tests |

## Before you open a pull request

```bash
cd core
cargo fmt
cargo clippy --release --all-targets -- -D warnings
cargo test --release
cargo rustc --release --lib --no-default-features --crate-type cdylib   # browser build still compiles
cd ..
./core/target/release/lugh examples/stdlib_test.lugh --test
node editors/vscode/test/grammar.test.js
```

## Rules

- Keep both runtimes giving identical results for the features they share.
- Anything that touches the outside world is an adapter, and each adapter verb
  declares exactly one effect. Never add a way to act without passing the policy gate.
- Standard-library functions stay pure: no files, network or clock-dependent side effects.
- Code behind the `native` feature may use the operating system; everything else
  must also build for `wasm32-unknown-unknown`.
- Add a test with every fix. Expected values for library functions should be
  checked against an independent implementation (for example Python's standard library).
- Never commit secrets. Keys belong in the environment or a `.env` file.
