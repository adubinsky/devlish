# Capability packages

Last updated: 2026-10-19
Status: Architecture specification. Not implemented language behavior.
Related: [Server harness architecture](SERVER_HARNESS_ARCHITECTURE.md),
[Language gaps §8](DEVLISH_LANGUAGE_GAPS.md), [Effect policy](EFFECT_POLICY.md),
[Execution boundary](EXECUTION_BOUNDARY.md), [Standard library](STANDARD_LIBRARY_CURRENT.md).

## Why this exists

Bundled `Use` / `Import` and runnable `package` artifacts exist today. There is
no installable module ecosystem with a package boundary separate from grammar
([DEVLISH_LANGUAGE_GAPS.md](DEVLISH_LANGUAGE_GAPS.md) §8).

Python and Ruby gems assume human authors and human review. A **server-first
harness** runs agent-authored application code. Open holes (raw HTTP clients,
DB drivers, unsupervised LLM tool loops) must not be reachable from app source
without an admitted wrapper. Capability packages are that systemic guardrail.

Infrastructure remains Rust crates (`devlish_llm`, future DB adapters). User
and agent workflow code never bypasses admission, policy, permissions, effect
budgets, or durable records ([AGENTS.md](../AGENTS.md)).

## What a capability package is

A versioned, catalog-admitted (or signed) module that:

1. Exposes a narrow English API (for example `Ask the model…`,
   `Fetch from payments…`, `Query the ledger…`).
2. Declares the **only** host effects it may request (`llm_complete`,
   `http_request` to allowlisted hosts, `service_call` for data, clock,
   randomness, catalog tools).
3. Carries or composes a deny-by-default effect policy and effect budgets.
4. Resolves like a crate/gem by name and version, not by inlining arbitrary
   transport code into the app.
5. Leaves open holes outside its surface: app code cannot reach them without a
   new admitted capability.

Packages extend libraries and runtime adapters. They must not redefine core
language semantics.

## Relationship to what exists

| Existing piece | Role after packages |
| --- | --- |
| `Import "path.dvl"` | Project-local source composition; not a capability boundary |
| `Use` + bundled `stdlib/` | Remains the zero-dependency taught modules; may later be thin facades over capabilities |
| Program `Permissions:` / `Boundaries:` / `Callers:` | App still declares what it needs; capabilities must be a subset of admitted effects |
| Effect policy + `PolicyHost` | Still evaluates every tool effect; capability policies compose under the app/operator policy |
| Effect budgets | Still hard caps; packages may tighten but not raise operator budgets |
| Runnable bytecode package | Deploy unit embeds a **locked capability set** (see lockfile) |
| Signed catalog / verified admission | Capabilities are admitted the same way model routes and releases are |

## Package layout (normative sketch)

A capability package directory (exact on-disk names are implementation detail):

```text
payments-1.2.0/
  package.toml          # name, version, effects, digests
  api.dvl               # English surface modules
  policy.dvl            # deny-by-default rules for this capability
  budgets.json          # optional tighter limits
  README.md             # human contract
```

`package.toml` fields (conceptual):

- `name`, `version`
- `effects`: list of allowed host effect kinds and constraints
  (hosts, methods, models, services)
- `api_modules`: exported module names resolvable via `Use`
- `policy_sha256`, `api_sha256`: pinned digests for admission
- `requires`: other capability packages (lockfile resolves the graph)

Language surface target (discuss before implementing; not grammar today):

```text
Use the ledger module from payments 1.2.0.
```

Until that syntax exists, architecture requires resolving capabilities through
the project lockfile and compile-time admission, not ad-hoc path imports of
adapter code.

## Lockfile and admission

An application project carries:

- `devlish.toml` — project root, search paths, default authorization posture
- `devlish.lock` — exact capability names, versions, and content digests

Admission rules:

1. Compile and verified serve refuse packages not listed in the lockfile
   (or not present in the signed catalog for that profile).
2. Capability digests in the lockfile must match bytes loaded at run time.
3. App source that requests an undeclared host effect directly (outside a
   capability’s API) fails closed under a server-harness profile.
4. Changing a capability version is a lockfile + re-admission event, not a
   silent path edit.

Local `serve` and remote `serve-verified` use the **same** lockfile and
package digests. Only credentials, env, and trust roots differ.

## Priority capability families

### LLM

Tighten `llm_complete` / JEV routes into a package API:

- Model and provider allowlists
- System/instruction packing under policy
- Prompt and response redaction hooks
- Full journaling of requests and outcomes
- No free-form tool loop unless a separate admitted “agent tools” capability
  exists

### HTTP egress

- Host allowlists, method and body budgets, redirect policy
- Builds on existing effect-policy guidance; packages make the allowlist
  portable across apps

### Data / DB

- Declared `service_call` / data-service effects only
- Never embed a free SQL engine or unsupervised driver in the VM
- Query shape and target service named in the capability contract

### Secrets / credentials

- Process environment and verified credential store only
- Never stored in agent-writable application trees
- Capabilities may name which credential keys they consume; they do not
  embed secrets

## What packages must not do

- Redefine parser or core language meaning
- Open raw sockets, shells, or filesystems except through declared effects
- Call provider SDKs from `.dvl` or from standalone Rust mains outside the
  host-effect path
- Ship as a user-workflow executable that bypasses verified admission
- Widen WASM view hosts to privileged egress “for convenience”

## Implementation sequence (spec → code)

1. Document and test the lockfile + digest admission contract against
   `serve-verified` and CLI governed run.
2. Ship first capabilities: LLM and HTTP egress, wrapping existing
   `HostEffects` and `devlish_llm`.
3. Formalize `Use … from <package> <version>` (or equivalent) only after
   owner agreement on language surface; until then, lockfile-bound module
   resolution is sufficient for the architecture.
4. Add data-service capability once a single DB adapter exists behind
   `service_call`.
5. Retire any teaching examples that demonstrate raw privileged effects as
   the recommended app pattern under the server-harness profile.

## Success criteria

- Agent-authored app code can reach LLM, HTTP, and DB only through admitted
  capability packages.
- Lockfile digests match local and remote loads.
- Policy and budget failures journal and fail closed.
- No new alternate entry point that skips admission or records.
