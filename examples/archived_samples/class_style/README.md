# Devlish Class-Style Lessons

Last updated: 2026-03-23
Status: Current class-style curriculum for the Devlish 2.0 class/module surface.

This folder teaches the class-oriented side of Devlish.

Course position:
- Module 2 in [examples/DEVLISH_COURSE.md](/Users/admin/code/devlish/examples/DEVLISH_COURSE.md)

These lessons are not the same shape as the workflow lessons in
`examples/tutorial/`. They are organized like modules, classes, and methods:
- a file starts with `Module's Class Name:`
- each method ends with `:`
- methods can accept parameters with `using ...`
- methods usually finish with `respond with ...`

## How To Use These Lessons

From the project root:

```bash
./bin/devlish parse examples/class_style/01_payroll_calculator.dvl
./bin/devlish trace examples/class_style/01_payroll_calculator.dvl
./bin/devlish parse examples/class_style/02_participant_classifier.dvl
./bin/devlish parse examples/class_style/03_private_helper.dvl
./bin/devlish parse examples/class_style/04_helper_invocation.dvl
```

You can also run them through the current runtime:

```bash
./bin/devlish run examples/class_style/01_payroll_calculator.dvl
```

You can also compile them through the Devlish 2.0 backend:

```bash
./bin/devlish compile examples/class_style/01_payroll_calculator.dvl --target ruby --output tmp/payroll_calculator.rb
./bin/devlish compile examples/class_style/01_payroll_calculator.dvl --target javascript --output tmp/payroll_calculator.js
```

The `run` path still loads the generated class/module definition without
invoking a method automatically. The compiled Ruby and JavaScript outputs
include a small invocation harness you can drive with `DEVLISH_METHOD` and
`DEVLISH_ARGS`.

The current class trace path can also show invocation resolution and method
call flow:

```bash
./bin/devlish trace examples/class_style/04_helper_invocation.dvl --method review_invoice --args '[12000]'
```

## Lesson Sequence

### Lesson 1: Define a calculator method

File:
- `01_payroll_calculator.dvl`

What it teaches:
- module and class declaration
- a method with parameters
- assignment inside a method
- returning a value with `respond with`

### Lesson 2: Classify with guarded assignments

File:
- `02_participant_classifier.dvl`

What it teaches:
- multi-step method bodies
- guarded assignments inside class methods
- returning a classification string

### Lesson 3: Use a private helper

File:
- `03_private_helper.dvl`

What it teaches:
- private method declarations
- separating public API from helper logic
- readable class-style organization

### Lesson 4: Call a private helper

File:
- `04_helper_invocation.dvl`

What it teaches:
- helper calls as expressions
- composing public and private methods
- returning helper-produced values from a public method
