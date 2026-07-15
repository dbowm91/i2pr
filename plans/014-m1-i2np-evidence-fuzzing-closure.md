# Milestone 1 plan D: initial I2NP model, evidence, fuzzing, and closure

## Purpose

Complete Milestone 1 by implementing the initial I2NP envelope/message subset required by later milestones, establishing repeatable protocol evidence, fuzzing all public parsers, and closing the milestone without overstating router interoperability.

## Required sources

- `specs/protocols/02-i2np.md`
- relevant common-structure dossiers
- `specs/CONFORMANCE.md`
- pinned official specification and proposal revisions in `specs/SOURCES.md`

## Scope

### I2NP protocol model

Implement:

- I2NP message type identifiers;
- standard header/envelope variants required by the pinned specification and later NTCP2/SSU2 plans;
- message ID, expiration, size, and checksum/authentication fields as applicable;
- only the initial message bodies required by Milestones 3–6, such as database lookup/store/search replies, delivery status, garlic/container transport, tunnel data/gateway, and tunnel build messages, according to the dossier’s explicit subset.

The exact list must be reconciled against the dossier before code begins. A message may be represented as `unsupported` or deferred where body semantics belong to later milestones, but the decoder must not falsely accept arbitrary opaque content as implemented behavior.

Do not implement routing, NetDB actions, tunnel cryptography, garlic decryption, or transport framing in this plan.

## I2NP design requirements

### Envelope and body separation

Separate:

- header/envelope decoding;
- message-body byte bounds;
- typed body decoding;
- semantic policy performed by future subsystems.

A transport should eventually be able to decode an authenticated I2NP frame without gaining access to NetDB or tunnel policy.

### Expiration handling

Structural parsing validates representability. Runtime clock-skew and stale-message policy belong to later state machines. Test helpers may classify timestamps, but the codec must not depend on wall-clock time.

### Message limits

Define explicit maximum body and total sizes from the specification and router policy. Checked arithmetic must cover header plus body calculations. Unknown message types must produce explicit unsupported errors unless the specification defines safe forwarding behavior needed by the MVP.

### Checksums and integrity fields

Implement only the exact checksum/integrity behavior specified for each header variant. Use reviewed hash implementations. Test mismatches and truncated checksum material.

### Typed versus opaque bodies

Opaque body retention is acceptable only for message types whose framing must be carried by a later subsystem and whose dossier explicitly permits deferring semantic decode. Such values must be named `Opaque` or `Deferred`, bounded, and never counted as fully implemented support.

## Implementation phases

### Phase A: message registry

1. Reconcile the required message list against Milestones 3–6.
2. Create typed identifiers and a support table.
3. Mark each as fully decoded, framing-only/deferred, or unsupported.
4. Update the machine-readable support ledger before implementation claims.

### Phase B: envelope codecs

1. Implement strict bounded header parsing.
2. Validate declared lengths before body allocation.
3. Validate checksums/integrity fields.
4. Reject trailing bytes in top-level decode.
5. Add canonical encoding for locally originated supported message forms.

### Phase C: selected body codecs

1. Implement body types in dependency order.
2. Reuse common structures from Plan 012.
3. Keep network actions out of codec methods.
4. Add fixed positive and malformed vectors per type.
5. Ensure each body has an explicit maximum count/size.

### Phase D: fixture corpus

Create a structured fixture layout, for example:

```text
tests/fixtures/
  common/
  router-info/
  lease-set/
  i2np/
  malformed/
```

Each fixture must have adjacent metadata or a manifest containing:

- identifier;
- source/provenance;
- specification revision;
- generator and seed if generated;
- expected decode outcome;
- expected error class for malformed cases;
- license/redistribution note;
- hash of the fixture.

Do not store live peer identities, IP addresses, destination keys, or operational captures without sanitization and review.

### Phase E: fuzzing

Set up maintained fuzz targets for all public top-level decoders:

- Mapping;
- certificate/key certificate;
- RouterIdentity;
- Destination;
- RouterAddress;
- RouterInfo;
- Lease and selected LeaseSet variants;
- I2NP envelope;
- each independently complex I2NP body.

Harness rules:

- cap input size;
- assert no panic;
- assert no unbounded allocation or pathological loop where measurable;
- avoid network, filesystem, and nondeterministic global state;
- preserve minimized regressions in the ordinary test corpus;
- document how to run short local fuzz smoke tests and longer campaigns.

Use `cargo-fuzz` or another focused tool only after dependency/tooling review. Fuzz-only dependencies must not enter production builds.

### Phase F: property and differential tests

Add properties where they add independent assurance:

- canonical encode is deterministic;
- decode(canonical encode(valid value)) succeeds;
- encoded length equals calculated length;
- mutation of signed bytes invalidates verification;
- unsupported identifiers never fall back to a supported type;
- strict top-level decoders reject appended bytes.

Round-trip alone is insufficient. Include fixed expected bytes and, where practical, independently generated vectors decoded by another implementation or tool.

### Phase G: support and documentation closure

Update:

- machine-readable protocol-support ledger;
- `docs/protocol-support.md`;
- `docs/architecture.md` if crate boundaries changed;
- `docs/security-model.md`;
- relevant dossiers with resolved ambiguities and source pins;
- known limitations;
- an explicit Milestone 1 closure record.

Support wording must distinguish:

- structure codec implemented;
- signature verification implemented;
- locally generated RouterInfo implemented;
- I2NP framing/body codec implemented;
- router-to-router interoperability not yet implemented.

## Milestone 1 validation matrix

Run at minimum:

```text
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
cargo deny check advisories bans sources
cargo +1.85 check --workspace --all-targets
```

Use the actual declared MSRV if corrected from 1.85.

Also run:

- short smoke runs for every fuzz target;
- tests under at least Linux and macOS CI;
- fixture hash/provenance validation;
- secret-scanning or targeted output inspection for committed fixtures and logs.

## Milestone exit criteria

Milestone 1 closes only when:

- all planned common structures have strict bounded codecs;
- selected algorithms have reviewed wrappers and vectors;
- identity generation and atomic reload work;
- canonical RouterInfo signing/verification works;
- initial required I2NP envelopes and bodies have fixed and malformed vectors;
- every public top-level parser has a fuzz target or documented justified exception;
- minimized malformed fixtures are retained;
- source-to-code traceability is complete;
- support documentation is exact and noninflated;
- CI and MSRV checks are green;
- no sockets, reseeding, NetDB actions, tunnels, or capability advertisement exist.

## Closure record

Create `plans/010-milestone-1-closure.md` or an equivalent closure file containing:

- completed plan files and commits;
- public API inventory;
- supported structures and algorithms;
- storage format version;
- fixture and fuzz inventory;
- quality/CI results;
- security decisions;
- deviations;
- unresolved ambiguities;
- explicit prerequisites for Milestone 2 and Milestone 3.

Do not mark the roadmap milestone complete until this record exists.

## Stop conditions

Stop and report if:

- the required I2NP subset cannot be derived consistently from later milestone needs;
- a message’s signed/checksummed region is ambiguous;
- fixture provenance cannot be established;
- fuzzing reveals uncontrolled allocation or complexity that requires architectural changes;
- supporting a message body would prematurely require NetDB, tunnel, or garlic state-machine behavior;
- tests pass only by loosening strict decoder policy.

## Handoff

Report the message support registry, limits, fixtures, fuzz targets, commands/results, differential evidence, support-ledger changes, and all remaining unsupported or framing-only message types.