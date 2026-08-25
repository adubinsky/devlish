# Lesson 5.4: Using More Than One File

Last updated: 2026-03-25
Status: Current lesson.

## Purpose

Teach the beginner idea that one program can reuse work from another file.

## Learning Goals

- explain what `Import` does
- run a Devlish program that depends on another file
- recognize why breaking a program into smaller files can help

## Vocabulary

- import
- reuse
- shared rules
- source file

## Big Idea

As programs grow, it helps to split them into smaller pieces.

One file can hold shared rules.
Another file can use those rules.

In Devlish, the first simple way to do that is:

```text
Import another_file.dvl
```

## Example Files

- [04_shared_review_rules.dvl](/Users/admin/code/devlish/docs/course/05-real-programs/examples/04_shared_review_rules.dvl#L1)
- [04_using_more_than_one_file.dvl](/Users/admin/code/devlish/docs/course/05-real-programs/examples/04_using_more_than_one_file.dvl#L1)

## Main Program

```text
Import 04_shared_review_rules.dvl

invoice_amount equals 1200

If invoice_amount >= review_threshold
  final_label equals review_label
Otherwise
  final_label equals safe_label

Print final_label
```

## Imported File

```text
review_threshold equals 1000
review_label equals "needs_review"
safe_label equals "approved"
```

## How To Run It

```bash
./bin/devlish run docs/course/05-real-programs/examples/04_using_more_than_one_file.dvl
```

## What Happens

1. Devlish reads the imported file first.
2. It learns the shared values from that file.
3. Then it runs the main file.
4. The main file can use names from the imported file.

## Expected Output

```text
needs_review
```

## Why This Matters

This is the beginning of code organization.

It means you can:
- keep shared rules in one place
- reuse them in more than one program
- change one rule file instead of editing many copies

## Try This

1. Change `review_threshold` in the imported file to `1500`.
2. Run the main file again.
3. Explain why the output changed.

## Check Yourself

1. Which file holds the shared rule values?
2. Which file makes the decision?
3. Why is `Import` useful in larger programs?
