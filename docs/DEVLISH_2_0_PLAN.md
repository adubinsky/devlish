# Devlish 2.0 Plan

Last updated: 2026-03-26
Status: Active architecture plan for `codex/devlish-2-0-foundation`.

## Current Branch Progress

Doc truth policy:
- this plan tracks the working-tree implementation on the active branch
- it is not a commit-history summary
- behavior reflected in parser/runtime/emitter tests and course examples takes
  precedence over stale milestone text

Implemented on this branch so far:
- explicit AST node classes for workflow-style and class-style Devlish
- English-parser AST emission for lesson-style programs
- class-style AST emission for module/class/method files
- semantic analysis with a first symbol table and simple type inference
- Devlish IR for workflow-style programs
- class/module IR for the current class-style subset
- AST/IR interpretation for workflow-style programs without `instance_eval`
- class-style IR execution for supported class methods
- structured arithmetic/logical expression nodes and guarded assignments
- deterministic Ruby and JavaScript emitters from Devlish IR
- deterministic Ruby and JavaScript emitters for the supported class-style subset
- `devlish compile` support for Ruby and JavaScript output
- `devlish trace` support for parse-result and IR inspection
- a Devlish-native `.dvt` test harness with `Scenario`, `Given`, `When`, and `Then`
- direct output with `Print` and `Show`
- `For each`, `While`, `Until`, `Break`, and `Continue`
- `list of ...` literals, including empty `list of`
- `record with ... as ...` literals with field access
- nested record updates with `Set ... to ...` for common record-building flows
- beginner built-ins for collections, text, and numbers:
  `count of`, `first of`, `last of`, `item`, `slice`, `Append`, `Pop`,
  `trim`, `uppercase`, `lowercase`, `replace`, `split ... by ...`,
  `join ... with ...`, `absolute value of`, and `round`
- richer collection logic: `sort`, `sort ... by ...`, `filter ... where ...`,
  `map`, `reject`, `reduce`, `any of ...`, and `all of ...`
- collection summary helpers: `unique of ...`, `flatten ...`,
  `minimum of ...`, and `maximum of ...`
- record helpers: `keys`, `values`, and `entries`
- list access and mutation with `item`, `slice`, `Append`, and `Pop`
- workflow-style imports with `Import ...`
- class-style imports for workflow fragments inside methods
- top-level class-style imports of other class files for compile/package flows
- explicit failure with `Fail with ...`
- guarded failure with `Require ... otherwise fail with ...`
- first-pass recovery with `Try:` / `Otherwise:`
- text comparisons with `contains`, `starts with`, and `ends with`
- field access with `amount of invoice`-style expressions
- `package` support for bundled runnable artifacts
- lesson and compiler/interpreter specs proving the tutorial path and course path

Still pending:
- deeper structured data semantics and advanced nested record ergonomics
- broader multi-file composition rules beyond first-pass import support
- recovery-style error handling beyond fail-fast requirements
- broader standard-library breadth, especially date/time and richer search/group helpers
- beginner tooling improvements such as formatter/linter/REPL-quality UX

## Purpose

Devlish 2.0 is the point where Devlish stops being "English that generates
Ruby" and becomes a real first-class programming language for vibe coding.

The product goals are:
- let a user and an LLM collaborate directly in near-English source
- validate the LLM-to-code path without requiring the user to read another
  language
- execute simple programs reliably
- make Devlish portable across runtimes, languages, and platforms

## Product Goals

### Goal 1: Close the user-LLM-code loop

The canonical artifact must be Devlish source, not generated Ruby.

That means:
- the user edits Devlish
- the LLM edits Devlish
- validation explains Devlish
- runtime traces refer to Devlish statements
- generated Ruby or other host-language targets are compatibility artifacts
- long-term Devlish files should compile to Devlish-native bytecode, packaged
  execution plans, WASM, or native binaries

### Goal 2: Validate the LLM code path in Devlish terms

The system must answer:
- what did this Devlish sentence mean
- what variables were created
- which branch will run
- which services will be called
- which outputs are expected
- where the source is ambiguous

Those answers should come from Devlish-native analysis, not inspection of
generated Ruby.

### Goal 3: Support both interpretation and compilation

Devlish should be able to:
- run directly through an interpreter
- compile to a portable Devlish execution plan
- compile to a Devlish bytecode or binary artifact
- compile eligible subsets to WASM or native code
- support standalone runnable packaging without requiring users to review a
  generated host language

## Current State

Today the implementation is partially transitioned, but much further along than
the early 2.0 scaffolding phase:
- workflow-style Devlish has a real AST -> IR -> interpreter path
- class-style Devlish has a real AST -> class IR -> interpreter/compiler path
- Ruby and JavaScript compatibility backends exist for the supported subset
- `generated_code` and the older Ruby bridge still exist in the codebase, but
  they are no longer the best description of the current workflow-style path

Current implementation references:
- [parse_result.rb](/Users/admin/code/devlish/lib/devlish/parser/parse_result.rb#L9)
- [english_parser.rb](/Users/admin/code/devlish/lib/devlish/parser/english_parser.rb#L35)
- [ir_interpreter.rb](/Users/admin/code/devlish/lib/devlish/executor/ir_interpreter.rb#L14)
- [class_ir_interpreter.rb](/Users/admin/code/devlish/lib/devlish/executor/class_ir_interpreter.rb#L14)

## Non-Goals For The First Devlish 2.0 Cut

These are explicitly not required for the first complete 2.0 milestone:
- native-code compilation of the full language
- LLVM as the primary execution strategy
- advanced optimizer passes
- full static typing
- automatic support for every existing parser phrase
- replacing host service adapters on day one

## What "Finished" Means

Devlish 2.0 is "finished enough" when all of the following are true:
- Devlish parses to a real AST as the primary artifact
- AST lowers to a Devlish IR or execution plan
- the interpreter executes AST/IR directly, without `instance_eval`
- tutorial lessons run through the AST/IR runtime
- diagnostics, traces, and validation all reference Devlish source locations
- there is a Devlish-native test harness for user-facing programs
- there is at least one supported backend beyond direct interpretation
- the beginner-core language surface is strong enough to teach useful programs
  without major conceptual gaps

## Next Delivery Milestone

The next delivery milestone should be Language Completion Phase 1.

This is no longer a "beginner core completion" milestone. The beginner core is
substantially in place on the current branch.

Why this is next:
- the AST, IR, interpreters, emitters, test harness, and course are already far
  enough along to support the core teaching path
- the biggest remaining gaps are now language depth, reuse, recovery, and UX
- the roadmap should now optimize for completeness and ergonomics, not for
  first-pass beginner coverage

This milestone should deliver:
1. deeper structured data
   - nested records
   - nested access/update ergonomics
   - stronger data-shaping semantics
2. broader reuse/imports
   - class-style reuse/import story
   - multi-file composition rules
   - clearer package/module organization
   - extension/package loading boundaries that do not blur language vs library
3. recovery-style error handling
   - fallback/default flow
   - guarded operations beyond hard fail
   - recoverable failure patterns
4. standard-library expansion
   - broader collection helpers
   - date/time helpers
   - richer numeric helpers
   - broader text normalization/search helpers
5. tooling and teaching UX
   - clearer trace output
   - formatter/linter
   - beginner REPL or interactive playground path

The goal is to make Devlish:
- useful for larger small-to-medium programs
- easier to organize across files
- more expressive with real-world data
- more forgiving and informative in failure scenarios
- easier to teach without constantly caveating rough edges

## Workstreams By Layer

This is the recommended way to organize the remaining Devlish 2.0 work.

### 1. Language Core

What belongs here:
- syntax and grammar
- control flow
- literals and expressions
- classes and methods
- imports
- error semantics

Current state:
- workflows, classes, loops, branching, lists, records, and fail-fast checks
  are already part of the implemented teaching path
- first-pass imports work in workflow-style Devlish and in key class-style reuse
  cases

Main gaps:
- deeper structured-data semantics
- stronger class and method semantics
- broader reuse/import rules across workflow-style and class-style Devlish
- richer error semantics beyond first-pass `Try` recovery and fail-fast `Fail` / `Require`

Plan:
1. deepen structured data
   - nested record access/update ergonomics
   - stronger data-shaping semantics
   - cleaner nested traversal rules
2. harden class and method semantics
   - explicit duplicate-definition policy
   - clearer override/inheritance rules
   - stronger method-call diagnostics
3. broaden reuse/import semantics
   - unify workflow-style and class-style reuse rules
   - document module/package organization for multi-file programs
4. add recovery semantics
   - fallback/default-flow constructs
   - guarded operations that can recover instead of only fail
   - language-level distinction between recoverable and unrecoverable failure

Suggested priority:
- highest

### 2. Core Runtime

What belongs here:
- execution engine
- interpreter/compiler boundaries
- document/runtime boundaries
- packaging and execution hooks

Current state:
- workflow-style AST -> IR -> interpreter is real
- class-style AST -> class IR -> interpreter/compiler is real
- compile, trace, test, and package already exist

Main gaps:
- interactive runtime boundaries are still thin
- trace/debug output is still too runtime-shaped for beginners
- larger program packaging and execution rules need more polish

Plan:
1. improve execution UX
   - clearer trace output
   - better debug presentation for loops, imports, and failures
2. formalize runtime boundaries
   - define what belongs in the runtime vs standard library vs package layer
   - clarify document/runtime boundaries for host integrations
3. strengthen packaging hooks
   - make multi-file packaged programs more predictable
   - document packaging contracts for compiled/interpreted targets

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
- Devlish already has a real first-pass standard library for text,
  collections, numeric cleanup, basic input/output, and document-driven
  workflows

Main gaps:
- richer interactive input helpers
- richer write/export helpers
- broader collection helper set
- broader numeric helper set
- date/time support
- richer text normalization and search helpers

Plan:
1. expand I/O helpers
   - multi-line input and input validation
   - append/overwrite policy and table export
2. expand collection helpers
   - `find`, `group by`, and related helpers
3. expand numeric and date/time helpers
   - clamping, averages, ranges
   - beginner-friendly date/time operations
4. expand text helpers
   - normalization, searching, cleanup, and matching breadth

Suggested priority:
- highest

### 4. Extension And Package System

What belongs here:
- installable/addable modules
- reusable library packages
- adapters and integrations
- future gems/packages/plugins

Current state:
- workflow imports exist
- runnable packaging exists
- project layouts are already possible

Main gaps:
- no formal extension model
- no explicit package boundary separate from grammar
- no dependency/manifest model in Devlish terms
- no rules for what external packages may extend safely

Plan:
1. define the boundary
   - language core stays stable
   - packages extend libraries, adapters, and runtime integrations
   - packages do not redefine core grammar semantics
2. define the packaging model
   - manifest/dependency format
   - module naming and loading rules
   - package search/install story
3. define extension capabilities
   - library modules
   - runtime adapters
   - service integrations
   - future gem/package/plugin bridge
4. add teaching and tooling story
   - how learners discover, install, and use reusable packages

Suggested priority:
- high

## Priority Summary

If the goal is a mostly complete language with a strong lesson plan, the
recommended order is:

1. language core
2. standard library
3. extension and package system
4. core runtime

This order reflects the current branch reality:
- the runtime foundation is already stronger than the language-completion gaps
- the biggest remaining usefulness gains now come from semantics, library
  breadth, and extensibility boundaries

## Language, Runtime, And Library Boundaries

Devlish should now be planned and documented as four separate layers:

### 1. Language Core

This is the language itself:
- syntax
- control flow
- literals and expressions
- classes and methods
- imports
- error semantics

Examples:
- `If`
- `For each`
- `While`
- `Until`
- `Break`
- `Continue`
- `list of ...`
- `record with ... as ...`
- class-style module/class/method syntax
- `Import ...`
- `Fail with ...`
- `Require ... otherwise fail with ...`

### 2. Core Runtime

This is how Devlish executes:
- interpreter and compiler execution hooks
- document/runtime boundaries
- packaging/execution boundaries
- host-environment integration rules

Examples:
- AST -> IR interpretation
- class IR execution
- package/run/trace/test hooks
- document loading boundaries

### 3. Standard Library

This is the reusable built-in functionality available to programs.

Examples:
- `Print`, `Show`
- `count of`, `first of`, `last of`, `item`, `slice`
- `Append`, `Pop`
- `keys`, `values`, `entries`
- `trim`, `uppercase`, `lowercase`, `replace`, `split`, `join`
- `absolute value of`, `round`
- `filter`, `sort`, `map`, `reject`, `reduce`, `any`, `all`

### 4. Extension And Package System

This is how Devlish grows without changing the grammar for every new feature.

Examples:
- installable library modules
- runtime adapters
- service integrations
- future gem/package/plugin bridges

Planning rule:
- language core defines syntax and semantics
- core runtime defines execution and host boundaries
- standard library defines built-in capabilities
- extension/package system adds reusable modules without redefining core
  language semantics
## Core Architecture

### Layer 1: Source

Author-facing files stay near-English:
- rule files
- definition files
- lesson files
- test files

Suggested file categories:
- `.dvl` for runtime programs and definitions
- `.dvt` for Devlish tests, if a separate test extension proves useful

### Layer 2: AST

The AST should become the first canonical structured representation.

The AST needs explicit node types instead of a generic bag of attributes.
Minimum nodes:
- `ProgramNode`
- `DefinitionNode`
- `LoadNode`
- `DocumentRequirementNode`
- `ExtractNode`
- `AssignmentNode`
- `OutputNode`
- `BuiltinCallNode`
- `ValidationNode`
- `IfNode`
- `ElseNode`
- `ForEachNode`
- `RouteNode`
- `ServiceCallNode`
- `BindNode`
- `TriggerNode`
- `RespondNode`
- `LiteralNode`
- `VariableRefNode`
- `BinaryExpressionNode`
- `ComparisonNode`

Every node should carry:
- source line and column
- original source text
- normalized identifiers
- optional inferred type

Current branch note:
- the AST now includes loop nodes for `ForEach`, `While`, and `Until`
- built-in call nodes now cover collection transforms such as `map`,
  `reject`, and `reduce`

### Layer 3: Semantic Model

After parsing, the compiler should run semantic analysis:
- name resolution
- term resolution
- scope analysis
- capability discovery
- type inference for simple values
- branch safety checks
- service argument validation

Outputs of semantic analysis:
- resolved variable table
- symbol table
- warnings and errors
- capability requirements
- normalized expression graph

### Layer 4: Devlish IR

AST is good for authoring and diagnostics. Execution should use a simpler IR.

Minimum IR operations:
- `load_document`
- `assert_contains`
- `extract_value`
- `assign_value`
- `output_value`
- `builtin_call`
- `validate_minimum`
- `validate_maximum`
- `validate_equals`
- `branch`
- `loop_each`
- `route_value`
- `call_service`
- `http_request` (Get/Post/Put/Delete via HostEffects.http_request())
- `bind_name`
- `respond` (Respond with: JSON to stdout, exit 0)

The IR should be:
- serializable
- deterministic
- debuggable
- backend-friendly

The IR is the real bridge to:
- interpreters
- Devlish bytecode
- packaged binary artifacts
- compatibility host-language backends
- remote execution
- future visual tooling

### Layer 5: Runtime

There should be two execution modes.

#### Interpreter mode

The interpreter walks the IR directly.

This is the fastest path to a real language runtime and should be the default
for Devlish 2.0.

#### Compiled mode

The compiler should lower Devlish IR to one of:
- a serialized checked AST artifact
- Devlish bytecode
- a packaged Devlish runner artifact
- WASM for eligible pure or declared-effect subsets
- native code for a narrower eligible subset

Generated Ruby and JavaScript remain useful compatibility targets, but they
should not define the long-term compiler architecture.

For the first cut, interpreted and compiled modes can share the same IR.

Current branch note:
- Ruby and JavaScript compiled-script generation now exists for the supported
  Devlish 2.0 subset.
- The next backend step is broadening IR coverage, not inventing a second
  intermediate representation.

## Testing Strategy

### Will tests be in Devlish

Yes, user-facing behavior tests should be writable in Devlish.

That does not replace host-language tests. It adds a Devlish-native layer on
top of them.

The stack should be:
- Ruby tests for parser internals, compiler passes, runtime internals, and
  service adapter behavior
- Devlish tests for user-visible behavior, lesson examples, and LLM-generated
  programs

### Devlish test harness

A test harness does need to be built.

The harness should support simple near-English tests such as:

```text
Test "routes approved reviews"

Given document from "examples/tutorial/data/review_packet.txt"
When I run "examples/tutorial/03_branch_and_route.dvl"
Then review_status should equal "approved"
Then the route should be "approved_queue"
Then the run should succeed
```

Or a more direct inline-program style:

```text
Scenario "extract and validate a value"

Given this document:
  review status: approved.
  contract value: $12000.

When I run:
  Find review status and save as review_status
  Find contract value and save as contract_value
  contract_value must be at least 10000

Then review_status should equal "approved"
Then contract_value should equal 12000
Then validation should pass
```

The test harness needs:
- fixture loading
- run execution
- assertion vocabulary
- snapshot or trace capture
- golden output support for lessons and examples

### Why Devlish tests matter

This is how Devlish validates the LLM-code path.

If an LLM writes Devlish, the user should be able to inspect:
- the Devlish source
- the Devlish test
- the Devlish trace

without reading Ruby.

## Compilation Strategy

The compiler strategy should avoid user-visible intermediate host languages.
The normal path should be Devlish source to AST, AST lint passes, typed Devlish
lowering, then a binary or package target.

For the detailed native and bytecode roadmap, see
`docs/NATIVE_COMPILATION_PLAN.md`.

### What is required before native code is realistic

Native compilation requires:
- a versioned AST schema
- AST lint passes for structure, names, types, control flow, dataflow, effects,
  capabilities, package boundaries, and backend eligibility
- a typed Devlish execution graph
- runtime library boundaries for documents, files, services, model calls, and
  other effects
- data layout rules for strings, records, lists, booleans, missing values, and
  numbers
- source maps from binary instructions back to Devlish lines

### Useful output modes

#### 1. Checked AST artifact

Serialize a linted AST into a binary package and run it with an AST runtime.
This is the easiest binary format and the best bridge from current parsing to
reviewable reusable artifacts.

#### 2. Devlish bytecode

Lower the AST into a compact Devlish instruction set. This should be the
primary near-term compiled format because it avoids generated host-language
source while preserving portability and source maps.

#### 3. Packaged runner

Bundle bytecode with a small native runner so users receive one executable
artifact. This gives an executable experience before true native codegen.

#### 4. WASM

Compile eligible subsets to WebAssembly, with file IO, services, shell/tool
calls, and model calls represented as declared host imports.

#### 5. Native backend

Compile a narrower eligible subset to object code through a backend library.
This is appropriate after bytecode, source maps, effects, and data layout are
stable.

## Minimum Working System

To compile simple Devlish into runnable code, the system needs more than a
parser.

Minimum required parts:
- source loader
- parser
- AST schema
- semantic analyzer
- symbol table
- IR builder
- interpreter
- service runtime boundary
- standard library runtime
- diagnostics and trace system
- package or backend emitter
- test harness
- CLI

## Standard Library Formalization Work

The standard library should be formalized separately from the language core so
the compiler can classify what is:
- syntax
- literal/data form
- pure library operation
- effectful library operation
- runtime boundary call

Minimum formal standard-library baseline for 2.0:
- output values
- text normalization
- collection operations
- numeric cleanup helpers
- record helpers

Near-term additions:
- save or export
- broader collection search/group helpers
- richer equality and presence helpers
- date/time helpers

## Language Completion Focus Areas

This section records the major language-completion tracks after the AST/IR and
compiler foundation. Much of the beginner core in these tracks is already
implemented on the current branch; the remaining value is in broadening and
hardening them.

### Track A: Output And Visible Feedback

Deliverables:
- `Print` or `Show` as a first-class statement
- AST support
- IR support
- interpreter support
- Ruby and JavaScript emitter support
- CLI output display for `devlish run`

Exit criteria:
- a beginner can write a file that visibly prints a value
- the same file works in interpreted and compiled modes

Current branch note:
- `Print` and `Show` now work in the parser, AST, IR, interpreter, CLI, and
  Ruby/JavaScript compiled output.

### Track B: Collections And Structured Data

Deliverables:
- list literals
- collection-aware literals in AST and IR
- first semantic rules for collection values
- first runtime support in interpreter and emitters

Exit criteria:
- Devlish can represent and evaluate small collections and grouped values

### Track C: Repetition And Control Flow

Deliverables:
- beginner-friendly `For each`
- loop lowering into IR
- loop execution in interpreter
- loop code generation in Ruby and JavaScript
- trace support for repeated steps

Exit criteria:
- a small list can be processed item by item in interpreted and compiled modes

Current branch note:
- `For each` now works for named collections and inline collection headers such
  as `approved and pending and rejected`
- `While` and `Until` now lower into IR, execute in the interpreter, and
  compile to Ruby and JavaScript
- first-class list literals are now in place

### Track D: Standard Library Expansion

Deliverables:
- text helpers such as `contains`, `starts with`, `ends with`, `split`,
  `join`, and `trim`
- collection helpers such as `count`, `first`, `any`, and `all`
- semantic classification of pure operations vs effectful operations
- clear separation between standard-library APIs and language syntax

Exit criteria:
- beginner exercises can rely on a compact but useful library of common
  operations
- implementers can add helpers without changing the parser grammar

Current branch note:
- the beginner library now includes `count`, `first`, `last`, `trim`,
  `split`, `join`, `contains`, `starts with`, `ends with`, `sort`, `filter`,
  `map`, `reject`, `reduce`, `any`, `all`, `keys`, `values`, and `entries`
- the next step is documenting and enforcing where language features stop and
  standard-library APIs begin

### Track E: Data Depth And Ergonomics

Deliverables:
- lightweight record or object literals
- field access rules
- serialization support in IR and traces

Exit criteria:
- Devlish can group related values together without falling back to external
  host-language structures

Current branch note:
- `record with ... as ...` literals and field access like `amount of invoice`
  now work in interpreted and compiled paths
- nested records now work for common beginner cases when outer record fields or
  list items are comma-separated

## Required Tooling

To feel like a real language, Devlish needs tooling beyond runtime execution.

Minimum tooling:
- parser diagnostics with locations
- formatter or normalizer
- linter
- trace viewer
- IR dump command
- test runner
- package command
- lesson runner

Useful CLI targets:
- `devlish parse`
- `devlish validate`
- `devlish run`
- `devlish trace`
- `devlish test`
- `devlish compile`
- `devlish package`

## Lesson And Curriculum Requirements

The tutorial lessons should become the first acceptance suite for Devlish 2.0.

That suite should prove:
- loading works
- extraction works
- validation works
- branching works
- service output works
- binding works

The existing lesson files under `examples/tutorial/` should be treated as the
seed corpus for interpreter parity tests and future compiler parity tests.

## Concrete Action Plan

### Phase 0: Freeze the core subset

Deliverables:
- formal list of supported 2.0 statements
- formal list of expression forms
- formal list of built-in nouns
- formal list of effectful operations
- tutorial corpus tagged by feature

Exit criteria:
- no ambiguity about which language subset is in scope

### Phase 1: Define AST as the source of truth

Deliverables:
- explicit AST node classes
- parser output tests for all core statements
- source locations on every node
- parser no longer relies on generated Ruby for semantic meaning

Exit criteria:
- tutorial lessons parse into AST cleanly
- AST snapshots are stable

### Phase 2: Build semantic analysis

Deliverables:
- symbol table
- scope rules
- variable resolution
- service capability resolution
- simple type inference
- Devlish-native diagnostics

Exit criteria:
- invalid references and ambiguous statements are reported from AST analysis

### Phase 3: Lower AST to Devlish IR

Deliverables:
- IR schema
- AST-to-IR lowering pass
- IR serialization
- IR debug dump tooling

Exit criteria:
- every tutorial lesson lowers to deterministic IR

### Phase 4: Build the interpreter

Deliverables:
- IR interpreter
- runtime context model
- effect handling for document IO and service calls
- execution trace output

Exit criteria:
- tutorial lessons pass through the interpreter
- no `instance_eval` required for lesson execution

### Phase 5: Build the Devlish test harness

Deliverables:
- test file format
- `devlish test`
- fixture loading
- assertions over values, routes, validations, and service calls
- golden lesson tests

Exit criteria:
- lesson suite can be expressed as Devlish tests

### Phase 6: Harden The Completed Beginner Core

Deliverables:
- broaden and stabilize the now-implemented beginner core
- deepen structured data support and nested record ergonomics
- improve imports and multi-file reuse
- expand recovery-style error handling
- expand the standard library
- formalize the language core vs runtime vs standard-library boundary
- define the extension/package model for libraries, modules, and adapters
- improve lesson and test coverage around those additions

Exit criteria:
- the current course no longer relies on major caveats for the implemented core
- deeper structured data and reuse flows feel natural in Devlish
- the next wave of lessons can teach larger useful programs without major holes
- LLM-authored programs can be validated with Devlish-native assertions and
  richer diagnostics

Current branch status:
- the beginner core listed in the earlier version of this phase is now largely
  implemented for the current Devlish 2.0 subset
- workflow-style and class-style source both support the current repetition,
  collection, output, and fail-fast requirement features
- the next highest-value gaps have shifted to deeper structured data, broader
  reuse, recovery-style error handling, and broader standard-library coverage

### Phase 7: Add and Broaden Compiler Backends

Recommended first backend:
- Ruby or JavaScript

Deliverables:
- IR-to-backend lowering
- backend runtime shims
- backend parity tests

Exit criteria:
- tutorial lessons run in interpreter mode and compiled mode with matching
  results

### Phase 8: Package runnable programs

Deliverables:
- package format
- embedded-runtime runner or host-language wrapper
- standalone execution path for simple programs

Exit criteria:
- a simple Devlish program can be built and run outside the repo

Current branch status:
- `devlish package` is implemented for Ruby and JavaScript output
- packages include the original `.dvl` source, compiled host-language output,
  a `manifest.json`, and a package-local `run` launcher
- workflow packages bundle loaded document assets under `assets/`
- class-style packages support default method invocation via packaged launcher

## Recommended First Deliverables On This Branch

These are the most practical next moves for `codex/devlish-2-0-foundation`:

1. Define the AST node set and add explicit classes.
2. Add parser tests that assert AST output for the tutorial lesson subset.
3. Introduce an IR schema for the lesson subset only.
4. Add an interpreter that can execute the tutorial subset from IR.
5. Keep the existing Ruby pipeline temporarily for unsupported features.

That last point matters: we can migrate in slices instead of requiring a full
cutover on day one.

## Acceptance Criteria For The First Devlish 2.0 Milestone

The first milestone is complete when:
- tutorial lessons parse to AST
- tutorial lessons lower to IR
- tutorial lessons run through the interpreter
- tutorial lessons have Devlish-native tests
- traces explain behavior in Devlish terms
- the old Ruby execution path is no longer required for the tutorial subset

## Open Design Decisions

The branch should settle these decisions explicitly:
- whether test files should be `.dvl` or `.dvt`
- whether definitions and runnable programs share one AST root or separate ones
- whether compiled output targets Ruby first or JavaScript first
- whether package mode bundles source, IR, or generated host code
- how much syntax normalization the formatter should perform

## Summary

Devlish 2.0 is not just "more parsing." It is a shift in source of truth.

Today:
- Devlish parses to Ruby
- Ruby execution defines behavior

Devlish 2.0:
- Devlish parses to AST
- AST lowers to IR
- IR defines behavior
- interpreters and compilers become replaceable backends

## Simple Program Shape

This is the kind of simple program Devlish 2.0 should support cleanly:

```text
Load examples/tutorial/data/review_packet.txt

Find review status and save as review_status

If review_status is "approved"
  Route invoice to approved_queue
Otherwise
  Send Email via NotificationService to review_team with template "manual_review"
```

Expected meaning:
- load a document
- extract one value
- branch on that value
- either route work or produce an output

This example is intentionally small, but it exercises the core 2.0 pipeline:
- parsing
- AST
- semantic resolution
- IR lowering
- execution
