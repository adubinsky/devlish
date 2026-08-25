# Lesson 2.2: If And Otherwise

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Teach how a program chooses one path or another.

## Learning Goals

- read an `If` block
- explain when the `Otherwise` block runs
- predict the outcome of a small branching program

## Vocabulary

- condition
- branch
- if
- otherwise

## First Example

```text
invoice_amount equals 1200
decision equals "approved"

If invoice_amount >= 1000
  decision equals "needs review"
Otherwise
  decision equals "approved"
```

Open the example file here:
- [02_if_and_otherwise.dvl](/Users/admin/code/devlish/docs/course/02-decisions-and-logic/examples/02_if_and_otherwise.dvl#L1)

## Big Idea

An `If` block means:

"Only do these lines when the condition is true."

An `Otherwise` block means:

"Do these lines when the condition was not true."

## Run It

```bash
./bin/devlish run docs/course/02-decisions-and-logic/examples/02_if_and_otherwise.dvl --debug
```

## Line By Line

### `decision equals "approved"`

Start with a default answer.

### `If invoice_amount >= 1000`

Ask a question.

### `decision equals "needs review"`

Use this answer when the condition is true.

### `Otherwise`

Use a different path when the condition is false.

## Practice

1. Change `invoice_amount` to `900`.
2. Predict the new decision.
3. Run the file and compare the result to your prediction.
