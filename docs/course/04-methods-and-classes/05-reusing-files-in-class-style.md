# Lesson 4.5: Reusing Files In Class-Style Devlish

Last updated: 2026-03-26
Status: Current lesson.

## Purpose

Teach that class-style Devlish can reuse code from other files too.

## Learning Goals

- import a shared workflow fragment into a class method
- explain the difference between importing shared logic and importing a whole class file
- recognize that multi-file reuse works in both workflow-style and class-style Devlish

## Vocabulary

- import
- shared logic
- reusable fragment
- base class

## Big Idea

As programs grow, methods should not have to repeat the same setup logic over
and over.

Class-style Devlish now supports two useful reuse patterns:
- import a workflow-style fragment inside a method
- import another class-style file at the top of a class file

The first pattern is the most practical one to learn first.

## Example 1: Shared Workflow Logic Inside A Method

Shared file:
- [05_shared_review_logic.dvl](/Users/admin/code/devlish/docs/course/04-methods-and-classes/examples/05_shared_review_logic.dvl#L1)

Class file:
- [05_reusing_workflow_fragments_in_class.dvl](/Users/admin/code/devlish/docs/course/04-methods-and-classes/examples/05_reusing_workflow_fragments_in_class.dvl#L1)

Class file contents:

```text
Operations's Invoice Reviewer:
  review invoice:
    Import 05_shared_review_logic.dvl
    respond with final_label
```

Shared file contents:

```text
review_threshold equals 1000
final_label equals "needs_review"
```

## How To Run It

```bash
./bin/devlish run docs/course/04-methods-and-classes/examples/05_reusing_workflow_fragments_in_class.dvl --method review_invoice --args '[]'
```

## What Happens

1. The method imports the shared logic file.
2. The shared file creates values like `review_threshold` and `final_label`.
3. The method then responds with one of those imported values.

This is useful when several methods need the same setup logic.

## Example 2: Importing A Class File

Base class:
- [06_review_base.dvl](/Users/admin/code/devlish/docs/course/04-methods-and-classes/examples/06_review_base.dvl#L1)

Child class:
- [06_importing_class_files.dvl](/Users/admin/code/devlish/docs/course/04-methods-and-classes/examples/06_importing_class_files.dvl#L1)

Child class contents:

```text
Import 06_review_base.dvl

Operations's Invoice Reviewer based on Operations's Review Base:
  review invoice:
    respond with "ready"
```

This second pattern is about organization:
- the base class lives in one file
- the child class lives in another
- the child file can import the base file before its own class declaration

## Why This Matters

This is the beginning of real code reuse in larger Devlish programs.

It means you can:
- keep shared setup logic in one file
- split class hierarchies across files
- organize class-style code without copying and pasting everything

## Try This

1. Change `final_label` in the shared workflow fragment and run the method again.
2. Add another shared value to the fragment and print it before the response.
3. Rename the imported class file and update the import path.

## Check Yourself

1. What is the difference between importing a workflow fragment and importing a class file?
2. Which reuse pattern is easier for beginners to start with?
3. Why might a larger project need more than one file?
