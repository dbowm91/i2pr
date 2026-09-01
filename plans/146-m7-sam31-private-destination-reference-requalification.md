# Plan 146 — SAM 3.1 private-destination reference compatibility requalification

Status: **next executable implementation/evidence plan** under Plan 145.

Depends on: Plan 145; retain the Base64 codec correction from Plan 142.

## 1. Goal

Resolve the remaining uncertainty around SAM `PRIV` / PrivateKeyFile representation using an actual reference-generated artifact and an actual reference consumer.

This plan is intentionally narrow. It does not implement the raw STREAM socket driver.

The result must answer one question with evidence:

> What exact private-destination byte representation should i2pr emit and accept for its declared SAM 3.1 Ed25519 destination profile, and is that representation accepted by current reference tooling/clients?

## 2. Why this requalification is required

Plan 142 correctly fixed the SAM Base64 alphabet, but its closure record overstates the private-destination evidence.

The current repository assumes:

```text
Destination                     391 bytes
X25519 private key               32 bytes
Ed25519 signing private/seed     32 bytes
-----------------------------------------
PRIV                             455 bytes
Base64                           608 chars
```

Current official SAM documentation, however, describes `DEST GENERATE PRIV=` as Destination + Private Key + Signing Private Key and states a size of **663 or more binary bytes / 884 or more Base64 characters**. It also states that the 256-byte encryption-private-key field is unused and may be random or zero. Current common-structures documentation separately permits type-specific `PrivateKey` sizes where key type is known.

This is an ambiguity that must be resolved by executable reference behavior, not by choosing whichever prose best matches the existing code.

Normative/current sources to record in the Plan 146 evidence:

- official SAM v3 specification: `https://i2p.net/en/docs/api/samv3/`;
- common structures specification: `https://i2p.net/en/docs/specs/common-structures/`;
- Proposal 161 background on Destination / private-key padding: `https://i2p.net/en/proposals/161-ri-dest-padding/`;
- current Java I2P `PrivateKeyFile` implementation/javadocs;
- at least one second implementation or independent consumer such as i2pd, i2plib, libsam3, or Bitcoin Core's SAM client.

Pin exact versions/commits used at execution time.

## 3. Do not regress the Plan 142 Base64 fix

The following is already accepted and must remain unchanged unless a concrete new reference contradiction is found:

```text
SAM alphabet = A-Z a-z 0-9 - ~
padding      = =
RFC4648 +/   = not canonical SAM input/output
```

Plan 146 is about **binary private-destination structure and reference consumption**, not another Base64 rewrite.

## 4. Reference execution strategy

Use the smallest practical reference lane that works without root or public-network participation.

Preferred options, in order:

1. **Java I2P reference library/tooling**
   - pin current release/revision;
   - use `PrivateKeyFile` / destination key-generation classes directly in a tiny throwaway helper;
   - no router network participation required if the library API can generate and parse the structure directly.
2. **Reference SAM bridge on localhost**
   - Java I2P or i2pd may be started only for local `DEST GENERATE` / `SESSION CREATE` behavior;
   - disable or avoid public-network participation where practical;
   - no root, namespace, Docker, systemd, or live-peer requirement.
3. **Independent parser/consumer**
   - i2plib, libsam3, Bitcoin Core SAM code, or another maintained implementation may consume the i2pr output and independently derive/store the destination.

Do not construct another general-purpose orchestration harness. A small Java/C/Python/Rust helper under `tests/integration/sam/reference/` is acceptable if it has one purpose and pinned provenance.

## 5. Mandatory evidence A — reference generates, i2pr imports

Generate a fresh throwaway private destination outside i2pr.

Record without committing the secret:

```text
reference implementation
version / git commit
command/API used
signature type
crypto/encryption type if explicit
binary PRIV length
Base64 PRIV length
binary public Destination length
Base64 public Destination length
SHA-256 of ephemeral PRIV bytes
SHA-256 / Destination hash of public Destination
whether a 256-byte legacy/unused private-key slot is present
whether any type-specific 32-byte key field is present
```

Then:

1. pass the reference-generated `PRIV` to i2pr's `SamPrivateDestination` / real `SESSION CREATE` path;
2. require successful import;
3. derive i2pr's public Destination;
4. compare exact public Destination bytes and Destination hash to the reference result;
5. destroy the secret run artifact after the evidence digest is recorded.

If i2pr cannot import the reference form, this is the defect Plan 146 must correct.

## 6. Mandatory evidence B — i2pr generates, reference consumes

Use the real i2pr SAM listener:

```text
HELLO VERSION MIN=3.1 MAX=3.1
DEST GENERATE SIGNATURE_TYPE=7
```

Take the returned `PRIV` and feed it to an independent/reference consumer.

Preferred proof:

- reference `PrivateKeyFile` parser successfully loads it; and
- the reference-derived public Destination exactly equals i2pr's returned `PUB`.

If a reference SAM bridge is used instead, create a local STREAM session using the i2pr-generated `PRIV` and confirm the session's public Destination matches i2pr's `PUB`.

A parser that merely accepts Base64 text without interpreting the PrivateKeyFile structure does **not** satisfy this criterion.

## 7. Representation decision

After both evidence directions are collected, choose exactly one declared canonical i2pr SAM output representation.

Possible outcomes include, but are not limited to:

### Outcome A — current compact 455-byte form is reference-compatible

If a current reference `PrivateKeyFile` implementation both generates or accepts the type-4/type-7 compact form and exact public identity equality is proven:

- retain 455/608;
- document why the SAM specification's 663+/884+ prose is a legacy/general-size statement for the historical unused field;
- record the exact reference implementation that proves compact portability.

### Outcome B — SAM requires legacy-width unused private-key slot

If reference SAM/PrivateKeyFile behavior expects the 256-byte field:

- change i2pr SAM `PRIV` serialization to the compatible representation;
- preserve the real destination encryption/session keys elsewhere in i2pr's destination runtime as needed;
- do not pretend the unused SAM PrivateKeyFile field is the ECIES destination session key if the reference format treats it as unused padding;
- parser may optionally accept both legacy-width and demonstrably compatible compact forms only if the compatibility policy is explicit and bounded;
- canonical encoder must emit the form proven portable.

### Outcome C — incompatible identity model

If reference compatibility cannot be obtained without changing deeper destination identity semantics:

- stop;
- create `plans/146-status.md` with a typed blocker;
- do not begin Plan 147;
- identify the exact M6 identity seam that would need a separate corrective plan.

Do not silently invent a custom i2pr-only private-destination format.

## 8. Required code audit

Inspect at minimum:

```text
crates/i2pr-api/src/sam/private_destination.rs
crates/i2pr-api/src/sam/dest_generate.rs
crates/i2pr-api/src/sam/session_create.rs
crates/i2pr-api/src/sam/base64.rs
crates/i2pr-client/src/identity.rs
specs/references/sam31-private-destination.md
```

Questions to answer:

1. Does `SamPrivateDestination` conflate the SAM PrivateKeyFile's unused encryption-private-key slot with i2pr's actual X25519 destination session secret?
2. Is `DestinationIdentity` constructed from data that a reference SAM bridge would preserve?
3. Can a reference-generated public Destination be reproduced byte-for-byte after i2pr import?
4. Can i2pr preserve unknown/unused private-field bytes where compatibility requires round-trip preservation?
5. Are all length constants based on proven representation rather than the current implementation?

## 9. Secret handling

No raw reference `PRIV` value may be committed.

Ephemeral evidence may live under a temporary run directory such as:

```text
target/sam-reference/plan146-<run-id>/
```

Committed evidence must contain only:

- implementation/version/commit;
- commands/API names;
- lengths;
- public Destination hash;
- SHA-256 digest of ephemeral private bytes;
- pass/fail result;
- representation classification.

Delete the ephemeral private material after validation.

Tests that need stable fixture behavior should use a generator or a public structural vector that does not embed a usable private identity.

## 10. Real listener smoke

Plan 146 must include at least one external process/library interaction with the real i2pr listener.

Minimum sequence:

```text
connect to 127.0.0.1:<ephemeral>
HELLO VERSION MIN=3.1 MAX=3.1
DEST GENERATE SIGNATURE_TYPE=7
external/reference consume returned PRIV
SESSION CREATE STYLE=STREAM ID=reference DESTINATION=<reference-generated PRIV>
verify session-created public Destination identity
close control socket
verify resource count returns to baseline
```

This is representation-level evidence only. No STREAM application bytes are required here.

## 11. Tests

Add focused tests for whichever canonical representation is proven:

- exact accepted binary lengths;
- exact emitted binary length;
- exact Base64 length;
- truncation at every component boundary;
- trailing bytes;
- unsupported signature/crypto type;
- public/private signing-key mismatch;
- malformed unused/padding field policy;
- I2P Base64 `-`/`~` regression;
- private `Debug` redaction;
- zeroization/owned-secret behavior;
- repeated import failure does not leak allocation/session state.

Do not add tests that declare reference compatibility solely because i2pr can round-trip its own output.

## 12. Documentation corrections

Update:

```text
specs/references/sam31-private-destination.md
specs/support.toml
docs/protocol-support.md
tests/integration/sam/README.md
README.md if representation claims change
plans/145-status.md
```

Add a superseding note for Plan 142's private-destination claim. Preserve:

```text
plan142_base64 = passed
```

but only restore:

```text
sam31_private_destination = reference-compatible
```

when both evidence directions pass.

## 13. Acceptance criteria

Plan 146 closes only if all are true:

1. current official SAM / common-structures / Proposal 161 behavior is reconciled in documentation;
2. the reference versions/commits used are pinned;
3. a reference-generated private destination is imported by i2pr;
4. exact public Destination bytes/hash match after import;
5. i2pr `DEST GENERATE SIGNATURE_TYPE=7` output is parsed/consumed by an independent/reference implementation;
6. the reference-derived public Destination exactly equals i2pr `PUB`;
7. the canonical binary/encoded length is based on executable reference evidence;
8. any accepted alternate input forms are explicitly documented and bounded;
9. no i2pr-only custom private-key format is emitted as canonical SAM output;
10. no private key material is committed or logged;
11. the real i2pr localhost SAM listener participates in the evidence lane;
12. the Base64 correction from Plan 142 remains green;
13. workspace and boundary gates pass;
14. `plans/146-status.md` records exact evidence and sets `next_executable_plan = 147`.

## 14. Required validation

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Plus the exact pinned reference generation/import commands recorded in the status file.

## 15. Handoff

Do not begin Plan 147 until the private-destination format is proven by both external directions.

If Plan 146 passes, execute **Plan 147 only** next.