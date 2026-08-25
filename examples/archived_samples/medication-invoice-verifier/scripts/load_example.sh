#!/usr/bin/env bash
set -euo pipefail

# This script documents the command sequence for loading the example.
# It is not executed by this task.

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

cd "$ROOT_DIR"

echo "1) Review definitions"
cat app/devlish/definitions/medication_invoice_terms.dvl

echo "2) Review parser-compatible process"
cat app/devlish/processes/verify_medication_invoice_compatible.dvl

echo "3) (Optional) Parse with devlish CLI"
echo "   ./bin/devlish parse app/devlish/processes/verify_medication_invoice_compatible.dvl"

echo "4) Review full PRD process (target language)"
cat app/devlish/processes/verify_medication_invoice_full_prd.dvl

echo "5) (Optional) Parse full PRD process to surface unsupported constructs"
echo "   ./bin/devlish parse app/devlish/processes/verify_medication_invoice_full_prd.dvl"
