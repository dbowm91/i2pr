# Plan 119 — Milestone 6 LeaseSet2 protocol foundation

## Status

- **Ready after Plan 118 closes**.
- Date: **2026-08-19**.
- Parent roadmap: [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).
- Scope: protocol/common-structure, validation, and NetDB ownership only.
- Does **not** create a local destination runtime, destination tunnel pools, or
  Garlic encryption.

## Objective

Implement the modern ordinary destination NetDB object that Milestone 6 will
build on: Standard LeaseSet2 (DatabaseStore type 3), including `Lease2`, the
LeaseSet2 header, typed encryption-key list, strict canonical validation,
signature-domain handling, and bounded NetDB storage/retrieval.

The existing repository already has:

```text
Destination structural codecs
classic Lease / LeaseSet
CryptoKeyType::X25519 == type 4
X25519 / Ed25519 / SHA-256 primitives
DatabaseStore / DatabaseLookup machinery
bounded NetDB storage and validation patterns
```

The existing classic LeaseSet implementation deliberately rejects type 3/5/7
as deferred. Plan 119 replaces only the type-3 deferral with a complete,
strictly validated Standard LeaseSet2 path. Classic LeaseSet remains available
as compatibility structure; EncryptedLeaseSet and MetaLeaseSet remain deferred.

---

# 1. Normative references and minimum supported subset

Primary references:

```text
https://i2p.net/en/docs/specs/common-structures/
https://i2p.net/en/docs/specs/i2np/
https://i2p.net/en/docs/specs/ecies/
```

Implement the ordinary published online-signed Standard LeaseSet2 subset.

Required support:

```text
DatabaseStore type                  3
Lease                              Lease2
published timestamp                supported
expires offset                     supported
flags                              strict validation
options Mapping                    canonical strict encoding
one or more encryption keys        supported
X25519 encryption key type 4       required
multiple known/unknown typed keys  parse safely according to declared lengths
lease count                         bounded
signature                          required and verified
key                                SHA256(Destination)
```

Deferred in this plan:

```text
EncryptedLeaseSet (type 5)
MetaLeaseSet (type 7)
blinded destination semantics
offline-signing generation
PQ-hybrid destination crypto
legacy ElGamal session implementation
Garlic encryption/session state
```

For unsupported flags that materially change verification semantics, fail with a
typed unsupported/policy error rather than accepting an object with incomplete
validation.

---

# 2. Proposed code ownership

Prefer a clear common-structure split instead of continuing to grow
`common/lease.rs` indefinitely.

Recommended layout:

```text
crates/i2pr-proto/src/common/lease.rs
    classic Lease / LeaseSet retained

crates/i2pr-proto/src/common/lease2.rs
    Lease2
    LeaseSet2Flags
    LeaseSet2Header
    LeaseSet2EncryptionKey
    LeaseSet2
```

If repository conventions strongly favor one file, equivalent organization is
acceptable, but public types must remain explicit and testable.

NetDB changes should remain in `i2pr-netdb`; daemon composition is not needed in
this plan.

---

# 3. Phase A — implement `Lease2`

Add a structural `Lease2` matching the Standard LeaseSet2 layout:

```text
tunnel gateway   32-byte router hash
tunnel id         4-byte unsigned integer
end date           4-byte unsigned seconds since epoch
```

Do not reuse the classic eight-byte `Date` representation internally in a way
that obscures the wire width. A dedicated seconds-since-epoch value or explicit
`u32` newtype is preferable.

Required properties:

- exact 40-byte wire structure;
- strict full-input consumption;
- explicit conversion helpers to wider internal time types with checked
  arithmetic;
- tunnel ID policy remains separate from raw structural parsing where special
  values are allowed by the common structure;
- expiry comparison uses deterministic caller-supplied time in validation
  layers; no wall-clock calls inside the codec.

Tests:

```text
lease2_exact_wire_length
lease2_round_trip_known_values
lease2_big_endian_tunnel_id
lease2_big_endian_end_date
lease2_reject_trailing_bytes
lease2_checked_time_conversion
```

---

# 4. Phase B — implement LeaseSet2 header

Implement the Standard LeaseSet2 common header using the current official common
structures specification.

The header must represent at least:

```text
Destination
published timestamp (seconds)
expires offset
flags
optional offline-signature section when the flag is structurally observed
```

For the initial supported policy:

- ordinary online-signed published LS2 is accepted;
- reserved flag bits must be zero;
- if offline-signing semantics are not implemented completely in this plan,
  the offline-signature flag is parsed only far enough to return a precise
  `UnsupportedOfflineLeaseSet2Signing`/equivalent error; do not verify the final
  signature with the wrong key;
- unpublished/blinded flags must be either implemented according to the exact
  specification or rejected as unsupported policy for this first ordinary
  destination path;
- expiration arithmetic must be checked;
- published time and expires offset must be exposed separately so NetDB policy
  can make freshness decisions without reparsing bytes.

Keep codec correctness and NetDB freshness policy separate.

Required tests:

```text
ls2_header_round_trip_online_published
ls2_header_reserved_flags_rejected
ls2_header_unsupported_offline_flag_is_explicit
ls2_header_expiration_checked
ls2_header_trailing_data_rejected
```

---

# 5. Phase C — encryption-key list

Standard LeaseSet2 may carry one or more typed encryption public keys.

Introduce a type that retains:

```rust
struct LeaseSet2EncryptionKey {
    key_type: CryptoKeyType,
    bytes: Vec<u8>,
}
```

or the closest repository-style equivalent.

The on-wire structure includes type and declared key length. Parsing therefore
must not assume every key has the length implied by a currently supported type
before consuming the record safely.

Policy:

- `CryptoKeyType::X25519` is required and must have exactly 32 bytes;
- a locally created ordinary LS2 for Plans 120+ must include at least one usable
  X25519 type-4 key;
- known key types with invalid lengths fail closed;
- unknown key types may be retained as bounded opaque typed values only if the
  common structures specification permits forward parsing through the declared
  length; unknown keys are never considered usable for routing/encryption;
- total key count and aggregate key bytes are explicitly bounded;
- duplicate key-type policy is explicit; do not silently pick an arbitrary key.

Suggested validation API:

```text
LeaseSet2::usable_encryption_key(CryptoKeyType::X25519)
  -> Result<&PublicKey, LeaseSet2KeySelectionError>
```

or equivalent deterministic selector.

Tests must cover:

```text
one_x25519_key_round_trip
multiple_typed_keys_parse
unknown_key_retained_bounded_but_not_usable
x25519_wrong_length_rejected
zero_keys_rejected_for_ordinary_supported_policy
duplicate_x25519_policy_is_deterministic
aggregate_key_bytes_bounded
```

---

# 6. Phase D — canonical options Mapping

LeaseSet2 includes an options Mapping. Signature verification requires a stable
byte representation.

The common structures specification requires multi-entry options mappings to be
sorted by key for invariant signatures.

Use the repository's existing Mapping codec only if it can prove canonical key
ordering for signed structures. If it currently retains arbitrary input order,
add a signed-structure canonicalization boundary rather than changing unrelated
Mapping behavior globally without audit.

Required invariants:

```text
locally generated LS2 options are canonical
non-canonical signed input is either rejected or normalized only before signing,
never normalized before verifying an existing signature
```

For received LS2, signature verification must cover the exact signed bytes as
received. Do not decode -> reorder -> re-encode -> verify.

Tests:

```text
ls2_generated_options_sorted
received_ls2_signature_uses_original_bytes
noncanonical_mapping_does_not_gain_valid_signature_by_reencoding
```

---

# 7. Phase E — Standard `LeaseSet2` codec

Implement a complete Standard LeaseSet2 value containing:

```text
header
options
one or more typed encryption keys
lease count
Lease2 list
signature
exact retained signed region
```

The signature domain is critical.

For Standard LeaseSet2 in a DatabaseStore type-3 record, the signed material is:

```text
single byte 0x03 || LeaseSet2 bytes before Signature
```

Do not reuse the classic LeaseSet signature domain.

The decoded type should retain enough original bytes to verify without
round-trip canonicalization ambiguity.

Suggested API shape:

```text
LeaseSet2::decode(...)
LeaseSet2::encode_to_vec(...)
LeaseSet2::signature_preimage()
LeaseSet2::destination()
LeaseSet2::published_seconds()
LeaseSet2::expires_seconds()
LeaseSet2::encryption_keys()
LeaseSet2::leases()
LeaseSet2::signature()
```

Use repository-native typed error conventions.

Bounds must include:

```text
maximum total LS2 bytes
maximum options bytes
maximum encryption-key count
maximum aggregate encryption-key bytes
maximum lease count
```

Do not allocate based solely on an untrusted declared count.

---

# 8. Phase F — signature verification and local construction

Add verification using the Destination signing key for the supported online
subset.

Rules:

```text
key hash = SHA256(encoded Destination)
signature key type = Destination signing key type
signature preimage begins with 0x03
verification uses exact retained signed bytes
```

Add a local constructor/builder suitable for Plan 120, but keep key ownership
outside `i2pr-proto`.

The proto builder receives already-owned public values and a signature or a
signing callback at a higher layer; protocol structures should not own private
signing keys.

Preferred separation:

```text
i2pr-proto: encode unsigned LS2 / expose signature preimage
i2pr-crypto or client layer: sign preimage
proto: attach typed signature and finalize
```

Avoid adding client lifecycle to proto.

Tests must include a frozen or independently generated known-good fixture, not
only self-round-trips.

---

# 9. Phase G — DatabaseStore type-3 integration

Inspect the current `DatabaseStore` representation and replace the type-3
`DeferredPayload` path with a typed Standard LeaseSet2 carrier.

Target conceptual enum:

```text
DatabaseStoreRecord::RouterInfo(...)
DatabaseStoreRecord::LeaseSet(...)
DatabaseStoreRecord::LeaseSet2(...)
Deferred / unsupported type 5
Deferred / unsupported type 7
```

Exact naming may follow existing code.

Requirements:

- the DatabaseStore key must equal the destination hash for LS2;
- type 3 must be encoded/decoded without hand-built bytes;
- type 5/7 remain explicit unsupported/deferred variants;
- malformed type 3 never falls back to opaque success;
- RouterInfo and classic LeaseSet behavior are unchanged.

Add I2NP-level round-trip tests with both standard and short-transport headers
where those existing envelopes are supported.

---

# 10. Phase H — NetDB validation/store support

Extend `i2pr-netdb` to own validated Standard LeaseSet2 records.

Do not force RouterInfo and LS2 into one undifferentiated store if their policy
is materially different. A typed `NetDbEntry` or parallel bounded store is
acceptable.

Minimum validation:

```text
signature valid
DatabaseStore key matches Destination hash
ordinary supported LS2 flags/policy
at least one usable X25519 encryption key
lease count nonzero and bounded
lease expirations sane and not all expired
published/expires fields checked with caller-supplied now
entry total size bounded
```

Define replacement/freshness semantics using the `published` field. A newer
entry may replace an older one; stale or duplicate state must be deterministic.
Do not infer LS2 version solely from the earliest lease as classic LeaseSet did.

Storage must remain bounded independently from RouterInfo count/bytes if that is
needed to prevent one entry class starving the other.

Required tests:

```text
valid_ls2_stores_by_destination_hash
invalid_signature_rejected
wrong_database_store_key_rejected
expired_ls2_rejected_or_not_routable_per_policy
newer_published_replaces_older
older_published_does_not_replace_newer
duplicate_is_idempotent
routerinfo_capacity_not_corrupted_by_ls2
```

---

# 11. Phase I — lookup/publication contract readiness

Do not yet build the destination runtime. Expose the minimum typed contracts
that Plans 120/122 can consume.

Required capabilities:

```text
lookup target can distinguish RouterInfo vs LeaseSet2 intent
validated LS2 can be retrieved by Destination hash
publication coordinator can accept a typed locally generated LS2 record or a
narrow extension point exists for Plan 122
```

If the current NetDB lookup state machine is RouterInfo-specific, add a typed
query class without duplicating the whole state machine. Keep response handling
strict by requested entry type.

Do not yet route a Garlic message.

---

# 12. Validation commands

At minimum:

```bash
cargo fmt --all --check
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
```

Use the repository-pinned Rust toolchain command form where required.

No external router, socket, namespace, VM, or Python harness is required for
this plan.

---

# 13. Explicit acceptance criteria

Plan 119 is complete only when:

- [ ] `Lease2` is a strict exact 40-byte structural codec.
- [ ] Standard online-signed LeaseSet2 type 3 is fully encoded/decoded.
- [ ] The LS2 signature domain prepends DatabaseStore type byte `0x03`.
- [ ] Existing signed bytes are verified without decode/re-encode mutation.
- [ ] X25519 crypto type 4 is accepted as a 32-byte LS2 encryption key.
- [ ] Locally supported ordinary LS2 requires at least one usable X25519 key.
- [ ] Unknown/unsupported encryption keys cannot be selected accidentally.
- [ ] Options canonicalization for generated signed structures is deterministic.
- [ ] Reserved/unsupported flags fail with explicit typed errors.
- [ ] DatabaseStore type 3 owns a typed LeaseSet2, not a deferred opaque payload.
- [ ] Type 5 and 7 remain explicitly deferred/unsupported.
- [ ] NetDB verifies signature, key/hash ownership, freshness, bounds, and leases.
- [ ] A validated LS2 can be stored/retrieved by Destination hash.
- [ ] Newer `published` entries replace older ones deterministically.
- [ ] Classic LeaseSet and RouterInfo regressions remain green.
- [ ] At least one independent/frozen LS2 fixture verifies successfully.
- [ ] No private destination key ownership, Garlic session state, or destination
      tunnel pool is introduced into `i2pr-proto` or `i2pr-netdb`.
- [ ] Workspace tests/clippy/docs and dependency-direction checks pass.

## Handoff

On closure:

```text
plan_119 = passed-leaseset2-protocol-foundation
next = plans/120-m6-destination-lifecycle-and-tunnel-pools.md
```

Plan 120 may then create the first real local destination and use this protocol
foundation to derive and sign its LeaseSet2.