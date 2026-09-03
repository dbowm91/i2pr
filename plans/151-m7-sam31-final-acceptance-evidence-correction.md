# Plan 151 — Milestone 7 SAM 3.1 final acceptance evidence correction

Status: **next executable Milestone 7 corrective pass**.

Depends on retained results from:

- Plan 134 Milestone 6 local product closure;
- Plan 146 private-destination reference requalification;
- Plan 147 raw socket owner / TCP↔Streaming implementation;
- Plan 149 self-composing localhost SAM product;
- the valid external-client portions of Plan 150.

Supersedes for final Milestone 7 closure authority:

- Plan 150's broad `passed-m7-sam31-external-client-final-closure` interpretation.

Plan 150 remains valuable evidence. This plan does **not** discard its successful external-client STREAM, SILENT, private-destination, NAMING, negative-input, or basic FORWARD results. It corrects the mismatch between Plan 150's acceptance contract and what its executable harness actually proved.

## 1. Goal

Produce one auditable final Milestone 7 acceptance pass in which every claimed `passed` result is backed by an executable test or command that ran on the closing revision.

The remaining work is acceptance/evidence work, not another SAM architecture redesign.

When Plan 151 closes successfully, the project may truthfully state:

```text
milestone7_local_product = passed-via-plan149
milestone7_sam_localhost = passed-via-plan151
sam_independent_clients = at-least-two-passed-via-plan150
plan150_external_core_evidence = retained
plan151_final_acceptance = passed
next_product_layer = Milestone 8 planning
```

Until then:

```text
plan150_external_core_evidence = retained-passed
plan150_final_acceptance = superseded-by-plan151
plan151 = next-executable
milestone7_final_acceptance = not-yet-closed
next_product_layer = remain-on-milestone7
```

## 2. Why this corrective pass exists

The post-Plan-150 audit found that the implementation is substantially healthier and both current-head workflows are green, but Plan 150's evidence ledger overstates several acceptance items.

The concrete defect is not hypothetical. `tests/integration/sam/run-independent.sh` currently records:

```text
multiple-stream-lifecycle = passed
```

by referring to a retained Plan 149 sibling/lifecycle suite. The canonical Plan 149 file `crates/i2pr-daemon/tests/sam_stream_self_composed.rs` contains four tests covering:

- self-composed bidirectional 2 MiB transfer;
- `SILENT=true` raw transition;
- session teardown;
- same-read command + raw-byte preservation.

It does not contain the required two-sibling-stream isolation test.

Plan 149 also explicitly carried the following items forward to Plan 150 rather than claiming them as passed:

- slow reader / slow writer boundedness;
- DATA loss and retransmission;
- ACK loss;
- duplicate DATA;
- reordered DATA;
- authenticated/ciphertext corruption rejection;
- retransmission-ceiling terminal behavior;
- close/reset/sibling-stream lifecycle.

Plan 150's own final acceptance criteria require those cases, but the current external harness does not execute them. It reruns `sam_stream_self_composed` and then treats several carry-forward items as retained coverage.

The current FORWARD external lane also proves a valuable positive loopback trajectory but does not itself exercise the entire Plan 150 FORWARD lifecycle/negative matrix.

Therefore the product implementation may be correct, but the final evidence authority is not yet internally consistent.

## 3. Preserve these results; do not redo working architecture

Treat the following as retained unless Plan 151 exposes a concrete defect:

### 3.1 Plan 149 product composition

Retain:

- one shared `Arc<DestinationIdentity>` secret ownership graph;
- transactional `SESSION CREATE` product composition;
- `SamLocalProductFabric` localhost delivery capability;
- automatic per-destination runtime-driver ownership;
- local peer LeaseSet2 resolution/validation;
- typed `DeliverySweepCounters` instead of silent delivery loss;
- exact CONNECT/ACCEPT raw transition semantics;
- same-read raw byte preservation;
- the canonical 2 MiB black-box product trajectory.

Do not reintroduce post-`SESSION CREATE` calls to private bridge, LeaseSet2, tunnel-factory, driver, or byte-moving APIs.

### 3.2 Plan 150 external-client core evidence

Retain:

- pinned `i2psam` at `b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac`;
- pinned `i2plib` SAM surface at `6edf51cd5d21cc745aa7e23cb98c582144884fa8` as the qualified independent substitute;
- official `libsam3` build/probe result at `7d6e658798baec31394c5685f9583343cc00900b`, not counted because its public API cannot consume the compact Ed25519 `PRIV` shape;
- two cross-client exact 2 MiB directions;
- private destination import/generation evidence;
- SILENT transcript evidence;
- NAMING and unsupported-input transcript evidence;
- basic positive STREAM FORWARD loopback evidence;
- manual unprivileged GitHub-hosted workflow structure.

Do not vendor or patch external clients merely to close Plan 151.

### 3.3 Milestone 6

Do not reopen Plan 134 or Streaming wire semantics unless one of the new acceptance tests demonstrates a concrete protocol defect.

## 4. Non-goals

Plan 151 must not expand into:

- live public I2P testing;
- NTCP2/SSU2 interoperability;
- router-to-router tunnel interoperability;
- Java router orchestration;
- Docker/namespaces/VM/root/sudo infrastructure;
- a new general-purpose Python harness framework;
- a second SAM/Streaming implementation;
- large production observability APIs created only for tests.

The environment remains:

```text
root/sudo              = not required
namespaces             = not required
Docker                 = not required
VM/Multipass           = not required
systemd                = not required
public I2P network     = not required
live NTCP2/SSU2        = not required
localhost TCP          = required
manual GitHub runner   = allowed
```

## 5. First task: make the evidence ledger executable

Before adding new tests, remove the possibility of synthetic closure results.

### 5.1 No unconditional success records

In `tests/integration/sam/run-independent.sh` and any Plan 151 wrapper:

- no required result may be emitted as `passed` unless the immediately associated command/test returned success;
- no required result may be marked passed solely because another plan/status file says it passed;
- no required result may be marked passed by an unconditional `record <label> passed ...` statement;
- aggregated results must be derived from lower-level executed rows.

A literal hard-coded `passed` is acceptable only for metadata that is not an acceptance result, and should preferably not use the same `record` function.

### 5.2 Add a static evidence-integrity check

Add a small static checker, preferably:

```text
scripts/check-sam-acceptance-evidence.sh
```

or equivalent narrowly scoped test, which rejects known dangerous patterns in the final harness, including required acceptance labels recorded unconditionally.

At minimum guard these final fields:

```text
multiple-stream-lifecycle
slow-reader
slow-writer
fault-data-drop
fault-ack-drop
fault-duplicate
fault-reorder
fault-corruption
fault-retransmit-ceiling
forward-lifecycle
plan127-134-regressions
```

Do not build a parser framework. A simple explicit shell/Python check is enough.

### 5.3 Evidence provenance

The final generated evidence must associate each required result with:

- executable command/test name;
- exit status/result;
- i2pr commit;
- execution lane;
- relevant exact external-client revision, if external;
- sanitized detail.

The generated summary may aggregate these rows, but aggregation cannot invent success.

## 6. Add the missing sibling-stream lifecycle acceptance

Add a real TCP-only black-box test, preferably in a new focused file such as:

```text
crates/i2pr-daemon/tests/sam_stream_final_acceptance.rs
```

or extend `sam_stream_self_composed.rs` only if the resulting file remains readable.

Required trajectory:

1. start the normal Plan 149 loopback listener;
2. create destination A and B through HELLO + `SESSION CREATE` only;
3. establish **two distinct simultaneous STREAM connections** between the same sessions/destinations through normal CONNECT/ACCEPT commands;
4. transfer unique binary payloads on both streams;
5. close stream 1;
6. prove stream 2 remains established and transfers another unique payload in both directions;
7. close stream 2;
8. close the control session;
9. assert session/destination/stream/driver/attachment counts return to baseline.

No private product setup is allowed after listener startup.

Acceptance must distinguish the two Streaming connection IDs and prove close/reset state for one does not contaminate its sibling.

## 7. Add slow-reader and slow-writer boundedness

The purpose is not throughput benchmarking. The purpose is to prove bounded memory/backpressure behavior at the real SAM TCP boundary.

Use a deliberately small test profile if that lets the test complete quickly, but do not alter production Streaming semantics.

### 7.1 Slow reader

Required behavior:

1. establish a real self-composed SAM STREAM;
2. receiver B deliberately stops reading its application TCP socket;
3. A attempts to send beyond the configured per-stream/application reservoirs;
4. assert retained/accounted bytes stay under explicit documented ceilings;
5. assert the writer stalls/backpressures or reaches a typed bounded condition rather than growing an unbounded queue;
6. resume B;
7. verify exact ordered bytes;
8. verify all queue/resource counters return to baseline after close.

### 7.2 Slow writer / reverse direction

Repeat the same pressure in the reverse direction or use a constrained sink so both halves of the raw bridge are exercised.

### 7.3 What may be inspected

The test may read non-secret counters/snapshots needed to prove boundedness. It may not manipulate Streaming state or move application payloads through private APIs after listener startup.

If no narrow read-only counter exists, add the smallest non-secret diagnostic snapshot required. Do not expose private keys, payload bytes, or mutation capability.

## 8. Close the deterministic fault matrix beneath real SAM sockets

Plan 151 must prove that the existing Streaming recovery semantics remain correct when the application trajectory starts at the real SAM TCP listener.

The preferred design is **pre-start fault configuration** below the SAM socket boundary:

```text
SAM TCP application clients
    ↓
normal Plan 149 SESSION CREATE / CONNECT / ACCEPT
    ↓
existing Streaming / destination delivery path
    ↓
deterministic test-only fault seam
```

Rules:

- configure faults before listener startup;
- after startup, behavior-driving interactions remain SAM TCP only;
- do not call private byte-moving/delivery APIs to advance the stream;
- no production dependency on `i2pr-testkit`;
- prefer a narrow test-only/local-fabric injection point over general production configurability;
- one deterministic fault per focused test where possible.

Required cases:

### 8.1 Drop one DATA packet

- drop one selected DATA transmission;
- prove retransmission occurs;
- receiver obtains exact bytes once and in order;
- no duplicate application delivery;
- stream remains usable after recovery.

### 8.2 Drop an ACK

- suppress one delayed/standalone ACK or equivalent selected acknowledgement;
- sender recovers through existing retransmission/ACK rules;
- exact bytes delivered once;
- no busy loop.

### 8.3 Duplicate DATA

- duplicate a selected DATA packet below the application boundary;
- receiver emits the bytes exactly once.

### 8.4 Reorder DATA

- reorder at least two selected DATA packets;
- application receives correct ordered byte stream.

### 8.5 Corrupt authenticated/ciphertext material

- corrupt one authenticated delivery below SAM;
- corrupted payload must not reach the application;
- failure must be bounded/typed;
- no subsequent secret/payload logging.

### 8.6 Retransmission ceiling

- force persistent non-delivery until the existing retransmission ceiling is reached;
- stream terminates/resets according to existing semantics;
- waiters wake;
- tasks and buffers release;
- no infinite retry/busy loop.

If any of these exposes a genuine M6 Streaming protocol defect, stop this plan and write one narrow protocol corrective plan. Do not weaken the expected result merely to close M7.

## 9. Finish CLOSE / RESET behavior

Add focused black-box cases for:

- graceful local raw TCP EOF -> appropriate Streaming CLOSE path;
- peer CLOSE -> application sees EOF only after accepted bytes are delivered;
- abrupt application/socket failure -> RESET/terminal behavior where appropriate;
- remote RESET -> prompt application termination;
- control socket/session EOF while raw streams are active -> child streams/drivers terminate within the shutdown bound;
- repeated create/connect/close cycles -> all registries/tasks/counters return to baseline.

Use exact bounded deadlines. Do not use unbounded sleeps.

## 10. Complete the FORWARD matrix

Retain the current positive external `i2psam` FORWARD + loopback echo result, then add executable evidence for the missing Plan 150 cases.

Required:

1. positive non-silent FORWARD with authenticated peer Destination metadata and exact bytes;
2. `SILENT=true` target receives raw bytes first, without metadata;
3. second independent forwarded stream succeeds;
4. target connection refusal returns/terminates within the existing deadline and leaks no attachment;
5. unreachable/timeout case terminates within the configured policy;
6. closing the owning control socket unregisters the forward;
7. ACCEPT/FORWARD mutual exclusion remains enforced;
8. non-loopback target remains rejected.

These may be split between external-client tests and focused Rust black-box tests when an external client API cannot express a negative condition. The positive product trajectory must remain external-client driven.

## 11. Explicitly rerun the Plan 127–134 Milestone 6 regression floor

Plan 150's contract required focused M6 evidence rather than relying only on aggregate workspace counts. Plan 151 must record those focused commands on the closing revision.

At minimum identify and run the canonical focused tests/status-listed commands for:

- Plan 127 destination session/routing;
- Plan 128 Streaming wire protocol;
- Plan 129 integrated destination/Streaming trajectory;
- Plan 130/131/132/133 retained final local trajectory/evidence tests as applicable;
- Plan 134 receive-window / ACK ceiling regression.

Do not invent test names. Resolve the actual existing test files/commands from the current repository and record them verbatim in `151-status.md`.

If an old plan's focused test was intentionally superseded/deleted, record the current superseding test and why it is authoritative.

## 12. Final external-client lane

After all local Plan 151 tests are green, rerun the existing Plan 150 external-client workflow unchanged in client provenance:

```text
bash scripts/interop/fetch-sam-clients.sh --rebuild
bash tests/integration/sam/clients/build.sh
bash tests/integration/sam/run-independent.sh
```

Update `run-independent.sh` so it invokes the new Plan 151 acceptance suite(s) and derives its final rows from their actual result.

The manual GitHub workflow must then run the exact closing revision and upload sanitized evidence.

Required final evidence rows include at least:

```text
external-client-a-to-b
external-client-b-to-a
private-destination
binary-matrix
silent
naming
negative-matrix
forward-positive
forward-lifecycle
sibling-stream-isolation
slow-reader
slow-writer
fault-data-drop
fault-ack-drop
fault-duplicate
fault-reorder
fault-corruption
fault-retransmit-ceiling
close-reset-lifecycle
plan127-134-regressions
workspace-gates
```

Each row must come from an executed command/test.

## 13. Privacy and evidence hygiene

Retain the current evidence restrictions:

Never commit/upload:

- raw SAM `PRIV` strings;
- signing seeds/static private keys;
- raw secret application payloads;
- unsanitized environment dumps;
- temp source trees for external clients.

Allowed evidence:

- hashes/digests;
- byte counts;
- result categories;
- exact source revisions;
- command names;
- non-secret IDs/counters;
- timing/deadline outcome categories.

Add a default-log regression if one does not already prove that the new failure paths do not log payload/private material.

## 14. Expected files

Likely implementation/evidence changes:

```text
crates/i2pr-daemon/tests/sam_stream_final_acceptance.rs   # preferred new focused suite
crates/i2pr-daemon/tests/sam_stream_self_composed.rs      # only if small extensions fit
crates/i2pr-daemon/tests/sam_forward_naming.rs            # missing forward negatives/lifecycle
crates/i2pr-daemon/src/sam.rs                             # only if a concrete lifecycle bug is exposed
crates/i2pr-daemon/src/sam/raw_stream.rs                  # only if a concrete lifecycle/backpressure bug is exposed
crates/i2pr-daemon/src/sam/fabric.rs                      # narrow pre-start test fault seam if needed
crates/i2pr-client/src/streaming/*                        # only if a concrete M6 defect is proven

tests/integration/sam/run-independent.sh
scripts/check-sam-acceptance-evidence.sh
tests/integration/sam/evidence.md
.github/workflows/sam-external.yml                        # only to invoke/check the final lane

plans/151-status.md
plans/150-status.md
plans/145-status.md
plans/149-status.md                                       # remove stale contradictory wording only
plans/README.md
README.md
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
```

Avoid touching unrelated NTCP2/SSU2 code merely to make CI stable. If a pre-existing cross-platform flaky test fails, fix it separately and document why it is unrelated.

## 15. Acceptance criteria

Plan 151 closes only when **all** are true:

1. Plan 149 self-composition remains green without private post-start setup;
2. valid Plan 150 external-client core evidence remains green;
3. no required final evidence result is hard-coded/unconditionally marked `passed`;
4. an evidence-integrity checker rejects synthetic pass bookkeeping for required rows;
5. two simultaneous sibling STREAMs are proven through the real listener;
6. closing one sibling leaves the other usable;
7. slow-reader pressure stays within explicit bounded reservoirs and recovers exactly;
8. slow-writer/reverse pressure stays bounded and recovers exactly;
9. one dropped DATA packet recovers by retransmission with exact-once bytes;
10. one dropped ACK recovers without unbounded retry;
11. duplicate DATA produces no duplicate application bytes;
12. reordered DATA is delivered to the application in order;
13. corrupted authenticated/ciphertext material produces no application delivery;
14. retransmission ceiling causes bounded terminal behavior and resource release;
15. graceful CLOSE behavior is proven at the SAM application boundary;
16. RESET/abrupt failure behavior is proven and bounded;
17. active streams terminate cleanly on owning session/control teardown;
18. repeated lifecycle cycles return all relevant registries/tasks/counters to baseline;
19. FORWARD positive, silent, second-stream, refusal/timeout, unregister, exclusion, and loopback-only policy are executable evidence;
20. private-destination, NAMING, negative SAM matrix, SILENT, and exact external binary trajectories remain green;
21. the actual current Plan 127–134 focused regression commands are run and recorded;
22. workspace format/check/test/clippy/doc/boundary/deny gates pass;
23. the manual external-client workflow passes on the exact closing head;
24. generated/committed evidence is sanitized and records exact provenance;
25. Plan 150 status is retained as successful external-client core evidence but superseded for final acceptance by Plan 151;
26. only after 1–25 pass does `151-status.md` set `milestone7_sam_localhost = passed` and `next_product_layer = Milestone 8 planning`.

No waiver text may convert an unexecuted acceptance item into `passed`.

## 16. Validation floor

Resolve exact focused M6 test names from the repository, then run at minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1   # if created
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- --test-threads=1
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/interop/fetch-sam-clients.sh --rebuild
bash tests/integration/sam/clients/build.sh
bash tests/integration/sam/run-independent.sh
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
cargo deny check advisories bans sources
```

Also run the resolved focused Plan 127–134 commands and list them individually in `plans/151-status.md`.

## 17. Stop conditions

Stop and write one narrower corrective plan instead of weakening Plan 151 if:

- a fault test exposes an M6 Streaming protocol defect;
- sibling isolation requires redesigning Streaming connection ownership;
- bounded slow-peer behavior cannot be achieved without changing existing buffer/window architecture;
- correct CLOSE/RESET semantics are absent rather than merely untested;
- FORWARD cleanup exposes a broader session-ownership defect;
- the external clients disagree on required SAM behavior and the official SAM/reference behavior does not resolve the disagreement.

Do **not** respond to a stop condition by deleting the test, increasing an unbounded timeout, or changing expected output to match the defect.

## 18. Closure record requirements

`plans/151-status.md` must contain:

- exact closing commit;
- current-head routine CI run id/result;
- current-head SAM external workflow run id/result;
- exact external client revisions and counted/non-counted roles;
- each Plan 151 acceptance row with the exact test/command that proves it;
- explicit focused Plan 127–134 commands/results;
- resource ceiling values used for slow-peer tests;
- fault scenario -> expected -> observed table;
- FORWARD matrix table;
- lifecycle matrix table;
- privacy/log result;
- known limitations;
- final qualified claim language.

If any row is not passed, status stays open/blocked and Milestone 8 remains non-executable.

## 19. Handoff

Execute **Plan 151 only**.

Do not begin Milestone 8 implementation from the current Plan 150 closure label. Preserve the working Plan 149 architecture and valid Plan 150 external-client results, add the missing executable acceptance evidence, rerun the exact hosted lane, and close Milestone 7 only when the evidence ledger and tests agree.