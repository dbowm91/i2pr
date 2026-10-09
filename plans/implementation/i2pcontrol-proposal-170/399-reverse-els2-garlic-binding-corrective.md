# Plan 399 — reverse ELS2 Garlic binding corrective

Status: **in-progress-reverse-els2-garlic-binding-corrective**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Identify the exact typed reason stock i2pd's reverse Garlic envelopes fail after
receive-owner resolution, correct only the first demonstrated binding defect, and
prove the NONE reverse fixture payload on the exact-pinned healthy mesh.

## Why ready

Plan 398 closed the ingress boundary: a healthy exact-pinned run delivered seven complete
Garlic envelopes to the correct service owner, with no dispatch errors, but all seven
were rejected before payload dequeue. Its coarse binding category covered missing
sender LeaseSet2, sender LeaseSet2 validation, sender key mismatch, or unknown local
destination. Plan 398's last attempt split those outcomes but the reference self-connect
control failed before reverse traffic, so the healthy run remains required.

Hard dependency: Plan 398 closure. No unresolved protocol decision is to be decided in
this plan; if the live result requires changing transcript or supported profile, stop and
register a decision plan.

## Invariants

1. Stock i2pd 2.61.0 at SHA
   `635b013a612ff47278ef02acf8580a28e10e26c5`; each runner execution retains
   `MAX_ATTEMPTS=1`.
2. Keep the reference unmodified and all payload traffic on loopback/private mesh.
3. Evidence contains bounded counts/enums only: no raw logs, identities, addresses,
   tunnel IDs, keys, or payloads.
4. Preserve type-5 key binding, local receive-ID ownership, cancellation and rollback,
   and the Standard LS2 path.
5. Do not change transcript, Proposal inventory, credentials, support, capability, or
   advertisement.

## Scope

In scope: exact typed rejection attribution for the reverse attempt; source review of the
corresponding I2P ECIES/LeaseSet2 binding requirement at the frozen reference pin; one
first-transition correction with deterministic regression coverage; exact-pinned NONE
rerun and evidence/checker updates.

Out of scope: PSK/DH reverse qualification, Java I2P, protocol transcript changes,
capability promotion, unrelated inbound crypto refactors, and weakening evidence guards.

## Work packages

1. Verify the new fine-grained counters and checker against source mutations; ensure every
   rejection variant is classified and aggregate totals remain consistent.
2. Run one healthy-mesh exact-pinned NONE lane. Confirm the reference authority controls
   pass before interpreting reverse counters. Record sanitized artifacts and hashes.
3. Trace the observed typed outcome through `DestinationDispatcher`, ECIES session state,
   sender LeaseSet2 validation, and local clove ownership. Compare only the frozen
   reference source at the pinned commit.
4. Correct the first proven defect and add deterministic regression tests for acceptance
   and fail-closed behavior, including capacity/teardown if state ownership changes.
5. Rerun all focused guards and the exact-pinned NONE lane. If the outcome exposes a
   transcript/support decision or the reference control remains unhealthy, stop with a
   successor plan and do not claim reverse completion.

## Failure, cancellation, and compatibility

Typed dispatch failures remain fail-closed and do not enqueue application payloads.
Any provisional responder/session state must be dropped on a failed binding. Diagnostics
are saturating and transaction-scoped; they never retain raw failure data. No persisted
format or configuration migration is expected.

## Verification

- `cargo fmt --all --check`
- `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd`
- focused daemon and client tests for the changed boundary, serial where sockets are used
- `python3 scripts/check-els2-live-lane-evidence.py`
- `bash tests/integration/els2/run-i2pd-els2.sh --self-test`
- `bash scripts/check-els2-live-lane-evidence.sh`
- `bash scripts/check-encrypted-service-consumer-caller.sh`
- exact-pinned NONE lane, `MAX_ATTEMPTS=1`, with healthy authority controls

## Acceptance

The stock i2pd reverse request authenticates, passes sender identity/LeaseSet2 binding,
reaches the correct local service owner, and returns the fixture banner through canonical
Streaming. Evidence records each transition and all existing authority controls. No
broader ELS2 conformance or support claim is made.

## Stop conditions

Stop if the precise rejection cannot be distinguished with bounded evidence, the healthy
mesh controls fail, the frozen reference source disagrees with an assumed contract, or a
fix would require changing protocol transcript, inventory, credentials, support, or
advertisement. Register a new corrective or decision plan with the exact blocker.

## Closure evidence required

Record changed files and commits, focused tests and guards, healthy exact-pinned result
and artifact hashes, reference source anchors, rejection-to-fix evidence, security and
compatibility review, limitations, findings, and the registry/roadmap unblock audit.
