// Checks every grammar pattern compiles, and that key tokens of real Lugh code
// get the expected scopes. Run: node test/grammar.test.js
"use strict";
const assert = require("assert");
const g = require("../syntaxes/lugh.tmLanguage.json");

const rules = [];
(function collect(node, name) {
  if (!node || typeof node !== "object") return;
  for (const key of ["match", "begin", "end"]) {
    if (typeof node[key] === "string") rules.push({ name: node.name || name, key, re: node[key] });
  }
  for (const v of Object.values(node)) if (typeof v === "object") collect(v, node.name || name);
})(g, "root");
for (const r of rules) new RegExp(r.re, "u"); // throws if a pattern is invalid
console.log(`✓ ${rules.length} patterns compile`);

const scopeOf = (repo, text) => {
  const node = g.repository[repo];
  const list = node.patterns || [node];
  // Like the TextMate engine: the match that starts earliest in the line wins.
  let best = null;
  for (const p of list) {
    if (!p.match) continue;
    const m = new RegExp(p.match, "u").exec(text);
    if (m && (!best || m.index < best.index)) {
      best = { index: m.index, scope: p.name || (p.captures && Object.values(p.captures).map((c) => c.name).join(" ")), text: m[0] };
    }
  }
  return best;
};
const cases = [
  ["declaration", "agent FareDesk", "keyword.declaration.agent.lugh entity.name.type.agent.lugh"],
  ["comment", "  # a note", "comment.line.number-sign.lugh"],
  ["adapter-call", "    mcp.call \"notes\"", "support.class.adapter.lugh support.function.adapter.lugh"],
  ["function-call", "log text.upper(x)", "entity.name.function.lugh"],
  ["effect", "    allow read, write", "support.constant.effect.lugh"],
  ["keyword", "    ensure x else stop", "keyword.control.lugh"],
  ["keyword", "filter f in xs where f.price < 3", "keyword.operator.word.lugh"],
  ["keyword", "    deny delete", "keyword.other.policy.lugh"],
  ["number", "price: 41800", "constant.numeric.lugh"],
  ["constant", "let x = none", "constant.language.lugh"],
  ["page-element", "    table from \"/api\"", "support.type.page-element.lugh"],
  ["http-method", "  route POST \"/api/book\"", "keyword.other.http-method.lugh"],
];
for (const [repo, text, want] of cases) {
  const got = scopeOf(repo, text);
  assert.ok(got, `${repo}: no match in ${JSON.stringify(text)}`);
  assert.strictEqual(got.scope, want, `${repo} on ${JSON.stringify(text)}`);
}
// Effects are only highlighted in effect lists, not as ordinary words.
assert.strictEqual(scopeOf("effect", "let read_count = 3"), null);
// Interpolation opens on "{name" but not on "{ " (plain braces in text).
const interp = new RegExp(g.repository.string.patterns[1].begin, "u");
assert.ok(interp.test("{best.price}") && !interp.test("{ spaced }"));
console.log(`✓ ${cases.length + 2} tokens scoped as expected`);
