# Plan 085: Milestone 3 host-loopback development execution roadmap

## Status and authority

- Status: planned, active corrective roadmap.
- Date: 2026-08-04.
- Parent authority: Plan 067 and ADR 0023.
- Supersedes Plan 081 as the active execution roadmap while preserving Plans 082-084 as implementation history.
- Corrects the active interpretation of Plans 083 and 084: their schemas, runners, and observer surfaces are implemented, but neither plan has completed its required real wire execution.
- Keeps Plan 079 blocked until Plan 088 records `decision = two-way-development-probe-passed`.
- Keeps Plan 072 inactive unless Plan 088 records `decision = ambiguous-reference-divergence` with one exact wire-stage question.
- NTCP2 remains experimental, non-advertised, and disabled in normal daemon operation.

## Current repository state

```text
plan_075_runner_integrity = implemented
plan_076_real_i2pd_driver = implemented
plan_080_historical_multipass_qualification = valid_historical_evidence
plan_080_current_guest_availability = unavailable_or_stale
plan_082_state_preparation = implemented_and_closed
plan_083_forward_schema_runner_observer = implemented
plan_083_forward_wire_execution = pending
plan_084_reverse_schema_runner = implemented
plan_084_reverse_wire_execution = pending
plan_084_historical_lane_invalidated_decision = superseded_for_active_execution
real_mixed_router_tcp_attempt = 0
real_ntcp2_handshake_attempt = 0
real_authenticated_frame_attempt = 0
real_delivery_status_decode_attempt = 0
protocol_conformance_result = not_yet_observed
current_primary_reference = i2pd_2_60_0
plan_079 = blocked_pending_plan_088
plan_072 = conditional_only
support = experimental
advertised = false
```

## Problem statement

The repository now contains the difficult interoperation machinery:

- authentic i2pr state preparation and strict scenario validation;
- a real pinned i2pd 2.60.0 direct driver;
- passive post-authentication, sent-I2NP, and received-I2NP observer seams;
- bounded forward and reverse compact probe schemas;
- real-subprocess forward and reverse runners;
- exact Router Hash and DeliveryStatus correlation;
- fail-closed process accounting and cleanup records.

However, the runners are still restricted to rootless-sealed or Multipass lane kinds that cannot currently execute on the available host. No retained attempt reached TCP. Continued expansion of schemas, fake-process tests, evidence aggregation, or reference implementations would not answer the protocol question.

The next objective is therefore to obtain the first genuine i2pr/i2pd wire result through a deliberately weaker but executable development lane:

```text
host-loopback-development
```

This lane is not release qualification and does not claim network isolation. It exists only to exercise the authentic local NTCP2 bytes and state machines using literal IPv4 loopback and the existing direct test binaries.

## Roadmap objectives

This roadmap must:

1. correct active status authority without rewriting historical records;
2. introduce one explicit development-only host-loopback topology;
3. keep the topology unavailable to normal daemon or release-evidence paths;
4. run one genuine `i2pr -> i2pd` DeliveryStatus probe;
5. correct only the earliest directly observed protocol defect, when one exists;
6. run one genuine `i2pd -> i2pr` DeliveryStatus probe after the forward path passes;
7. issue the actual two-way development decision;
8. activate Plan 079 only after two genuine passing directions and control-build neutrality;
9. activate Emissary only for one unresolved wire-stage differential question;
10. retain a manual isolated `--network none` fallback without adding recurring CI.

## Plan decomposition

### Plan 086: status authority and host-loopback development lane

Owns:

- superseding the current `Plan 084 closed lane-invalidated` execution interpretation;
- distinguishing historical lane qualification from current lane availability;
- adding `host-loopback-development` to the narrow Plan 083/084 execution surface;
- allowing literal `127.0.0.1` only under that topology;
- refusing wildcard, public, documentation-range, hostname, DNS, and IPv6 endpoints for this lane;
- adding a thin command entry point for each existing real-subprocess runner;
- proving preparation, strict scenario rendering, listener startup contract, and teardown without claiming protocol success.

Plan 086 must close before any live development probe is attempted.

### Plan 087: first real i2pr-to-i2pd host-loopback probe

Owns:

- one fresh-state real `i2pr initiator -> i2pd responder` execution;
- exact stage and event capture;
- one bounded fresh-state reproduction of a real failure;
- a narrow correction only when ownership is directly observed;
- rerun to pass or a precise localized protocol defect;
- behavior-neutral control-build comparison after the instrumented path passes.

Plan 087 must pass before Plan 088 begins.

### Plan 088: reverse probe and development decision

Owns:

- one fresh-state real `i2pd initiator -> i2pr responder` execution;
- exact reverse-stage and event capture;
- bounded correction and rerun when a real owned defect is observed;
- behavior-neutral control-build comparison after a pass;
- the exact development decision that gates Plans 079 and 072.

### Plan 089: conditional manual isolated fallback

Owns a non-recurring fallback only when Plan 086 proves that direct host loopback cannot execute for a concrete placement reason rather than a protocol reason.

The preferred fallback is one manually invoked Ubuntu run using one rootful container with:

```text
--network none
loopback enabled inside the container
both i2pr and i2pd processes in the same container
prepared artifacts mounted read-only
one writable run root
```

It must reuse the Plan 087/088 runners and record schemas. It must not create a second interoperability architecture or a per-push CI matrix.

## Dependency graph

```text
Plan 085 roadmap
      |
      v
Plan 086 status correction + host-loopback-development lane
      |
      v
Plan 087 real i2pr -> i2pd probe
      |
      v
Plan 088 real i2pd -> i2pr probe + decision
      |
      +--> Plan 079 when two-way-development-probe-passed
      +--> Plan 072 when ambiguous-reference-divergence

Plan 089 is conditional from Plan 086 only when direct host placement is impossible.
```

## Host-loopback development contract

The topology must be named exactly:

```text
host-loopback-development
```

Required properties:

```text
development_only = true
release_qualified = false
isolation_qualified = false
public_network_blocked = unproven
endpoint_family = ipv4
bind_address = 127.0.0.1
peer_address = 127.0.0.1
network_id = 99
reference = i2pd
```

Required restrictions:

- literal `127.0.0.1` only;
- no `0.0.0.0`, `::`, hostname, DNS, public IP, RFC 5737 address, or interface discovery;
- distinct listener ports per process and run;
- no public NetDB bootstrap or reseed;
- no SAM, I2CP, HTTP proxy, SOCKS, tunnels, SSU2, or normal daemon activation;
- no fallback reference driver;
- no protocol-event synthesis;
- no claim of namespace or egress isolation;
- exact source, binary, RouterInfo, Router Hash, run identity, and message-ID correlation remains mandatory.

## Result authority

A result becomes protocol evidence only after a real current-run event establishes at least:

```text
tcp_connected
```

Pre-TCP failures remain lane, preparation, configuration, or process ownership.

A passing direction requires:

```text
noise_authenticated
session_confirmed_accepted
authenticated_frame_written
authenticated_frame_decrypted
i2np_delivery_status_decoded
exact DeliveryStatus ID match
exact Router Hash match
clean teardown
```

The development lane proves protocol behavior only. It does not prove:

- no external egress;
- release deployment isolation;
- anonymity properties;
- Java compatibility;
- production readiness.

## Bounded correction policy

For either direction:

1. preserve the first compact result;
2. identify the highest authentic stage;
3. reproduce once from fresh state with unchanged binaries and timeouts;
4. inspect the owning source and specification section;
5. change only the owning i2pr implementation or narrow test adapter surface;
6. never patch i2pd cryptographic or acceptance behavior;
7. rerun from fresh state;
8. stop once the direction passes or one precise reproducible defect is localized.

Do not add retries, generalized topology frameworks, additional schemas, or larger evidence bundles to hide an observed failure.

## Global non-goals

Plans 085-089 do not:

- enable NTCP2 in normal daemon operation;
- advertise protocol support;
- close Java or release qualification;
- use the public I2P network;
- require Emissary by default;
- redesign the Plan 076 driver;
- create a new evidence-certificate hierarchy;
- add recurring CI, release automation, performance tests, or soak tests;
- expand into SAM, I2CP, tunnels, NetDB, SSU2, or transit routing.

## Roadmap acceptance criteria

Plan 085 is complete as a planning artifact when:

- Plans 086 through 089 exist;
- a Plan 067 active-sequence amendment registers the new authority;
- Plans 083 and 084 are reclassified as implementation-complete but execution-pending;
- Plan 079 entry authority points to the Plan 088 decision;
- Plan 072 activation authority points to the Plan 088 ambiguity decision;
- each child plan contains explicit acceptance criteria, stop rules, validation commands, and small-model guidance;
- host-loopback limitations are explicit and cannot be confused with release isolation;
- the fallback remains manual and conditional;
- no code or interoperability result is claimed by the planning pass.

## Handoff order

Execute one plan at a time:

1. Plan 086;
2. Plan 087;
3. Plan 088;
4. Plan 089 only when Plan 086 records `manual-isolated-fallback-required`.

Do not combine Plan 086 and Plan 087 in one commit. The lane contract must be independently reviewable before it carries a protocol result.