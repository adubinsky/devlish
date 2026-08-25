# Lesson 1.2: Names And Memory

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Teach that a program can remember a value by giving it a name.

## Learning Goals

- explain what a named value is
- describe why a program stores information for later use

## Vocabulary

- name
- variable
- remember
- reuse

## First Example

```text
invoice_amount equals 1200
review_limit equals 1000
manual_review equals false
manual_review equals true if invoice_amount >= review_limit
```

Open the example file here:
- [02_names_and_memory.dvl](/Users/admin/code/devlish/docs/course/01-values-and-names/examples/02_names_and_memory.dvl#L1)

## Big Idea

A name lets the program remember a value so it can use it later.

That is why names matter. They turn stored information into something the
program can refer to again.

## Run It

```bash
./bin/devlish run docs/course/01-values-and-names/examples/02_names_and_memory.dvl --debug
```

## What Happens

The program remembers:
- the invoice amount
- the review limit
- the current answer for `manual_review`

Then it uses two earlier names to decide whether to change the answer.

## Practice

1. If `invoice_amount` were `900`, what would `manual_review` become?
2. If `review_limit` were `2000`, what would happen?
3. Change one value and rerun the file.
