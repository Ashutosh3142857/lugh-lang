# Working on Lugh

Guidance for coding agents and contributors changing this repository.

## Layout

| Path | What it is |
| --- | --- |
| `core/` | Lugh Core, the Rust runtime (library in `src/lib.rs`, CLI in `src/main.rs`) |
| `lugh.py` | The Python reference runtime (core language and v0.2 primitives) |
| `examples/` | Example programs; several are run by the test suite |
| `docs/` | Specification, standard library, MCP and research notes |
| `playground/` | The browser playground page, its examples, the `run.mjs` command-line runner and the WebAssembly smoke test |
| `editors/vscode/` | The VS Code extension and its tests |
| `skills/lugh/` | An Agent Skill that teaches coding agents to write and dry-run Lugh |
| `plugins/lugh/`, `.claude-plugin/` | The Claude Code plugin and its marketplace listing (keep `plugins/lugh/skills/lugh/SKILL.md` a copy of `skills/lugh/SKILL.md`) |
| `templates/` | Index of starter agents with playground links |
| `npm/` | The `lugh-lang` npm package: sandbox CLI and MCP server over the WebAssembly build |

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
python3 playground/sync_examples.py   # after editing playground/examples/
```

To check a Lugh program you wrote without building anything, run it in the
playground sandbox: `npx lugh-lang run program.lugh` (or `node playground/run.mjs`).

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
