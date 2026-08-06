# Plan 092: forward-handshake evidence integrity and ownership closure

## Status and authority

- Status: planned; single next executable plan.
- Parent roadmap: Plan 085.
- Immediate predecessors: Plan 090 and Plan 091.
- Corrective target: the still-open Plan 087 `i2pr -> i2pd` forward direction.
- Reuses: Plan 076 pinned i2pd direct driver, Plan 083 forward runner, Plan 086 host-loopback placement, Plan 090 RouterInfo correction, and the Plan 091 TCP-stage observations.
- Blocks: Plan 088, Plan 079, and every claim of forward or two-way development interoperability.
- Plan type: evidence-integrity repair, one behavior-neutral ownership reproduction, one demonstrated narrow correction, and instrumented/control forward closure.
- Active-sequence amendment:

  ```text
  Plan 085 -> Plan 086 -> Plan 087 -> Plan 090 -> Plan 091
           -> Plan 092 -> Plan 088 -> conditional Plan 079 or Plan 072
  ```

Plan 092 is the only active implementation authority after the partial Plan 091 landing. Plan 088 must not execute until this plan closes and the Plan 087 status records an exact passing instrumented record and an exact equivalent control record.

This plan supersedes two unsafe or inaccurate Plan 091 handoff statements:

1. Plan 091 is not closed merely because diagnostic vocabulary and TCP-stage events landed. Its ownership, correction, instrumented-pass, control-pass, and durable-evidence criteria remain incomplete.
2. Raw or hexadecimal SessionRequest, SessionCreated, SessionConfirmed, Noise transcript, frame, or payload capture is forbidden. Ownership must be determined from typed stage outcomes, bounded octet counts, and source-level control flow.

## Current baseline

The repository currently records the Plan 091 implementation commit:

```text
7952e7ad51d6ff1fcb334f89027e65e4c7f81e0e
```

The following corrections are real and remain load-bearing:

1. The Plan 090 i2pd direct driver emits a signed RouterInfo containing the exact configured NTCP2 endpoint.
2. The i2pd direct driver sets network ID 99 through `i2p::context.SetNetID` before `context.Init()`.
3. The driver explicitly starts and stops the i2pd logger.
4. The pinned i2pd observer records a real TCP accept.
5. The i2pr launcher emits `tcp_connected` immediately after the outbound `TcpStream::connect` succeeds.
6. The runner launches the i2pd listener and i2pr dialer concurrently under `HostLoopbackDevelopmentPlacement` and performs bounded cleanup.

The latest forward reproduction does not pass. The current evidence says:

```text
listener_ready                         = observed
i2pr tcp_connected                     = observed
i2pd TCP accept                        = visible in i2pd log/driver state
canonical-record i2pd tcp_accepted     = missing
SessionRequest body accepted           = not observed
i2pd terminal                          = SessionRequest read EOF
i2pr terminal                          = receiver_delivery_status_missing
ntcp2_authenticated                    = not observed
authenticated frame                    = not observed
i2np DeliveryStatus decode             = not observed
Plan 087                               = open
Plan 088                               = blocked
```

The evidence cannot yet establish whether:

- i2pr never completed the SessionRequest write;
- the i2pr socket or handshake owner closed the stream after an incomplete write;
- the i2pd direct-driver lifecycle caused the accepted session to read EOF before the normal responder path consumed the request;
- i2pd consumed and replied but the i2pr stage/status path misreported progress;
- or the harness lost or mis-correlated stage events.

The repository also has four evidence defects that must be fixed before another ownership claim:

1. The Plan 083 live loop recognizes `tcp_accepted`, but its final drain omits that event, allowing a fast terminal to lose the accepted-side stage.
2. Events are deduplicated primarily by event kind rather than by current-run invocation and sequence, which can collapse legitimate repeated events or admit stale ambiguity.
3. The i2pr terminal path replaces accumulated counters with `StatusCounters::default()`, destroying the last authenticated/frame/I2NP correlation state on failure.
4. The newly added first-flight status variants are not emitted around the actual SessionRequest write and SessionCreated read operations; they currently exist mainly as vocabulary and runner recognition.

## Objective

Close the remaining Plan 087 forward direction by performing the following in strict order:

1. repair evidence collection, event correlation, status-counter preservation, and status authority;
2. add behavior-neutral, metadata-only stage observations inside the actual i2pr and pinned i2pd handshake I/O boundaries;
3. commit the evidence-only implementation before any protocol correction;
4. execute exactly one clean-head instrumented reproduction;
5. select one exact ownership branch from that reproduction;
6. implement one narrowly owned correction;
7. produce a passing clean-head instrumented forward record;
8. produce a semantically equivalent fresh-state control forward record;
9. retain exact sanitized digests and reconcile Plans 087, 088, 091, and 092;
10. hand off Plan 088 as the next executable plan only after every closure condition passes.

The plan must answer one bounded question:

```text
Did i2pr fully write the first NTCP2 handshake flight, did the pinned i2pd
responder fully read and process it, and what is the first operation on either
side whose typed result diverges from the NTCP2 specification and pinned
reference control flow?
```

## Non-goals

Plan 092 must not:

- run the Plan 088 reverse direction before Plan 087 closes as passed;
- weaken RouterInfo signature, endpoint, Router Hash, network ID, static-key, clock-skew, replay, length, padding, or Noise transcript validation;
- accept listener readiness, TCP establishment, partial Noise progress, or one-sided authentication as a pass;
- retain raw handshake bytes, payloads, packet captures, transcript fragments, keys, nonces, IV material, decrypted options, or RouterInfo contents;
- add a packet proxy, packet recorder, pcap dependency, broad wire logger, or hex-dump facility;
- patch pinned i2pd handshake semantics without first proving that the bounded direct-driver lifecycle is not the owner;
- add retries, fallback transports, timeout inflation, or automatic repeated attempts that hide the first deterministic failure;
- route through SAM, I2CP, HTTP, SOCKS, reseed, NetDB bootstrap, DNS, SSU2, or the public I2P network;
- activate Plan 089 because the current host-loopback placement starts, binds, connects, and accepts TCP;
- activate Plan 072 directly; Plan 072 remains gated by a future Plan 088 `ambiguous-reference-divergence` decision;
- create a new container, VM, rootless, Multipass, CI, release, or qualification lane;
- enable NTCP2 in the normal daemon or change `specs/support.toml` to advertise support.

## Mandatory invariants

```text
reference revision                  = pinned i2pd 2.60.0 revision
topology                            = host-loopback-development
network ID                          = 99
endpoint                            = literal 127.0.0.1:<fresh-port>
development_only                    = true
release_qualified                   = false
isolation_qualified                 = false
peer RouterInfo                     = real signed i2pd output
peer endpoint                       = exact address/port match
source attribution                  = exact clean committed SHA
binary attribution                  = measured i2pr/instrumented/control digests
process ownership                   = HostLoopbackDevelopmentPlacement
observer behavior                   = passive, bounded, metadata-only
instrumented/control protocol path  = behaviorally equivalent
cleanup                             = bounded and clean
Plan 088                            = blocked until all Plan 092 closure gates pass
NTCP2                               = experimental and non-advertised
```

## Work package 1: normalize status authority before new execution

Rewrite the current status authority so the repository has one coherent description of the active blocker.

Required changes:

1. `plans/091-status.md` must record Plan 091 as partial/incomplete, not closed as a successful corrective pass.
2. Preserve the Plan 091 reproduction as diagnostic history with its exact record digest and exact source commit where available.
3. Remove the recommendation to capture or hex-dump handshake bytes.
4. Replace that recommendation with the metadata-only observation contract in this plan.
5. `plans/087-status.md` must begin with the current Plan 091/092 forward state rather than the superseded Plan 090 zero-address history.
6. Historical Plan 087 and Plan 090 attempts may remain only under clearly labeled historical sections.
7. `plans/088-status.md` must state:

   ```text
   decision = insufficient-evidence
   plan_086 = host-loopback-development-ready
   plan_087 = open_pending_plan_092_forward_closure
   plan_092 = planned_next_executable
   plan_088 = blocked_pending_plan_087_instrumented_and_control_pass
   ```

8. Remove contradictory statements that Plan 086 never closed, Plan 087 never executed, or no authentic TCP attempt exists.
9. Keep Plan 079 blocked and Plan 072 inactive.

Acceptance criteria:

- exactly one current authoritative section exists in each status file;
- no file simultaneously calls Plan 091 closed and incomplete;
- no file simultaneously says Plan 086 is ready and unclosed;
- no file says Plan 087 never ran;
- no zero digest substitutes for a known diagnostic record;
- no raw-transcript capture is proposed anywhere in the active handoff.

## Work package 2: define a privacy-safe handshake observation contract

Add a versioned observation type owned by the runtime/tooling boundary. It must carry operation metadata only.

Suggested schema:

```text
schema                         = i2pr-ntcp2-handshake-stage-v1
run_id                         = bounded current run ID
direction                      = i2pr-to-i2pd-ipv4
source_side                    = i2pr | i2pd
invocation_id                  = current process invocation correlation
event_sequence                 = monotonically increasing per invocation
stage                          = closed allowlist
expected_octets                = bounded integer or absent
completed_octets               = bounded integer or absent
io_result                      = not-applicable | completed | eof | closed | timeout | cancelled | failed
elapsed_millis                 = bounded integer
peer_router_hash_sha256        = expected public correlation digest or absent
event_sha256                   = canonical digest of the sanitized event
```

Allowed i2pr stages:

```text
initiator_state_initialized
session_request_encode_started
session_request_encode_completed
session_request_write_started
session_request_write_completed
session_request_write_failed
session_created_read_started
session_created_read_completed
session_created_read_eof
session_created_process_started
session_created_process_completed
session_created_process_failed
session_confirmed_write_started
session_confirmed_write_completed
noise_authenticated
```

Allowed i2pd stages:

```text
tcp_accepted
session_request_prefix_read_started
session_request_prefix_read_completed
session_request_prefix_read_eof
session_request_body_read_started
session_request_body_read_completed
session_request_body_read_eof
session_request_process_started
session_request_process_completed
session_request_process_failed
session_created_write_started
session_created_write_completed
session_created_write_failed
session_confirmed_read_started
session_confirmed_read_completed
session_confirmed_process_failed
noise_authenticated
```

Forbidden fields and content include:

```text
raw
payload
payload_hex
transcript
transcript_hex
ciphertext
plaintext
key
private_key
ephemeral_private
nonce
iv
padding_contents
router_info_bytes
packet_capture
socket_address_other_than_the_declared_loopback_contract
arbitrary OS or peer-controlled error text
```

The observation contract may record counts but never the counted bytes.

Acceptance criteria:

- schemas reject unknown stages and unknown fields;
- schemas reject oversized counts, negative values, malformed digests, mismatched direction, and mismatched run/invocation correlation;
- no observation contains raw bytes or unbounded strings;
- event canonicalization produces stable SHA-256 digests;
- instrumented and control builds share identical protocol code paths; the control build uses a no-op observer.

## Work package 3: instrument the actual i2pr handshake I/O boundary

The current launcher emits `tcp_connected` outside `drive_initiator_handshake`, then treats the entire handshake as opaque. Add a behavior-neutral progress observer inside the actual runtime handshake driver.

Implementation guidance:

1. Define a runtime-local observer trait or callback with a default no-op implementation, for example:

   ```text
   HandshakeProgressObserver::observe(HandshakeStageObservation)
   ```

2. Preserve the existing production call surface by making the existing method delegate to the observed implementation with a no-op observer.
3. Add an observed method used only by `tools/i2pr-interop`.
4. Emit observations immediately before and after the exact encode/read/write/process operations, not from inferred launcher state.
5. For writes, record:

   ```text
   expected_octets
   completed_octets
   io_result
   ```

6. For reads, record the exact target count and the completed count at EOF/failure without recording data.
7. Do not log `Debug` representations that may contain implementation or peer-controlled details.
8. Preserve cancellation and deadline ordering. Observation callbacks must not await, allocate unboundedly, lock across I/O, or influence the result.
9. The observed method must return the same `HandshakeRun` or the same typed failure as the no-op path.
10. Add an internal parity test proving identical state transitions and output for no-op versus recording observers under deterministic test vectors.

Terminal status must preserve the last valid counters. Replace the current error-path reset with an outcome that carries both the typed error and the accumulated counter snapshot, for example:

```text
WireExecutionOutcome {
    result: Result<(), LauncherError>,
    counters: StatusCounters,
}
```

or an equivalent structure.

Acceptance criteria:

- a successful `write_all` cannot be represented with `completed_octets < expected_octets`;
- an EOF read records the exact completed count before EOF;
- a handshake success requires the expected stage sequence through `noise_authenticated`;
- terminal failure retains `tcp_connected`, authentication, frame, I2NP, message-ID, and peer-hash counters reached before failure;
- impossible combinations fail closed as evidence inconsistency rather than being serialized as a normal protocol result;
- production daemon behavior and dependency boundaries remain unchanged.

## Work package 4: instrument the pinned i2pd responder boundary without changing semantics

Extend the existing compile-time observer patch only at the normal pinned i2pd responder read/process/write boundaries.

Required behavior:

1. Reset the process-local observer immediately before listener readiness is published and before the runner is allowed to start the dialer.
2. Assign one invocation ID and monotonic event sequence for the listener-mode process.
3. Observe the first prefix read, body read, SessionRequest processing, SessionCreated write, SessionConfirmed read/process, and established transition.
4. Record expected/completed octet counts and a bounded I/O result only.
5. Do not change buffer sizes, callback order, asynchronous ownership, exception behavior, timeout behavior, or session close behavior.
6. The observer seam must remain `noexcept` and nonblocking.
7. The control build must compile the exact pinned i2pd path without observer callbacks.
8. Add source-lock checks proving the patch modifies only allowlisted observer seams and does not alter the surrounding handshake decisions.

Acceptance criteria:

- `tcp_accepted` belongs to the current invocation and cannot be stale;
- the first prefix/body read result is retained even when the session closes immediately afterward;
- SessionCreated write completion is observable without retaining its contents;
- observer drop count is zero for a passing record;
- instrumented and control lifecycle/output remain semantically equivalent in existing direct-driver parity tests.

## Work package 5: repair Plan 083 event ingestion and classification

Replace the duplicated live-loop/final-drain logic with one shared ingestion function.

Required changes:

1. Use the same allowlist and validation path for:

   - live polling;
   - final drain after dialer exit;
   - final drain before listener reap.

2. Include `tcp_accepted` and every Plan 092 stage event in the final drain.
3. Deduplicate by a current-run key such as:

   ```text
   (source_side, invocation_id, event_sequence, event_sha256)
   ```

   Do not deduplicate only by event kind.

4. Reject events whose run ID, direction, scenario ID, invocation ID, or expected source side does not match the active process.
5. After the i2pr terminal appears, retain a bounded listener-drain grace period sufficient to collect the i2pd terminal/stage events before reap.
6. Reap the i2pr dialer first, then drain the i2pd events, then reap the listener.
7. Preserve the exact i2pr terminal reason rather than replacing every post-TCP failure with `reference-events-missing`.
8. Add bounded reason codes for evidence inconsistency and stage-specific failures. Generic `reference-events-missing` may be used only when an expected authenticated reference event is genuinely absent after all current-run streams are drained.
9. Enforce stage monotonicity independently for i2pr and i2pd.
10. A record must not advance to `noise_authenticated` unless both sides have current-run authenticated evidence.
11. A record must not advance to frame or I2NP stages from an event emitted before authentication.
12. A terminal `receiver_delivery_status_missing` with `authenticated = 0` is an evidence-inconsistent record, not a normal data-phase failure.

Acceptance criteria:

- a fast i2pr terminal cannot lose a late `tcp_accepted` event;
- repeated legitimate event kinds remain distinguishable by sequence;
- stale inspect-mode events cannot promote listener-mode stages;
- terminal counters survive failure;
- stage and prose cannot disagree about TCP, Noise authentication, or data-phase progress;
- cleanup remains bounded and process ownership remains exclusively in the placement.

## Work package 6: add the dedicated Plan 092 regression matrix

Add:

```text
tests/integration/ntcp2/harness/test_plan092.py
```

The matrix must cover at least:

1. Plan 091 cannot be marked passed or closed without ownership and both passing forward records.
2. Raw/hex transcript fields are rejected.
3. Counts are accepted only within bounded ranges.
4. i2pr write-start/write-complete observations originate inside the actual runtime handshake operation.
5. i2pd read-start/read-complete/EOF observations originate inside the patched responder operation.
6. No-op and recording observers produce identical handshake outcomes.
7. Terminal counters are preserved on every representative error branch.
8. `tcp_accepted` is captured by both live polling and final drain.
9. Shared ingestion treats live and final events identically.
10. Deduplication uses current-run invocation/sequence, not event kind alone.
11. Mismatched run ID, direction, source side, scenario ID, or invocation ID fails closed.
12. A stale inspect event cannot satisfy a listener event.
13. A post-connect failure cannot become pre-protocol.
14. A pre-connect failure cannot become protocol-level.
15. `receiver_delivery_status_missing` with zero authentication counters is rejected as evidence inconsistent.
16. Missing i2pd authentication after all drains retains the actual i2pr terminal reason plus the exact last common stage.
17. Instrumented/control driver source parity remains intact.
18. Plan 088 entry rejects missing, zero, malformed, mismatched-commit, mismatched-reference, non-passing, or skipped-live forward records.
19. Status files cannot simultaneously claim Plan 087 passed and blocked.
20. The active-sequence authority names Plan 092 as the only next executable plan.

Extend `scripts/check-ntcp2-interoperability.sh` to require the Plan 092 file, test matrix, privacy-safe field allowlist, and active-sequence token. Do not implement a brittle grep that rejects the words used in documentation; enforce the actual schema and source boundaries through tests and targeted static checks.

Live binary tests must report executed/passed/failed/skipped counts. A skipped instrumented or control binary test is not closure evidence.

## Work package 7: commit diagnostics before execution

All work from packages 1-6 must land in one or more commits before the ownership reproduction.

Before execution:

1. verify a clean worktree;
2. record the exact source commit;
3. rebuild `i2pr-interop` from that commit;
4. rebuild instrumented and control i2pd drivers from the pinned revision;
5. verify source-lock, observer-patch, build-manifest, and binary digests;
6. use fresh i2pr/i2pd state directories;
7. use fresh loopback ports;
8. use one exact nonzero DeliveryStatus message ID;
9. ensure no prior listener process or run root is reused.

Do not modify protocol behavior before the reproduction.

## Work package 8: execute exactly one ownership reproduction

Run one instrumented `i2pr -> i2pd` attempt from the committed diagnostics head.

Required sanitized record fields:

```text
source_commit
reference_revision
i2pr_binary_sha256
i2pd_instrumented_binary_sha256
i2pd_control_binary_sha256
driver_source_sha256
build_manifest_sha256
observer_patch_sha256
i2pr_router_info_sha256
i2pd_router_info_sha256
i2pr_router_hash_sha256
i2pd_router_hash_sha256
placement_record_sha256
delivery_status_message_id
i2pr_stage_observations
i2pd_stage_observations
last_common_stage
terminal_result
reason_code
terminal_counters
observer_drop_count
cleanup_result
record_sha256
```

The run root remains secret-bearing and temporary. Retain only the compact sanitized JSON under the existing `target/interop/evidence/` policy. Record its exact SHA-256 in `plans/092-status.md`; do not copy logs, RouterInfo, identities, configurations, addresses beyond the declared loopback contract, or raw artifacts into the repository.

Stop after this first reproduction. Do not automatically retry and do not change protocol code in the same uncommitted worktree.

Acceptance criteria:

- exact clean-head attribution;
- current-run correlation on both sides;
- no raw transcript data;
- exact last successful stage and octet counts on each side;
- preserved terminal counters;
- clean bounded teardown;
- one and only one ownership reproduction before the ownership record.

## Work package 9: select exactly one ownership branch

Write the ownership decision into `plans/092-status.md` before changing protocol behavior.

### Branch A: i2pr SessionRequest write or socket-lifetime defect

Select when any of the following is observed:

- SessionRequest write fails before `completed_octets == expected_octets`;
- the i2pr stream closes or is shut down before the full write completes;
- the runtime reports write completion but its deterministic real-TCP regression proves incomplete delivery;
- i2pd records prefix/body EOF consistent with the incomplete i2pr write.

Required ownership record:

```text
owner = i2pr
first_failing_stage
expected_octets
completed_octets
owning Rust module/function
specification section
expected behavior
observed behavior
minimal correction surface
```

### Branch B: bounded i2pd direct-driver lifecycle/read defect

Select only when:

- i2pr records a complete SessionRequest write;
- the no-op/control-compatible i2pr path proves the same complete write;
- the pinned i2pd accepted session records EOF or zero progress before the normal responder consumes the complete request;
- and source inspection ties the close to driver initialization, reset, context, transport, observer, or shutdown ownership rather than pinned handshake semantics.

The correction must remain in the direct-driver lifecycle/observer surface. Do not patch pinned i2pd Noise validation.

### Branch C: i2pr SessionCreated read or processing defect

Select only when:

- i2pd records complete SessionRequest processing;
- i2pd records complete SessionCreated write;
- i2pr records SessionCreated read EOF, short read, decode, deobfuscation, or Noise-message-2 processing failure.

The ownership record must identify the exact runtime/state-machine function and specification section.

### Branch D: evidence/observer defect

Select when events or counters remain impossible, missing, stale, observer-dependent, or contradictory after the Plan 092 repair.

Required action:

- fix only the evidence/observer defect;
- add the regression;
- commit;
- run one replacement ownership reproduction;
- do not patch protocol behavior;
- do not run Plan 088.

Only one replacement reproduction is permitted under this branch.

### Branch E: unresolved specification/reference ambiguity

Select only after the source/specification comparison cannot distinguish valid behavior from divergence despite internally consistent observations.

Required action:

- record one exact role/stage question, specification section, disputed operation, and expected discriminating outcomes;
- stop;
- do not activate Plan 072 directly;
- do not run Plan 088;
- create a separate decision plan if required.

Acceptance criteria:

- exactly one branch is selected;
- “socket closed,” “EOF,” or `reference-events-missing` alone is not an ownership conclusion;
- no protocol correction lands before the ownership record;
- the selected branch is supported by current-run observations and source/specification references.

## Work package 10: implement exactly one narrow correction

Implement only the correction demonstrated by work package 9.

General requirements:

- preserve all strict RouterInfo, Router Hash, endpoint, network-ID, static-key, replay, length, padding, and transcript checks;
- preserve bounded deadlines, cancellation, queue ownership, and cleanup;
- preserve dependency and runtime boundaries;
- preserve instrumented/control protocol parity;
- add a deterministic regression that fails before the correction and passes afterward;
- add relevant malformed, truncated, oversized, wrong-network, wrong-key, early-close, cancellation, and timeout negatives for the changed operation;
- do not add retries, alternate paths, or permissive fallbacks.

If Branch A or C owns the defect, the production Rust change must remain in the smallest correct `i2pr-runtime` or `i2pr-transport-ntcp2` surface. `tools/i2pr-interop` may adapt observations but must not implement protocol semantics.

If Branch B owns the defect, modify only the direct-driver lifecycle/observer seam and rebuild both instrumented and control binaries from the same pinned source.

If Branch D owns the defect, no protocol change is permitted.

Commit the correction before any closure execution.

## Work package 11: run the passing instrumented forward attempt

After the narrow correction is committed:

1. verify a clean worktree;
2. rebuild all executed binaries from the exact corrective commit;
3. use fresh state, ports, run ID, invocation IDs, and message ID;
4. run one instrumented forward attempt;
5. preserve the compact sanitized record and exact digest;
6. stop immediately if the attempt does not pass.

A passing instrumented result requires all of:

```text
i2pd RouterInfo exact endpoint verified
i2pr tcp_connected current-run event
i2pd tcp_accepted current-run event
SessionRequest complete write/read/process
SessionCreated complete write/read/process
SessionConfirmed complete write/read/process
noise_authenticated on both sides
exact peer Router Hash correlation
i2pr authenticated frame write completed
i2pd authenticated/decrypted frame
i2pd exact DeliveryStatus decoded
i2pd symmetric DeliveryStatus emitted
i2pr exact DeliveryStatus decoded
exact DeliveryStatus message ID correlation
terminal_result = passed
terminal counters preserved and nonzero where required
observer_drop_count = 0
cleanup_result = clean
no owned process remains
record_sha256 exact and nonzero
```

TCP-only, one-sided Noise success, handshake-only, frame-only, or decoded-without-correlation results do not pass.

If the attempt fails at a new stage, preserve the record, keep Plan 088 blocked, and stop. Do not run the control attempt.

## Work package 12: run the behavior-neutral control forward attempt

Run only after the instrumented forward attempt passes.

Requirements:

- same corrective source commit;
- same pinned i2pd revision;
- control driver built without observer callbacks;
- fresh state and fresh ports;
- same network ID, timeout classes, message profile, strict scenario contract, and pass predicate;
- externally visible authentication and exact bidirectional DeliveryStatus correlation;
- clean teardown;
- exact control record digest.

The control record need not contain internal i2pd stage events. It must satisfy the established external control oracle and prove that instrumentation did not create the success.

Instrumented/control disagreement is a blocker, not a pass.

## Work package 13: durable evidence and roadmap continuation

After both forward attempts pass:

1. write `plans/092-status.md` with:

   ```text
   status = passed
   ownership = <exact branch and owner>
   correction_commit = <exact SHA>
   forward_instrumented_record_sha256 = <64 hex>
   forward_control_record_sha256 = <64 hex>
   ```

2. Record exact digests for all executed binaries, RouterInfos, placement record, build manifest, observer patch, and sanitized records.
3. Record exact Router Hash and DeliveryStatus message-ID correlation.
4. Validate sanitized evidence under `target/interop/evidence/` using the existing evidence validator.
5. Delete secret-bearing run roots after validation and digest extraction.
6. Rewrite `plans/087-status.md` to one authoritative closure section with:

   ```text
   status = passed
   ```

7. Rewrite `plans/088-status.md` so it states:

   ```text
   plan_087 = passed
   plan_092 = passed
   plan_088 = next_executable
   decision = not-yet-run
   ```

8. Remove stale Plan 090/091 blocker prose from the current sections; retain historical material only under labeled history.
9. Update README/AGENTS active-sequence guidance to name Plan 088 as the single next executable plan.
10. Keep Plan 079 blocked and Plan 072 inactive until Plan 088 records its actual decision.
11. Keep NTCP2 experimental, disabled, and non-advertised.

Plan 088 becomes executable only after the status/evidence reconciliation commit lands.

## Required validation commands

At minimum:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan092.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan091.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan090.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
git diff --check
```

If `test_plan091.py` does not yet exist, Plan 092 must either add the missing Plan 091 contract tests or explicitly fold every still-required Plan 091 assertion into `test_plan092.py`; the validation command and status record must accurately reflect which file owns the checks.

Live binary checks must report executed, passed, failed, and skipped counts. Skipped live checks do not satisfy closure.

## Explicit closure criteria

Plan 092 closes only when every item is true:

- [ ] Plan 091 is recorded as partial/incomplete rather than falsely closed.
- [ ] Plans 087 and 088 have one current noncontradictory authority section.
- [ ] Raw/hex handshake capture is prohibited by schema and tests.
- [ ] Actual i2pr SessionRequest write and SessionCreated read operations emit metadata-only stage observations.
- [ ] Actual pinned i2pd SessionRequest read/process and SessionCreated write operations emit metadata-only stage observations.
- [ ] Instrumented and no-op/control protocol paths are behaviorally equivalent.
- [ ] Terminal counters are preserved on failure.
- [ ] Live and final event drains use one ingestion function.
- [ ] `tcp_accepted` and all current-run terminal events survive fast-process races.
- [ ] Event correlation rejects stale, duplicate, mismatched-run, mismatched-direction, and mismatched-invocation events.
- [ ] Dedicated Plan 092 regressions pass with no required live test skipped.
- [ ] Diagnostics were committed before the ownership reproduction.
- [ ] Exactly one clean-head ownership reproduction was executed before protocol correction, except the single permitted Branch D replacement.
- [ ] Exactly one ownership branch was selected with source/specification support.
- [ ] Exactly one narrow correction was implemented.
- [ ] The selected regression proves the correction.
- [ ] A fresh clean-head instrumented forward attempt passed the exact bidirectional DeliveryStatus predicate.
- [ ] A fresh control forward attempt produced equivalent external success.
- [ ] Both forward record digests are exact, nonzero, and bound to the same corrective commit and pinned revision.
- [ ] Router Hash, endpoint, network ID, and DeliveryStatus correlations are exact.
- [ ] Both attempts have clean teardown and zero observer drops.
- [ ] Secret-bearing run roots were deleted after evidence sanitation.
- [ ] `plans/087-status.md` records `status = passed`.
- [ ] `plans/092-status.md` records `status = passed` and the exact ownership/correction.
- [ ] `plans/088-status.md` records Plan 088 as `next_executable` without claiming a reverse result.
- [ ] Plan 079 remains blocked and Plan 072 remains inactive.
- [ ] NTCP2 remains experimental, disabled, and non-advertised.

If any checkbox is false, Plan 088 remains blocked.

## Smaller-model execution guidance

Execute this plan in the exact order below. Do not combine implementation and live evidence in one dirty-tree pass.

1. Rewrite status authority and remove the raw-dump recommendation.
2. Define the sanitized stage-observation schema and tests.
3. Add actual i2pr runtime observations and counter preservation.
4. Add actual i2pd responder observations.
5. Refactor Plan 083 to one ingestion/final-drain path.
6. Add `test_plan092.py` and static enforcement.
7. Run focused and full validation.
8. Commit all diagnostic/evidence changes.
9. Rebuild from the exact clean commit.
10. Run exactly one instrumented ownership reproduction.
11. Write and commit the ownership record before protocol changes.
12. Implement one narrow correction.
13. Add the correction regression and negatives.
14. Run all required validation.
15. Commit the correction.
16. Rebuild from the exact corrective commit.
17. Run one fresh instrumented forward attempt.
18. Stop if it does not pass.
19. Run one fresh control forward attempt only after instrumented success.
20. Reconcile Plans 087, 088, 091, and 092 and record exact digests.
21. Commit closure evidence and active-sequence updates.
22. Only then hand off Plan 088.

Do not infer success from a zero process exit, listener readiness, TCP establishment, a complete local write without peer processing, one-sided authentication, a green unit suite with skipped live binaries, or prose that is not supported by the canonical record.

## Handoff state

Until Plan 092 closes:

```text
plan_086 = host-loopback-development-ready
plan_090 = routerinfo-correction-landed
plan_091 = partial-diagnostic-surface-landed-forward-not-passed
plan_087 = open_forward_handshake_ownership_unresolved
plan_092 = planned_next_executable
plan_088 = blocked_pending_plan_087_instrumented_and_control_pass
plan_079 = blocked_pending_plan_088_two_way_pass
plan_072 = inactive_pending_plan_088_ambiguity
ntcp2 = experimental_non_advertised
```
