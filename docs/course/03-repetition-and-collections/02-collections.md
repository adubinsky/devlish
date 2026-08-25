# Lesson 3.2: Collections

Last updated: 2026-03-24
Status: Current lesson.

## Purpose

Introduce the idea that programs often work with many items, not just one.

## Learning Goals

- explain what a collection is
- write a simple Devlish list
- use a few first collection helpers

## Vocabulary

- collection
- list
- item
- group

## Big Idea

A collection is a group of values.

Examples:
- a list of invoice amounts
- a list of employees
- a list of missing documents

Collections matter because many useful programs do not act on one thing. They
act on many things.

## Example

```text
statuses equals list of " approved ", "pending", and "rejected"

Print count of statuses
Print first of statuses
Print trim first of statuses
Print join statuses with ", "
Print split "A|B|C" by "|"
```

You can run this lesson file at:

`docs/course/03-repetition-and-collections/examples/02_named_list_and_helpers.dvl`

## Line By Line

`statuses equals list of " approved ", "pending", and "rejected"`

This creates one list named `statuses`.
The list has three items.

`Print count of statuses`

This asks how many items are in the list.
The result is `3`.

`Print first of statuses`

This asks for the first item in the list.

`Print trim first of statuses`

This gets the first item, then removes extra spaces from the beginning and end.

`Print join statuses with ", "`

This turns the whole list into one string with commas between the items.

`Print split "A|B|C" by "|"`

This does the opposite kind of work.
It starts with one string and turns it into a list.

## Why This Matters

If you had three invoice amounts:

```text
1200
3000
450
```

You could think of them as one collection instead of three unrelated values.

Then the program could ask:

"For each invoice amount, should this go to manual review?"

That combines two important ideas:
- collections
- repetition

## Practice

1. Name three values that naturally belong in one collection.
2. Describe one repeated operation you would perform on each item.
3. Change the list items and run the example again.
4. Change the text used in the `split` example and predict the output first.

## Next Step

Once plain lists feel comfortable, move to:

`docs/course/03-repetition-and-collections/examples/03_records_and_collection_logic.dvl`

That lesson example introduces:
- records with named fields
- filtering a list
- sorting a list by a field
- asking whether any or all items match a rule

After that, move to:

`docs/course/03-repetition-and-collections/examples/04_transforming_collections.dvl`

That lesson example introduces:
- `map`
- `reject`
- `reduce`
- turning a list into one final answer
