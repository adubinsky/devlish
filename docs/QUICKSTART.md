# Devlish Quick Start Guide

Last updated: 2026-07-10
Status: Current Rust compiler and VM quickstart.

## Installation

Build the native compiler:

```bash
cd crates/devlish_core
cargo build --release
cd ../..
```

The `bin/devlish` shim runs the compiled `devlish-core` binary.

## 5-Minute Tutorial

### 1. Write A Program

Create a small Devlish file:

```bash
cat > hello.dvl << 'EOF'
# Ask reads a value from input and saves it as user_name.
Ask "What is your name?" as user name

# Print emits the saved value.
Print user_name
EOF
```

### 2. Run It

Run `.dvl` source directly. The CLI compiles it in memory before execution:

```bash
bin/devlish run hello.dvl --input '{"user_name": "World"}'
```

You should see `World` in the output.

### 3. Validate Syntax

```bash
bin/devlish validate hello.dvl
```

Validation checks whether the compiler accepts the source file.

### 4. Compile To Bytecode

```bash
bin/devlish compile hello.dvl --output hello.dvlc.json
bin/devlish disassemble hello.dvlc.json
bin/devlish run hello.dvlc.json --input '{"user_name": "World"}'
```

Bytecode is the current compile target for native and WASM execution.

### 5. Try A Document Workflow

```bash
bin/devlish run docs/course/00-getting-started/examples/01_check_notice.dvl
```

That example loads a text file and checks whether required phrases are present.
The course examples now include comments that explain each step.

### 6. Try Structured Data I/O

```bash
cat > invoices.csv << 'EOF'
name,amount
Ada,1200
Grace,800
EOF

cat > review.dvl << 'EOF'
Read CSV from "invoices.csv" as invoices
invoice_shape equals record with "text" as name and "text" as amount
For each invoice in invoices:
  Require invoice has fields name, amount
  Require invoice matches shape invoice_shape
first_amount equals amount of first of invoices
Export invoices to "review.csv" as CSV
Overwrite "review started\n" to file "review.log"
Append "review complete\n" to file "review.log"
EOF

bin/devlish run review.dvl
```

CSV reads produce a list of records. Shape checks make required table fields
explicit. CSV exports, overwrite writes, and append writes use explicit file
modes so local runs are predictable.

## Current CLI Commands

```text
devlish compile <file.dvl> [--output path.dvlc.json]
devlish run <file> [--input json] [--method name] [--test] [--env KEY=VALUE]
devlish disassemble <file.dvlc.json>
devlish validate <file.dvl>
devlish lint <file.dvl> [--json]
devlish new <project_name>
devlish mcp
devlish version
devlish help
```

Implicit file arguments also work:

```bash
bin/devlish hello.dvl --input '{"user_name": "World"}'
```

## Language Features To Try

```text
# Lists and records
invoice equals record with 1200 as amount and "pending" as status
statuses equals list of "approved", "pending", and "rejected"
Set amount of invoice to 1300
approved_statuses equals reject statuses where item is "pending"
status_count equals reduce approved_statuses starting at 0 with total and item to total plus 1

# Validation and text/date helpers
status_count must be at least 1
slug equals slugify "Invoice Review"
due_date equals add 7 days to "2026-06-30"

# Loops
For each status in statuses:
  Print status

# Assertions for test runs
amount equals amount of invoice
Expect amount equals 1200 as "amount-is-read"

# Recovery
Try:
  Require amount is greater than 2000 otherwise fail with "too small"
Otherwise:
  Print "used fallback"

# Human checkpoint for LLM-assisted workflows
Checkpoint "Review extracted fields before export"

# Filesystem operations
Create directory "/tmp/devlish-demo"
Copy file from "invoices.csv" to "/tmp/devlish-demo/invoices.csv"
Check if "/tmp/devlish-demo/invoices.csv" exists as found
List files in "/tmp/devlish-demo" as entries
Find files matching "*.csv" in "/tmp/devlish-demo" as csv_files

# Program manifest (place at top of file for permission enforcement)
# Permissions:
#   Read files
#   Write files to "/tmp/"
#   Filesystem operations on "/tmp/"
```

## Embed In The Browser

Compiled bytecode can run directly in web applications using the
`devlish-runtime` npm package:

```bash
npm install devlish-runtime
```

```javascript
import { runTool } from "devlish-runtime";

// Load bytecode compiled with `devlish compile`
const bytecode = await fetch("/rules/pricing.dvlc.json").then(r => r.json());
const result = await runTool(bytecode, { customer_tier: "enterprise" });

if (result.success) {
  console.log("Result:", result.response);
}
```

The runtime executes in a WASM sandbox with a Web Worker by default. Programs
that require HTTP or filesystem access are rejected at load time. See
`packages/devlish-runtime/README.md` for the full API.

## Next Steps

1. Work through [docs/course/README.md](/Users/admin/code/devlish/docs/course/README.md).
2. Keep [docs/LANGUAGE_REFERENCE.md](/Users/admin/code/devlish/docs/LANGUAGE_REFERENCE.md) nearby for syntax.
3. Use [docs/TESTING_REFERENCE.md](/Users/admin/code/devlish/docs/TESTING_REFERENCE.md) when adding `Expect` checks.
