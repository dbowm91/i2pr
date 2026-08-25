# Plans 126–130 — Milestone 6 final corrective roadmap

## Status and authority

- **Current Milestone 6 authority.**
- Date: **2026-08-25**.
- Source floor: `29fb88d36f9794202e88d4b947faed30569c1991` (`client: land Plan 129 integrated destination-streaming final gate`).
- Plans 126–129 remain valuable implementation and regression evidence.
- The post-Plan-129 audit found four narrow local protocol/runtime defects that invalidate the stronger `milestone6_local_product = passed` claim until Plan 130 corrects them.
- **Plan 130 is the only remaining Milestone 6 implementation pass.** Do not create another broad validation program merely because the earlier Plan 129 status said the milestone was closed.

This roadmap supersedes `plans/126-129-milestone6-final-corrective-roadmap.md` as the active execution authority. The older roadmap and status files remain historical evidence and should not be rewritten to imply that their original closure conclusions were known in advance.

## What Plans 126–129 successfully established

The following work is retained and MUST NOT be discarded or broadly rewritten.

### Plan 126 — destination ECIES ratchet foundation

Plan 126 replaced the old i2pr-specific destination ECIES dialect with the current bound ECIES-X25519-AEAD-Ratchet structure:

- `Noise_IKelg2+hs2_25519_ChaChaPoly_SHA256` transcript/KDF contract;
- bound New Session with Alice's authenticated static X25519 public key inside the encrypted static section;
- New Session Reply and Existing Session messages identified through session tags rather than message-type flag bytes;
- directional post-Split tag sets and `AttachPayloadKDF`;
- bounded session/tag state and replay rejection;
- independent frozen cryptographic conformance vectors.

Plan 130 reopens **only the production on-wire Elligator2 representation randomization**, not the Noise/KDF/session architecture.

### Plan 127 — destination session routing closure

Plan 127 successfully composed the corrected destination ratchet with:

- Standard LeaseSet2 sender binding;
- validation that the sender LS2 type-4 key equals the authenticated NS static key;
- reverse route installation from the validated sender LS2;
- real destination outbound and inbound tunnel roles;
- a post-OBEP authenticated-router-link-bypassed local seam;
- a production New Session -> New Session Reply -> Existing Session lifecycle in both directions.

This layer remains closed unless Plan 130 exposes a concrete regression.

### Plan 128 — Streaming packet wire correction

Plan 128 successfully corrected major wire-format defects:

- the normative flag bit assignments;
- non-TLV, flag-ordered option data;
- variable-length signatures and exact zeroed-signature preimages;
- two-byte `MAX_PACKET_SIZE` payload semantics;
- initial-SYN replay binding and `NO_ACK` placement;
- SYN-response zero-NACK / no-`NO_ACK` shape;
- canonical CLOSE/RESET flag sets.

Plan 130 reopens **sequence-number and ACK/NACK behavior only**. Do not regress the corrected packet codec.

### Plan 129 — full local stack composition

Plan 129 materially improved the evidence quality: its authoritative trajectories no longer transfer `TransportSendRequest` directly between Streaming managers. They traverse:

```text
Streaming packet
 -> I2P protocol-6 gzip ClientPayload
 -> I2NP Data
 -> ECIES NS / NSR / ES
 -> I2NP Garlic
 -> destination outbound tunnel
 -> OBEP
 -> authenticated-router-link-bypassed local seam
 -> destination inbound tunnel
 -> destination dispatcher
 -> ECIES authentication/decryption
 -> I2NP Data
 -> gzip ClientPayload decode
 -> Streaming packet
```

It also added integrated retransmission, reorder, duplicate, corruption, CLOSE, and RESET coverage.

Plan 130 retains this architecture but fixes two boundary/evidence defects: I2P destination-port routing and persistence of tunnel duplicate-window state in replay tests.

## Why Milestone 6 is reopened after Plan 129

### A. Production Elligator2 representatives are fingerprintable

At the source floor, production `EciesEphemeralKeypair::from_seed_bytes()` canonicalizes the seed and calls the Elligator library with a fixed tweak/high-bit choice. This produces an on-wire representation whose unused high bits are not randomized as expected by I2P reference implementations.

The current Java I2P implementation explicitly distinguishes deterministic/test Elligator encoding from the randomized on-wire form. Production i2pr must retain the library-backed Elligator mapping while restoring compatible output entropy. This is an anonymity/fingerprinting defect, not a cosmetic encoding preference.

### B. Streaming application data starts at sequence zero

At the source floor, `SendWindowPolicy` and `RecvWindowPolicy` begin ordinary application sequencing at `0`. In I2P Streaming, sequence zero belongs to the SYN/simple-ACK control space; post-SYN application messages start at sequence one. A non-SYN sequence-zero packet is not ordinary data to a conforming peer.

The Plan 129 self-to-self trajectory therefore accepts a wire behavior that external peers would interpret differently.

### C. Standalone ACK / ACK-through behavior is incomplete

The source floor treats `ackThrough == 0` as if it carried no acknowledgement information and relies on reverse application traffic to piggyback ACK state. That fails for one-way streams and conflates the numeric value zero with absence of ACK semantics. The router needs a runtime-neutral delayed/simple ACK path and correct ACK/NACK generation without introducing sockets or a new scheduler framework.

### D. Decoded I2P destination ports do not select the listener

`StreamingDestinationAdapter::receive()` decodes the protocol-6 ClientPayload's `destination_port`, but a separate caller-supplied `listener_port` currently determines where an inbound SYN is queued. The runtime can therefore route a valid packet addressed to one I2P port into another listener. Port routing belongs at this adapter/Streaming boundary and must be derived from the wire metadata.

### E. Plan 129 recreates inbound tunnel replay state

The Plan 129 fixture rebuilds the receiver's inbound role chain before deliveries. That weakens the claim that an exact router-delivery replay was rejected by the **same live tunnel duplicate window**. Plan 130 must preserve the tunnel role/window across ordinary fixture deliveries and separately prove tunnel replay rejection versus higher-layer ECIES/Streaming duplicate suppression.

## Execution order

There is one remaining pass:

### Plan 130 — final wire/runtime corrective closure

`plans/130-m6-final-wire-runtime-corrective-closure.md`

Plan 130 SHALL:

1. restore I2P-compatible randomized production Elligator2 representatives without hand-rolling Elligator2;
2. correct post-SYN Streaming sequence numbering;
3. implement correct runtime-neutral ACK/NACK semantics including standalone delayed/simple ACKs;
4. make decoded I2P destination-port metadata authoritative for inbound listener/connection routing;
5. preserve live tunnel duplicate-window state in the integrated fixture;
6. rerun the existing Plan 129 full-stack trajectories with focused regressions for these defects;
7. synchronize Milestone 6 status only after all acceptance criteria pass.

## Current authoritative classification

Until Plan 130 passes:

```text
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = corrective-reopened-through-plan130-elligator-wire-randomization
plan_122 = passed-corrected-local-destination-routing
plan_123 = corrective-reopened-through-plan130-sequence-ack-semantics
plan_124 = passed-corrected-destination-routing-local-closure
plan_125 = superseded-by-final-corrective-line
plan_126 = corrective-reopened-plan130-elligator-wire-randomization
plan_127 = passed-destination-session-routing-final-closure
plan_128 = corrective-reopened-plan130-streaming-sequence-ack
plan_129 = corrective-reopened-plan130
plan_130 = ready-for-execution
milestone6_local_product = not-closed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = hold-for-plan130-only
next = plans/130-m6-final-wire-runtime-corrective-closure.md
```

## Non-goals / anti-overengineering rules

Plan 130 MUST NOT reopen or introduce:

- NTCP2 or SSU2 daemon activation;
- Plan 116 / Plan 117 host-lane work;
- external-router or public-network interoperability as a closure prerequisite;
- Docker, rootless namespaces, Multipass, QEMU, VMs, privileged host setup, or new CI hosts;
- Python interoperability harnesses or another general-purpose test framework;
- SAM, I2CP socket APIs, HTTP proxy, SOCKS proxy, service tunnels, or application-facing runtime work;
- a hand-written Elligator2 mapping;
- broad ECIES or Streaming redesign unrelated to the four defects above.

Use the existing Rust protocol/runtime types and Plan 129 integration fixture. Small production abstractions required for ACK scheduling or port ownership are appropriate; generalized harness infrastructure is not.

## Normative references

Use current official I2P specifications as authority and current Java I2P / i2pd source as independent behavioral references where specification prose is ambiguous:

- `https://i2p.net/en/docs/specs/ecies/`
- `https://i2p.net/en/proposals/144-ecies-x25519-aead-ratchet/`
- `https://i2p.net/en/docs/specs/streaming/`
- `https://i2p.net/en/docs/api/streaming/`
- `https://i2p.net/en/proposals/164-streaming/`
- `https://i2p.net/en/docs/specs/i2cp-overview/`
- current Java I2P Elligator2 and Streaming implementations;
- current/pinned i2pd ECIES ratchet implementation.

Do not infer wire behavior solely from i2pr's existing self-tests.

## Final Milestone 6 gate

Plan 130 may restore `milestone6_local_product = passed` only when all of the following are true:

```text
Standard LeaseSet2 binding                 retained
ECIES NS/NSR/ES wire/KDF                   retained
Production Elligator2 representation       compatible and non-fingerprintable
Destination tunnel composition             retained both directions
Streaming packet flags/options/signatures  retained current-wire-correct
Post-SYN sequence numbering                current I2P semantics
ACK / NACK / standalone ACK                current I2P semantics
I2P destination-port routing               authoritative at inbound adapter
Tunnel duplicate-window evidence           persistent-state proof
Integrated loss/reorder/retransmit          green over full destination stack
CLOSE / RESET                              green over full destination stack
Resource ceilings                          explicit and bounded
Mixed-router interoperability              not falsely claimed
```

If and only if that gate passes, the next product layer is **SAM baseline planning (Milestone 7)**. External mixed-router acceptance remains separate debt and must not be smuggled back into the local Milestone 6 gate.