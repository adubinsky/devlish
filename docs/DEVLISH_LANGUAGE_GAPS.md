# Devlish Language Gaps

Last updated: 2026-07-10
Status: Current gap reference for teaching and language planning.

## Purpose

This document lists the major ways Devlish still falls short of a relatively
complete beginner-friendly programming language.

It is meant to support:
- curriculum design
- honest teaching
- language planning
- standard-library planning

Doc truth policy:
- this document tracks the working-tree implementation on the active branch
- it is not a commit-history summary
- behavior proven by the parser, runtimes, emitters, tests, and course examples
  takes precedence over stale milestone wording

## Summary

Devlish now has most of the beginner core in place.

That includes:
- readable step-by-step workflows
- direct output with `Print` and `Show`
- first-pass interactive input with `Ask`, `Ask multiline`, and `Read input`
- first-pass file output with `Write`, `Overwrite`, `Append`, and `Export`
- JSON/CSV file reads and CSV export
- comparisons and branching
- fail-fast requirements with `Fail` and `Require`
- first-pass recovery with `Try:` / `Otherwise:`
- list and record basics
- nested record updates and schema-like record shape checks
- loop basics with `For each`, `While`, `Until`, `Break`, and `Continue`
- class-style methods and helpers
- Ruby and JavaScript compilation
- `trace`, `test`, and `package`

The remaining gaps are no longer about "can Devlish teach the basics at all."
They are about language completion, ergonomics, and broader usefulness.

## Gap Map By Layer

This is the clearest way to prioritize the remaining work.

### 1. Language Core

What belongs here:
- syntax and grammar
- control flow
- literals and expressions
- classes and methods
- imports
- error semantics

Current state:
- the beginner language core is largely present
- workflows, classes, loops, branching, lists, records, and fail-fast checks
  are all teachable now

Main remaining gaps:
- deeper structured-data semantics
- stronger class and method semantics
- a better import/reuse model across both workflow-style and class-style Devlish
- richer recovery semantics beyond fail-fast failure

Suggested priority:
- high

### 2. Core Runtime

What belongs here:
- execution engine
- interpreter/compiler boundaries
- document/runtime boundaries
- packaging and execution hooks

Current state:
- workflow-style AST -> IR -> interpreter exists
- class-style AST -> class IR -> interpreter/compiler exists
- compile, trace, test, and package are all real features

Recent progress:
- program manifest with Permissions/Boundaries/Callers header block: compiles
  to bytecode metadata, VM enforces declared permissions at runtime (DEVL-68)
- credential management via .env files and CLI --env (DEVL-70)
- 8 filesystem operation keywords with native HostEffects methods (DEVL-71)

Main remaining gaps:
- better runtime presentation for trace/debugging
- stronger packaging/runtime rules for larger composed programs

Suggested priority:
- medium

### 3. Standard Library

What belongs here:
- text helpers
- collection helpers
- numeric helpers
- I/O helpers
- document helpers

Current state:
- Devlish has a real first-pass standard library for text, collections,
  numbers, document checks, basic input, and basic output

Main remaining gaps:
- richer interactive input and write/export helpers
- broader collection helpers
- broader numeric helpers
- date/time helpers
- richer text normalization and search helpers

Suggested priority:
- high

### 4. Extension And Package System

What belongs here:
- installable/addable modules
- reusable library packages
- adapters and integrations
- future gems/packages/plugins

Current state:
- workflow imports exist
- class-style files can import workflow fragments inside methods
- class-style files can import other class-style files at the top level
- packaging exists
- project layouts are possible

Recent progress:
- program manifest provides a first-pass declaration model for permissions,
  boundaries, and callers (DEVL-68)

Main remaining gaps:
- no formal extension model
- no documented package boundary separate from language grammar
- no rule for what packages may extend safely

Suggested priority:
- high

## 1. Interactive I/O Has A First Pass

Current strength:
- document input works well
- route and service output are observable
- direct `Print` and `Show` exist for visible beginner feedback
- `Ask "prompt" as name` records prompt-style input
- `Ask multiline "prompt" as name` records multi-line prompt-style input
- `Read input as name` and `Read stdin as name` read one line
- `Read multiline input as name` and `Read multiline stdin as name` read
  multi-line text from the input context
- `Read JSON from ...` and `Read CSV from ...` load structured files through
  the host file reader
- `Write value to "path"` writes text output
- `Overwrite value to "path"` makes overwrite behavior explicit
- `Append value to file "path"` appends text output
- `Export value to "path"` writes text output and serializes records/lists as
  pretty JSON
- `Export rows to "path" as CSV` serializes records/lists as a CSV table

Missing or underdeveloped:
- no permission/policy story for filesystem writes in hosted runtimes
- structured CLI input still uses the current context hash/runtime path

Why this matters:
- beginner languages usually rely on fast console input/output cycles
- useful small programs often need both read and write flows

## 2. Structured Data Depth Is Still Limited

Current strength:
- list literals, including empty `list of`
- record literals with `record with ... as ...`
- field access
- nested record updates with `Set ... to ...` for common cases
- `keys`, `values`, `entries`, `has_fields`, and `matches_shape`
- nested records for common beginner cases
- record field requirements with `has fields`
- schema-like shape checks with simple type names

Missing or underdeveloped:
- richer user-defined type declarations are still missing
- shape checks are intentionally small and do not replace a full type system
- diagnostics for complex nested paths can still become more specific

Why this matters:
- once learners move past one-off values, they need richer grouped data
- real programs usually evolve toward deeper nested structures

## 3. Multi-File Reuse And Organization Need A Better Story

Current strength:
- workflow-style `Import ...`
- class-style method imports of workflow fragments
- top-level class-style imports of class files
- project-boundary import lookup through `devlish.toml`, project root,
  `devlish/`, and `lib/`
- duplicate import and imported-name collision diagnostics
- larger project layouts exist
- class-style files can be parsed, traced, compiled, and packaged

Missing or underdeveloped:
- `Import` still inlines source rather than providing true namespaces/modules
- no strong package/module organization story for teaching

Why this matters:
- a mostly complete language needs a clean composition model
- a real course eventually has to teach how programs grow past one file

## 4. Error Handling Is Still Mostly Fail-Fast

Current strength:
- explicit failure with `Fail with ...`
- guarded failure with `Require ... otherwise fail with ...`
- recoverable `Try:` / `Otherwise:` blocks for expected runtime failures
- recovered failures store `last_error` and emit recovery events

Missing or underdeveloped:
- no reusable error policy surface
- the recoverable/unrecoverable boundary is still policy-light
- there is not yet a named catch/rescue syntax beyond `Otherwise`

Why this matters:
- real programs often need to keep going safely, not only stop
- learners benefit from seeing both "stop here" and "recover gracefully"

## 5. Standard Library Breadth Still Needs Work

Current strength:
- list helpers: `count`, `first`, `last`, `item`, `slice`, `Append`, `Pop`
- collection query helpers: `find`, `filter`, `reject`, `any`, `all`
- collection organization helpers: `sort`, `group by`, `index by`,
  `partition`, `take`, `drop`, `zip`, `chunk`
- set-style helpers: `union`, `intersection`, `difference`
- collection summary helpers: `unique`, `flatten`, `minimum of`, `maximum of`
- record helpers: `keys`, `values`, `entries`, `has_fields`, `matches_shape`
- text helpers: `trim`, `uppercase`, `lowercase`, `replace`, `split`, `join`,
  `normalize whitespace`, `slugify`, `title case`, `sentence case`, `words`
- date helpers: ISO date parsing, adding days, day spans, business-day spans
- comparisons: `contains`, `starts with`, `ends with`
- numeric helpers: `absolute value of`, `round`

Missing or underdeveloped:
- richer predicate and transform expressions are still intentionally small
- collection diagnostics can be more specific when a field is missing
- broader numeric helpers such as clamping, averages, medians, and ranges
- timezone-aware timestamps, date formatting options, and holiday calendars
- broader Unicode text normalization and regex-style pattern matching

Why this matters:
- once the beginner core exists, usefulness is mostly a standard-library story
- a wider helper set unlocks more natural exercises and real programs

Boundary note:
- the current implementation already mixes language-shaped statement forms and
  library/runtime capabilities in a few places
- the documentation and architecture should make it explicit which features are
  syntax, which are standard-library APIs, and which are runtime boundaries

## 6. Class And Method Semantics Are Still Young

Current strength:
- class-style module/class/method syntax
- helper calls
- private helpers
- class-style tracing, lowering, compilation, and packaging

Missing or underdeveloped:
- override and inheritance policy are still immature
- duplicate-definition policy should be made more explicit in the docs
- richer method-call diagnostics are still growing
- there is no top-level function surface separate from class-style methods

Why this matters:
- reusable logic is a major programming idea
- once class-style Devlish is first-class, semantics need to be more explicit

## 7. Beginner Tooling UX Is Still Early

Current strength:
- `run`
- `trace`
- `test`
- `compile`
- `package`

Missing or underdeveloped:
- no formatter aimed at beginners
- no pedagogy-oriented linter
- no REPL
- no visual debugger
- trace is useful, but still too runtime-shaped for early learners

Why this matters:
- beginner success depends heavily on feedback quality
- good language tooling reduces confusion as much as syntax does

## 8. Extension And Package Boundaries Are Not Formalized Yet

Current strength:
- `package` exists for runnable artifacts
- workflow-style imports exist
- larger project layouts are possible

Missing or underdeveloped:
- no documented extension model for adding reusable library modules
- no explicit package/module system boundary separate from language grammar
- no clear rule for what third-party gems/packages/modules may extend
- no formal manifest-based model for dependency resolution in Devlish terms

Why this matters:
- a complete language needs a way to grow without changing the parser for every
  new capability
- packages should extend libraries and runtime adapters, not redefine core
  language semantics

## 9. Implemented And Teachable Now

These areas are implemented and can be taught directly in the current course:
- sequence
- variables and naming
- document input
- extraction
- comparisons
- conditionals
- direct output
- fail-fast requirements
- lists
- records
- field access
- `keys`, `values`, and `entries`
- list access and mutation with `item`, `slice`, `Append`, and `Pop`
- `For each`, `While`, `Until`, `Break`, and `Continue`
- text cleanup with `trim`, `uppercase`, `lowercase`, `replace`,
  `normalize whitespace`, `slugify`, `title case`, and `sentence case`
- numeric cleanup with `absolute value of` and `round`
- predicate collection transforms such as `find`, `filter`, `reject`, `any`,
  and `all`
- everyday ISO date helpers
- routing
- service-style outputs
- workflow imports with `Import ...`
- class-style methods and helpers
- `trace`, `test`, `compile`, and `package`

## 10. Implemented But Still Ergonomically Young

These areas work today, but still feel early or incomplete in day-to-day use:
- nested structured data
- workflow imports and larger code organization
- class-style semantics and method contracts
- trace output for beginners
- standard-library breadth beyond the current first pass
- extension/package organization boundaries

## Highest-Value Gaps To Close

If the goal is a mostly complete language with a strong lesson plan, the most
valuable next additions are:

1. deeper structured data and nested record manipulation
2. broader multi-file reuse across both workflow-style and class-style Devlish
3. recovery-style error handling beyond fail-fast requirements
4. broader standard-library coverage, especially collections, numeric helpers,
   and date/time
5. explicit extension/package boundaries for modules, libraries, and adapters
6. better beginner tooling and trace UX
7. interactive input and simple write/export output

## Priority View By Layer

If you want to prioritize the roadmap by the four major areas, the current
recommended order is:

1. language core
   - because imports, recovery semantics, structured-data depth, and class
     semantics still limit larger useful programs
2. standard library
   - because breadth is now the biggest day-to-day usefulness multiplier
3. extension and package system
   - because Devlish needs a growth path that does not keep expanding the
     grammar itself
4. core runtime
   - because the runtime is already much stronger than the language/library
     boundaries above it, though interactive I/O and better trace UX still
     matter

## Recommended Next Implementation Order

1. deeper nested records and structured-data ergonomics
2. broader reuse/imports and clearer multi-file organization
3. recovery-style error handling
4. standard-library expansion
5. extension/package model
6. beginner tooling and trace UX
7. interactive I/O

Why this order:
- the beginner core is largely in place already
- the biggest remaining blockers are now language depth and ergonomics
- a richer standard library matters more now than another syntax-only pass
- tooling improvements become much more valuable once the language surface is
  broad enough to teach useful programs end to end

## Teaching Guidance

When writing lessons:
- teach the programming idea honestly
- say clearly when Devlish supports it well
- say clearly when the language is still missing an important piece
- distinguish between "implemented and teachable now" and "implemented but
  still ergonomically young"
- do not pretend a workaround is the same as first-class language support

That honesty will make the course stronger.
