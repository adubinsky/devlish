# Lesson 6.5: Requiring A Condition

Last updated: 2026-03-25
Status: Current lesson.

## Purpose

Teach a beginner-friendly way to stop a program when an important condition is
not true.

## Learning Goals

- read a `Require ... otherwise fail with ...` line
- explain the difference between a condition and an error message
- use `Require` to protect a program from bad state

## Vocabulary

- require
- condition
- fail
- message

## Example Program

File:
- [05_require_review_status.dvl](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/examples/05_require_review_status.dvl#L1)

```text
review_status equals "pending"
Require review_status is "approved" otherwise fail with "Review must be approved"
Print "ready to continue"
```

## How To Run It

```bash
./bin/devlish run docs/course/06-testing-and-debugging/examples/05_require_review_status.dvl
```

## What Each Line Means

`review_status equals "pending"`
Stores the current review status.

`Require review_status is "approved" otherwise fail with "Review must be approved"`
Checks whether the status is `approved`.

If it is not, the program stops and shows the message `Review must be approved`.

`Print "ready to continue"`
Would only run if the requirement passed.

## Expected Result

This program fails with:

```text
Review must be approved
```

The final `Print` line does not run.

## Why This Matters

Programs often depend on something being true before they continue.

`Require` gives you a clear way to say:
- this must be true
- otherwise stop
- show this message

That is easier for beginners to read than building the same idea from a larger
`If` block every time.

## Try This

1. Change `pending` to `approved` and run the file again.
2. Change the failure message to something more specific.
3. Add a `Print "ready to continue"` line after the `Require` and notice when it appears.

## Check Yourself

1. What does `Require` check?
2. When does the failure message appear?
3. Does the program continue after a failed `Require`?

## Related Lessons

- [03-fixing-a-bug.md](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/03-fixing-a-bug.md#L1)
- [04-failing-with-a-clear-message.md](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/04-failing-with-a-clear-message.md#L1)
