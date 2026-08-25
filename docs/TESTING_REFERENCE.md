# Devlish Testing Reference

Last updated: 2026-03-23
Status: Current reference for Devlish-native tests.

Devlish tests let users validate Devlish programs in near-English, without
dropping into Ruby.

## File Type

Use `.dvt` files for Devlish-native tests.

Example:

```text
Scenario "routes approved reviews"
When I run "../03_branch_and_route.dvl"
Then run should succeed
Then review_status should equal "approved"
Then the route should be "approved_queue"
```

Run with:

```bash
devlish test examples/tutorial/tests/tutorial_curriculum.dvt
```

## Supported Structure

Each test file contains one or more `Scenario` or `Test` blocks.

Supported top-level lines:
- `Scenario "title"`
- `Test "title"`

## Supported Given Steps

Load input for an inline program or provide external fixtures:

```text
Given document from "../data/review_packet.txt"
```

```text
Given this document:
  review status: approved.
  claims queue: claims_review_queue.
```

## Supported When Steps

Run an existing Devlish program file:

```text
When I run "../04_send_email_output.dvl"
```

Run a class-style Devlish file by invoking a method with JSON arguments:

```text
When I run "../01_payroll_calculator.dvl" method "calculate_wages" with [40, 25]
```

Run an inline Devlish program:

```text
When I run:
  Find review status and save as review_status
  If review_status is "approved"
    Route invoice to approved_queue
  Otherwise
    Route invoice to manual_review_queue
```

## Supported Then Steps

Run status:

```text
Then run should succeed
Then run should fail
```

Variable equality:

```text
Then review_status should equal "approved"
Then liability_cap should equal 1500000
Then working_copy should equal document
```

Return-value assertions:

```text
Then return value should equal 1000
Then return value should equal "catch_up"
```

Route assertions:

```text
Then the route should be "approved_queue"
```

Document check assertions:

```text
Then check for "payment terms" should pass
Then check for "governing law" should pass
```

Validation assertions:

```text
Then validation for liability_cap should pass
Then validation for payment_terms should pass
```

Service argument assertions:

```text
Then service "NotificationService" action "send_email" should have to "approvals_team"
Then service "NotificationService" action "send_email" should have template "review_complete"
Then service "MessagingService" action "send_message" should have to "claims_review_queue"
```

## Current Scope

The first test harness is aimed at the Devlish 2.0 tutorial subset:
- loading
- extraction
- validation
- branching
- routing
- service outputs
- binding
- class-style method invocation and return values

It is intended to validate lesson programs and early LLM-authored examples in
Devlish terms.

## Current Limitations

The current harness does not yet support:
- nested scenario groups
- rich collection assertions
- snapshot assertions
- custom matchers
- trigger assertions
- full semantic diffing

Those can be added as the Devlish IR and semantic layers expand.
