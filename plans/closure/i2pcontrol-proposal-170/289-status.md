# Plan 289 status — TunnelManager control state and existing service-runtime adapter

Status: **`passed-prop170-tunnelmanager-control-state-and-existing-service-adapter`**.

Plan of record: [`289-tunnelmanager-control-state-and-existing-service-adapter.md`](../../implementation/i2pcontrol-proposal-170/289-tunnelmanager-control-state-and-existing-service-adapter.md).

Hard dependencies closed: Plan 287 (`passed-prop170-secure-base-i2pcontrol-jsonrpc-auth-tls`); M10 service-tunnel authority (Plan 215) reused, not replaced.

## Implementation commit

- `d4c4692` — `plan(289): TunnelManager control state and existing service-runtime adapter`
  (closed seven-action envelope, durable generation store, transaction
  coordinator over the one M10 manager for six families, 7-option
  real-effect subset, provenance enforcement, supervisor snapshots,
  startup restore, disabled-mode isolation, drain-cancel runtime fix,
  contract/unit/wire tests, `i2pr-i2pcontrol`/`i2pr-daemon` doc updates,
  `support.toml` surface).

This closure commit (closure record + registry + roadmap) lands on top
of `d4c4692` with no production-code change.

## Requirement-to-evidence matrix

| Plan 289 requirement | Evidence |
|---|---|
| Closed seven-action envelope, no invented rename action | `crates/i2pr-i2pcontrol/src/tunnel_request.rs`: `decode_tunnel_request` (action required exact `TunnelAction` spelling; `name` required except inventory `get`; `type` required-for-create/forbidden-otherwise; `new_name` edit-only; `options` create/edit-only; unknown top-level keys rejected; `edit` requires `new_name` or options); `plan289_tunnel_request_envelope_rules` contract test |
| Option keys bound to the frozen 46-option universe | `find_option` rejects unknown keys, `CaseMismatch` preserved; scalar-only values (string/integer/boolean normalized, null/array/object rejected); `MAX_OPTIONS_PER_TUNNEL`/`MAX_OPTION_NAME_LEN`/`MAX_OPTION_VALUE_LEN` enforced; `truncated_key` bounds diagnostics at 128 chars |
| StartupOwned vs ControlOwned provenance, collision fail-closed | `TunnelProvenance::{StartupOwned, ControlOwned}`; `definition`/`startup_name` lookups; `NameCollision`/`StartupMutationRejected` on cross-class collision or control mutation of startup names; `plan289_collision_and_startup_protection` + `tunnel_collision_and_startup_rejected_over_wire` |
| Control state never rewrites router.toml or ordinary service files | `CONTROL_STATE_SUBDIR="i2pcontrol"` + `CONTROL_TUNNELS_SUBDIR="tunnels"` state root; `ControlStore` writes `staged-<id>.json`/`generation-<id>.json`/`current.json` only; no router-config write path |
| Durable generation store with schema, ordering, ceilings, perms, atomic publish, recovery, escape rejection, retention | `CONTROL_SCHEMA_VERSION=1`; deterministic `BTreeMap` definition order; `MAX_GENERATION_BYTES=1048576` enforced on write and read; owner-only permissions where supported; temp write + sync + atomic rename + directory sync; `load` prefers pointer then newest-valid scan fallback (staged files invisible); symlink/special-file/path-escape rejection; retention keeps current + prior only; `plan289_store_round_trip_and_recovery`, `plan289_store_rejects_symlinks_and_oversized_reads`, `plan289_store_enforces_ceilings_and_retention`, `plan289_store_orphan_cleanup` |
| Secrets never reach storage, responses, errors, logs, or `Debug` | `SUPPORTED_289_OPTIONS` contains no secret-classified key; `build_control_spec` rejects secret/unsupported keys before storage; `ControlDefinition` `Debug` prints keys only; `static_control_reason`/`ControlError` carry static reasons; `plan289_unsupported_options_rejected_before_storage` + `tunnel_unsupported_and_secret_rejected_over_wire` |
| Validate-mirror-stage-reconcile-publish-verify transaction; reconcile-back on publish failure; success means durable intent and runtime agree | `commit_locked` (validate candidate → mirror `candidate_set` → stage → `reconcile` under `CONTROL_DRAIN_DEADLINE=ZERO` → publish pointer+rename → sync supervisors → verify generation agreement); publish failure reconciles back to prior committed generation under `CONTROL_ROLLBACK_DEADLINE=5s`; `plan289_publish_failure_reconciles_back`, `plan289_create_validates_before_side_effects` |
| Start/stop/restart are runtime actions over the exact control-owned definition; StartOnLoad is persisted intent only | `start`/`stop`/`restart` resolve the stored definition (no option payload accepted); `get` reports running state separately from `start_on_load`; `plan289_start_on_load_vs_running_truthfulness`, `plan289_failed_start_reports_failed_status` |
| Six-family typed mapping; unsupported types resource-free | `map_tunnel_type`: client→GenericClient, server→GenericServer, httpclient→HttpClient, socks→Socks5Client, ircclient→IrcClient, ircserver→IrcServer; all other six types `UnsupportedType` before allocation/listener/destination/task; `plan289_family_mapping_six_backends` |
| 7-option subset, each with a real effect; everything else explicitly unsupported | `SUPPORTED_289_OPTIONS` (target_destination/target_host/target_port/listen_host/listen_port/start_on_load/max_streams); server targets require both halves, loopback-only, `unix:` rejected; client `listen_host` loopback default `127.0.0.1`, `listen_port` `0..=65535` default `0`; `max_streams` → per-service ceiling `1..=128` default `16`; `plan289_option_subset_has_real_effect` |
| Per-name supervisor snapshot (status, generation, server destination, bind metadata, counters) | `get` returns control detail (provenance, type, options, start_on_load, running, status, generation, listener/bind, server destination, counters) plus startup summaries; `plan289_get_inventory_lists_both_classes`, `tunnel_lifecycle_over_wire` |
| Startup: load newest, recover prior, collide-detect before start, start only StartOnLoad+supported, per-definition isolation, stable server identity | `TunnelControlState::startup` (cleanup orphans → load → collision check → start eligible → per-name failure isolation); `plan289_server_identity_stable_across_restart` + `tunnel_server_identity_stable_over_wire` + `tunnel_restart_recovery_over_wire` |
| Disabled mode preserves state on disk without reading/reconciling/starting it | `for_config` returns unavailable control when I2PControl disabled; `tunnel_disabled_mode_preserves_state` proves files untouched and no runtime effect |
| One M10 runtime only; drained runtimes release promptly | `ServiceRuntime::cancellation_token` + drain-cancel branches in client/server accept loops and the resolve-fail park; committed-generation spec lookup (no construction-config rewrite); profile loops in http/socks5/irc-client/irc-server observe the token |
| Concurrency: same-name mutations serialize; different-name mutations stay bounded and generation-consistent | `plan289_concurrent_same_name_serializes_deterministically`, `plan289_concurrent_different_names_stay_bounded_and_consistent` |
| Dispatch integrated with auth/batch/ceilings unchanged | `process_tunnel_manager` behind `Authenticate`; `set_control_manager`/`new_with_inspection` wiring; `run()` starts control before bind; `lib.rs` `for_config` install; envelope rejections over wire incl. inside batches (`tunnel_envelope_rejections_over_wire`) |
| No M12/mainline behavior change | Additive only (see Migration); full workspace floor green; Plan 288 inspection/base suites intact |

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK
cargo test --locked --workspace --all-targets -- --test-threads=1          3330 passed, 0 failed
  (delta over Plan 288 floor 3304: +18 control unit, +7 control wire, +1 envelope contract)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 failed
cargo test --locked -p i2pr-daemon --lib i2pcontrol                      57 passed (39 Plan-288 + 18 Plan-289)
cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels              7 passed
  (lifecycle, server_identity, unsupported/secret, collision/startup, envelope, restart_recovery, disabled_mode)
cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection           9 passed (unchanged behaviors intact)
cargo test --locked -p i2pr-daemon --test i2pcontrol_base                10 passed (unchanged behaviors intact)
cargo test --locked -p i2pr-i2pcontrol --test contract plan289            1 passed (envelope rules)
bash scripts/check-dependency-direction.sh                                 dependency direction: ok
bash scripts/check-runtime-boundaries.sh                                   runtime boundary checks passed
bash scripts/check-service-tunnel-boundaries.sh                            service-tunnel boundary checks passed
bash scripts/check-ntcp2-interoperability.sh                               OK
bash scripts/check-constrained-host-lane-boundary.sh                      OK
bash scripts/check-m11-transit-boundaries.sh                               passed
bash scripts/check-m11-transit-qualification-evidence.sh                   175 guarded rows green
bash scripts/check-sam-acceptance-evidence.sh                              22 rows green
bash scripts/check-ssu2-acceptance-evidence.sh                             15 rows green
bash scripts/check-i2cp-acceptance-evidence.sh                             24 rows green
bash scripts/check-exploratory-tunnel-evidence.sh                          12 labels green
bash scripts/check-netdb-tunnel-evidence.sh                                12 labels green
bash scripts/check-destination-tunnel-evidence.sh                          21 labels green
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                  11 labels green
python3 -m unittest discover -s tests/integration/ntcp2/harness            OK (18 tests)
cargo deny check advisories bans sources                                   advisories ok, bans ok, sources ok
```

Not run locally (pre-existing macOS environment limit, identical on
unmodified `main`): `check-fixture-manifest.sh`,
`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`,
`check-service-tunnel-acceptance-evidence.sh`,
`check-streaming-tunnel-evidence.sh` require bash 4+ associative arrays
(`declare -A`); this host ships bash 3.2.57 only. None of their areas
are touched by this plan beyond the service-tunnel drain-cancel path
(covered by the workspace floor and the service-tunnel boundary
checker above); Linux CI is authoritative for those six.

## Acceptance criteria

Plan 289 closes when the seven lifecycle actions are real over one M10
`ServiceTunnelManager` for the six existing families, durable state is
recoverable, ownership provenance is enforced, and unsupported Proposal
families fail before resource allocation. All four hold with executed
evidence above. Plans 290 and 291 become ready in parallel after
closure (see Unblock audit).

## Defects found during implementation (all corrected)

- Drained control runtimes held their listeners: the supervisor task
  owns a runtime clone, so manager-side removal never unblocked the
  accept loop. Fixed with per-service `cancellation_token` observed
  in client/server accept loops and the resolve-fail park
  (`service_tunnels.rs`, plus http/socks5/irc-client/irc-server
  loops).
- Control-reconciled specs went stale under a construction-config-only
  lookup: `reconcile` publishes new generations without rewriting the
  construction config. Fixed with committed-generation-first spec
  resolution, construction config as fallback.
- Clippy `-D warnings` (17 findings, all corrected, no behavior
  change): doc lazy-continuation in the module header, collapsible
  nested `if let` in the pointer-load path, `is_err()` instead of
  `if let Err(_)`, needless `Ok(...?)` in `get`, explicit auto-deref
  in `static_control_reason`, single-element `for` in the Plan 294
  floor test, unused binding in the restart-recovery wire test,
  redundant explicit rustdoc link target in `tunnel_request.rs`.

## Deliberate deviations (recorded, not defects)

- Drain policy is immediate release (`CONTROL_DRAIN_DEADLINE =
  Duration::ZERO`): administrative stop/restart/delete frees listeners
  at once so ports recycle deterministically. This differs
  deliberately from the product grace default, which favors
  connection grace over port reuse; in-flight connections observe
  cancellation promptly.
- Rename travels as `new_name` on `edit` (pinned seven-action
  vocabulary preserved; no invented rename action). Tunnel types are
  immutable after creation (`type` forbidden outside `create`).
- Server targets require both `target_host` and `target_port`; no
  silent default for where tunneled traffic exits. `unix:` hosts are
  rejected as not-yet-supported. Client `listen_host` defaults to
  loopback `127.0.0.1`; `listen_port` defaults to ephemeral `0`.
- `max_streams` maps onto the per-service connection ceiling
  `1..=128` (default `16`).
- `httpclient`/`socks`/`ircclient` profile options use the reviewed
  M10 defaults; profile tuning belongs to Plans 290/292.
- `serde_json::Map` sorts keys lexicographically, not canonical matrix
  order (deterministic either way; frozen by Plan 288 wire tests and
  unchanged here).

## Migration / compatibility

Additive only: two new modules (`tunnel_request.rs`,
`i2pcontrol_tunnels.rs`), two new test files (contract envelope test,
`tunnels` wire suite), one new `control` field on
`I2pControlServiceState` plus `set_control_manager` /
`new_with_inspection`, one new `for_config` install in `lib.rs`, the
drained-runtime prompt-stop path in `service_tunnels*.rs`.
`Authenticate`/batch/TLS/throttle and all Plan 288 inspection
behaviors are unchanged (`i2pcontrol_base` 10 + `i2pcontrol_inspection`
9 intact). No router.toml or service-config file format change; the
control root is a new dedicated directory.

## Security review

- No secret ever reaches storage: the 289 subset contains no
  secret-classified key, `build_control_spec` rejects secret and
  unsupported keys before any side effect, `ControlDefinition`
  `Debug` prints keys only, and error/reason strings are static or
  bounded (unknown keys truncated at 128 chars).
- No second runtime: every lifecycle action reconciles the one
  existing `ServiceTunnelManager`; unsupported types fail before any
  listener/destination/task allocation.
- No owner bypass: NetDB, destination, tunnel-pool, SAM/I2CP, and
  transport internals stay with their owners; control sees only
  bounded snapshots and typed commands.
- Disabled mode is isolated: control state on disk is neither read
  nor reconciled nor started.
- Loopback-default, token/throttle/body/batch ceilings, and
  fail-closed non-loopback TLS from Plan 287 are unchanged.

## Documentation / operational evidence

- `docs/architecture/i2pr-i2pcontrol.md`: `tunnel_request` module row,
  public-surface and test notes, Plan 289 closure cross-reference.
- `docs/architecture/i2pr-daemon.md`: `i2pcontrol.rs` TunnelManager
  dispatch note, new `i2pcontrol_tunnels.rs` row, `service_tunnels.rs`
  drain-cancel + committed-lookup note.
- `specs/support.toml`: new `prop170.tunnelmanager-control-state`
  surface (`experimental`, `advertised = false`).

## Known limitations

- Only 6 of 12 Proposal types have backends; the remaining six
  (`connectclient`, `socksirc`, `httpserver`, `httpbidirserver`,
  `streamrclient`, `streamrserver`) fail explicitly as unsupported
  until Plans 290–291.
- Only the 7-option subset has real effects; every other supplied
  option fails unsupported (never accepted inertly) until Plans
  292–293.
- Profile tuning (SOCKS/HTTP parity) belongs to Plans 290/292.
- Uptime/hash/transport/tunnel/rate residuals from Plan 288 are
  unchanged (Plans 294/295).
- The six bash-4-only checkers remain unverifiable on this macOS host
  (environment debt, unchanged from Plans 286–288).

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the six bash-4-only
  checkers (environment debt); the drained-runtime listener hold
  (found and fixed in the implementation commit, covered by
  lifecycle/restart wire tests).

## Roadmap disposition

Plan 289 is **closed** (`passed-*`). Plans 290 and 291 move to
`ready` in parallel (see Unblock audit). Plan 292 stays blocked on
290 + 291; Plan 293 stays blocked on 292; Plan 294 stays `ready`
(parallel branch); Plan 295 stays blocked on 288 + 293 + 294 (288
now closed, 289 now closed). No other Proposal 170 capability is
claimed.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 290 (hard dep: Plan 289, now closed) — moves to `ready` in
  this commit.
- Plan 291 (hard dep: Plan 289, now closed) — moves to `ready` in
  this commit.
- Plan 292 (hard deps: Plans 290 + 291, both still open) — remains
  blocked.
- Plan 293 (hard dep: Plan 292) — remains blocked.
- Plan 294 (hard dep: Plan 287 only) — already `ready`; unaffected.
- Plan 295 (hard deps: Plans 288 + 293 + 294) — remains blocked
  (288 + 289 now closed; 293 + 294 still open).
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 289 acceptance criterion
passes with executed evidence above.
