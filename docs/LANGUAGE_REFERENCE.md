# Devlish Language Reference (Authoring Guide)

Last updated: 2026-07-10
Status: Current authoring guide.

Devlish is documented as a controlled subset of English.
Internal grammar notation is descriptive only.

For a parser-faithful grammar derived from the current implementation, see
`docs/LANGUAGE_GRAMMAR.ebnf`.

For the current reserved-word lists and definitions, see
`docs/RESERVED_WORDS_CURRENT.md`.

For the current split between surface grammar, built-in nouns, and the actual
core standard library, see `docs/STANDARD_LIBRARY_CURRENT.md`.

For the concrete Devlish 2.0 architecture and execution plan, see
`docs/DEVLISH_2_0_PLAN.md`.

For the beginner-first teaching plan, see `docs/BEGINNER_COURSE.md`.

For the current language gaps that affect teaching and beginner coverage, see
`docs/DEVLISH_LANGUAGE_GAPS.md`.

This is the practical grammar for writing `.dvl` files today. The current
implementation is the Rust compiler and shared bytecode VM; `.dvl` files run
directly through `devlish run` or compile explicitly to `.dvlc.json`.

## 1) File Basics

- One statement per line.
- `#` starts a comment.
- Blank lines are allowed.
- Author files in:
  - `app/devlish/rules/*.dvl`
  - `app/devlish/definitions/*.dvl`
  - service or tool adapters declared by the host runtime

## 2) Rule File Grammar (Most Common)

### Load input

```text
Load document
Load contract.pdf
```

### Ask for console input

```text
Ask "What is your name?" as user name
Ask multiline "Paste the review notes" as review notes
Read input as favorite color
Read multiline input as raw notes
Read stdin as raw payload
```

`Ask` shows a prompt and saves one input value. `Ask multiline` marks the value
as multi-line input. `Read input` and `Read stdin` save input without a prompt;
the `Read multiline ...` forms do the same for multi-line text.

### Produce output

```text
Print user_name
Show "done"
Write user_name to "tmp/name.txt"
Overwrite user_name to "tmp/name.txt"
Export report to "tmp/report.json"
Export rows to "tmp/rows.csv" as CSV
Append "processed\n" to file "tmp/run.log"
```

`Write` saves text output. `Overwrite ... to ...` is the explicit overwrite
form. `Export` saves text too, and exports records/lists as pretty JSON by
default. `Export ... as CSV` writes a list of records as a CSV table.
`Append ... to file ...` explicitly appends text instead of overwriting.

### Read typed files

```text
Read JSON from "packet.json" as packet
Read CSV from "invoices.csv" as invoices
Read XLSX cell "Sheet1!B2" as total_amount
Read PDF text "packet.pdf" as packet_text
Read DOCX text "contract.docx" as contract_text
```

JSON and CSV reads use the host file reader at runtime and save structured
records/lists. PDF, DOCX, and XLSX reads use host-provided typed file effects.

### Define terms

```text
liability cap is the maximum amount payable
Termination Clause is required language for ending the agreement
```

Use `is` for definitions.

### Require content

```text
Document must contain liability clause
Document should have Termination Clause
Require indemnification clause in document
Check for governing law clause
```

### Extract values

```text
Find liability cap and save as liability_cap
Extract effective date and save as effective_date
Find contract value
```

If `save as` is omitted, Devlish auto-creates a variable name.

### Validate values

```text
liability_cap must be at least 1000000
termination_days must be at most 30
status must equal "approved"
notes must contain "signed"
invoice_code must match "INV-*"
contact_email must be present
legacy_field must be missing
status must be one of list of "approved", "pending"
Verify contract_value is at least 10000
```

Validation statements record structured validation results and fail the run when
the rule does not pass. Use `Require` for arbitrary conditions, `Expect` for
test assertions, and `must` / `should` validation phrases for business-rule
checks on named values.

### Assign calculations/logic

```text
total exposure equals liability_cap + deductible
adjusted amount equals base amount plus surcharge if risk score > 70
```

Supported math words/operators:
- `plus`, `minus`, `times`, `divided by`
- `+`, `-`, `*`, `/`

Supported condition words:
- comparisons: `> < >= <= equals`
- boolean: `and`, `or`

### Lists, records, and helpers

```text
statuses equals list of "approved", "pending", and "rejected"
invoice equals record with 1200 as amount and "pending" as status
first_status equals first of statuses
invoice_amount equals amount of invoice
Set amount of invoice to 1300
sorted_statuses equals sort statuses
invoice_keys equals keys of invoice
cleaned_statuses equals map statuses to trim item
kept_statuses equals reject cleaned_statuses where item is "pending"
status_count equals reduce kept_statuses starting at 0 with total and item to total plus 1
pending_invoice equals find invoices where status equals "pending"
large_invoices equals filter invoices where amount >= 1000
by_status equals group invoices by status
indexed_invoices equals index invoices by invoice_id
first_three equals take 3 of invoices
remaining equals drop 3 of invoices
chunks equals chunk invoices by 10
combined_statuses equals union of statuses and archived_statuses
```

### Callback expressions in collection helpers

`map`, `filter`, `reject`, `find`, `any of`, `all of`, `reduce`, and
`sort ... by` accept an arbitrary expression over each element, not just a
field name. The current element is bound to `item` (reduce lets you name both
the accumulator and the element). Reach an element's record fields with
`<field> of item`:

```text
doubled equals map xs to item times 2
large_totals equals filter invoices where amount of item times quantity of item > 1000
open_or_new equals filter invoices where status of item is "open" or status of item is "new"
first_big equals find invoices where amount of item > cap
any_overdue equals any of invoices where days of item > 30
all_flags_set equals all of flags
total equals reduce xs starting at 0 with total and item to total plus item
by_value equals sort invoices by amount of item times quantity of item
```

These compile to inline loops over the list; there are no function values at
runtime, so behavior is fully deterministic and journals identically to the
equivalent `For each` loop. Like `For each`, the `item` binding lives in the
flat variable namespace: an outer variable with the same name is overwritten
by the loop.

In class-style programs a helper method can be the callback. `using <method>`
calls the method once per element with the element as its argument:

```text
cleaned equals map raw_rows using normalize row
kept equals filter rows using is valid
```

Method calls (including these helper callbacks) are inlined at compile time,
so recursive method calls are a compile error.

Nested field reads and writes use the same English path shape:
`amount of invoice of packet`. `Set amount of invoice of packet to 1300`
updates `packet.invoice.amount`; missing intermediate records are created during
the update, while reads of missing fields return `nil`.

Record field requirements and schema-like shape checks are available in
conditions:

```text
invoice_shape equals record with "number" as amount and "text" as customer
Require invoice has fields amount, customer
Require invoice matches shape invoice_shape
```

Use commas for multiple field names in `has fields`; `and` keeps its normal
boolean meaning in conditions. Shape values use simple type names such as
`text`, `number`, `boolean`, `list`, `record`, and `any`.

Current helpers include `count`, `first`, `last`, `unique`, `flatten`,
`minimum` / `min`, `maximum` / `max`, `sum`, `average`, `reverse`, `sort`,
`find`, `filter`, `reject`, `any`, `all`, `group`, `index`, `partition`,
`take`, `drop`, `zip`, `chunk`, `union`, `intersection`, `difference`,
`uppercase`, `lowercase`, `trim`, `normalize whitespace`, `slugify`,
`title case`, `sentence case`, `words`, `contains ... in ...`,
`starts with ... in ...`, `ends with ... in ...`, `length`, `round`, `abs`,
`replace`, `split`, `join`, `item`, `slice`, `keys`, `values`, `entries`,
`has_fields`, `matches_shape`, and `type_of`.

### Arithmetic operators

Beyond `plus`, `minus`, `times`, and `divided by`:

```text
remainder equals total modulo 3
buckets equals total integer divided by 12
growth equals principal times decimal 1.05 to the power of years
area equals side squared
volume equals side cubed
```

- `modulo` (or `%`) follows Python's sign rules: `-7 modulo 3` is `2` for
  integers and fractions; decimals keep the dividend's sign.
- `integer divided by` (or `//`) floors for integers, floats, and fractions;
  decimals truncate toward zero (Python `Decimal` semantics).
- `to the power of` (or `**` / `^`) binds tighter than `times`, so
  `3 times 2 to the power of 2` is `12`. `squared` and `cubed` are shorthand
  for powers of 2 and 3. Decimal and fraction exponents must be whole
  numbers (fractional powers require an explicit float conversion).
- Modulo or integer division by zero is a loud error for every numeric kind.

### Exact numbers: decimals and fractions

Plain numbers are floats (with whole numbers kept as integers). For money and
other exact arithmetic, use decimals and fractions:

```text
price equals decimal 19.99
total equals price times 3
tax equals round total times decimal 0.0825 to 2 decimal places
share equals fraction 1 over 3
sum_exact equals sum of list of decimal 0.1, decimal 0.2, decimal 0.3
converted equals decimal of some_number
plain equals numeric value of total
```

- `decimal <digits>` is exact from the source text: `decimal 19.99` is
  exactly 19.99, and `decimal 19.99 times 3` is exactly 59.97. `decimal of X`
  converts a number or string at runtime; a bad literal is a compile error.
- `fraction A over B` is an exact rational, always reduced (`fraction 2 over
  6` is 1/3). Read parts back with `numerator of f` / `denominator of f`.
  A zero denominator is a compile error when literal, a loud runtime error
  otherwise.
- Integers combine exactly with decimals and fractions. Mixing a decimal or
  fraction with a float in arithmetic is a loud error; convert explicitly
  with `decimal of X` or `numeric value of X` so precision loss is a
  decision, not an accident. Comparisons across kinds are allowed and compare
  quantities (`decimal 5.0 equals 5` is true).
- `round X to N decimal places` rounds exactly, defaulting to banker's
  rounding (half even). Add `rounding half up` (or `half down`, `up`, `down`,
  `ceiling`, `floor`) for other modes. The result is a decimal.
- `sum of`, `average of`, `minimum of`, `maximum of`, and `sort` are exact
  over lists containing decimals or fractions.
- Decimals print and concatenate as their quantity (`"total: " plus decimal
  19.99` is `"total: 19.99"`); in JSON output they appear as tagged records
  (`{"__type": "decimal", "value": "59.97"}`), so exactness survives
  journaling, checkpoints, and the browser runtime.

### Pattern (regex) helpers

Regular-expression helpers use standard regex syntax inside a double-quoted
pattern, with an English surface:

```text
ok equals yes if code matches the pattern "^[A-Z]{2}-[0-9]+$"
m equals first match of "(?P<user>[a-z]+)@(?P<host>[a-z]+)" in email
nums equals all matches of "[0-9]+" in text
clean equals replace matches of "[0-9]+" in text with "#"
parts equals split text by pattern "[,;] *"
hits equals all matches of "abc" in text ignoring case
```

- `matches the pattern` (or `matches pattern`) is a condition: true when the
  pattern is found anywhere in the text (anchor with `^`/`$` for a full
  match).
- `first match of` returns a match record with `text`, `start`/`end`
  (character offsets), `groups` (positional captures, null when unmatched),
  and `named` (named captures via `(?P<name>...)`) — or `nil` when nothing
  matches.
- `all matches of` returns the matched strings; `replace matches of ... with`
  replaces every occurrence (`$1` / `${name}` in the replacement refer to
  captures); `split ... by pattern` splits on every match.
- A trailing `ignoring case` makes the pattern case-insensitive. Inline flags
  such as `(?im)` also work inside the pattern.
- An invalid pattern written literally is a compile error; an invalid pattern
  arriving through a variable fails the run loudly.
- Plain `replace X in Y with Z` and `split X by Y` are unchanged: they match
  literal text, never patterns.

### Date helpers

```text
due_date equals add 7 days to "2026-06-30"
span equals days between "2026-06-30" and due_date
business_span equals business days between "2026-06-30" and due_date
```

Date helpers use ISO `YYYY-MM-DD` strings. Invalid dates produce `nil` or an
empty string depending on the helper. Business days count Monday through
Friday and do not include a holiday calendar.

### Loops and loop control

```text
For each status in statuses:
  Print status

While retry_count is less than 3:
  retry_count equals retry_count plus 1

Until status is "complete":
  status equals "complete"
```

Use `Continue` inside a loop to skip to the next pass. Use `Break` to stop the
loop.

### Failure, requirements, and recovery

```text
Require review_status is "approved" otherwise fail with "Review must be approved"

Try:
  Require review_status is "approved" otherwise fail with "Review must be approved"
Otherwise:
  review_status equals "manual_review"

Fail with "Contact email is required"
```

`Try` runs the indented body. If a validation, requirement, service call, file
read/write, or explicit `Fail with` errors, Devlish stores `last_error` and
runs the `Otherwise` body instead of stopping the whole program.

### Assertions and test runs

```text
amount equals 1200
Expect amount equals 1200 as "amount-is-1200"
Export assertions to "tmp/assertions.json"
```

Run with `devlish run file.dvl --test` to make failed assertions produce a
non-zero exit status.

### Imports and checkpoints

```text
Import "shared_rules.dvl"

Checkpoint "Review extracted fields before export"
Checkpoint "Approval needed" saving context as approval_state
```

`Import` resolves relative files at compile time. If a project has
`devlish.toml`, imports also search the project root, `devlish/`, and `lib/`.
Workflow fragments imported inside class-style methods are inlined before
bytecode compilation. Duplicate imports and imported-name collisions produce
compile diagnostics. `Checkpoint` pauses execution and returns structured
context to the caller.

### Modules and Use

```text
Use the math module.
Use pi and tau from the math module.

circumference equals math's tau times radius
Set m to statistics' mean of scores
```

`Use` brings in a named, namespaced module (DEVL-131). Unlike `Import`, which
inlines a file's symbols into the flat namespace, a `Use`d module keeps its
symbols behind the module name:

- `Use the math module.` makes the module's symbols available only through
  qualified possessive references: `math's pi`. Module symbols never collide
  with local names.
- `Use pi and tau from the math module.` additionally binds the chosen
  symbols to their plain names at the Use site; binding a name the file
  already defines is a compile error.
- Module names resolve to the standard library bundled inside the toolchain
  first (so `Use the math module` works identically in the CLI, MCP server,
  and browser WASM compiler, with no filesystem), then to `<name>.dvl` on the
  usual search paths (`devlish.toml` project dirs, `DEVLISH_PATH`,
  `~/.devlish/lib/`).
- Modules ending in `s` use the trailing-apostrophe possessive:
  `statistics' mean`.
- Repeating `Use` for the same module is legal; the module body is inlined
  once per compilation unit.
- Bundled module sources join the artifact's `source_hash` closure (listed as
  `stdlib:<name>.dvl` in `source_files`), and the package records the stdlib
  version and module names under a `stdlib` key, so governed artifacts stay
  tamper-evident.

Qualified references to modules that were never `Use`d, or to names a module
does not define, are compile errors.

### Notifications

```text
Email legal@company.com
Notify operations team if contract_value > 500000
```

### HTTP Requests

Devlish provides HTTP verbs as native keywords for fetching and sending data
to web APIs. Each verb is its own English-natural construct.

**Retrieving data:**

```text
Get the url at "https://api.census.gov/data/2020/acs" as census_data
```

**Submitting data:**

```text
Post to "https://api.example.com/submit" with payload as response
```

**Updating a resource:**

```text
Put to "https://api.example.com/items/42" with updated_item as response
```

**Removing a resource:**

```text
Delete the url at "https://api.example.com/items/42" as response
```

The response is a record with `status`, `headers`, and `body`. If the
response content type is JSON, `body` is a parsed record; otherwise it
is a string.

| Verb | Syntax | Has body |
|------|--------|----------|
| Get | `Get the url at "<url>" as <var>` | No |
| Post | `Post to "<url>" with <body> as <var>` | Yes |
| Put | `Put to "<url>" with <body> as <var>` | Yes |
| Delete | `Delete the url at "<url>" as <var>` | No |

### Filesystem Operations

Devlish provides native keywords for common filesystem operations. Each
compiles to a dedicated opcode and dispatches through the host.

**Copying and moving files:**

```text
Copy file from "/inbox/receipt.pdf" to "/receipts/2026-07/receipt.pdf"
Move file from "/tmp/draft.txt" to "/archive/final.txt"
```

Both create parent directories automatically. Copy supports directories
(recursive).

**Creating and deleting:**

```text
Create directory "/receipts/2026-07/"
Delete file "/tmp/scratch.txt"
```

`Delete file` removes files or directories (recursive).

**Checking existence and metadata:**

```text
Check if "/inbox/receipt.pdf" exists as file_found
Get file info for "/inbox/receipt.pdf" as info
```

`Check if ... exists` stores `true` or `false`. `Get file info` returns a
record with `path`, `type` ("file", "directory", or "symlink"), `size`
(bytes), and `modified` (Unix timestamp).

**Listing and globbing:**

```text
List files in "/inbox/" as entries
Find files matching "*.pdf" in "/inbox/" as pdf_files
```

`List files` returns a sorted list of filenames (not full paths).
`Find files matching` uses glob patterns and returns a sorted list of full
paths. Both return lists that can be iterated with `For each`.

| Keyword | Syntax | Returns |
|---------|--------|---------|
| Copy file | `Copy file from <src> to <dst>` | (none) |
| Move file | `Move file from <src> to <dst>` | (none) |
| Create directory | `Create directory <path>` | (none) |
| Delete file | `Delete file <path>` | (none) |
| Check if exists | `Check if <path> exists as <var>` | boolean |
| Get file info | `Get file info for <path> as <var>` | record |
| List files | `List files in <path> as <var>` | list |
| Find files matching | `Find files matching <pattern> in <dir> as <var>` | list |

### Structured Output

A Devlish program can return structured data to its caller using `Respond`
and `Fail`.

**Returning a result (exit 0):**

```text
Respond with result
Respond with record with "completed" as status and file_list as files
```

`Respond` serializes the value as JSON, writes it to the program's output,
and stops execution. This is how a program returns data when called as a tool.

**Returning an exception (exit 1):**

```text
Fail with record with "awaiting_input" as status and "Cannot classify" as message
Fail with "Something went wrong"
```

When `Fail` receives a record, it serializes the value as JSON and exits with
a non-zero code. When it receives a string, it exits with the error message
as before.

The calling system (an LLM, another tool, or a human) interprets the JSON
based on context. The program author controls the shape of the response.

### Service Calls (Legacy)

The following patterns compile to the `SERVICE_CALL` opcode. They require a
host that provides the named service (e.g., a WASM host with service bindings).
For HTTP-based integrations, prefer the HTTP keywords above.

```text
Search the <Service> for <query>
Create <Service> entry with <fields>
Send email via <Service> to <recipient>
Send message to <recipient>
Email <recipient>
Notify <recipient>
```

## 3) Program Manifest (Permissions and Access)

A Devlish program can declare its required permissions, resource boundaries,
and allowed callers in a manifest header. The manifest appears at the top of
the file before the program body.

### Declaring permissions

```text
Permissions:
  Read files from "/inbox/"
  Write files to "/receipts/"
  HTTP requests
  Filesystem operations
  Call Gmail service
```

When a manifest with permissions is present, the VM enforces them at runtime.
An undeclared effect produces a "Permission denied" error. Programs without
a manifest remain unrestricted (backward compatible).

Available permission types:

| Permission | Syntax |
|------------|--------|
| Read files | `Read files` or `Read files from "<path>"` |
| Write files | `Write files` or `Write files to "<path>"` |
| HTTP requests | `HTTP requests` or `HTTP requests to "<domain>"` |
| Filesystem | `Filesystem operations` or `Filesystem operations on "<path>"` |
| Service calls | `Call <ServiceName> service` |

Scoped permissions (with `from`, `to`, or `on`) restrict the effect to paths
starting with the given prefix. Unscoped permissions allow all paths.

### Declaring boundaries

```text
Boundaries:
  No writes outside "/Users/admin/Dropbox/Financials/"
```

Boundaries constrain where effects can reach.

### Declaring callers

```text
Callers:
  Any MCP client
```

Caller declarations are metadata for tooling and documentation. They compile
into the `.dvlc.json` package so the calling system can inspect them before
invocation.

### Declaring inputs

A program that receives runtime variables through `--input` JSON can declare
their names so tooling knows they are provided externally:

```text
inputs:
  latitude
  longitude
```

Each indented line names one expected input. The lint pass (see below) treats
declared inputs as bound, so a program driven entirely by `--input` no longer
produces spurious "used but never assigned" warnings for those names. Inputs
are documentation and lint hints only; they are not enforced at runtime.

### Compiled metadata

The manifest compiles into a `manifest` field in the `.dvlc.json` bytecode
package:

```json
{
  "manifest": {
    "permissions": [
      {"kind": "read_file", "scope": "/inbox/"},
      {"kind": "write_file", "scope": "/receipts/"}
    ],
    "boundaries": ["No writes outside \"/Users/admin/Dropbox/\""],
    "callers": ["Any MCP client"],
    "inputs": ["latitude", "longitude"]
  }
}
```

This allows MCP clients and other tooling to inspect what a program does
without running it.

## 4) Trigger Lines (Parsed as Metadata)

These are recognized by the parser:

```text
Every day at 9am:
Every Monday at 8am:
Every 2 hours:
Every hour:
When an Order is created:
When invoice is submitted:
```

Current behavior: triggers are parsed and reported as metadata for tooling/linting.

## 5) Class/Module Style Grammar (Advanced)

If the first non-comment line matches this pattern, Devlish uses class parsing:

```text
Module's Class Name:
Module's Class Name based on ParentModule's ParentClass:
```

Methods:

```text
calculate exposure using contract value and risk score:
  total exposure equals contract value * risk score
  respond with total exposure
```

Private methods:

```text
privately validate cap:
  cap is maximum allowed amount
  cap must be at most 1000000
  respond with yes if cap <= 1000000
```

Method blocks end with `respond with ...`.

## 6) Naming and Term Rules

- You can write human-readable multi-word names; Devlish normalizes to snake_case internally.
- **Lowercase articles (`a`, `an`, `the`) are stripped from multi-word names;
  capitalized articles are kept.** Capitalization is the intent signal: a
  lowercase article is filler and is dropped, but any capitalization (`A`,
  `An`, `The`) marks the word as an intentional part of the name and preserves
  it. So `Set the discount equal to X` assigns `discount` and `total for the
  year` becomes `total_for_year`, while `Set exhibit A to 1` keeps `exhibit_a`
  and `Set The Hague to 1` keeps `the_hague`. A name that is *only* an article
  (`a` used alone) is left as-is, and a name written with explicit underscores
  is treated as a single token and is never touched: `the_white_house` stays
  `the_white_house`. To keep a lowercase article as a literal part of a name,
  either capitalize it or join it with an underscore.
- In a `Set` statement, the filler between the target and the value is absorbed:
  `Set the discount equal to X`, `Set the discount equals X`, and
  `Set the discount to be X` all assign `discount`.
- **Strings are double-quoted only.** A single quote is never a string
  delimiter: apostrophes are ordinary English text (possessives,
  contractions), so `"the buyer's total"` needs no escaping and `'draft'` is
  not a string literal.
- **Possessives fold into names with `_` as the only connector.** In a name
  position (a `Set` target, a record field key), `salesperson's commission`
  becomes `salesperson_commission` and `owners' equity` becomes
  `owners_equity` (the possessive marker drops; a bare apostrophe like
  `o'brien` just drops). Read the value back with the plain phrasing:
  `salesperson commission`.
- **In expression position, `X's Y` is a module reference** (see Modules and
  Use). If `X` is not a `Use`d module, that is a compile error, not a silent
  null, so a typo like `maht's pi` fails loudly. To read a stored possessive
  name, use the apostrophe-free phrasing (`salesperson commission`).
- **Module values are read-only.** A `Set` target always folds to a local
  name, even when the owner is a `Use`d module: `Set math's pi to 5` binds a
  local `math_pi` and leaves the module's `math's pi` untouched.
- For extracted variables, prefer explicit snake_case in `save as`.
- Capitalized proper nouns should be defined in `definitions` or be known built-ins/services/models.

Built-in terms include:
- `Host`
- `Document`
- `Email`
- `File`
- `System`

## 7) Recommended Authoring Pattern

1. `Load document`
2. define key terms (`is`)
3. require expected content (`Document must contain ...`)
4. extract data (`Find ... and save as ...`)
5. validate thresholds (`must be at least/at most`)
6. optional `Print`, `Show`, `Write`, `Export`, or notifications

## 8) Canonical Example

```text
# Contract review rule

Load document

liability cap is maximum total liability amount
termination notice is days required before termination

Document must contain Liability Clause
Document must contain Termination Clause

Find liability cap and save as liability_cap
Find termination notice and save as termination_days

liability_cap must be at least 1000000
termination_days must be at least 30
```

## 9) Credentials and Environment

Programs that use service adapters or external APIs need credentials. Devlish
provides a secure credential pipeline that never exposes secrets to program
variables.

### .env files

Create a `.env` file next to your `.dvl` program or in `~/.devlish/.env`:

```text
GMAIL_OAUTH_TOKEN=ya29.a0AfH6SM...
API_KEY=sk-proj-abc123
```

Resolution order (highest priority first):
1. CLI `--env` parameter
2. Program-local `.env` (same directory as the `.dvl` file)
3. Global `~/.devlish/.env`
4. System environment variables

### CLI override

```bash
devlish run program.dvl --env API_KEY=sk-test-123 --env DEBUG=true
```

`--env` is repeatable for multiple values.

### Security

Credentials flow through `HostEffects.resolve_credential()` to host
methods only. Programs cannot read credentials directly. Credentials
never appear in program output or checkpoint data.

## 10) Validation/Linting

Use:

```bash
devlish lint app/devlish/rules/contract_review.dvl
```

Add `--json` for machine-readable diagnostics.

**Errors** stop compilation. In particular, bracket characters (`[` `]`) are not
Devlish grammar and are rejected as hard errors in expression position, so a
line like `line_items equals list of [1200, 450, 89]` fails with a message
pointing you to the `list of 1200, 450, 89` phrasing. (Brackets inside a quoted
string are fine.)

**Warnings** do not fail the lint (the file is still reported valid). Today the
linter warns when an identifier is referenced but never bound by any assignment,
`Ask`, import, loop variable, method parameter, defined term, or declared
`inputs:` entry (see the manifest section), which usually means a typo. Such a
name would otherwise evaluate to `null` silently. In `--json` output each
diagnostic carries a `severity` of `"warning"` (for these lint findings) or
`"error"` (for compile failures).
