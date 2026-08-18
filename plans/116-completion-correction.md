# Plan 116 completion/correction: finish the local tunnel data plane

## Status

- **Ready for execution**.
- Date: 2026-08-18.
- Milestone: **5 — Network tunnel data plane and exploratory tunnels**.
- Implementation floor: `91d3a8569ee20d71ab7a4ae27b6c54a1e5009429`.
- Parent plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md).
- Current partial status: [`116-status.md`](116-status.md).
- Successor after this correction passes: Plan 117 live exploratory/NetDB integration.

This is a **narrow completion/correction pass over the existing Plan 116 implementation**.
Do not replace the new module structure and do not start another interoperability project.
The objective is to turn the current scaffolding into the working local data plane that Plan 116 originally required.

The current implementation is useful but is not a Plan 116 closure. It added the intended module boundaries, then left core behavior incomplete or incorrect and marked 17 failing tests `#[ignore]`. This pass must correct those defects, remove the ignored-test escape hatch, wire the successful short-build state into the real exploratory pool, and prove the deterministic outbound-to-inbound tunnel trajectory before Plan 117 is allowed to start.

---

# 1. Hard scope lock

This pass is local, Rust-only, runtime-neutral, and transport-neutral.

Do **not** perform or require:

- new Emissary/i2pd/Java runtime validation;
- NTCP2 correction, activation, or harness work;
- SSU2 work;
- Q1/Q2 external-delivery closure;
- rootless namespaces, privileged namespaces, Docker, Multipass, QEMU, or VMs;
- Python interoperability orchestration;
- public I2P participation;
- a generic I2NP router dispatcher;
- garlic, LeaseSet, streaming, SAM, I2CP, HTTP/SOCKS, or destination tunnels;
- Milestone 11 public transit admission policy;
- a new crate for TunnelData;
- a second tunnel-message implementation beside the current Plan 116 modules.

Do not use an environment limitation as a reason to stop this pass. Every required acceptance trajectory is deterministic and in-process.

The only acceptable reason to pause for research is a precise protocol ambiguity discovered in the current Tunnel Message/AES implementation. Resolve that exact question from the official I2P specification and pinned deployed reference source, then continue.

---

# 2. Existing work to retain

Do not discard the current Plan 116 module split unless a specific API shape prevents correctness:

```text
crates/i2pr-tunnel/src/established.rs
crates/i2pr-tunnel/src/data.rs
crates/i2pr-tunnel/src/fragment.rs
crates/i2pr-tunnel/src/layer.rs
crates/i2pr-tunnel/src/roles.rs
```

Retain where correct:

- `EstablishedTunnel` / `EstablishedHop` as the secret-bearing ownership concept;
- `TunnelMessageBuilder` / `TunnelMessageParser` as the tunnel-message preprocessing boundary;
- `BoundedReassembler` as the endpoint reassembly boundary;
- `TunnelLayerTransform` as the AES transform boundary;
- `DuplicateToken` / `DuplicateWindow` as the bounded local replay primitive;
- runtime-neutral role objects in `roles.rs`;
- existing `i2pr-proto::TunnelDataMessage` and `TunnelGatewayMessage` framing;
- the existing `LayerKeys` KDF output; never derive tunnel layer keys again.

This pass should correct semantics and complete wiring rather than rewrite the subsystem.

---

# 3. Authoritative protocol facts

Primary authority:

- Tunnel Message Specification:
  <https://geti2p.net/spec/tunnel-message>
- Tunnel Implementation:
  <https://geti2p.net/docs/tunnels/implementation>
- ECIES-X25519 Tunnel Creation:
  <https://geti2p.net/spec/tunnel-creation-ecies>

Pinned source reference already used by Plans 115/116:

```text
repository = https://github.com/eepnet/emissary.git
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
```

Useful files at that revision:

```text
emissary-core/src/i2np/tunnel/data.rs
emissary-core/src/tunnel/noise.rs
emissary-core/src/tunnel/transit/inbound.rs
emissary-core/src/tunnel/transit/participant.rs
emissary-core/src/tunnel/transit/outbound.rs
emissary-core/src/tunnel/hop/inbound.rs
emissary-core/src/tunnel/hop/outbound.rs
```

These references are for source reconciliation only. Do not execute Emissary in this pass.

## 3.1 Fixed TunnelData body

```text
Tunnel ID          4 bytes, nonzero
IV                16 bytes
Encrypted Data  1008 bytes
---------------------------
total           1028 bytes
```

The decrypted 1008-byte data region is:

```text
checksum[4]
nonzero random padding[0..]
zero delimiter 0x00
one or more delivery-instruction + fragment records
```

Checksum is exactly:

```text
SHA256(bytes AFTER the zero delimiter || IV)[0..4]
```

The checksum does **not** include:

- the checksum bytes themselves;
- padding;
- the zero delimiter.

## 3.2 Initial/unfragmented control byte

For a control byte with bit 7 clear:

```text
bit 7    = 0
bits 6-5 = delivery type
            00 LOCAL
            01 TUNNEL
            10 ROUTER
            11 invalid
bit 4    = delay, unsupported => 0
bit 3    = fragmented
            0 => complete unfragmented I2NP message
            1 => first fragment; Message ID follows
bit 2    = extended options, unsupported => 0
bits 1-0 = reserved => 0
```

The **Message ID is present only when bit 3 is 1**.

An unfragmented message is not semantically the same thing as a first fragment.

## 3.3 Follow-on fragment

```text
bit 7    = 1
bits 6-1 = sequence 1..63
bit 0    = last-fragment flag
Message ID[4]
size u16
fragment bytes
```

The wire sequence range remains `1..=63`. The current maximum complete I2NP message limit remains 62,708 bytes.

## 3.4 AES layer transform

Participant, inbound gateway, and outbound endpoint apply the forward transform:

```text
working_iv = AES256-ECB-ENC(iv_key, received_iv)
new_data   = AES256-CBC-ENC(layer_key, working_iv, received_data)
next_iv    = AES256-ECB-ENC(iv_key, working_iv)
```

The creator inverse for one hop is therefore:

```text
working_iv = AES256-ECB-DEC(iv_key, received_next_iv)
prior_data = AES256-CBC-DEC(layer_key, working_iv, received_data)
prior_iv   = AES256-ECB-DEC(iv_key, working_iv)
```

Outbound creator preprocessing applies the inverse over hops in **reverse path order**.

The local inbound endpoint applies the same inverse over the remote inbound-hop keys in **reverse path order**.

The OBEP is a transit hop for the final layer. It applies the **forward participant transform once**, then verifies/parses the exposed Tunnel Message. It does not call the creator inverse primitive.

---

# 4. Defects localized at the implementation floor

Treat this list as the mandatory correction inventory. Do not close the pass by documenting these as “provisional”.

## C1 — ignored tests hide Plan 116 failures

At `91d3a856...`, 17 tests in `data.rs`, `layer.rs`, and `roles.rs` were marked:

```rust
#[ignore = "Plan 116 provisional scaffolding: see plans/116-status.md"]
```

These tests cover core Plan 116 behavior and may not remain ignored at closure.

## C2 — checksum builder/parser disagree and the builder hashes the delimiter

Current `pack_payload()` constructs a checksum input containing `0x00 || body`, while the specification says checksum begins with the first delivery instruction after the delimiter.

Current parser verification also passes the checksum-plus-padding prefix into a helper that expects a four-byte slice, causing canonical padded messages to fail.

## C3 — unfragmented and first-fragment encoding are conflated

Current builder always encodes a Message ID in the initial record while leaving the fragmented bit clear. This produces a non-canonical initial header.

## C4 — automatic fragmentation is incomplete

The builder currently handles one short initial record and pre-split fragment records but does not provide the required complete-message -> first/follow-on fragmentation path for messages larger than one tunnel cell.

## C5 — creator AES inverse uses ECB encryption instead of decryption

`creator_inverse_one_hop()` must use ECB decryption around CBC decryption. The current one-hop and multi-hop tests fail because the inverse is not actually the inverse of the participant transform.

## C6 — OBEP uses the inverse transform instead of the participant forward transform

The outbound endpoint must apply the final forward hop transformation before parsing the exposed plaintext.

## C7 — role APIs use deterministic/fake production randomness

Current role code includes deterministic IVs and a zero-only RNG that implements `CryptoRng`. That must not exist in a production-facing send path.

## C8 — padding randomness is repeated/deterministically substituted

Current `fill_nonzero()` requests only 64 bytes, repeats them over the full padding region, and substitutes zero outputs with deterministic index-derived values. Padding must come from the injected cryptographic RNG and remain nonzero without deterministic fallback bytes.

## C9 — established ownership remains disconnected from short-build success

`ShortBuildRegistrar::admit()` still returns fabricated `TunnelSlot::from_raw(0)` and does not insert a secret-bearing established tunnel into the pool.

## C10 — inbound established topology synthesizes a local endpoint as a remote hop

The actual inbound short-build remote path is:

```text
IBGW -> Participant*
```

The local creator is the inbound endpoint but is not another remote short-build hop and does not own another remote-hop build record.

Established inbound ownership must therefore retain:

```text
remote_hops = [IBGW, Participant*]
local_inbound_receive_tunnel = distinct local endpoint identifier
```

Do not require a synthetic `EstablishedRole::InboundEndpoint` element in the remote hop vector merely to represent the local endpoint.

## C11 — “not applicable” tunnel/router fields use synthetic sentinel values

Do not use `u32::MAX` or an all-zero router hash as meaningful “none” values inside established routing state. Represent optional routing values with `Option<TunnelId>`, `Option<TunnelPeer>`, or one typed optional next-hop structure.

## C12 — reassembly expiry is a no-op

`BoundedReassembler::expire_due()` currently retains all partials regardless of caller time.

## C13 — reassembly capacity accounting occurs after insertion

A new entry can be inserted and only then produce `CapacityExceeded`, leaving state beyond the advertised bound unless explicitly rolled back.

## C14 — aggregate reassembly byte accounting is absent

Per-message limits exist, but Plan 116 requires a bounded aggregate retained-byte ceiling.

## C15 — endpoint/role fragment handling only consumes the first parsed record

Role composition must process every fragment record emitted by a Tunnel Message and drive reassembly correctly. It may not take `records.into_iter().next()` and discard the rest.

## C16 — message metadata is fabricated or lost

Role code currently hard-codes message IDs/expiration in some inbound paths and emits `expiration_ms: 0` in endpoint actions. Where an existing standard I2NP message already supplies its canonical header, preserve/reparse that message rather than inventing separate tunnel metadata.

---

# 5. Closure rule before implementation starts

The current 17 ignored tests are not an acceptable permanent state.

At the beginning of this pass:

1. enumerate every `#[ignore = "Plan 116 provisional scaffolding..."]` test;
2. run those tests explicitly with `--ignored` to confirm the current failure categories;
3. remove the `#[ignore]` markers as the relevant correction is implemented;
4. do not replace them with weaker tests;
5. do not change assertions merely to match the broken implementation;
6. by closure, **zero Plan 116 tests are ignored** for “provisional scaffolding”.

A test may remain ignored only if it is unrelated to Plan 116 and has an established pre-Plan-116 reason. If such a test exists, document its exact historical provenance. The current 17 do not qualify.

---

# 6. Execution order

Execute strictly in this order:

```text
116C-1  restore wire-format correctness
        -> checksum
        -> unfragmented vs first-fragment encoding
        -> real automatic fragmentation
        -> parser round trips green

116C-2  restore AES correctness
        -> true inverse ECB-DEC/CBC-DEC/ECB-DEC
        -> OBEP forward transform
        -> one-hop/multi-hop vectors green

116C-3  remove fake randomness
        -> injected CSPRNG in gateway roles
        -> fresh IV per cell
        -> fresh nonzero padding
        -> no zero-only CryptoRng

116C-4  repair established-state model and registration
        -> remote-hop-only inbound topology
        -> remove sentinel next-hop values
        -> one-time material transfer
        -> real pool ownership
        -> reply path uses first remote IBGW receive ID

116C-5  complete bounded reassembly
        -> per-entry timestamps
        -> expiry
        -> pre-admission capacity checks
        -> aggregate byte budget
        -> all records consumed

116C-6  complete role trajectories
        -> outbound ROUTER
        -> outbound TUNNEL -> inbound gateway
        -> inbound participants
        -> local inbound endpoint
        -> fragmented/out-of-order path

116C-7  remove all provisional ignores
        -> full Plan 116 tests green
        -> full workspace validation
        -> close Plan 116
```

Do not wire the pool on top of broken wire/AES primitives. Do not start role E2E debugging until the isolated wire and AES tests are green.

---

# 7. Correction 1 — canonical tunnel-message preprocessing

Modify `crates/i2pr-tunnel/src/data.rs` in place.

## 7.1 Separate semantic fragment states

The representation must distinguish at least:

```text
Unfragmented {
    delivery,
    body,
}

FirstFragment {
    delivery,
    message_id,
    body,
}

FollowOn {
    message_id,
    sequence,
    is_last,
    body,
}
```

A different type layout is acceptable if it guarantees the same wire distinction.

Do not infer “unfragmented” from a `First` variant whose `is_last()` always returns true.

## 7.2 Initial record encoding

For unfragmented:

```text
bit7 = 0
bit3 = 0
no Message ID
size u16
message bytes
```

For first fragment:

```text
bit7 = 0
bit3 = 1
Message ID u32, nonzero
size u16
fragment bytes
```

Both use the normal LOCAL/TUNNEL/ROUTER delivery fields before the optional Message ID.

Parser output must preserve whether the record was unfragmented or fragmented-first.

## 7.3 Checksum construction

Build the final 1008-byte region as:

```text
[checksum 4][padding nonzero][00][record bytes]
```

Compute:

```text
checksum = SHA256(record_bytes || IV)[0..4]
```

If multiple records are packed into one cell, `record_bytes` means the entire byte sequence after the delimiter.

Do not hash the delimiter.

## 7.4 Checksum parsing

Parser logic:

```text
expected_checksum = plaintext[0..4]
scan plaintext[4..] for first 0x00 delimiter
post_zero = plaintext[delimiter+1..]
actual_checksum = SHA256(post_zero || IV)[0..4]
constant-time compare first four bytes
parse post_zero as records
```

The parser must not require `plaintext[..delimiter]` to be length four; that prefix necessarily includes optional padding.

## 7.5 Padding generation

Padding may be zero bytes long if the record region fills the cell exactly.

If padding is required:

- call the injected CSPRNG over the actual padding region;
- for each zero output byte, resample that byte from the RNG;
- use a small explicit retry ceiling per byte (for example 32 attempts);
- if the source continues returning zero or errors, return a typed randomness failure;
- never derive fallback bytes from the index;
- never reuse one 64-byte random block cyclically across the full padding.

Repeated random byte values are not themselves invalid; deterministic periodic padding is.

## 7.6 Automatic fragmentation

Add one production helper that accepts a complete standard I2NP byte sequence plus delivery instruction and message ID and returns the required ordered fragment records/cells.

Minimum implementation policy may emit one fragment record per TunnelData cell. Batching multiple fragment records into one cell is not required for this correction.

Required behavior:

- if the complete message fits unfragmented for that delivery instruction, emit one unfragmented cell;
- otherwise emit one fragmented-first cell and one or more follow-on cells;
- follow-on sequence starts at 1;
- only the terminal follow-on sets `is_last = true`;
- reject if representation would exceed sequence 63 / bounded complete-message maximum;
- no zero-length fragments;
- checked arithmetic only.

## 7.7 Wire acceptance tests

All must be active, not ignored:

1. unfragmented LOCAL round trip;
2. unfragmented ROUTER round trip;
3. unfragmented TUNNEL round trip;
4. fragmented first record has bit 3 set and Message ID present;
5. unfragmented record has bit 3 clear and no Message ID;
6. two-fragment round trip;
7. multi-fragment round trip near a several-cell boundary;
8. checksum excludes padding and delimiter;
9. changing IV causes checksum failure;
10. changing post-zero bytes causes checksum failure;
11. malformed delimiter fails;
12. delivery type `11` fails;
13. delay/extended/reserved flags fail;
14. sequence 0 and >63 fail;
15. zero-size fragment fails;
16. oversize message fails;
17. RNG failure / repeated-zero source returns typed failure rather than deterministic padding.

Freeze at least one full 1008-byte deterministic test vector using a deterministic **test-only** seeded CSPRNG such as `ChaCha8Rng`; do not expose that RNG in a production role method.

---

# 8. Correction 2 — canonical AES forward/inverse transforms

Modify `crates/i2pr-tunnel/src/layer.rs`.

## 8.1 Add ECB decrypt primitive

Implement the single-block equivalent of:

```text
AES256-ECB-DEC(key, block)
```

using RustCrypto block-decrypt traits.

No padding and no allocation are necessary for a 16-byte block.

## 8.2 Correct one-hop inverse

Replace the current inverse with:

```text
working_iv = ECB_DEC(iv_key, received_next_iv)
prior_data = CBC_DEC(layer_key, working_iv, received_data)
prior_iv   = ECB_DEC(iv_key, working_iv)
```

Do not use ECB encryption anywhere in this inverse helper.

## 8.3 Multi-hop order

For creator preprocessing:

```text
plaintext
 -> inverse(last hop)
 -> inverse(previous hop)
 -> ...
 -> inverse(first hop)
 -> cell sent to first hop
```

Remote hops then call forward transform in path order and recover the original `(IV, plaintext)` at the terminal endpoint.

For inbound tunnels, remote IBGW/participants apply forward transforms. The local creator applies inverse transforms over the remote hops in reverse order.

## 8.4 OBEP behavior

In `roles.rs`, OBEP handling must:

```text
validate receive tunnel
check previous peer / replay policy as applicable
split received cell
participant_forward(OBEP keys, received IV/data)
use returned IV + data as the exposed Tunnel Message
verify checksum
parse/reassemble
apply delivery instruction
```

Do not call `creator_inverse_one_hop()` in OBEP processing.

## 8.5 AES acceptance tests

Activate and pass:

1. one-hop inverse -> forward exact round trip;
2. one-hop forward -> inverse exact round trip;
3. three-hop outbound creator inverse chain -> A forward -> B forward -> OBEP forward yields exact original IV/data;
4. inbound IBGW forward -> participant forward -> creator inverse reverse chain yields exact original IV/data;
5. wrong layer key fails to reproduce original;
6. wrong IV key fails to reproduce original;
7. duplicate token remains `received_iv XOR received_ciphertext[0..16]`;
8. fixed vector compared against the algorithm/order in pinned Emissary source.

The test vector remains local Rust. Do not execute Emissary.

---

# 9. Correction 3 — production randomness boundary

Modify gateway/creator role APIs in `roles.rs` so randomness is explicit.

Recommended shape:

```rust
fn forward<R: CryptoRng + RngCore>(..., rng: &mut R) -> Result<...>
```

or an equivalent caller-owned randomness seam.

Requirements:

- fresh 16-byte IV per TunnelData cell;
- CSPRNG-sourced nonzero padding;
- fragmented messages get a fresh IV for every cell;
- no constant IV literals in production send logic;
- no `DeterministicZeroRng` implementing `CryptoRng` in production code;
- no deterministic padding substitution;
- RNG failure propagates as a typed tunnel-role/tunnel-message failure;
- tests use seeded `ChaCha8Rng` or another explicit test RNG only under `#[cfg(test)]`.

Add a test that two sequential cells from the same seeded RNG consume distinct IV material.

Do not test statistical randomness. Test correct dependency injection and absence of hard-coded values.

---

# 10. Correction 4 — established tunnel ownership model

Modify `established.rs`, `short.rs`, `short_state.rs`, and `pool.rs` as needed.

## 10.1 Remote-hop model

`EstablishedTunnel.hops` represents remote tunnel hops only.

Outbound:

```text
[Participant*, OBEP]
```

Inbound:

```text
[IBGW, Participant*]
```

The local outbound gateway and local inbound endpoint are creator-side roles and are not remote build-record hops.

Remove any constructor invariant requiring the last inbound remote hop to be `InboundEndpoint`.

If `EstablishedRole::InboundEndpoint` is only used for the synthetic local element, remove it from `EstablishedHop` role semantics. The local endpoint role may remain a role object outside the remote-hop vector.

## 10.2 Eliminate sentinel routing state

Do not represent absence as:

```text
TunnelId(u32::MAX)
Hash([0; 32])
```

Use typed optional state, for example:

```text
next: Option<EstablishedNextHop>
```

where:

```text
EstablishedNextHop {
    router: TunnelPeer,
    tunnel: TunnelId,
}
```

Participants/IBGW require `Some(next)`.
OBEP requires `None` for data-plane forwarding because delivery is per-message.

For inbound creator ownership, keep `local_inbound_receive: TunnelId` as a separate local field.

## 10.3 One-time state-machine transfer

Add a success-only material extraction seam, equivalent to:

```text
ShortBuildStateMachine::take_established_material()
```

Requirements:

- unavailable before terminal `Established`;
- unavailable after rejection/timeout/cancel/invalid reply/delivery failure;
- consumes/moves the retained per-hop `LayerKeys` and path metadata;
- may succeed exactly once;
- second call returns a typed already-taken/unavailable result;
- no second long-lived copy of all established keys remains in the state machine;
- the public `ShortBuildOutcome` remains non-secret.

The transfer must preserve:

```text
direction
creator_tunnel_id
remote hop order
router hash per hop
receive_tunnel per hop
next router/tunnel where applicable
role per remote hop
LayerKeys per remote hop
created time / established time needed by pool lifecycle
inbound external gateway identity + its receive tunnel ID
local inbound receive tunnel ID
```

For inbound paths, derive the local inbound receive identifier from the existing explicit terminal routing/path field that currently represents the creator-side receive target. Do not manufacture another random/sentinel ID.

## 10.4 Real pool entry

Refactor `ExploratoryPool` so an established entry owns both:

```text
public TunnelRegistration metadata
secret EstablishedTunnel material
```

Recommended internal shape:

```text
TunnelEntry {
    registration: TunnelRegistration,
    established: EstablishedTunnel,
}
```

The public inspection methods may return metadata snapshots without exposing secrets.

Do not maintain independent metadata/key maps unless necessary; one owned entry is preferred.

## 10.5 Replace fake registrar

`ShortBuildRegistrar::admit()` must no longer return a fabricated `TunnelSlot::from_raw(0)`.

The successful path becomes:

```text
terminal Established
 -> take established material once
 -> validate pool direction/capacity/duplicate policy
 -> insert real TunnelEntry
 -> return actual assigned TunnelSlot
```

On registration failure, secret material must be dropped/zeroized and no active orphan remains.

## 10.6 Inbound reply-path correctness

`select_inbound_reply_path()` must return:

```text
gateway router = first remote IBGW peer
reply tunnel ID = first remote IBGW receive_tunnel
```

It must not return `creator_tunnel_id` merely because historical `TunnelRegistration.tunnel_id` stored that value.

## 10.7 Pool lifecycle

`advance_time()` must make expired entries unusable by data-plane lookup.

Add or use explicit cleanup/removal so expired secret-bearing entries are dropped and zeroized rather than retained indefinitely as metadata-plus-secrets.

No background task is required; caller-driven time remains correct.

## 10.8 Ownership acceptance tests

Active tests required:

1. successful outbound short build -> one real outbound pool entry;
2. successful inbound short build -> one real inbound pool entry;
3. returned `TunnelSlot` is non-fabricated and corresponds to stored entry;
4. outbound remote hop vector ends in OBEP;
5. inbound remote hop vector begins in IBGW and contains no synthetic local endpoint hop;
6. exact remote hop order preserved;
7. exact receive/next tunnel chain preserved;
8. OBEP next hop is `None`;
9. participant/IBGW next hop is `Some(...)`;
10. inbound reply path uses first IBGW receive tunnel ID;
11. local inbound receive ID remains distinct from creator logical slot/ID;
12. pre-success material extraction fails;
13. second extraction fails;
14. rejected/timed-out/cancelled build inserts nothing;
15. pool-full/duplicate handling leaves no orphan secret entry;
16. remove/fail/expire makes established material unavailable;
17. `Debug` output contains no key bytes.

---

# 11. Correction 5 — bounded reassembly completion

Modify `fragment.rs` rather than replacing it.

## 11.1 Per-entry age

Each partial entry must retain a caller-time creation or last-update timestamp sufficient for deterministic expiry.

`expire_due()` must actually remove entries older than the configured expiry policy.

No wall-clock call inside `i2pr-tunnel`.

## 11.2 Capacity check before ownership growth

When a fragment would create a new partial:

1. expire due entries;
2. check concurrent-partial capacity;
3. check aggregate-byte budget needed by the new fragment;
4. only then insert the new partial/body.

If an existing partial receives a new fragment, check per-message and aggregate byte budget before mutating retained state.

On error, leave accounting/state unchanged or explicitly roll it back before returning.

## 11.3 Aggregate byte budget

Add a named hard ceiling and tracked retained-byte count.

Expose non-secret introspection for tests such as:

```text
retained_messages()
retained_bytes()
```

Values must return to zero after completion, expiry, conflicting-invalid cleanup, and purge.

## 11.4 Duplicate semantics

Required behavior:

- exact duplicate first/follow-on fragment with identical body is idempotent;
- conflicting duplicate invalidates/drops the affected partial message;
- another tunnel context with the same message ID remains isolated;
- last sequence lower than already-seen higher sequence rejects and cleans only that partial;
- completion requires every sequence from 0 through final sequence.

If historical code treats duplicate first fragment differently from identical follow-on duplicate, normalize to the Plan 116 rule: identical duplicates are harmless/idempotent.

## 11.5 Reassembly tests

Require active tests for:

- first then follow-ons;
- follow-ons before first;
- out-of-order completion;
- exact duplicate idempotence;
- conflicting duplicate cleanup;
- missing fragment then expiry;
- same message ID in two tunnel contexts;
- per-message byte ceiling;
- aggregate byte ceiling;
- concurrent partial ceiling;
- no bound overshoot after a rejected insertion;
- retained bytes return to zero after completion/purge/expiry.

---

# 12. Correction 6 — finish runtime-neutral roles

Modify `roles.rs` after Sections 7–11 are green.

## 12.1 Outbound gateway

Input must include:

```text
usable EstablishedTunnel
complete standard I2NP bytes
ROUTER or TUNNEL delivery instruction
caller time
caller CSPRNG
```

Required:

```text
validate usable/not expired
 -> automatic fragment into one or more Tunnel Message cells
 -> fresh IV/padding per cell
 -> creator inverse over every outbound remote hop in reverse order
 -> TunnelDataMessage uses first remote hop receive_tunnel
 -> emit ordered semantic deliveries to first-hop router
```

Return a vector/iterator of cells/actions for fragmented messages, not one hard-coded cell.

Do not carry `original_iv` or complete plaintext message inside a production delivery object merely for diagnostics. Tests can retain expected values outside the role object.

## 12.2 Participant

Participant state remains minimal:

```text
receive tunnel ID
next router/tunnel
layer/IV keys
locked previous peer
bounded replay window
expiry
```

Required:

```text
validate receive ID
lock/validate previous peer
check duplicate token
participant_forward
rewrite to next tunnel ID
emit next router
```

Participant must not parse Tunnel Message fragments.

## 12.3 OBEP

Required:

```text
validate receive ID
previous-peer/replay handling
participant_forward final hop
parse every post-zero fragment record
feed every fragment to reassembler
when complete:
  ROUTER -> emit standard I2NP to router
  TUNNEL -> wrap standard I2NP in TunnelGatewayMessage for gateway/tunnel
```

LOCAL at outbound endpoint may be rejected unless the current spec/reference demonstrates a required outbound use.

Do not discard records after the first record in a cell.

## 12.4 Inbound gateway

Input is an actual `TunnelGatewayMessage` targeting this gateway's receive tunnel.

Required:

```text
validate gateway tunnel ID
encode/obtain exact nested standard I2NP bytes
fragment using LOCAL delivery
fresh IV/padding per cell
participant_forward using IBGW key
rewrite to configured next tunnel
emit to configured next router
```

Do not fabricate the nested message ID/expiration as tunnel metadata. The standard I2NP message already contains canonical message metadata.

## 12.5 Inbound participant

Same bounded forwarding primitive as outbound participant.

No fragment parsing.

## 12.6 Local inbound endpoint

The local endpoint owns the creator-known inbound `EstablishedTunnel` containing only remote hops plus the explicit local receive tunnel ID.

Required:

```text
validate local receive tunnel
split received cell
creator inverse over remote inbound hops in reverse order
verify checksum
parse every fragment record
require LOCAL delivery on initial/unfragmented record
bounded reassembly
return exact reconstructed standard I2NP bytes
```

No synthetic local hop key is used.

---

# 13. Required deterministic acceptance trajectories

These are the closure evidence. They must call the production module APIs corrected by this pass.

## 13.1 Outbound two-hop ROUTER

```text
local OBGW
 -> Participant A
 -> OBEP B
 -> ROUTER target
```

Use a real standard encoded I2NP message that fits one TunnelData cell.

Require byte-for-byte reconstruction at OBEP and exact target router.

## 13.2 Outbound-to-inbound TUNNEL pair

```text
local outbound OBGW
 -> outbound Participant A
 -> outbound OBEP B
 -> TUNNEL delivery
 -> TunnelGatewayMessage
 -> inbound IBGW C
 -> inbound Participant D (optional but preferred)
 -> local inbound endpoint
```

The exact original standard I2NP bytes must emerge locally.

The test must verify the gateway router/tunnel ID selected by outbound TUNNEL delivery equals the inbound established tunnel's first remote IBGW router and receive tunnel ID.

## 13.3 Fragmented end-to-end

Use a standard I2NP message large enough to require multiple TunnelData cells.

Require:

- automatic first/follow-on fragmentation;
- fresh IV per cell;
- deliberately reorder fragment-bearing cells before endpoint reassembly where the role boundary permits;
- exact duplicate is harmless;
- conflicting duplicate drops only that partial;
- missing fragment expires under caller time;
- final byte sequence exactly equals original.

## 13.4 Negative/security trajectory

Cover at least:

```text
zero/unknown tunnel ID
wrong receive tunnel
wrong previous peer after lock
replayed duplicate token
expired tunnel
removed tunnel
malformed checksum
missing delimiter
reserved delivery type
reserved flag bit
zero fragment size
sequence > 63
message over maximum
reassembly capacity saturation
RNG failure
```

Every public malformed-input path must return a typed error or bounded rejection, not panic.

---

# 14. Dependency cleanup

The current pass added a workspace `crypto-common = "=0.1.7"` pin solely to force AES/CBC dependency resolution.

Do not remove it blindly. During correction:

1. run `cargo tree -d` and inspect why duplicate `crypto-common` versions appear;
2. verify whether the direct workspace pin is truly required after the final `aes`/`cbc` feature selection;
3. if not required, remove the artificial workspace dependency;
4. if required to compile the supported MSRV/dependency graph, retain it but document the exact reason in the status file.

Do not upgrade unrelated crypto stacks as part of this pass.

---

# 15. No ignored-test closure policy

Before declaring Plan 116 complete, run:

```bash
rg -n 'Plan 116 provisional scaffolding|#\[ignore' crates/i2pr-tunnel/src
```

Required result for the Plan 116 provisional marker:

```text
0 matches
```

Do not make CI green by ignoring, deleting, weakening, or feature-gating the failing Plan 116 correctness tests.

If a test's original assumption is proven contrary to the official specification, replace it with a test that freezes the correct normative behavior and document the exact specification reason in the commit/status record.

---

# 16. Validation command set

Run in this order while correcting:

```bash
cargo fmt --all --check

cargo test --locked -p i2pr-tunnel --lib -- --ignored
cargo test --locked -p i2pr-tunnel --lib

cargo test --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo check --locked --workspace --all-targets
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

After all Plan 116 `#[ignore]` markers are removed, the `--ignored` selector is expected to find no provisional Plan 116 tests; record that fact rather than treating “0 tests” as a substantive pass.

Do not edit the historical NTCP2 harness merely because one of its boundary scripts reports an unrelated pre-existing environment condition.

---

# 17. Required closure status

Update `plans/116-status.md` only after implementation validation.

The only successful terminal state for this pass is:

```text
plan_116 = passed-local-tunnel-data-plane
```

The status file must record at least:

```text
source_commit
plan_116_provisional_ignored_tests = 0
wire_unfragmented_roundtrip
wire_fragmented_roundtrip
wire_checksum_rule
layer_one_hop_roundtrip
layer_multihop_outbound_roundtrip
layer_multihop_inbound_roundtrip
obep_forward_transform
production_rng_injected
real_registrar_pool_insertion
inbound_reply_path_first_hop_id
reassembly_out_of_order
reassembly_expiry
reassembly_aggregate_bound
outbound_router_trajectory
outbound_to_inbound_tunnel_trajectory
fragmented_e2e_trajectory
workspace_tests
workspace_clippy
workspace_docs
boundary_scripts
plan_117 = unblocked-next
```

Do not claim Q1, Q2, live mixed-router TunnelData, or public-network readiness.

---

# 18. Explicit acceptance criteria

Plan 116 may close only when **all** of the following are true:

1. No `Plan 116 provisional scaffolding` test remains ignored.
2. All formerly ignored wire/crypto/role tests are active and green, with any spec-invalid test replaced by a normative equivalent rather than deleted.
3. Checksum is `SHA256(post-zero-record-bytes || IV)[0..4]` and excludes padding and zero delimiter.
4. Parser verifies checksum without treating checksum+padding as a four-byte field.
5. Unfragmented records set fragmented bit 0 and omit Message ID.
6. Fragmented first records set fragmented bit 1 and include nonzero Message ID.
7. Follow-ons use sequence 1..63 and correct last flag.
8. Complete-message automatic fragmentation works through the current 62,708-byte bound.
9. Padding is random nonzero data from an injected CSPRNG with no deterministic fallback pattern.
10. Every emitted TunnelData cell gets a fresh CSPRNG-sourced IV.
11. No production-facing zero-only or deterministic RNG implements `CryptoRng`.
12. Participant forward AES remains ECB-ENC/CBC-ENC/ECB-ENC.
13. Creator inverse is ECB-DEC/CBC-DEC/ECB-DEC.
14. OBEP applies the forward transform, not creator inverse.
15. One-hop AES round trip is exact.
16. Multi-hop outbound creator preprocessing plus remote forward transforms is exact.
17. Multi-hop inbound remote forward transforms plus creator inverse is exact.
18. Inbound established remote-hop state contains IBGW + participant(s), not a synthetic local endpoint hop.
19. Optional next-hop state uses typed `Option`/equivalent rather than fake `u32::MAX` or zero-hash sentinels.
20. Successful short-build state can move established material exactly once.
21. `ShortBuildRegistrar::admit()` no longer fabricates slot 0.
22. Successful inbound/outbound builds insert real secret-bearing pool entries.
23. Pool duplicate/full/failure paths do not leak orphan secret entries.
24. Inbound reply path returns first remote IBGW router + first remote IBGW receive tunnel ID.
25. Removal/failure/expiry makes established secret state unavailable and drops it.
26. Reassembly supports out-of-order valid fragments.
27. Identical duplicate fragments are idempotent.
28. Conflicting duplicates invalidate only the affected partial message.
29. Reassembly has functional caller-time expiry.
30. Reassembly enforces concurrent-message, per-message-byte, aggregate-byte, and fragment-count bounds before state can exceed them.
31. Endpoint roles consume all records in a Tunnel Message rather than only the first.
32. Outbound gateway supports one-or-more TunnelData cells from a complete standard I2NP message.
33. Deterministic outbound ROUTER trajectory reconstructs exact original standard I2NP bytes.
34. Deterministic outbound TUNNEL -> inbound tunnel trajectory reconstructs exact original standard I2NP bytes locally.
35. Fragmented end-to-end trajectory reconstructs exact original bytes after reordering.
36. Wrong previous peer, duplicate token, malformed wire, expired tunnel, and missing tunnel fail closed.
37. No raw layer/IV/reply/garlic keys or plaintext/ciphertext payloads appear in `Debug` or error formatting.
38. `i2pr-tunnel` remains runtime-neutral and transport-neutral.
39. Full required workspace checks/tests/clippy/docs are green, aside from any precisely documented pre-existing historical harness condition unrelated to Plan 116.
40. Plan 117 remains blocked until criteria 1–39 are all satisfied.

---

# 19. Handoff rule

If this correction passes, update the handoff to:

```text
plan_116 = passed-local-tunnel-data-plane
plan_117 = ready-for-planning-or-execution
```

Then—and only then—move back to the smallest available real external lane for Plan 117.

If one of the local protocol corrections still fails, do **not** create another broad plan. Record the exact failing criterion/stage in `plans/116-status.md` and continue correcting Plan 116 until the local data-plane contract is complete.

The project must not return to external verification while its own local TunnelData path is knowingly incomplete.
