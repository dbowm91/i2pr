# Plan 260 — M11 creator-owned inbound receipt topology and planning-authority corrective: status

**retained-m11-creator-owned-inbound-partially-proven-forward-sustainability-boundary-corrective-required-via-plan261**

## Disposition

Plan 260 executed as a corrective capability-qualification plan and
**stops at two exactly localized boundaries**. It does NOT close the
lane:

- The Plan 259 endpoint-class conflation is corrected and stays
  corrected: exact-pinned i2pd DOES have a distinct creator-owned
  `InboundTunnel` path, and Plan 260 source-locked all seven rows of
  it (WP A green, static + runner-enforced).
- The dedicated receiver topology is lane-buildable and was exhibited
  live: on a fresh mesh, A's receiver destination
  (`inbound.length = 1`, zero variance, `explicitPeers = i2pr`)
  produces 1-hop inbound builds that i2pr accepts as typed IBGW with
  `next_router == A` (four `[A,A]` accepts, two with exact +1
  cardinality), and A reports SAM SESSION STATUS OK
  (`m11-plan260-diag-receipt2`, 201 s).
- The constant fragmented IBGW message-id hardening (WP F) landed
  with all five regressions green locally and zero external
  regressions (legacy 33-ingress/0-failure multicell intact).
- Receipt does NOT close. Two boundaries stop it:
  - **B2 — forward-path garlic death at B's endpoint.** With the
    receiver established, four 1400-byte send rounds through the
    A-side sender's outbound `[B]` reassemble as type-11 garlics at
    B's endpoint (`Handle fragment of 966 bytes, msg type 11`) with
    **zero** `GatewayDelivered` on either counted `[A,A]` id
    (`0x349d15eb`, `0x0c0f1921`) and zero socket receipt. B-side
    debug shows endpoint reassembly with no onward delivery to
    i2pr — the forward-direction sibling of the Plan 259 Fork-2
    transit-endpoint mechanism.
  - **B1 — late-run mesh sustainability.** Receiver establishment is
    1/3 across executions (attempt-1 minute 7–9.5 and diag-receipt3
    never establish: 1 B-side IBGW build / 6 IBGW accepts against
    14 code-30 rejections under background churn). A's outbound
    plane collapses within ~2 minutes even when established
    (declined builds, expired pendings, failing tunnel tests —
    structural with no floodfill in the lane).

No row was weakened. No delivery-type deviation, no lane forcing, no
reference patching, no timeout inflation, no quota resizing, no new
dependency, no production wire change beyond the WP F field-value
hardening (format unchanged). No capability beyond the evidenced
subset is claimed. M11 remains unclaimed/non-advertised; M12 remains
deferred; the narrow successor is Plan 261 (B-sender receipt
requalification), registered ready in the same commit.

## Commits

- `48ee9ec13f4c685b27224027241b8da2baa4c793` — Plan 260
  implementation (see §"Implementation contents" below; production
  + test + lane + checker changes).
- this commit — this record + Plan 261 registration +
  registry/roadmap/support/conformance/dossier reconciliation
  (closure SHA recorded at commit time).
- Follow-up: hosted exact-head CI evidence record (Plan 258/259
  precedent; local floor fully green below).

Baseline: `a545f09c463c03bfbac20a31c7f5bfa77d351da5` (Plan 260
registration head).

Reference: unmodified i2pd 2.61.0 @
`635b013a612ff47278ef02acf8580a28e10e26c5` throughout.

## Implementation contents

Production code (bounded, two ownership boundaries):

- `crates/i2pr-tunnel/src/transit.rs` — `TransitParticipantData`
  gains per-registration `next_ibgw_fragment_id` (zero = unseeded);
  `claim_ibgw_fragment_id` seeds nonzero from the caller RNG on
  first use, returns the current id, advances with wrap-skips-zero;
  test-only `set_next_ibgw_fragment_id_for_test`. The IBGW
  canonical emission path claims one id per ingress (every fragment
  shares it) instead of the constant `1`. Four regressions:
  distinct-across-ingresses, same-id follow-ons, wrap-skips-zero,
  interleaved two-message no-cross-assembly.
- `crates/i2pr-tunnel/src/roles.rs` — `InboundGatewayRole` gains
  per-role `next_fragment_id: Cell<u32>` (role is already `!Sync`;
  no new sharing) with `claim_fragment_id`; both `process_cells`
  emission arms use it. One regression (distinct across ingresses,
  next-hop parse verified).
- `crates/i2pr-daemon/src/transit_owner.rs` —
  `LiveGatewayOutcome::Delivered` carries the committed
  `next_router`/`next_tunnel` (public routing facts already in
  evidence as receive-id rows); empty forward vectors fail closed
  as drops.

Test/lane/checker code (no production behavior beyond the above):

- `crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs` —
  six-field `ReceiptTuple` + `ReceiptTupleError`
  (`MissingCreatorLocalTunnel` / `PoolOwnerMismatch` /
  `LeaseSetMismatch` / `MalformedField`) + `validate_receipt_tuple`
  (endpoint-class order: router-hash → local-tunnel → pool →
  LeaseSet); test-only `i2p_b64_decode` (+ round-trip test);
  `Epoch::IbgwReceipt` with ten evidence keys; `gateway_observation`
  carries the committed next tuple; the receipt epoch (receiver
  1-hop `[i2pr]` + sender + 1400-byte ×4-round stimulus +
  six-field binding + exactly-once socket gate); the 2-hop receipt
  hard gate replaced by `gateway-receipt-superseded-note`
  (diagnostic history preserved, closing gate moved to the receipt
  epoch); `I2PR_M11_ONLY_EPOCH=receipt` non-counted diagnostic gate
  (bootstrap + receipt epoch only; cannot satisfy counted rows).
  An A-sender-via-`[i2pr]` diagnostic variant was implemented,
  statically refuted (`deliver_tunnel_data_forward` →
  `NoActiveSession` for self; no data-plane loopback exists), and
  reverted — recorded as analysis in §"Refuted variant", not as
  evidence.
- `tests/integration/m11-transit/run-i2pd.sh` — seven WP-A
  source-lock rows + WP-B explicit-peer source-lock row; four
  Plan 260 receipt `m11_row`s; manifest schema `v3` / `plan: 260`
  with the `ibgw_receipt` ten-field object.
- `scripts/check-m11-transit-qualification-evidence.sh` — 7 WP-A +
  1 WP-B + 4 receipt guarded labels; `Epoch::IbgwReceipt` key
  variant; fragment-id claim + 5-regression + 5-tuple-predicate
  enforcement; stale-259-gate rejection
  (`MissingCreatorLocalTunnel`, `Epoch::IbgwReceipt` required,
  2-hop receipt hard gate forbidden); manifest must name plan 260,
  must not name 257/259.
- `scripts/check-m11-transit-boundaries.sh` — manifest shape names
  plan 260.

## Plan 260 §18 acceptance criteria — requirement-to-evidence matrix

1. **Plan 258 fragmentation correction intact** — PASS. Legacy
   `ibgw-data` epoch in attempt-1: 33 ingress (30 single + 3
   multi), emitted-max 2, failures-total 0; `multicell-bounded:
   true`. New fragment ids on the wire, zero regressions.
2. **Plan 259 inventory traceable/unmodified** — PASS. Nine-chain
   table + transit-endpoint analysis preserved verbatim below the
   Plan 260 amendment header; raw counts never rewritten.
3. **Plan 259 global receipt-is-OBEP-only interpretation superseded**
   — PASS (planning authority). This record + registry/roadmap/
   support/conformance/dossier updates in the closure commit narrow
   it: the creator-owned path was not modeled and is now exhibited
   at build level.
4. **Exact-pinned source locks the creator-owned inbound
   construction path** — PASS (static). 7/7 needles verified
   `grep -qF` against `target/interop/ssu2-sources/i2pd-.../`:
   `CreatePeers (peers);` + `m_LastHop->SetNextIdent
   (i2p::context.GetIdentHash ());` (TunnelConfig.cpp);
   `isEndpoint = false;` + nonzero `nextTunnelID` (SetNextIdent);
   `GetTunnelID() == m_LastHop->nextTunnelID` + first-hop gateway
   tuple (TunnelConfig.h); `GetTunnel (tunnelID)` → resolved
   `HandleTunnelDataMsg` (Tunnel.cpp tunnel loop);
   `InboundTunnel::HandleTunnelDataMsg` + `msg->from =
   GetSharedFromThis ();`; pool-owned LOCAL garlic dispatch with
   router-context fallback (I2NPProtocol.cpp);
   `InboundTunnel` vs `TransitTunnelEndpoint` class distinction
   (Tunnel.h / TransitTunnel.h). Runner rows enforce all seven.
5. **Transit vs creator-owned endpoint distinction** — PASS
   (criterion 4, seventh lock + negative predicate rows
   §16.10–12 below).
6. **Pool-owned LOCAL garlic dispatch proven** — PASS (criterion 4,
   sixth lock; live pool delivery itself unproven — see B2).
7. **Dedicated A receiver owns an established inbound whose only
   remote peer is i2pr** — PARTIAL. Builds exhibited (`[A,A]`
   accepts ×4, diag-receipt2) and A STATUS-OK proven; "only remote
   peer" holds by explicit-peer construction (single-peer set) but
   no reference-side peer-list read exists — behavioral only.
8. **i2pr accepts the build as typed IBGW** — PASS (ledger:
   `[A,A]` IBGW accepts with exact +1 cardinality on
   `0x349d15eb`, `0x0c0f1921`; plus `[B,B]` router-pool analogues).
9. **Receive id matches A's published lease gateway tunnel id** —
   UNPROVEN (no receipt; LeaseSet-advertisement read never bound).
10. **`next_router` is A** — PASS on the build leg (ledger:
    counted accepts carry next == A `0522b9b9…`, diag-receipt2).
11. **`next_tunnel` matches A's creator-local
    `InboundTunnel::GetTunnelID()`** — UNPROVEN as an independent
    read (stock i2pd exposes no numeric read; the behavioral
    binding — socket receipt — never occurred). The i2pr-side
    committed tuple is exact; the A-side equality is not
    established.
12. **A inbound tunnel belongs to the receiver pool** — UNPROVEN
    (same behavioral gap as 11).
13. **Six-field tuple recorded** — PARTIAL. Validator + all rows
    recorded; `full-tuple-bound` never true (no receipt to bind
    pool/LeaseSet behaviorally).
14. **B sends a genuine destination-addressed payload through A's
    published LeaseSet** — UNPROVEN (B-sender never executed; see
    Plan 261 WP A). The executed send leg was A-side (see 15).
15. **Genuine TunnelGateway ingress on the counted IBGW
    registration** — FAIL. Zero `GatewayDelivered` on either
    counted `[A,A]` id over four 1400-byte rounds (diag-receipt2).
16. **Counted payload forces ≥2 TunnelData cells** — UNPROVEN live
    (no ingress); proven locally (1500 B → exactly 2 cells rows
    kept green) and on the legacy epoch (emitted-max 2).
17. **Zero i2pr forwarding failures on counted cells** — VACUOUS
    (no counted cells); legacy epoch failures-total 0 retained.
18. **A resolves the exact creator-local tunnel id** — UNPROVEN.
19. **LOCAL garlic dispatch on the pool-owned inbound path** —
    UNPROVEN live (source-locked statically only).
20. **Receiver SAM socket receives exact payload/digest exactly
    once** — FAIL (`ibgw-receipt/gateway-receipt = 0` all runs).
21. **Plan 257 cardinality + bandwidth rows** — PASS (early-run,
    attempt-1: exact role cardinality, typed bandwidth
    request/reply/disposition on all roles).
22. **Participant far-side row** — PASS (early-run, attempt-1
    legacy matrix).
23. **Replay row** — PASS (local `plan255_*`/`plan257_*` rows
    green; external replay epoch passed pre-receipt in attempt-1
    ordering — retained as infrastructure evidence, not as a
    same-run counted pass).
24. **Expiry row** — same status as 23.
25. **Cancellation all-dimension drain** — same status as 23
    (local `plan257_cancellation_requires_all_dimensions_zero`
    green).
26. **Session-close unrelated-peer retention** — same status as 23.
27. **Runtime restart + fresh accepted build** — same status as 23.
28. **Nonconstant bounded unique fragment ids** — PASS (local:
    5/5 regressions; external: mixed-id multicell with 0
    failures).
29. **Interleaved no-cross-assembly regression** — PASS (local
    `plan260_ibgw_interleaved_reassembly_no_cross_assembly`).
30. **Complete external attempt 1 passes every mandatory row** —
    FAIL (attempt-1 stops at receiver-not-ready, minute 7–9.5).
31. **Complete external attempt 2 passes every mandatory row** —
    FAIL (not launched as full-matrix: three executions already
    establish the systematic signatures — see §"Executions"; a
    fourth run adds no information per the Plan 257 three-run
    precedent).
32. **Same SHA + fresh datadirs** — N/A (no passing attempts).
33. **Full workspace verification passes** — PASS (local floor
    §"Verification", all green on the implementation head).
34. **Exact-head ordinary CI green on all four jobs** — PENDING at
    write time (follow-up commit per Plan 258/259 precedent; local
    floor is the current truth).
35. **Registry/roadmap/support/conformance/dossier agree** — PASS
    (closure commit reconciles all five; see §"Planning
    authority").
36. **No product default/capability/version/public-network change**
    — PASS (verified: no config/CLI/API/RI/version diff; transit
    stays disabled-by-default and non-advertised).
37. **No critical/high finding open** — RECORDED WITH OWNERS (see
    §"Findings": two HIGH lane findings owned by Plan 261; no
    open production defect).

## Executions (all fail-closed, sanitized, unmerged)

Exact pin `635b013a…`, fresh datadirs per run, loopback-only. Raw
reference logs read for diagnosis only, never as evidence.

- **attempt-1** (full matrix, pre-gate code): 541.6 s driver.
  Legacy build/data/participant/replay/expiry/cancel/close/restart
  epochs pass early (IBGW 33-ingress/0-failure multicell;
  OBEP rx512+rx4096 delivered). Receipt epoch: receiver session
  never STATUS-OK (single IBGW build in-epoch, B-side). Ledger
  receipt scope: 77 B-observed + 74 A-observed, 32 A-participant +
  10 A-OBEP (+5 cancelled) + 12 B-OBEP + 1 B-participant + 1
  B-IBGW accepts, zero rejections. A-log late window: `Inbound
  tunnel … declined` ×4, `Pending tunnel for message … not found`
  ×12+, `tunnel build messages dropped`, tunnel-test failures en
  masse, LeaseSet publish-confirmation failures (structural: no
  floodfill). Signature: B1 (late-run establishment failure).
- **diag-receipt2** (`I2PR_M11_ONLY_EPOCH=receipt`, fresh mesh):
  201.5 s. Receiver STATUS-OK. Ledger: 9 IBGW accepts (4×
  `[A,A]` on `0x349d15eb UNREDACTED_ID_1` — first two
  with exact +1 cardinality — plus 4× `[B,B]`, 1× `[B,A]`);
  7 `GatewayDelivered`, all on B `[B,B]` ids (`0x34fa926c` ×2,
  `0xbcad4274` ×5 — B's own tunnel tests); 4 `GatewayDropped`
  on B ids; 59 `DataForwarded` / 42 `DataDropped` background.
  Four 1400-byte send rounds: **zero** ingress on either counted
  `[A,A]` id; `gateway-receipt = 0`. B-log at send time:
  `TransitTunnel: handle msg for endpoint …` +
  `TunnelMessage: Handle fragment of 966 bytes, msg type 11` (×n)
  with no onward delivery lines to i2pr. Signature: B2
  (forward-path endpoint death).
- **diag-receipt3** (same subset, fresh mesh): 151.2 s. Receiver
  never STATUS-OK. Ledger: 126 observed, 6 IBGW accepts
  (self-shaped `[X,X]` for both routers) against **14 IBGW +
  6 participant + 4 OBEP code-30 rejections** under background
  churn. Signature: B1 (quota-saturated establishment failure).

23 focused unit rows (17 legacy + 6 new receipt-tuple/b64 rows)
and all local regression suites pass on every execution head;
the failures are lane-topology/health signatures, not unit
regressions. No execution was retried to go green; each ran once
with a fresh datadir and its exact signature recorded.

## Source-lock table (WP A + WP B)

| Row | Pinned needle | File | Verdict |
|---|---|---|---|
| inbound-last-hop-targets-creator | `CreatePeers (peers);` + `m_LastHop->SetNextIdent (i2p::context.GetIdentHash ());` | TunnelConfig.cpp | holds |
| inbound-last-hop-not-transit-endpoint | `isEndpoint = false;` + nonzero `nextTunnelID` in `SetNextIdent` | TunnelConfig.cpp | holds |
| inbound-local-tunnel-id | `GetTunnelID() == m_LastHop->nextTunnelID`; first-hop `GetNextTunnelID`/`GetNextIdentHash` | TunnelConfig.h | holds |
| tunnel-data-local-lookup | `GetTunnel (tunnelID)` → `HandleTunnelDataMsg` | Tunnel.cpp | holds |
| inbound-sets-message-owner | `InboundTunnel::HandleTunnelDataMsg` + `msg->from = GetSharedFromThis ();` | Tunnel.cpp | holds |
| local-garlic-pool-dispatch | `msg->from->GetTunnelPool ()->ProcessGarlicMessage (msg);` with `i2p::context.ProcessGarlicMessage` fallback | I2NPProtocol.cpp | holds |
| transit-endpoint-vs-inbound-owner-distinction | `class InboundTunnel: public Tunnel` vs `class TransitTunnelEndpoint: public TransitTunnel` | Tunnel.h / TransitTunnel.h | holds |
| explicit-peer-inbound-selection | `SelectExplicitPeers (Path&, bool)` + nonempty-preference + `I2CP_PARAM_EXPLICIT_PEERS[] = "explicitPeers"` + `I2CP_PARAM_INBOUND_TUNNEL_LENGTH[] = "inbound.length"` | TunnelPool.cpp / Destination.h | holds |

All verified `grep -qF` against the exact-pinned tree plus
runner-enforced (`record_guarded`) in every lane invocation.

## Refuted variant (analysis, not evidence)

A-sender-via-`[i2pr]` (sender outbound peer `b_b64` → `i2pr_b64`)
was implemented to route around B's endpoint, then statically
refuted before any counted use: `deliver_tunnel_data_forward`
requires a session peer mapping for `next_router`
(`NoActiveSession` otherwise); the peer index only holds A/B
session peers; no data-plane self-loopback exists (only the
Plan 257 OBEP build-reply self path). Adding self-delivery is a
production data-plane redesign — explicitly out of scope (Plan
260 §10/WP F stop rule). Reverted in the implementation commit.
The B-sender design (Plan 260 §8 as written, Plan 261 WP A/B)
avoids self-delivery by construction: the final hop into i2pr
always comes from another router.

## Historical Plan 259 A-ending chain (WP E)

Classified as **insufficient to classify**: `0xf6f56b0a` proves
only i2pr emission toward A with receipt 0. It binds no
creator-local `InboundTunnel` id, no pool owner, and no LeaseSet
tuple, so it is neither a creator-owned receiver path (cannot
pass) nor a proven non-creator path (cannot refute). Raw counts
unchanged. The new positive examples (`[A,A]`/`[B,B]` 1-hop
creator-owned builds, diag-receipt2/3) subsume its topology
question: such builds are real, accepted, and establish — delivery
through them is what now stops (B2), not their existence.

## Verification (local truth; CI labeled)

On the implementation head (all green):

- `cargo fmt --all --check` — clean.
- `cargo check --locked --workspace --all-targets` — clean.
- `cargo test --locked --workspace --all-targets --
  --test-threads=1` — **3104 passed, 27 ignored**, 106 suites,
  0 failed.
- Focused: `i2pr-tunnel --all-targets` 391 passed;
  `i2pr-daemon --all-targets` 1245 passed, 26 ignored
  (incl. 6/6 new `plan260_*` driver tests); 5/5 new
  `plan260_*` lib tests green by exact-name groups.
- `cargo clippy --locked --workspace --all-targets
  --all-features -- -D warnings` — no issues.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace
  --no-deps` — clean; `cargo test --locked --workspace --doc`
  — 0 passed (16 suites), 0 failed.
- `cargo deny check advisories bans sources` — ok.
- All 20 gate scripts green (9 infra + NTCP2 harness 18 ok +
  11 evidence/boundary checkers incl. both M11 checkers).
- `git diff --check` — clean.

Hosted exact-head CI (Ubuntu/macOS/MSRV/policy): PENDING at
write time; recorded in a follow-up commit per the Plan 258/259
precedent. No external execution is required to close under
retained/blocked (three fail-closed executions already bound
the B1/B2 signatures).

## Invariant / failure / migration / security reviews

- Invariants: exact pin, unmodified reference, loopback-only,
  fresh datadirs per run, single-owner/single-decode, bounded
  state (new id counters are `u32`/`Cell<u32>` on existing
  structs; new evidence fields are hashes/ids; no new channels,
  tasks, queues, or per-cell spawning), no
  version/capability/advertisement change, no public transit.
- Failure semantics: fragment-path errors stay typed
  (`TunnelMessage` → owner `Ok(None)` → `Dropped` with
  dimensions); empty forward vectors fail closed as drops (new
  explicit arm); tuple validator rejects partial bindings in
  endpoint-class order (never infers); missing B-SAM env (261
  scope) untouched.
- Migration/compat: no wire-format change (fragment id is a
  value-uniqueness fix inside the existing field); no config,
  CLI, API, RI, or version change.
- Security: fragment ids are routing nonces (RNG-seeded,
  per-registration/role, die with owner; never secret, never
  payload-derived). `LiveGatewayOutcome` additions and receipt
  rows carry public routing facts only (hashes, ids, counts).
  No payload/key/digest/secret retention added. `cargo deny`
  clean. `#[ignore]`-gated lane contract untouched (ordinary
  runs: 11 new unit rows execute, 1 external ignored).
- Concurrency: `Cell<u32>` confined to the already-`!Sync`
  role; registration counter is `&mut`-owned; counters die with
  their owner on cancel/drop/timeout/teardown (no cross-ingress
  or cross-registration sharing).

## Findings by severity

- HIGH (lane, owned by Plan 261): **B2 forward-path garlic death
  at B's endpoint.** A-side sender garlics for the A-local
  receiver reassemble at B's transit endpoint with no onward
  delivery to i2pr (zero ingress on all counted ids over four
  multicell-forcing rounds). Exact stop provenance: diag-receipt2
  ledger + driver-evidence + B debug log. Successor owns the
  B-sender topology that avoids B's endpoint by construction.
- HIGH (lane, owned by Plan 261): **B1 mesh sustainability.**
  Receiver establishment 1/3; A's outbound plane collapses
  within ~2 minutes (declined builds, expired pendings, failing
  tests; no floodfill). Successor owns the receipt-first matrix
  budget and the B-side LeaseSet-resolution lane work.
- MEDIUM → CLOSED (production, was Plan 258 open): constant
  fragment `message_id: 1` replaced by per-registration/per-role
  monotonic sequences (nonzero seed, wrap-skips-zero,
  ingress-shared, registration-scoped) with five regressions
  green locally and zero external regressions. Counted external
  proof of the hardening rides with Plan 261's matrix.
- LOW (residual, owned by Plan 261): dispatch-line divergence —
  reassembled type-11 completions at B's endpoint (966-byte
  forward fragments here; 52 historical Plan 259 completions)
  show no per-completion dispatch line, while co-timed 73-byte
  test garlics show `Handling message with type 11` +
  `Garlic: Type local`. Both constructed explanations remain
  receipt-incapable; the successor may isolate the drop site
  with a dedicated binary-attribution run under its own
  authority.
- LOW (informational): no data-plane self-loopback exists
  (`NoActiveSession` for self next-hops by design); A-sender
  topologies must always enter i2pr from another router. Not a
  defect; recorded so no successor re-proposes the `[i2pr]`-sender.
- No critical findings. No silent rows, no weakened gates, no
  tuning.

## Planning authority (WP H)

Registration-time reconciliation (commits `e7d5b84`, `8a017ae`)
is completed by the closure commit:

- `plans/registry.md`: Plan 260 ready → retained-blocked with
  the B1/B2 boundary; Plan 261 registered ready with handoff +
  dependencies; `active_plan = 261`, `next_executable_plan = 261`;
  M11 row names 261 as current authority; M12 stays deferred.
- `plans/subsystems/transit-tunnels-roadmap.md`: §4 current
  state, §6 dependency graph, §7 rows (260 retained-blocked,
  261 ready), §9 Plan 260 outcome section, §12 summary.
- `specs/support.toml`: `plan_260_status` retained-blocked
  token, new `plan_261_*` entries, `m11_transit_tunnels` restated
  (build half proven, receipt blocked on B1/B2),
  `next_executable_plan = 261`.
- `specs/CONFORMANCE.md`: M11 rows name 260's partial proof
  (creator-owned build exhibited, receipt unclaimed) with 261 as
  the forward authority; no support-state advance.
- `specs/protocols/05-tunnels.md`: dossier distinguishes the
  proven build half from the blocked delivery half, records B1/B2
  with stop provenance, and hands authority to 261.
- Historical records (`249`–`259` closures, registry archive)
  untouched.

## Roadmap disposition + unblock audit

- Plan 260 closes **retained-blocked** (`…-corrective-required-
  via-plan261`): source locks, msg-id hardening, tuple
  validator, receipt harness, and diagnostic gate are durable
  infrastructure; receipt qualification is not claimed.
- Plan 261 (M11 B-sender receipt requalification) is REGISTERED
  `ready` in the same commit (unblock audit: its only hard
  dependency is the Plan 260 boundary localization, now closed;
  no other deps). It is the sole `ready` M11 plan;
  `active_plan = next_executable_plan = 261`.
- `m11_transit_qualification` (receipt-capable experimental
  transit): remains unclaimed by explicit record.
- M12 floodfill: planning stays deferred (requires
  receipt-capable transit). No M12 plan registered.
- M6 Java / Plan 201-247 debt, Plan 204 bookkeeping, M10
  authority: unaffected (no shared dependency; verified by
  registry audit — no other registered plan lists Plan 260 as a
  hard/interface dependency).
- Nothing else unblocks. No silent unblock.

## Docs / ops

- This record; Plan 261 plan-of-record; registry; roadmap;
  support ledger; conformance matrix; tunnel dossier (§"Planning
  authority").
- Operator impact: none (no daemon/config change in the closure
  commit; transit stays non-advertised and disabled by default).

## Handoff

To close: commit the implementation (`<IMPL-SHA>`), then commit
this record with Plan 261 + all planning/spec updates
(`<CLOSURE-SHA>`), push, observe exact-head hosted CI (four
jobs), record the run ID in a follow-up commit (Plan 258/259
precedent). Plan 261 then owns the B-sender requalification.
M11 rests at partial proof (creator-owned build exhibited,
delivery blocked on B1/B2) until Plan 261 executes.
