# Lugh standard library

The standard library covers what most Python and JavaScript projects pull in
from their ecosystems. It's built into Lugh Core, so there's nothing to install.

Functions are called as `module.name(args)`. The most common have short names:
`sum`, `avg`, `min`, `max`, `round`, `abs`, `upper`, `lower`, `trim`, `split`,
`join`, `replace`, `contains`, `unique`, `first`, `last`, `range`, `keys`,
`values`, `now`, `today`, `uuid`, `number`.

Standard-library functions are pure: they don't touch files, the network or
databases, so they don't pass the policy gate. Anything with an effect is an
adapter (`fs`, `http`, `shell`, `db`, `llm`) and is checked against `policy` on
every call. A misspelled function name is a compile-time error.

The whole library is exercised by [`examples/stdlib_test.lugh`](../examples/stdlib_test.lugh).
Its expected values were computed independently with Python's standard library
and NumPy.

## Where it comes from

| Need | Python | JavaScript | Lugh |
| --- | --- | --- | --- |
| Strings and regex | `str`, `re` | `String`, `RegExp` | `text.*` |
| Collections | comprehensions, `itertools` | lodash, `Array` methods | `filter`, `map`, `sort`, `group`, `list.*` |
| Maths | `math`, `random` | `Math` | `math.*` |
| Statistics | `statistics`, NumPy | simple-statistics | `stats.*` |
| Dates and times | `datetime` | date-fns, Day.js | `time.*` |
| Data formats | `json`, `csv` | `JSON`, PapaParse | `json.*`, `csv.*` |
| Hashing, signing, IDs | `hashlib`, `hmac`, `uuid`, `secrets` | `node:crypto` | `crypto.*` |
| Encoding | `base64`, `urllib.parse` | `btoa`, `encodeURIComponent` | `encode.*` |
| Validation | pydantic | zod | `schema.*` |
| Templates | Jinja2 | Handlebars, Mustache | `template.render` |
| Database | `sqlite3` | better-sqlite3 | `db` adapter |
| Configuration | `os.environ`, python-dotenv | `process.env`, dotenv | `env()` |
| Testing | pytest | Jest | `test` blocks, `expect` |
| Scheduling | APScheduler, Celery beat | node-cron | `every` blocks |
| Web framework | FastAPI, Flask | Express | `route`, `page` ([SPEC](SPEC.md)) |
| Auth middleware | Flask-HTTPAuth | Passport (bearer) | `auth token env` |

## Collection expressions

```lugh
let cheap   = filter f in fares where f.price < 44000
let prices  = map f in fares to f.price
let ordered = sort fares by price          # add `desc` for descending
let by      = group fares by airline       # {"Etihad": [...], ...}
```

## Reference

### text

| Function | Result |
| --- | --- |
| `upper(s)`, `lower(s)`, `trim(s)`, `text.title(s)` | Case and whitespace |
| `split(s, sep=" ")`, `join(list, sep="")` | Split and join |
| `replace(s, old, new)` | Literal replace |
| `text.contains(s, part)`, `text.starts(s, p)`, `text.ends(s, p)` | Tests |
| `text.slice(s, from, to?)` | Substring by character; negative indexes count from the end |
| `text.words(s)`, `text.lines(s)`, `text.length(s)` | Pieces and length |
| `text.pad(s, width, fill=" ")`, `text.repeat(s, n)` | Padding |
| `number(s)` | Parses `"45,000"` → `45000`; `none` if not a number |
| `text.matches(s, pattern)` | Regex test |
| `text.find(s, pattern)` | First match (or first capture group), else `none` |
| `text.find_all(s, pattern)` | All matches |
| `text.sub(s, pattern, replacement)` | Regex replace (`$1` refers to groups) |

### list and object

| Function | Result |
| --- | --- |
| `sum(xs)`, `min(xs)` / `min(a, b, …)`, `max(…)` | Aggregates |
| `first(xs)`, `last(xs)`, `list.take(xs, n)`, `list.skip(xs, n)`, `list.reverse(xs)` | Slicing |
| `unique(xs)`, `list.flatten(xs)`, `list.compact(xs)` | Cleaning |
| `list.zip(a, b)`, `list.chunk(xs, n)`, `list.concat(a, b, …)` | Combining |
| `range(n)`, `range(a, b, step=1)` | Numbers; at most 1,000,000 |
| `contains(xs, v)`, `list.index(xs, v)` | Membership |
| `list.pluck(xs, field)`, `list.count_by(xs, field)`, `list.index_by(xs, field)` | Records |
| `keys(o)`, `values(o)`, `list.merge(a, b, …)` | Objects |
| `list.get(o, "a.b.0", default)`, `list.set(o, key, v)`, `list.has(o, key)` | Safe access |

### math and stats

| Function | Result |
| --- | --- |
| `round(x, digits=0)`, `math.floor`, `math.ceil`, `abs` | Rounding |
| `math.sqrt`, `math.pow(x, y)`, `math.log(x, base?)`, `math.exp`, `math.pi()` | Maths |
| `math.clamp(x, lo, hi)` | Bounds |
| `math.random()`, `math.random_int(a, b)` | OS randomness (not seedable) |
| `avg(xs)` / `stats.mean`, `stats.median`, `stats.mode` | Centre |
| `stats.variance`, `stats.stdev` | Sample spread (n − 1), as Python's `statistics` |
| `stats.percentile(xs, p)` | Linear interpolation, as NumPy's default |
| `stats.normalize(xs)` | Rescale to 0..1 |

### time

All times are UTC. Inputs may be unix seconds, `"YYYY-MM-DD"`, or ISO 8601
with an optional offset (`2026-11-14T01:00:00+05:30`).

| Function | Result |
| --- | --- |
| `now()`, `today()`, `time.unix(t?)` | Current time |
| `time.parse(t)` → unix seconds; `time.format(t, "date"\|"datetime")` → ISO text | Conversion |
| `time.add(t, n, unit)` | `unit`: seconds, minutes, hours, days, weeks. A date stays a date. |
| `time.diff(a, b, unit)` | `b − a` in units |
| `time.weekday(t)`, `time.start_of(t, "day"\|"week"\|"month"\|"year")` | Calendar; weeks start Monday |

### json and csv

| Function | Result |
| --- | --- |
| `json.parse(s)`, `json.text(v)`, `json.pretty(v)` | JSON |
| `csv.parse(s)` | List of objects keyed by the header row (RFC 4180 quoting). Numeric cells become numbers, except ones with leading zeros such as postcodes. |
| `csv.text(rows, columns?)` | CSV text with quoting |

### crypto and encode

| Function | Result |
| --- | --- |
| `crypto.sha256(s)`, `crypto.hmac_sha256(key, msg)` | Hex digests |
| `uuid()` | Random UUID v4 |
| `crypto.random_hex(bytes=16)` | Secure random token |
| `crypto.equal(a, b)` | Constant-time comparison for tokens and signatures |
| `encode.base64(s)`, `encode.unbase64(s)`, `encode.url(s)`, `encode.unurl(s)`, `encode.hex(s)` | Encodings |

### schema

```lugh
let spec = {title: "text 2..80", amount: "number 0.01..1000000", date: "date", "notes?": "text"}
let errors = schema.check(request.body, spec)     # ["amount: must be within 0.01..1000000", ...]
ensure schema.valid(request.body, spec) else respond 422 {errors: errors}
```

Types: `text`, `number`, `int`, `bool`, `list`, `object`, `email`, `date`,
`any`. An optional range `lo..hi` bounds numbers, or the length of text and lists.
A key ending in `?` is optional. A nested object as the rule validates a nested
object.

### template

```lugh
template.render("Hi {{name}}! {{#items}}[{{title}}]{{/items}}{{^items}}Nothing yet{{/items}}", data)
```

`{{x}}` is HTML-escaped, `{{{x}}}` is raw, `{{#list}}…{{/list}}` loops or shows
when truthy, `{{^x}}…{{/x}}` shows when falsy, `{{.}}` is the current item.

### env and type

`env("NAME", default?)` reads the environment, then a `.env` file in the
working directory. `type(v)` returns `none`, `bool`, `number`, `text`, `list`
or `object`.

## Adapter: db (SQLite)

```lugh
use db
db.exec "CREATE TABLE IF NOT EXISTS expenses (id TEXT PRIMARY KEY, title TEXT, amount REAL)"
db.exec "INSERT INTO expenses VALUES (?, ?, ?)" [uuid(), "Visa fee", 9800]
let rows = db.query "SELECT title, amount FROM expenses WHERE amount > ?" [1000]
```

`db.query` is a `read` effect and returns rows as objects. `db.exec` is a
`write` effect and returns the number of changed rows. Values are always bound
as parameters, never spliced into SQL. The database file is `LUGH_DB`, or
`lugh.db` by default. SQLite is compiled into the binary.

## Tests

```lugh
test "fares under budget"
  let cheap = filter f in fares where f.price < 45000
  expect count(cheap) == 3
```

`lugh program.lugh --test` runs every `test` block with auto-approval and
reports each one. It exits non-zero if any test fails. Policy and budgets still
apply inside tests.

## Scheduled jobs

```lugh
every 1h
  let rows = db.query "SELECT amount FROM expenses"
  remember "ledger_total" = sum(map r in rows to r.amount)
```

`every <n>s|m|h|d` blocks run on their own thread while `--serve` is running,
first after one interval. Each run gets a fresh budget and is recorded in the
audit log as `job_start` / `job_end`.

## Auth for served apps

```lugh
auth token env "LUGH_TOKEN"
```

Every request then needs `Authorization: Bearer <token>`, or the `lugh_token`
cookie. Opening any page once with `?token=<token>` sets that cookie
(`HttpOnly`, `SameSite=Strict`). The server refuses to start unless the variable
holds at least 16 characters. Tokens are compared in constant time. TLS is not
built in: terminate HTTPS in a reverse proxy before exposing a server publicly.
