# Plan 350 — Floodfill type-5 store and lookup serve path

Status: **passed-floodfill-now-stores-and-serves-encrypted-leaseset2-type5; reference-consumers-can-resolve-an-i2pr-published-service**

> **Amended during implementation.** This plan was registered from Plan 347's boundary with the
> premise that the type-5 **store** half already worked and only the serve list was missing.
> **That premise was wrong**, and reading the code before editing it is what caught it:
> `FloodfillStoreService::handle` also refused `record_type == 5` outright
> (`if record_type == 5 { return FloodfillStoreEffect::Unsupported; }`), so the type-5 arm in
> `validate` and the `5 =>` arm in `ServerNetDb::database_store_for_answer` were both
> unreachable. The scope was widened from one change to two. Plan 347's record was corrected in
> the same commit. See the closure record for the full account.

Classification: **invariant + narrow capability gap**. Origin: the corrected boundary in
[`347-status.md`](../../closure/i2pcontrol-proposal-170/347-status.md).

Hard dependencies:
- Plan 346 passed (ADR 0032). Required, not formal: before Plan 346 the type-11 verification in
  `ValidatedEncryptedLeaseSet2::validate` would have rejected the deployed transcript, so a
  controlled floodfill could not have usefully admitted a *reference-published* type-5 record
  even with the store refusal removed.
- Plan 333 passed (type-5 framing and validation).

No other dependency. Subsystem: `i2pcontrol-proposal-170`.

## The gap, stated exactly

The controlled floodfill would not store an encrypted LeaseSet2, and could not serve one.

**Store.** `ServerNetDb` has complete type-5 machinery: a
`ValidatedNetDbRecord::EncryptedLeaseSet2` variant filed at `RecordId::new(5, storage_key)`
(`server_store.rs:31,41`), insertion through `self.els2.insert(value)` (`:170`), retrieval via
`encrypted_lease_set2_for_answer` (`:470`), and a complete `5 =>` arm in
`database_store_for_answer` (`:291-300`). The floodfill's `validate()` already builds the right
variant for an inbound type-5 record (`floodfill_service.rs`, the
`DatabaseStoreData::EncryptedLeaseSet` branch, including the note that the floodfill never
derives the subcredential because it never learns the unblinded public key).

**But the store entry point refused it**, before validation was reached:

```rust
if record_type == 5 {
    return FloodfillStoreEffect::Unsupported;
}
```

The guard was present in the commit that created the service (`53a404b netdb: add bounded
floodfill DatabaseStore service`) — a deliberate from-the-start hold-back, not an oversight. Its
rationale was the type-11 transcript disagreement that Plan 346 closed.

**Serve.** Even once stored, `lookup_body` never asked for type 5:

```rust
let types: &[u8] = match lookup.lookup_type {
    0 => &[0, 1, 3, 7],   // RouterInfo, LeaseSet, LeaseSet2, MetaLeaseSet
    1 => &[1, 3, 7],      // LeaseSet lookup
    2 => &[0],
    3 => &[],
```

A stock Java or i2pd client resolving an encrypted service issues a blinded-key LeaseSet lookup
— `DatabaseLookupMessage.lookup_type == 1`, a 2-bit field per `i2pr-proto/src/i2np/message.rs:662`
— and therefore always missed, returning nothing even when the record was stored and fresh under
exactly the key that was requested.

Type 5 is filed under the **blinded** storage key, not the destination hash, so the lookup a
reference client issues is a `lookup_type == 1` lookup *at that blinded key*. That is the row
that must work.

## Why it matters

Plan 347 needs `i2pr → Java I2P` and `i2pr → i2pd`: a publisher stores a type-5 record, an
independent router's NetDB actually serves it, and the consumer verifies the outer type-11
signature, decrypts, validates the inner LeaseSet2, and uses it. Both halves of the store side
were missing.

## Objective

Make the controlled floodfill accept a type-5 record and answer a blinded-key LeaseSet lookup
with it, so a stock reference client can complete the store→lookup→decrypt chain against i2pr.

## In scope

1. **Remove the `record_type == 5` store refusal** so the already-written type-5 arm in
   `validate` becomes reachable.
2. **Add record type 5 to the floodfill's lookup candidate list** for `lookup_type == 1`.
3. Decide and document the `lookup_type == 0` case. **Decision: include type 5.** A type-0
   lookup at a plain destination hash misses the type-5 slot and is answered from the 1/3/7
   slots, so the extra probe is a normal store miss rather than an error — and excluding a
   servable type from the catch-all list is exactly the omission that caused this bug.
4. A regression row proving a stored type-5 record is returned byte-identically, and that
   `lookup_type` 0/2/3 answers are unchanged.
5. A guard preventing a servable type from silently dropping out of the lookup lists again.

## Out of scope

- **The i2pr type-5 *consumer* path** (address → secret → blinded key → lookup → validate →
  decrypt). Plan 349, a separate and larger piece of work.
- **Any reference-side driver.** Java 2.13.0 already implements the consumer surface
  (`EncryptedLeaseSet`, `Blinding`, `BlindingInfoMessage`, blinded lookup in
  `RequestVariableLeaseSetMessageHandler`/`LookupDestJob`) and pinned i2pd 2.61.0 does too
  (`Destination.cpp:491`, `RequestLeaseSet` with `requestedBlindedKey`). Those drivers belong to
  the Plan 347 re-attempt.
- **Any caps or advertisement change.** The Java caps gate is a tunnel-peering gate
  (`TunnelPeerSelector.shouldExclude` caps arity plus `allowAsIBGW`'s `R` requirement); nothing
  here requires Java to peer with i2pr, so ADR 0030 is untouched and no caps letter is added.
- **Any transcript change.** ADR 0032 is the current, correct policy.
- Running the live cross-router matrix.

## Invariants

- Type 5 gains **no exemption from any budget**. `admit_request` already applies a global
  request cap, a global byte cap, a per-source cap, and a per-key cap (keyed on
  `RecordId::new(5, blinded_key)`), and the crypto-validation budget now bounds the extra
  Red25519 verification. Type-5 records land in the separately bounded `Els2Store`.
- The floodfill still **never derives a subcredential**, because it never learns the unblinded
  public key.
- Client-tunnel ingress and a non-serving role remain refused for type 5, as for every type.
- The returned record is the same validated record the store admitted, with no re-validation that
  could accept bytes the store rejected and no re-encoding that could shift the signature
  preimage.
- Record type 0 and 2 (RouterInfo) and type 3 (exploration) answers stay byte-identical.
- No new unbounded growth: a lookup probes a fixed, small type list, and the existing per-key
  and global lookup throttles are unchanged.
- `i2pr-netdb` stays runtime-neutral; no production crate may depend on `i2pr-testkit`.
- No new dependency, wire-format change, configuration surface, or advertisement change.

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
