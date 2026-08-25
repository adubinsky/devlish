# Lesson 3.3: Transforming Collections

Last updated: 2026-03-24
Status: Current lesson.

## Purpose

Teach how to change a collection, remove unwanted items, and combine many items
into one result.

## Learning Goals

- explain what it means to transform a collection
- use `map` to change every item
- use `reject` to remove items you do not want
- use `reduce` to build one final answer from many items

## Vocabulary

- transform
- map
- reject
- reduce
- accumulator

## Big Idea

Collections are useful because they let one program work on many values.

But programs do more than loop through a list.
They often need to:
- clean every item
- keep only some items
- count or total the final result

Devlish now supports all three of these ideas directly.

## Example

```text
statuses equals list of " approved ", "pending", and "rejected"
cleaned_statuses equals map statuses to trim item
kept_statuses equals reject cleaned_statuses where item is "pending"
status_count equals reduce kept_statuses starting at 0 with total and item to total plus 1

Print first of cleaned_statuses
Print status_count
```

You can run this lesson file at:

`docs/course/03-repetition-and-collections/examples/04_transforming_collections.dvl`

## Line By Line

`statuses equals list of " approved ", "pending", and "rejected"`

This creates a starting list.

`cleaned_statuses equals map statuses to trim item`

`map` goes through the list one item at a time.
For each item, it runs the expression after `to`.
Here, it trims extra spaces from each status.

`kept_statuses equals reject cleaned_statuses where item is "pending"`

`reject` removes items that match the rule after `where`.
Here, the program throws away `"pending"`.

`status_count equals reduce kept_statuses starting at 0 with total and item to total plus 1`

`reduce` combines many items into one final value.
It starts with `0`.
Then for each item, it calculates the next `total`.
Because the expression says `total plus 1`, this program counts the kept items.

`Print first of cleaned_statuses`

This shows the first cleaned value.

`Print status_count`

This shows the final count.

## Why This Matters

Many useful programs follow this pattern:

1. start with a list
2. clean it
3. remove items you do not want
4. compute one final answer

That is a real programming pattern, not just a Devlish pattern.

## Practice

1. Change the item rejected by the program.
2. Replace `trim item` with another expression that changes each item.
3. Predict the final count before you run the file.
4. Change the starting list so only one item remains after `reject`.

## Related Reference

- [DEVLISH_LANGUAGE_GAPS.md](/Users/admin/code/devlish/docs/DEVLISH_LANGUAGE_GAPS.md#L1)
