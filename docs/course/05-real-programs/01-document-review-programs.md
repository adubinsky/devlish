# Lesson 5.1: Document Review Programs

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Combine extraction, naming, and decisions into one useful program.

## Learning Goals

- explain a complete document-review workflow
- identify where information is extracted
- describe how the final decision is made

## Vocabulary

- workflow
- review
- extract
- validate

## First Example

Open the files here:
- [01_document_review.dvl](/Users/admin/code/devlish/docs/course/05-real-programs/examples/01_document_review.dvl#L1)
- [01_document_review_packet.txt](/Users/admin/code/devlish/docs/course/05-real-programs/examples/01_document_review_packet.txt#L1)

## Run It

```bash
./bin/devlish run docs/course/05-real-programs/examples/01_document_review.dvl --debug
```

## Big Idea

A real program usually combines several smaller ideas:
- read input
- extract useful values
- make a decision
- produce an output

That full chain is a workflow.

## Practice

1. Change the review status in the packet.
2. Change the invoice amount in the packet.
3. Run the workflow after each change and explain the result.
