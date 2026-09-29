# Plan 264 — M11 single-mesh sustainability scoping corrective

Status at registration:
**registered-m11-single-mesh-sustainability-scoping-ready**

Scope refinement baseline:
`9bd2f39aa2c8509273177eb509157d60a09cafb2`

Planning reconciliation through the Plan 263 closure commit is
planning/specification-only after the implementation SHA above;
it does not change the production source baseline inherited from
Plan 262 (Plan 263 itself is zero-production-diff). The
implementation agent MUST start from the latest `main`, record
the exact pre-implementation HEAD in the handoff, and bind every
counted external attempt to the eventual implementation SHA. The
historical registration SHA is planning provenance, not a
qualification SHA.

Original registration baseline and Plan 263 closure authority
remain recorded in
`plans/closure/transit-tunnels/263-status.md`.

Corrects:

- `plans/implementation/transit-tunnels/263-m11-qualification-sustainability-corrective.md`
  (§10 criteria 6–7 / §15 single-mesh stops: receipt
  re-proven `0/18/1` tuple-bound and IBGW multicell re-proven
  max 2 on one SHA, but two same-SHA single-mesh complete
  attempts stop on environmental signatures);
- `plans/closure/transit-tunnels/263-status.md` (single-mesh
  boundary: attempt1 IBGW-data single-cell-only 14-ingress
  window, 400.3 s; attempt2 receipt-epoch `0/0/0`
  starvation after IBGW multicell passed, 1047.4 s;
  diag-receipt1 `0/0/0` vs diag-receipt2 `0/18/1` on the
  same SHA).

Retains:

- Plan 262 WP A/B/C/D/E production corrective in full
  (dedicated `TransitGatewayData`, exact receive-id ownership,
  source-neutral `route_ibgw_gateway` seam, OBEP TUNNEL-to-self
  local branch with `LocalIbgwDelivered`/`LocalIbgwDropped`, no
  synthetic peer/index mutation; Participant/OBEP peer locks
  unchanged);
- Plan 263 WP A harness in full (mesh-liveness + relay-NetDB
  + B-floodfill prerequisites before counted sends, canonical
  `SAM_READ_TIMEOUT_MSG` tail, 4 `plan263_*` rows, manifest
  `plan: 263` shape — bump to `plan: 264` only in the
  qualified lane, with checker updates);
- Plan 263 re-proofs as the semantic baseline (receipt
  `0/18/1` tuple-bound + IBGW multicell max 2 on `9bd2f39a`);
  do not re-prove ownership, do not touch production routing
  code;
- Plan 260 WP A/B/F + Plan 261 WP A/B lane work + all source
  locks + 5 + 4 + 7 + 4 regressions + driver self-loop
  mapping;
- Exact-pinned i2pd 2.61.0 @
  `635b013a612ff47278ef02acf8580a28e10e26c5`, unmodified,
  loopback-only, fresh datadirs per run, no public fallback.

Reference authority remains exact-pinned, unmodified i2pd 2.61.0.
Normative protocol authority is unchanged from Plan 262
(IBGW receive-id routing without creator affinity; tunnel ids as
per-hop routing identifiers).

## 1. Objective

Re-scope the unclosable gate — "two complete passes on ONE
mesh" — into per-epoch fresh-mesh counted qualification that
the evidence shows CAN close, without touching production
routing code and without tuning until green.

Each counted epoch (OBEP/IBGW/Participant builds, OBEP-data,
IBGW-data with multicell, receipt with tuple-bound socket
delivery, Participant far-side, code-30, replay, expiry,
cancel, session-close, restart) is qualified on its OWN fresh
mesh; epochs never share a mesh, so single-mesh churn cannot
cascade across epochs. Two independent per-epoch passes on one
SHA (fresh mesh per epoch per pass, no cross-epoch or
cross-pass merge) close each row.

This is a **narrow qualification-scoping corrective plus final
pass**. It does not change wire format, add a task/channel/
queue, raise quotas, inflate timeouts, alter RouterInfo/
router.version, enable public transit, or revise the Plan 262
ownership semantics. Production `crates/*/src` diff must stay
empty (test/lane/checker/workflow + planning/spec only), like
Plans 261/263 zero-production-diff precedent.

## 2. Why this scope is required

Plan 263 proved every semantic row passes on a healthy window
but one mesh cannot deterministically survive ~15 consecutive
counted epochs (~400–1050 s):

- diag-receipt2 closes receipt `0/18/1` tuple-bound
  (257.6 s); attempt2 closes IBGW multicell max 2 — same
  SHA, different meshes.
- attempt1's mesh delivers 14 single-cell ingresses but never
  fragments in-window; attempt2's mesh fragments (max 2) but
  later starves the receipt leg `0/0/0`; diag-receipt1's mesh
  starves from the start. All with B3 holding and no
  production row failing.

The defect is the GATE (single-mesh endurance), not the
routing and not the harness: no further pre-send proof can
make an unmanaged reference mesh stop churning, and longer
runs only burn more wall time into the same churn. The
correct size is re-scoping qualification to match what the
controlled lane can deterministically own: one fresh mesh per
counted epoch. Do not pre-register Plan 265; the M11
dependency graph remains linear at Plan 264.

If execution exposes a defect outside this scoping boundary
(e.g., a new crypto/fragment/pool routing defect with healthy-
mesh stop provenance), stop and register the narrow successor.

## 3. Frozen invariants

### 3.1 Reference/network

- i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Reference source clean/unmodified.
- Loopback-only qualification meshes.
- Public reseed/network disabled.
- Fresh datadirs/ports/evidence root for every counted epoch
  run (finer-grained than before: per-epoch, not per-matrix).
- No reference patching, LD_PRELOAD, fake peer state, or public
  fallback.

### 3.2 Product authority

- Ordinary i2pr transit remains disabled.
- No RouterInfo transit capability.
- No router.version change.
- No public transit config option.
- No M12 implementation.
- No production `crates/*/src` diff (scoping-only corrective).

### 3.3 Harness ownership

- Existing finite timeouts remain (no inflation to fit).
- No quota enlargement (no new tasks/channels/queues, no raised
  ceilings).
- No repeated restart loop; no "retry until two passes". Each
  counted epoch run is one fresh execution and its result is
  retained.
- Receipt-first ordering is required per epoch group.
- Diagnostic runs never satisfy final rows; two final passes
  per epoch may not merge evidence across epochs or passes.

### 3.4 Evidence

- Raw reference logs remain diagnostic input only.
- Counted evidence contains public hashes/ids/counts/status only.
- Per-epoch manifests carry `plan: 264` + epoch + pass ids;
  the two-pass gate is evaluated PER EPOCK, never across
  epochs.
- The composition rule (which per-epoch passes compose the
  closure claim) is explicit in the checker: every mandatory
  epoch needs two same-SHA passes; no epoch is waived.

## 4. Work package A — per-epoch fresh-mesh lane

Re-scope only the lane driver + runner + checkers, bounded
and fail-closed:

1. Epoch isolation: each counted epoch runs on its own fresh
   mesh (fresh datadirs/ports/evidence per epoch run, same
   as today per matrix run). The existing
   `I2PR_M11_ONLY_EPOCH` selector becomes the counted
   mechanism (today it is diagnostic-only); extend it to
   every mandatory epoch with manifest `plan: 264` +
   epoch + pass ids.
2. Composition gate: the checker composes closure ONLY from
   per-epoch manifests (two same-SHA passes per epoch, all
   mandatory epochs present, no cross-epoch merge). A
   single-mesh full-matrix run is no longer the gate; if
   one is executed it is diagnostic-only.
3. Retained proofs: mesh-liveness + relay-NetDB + B-floodfill
   prerequisites (Plan 263) stay mandatory inside every
   per-epoch run; SAM discipline unchanged.
4. No tuning: timeouts, quotas, ceilings, retry budgets, and
   message sizes stay exactly as in Plan 263. Any change that
   would make an epoch pass by enlarging resources is
   forbidden; stop instead.

## 5. Work package B — per-epoch two-pass qualification

Run every mandatory epoch twice on one implementation SHA
(manifest `plan: 264`), each pass on a fresh mesh:

- OBEP/IBGW/Participant builds with exact cardinality +
  typed bandwidth;
- OBEP unfragmented + fragmented semantic delivery;
- IBGW live receipt and multicell bounded emission;
- receipt with tuple-bound socket delivery (`terminal-
  garlic-self:0/ingress:≥1/socket:1`);
- Participant far-side forwarding proof;
- code-30 rejection with zero state; replay suppression;
- logical >600-second expiry and post-expiry drop;
- cancellation all-dimensional drain;
- session-close removal with unrelated peer retained;
- real runtime restart, zero new state, fresh accepted build;
- fragment-id hardening; source locks; B-sender rows.

No cross-epoch or cross-pass evidence merge.

## 6. Compatibility / migration / security

No user migration. No change to config schema, CLI, SAM/I2CP
public API, RouterInfo capabilities, router.version,
public-network behavior, or default transit-disabled
construction. Security interpretation unchanged from Plan 263
(transport authentication at the owner; IBGW receive-id
authorization; Participant/OBEP locks; self-loop only after
decoded target == own hash; no synthetic peer). No new
dependency.

## 7. Stop conditions

Stop and register a new narrow corrective if:

- exact-pinned source contradicts any retained source-lock
  premise;
- a healthy-mesh run exposes a new routing defect (crypto,
  fragment, pool, forward, dispatch) outside the harness seam
  with stop provenance;
- a single epoch cannot close two same-SHA fresh-mesh passes
  (that epoch owns a narrower successor; do not widen this
  plan);
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
6. B SAM gate + B-sender rows;
7. receipt flip + IBGW multicell re-proof on the new SHA;
8. per-epoch composition gate tests (one pass cannot close;
   mixed SHAs rejected; cross-epoch merge rejected;
   missing epoch rejected);
9. diagnostic-only single-mesh run cannot satisfy counted
   closure;
10. missing env fails before network startup;
11. ordinary product transit remains disabled.

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
    bash scripts/check-m6-mixed-router-acceptance-evidence.sh
    bash scripts/check-exploratory-tunnel-evidence.sh
    git diff --check

Run every new Plan 264 focused test exactly by name (retained
Plan 263 rows plus composition-gate rows).

After re-proving receipt + multicell on the new SHA, run every
mandatory epoch twice (fresh mesh per epoch per pass) on the
same implementation SHA. Closure also requires exact-head
ordinary CI green on Quality (ubuntu/macos), MSRV, Dependency
policy.

## 10. Acceptance criteria

Plan 264 closes only when all are directly evidenced (plus all
retained Plan 263 criteria still green):

1. Plan 263 retained evidence remains traceable.
2. Receipt + IBGW multicell re-proven on the implementation
   SHA (healthy-window baselines).
3. Three source locks hold.
4. No production `crates/*/src` diff (scoping-only).
5. Every mandatory epoch passes twice on the same SHA, each
   pass on a fresh mesh, unmerged (per-epoch manifests
   `plan: 264`).
6. Full workspace verification passes.
7. Exact-head ordinary CI passes all four jobs.
8. Registry/roadmap/support/conformance/dossier agree on Plan
   264 as the sole dependency-ready M11 closure authority (or
   mark the ADR 0026 one-family M11 experimental qualification
   passed, if the per-epoch matrix closes).
9. No product default/capability/version/public-network change.
10. No critical/high finding remains open.

Only then may the unblock audit mark the ADR 0026 one-family M11
experimental qualification passed and consider M12 planning
ready.

## 11. Closure evidence required

Write `plans/closure/transit-tunnels/264-status.md` with
implementation/closure SHAs, reference pin/version, retained
source-lock proofs, scoping changes with no-production-diff
proof, receipt + multicell re-proofs, every per-epoch
same-SHA two-pass manifest, sustainability timings/failures,
full verification, exact-head CI,
security/resource/concurrency/migration reviews, findings by
severity, unblock audit. If a new boundary appears, close
retained/blocked and register only the narrow successor exposed
by the evidence.

## 12. Handoff order

1. scope the per-epoch lane (isolation/composition gates,
   no tuning);
2. re-prove receipt + multicell once (baselines still hold
   on the new SHA);
3. run every mandatory epoch twice (fresh mesh per pass);
4. run the complete local verification floor;
5. obtain exact-head ordinary CI green;
6. close only if all §10 criteria are directly evidenced.

Do not create Plan 265 pre-emptively. The next plan is
registered only if Plan 264 execution exposes a new bounded
defect or qualification boundary.
