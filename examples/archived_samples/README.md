# Devlish Examples

Last updated: 2026-03-23
Status: Archived sample corpus used by the course plan.

This directory contains the current sample corpus that the beginner course
maps onto.

Primary teaching docs now live in:
- `docs/BEGINNER_COURSE.md`
- `docs/DEVLISH_LANGUAGE_GAPS.md`

This archived sample corpus is still useful because it contains runnable files
for:
- warmups
- workflow lessons
- class-style lessons
- domain workflows
- specialized rule packs
- system examples

## Current Lesson Packs

- `simple/` - Module 0 warmup and syntax drills
- `tutorial/` - Module 1 workflow fundamentals
- `class_style/` - Module 2 class-style lessons
- `business/` - Module 3 domain workflow lessons
- `accounting/`, `hr/`, `retirement/` - Module 4 specialized rule packs
- `medication-invoice-verifier/` - Module 5 system example
- `legacy/` - archived pre-course material only

## Quick Start

```bash
./bin/devlish run examples/archived_samples/tutorial/01_load_and_check.dvl --debug
./bin/devlish test examples/archived_samples/tutorial/tests/tutorial_curriculum.dvt
./bin/devlish run examples/archived_samples/class_style/01_payroll_calculator.dvl --method calculate_wages --args '[40,25]'
./bin/devlish trace examples/archived_samples/class_style/04_helper_invocation.dvl --method review_invoice --args '[12000]'
```
