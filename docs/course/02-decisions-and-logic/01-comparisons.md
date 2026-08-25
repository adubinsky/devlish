# Lesson 2.1: Comparisons

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Introduce the questions a program can ask about values.

## Learning Goals

- compare whether two values are equal
- compare whether one number is greater than another
- understand that comparisons produce true or false

## Vocabulary

- compare
- equal
- greater than
- less than
- boolean

## First Example

```text
invoice_amount equals 1200
review_limit equals 1000
needs_review equals false
needs_review equals true if invoice_amount >= review_limit
```

Open the example file here:
- [01_comparisons.dvl](/Users/admin/code/devlish/docs/course/02-decisions-and-logic/examples/01_comparisons.dvl#L1)

## Big Idea

A comparison asks a question about two values.

In this lesson, the question is:

"Is `invoice_amount` greater than or equal to `review_limit`?"

## Run It

```bash
./bin/devlish run docs/course/02-decisions-and-logic/examples/01_comparisons.dvl --debug
```

## Why This Matters

Programs make decisions by comparing values.

The result of the comparison is usually:
- true
- false

## Practice

1. Change `invoice_amount` to `800`.
2. Predict the value of `needs_review`.
3. Run the file and check whether you were right.
