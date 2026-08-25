# Lesson 0.2: Running Your First Program

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Show the student how to run a Devlish file and observe its result.

## Learning Goals

- run a Devlish program from the command line
- recognize a successful run
- describe what changed because the program ran

## Vocabulary

- run
- result
- success
- trace

## First Example

This lesson uses two almost identical files:

- [01_check_notice.dvl](/Users/admin/code/devlish/docs/course/00-getting-started/examples/01_check_notice.dvl#L1)
- [02_check_notice_missing_email.dvl](/Users/admin/code/devlish/docs/course/00-getting-started/examples/02_check_notice_missing_email.dvl#L1)

One succeeds.
One fails.

## Big Idea

Running a program means asking the computer to follow the instructions in a
file right now.

The file does not become “real” only after a human explains it. It becomes
real when the computer follows its steps.

## Run A Successful File

```bash
./bin/devlish run docs/course/00-getting-started/examples/01_check_notice.dvl --debug
```

You should see a successful result.

## Run A Failing File

```bash
./bin/devlish run docs/course/00-getting-started/examples/02_check_notice_missing_email.dvl --debug
```

You should see a failure because the second notice is missing one required
phrase.

## Why This Is Useful

Failure is not always bad.

If a document is missing required information, the correct behavior may be to
fail and report that problem.

That is part of what programs are for.

## What `--debug` Shows

The `--debug` flag gives you extra visibility.

It helps a beginner answer:
- what file ran
- what steps happened
- what passed
- what failed

## Check Yourself

1. What changed between the two runs?
   The input document changed.

2. Did the instructions change?
   No.

3. Why did the result change?
   Because the program received different input.

## Practice

1. Run both files without `--debug`.
2. Run both files again with `--debug`.
3. Describe one thing the debug output taught you.
