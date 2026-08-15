# Plan 109: Short tunnel-build record and Noise-N conformance correction

- Status: **ready for implementation**
- Date: 2026-08-15
- Parent corrective authority: `plans/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md`
- Corrects: Plan 108 local ECIES-X25519 implementation
- Successor: Plan 110
- Milestone: 5 — exploratory tunnel construction
- Scope class: **single-record protocol conformance; no multi-record/live-network gate**

## 1. Goal

Correct the Plan 108 short tunnel-build wire and cryptographic primitive so that one ECIES-X25519 hop can be represented and processed according to the current official I2P Tunnel Creation Specification.

At Plan 109 closure, i2pr must be able to do all of the following with deterministic test inputs:

1. encode an exact 154-byte short request plaintext;
2. execute the normative Noise `N` request transcript against one hop's static ECIES-X25519 key;
3. produce an exact 218-byte encrypted request record with the required truncated hop hash, ephemeral key, ciphertext, and Poly1305 tag;
4. retain the normative post-request `ck` and `h` state;
5. derive the short-build reply key, AES layer key, IV key, and OBEP-specific continuation material exactly as specified;
6. encode/decode an exact 202-byte short reply plaintext;
7. protect/open a hop's own exact 218-byte reply record using its derived reply key, its record-slot nonce, and saved request transcript hash as associated data;
8. prove these operations against independent fixed expected values rather than only creator/responder self-round-trips.

Plan 109 does **not** assemble or claim a complete standards-conformant multi-record `ShortTunnelBuild` message. That belongs to Plan 110.

## 2. Why Plan 108 must be corrected rather than extended

The current Plan 108 code has useful ownership/state-machine structure, but its protocol semantics are incompatible with I2P. Known defects include:

- role flags encoded as `0x01` / `0x02` instead of `0x80` / `0x40`;
- layer encryption type `0x05` instead of current type `0` (AES);
- millisecond `u64` request/expiration fields at non-normative offsets;
- custom options representation instead of the normal `Mapping` beginning at byte 56;
- request record envelope `ephemeral || custom nonce || AEAD body` instead of `truncated hop hash || ephemeral || ciphertext || tag`;
- custom HKDF labels and out-of-band `request_key_seed` rather than the literal Noise-N state;
- empty AEAD associated data rather than transcript hash `h`;
- fresh X25519 exchange for replies rather than the request-derived `replyKey`;
- custom reply plaintext fields rather than Mapping + padding + response byte;
- response code `1` for rejection rather than the currently defined bandwidth rejection `30`.

Do not preserve these semantics for API compatibility. Where a Plan 108 public/internal type encodes a false protocol model, replace or narrow it.

## 3. Normative sources

Implementation must be derived from:

- `https://i2p.net/en/docs/specs/tunnel-creation-ecies/`
- `https://i2p.net/en/docs/specs/i2np/`
- current Common Structures specification for `Mapping`, `Hash`, and integer encodings;
- primitive references explicitly delegated by those specifications.

Proposal 157 may be used for rationale only. Current final specifications win on conflict.

Before changing code, record in Plan 109 closure notes the `Updated` / `Accurate for` metadata observed from the official pages used during implementation.

## 4. Scope lock

### 4.1 In scope

Plan 109 owns:

- exact 154-byte short request plaintext typed representation and codec;
- exact request flags, reserved fields, layer type, timestamp, expiration, message ID, Mapping, and padding rules;
- exact hop truncated-hash calculation/input semantics;
- exact 218-byte encrypted request record layout;
- Noise-N initialization, `MixHash`, `MixKey`, X25519 `es`, ChaChaPoly request encryption/decryption, and transcript continuation;
- unique per-hop ephemeral key generation for the one-record primitive;
- short-record KDF derivation from post-request `ck`;
- typed secret ownership for `replyKey`, `layerKey`, `ivKey`, and applicable OBEP garlic key/tag;
- exact 202-byte short reply plaintext representation and codec;
- exact 218-byte hop-own reply encryption/decryption;
- one-record responder-side open/seal support sufficient to test creator/responder roles independently;
- replacement/removal of Plan 108 custom seed/nonce/session-label semantics;
- independent deterministic fixtures for wire and crypto intermediate values;
- regression tests that prevent the known Plan 108 wire layouts from reappearing;
- narrowly required documentation/support-registry downgrades or corrections.

### 4.2 Explicitly out of scope

Plan 109 must not implement:

- randomized multi-record slot assignment;
- fake build records;
- inbound originator fake-record integrity;
- iterative ChaCha20 preprocessing of other records;
- complete `ShortTunnelBuild` / `OutboundTunnelBuildReply` payload framing;
- live mixed-router delivery;
- NTCP2 activation or repair;
- SSU2;
- generic I2NP router dispatch;
- garlic wrapping of build messages;
- NetDB live lookup;
- transit tunnel participation;
- client/destination tunnel pools;
- streaming/SAM/I2CP/LeaseSet work;
- new Python harnesses or privileged environment machinery.

If a multi-record requirement becomes necessary to define a parameter (for example the reply record's slot nonce), represent that parameter explicitly and test it for valid `0..=7`; do not implement Plan 110's slot allocator here.

## 5. Preserve useful Plan 108 architecture

Prefer retaining these concepts if they remain semantically correct after repair:

- runtime-neutral `i2pr-tunnel` ownership;
- `BuildCryptography`-like abstraction, if revised to carry the required Noise context instead of lossy custom seeds;
- `ShortBuildStateMachine` lifecycle;
- typed `ShortBuildConstructionError` categories;
- `ShortBuildRegistrar` success-only behavior;
- explicit injected RNG/time;
- zeroizing secret wrappers.

Do not retain an API merely because tests currently use it. Protocol correctness has priority.

## 6. Work package A — replace the short request plaintext model

### 6.1 Canonical field layout

Implement the exact 154-byte plaintext layout:

```text
bytes 0..3     receive tunnel ID, nonzero u32 BE
bytes 4..7     next tunnel ID, nonzero u32 BE
bytes 8..39    next router identity hash, 32 bytes
byte  40       flags
bytes 41..42   additional flags, MUST be zero
byte  43       layer encryption type
bytes 44..47   request time, minutes since Unix epoch, rounded down, u32 BE
bytes 48..51   request expiration in seconds since creation, u32 BE
bytes 52..55   next message ID, u32 BE
bytes 56..x    tunnel build options Mapping
bytes x..153   option-defined data if any, then random padding
```

### 6.2 Role flags

Use exact values:

```rust
Participant     = 0x00
InboundGateway  = 0x80
OutboundEndpoint = 0x40
```

Reject:

- both IBGW and OBEP bits set;
- any undefined bits 5..0;
- nonzero additional flag bytes 41 or 42 when decoding.

Do not encode role as low-order bits.

### 6.3 Layer encryption type

Current supported value:

```text
0 = AES
```

Do not emit `0x05`.

Unknown values must be rejected for construction. Decoder behavior should be fail-closed unless the current spec explicitly requires otherwise.

### 6.4 Time and expiration

Replace Plan 108's millisecond `u64` wire representation.

Request time wire value:

```text
floor(unix_seconds / 60) as u32
```

Provide an explicit conversion boundary from the repository's internal time representation. Reject overflow rather than truncate.

Current supported expiration:

```text
600 seconds
```

Do not interpret this field as an absolute timestamp.

### 6.5 Mapping and padding

Use the existing canonical `Mapping` codec directly.

Rules:

- empty Mapping encodes to exactly two bytes `00 00`;
- Mapping total encoded length <= 98 bytes;
- Mapping length field <= 96;
- no custom one-byte Mapping-length field;
- remaining record bytes are random padding, supplied through injected randomness;
- padding must not be implicitly all-zero in production construction;
- deterministic test RNG is acceptable for fixtures.

If currently defined bandwidth options (`m`, `r`, `l`) are already represented in reusable NetDB/common code, reuse canonical Mapping semantics only. Do not add bandwidth-policy behavior to this plan.

### 6.6 Creator ephemeral public key note

The current Tunnel Creation specification states that a creator ephemeral public key is included in an inbound-build plaintext context for IBGW layer/reply derivation but does not assign an unconditional fixed offset in the basic table.

Do not invent an offset.

For Plan 109:

- implement the fixed table and generic bounded Mapping/option-defined-data region exactly;
- document the unresolved/conditional inbound creator-key placement if it is not already unambiguously represented by the authoritative spec corpus in `specs/`;
- defer originator-fake/inbound message-level use to Plan 110;
- if this detail blocks a correct single-record IBGW fixture, stop and resolve it with a narrow spec note before proceeding.

## 7. Work package B — define protocol-accurate request crypto state

The current `request_key_seed` model is not normative. Replace it with a typed per-record Noise state.

A representative internal model:

```rust
struct TunnelBuildNoiseState {
    h: Zeroizing<[u8; 32]>,
    ck: Zeroizing<[u8; 32]>,
}

struct PendingHopBuildSecrets {
    reply_key: Zeroizing<[u8; 32]>,
    layer_key: Zeroizing<[u8; 32]>,
    iv_key: Zeroizing<[u8; 32]>,
    request_hash: [u8; 32],
    // OBEP-only garlic material if applicable
}
```

Exact names are discretionary. Ownership semantics are not.

### 7.1 Noise initialization

Implement literally:

1. protocol name = ASCII `Noise_N_25519_ChaChaPoly_SHA256` (31 bytes);
2. initialize `h` by padding protocol name to 32 bytes, not hashing it;
3. set `ck = h`;
4. null-prologue `MixHash`: `h = SHA256(h)`.

Add fixture tests for the exact initial `h` and `ck` values.

### 7.2 Peer static key mix

For hop static X25519 public key `hepk`:

```text
h = SHA256(h || hepk)
```

Validate peer key material and reject unsupported/non-X25519 hops before expensive operations where possible.

### 7.3 Sender ephemeral key

Generate one fresh X25519 ephemeral private key for the record through the injected cryptographic RNG.

Derive `sepk` and:

```text
h = SHA256(h || sepk)
```

Ephemeral public key is serialized in the encrypted request record in X25519's specified little-endian representation.

The product API must not accept a caller-supplied arbitrary `request_key_seed` as a substitute for the ephemeral private key/Noise state.

### 7.4 `es` and MixKey

Compute:

```text
sharedSecret = X25519(sesk, hepk)
keydata = HKDF(ck, sharedSecret, "", 64)
ck = keydata[0..32]
k  = keydata[32..64]
```

Reject all-zero DH result.

The existing `i2pr-crypto` HKDF helper may be reused if its parameter ordering and semantics exactly match the spec. Add explicit tests proving this use, rather than assuming the generic helper is correct because RFC5869 tests pass.

### 7.5 Request AEAD

Use ChaCha20-Poly1305 as Noise ChaChaPoly:

```text
key = k
nonce counter = 0
plaintext = exact 154-byte short request plaintext
associated data = current h
```

Do not:

- derive/store a separate 16-byte request nonce;
- use empty associated data;
- prepend a nonce to the record;
- use custom protocol labels.

After encryption:

```text
h = SHA256(h || ciphertext_and_tag)
```

Retain the post-request `h` and `ck` for reply/KDF use.

## 8. Work package C — exact encrypted request record

Implement an exact-size typed wrapper for the 218-byte encrypted request record.

Layout:

```text
0..16    truncated hop identity hash (first 16 bytes)
16..48   sender ephemeral X25519 public key
48..202  154-byte request ciphertext
202..218 16-byte Poly1305 tag
```

Construction must require the full hop RouterIdentity Hash or a validated typed truncated-hash value derived from it.

Requirements:

- no caller may supply an unrelated arbitrary 16-byte prefix without an explicit test-only constructor;
- responder checks the prefix before DH where practical;
- exact 218-byte invariant is type-enforced where possible;
- mutation of prefix, ephemeral key, ciphertext, tag, or associated Noise state causes rejection;
- do not expose private ephemeral key in the returned wire record.

Add a regression test proving bytes `0..16` are the expected hop hash prefix and bytes `16..48` are the sender ephemeral key.

## 9. Work package D — derive short-build key material from `ck`

After request AEAD and transcript update, derive the protocol material in exact order.

### 9.1 Reply key

```text
keydata = HKDF(ck, ZEROLEN, "SMTunnelReplyKey", 64)
ck       = keydata[0..32]
replyKey = keydata[32..64]
```

### 9.2 AES layer key and non-OBEP IV key

```text
keydata  = HKDF(ck, ZEROLEN, "SMTunnelLayerKey", 64)
layerKey = keydata[32..64]
```

For non-OBEP:

```text
ivKey = keydata[0..32]
```

### 9.3 OBEP continuation

For OBEP:

```text
ck = keydata[0..32]
keydata = HKDF(ck, ZEROLEN, "TunnelLayerIVKey", 64)
ck = keydata[0..32]
ivKey = keydata[32..64]

keydata = HKDF(ck, ZEROLEN, "RGarlicKeyAndTag", 64)
garlicReplyKey = keydata[32..64]
garlicReplyTag = required protocol prefix from keydata[0..]
```

Use the exact tag length specified by the current final specification.

Do not implement garlic wrapping; only retain the required derived material as a typed secret for later use.

### 9.4 Secret ownership

Derived secrets must:

- be zeroized on drop;
- not derive `Clone` unless ownership cannot otherwise be represented;
- redact `Debug`;
- have no serde implementation;
- not be logged;
- not be copied into generic byte vectors longer than necessary.

The state machine should retain the post-request transcript hash `h` as non-secret authenticated context and secret key material separately.

## 10. Work package E — replace short reply plaintext model

Delete/rework the Plan 108 reply fields `expiration`, `next_message_id`, and `tunnel_id` from the wire plaintext unless a current normative option explicitly defines them.

Exact 202-byte plaintext:

```text
0..x      canonical Mapping
x..201    random padding / defined option data
201       reply byte
```

Rules:

- Mapping total encoded length <= 201 bytes;
- Mapping length field <= 199;
- empty Mapping = `00 00`;
- padding fills through byte 200;
- byte 201 is response;
- accepted response `0x00`;
- current defined rejection `30`;
- other response values fail closed unless final spec explicitly allows them.

Padding comes from injected RNG for construction. Decoding does not require padding to be zero.

## 11. Work package F — exact hop-own reply AEAD

The hop's own encrypted short reply record is exactly 218 bytes:

```text
0..202    ciphertext
202..218  Poly1305 tag
```

Do not generate a new reply ephemeral X25519 keypair.

Do not prepend a nonce.

Use:

```text
key = derived replyKey
nonce = caller-supplied record slot 0..=7, encoded according to I2P ChaChaPoly nonce convention
associated data = saved post-request h
plaintext = exact 202-byte reply plaintext
```

Expose an explicit `RecordSlot` / equivalent validated type whose valid domain is `0..=7`.

Tests must prove:

- slot 0 and slot 7 work;
- slot 8 is rejected before crypto;
- wrong slot fails authentication;
- wrong reply key fails authentication;
- wrong saved `h` fails authentication;
- modified ciphertext/tag fails authentication;
- output length is exactly 218.

## 12. Work package G — repair the Plan 108 state-machine crypto context

Update the existing state machine only as far as necessary to carry correct per-hop cryptographic state into Plan 110.

Remove concepts that are no longer protocol-valid:

- `HopCryptoSeed` as an out-of-band request key seed;
- `session_digest` if it does not correspond to a normative retained value;
- creator static X25519 reply private key if it exists only to support Plan 108's non-normative fresh reply DH;
- custom 16-byte request/reply nonce storage;
- any API whose documentation claims reply encryption performs another X25519 exchange.

Replace them with a pending per-hop context containing at least:

- hop identity/hash;
- role;
- sender ephemeral public key if needed for diagnostics/fixtures;
- saved post-request transcript hash `h`;
- `replyKey`;
- `layerKey`;
- `ivKey`;
- OBEP garlic reply material when applicable;
- record slot parameter placeholder, initially unset until Plan 110 assigns slots.

Plan 109's state machine may stop at a `PreparedRecords` / equivalent local state. Do not pretend complete message assembly exists.

## 13. Work package H — independent conformance fixtures

This package is mandatory. Self-round-trip tests alone are insufficient.

### 13.1 Fixture provenance

Create a compact fixture format under the existing specs/test-fixture conventions containing:

- fixture name and version;
- source specification URL/version metadata;
- fixed hop static private/public test key;
- fixed sender ephemeral private/public test key;
- fixed RouterIdentity hash;
- exact request plaintext bytes;
- initial `h` and `ck`;
- `h` after peer static mix;
- `h` after ephemeral mix;
- DH result;
- post-MixKey `ck`;
- request AEAD key;
- request ciphertext/tag;
- post-ciphertext `h`;
- derived reply/layer/IV keys;
- exact reply plaintext;
- exact encrypted reply for at least one slot.

No production secret is used.

### 13.2 Independence requirement

Expected bytes must not be generated at test runtime by calling the same product functions under test.

Use one of:

- a test-only literal step-by-step reference implementation using low-level primitives without invoking `EciesX25519BuildCryptography`;
- precomputed fixture values with documented independent provenance;
- independently derived manual/intermediate vector values.

If using a test-only reference implementation, keep it small and specification-shaped; do not build another router or harness.

### 13.3 Failure behavior

Any disagreement between fixture and product code is a Plan 109 failure until resolved. Do not update expected fixture bytes merely to match product output without re-derivation from the normative algorithm.

## 14. Work package I — tests

At minimum, add/repair tests for:

### Request plaintext

- exact offsets of every fixed field;
- `0x80`, `0x40`, `0x00` role values;
- `0xC0` rejected;
- lower reserved flag bits rejected;
- bytes 41–42 zero;
- layer type 0 accepted;
- Plan 108 layer type `0x05` rejected;
- request minutes conversion exact;
- expiration exactly 600 encoded;
- Mapping empty bytes exactly `00 00` at offset 56;
- Mapping maximum accepted/rejected boundary;
- deterministic nonzero/random padding from test RNG;
- receive/next tunnel ID zero rejected.

### Noise request

- exact initial `h` and `ck` fixture;
- exact peer-static MixHash fixture;
- exact ephemeral MixHash fixture;
- exact DH fixture;
- exact MixKey/HKDF fixture;
- AEAD with `ad = h` fixture;
- post-ciphertext `h` fixture;
- all-zero DH rejected;
- mutated hop static/ephemeral/ciphertext/tag rejected.

### Encrypted request envelope

- exact truncated hash prefix;
- exact ephemeral key offset;
- exact ciphertext/tag offsets;
- exact total 218 bytes;
- Plan 108 `32 + 16 + 170` envelope regression rejected.

### Derived keys

- exact `SMTunnelReplyKey` fixture;
- exact `SMTunnelLayerKey` fixture;
- non-OBEP IV fixture;
- OBEP IV continuation fixture;
- OBEP garlic material fixture.

### Reply plaintext

- empty Mapping at bytes 0–1;
- padding through byte 200;
- response at byte 201;
- accept 0;
- reject 30;
- Plan 108 reject 1 rejected;
- malformed Mapping rejected.

### Reply AEAD

- exact fixture for slot 0;
- at least one nonzero slot fixture;
- wrong slot/key/h/tag rejected;
- exact total 218 bytes;
- no ephemeral key/nonce prefix present.

## 15. Dependency policy

Prefer existing dependencies:

- `sha2`;
- `hmac` / existing HKDF helper;
- `x25519-dalek` / existing X25519 wrapper;
- `chacha20poly1305`;
- `zeroize`;
- existing `Mapping` codec.

Do not add a new Noise framework crate merely to implement this small one-way pattern unless a concrete review shows the current primitive composition cannot be implemented safely. A small protocol-specific state type is preferable.

No OpenSSL, libsodium, FFI crypto, or system crypto dependency.

Maintain workspace MSRV and `forbid(unsafe_code)` policy.

## 16. Validation commands

Run the repository's normal local validation surface appropriate to a Rust protocol change:

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
git diff --check
```

Do not make the obsolete Plan 046 rootless checker a Plan 109 gate. Record the known baseline issue if still present.

Do not add an external live-interop CI job.

## 17. Documentation/support status during Plan 109

Until Plan 110 closes, documentation must not claim full short-build message conformance.

Permitted status after Plan 109:

```text
short request/reply record codec       = locally conformant
single-record Noise-N request crypto   = locally conformant
single-record reply crypto             = locally conformant
short-build derived keys               = locally conformant
multi-record preprocessing             = pending Plan 110
complete STBM/OTBRM payload             = pending Plan 110
live mixed-router build                = blocked
```

If `specs/support.toml` currently claims a broader feature as implemented, split or downgrade the feature entry rather than leaving a false positive.

## 18. Acceptance criteria

Plan 109 passes only when **all** are true.

### Wire format

- [ ] Request plaintext is exactly 154 bytes and matches every normative offset.
- [ ] Role flags are `0x80`/`0x40`/`0x00`, with invalid combinations/bits rejected.
- [ ] Layer encryption type is 0 for current AES tunnels.
- [ ] Request time is encoded as minutes since epoch in u32 BE.
- [ ] Expiration is encoded as seconds since creation and current construction uses 600.
- [ ] Mapping begins at byte 56 using canonical two-byte Mapping length semantics.
- [ ] Remaining request bytes are padding, not custom fields invented by Plan 108.
- [ ] Encrypted request is exactly 218 bytes: 16-byte hash prefix + 32-byte ephemeral + 154-byte ciphertext + 16-byte tag.
- [ ] Reply plaintext is exactly 202 bytes with Mapping first and response byte at 201.
- [ ] Reply codes 0 and 30 are supported; Plan 108 code 1 is not emitted.
- [ ] Encrypted hop-own reply is exactly 218 bytes with no fresh ephemeral/nonce prefix.

### Cryptography

- [ ] Noise protocol name and initial `h`/`ck` are exact.
- [ ] Null prologue, peer-static MixHash, ephemeral MixHash, `es`, and MixKey ordering are exact.
- [ ] Request AEAD uses nonce 0 and associated data `h`.
- [ ] Ciphertext is mixed into `h` after encryption/decryption.
- [ ] `SMTunnelReplyKey` derivation is exact.
- [ ] `SMTunnelLayerKey` derivation is exact.
- [ ] Non-OBEP and OBEP IV derivation paths are exact.
- [ ] OBEP garlic reply material is derived exactly but not yet used for wrapping.
- [ ] Reply AEAD uses derived reply key, validated record slot, and saved request `h`.
- [ ] No custom Plan 108 request/reply seed or nonce derivation remains on the production path.

### Evidence

- [ ] At least one complete request fixture verifies intermediate Noise values and final 218-byte record.
- [ ] At least one complete reply fixture verifies derived keys and final 218-byte reply record.
- [ ] Expected fixture values are independent of the product primitive under test.
- [ ] Negative mutation tests fail closed.

### Architecture

- [ ] `i2pr-tunnel` remains runtime/transport neutral.
- [ ] Secrets are zeroized/redacted and not serialized/logged.
- [ ] No normal-daemon NTCP2 behavior changes.
- [ ] No Python/live-network harness added.
- [ ] Full workspace checks pass, excluding only explicitly documented pre-existing obsolete baseline scripts.

## 19. Closure record

On success, create `plans/109-status.md` with:

```text
plan_109 = passed-record-and-noise-conformance
plan_108 = superseded-local-architecture-retained-wire-crypto-corrected
single_record_short_build_crypto = locally-conformant
short_build_derived_keys = locally-conformant
multirecord_short_build_message = pending-plan110
external_build_delivery = unavailable
live_mixed_router_build = blocked-on-plan110-and-qualified-delivery
```

Record:

- commit SHA;
- exact fixture provenance;
- test counts;
- dependency changes, if any;
- any conditional inbound creator-key issue deferred to Plan 110.

Do not state `interoperable`, `wire-proven-live`, or `mixed-router-passed`.

## 20. Handoff to Plan 110

Plan 110 may start only after the Plan 109 closure demonstrates that an independently checked one-hop request/reply cryptographic record is byte-for-byte consistent with the current official I2P specification.

The input Plan 110 receives is:

- canonical request plaintext encoder;
- canonical encrypted request primitive;
- canonical reply plaintext encoder/decoder;
- canonical own-reply encryption/decryption;
- retained `replyKey`, `layerKey`, `ivKey`, and request `h` per hop;
- validated `RecordSlot` parameter surface;
- no assumption that path order equals record slot order.