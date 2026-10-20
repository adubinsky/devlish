# Server-first Devlish harness architecture

Last updated: 2026-10-19
Status: Architecture specification. Guiding product shape; not all pieces are
implemented.
Related: [Capability packages](CAPABILITY_PACKAGES.md),
[Serve quick start](SERVE_QUICKSTART.md),
[Bytecode WASM](BYTECODE_WASM_FIRST_DELIVERABLES.md),
[Execution boundary](EXECUTION_BOUNDARY.md),
[Application](APPLICATION.md),
[Native compilation plan](NATIVE_COMPILATION_PLAN.md).

## Intent

Replace the local PC / container / virtual-desktop agent model with a **pure
server harness**:

- Develop against a local server the way Rails does (`devlish serve`).
- An agent authors `.dvl` against that server (compile/lint feedback like an
  LSP loop).
- Push the **same admitted package** to a remote machine with no app rewrite.
- Controllers and models live on the server; views run as WASM in the browser.
- Devlish rules and journals record what the agent and the app were allowed to
  do.
- The hard gap is **capability packages**
  ([CAPABILITY_PACKAGES.md](CAPABILITY_PACKAGES.md))—systemic guardrails around
  LLM, DB, HTTP—not another remote desktop.

This is organizational guardrails for **server-side agent-authored code**, not
personal desktop autonomy.

## Building blocks today

| Piece | Role today | Gap relative to this architecture |
| --- | --- | --- |
| Shared compile/run/lint service | CLI, MCP, HTTP | Keep as the single service core |
| Legacy `serve` | Loopback JSON; harness session routes alias `/v1/run` | Not durable; no executable effect policy on legacy path |
| `serve-verified` | Signed admission, token, policy logs, governed `/v1/run` | Promote to default local and remote host; add durable sessions |
| CLI harness sessions | Durable under `~/.devlish/sessions/` | Lift to HTTP with the same semantics |
| Bytecode + Rust VM | Only compile target | Deploy unit for controllers/models (v1); AOT later |
| WASM runner / `devlish-runtime` | Bytecode-in-WASM; HTTP/FS rejected | Product rule: views only; no privileged effects |
| Permissions, policy, budgets, audit | Native governed path | Compose under capability packages |
| `Import` / `Use` / thin stdlib | Composition without package ecosystem | Capability packages + lockfile |
| VS Code extension + CLI lint | No LSP server | Agent loop uses service first; LSP optional later |

Honest translation of “compile controllers to Rust”: the toolchain and host are
Rust; apps ship as **admitted bytecode packages** loaded by that host. True
`.dvl` → native object code remains [NATIVE_COMPILATION_PLAN.md](NATIVE_COMPILATION_PLAN.md)
Option D and is not a prerequisite for the server harness.

## Target shape

```text
Local (Rails-like)                  Admitted app package
─────────────────                   ────────────────────
Agent/human ──► compile/lint ──►    controllers/models (bytecode)
                devlish serve       capability lock + policies
                     │              view modules (WASM assets)
                     │
                     ▼
              same package digests
                     │
                     ▼
Remote serve-verified ──► Rust host adapters ──► journals / audit
                     │
                     ▼
Browser: WASM view runtime (no privileged effects) ◄── HTTP/JSON from server
```

### Controllers and models (server)

Devlish programs admitted into `serve-verified` request effects only through
capability packages. Host adapters in Rust implement those effects. Policy,
budgets, and journals wrap every call.

### Views (browser)

View logic ships as constrained WASM (reuse the bytecode-in-WASM runner).
Privileged effects are **not** available in the browser host. Views call the
server. Today’s WASM hard-rejects for HTTP/FS become a product rule: **views
are sandboxed; the server is the trust boundary**.

### Local → remote

Same package digests, same capability declarations, same policy versions.
Differences are environment, credentials, and admission trust roots—not
application structure.

---

## 1. Capability package model

See [CAPABILITY_PACKAGES.md](CAPABILITY_PACKAGES.md) for the full contract:
package layout, lockfile, admission, priority families (LLM, HTTP, data,
secrets), and non-goals.

**Summary:** extend `Use` / `Import` and runnable packages into versioned,
digest-pinned capabilities that wrap open holes. App code cannot call raw LLM,
DB, or HTTP outside an admitted capability under the server-harness profile.

---

## 2. Verified serve and durable HTTP sessions

### Elevate `serve-verified`

| Concern | Spec |
| --- | --- |
| Default host | Local development and remote production both use the verified admission path. Legacy serve without executable effect policy is compatibility-only, not the product story. |
| Rails-like local loop | `devlish serve` (verified profile) loads a project root, serves HTTP for app routes and agent tooling, writes journals under a project or configured log directory. |
| Remote | Same binary and package; different bind address, trust roots, and credentials. |
| Auth | Verified credential / token model already used by `serve-verified`; no anonymous privileged run. |

### Durable sessions (fill the documented gap)

Today `POST /v1/harness/sessions` returns an execution result; durable session
management is separate work ([SERVE_QUICKSTART.md](SERVE_QUICKSTART.md)).

Target HTTP session contract (aligned with CLI harness):

1. `POST /v1/harness/sessions` creates a session artifact (id, paths, policy
   digests, transcript skeleton) and may run an initial turn.
2. `POST /v1/harness/sessions/:id/resume` continues from durable state, not a
   fresh `/v1/run` alias.
3. Session directory holds transcript, policy log references, and capability
   lock digests used for the run.
4. Local CLI `harness run` / `resume` and HTTP sessions share one session
   schema so push/deploy does not change resume semantics.
5. Interrupted recovery remains governed (see existing recovery tickets); no
   silent fallback that skips policy.

Journals and policy logs are first-class session artifacts and feed
`devlish-audit` / compliance reports on both local and remote hosts.

---

## 3. Server controllers vs WASM views

| Layer | Runs where | May request | Must not |
| --- | --- | --- | --- |
| Controllers / models | Native VM on server | Effects via admitted capabilities | Bypass admission; hold long-lived secrets in source |
| Views | Browser WASM (`devlish-runtime`) | Pure compute, UI state, calls to server HTTP | `http_request`, filesystem, LLM, DB, credentials |
| UI shell | Ordinary web assets served by the host | Render WASM output; talk to server APIs | Embed provider keys or open privileged side channels |

Deployment:

- App package includes bytecode for server entrypoints and WASM (or
  bytecode-for-WASM) assets for views.
- Server routes: application JSON/HTML plus static/WASM view assets.
- View modules that declare privileged permissions fail at package build or
  load time—same spirit as today’s WASM runner rejects.

This is not “compile Devlish to a DOM framework.” It is **sandboxed Devlish
view logic** plus a thin UI shell, with all trust on the server.

---

## 4. Agent authoring loop (LSP-shaped, LSP optional)

### v1 (no new language server required)

1. Project opens against local verified `serve`.
2. Agent or editor calls existing service endpoints / CLI:
   `compile`, `lint --json`, governed `run`, project-scoped `harness generate`
   under authoring policy.
3. Every generate and run is policy-bound and journaled (generalize
   [`harness generate`](../crates/devlish_core/src/harness.rs) to project
   scope).
4. Capability uses are validated against `devlish.lock` at compile time.
5. VS Code extension continues to shell out to `devlish-core` until an LSP
   exists.

### Later (optional LSP)

A language server that speaks the same service contracts (diagnostics,
compile, symbol info) without becoming an alternate execution entry point.
Execution still goes through admitted run / serve-verified.

### Non-goals for the authoring loop

- Desktop-style unrestricted shell/edit loops as the primary product
- Authoring that skips policy logs
- Language changes without owner discussion ([AGENTS.md](../AGENTS.md))

---

## 5. Deploy unit and remote push

An admitted app package contains:

- Controller/model bytecode
- View WASM assets
- Locked capability set (`devlish.lock` digests)
- Policy and budget digests
- Release / catalog metadata required by verified profiles

Push = sync package + reload or restart remote `serve-verified`. No rewrite of
routes or capability wiring for environment promotion.

---

## 6. What not to build

- Remote virtual desktops or “cloud containers that behave like a laptop agent”
- Standalone Rust/Python demos that call providers outside admitted effects
- Granting WASM views HTTP/DB/LLM permissions for convenience
- Treating IDE desktop-agent feature parity as the product goal without
  rehoming each item behind server sessions and capability packages

---

## 7. Phased delivery

1. Capability package model + lockfile admission (LLM and HTTP first)
2. Durable verified HTTP sessions; `serve-verified` as local/remote default
3. View split on existing WASM runtime
4. Project agent compile/lint/generate under policy; LSP later if needed
5. Remote push of identical packages
6. Optional AOT for hot controllers
   ([NATIVE_COMPILATION_PLAN.md](NATIVE_COMPILATION_PLAN.md) Option D)

---

## 8. DEVL-231 remapping (server capabilities, not desktop parity)

Epic [DEVL-231](https://cloudze.atlassian.net/browse/DEVL-231) listed common
LLM harness features as desktop-shaped gaps. Under this architecture they are
**server session + capability package** work. Coding specialties that assume a
local personal workspace move behind declared server tools or stay out of
scope.

| Ticket | Summary | Server-first home | Disposition |
| --- | --- | --- | --- |
| DEVL-232 | Provider / model routing with fallbacks | LLM capability package | Keep — package contract |
| DEVL-233 | System prompt and instruction packing | LLM capability | Keep — package + policy |
| DEVL-234 | Durable message / turn transcript | Durable HTTP sessions | Keep — session schema |
| DEVL-235 | Governed agent loop | Server session + tool capability | Keep — no free desktop loop |
| DEVL-236 | Streaming tokens / partial tool calls | Verified serve events | Keep — journalable streams |
| DEVL-237 | Stop conditions | Session + effect budgets | Keep |
| DEVL-238 | Context assembly (files, git, rules) | Declared server tools capability | Narrow — only admitted tools |
| DEVL-239 | Context window management | LLM capability / session | Keep |
| DEVL-240 | Prompt caching | LLM capability | Keep |
| DEVL-241 | Short-term scratchpad / todos | Session artifact | Keep |
| DEVL-242 | Long-term / cross-session memory | Server store behind capability | Keep — not browser-local |
| DEVL-243 | RAG / retrieval | Retrieval capability + policy | Keep |
| DEVL-244 | Tool/function calling schema | Tool capability package | Keep |
| DEVL-245 | Built-in tools (shell, files, …) | Declared catalog/local tools only | Narrow — no open shell |
| DEVL-246 | MCP / plugin tool servers | Optional bridge behind admission | Narrow — not unconstrained MCP host |
| DEVL-247 | Structured output / constrained decode | LLM capability | Keep |
| DEVL-248 | Subagents / handoffs | Server session orchestration | Keep — governed only |
| DEVL-249 | Parallel tool calls | Tool capability | Keep |
| DEVL-250 | Permissions / allowlists | Capabilities + policy | Keep — core |
| DEVL-251 | Human-in-the-loop approvals | Server session gates | Keep |
| DEVL-252 | Sandboxing (worktrees, containers) | Server isolation profiles | Reframe — server sandbox, not VDI |
| DEVL-253 | Policy / guardrails / redaction | Policy + LLM/HTTP capabilities | Keep — core |
| DEVL-254 | Audit journal | Journals + `devlish-audit` | Keep — core |
| DEVL-255 | Session persistence and resume | Durable HTTP sessions | Keep — promote over CLI-only |
| DEVL-256 | Checkpoints / rollback | Server session + package snapshots | Keep |
| DEVL-257 | Plan vs agent modes | Server session modes | Keep |
| DEVL-258 | Interactive clarification | Server session UX / API | Keep |
| DEVL-259 | Diff review / apply | Authoring loop on server project | Narrow — project files under policy |
| DEVL-260 | Retries and backoff | LLM/HTTP capability | Keep |
| DEVL-261 | Malformed-tool / JSON recovery | LLM + tool capabilities | Keep |
| DEVL-262 | Token / cost / effect budgets | Budgets + packages | Keep — core |
| DEVL-263 | Tracing / observability / replay | Journals + reports | Keep |
| DEVL-264 | Evals / golden trajectories | Server harness test suite | Keep |
| DEVL-265 | Repo map / workspace index | Declared project tool | Narrow or defer |
| DEVL-266 | Edit engines | Authoring under policy | Narrow — not IDE patch UX as product |
| DEVL-267 | Terminal integration | Catalog tool / deny by default | Narrow or out of scope |
| DEVL-268 | Git awareness | Declared tool capability | Narrow or defer |
| DEVL-269 | Rules / skills / project instructions | Project pack loaded by serve | Keep — server project policy |
| DEVL-270 | Image / multimodal input | LLM capability | Keep |

**Out of product scope as desktop-agent features:** unconstrained shell, local
git commit autonomy, IDE edit engines, and MCP as an open plugin host. Those
either become tightly admitted server tools or remain non-goals.

Related foundation epic: DEVL-208 (Outbound LLM Harness) remains the shipped
base (providers, `llm_complete`, CLI sessions). DEVL-231 tracks parity beyond
that base under this server-first remapping.

---

## Success criteria

- Same app package runs under local verified serve and remote `serve-verified`
  without structural change.
- Agent-authored code can only touch the world through admitted capability
  packages; raw LLM/DB/HTTP holes are closed by default.
- Every agent and app effect leaves a durable, auditable journal.
- Browser views never hold privileged credentials or egress.
- No new user-workflow entry point that bypasses admission, policy,
  permissions, budgets, or records.
