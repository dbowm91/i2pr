# Plan 110: Short tunnel-build multi-record preprocessing and local conformance closure

- Status: **blocked until Plan 109 passes**
- Date: 2026-08-15
- Parent corrective authority: `plans/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md`
- Predecessor: Plan 109
- Corrects: remaining message-level defects from Plan 108
- Milestone: 5 — exploratory tunnel construction
- Scope class: **multi-record local protocol conformance; no live-network acceptance gate**

## 1. Goal

Complete the local, transport-neutral short tunnel-build construction path after Plan 109 has corrected the one-record wire/Noise primitive.

At Plan 110 closure, i2pr must be able to take a validated all-ECIES exploratory path and deterministic inputs and produce/process a complete standards-shaped multi-record short-build payload with:

- randomized record slots;
- privacy-preserving fake records;
- required inbound-originator fake record behavior;
- per-hop request preprocessing using raw ChaCha20;
- hop-by-hop request discovery, own-reply insertion, and transformation of every other record;
- creator-side reply postprocessing;
- reply authentication mapped back to the correct hop;
- exact one-byte record-count I2NP payload framing;
- bounded 1–8 record parsing and construction;
- independent multi-hop conformance fixtures;
- success-only `ExploratoryPool` registration after every real hop has authenticated and accepted.

Plan 110 is the local conformance closure for the Plan 108 corrective line. It is **not** live mixed-router acceptance.

## 2. Entry criteria

Do not start implementation until `plans/109-status.md` exists and states at least:

```text
plan_109 = passed-record-and-noise-conformance
single_record_short_build_crypto = locally-conformant
short_build_derived_keys = locally-conformant
multirecord_short_build_message = pending-plan110
```

The following Plan 109 outputs must be available:

- canonical 154-byte request plaintext codec;
- canonical 218-byte encrypted request record primitive;
- canonical 202-byte reply plaintext codec;
- canonical 218-byte own-reply AEAD primitive;
- retained per-hop `replyKey`;
- retained per-hop `layerKey` and `ivKey`;
- saved post-request transcript hash `h`;
- validated record-slot parameter type or equivalent;
- independent one-record fixture evidence.

If these do not exist, return to Plan 109 rather than reimplementing them in Plan 110.

## 3. Normative sources

Primary authority:

- `https://i2p.net/en/docs/specs/tunnel-creation-ecies/`
- `https://i2p.net/en/docs/specs/i2np/`

Important current requirements:

- `ShortTunnelBuild` I2NP type = 25;
- `OutboundTunnelBuildReply` I2NP type = 26;
- each encrypted short record = 218 bytes;
- payload = one-byte record count followed by `count * 218` bytes;
- valid wire count = 1–8;
- typical/recommended count = 4;
- build record order must be randomized;
- fake records are required when record count exceeds real hop count;
- inbound builds require an originator fake record with correct 16-byte hash prefix and a real X25519 ephemeral key;
- the originator must detect modification of its inbound fake record;
- the hop's own reply record uses ChaCha20-Poly1305;
- every *other* record is transformed at each ECIES hop using raw ChaCha20 with that hop's `replyKey` and the target record slot encoded in the IV;
- creator reply postprocessing must undo the accumulated transformations before authenticating each hop's own reply.

Proposal 157 may be consulted for rationale but final specifications control.

## 4. Scope lock

### 4.1 In scope

Plan 110 owns:

- typed 1–8-slot record set;
- production/default minimum record-count policy;
- randomized mapping of real hops to unique slots;
- fake-record construction policy;
- inbound originator fake record and integrity validation;
- raw ChaCha20 per-record transform primitive/adaptor;
- creator request preprocessing;
- deterministic hop-processing simulation of a complete short-build message;
- creator reply postprocessing;
- exact STBM/OTBRM payload codec with one-byte count;
- mapping replies from randomized slots back to canonical hop order;
- state-machine integration and success-only registrar integration;
- negative/adversarial tests for slot/reordering/fake-record/tamper cases;
- independent multi-hop fixtures;
- support/documentation/status correction after local conformance passes.

### 4.2 Explicitly out of scope

Do not include:

- opening a real TCP/UDP connection;
- NTCP2 activation or generic NTCP2 repair;
- SSU2;
- Java I2P/i2pd/Emissary as a required closure target;
- generic I2NP router dispatch;
- implementation of garlic wrapping itself;
- full tunnel data-message forwarding;
- transit participation;
- NetDB live lookup/publication;
- LeaseSet/client-tunnel/streaming/SAM/I2CP work;
- privileged networking, namespaces, Docker, Multipass, or rootless harness resurrection;
- a new Python interoperability framework;
- new CI workflows dedicated to live interop.

The Plan 110 output is a byte-correct payload and state-machine result that a future delivery adapter can carry.

## 5. Architectural constraints

### 5.1 Runtime neutral

`i2pr-tunnel` remains free of:

- Tokio runtime ownership;
- sockets;
- DNS;
- filesystem/network I/O;
- daemon configuration coupling;
- NTCP2/SSU2 concrete transport types.

The builder emits bytes + typed delivery metadata. A future adapter chooses how those bytes move.

### 5.2 Bounded state

Wire count domain is exactly `1..=8`.

Production builder policy should use at least four records unless a narrowly documented internal/test mode intentionally constructs a smaller valid wire payload.

No unbounded pending build registry, retry history, slot list, fake-record list, or fixture corpus.

### 5.3 Explicit randomness

Use injected cryptographic randomness for:

- slot permutation;
- request padding;
- fake-record bytes/material;
- fake-record ephemeral key where required;
- any IDs allocated by this layer.

Tests use deterministic seeded RNGs.

Do not call a process-global PRNG from the protocol core.

## 6. Work package A — typed slot and record-set model

Introduce a representation that makes record slot a first-class protocol value.

Representative shapes:

```rust
#[repr(transparent)]
pub struct RecordSlot(u8); // validated 0..=7

pub enum RecordOwner {
    RealHop(HopIndex),
    OriginatorFake,
    PaddingFake,
}

pub struct ShortBuildRecordSet {
    count: NonZeroU8,
    slots: [/* bounded representation */],
}
```

Exact implementation is discretionary; invariants are mandatory.

### Required invariants

- count 0 rejected;
- count > 8 rejected;
- every slot index unique by construction;
- every real hop appears exactly once;
- no real hop shares a slot with any fake;
- slot lookup from hop and hop lookup from slot are deterministic for the lifetime of one build;
- path order is stored separately from slot order;
- no slot map is logged at normal runtime verbosity.

Prefer small fixed/bounded arrays over generic maps for a maximum of eight records.

## 7. Work package B — record-count policy

The I2NP parser must accept valid wire counts 1–8.

The local exploratory builder should use a privacy-preserving construction policy:

1. target at least four records;
2. if real hop count is less than four, add fakes until count is four;
3. for inbound builds, always reserve one fake for the originator, even when hop count would otherwise fill the target;
4. never exceed eight total records;
5. reject path/policy combinations that cannot satisfy these invariants within eight slots.

Example expected local policy:

```text
outbound 1 hop -> 4 records: 1 real + 3 fake
outbound 3 hop -> 4 records: 3 real + 1 fake
outbound 4 hop -> 4 records
inbound  1 hop -> 4 records: 1 real + originator fake + 2 other fake
inbound  3 hop -> 4 records: 3 real + originator fake
inbound  8 real hops -> invalid under mandatory originator-fake policy
```

If existing tunnel configuration permits path lengths incompatible with this, validate before allocation and return a typed policy error.

Do not silently drop a real hop to make room for a fake.

## 8. Work package C — randomized slot assignment

Assign records to slots using unbiased bounded randomness.

Requirements:

- permutation is independent from canonical path order;
- deterministic seeded tests reproduce exact permutations;
- no modulo-bias shortcut if drawing from a wider random integer domain;
- no duplicate slot assignment;
- real-hop and fake ownership map remains private state;
- the per-hop retained cryptographic context receives its actual `RecordSlot` before reply-key use.

Tests must include many deterministic seeds proving no structural path-order assumption remains.

Do not write a statistical randomness benchmark as a closure gate; structural correctness is sufficient.

## 9. Work package D — fake records

### 9.1 Generic fake records

For outbound builds and extra non-originator inbound fakes, generate a full 218-byte record that cannot be mistaken by the local creator for a real-hop record.

The spec permits random or implementation-specific content because these records are not intended for a participating hop.

Requirements:

- full 218 bytes initialized;
- generated through injected RNG;
- ownership tracked as fake;
- never fed into real-hop reply acceptance;
- creator can distinguish local fake ownership by slot map, not by public on-wire marker.

### 9.2 Mandatory inbound originator fake

Inbound builds require a special originator fake record.

It must contain:

- the correct first 16 bytes of the originator's RouterIdentity Hash;
- a real X25519 ephemeral public key in bytes 16–47;
- remaining bytes in a creator-defined protected/random form;
- enough creator-side retained integrity state to detect modification after the record traverses the new tunnel.

Use one bounded integrity mechanism. Preferred choices:

- store a cryptographic hash/MAC of the complete expected fake record and compare after creator postprocessing; or
- use an existing AEAD primitive with explicit test-only/reference semantics.

Do not create a new cryptographic protocol exposed to other routers; the mechanism is local originator self-validation only.

If any byte of the originator fake differs after expected transforms are undone, fail the entire build and do not register the tunnel.

### 9.3 Fake-record tests

Test:

- inbound builder always includes exactly one designated originator fake;
- prefix equals originator hash prefix;
- ephemeral field is a valid nonzero X25519 public key derived from generated private material;
- tampering at prefix, ephemeral, body, or tag/checksum is detected;
- outbound builder does not accidentally emit an originator fake requirement;
- fake slots never become `Established` hop replies.

## 10. Work package E — raw ChaCha20 transform

Plan 110 requires raw ChaCha20 stream encryption, not ChaCha20-Poly1305, for transforming *other* records.

Normative parameters for transforming a 218-byte target record:

```text
key = current hop's derived replyKey
slot = target record number 0..=7
iv = 12 bytes, all zero except iv[4] = slot
input = complete 218-byte encrypted record
output = ChaCha20(key, iv, input)
```

Because ChaCha20 is an XOR stream transform, applying the identical key/IV stream twice restores the original bytes.

### Dependency choice

If the existing `chacha20poly1305` dependency does not expose the raw stream primitive, add the narrow RustCrypto `chacha20` crate with minimal/default-disabled features where possible.

Document why it is required.

Do not use AEAD as a substitute for this transform.

Do not add libsodium/OpenSSL/FFI.

### Primitive API

Prefer a small internal helper that:

- requires `[u8; 32]` key or zeroizing key wrapper;
- requires validated `RecordSlot`;
- accepts exactly 218 bytes;
- operates in place where practical;
- exposes no arbitrary stream-cipher API outside the tunnel module unless genuinely reusable.

## 11. Work package F — creator request preprocessing

The creator begins with each real hop's individually ECIES-protected request record from Plan 109, then preprocesses records so each hop encounters its own asymmetrically protected record at the correct stage while earlier hops cannot infer path location from slot order.

For an all-ECIES path in canonical hop order `H0..H(n-1)`:

- `H0`'s own request record leaves the creator without a prior-hop stream transform;
- `H1`'s own request record is pre-transformed using `H0.replyKey` for `H1`'s assigned slot;
- `H2`'s own request record is pre-transformed using the reply keys of `H0` and `H1` for `H2`'s slot;
- generally, hop `Hj`'s own request record is pre-transformed once for every earlier real hop `Hi`, `i < j`, using the target record slot each time.

This must be implemented from the specification's iterative symmetric-processing model, not from path-slot coincidence.

Fake records must receive the transforms necessary for their expected lifetime through the hop chain and later validation.

### Required tests

For a three-hop/four-record deterministic fixture, freeze:

- initial individually protected records;
- randomized slot ownership;
- creator-preprocessed record bytes;
- bytes observed before each hop processes the message;
- proof that each hop can identify its own hash-prefix record at its stage;
- proof that earlier/later slot ordering does not matter.

## 12. Work package G — deterministic per-hop processing

Replace/extend the Plan 108 `DeterministicResponder` so it models an independent participating hop at the **message level**, not merely a call to the same high-level seal/open helper.

For hop `Hi`:

1. scan the current record set for the 16-byte truncated RouterIdentity hash matching itself;
2. reject zero or multiple matches;
3. validate basic record shape before DH;
4. open its own request using Plan 109's normative responder-side primitive;
5. derive its own `replyKey`, layer key, IV key, and saved transcript state;
6. validate request fields/options/policy;
7. create an exact 202-byte accepted/rejected reply plaintext;
8. AEAD-protect its **own slot** using `replyKey`, its slot number, and request `h`;
9. replace its own request record with the 218-byte own reply record;
10. apply raw ChaCha20 with its `replyKey` to every other 218-byte record, using each target record's slot IV;
11. forward the resulting record set to the next simulated hop.

The responder/hop processor must not call the creator's complete message-preprocessing routine as an oracle.

Low-level Plan 109 primitives may be shared; message-level control flow must be independently exercised.

### Rejection behavior

A malformed request, duplicate hash match, invalid DH, invalid AEAD, unsupported role/layer type, or invalid Mapping results in failure/rejection according to the current protocol semantics. Do not continue a locally malformed build as accepted.

## 13. Work package H — creator reply postprocessing

After the final hop has processed the record set, each real-hop reply may have accumulated raw-ChaCha20 transformations from later hops.

The creator must undo the transforms using its retained per-hop reply keys and actual slot map before AEAD-opening each hop's own reply.

For real hop `Hi`, apply the same raw ChaCha20 transform for every later hop `Hj`, `j > i`, using `Hj.replyKey` and `Hi`'s target slot. Then open `Hi`'s own reply with:

- `Hi.replyKey`;
- `Hi`'s slot number;
- `Hi`'s saved request transcript hash `h`.

Because raw ChaCha20 transformation is symmetric, applying the same stream removes the layer.

Do not infer which reply belongs to which hop from response contents. Use the private slot map established before dispatch.

### Build success rule

A build reaches `Established` only if:

- all real-hop records are recovered;
- all own-reply AEAD tags authenticate;
- every real hop returns accepted response;
- all reply options parse canonically;
- the inbound originator fake, when present, passes integrity validation;
- record count/slot map remains consistent with the pending build.

Any failure leaves the tunnel unregistered.

## 14. Work package I — exact I2NP short-build payload codec

Add a narrow payload representation; do not build generic router dispatch.

### 14.1 ShortTunnelBuild payload

Exact payload:

```text
byte 0      num, 1..=8
bytes 1..   num * 218 request records
```

Total size:

```text
1 + num * 218
```

Four records = 873 bytes.

### 14.2 OutboundTunnelBuildReply payload

Same shape using reply records:

```text
byte 0      num, 1..=8
bytes 1..   num * 218 reply records
```

### 14.3 Codec ownership

Wire-shape types/constants that are genuinely I2NP facts belong in `i2pr-proto` if consistent with existing architecture.

Tunnel construction policy/slot ownership stays in `i2pr-tunnel`.

### 14.4 Decoder rules

Reject before allocating/copying large buffers:

- empty input;
- count 0;
- count > 8;
- length not exactly `1 + count * 218`;
- trailing bytes;
- truncated record.

Do not accept the Plan 108 representation of concatenated records without the count byte as a complete I2NP payload.

## 15. Work package J — state-machine integration

Update `ShortBuildStateMachine` so its output is an exact payload and its retained context is sufficient for postprocessing.

Representative lifecycle:

```text
PreparedPath
 -> RecordsProtected
 -> SlotsAssigned
 -> Preprocessed
 -> ReadyForDelivery
 -> AwaitingReply
 -> PostprocessingReply
 -> Established
```

Terminal failures remain bounded:

```text
InvalidPath
CryptoFailed
MalformedMessage
DeliveryFailed
TimedOut
Cancelled
HopRejected
OriginatorFakeModified
InvalidReply
```

Exact enum names are discretionary.

### Delivery action

The delivery action should carry:

- first-hop RouterHash;
- exact I2NP payload bytes beginning with `num`;
- I2NP message kind/type information (`ShortTunnelBuild`);
- deadline/correlation ID;
- no secret key material;
- no separate record-count field required for reconstructing bytes, though a diagnostic count may be derived from the typed payload.

Avoid an API where the caller can accidentally send `records` while forgetting the count byte.

## 16. Work package K — registrar integration

Retain Plan 108's success-only pool registration architecture.

Strengthen the registrar input so `Established` cannot be constructed from a partial or preprocessed-only state without validated replies.

The established result must contain the tunnel data-plane material needed later:

- ordered participating peers;
- tunnel IDs;
- per-hop AES layer keys/IV keys derived by Plan 109;
- expiration/lifetime metadata;
- direction/role;
- no request ephemeral private material that is no longer needed.

Destroy reply keys and build-only transcript secrets after registration unless a concrete post-build protocol use requires retention.

## 17. Work package L — independent multi-hop conformance fixtures

Self-simulation is useful but not sufficient.

Add at least one frozen multi-hop fixture, preferably:

```text
3 real ECIES hops + 1 fake = 4 records
```

The fixture must include deterministic:

- hop RouterIdentity hashes;
- hop static ECIES keys;
- per-hop sender ephemeral keys;
- request plaintexts;
- individually protected records;
- slot permutation;
- fake record;
- creator-preprocessed payload;
- payload bytes observed at each hop boundary;
- per-hop reply plaintexts;
- message bytes after each hop;
- creator postprocessed reply records;
- final decoded accepted responses.

For inbound coverage, add a second fixture or focused vector with mandatory originator fake record and tamper detection.

### Independence

Expected multi-hop bytes must be generated by a small specification-derived reference path or independent fixture process, not by serializing the final product state machine and committing its output as expected data.

A compact test-only Rust reference routine is acceptable. Do not add a large Python harness.

## 18. Work package M — adversarial tests

Add tests for at least:

### Slot/count handling

- count 0 rejected;
- count 9 rejected;
- exact count/length mismatch rejected;
- duplicate slot ownership impossible;
- randomized slot order differs from path order for selected deterministic seeds;
- malformed/trailing payload rejected.

### Request path

- duplicate matching truncated hop hashes rejected by responder;
- no matching hash rejected;
- one record moved to another slot causes expected auth/postprocessing failure where slot nonce matters;
- ciphertext/tag corruption rejected;
- earlier-hop transform omitted -> later hop cannot authenticate/open correctly;
- extra transform -> failure.

### Reply path

- wrong slot nonce -> own reply authentication fails;
- one later-hop transform omitted during creator postprocessing -> failure;
- record permutation after dispatch -> build fails rather than mis-associating hop replies;
- one reject code 30 -> entire build `HopRejected`;
- malformed reply Mapping -> build fails;
- modified inbound originator fake -> build fails;
- no partial tunnel registration after any failure.

### Secret/state behavior

- no build secret appears in Debug output;
- terminal cancellation/failure drops pending secret contexts;
- successful registration discards build-only secrets;
- repeated deterministic builds do not accidentally reuse slot/per-hop ephemeral state when RNG stream advances.

## 19. Work package N — support and documentation correction

After implementation, update only documentation necessary to describe actual state.

Expected final local status if Plan 110 passes:

```text
short request/reply codecs          = locally conformant
Noise-N request protection          = locally conformant
short-build KDF                     = locally conformant
reply AEAD                          = locally conformant
record randomization                = implemented
fake records                        = implemented
ChaCha20 preprocessing              = locally conformant
STBM/OTBRM payload codec             = locally conformant
multi-hop deterministic fixture      = passed
live mixed-router build              = not run
qualified external delivery          = unavailable
normal-daemon NTCP2                  = disabled-and-unenableable
```

`README.md`, `docs/protocol-support.md`, `docs/architecture/i2pr-tunnel.md`, `specs/support.toml`, and planning/status records must not overclaim live interoperability.

If a support feature is marked simply `implemented`, ensure its description makes the local/no-live-validation boundary explicit.

## 20. Validation commands

Run the full Rust/local verification appropriate to the changed crates:

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

If the current static NTCP2 checks are routinely part of workspace closure, they may be run to prove no regression, but they are not evidence for Plan 110 tunnel conformance.

The obsolete rootless checker remains non-gating if its only failure is the known retired Plan 099 harness artifact.

No new live-network CI workflow is required.

## 21. Explicit acceptance criteria

Plan 110 passes only when **all** are satisfied.

### Record-set policy

- [ ] Wire parser accepts exactly counts 1–8 and rejects 0/>8.
- [ ] Local production builder uses at least four records.
- [ ] Every real hop has exactly one unique randomized slot.
- [ ] Path order and slot order are independent.
- [ ] Fake records fill unused privacy slots.
- [ ] Inbound builds always include the mandatory originator fake record.
- [ ] Inbound originator fake has correct hash prefix and real X25519 ephemeral key.
- [ ] Originator fake modification is detected before registration.

### ChaCha20 preprocessing

- [ ] Raw ChaCha20, not AEAD, is used for other-record transforms.
- [ ] Transform key is the acting hop's derived `replyKey`.
- [ ] Target record slot determines the IV/nonce exactly as specified.
- [ ] Creator preprocessing exposes each real hop's request only at the correct hop stage.
- [ ] Each hop replaces only its own slot with its AEAD-protected reply.
- [ ] Each hop transforms every other record exactly once with its reply key.
- [ ] Creator postprocessing removes the accumulated later-hop transforms correctly.

### Payload framing

- [ ] STBM payload begins with one-byte count.
- [ ] OTBRM payload begins with one-byte count.
- [ ] Total payload is exactly `1 + count * 218` bytes.
- [ ] Four-record payload is exactly 873 bytes.
- [ ] Trailing/truncated/count-mismatched inputs fail closed.
- [ ] State-machine delivery cannot accidentally omit the count byte.

### Reply/result semantics

- [ ] Every real-hop own reply authenticates with its Plan 109 reply key, slot, and saved `h`.
- [ ] Reply records are mapped to hops using private slot ownership, not content guesses.
- [ ] Any reject code 30 fails the whole build.
- [ ] Any malformed/tampered record fails the whole build.
- [ ] No partial tunnel is registered.
- [ ] Only fully validated `Established` result reaches `ExploratoryPool`.
- [ ] Registered tunnel retains correct layer/IV key material and drops build-only secrets.

### Evidence

- [ ] Independent 3-hop + 1-fake deterministic fixture passes byte-for-byte.
- [ ] At least one inbound-originator-fake fixture passes and tamper variant fails.
- [ ] Fixture expected bytes are not generated at runtime by the final product state machine.
- [ ] Negative tests cover slot, count, transform, Mapping, AEAD, rejection, and fake-integrity failures.

### Architecture/scope

- [ ] `i2pr-tunnel` remains runtime/transport neutral.
- [ ] No normal-daemon NTCP2 activation/change.
- [ ] No SSU2 or generic I2NP dispatch expansion.
- [ ] No live mixed-router claim.
- [ ] No new Python/namespace/privileged harness.
- [ ] Workspace validation passes except explicitly recorded pre-existing obsolete checker failures.

## 22. Closure record

On success, create `plans/110-status.md` containing at least:

```text
plan_109 = passed-record-and-noise-conformance
plan_110 = passed-local-short-build-protocol-conformance
plan_108 = superseded-by-corrected-109-110-protocol-path
short_build_wire_format = locally-conformant
short_build_noise_state = locally-conformant
short_build_multirecord_processing = locally-conformant
short_build_i2np_payload = locally-conformant
exploratory_pool_registration = success-gated-corrected
external_build_delivery = unavailable
live_mixed_router_build = blocked-on-qualified-delivery-only
normal_daemon_ntcp2 = disabled-and-unenableable
ntcp2 = experimental-non-advertised
```

Record:

- closure commit SHA;
- dependency additions (especially raw `chacha20`, if needed);
- deterministic fixture provenance and hashes;
- test count;
- exact supported builder policy for record count/path length;
- any still-deferred inbound garlic wrapping details;
- confirmation that no live interop was claimed.

## 23. Post-Plan-110 handoff

Only after Plan 110 passes should the project decide how to carry one already-correct STBM payload to an independent router.

That next plan must be a separate narrow delivery/integration decision based on a concrete consumer:

```text
correct local STBM bytes
  -> choose smallest available qualified delivery lane
  -> send one build
  -> observe one independent reply/result
```

Do not pre-authorize another generic NTCP2 interoperability framework. If NTCP2 remains the selected lane, revisit only the exact retained defect necessary to carry this payload. If an alternate compatible lane becomes available, evaluate it on the same narrow product requirement.

Plan 110 itself closes without any network access.