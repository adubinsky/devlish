# Linux executor isolation qualification candidate

Status: deployment recipe only, not a qualified platform or evidence of applied
containment. No service was installed or started while authoring this recipe.
The development host is macOS; Linux/systemd execution tests remain outstanding.
Target a dedicated Linux host with systemd 255 or newer and the kernel features
required by the unit. Unknown, ignored or unavailable restrictions fail qualification.

`devlish-verified.service` starts the existing authenticated loopback adapter as
`devlish-executor`. Devlish still owns workflow and policy; systemd supplies the
OS identity, filesystem restrictions, process limits and credential mounting.
Use this with a release approved for the host's exact target, not a developer
checkout or the `bin/devlish` release/debug/PATH fallback wrapper.

## Trust and provisioning

An independently trusted administrator must provision the following. Do this on
a disposable qualification host first. This repository does not perform those
privileged installation operations or create production secrets automatically.

| Path or identity | Required custody |
| --- | --- |
| `devlish-executor` | Dedicated non-login account; never shared with clients, model workers, editors, build jobs or unrelated services |
| Unit and every drop-in | Root-owned, not writable by the executor or client; inspect the merged effective configuration |
| `/opt/devlish/runtime/devlish-core` | Independently approved exact runtime, root-owned with protected ancestors and loader/dependencies |
| `/opt/devlish/deployment` | Root-owned release artifacts, profile, trust and requirements; executor may read but cannot modify them |
| `/etc/devlish/secrets` | Root-only directory and credential source files; no ACL grants to clients or executor |
| `/var/lib/devlish/admission.json` | Explicitly provisioned durable rollback state, private and executor-writable; never reset automatically |
| `/var/log/devlish` | Private executor-owned audit logs; export signed checkpoints/receipts to independent storage separately |

Verify the runtime and audit-verifier distributions independently before trusting
their results. A candidate runtime's successful self-check cannot authenticate
itself. The service invokes an absolute installed executable, but the unit does
not add a trusted atomic hash-and-exec launcher or verify the loader and libraries.
Those bootstrap and supply-chain controls remain DEVL-119/DEVL-220 work.

Configure the root-controlled profile to use
`/var/lib/devlish/admission.json`, approved runtime/program/policy/catalog/permissions
and containment artifacts, explicit external trust roots, and operator requirements.
Provision the admission file using the existing `init-admission` command exactly
once, under the intended state custody. Preserve it across upgrades, restarts and
credential rotation. Missing or corrupt state must stop admission; deleting it is
not recovery. Select `require_recorded_controls: true` for release-bound audits.

The unit's current runtime containment artifact still describes the existing
in-process interpreter profile. This recipe does not make the independent verifier
certify systemd settings or change `execution_origin_verified` to true.

## Credentials and process boundary

Provision `serve-token` as a random 32-byte token encoded as 64 hex characters,
and `openrouter-key` through the operator's secret-management process. No real
values belong in Git, unit files, command arguments, test fixtures or logs.
`LoadCredential` exposes them under fixed lookup names; `%d` selects that directory
for Devlish. Mounted-source checks never fall back to environment secrets. The
source secret directory is inaccessible inside the service namespace; the
service receives only the mounted copies. Systemd credentials are loaded at
activation, so rotate them by stopping the old service and starting a new one.
See [systemd's credential interface](https://systemd.io/CREDENTIALS/).

Remove the OpenRouter credential line for a deployment that has no approved model
route. This template deliberately requires both files for the model harness.
The bearer token authenticates a single operator; it is not multi-tenant identity
or per-user authorization. Clients with the token can request the approved program.
They cannot provide the profile, policy, model route or credential source.

The unit requests read-only deployment files, no capabilities or privilege gain,
restricted process operations, a private temporary namespace and resource limits.
`ProtectProc` restricts the service's view of other processes; it does not itself
prevent a host process from inspecting the service. Different-UID access controls,
ptrace policy, ACLs and the host's core-dump policy require qualification. `LimitCORE=0`
alone is not a universal guarantee against privileged core-dump collection.
The directive reference used here is
[systemd 255 execution settings](https://raw.githubusercontent.com/systemd/systemd/v255/man/systemd.exec.xml).

Network sockets remain available for loopback clients, DNS and the fixed HTTPS
model adapter. This is not an OS egress allowlist. A compromised executor may use
its own credentials and network access; place it behind independently administered
egress controls before claiming that threat is contained. No signing key is mounted
in this executor. Receipt issuance requires a separate protected authority.

## Required Linux qualification

Keep the qualification transcript, exact unit/drop-in hashes, OS/kernel/systemd
versions, approved release digests and test identities. Use synthetic secrets and
no-op/fake effects. These are acceptance procedures, not tests already passed.

1. Run `systemd-analyze verify` on the unit and inspect its effective configuration
   and `systemd-analyze security` output. Treat unsupported or ignored settings as
   a failure. A security score alone does not establish the boundary.
2. Provision approved signed artifacts and the durable floor on the disposable
   host. Confirm service startup, authenticated health and one synthetic governed
   request. Check the actual UID, mounted credential mode/owner and log permissions.
3. From a separate unprivileged client UID, attempt to read mounted/source secrets,
   write runtime/policy/trust/unit/state/log files, access process memory and attach
   a debugger. All must fail. Do not run these probes as root and call that a client test.
4. From the service namespace/UID, confirm writes to runtime/policy/trust and access
   to source secrets fail while required state/log writes work. Confirm inherited
   loader overrides are absent, restrictions are actually applied, and host core
   dumps do not expose synthetic inputs or secrets under the configured policy.
5. Stop the service. On the disposable host only, replace an approved policy or
   runtime with altered bytes and verify startup/admission rejection using the
   independently trusted verifier too. Replacing the runtime with malicious code
   can bypass self-checks: passing that scenario requires the separate trusted
   bootstrap boundary, not this unit alone.
6. Test missing, unsafe and rotated credentials; invalid signatures; stale
   revocations; rollback attempts; and a request after artifact replacement.
   After a controlled stop/start, the old bearer token must fail and the new one work.
7. Interrupt a synthetic request between intent and outcome, preserve the log and
   floor, and confirm no automatic retry. Review ambiguous effects before accepting
   another request. `Restart=no` avoids automatic process restart; it does not
   provide idempotency or prevent an operator/client from resubmitting work.
8. Test slow/aborted local clients and disk/resource exhaustion. The current HTTP
   adapter is synchronous and lacks hard connection/read deadlines. Memory/task
   limits do not solve that availability gap. Reject a hostile-client availability
   claim until those behaviors are addressed.

A compromised executor account can alter its writable evidence and state. Root,
sudo, the service manager, kernel/loader compromise, a malicious release authority
or a compromised provider are outside this recipe's protection. Signed external
anchors, protected receipt issuance and qualified hardware/remote attestation are
separate work; do not describe file permissions or a signed log as proof of actual
policy enforcement.
