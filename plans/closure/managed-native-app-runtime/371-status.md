# Plan 371 closure — optional, non-blocking service startup substrate corrective

Status: **passed-optional-non-blocking-service-startup-substrate-corrective**.

Classification: **runtime supervision substrate + correctness corrective**. This
closure records a generic `i2pr-runtime` supervisor change that affects **every**
registered service's startup contract. It ships no capability, no advertisement,
and no managed-application functionality.

Plan: `plans/implementation/managed-native-app-runtime/371-optional-non-blocking-service-startup-corrective.md`.

Originating plan: Plan 369
(`plans/implementation/managed-native-app-runtime/369-trusted-application-runtime-manager-and-apphost-lifecycle-foundation.md`),
WP2.

## The defect, as executed evidence

Before this change, `Supervisor::run` awaited initial readiness for **every**
service in `startup_order()` and returned `SupervisorError::StartupFailed` for
any service that never signalled — **regardless of its `ServiceClassification`**.
`RestartExhaustion::Degrade` was honoured only in the steady-state handler,
which is reachable only *after* `set_lifecycle(RouterLifecycle::Ready)`.

The executed evidence is Plan 369's own supervision test, before and after.

Before, at committed `7b351292`, a manager that was spawned and then rejected
failed **router startup**. `a_manager_that_sends_the_wrong_magic_never_becomes_ready`
asserted `!snapshot.ready` and recorded the gap in its own doc comment.

After:

```text
test a_manager_that_sends_the_wrong_magic_degrades_the_feature_not_the_router ... ok
  // asserts: lifecycle == Ready, snapshot.ready == true,
  //          app-runtime == Degraded(Degraded(LocalPolicy))
```

This directly contradicted Plan 369 invariant 1 ("Router stays functional if app
runtime is disabled or broken") and §5 ("exhaustion degrades rather than
shutting down the router"), which were therefore **unimplementable**.

## What changed

### 1. `i2pr-core`: `StartupRequirement`

A new enum, deliberately **not** a fifth `ServiceClassification`:

- `ServiceClassification` describes how a service behaves **once the router is
  running**.
- `StartupRequirement` describes whether the router **needs it ready before
  startup completes**.

`Required` is `#[default]`. `StartupRequirement::gates_readiness()` is the
predicate the snapshot uses.

`HealthSnapshot` carries the requirement via `with_startup_requirement(..)` — a
builder, not a new `for_service` argument, so an existing caller keeps the
`Required` default instead of silently acquiring optional startup semantics.

### 2. `i2pr-runtime` graph: the policy and its validation

`ServiceSpec::startup_requirement(..)`, defaulting to `Required`. Graph
validation refuses the contradictory combination with the new
`GraphError::ContradictoryStartupRequirement { service, classification }`.

### 3. `i2pr-runtime` supervisor: startup disposition

`Supervisor::degrades_startup_failure` decides, at the single point where
startup can still fail, whether a completion degrades the service or fails the
router. Two independent policies make startup non-fatal, and both are decided at
**registration time**, never inferred from the failure:

1. `StartupRequirement::Optional` — for a service that may be unusable from the
   start (a manager whose handshake never completes, an unresolvable
   executable).
2. `RestartExhaustion::Degrade` on a `Restartable` service — now honoured
   **during startup as well as after**. Before, exhaustion meant one thing during
   startup and another afterwards, so a restartable service that broke in its
   first seconds killed a router that would have survived the identical failure
   a minute later.

### 4. Readiness semantics, stated explicitly

`SupervisorSnapshot::ready` is now computed over services whose
`startup_requirement` gates readiness **and** whose classification is
`Essential` or `Restartable`. `RouterLifecycle::Ready` and `ready` are therefore
consistent: an optional feature that never started no longer leaves a usable
router permanently reported as unready.

Degrading a service and releasing router readiness are **separate decisions**. A
`Restartable` service that degraded still gates `ready` unless it *also* opts
into `StartupRequirement::Optional`. This pre-existing contract is deliberately
preserved and is pinned by a test that asserts `!ready`.

## Six defects found while implementing this plan

Recorded rather than hidden; all three were found by the work this plan did, not
by review.

### D1 — `StartupRequirement::Optional` alone only worked for leaf services

`wait_for_initial_ready` turned any dependency's manager output into
`DependencyUnavailable` for the dependent. So once an optional service degraded,
its dependents still failed startup — meaning `Optional` would have worked only
for services with no dependants, i.e. almost none of the optional subsystems this
is for.

Found by `a_dependent_of_a_degraded_optional_service_still_starts`, which failed
with `StartupFailed { service: dependent, completion: Failed(DependencyUnavailable) }`.

**Fixed** by `observe_startup_peer` plus a `degraded_at_startup` set carried
through startup. A dependency edge constrains start **order**, not
**availability**: once a dependency has explicitly degraded, its dependents are
still given their own attempt. If such a dependent then fails on its own, being
`Required`, it correctly fails startup — the distinction is between "cannot work
at all" and "does not work without this".

### D2 — a degradation would have overwritten itself with `Failed`

`wait_for_initial_ready` stamped the current service's failing completion through
`record_completion` *before* returning `Err`, so a service declared
startup-optional would publish a `Failed` health state (and a `SERVICE_FAILED`
event) immediately before the degrade path corrected it to `Degraded`. Likewise
`observe_startup_peer` re-recording a degraded peer would have overwritten the
typed degradation with a classification-derived `Failed` for a `Restartable`
service.

**Fixed** by making the caller the single disposition decision point:
`wait_for_initial_ready` no longer stamps a current-service failure, and
`observe_startup_peer` skips already-degraded peers.

### D3 — the degradation reset `restart_count` to zero

`record_non_fatal_startup_failure` passed a hard-coded `0`, so a service that had
just exhausted a bounded budget of N attempts reported `restart_count: 0` — the
opposite of what happened to the operator.

**Fixed** by carrying the observed restart count through from health. Caught by
the assertion `restart_count == 2` in
`restart_budget_exhaustion_degrades_during_startup_like_it_does_after`, which
failed with `left: 0, right: 2`.

The pre-existing steady-state `Degrade` arm still passes `0` and is **left
alone**: Plan 371's out-of-scope list forbids changing `ServiceClassification`
semantics for running services. Recorded as known limitation 1 below.

### D4 — a peer exiting cleared another service's readiness gate

**This is the most serious of the six, and it was introduced by this plan.**

When `wait_for_initial_ready` observes a *peer* service's manager output, it
originally fell through and kept waiting. Collapsing that arm into
`return self.observe_startup_peer(..)` made an observed peer look like a
successful wait, so a service could clear its readiness gate **without ever
signalling readiness** — the Plan 360 defect class, reintroduced through the
Plan 371 peer-observation path. It was caught by
`a_benign_peer_exit_does_not_satisfy_another_services_readiness_gate`, and
`clippy::never_loop` independently flagged the same arm as structurally
impossible.

**Fixed** by giving `observe_startup_peer` an explicit
`Option<ServiceCompletion>` result — `Some` means *abort*, `None` means *keep
waiting* — and restoring the load-bearing inner join loop on the
readiness-sender-dropped path, where a peer's output can be ordered ahead of the
current service's.

### D5 — a startup degradation was re-processed by the steady-state handler

A manager's output can legitimately arrive *after* startup: the readiness gate
may be satisfied by the readiness signal while the failing output is still
queued behind it in the `JoinSet`. The steady-state handler then treated that
late output as a fresh post-startup failure and, for a `Degradable` service,
called `mark_dependents_degraded` — **cancelling dependents that startup had
deliberately allowed to run**.

This surfaced as a 1-in-6 flaky failure of
`a_dependent_of_a_degraded_optional_service_still_starts` (`dependent` observed
as `Stopped`/`Stopping`), found by running the suite repeatedly rather than once.

**Fixed** by dropping the service from `degraded_at_startup` when its output
finally arrives and skipping the steady-state classification entirely: its
disposition was already decided and published by `degrades_startup_failure`.
Re-applying it would report the same failure twice and undo startup's decision.

The flake is now covered deterministically by
`a_startup_degraded_service_is_not_reprocessed_by_the_steady_state_handler`,
which forces the ordering with a child that ignores cancellation for five
seconds rather than relying on scheduler timing.

### D6 — clippy caught two structural problems in the change itself

`wait_for_initial_ready` exceeded the workspace argument bound once the
`degraded_at_startup` set was added, and the sender-dropped branch became a
provable `never_loop` once every arm started returning. Both fixed structurally —
a `StartupSession<'_>` bundling the startup bookkeeping, and removal of the dead
loop — rather than by suppression.

## Negative evidence

Every new guard was removed in isolation and the targeted test observed to fail.
Seven mutations, each reverted.

| # | Mutation | Target test | Result |
| --- | --- | --- | --- |
| M1 | Delete the `ContradictoryStartupRequirement` validation from `graph.rs` | `a_startup_optional_service_may_not_also_be_essential` | **FAILED** |
| M2 | Drop `gates_readiness()` from the `ready` filter in `observability.rs` | `a_startup_optional_restartable_service_does_not_gate_readiness` | **FAILED** — `ready: false` |
| M3 | Force the `StartupRequirement::Optional` branch of `degrades_startup_failure` to `false` | `a_startup_optional_service_that_never_signals_ready_does_not_abort_startup` | **FAILED** |
| M3b | same mutation | `a_dependent_of_a_degraded_optional_service_still_starts` | **FAILED** |
| M4 | Remove the `Restartable` + `RestartBudgetExhausted` + `Degrade` branch | `restart_budget_exhaustion_degrades_during_startup_like_it_does_after` | **FAILED** |
| M5 | Remove the `degraded_at_startup` exemption in `observe_startup_peer` | `a_dependent_of_a_degraded_optional_service_still_starts` | **FAILED** |
| M6a | Flip the **real** `lifecycle` service to `StartupRequirement::Optional` | real `i2pr_daemon::build_daemon_graph` | **REFUSED**: `invalid service graph: service lifecycle cannot be startup-optional and Essential` |
| M6b | Drop the `StartupRequirement::Optional` opt-in from the **real** `app-runtime` service | `a_manager_that_sends_the_wrong_magic_degrades_the_feature_not_the_router` | **FAILED** — router startup aborted |
| M7 | Treat an observed benign peer as a successful wait (reintroduces D4) | `a_benign_peer_exit_does_not_satisfy_another_services_readiness_gate` | **FAILED** |
| M8 | Disable the steady-state skip for startup-degraded services (reintroduces D5) | `a_startup_degraded_service_is_not_reprocessed_by_the_steady_state_handler` | **FAILED**, 3/3 |

M1–M8 including M3b are **nine mutations**. Each was reverted; the tree compiles
clean and the suite passes.

M6a and M6b are the mutation the plan itself names, run against real production
composition rather than a synthetic harness. M6b is the end-to-end demonstration:
with the opt-in removed the router aborts startup; with it present the router
starts and degrades only the app runtime.

### Two coverage gaps the negative testing exposed

Recorded because both would otherwise have shipped as false confidence.

1. **M2 initially passed.** The first "never ready" test used a `Degradable`
   service, which `ready` already excludes by classification, so it could not
   detect the mutation. The load-bearing case is `Restartable`, so
   `a_startup_optional_restartable_service_does_not_gate_readiness` was added and
   M2 re-run against it.
2. **M3 initially passed against the restartable test.** With the `Optional`
   branch negated, the `Restartable`+`Degrade` arm still degraded the service, so
   that test does not isolate the `Optional` branch. The branch *is* covered — by
   M3 and M3b — but no single test owns it alone. Stated rather than papered over.
3. **M8 initially passed because the guard had no deterministic test.** D5 first
   appeared as a 1-in-6 flake rather than a clean failure, and negating the
   steady-state skip at that point changed nothing observable in a single run. A
   test that forces the ordering was written before the mutation was re-run; it
   now fails 3/3 under negation.

## Tests added

`crates/i2pr-runtime/src/graph.rs` (3):

- `a_startup_optional_service_may_not_also_be_essential` — the contradiction is
  refused at graph build.
- `startup_requirement_defaults_to_required_for_every_classification` — the
  `Required` default for all four classifications, which is the executable form
  of "no existing service changes behaviour without an explicit opt-in".
- `optional_startup_is_accepted_for_every_non_essential_classification`.

`crates/i2pr-runtime/src/supervisor.rs` (9):

- `a_startup_optional_service_that_never_signals_ready_does_not_abort_startup` —
  the headline contract, plus a clean reap of the degraded service's children
  (the plan's own stop condition).
- `a_startup_optional_service_is_drained_without_leaking_owned_tasks`.
- `a_required_service_that_never_signals_ready_still_fails_startup` — regression
  guard on the all-or-nothing default.
- `a_dependent_of_a_degraded_optional_service_still_starts` — D1.
- `restart_budget_exhaustion_degrades_during_startup_like_it_does_after` — plus
  the `!ready` assertion pinning the preserved pre-existing contract.
- `restart_budget_exhaustion_with_shutdown_policy_still_fails_startup` — proves
  this plan did not turn every broken restartable service into a silent
  degradation.
- `a_startup_optional_restartable_service_does_not_gate_readiness`.
- `a_benign_peer_exit_does_not_satisfy_another_services_readiness_gate` — D4.
- `a_startup_degraded_service_is_not_reprocessed_by_the_steady_state_handler` —
  D5, with the ordering forced rather than raced.

`crates/i2pr-daemon/tests/app_runtime_supervision.rs` (1, replacing 1):

- `a_manager_that_sends_the_wrong_magic_never_becomes_ready` was **replaced** by
  `a_manager_that_sends_the_wrong_magic_degrades_the_feature_not_the_router`, as
  Plan 371 acceptance criterion 4 requires. It asserts the properties that
  actually matter and are deliberately *not* the negation of the old ones: the
  app runtime is `Degraded(LocalPolicy)` and never `Ready`, the router reaches
  `Ready` with `ready == true`, the essential `lifecycle` service is still ready,
  and every manager child spawned across the exhausted budget is reaped with zero
  cleanup failures.

## Plan acceptance criteria

| # | Criterion | Disposition |
| --- | --- | --- |
| 1 | An optional service can fail startup without failing the router | met — `a_startup_optional_service_that_never_signals_ready_does_not_abort_startup` |
| 2 | No existing service changes behaviour without an explicit opt-in | met — `Required` is `#[default]`; `startup_requirement_defaults_to_required_for_every_classification`; the entire pre-existing supervisor suite passes unchanged |
| 3 | The contradiction case is rejected at graph build | met — `a_startup_optional_service_may_not_also_be_essential`, negative-tested (M1), and proven on real production composition (M6a) |
| 4 | Plan 369's `app-runtime` is switched to the non-blocking policy and the wrong-magic assertion is **replaced** | met — `register_app_runtime_service` sets `StartupRequirement::Optional`; the test was replaced, not weakened |
| 5 | The full routine floor passes | met — see "Routine floor" |

## Why Plan 369 did not fix this itself

Plan 369's own Stop conditions include "existing supervisor semantics cannot
own/restart the manager without a new generic process-supervision substrate".
This change is generic `i2pr-runtime` behaviour affecting every registered
service. Folding it into Plan 369 would have hidden a supervisor-wide semantic
change inside an application-runtime plan and given it a single-purpose test.

## Security review

- **No new authority.** The change adds a startup-disposition enum and one
  read-only field on a health snapshot. It grants no capability, opens no
  listener, spawns no process, and creates no configuration surface. There is no
  key, token, or secret in the changed surface.
- **Fails closed on ambiguity.** `Optional` + `Essential` is refused at graph
  build rather than silently resolved, and it is refused *on real production
  composition* (M6a), not only in a unit test. A developer cannot opt a required
  service out of startup by accident.
- **Degradation is visible, not silent.** A service that degrades at startup is
  stamped `Degraded`/`LocalPolicy` with its failure category, keeps its
  completion in `ShutdownReport::completions()`, and appears in
  `SupervisorSnapshot` and in the per-service `HealthReceiver`. Nothing about a
  broken optional subsystem is hidden from an operator.
- **No task may outlive shutdown.**
  `a_startup_optional_service_is_drained_without_leaking_owned_tasks` asserts
  `remaining_tasks == 0`, `remaining_child_tasks == 0` and `cleanup_failures == 0`
  for an optional service that owns a cooperative child and never becomes ready.
  The plan's stop condition about draining an optional service's children is
  therefore tested, not assumed.
- **Bounded.** Every path is bounded by an existing timeout; the plan adds no
  unbounded wait and no new queue.
- **The opt-in is not a silencing switch for required work.** A dependent that
  fails on its own after its dependency degraded still fails startup if it is
  `Required`. `Optional` relaxes *startup blocking*, never the requirement that a
  service function.

### Concurrency, cancellation, restart

No new concurrency. `degraded_at_startup` is a `BTreeSet` owned by the single
`run` future and is never shared across tasks, so it needs no locking.
Cancellation semantics are unchanged: the degradation path does **not** cancel
the failing service's dependents — none are spawned yet, `startup_order` being
dependency-first — and the failing service's own manager task is still owned by
the `JoinSet` and drained by `shutdown`. Restart arithmetic and backoff timing
are untouched, per the plan's out-of-scope list.

## Documentation

- `docs/architecture/i2pr-runtime.md` — new "Startup requirement (Plan 371)"
  section stating the two policies, the degrade-vs-release distinction, the
  dependency edge semantics, and the `ready` rule; new
  `ContradictoryStartupRequirement` error entry; updated module and test tables.
- `docs/architecture/i2pr-core.md` — `StartupRequirement` documented as
  deliberately orthogonal to `ServiceClassification`; `HealthSnapshot` builder and
  accessor recorded; consumer count corrected.
- `crates/i2pr-daemon/src/lib.rs` — `register_app_runtime_service` documents why
  the service carries **both** a `Degrade` restart policy and an `Optional`
  startup requirement, and why they are different guarantees.
- The Plan 369 composition preflight comment was **corrected forward**: its
  stated justification ("the supervisor aborts startup for any service") became
  false with this change. It is retained because its remaining justification is
  stronger — a *present but broken* manager should degrade, while an operator who
  enabled a feature without installing its manager should get an actionable
  configuration error rather than a silently degraded router.

No plan state, status token, or ledger was copied outside `plans/`.

## Known limitations

1. **The steady-state `Degrade` arm still reports `restart_count: 0`** when a
   restartable service exhausts its budget after startup. The startup path was
   corrected (D3); the steady-state path is unchanged because Plan 371's
   out-of-scope list forbids changing `ServiceClassification` semantics for
   running services. A follow-up corrective owns it.
2. **No single test isolates the `StartupRequirement::Optional` branch.** Two
   tests each fail if it is removed, but neither fails alone against a
   `Restartable` service, because the `Degrade` arm also degrades it. Recorded
   rather than engineered into a contrived fixture.
3. **`StartupRequirement` is orthogonal to classification by design, which means
   three of the four combinations are legal.** Only `Optional` + `Essential` is
   rejected. A `Restartable` + `Optional` + `RestartExhaustion::Shutdown`
   combination would degrade at startup but shut the router down later; that is
   coherent but surprising, and no test pins it.
4. **`observe_startup_peer` exempts a degraded dependency even when the dependent
   is `Required`.** The dependent then fails on its own terms, which is correct,
   but the *failure message* names the dependent rather than the dependency that
   caused it. Acceptable for now: the degraded dependency is visible in the
   snapshot with its own failure category.

## Findings by severity

- **Critical:** none.
- **High:** none outstanding.
- **Medium:** none outstanding.
- **Low (recorded, not this plan's scope):**
  - steady-state `Degrade` reports `restart_count: 0` (known limitation 1);
  - the combinations in known limitation 3 are unpinned;
  - carried forward from Plan 369/370 and unchanged here:
    `check-dependency-direction.sh` fails open for a new workspace member and
    ignores dev/build dependencies (live instance: `i2pr-addressbook`);
    `check-managed-app-manager-boundary.py` has no `--self-test`;
    `control_scoped` is not unrepresentable at the app-proto layer.

## Routine floor

The **complete `AGENTS.md` routine floor was executed locally and every step
passed** — **50 of 50 steps, 0 failures**:

| Step group | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo check --locked --workspace --all-targets` | pass |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | pass — **4496 passed, 0 failed, 35 ignored** |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | pass |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | pass |
| `cargo test --locked --workspace --doc` | pass |
| All 44 boundary/evidence/guard checkers | pass |
| `cargo deny check advisories bans sources` | pass |

The 44 checkers are every `scripts/check-*.{sh,py}` in the `AGENTS.md` floor,
including the two `--self-test` variants of the M12 floodfill checkers and the
planning unittest suite. `check-global-plan-number-uniqueness.py` also
**caught a real error in this closure's own drafting**: the record was first
written to `plans/closure/runtime-supervision/`, a subsystem that does not own
Plan 371, and the checker refused it. The record belongs to
`plans/closure/managed-native-app-runtime/`.

Host: Linux 6.8.0-142-generic x86_64, GNU bash 5.2.21, rustc/cargo 1.95.0, run
with `--offline --locked`. **None of these steps were run on CI.** The seven
bash-4+ checkers that `AGENTS.md` records as unrunnable under macOS/bash 3.2 —
including `check-fixture-manifest.sh`, the three `check-*-vectors.sh`,
`check-streaming-tunnel-evidence.sh`, `check-java-source-lock-gating.sh`, and
`check-service-tunnel-acceptance-evidence.sh` — all executed and passed here,
because this host has bash 5.

The floor was driven by a temporary reporting wrapper so that a per-step outcome
could be recorded rather than stopping at the first failure. The wrapper is a
verification artifact and is **not** committed.

The 35 ignored tests are the repository's pre-existing environment-gated external
interop lanes (i2pd / Java I2P / go-i2cp). They are gated by design and were not
enabled; this change touches none of them.

### Operational note

The workspace test step took **1137 s** on this host. The machine was under a
load average of 30–75 from unrelated work throughout; that is why the step count
and outcome, not its wall time, are the evidence here.

## Unblock audit — Plan 369

Per `plans/README.md`, at every closure the registry's blocked work and the
affected roadmap dependency graphs are audited.

**Audited:** Plan 369, which was `in-progress-…-blocked-on-plan-371`.

- Hard dependency: Plan 345 — closed.
- Hard dependency: Plan 368 — closed.
- Hard dependency: Plan 370 — closed; its WP4 gate remains lifted.
- Stop condition "existing supervisor semantics cannot own/restart the manager
  without a new generic process-supervision substrate" — **now satisfied** by
  this plan.
- Plan 369 invariant 1 ("router stays functional if app runtime is disabled or
  broken") and §5 ("exhaustion degrades rather than shutting down the router") —
  **were unimplementable and are now implemented and evidenced** end-to-end in
  `a_manager_that_sends_the_wrong_magic_degrades_the_feature_not_the_router`.

**Result: Plan 369 WP3, WP4, WP5, and WP6 are UNBLOCKED.**

**Audited:** Plan 371 itself — closed by this record. No corrective pass is
required.

**Newly unblocked beyond Plan 369.** Any future optional subsystem — a metrics
exporter, a UI bridge, an external tool integration — now has a supported way to
opt in that is neither "die with the router" nor "silently pretend to work". The
footgun the plan identified, where the only available escape hatch was never
signalling readiness and thereby failing startup with no explanation, no longer
exists.

## Roadmap disposition

**Closed.** Corrective plan, all five acceptance criteria met with executed
evidence including seven negative mutations. The defect was corrected forward;
Plan 369's own record is **not** rewritten — its WP2 outcome section documents
the stop condition as it stood, which is what happened.

Plan 369 resumes with **WP3 (apphost exec and bounded lifecycle, `Secured`
fail-closed), WP4 (app v1 consumer), WP5 (fixture native app and black-box
SAM/I2CP qualification), and WP6 (process-boundary checker, docs, closure
record)** all actionable. WP1 and WP2 remain landed and valid.