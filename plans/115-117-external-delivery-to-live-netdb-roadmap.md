# Plans 115-117 roadmap — independent short build -> local data plane -> native exploratory NetDB

## Current status

- Date: 2026-08-19.
- Parent roadmap: [`000-mvp-roadmap.md`](000-mvp-roadmap.md).
- Plan 115 independent native short-build Q0: **passed**.
- Plan 116 local TunnelData data plane: **passed-final-local-closure**.
- Plan 117 local production composition: **passed through Phase G**.
- Plan 117 parser compatibility with pinned Emissary: **passed**.
- Plan 117 full native reference composition: **blocked at pinned Emissary reply layout**.
- Plan 117 authenticated transport: **deferred-host-lane-unavailable**.
- Current Plan 117 execution file:
  [`117-terminal-native-reference-correction.md`](117-terminal-native-reference-correction.md).
- Current handoff: [`117-handoff.md`](117-handoff.md).
- Current status: [`117-status.md`](117-status.md).

## Current authority

```text
plan_115                              = passed-emissary-q0-construction-and-obep-reply-only
Q0_native_emissary                    = passed
plan_116                              = passed-final-local-closure
plan_117_c1_routing                   = passed
plan_117_c2_transport_framing         = passed-short-transport-tunneldata
plan_117_c3_activation_ownership      = passed-metadata-retained-secrets-once
plan_117_c4_runtime_readiness         = passed-registry-derived
plan_117_g_local_production_seam      = passed-all-i2pr-production-seam-netdb
plan_117_h_parser_compatibility       = passed-emissary-wire-format-compatibility
plan_117_h_native_reference           = blocked-emissary-native-reply-layout
plan_117_i_authenticated_transport    = deferred-host-lane-unavailable
plan_117                              = native-reference-terminal-pending
Q1_authenticated_transport            = deferred
Q2_external_return_established        = deferred
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
router_construction                   = hold-on-one-native-reference-decision
```

This roadmap continues to separate local router functionality, native
cross-implementation evidence, and authenticated external transport. Host
limitations may defer the last category; they must not repeatedly block
unrelated router construction after the first two are green.

The corrected in-tree Emissary test reached native OBEP admission and reply
AEAD opening, but strict i2pr reply decoding rejected the pinned reference's
request-prefixed reply plaintext. The failure is recorded in
[`117-status.md`](117-status.md); parser-only compatibility remains historical
evidence and is not promoted to native mixed-router evidence.

---

# Gate 115 — independent short-build consumer

Status: **closed for progression**.

Authority:

- [`115-status.md`](115-status.md)
- [`115-handoff.md`](115-handoff.md)
- [`115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md)

Pinned upstream Emissary independently consumed a production-generated i2pr
ShortTunnelBuild through its native short-build handler and reached OBEP reply
composition.

```text
Q0 independent native short-build consumption = passed
Q1 authenticated transport                    = deferred
Q2 live reply -> Established                   = deferred
```

This established that pinned Emissary is a useful independent native reference
without requiring daemon startup, sockets, namespaces, or public I2P.

Do not reopen Plan 115 without a new affirmative short-build protocol defect.

---

# Gate 116 — local tunnel data plane

Status: **closed**.

Authority:

- [`116-status.md`](116-status.md)
- [`116-handoff.md`](116-handoff.md)
- [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- [`116-completion-correction.md`](116-completion-correction.md)
- [`116-final-closure.md`](116-final-closure.md)
- [`116-terminal-cleanup.md`](116-terminal-cleanup.md)

Plan 116 provides the runtime-neutral local tunnel data plane:

```text
successful ShortBuild
 -> real established secret/key ownership
 -> exploratory pool registration
 -> one-shot EstablishedMaterial transfer
 -> TunnelData preprocessing
 -> bounded fragmentation/reassembly
 -> AES layer/IV transforms
 -> outbound gateway
 -> participant(s)
 -> outbound endpoint
 -> ROUTER / TUNNEL delivery
 -> inbound gateway
 -> inbound participant(s)
 -> local endpoint
 -> exact nested I2NP recovery
```

Ordered and out-of-order exact-byte trajectories passed. Duplicate accounting
and delivery metadata integrity were corrected before closure.

Do not reopen Plan 116 because later composition code misuses a Plan 116 API.

A small non-blocking fragment terminal-sequence hardening item remains a future
cleanup opportunity and does not gate Plan 117.

---

# Gate 117 — exploratory NetDB composition

Status: **terminal native-reference correction pending**.

## 117-L — local production composition

Status: **passed**.

The first Plan 117 implementation added:

```text
typed DatabaseLookup ownership
typed publication DatabaseStore ownership
RouterInfo gzip publication
post-reply-path lookup advancement
bounded DataPlaneRegistry
outbound lookup/publication composition
inbound TunnelData dispatch
```

The first post-landing audit found four product defects. The corrective
implementation at `b7e12e09...` fixed them:

```text
C1 lookup tunnel ROUTER destination K -> selected floodfill F
C2 raw TunnelData body -> complete short-transport I2NP type 18
C3 activation removes metadata -> metadata retained, secrets transferred once
C4 sticky readiness flag -> production registry-derived readiness
```

Phase G then passed a production-origin all-i2pr trajectory:

```text
successful outbound/inbound short builds
 -> real EstablishedMaterial
 -> ExploratoryPool registration
 -> activation
 -> DataPlaneRegistry
 -> DatabaseLookup K to floodfill F
 -> outbound TunnelData
 -> simulated remote response
 -> activated inbound TunnelData
 -> DatabaseStore recovery
 -> RouterInfo validation/store
 -> RouterInfoLookup Success
```

Therefore:

```text
117-L = passed-all-i2pr-production-seam-netdb
```

Do not redo 117-L unless the independent native reference identifies a concrete
protocol defect.

---

## 117-N — independent native reference

Status: **pending terminal corrected method**.

Pinned reference remains:

```text
repository = eepnet/emissary
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package    = emissary-core 0.4.0
```

### Retained parser evidence

The first Phase H attempt proved:

```text
passed-emissary-wire-format-compatibility
highest stage = h_emissary_database_lookup_parsed
```

That evidence is retained. It does not equal native mixed-router NetDB
acceptance.

### Blocker research correction

The parser-only attempt used a separate helper crate with `emissary-core` as a
normal path dependency.

Pinned Emissary's critical native reference helpers are compiled only as part of
`emissary-core`'s own `#[cfg(test)]` build:

```text
TestTransitTunnelManager
make_router
connect_routers
TunnelPoolHandle::create
TunnelMessage test exports
GarlicHandler test exports
```

The pinned production code already contains:

```text
native short-build IBGW / Participant / OBEP admission
SubsystemManager routing to installed tunnels and NetDb
OBEP TunnelData decrypt/checksum/fragment/nested-I2NP processing
native floodfill DatabaseStore and DatabaseLookup handling
IBGW TunnelGateway -> TunnelData
participant TunnelData forwarding
```

Therefore the reference is suitable; the **test placement** was wrong.

Correct method:

```text
fresh temporary pinned Emissary checkout
 -> i2pr crates as emissary-core dev-dependencies
 -> one temporary test inside emissary-core/src/tunnel/tests/
 -> compile emissary-core with cfg(test)
 -> exercise native production handlers in-process
```

No reference switch is currently justified.

### Required native path

The terminal checkpoint must prove:

```text
i2pr outbound short build -> Emissary native OBEP

i2pr inbound short build -> Emissary native IBGW -> Emissary native Participant

i2pr RouterInfo publication
 -> i2pr outbound TunnelData
 -> Emissary native OBEP
 -> Emissary native floodfill accepts RouterInfo K

i2pr DatabaseLookup K for selected floodfill F
 -> i2pr outbound TunnelData
 -> Emissary native OBEP
 -> Emissary native floodfill handles lookup
 -> native floodfill chooses i2pr declared inbound reply router/tunnel
 -> Emissary native IBGW
 -> Emissary native Participant
 -> i2pr local endpoint
 -> DatabaseStore K
 -> i2pr RouterInfo validation/store
 -> lookup Success
```

The required label remains:

```text
117-N = passed-emissary-mixed-router-netdb
```

### Explicit test boundaries

Two test-only boundaries are permitted and must be recorded honestly.

1. **Outbound short-build Garlic reply**

The temporary Emissary test may capture the already-native transformed raw
count-prefixed build reply immediately before the OBEP wraps it in Garlic.
Those exact native reply bytes may be fed to i2pr's BuildReply event.

```text
Q2_external_return_established = not-proven-test-q2-bypass
```

This avoids implementing general garlic solely for Plan 117.

2. **Floodfill response outbound routing**

The native floodfill's own exploratory pool handle may be observed at:

```text
TunnelMessage::TunnelDeliveryViaRoute {
    router_id = i2pr inbound gateway,
    tunnel_id = i2pr reply tunnel,
    message = native DatabaseStore
}
```

The test may convert this native routing decision directly into the
corresponding TunnelGateway injected into the Emissary IBGW.

```text
reference_floodfill_response_routing = native-TunnelDeliveryViaRoute-observed
reference_floodfill_outbound_exploratory_tunnel = bypassed-at-test-routing-boundary
```

Building an additional Emissary outbound exploratory tunnel would mostly test
Emissary against itself and is not required for the i2pr cross-implementation
claim.

### Publication observation

Publish RouterInfo K first, then perform the lookup K. Preferred native evidence:

```text
publication accepted by native Emissary floodfill
 -> later lookup K
 -> native floodfill returns DatabaseStore for K
```

Record this as read-after-write evidence.

### Evidence integrity

The corrected pass must record one final:

```text
git diff --binary patch byte length
patch SHA-256
```

before deleting the temporary checkout.

The individual file hashes from the parser-only attempt remain historical but
do not satisfy this terminal evidence requirement.

See [`117-terminal-native-reference-correction.md`](117-terminal-native-reference-correction.md)
for exact implementation and acceptance criteria.

---

## 117-X — authenticated transport / live-process delivery

Status: **deferred-host-lane-unavailable**.

```text
Q1_authenticated_transport     = deferred
Q2_external_return_established = deferred
```

This remains separate from local/native product validation.

Do not rebuild:

```text
rootless namespace infrastructure
Multipass / VMs
Docker
Python interop harnesses
public-network participation
```

to change this classification during Plan 117.

---

# Current execution order

```text
N0  add TunnelSlot-based inbound registry cleanup
N1  fresh pinned Emissary checkout
N2  compile test inside emissary-core cfg(test)
N3  native outbound/inbound short-build roles
N4  native publication through Emissary OBEP -> floodfill
N5  native lookup through Emissary OBEP -> floodfill
N6  observe native tunnel reply route
N7  native Emissary IBGW -> Participant -> i2pr endpoint
N8  i2pr RouterInfo validation/store -> lookup Success
N9  read-after-write publication evidence
N10 record final patch SHA-256 before cleanup
N11 synchronize authority
```

No other Plan 117 work is authorized absent new defect evidence.

---

# Plan 117 success state

If 117-N passes while authenticated transport remains host-blocked:

```text
plan_117_local_composition           = passed-all-i2pr-production-seam-netdb
plan_117_native_reference            = passed-emissary-mixed-router-netdb
plan_117_authenticated_transport     = deferred-host-lane-unavailable
Q1_authenticated_transport           = deferred
Q2_external_return_established       = deferred
plan_117                             = local-native-complete-external-deferred
milestone4b_authenticated_external   = blocked
router_construction                  = may-continue
normal_daemon_ntcp2                  = disabled-and-unenableable
ntcp2                                = experimental-non-advertised
```

**The next router-construction plan is unblocked in this state.**

Do not set:

```text
next_roadmap_plan = blocked-until-external-delivery-lane-available
```

That would recreate the environment validation loop this roadmap was explicitly
designed to prevent.

Until 117-N passes:

```text
next_roadmap_plan = blocked-on-plan117-native-terminal-pass
```

---

# After Plan 117

Once 117-L and 117-N are green, subsequent transport-neutral/local router work
may proceed even if 117-X remains deferred.

The next product layer may move toward:

```text
Destination lifecycle
 -> destination tunnel pools
 -> garlic
 -> LeaseSet creation/publication/lookup
 -> local destination routing
 -> minimal streaming
```

SAM and I2CP remain downstream of a functioning destination/streaming core.

Authenticated external transport remains an independently tracked evidence gap
until the environment provides a qualified execution lane.

---

# Anti-loop rules

1. Environment limitations defer authenticated transport; they do not invalidate
   local/native router construction.
2. A native reference protocol rejection is different: localize that exact
   protocol defect.
3. Keep pinned Emissary unless its source proves unusable for the documented
   in-tree test path.
4. Prefer one temporary Rust test inside the reference implementation over any
   orchestration framework.
5. Do not reopen Plan 116.
6. Do not redo Plan 117 C1-C5 or Phase G without new defect evidence.
7. Do not equate parser compatibility with native mixed-router success.
8. Do not equate native mixed-router success with authenticated Q1/Q2.
9. Do not activate normal-daemon NTCP2 merely to close the planning label.
10. Once 117-N passes, move router construction forward and retain 117-X as a
    separate deferred evidence item.
