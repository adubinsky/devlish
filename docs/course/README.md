# Devlish Course

Last updated: 2026-06-30
Status: Current beginner-first lesson tree.

This folder is the home for the new beginner Devlish course.

The goal is to teach programming from zero using Devlish as the first
language, with a structure similar to an introductory Python course but
written entirely in Devlish terms.

## Course Design

This course is designed like an introductory programming course for complete
beginners.

It should:
- teach one major idea at a time
- use Devlish as the primary language, not as a translation target
- explain every new term before building on it
- move from tiny examples to complete small programs
- treat testing and debugging as part of programming, not advanced extras

## Planned Unit Order

1. `00-getting-started/`
2. `01-values-and-names/`
3. `02-decisions-and-logic/`
4. `03-repetition-and-collections/`
5. `04-methods-and-classes/`
6. `05-real-programs/`
7. `06-testing-and-debugging/`
8. `projects/`

## Current Language Baseline

The course is now written for the Rust compiler and shared bytecode VM. The
tutorial examples use current `.dvl` source and can be run directly with
`bin/devlish run`.

The current beginner-facing baseline includes:
- direct `.dvl` execution with in-memory compilation
- bytecode output through `devlish compile`
- lists, records, nested field access, nested `Set`, and collection helpers,
  including query, grouping, set-style, and beginner transform helpers
- record field requirements and schema-like shape checks
- expanded validation phrases such as `must equal`, `must contain`,
  `must match`, `must be present`, `must be missing`, and `must be one of`
- `For each`, `While`, `Until`, `Break`, and `Continue`
- recoverable `Try` / `Otherwise`, `Fail with`, and `Require`
- `Expect` assertions for test runs
- `Import` for sharing workflow files, including project `lib/` imports when
  `devlish.toml` is present, plus duplicate import/name-collision diagnostics
- `Checkpoint` for resumable human or LLM review points
- file effects including text output, explicit overwrite and append writes,
  JSON/CSV reads, CSV export, plus PDF, DOCX, and XLSX reads
- one-line and multi-line input helpers
- text cleanup helpers and ISO date helpers for everyday workflow logic

## Teaching Rules

- start with ideas, not grammar documents
- use short runnable examples
- explain every line in plain English
- introduce one major concept at a time
- keep exercises small and cumulative
- clearly mark where the language still has gaps

## Lesson File Pattern

Each lesson file should contain:
- purpose
- learning goals
- vocabulary
- a first example
- line-by-line explanation
- one small modification exercise
- one or two short practice tasks
- a checkpoint

Each unit folder should also include or grow toward:
- `examples/`
- `exercises/`
- `checks/`

## Source Of Truth

The full course plan lives in:
- `docs/BEGINNER_COURSE.md`

The language gap reference lives in:
- `docs/DEVLISH_LANGUAGE_GAPS.md`
