# Lesson 4.4: Class-Style Devlish

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Introduce the class-style form of Devlish as a way to group related methods.

## Learning Goals

- read a class header
- identify the class name, method names, and parameters
- explain the difference between workflow-style and class-style Devlish

## Vocabulary

- class
- module
- method
- helper

## First Example

Open the example file here:
- [04_invoice_reviewer.dvl](/Users/admin/code/devlish/docs/course/04-methods-and-classes/examples/04_invoice_reviewer.dvl#L1)

## Big Idea

Workflow-style Devlish is good for step-by-step processes.

Class-style Devlish is good for reusable logic grouped into named methods.

## Run It

```bash
./bin/devlish run docs/course/04-methods-and-classes/examples/04_invoice_reviewer.dvl --method review_invoice --args '[12000]'
```

## What To Notice

The class has:
- a module name: `Operations`
- a class name: `Invoice Reviewer`
- a public method: `review invoice`
- a private helper: `escalation label`

## Practice

1. Run the method with `[8000]`.
2. Compare the returned label to the result for `[12000]`.
