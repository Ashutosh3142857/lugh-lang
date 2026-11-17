#!/usr/bin/env python3
"""Collect Lugh usage numbers into stats/data.json, keeping history.

GitHub only keeps 14 days of traffic and npm/crates.io only serve recent
windows, so this runs daily and merges each day into the file.

Environment:
  GITHUB_TOKEN  repo metadata (stars, forks, watchers)
  STATS_TOKEN   optional; a token that can read repository traffic
                (fine-grained: Administration read-only on this repo).
                Without it, views and clones are skipped.
"""
import datetime as dt
import json
import os
import pathlib
import urllib.request

REPO = "Ashutosh3142857/lugh-lang"
NPM = "lugh-lang"
CRATE = "lugh"
OUT = pathlib.Path(__file__).resolve().parents[2] / "stats" / "data.json"


def get(url, token=None):
    headers = {"User-Agent": "lugh-stats", "Accept": "application/vnd.github+json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=30) as r:
        return json.load(r)


def safe(label, fn):
    try:
        return fn()
    except Exception as e:  # one source failing must not lose the others
        print(f"skip {label}: {e}")
        return None


def main():
    data = json.loads(OUT.read_text()) if OUT.exists() else {}
    today = dt.date.today().isoformat()
    gh = os.environ.get("GITHUB_TOKEN")
    traffic_token = os.environ.get("STATS_TOKEN")

    repo = safe("github repo", lambda: get(f"https://api.github.com/repos/{REPO}", gh))
    if repo:
        data.setdefault("github", {})[today] = {
            "stars": repo["stargazers_count"],
            "forks": repo["forks_count"],
            "watchers": repo["subscribers_count"],
            "open_issues": repo["open_issues_count"],
        }

    if traffic_token:
        for kind in ("views", "clones"):
            t = safe(kind, lambda: get(f"https://api.github.com/repos/{REPO}/traffic/{kind}", traffic_token))
            if t:
                series = data.setdefault(kind, {})
                data.setdefault("traffic_14d", {})[kind] = {"count": t["count"], "uniques": t["uniques"], "as_of": today}
                for d in t[kind]:
                    series[d["timestamp"][:10]] = {"count": d["count"], "uniques": d["uniques"]}
        refs = safe("referrers", lambda: get(f"https://api.github.com/repos/{REPO}/traffic/popular/referrers", traffic_token))
        if refs is not None:
            data["referrers"] = {"as_of": today, "items": refs}
    else:
        print("skip traffic: STATS_TOKEN not set")

    npm = safe("npm", lambda: get(f"https://api.npmjs.org/downloads/range/last-month/{NPM}"))
    if npm:
        series = data.setdefault("npm", {})
        for d in npm["downloads"]:
            series[d["day"]] = d["downloads"]

    crates = safe("crates", lambda: get(f"https://crates.io/api/v1/crates/{CRATE}/downloads"))
    if crates:
        per_day = {}
        for d in crates["version_downloads"]:
            per_day[d["date"]] = per_day.get(d["date"], 0) + d["downloads"]
        data.setdefault("crates", {}).update(per_day)

    rel = safe("releases", lambda: get(f"https://api.github.com/repos/{REPO}/releases", gh))
    if rel is not None:
        data["releases"] = {
            r["tag_name"]: sum(a["download_count"] for a in r["assets"]) for r in rel
        }

    for key in ("github", "views", "clones", "npm", "crates"):
        if key in data:
            data[key] = dict(sorted(data[key].items()))
    data["updated"] = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    OUT.parent.mkdir(exist_ok=True)
    OUT.write_text(json.dumps(data, indent=1) + "\n")
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
