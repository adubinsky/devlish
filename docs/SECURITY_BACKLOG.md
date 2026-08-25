# Security Backlog (Deferred While Language Features Are Prioritized)

This project currently prioritizes language and parser feature completion.
The items below are intentionally deferred and must be addressed before production deployment.

## Service Runtime Isolation
- Move service execution out-of-process (broker/worker boundary).
- Add least-privilege runtime controls for filesystem and network access.
- Define per-service capability permissions and enforce them at runtime.

## Service Installation Trust
- Require signed/verified service packages or explicit allowlist approval.
- Add provenance metadata (publisher, version, checksum) to installed services.
- Block untrusted installs by default.

## Service Registry Integrity
- Prevent silent service-name collisions in the registry.
- Require explicit override mode for replacement of existing service names.
- Add audit trail for registration, update, and removal events.

## Configuration and Secret Handling
- Keep credentials out of plain YAML where possible (secret manager/env indirection).
- Add validation for required config fields before service activation.

## Operational Safeguards
- Add startup security checks (`devlish doctor` style) for trust/isolation policy.
- Add structured security logs for service load/execute events.

---

Status: deferred by decision while language features are completed first.
