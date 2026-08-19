# Plans 118-123 roadmap — resume router construction at Milestone 6

## Status

- **Ready for staged execution**.
- Date: **2026-08-19**.
- Source implementation floor: Plan 117 local production composition at
  `99374cf498227cf8ab1c4ec6ec4216b5d4d2e08e`.
- Entry gate: execute Plan 118 first.
- Product target: the first local destination capable of publishing a modern
  LeaseSet2, sending and receiving encrypted Garlic messages over dedicated
  destination tunnels, resolving remote LeaseSet2 records, and carrying a
  minimal reliable streaming byte stream.
- External mixed-router acceptance remains a separate evidence track.

## Why this roadmap exists

The repository has enough router substrate to stop extending the Milestone 5
validation campaign and move into the next functional router layer.

Retained substrate includes:

```text
strict protocol/common-structure codecs
Ed25519 / X25519 / ChaChaPoly / HKDF wrappers
persistent router identity
bounded NetDB store and lookup/publication state machines
reseed ingestion
short tunnel construction
real EstablishedMaterial ownership
exploratory tunnel pools
TunnelData gateway / participant / endpoint processing
bounded fragmentation / reassembly
production exploratory NetDB composition
```

What does **not** exist yet is the destination/client core. The workspace still
has no `i2pr-client` crate, Garlic message semantics remain deferred/opaque, and
the common-structure layer only implements classic LeaseSet while explicitly
deferring LeaseSet2-family semantics.

That is now the highest-value implementation frontier.

---

# 1. Normative protocol direction

Milestone 6 should target the modern deployed destination stack rather than
making legacy ElGamal LeaseSet1 the architectural center.

Primary normative references:

```text
I2P Common Structures Specification
https://i2p.net/en/docs/specs/common-structures/

I2P I2NP Specification
https://i2p.net/en/docs/specs/i2np/

ECIES-X25519-AEAD-Ratchet
https://i2p.net/en/docs/specs/ecies/

Streaming Protocol Specification
https://i2p.net/en/docs/specs/streaming/
```

Current relevant facts from the official specifications:

- Standard LeaseSet2 is a DatabaseStore type-3 structure.
- LeaseSet2 uses `Lease2`, a four-byte seconds-since-epoch expiration field.
- LeaseSet2 may carry one or more typed encryption public keys.
- X25519 destination end-to-end encryption is crypto type 4 with a 32-byte
  public key.
- The LeaseSet2 signature domain prepends the DatabaseStore type byte (`3`) to
  the LeaseSet2 bytes being signed.
- ECIES-X25519-AEAD-Ratchet is the deployed replacement for
  ElGamal/AES+SessionTags for destination-to-destination encryption.
- ECIES sessions are destination-context isolated; session state must not be
  shared between local destinations or between router and destination context.
- Streaming is layered above the I2P message layer and requires a signed SYN,
  sequence/ack state, retransmission, close/reset handling, and replay
  prevention.

The implementation should support the minimum normative subset necessary for
ordinary published Standard LeaseSet2 + X25519 destination communication.
EncryptedLeaseSet, MetaLeaseSet, PQ-hybrid ECIES, offline signing, datagrams,
and advanced streaming optimization are deliberately deferred unless a later
plan explicitly pulls them in.

---

# 2. Execution sequence

```text
Plan 118
  planning authority cleanup + Plan 117 disposition
        |
        v
Plan 119
  Lease2 / Standard LeaseSet2 / type-3 NetDB foundation
        |
        v
Plan 120
  i2pr-client destination lifecycle + dedicated tunnel pools
        |
        v
Plan 121
  ECIES-X25519 Garlic encryption + session state
        |
        v
Plan 122
  destination routing + LeaseSet2 NetDB composition
        |
        v
Plan 123
  minimal streaming core
```

Each plan is intended to close independently with deterministic/local evidence.
No plan may require the unavailable authenticated external transport lane simply
to prove its internal product invariants.

---

# 3. Plan 119 — LeaseSet2 protocol foundation

File:

```text
plans/119-m6-leaseset2-protocol-foundation.md
```

Primary outputs:

```text
Lease2 codec
LeaseSet2Header codec
Standard LeaseSet2 codec
multiple typed encryption keys
crypto type 4/X25519 policy
canonical signature preimage including DatabaseStore type 3
strict validation and bounds
DatabaseStore type-3 ownership in i2pr-proto / i2pr-netdb
local LeaseSet2 verification/store path
```

This plan is intentionally protocol/storage focused. It does not create a live
local destination or implement Garlic encryption.

Exit gate:

```text
known-good Standard LeaseSet2 fixtures round-trip
invalid signatures / ordering / lengths / flags fail closed
validated type-3 LeaseSet2 can be stored and retrieved by destination hash
classic LeaseSet behavior is not silently changed
```

---

# 4. Plan 120 — destination lifecycle and dedicated tunnel pools

File:

```text
plans/120-m6-destination-lifecycle-and-tunnel-pools.md
```

Primary outputs:

```text
new i2pr-client crate
destination identity/key ownership
destination-context isolation
bounded destination registry
inbound/outbound destination tunnel pools
reuse of established i2pr-tunnel construction/data-plane primitives
rotation / expiry / replacement
Lease2 derivation from usable inbound tunnels
signed local Standard LeaseSet2 generation
publication-ready local destination state
```

No Garlic encryption is required yet. The plan should end with a destination
that owns keys, owns dedicated tunnel capacity, and can construct its signed
LeaseSet2 from real established inbound tunnel metadata.

Exit gate:

```text
destination startup -> usable tunnel set -> signed LS2
expiry/failure -> replacement -> new LS2
shutdown -> secrets/sessionless state/tunnels released
one local destination cannot access another destination's secrets or tunnel pool
```

---

# 5. Plan 121 — ECIES-X25519 Garlic/session layer

File:

```text
plans/121-m6-ecies-garlic-session-layer.md
```

Primary outputs:

```text
ECIES destination cryptographic primitives/adapters
Elligator2 dependency decision and wrapper
New Session message support for bound ordinary destination traffic
New Session Reply processing
Existing Session tags / ratchet state sufficient for sequential messaging
ECIES payload block codec
Garlic Clove block support needed by destination delivery
bounded replay/session/tag state
strict destination-context isolation
zeroization / non-Debug secret state
```

Do not hand-roll Elligator2 or other curve primitives. The implementation phase
must select a maintained/auditable Rust primitive or stop at a documented
cryptographic dependency gate rather than reimplement the map locally.

Advanced ECIES features that the upstream specification itself still marks
unimplemented or unnecessary for the MVP remain out of scope.

Exit gate:

```text
Alice destination -> New Session Garlic -> Bob decrypt
Bob -> New Session Reply -> Alice accepts
Existing Session messages in both directions decrypt exactly once
replay / tag reuse / wrong-destination context fail closed
```

---

# 6. Plan 122 — destination routing and NetDB composition

File:

```text
plans/122-m6-destination-routing-and-netdb-composition.md
```

Primary outputs:

```text
local LeaseSet2 publication through existing NetDB publication machinery
remote LeaseSet2 lookup through existing NetDB lookup machinery
route selection from remote leases
destination message composition into Garlic
outbound destination tunnel delivery
inbound destination tunnel recovery
destination demultiplexing by local destination
ECIES decrypt -> Garlic clove -> local payload delivery
bounded pending-send state while LS2 lookup is active
```

This is the first Milestone 6 full-product trajectory.

Required deterministic trajectory:

```text
Destination A
 -> resolve Destination B LeaseSet2
 -> choose one valid B lease
 -> ECIES/Garlic encode
 -> A outbound destination tunnel
 -> deterministic routing seam
 -> B inbound destination tunnel
 -> B local endpoint
 -> ECIES/Garlic decode
 -> exact application payload at Destination B
```

Then run the reverse direction so the session/reply state is exercised.

Exit gate:

```text
two local i2pr destinations exchange exact payloads over dedicated tunnel paths
remote LS2 expiry and refresh are enforced
no direct transport shortcut replaces the tunnel path
```

---

# 7. Plan 123 — minimal streaming core

File:

```text
plans/123-m6-minimal-streaming-core.md
```

Primary outputs:

```text
stream packet codec
signed SYN/SYN-ACK
stream IDs
sequence and acknowledgement state
NACK / fast-retransmit minimum behavior
retransmission timer and bounded retry state
receive reordering window
flow/congestion window minimum viable behavior
CLOSE and RESET
current SYN replay-prevention destination-hash field
listener/connect internal API
clean shutdown/cancellation
```

The plan must build on Plan 122 destination messaging and must not bypass it.

Exit gate:

```text
A opens stream to B
bidirectional byte stream works
loss / duplication / reordering deterministic tests recover
replay and invalid signature tests fail closed
close/reset release all stream state
```

SAM and I2CP remain downstream.

---

# 8. External acceptance debt — deliberately parallel

The following items remain valuable and must remain visible, but they do not
block Plans 119-123 when the relevant local product invariant can be established
without them:

| Evidence item | Current state | Blocks M6 construction? |
| --- | --- | --- |
| NTCP2 authenticated Q1 | host-lane unavailable | No |
| external ShortBuild Q2 return | host-lane unavailable | No |
| Plan 117 native mixed-router NetDB | Plan 118 disposition required | Only through Plan 118 |
| live exploratory tunnels | deferred | No |
| live RouterInfo NetDB publication/lookup | deferred | No |
| live LS2 publication/lookup | future | No during local implementation |
| independent destination ECIES interop | future M6 acceptance | No during foundational slices |
| independent streaming interop | future M6 acceptance | No during foundational slices |

When an appropriate host lane becomes available, these checkpoints should be
run against the already-built product layers. They should not trigger a second
implementation architecture.

---

# 9. Crate-boundary target

The original MVP roadmap already anticipates `i2pr-client`. Plan 120 should add
it rather than putting destination state into the daemon or tunnel crate.

Target ownership:

```text
i2pr-proto
  Lease2 / LeaseSet2 / Garlic and streaming wire structures

i2pr-crypto
  low-level reusable ECIES/X25519/ChaChaPoly/HKDF/Elligator wrappers
  no destination lifecycle or routing policy

i2pr-netdb
  validated LeaseSet2 storage / lookup / publication contracts

i2pr-tunnel
  transport-neutral tunnel construction and data-plane mechanics
  no destination application/session policy

i2pr-client
  destination lifecycle
  per-destination key/session contexts
  destination tunnel pools
  ECIES session manager
  LeaseSet2 lifecycle
  destination routing
  minimal streaming

i2pr-daemon
  composition root only
```

Forbidden dependency direction:

```text
i2pr-proto -> client/tunnel/netdb/daemon
i2pr-crypto -> client/daemon
i2pr-tunnel -> client/daemon
i2pr-netdb -> client/daemon
i2pr-client -> daemon
```

The dependency-direction script should be updated only if the new intended
`client -> proto/crypto/netdb/tunnel/core` edges are not already represented.

---

# 10. Security and anonymity requirements across Milestone 6

Every plan must preserve the following cross-cutting invariants.

### Destination isolation

A local destination is an anonymity boundary. Never share:

```text
static destination X25519 private keys
ECIES session/tag state
destination-specific tunnel-pool identity
stream connection state
application delivery queues
```

between destinations unless a future explicit shared-destination design says
otherwise.

### Bounded state

All of the following require explicit count/byte/time limits:

```text
local destinations
remote LeaseSet2 cache ownership
pending LeaseSet2 lookups
ECIES inbound/outbound sessions
ratchet/tag look-ahead
Garlic cloves / block sizes
pending destination messages
streams per destination
reordering windows
unacked packets
retransmission attempts
listener backlog
```

### Secret handling

Secret-bearing types must not derive or implement byte-revealing `Debug`.
Avoid unnecessary cloning. Zeroize private/session material where supported.
Do not persist ephemeral ECIES ratchet/session state unless a later design
explicitly requires it.

### Timing / correlation

Do not introduce direct-send fallback from a destination to a remote router when
no usable destination tunnel exists. Failure to obtain a tunnel or LeaseSet
must remain a typed routing failure, not silently degrade anonymity boundaries.

### Strict protocol behavior

Unknown or unsupported LS2 variants, ECIES blocks, streaming flags/options, and
future extensions must be rejected or retained only where the specification
explicitly permits forward-compatible opaque handling. Never reinterpret an
unknown type as a supported default.

---

# 11. Definition of Milestone 6 local construction completion

Plans 119-123 are locally complete when this deterministic product path passes:

```text
create Destination A + Destination B
 -> establish dedicated inbound/outbound tunnel pools
 -> construct and validate signed Standard LeaseSet2 for A and B
 -> publish/store/resolve LS2 through production NetDB interfaces
 -> establish ECIES destination session
 -> exchange Garlic-protected destination messages through tunnel data planes
 -> open signed streaming connection
 -> transfer bidirectional bytes with injected loss/reordering
 -> close stream
 -> expire/cancel destinations and tunnels
 -> release all bounded state and secrets
```

This label is intentionally:

```text
milestone6_local_product = passed
```

It is **not**:

```text
milestone6_interoperable = passed
```

The latter requires later independent-router evidence when an executable lane is
available.

---

# 12. Anti-loop rules

1. Execute Plan 118 once; do not create another Plan 117 validation campaign.
2. Do not reopen Plan 116 without a new affirmative defect localized to its
   implementation.
3. Do not enable normal-daemon NTCP2 to make Milestone 6 tests look more real.
4. Prefer deterministic Rust trajectories at real production seams over Python
   orchestration.
5. Do not implement legacy ElGamal destination encryption first merely because
   classic LeaseSet already exists.
6. Standard LeaseSet2 + X25519 is the ordinary M6 baseline; encrypted/meta/PQ
   variants remain future work.
7. Do not implement SAM or I2CP until destination routing and minimal streaming
   are usable internally.
8. Do not implement HTTP/SOCKS/service tunnels before streaming.
9. A reference implementation failure must be localized and classified; it is
   not automatically an i2pr defect.
10. Keep implementation momentum: every plan after 118 must add product Rust
    capability, not only evidence infrastructure.
