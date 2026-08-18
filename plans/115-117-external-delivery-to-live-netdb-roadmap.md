# Plans 115-117 roadmap: independent short-build -> local data plane -> exploratory NetDB composition

## Status

- Date: 2026-08-18.
- Parent roadmap: [`000-mvp-roadmap.md`](000-mvp-roadmap.md).
- Plan 115 independent native short-build Q0: **passed**.
- Plan 116 local TunnelData data plane: **passed-final-local-closure**.
- Plan 117 exploratory/NetDB composition: **corrective-closure-required**.
- Original Plan 117: [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md).
- Corrective Plan 117: [`117-corrective-closure.md`](117-corrective-closure.md).
- Plan 117 handoff: [`117-handoff.md`](117-handoff.md).
- Plan 117 status: [`117-status.md`](117-status.md).

The sequence deliberately separates protocol construction, local router functionality, independent native evidence, and authenticated external transport so host limitations cannot repeatedly block unrelated router construction.

## Current authority

```text
plan_115                              = passed-emissary-q0-construction-and-obep-reply-only
Q0_native_emissary                    = passed
plan_116                              = passed-final-local-closure
plan_117                              = corrective-closure-required
plan_117_local_composition            = pending-corrective-closure
plan_117_native_reference             = pending
plan_117_authenticated_transport      = deferred-until-local-native-pass
Q1_authenticated_transport            = deferred
Q2_external_return_established        = deferred
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
router_construction                   = active-plan117-current-line
next_roadmap_plan                     = blocked-until-plan117-local-native-closure
```

---

# Gate 115 — independent short-build evidence

Status: **closed for progression**.

Authority:

- [`115-status.md`](115-status.md)
- [`115-handoff.md`](115-handoff.md)
- [`115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md)

Pinned upstream Emissary independently consumed the production-generated i2pr ShortTunnelBuild and reached native OBEP reply construction.

```text
Q0 independent native consumption = passed
Q1 authenticated transport        = deferred
Q2 live reply -> Established       = deferred
```

Do not add another Plan 115 validation pass without new concrete protocol-defect evidence.

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
ShortBuild Established
 -> real established secret/key ownership
 -> exploratory-pool registration
 -> one-shot EstablishedMaterial transfer
 -> TunnelData preprocessing
 -> bounded fragmentation/reassembly
 -> AES tunnel layer/IV transformations
 -> outbound gateway
 -> participant(s)
 -> outbound endpoint
 -> ROUTER or TUNNEL delivery
 -> inbound gateway
 -> inbound participant(s)
 -> local inbound endpoint
 -> exact reconstructed I2NP message
```

The terminal closure includes ordered and out-of-order exact-byte outbound-to-inbound trajectories, bounded duplicate accounting, and delivery-metadata integrity.

Do not reopen Plan 116 merely because Plan 117 first-pass composition used a Plan 116 primitive incorrectly.

A non-blocking Plan 116 hardening item remains available for future opportunistic cleanup: reject a new fragment sequence greater than an already-declared terminal `is_last=true` sequence. It does not block Plan 117.

---

# Gate 117 — exploratory tunnel / NetDB composition

Status: **execute the corrective closure now**.

Corrective plan:
[`117-corrective-closure.md`](117-corrective-closure.md)

Handoff:
[`117-handoff.md`](117-handoff.md)

## Objective

Join the short-build, exploratory-pool, TunnelData, NetDB, publication, and transport ownership surfaces into the real router product path:

```text
validated/reseeded RouterInfo store
 -> real inbound + outbound exploratory short builds
 -> registered public tunnel metadata + one-shot secret material
 -> activated local tunnel roles
 -> state-machine-owned DatabaseLookup
 -> outbound exploratory TunnelData
 -> selected floodfill handling
 -> reply through the declared inbound exploratory path
 -> local inbound endpoint
 -> RouterInfo validation/store
 -> local RouterInfo publication
 -> independent publication observation
```

Direct `DatabaseLookup` over NTCP2 remains forbidden as a substitute for this path.

## First A-F implementation state

The first Plan 117 pass successfully added:

```text
typed DatabaseLookup ownership
publication DatabaseStore ownership
RouterInfo gzip publication body
post-reply-path lookup advancement
bounded DataPlaneRegistry
outbound lookup/publication composition helpers
inbound TunnelData dispatch
```

Those surfaces are retained.

Post-landing audit found four blocking product defects:

```text
C1 lookup Tunnel ROUTER target uses lookup key K instead of selected floodfill F
C2 DeliveryRequest contains raw 1028-byte TunnelData body instead of complete short-transport I2NP type 18
C3 pool activation removes public registration/reply-path metadata while moving secrets
C4 NetDbSeam dispatch readiness is a caller-set boolean rather than real registry state
```

Phases G and H also remain unexecuted.

Therefore the first A–F landing is **partial implementation**, not Plan 117 closure.

---

# Gate 117 evidence decomposition

Keep these three claims separate:

```text
117-L  local production composition
117-N  independent native reference composition
117-X  authenticated transport / live-process delivery
```

This distinction is mandatory.

## 117-L — mandatory local product closure

117-L closes only after the corrective routing/framing/activation/readiness work and Phase G pass.

Critical identity invariant:

```text
K = requested RouterInfo key
F = selected floodfill
P = outbound first hop

DatabaseLookup.key      = K
Tunnel ROUTER target    = F
DeliveryRequest.target  = P
```

Critical framing invariant:

```text
nested DatabaseLookup / DatabaseStore
    = standard 16-byte I2NP header

transport-facing TunnelData
    = I2NP type 18 using NTCP2/SSU2 short-transport header
```

Critical ownership invariant:

```text
ExploratoryPool retains public registration/routing/lifetime metadata
DataPlaneRegistry owns activated secret-bearing role material
```

Activation must not make the inbound reply path disappear.

Phase G must begin from successful production short-build state machines and real registered `EstablishedMaterial`, not creator-side fabricated `EstablishedTunnel` fixtures.

Phase G success label:

```text
117-L = passed-all-i2pr-production-seam-netdb
```

## 117-N — mandatory pinned in-process Emissary native checkpoint

Use exactly:

```text
upstream Emissary
eepnet/emissary
revision 9b43484a21d5a1291c4881cdae62a36c527f8c0f
emissary-core 0.4.0
```

The native checkpoint is intentionally **in-process**.

It does not require:

```text
rootless namespace
Multipass
Docker
VM
public I2P
permanent Python harness
```

The current first-pass status conclusion that Phase H requires the Plan 046/048/049 host lane is superseded by the corrective plan.

Use a temporary Emissary checkout and a test-only reference patch. Record the final patch SHA-256 before deletion.

Minimum native path:

```text
i2pr creator outbound build
 -> Emissary native OBEP short-build acceptance
 -> corrected i2pr short-transport TunnelData
 -> Emissary production OBEP TunnelData processing
 -> standard DatabaseLookup for K, routed to F
 -> Emissary native floodfill handler
 -> DatabaseStore response
 -> Emissary native IBGW / participant path
 -> i2pr local inbound endpoint
 -> i2pr RouterInfo validation/store
 -> RouterInfoLookup Success
```

Also require native acceptance of i2pr RouterInfo publication and record the strongest real independent observation available.

A test-only Emissary Q2 decapsulation seam is acceptable if explicitly classified as not proving live external Q2.

Phase H success label:

```text
117-N = passed-emissary-mixed-router-netdb
```

## 117-X — authenticated transport checkpoint

117-X is attempted only after 117-L and 117-N pass.

Use an **existing qualified lane only**.

If one is runnable:

```text
run one bounded authenticated delivery probe
 -> record Q1 and Q2 separately
```

If the current host cannot provide the lane:

```text
117-X = deferred-host-lane-unavailable
Q1_authenticated_transport = deferred
Q2_external_return_established = deferred-or-not-proven
```

Stop external work at that point.

Do not rebuild rootless namespace, Multipass, Docker, VM, or Python harness infrastructure to change the result.

A host limitation is not evidence of an I2P protocol defect.

---

# Gate 117 corrective product order

Execute in this order:

```text
C1  correct selected floodfill vs lookup-key ROUTER identity
C2  encode complete short-transport TunnelData at authenticated-link boundary
C3  preserve public pool metadata while transferring secrets once
C4  derive dispatch readiness from the real DataPlaneRegistry
C5  focused regression suite
G   all-i2pr production-seam terminal lookup/publication
H   pinned in-process Emissary mixed-router lookup/publication
I   authenticated transport classification only if existing lane is runnable
J   synchronize status / architecture / support authority
```

Do not start H before G.

Do not start I before G and H.

---

# Gate 117 security invariants

1. `DatabaseLookup.key` remains the requested NetDB key.
2. The outbound tunnel ROUTER destination is the selected floodfill.
3. `DeliveryRequest.target` is the outbound first hop.
4. DatabaseLookup must use an outbound exploratory tunnel.
5. Reply routing names the real inbound exploratory gateway/tunnel.
6. Nested NetDB messages use the standard I2NP header.
7. Transport-facing TunnelData uses the repository's short-transport I2NP codec.
8. TunnelData outer message IDs are hop-local/fresh.
9. Activated inbound tunnels remain reply-path selectable until expiry/failure.
10. Layer keys transfer once and are not persistently cloned to retain metadata.
11. Pool lifetime/capacity/duplicate rules continue after activation.
12. Dispatch readiness derives from actual usable runtime roles.
13. Unknown inbound TunnelData IDs fail closed.
14. RouterInfo validation/decompression/store bounds remain in force.
15. Publication queueing is not equivalent to independent publication observation.
16. `ReplyEncryption::None` remains limited to this first RouterInfo-only integration checkpoint.
17. Normal-daemon NTCP2 remains disabled unless separately authorized later.
18. Raw secrets/tunnel plaintext are never placed in evidence files.

---

# Gate 117 completion states

## Local + native pass, authenticated host lane unavailable

This is the expected successful product-construction closure on the current constrained environment:

```text
plan_117_corrective_routing          = passed
plan_117_transport_framing           = passed-short-transport-tunneldata
plan_117_activation_ownership        = passed-metadata-retained-secrets-once
plan_117_runtime_readiness           = passed-registry-derived
plan_117_local_composition           = passed-all-i2pr-production-seam-netdb
plan_117_native_reference            = passed-emissary-mixed-router-netdb
plan_117_authenticated_transport     = deferred-host-lane-unavailable
plan_117                              = local-native-complete-external-deferred
milestone4b_authenticated_external    = blocked
router_construction                   = may-continue
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
```

This permits the roadmap to move into later transport-neutral/local router construction while tracking authenticated external evidence independently.

## Local + native + authenticated transport pass

```text
plan_117_local_composition            = passed-all-i2pr-production-seam-netdb
plan_117_native_reference             = passed-emissary-mixed-router-netdb
plan_117_authenticated_transport      = passed
plan_117                              = passed-qualified-exploratory-netdb-integration
milestone4b_authenticated_external    = eligible-for-closure-review
```

Do not infer normal-daemon production transport support merely from the test lane.

## Native reference exposes a protocol defect

Record the exact highest native stage and localize that one defect.

Do not create another broad validation branch.

---

# After Gate 117

Once 117-L and 117-N are green, the next local router-construction line may move into the destination layer even if 117-X remains host-deferred:

```text
Destination lifecycle
 -> destination tunnel pools
 -> garlic
 -> LeaseSet creation/publication/lookup
 -> local destination routing
 -> minimal streaming
 -> independent destination interoperability
```

SAM and I2CP remain downstream of a functioning destination/streaming core.

The authenticated external transport gap remains tracked independently until a suitable host lane exists.

---

# Anti-loop and artifact rules

1. Environment blockers defer authenticated transport claims; they do not erase successful local/native construction.
2. An affirmative independent protocol rejection is different: localize that specific defect.
3. Prefer production Rust composition tests over orchestration.
4. Do not rebuild the historical Python/NTCP2 harness for Plan 117.
5. Phase H is in-process native Emissary and is not host-lane dependent.
6. Use one pinned independent implementation unless a concrete ambiguity requires otherwise.
7. Keep temporary reference patches small and record their digest before deletion.
8. Keep evidence sanitized: stages, types, lengths, hashes, and outcomes rather than raw traffic/secrets.
9. Do not reopen Plan 116 merely because Plan 117 composition was wrong.
10. Do not activate normal-daemon NTCP2 merely to make Gate 117 convenient.
11. Close Plan 117 as local/native complete when appropriate instead of creating another environment-driven closure loop.
