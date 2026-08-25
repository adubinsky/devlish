# Evidence bundles

An **evidence bundle** turns a rule's golden test cases into a compliance
artifact: the document an approver signs off on, and the input the controlled
release workflow requires before a rule version can be published.

## Golden cases

A rule's cases live next to it as `<rule>.cases.json` (or point at any file with
`--cases`). Each case is an input and the exact output the rule must produce:

```json
[
  { "name": "conforming DTI", "input": { "income": 6000, "debt": 1800 }, "expected": true },
  { "name": "over threshold",  "input": { "income": 3000, "debt": 1500 }, "expected": false }
]
```

`expected` is compared against the rule's `Respond with` value.

## Generating the report

```bash
devlish evidence credit_dti.dvl --output evidence.json
```

This compiles the rule once, runs every case against that exact artifact, and
emits a machine-readable report. It exits non-zero if any case fails, so it can
gate a release.

The report records, for the version under test:

- `rule` — the governed `id` and `version` (a `Rule:` section is required)
- `artifact_sha256` — hash of the compiled bytecode the cases ran against
- `compiler_version` — the compiler that produced it
- `generated_at` — Unix timestamp of the run
- `cases[]` — per case: `passed`, plus `input_sha256`, `output_sha256`, and
  `expected_sha256`
- `totals` — total / passed / failed
- `report_sha256` — a SHA-256 over the whole report body (every field except
  `report_sha256` itself), so tampering is detectable

## Verifying a report

The bundle is tamper-evident without any secret:

```bash
devlish evidence --verify evidence.json
```

This removes `report_sha256`, canonically re-serializes the report (sorted keys,
compact), recomputes the digest, and compares. It exits non-zero and reports a
mismatch if the file was altered. Because the case, input, output, and artifact
hashes all feed that digest, changing any of them changes `report_sha256`.

(Use the built-in `--verify` rather than hand-rolling the hash: the canonical
form is sorted-key, whitespace-free JSON with serde number formatting, which
`jq` or `python -m json.tool` will not reproduce byte-for-byte.)

## Role in approval

The evidence bundle is what an approver reviews and signs: it proves that a
named rule version, identified by its artifact hash, passed a specific set of
cases whose inputs and outputs are themselves hashed. The controlled release
workflow (a later step) refuses to publish a version without a passing bundle,
and records the bundle's `report_sha256` alongside the release.
