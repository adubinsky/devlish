# Python-Parity Standard Library Plan

Last updated: 2026-07-26
Status: Build plan. Tickets tracked in Jira project DEVL under the "Python-parity
standard library" epic.

## Goal

Give Devlish an optional, included standard library that reaches for the same
coverage as Python's standard library, while staying true to Devlish's identity:

- **English-first**: every function reads as a controlled-English phrase
  (`the square root of x`, `x rounded to 2 places`), never a cludgy
  `math.sqrt(x)` pass-through wrapper.
- **Module namespaces**: libraries are named, namespaced modules you import,
  not a flat global symbol soup (a new language feature, see L1).
- **Hybrid implementation**: primitives that need real computation (regex,
  float math, hashing, decimal, date math) are native Rust builtins in
  `devlish_vm`; higher-level, composable helpers are Devlish-authored `.dvl`
  modules discoverable via the bundled stdlib path. Both are called through the
  same English surface; a caller cannot tell which layer a function lives in.
- **Deterministic and verifiable**: nondeterministic capabilities (`random`,
  `time.now`, `uuid`) are not banned (full-parity goal) but are routed through
  declared, journaled effects so governed runs still replay deterministically.

### Runtime model (what constrains scope, and what does not)

Devlish is **not** an inherently sandboxed or single-threaded language. Those
are properties of a specific embedding, not the language:

- **"Sandboxed"** describes a *host* that withholds effects — chiefly the WASM
  embedding, which only gets the capabilities its JS host chooses to import.
  The native runtime already performs real I/O through `HostEffects`
  (`HTTP_REQUEST`, `FILE_*`, `SERVICE_CALL`), so nothing here is blocked by a
  sandbox in general.
- **"Single-threaded"** describes today's sequential bytecode VM, not a language
  rule.

The actual gate on the standard library is **determinism + verifiability plus
declared, permissioned, journaled effects** (Permissions/Boundaries manifest,
DEVL-68; effect journaling + replay, DEVL-122). A capability is in scope when it
can be expressed as a declared host effect and either is deterministic or
journals enough to replay deterministically. So OS/process/network/concurrency
modules are **effect-gated or determinism-gated, not categorically excluded** —
sockets generalize the HTTP effect we already have; a subprocess call is a
declared effect like any other; concurrency needs a deterministic-scheduling
design but is not forbidden.

## Method: language first, methods second

The assignment splits cleanly in two:

1. **"The language itself is insufficient."** A Python method is impossible to
   express because Devlish lacks a base capability (no regex engine, no bytes
   type, no callables, no modulo operator). These are **language-core gaps** and
   must be built first because many modules depend on them. See Tier 0.
2. **"The method itself is missing."** The base capability exists (lists,
   records, strings, f64 numbers, HTTP, filesystem) but the specific helper has
   not been written. These are **module tickets**. See Tiers 1-3.

## Current baseline (what Devlish already has)

From `crates/devlish_vm/src/lib.rs` (65 opcodes, 58 builtin arms) and
`crates/devlish_core/src/lib.rs` (parser, `Import` system):

- **Data model**: serde_json values only — `nil, string, number (f64), boolean,
  list, record`. No integer/float distinction, no decimal, no bytes, no set, no
  date type (dates are ISO strings).
- **Collections**: rich — `count, first, last, unique, flatten, minimum,
  maximum, sum, average, reverse, sort, find_where, filter_where, reject_where,
  any_where, all_where, partition_where, group_by, index_by, take, drop, zip,
  chunk, union, intersection, difference, map_transform, pluck`.
- **Strings**: `uppercase, lowercase, trim, normalize_whitespace, slugify,
  title_case, sentence_case, words, contains_text, starts_with_text,
  ends_with_text, replace, split, join` + `STR_*` opcodes.
- **Numbers**: `sum, average, minimum, maximum, round, abs` + `ADD/SUB/MUL/DIV`.
  No modulo, no power, no floor/ceil/sqrt/trig/log, no bitwise.
- **Dates**: `date_parse, date_add_days, days_between, business_days_between`.
- **Records**: `keys, values, entries, has_fields, matches_shape, type_of`.
- **I/O and effects**: `HTTP_REQUEST/DOWNLOAD`, `FILE_*` (copy/move/mkdir/delete/
  exists/stat/list/glob), `READ_FILE/LOAD_FILE/EXPORT`, `XLSX/PDF/DOCX` readers,
  `SERVICE_CALL`, `CHECKPOINT`, effect journaling + deterministic replay
  (DEVL-122).
- **Modules**: `Import "<path>.dvl"` inlines a file's symbols into a flat global
  namespace with collision detection; resolved via file-relative dirs,
  `devlish.toml` project dirs, `DEVLISH_PATH`, and `~/.devlish/lib/`. No
  namespacing, no qualified names, no selective import, no bundled stdlib.

## Tier 0 - Language-core gaps (build these FIRST; everything blocks on them)

| ID | Capability | Why it blocks parity | Blocks modules |
|----|-----------|----------------------|----------------|
| L1 | **Module namespace system + bundled optional stdlib** -- SHIPPED (DEVL-131): `Use the <name> module.` / `Use <a> and <b> from the <name> module.` with possessive qualification (`math's pi`), bundled-stdlib-first resolution embedded in the toolchain binary, source-closure integrity (`stdlib:<name>.dvl`) and a package `stdlib` version record. First bundled module: `math` (constants). | User-selected: named, namespaced, importable modules; qualified/selective import; a stdlib that ships with the toolchain and imports by well-known name with versioning. | ALL modules |
| L2 | **First-class function / block callbacks** -- SHIPPED (DEVL-132): `map/filter/reject/find/any of/all of/reduce/sort-by` accept arbitrary expressions over `item` (fields via `<field> of item`) and, in class programs, named helper methods via `using <method>`. Compiled as inline index loops; method calls inline the callee body (alpha-renamed, recursion rejected at compile time); `sort_by_keys` VM builtin for expression sort keys. | `map(f)`, `sorted(key=)`, `filter(f)`, `functools.reduce`, `itertools` all take a function. | itertools, functools, operator, collections(Counter) |
| L3 | **Regex primitive** -- SHIPPED (DEVL-133): `matches the pattern` condition, `first match of` (match record with groups/named/offsets), `all matches of`, `replace matches of ... with`, `split ... by pattern`, `ignoring case` flag; `regex` crate behind five pure VM builtins; literal patterns validated at compile time. | Needed for `re` and any pattern helper. | re, textwrap(some), string |
| L4 | **Numeric tower: integer + Decimal + Fraction** -- SHIPPED (DEVL-134): `decimal 19.99` exact-from-source literals, `fraction 1 over 3` reduced rationals, tagged-JSON representation surviving journal/checkpoint/WASM, Python-style mixing rules (float mixes error loudly), `round ... to N decimal places` with 7 rounding modes (half-even default), exact sum/average/min/max/sort, checked 64-bit integer arithmetic. Remaining for the decimal module (DEVL-153): precision contexts, quantize. | f64-only breaks exact money math, `math.isqrt`, big ints, `decimal`, `fractions`. Critical for DealStar financial precision. | decimal, fractions, math(int ops), statistics(exact) |
| L5 | **Bytes / binary value type + text<->bytes codecs** | No bytes type; blocks all byte-oriented modules. | base64, hashlib, hmac, struct, binascii, secrets |
| L6 | **Arithmetic operators: modulo, integer division, exponent** -- SHIPPED (DEVL-136): `modulo`/`%`, `integer divided by`/`//`, `to the power of`/`**`/`^`, `squared`/`cubed`; Python semantics across the numeric tower (floor mod/div for int+fraction, dividend-sign `%` and truncating `//` for Decimal); power binds tighter than times; zero/overflow are loud errors. | No `%`, `//`, `**`. Pervasive across math/statistics/algorithms. | math, statistics, itertools, random |
| L7 | **Bitwise operators (`and/or/xor/not/shift`)** | Needed for struct, hashing helpers, low-level parity. | struct, binascii, hashlib(helpers) |
| L8 | **Deterministic randomness as a declared, journaled effect** | `random`/`secrets`/`uuid4` are nondeterministic; must be seeded and journaled so replay is deterministic. | random, secrets, uuid |
| L9 | **Clock/now as a declared, journaled effect** | `time`, `datetime.now`, `uuid1`, `calendar(now)` read the wall clock; must be injected + journaled. | time, datetime(now), calendar, uuid |
| L10 | **String formatting mini-language** | Padding, alignment, thousands separators, fixed precision, f-string-style interpolation. Devlish only has `plus` concatenation. | string, everything that formats output |
| L11 | **Lazy iterators / bounded generators** | `itertools.count/cycle/repeat` are infinite; Devlish lists are eager. Offer lazy or explicitly-bounded variants. | itertools, functools(partial pipelines) |
| L12 | **Richer typed errors / exception values** | Python modules raise typed exceptions; Devlish has `Fail`/`Try`/`Otherwise` only. Needed for faithful error surfaces. | json(parse errors), decimal, re, struct |
| L13 | **Deterministic concurrency model** (added after scope correction) | Structured concurrency + journaled, replayable schedule so parallelism cannot make a governed run irreproducible. Later-phase design item. | threading, asyncio, multiprocessing |

## Tiers 1-3 - Module build order (one Jira ticket per module)

Priority derives from (a) real-world Python import frequency and (b) DealStar's
financial / document-workflow context (exact decimals, dates, CSV, hashing for
provenance all rank up).

### Tier 1 - High (build first after Tier 0 unblocks them)

| Module | Key surface to build | Blocked by |
|--------|---------------------|-----------|
| `math` | sqrt, pow, floor, ceil, trunc, gcd/lcm, factorial, log/log2/log10, exp, trig, pi/e/tau, isnan/isinf, copysign, hypot | L4, L6 |
| `statistics` | mean, median, mode, pstdev/stdev, pvariance/variance, quantiles, harmonic/geometric mean | L4, L6 |
| `datetime` | date/time/datetime/timedelta values, formatting (strftime-equiv), parsing, arithmetic, weekday, ISO week | L9(now), L10 |
| `string` | constants (ascii/digits/punctuation), Template/format helpers, capwords, padding | L3, L10 |
| `json` | parse-string, dump-string, pretty, sort-keys (native values already; add string<->value) | L12 |
| `re` | search, match, fullmatch, findall, finditer, sub, split, groups, named groups, flags | L3 |
| `collections` | Counter, defaultdict, OrderedDict, deque, namedtuple (over set/record + a set type) | L2 (Counter) |
| `itertools` | chain, product, permutations, combinations, accumulate, groupby, islice, count/cycle/repeat | L2, L11, L6 |
| `functools` | reduce, partial, cache/lru_cache, cmp_to_key, wraps | L2 |
| `random` | random, randint, uniform, choice, sample, shuffle, seed, gauss | L8, L6 |
| `decimal` | exact decimal arithmetic, rounding modes, quantize, contexts | L4 |
| `csv` | reader/DictReader/writer/DictWriter parity (partial today via EXPORT/XLSX) | none (extend existing) |
| `textwrap` | wrap, fill, shorten, indent/dedent, truncate | L3(optional) |
| `os` + `os.path` + `pathlib` | path join/split/basename/dirname/suffix/parts, exists/isdir, env vars, cwd (extends FILE_* opcodes) | none (extend) |
| `hashlib` | sha256/sha1/md5/sha512/blake2, hexdigest, streaming update | L5 |

### Tier 2 - Medium

| Module | Key surface | Blocked by |
|--------|------------|-----------|
| `operator` | itemgetter/attrgetter/add/mul/etc. as named callables | L2 |
| `enum` | Enum/IntEnum/Flag as declared value sets | L1 |
| `dataclasses` | record schemas with defaults/typing (leverages `matches_shape`) | L1 |
| `heapq` | heappush/heappop/heapify/nlargest/nsmallest | L6 |
| `bisect` | bisect_left/right, insort | none |
| `base64` | b64/b32/b16 encode/decode, urlsafe | L5 |
| `uuid` | uuid4 (random), uuid1 (time), uuid5 (namespace/sha1) | L5, L8, L9 |
| `secrets` | token_hex/token_urlsafe/choice/randbelow | L5, L8 |
| `calendar` | monthrange, weekday, isleap, month/day names, calendar grids | L9(now) |
| `zoneinfo` | timezone-aware datetime, offsets, DST | datetime |
| `urllib.parse` | quote/unquote, urlencode, urljoin, urlsplit/urlparse | L10 |
| `html` | escape/unescape, entity handling | none |
| `fractions` | exact rational arithmetic, from float/decimal, limit_denominator | L4 |
| `copy` | shallow copy, deepcopy of records/lists | none |
| `pprint` | pretty-format nested records/lists | L10 |
| `difflib` | ratio, get_close_matches, unified_diff | none |
| `unicodedata` | normalize (NFC/NFD), category, name, numeric | L5(optional) |
| `logging` | leveled diagnostics mapped to trace/events (aligns with DEVL-109) | none |
| `shutil` + `tempfile` | rmtree/copytree/which; temp file/dir (extends FILE_*) | none |

### Tier 3 - Low / effect-gated / deferred (build last, some need policy)

| Module | Notes | Blocked by |
|--------|-------|-----------|
| `struct` | binary pack/unpack | L5, L7 |
| `binascii` | hexlify/crc32 | L5, L7 |
| `hmac` | keyed hashing | L5, hashlib |
| `array` | typed numeric arrays (may fold into list) | L5 |
| `time` (module) | monotonic/sleep/perf_counter as declared effects | L9 |
| `xml` / `html.parser` | tree parsing (large; consider a native reader like XLSX/PDF) | none |
| `email` / `mimetypes` | MIME assembly/parsing | none |
| `pickle` | replace with a Devlish-native, auditable serialization; do NOT mirror pickle's arbitrary-code semantics | L5 |
| `glob` | already covered by `FILE_GLOB`; expose stdlib surface | none |

### Effect-gated and determinism-gated (not categorically excluded)

These are NOT antithetical to Devlish; they are gated on a declared host effect
and, where they add nondeterminism, on journaling for replay (per the runtime
model above). Sequenced after the core stdlib but in scope for the full-parity
goal:

- **Process / OS effects** — `subprocess` (DEVL-187), `signal` (DEVL-190),
  `sys` argv/env/exit (DEVL-191), `mmap`: declared host effects behind the
  Permissions manifest, journaled. `subprocess` output is journaled so a
  governed replay does not re-execute.
- **Networking** — `socket`/`ssl`/`selectors` (DEVL-188): generalize the
  existing `HTTP_REQUEST` effect into a lower-level declared socket effect;
  journaled.
- **Data services** — `sqlite3` (DEVL-189) and similar: expose via a declared
  data-service effect (the `SERVICE_CALL` path), not by embedding a DB engine in
  the VM.
- **Concurrency** — `threading`, `asyncio`, `multiprocessing`: require a
  **deterministic-concurrency design first** (L13 / DEVL-186 — structured
  concurrency with a journaled, replayable schedule) so parallelism cannot make
  a governed run irreproducible. Tracked as a language-design gap, not an
  exclusion; the module tickets follow once L13 lands.

### Genuinely deferred (for reasons other than sandboxing)

- `ctypes` / FFI — arbitrary native code defeats provenance and verifiability;
  deferred on safety grounds, revisited only behind a signed-capability model.
- `tkinter` / `turtle` / `curses` / `wave` / `audioop` — GUI/terminal/audio
  surfaces with no display or device target in current deployments; low value,
  reconsidered only if a host provides the surface.

## Sequencing summary

1. **L1 (namespaces + bundled stdlib)** unblocks literally everything — build it
   first.
2. Build the other Tier 0 primitives (L2-L12) in parallel where independent.
   L4 (numeric tower) and L5 (bytes) are the widest unblockers after L1.
3. Ship Tier 1 modules as each blocker clears; `math`, `datetime`, `json`,
   `csv`, `os/pathlib` can start earliest (fewest deep blockers).
4. Tier 2, then Tier 3.
