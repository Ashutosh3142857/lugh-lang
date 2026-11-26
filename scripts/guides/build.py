"""Render docs/guides/*.md into playground/guides/*.html, plus an index,
and sitemap.xml for the GitHub Pages site.

    python3 scripts/guides/build.py
"""
import html, json, pathlib, re, markdown

ROOT = pathlib.Path(__file__).resolve().parents[2]
SRC = ROOT / "docs" / "guides"
OUT = ROOT / "playground" / "guides"
SITE = "https://ashutosh3142857.github.io/lugh-lang/"
REPO = "https://github.com/Ashutosh3142857/lugh-lang"

CSS = """
:root{--bg:#fbfbf9;--ink:#1d1d1b;--muted:#64645e;--line:#e4e3dc;--code:#f1f0ea;--accent:#2f6f55}
@media (prefers-color-scheme:dark){:root{--bg:#121211;--ink:#ecebe6;--muted:#9a9993;--line:#2c2c29;--code:#1d1d1b;--accent:#7fc4a4}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:17px/1.65 system-ui,-apple-system,Segoe UI,sans-serif}
main{max-width:760px;margin:0 auto;padding:28px 20px 64px}
nav{font-size:14px;color:var(--muted);margin-bottom:28px}nav a{color:var(--muted)}
h1{font-size:30px;line-height:1.25;margin:0 0 18px}h2{font-size:21px;margin:36px 0 10px}
a{color:var(--accent)}code{font:14.5px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;background:var(--code);padding:1px 5px;border-radius:5px}
pre{background:var(--code);padding:14px 16px;border-radius:10px;overflow-x:auto}pre code{padding:0;background:none}
table{border-collapse:collapse;width:100%;font-size:15px;margin:12px 0}th,td{border-bottom:1px solid var(--line);padding:8px 10px;text-align:left;vertical-align:top}
.cta{margin-top:40px;padding:18px 20px;border:1px solid var(--line);border-radius:12px;font-size:15.5px}
.list a{display:block;padding:14px 0;border-bottom:1px solid var(--line);text-decoration:none;color:var(--ink)}.list a b{color:var(--accent)}.list a span{display:block;color:var(--muted);font-size:15px}
footer{margin-top:48px;font-size:13px;color:var(--muted)}
"""

def page(title, desc, url, body, ld):
    return f"""<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{html.escape(title)}</title>
<meta name="description" content="{html.escape(desc)}">
<link rel="canonical" href="{url}">
<meta property="og:title" content="{html.escape(title)}">
<meta property="og:description" content="{html.escape(desc)}">
<meta property="og:url" content="{url}">
<meta property="og:type" content="article">
<meta property="og:image" content="https://raw.githubusercontent.com/Ashutosh3142857/lugh-lang/main/docs/img/how-it-works.gif">
<script type="application/ld+json">{json.dumps(ld)}</script>
<style>{CSS}</style></head>
<body><main>
<nav><a href="{SITE}">Lugh</a> · <a href="{SITE}guides/">Guides</a> · <a href="{REPO}">GitHub</a></nav>
{body}
<footer>Lugh is MIT licensed. <a href="{REPO}">Source, issues and docs on GitHub</a>.</footer>
</main></body></html>
"""

def front(text):
    m = re.match(r"---\n(.*?)\n---\n", text, re.S)
    meta = dict(line.split(": ", 1) for line in m.group(1).splitlines())
    return meta, text[m.end():]

CTA = f"""<div class="cta"><b>Try it:</b> <code>npx lugh-lang protect</code> wraps every MCP server in your
Claude Code, Cursor, VS Code, Claude Desktop and Windsurf configs, shows the change first, and
<code>--undo</code> reverts it. <a href="{REPO}/blob/main/docs/GUARD.md">How guard works</a>.</div>"""

OUT.mkdir(parents=True, exist_ok=True)
guides = []
for md in sorted(SRC.glob("*.md")):
    meta, body = front(md.read_text())
    url = f"{SITE}guides/{meta['slug']}.html"
    content = markdown.markdown(body, extensions=["fenced_code", "tables"]) + CTA
    ld = {"@context": "https://schema.org", "@type": "TechArticle", "headline": meta["title"],
          "description": meta["description"], "url": url, "author": {"@type": "Organization", "name": "Lugh"}}
    (OUT / f"{meta['slug']}.html").write_text(page(meta["title"] + " · Lugh", meta["description"], url, content, ld))
    guides.append((meta, url))

items = "\n".join(f'<a href="{m["slug"]}.html"><b>{html.escape(m["title"])}</b><span>{html.escape(m["description"])}</span></a>' for m, _ in guides)
idx = f"<h1>Guides: keeping AI agents in bounds</h1><p>Short, practical fixes for agents that use MCP servers in Claude Code, Cursor and VS Code.</p><div class=\"list\">{items}</div>" + CTA
(OUT / "index.html").write_text(page("Guides: keeping AI agents in bounds · Lugh",
    "Practical guides for stopping AI agents from running destructive MCP tool calls, asking before they send or buy, and logging what they do.",
    SITE + "guides/", idx, {"@context": "https://schema.org", "@type": "CollectionPage", "url": SITE + "guides/"}))

urls = [SITE, SITE + "guides/"] + [u for _, u in guides]
(ROOT / "playground" / "sitemap.xml").write_text('<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
    + "".join(f"  <url><loc>{u}</loc></url>\n" for u in urls) + "</urlset>\n")
print("\n".join(urls))
