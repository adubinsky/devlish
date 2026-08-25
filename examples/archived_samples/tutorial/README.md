# Devlish Tutorial Lessons

Last updated: 2026-03-22
Status: Current runnable workflow curriculum.

This folder is a curriculum-style introduction to Devlish using the current
parser and runtime subset.

Course position:
- Module 1 in [examples/DEVLISH_COURSE.md](/Users/admin/code/devlish/examples/DEVLISH_COURSE.md)

The goal is not to pretend Devlish is Python. The goal is to show how Devlish
solves the kinds of small programs beginners expect to write:
- read some input
- check required facts
- extract values
- make a decision
- produce an observable output

## How To Use These Lessons

Run each lesson from the project root:

```bash
./bin/devlish run examples/tutorial/01_load_and_check.dvl --debug
./bin/devlish run examples/tutorial/02_extract_and_validate.dvl --debug
./bin/devlish run examples/tutorial/03_branch_and_route.dvl --debug
./bin/devlish run examples/tutorial/04_send_email_output.dvl --debug
./bin/devlish run examples/tutorial/05_send_message_output.dvl --debug
./bin/devlish run examples/tutorial/06_bind_a_name.dvl --debug
```

The `--debug` flag matters because Devlish currently exposes most "output" as:
- extracted values in `result.results`
- routes in `result.results[:routes]`
- service effects in `result.results[:service_actions]`
- binding metadata in `result.results[:bindings]`

## Lesson Sequence

### Lesson 1: Load input and check text

File:
- `01_load_and_check.dvl`

What it teaches:
- loading a document
- treating a document as the main program input
- checking for required phrases

### Lesson 2: Extract values and validate them

File:
- `02_extract_and_validate.dvl`

What it teaches:
- extracting values from document input
- saving values into context
- validating thresholds

### Lesson 3: Make a decision and route work

File:
- `03_branch_and_route.dvl`

What it teaches:
- branching on extracted input
- using verbose English conditions
- producing a route as program output

### Lesson 4: Produce notification output

File:
- `04_send_email_output.dvl`

What it teaches:
- extracting a destination from input
- sending a deterministic no-op email action
- treating service outboxes as observable output

### Lesson 5: Produce message output

File:
- `05_send_message_output.dvl`

What it teaches:
- extracting a queue or channel name from input
- sending a message through the built-in messaging service

### Lesson 6: Bind a new name

File:
- `06_bind_a_name.dvl`

What it teaches:
- assigning a value
- aliasing that value with a new name
- observing bindings in runtime results

## What These Lessons Show Devlish Can Do Today

- read a document from disk
- inspect document text
- extract named values
- validate simple thresholds
- branch on string comparisons
- route work
- emit notification and messaging outputs through no-op services
- bind alternate names onto existing context values

## What These Lessons Also Expose As Current Limits

- there is no built-in `print` or console output statement in the language
- there is no first-class `save document` or `write file` English surface form
- triggers parse, but do not execute as runtime scheduling behavior
- numeric control flow from extracted values is still less polished than string
  branching and threshold validation, because most English-mode extraction
  still defaults to string output unless a more explicit type step is added
- there is no simple list literal syntax for beginner-friendly loop lessons

## Data Files

- `data/contract_notice.txt`
- `data/review_packet.txt`

These are intentionally small so the examples stay easy to reason about.
