# Plan 117 corrective closure — exploratory NetDB routing, activation ownership, transport framing, and native reference

## Status

- **Ready for execution**.
- Date: **2026-08-18**.
- Corrective plan for [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md).
- Current implementation floor: `1608f5e5be3d2003b82340fb0293776087c3672c`.
- Predecessor Plan 116: **closed** (`passed-final-local-closure`).
- Current Plan 117 implementation: Phases A–F partially landed, but **not closure-safe**.
- Handoff authority after this file lands: [`117-handoff.md`](117-handoff.md).
- Status authority after this file lands: [`117-status.md`](117-status.md).
- Pinned independent reference remains upstream Emissary revision `9b43484a21d5a1291c4881cdae62a36c527f8c0f`, `emissary-core 0.4.0`.

This is a narrow corrective closure pass over the existing Plan 117 implementation. It is **not** a new architecture, another Milestone 3 interoperability program, or a reason to reopen Plan 116.

---

# 1. Objective

Correct the concrete composition defects found after the first Plan 117 A–F implementation, then finish the two mandatory closure checkpoints that were not executed:

```text
correct floodfill ROUTER delivery target
 -> correct transport-facing TunnelData I2NP framing
 -> preserve reply-path metadata across one-shot secret activation
 -> derive readiness from real activated roles
 -> Phase G full all-i2pr production-seam trajectory
 -> Phase H pinned Emissary in-process native mixed-router checkpoint
 -> Phase I authenticated transport classification only if an existing lane is runnable
 -> synchronized Plan 117 closure authority
```

The final product path must be honest and executable:

```text
RouterInfoLookup selects floodfill peer F for requested key K
 -> DatabaseLookup body keeps key=K and reply path=inbound exploratory tunnel
 -> outbound tunnel ROUTER delivery targets F, not K
 -> local OBGW emits TunnelData body/bodies
 -> transport boundary encodes each TunnelData as a complete short-transport I2NP message
 -> DeliveryRequest targets the outbound first-hop router
 -> OBEP recovers the nested standard-header DatabaseLookup
 -> floodfill handles the request
 -> DatabaseStore/DatabaseSearchReply returns through the declared inbound tunnel
 -> activated local inbound endpoint recovers the nested standard-header response
 -> existing NetDB validator/store handles the response
```

Publication must use the same corrected outer TunnelData framing and must route the `DatabaseStore` to the selected floodfill, while the stored RouterInfo key remains the local router hash.

---

# 2. Why this corrective pass exists

The first Plan 117 implementation landed useful composition surfaces, but four product-level defects remain and Phases G/H/I were not completed.

Do **not** discard the working A–F code. Correct it in place.

## 2.1 Retain these successful first-pass results

Keep the following unless a focused correction requires a small API adjustment:

```text
LookupAction::SendDatabaselookup owns DatabaseLookupMessage
PublicationAttemptRecord owns DatabaseStoreMessage
RouterInfo publication produces canonical gzip payload
RouterInfoLookup::handle_pending_after_path exists
NetDbSeam no longer returns the old post-path placeholder
DataPlaneRegistry exists as the runtime owner of activated local roles
outbound_lookup.rs composes nested standard-header NetDB messages through OutboundGatewayRole
inbound_dispatch.rs dispatches local inbound TunnelData to LocalInboundEndpointRole
unknown inbound tunnel IDs fail closed
normal daemon NTCP2 remains disabled
```

The correction is about routing identity, framing ownership, activation lifetime semantics, actual production-seam closure, and the independent native checkpoint.

---

# 3. Normative and repository framing facts

These facts are closure requirements, not suggestions.

## 3.1 RouterInfo lookup targets a floodfill peer, not the lookup key as a router

Official I2P Network Database documentation states that `DatabaseLookup` requests are sent to floodfill routers through outbound exploratory tunnels and replies return through inbound exploratory tunnels.

The two distinct RouterHashes are:

```text
lookup key K      = RouterInfo hash being requested
selected peer F   = floodfill router selected to answer the request
```

They are frequently different and must remain different in tests.

Correct outbound delivery semantics:

```text
DatabaseLookup.key                 = K
LookupAction::SendDatabaselookup.peer = F
Tunnel Message ROUTER destination = F
DeliveryRequest target            = outbound first hop P1
```

Incorrect first-pass behavior:

```text
Tunnel Message ROUTER destination = K
```

That bypasses the selected floodfill and is a hard Phase D defect.

Primary reference:

- <https://i2p.net/en/docs/overview/network-database/>

## 3.2 TunnelData has a 1028-byte I2NP body

The Tunnel Message and I2NP specifications define the TunnelData body as:

```text
Tunnel ID       4 bytes, nonzero
IV             16 bytes
encrypted data 1008 bytes
-------------------------
body total      1028 bytes
```

The body is I2NP message type 18 (`TunnelData`).

Primary references:

- <https://i2p.net/en/docs/specs/tunnel-message/>
- <https://i2p.net/en/docs/specs/i2np/>

## 3.3 Nested I2NP inside a tunnel uses the standard header

The `DatabaseLookup` placed into tunnel preprocessing is a nested I2NP message and therefore uses the standard 16-byte I2NP header.

The existing helper:

```rust
I2npMessage::new_standard(...)
    -> encode_standard_to_vec(...)
```

is correct for the nested NetDB message.

Do not change this part merely to fix the outer transport representation.

## 3.4 NTCP2/SSU2 authenticated transport uses the short-transport I2NP header

The I2NP specification defines a 9-byte short header for NTCP2/SSU2:

```text
type        1 byte
message ID  4 bytes
expiration  4 bytes, seconds
```

The repository already implements this exact boundary:

```rust
I2npMessage::new_short_transport(...)
I2npMessage::encode_short_transport_to_vec(...)
I2npMessage::decode_short_transport(...)
```

and `i2pr-transport-ntcp2::I2npMessageBlock::new()` explicitly requires an already-encoded NTCP2 short-header message.

Therefore the first-pass helper that puts raw `TunnelDataMessage` body bytes into `EncodedI2npMessage` is incorrect.

Correct hierarchy:

```text
DatabaseLookup
  standard 16-byte header
    -> tunnel preprocessing / encryption
      -> TunnelData body (1028 bytes)
        -> TunnelData I2NP type 18 short-transport header
          -> EncodedI2npMessage
            -> DeliveryRequest
              -> authenticated link block
```

Do not add a second ad-hoc I2NP framing implementation.

## 3.5 TunnelData outer message IDs are hop-local

The I2NP specification notes that a TunnelData message receives a new message ID at each hop.

For the local creator -> first-hop transport boundary in this plan:

- generate a fresh outer TunnelData I2NP message ID for each cell;
- do **not** reuse the nested DatabaseLookup/DatabaseStore message ID as the outer TunnelData message ID merely for convenience;
- use the injected production CSPRNG already available to the composition function;
- deterministic seeded RNG remains acceptable in tests.

Expiration is encoded in seconds for the short-transport header and must be checked for overflow/conversion failure.

---

# 4. Defect inventory and closure classification

## C1 — wrong ROUTER delivery target for DatabaseLookup

Current first-pass shape:

```rust
LookupAction::SendDatabaselookup { message, .. }
...
TunnelDelivery::Router { router: message.key }
```

Required:

```rust
LookupAction::SendDatabaselookup { peer, message, .. }
...
TunnelDelivery::Router { router: peer_hash }
```

while preserving:

```rust
message.key == requested NetDB key
```

Classification: **protocol/product routing defect; blocking**.

## C2 — raw TunnelData body is passed as `EncodedI2npMessage`

Current first-pass helper serializes only:

```text
TunnelId || 1024-byte data
```

and calls `EncodedI2npMessage::new()`.

Required:

```rust
let outer = I2npMessage::new_short_transport(
    fresh_outer_message_id,
    expiration_seconds,
    I2npBody::TunnelData(Box::new(cell)),
)?;
let encoded = outer.encode_short_transport_to_vec(...)?;
let payload = EncodedI2npMessage::new(encoded)?;
```

Classification: **authenticated-link framing defect; blocking**.

## C3 — `ExploratoryPool::activate()` destroys reply-path metadata

Current first-pass implementation removes the complete `TunnelEntry` from the pool to transfer `EstablishedMaterial`.

But the same pool is the authority for:

```text
select_inbound_reply_path()
registration lifetime
capacity accounting
duplicate tunnel identity
expiry/failure bookkeeping
```

After activation, an inbound tunnel therefore disappears from reply-path selection even though its runtime endpoint role is active.

Required invariant:

```text
secret-bearing EstablishedMaterial transfers once
public registration/routing metadata remains until expiry/failure/removal
```

Classification: **ownership/lifetime defect; blocking**.

## C4 — `ActivationError::AlreadyActivated` is not reachable as designed

Because activation removes the entry, a second call naturally becomes `UnknownSlot` rather than `AlreadyActivated`.

Correct activation state must remain observable without retaining duplicate secrets.

Classification: **state-machine invariant defect; coupled to C3**.

## C5 — outbound readiness is a manually writable boolean

`NetDbSeam::set_outbound_role_available(true)` can report `LookupReadyForTunnelDispatch` even when the role registry contains no usable outbound role.

Required:

```text
readiness derives from actual DataPlaneRegistry state
```

A stale or caller-invented readiness bit must not authorize dispatch.

Classification: **composition consistency defect; blocking Phase G closure**.

## C6 — Phase G is not executed

The current tests construct `EstablishedTunnel` values directly for several A–F unit/integration checks.

Plan 117 requires one terminal deterministic creator-side test whose established material originates from successful production short-build state machines.

Classification: **missing mandatory closure evidence**.

## C7 — Phase H was incorrectly reclassified as requiring rootless/Multipass

The current status document points Phase H at the historical NTCP2 harness and declares the native checkpoint blocked by Plan 046 / Multipass.

That contradicts the Plan 117 design.

Phase H is the pinned **in-process Emissary native** checkpoint. It must use a temporary checkout and a test-only reference patch, just as the Plan 115 Q0 native seam did.

Rootless namespaces, Multipass, Docker, and a permanent Python harness are forbidden for 117-N.

Classification: **planning/execution drift; blocking native closure**.

## C8 — Phase I authenticated transport remains legitimately separate

117-X may still be unavailable on this host. That is not a Plan 117-L/117-N failure.

Classification: **external evidence gap, not a local/native product defect**.

---

# 5. Hard scope lock

This pass may modify the existing Plan 117 product surfaces only as needed to close C1–C8.

Expected production files:

```text
crates/i2pr-daemon/src/outbound_lookup.rs
crates/i2pr-daemon/src/netdb_seam.rs
crates/i2pr-daemon/tests/netdb_integration.rs
crates/i2pr-tunnel/src/pool.rs
crates/i2pr-tunnel/src/data_plane_registry.rs
crates/i2pr-proto/...                    # only if a tiny reusable framing helper is genuinely missing
```

Possible narrow supporting changes:

```text
crates/i2pr-tunnel/src/short*.rs          # test access only if production material cannot otherwise be driven
crates/i2pr-netdb/...                     # only for Phase G response plumbing proven missing
crates/i2pr-testkit/...                   # only if existing testkit is the cleanest home for production-seam fixtures
```

Completion documentation:

```text
plans/117-status.md
plans/117-handoff.md
plans/115-117-external-delivery-to-live-netdb-roadmap.md
README.md
AGENTS.md
docs/architecture/i2pr-daemon.md
docs/architecture/i2pr-netdb.md
specs/support.toml
```

Do not update closure documentation to `passed` until the terminal acceptance section is actually satisfied.

Forbidden:

```text
normal-daemon NTCP2 activation
NTCP2 protocol correction unrelated to the short-header ownership boundary
SSU2 implementation work
rootless namespace engineering
Multipass or VM recovery engineering
Docker orchestration
new Python interoperability harness
public I2P participation
new Java/i2pd adapter
reference implementation matrix
LeaseSet/client tunnel implementation
general garlic subsystem
streaming
SAM
I2CP
HTTP/SOCKS proxy work
new generic router dispatcher
rewrite of Plan 116 AES/TunnelData/fragment machinery
```

If a Plan 116 primitive fails a new independent native test, localize that exact defect. Do not reopen Plan 116 by default.

---

# 6. Required execution order

Execute in this order. Do not jump directly to Emissary.

```text
117-C1  correct floodfill delivery identity
117-C2  correct transport-facing TunnelData short I2NP framing
117-C3  correct pool activation metadata/secret ownership
117-C4  derive composition readiness from DataPlaneRegistry
117-C5  add focused regression tests for C1-C4
117-G   pass the all-i2pr production-seam terminal trajectory
117-H   pass the pinned Emissary in-process mixed-router checkpoint
117-I   classify authenticated transport using an existing qualified lane only
117-J   synchronize closure authority
```

Each phase is a gate for the next.

---

# 7. 117-C1 — correct floodfill delivery identity

## 7.1 Change the outbound lookup composition

`compose_outbound_lookup()` must use both fields from the action:

```rust
LookupAction::SendDatabaselookup {
    peer,
    message,
    ..
}
```

Convert `peer` into the protocol `Hash` used by the tunnel delivery instruction.

Build:

```rust
TunnelPayloadHeader {
    delivery: TunnelDelivery::Router {
        router: floodfill_peer_hash,
    },
    ...
}
```

Do not modify:

```rust
message.key
message.from
message.reply_tunnel_id
message.lookup_type
```

## 7.2 Remove misleading naming

Do not call `message.key` or an argument derived from it `target_floodfill`.

Use explicit names:

```text
lookup_key
selected_floodfill
outbound_first_hop
```

The three identities must be distinguishable in code review and tests.

## 7.3 Required focused tests

Add at least:

### `outbound_lookup_routes_to_selected_floodfill_not_lookup_key`

Use deliberately distinct hashes:

```text
lookup_key          = 0x11...11
selected_floodfill  = 0x22...22
outbound_first_hop  = 0x33...33
```

Require:

```text
DatabaseLookup.key == 0x11...11
OBEP/router delivery target == 0x22...22
DeliveryRequest.target == 0x33...33
```

The test must fail against the current `message.key` implementation.

### `outbound_lookup_peer_and_key_may_differ`

Require a valid dispatch when `peer != message.key`.

### `publication_routes_to_selected_floodfill`

Keep publication target semantics explicit and independently test that the local RouterInfo key is not substituted as the ROUTER destination when a different floodfill is selected.

---

# 8. 117-C2 — correct outer TunnelData framing

## 8.1 Delete the raw-body authenticated-link handoff

Remove or stop using the current manual helper equivalent to:

```rust
fn encode_tunnel_data_cell(cell: &TunnelDataMessage) -> Vec<u8> {
    tunnel_id || data
}
```

Raw body bytes may be useful inside codec tests, but they are not a valid `EncodedI2npMessage` for the NTCP2/SSU2 short-header boundary.

## 8.2 Use the existing typed codec

For every outbound cell emitted by `OutboundGatewayRole::forward_cells`:

1. move/clone the typed `TunnelDataMessage` as appropriate;
2. create `I2npBody::TunnelData`;
3. generate a fresh outer TunnelData message ID from the injected CSPRNG;
4. convert the expiration to checked short-header seconds;
5. construct `I2npMessage::new_short_transport`;
6. call `encode_short_transport_to_vec`;
7. wrap the complete short-header bytes in `EncodedI2npMessage`;
8. create `DeliveryRequest` targeting the outbound first hop.

Preferred narrow helper:

```rust
fn encode_transport_tunnel_data<R: CryptoRng + RngCore>(
    cell: TunnelDataMessage,
    expiration_ms: u64,
    rng: &mut R,
) -> Result<EncodedI2npMessage, OutboundLookupError>
```

Exact spelling may differ.

Do not create an NTCP2-specific dependency in `i2pr-daemon` merely to perform I2NP short framing. `i2pr-proto` already owns the codec.

## 8.3 Outer message ID requirements

For a multi-cell fragmented dispatch:

```text
outer TunnelData message IDs are independently generated per cell
```

Test this when more than one cell is produced.

A zero random value is protocol-legal for an I2NP message ID unless repository policy already rejects it; do not invent a nonzero rule without an existing requirement. If the repository uses a stronger local invariant, document and test it consistently.

## 8.4 Expiration conversion

The short transport header stores expiration as unsigned seconds.

Use checked conversion:

```text
expiration_seconds = expiration_ms / 1000
```

and reject values that do not fit the repository's `u32` short-expiration representation.

Do not silently wrap.

## 8.5 Required framing tests

### `outbound_lookup_delivery_is_complete_short_transport_tunneldata`

For every `DeliveryRequest`:

```rust
let decoded = I2npMessage::decode_short_transport(
    delivery.message_bytes(),
    MAX_I2NP_MESSAGE_BYTES,
)?;
```

Require:

```text
decoded.body is TunnelData
TunnelData tunnel_id == outbound first-hop receive tunnel ID
TunnelData body == exact cell emitted by OutboundGatewayRole
```

### `raw_1028_byte_tunneldata_body_is_not_transport_message`

A regression test should demonstrate that the transport-facing helper does not return exactly the 1028-byte TunnelData body and that a complete short transport message is larger by the existing short-header size.

### `nested_database_lookup_remains_standard_header`

After applying the outbound tunnel layers and the simulated remote hop/OBEP path, the recovered nested payload must decode with `decode_standard`, not `decode_short_transport`, and must be the exact `DatabaseLookupMessage` from the lookup action.

### `fragmented_dispatch_uses_distinct_outer_message_ids`

Force multiple TunnelData cells and assert that each short-transport header has its own generated message ID.

### `publication_delivery_is_complete_short_transport_tunneldata`

Apply the same outer framing requirements to RouterInfo publication.

---

# 9. 117-C3 — preserve public pool metadata across one-shot activation

## 9.1 Required ownership model

The pool must stop treating `TunnelEntry` as an indivisible metadata+secret object that disappears on activation.

Preferred representation:

```rust
struct TunnelEntry {
    registration: TunnelRegistration,
    routing: PublicTunnelRouting,
    material: MaterialState,
}

enum MaterialState {
    Available(EstablishedMaterial),
    Activated,
}
```

Exact naming may differ.

`PublicTunnelRouting` contains only non-secret data required after activation, for example:

```text
first_hop_router
first_hop_receive_tunnel
inbound reply gateway/tunnel when direction is inbound
local inbound receive tunnel if needed for registry cleanup
```

Do not place `LayerKeys` or other secret-bearing state in this public metadata.

## 9.2 Derive public routing metadata before secret transfer

At `register_*_with_material()` time, copy the required non-secret routing fields from `EstablishedMaterial` into the entry.

Then activation may move `EstablishedMaterial` exactly once while leaving registration/routing metadata in the pool.

## 9.3 Correct `activate(slot)` semantics

Required behavior:

```text
unknown slot                      -> UnknownSlot
real material available           -> move EstablishedTunnel out; entry remains; state=Activated
second activation of same slot    -> AlreadyActivated
placeholder/test-only material    -> PlaceholderMaterial or test-only equivalent
```

Activation must not decrement pool registration count.

Activation must not make an inbound reply path disappear.

Activation must not allow duplicate registration of the same creator tunnel ID simply because the secrets moved to the runtime registry.

## 9.4 Reply-path selection after activation

`select_inbound_reply_path()` must use retained public routing metadata, not `EstablishedMaterial`.

Required regression:

```text
register real inbound material
 -> select_inbound_reply_path() == Some(P)
 -> activate(slot)
 -> select_inbound_reply_path() == Some(P)
```

until expiry/failure/removal.

## 9.5 Expiry and registry cleanup

The pool remains lifetime authority.

When `advance_time()` or `mark_failed()` evicts a registration, the runtime role must also be removable using the same `TunnelSlot`.

The current registry keys inbound roles by local receive tunnel ID only. Add the minimum bookkeeping required so an evicted pool slot can clean up either direction without a global search.

Recommended registry shape:

```text
outbound: TunnelSlot -> OutboundGatewayRole
inbound:  local_receive_tunnel -> LocalInboundEndpointRole
inbound_slot_index: TunnelSlot -> local_receive_tunnel
```

or an equivalent single binding type.

Add one API such as:

```rust
remove_slot(slot: TunnelSlot)
```

that removes public first-hop metadata and whichever role is bound to the slot.

Do not add an unbounded registry or background sweeper.

## 9.6 Required activation tests

Add at least:

```text
activate_preserves_inbound_reply_path
activate_preserves_registration_count
activate_preserves_duplicate_identity
second_activation_returns_already_activated
activated_entry_expires_normally
activated_entry_mark_failed_removes_registration
pool_eviction_can_remove_matching_registry_role
activation_transfers_secret_material_once
no_layer_key_clone_is_introduced_by_activation
```

The first three must fail against the current remove-entry implementation.

---

# 10. 117-C4 — derive readiness from the real role registry

## 10.1 Remove the sticky caller-set readiness bit

Do not keep:

```rust
set_outbound_role_available(true)
```

as the production authority for `LookupReadyForTunnelDispatch`.

The runtime already has a bounded `DataPlaneRegistry`; use it.

Preferred options, in order:

1. `NetDbSeam::composition_outcome(&DataPlaneRegistry)`;
2. a narrow read-only trait implemented by `DataPlaneRegistry` if direct coupling is undesirable;
3. an explicit role-selection result passed into the coordinator for the current operation.

Do not introduce a second registry or a cached boolean that can diverge.

## 10.2 Readiness means usable role, not merely map non-empty

If roles have expiration checks, derive readiness from at least one role usable at the current supplied time.

If `composition_outcome()` currently lacks a clock argument, add the narrow deterministic `now_ms` argument rather than reading wall-clock time internally.

Recommended contract:

```rust
composition_outcome(registry: &DataPlaneRegistry, now_ms: u64)
```

with:

```text
no inbound reply path                 -> NeedInboundExploratory
inbound path but no usable outbound   -> NeedOutboundExploratory
both present                          -> LookupReadyForTunnelDispatch
```

## 10.3 Required readiness tests

```text
registry_empty_never_reports_lookup_ready
activated_outbound_role_enables_lookup_ready
expired_outbound_role_does_not_enable_lookup_ready
removing_outbound_slot_returns_to_need_outbound
caller_cannot_force_ready_without_registry_role
```

---

# 11. 117-C5 — focused A–F regression suite

Before Phase G, all corrected A–F contracts must be independently green.

Minimum regression matrix:

| Area | Required proof |
|---|---|
| Lookup identity | lookup key and selected floodfill remain distinct |
| Tunnel delivery | ROUTER destination equals selected floodfill |
| First-hop transport | DeliveryRequest target equals outbound first hop |
| Outer framing | DeliveryRequest payload decodes as short-transport TunnelData |
| Nested framing | recovered NetDB message decodes as standard I2NP |
| Publication | selected floodfill distinct from stored local RouterInfo key |
| Activation | reply path survives secret transfer |
| Activation | second activation is `AlreadyActivated` |
| Lifetime | activated registration still expires and can remove runtime role |
| Readiness | derived from actual usable role registry |

Do not proceed to Phase G with a known failure in this matrix.

---

# 12. Phase G — mandatory all-i2pr production-seam terminal trajectory

Phase G is the local closure proof. It must stop using directly fabricated creator-side `EstablishedTunnel` objects as the source of the tunnel roles under test.

## 12.1 Production material requirement

Both local creator tunnels must originate from the production short-build path:

```text
ShortBuildPath
 -> ShortBuildStateMachine::prepare
 -> canonical short-build record processing
 -> accepted hop replies
 -> ShortBuildStateMachine reaches Established
 -> ShortBuildRegistrar::admit_established_machine
 -> ExploratoryPool entry with real EstablishedMaterial
 -> ExploratoryPool::activate
 -> DataPlaneRegistry role activation
```

No placeholder `EstablishedMaterial`.

No direct `EstablishedTunnel::new()` for the creator-side roles being validated.

A test-only remote-hop simulator may construct reference-side role material from the same accepted build records/keys, but the local creator ownership path must remain production.

## 12.2 Minimum local topology

Use a small deterministic topology that exercises the real routing distinction:

```text
outbound creator O
 -> outbound participant or direct OBEP reference role
 -> OBEP
 -> floodfill peer F

inbound reply path
F / response source
 -> IBGW
 -> optional participant
 -> local creator endpoint I
```

A one-hop outbound and one/two-hop inbound topology is acceptable if it exercises every composition boundary under test.

Do not add hops merely to increase test complexity.

## 12.3 Phase G lookup sequence

Required sequence:

1. Create a validated local RouterInfo store containing:
   - selected floodfill `F`;
   - target RouterInfo `K` available to the simulated responder;
   - enough distinct identities to prevent accidental equality.
2. Build real inbound and outbound exploratory tunnels through production short-build state machines.
3. Register and activate both through the corrected pool/registry seam.
4. Confirm the inbound reply path still exists **after activation**.
5. Start `RouterInfoLookup` for key `K`.
6. Require `LookupAction::SendDatabaselookup { peer = F, message.key = K }`.
7. Compose outbound lookup.
8. Decode the first-hop `DeliveryRequest` as short-transport `TunnelData`.
9. Drive the TunnelData through the reference remote outbound role(s) until the OBEP exposes the nested standard I2NP message.
10. Require the exposed standard message is `DatabaseLookup` with:
    - key `K`;
    - reply gateway equal to the real inbound IBGW router;
    - reply tunnel equal to the real inbound IBGW receive tunnel;
    - RouterInfo lookup type.
11. Construct the matching response using existing typed `DatabaseStoreMessage`/codec paths. Phase G does not need to invent a full local floodfill server if none exists; the independent Phase H owns native floodfill handling.
12. Route the response into the established inbound tunnel using production inbound roles.
13. Feed returned TunnelData through `dispatch_inbound_tunnel_data`.
14. Route the recovered `DatabaseStore` into `RouterInfoLookup`.
15. Require:

```text
lookup final state == Success
validated RouterInfo key == K
RouterInfoStore contains K
exact response passed existing decompression/validation rules
```

## 12.4 Wrong-target negative path

In the same test module, prove a `DatabaseStore` for a different key does not complete the lookup.

## 12.5 DatabaseSearchReply path

Prove a valid `DatabaseSearchReply` returned through the same inbound tunnel preserves the iterative lookup state rather than falsely completing with success.

## 12.6 Publication local trajectory

Use a real activated outbound tunnel and `PublicationCoordinator`:

```text
local signed RouterInfo
 -> canonical gzip DatabaseStoreMessage
 -> selected floodfill F2
 -> compose_outbound_publication
 -> short-transport TunnelData DeliveryRequest to first hop
 -> remote OBEP exposes nested standard DatabaseStore
```

Require:

```text
ROUTER destination == F2
DatabaseStore.key == local router hash
compressed payload decompresses to exact local RouterInfo bytes
```

## 12.7 Phase G acceptance label

Only after these pass:

```text
117-G = passed-all-i2pr-production-seam-netdb
117-L = passed
```

Do not infer independent interoperability from Phase G.

---

# 13. Phase H — pinned Emissary in-process native mixed-router checkpoint

This phase corrects the execution drift in the current `117-status.md`.

## 13.1 Phase H does not require rootless/Multipass

Do **not** use or create:

```text
tests/integration/ntcp2/harness/test_pinned_native_mixed_router_emissary_checkpoint.rs
```

as a required execution lane unless that file is merely a small Rust-native test wrapper with no namespace/process dependency. The current status description of a rootless/Multipass requirement is superseded by this corrective plan.

The native reference checkpoint is intentionally **in-process**.

## 13.2 Pinned reference

Use exactly:

```text
repository = https://github.com/eepnet/emissary.git
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package    = emissary-core 0.4.0
```

Do not silently update Emissary during this pass.

## 13.3 Execution method

Use a temporary checkout outside the i2pr repository.

Recommended:

```bash
set -euo pipefail
I2PR_ROOT="$(git rev-parse --show-toplevel)"
WORK="$(mktemp -d)"
git clone https://github.com/eepnet/emissary.git "$WORK/emissary"
git -C "$WORK/emissary" checkout --detach 9b43484a21d5a1291c4881cdae62a36c527f8c0f
ln -s "$I2PR_ROOT" "$WORK/emissary/i2pr-under-test"
```

Add only test/dev dependencies required to call i2pr production crates from Emissary's existing test module.

Do not vendor the checkout.

Do not commit the reference patch to i2pr.

## 13.4 Record the patch digest before deletion

Mandatory sequence:

```bash
git -C "$WORK/emissary" diff --binary > "$WORK/emissary-plan117.patch"
sha256sum "$WORK/emissary-plan117.patch"
```

Record the final SHA-256 in `plans/117-status.md` **before deleting the temporary checkout**.

This explicitly avoids the Plan 115 evidence defect where the temporary patch digest was not recorded before deletion.

## 13.5 Native topology

Minimum useful topology:

```text
outbound: i2pr creator -> Emissary OBEP
inbound:  Emissary IBGW -> Emissary Participant -> i2pr local endpoint
```

If the exact pinned test utilities make a one-participant inbound topology significantly simpler, one Emissary IBGW plus one Emissary participant is preferred over adding unnecessary routers.

Use Emissary's existing `TestTransitTunnelManager<MockRuntime>` / native short-build production handler seams identified in Plan 115.

## 13.6 Build tunnels through native Emissary short-build processing

The reference test must not simply import i2pr's final `EstablishedTunnel` and trust it.

For each Emissary remote hop:

```text
i2pr generates production STBM/request record
 -> Emissary native handler locates/decrypts/parses/admit the record
 -> Emissary starts its production transit role
 -> returned build message/reply is fed to the next native hop or back to i2pr as appropriate
```

A test-only Q2 decapsulation seam for the outbound garlic-wrapped OTBRM remains permitted if it delegates to pinned Emissary crypto/test utilities and is explicitly recorded as:

```text
Q2_external_return_established = not-proven
```

Do not implement the general i2pr garlic subsystem in Plan 117 merely to eliminate that test-only boundary.

## 13.7 Native lookup path

After the i2pr creator has usable outbound and inbound material:

1. start an i2pr RouterInfo lookup for key `K`;
2. require selected floodfill `F` is distinct from `K`;
3. compose corrected outbound lookup;
4. hand the complete first-hop short-transport TunnelData message to the reference boundary;
5. require Emissary production OBEP processing recovers the nested **standard-header** `DatabaseLookup`;
6. require:

```text
DatabaseLookup.key == K
ROUTER destination == F
DatabaseLookup.from == i2pr inbound gateway router hash
DatabaseLookup.reply_tunnel_id == i2pr inbound gateway receive tunnel
```

7. pass the lookup into Emissary's native floodfill NetDB handling;
8. require native `DatabaseStore` or expected `DatabaseSearchReply` behavior;
9. route the successful `DatabaseStore` response through the Emissary inbound gateway/participant roles;
10. feed resulting TunnelData to i2pr's production `LocalInboundEndpointRole` / daemon dispatch;
11. require i2pr validates and stores the target RouterInfo and the lookup reaches success.

## 13.8 Native publication path

Publish i2pr's local RouterInfo through the corrected outbound exploratory path.

Require:

```text
Emissary OBEP accepts TunnelData
 -> nested standard DatabaseStore targets selected floodfill
 -> RouterInfoCompressed payload is accepted by Emissary floodfill
```

Preferred independent observation:

```text
read-after-write lookup through Emissary native NetDB
```

If the pinned test API cannot expose a read-after-write query without modifying production Emissary, record the strongest native store acceptance stage available. Do not fake an observation.

## 13.9 Phase H stage taxonomy

Record the highest reached stage:

```text
h_reference_checkout_pinned
h_i2pr_inbound_build_generated
h_emissary_ibgw_build_accepted
h_emissary_participant_build_accepted
h_i2pr_inbound_established
h_i2pr_outbound_build_generated
h_emissary_obep_build_accepted
h_i2pr_outbound_established_test_q2_bypass
h_lookup_tunneldata_short_transport_encoded
h_emissary_obep_tunneldata_accepted
h_emissary_database_lookup_parsed
h_emissary_floodfill_lookup_handled
h_emissary_inbound_return_started
h_i2pr_database_store_recovered
h_i2pr_routerinfo_validated
h_i2pr_lookup_success
h_publication_tunneldata_accepted
h_publication_floodfill_accepted
h_publication_independently_observed
```

Failure labels:

```text
failed-reference-build
failed-emissary-short-build-role
failed-i2pr-outer-tunneldata-framing
failed-emissary-obep-tunneldata
failed-emissary-database-lookup-parse
failed-emissary-floodfill-response
failed-emissary-inbound-return
failed-i2pr-reference-routerinfo-validation
failed-emissary-publication-store
```

Do not collapse protocol failure and reference/tooling failure into one label.

## 13.10 Strict attempt budget

```text
1 baseline compile/test attempt
2 narrow corrections for temporary test-patch/API spelling/build integration
1 confirmation run
STOP
```

If Emissary production processing exposes one reproducible i2pr protocol defect:

```text
1 localized product correction
1 focused confirmation
STOP
```

Do not switch to i2pd or Java inside this pass.

## 13.11 Phase H acceptance label

Successful native lookup and publication checkpoint:

```text
117-N = passed-emissary-mixed-router-netdb
```

A reference build failure before native processing is not that label and must be reported honestly.

---

# 14. Phase I — authenticated transport classification

117-X remains a separate evidence level.

After 117-G and 117-H pass, inspect the repository's **existing** authenticated transport execution lanes.

Do not create a new lane.

## 14.1 Local framing precondition

Before any live probe, the corrected `EncodedI2npMessage` must already be accepted by the local NTCP2 `I2npMessageBlock` constructor/parser as a valid short-header I2NP message.

This local test is useful but does **not** count as Q1.

## 14.2 If an existing qualified lane is runnable

Run one bounded authenticated delivery probe sufficient to establish the highest stage reached.

Record separately:

```text
Q1_authenticated_transport
Q2_external_return_established
```

Do not infer one from the other.

## 14.3 If the host lane is unavailable

Record:

```text
117-X = deferred-host-lane-unavailable
Q1_authenticated_transport = deferred
Q2_external_return_established = deferred
```

and stop.

This is a valid Plan 117 local/native closure outcome.

Do not invoke rootless namespace, Multipass, Docker, VM, or Python-harness engineering to change this classification.

---

# 15. Phase J — closure authority and stale-document correction

After G/H/I are classified, synchronize all current authority.

At minimum update:

```text
plans/117-status.md
plans/117-handoff.md
plans/115-117-external-delivery-to-live-netdb-roadmap.md
README.md
AGENTS.md
docs/architecture/i2pr-daemon.md
docs/architecture/i2pr-netdb.md
specs/support.toml
```

The current `specs/support.toml` Plan 116 commentary is stale and must be brought forward to the real Plan 116 closure / Plan 117 status when this pass closes.

The current status/AGENTS material that claims the native Emissary checkpoint requires rootless/Multipass must be removed.

Do not rewrite historical plan files merely to make history look cleaner. Update current authority and mark superseded conclusions explicitly.

---

# 16. Required test inventory

Exact final identifiers may differ, but the test suite must make each invariant independently discoverable.

Recommended names:

## Routing/framing

```text
outbound_lookup_routes_to_selected_floodfill_not_lookup_key
outbound_lookup_peer_and_key_may_differ
outbound_lookup_delivery_is_complete_short_transport_tunneldata
nested_database_lookup_remains_standard_header
fragmented_dispatch_uses_distinct_outer_message_ids
publication_routes_to_selected_floodfill
publication_delivery_is_complete_short_transport_tunneldata
short_transport_expiration_overflow_fails_closed
```

## Activation/lifetime

```text
activate_preserves_inbound_reply_path
activate_preserves_registration_count
activate_preserves_duplicate_identity
second_activation_returns_already_activated
activated_entry_expires_normally
activated_entry_mark_failed_removes_registration
pool_eviction_can_remove_matching_registry_role
activation_transfers_secret_material_once
```

## Readiness

```text
registry_empty_never_reports_lookup_ready
activated_outbound_role_enables_lookup_ready
expired_outbound_role_does_not_enable_lookup_ready
removing_outbound_slot_returns_to_need_outbound
```

## Phase G

```text
plan117_all_i2pr_production_seam_routerinfo_lookup_success
plan117_all_i2pr_wrong_target_does_not_complete
plan117_all_i2pr_database_search_reply_continues
plan117_all_i2pr_publication_reaches_selected_floodfill
```

## Reference

The Emissary temporary test may use a reference-side identifier such as:

```text
i2pr_plan117_native_mixed_router_lookup_and_publication
```

Record the exact final identifier in `117-status.md`.

---

# 17. Security and privacy invariants

1. `DatabaseLookup.key` is the requested NetDB key; it is not automatically the destination router.
2. The tunnel ROUTER destination for a lookup is the selected floodfill peer.
3. The authenticated-link `DeliveryRequest.target` is the outbound first hop, not the floodfill and not the lookup key.
4. Nested NetDB I2NP uses the standard header; transport-facing TunnelData uses the correct transport-short header for NTCP2/SSU2.
5. Outer TunnelData message IDs are hop-local and freshly generated.
6. Direct `DatabaseLookup` transport to the floodfill remains forbidden as a substitute for the exploratory tunnel.
7. The inbound reply path remains selectable after its secret material is activated into the runtime role registry.
8. Layer keys are transferred once; no persistent key clone is introduced to preserve metadata.
9. Activated tunnels remain subject to the same expiry/failure/capacity rules as unactivated registered tunnels.
10. Runtime readiness derives from actual usable roles, not a sticky boolean.
11. Unknown inbound tunnel IDs continue to fail closed without allocating reassembly state.
12. RouterInfo decompression/validation/store limits remain unchanged unless an independent protocol defect requires a narrow correction.
13. Raw tunnel plaintext, ciphertext, private keys, layer keys, and Noise secrets must not enter Plan 117 evidence documents.
14. Reference evidence records hashes/stages/types/lengths/results, not secrets.
15. `ReplyEncryption::None` remains scoped to the RouterInfo-only integration checkpoint and must not become future LeaseSet policy.
16. Normal-daemon NTCP2 remains disabled and non-advertised.

---

# 18. Validation order

Run focused tests after each corrective phase, then the workspace bar.

Minimum local validation:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ntcp2 --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-daemon --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
git diff --check
```

If a known pre-existing repository-wide lint/environment failure remains, record its exact pre-existing baseline and prove this pass introduces no new instance. Prefer fixing tiny unrelated pre-existing lint defects only if the fix is trivial and does not expand scope.

Do not edit historical interoperability scripts merely to make the Plan 117 pass green.

Reference validation is the bounded Phase H command executed inside the temporary pinned Emissary checkout.

GitHub Actions status may be recorded only when actually available; local command success is not equivalent to remote CI success.

---

# 19. Explicit terminal acceptance criteria

Plan 117 may not close until all mandatory local/native criteria below are satisfied.

## Product corrections

1. `LookupAction::SendDatabaselookup.peer` remains distinct from `message.key` in a regression test.
2. Outbound lookup Tunnel Message ROUTER destination equals the selected floodfill peer.
3. DatabaseLookup body key remains the requested target key.
4. `DeliveryRequest.target` equals the outbound first-hop router.
5. Every lookup TunnelData `DeliveryRequest.message_bytes()` decodes as short-transport I2NP type 18.
6. Decoded short-transport TunnelData body equals the `TunnelDataMessage` emitted by the local OBGW.
7. Recovered nested `DatabaseLookup` decodes as standard-header I2NP and equals the state-machine-owned message.
8. Multi-cell dispatch uses a fresh outer TunnelData message ID per cell.
9. Publication uses the same valid short-transport TunnelData framing.
10. Publication ROUTER destination equals the selected publication floodfill.
11. Publication DatabaseStore key remains the local RouterInfo hash.

## Activation/lifetime

12. Pool activation transfers real `EstablishedMaterial` exactly once.
13. Pool entry registration/public routing metadata remains after activation.
14. Inbound reply-path selection returns the same path before and after activation.
15. Second activation returns `AlreadyActivated`, not `UnknownSlot`.
16. Activated tunnel still counts toward pool capacity.
17. Duplicate creator tunnel IDs remain duplicate after activation.
18. Activated registration expires through normal pool lifetime handling.
19. Failure/expiry can remove the corresponding DataPlaneRegistry role by slot.
20. No new persistent `LayerKeys` clone is introduced to preserve metadata.

## Readiness

21. NetDbSeam cannot report dispatch-ready when the registry has no usable outbound role.
22. Activating a usable outbound role allows dispatch-ready when an inbound reply path also exists.
23. Expiring/removing the role removes readiness.

## Phase G local closure

24. Inbound and outbound creator-side roles used by the terminal test originate from successful production short-build state machines and real registered `EstablishedMaterial`.
25. Phase G lookup chooses floodfill `F` and target key `K` with `F != K`.
26. Corrected outbound short-transport TunnelData reaches the simulated OBEP path.
27. OBEP exposes the exact standard-header DatabaseLookup with the real inbound reply gateway/tunnel.
28. Matching DatabaseStore returns through the real activated inbound tunnel.
29. i2pr local endpoint recovers the exact standard-header DatabaseStore.
30. Existing RouterInfo validation accepts the target RI.
31. RouterInfoStore contains the target RI.
32. RouterInfoLookup reaches `Success`.
33. Wrong-target DatabaseStore does not complete the lookup.
34. DatabaseSearchReply path remains iterative/non-success as appropriate.
35. Local RouterInfo publication traverses the corrected outbound path and exposes a valid standard DatabaseStore at the simulated OBEP.

## Phase H native closure

36. Pinned Emissary revision is exact and recorded.
37. Temporary reference patch SHA-256 is recorded before checkout deletion.
38. Emissary native short-build handler accepts the required outbound/inbound roles.
39. Emissary production OBEP accepts i2pr-generated TunnelData.
40. Emissary OBEP exposes DatabaseLookup key `K` and ROUTER destination `F` correctly.
41. Emissary native floodfill handles the lookup.
42. Successful native response traverses the Emissary inbound role path.
43. i2pr local endpoint recovers the native DatabaseStore response.
44. i2pr validates/stores the RouterInfo and lookup reaches success.
45. Emissary native floodfill accepts i2pr RouterInfo publication.
46. Strongest available independent publication observation is recorded without overclaim.

## External classification / authority

47. 117-X is either genuinely passed through an existing authenticated lane or explicitly `deferred-host-lane-unavailable`.
48. Q1 and Q2 are separately classified.
49. `117-status.md`, `117-handoff.md`, roadmap, README/AGENTS architecture authority, and `specs/support.toml` agree on the final state.
50. No document claims the in-process Emissary native checkpoint requires rootless namespace or Multipass.
51. No normal-daemon NTCP2 activation is introduced.
52. No new Python/VM/container interoperability harness is introduced.

---

# 20. Closure states

## 20.1 Required local/native success, authenticated lane unavailable

This is a successful Plan 117 product closure:

```text
plan_117_corrective_routing          = passed
plan_117_transport_framing           = passed-short-transport-tunneldata
plan_117_activation_ownership        = passed-metadata-retained-secrets-once
plan_117_runtime_readiness           = passed-registry-derived
plan_117_local_composition           = passed-all-i2pr-production-seam-netdb
plan_117_native_reference            = passed-emissary-mixed-router-netdb
plan_117_authenticated_transport     = deferred-host-lane-unavailable
Q1_authenticated_transport           = deferred
Q2_external_return_established       = deferred-or-not-proven-test-q2-bypass
plan_117                             = local-native-complete-external-deferred
milestone4b_authenticated_external   = blocked
router_construction                  = may-continue
normal_daemon_ntcp2                  = disabled-and-unenableable
ntcp2                                = experimental-non-advertised
```

This state is sufficient to move the router-construction roadmap forward while retaining the external transport evidence gap honestly.

## 20.2 Local/native/authenticated all pass

```text
plan_117_local_composition           = passed-all-i2pr-production-seam-netdb
plan_117_native_reference            = passed-emissary-mixed-router-netdb
plan_117_authenticated_transport     = passed
plan_117                             = passed-qualified-exploratory-netdb-integration
milestone4b_authenticated_external   = eligible-for-closure-review
```

Do not infer normal-daemon production-readiness.

## 20.3 Emissary exposes a reproducible protocol defect

Record the exact native stage and keep Plan 117 open only for that localized defect.

Do not reopen broad Plan 116 or Milestone 3 validation.

## 20.4 Emissary cannot build/run before native processing

Record the reference/tooling failure precisely.

Do not substitute the old rootless/Multipass harness automatically.

If the failure is purely temporary reference-build tooling and 117-L is green, stop at the attempt budget and leave 117-N unresolved rather than generating infrastructure.

---

# 21. Stop conditions and anti-loop rules

1. Do not spend time on authenticated transport before C1–C5 and Phase G are green.
2. Do not start the Emissary checkpoint before Phase G is green.
3. Do not use rootless namespaces or Multipass to execute Phase H.
4. Do not create a permanent Emissary adapter in i2pr.
5. Do not add a second independent router unless Emissary exposes an ambiguity that cannot be resolved from specification/source.
6. Do not reopen Plan 116 because the Plan 117 composition code used a Plan 116 primitive incorrectly.
7. Do not rewrite TunnelData crypto/fragments unless a focused regression or native reference proves that primitive itself wrong.
8. Do not enable normal-daemon NTCP2 to obtain a convenient integration test.
9. Keep production Rust changes smaller than the original A–F landing wherever possible.
10. If local/native closure passes and only 117-X is host-blocked, close Plan 117 as `local-native-complete-external-deferred` and move on.

---

# 22. Handoff summary for the executor

Start with these files:

```text
crates/i2pr-daemon/src/outbound_lookup.rs
crates/i2pr-tunnel/src/pool.rs
crates/i2pr-tunnel/src/data_plane_registry.rs
crates/i2pr-daemon/src/netdb_seam.rs
crates/i2pr-daemon/tests/netdb_integration.rs
```

First prove the three distinct routing identities in a failing regression:

```text
K = lookup key
F = selected floodfill
P = outbound first hop
```

Then make the transport handoff decode as:

```text
short-transport I2NP type 18 TunnelData
```

not raw 1028-byte TunnelData body bytes.

Then repair activation so the pool retains public metadata while the registry owns secrets.

Only after those contracts are green, build the Phase G terminal test from real successful short-build material.

Only after Phase G passes, execute the temporary pinned Emissary in-process native checkpoint.

Authenticated transport is last and may remain deferred on this host without reopening infrastructure work.
