# Plan 116: local tunnel data plane and exploratory-tunnel closure

## Status

- **Ready for implementation**.
- Date: 2026-08-18.
- Milestone: **5 — Network tunnel data plane and exploratory tunnels**.
- Predecessor: Plan 115 Emissary Q0, status
  `passed-emissary-q0-construction-and-obep-reply-only`.
- Planning implementation floor: `952f644e90b18e4af0ed92df54ef87f5c8b1bb54`.
- Successor after this plan passes: **Plan 117 — live exploratory delivery + NetDB integration**.

This is a product-construction plan. It is **not** another interoperability or
transport-validation plan.

Plan 115 answered the question that mattered before building on the short-build
format: pinned upstream Emissary independently consumed the exact production
`ShortBuildStateMachine -> ShortBuildI2npBridge` type-25 message and reached its
native OBEP reply path. Q1 authenticated transport and Q2 live reply return are
still deferred, but they are no longer prerequisites for implementing the
transport-neutral tunnel data plane.

The purpose of Plan 116 is to turn the existing tunnel-build control plane into
a real local data plane that can move bounded I2NP messages through established
outbound and inbound exploratory tunnels under deterministic simulation.

---

## 1. Hard anti-loop rule

During Plan 116, do **not** perform or require:

- another Emissary/i2pd/Java short-build validation pass;
- NTCP2 correction or activation;
- SSU2 work;
- rootless namespace, privileged namespace, Docker, Multipass, or VM work;
- Python interoperability orchestration;
- public-I2P participation;
- live mixed-router tunnel execution;
- Q1 or Q2 closure;
- a generic I2NP router dispatcher;
- Milestone 6 garlic/LeaseSet/streaming implementation.

A live transport limitation may remain recorded, but it must not block this
transport-neutral implementation.

The only reason to stop Plan 116 for protocol research is a **specific wire or
cryptographic ambiguity discovered while implementing TunnelData** that cannot
be resolved from the current official specification plus pinned deployed
reference implementations. If that happens, answer that exact question and
continue this plan; do not create another broad validation program.

---

## 2. Normative and deployed-reference basis

### 2.1 Primary protocol authority

Use these as the normative wire sources:

- Tunnel Message Specification:
  <https://geti2p.net/spec/tunnel-message>
- Tunnel Implementation:
  <https://geti2p.net/en/docs/tunnels/implementation>
- ECIES-X25519 Tunnel Creation:
  <https://geti2p.net/spec/tunnel-creation-ecies>
- I2NP Specification:
  <https://geti2p.net/spec/i2np>
- Repository dossier:
  [`specs/protocols/05-tunnels.md`](../specs/protocols/05-tunnels.md)

The established tunnel data plane remains AES-based. Proposal 153
(ChaCha tunnel layer encryption) is still an open proposal and is **out of
scope**:

- <https://geti2p.net/spec/proposals/153-chacha20-layer-encryption>

Do not infer from the ECIES/ChaCha short-build record format that established
TunnelData uses ChaCha20. The short-build KDF already derives the AES
`layerKey` and `ivKey` used by the established tunnel.

### 2.2 Pinned deployed reference

Use the same independent implementation already qualified by Plan 115 for
source comparison only:

```text
reference_repository = https://github.com/eepnet/emissary.git
reference_revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
reference_package    = emissary-core 0.4.0
```

Useful files at that revision:

```text
emissary-core/src/i2np/tunnel/data.rs
emissary-core/src/tunnel/noise.rs
emissary-core/src/tunnel/hop/outbound.rs
emissary-core/src/tunnel/hop/inbound.rs
emissary-core/src/tunnel/transit/participant.rs
emissary-core/src/tunnel/transit/inbound.rs
emissary-core/src/tunnel/transit/outbound.rs
```

These are implementation references, not a runtime dependency and not an
acceptance gate.

---

## 3. Protocol facts Plan 116 must preserve

### 3.1 TunnelData wire body

The I2NP TunnelData body is exactly:

```text
Tunnel ID       4 bytes, nonzero
IV             16 bytes
Encrypted Data 1008 bytes
------------------------
total          1028 bytes
```

The existing `i2pr-proto::TunnelDataMessage` models the portion after the
I2NP body-level tunnel ID as a fixed 1024-byte `data` field:

```text
data[0..16]    = IV
data[16..1024] = encrypted 1008-byte tunnel payload
```

Do not introduce a second incompatible TunnelData I2NP codec in
`i2pr-tunnel`.

### 3.2 Decrypted 1008-byte payload

Before layer encryption the 1008-byte data region is:

```text
checksum[4]
nonzero random padding[0..]
zero delimiter[1] = 0x00
one or more delivery-instruction + fragment records
```

Checksum:

```text
SHA256(bytes after zero delimiter || IV)[0..4]
```

The checksum does **not** cover the padding or zero delimiter.

Padding bytes before the delimiter must be nonzero so endpoint parsing can find
the delimiter unambiguously.

### 3.3 First-fragment delivery-instruction control byte

```text
bit 7    = 0
bits 6-5 = delivery type
           00 LOCAL
           01 TUNNEL
           10 ROUTER
           11 invalid
bit 4    = delay-present; unsupported here, require 0
bit 3    = fragmented; 0 complete, 1 first fragment
bit 2    = extended options; unsupported here, require 0
bits 1-0 = reserved; require 0
```

Extra fields:

```text
LOCAL              = no destination field
ROUTER             = router hash[32]
TUNNEL             = tunnel id[4] + gateway router hash[32]
first fragmented   = message id[4]
all first records  = fragment length u16 + fragment bytes
```

Plan 116 minimum delivery modes:

- inbound gateway -> local inbound endpoint: `LOCAL`;
- outbound endpoint -> another router: `ROUTER`;
- outbound endpoint -> an inbound tunnel gateway: `TUNNEL`.

Destination delivery is not a Tunnel Message delivery mode and must not be
invented here. Destination/garlic behavior belongs to Milestone 6.

### 3.4 Follow-on fragment control byte

```text
bit 7    = 1
bits 6-1 = sequence number 1..63
bit 0    = last-fragment flag
message id[4]
fragment length u16
fragment bytes
```

Hard wire ceiling: 64 fragments total. The official specification derives an
approximate maximum complete I2NP message size of 62,708 bytes under the most
expensive first-fragment delivery mode. `i2pr` may enforce an equal or lower
resource-policy ceiling, but never a higher wire-format ceiling.

### 3.5 Participant layer transform

For a received `(IV, data[1008])` and a hop's AES-256 `ivKey` / `layerKey`:

```text
working_iv = AES256-ECB-ENCRYPT(ivKey, received_iv)
new_data   = AES256-CBC-ENCRYPT(layerKey, working_iv, received_data)
next_iv    = AES256-ECB-ENCRYPT(ivKey, working_iv)
```

The participant forwards:

```text
next_tunnel_id || next_iv || new_data
```

Outbound creator preprocessing applies the inverse operation hop-by-hop in
**reverse path order**, so each remote participant's forward transform exposes
the original preprocessed payload at the OBEP.

The local inbound endpoint likewise applies the inverse transforms in reverse
path order after all remote inbound hops have applied their forward transforms.

### 3.6 Duplicate/tagging detection input

The implementation documentation defines the duplicate token as:

```text
received_iv XOR first_16_bytes(received_encrypted_data)
```

Plan 116 must implement this exact fingerprint function and a bounded replay
window for transit-role simulation. A bounded exact set is acceptable for this
local phase; if it reaches capacity, fail/drop rather than silently evict an
unexpired token and re-enable replay. Do not add a Bloom-filter dependency just
to imitate one reference implementation before runtime bandwidth policy exists.

---

## 4. Existing repository surfaces to reuse

### 4.1 `i2pr-proto`

Already present:

```text
I2npBody::TunnelData
TunnelDataMessage
I2npBody::TunnelGateway
TunnelGatewayMessage
standard and short-transport I2NP framing
```

`TunnelDataMessage` already enforces a nonzero tunnel ID at encode/decode and a
fixed 1024-byte IV+ciphertext field. `TunnelGatewayMessage` already owns a
nested **standard-header** I2NP message.

Plan 116 may add bounded tunnel-fragment structural types to `i2pr-tunnel`, but
must not fork these I2NP body codecs.

### 4.2 `i2pr-tunnel::LayerKeys`

Already present from Plans 109-114:

```text
reply_key
layer_key
iv_key
optional OBEP garlic key/tag
```

`derive_layer_keys()` already performs the deployed short-build KDF chain,
including `SMTunnelLayerKey` and the OBEP `TunnelLayerIVKey` continuation.
Plan 116 must consume the existing `layer_key()` and `iv_key()` values rather
than derive a parallel key schedule.

### 4.3 `i2pr-transport`

Already present:

```text
EncodedI2npMessage
DeliveryRequest
DeliveryOutcome
```

Plan 116 must keep `i2pr-tunnel` runtime-neutral and should **not** add a direct
production dependency on `i2pr-transport`. The tunnel crate should emit a small
router-delivery action containing the next router and semantic I2NP message
metadata/body; a later runtime/composition adapter can convert that action into
the existing transport boundary and select the proper transport header.

### 4.4 Current blocking defect: registrar is not real

`crates/i2pr-tunnel/src/short_state.rs::ShortBuildRegistrar::admit()` is still a
placeholder. On `ShortBuildOutcome::Established`, it advances pool time and
returns:

```text
RegisterOutcome::Inserted {
    slot: TunnelSlot::from_raw(0),
    replaced: None,
}
```

without inserting the established tunnel or retaining its per-hop layer/IV
keys.

That must be corrected before TunnelData work is considered usable. Plan 116
therefore starts with established-state ownership rather than pretending the
pool already has live tunnel material.

---

# Work Package A — real established-tunnel ownership and registration

## 5. Goal

Convert a successful `ShortBuildStateMachine` into one real, secret-bearing,
usable tunnel entry with all data-plane material retained until expiration,
failure, or removal.

This is the mandatory first phase.

## 6. New established-material model

Create a focused module, preferably:

```text
crates/i2pr-tunnel/src/established.rs
```

Use names equivalent to:

```rust
EstablishedTunnel
EstablishedHop
HopForwarding
```

Exact names may vary, but the ownership semantics may not.

### 6.1 `EstablishedHop`

Must retain:

```text
peer router hash
hop role
receive tunnel ID
LayerKeys
forwarding metadata when the established role actually forwards TunnelData
```

Forwarding metadata should be role-aware instead of blindly copying the
build-reply terminal route:

```text
Participant / InboundGateway:
    next router + next tunnel id

OutboundEndpoint:
    endpoint marker only; data-plane destination comes from Tunnel Message
    delivery instructions, NOT from the OBEP build-reply route
```

This distinction is critical. Plan 114's terminal OBEP
`outbound_reply_router` and `next_tunnel` describe **build reply routing**. They
must not accidentally become a fixed TunnelData destination after the tunnel is
established.

### 6.2 `EstablishedTunnel`

Must retain at least:

```text
direction
creator/local logical tunnel identifier
ordered established hops
inbound externally-visible gateway router + gateway receive tunnel ID
inbound local receive tunnel ID where applicable
creation time supplied at registration
```

Do not collapse these IDs:

```text
creator_tunnel_id
first remote hop receive_tunnel
terminal inbound hop next_tunnel / local receive tunnel
```

Plans 111-114 intentionally made them independent.

Required helpers should make semantics explicit, for example:

```text
first_hop_router()
first_hop_receive_tunnel()
inbound_gateway()
local_receive_tunnel()
direction()
hops()
```

### 6.3 Secret ownership

`EstablishedTunnel` / `EstablishedHop` must:

- not derive or implement ordinary `Debug` containing key bytes;
- not expose key bytes in display/log output;
- avoid an ordinary long-lived `Clone` implementation for the established
  secret owner;
- zeroize secret-bearing material on drop;
- move `LayerKeys` out of the completed build state where practical rather than
  creating additional persistent copies.

`LayerKeys` itself currently supports `Clone` for build processing. Plan 116
does not need to redesign that historical build API, but the new established
owner must not encourage further duplication.

## 7. One-time transfer from the short-build state machine

Add an explicit success-only transfer, for example:

```text
ShortBuildStateMachine::take_established_material()
```

Required behavior:

- only succeeds after the state machine has reached `Established`;
- moves the retained per-hop contexts/keys into `EstablishedTunnel`;
- preserves exact path order, router hashes, roles, receive IDs, and data-plane
  forwarding IDs;
- fails before success;
- may be taken exactly once;
- after transfer, the build state no longer retains a second long-lived copy of
  the established keys;
- does not expose raw key material through the public terminal
  `ShortBuildOutcome` summary.

Do not stuff secret-bearing `LayerKeys` into a cloneable/debuggable public
`ShortBuildOutcome` merely to make registration convenient.

## 8. Replace fake pool admission

Refactor `ExploratoryPool` so an established pool entry cannot exist without
usable material.

Recommended internal shape:

```text
TunnelEntry {
    public metadata: TunnelRegistration,
    secret material: EstablishedTunnel,
}
```

Store `TunnelEntry` directly in the inbound/outbound maps. Continue exposing
`TunnelRegistration` snapshots for nonsecret inspection and reply-path
selection.

Avoid parallel maps for metadata and keys unless there is a compelling reason;
parallel ownership creates desynchronization and key-lifetime bugs.

### 8.1 Production registration

Replace the placeholder registrar path with real admission:

```text
successful short build
 -> take established material once
 -> ShortBuildRegistrar
 -> ExploratoryPool real insertion
```

After this phase:

- pool length actually changes;
- the returned `TunnelSlot` is the real inserted slot;
- duplicate/full rejection does not leak or retain orphaned secrets;
- failure/removal/expiry drops the secret owner;
- inbound `ReplyPath` selection uses the **remote inbound gateway's receive
  tunnel ID**, not a creator-local slot identifier.

Any old public `register_inbound` / `register_outbound` API that can manufacture
an `Established` entry from hashes and a tunnel ID but no keys must be removed,
restricted to tests, or replaced. There must be no production path that creates
an unusable established entry.

## 9. Work Package A acceptance

Required tests:

1. successful outbound build -> one real outbound established pool entry;
2. successful inbound build -> one real inbound established pool entry;
3. pool entry preserves exact path hop order and receive IDs;
4. inbound reply path returns first-hop router + first-hop receive tunnel ID;
5. local inbound receive tunnel remains distinct from creator logical ID;
6. `take_established_material()` before success fails;
7. second `take_established_material()` fails;
8. rejected/timed-out/cancelled/invalid build inserts nothing;
9. pool-full/duplicate rejection leaves no orphaned active entry;
10. removal/failure/expiry removes usable secret material;
11. debug output contains no layer, IV, reply, garlic key, or raw build bytes;
12. the existing strict Plan 114 outbound/inbound `Established` trajectories
    remain green.

Do not proceed to Work Package B until these pass.

---

# Work Package B — tunnel-message preprocessing, fragmentation, and AES layer crypto

## 10. Goal

Implement the pure, runtime-neutral TunnelData transformation layer in
`i2pr-tunnel`.

Suggested modules:

```text
crates/i2pr-tunnel/src/data.rs
crates/i2pr-tunnel/src/fragment.rs
crates/i2pr-tunnel/src/layer.rs
```

Keep files cohesive; combining two modules is acceptable if the resulting API is
clear. Do not create a new crate for this work.

## 11. Minimal dependency delta

The established tunnel data plane requires AES-256 ECB block operations and
AES-256 CBC without padding.

Current workspace already pins RustCrypto `aes = 0.8.4`; it is only a dev
dependency of `i2pr-tunnel` today.

Recommended dependency change:

```text
promote `aes` to an i2pr-tunnel production dependency
add RustCrypto `cbc` 0.1.x with minimal/default-disabled features
```

Use the `aes` crate's block encrypt/decrypt traits for the two single-block ECB
operations. Use the reviewed `cbc` crate in no-padding mode for the fixed
1008-byte data region.

Do **not** hand-roll AES, CBC padding, or another cryptographic primitive.
Do not add OpenSSL/libsodium/native dependencies.

## 12. Decrypted tunnel-message builder/parser

Implement a bounded model for the 1008-byte decrypted payload.

Required delivery enum:

```text
Local
Router { router_hash }
Tunnel { gateway_router_hash, tunnel_id }
```

Required fragment enum or equivalent:

```text
Unfragmented
First { message_id }
FollowOn { message_id, sequence: 1..63, is_last }
```

Parser requirements:

- strict flag validation;
- delivery value `3` rejected;
- delay bit rejected if set;
- extended-options bit rejected if set;
- reserved bits rejected if nonzero;
- zero/noncanonical fragment sequence rejected;
- zero fragment length rejected;
- declared length exceeding remaining payload rejected;
- parser never reads beyond the fixed 1008-byte region;
- padding scan is bounded;
- checksum verified before fragment dispatch;
- checksum comparison should not expose useful timing distinctions if a
  constant-time primitive is already available cheaply; do not add a large
  dependency solely for four public checksum bytes.

Builder requirements:

- cryptographic RNG supplied by caller;
- random 16-byte IV per TunnelData cell;
- nonzero random padding before the delimiter;
- one zero delimiter;
- exact checksum construction;
- exact 1008-byte payload;
- no trailing padding after fragment records;
- no payload-sized allocation based solely on untrusted fragment metadata.

Batching multiple small records into one 1008-byte payload may be implemented
if simple, but **correct single-message packing is the required floor**. Do not
delay closure for packing optimization.

## 13. Fragmentation

Implement deterministic fragmentation under explicit bounds.

Required rules:

- maximum 64 fragments total;
- follow-on sequence range 1..63;
- first fragment carries delivery instructions;
- follow-on fragments inherit destination solely by matching message ID;
- complete I2NP input must already satisfy the repository's I2NP maximum and
  the tunnel-fragment maximum;
- fragment length fields use checked `u16` conversion;
- message IDs are caller/RNG supplied and must not be zero if the implementation
  chooses zero as a local sentinel;
- do not assume fragments remain ordered through a tunnel.

Required deterministic builder tests:

```text
small LOCAL message -> one cell
small ROUTER message -> one cell
small TUNNEL message -> one cell
boundary-sized unfragmented messages
2-fragment message
maximum permitted fragment sequence
message too large -> typed rejection
```

## 14. Bounded reassembly

Create a runtime-neutral reassembler with caller-supplied time.

It must bound at least:

```text
maximum concurrent partial messages
maximum bytes per partial message
maximum aggregate retained bytes
maximum fragments per message = 64
maximum age / explicit expiry policy
```

The exact policy defaults are implementation policy, not wire format. Define
named constants/config fields and document the chosen values. They must be
small enough for ordinary router operation and testable without wall-clock
sleep.

Required behavior:

- first fragment may arrive before or after some follow-ons if the chosen data
  structure can safely retain bounded orphans;
- out-of-order valid fragments reassemble;
- exact duplicate fragment is idempotent;
- conflicting duplicate fragment invalidates/drops that partial message;
- fragment from another tunnel/endpoint cannot join a partial message;
- sequence above 63 rejected;
- `last` cannot imply a sequence smaller than an already-observed higher
  sequence;
- completion requires every sequence from first through declared last;
- delivery instructions come only from the first fragment;
- expiry drops retained buffers;
- saturation fails closed without unbounded allocation.

Do not key partial state only by message ID globally. Include the relevant local
tunnel/endpoint context so cross-tunnel collisions cannot merge messages.

## 15. Layer cryptography API

Implement pure fixed-size transforms over:

```text
iv: [u8; 16]
data: [u8; 1008]
```

Required operations:

```text
participant_forward(layer_keys, iv, data)
creator_inverse_one_hop(layer_keys, iv, data)
outbound_preprocess(all_hops_reverse_order, iv, plaintext_data)
inbound_endpoint_decrypt(all_hops_reverse_order, received_iv, received_data)
```

Use the existing `LayerKeys` getters.

Never derive tunnel layer keys again in this module.

Required crypto tests:

1. one-hop inverse followed by participant forward returns original IV/data;
2. multi-hop outbound inverse chain followed by participant transforms in path
   order returns the original preprocessed IV/data;
3. multi-hop inbound participant transforms followed by creator inverse chain
   returns the original preprocessed IV/data;
4. wrong layer key does not reproduce the original payload/checksum;
5. wrong IV key does not reproduce the original payload/checksum;
6. fixed deterministic vector freezes at least one complete IV/data transform;
7. compare the algorithm/order against pinned Emissary source during review;
   the test itself remains local Rust and does not run Emissary.

## 16. Duplicate fingerprint/window

Implement:

```text
duplicate_tag = received_iv XOR received_data[0..16]
```

Use a bounded exact replay window for Plan 116 role state unless an existing
repository bounded-filter primitive is clearly reusable.

Required tests:

- first observation accepted;
- exact duplicate rejected;
- swapping IV and first data block does not evade the XOR token;
- capacity cannot allocate beyond configured bound;
- expiry/caller-time cleanup releases entries;
- separate test role instances do not accidentally share mutable global state.

Do not introduce a process-global Bloom filter in `i2pr-tunnel`; production
runtime aggregation policy can be composed later without changing the wire
transform.

---

# Work Package C — runtime-neutral tunnel roles and deterministic exploratory pair

## 17. Goal

Compose Work Packages A and B into actual local role trajectories that consume
registered tunnel material and emit semantic router-delivery actions.

Suggested module:

```text
crates/i2pr-tunnel/src/roles.rs
```

Do not introduce Tokio or sockets.

## 18. Router-delivery action boundary

Define one small action equivalent to:

```text
RouterDeliveryAction {
    target_router: Hash,
    semantic I2NP message/body + message id + expiration
}
```

Requirements:

- no socket/future/channel ownership;
- no NTCP2-specific header selection;
- no dependency from `i2pr-tunnel` to `i2pr-transport` unless dependency review
  proves it is already an intended direction;
- easy later adaptation to `EncodedI2npMessage -> DeliveryRequest` in the
  runtime composition layer;
- Debug must report type/length/target category without raw tunnel payload.

The deterministic Plan 116 test driver may route these actions directly to the
next in-memory role instance.

## 19. Outbound gateway role — local creator

Input:

```text
usable established outbound tunnel
a complete standard semantic I2NP message
delivery instruction: ROUTER or TUNNEL
caller time/RNG
```

Required behavior:

```text
validate tunnel usable/not expired
 -> encode nested/forwarded I2NP as required by delivery mode
 -> fragment/pack into decrypted tunnel payload(s)
 -> choose fresh IV for each cell
 -> checksum + nonzero padding
 -> apply creator inverse layer transform over all hops in reverse order
 -> construct i2pr-proto TunnelDataMessage addressed to first hop receive_tunnel
 -> emit action to first hop router
```

LOCAL delivery from an outbound gateway is not required and should fail
explicitly rather than be silently treated as router delivery.

## 20. Participant role

Create minimal transit-role state for deterministic execution and future
Milestone 11 reuse. Do not place creator path knowledge in this state.

Per role instance retain only what that hop needs:

```text
receive tunnel id
next router
next tunnel id
layer/IV keys
first/locked previous peer identity
bounded duplicate window
expiry
```

Input is one TunnelData I2NP semantic message plus previous router identity.

Required behavior:

```text
message type must be TunnelData
 -> tunnel id must match receive id
 -> lock previous peer on first accepted packet
 -> later packet from different previous peer rejected
 -> duplicate token checked
 -> participant forward AES transform
 -> rewrite outer tunnel id to next tunnel id
 -> emit next-router TunnelData action
```

The participant must not parse fragment semantics or inner I2NP messages.

## 21. Outbound endpoint role

The OBEP applies the same participant forward transform once, exposing the
preprocessed tunnel payload.

Then:

```text
verify checksum/padding
 -> parse fragment records
 -> bounded reassembly
 -> apply delivery instructions
```

Minimum actions:

### ROUTER

Emit the reconstructed standard I2NP message to the target router.

### TUNNEL

Wrap the reconstructed standard I2NP message in the existing
`i2pr-proto::TunnelGatewayMessage` addressed to the specified gateway tunnel
ID, then emit it to the specified gateway router.

Do not implement destination delivery or garlic semantics here.

## 22. Inbound gateway role

The inbound gateway receives an existing `TunnelGatewayMessage` addressed to
its receive tunnel ID.

Required behavior mirrors the deployed implementation:

```text
validate TunnelGateway target ID
 -> validate nested standard I2NP message / expiration policy
 -> fragment with LOCAL delivery instructions
 -> generate IV/checksum/padding
 -> apply this IBGW hop's participant-forward AES transform
 -> rewrite to configured next tunnel ID
 -> emit TunnelData to configured next router
```

The IBGW does not possess the creator's full path and must not require it.

## 23. Inbound participant role

Same forwarding primitive and state boundaries as §20.

No fragment parsing.

## 24. Local inbound endpoint — creator

Lookup by the explicit local inbound receive tunnel ID retained in Work Package
A.

Required behavior:

```text
receive TunnelData
 -> validate local receive tunnel
 -> apply inverse layer transform over established inbound hops in reverse order
 -> verify checksum/padding
 -> parse/reassemble LOCAL fragment stream
 -> require LOCAL delivery
 -> return reconstructed standard I2NP message to local router-facing boundary
```

ROUTER/TUNNEL delivery at the local inbound endpoint is outside the normal
inbound creator use case and should be rejected unless the official current
specification/reference demonstrates a required use for it.

## 25. Expiration and usable-state rules

Every gateway/endpoint lookup must refuse:

```text
unknown tunnel
Building tunnel
Expired tunnel
Failed tunnel
removed tunnel
```

Plan 116 must make expiration operational, not merely a metadata state:

- `advance_time()` must make expired entries unavailable to send/receive paths;
- cleanup must drop established secret material;
- pre-expiry replacement scheduling may be represented as a typed pool need
  signal rather than a runtime timer;
- deterministic caller time is sufficient;
- do not add a background task in `i2pr-tunnel`.

If existing `advance_time()` only marks entries `Expired` but retains keys
indefinitely, add an explicit deterministic cleanup/removal path and test it.

---

# 26. Deterministic end-to-end acceptance trajectories

These tests are the core Plan 116 evidence. They must use actual production
TunnelData builder/parser/layer/role code, not a parallel simulator codec.

## 26.1 Outbound two-hop path

Topology:

```text
local OBGW
 -> remote Participant A
 -> remote OBEP B
 -> ROUTER delivery target
```

Require:

- established outbound material comes from the build-success path or the
  closest deterministic production constructor;
- OBGW emits one or more TunnelData messages addressed to A's receive ID;
- A accepts previous-peer identity, applies one layer, rewrites tunnel ID;
- B applies final layer, validates checksum, reassembles;
- reconstructed standard I2NP bytes equal the original;
- B emits ROUTER action to the declared target.

## 26.2 Outbound-to-inbound tunnel delivery

Topology:

```text
local outbound OBGW
 -> outbound participant(s)
 -> OBEP
 -> TUNNEL delivery
 -> inbound IBGW
 -> inbound participant(s)
 -> local inbound endpoint
```

Require the original nested standard I2NP message to emerge byte-for-byte at
the local inbound endpoint after the full tunnel-to-tunnel path.

This is the most important local Milestone 5 trajectory.

## 26.3 Fragmented trajectory

Use an I2NP message large enough to require multiple TunnelData cells.

Require:

- first + follow-on fragment encoding is canonical;
- deliberately reorder cells before endpoint processing;
- reassembly succeeds;
- exact duplicate fragment is harmless/idempotent;
- conflicting duplicate causes only that partial message to fail;
- missing fragment followed by caller-time expiry frees memory;
- another tunnel using the same message ID cannot join the partial state.

## 26.4 Negative role trajectory

Cover at least:

```text
unknown receive tunnel
wrong previous peer after lock
replayed TunnelData token
malformed checksum
missing zero delimiter
zero padding byte before delimiter / ambiguous padding handling
invalid delivery type 3
reserved flag bits
fragment sequence > 63
zero fragment length
oversized complete message
expired established tunnel
removed established tunnel
```

All failures must be typed or deterministically classified and bounded.

---

# 27. Resource and security requirements

Plan 116 remains security-sensitive despite being local-only.

Required properties:

- fixed TunnelData buffers stay fixed-size; avoid attacker-directed large
  allocations;
- reassembly memory is explicitly bounded globally per reassembler and per
  tunnel context;
- duplicate state is bounded;
- no raw tunnel ciphertext/plaintext in `Debug`/tracing;
- no layer/IV/reply/garlic key bytes in formatting;
- errors do not embed raw attacker payloads;
- no participant gets creator-only path vectors;
- no endpoint accepts a fragment sequence with impossible/inconsistent state;
- checked integer arithmetic for size/fragment calculations;
- no panic on malformed public input;
- no unbounded collections;
- RNG failures surface as typed failures where randomness is mandatory;
- tunnel cleanup releases key/reassembly/replay state.

Do not optimize allocations or batching before the full deterministic trajectory
works correctly. Correct fixed-size behavior first.

---

# 28. Required implementation order

Execute in this order:

```text
116A. established secret/material ownership
     -> one-time state-machine transfer
     -> real registrar/pool insertion
     -> correct inbound gateway/local IDs
     -> acceptance A green

116B. decrypted payload + delivery instructions
     -> fragmentation/reassembly
     -> AES layer transforms
     -> duplicate fingerprint/window
     -> acceptance B green

116C. outbound gateway + participant + OBEP
     -> inbound gateway + participant + local endpoint
     -> full deterministic tunnel-to-tunnel trajectory
     -> lifecycle cleanup
     -> acceptance C green

116D. documentation/support propagation and closure status
```

Do not parallelize 116A and 116C: C depends on real usable established state.
116B may be implemented after A's data model is fixed.

---

# 29. Validation commands

At minimum, before closure:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

If a historical interop script has a known pre-existing failure unrelated to
Plan 116, record it precisely; do not modify the historical harness to make the
Plan 116 data plane pass.

Add focused Plan 116 test selectors to the closure record so a smaller model or
future maintainer can reproduce the data-plane evidence without running every
historical interoperability tool.

---

# 30. Documentation updates required at closure

Update only the current support/architecture surfaces necessary to describe the
new capability:

```text
plans/116-status.md                 new closure record
plans/116-handoff.md                point to Plan 117 only after closure
plans/115-handoff.md                remain closed / Plan 116 predecessor
plans/115-117-external-delivery-to-live-netdb-roadmap.md
README.md
docs/architecture/i2pr-tunnel.md
docs/architecture/overview.md
docs/protocol-support.md
specs/support.toml
AGENTS.md                            concise current-state block only
```

Do not expand the NTCP2 skill/harness documentation unless Plan 116 actually
changes that surface, which it should not.

---

# 31. Plan 116 closure criteria

Plan 116 may close as:

```text
plan_116 = passed-local-tunnel-data-plane
```

only when **all** of the following are true:

1. `ShortBuildRegistrar` no longer fabricates `TunnelSlot(0)` success.
2. A successful short build transfers usable per-hop established key material
   exactly once into real bounded pool ownership.
3. Inbound external gateway tunnel ID and local receive tunnel ID are modeled
   distinctly and correctly.
4. Established secret material is unavailable after failure/removal/cleanup.
5. TunnelData decrypted-payload parsing/building matches the current official
   tunnel-message format.
6. Checksum and nonzero-padding rules are enforced.
7. LOCAL, ROUTER, and TUNNEL delivery instructions required by exploratory
   routing are supported.
8. Fragmentation/reassembly supports valid out-of-order delivery within strict
   bounds and rejects malformed/conflicting state.
9. Participant AES transform matches the official ECB/CBC/ECB ordering.
10. Outbound creator inverse preprocessing over multiple hops round-trips
    through participant transforms.
11. Inbound creator inverse processing over multiple hops recovers the original
    preprocessed payload.
12. Previous-hop pinning and duplicate token suppression are enforced in
    participant state.
13. A deterministic outbound tunnel carries a real encoded I2NP message through
    participant + OBEP and emits correct ROUTER delivery.
14. A deterministic outbound-to-inbound tunnel chain carries a real encoded
    I2NP message through TUNNEL delivery + IBGW + inbound participant(s) and
    returns the exact original I2NP message at the local inbound endpoint.
15. Multi-fragment/out-of-order/duplicate/timeout paths are tested.
16. Unknown, malformed, expired, failed, and removed tunnel inputs fail within
    explicit resource limits.
17. `i2pr-tunnel` remains runtime-neutral and does not hard-code NTCP2.
18. No Q1/Q2/live mixed-router claim is made.
19. Full required local CI/boundary validation is green except any explicitly
    documented pre-existing historical-harness blocker unrelated to this plan.
20. Current planning/status/support docs state **Plan 117** as the next external
    integration checkpoint only after this local data plane passes.

---

# 32. What Plan 116 does not prove

A successful Plan 116 means:

```text
local tunnel build control plane  = usable
local established-state ownership = real
local TunnelData data plane       = working
local exploratory tunnel pair     = deterministic-working
```

It does **not** mean:

```text
NTCP2 interoperability            = passed
live independent TunnelData       = passed
mixed-router exploratory pair     = passed
live NetDB over exploratory       = passed
normal daemon NTCP2               = enabled
production-ready/anonymity-safe   = true
```

Those distinctions must remain visible.

---

# 33. Handoff after success

After Plan 116 closes, the next line of work is Plan 117, and only then should
external delivery become the dominant question again.

Plan 117 should integrate already-built components rather than build them live:

```text
validated RouterInfo / path selection
 -> real short tunnel build
 -> established tunnel material
 -> working TunnelData data plane
 -> smallest available real router-delivery lane
 -> independent hop(s)
 -> outbound exploratory DatabaseLookup
 -> reply through inbound exploratory tunnel
 -> NetDB validation/persistence
 -> local RouterInfo publication verification
```

If the development host still cannot supply a qualified router transport at
that point, record that as a Plan 117 integration/evidence blocker. Do not
back-propagate it into Plan 116 or reopen short-build construction work.