# Programming With Devlish

Last updated: 2026-03-23
Status: Current beginner-first course plan.

## Purpose

This is the teaching plan for learning programming from zero with Devlish as
the first language.

It is modeled after the conceptual arc of an introductory Python course, but
it is written in Devlish terms and assumes the student knows no existing
language at all.

The course is designed to teach the major ideas of programming:
- programs are step-by-step instructions
- values represent information
- names let a program remember information
- comparisons ask questions
- conditionals choose between paths
- repetition handles more than one item
- methods reuse logic
- tests check whether a program behaves as intended

## Teaching Voice

The course voice should be:
- calm
- plainspoken
- concrete
- patient
- encouraging

The writing should:
- define every new term before using it casually
- explain what each line means in ordinary language
- use small examples before rules
- avoid comparing Devlish to other languages unless absolutely necessary
- treat confusion as normal

## Beginner Promise

By the end of the course, a student should be able to:
- read a small Devlish program
- run it
- explain what values it stores
- explain why it chooses one path instead of another
- write a small workflow
- write a small class-style method
- test behavior with Devlish-native tests

## Course Structure

The course should be organized around programming ideas first, not syntax
lists.

### Unit 0: What Programming Is

Goal:
- explain what a computer program is
- explain what it means to run a file
- explain input, work, and output

Topics:
- a program is a list of instructions
- the computer follows those instructions in order
- Devlish programs describe work in near-English

Student outcome:
- "I understand what a program does."

### Unit 1: First Programs

Goal:
- teach sequencing
- show that a program can inspect input and produce a result

Topics:
- loading a document
- checking for required text
- reading program results

Major ideas:
- sequence
- input
- output
- observable behavior

Planned lessons:
- `docs/course/00-getting-started/01-what-is-a-program.md`
- `docs/course/00-getting-started/02-running-your-first-program.md`
- `docs/course/00-getting-started/03-input-work-output.md`

Example arc:
- read a tiny Devlish file from top to bottom
- run a first program that inspects a document
- explain what the program received and what result it produced

### Unit 2: Values And Names

Goal:
- teach values, variables, and assignment

Topics:
- extracting a value
- storing it under a name
- reusing that name later
- giving a value another name

Major ideas:
- values
- variables
- assignment
- remembering information

Planned lessons:
- `docs/course/01-values-and-names/01-values.md`
- `docs/course/01-values-and-names/02-names-and-memory.md`
- `docs/course/01-values-and-names/03-extracting-and-saving-values.md`
- `docs/course/01-values-and-names/04-binding-and-renaming.md`

Example arc:
- save a found value under a name
- use that name later in the same program
- rename a value so the program reads more clearly

### Unit 3: Comparisons And Decisions

Goal:
- teach how a program asks questions and chooses a path

Topics:
- equality
- greater than / less than
- true and false
- `If`
- `Otherwise`

Major ideas:
- comparisons
- boolean logic
- control flow
- branches

Planned lessons:
- `docs/course/02-decisions-and-logic/01-comparisons.md`
- `docs/course/02-decisions-and-logic/02-if-and-otherwise.md`
- `docs/course/02-decisions-and-logic/03-routing-decisions.md`

Example arc:
- compare two values
- decide whether a review is needed
- send work to one place or another based on a condition

### Unit 4: Repetition And Collections

Goal:
- teach the idea of repeated work and working with more than one item

Topics:
- what a collection is
- what a loop is
- doing the same kind of work for each item

Major ideas:
- loops
- repetition
- collections

Important teaching note:
- this unit is conceptually essential
- Devlish does not yet support this area strongly enough for a full beginner
  experience
- the course should teach the idea honestly and point to the gap document

### Unit 5: Reusable Logic

Goal:
- teach methods, inputs, and return values

Topics:
- what a method is
- parameters
- return values
- public methods
- private helpers

Major ideas:
- abstraction
- reuse
- inputs
- outputs

Planned lessons:
- `docs/course/04-methods-and-classes/01-what-a-method-is.md`
- `docs/course/04-methods-and-classes/02-parameters-and-return-values.md`
- `docs/course/04-methods-and-classes/03-private-helpers.md`
- `docs/course/04-methods-and-classes/04-class-style-devlish.md`

Example arc:
- build a simple calculator method
- pass a value into a method
- return a result
- separate public work from private helper logic

### Unit 6: Real Programs

Goal:
- combine beginner ideas into complete workflows

Topics:
- extraction
- validation
- routing
- service outputs
- domain workflows

Major ideas:
- combining steps
- end-to-end program flow
- data in, decisions made, outputs produced

Planned lessons:
- `docs/course/05-real-programs/01-document-review-programs.md`
- `docs/course/05-real-programs/02-notifications-and-messages.md`
- `docs/course/05-real-programs/03-domain-workflows.md`

Example arc:
- inspect a realistic document
- notify someone about a result
- connect several steps into a small business workflow

### Unit 7: Testing And Debugging

Goal:
- teach how programmers verify and explain behavior

Topics:
- what a test is
- expected vs actual result
- how to trace a program
- how to inspect a failure

Major ideas:
- testing
- debugging
- confidence

Planned lessons:
- `docs/course/06-testing-and-debugging/01-what-is-a-test.md`
- `docs/course/06-testing-and-debugging/02-reading-trace-output.md`
- `docs/course/06-testing-and-debugging/03-fixing-a-bug.md`

Example arc:
- write a first behavior check
- read a trace to understand what happened
- fix a wrong decision and confirm the repair

### Unit 8: Systems And Larger Projects

Goal:
- show how Devlish programs fit inside larger systems

Topics:
- definitions
- processes
- packaging
- gateways
- runtime boundaries

Major ideas:
- architecture
- separation of concerns
- moving from one file to a small system

Planned lessons:
- `docs/course/projects/01-first-document-checker.md`
- `docs/course/projects/02-first-branching-workflow.md`
- `docs/course/projects/03-first-class-calculator.md`
- `docs/course/projects/04-first-tested-real-program.md`

## Lesson Template

Every beginner lesson should follow the same structure:

1. Big idea
2. New vocabulary
3. Small runnable example
4. Line-by-line explanation
5. What the program remembers
6. What decision the program makes
7. One small change exercise
8. One or two short practice tasks
9. One checkpoint test or expected result

## Example Policy

Examples should start small and grow gradually.

The teaching order should be:
1. tiny mechanical examples
2. small decision examples
3. reusable-logic examples
4. domain examples
5. system examples

Examples should always answer:
- what is the input
- what is the program trying to decide
- what values does it store
- what does it output

## Planned New Folder Structure

The course should not depend on the old archived sample tree.

The planned new teaching structure is:

```text
docs/course/
  README.md
  00-getting-started/
  01-values-and-names/
  02-decisions-and-logic/
  03-repetition-and-collections/
  04-methods-and-classes/
  05-real-programs/
  06-testing-and-debugging/
  projects/
```

Each unit should contain:
- lesson documents
- beginner exercises
- small runnable Devlish files
- matching `.dvt` checkpoints where appropriate

## Planned Lesson Flow

### Unit 0: Getting Started
- `docs/course/00-getting-started/01-what-is-a-program.md`
- `docs/course/00-getting-started/02-running-your-first-program.md`
- `docs/course/00-getting-started/03-input-work-output.md`

### Unit 1: Values And Names
- `docs/course/01-values-and-names/01-values.md`
- `docs/course/01-values-and-names/02-names-and-memory.md`
- `docs/course/01-values-and-names/03-extracting-and-saving-values.md`
- `docs/course/01-values-and-names/04-binding-and-renaming.md`

### Unit 2: Decisions And Logic
- `docs/course/02-decisions-and-logic/01-comparisons.md`
- `docs/course/02-decisions-and-logic/02-if-and-otherwise.md`
- `docs/course/02-decisions-and-logic/03-routing-decisions.md`

### Unit 3: Repetition And Collections
- `docs/course/03-repetition-and-collections/01-what-repetition-means.md`
- `docs/course/03-repetition-and-collections/02-collections.md`
- `docs/course/03-repetition-and-collections/03-language-gaps-for-loops.md`

### Unit 4: Methods And Classes
- `docs/course/04-methods-and-classes/01-what-a-method-is.md`
- `docs/course/04-methods-and-classes/02-parameters-and-return-values.md`
- `docs/course/04-methods-and-classes/03-private-helpers.md`
- `docs/course/04-methods-and-classes/04-class-style-devlish.md`

### Unit 5: Real Programs
- `docs/course/05-real-programs/01-document-review-programs.md`
- `docs/course/05-real-programs/02-notifications-and-messages.md`
- `docs/course/05-real-programs/03-domain-workflows.md`

### Unit 6: Testing And Debugging
- `docs/course/06-testing-and-debugging/01-what-is-a-test.md`
- `docs/course/06-testing-and-debugging/02-reading-trace-output.md`
- `docs/course/06-testing-and-debugging/03-fixing-a-bug.md`

### Projects
- `docs/course/projects/README.md`

## What This Course Still Needs

To become a full teaching product rather than a course plan, the repo still
needs:
- lesson-by-lesson prose
- student exercises
- instructor notes
- checkpoint projects
- collection and loop lessons once the language surface improves

## Related References

- `docs/course/README.md`
- `docs/LANGUAGE_REFERENCE.md`
- `docs/TESTING_REFERENCE.md`
- `docs/PACKAGING_REFERENCE.md`
- `docs/DEVLISH_LANGUAGE_GAPS.md`
