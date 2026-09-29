# Plan 261 — M11 B-sender receipt requalification (forward-path garlic delivery + B-side LeaseSet resolution)

Status at registration:
**registered-m11-b-sender-receipt-requalification-ready**

Baseline: `48ee9ec13f4c685b27224027241b8da2baa4c793` (Plan 260
implementation head; the Plan 260 closure commit adds planning/spec
prose only, no code).

Corrects:
- `plans/implementation/transit-tunnels/260-m11-creator-owned-inbound-receipt-topology-and-planning-authority-corrective.md`
  (§8 B-sender experiment, unexecuted as written).
- `plans/closure/transit-tunnels/260-status.md` (B1/B2 stop boundaries).

Retains:
- Plan 260 WP A: all seven creator-owned inbound source locks
  (`m11-i2pd-inbound-last-hop-targets-creator-source-lock` through
  `m11-i2pd-transit-endpoint-vs-inbound-owner-distinction-source-lock`).
- Plan 260 WP B: explicit-peer inbound-selection source lock
  (`m11-i2pd-explicit-peer-inbound-selection-source-lock`).
- Plan 260 WP F: per-registration/per-role fragmented IBGW message-id
  hardening (`claim_ibgw_fragment_id` / `claim_fragment_id`) with all five
  interleaving/wrap/distinctness regressions.
- Plan 260 receipt-epoch harness: six-field `ReceiptTuple` validator
  (`MissingCreatorLocalTunnel` / `PoolOwnerMismatch` / `LeaseSetMismatch`),
  `Epoch::IbgwReceipt` evidence keys, `gateway-receipt-superseded-note`,
  and the `I2PR_M11_ONLY_EPOCH=receipt` non-counted diagnostic gate.
- Plan 260 external facts: receiver 1-hop `[i2pr]` inbound builds accepted
  as IBGW `[A,A]` with SAM STATUS OK (diag-receipt2); legacy
  OBEP/IBGW/Participant build + multicell emission rows (attempt-1).
- Plans 250–259 admission, build, ownership, typed evidence, state
  snapshot, bandwidth, replay/expiry/cancel/session-close/restart, and
  external-lane infrastructure unless this plan localizes a new defect.

Reference authority remains exact-pinned, unmodified i2pd 2.61.0 at
`635b013a612ff47278ef02acf8580a28e10e26c5`.

## 1. Objective

Close the two exact boundaries Plan 260 localized, in order, without
weakening any row:

1. **B2 (primary): forward-path garlic death.** With the receiver on A
   established through its 1-hop `[i2pr]` inbound, an A-side sender
   (outbound `[B]`) reaches B's endpoint (type-11 966-byte fragments
   reassembled at send time) but zero ingress ever reaches any counted
   `[A,A]` IBGW id over four send rounds, and the receiver socket gets
   0. Execute the experiment Plan 260 §8 actually specifies — a **B-side
   sender destination** delivering to A's published lease — so no
   destination garlic transits B's endpoint at all.
2. **B1 (secondary): late-run mesh sustainability.** Full-matrix runs
   degrade after ~6 minutes (tunnel tests fail with no floodfill,
   builds decline/expire, receiver establishment flakes 2/3). Scope
   the counted matrix to fit the healthy window, or prove the window
   suffices — without timeout inflation, retry-until-green, or quota
   resizing.

Only then may the six-field receipt tuple bind and the receiver-socket
receipt row pass.

This is a **corrective capability-qualification plan**, not a
public-transit enablement plan.

## 2. Why Plan 261 is required

### 2.1 Plan 260 proved the build half and localized the delivery half

Plan 260's fresh-mesh diagnostic proved the creator-owned inbound
topology is real and lane-buildable:

- A receiver destination (`inbound.length = 1`, zero variance,
  `explicitPeers = i2pr`) produces 1-hop inbound builds that i2pr
  accepts as typed IBGW with `next_router == A` (four `[A,A]` accepts
  in `m11-plan260-diag-receipt2`, plus `[B,B]` router-pool analogues).
- A reports SAM SESSION STATUS OK (LeaseSet + outbound), i.e. the
  creator establishes its side.

What never happens is delivery: four 1400-byte send rounds through the
A-side sender's outbound `[B]` reassemble as type-11 garlics at B's
endpoint (`TransitTunnel: handle msg for endpoint ...` + `Handle
fragment of 966 bytes, msg type 11`) with zero ingress on every counted
`[A,A]` id and zero socket receipt. B-side debug shows endpoint
reassembly with no onward delivery to i2pr — the forward-direction
sibling of the Plan 259 Fork-2 transit-endpoint mechanism. The
co-timed `Garlic: Type local` line is consistent with LOCAL-labeled
sender garlics (same-router sender/receiver confusion in the reference
is not excluded) but is not isolated to our fragments, so it stays
corroborating — not closing — evidence.

### 2.2 The committed lane cannot route around B's endpoint

Three candidate send paths, three exact dispositions:

- A-sender via `[B]` (committed): dies at B's endpoint (externally
  proven, diag-receipt2). Blocked by B2.
- A-sender via `[i2pr]` (diagnostic hypothesis, statically refuted):
  i2pr's OBEP data forward requires a session peer mapping for
  `next_router` (`deliver_tunnel_data_forward` →
  `NoActiveSession`); no self-loopback exists in the data plane, and
  adding one is a production data-plane redesign, out of scope here.
- B-sender per Plan 260 §8 (never executed): needs (a) SAM on
  reference B (currently disabled in the lane) and (b) B-side
  resolution of A's receiver LeaseSet with no floodfill. Both are
  lane work owned by this plan.

### 2.3 Mesh sustainability bounds the counted matrix

Attempt-1 (minute 7–9.5) never established the receiver (1 IBGW build,
both from B); diag-receipt3 (fresh mesh) established nothing either
(6 IBGW accepts but 14 code-30 rejections under background churn).
Only diag-receipt2 (fresh mesh) established. Receiver establishment is
~1/3 across executions, and A's outbound plane collapses within ~2
minutes even when established (declined builds, expired pendings,
failing tunnel tests — structural without floodfill). The counted
matrix must fit the healthy window or prove it does; inflating
timeouts, resizing production quotas, or clearing state periodically
to fit are all forbidden.

## 3. Why ready

Hard dependencies are sufficient:

- Plan 260 WP A/B/F implementation + local rows are green on the
  closure head (3104 workspace tests, all gate scripts).
- The receipt-epoch harness, tuple validator, and receipt-only
  diagnostic gate exist and are exercised.
- The exact failure signatures (B1/B2) are recorded with ledger,
  driver-evidence, and reference-log provenance — the work starts
  from localization, not from zero.
- No new architectural decision is required unless B-side LeaseSet
  resolution needs one (then stop per §10 and register the narrower
  lane-plumbing plan).

## 4. Frozen invariants

### 4.1 Reference/network

- i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Reference source clean and unmodified.
- Loopback-only; public reseed/network disabled.
- Fresh datadirs for every counted complete attempt.
- No LD_PRELOAD, source patch, in-memory reference mutation, fake
  peer injection, or public peer.
- SAM on B is enabled via stock `sam.enabled` lane configuration
  only (same mechanism as A today); no new reference behavior.

### 4.2 Product authority

- Ordinary i2pr transit remains disabled; no RouterInfo transit
  capability; no router.version change; no public transit config
  option; no public-network transit; no M12 work.

### 4.3 Protocol/data plane

- Plan 260 fragment-id hardening stays canonical; no wire change.
- No delivery-type deviation to make receipt pass.
- No production quota/timeout/resizing change to fit the lane.
- No self-forward/loopback data-plane redesign (separate plan if
  ever wanted).
- Existing replay/reassembly/resource bounds remain.

### 4.4 Evidence

- Raw reference logs are diagnostic input only; counted evidence is
  sanitized ids/counts/hashes/status only.
- Every receipt claim must bind one epoch's build, registration,
  downstream local tunnel, destination pool, emission,
  reference-local dispatch, and receiver socket delivery.
- Two attempts may not merge rows. Non-counted diagnostics
  (`I2PR_M11_ONLY_EPOCH=receipt` and any new subset flag) may
  localize but never satisfy a counted row.

## 5. Work package A — B-side sender lane plumbing

1. Enable the stock SAM bridge on reference B (loopback port,
   same conf mechanism as A; new `I2PD_B_SAM_PORT` lane env).
2. Add a B-side sender session (`m11-tx-b`) with `inbound.length = 0`,
   `outbound.length = 1`, zero variance, `explicitPeers = <i2pr>`:
   B's outbound `[i2pr]` puts i2pr (not B) at the outbound endpoint,
   so destination garlics never reassemble at B's endpoint.
3. Resolve A's receiver LeaseSet on B **without floodfill and without
   reference patching**. Candidate mechanisms in order of
   preference:
   - (a) live NetDB lookup through the lane's existing RI/LeaseSet
     plumbing if B can resolve A-local client LeaseSets (prove with
     a lookup row before sending payload);
   - (b) driver-mediated transport of A's locally published LeaseSet
     bytes into B's NetDB **only if** a source-supported,
     unmodified-reference ingestion path exists (source-lock it;
     file-drop tricks without source support are forbidden);
   - (c) stop: if neither is source-supported, record the exact
     lookup boundary and close blocked (do not fake the LeaseSet,
     do not hand-build garlics, do not weaken the row).
4. Source-lock the B-side outbound selection + endpoint-forward
   expectations actually used (explicit-peer outbound selection,
   outbound-endpoint TUNNEL-forward to the lease gateway), as new
   `m11-i2pd-*-source-lock` rows. Do not conflate with the WP A
   inbound locks.

Required new rows:

    m11-i2pd-b-sam-bridge-enabled-source-lock (config surface only)
    m11-i2pd-explicit-peer-outbound-selection-source-lock
    m11-i2pd-outbound-endpoint-tunnel-forward-source-lock
    m11-i2pd-b-leaseset-resolution-source-lock (mechanism a/b + proof)

## 6. Work package B — B-sender receipt epoch

Replace the counted send leg (keep the A-side receiver construction
unchanged — it is proven):

    B sender destination (SAM on B, outbound [i2pr])
      -> destination-addressed garlic for A receiver (B-resolved LeaseSet)
      -> B outbound delivery toward A's published lease
      -> TunnelGateway to i2pr (accepted [A,A] IBGW receive id)
      -> Plan 258 canonical multicell emission (1400 B stimulus kept)
      -> A creator-local InboundTunnel id
      -> creator-owned inbound transform/reassembly
      -> LOCAL I2NP garlic
      -> receiver TunnelPool::ProcessGarlicMessage
      -> A receiver SAM socket, exactly once

A pass requires all of Plan 260 §8/D rows 1–8, with the sender leg
 rebound to B:

    m11-i2pd-ibgw-gateway-ingress (on the counted [A,A] id)
    m11-i2pd-ibgw-multicell-bounded (≥2 cells, zero forward failures)
    m11-i2pd-ibgw-creator-local-tunnel-bound
    m11-i2pd-ibgw-pool-owned-local-dispatch
    m11-i2pd-ibgw-gateway-receipt (exact payload/digest)
    m11-i2pd-ibgw-gateway-receipt-once (no duplicate)

plus the six-field tuple rows from Plan 260 §7 (unchanged
validator; `MissingCreatorLocalTunnel` / `PoolOwnerMismatch` /
`LeaseSetMismatch` still reject partial bindings).

The A-side sender leg is deleted from the counted matrix (its
B-endpoint death is retained as the Plan 260 B2 boundary, not
retried). The `I2PR_M11_ONLY_EPOCH=receipt` diagnostic keeps working
and gains the B-sender leg.

## 7. Work package C — mesh-sustainability budget

Fit the counted matrix into the healthy window without tuning:

1. Measure, don't tune: record per-epoch wall times and A's
   tunnel-test failure onset across executions (ledger + A-log
   already carry both).
2. Order the counted matrix receipt-first (Plan 260 WP G order):
   receiver construction → B-sender receipt → legacy OBEP/IBGW data
   → Participant → replay/expiry/cancel/session-close/restart.
   Epoch order is not a pass gate; the two-attempt rule is
   order-independent. Record the order change as WP-G fidelity, not
   as an optimization.
3. If the full matrix still exceeds the healthy window, STOP and
   record the exact overrun (which epoch, which health signal) —
   do not split counted attempts, do not inflate timeouts, do not
   resize quotas. That overrun owns the next corrective, not this
   plan.

## 8. Work package D — preserve Plan 257 completion rows

Once the B-sender receipt gate passes, continue the complete
existing matrix (same rows as Plan 260 §11/G). Any newly exposed
production defect receives a new corrective plan under the existing
stop policy. Do not weaken the row.

## 9. Compatibility and migration

No user migration. No config-schema, CLI, public SAM/I2CP API,
RouterInfo capability, router.version, product-transit-default, or
public-network change. New lane env (`I2PD_B_SAM_PORT`) is
test-only. No new dependency.

## 10. Stop conditions

Stop and record the exact boundary if:

- B-side LeaseSet resolution has no source-supported,
  unmodified-reference mechanism in this lane (WP A mechanism c);
- B's outbound `[i2pr]` builds but i2pr's OBEP data forward drops
  the garlics before IBGW ingress (localizes an i2pr data-plane
  defect → separate production corrective, not this lane);
- a correctly bound creator-owned inbound tunnel receives the
  exact cells but fails before destination-pool garlic processing;
- destination-pool garlic processing occurs but the receiver SAM
  socket still sees no payload;
- the matrix demonstrably exceeds the healthy mesh window (§7.3);
- another production crypto/data-plane defect outside this scope
  appears;
- public network, reference patching, false capabilities, timeout
  inflation, quota resizing, or a new dependency would be needed.

Do not retire the receipt row because the B-side plumbing is
difficult. Do not merge attempts. Do not relabel diagnostic rows
as counted.

## 11. Required focused tests

At minimum (all determine pass/fail by executed command):

1. source lock: B SAM bridge config surface enables loopback SAM;
2. source lock: explicit-peer outbound selection path;
3. source lock: outbound-endpoint TUNNEL-forward to lease gateway;
4. source lock: B-side LeaseSet resolution mechanism + proof row;
5. B-sender session establishes with outbound `[i2pr]` (ledger
   OBEP accept, next == i2pr);
6. B-resolved receiver LeaseSet names the counted `(i2pr, recv)` tuple;
7. genuine TunnelGateway ingress on the counted `[A,A]` IBGW id;
8. counted 1400-byte stimulus emits ≥2 cells, zero forward failures;
9. `next_tunnel` equals the accepted registration's committed tuple;
10. receiver SAM socket receives the exact digest exactly once;
11. six-field tuple validator still rejects all three partial
    bindings and accepts the complete tuple (existing rows kept green);
12. legacy OBEP/IBGW/Participant + replay/expiry/cancel/close/restart
    rows still pass in the reordered matrix;
13. two-attempt checker still rejects one attempt / mixed SHAs /
    stale Plan 260 "blocked" rows as current authority;
14. `I2PR_M11_ONLY_EPOCH=receipt` still cannot satisfy the counted gate;
15. missing B-SAM env fails before network startup;
16. ordinary product transit remains disabled.

## 12. Exact verification floor

Run (same floor as Plan 260 §17):

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

Run every new Plan 261 focused test exactly by name. Then execute
two complete fresh-datadir external attempts on the same
implementation SHA with fresh A/B datadirs and exact-head ordinary
CI green (Quality ubuntu, Quality macos, MSRV, Dependency policy).

## 13. Acceptance criteria

Plan 261 closes only when all are directly evidenced:

1. Plan 260 WP A/B/F implementation and local rows remain intact.
2. Plan 260 B1/B2 boundaries remain traceable as the starting point.
3. B SAM bridge enabled via stock config; missing env fails closed.
4. B-side LeaseSet resolution is source-supported and proven by a
   lookup row before any payload send.
5. B sender establishes outbound `[i2pr]` (typed OBEP accept).
6. B-resolved LeaseSet names the counted `(i2pr, recv)` tuple.
7. Genuine TunnelGateway ingress on the counted `[A,A]` IBGW id.
8. Counted stimulus forces ≥2 TunnelData cells, zero failures.
9. `next_tunnel` matches the committed creator-local tuple.
10. Pool-owned LOCAL dispatch observed on the receiver path.
11. Receiver SAM socket receives the exact payload/digest exactly once.
12. Full six-field tuple bound in sanitized evidence.
13. Plan 257 typed cardinality/bandwidth rows pass in the reordered matrix.
14. Participant far-side, replay, expiry, cancellation-drain,
    session-close retention, and restart + fresh-build rows pass.
15. Fragment-id hardening rows stay green.
16. Complete external attempt 1 passes every mandatory row.
17. Complete external attempt 2 passes every mandatory row (same SHA,
    independent fresh datadirs).
18. Full workspace verification passes; exact-head CI green on all
    four required jobs.
19. Registry, roadmap, support ledger, conformance matrix, and tunnel
    dossier agree on the final state.
20. No product default/capability/version/public-network change.
21. No critical/high finding remains open.

Only then may the unblock audit mark the ADR 0026 one-family M11
experimental qualification passed and reconsider M12 registration.

## 14. Closure evidence required

Write `plans/closure/transit-tunnels/261-status.md` with:

- implementation and closure SHAs; exact reference pin/version;
- B-side LeaseSet-resolution mechanism + source locks;
- B-sender build/OBEP evidence; receiver `[A,A]` IBGW evidence;
- complete receipt tuple; LeaseSet binding; SAM payload receipt;
- mesh-sustainability budget (per-epoch times, test-failure onset);
- all carried-forward lifecycle rows; both same-SHA manifests;
- full verification floor; exact-head CI run;
- security/resource/concurrency review; findings by severity;
- roadmap/unblock audit.

If the plan stops at a new boundary, close as retained/blocked
with the exact failing tuple and register the narrow successor. Do
not convert a stopped row into a pass.

## 15. Handoff order

1. source-lock B-side SAM/LeaseSet/outbound/forward expectations;
2. plumb B SAM + B-sender session into the lane (fail-closed env);
3. prove B-side LeaseSet resolution before any payload send;
4. prove B outbound `[i2pr]` establishment;
5. execute the B-sender receipt epoch (diagnostic subset first);
6. bind the six-field tuple + socket receipt;
7. reorder the counted matrix receipt-first (WP G fidelity);
8. carry the remaining lifecycle rows to completion;
9. harden checker/manifest rules for Plan 261;
10. run the full local floor;
11. run two complete same-SHA external attempts;
12. obtain exact-head ordinary CI green;
13. close only if all 21 acceptance criteria are directly evidenced.

Do not begin by weakening the receipt row or by re-plumbing the
A-sender leg. The A-sender B-endpoint death stays retained as the
Plan 260 B2 boundary.
