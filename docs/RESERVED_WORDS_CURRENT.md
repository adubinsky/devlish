# Devlish Reserved Words (Current)

Last updated: 2026-07-10
Status: Current implementation reference.

This document is the canonical reference for Devlish reserved words as they
exist in the codebase today.

Source of truth:
- `crates/devlish_core/src/lib.rs` (Rust parser)

Important design note:
- this file is about reserved words, not the whole language model
- reserved words are not the same thing as commands
- English control-flow words such as `if` and `otherwise` should not be treated
  as standard-library operations
- author-facing Devlish should remain an English-like surface language

For the current language-layer split, see `docs/STANDARD_LIBRARY_CURRENT.md`.

Devlish currently has two separate reserved-word systems:

1. Semantic reserved vocabulary
These are the normalized built-in terms returned by
`Devlish::Parser::ReservedWords.all_terms`. They drive type inference,
pattern inference, comparison semantics, quantifier semantics, and other
language behavior.

2. Parser keyword-ignore vocabulary
These are the words and phrases hard-coded in
`EnglishParser#reserved_keyword?`. They are used to suppress false
"undefined term" errors during term extraction. Some of them are syntax
keywords, and some are common domain nouns.

These two lists overlap, but they are not identical.

## Semantic Reserved Vocabulary

### Time Units

| Term | Meaning |
| --- | --- |
| `second` | Time unit for seconds. |
| `minute` | Time unit for minutes. |
| `hour` | Time unit for hours. |
| `day` | Time unit for days. |
| `week` | Time unit for weeks. |
| `month` | Time unit for months. |
| `year` | Time unit for years. |
| `business_day` | Normalized internal token for "business day". |

### Money and Currency Terms

| Term | Meaning |
| --- | --- |
| `dollar` | Currency term for US dollars. |
| `euro` | Currency term for euros. |
| `pound` | Currency term for pounds sterling. |
| `yen` | Currency term for yen. |
| `usd` | Currency term for values explicitly labeled USD. |
| `amount` | Generic currency-like amount term. |
| `price` | Generic price or monetary value term. |
| `cost` | Generic cost term. |
| `fee` | Generic fee term. |
| `payment` | Generic payment amount term. |
| `value` | Generic monetary or numeric value term. |

### Percentage Terms

| Term | Meaning |
| --- | --- |
| `percent` | Percentage term. |
| `percentage` | Percentage term. |
| `rate` | Generic rate term, inferred as percentage-like. |

### Numeric Terms

| Term | Meaning |
| --- | --- |
| `number` | Generic integer-like number term. |
| `count` | Counting quantity term. |
| `quantity` | Quantity term. |
| `total` | Aggregate numeric total. |
| `sum` | Aggregate numeric sum. |

### Date and Time Pattern Terms

| Term | Meaning |
| --- | --- |
| `date` | Date-like value term. |
| `datetime` | Date-and-time value term. |
| `timestamp` | Timestamp-like value term. |

### Action Verbs

| Term | Meaning |
| --- | --- |
| `check` | Check or verification action. |
| `require` | Requirement action. |
| `must` | Required-modality marker. |
| `may` | Optional-modality marker. |
| `find` | Extraction or lookup action. |
| `extract` | Extraction action. |
| `parse` | Parsing or interpretation action. |
| `save` | Store or retain action. |
| `set` | Assignment or establishment action. |
| `compare` | Comparison action. |
| `validate` | Validation action. |
| `calculate` | Calculation action. |
| `if` | Conditional marker. |
| `unless` | Negative conditional marker. |
| `when` | Conditional or event marker. |

### Comparison Terms

| Term | Meaning |
| --- | --- |
| `greater than` | Greater-than comparison. |
| `more than` | Greater-than comparison. |
| `above` | Greater-than comparison. |
| `over` | Greater-than comparison. |
| `exceeds` | Greater-than comparison. |
| `less than` | Less-than comparison. |
| `fewer than` | Less-than comparison. |
| `below` | Less-than comparison. |
| `under` | Less-than comparison. |
| `at least` | Greater-than-or-equal comparison. |
| `minimum` | Minimum-threshold comparison. |
| `no less than` | Greater-than-or-equal comparison. |
| `at most` | Less-than-or-equal comparison. |
| `maximum` | Maximum-threshold comparison. |
| `no more than` | Less-than-or-equal comparison. |
| `equals` | Equality comparison. |
| `is` | Equality comparison in semantic vocabulary. |
| `matches` | Equality or matching comparison. |
| `between` | Range comparison. |

### Logical Connectors

| Term | Meaning |
| --- | --- |
| `and` | Logical conjunction. |
| `or` | Logical disjunction. |
| `not` | Logical negation. |
| `but` | Contrastive connector. |
| `except` | Exclusion connector. |

### Quantifiers and Articles

| Term | Meaning |
| --- | --- |
| `all` | Universal quantifier. |
| `every` | Universal quantifier. |
| `each` | Universal quantifier. |
| `any` | Existential or permissive quantifier. |
| `some` | Existential quantifier. |
| `a` | Indefinite article used as an introducing quantifier. |
| `an` | Indefinite article used as an introducing quantifier. |
| `the` | Definite article used as a referencing marker. |
| `no` | Empty-set or none quantifier. |
| `none` | Empty-set or none quantifier. |

### Severity Levels

| Term | Meaning |
| --- | --- |
| `critical` | Highest severity label. |
| `high` | High severity label. |
| `medium` | Medium severity label. |
| `low` | Low severity label. |
| `info` | Informational severity label. |

### Status Words

| Term | Meaning |
| --- | --- |
| `valid` | Valid-state marker. |
| `invalid` | Invalid-state marker. |
| `present` | Present or exists-state marker. |
| `absent` | Absent-state marker. |
| `missing` | Missing-state marker. |
| `required` | Mandatory-state marker. |
| `optional` | Optional-state marker. |
| `forbidden` | Prohibited-state marker. |

## Parser Keyword-Ignore Vocabulary

These are the tokens the English parser treats as reserved when deciding
whether a capitalized word or phrase should be reported as an undefined term.

### Surface Grammar and Control-Flow Words

| Term | Meaning |
| --- | --- |
| `Every` | Trigger keyword. |
| `When` | Trigger or condition keyword. |
| `If` | Conditional keyword. |
| `Then` | Flow-control keyword. |
| `Else` | Alternative-branch keyword. |
| `And` | Logical connector keyword. |
| `Or` | Logical connector keyword. |
| `Not` | Negation keyword. |
| `Otherwise` | Else-branch keyword in English mode. |
| `For` | Loop-introducer keyword. |
| `Each` | Loop quantifier keyword. |

### Command and Action Words

| Term | Meaning |
| --- | --- |
| `Load` | Load statement keyword. |
| `Find` | Extraction statement keyword. |
| `Extract` | Extraction statement keyword. |
| `Save` | Storage keyword. |
| `Check` | Requirement or validation keyword. |
| `Validate` | Validation keyword. |
| `Search` | Service-search keyword. |
| `Create` | Service-entry creation keyword. |
| `Route` | Routing keyword. |
| `Copy` | Filesystem copy keyword. |
| `Move` | Filesystem move keyword. |
| `Delete` | Filesystem delete keyword or HTTP verb keyword. |
| `List` | Filesystem list keyword. |
| `Get` | HTTP verb keyword or filesystem stat keyword. |
| `Post` | HTTP verb keyword. |
| `Put` | HTTP verb keyword. |
| `Download` | HTTP download keyword. |
| `Respond` | Structured output keyword. |
| `Permissions` | Program manifest section header. |
| `Boundaries` | Program manifest section header. |
| `Callers` | Program manifest section header. |
| `Alias` | Binding keyword for aliasing one name to another. |
| `Nickname` | Binding keyword for nickname bindings. |
| `Symbol` | Binding keyword for symbolic bindings. |
| `Handle` | Binding keyword for handle bindings. |

### Built-In Nouns and Common Domain Nouns

| Term | Meaning |
| --- | --- |
| `Document` | Built-in document noun. |
| `Email` | Email action noun or verb. |
| `Notification` | Notification noun. |
| `Message` | Message-action noun. |
| `Messaging` | Messaging-service adjective or noun. |
| `Agent` | Service or automation actor noun. |
| `Invoice` | Common business document noun. |
| `Order` | Common business transaction noun. |
| `User` | Common application domain noun. |
| `Customer` | Common customer-domain noun. |
| `Vendor` | Common supplier-domain noun. |
| `Payment` | Common payment-domain noun. |
| `Record` | Generic record noun. |
| `File` | Generic file noun. |
| `Report` | Generic report noun. |
| `Data` | Generic data noun. |
| `Item` | Generic item noun. |
| `Line` | Generic line-item noun. |

### Requirement and Preposition Terms

| Term | Meaning |
| --- | --- |
| `Must` | Required-modality keyword. |
| `Should` | Advisory-modality keyword. |
| `Have` | Requirement verb. |
| `Has` | Requirement verb. |
| `Contains` | Requirement verb. |
| `Include` | Requirement verb. |
| `From` | Source preposition keyword. |
| `To` | Destination preposition keyword. |
| `As` | Alias or interpretation preposition keyword. |
| `With` | Attachment or argument preposition keyword. |
| `Using` | Parameter or argument-list keyword. |
| `Via` | Service channel keyword. |
| `Template` | Template argument keyword. |
| `Entry` | Service-entry noun used in create-entry grammar. |

### Articles, Quantifiers, and Time Words

| Term | Meaning |
| --- | --- |
| `All` | Universal quantifier keyword. |
| `The` | Definite article keyword. |
| `A` | Indefinite article keyword. |
| `An` | Indefinite article keyword. |
| `Day` | Time unit keyword. |
| `Week` | Time unit keyword. |
| `Month` | Time unit keyword. |
| `Year` | Time unit keyword. |
| `Hour` | Time unit keyword. |
| `Minute` | Time unit keyword. |
| `Monday` | Weekly trigger day name. |
| `Tuesday` | Weekly trigger day name. |
| `Wednesday` | Weekly trigger day name. |
| `Thursday` | Weekly trigger day name. |
| `Friday` | Weekly trigger day name. |
| `Saturday` | Weekly trigger day name. |
| `Sunday` | Weekly trigger day name. |

### HTTP Verbs

| Term | Meaning |
| --- | --- |
| `Get the url at` | HTTP GET request keyword. |
| `Post to` | HTTP POST request keyword. |
| `Put to` | HTTP PUT request keyword. |
| `Delete the url at` | HTTP DELETE request keyword. |

### Structured Output

| Term | Meaning |
| --- | --- |
| `Respond with` | Return structured JSON data and exit successfully. |

### Communication and Service Words

| Term | Meaning |
| --- | --- |
| `Send` | Generic send-action keyword. |
| `Notify` | Notification-action keyword. |
| `Send Email` | Phrase-level reserved keyword for email actions. |

### Generic Guardrail Words Ignored During Term Validation

| Term | Meaning |
| --- | --- |
| `New` | Common state adjective. |
| `Old` | Common state adjective. |
| `Any` | Common quantifier also ignored by term validation. |
| `Requirements` | Generic requirement noun. |
| `Fails` | Generic failure verb or state word. |
| `Passes` | Generic success verb or state word. |

## Important Distinctions

- `ReservedWords.all_terms` is the canonical semantic vocabulary.
- `EnglishParser#reserved_keyword?` is a practical parser guardrail list.
- Synonyms listed in `reserved_words.rb` are not automatically primary reserved
  terms unless they also appear in `all_terms` or the parser keyword list.

Examples:
- `verify` is a documented synonym for `check`, but `verify` is not itself in
  `ReservedWords.all_terms`.
- `search` is a documented synonym for `find`, but `search` is not itself in
  `ReservedWords.all_terms`, even though `Search` is in the parser keyword list.
- `business day` is the human phrase, but the semantic reserved token is
  `business_day`.

## Related Documents

- `docs/STANDARD_LIBRARY_CURRENT.md` - current language-layer and standard-library split
- `docs/LANGUAGE_REFERENCE.md` - current authoring guide
- `docs/LANGUAGE_GRAMMAR.ebnf` - parser-faithful grammar
- `docs/RESERVED_WORDS.md` - current pointer document
- `docs/RESERVED_WORDS.old.md` - archived old reserved-words document
