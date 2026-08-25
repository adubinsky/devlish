# Lesson 3.1: What Repetition Means

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Teach the idea of doing the same kind of work more than once.

## Learning Goals

- explain repetition in plain language
- recognize when a task should repeat

## Vocabulary

- repeat
- loop
- each
- item

## Big Idea

Repetition means doing the same kind of work for more than one item.

In everyday life:
- check every line on an invoice
- review every applicant in a list
- send a reminder to every customer with a missing document

Programming languages need a way to describe that repeated work clearly.

## Everyday Example

Imagine you are checking three forms.

Without repetition, you might describe the work like this:

1. Check form 1.
2. Check form 2.
3. Check form 3.

That works for three forms, but it does not scale well.

The better idea is:

"For each form, do the same check."

That is the mental model behind loops.

## Why Beginners Need This

Repetition is one of the central ideas of programming.

Without it, programs stay stuck at:
- one input
- one decision
- one result

With repetition, programs can work across a whole set of items.

## Practice

1. Name one real-world task that repeats over many items.
2. Describe the repeated step in one sentence.
3. Explain why repeating the idea is better than writing the same step ten times.

## First Runnable Devlish Example

Devlish can now express a simple loop directly:

```text
For each status in approved and pending and rejected:
  Print status
```

Open the example file here:
- [01_for_each_statuses.dvl](/Users/admin/code/devlish/docs/course/03-repetition-and-collections/examples/01_for_each_statuses.dvl#L1)

Run it with:

```bash
./bin/devlish run docs/course/03-repetition-and-collections/examples/01_for_each_statuses.dvl
```
