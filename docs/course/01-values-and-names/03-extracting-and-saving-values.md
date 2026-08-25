# Lesson 1.3: Extracting And Saving Values

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Show how a Devlish program can find information in input and save it for later
steps.

## Learning Goals

- extract a value from a document
- save it under a name
- use the saved value later in the same program

## Vocabulary

- extract
- save
- name
- context

## First Example

```text
Load docs/course/01-values-and-names/examples/03_review_packet.txt as Document

Find invoice amount and save as invoice_amount
Find review status and save as review_status
```

Open the files here:
- [03_extract_and_save.dvl](/Users/admin/code/devlish/docs/course/01-values-and-names/examples/03_extract_and_save.dvl#L1)
- [03_review_packet.txt](/Users/admin/code/devlish/docs/course/01-values-and-names/examples/03_review_packet.txt#L1)

## Big Idea

Sometimes the information you need is already inside the input.

Extraction lets the program pull that information out and store it under a
clear name.

## Run It

```bash
./bin/devlish run docs/course/01-values-and-names/examples/03_extract_and_save.dvl --debug
```

## Practice

1. Change the invoice amount in the text file.
2. Run the file again.
3. Describe how the extracted value changed.
