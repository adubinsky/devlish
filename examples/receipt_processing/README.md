# Receipt Processing Workflow

Integration story (DEVL-74) that validates the full Devlish tool surface.

## What it does

Scans an inbox directory for receipt/invoice/statement files, classifies them
by filename patterns, organizes them into dated subdirectories, and returns a
structured JSON summary.

## Features exercised

- Filesystem keywords: `Find files matching`, `Get file info`, `Create directory`, `Copy file`
- Program manifest: `Permissions:` header with declared file and HTTP access
- Structured output: `Respond with` returns JSON summary
- Error handling: `Try`/`Otherwise` for file operation failures
- Collections: list building, `For each`, `count of`

## Running

```bash
# Create test fixture files
mkdir -p /tmp/devlish-receipt-test/inbox
echo "test" > /tmp/devlish-receipt-test/inbox/receipt-coffee-shop.pdf
echo "test" > /tmp/devlish-receipt-test/inbox/invoice-consulting.pdf
echo "test" > /tmp/devlish-receipt-test/inbox/stmt-bank-july.pdf
echo "test" > /tmp/devlish-receipt-test/inbox/photo-sunset.jpg

# Run the workflow
bin/devlish run examples/receipt_processing/receipt_processing.dvl \
  --input '{"inbox_dir": "/tmp/devlish-receipt-test/inbox", "output_dir": "/tmp/devlish-receipt-test/output"}'

# Check organized output
find /tmp/devlish-receipt-test/output -type f
```
