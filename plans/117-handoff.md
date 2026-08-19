# Plan 117 handoff — terminal disposition to Plan 118 / Plan 119

- Status: **closed-for-progression-with-evidence-gap**
- Date: 2026-08-19
- Plan-of-record: [`118-planning-authority-cleanup-and-plan117-disposition.md`](118-planning-authority-cleanup-and-plan117-disposition.md)
- Status authority: [`117-status.md`](117-status.md)
- Original plan: [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md)
- Prior corrective pass: [`117-corrective-closure.md`](117-corrective-closure.md)
- Predecessor Plan 116: **passed-final-local-closure**
- Source floor: `99374cf498227cf8ab1c4ec6ec4216b5d4d2e08e`
- Pinned independent reference: `eepnet/emissary@9b43484a21d5a1291c4881cdae62a36c527f8c0f`, `emissary-core 0.4.0`

## Terminal handoff

Plan 117 is closed for progression. The terminal disposition is
`closed-for-progression-with-evidence-gap`:

```text
plan_117_local_composition         = passed-all-i2pr-production-seam-netdb
plan_117_native_reference          = blocked-reference-defect
plan_117_external_transport        = deferred-host-lane-unavailable
plan_117                           = closed-for-progression-with-evidence-gap
router_construction                = may-continue
next_router_construction_plan      = Plan 119 (LeaseSet2 protocol foundation)
normal_daemon_ntcp2                = disabled-and-unenableable
ntcp2                              = experimental-non-advertised
```

The next executable plan is **Plan 119** (LeaseSet2 protocol
foundation) under the Milestone 6 router-construction roadmap in
[`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).
Router construction is **not** blocked on the unavailable
authenticated external transport lane.

The Plan 117 evidence gap is tracked separately under the
external acceptance debt ledger in the same Milestone 6 roadmap.

## What is preserved

The Plan 117 product floor is retained and is the implementation
basis for the Milestone 6 plan sequence:

```text
C1 floodfill-vs-key routing                passed
C2 outer TunnelData short transport        passed
C3 activation metadata/secrets split       passed
C4 registry-derived readiness              passed
C5 regressions                              passed
117-G all-i2pr production-seam NetDB       passed
117-H historical parser compatibility       passed-emissary-wire-format-compatibility
```

The bounded native reference test reached native Emissary OBEP
admission, registered the live role, returned a pre-Garlic reply,
and opened that reply's AEAD with i2pr-derived context. Strict
i2pr reply decoding rejected the pinned reference's
request-prefixed reply plaintext. The reference-side defect is
localized to the pinned Emissary revision
(`9b43484a21d5a1291c4881cdae62a36c527f8c0f`); no upstream correction
is available, and no i2pr parser relaxation was made.

## What is not advanced

- No native mixed-router NetDB acceptance is claimed.
- No Java I2P, i2pd, or new harness direction is introduced.
- No general garlic implementation is started.
- No normal-daemon NTCP2 activation is performed.
- No Python interop harness is rebuilt.
- No Multipass, Docker, rootless namespace, or QEMU lane is retried.

## Historical execution notes (preserved as audit context)

The Plan 117 execution sequence was documented in
[`117-terminal-native-reference-correction.md`](117-terminal-native-reference-correction.md)
and the earlier corrective closure at
[`117-corrective-closure.md`](117-corrective-closure.md). The
preserved execution details (N0–N10 stages, native reference
placement, evidence integrity requirements, fresh attempt budget)
remain valid documentation of how the bounded native reference was
exercised and where it stopped. Future plans that need to
revisit this surface must read the closure record in
[`117-status.md`](117-status.md) first.

## Forbidden scope for follow-on plans

```text
open a new Plan 117 native closure campaign
rewrite Milestone 5 to revive the Plan 046/048/049/050/051 lanes
relax the i2pr ShortReplyRecord decoder to match the pinned Emissary layout
switch to Java I2P or i2pd as a permanent harness
add a new permanent Emissary adapter crate
enable normal-daemon NTCP2 to make Milestone 6 tests look more real
introduce another tunnel-build validation harness
```

## External acceptance debt (tracked separately)

The following items remain visible but do not block the Milestone 6
plan sequence; see section 8 of
[`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md)
for the bounded debt ledger:

```text
Q1_authenticated_transport           = deferred
Q2_external_return_established       = deferred
live exploratory tunnel pair          = deferred
live RouterInfo publication/lookup    = deferred
live LeaseSet2 publication/lookup     = future
independent destination ECIES interop = future M6 acceptance
independent streaming interop         = future M6 acceptance
```

Each debt item is classified on
`blocks_M6_product_construction = false` at the current product
floor. The next executable implementation plan is **Plan 119**.
