# Lesson 2.3: Routing Decisions

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Show that decisions in code can send work to different destinations.

## Learning Goals

- connect a condition to an outcome
- route one item to different destinations based on a comparison

## Vocabulary

- route
- destination
- decision
- outcome

## First Example

```text
Load docs/course/02-decisions-and-logic/examples/03_review_route_packet.txt as Document

Find review status and save as review_status

If review_status is "approved"
  Route invoice to approved_queue
Otherwise
  Route invoice to manual_review_queue
```

Open the files here:
- [03_routing_decisions.dvl](/Users/admin/code/devlish/docs/course/02-decisions-and-logic/examples/03_routing_decisions.dvl#L1)
- [03_review_route_packet.txt](/Users/admin/code/devlish/docs/course/02-decisions-and-logic/examples/03_review_route_packet.txt#L1)

## Big Idea

A route is one kind of program output.

After the program makes a decision, it can send the work somewhere specific.

## Run It

```bash
./bin/devlish run docs/course/02-decisions-and-logic/examples/03_routing_decisions.dvl --debug
```

## What Happens

The program:
1. loads the packet
2. finds the review status
3. checks whether it is approved
4. chooses a route

## Practice

1. Change the review status in the text file from `approved` to `pending`.
2. Run the file again.
3. Explain why the route changed.
