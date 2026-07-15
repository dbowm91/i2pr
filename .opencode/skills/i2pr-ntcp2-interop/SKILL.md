---
name: i2pr-ntcp2-interop
description: Operate, diagnose, or extend the repository's Ubuntu 24.04 reference-router NTCP2 interoperability harness, including host preflight, pinned Java I2P and i2pd preparation, isolated scenario execution, typed evidence validation, cleanup, and fail-closed result interpretation. Use when Codex is asked to run Plan 038, prepare its reference routers, add scenarios or adapters, inspect interoperability outcomes, or update this apparatus.
---

# I2PR NTCP2 interoperability

Use this skill from the repository root for the manual, opt-in Plan 038
harness. Read `AGENTS.md`, `plans/038-ubuntu-reference-router-interoperability-harness.md`,
`tests/integration/ntcp2/README.md`, and the relevant architecture/ADR files
before changing the apparatus.

## Safety boundary

Treat the harness as experimental infrastructure, not an anonymity or security
tool. Never enable `i2pr-daemon`, use public egress, perform DNS/bootstrap or
reseed, retain identities/keys/RouterInfo/raw logs/packet captures, or turn a
local self-handshake, loopback run, vector, or testkit result into Java I2P or
i2pd interoperability evidence. Keep support rows experimental and
non-advertised unless sanitized evidence satisfies `specs/CONFORMANCE.md`.

Run only on an authorized disposable Ubuntu 24.04 amd64 host. The namespace
and firewall checks are mandatory and fail closed. Do not bypass a host,
privilege, route, cleanup, or evidence validation error.

## Workflow

1. Inspect the lock and scenario definitions before execution. Do not change
   source revisions, package assumptions, scenario IDs, or the IzPack hash
   without updating the plan and conformance documentation.
2. Run `bash scripts/interop/ubuntu/check-host.sh --pre-install`. On the
   authorized host, run the declared `setup-host.sh` once, then
   `check-host.sh --post-install`.
3. Prepare the exact reference caches with
   `bash scripts/interop/build-references.sh`; use `--offline` only when the
   cache already exists and network access is intentionally unavailable.
4. Run the smallest required profile first. Use `run-matrix.sh --profile
   environment-smoke`, then `reference-crosscheck-ipv4`, then handshake/full
   only after the earlier gates pass. Pass `--offline` when appropriate and
   use `--keep-failed-sanitized` only when reviewing an allowed sanitized
   failure record.
5. Validate every retained record with
   `bash scripts/interop/validate-evidence.py` and
   `bash scripts/check-ntcp2-interoperability.sh`. Empty evidence is not
   success.
6. Always run the bounded cleanup path and verify no namespaces, veths, child
   processes, or secret-bearing run roots remain.

Consult [operations.md](references/operations.md) for command routing,
profiles, typed outcomes, and implementation-specific stop conditions.

## Development rules

Keep production ownership boundaries intact: runtime owns Tokio tasks and
sockets; transport contracts remain runtime-neutral; the launcher crate under
`tools/i2pr-interop` is a non-production seam and must not activate the daemon.
Add negative-path tests for new configuration, topology, process, parser, or
evidence behavior. Prefer deterministic local checks and never add raw network
fixtures or secrets.

Before handoff, run the repository's required Rust, boundary, fixture/vector,
interoperability, Python harness, and shell syntax checks. Record commands,
results, host constraints, and any blocked stop condition in a closure record;
do not report a blocked profile as a passing interoperability result.
