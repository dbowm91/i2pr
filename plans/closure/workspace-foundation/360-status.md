# Plan 360 — `i2pr run` must actually start the router: status

Status: **passed-run-starts-binds-configured-listener-and-shuts-down-cleanly**

Plan of record:
[`360-run-lifecycle-readiness.md`](../../implementation/workspace-foundation/360-run-lifecycle-readiness.md).
Origin: the 2026-10-05 verified defect recorded in `AGENTS.md` and
`docs/architecture/i2pr-daemon.md`, both of which said closing it "needs a plan-of-record".

## The headline

**`i2pr run` works.** Verified live, not only by test:

```text
$ i2pr run --config cfg.toml
... i2pr_daemon::sam: SAM v3.1 loopback listener bound address="127.0.0.1:17656"
$ # from another shell:
HELLO REPLY RESULT=OK VERSION=3.1
```

Before this plan the same command exited `ReadinessTimeout` with **no listener opened**. The
`AGENTS.md` "known limitation" paragraph and the README section describing that failure are now
removed, because both were false the moment this landed.

## The scope was larger than the recorded defect

The recorded defect blamed `lifecycle` alone. Reading the supervisor showed that was only the
first of a chain:

`i2pr_runtime::Supervisor::start` waits per service via `wait_for_initial_ready`
(`crates/i2pr-runtime/src/supervisor.rs:380`), which returns only when the watch receiver flips
(`supervisor.rs:675-677`). That flip comes from `ServiceContext::signal_ready()` →
`Readiness::signal_ready()` (`context.rs:80`), which `run_attempt` selects on alongside the service
future (`supervisor.rs:1158-1185`).

Two consequences the plan of record did not anticipate:

1. **A readiness timeout is fatal for *any* classification** (`supervisor.rs:736-739` →
   `SupervisorError::StartupFailed`), and the supervisor walks services in `startup_order()`.
   Fixing only `lifecycle` would have moved the 30-second timeout to the next never-ready service
   and produced the same user-visible failure with a different name.
2. **Audit of all nine registered services found `lifecycle` and `netdb-bootstrap` never signalled
   at all, and no listener service signalled either.** Only the SSU2 service signalled, and it did
   so correctly.

A second, latent defect surfaced from the same reading: **SAM, I2CP, and I2PControl wrapped
`state.run()` — bind *plus* serve-forever — in a one-second `bounded_timeout`.** Even after
readiness was fixed, a healthy listener would have been killed at t=1s. The console service already
had the correct split (bounded bind, unbounded serve, `src/lib.rs:503-506`); that shape was applied
to the other three.

## Readiness shape chosen: (b), and the contract it commits to

Readiness means **"this service is running"** — never "some task finished", never "we hope".
`readiness_expectation()` in `crates/i2pr-daemon/src/lib.rs` is the single source of truth; it is
installed as each `ServiceSpec::description` and quoted verbatim in the startup diagnostic, so the
operator can see what "ready" was supposed to have meant.

| Service | Classification | Signals readiness when |
|---|---|---|
| `lifecycle` | Essential | immediately — it owns no work; a cancellation-scoped anchor is up when scheduled |
| `netdb-bootstrap` | Essential | after its bounded pipeline returns, success or classified failure |
| `sam-bridge`, `i2cp-bridge`, `i2pcontrol-bridge`, `console` | Optional | **after the bind succeeds**, then serve unbounded |
| `ssu2-listener` | Optional | already correct; owner-tracked |
| `addressbook-refresh`, `news-fetch` | Optional | once the cadence loop exists, **not** on a remote fetch |

**No `i2pr-runtime` change was needed** — `git diff --stat crates/i2pr-runtime/` is empty. The
contract is expressible in the daemon through the existing `signal_ready()` plus the already-public
`bind()`/`serve()` seams.

## Requirement → evidence

| Requirement | Evidence | Result |
|---|---|---|
| In-scope 1: truthful readiness contract | `lib.rs` `readiness_expectation()` / `service_spec()` / `signal_running()` / `bind_then_serve()`; table above | met |
| In-scope 2: readiness failure names service/deadline/reason | `describe_startup_failure()`; negative row | met |
| In-scope 3: black-box proof a listener is bound | `tests/run_lifecycle_readiness.rs`, 4 rows, CLI-only | met |
| Required evidence: listener observably bound | live: `HELLO REPLY RESULT=OK VERSION=3.1` on `127.0.0.1:17656` | met |
| Required evidence: clean termination, no orphaned listener | row `run_binds_configured_listener_and_shuts_down_cleanly`; live SIGINT | met |
| Required evidence: negative row, diagnostic not bare `ReadinessTimeout` | row `occupied_loopback_port_is_refused_with_a_named_diagnostic` | met |
| Invariant: no listener default changed | `git diff` touches no `Config` default; every listener stays loopback + disabled-by-default | met |
| AC5 (no default/capability/advertisement widened) | no `specs/` change, no advertisement change | met |
| AC6 (docs no longer describe the defect) | `AGENTS.md`, `README.md`, `docs/architecture/i2pr-daemon.md` all rewritten | met |
| **AC7 (exact-head routine CI green)** | **no CI reachable from this environment** | **UNPROVEN** |

### The negative row's actual diagnostic

```text
error: supervisor terminated: supervisor failed: service `sam-bridge` failed during startup:
sam-bridge bind failed: failed to bind SAM listener on 127.0.0.1:17657: Address already in use
(os error 98) (startup deadline 30s, readiness deadline 30s; readiness means loopback SAM listener
bound and accepting)
```

Compare the recorded defect: `... service lifecycle failed during startup: ReadinessTimeout`.

## Commands (all local; no CI available in this environment)

```text
cargo fmt --all --check                                                              PASS
cargo check --locked -p i2pr-daemon --all-targets                                    PASS
cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1 4 passed, 1.97s
cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings       PASS
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-daemon --no-deps                PASS
cargo test --locked -p i2pr-daemon --lib -- --test-threads=1                          613 passed
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1                  exit 0, ~90 binaries
bash scripts/check-dependency-direction.sh                                           PASS
bash scripts/check-runtime-boundaries.sh                                             PASS
bash scripts/check-console-boundaries.sh                                             PASS
bash scripts/check-console-browser-security.sh                                       PASS
```

Plus the **combined 49-step routine floor** run once on the combined tree covering Plans
360/361/362/364/365/366 — see "Verification batching" below.

### Row-level mutation testing (7/7)

| # | Mutation | Rows killed |
|---|---|---|
| 1 | Remove the `lifecycle` readiness signal (the original defect) | 3 |
| 2 | Signal readiness **before** the bind (the "we hope" shape) | 2 |
| 3 | Generic diagnostic → bare `ReadinessTimeout` | 1 |
| 4 | Drop only the deadline phrase | 1 |
| 5 | Readiness-timeout arm loses its explanation | 1 |
| 6 | Remove the `netdb-bootstrap` readiness signal | 3 |
| 7 | Re-wrap serving in the 1s bind deadline (the latent pre-fix bug) | 1 |

**Two mutations initially survived and both forced test additions rather than accepted gaps:**

- **#5 survived** because nothing exercised the `ReadinessTimeout` message. Added the unit test
  `readiness_timeout_diagnostic_explains_the_missing_signal`; it now fails under mutation 5.
- **#7 survived** because the test probed the listener too fast to observe it die at the 1-second
  deadline. Added a 1.5s hold-open plus a "must stay up after binding" assertion; it now fails under
  mutation 7.

Both first runs are recorded because they are the evidence that the gap existed.

## Findings by severity

| ID | Sev | Finding | Disposition |
|---|---|---|---|
| P1 | **critical** | `i2pr run` opened no listener at all | fixed |
| P2 | **high** | Readiness timeout is fatal for every classification, so a single-service fix would only move the failure | all 9 services audited and signalled |
| P3 | **high** | SAM/I2CP/I2PControl wrapped serve-forever in a 1s deadline — a healthy listener would die at t=1s | split into bounded bind + unbounded serve |
| P4 | medium | `SupervisorError`'s own `Display` still emits a bare `ReadinessTimeout` for other callers | daemon enriches at its own boundary; runtime left untouched deliberately |
| P5 | low | I2PControl's 3-call control-startup prelude is now inlined in `lib.rs` | named drift risk below |
| P6 | info | `addressbook-refresh` now returns an error where it previously returned `RequestedShutdown` when inactive | deliberate; the path is unreachable (registration is gated on `addressbook.is_active()`) |

## Known limitations

1. **AC7 is unproven.** No CI is reachable from this environment. The local floor is green; that is
   not the criterion as written. Recorded as unproven, not claimed.
2. **I2PControl control-startup duplication is a drift risk.** The same three calls are now invoked
   from `lib.rs` rather than inside `I2pControlServiceState::run`, because readiness must sit
   between bind and serve and that file was not owned here. If its ordering changes, `lib.rs` must
   follow. A comment ties them together. Highest-value follow-on cleanup.
3. **`SupervisorError`'s base `Display` is still terse.** Any future runtime consumer outside the
   daemon sees `service X failed during startup: ReadinessTimeout` without the enriched text.
4. **Timed-out workers stay alive** if the supervisor restarts them — pre-existing, out of scope.
5. **Verification was batched.** See below.

## Verification batching — stated plainly

Plans 360, 361, 362, 364, 365, and 366 were executed together. Targeted verification ran per plan
as each landed (every command in the table above is real and was run for this plan specifically).
The **full `AGENTS.md` routine floor ran once on the combined tree**, not six times. No per-plan
floor run is claimed.

## Roadmap disposition

`plans/subsystems/workspace-foundation-roadmap.md` gains Plan 360 as passed. The consequence worth
recording: `plans/subsystems/router-console-roadmap.md` states the console "has no product-reachable
path, because `i2pr run` does not open any listener". **That statement is now false** — the console
service is registered and binds when `[console]` is enabled. What remains true is that the console is
still disabled by default, loopback-only, non-advertised, and requires explicit configuration. The
console roadmap's wording is corrected by Plan 358's own line rather than by this plan.

No capability, version, RouterInfo, or support-surface claim is promoted. Nothing on the network is
advertised, and no default listener was enabled.