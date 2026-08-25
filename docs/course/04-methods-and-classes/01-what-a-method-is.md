# Lesson 4.1: What A Method Is

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Teach that a method is a named piece of reusable work.

## Learning Goals

- explain what a method does
- recognize that methods help avoid repeated logic

## Vocabulary

- method
- reusable
- input
- output

## First Example

```text
Operations's Review Decider:
  decide review using invoice amount:
    review_needed equals false
    review_needed equals true if invoice amount >= 10000
    respond with review_needed
```

Open the example file here:
- [01_review_decider.dvl](/Users/admin/code/devlish/docs/course/04-methods-and-classes/examples/01_review_decider.dvl#L1)

## Big Idea

A method is a named piece of work you can run more than once.

Instead of rewriting the review rule again and again, the method gives that
logic one home and one name.

## Run It

```bash
./bin/devlish run docs/course/04-methods-and-classes/examples/01_review_decider.dvl --method decide_review --args '[12000]'
```

## Practice

1. Run the same method with `[9000]`.
2. Compare the returned value.
3. Explain what job the method performs.
