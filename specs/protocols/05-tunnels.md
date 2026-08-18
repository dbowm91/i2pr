# Tunnel construction and tunnel messages

Status: **required**  
Primary roadmap milestone: **5**  
Dependencies: common structures, I2NP, NetDB, cryptography, and eventually a working router transport

## Scope

I2P network tunnels are unidirectional paths used to carry I2NP messages.
This dossier covers tunnel IDs, short tunnel-build request/reply messages and
records, ECIES-X25519 build encryption, tunnel-message preprocessing,
fragmentation/reassembly, established tunnel layer encryption, participant
roles, exploratory-pool lifecycle, and tunnel testing.

Inbound and outbound tunnels are separate. A bidirectional application flow
therefore depends on multiple unidirectional tunnel pools; this must remain
explicit in the architecture.

## Authoritative sources

- [Tunnel creation with ECIES-X25519](https://geti2p.net/spec/tunnel-creation-ecies)
  for current short-build cryptography and derived layer keys.
- [Tunnel Message Specification](https://geti2p.net/spec/tunnel-message) for
  fixed TunnelData layout, delivery instructions, fragmentation, checksum, and
  packing rules.
- [Tunnel Implementation](https://geti2p.net/en/docs/tunnels/implementation)
  for gateway, participant, and endpoint layer-transform behavior.
- [I2NP specification](https://geti2p.net/spec/i2np) for TunnelData,
  TunnelGateway, short build messages, and other router messages.
- Proposals 152 and 157 for historical ECIES/short-build context.
- Proposal 153 only as an explicitly **open/deferred** ChaCha tunnel-layer
  proposal; it is not the established AES data plane implemented by Plan 116.
- Pinned deployed-reference source when specification prose needs implementation
  confirmation. Plan 115/116 use upstream Emissary revision
  `9b43484a21d5a1291c4881cdae62a36c527f8c0f` for this purpose.

The current ECIES tunnel-creation specification identifies the old long ECIES
build-record format as deprecated/obsolete and directs current ECIES routers to
the short-record format. Mixed legacy compatibility remains outside the
immediate Plan 116 data-plane implementation unless a later live checkpoint
shows it is required.

## Current project implementation floor

Plans 111-114 established locally strict short-build construction and reply
processing. Plan 115 added the canonical production I2NP type-25 bridge and
then obtained independent native-consumer Q0 evidence against pinned upstream
Emissary.

Current state:

```text
short-build outbound/inbound local state = strict-established
independent short-build Q0                = passed-emissary-native-consumer
Q1 authenticated router transport        = deferred
Q2 live reply to i2pr Established        = deferred
TunnelData local data plane               = passed-local-tunnel-data-plane
live mixed-router exploratory             = Plan 117 target
```

Plan-of-record:
[`plans/116-local-tunnel-data-plane.md`](../../plans/116-local-tunnel-data-plane.md) +
[`plans/116-completion-correction.md`](../../plans/116-completion-correction.md) +
[`plans/116-status.md`](../../plans/116-status.md)

## Required MVP roles

Keep explicit role-specific state:

- **creator/builder** — constructs encrypted per-hop records and processes
  replies;
- **local outbound gateway** — preprocesses I2NP messages and applies inverse
  layer transforms for the complete outbound path;
- **inbound gateway** — accepts TunnelGateway messages and injects LOCAL
  fragments into an inbound tunnel using only its own hop keys/routing state;
- **participant** — applies one tunnel layer transform, performs previous-peer
  and duplicate checks, rewrites the next tunnel ID, and forwards;
- **outbound endpoint** — applies the final participant transform, exposes the
  preprocessed payload, reassembles fragments, and follows Tunnel Message
  delivery instructions;
- **local inbound endpoint** — iteratively removes the creator-known inbound
  layers, verifies/reassembles LOCAL delivery, and returns the I2NP message to
  the local router boundary.

Role separation is important for auditing secrets and resource ownership. A
participant must not gain creator-only path knowledge through shared state.

## Required construction behavior

- Use current short tunnel-build messages/records for ECIES routers.
- Generate independent ephemeral X25519 material per hop as specified.
- Reproduce exact Noise-N transcript, HKDF, ChaCha20/Poly1305, reply encryption,
  and record ordering.
- Populate and validate receive tunnel IDs, next-hop router/tunnel IDs, flags,
  request time, expiration, and current build options.
- Derive `replyKey`, `layerKey`, and `ivKey` from the post-request short-build
  state exactly once through the existing KDF implementation.
- Preserve independent receive/next/creator tunnel identifiers; do not derive
  IDs from router hashes or alias creator-local and remote receive IDs.
- Support reject response codes without exposing excessive local policy detail.
- Randomize record positions and prevent a hop from learning path position
  beyond protocol leakage.
- Bound pending builds globally/per pool and clean up key material on timeout,
  rejection, malformed reply, cancellation, or disconnect.

## Established TunnelData wire format

The encrypted TunnelData body is fixed at 1028 bytes:

```text
Tunnel ID       4 bytes, nonzero
IV             16 bytes
Encrypted Data 1008 bytes
```

The decrypted 1008-byte data region is:

```text
checksum[4]
nonzero random padding[0..]
zero delimiter 0x00
one or more delivery-instruction + I2NP-fragment records
```

Checksum is the first four bytes of:

```text
SHA256(bytes after the zero delimiter || IV)
```

It does not cover padding or the zero delimiter.

The existing `i2pr-proto::TunnelDataMessage` and
`i2pr-proto::TunnelGatewayMessage` own the I2NP body framing and must be reused;
`i2pr-tunnel` owns preprocessing, fragment state, and layer transforms.

## Tunnel Message delivery instructions

Tunnel Message first-fragment delivery modes are exactly:

```text
LOCAL  = 0
TUNNEL = 1
ROUTER = 2
3      = invalid
```

There is **no destination delivery mode in the Tunnel Message format**.
Destination delivery belongs to garlic/client-layer behavior and is downstream
of Milestone 5.

For the MVP exploratory data plane:

- `LOCAL` is required for inbound tunnels;
- `ROUTER` is required for outbound router delivery;
- `TUNNEL` is required for outbound delivery into another router's inbound
  tunnel through TunnelGateway.

Delay and extended-option bits are unimplemented in the current protocol and
must be rejected if set. Reserved bits must be zero.

## Fragmentation and reassembly

First fragments carry delivery instructions and, when fragmented, a 4-byte
message ID. Follow-on fragments use:

```text
bit 7    = 1
bits 6-1 = sequence number 1..63
bit 0    = last fragment
message ID[4]
size u16
fragment bytes
```

The wire format therefore permits at most 64 fragments total. The official
specification derives an approximate maximum complete I2NP message size of
62,708 bytes. `i2pr` may enforce a lower resource ceiling but must never accept
a larger fragment sequence than the wire format permits.

Reassembly must be explicitly bounded by:

- concurrent partial messages;
- bytes per partial message;
- aggregate retained bytes;
- fragment count;
- caller-supplied age/expiry;
- tunnel/endpoint context so identical message IDs on different tunnels cannot
  merge.

Out-of-order valid fragments must be supported. Exact duplicates may be
idempotent; conflicting duplicate fragments must invalidate/drop the affected
partial message without unbounded work.

## Established AES tunnel layer transform

Plan 116 implements the deployed AES layer, not Proposal 153 ChaCha.

Participant forward transform:

```text
workingIV = AES256-ECB-ENCRYPT(ivKey, receivedIV)
newData   = AES256-CBC-ENCRYPT(layerKey, workingIV, receivedData[1008])
nextIV    = AES256-ECB-ENCRYPT(ivKey, workingIV)
```

The participant forwards:

```text
nextTunnelId || nextIV || newData
```

The outbound creator applies the inverse transform for all hops in reverse path
order before sending to the first hop. The local inbound endpoint applies the
same inverse sequence over its creator-known hop keys in reverse path order.

The per-hop `layerKey` / `ivKey` values already exist in
`i2pr-tunnel::LayerKeys`; no parallel key schedule is permitted.

## Duplicate/replay suppression

The established tunnel implementation defines the duplicate token as:

```text
receivedIV XOR first_16_bytes(receivedEncryptedData)
```

Participants must reject duplicate tokens within a bounded tunnel-lifetime
window and lock the previous-hop router identity after the first accepted
message. Plan 116 may use a bounded exact set in its runtime-neutral role model;
a scalable process-wide probabilistic filter may be composed later if needed.

Never silently evict unexpired replay tokens merely to admit new ones.

## Tunnel pools and lifecycle

The first pool implementation supports exploratory inbound/outbound tunnels and
later destination-specific pools. It must define:

- target quantity and path length policy;
- real success registration with established key material;
- build-ahead/replacement and expiration;
- success/failure and peer-avoidance inputs;
- graceful removal and immediate failed-state cleanup;
- selection among usable tunnels;
- bounded concurrent builds during startup/degradation.

A current Plan 116 prerequisite is removal of the placeholder
`ShortBuildRegistrar` behavior that reports slot-0 insertion without storing
usable keys. No pool entry may be considered `Established` for production use
unless its data-plane material exists.

Tunnel length and peer selection are anonymity policy, not wire format. Keep
them outside codecs and cryptographic records.

## Transit participation

Full public transit participation remains a later milestone. Plan 116 may
implement a minimal runtime-neutral participant role solely to prove the local
data-plane trajectory and to establish reusable role primitives.

The later public transit mode must separately consider bandwidth/queue capacity,
active tunnel limits, abuse controls, supported build format, expiration,
operational mode, and shutdown/degraded state.

A rejection should be protocol-correct and inexpensive. Never reserve large
buffers or spawn long-lived tasks before admission succeeds.

## Required Plan 116 tests

- successful short build transfers real established key material into the pool;
- removal/failure/expiry makes secret material unusable and drops it;
- checksum/padding/delivery-instruction positive and negative vectors;
- LOCAL/ROUTER/TUNNEL first-fragment encoding/decoding;
- follow-on sequence bounds and 64-fragment ceiling;
- lost/reordered/duplicate/conflicting fragment behavior;
- reassembly memory pressure under explicit limits;
- participant AES transform and inverse fixed vectors;
- multi-hop outbound inverse -> participant-forward round trip;
- multi-hop inbound participant-forward -> creator-inverse round trip;
- previous-peer lock and duplicate-token rejection;
- deterministic outbound OBGW -> participant -> OBEP ROUTER delivery;
- deterministic outbound -> OBEP TUNNEL -> IBGW -> inbound participant -> local
  inbound endpoint round trip;
- tunnel expiration/removal under caller-supplied virtual time;
- fuzz targets or corpus additions for delivery-instruction/fragment parsing
  where practical.

## Deferred and compatibility behavior

- Q1/Q2 authenticated transport evidence: Plan 117/later integration.
- Deprecated long ECIES records: compatibility-only if later live testing
  requires them.
- ElGamal router generation: excluded.
- Proposal 153 ChaCha established layer encryption: deferred/open proposal.
- Advanced batching/mixing, cover traffic, adaptive path-length experiments:
  post-correctness optimization.
- Destination/garlic/LeaseSet semantics: Milestone 6.
- Public-network transit participation: later resource/privacy review.

## Open decisions after Plan 116

1. Exact production reassembly/replay-window defaults after real traffic
   measurements.
2. Whether a process-wide decaying Bloom filter replaces/backs the bounded exact
   duplicate window used by the local role implementation.
3. Pool quantity/length defaults for low-resource and balanced profiles.
4. Tunnel-test cadence/failure thresholds required before live promotion.
5. Whether MVP mixed-router compatibility needs legacy ElGamal tunnel-build
   records.
6. How runtime bandwidth accounting integrates with the router-wide resource
   governor without introducing timing-dependent deadlocks.
