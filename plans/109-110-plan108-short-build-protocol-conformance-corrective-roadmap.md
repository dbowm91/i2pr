# Plans 109–110 corrective roadmap: Plan 108 short-build protocol conformance

- Status: **active corrective authority**
- Date: 2026-08-15
- Parent authority: Plan 102 + `plans/102-amendment-exploratory-tunnel-dependency.md`
- Corrects: Plan 108 (`plans/108-live-ecies-x25519-short-tunnel-build-construction.md` and `plans/108-status.md`)
- Milestone: 5 — exploratory tunnel construction
- Scope class: **protocol-conformance correction only; no live-network acceptance gate**

## 1. Why this corrective sequence exists

Plan 108 landed a useful runtime-neutral tunnel-build architecture, but the implementation that was recorded as `passed-local-short-build-construction` does not encode or protect short tunnel-build records according to the current I2P ECIES-X25519 tunnel-creation specification.

The architecture should be retained where it remains useful:

- bounded `ShortBuildStateMachine` ownership;
- explicit transport-independent delivery actions/events;
- success-only `ExploratoryPool` registration;
- typed errors and bounded allocations;
- fail-closed default behavior;
- runtime-neutral `i2pr-tunnel` boundaries;
- deterministic testing.

The wire and cryptographic semantics must be corrected before any live mixed-router validation is attempted.

Current local tests mostly prove that i2pr's creator and responder implementations agree with each other. They do **not** currently prove that either side agrees with I2P. This distinction is the central corrective requirement.

## 2. Authoritative specification basis

Use the current official I2P specifications as normative authority:

1. Tunnel Creation Specification (ECIES-X25519):
   `https://i2p.net/en/docs/specs/tunnel-creation-ecies/`
2. I2NP Specification:
   `https://i2p.net/en/docs/specs/i2np/`
3. Common Structures Specification for `Mapping` semantics and Router Identity hashes.
4. Low-level/current referenced cryptographic specifications only where the tunnel-creation document delegates primitive behavior.
5. Proposal 157 may be used as historical rationale, but the current Tunnel Creation and I2NP specifications supersede proposal text where they differ.

Do not derive protocol behavior from Java I2P, i2pd, Emissary, or another implementation unless used only as an **independent post-implementation comparison**. Product code must remain clean-room with specification-derived behavior.

## 3. Confirmed Plan 108 conformance defects

The following are known defects and are not optional cleanup items.

### 3.1 Short request plaintext layout

Current Plan 108 encoding uses incorrect offsets/semantics for flags, layer-encryption type, timestamps, expiration, message ID, options, and padding.

Normative 154-byte plaintext layout:

```text
0..=3     receive tunnel ID, u32 BE, nonzero
4..=7     next tunnel ID, u32 BE, nonzero
8..=39    next router identity hash
40        flags
41..=42   additional flags, zero
43        layer encryption type
44..=47   request time, minutes since Unix epoch, u32 BE
48..=51   request expiration, seconds since creation, u32 BE
52..=55   next message ID, u32 BE
56..x     Mapping, including its normal two-byte length
x..153    required additional data if defined, then random padding
```

Current supported flags are:

```text
IBGW = bit 7 = 0x80
OBEP = bit 6 = 0x40
participant = 0x00
0xC0 is invalid
bits 5..0 must be zero
```

Current supported layer encryption type is **0 (AES)**. Plan 108's `0x05` is invalid for this field.

Current supported expiration is **600 seconds**.

The Mapping occupies up to 98 bytes including its two-byte length field. Empty Mapping is exactly `00 00`. Remaining plaintext is padding, not a custom single-byte mapping length scheme.

### 3.2 Short encrypted request record

Normative 218-byte encrypted request record:

```text
0..=15    first 16 bytes of hop RouterIdentity Hash
16..=47   sender ephemeral X25519 public key
48..=201  154-byte request ciphertext
202..=217 16-byte Poly1305 tag
```

The record does **not** contain a separate 16-byte nonce field. Plan 108's current `ephemeral || nonce || ciphertext+tag` envelope must be replaced.

### 3.3 Noise N transcript and request KDF

The current custom labels such as `ECIES-X25519-Build-Session-v1`, custom request-key seeds, custom derived nonces, and empty AEAD associated data are not normative.

The implementation must follow the specification's Noise `N` state:

- protocol name `Noise_N_25519_ChaChaPoly_SHA256`;
- exact initial `h` and `ck` construction;
- null-prologue `MixHash`;
- `MixHash(hepk)`;
- unique sender ephemeral keypair per ECIES hop;
- `MixHash(sepk)`;
- X25519 `es` DH;
- `HKDF(chainKey, sharedSecret, "", 64)`;
- request AEAD key from the second half of derived keydata;
- nonce `n = 0` in Noise ChaChaPoly form;
- associated data = transcript hash `h`;
- `MixHash(ciphertext)` after encrypt/decrypt;
- retention of resulting `ck` and `h` for the short-record KDF and reply processing.

### 3.4 Short-record derived keys

After request encryption/decryption, derive the actual short-build key material from `ck` using the normative labels and order:

```text
SMTunnelReplyKey
SMTunnelLayerKey
TunnelLayerIVKey       (OBEP-specific continuation)
RGarlicKeyAndTag       (OBEP garlic-reply material)
```

The derived state must retain only material necessary for reply processing and eventual tunnel data-plane use, with secret zeroization and no logging.

### 3.5 Short reply plaintext and reply encryption

Normative 202-byte reply plaintext:

```text
0..x      Mapping, including normal two-byte length
x..200    random padding / option-defined data
201       reply byte
```

Current accepted response codes are:

```text
0x00 = accept
30   = TUNNEL_REJECT_BANDWIDTH
```

The hop's own encrypted reply record is exactly 218 bytes:

```text
0..=201   ChaCha20-Poly1305 ciphertext of the 202-byte plaintext
202..=217 Poly1305 tag
```

There is no fresh reply ephemeral key and no extra reply nonce field.

For the hop's own reply record:

- key = derived `replyKey`;
- nonce = record slot number 0–7 encoded per the specification;
- associated data = saved request transcript hash `h`.

### 3.6 Multi-record processing and message shape

Plan 108 did not implement the required complete multi-record behavior.

Required corrections include:

- randomized record order;
- unique slot assignment;
- recommended minimum four records;
- fake records when record count exceeds real hops;
- mandatory originator fake record for inbound builds with correct hash prefix + real X25519 ephemeral key;
- originator validation of its inbound fake record against modification;
- iterative ChaCha20 processing of the *other* 218-byte records at each hop using the derived reply key and record-position IV;
- creator-side preprocessing and reply postprocessing in the correct order;
- I2NP `ShortTunnelBuild` and `OutboundTunnelBuildReply` payloads beginning with one-byte `num`, followed by `num * 218` bytes.

## 4. Corrective execution sequence

The work is deliberately split into two plans.

```text
Plan 108 implementation
   -> Plan 109 exact short-record + Noise-N + derived-key conformance
   -> Plan 110 multi-record preprocessing + I2NP framing + independent conformance validation
   -> narrow external-delivery decision/checkpoint
```

### Plan 109

`plans/109-short-build-record-and-noise-conformance-correction.md`

Owns only:

- exact request/reply plaintext formats;
- exact 218-byte request/reply cryptographic envelopes;
- literal Noise-N transcript/state;
- short-record KDF and retained key material;
- one-record creator/responder correctness;
- replacement/removal of non-normative Plan 108 cryptographic concepts;
- conformance fixtures/vectors that do not depend on i2pr round-tripping against itself.

Plan 109 must **not** claim a complete `ShortTunnelBuild` message implementation.

### Plan 110

`plans/110-short-build-multirecord-preprocessing-and-conformance-closure.md`

Depends on Plan 109 and owns only:

- randomized slot assignment;
- fake records and inbound originator fake-record integrity;
- raw ChaCha20 preprocessing/postprocessing of other records;
- slot-number-based reply AEAD nonces;
- complete one-byte-count I2NP payload serialization/parsing;
- multi-hop deterministic simulation using an independently specified responder path;
- negative/adversarial conformance tests;
- documentation/support-registry correction;
- a final local protocol-conformance closure decision.

Plan 110 must **not** activate a network transport or perform live mixed-router acceptance.

## 5. Explicitly deferred work

Do not include any of the following in Plans 109 or 110:

- normal-daemon NTCP2 activation;
- NTCP2 defect repair beyond a concrete compile dependency accidentally exposed by the corrective code;
- SSU2;
- Java I2P/i2pd/Emissary live validation as a closure gate;
- network namespaces, Docker, Multipass, rootless network isolation, or privileged test infrastructure;
- new Python interoperability frameworks;
- generic I2NP router dispatch;
- garlic-message implementation beyond deriving/storing the specified OBEP garlic reply key/tag where required by the short-build KDF;
- live NetDB lookup/publication;
- transit participation;
- destination/client tunnel pools;
- LeaseSet, streaming, SAM, I2CP, HTTP/SOCKS proxy work;
- broad performance benchmarking;
- unrelated dependency upgrades.

## 6. Independent validation policy

A test is not protocol-conformance evidence merely because this sequence succeeds:

```text
i2pr seal -> i2pr open
```

At least one independent basis must exist for every load-bearing cryptographic/wire property. Acceptable forms, in descending preference:

1. official I2P published vector/fixture with known intermediate values;
2. a small specification-derived fixture generated by a standalone test implementation that does not call the product primitive under test;
3. a fixture generated once by an independent maintained router/tool and committed with provenance + hash;
4. manual step-by-step expected intermediate values derived from the normative algorithm and frozen as test data.

Do not add a large external harness merely to satisfy this rule. A small fixture file or test-only reference function is preferred.

For Noise state tests, freeze intermediate values where practical:

- initial `h`;
- initial `ck`;
- `h` after peer static key;
- ephemeral public key;
- `h` after ephemeral key;
- DH output;
- post-`MixKey` `ck` and request AEAD key;
- ciphertext/tag;
- post-ciphertext `h`;
- `replyKey`;
- `layerKey`;
- `ivKey`;
- OBEP-specific garlic reply material where applicable.

These fixtures must be deterministic and contain only test keys.

## 7. Stop conditions

Stop implementation and record the blocker instead of broadening scope if any of these occur:

- current official documentation is internally ambiguous in a way that affects wire bytes or cryptographic state;
- correct inbound-creator ephemeral-key placement cannot be resolved from current normative material;
- a required primitive cannot be implemented with the current pure-Rust dependency policy without a substantial dependency expansion;
- independent fixtures materially disagree with the specification-derived implementation after review;
- completing Plan 110 would require a live transport merely to establish local wire correctness.

A stop condition produces a narrow research/correction plan, not a return to generic interoperability infrastructure.

## 8. Authority and closure states

Until Plan 109 passes, use:

```text
plan_108 = implementation-landed-protocol-conformance-reopened
short_build_record_format = nonconformant
short_build_noise_state = nonconformant
short_build_reply_crypto = nonconformant
short_build_multirecord_processing = missing
live_mixed_router_build = blocked-do-not-attempt
```

After Plan 109 only:

```text
plan_109 = passed-record-and-noise-conformance
single_record_short_build_crypto = locally-conformant
multirecord_short_build_message = not-yet-conformant
live_mixed_router_build = blocked-on-plan110-and-qualified-delivery
```

After Plan 110 only if every acceptance criterion passes:

```text
plan_110 = passed-local-short-build-protocol-conformance
short_build_wire_format = locally-conformant
short_build_noise_state = locally-conformant
short_build_multirecord_processing = locally-conformant
short_build_i2np_payload = locally-conformant
live_mixed_router_build = blocked-on-qualified-delivery-only
```

No local closure state may use `interoperable`, `live`, or `network-validated` terminology.

## 9. Handoff

The next executable plan is **Plan 109**.

Do not begin Plan 110 until Plan 109's closure record explicitly states that the single-record request/reply cryptographic path and its retained derived-key context match the current official specification and independent fixtures.