# Lesson 6.6: Recovering With Try

Last updated: 2026-03-26
Status: Current lesson.

## Purpose

Teach that not every problem has to stop the whole program.

## Learning Goals

- explain what `Try:` means
- explain what `Otherwise:` means after a `Try:`
- read a small program that recovers from a failed requirement

## Vocabulary

- recovery
- fallback
- failure
- continue safely

## Big Idea

Sometimes a program should stop.

Sometimes a program should use a safe fallback instead.

Devlish now supports that pattern with:

```text
Try:
  ...
Otherwise:
  ...
```

The `Try:` block does the main work.

If something inside it fails, the `Otherwise:` block runs as the fallback.

## Example File

- [06_try_recovery.dvl](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/examples/06_try_recovery.dvl#L1)

## Example

```text
review_status equals "pending"

Try:
  Require review_status is "approved" otherwise fail with "Review must be approved"
Otherwise:
  review_status equals "manual_review"
  Print "used fallback"

Print review_status
```

## How To Run It

```bash
./bin/devlish run docs/course/06-testing-and-debugging/examples/06_try_recovery.dvl
```

## What Happens

1. The program starts with `review_status` set to `"pending"`.
2. The `Try:` block checks whether the review is approved.
3. That requirement fails.
4. Instead of ending the whole run, the `Otherwise:` block runs.
5. The program uses `"manual_review"` as a safe fallback.

## Expected Output

```text
used fallback
manual_review
```

## Why This Matters

This is the first recovery-style construct in Devlish.

It helps beginners see that programs can do more than:
- succeed
- fail

They can also:
- try the main path
- fall back to a safer path

## Try This

1. Change `review_status` to `"approved"` and run the file again.
2. Remove the `Print "used fallback"` line and compare the output.
3. Change the fallback value from `"manual_review"` to `"needs_attention"`.

## Check Yourself

1. What causes the `Otherwise:` block to run?
2. What value does the program use after recovery?
3. When would recovery be better than stopping the program?
