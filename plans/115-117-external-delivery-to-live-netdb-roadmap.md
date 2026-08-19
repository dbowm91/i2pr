# Plans 115-117 roadmap — terminal campaign record

> **Terminal disposition (Plan 118, 2026-08-19).** This roadmap
> is now a **completed/terminal campaign record**. It documents
> the Plan 115-117 evidence and exact native-stage failure, not an
> active execution chain. The active transport-neutral
> router-construction sequence is governed by
> [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).
> Do not start a new Plan 115-117 closure pass.

## Historical status

- Date: 2026-08-19.
- Plan 115 independent native short-build Q0: **passed-emissary-q0-construction-and-obep-reply-only**.
- Plan 116 local TunnelData data plane: **passed-final-local-closure**.
- Plan 117 local production composition: **passed-all-i2pr-production-seam-netdb** (Phase G).
- Plan 117 parser compatibility with pinned Emissary: **passed-emissary-wire-format-compatibility** (Phase H historical).
- Plan 117 corrected native reference composition: **blocked-reference-defect** (Phase H corrected attempt).
- Plan 117 authenticated transport: **deferred-host-lane-unavailable** (Phase I).
- Current Plan 117 status: [`117-status.md`](117-status.md).
- Current Plan 117 handoff: [`117-handoff.md`](117-handoff.md).
- Plan 118 plan-of-record: [`118-planning-authority-cleanup-and-plan117-disposition.md`](118-planning-authority-cleanup-and-plan117-disposition.md).
- Next router-construction plan: **Plan 119** (LeaseSet2 protocol foundation).

## Closed authority

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
plan_117_h_native_reference           = blocked-reference-defect
plan_117_i_authenticated_transport    = deferred-host-lane-unavailable
plan_117                              = closed-for-progression-with-evidence-gap
Q1_authenticated_transport            = deferred
Q2_external_return_established        = deferred
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
router_construction                   = may-continue
```

The corrected in-tree Emissary test reached native OBEP admission
and reply AEAD opening, but strict i2pr reply decoding rejected
the pinned reference's request-prefixed reply plaintext. The
failure is recorded in [`117-status.md`](117-status.md); no parser
relaxation was made. The reference-side defect is reproducible
and localized to the pinned Emissary revision
`9b43484a21d5a1291c4881cdae62a36c527f8c0f`; no upstream correction
is available. The Plan 117 terminal disposition is
`closed-for-progression-with-evidence-gap`, which is the Plan 118
Phase B Outcome 2.

This roadmap separates local router functionality, native
cross-implementation evidence, and authenticated external
transport. Host limitations defer the last category; they do not
repeat block transport-neutral router construction after the
first two are green.

---

## Gate 115 — independent short-build consumer (closed)

Status: **closed for progression**.

Authority:

- [`115-status.md`](115-status.md)
- [`115-handoff.md`](115-handoff.md)
- [`115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md)

Pinned upstream Emissary (`9b43484a21d5a1291c4881cdae62a36c527f8c0f`,
`emissary-core 0.4.0`) independently consumed a production-generated
i2pr ShortTunnelBuild through its native short-build handler and
reached OBEP reply composition.

```text
Q0 independent native short-build consumption = passed
Q1 authenticated transport                    = deferred
Q2 live reply -> Established                   = deferred
```

This established that pinned Emissary is a useful independent native
reference for the Plan 115 Q0 milestone without requiring daemon
startup, sockets, namespaces, or public I2P. It does **not** claim
native mixed-router NetDB acceptance; the Plan 117 native
mixed-router closure remained pending at the time the Plan 115
record was produced.

---

## Gate 116 — local tunnel data plane (closed)

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

Ordered and out-of-order exact-byte trajectories passed. Duplicate
accounting and delivery metadata integrity were corrected before
closure.

---

## Gate 117 — exploratory NetDB composition (closed for progression)

Status: **closed-for-progression-with-evidence-gap**.

### 117-L — local production composition (passed)

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

The first post-landing audit found four product defects. The
corrective implementation at `b7e12e09...` fixed them:

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

Do not redo 117-L unless an independent native reference identifies
a concrete protocol defect.

### 117-N — independent native reference (blocked-reference-defect)

The pinned reference remained:

```text
repository = eepnet/emissary
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package    = emissary-core 0.4.0
```

The historical retained parser evidence:

```text
passed-emissary-wire-format-compatibility
highest stage = h_emissary_database_lookup_parsed
```

The Plan 117 terminal native reference correction attempt compiled
its in-tree test inside `emissary-core`'s own `#[cfg(test)]` build
and reached native OBEP admission, registered the live role,
returned a 4-record/873-byte pre-Garlic reply, and opened that
reply's AEAD with i2pr-derived context. Strict i2pr reply decoding
rejected the encrypted plaintext because the pinned Emissary
handler leaves the request envelope prefix in the reply body rather
than emitting the normative reply Mapping at offset zero.

No upstream correction is available against the pinned revision;
Plan 118 Phase B1 inspected Emissary history and confirmed the
remote master HEAD still equals the pinned revision. The bounded
Phase B1 upstream-correction decision is exhausted; Plan 117
closes as `closed-for-progression-with-evidence-gap` rather than
as `passed-emissary-mixed-router-netdb`.

Therefore:

```text
117-N = blocked-reference-defect (closed for progression)
```

Do not reopen 117-N to chase another harness, host lane, or
reference implementation in this roadmap.

### 117-X — authenticated transport / live-process delivery (deferred)

Status: **deferred-host-lane-unavailable**.

```text
Q1_authenticated_transport     = deferred
Q2_external_return_established = deferred
```

This remains separate from local/native product validation; see
the external acceptance debt ledger in
[`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).

---

## Anti-loop rules (preserved)

1. Environment limitations defer authenticated transport; they
   do not invalidate local/native router construction.
2. A native reference protocol rejection is different: localize
   that exact protocol defect. The Plan 117 reference defect is
   localized to the pinned Emissary reply layout; the i2pr parser
   is retained.
3. Keep pinned Emissary unless its source proves unusable for the
   documented in-tree test path. The pinned source remains usable
   for Q0 short-build consumption but its reply layout is not
   admissible as native mixed-router NetDB evidence.
4. Do not reopen Plan 116.
5. Do not redo Plan 117 C1-C5 or Phase G without new defect
   evidence.
6. Do not equate parser compatibility with native mixed-router
   success.
7. Do not equate native mixed-router success with authenticated
   Q1/Q2.
8. Do not activate normal-daemon NTCP2 merely to close a
   planning label.
9. The reference cannot be switched to Java I2P, i2pd, or any
   other permanent harness in this campaign.
10. Once this campaign is closed, do not start a new Plan 117
    native closure campaign; subsequent router construction is
    governed by the Milestone 6 roadmap.
