# Lesson 3.6: Controlling Loops

Last updated: 2026-03-25
Status: Current lesson.

## Purpose

Teach beginners how a loop can skip one item or stop early.

## Learning Goals

- use `Continue` to skip the rest of one loop pass
- use `Break` to stop a loop completely
- explain why some items are processed and others are not

## Vocabulary

- loop
- continue
- break
- skip
- stop

## Example Program

File:
- [07_loop_control.dvl](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/examples/07_loop_control.dvl#L1)

```text
statuses equals list of approved, pending, rejected, and archived

For each status in statuses:
  If status is "pending"
    Continue
  If status is "rejected"
    Break
  Print status
```

## How To Run It

```bash
./bin/devlish run docs/course/03-repetition-and-collections/examples/07_loop_control.dvl
```

## What Each Line Means

`statuses equals list of approved, pending, rejected, and archived`
Creates a list of four values.

`For each status in statuses:`
Starts a loop that will look at each status one at a time.

`If status is "pending"`
Checks whether the current status is `pending`.

`Continue`
Skips the rest of this loop pass and moves to the next status.

`If status is "rejected"`
Checks whether the current status is `rejected`.

`Break`
Stops the loop completely. Nothing after that item is processed.

`Print status`
Shows the current status if it was not skipped and did not stop the loop first.

## Expected Result

The program prints:

```text
approved
```

Why only `approved` prints:
- `approved` is printed
- `pending` is skipped by `Continue`
- `rejected` stops the loop with `Break`
- `archived` is never reached

## Why This Matters

Some programs need more than a simple loop.

They may need to:
- ignore one item
- stop once they have found what they need

That is what `Continue` and `Break` let you express clearly.

## Try This

1. Change `pending` to `archived` in the first condition. What changes?
2. Remove the `Break` line and run the file again.
3. Add another `Print "done checking"` after the loop and notice when it runs.

## Check Yourself

1. What is the difference between `Continue` and `Break`?
2. Does `Continue` stop the whole loop?
3. Why does `archived` never print in this example?

## Related Lessons

- [01-what-repetition-means.md](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/01-what-repetition-means.md#L1)
- [03-transforming-collections.md](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/03-transforming-collections.md#L1)
- [05-changing-lists.md](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/05-changing-lists.md#L1)
