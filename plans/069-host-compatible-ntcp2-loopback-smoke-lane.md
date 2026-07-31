# Plan 069: host-compatible NTCP2 loopback smoke lane

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 067.
- Requires Plan 068 closed.
- Must close before Plan 070 live i2pd execution.
- Plan type: test/integration runner implementation.
- This plan does not claim mixed-router interoperability by itself; it creates the lane used by Plan 070.

## Objective

Implement the smallest safe, useful, non-authoritative two-process NTCP2 runner that can execute on the current Ubuntu host without unprivileged namespaces or Multipass.

The lane must:

1. run one i2pr process and one reference-driver process over host loopback;
2. use fresh temporary state and ephemeral ports;
3. exchange signed RouterInfos directly through an owned run directory;
4. disable every public-network, bootstrap, client, tunnel, and alternate-transport path;
5. preserve Plan 065 exact DeliveryStatus and Router Hash correlation;
6. record the earliest real protocol/runtime failure stage;
7. perform bounded process teardown and residual checks;
8. optionally audit network syscalls with strace;
9. write one concise Level 1 smoke record;
10. remain structurally incapable of producing a Level 3 release bundle or certificate.

## Target topology

```text
127.0.0.1:<ephemeral-i2pr> <------ NTCP2 ------> 127.0.0.1:<ephemeral-reference>
        i2pr process                              reference driver process
```

There are exactly two processes in the primary smoke topology.

The run uses:

- network ID 99;
- IPv4 only;
- X25519 NTCP2 mode supported by the pinned references;
- one direct peer RouterInfo per process;
- one exact DeliveryStatus I2NP message;
- one direction per invocation;
- no default peer list;
- no reseed;
- no DNS lookup;
- no public RouterInfo publication;
- no SSU2;
- no UDP;
- no UPnP;
- no SAM;
- no I2CP;
- no I2PControl/HTTP trigger;
- no tunnels;
- no floodfill;
- no NetDB bootstrap.

## Direction model

The runner supports exactly these initial directions:

```text
i2pr-to-i2pd-ipv4
i2pd-to-i2pr-ipv4
```

The runner interface may be designed so Java or Emissary can be added later, but Plan 069 must not implement them.

One invocation runs one direction. Plan 070 owns executing both directions and interpreting the result.

## Deliverables

### D1. Loopback runner module

Create:

```text
tests/integration/ntcp2/harness/loopback_smoke.py
```

Responsibilities:

- parse a strict bounded CLI/config;
- allocate a run ID;
- create one owned temporary run root;
- allocate nonconflicting loopback ports by binding sockets before process launch or through another race-minimizing method;
- render i2pr and reference strict configs;
- generate or request fresh identities;
- coordinate RouterInfo export/import;
- launch the listener before the dialer;
- enforce readiness, handshake, data-phase, and total-run deadlines;
- capture structured events only;
- determine the earliest failure stage;
- request graceful shutdown and then bounded termination/kill fallback;
- check listener ports and child PIDs are gone;
- write the Plan 068 smoke record;
- delete the temporary state by default after the record is finalized;
- preserve sanitized diagnostics only when explicitly requested.

The module must not import or call Plan 056/066 candidate, bundle, certificate, rootless-topology, or Multipass authority.

### D2. Thin shell entry point

Create:

```text
scripts/interop/run-ntcp2-loopback-smoke.sh
```

Responsibilities:

- locate repository root;
- verify required binaries/config inputs;
- invoke the Python runner;
- pass through the runner exit status;
- avoid build, package installation, source fetching, sudo, namespace creation, and VM operations.

Required usage shape:

```bash
bash scripts/interop/run-ntcp2-loopback-smoke.sh \
  --direction i2pr-to-i2pd-ipv4 \
  --reference-driver <path> \
  --output <smoke-record.json>
```

The exact option names may differ only when the existing harness conventions strongly justify it.

### D3. Strict runner configuration

Use a small strict config type. Required fields:

```text
direction
reference_driver_binary
reference_build_manifest
reference_source_lock
output_record
source_commit
run_timeout_seconds
readiness_timeout_seconds
handshake_timeout_seconds
data_timeout_seconds
network_audit_mode
diagnostics_mode
```

Allowed audit modes:

```text
auto
strace
configuration-only
```

Allowed diagnostics modes:

```text
off
sanitized
```

Unknown fields/options fail. Raw protocol payload capture is not supported.

### D4. Process adapters

Reuse existing adapters where they are already correct:

```text
tests/integration/ntcp2/harness/i2pr.py
tests/integration/ntcp2/harness/i2pd_direct_driver.py
tests/integration/ntcp2/harness/launcher_renderer.py
tests/integration/ntcp2/harness/launcher_protocol.py
```

Do not route the smoke invocation through `mixed_runner.py` if doing so imports release/candidate/isolation policy. Extract only small shared helpers when needed.

Required adapter behaviors:

- i2pr listener and dialer use the strict scenario-v2 contract;
- reference driver uses the Plan 064 strict config contract;
- both sides receive the same run identity, expected Router Hashes, and DeliveryStatus message ID;
- structured events are read from owned files or pipes;
- generic logs cannot produce a pass.

### D5. Port allocation and readiness

Port allocation must avoid a long-lived race-prone global port registry.

Preferred strategy:

1. bind a temporary loopback socket to port 0;
2. obtain the assigned port;
3. close it immediately before listener launch;
4. start listener and require readiness within a short deadline;
5. retry the entire fresh run at most once on typed `address-in-use` preflight failure.

Do not silently retry protocol failures.

Readiness requires:

- process alive;
- structured `listener_ready` event when reference supports it;
- expected RouterInfo written and validated;
- TCP listener observable on the assigned loopback endpoint.

### D6. Direct RouterInfo exchange

Each process writes its own signed RouterInfo under the run root.

The runner validates before peer import:

- file is regular and bounded;
- signature verifies through the existing strict validator;
- network ID is 99;
- Router Hash matches expected identity;
- exactly one relevant NTCP2 IPv4 address exists;
- host is loopback;
- port matches the run allocation;
- `s` and `i` values are present and valid for the selected NTCP2 address;
- no SSU2 address is selected as NTCP2 authority.

The peer imports the exact validated bytes/digest. Do not reconstruct RouterInfo from individual fields.

### D7. Correlation authority

Reuse the Plan 065 domain-separated DeliveryStatus message-ID derivation or move it into one small shared helper consumed by both runners.

A passed run requires:

```text
scenario.delivery_status_message_id
== trigger.delivery_status_message_id
== sender.delivery_status_message_id
== receiver.delivery_status_message_id
```

It also requires expected sender/receiver Router Hash continuity across:

- RouterInfo validation;
- strict scenario;
- reference trigger/event;
- i2pr counters/events;
- smoke record.

### D8. Network audit

When `strace` is available and permitted:

- launch both processes under `strace -ff -e trace=network -yy` or an equivalent bounded invocation;
- parse only network syscall destination metadata;
- allow IPv4 loopback endpoints declared by the run;
- allow local Unix sockets only when explicitly used by the runner/runtime;
- reject any external IPv4/IPv6 destination, DNS socket, or undeclared listener;
- retain a sanitized summary, not full raw traces, by default.

When strace is unavailable or ptrace is denied:

- use `network_audit = configuration-only`;
- verify all bootstrap/discovery/alternate-transport configuration is disabled;
- record the absence of syscall-level proof;
- allow the run to remain Level 1 diagnostic evidence.

A passed record may not use `network_audit = not-run`.

### D9. Deadlines

Define bounded defaults suitable for a stressed local host:

```text
process readiness: 20 seconds
RouterInfo exchange: 20 seconds
TCP connect: 15 seconds
NTCP2 handshake: 30 seconds
data phase: 20 seconds
graceful cleanup: 15 seconds
total run: 120 seconds
```

These defaults may be configurable within documented bounds. Do not add unbounded waits or provider-style exponential backoff.

### D10. Cleanup

Cleanup order:

1. request graceful shutdown from dialer/reference as supported;
2. request graceful shutdown from listener/i2pr;
3. wait boundedly;
4. send TERM to remaining owned process groups;
5. wait boundedly;
6. send KILL only to remaining owned process groups;
7. verify PIDs are absent;
8. verify assigned listener ports no longer accept connections;
9. close runner-owned files/pipes;
10. remove temporary state unless diagnostics preservation was requested.

Any cleanup failure overrides a protocol pass.

### D11. Failure staging

Map failures into the Plan 068 smoke stages. Preserve specific underlying typed reason codes.

Examples:

```text
build: reference binary/manifest unavailable
process-start: binary exits before readiness
router-info: signature/network/address/static-key mismatch
connect: TCP refused/timeout
handshake-request: initiator fails before valid SessionRequest completion
handshake-created: responder/initiator rejects SessionCreated
handshake-confirmed: SessionConfirmed or peer identity failure
data-frame-write: sender cannot queue/write exact message
frame-authentication: receiver AEAD/frame validation failure
i2np-decode: authenticated frame but invalid/missing I2NP
correlation: wrong/duplicate DeliveryStatus ID or Router Hash
cleanup: residual PID/socket/state
network-audit: undeclared destination observed
timeout: stage-specific deadline
```

The runner must not collapse all failures into `evidence-incomplete` or `blocked_execution_lane_unavailable`.

### D12. Tests

Create:

```text
tests/integration/ntcp2/harness/test_loopback_smoke.py
```

Use fake subprocesses, temporary files, and local sockets for runner-control tests. Do not require the real i2pd binary in unit tests.

Required cases include:

1. valid strict config;
2. unknown option rejected;
3. unsupported direction rejected;
4. fresh run root created;
5. output path outside owned/declared destination rejected where applicable;
6. unique nonzero message ID generated;
7. listener starts before dialer;
8. readiness timeout staged correctly;
9. invalid RouterInfo staged correctly;
10. wrong network ID rejected;
11. non-loopback RouterInfo address rejected;
12. wrong port rejected;
13. wrong static key/IV authority rejected;
14. exact message correlation accepted;
15. message mismatch rejected;
16. Router Hash mismatch rejected;
17. handshake-only result rejected;
18. sender-only result rejected;
19. clean pass record written;
20. cleanup failure overrides pass;
21. external network destination rejects run;
22. strace unavailable degrades to configuration-only;
23. raw diagnostics option rejected;
24. release bundle/certificate helper not imported or selected;
25. run root removed after normal completion;
26. bounded address-in-use retry occurs once only;
27. protocol failure is not retried;
28. child process group receives bounded teardown.

### D13. Static boundary check

Add a focused script or extend the existing interoperability check narrowly to verify:

- loopback runner exists;
- supported directions are only the two i2pd directions;
- release candidate/certificate modules are not imported by the smoke runner;
- rootless/Multipass modules are not imported by the smoke runner;
- raw diagnostics are unavailable;
- public bootstrap features are explicitly disabled in rendered configs;
- smoke schema is used;
- exact correlation fields are required.

Do not check plan prose, test class names, or exact function layout.

### D14. Documentation and status

Update:

```text
tests/integration/ntcp2/README.md
docs/architecture/interop-apparatus.md
AGENTS.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

Create at closure:

```text
plans/069-status.md
```

The status record must include:

- exact implementation commit;
- runner command examples;
- focused tests and results;
- no real i2pd pass claim unless one was incidentally executed and separately recorded;
- next plan: 070.

## Work packages

### WP1. Extract minimal shared correlation helpers

- inspect Plan 065 message-ID and strict field derivation;
- move only reusable pure helpers if necessary;
- retain existing behavior and tests.

Acceptance:

- mixed runner and smoke runner derive identical correlation fields;
- no broad runner refactor.

### WP2. Implement run context and config

- strict CLI/config;
- run root;
- port allocation;
- deadline model;
- output writer.

Acceptance:

- unit tests pass without reference binaries;
- no candidate/isolation dependency.

### WP3. Implement process sequencing and RouterInfo exchange

- listener launch;
- readiness;
- RouterInfo validation/import;
- dialer launch;
- structured event collection.

Acceptance:

- fake-process positive fixture reaches data-stage evaluation;
- earliest failure stage preserved.

### WP4. Add network audit and cleanup

- strace allowlist mode;
- configuration-only fallback;
- process-group cleanup;
- port/PID residual checks.

Acceptance:

- external destination fixture fails;
- cleanup failure overrides pass.

### WP5. Integrate documentation and status

- shell entry point;
- README/architecture/skill guidance;
- focused boundary check;
- Plan 069 status record.

## Validation commands

Focused:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
```

Use the actual script name if the boundary check is integrated elsewhere.

Closure baseline:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

A real reference run is not required to close Plan 069; Plan 070 owns it.

## Non-goals

Plan 069 does not:

- build i2pd;
- modify the i2pd observer patch;
- run Java or Emissary;
- perform repeated interoperability;
- execute negative protocol controls against a real reference;
- modify production NTCP2 code;
- produce release evidence;
- use sudo, namespaces, containers, VMs, or public network access;
- add CI.

## Stop rules

Stop and record a typed blocker when:

- the existing direct-driver interfaces cannot be invoked without importing release-only authority;
- loopback address rejection in either implementation cannot be disabled through test-only configuration without altering protocol behavior;
- the runner would need to patch cryptography or acceptance logic;
- exact structured events cannot be consumed without generic log parsing;
- clean process ownership cannot be established;
- implementation expands into reference build fixes owned by Plan 070.

## Closure criteria

Plan 069 closes only when:

- the runner and shell entry point exist;
- the runner supports both i2pd directions;
- fresh state, loopback endpoints, direct RouterInfo exchange, deadlines, correlation, audit, and cleanup are implemented;
- no release/candidate/rootless/Multipass dependency exists;
- smoke records cannot satisfy Level 3;
- focused tests pass;
- closure baseline passes;
- documentation is updated;
- `plans/069-status.md` records exact results and no fabricated live pass.

## Small-model handoff instructions

- Implement one work package at a time.
- Prefer composition of existing adapters over a generic orchestration framework.
- Keep the runner in one primary Python module until a concrete size/ownership reason requires a second.
- Do not redesign `mixed_runner.py`.
- Do not add Java/Emissary options.
- Use fake-process tests for orchestration; do not make unit tests depend on CMake or i2pd.
- Preserve the first real failure stage even when record finalization also fails.
