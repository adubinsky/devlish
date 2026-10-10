# Governed execution requirement

User workflows enter the admitted Devlish runtime and execute through its VM
and governed host effects. Provider adapters implement those effects. They must
not become standalone executables or independently run a business workflow.

Removed execution surfaces: the arbitrary-command devlish-toolrun crate, JEV
Rust demo, both audit signing/receipt demos and legacy live-model serve smoke
script. Synthetic provider responses now live in test fixtures. The core runtime
and offline audit verifier remain the only native binaries. Existing compiler,
VM, build tools and automated tests remain infrastructure.

AGENTS.md is the mandatory agent instruction. It also requires explicit user
instruction before authoring language files. This cleanup changes no DVL or DVT
source.

## Enforcement

```bash
make install-hooks
make check-boundary
python3 scripts/check_execution_boundary.py --staged
```

The installer respects the configured hooks directory and refuses to overwrite
an unrelated hook. Linked worktrees share that directory. The installed hook
fails closed if a checkout lacks the guard. New clones must install it; Git does
not distribute active hooks automatically.

Pre-commit scans the complete Git index, including manifests and renamed files.
It rejects Rust examples, unapproved main functions, additional Cargo binary
targets, implicit binary/example discovery, custom executable test harnesses,
build scripts and host scripts outside the reviewed infrastructure inventory.
CI performs the same staged-tree check and exercises rejection tests, including
a real refused commit and staged content hidden by an unstaged correction.

The inventory is not permission to put user workflow logic in a tooling file.
Do not expand it to accommodate a prohibited runner. New execution surfaces
require explicit human architectural review and runtime integration. A source
check cannot prove semantic policy compliance. Runtime admission, containment,
policy, permission, budget and recorder tests remain mandatory. Local hooks can
be bypassed with Git options, so CI must pass before landing changes.
