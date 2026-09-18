# Plan 071: repeated i2pd development validation and negative controls

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 067.
- Requires Plan 070 closed with one real pass in each i2pd direction.
- Plan type: repeated fresh-state interoperability, bounded negative controls, and development-validation closure.
- This plan is the mandatory development-validation gate for continuing later protocol/router work under Plan 067.
- It is not release qualification.

## Objective

Demonstrate that i2pr's NTCP2 interoperability with pinned i2pd 2.60.0 is stable enough for continued development rather than a one-off smoke success.

The plan must produce:

- three independent fresh-state passes for `i2pr-to-i2pd-ipv4`;
- three independent fresh-state passes for `i2pd-to-i2pr-ipv4`;
- bounded negative controls that fail at the expected stage;
- repeated cleanup and network-audit results;
- one Level 2 development-validation summary;
- an explicit statement that Java/release qualification remains open.

## Fresh-state rules

Every positive and negative run uses new:

- run ID;
- run directory;
- i2pr identity and NTCP2 static key/IV;
- i2pd identity and NTCP2 static key/IV;
- listener ports;
- DeliveryStatus message ID;
- RouterInfo files;
- event files/cursors;
- mutable reference data directory;
- process groups.

No run may copy or reuse a previous writable state directory. Read-only pinned source and built binaries may be reused when their digests remain unchanged.

## Positive matrix

Required matrix:

| Direction | Required passes | Failure allowance |
| --- | ---: | --- |
| `i2pr-to-i2pd-ipv4` | 3/3 | none |
| `i2pd-to-i2pr-ipv4` | 3/3 | none |

If a run fails:

1. preserve the record and sanitized diagnostics;
2. classify the earliest stage;
3. determine whether the result is deterministic, flaky, or environmental;
4. correct the owning defect through a bounded commit;
5. restart the entire 3/3 sequence for the affected direction from fresh state.

Do not count a pass produced before a code/config/reference change toward the restarted sequence.

## Positive pass predicate

Each run must satisfy every Level 1 predicate from Plans 068-070 plus:

- distinct run ID and message ID from every other run;
- distinct mutable-state digests from every other run;
- identical source commit for all six accepted runs;
- identical i2pd source revision and driver binary digest;
- no cleanup warning;
- no protocol retry hidden by the runner;
- no address-in-use retry except the single preflight allocation retry permitted by Plan 069;
- no synthetic events or fixture substitution;
- no external destination observed when strace auditing is active.

Direction ordering may vary. Results must not depend on running all initiator cases first or on a warmed listener.

## Negative-control matrix

### N1. Network ID mismatch

Setup:

- one side uses network ID 99;
- the other uses a different explicit test network ID.

Expected:

- connection does not produce a passing authenticated data phase;
- failure occurs at RouterInfo validation or handshake identity/network validation;
- no DeliveryStatus decode is observed;
- cleanup remains clean.

### N2. RouterInfo/static-key mismatch

Setup:

- provide the valid peer RouterInfo identity/address but replace or bind the strict scenario to a different NTCP2 `s` key or IV digest without altering the signed RouterInfo bytes accepted by the peer.

Preferred implementation:

- reject before launch when the strict config does not match RouterInfo;
- when a runtime control is needed, use a separately signed RouterInfo from another fresh identity rather than mutating signed bytes.

Expected:

- preflight or handshake authentication failure;
- never a data-phase pass;
- no bypass of signature verification.

### N3. DeliveryStatus correlation mismatch

Setup:

- sender emits a valid authenticated DeliveryStatus with message ID A;
- scenario/receiver expects message ID B.

Expected:

- NTCP2 authentication and frame decryption may succeed;
- receiver rejects the direction at `correlation`;
- exact wrong observed ID is recorded safely;
- generic type-10 detection cannot pass.

### N4. Malformed or unauthenticated data-phase frame

Setup:

- use the existing i2pr test/interop seam to alter one bounded frame after handshake without patching reference cryptography;
- preferred controls are bad AEAD tag, impossible length, or invalid block framing.

Expected:

- receiver does not report authenticated I2NP decode;
- failure occurs at `data-frame-authentication` or `i2np-decode` as appropriate;
- session closes boundedly;
- no crash or process leak.

Do not add a general packet mutation framework. Implement one deterministic bounded control.

### N5. Replay or stale handshake control

Execute only when the existing testkit/launcher can provide it without broad runner expansion.

Preferred controls:

- replay a captured same-test SessionRequest within the private run seam; or
- render a timestamp outside the accepted skew.

Expected:

- handshake rejected;
- no authenticated data phase;
- cleanup clean.

If this control cannot be executed through an existing bounded seam, record it as `deferred-existing-local-coverage` and cite the relevant Level 0 replay/skew tests. N1-N4 remain mandatory.

## Stability observations

Collect bounded non-secret metrics for each positive run:

```text
process readiness duration
TCP connect duration
handshake duration
time to exact I2NP decode
total run duration
bytes written/read when already available
cleanup duration
network audit mode
```

Purpose:

- identify gross nondeterminism;
- detect timeout budgets that pass only by chance;
- detect cleanup degradation.

This is not a performance benchmark. Do not introduce throughput, percentile infrastructure, or long-duration load testing.

## Timeout policy

Use Plan 069 defaults initially. A timeout increase requires:

- evidence that the process is progressing rather than deadlocked;
- one documented reason;
- bounded new value;
- no increase above twice the original stage default under this plan.

Do not make timeouts unbounded or globally increase every stage to hide one failure.

## Network-audit policy

For six positive runs:

- prefer `strace-allowlist` when host permits it;
- otherwise all may use `configuration-only` with the exact reason recorded;
- mixed audit modes are permitted but summarized explicitly.

For negative runs:

- an external destination is always a failure independent of expected protocol rejection;
- control-induced local socket errors are permitted only on declared loopback endpoints.

## Cleanup policy

Every run must verify:

- process groups gone;
- listener ports closed;
- no owned lock/pid files active;
- no writable state reused by later runs;
- diagnostics file handles closed;
- temporary run roots removed unless a failed run is explicitly retained for sanitized investigation.

Any cleanup failure invalidates that run and restarts the relevant sequence after correction.

## Deliverables

### D1. Repetition driver

Add the smallest orchestration wrapper around Plan 069, recommended:

```text
scripts/interop/run-ntcp2-i2pd-development-validation.py
```

or a bounded subcommand in `loopback_smoke.py` if that remains clearer.

Responsibilities:

- lock source commit/reference binary digest at start;
- execute 3/3 fresh runs per direction;
- verify run independence;
- stop on first positive failure;
- execute N1-N4 controls;
- optionally execute N5;
- write one Level 2 summary;
- never retry protocol failures silently.

Do not implement a generic test scheduler.

### D2. Independence validator

Validate across accepted positive records:

- unique run IDs;
- unique message IDs;
- unique mutable-state digests;
- no reused event/record file digest where content should differ;
- same source commit;
- same reference revision/binary digest;
- same evidence tier;
- correct direction counts.

This may be a small pure helper with table-driven tests.

### D3. Negative control fixtures/config

Add explicit strict control configuration rather than hidden environment variables.

Recommended values:

```text
network-id-mismatch
router-info-key-mismatch
delivery-status-id-mismatch
bad-data-frame-authentication
stale-handshake
```

The runner rejects unknown controls and allows at most one control per invocation.

### D4. Development summary

Write:

```text
tests/integration/ntcp2/qualification/i2pd-development-validation.json
```

using the Plan 068 Level 2 schema.

Required content:

- source commit;
- reference source revision;
- instrumented/control binary digests;
- six positive record digests;
- positive pass counts;
- N1-N4 results and expected stages;
- N5 result or bounded deferral reason;
- cleanup summary;
- network-audit summary;
- status;
- explicit `release_qualified = false` or equivalent non-promotable semantics.

### D5. Support and milestone status

On successful closure, update active documentation to state:

```text
development_validation = passed-i2pd-level2
release_qualification = pending-java-and-isolated-i2pd
support = experimental
advertised = false
```

This result permits planning/implementation of later milestones, but does not permit production advertisement or public-network operation.

### D6. Closure record

Create:

```text
plans/071-status.md
```

Record:

- accepted source commit;
- exact six run IDs and record digests;
- exact control results;
- audit modes;
- any defect fixes and rerun resets;
- exact validation commands;
- explicit Level 2/non-release status;
- whether Plan 072 entry criteria are met;
- Plan 073 remains deferred until a suitable environment exists.

## Required tests

Add tests covering at least:

1. 3/3 matrix accepted;
2. 2/3 rejected;
3. duplicate run ID rejected;
4. duplicate message ID rejected;
5. reused mutable-state digest rejected;
6. source commit drift rejected;
7. reference binary drift rejected;
8. wrong direction count rejected;
9. smoke schema accepted as input but only through validated Level 1 parser;
10. synthetic or fixture pass rejected;
11. cleanup failure rejects sequence;
12. external destination rejects sequence;
13. N1 expected stage accepted;
14. N1 accidental data-phase success rejected;
15. N2 preflight rejection accepted;
16. N2 signature/identity bypass rejected;
17. N3 frame decode plus correlation rejection accepted;
18. N3 generic type-only pass rejected;
19. N4 authentication failure accepted;
20. N4 process crash without typed result rejected;
21. N5 valid rejection accepted;
22. bounded N5 deferral accepted only with cited Level 0 coverage;
23. unknown control rejected;
24. multiple controls in one invocation rejected;
25. summary cannot satisfy release validator;
26. duration fields remain bounded/nonnegative;
27. timeout increase over plan bound rejected.

Recommended focused files:

```text
tests/integration/ntcp2/harness/test_i2pd_development_validation.py
tests/integration/ntcp2/harness/test_development_validation.py
tests/integration/ntcp2/harness/test_ntcp2_negative_controls.py
```

## Validation commands

Focused:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_development_validation.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_development_validation.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_ntcp2_negative_controls.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
```

Manual execution command must be finalized by implementation, for example:

```bash
python3 scripts/interop/run-ntcp2-i2pd-development-validation.py \
  --reference-driver target/interop/i2pd-driver/i2pd_ntcp2_interop_driver_instrumented \
  --control-driver target/interop/i2pd-driver/i2pd_ntcp2_interop_driver_control \
  --output-root target/interop/development-validation
```

Closure baseline:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-vectors.sh
git diff --check
```

Run focused fuzz/tests for any production protocol defect corrected during the repetition campaign.

## Plan 072 entry criteria

Plan 072 becomes active only when one of these is true:

- a repeatable i2pd disagreement remains ambiguous after specification and source inspection;
- i2pr passes one direction but persistently fails the other and an independent third implementation would localize ownership;
- the maintainer explicitly requests additional differential confidence after Plan 071 passes.

If Plan 071 passes cleanly and no ambiguity remains, Plan 072 may be recorded `not-needed-for-current-development-gate` and skipped without blocking continuation.

## Non-goals

Plan 071 does not:

- run Java;
- require Emissary;
- close release qualification;
- advertise NTCP2;
- add CI;
- perform load or longevity testing;
- test IPv6;
- test public network behavior;
- expand into tunnels/NetDB/SAM/I2CP/SSU2.

## Stop rules

Stop and record a typed blocker when:

- positive runs attempt undeclared networking;
- the same source commit cannot reproduce both directions;
- exact identity/message correlation is unavailable;
- a negative control requires bypassing cryptographic validation;
- cleanup remains unreliable after one bounded correction attempt;
- repeated failures indicate a production defect too large for a bounded Plan 071 correction; in that case create a narrow corrective plan rather than expanding this one;
- implementation begins building a generalized chaos/performance framework.

## Closure criteria

Plan 071 closes only when:

- six fresh-state positive passes exist at one source commit;
- N1-N4 reject at expected stages;
- N5 passes or is narrowly deferred with existing Level 0 coverage cited;
- all records validate and are independent;
- cleanup passes every run;
- network audit is explicit;
- Level 2 summary is written and non-promotable;
- active Milestone 3 status records development validation passed and release qualification open;
- Plan 072 activation decision is recorded;
- `plans/071-status.md` contains exact evidence references;
- NTCP2 remains experimental/non-advertised.

## Small-model handoff instructions

- Do not change the Level 1 runner unless a real repetition defect requires it.
- Implement repetition as a thin loop, not a scheduler.
- Keep each negative control explicit and deterministic.
- Stop on the first unexpected positive-run failure.
- Restart the affected 3/3 sequence after any code/config/reference change.
- Do not combine multiple negative mutations.
- Do not treat timing metrics as a benchmarking project.
- State clearly that Level 2 permits continued development but is not release qualification.
