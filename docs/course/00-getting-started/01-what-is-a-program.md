# Lesson 0.1: What Is A Program?

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Introduce the idea that a program is a set of instructions a computer follows
step by step.

## Learning Goals

- explain what a program is in plain language
- describe input, work, and output
- read a tiny Devlish file from top to bottom

## Vocabulary

- program
- instruction
- input
- output

## First Example

In this lesson, we will use a very small Devlish program:

```text
Load docs/course/00-getting-started/examples/01_notice.txt as Document

Document must contain payment terms
Document must contain contact email
```

You can open the example files here:
- [01_check_notice.dvl](/Users/admin/code/devlish/docs/course/00-getting-started/examples/01_check_notice.dvl#L1)
- [01_notice.txt](/Users/admin/code/devlish/docs/course/00-getting-started/examples/01_notice.txt#L1)

## Big Idea

A program is a set of instructions.

The computer does not guess what you mean. It follows the instructions you
give it, in order, from top to bottom.

In this example:
- the input is a text document
- the work is checking whether certain phrases are present
- the output is a visible success or failure result

That pattern appears again and again in programming:
- receive something
- do work with it
- produce a result

## Run It

From the Devlish project root, run:

```bash
./bin/devlish run docs/course/00-getting-started/examples/01_check_notice.dvl --debug
```

Because the sample notice includes both required phrases, this run should
succeed.

## Line By Line

### `Load docs/course/00-getting-started/examples/01_notice.txt as Document`

This tells the program what to work on.

- `Load` means “bring this input into the program”
- the file path tells Devlish which text file to read
- `as Document` gives that loaded text a name inside the program

At this point, the program knows what its input is.

### `Document must contain payment terms`

This is the first check.

The program looks inside the loaded document and asks:

"Does this document contain the phrase `payment terms`?"

If the phrase is missing, this check fails.

### `Document must contain contact email`

This is the second check.

The program asks whether the document also contains the phrase
`contact email`.

Because the program reads from top to bottom, it performs this check after the
first one.

## What The Program Is Doing

Here is the same program described in plain language:

1. Open the notice file.
2. Call that file `Document`.
3. Check whether `Document` contains `payment terms`.
4. Check whether `Document` contains `contact email`.

That is a program.

It is small, but it still has the three main parts:
- input
- work
- output

## What Counts As Output Here?

In many beginner languages, the first output is something printed to the
screen.

In this Devlish lesson, the first output is simpler:
- the program succeeds if the document passes its checks
- the program fails if a required phrase is missing

That still counts as output because the program has produced a visible result.

## Try One Small Change

Open [01_check_notice.dvl](/Users/admin/code/devlish/docs/course/00-getting-started/examples/01_check_notice.dvl#L1) and change one line to this:

```text
Document must contain office address
```

Then run the file again.

What should happen?

It should fail, because the sample notice does not contain that phrase.

## Check Yourself

1. What is the input in this program?
   The input is the text file [01_notice.txt](/Users/admin/code/devlish/docs/course/00-getting-started/examples/01_notice.txt#L1).

2. What work does the program do?
   It checks whether the document contains required phrases.

3. What is the output?
   The output is whether the run succeeds or fails.

## Practice

1. In your own words, explain what `Load ... as Document` means.
2. Add a third check that should pass.
3. Add a third check that should fail.
4. Run the program after each change and describe what happened.

## Vocabulary Review

- `program`: a set of instructions for the computer
- `instruction`: one step in a program
- `input`: what the program receives
- `output`: the result the program produces

## Next Lesson

In Lesson 0.2, the focus will be on running Devlish programs comfortably and
reading their results with more confidence.
