#!/usr/bin/env python3
"""
Lugh v0.2 — a programming language for AI agents.
Named for Lugh, the Irish god "skilled in all the arts" — an agent that can use every tool.

Interpreter: parses .lugh programs, enforces policy + budget,
routes every action through adapters to existing infrastructure,
writes an audit log of every effect, and learns which routes work.

v0.2 adds primitives drawn from foundational ML research: attention-style
relevance (attend), grounded extraction (point), content-addressed memory
(recall like), gated forgetting (keep), residual refinement (refine),
ensemble consensus, adaptive route ordering (attempt auto), pipelined
loops (parallel), compression-based novelty and simplicity (MDL), pairwise
relational queries (pairs), test-time sampling (samples) and a
simplicity-weighted evaluation score (--eval). See docs/RESEARCH.md.

Usage:
    python lugh.py program.lugh            # interactive approvals
    python lugh.py program.lugh --yes      # auto-approve (for demos/CI)
    python lugh.py program.lugh --check    # parse + static policy check only
    python lugh.py program.lugh --eval     # run and report a weighted score
"""
import json, math, os, re, subprocess, sys, threading, time, urllib.request, zlib
from collections import Counter
from concurrent.futures import ThreadPoolExecutor

# ───────────────────────── 0. Information-theoretic helpers (MDL / Kolmogorov) ─────────────────────────

def _txt(v):
    return v if isinstance(v, str) else json.dumps(v, ensure_ascii=False, sort_keys=True, default=str)

def K(v):
    """Description length in bytes: a computable upper bound on Kolmogorov complexity."""
    return len(zlib.compress(_txt(v).encode("utf-8"), 9))

def ncd(a, b):
    """Normalized compression distance (0 = same information, ~1 = unrelated)."""
    ka, kb, kab = K(a), K(b), K(_txt(a) + " " + _txt(b))
    return (kab - min(ka, kb)) / max(ka, kb, 1)

def _tokens(s):
    return set(re.findall(r"[a-z0-9]+", _txt(s).lower()))

def relevance(query, item):
    """Attention-style score: token overlap blended with compression similarity."""
    q = _tokens(query)
    overlap = len(q & _tokens(item)) / max(len(q), 1)
    return 0.7 * overlap + 0.3 * (1 - min(ncd(query, item), 1))

EFFECTS = {"read", "write", "send", "spend", "delete", "submit", "execute"}

class LughError(Exception): pass
class Stop(Exception): pass
class PolicyDenied(LughError): pass
class BudgetExceeded(LughError): pass

# ───────────────────────── 1. Lexing lines into a block tree ─────────────────────────

class Line:
    def __init__(self, text, indent, no):
        self.text, self.indent, self.no, self.children = text, indent, no, []
    def __repr__(self): return f"Line({self.no}: {self.text!r}, {len(self.children)} kids)"

def build_tree(src):
    root = Line("<root>", -1, 0)
    stack = [root]
    for no, raw in enumerate(src.splitlines(), 1):
        stripped = raw.split("#", 1)[0] if not _in_string_hash(raw) else raw
        if not stripped.strip():
            continue
        indent = len(stripped) - len(stripped.lstrip(" "))
        node = Line(stripped.strip(), indent, no)
        while stack[-1].indent >= indent:
            stack.pop()
        stack[-1].children.append(node)
        stack.append(node)
    return root

def _in_string_hash(raw):
    # keep '#' that appears inside quotes (e.g. "#travel")
    i = raw.find("#")
    return i != -1 and raw[:i].count('"') % 2 == 1

# ───────────────────────── 2. Expression tokenizer + parser ─────────────────────────

TOKEN_RE = re.compile(r'''
    (?P<str>"(?:[^"\\]|\\.)*")
  | (?P<num>\d+(?:\.\d+)?)
  | (?P<op><=|>=|==|!=|->|[<>+\-*/=\[\],():])
  | (?P<name>[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)*)
  | (?P<ws>\s+)
''', re.X)

def tokenize(s):
    out, pos = [], 0
    while pos < len(s):
        m = TOKEN_RE.match(s, pos)
        if not m:
            raise LughError(f"Unexpected character {s[pos]!r} in: {s}")
        kind = m.lastgroup
        if kind != "ws":
            out.append((kind, m.group()))
        pos = m.end()
    return out

class Expr:
    """Recursive-descent parser producing nested tuples (AST)."""
    KEYWORDS = {"and", "or", "not", "as", "from", "pick", "lowest", "highest",
                "recall", "think", "with", "true", "false", "none", "count", "ask", "human"}

    def __init__(self, toks, adapters):
        self.t, self.i, self.adapters = toks, 0, adapters

    def peek(self, k=0):
        j = self.i + k
        return self.t[j] if j < len(self.t) else (None, None)
    def eat(self, val=None):
        tok = self.peek()
        if val is not None and tok[1] != val:
            raise LughError(f"Expected {val!r}, got {tok[1]!r}")
        self.i += 1
        return tok
    def done(self): return self.i >= len(self.t)

    def parse(self):
        e = self.or_()
        if not self.done():
            raise LughError(f"Unexpected {self.peek()[1]!r}")
        return e

    def or_(self):
        e = self.and_()
        while self.peek()[1] == "or":
            self.eat(); e = ("or", e, self.and_())
        return e
    def and_(self):
        e = self.not_()
        while self.peek()[1] == "and":
            self.eat(); e = ("and", e, self.not_())
        return e
    def not_(self):
        if self.peek()[1] == "not":
            self.eat(); return ("not", self.not_())
        return self.cmp()
    def cmp(self):
        e = self.add()
        if self.peek()[1] in ("<", ">", "<=", ">=", "==", "!="):
            op = self.eat()[1]; e = ("cmp", op, e, self.add())
        return e
    def add(self):
        e = self.mul()
        while self.peek()[1] in ("+", "-"):
            op = self.eat()[1]; e = ("bin", op, e, self.mul())
        return e
    def mul(self):
        e = self.primary()
        while self.peek()[1] in ("*", "/"):
            op = self.eat()[1]; e = ("bin", op, e, self.primary())
        return e

    def primary(self):
        kind, val = self.peek()
        if kind == "num":
            self.eat(); return ("lit", float(val) if "." in val else int(val))
        if kind == "str":
            self.eat(); return ("str", bytes(val[1:-1], "utf-8").decode("unicode_escape"))
        if val == "(":
            self.eat(); e = self.or_(); self.eat(")"); return e
        if val == "[":
            self.eat(); items = []
            while self.peek()[1] != "]":
                items.append(self.or_())
                if self.peek()[1] == ",": self.eat()
            self.eat("]"); return ("list", items)
        if val in ("true", "false", "none"):
            self.eat(); return ("lit", {"true": True, "false": False, "none": None}[val])
        if val == "pick":           # pick lowest price from fares
            self.eat(); mode = self.eat()[1]; field = self.eat()[1]; self.eat("from")
            return ("pick", mode, field, self.primary())
        if val == "count":          # count fares
            self.eat(); return ("count", self.primary())
        if val == "recall":         # recall "key"  |  recall like "query"
            self.eat()
            if self.peek()[1] == "like":
                self.eat(); return ("recall_like", self.primary())
            return ("recall", self.primary())
        if val == "think":          # think "prompt" [with expr] [samples N]
            self.eat(); prompt = self.primary(); ctx = None; n = ("lit", 1)
            if self.peek()[1] == "with":
                self.eat(); ctx = self.primary()
            if self.peek()[1] == "samples":
                self.eat(); n = self.primary()
            return ("think", prompt, ctx, n)
        if val == "attend":         # attend to items by "query" [top N]
            self.eat(); self.eat("to"); src = self.primary(); self.eat("by"); q = self.primary(); n = ("lit", 3)
            if self.peek()[1] == "top":
                self.eat(); n = self.primary()
            return ("attend", src, q, n)
        if val == "point":          # point to "query" in text
            self.eat(); self.eat("to"); q = self.primary(); self.eat("in")
            return ("point", q, self.primary())
        if val == "novelty":        # novelty x [against list]
            self.eat(); x = self.primary(); ref = None
            if self.peek()[1] == "against":
                self.eat(); ref = self.primary()
            return ("novelty", x, ref)
        if val == "simplest":       # simplest of list
            self.eat(); self.eat("of"); return ("simplest", self.primary())
        if val == "pairs":          # pairs a from X, b from Y where cond
            self.eat(); a = self.eat()[1]; self.eat("from"); xs = self.primary(); self.eat(",")
            b = self.eat()[1]; self.eat("from"); ys = self.primary(); self.eat("where")
            return ("pairs", a, xs, b, ys, self.or_())
        if val == "ask" and self.peek(1)[1] == "human":
            self.eat(); self.eat(); return ("ask_human", self.primary())
        if kind == "name":
            self.eat()
            head = val.split(".")[0]
            if head in self.adapters and "." in val:      # adapter call: http.get "url" as json
                args = []
                while not self.done() and self.peek()[1] not in ("as", ")", ",", "]", "and", "or") \
                        and self.peek()[1] not in ("<", ">", "<=", ">=", "==", "!="):
                    args.append(self.primary())
                cast = None
                if self.peek()[1] == "as":
                    self.eat(); cast = self.eat()[1]
                return ("call", val, args, cast)
            return ("var", val)
        raise LughError(f"Cannot parse near {val!r}")

# ───────────────────────── 3. Adapters: the bridge to existing infra ─────────────────────────

class Adapter:
    name = "base"
    effects = {}            # method -> effect class
    def effect_of(self, method):
        if method not in self.effects:
            raise LughError(f"{self.name} has no action '{method}'. Known: {sorted(self.effects)}")
        return self.effects[method]

class HttpAdapter(Adapter):
    """Any REST/JSON API on the internet."""
    name = "http"
    effects = {"get": "read", "post": "send", "put": "write", "delete": "delete"}
    def call(self, method, args):
        url = args[0]
        data = json.dumps(args[1]).encode() if len(args) > 1 else None
        req = urllib.request.Request(url, data=data, method=method.upper(),
                                     headers={"User-Agent": "lugh/0.1", "Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.read().decode()

class FsAdapter(Adapter):
    """Local files."""
    name = "fs"
    effects = {"read": "read", "list": "read", "write": "write", "append": "write", "delete": "delete"}
    def call(self, method, args):
        p = args[0]
        if method == "read":   return open(p, encoding="utf-8").read()
        if method == "list":   return sorted(os.listdir(p))
        if method == "write":  open(p, "w", encoding="utf-8").write(_to_text(args[1])); return p
        if method == "append": open(p, "a", encoding="utf-8").write(_to_text(args[1]) + "\n"); return p
        if method == "delete": os.remove(p); return p

class ShellAdapter(Adapter):
    """Any command-line software already installed."""
    name = "shell"
    effects = {"run": "execute"}
    def call(self, method, args):
        r = subprocess.run(args[0], shell=True, capture_output=True, text=True, timeout=60)
        if r.returncode != 0:
            raise LughError(f"shell exited {r.returncode}: {r.stderr.strip()[:200]}")
        return r.stdout.strip()

class BrowserAdapter(Adapter):
    """Real web browser via Playwright (Chromium). Install: pip install playwright && playwright install chromium"""
    name = "browser"
    effects = {"open": "read", "text": "read", "click": "submit", "type": "write", "screenshot": "read"}
    def __init__(self): self._page = None
    def _ensure(self):
        if self._page is None:
            try:
                from playwright.sync_api import sync_playwright
            except ImportError:
                raise LughError("browser adapter needs Playwright: pip install playwright && playwright install chromium")
            self._pw = sync_playwright().start()
            self._page = self._pw.chromium.launch(headless=True).new_page()
        return self._page
    def call(self, method, args):
        pg = self._ensure()
        if method == "open":       pg.goto(args[0]); return pg.title()
        if method == "text":       return pg.inner_text(args[0] if args else "body")
        if method == "click":      pg.click(args[0]); return True
        if method == "type":       pg.fill(args[0], str(args[1])); return True
        if method == "screenshot": pg.screenshot(path=args[0]); return args[0]

class McpAdapter(Adapter):
    """Model Context Protocol servers: Slack, Gmail, Notion, GitHub, Drive... (stub in v0.1)."""
    name = "mcp"
    effects = {"call": "send", "query": "read"}
    def call(self, method, args):
        raise LughError("mcp adapter is a stub in v0.1 — wire an MCP client (see blueprint, Roadmap phase 1)")

class AppAdapter(Adapter):
    """Desktop apps via OS accessibility APIs, with screen-vision fallback (stub in v0.1)."""
    name = "app"
    effects = {"open": "read", "read": "read", "click": "submit", "type": "write"}
    def call(self, method, args):
        raise LughError("app adapter is a stub in v0.1 — needs UIA (Windows) / AX (macOS) / AT-SPI (Linux)")

ADAPTERS = {a.name: a for a in (HttpAdapter, FsAdapter, ShellAdapter, BrowserAdapter, McpAdapter, AppAdapter)}

def _to_text(v):
    return v if isinstance(v, str) else json.dumps(v, ensure_ascii=False, indent=2)

def cast_value(v, cast):
    if cast is None: return v
    if cast == "json":   return json.loads(v) if isinstance(v, str) else v
    if cast == "number": return float(v)
    if cast == "text":   return _to_text(v)
    if cast == "lines":  return [l for l in str(v).splitlines() if l.strip()]
    raise LughError(f"Unknown type '{cast}'")

# ───────────────────────── 4. Runtime: policy, budget, audit, execution ─────────────────────────

class Runtime:
    def __init__(self, auto_yes=False, audit_path="lugh_audit.jsonl", memory_path="lugh_memory.json"):
        self.auto_yes = auto_yes
        self.audit_path, self.memory_path = audit_path, memory_path
        self.agent = "anonymous"
        self.adapters = {}
        self.policy = {"allow": set(), "ask": set(), "deny": set()}
        self.budget = {"time": 120.0, "calls": 25}     # safe defaults
        self.calls = 0
        self.lock = threading.Lock()
        self.memory = json.load(open(memory_path)) if os.path.exists(memory_path) else {}
        self.forget_expired()
        self.stats_path = "lugh_stats.json"
        self.stats = json.load(open(self.stats_path)) if os.path.exists(self.stats_path) else {}
        self.results = []

    # ---- gated memory (LSTM-style forget gate) ----
    def forget_expired(self):
        exp = self.memory.setdefault("__expires__", {})
        now = time.time()
        for key in [k for k, t in exp.items() if t < now]:
            self.memory.pop(key, None); exp.pop(key, None)

    def recall_like(self, query):
        """Content-addressed read (Neural Turing Machine style): best match by key + value."""
        items = [(k, v) for k, v in self.memory.items() if not k.startswith("__")]
        if not items: return None
        k, v = max(items, key=lambda kv: relevance(query, kv[0] + " " + _txt(kv[1])))
        if relevance(query, k + " " + _txt(v)) < 0.15: return None
        self.audit("recall_like", query=query, key=k)
        return v

    # ---- adaptive route ordering (Adam-style moment estimates) ----
    def route_score(self, key):
        s = self.stats.get(key)
        if not s: return 0.5                                   # untried routes sit in the middle
        b1, b2, t = 0.9, 0.999, s["t"]
        m_hat = s["m"] / (1 - b1 ** t)                         # bias-corrected success rate
        v_hat = s["v"] / (1 - b2 ** t)                         # bias-corrected squared latency
        return m_hat / (math.sqrt(v_hat) + 1.0)

    def route_update(self, key, ok, secs):
        s = self.stats.setdefault(key, {"m": 0.0, "v": 0.0, "t": 0})
        s["t"] += 1
        s["m"] = 0.9 * s["m"] + 0.1 * (1.0 if ok else 0.0)
        s["v"] = 0.999 * s["v"] + 0.001 * secs ** 2

    # ---- audit ----
    def audit(self, kind, **kw):
        rec = {"t": round(time.time(), 3), "agent": self.agent, "kind": kind, **kw}
        with self.lock, open(self.audit_path, "a", encoding="utf-8") as f:
            f.write(json.dumps(rec, ensure_ascii=False, default=str) + "\n")

    def say(self, icon, msg): print(f"  {icon} {msg}")

    # ---- gatekeeping: every effect passes here ----
    def gate(self, effect, desc):
        if time.time() - self.started > self.budget["time"]:
            raise BudgetExceeded(f"time budget of {self.budget['time']:.0f}s used up")
        with self.lock:
            self.calls += 1
            if self.calls > self.budget["calls"]:
                raise BudgetExceeded(f"call budget of {self.budget['calls']} used up")
        if effect in self.policy["deny"]:
            self.audit("denied", effect=effect, action=desc)
            raise PolicyDenied(f"policy denies '{effect}': {desc}")
        if effect in self.policy["allow"]:
            return
        # 'ask' list, or anything not explicitly allowed → human approval
        if not self.human_approves(f"[{effect}] {desc}"):
            self.audit("rejected", effect=effect, action=desc)
            raise PolicyDenied(f"human rejected: {desc}")

    def human_approves(self, question):
        if self.auto_yes:
            self.say("👤", f"auto-approved → {question}")
            self.audit("human", question=question, answer="yes(auto)")
            return True
        ans = input(f"  👤 APPROVE? {question} [y/N] ").strip().lower()
        self.audit("human", question=question, answer=ans)
        return ans in ("y", "yes")

    # ---- program entry ----
    def run_source(self, src, check_only=False):
        tree = build_tree(src)
        if not tree.children or not tree.children[0].text.startswith("agent "):
            raise LughError("A Lugh program must start with: agent <Name>")
        top = tree.children[0]
        self.agent = top.text.split(None, 1)[1]
        goals = []
        for node in top.children:
            head = node.text.split()[0]
            if head == "use":       self.setup_use(node.text[4:])
            elif head == "budget":  self.setup_budget(node.text[7:])
            elif head == "policy":  self.setup_policy(node)
            elif head == "goal":    goals.append(node)
            else: raise LughError(f"line {node.no}: expected use/budget/policy/goal, got '{head}'")
        if not goals: raise LughError("No goal defined")
        print(f"\n⚡ Lugh agent '{self.agent}'  adapters={sorted(self.adapters)}  "
              f"budget={self.budget}  allow={sorted(self.policy['allow'])}  deny={sorted(self.policy['deny'])}")
        if check_only:
            for g in goals: self.static_check(g)
            print("✅ static check passed"); return
        self.started = time.time()
        for g in goals:
            goal_text = Expr(tokenize(g.text[5:]), {}).parse()[1]
            print(f"\n🎯 goal: {goal_text}")
            self.audit("goal_start", goal=goal_text)
            env = {"memory": self.memory}
            src = "\n".join(l.text for l in self._walk(g.children))
            try:
                self.block(g.children, env)
                self.audit("goal_done", goal=goal_text)
                print("✅ goal complete"); self.results.append((goal_text, 1.0, src))
            except Stop as s:
                self.audit("goal_stopped", goal=goal_text, reason=str(s))
                print(f"⏹  stopped: {s}"); self.results.append((goal_text, 0.5, src))
            except LughError as e:
                self.audit("goal_failed", goal=goal_text, error=str(e))
                print(f"❌ failed: {e}"); self.results.append((goal_text, 0.0, src))
        json.dump(self.memory, open(self.memory_path, "w"), indent=2, ensure_ascii=False, default=str)
        json.dump(self.stats, open(self.stats_path, "w"), indent=2)
        print(f"\n📜 audit log → {self.audit_path}   🧠 memory → {self.memory_path}   📈 route stats → {self.stats_path}")

    def report_score(self):
        """Simplicity-weighted success, after Legg & Hutter: sum of 2^-K(goal) * value, normalised."""
        if not self.results: return
        w = [(g, v, 2 ** (-K(src) / 256)) for g, v, src in self.results]
        score = sum(v * wt for _, v, wt in w) / sum(wt for *_, wt in w)
        print("\n📊 evaluation")
        for g, v, wt in w:
            print(f"  {v:.1f}  weight {wt:.3f}  {g}")
        print(f"  weighted score: {score:.3f}  (1.0 = every goal done)")

    def setup_use(self, spec):
        for name in [s.strip() for s in spec.split(",") if s.strip()]:
            base = name.split()[0]
            if base == "llm": self.adapters["llm"] = None; continue
            if base not in ADAPTERS: raise LughError(f"Unknown adapter '{base}'. Available: {sorted(ADAPTERS)} + llm")
            self.adapters[base] = ADAPTERS[base]()

    def setup_budget(self, spec):
        for part in [p.strip() for p in spec.split(",")]:
            k, v = part.split(None, 1)
            if k == "time":
                n = float(re.match(r"[\d.]+", v).group())
                self.budget["time"] = n * (60 if v.endswith("m") else 3600 if v.endswith("h") else 1)
            elif k == "calls": self.budget["calls"] = int(v)
            else: self.budget[k] = v   # spend/tokens recorded, enforced by adapters in later versions

    def setup_policy(self, node):
        for rule in node.children:
            t = rule.text
            if t.startswith("allow "):   bucket, rest = "allow", t[6:]
            elif t.startswith("deny "):  bucket, rest = "deny", t[5:]
            elif t.startswith("ask human before "): bucket, rest = "ask", t[17:]
            else: raise LughError(f"line {rule.no}: policy rules are allow / deny / ask human before")
            for e in [x.strip() for x in rest.split(",")]:
                if e not in EFFECTS: raise LughError(f"line {rule.no}: unknown effect '{e}'. Use {sorted(EFFECTS)}")
                self.policy[bucket].add(e)

    def static_check(self, goal):
        """Before running: report every effect the program could cause and how policy treats it."""
        for ln in self._walk(goal.children):
            for m in re.finditer(r"\b(\w+)\.(\w+)\b", ln.text):
                a, meth = m.groups()
                if a in self.adapters and self.adapters[a]:
                    eff = self.adapters[a].effect_of(meth)
                    rule = "DENIED" if eff in self.policy["deny"] else "allowed" if eff in self.policy["allow"] else "needs approval"
                    print(f"  line {ln.no:>3}: {a + "." + meth:<14} effect={eff:<8} → {rule}")
    def _walk(self, lines):
        for l in lines:
            yield l; yield from self._walk(l.children)

    # ---- statements ----
    def block(self, lines, env):
        last, i = None, 0
        while i < len(lines):
            ln = lines[i]
            nxt = lines[i + 1] if i + 1 < len(lines) else None
            consumed_next = False
            try:
                last, consumed_next = self.stmt(ln, nxt, env)
            except (Stop, LughError):
                raise
            except Exception as e:
                raise LughError(f"line {ln.no}: {type(e).__name__}: {e}")
            i += 2 if consumed_next else 1
        return last

    def stmt(self, ln, nxt, env):
        t = ln.text
        head = t.split()[0]

        if head == "let":
            m = re.match(r"let\s+(\w+)\s*=\s*(.*)$", t)
            if not m: raise LughError(f"line {ln.no}: let <name> = <value>")
            name, rhs = m.groups()
            if rhs == "attempt":
                val, used = self.attempt(ln, nxt, env); env[name] = val; return val, used
            if rhs == "attempt auto":
                env[name] = self.attempt_auto(ln, env); return env[name], False
            if rhs == "together":
                env[name] = self.together(ln, env); return env[name], False
            if rhs.startswith("consensus"):
                m2 = re.match(r"consensus(?:\s+min\s+(\d+))?$", rhs)
                env[name] = self.consensus(ln, env, int(m2.group(1)) if m2 and m2.group(1) else None)
                return env[name], False
            env[name] = self.eval_str(rhs, env, ln); return env[name], False

        if t == "attempt auto":
            return self.attempt_auto(ln, env), False
        if head == "attempt":
            return self.attempt(ln, nxt, env)

        if head == "refine":     # refine x with <expr> until <cond> [rounds N]
            m = re.match(r"refine\s+(\w+)\s+with\s+(.*?)\s+until\s+(.*?)(?:\s+rounds\s+(\d+))?$", t)
            if not m: raise LughError(f"line {ln.no}: refine <name> with <expr> until <cond> [rounds N]")
            return self.refine(env, m.group(1), m.group(2), m.group(3), int(m.group(4) or 3), ln), False

        if head == "ensure":     # ensure <cond> else stop "msg" | else ask human "msg"
            cond, _, fallback = t[7:].partition(" else ")
            if self.truthy(self.eval_str(cond, env, ln)):
                self.say("✔", f"ensured: {cond.strip()}"); return True, False
            self.audit("ensure_failed", condition=cond.strip())
            fb = fallback.strip()
            if fb.startswith("stop"): raise Stop(self.interp(self.eval_str(fb[4:], env, ln), env))
            if fb.startswith("ask human"):
                q = self.interp(self.eval_str(fb[9:], env, ln), env)
                if self.human_approves(f"ensure failed — continue anyway? {q}"): return False, False
                raise Stop("human chose to stop")
            raise Stop(f"ensure failed: {cond.strip()}")

        if head == "remember":   # remember "key" = expr [keep 7d]
            m = re.match(r'remember\s+("[^"]+")\s*=\s*(.*?)(?:\s+keep\s+(\d+)([smhd]))?$', t)
            key = m.group(1)[1:-1]; val = self.eval_str(m.group(2), env, ln)
            self.memory[key] = val; self.audit("remember", key=key, value=val)
            exp = self.memory.setdefault("__expires__", {})
            if m.group(3):
                secs = int(m.group(3)) * {"s": 1, "m": 60, "h": 3600, "d": 86400}[m.group(4)]
                exp[key] = time.time() + secs
                self.say("🧠", f"remembered '{key}' for {m.group(3)}{m.group(4)}")
            else:
                exp.pop(key, None); self.say("🧠", f"remembered '{key}'")
            return val, False

        if head == "ask":        # ask human "question" -> name
            m = re.match(r'ask\s+human\s+(.*?)\s*->\s*(\w+)$', t)
            if not m: raise LughError(f'line {ln.no}: ask human "question" -> name')
            q = self.interp(self.eval_str(m.group(1), env, ln), env)
            env[m.group(2)] = self.human_approves(q); return env[m.group(2)], False

        if head == "when":       # when <cond> ... otherwise ...
            if self.truthy(self.eval_str(t[5:], env, ln)):
                v = self.block(ln.children, env)
                return v, bool(nxt and nxt.text == "otherwise")
            if nxt and nxt.text == "otherwise":
                return self.block(nxt.children, env), True
            return None, False

        if head == "each":       # each item in list [parallel N]
            m = re.match(r"each\s+(\w+)\s+in\s+(.*?)(?:\s+parallel\s+(\d+))?$", t)
            items = list(self.eval_str(m.group(2), env, ln) or [])
            if m.group(3):       # pipelined: N workers stream items through the block
                def run(item):
                    local = dict(env); local[m.group(1)] = item
                    return self.block(ln.children, local)
                with ThreadPoolExecutor(max_workers=int(m.group(3))) as ex:
                    out = list(ex.map(run, items))
                self.say("⇉", f"processed {len(items)} items on {m.group(3)} workers")
                return out, False
            out = []
            for item in items:
                env[m.group(1)] = item; out.append(self.block(ln.children, env))
            return out, False

        if head == "send":       # send <channel> <target> <message>
            parts = Expr(tokenize(t[5:]), {}).t
            channel = parts[0][1]
            rest = self.eval_args(t[5 + len(channel):], env, ln)
            target, msg = rest[0], self.interp(rest[1], env)
            self.gate("send", f"send via {channel} to {target}: {msg[:60]}")
            self.dispatch_send(channel, target, msg); return msg, False

        if head == "log":
            v = self.eval_str(t[4:], env, ln); v = self.interp(v, env) if isinstance(v, str) else v
            self.say("📝", _to_text(v)); self.audit("log", value=v); return v, False

        if head == "stop":
            raise Stop(self.interp(self.eval_str(t[5:], env, ln), env))

        if head == "otherwise":
            raise LughError(f"line {ln.no}: 'otherwise' without attempt/when")

        # bare expression (usually an adapter call)
        return self.eval_str(t, env, ln), False

    def attempt(self, ln, nxt, env):
        try:
            v = self.block(ln.children, dict(env))
            self.say("✔", "attempt succeeded"); return v, bool(nxt and nxt.text == "otherwise")
        except (LughError, OSError) as e:
            if isinstance(e, (PolicyDenied, BudgetExceeded)): raise
            self.audit("attempt_failed", error=str(e))
            if nxt and nxt.text == "otherwise":
                self.say("↪", f"attempt failed ({str(e)[:70]}) → trying fallback")
                return self.block(nxt.children, env), True
            raise

    def attempt_auto(self, ln, env):
        """Each child line is an alternative route; try them best-first by learned score."""
        routes = sorted(ln.children, key=lambda c: self.route_score(c.text), reverse=True)
        self.say("🧭", "route order: " + " > ".join(r.text.split()[0] for r in routes))
        last_err = None
        for r in routes:
            t0 = time.time()
            try:
                v = self.block([r], dict(env))
                self.route_update(r.text, True, time.time() - t0)
                self.say("✔", f"route succeeded: {r.text.split()[0]}"); return v
            except (LughError, OSError) as e:
                if isinstance(e, (PolicyDenied, BudgetExceeded)): raise
                self.route_update(r.text, False, time.time() - t0)
                self.audit("route_failed", route=r.text, error=str(e)); last_err = e
                self.say("↪", f"route failed: {r.text.split()[0]} → next")
        raise LughError(f"all routes failed; last error: {last_err}")

    def consensus(self, ln, env, need):
        """Ask several independent sources; accept only a value enough of them agree on."""
        vals = self.together(ln, env)
        keyed = Counter(_txt(v) for v in vals)
        top, votes = keyed.most_common(1)[0]
        need = need or (len(vals) // 2 + 1)
        self.audit("consensus", sources=len(vals), votes=votes, needed=need)
        if votes < need:
            raise LughError(f"no consensus: best value had {votes}/{len(vals)} votes, needed {need}")
        self.say("🤝", f"consensus {votes}/{len(vals)}")
        return next(v for v in vals if _txt(v) == top)

    def refine(self, env, name, gen, cond, rounds, ln):
        """Residual refinement: propose, keep a proposal only if it passes; else fall back to the original."""
        original = env.get(name)
        if self.truthy(self.eval_str(cond, env, ln)):
            return original
        for r in range(1, rounds + 1):
            env[name] = self.eval_str(gen, env, ln)
            if self.truthy(self.eval_str(cond, env, ln)):
                self.audit("refine", name=name, rounds=r, accepted=True)
                self.say("♻", f"refined '{name}' in {r} round(s)"); return env[name]
        env[name] = original                                   # identity skip: never end worse than we began
        self.audit("refine", name=name, rounds=rounds, accepted=False)
        self.say("♻", f"refine of '{name}' did not pass in {rounds} rounds — kept original")
        return original

    def together(self, ln, env):
        """Run each child line in parallel; returns list of results."""
        with ThreadPoolExecutor(max_workers=8) as ex:
            futs = [ex.submit(self.block, [c], dict(env)) for c in ln.children]
            return [f.result() for f in futs]

    def dispatch_send(self, channel, target, msg):
        if channel == "console":
            print(f"  📣 [{target}] {msg}")
        elif channel == "file":
            open(target, "a", encoding="utf-8").write(msg + "\n"); self.say("📣", f"wrote to {target}")
        else:
            # other channels (slack, email, whatsapp...) map to MCP servers (roadmap)
            print(f"  📣 [{channel}:{target}] {msg}   (simulated — connect an MCP server for real delivery)")
        self.audit("sent", channel=channel, target=target, message=msg)

    # ---- expressions ----
    def eval_str(self, s, env, ln):
        return self.eval(Expr(tokenize(s.strip()), self.adapters).parse(), env)

    def eval_args(self, s, env, ln):
        p = Expr(tokenize(s.strip()), self.adapters); out = []
        while not p.done(): out.append(self.eval(p.primary(), env))
        return out

    def eval(self, node, env):
        k = node[0]
        if k == "lit": return node[1]
        if k == "str": return self.interp(node[1], env)
        if k == "list": return [self.eval(x, env) for x in node[1]]
        if k == "var": return self.lookup(node[1], env)
        if k == "not": return not self.truthy(self.eval(node[1], env))
        if k == "and": return self.truthy(self.eval(node[1], env)) and self.truthy(self.eval(node[2], env))
        if k == "or":  return self.truthy(self.eval(node[1], env)) or self.truthy(self.eval(node[2], env))
        if k == "cmp":
            a, b = self.eval(node[2], env), self.eval(node[3], env)
            return {"<": a < b, ">": a > b, "<=": a <= b, ">=": a >= b, "==": a == b, "!=": a != b}[node[1]]
        if k == "bin":
            a, b = self.eval(node[2], env), self.eval(node[3], env)
            return {"+": lambda: a + b, "-": lambda: a - b, "*": lambda: a * b, "/": lambda: a / b}[node[1]]()
        if k == "pick":
            _, mode, field, src = node
            items = self.eval(src, env)
            if not items: raise LughError("pick from an empty list")
            fn = min if mode == "lowest" else max
            return fn(items, key=lambda x: x[field])
        if k == "count": return len(self.eval(node[1], env))
        if k == "recall": return self.memory.get(self.eval(node[1], env))
        if k == "recall_like": return self.recall_like(self.eval(node[1], env))
        if k == "ask_human": return self.human_approves(self.eval(node[1], env))
        if k == "think":
            prompt = self.eval(node[1], env)
            ctx = self.eval(node[2], env) if node[2] else None
            n = int(self.eval(node[3], env))
            if n <= 1: return self.think(prompt, ctx)
            outs = [self.think(prompt, ctx) for _ in range(n)]          # test-time sampling
            best, votes = Counter(outs).most_common(1)[0]
            self.audit("vote", samples=n, agreement=round(votes / n, 2))
            self.say("🗳", f"think: {votes}/{n} samples agree")
            return best
        if k == "attend":
            items = list(self.eval(node[1], env) or [])
            q, n = self.eval(node[2], env), int(self.eval(node[3], env))
            ranked = sorted(items, key=lambda it: relevance(q, it), reverse=True)[:n]
            self.audit("attend", query=q, considered=len(items), kept=len(ranked))
            return ranked
        if k == "point":
            q, text = self.eval(node[1], env), _txt(self.eval(node[2], env))
            spans = [s.strip() for s in re.split(r"(?<=[.!?])\s+|\n+", text) if s.strip()]
            if not spans: return None
            best = max(spans, key=lambda s: relevance(q, s))
            if relevance(q, best) == 0: return None
            assert best in text                                         # grounded: always a verbatim span
            self.audit("point", query=q, span=best)
            return best
        if k == "novelty":
            x = self.eval(node[1], env)
            ref = self.eval(node[2], env) if node[2] else [v for kk, v in self.memory.items() if not kk.startswith("__")]
            return round(min((ncd(x, r) for r in ref), default=1.0), 3)
        if k == "simplest":
            items = list(self.eval(node[1], env) or [])
            if not items: raise LughError("simplest of an empty list")
            return min(items, key=K)
        if k == "pairs":
            _, a, xs, b, ys, cond = node
            out, local = [], dict(env)
            for x in self.eval(xs, env) or []:
                for y in self.eval(ys, env) or []:
                    local[a], local[b] = x, y
                    if self.truthy(self.eval(cond, local)):
                        out.append({a: x, b: y})
            return out
        if k == "call":
            _, name, args, cast = node
            adapter_name, method = name.split(".", 1)
            adapter = self.adapters[adapter_name]
            effect = adapter.effect_of(method)
            vals = [self.eval(a, env) for a in args]
            desc = f"{name} {' '.join((v if isinstance(v, str) else json.dumps(v, ensure_ascii=False))[:50] for v in vals)}"
            self.gate(effect, desc)
            self.say("→", desc)
            t0 = time.time()
            try:
                result = adapter.call(method, vals)
            except LughError: raise
            except Exception as e:
                self.audit("effect_error", effect=effect, action=desc, error=str(e))
                raise LughError(f"{name} failed: {e}")
            self.audit("effect", effect=effect, action=desc, ms=int((time.time() - t0) * 1000))
            return cast_value(result, cast)
        raise LughError(f"Unknown node {k}")

    def think(self, prompt, ctx):
        """The only probabilistic step. Uses Claude if ANTHROPIC_API_KEY is set, else a local stub."""
        if "llm" not in self.adapters: raise LughError("think needs 'use llm'")
        self.gate("read", f"think: {prompt[:60]}")
        full = prompt + (f"\n\nContext:\n{_to_text(ctx)}" if ctx is not None else "")
        key = os.environ.get("ANTHROPIC_API_KEY")
        if key:
            body = json.dumps({"model": os.environ.get("LUGH_MODEL", "claude-sonnet-5-5"), "max_tokens": 400,
                               "messages": [{"role": "user", "content": full}]}).encode()
            req = urllib.request.Request("https://api.anthropic.com/v1/messages", data=body, headers={
                "x-api-key": key, "anthropic-version": "2023-06-01", "content-type": "application/json"})
            out = json.loads(urllib.request.urlopen(req, timeout=60).read())["content"][0]["text"]
        else:
            out = f"[llm stub] would reason about: {prompt[:80]}"
        self.audit("think", prompt=prompt, output=out)
        return out

    def lookup(self, path, env):
        parts = path.split(".")
        if parts[0] not in env: raise LughError(f"'{parts[0]}' is not defined")
        v = env[parts[0]]
        for p in parts[1:]:
            v = v[int(p)] if isinstance(v, list) else v[p]
        return v

    def interp(self, s, env):
        if not isinstance(s, str): return s
        def rep(m):
            try:
                v = self.lookup(m.group(1), env)
                return str(int(v)) if isinstance(v, float) and v.is_integer() else str(v)
            except Exception: return m.group(0)
        return re.sub(r"\{([\w.]+)\}", rep, s)

    @staticmethod
    def truthy(v): return bool(v)


def main():
    if len(sys.argv) < 2:
        print(__doc__); sys.exit(1)
    path = sys.argv[1]
    rt = Runtime(auto_yes="--yes" in sys.argv)
    try:
        rt.run_source(open(path, encoding="utf-8").read(), check_only="--check" in sys.argv)
        if "--eval" in sys.argv: rt.report_score()
    except LughError as e:
        print(f"❌ {e}"); sys.exit(2)

if __name__ == "__main__":
    main()
