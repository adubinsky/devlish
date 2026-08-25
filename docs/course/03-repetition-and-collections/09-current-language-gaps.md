# Lesson 3.9: Current Devlish Strengths And Remaining Gaps

Last updated: 2026-06-30
Status: Current lesson.

## Purpose

Show what Devlish now supports for repetition and be honest about the remaining
rough edges.

## Learning Goals

- distinguish between a programming idea and a current language limitation
- understand which repetition ideas are now supported
- understand which repetition ideas still need more language work

## Vocabulary

- language gap
- first-class support
- workaround

## Big Idea

A course should not pretend the language can do something well when it cannot.

Right now, Devlish is strong for:
- workflows over one input
- extraction
- decisions
- class-style methods

Right now, Devlish can already do:
- `For each`
- `While`
- `Until`
- helpers such as `count`, `first`, `last`, `sort`, `item`, and `slice`
- predicate helpers such as `find`, `filter`, `reject`, `any`, and `all`
- grouping helpers such as `group by`, `index by`, and `partition`
- set-style helpers such as `union`, `intersection`, and `difference`
- list and record work for beginner examples
- `Append`
- `Pop`
- nested field updates with `Set`
- record field and shape checks

Right now, Devlish is still weaker for:
- very complex nested structured data
- deeply nested collection transforms that stay easy for beginners to read
- richer diagnostics when a transform expression is wrong
- more advanced loop debugging and tracing

## Why This Matters

This does not make the earlier lessons useless.

It means the course needs to do two things honestly:
- teach the programming idea
- say where the language still needs to grow

## What A Future Lesson Could Look Like

A stronger future Devlish lesson might say:

```text
For each invoice in invoices
  If invoice amount >= 1000
    Route invoice to manual_review_queue
```

That is readable and beginner-friendly.

Devlish has moved a lot further in this direction with `For each`, `While`,
`Until`, list literals, record-style data, `keys`, `values`, `entries`, `item`,
`slice`, `Append`, `Pop`, nested `Set`, record shape checks, predicate helpers
such as `find` and `filter`, grouping helpers, and set-style helpers. The next
gaps are less about basic loop syntax and more about depth: richer transform
expressions, complex nested data, stronger transform diagnostics, and clearer
teaching/debugging support.

## Related Reference

- [DEVLISH_LANGUAGE_GAPS.md](/Users/admin/code/devlish/docs/DEVLISH_LANGUAGE_GAPS.md#L1)

## Practice

1. Which part of the example above is the repeated step?
2. Which part assumes collection support with named fields?
3. Which remaining gap would most improve real programs?
