# Plan 262 — M11 self-delivery loopback corrective (OBEP TUNNEL-to-self local ingress)

Status at registration:
**registered-m11-self-delivery-loopback-corrective-ready**

Baseline: Plan 261 closure head (production tree identical to the
Plan 261 implementation head plus the closure record itself;
exact SHAs recorded in `plans/closure/transit-tunnels/261-status.md`).

Corrects:
- `plans/implementation/transit-tunnels/261-m11-b-sender-receipt-requalification.md`
  (§10 stop #2 fired: B-sender garlics die as OBEP TUNNEL
  `NoActiveSession` before IBGW ingress).
- `plans/closure/transit-tunnels/261-status.md` (B3 boundary).

Retains:
- Plan 260 WP A/B/F: seven creator-owned inbound source locks,
  explicit-peer inbound lock, per-registration/per-role fragment-id
  hardening with regressions.
- Plan 260 receipt-epoch harness: six-field `ReceiptTuple`
  validator, `Epoch::IbgwReceipt` keys, `I2PR_M11_ONLY_EPOCH=receipt`
  diagnostic gate.
- Plan 261 WP A/B lane work: four B-side source locks, B SAM
  plumbing (`I2PD_B_SAM_PORT`, fail-closed), `m11-tx-b` B-sender
  session shape, `b-sender-obep-accepted` / `b-leaseset-resolved` /
  `b-sender-outcome` / `send-window-ms` evidence keys, manifest
  schema `v3` / `plan: 261`.
- Plan 261 external facts: B-side LeaseSet resolution via the
  floodfill local store, B outbound `[i2pr]` establishment, and the
  exact B3 terminal signature (4/4 send rounds → self-targeted
  OBEP TUNNEL `NoActiveSession`, zero ingress, zero socket receipt;
  33 background A-originated self-terminals with the same
  signature).

Reference authority remains exact-pinned, unmodified i2pd 2.61.0 at
`635b013a612ff47278ef02acf8580a28e10e26c5`.

## 1. Objective

Close the B3 boundary Plan 261 localized: when an OBEP-endpointed
garlic names the local router as the lease gateway, i2pr must
deliver it to the local IBGW registration instead of dropping it
at the network peer seam. Only then may the Plan 261 B-sender
matrix run to receipt.

This is a **narrow production data-plane corrective**, not a
public-transit enablement plan. No wire change, no new
task/channel/queue, no quota/timeout/resizing change, no
capability/version/advertisement change.

## 2. Why Plan 262 is required

### 2.1 What Plan 261 proved

On a fresh mesh (`m11-plan261-diag-bsender2`, 508 s):

- A receiver (`inbound.length = 1`, `explicitPeers = i2pr`)
  established with four counted `[A,A]` IBGW registrations
  (`0x2276888c`, `0x22d99981`, `0xb25a97f0`, `0xda72a534`).
- B's `m11-tx-b` sender (stock SAM on B, outbound `[i2pr]`)
  established (three B-originated OBEP accepts).
- B resolved A's receiver LeaseSet from its floodfill local store
  (B log: `Store request: LeaseSet2` + `LeaseSet2 updated` for the
  A-published set, then `Requested LeaseSet <b64> found` +
  `New remote LeaseSet added`; zero `Can't request LeaseSet`).
- All four 1400-byte sends addressed COUNTED tunnel ids
  (`0x2276888c` ×3, `0xda72a534` ×1): the B-resolved lease names
  the exact `(i2pr, recv)` tuple.

### 2.2 Where delivery stops (B3)

Each send arrives at i2pr down B's 1-hop outbound tunnel and
decrypts at i2pr's OBEP registration as a delivery-type-TUNNEL
action with `target_router == self`. The daemon path
(`handle_inbound` Deliver arm → `deliver_obep_action` →
`deliver_obep_tunnel`) looks the target up in the session peer
index, which holds only the A/B remote peers, and terminates
with the designed `NoActiveSession` outcome. Ledger: 4/4
`DataDeliveredObep`/`obep-terminal-garlic` with
`next_router == self`; `GatewayDelivered` 0; socket receipt 0.
Thirty-three background A-originated self-terminals carry the
same signature, so the condition is systematic, not send-specific.

### 2.3 Why earlier verification missed it

Every prior M11 send leg addressed a REMOTE lease gateway (B),
so the peer seam always had a mapping and the self-target arm
never executed. No local or external row ever addressed a lease
gatewayed at i2pr itself; the OBEP TUNNEL-to-self path is dead
code in all previously exercised topologies. Stock i2pd does
have this arm (`Transports::PostMessages` injects
self-addressed messages into the local `LoopbackHandler`,
source to be locked by this plan); i2pr has no equivalent.

## 3. Why ready

Hard dependencies are sufficient:

- Plan 261 closure is the localization (exact tuple, stop
  provenance, ledger + driver-evidence + reference-log binding).
- The B-sender lane, the counted-id addressing proof, and the
  terminal-signature instrumentation exist and are exercised.
- No new architectural decision is required: local ingress
  reuses the existing `route_tunnel_gateway` IBGW path.
- The fix is bounded to one delivery arm (see §5).

## 4. Frozen invariants

### 4.1 Reference/network

- i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Reference source clean and unmodified.
- Loopback-only; public reseed/network disabled.
- Fresh datadirs for every counted complete attempt.
- No LD_PRELOAD, source patch, in-memory reference mutation,
  fake peer injection, or public peer.

### 4.2 Product authority

- Ordinary i2pr transit remains disabled; no RouterInfo transit
  capability; no router.version change; no public transit config
  option; no public-network transit; no M12 work.

### 4.3 Protocol/data plane

- No wire-format change (the arm reroutes an already-decoded
  action to the existing local ingress path).
- No new tasks, channels, queues, or per-cell spawning; no
  quota/timeout/resizing change.
- Non-self targets behave bit-identically (peer-seam path
  untouched).
- A self-targeted action for an UNKNOWN local tunnel id, a
  wrong peer, a non-IBGW role, or an expired registration still
  fails closed as a drop (the existing `route_tunnel_gateway`
  `Ok(None)` arms).
- No self-forward beyond the single OBEP TUNNEL-to-self arm
  unless a stop-rule defect explicitly requires a second arm
  (then stop per §10 and narrow further).

### 4.4 Evidence

- Raw reference logs are diagnostic input only; counted
  evidence is sanitized ids/counts/hashes/status only.
- Two attempts may not merge rows. Non-counted diagnostics may
  localize but never satisfy a counted row.

## 5. Work package A — self-delivery arm + source lock

1. Source-lock the reference expectation as a new
   `m11-i2pd-self-loopback-source-lock` row: i2pd delivers
   self-addressed transport messages locally
   (`Transports::PostMessages` self comparison →
   `m_LoopbackHandler`, exact-pinned `libi2pd/Transports.cpp`).
2. Add the bounded production arm: when an OBEP TUNNEL action's
   `target_router` equals the local router hash, ingest the
   already-decoded nested message through the existing local
   IBGW ingress (`route_tunnel_gateway`-equivalent on the local
   registration) instead of the network peer seam. The local
   hash comparison must use the service's own identity, never a
   lane constant.
3. The arm must preserve every existing outcome dimension:
   unknown id / wrong peer / non-IBGW / expired → drop;
   empty forward vector → drop (no fabricated routing facts);
   failures per cell explicit and bounded, no retry.
4. Negative guards: a self-targeted ROUTER-kind action and a
   self-targeted TUNNEL-forward cell keep their current
   terminal behavior unless this plan's own external run proves
   they block the B-sender matrix (then stop per §10; do not
   silently widen the arm).

Required new rows:

    m11-i2pd-self-loopback-source-lock (reference behavior only)
    m11-i2pr-self-tunnel-local-ingress-unit (local test)
    m11-i2pr-self-tunnel-unknown-id-drops-unit (local test)
    m11-i2pr-non-self-tunnel-unchanged-unit (local test)

## 6. Work package B — B-sender matrix requalification

Rerun the Plan 261 B-sender matrix unchanged (same lane, same
rows, manifest `plan: 261` shape retained or explicitly
superseded by a `plan: 262` shape with a checker rule — do not
run a matrix the checker cannot parse). The B3 arm must flip,
with no other row changing:

    m11-i2pd-ibgw-gateway-ingress (on a counted [A,A] id)
    m11-i2pd-ibgw-multicell-bounded (≥2 cells, zero failures)
    m11-i2pd-ibgw-creator-local-tunnel-bound
    m11-i2pd-ibgw-pool-owned-local-dispatch
    m11-i2pd-ibgw-gateway-receipt (exact payload/digest)
    m11-i2pd-ibgw-gateway-receipt-once (no duplicate)

plus the six-field tuple rows (unchanged validator) and the
`b-sender-outcome` signature flipping to
`terminal-garlic-self:0/ingress:>0/socket:1`.

## 7. Work package C — carried-forward lifecycle rows

Once the B-sender receipt gate passes, continue the complete
existing matrix (Plan 261 §8/D: receipt-first order, legacy
OBEP/IBGW/Participant, replay/expiry/cancel/session-close/
restart, fragment-id hardening). Any newly exposed production
defect receives a new corrective plan under the existing stop
policy. Do not weaken the row.

## 8. Compatibility and migration

No user migration. No config-schema, CLI, public SAM/I2CP API,
RouterInfo capability, router.version, product-transit-default,
or public-network change. No new dependency.

## 9. Stop conditions

Stop and record the exact boundary if:

- the self arm changes any non-self delivery outcome;
- the self arm requires a wire-format, task/channel/queue, or
  quota/timeout change;
- a second self arm (ROUTER-kind or TUNNEL-forward cell)
  proves necessary (register the narrower follow-up; do not
  widen silently);
- the B-sender matrix exposes another production defect
  outside this scope;
- public network, reference patching, false capabilities, or a
  new dependency would be needed.

Do not retire the receipt row because the arm is delicate. Do
not merge attempts. Do not relabel diagnostic rows as counted.

## 10. Required focused tests

At minimum (all determine pass/fail by executed command):

1. source lock: i2pd self-loopback (`PostMessages` →
   `LoopbackHandler`);
2. unit: self-targeted TUNNEL action with a known local IBGW
   id ingresses locally (cells emitted toward the committed
   next tuple);
3. unit: self-targeted TUNNEL action with an unknown local id
   drops (no state, no facts);
4. unit: self-targeted TUNNEL action on an expired/non-IBGW
   registration drops;
5. unit: non-self TUNNEL action still traverses the peer seam
   unchanged (regression);
6. B-sender session establishes outbound `[i2pr]` (existing
   row kept green);
7. B-resolved receiver LeaseSet names the counted `(i2pr,
   recv)` tuple (existing row kept green);
8. genuine TunnelGateway ingress on the counted `[A,A]` IBGW
   id (existing gate, must now pass);
9. counted 1400-byte stimulus emits ≥2 cells, zero forward
   failures;
10. `next_tunnel` equals the accepted registration's committed
    tuple;
11. receiver SAM socket receives the exact digest exactly once;
12. six-field tuple validator still rejects all three partial
    bindings and accepts the complete tuple;
13. legacy OBEP/IBGW/Participant + replay/expiry/cancel/close/
    restart rows pass in the receipt-first matrix;
14. two-attempt checker still rejects one attempt / mixed SHAs
    / stale authority;
15. `I2PR_M11_ONLY_EPOCH=receipt` still cannot satisfy the
    counted gate;
16. missing B-SAM env still fails before network startup;
17. ordinary product transit remains disabled.

## 11. Exact verification floor

Run (same floor as Plan 261 §12):

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

Run every new Plan 262 focused test exactly by name. Then
execute two complete fresh-datadir external attempts on the
same implementation SHA with fresh A/B datadirs and exact-head
ordinary CI green (Quality ubuntu, Quality macos, MSRV,
Dependency policy).

## 12. Acceptance criteria

Plan 262 closes only when all are directly evidenced:

1. Plan 260 WP A/B/F and Plan 261 WP A/B lane work remain
   intact.
2. Plan 261 B3 boundary remains traceable as the starting
   point.
3. Reference self-loopback source lock holds on the exact pin.
4. Self-delivery arm lands with no wire/task/queue/quota
   change.
5. Self-target unit rows (ingress/drop/expiry/non-self
   regression) pass.
6. B SAM bridge + missing-env gate still green.
7. B outbound `[i2pr]` establishment row passes.
8. B-resolved LeaseSet names the counted `(i2pr, recv)` tuple.
9. Genuine TunnelGateway ingress on the counted `[A,A]` IBGW
   id.
10. Counted stimulus forces ≥2 TunnelData cells, zero
    failures.
11. `next_tunnel` matches the committed creator-local tuple.
12. Pool-owned LOCAL dispatch observed on the receiver path.
13. Receiver SAM socket receives the exact payload/digest
    exactly once.
14. Full six-field tuple bound in sanitized evidence.
15. Plan 257 typed cardinality/bandwidth rows pass in the
    receipt-first matrix.
16. Participant far-side, replay, expiry, cancellation-drain,
    session-close retention, and restart + fresh-build rows
    pass.
17. Fragment-id hardening rows stay green.
18. Complete external attempt 1 passes every mandatory row.
19. Complete external attempt 2 passes every mandatory row
    (same SHA, independent fresh datadirs).
20. Full workspace verification passes; exact-head CI green on
    all four required jobs.
21. Registry, roadmap, support ledger, conformance matrix, and
    tunnel dossier agree on the final state.
22. No product default/capability/version/public-network
    change.
23. No critical/high finding remains open.

Only then may the unblock audit mark the ADR 0026 one-family
M11 experimental qualification passed and reconsider M12
registration.

## 13. Closure evidence required

Write `plans/closure/transit-tunnels/262-status.md` with:

- implementation and closure SHAs; exact reference pin/version;
- reference self-loopback source lock;
- self-delivery arm (files, arm condition, preserved drop arms);
- B-sender build/OBEP evidence; receiver `[A,A]` IBGW evidence;
- complete receipt tuple; LeaseSet binding; SAM payload receipt;
- flipped `b-sender-outcome` signature;
- mesh-sustainability budget (per-epoch times, test-failure
  onset);
- all carried-forward lifecycle rows; both same-SHA manifests;
- full verification floor; exact-head CI run;
- security/resource/concurrency review; findings by severity;
- roadmap/unblock audit.

If the plan stops at a new boundary, close as retained/blocked
with the exact failing tuple and register the narrow successor.
Do not convert a stopped row into a pass.

## 14. Handoff order

1. source-lock the reference self-loopback expectation;
2. implement the bounded self-delivery arm with unit rows;
3. prove non-self behavior unchanged (regression);
4. prove unknown/expired/non-IBGW self targets still drop;
5. rerun the B-sender receipt epoch (diagnostic subset first);
6. bind the six-field tuple + socket receipt;
7. carry the remaining lifecycle rows receipt-first to
   completion;
8. harden checker/manifest rules for Plan 262;
9. run the full local floor;
10. run two complete same-SHA external attempts;
11. obtain exact-head ordinary CI green;
12. close only if all 23 acceptance criteria are directly
    evidenced.

Do not begin by weakening the receipt row or by re-plumbing
the sender leg. The B3 terminal signature stays the starting
point until the arm flips it.
