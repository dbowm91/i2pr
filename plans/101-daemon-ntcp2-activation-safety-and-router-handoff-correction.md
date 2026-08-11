# Plan 101: daemon NTCP2 activation-safety correction and router-buildout handoff

## Status and authority

- Status: planned; active narrow daemon-safety corrective pass.
- Date: 2026-08-11.
- Baseline: `11b8989235ae34de754a379841771c17eca7f36e` or a clean descendant that has not already corrected the defects below.
- Parent authority: Plan 099, Plan 100, ADR 0023, and the production daemon-composition work that landed after Plan 100.
- Development interoperability result: `protocol-defect-localized` at `noise_authenticated` from Plan 099 workflow run `31521642090`.
- NTCP2 support state: experimental, non-advertised, not qualified for normal daemon activation.
- Plan 079 remains deferred to the pre-normal-NTCP2-activation / pre-public-network checkpoint.
- Plan 101 is **not** an NTCP2 interoperability plan and must not dispatch, modify, extend, or replace the Plan 099 development-interoperability workflow.
- This plan exists because the first production daemon-composition commit after Plan 100 accidentally crossed the activation boundary that Plan 100 explicitly kept closed.

## Executive finding

Plan 100 achieved its strategic exit from the interoperability apparatus and produced a real post-TCP development result:

```text
development_interop = protocol-defect-localized
highest_stage        = noise_authenticated
ntcp2                 = experimental-non-advertised
external_netdb_over_ntcp2 = blocked
```

The correct next product direction was production router construction. That work began at commit:

```text
11b8989235ae34de754a379841771c17eca7f36e
```

and usefully replaced the old `RuntimeNotImplemented` daemon shell with a real Tokio composition root, identity load, supervisor, service graph, and shutdown path.

However, the same commit introduced an activation regression:

1. `default_ntcp2_enabled()` returns `true`.
2. `run_daemon()` does not consult `config.transport.ntcp2.enabled` before constructing and registering `Ntcp2RuntimeService`.
3. the daemon always registers `ntcp2-transport` as an essential service and calls `service.listen(...)`.
4. therefore ordinary `i2pr run` can bind an NTCP2 listener even though independent interoperability is currently localized and Plan 100 requires normal-daemon NTCP2 to remain disabled.
5. `scripts/check-ntcp2-interoperability.sh` claims to prevent accidental production activation, but its current grep-based predicate does not detect this call path.

This is a **production composition/policy bug**, not a reason to reopen the Plan 095-100 interoperability sequence.

## Objective

Correct the production activation boundary while preserving the useful daemon-composition progress:

```text
current daemon composition
        |
        v
retain real i2pr run / identity / supervisor / shutdown
        |
        v
remove NTCP2 from normal service graph while interop is localized
        |
        v
prove ordinary daemon startup opens no NTCP2 listener
        |
        v
make activation guard behavioral rather than grep-shaped
        |
        v
reconcile Plan 100 closure/status deviation once
        |
        v
continue RouterInfo + local NetDB construction
```

Plan 101 must **not** revert the production composition root to `RuntimeNotImplemented`. The router should continue becoming a real long-lived Rust process; only the unqualified transport activation is removed.

## Current authoritative constraints

Until a later activation checkpoint explicitly changes authority:

```text
ntcp2_status                 = experimental
ntcp2_advertised             = false
ntcp2_normal_daemon          = disabled
ntcp2_public_listener        = forbidden
external_netdb_over_ntcp2    = blocked
plan079                      = deferred
production_daemon            = allowed
local_routerinfo             = allowed
local_netdb                  = allowed
offline_su3                  = allowed
```

A configuration flag must not be able to bypass this authority merely because the parser accepts the field.

## Defects to correct

### D1 — unsafe default

`crates/i2pr-daemon/src/config.rs` currently defines NTCP2 as enabled by default.

Required correction:

```text
default_ntcp2_enabled = false
```

But changing the default alone is **not sufficient**, because an explicit `enabled = true` would still activate an unqualified transport.

### D2 — `enabled` is ignored by composition

`run_daemon()` currently constructs `Ntcp2RuntimeService`, registers `ntcp2-transport`, and binds the listener regardless of the normalized `enabled` value.

Required correction: normal production daemon composition must not construct/register/bind the NTCP2 service under current authority.

### D3 — configuration can imply unsupported activation

The `[transport.ntcp2].enabled` setting exists, but current project authority does not permit normal-daemon activation.

Preferred behavior for this phase:

- default is `false`;
- explicit `enabled = false` is accepted;
- explicit `enabled = true` is rejected during config normalization with a clear semantic error indicating that normal-daemon NTCP2 activation is not available while support remains experimental/unqualified.

Do not silently accept `enabled = true` and ignore it. Silent ignore creates configuration ambiguity and makes later activation migrations harder to reason about.

### D4 — static checker does not enforce its own stated invariant

`scripts/check-ntcp2-interoperability.sh` says production NTCP2 activation is forbidden, but it only looks for public `listen`/`dial` function declarations. It does not catch calls to `Ntcp2RuntimeService::listen()` inside another function.

The durable owner for this invariant should be functional behavior/configuration tests, with the static checker reduced to a small secondary boundary assertion.

### D5 — Plan 100 closure overstates procedural compliance

Plan 100's core technical result is valid, but the execution exceeded its stated pre-TCP correction budget and did not perform the post-TCP unchanged reproduction exactly as written.

Required documentation treatment:

- preserve `development_interop = protocol-defect-localized`;
- preserve the retained run/artifact identity;
- explicitly record that Plan 100's correction-count/reproduction procedure was not followed exactly;
- do **not** reopen Plan 100 or create another interop execution requirement to repair this historical process deviation;
- do **not** change the localized transport result into a stronger pass/fail claim.

### D6 — stale active status blocks

The top active-correction blocks in Plans 087/088 still describe Plan 100 as awaiting a live run even though Plan 100 is closed.

Required correction: active headers must point to the final Plan 099/100 result and to Plan 101 as the daemon-safety correction. Historical evidence below remains untouched.

### D7 — top-level project status is stale

The README currently describes a non-networked daemon shell and says no retained real mixed-router NTCP2 result exists. Both are stale after Plan 100 and the daemon-composition commit.

Update only the current project-status paragraph necessary to state:

- a real daemon composition root now exists;
- NTCP2 development interop is localized at `noise_authenticated`, not passed;
- normal-daemon NTCP2 activation remains disabled after Plan 101;
- RouterInfo/NetDB construction is the active product direction.

Do not rewrite the long historical milestone narrative.

## Hard scope lock

Expected implementation surfaces:

```text
crates/i2pr-daemon/src/config.rs
crates/i2pr-daemon/src/lib.rs
crates/i2pr-daemon/tests/cli.rs
possibly crates/i2pr-daemon/src/error.rs only if an existing semantic error is insufficient
scripts/check-ntcp2-interoperability.sh
plans/099-status.md
plans/087-status.md
plans/088-status.md
README.md
```

Other files may change only when a directly owned compile/test correction requires them.

Do not touch:

- NTCP2 Noise/handshake/frame cryptography;
- the Plan 099 GitHub Actions workflow;
- i2pd reference-driver sources;
- Plan 099 exit-gate classifier;
- Plan 083/084 runner behavior;
- Plan 079 execution matrix;
- SSU2;
- public reseed/bootstrap;
- RouterInfo advertisement of NTCP2;
- public network behavior.

## Non-goals

Plan 101 does not:

- rerun independent NTCP2 interoperability;
- create a new interoperability plan, topology, schema, runner, or evidence artifact;
- fix the localized `noise_authenticated` cross-side question;
- enable NTCP2 behind an opt-in flag;
- advertise NTCP2;
- implement peer acceptance over NTCP2;
- implement external NetDB exchange;
- implement RouterInfo publication itself;
- implement NetDB storage itself;
- revert daemon composition to a dry-run-only shell;
- add another plan-number-specific Python test;
- add new CI workflow complexity.

## Work package 1 — preserve the useful daemon composition root

Before editing, confirm the current daemon composition provides these useful owners:

```text
config load
persistent identity load
Tokio runtime entry
ServiceGraph construction
Supervisor ownership
shutdown signal handling
runtime exit classification
```

These stay.

The correction must remove only the premature NTCP2 production service activation.

## Work package 2 — make NTCP2 activation impossible under current authority

### 2.1 Safe default

Change the normalized default to:

```text
transport.ntcp2.enabled = false
```

Add/update the config test that proves omission of `[transport.ntcp2]` produces `enabled == false`.

### 2.2 Reject explicit enable

During configuration validation, reject:

```toml
[transport.ntcp2]
enabled = true
```

with a stable semantic error. Suggested meaning:

```text
field  = transport.ntcp2.enabled
reason = normal-daemon NTCP2 activation is unavailable while support is experimental
```

Do not expose an undocumented override/environment variable.

### 2.3 Do not construct or register NTCP2 in the service graph

`run_daemon()` must not instantiate `Ntcp2RuntimeService` or register `ntcp2-transport` under the current supported configuration.

Preferred implementation for this phase:

```text
configuration validates enabled == false
run_daemon builds a service graph containing only currently-qualified daemon services
NTCP2 runtime wiring remains in i2pr-runtime for experimental/test use
```

Do not delete the NTCP2 runtime crate/service implementation. It remains useful for tests and the later activation checkpoint.

### 2.4 Keep the daemon alive without NTCP2

The production daemon must still have a valid lifecycle when the service graph contains no network transport service yet.

If the supervisor requires at least one service, add the smallest production-appropriate lifecycle service rather than using NTCP2 as a keepalive. A lifecycle service may own shutdown/readiness only; it must not simulate network functionality.

Do not introduce a generic plugin framework or placeholder-service hierarchy.

## Work package 3 — add behavioral activation regressions

Static grep is insufficient. Add tests that prove behavior.

### 3.1 Config behavior

Required cases:

1. omitted NTCP2 section -> disabled;
2. explicit `enabled = false` -> accepted;
3. explicit `enabled = true` -> rejected;
4. other NTCP2 tuning fields do not make activation possible when disabled;
5. schema unknown-field behavior remains strict.

### 3.2 Daemon service-graph behavior

Refactor only as much as needed to test the graph before running it indefinitely.

Preferred seam:

- a small internal `build_service_graph(&Config, ...)` or equivalent composition helper;
- test that the current graph contains no `ntcp2-transport` service;
- test that no `Ntcp2RuntimeService` is required to construct a valid daemon graph.

Avoid exposing production internals publicly solely for tests.

### 3.3 Socket behavior

Add one bounded local integration test proving ordinary daemon startup does **not** own the configured NTCP2 TCP endpoint.

A robust pattern is:

1. reserve/select a loopback TCP port;
2. create a valid temporary identity/config with NTCP2 disabled;
3. start the daemon under a cancellable/test-owned lifecycle;
4. verify the daemon does not bind the NTCP2 endpoint;
5. verify another test-owned `TcpListener` can bind that endpoint while the daemon is running, or otherwise inspect the service graph before bind if that is more deterministic;
6. terminate the daemon cleanly.

Do not use sleep-heavy polling. Use bounded synchronization/readiness.

If a pure service-graph assertion gives stronger deterministic proof than a socket race, prefer it and add one small process-level smoke only if necessary.

## Work package 4 — strengthen the durable NTCP2 boundary check

The static checker should no longer pretend its grep pattern proves runtime behavior.

Preferred shape:

- keep support-marker assertions (`experimental`, `advertised = false`);
- keep test-only reference-driver boundary;
- assert the daemon's NTCP2 default is false if this can be done robustly;
- assert the functional daemon/config regression tests exist;
- remove or replace the ineffective "public listen function" heuristic.

Do not expand the checker back into a plan-history checker.

Target size remains small and durable.

## Work package 5 — record Plan 100 procedural deviation without reopening it

Update `plans/099-status.md` with a short correction note:

```text
Plan 100 technical closure result remains protocol-defect-localized.
The implementation required more pre-TCP harness corrections than the written Branch C budget and the final post-TCP result was not reproduced unchanged as required by criterion 47.
These are recorded procedural deviations, not grounds to reopen the retired interop sequence.
```

Preserve:

```text
workflow_run_id = 31521642090
source_commit = 51742e3e27fad0ede1ed716cfaafef3fd557ebbd
highest_stage = noise_authenticated
reason = reference-events-missing
development_interop = protocol-defect-localized
```

Do not relabel the result as `passed`, `failed`, or release-qualified.

## Work package 6 — reconcile active planning/status authority

### 6.1 Plan 099 status

Add Plan 101 as the active daemon-safety correction:

```text
plan_099 = closed-protocol-defect-localized
plan_100 = closed-exit-cleanup-with-recorded-procedural-deviation
plan_101 = active-daemon-ntcp2-activation-safety-correction
plan_079 = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
ntcp2 = experimental-non-advertised
normal_daemon_ntcp2 = temporarily-regressed-pending-plan101
router_construction = active
```

After implementation, change only the Plan 101 line and `normal_daemon_ntcp2` to the closed state.

### 6.2 Plans 087/088

Replace only their top active-correction blocks with the final authority:

```text
plan_087 = historical-development-sequence
plan_088 = historical-development-sequence
plan_095 = historical
plan_099 = closed-protocol-defect-localized
plan_100 = closed
plan_101 = active daemon-safety correction
plan_079 = deferred
```

Do not rewrite their historical bodies.

### 6.3 Plan 079

No implementation changes are required unless its top active status has drifted. It remains deferred.

## Work package 7 — correct the README current status only

Update the top current-status section so a new contributor does not infer either of these false states:

```text
daemon = still non-networked RuntimeNotImplemented shell
interop = no authentic post-TCP result exists
```

The replacement should state concisely:

```text
production daemon composition root = landed
NTCP2 development interop = localized at noise_authenticated
normal-daemon NTCP2 = disabled after Plan 101
NTCP2 advertised = false
active router work = RouterInfo + local NetDB foundation
```

Do not rewrite the historical Plan 031-100 narrative in this pass.

## Work package 8 — validation

Required local validation:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Also run the specific daemon test target directly so failures are obvious, for example:

```bash
cargo test --locked -p i2pr-daemon
```

No Plan 099 workflow dispatch is part of Plan 101.

No Java/i2pd build is required.

No Python interop suite is required unless an implementation accidentally touches that surface, which should be treated as a scope warning.

## Work package 9 — completion and immediate router handoff

Plan 101 closes when the production daemon is real but transport-neutral under current authority:

```text
i2pr run                    = real supervised daemon
identity load               = active
shutdown                    = active
ntcp2 normal daemon         = disabled and unenableable
ntcp2 advertised            = false
interop result              = protocol-defect-localized
external netdb over ntcp2   = blocked
routerinfo local work       = next/active
netdb local work            = next/active
```

After Plan 101, do not create another transport-safety cleanup plan unless a later router-composition change violates this boundary again.

The next substantial implementation roadmap should proceed into actual router functionality:

```text
local RouterInfo publication owner
    -> signed immutable RouterInfo snapshot lifecycle
    -> validated local NetDB record store
    -> expiry / replacement / quotas
    -> persistence + restart revalidation
    -> deterministic DatabaseStore / Lookup / SearchReply state machines
    -> offline/local SU3 ingestion
```

The existing NTCP2 runtime remains available for experimental/test use and for the later Plan 079 activation checkpoint.

## Explicit acceptance criteria

Plan 101 is complete only when all applicable criteria below hold.

### Activation safety

1. `default_ntcp2_enabled()` returns false or the equivalent normalized default is false.
2. Omitting `[transport.ntcp2]` results in `enabled == false`.
3. Explicit `enabled = false` is accepted.
4. Explicit `enabled = true` is rejected under current authority.
5. The rejection is a stable config semantic error, not a panic or silent ignore.
6. No environment variable or undocumented flag bypasses the rejection.
7. `run_daemon()` does not construct `Ntcp2RuntimeService` under the supported configuration.
8. `run_daemon()` does not register an `ntcp2-transport` service.
9. `run_daemon()` does not call NTCP2 `listen`, `dial`, or `connect` paths.
10. Ordinary daemon startup does not bind the configured NTCP2 endpoint.
11. NTCP2 runtime/test code is retained rather than deleted.
12. `specs/support.toml` still records NTCP2 experimental and `advertised = false`.

### Daemon composition preservation

13. `i2pr run` remains a real non-dry-run daemon path; `RuntimeNotImplemented` is not restored.
14. Persistent router identity load remains part of startup.
15. The Tokio supervisor/service graph remains the daemon composition owner.
16. Graceful shutdown remains functional.
17. The daemon can remain alive without using NTCP2 as a keepalive service.
18. No fake network service is introduced to satisfy lifecycle tests.

### Tests/guardrails

19. A config regression proves omitted NTCP2 is disabled.
20. A config regression proves explicit enable is rejected.
21. A composition regression proves `ntcp2-transport` is absent from the normal service graph.
22. A bounded behavioral test proves normal startup does not own/bind the NTCP2 endpoint, unless an equivalent deterministic composition assertion is demonstrably stronger.
23. The tests do not depend on public networking.
24. The tests do not use rootless namespaces, Multipass, Docker, or VM placement.
25. `scripts/check-ntcp2-interoperability.sh` no longer relies on the ineffective public-function grep as its only activation guard.
26. The checker remains small and protocol-policy-oriented.
27. No plan-history token matrix is added to the checker.

### Interop boundary

28. Plan 099 workflow is unchanged.
29. No Plan 099/100 live interop rerun occurs.
30. No new NTCP2 interoperability runner/schema/topology is added.
31. The localized `noise_authenticated` result remains the current development result.
32. External NetDB over NTCP2 remains blocked.
33. Plan 079 remains deferred to the later activation/public-network checkpoint.
34. NTCP2 remains non-advertised.

### Planning truthfulness

35. Plan 099 status records the Plan 100 correction-budget/reproduction deviation explicitly.
36. That deviation does not reopen Plan 100.
37. Plans 087/088 top active blocks no longer say Plan 100 is awaiting a live run.
38. Historical bodies of Plans 087/088 remain preserved.
39. Plan 101 is identified as a daemon-safety correction, not an interop continuation.
40. README current status no longer says the daemon is a non-networked `RuntimeNotImplemented` shell.
41. README current status acknowledges the real localized post-TCP interop result without overstating compatibility.

### Scope and quality

42. No NTCP2 handshake/Noise/frame/data-phase behavior changes occur.
43. No new Python interoperability code is added.
44. No new CI workflow is added.
45. No new dependency is added unless strictly required by daemon composition; preference is zero new dependencies.
46. No RouterInfo/NetDB implementation is mixed into this correction.
47. Required cargo/static checks pass.
48. Working tree is clean at handoff.
49. Plan 101 closes with normal-daemon NTCP2 disabled and unenableable.
50. RouterInfo/local-NetDB construction is explicitly the next product-development line.

## Smaller-model execution sequence

Execute in this order. Do not parallelize edits to `config.rs` and `lib.rs` across separate agents because their activation semantics must remain coherent.

### Phase A — authority and baseline

1. Read Plan 101 completely.
2. Confirm HEAD/baseline.
3. Read the final section of `plans/099-status.md` and the top active blocks of Plans 087/088.
4. Read `crates/i2pr-daemon/src/config.rs`, `lib.rs`, daemon tests, and the small NTCP2 boundary checker.
5. Confirm no interop workflow modification is needed.

### Phase B — configuration safety

6. Set the default disabled.
7. Reject explicit enable.
8. Add focused config tests.
9. Run `cargo test -p i2pr-daemon`.

### Phase C — composition safety

10. Remove normal service-graph NTCP2 construction/registration.
11. Preserve daemon lifecycle with the smallest valid transport-neutral service graph/lifecycle owner.
12. Add composition/socket behavioral regression.
13. Run daemon tests again.

### Phase D — durable guardrail

14. Replace the ineffective static activation heuristic with a small durable assertion backed by the functional tests.
15. Run `scripts/check-ntcp2-interoperability.sh`.

### Phase E — status/document reconciliation

16. Record the Plan 100 procedural deviation in Plan 099 status.
17. Refresh only the active headers of Plans 087/088.
18. Update the README current-status paragraph.
19. Do not rewrite historical plan bodies.

### Phase F — validation and close

20. Run the required workspace validation set.
21. Run `git diff --check`.
22. Confirm no interop workflow/runner/NTCP2 protocol code changed.
23. Confirm no socket is bound by normal daemon NTCP2.
24. Commit the correction cleanly.
25. Mark Plan 101 passed and hand off to RouterInfo/local-NetDB implementation.

## Expected final state

```text
plan_099                     = closed-protocol-defect-localized
plan_100                     = closed-with-recorded-procedural-deviation
plan_101                     = passed-daemon-ntcp2-activation-safety
plan_079                     = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint

development_interop          = protocol-defect-localized
highest_stage                 = noise_authenticated
ntcp2_status                  = experimental
ntcp2_advertised              = false
ntcp2_normal_daemon           = disabled
ntcp2_enable_config           = rejected
external_netdb_over_ntcp2     = blocked

production_daemon             = active supervised composition root
persistent_identity           = active
routerinfo_local_foundation   = next
netdb_local_foundation        = next
offline_su3                   = later local milestone work
```

## Closure principle

Plan 101 protects the distinction between **building the router** and **prematurely activating an unqualified transport**.

The production daemon should continue advancing as a real supervised Rust process. Local RouterInfo, NetDB, persistence, lookup state machines, and offline reseed parsing do not need to wait for NTCP2. At the same time, a localized independent-interoperability result must not silently turn into a public listener merely because daemon composition has begun.

Fix that boundary once, add a regression that actually catches it, record the historical Plan 100 procedural deviation truthfully, and continue building the router.