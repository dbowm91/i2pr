# Plan 261 — M11 B-sender receipt requalification: status

**retained-m11-b-sender-topology-proven-self-delivery-boundary-corrective-required-via-plan262**

## Disposition

Plan 261 executed as a corrective capability-qualification plan
and **stops at one exactly localized boundary**. It does NOT close
the lane:

- The Plan 260 §8 B-sender experiment is now executed as written
  (WP A/B green): B's stock SAM bridge is plumbed, the `m11-tx-b`
  sender establishes outbound `[i2pr]`, and B-side LeaseSet
  resolution is source-supported AND behaviorally proven.
- The B-resolved lease names the counted tuple: all four sends
  address counted `[A,A]` IBGW receive ids (`0x2276888c` ×3,
  `0xda72a534` ×1).
- Receipt does NOT close. The single boundary that stops it:
  - **B3 — self-delivery loopback gap at i2pr's OBEP endpoint.**
    Each B-originated garlic arrives down B's 1-hop outbound
    tunnel and decrypts as a delivery-type-TUNNEL action with
    `target_router == self` (the lease gateway IS i2pr). The
    daemon path (`handle_inbound` Deliver arm →
    `deliver_obep_action` → `deliver_obep_tunnel`) looks the
    target up in the session peer index, which holds only the
    A/B remote peers, and terminates with the designed
    `NoActiveSession` outcome. Ledger: 4/4 send rounds →
    `DataDeliveredObep`/`obep-terminal-garlic` with
    `next_router == self`
    (`1d2172673ac93278c00220232559fd5c56255f16e3cf6ff4e1ec1883867c3d49`),
    **zero** `GatewayDelivered` on any counted id, zero socket
    receipt. Thirty-three background A-originated
    self-terminals carry the identical signature, so the
    condition is systematic, not send-specific. Stock i2pd has
    this arm (`Transports::PostMessages` injects self-addressed
    messages into the local `LoopbackHandler`); i2pr has no
    equivalent. This is the verbatim Plan 261 §10 stop #2.

No row was weakened. No delivery-type deviation, no lane
forcing, no reference patching, no timeout inflation, no quota
resizing, no new dependency, no production code change at all
(the Plan 261 implementation touches test driver + lane +
checkers + workflow only). No capability beyond the evidenced
subset is claimed. M11 remains unclaimed/non-advertised; M12
remains deferred; the narrow successor is Plan 262
(self-delivery loopback corrective), registered ready in the
same commit.

## Commits

- `7b1e6cf3b795bcd1cf7c02946d08175991892916` — Plan 261
  implementation (see §"Implementation contents" below;
  test/lane/checker/workflow changes only, zero production
  diff).
- this commit — this record + Plan 262 registration +
  registry/roadmap/support/conformance/dossier reconciliation
  (closure SHA recorded at commit time).
- Follow-up: hosted exact-head CI evidence record (Plan
  258/259/260 precedent; local floor fully green below).

Baseline: `79c05bc` (Plan 260 closure + CI-evidence head;
production tree identical to the Plan 260 implementation head).

Reference: unmodified i2pd 2.61.0 @
`635b013a612ff47278ef02acf8580a28e10e26c5` throughout.

## Implementation contents

Test/lane/checker/workflow code (no production behavior
change — `git diff` on `crates/*/src` is empty):

- `crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs` —
  `check_b_sam_port` fail-closed gate (`I2PD_B_SAM_PORT`,
  nonzero loopback port, before any socket/process/file
  mutation) + `plan261_b_sam_port_missing_fails_before_
  network_startup` local row; B's conf enables the stock SAM
  bridge; `b_sam_addr` session target; the receipt epoch keeps
  the proven A-side receiver construction unchanged and
  REPLACES the A-side sender leg with `m11-tx-b` on B's bridge
  (`inbound.length = 0`, `outbound.length = 1`, zero variance,
  `explicitPeers = <i2pr>`); the sender freshness gate watches
  B's `Outbound tunnel … has been created` lines; new counted
  keys `b-sender-obep-accepted` (typed OBEP accept replying to
  B), `b-leaseset-resolved` (behavioral: B-originated OBEP
  data in-epoch AND no `Can't request LeaseSet` on B),
  `b-sender-outcome` (sanitized terminal signature
  `terminal-garlic-self:<n>/ingress:<m>/socket:<k>`), and
  `send-window-ms` (WP C mesh-budget measurement). The deleted
  A-side `m11-tx-receipt` leg is gone (its B-endpoint death
  stays retained as the Plan 260 B2 boundary, not retried).
- `tests/integration/m11-transit/run-i2pd.sh` — four WP-A
  source-lock rows + `m11-i2pd-b-sam-missing-env-fails` exact
  row + three B-sender `m11_row`s; `I2PD_B_SAM_PORT` lane env
  (default `44984`) passed to the driver; manifest schema `v3`
  / `plan: 261` with the same ten-field `ibgw_receipt` object.
- `scripts/check-m11-transit-qualification-evidence.sh` — 4 new
  guarded source-lock rows + gate row + 3 B-sender rows (168
  guarded total), 4 new epoch keys (57 total), manifest must
  name plan 261 and must NOT name 257/259/260, driver must
  require `I2PD_B_SAM_PORT` + own `m11-tx-b` + record the three
  B-sender keys + carry the missing-env gate test, driver must
  NOT retain `m11-tx-receipt`.
- `scripts/check-m11-transit-boundaries.sh` — rule 23 requires
  `I2PD_B_SAM_PORT`; rule 29 names `plan: 261`; new rule 31
  requires the four B-side source-lock rows + B SAM port and
  rejects stale `plan: 260`.
- `.github/workflows/m11-transit-external.yml` —
  `I2PR_I2PD_B_SAM_PORT` per attempt (`44984`/`44994`).

## Plan 261 §13 acceptance criteria — requirement-to-evidence matrix

1. **Plan 260 WP A/B/F implementation and local rows intact**
   — PASS. Full floor green (§"Verification"); both M11
   checkers green; fragment-id regressions green.
2. **Plan 260 B1/B2 boundaries traceable as the starting
   point** — PASS. diag-bsender1 reproduces B1 (receiver never
   ready, 151 s, declined builds + mass test failures); B2 is
   untouched by code (A-side leg deleted, retained as
   boundary).
3. **B SAM bridge via stock config; missing env fails closed**
   — PASS. B conf uses the same `[sam]` section mechanism as
   A; `plan261_b_sam_port_missing_fails_before_network_
   startup` proves absent/empty/garbage/zero fail with no
   socket, process, or file mutation (same precedent as the
   Plan 256 pin gate).
4. **B-side LeaseSet resolution source-supported and proven
   by a lookup row before any payload send** — PASS. Source
   locks: floodfill self-listing
   (`m_Floodfills.Insert(GetSharedRouterInfo())`), local-store
   lookup reply (`FindLeaseSet` → `CreateDatabaseStoreMsg`),
   client request path (`RequestLeaseSet` →
   `GetClosestFloodfill` → `SendLeaseSetRequest`). Live proof:
   B log shows `Store request: LeaseSet2` + `LeaseSet2
   updated` for the A-published set, then `Requested LeaseSet
   <b64> found` + `New remote LeaseSet added` at send time,
   with zero `Can't request LeaseSet`/`was not sent`/`no
   compatible` lines; `b-leaseset-resolved = true`.
5. **B sender establishes outbound `[i2pr]`** — PASS. B log:
   `Session create: STYLE=DATAGRAM ID=m11-tx-b …
   inbound.length=0 outbound.length=1 … explicitPeers=<i2pr>`;
   three B-originated OBEP accepts (`0x783c739c`,
   `0x37d392c5`, `0x366ce62e` with exact +1 cardinality);
   `b-sender-obep-accepted = true`.
6. **B-resolved LeaseSet names the counted `(i2pr, recv)`
   tuple** — PASS (behavioral). All four B sends address
   counted ids (`0x2276888c` ×3, `0xda72a534` ×1, both in the
   epoch's exact-+1 `[A,A]` accept set). Caveat (same class as
   Plan 260 criterion 7): no independent reference-side
   LeaseSet-content read exists; the addressing is the proof.
7. **Genuine TunnelGateway ingress on the counted `[A,A]`
   IBGW id** — FAIL (B3). Zero `GatewayDelivered` on any
   counted id over four multicell-forcing rounds;
   `b-sender-outcome = terminal-garlic-self:4/ingress:0/
   socket:0`.
8. **Counted stimulus forces ≥2 TunnelData cells** — UNPROVEN
   live (no ingress); proven locally and on the legacy epoch
   (retained).
9. **`next_tunnel` matches the committed creator-local tuple**
   — UNPROVEN as a bound delivery (no delivery); the i2pr-side
   committed tuples exist but no ingress names one.
10. **Pool-owned LOCAL dispatch on the receiver path** —
    UNPROVEN live (source-locked statically only, retained).
11. **Receiver SAM socket exact receipt exactly once** — FAIL
    (`gateway-receipt = 0`, `b-sender-outcome` socket `0`).
12. **Full six-field tuple bound** — UNPROVEN (`full-tuple-
    bound` never recorded; validator + partial rows retained).
13. **Plan 257 cardinality + bandwidth rows in the reordered
    matrix** — NOT EXECUTED (WP D unreached: the stop fired in
    the diagnostic subset before any counted matrix; the
    receipt-first reorder + full matrix belong to Plan 262's
    qualification — recorded, not waived).
14. **Participant/replay/expiry/cancel/close/restart rows** —
    NOT EXECUTED (same as 13).
15. **Fragment-id hardening rows green** — PASS (floor).
16. **Complete external attempt 1 passes** — FAIL (not
    launched as full-matrix: the diagnostic subset localized
    the structural stop; a full matrix adds no information —
    same precedent as Plan 260 criterion 31).
17. **Complete external attempt 2 passes** — FAIL (same as 16;
    no passing attempts; diagnostics unmerged under
    `target/`).
18. **Full workspace verification passes** — PASS local
    (§"Verification"); **exact-head CI PENDING** at write time
    (follow-up commit per precedent).
19. **Registry/roadmap/support/conformance/dossier agree** —
    PASS (closure commit reconciles all five; see §"Planning
    authority").
20. **No product default/capability/version/public-network
    change** — PASS (verified: `crates/*/src` diff is empty;
    transit stays disabled-by-default and non-advertised).
21. **No critical/high finding open** — RECORDED WITH OWNERS
    (see §"Findings": one HIGH lane/production finding owned
    by Plan 262; no other open defect).

## Executions (all fail-closed, sanitized, unmerged)

Exact pin `635b013a…`, fresh datadirs per run, loopback-only.
Raw reference logs read for diagnosis only, never as evidence.

- **diag-bsender1** (`I2PR_M11_ONLY_EPOCH=receipt`, fresh
  mesh): 151.4 s. Receiver never STATUS-OK (single-epoch
  bootstrap green: sessions 2/2, both NetDB loads observed).
  A-log late window: mass `Test of tunnel … failed` +
  `Inbound/Outbound tunnel … declined` (B1 establishment
  signature, identical to Plan 260 diag-receipt3). No B-sender
  conclusions drawn — the run never reached the send leg.
- **diag-bsender2** (`I2PR_M11_ONLY_EPOCH=receipt`, fresh
  mesh): 508.2 s, send-window 488.7 s. Receiver STATUS-OK
  with four counted `[A,A]` IBGW registrations
  (`0x2276888c`, `0x22d99981`, `0xb25a97f0`, `0xda72a534`,
  each exact +1). B `m11-tx-b` established (three B OBEP
  accepts; nine B `Outbound tunnel … has been created`
  lines across the run — mesh churn, no tuning). Four
  `DATAGRAM SEND ID=m11-tx-b … SIZE=1400` to the one receiver
  destination. B log at send time: floodfill store of the
  A-published LeaseSet2, `Requested LeaseSet … found`, `New
  remote LeaseSet added`, zero lookup-failure lines.
  Ledger: four B-originated (`d4677289…`)
  `DataDeliveredObep`/`obep-terminal-garlic` with
  `next_router == self`
  (`1d2172673ac93278c00220232559fd5c56255f16e3cf6ff4e1ec1883867c3d49`)
  targeting `0x2276888c` ×3 + `0xda72a534` ×1;
  `GatewayDelivered` 0 on every counted id; socket receipt 0.
  Lane aborts closed with `Plan 261 receipt epoch observed no
  genuine gateway ingress on the B-sender topology`.
  Signature: B3 (self-delivery loopback gap).

57 focused unit rows (56 retained + 1 new B-SAM gate) pass on
the execution head; the failure is a lane-topology/code-path
signature, not a unit regression. No execution was retried to
go green; each ran once with a fresh datadir and its exact
signature recorded.

## Source-lock table (WP A — all four new rows)

| Row | Pinned needle | File | Verdict |
|---|---|---|---|
| b-sam-bridge-enabled | `("sam.enabled", value<bool>()->default_value(true)` | Config.cpp | holds |
| explicit-peer-outbound-selection | `void TunnelPool::CreateOutboundTunnel (uint64_t ts)` + `if (SelectPeers (path, false))` + `if (!m_ExplicitPeers.empty ()) return SelectExplicitPeers (path, isInbound);` | TunnelPool.cpp | holds |
| outbound-endpoint-tunnel-forward | `case eDeliveryTypeTunnel:` + `SendMessageTo (msg.hash, i2p::CreateTunnelGatewayMsg (msg.tunnelID, msg.data));` | TunnelEndpoint.cpp | holds |
| b-leaseset-resolution | `m_Floodfills.Insert (i2p::context.GetSharedRouterInfo ());` + `auto leaseSet = FindLeaseSet (ident);` + `replyMsg = CreateDatabaseStoreMsg (ident, leaseSet);` | NetDb.cpp | holds |
| b-leaseset-resolution (request path) | `auto floodfill = i2p::data::netdb.GetClosestFloodfill (dest, excluded);` + `if (!SendLeaseSetRequest (dest, floodfill, request))` | Destination.cpp | holds |

All verified `grep -qF` against the exact-pinned tree plus
runner-enforced (`record_guarded`) in every lane invocation.
Plan 260's 7 + 1 locks are retained (checkers still enforce
all of them).

## B3 analysis (not evidence — code-path reading)

The B-sender garlic's final TunnelData layer decrypts at
i2pr's OBEP registration into a TUNNEL-delivery action for
`(self, X)`. `deliver_obep_tunnel` resolves the target through
the session peer index (`peer_index.get`), which maps only
authenticated remote peers (A/B); the local hash has no entry
by construction, so the designed `NoActiveSession` terminal
fires. There is no self-comparison anywhere on the OBEP
action path (`transit_owner.rs` `deliver_obep_action` →
`transit_compose.rs` `deliver_obep_tunnel`). Stock i2pd
performs the missing step in `Transports::PostMessages`
(self comparison → `m_LoopbackHandler`), which is why the
reference side of every symmetric topology delivers. The
corrective (Plan 262 WP A) reroutes the already-decoded
self-targeted TUNNEL action into the existing local
`route_tunnel_gateway` IBGW ingress; unknown id / wrong peer /
non-IBGW / expired still drop on the existing `Ok(None)`
arms. No wire change, no new task/channel/queue, non-self
targets bit-identical.

## Mesh-sustainability budget (WP C)

- diag-bsender1: 151.4 s, establishment failure (B1 flake;
  receiver-establishment rate now 2/5 across the Plan 260 +
  261 fresh-mesh diagnostics).
- diag-bsender2: 508.2 s total, 488.7 s send-window (4
  rounds + drains + freshness gates). The mesh stayed healthy
  enough for 4/4 sends (B rebuilt outbound 9× under churn;
  no timeout was inflated to fit).
- Per-epoch wall times and A's tunnel-test onset are carried
  by the ledger `logical_ms` column + reference logs (both
  retained under `target/`, unmerged).
- The WP C.2 receipt-first reorder was NOT applied: no
  counted matrix executed (stop fired in the diagnostic
  subset), so there was no matrix to reorder. The reorder +
  full matrix belong to Plan 262 WP B/C.

## Verification (local truth; CI labeled)

On the implementation head (all green):

- `cargo fmt --all --check` — clean.
- `cargo check --locked --workspace --all-targets` — clean.
- `cargo test --locked --workspace --all-targets --
  --test-threads=1` — **3105 passed, 27 ignored**, 106
  suites, 0 failed.
- Focused: `i2pr-tunnel --all-targets` 391 passed;
  `i2pr-daemon --all-targets` 1246 passed, 26 ignored
  (incl. the new `plan261_*` gate test);
  `m11_transit_i2pd_external` local rows 56 passed.
- `cargo clippy --locked --workspace --all-targets
  --all-features -- -D warnings` — no issues.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace
  --no-deps` — clean; `cargo test --locked --workspace --doc`
  — 0 passed (16 suites), 0 failed.
- `cargo deny check advisories bans sources` — ok.
- All 20 gate scripts green (both M11 checkers: 168 guarded
  rows + 57 epoch keys) + NTCP2 harness 18 ok.
- `git diff --check` — clean.
- `crates/*/src` diff is EMPTY (criterion 20 evidence).

Hosted exact-head CI (Ubuntu/macOS/MSRV/policy): PENDING at
write time (follow-up commit per Plan 258/259/260 precedent;
local floor is the current truth).

## Invariant / failure / migration / security reviews

- Invariants: exact pin, unmodified reference, loopback-only,
  fresh datadirs per run, single-owner/single-decode, bounded
  state (no new counters/channels/tasks/queues; new evidence
  fields are hashes/ids/counts), no
  version/capability/advertisement change, no public transit.
- Failure semantics: B-SAM absence fails before network
  startup; unproven LeaseSet resolution fails closed with the
  B-log tail in the error; zero-ingress fails closed with the
  terminal signature in `b-sender-outcome`; tuple validator
  unchanged (rejects partial bindings in endpoint-class
  order); `#[ignore]`-gated lane contract untouched
  (ordinary runs: 57 local rows execute, 1 external ignored).
- Migration/compat: no wire-format, config, CLI, API, RI, or
  version change (no production diff at all).
- Security: new rows carry public routing facts only
  (hashes, ids, counts, outcome labels). No
  payload/key/digest/secret retention added. `cargo deny`
  clean.
- Concurrency: no new shared state (driver-local ledger
  only; lane processes already lifecycle-owned).

## Findings by severity

- HIGH (lane + production, owned by Plan 262): **B3
  self-delivery loopback gap.** Self-targeted OBEP TUNNEL
  actions terminate `NoActiveSession` instead of ingressing
  locally (4/4 counted rounds + 33 background self-terminals,
  zero ingress, zero socket receipt). Exact stop provenance:
  diag-bsender2 ledger + driver-evidence + B debug log.
  Successor owns the bounded self-delivery arm that reroutes
  the decoded action into the existing local IBGW ingress.
- LOW (residual, owned by Plan 262): ROUTER-kind and
  TUNNEL-forward-cell self targets keep current terminal
  behavior; the successor widens only on proven need
  (Plan 262 §9 stop rule).
- LOW (informational): receiver establishment 2/5 across
  fresh-mesh diagnostics (B1 structural, unchanged); the
  receipt-first reorder is deferred to the successor's
  counted matrix (WP C.2 not applied, not waived).
- No critical findings. No silent rows, no weakened gates, no
  tuning.

## Planning authority

- `plans/registry.md`: Plan 261 active → retained-blocked
  with the B3 boundary; Plan 262 registered ready with
  handoff + dependencies; `active_plan = 262`,
  `next_executable_plan = 262`; M11 row names 262 as current
  authority; M12 stays deferred.
- `plans/subsystems/transit-tunnels-roadmap.md`: §4 current
  state, §6 dependency graph, §7 rows (261 retained-blocked,
  262 ready), §9 Plan 261 outcome section, §12 summary.
- `specs/support.toml`: `plan_261_status` retained-blocked
  token, new `plan_262_*` entries, `m11_transit_tunnels`
  restated (B-sender topology proven through addressing,
  delivery stopped on B3), `next_executable_plan = 262`.
- `specs/CONFORMANCE.md`: M11 rows name 261's partial proof
  (B-sender establishment + floodfill-store resolution +
  counted-id addressing exhibited, receipt unclaimed) with
  262 as the forward authority; no support-state advance.
- `specs/protocols/05-tunnels.md`: dossier records B3 with
  stop provenance and hands authority to 262.
- Historical records (`249`–`260` closures, registry archive)
  untouched.

## Roadmap disposition + unblock audit

- Plan 261 closes **retained-blocked** (`…-corrective-required-
  via-plan262`): B SAM plumbing, B-sender session, LeaseSet
  resolution proof, terminal-signature instrumentation, and
  the four source locks are durable infrastructure; receipt
  qualification is not claimed.
- Plan 262 (M11 self-delivery loopback corrective) is
  REGISTERED `ready` in the same commit (unblock audit: its
  only hard dependency is the Plan 261 B3 localization, now
  closed; no other deps). It is the sole `ready` M11 plan;
  `active_plan = next_executable_plan = 262`.
- `m11_transit_qualification` (receipt-capable experimental
  transit): remains unclaimed by explicit record.
- M12 floodfill: planning stays deferred (requires
  receipt-capable transit). No M12 plan registered.
- M6 Java / Plan 201-247 debt, Plan 204 bookkeeping, M10
  authority: unaffected (registry audit — no other registered
  plan lists Plan 261 as a hard/interface dependency).
- Nothing else unblocks. No silent unblock.

## Docs / ops

- This record; Plan 262 plan-of-record; registry; roadmap;
  support ledger; conformance matrix; tunnel dossier
  (§"Planning authority").
- Operator impact: none (no daemon/config change in the
  closure commit; transit stays non-advertised and disabled
  by default).

## Handoff

To close: commit the implementation (`<IMPL-SHA>`), then
commit this record with Plan 262 + all planning/spec updates
(`<CLOSURE-SHA>`), push, observe exact-head hosted CI (four
jobs), record the run ID in a follow-up commit (Plan
258/259/260 precedent). Plan 262 then owns the self-delivery
loopback corrective. M11 rests at addressed-but-undelivered
(B-sender topology proven through counted-id addressing,
delivery blocked on B3) until Plan 262 executes.
