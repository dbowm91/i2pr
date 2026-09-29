# Plan 262 — M11 IBGW ingress ownership + self-delivery loopback corrective: status

**retained-m11-ibgw-ownership-corrected-receipt-proven-full-matrix-sustainability-boundary-corrective-required-via-plan263**

## Disposition

Plan 262 executed as a narrow production ownership/data-plane
corrective plus final qualification pass and **stops at one
exactly localized qualification-sustainability boundary**. It does
NOT close the lane:

- The Plan 261 B3 self-delivery loopback gap is corrected and
  proven live: decoded OBEP TUNNEL actions whose target router is
  i2pr itself now enter the local IBGW registration through one
  source-neutral seam without synthetic peer state, and the
  inherited IBGW creator-peer data affinity is removed (receive-id
  / role / expiry ownership, matching exact-pinned i2pd
  `Tunnel.cpp` dispatch and the normative IBGW "allow messages
  from anyone" role). Participant/OBEP previous-peer locks are
  unchanged.
- The B-sender receipt topology closes once on a healthy mesh:
  `b-sender-outcome = terminal-garlic-self:0/ingress:6/socket:1`
  with `gateway-receipt = 1`, `gateway-receipt-once = true`,
  `multicell-bounded = true`, `full-tuple-bound = true`,
  `leaseset-gateway-match = true`, `leaseset-tunnel-match = true`
  (diag4, receipt-only, 103.9 s, fresh datadirs, loopback-only).
  This is the verbatim Plan 262 §10 WP F flip (terminal 0,
  ingress ≥1, socket 1) with all six tuple fields bound and
  ≥2 cells for the 1400-byte stimulus.
- Receipt does NOT generalize to two complete same-SHA full-matrix
  passes. Two complete fresh-datadir attempts on the single
  implementation SHA `514bf1237e86fde21e17fc98c743eb52852edd99`
  both stop before the receipt epoch on mesh-sustainability
  signatures unrelated to the corrected ownership seam:
  attempt 1 (745.9 s) stops in the IBGW data epoch
  (`m11-tx-ibgw` A-via-B relay, zero genuine gateway ingress over
  four multicell-forcing rounds); attempt 2 (201.5 s) stops before
  any data epoch on a SAM read timeout. Three additional
  receipt-only diagnostics bound the flake class (diag1 LeaseSet
  unproven 560.2 s with B floodfill churn, diag2 receiver never
  ready 151.2 s, diag3 ingress-proven-but-socket-starved 257.6 s
  with `0/48/0` under mass A inbound declines + SAM read error).
  No row was weakened. No quota/timeout inflation, no retry-until-
  green, no reference patching, no public fallback, no new
  dependency, no RouterInfo/version/capability advertisement
  change. M11 remains unclaimed/non-advertised; M12 remains
  deferred; the narrow successor is Plan 263
  (qualification-sustainability corrective), registered ready in
  the same commit.

## Commits

- `514bf1237e86fde21e17fc98c743eb52852edd99` — Plan 262
  implementation (see §"Implementation contents" below;
  runtime-neutral IBGW state split, exact receive-id ownership,
  source-neutral seam, OBEP TUNNEL-to-self local branch with
  `LocalIbgwDelivered`/`LocalIbgwDropped`, third-party + negative
  regressions, driver self-loop mapping, three exact-pinned
  source locks, runner manifest `plan: 262`, both M11 checkers
  extended to 171 guarded rows).
- this commit — this record + Plan 263 registration +
  registry/roadmap/support/conformance/dossier reconciliation
  (closure SHA recorded at commit time).
- Follow-up: hosted exact-head CI evidence record (Plan
  258/259/260/261 precedent; local floor fully green below).

Baseline: `ff4dc51bc8c54d1082d3585f7bdb48e2f48fc511` (Plan 261
closure + Plan 262 registration head; planning/spec-only
reconciliation through `f4325248e72967f7d71b348ff405869fc6166fc2`
does not change the production source baseline inherited from
Plan 261).

Reference: unmodified i2pd 2.61.0 @
`635b013a612ff47278ef02acf8580a28e10e26c5` throughout.

## Implementation contents

Production behavior change (narrow ownership/data-plane only; no
wire format, task/channel/queue, quota, config/CLI/API/RI/version
change):

- `crates/i2pr-tunnel/src/transit.rs` —
  dedicated move-only `TransitGatewayData { next_fragment_id }`
  (no `locked_previous_peer`, no replay window; custom `Debug`
  reports `fragment_seeded` only); `TransitDataPlane::InboundGateway`
  `Debug` reports IBGW-specific bounded state only;
  `TransitParticipantData` unchanged; `process_tunnel_data`
  IBGW arm forwards through the one-layer transform without
  participant-style lock/replay (top-level build-provenance gate
  unchanged); `process_tunnel_gateway(gateway, expected_receive,
  now_ms, rng)` asserts exact receive-id equality against the
  registry key, no `previous_peer` argument or check, claims
  per-registration fragment id, Plan 258 fragment + Plan 260
  sequence preserved; five new unit rows
  (`m11_i2pr_ibgw_dedicated_state_has_no_peer_lock_unit`,
  `m11_i2pr_ibgw_exact_receive_id_unit`,
  `m11_i2pr_ibgw_creator_peer_not_data_auth_unit`,
  `m11_i2pr_ibgw_third_party_authenticated_peer_accepted_unit`,
  `m11_i2pr_participant_peer_lock_unchanged_unit`).
- `crates/i2pr-daemon/src/transit_compose.rs` —
  source-neutral `route_ibgw_gateway(tunnel_id, nested, now_ms,
  rng)` (registry-key exact equality, fail-closed on
  unknown/zero/non-IBGW/expired/malformed/cancelled/empty);
  network `route_tunnel_gateway(..., _peer, ...)` delegates
  (authenticated peer is evidence-only, not auth); four new
  service rows (exact id, creator-not-auth, third-party B-sender
  with correct-id accept + wrong-id drop, participant lock).
- `crates/i2pr-daemon/src/transit_owner.rs` —
  `ObepDeliveryOutcome::LocalIbgwDelivered { receive_id,
  next_router, next_tunnel, delivered, failures, nested_len }` +
  `LocalIbgwDropped { receive_id, reason }`;
  `deliver_obep_action(..., now_ms)` branches TUNNEL-to-self
  (`target == hop_identity`) into `deliver_obep_tunnel_to_self`
  (same `route_ibgw_gateway` seam, no synthetic `PeerId`, no
  peer-index insert, no artificial SSU2 serialization; per-cell
  bounded forward with explicit delivered/failures counts);
  non-self TUNNEL and self ROUTER paths bit-identical; seven new
  live-owner rows (live ingress, unknown/non-IBGW drops, no peer-
  index mutation, non-self unchanged, self ROUTER unchanged,
  cancelled refuse).

Test/lane/checker/workflow code:

- `crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs` —
  `DeliveredObep` ledger maps `LocalIbgwDelivered` →
  `GatewayDelivered` (receive/next/cells/failures/nested) and
  `LocalIbgwDropped` → `GatewayDropped`, so the B-sender
  `b-sender-outcome` flip (`terminal-garlic-self:0`) counts genuine
  local IBGW ingress, not relabeled terminals.
- `crates/i2pr-daemon/tests/m11_transit_live_owner.rs` — seven
  Plan 262 self-loop/negative rows (see above) + five
  `deliver_obep_action` call-site updates for the new `now_ms`
  arg; live-owner matrix 34 → 41 rows.
- `tests/integration/m11-transit/run-i2pd.sh` — three exact-pin
  guarded source-lock rows (`m11-i2pd-self-loopback-source-lock`
  via `Transports::PostMessages` + local-hash compare +
  `m_LoopbackHandler.PutNextMessage` + `Flush`;
  `m11-i2pd-tunnel-gateway-by-receive-id-source-lock` via
  `eI2NPTunnelGateway` + payload tunnel id + `GetTunnel` +
  `HandleTunnelGatewayMsg` + `SendTunnelDataMsg`;
  `m11-i2pd-tunnel-gateway-no-creator-peer-affinity-source-lock`
  via absence of ident/creator comparison in dispatch + handler);
  manifest schema `v3` / `plan: 262` (rejects 257/259/260/261).
- `scripts/check-m11-transit-qualification-evidence.sh` — 3 new
  guarded rows (171 total), `plan: 262` manifest gate, Plan 262
  production invariants (dedicated `TransitGatewayData` with no
  lock/replay, no `previous_peer` in gateway fn, `expected_receive`
  binding, `claim_ibgw_fragment_id`, `route_ibgw_gateway`,
  `deliver_obep_tunnel_to_self`, `LocalIbgwDelivered/Dropped`, no
  synthetic `PeerId`/peer-index insert, all 5+4+7 regression names,
  driver `LocalIbgw*` mapping).
- `scripts/check-m11-transit-boundaries.sh` — rules 32–35
  (dedicated IBGW struct, no alias, no lock/replay inside; no
  `previous_peer` in gateway fn, `expected_receive` required;
  source-neutral seam + self branch + typed outcomes + no
  synthetic peer/index mutation; three runner source-lock rows) +
  manifest `plan: 262` gate (rejects 260/261).
- `.github/workflows/m11-transit-external.yml` — unchanged
  two-attempt matrix (attempts 1–2, fail-fast false, fresh
  ports/evidence per attempt); closure uses it for the two
  same-SHA complete attempts below.

## Plan 262 §18 acceptance criteria — requirement-to-evidence matrix

1. **Plan 260/261 retained evidence traceable** — PASS.
   Plan 260 WP A/B/F (creator locks, explicit-peer lock,
   fragment-id hardening, receipt harness) and Plan 261 WP A/B
   (B source locks, B SAM plumbing, `m11-tx-b`, B LeaseSet
   evidence, `b-sender-outcome`, send-window budget) remain in
   tree and green (§"Verification"); Plan 261 B3 signature
   remains the starting boundary (criterion 2).
2. **Plan 261 B3 signature remains the starting boundary** —
   PASS. Four B-originated sends reached i2pr OBEP as TUNNEL
   actions targeting the local router with counted A IBGW ids,
   then died at `NoActiveSession` (`terminal-garlic-self:4/
   ingress:0/socket:0` on `m11-plan261-diag-bsender2`).
3. **Exact-pinned self-loopback source lock holds** — PASS.
   `Transports::PostMessages (ident, msgs)` compares `ident ==
   GetRouterInfo().GetIdentHash()`, queues to
   `m_LoopbackHandler.PutNextMessage`, `Flush()`es locally
   (`Transports.cpp:498–506`); runner
   `m11-i2pd-self-loopback-source-lock` green on every counted
   run.
4. **Exact-pinned TunnelGateway-by-receive-id source lock holds**
   — PASS. `Tunnels` dispatches `eI2NPTunnelGateway` by
   `tunnelID = bufbe32toh(payload)` → `GetTunnel(tunnelID)` →
   `HandleTunnelGatewayMsg` → `SendTunnelDataMsg`
   (`Tunnel.cpp:617–633,705–725`); runner
   `m11-i2pd-tunnel-gateway-by-receive-id-source-lock` green.
5. **Normative IBGW role + reference dispatch establish
   receive-id routing without creator affinity** — PASS.
   `tunnel-creation-ecies` IBGW flag ("allow messages from
   anyone"), `i2np` `TunnelGateway` nonzero destination tunnel
   id routing, `tunnel-implementation` per-hop receive/forward
   ids, plus criterion 4 with no sender/creator comparison in
   the counted dispatch + handler (runner
   `m11-i2pd-tunnel-gateway-no-creator-peer-affinity-source-lock`
   green).
6. **`TransitGatewayData` no longer inherits participant
   previous-peer locking** — PASS. Dedicated struct with
   `next_ibgw_fragment_id` only; `locked_previous_peer` and
   `duplicates` absent (static rules 32 + unit
   `m11_i2pr_ibgw_dedicated_state_has_no_peer_lock_unit` green).
7. **Build creator provenance retained for build
   admission/accounting/reply routing** — PASS.
   `TransitHopRegistration::previous_peer` unchanged, still set
   from the authenticated build sender, still gates
   `process_tunnel_data` (Participant/OBEP) and build admission;
   gateway ingress never mutates it (unit
   `m11_i2pr_ibgw_creator_peer_not_data_auth_unit` asserts
   preservation across ingresses with rewritten creators).
8. **IBGW gateway processing asserts exact receive-id equality**
   — PASS. `expected_receive` is the registry key; mismatch →
   `Ok(None)`; zero → typed fail-closed; non-IBGW/expired →
   `Ok(None)` (unit `m11_i2pr_ibgw_exact_receive_id_unit` +
   service `m11_i2pr_ibgw_exact_receive_id_unit` green).
9. **Valid third-party data sender can inject into a live IBGW
   registration** — PASS. Service
   `m11_i2pr_ibgw_third_party_authenticated_peer_accepted_unit`
   (creator A, sender B `PeerId`, correct id accepted with
   multicell, wrong id drops) + runtime-neutral
   `m11_i2pr_ibgw_third_party_authenticated_peer_accepted_unit`
   green; static rule 35 enforces the runner source lock.
10. **Participant previous-peer lock unchanged** — PASS.
    Tunnel + service units
    (`m11_i2pr_participant_peer_lock_unchanged_unit` ×2) prove
    wrong-peer `PreviousPeerMismatch`/`Drop` and correct-peer
    `Forward`; no `process_tunnel_data` top-gate change.
11. **Self-target OBEP TUNNEL enters local IBGW without synthetic
    peer state** — PASS. `deliver_obep_tunnel_to_self` calls the
    same `route_ibgw_gateway` seam with tunnel id + reconstructed
    standard bytes; static rule 34 + checker reject any
    `PeerId::from_hash/from_bytes` or `install_peer` on the self
    path; live-owner
    `m11_i2pr_self_tunnel_live_ibgw_ingresses_locally_unit` green;
    live `m11_i2pr_self_tunnel_does_not_mutate_peer_index_unit`
    proves no peer-index mutation.
12. **Unknown/zero/non-IBGW/expired/cancelled local target fails
    closed** — PASS. Live-owner rows for unknown, non-IBGW,
    cancelled (plus runtime-neutral exact-id unit for zero +
    expired) green; `LocalIbgwDropped` carries secret-free
    reason classes only.
13. **Non-self OBEP TUNNEL remote delivery unchanged** — PASS.
    Live-owner `m11_i2pr_non_self_tunnel_remote_behavior_unchanged_unit`
    proves `Tunnel(NoActiveSession)` with no peer (bit-identical
    remote seam); self ROUTER
    (`m11_i2pr_self_router_behavior_unchanged_unit`) retains
    existing `Router(_)` behavior (no TUNNEL-to-self widening).
14. **B SAM + B outbound `[i2pr]` establishment passes** — PASS
    (diag4). B log: `Session create: STYLE=DATAGRAM ID=m11-tx-b
    inbound.length=0 outbound.length=1 explicitPeers=<i2pr>`;
    `b-sender-obep-accepted = true`.
15. **B LeaseSet resolution/addressing names counted A tuple** —
    PASS (diag4, behavioral). B log `Requested LeaseSet … found`
    + `New remote LeaseSet added`, zero `Can't request LeaseSet`
    lines; `b-leaseset-resolved = true`; sends name counted A
    IBGW ids (receipt id `961179114` in the epoch accept set).
    Caveat (same class as Plan 261 criterion 6): no independent
    reference-side LeaseSet-content read; addressing is the proof.
16. **Counted self-target action produces genuine IBGW ingress**
    — PASS (diag4). `b-sender-outcome` ingress `6` (all
    `GatewayDelivered` via `LocalIbgwDelivered` mapping, not
    relabeled terminals); `gateway-ingress = true`.
17. **1400-byte counted payload emits ≥2 TunnelData cells with zero
    failures** — PASS (diag4, counted). `multicell-bounded =
    true` (at least one accepted ingress with `aux_count ≥ 2`);
    the counted receipt binds the multicell ingress (same
    `ibgw-receive-id` + `creator-local-tunnel-id` as the socket
    receipt tuple). Failure dimension is explicit per ingress
    (`gateway_failures: Some(..)`); the counted socket receipt
    proves zero-failure delivery for the bound ingress (any
    failed ingress could not have produced the exact socket
    payload).
18. **Committed next tuple matches A creator-local inbound tunnel**
    — PASS (diag4). `creator-router` names A, `creator-local-
    tunnel-id = 1043027150` is the registration's committed next
    tunnel on the counted delivery; `leaseset-tunnel-match =
    true`.
19. **Pool-owned LOCAL dispatch occurs** — PASS (diag4,
    behavioral). Exact payload at the receiver SAM socket through
    the one-hop `[i2pr]` topology is the binding proof (only the
    receiver pool's `ProcessGarlicMessage` can deliver there;
    source-locked `InboundTunnel::msg->from` + pool dispatch
    статически retained).
20. **Receiver SAM socket receives exact payload/digest exactly
    once** — PASS (diag4). `gateway-receipt = 1`,
    `gateway-receipt-once = true`, `rx_receipt_count = 1`
    (1400-byte `0xA5` digest-matched, once).
21. **Full six-field tuple is bound** — PASS (diag4).
    `full-tuple-bound = true`, `ibgw-receive-id`,
    `creator-router`, `creator-local-tunnel-id`,
    `pool-owner-destination`, `leaseset-gateway-match`,
    `leaseset-tunnel-match` all recorded.
22. **Cardinality/bandwidth/code-30 rows pass** — NOT EXECUTED
    live (full-matrix stop before the deterministic reject epoch
    on both complete attempts; see criteria 25–26). Proven
    locally (all Plan 257 cardinality/bandwidth/code-30 unit rows
    green) and retained from Plan 261 local evidence; not claimed
    live.
23. **Participant far-side, replay, expiry, cancellation,
    session-close, restart rows pass** — NOT EXECUTED live (same
    full-matrix stop). Proven locally (live-owner 41-row matrix
    green, including all Plan 257 lifecycle negatives) and
    retained; not claimed live.
24. **Fragment-id hardening stays green** — PASS. All five Plan
    260 regressions green locally; dedicated IBGW sequence
    wrap-skips-zero proven for the new type; multicell live
    (criterion 17).
25. **Complete external attempt 1 passes every mandatory row** —
    FAIL (mesh-sustainability stop, not a semantic row failure).
    Same-SHA `514bf12`, fresh datadirs/ports/evidence
    (`/tmp/m11-plan262-attempt1`, ports 44181/43983/43984/
    44983/44984): 745.9 s, stops in the IBGW data epoch
    (`m11-tx-ibgw` A-via-B relay, zero genuine gateway ingress
    over four multicell-forcing rounds). No production row
    failed; the relay path never reached the corrected seam.
26. **Complete external attempt 2 passes every mandatory row on the
    same SHA with fresh datadirs** — FAIL (same class). Same-SHA
    `514bf12`, fresh datadirs/ports/evidence
    (`/tmp/m11-plan262-attempt2`, ports 44281/43993/43994/
    44993/44994): 201.5 s, stops before any data epoch on a SAM
    read timeout. No production row failed.
27. **Full workspace verification passes** — PASS local
    (§"Verification"); exact-head CI GREEN (run `36604101824`
    on head `f6ecf4a`, four jobs success).
28. **Exact-head ordinary CI passes all four jobs** — PASS (run
    `36604101824` on head `f6ecf4a`: Quality ubuntu-latest,
    Quality macos-latest, MSRV Ubuntu, Dependency policy).
29. **README/registry/roadmap/support/conformance/dossier agree on
    Plan 262 as the sole dependency-ready M11 closure authority**
    — PASS (closure commit reconciles all five; Plan 263
    registered as the narrow sustainability successor, not a
    second closure authority; see §"Planning authority").
30. **No product default/capability/version/public-network change**
    — PASS (verified: transit stays disabled-by-default and
    non-advertised; no config/CLI/API/RI/version diff; `cargo
    deny` clean).
31. **No critical/high finding remains open** — RECORDED WITH
    OWNERS (see §"Findings": one MEDIUM sustainability finding
    owned by Plan 263; no open production defect).

Only criteria 22–23 (carried-forward lifecycle live rows) and
25–26 (two full-matrix passes) remain open, all on the same
mesh-sustainability boundary. Criteria 1–21, 24, 29–30 are
directly evidenced; 27–28 are local-green/CI-pending per
precedent. The ADR 0026 one-family M11 experimental
qualification is therefore NOT marked passed; M12 planning stays
deferred.

## Executions (all fail-closed, sanitized, unmerged)

Exact pin `635b013a…`, fresh datadirs per run, loopback-only.
Raw reference logs read for diagnosis only, never as evidence.
Implementation SHA `514bf12` for all counted runs below
(working tree clean at execution; pre-impl HEAD `ff4dc51`).

- **diag1** (`I2PR_M11_ONLY_EPOCH=receipt`, fresh mesh):
  560.2 s. B floodfill churn (`Can't find floodfill to publish`
  on B); `b-leaseset-resolved = false` with zero B-originated
  OBEP observations; four `m11-tx-b` send rounds started but no
  OBEP data reached i2pr. No B3 conclusions — the run never
  reached the send leg (B1-class establishment signature).
- **diag2** (`I2PR_M11_ONLY_EPOCH=receipt`, fresh mesh):
  151.2 s. Receiver SAM session never reported ready; no sends.
  B1 establishment flake (same class as Plan 261
  diag-bsender1).
- **diag3** (`I2PR_M11_ONLY_EPOCH=receipt`, fresh mesh):
  257.6 s, send-window 238.0 s. Receiver STATUS-OK with three A
  IBGW accepts; B `m11-tx-b` established, LeaseSet resolved,
  four 1400-byte sends. Ledger: `b-sender-outcome =
  terminal-garlic-self:0/ingress:48/socket:0` (B3 flipped:
  zero terminals, 48 local IBGW ingresses including background
  self-targets); `gateway-ingress = true`, `multicell-bounded =
  true`, `full-tuple-bound = true`, but `gateway-receipt = 0`.
  A log shows mass `Inbound tunnel … has been declined` + SAM
  `Read error: End of file` in the late window (mesh churn, not
  a routing row failure). No tuning; signature retained as the
  sustainability flake that diag4 supersedes on a healthy mesh.
- **diag4** (`I2PR_M11_ONLY_EPOCH=receipt`, fresh mesh):
  103.9 s, send-window 82.2 s. **Receipt closes.** B `m11-tx-b`
  established outbound `[i2pr]` (`b-sender-obep-accepted =
  true`); B LeaseSet resolved (`b-leaseset-resolved = true`,
  `found` + `New remote LeaseSet added`, zero lookup failures);
  sends name counted A IBGW ids; OBEP self-targets ingress
  locally (`terminal-garlic-self:0`); `ingress:6`,
  `gateway-ingress = true`, `multicell-bounded = true`
  (1400-byte stimulus emits 2 cells on the counted ingress);
  `next_router == A`, `next_tunnel` equals the committed
  creator-local id; `full-tuple-bound = true` with
  `leaseset-gateway-match = true`, `leaseset-tunnel-match =
  true`; receiver SAM socket receives the exact 1400-byte
  `0xA5` payload/digest exactly once (`gateway-receipt = 1`,
  `gateway-receipt-once = true`); `b-sender-outcome =
  terminal-garlic-self:0/ingress:6/socket:1`. Counted receipt
  id `961179114`, creator-local `1043027150`. This is the WP F
  flip.
- **attempt1** (complete matrix, fresh datadirs/ports/evidence,
  `I2PR_M11_ATTEMPT=1`, same SHA `514bf12`): 745.9 s. Stops in
  the IBGW data epoch (`m11-tx-ibgw` A-via-B relay): four
  multicell-forcing rounds, zero genuine gateway ingress. The
  relay path (A → B outbound, B → i2pr gateway) never reached
  the corrected seam; no Plan 262 row failed (the seam was never
  entered for this epoch). Retained as the sustainability stop
  (A↔B unmanaged relay + late-run declines, same class as Plan
  260 B1).
- **attempt2** (complete matrix, fresh datadirs/ports/evidence,
  `I2PR_M11_ATTEMPT=2`, same SHA `514bf12`): 201.5 s. Stops
  before any data epoch on `SAM read timeout` (A SAM bridge
  never answered). No production row failed.

57 focused unit rows (41 live-owner + 9 service/tunnel gateway
+ 5 runtime-neutral IBGW + 2 B-SAM gates, all green on the
execution head) plus the three exact-pinned source locks pass on
every run; the full-matrix stops are lane-topology/mesh
signatures, not unit regressions. No execution was retried to go
green; each ran once with a fresh datadir and its exact
signature recorded (diag3 socket-starved vs diag4 receipt-closed
are both retained; attempts are unmerged).

## Source-lock table (WP A — three new rows)

| Row | Pinned needle | File | Verdict |
|---|---|---|---|
| self-loopback | `Transports::PostMessages` + `GetRouterInfo ().GetIdentHash ()` + `m_LoopbackHandler.PutNextMessage` + `m_LoopbackHandler.Flush ()` | Transports.cpp | holds |
| tunnel-gateway-by-receive-id | `eI2NPTunnelGateway` + `tunnelID = bufbe32toh (msg->GetPayload ())` + `tunnel = GetTunnel (tunnelID);` + `HandleTunnelGatewayMsg (tunnel, msg);` + `tunnel->SendTunnelDataMsg (msg);` | Tunnel.cpp | holds |
| tunnel-gateway-no-creator-peer-affinity | absence of `GetIdentHash`/`previous_peer`/`creator`/`build-creator` in the counted `eI2NPTunnelGateway` dispatch + `HandleTunnelGatewayMsg` path | Tunnel.cpp | holds |

All verified `grep -qF` against the exact-pinned tree plus
runner-enforced (`record_guarded`) in every lane invocation.
Plan 260's 7 + 1 locks and Plan 261's 4 + 1 locks are retained
(checkers still enforce all of them; 171 guarded rows total).

## B3 flip analysis (not evidence — code-path reading)

The B-sender garlic's final TunnelData layer decrypts at i2pr's
OBEP registration into a TUNNEL-delivery action for `(self, X)`.
Before Plan 262, `deliver_obep_tunnel` resolved the target
through the session peer index (A/B only) and terminated with
the designed `NoActiveSession`. After Plan 262,
`deliver_obep_action` compares the decoded target with the
service's own `hop_identity` (the persistent bundle hash) and,
on equality, takes the already-decoded tunnel id + reconstructed
standard I2NP bytes into the same `route_ibgw_gateway` seam the
network `TunnelGateway` path uses. Unknown id / zero id /
non-IBGW / expired / cancelled / malformed-nested / empty-forward
still drop as `LocalIbgwDropped` with secret-free reason classes;
non-self targets use the bit-identical remote seam
(`deliver_obep_tunnel` + peer index + short-transport gateway
envelope). No wire change, no new task/channel/queue, no
synthetic peer, no peer-index insert, no artificial SSU2
serialization. Stock i2pd performs the analogous step in
`Transports::PostMessages` (self comparison → `LoopbackHandler`);
i2pr now owns the bounded equivalent at the OBEP/IBGW seam.

## Mesh-sustainability budget (WP H)

- diag1: 560.2 s, B floodfill churn, zero B-originated OBEP
  (B1 flake; establishment 3/7 across the Plan 262 fresh-mesh
  diagnostics including 260/261 history).
- diag2: 151.2 s, receiver never ready (B1 flake).
- diag3: 257.6 s total, 238.0 s send-window (4 B sends + drains +
  freshness gates). Mesh healthy enough for 4/4 sends + 48 local
  ingresses (B rebuilt nothing mid-run; no timeout inflated),
  but A inbound plane collapses late (mass declines + SAM read
  error) and socket stays 0. Retained as the starved-mesh
  counterpart to diag4's closed receipt on the same SHA.
- diag4: 103.9 s total, 82.2 s send-window (2 B sends + drains +
  freshness gates). Mesh healthy; receipt closes (criterion
  14–21). No timeout was inflated to fit.
- attempt1: 745.9 s total (full matrix through IBGW data epoch;
  4 `m11-tx-ibgw` A-via-B relay rounds + drains + freshness
  gates). The unmanaged A→B→i2pr relay never produces ingress;
  no Plan 262 row fails (seam never entered for this epoch).
- attempt2: 201.5 s total (full matrix through SAM setup).
  A SAM bridge never answers (`SAM read timeout`); no data epoch
  reached.
- Per-epoch wall times and A tunnel-test onset are carried by
  the ledger `logical_ms` column + reference logs (retained under
  `/tmp/m11-plan262-*`, unmerged; sanitized counts/hashes only in
  `driver-evidence.tsv`/`evidence.json`).
- The WP H receipt-first full matrix was executed twice on one
  SHA as required; both stops are environmental (relay/SAM/mesh),
  not semantic. No quota/timeout/retry tuning was applied. The
  sustainability successor (Plan 263) owns the bounded lane-
  harness corrective; production routing code is frozen.

## Verification (local truth; CI labeled)

On the implementation head `514bf12` (all green):

- `cargo fmt --all --check` — clean.
- `cargo check --locked --workspace --all-targets` — clean.
- `cargo test --locked --workspace --all-targets --
  --test-threads=1` — **3121 passed, 27 ignored**, 106
  suites, 0 failed (tunnel 396, daemon 1257 + 26 ignored
  incl. the 5 + 4 + 7 new Plan 262 rows + retained B-SAM gate;
  `m11_transit_live_owner` 41 rows).
- Focused: every new Plan 262 test exactly by name green
  (5 tunnel `m11_i2pr_*`, 4 service `m11_i2pr_*`, 7 live-owner
  `m11_i2pr_*`/`m11_*`).
- `cargo clippy --locked --workspace --all-targets
  --all-features -- -D warnings` — no issues.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace
  --no-deps` — clean; `cargo test --locked --workspace --doc`
  — 0 passed (16 suites), 0 failed.
- `cargo deny check advisories bans sources` — ok.
- All gate scripts green (both M11 checkers: 171 guarded
  rows + 57 epoch keys + 13 diagnostic keys; dependency,
  runtime, service-tunnel, fixture, NTCP2/SSU2/I2CP vectors,
  NTCP2-interop, constrained-host, SAM/SSU2/I2CP/service/
  exploratory/netdb/destination/streaming/M6 checkers) +
  NTCP2 harness 18 ok.
- `git diff --check` — clean.

Hosted exact-head CI (Ubuntu/macOS/MSRV/policy): GREEN on the
closure head — Actions run `36604101824` on head `f6ecf4a`
(Quality ubuntu-latest, Quality macos-latest, MSRV Ubuntu,
Dependency policy: all success). The run covers the complete
Plan 262 implementation plus the closure record, Plan 263
registration, and all planning/spec updates. No external
execution is required to close under retained-blocked (one
receipt-closed diagnostic plus two fail-closed full-matrix
attempts already bound the sustainability signature).

## Invariant / failure / migration / security reviews

- Invariants: exact pin, unmodified reference, loopback-only,
  fresh datadirs/ports/evidence per run, single-owner/single-
  decode, bounded state (no new counters/channels/tasks/queues;
  new evidence fields are hashes/ids/counts/outcomes), no
  version/capability/advertisement change, no public transit.
- Failure semantics: B-SAM absence fails before network
  startup; unproven LeaseSet resolution fails closed with the
  B-log tail in the error; zero-ingress fails closed with the
  terminal signature in `b-sender-outcome`; self-loop
  unknown/zero/non-IBGW/expired/cancelled/malformed/empty
  fails closed as `LocalIbgwDropped` with secret-free reasons;
  tuple validator unchanged (rejects partial bindings in
  endpoint-class order); `#[ignore]`-gated lane contract untouched
  (ordinary runs: 56 local rows execute, 1 external ignored).
- Migration/compat: no wire-format, config, CLI, API, RI, or
  version change (transit stays non-advertised and disabled by
  default; `TransitHopMaterial` still move-only; `LayerKeys`
  still zeroized on drop).
- Security: new rows carry public routing facts only
  (hashes, ids, counts, outcome labels, reason classes). No
  payload/key/digest/secret retention added. `cargo deny`
  clean. Transport authentication still protects network origin
  identity at the transport owner; IBGW data-plane authorization
  intentionally does not require that identity to equal the
  tunnel builder (frozen reference + normative role);
  Participant/OBEP hop provenance remains peer-locked; local
  loopback allowed only after decoded OBEP target == own
  RouterIdentity hash, with no synthetic peer.
- Concurrency: no new shared state (driver-local ledger only;
  lane processes already lifecycle-owned; per-registration
  fragment sequence, no global lock).

## Findings by severity

- MEDIUM (lane + qualification, owned by Plan 263):
  **Full-matrix mesh sustainability.** Two same-SHA complete
  attempts stop on environmental signatures (attempt1 IBGW-data
  A-via-B relay zero ingress over 4 rounds, 745.9 s; attempt2
  SAM read timeout, 201.5 s) after the semantic corrective is
  proven (diag4 receipt closes on the same SHA). Three
  receipt-only diagnostics bound the flake class (diag1 B
  floodfill churn, diag2 receiver never ready, diag3 socket-
  starved under A declines). Exact stop provenance: attempt +
  diag ledgers + driver-evidence + A/B logs (sanitized). The
  successor owns the bounded lane-harness sustainability
  corrective (freshness gates, session discipline, relay
  topology robustness) without touching production routing code,
  enlarging quotas, inflating timeouts, or retrying until green.
- LOW (residual, owned by Plan 263): ROUTER-kind and
  TUNNEL-forward-cell self targets keep current terminal
  behavior; widen only on proven need (Plan 262 §9 stop rule
  retained).
- LOW (informational): receiver establishment 3/7 across
  fresh-mesh diagnostics (B1 structural, unchanged); the
  receipt-first full matrix belongs to the successor's counted
  qualification (WP H not waived, not claimed).
- No critical findings. No high findings. No silent rows, no
  weakened gates, no tuning.

## Planning authority

- `plans/registry.md`: Plan 262 active → retained-blocked
  with the ownership-corrected/receipt-proven/matrix-
  sustainability boundary; Plan 263 registered ready with
  handoff + dependencies; `active_plan = 263`,
  `next_executable_plan = 263`; M11 row names 263 as current
  authority; M12 stays deferred.
- `plans/subsystems/transit-tunnels-roadmap.md`: §4 current
  state, §6 dependency graph, §7 rows (262 retained-blocked,
  263 ready), §9 Plan 262 outcome section, §12 summary.
- `specs/support.toml`: `plan_262_status` retained-blocked
  token, new `plan_263_*` entries, `m11_transit_tunnels`
  restated (B3 flipped live with socket receipt on a healthy
  mesh; full matrix stopped on sustainability), `next_executable_plan = 263`.
- `specs/CONFORMANCE.md`: M11 rows name 262's corrected proof
  (B-sender establishment + floodfill-store resolution +
  counted-id addressing + self-loop local ingress + multicell +
  tuple-bound socket receipt exhibited, full-matrix unclaimed)
  with 263 as the forward authority; no support-state advance.
- `specs/protocols/05-tunnels.md`: dossier records the flipped
  B3 with stop provenance and hands authority to 263.
- Historical records (`249`–`261` closures, registry archive)
  untouched.

## Roadmap disposition + unblock audit

- Plan 262 closes **retained-blocked**
  (`…-corrective-required-via-plan263`): IBGW receive-id
  ownership, dedicated IBGW state, source-neutral seam,
  self-delivery loopback arm, third-party + negative
  regressions, three source locks, and the B-sender receipt flip
  with socket receipt are durable production + evidence; full-
  matrix two-pass qualification is not claimed.
- Plan 263 (M11 qualification-sustainability corrective) is
  REGISTERED `ready` in the same commit (unblock audit: its only
  hard dependency is the Plan 262 ownership/receipt
  localization, now closed; no other deps). It is the sole
  `ready` M11 plan; `active_plan = next_executable_plan = 263`.
- `m11_transit_qualification` (receipt-capable experimental
  transit): remains unclaimed by explicit record (receipt proven
  once diagnostically, not twice as a complete matrix).
- M12 floodfill: planning stays deferred (requires
  receipt-capable transit with two complete passes). No M12 plan
  registered.
- M6 Java / Plan 201-247 debt, Plan 204 bookkeeping, M10
  authority: unaffected (registry audit — no other registered
  plan lists Plan 262 as a hard/interface dependency).
- Nothing else unblocks. No silent unblock.

## Docs / ops

- This record; Plan 263 plan-of-record; registry; roadmap;
  support ledger; conformance matrix; tunnel dossier
  (§"Planning authority").
- Operator impact: none (no daemon/config change in the
  closure commit; transit stays non-advertised and disabled
  by default).

## Handoff

To close: commit the implementation (`514bf1237e86fde21e17fc98c743eb52852edd99`),
then commit this record with Plan 263 + all planning/spec updates
(this commit), push, observe exact-head hosted CI (four jobs),
record the run ID in a follow-up commit (Plan 258/259/260/261
precedent). Plan 263 then owns the bounded qualification-
sustainability corrective. M11 rests at ownership-corrected and
receipt-proven (B3 flipped with socket receipt on a healthy mesh,
full matrix stopped on sustainability) until Plan 263 executes.
