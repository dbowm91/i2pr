# Plan 371 — optional, non-blocking service startup substrate corrective

Status: **registered-not-started**.

Classification: **runtime supervision substrate + correctness corrective**.

Roadmap:
- `plans/subsystems/managed-native-app-runtime-roadmap.md` (origin)
- `plans/subsystems/runtime-supervision-roadmap.md` if present

Originating plan:
- Plan 369 (`plans/implementation/managed-native-app-runtime/369-trusted-application-runtime-manager-and-apphost-lifecycle-foundation.md`)

Hard dependency:
- none. This is a self-contained `i2pr-runtime` change.

## Objective

Give the runtime supervisor a way to register a service that is genuinely
**optional at startup**, so that a service which never signals initial
readiness degrades its own feature instead of aborting router startup.

Today no such thing exists. `Supervisor::run` iterates `startup_order()` and
awaits every service's initial readiness; a service that never signals it ends
the whole graph with `SupervisorError::StartupFailed`, **regardless of its
`ServiceClassification`**.

## The defect

In `crates/i2pr-runtime/src/supervisor.rs`, the startup loop is:

```rust
for name in self.graph.startup_order() {
    ...
    match self.wait_for_initial_ready(name, spec.startup_timeout(), ...).await {
        Ok(()) => {}
        Err(completion) => {
            let report = self.shutdown(...).await;
            self.state.set_lifecycle(RouterLifecycle::Failed);
            return Err(SupervisorError::StartupFailed { ... });
        }
    }
}
```

There is no branch on classification. `RestartExhaustion::Degrade` is honoured
only in the steady-state handler (`ServiceClassification::Restartable` arm),
which is reached only **after** `self.state.set_lifecycle(RouterLifecycle::Ready)`.

`wait_for_initial_ready` has exactly one non-fatal escape: a service that had
already reached ready and is `Degradable`/`Optional` may exit, and that is
treated as success. A service that never becomes ready at all has no path.

### Why this matters beyond one service

Plan 369 invariant 1 is "Router stays functional if app runtime is disabled or
broken", and Plan 369 §5 requires exhaustion to "degrade/disable the app-runtime
feature rather than shutting down the router". Both are unimplementable today.
Any future optional subsystem — a metrics exporter, a UI bridge, an external
tool integration — inherits the same trap: opting in means the router dies if
the optional thing is broken.

The current escape hatch is worse than it looks: a developer who wants an
optional feature simply never signals readiness, which makes the router fail to
start with no explanation. That is a footgun the classification enum appears to
promise it does not have.

## Required changes

1. **A startup-disposition policy on `ServiceSpec`**, distinct from
   `ServiceClassification` (which describes *runtime* failure behaviour).
   Something like `StartupRequirement::{Required, Optional}`:
   - `Required` (default) — today's behaviour, unchanged;
   - `Optional` — on startup failure the service is recorded as degraded, the
     graph continues, and `snapshot.ready` stays `false` **only** if the service
     was ever required by readiness. Readiness semantics must be stated
     explicitly, not inferred.
2. **Honour `RestartExhaustion::Degrade` during startup**, not only afterwards.
   A `Restartable` service with `Degrade` that exhausts its budget before
   becoming ready should degrade, matching its steady-state behaviour.
3. **Fail closed on ambiguity.** A service that is `Optional` at startup but
   `Essential` in classification is contradictory and must be rejected by graph
   validation rather than silently resolved.
4. **Graph validation**, symmetric with the existing `Restartable` ⟺
   restart-policy rule, so the new field cannot be omitted silently.

## Why Plan 369 does not fix this itself

Plan 369's own Stop conditions include:

> existing supervisor semantics cannot own/restart the manager without a new
> generic process-supervision substrate.

This change is generic `i2pr-runtime` behaviour affecting every registered
service. Folding it into Plan 369 would hide a supervisor-wide semantic change
inside an application-runtime plan and would give it a single-purpose test.

## Required tests

- an `Optional` service that never signals readiness does **not** abort startup;
- an `Optional` service that fails is visible as degraded in
  `SupervisorSnapshot`, and router `ready` reports the operator-usable truth;
- a `Required` (default) service that never signals readiness still produces
  `StartupFailed` — **no regression** of the existing all-or-nothing contract;
- `Degrade` exhaustion during startup matches the same exhaustion after startup;
- the contradictory `Optional` + `Essential` combination is refused at graph
  build;
- the new field defaults to `Required`, so **every existing service keeps its
  current behaviour** — asserted by running the existing supervisor suites
  unchanged.

## Negative evidence required

The guard is behavioural, not textual, so the mutation test is: flip one
existing service to `Optional` and observe the router survive a readiness
timeout that previously killed it; then flip it back.

## Acceptance criteria

Plan 371 passes only when:

1. an optional service can fail startup without failing the router;
2. no existing service changes behaviour without an explicit opt-in;
3. the contradiction case is rejected at graph build;
4. Plan 369's `app-runtime` service is switched to the non-blocking policy and
   `a_manager_that_sends_the_wrong_magic_never_becomes_ready` is **replaced**
   with the degradation assertion Plan 369 §5 requires;
5. the full routine floor passes.

## Out of scope

- any change to restart policy arithmetic or backoff timing;
- changing `ServiceClassification` semantics for running services;
- adding, renaming, or removing any supervisor health vocabulary that existing
  consumers depend on;
- anything about the managed-app runtime itself.

## Stop conditions

Stop and register a further plan if:

- making startup non-blocking cannot be expressed without changing what
  `snapshot.ready` means for existing consumers;
- the supervisor's shutdown path cannot drain an optional service's children
  without regressing the "no owned task may outlive shutdown" invariant.