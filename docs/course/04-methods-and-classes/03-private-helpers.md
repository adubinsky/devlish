# Lesson 4.3: Private Helpers

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Teach that some methods are internal helpers and not part of the public entry
points.

## Learning Goals

- distinguish public methods from private helpers
- explain why a helper can make a class easier to read

## Vocabulary

- public
- private
- helper
- internal logic

## First Example

Open the example file here:
- [03_private_helper.dvl](/Users/admin/code/devlish/docs/course/04-methods-and-classes/examples/03_private_helper.dvl#L1)

## Big Idea

A private helper is a method the class uses internally.

It helps organize the work, but it is not meant to be the main entry point
someone calls from the outside.

## What To Notice

In this example:
- `review invoice` is the public method
- `escalation label` is the private helper

That structure makes the class easier to read because each method has one job.

## Practice

1. Read both methods and describe each job in one sentence.
2. Explain why `escalation label` is a helper instead of the main method.
