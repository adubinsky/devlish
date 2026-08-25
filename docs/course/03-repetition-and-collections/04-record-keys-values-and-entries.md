# Lesson 3.4: Record Keys, Values, And Entries

Last updated: 2026-03-25
Status: Current lesson.

## Purpose

Teach how to inspect a record the way beginner Python courses teach dictionary
keys, values, and items.

## Learning Goals

- explain the difference between a key and a value
- use `keys of`, `values of`, and `entries of`
- loop through record entries
- read a nested value from a record inside another record

## Vocabulary

- record
- key
- value
- entry
- nested data

## Big Idea

A record groups related values under names.

That means a program can ask:
- what fields does this record have?
- what values are stored in it?
- how can I loop over each field and value?

Devlish now supports those ideas directly.

## Example

```text
invoice equals record with 1200 as amount and "pending" as status
review_packet equals record with invoice as invoice and "Ada" as reviewer
packet_keys equals keys of review_packet
packet_entries equals entries of review_packet
review_amount equals amount of invoice of review_packet

Print packet_keys
Print review_amount

For each entry in packet_entries:
  Print key of entry
```

You can run this lesson file at:

`docs/course/03-repetition-and-collections/examples/05_record_keys_values_entries.dvl`

## Line By Line

`invoice equals record with 1200 as amount and "pending" as status`

This creates one record named `invoice`.

`review_packet equals record with invoice as invoice and "Ada" as reviewer`

This creates one record named `review_packet`.
Inside it, the `invoice` field stores the invoice record.
That means the data is nested.

`packet_keys equals keys of review_packet`

This asks for the names of the fields in the record.

`packet_entries equals entries of review_packet`

This builds a collection where each item has a `key` and a `value`.
That makes it easier to loop through the record.

`review_amount equals amount of invoice of review_packet`

This reads a nested field.
The program first gets `invoice` from `review_packet`.
Then it gets `amount` from that invoice record.

`Print packet_keys`

This shows the field names.

`Print review_amount`

This shows the nested invoice amount.

`For each entry in packet_entries:`

This loops through the record entries one by one.

`Print key of entry`

This prints the field name for the current entry.

## Why This Matters

Many real programs use grouped data:
- an invoice with amount and status
- a person with name and role
- a review packet with several related fields

Keys, values, and entries make those records easier to inspect and process.

## Practice

1. Add another field to `review_packet` and predict the new keys first.
2. Print `values of review_packet`.
3. Add another nested field inside `invoice`.
4. Change the loop so it prints `value of entry` instead of `key of entry`.

## Related Reference

- [DEVLISH_LANGUAGE_GAPS.md](/Users/admin/code/devlish/docs/DEVLISH_LANGUAGE_GAPS.md#L1)
