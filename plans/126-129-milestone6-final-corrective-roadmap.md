# Plans 126–129 — Milestone 6 final corrective roadmap

## Status and authority

- **Ready for execution.**
- Date: **2026-08-25**.
- Source floor: `523d5dcd87f6c04853a016f7b54e3922697ffb2b`.
- This roadmap is the **authoritative Milestone 6 execution order** until Plan 129 closes.
- It supersedes the stronger `milestone6_local_product = passed` statements written by the Plan 125 implementation pass.
- Historical Plan 119–125 work remains useful evidence; this roadmap does not erase it. It narrows the remaining defects that must be corrected before SAM work begins.

## Why Milestone 6 is reopened

The post-Plan-125 audit found three classes of protocol defects plus one evidence gap.

### A. Destination ECIES ratchet is not yet I2P wire compatible

The current `i2pr-crypto::ecies` / `i2pr-client::session` surface uses an i2pr-specific representation that does not match the current I2P ECIES-X25519-AEAD-Ratchet specification. Material examples at the source floor include:

- `ECIES_NOISE_PROTOCOL_NAME = "Noise_NK_25519_ChaChaPoly_SHA256"`, while the current destination ratchet specification is based on `Noise_IK_25519_ChaChaPoly_SHA256` and uses the I2P initializer `Noise_IKelg2_25519_ChaChaPoly_SHA256`;
- `NewSessionMessage` serializes an i2pr-specific leading `0xE0` marker and a clear `static_key` field before the Elligator2 representative, while the I2P bound New Session encrypted-data format begins with the 32-byte Elligator2 representative and carries Alice's static key inside the authenticated encrypted static-key section;
- `0xE2` is currently used as a combined New-Session-Reply / Existing-Session marker, but the real protocol distinguishes New Session Reply and Existing Session messages through their 8-byte session tags and retained session state, not a message-type byte;
- the manager discards the initial outbound session state when building a New Session, keys received state by ephemeral material rather than the far-end Destination, and does not provide a complete production New Session Reply -> Existing Session lifecycle;
- the dispatcher treats all `0xE2` messages as New Session Replies and therefore has no real Existing Session receive path.

The official ECIES specification states that sessions are paired and bound to the far-end Destination; for repliable traffic such as Streaming, Alice's static X25519 key is included in the New Session and Bob binds it to Alice only after discovering a LeaseSet whose type-4 key matches that authenticated static key.

### B. Plan 124 fixed the plaintext tunnel bug, but did not complete the destination-session closure

The important Plan 124 invariant is retained: `compose_outbound_delivery()` now sends standard-encoded I2NP Garlic bytes through the outbound tunnel instead of plaintext I2NP Data bytes.

However, the full acceptance originally required:

```text
A -> B bound New Session
B -> A New Session Reply
A -> B Existing Session
B -> A Existing Session
```

through the destination-owned outbound/inbound tunnel path. That complete production session lifecycle was not demonstrated.

### C. Streaming wire framing is still non-standard below the corrected gzip layer

Plan 125 correctly replaced the custom zlib/SHA envelope with I2P's RFC 1952 client-payload gzip framing and corrected the optimistic connection-state transition. Those fixes are retained.

The Streaming packet codec still has wire incompatibilities:

- incorrect flag bit assignments;
- invented TLV-style Streaming option data;
- 4-byte MAX_PACKET_SIZE instead of the specified 2-byte integer;
- fixed 64-byte/TLV signature parsing rather than flag-ordered raw Signature data whose length comes from the signing-key type;
- replay-prevention NACKs placed on the SYN response even though they are initial-SYN-only;
- `NO_ACK` placed on the SYN response rather than the initial SYN;
- the 1730-byte default MTU is currently treated as a full packet ceiling / reduced by the 22-byte header, while I2P defines the negotiated maximum as **payload bytes only**.

### D. No Milestone 6 test traverses the complete corrected stack

Plan 125 added `StreamingDestinationAdapter`, but its closure tests still pass client-payload bytes directly between `StreamingManager` instances. There is no authoritative trajectory that carries:

```text
Streaming
 -> I2P gzip client payload
 -> I2NP Data
 -> bound ECIES NS/NSR/ES
 -> outbound destination tunnel
 -> OBEP
 -> explicit local router-link seam
 -> inbound destination tunnel
 -> destination owner dispatch
 -> ECIES authentication/decryption
 -> I2NP Data
 -> gzip decode
 -> Streaming
```

in both directions.

## Normative references for the corrective passes

Use current official I2P specifications as protocol authority:

- `https://i2p.net/en/docs/specs/ecies/`
- `https://i2p.net/en/proposals/144-ecies-x25519-aead-ratchet/`
- `https://i2p.net/en/docs/specs/common-structures/`
- `https://i2p.net/en/docs/specs/streaming/`
- `https://i2p.net/en/proposals/164-streaming/`
- `https://i2p.net/en/docs/api/streaming/`
- `https://i2p.net/en/docs/specs/i2cp-overview/`

Current Java I2P and i2pd source may be used as clean-room behavioral references when the prose specification leaves an implementation detail ambiguous. Do not copy implementation code.

## Execution order

### Plan 126 — ECIES destination wire and session corrective foundation

`plans/126-m6-ecies-destination-ratchet-corrective-foundation.md`

Correct the destination ECIES encrypted-data wire formats, KDF/transcript contract, session-tag classification, bound New Session / New Session Reply pairing, and production session-manager ownership.

This pass must end with a true primitive/manager-level:

```text
NS -> NSR -> ES A->B -> ES B->A
```

trajectory before routing/tunnels are added back.

### Plan 127 — destination routing/session corrective closure

`plans/127-m6-destination-session-routing-final-closure.md`

Compose the corrected Plan 126 ratchet through LeaseSet2 binding, destination routing, both destination tunnel directions, dispatcher ownership, reverse routing, NSR, and Existing Session traffic.

This is the final corrective closure for the Plan 121 / 122 / 124 destination layer.

### Plan 128 — Streaming wire-protocol corrective closure

`plans/128-m6-streaming-wire-protocol-corrective-closure.md`

Correct the Streaming flag map, option-data format/order, signatures, replay NACK policy, NO_ACK policy, MTU semantics, and current SYN/SYN-response wire behavior. Keep the Plan 125 gzip and state-machine fixes.

This pass remains transport-neutral and may use fast direct Streaming tests.

### Plan 129 — integrated Milestone 6 local-product gate

`plans/129-m6-integrated-destination-streaming-final-gate.md`

Add the inbound Streaming adapter and drive one authoritative two-destination test through the complete corrected stack. Re-run loss, duplicate, reordering, retransmission, CLOSE, RESET, and corruption cases at protocol-appropriate seams.

Only Plan 129 may restore:

```text
milestone6_local_product = passed
```

## No-go / anti-overengineering rules

Plans 126–129 MUST NOT reopen:

- live NTCP2 or SSU2 transport activation;
- Plan 116 / Plan 117 external host-lane work;
- Docker, rootless namespaces, Multipass, QEMU, VMs, or privileged host setup;
- public I2P network participation;
- Python interoperability harnesses;
- a new general-purpose test framework;
- SAM, I2CP socket APIs, HTTP proxy, SOCKS proxy, or service tunnels.

Use ordinary Rust unit/integration tests and the existing tunnel/data-plane types. A narrow test-local fixture is preferable to another reusable harness unless production code genuinely needs the abstraction.

## Milestone 6 status after closure

Plan 126 closed as `passed-ecies-destination-ratchet-corrective-foundation`
(`plans/126-status.md`). Plan 127 closed as
`passed-destination-session-routing-final-closure`
(`plans/127-status.md`), restoring the local destination-layer claims.
Plan 128 closed as `passed-streaming-wire-protocol-corrective-closure`
(`plans/128-status.md`) and restored Plan 123/125 as streaming-wire
correct. Plan 129 closed as
`passed-milestone6-integrated-local-product-gate`
([`plans/129-status.md`](129-status.md)):

```text
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = passed-corrected-ecies-destination-session-layer-local
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-streaming-wire-local
plan_124 = passed-corrected-destination-routing-local-closure
plan_125 = superseded-by-final-corrective-closure
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
plan_127 = passed-destination-session-routing-final-closure
plan_128 = passed-streaming-wire-protocol-corrective-closure
plan_129 = passed-milestone6-integrated-local-product-gate
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
next_product_layer = SAM baseline planning (Milestone 7)
```

## Final gate

The Plan 129 integrated gate **passed** on 2026-08-25; the closure record is [`plans/129-status.md`](129-status.md). Every row of the gate below is satisfied by the Plan 129 integrated trajectory suite (`crates/i2pr-client/tests/plan129_trajectory.rs`):

```text
Standard LeaseSet2                         correct local product path      [x]
ECIES bound NS/NSR/ES wire                corrected to current spec       [x]
Destination session binding/pairing       correct and bounded             [x]
Garlic-through-tunnel composition         correct in both directions      [x]
Streaming client payload gzip             current I2P format              [x]
Streaming packet wire format              current I2P format              [x]
Streaming handshake                       real SYN/SYN response           [x]
Streaming data/ACK/NACK/retransmit         works over destination stack    [x]
CLOSE/RESET                               works over destination stack     [x]
Resource ceilings                         retained                        [x]
Mixed-router interoperability             not falsely claimed             [x]
```

With that gate passed, the next product layer is **SAM baseline planning (Milestone 7)**. Do not create another Milestone 6 plan merely for generalized validation unless a concrete local protocol/product defect appears.

```text
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = passed-corrected-ecies-destination-session-layer-local
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-streaming-wire-local
plan_124 = passed-corrected-destination-routing-local-closure
plan_125 = superseded-by-final-corrective-closure
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
plan_127 = passed-destination-session-routing-final-closure
plan_128 = passed-streaming-wire-protocol-corrective-closure
plan_129 = passed-milestone6-integrated-local-product-gate
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```