# Native Compilation Plan

Last updated: 2026-06-25
Status: Complete. Devlish is 100% Rust. Zero Ruby remains.

## Position

Devlish should compile from Devlish source and AST, not by translating the
program into another user-visible programming language first.

The long-term path is:

```text
.dvl source
  -> parser
  -> canonical AST
  -> AST lint and semantic passes
  -> typed Devlish execution graph
  -> Devlish bytecode or backend-specific machine representation
  -> packaged binary artifact
```

Current implementation:

```text
.dvl source
  -> Rust parser (full language coverage, 32 statement types)
  -> Rust bytecode emitter (42 opcodes)
  -> devlish-bytecode JSON package
  -> Rust/WASM runner or native Rust runner (shared devlish_vm crate)
```

The Rust compiler in `crates/devlish_core` covers the full Devlish language.
Compilable features emit real bytecode (loops, conditionals, arithmetic,
lists, records, built-in functions, field access, string operations, fail/
require, validate). Features needing runtime host effects (load document,
extract, service calls, route, import) parse successfully but emit NOP
with a diagnostic note; they need host-import extensions in the VMs.

Ruby and JavaScript emitters have been removed. The Rust compiler is the
only compiler. The CLI, VM, and WASM runner are all Rust.

## Meaning Of "No Intermediate Language"

Avoiding an intermediate language means Devlish should not depend on generated
Ruby, JavaScript, Go, Swift, Python, C, or another source language as the normal
compiler path.

Internal compiler representations are still acceptable and probably necessary:
- AST, for source structure and diagnostics
- typed high-level IR, for normalized semantics
- control-flow graph, for analysis
- bytecode, for portable execution
- SSA or backend IR, for native code generation

Those are compiler data structures, not alternate authoring targets.

## Compiler Goals

Primary goals:
- preserve Devlish source as the human-reviewable artifact
- lint and validate meaning from the AST
- produce deterministic execution plans
- package workflows so an LLM can call them repeatedly without re-planning
- support a future binary format that does not require a Ruby runtime
- make effects explicit: file IO, shell/tool calls, network, services, model calls

Non-goals:
- treating generated Ruby as the source of truth
- requiring users to review generated host-language code
- promising full native compilation before the language core is stable
- hiding behavior in opaque helper scripts

## AST Lint Options

AST linting should become a pass pipeline. Each pass reads a typed tree and
emits diagnostics with source spans, severity, fix hints, and optional machine
codes.

### 1. Schema Lint

Validates tree shape:
- every node has a known type
- required attributes are present
- child nodes are allowed for that parent
- source spans exist
- literal values match their declared literal type

This is the first line of defense for compiled artifacts and serialized AST.

### 2. Name And Scope Lint

Validates symbol use:
- no use before definition
- no accidental shadowing unless explicitly allowed
- loop variables stay inside loop scope
- imported symbols resolve deterministically
- class and method names do not collide unexpectedly

This should replace any remaining reliance on generated code for name behavior.

### 3. Type And Shape Lint

Validates values:
- numeric operators receive numeric-compatible values
- record field access targets records
- list operations target lists
- missing values are handled where needed
- `Export` can serialize the selected value shape

Early Devlish can keep simple inferred types. The important part is that type
facts live in the AST or semantic model.

### 4. Control-Flow Lint

Validates flow:
- `Break` and `Continue` only appear in loops
- unreachable statements are flagged
- loops have bounded or explainable execution
- `Try` recovery blocks are reachable
- `Fail` and `Require` produce understandable failure paths

This pass later becomes the source for control-flow graph generation.

### 5. Dataflow Lint

Validates value movement:
- every output can be traced to inputs or constants
- every file write/export has a value and path
- every branch condition can be evaluated from available values
- possible missing values are surfaced before runtime
- repeated tool or service calls can be identified for caching or batching

This is critical for turning LLM workflows into repeatable tools.

### 6. Effect And Capability Lint

Validates side effects:
- file reads and writes are explicit
- shell/tool calls declare commands, args, and outputs
- service calls declare service, action, inputs, and expected result
- model calls declare prompt/context boundaries
- package manifests list required capabilities

This is where policy belongs. The AST should say what the program may do before
the runtime asks the user or host for permission.

### 7. Portability Lint

Validates backend eligibility:
- can this program run in the interpreter
- can this program compile to Devlish bytecode
- can this program compile to WASM
- can this program compile to native code
- which nodes or effects block a backend

This gives clear diagnostics such as: "This workflow cannot compile to WASM
because it uses an undeclared filesystem write."

### 8. Package Lint

Validates distribution:
- imports are versioned
- capabilities are declared
- package names are stable
- bytecode or AST format version is known
- checksums/signatures match packaged content

This becomes mandatory once Devlish artifacts can be shared.

## Compile Options From AST

There are several realistic compile targets. They can coexist, but they should
share the same AST lint passes.

### Option A: Serialized AST Artifact

Compile source to a checked binary AST package.

```text
.dvl -> AST -> lint -> .dvla
```

The runtime loads the binary AST and evaluates it directly.

Strengths:
- fastest implementation
- preserves exact source-level meaning
- excellent for tooling, trace, and review
- no host-language source generation

Weaknesses:
- not a real performance win
- still needs an AST interpreter
- harder to guarantee stable runtime behavior unless the AST schema is versioned

Best use:
- first binary artifact format
- cached LLM-authored workflows
- signed reviewable packages

### Option B: Devlish Bytecode And VM

Lower AST to a compact instruction set and run it in a Devlish VM.

```text
.dvl -> AST -> lint -> typed graph -> bytecode -> .dvlc
```

Strengths:
- no generated host language
- portable across operating systems
- easier to sandbox than arbitrary native code
- stable target for package signing
- good fit for deterministic workflows and tool orchestration

Weaknesses:
- requires designing bytecode, verifier, and VM
- performance depends on VM quality
- effect boundaries must be encoded carefully

Best use:
- primary near-term "compiled" format
- repeatable LLM tool workflows
- server-side and desktop execution

Recommended first bytecode shape:
- register-based instruction stream
- constant pool
- symbol table
- source map
- effect table
- capability table
- import table
- package manifest

### Option C: WASM Binary

Compile the pure or mostly pure subset to WebAssembly, with effects imported
through a host ABI.

```text
.dvl -> AST -> lint -> typed graph -> WASM module
```

Strengths:
- real binary format
- portable and sandboxable
- strong host boundary for effects
- useful for browser, edge, and server runtimes

Weaknesses:
- records, strings, and dynamic values need a runtime ABI
- service calls must be imports
- not all Devlish effects map naturally without host glue

Best use:
- deterministic pure workflows
- sandboxed hosted execution
- eventual plugin mode

### Option D: Native Object Code Via A Backend Library

Compile the typed execution graph to native object code through a backend such
as Cranelift or LLVM.

```text
.dvl -> AST -> lint -> typed graph -> backend IR -> object file -> executable
```

Strengths:
- true native binaries
- strong performance for pure computation
- no user-visible intermediate source language

Weaknesses:
- largest implementation lift
- requires data layout, ABI, runtime, linker, and platform work
- effectful workflows still need runtime calls
- backend IR is still an internal compiler representation

Best use:
- later-stage pure computational subset
- high-throughput deterministic transformations
- signed standalone tools

### Option E: Hybrid Binary Package

Package Devlish bytecode plus a small native runner.

```text
.dvl -> AST -> lint -> bytecode -> runner + bytecode + manifest -> executable
```

Strengths:
- gives users one executable file
- avoids host-language source generation
- lets the VM evolve behind a package version
- practical bridge before true native codegen

Weaknesses:
- the binary includes a runtime
- not as small or fast as native object code

Best use:
- first "standalone executable" story
- internal tools
- workflows with effects

## Recommended Roadmap

### Phase 1: Canonical AST And Lint Pipeline

Deliver:
- versioned AST schema
- source spans on every node
- AST schema lint
- name/scope lint
- type/shape lint
- effect/capability lint
- portability lint
- `devlish lint --format json`

Exit criteria:
- every diagnostic can point to Devlish source
- no lint pass depends on generated Ruby or JavaScript
- unsupported binary targets explain exactly which AST nodes block compilation

### Phase 2: Devlish Bytecode

Deliver:
- bytecode instruction set
- bytecode verifier
- binary `.dvlc` format
- bytecode disassembler
- source map
- effect table
- bytecode interpreter

Exit criteria:
- `devlish compile workflow.dvl --target bytecode --output workflow.dvlc`
- `devlish run workflow.dvlc`
- trace can map bytecode instructions back to Devlish lines

### Phase 3: Packaged Runner

Deliver:
- executable package format
- embedded bytecode and manifest
- signed package option
- capability prompt metadata
- reproducible package builds

Exit criteria:
- `devlish package workflow.dvl --target native-runner`
- generated artifact can run without reading the source file
- package still includes source hash and source map for review

### Phase 4: WASM Backend

Deliver:
- pure subset lowering
- runtime ABI for records, strings, lists, and results
- imported host functions for effects
- WASM package manifest

Exit criteria:
- pure workflows compile to `.wasm`
- effectful workflows fail portability lint unless their imports are declared

For the first demonstrable browser and Node integration path, use a
bytecode-in-WASM VM before direct Devlish-to-WASM code generation. See
`docs/BYTECODE_WASM_FIRST_DELIVERABLES.md`.

### Phase 5: Native Backend

Deliver:
- typed lower-level graph
- data layout rules
- runtime ABI
- object emission through a backend library
- linker/package integration

Exit criteria:
- a restricted subset compiles to native executable code
- effectful operations call declared runtime functions
- backend eligibility is proven by lint before compilation begins

## Binary Format Sketch

The first binary format should be boring and inspectable.

Suggested `.dvlc` sections:
- magic and format version
- compiler version
- source hash
- source map
- symbol table
- constant pool
- instruction stream
- import table
- effect/capability table
- package metadata
- optional embedded source
- optional signature

Instruction families:
- load/store variable
- load constant
- build list/record
- field read/write
- arithmetic and comparison
- branch and jump
- loop setup and iteration
- call built-in
- call declared effect
- output
- fail/require
- return/checkpoint

## Lint And Compile API Shape

CLI:

```bash
devlish lint workflow.dvl --format json
devlish compile workflow.dvl --target ast --output workflow.dvla
devlish compile workflow.dvl --target bytecode --output workflow.dvlc
devlish compile workflow.dvl --target wasm --output workflow.wasm
devlish compile workflow.dvl --target native --output workflow
devlish disassemble workflow.dvlc
```

Library:

```ruby
parse_result = Devlish.parse_with_validation(source)
ast = parse_result.ast.first
lint = Devlish.lint_ast(ast, target: :bytecode)
artifact = Devlish.compile_ast(ast, target: :bytecode) if lint.valid?
```

Long term, the compiler should not require Ruby as the implementation language.
The Ruby library can remain an authoring and compatibility surface while the
compiler core moves to a self-contained implementation.

## Open Questions

- Is the first binary artifact a serialized AST, bytecode, or both?
- Should bytecode be stack-based for simplicity or register-based for better
  disassembly and optimization?
- Which effects are allowed in bytecode v1: file IO, service calls, shell/tool
  calls, model calls?
- Should bytecode packages embed source by default or only source hashes and
  source maps?
- Is WASM a better plugin boundary than a custom VM for hosted integrations?
- Which compiler implementation language should own the long-term compiler
  core?
