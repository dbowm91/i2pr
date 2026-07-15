# ADR 0012: bounded NTCP2 handshake state machines

- Status: Accepted for the experimental Plan 033 handshake surface
- Date: 2026-07-15
- Scope: `i2pr-transport-ntcp2` handshake codecs, policy seams, and pure states

## Decision

Keep NTCP2 handshake transitions in consuming, runtime-neutral states. A state
accepts one typed input and returns the next state plus bounded actions. The
runtime adapter will own partial reads/writes, deadlines, cancellation, wall
clock access, padding randomness, replay storage, and local RouterInfo
retrieval. No async trait, socket, Tokio type, filesystem access, or NetDB
mutation crosses into the protocol crate.

SessionRequest and SessionCreated are split as 32-byte AES-obfuscated
ephemeral bytes, a fixed 32-byte ChaChaPoly options frame, and bounded
cleartext padding. SessionConfirmed is the fixed 48-byte encrypted static-key
frame followed by the negotiated encrypted part-two frame. Part two accepts
only RouterInfo, optional Options, then optional Padding blocks. Fixed regions
require exact lengths; variable cleartext padding is consumed only after its
authenticated length is known, and the part-two block parser rejects unknown
or trailing bytes.

The initial local policy uses the pinned 0.9.69 non-PQ padding maxima of 880
bytes for SessionRequest and 848 bytes for SessionCreated, a ±60-second clock
skew window, and replay retention of at least 2× that window. Replay tokens are
SHA-256 digests of the encrypted ephemeral field and cache admission is
fail-closed for replay, full, or unavailable decisions. The exact production
padding distribution remains deliberately deferred because the specification
still leaves the distribution and negotiation open.

RouterInfo is decoded with the existing bounded structural codec, its exact
signed region is verified by `i2pr-crypto`, and its NTCP/NTCP2 version-2 `s`
option must match the authenticated X25519 static key. The result contains
only the typed role, RouterIdentity hash, negotiated bounds, and consuming
data-phase key owners. It does not update NetDB or publish an address.

## Consequences and review triggers

The action boundary is deterministic and testable with synthetic inputs, but
it requires a later runtime owner to implement exact stream adaptation. Local
vectors and state-machine tests are structural/experimental evidence only.
Revisit this decision before adding runtime I/O, a production padding
distribution, mixed-router interoperability, data-phase frames, or capability
advertisement.
