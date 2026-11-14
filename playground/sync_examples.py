#!/usr/bin/env python3
"""Embed playground/examples/*.lugh into index.html, in the order of examples/index.json.

Run after editing an example: python3 playground/sync_examples.py
The test suite checks that index.html matches the files.
"""
import html
import json
import pathlib

here = pathlib.Path(__file__).parent
items = json.loads((here / "examples" / "index.json").read_text())
blocks = []
for it in items:
    src = (here / "examples" / f"{it['id']}.lugh").read_text()
    assert "</script" not in src, it["id"]
    blocks.append(
        f'<script type="text/lugh" id="{it["id"]}" data-group="{html.escape(it["group"])}" '
        f'data-name="{html.escape(it["name"])}">\n{src}</script>'
    )
page = here / "index.html"
text = page.read_text()
a, b = text.index("<!--EXAMPLES-->"), text.index("<!--/EXAMPLES-->")
page.write_text(text[: a + len("<!--EXAMPLES-->")] + "\n" + "\n".join(blocks) + "\n" + text[b:])
print(f"embedded {len(items)} examples")
