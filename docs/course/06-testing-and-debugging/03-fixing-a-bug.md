# Lesson 6.3: Fixing A Bug

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Teach a beginner-friendly process for changing a wrong program into a correct
one.

## Learning Goals

- spot a simple bug
- use a test and trace to explain the problem
- change one line and verify the fix

## Vocabulary

- bug
- fix
- verify
- regression

## First Example

Compare these files:
- [03_buggy_review.dvl](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/examples/03_buggy_review.dvl#L1)
- [03_fixed_review.dvl](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/examples/03_fixed_review.dvl#L1)
- [03_fixed_review.dvt](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/checks/03_fixed_review.dvt#L1)

## Big Idea

A bug is a program doing the wrong thing.

The beginner-friendly repair loop is:
1. notice the wrong behavior
2. isolate the bad rule
3. change the rule
4. rerun the check

## Run The Passing Check

```bash
./bin/devlish test docs/course/06-testing-and-debugging/checks/03_fixed_review.dvt
```

## Practice

1. Compare the threshold in the buggy file to the threshold in the fixed file.
2. Explain why the buggy one is wrong.
3. Explain how the test protects the repaired version.
