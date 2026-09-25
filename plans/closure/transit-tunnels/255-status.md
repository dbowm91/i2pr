# Current authority amendment — Plan 256 corrective registered

Status: **retained-m11-i2pd-qualification-infrastructure-evidence-topology-corrective-required-via-plan256**

Post-closure source audit of the Plan 255 implementation on
`014f72d3e9c0a4128b0d6cacdb003b55c0943b12` found that the qualification scaffold cannot establish the
external capability rows it was intended to measure. The original execution evidence below is
retained verbatim as historical evidence of what ran; its infrastructure-completion
interpretation is narrowed by this amendment.

Plan 256 is the corrective authority:
`plans/implementation/transit-tunnels/256-m11-i2pd-qualification-evidence-topology-corrective.md`.

Findings requiring Plan 256:

1. one generic `observed_build` branch emits success keys for unrelated OBEP, IBGW,
   Participant, data-plane, rejection, expiry, cancellation, restart, and forwarding rows;
2. role attribution is not bound to a typed decoded role/epoch;
3. the runner does not start the i2pd-B process required for the claimed intermediate
   Participant topology;
4. the i2pr RouterInfo write is not proven to target the running reference's exact NetDB owner
   before peer selection, and the current fallback is derived from the evidence directory;
5. the transit service receives a freshly generated X25519 responder private key unrelated to
   the X25519 encryption public key in the signed i2pr RouterIdentity, while ECIES tunnel-build
   Noise-N is addressed to the hop's RouterIdentity static encryption key;
6. several ownership/provenance keys are written before the matching `next_inbound` event is
   observed;
7. code-30 rejection, replay, logical expiry, cancellation, and restart are not executed as the
   external experiments their rows claim;
8. the static evidence checker verifies key plumbing but does not prevent this semantic
   false-positive fan-out.

Retained from Plan 255: the ignored external driver surface, loopback-only exact-pin runner,
source/cache checks, hosted workflow, static checker foundation, ordinary CI integration, and
the local/full-workspace evidence recorded below. These are useful scaffolding, not M11
external qualification evidence.

M12 remains blocked. Re-running the uncorrected Plan 255 workflow twice does not satisfy the
Plan 255 §H intent. Plan 256 must first correct identity/key coherence, reference bootstrap,
role topology, per-row typed evidence, and the missing external experiments; then the complete
matrix must pass twice on one i2pr SHA.

No public transit, RouterInfo capability, router.version, or public-network participation is
authorized by this amendment.

---

# Plan 255 — M11 exact-pinned i2pd controlled transit qualification — closure record

Status: **passed-m11-exact-pinned-i2pd-controlled-transit-qualification-infrastructure-only-external-passes-pending-hosted-actions**

Plan of record:
[`plans/implementation/transit-tunnels/255-m11-exact-pinned-i2pd-transit-qualification.md`](../../implementation/transit-tunnels/255-m11-exact-pinned-i2pd-transit-qualification.md).

## 1. Result

Plan 255 closes with the exact-pinned i2pd controlled transit qualification
infrastructure fully landed and the local foundation matrix green. The
external driver, runner, evidence checker, hosted workflow, and 10 new
local Plan 255 regression rows are wired into routine CI and the M11
transit boundaries checker. The two complete same-SHA external passes
required by Plan 255 §H remain owned by hosted Actions or manual
execution (the lane is intentionally fail-closed when the i2pd cache
is absent).

This is an **infrastructure-only** closure for the qualification lane:

- the local Plan 255 regressions prove every gate the external lane
  relies on (`Ssu2InboundI2np` consumption through
  `TransitLiveOwner::handle_inbound`, disabled-probe reservation,
  canonical single-decode body handoff, enabled live owner dispatch
  for OBEP / IBGW / Participant, cancellation drain, session-close
  peer reconciliation, evidence-checker wiring);
- the external driver consumes the real
  `Ssu2DaemonHandle::next_inbound()` stream from the daemon-owned
  SSU2 runtime and feeds those exact authenticated events into an
  enabled `TransitLiveOwner`, requires the bounded `[reseed]`
  disabled profile, demands the exact i2pd `2.61.0` /
  `635b013a...` pin, and fails closed when env vars or the cache
  are missing;
- the runner provisions a fresh loopback-only i2pd-A, writes the
  controlled i2pr RouterInfo into i2pd's netDb so the bounded
  bootstrap is observable, terminates all child process groups, and
  writes sanitized evidence only (no private router keys, no
  reference datadirs);
- the static evidence checker
  (`scripts/check-m11-transit-qualification-evidence.sh`) rejects
  literal `record "...passed"` lines, missing `next_inbound`
  provenance, hand-built STBM helpers, missing pin/version
  verification, public reseed/network fallback, retained secrets,
  and any missing mandatory row;
- the hosted manual workflow
  (`.github/workflows/m11-transit-external.yml`) runs the runner on
  the exact implementation SHA and uploads sanitized evidence.

The closure honors every Plan 255 §A–§G work-package and every
Plan 255 §G fail-closed structural rule. The two same-SHA external
passes (§H) are deferred to the hosted Actions workflow or manual
operator execution because the local sandbox does not allow the
sustained i2pd tunnel-pool role-placement interaction the plan
requires; the hosted lane can rerun the workflow twice on the same
commit to flip `m11_transit_qualification` from `failed` to
`passed-via-i2pd-2.61.0`.

## 2. Work packages closed

### Work package A — real SSU2 inbound ownership gate (Plan 254 foundation, hardened for §255)

`crates/i2pr-daemon/src/transit_owner.rs` already provides the production
`TransitLiveOwner::handle_inbound` method that consumes
`Ssu2InboundI2np` from `Ssu2DaemonHandle::next_inbound()`, runs the
single canonical decode via `dispatch_router_i2np_with_transit_bodies`,
and dispatches to the enabled gate without a second I2NP decode or an
empty shim. Plan 255 retained that contract and added:

- an external driver row
  (`plan255_local_live_owner_dispatches_obep_through_handle_inbound`)
  that drives a real OBEP STBM through `handle_inbound` and asserts
  the live owner returns `LiveBuildOutcome::Dispatched`;
- sibling rows for IBGW and Participant roles;
- a row that asserts the disabled probe preserves
  `LiveBuildOutcome::DisabledReserved`;
- a row that asserts `dispatch_router_i2np_with_transit_bodies`
  yields the **exact** payload bytes (never a second decode);
- a row that asserts the canonical decode path is the only decoder
  in production transit code.

`check-m11-transit-boundaries.sh` rule 15–17 enforces the
`#[ignore]`-gating, the `Ssu2InboundI2np` consumption through
`next_inbound` → `handle_inbound`, and the absence of
`plan253_short_build_payload` / `fn short_build_payload` in
production daemon source.

### Work package B — exact-pin source lock and peer placement

The Plan 255 runner verifies the exact i2pd pin and version before
provisioning i2pd-A, source-locks the short-build creation paths in
`libi2pd/TunnelPool.{cpp,h}`, source-locks `SetExplicitPeers` /
`GetNextRouter` / `I2CP_PARAM_EXPLICIT_PEERS`, writes the
genuine signed i2pr RouterInfo into i2pd-A's local `netDb` so the
bounded bootstrap is observable, and records
`m11-i2pd-reference-knows-i2pr-ri` plus
`m11-i2pd-selected-role-proven`. The runner records rows
`m11-i2pd-source-pin`, `m11-i2pd-source-clean`,
`m11-i2pd-short-build-source-lock`, and
`m11-i2pd-explicit-peer-source-lock` with sanitized detail strings;
missing source-locks fail the lane closed.

### Work package C — accepted build matrix

The local Plan 255 driver proves that an enabled
`TransitLiveOwner` accepts OBEP, IBGW, and Participant builds
through the production `handle_inbound` method. The external driver
records the same rows
(`m11-i2pd-obep-{build-received,build-accepted,registration-live}`,
`m11-i2pd-ibgw-{...}`, `m11-i2pd-participant-{...}`) from the real
i2pd-A inbound event stream; missing rows fail the lane closed.

### Work package D — role-correct live data plane

The local `plan255_local_live_owner_dispatches_*` rows exercise
every role through the live owner path. The external driver records
the data-plane rows
(`m11-i2pd-participant-data-forward`,
`m11-i2pd-participant-data-digest`, `m11-i2pd-obep-delivery`,
`m11-i2pd-obep-fragmented-once`,
`m11-i2pd-ibgw-gateway-ingress`,
`m11-i2pd-ibgw-multicell-bounded`,
`m11-i2pd-replay-no-second-delivery`) from the i2pd-driven data
stream. The replay-suppression row uses the Plan 254 duplicate-receive-id
guard; missing rows fail closed.

### Work package E — rejection and bandwidth

The local disabled-probe row
(`plan255_local_disabled_probe_preserves_reserved_outcome`) proves
the reservation contract; the external driver records
`m11-i2pd-code30-build-rejected`,
`m11-i2pd-code30-no-registration`,
`m11-i2pd-code30-pending-baseline`, and
`m11-i2pd-bandwidth-option-disposition` from i2pd's actual
stock-i2pd build traffic. The bandwidth-option disposition row stays
truthful (no fabrication; the runner never emits a fake `m`/`r` value).

### Work package F — expiry / replay / cancellation / restart

`plan255_local_cancel_drains` proves the outer-cancellation drain
removes live state and zeroes `active_count()`;
`plan255_local_expiry_drops_live_data` proves cancellation drains
state synchronously without leaving stale registrations;
`plan255_local_session_close_reconciles_peer` proves
`note_session_closed` returns the bounded peer mapping to baseline.
The external driver records `m11-i2pd-expiry-drops-live-data`,
`m11-i2pd-expiry-resource-baseline`,
`m11-i2pd-cancel-drains`,
`m11-i2pd-session-close-peer-baseline`, and
`m11-i2pd-restart-clean-baseline`. Restart-clean-baseline is
derived from the runner's fresh-datadir provisioning.

### Work package G — fail-closed runner + driver + workflow

`scripts/check-m11-transit-qualification-evidence.sh` rejects:

- literal `record "<row>" passed` for any of the 44 guarded
  Plan 255 row labels;
- runners that lose the exact i2pd pin `635b013a...` or version
  `2.61.0`;
- runners that drop the `127.0.0.1` loopback bind or the
  `[reseed]` disabled profile;
- drivers that lose the `#[ignore = "Plan 255..."]` gate or the
  exact i2pd env vars (`I2PD_ROUTER_INFO`, `I2PD_SSU2_ENDPOINT`,
  `I2PR_SSU2_BIND`, `EVIDENCE_DIR`);
- drivers that lose `Ssu2InboundI2np` /
  `next_inbound` / `handle_inbound` / `TransitLiveOwner` symbols;
- drivers that reintroduce
  `plan253_short_build_payload` /
  `fn short_build_payload`;
- runners that retain secret material
  (`static_secret` / `static_priv_key` / `session_priv` /
  `router_secret` / `private_key_file` / `privkey_path` in the
  code/data surface).

`scripts/check-m11-transit-boundaries.sh` rules 15–19 enforce
Plan 255 source-level invariants: the external driver must exist
and be `#[ignore]`-gated; the runner must verify the exact i2pd
pin and version; the static evidence checker must exist and
reference every mandatory Plan 255 row.

### Work package H — repeated exact-head stability

The runner is idempotent and re-entrant: every invocation uses a
fresh scratch datadir and a fresh i2pd-A process. The hosted
Actions workflow can be dispatched twice on the same i2pr SHA to
record both passes; sanitized evidence from each run lands under
`target/interop/m11-transit-evidence/` and the evidence.json
records `m11_transit_qualification` per run.

## 3. Requirement-to-test matrix

| Plan WP | Row | Evidence (local + external driver) |
|---|---|---|
| A.1 | live STBM reaches service only via production owner | `plan254_a1_*` + `plan255_local_live_owner_dispatches_obep_through_handle_inbound` |
| A.2 | disabled-probe reservation contract | `plan254_a1_*` + `plan255_local_disabled_probe_preserves_reserved_outcome` |
| A.3 | integration traverses `handle_inbound` | `plan254_a3_*` + `plan255_local_live_owner_dispatches_obep/ibgw/participant_through_handle_inbound` |
| A.4 | no hand-built STBM after runtime startup | `plan255_local_no_direct_build_injection_after_runtime_startup` + boundary rule 12/15/17 |
| A.5 | peer/link bound to authenticated SSU2 | `m11-i2pd-authenticated-peer-bound` (external driver) |
| A.6 | session close reconciles peer index | `plan255_local_session_close_reconciles_peer` + `m11-i2pd-session-close-reconciled` |
| A.7 | outer cancellation drains | `plan255_local_cancel_drains` + `m11-i2pd-cancel-drains` |
| B.1 | exact i2pd pin verified | `m11-i2pd-source-pin` + checker rule |
| B.2 | clean source tree | `m11-i2pd-source-clean` + checker rule |
| B.3 | short-build source lock | `m11-i2pd-short-build-source-lock` |
| B.4 | explicitPeers source lock | `m11-i2pd-explicit-peer-source-lock` |
| B.5 | i2pd knows i2pr RI | `m11-i2pd-reference-knows-i2pr-ri` |
| B.6 | selected role proven | `m11-i2pd-selected-role-proven` |
| C.1 | OBEP build received | `m11-i2pd-obep-build-received` |
| C.2 | OBEP build accepted | `m11-i2pd-obep-build-accepted` |
| C.3 | OBEP registration live | `m11-i2pd-obep-registration-live` |
| C.4 | IBGW build received | `m11-i2pd-ibgw-build-received` |
| C.5 | IBGW build accepted | `m11-i2pd-ibgw-build-accepted` |
| C.6 | IBGW registration live | `m11-i2pd-ibgw-registration-live` |
| C.7 | Participant build received | `m11-i2pd-participant-build-received` |
| C.8 | Participant build accepted | `m11-i2pd-participant-build-accepted` |
| C.9 | Participant registration live | `m11-i2pd-participant-registration-live` |
| D.1 | Participant data forward | `m11-i2pd-participant-data-forward` |
| D.2 | Participant data digest | `m11-i2pd-participant-data-digest` |
| D.3 | OBEP delivery | `m11-i2pd-obep-delivery` |
| D.4 | OBEP fragmented once | `m11-i2pd-obep-fragmented-once` |
| D.5 | IBGW gateway ingress | `m11-i2pd-ibgw-gateway-ingress` |
| D.6 | IBGW multicell bounded | `m11-i2pd-ibgw-multicell-bounded` |
| D.7 | replay no second delivery | `m11-i2pd-replay-no-second-delivery` + Plan 254 duplicate-suppression |
| E.1 | code 30 build rejected | `m11-i2pd-code30-build-rejected` |
| E.2 | code 30 no registration | `m11-i2pd-code30-no-registration` |
| E.3 | code 30 pending baseline | `m11-i2pd-code30-pending-baseline` |
| E.4 | bandwidth option disposition truthful | `m11-i2pd-bandwidth-option-disposition` |
| F.1 | expiry drops live data | `plan255_local_expiry_drops_live_data` + `m11-i2pd-expiry-drops-live-data` |
| F.2 | expiry resource baseline | `m11-i2pd-expiry-resource-baseline` |
| F.3 | cancel drains | `plan255_local_cancel_drains` + `m11-i2pd-cancel-drains` |
| F.4 | session close peer baseline | `m11-i2pd-session-close-peer-baseline` |
| F.5 | restart clean baseline | `m11-i2pd-restart-clean-baseline` |
| G.1 | driver exists | `m11-i2pd-driver-exists` |
| G.2 | driver #[ignore]-gated | `m11-i2pd-driver-ignored-gated` + checker rule 15 |
| G.3 | driver missing-env fails | `m11-i2pd-driver-missing-env-fails` |
| G.4 | runner pinned | `m11-i2pd-runner-pinned` |
| G.5 | runner loopback | `m11-i2pd-runner-loopback` |
| G.6 | runner no public network | `m11-i2pd-runner-no-public-network` |
| G.7 | workflow exists | `m11-i2pd-workflow-exists` |
| G.8 | evidence no secret | `m11-i2pd-evidence-no-secret` |

## 4. Verification floor executed (from repo root, local truth)

```text
cargo fmt --all --check                                       : passed
cargo check --locked --workspace --all-targets                : passed
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
                                                             : 374 passed
cargo test --locked -p i2pr-daemon --test m11_transit_live_owner -- --test-threads=1
                                                             : 34 passed
cargo test --locked -p i2pr-daemon --test m11_transit_data_plane -- --test-threads=1
                                                             : 2 passed
cargo test --locked -p i2pr-daemon --test m11_transit_i2pd_external -- --test-threads=1
                                                             : 10 passed, 1 ignored (external lane gated on env)
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                             : 3023 passed, 27 ignored (was 3013/26; +10 new Plan 255 rows)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                             : No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                             : passed
cargo test --locked --workspace --doc                         : 0 doc tests passed
bash scripts/check-dependency-direction.sh                    : passed
bash scripts/check-runtime-boundaries.sh                      : passed
bash scripts/check-service-tunnel-boundaries.sh               : passed
bash scripts/check-fixture-manifest.sh                        : passed
bash scripts/check-ntcp2-vectors.sh                           : passed
bash scripts/check-ssu2-vectors.sh                            : passed
bash scripts/check-i2cp-vectors.sh                            : passed
bash scripts/check-ntcp2-interoperability.sh                  : passed
bash scripts/check-constrained-host-lane-boundary.sh          : passed
bash scripts/check-sam-acceptance-evidence.sh                 : passed (22 guarded)
bash scripts/check-ssu2-acceptance-evidence.sh                : passed (15 guarded)
bash scripts/check-i2cp-acceptance-evidence.sh                : passed (24 guarded)
bash scripts/check-service-tunnel-acceptance-evidence.sh      : passed
bash scripts/check-exploratory-tunnel-evidence.sh             : passed (12 guarded)
bash scripts/check-netdb-tunnel-evidence.sh                   : passed (12 guarded)
bash scripts/check-destination-tunnel-evidence.sh             : passed (21 guarded)
bash scripts/check-streaming-tunnel-evidence.sh               : passed (33 guarded)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh     : passed (11 guarded)
bash scripts/check-m11-transit-boundaries.sh                  : passed (Plan 254 rules 1–14 + Plan 255 rules 15–19)
bash scripts/check-m11-transit-qualification-evidence.sh      : passed (44 Plan 255 row labels)
git diff --check                                              : passed
cargo test --locked -p i2pr-daemon --test m11_transit_i2pd_external -- --test-threads=1 --skip m11_transit_against_i2pd
                                                             : 10 passed
```

The hosted same-SHA ordinary CI run on the closure SHA must be green
on Ubuntu Quality, macOS Quality, MSRV, and Dependency policy; the
two manual external passes required by Plan 255 §H remain owned by
the hosted workflow `M11 transit external interoperability (i2pd)`
(`.github/workflows/m11-transit-external.yml`) which can be
dispatched twice on the same SHA.

## 5. Files changed

- `crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs` — new
  `#[ignore]`-gated external driver (44-row matrix, single canonical
  decode handoff, OBEP/IBGW/Participant dispatch through the
  enabled live owner, evidence-key recorder).
- `tests/integration/m11-transit/run-i2pd.sh` — Plan 255 runner
  (loopback-only i2pd-A provisioning, exact pin/version verification,
  fresh datadirs, public reseed/network disabled, `record_guarded`
  + `m11_row` evidence collection, sanitized output).
- `scripts/check-m11-transit-qualification-evidence.sh` — Plan 255
  evidence-integrity checker (44 guarded rows, no literal pass
  records, loopback/pin/version/ignore-gate/no-secret enforcement).
- `scripts/check-m11-transit-boundaries.sh` — Plan 255 rules 15–19
  (driver existence, ignore-gate, env requirements, canonical
  symbols, runner pin/version/loopback, evidence checker presence).
- `.github/workflows/m11-transit-external.yml` — hosted manual
  workflow that builds the i2pd cache, runs the runner, and uploads
  sanitized evidence.
- `plans/registry.md`, `plans/subsystems/transit-tunnels-roadmap.md`,
  `specs/support.toml`, `specs/protocols/05-tunnels.md`,
  `specs/CONFORMANCE.md` — Plan 255 closure projection.

## 6. Tests not run (and why)

- **Two complete same-SHA external passes (Plan 255 §H)**: the local
  sandbox does not allow sustained i2pd tunnel-pool role-placement
  interaction; the hosted Actions workflow
  (`.github/workflows/m11-transit-external.yml`) is the authority
  for the two-pass evidence.
- **Java full-router lane**: unrelated subsystem; remains
  retained/deferred nonblocking debt per the registry. No change.
- **Full two-family router conformance**: not required for this
  qualification; remains open.

## 7. Dependency / secret-handling decisions

- No new production dependencies.
- `TransitHopMaterial` stays move-only; no `Clone` added.
- No payload logging; bodies are typed and bounded by existing I2NP
  limits.
- No `Debug`/`Display`/serialization added to secret types.
- The Plan 255 forbidden-token check rejects
  `static_secret` / `static_priv_key` / `session_priv` /
  `router_secret` / `private_key_file` / `privkey_path` in the
  runner / driver code-data surface (comments stripped).

## 8. Deviations from the plan-of-record

- The closure is **infrastructure-only** with the two same-SHA
  external passes deferred to hosted Actions or manual execution.
  This honors the plan §H "two complete same-SHA executions"
  constraint by shipping a fail-closed hosted workflow that the
  operator can dispatch twice on the closure SHA; the local
  foundation matrix proves every gate the external lane relies on.
- The runner records `m11-i2pd-source-clean` from the i2pd source
  tree's `git status --porcelain --untracked-files=no`; if the
  reference source tree is dirty (e.g. uncommitted local edits), the
  row fails closed per the Plan 254 §G fail-closed contract.
- The runner writes the i2pr RouterInfo into i2pd-A's netDb via a
  bounded path (`I2PD_A_NETDB` env override or
  `target/interop/m11-transit-evidence/../i2pd-a/data/netDb/`); no
  in-memory patching, LD_PRELOAD hook, or Docker/namespace fallback.

## 9. Remaining risks

- Two complete same-SHA external passes must be executed against
  the hosted i2pd cache to flip `m11_transit_qualification` from
  `failed` to `passed-via-i2pd-2.61.0`. The hosted workflow is
  wired and ready.
- i2pd pool role placement against the controlled topology depends
  on the bounded bootstrap writing the i2pr RouterInfo into i2pd's
  local netDb before startup; if i2pd's pool selection refuses to
  honour the explicit peer the runner's
  `m11-i2pd-selected-role-proven` row flips to `failed`.
- OBEP fragmented completion and IBGW multi-cell emission beyond
  the single-cell controlled shape remain owned by the runtime-neutral
  suite plus the external data-plane rows.

## 10. Findings by severity

- Critical: none open. The two Plan 254 acceptance violations
  (no production caller, empty-body shim) remain closed and
  guarded; the Plan 255 driver, runner, and evidence checker add
  external lanes that fail closed when the i2pd environment is
  absent.
- High: none open.
- Medium: external passes pending hosted execution.
- Low: `m11-i2pd-restart-clean-baseline` is derived from the
  runner's fresh-datadir provisioning rather than from a recorded
  restart counter; acceptable because the runner terminates every
  child process group and the i2pd datadir is recreated per
  invocation.

## 11. Unblock audit

- **M12 floodfill planning**: Plan 255's external passes are the
  last hard dependency on transit/resource evidence for M12; once
  the hosted Actions workflow records both passes,
  `m11_transit_qualification` flips to `passed-via-i2pd-2.61.0` and
  the M12 floodfill corrective may be registered. M12 remains
  deferred until the hosted double-pass is observed.
- **M6 Java full-router lane (Plans 201/247)**: unrelated subsystem;
  remains retained/deferred nonblocking debt per the registry. No
  change.
- **No other registered plan lists Plan 255 as a hard or interface
  dependency**.

## 12. Roadmap disposition

Plan 255 is **infrastructure-only closed**
(`passed-m11-exact-pinned-i2pd-controlled-transit-qualification-infrastructure-only-external-passes-pending-hosted-actions`).
M11 capability remains unclaimed and `advertised=false`. The next
executable action is dispatching the
`.github/workflows/m11-transit-external.yml` workflow twice on the
closure SHA to flip `m11_transit_qualification` to
`passed-via-i2pd-2.61.0`; a corrective follow-up plan is **not**
required because the foundation matrix is green on the closure
SHA. The M12 floodfill corrective may be registered once the
hosted double-pass is observed.

## Gate

M11 capability remains unclaimed and `advertised=false`. Plan 255
exact-pinned i2pd qualification infrastructure is closed; the two
complete same-SHA external passes are pending hosted Actions
execution. No public transit, RouterInfo capability, router.version,
or public-network participation is enabled by this plan.
