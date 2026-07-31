# Plan 067: Milestone 3 staged interoperability corrective roadmap

## Status and authority

- Status: planned.
- Plan type: corrective roadmap and active execution-order authority for Milestone 3 recovery.
- Supersedes Plan 066 as the active execution authority. Plan 066 remains an immutable historical record of the unavailable release-qualification lane on the constrained host.
- Does not erase Plans 031 through 066, ADR 0021, ADR 0022, qualification receipts, or prior blocked evidence.
- Corrects the active planning premise that release-grade isolation and two-run certification are prerequisites for the first external protocol test.
- Corrects the stale active use of ADR 0021 rejection as a Java architecture blocker after ADR 0022 accepted the direct Java stripped-router driver.
- NTCP2 remains experimental, non-advertised, and disabled in normal daemon operation throughout this roadmap.

## Problem statement

Milestone 3 contains substantial implemented NTCP2 protocol and runtime code, extensive deterministic tests, a source-locked i2pd direct driver, a source-locked Java I2P direct driver, exact DeliveryStatus correlation, and a canonical evidence pipeline. However, the current qualification receipts record zero real listen, dial, or control attempts for both reference implementations.

The current blocker is primarily architectural coupling in the test apparatus:

1. the first external handshake is gated by the same rootless namespace or Multipass isolation required for release-grade evidence;
2. the current host cannot create the required unprivileged namespace and cannot reliably sustain the Multipass recovery lane;
3. candidate, digest, evidence-bundle, reviewer, and two-run certificate requirements are enforced before any simple two-process interoperability result exists;
4. the direct i2pd and Java build/run seams are host-executable, but the canonical runner refuses to exercise them outside the sealed qualification lane;
5. Plan 066 still cites the rejected ADR 0021 support topology as a Java blocker even though ADR 0022 accepted and implemented a replacement direct-driver topology;
6. hundreds of harness-schema tests now exist while the external qualification attempt counts remain zero.

The repository therefore needs a staged validation model that separates protocol discovery and development validation from release qualification.

## Correct staged validation model

### Level 0: local conformance

Purpose: prove deterministic protocol components and runtime ownership locally.

Evidence includes:

- official or source-locked cryptographic vectors;
- codec round trips;
- handshake state-machine tests;
- frame/block parser rejection tests;
- replay, skew, duplicate-link, and admission-policy tests;
- fuzzing and partial-I/O tests;
- loopback i2pr self-composition only as an implementation diagnostic.

Level 0 cannot claim mixed-router interoperability.

### Level 1: external loopback smoke

Purpose: answer whether i2pr can complete NTCP2 and exchange one exact I2NP message with an independent implementation on the current host.

Required properties:

- two real processes;
- one i2pr process and one independent reference process;
- fresh temporary identity and state directories;
- loopback-only endpoints with ephemeral ports;
- network ID 99;
- direct signed RouterInfo exchange;
- NTCP2 only;
- no reseed, NetDB bootstrap, SAM, I2CP, tunnel pool, floodfill, SSU2, DNS, or public peer discovery;
- exact DeliveryStatus message ID continuity;
- exact sender and receiver Router Hash continuity;
- bounded deadlines and clean shutdown;
- a concise non-authoritative smoke record;
- best-effort network syscall observation, with an explicit weaker configuration-only attestation when ptrace/strace is unavailable.

Level 1 does not require a rootless network namespace, Multipass, a frozen candidate, two evidence bundles, reviewer certification, or Java I2P.

### Level 2: repeated development interoperability

Purpose: establish a stable development-level interoperability result against the primary independent validator.

Initial primary validator: pinned i2pd 2.60.0.

Required properties:

- both directions pass;
- three independent fresh-state repetitions per direction;
- exact message and identity correlation in every pass;
- bounded negative controls;
- clean process/socket/state teardown;
- no observed network destination outside the declared loopback allowlist when syscall observation is available;
- failures remain protocol/runtime failures rather than evidence-generation failures.

Level 2 is sufficient to continue later router development while NTCP2 remains experimental and non-advertised.

### Level 2D: differential validation

Purpose: resolve ambiguous failures or gain a third-implementation comparison.

Preferred secondary implementation: Emissary.

This level is conditional and non-blocking. It should execute only when:

- i2pr and i2pd disagree and the failure cannot be localized from the specification and structured events;
- a low-cost third implementation comparison materially reduces uncertainty; or
- the project explicitly chooses to increase confidence before Java qualification.

### Level 3: release qualification

Purpose: justify enabling or advertising NTCP2 beyond experimental development use.

Required properties:

- Java I2P and i2pd;
- both directions for each reference;
- isolated no-public-egress execution lane;
- reproducible source/reference provenance;
- exact authenticated data-phase message correlation;
- independent fresh state;
- cleanup and residual-state verification;
- sanitized durable evidence;
- final Milestone 3 release-qualification decision.

The current Plan 066 certificate machinery may be reused or simplified at Level 3. It must not gate Levels 1 or 2.

## Roadmap decomposition

### Plan 068: staged-evidence architecture and planning correction

Owns:

- proposed ADR 0023 for staged interoperability evidence;
- active-status correction for Plan 030 and Plan 066;
- removal of stale ADR 0021 Java blocker from active freeze/readiness logic;
- explicit distinction between development validation and release qualification;
- smoke/development record contracts;
- simplification boundaries for static checks and test matrices;
- documentation and support-ledger wording.

### Plan 069: host-compatible NTCP2 loopback smoke lane

Owns:

- a new non-authoritative loopback runner;
- ephemeral ports and fresh temporary state;
- direct RouterInfo exchange;
- strict feature disablement;
- minimal smoke record;
- optional strace network allowlist audit;
- reuse of exact Plan 065 DeliveryStatus and Router Hash correlation;
- no candidate or certificate dependency.

### Plan 070: i2pd driver build and first two-way live execution

Owns:

- building the existing source-locked i2pd driver on the current Ubuntu host;
- resolving bounded build/runtime defects in the test-only driver surface;
- one i2pr-to-i2pd passing smoke run;
- one i2pd-to-i2pr passing smoke run;
- exact failure-stage triage when either direction fails;
- updating qualification/development receipts without fabricating release qualification.

### Plan 071: repeated i2pd development validation and negative controls

Owns:

- three fresh-state passes in each direction;
- network-ID mismatch control;
- RouterInfo/static-key mismatch control;
- DeliveryStatus correlation mismatch control;
- malformed or unauthenticated data-phase control;
- replay/stale-handshake control when supported without expanding scope;
- cleanup and network-audit repetition;
- a development-validation closure record.

### Plan 072: conditional Emissary differential lane

Owns:

- source-locking an Emissary revision only if the conditional entry criteria are met;
- the smallest practical two-process NTCP2 path;
- one direction in each role;
- differential failure localization;
- no promotion of Emissary to sole conformance authority;
- no delay of Plan 071 or ordinary development when i2pd already passes.

### Plan 073: deferred Java and release qualification closure

Owns:

- exercising the existing Java direct driver on a suitable host or guest;
- final i2pd confirmation in the isolated lane;
- Java and i2pd both directions;
- release-grade evidence and cleanup;
- simplified final certificate/closure criteria;
- the decision whether NTCP2 may move beyond experimental/non-advertised status.

## Dependency graph

```text
Plan 067 roadmap
        |
        v
Plan 068 architecture/status correction
        |
        v
Plan 069 loopback smoke lane
        |
        v
Plan 070 first i2pd two-way execution
        |
        v
Plan 071 repeated i2pd validation + negatives
        |
        +--------------------------+
        |                          |
        v                          v
Plan 072 conditional         ordinary later
Emissary differential        development may continue
        |
        v
Plan 073 Java + release qualification
```

Plan 072 is not on the mandatory critical path unless its entry criteria are met.

## Global implementation rules

1. Reuse the existing Plan 063 and Plan 064 direct drivers before designing any replacement reference-driver architecture.
2. Reuse Plan 065 exact DeliveryStatus message ID and Router Hash correlation.
3. Do not let a smoke record satisfy a release-qualification predicate.
4. Do not require candidate freeze, two-bundle certification, reviewer records, or sealed namespaces for Level 1 or Level 2.
5. Do not silently weaken Level 3. Release qualification remains isolated, reproducible, and multi-implementation.
6. Keep all reference drivers and runner changes in test/integration tooling. No production crate may depend on i2pd, Java I2P, Emissary, Python harness code, strace, or Multipass.
7. Never patch reference NTCP2 cryptography, Noise transcript behavior, frame encoding, RouterInfo signature verification, or acceptance policy to make a run pass.
8. Use fresh temporary state for every run. Do not repair failures by reusing a warmed reference data directory.
9. Record protocol/runtime failure stages directly. Evidence formatting errors must not obscure the primary failure.
10. Keep CI light. Level 1 and Level 2 remain manual/local integration commands unless a later plan explicitly provides a suitable runner.
11. Do not add another schema version unless a materially new semantic contract cannot be represented by the proposed smoke/development record.
12. Historical plan and evidence documents remain immutable except for explicit supersession notices and active-status corrections.

## Global non-goals

This roadmap does not authorize:

- production daemon NTCP2 activation;
- public RouterInfo publication;
- live I2P network participation;
- reseed, floodfill, NetDB, tunnel, SAM, or I2CP development;
- SSU2 implementation;
- IPv6 qualification;
- load, throughput, anonymity, or de-anonymization claims;
- replacement of i2pd or Java code with locally implemented protocol stubs;
- broad CI expansion;
- release automation.

## Environment constraints

The current environment is assumed to provide:

- Ubuntu host execution;
- no reliable unprivileged user/network namespace;
- no reliable Multipass qualification guest;
- ordinary process execution;
- loopback TCP;
- temporary directories;
- Rust/Cargo, Python, CMake, C/C++, OpenSSL, and Boost as installable host dependencies;
- optional ptrace/strace capability, which may be unavailable.

The plans must degrade explicitly:

- when strace is available, audit network syscalls and reject destinations outside the allowlist;
- when strace is unavailable, record `network_audit = configuration_only` and require all application-level peer discovery and bootstrap paths disabled;
- absence of strace may reduce evidence strength but does not prevent a Level 1 diagnostic run;
- absence of a sealed namespace prevents Level 3, not Level 1 or Level 2.

## Validation policy

### Required on every implementation plan

Use focused tests for the touched surface plus:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Run Clippy, rustdoc, fuzz smoke, the full historical harness matrix, rootless checks, and Multipass checks only when the plan changes their owned surface or at a deliberate integration checkpoint. Do not require every historical evidence-schema test for a small loopback-runner correction.

### Manual integration commands

Plans 069 through 073 must publish exact manual commands in their status records. Integration commands are not converted into CI requirements under this roadmap.

## Roadmap-level acceptance criteria

Plan 067 is complete as a planning artifact when:

- Plans 068 through 073 exist;
- Plan 068 explicitly corrects the stale Plan 066/ADR 0021 authority;
- Level 1 and Level 2 are executable without a namespace or Multipass;
- i2pd is the mandatory initial independent validator;
- Emissary is conditional and non-blocking;
- Java remains required for Level 3 release qualification but not for initial development validation;
- exact DeliveryStatus and Router Hash correlation are preserved;
- public-network access remains disabled;
- the roadmap gives explicit acceptance criteria, non-goals, stop rules, and handoff order;
- the roadmap reduces verification coupling rather than adding a parallel certificate apparatus.

## Roadmap stop rules

Stop the active plan and record a typed blocker when:

- the existing i2pd direct driver cannot be built without modifying reference protocol behavior;
- a Level 1 run attempts any undeclared non-loopback network destination;
- the only proposed fix is to bypass RouterInfo signature, static-key, Noise, frame authentication, or I2NP decoding;
- exact DeliveryStatus or Router Hash correlation cannot be observed on both sides;
- fresh-state cleanup cannot remove listeners or child processes;
- a proposed schema/check change would allow smoke evidence to satisfy Level 3;
- scope expands into NetDB, tunnels, SAM, I2CP, SSU2, or public network operation.

A protocol mismatch is not a roadmap blocker. It is the expected output of Level 1 and must be localized and corrected in the owning production/test surface.

## Handoff order

A smaller implementation model should execute exactly one plan at a time:

1. Plan 068;
2. Plan 069;
3. Plan 070;
4. Plan 071;
5. Plan 072 only when its entry criteria are met;
6. Plan 073 only when an isolated qualification host or guest is available.

For each plan:

- read Plan 067 and the active plan in full;
- inspect every named file before editing it;
- make the smallest cohesive changes that satisfy the plan;
- do not add unrelated hardening or CI;
- run focused checks after each work package;
- write a status record with exact commands, results, and blockers;
- never claim a live run that was not executed;
- never convert an unavailable Level 3 lane into a failed Level 1 result.
