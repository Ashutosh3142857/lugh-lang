// Runs the extension against a stand-in for the VS Code API and the real `lugh`
// binary, and checks that compile errors become diagnostics on the right lines.
// Run: LUGH=/path/to/lugh node test/extension.test.js
"use strict";
const assert = require("assert");
const Module = require("module");

const set = new Map();
const commands = {};
let config = { path: process.env.LUGH || "lugh", checkOnType: true };
class Range { constructor(sl, sc, el, ec) { Object.assign(this, { sl, sc, el, ec }); } }
class Diagnostic { constructor(range, message, severity) { Object.assign(this, { range, message, severity }); } }
const fake = {
  Range, Diagnostic, DiagnosticSeverity: { Error: 0 },
  languages: { createDiagnosticCollection: () => ({ set: (u, d) => set.set(u.toString(), d), delete: (u) => set.delete(u.toString()), dispose() {} }) },
  workspace: {
    getConfiguration: () => ({ get: (k) => config[k] }),
    onDidOpenTextDocument: () => ({}), onDidSaveTextDocument: () => ({}),
    onDidChangeTextDocument: () => ({}), onDidCloseTextDocument: () => ({}),
    textDocuments: [],
  },
  window: { showWarningMessage: (m) => { fake.warned = m; }, showInformationMessage() {}, terminals: [], activeTextEditor: null },
  commands: { registerCommand: (name, fn) => { commands[name] = fn; return {}; } },
};
const load = Module._load;
Module._load = (req, ...rest) => (req === "vscode" ? fake : load(req, ...rest));
const ext = require("../extension.js");
ext.activate({ subscriptions: [] });

const doc = (uri, text) => {
  const lines = text.split("\n");
  return {
    uri: { toString: () => uri, fsPath: "/tmp/" + uri }, languageId: "lugh", lineCount: lines.length,
    getText: () => text,
    lineAt: (i) => ({ text: lines[i], firstNonWhitespaceCharacterIndex: lines[i].length - lines[i].trimStart().length }),
  };
};
const wait = () => new Promise((r) => setTimeout(r, 1500));

(async () => {
  assert.deepStrictEqual(Object.keys(commands).sort(), ["lugh.check", "lugh.run", "lugh.serve", "lugh.test"]);

  ext.check(doc("bad.lugh", 'agent A\n  goal "g"\n    log upper("x")\n    log text.nope(1)\n'));
  await wait();
  const bad = set.get("bad.lugh");
  assert.strictEqual(bad.length, 1);
  assert.strictEqual(bad[0].range.sl, 3, "error on line 4 (index 3)");
  assert.strictEqual(bad[0].range.sc, 4, "underline starts at the first non-space character");
  assert.match(bad[0].message, /unknown function 'text.nope'/);

  ext.check(doc("good.lugh", 'agent A\n  goal "g"\n    log upper("x")\n'));
  await wait();
  assert.deepStrictEqual(set.get("good.lugh"), [], "a valid program clears its diagnostics");

  config.path = "/no/such/lugh";
  ext.check(doc("x.lugh", "agent A\n"));
  await wait();
  assert.match(fake.warned || "", /not found/, "a missing binary produces one clear warning");

  console.log("✓ commands registered; errors underlined on the right line; valid code clears them; missing binary warned");
})().catch((e) => { console.error(e); process.exit(1); });
