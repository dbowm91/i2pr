# Plan 086: status authority and host-loopback development lane

## Status and dependencies

- Status: planned, next executable plan.
- Parent roadmap: Plan 085.
- Requires Plan 082 implementation to remain valid.
- Reuses the implemented Plan 083 and Plan 084 schemas, runners, adapters, and i2pd observer surface.
- Blocks Plans 087, 088, 079, and conditional Plan 089.
- Plan type: execution-authority correction and development-only placement enablement.

## Objective

Make the already implemented real-subprocess probes executable on the current constrained host without claiming release isolation.

This plan must introduce exactly one new topology kind:

```text
host-loopback-development
```

The topology permits the test-only i2pr launcher and pinned i2pd direct driver to exchange NTCP2 traffic through literal IPv4 loopback. It must remain unavailable to production daemon paths, release qualification, broad scenario matrices, Java qualification, and normal support claims.

Plan 086 ends after proving that the lane can prepare both identities, render both strict live scenarios, start the intended listener process, and cleanly tear down. It must not execute a complete DeliveryStatus probe or claim any protocol result.

## Status correction owned by this plan

The active status must become:

```text
plan_082 = implemented_and_closed
plan_083_implementation = complete
plan_083_execution = pending
plan_084_implementation = complete
plan_084_execution = pending
plan_084_development_decision = pending
plan_079 = blocked_pending_plan_088
plan_072 = inactive_pending_plan_088_ambiguity
historical_plan080_lane_qualification = retained
current_plan080_guest_availability = unavailable_or_stale
```

Do not delete or rewrite the historical Plan 083/084 status records. Add a dated active-status amendment and update concise current-status sections where necessary.

The following historical statement is no longer active authority:

```text
Plan 084 closed with decision = lane-invalidated
```

The corrected interpretation is:

```text
Plan 084 runner implementation completed;
required reverse wire execution never occurred;
development decision remains pending.
```

## Work packages

### WP1. Add the topology constant and bounded metadata

Add `host-loopback-development` only to the Plan 083/084 development probe topology allowlist.

Required record metadata:

```text
topology_kind = host-loopback-development
development_only = true
release_qualified = false
isolation_qualified = false
public_network_blocked = unproven
parent_network_state_unchanged = true
```

When the existing compact schema cannot express these flags without a schema revision, prefer a narrow additive v2 shared by forward and reverse records. Do not create separate new schemas for each direction. A schema bump is not required merely to rename a topology value when existing fields already preserve the limitation.

### WP2. Allow literal IPv4 loopback only in the test-only preparation boundary

The current preparation path accepts synthetic documentation ranges. Extend the test-only `i2pr-interop ntcp2 prepare` and scenario validation boundary so that:

```text
127.0.0.1
```

is accepted only when the scenario topology is `host-loopback-development`.

Reject:

```text
127.0.0.0
127.0.0.2-127.255.255.255
0.0.0.0
::1
::
hostnames
DNS names
public addresses
RFC 5737 documentation addresses under the loopback topology
```

The normal synthetic-range paths must remain unchanged for isolated lanes.

Do not modify production configuration validation or normal daemon address policy.

### WP3. Add an explicit placement type

Add a narrow placement or runner mode that executes the command directly on the current host while binding only to `127.0.0.1`.

Preferred name:

```text
HostLoopbackDevelopmentPlacement
```

Required behavior:

- no namespace command wrapping;
- no Multipass wrapping;
- no shell interpolation;
- absolute measured binary paths;
- one confined run root;
- bounded stdout/stderr capture;
- environment allowlist;
- direct process handles for truthful counters and teardown;
- no privilege escalation;
- no network-management side effects.

Do not generalize this into an arbitrary host-command placement framework.

### WP4. Allocate bounded loopback endpoints

Provide one small endpoint allocator or explicit input contract.

Required properties:

- address always `127.0.0.1`;
- two distinct nonzero TCP ports when both endpoints are represented;
- port reservation race minimized through a bounded existing helper or immediate listener startup;
- no listener binds `0.0.0.0`;
- no port is reused across concurrent owned runs;
- allocated endpoints are included in the run identity before live startup.

Do not implement a persistent port registry service.

### WP5. Prove the test binaries have no bootstrap dependency

Before enabling the lane, assert through focused source/config checks that the exact execution path uses only:

```text
i2pr-interop ntcp2 prepare/listen/dial/validate-scenario
i2pd direct driver inspect/listen/dial
network ID 99
explicit peer RouterInfo exchange
explicit 127.0.0.1 endpoint
```

It must not activate:

- reseed;
- public NetDB bootstrap;
- DNS lookup;
- SAM or I2CP;
- HTTP/SOCKS proxy;
- normal router daemon;
- SSU2;
- transit tunnels.

Do not attempt to prove host-wide egress prevention. Record that isolation is unproven.

### WP6. Add thin executable entry points

The existing runner functions need a bounded operator surface. Add either:

```text
python3 -m tests.integration.ntcp2.harness.plan083_runner ...
python3 -m tests.integration.ntcp2.harness.plan084_runner ...
```

or one thin wrapper:

```text
scripts/interop/run-minimal-i2pd-host-loopback-probe.py --direction i2pr-to-i2pd|i2pd-to-i2pr
```

The entry point must:

- accept explicit repo root, run root, binary paths, source commit, reference revision, and message ID;
- select only `host-loopback-development`;
- refuse support/release profile flags;
- write one compact record path;
- return nonzero for rejected, failed, or cleanup-failed records;
- print no private material or raw protocol bytes.

Do not duplicate runner orchestration in the wrapper.

### WP7. Add lane-only preflight and smoke

Provide a preflight that stops before a peer connection completes.

It must prove:

```text
i2pr prepare succeeds on 127.0.0.1
signed RouterInfo validates for the loopback endpoint
i2pd inspect succeeds and exports a signed RouterInfo
strict forward scenario renders and validates
strict reverse scenario renders and validates
intended listener starts on 127.0.0.1
listener-ready event is authentic
listener is terminated before any dialer starts
cleanup is clean
```

This preflight may start one listener solely to validate placement and binding. It must not start the opposite dialer or promote any stage beyond `listener_ready`.

### WP8. Correct exact status and decision vocabulary

Use one exact execution blocker token:

```text
manual-isolated-fallback-required
```

only when direct host placement cannot start or bind correctly for a demonstrated non-protocol reason after one bounded attempt.

Do not use both `lane_invalidation_pending` and `lane-invalidated` in new active records.

Allowed Plan 086 closure states:

```text
host-loopback-development-ready
manual-isolated-fallback-required
blocked-artifact-or-build-defect
```

## Suggested touched files

Keep the change narrow. Expected files include:

```text
tools/i2pr-interop/src/main.rs
tools/i2pr-interop/src/scenario.rs
tests/integration/ntcp2/harness/process.py
tests/integration/ntcp2/harness/i2pr.py
tests/integration/ntcp2/harness/plan083_runner.py
tests/integration/ntcp2/harness/plan084_runner.py
tests/integration/ntcp2/harness/minimal_i2pd_probe.py only if topology metadata requires it
tests/integration/ntcp2/harness/minimal_i2pd_reverse_probe.py only if topology metadata requires it
tests/integration/ntcp2/harness/test_plan086.py
scripts/interop/run-minimal-i2pd-host-loopback-probe.py or module CLI
scripts/check-ntcp2-interoperability.sh only for narrow invariants
plans/086-status.md
```

Do not change production daemon, NetDB, tunnel, SAM/I2CP, SSU2, release, or CI code.

## Required tests

At minimum cover:

1. loopback accepted only for `host-loopback-development`;
2. synthetic addresses remain accepted only in existing isolated topology paths;
3. wildcard, hostname, public, alternate loopback, and IPv6 addresses are rejected;
4. topology cannot set release/isolation qualification true;
5. host placement uses absolute argv and no shell;
6. process counters remain zero on failed start;
7. listener binds exactly `127.0.0.1`;
8. preflight never starts a dialer;
9. cleanup removes all owned processes and sockets;
10. Plan 083/084 existing fake-process tests continue to pass;
11. normal daemon configuration does not recognize the development topology;
12. the wrapper refuses release/support flags.

## Acceptance criteria

Plan 086 closes as `host-loopback-development-ready` only when:

- current status authority correctly distinguishes implementation completion from execution completion;
- Plan 083 and Plan 084 are active execution-pending plans;
- literal `127.0.0.1` is accepted only under the development topology;
- the exact i2pr and i2pd test binaries prepare authentic state;
- both strict role scenarios validate with real hashes and run identity;
- one intended listener starts and emits authentic readiness;
- no dialer is started by the lane preflight;
- teardown leaves no owned child or listening socket;
- records explicitly state that isolation and release qualification are false;
- Plan 087 is registered as the next executable plan;
- `plans/086-status.md` records exact commands and focused test results.

Plan 086 may close as `manual-isolated-fallback-required` only when:

- the test binaries and artifacts validate;
- direct host process placement or literal loopback binding fails before protocol execution;
- the reason is reproduced once;
- the exact reason is not a protocol-stage failure;
- Plan 089 is activated without changing the Plan 087/088 runner architecture.

## Validation commands

Use focused checks:

```bash
cargo fmt --all --check
cargo check -p i2pr-interop
cargo test -p i2pr-interop
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan086.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pr_prepare.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan082.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan084.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Run a direct lane preflight, but do not execute a complete forward or reverse protocol probe in this plan.

## Stop rules

Stop and record a typed blocker when:

- accepting loopback requires weakening production address validation;
- the direct driver unexpectedly requires public bootstrap or DNS;
- the listener cannot be constrained to literal `127.0.0.1`;
- the only proposed fix adds privilege escalation or host network reconfiguration;
- preparation or strict scenario rendering regresses;
- a change expands into protocol correction, Plan 087 execution, Java, Emissary, or recurring CI.

## Non-goals

Plan 086 does not:

- establish TCP between peers;
- run a complete NTCP2 handshake;
- send DeliveryStatus;
- fix protocol code;
- qualify isolation or release behavior;
- run Plan 079;
- activate Emissary;
- add a general topology or plugin framework.

## Small-model execution guidance

1. Add the topology constant and validation tests first.
2. Add literal loopback support only in the test-only prepare/scenario path.
3. Add the direct host placement.
4. Add the thin runner CLI without copying orchestration.
5. Run the listener-only lane preflight.
6. Write `plans/086-status.md`.
7. Stop before starting a dialer.

Do not execute Plan 087 in the same commit.