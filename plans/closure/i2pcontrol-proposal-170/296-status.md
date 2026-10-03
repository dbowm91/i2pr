# Plan 296 status — Tunnel pool shaping residuals and reply-bundling primitive

Status: **`passed-prop170-pool-shaping-and-bundling-residuals`**.

Plan of record: [`296-tunnel-pool-shaping-and-bundling-residuals.md`](../../implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md).

Hard dependency closed: Plan 292 (`passed-prop170-tunnel-option-matrix-and-noncrypto-completion`)
with the exact `CorrectivePending{plan: 296}` residual set (39 cells).

## Implementation commits

Branch `plan-296-pool-shaping-bundling`, on top of `a9f1b43`
(Plan 295 closure):

- `2b2656f` — pool shaping owners (standby, variance sampler,
  spec fields, generation taxonomy).
- `131457d` — garlic reply-bundling delivery primitive
  (`bundle.rs`, multi-route dispatch, `deliver_batched` with
  shared drive/drain pieces).
- `593eb3d` — daemon seams (batched bridge, sweep grouping,
  multihoming dial selection) with trajectory, sweep-policy, and
  live product tests.
- `943b83d` — control boundary (supported list, parse/resolve
  arms, oversum contradiction), matrix flip to Apply, spec-13
  authority update, support surface, arch deep-dives,
  rejection-pin conversion, FIFO corrective, clippy-clean.

This closure commit (closure record + registry + roadmap) lands on
top with no production-code change.

## Outcome

All 39 Plan 292 residual cells are consumed by real owners; the
matrix leaves no `CorrectivePending{plan: 296}` cell:

- `APPLY_CELLS = 266` (was 227), `NOT_APPLICABLE_CELLS = 37`,
  `INCOMPATIBLE_CELLS = 30`, `CORRECTIVE_296_CELLS = 0`,
  `CORRECTIVE_297_CELLS = 3` (server `use_ssl`, owned by Plan 297).

## Requirement-to-evidence matrix

| Plan 296 requirement | Evidence |
|---|---|
| Backup quantity with standby semantics in the pool owner | `TunnelShaping.backup_quantity` (0..=3) → `DestinationConfig` effective targets (pool maximums) with usability on the base target; `DestinationTunnelPool` standby counts, promotion counting on failure/expiry loss, registration bounded at the effective target; per-direction pool-maximum sum check (never clamped) |
| Length variance with deterministic test hooks | `TunnelShaping.length_variance` (−2..=+2) → `DestinationConfig::sampled_build_length` pure sampler (injected sample byte, pool hop-policy clamp, sign-is-radius); distribution test over all 256 sample bytes; no ambient RNG in the sampler |
| Multihoming conflict resolved explicitly | Documented target-selection semantic (no session reply-info flag introduced): round-robin server dial with sequential failover inside the overall connect deadline, two-target minimum, server kinds only, Unix targets rejected; `multihoming_start_index` rotation contract; live rotation (A,B,A,B), failover (refused→next), first-target default, and fail-safe fallback tests |
| Reply-bundling primitive in the delivery owner | `ReplyBundling` policy + `BundleError` + multi-data-clove reply encoder + bundled-reply composer sharing the single path's clove bytes and Garlic carrier; dispatcher routes every data clove with atomic all-or-nothing admission; outbound sweep bundles consecutive same-remote runs with per-index reports and no-double-send fallback |
| Per-cell positive and negative tests | Shaping bounds/oversum, multihoming kind/target/Unix gates, bundling kind-free apply, standby promotion/usability/threshold/bound, sampler distribution/clamp, bundle encode/compose/gating, dispatch multi-route/cap, deliver_batched bundled/fallback paths, sweep grouping/policy, live dial paths |
| Matrix re-evaluation with named owners | Options 12/13 → `TunnelShaping backup/variance into DestinationConfig standby and sampler`; 34 → `ServiceTunnelSpec.multihoming into server dial target selection`; 35 → `DestinationConfig reply_bundling into bundled-reply delivery`; census tests pin 266/37/30/0/3 |
| No regression of Plans 289–295 paths | Full serial workspace floor green (130 suites), all acceptance/boundary checkers green, FIFO corrective covered below |

## Acceptance criteria disposition (§Acceptance criteria, plan of record)

1. Every residual cell consumed by a real owner — yes (39 → Apply
   with the owners above; `CORRECTIVE_296_CELLS = 0`
   machine-checked).
2. No `CorrectivePending{plan: 296}` cell remains — yes
   (tunnel-matrix census + final-matrix census green).
3. Routine floor (including the 292 boundary census tests) green
   on the closing commit — yes (see below; six bash-4 scripts
   remain Linux-CI-authoritative on this bash-3.2 host, as at
   Plans 294/295).

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK (normalized)
cargo check --locked --workspace --all-targets                             OK (also clean on MSRV 1.88.0)
cargo test --locked --workspace --all-targets -- --test-threads=1          130 suites green, exit 0
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 failed
cargo test --locked -p i2pr-client --all-targets                          231 passed, 0 failed
cargo test --locked -p i2pr-client --test plan296_trajectory               2 passed
cargo test --locked -p i2pr-daemon --lib                                   374 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_multihoming_product  4 passed
cargo test --locked -p i2pr-i2pcontrol --all-targets                       contract+matrix+final green
bash scripts/check-dependency-direction.sh                                 dependency direction: ok
bash scripts/check-runtime-boundaries.sh                                   runtime boundary checks passed
bash scripts/check-service-tunnel-boundaries.sh                            service-tunnel boundary checks passed
bash scripts/check-ntcp2-interoperability.sh                               OK
bash scripts/check-constrained-host-lane-boundary.sh                       OK
bash scripts/check-m11-transit-boundaries.sh                               passed
bash scripts/check-m11-transit-qualification-evidence.sh                   175 guarded rows green
bash scripts/check-sam-acceptance-evidence.sh                              22 rows green
bash scripts/check-ssu2-acceptance-evidence.sh                             15 rows green
bash scripts/check-i2cp-acceptance-evidence.sh                             24 rows green
bash scripts/check-i2pcontrol-acceptance-evidence.sh                       ok
bash scripts/check-exploratory-tunnel-evidence.sh                          12 labels green
bash scripts/check-netdb-tunnel-evidence.sh                                12 labels green
bash scripts/check-destination-tunnel-evidence.sh                          21 labels green
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                   11 labels green
python3 -m unittest discover -s tests/integration/ntcp2/harness            OK (18 tests)
cargo deny check advisories bans sources                                   advisories ok, bans ok, sources ok
```

Not run locally (pre-existing macOS environment limit, identical
on unmodified `main`): `check-fixture-manifest.sh`,
`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`,
`check-service-tunnel-acceptance-evidence.sh` (bash 3.2.57
parse-rejects line 1952; file untouched since Plan 215),
`check-streaming-tunnel-evidence.sh` require bash 4+ on this
host's bash 3.2.57. Linux CI is authoritative for those six.

## New dependencies (reviewed)

None. No new crate, no new external dependency, no
`Cargo.toml`/`Cargo.lock` change. `cargo deny`
(advisories/bans/sources) is clean.

## Security review

- No new sockets, tasks, files, channels, or listeners. The new
  client module (`bundle.rs`) is pure assembly with bounded
  counts/bytes and typed errors.
- Failover dial stays inside 127/8 (loopback-only invariant
  holds on every attempt); Unix targets are rejected with the
  flag; no ambient trust or key material anywhere in this plan.
- Bundled replies are same-remote only (mixed groups fall back to
  singles); sealed-but-undeliverable bundles report failure
  without retry (no duplicate application bytes); inbound
  admission is all-or-nothing with pre-count caps and queue
  ceilings (no amplification, no partial effects).
- Multihoming rotation state is a per-runtime monotonic counter
  (no RNG, deterministic order); legacy specs never advance it.
- Evidence carries no secrets; all four keys are
  public-classified (values echo in GET output like every Plan
  292 public key).

No high/medium/low findings beyond the noted environment debt.

## Failure / migration review

- No persisted-state shape change: the control generation files
  carry the same options-map schema (new keys flow as ordinary
  entries); no migration exists or is needed.
- Standby/metrics/counters are memory-only and re-derive at
  composition; restarts lose only the trailing window.
- The dispatcher FIFO corrective (below) changes pop order for
  back-to-back queued payloads from newest-first to oldest-first,
  matching the documented discipline and `BoundedPayloadQueue`;
  single-payload behavior is identical and Streaming/datagram
  consumers tolerate order (sequence numbers; queue order).
- Multihoming edits and bundling-flag edits are `MutableInPlace`
  (per-connection/per-sweep committed reads); shaping edits
  replace the destination runtime through the existing drain path.
- Whole-request validation and disabled-isolation semantics are
  unchanged from Plans 287–295.

## Corrective found during execution

The dispatcher-owned inbound application queue popped newest-first
(`Vec::pop`) while its documentation promises oldest-first and
claims `BoundedPayloadQueue` semantics (which is FIFO). The
bundled-reply trajectory test exposed it (second clove surfaced
first). Fixed to FIFO (`VecDeque` + `pop_front`); the Plan 127
`malformed_remote_does_not_poison_valid_session` trajectory now
drains the handshake-era payload first and asserts both payloads
in order. Single-payload behavior is unchanged.

## Documentation / operational evidence

- `specs/protocols/13-tunnel-option-matrix.md`: census 266/37/30/0/3,
  standby/variance semantics, target-selection resolution,
  reply-bundling section, edit classes.
- `specs/support.toml`: new `prop170.pool-shaping-and-bundling-residuals`
  surface (`experimental`, `advertised = false`); prior plan rows
  untouched.
- `docs/architecture/i2pr-i2pcontrol.md`,
  `docs/architecture/i2pr-client.md`,
  `docs/architecture/i2pr-daemon.md`: matrix counts, new owners,
  dial/sweep behavior rows.
- Plan 295/293/292 closure records and the Plan 295 dossier keep
  their historical counts (no rewrite).

## Known limitations

- `multihoming=true` through I2PControl always meets the
  two-target minimum as a named contradiction: the control surface
  carries a singular target and no multi-target wire key exists
  (none may be added in this plan). The dial-selection owner
  serves multi-target specs built through other surfaces
  (daemon TOML `targets` lists, programmatic specs).
- The variance sampler is the build path's contract; no live
  build loop reads it yet (same depth as the Plan 292 length
  cell, which likewise has no live reader — the exploratory
  destination build path is a future consumer).
- Bundling applies to the local bridge delivery path; the
  Plan 208 remote route sends singly (documented).
- I2CP backup/variance keys remain noted-and-dropped (I2CP is a
  separate API surface, out of scope); I2CP has no bundling or
  multihoming keys.

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the
  dispatcher LIFO-vs-documented-FIFO defect (fixed with
  regression coverage); the six bash-4-only checkers
  (environment debt, unchanged since Plan 286).

## Roadmap disposition

Plan 296 is **closed** (`passed-*`). Plan 297 stays `ready`
(unaffected — its hard dep is Plan 292, closed; this plan adds
no new dependency). No other Proposal 170 capability is claimed.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 296 (hard dep: Plan 292, closed) — moves to `closed` in
  this commit.
- Plan 297 (hard dep: Plan 292, closed) — already `ready`;
  unaffected (its 3 residual cells are untouched by this plan).
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 296 acceptance
criterion passes with executed evidence above. The deferred
items (live variance consumer, remote-route bundling, second
control target key) are explicit documented boundaries, not
hidden gaps.
