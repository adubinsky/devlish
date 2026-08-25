# Devlish for VS Code

Language support for [Devlish](https://github.com/adubinsky/devlish), the
English-first programming language.

## Features

### Syntax Highlighting

Full TextMate grammar covering 70+ keywords, operators, builtins, strings,
numbers, comments, and class/method definitions.

### Autocomplete

Type any keyword to get IntelliSense suggestions with descriptions and
snippet-style insertions for block structures (If/Otherwise, Try/Otherwise,
For each loops, manifest sections).

### Hover Documentation

Hover over any keyword to see its description and syntax example. Covers
control flow, I/O, filesystem operations, HTTP verbs, validation, manifest
declarations, and all 26 builtin functions.

### Lint on Save

Real-time diagnostics from `devlish-core lint --json` on every save.
Errors appear inline with line numbers and messages.

### Run Files

Press `Cmd+Shift+R` (or `Ctrl+Shift+R`) to run the active `.dvl` file.
Output appears in the Devlish output channel with structured result parsing.

### Format

Format Document (`Shift+Alt+F`) normalizes indentation for If/Otherwise,
For each, While, Until, Try blocks, and manifest sections.

### Debug

Full Debug Adapter Protocol support:
- Line breakpoints
- Step through execution (Next, Step In, Continue)
- Variable inspection at each step
- Timeline view mapped to source lines
- Output capture

Press `F5` to start debugging the active `.dvl` file.

### MCP Setup

Run "Devlish: Setup MCP" from the command palette to configure MCP
integration for Claude Desktop, Claude Code, or Cursor.

### Snippets

18 code snippets for common patterns: If/Otherwise, For each, While,
Try/Otherwise, Class, Method, Require, Expect, Load, Import, and more.

## Requirements

Install the `devlish-core` binary:

```bash
cd crates/devlish_core
cargo build --release
# Add to PATH or set devlish.cliPath in VS Code settings
```

## Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `devlish.lintOnSave` | `true` | Run linter on every save |
| `devlish.cliPath` | `""` | Path to devlish-core binary (uses PATH if empty) |

## Development

```bash
cd extensions/devlish-vscode
npm install
npm run compile
# Press F5 in VS Code to launch Extension Development Host
```
