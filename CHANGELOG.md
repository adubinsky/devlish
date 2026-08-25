# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- Single quotes are no longer string delimiters; string literals are double-quoted only. Apostrophes are ordinary English text everywhere, which removes an entire class of parser bugs where a possessive (`math's pi times 2`, `r equals math's pi if flag`) silently swallowed the rest of the line by "opening a string": the operator splitter, trailing-`if` splitter, and bracket guard no longer treat `'` as a quote. Possessive markers fold into names with `_` as the only connector (`Set salesperson's commission to 5` binds `salesperson_commission`, `owners' equity` becomes `owners_equity`; read back with the plain phrasing `salesperson commission`). In expression position `X's Y` remains a module reference and errors loudly when `X` is not a `Use`d module. `Import 'file.dvl'` must now be written with double quotes.

### Fixed
- `sum of X` and `average of X` now parse as the sum/average builtins. They were documented but had no parser rule, so they fell through to generic field access and silently evaluated to null (`avg of X` also accepted).
- `sort` orders numbers numerically. It previously compared stringified values, so `sort list of 10, 2` returned `[10, 2]`.
- Class-style compilation now rebases each method's constant indices and control-flow addresses (`CONST`, `JUMP`, `JUMP_IF_FALSE`, `TRY_BEGIN` handler) onto the concatenated instruction stream. Every method compiles with a fresh compiler, so these were method-relative; any method after the first read the wrong constants and jumped into the wrong method when executed from its `entry_point`. Latent until DEVL-132 made method bodies executable.

### Added
- Arithmetic operators: modulo, integer division, exponent (DEVL-136, epic DEVL-130): `total modulo 3` / `%`, `total integer divided by 12` / `//`, `principal times decimal 1.05 to the power of years` / `**` / `^`, plus `squared` and `cubed` shorthands. Exponentiation binds tighter than multiplication. Semantics match Python across the whole numeric tower: integer/fraction modulo follows the divisor's sign and integer division floors, decimal `%` keeps the dividend's sign and decimal `//` truncates toward zero, all with exact arithmetic (checked 64-bit integers, i128 fraction intermediates, repeated-squaring decimal powers). Modulo or integer division by zero, fractional decimal/fraction exponents, and overflow are loud errors.
- Numeric tower: exact decimals and fractions (DEVL-134, epic DEVL-130): `decimal 19.99` is exact from the source digits (`decimal 19.99 times 3` is exactly `59.97`, `decimal 0.1 plus decimal 0.2` is exactly `0.3`), `fraction 1 over 3` is an exact reduced rational, and both are tagged JSON records (`{"__type": "decimal", "value": "..."}`) so exactness flows unchanged through artifacts, journals, checkpoints, and the WASM boundary. Integers combine exactly with both; mixing a decimal or fraction with a float in arithmetic errors loudly with a pointer to `decimal of X` / `numeric value of X` (comparisons across kinds are allowed and compare quantities). `round X to N decimal places [rounding half up|half down|up|down|ceiling|floor]` does exact rounding with banker's (half even) as the default. `sum of` / `average of` / `minimum of` / `maximum of` / `sort` are exact over decimal or fraction lists. Integer arithmetic (`+ - *`) is now checked 64-bit (overflow errors loudly instead of silently losing precision above 2^53). Bad decimal literals and zero-denominator fraction literals are compile errors.
- Regex primitive with an English surface (DEVL-133, epic DEVL-130): `code matches the pattern "^[A-Z]{2}-[0-9]+$"` in conditions, `first match of P in T` (match record with `text`, character `start`/`end`, positional `groups`, and `named` captures, or nil), `all matches of P in T` (matched strings), `replace matches of P in T with R` (all occurrences; `$1`/`${name}` reference captures), and `split T by pattern P`. A trailing `ignoring case` sets the case-insensitive flag; `(?im...)` inline flags also work. Backed by the Rust `regex` crate as five pure VM builtins (`regex_test`, `regex_match`, `regex_find_all`, `regex_replace`, `regex_split`) -- deterministic, no effects to journal. Literal patterns are validated at compile time with the same engine (a bad pattern is a compile error); dynamic patterns fail the run loudly. Literal `replace`/`split` phrasings are unchanged.
- Callback expressions for collection helpers (DEVL-132, epic DEVL-130): `map`, `filter`, `reject`, `find`, `any of`, `all of`, `reduce`, and `sort ... by` now take an arbitrary expression over each element (`map xs to item times 2`, `filter invoices where amount of item times quantity of item > 1000`, `reduce xs starting at 0 with total and item to total plus item`, `sort invoices by amount of item times quantity of item`), with the element bound to `item` and record fields reached as `<field> of item`. Compound `and`/`or` predicates and arithmetic predicates route to the general form; single field/operator/value predicates keep the existing fast path. Everything compiles to inline index loops (the ForEach skeleton) -- no function values or call frames exist at runtime, so callback results journal and replay identically to the equivalent explicit loop. `sort` by an expression key computes keys per element and orders through a new stable `sort_by_keys` VM builtin (numeric keys compare numerically). In class-style programs `using <method>` passes a helper method as the callback (`map rows using normalize row`), and method calls in general now execute by inlining the callee's body at the call site with alpha-renamed locals (previously `CALL_METHOD` bytecode was emitted that no runtime implemented); recursive method calls are rejected at compile time. Bare `any of flags` / `all of flags` evaluate element truthiness.
- Module namespace system and bundled standard library (DEVL-131, epic DEVL-130): `Use the math module.` brings in a named module whose symbols stay behind the module name and are reached with English possessive qualification (`math's pi`, `statistics' mean`); `Use pi and tau from the math module.` selectively binds chosen symbols to their plain names (collision-checked against local definitions). Module names resolve to the standard library embedded in the toolchain binary first -- identical behavior in the CLI, MCP server, and browser WASM compiler with no filesystem -- then to `<name>.dvl` on the existing search paths. Bundled module sources participate in the `source_hash` closure (listed as `stdlib:<name>.dvl`) and packages that use them record the stdlib version and module names under a `stdlib` key. Qualified references to un-`Use`d modules or undefined symbols are compile errors. Ships the first bundled module, `math` (constants `pi`, `e`, `tau`; functions land with DEVL-134/DEVL-136). Class-style detection was tightened so a statement containing a possessive (`Set r to math's pi`) is no longer misread as a class header.
- Controlled release workflow: `registry.json` is an append-only, hash-chained event log mapping each `rule@version` through draft/approved/published/retired, with releases derived by folding events -- never edited, only superseded. `devlish release propose <rule.dvl> --author NAME` compiles the rule, runs its golden cases, and adds a draft binding artifact hash to evidence hash (failing cases refuse the propose); `release approve --approver NAME` enforces separation of duties (author cannot self-approve); `release publish` refuses overlapping effective windows for the same rule id; `release retire` and rollback-by-republishing append new events; `release list`/`release verify` inspect state and the chain. `devlish run --governed <registry>` refuses any artifact whose hash is not a currently published release (tampered files hash differently and are refused), and `--as-of` candidates are all checked so effective-date resolution happens over published releases only. Walkthrough in `docs/RELEASE.md` (DEVL-115, epic DEVL-110).
- Effect journaling and deterministic replay: `devlish run --journal <dir>` (with `--audit-log`) archives a governed run's exact bytecode, full input, and every host-effect exchange (HTTP, file reads/writes, stats, globs, service calls) as a content-addressed attachment linked from the audit record via `journal_sha256`. `devlish replay <log> [--line N]` re-executes the archived bytecode against the journaled effect responses -- offline, never touching the live world -- and verifies the output hash and instruction count against the record; any divergence (tampered attachment, changed response, different execution path) exits nonzero. Credentials never enter the journal (resolved below the journaled boundary). Determinism is asserted and tested: no clock/RNG in VM evaluation, canonical sorted-key serialization, defined IEEE-754 float semantics, identical inputs + responses yield byte-identical output (DEVL-122, epic DEVL-110).
- Execution provenance audit log: every run of a governed rule (a `Rule:` manifest section) emits one record at completion binding the output to the rule that produced it — rule id/version, artifact sha256 (canonical form, agrees with evidence bundles), canonical input/output sha256, success/failure, instruction count, runtime kind (native/wasm) and version, timestamp. `devlish run --audit-log <path>` (or `DEVLISH_AUDIT_LOG`) appends hash-chained JSON lines (`prev_sha256` links each record to the previous line, continuing across process restarts), and `devlish audit-verify <log>` walks the chain to detect a modified, reordered, or deleted record. devlish-runtime adds an `onAuditRecord` callback to `LoadToolOptions` so embedding apps persist records in their own store; wasm and native produce identical record shapes and hashes. Ungoverned programs emit nothing. A governed run whose record cannot be written fails rather than reporting success. Documented in `docs/AUDIT.md` (DEVL-114, epic DEVL-110).
- Test evidence bundles: `devlish evidence <rule.dvl> [--cases file.json] [--output evidence.json]` runs a governed rule's golden input/expected cases against the exact compiled artifact and emits a machine-readable, tamper-evident report (rule id/version, artifact sha256, compiler version, timestamp, per-case pass/fail with input/output/expected hashes, totals, and a `report_sha256` over the whole body). Exits non-zero if any case fails (or the case set is empty), so it can gate a release. `devlish evidence --verify <report.json>` recomputes `report_sha256` to detect tampering. Documented in `docs/EVIDENCE.md` (DEVL-113, epic DEVL-110).
- Effective-date rule resolution: `devlish run <v1> <v2> ... --as-of YYYY-MM-DD` runs the rule version whose effective window is in force on that date (for compliance recomputation under the historically correct rule), reporting the chosen `id`/`version` on stderr and erroring if none or more than one version applies. devlish-runtime adds a matching `selectVersion(artifacts, asOfDate)` helper and `isValidIsoDate`, so browser embeds resolve identically (DEVL-112, epic DEVL-110).
- devlish-runtime `tool.info.rule` surfaces a governed artifact's `Rule:` identity (`id`, `version`, `author`, `effectiveFrom`, `effectiveUntil`); `null` for ungoverned artifacts (DEVL-111).
- Rule governance metadata: a `Rule:` manifest section declaring `id` (dotted identifier), `version` (semver), optional `author`, and optional `effective from` / `effective until` dates. The compiler validates each field and embeds them in the bytecode manifest as `manifest.rule`; invalid ids, versions, or dates are line-numbered compile errors, and `effective until` before `effective from` is rejected. Programs without a `Rule:` section compile unchanged (ungoverned mode). First workstream of compliance-grade auditability (DEVL-111, epic DEVL-110).
- `devlish run --quiet` suppresses VM debug events on stderr, leaving stdout as the program result and stderr for real errors only (DEVL-95).
- `devlish.toml` tool manifests now support per-parameter subsections (`[tools.parameters.NAME]` / `[tools.inputs.NAME]`) with `type` and `description` keys, alongside the existing inline-table form (DEVL-93).
- VM validates bytecode control flow before execution: every `JUMP`/`JUMP_IF_FALSE` target and `TRY_BEGIN` handler must be numeric and within bounds, so malformed or untrusted bytecode fails at load with a clear error instead of exiting as an apparent success (DEVL-106).
- devlish-runtime translates WASM traps into structured `{ success: false, trapped: true }` results and automatically replaces the WASM instance before the next run, on both the main thread and the worker path (DEVL-102).
- devlish-runtime artifact contract: `loadTool` validates the bytecode format marker and `format_version` (exported `SUPPORTED_FORMAT_VERSIONS`, the migration gate) and structural shape before instantiating the sandbox, throwing `ArtifactError` at load instead of failing mid-run. An optional `expectedSha256` option verifies the artifact bytes via WebCrypto for tamper/corruption detection, and `tool.info` surfaces artifact metadata (format version, compiler version, source hash, declared permissions) to consumers (DEVL-99).
- 8 filesystem operation keywords: `Copy file`, `Move file`, `Create directory`, `Delete file`, `Check if exists`, `Get file info`, `List files`, `Find files matching`. Each compiles to a dedicated opcode and dispatches through `HostEffects` trait methods. NativeHost implements all operations using `std::fs` and the `glob` crate (DEVL-71).
- Credential and environment management: `CredentialStore` with `.env` file loading (program-local and `~/.devlish/.env`), CLI `--env KEY=VALUE` override, and resolution chain. Credentials flow through `HostEffects.resolve_credential()` to host methods only, never to program variables (DEVL-70).
- Program manifest: `Permissions:` / `Boundaries:` / `Callers:` header block for declaring required host effects. Compiles into `manifest` metadata in `.dvlc.json`. VM enforces permissions at runtime; undeclared effects fail with "Permission denied". Programs without a manifest remain unrestricted for backward compatibility (DEVL-68).
- MCP tool discovery from `devlish.toml` manifests (DEVL-73).
- `Respond` keyword and extended `Fail` to accept records (DEVL-76).
- `Download` keyword and `Read XLSX rows` for generic file workflows.
- HTTP verb keywords rewritten from Rust, GeocivicHost adapter removed (DEVL-77, DEVL-78).
- Native Rust compiler (`devlish-core`, ~5,800 lines) covering the full Devlish language: 33+ statement types, 16 expression types, class-style programs with methods and inheritance, 69 tests.
- Shared `devlish-vm` crate (~1,450 lines): platform-independent bytecode VM with `HostEffects` trait, 52 opcodes, 26 built-in functions (count, first, last, unique, flatten, min, max, sum, average, reverse, sort, uppercase, lowercase, trim, length, round, abs, replace, split, join, item, slice, keys, values, entries, type_of).
- WASM runner (110 lines) for browser and Node embedding via shared `devlish-vm` crate.
- Full Rust CLI: compile, run (auto-compiles .dvl), disassemble, validate, lint, new, mcp, version, help. Implicit file arguments supported.
- `devlish-core run --method` flag for class-style method dispatch.
- `devlish-core run --test` flag for assertion-based test runs: exits non-zero if any Expect assertion failed, prints pass/fail summary (DEVL-14).
- `devlish-core lint <file> --json` for structured JSON diagnostics with line, severity, message, and source_text per diagnostic (DEVL-23).
- `devlish-core mcp` stdio MCP server with 4 tools: compile, run, validate, lint. Accepts Devlish source as strings, returns structured results over JSON-RPC (DEVL-29).
- `Checkpoint` statement: pauses execution and returns structured context (prompt, variables, results) for an LLM caller. Supports custom context keys via `Checkpoint "prompt" saving context as key`. Enables resumable workflows where an LLM must intervene (DEVL-9).
- `devlish-toolrun` crate for compressing command output into model-visible JSON reports.
- PDF, DOCX, and XLSX benchmark fixtures with deterministic token-savings reports.
- XLSX recalc verification loop example: inspects a workbook, verifies formula health, checkpoints for LLM repair on errors, loops until clean (DEVL-4).
- 25 end-to-end tests that compile AND execute .dvl programs through the VM, covering variables, arithmetic, control flow, loops, assertions, error handling, builtins, records, lists, imports, checkpoints, and all statement types.
- `SERVICE_CALL` opcode with `HostEffects::call_service()` for outbound service calls (DEVL-30).
- `LOAD_FILE` opcode with `HostEffects::read_file()` for loading files into context (DEVL-31).
- `EXTRACT` opcode for pulling named fields from context variables (DEVL-32).
- `ROUTE` opcode for routing data between named destinations (DEVL-33).
- Compile-time `Import` resolution with search path support: absolute paths, relative to current file, `DEVLISH_PATH` env var, `~/.devlish/lib/`. Circular import detection prevents infinite loops (DEVL-34).
- `REQUIRE_DOC` opcode for validating that named context keys exist (DEVL-35).
- `CHECKPOINT` opcode for LLM-resumable workflow pauses with structured context capture.
- VM `source_map` parsing for error messages that include the original Devlish source line.

### Changed
- Devlish is now a single Rust binary with zero Ruby dependency. `bin/devlish` is a bash shim that execs `devlish-core`.
- Bytecode is the only compile target. The `run` command auto-compiles `.dvl` files in memory.
- All 6 previously-NOP statement types (ServiceCall, Load, Extract, Route, Import, DocumentRequirement) now compile to real opcodes. Zero NOP statements remain in the compiler.
- NOP bytecode instructions now fail at runtime with an error message including the source line, instead of silently doing nothing.

### Fixed
- `devlish validate` (and `compile`/`run`) now reject malformed assignments like `x equals equals 5` with a line-numbered compile error instead of silently compiling the reserved word into a variable name (DEVL-94).
- MCP `tools/list` no longer reports a parameter's description as the tool description when the manifest uses parameter subsections; parameters are now discovered correctly (DEVL-93).
- `devlish mcp` help text no longer claims the MCP server is unimplemented (DEVL-92).
- A Rust panic inside the WASM runner no longer poisons the runtime permanently; locks recover and later runs proceed (DEVL-102).
- `npm test` in devlish-runtime works on Node 24 (test script pointed at a directory the runner could not resolve).
- `source_hash` now covers every source file that produced an artifact, not just the top-level file. A program with `Import`s hashes an ordered manifest of all inlined files (recorded in a new `source_files` field), so editing an imported rule changes `source_hash` instead of leaving it stale. Single-file programs are unchanged: no `source_files`, and `source_hash` is still the sha256 of the one file (DEVL-121).
- `"x is greater than 3"` was parsed as `x == "greater than 3"` because bare `" is "` matched the equals operator before compound forms like `" is greater than "`. Compound `" is X"` comparison forms now take precedence (DEVL-36).
- `"uppercase of x"` returned empty string because `"of x"` was passed as the argument instead of `"x"`. Added `"uppercase of "`, `"lowercase of "`, `"trim of "`, `"round of "` patterns before bare forms (DEVL-38).

### Removed
- All Ruby code (~24,000 lines): parsers, compilers, emitters, interpreters, IR, DSL, services, validators, translators, gateway, packager, testing framework, CLI, REPL, MCP server, and all associated specs.
- Ruby/JavaScript code generation targets (`--target ruby`, `--target javascript`).
- Rails engine/UI, routes, controllers, views, migration, generator, and rake task integration.

### Fixed
- String concatenation no longer breaks on operator substrings inside quoted literals: `split_binary_expression` now scans with a quote-aware splitter, so `name plus " / " plus tier` (and any literal containing `" / "`, `" - "`, etc.) concatenates instead of being split at the character and parsed as arithmetic ("Invalid numeric result") (DEVL-199).
- `is present` is now recognized in expressions and conditions (compiles to `NOT(is missing)`), mirroring the present/missing pairing already used by validations and assertions. Previously `if X is present` had no parser case and degraded to an equality comparison against an undefined `present` identifier, so it behaved like `is missing` (DEVL-199).

## [0.1.0] - 2024-12-01

### Added
- Initial release of Devlish DSL
- Core DSL engine with English-like syntax
- CLI interface with REPL mode
- Commands: `run`, `parse`, `translate`, `validate`
- Claude API integration for English-to-Devlish translation
- Grammar validation and security checks
- Deterministic execution engine
- Support for .dvl (natural English) and .devlish (DSL) files
- Class-based and English-based parsers
- Example scripts for contracts, accounting, HR, and retirement calculations
- Comprehensive documentation
- Reserved words system for domain-specific terminology

### Core Features
- **DSL Operations**: Load, check, extract, validate, calculate
- **Type System**: Support for currency, dates, integers, strings, booleans
- **Pattern Library**: Common regex patterns for validation
- **Security**: Sandboxed execution with method whitelisting
- **Extensibility**: Modular architecture for custom operations

### Documentation
- README.md with quick start guide
- QUICKSTART.md for 5-minute tutorial
- DOCUMENTATION.md with full language specification
- TESTING.md with testing guide
- PROJECT_SUMMARY.md with architecture overview
- STATUS.md tracking project completion

[Unreleased]: https://github.com/devlish/devlish/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/devlish/devlish/releases/tag/v0.1.0
