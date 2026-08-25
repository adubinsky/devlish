# Simple Devlish Examples

Last updated: 2026-03-23
Status: Current warmup lesson pack.

These files are intentionally small and runnable directly as `.dvl` inputs.
They use the currently supported parser/runtime subset so you can focus on concrete behavior.

Course position:
- Module 0 in [examples/DEVLISH_COURSE.md](/Users/admin/code/devlish/examples/DEVLISH_COURSE.md)

## Files
- `01_load_and_check.dvl` - load a document and check required text
- `02_extract_and_validate.dvl` - extract values and validate numeric thresholds
- `03_conditionals.dvl` - basic `If/Otherwise` control flow
- `04_route_and_log.dvl` - route decisions using extracted context
- `05_bindings_aliases.dvl` - strict named bindings (`Alias`, `Symbol`, `Handle`)
- `06_nickname_collision.dvl` - permissive naming (`Nickname`) with collision behavior
- `comprehensive.dvl` - compact end-to-end flow using core features
- `defined_terms.dvl` - current definition style (`term is meaning`) and usage
- `natural_contract.dvl` - plain-English contract checks (parser-safe)
- `reserved_words_demo.dvl` - reserved-word flavored example within supported comparisons

## Run
From project root:

```bash
./bin/devlish run examples/simple/01_load_and_check.dvl
./bin/devlish run examples/simple/02_extract_and_validate.dvl
./bin/devlish run examples/simple/03_conditionals.dvl
./bin/devlish run examples/simple/04_route_and_log.dvl
./bin/devlish run examples/simple/05_bindings_aliases.dvl
./bin/devlish run examples/simple/06_nickname_collision.dvl
```

## Validate all

```bash
for f in examples/simple/*.dvl; do ./bin/devlish validate "$f"; done
```
