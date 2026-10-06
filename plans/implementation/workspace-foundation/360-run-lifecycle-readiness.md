# Plan 360 — `i2pr run` must actually start the router

Status: **registered-verified-product-path-defect-router-never-starts**

Classification: **defect corrective**. Not a capability plan, not polish. The product's primary
entry point does not work today.

Origin: verified 2026-10-05 and recorded in `docs/architecture/i2pr-daemon.md` §"CLI and
configuration" and in `AGENTS.md`. That entry said closing it "needs a plan-of-record"; this is it.

Subsystem: `workspace-foundation` — the supervisor, service lifecycle, and cancellation owner
(`plans/implementation/workspace-foundation/021-m2-supervision-cancellation.md`). The defect is in
the composition root's use of that machinery, not in Proposal 170, the console, or any protocol.

## The defect

`i2pr run` exits with `ReadinessTimeout` and **opens no listener at all**.

```text
error: supervisor terminated: supervisor failed: service lifecycle failed during startup: ReadinessTimeout
```

`run_daemon` registers `lifecycle` as an **Essential** service before the **Optional**
`sam-bridge`. Both the `lifecycle` and `netdb-bootstrap` service bodies are
`cancellation.cancelled().await` — they await cancellation forever and therefore **never report
initial readiness**. `i2pr_runtime::Supervisor` starts services in `startup_order()` and waits for
each one's `wait_for_initial_ready`, so the 30-second readiness timeout on `lifecycle` fires first,
and because `lifecycle` is Essential the supervisor tears down the whole daemon before `sam-bridge`
is ever started.

What works today: `check-config`, `identity generate`, `identity inspect`, `run --dry-run`, and the
`sam_loopback_listener` example. What does not: the real thing.

## Why ready

- **No hard dependency.** The defect is reproducible on the current tree. Nothing needs to land
  first.
- **No interface dependency.** The supervisor contract (`i2pr_runtime::Supervisor`,
  `wait_for_initial_ready`, Essential/Optional semantics) already exists and is not changing shape.
- **No new dependency, no protocol change, no advertisement change.** This is startup ordering and
  readiness signalling inside the daemon composition root.

## Objective

`cargo run -p i2pr-daemon -- run --config <cfg>` starts, binds its configured listeners, and stays up
until signalled. It must not exit `ReadinessTimeout`. And it must not exit successfully having bound
nothing — a router that reports ready without a listener is a worse failure than the current one.

## In scope

1. **Decide and implement the readiness contract for `lifecycle`.** Two defensible shapes; pick one
   and justify it:
   - **(a) Signal readiness on real completion of the bootstrap pipeline.** `lifecycle` reports
     initial readiness once the work it owns is observably done. Most faithful to the service's
     meaning, but couples readiness to NetDB bootstrap success, which needs a bounded, non-fatal
     failure mode when the network is unreachable — a router with no NetDB is still useful.
   - **(b) Stop gating optional services behind a never-ready Essential service.** `lifecycle` is a
     long-running no-op that is ready as soon as it is running; the bootstrap work moves to a
     service whose own readiness reflects its own progress.

   **(b) is the default** unless (a) can be made honest. A readiness signal must mean "this service
   is up", not "some unrelated async task finished". A `lifecycle` service that awaits cancellation
   *is* up the moment it starts, so reporting readiness immediately is truthful; the current code is
   simply missing that one call.
2. **Make the readiness failure mode diagnostic.** If readiness genuinely cannot be established, the
   operator must be told which service, which deadline, and what it was waiting on — not just
   `ReadinessTimeout`.
3. **Prove it end-to-end.** A black-box test that runs the composition root, observes a listener
   bound on loopback, and shuts down cleanly. No listener ⇒ failure.

## Out of scope

- **Enabling anything beyond what the config already gates.** SAM/I2CP/I2PControl/service-tunnels/
  console stay loopback-only, disabled by default, non-advertised. Fixing startup must not change a
  single default.
- **The Plan 342 live failover / live restart gap.** Separate; still unproven.
- **Making the router interoperate with a deployed network.** Bootstrap resilience to an unreachable
  reseed source is adjacent, not this plan.
- **`RouterInfo` publication, transit, floodfill.** Untouched.
- **Any new network listener.** The listeners already exist in the graph; this plan makes them
  reachable.

## Invariants

- **No default changes.** Every listener stays disabled-by-default and loopback-only.
- **Readiness means "this service is running",** never "some task finished" and never "we hope".
- **A service that cannot become ready must fail loudly and name itself,** not time out anonymously.
- **Cancellation still tears the daemon down promptly,** and channel/socket close stays a lifecycle
  event, not a blind retry.
- **No unbounded channel or queue is introduced.**
- **No new dependency, no wire change, no capability/version/RouterInfo advertisement.**

## Required evidence

- `i2pr run` reaches a running state and a loopback listener is observably bound, driven through
  the CLI only — not by calling private bootstrap/manager APIs from the test.
- The same command terminates cleanly on cancellation, with no orphaned listener.
- A negative row: a configuration that genuinely cannot start is refused with a diagnostic naming
  the service and the reason, rather than a bare `ReadinessTimeout`.
- No existing daemon test regresses.

## Production changes

`crates/i2pr-daemon/src/lib.rs` (service graph construction / `run_daemon`), and
`crates/i2pr-runtime/src/**` only if the readiness contract genuinely cannot be expressed without a
runtime change. New tests under `crates/i2pr-daemon/tests/`.

If a guard script needs to change it is to **add** an assertion, never to relax one.

## Documentation updates

- `docs/architecture/i2pr-daemon.md` — replace the "Verified defect" paragraph with the fixed
  behaviour and the readiness contract. The current text becomes false the moment this lands.
- `AGENTS.md` — remove the `i2pr run` defect paragraph and the `sam_loopback_listener` workaround.
- `README.md` — if it claims the router starts.

## Acceptance criteria

Plan 360 passes only when:

1. `i2pr run` starts, binds its configured listener, and stays up, proven by an executed test driven
   through the CLI;
2. the readiness contract chosen is documented and matches the code;
3. the negative row produces a diagnostic naming the service, not a bare `ReadinessTimeout`;
4. shutdown on cancellation is clean, with no leaked listener;
5. **no listener default, capability, or advertisement changed** — asserted, not asserted-by-prose;
6. `docs/architecture/i2pr-daemon.md` and `AGENTS.md` no longer describe the defect;
7. exact-head routine CI is green.

## Stop conditions

Stop and record a classified boundary if a truthful readiness signal cannot be produced without
changing a listener default, or if the listener the test must observe cannot be bound on loopback in
a test environment.

## Closure evidence required

Commits; a requirement-to-evidence matrix; exact commands with local/CI outcomes labelled truthfully;
the readiness-contract rationale; a security review (no new listener surface, no default widened);
the test transcripts; known limitations; findings by severity; and the roadmap disposition.

**Note on verification batching.** This plan is executed alongside Plans 361, 362, and 364. Targeted
verification runs per plan as its work lands; the full `AGENTS.md` routine floor runs **once** on the
combined tree rather than four times. The closure record must say exactly that, and must not imply a
per-plan floor run occurred.