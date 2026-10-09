# Plan 403 — reverse ELS2 consumer secret option corrective

Status: **in-progress-reverse-els2-consumer-secret-option-corrective**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Pass the authorized ELS2 secret to stock i2pd's reverse SAM consumer through the
pinned consumer parameter, `i2cp.leaseSetPrivKey`, then qualify reverse PSK and DH
with healthy controls and explicit signature type 7.

## Why ready

Plans 401 and 402 preserved two attempts where i2pd reported `LeaseSet not found`.
Exact-pin source review distinguishes publisher configuration from consumer
decryption: `Destination.cpp:1084-1102` uses `i2cp.leaseSetAuthType` and
`i2cp.leaseSetClient.*` to construct a local published ELS2; received ELS2 goes
through `Destination.cpp:498-503` with `m_LeaseSetPrivKey`, parsed from
`i2cp.leaseSetPrivKey` at `Destination.cpp:88-97`. `LeaseSet.cpp:661-706` uses
that 32-byte secret for the PSK or DH authorization transcript. The reverse test
currently supplies publisher auth options and does not set the consumer secret
option. This is a bounded test-driver configuration defect.

## Invariants

1. Keep stock i2pd at `635b013a612ff47278ef02acf8580a28e10e26c5`.
2. Keep reverse requester signature type 7 explicit.
3. Supply only `i2cp.leaseSetPrivKey` for remote ELS2 authorization. Remove
   `i2cp.leaseSetType`, `i2cp.leaseSetAuthType`, and publisher client-key groups
   from the requester; they configure a locally published ELS2.
4. The option value is the 32-byte PSK secret or DH private key, base64-encoded
   per pinned i2pd parsing. Never emit it to evidence or logs.
5. Do not change production crypto, transcripts, Proposal inventory, support,
   conformance, or advertisement.
6. One attempt per lane invocation; preserve every failure artifact.

## Scope

In scope: simplify the Rust requester auth branch to add `i2cp.leaseSetPrivKey`
from the existing zeroized/temporary mode credential; remove Plan 402's local
publisher options; add static and mutation guards for the exact consumer option
and absent publisher options; run one healthy PSK and one healthy DH lane.

Out of scope: production behavior changes, reference modifications, type-0
support, new dependencies, transcript changes, Java qualification, or broad
Proposal-170 conformance.

## Work packages

1. Confirm mode credential semantics: PSK bytes are the shared 32-byte secret;
   DH bytes are the private X25519 scalar. Keep the source buffer's lifetime and
   zeroization behavior intact.
2. Build `i2cp.leaseSetPrivKey=<base64(secret)>` only for authorized modes;
   do not append publication auth parameters.
3. Extend checker/mutation coverage for the consumer secret option and reject
   reintroduction of publisher-only auth settings in the requester.
4. Build the managed-app siblings, run the focused driver check and all ELS2
   checker/self-test guards.
5. Run exact-pinned PSK and DH attempts separately, `MAX_ATTEMPTS=1`, only after
   mesh and authority controls pass. Preserve all failed artifacts.
6. If either mode reaches a new boundary, register a successor before behavior
   changes. If both pass, complete Plan 400's acceptance matrix and run the
   routine floor before closing the upstream gate.

## Failure and compatibility

Only the test requester parameters change. Credentials remain process-local and
must be absent from all evidence artifacts. A failed live attempt is retained and
not retried within its invocation.

## Verification

- `cargo fmt --all --check`
- Build managed-app siblings before focused daemon tests.
- `cargo check --locked -p i2pr-daemon --test els2_i2pd_external`
- ELS2 source checker, runner `--self-test`, evidence guard, and encrypted-consumer
  caller guard.
- Exact-pinned PSK and DH lanes with all required controls, each `MAX_ATTEMPTS=1`.
- Full routine floor after both authorized reverse rows pass.

## Acceptance

The requester supplies the pinned consumer secret parameter without publisher
options, guards enforce this distinction, no credential is present in sanitized
artifacts, and stock i2pd returns the fixture payload for reverse PSK and DH.
Otherwise preserve the failing mode and register a bounded successor.

## Closure evidence required

Record exact source paths/symbols, parameter construction, checker mutations,
controls, per-mode results and hashes, commands/outcomes, security and compatibility
review, limitations, and registry/roadmap unblock audit. State all unqualified
Plan 400 rows explicitly.
