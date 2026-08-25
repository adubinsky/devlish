# Devlish Standard Library (Current)

Last updated: 2026-07-10
Status: Current implementation reference.

## Design Principle

Devlish is meant to read as a controlled subset of English.

That means:
- verbosity is a feature, not a bug
- author-facing concepts should be described as English phrases
- internal parser notation is descriptive, not normative
- parser keywords are not automatically standard-library commands

This file separates the current language into the layers that matter.

For the Devlish 2.0 execution-model rewrite plan, see
`docs/DEVLISH_2_0_PLAN.md`.

## The Four Layers

### 1. English Surface Grammar

These are words and phrases that structure sentences. They are part of the
surface language, but they are not standard-library operations.

Examples:
- `if`
- `otherwise`
- `for each`
- `when`
- `every`
- `and`
- `or`
- `not`
- `the`
- `a`
- `an`
- `all`

These are handled primarily by the English parser in
`lib/devlish/parser/english_parser.rb`.

### 2. Built-In Domain Nouns

These are built-in language concepts, not imperative commands.

Examples:
- `document`
- service names such as `NotificationService`
- common built-in business nouns currently ignored during term validation, such
  as `Order`, `Invoice`, and `User`

Some of these are true language nouns. Others are parser guardrails to avoid
false undefined-term errors.

### 3. Core Standard-Library Operations

These are the closest thing Devlish currently has to a real standard library.
Older runtime helpers still exist in `lib/devlish/dsl/base.rb`, but the
language direction is AST-first: standard-library operations should be modeled
as parsed nodes, lowered operations, or declared runtime effects.

Current core operations:
- `load document`
- `check`
- `extract`
- `validate`
- `calculate`
- `flag`
- `store as`
- `require presence`
- `document must contain`
- `require`
- `find`
- `must be at least`
- `must be at most`
- `service ... action ...`
- `route`
- `bind`
- `copy file from ... to ...`
- `move file from ... to ...`
- `create directory`
- `delete file`
- `check if ... exists`
- `get file info for`
- `list files in`
- `find files matching ... in ...`
- `get the url at`
- `post to`
- `put to`
- `delete the url at`
- `download`
- `respond with`

Implementation reference: `crates/devlish_core/src/lib.rs` (Rust parser and compiler)

### 4. Service and Workflow Verbs

These are higher-level action phrases that the English parser lowers into the
core runtime.

Examples:
- `search the PatientService for patient_id`
- `create DecisionLog entry with patient_id, decision_status`
- `send email via NotificationService to billing_team`
- `send message to claims queue`
- `notify legal team`

These are not independent low-level primitives in the same sense as
`load document` or `route`. They are English surface patterns that compile into
service actions.

## What Is Not a Command

The following should not be documented as standard-library commands:
- `if`
- `then`
- `else`
- `otherwise`
- `and`
- `or`
- `not`
- `for`
- `each`
- `when`
- `every`

These are grammar and control-flow words.

The following should not usually be documented as commands either:
- `document`

`document` is better described as a built-in noun with associated sentence
forms, such as:
- `document must contain ...`
- `document should have ...`
- `document include ...`

## Current Minimal Core Library

If we strip away parser grammar and domain nouns, the smallest practical
English-first standard library today is roughly:

### Input and Loading
- `load document`
- `load <path>`
- `load <path> as <name>`

### Requirement and Presence
- `document must contain <thing>`
- `require <thing>`
- `check for <thing>`

### Extraction
- `find <thing>`
- `extract <thing>`
- `find <thing> and save as <name>`

### Assignment and Calculation
- `<name> equals <expression>`
- `calculate <thing>`

### Validation
- `<name> must be at least <value>`
- `<name> must be at most <value>`
- `<name> must equal <value>`
- `<name> must contain <value>`
- `<name> must match <pattern>`
- `<name> must be present`
- `<name> must be missing`
- `<name> must be one of <list>`
- `validate <name>`

### Routing and Integration
- `route <thing> to <destination>`
- `search the <service> for <value>`
- `create <service> entry with <fields>`
- `send email ...`
- `send message ...`
- `notify ...`

### Naming and Binding
- `alias <source> as <target>`
- `nickname <source> as <target>`
- `symbol <source> as <target>`
- `handle <source> as <target>`

## What Is Missing From an English-First Standard Library

The current runtime is small and workable, but it is uneven.

What appears missing is not "more grammar tokens." It is a cleaner set of
English-named primitives.

### 1. Symmetric Output and Persistence

Current state:
- `load document` exists
- storage mostly means "save extracted value into context"

Missing English-first primitives:
- `save document`
- `write report`
- `export results`
- `store <value> as <name>` as a first-class surface form

### 2. First-Class Assignment Language

Current state:
- `<name> equals <expression>` works
- it compiles directly to Ruby assignment

Missing English-first primitives:
- `set <name> to <value>`
- `remember <value> as <name>`
- `use <name> for <value>`

These would read more like English than exposing internal assignment behavior.

### 3. Richer Validation Vocabulary

Current state:
- `must be at least`
- `must be at most`
- `must equal`
- `must match`
- `must contain`
- `must be present`
- `must be missing`
- `must be one of`

Missing English-first primitives:
- reusable named validation rules
- validation report export separate from assertion export

### 4. Basic Collection Operations

Current state:
- `for each` exists
- `count`, `first`, `last`, `sort`
- `find`, `filter`, `reject`, `any`, `all`
- `group by`, `index by`, `partition`
- `take`, `drop`, `zip`, `chunk`
- `union`, `intersection`, `difference`

Missing English-first primitives:
- `select`
- `keep`
- `discard`
- richer predicate expressions
- richer transform expressions

### 5. Basic Text Operations

Current state:
- extraction and regex inference exist
- `trim`, `split`, `join`, `replace`
- `normalize whitespace`
- `slugify`, `title case`, `sentence case`
- `contains`, `starts with`, `ends with`

Missing English-first primitives:
- Unicode normalization policy
- regex-style pattern matching as a documented language form

### 6. Consistent Service Vocabulary

Current state:
- service actions are implemented as a collection of parser phrases
- HTTP verbs (Get, Post, Put, Delete) are planned as native keywords (DEVL-75)
- structured output via Respond/Fail is planned (DEVL-76)

The HTTP vocabulary uses English-natural verb phrases:

| Verb | Devlish keyword | Purpose |
| --- | --- | --- |
| GET | `Get the url at "<url>" as <var>` | Retrieve data from a URL |
| POST | `Post to "<url>" with <body> as <var>` | Submit data to a URL |
| PUT | `Put to "<url>" with <body> as <var>` | Update a resource at a URL |
| DELETE | `Delete the url at "<url>" as <var>` | Remove a resource at a URL |

Output vocabulary:

| Keyword | Purpose | Exit code |
| --- | --- | --- |
| `Respond with <value>` | Return structured JSON result | 0 |
| `Fail with <record>` | Return structured JSON exception | 1 |
| `Fail with "<string>"` | Return error message (existing) | 1 |

Legacy service call patterns (Search the, Create entry, Send email via) remain
for backward compatibility but are not the recommended path for HTTP-based
integrations.

## Recommendation

Future docs should distinguish these categories explicitly:

1. Surface grammar words
2. Built-in nouns
3. Core library operations
4. Service phrase patterns

The user-facing language should continue to privilege readable English phrases
over compact symbolic notation.

## Codebase Review Recommendations To Complete the Grammar

These recommendations come from reviewing the current parser, AST/IR runtime,
executor, and bundled services together.

### 1. Make the grammar inventory executable

Only document a phrase as "current grammar" if it parses and runs.

Today there are several places where the language model is broader than the
runtime contract. Grammar completion should start by treating runnable surface
forms as the source of truth.

### 2. Keep grammar words separate from library operations

Control-flow words should stay in the grammar layer:
- `if`
- `otherwise`
- `for each`
- `when`
- `every`
- `and`
- `or`
- `not`

Library operations should stay in the standard-library layer:
- `load document`
- `find ... and save as ...`
- `document must contain ...`
- `route ... to ...`
- notification and messaging phrases

This keeps Devlish English-first instead of turning it into Ruby terminology
with a prose wrapper.

### 3. Define the input/output story explicitly

The most basic teaching and grammar-completion question is not "what verbs do
we have?" It is "how does a Devlish program take input and produce output?"

Current practical inputs:
- document text loaded from a path
- one-line prompt input with `Ask "prompt" as name`
- multi-line prompt input with `Ask multiline "prompt" as name`
- one-line stdin input with `Read input as name`
- multi-line stdin/context input with `Read multiline input as name`
- structured file input with `Read JSON from "path" as name` and
  `Read CSV from "path" as name`
- constant values introduced with `equals`
- context values introduced indirectly through prior operations

Current practical outputs:
- direct visible output with `Print` and `Show`
- file output with `Write value to "path"`
- explicit overwrite output with `Overwrite value to "path"`
- append output with `Append value to file "path"`
- record/list export with `Export value to "path"`
- CSV table export with `Export value to "path" as CSV`
- validation results
- extracted values saved into context
- routes
- service action results
- notification and messaging outboxes
- bindings

This should be documented as a first-class part of the language model.

### 4. Expand first-class surface forms for output and persistence

There is a strong input primitive:
- `load document`
- `Ask "prompt" as name`
- `Ask multiline "prompt" as name`
- `Read input as name`
- `Read multiline input as name`
- `Read JSON from "path" as name`
- `Read CSV from "path" as name`

There is now a first-pass English-first output primitive:
- `Print value`
- `Show value`
- `Write value to "path"`
- `Overwrite value to "path"`
- `Append value to file "path"`
- `Export value to "path"`
- `Export value to "path" as CSV`

Grammar completion should still add and standardize richer phrases like:
- `save document`
- `store <value> as <name>`

### 5. Add a first-class literal and data model

Right now numbers, booleans, quoted strings, and context-backed names are the
main practical value forms.

The grammar still needs an explicit story for:
- string literals
- numeric literals
- booleans
- missing values
- list values
- record values

Without that, many "basic programming" lessons stay awkward or impossible.

### 6. Normalize the service grammar around real service actions

Some service-oriented English phrases exist, but they are uneven:
- `send email ...`
- `send message ...`
- `notify ...`
- `search the <service> for ...`
- `create <service> entry with ...`

Grammar completion should align these phrases with registered service actions,
or mark them as aspirational.

### 7. Close the current parser/runtime mismatches

The code review surfaced several important mismatches that should be resolved
before the grammar is considered complete:

- `check for <thing>` has a late parser fallback that emits `check_for(...)`,
  but there is no matching runtime method in `DSL::Base`.
- `Then` and `Else` appear in the parser keyword-ignore list, but the current
  English parser uses `If` and `Otherwise` as the real control-flow surface.
- `load ... from <location>` currently lowers to a TODO comment rather than an
  executable operation.
- trigger phrases such as `Every day at 9am:` and `When an Order is created:`
  are parsed as metadata, not executed as runtime behavior.
- extracted values are easy to validate, but less reliable to use in numeric
  control flow because English extraction currently saves string values unless a
  lower-level type coercion step is explicitly introduced.
- same-file extracted-value dataflow now works for basic branching and service
  arguments, but any remaining legacy runtime bridge behavior should keep
  moving into the AST/IR model.

### 8. Prefer verbose canonical phrases over compact aliases

For teaching and long-term grammar stability, favor phrases such as:
- `document must contain`
- `find ... and save as ...`
- `send email via NotificationService to ...`

Then treat shorter variants as optional aliases later.

### 9. Add curriculum-driven acceptance examples

Before adding more grammar, the language should be able to teach itself through
small runnable lessons:
- read input
- check text
- extract data
- make a decision
- route an item
- send a notification
- bind a name

Those lessons should double as grammar acceptance tests and documentation.
