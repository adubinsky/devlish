# Lesson 3.5: Changing Lists

Last updated: 2026-03-25
Status: Current lesson.

## Purpose

Teach beginners how to look at one item in a list, take part of a list, add a
new item, and remove the last item.

## Learning Goals

- read one item from a list with `item`
- take part of a list with `slice`
- add a new value with `Append`
- remove the last value with `Pop`
- notice how a list changes over time

## Vocabulary

- item
- slice
- append
- pop
- mutation

## Big Idea

Some programs do not just read data. They change data.

When we add something to a list or remove something from a list, we are
changing the state of the program.

In Devlish, list positions are taught in a natural beginner way:
- `item 1 of statuses` means the first item
- `item 2 of statuses` means the second item
- `slice statuses from 2 to 3` means the second and third items

## Example Program

File:
- [06_list_access_and_updates.dvl](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/examples/06_list_access_and_updates.dvl#L1)

```text
statuses equals list of approved, pending, and rejected
second_status equals item 2 of statuses
middle_statuses equals slice statuses from 2 to 3
Append needs_review to statuses
Pop from statuses and save as removed_status
Print second_status
Print removed_status
```

## How To Run It

```bash
./bin/devlish run docs/course/03-repetition-and-collections/examples/06_list_access_and_updates.dvl
```

## What Each Line Means

`statuses equals list of approved, pending, and rejected`
Creates a list with three values.

`second_status equals item 2 of statuses`
Reads the second value from the list and saves it as `second_status`.

`middle_statuses equals slice statuses from 2 to 3`
Builds a smaller list from the second item through the third item.

`Append needs_review to statuses`
Adds `needs_review` to the end of the list.

`Pop from statuses and save as removed_status`
Removes the last item from the list and saves that removed value.

`Print second_status`
Shows the value we read earlier from the list.

`Print removed_status`
Shows the value that was removed from the list.

## Expected Result

The program prints:

```text
pending
needs_review
```

After the `Pop`, the list returns to:

```text
approved
pending
rejected
```

## Why This Matters

This is an important programming idea:
- programs can remember data
- programs can inspect part of that data
- programs can change that data

This is one of the first places where a program starts to feel active rather
than only descriptive.

## Try This

1. Change `item 2 of statuses` to `item 1 of statuses`. What prints now?
2. Change `slice statuses from 2 to 3` to `slice statuses from 1 to 2`.
3. Append a different final status and see what `removed_status` becomes.

## Check Yourself

1. What does `item 1 of statuses` mean?
2. Does `slice statuses from 2 to 3` return one item or two items?
3. What is the difference between `Append` and `Pop`?

## Related Lessons

- [02-collections.md](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/02-collections.md#L1)
- [03-transforming-collections.md](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/03-transforming-collections.md#L1)
- [04-record-keys-values-and-entries.md](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/04-record-keys-values-and-entries.md#L1)
