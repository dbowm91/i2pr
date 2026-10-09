# Plan 402 — reverse authorized requester key-loading corrective

Status: **in-progress-reverse-authorized-requester-key-loading-corrective**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Correct the Plan 400 reverse SAM requester setup so stock i2pd loads its supplied
PSK/DH client authorization key, then qualify reverse PSK and DH with healthy
controls and explicit signature type 7.

## Why ready

Plan 401 inspected the exact i2pd 2.61.0 pin. `LeaseSetDestination::SetOptions`
loads `i2cp.leaseSetClient.psk` or `.dh` only when `i2cp.leaseSetType=5`; Plan 400's
requester supplies the auth type and key but omitted that prerequisite, leaving
`m_AuthKeys` unset. The missing option is a bounded test-driver defect, not a
production router or crypto defect. Plan 401 records the source proof and failed
artifact.

## Invariants

1. Stock i2pd remains at SHA `635b013a612ff47278ef02acf8580a28e10e26c5`.
2. Reverse requester signature type remains explicitly 7.
3. For PSK/DH, configure `i2cp.leaseSetType=5` alongside auth type and key so the
   pinned requester loads the key group selected by that auth type.
4. Emit no credentials, raw logs, addresses, IDs, or payloads into evidence.
5. Do not change production crypto, transcript, Proposal inventory, support,
   conformance, or advertisement.
6. Each live row is one attempt per runner invocation; preserve every failure.

## Scope

In scope: add the missing `i2cp.leaseSetType=5` to the Rust reverse requester when
PSK/DH credentials are configured; make the checker assert the mode-coupled option
and detect mutations; update Plan 402 evidence labels; run PSK and DH with all
required healthy controls. If both pass, update the Plan 400 disposition and run
the authorized full acceptance floor.

Out of scope: production behavior changes, reference source/binary modifications,
type-0 support, new dependencies, transcript changes, Java qualification, or broad
Proposal-170 conformance.

## Work packages

1. Add `i2cp.leaseSetType=5` only in the requester credential branch, adjacent to
   `i2cp.leaseSetAuthType` and the mode-specific key. Preserve NONE's existing
   profile unless i2pd requires the option for type-7 destination setup.
2. Extend the static lane checker and negative mutations to require the exact
   condition and property; retain evidence that signature type 7 is explicit.
3. Build/check the changed test and run focused checker/self-tests.
4. Run one exact-pinned PSK lane and one DH lane with `MAX_ATTEMPTS=1`; stop if any
   authority, mesh, or pool-health control fails.
5. If a corrected mode reaches a new rejection boundary, register its successor
   before any production behavior change. If both pass, complete Plan 400's matrix
   and routine floor before closing the upstream gate.

## Failure, cancellation, and compatibility

The only change is a standard i2cp option in external test-driver setup. Credentials
remain in process memory and are not written to sanitized artifacts. A failed
attempt remains recorded and is not retried within the same runner execution.

## Verification

- `cargo fmt --all --check`
- Build managed-app siblings before focused daemon tests.
- Focused `els2_i2pd_external` check and any direct credential-profile unit test.
- `python3 scripts/check-els2-live-lane-evidence.py`
- `bash tests/integration/els2/run-i2pd-els2.sh --self-test`
- `bash scripts/check-els2-live-lane-evidence.sh`
- `bash scripts/check-encrypted-service-consumer-caller.sh`
- Exact-pinned PSK and DH runs with all required controls and `MAX_ATTEMPTS=1`.
- Full routine floor after both authorized reverse rows pass.

## Acceptance

The source-proven missing key-loading option is guarded, and stock i2pd returns the
fixture payload from the i2pr-published reverse server for both PSK and DH, with
signature type 7 explicit and all authority/mesh controls passing. No production
crypto or protocol behavior changes. Otherwise, preserve the failed row and register
a bounded successor before changing behavior.

## Closure evidence required

Record the exact source proof, diff, checker mutations, all controls and per-mode
results, artifact hashes, commands/outcomes, security/compatibility review,
limitations, and registry/roadmap unblock audit. State any Plan 400 matrix rows
that remain unqualified.
