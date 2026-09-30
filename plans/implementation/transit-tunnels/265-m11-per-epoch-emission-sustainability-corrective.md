# Plan 265 — M11 per-epoch emission sustainability corrective

Status at registration:
**registered-m11-per-epoch-emission-sustainability-corrective-ready**

Scope refinement baseline:
`6ab9dc2dd80526ce38e62014635ec3e3ad5521f5`

Planning reconciliation through the Plan 264 closure commit is
planning/specification-only after the implementation SHA above;
it does not change the production source baseline inherited from
Plan 262 (Plans 263/264 are zero-production-diff). The
implementation agent MUST start from the latest `main`, record
the exact pre-implementation HEAD in the handoff, and bind every
counted external attempt to the eventual implementation SHA. The
historical registration SHA is planning provenance, not a
qualification SHA.

Original registration baseline and Plan 264 closure authority
remain recorded in
`plans/closure/transit-tunnels/264-status.md`.

Corrects:

- `plans/implementation/transit-tunnels/264-m11-single-mesh-sustainability-scoping.md`
  (§10 criterion 5: 5/13 epochs close 2/2 per-epoch, but the
  4 emission-dependent epochs stop on window signatures —
  `ibgw-data` 1/2 single-cell-only, `receipt` 0/2 starvation,
  `participant-data` 1/2 no-forward, `replay` 0/2
  forward-setup stops — and 4 lifecycle epochs are
  structurally blocked behind the forward cell);
- `plans/closure/transit-tunnels/264-status.md` (per-epoch
  emission-window boundary + replay singleton LOW finding).

Retains:

- Plan 262 WP A/B/C/D/E production corrective in full
  (dedicated `TransitGatewayData`, exact receive-id ownership,
  source-neutral `route_ibgw_gateway` seam, OBEP TUNNEL-to-self
  local branch with `LocalIbgwDelivered`/`LocalIbgwDropped`, no
  synthetic peer/index mutation; Participant/OBEP peer locks
  unchanged);
- Plan 263 WP A harness in full (mesh-liveness + relay-NetDB
  + B-floodfill prerequisites, canonical
  `SAM_READ_TIMEOUT_MSG` tail, 4 `plan263_*` rows);
- Plan 264 WP A lane in full (per-epoch selector for all 13
  epochs, setup prerequisites, lifecycle chain, manifest
  v4 with `epoch_qualification`, composition script + 5
  `plan264_*` rows, workflow epoch/pass inputs, checker
  rules 29/30/37 + evidence-gate Plan 264 section);
- The 5 closed per-epoch epochs as the determinism baseline
  (`obep`, `ibgw`, `participant`, `reject`, `obep-data`
  2/2 on `6ab9dc2d`); do not re-prove ownership, do not
  touch production routing code;
- Exact-pinned i2pd 2.61.0 @
  `635b013a612ff47278ef02acf8580a28e10e26c5`, unmodified,
  loopback-only, fresh datadirs per run, no public fallback.

Reference authority remains exact-pinned, unmodified i2pd 2.61.0.
Normative protocol authority is unchanged from Plan 262
(IBGW receive-id routing without creator affinity; tunnel ids as
per-hop routing identifiers).

## 1. Objective

Close the 4 blocked emission epochs (`ibgw-data`,
`receipt`, `participant-data`, `replay`) plus the 4
forward-blocked lifecycle epochs (`expiry`, `cancel`,
`session-close`, `restart`) to two same-SHA fresh-mesh
passes each, WITHOUT touching production routing code and
WITHOUT tuning until green.

The Plan 264 evidence shows emission windows (B-side
fragmentation, socket delivery, forward routing) are
per-window on the unmanaged reference (~50/50 per fresh
mesh), not per-mesh. The corrective must therefore change
the qualification SHAPE for emission rows (not the
production code, not the lane constants): e.g. qualify
emission-dependent rows over a bounded fresh-mesh budget
with first-two-counted retention, and/or re-scope which
reference-emission observable closes each row, and/or
harden the replay predicate with outcome-kind evidence so
local containment counts distinctly from wire delivery.
Whatever shape is chosen, it must be stated upfront in
this plan's WP A, locked by checker invariants, and
executed without retry-until-green (every counted run is
one fresh execution; its result is retained; the budget
and the counting rule are fixed before execution).

This is a **narrow emission-qualification corrective plus
final pass**. It does not change wire format, add a task/
channel/queue, raise quotas, inflate timeouts, alter
RouterInfo/router.version, enable public transit, or revise
the Plan 262 ownership semantics. Production `crates/*/src`
diff must stay empty (test/lane/checker/workflow +
planning/spec only), like Plans 261/263/264
zero-production-diff precedent.

## 2. Why this scope is required

Plan 264 proved the per-epoch gate restores determinism
for builds and locally-driven data (5 epochs 2/2) but the
unmanaged reference emits in windows:

- `ibgw-data` fragments max-2 on some meshes, single-cell-
  only on others (15/max2 vs 4/max1 on one SHA);
- `receipt` starves (0/0/0) or partially delivers (8
  ingress, 0 socket) with B3 holding;
- `participant-data` forwards + far-sides on some meshes,
  observes no genuine forward on others;
- `replay` (and structurally `expiry`/`cancel`/
  `session-close`/`restart`) cannot reach its experiment
  without a forward cell.

No further pre-send proof can make the unmanaged reference
emit on demand, and longer runs only burn wall time into
the same windows. The remaining work is purely the
counting shape for emission rows plus the replay
predicate hardening. Do not pre-register Plan 266; the M11
dependency graph remains linear at Plan 265.

If execution exposes a defect outside this emission
boundary (e.g., a new crypto/fragment/pool routing defect
with healthy-mesh stop provenance, or a second wire
duplicate with B-side receipt), stop and register the
narrow successor.

## 3. Frozen invariants

### 3.1 Reference/network

- i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Reference source clean/unmodified.
- Loopback-only qualification meshes.
- Public reseed/network disabled.
- Fresh datadirs/ports/evidence root for every counted run.
- No reference patching, LD_PRELOAD, fake peer state, or public
  fallback.

### 3.2 Product authority

- Ordinary i2pr transit remains disabled.
- No RouterInfo transit capability.
- No router.version change.
- No public transit config option.
- No M12 implementation.
- No production `crates/*/src` diff (corrective-only).

### 3.3 Harness ownership

- Existing finite timeouts remain (no inflation to fit).
- No quota enlargement (no new tasks/channels/queues, no raised
  ceilings).
- The counting rule and budget are fixed BEFORE execution;
  no "retry until two passes". Each counted run is one fresh
  execution and its result is retained.
- Receipt-first ordering is required per epoch group.
- Diagnostic runs never satisfy final rows.

### 3.4 Evidence

- Raw reference logs remain diagnostic input only.
- Counted evidence contains public hashes/ids/counts/status only.
- Per-epoch manifests carry `plan: 265` + epoch + pass ids;
  the composition gate from Plan 264 stays (extended only
  as WP A states).
- B-side endpoint receipts remain the only proof that a
  duplicate reached the wire.

## 4. Work package A — emission counting shape + replay hardening

Bounded and fail-closed; state the shape BEFORE executing:

1. Counting shape: define exactly how emission-dependent
   rows close over fresh meshes (budget, retention rule,
   which observable binds each row). Lock it in the
   composition checker. The rule must not be re-tunable
   mid-execution.
2. Replay hardening: record the replay outcome kind
   (forward vs drop vs unrecorded containment) as
   diagnostic-only evidence (Plan 258 pattern: emitted,
   never read by the pass predicate except through the
   stated suppression predicate), so local containment
   without a row counts distinctly from a wire duplicate
   (which additionally requires the B-side endpoint
   receipt). Resolve the Plan 264 singleton class with
   stop provenance on the new SHA.
3. Retained proofs: mesh-liveness + relay-NetDB +
   B-floodfill prerequisites stay mandatory inside every
   per-epoch run; SAM discipline unchanged.
4. No tuning: timeouts, quotas, ceilings, retry budgets,
   and message sizes stay exactly as in Plan 264. Any
   change that would make an epoch pass by enlarging
   resources is forbidden; stop instead.

## 5. Work package B — per-epoch emission qualification

Qualify the 8 open epochs per the WP A shape on one
implementation SHA (manifest `plan: 265`), each pass on a
fresh mesh:

- IBGW multicell bounded emission twice;
- receipt with tuple-bound socket delivery twice
  (`terminal-garlic-self:0/ingress:≥1/socket:1`);
- Participant far-side forwarding proof twice;
- replay suppression twice (with outcome-kind evidence);
- code-30, replay-expiry-cancel-session-close-restart
  lifecycle rows twice (riding the forward cell);
- fragment-id hardening; source locks; B-sender rows.

No cross-epoch or cross-pass evidence merge beyond the WP A
counting rule.

## 6. Compatibility / migration / security

No user migration. No change to config schema, CLI, SAM/I2CP
public API, RouterInfo capabilities, router.version,
public-network behavior, or default transit-disabled
construction. Security interpretation unchanged from Plan 264
(transport authentication at the owner; IBGW receive-id
authorization; Participant/OBEP locks; self-loop only after
decoded target == own hash; no synthetic peer; duplicates
contained locally with B-side proof required for any wire-
duplicate claim). No new dependency.

## 7. Stop conditions

Stop and register a new narrow corrective if:

- exact-pinned source contradicts any retained source-lock
  premise;
- a healthy-mesh run exposes a new routing defect (crypto,
  fragment, pool, forward, dispatch) outside the harness seam
  with stop provenance (including a second wire duplicate
  WITH a third B-side endpoint receipt);
- a single epoch cannot close under the WP A counting
  shape within its fixed budget (that epoch owns a narrower
  successor; do not widen this plan);
- public network, false capability advertisement, reference
  patching, quota/timeout enlargement, or a new dependency would
  be needed.

Do not weaken or retire any semantic row. Do not touch
production routing code to go green.

## 8. Required focused tests

At minimum (all retained green plus):

1. three exact-pinned source locks;
2. five runtime-neutral IBGW regressions;
3. four service regressions;
4. seven live-owner self-loop/negative rows;
5. four Plan 263 harness rows;
6. five Plan 264 composition rows;
7. B SAM gate + B-sender rows;
8. receipt flip + IBGW multicell + far-side re-proof on the
   new SHA;
9. replay outcome-kind rows (forward/drop/unrecorded
   distinguished; wire duplicate requires B-side receipt);
10. WP A counting-shape gate tests (budget fixed before
    execution; over-budget runs rejected; cross-epoch merge
    rejected outside the stated rule);
11. missing env fails before network startup;
12. ordinary product transit remains disabled.

## 9. Exact verification floor

Run:

    cargo fmt --all --check
    cargo check --locked --workspace --all-targets
    cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
    cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
    cargo test --locked --workspace --all-targets -- --test-threads=1
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
    cargo test --locked --workspace --doc
    cargo deny check advisories bans sources
    bash scripts/check-dependency-direction.sh
    bash scripts/check-runtime-boundaries.sh
    bash scripts/check-service-tunnel-boundaries.sh
    bash scripts/check-m11-transit-boundaries.sh
    bash scripts/check-m11-transit-qualification-evidence.sh
    bash scripts/check-m11-per-epoch-composition.sh
    bash scripts/check-m6-mixed-router-acceptance-evidence.sh
    bash scripts/check-exploratory-tunnel-evidence.sh
    git diff --check

Run every new Plan 265 focused test exactly by name
(retained Plan 263/264 rows plus counting-shape rows).

After the WP A shape lands, qualify every open epoch per
that shape on the same implementation SHA. Closure also
requires exact-head ordinary CI green on Quality
(ubuntu/macos), MSRV, Dependency policy.

## 10. Acceptance criteria

Plan 265 closes only when all are directly evidenced (plus all
retained Plan 264 criteria still green):

1. Plan 264 retained evidence remains traceable.
2. Emission rows re-proven on the implementation SHA per
   the WP A shape (multicell + receipt + far-side).
3. Three source locks hold.
4. No production `crates/*/src` diff (corrective-only).
5. Every mandatory epoch passes per the WP A counting
   shape on the same SHA, each pass on a fresh mesh,
   unmerged beyond the stated rule (per-epoch manifests
   `plan: 265`).
6. Replay singleton class resolved with outcome-kind
   evidence (no open wire-duplicate question).
7. Full workspace verification passes.
8. Exact-head ordinary CI passes all four jobs.
9. Registry/roadmap/support/conformance/dossier agree on Plan
   265 as the sole dependency-ready M11 closure authority (or
   mark the ADR 0026 one-family M11 experimental qualification
   passed, if the matrix closes).
10. No product default/capability/version/public-network change.
11. No critical/high finding remains open.

Only then may the unblock audit mark the ADR 0026 one-family M11
experimental qualification passed and consider M12 planning
ready.

## 11. Closure evidence required

Write `plans/closure/transit-tunnels/265-status.md` with
implementation/closure SHAs, reference pin/version, retained
source-lock proofs, WP A shape with no-production-diff
proof, every per-epoch same-SHA manifest per the counting
rule, replay resolution, full verification, exact-head CI,
security/resource/concurrency/migration reviews, findings by
severity, unblock audit. If a new boundary appears, close
retained/blocked and register only the narrow successor exposed
by the evidence.

## 12. Handoff order

1. state the WP A counting shape + replay hardening (no
   tuning, budget fixed upfront);
2. re-prove multicell + receipt + far-side once
   (baselines still hold on the new SHA);
3. qualify every open epoch per the shape (fresh mesh per
   pass);
4. resolve the replay singleton class with outcome-kind
   evidence;
5. run the complete local verification floor;
6. obtain exact-head ordinary CI green;
7. close only if all §10 criteria are directly evidenced.

Do not create Plan 266 pre-emptively. The next plan is
registered only if Plan 265 execution exposes a new bounded
defect or qualification boundary.
