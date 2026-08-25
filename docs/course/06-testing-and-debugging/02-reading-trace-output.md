# Lesson 6.2: Reading Trace Output

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Show how a trace helps a beginner understand what the program actually did.

## Learning Goals

- recognize the major parts of trace output
- use a trace to explain a program result

## Vocabulary

- trace
- step
- context
- result

## First Example

Use this workflow:
- [01_branching_workflow.dvl](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/examples/01_branching_workflow.dvl#L1)

Run:

```bash
./bin/devlish trace docs/course/06-testing-and-debugging/examples/01_branching_workflow.dvl
```

## Big Idea

A trace shows how the program was understood and what it was prepared to do.

For a beginner, that is helpful because it makes the invisible parts of
programming more visible.

## Practice

1. Find the extracted value in the trace.
2. Find the branch in the trace.
3. Explain why the route is the one it is.
