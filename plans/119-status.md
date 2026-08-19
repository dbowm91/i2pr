# Plan 119 status — closed

- Status: **passed-leaseset2-protocol-foundation**
- Date: 2026-08-19
- Plan-of-record: [`119-m6-leaseset2-protocol-foundation.md`](119-m6-leaseset2-protocol-foundation.md)
- Source floor: `e48cf101b4fc2f18f998d86d9dc437fa340a33fa`
- Predecessor: Plan 118 (planning authority cleanup, closed)
- Successor: **Plan 120** (destination lifecycle and dedicated tunnel pools)

## Authority after Plan 119

```text
plan_119                               = passed-leaseset2-protocol-foundation
plan_118                               = closed
plan_117                               = closed-for-progression-with-evidence-gap
leaseset2_ordinary_subset              = implemented
leaseset2_ordinary_database_store_path  = typed (DatabaseStoreData::LeaseSet2)
leaseset2_encrypted_or_meta            = deferred
leaseset2_blind_or_offline_or_pq       = deferred
next_router_construction_plan          = Plan 120 (destination lifecycle and dedicated tunnel pools)
router_construction                    = may-continue
normal_daemon_ntcp2                    = disabled-and-unenableable
ntcp2                                  = experimental-non-advertised
```

## Plan 119 deliverables (closed)

Plan 119 lands the ordinary online-signed published Standard LeaseSet2
(DatabaseStore type 3) carrier in `i2pr-proto` and `i2pr-netdb`. The
`Lease2`, `LeaseSet2Header`, `LeaseSet2EncryptionKey`, and `LeaseSet2`
codecs are owned by `i2pr-proto`; validation, freshness, and bounded
NetDB storage are owned by `i2pr-netdb`. EncryptedLeaseSet (type 5),
MetaLeaseSet (type 7), blinded, offline-signing, leased, and PQ-hybrid
variants remain deferred.

### Phase A — Lease2 wire format (closed)

- `Lease2` is a strict 40-byte struct: 32-byte gateway `Hash` followed
  by 4-byte big-endian `tunnel_id` and 4-byte big-endian `end_date`
  seconds-since-epoch. Trailing bytes / truncation / noncanonical
  length → typed `Lease2ConversionError`. Time conversion uses
  checked arithmetic into `i2pr_proto::Date32`.
- Phase A unit tests cover exact length, big-endian field order,
  trailing bytes, truncation, and checked time conversion.

### Phase B — Header round-trip (closed)

- `LeaseSet2Flags` is a typed bitfield with reserved bits 4..=15
  tracked separately from the user-settable offline / unpublished /
  leased / blinded bits. `LeaseSet2Flags::from_raw` rejects non-zero
  reserved bits with `LeaseSet2HeaderError::ReservedBits` and the four
  support-policy bits with `LeaseSet2HeaderError::OfflineSigning`,
  `Unpublished`, `Leased`, or `Blinded`.
- `LeaseSet2Header::encode/decode` round-trip:
  `Destination` + 4-byte BE `published_seconds` + 2-byte BE
  `expires_offset_seconds` + 2-byte BE flags. Trailing data after the
  `expires_offset_seconds + flags` segment yields
  `LeaseSet2HeaderError::TrailingBytes`; decoding a longer header
  than the destination allows yields
  `LeaseSet2HeaderError::Overflow`.
- Phase B unit tests cover reserved/unsupported flag typed errors,
  expiration overflow, and trailing data.

### Phase C — Typed encryption-key list (closed)

- `LeaseSet2EncryptionKey` carries `(type, bytes)` with a
  `MAX_LEASE_SET2_ENCRYPTION_KEYS = 8` cap and
  `MAX_LEASE_SET2_ENCRYPTION_KEY_BYTES = 8192` per-key cap.
- `LeaseSet2::encode/decode` round-trip the 2-byte BE key count, the
  2-byte BE type, the 2-byte BE length, and the per-key body. Wrong
  length for `CryptoKeyType::X25519` (type 4) yields
  `LeaseSet2BuildError::X25519WrongLength`; zero keys yields
  `EmptyEncryptionKeyList`. The aggregate key bytes are bounded by
  `MAX_LEASE_SET2_BYTES`.
- Phase C unit tests cover the single-X25519 selector, the
  multi-key list (X25519 + Ed25519 + unknown types),
  unknown-key retention without selection, and duplicate X25519
  rejection in deterministic order.

### Phase D — Options Mapping and signed-bytes preservation (closed)

- `LeaseSet2::signed_bytes` is the verbatim pre-signature byte slice
  reconstructed by the canonical encoder. The canonical Mapping
  encoder (`MappingBuilder`) writes the options in deterministic
  sorted order; the decoder preserves the exact bytes the verifier
  needs. A Mapping encoded non-canonically can never produce a
  signature that verifies against the canonical fixed-vector
  destination.
- Phase D unit tests cover canonical options ordering, the
  `signed_bytes` preservation invariant, and the failure of a
  non-canonical mapping against the canonical fixture.

### Phase E — Signature-domain preimage (closed)

- `LEASE_SET2_SIGNATURE_DOMAIN_BYTE = 0x03`. The verifier computes
  `preimage = [0x03, signed_bytes...]` and verifies against the
  signature stored in the trailing 64-byte Ed25519 signature slot.
- A wrong-domain signature returns
  `LeaseSet2BuildError::SignatureType` when the signature type field
  mismatches the destination's signing key type.
- Phase E unit tests cover full round-trip and signature preimage
  prepended type byte.

### Phase F — EncryptedLeaseSet / MetaLeaseSet remain deferred (closed)

- `DatabaseStoreData::LeaseSet2(Box<LeaseSet2>)` replaces the
  type-3 `Deferred` payload for the ordinary subset. Types 5
  (EncryptedLeaseSet) and 7 (MetaLeaseSet) remain
  `DatabaseStoreData::Deferred(Box<DeferredPayload>)`. The
  i2np-level round-trip (`tests/lease_set2_fixture.rs`)
  verifies the `0x03` byte in the wire form through both the
  standard and short-transport envelopes and asserts that the
  EncryptedLeaseSet / MetaLeaseSet paths still produce typed
  deferred framing.

### Phase G — Destination identity + LeaseSet2 wire (closed)

- `i2pr-netdb::lease_set2::DestinationHash` is a 32-byte wrapper over
  `SHA256(destination)` and is the typed store key.
- `LookupKind::LeaseSet2` carries the wire-code `1` and is wired
  into `i2pr-netdb::lookup_id` with a wire-code unit test. Plan 119
  ships the structural surface only; the full `DatabaseLookup` state
  machine wiring for LeaseSet2 belongs to Plan 122.
- `LeaseSet2Store` is a bounded in-memory store of
  `ValidatedLeaseSet2` records indexed by `DestinationHash`,
  independent of the `RouterInfoStore` so the two entry classes
  cannot starve each other.

### Phase H — Validation, freshness, and bounded store (closed)

- `ValidatedLeaseSet2` is the only constructor for a validated
  LeaseSet2; it enforces the Plan 119 §10 checklist: length,
  expected-key derivation (using the embedded destination), signature
  verification (using the embedded destination's signing key with
  the `0x03 || signed_bytes` preimage), X25519 policy (at least one
  usable 32-byte X25519 key), and freshness policy
  (`published + expires_offset > current_time_seconds`, with
  `now <= expires_at`).
- `LeaseSet2ValidationPolicy::default_const()` returns the
  ordinary-only policy for use in `const fn` paths.
- `LeaseSet2InsertOutcome` exposes the
  `Inserted { replaced, fresh }`, `Idempotent`, `Conflict`,
  `StaleReplacement`, `CapacityExceeded`, and `Invalid` variants.
  Equal `published` + identical signed bytes is idempotent; equal
  `published` + different bytes is `Conflict`; older `published`
  yields `StaleReplacement`; capacity exceeded is fail-closed.
- Phase H integration tests at
  `crates/i2pr-netdb/tests/lease_set2_integration.rs` cover
  DestinationHash indexing, integration of `ValidatedLeaseSet2` with
  the Plan 119 signature path, typed rejection of an invalid
  signature, typed rejection of a wrong expected key, typed rejection
  of an expired LS2, deterministic replacement idempotency,
  swap-on-different-bytes, capacity isolation from `RouterInfoStore`,
  and the `LeaseSet2StoreStats` contract.

### Phase I — Synchronized authority (closed)

- `README.md`, `AGENTS.md`,
  `docs/architecture/overview.md`,
  `docs/architecture/i2pr-proto.md`,
  `docs/architecture/i2pr-netdb.md`,
  `docs/architecture/i2pr-tunnel.md`,
  `docs/architecture/i2pr-daemon.md`,
  `docs/protocol-support.md`, `specs/support.toml`, and the
  `i2pr-ntcp2-interop` skill all carry the Plan 119 closure marker
  and the Plan 120 next-plan pointer.
- `specs/support.toml` flips `common.leaseset2-family` from
  `deferred` to `experimental` with Plan 119 evidence; the same flip
  is applied to `i2np.store-payloads`.

## Local product floor remains green

```text
Plan 115 Emissary Q0 construction + native OBEP reply (closed)
Plan 116 local tunnel data plane (closed)
Plan 117 local production composition (closed)
Plan 117 native reference (blocked-reference-defect; closed for progression)
Plan 118 planning authority cleanup (closed)
Plan 119 Standard LeaseSet2 protocol foundation (closed)
```

## Required local checks (green on the Plan 119 source floor)

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace --no-fail-fast
cargo +1.95.0 test --locked -p i2pr-proto --all-targets
cargo +1.95.0 test --locked -p i2pr-netdb --all-targets
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
```

The Plan 119 closure records the final result:

```text
cargo +1.95.0 test --locked --workspace --no-fail-fast
... 23 non-empty test result lines, 812 tests passed, 0 failed
```

The pre-existing baseline failure of
`scripts/check-rootless-interop-boundary.sh` is documented in
`AGENTS.md` (the `rootless_supervisor.py` file was retired by the
Plan 099 harness-reduction commit) and is unrelated to Plan 119.

## Next executable plan

```text
Plan 120: destination lifecycle and dedicated tunnel pools
```

See
[`plans/120-m6-destination-lifecycle-and-tunnel-pools.md`](plans/120-m6-destination-lifecycle-and-tunnel-pools.md)
and the Milestone 6 roadmap at
[`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).
Plan 120 will create the first real local destination and use this
protocol foundation to derive and sign its LeaseSet2.