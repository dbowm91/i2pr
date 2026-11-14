# Plan 495 — Peer-eligibility preflight and the first-failure-owner gate

**Status:** closed as `passed-peer-eligibility-gate-and-first-failure-owner`;
closure record
`plans/closure/core-router-recovery/495-status.md`
**Owner:** no production crate — the deliverable is the gate's enforcement
(`scripts/plan495-peer-eligibility.py`) and its canonical model
(`specs/QUALIFICATION-GATES.md`)
**Baseline:** `work/plans-477-480-router-recovery@dbc80596`
**Authoritative predecessor:** the blocked closures of Plans 449, 476, 493
and 434, and the Plan 230 mixed-router findings
(`plans/closure/mixed-router-interop/230-status.md`)

The plan body below is the plan-of-record as written before implementation.
Its status line above is authoritative; the body is not rewritten.

Status: registered / **ready**; no external environment required for the
offline scope. This is a corrective successor to the blocked closures
Plan 449 (`passed-portable-preflight-infrastructure-only`), Plan 476
(`blocked-local-process-executor-and-authorized-host-pair-unavailable`), Plan
493 (`blocked` on the authorized two-host topology) and Plan 434
(`blocked-current-pinned-i2pd-reverse-initiator-runner-unavailable`), and to
the Plan 230 mixed-router findings
(`plans/closure/mixed-router-interop/230-status.md`). Date: 2026-10-24.
Core-router recovery owner; the gate it owns is cross-family and binds the
core-router-recovery, network-participation, ssu2, ntcp2-transport and
mixed-router lanes.

**Objective.** Make the qualification gate itself state, before any protocol
attempt, whether the *stock peer* could admit and select `i2pr` — and make the
lane's outcome say which side owns a failure. Today the external lanes assume
the peer will choose us and read a refusal as an `i2pr` defect. That
assumption is wrong exactly where it is most dangerous: in an isolated
topology the peer has no alternatives, so one bad interaction is the whole
topology, and the peer's own state (profile store, selection filters, ban,
cooldown) can dominate the outcome. This plan closes that gap in the
**offline, deterministic** layer, where it can be closed without a host pair.

**Owner boundary.** The gate's *model* is canonical and lives in
[`docs/operations/ssu2-private-lan-qualification.md`](../../../docs/operations/ssu2-private-lan-qualification.md)
and `specs/CONFORMANCE.md`. This plan owns the **enforcement**: the offline
precondition checker, the tier ladder, the outcome classification, the
peer-state hygiene protocol, and the deliberate-withholding control. It owns
no production behaviour, changes no wire format, and claims no capability.

## Why ready

- **Hard dependencies: none.** Every input is already in the tree: the pinned
  reference sources and binaries, the working controlled SSU2 lane
  (`tests/integration/ssu2/run-independent.sh` and
  `crates/i2pr-runtime/tests/ssu2_independent.rs`), Plan 453's guarded
  private-LAN owner, and the Plan 449 offline preflight.
- **Interface dependencies are stable written contracts.** The gate model is
  already canonical in `plans/README.md`'s bounded-execution policy (rules 1,
  2, 3 and 6) and in `plans/implementation/README.md`'s handoff rules:
  *"identify a legitimate reference-owned trigger, source-derived selection
  prerequisites, deterministic offline fixture checks, the first failure-owner
  boundary"* and *"a randomized peer selector cannot guarantee one named
  candidate unless its actual admission and selection are verified."*
- **No external environment is needed for this scope.** Every work package
  below is offline, deterministic and runnable on one host.

## Current implementation evidence

**What already works, and is the model this plan generalizes.** The controlled
SSU2 lane already demonstrates the peer-selection design against stock i2pd
2.61.0 at `netid = 2`, on loopback:

- the peer is configured with **no reseed source at all**
  (`tests/integration/ssu2/run-independent.sh:101-102`: `urls =` empty,
  `threshold = 0`), so it starts with no seeds and no peers;
- `i2pr` initiates, and the peer **learns our RouterInfo in-band** during
  that session (`crates/i2pr-runtime/tests/ssu2_independent.rs:847`);
- `i2pr` advertises the **floodfill** capability (`caps = "fO"`), documented
  at `crates/i2pr-runtime/tests/ssu2_independent.rs:41` as the
  *"direction-B trigger"*, and the peer then initiates a **fresh session back
  to us** (`crates/i2pr-runtime/tests/ssu2_independent.rs:846`);
- the peer's ingest gates are recorded in the driver's own comment
  (`crates/i2pr-runtime/tests/ssu2_independent.rs:227`): `router.version` must
  clear the peer's minimum-allowed floor, and a matching `netId` must be
  present — **a missing `netId` marks the RouterInfo unreachable without
  logging**, and the peer's NetDB silently refuses to persist it.

**The Java side is stricter and has already produced the false failure this
plan exists to prevent.** `ProfileManagerImpl.heardAbout()` — the only route
to a profile — is gated on the peer RouterInfo carrying `R` plus floodfill,
bandwidth and congestion conditions
(`plans/closure/mixed-router-interop/201-status.md:661`); selection then
additionally requires `selectAllPeers`/`getProfileNonblocking` membership,
`not_failing_count > 0`, and a selectable, non-banned, non-floodfill peer
(`plans/closure/mixed-router-interop/230-status.md:149`). Plan 230's attempt
3b recorded `P230-D-ELIGIBLE-BUT-NO-PROFILE` — *"eligible 3/3 on this SHA;
30 s D poll exhausted with count=1, C absent"*
(`plans/closure/mixed-router-interop/230-status.md:168`): eligible by the
capability predicate, never profiled, never selected, poll exhausted. Read
naively that says "the peer does not implement this". It says "the peer's
selection state never admitted us".

**The ban mechanism is real and documented.** A static-key mismatch on a
published address makes the pinned peer ban the whole address for **~30–39
minutes**, breaking every later run until restart
(`plans/closure/ssu2/161-status.md:247`). Stale `netDb/r*/*.dat` files carry
stale peer profiles, and the peer redials previously learned RouterInfos,
which can swallow our inbound handshake by endpoint routing
(`plans/closure/ssu2/161-status.md:242`).

**What is missing.** The gate's *model* is now written down
(`docs/operations/ssu2-private-lan-qualification.md`), but nothing enforces
it: no offline precondition checker exists, no lane records the tier ladder,
no outcome classification distinguishes a peer-side refusal from an `i2pr`
defect, no lane enforces peer-state hygiene, and no lane has a
deliberate-withholding control. Plan 449's operator procedure previously
instructed *"Stop on the first failed stage; do not retry an unchanged
fixture"* with no peer-eligibility preflight, which destroys lower-tier
evidence and misreads a peer refusal as an implementation defect.

## Invariants that must not regress

1. **The failure owner is always named.** No external lane may report a bare
   "no session" or "no tunnel". Every outcome is one of the taxonomy's
   classes, and the class is evidence.
2. **A peer-side outcome is never an `i2pr` verdict.** `peer-admission-refused`,
   `peer-selection-refused`, `peer-state-poisoned`, `environment-unavailable`
   and `timeout` say nothing about `i2pr` in either direction.
3. **Lower tiers survive higher-tier failure.** A tier pass is never
   retracted by a later tier's failure.
4. **No capability claim exceeds the tier reached.** A transport pass is not
   a product pass; a one-family pass is not a two-family claim; a
   private-LAN pass is not public reachability.
5. **No forged reachability, bandwidth or capability.** `R` is set only where
   it is a true statement about the lab, and only under the lab profile.
6. **The peer is never patched, vendored, or tuned to pass.** A peer
   configuration that is required to make a lane pass is itself a finding
   about the lane's design and is recorded as such.
7. **Fail closed, never silently skip.** A missing environment, a missing
   pinned checkout, or a missing precondition fails the lane with a named
   reason.
8. **Raw reference logs are never evidence.** Sanitized counts, tier
   outcomes and digests only.

## Scope

**In:**

- the **offline precondition checker**: for each pinned family, derive the
  peer's admission and selection predicates from its own source, and evaluate
  a candidate lab RouterInfo against them — deterministic, no socket, no
  process;
- the **lab RouterInfo profile** as a checked artifact: the field set, the
  reason for each field, and the per-family differences;
- the **tier ladder** and the **outcome classification** as a shared,
  reusable vocabulary with a machine-checkable shape;
- the **peer-state hygiene protocol** as an executable precondition;
- the **deliberate-withholding control** as the gate's own self-test;
- the amendments that bind Plan 449's live column, Plan 476's executor, and
  Plan 493's matrix to the gate.

**Explicitly out:**

- **any live or two-host execution.** This plan needs no host pair and
  authorizes none.
- **any production behaviour change.** The daemon's `private_lan_qualification`
  profile, its signed-RouterInfo path and Plan 453's owner are unchanged.
- **any change to the reference pins** (i2pd `2.61.0`, Java I2P `2.13.0`) or
  to the reference routers.
- **any weakening of a boundary script** or of the fail-closed lane
  semantics.
- **the two-family claim itself** — Plan 493 owns that, and it remains
  blocked on the authorized topology and Plan 435.
- **public reachability, `R`, `f`, introducer, or support-inventory
  promotion.**

## Required production changes

None. This plan adds test and lane tooling plus planning and documentation
artifacts. It may add:

- `scripts/plan495-peer-eligibility.py` — the offline precondition checker
  (pure, no socket, no process, no network);
- `tests/integration/ssu2/plan495-eligibility-fixtures/` — deterministic
  positive and negative lab-RouterInfo fixtures;
- `specs/QUALIFICATION-GATES.md` — the canonical, citable statement of the
  gate model, referenced by `specs/CONFORMANCE.md`.

## Ordered work packages

1. **WP1 — the canonical gate model.** Publish the gate as a citable
   canonical document: the peer-eligibility preflight, the lab RouterInfo
   profile, the tier ladder, the outcome taxonomy, the peer-state hygiene
   protocol, the withholding control, and the isolated-topology asymmetry
   statement. Bind it from `specs/CONFORMANCE.md` so it is discoverable, and
   from the operator procedure so it is usable. *Partially landed in this
   commit: `docs/operations/ssu2-private-lan-qualification.md` and
   `specs/CONFORMANCE.md`.*
2. **WP2 — the offline precondition checker.** For each pinned family,
   derive the peer's admission gates and selection predicates from its own
   source and evaluate a candidate lab RouterInfo against them. The checker
   reports, per predicate, `satisfied` / `deliberately-unsatisfied` /
   `unknown`, and refuses to report a lane-ready verdict when a predicate is
   `unknown`. It must never open a socket.
3. **WP3 — the lab profile as a checked artifact.** Encode the profile table
   (field, value, reason, family) so a drift between the documented profile
   and the checked profile fails a static guard.
4. **WP4 — the tier ladder and outcome classification.** A shared vocabulary
   plus a validator for the evidence shape: every tier carries
   `pass`/`fail`/`not-attempted` with a reason; every run carries exactly one
   outcome class; a run that reaches tier *n* must have recorded tiers
   `0..n`.
5. **WP5 — the peer-state hygiene protocol.** An executable precondition:
   fresh peer data directory and fresh peer process per attempt, no stale
   `netDb/r*/*.dat`, no mismatched static key on the live port, advertised
   endpoint equal to the bound socket. A `peer-state-poisoned` outcome must
   not consume the attempt budget and must name the cooldown.
6. **WP6 — the deliberate-withholding control.** With the selection predicate
   deliberately withheld, the lane must classify the result as
   `peer-selection-refused` with the peer's own reason, and must not
   classify it as `i2pr-defect` and must not report a pass. This is the
   inverted probe for the whole gate.
7. **WP7 — bind the lanes.** Amend Plan 449's live column, Plan 476's
   executor, and Plan 493's matrix to the gate, and amend the affected
   subsystem roadmaps. Plan 493's "an unchanged-topology retry is not a new
   attempt" is reconciled with the canonical rule that a *peer-state* change
   — a cooldown expiring, a profile store reset — **is** a material change.

## Failure, cancellation, restart and contention semantics

- **The checker fails closed.** An unknown predicate, a missing pinned
  checkout, an unresolvable family, or a profile drift is a hard failure with
  a named reason, never a skip and never a pass.
- **A lane run produces exactly one outcome class**, and the class is
  recorded with the reason that selected it.
- **A `peer-state-poisoned` outcome does not consume the attempt budget**; a
  cooldown expiring or a profile store reset is a material change and
  licenses a controlled rerun inside the same plan.
- **No raw reference log, RouterInfo, address, key or identity reaches an
  evidence file.** The checker's output is counts, predicate states and
  digests.
- **The checker is pure and offline.** It must not import a socket, spawn a
  process, or touch the network; a static guard asserts this, because a
  checker that could reach the network would make the gate unfalsifiable.

## Compatibility and migration

- **No wire change**, no `router.version` change, no migration, no support
  inventory change, no advertisement change.
- The gate model is additive to the planning and evidence surface. Existing
  closure records are **not** rewritten: the Plan 230 and Plan 161 findings
  they carry are the evidence this plan builds on, and their tokens remain
  authoritative.

## Required tests

- the checker's `--self-test`, with at least one negative case per predicate
  (an RI missing `netId`; a version below the peer's floor; a mismatched
  static key; a non-RFC1918 address; a profile field the peer does not
  accept);
- a fixture that proves a `peer-selection-refused` classification is
  reachable and is distinct from `i2pr-defect`;
- a fixture that proves a `peer-state-poisoned` outcome does not consume the
  attempt budget;
- a static guard proving the checker contains no socket, process-spawn or
  network code, with an inverted probe;
- the ordinary offline suite must remain green.

## Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
python3 scripts/plan495-peer-eligibility.py --self-test
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
python3 scripts/check-tooling-inventory.py
```

## Documentation updates

- `docs/operations/ssu2-private-lan-qualification.md` — the operator-facing
  gate, rewritten to state the peer-eligibility preflight, the lab
  RouterInfo profile, the tier ladder, the outcome taxonomy, the peer-state
  hygiene protocol and the withholding control;
- `specs/CONFORMANCE.md` — a binding section that makes the gate discoverable
  as canonical evidence policy;
- `plans/implementation/core-router-recovery/449-*.md`,
  `plans/implementation/network-participation/493-*.md`,
  `plans/implementation/core-router-recovery/476-*.md` — the binding
  amendments;
- `plans/subsystems/core-router-recovery-roadmap.md` and
  `plans/subsystems/network-participation-roadmap.md` — the milestone rows;
- `plans/registry.md` — the plan's row and the affected blocked rows.

## Acceptance criteria

1. The gate model is published, canonical, citable and bound from
   `specs/CONFORMANCE.md` and from the operator procedure.
2. An offline checker evaluates a candidate lab RouterInfo against each
   pinned family's admission and selection predicates and reports each
   predicate's state, refusing a lane-ready verdict on an unknown predicate.
3. The checker opens no socket, starts no process and touches no network, and
   a static guard with an inverted probe proves it.
4. The tier ladder and outcome taxonomy are machine-checkable, and the
   validator rejects evidence that omits a tier, carries two outcome classes,
   or reports a pass without the tier that earned it.
5. The deliberate-withholding control classifies as
   `peer-selection-refused` with the peer's own reason, and never as
   `i2pr-defect` and never as a pass.
6. A `peer-state-poisoned` outcome does not consume the attempt budget.
7. Plans 449, 476 and 493 bind to the gate, and Plan 493's attempt-budget
   rule is reconciled with the canonical causal-corrective policy.
8. No support inventory, conformance, advertisement or capability claim
   changes, and the routine floor is green on the closing tree.

## Stop conditions

Stop and record a `stopped-*` closure when:

- the pinned peer's selection predicates **cannot be derived offline** from
  its own source, and no deterministic offline fixture can stand in for
  them — in that case the gate is not machine-checkable and a human-read
  source citation is the honest fallback, recorded as such;
- the checker would need to reach the network to be meaningful — that would
  make the gate unfalsifiable and is refused;
- satisfying a predicate would require forging reachability, bandwidth or
  capability, or would require patching, vendoring or re-tuning a reference
  router;
- any step would require weakening a boundary script, relaxing the
  fail-closed semantics, or changing a reference pin to go green;
- the attempt budget is spent without a material change.

## Closure evidence required

- `plans/closure/core-router-recovery/495-status.md` with the full closure
  content the repository requires, including the requirement-to-evidence
  matrix, the command outcomes, the invariant/failure/migration/security
  reviews, the findings by severity, the roadmap disposition and the unblock
  audit;
- the checker's self-test evidence;
- the registry and roadmap rows advanced;
- the unblock audit recorded, with any newly ready plan moved to `ready` in
  the same commit.

## Handoff notes

- **This plan is a gate repair, not a capability plan.** It produces no
  interoperability evidence and authorizes no external run. Its deliverable
  is that a *later* lane cannot misreport a peer-side condition as an `i2pr`
  defect.
- **Do not weaken the classification to make a lane green.** A lane that
  cannot say which side owns a failure must record `timeout` or
  `environment-unavailable`, not `pass` and not `i2pr-defect`.
- **The canonical policy already requires this.** `plans/README.md`'s
  bounded-execution rules 1, 2, 3 and 6, and `plans/implementation/README.md`'s
  external-router handoff rules, already mandate source-derived selection
  prerequisites, a first-failure-owner boundary, and a randomized peer
  selector whose admission and selection are actually verified. This plan
  implements them; it does not relax them.
- **The isolated-topology asymmetry is the reason this plan exists.** On the
  live network a peer has alternatives and one bad interaction is noise; in
  the lab the peer has none, and its own state dominates. That asymmetry is
  why a lab failure is not a live-network verdict — in either direction.
