# Audit and harness verification in CI

The `Audit and harness verification` workflow runs on pull requests and manual
dispatch. Its `Linux audit and harness` job tests the independent verifier, native
core/harness, shared VM and model adapter on GitHub's Ubuntu 24.04 runner. Tests use
synthetic keys, data and local adapters. It runs strict audit lint, native lint
with the existing warning baseline, and the wasm32 browser compiler check.

The workflow pins Rust 1.96.0, uses each crate's committed Cargo lockfile with
`--locked`, and pins `actions/checkout` v7.0.1 to its complete upstream commit.
Checkout does not persist credentials. The token has only repository contents
read permission; no signing keys, provider credentials, deployment environment,
OIDC token permission, artifact publication or release permissions are requested.
Pull request code runs through `pull_request`, not a privileged target workflow.
These choices follow [GitHub's secure workflow guidance](https://docs.github.com/en/actions/reference/security/secure-use)
and [checkout's credential settings](https://github.com/actions/checkout).

CI sets `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0`. Verified
session fixtures hash and admit their actual runtime and test executables;
Linux debug sections can push those files above the verifier's 64 MiB artifact
limit. Disabling debug information keeps these test artifacts within that limit
without changing verification or disabling assertions. Use the same environment
variables for local test builds if the fixture's size preflight rejects them.

A green job establishes that these tests passed for the checked-out revision in
that runner. It is not a Devlish builder statement, a release approval, a signature
on a compiler or policy, or proof of actual production enforcement. The hosted
runner image and its preinstalled bootstrap tools remain external dependencies;
`ubuntu-24.04` is a maintained image label, not an immutable image digest. Pull
request workflows are candidate code and can be changed by a proposal. Independent
review and protected release workflows are still needed for release authority.
No branch protection or repository approval settings are changed by this file.

The job does not install or run the [Linux executor service](../deploy/linux/README.md),
qualify its OS isolation matrix, or test hostile-root resistance. It provides a
repeatable Linux check for the current in-process implementation. Before relying
on a result, inspect the actual run and commit, including any failed or skipped
steps. The first hosted result must be observed before Linux success is claimed.
