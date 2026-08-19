# Plan 118 — planning-authority cleanup and Plan 117 terminal disposition

## Status

- **Ready for execution**.
- Date: **2026-08-19**.
- Source floor: `99374cf498227cf8ab1c4ec6ec4216b5d4d2e08e`.
- Predecessor: Plan 117 (`native-reference-terminal-pending`).
- Product implementation floor retained from Plan 117:
  `117-G = passed-all-i2pr-production-seam-netdb`.
- Authenticated external transport remains:
  `deferred-host-lane-unavailable`.
- This is a **planning/documentation and terminal-classification pass**, not a new protocol implementation campaign.

## Purpose

The repository has reached a point where the historical planning structure is
starting to obscure the implemented router surface. Plans 111-117 were useful
corrective work: they localized and fixed real short-build, tunnel-data,
fragmentation, routing, activation, and NetDB-composition defects. The remaining
Plan 117 failure is now materially different from those defects.

The corrected native-reference test reaches all of the following before it
stops:

```text
i2pr production ShortBuildStateMachine
 -> Emissary native short-build handler
 -> native OBEP admission
 -> live OBEP role registration
 -> native transformed short-build reply
 -> i2pr-derived reply AEAD opening
 -> strict i2pr ShortReplyRecord decode
```

The final step rejects the pinned Emissary reply plaintext because the reference
handler retains request-derived bytes ahead of the normative reply Mapping.
No i2pr protocol relaxation was made. Publication / lookup / inbound return were
therefore not reached in that particular native test.

This result must remain visible as an interoperability evidence gap. It must not
turn into another unbounded harness or corrective-plan loop, and it must not
block transport-neutral router construction indefinitely.

Plan 118 therefore has two jobs:

1. collapse the planning authority into a small, unambiguous current-state
   model; and
2. make one bounded disposition decision for the Plan 117 reference defect so
   Milestone 6 router construction can begin without falsely claiming external
   interoperability that was not demonstrated.

---

# 1. Current state that Plan 118 must preserve

The following implementation results are **not reopened** by this plan:

```text
Plan 115 independent native short-build consumer     passed Q0
Plan 116 local tunnel data plane                      passed-final-local-closure
Plan 117 C1 floodfill-vs-key routing                  passed
Plan 117 C2 transport-facing TunnelData framing       passed
Plan 117 C3 activation ownership                      passed
Plan 117 C4 registry-derived readiness                passed
Plan 117 C5 regressions                               passed
Plan 117 G all-i2pr production exploratory NetDB      passed
Plan 117 H parser compatibility                       passed
Plan 117 H corrected native reference                 blocked-reference-reply-layout
Plan 117 I authenticated external transport           deferred-host-lane-unavailable
normal daemon NTCP2                                   disabled-and-unenableable
NTCP2 product status                                  experimental-non-advertised
```

The all-i2pr production-seam result is sufficient evidence that subsequent
transport-neutral router layers may compose on the current tunnel / NetDB
interfaces. It is not evidence that live network interoperability exists.

Do not reopen Plan 116, redo Plan 117 C1-C5, replace the Plan 117 production
composition, or weaken `ShortReplyRecord` merely to change a planning label.

---

# 2. Target planning model

After this pass, the project should distinguish **product construction state**
from **external acceptance debt**.

Target authority:

```text
PRODUCT CONSTRUCTION
--------------------
M0-M2 foundation                       retained
M3 transport-neutral / NTCP2 code      retained experimental
M4 local NetDB machinery               retained
M5 local exploratory tunnel + NetDB    complete for progression
M6 destination / garlic / LS2          next implementation frontier

EXTERNAL ACCEPTANCE DEBT
------------------------
Q1 authenticated NTCP2 delivery        deferred-host-lane-unavailable
Q2 real external build return          deferred-host-lane-unavailable
Plan 117 native mixed-router NetDB     passed OR blocked-reference-defect
live exploratory tunnel pair           deferred
live NetDB publication / lookup        deferred
```

The dependency direction is one-way:

```text
external acceptance success may upgrade interoperability claims
external acceptance absence must not erase already-passed local product work
```

A newly discovered reproducible **i2pr protocol defect** may still block the
specific dependent product layer. A missing host lane or a localized reference
implementation defect may not.

---

# 3. Phase A — planning inventory and authority collapse

Perform a narrow inventory of the currently active planning surfaces.

At minimum inspect:

```text
plans/000-mvp-roadmap.md
plans/115-117-external-delivery-to-live-netdb-roadmap.md
plans/117-live-exploratory-netdb-integration.md
plans/117-corrective-closure.md
plans/117-terminal-native-reference-correction.md
plans/117-status.md
plans/117-handoff.md
AGENTS.md
README.md
docs/architecture/i2pr-tunnel.md
docs/protocol-support.md or the current protocol-support authority
specs/support.toml
```

Classify each statement about Plans 115-117 into one of:

```text
historical record
current product implementation state
current external evidence state
obsolete execution instruction
```

Do **not** delete historical plan files merely because they are superseded.
Historical plans are useful audit evidence. Instead, remove them from the active
execution chain by adding concise supersession / terminal-status banners where
necessary.

The active execution chain after Plan 118 must fit on one short sequence:

```text
Plan 118 authority cleanup / Plan 117 disposition
 -> Plan 119 LeaseSet2 protocol foundation
 -> Plan 120 destination lifecycle and tunnel pools
 -> Plan 121 ECIES garlic/session layer
 -> Plan 122 destination routing + NetDB composition
 -> Plan 123 minimal streaming core
```

---

# 4. Phase B — one bounded Plan 117 reference decision

This phase is deliberately small.

## B1. Check whether the reference defect has an upstream correction

Inspect Emissary history newer than the pinned revision
`9b43484a21d5a1291c4881cdae62a36c527f8c0f` specifically for the native
short-build reply construction used by the existing Plan 117 test.

The question is only:

```text
Does a newer usable Emissary revision emit the normative short-build reply
plaintext layout that i2pr already expects?
```

Do not broaden this into an Emissary audit.

If a clear correction exists:

1. pin the smallest suitable corrected revision;
2. reapply the already-defined temporary in-tree `emissary-core #[cfg(test)]`
   Plan 117 test;
3. run the exact native test trajectory;
4. permit at most one narrow i2pr correction **only if** the new run exposes a
   reproducible i2pr protocol defect against normative specification behavior;
5. record the result.

If no clear correction exists, or the corrected reference still stops because
of a demonstrable reference-side layout defect, stop. Do not switch to Java,
i2pd, another host virtualization design, or another permanent harness in this
plan.

## B2. Accepted terminal outcomes

Exactly two outcomes may unblock router construction.

### Outcome 1 — native reference passes

```text
plan_117_native_reference = passed-emissary-mixed-router-netdb
plan_117                  = local-native-complete-external-deferred
router_construction       = may-continue
```

Authenticated external transport remains deferred.

### Outcome 2 — reference defect remains localized

```text
plan_117_local_composition = passed-all-i2pr-production-seam-netdb
plan_117_native_reference  = blocked-reference-defect
plan_117_external_transport = deferred-host-lane-unavailable
plan_117                    = closed-for-progression-with-evidence-gap
router_construction         = may-continue
```

This outcome does **not** claim native mixed-router NetDB success. The evidence
ledger must retain the exact highest native stage reached and the reference-side
failure.

The following outcome is forbidden:

```text
router_construction = blocked-indefinitely-until-host-or-reference-changes
```

That recreates the environment-validation loop Plans 99-117 were intended to
escape.

---

# 5. Phase C — separate external acceptance debt from the product roadmap

Create or update one compact plans-directory authority for deferred external
evidence. Prefer a section in the Milestone 6 construction roadmap unless a
separate file is materially clearer.

The debt ledger must contain, at minimum:

```text
Q1 authenticated NTCP2 delivery
Q2 real external ShortBuild reply -> Established
live exploratory tunnel pair with independent router
live RouterInfo publication / lookup
live LeaseSet2 publication / lookup (future)
live destination interoperability (future)
```

Each debt item must carry:

```text
status
last valid evidence
blocker classification
what would make it executable
whether it blocks current product construction
```

Current Q1/Q2 items must be explicitly marked:

```text
blocker = host-lane-unavailable
blocks_M6_product_construction = false
```

No new test harness is created by this phase.

---

# 6. Phase D — synchronize roadmap authority

Update the current roadmap surfaces so they all say the same thing.

Required planning changes:

### `plans/000-mvp-roadmap.md`

Retain the original functional milestone definitions, but add an execution-state
note explaining that formal mixed-router transport exit criteria remain external
acceptance debt while transport-neutral implementation continues. Milestone 6
becomes the current product-construction milestone after Plan 118.

Do not rewrite history or mark Milestones 3-5 fully interoperable.

### `plans/115-117-external-delivery-to-live-netdb-roadmap.md`

Convert this from an active execution roadmap to a completed/terminal campaign
record. It should point to Plan 118 for final disposition and to the new
Milestone 6 construction roadmap for subsequent work.

### `plans/117-status.md`

Record the terminal disposition from Phase B. Preserve the exact prior native
failure evidence and the fact that publication / lookup / inbound return were
not reached on the defective pinned reference.

### `plans/117-handoff.md`

Replace the active N0-N10 execution posture with a short terminal handoff to
Plan 118 / Plan 119. Keep the old details as historical evidence only if useful,
but do not leave them looking like the next authorized work.

### new Milestone 6 roadmap

Add:

```text
plans/118-123-milestone6-router-construction-roadmap.md
```

It becomes the active transport-neutral router-construction sequence.

### non-plan authority

Synchronize `README.md`, `AGENTS.md`, architecture documentation, protocol
support, and support-matrix status only after the plans-directory authority is
internally consistent.

---

# 7. Phase E — planning hygiene rules for subsequent work

The following rules become normative for Plans 119+.

1. One numbered plan should represent one coherent product slice.
2. A failed implementation plan may receive one corrective addendum when a real
   code/protocol defect is found; avoid chains of status/handoff/corrective files
   for purely administrative label changes.
3. Status files should record evidence, not duplicate the execution plan.
4. Environment blockers belong in the external acceptance debt ledger unless
   the planned product code intrinsically requires that environment.
5. Reference implementations are validation tools, not architectural
   dependencies.
6. Parser-only evidence, native in-process evidence, authenticated-link
   evidence, and public/mixed-network evidence must retain distinct labels.
7. Python orchestration is not introduced when a narrow Rust integration test or
   deterministic in-process trajectory can establish the product invariant.
8. New product milestones should prefer implementation LOC and deterministic
   Rust tests over growing validation infrastructure.
9. The project must never obtain a green interoperability label by relaxing a
   stricter normative parser to accept demonstrably malformed reference output.
10. `normal-daemon NTCP2` remains disabled/non-advertised until its own external
    acceptance debt is actually closed.

---

# 8. Required validation

Plan 118 is primarily documentation work. It must nevertheless verify that the
retained product floor remains green.

Run at minimum:

```bash
cargo fmt --all --check
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-daemon --all-targets
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

If the repository's pinned toolchain requires `cargo +<version>`, use the
existing documented command form rather than changing toolchain policy.

Do not run or rebuild the old Python NTCP2 harness merely for Plan 118 closure.

If Phase B selects a corrected Emissary revision, run the focused native
reference test in addition to the local checks and record its exact revision and
highest stage.

---

# 9. Explicit acceptance criteria

Plan 118 is complete only when all of the following are true:

- [ ] Plans 115 and 116 remain closed and are not reopened.
- [ ] Plan 117 local production composition remains recorded as passed.
- [ ] The pinned Emissary reply-layout failure remains recorded accurately.
- [ ] Exactly one bounded upstream-correction decision was performed; no new
      interoperability framework was created.
- [ ] Plan 117 has a terminal progression classification: either native-pass or
      closed-for-progression-with-reference-evidence-gap.
- [ ] No document claims native mixed-router NetDB success unless that path was
      actually demonstrated.
- [ ] Q1/Q2 authenticated external transport remain separate deferred evidence
      items.
- [ ] `router_construction = may-continue` after the terminal disposition.
- [ ] `plans/000-mvp-roadmap.md`, the 115-117 roadmap, Plan 117 status/handoff,
      and the new Milestone 6 roadmap agree on the next implementation sequence.
- [ ] Historical plan files remain available for audit but no obsolete plan is
      presented as the next active execution item.
- [ ] The active next product plan is Plan 119.
- [ ] Local tunnel, NetDB, and daemon regression suites remain green.
- [ ] No normal-daemon NTCP2 activation, SSU2 work, Java/i2pd matrix, VM,
      namespace, Docker, or public-network testing is introduced.

---

# 10. Handoff on completion

The terminal handoff should be short:

```text
Plan 118: closed
Plan 117 local product floor: retained
Plan 117 native evidence: passed OR blocked-reference-defect
Authenticated external evidence: deferred and tracked separately
Current product milestone: Milestone 6
Next plan: 119-m6-leaseset2-protocol-foundation.md
```

The purpose of this pass is to stop spending implementation cycles proving that
planning labels are green and resume construction of the router while retaining
honest evidence boundaries.