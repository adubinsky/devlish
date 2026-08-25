# Lesson 6.1: What Is A Test?

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Teach that a test is a small check that compares expected behavior with actual
behavior.

## Learning Goals

- explain why tests matter
- read a simple Devlish-native test
- connect a test to the program behavior it protects

## Vocabulary

- test
- expected
- actual
- scenario

## First Example

Open the files here:
- [01_branching_workflow.dvl](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/examples/01_branching_workflow.dvl#L1)
- [01_branching_workflow.dvt](/Users/admin/code/devlish/docs/course/06-testing-and-debugging/checks/01_branching_workflow.dvt#L1)

## Run It

```bash
./bin/devlish test docs/course/06-testing-and-debugging/checks/01_branching_workflow.dvt
```

## Big Idea

A test is a promise about behavior.

In this lesson, the promise is:

"When the review status is approved, the program should route the work to the
approved queue."

## Practice

1. Read the scenario line by line.
2. Explain what behavior it is protecting.
