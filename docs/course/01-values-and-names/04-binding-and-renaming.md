# Lesson 1.4: Binding And Renaming

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Teach that a program can give an existing value a clearer name.

## Learning Goals

- explain why readable names matter
- bind one name to another value for clarity

## Vocabulary

- bind
- alias
- rename
- readability

## First Example

```text
Load docs/course/01-values-and-names/examples/03_review_packet.txt as Document
Alias Document as Packet
```

Open the example file here:
- [04_binding_aliases.dvl](/Users/admin/code/devlish/docs/course/01-values-and-names/examples/04_binding_aliases.dvl#L1)

## Big Idea

Sometimes the original name works, but another name makes the next few lines
easier to read.

An alias gives the same value another readable name.

## Run It

```bash
./bin/devlish run docs/course/01-values-and-names/examples/04_binding_aliases.dvl --debug
```

## Practice

1. Change `Packet` to `WorkingPacket`.
2. Run the file again.
3. Check the debug results to confirm the new alias name.
