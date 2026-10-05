# Plan 350 — Floodfill type-5 lookup serve path

Status: **registered-floodfill-stores-type5-but-never-serves-it-on-lookup-ready**

Classification: **invariant + narrow capability gap**. Origin: the corrected boundary in
[`347-status.md`](../../closure/i2pcontrol-proposal-170/347-status.md).

Hard dependencies:
- Plan 346 passed (ADR 0032). Required, because a reference-published type-5 record only
  validates at the floodfill once the deployed transcript is accepted — before Plan 346 this
  plan's own inbound store path would have rejected every Java- and i2pd-signed record.
- Plan 333 passed (type-5 framing and validation).

No other dependency. Subsystem: `i2pcontrol-proposal-170`.

## The gap, stated exactly

The controlled floodfill **stores** type-5 records and the store layer **retrieves** them by
blinded key. The floodfill's lookup path **never asks for type 5**.

Storage works. `floodfill_service.rs:739-752` accepts
`DatabaseStoreData::EncryptedLeaseSet`, validates it with
`ValidatedEncryptedLeaseSet2::validate(value, Some(BlindedStorageKey::from_hash(key)), …)`, and
files it as `ValidatedNetDbRecord::EncryptedLeaseSet2`. `message_key` at
`floodfill_service.rs:778-780` already maps `DatabaseStoreData::EncryptedLeaseSet(_) => 5`.

Retrieval works. `server_store.rs:291-300` is a complete `5 =>` arm of
`database_store_for_answer`, looking up `BlindedStorageKey::from_hash(key)` and returning
`DatabaseStoreData::EncryptedLeaseSet`.

The lookup does not reach it. `floodfill_service.rs:336-343`:

```rust
let types: &[u8] = match lookup.lookup_type {
    0 => &[0, 1, 3, 7],   // RouterInfo, LeaseSet, LeaseSet2, MetaLeaseSet
    1 => &[1, 3, 7],      // LeaseSet lookup
    2 => &[0],
    3 => &[],
    _ => return Ok(None),
};
```

Record type 5 is absent from every list. A stock Java or i2pd client resolving an encrypted
service issues a blinded-key LeaseSet lookup — `DatabaseLookupMessage.lookup_type == 1`, a
2-bit field per `i2pr-proto/src/i2np/message.rs:662` — and therefore always misses, returning
`NoResponse(LookupFailure::UnsupportedType)` or an empty answer even when the record is stored
and fresh under exactly the key that was requested.

This is the **only** thing standing between the current tree and a reference consumer being
able to resolve an i2pr-published encrypted service. It is not a policy question, a
transcript question, or a topology question.

## Why it matters now

Plan 347 needs `i2pr → Java I2P` and `i2pr → i2pd`: a publisher stores a type-5 record, an
independent router's NetDB actually serves it, and the consumer verifies the outer type-11
signature, decrypts, validates the inner LeaseSet2, and uses it. The store half is done and
Plan 346 made the inbound path accept the deployed transcript. The serve half is missing.

## Objective

Make the controlled floodfill answer a blinded-key LeaseSet lookup with the stored type-5
record, so a stock reference client can complete the store→lookup→decrypt chain against i2pr.

## In scope

1. **Add record type 5 to the floodfill's lookup candidate list** for `lookup_type == 1`
   (LeaseSet lookup), which is the type a blinded-key client issues.
2. Decide and document the `lookup_type == 0` case. A type-5 record is stored under the
   *blinded* storage key, not the destination hash, so a type 0 lookup at a destination hash
   can never hit one. Adding 5 there is harmless but semantically noisy; leaving it out is
   defensible. Either choice is acceptable **provided it is stated and pinned by a test**, so
   the choice cannot be mistaken for an oversight later.
3. A regression row that stores a type-5 record, performs a real `lookup_type == 1` lookup at
   its blinded storage key, and asserts the returned `DatabaseStore` carries the type-5 record
   back byte-for-byte.
4. A guard preventing the served-type list from silently losing a type that the store layer
   can serve.

## Out of scope

- **The i2pr type-5 *consumer* path** (address → secret → blinded key → lookup → validate →
  decrypt). That is Plan 349 and it is a separate, larger piece of work.
- **Any reference-side driver.** Java 2.13.0 already implements the consumer surface
  (`EncryptedLeaseSet`, `Blinding`, `BlindingInfoMessage`, blinded lookup in
  `RequestVariableLeaseSetMessageHandler`/`LookupDestJob`) and pinned i2pd 2.61.0 does too
  (`Destination.cpp:491`, `RequestLeaseSet` with `requestedBlindedKey`). Building those drivers
  belongs to the Plan 347 re-attempt, not here.
- **Any caps or advertisement change.** The Java caps gate is a tunnel-peering gate
  (`TunnelPeerSelector.shouldExclude`); nothing in this plan requires Java to peer with i2pr,
  so ADR 0030 is untouched and no caps letter is added.
- **Any transcript change.** ADR 0032 is the current, correct policy.
- Running the live cross-router matrix.

## Invariants

- The change is a lookup-type list, not a new store, index, or key space. No new
  configuration, key derivation, or record type is introduced.
- The returned record must be the **same validated record** the store admitted, with no
  re-validation that could accept bytes the store rejected, and no re-encoding that could change
  the signature preimage.
- No unbounded growth: a lookup probes a fixed, small type list. Type 5 adds at most one
  additional store probe per lookup, and the existing per-key and global lookup throttles
  (`floodfill_service.rs:308-321`) are unchanged and still apply.
- Record type 0 and 2 (RouterInfo) answers, and type 3 (exploration) answers, must be
  byte-identical before and after this change.
- `i2pr-netdb` stays runtime-neutral: no `tokio`, `std::net`, `std::fs`, or `JoinHandle`.
- No production crate may depend on `i2pr-testkit`.

## Required evidence

**Positive**
- A type-5 record stored under a blinded key is returned by a `lookup_type == 1` lookup at
  that key, byte-identical.
- The same for a record with the offline-key flag set.
- The returned record re-validates through `ValidatedEncryptedLeaseSet2::validate` and still
  reports the transcript profile it was stored under (ADR 0032).

**Negative**
- A `lookup_type == 1` lookup at a **destination hash** that has a type-3 record but no
  type-5 record still answers with the type-3 record and does not fabricate a type-5 miss
  error.
- A `lookup_type == 1` lookup at a blinded key with **no** stored type-5 record returns a
  clean miss, not an error that could be read as a store failure.
- A **tampered** or **expired** stored type-5 record is still refused on the way out.
- A type-5 record stored under a key that does not match its own blinded key is not served
  under the requested key.
- `lookup_type 0` and `lookup_type 3` answers are unchanged.
- A wrong-length or malformed `DatabaseLookupMessage` is still refused before any type probe.

**Guard**
- A static check that every record type the store layer can serve is either present in the
  floodfill lookup list or explicitly listed as intentionally not served. The check fails if a
  new servable type is added to `database_store_for_answer` without a decision here.

## Production changes

Expected: one candidate-list change in
`crates/i2pr-netdb/src/floodfill_service.rs::lookup_body`, plus the static guard and its
regression rows. No new dependency, no wire format change, no advertisement change, no
configuration surface.

## Verification

```text
cargo test --locked -p i2pr-netdb --test floodfill_type5_serve -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
cargo test --locked -p i2pr-crypto --all-targets -- --test-threads=1
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-els2-type11-transcript-boundary.sh
```
plus the full `AGENTS.md` routine floor.

## Acceptance criteria

Plan 350 passes only when:

1. a `lookup_type == 1` lookup at a blinded storage key returns the stored type-5 record
   byte-identically, for both the plain and offline-key-flagged forms;
2. every negative row above fails closed and is covered by an executed test;
3. type 0, 2, and 3 lookup answers are proven unchanged;
4. the store-serve coverage guard exists and passes;
5. ADR 0032's guard still passes and no transcript, caps, or advertisement policy changed;
6. exact-head routine CI is green.

## Acceptance effect on Plan 347

Passing Plan 350 makes the `i2pr → Java I2P` and `i2pr → i2pd` directions **executable** — the
reference consumer can finally complete store→lookup against i2pr. It does not by itself make
them pass: the reference-side ELS2 drivers and the live lane are still required, and
`i2pd → i2pr` and `Java → i2pr` additionally need Plan 349. It must not be credited with
unblocking the Java *peer-peering* question, because that question does not arise for this
matrix.

## Closure evidence required

Commit, a requirement-to-evidence matrix, commands with local/CI outcomes labelled truthfully,
the store-serve coverage guard's design, the no-new-key-space review, the "no caps or
transcript change" confirmation, known limitations, findings by severity, and the roadmap
disposition.
