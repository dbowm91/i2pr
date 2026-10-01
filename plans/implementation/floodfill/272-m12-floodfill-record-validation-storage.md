# Plan 272 — M12 complete floodfill record validation and bounded storage surface

Status: **passed-m12-record-validation-storage-types-1-3-7-type5-deferred**

Classification: protocol/invariant capability foundation.

Hard dependencies: Plan 271 passed and Plan 281 passed with the corrected type-5-deferred support floor.

## 1. Objective

Implement the current NetDB record surface required for truthful floodfill serving: RouterInfo
plus DatabaseStore types 1, 3, and 7, with strict canonical decoding, signature/key binding,
freshness/replacement semantics, per-type quotas, and provenance-aware storage.

No server replies, flooding, or capability advertisement are added.

## 2. Current evidence

RouterInfo validation/store is mature. Standard LeaseSet2 has a bounded validator/store for the
current ordinary online-signed subset. i2pr-proto structurally recognizes classic LeaseSet and
currently retains EncryptedLeaseSet payloads as deferred. ADR 0027 freezes the exact
M12 support floor and signature/key rules.

## 3. Invariants

- Unsupported/deferred bytes are never served merely because they were stored.
- Every accepted record is bound to the DatabaseStore key using the type-specific normative
  derivation.
- Signatures, offline signatures, and type-byte signature domains are verified exactly.
- Type 5 encrypted payload remains opaque; floodfill code never attempts destination decryption.
- Unpublished flags are retained but deny answer/flood eligibility as ADR 0027 requires.
- Expired records are never answerable/floodable.
- Record count and aggregate bytes are bounded independently by type and globally.
- Replacement/conflict semantics are deterministic and do not permit same-version byte conflicts.
- No parser allocates from peer-controlled lengths without hard ceilings.

## 4. Required production changes

A. Add ValidatedLeaseSet + bounded classic LeaseSet store or a unified LeaseSet record store.

B. Correct the existing LeaseSet2 flag constants to the pinned normative assignment before broadening validation: bit 0 offline signature, bit 1 unpublished, bit 2 blinded-on-publication, bits 15–3 reserved. Remove the non-normative bit-2 `LEASED` interpretation and reject reserved bit 3. Extend Standard LeaseSet2 validation to the ADR-authorized current surface, including offline signatures and supported encryption/signature key families needed for floodfill storage.

C. Add canonical MetaLeaseSet codec/validator:
- LeaseSet2 header and offline signature;
- bounded options, MetaLease count, revocation count;
- type-7 signature domain;
- destination/key binding;
- published/expires semantics.

Deferred: EncryptedLeaseSet type-5 validation is out of scope under Plan 281. Preserve the existing
deferred protocol representation and reject it for server-authority storage, answer, persistence,
and replication. Do not implement a partial outer validator without Red25519.

No type-5 outer validator is added in this plan. It remains deferred until a separate Red25519
provider qualification plan passes.

D. Introduce a typed NetDbRecord/ValidatedNetDbRecord enum or equivalent so server plans can handle
record type without Deferred payload branches.

E. Add per-type store policies and aggregate accounting while preserving RouterInfoStore APIs for
existing callers where practical.

## 5. Scope / non-goals

In scope: codecs, validation, storage, expiry/replacement/conflict, fuzz/property seeds, fixtures.

Out of scope: DatabaseStore request semantics, DeliveryStatus, DatabaseLookup replies, peer
selection, replication, persistence changes, role state, daemon I/O, config, caps=f.

## 6. Work packages

1. Freeze independent fixtures/vectors from official structures and exact reference encodings.
2. Implement classic LeaseSet validation.
3. Generalize Standard LS2 to the ADR floor without weakening current strict checks.
4. Implement MetaLeaseSet.
5. Compose all validated records into provenance-aware bounded storage.
6. Add malformed/truncation/oversize/property/fuzz coverage.
7. Add expiry sweep APIs driven by caller-supplied time.

## 7. Failure / cancellation / restart / contention

Synchronous only. A malformed, unsupported-key, bad-signature, stale, conflicting, or
capacity-exceeded record returns a typed error/outcome without partial insertion. No async work.
Restart persistence remains Plan 276.

## 8. Compatibility and migration

Existing Standard LS2 consumers must continue to accept their current valid subset. New support is
additive but strict. Deferred type-5 payloads must not be reinterpreted without successful
canonical decode/validation.

No new dependency is allowed merely to implement crypto already provided by workspace primitives.
Any unavoidable dependency requires the standard supply-chain review.

## 9. Required tests

For each type: canonical encode/decode, key mismatch, bad signature, bad offline signature,
future/expired timestamp, zero/excessive counts, length overflow/truncation/trailing bytes,
replacement newer/older/equal-identical/equal-conflict, quota atomicity, unpublished behavior,
and persistence-eligibility classification.

Property/fuzz targets must cover the new type-7 decoder. Do not add a type-5 decoder or fuzz
target while Red25519 remains unavailable.

## 10. Exact verification

~~~bash
cargo fmt --all --check
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-crypto --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo clippy --locked -p i2pr-proto -p i2pr-crypto -p i2pr-netdb --all-targets -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
~~~

Run repository fuzz/property smoke commands for all newly registered targets.

## 11. Documentation

Update common-structure support rows and specs/protocols/04-reseed-netdb.md with exact supported
record semantics. Do not mark floodfill serving implemented.

## 12. Acceptance criteria

- Types 0/1/3/7 have validated non-Deferred server-authority representations; type 5 remains unsupported and cannot enter server-authority storage.
- All key/signature/freshness rules from ADR 0027 are enforced.
- Per-type/global quotas are explicit and tested.
- Unpublished/expired material cannot become answer/flood eligible.
- Existing M4/M6 suites remain green.
- No server reply, flood action, daemon activation, or caps=f exists.
- No critical/high finding remains.

## 13. Stop conditions

Stop if a required signature/key type lacks reviewed crypto support, an official structure is
underspecified in a security-sensitive way, or reference fixtures disagree with normative bytes.
Resolve by focused plan/ADR, not permissive parsing.

## 14. Closure evidence

Record support matrix, vectors/provenance, fuzz/property coverage, quota evidence, exact commands,
dependency review, and unblock audit. On pass, move Plan 273 to ready.

## 15. Handoff

Plan 273 consumes only ValidatedNetDbRecord values and may not add alternate validation paths.
