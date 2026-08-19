# Plan 117 terminal native-reference correction

## Status

- **Ready for execution**.
- Date: **2026-08-18**.
- Current implementation floor: `b7e12e09d84089b5459d29aa962d01a963554b29`.
- Predecessor Plan 116: **closed** (`passed-final-local-closure`).
- Plan 117 local product composition: **passed through Phase G**.
- Plan 117 native reference: **not yet closed**.
- Plan 117 authenticated transport: **deferred-host-lane-unavailable**.
- Pinned independent reference remains:

```text
repository = https://github.com/eepnet/emissary.git
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package    = emissary-core 0.4.0
```

This plan supersedes only the premature Plan 117 closure conclusion at
`b7e12e09...`. It does **not** reopen Plan 116, does not redo Plan 117 C1-C5 or
Phase G, and does not authorize another transport-harness program.

---

# 1. Executive conclusion from blocker research

The remaining blocker is **not** that pinned Emissary lacks a usable in-process
native path, and it is **not** the constrained host.

The prior Phase H attempt used a separate path-dependency test crate
(`i2pr-emissary-test`) depending on `emissary-core`. That arrangement compiled
`emissary-core` as a normal dependency, so its `#[cfg(test)]` internal/native
test harness was unavailable. The attempt therefore stopped at public I2NP
parser compatibility instead of reaching Emissary's native tunnel and NetDB
runtime seams.

The pinned revision already contains the required native machinery under its
own test build:

```text
emissary-core/src/tunnel/mod.rs
    #[cfg(test)] mod tests
    #[cfg(test)] pub use GarlicHandler
    #[cfg(test)] pub use TunnelMessage / TunnelMessageRecycle

emissary-core/src/tunnel/tests/mod.rs
    make_router(...)
    TestTransitTunnelManager
    connect_routers(...)
    build_outbound_tunnel(...)
    build_inbound_tunnel(...)

emissary-core/src/tunnel/transit/mod.rs
    TransitTunnelManager::handle_short_tunnel_build(...)
    native IBGW / Participant / OBEP role creation

emissary-core/src/tunnel/transit/outbound.rs
    production OBEP TunnelData decrypt / checksum / fragment / standard-I2NP parse
    ROUTER and TUNNEL delivery

emissary-core/src/tunnel/transit/inbound.rs
    production IBGW TunnelGateway -> TunnelData transformation

emissary-core/src/tunnel/transit/participant.rs
    production participant TunnelData layer transformation / forwarding

emissary-core/src/subsystem/mod.rs
    in-process message routing to native tunnel roles and NetDB

emissary-core/src/netdb/mod.rs
    native floodfill DatabaseLookup / DatabaseStore handling

emissary-core/src/tunnel/pool/handle.rs
    #[cfg(test)] TunnelPoolHandle::create()
    observable TunnelMessage::TunnelDeliveryViaRoute
```

Therefore:

```text
reference implementation                 = keep pinned Emissary
host/network changes                     = not needed
external path-dependency helper crate    = superseded / do not reuse
correct reference-test placement         = inside emissary-core #[cfg(test)]
```

The correction should be a small temporary Emissary **in-tree test patch**, not
new permanent i2pr infrastructure.

---

# 2. What remains valid and must not be redone

The following Plan 117 work at `b7e12e09...` is retained:

```text
C1 selected-floodfill routing identity        = passed
C2 outer TunnelData short-transport framing   = passed
C3 metadata-retaining one-shot activation     = passed
C4 registry-derived readiness                 = passed
C5 focused regression coverage                = passed
117-G all-i2pr production-seam NetDB path      = passed
117-I authenticated transport                  = deferred-host-lane-unavailable
normal-daemon NTCP2                            = disabled-and-unenableable
NTCP2                                          = experimental-non-advertised
```

Do not rewrite:

```text
crates/i2pr-tunnel/src/data.rs
crates/i2pr-tunnel/src/fragment.rs
crates/i2pr-tunnel/src/layer.rs
short-build cryptography
TunnelData framing
NetDB lookup state machine
publication codec
normal daemon transport configuration
```

unless this terminal native test produces a concrete, reproducible protocol
defect in one of those surfaces.

---

# 3. Remaining defects / incomplete closure items

## R1 — Phase H stopped at parser compatibility

Current highest retained stage:

```text
h_emissary_database_lookup_parsed
```

Current retained label:

```text
passed-emissary-wire-format-compatibility
```

That is valid evidence, but it does not satisfy the Plan 117 native reference
claim.

Missing native stages are:

```text
Emissary native short-build role admission
Emissary production OBEP TunnelData processing
Emissary production floodfill DatabaseLookup handling
Emissary native inbound IBGW / participant return
producer-to-i2pr DatabaseStore recovery
existing i2pr RouterInfo validation/store
existing i2pr lookup Success
Emissary native publication acceptance
strongest practical publication observation
```

## R2 — prior Phase H test placement disabled the intended reference harness

The prior external helper crate was structurally incapable of importing
Emissary's `#[cfg(test)]` `tunnel::tests` module and test-only pool seams.

Correct placement:

```text
temporary pinned Emissary checkout
  -> emissary-core/Cargo.toml dev-dependencies on i2pr crates
  -> emissary-core/src/tunnel/tests/i2pr_plan117_native.rs
  -> mod i2pr_plan117_native; in emissary-core/src/tunnel/tests/mod.rs
  -> cargo test -p emissary-core --lib <exact selector>
```

Because the test now belongs to `emissary-core`, its `#[cfg(test)]` internals are
available without exposing new production APIs.

## R3 — no canonical final patch digest was retained

The previous attempt retained SHA-256 values for individual files, but the
corrective plan required one final binary diff artifact digest.

This pass must record:

```bash
git -C "$WORK/emissary" diff --binary > "$WORK/emissary-plan117-native.patch"
sha256sum "$WORK/emissary-plan117-native.patch"
```

Record the resulting single SHA-256 **before deleting the checkout**.

Individual file digests may be recorded additionally but do not replace the
patch digest.

## R4 — inbound runtime-role cleanup is not addressable by TunnelSlot

`ExploratoryPool` returns expired/failed `TunnelSlot`s. `DataPlaneRegistry`
indexes outbound roles by slot, but inbound roles are currently keyed by local
receive `TunnelId`.

The current test manually remembers the local receive ID before pool eviction.
That is sufficient as unit coverage but is a weak runtime synchronization seam.

Add a minimal reverse mapping so pool lifecycle events can remove either
runtime role using the same `TunnelSlot` identity the pool emits.

This is the only planned i2pr production correction in this pass unless the
native reference identifies a protocol defect.

## R5 — Plan 117 authority is contradictory

Current documents simultaneously claim:

```text
plan_117 = local-native-complete-external-deferred
```

and retain the stricter statement that native closure requires:

```text
plan_117_native_reference = passed-emissary-mixed-router-netdb
```

while recording only parser compatibility.

Current authority must be corrected to:

```text
plan_117 = terminal-native-reference-correction-pending
plan_117_local_composition = passed-all-i2pr-production-seam-netdb
plan_117_native_reference = parser-compatible-native-path-pending
plan_117_authenticated_transport = deferred-host-lane-unavailable
next_roadmap_plan = blocked-on-plan117-native-terminal-pass
```

If this plan succeeds, and only authenticated transport remains unavailable,
Plan 117 may then close and router construction may continue. It must **not** be
blocked indefinitely waiting for the host transport lane.

---

# 4. Normative boundaries retained from Plan 117

Official I2P behavior relevant to this pass remains:

```text
DatabaseLookup key K        = requested NetDB key
selected floodfill F        = router that receives the lookup
lookup travels              = through outbound exploratory tunnel
reply travels               = through inbound exploratory tunnel
nested I2NP in TunnelData   = standard-header I2NP
transport-facing TunnelData = short-transport I2NP for NTCP2/SSU2 boundary
```

The native reference test must keep at least these identities distinct:

```text
K = requested RouterInfo hash
F = Emissary floodfill router
P = first Emissary outbound tunnel hop / OBEP
I = Emissary inbound gateway
Q = Emissary inbound participant
L = i2pr local creator endpoint
```

For the outgoing i2pr lookup:

```text
DatabaseLookup.key       == K
Tunnel ROUTER destination == F
DeliveryRequest target    == P
DatabaseLookup.from       == I
DatabaseLookup.reply_tunnel_id == I's receive tunnel ID
```

Do not replace the exploratory tunnel with a direct transport send.

---

# 5. Corrected Phase H validation architecture

## 5.1 Use one temporary Emissary in-tree test

Do not create a new i2pr workspace crate or permanent integration harness.

Temporary checkout:

```bash
set -euo pipefail
I2PR_ROOT="$(git rev-parse --show-toplevel)"
WORK="$(mktemp -d)"
git clone https://github.com/eepnet/emissary.git "$WORK/emissary"
git -C "$WORK/emissary" checkout --detach 9b43484a21d5a1291c4881cdae62a36c527f8c0f
ln -s "$I2PR_ROOT" "$WORK/emissary/i2pr-under-test"
```

Add only dev dependencies needed by the temporary test, e.g.:

```toml
[dev-dependencies]
i2pr-proto = { path = "../i2pr-under-test/crates/i2pr-proto" }
i2pr-tunnel = { path = "../i2pr-under-test/crates/i2pr-tunnel" }
i2pr-netdb = { path = "../i2pr-under-test/crates/i2pr-netdb" }
i2pr-daemon = { path = "../i2pr-under-test/crates/i2pr-daemon" }
i2pr-transport = { path = "../i2pr-under-test/crates/i2pr-transport" }
```

Use the minimum subset that compiles the test. Do not add application/runtime
dependencies simply for convenience.

Preferred test location:

```text
emissary-core/src/tunnel/tests/i2pr_plan117_native.rs
```

and in `emissary-core/src/tunnel/tests/mod.rs`:

```rust
mod i2pr_plan117_native;
```

This is the critical correction from the parser-only attempt.

## 5.2 No sockets or reference daemon

The test must remain in-process:

```text
MockRuntime
SubsystemManager
TransitTunnelManager
native tunnel routing table
native NetDb
thingbuf channels
```

Do not use:

```text
TCP/UDP sockets
rootless namespaces
Multipass
Docker
VMs
reference daemon startup
public I2P
Python orchestration
```

## 5.3 Reference-side helpers may be added only under #[cfg(test)]

A small helper extension to `TestTransitTunnelManager` is authorized when needed
to drive the already-existing production futures.

Examples:

```rust
#[cfg(test)]
pub fn inject_message(&self, sender: RouterId, message: Message) {
    self.transport_tx.try_send(SubsystemEvent::Message {
        messages: vec![(sender, message)],
    })?;
}
```

and/or a bounded helper for receiving one outbound message while polling the
retained transit manager.

Requirements:

- helper is test-only;
- helper routes through `SubsystemManager`, not directly into OBEP internals;
- helper does not decrypt or transform TunnelData itself;
- helper has bounded polling / timeout behavior;
- helper is removed when the temporary checkout is deleted.

Do not expose these helpers as Emissary production API.

---

# 6. Minimal i2pr runtime cleanup before native reference

## 6.1 Add slot identity for inbound registry entries

Modify `DataPlaneRegistry` so the scheduler can synchronize pool expiry/failure
using the pool's canonical `TunnelSlot`.

Recommended representation:

```rust
outbound: BTreeMap<TunnelSlot, OutboundGatewayRole>
inbound: BTreeMap<TunnelId, LocalInboundEndpointRole>
inbound_slot_to_receive: BTreeMap<TunnelSlot, TunnelId>
inbound_receive_to_slot: BTreeMap<TunnelId, TunnelSlot> // optional if useful
```

or another bounded equivalent.

Change inbound activation to bind the pool slot explicitly:

```rust
activate_inbound(
    slot: TunnelSlot,
    established: EstablishedTunnel,
    ...
)
```

Then expose one scheduler cleanup seam:

```rust
remove_slot(slot: TunnelSlot) -> RegistryRemoval
```

Behavior:

```text
if outbound slot exists -> remove/return outbound role
if inbound slot exists  -> resolve local receive ID, remove inbound role + reverse metadata
if unknown              -> typed no-op / UnknownSlot according to current registry style
```

Do not make `ExploratoryPool` depend on `DataPlaneRegistry`.

The runtime composition owner remains responsible for:

```text
for slot in pool.advance_time(now_seconds) {
    registry.remove_slot(slot)
}
```

and the same after `mark_failed(slot)`.

## 6.2 Required lifecycle tests

At minimum:

```text
inbound_activation_records_slot_identity
remove_slot_removes_inbound_role
remove_slot_removes_outbound_role
expired_pool_slot_can_remove_registry_inbound
failed_pool_slot_can_remove_registry_inbound
remove_slot_clears_inbound_reverse_mapping
inbound_duplicate_slot_rejected
inbound_duplicate_receive_id_rejected
unknown_slot_cleanup_is_bounded_and_fail_closed
```

Retain existing activation/reply-path tests.

---

# 7. Native fixture construction

Create three native reference fixtures in the one Emissary test:

```text
P = Emissary outbound endpoint router
I = Emissary inbound gateway router
Q = Emissary inbound participant router
F = Emissary floodfill fixture
```

P/I/Q should use `TestTransitTunnelManager` so their real transit roles are
created by `handle_short_tunnel_build()`.

F should use Emissary's own components directly:

```text
make_router(...)
RouterContext
SubsystemManager<MockRuntime>
TunnelPoolHandle::create()
NetDb<MockRuntime>::new(..., floodfill=true, ...)
```

The floodfill fixture must retain:

```text
router ID/hash F
Subsystem transport input
TunnelPoolHandle message receiver
native NetDb future/task
```

It does not need a real external network transport.

Connect the native reference routers through Emissary's test connection
channels (`connect_router` / `connect_routers`) as required.

---

# 8. Native short-build construction

## 8.1 Outbound i2pr -> Emissary OBEP

Use the production i2pr short-build path with P as the real OBEP hop.

Required source path:

```text
ShortBuildStateMachine::new
 -> prepare
 -> deliver_action
 -> ShortBuildI2npBridge
 -> Emissary Message::parse_standard
 -> TestTransitTunnelManager::handle_short_tunnel_build
```

Require native acceptance:

```text
Emissary found the real record
Emissary Noise decrypt succeeded
Emissary short request parse succeeded
role == OutboundEndpoint
feedback == Some(...)
returned next router/tunnel match i2pr request
native OBEP transit role is registered and runnable
```

This extends the already-passed Plan 115 Q0 evidence into the actual Plan 117
fixture.

## 8.2 Outbound build reply Q2 test boundary

The Emissary OBEP returns the transformed short-build reply wrapped in
`TunnelGateway -> Garlic`.

Do **not** implement general i2pr garlic merely to finish this test.

The prior Plan 117 rules already authorized a test-only Q2 decapsulation
boundary. Make that boundary explicit and narrow.

Preferred implementation:

- in the temporary Emissary patch only, add a `#[cfg(test)]` observer to the
  short-build handler that captures a copy of the already-transformed,
  count-prefixed reply payload immediately **after** native record processing
  and **before** OBEP Garlic wrapping;
- the production handler must still construct its normal TunnelGateway/Garlic
  response;
- feed the captured raw reply payload to i2pr's
  `BuildEvent::BuildReply` so the creator state machine may reach
  `Established`;
- do not synthesize reply bytes or reimplement Emissary's tunnel-build crypto.

Record:

```text
Q2_external_return_established = not-proven-test-q2-bypass
```

This test boundary is acceptable because Phase 117 is validating the data plane,
not general garlic reception.

If a cleaner existing Emissary test utility exposes equivalent pre-Garlic reply
bytes, use it instead of adding a new observer.

## 8.3 Inbound i2pr -> Emissary IBGW + Participant

Create a production i2pr inbound short build targeting:

```text
I = InboundGateway
Q = Participant / terminal remote hop
L = i2pr creator endpoint
```

Drive each request through the respective Emissary production
`handle_short_tunnel_build()`.

The terminal non-OBEP path should return the transformed short-build records
without OBEP garlic. Feed the count-prefixed reply payload to the i2pr state
machine through `BuildEvent::BuildReply`.

Require:

```text
I native admission accepted
Q native admission accepted
i2pr inbound state reaches Established
real EstablishedMaterial extracted
real pool registration / activation
real LocalInboundEndpointRole installed
```

If the pinned API requires one narrow test-only message-routing shim to return
the final build reply, use Emissary's existing subsystem/listener machinery.
Do not bypass native short-build processing.

---

# 9. Native publication first: preload and independently observe the RouterInfo

Running publication before lookup gives the floodfill a known RouterInfo K and
allows a read-after-write observation later.

## 9.1 Use production i2pr publication message construction

Use the existing Plan 117 production path:

```text
PublicationCoordinator / retained DatabaseStoreMessage
 -> compose_outbound_publication
 -> activated i2pr OutboundGatewayRole
 -> complete short-transport TunnelData DeliveryRequest
```

Target floodfill:

```text
F = native Emissary floodfill
```

Published key:

```text
K = i2pr test RouterInfo hash
```

Do not hand-build the DatabaseStore wire body.

## 9.2 Feed the TunnelData through native Emissary OBEP

Convert only the outer transport representation at the reference boundary:

```text
i2pr DeliveryRequest.message_bytes()
 -> Emissary Message::parse_short / equivalent short-header parser
 -> TestTransitTunnelManager(P).inject_message(...)
 -> Emissary SubsystemManager
 -> native registered OBEP
 -> native OBEP decrypt/checksum/TunnelData parse
 -> ROUTER delivery to F
```

Require the message captured on the P -> F reference connection is:

```text
MessageType::DatabaseStore
standard nested I2NP semantics preserved
key == K
RouterInfo payload is accepted by Emissary
```

## 9.3 Inject the OBEP output into native floodfill F

Use F's `SubsystemEvent::Message` input so its real SubsystemManager routes the
DatabaseStore to its native NetDb.

Poll/spawn the native NetDb normally.

Require no parser/validation error and preserve the strongest observable store
state available through public/test-native APIs.

Do not add production getter APIs solely to inspect private `router_infos`.
Read-after-write in the next phase is the preferred observation.

---

# 10. Native lookup through OBEP and floodfill

## 10.1 Do not re-prove local floodfill candidate selection in Phase H

Phase G already proves `RouterInfoLookup` selects F from the i2pr store with
`F != K`.

The native reference should focus on cross-implementation consumption, not add
an unrelated requirement to import Emissary's RouterInfo into the i2pr candidate
store merely to select a peer already known by the test fixture.

Use the production i2pr lookup/body construction path while binding the selected
peer to the native fixture's public router hash F.

Allowed approaches, in preference order:

1. use the real `RouterInfoLookup` action if the fixture can seed F through
   existing public validation/store APIs without extra product changes;
2. otherwise construct the typed `LookupAction::SendDatabaselookup` through the
   existing production/test builder path using the already-built
   `DatabaseLookupMessage`, with `peer = F`.

Forbidden:

```text
hand-written DatabaseLookup bytes
reimplementation of lookup flags
using K as the selected peer
adding a permanent reference-peer injection API to production solely for test
```

The test must still assert:

```text
F != K
P != F
DatabaseLookup.key == K
Tunnel ROUTER target == F
DeliveryRequest target == P
reply gateway == I
reply tunnel == I receive tunnel
```

## 10.2 Native OBEP acceptance

Compose the corrected outbound lookup and inject the outer short-transport
TunnelData into P through the same subsystem route as publication.

Require P's production OBEP:

```text
decrypts TunnelData
validates checksum
parses Tunnel Message record(s)
reassembles if needed
parses nested standard I2NP
emits DatabaseLookup to router F
```

Capture the emitted standard DatabaseLookup from P's connected F receiver.

Assert the exact semantic fields above.

This is the minimum point at which the prior parser-only result is materially
surpassed.

## 10.3 Native floodfill handling

Inject P's emitted DatabaseLookup into F's SubsystemManager as a received
message.

Require F's native `NetDb` with `floodfill=true` executes its production lookup
handler.

Because K was published in §9, require the result is a native
`DatabaseStore`, not a synthetic response.

For a tunnel reply, Emissary's native NetDb emits through its exploratory pool
handle. Observe the test-only `TunnelPoolHandle::create()` receiver.

Expected native output:

```text
TunnelMessage::TunnelDeliveryViaRoute {
    router_id: I,
    tunnel_id: I_receive,
    message: <standard DatabaseStore bytes>,
    ...
}
```

Require:

```text
router_id == I
tunnel_id == i2pr declared reply tunnel
message parses as standard DatabaseStore
DatabaseStore.key == K
```

This is native floodfill handling, not parser-only handling.

---

# 11. Explicit boundary: floodfill outbound exploratory tunnel is not part of this checkpoint

Do not build an Emissary exploratory outbound tunnel for the floodfill solely to
carry its response to I.

The native NetDb's `TunnelDeliveryViaRoute` is the semantic handoff into that
routing layer and already proves that the floodfill chose the declared inbound
reply router/tunnel.

Convert this test-observed instruction to the corresponding native
`TunnelGateway` message and inject it into I through I's SubsystemManager.

This boundary is intentionally recorded as:

```text
reference_floodfill_response_routing = native-TunnelDeliveryViaRoute-observed
reference_floodfill_outbound_exploratory_tunnel = bypassed-at-test-routing-boundary
```

Why this is acceptable:

- the i2pr **outbound** lookup still traverses real i2pr tunnel crypto and a
  native Emissary OBEP;
- the lookup is processed by native Emissary floodfill NetDb;
- the native floodfill independently chooses the declared i2pr inbound reply
  router/tunnel;
- the response then traverses the native Emissary IBGW + participant data path;
- building a second Emissary creator-side exploratory pool would test Emissary
  against itself and add unrelated orchestration, not increase confidence in
  i2pr's protocol implementation.

Do not call this Q1/live transport evidence.

---

# 12. Native inbound return through Emissary IBGW + participant

Take the native standard DatabaseStore from the floodfill's
`TunnelDeliveryViaRoute` and construct the corresponding Emissary
`TunnelGateway` message for I's receive tunnel.

Inject into I's SubsystemManager.

Require native flow:

```text
I SubsystemManager
 -> installed native InboundGateway
 -> TunnelGateway parse
 -> standard nested DatabaseStore parse
 -> native TunnelData construction
 -> native IBGW layer transform
 -> forward to Q
 -> Q native Participant
 -> participant layer transform
 -> forward final TunnelData toward L
```

Capture the final TunnelData from Q's connection to the synthetic i2pr creator
router L.

Do **not** decrypt or rewrite the TunnelData on the reference side.

Convert only the Emissary typed/message body representation to the existing i2pr
`TunnelDataMessage` representation, preserving exact body bytes.

Feed it to:

```text
i2pr daemon inbound dispatch
 -> DataPlaneRegistry local inbound endpoint
 -> LocalInboundEndpointRole
 -> standard DatabaseStore recovery
 -> existing RouterInfo lookup response handler
 -> existing RouterInfo validator
 -> RouterInfoStore
```

Require:

```text
recovered DatabaseStore.key == K
RouterInfo validates
RouterInfoStore.contains(K) == true
RouterInfoLookup reaches Success
```

This is the terminal `117-N` lookup result.

---

# 13. Publication acceptance and independent observation

The publication in §9 counts as native publication acceptance only if native
Emissary NetDb accepts the store.

The subsequent lookup in §10 is the preferred independent observation:

```text
i2pr publication DatabaseStore
 -> i2pr outbound tunnel
 -> native Emissary OBEP
 -> native Emissary floodfill store
 -> later native DatabaseLookup for same K
 -> native floodfill emits DatabaseStore for K
```

Record:

```text
emissary_native_publication_acceptance = passed
emissary_publication_observation = passed-read-after-write
```

If a pinned Emissary behavior prevents read-after-write despite confirmed native
store acceptance, stop and record the strongest actual native stage. Do not add
production inspection APIs or fake the observation.

However, `117-N = passed-emissary-mixed-router-netdb` requires at minimum:

```text
native publication accepted
native lookup handled
native response routed to declared inbound tunnel
native inbound roles consumed response
final i2pr lookup succeeded
```

---

# 14. Corrected Phase H stage taxonomy

Record the highest actual stage monotonically:

```text
h2_reference_checkout_pinned
h2_in_tree_test_compiled_with_cfg_test
h2_i2pr_outbound_stbm_generated
h2_emissary_obep_build_accepted
h2_i2pr_outbound_established_test_q2_bypass
h2_i2pr_inbound_stbm_generated
h2_emissary_ibgw_build_accepted
h2_emissary_participant_build_accepted
h2_i2pr_inbound_established
h2_publication_tunneldata_encoded
h2_emissary_obep_publication_tunneldata_accepted
h2_emissary_floodfill_publication_accepted
h2_lookup_tunneldata_encoded
h2_emissary_obep_lookup_tunneldata_accepted
h2_emissary_database_lookup_exposed
h2_emissary_floodfill_lookup_handled
h2_emissary_native_tunnel_reply_instruction_observed
h2_emissary_ibgw_return_accepted
h2_emissary_participant_return_accepted
h2_i2pr_database_store_recovered
h2_i2pr_routerinfo_validated
h2_i2pr_lookup_success
h2_publication_read_after_write_observed
```

Failure taxonomy:

```text
failed-h2-reference-build
failed-h2-cfg-test-access
failed-h2-i2pr-build-construction
failed-h2-emissary-short-build-obep
failed-h2-emissary-short-build-ibgw
failed-h2-emissary-short-build-participant
failed-h2-q2-test-boundary
failed-h2-emissary-obep-tunneldata
failed-h2-emissary-obep-nested-i2np
failed-h2-emissary-publication-store
failed-h2-emissary-floodfill-lookup
failed-h2-native-reply-routing
failed-h2-emissary-ibgw-return
failed-h2-emissary-participant-return
failed-h2-i2pr-inbound-tunneldata
failed-h2-i2pr-databasestore
failed-h2-i2pr-routerinfo-validation
failed-h2-i2pr-lookup-completion
```

Do not use a generic `interop failed` label.

---

# 15. Attempt budget

The prior parser-only attempt budget is **not** counted against this corrected
method because those attempts never compiled Emissary with the test
configuration needed to access the intended native harness.

Authorize one fresh, bounded budget for this in-tree method only:

```text
1 baseline in-tree compile/test attempt
2 narrow corrections for temporary test API/borrow/lifecycle mistakes
1 confirmation run after the full native path succeeds
STOP
```

A compile error caused by temporary test imports, visibility, async polling, or
borrow lifetimes is a reference-test tooling issue, not an i2pr protocol defect.

If native production processing reaches a protocol boundary and rejects i2pr in
a reproducible way:

```text
1 localized i2pr product correction
1 focused confirmation
STOP
```

Do not switch to i2pd/Java inside this pass unless the pinned Emissary source
itself proves incapable of exercising the documented native path. Current source
research does **not** indicate that.

---

# 16. Required test-only Emissary patch discipline

Allowed temporary changes:

```text
emissary-core/Cargo.toml dev-dependencies
emissary-core/src/tunnel/tests/mod.rs module declaration
one new test module under emissary-core/src/tunnel/tests/
small #[cfg(test)] helper(s) on TestTransitTunnelManager
small #[cfg(test)] pre-Garlic reply observer if no existing utility suffices
```

Forbidden temporary changes:

```text
changing production protocol semantics
weakening checks in native OBEP/IBGW/Participant/NetDb
hard-coding i2pr ciphertext as accepted
bypassing native short-build Noise processing
bypassing native OBEP TunnelData decrypt/checksum/parser
bypassing native floodfill DatabaseLookup handler
bypassing native IBGW/Participant TunnelData transforms
adding a permanent public Emissary API
```

The Q2 observer may expose the already-produced raw build reply to the test, but
must not replace the native handler that generated it.

---

# 17. Durable evidence

Before deleting the temporary checkout, record:

```text
i2pr_source_commit
reference_repository
reference_revision
reference_package_version
reference_test_name
reference_command
reference_patch_sha256
reference_patch_byte_length
reference_highest_stage
reference_decision
reference_q2_boundary
reference_floodfill_response_boundary
outbound_real_hop_count
inbound_real_hop_count
lookup_key_sha256_or_routerhash
selected_floodfill_routerhash
publication_key_matches_lookup_key
native_publication_accepted
native_read_after_write_observed
native_lookup_response_type
i2pr_lookup_final_state
raw_secret_material_retained = false
```

Do not record:

```text
private X25519 keys
short-build reply keys
layer keys
IV keys
garlic keys/tags
raw decrypted build records
raw TunnelData plaintext
full reference logs
```

### Required patch-digest sequence

Run before checkout deletion:

```bash
git -C "$WORK/emissary" diff --binary > "$WORK/emissary-plan117-native.patch"
PATCH_BYTES="$(wc -c < "$WORK/emissary-plan117-native.patch")"
PATCH_SHA256="$(sha256sum "$WORK/emissary-plan117-native.patch" | awk '{print $1}')"
printf 'patch_bytes=%s\npatch_sha256=%s\n' "$PATCH_BYTES" "$PATCH_SHA256"
```

Copy the digest and byte length into `plans/117-status.md` before cleanup.

Then delete the checkout.

---

# 18. Local validation bar

Before the reference pass, run focused local checks for the slot-cleanup change:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
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

Do not make unrelated product changes solely to fix a historical warning unless
it prevents this pass from compiling under the repository's actual validation
bar.

Reference command should be one exact in-tree test selector, e.g.:

```bash
cargo test -p emissary-core --lib \
  tunnel::tests::i2pr_plan117_native::i2pr_plan117_native_mixed_router_netdb \
  -- --exact --nocapture
```

Use `cargo test -p emissary-core --lib -- --list` once if required to resolve the
exact selector.

A broad Emissary suite is not required because production Emissary behavior is
unchanged by the temporary test patch.

---

# 19. Explicit acceptance criteria

Plan 117 may close only when every mandatory item below is satisfied or a
specifically listed deferred external item is honestly classified.

## A. Retained local correction state

1. C1 remains green: lookup key K and selected floodfill F are distinct and
   tunnel ROUTER delivery targets F.
2. C2 remains green: transport-facing TunnelData is complete short-transport
   I2NP type 18.
3. C3 remains green: pool metadata remains after one-shot secret activation.
4. C4 remains green: production readiness comes from DataPlaneRegistry.
5. Phase G remains green: all-i2pr production-seam lookup reaches Success.

## B. Registry lifecycle cleanup

6. Inbound activated role is associated with its canonical pool `TunnelSlot`.
7. `remove_slot(slot)` removes an outbound role by slot.
8. `remove_slot(slot)` removes an inbound role and its receive-ID metadata by
   slot.
9. Pool expiry slot can drive registry inbound cleanup without retaining an
   out-of-band local receive ID.
10. Pool failure slot can drive the same cleanup.
11. Duplicate inbound slot/receive identities fail closed.
12. No persistent secret clone is introduced by the reverse mapping.

## C. Correct reference method

13. Emissary is pinned exactly to
    `9b43484a21d5a1291c4881cdae62a36c527f8c0f`.
14. The Plan 117 native test is compiled **inside `emissary-core` with
    `#[cfg(test)]` active**.
15. No external `i2pr-emissary-test` path-dependency crate is used as the native
    validation method.
16. No socket, namespace, VM, Docker, Multipass, or public network is used.
17. Only temporary test/dev-dependency instrumentation is added to Emissary.

## D. Native short builds

18. i2pr-generated outbound STBM is independently parsed and accepted by
    Emissary's production short-build handler for OBEP role.
19. Emissary creates/registers a live native OBEP transit role.
20. The temporary Q2 boundary returns only the native transformed raw build
    reply payload, while Emissary still constructs its normal Garlic reply.
21. i2pr outbound state reaches Established using the captured native reply.
22. i2pr-generated inbound STBM is accepted by native Emissary IBGW.
23. i2pr-generated inbound STBM is accepted by native Emissary Participant.
24. i2pr inbound state reaches Established from native transformed replies.
25. Extracted established material activates through the existing pool/registry
    production seams.

## E. Native publication

26. i2pr publication uses its existing typed DatabaseStore production path.
27. Publication outer TunnelData passes through the production Emissary OBEP.
28. Emissary OBEP emits a standard DatabaseStore to F.
29. Emissary native floodfill accepts the RouterInfo publication for K.

## F. Native lookup

30. Lookup test has `F != K` and `P != F`.
31. i2pr production lookup carries `DatabaseLookup.key == K`.
32. i2pr tunnel ROUTER delivery targets F.
33. i2pr transport DeliveryRequest targets P.
34. i2pr lookup declares native Emissary IBGW router I and its receive tunnel as
    reply path.
35. Production Emissary OBEP accepts the i2pr TunnelData.
36. Production Emissary OBEP exposes the nested standard DatabaseLookup.
37. The exposed lookup has exact K/F/reply semantics.
38. Native Emissary floodfill handles the lookup.
39. Native floodfill produces DatabaseStore for K from the previously accepted
    publication, not a test-synthetic response.
40. Native floodfill emits a `TunnelDeliveryViaRoute` for I and the exact
    declared reply tunnel.

## G. Native inbound return

41. The observed native tunnel-delivery instruction is converted only to its
    corresponding native TunnelGateway representation at the explicit test
    boundary.
42. Native Emissary IBGW consumes the TunnelGateway.
43. Native Emissary IBGW produces/forwards TunnelData.
44. Native Emissary Participant consumes/transforms/forwards TunnelData.
45. The final TunnelData bytes are delivered to i2pr without reference-side
    decryption/re-encryption.
46. i2pr production local inbound endpoint recovers a standard DatabaseStore.
47. Recovered DatabaseStore key equals K.
48. Existing i2pr RouterInfo validation accepts the RouterInfo.
49. Existing i2pr RouterInfoStore contains K.
50. Existing i2pr RouterInfoLookup reaches Success.

## H. Independent publication observation

51. Native publication acceptance is recorded.
52. The later native lookup observes the stored RouterInfo as a DatabaseStore
    read-after-write result, unless a pinned-reference limitation is explicitly
    demonstrated.

## I. Evidence / authority

53. One final `git diff --binary` patch SHA-256 is recorded before temporary
    checkout deletion.
54. Q2 is recorded as `not-proven-test-q2-bypass` when the pre-Garlic observer is
    used.
55. Floodfill response routing boundary is recorded as
    `native-TunnelDeliveryViaRoute-observed`.
56. 117-X remains separately classified as
    `deferred-host-lane-unavailable` unless a pre-existing qualified lane has
    independently become runnable.
57. No document equates parser compatibility with mixed-router NetDB success.
58. Status, handoff, roadmap, README/AGENTS/architecture/support authority agree
    on the final Plan 117 state.
59. Normal-daemon NTCP2 remains disabled and non-advertised.
60. No permanent reference harness is added to i2pr.

---

# 20. Closure labels

## Success branch

If the terminal native path passes and the authenticated host lane remains
unavailable:

```text
plan_117_c1_routing                   = passed
plan_117_c2_transport_framing         = passed-short-transport-tunneldata
plan_117_c3_activation_ownership      = passed-metadata-retained-secrets-once
plan_117_c4_runtime_readiness         = passed-registry-derived
plan_117_g_local_production_seam      = passed-all-i2pr-production-seam-netdb
plan_117_h_native_reference           = passed-emissary-mixed-router-netdb
plan_117_h_q2_build_return            = not-proven-test-q2-bypass
plan_117_h_floodfill_response_route   = native-TunnelDeliveryViaRoute-observed
plan_117_i_authenticated_transport    = deferred-host-lane-unavailable
Q1_authenticated_transport            = deferred
Q2_external_return_established        = deferred
plan_117                              = local-native-complete-external-deferred
milestone4b_authenticated_external    = blocked
router_construction                   = may-continue
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
```

**This state permits the router-construction roadmap to continue.**

Do not change it to `blocked-until-external-delivery-lane-available`; that would
recreate the environment loop Plan 115-117 was designed to avoid.

## Native protocol-defect branch

If production Emissary reaches one of the native stages and rejects i2pr due to
a reproducible protocol disagreement:

```text
plan_117 = protocol-defect-localized-native-reference
reference_highest_stage = <exact h2 stage>
```

Correct exactly that defect once and rerun the focused test.

Do not reopen Plan 116 broadly.

## Reference-tooling branch

If the corrected **in-tree** test cannot compile/run through native processing
inside the fresh attempt budget:

```text
plan_117_h_native_reference = unresolved-reference-test-tooling
plan_117 = native-reference-terminal-pending
router_construction_next = hold-for-one-decision
```

Record the exact failure. Do not silently substitute parser compatibility as a
pass and do not automatically build a namespace/VM harness.

At that point a new human/planning decision may determine whether the native
criterion should be narrowed or another implementation used. Current source
research does not justify that fallback preemptively.

---

# 21. Authority update order after execution

Only after the technical result is known, update current authority in this
order:

```text
plans/117-status.md
plans/117-handoff.md
plans/115-117-external-delivery-to-live-netdb-roadmap.md
specs/support.toml
docs/architecture/i2pr-daemon.md
docs/architecture/i2pr-netdb.md
README.md
AGENTS.md
```

Do not rewrite historical plan files to make the path look cleaner.

Retain the parser-only Phase H attempt as historical intermediate evidence:

```text
prior_phase_h_stage = h_emissary_database_lookup_parsed
prior_phase_h_label = passed-emissary-wire-format-compatibility
prior_phase_h_method = external-path-dependency-crate
prior_phase_h_disposition = valid-parser-evidence-but-not-native-closure
```

Then add the new `h2_*` record beneath it.

---

# 22. Stop conditions / anti-overengineering rules

1. Do not redo C1-C5.
2. Do not redo Phase G except as regression validation.
3. Do not reopen Plan 116.
4. Do not switch to Java I2P or i2pd before attempting this corrected Emissary
   in-tree method.
5. Do not build a permanent Emissary adapter crate.
6. Do not create Python orchestration.
7. Do not create Docker/VM/namespace infrastructure.
8. Do not start normal-daemon NTCP2.
9. Do not implement general garlic for the OBEP build reply; use the authorized
   narrow Q2 test boundary.
10. Do not make the native floodfill build its own exploratory outbound tunnel
    merely to carry the reply; observe its native `TunnelDeliveryViaRoute` and
    begin the native inbound transit path there.
11. Do not add production Emissary inspection APIs.
12. Do not add production i2pr reference-only injection APIs.
13. Do not confuse temporary test compilation failures with protocol failures.
14. Do not close Plan 117 at parser compatibility.
15. Once full 117-N passes, do not wait for the unavailable authenticated host
    lane before continuing router construction.

---

# 23. Handoff summary

Execute in this exact order:

```text
N0  add TunnelSlot-based inbound DataPlaneRegistry cleanup
 -> focused i2pr tests green

N1  fresh pinned Emissary checkout
 -> add i2pr crates as emissary-core dev-dependencies
 -> put one Plan 117 test inside emissary-core's own #[cfg(test)] tree

N2  add only necessary TestTransitTunnelManager test helpers
 -> transport-event injection / bounded polling
 -> optional pre-Garlic raw-reply observer

N3  native short-build fixture
 -> i2pr outbound -> Emissary OBEP accepted/active
 -> i2pr inbound -> Emissary IBGW + Participant accepted/active
 -> i2pr established material activated

N4  i2pr publication
 -> corrected outbound TunnelData
 -> native Emissary OBEP
 -> native Emissary floodfill accepts RouterInfo K

N5  i2pr lookup K through selected floodfill F
 -> native Emissary OBEP exposes DatabaseLookup
 -> native Emissary floodfill handles lookup
 -> native TunnelDeliveryViaRoute(I, tunnel) observed

N6  native response return
 -> TunnelGateway to Emissary IBGW
 -> Emissary Participant
 -> final TunnelData to i2pr endpoint
 -> DatabaseStore recovered
 -> RI validates/stores
 -> lookup Success

N7  require read-after-write publication observation

N8  record final `git diff --binary` patch SHA-256 before deleting checkout

N9  run local validation bar + one confirmation native run

N10 synchronize Plan 117 authority
 -> close only as `local-native-complete-external-deferred` if 117-N passed
 -> leave 117-X deferred
 -> continue router construction
```

The key implementation correction is simple conceptually:

> **Compile the interoperability test as part of `emissary-core`'s own test
> build so the native reference harness actually exists. Do not test Emissary
> only as a public dependency.**

That is the narrowest path supported by the pinned source and the current host
constraints.
