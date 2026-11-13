# Lugh for VS Code

Language support for [Lugh](https://github.com/Ashutosh3142857/lugh-lang), the
programming language for AI agents.

- **Highlighting** for keywords, adapters, effects, string interpolation and page elements
- **Live errors** as you type, from the Lugh compiler (`lugh --lint`)
- **Snippets**: `agent`, `goal`, `test`, `route`, `page`, `when`, `attempt`, `each`, `filter`, `ask`, `every`
- **Commands**: Run File, Run Tests, Show What This Program Can Do, Serve Routes and Pages

## Requirements

Lugh Core on your `PATH`, or set `lugh.path`:

```bash
git clone https://github.com/Ashutosh3142857/lugh-lang && cd lugh-lang/core && cargo build --release
```

## Install from source

```bash
cd editors/vscode
npx @vscode/vsce package
code --install-extension lugh-0.1.0.vsix
```
