# Plan 263 — M11 qualification-sustainability corrective: status

**retained-m11-sustainability-harness-landed-receipt-and-multicell-reproven-single-mesh-full-matrix-boundary-corrective-required-via-plan264**

## Disposition

Plan 263 executed as a narrow harness-only sustainability
corrective plus final qualification pass and **stops at one
exactly localized single-mesh full-matrix sustainability
boundary**. It does NOT close the lane:

- The harness hardening landed with zero production diff:
  explicit SSU2 mesh-liveness proofs after the best-effort
  redial, relay NetDB + B-floodfill prerequisite proofs before
  counted sends, and the canonical fatal SAM read-timeout tail
  (4 new `plan263_*` harness rows, runner manifest `plan: 263`,
  both M11 checkers extended with sustainability + frozen-
  constant invariants). No timeout/quota/ceiling/retry/
  message-size constant changed from Plan 262.
- Receipt re-proven on the implementation SHA: `b-sender-
  outcome = terminal-garlic-self:0/ingress:18/socket:1` with
  `gateway-receipt = 1`, `gateway-receipt-once = true`,
  `multicell-bounded = true`, `full-tuple-bound = true`
  (diag-receipt2, receipt-only, driver PASS in 257.6 s, fresh
  datadirs, loopback-only, same SHA `9bd2f39a`).
- IBGW-data multicell re-proven on the same SHA in a full-
  matrix run (attempt2: 17 genuine ingresses, `multicell-
  bounded = true`, `multicell-max = 2`).
- Two complete same-SHA full-matrix attempts both stop on
  mesh-sustainability signatures with no production row ever
  failing and no B3 regression (`terminal-garlic-self:0` in
  every run): attempt1 stops in the IBGW-data epoch with 14
  genuine ingresses all single-cell (multicell gate); attempt2
  passes IBGW-data multicell then stops in the receipt epoch
  with `0/0/0` starved (B OBEP accepted, LeaseSet resolved,
  zero ingress on the counted registration over 1047 s).
  A third receipt-only diagnostic bounds the starved class
  (diag-receipt1 `0/0/0`, 338.9 s). No row weakened, no
  tuning, no retry-until-green, no reference patching, no
  public fallback, no new dependency, no RouterInfo/version/
  capability change. M11 remains unclaimed/non-advertised;
  M12 remains deferred; the narrow successor is Plan 264
  (single-mesh sustainability scoping), registered ready in
  the same commit.

## Commits

- `9bd2f39aa2c8509273177eb509157d60a09cafb2` — Plan 263
  implementation (see §"Implementation contents" below;
  harness-only: 4 unit rows, 3 prerequisite helpers + 2
  call sites, `SAM_READ_TIMEOUT_MSG` const, runner manifest
  `plan: 263`, both M11 checkers extended, zero
  `crates/*/src` diff).
- this commit — this record + Plan 264 registration +
  registry/roadmap/support/conformance/dossier reconciliation
  (closure SHA recorded at commit time).
- Follow-up: hosted exact-head CI evidence record (Plan
  258/259/260/261/262 precedent; local floor fully green
  below).

Baseline: `4cee1a634b3c8602f08b2cd7ebcb9569d996d78e` (Plan 262
closure + Plan 263 registration head; planning/spec-only
reconciliation through that head does not change the
production source baseline inherited from Plan 262, and Plan
263 itself adds zero production diff).

Reference: unmodified i2pd 2.61.0 @
`635b013a612ff47278ef02acf8580a28e10e26c5` throughout.

## Implementation contents

Harness-only corrective (no wire format, task/channel/queue,
quota, timeout, config/CLI/API/RI/version change; verified
`git diff --name-only | grep -E '^crates/.*/src/'` empty):

- `crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs` —
  `SAM_READ_TIMEOUT_MSG` canonical tail const (used by
  `roundtrip`; interleave `try_read_line` still returns
  `Ok(None)`, handshake maps absence to the fatal tail);
  `mesh_liveness_error` + `mesh_liveness_status` (explicit
  SSU2 delivery-capability proof for A+B after the
  best-effort `ensure_sessions` redial, fail-closed with the
  exact missing-peer signature); `verify_relay_netdb_
  prerequisites` (B-RI-in-A + i2pr-RI-in-A/B existence);
  `verify_b_floodfill_conf` (lane-written B conf still
  carries `floodfill = true`); retained NetDB paths
  (`i2pr_ri_in_a_netdb`, `i2pr_ri_in_b_netdb`,
  `b_ri_in_a_netdb`); both counted send legs (IBGW-data
  A-via-B relay, receipt B-sender) prove liveness +
  prerequisites before payload; four new unit rows
  (`plan263_sam_read_timeout_tail_is_canonical_and_fatal`,
  `plan263_mesh_liveness_error_names_missing_links`,
  `plan263_relay_netdb_prerequisites_require_all_placements`,
  `plan263_b_floodfill_conf_requires_floodfill_role`).
- `tests/integration/m11-transit/run-i2pd.sh` — manifest
  `plan: 263` (rejects 257/259/260/261/262), evidence.md
  title + pass/fail lines name Plan 263, scratch template
  `plan263`, added `known_limitations` row for the
  harness-only corrective; all source locks, gates, and
  epoch rows unchanged.
- `scripts/check-m11-transit-boundaries.sh` — rule 29
  requires `plan: 263` (rejects 262); new rule 36 requires
  the 8 Plan 263 driver symbols + 5 frozen Plan 262 lane
  numerics (`DIAL_TIMEOUT` 20 s, `SAM_IO_TIMEOUT` 15 s,
  `SETUP_HEARTBEAT_SECS` 30, `MAX_SAM_DATAGRAM_RX_BYTES`
  8192, `MAX_LEDGER_OBSERVATIONS` 4096).
- `scripts/check-m11-transit-qualification-evidence.sh` —
  manifest gate requires `plan: 263` (rejects 262 and
  older); new Plan 263 section requires the 5 symbols, 4
  unit rows, 3 call-site gates, and 5 frozen numerics.
- `.github/workflows/m11-transit-external.yml` — unchanged
  two-attempt matrix (attempts 1–2, fail-fast false, fresh
  ports/evidence per attempt); closure uses it for the two
  same-SHA complete attempts below.

## Plan 263 §10 acceptance criteria — requirement-to-evidence matrix

1. **Plan 262 retained evidence traceable** — PASS. Plan 262
   WP A/B/C/D/E (dedicated `TransitGatewayData`, exact
   receive-id, source-neutral seam, self-loop branch, 3
   source locks, 5+4+7 regressions, driver mapping) remain
   in tree and green (§"Verification"); production source
   is byte-identical to Plan 262 (zero diff proof above).
2. **Plan 262 B3 flip remains the starting point** — PASS.
   `terminal-garlic-self:0` in ALL FOUR counted runs on
   the new SHA (no `NoActiveSession` self-terminal anywhere;
   the ownership corrective never regresses).
3. **Three source locks hold** — PASS (runner-enforced on
   every run; checkers enforce statically).
4. **No production `crates/*/src` diff** — PASS (verified
   above; both M11 checkers Lock the Plan 262 production
   shapes unchanged).
5. **Receipt closes on the implementation SHA** — PASS
   (diag-receipt2: `0/18/1` with `gateway-receipt = 1`,
   `gateway-receipt-once = true`, `multicell-bounded =
   true`, `full-tuple-bound = true`,
   `leaseset-gateway-match = true`, `leaseset-tunnel-match
   = true`; receipt id `74527871`, creator-local
   `2957044763`; driver PASS 257.6 s; B `m11-tx-b`
   established, LeaseSet resolved, 4 send rounds).
6. **Complete external attempt 1 passes every mandatory
   row** — FAIL (sustainability stop, not a semantic row
   failure). Same-SHA `9bd2f39a`, fresh datadirs/ports/
   evidence (`/tmp/m11-plan263-attempt1`, attempt 1 ports):
   400.3 s driver. Progresses through ALL build epochs +
   OBEP-data, stops in IBGW-data: 14 genuine gateway
   ingresses (relay DELIVERED — prerequisites held) but
   all single-cell (`nested-single 14`, `nested-multi 0`,
   `emitted-max 1`), so the multicell gate fails closed.
   53 drops (45 stale-id from churn, 8 accepted-id). No
   production row failed; B3 holds (no self-terminals).
7. **Complete external attempt 2 passes every mandatory
   row on the same SHA** — FAIL (same class, later
   epoch). Same-SHA `9bd2f39a`, fresh datadirs/ports/
   evidence (`/tmp/m11-plan263-attempt2`, attempt 2 ports):
   1047.4 s driver. Progresses through ALL build epochs +
   OBEP-data + IBGW-data WITH multicell (`17 ingresses`,
   `multicell-bounded = true`, `multicell-max = 2`),
   then stops in the receipt epoch: `b-sender-outcome =
   terminal-garlic-self:0/ingress:0/socket:0` (B OBEP
   accepted, LeaseSet resolved, zero ingress on the
   counted registration). No production row failed.
8. **Full workspace verification passes** — PASS local
   (§"Verification"); exact-head CI GREEN (run `36617070707`
   on head `4539f40`, four jobs success).
9. **Exact-head ordinary CI passes all four jobs** — PASS (run
   `36617070707` on head `4539f40`: Quality ubuntu-latest,
   Quality macos-latest, MSRV Ubuntu, Dependency policy).
10. **Registry/roadmap/support/conformance/dossier agree on
    Plan 263** — PASS (this commit reconciles all five;
    Plan 264 registered as the narrow scoping successor,
    not a second closure authority; see §"Planning
    authority").
11. **No product default/capability/version/public-network
    change** — PASS (transit stays disabled-by-default and
    non-advertised; `cargo deny` clean).
12. **No critical/high finding remains open** — RECORDED
    WITH OWNERS (see §"Findings": one MEDIUM single-mesh
    sustainability finding owned by Plan 264; no open
    production defect).

Retained Plan 262 criteria 1–21, 24, 29–30 stay green
(production-identical source; 3125 workspace tests pass).
Only criteria 6–7 (two full-matrix passes) remain open,
both on the same single-mesh sustainability boundary. The
ADR 0026 one-family M11 experimental qualification is
therefore NOT marked passed; M12 planning stays deferred.

## Executions (all fail-closed, sanitized, unmerged)

Exact pin `635b013a…`, fresh datadirs per run, loopback-only.
Raw reference logs read for diagnosis only, never as
evidence. Implementation SHA `9bd2f39a` for all counted runs
below (working tree clean at execution; pre-impl HEAD
`4cee1a6`).

- **diag-receipt1** (`I2PR_M11_ONLY_EPOCH=receipt`, fresh
  mesh, `/tmp/m11-plan263-diag-receipt`): driver FAIL
  338.9 s. `b-sender-outcome = terminal-garlic-self:0/
  ingress:0/socket:0` (B3 holds, B OBEP accepted, LeaseSet
  resolved, 4 sends, zero ingress on the counted
  registration). New Plan 263 prerequisites all passed
  (mesh live, NetDB placements present, B floodfill) — the
  starvation is downstream of every proof the harness can
  own (B-side relay/LeaseSet delivery on the unmanaged
  leg). Retained as the starved-mesh bound.
- **diag-receipt2** (`I2PR_M11_ONLY_EPOCH=receipt`, fresh
  mesh, `/tmp/m11-plan263-diag-receipt2`): driver PASS
  257.6 s. **Receipt closes.** B `m11-tx-b` established,
  LeaseSet resolved, `b-sender-outcome = terminal-garlic-
  self:0/ingress:18/socket:1`, `gateway-receipt = 1`,
  `gateway-receipt-once = true`, `multicell-bounded =
  true`, `full-tuple-bound = true` (receipt id `74527871`,
  creator-local `2957044763`, 4 rounds, send-window
  237.5 s). This is the WP B re-proof (criterion 5).
- **attempt1** (complete matrix, `I2PR_M11_ATTEMPT=1`,
  fresh datadirs/ports/evidence, same SHA `9bd2f39a`):
  driver FAIL 400.3 s. All build epochs + OBEP-data pass;
  IBGW-data observes 14 genuine gateway ingresses (relay
  delivered) but all single-cell (`emitted-max 1`);
  multicell gate fails closed. 53 drops recorded (45
  stale-id churn, 8 accepted-id). No production row
  failed; B3 holds.
- **attempt2** (complete matrix, `I2PR_M11_ATTEMPT=2`,
  fresh datadirs/ports/evidence, same SHA `9bd2f39a`):
  driver FAIL 1047.4 s. All build epochs + OBEP-data +
  IBGW-data pass WITH multicell (17 ingresses, `multicell-
  bounded = true`, `multicell-max = 2`); receipt epoch
  sets up (B OBEP accepted, LeaseSet resolved) but sends
  observe `terminal-garlic-self:0/ingress:0/socket:0`.
  No production row failed; B3 holds.

61 focused unit rows (41 live-owner + 9 service/tunnel
gateway + 5 runtime-neutral IBGW + 2 B-SAM gates + 4 new
Plan 263 harness rows, all green on the execution head)
plus the three exact-pinned source locks pass on every
run. No execution was retried to go green; each ran once
with a fresh datadir and its exact signature recorded.

## Source-lock table (retained — zero change)

| Row | Pinned needle | File | Verdict |
|---|---|---|---|
| self-loopback | `Transports::PostMessages` + `GetRouterInfo ().GetIdentHash ()` + `m_LoopbackHandler.PutNextMessage` + `m_LoopbackHandler.Flush ()` | Transports.cpp | holds |
| tunnel-gateway-by-receive-id | `eI2NPTunnelGateway` + `tunnelID = bufbe32toh (msg->GetPayload ())` + `tunnel = GetTunnel (tunnelID);` + `HandleTunnelGatewayMsg (tunnel, msg);` + `tunnel->SendTunnelDataMsg (msg);` | Tunnel.cpp | holds |
| tunnel-gateway-no-creator-peer-affinity | absence of `GetIdentHash`/`previous_peer`/`creator`/`build-creator` in the counted `eI2NPTunnelGateway` dispatch + `HandleTunnelGatewayMsg` path | Tunnel.cpp | holds |

All verified `grep -qF` against the exact-pinned tree plus
runner-enforced (`record_guarded`) in every lane invocation.

## Single-mesh sustainability analysis (not evidence — reading)

The four runs decompose the boundary precisely:

- When the mesh is healthy at send time, EVERY semantic row
  passes: receipt closes with tuple-bound multicell socket
  delivery (diag-receipt2), IBGW multicell closes with
  bounded multi-cell emission (attempt2 IBGW-data).
- When the mesh starves, NOTHING reaches the corrected seam
  (0/0/0 with B3 holding — the self-loop arm is never
  entered, so no ownership row can fail).
- When the mesh half-delivers (attempt1: 14 single-cell
  ingresses), the relay path demonstrably works but the
  unmanaged B-side emission never fragments in that window;
  the reference CAN fragment the same path (attempt2 max
  2), so the signature is window-dependent, not structural.

The production seam is therefore never the stop: it
processes every ingress it receives correctly (multicell
emission, tuple binding, socket delivery all proven), and
every stop is upstream (relay starvation, unfragmented
windows, SAM/mesh churn) on legs the lane does not own
(unmanaged B-side admission/quotas, floodfill churn,
reference scheduling). Sustaining ONE mesh through ~15
consecutive counted epochs (~400–1050 s) is the remaining
gap, and no further harness proof can close it without
either tuning (forbidden) or re-scoping the single-mesh
two-pass gate (Plan 264 authority).

## Mesh-sustainability budget (WP A/B)

- diag-receipt1: 338.9 s driver (4 B sends + drains +
  freshness gates + new prerequisite proofs). Prerequisites
  pass; sends starve downstream. No timeout inflated.
- diag-receipt2: 257.6 s driver (4 B sends + drains +
  gates). Mesh healthy; receipt closes (criterion 5).
- attempt1: 400.3 s driver (full matrix through IBGW-data;
  4 `m11-tx-ibgw` relay rounds + drains + gates). Relay
  delivers 14 single-cell ingresses; multicell gate stops.
- attempt2: 1047.4 s driver (full matrix through receipt;
  all prior epochs + 4 `m11-tx-b` rounds + drains +
  gates). IBGW multicell passes; receipt starves.
- Per-epoch wall times and tunnel-test onset are carried by
  the ledger `logical_ms` column + reference logs (retained
  under `/tmp/m11-plan263-*`, unmerged; sanitized
  counts/hashes only in evidence files).
- The WP B receipt-first full matrix was executed twice on
  one SHA as required; both stops are environmental
  (relay emission windows / mesh starvation), not
  semantic. No quota/timeout/retry tuning was applied. The
  scoping successor (Plan 264) owns the single-mesh gate;
  production routing code is frozen.

## Verification (local truth; CI labeled)

On the implementation head `9bd2f39a` (all green):

- `cargo fmt --all --check` — clean.
- `cargo check --locked --workspace --all-targets` — clean.
- `cargo test --locked --workspace --all-targets --
  --test-threads=1` — **3125 passed, 27 ignored**, 106
  suites, 0 failed (tunnel 396, daemon 1261 + 26 ignored
  incl. the 4 new Plan 263 rows + retained rows;
  `m11_transit_live_owner` 41 rows).
- Focused: every new Plan 263 test exactly by name green
  (4 `plan263_*` rows).
- `cargo clippy --locked --workspace --all-targets
  --all-features -- -D warnings` — no issues.
- `cargo test --locked --workspace --doc` — 0 passed
  (16 suites), 0 failed.
- `cargo deny check advisories bans sources` — ok.
- All gate scripts green (both M11 checkers with the new
  Plan 263 invariants: boundaries rule 36 + evidence
  checker sustainability section; dependency, runtime,
  service-tunnel, fixture, NTCP2/SSU2/I2CP vectors,
  NTCP2-interop, constrained-host, SAM/SSU2/I2CP/service/
  exploratory/netdb/destination/streaming/M6 checkers) +
  `git diff --check` — clean.
- Zero-production-diff proof: `git diff --name-only |
  grep -E '^crates/.*/src/'` empty on the implementation
  commit.

Hosted exact-head CI (Ubuntu/macOS/MSRV/policy): GREEN on the
closure head — Actions run `36617070707` on head `4539f40`
(Quality ubuntu-latest, Quality macos-latest, MSRV Ubuntu,
Dependency policy: all success). The run covers the complete
Plan 263 implementation plus the closure record, Plan 264
registration, and all planning/spec updates. No external
execution is required to close under retained-blocked (receipt
+ multicell re-proofs plus two fail-closed single-mesh
attempts already bound the sustainability signature).

## Invariant / failure / migration / security reviews

- Invariants: exact pin, unmodified reference, loopback-only,
  fresh datadirs/ports/evidence per run, single-owner/single-
  decode, bounded state (no new counters/channels/tasks/queues;
  new helpers are pure filesystem/capability checks + one
  error-format fn), no version/capability/advertisement change,
  no public transit, frozen lane numerics (checker-locked).
- Failure semantics: dead SSU2 links fail closed before sends
  (`mesh_liveness_status` names A/B); missing NetDB placements
  fail closed (names the exact placement); non-floodfill B
  fails closed before sends; SAM read timeout fails with the
  canonical tail via `?` (never retried); zero-ingress and
  single-cell-only windows fail closed with the exact
  terminal/multicell signatures; `#[ignore]`-gated lane
  contract untouched.
- Migration/compat: no wire-format, config, CLI, API, RI, or
  version change (transit stays non-advertised and disabled by
  default).
- Security: new rows carry public routing facts only (file
  presence, capability booleans, reason strings). No
  payload/key/digest/secret retention added. `cargo deny`
  clean. Authorization model unchanged from Plan 262.
- Concurrency: no new shared state (pure checks + error
  formatting; lane processes already lifecycle-owned).

## Findings by severity

- MEDIUM (lane + qualification, owned by Plan 264):
  **Single-mesh full-matrix sustainability.** Two same-SHA
  complete attempts stop on environmental signatures
  (attempt1 IBGW-data single-cell-only 14-ingress window,
  400.3 s; attempt2 receipt-epoch `0/0/0` starvation after
  IBGW multicell passed, 1047.4 s) after receipt (diag-
  receipt2 `0/18/1` tuple-bound) and multicell (attempt2
  IBGW-data max 2) are both proven on the same SHA. Exact
  stop provenance: attempt + diag ledgers + evidence +
  A/B logs (sanitized). The successor owns re-scoping the
  single-mesh two-pass gate (per-epoch fresh-mesh counted
  qualification) without touching production routing code,
  enlarging quotas, inflating timeouts, or retrying until
  green.
- LOW (residual, owned by Plan 264): ROUTER-kind and
  TUNNEL-forward-cell self targets keep current terminal
  behavior (Plan 262 §9 stop rule retained).
- LOW (informational): receipt-epoch establishment 1/2
  across fresh-mesh receipt diagnostics on this SHA (plus
  1/1 receipt close); the counted qualification belongs to
  the successor (WP B not waived, not claimed).
- No critical findings. No high findings. No silent rows, no
  weakened gates, no tuning.

## Planning authority

- `plans/registry.md`: Plan 263 active → retained-blocked
  with the sustainability-harness-landed/receipt-and-
  multicell-reproven/single-mesh boundary; Plan 264
  registered ready with handoff + dependencies;
  `active_plan = 264`, `next_executable_plan = 264`; M11 row
  names 264 as current authority; M12 stays deferred.
- `plans/subsystems/transit-tunnels-roadmap.md`: §4 current
  state, §6 dependency graph, §7 rows (263 retained-blocked,
  264 ready), §9 Plan 263 outcome section, §12 summary.
- `specs/support.toml`: `plan_263_status` retained-blocked
  token, new `plan_264_*` entries, `m11_transit_tunnels`
  restated (receipt + multicell re-proven live on `9bd2f39a`;
  single-mesh two-pass stopped on sustainability),
  `next_executable_plan = 264`.
- `specs/CONFORMANCE.md`: M11 rows name 263's re-proof
  (B-sender receipt + IBGW multicell exhibited, single-mesh
  two-pass unclaimed) with 264 as the forward authority; no
  support-state advance.
- `specs/protocols/05-tunnels.md`: dossier records the
  re-proven receipt/multicell with stop provenance and hands
  authority to 264.
- Historical records (`249`–`262` closures, registry archive)
  untouched.

## Roadmap disposition + unblock audit

- Plan 263 closes **retained-blocked**
  (`…-corrective-required-via-plan264`): sustainability
  harness, canonical SAM tail, 4 harness regressions, Plan
  263 manifest + checker invariants, receipt re-proof
  (`0/18/1` tuple-bound) and IBGW multicell re-proof (max
  2) are durable harness + evidence; single-mesh two-pass
  qualification is not claimed.
- Plan 264 (M11 single-mesh sustainability scoping) is
  REGISTERED `ready` in the same commit (unblock audit: its
  only hard dependency is the Plan 263 harness/re-proof
  localization, now closed; no other deps). It is the sole
  `ready` M11 plan; `active_plan = next_executable_plan =
  264`.
- `m11_transit_qualification` (receipt-capable experimental
  transit): remains unclaimed by explicit record (receipt +
  multicell proven on healthy windows, not twice as a
  single-mesh complete matrix).
- M12 floodfill: planning stays deferred (requires
  receipt-capable transit with two complete passes). No M12
  plan registered.
- M6 Java / Plan 201-247 debt, Plan 204 bookkeeping, M10
  authority: unaffected (registry audit — no other registered
  plan lists Plan 263 as a hard/interface dependency).
- Nothing else unblocks. No silent unblock.

## Docs / ops

- This record; Plan 264 plan-of-record; registry; roadmap;
  support ledger; conformance matrix; tunnel dossier
  (§"Planning authority").
- Operator impact: none (no daemon/config change in the
  closure commit; transit stays non-advertised and disabled
  by default).

## Handoff

To close: implementation already committed
(`9bd2f39aa2c8509273177eb509157d60a09cafb2`); commit this
record with Plan 264 + all planning/spec updates (this
commit), push, observe exact-head hosted CI (four jobs),
record the run ID in a follow-up commit (Plan
258/259/260/261/262 precedent). Plan 264 then owns the
single-mesh sustainability scoping. M11 rests at
ownership-corrected, receipt-and-multicell-reproven
(B3 holds everywhere, full single-mesh matrix stopped on
sustainability) until Plan 264 executes.
