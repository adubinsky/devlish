# Lesson 1.5: Working With Text And Numbers

Last updated: 2026-03-25
Status: Current lesson.

## Purpose

Teach beginners that a program can clean up text and adjust numbers before
using them.

## Learning Goals

- turn text into uppercase and lowercase
- replace one piece of text with another
- use `absolute value of` for negative numbers
- use `round` to simplify a number

## Vocabulary

- uppercase
- lowercase
- replace
- absolute value
- round

## Example Program

File:
- [05_text_and_number_helpers.dvl](/Users/admin/code/devlish/docs/course/01-values-and-names/examples/05_text_and_number_helpers.dvl#L1)

```text
status_text equals " Pending Review "
loud_status equals uppercase trim status_text
quiet_status equals lowercase loud_status
cleaned_status equals replace " " in quiet_status with "_"
distance equals absolute value of -4.8
rounded_distance equals round distance
Print loud_status
Print cleaned_status
Print rounded_distance
```

## How To Run It

```bash
./bin/devlish run docs/course/01-values-and-names/examples/05_text_and_number_helpers.dvl
```

## What Each Line Means

`status_text equals " Pending Review "`
Stores text with extra spaces around it.

`loud_status equals uppercase trim status_text`
First removes the extra spaces, then changes the text to all capital letters.

`quiet_status equals lowercase loud_status`
Changes that capital text into all lowercase letters.

`cleaned_status equals replace " " in quiet_status with "_"`
Replaces spaces with underscores so the text is easier to use as a label.

`distance equals absolute value of -4.8`
Turns a negative number into its positive size.

`rounded_distance equals round distance`
Rounds the number to the nearest whole number.

`Print loud_status`
Shows the uppercase version of the text.

`Print cleaned_status`
Shows the cleaned label with underscores.

`Print rounded_distance`
Shows the rounded number.

## Expected Result

The program prints:

```text
PENDING REVIEW
pending_review
5
```

## Why This Matters

Real programs often need to tidy up messy input before they can use it.

This lesson shows two very common programming ideas:
- clean up text
- reshape a number into the form you need

## Try This

1. Change `Pending Review` to `Needs Approval`.
2. Replace spaces with `-` instead of `_`.
3. Change `-4.8` to `-12.2` and rerun the file.

## Check Yourself

1. What is the difference between `uppercase` and `lowercase`?
2. Why might a program use `replace " " in text with "_"`?
3. What does `absolute value of -4.8` return?

## Related Lessons

- [01-values.md](/Users/admin/code/devlish/docs/course/01-values-and-names/01-values.md#L1)
- [02-names-and-memory.md](/Users/admin/code/devlish/docs/course/01-values-and-names/02-names-and-memory.md#L1)
