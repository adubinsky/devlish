# Catalog tool effect: policy and replay example

This is the declared-effect foundation for DEVL-187 and DEVL-220. The Rust
compiler and VM support `Run catalog tool request as result`, and the shared
policy host intercepts `run_tool` before dispatch. Tests use a counting fake
adapter. There is no production subprocess launcher in this example.

The built-in native and WASM hosts refuse this effect. `run-verified` and
`serve-verified` also reject catalogs containing `run_tool` during admission,
before creating a run log. A declaration or a passing example does not authorize
an external executable. Do not substitute the development `devlish-toolrun`
command for a verified launcher.

## English program and policy

The [agent](agent.dvl) declares one exact, case-sensitive logical tool ID:

```devlish
Permissions:
  Run catalog tool "public-grep"

Ask "Which catalog request?" as tool_request
Run catalog tool tool_request as tool_result
```

`Run catalog tools` declares all logical IDs, but still grants no authority beyond
the host and policy controls. Unlike legacy effects, `run_tool` requires an
explicit declaration even when a program has no other manifest permissions.
Scoped declarations compare the complete ID; `public` cannot authorize
`public-grep`. The compiler records the `run_tool` import and effect entry.

Requests have exactly two fields:

```json
{
  "tool_id": "public-grep",
  "arguments": ["--fixed-strings", "--", "published", "/work/public.txt"]
}
```

The transport accepts a 1–128 character ASCII ID (letters, digits, `_`, `-`),
up to 64 string arguments, up to 4,096 UTF-8 bytes per argument and 16,384 bytes
in all arguments. NUL bytes are rejected. Shell command strings, executable path,
environment, cwd and stdin fields are not part of this request. Argument strings
are data: the eventual adapter must pass argv directly and independently enforce
the signed catalog's argument/path grammar. Transport bounds alone do not make
arguments safe or identify an authorized binary.

The [policy](policy.dvl) authorizes only the exact public search above. Alternate
IDs, private NPPI or company-IP paths, recursive search, extra arguments and
loader/environment/path overrides are rejected. The agent validates the returned
exit status and emits only a fixed acknowledgement. Raw stdout/stderr never become
a model prompt or user response in this example. Even an altered program that
tries to respond with stdout is denied by the output policy.

## Recording and repeatable tests

```bash
cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --test catalog_tool
```

On Linux debug builds, use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`
if test executables exceed the application-report artifact limit.

Tests exercise request rejection before any fake invocation, exact declarations,
permission intersection, effect budgets, recorder failure before/after dispatch,
nonzero exit status, host failure without retry, output denial, inline records,
quoted delimiters and module symbol isolation. Policy golden cases are in
[policy.cases.json](policy.cases.json). A successful synthetic run generates
application, policy and process reports; two offline process replays must return
identical reports and never call a real tool.

Normal governed logs contain request/result commitments. Raw replay evidence is
an explicit operator opt-in and must be treated as sensitive. A successful host
outcome means the host returned a value, not that a child exited successfully;
the returned exit status remains program data that the Devlish agent checks.
Recorded effects with missing outcomes must not be retried automatically.

## Remaining launcher boundary

Before enabling this effect in verified native admission, implement an
operator-controlled signed catalog, exact executable and dependency identities,
verification bound to the executed object, platform-qualified containment,
process/time/output limits, descriptor and credential isolation, and governed
output handling. Report the actual checked executable/containment identities in
the outcome. Platform tests must cover substitution, check/exec races, symlink
swaps, loader overrides and disclosure. Signed logs and replay still cannot prove
that a compromised runtime performed the claimed execution.

The [Linux sealed snapshot primitive](../../docs/SEALED_TOOL_SNAPSHOTS.md) now
provides byte retention for a future launcher. It does not execute the snapshot
or satisfy the remaining catalog, loader or containment requirements.
