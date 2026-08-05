# Plan 091: forward NTCP2 Noise-handshake corrective pass

## Status and authority

- Status: planned.
- Parent roadmap: Plan 085.
- Predecessor corrective plan: Plan 090.
- Corrective target: the open Plan 087 `i2pr -> i2pd` forward direction.
- Reuses: Plan 076 pinned i2pd direct driver, Plan 083 forward runner, Plan 086 host-loopback placement, Plan 090 RouterInfo correction, and the Plan 087 result schema.
- Blocks: Plan 088, Plan 079, and every claim of two-way development interoperability.
- Does not reopen: the Plan 086 placement qualification or the Plan 090 zero-address RouterInfo correction unless new evidence directly disproves either result.
- Plan type: bounded stage-authority correction, one evidence-only reproduction, one ownership decision, one narrowly owned protocol or reference-driver correction, and clean-head instrumented/control closure.

## Current baseline

The repository has advanced beyond the original pre-TCP RouterInfo blocker:

1. Plan 090 corrected the i2pd direct driver so its signed RouterInfo contains the exact configured NTCP2 endpoint.
2. The Plan 083 forward runner prepares both peers, verifies and copies the exact RouterInfo, validates the strict scenario, starts the i2pd listener, starts the i2pr dialer while the listener remains alive, drains both event streams, and performs bounded cleanup.
3. A clean committed-head Plan 087 attempt entered `drive_initiator_handshake` and terminated with:

   ```text
   result = authentication_failed
   reason_code = handshake_failed
   error = Io(ExactIoError { kind: Closed })
   ```

4. No `ntcp2_authenticated`, authenticated-frame, or `i2np_message_decoded` event was retained.
5. The status prose says TCP was established, but the canonical forward record contains no authentic `tcp_connected` event and therefore serializes the result as `pre_protocol_rejected`.
6. The present record is useful diagnostic evidence, but it is not a Plan 087 pass and cannot open Plan 088.
7. No control-build forward pass exists.
8. `plans/087-status.md` and `plans/088-status.md` still contain stale historical sections and contradictory gate statements.

This plan closes the remaining forward-handshake work without broadening the Milestone 3 scope.

## Objective

Produce a trustworthy, reproducible, clean-head Plan 087 forward result by:

1. establishing exact stage authority for TCP connection and the first NTCP2 handshake flight;
2. reproducing the current close once with evidence-only instrumentation and unchanged protocol behavior;
3. determining whether the first failing operation is owned by i2pr, the bounded i2pd direct driver, or the pinned i2pd implementation/reference contract;
4. applying one narrow correction only after ownership is demonstrated;
5. obtaining a passing instrumented forward result;
6. obtaining a semantically equivalent passing control result;
7. retaining exact record digests and reconciling the Plan 087/088 gate records.

The plan answers:

```text
At which exact operation does the current i2pr initiator -> i2pd responder
NTCP2 handshake terminate, which side owns that operation, and does a narrow
correction permit the exact DeliveryStatus exchange required by Plan 087?
```

## Non-goals

This plan must not:

- execute the Plan 088 reverse direction before Plan 087 closes as passed;
- weaken RouterInfo endpoint, Router Hash, network ID, signature, or NTCP2 static-key validation;
- accept a listener-ready, TCP-only, or Noise-partial result as a pass;
- add retry loops that hide the first deterministic failure;
- change handshake timeouts merely to convert a close into a timeout;
- log raw SessionRequest, SessionCreated, or SessionConfirmed bytes;
- log Noise chaining keys, cipher keys, ephemeral secrets, static private keys, IV material, nonces, or decrypted payloads;
- patch pinned i2pd transport behavior speculatively;
- route through SAM, I2CP, HTTP, SOCKS, a full public router, reseed, NetDB bootstrap, DNS, SSU2, or public network access;
- activate Plan 089, because the host-loopback placement starts and binds successfully;
- activate Plan 072 merely because the handshake failure is initially ambiguous;
- add a new interop framework, container/VM lane, CI workflow, broad qualification bundle, or release apparatus;
- modify production daemon enablement or advertise NTCP2 support.

## Mandatory invariants

```text
reference revision                    = pinned i2pd 2.60.0 revision
topology                              = host-loopback-development
network ID                            = 99
addresses                             = literal IPv4 127.0.0.1:<fresh-port>
development_only                      = true
release_qualified                     = false
isolation_qualified                   = false
peer RouterInfo                       = real signed i2pd output
peer endpoint                         = exact address and port match
source attribution                    = clean committed SHA
binary attribution                    = measured i2pr/instrumented/control digests
process ownership                     = HostLoopbackDevelopmentPlacement
observer instrumentation              = passive, bounded, sanitized
instrumented/control transport path   = behaviorally identical
cleanup                               = bounded and clean
Plan 088                              = blocked until all closure criteria pass
```

## Work package 1: freeze and normalize the baseline

Before modifying protocol behavior:

1. Preserve the existing Plan 090 forward record as diagnostic history.
2. Record its exact SHA-256 in the updated Plan 087 status instead of `non-zero` prose.
3. Record the exact source commit, i2pr binary digest, instrumented i2pd binary digest, RouterInfo digests, Router Hashes, message ID, placement digest, terminal status, and cleanup result.
4. Mark the existing `/tmp/...` path as a non-durable local location. Do not treat the path itself as retained repository evidence.
5. Remove or clearly label every historical section that describes the superseded zero-address attempt as the current result.
6. Keep Plan 087 open and Plan 088 blocked.

Acceptance:

- one current baseline section exists;
- historical attempts are explicitly labeled historical;
- no status file claims both that a forward attempt exists and that no authentic attempt exists;
- no status file says Plan 086 is unclosed;
- no zero digest is used for a known retained forward record.

## Work package 2: establish authoritative TCP-stage events

The current status says the handshake reached TCP, while the canonical record has no `tcp_connected` event. Correct this ambiguity before diagnosing Noise.

### i2pr authority

Add or verify one structured event emitted immediately after the intended `TcpStream::connect` succeeds and before the first handshake byte is written:

```text
event_name = tcp_connected
source_side = i2pr
run_id
direction
peer endpoint correlation
current-run event digest
```

The event must not be inferred from:

- entry into `drive_initiator_handshake`;
- a write attempt;
- a later `Closed` error;
- listener readiness;
- process survival.

### i2pd authority

Add or verify one passive event at the real pinned i2pd accept/session-construction boundary:

```text
event_kind = tcp_accepted
source_side = i2pd
run_id
direction
local listener correlation
current-run event digest
```

The i2pd event does not replace the i2pr `tcp_connected` event. The forward stage advances to `tcp_connected` only under the exact schema rule selected for Plan 083, and that rule must be documented and tested.

### Required classification rule

```text
connect call never succeeded
  -> pre_protocol_rejected

connect call succeeded and tcp_connected was emitted
  -> all later failures are protocol-level outcomes

listener_ready without tcp_connected
  -> highest stage remains listener_ready
```

Acceptance:

- the same run cannot be described as TCP-established in prose while lacking the canonical event;
- a post-connect socket close cannot serialize as `pre_protocol_rejected`;
- a pre-connect failure cannot serialize as `protocol_rejected`;
- focused tests cover both boundaries.

## Work package 3: add bounded first-flight diagnostics

Add passive, sanitized stage markers around the first NTCP2 handshake flight. These markers must identify operations, not secret data.

### i2pr initiator markers

At minimum:

```text
initiator_noise_state_initialized
session_request_encode_started
session_request_encode_completed
session_request_write_started
session_request_write_completed
session_created_read_started
session_created_read_eof
session_created_read_completed
session_created_decode_failed
noise_message_2_processing_failed
```

Each marker may carry only bounded metadata such as:

- declared or observed byte count;
- expected fixed/minimum/maximum length class;
- elapsed bounded duration;
- typed local error category;
- run/direction correlation;
- expected peer Router Hash correlation.

### i2pd responder markers

Place passive markers at the pinned i2pd session path or the existing observer seam sufficient to distinguish:

```text
tcp_accepted
session_request_read_started
session_request_read_completed
session_request_length_rejected
session_request_deobfuscation_failed
noise_message_1_processing_failed
session_request_options_rejected
session_created_encode_started
session_created_write_completed
responder_closed_before_session_created
```

Do not add a broad packet dump. Do not record raw bytes, keys, nonces, padding contents, or decrypted options.

### Instrumented/control constraint

All new diagnostics must be compile-time passive observer surfaces. The control binary must execute the same transport lifecycle without observer callbacks or behavior changes.

Acceptance:

- one run identifies the last successful operation on each side;
- terminal status names a bounded stage-specific reason rather than generic `handshake_failed` alone;
- no secret-bearing data enters repository records or status documents;
- static tests reject raw transcript/logging additions.

## Work package 4: execute exactly one evidence-only reproduction

After work packages 1-3 land in one clean commit:

1. rebuild `i2pr-interop`;
2. rebuild the instrumented and control i2pd drivers from the pinned source;
3. measure all binaries and manifests;
4. use a fresh run root, fresh identities, fresh loopback ports, and one exact nonzero DeliveryStatus ID;
5. run one instrumented `i2pr -> i2pd` attempt;
6. do not change protocol behavior before this attempt;
7. preserve the compact record and sanitized stage events;
8. stop after the first attempt.

Required reproduction record:

```text
source_commit
reference_revision
i2pr_binary_sha256
i2pd_instrumented_binary_sha256
driver_source_sha256
build_manifest_sha256
observer_patch_sha256
i2pr_router_info_sha256
i2pd_router_info_sha256
i2pr_router_hash_sha256
i2pd_router_hash_sha256
placement_record_sha256
delivery_status_message_id
observed_stage_events
highest_stage_reached
terminal_result
reason_code
cleanup_result
record_sha256
```

Acceptance:

- clean committed-head attribution is exact;
- TCP stage is unambiguous;
- the last successful i2pr and i2pd first-flight operations are known;
- no automatic second attempt occurs.

## Work package 5: determine ownership

Use the evidence-only reproduction to select exactly one branch.

### Branch A: i2pr first-flight construction defect

Select only when i2pd accepts TCP and rejects the SessionRequest before producing SessionCreated, and the evidence ties the rejection to i2pr-generated input.

Inspect against the existing implementation, pinned source-verification notes, and applicable NTCP2 specification sections:

- responder static NTCP2 key selected from the exact peer RouterInfo;
- Noise pattern/protocol-name/prologue initialization;
- SessionRequest fixed fields and declared lengths;
- ephemeral-key handling;
- X-field obfuscation inputs and transformation;
- network ID and timestamp/options encoding;
- padding length and total message bounds;
- complete-write handling and cancellation behavior.

Required ownership record:

```text
owner = i2pr
first_failing_operation
owning Rust module/function
pinned i2pd rejecting function
specification section
expected behavior
observed behavior
minimal correction surface
```

### Branch B: bounded i2pd driver lifecycle defect

Select only when i2pr emits a structurally valid first flight but the direct driver config/lifecycle/observer path closes the session before the normal pinned i2pd responder logic can process it.

Examples include:

- driver starts the listener but tears down required context or transport ownership;
- imported peer state or network ID is not available to the real responder session;
- observer code changes lifetime, callback order, or exception behavior;
- the bounded driver starts a different path than the control binary.

The correction must remain in the driver/lifecycle surface and must not patch pinned handshake semantics.

### Branch C: i2pr receive/second-flight defect

Select only when i2pd successfully processes SessionRequest and writes SessionCreated, but i2pr fails while reading, decoding, deobfuscating, or processing Noise message 2.

Inspect:

- exact read target and partial-read behavior;
- SessionCreated length/options bounds;
- responder static/ephemeral key expectations;
- Noise message 2 state transition;
- clock-skew and network-ID validation;
- EOF handling after a complete SessionCreated write.

### Branch D: unresolved reference divergence

Use only when both sides produce evidence consistent with their own implementations but the expected behavior cannot be selected from the specification and pinned source after one bounded analysis cycle.

Do not execute Plan 088 and do not activate Plan 072 directly. Record one exact diagnostic question and stop for a separate decision plan.

Acceptance:

- exactly one ownership branch is selected;
- no protocol change lands without an ownership record;
- “socket closed” alone is not an ownership conclusion.

## Work package 6: implement one narrow correction

Implement only the demonstrated correction from work package 5.

Requirements:

- preserve all strict RouterInfo, Router Hash, endpoint, signature, and network-ID checks;
- preserve bounded I/O and cancellation behavior;
- preserve production/runtime dependency boundaries;
- preserve instrumented/control transport parity;
- add a deterministic regression reproducing the old failure and proving the corrected stage transition;
- add negative tests for malformed, truncated, oversized, wrong-network, wrong-key, and early-close inputs relevant to the changed operation;
- do not add retries or fallback paths.

If the correction touches production Rust NTCP2 code, require:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

If the correction touches the i2pd driver, require rebuilt instrumented and control binaries and the existing source-lock/build-manifest checks.

## Work package 7: run the passing instrumented forward attempt

After the narrow correction is committed:

1. verify a clean worktree;
2. rebuild all executed binaries from that exact commit;
3. use fresh i2pr/i2pd state and fresh ports;
4. run one instrumented forward attempt;
5. do not reuse the evidence-only reproduction state;
6. preserve the compact result and exact digest.

A passing instrumented result requires all of:

```text
i2pd RouterInfo contains exact configured NTCP2 endpoint
i2pr tcp_connected event
i2pd tcp_accepted event
SessionRequest accepted
SessionCreated accepted
SessionConfirmed accepted
ntcp2_authenticated observed on both sides
i2pr authenticated frame write completed
i2pd authenticated/decrypted frame
i2pd decoded the exact DeliveryStatus
DeliveryStatus message ID matched exactly
peer Router Hash matched exactly
terminal result = passed
cleanup_result = clean
no listener or child process remains
record_sha256 is exact and non-zero
```

A TCP-only, handshake-only, or authenticated-without-data result does not pass.

If the instrumented attempt fails at a new stage:

- preserve the record;
- do not run the control attempt;
- do not execute Plan 088;
- create a new narrowly scoped follow-up only if ownership is clear.

## Work package 8: run the behavior-neutral control attempt

Run only after the instrumented forward attempt passes.

Requirements:

- fresh state and fresh ports;
- same corrective source commit;
- same pinned i2pd revision;
- control driver built without observer callbacks;
- same timeout, endpoint, network ID, message profile, and strict scenario contract;
- externally visible i2pr success and exact DeliveryStatus correlation;
- clean teardown;
- exact control record digest.

The control result need not contain instrumented internal i2pd events. It must prove behavior equivalent through the control-build oracle defined by Plans 076/087/090.

Instrumented/control disagreement is a blocker, not a pass.

## Work package 9: durable evidence and status reconciliation

After both forward attempts pass:

1. update `plans/087-status.md` to one current authoritative closure section;
2. set exactly:

   ```text
   status = passed
   ```

3. record actual 64-character digests for:

   ```text
   forward_instrumented_record_sha256
   forward_control_record_sha256
   source_commit
   i2pr_binary_sha256
   i2pd_instrumented_binary_sha256
   i2pd_control_binary_sha256
   placement_record_sha256
   i2pr_router_info_sha256
   i2pd_router_info_sha256
   ```

4. record exact Router Hash and DeliveryStatus ID correlation;
5. retain only sanitized compact evidence at the repository-approved evidence location;
6. remove secret-bearing run roots after digest extraction and validation;
7. rewrite `plans/088-status.md` so its entry gate states that Plan 087 passed and Plan 088 is the next executable plan;
8. remove stale statements that Plan 086 never closed, Plan 087 never ran, no authentic forward record exists, or all forward digests are zero;
9. keep Plan 079 blocked and Plan 072 inactive until Plan 088 reaches its decision.

Plan 088 may become executable only after these status updates are committed.

## Required focused tests

Add or extend focused tests proving:

1. `tcp_connected` is emitted only after successful i2pr connect;
2. `tcp_accepted` is emitted only by the real i2pd accept/session path;
3. listener readiness cannot promote TCP stage;
4. entering handshake code cannot promote TCP stage by itself;
5. post-connect EOF is protocol-level, not pre-protocol;
6. pre-connect failure remains pre-protocol;
7. first-flight stage markers carry no raw bytes or secret fields;
8. i2pr and i2pd event run IDs/directions must match;
9. duplicate or stale events cannot promote stages;
10. the old generic `handshake_failed` path maps to a bounded stage-specific reason;
11. the selected ownership regression fails before the correction and passes after it;
12. instrumented/control source paths remain behaviorally equivalent;
13. a forward pass requires the exact DeliveryStatus and Router Hash correlations;
14. Plan 088 gate rejects missing, zero, malformed, mismatched-commit, or non-passing forward digests;
15. status files cannot simultaneously claim Plan 087 passed and blocked.

## Validation commands

At minimum:

```text
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

Live binary tests must not be counted as passing when skipped. The Plan 091 closure record must explicitly list executed, passed, failed, and skipped live checks.

## Explicit closure criteria

Plan 091 closes only when all of the following are true:

- [ ] The historical Plan 090 record has an exact digest and is labeled diagnostic.
- [ ] TCP connection stage is authoritative and consistent between events, record, and prose.
- [ ] Passive first-flight diagnostics identify the last successful operation on both sides.
- [ ] Exactly one evidence-only reproduction was executed before protocol changes.
- [ ] One ownership branch was selected with source/specification support.
- [ ] Exactly one narrow correction was implemented.
- [ ] The selected regression test proves the correction.
- [ ] A clean-head instrumented forward attempt passed through exact DeliveryStatus decode.
- [ ] A fresh-state control forward attempt produced equivalent external success.
- [ ] Both forward record digests are exact, non-zero, and bound to the same corrective commit.
- [ ] Router Hash, endpoint, network ID, and DeliveryStatus ID correlations are exact.
- [ ] Instrumented/control cleanup is clean and no owned process remains.
- [ ] `plans/087-status.md` records `status = passed` without stale contradictory sections.
- [ ] `plans/088-status.md` records Plan 088 as next executable without claiming reverse success.
- [ ] Plan 079 remains blocked and Plan 072 remains inactive.
- [ ] NTCP2 remains experimental, disabled, and non-advertised.

If any checkbox is false, Plan 088 remains blocked.

## Smaller-model execution guidance

Execute in this order and do not combine stages:

1. Normalize status/evidence only.
2. Add TCP-stage authority and tests.
3. Add passive first-flight markers and sanitization tests.
4. Commit.
5. Run one evidence-only reproduction.
6. Write the ownership record before changing protocol code.
7. Implement one correction.
8. Run focused tests and full required gates.
9. Commit the correction.
10. Run one fresh instrumented forward attempt.
11. Stop if it does not pass.
12. Run one fresh control attempt only after instrumented success.
13. Reconcile Plan 087/088 status files.
14. Commit closure evidence.
15. Only then hand off Plan 088.

Do not infer success from process exit zero, listener readiness, TCP establishment, one-sided authentication, or a green unit suite with skipped live binaries.

## Handoff state

Until this plan closes:

```text
plan_086 = host-loopback-development-ready
plan_090 = routerinfo-correction-landed-forward-not-passed
plan_087 = open_forward_handshake_failure
plan_091 = planned_next_executable
plan_088 = blocked_pending_plan_087_instrumented_and_control_pass
plan_079 = blocked_pending_plan_088_two_way_pass
plan_072 = inactive_pending_plan_088_ambiguity
ntcp2 = experimental_non_advertised
```
