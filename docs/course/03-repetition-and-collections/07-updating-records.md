# Lesson 3.7: Updating Records

Last updated: 2026-03-26
Status: Current lesson.

## Purpose

Teach how to change a value inside a record, including a value inside a nested
record.

## Learning Goals

- update one field in a record with `Set ... to ...`
- update a nested field such as `amount of invoice of review_packet`
- see that records can be created gradually, not only all at once
- connect record updates to the idea of program state changing over time

## Vocabulary

- update
- nested field
- state
- mutation

## Big Idea

Reading data is only half of programming.

Useful programs also change data.

With records, that means a program should be able to:
- create a record
- read fields from that record
- update fields later

Devlish now supports that with a readable form:

`Set amount of invoice of review_packet to 1300`

That reads almost like spoken instructions:
- go to `review_packet`
- inside it, go to `invoice`
- inside that, set `amount`

## Example Program

File:
- [08_updating_records.dvl](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/examples/08_updating_records.dvl#L1)

```text
Set amount of invoice of review_packet to 1300
Set reviewer of review_packet to "Ada"
Print amount of invoice of review_packet
Print reviewer of review_packet
```

## How To Run It

```bash
./bin/devlish run docs/course/03-repetition-and-collections/examples/08_updating_records.dvl
```

## What Each Line Means

`Set amount of invoice of review_packet to 1300`
If `review_packet` does not exist yet, Devlish creates it.
If `invoice` does not exist yet, Devlish creates that nested record too.
Then it stores `1300` in the `amount` field.

`Set reviewer of review_packet to "Ada"`
This adds another field to the same outer record.

`Print amount of invoice of review_packet`
This reads back the nested value we just stored.

`Print reviewer of review_packet`
This reads back the outer record field.

## Expected Result

The program prints:

```text
1300
Ada
```

After both updates, the record behaves like:

```text
review_packet = {
  invoice: {
    amount: 1300
  },
  reviewer: "Ada"
}
```

## Why This Matters

This is a major programming idea:
- programs can build structured data over time
- programs can change one part of a larger structure
- nested data can model real things such as invoices, packets, and reports

Without update operations, records are mostly static.
With update operations, they become useful working data.

## Try This

1. Add `Set status of invoice of review_packet to "pending"`.
2. Print `status of invoice of review_packet`.
3. Change the amount from `1300` to `1500`.
4. Add another top-level field such as `priority`.

## Check Yourself

1. What part of the record changes in `Set amount of invoice of review_packet to 1300`?
2. Does Devlish need the full record to exist first?
3. What is the difference between reading `amount of invoice of review_packet` and setting it?

## Related Lessons

- [04-record-keys-values-and-entries.md](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/04-record-keys-values-and-entries.md#L1)
- [05-changing-lists.md](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/05-changing-lists.md#L1)
- [06-controlling-loops.md](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/06-controlling-loops.md#L1)
