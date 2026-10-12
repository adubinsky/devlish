# Governed JEV document checks

`check.dvl` executes a typed document batch through Devlish's existing model
effect. `policy.dvl` allows only a fixed synthetic document, exact questions and
an explicitly approved response. It denies checklist changes. `catalog.json`
pins the JEV model and transport limits. `permissions.json` caps execution at
one model attempt and one response.

Run the real program through the shared governed runtime with a recorded
provider reply, real permission/policy/budget checks and a fake network adapter:

```bash
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --test jev_checklist
```

The verified CLI admission tests bind this program, policy and route into a
signed synthetic release and prove changed catalog bytes are rejected before
dispatch. Run them without credentials:

```bash
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --test verified_run jev
```

An ignored integration test makes the actual API call through this compiled
DVL program and the verified CLI. It requires `JEV_API_KEY` in the operator
environment. It uses an ephemeral test signing key and test trust, not production
release approval:

```bash
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test --locked --manifest-path crates/devlish_core/Cargo.toml --test verified_run \
  signed_jev_program_runs_the_live_synthetic_batch -- --ignored
```

Production must use the existing operator approval/signing workflow and protected
profile/credentials. See [JEV integration](../../docs/JEV_PROVIDER.md) and
[policy provenance](../../docs/POLICY_PROVENANCE.md). Raw replay evidence is
operator opt-in; ordinary logs retain only commitments and decisions.
