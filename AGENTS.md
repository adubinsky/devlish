# Mandatory execution boundary

Before proposing or implementing a language feature, read
`docs/LANGUAGE_REFERENCE.md`, `docs/LANGUAGE_GRAMMAR.ebnf`,
`docs/STANDARD_LIBRARY_CURRENT.md` and `docs/DEVLISH_LANGUAGE_GAPS.md`.
Use `docs/DOCUMENTATION.md` to find related references. Check the parser,
runtime and tests against those documents; report stale claims rather than
treating a roadmap or legacy packaging guide as implemented behavior.
Discuss language extensions with the user before implementing them. After an
agreed change, update the authoring reference and grammar together with tests.

User workflows must execute through Devlish's admitted, governed runtime.
Rust, Ruby, JavaScript, Python and shell are infrastructure implementation
languages, never substitute user workflow executors.

Hard ban: do not add standalone runners, Cargo examples, arbitrary command
wrappers, direct-provider workflow demos, self-approved signing demos, or any
alternate user-code entry point that bypasses verified admission, policy,
permissions, effect budgets or durable execution records. Calling a provider
library from a Rust main function is not a deployable Devlish integration.

Model integrations belong in the existing approved host-effect path. The only
native executable targets are devlish-core (runtime) and devlish-audit (offline
verification, never execution). Synthetic adapters and signing helpers belong
in automated tests, not executable examples. Do not weaken the guard, widen its
inventory or add a bypass to make a prohibited implementation pass.

Do not author or change .dvl or .dvt code unless the user explicitly requests
language authoring. Implement infrastructure changes in existing runtime paths.

Run `python3 scripts/check_execution_boundary.py` and
`python3 -m unittest discover -s scripts/tests` before committing. Install the
tracked pre-commit hook with `scripts/install_git_hooks.sh`. CI repeats the
boundary check. This structural check complements runtime security tests; it
does not establish that arbitrary source code is secure.
