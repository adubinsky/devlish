# Interactive Course and VS Code Extension Plan

Last updated: 2026-07-07

Two initiatives to make Devlish accessible to beginners and productive for
authors: an interactive browser-based course powered by WASM, and a VS Code
extension with run, lint, MCP, snippets, and step-through debugging.

## Dependency Graph

```text
DEVL-54 (WASM Compiler) ─────────────────────────────────────┐
    │                                                         │
    ├─── DEVL-55 (Course App Shell) ────┐                     │
    │        │                          │                     │
    │        ├── DEVL-56 (Code Editor)  │                     │
    │        │       │                  │                     │
    │        │       └── DEVL-58 (Convert Course Content)     │
    │        │               │                                │
    │        └───────────────┴── DEVL-57 (Exercise Checker)   │
    │                                                         │
    ├─── DEVL-60 (Run .dvl in VS Code) ──────────────────────┘
    │        │
DEVL-59 (VS Code Language Support) ──┬── DEVL-61 (Lint Diagnostics)
    │                                 ├── DEVL-62 (MCP Setup)
    │                                 ├── DEVL-63 (Snippets)
    │                                 └── DEVL-64 (Debugger)
```

## Phase 1: Foundations (parallel start)

### DEVL-54: WASM Compiler (Critical Path)

Today devlish_wasm_runner only runs pre-compiled bytecode. This adds a second
WASM crate that exposes the parser and compiler so browsers can go from .dvl
source to execution with no server.

1. Create `crates/devlish_wasm_compiler/` as a new cdylib crate depending on
   devlish-core. Use the same `[profile.release]` settings as
   devlish_wasm_runner (opt-level "s", LTO, strip).
2. Implement the same alloc/free/result pattern from
   `devlish_wasm_runner/src/lib.rs`. Export
   `devlish_compile(source_ptr, source_len) -> i32` that calls
   `compile_source_to_json`. On success store bytecode JSON in LAST_RESULT and
   return 0. On error store a diagnostics JSON object and return 1.
3. Guard Import path resolution with `cfg(target_arch = "wasm32")` since there
   is no filesystem in the browser. Course lessons do not use imports until
   chapter 5, so this is acceptable for the interactive course.
4. Build script: `cargo build --release --target wasm32-unknown-unknown -p devlish_wasm_compiler`.
5. JavaScript wrapper `compiler.mjs` following the `index.mjs` pattern: export
   `loadDevlishCompiler()` returning
   `{ compile(source) -> { success, bytecode?, diagnostics? } }`.
6. Combined `compileAndRun(source, input)` convenience wrapper chaining
   compiler and runner.
7. Integration test in Node: compile a .dvl string, assert valid bytecode JSON.
8. Measure WASM binary size. Apply `wasm-opt -Oz` if over 2MB.

Files:
- New: `crates/devlish_wasm_compiler/Cargo.toml`
- New: `crates/devlish_wasm_compiler/src/lib.rs`
- New: `crates/devlish_wasm_compiler/js/compiler.mjs`
- New: `crates/devlish_wasm_compiler/test/compile.test.mjs`
- Reference: `crates/devlish_wasm_runner/src/lib.rs`, `crates/devlish_wasm_runner/js/index.mjs`

Risk: binary size. The compiler is much larger than the VM alone. LTO plus
opt-level "s" plus strip should help. Fallback is wasm-opt -Oz.

### DEVL-59: VS Code Extension Scaffold and Language Support

Following the deckhost-vscode pattern directly.

1. Scaffold `extensions/devlish-vscode/` with package.json (activate on
   onLanguage:devlish), tsconfig.json, .vscodeignore.
2. Create `language-configuration.json`: # line comments, indent-based folding,
   quote auto-closing.
3. Write TextMate grammar `syntaxes/devlish.tmLanguage.json` with scopes for:
   - Keywords: If, Otherwise, For each, While, Until, Print, Load, Set, Import,
     Fail with, Require, Try, Expect, Checkpoint
   - Operators: equals, contains, must contain, must equal, is greater than,
     plus, minus, and, or
   - Comments (#), strings (double-quoted), numbers
   - Class-style headers (e.g. "Operations's Review Decider:")
4. Register language and grammar in package.json contributes.
5. Minimal `src/extension.ts` with activate/deactivate.
6. Add .dvl file icon.

Files:
- New: `extensions/devlish-vscode/package.json`
- New: `extensions/devlish-vscode/src/extension.ts`
- New: `extensions/devlish-vscode/syntaxes/devlish.tmLanguage.json`
- New: `extensions/devlish-vscode/language-configuration.json`
- Reference: deckhost-vscode package.json

## Phase 2: Core Features (after Phase 1)

### DEVL-60: Run .dvl in Editor

Depends on DEVL-59 (extension scaffold). Soft-depends on DEVL-54 (falls back
to native CLI).

1. Add devlish.runFile command with keybinding (Cmd+Shift+R).
2. Native CLI runner first (no WASM needed): `src/runner.ts` spawns
   `devlish-core run <file>`, parses JSON output.
3. Display output in a dedicated OutputChannel or WebView panel: Print output,
   events, errors with clickable source locations.
4. WASM runner later (when DEVL-54 lands): bundle both .wasm files,
   compile+run in a Node worker thread.
5. Handle Ask inputs via vscode.showInputBox.
6. Status bar indicator showing run state.

Files:
- New: `extensions/devlish-vscode/src/runner.ts`
- New: `extensions/devlish-vscode/src/wasm-runner.ts`
- Modified: `extensions/devlish-vscode/src/extension.ts`
- Modified: `extensions/devlish-vscode/package.json`

### DEVL-61: Lint Diagnostics on Save

Depends on DEVL-59.

1. `src/linter.ts`: register onDidSaveTextDocument for .dvl files.
2. Spawn `devlish-core lint <file> --json`, parse the existing JSON diagnostic
   format (line, message, source_text).
3. Map each diagnostic to a vscode.Diagnostic with correct line, severity,
   message.
4. Create a DiagnosticCollection, clear on fix.
5. Settings: devlish.lintOnSave (default true), devlish.cliPath.

Files:
- New: `extensions/devlish-vscode/src/linter.ts`
- Modified: `extensions/devlish-vscode/src/extension.ts`
- Modified: `extensions/devlish-vscode/package.json`

### DEVL-55: Course App Shell

Depends on DEVL-54 (WASM compiler must be ready).

1. Set up `apps/course/` as a Vite + vanilla TypeScript static SPA.
2. Build script `scripts/build-manifest.mjs` that walks docs/course/, reads all
   .md lessons and .dvl examples, outputs a JSON manifest.
3. Sidebar navigation with chapter accordion and lesson list.
4. Hash-based routing (#/00/01-what-is-a-program).
5. Markdown renderer (marked or markdown-it) for lesson content.
6. Load both WASM modules once on init, expose global compileAndRun.
7. LocalStorage for progress tracking.
8. Responsive layout (desktop and tablet).

Files:
- New: `apps/course/package.json`, `apps/course/vite.config.ts`, `apps/course/index.html`
- New: `apps/course/src/main.ts`, `apps/course/src/manifest.ts`, `apps/course/src/navigation.ts`
- New: `apps/course/scripts/build-manifest.mjs`
- Reference: all files under docs/course/

## Phase 3: Interactive Content (after Phase 2)

### DEVL-56: Code Editor Widget

Depends on DEVL-55 (app shell). Benefits from DEVL-59 (TextMate grammar can
be ported to CodeMirror highlighting rules).

1. Choose CodeMirror 6 (lighter than Monaco, better mobile support).
2. `src/editor/devlish-language.ts`: port TextMate grammar patterns from
   DEVL-59 into CodeMirror StreamLanguage highlighting.
3. `src/editor/DevlishEditor.ts`: wraps CodeMirror with Run button, Reset
   button, and output pane.
4. Output pane shows: printed output, variable assignments, errors with line
   highlighting.
5. Read-only mode for demonstrations, editable mode for exercises.
6. Responsive: stacked on mobile, side-by-side on desktop.

Files:
- New: `apps/course/src/editor/devlish-language.ts`
- New: `apps/course/src/editor/DevlishEditor.ts`
- New: `apps/course/src/editor/output-panel.ts`

### DEVL-57: Exercise Checker

Depends on DEVL-56 (editor widget).

1. Define exercise metadata format in manifest: expected_outputs,
   expected_variables, hints.
2. Parse existing .dvt test files from docs/course/*/checks/ into checker
   configs.
3. `src/checker/exercise-checker.ts`: after compileAndRun, compare actual
   outputs to expected.
4. "Check" button in editor widget, distinct from "Run".
5. Green checkmark or red X with specific feedback.
6. On pass, mark complete in LocalStorage, update navigation progress.

Files:
- New: `apps/course/src/checker/exercise-checker.ts`
- New: `apps/course/src/checker/dvt-parser.ts`
- Modified: `apps/course/src/editor/DevlishEditor.ts`
- Modified: `apps/course/scripts/build-manifest.mjs`

### DEVL-58: Convert Course Content

Depends on DEVL-55, DEVL-56, DEVL-57.

1. Extend build-manifest.mjs to extract exercise sections from lesson markdown
   (headings like "Exercise" or "Practice").
2. Bundle all .txt fixture files (e.g. 01_notice.txt) as inline strings in the
   manifest since the browser has no filesystem.
3. Override WASM host's read_file to look up bundled fixtures.
4. Audit all 40+ examples for WASM compatibility; flag any using Import or
   filesystem effects.
5. Add exercise definitions with expected outputs for each lesson.
6. Content review pass: ensure all examples compile and run cleanly in-browser.

Files:
- Modified: `apps/course/scripts/build-manifest.mjs`
- New: `apps/course/src/fixtures.ts`
- New: `apps/course/src/wasm-host.ts`

### DEVL-62: MCP Setup

Depends on DEVL-59.

1. Port mcp-setup.ts from deckhost-vscode (133 lines, directly reusable).
2. Change entry key to "devlish", command to `devlish-core mcp`.
3. Detect Claude Desktop, Claude Code, and Cursor config paths.
4. Register devlish.setupMcp command.

Files:
- New: `extensions/devlish-vscode/src/mcp-setup.ts`
- Modified: `extensions/devlish-vscode/src/extension.ts`
- Modified: `extensions/devlish-vscode/package.json`

### DEVL-63: Snippets

Depends on DEVL-59.

1. Create `snippets/devlish.json` with tab-completion patterns: if, for, while,
   try, class, method, require, fail, expect, load, import, checkpoint, print,
   record.
2. Register in package.json contributes.

Files:
- New: `extensions/devlish-vscode/snippets/devlish.json`
- Modified: `extensions/devlish-vscode/package.json`

## Phase 4: Debugger (after Phase 2)

### DEVL-64: Step-Through Debugger via DAP

Depends on DEVL-59 and DEVL-60. The VM already emits instruction_started,
instruction_finished, and variable_assigned events. Bytecode packages include
source_map arrays mapping bytecode addresses to source lines (DEVL-13).

1. `src/debug-adapter.ts`: implement DebugAdapterDescriptorFactory as an inline
   DAP adapter.
2. `src/debug-session.ts`: core DAP requests: initialize, launch,
   setBreakpoints, configurationDone, continue, next (step over), stepIn,
   pause, stackTrace, scopes, variables, disconnect.
3. Execution engine: run the VM via WASM in a Node worker thread. After each
   instruction_started event, check if the bytecode address maps to a
   breakpoint line via source_map. If so, pause and wait for a DAP command.
4. Source map resolution: translate bytecode PC to .dvl line numbers for
   breakpoint matching and stack traces.
5. Variable inspection: on pause, read accumulated variable_assigned events up
   to current PC, present as DAP "Locals" scope.
6. Register debugger in package.json contributes with launch.json
   configuration.
7. Add "Debug" code lens above first line of .dvl files.

Files:
- New: `extensions/devlish-vscode/src/debug-adapter.ts`
- New: `extensions/devlish-vscode/src/debug-session.ts`
- Modified: `extensions/devlish-vscode/src/extension.ts`
- Modified: `extensions/devlish-vscode/package.json`

## Execution Summary

```text
Phase 1 (parallel):  DEVL-54 ──────────┐    DEVL-59 ──────────┐
                                        │                      │
Phase 2:             DEVL-55 ───────────┤    DEVL-60 ──────────┤
                                        │    DEVL-61 ──────────┤
                                        │                      │
Phase 3:             DEVL-56 ───────┐   │    DEVL-62 ──────────┘
                     DEVL-57 ───────┤   │    DEVL-63 ──────────┘
                     DEVL-58 ───────┘   │
                                        │
Phase 4:                                     DEVL-64 ──────────┘
```

## Shared Artifacts

- TextMate grammar (DEVL-59) feeds CodeMirror highlighting rules (DEVL-56).
- WASM compiler (DEVL-54) is consumed by both the course app and the VS Code
  extension.
- MCP setup pattern from deckhost-vscode ports directly to DEVL-62.
- .dvt test files in docs/course/*/checks/ drive DEVL-57 exercise checker.

## Related Existing Tickets

Done: DEVL-13 (source maps), DEVL-15 (debugger protocol), DEVL-17 (WASM
prototype), DEVL-18 (WASM ABI), DEVL-20 (WASM interpreter spike), DEVL-23
(lint --json), DEVL-29 (MCP server).

Open: DEVL-25 (async pause/resume for Ask), DEVL-26 (DealStar WASM
integration), DEVL-45 (beginner tooling).
