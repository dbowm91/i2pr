# Plan 111: Final local short-build conformance correction

- Status: **ready for implementation**
- Date: 2026-08-15
- Parent authority: `plans/102-amendment-exploratory-tunnel-dependency.md`
- Corrects: Plan 109 and Plan 110 conformance claims
- Predecessors: `plans/109-status.md`, `plans/110-status.md`
- Milestone: 5 — exploratory tunnel construction
- Scope class: **narrow local protocol-correction pass; no live-network acceptance gate**

## 1. Goal

Correct the remaining protocol defects in the Plan 109/110 short-tunnel-build implementation before any external-delivery or mixed-router validation is attempted.

This plan is intentionally narrow. It must preserve the useful Plan 109/110 architecture — typed short records, runtime-neutral build state machine, randomized record slots, fake records, multi-record preprocessing/postprocessing, exact one-byte-count STBM/OTBRM framing, and success-only pool registration — while replacing the remaining incorrect protocol semantics.

At Plan 111 closure, i2pr must have a locally standards-conformant all-ECIES short-build construction surface for the current official I2P ECIES-X25519 Tunnel Creation Specification, with independently fixed evidence sufficient to justify moving to a later **qualified external-delivery** checkpoint.

Plan 111 does **not** perform that external-delivery checkpoint.

## 2. Why this pass exists

Plan 109 and Plan 110 corrected most of the Plan 108 wire model, but a post-implementation audit against the current official specification found a small set of high-impact defects that make the existing `conformant = true` / `passed-*-conformance` claims premature.

Known defects at the Plan 111 start point:

1. **Noise-N null-prologue state is incomplete.**
   The current implementation initializes `h = ck = protocol_name || 0` but does not apply the required null-prologue `MixHash`, i.e. `h = SHA256(h)`, before `MixHash(hop_static_pub)`.

2. **The request `es` KDF is split incorrectly.**
   The current implementation performs `MixKey(shared)` to retain only the new `ck`, then runs a second empty-input HKDF to derive the request AEAD key. The specification requires one `HKDF(ck, sharedSecret, "", 64)` operation whose first 32 bytes become the new chaining key and whose second 32 bytes are the request AEAD key.

3. **Record-slot nonce/IV placement is wrong.**
   `ValidatedRecordSlot::nonce()` and raw-ChaCha20 record transforms currently place the slot byte at index 11. The current I2P specification places the record number in byte **4** of the 12-byte IV/nonce (`iv[4] = n`, all other bytes zero) because the eight-byte little-endian nonce occupies bytes 4–11.

4. **OBEP garlic reply tag size is wrong.**
   The current `LayerKeys` model retains a 16-byte garlic tag prefix. The current KDF defines an **8-byte** `garlicReplyTag = keydata[0:7]` and a 32-byte `garlicReplyKey = keydata[32:63]`.

5. **Inbound creator ephemeral-key semantics are absent.**
   The short request specification requires the creator ECIES ephemeral public key in the plaintext record for an inbound tunnel build because the IBGW layer has no DH at that layer. The current request model has no explicit conditional representation for this material.

6. **Per-hop receive/next tunnel IDs are not modeled correctly.**
   `ShortBuildPath` currently exposes one creator tunnel ID, while `MultiRecordHopSpec` has no explicit receive/next tunnel IDs. `build_hop_request_plaintext()` therefore reuses the creator ID and synthesizes a next tunnel ID from the first four bytes of the next router hash. Tunnel IDs and router hashes are independent protocol fields and must not be derived from one another.

7. **The simulated hop role is flattened.**
   `MessageHopProcessor` accepts an OBEP indicator but the current helper returns `Participant` unconditionally. The responder path therefore does not exercise the actual OBEP KDF continuation.

8. **The Plan 109 fixture is not a sufficient independent oracle.**
   `ReferenceFixture::canonical()` obtains expected post-request `h` and `ck` from the production `EciesX25519BuildCryptography` implementation under test. This can preserve the same defect on both sides. Plan 111 requires fixed, independently generated expected values for all critical intermediate states.

These defects are local and deterministic. They do not require NTCP2, SSU2, network namespaces, Java I2P, i2pd, Emissary, public I2P access, or another interoperability harness.

## 3. Normative sources

Implementation must use the current final specifications as authority:

1. I2P Tunnel Creation Specification (ECIES-X25519):
   `https://i2p.net/en/docs/specs/tunnel-creation-ecies/`
   - current page metadata observed during planning: **Updated 2025-06; Accurate for 0.9.66**;
   - short request/reply record layouts;
   - Noise-N request KDF/transcript;
   - `SMTunnelReplyKey`, `SMTunnelLayerKey`, `TunnelLayerIVKey`, `RGarlicKeyAndTag` derivations;
   - record-number nonce/IV behavior;
   - inbound creator ephemeral requirement.

2. I2P ECIES-X25519 Router Messages specification:
   `https://i2p.net/en/docs/specs/ecies-routers/`
   - confirms the initial Noise-N state and null-prologue `MixHash` sequence.

3. I2P I2NP specification:
   `https://i2p.net/en/docs/specs/i2np/`
   - STBM type 25 / OTBRM type 26 and message framing.

4. I2P Tunnel Implementation / Tunnel Routing documentation:
   - confirms tunnel IDs are independent four-byte values associated per hop.

5. The repository-local `specs/` corpus where it accurately mirrors the current official specifications.

Proposal documents may be used only for historical rationale. Final specifications win on conflict.

### 3.1 Important planning-time facts that implementation must preserve

The implementation agent must re-check the current official page before coding, but the following were verified during Plan 111 planning:

- `Noise_N_25519_ChaChaPoly_SHA256` is 31 bytes and is zero-padded to 32 bytes for the initial `h`; `ck` is copied from that initial value.
- a null prologue is then mixed as `h = SHA256(h)` before the peer static public key is mixed;
- request `es` uses exactly one `HKDF(chainKey, sharedSecret, "", 64)` operation, producing both new `ck` and request AEAD key `k`;
- the hop-own reply uses the derived `replyKey`, record number `n = 0..7`, and saved request `h` as associated data;
- raw ChaCha20 transforms of other records use a 12-byte IV of all zero bytes except `iv[4] = record_number`;
- the OBEP garlic reply continuation produces a 32-byte key and an 8-byte tag;
- the short request plaintext's first fields are receive tunnel ID, next tunnel ID, and next router identity hash; those are independent values;
- an inbound build plaintext includes the creator ECIES ephemeral public key because the IBGW layer has no build-record DH.

## 4. Scope lock

### 4.1 In scope

Plan 111 owns only:

- correction of initial Noise-N `h` / `ck` initialization and null-prologue state;
- correction of the single request `es` HKDF split and request AEAD key ownership;
- correction of request-side and responder-side transcript parity;
- correction of record-number nonce/IV construction to byte 4;
- correction of raw ChaCha20 preprocessing/postprocessing IV construction to byte 4;
- correction of OBEP `RGarlicKeyAndTag` output representation to an 8-byte tag;
- explicit typed modeling and encoding of the inbound creator ephemeral public key where required by the final specification;
- explicit per-hop receive tunnel ID and next tunnel ID in the tunnel path/build-plan model;
- removal of any derivation of tunnel IDs from router hashes;
- exact role-aware responder KDF behavior, including OBEP continuation;
- hard-coded or fixture-file expected intermediate cryptographic values produced independently from the production implementation;
- downgrade/reopen and then re-close the Plan 109/110 conformance status only after the corrected evidence passes;
- narrowly necessary docs/support-registry updates.

### 4.2 Explicitly out of scope

Do **not** add any of the following to Plan 111:

- live mixed-router build execution;
- NTCP2 activation or repair;
- SSU2 implementation;
- generic I2NP router dispatch;
- public I2P network join;
- Java/i2pd/Emissary subprocess orchestration;
- Docker, Podman, Multipass, network namespaces, user namespaces, root, or rootless-host work;
- Python orchestration or new interoperability harnesses;
- reseed acquisition changes;
- NetDB algorithm changes;
- transit tunnel participation;
- tunnel data-plane forwarding;
- SAM, I2CP, streaming, LeaseSet, or garlic-routing implementation beyond representing the already-derived OBEP key/tag material correctly;
- mixed ElGamal/ECIES build records;
- generic cryptographic framework abstraction unrelated to this one protocol correction;
- new repo-wide CI architecture.

If implementation pressure tries to pull any of these into Plan 111, stop at the existing seam and record the dependency instead.

## 5. Required implementation order

Execute the work in this order. Do not start external-delivery work while any earlier phase is unresolved.

### Phase A — freeze the corrected protocol model before editing code

Create a small implementation note or test-local table that records, from the current official specification:

```text
initial protocol name          = Noise_N_25519_ChaChaPoly_SHA256
initial h                      = protocol name || zero padding to 32
initial ck                     = initial h
null prologue                  = h = SHA256(h)
MixHash static                 = h = SHA256(h || hepk)
MixHash ephemeral              = h = SHA256(h || sepk)
sharedSecret                   = X25519(sesk, hepk)
request keydata                = HKDF(ck, sharedSecret, "", 64)
new ck                         = keydata[0..32]
request AEAD key               = keydata[32..64]
request AEAD nonce             = 0
request AEAD AD                = h before ciphertext MixHash
post-request h                 = SHA256(h || ciphertext || tag)
reply slot nonce/IV            = [0,0,0,0,n,0,0,0,0,0,0,0]
raw ChaCha20 target IV         = [0,0,0,0,n,0,0,0,0,0,0,0]
garlic reply tag               = 8 bytes
```

Acceptance:

- table is checked against the current official pages;
- no Plan 109/110 helper is treated as normative evidence;
- implementation changes begin only after discrepancies are enumerated.

### Phase B — repair Noise-N initialization and request `es`

Refactor `crates/i2pr-tunnel/src/build_crypto.rs` so the protocol state is literal and difficult to misuse.

Required behavior:

1. `NoiseRequestState::new_for_short_build()` or equivalent must:
   - build padded protocol-name `h0`;
   - set `ck = h0`;
   - set `h = SHA256(h0)` to apply the null prologue;
   - not mix the peer key yet unless the constructor is explicitly peer-bound.

2. Peer static mix:
   - `h = SHA256(h || hepk)`.

3. Sender ephemeral mix:
   - generate a fresh per-hop X25519 ephemeral keypair;
   - `h = SHA256(h || sepk)`.

4. `es`:
   - compute `sharedSecret = X25519(sesk, hepk)`;
   - reject the all-zero shared secret;
   - run exactly one `HKDF(old_ck, sharedSecret, "", 64)`;
   - assign `new_ck = keydata[0..32]`;
   - assign request AEAD key `k = keydata[32..64]`;
   - do **not** run a second empty-input HKDF to obtain the request key.

5. Request AEAD:
   - nonce = zero;
   - AD = current `h`;
   - encrypt exactly 154 plaintext bytes and produce the 16-byte tag;
   - post-encryption `MixHash` covers the complete ciphertext+tag as specified.

6. Responder open path must perform the exact same state transitions independently from its static private key and sender ephemeral public key.

Recommended API shape:

```rust
struct NoiseRequestState {
    ck: Zeroizing<[u8; 32]>,
    h: [u8; 32],
}

struct RequestKeyMaterial {
    next_ck: Zeroizing<[u8; 32]>,
    aead_key: Zeroizing<[u8; 32]>,
}
```

The names are illustrative. The important requirement is that the API makes the single-HKDF split explicit and prevents deriving `k` from a second independent operation.

Acceptance:

- null-prologue test fails if `SHA256(h0)` is removed;
- fixed-vector test fails if a second HKDF is reintroduced;
- creator/responder produce identical corrected post-request `ck` and `h`;
- all request-envelope mutation/authentication tests remain fail-closed;
- no production secret gains `Debug`, serde, or accidental persistent serialization.

### Phase C — correct slot nonce and raw-ChaCha20 IV placement

Correct both:

- `ValidatedRecordSlot::nonce()` in `build_crypto.rs`;
- raw ChaCha20 IV construction in `multirecord.rs`.

Canonical representation for slot `n` in `[0, 7]`:

```text
[0, 0, 0, 0, n, 0, 0, 0, 0, 0, 0, 0]
```

Do not use `nonce[11] = n`.

Required tests:

- slot 0 -> all-zero 12-byte nonce;
- slot 1 -> only byte 4 is `0x01`;
- slot 7 -> only byte 4 is `0x07`;
- regression assertion that byte 11 remains zero for every valid slot;
- hard-coded ChaCha20 transform vector for at least one nonzero slot;
- hard-coded ChaCha20-Poly1305 reply vector for at least one nonzero slot;
- wrong-slot authentication remains fail-closed.

Acceptance:

- all production and fixture paths use the same canonical record-number encoding;
- no helper still writes slot number at byte 11;
- multi-record preprocessing/postprocessing tests still pass with corrected vectors.

### Phase D — correct OBEP garlic continuation material

Change `LayerKeys` or the narrower OBEP continuation type so:

```text
garlicReplyKey = 32 bytes
garlicReplyTag = 8 bytes
```

Required behavior:

- derive `RGarlicKeyAndTag` exactly from the OBEP continuation chaining key;
- retain bytes `0..8` as the tag;
- retain bytes `32..64` as the 32-byte key;
- do not expose a 16-byte prefix as the normative tag;
- zeroize key material on drop;
- the 8-byte tag may be debugged only if protocol policy treats it as public/opaque; default to redaction if uncertain.

Acceptance:

- compile-time type size makes a 16-byte garlic tag impossible on the Plan 111 path;
- fixed expected vector asserts all 8 tag bytes and all 32 key bytes;
- OBEP and non-OBEP KDF branches remain distinguishable in tests.

### Phase E — model real per-hop tunnel IDs

Remove the current shortcut in which one creator tunnel ID is reused for every hop and next tunnel IDs are derived from router-hash bytes.

The path/build-plan surface must explicitly contain the protocol values needed for each request record.

Recommended shape:

```rust
struct HopSpec {
    router_hash: Hash,
    static_encryption_key: [u8; 32],
    receive_tunnel_id: TunnelId,
    next_tunnel_id: TunnelId,
    next_router: Hash,
    role: HopRole,
}
```

The exact ownership split between `HopSpec`, `ShortBuildPath`, and `MultiRecordHopSpec` may differ, but the following invariants are mandatory:

- each real hop has its own nonzero receive tunnel ID;
- each request carries an explicit nonzero next tunnel ID;
- next router hash and next tunnel ID are independent typed fields;
- no production code derives a tunnel ID from any prefix or slice of a router hash;
- path construction rejects missing/zero IDs before encryption;
- tests use distinct values for receive ID, next ID, and router-hash prefix so accidental coupling is visible;
- the creator/gateway/endpoint boundary semantics remain explicit for inbound versus outbound paths.

Do not add tunnel-ID allocation policy beyond what is minimally needed to consume explicitly supplied IDs. Random ID selection policy can stay with the future pool/builder owner if not already present.

Acceptance:

- grep/code search finds no `u32::from_be_bytes(router_hash[..4])`-style tunnel-ID synthesis in the production short-build path;
- a three-hop fixture demonstrates three distinct receive IDs and distinct next IDs;
- the encoded first eight bytes of every request exactly match the explicit path plan.

### Phase F — implement inbound creator ephemeral plaintext semantics

The current official specification states that the creator ECIES ephemeral public key is included in the plaintext record for an inbound tunnel build and is used for IBGW layer/reply KDF material because no build-record DH exists at that layer.

This requirement must become an explicit typed part of the short-request construction path rather than hidden in random padding.

Because the public specification describes this as "other data as implied by flags or options" rather than presenting a separate fixed byte-offset row, the implementer must **not invent an offset**.

Before coding the encoding location:

1. inspect the current final specification and repository-local corpus again;
2. inspect at least one current independent router implementation (prefer Java I2P first; i2pd may be used as a second cross-check) solely to disambiguate the placement/order if the final prose remains underspecified;
3. record the observed source file/function/commit in `plans/111-status.md` or closure notes;
4. implement the smallest representation matching the observed interoperable layout;
5. if the implementations disagree, stop and mark only this sub-item blocked rather than guessing.

Required semantics regardless of placement:

- inbound-only field;
- exactly 32-byte ECIES/X25519 public key;
- distinct from the 32-byte sender ephemeral in the encrypted outer record envelope unless the final protocol explicitly defines a relationship;
- generated/owned by the creator as required for the IBGW layer KDF;
- absent from outbound request plaintexts;
- included in bounds calculations for the Mapping + extra-data + padding budget;
- malformed/missing inbound key fails closed.

Acceptance:

- inbound and outbound plaintext encoders have explicit different behavior;
- an inbound fixture asserts exact location/bytes based on the documented external reference;
- an outbound fixture proves no inbound creator key is accidentally emitted;
- options/padding bounds remain within exactly 154 bytes.

### Phase G — make hop processing role-aware

Remove `_is_outbound_endpoint` / `hop_role_from_opened() -> Participant` shortcuts.

Required behavior:

- decode the role from the authenticated request plaintext;
- reject invalid role flags using `ShortRequestRecord` parsing/validation;
- derive normal participant/IBGW keys through the non-OBEP branch;
- derive OBEP layer/IV/garlic material through the OBEP branch;
- the simulated processor must exercise exactly the same role decision a real hop would make after successfully opening its record.

Acceptance:

- participant fixture produces no garlic continuation;
- OBEP fixture produces the expected 32-byte key + 8-byte tag;
- changing an authenticated fixture's role changes the KDF branch as expected;
- invalid role bits are rejected before successful reply creation.

### Phase H — replace self-derived conformance evidence

The corrected implementation must not call the production primitive to create the expected values used to validate that same primitive.

Required fixed evidence for at least one participant request and one OBEP request:

- input static private/public key;
- input ephemeral private/public key;
- router identity hash / truncated prefix;
- exact 154-byte plaintext request;
- initial padded protocol-name value;
- post-null-prologue `h`;
- post-static `h`;
- post-ephemeral `h`;
- X25519 shared secret;
- 64-byte request HKDF output;
- resulting post-request `ck`;
- request AEAD key;
- exact 218-byte encrypted request record;
- post-request transcript `h`;
- `SMTunnelReplyKey` 64-byte output;
- `replyKey`;
- `SMTunnelLayerKey` 64-byte output;
- `layerKey` and non-OBEP `ivKey`;
- OBEP `TunnelLayerIVKey` continuation;
- OBEP `RGarlicKeyAndTag` output, 32-byte key, 8-byte tag;
- exact reply nonce for a nonzero record slot;
- exact 218-byte hop-own reply;
- one raw-ChaCha20 transformed 218-byte other-record example using a nonzero slot.

### Acceptable vector-generation methods

Use one of these approaches:

1. a tiny throwaway reference script/tool outside the production crate, written directly from the final specification and then **freeze the resulting values as constants/fixture files**; or
2. extract known values from a current reference-router unit test if such vectors exist and licensing/attribution permits; or
3. independently calculate the values using low-level primitives without importing/calling any `i2pr-tunnel` production helper.

The final repository tests must compare production outputs to frozen values. Runtime "reference" generation that calls production code is not closure evidence.

A short Rust test-only oracle is preferred over Python if values must be regenerated, but the frozen values themselves are the important artifact. Do not create another general harness.

Acceptance:

- deleting or perturbing any production transcript step causes at least one fixed-vector test to fail;
- moving slot byte from 4 back to 11 causes fixed-vector tests to fail;
- changing garlic tag from 8 to 16 bytes cannot compile or fails fixture parsing;
- expected `ck`/`h` values are not obtained from `SealedShortRequest.state` during fixture construction;
- fixture provenance is documented.

## 6. State-machine and pool integration requirements

Do not redesign the state machine.

After the corrected protocol values are wired back into `ShortBuildStateMachine`:

- `prepare()` must still emit a complete count-prefixed STBM payload;
- per-hop contexts must retain only the corrected secret/transcript material needed for reply postprocessing and eventual layer establishment;
- reply postprocessing must use the corrected slot byte-4 nonce/IV semantics;
- a rejected, malformed, unauthenticated, incomplete, or role-invalid build must not reach `Established`;
- only a fully authenticated all-hop Accepted result may pass `ShortBuildRegistrar` into `ExploratoryPool`;
- the registrar must not be loosened to make fixtures easier to pass.

Required regression cases:

- one bad request tag;
- one wrong hop identity prefix;
- one bad reply tag;
- wrong record slot;
- wrong saved `h`;
- wrong reply key;
- wrong role/OBEP branch;
- missing inbound creator ephemeral;
- zero receive tunnel ID;
- zero next tunnel ID;
- swapped per-hop tunnel IDs;
- modified inbound originator fake;
- duplicate terminal event;
- timeout before valid reply;
- HopRejected response;
- any one of the above must leave the exploratory pool unchanged.

## 7. Documentation and authority correction

At implementation start, do not leave the existing Plan 109/110 closure claims unqualified.

The implementation commit must update the relevant status/support surfaces to say, until Plan 111 closes:

```text
plan_109 = implementation-landed-conformance-corrected-by-plan111
plan_110 = implementation-landed-conformance-corrected-by-plan111
plan_111 = in-progress
short_build_local_conformance = reopened
external_build_delivery = blocked-on-plan111
```

At successful closure, use:

```text
plan_109 = superseded-by-plan111-corrected
plan_110 = superseded-by-plan111-corrected
plan_111 = passed-final-local-short-build-conformance
noise_n_request_transcript = locally-conformant-fixed-vectors
record_slot_nonce_iv = locally-conformant-byte4
obep_garlic_material = locally-conformant-32-key-8-tag
inbound_creator_ephemeral = locally-conformant
per_hop_tunnel_ids = explicit-and-validated
short_build_multirecord_processing = locally-conformant-fixed-vectors
complete_stbm_payload = locally-conformant-fixed-vectors
external_build_delivery = next-checkpoint
live_mixed_router_build = blocked-on-qualified-delivery
normal_daemon_ntcp2 = disabled-and-unenableable
ntcp2 = experimental-non-advertised
```

Update only the documents necessary to prevent stale claims:

- `plans/109-status.md` amendment/banner;
- `plans/110-status.md` amendment/banner;
- new `plans/111-status.md` closure record;
- `specs/support.toml`;
- `docs/protocol-support.md`;
- `docs/architecture/i2pr-tunnel.md` if its current conformance statement becomes inaccurate;
- README/AGENTS only if they currently state the superseded Plan 109/110 claims as active truth.

Do not rewrite unrelated historical plans.

## 8. Dependency policy

Prefer existing dependencies.

Plan 111 already has the needed primitive families in-tree:

- `x25519-dalek`;
- `sha2`;
- `hmac` / repository HKDF helper;
- `chacha20poly1305`;
- `chacha20`;
- `zeroize`.

Do not add a general Noise framework solely to fix this short transcript. The literal state machine is small and protocol-specific.

A new dependency requires explicit justification in the closure record and must not materially expand the router's footprint for something already representable with existing primitives.

## 9. Verification matrix

Run, at minimum:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-multipass-interop-boundary.sh
bash scripts/check-constrained-host-lane-boundary.sh
git diff --check
```

The historical rootless-interoperability baseline may remain broken only if it is still the already-documented Plan 099 retirement artifact. Do not revive its Python machinery for Plan 111.

### 9.1 Focused mandatory tests

Add focused tests whose names make these regressions obvious:

```text
noise_null_prologue_is_mixed_before_peer_static
request_es_uses_single_hkdf_split
request_fixed_vector_matches_independent_expected_bytes
record_slot_nonce_places_n_at_byte_4
record_slot_nonce_never_places_n_at_byte_11
raw_chacha20_nonzero_slot_matches_fixed_vector
reply_aead_nonzero_slot_matches_fixed_vector
obep_garlic_reply_tag_is_exactly_8_bytes
obep_kdf_matches_independent_fixed_vector
inbound_request_contains_creator_ephemeral_at_reference_layout
outbound_request_does_not_contain_inbound_creator_ephemeral
per_hop_receive_and_next_tunnel_ids_are_independent
router_hash_prefix_is_never_used_as_tunnel_id
obep_processor_uses_obep_kdf_branch
fixed_fixture_does_not_call_production_to_generate_expected_state
failed_build_never_registers_in_exploratory_pool
```

Exact test names may vary, but every semantic must have a direct regression test.

## 10. Explicit acceptance criteria

Plan 111 closes only when **all** of the following are true:

### Noise request transcript

- [ ] initial protocol-name handling matches final spec;
- [ ] null prologue `h = SHA256(h)` is present;
- [ ] peer static and sender ephemeral are mixed in correct order;
- [ ] one and only one `HKDF(ck, sharedSecret, "", 64)` produces `new_ck` and request AEAD key;
- [ ] no second HKDF derives the request AEAD key;
- [ ] request AEAD nonce is zero;
- [ ] request AEAD AD is pre-ciphertext `h`;
- [ ] ciphertext+tag is mixed into `h` afterward;
- [ ] creator/responder fixed vectors match independently frozen values.

### Slot nonce / preprocessing

- [ ] all valid slots encode record number at byte 4;
- [ ] byte 11 is zero for slots 0–7;
- [ ] hop-own reply AEAD uses corrected record-number nonce;
- [ ] raw ChaCha20 other-record transform uses corrected byte-4 IV;
- [ ] nonzero-slot fixed vectors pass.

### KDF continuation

- [ ] `SMTunnelReplyKey` output matches independent fixed vector;
- [ ] `SMTunnelLayerKey` output matches independent fixed vector;
- [ ] non-OBEP IV key is correct;
- [ ] OBEP `TunnelLayerIVKey` continuation is correct;
- [ ] OBEP `RGarlicKeyAndTag` is correct;
- [ ] garlic reply key is 32 bytes;
- [ ] garlic reply tag is exactly 8 bytes.

### Path/request semantics

- [ ] every hop has explicit receive tunnel ID;
- [ ] every hop has explicit next tunnel ID;
- [ ] next router hash remains independent from tunnel IDs;
- [ ] no router-hash bytes are converted into a tunnel ID;
- [ ] zero tunnel IDs fail before request encryption;
- [ ] request first eight bytes match the explicit hop plan.

### Inbound-specific semantics

- [ ] creator ephemeral public key is explicitly represented for inbound builds;
- [ ] its exact interoperable location/order is documented from final spec/reference-router evidence, not guessed;
- [ ] inbound missing/malformed creator key fails closed;
- [ ] outbound records omit inbound-only creator-key data;
- [ ] Mapping/additional-data/padding bounds still total exactly 154 bytes.

### Role-aware processing

- [ ] processor decodes authenticated role from plaintext;
- [ ] OBEP path really uses OBEP KDF continuation;
- [ ] participant/IBGW path does not fabricate OBEP garlic material;
- [ ] invalid role fails closed.

### Independent evidence

- [ ] expected critical values are frozen constants/fixture bytes;
- [ ] fixture generation does not call production `seal_short_request*`, `open_short_request`, `derive_layer_keys`, or slot-nonce helpers to obtain expected values;
- [ ] fixture provenance records spec/reference source and generation method;
- [ ] mutating each critical production step causes fixture failure.

### Scope / architecture

- [ ] no normal-daemon NTCP2 activation;
- [ ] no SSU2 work;
- [ ] no live router required to close the plan;
- [ ] no new Python interop harness;
- [ ] no namespace/container/root requirement;
- [ ] no generic I2NP dispatcher expansion;
- [ ] `i2pr-tunnel` remains runtime/network neutral;
- [ ] only successful authenticated builds can register in `ExploratoryPool`;
- [ ] workspace verification is green.

## 11. Failure and stop conditions

Do not paper over uncertainty.

### Stop as `blocked-inbound-layout-ambiguity` if

- the current final spec and at least one current reference router do not provide enough consistent evidence to place/interpret the inbound creator ephemeral field safely.

In that case:

- finish all other Plan 111 corrections;
- keep inbound live construction disabled;
- record the exact disagreement/ambiguity;
- do not invent a private layout.

### Stop as `blocked-fixed-vector-disagreement` if

- independently generated fixed vectors disagree with production after the literal spec transcription has been checked twice.

In that case:

- do not mark support `conformant = true`;
- preserve the mismatch as evidence;
- isolate whether the disagreement is X25519 encoding, HKDF convention, ChaCha nonce encoding, transcript order, or external vector error.

### Do not treat as blockers

- absence of a rootless host;
- absence of Java I2P runtime;
- inability to create network namespaces;
- unresolved NTCP2 live-wire defect;
- lack of public I2P access.

Those are outside this local correction.

## 12. Closure record requirements

Create `plans/111-status.md` only as the implementation progresses; do not pre-mark it passed.

The final closure record must include:

- implementation commit SHA;
- official specification metadata used (`Updated` / `Accurate for`);
- exact reference-router source/commit used only to resolve any underspecified inbound creator-key placement;
- fixture provenance and generation method;
- before/after table for every known defect in §2;
- focused test count and workspace test count;
- dependency delta;
- documentation/support-registry changes;
- confirmation that NTCP2 remains disabled/unenableable in normal daemon configuration;
- confirmation that no live interoperability claim is made;
- the next checkpoint wording below.

## 13. Required terminal state

Successful closure:

```text
plan_111                           = passed-final-local-short-build-conformance
noise_n_request_transcript         = locally-conformant-fixed-vectors
short_request_record               = locally-conformant
short_reply_record                 = locally-conformant
record_slot_nonce_iv               = locally-conformant-byte4
obep_garlic_material               = locally-conformant-32-key-8-tag
inbound_creator_ephemeral          = locally-conformant
per_hop_tunnel_ids                 = explicit-and-validated
short_build_multirecord_processing = locally-conformant-fixed-vectors
complete_stbm_payload              = locally-conformant-fixed-vectors
success_gated_pool_registration    = retained
external_build_delivery            = next-checkpoint
live_mixed_router_build            = blocked-on-qualified-delivery
normal_daemon_ntcp2                 = disabled-and-unenableable
ntcp2                               = experimental-non-advertised
```

If inbound layout remains genuinely ambiguous but all other work closes:

```text
plan_111                   = blocked-inbound-layout-ambiguity
outbound_short_build       = locally-conformant-fixed-vectors
inbound_short_build        = disabled-pending-layout-resolution
external_build_delivery    = blocked
```

## 14. Handoff after Plan 111

Only after successful Plan 111 closure should a new plan answer the external-delivery question.

That later checkpoint must begin from the concrete consumer now available:

```text
byte-correct count-prefixed STBM payload
        + explicit first-hop RouterHash
        + explicit per-hop tunnel IDs
        + retained reply/postprocessing state
```

Then ask, in order:

1. What already-existing router-message delivery seam can carry this STBM to one peer?
2. Can this be done without adding a generic I2NP dispatcher?
3. Which currently available transport is the smallest qualified lane?
4. If NTCP2 is chosen, what exact remaining Plan 099 defect must be corrected for this one consumer?
5. Can i2pd or Emissary provide the independent peer without requiring privileged isolation?
6. What minimal evidence distinguishes transport delivery failure from STBM cryptographic/record rejection?

Do not pre-create a broad interop framework. Do not reopen Milestone 3 wholesale.
