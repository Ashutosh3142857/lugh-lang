// Lugh for VS Code: live error checking via `lugh <file> --lint`, and commands
// that run, test, check and serve the current file in a terminal.
"use strict";
const vscode = require("vscode");
const { execFile } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

let diagnostics;
let warnedMissing = false;
const timers = new Map();

function lughPath() {
  return vscode.workspace.getConfiguration("lugh").get("path") || "lugh";
}

/** Compile the document's current text and turn errors into diagnostics. */
function check(doc) {
  if (doc.languageId !== "lugh") return;
  const tmp = path.join(os.tmpdir(), `lugh-lint-${process.pid}-${Buffer.from(doc.uri.toString()).toString("hex").slice(-16)}.lugh`);
  fs.writeFileSync(tmp, doc.getText());
  execFile(lughPath(), [tmp, "--lint"], { timeout: 10000, cwd: path.dirname(doc.uri.fsPath || tmp) }, (err, stdout) => {
    fs.unlink(tmp, () => {});
    if (err && err.code === "ENOENT") {
      if (!warnedMissing) {
        warnedMissing = true;
        vscode.window.showWarningMessage("Lugh: the `lugh` executable was not found. Build Lugh Core and set `lugh.path`.");
      }
      return;
    }
    let result;
    try {
      result = JSON.parse(stdout);
    } catch {
      return;
    }
    const items = (result.errors || []).map((e) => {
      const line = Math.max(0, Math.min(doc.lineCount - 1, (e.line || 1) - 1));
      const text = doc.lineAt(line);
      const range = new vscode.Range(line, text.firstNonWhitespaceCharacterIndex, line, text.text.length);
      const d = new vscode.Diagnostic(range, e.message, vscode.DiagnosticSeverity.Error);
      d.source = "lugh";
      return d;
    });
    diagnostics.set(doc.uri, items);
  });
}

function schedule(doc) {
  if (doc.languageId !== "lugh") return;
  clearTimeout(timers.get(doc.uri.toString()));
  timers.set(doc.uri.toString(), setTimeout(() => check(doc), 400));
}

function inTerminal(args) {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== "lugh") {
    vscode.window.showInformationMessage("Open a .lugh file first.");
    return;
  }
  editor.document.save().then(() => {
    const term = vscode.window.terminals.find((t) => t.name === "Lugh") || vscode.window.createTerminal("Lugh");
    term.show();
    const quote = (s) => (/[\s"']/.test(s) ? JSON.stringify(s) : s);
    term.sendText([lughPath(), editor.document.uri.fsPath, ...args].map(quote).join(" "));
  });
}

function activate(context) {
  diagnostics = vscode.languages.createDiagnosticCollection("lugh");
  context.subscriptions.push(
    diagnostics,
    vscode.workspace.onDidOpenTextDocument(check),
    vscode.workspace.onDidSaveTextDocument(check),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (vscode.workspace.getConfiguration("lugh").get("checkOnType")) schedule(e.document);
    }),
    vscode.workspace.onDidCloseTextDocument((doc) => diagnostics.delete(doc.uri)),
    vscode.commands.registerCommand("lugh.run", () => inTerminal([])),
    vscode.commands.registerCommand("lugh.test", () => inTerminal(["--test"])),
    vscode.commands.registerCommand("lugh.check", () => inTerminal(["--check"])),
    vscode.commands.registerCommand("lugh.serve", () => inTerminal(["--serve", "8080"]))
  );
  vscode.workspace.textDocuments.forEach(check);
}

function deactivate() {}

module.exports = { activate, deactivate, check };
