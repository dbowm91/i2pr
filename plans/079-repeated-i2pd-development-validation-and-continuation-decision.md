# Plan 079: repeated i2pd development validation and continuation decision

## Status and dependencies

- Status: planned (blocked by Plan 078 closed-as-blocked-protocol-defect; see `plans/080-status.md`).
- Parent roadmap: Plan 074.
- Requires Plan 078 closed with one genuine pass in each direction.
- Supersedes Plan 071 as the active repeated i2pd development-validation plan.
- Plan type: repeated fresh-state validation, bounded negative controls, and Milestone 3 development-continuation decision.

## Objective

Establish that the first two-way i2pd result is reproducible and that major rejection/correlation controls fail for the correct reasons, without expanding into release-grade certification.

A passing Plan 079 permits continued later implementation work while NTCP2 remains experimental, non-advertised, and disabled in normal daemon operation. It does not close Java compatibility or final Milestone 3 release qualification.

## Positive matrix

Run three independent fresh-state passes for each direction:

```text
i2pr-to-i2pd-ipv4: 3/3
i2pd-to-i2pr-ipv4: 3/3
```

Every run must use:

- new run ID;
- new mutable state directories;
- new identities unless the exact test contract requires a controlled identity repetition;
- new ports;
- new nonzero DeliveryStatus message ID;
- the same verified source commit and reference revision;
- the same selected execution-lane class;
- exact binary and image/guest digests;
- clean teardown before the next run.

A retry of a failed run does not replace the failed record. Preserve all attempts and require three actual passing records per direction.

## Required proof per positive run

Each pass must independently prove:

- real reference process execution;
- exact source and binary provenance;
- signed RouterInfo and Router Hash continuity;
- TCP connection through the intended lane;
- NTCP2 authentication on both sides;
- successful encrypted frame write;
- receiver frame authentication/decryption;
- exact DeliveryStatus I2NP decode;
- exact message ID and sender Router Hash;
- no public network interface/route in execution lane;
- clean process/socket/state teardown.

## Negative controls

Run at least these bounded controls using fresh state:

### N1. Network ID mismatch

One peer uses an incorrect private network ID. Require deterministic rejection before a passing authenticated session/data record.

### N2. RouterInfo/static-key mismatch

Supply a RouterInfo whose NTCP2 static key or identity binding differs from the expected peer. Require authentication or identity validation failure.

### N3. DeliveryStatus ID mismatch

Send or expect a different nonzero message ID. Require correlation failure; no passing record.

### N4. Duplicate DeliveryStatus

Deliver the primary ID more than once or replay the same correlated event fixture through the real bounded test seam where practical. Require duplicate rejection.

### N5. Malformed or unauthenticated data frame

Use a test-only bounded mutation at the owning frame test seam, not a reference cryptography patch. Require frame authentication/decode failure.

### N6. Stale/replay handshake where practical

Exercise existing local replay/timestamp controls and, only if the real lane supports it without large new machinery, one reference-level stale/replay attempt. A local deterministic control is acceptable when a live replay would require intrusive packet capture/reinjection.

### N7. External-network boundary control

Demonstrate that the selected lane cannot create or route an external connection during execution. This may be a lane-level control rather than an i2pr protocol process attempt.

### N8. Cleanup failure control

Use a controlled fake/stubborn child in harness tests to prove cleanup failure overrides protocol pass. Do not sabotage a real reference run merely to create residue.

## Development-validation summary

Use the Plan 068 development-validation schema or a compatible corrected version.

The summary must bind:

```text
source_commit
reference_revision
execution_lane
lane_qualification_sha256
i2pr_binary_sha256
i2pd_instrumented_binary_sha256
i2pd_control_binary_sha256
positive_run_record_sha256[]
negative_control_record_sha256[]
all_positive_runs_passed
all_required_negative_controls_passed
cleanup_complete
status
summary_sha256
```

Allowed status:

```text
level2-passed
failed
blocked
```

No summary may claim release qualification.

## Failure policy

When a repetition fails:

1. preserve the failed record;
2. compare against the immediately preceding pass;
3. determine whether the difference is identity/state/port/timing/lane/protocol;
4. reproduce once from fresh state;
5. correct only a demonstrated deterministic defect;
6. restart the affected 3/3 sequence after any code, driver, observer, or lane-image change.

Do not hide intermittent failure by increasing retries or timeouts without measured evidence.

Reasonable timeout adjustments are allowed only when:

- the slow stage is identified;
- no protocol state timeout is weakened beyond specification requirements;
- the adjustment is bounded and documented;
- all subsequent runs use the same value.

## Continuation decision

### `level2-passed`

May be declared only when:

- six required positive runs pass;
- required negative controls pass;
- records validate and bind exact artifacts;
- cleanup is clean;
- lane guarantees remain valid;
- no observer-dependent success exists.

Consequence:

- later milestones may continue;
- NTCP2 remains experimental/non-advertised;
- Plan 072 is optional for differential confidence or unresolved edge cases;
- Plan 073 remains required before any final support/advertisement decision.

### `failed`

Use when real protocol/runtime behavior is reproducibly incorrect. Record the exact stage and return to the owning corrective surface.

### `blocked`

Use only for an unavailable/invalid execution lane, missing dependency/artifact, or resource condition that prevents attempts. Do not classify protocol mismatch as environment blocked.

## Deliverables

- six positive run records;
- required negative-control records;
- one development-validation summary;
- `plans/079-status.md`;
- updated Milestone 3 active status and protocol-support wording.

The status record must distinguish:

```text
development_interop = passed|failed|blocked
java_qualification = pending
release_qualification = pending
support = experimental
advertised = false
```

## Acceptance criteria

Plan 079 closes as passed only when:

- 3/3 fresh-state runs pass in each direction;
- exact authenticated data-phase correlation is proven in every run;
- all required negative controls reject correctly;
- the execution lane remains no-public-network and artifact-bound;
- cleanup is clean across all real runs;
- the development summary validates;
- no Java, Emissary, or release certificate result is fabricated;
- documentation states that development may continue but Milestone 3 release closure remains open.

## Validation commands

Use Plan 077/078 exact lane commands plus:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_development_validation.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-vectors.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
git diff --check
```

Run focused fuzz targets relevant to any malformed-frame control. Do not run an unrelated exhaustive fuzz/CI matrix.

## Non-goals

Plan 079 does not:

- run Java I2P;
- require Emissary;
- close Plan 073;
- enable production NTCP2 by default;
- advertise support;
- add recurring CI or release automation;
- benchmark throughput or anonymity properties.

## Stop rules

Stop and record a blocker when:

- lane qualification changes or becomes invalid;
- binary/image/guest digests drift;
- a pass depends on observer instrumentation;
- a required negative control cannot be implemented without modifying reference cryptographic behavior;
- cleanup cannot be proven;
- failures become intermittent and cannot be localized after one bounded reproduction cycle.

## Small-model execution guidance

- Finish all six positive runs before optional controls.
- Keep a run ledger with immutable record hashes.
- Never delete failed attempts.
- Restart a direction's 3/3 count after any implementation or lane change.
- Implement one negative control at a time.
- Do not broaden Plan 079 into Java, Emissary, load, or public-network testing.