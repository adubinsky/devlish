# Lesson 0.3: Input, Work, And Output

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Teach the beginner mental model that every program receives something, does
work, and produces a result.

## Learning Goals

- name the input of a Devlish program
- describe the work a program performs
- identify the output or effect

## Vocabulary

- input
- process
- output
- effect

## First Example

Use this workflow:

```text
Load docs/course/00-getting-started/examples/03_review_packet.txt as Document

Find review status and save as review_status

If review_status is "approved"
  Route invoice to approved_queue
Otherwise
  Route invoice to manual_review_queue
```

Open the files here:
- [03_route_review_packet.dvl](/Users/admin/code/devlish/docs/course/00-getting-started/examples/03_route_review_packet.dvl#L1)
- [03_review_packet.txt](/Users/admin/code/devlish/docs/course/00-getting-started/examples/03_review_packet.txt#L1)

## Big Idea

Many programs can be explained with three questions:

1. What goes in?
2. What work happens?
3. What comes out?

That is what “input, work, and output” means.

## Run It

```bash
./bin/devlish run docs/course/00-getting-started/examples/03_route_review_packet.dvl --debug
```

## Input

The input is the text file.

It contains the review status the program needs.

## Work

The work is:
- finding the review status
- storing it as `review_status`
- comparing it to `"approved"`

## Output

The output is the route:
- `approved_queue` when the status is approved
- `manual_review_queue` otherwise

## Line By Line

### `Load ... as Document`

This gives the program its input.

### `Find review status and save as review_status`

This pulls one useful value out of the input and remembers it.

### `If review_status is "approved"`

This asks a question.

### `Route invoice to approved_queue`

This is one possible result.

### `Otherwise`

This means “if the earlier condition was not true.”

### `Route invoice to manual_review_queue`

This is the other possible result.

## Practice

1. Change the review status in the text file from `approved` to `pending`.
2. Run the program again.
3. Explain what changed in the output.
