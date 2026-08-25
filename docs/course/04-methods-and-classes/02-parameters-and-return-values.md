# Lesson 4.2: Parameters And Return Values

Last updated: 2026-03-23
Status: Current lesson.

## Purpose

Show how a method receives information and gives back a result.

## Learning Goals

- identify a method parameter
- explain what `respond with` returns
- run one method with two different inputs

## Vocabulary

- parameter
- argument
- return value
- respond with

## First Example

```text
HR's Payroll Calculator:
  calculate wages using hours worked and hourly rate:
    wages equals hours worked times hourly rate
    respond with wages
```

Open the example file here:
- [02_payroll_calculator.dvl](/Users/admin/code/devlish/docs/course/04-methods-and-classes/examples/02_payroll_calculator.dvl#L1)

## Run It

```bash
./bin/devlish run docs/course/04-methods-and-classes/examples/02_payroll_calculator.dvl --method calculate_wages --args '[40,25]'
```

## Big Idea

Parameters are the inputs to a method.

In this example:
- `hours worked` is one parameter
- `hourly rate` is another parameter

`respond with wages` means the method returns the final value of `wages`.

## Practice

1. Run the method with `[10,30]`.
2. Predict the new return value before you run it.
