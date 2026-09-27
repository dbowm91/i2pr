# Plan 257 status — retained; corrective required via Plan 258

Status:
**retained-m11-production-self-reply-and-external-evidence-completion-corrective-required-via-plan258**

Plan:
plans/implementation/transit-tunnels/257-m11-production-self-reply-and-external-evidence-completion-corrective.md

Registration baseline:
bc9b5c6172d0560d3eb8d2ef1ae0d447478ba480

Implementation head reviewed:
1784ff87a8939c038ae6d8d2d574b73332be9cd0

Corrective authority:
plans/implementation/transit-tunnels/258-m11-ibgw-data-plane-multicell-diagnostic-corrective.md

Reference authority: exact-pinned, unmodified i2pd 2.61.0 at
635b013a612ff47278ef02acf8580a28e10e26c5 (source verified clean,
loopback-only, public reseed/network disabled in all attempts).

## Disposition

Plan 257 is not closed as M11 qualification.

Its implementation is complete and locally proven, and those repairs are
retained. Three same-SHA external executions of the complete corrected
matrix (two counted attempts plus one B-loglevel diagnostic, all on
1784ff8 with fresh datadirs) stop at the identical retained IBGW
data-plane row with an identical quantitative signature. That boundary is
outside the Plan 257 reply/state/evidence scope, so per Plan 257 §21 the
evidence is preserved, no row is weakened, and Plan 258 is registered as
the narrow diagnostic corrective. This status narrows the interpretation
of the Plan 257 implementation; it does not rewrite its evidence.

Historical Plan 256 implementation evidence is preserved under
plans/closure/transit-tunnels/256-status.md.

## Retained implementation evidence (local, all green on 1784ff8)

Work package I (macOS Clippy corrective): the Plan 256
`too_many_arguments` failures (`TransitBuildService::
deliver_self_reply_otbrm` 9/7, driver `poll_rx_datagrams` 8/7) are
corrected without suppression — `SelfReplyOtbrmArgs` bundle in
`crates/i2pr-daemon/src/transit_compose.rs` plus call-site update in
`transit_owner.rs`, `RxDatagramCounts` bundle in the driver.
`cargo clippy --locked --workspace --all-targets --all-features --
-D warnings` passes (exit 0).

Work package C (state snapshot): `TransitLiveStateSnapshot`
(active/pending-global/pending-peer/peer-index/queued-work) with
`zero()`, `is_zero()`, secret-free `evidence_label()`;
`TransitBuildService::live_state_snapshot()` plus
`pending_peer_entries()`/`peer_index_entries()` seams;
`TransitLiveOwner::live_state_snapshot()`/`has_peer()` delegation;
`TransitAdmissionState::pending_peer_entries()` in `i2pr-tunnel`.

Work package E (typed bandwidth): `TransitBandwidthSummary` in
`i2pr-tunnel` (request m/r/l + reply b + accepted flag,
`evidence_label()`, `request_is_empty()`); `bandwidth_request` added to
`TransitBuildOutcome` and `TransitBuildMessageOutcome` on both accept
and reject paths; `bandwidth` carried on every `TransitDispatch`
variant via `build_dispatch_from_outcome`; copied onto
`TransitBuildEvidence` (fatal inputs carry the empty summary). The
daemon never re-decodes the `Mapping` (boundary script rule 26).

Work packages §4+B (reply qualification): exact-pinned source locks for
the remote garlic/TunnelGateway branch (`RGarlicKeyAndTag`,
`CreateTunnelGatewayMsg`, `WrapECIESX25519Message`) and the local IBGW
branch (`IBGW is local`, `SendTunnelDataMsg (replyMsg)`,
`FlushTunnelDataMsgs`, fail-closed drop line) plus the B-side endpoint
line (`TransitTunnel: handle msg for endpoint `), all verified against
`TransitTunnel.cpp` at the exact pin; 10 new service-level regressions
in `transit_compose.rs` (remote-once, local-bypass, id preservation,
snapshot dimensions, +1 cardinality, +0 reject, bandwidth absence/
presence, full-drain cancel, A-removed/B-retained close, missing-IBGW
rollback).

Work package A (negative evidence): 12 new driver-local predicate rows
(local-only forward, exact-tunnel binding, creator-secondary,
active-only drain, A-without-B close, constructor-only restart,
single-attempt/cross-SHA/merge rejections, hard-coded bandwidth,
receive-id-only cardinality, runner source-lock presence).

Work packages D/F/G/H (external evidence framework): exact cardinality
helper (`record_cardinality_rows`/`record_cardinality_for_obs`, delta
exactly +1 with counted receive-id and pending baseline), typed
bandwidth helper (`record_bandwidth_rows`, never hard-coded),
independent B-side far-side proof (`count_endpoint_messages` on the
source-locked debug line + `far_side_satisfied` binding to the exact
next tunnel; B runs at debug in the counted lane), full-drain cancel
(all five dimensions + genuine-cell new-ingress-refused probe), A/B
session close (`has_peer` membership, five rows), real runtime restart
(old drained → new zero → sessions re-established → fresh OBEP build
accepted with +1 → final drain; seven rows).

Work package J (two-attempt gate): workflow matrix `attempt: [1, 2]`
with `fail-fast: false`, per-attempt ports/evidence/artifact names,
manifest `plan: 257` + `attempt` + `fresh_datadir_id`; `m11_row`/
`exact_row`/`exact_lib_row` discipline; 148 guarded rows + 43 epoch
keys in the hardened checker.

## Verification outcomes (local truth, on 1784ff8)

- `cargo fmt --all --check`: pass.
- `cargo check --locked --workspace --all-targets`: pass.
- `cargo test --locked --workspace --all-targets -- --test-threads=1`:
  pass, exit 0 (106 suites ok, 0 failed).
- Focused: `i2pr-tunnel` 375 + 5 passed; `i2pr-daemon --lib` 261
  passed (10 new plan257 rows); `m11_transit_live_owner` 34 passed;
  `m11_transit_i2pd_external` local 40 passed + 1 ignored (external
  driver, fail-closed without env).
- `cargo clippy --locked --workspace --all-targets --all-features --
  -D warnings`: pass.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`:
  pass. `cargo test --locked --workspace --doc`: pass.
- `cargo deny check advisories bans sources`: pass.
- All 20 boundary/acceptance scripts: pass, including the extended
  `check-m11-transit-boundaries.sh` (rules 25–30) and
  `check-m11-transit-qualification-evidence.sh` (148 guarded rows,
  43 epoch keys, §15 rejections).
- `git diff --check`: pass.
- Hosted exact-head ordinary CI on the closure head 86adc3f (Actions
  run 36338600245): Quality (ubuntu-latest) success, Quality
  (macos-latest) success, MSRV (Ubuntu) success, Dependency policy
  success. The Plan 256 macOS Clippy failure class is structurally
  removed by the Plan 257 arg-bundle fix. Local MSRV/deny floors pass
  identically. Plan 258 must still go green on its own head.

## External evidence (all on implementation SHA 1784ff8)

| Attempt | Evidence root | Result | Driver time | Gateway deliveries | multicell-max | Receipt |
|---|---|---|---|---|---|---|
| 1 (counted) | target/interop/m11-plan257-attempt-1 | fail at `multicell-bounded` | 395 s | 9 | 1 | 0 |
| 2 (counted) | target/interop/m11-plan257-attempt-2 | fail at `multicell-bounded` | 431 s | 13 | 1 | 0 |
| diag (B info) | target/interop/m11-plan257-diag-infoB | fail at `multicell-bounded` | 455 s | 14 | 1 | 0 |

Manifests name `plan: 257`, the same `i2pr_commit: 1784ff8...`,
per-attempt ids, and fresh datadir ids. Local rows (identity,
ledger, source locks, unit regressions, static gates) pass in all
three runs; every external bootstrap/role/OBEP-data row passes;
the lane stops fail-closed at the retained IBGW multicell gate, so
no Plan 257 far-side/cancel/session-close/restart row executes
externally in any attempt. Raw reference logs are diagnostic input
only; counted evidence holds sanitized counts/ids.

Transport snapshots during every `ibgw-data` drain show stable
sessions (`sess=2+0/0`, `nosess=0`, zero queue/auth drops) with live
traffic both directions — the i2pr-side session-death hypothesis is
excluded by data. The info-B diagnostic reproduces the identical
signature, excluding Plan 257 work package F's B-debug requirement
as a destabilizing factor.

## Requirement-to-evidence disposition (33 Plan 257 criteria)

| # | Criterion | Disposition |
|---|---|---|
| 1 | Plan 256 history traceable, not rewritten | retained (256-status untouched) |
| 2 | Exact-pinned i2pd 2.61.0 reference | retained (pin verified every run) |
| 3 | Loopback / no public reseed | retained (gates green 3/3 runs) |
| 4 | RouterIdentity/build-key coherence | retained (local rows green) |
| 5 | Exact NetDB owner/load proof | retained (bootstrap rows pass 3/3) |
| 6 | Typed OBEP/IBGW/Participant epochs | retained (role rows pass 3/3) |
| 7 | Reply path source-locked | retained (3 source-lock rows green) |
| 8 | Local self-reply via live IBGW | retained locally (unit); externally unexecuted |
| 9 | Remote reply garlic/TunnelGateway once | retained locally (unit); externally unexecuted |
| 10 | Missing/wrong/expired IBGW fails closed | retained locally (unit) |
| 11 | Registration delta exactly +1 | implemented; externally unexecuted (lane stops earlier) |
| 12 | Pending returns to baseline | implemented; externally unexecuted |
| 13 | Participant far-side B observation | implemented; externally unexecuted |
| 14 | Creator-accepted secondary only | implemented (predicate + checker); externally unexecuted |
| 15 | OBEP unfragmented + fragmented delivery | retained (OBEP-data rows pass 3/3) |
| 16 | IBGW gateway ingress + multicell | **STOPPED** — ingress proven (9–14 genuine deliveries 3/3), multicell-max 1 and receipt 0 systematically |
| 17 | Replay no second delivery | unexecuted (lane stops earlier) |
| 18 | Code-30 no registration + baseline | retained (reject rows pass 3/3; typed bandwidth rows recorded) |
| 19 | Typed m/r/l + b disposition | implemented; reject-tier rows recorded 3/3; role-tier unexecuted |
| 20 | Expiry drops + cleanup | unexecuted (lane stops earlier) |
| 21 | Cancel all-dimensions zero | implemented locally (unit + predicate); externally unexecuted |
| 22 | Session close A-removed/B-retained | implemented locally; externally unexecuted |
| 23 | Real runtime restart + fresh build | implemented; externally unexecuted |
| 24 | Checker rejects §15 shapes | retained (checker green; 148 rows + rejections enforced) |
| 25 | No raw logs/secrets as evidence | retained (secret gates green) |
| 26 | Complete attempt 1 all rows | **not met** (stops at criterion 16) |
| 27 | Complete attempt 2 all rows | **not met** (stops at criterion 16) |
| 28 | Same SHA + fresh datadirs | retained (all 3 runs same SHA, fresh datadirs, attempt ids) |
| 29 | Full workspace verification | retained (floor green on 1784ff8) |
| 30 | Exact-head Ubuntu/macOS/MSRV/policy | **not observed** (no push yet; owned by Plan 258 head) |
| 31 | macOS Clippy corrected w/o suppression | retained (structs; workspace clippy `-D warnings` green) |
| 32 | No product/config/version/capability change | retained (boundary scripts green) |
| 33 | No critical/high finding | **open** — one high finding (see below) |

## Findings by severity

### High — retained IBGW multicell/receipt boundary (owns Plan 258)

Three same-SHA executions stop at `ibgw-data/multicell-bounded` with
genuine single-cell-only gateway ingress (9/13/14 deliveries to the
accepted registration), zero end-to-end receipt, healthy sessions, and
B-loglevel independence. The lane cannot currently distinguish
emission-side (reference never delivers a complete multi-cell batch),
delivery-side (onward delivery fails silently — `failures` unrecorded),
or production (missing ingress reassembly) causes. No Plan 257-scope
defect is evidenced; no row is weakened to go green. Corrective: Plan
258 (telemetry → classify H1/H2/H3 → narrow fix → two counted passes).

### Medium — Plan 257 external rows implemented but unexecuted

Cardinality, role-tier bandwidth, far-side, replay/expiry, full-drain
cancel, session close, and real restart never execute externally because
the driver aborts fail-closed at the IBGW gate. The code is reviewed and
locally tested but carries no external proof. Owned by Plan 258 §D.

### Medium — exact-head hosted CI observed green on the closure head (residual: Plan 258 head)

Actions run 36338600245 on closure head 86adc3f is green on all four
required jobs (Ubuntu Quality, macOS Quality, MSRV, dependency policy).
The Plan 256 macOS Clippy failure class is proven removed hosted-side.
Residual: Plan 258 must still go green on its own implementation head
per its §D; this record does not pre-claim it.

### Low — diagnostic evidence roots are local-only

`target/interop/m11-plan257-*` roots are git-ignored working copies, not
committed artifacts. Sanitized counts/manifest fields cited above are the
committed evidence; full reruns reproduce them from SHA 1784ff8.

## Security review

No secret material in evidence (checker secret gates green 3/3 runs;
`TransitBandwidthSummary`/`TransitLiveStateSnapshot` carry counts only;
`Debug` impls redacted and unit-asserted). Secret-owning wrappers remain
move-only (`Clone` prohibitions green). No public transit surface, no
config/CLI/RouterInfo/version change (boundary scripts green). The B
debug-log requirement is bounded to the counted lane and contains no
key material (reference logs stay out of evidence).

## Resource/contention review

All maps/queues/ledgers remain bounded (`MAX_TRANSIT_*`,
`MAX_LEDGER_OBSERVATIONS`, `MAX_RETAINED_CELLS` ceilings untouched; no
per-cell tasks; cancellation synchronous; restart transfers no state).
The far-side 5 s settle sleep is bounded and inside the driver's
existing 1500 s timeout. No new dependency.

## Compatibility/migration

None: no config/CLI/wire/advertisement change. `TransitDispatch`,
`TransitBuildOutcome`, `TransitBuildMessageOutcome`, and `Observation`
gain non-secret fields; all constructors updated in-tree; workspace
check green.

## Roadmap disposition

- Plan 257: retained corrective infrastructure with valid
  reply/state/evidence repairs; external closure requires Plan 258.
- Plan 258: registered ready as the IBGW data-plane diagnostic
  corrective and the next executable M11 authority.
- M11 experimental progression: open (no `passed-via-i2pd-2.61.0` claim).
- M12 floodfill planning: blocked until two complete same-SHA
  exact-pinned i2pd passes close the lane.

## Unblock audit (required)

- `m11_sequence`: 249 → 250 → 252 → 253 → 254 → 255 → 256 → 257 →
  **258**. Plan 258 lists Plan 257 as its only hard dependency (closed
  as retained infrastructure with the boundary above); all interface
  deps (exact pin, NetDB owner contract, typed evidence vocabulary)
  are stable and green locally. → Plan 258 moves to `ready` in this
  same commit.
- No other registered plan lists Plan 257 as a hard dependency.
  M12 floodfill planning stays deferred (its gate is two-pass M11
  external closure, still unmet). Java full-router lane (Plan 247
  retained/deferred) is unaffected. No plan transitions to `ready`
  besides 258; nothing is silently unblocked.
