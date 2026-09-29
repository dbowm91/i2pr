# Plan 263 — M11 qualification-sustainability corrective

Status at registration:
**registered-m11-qualification-sustainability-corrective-ready**

Scope refinement baseline:
`514bf1237e86fde21e17fc98c743eb52852edd99`

Planning reconciliation through the Plan 262 closure commit is
planning/specification-only after the implementation SHA above;
it does not change the production source baseline inherited from
Plan 262. The implementation agent MUST start from the latest
`main`, record the exact pre-implementation HEAD in the handoff,
and bind every counted external attempt to the eventual
implementation SHA. The historical registration SHA is planning
provenance, not a qualification SHA.

Original registration baseline and Plan 262 closure authority
remain recorded in
`plans/closure/transit-tunnels/262-status.md`.

Corrects:

- `plans/implementation/transit-tunnels/262-m11-self-delivery-loopback-corrective.md`
  (§12 WP H / §15 sustainability stop: two same-SHA complete
  attempts stop on environmental signatures after the semantic
  corrective is proven);
- `plans/closure/transit-tunnels/262-status.md` (sustainability
  boundary: attempt1 IBGW-data A-via-B relay zero ingress,
  745.9 s; attempt2 SAM read timeout, 201.5 s; diags 1–3 flake
  class vs diag4 receipt-closed on the same SHA).

Retains:

- Plan 262 WP A/B/C/D/E production corrective in full
  (dedicated `TransitGatewayData`, exact receive-id ownership,
  source-neutral `route_ibgw_gateway` seam, OBEP TUNNEL-to-self
  local branch with `LocalIbgwDelivered`/`LocalIbgwDropped`, no
  synthetic peer/index mutation; Participant/OBEP peer locks
  unchanged);
- Plan 262 receipt proof (diag4 `terminal-garlic-self:0/
  ingress:6/socket:1` with tuple-bound multicell socket receipt)
  as the semantic baseline; do not re-prove ownership, do not
  touch production routing code;
- Plan 260 WP A/B/F + Plan 261 WP A/B lane work + Plan 262 three
  source locks + 5 + 4 + 7 regressions + driver self-loop
  mapping + `plan: 262` manifest shape (bump to `plan: 263`
  only in the qualified lane, with checker updates);
- Exact-pinned i2pd 2.61.0 @
  `635b013a612ff47278ef02acf8580a28e10e26c5`, unmodified,
  loopback-only, fresh datadirs per run, no public fallback.

Reference authority remains exact-pinned, unmodified i2pd 2.61.0.
Normative protocol authority is unchanged from Plan 262
(IBGW receive-id routing without creator affinity; tunnel ids as
per-hop routing identifiers).

## 1. Objective

Sustain the controlled mesh long enough to execute the complete
carried-forward M11 matrix twice on one SHA, without touching
production routing code and without tuning until green.

This is a **narrow lane-harness sustainability corrective plus
final qualification pass**. It does not change wire format, add a
task/channel/queue, raise quotas, inflate timeouts, alter
RouterInfo/router.version, enable public transit, or revise the
Plan 262 ownership semantics. Production `crates/*/src` diff must
stay empty (test/lane/checker/workflow + planning/spec only),
like Plan 261's zero-production-diff precedent.

## 2. Why this scope is required

Plan 262 proved the semantic corrective (B3 flipped, receipt
closes on a healthy mesh in 103.9 s) but the full matrix cannot
sustain:

- Attempt1 (745.9 s) dies in the IBGW data epoch (`m11-tx-ibgw`
  A-via-B unmanaged relay, zero ingress over four rounds).
- Attempt2 (201.5 s) dies before any data epoch (A SAM read
  timeout).
- Diags 1–3 show the same flake class (B floodfill churn,
  receiver never ready, socket-starved under A declines) vs
  diag4's healthy-mesh close on the identical SHA.

The production seam is never entered for the failed epochs, so no
production row fails. The defect is lane-harness sustainability
(session freshness, relay topology robustness, SAM discipline),
not routing. A narrow successor that hardens the harness without
production change is the correct size. Do not pre-register Plan
264; the M11 dependency graph remains linear at Plan 263.

If execution exposes a defect outside this harness boundary
(e.g., a new crypto/fragment/pool routing defect with healthy-mesh
stop provenance), stop and register the narrow successor.

## 3. Frozen invariants

### 3.1 Reference/network

- i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Reference source clean/unmodified.
- Loopback-only qualification mesh.
- Public reseed/network disabled.
- Fresh datadirs/ports/evidence root for every counted complete
  attempt.
- No reference patching, LD_PRELOAD, fake peer state, or public
  fallback.

### 3.2 Product authority

- Ordinary i2pr transit remains disabled.
- No RouterInfo transit capability.
- No router.version change.
- No public transit config option.
- No M12 implementation.
- No production `crates/*/src` diff (harness-only corrective).

### 3.3 Harness ownership

- Existing finite timeouts remain (no inflation to fit).
- No quota enlargement (no new tasks/channels/queues, no raised
  ceilings).
- No repeated restart loop; no "retry until two passes". Each
  complete attempt is one fresh execution and its result is
  retained.
- Receipt-first ordering is required (proven healthy-mesh path
  first, then the full matrix).
- Diagnostic attempts never satisfy final rows; two final
  attempts may not merge evidence.

### 3.4 Evidence

- Raw reference logs remain diagnostic input only.
- Counted evidence contains public hashes/ids/counts/status only.
- Diagnostic attempts never satisfy final rows.
- Two final attempts may not merge evidence.

## 4. Work package A — sustain the A↔B↔i2pr mesh

Harden only the lane harness (driver + runner + workflow),
bounded and fail-closed:

1. Session freshness: ensure A/B SAM sessions and SSU2 sessions
   are freshly established before each counted epoch (same
   precedent as Plan 260 freshness gates); stale/idle links die
   silently mid-run and must be re-established explicitly, never
   retried blindly.
2. Relay robustness: the IBGW data epoch's A-via-B unmanaged
   relay is the observed stop; bound its setup (explicit-peer
   B selection, B floodfill self-listing, A→B session proof)
   before any payload send, and fail closed with the exact
   relay signature when B cannot relay (do not force the epoch
   with direct-sender substitution; the topology is the row).
3. SAM discipline: A/B SAM bridges stay stock config with
   fail-closed env gates (Plan 261 precedent); a SAM read
   timeout fails the attempt with the exact tail, never
   retried in-process.
4. No tuning: timeouts, quotas, ceilings, retry budgets, and
   message sizes stay exactly as in Plan 262. Any change that
   would make the lane pass by enlarging resources is forbidden;
   stop instead.

## 5. Work package B — receipt-first full matrix

Run the existing Plan 262 receipt-first full matrix unchanged
semantically (manifest `plan: 263`):

- OBEP unfragmented + fragmented semantic delivery;
- IBGW live receipt and multicell bounded emission;
- Participant far-side forwarding proof;
- exact accepted-registration cardinality;
- typed bandwidth option/reply evidence;
- code-30 rejection with zero state;
- replay suppression;
- logical >600-second expiry and post-expiry drop;
- cancellation all-dimensional drain;
- session-close removal with unrelated peer retained;
- real runtime restart, zero new state, re-established sessions,
  fresh accepted build;
- fragment-id hardening;
- source locks and all fail-closed environment gates;
- B-sender receipt (WP F flip: `terminal-garlic-self:0/
  ingress:≥1/socket:1` with tuple-bound multicell socket receipt).

Then execute **two complete independent attempts on one
implementation SHA**, each with fresh A/B datadirs, ports, and
evidence root. No cross-attempt merge.

## 6. Compatibility / migration / security

No user migration. No change to config schema, CLI, SAM/I2CP
public API, RouterInfo capabilities, router.version,
public-network behavior, or default transit-disabled
construction. Security interpretation unchanged from Plan 262
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
- two complete runs reach a new repeatable sustainability
  boundary after all semantic rows pass (narrow successor owns
  that boundary; do not widen this plan);
- public network, false capability advertisement, reference
  patching, quota/timeout enlargement, or a new dependency would
  be needed.

Do not weaken or retire the receipt row. Do not touch production
routing code to go green.

## 8. Required focused tests

At minimum (all retained green plus):

1. three exact-pinned source locks (self-loopback,
   gateway-by-receive-id, no-creator-affinity);
2. five runtime-neutral IBGW regressions;
3. four service regressions (exact id, creator-not-auth,
   third-party, participant lock);
4. seven live-owner self-loop/negative rows;
5. B SAM gate + B-sender rows;
6. receipt flip (`terminal-garlic-self:0/ingress:≥1/socket:1`
   with tuple-bound multicell);
7. two-attempt checker rejects one attempt / mixed SHAs /
   cross-attempt merge;
8. diagnostic-only epoch cannot satisfy counted closure;
9. missing env fails before network startup;
10. ordinary product transit remains disabled.

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

Run every new Plan 263 focused test exactly by name (retained
Plan 262 rows plus any new harness rows).

After a passing receipt diagnostic, run two complete
fresh-datadir external attempts on the same implementation SHA.
Closure also requires exact-head ordinary CI green on Quality
(ubuntu/macos), MSRV, Dependency policy.

## 10. Acceptance criteria

Plan 263 closes only when all are directly evidenced (plus all
retained Plan 262 criteria 1–21, 24, 29–30 still green):

1. Plan 262 retained evidence remains traceable.
2. Plan 262 B3 flip remains the starting point
   (`terminal-garlic-self:0` with local ingress on a healthy
   mesh).
3. Three source locks hold.
4. No production `crates/*/src` diff (harness-only).
5. Receipt closes (`terminal-garlic-self:0/ingress:≥1/socket:1`
   with tuple-bound multicell) on the implementation SHA.
6. Complete external attempt 1 passes every mandatory row.
7. Complete external attempt 2 passes every mandatory row on the
   same SHA with fresh datadirs/ports/evidence, unmerged.
8. Full workspace verification passes.
9. Exact-head ordinary CI passes all four jobs.
10. Registry/roadmap/support/conformance/dossier agree on Plan
    263 as the sole dependency-ready M11 closure authority (or
    mark the ADR 0026 one-family M11 experimental qualification
    passed, if the matrix closes).
11. No product default/capability/version/public-network change.
12. No critical/high finding remains open.

Only then may the unblock audit mark the ADR 0026 one-family M11
experimental qualification passed and consider M12 planning
ready.

## 11. Closure evidence required

Write `plans/closure/transit-tunnels/263-status.md` with
implementation/closure SHAs, reference pin/version, retained
source-lock proofs, harness changes with no-production-diff
proof, receipt re-proof, both complete same-SHA manifests,
sustainability timings/failures, full verification, exact-head
CI, security/resource/concurrency/migration reviews, findings by
severity, unblock audit. If a new boundary appears, close
retained/blocked and register only the narrow successor exposed
by the evidence.

## 12. Handoff order

1. sustain the mesh harness (freshness/relay/SAM discipline,
   no tuning);
2. re-prove receipt once (flip still holds on the new SHA);
3. run the receipt-first full matrix;
4. run the complete local verification floor;
5. run two complete fresh-datadir same-SHA external attempts;
6. obtain exact-head ordinary CI green;
7. close only if all §10 criteria are directly evidenced.

Do not create Plan 264 pre-emptively. The next plan is
registered only if Plan 263 execution exposes a new bounded
defect or qualification boundary.
