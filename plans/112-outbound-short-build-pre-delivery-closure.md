# Plan 112: Outbound short-build pre-delivery closure

- Status: **passed-outbound-pre-delivery-closure**
- Date: 2026-08-15
- Parent roadmap: `plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md`
- Parent authority: `plans/102-amendment-exploratory-tunnel-dependency.md`
- Predecessor: `plans/111-status.md`
- Scope class: **narrow local corrective pass**
- External network gate: **none**
- Blocks: first outbound qualified external-delivery checkpoint
- Does not block on: Plan 113 inbound standards/reference reconciliation

## 1. Goal

Close the remaining deterministic local defects in the outbound ECIES-X25519 short-tunnel-build path so the bytes emitted by the runtime-neutral state machine have a defensible contract before a transport adapter sends them to an independent router.

Plan 112 is not a new tunnel-build implementation. It must preserve the Plan 111 cryptographic core and Plan 110 multi-record machinery while correcting five concrete classes of defects:

1. random request/reply plaintext padding;
2. direction/role topology validation;
3. the incorrect `HopCryptoContext::ephemeral_public()` accessor;
4. count-prefixed STBM/OTBRM action/event contract inconsistencies;
5. frozen-vector provenance that is not reproducible from the repository.

Plan 112 must also make the current inbound support boundary honest: public production construction must fail closed with a typed error until Plan 113 resolves the remaining inbound standards/reference discrepancy.

At closure, **outbound** short-build construction may be described as `locally-conformant-pre-delivery`. Plan 112 does not claim live interoperability.

## 2. Why this pass is required

### 2.1 Request plaintext padding is not random

Current `ShortRequestRecord::encode()` creates a zero-initialized 154-byte buffer, writes the fixed fields and Mapping, and leaves all remaining bytes zero.

The final I2P Tunnel Creation Specification defines bytes after Mapping through byte 153 as **random padding**.

Current Java I2P master at commit `498488b0d01d9f59efe906424e56ff5e25f58a4d` agrees: the short-record constructor calls `ctx.random().nextBytes(buf, off, sz)` for the remaining request plaintext.

Current i2pd at commit `dfcb8a8043c0c689e5681c5ae5da89df5643347e` currently zero-fills this area. That is a reference divergence from the final specification and must not be copied into i2pr merely because it interoperates.

### 2.2 Reply plaintext padding is not random

Current `ShortReplyRecord::encode()` similarly leaves the bytes between the Mapping and reply byte zero.

The final specification defines this area through byte 200 as **random padding**.

Current Java I2P `BuildResponseRecord.createShort()` fills this area from the router RNG before setting the reply byte and AEAD-encrypting.

### 2.3 Role topology is not validated at the path boundary

Plan 111 correctly made the hop processor respect the authenticated role byte, but `ShortBuildPath::validate()` does not validate that the configured role sequence is meaningful for the chosen tunnel direction.

Current independent router behavior establishes the canonical remote-hop topology:

#### Outbound tunnel

The local creator is the outbound gateway and is **not** represented as a remote hop record.

Remote hops must be:

```text
Participant ... Participant -> OutboundEndpoint
```

For one remote hop:

```text
OutboundEndpoint
```

No remote hop may be `InboundGateway`.

Evidence:

- Java I2P `BuildMessageGenerator.createUnencryptedRecord()` sets `isOutEnd` only for the final outbound remote hop and `isInGW` only for inbound hop zero.
- i2pd `TunnelConfig` outbound constructor clears `m_FirstHop->isGateway` and sets the final hop using `SetReplyHop(...)`.

#### Inbound tunnel

The local creator is the inbound endpoint and is not represented as a real remote hop.

Remote hops must be:

```text
InboundGateway -> Participant ... Participant
```

For one remote hop:

```text
InboundGateway
```

No remote hop may be `OutboundEndpoint`.

Evidence:

- Java I2P sets `isInGW = cfg.isInbound() && hop == 0` and never marks an inbound remote hop as OBEP.
- i2pd inbound `TunnelConfig` keeps the first remote hop as gateway and `SetNext()` clears gateway on following peers; the final remote hop points to the creator and is not endpoint-flagged.

### 2.4 `HopCryptoContext::ephemeral_public()` returns the wrong bytes

The Plan 111 encrypted request envelope is:

```text
0..16    truncated hop identity hash
16..48   sender ephemeral X25519 public key
48..202  154-byte ciphertext
202..218 Poly1305 tag
```

Current accessor code copies `own_record[..32]`, returning:

```text
16 bytes hop hash || first 16 bytes ephemeral pubkey
```

This is a deterministic API bug.

If no production caller requires the accessor, deleting it is preferable to retaining unnecessary public API. If it is retained, it must return bytes `16..48` exactly and have a regression test.

### 2.5 State-machine payload contract is internally inconsistent

`prepare_short_build_message()` correctly returns a count-prefixed STBM payload:

```text
1 byte count || count * 218-byte records
```

But the state-machine action/event surface still describes payloads as if they were bare concatenated 218-byte records.

Current examples:

- `ShortBuildAction::Deliver.message` documentation says the message is “218-byte-aligned” records concatenated;
- `BuildEvent::BuildReply.reply` documentation says its size is `records * SHORT_BUILD_RECORD_SIZE`;
- `deliver_action()` derives record count with `message.records.len() / 218`, which currently works only accidentally because integer division ignores the one-byte count prefix;
- the field name `records` on `ShortTunnelBuildMessage` obscures that the buffer contains a full count-prefixed payload.

This is dangerous immediately before a transport adapter is written: an adapter could strip or prepend a second count byte based on the incorrect public contract.

### 2.6 Frozen vector provenance is stale

`fixed_vectors.rs` says the values were produced by:

an uncommitted integration-test generator that is not present in the
repository. `plans/111-status.md` separately says the generator was not
committed.

The constants remain useful, but their provenance is not reproducible from the repository.

Plan 112 must repair this without creating a general-purpose harness.

### 2.7 Inbound support status is not coherently enforced

Documentation says:

```text
inbound_short_build = disabled-pending-layout-resolution
```

but lower-level `prepare_short_build_message()` accepts `TunnelDirection::Inbound` and current tests successfully construct inbound messages.

The high-level `ShortBuildStateMachine::prepare()` passes `originator_hash = None`, which makes an inbound call fail indirectly when originator-fake creation requests the missing hash.

That is not a valid fail-closed API contract. It is an incidental failure with the wrong error category.

Plan 112 must make all production entry points explicitly refuse inbound construction until Plan 113 decides the standards/reference discrepancy.

## 3. Normative and reference sources

Use these exact sources during implementation review.

### Final specification

I2P Tunnel Creation Specification (ECIES-X25519):

`https://i2p.net/en/docs/specs/tunnel-creation-ecies/`

Relevant requirements:

- 154-byte short request plaintext;
- Mapping at byte 56;
- random request padding through byte 153;
- 202-byte short reply plaintext;
- random reply padding through byte 200;
- reply byte at 201;
- 218-byte encrypted record;
- record number at IV byte 4;
- randomized record order and fake-record rules.

I2NP specification:

`https://i2p.net/en/docs/specs/i2np/`

Relevant requirements:

- STBM type 25;
- OTBRM type 26;
- payload is exactly `1 + num*218` bytes;
- `num` valid domain 1..=8.

### Java I2P reference

Repository `i2p/i2p.i2p`, pinned commit:

`498488b0d01d9f59efe906424e56ff5e25f58a4d`

Files:

- `router/java/src/net/i2p/data/i2np/BuildRequestRecord.java`
- `router/java/src/net/i2p/data/i2np/BuildResponseRecord.java`
- `router/java/src/net/i2p/router/tunnel/pool/BuildMessageGenerator.java`

### i2pd cross-check

Repository `PurpleI2P/i2pd`, pinned commit:

`dfcb8a8043c0c689e5681c5ae5da89df5643347e`

Files:

- `libi2pd/TunnelConfig.cpp`
- `libi2pd/Tunnel.cpp`
- `libi2pd/TransitTunnel.cpp`

Where i2pd conflicts with the final spec on padding, final spec wins.

## 4. Non-goals

Plan 112 must not:

- activate NTCP2;
- repair the Plan 099 NTCP2 wire defect;
- implement SSU2;
- run Java I2P or i2pd as subprocesses;
- require root, network namespaces, Docker, Multipass, or privileged networking;
- add Python;
- build a generic I2NP dispatcher;
- implement inbound creator-key semantics beyond an explicit fail-closed gate;
- implement tunnel data-plane encryption/forwarding;
- modify NetDB behavior;
- perform public-network or live mixed-router tests.

## 5. Work package A — make plaintext padding explicit and random

### A1. Preserve runtime neutrality

Do **not** call an operating-system RNG directly from `ShortRequestRecord` or `ShortReplyRecord`.

The tunnel crate already accepts injected `CryptoRng + RngCore` instances in the build path. Continue that model.

### A2. Request encoder

Refactor request encoding so production construction receives randomness explicitly.

Acceptable designs include:

```rust
pub fn encode_with_rng<R: CryptoRng + RngCore>(
    &self,
    rng: &mut R,
) -> Result<Zeroizing<[u8; SHORT_REQUEST_PLAINTEXT_SIZE]>, ShortBuildError>
```

or a lower-level exact-padding method plus an RNG wrapper.

Required behavior:

1. encode fixed fields exactly as Plan 111 does today;
2. encode Mapping exactly;
3. calculate the padding slice as:
   `56 + encoded_mapping_len .. 154`;
4. fill **all** bytes in that slice from the supplied CSPRNG;
5. never overwrite Mapping bytes;
6. no production method may silently zero-fill the padding.

If a deterministic exact-padding serializer is retained for fixed-vector tests, it must be private/test-only or explicitly named so it cannot be mistaken for the production path.

### A3. Reply encoder

Apply the same rule to reply plaintext:

1. encode Mapping at offset 0;
2. fill from `encoded_mapping_len .. 201` with supplied random bytes;
3. set response byte at offset 201 **after** padding;
4. never include the response byte in the random region.

### A4. RNG failure

Any `try_fill_bytes()` failure must return a typed error and abort construction.

Do not fall back to zero padding.

### A5. State-machine threading

The existing RNG passed into `ShortBuildStateMachine::prepare()` must flow into request plaintext creation before ECIES sealing.

For hop reply generation, make randomness an explicit input to the message-level hop processor or to a typed reply-plaintext construction helper.

Recommended shape:

```rust
pub fn process_hop<R: CryptoRng + RngCore>(
    ...,
    rng: &mut R,
) -> Result<...>
```

Do not hide a global RNG behind the responder.

## 6. Work package B — enforce direction/role topology

### B1. High-level path validation

Add explicit topology validation to `ShortBuildPath::validate()`.

#### Outbound rules

For `TunnelDirection::Outbound`:

- path must contain at least one remote hop;
- exactly the final hop must be `HopRole::OutboundEndpoint`;
- every earlier hop must be `HopRole::Participant`;
- no hop may be `HopRole::InboundGateway`.

Examples:

```text
[OBEP]                         = valid
[Participant, OBEP]            = valid
[Participant, Participant, OBEP] = valid
[IBGW, Participant, OBEP]      = invalid
[Participant, Participant]     = invalid
[OBEP, Participant]            = invalid
```

#### Inbound structural rules

For `TunnelDirection::Inbound`:

- first remote hop must be `HopRole::InboundGateway`;
- every later remote hop must be `HopRole::Participant`;
- no remote hop may be `HopRole::OutboundEndpoint`.

These rules are structural validation even though production inbound construction remains disabled until Plan 113.

Examples:

```text
[IBGW]                         = structurally valid
[IBGW, Participant]            = structurally valid
[IBGW, Participant, Participant] = structurally valid
[Participant, Participant]     = invalid
[IBGW, OBEP]                   = invalid
[OBEP]                         = invalid
```

### B2. Lower-level defense

`prepare_short_build_message()` must not trust arbitrary caller-supplied `MultiRecordHopSpec` roles.

Either:

- centralize topology validation in a shared helper used by both high-level and lower-level builders; or
- make the lower-level builder `pub(crate)` and require callers to pass a validated path representation.

Prefer consolidation over duplicate validation logic.

### B3. Fix current tests

Any current “canonical outbound” test that uses an inbound-gateway role on the first remote outbound hop must be corrected.

Do not weaken the new validator merely to retain old test fixtures.

## 7. Work package C — make inbound production gating explicit

Until Plan 113 closes, the production inbound path must return a typed, intentional error.

Recommended error category:

```rust
ShortBuildConstructionError::InboundBuildPendingReconciliation
```

or an equivalent `MultiRecordError` that is translated consistently at the high-level boundary.

Requirements:

- `ShortBuildStateMachine::prepare()` with `TunnelDirection::Inbound` fails before allocating slots or generating cryptographic material;
- the public multi-record production builder also fails before constructing a message;
- error text must reference the Plan 113 standards/reference reconciliation, not `EmptyPath` or missing `originator_hash`;
- originator-fake primitive helpers remain testable independently;
- component tests may still validate originator fake layout, integrity checking, and inbound slot-count policy without enabling full inbound message construction.

This is a temporary product gate, not deletion of inbound support code.

## 8. Work package D — fix or remove the ephemeral-public accessor

Current code:

```rust
out.copy_from_slice(&self.inner.own_record[..EPHEMERAL_KEY_LEN]);
```

is wrong for the Plan 111 envelope.

Preferred decision order:

1. search for all production consumers;
2. if there are none, delete `HopCryptoContext::ephemeral_public()` and any dead comments/tests;
3. if a caller needs it, return exactly:
   `own_record[HASH_PREFIX_LEN .. HASH_PREFIX_LEN + EPHEMERAL_KEY_LEN]`;
4. add a regression test using a record whose hash prefix and ephemeral bytes are deliberately different.

Do not expose an accessor whose semantics duplicate data already retained elsewhere unless a real consumer needs it.

## 9. Work package E — make STBM/OTBRM payload contracts exact

### E1. Canonical payload invariant

All public state-machine delivery/reply surfaces must use the same definition:

```text
payload[0] = count, 1..=8
payload.len() = 1 + count * 218
payload[1..] = encrypted records
```

### E2. Delivery action

Correct `ShortBuildAction::Deliver` documentation and construction.

Do not derive count as:

```rust
message.len() / 218
```

Instead validate and derive from the prefix, then check exact length.

Recommended helper:

```rust
fn validate_count_prefixed_short_payload(payload: &[u8]) -> Result<u8, ...>
```

Reuse the existing multi-record decoder if it already provides exactly this validation without unnecessary allocation.

### E3. Message naming

If `ShortTunnelBuildMessage.records` contains the count-prefixed payload, rename the field to `payload` if the change is local and low-risk.

If retaining the field name minimizes churn, its documentation must explicitly say it contains the one-byte count prefix.

Do not create a generic I2NP message abstraction merely for naming cleanliness.

### E4. Reply event

Correct `BuildEvent::BuildReply` documentation and validation to require the same count-prefixed structure.

The creator postprocessor already expects the prefix; make the public contract match the actual implementation.

### E5. Record count redundancy

`ShortBuildAction::Deliver` currently carries both payload and `record_count`.

Either:

- retain `record_count` as a convenience but derive it from validated `payload[0]` and assert equality; or
- remove it if there is no real consumer.

Prefer deleting redundant state when possible.

## 10. Work package F — repair frozen-vector provenance

The Plan 111 constants must be reproducible without reviving a harness subsystem.

Add exactly one small Rust-only reference generator/test artifact.

Preferred location:

```text
crates/i2pr-tunnel/tests/plan111_reference_vectors.rs
```

or another test-only location consistent with the workspace.

Requirements:

- use only low-level primitives and literal protocol constants;
- must not call `EciesX25519BuildCryptography`, `NoiseRequestState`, `derive_layer_keys`, or production high-level build helpers;
- must independently reproduce:
  - null-prologue hash;
  - hop/ephemeral X25519 public keys;
  - shared secret;
  - request HKDF 64-byte output;
  - request AEAD key and post-request ck;
  - sealed 218-byte request;
  - post-request transcript hash;
  - reply/layer/IV keys;
  - OBEP garlic key and 8-byte tag;
  - slot nonce examples;
- compare results against committed frozen constants;
- contain comments naming the final specification and the pinned Java/i2pd source commits used during Plan 112 review;
- no subprocesses;
- no file generation during normal CI;
- no Python.

If integration-test dependency visibility makes this awkward, a `#[cfg(test)]` reference module inside `i2pr-tunnel` is acceptable. Keep it small.

Correct the stale documentation that names a nonexistent generator path.

## 11. Work package G — padding-specific fixed evidence

Add deterministic tests using seeded CSPRNG-compatible test RNGs.

Required request tests:

- empty Mapping leaves bytes `56..58 = 00 00`;
- bytes `58..154` match the supplied deterministic RNG stream;
- two different seeded streams produce different padding while all semantic fields remain identical;
- oversized Mapping still fails;
- RNG failure fails closed.

Required reply tests:

- empty Mapping leaves bytes `0..2 = 00 00`;
- bytes `2..201` match deterministic RNG output;
- response byte remains exactly at 201;
- two RNG streams produce different padding;
- RNG failure fails closed.

Do not assert that random padding is nonzero. The requirement is random bytes, not nonzero bytes.

## 12. Work package H — topology and contract negative tests

Add explicit tests for:

- outbound with IBGW anywhere -> reject;
- outbound missing OBEP -> reject;
- outbound OBEP not final -> reject;
- outbound multiple OBEP roles -> reject;
- inbound first hop not IBGW -> reject structurally;
- inbound OBEP anywhere -> reject structurally;
- production inbound `prepare()` -> typed Plan 113 gate error;
- lower-level public inbound builder -> same gate;
- malformed STBM count 0 -> reject;
- malformed STBM count >8 -> reject;
- truncated count-prefixed payload -> reject;
- trailing bytes after exact count -> reject;
- action `record_count` cannot disagree with prefix;
- reply event with bare `count*218` data and no prefix -> reject;
- `ephemeral_public()` returns bytes 16..48 exactly, if retained.

## 13. Work package I — retain Plan 111 cryptographic invariants

Plan 112 must not alter the following except for mechanically threading random plaintext bytes into the existing request/reply AEAD inputs:

- Noise protocol name;
- null-prologue behavior;
- static/ephemeral MixHash order;
- one-HKDF request `es` split;
- 16+32+154+16 encrypted request layout;
- post-request `h` handling;
- `SMTunnelReplyKey`;
- `SMTunnelLayerKey`;
- `TunnelLayerIVKey`;
- `RGarlicKeyAndTag`;
- slot IV byte 4;
- raw ChaCha20 preprocessing order;
- randomized wire slot assignment;
- fake-record policy;
- success-only pool registration.

If a padding change appears to require changing those primitives, stop and diagnose rather than broadening Plan 112.

## 14. Work package J — documentation and authority correction

On successful implementation:

1. create `plans/112-status.md`;
2. amend the Plan 111 closure record to state that Plan 112 supersedes the premature full outbound pre-delivery claim;
3. update `plans/102-amendment-exploratory-tunnel-dependency.md`;
4. update `AGENTS.md` current-state block;
5. update `README.md`, `docs/protocol-support.md`, `docs/architecture/i2pr-tunnel.md`, and `specs/support.toml` only where they currently claim a stronger short-build status than the implementation supports;
6. keep inbound status as `blocked-on-plan113`;
7. do not mark live interoperability passed.

Expected post-Plan-112 status:

```text
plan_111_core_crypto              = retained
plan_112                          = passed-outbound-pre-delivery-closure
request_padding                   = random-injected-csprng
reply_padding                     = random-injected-csprng
outbound_topology                 = validated
inbound_topology                  = structurally-validated-production-disabled
hop_context_ephemeral_accessor    = corrected-or-removed
stbm_payload_contract             = exact-count-prefixed
otbrm_payload_contract            = exact-count-prefixed
fixed_vector_reference            = reproducible-rust-only
outbound_short_build              = locally-conformant-pre-delivery
inbound_short_build               = blocked-on-plan113
outbound_external_delivery        = next-qualified-checkpoint
normal_daemon_ntcp2               = disabled-and-unenableable
```

## 15. Required validation

Run the repository's normal bounded local validation, at minimum:

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Existing static NTCP2 boundary checks may be run if part of the normal repository check set, but they are not evidence for Plan 112's short-build conformance.

Do **not** add a new CI workflow.

## 16. Explicit acceptance criteria

Plan 112 passes only if every item below is true.

### Protocol bytes

- [x] Production request plaintext padding is filled from an injected CSPRNG.
- [x] Production reply plaintext padding is filled from an injected CSPRNG.
- [x] No production request/reply encoder silently leaves protocol padding zero-filled.
- [x] Mapping boundaries remain exact.
- [x] Reply status remains at byte 201.
- [x] Plan 111 encrypted request/reply sizes and crypto remain unchanged.

### Topology

- [x] Outbound path validates exactly one final OBEP and participants before it.
- [x] Outbound path rejects IBGW.
- [x] Inbound path structurally validates exactly one first IBGW and participants after it.
- [x] Inbound path rejects OBEP.
- [x] Current tests no longer describe an outbound path beginning with an IBGW as canonical.

### Inbound gate

- [x] High-level production inbound build construction returns a typed Plan 113 gate error before crypto work.
- [x] Any public lower-level production inbound construction entry point does the same.
- [x] Originator-fake component tests remain available without bypassing the production gate.

### API correctness

- [x] `HopCryptoContext::ephemeral_public()` is deleted if unused, or returns exact bytes 16..48 if retained.
- [x] STBM delivery action contract explicitly includes the one-byte count prefix.
- [x] Build-reply event contract explicitly includes the one-byte count prefix.
- [x] Record count is validated from the prefix rather than inferred by truncating `len / 218`.
- [x] Exact payload length is `1 + count * 218`; trailing/truncated data fail closed.

### Evidence

- [x] Frozen Plan 111 vectors remain unchanged unless an independently demonstrated prior constant is wrong.
- [x] A committed Rust-only reference artifact reproduces the frozen constants without calling production short-build crypto helpers.
- [x] No stale reference to a nonexistent vector generator remains.
- [x] Padding behavior has deterministic seeded tests plus RNG-failure tests.

### Scope

- [x] No NTCP2 activation/repair.
- [x] No SSU2 work.
- [x] No subprocess-based router harness.
- [x] No Python added.
- [x] No root/container/namespace requirement.
- [x] No generic I2NP dispatcher.
- [x] No live network claim.

## 17. Stop conditions

Stop Plan 112 rather than expanding scope if:

- correcting random padding requires changing the Noise/KDF transcript;
- the current final I2P spec changes materially during implementation;
- role topology in current Java I2P and i2pd no longer agrees;
- a production inbound consumer requires immediate enablement rather than the Plan 113 gate;
- external transport behavior becomes necessary to prove a local acceptance criterion.

Record the blocker in `plans/112-status.md` and create a narrowly scoped successor only if necessary.

## 18. Handoff after success

After Plan 112 passes, the project may create or execute the smallest possible **outbound-only qualified external-delivery checkpoint**.

That future checkpoint must consume the already-correct count-prefixed STBM payload and must not reopen request-record crypto, multi-record preprocessing, random-padding semantics, or topology validation absent concrete independent-router rejection evidence.

Plan 113 remains the sole authority for enabling inbound short builds.
