//! Front end: source text → block tree → statement and expression trees.
//!
//! Programs are parsed once, up front. The runtime then executes the
//! resulting trees directly and never re-reads source text.

use serde_json::Value;

// ───────────────────────────── tokens ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Str(String),
    Num(f64),
    Op(String),
    Name(String),
}

pub fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let (mut i, mut out) = (0, Vec::new());
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch == '"' {
            let (mut j, mut buf) = (i + 1, String::new());
            while j < c.len() && c[j] != '"' {
                if c[j] == '\\' && j + 1 < c.len() {
                    j += 1;
                    buf.push(match c[j] {
                        'n' => '\n',
                        't' => '\t',
                        x => x,
                    });
                } else {
                    buf.push(c[j]);
                }
                j += 1;
            }
            if j >= c.len() {
                return Err(format!("unterminated string in: {s}"));
            }
            out.push(Tok::Str(buf));
            i = j + 1;
            continue;
        }
        if ch.is_ascii_digit() {
            let mut j = i;
            while j < c.len() && (c[j].is_ascii_digit() || c[j] == '.') {
                j += 1;
            }
            let t: String = c[i..j].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| format!("bad number {t}"))?));
            i = j;
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            let mut j = i;
            while j < c.len()
                && (c[j].is_alphanumeric()
                    || c[j] == '_'
                    || (c[j] == '.'
                        && j + 1 < c.len()
                        && (c[j + 1].is_alphanumeric() || c[j + 1] == '_')))
            {
                j += 1;
            }
            out.push(Tok::Name(c[i..j].iter().collect()));
            i = j;
            continue;
        }
        // `.field` after a call, list or object: (…).name, […].0
        if ch == '.' && i + 1 < c.len() && (c[i + 1].is_alphanumeric() || c[i + 1] == '_') {
            out.push(Tok::Op(".".into()));
            i += 1;
            continue;
        }
        let two: String = c[i..(i + 2).min(c.len())].iter().collect();
        if ["<=", ">=", "==", "!=", "->"].contains(&two.as_str()) {
            out.push(Tok::Op(two));
            i += 2;
            continue;
        }
        if "<>+-*/=[],():{}".contains(ch) {
            out.push(Tok::Op(ch.to_string()));
            i += 1;
            continue;
        }
        return Err(format!("unexpected character {ch:?} in: {s}"));
    }
    Ok(out)
}

// ───────────────────────────── expressions ─────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    Lit(Value),
    Str(String),
    List(Vec<Expr>),
    /// A variable path, split once at compile time: `fare.price` → ["fare", "price"].
    Var(Vec<String>),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Cmp(String, Box<Expr>, Box<Expr>),
    Bin(String, Box<Expr>, Box<Expr>),
    Pick {
        lowest: bool,
        field: String,
        src: Box<Expr>,
    },
    Count(Box<Expr>),
    Recall(Box<Expr>),
    RecallLike(Box<Expr>),
    AskHuman(Box<Expr>),
    Think {
        prompt: Box<Expr>,
        ctx: Option<Box<Expr>>,
        n: Box<Expr>,
    },
    Attend {
        src: Box<Expr>,
        q: Box<Expr>,
        n: Box<Expr>,
    },
    Point {
        q: Box<Expr>,
        text: Box<Expr>,
    },
    Novelty {
        x: Box<Expr>,
        against: Option<Box<Expr>>,
    },
    Simplest(Box<Expr>),
    Pairs {
        a: String,
        xs: Box<Expr>,
        b: String,
        ys: Box<Expr>,
        cond: Box<Expr>,
    },
    Call {
        adapter: String,
        method: String,
        args: Vec<Expr>,
        cast: Option<String>,
    },
    /// Field or index access on any value: `count_by(xs, "a").Etihad`, `check(x).0`.
    Field(Box<Expr>, String),
    /// Object literal: `{airline: "Etihad", price: 41800}`.
    Obj(Vec<(String, Expr)>),
    /// Standard-library function: `text.upper(x)`, `sum(xs)`.
    Fn {
        name: String,
        args: Vec<Expr>,
    },
    /// `filter f in xs where f.price < 100`
    Filter {
        var: String,
        src: Box<Expr>,
        cond: Box<Expr>,
    },
    /// `map f in xs to f.price * 2`
    Map {
        var: String,
        src: Box<Expr>,
        out: Box<Expr>,
    },
    /// `sort xs by price [desc]`
    Sort {
        src: Box<Expr>,
        field: String,
        desc: bool,
    },
    /// `group xs by airline`
    Group {
        src: Box<Expr>,
        field: String,
    },
}

pub fn num(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        Value::from(n as i64)
    } else {
        serde_json::Number::from_f64(n)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

pub struct Parser<'a> {
    t: Vec<Tok>,
    i: usize,
    adapters: &'a [String],
}

const STOP: [&str; 17] = [
    "as", ")", ",", "]", "}", "and", "or", "<", ">", "<=", ">=", "==", "!=", "where", "to", "by",
    "desc",
];

impl<'a> Parser<'a> {
    pub fn new(t: Vec<Tok>, adapters: &'a [String]) -> Self {
        Parser { t, i: 0, adapters }
    }
    fn peek_is(&self, s: &str) -> bool {
        matches!(self.t.get(self.i), Some(Tok::Op(x)) | Some(Tok::Name(x)) if x == s)
    }
    fn bump(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn expect(&mut self, s: &str) -> Result<(), String> {
        if self.peek_is(s) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected '{s}', found {:?}", self.t.get(self.i)))
        }
    }
    fn name(&mut self) -> Result<String, String> {
        match self.bump() {
            Some(Tok::Name(n)) => Ok(n),
            o => Err(format!("expected a name, found {o:?}")),
        }
    }
    pub fn done(&self) -> bool {
        self.i >= self.t.len()
    }
    pub fn parse_all(&mut self) -> Result<Expr, String> {
        let e = self.or()?;
        if !self.done() {
            return Err(format!("unexpected {:?}", self.t.get(self.i)));
        }
        Ok(e)
    }
    fn or(&mut self) -> Result<Expr, String> {
        let mut e = self.and()?;
        while self.peek_is("or") {
            self.i += 1;
            e = Expr::Or(Box::new(e), Box::new(self.and()?));
        }
        Ok(e)
    }
    fn and(&mut self) -> Result<Expr, String> {
        let mut e = self.not()?;
        while self.peek_is("and") {
            self.i += 1;
            e = Expr::And(Box::new(e), Box::new(self.not()?));
        }
        Ok(e)
    }
    fn not(&mut self) -> Result<Expr, String> {
        if self.peek_is("not") {
            self.i += 1;
            return Ok(Expr::Not(Box::new(self.not()?)));
        }
        self.cmp()
    }
    fn cmp(&mut self) -> Result<Expr, String> {
        let e = self.add()?;
        for op in ["<=", ">=", "==", "!=", "<", ">"] {
            if self.peek_is(op) {
                self.i += 1;
                return Ok(Expr::Cmp(op.into(), Box::new(e), Box::new(self.add()?)));
            }
        }
        Ok(e)
    }
    fn add(&mut self) -> Result<Expr, String> {
        let mut e = self.mul()?;
        while self.peek_is("+") || self.peek_is("-") {
            let op = if self.peek_is("+") { "+" } else { "-" };
            self.i += 1;
            e = Expr::Bin(op.into(), Box::new(e), Box::new(self.mul()?));
        }
        Ok(e)
    }
    fn mul(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        while self.peek_is("*") || self.peek_is("/") {
            let op = if self.peek_is("*") { "*" } else { "/" };
            self.i += 1;
            e = Expr::Bin(op.into(), Box::new(e), Box::new(self.primary()?));
        }
        Ok(e)
    }

    pub fn primary(&mut self) -> Result<Expr, String> {
        let mut e = self.atom()?;
        while self.peek_is(".") {
            self.i += 1;
            let key = match self.bump() {
                Some(Tok::Name(n)) => n,
                Some(Tok::Num(n)) => format!("{}", n as i64),
                other => return Err(format!("expected a field name after '.', found {other:?}")),
            };
            for part in key.split('.') {
                e = Expr::Field(Box::new(e), part.to_string());
            }
        }
        Ok(e)
    }

    fn atom(&mut self) -> Result<Expr, String> {
        match self.bump().ok_or("unexpected end of expression")? {
            Tok::Num(n) => Ok(Expr::Lit(num(n))),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Op(o) if o == "(" => {
                let e = self.or()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Op(o) if o == "[" => {
                let mut v = Vec::new();
                while !self.peek_is("]") {
                    if self.done() {
                        return Err("unclosed '['".into());
                    }
                    v.push(self.or()?);
                    if self.peek_is(",") {
                        self.i += 1;
                    }
                }
                self.i += 1;
                Ok(Expr::List(v))
            }
            Tok::Op(o) if o == "-" => Ok(Expr::Bin(
                "-".into(),
                Box::new(Expr::Lit(Value::from(0))),
                Box::new(self.primary()?),
            )),
            Tok::Op(o) if o == "{" => {
                let mut fields = Vec::new();
                while !self.peek_is("}") {
                    let key = match self.bump() {
                        Some(Tok::Name(k)) | Some(Tok::Str(k)) => k,
                        other => return Err(format!("object key expected, found {other:?}")),
                    };
                    self.expect(":")?;
                    fields.push((key, self.or()?));
                    if self.peek_is(",") {
                        self.i += 1;
                    } else if !self.peek_is("}") {
                        return Err("expected ',' or '}' in object".into());
                    }
                }
                self.i += 1;
                Ok(Expr::Obj(fields))
            }
            Tok::Name(n) => self.named(n),
            other => Err(format!("cannot parse near {other:?}")),
        }
    }

    fn named(&mut self, n: String) -> Result<Expr, String> {
        let b = Box::new;
        match n.as_str() {
            "true" => return Ok(Expr::Lit(Value::Bool(true))),
            "false" => return Ok(Expr::Lit(Value::Bool(false))),
            "none" => return Ok(Expr::Lit(Value::Null)),
            "pick" => {
                let mode = self.name()?;
                let field = self.name()?;
                self.expect("from")?;
                return Ok(Expr::Pick {
                    lowest: mode == "lowest",
                    field,
                    src: b(self.primary()?),
                });
            }
            "count" => return Ok(Expr::Count(b(self.primary()?))),
            "recall" => {
                if self.peek_is("like") {
                    self.i += 1;
                    return Ok(Expr::RecallLike(b(self.primary()?)));
                }
                return Ok(Expr::Recall(b(self.primary()?)));
            }
            "think" => {
                let prompt = b(self.primary()?);
                let mut ctx = None;
                let mut n = b(Expr::Lit(Value::from(1)));
                if self.peek_is("with") {
                    self.i += 1;
                    ctx = Some(b(self.primary()?));
                }
                if self.peek_is("samples") {
                    self.i += 1;
                    n = b(self.primary()?);
                }
                return Ok(Expr::Think { prompt, ctx, n });
            }
            "attend" => {
                self.expect("to")?;
                let src = b(self.primary()?);
                self.expect("by")?;
                let q = b(self.primary()?);
                let mut n = b(Expr::Lit(Value::from(3)));
                if self.peek_is("top") {
                    self.i += 1;
                    n = b(self.primary()?);
                }
                return Ok(Expr::Attend { src, q, n });
            }
            "point" => {
                self.expect("to")?;
                let q = b(self.primary()?);
                self.expect("in")?;
                return Ok(Expr::Point {
                    q,
                    text: b(self.primary()?),
                });
            }
            "novelty" => {
                let x = b(self.primary()?);
                let mut against = None;
                if self.peek_is("against") {
                    self.i += 1;
                    against = Some(b(self.primary()?));
                }
                return Ok(Expr::Novelty { x, against });
            }
            "simplest" => {
                self.expect("of")?;
                return Ok(Expr::Simplest(b(self.primary()?)));
            }
            "pairs" => {
                let a = self.name()?;
                self.expect("from")?;
                let xs = b(self.primary()?);
                self.expect(",")?;
                let bn = self.name()?;
                self.expect("from")?;
                let ys = b(self.primary()?);
                self.expect("where")?;
                return Ok(Expr::Pairs {
                    a,
                    xs,
                    b: bn,
                    ys,
                    cond: b(self.or()?),
                });
            }
            "filter" | "map" if matches!(self.t.get(self.i + 1), Some(Tok::Name(x)) if x == "in") =>
            {
                let var = self.name()?;
                self.expect("in")?;
                let src = b(self.primary()?);
                return Ok(if n == "filter" {
                    self.expect("where")?;
                    Expr::Filter {
                        var,
                        src,
                        cond: b(self.or()?),
                    }
                } else {
                    self.expect("to")?;
                    Expr::Map {
                        var,
                        src,
                        out: b(self.or()?),
                    }
                });
            }
            "sort" | "group" if !self.peek_is("(") => {
                let src = b(self.primary()?);
                self.expect("by")?;
                let field = self.name()?;
                if n == "group" {
                    return Ok(Expr::Group { src, field });
                }
                let desc = self.peek_is("desc");
                if desc {
                    self.i += 1;
                }
                return Ok(Expr::Sort { src, field, desc });
            }
            "ask" if self.peek_is("human") => {
                self.i += 1;
                return Ok(Expr::AskHuman(b(self.primary()?)));
            }
            _ => {}
        }
        let is_adapter = n
            .split_once('.')
            .is_some_and(|(h, _)| self.adapters.iter().any(|a| a == h));
        if self.peek_is("(") && !is_adapter {
            self.i += 1;
            let mut args = Vec::new();
            while !self.peek_is(")") {
                if self.done() {
                    return Err(format!("unclosed '(' in call to {n}"));
                }
                args.push(self.or()?);
                if self.peek_is(",") {
                    self.i += 1;
                }
            }
            self.i += 1;
            return Ok(Expr::Fn { name: n, args });
        }
        if let Some((head, method)) = n.split_once('.') {
            if self.adapters.iter().any(|a| a == head) {
                let mut args = Vec::new();
                while !self.done() && !STOP.iter().any(|s| self.peek_is(s)) {
                    args.push(self.primary()?);
                }
                let cast = if self.peek_is("as") {
                    self.i += 1;
                    Some(self.name()?)
                } else {
                    None
                };
                return Ok(Expr::Call {
                    adapter: head.into(),
                    method: method.into(),
                    args,
                    cast,
                });
            }
        }
        Ok(Expr::Var(n.split('.').map(String::from).collect()))
    }
}

pub fn parse_expr(s: &str, adapters: &[String]) -> Result<Expr, String> {
    Parser::new(tokenize(s.trim())?, adapters).parse_all()
}

pub fn parse_args(s: &str, adapters: &[String]) -> Result<Vec<Expr>, String> {
    let mut p = Parser::new(tokenize(s.trim())?, adapters);
    let mut out = Vec::new();
    while !p.done() {
        out.push(p.primary()?);
    }
    Ok(out)
}

// ───────────────────────────── block tree ─────────────────────────────

#[derive(Debug, Clone)]
pub struct Line {
    pub text: String,
    pub indent: usize,
    pub no: usize,
    pub children: Vec<Line>,
}

fn strip_comment(raw: &str) -> &str {
    let mut in_str = false;
    for (i, ch) in raw.char_indices() {
        match ch {
            '"' => in_str = !in_str,
            '#' if !in_str => return &raw[..i],
            _ => {}
        }
    }
    raw
}

pub fn build_tree(src: &str) -> Vec<Line> {
    let flat: Vec<Line> = src
        .lines()
        .enumerate()
        .filter_map(|(n, raw)| {
            let s = strip_comment(raw);
            if s.trim().is_empty() {
                return None;
            }
            let indent = s.len() - s.trim_start_matches(' ').len();
            Some(Line {
                text: s.trim().to_string(),
                indent,
                no: n + 1,
                children: vec![],
            })
        })
        .collect();
    fn build(items: &[Line], pos: &mut usize, parent: Option<usize>) -> Vec<Line> {
        let mut out = Vec::new();
        while *pos < items.len() && parent.is_none_or(|p| items[*pos].indent > p) {
            let mut line = items[*pos].clone();
            *pos += 1;
            line.children = build(items, pos, Some(line.indent));
            out.push(line);
        }
        out
    }
    build(&flat, &mut 0, None)
}

pub fn walk(lines: &[Line], out: &mut Vec<String>) {
    for l in lines {
        out.push(l.text.clone());
        walk(&l.children, out);
    }
}

// ───────────────────────────── statements ─────────────────────────────

pub type Block = Vec<(usize, Stmt)>;

#[derive(Debug, Clone)]
pub enum Rhs {
    Expr(Expr),
    Attempt(Block, Option<Block>),
    AttemptAuto(Vec<(String, Block)>),
    Together(Vec<Block>),
    Consensus(Vec<Block>, Option<usize>),
}

#[derive(Debug, Clone)]
pub enum Fallback {
    Stop(Expr),
    Ask(Expr),
    Respond(u16, Expr),
    None,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(String, Rhs),
    Do(Rhs),
    Refine {
        name: String,
        gen: Expr,
        cond: Expr,
        rounds: usize,
    },
    Ensure {
        cond: Expr,
        src: String,
        fb: Fallback,
    },
    Remember {
        key: String,
        expr: Expr,
        keep: Option<f64>,
    },
    Ask {
        q: Expr,
        name: String,
    },
    When {
        cond: Expr,
        body: Block,
        otherwise: Option<Block>,
    },
    Each {
        var: String,
        list: Expr,
        parallel: Option<usize>,
        body: Block,
    },
    Send {
        channel: String,
        target: Expr,
        msg: Expr,
    },
    Log(Expr),
    Stop(Expr),
    Respond {
        status: u16,
        body: Expr,
    },
    /// `expect <cond>` inside a test block.
    Expect {
        cond: Expr,
        src: String,
    },
}

/// `respond 201 value` or `respond value` (status 200).
fn parse_respond(s: &str, ad: &[String]) -> Result<(u16, Expr), String> {
    let s = s.trim();
    let (first, rest) = s.split_once(' ').unwrap_or((s, ""));
    match first.parse::<u16>() {
        Ok(code) if (100..600).contains(&code) && !rest.trim().is_empty() => {
            Ok((code, parse_expr(rest, ad)?))
        }
        _ => Ok((200, parse_expr(s, ad)?)),
    }
}

fn duration_secs(s: &str) -> Option<f64> {
    let (n, unit) = s.split_at(s.len().checked_sub(1)?);
    let n: f64 = n.parse().ok()?;
    Some(
        n * match unit {
            "s" => 1.0,
            "m" => 60.0,
            "h" => 3600.0,
            "d" => 86400.0,
            _ => return None,
        },
    )
}

fn each_child(line: &Line, ad: &[String]) -> Result<Vec<Block>, String> {
    line.children
        .iter()
        .map(|c| parse_block(std::slice::from_ref(c), ad))
        .collect()
}

fn complex_rhs(
    rhs: &str,
    line: &Line,
    next: Option<&Line>,
    ad: &[String],
) -> Result<Option<(Rhs, bool)>, String> {
    let otherwise = next.filter(|n| n.text == "otherwise");
    Ok(Some(match rhs {
        "attempt" => {
            let alt = otherwise
                .map(|o| parse_block(&o.children, ad))
                .transpose()?;
            (
                Rhs::Attempt(parse_block(&line.children, ad)?, alt),
                otherwise.is_some(),
            )
        }
        "attempt auto" => {
            let routes = line
                .children
                .iter()
                .map(|c| Ok((c.text.clone(), parse_block(std::slice::from_ref(c), ad)?)))
                .collect::<Result<_, String>>()?;
            (Rhs::AttemptAuto(routes), false)
        }
        "together" => (Rhs::Together(each_child(line, ad)?), false),
        r if r == "consensus" || r.starts_with("consensus min ") => {
            let need = r
                .strip_prefix("consensus min ")
                .map(|n| n.trim().parse::<usize>())
                .transpose()
                .map_err(|e| e.to_string())?;
            (Rhs::Consensus(each_child(line, ad)?, need), false)
        }
        _ => return Ok(None),
    }))
}

pub fn parse_block(lines: &[Line], ad: &[String]) -> Result<Block, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (ln, next) = (&lines[i], lines.get(i + 1));
        let (stmt, used_next) =
            parse_stmt(ln, next, ad).map_err(|e| format!("line {}: {e}", ln.no))?;
        out.push((ln.no, stmt));
        i += if used_next { 2 } else { 1 };
    }
    Ok(out)
}

fn parse_stmt(ln: &Line, next: Option<&Line>, ad: &[String]) -> Result<(Stmt, bool), String> {
    let t = ln.text.as_str();
    let head = t.split_whitespace().next().unwrap_or("");
    let rest = t[head.len()..].trim();
    let pe = |s: &str| parse_expr(s, ad);
    let otherwise = next.filter(|n| n.text == "otherwise");

    match head {
        "let" => {
            let (name, rhs) = rest
                .split_once('=')
                .ok_or("expected: let <name> = <value>")?;
            let (name, rhs) = (name.trim().to_string(), rhs.trim());
            if let Some((r, used)) = complex_rhs(rhs, ln, next, ad)? {
                return Ok((Stmt::Let(name, r), used));
            }
            Ok((Stmt::Let(name, Rhs::Expr(pe(rhs)?)), false))
        }
        "attempt" => {
            let (r, used) =
                complex_rhs(t, ln, next, ad)?.ok_or("expected: attempt or attempt auto")?;
            Ok((Stmt::Do(r), used))
        }
        "refine" => {
            let (name, rest) = rest
                .split_once(" with ")
                .ok_or("expected: refine <name> with <expr> until <cond>")?;
            let (gen, cond) = rest.split_once(" until ").ok_or("missing 'until'")?;
            let (cond, rounds) = match cond.rsplit_once(" rounds ") {
                Some((c, r)) => (c, r.trim().parse().map_err(|_| "rounds must be a number")?),
                None => (cond, 3),
            };
            Ok((
                Stmt::Refine {
                    name: name.trim().into(),
                    gen: pe(gen)?,
                    cond: pe(cond)?,
                    rounds,
                },
                false,
            ))
        }
        "ensure" => {
            let (cond, fb) = match rest.split_once(" else ") {
                Some((c, f)) => (c, f.trim()),
                None => (rest, ""),
            };
            let fb = if let Some(m) = fb.strip_prefix("respond") {
                let (code, e) = parse_respond(m, ad)?;
                Fallback::Respond(code, e)
            } else if let Some(m) = fb.strip_prefix("stop") {
                Fallback::Stop(pe(m)?)
            } else if let Some(m) = fb.strip_prefix("ask human") {
                Fallback::Ask(pe(m)?)
            } else {
                Fallback::None
            };
            Ok((
                Stmt::Ensure {
                    cond: pe(cond)?,
                    src: cond.trim().into(),
                    fb,
                },
                false,
            ))
        }
        "remember" => {
            let (key, rest) = rest
                .split_once('=')
                .ok_or("expected: remember \"key\" = <value>")?;
            let key = key.trim().trim_matches('"').to_string();
            let (expr, keep) = match rest.rsplit_once(" keep ") {
                Some((e, d)) if duration_secs(d.trim()).is_some() => (e, duration_secs(d.trim())),
                _ => (rest, None),
            };
            Ok((
                Stmt::Remember {
                    key,
                    expr: pe(expr)?,
                    keep,
                },
                false,
            ))
        }
        "ask" => {
            let body = rest
                .strip_prefix("human")
                .ok_or("expected: ask human \"question\" -> name")?;
            let (q, name) = body
                .rsplit_once("->")
                .ok_or("expected: ask human \"question\" -> name")?;
            Ok((
                Stmt::Ask {
                    q: pe(q)?,
                    name: name.trim().into(),
                },
                false,
            ))
        }
        "when" => {
            let alt = otherwise
                .map(|o| parse_block(&o.children, ad))
                .transpose()?;
            Ok((
                Stmt::When {
                    cond: pe(rest)?,
                    body: parse_block(&ln.children, ad)?,
                    otherwise: alt,
                },
                otherwise.is_some(),
            ))
        }
        "each" => {
            let (var, rest) = rest
                .split_once(" in ")
                .ok_or("expected: each <name> in <list>")?;
            let (list, parallel) = match rest.rsplit_once(" parallel ") {
                Some((l, n)) => (
                    l,
                    Some(n.trim().parse().map_err(|_| "parallel must be a number")?),
                ),
                None => (rest, None),
            };
            Ok((
                Stmt::Each {
                    var: var.trim().into(),
                    list: pe(list)?,
                    parallel,
                    body: parse_block(&ln.children, ad)?,
                },
                false,
            ))
        }
        "send" => {
            let channel = rest
                .split_whitespace()
                .next()
                .ok_or("expected: send <channel> <target> <message>")?;
            let args = parse_args(&rest[channel.len()..], ad)?;
            if args.len() != 2 {
                return Err("expected: send <channel> <target> <message>".into());
            }
            let mut it = args.into_iter();
            Ok((
                Stmt::Send {
                    channel: channel.into(),
                    target: it.next().unwrap(),
                    msg: it.next().unwrap(),
                },
                false,
            ))
        }
        "log" => Ok((Stmt::Log(pe(rest)?), false)),
        "stop" => Ok((Stmt::Stop(pe(rest)?), false)),
        "expect" => Ok((
            Stmt::Expect {
                cond: pe(rest)?,
                src: rest.to_string(),
            },
            false,
        )),
        "respond" => {
            let (status, body) = parse_respond(rest, ad)?;
            Ok((Stmt::Respond { status, body }, false))
        }
        "otherwise" => Err("'otherwise' without attempt/when".into()),
        _ => Ok((Stmt::Do(Rhs::Expr(pe(t)?)), false)),
    }
}

// ───────────────────────────── static analysis ─────────────────────────────

pub fn calls_in_block(b: &Block, out: &mut Vec<(usize, String, String)>) {
    for (no, s) in b {
        let mut ex: Vec<&Expr> = Vec::new();
        let mut blocks: Vec<&Block> = Vec::new();
        match s {
            Stmt::Let(_, r) | Stmt::Do(r) => rhs_parts(r, &mut ex, &mut blocks),
            Stmt::Refine { gen, cond, .. } => ex.extend([gen, cond]),
            Stmt::Ensure { cond, .. } => ex.push(cond),
            Stmt::Remember { expr, .. } => ex.push(expr),
            Stmt::Ask { q, .. } => ex.push(q),
            Stmt::When {
                cond,
                body,
                otherwise,
            } => {
                ex.push(cond);
                blocks.push(body);
                if let Some(o) = otherwise {
                    blocks.push(o)
                }
            }
            Stmt::Each { list, body, .. } => {
                ex.push(list);
                blocks.push(body)
            }
            Stmt::Send { target, msg, .. } => ex.extend([target, msg]),
            Stmt::Log(e) | Stmt::Stop(e) => ex.push(e),
            Stmt::Respond { body, .. } => ex.push(body),
            Stmt::Expect { cond, .. } => ex.push(cond),
        }
        for e in ex {
            calls_in_expr(e, *no, out);
        }
        for b in blocks {
            calls_in_block(b, out);
        }
    }
}

fn rhs_parts<'a>(r: &'a Rhs, ex: &mut Vec<&'a Expr>, blocks: &mut Vec<&'a Block>) {
    match r {
        Rhs::Expr(e) => ex.push(e),
        Rhs::Attempt(a, o) => {
            blocks.push(a);
            if let Some(o) = o {
                blocks.push(o)
            }
        }
        Rhs::AttemptAuto(rs) => rs.iter().for_each(|(_, b)| blocks.push(b)),
        Rhs::Together(bs) | Rhs::Consensus(bs, _) => bs.iter().for_each(|b| blocks.push(b)),
    }
}

fn calls_in_expr(e: &Expr, no: usize, out: &mut Vec<(usize, String, String)>) {
    use Expr::*;
    let mut kids: Vec<&Expr> = Vec::new();
    match e {
        Call {
            adapter,
            method,
            args,
            ..
        } => {
            out.push((no, adapter.clone(), method.clone()));
            kids.extend(args.iter());
        }
        List(v) => kids.extend(v.iter()),
        Not(a) | Count(a) | Recall(a) | RecallLike(a) | AskHuman(a) | Simplest(a) => kids.push(a),
        And(a, b) | Or(a, b) | Cmp(_, a, b) | Bin(_, a, b) => kids.extend([&**a, &**b]),
        Pick { src, .. } => kids.push(src),
        Think { prompt, ctx, n } => {
            kids.extend([&**prompt, &**n]);
            if let Some(c) = ctx {
                kids.push(c)
            }
        }
        Attend { src, q, n } => kids.extend([&**src, &**q, &**n]),
        Point { q, text } => kids.extend([&**q, &**text]),
        Novelty { x, against } => {
            kids.push(x);
            if let Some(a) = against {
                kids.push(a)
            }
        }
        Pairs { xs, ys, cond, .. } => kids.extend([&**xs, &**ys, &**cond]),
        Obj(fields) => kids.extend(fields.iter().map(|f| &f.1)),
        Field(inner, _) => kids.push(inner),
        Fn { name, args } => {
            out.push((no, String::new(), name.clone()));
            kids.extend(args.iter());
        }
        Filter { src, cond, .. } => kids.extend([&**src, &**cond]),
        Map { src, out, .. } => kids.extend([&**src, &**out]),
        Sort { src, .. } | Group { src, .. } => kids.push(src),
        Lit(_) | Str(_) | Var(_) => {}
    }
    for k in kids {
        calls_in_expr(k, no, out);
    }
}
