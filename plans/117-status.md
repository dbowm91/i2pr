# Plan 117 status — native reference terminal result pending

- Status: **native-reference-terminal-pending**
- Date: 2026-08-19
- Original plan: [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md)
- First corrective closure: [`117-corrective-closure.md`](117-corrective-closure.md)
- Current terminal correction: [`117-terminal-native-reference-correction.md`](117-terminal-native-reference-correction.md)
- Handoff: [`117-handoff.md`](117-handoff.md)
- Roadmap: [`115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md)
- Predecessor Plan 116: **passed-final-local-closure**
- Pre-correction source commit: `2c764ab6263e2407dbb62a0a767b58b5f5bb44e8`
- Terminal-plan commit: `2c764ab6263e2407dbb62a0a767b58b5f5bb44e8`

## Current authority

```text
plan_115                              = passed-emissary-q0-construction-and-obep-reply-only
plan_116                              = passed-final-local-closure
plan_117                              = native-reference-terminal-pending
plan_117_c1_routing                   = passed
plan_117_c2_transport_framing         = passed-short-transport-tunneldata
plan_117_c3_activation_ownership      = passed-metadata-retained-secrets-once
plan_117_c4_runtime_readiness         = passed-registry-derived
plan_117_c5_regression                = passed
plan_117_g_local_production_seam      = passed-all-i2pr-production-seam-netdb
plan_117_h_parser_compatibility       = passed-emissary-wire-format-compatibility
plan_117_h_native_reference           = blocked-emissary-native-reply-layout
plan_117_i_authenticated_transport    = deferred-host-lane-unavailable
Q1_authenticated_transport            = deferred
Q2_external_return_established        = deferred
router_construction                   = hold-on-one-native-reference-decision
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
```

Plan 116 remains closed. Do not reopen it for this work.

The retained Plan 117 product work is still valid, and the inbound registry
slot reverse map is now implemented. The corrected in-tree Emissary test did
reach the native OBEP and produced a cryptographically authenticated reply,
but the pinned reference's reply plaintext is not the normative 202-byte
short-reply layout: its production handler mutates the request-derived record
body and leaves the reply mapping at the wrong offset. i2pr therefore rejects
the reply during strict `ShortReplyRecord` decoding. No i2pr parser relaxation
was made to accept a non-conformant reply.

---

## Retained implementation state

The following are considered passed unless a new native reference failure
localizes a concrete protocol defect:

### C1 — routing identity

```text
K = requested RouterInfo key
F = selected floodfill
P = outbound first hop

DatabaseLookup.key      = K
Tunnel ROUTER target    = F
DeliveryRequest.target  = P
```

The regression and Phase G path use distinct K/F/P identities.

### C2 — authenticated-link TunnelData framing

Nested NetDB I2NP remains standard-header I2NP. Transport-facing TunnelData is
encoded as complete short-transport I2NP type 18 before entering
`EncodedI2npMessage` / `DeliveryRequest`.

### C3 — activation ownership

`ExploratoryPool::activate(slot)` moves established secret material once while
retaining public registration/routing metadata for capacity, duplicate,
lifetime, and reply-path behavior. A second activation returns
`AlreadyActivated`.

The terminal lifecycle cleanup is complete: inbound registry roles bind to
`TunnelSlot`, and `remove_slot` removes either direction plus all reverse
metadata without cloning `LayerKeys`.

### C4 — readiness

The production Plan 117 readiness seam is
`NetDbSeam::composition_outcome_with_registry(...)`, which derives readiness
from actual usable `DataPlaneRegistry` roles. The older sticky boolean seam is
legacy/deprecated compatibility only.

### Phase G — local production composition

`plan117_all_i2pr_production_seam_routerinfo_lookup_success` builds real inbound
and outbound short-build state machines, registers/extracts real
`EstablishedMaterial`, activates the roles, composes an outbound lookup through
TunnelData, returns a DatabaseStore through the activated inbound path, validates
and stores the RouterInfo, and reaches lookup Success.

Therefore:

```text
117-L = passed
```

Do not redo Phase G except as regression validation.

---

## Prior Phase H evidence retained, but insufficient for closure

Reference pin:

```text
repo     = https://github.com/eepnet/emissary.git
revision = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package  = emissary-core 0.4.0
```

Prior method:

```text
external helper crate = i2pr-emissary-test
emissary-core usage    = normal path dependency
```

Prior highest stage:

```text
h_emissary_database_lookup_parsed
```

Prior valid label:

```text
passed-emissary-wire-format-compatibility
```

Prior test results:

```text
i2pr_router_lookup_is_consumed_by_emissary_native_parser              = passed
i2pr_leaseset_lookup_is_consumed_by_emissary_native_parser            = passed
i2pr_router_lookup_with_direct_reply_is_consumed_by_emissary_parser   = passed
i2pr_router_lookup_with_ignore_list_is_consumed_by_emissary_parser    = passed
i2pr_exploration_lookup_is_consumed_by_emissary_native_parser         = passed
i2pr_normal_lookup_is_consumed_by_emissary_native_parser              = passed
i2pr_full_database_lookup_message_is_consumed_by_emissary             = passed
```

Individual file digests retained by that attempt:

```text
i2pr-emissary-test/src/lib.rs = 8106c7f11fc256cf4083c15bc1772045c7f4c949ea0f335fb2d75dcefda0d4ff
i2pr-emissary-test/Cargo.toml = 255c1b7c1e56b8bd84492416b3e040a83266ea8042d58a1d04820fd17429c597
i2pr-emissary-test/Cargo.lock = 4d5c0b512fe4b5459591ccd25779ee5ee295671a0f3fb7beb53dcc8599cdebf3
```

These hashes are retained as historical evidence, but they are **not** the
canonical final binary-patch SHA required by the terminal native pass.

---

## Research result: why the prior native attempt stopped early

The blocker was the **placement of the reference test**.

Pinned Emissary declares its key native harness under `#[cfg(test)]` inside
`emissary-core`:

```text
emissary-core/src/tunnel/tests/mod.rs
    TestTransitTunnelManager
    make_router
    connect_routers
    build_outbound_tunnel
    build_inbound_tunnel

emissary-core/src/tunnel/pool/handle.rs
    TunnelPoolHandle::create

emissary-core/src/tunnel/mod.rs
    #[cfg(test)] mod tests
    #[cfg(test)] exports for GarlicHandler / TunnelMessage
```

A separate path-dependency crate does not compile dependency code with the
dependency's `cfg(test)` enabled. It therefore could use Emissary's public I2NP
parser, but not the intended native tunnel test machinery.

The pinned source itself provides the required in-process production seams:

```text
TransitTunnelManager::handle_short_tunnel_build
native IBGW / Participant / OBEP transit roles
SubsystemManager message/tunnel routing
OBEP TunnelData decrypt/checksum/fragment/I2NP processing
native floodfill DatabaseLookup handling
native floodfill RouterInfo DatabaseStore handling
IBGW TunnelGateway -> TunnelData
Participant TunnelData forwarding
```

No reference switch and no host/network workaround are justified by the source.

Correct method:

```text
temporary pinned Emissary checkout
 -> add i2pr crates as emissary-core dev-dependencies
 -> add one test inside emissary-core/src/tunnel/tests/
 -> compile `cargo test -p emissary-core --lib ...`
 -> use native #[cfg(test)] harness + production handlers
```

See [`117-terminal-native-reference-correction.md`](117-terminal-native-reference-correction.md).

---

## Current required native path

The terminal test must prove:

```text
i2pr outbound short build
 -> Emissary native OBEP admission
 -> i2pr outbound Established via explicit test-only pre-Garlic Q2 reply boundary

and

i2pr inbound short build
 -> Emissary native IBGW admission
 -> Emissary native Participant admission
 -> i2pr inbound Established

then

i2pr RouterInfo publication
 -> corrected i2pr outbound TunnelData
 -> Emissary production OBEP
 -> Emissary native floodfill accepts RouterInfo K

then

i2pr DatabaseLookup(K), selected peer F
 -> corrected i2pr outbound TunnelData
 -> Emissary production OBEP
 -> native Emissary floodfill lookup
 -> native TunnelDeliveryViaRoute(I, inbound_tunnel) observed
 -> Emissary native IBGW
 -> Emissary native Participant
 -> final TunnelData to i2pr endpoint
 -> DatabaseStore(K)
 -> i2pr validation/store
 -> i2pr lookup Success
```

The native floodfill's own outbound exploratory tunnel is **not** required for
this checkpoint. The test may observe its native `TunnelDeliveryViaRoute` and
begin the native inbound transit path at the corresponding TunnelGateway. This
keeps the test focused on i2pr interoperability rather than testing an Emissary
creator-side tunnel pool against itself.

The boundary must be recorded explicitly.

## Terminal correction execution result

The temporary checkout was pinned to `eepnet/emissary@9b43484a21d5a1291c4881cdae62a36c527f8c0f` and compiled `emissary-core`'s own `#[cfg(test)]` target with the i2pr crates as dev-dependencies. The focused native test command was:

```text
cargo test --manifest-path /tmp/i2pr-plan117-native.2G7uME/emissary/Cargo.toml \
  -p emissary-core --lib \
  tunnel::tests::i2pr_plan117_native::i2pr_plan117_native_mixed_router_netdb \
  -- --exact --nocapture
```

Observed bounded stages:

```text
production i2pr STBM -> native Emissary Message::parse_standard       = passed
native Emissary OBEP admission and live role                       = passed
native pre-Garlic reply shape (count=4, bytes=873)                  = passed
native reply AEAD opening with i2pr-derived context                 = passed
strict i2pr reply Mapping/response decoding                         = rejected
native publication / lookup / return path                           = not reached
```

The rejection is `ShortRecord(Protocol(LengthExceeded { context:
"reply mapping body" }))`. The pinned Emissary handler's source mutates
the encrypted request-derived record at offsets used for the reply status,
so the authenticated plaintext retains the request envelope prefix instead
of beginning with the normative reply Mapping. This is a reference-side
protocol/layout defect at the native OBEP reply boundary, not a reason to
weaken i2pr's strict reply decoder. The temporary diagnostic source edit that
copied the decrypted request body into the response was reverted before the
patch digest was recorded.

The tracked temporary Emissary patch was recorded before checkout deletion:

```text
git diff --binary bytes = 25485
git diff --binary sha256 = 3564319346ffb99ab9e933c2e8da0ea934f7003e8e9a1b9f69b45e6fa725eda7
```

This is not native mixed-router NetDB evidence. The plan remains open as
`native-reference-terminal-pending` pending a decision on the pinned
reference's reply-layout defect or a corrected pinned reference revision.

---

## Native closure gate

Plan 117 remains open until:

```text
plan_117_h_native_reference = passed-emissary-mixed-router-netdb
```

Minimum native evidence:

```text
native Emissary OBEP short-build admission                = passed
native Emissary IBGW short-build admission                = passed
native Emissary Participant short-build admission         = passed
native Emissary OBEP TunnelData processing                = passed
native Emissary floodfill RouterInfo publication          = passed
native Emissary floodfill DatabaseLookup                  = passed
native floodfill reply router/tunnel decision             = passed
native Emissary IBGW return                               = passed
native Emissary Participant return                        = passed
i2pr DatabaseStore recovery                               = passed
i2pr RouterInfo validation/store                          = passed
i2pr lookup Success                                       = passed
publication read-after-write observation                  = passed-preferred
final temporary binary patch SHA-256                      = recorded-before-deletion
```

The outbound build-reply test seam may capture the already-native transformed
raw reply immediately before Emissary's Garlic wrapping. If used:

```text
Q2_external_return_established = not-proven-test-q2-bypass
```

Do not implement general garlic in Plan 117.

---

## Phase I remains separately deferred

Current host classification remains:

```text
117-X = deferred-host-lane-unavailable
Q1_authenticated_transport = deferred
Q2_external_return_established = deferred
```

Do not rebuild namespaces, Multipass, Docker, VMs, or Python harnesses to change
this status.

A successful 117-N with 117-X still deferred is an accepted Plan 117 product
closure state.

---

## Expected final success state after the native reference decision

After the terminal native pass succeeds:

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

At that point the next router-construction plan is **not** blocked on the
unavailable authenticated transport lane. The external evidence gap remains
tracked separately.

Until then:

```text
plan_117_h_native_reference           = blocked-emissary-native-reply-layout
plan_117                              = native-reference-terminal-pending
next_roadmap_plan                     = blocked-on-one-native-reference-decision
```
