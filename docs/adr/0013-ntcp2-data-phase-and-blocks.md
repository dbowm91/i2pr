# ADR 0013: NTCP2 data-phase frames and payload blocks

## Status

Accepted for Plan 034. This decision is limited to runtime-neutral protocol
composition and local deterministic evidence; it does not authorize sockets,
Tokio tasks, NetDB mutation, publication, or capability advertisement.

## Context

Plan 033 produces distinct handshake transmit/receive split keys. Plan 034
needs a bounded data phase without allowing frame lengths, block counts,
unknown extensions, or payload ownership to become allocation or logging
hazards. The current NTCP2 specification defines SipHash-obfuscated lengths,
ChaCha20-Poly1305 frames with empty associated data, and typed payload blocks.
It does not define a periodic in-session data rekey threshold.

## Decisions

1. `TransmitState` and `ReceiveState` own independent cipher and SipHash
   states. `AuthenticatedHandshake::into_data_phase` consumes the combined
   split owner into those directions.
2. Clear ciphertext lengths are 16..=65,535 bytes, including the 16-byte tag;
   plaintext is at most 65,519 bytes. The clear length is validated before a
   receive buffer is admitted. Prefixes are encoded big-endian after XOR with
   the next little-endian SipHash-derived mask.
3. AEAD authentication is a hard gate. Block parsing and unknown-block
   skipping occur only after successful opening. Authentication, malformed
   block, terminal ordering, and counter failures make the direction owner
   terminal; failed authentication is never retried with reused state.
4. Blocks have a 1-byte type and 2-byte big-endian length. Types 0, 1, 2, 3,
   4, and 254 are implemented for timestamp, options, RouterInfo, I2NP,
   termination, and padding. Unknown types are skipped as bounded padding only
   after authentication, with 256 blocks and 4,096 unknown bytes as aggregate
   limits. General data-phase non-padding blocks may repeat where permitted;
   Padding is at most once and last, and Termination is at most once and the
   last non-padding block. The separate SessionConfirmed part-two parser keeps
   its strict RouterInfo/Options/Padding ordering and singleton rules.
5. Outbound I2NP blocks consume the existing bounded
   `EncodedI2npMessage`. Inbound I2NP views borrow the authenticated plaintext
   owner and only copy at an explicit transport handoff. RouterInfo is decoded,
   signature-verified, and returned as a candidate; no NetDB or publication
   side effect occurs here.
6. Padding/coalescing is represented as a pure bounded policy input. This
   layer never waits for more messages, selects queues, or generates runtime
   randomness. Deterministic tests use fixed zero padding; the Plan 042
   runtime-owned driver supplies compliant random padding and scheduling
   decisions.
7. Because the current specification supplies no data-phase rekey threshold,
   this implementation does not invent one. The last permitted nonce is
   usable, the forbidden `2^64 - 1` value is never emitted, and exhaustion or
   static-key/IV rotation requires a fresh Noise handshake.

## Consequences

The protocol crate can prove framing, block bounds, ownership, terminal-state,
and malformed-input behavior without sockets or a runtime. Plan 042's driver
retains partial length/ciphertext buffers under explicit resource leases,
flushes complete encoded frames, applies deadlines/cancellation, and performs
policy actions only after typed data-phase output. The non-production launcher
now composes handshake-to-link and local smoke exchange; local vectors,
loopback, and launcher results remain experimental evidence and do not
establish interoperability.

## Plan 042 smoke-message scope

The initial bounded I2NP smoke scope is `DeliveryStatus` (I2NP type 10), whose
fixed body is 12 bytes. The NTCP2/SSU2 short transport representation is
21 bytes (9-byte short header plus body), and its NTCP2 block is 24 bytes before
frame overhead and padding. The intended positive result requires one valid
outbound and one valid inbound DeliveryStatus in each direction, with no
payload retention in status or evidence. This scope does not imply that either
reference echoes or otherwise acknowledges the message; that behavior must be
verified during an authorized run before it can support an interoperability
claim.

## Evidence

- `crates/i2pr-transport-ntcp2/src/block.rs`
- `crates/i2pr-transport-ntcp2/src/frame.rs`
- `crates/i2pr-transport-ntcp2/src/crypto.rs`
- `crates/i2pr-testkit/src/ntcp2.rs`
- `tests/fixtures/ntcp2/crypto/manifest.tsv`
- `plans/034-closure.md`

## Plan 037 corrective amendment

The original singleton and Termination-first wording applied the handshake
payload constraints to general data frames. The implementation now keeps those
contexts separate: authenticated general frames accept specification-permitted
repeated non-padding blocks and allow valid blocks before Termination, while
still rejecting blocks after Termination except final Padding, duplicate
Padding, malformed lengths, and excessive unknown bytes. This is a local
wire-conformance correction; no mixed-router evidence or support advertisement
is implied.

## Plan 042 runtime composition amendment

`i2pr-runtime` will own the authenticated `TransmitState` and `ReceiveState`,
exact encrypted-frame reads/writes, bounded frame/message queues, and sibling
reader/writer cancellation and joins. The launcher may report aggregate frame
and message counters, but it must not expose frame payloads, endpoints, or
identity material. Authentication, a complete bounded smoke exchange, and
successful cleanup are separate gates; TCP connection or listener readiness is
not a data-phase result.
