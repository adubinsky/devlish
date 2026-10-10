# Devlish Standard Library (Current)

Last updated: 2026-10-09
Status: Current implementation inventory.

The Rust parser/compiler in `crates/devlish_core/src/lib.rs` and shared VM in
`crates/devlish_vm/src/lib.rs` define current behavior. Ruby parser and DSL
files are historical compatibility code, not the source of truth for current
native or WASM execution.

## Language layers

Control flow (`If`, `Otherwise`, `For each`, `While`, `Until`, `Try`, `Break`,
`Continue`), assignment (`equals`, `Set`), definitions (`is`), and manifests
are surface grammar. Domain nouns and service names are not library functions.

Pure expression helpers compile into deterministic VM operations. File, HTTP,
model, clock, random, and tool operations dispatch through a host and can be
permission checked, governed by policy, and journaled. Host support differs:
the browser sandbox cannot execute native tools or access arbitrary files.

## Current operations

| Family | Implemented surface forms |
| --- | --- |
| Input and output | `Ask`, `Read input`, `Read stdin`, `Print`, `Show`, `Respond with`, `Fail with` |
| Files | JSON/CSV/PDF/DOCX reads, XLSX cells, `Write`, `Overwrite`, `Export`, CSV export, `Append ... to file` |
| Filesystem | Copy, move, create directory, delete, exists, metadata, listing, globbing |
| Collections | count, first, last, unique, flatten, reverse, sort, map, filter, reject, find, reduce, any/all, group, index, partition, take, drop, zip, chunk, union, intersection, difference |
| Aggregation | sum, average, minimum, maximum |
| Records | `record with`, nested field reads and `Set` writes, keys, values, entries, field/shape checks, type inspection |
| Text | uppercase, lowercase, trim, normalize whitespace, slugify, title/sentence case, words, contains/starts/ends, length, replace, split, join, item, slice |
| Patterns | matches the pattern, first/all matches, replace matches, split by pattern, ignoring case |
| Numbers | word/symbol arithmetic, modulo, integer division, powers, squared/cubed, decimals, fractions, conversion, exact rounding |
| Dates | add days, days between, business days between |
| Effects | HTTP Get/Post/Put/Delete, `Ask the model`, current time, random number, `Run catalog tool` |
| Checks | must/should validations, `Require`, `Verify`, `Expect` |
| Composition | `Import`, `Use`, qualified module references, class methods, helper callbacks, `Checkpoint` |

Collection callback expressions compile to inline loops; methods are inlined
at compile time, and recursion is rejected. Callback bindings share the flat
variable namespace. See `LANGUAGE_REFERENCE.md` for exact syntax and semantics.

## Bundled modules

`Use the math module.` and selective imports such as
`Use pi and tau from the math module.` expose bundled `.dvl` module sources.
Qualified reads use possessives such as `math's pi`. The bundled sources
currently include math constants, a math_helpers placeholder, and a json module
version placeholder; there is no bundled statistics module.
The bundled sources live in `stdlib/`; module names and the stdlib version are
included in compiled artifact metadata and sources join the source-hash closure.

## Execution and policy

The application supports an interactive prompt, direct source execution, and
an HTTP server. These are host entry points, not additional parser modes.
Local tool execution uses declared tool capabilities and operator-captured
working-directory/PATH roots. Policy decisions support allow, deny, and abstain;
operator-selected defaults resolve only explicit abstentions. Signed production
admission, containment, durable launch reservations, terminal capture evidence,
and output disclosure checks remain distinct from ordinary local execution.

See `APPLICATION.md`, `../examples/local_tools/README.md`, and
`LANGUAGE_REFERENCE.md` for configuration, runnable examples, and profile limits.
