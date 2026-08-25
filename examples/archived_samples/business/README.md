# Business Examples (No Service Actions)

Last updated: 2026-03-23
Status: Current domain-workflow lesson pack.

These examples are intentionally simple, business-oriented, and avoid explicit service actions.
They target the current core language surface: load, defined terms, extraction, validation, conditionals,
routing, and naming/binding.

Course position:
- Module 3 in [examples/DEVLISH_COURSE.md](/Users/admin/code/devlish/examples/DEVLISH_COURSE.md)

## Domains
- `finance/expense_report_review.dvl`
- `hr/onboarding_packet_check.dvl`
- `healthcare/claim_intake_triage.dvl`
- `retail/return_request_triage.dvl`
- `operations/incident_priority_sort.dvl`
- `legal/vendor_contract_screen.dvl`
- `testing/star_browser_verification_runbook.dvl`

## Supporting Inputs
- `data/expense_report.txt`
- `data/onboarding_packet.txt`
- `data/claim_submission.txt`
- `data/return_request.txt`
- `data/incident_report.txt`
- `data/vendor_contract.txt`
- `data/star_browser_verification_request.txt`

## Run
From project root:

```bash
./bin/devlish run examples/business/finance/expense_report_review.dvl
./bin/devlish run examples/business/hr/onboarding_packet_check.dvl
./bin/devlish run examples/business/healthcare/claim_intake_triage.dvl
./bin/devlish run examples/business/retail/return_request_triage.dvl
./bin/devlish run examples/business/operations/incident_priority_sort.dvl
./bin/devlish run examples/business/legal/vendor_contract_screen.dvl
./bin/devlish run examples/business/testing/star_browser_verification_runbook.dvl
```

## Validate all business examples

```bash
for f in examples/business/*/*.dvl; do ./bin/devlish validate "$f"; done
```
