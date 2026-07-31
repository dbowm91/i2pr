# Plan 073: deferred Java and release qualification closure

## Status and activation

- Status: planned-deferred.
- Parent roadmap: Plan 067.
- Requires Plan 068 closed.
- Requires Plan 079 closed for normal execution.
- May incorporate Plan 072 results when Plan 072 was activated.
- Activates only when a suitable isolated Ubuntu host or guest is available.
- Plan type: final Java/i2pd interoperability qualification, release-grade evidence, and Milestone 3 support decision.

## Objective

Complete the release-grade portion of Milestone 3 after development validation has already established stable two-way i2pd interoperability.

The plan must:

1. exercise the existing direct Java I2P driver in both roles;
2. confirm i2pd in both roles inside the same class of isolated execution lane;
3. use exact DeliveryStatus and Router Hash correlation;
4. use fresh independent state;
5. enforce no-public-egress isolation;
6. produce sanitized durable evidence;
7. verify cleanup and provenance;
8. make a final decision on NTCP2 support status;
9. avoid recreating the excessive pre-execution machinery that blocked Plans 056-066.

## Activation environment

A suitable execution environment must provide:

- Ubuntu 24.04 or another explicitly documented compatible Ubuntu release;
- amd64 when required by pinned reference build assumptions, or a verified architecture-compatible build;
- at least 4 vCPU;
- at least 16 GiB RAM, with 24 GiB preferred;
- at least 50 GiB free disk;
- working unprivileged user/network/mount/PID namespaces or an equivalently isolated disposable guest;
- no public route and no DNS in the execution namespace/guest;
- host-side preparation domain for source acquisition/builds;
- JDK 17 and required Ant version;
- CMake/C++/Boost/OpenSSL for i2pd;
- Rust/Cargo/Python;
- stable local process execution.

The current constrained host may remain the development host. Plan 073 may be executed on a separate dedicated host or disposable guest.

## Authority corrections inherited from Plan 068

The following are mandatory:

- ADR 0022 direct Java stripped-router driver is the active Java architecture;
- ADR 0021 rejection is not a blocker to the direct driver;
- Plan 066 is historical, not active execution authority;
- lower-tier smoke/development records inform readiness but cannot substitute for Level 3 evidence;
- a fresh release candidate is cut only after the real reference drivers build and pass prequalification on the selected host.

## Qualification matrix

Required primary IPv4 directions:

| Reference | Direction |
| --- | --- |
| Java I2P | `i2pr-to-java-ipv4` |
| Java I2P | `java-to-i2pr-ipv4` |
| i2pd | `i2pr-to-i2pd-ipv4` |
| i2pd | `i2pd-to-i2pr-ipv4` |

IPv6 remains outside this closure unless separately planned.

## Repetition policy

The minimum release matrix is:

- two independent complete runs of all four directions;
- fresh mutable state for every direction and every run;
- one locked source commit/reference set across both runs.

This preserves the useful core of the Plan 066 two-run model while removing the requirement to pre-build an elaborate candidate declaration before the drivers have passed host prequalification.

Driver prequalification before candidate freeze:

- Java inspect/listen/dial: at least 3/3 fresh-state passes per mode on the selected host;
- i2pd inspect/listen/dial: at least 3/3 fresh-state passes per mode on the selected host;
- instrumented/control comparison for any patched observer;
- cleanup clean;
- no public egress.

The old 10/10 requirement is not mandatory unless real flakiness or a separate release-risk decision justifies it.

## Release pass predicate

Every direction in both runs requires:

```text
candidate source commit locked
reference revision and binary digest locked
fresh mutable state
valid signed local and peer RouterInfo
expected network ID
expected NTCP2 address/static key/IV
TCP connected
both sides NTCP2 authenticated
sender exact frame emitted
receiver exact frame authenticated and decrypted
receiver exact DeliveryStatus I2NP decoded
message ID continuity exact
Router Hash continuity exact
no synthetic fallback
topology isolation attested
no public route/DNS/undeclared destination
cleanup clean
sanitized evidence complete
```

A generic log, sender-only event, handshake-only connection, fixture, local self-test, or lower-tier record cannot pass.

## Phase 1: selected-host attestation

Create a concise environment attestation containing:

- OS/kernel/architecture;
- CPU/RAM/disk;
- namespace or guest-isolation probe;
- routing table before execution;
- DNS state;
- toolchain versions;
- time source/skew status;
- preparation/execution boundary;
- writable roots;
- cleanup authority.

Reject the host when:

- public egress cannot be disabled;
- process/namespace ownership is ambiguous;
- memory pressure causes repeated guest/process termination;
- source/build storage is insufficient;
- required toolchains cannot be pinned/recorded.

Do not add another Multipass-specific recovery roadmap. Select one working lane and record it.

## Phase 2: build and prequalify reference drivers

### Java I2P

Use existing Plan 063 artifacts:

```text
tests/integration/ntcp2/reference-drivers/java/
```

Required prequalification:

- pinned source/classpath verification;
- driver builds with nonzero binary/build-manifest digests;
- inspect mode validates local signed RouterInfo and disabled subsystems;
- 3/3 listen mode fresh-state passes against the appropriate test peer;
- 3/3 dial mode fresh-state passes;
- exact DeliveryStatus handler correlation;
- clean JVM/router shutdown with no residual threads/locks/listeners;
- no SAM/I2CP/SSU2/tunnel/floodfill/reseed path.

Fix bounded pinned-API/build defects before candidate freeze. Any protocol behavior correction to i2pr returns to the owning source and requires rerunning Plan 079.

### i2pd

Use the Plan 078/079 driver and development results.

Required selected-host prequalification:

- source/binary digests match the intended release reference set;
- 3/3 listen and 3/3 dial passes in the isolated lane;
- instrumented/control behavior-neutrality passes;
- cleanup/no-egress pass.

## Phase 3: freeze release candidate

Freeze only after Phase 2 passes.

Candidate record must be concise and include:

```text
schema/version
candidate source commit
source tree digest or Git tree identity
Java revision/binary/build-manifest digests
i2pd revision/instrumented/control/build-manifest digests
runner/verifier digests
selected execution-lane attestation digest
protocol support state
candidate status = executable
```

Do not include large tables of every planning document or static checker. Bind only artifacts that can change the protocol result, observation authority, or verifier outcome.

Any change to:

- production NTCP2 code;
- reference driver/observer;
- reference revision;
- strict scenario/correlation code;
- release runner;
- release verifier;
- execution isolation configuration;

retires the candidate and returns to Phase 2.

Documentation-only changes that do not alter these artifacts do not retire the candidate.

## Phase 4: Run A

Execute all four directions in any deterministic recorded order.

Each direction gets:

- independent state;
- unique run/direction ID;
- unique message ID;
- exact RouterInfo exchange;
- structured sender/receiver events;
- cleanup record;
- sanitized summary;
- network/isolation attestation reference.

Run A passes only if all four directions pass.

Stop after first primary failure. Preserve sanitized evidence, diagnose, retire candidate if a code/config artifact must change, and return to Phase 2.

## Phase 5: Run B

Execute from new independent mutable state after Run A completes.

Required independence:

- new run ID;
- new identities/static keys/IVs;
- new ports;
- new message IDs;
- no copied writable state;
- no reused event files;
- same candidate/source/reference binaries;
- same selected isolation lane class;
- same pass predicate.

Run B passes only if all four directions pass.

## Phase 6: final verifier

Reuse the existing Plan 056/066 certificate verifier only after simplifying its active contract under Plan 068.

The final verifier must check:

- each bundle validates independently;
- exactly four required directions per run;
- all direction predicates pass;
- source/reference/runner/verifier identity is stable across runs;
- mutable state/run/message identities are independent;
- no lower-tier record is promoted;
- no raw secret-bearing diagnostics are included;
- cleanup/isolation pass;
- no evidence mutation after finalization.

The verifier need not require:

- Plan 056/060/066 prose markers;
- digest entries for historical plans;
- exact test class names;
- reviewer identity as a cryptographic prerequisite;
- irrelevant static-check artifacts.

Independent review is still recommended before support status changes, but reviewer metadata must not be conflated with protocol evidence validity.

## Phase 7: bounded adversarial controls

Run at least once per reference implementation, not necessarily in both Run A and Run B:

- network ID mismatch;
- wrong peer RouterInfo/static-key binding;
- wrong DeliveryStatus message ID;
- malformed/unauthenticated data frame;
- stale/replayed handshake when practical;
- duplicate-link race control when already supported by the runtime harness.

Expected rejection and cleanup are required. These controls may reuse the Level 2 mechanisms adapted to the isolated lane.

Do not add load testing or broad fault injection.

## Phase 8: release decision

Possible outcomes:

### `qualified-experimental`

Criteria:

- both complete runs pass;
- required controls reject correctly;
- cleanup/isolation/evidence pass;
- no unresolved conformance defect.

Consequence:

- Milestone 3 closes;
- NTCP2 may remain experimental but can be enabled behind an explicit development/operator flag according to a later activation plan;
- advertisement remains false unless a separate activation/support decision changes it.

### `qualified-for-advertisement-review`

This stronger outcome requires an additional explicit support/activation review beyond this plan. Do not set it automatically.

### `blocked-environment`

Use only when the selected host cannot execute the isolation/build/runtime requirements and no protocol run occurred.

### `failed-protocol`

Use when a reproducible conformance failure occurs. Record the exact stage and return work to a narrow corrective plan.

### `failed-evidence-or-cleanup`

Use when protocol appears to pass but observation authority, isolation, cleanup, or durable evidence fails.

## Deliverables

At implementation/closure, expected artifacts include:

```text
plans/073-candidate.md
plans/073-closure.md
tests/integration/ntcp2/qualification/java-direct-driver.json
tests/integration/ntcp2/qualification/i2pd-direct-driver.json
tests/integration/ntcp2/qualification/release-summary.json
```

Evidence bundles may remain in an external/local artifact root with committed cryptographic receipts when repository policy forbids committing bulky diagnostics.

Update:

```text
plans/030-milestone-3-closure.md
README.md
AGENTS.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
specs/support.toml
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

Do not change `advertised = false` unless a separate explicit activation decision is included and justified.

## Required tests

Before candidate freeze:

- Java source-lock/build/driver tests;
- i2pd source-lock/build/control tests;
- strict scenario/correlation tests;
- release runner/verifier tests;
- lower-tier promotion rejection;
- candidate artifact drift tests;
- isolation/no-egress attestation tests;
- cleanup tests.

Required verifier negatives:

1. missing direction;
2. handshake-only direction;
3. sender-only evidence;
4. wrong message ID;
5. wrong Router Hash;
6. reused run ID;
7. reused mutable state;
8. source commit drift;
9. reference binary drift;
10. runner/verifier drift;
11. lower-tier smoke/development record included;
12. cleanup failure;
13. isolation/public-egress failure;
14. raw secret-bearing diagnostics;
15. post-finalization mutation.

Keep fixtures minimal. Do not recreate hundreds of plan-marker tests.

## Validation commands

Exact selected-host commands must be written into the Plan 073 closure record.

Local source baseline:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Selected-host commands must include:

- environment attestation;
- Java build/prequalification;
- i2pd build/prequalification;
- candidate freeze;
- Run A;
- Run B;
- final verifier;
- adversarial controls;
- cleanup/residual probe.

## Non-goals

Plan 073 does not:

- implement new NTCP2 features;
- test SSU2;
- test IPv6;
- test tunnels/SAM/I2CP/NetDB/floodfill;
- join the public I2P network;
- measure anonymity or performance;
- automate release publishing;
- require CI-hosted qualification;
- make Emissary mandatory;
- enable advertisement without a separate explicit decision.

## Stop rules

Stop and record the exact outcome when:

- the selected host cannot enforce no-public-egress isolation;
- Java or i2pd pinned drivers cannot build without behavior-changing patches;
- any primary direction fails exact correlation;
- observer instrumentation changes protocol outcome;
- cleanup leaves residual processes/sockets/state;
- a candidate-bound artifact changes;
- evidence contains placeholders or zero digests;
- lower-tier evidence is proposed as a substitute;
- the scope expands into other protocols or public network operation.

## Closure criteria

Plan 073 closes successfully only when:

- selected-host attestation passes;
- Java and i2pd prequalification pass;
- one executable candidate is frozen after prequalification;
- Run A passes all four directions;
- Run B independently passes all four directions;
- exact message/identity continuity passes throughout;
- adversarial controls reject correctly;
- cleanup and no-egress pass;
- final verifier passes;
- release summary and closure records are complete;
- Milestone 3 status is updated truthfully;
- support remains at the explicitly approved level.

## Small-model handoff instructions

- Do not begin without a suitable isolated host.
- Prequalify drivers before cutting a candidate.
- Use the existing direct Java/i2pd drivers; do not redesign topology.
- Bind only protocol-relevant artifacts in the candidate.
- Stop on the first primary failure.
- Retire the candidate after any relevant change.
- Keep release evidence distinct from Level 1/2 records.
- Do not alter advertisement status implicitly.
