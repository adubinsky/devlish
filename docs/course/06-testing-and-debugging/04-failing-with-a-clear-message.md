# Lesson 6.4: Failing With A Clear Message

Last updated: 2026-03-25
Status: Current lesson.

## Purpose

Teach the beginner idea that a program can stop on purpose and explain why.

## Learning Goals

- explain what `Fail with ...` means
- stop a Devlish program with a clear message
- understand the difference between normal output and a failure

## Vocabulary

- fail
- error message
- stop
- reason

## Big Idea

Sometimes a program should not keep going.

If an important condition is wrong, the program can stop and say why.

In Devlish, one simple way to do that is:

```text
Fail with "Contact email is required"
```

## Example File

- [04_failing_with_message.dvl](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/examples/04_failing_with_message.dvl#L1)

## Example Program

```text
contact_email equals ""

If contact_email is ""
  Fail with "Contact email is required"

Print "This line will not run"
```

## How To Run It

```bash
./bin/devlish run docs/course/06-testing-and-debugging/examples/04_failing_with_message.dvl
```

## What Happens

1. The program stores an empty contact email.
2. The `If` condition is true.
3. `Fail with ...` stops the program.
4. The final `Print` line never runs.

## Why This Matters

Useful programs should explain failure clearly.

This helps the learner see that:
- not every run is supposed to succeed
- some failures are intentional and correct
- a clear message is better than a vague crash

## Try This

1. Replace the empty string with `"team@example.com"`.
2. Run the file again.
3. What changes?

## Check Yourself

1. What is the difference between `Print` and `Fail with`?
2. Does the program keep running after `Fail with`?
3. Why is a clear failure message useful?
