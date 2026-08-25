# Medication Invoice Verifier (Devlish Working Folder)

This folder simulates a fresh, separate Devlish workspace loaded with the medication invoice verification PRD example.

## Goal
Build a deterministic rule flow that verifies whether an invoiced prescription medication is approved for a specific patient and routes results to approval, denial, or manual review.

## Folder Layout
- `app/devlish/definitions/` business terms and constants
- `app/devlish/processes/` runnable Devlish process files
- `app/devlish/services/services.yml` service registry for integrations
- `gateway/` Sinatra + Sidekiq style event ingress skeleton
- `scripts/load_example.sh` commands you would run to load/parse example
- `notes/BUILD_REPORT.md` what worked, what did not, and improvements

## Included Devlish Process Variants
- `verify_medication_invoice_compatible.dvl`:
  - constrained to parser patterns that are currently supported
  - intended as the most likely to parse today
- `verify_medication_invoice_full_prd.dvl`:
  - closer to full PRD intent (loops, richer service actions)
  - documents target language direction even where parser work is needed

## Usage (not executed in this setup)
See `scripts/load_example.sh` for the command sequence.

Note:
Files under `app/devlish/definitions/` are support assets, not standalone
examples to run directly with `devlish run`.
