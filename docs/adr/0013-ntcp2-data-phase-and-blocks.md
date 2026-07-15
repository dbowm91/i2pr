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
   limits. Padding is last; termination cannot share a frame with application
   or control blocks.
5. Outbound I2NP blocks consume the existing bounded
   `EncodedI2npMessage`. Inbound I2NP views borrow the authenticated plaintext
   owner and only copy at an explicit transport handoff. RouterInfo is decoded,
   signature-verified, and returned as a candidate; no NetDB or publication
   side effect occurs here.
6. Padding/coalescing is represented as a pure bounded policy input. This
   layer never waits for more messages, selects queues, or generates runtime
   randomness. Deterministic tests use fixed zero padding; a later runtime
   adapter supplies compliant random padding and scheduling decisions.
7. Because the current specification supplies no data-phase rekey threshold,
   this implementation does not invent one. The last permitted nonce is
   usable, the forbidden `2^64 - 1` value is never emitted, and exhaustion or
   static-key/IV rotation requires a fresh Noise handshake.

## Consequences

The protocol crate can prove framing, block bounds, ownership, terminal-state,
and malformed-input behavior without sockets or a runtime. The future runtime
must retain partial length/ciphertext buffers under explicit resource leases,
flush complete encoded frames, apply deadlines/cancellation, and perform
NetDB/policy actions only after typed data-phase output. Local vectors and fuzz
results remain experimental evidence and do not establish interoperability.

## Evidence

- `crates/i2pr-transport-ntcp2/src/block.rs`
- `crates/i2pr-transport-ntcp2/src/frame.rs`
- `crates/i2pr-transport-ntcp2/src/crypto.rs`
- `crates/i2pr-testkit/src/ntcp2.rs`
- `tests/fixtures/ntcp2/crypto/manifest.tsv`
- `plans/034-closure.md`
