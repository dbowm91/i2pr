# Plan 400 — i2pd reverse signature-profile qualification

Status: **in-progress-i2pd-reverse-signature-profile-qualification**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Run the stock i2pd reverse NONE/PSK/DH matrix using an explicit SAM destination
signature type 7, matching i2pr's selected inbound-verification profile. Prove the
reverse fixture payload and all existing authority controls without widening crypto
support or changing the reference binary.

## Why ready

Plan 399's exact-pinned reverse sample reached the correct i2pr owner, then all seven
sender LeaseSet2 records failed because they use signature type 0. ADR 0004 selects type
7 and does not include legacy DSA-SHA1 verification. The official SAM v3 contract exposes
`SIGNATURE_TYPE` on `SESSION CREATE`; this plan uses that standard option with the frozen
stock binary. It does not claim compatibility with i2pd's default type-0 transient
destination.

Hard dependency: Plan 399 closure. No protocol decision is reopened. No new crypto
provider or dependency is proposed.

## Invariants

1. Stock i2pd 2.61.0 at SHA
   `635b013a612ff47278ef02acf8580a28e10e26c5`; every runner invocation retains
   `MAX_ATTEMPTS=1`.
2. Reference binary and source remain unmodified. Use only documented SAM parameters.
3. The reverse requester explicitly selects signature type 7. Record this profile in
   sanitized evidence and test documentation; never label the result as i2pd-default
   compatibility.
4. Keep no type-0 DSA-SHA1 implementation, generation, or advertisement.
5. Evidence remains counts/enums and hashes only; no raw logs, addresses, IDs, keys, or
   payload bytes.
6. Preserve type-5 binding, local receive ownership, Standard LS2 flow, and authority
   controls.

## Scope

In scope: add the standard `SIGNATURE_TYPE=7` parameter to the reference SAM test
destination creation; pin it in the runner/checker and deterministic tests; execute reverse
NONE/PSK/DH and reference/i2pr authority controls; update evidence interpretation to state
the explicit type-7 profile.

Out of scope: DSA-SHA1 or other legacy signature verification, type-11 transcript changes,
Java I2P, new dependencies, support/capability promotion, and the broader Proposal 170
conformance gate.

## Work packages

1. Inspect SAM client and lane interfaces; add a fixed type-7 parameter only to the
   reference reverse client, with bounded evidence that the option is present and no
   secret material is emitted.
2. Add static guard/mutation coverage for the option and the explicit-profile wording.
3. Run focused self-tests, live-evidence checker, reference consumer test, and both
   authority controls on the exact pinned healthy mesh.
4. Run reverse NONE, PSK, and DH rows with the same explicit type-7 requester profile.
   Each mode remains one attempt per runner execution. A failed row is retained and not
   retried within that execution.
5. If a row reaches any new rejection boundary, register a successor before changing
   production behavior. If all rows pass, update the Plan 384/374 disposition and perform
   the full routine acceptance floor.

## Failure, cancellation, and compatibility

The reference client creation remains a normal stock SAM operation. The selected
signature type is explicit and fixed for this qualification, not a runtime preference.
Failure to create the session or pass any authority control makes that run non-qualifying.
No production state or persisted data changes.

## Verification

- `cargo fmt --all --check`
- `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd`
- focused client/daemon trajectory tests and diagnostic snapshot test, serial for socket
  suites
- `python3 scripts/check-els2-live-lane-evidence.py`
- `bash tests/integration/els2/run-i2pd-els2.sh --self-test`
- `bash scripts/check-els2-live-lane-evidence.sh`
- `bash scripts/check-encrypted-service-consumer-caller.sh`
- exact-pinned NONE, PSK, and DH lanes, `MAX_ATTEMPTS=1`, with authority rows green

## Acceptance

The i2pr-published encrypted server returns the fixture banner to stock i2pd for NONE,
PSK, and DH when the reference client selects signature type 7. The reference consumer
control and i2pr ordinary authority row pass on the same healthy mesh. Evidence clearly
labels the explicit signature profile. No DSA-SHA1, support, capability, or
advertisement claim is introduced.

## Stop conditions

Stop if the pinned i2pd SAM rejects the standard signature type parameter, the mesh or
authority control fails, a reverse mode still fails after selecting type 7, or acceptance
would require transcript, credential, support, capability, or advertisement changes.
Preserve artifacts and register a bounded successor.

## Closure evidence required

Record the SAM setting, exact pin, per-mode result and artifact hashes, all controls,
focused tests/guards, source/evidence interpretation, security and compatibility review,
limitations, and the registry/roadmap unblock audit. Explicitly state that type-0 default
compatibility was excluded by the type-7 profile boundary.
