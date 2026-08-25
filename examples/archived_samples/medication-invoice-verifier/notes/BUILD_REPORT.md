# Build Report: Medication Invoice Verifier Workspace

## Scope
Create a separate, fresh-install-style working folder that loads the PRD example into Devlish process/definition scripts, then aggressively test what works and what fails.

## Steps Taken
1. Created isolated workspace at `sandbox/medication-invoice-verifier`.
2. Added business term definitions in `app/devlish/definitions/medication_invoice_terms.dvl`.
3. Added two process variants:
   - `verify_medication_invoice_compatible.dvl` for current parser compatibility.
   - `verify_medication_invoice_full_prd.dvl` for full PRD intent.
4. Added service registry file at `app/devlish/services/services.yml`.
5. Added gateway skeleton (`Sinatra` ingress + `Sidekiq` worker):
   - `gateway/app.rb`
   - `gateway/invoice_verification_job.rb`
   - `gateway/config.ru`, `gateway/sidekiq.yml`, `gateway/Gemfile`
6. Added documented load script at `scripts/load_example.sh`.
7. Removed `gateway/verification_pipeline.rb` by request.
8. Ran parser, validator, and runtime-loading checks to identify failures.

## What Worked
- Workspace isolation and file organization aligned with architecture direction.
- PRD translated cleanly into Definitions + Process artifacts.
- Parser-compatible process file keeps to known line patterns (load, find/save, equals, threshold validations, notification statements).
- `./bin/devlish parse` on `verify_medication_invoice_compatible.dvl` generated code end-to-end.
- Parser spec suite passed: `bundle exec rspec spec/devlish/parser/english_parser_spec.rb` (17 examples, 0 failures).

## What Did Not Fully Work (By Design / Known Gaps)
- Full PRD process includes structures not fully supported by current parser:
  - block indentation semantics (`If ...` with nested lines)
  - `Otherwise` branches
  - `For each` loops
  - rich service action forms (`Search the X for ...`, `Create DecisionLog entry with ...`, `Route invoice to ...`)
- Current parser largely relies on line-level regex patterns and does not yet support a structured AST for nested flow control.
- Dot-notation and capitalization-as-type semantics are not first-class in current term normalization.
- `./bin/devlish validate verify_medication_invoice_full_prd.dvl` fails with Ruby syntax errors from unsupported grammar constructs (notably `in`, `Otherwise`, and block control flow).
- Gateway runtime dependencies are not installed in this repo context:
  - requiring `gateway/app.rb` fails on missing `sinatra/base`
  - requiring `gateway/invoice_verification_job.rb` fails on missing `sidekiq`
- Gateway worker intentionally left with a `NotImplementedError` hook until Devlish executor wiring is defined.

## Test Matrix
1. `./bin/devlish parse app/devlish/processes/verify_medication_invoice_compatible.dvl`
   - Result: Pass
   - Notes: Generates deterministic Ruby-style output; no hard parse failures.
2. `./bin/devlish parse app/devlish/processes/verify_medication_invoice_full_prd.dvl`
   - Result: Partial
   - Notes: Several lines become `# Could not parse`; some `If ... is ...` lines are misread as definitions.
3. `./bin/devlish validate app/devlish/processes/verify_medication_invoice_compatible.dvl`
   - Result: Pass with warning
   - Warning: "Potentially unknown methods detected: checks, threshold".
4. `./bin/devlish validate app/devlish/processes/verify_medication_invoice_full_prd.dvl`
   - Result: Fail
   - Error class: Ruby syntax errors in compiled output caused by unsupported control-flow and service phrases.
5. `bundle exec rspec spec/devlish/parser/english_parser_spec.rb`
   - Result: Pass (17/17)
   - Notes: Baseline parser behavior is stable for currently supported grammar.
6. `ruby -e "require_relative './sandbox/medication-invoice-verifier/gateway/app'"`
   - Result: Fail
   - Error: `LoadError` for `sinatra/base`.
7. `ruby -e "require_relative './sandbox/medication-invoice-verifier/gateway/invoice_verification_job'"`
   - Result: Fail
   - Error: `LoadError` for `sidekiq` (and pipeline reference remains stale after deletion).

## Suggested Improvements
1. Parser architecture
   - Move from regex-only line parsing to tokenization + AST with block-aware grammar.
   - Add explicit nodes for `if/else`, `for each`, and action invocations.
2. Term system
   - Preserve original token casing and dot paths (`Payroll.report`, `payroll.report`).
   - Classify terms by lexical form: `PascalCase` as service/type namespace, lowercase as domain namespace.
3. Service action model
   - Define declarative action grammar: `<Verb> <TargetService> [with args]`.
   - Validate action signatures against installed services metadata.
4. Runtime integration
   - Add a stable API to execute named process files with JSON context payload.
   - Return structured decision objects and traces for audit.
   - Refactor gateway worker to call a real Devlish execution interface instead of a local fallback class.
5. Developer UX
   - Add `devlish doctor` command to report unsupported syntax in `.dvl` files.
   - Add `devlish scaffold workspace <name>` for fresh folder creation.
6. Gateway packaging
   - Add a separate bundle context for `gateway/` (`bundle install --gemfile gateway/Gemfile`).
   - Add a smoke test task that verifies gateway boot, job enqueue, and minimal event acceptance.

## Proposed Gateway Direction (Matches Your Suggestion)
A `Sinatra + Sidekiq` service is a strong fit for now:
- Sinatra handles inbound HTTP events simply.
- Sidekiq gives queueing, retries, and throughput control.
- Devlish process execution can be called from the worker once parser/runtime APIs stabilize.
- This pattern is operationally similar to a lightweight node event server.

## Notes
- Parser/validator/spec commands were executed.
- Sinatra/Sidekiq services were not started; runtime checks were require/load probes only.
