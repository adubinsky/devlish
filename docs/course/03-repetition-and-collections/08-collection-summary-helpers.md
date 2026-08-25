# Lesson 3.8: Collection Summary Helpers

Last updated: 2026-03-26
Status: Current lesson.

## Purpose

Teach how to clean up a collection and then ask simple summary questions about
it.

## Learning Goals

- flatten a nested list into one list
- remove duplicates with `unique of ...`
- find the smallest and largest value in a list
- explain when summary helpers are easier than writing a loop yourself

## Vocabulary

- nested list
- flatten
- unique
- minimum
- maximum

## Big Idea

Sometimes a program already has the data it needs, but the data is in the wrong
shape.

One list may contain smaller lists.
One list may repeat the same value more than once.
One list may need a smallest or largest value.

Devlish now includes a few helpers for those jobs:
- `flatten ...`
- `unique of ...`
- `minimum of ...`
- `maximum of ...`

## Example File

- [09_collection_summary_helpers.dvl](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/examples/09_collection_summary_helpers.dvl#L1)

## Example

```text
nested_statuses equals list of list of "approved" and "pending", and list of "approved" and "rejected"
flattened_statuses equals flatten nested_statuses
unique_statuses equals unique of flattened_statuses
smallest_amount equals minimum of list of 1200, 300, and 800
largest_amount equals maximum of list of 1200, 300, and 800
Print count of unique_statuses
Print smallest_amount
Print largest_amount
```

## How To Run It

```bash
./bin/devlish run docs/course/03-repetition-and-collections/examples/09_collection_summary_helpers.dvl
```

## What Happens

1. `nested_statuses` starts as a list that contains smaller lists.
2. `flatten nested_statuses` turns it into one flat list.
3. `unique of flattened_statuses` removes repeated values.
4. `minimum of ...` finds the smallest number.
5. `maximum of ...` finds the largest number.

## Expected Output

```text
3
300
1200
```

## Why This Matters

These helpers let beginners solve common problems directly:
- clean up repeated values
- work with nested lists
- summarize numeric data

You could write loops for these jobs later.

At the start, direct helpers make the idea easier to see.

## Try This

1. Add another `"pending"` value to the nested list and run the file again.
2. Change one of the amounts to `50` and see how `minimum of ...` changes.
3. Replace `count of unique_statuses` with `first of unique_statuses`.

## Check Yourself

1. What does `flatten` change about the shape of the data?
2. Why does `unique of ...` not change the count in the output if there are no duplicates?
3. When would `maximum of ...` be useful in a real program?
