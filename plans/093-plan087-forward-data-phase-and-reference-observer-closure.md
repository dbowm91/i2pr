# Plan 093: Plan 087 forward data-phase and reference-observer closure

## Status and authority

- Status: planned; single next executable plan.
- Parent roadmap: Plan 085.
- Corrective target: Plan 087 `i2pr -> i2pd` forward closure.
- Immediate predecessors: Plans 090, 091, and 092.
- Execution lane: existing Plan 086 `host-loopback-development` only.
- Reference implementation: source-locked i2pd 2.60.0 revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Blocks: Plan 088, Plan 079, and any claim of two-way NTCP2 development interoperability.
- Plan type: source-classification correction, bounded data-phase correction, reference-observer lifecycle correction, measured forward instrumented/control closure, and status-authority reconciliation.

Active sequence:

```text
Plan 085 -> Plan 086 -> Plan 087 -> Plan 090 -> Plan 091
         -> Plan 092 -> Plan 093 -> Plan 088
         -> conditional Plan 079 or Plan 072
```

Plan 093 supersedes Plan 092 as the single next executable implementation authority. Plan 088 must not run until Plan 093 records both a passing instrumented forward record and a passing control forward record from the same corrective source commit and pinned i2pd revision.

## Executive finding

The current Plan 092 ownership record misclassifies the latest i2pd log message.

The retained log says:

```text
NTCP2: Receive length read error: End of file
```

This is **not** the pinned i2pd SessionRequest reader. In pinned i2pd 2.60.0, the message is emitted by:

```text
NTCP2Session::ReceiveLength()
NTCP2Session::HandleReceivedLength()
```

These functions read the two-byte obfuscated frame length in the authenticated NTCP2 **data phase**. The inbound handshake reader instead uses `HandleSessionRequestReceived` and emits a distinct `SessionRequest read error` diagnostic.

The same retained run reports i2pr counters:

```text
authenticated = 1
frames_sent   = 1
frames_received = 1
i2np_sent     = 1
i2np_received = 0
terminal      = receiver_delivery_status_missing
```

Those counters are consistent with a completed handshake followed by a data-phase sequencing failure. They are inconsistent with the Plan 092 Branch A theory that i2pr authenticated without i2pd reading SessionRequest.

Pinned i2pd also performs the following after an inbound session becomes established:

```text
NTCP2Session::Established()
  -> transports.PeerConnected(session)

Transports::PeerConnected(incoming session)
  -> session->SendLocalRouterInfo()
```

Therefore, on an inbound i2pr connection, i2pd may send its local RouterInfo as the first data-phase message before the direct driver submits the correlated DeliveryStatus reply. The NTCP2 specification explicitly permits RouterInfo in the data phase and identifies it as a valid message Bob may use to begin the data phase.

The current i2pr interop oracle reads exactly one authenticated frame and immediately returns `receiver_delivery_status_missing` when that first frame does not contain the target DeliveryStatus. This is too strict for interoperating with the pinned reference. It closes the socket after receiving i2pd's legitimate initial RouterInfo/DatabaseStore traffic, causing i2pd's next data-phase length read to observe EOF.

A second reference-driver defect compounds this:

1. `run_listen()` emits `listener_ready` and only then calls `ResetObserverSink()`.
2. The Plan 083 runner starts i2pr after observing `listener_ready`.
3. A fast TCP accept/authentication observation may therefore be recorded and then erased by the post-ready reset.
4. The observer's sent slot is process-global and cumulative. i2pd's automatic local-RouterInfo send may increment the sent counter before the driver submits its DeliveryStatus reply.
5. `WaitForSentI2NP()` currently accepts any nonzero sent count, so it may return the stale automatic-RouterInfo observation and permit shutdown before the DeliveryStatus reply is actually written.

The remaining forward blocker is therefore owned by the **data-phase oracle and reference-observer lifecycle**, not by the i2pr Noise state machine unless a new correctly classified run proves otherwise.

## Research basis

The implementation must be checked against these exact sources:

### Official NTCP2 specification

- `https://i2p.net/en/docs/specs/ntcp2/`
- Establishment order: SessionRequest, SessionCreated, SessionConfirmed, then data phase.
- Data-phase RouterInfo block: valid for Alice or Bob; Bob may use it to start the data phase.
- Data-phase I2NP block: a frame may carry I2NP messages, but the peer is not required to make the test's target DeliveryStatus the first received frame or first block.
- Implementations must ignore unknown data-phase block types for forward compatibility except where the specification defines a terminal or malformed condition.

### Pinned i2pd source

Revision:

```text
f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
```

Required call graph:

```text
libi2pd/NTCP2.cpp
  NTCP2Session::Established
  NTCP2Session::ReceiveLength
  NTCP2Session::HandleReceivedLength
  NTCP2Session::HandleReceived
  NTCP2Session::ProcessNextFrame
  NTCP2Session::HandleI2NPMsgsSent

libi2pd/Transports.cpp
  Transports::PeerConnected
  Transports::SendMessage
  Transports::SendMessages
  Transports::PostMessages
```

Source-specific conclusions:

- `Receive length read error` is a data-phase frame-length read failure.
- `Established()` precedes `PeerConnected()`.
- `PeerConnected()` sends local RouterInfo on incoming sessions.
- `HandleI2NPMsgsSent()` is the correct successful asynchronous frame-write observer seam.
- `SendMessage()` returns asynchronous work; calling it is not proof that the target frame reached the socket.

### Current i2pr surfaces

```text
tools/i2pr-interop/src/main.rs
  execute_initiator
  exchange_directional
  send_i2np_block
  receive_delivery_status

crates/i2pr-runtime/src/ntcp2_runtime.rs
  DialAttempt::drive_initiator_handshake
  AuthenticatedLink::recv

crates/i2pr-runtime/src/ntcp2_driver.rs
  drive_initiator_handshake
  drive_initiator_handshake_observed
```

The production-compatible handshake path must remain unchanged during the first Plan 093 correction. The Plan 092 observed-handshake API may remain as a bounded diagnostic seam, but it is not a prerequisite for Plan 093 forward closure unless the corrected data-phase attempt fails before authentication.

## Environment constraints

Plan 093 must complete on the current constrained environment without introducing another execution lane.

Mandatory environment contract:

```text
topology_kind                    = host-loopback-development
bind endpoints                   = literal 127.0.0.1:<fresh-port>
network_id                       = 99
privilege escalation             = forbidden
sudo                             = forbidden
rootless namespace requirement   = not used
Multipass requirement            = not used
Docker/QEMU requirement          = not used
public I2P network               = forbidden
reseed/bootstrap                 = forbidden
DNS                              = forbidden
SAM/I2CP/HTTP/SOCKS              = forbidden
SSU2                             = forbidden
reference revision               = pinned i2pd 2.60.0 revision
normal daemon NTCP2 activation   = forbidden
support advertisement           = forbidden
```

Plan 089 manual-isolated fallback is not applicable. The existing lane has already demonstrated listener bind, TCP accept, NTCP2 authentication evidence through i2pr counters and the pinned data-phase log location, and authenticated frame exchange. A post-auth data-phase defect is not a placement failure.

## Objective

Produce authoritative Plan 087 closure evidence under the existing host-loopback lane by completing these steps in order:

1. correct the Plan 092 source classification and withdraw the unsupported Branch A ownership claim;
2. lock the pinned source interpretation with regression tests;
3. correct i2pd observer reset and stale-observation semantics;
4. make i2pr's DeliveryStatus receive oracle bounded and tolerant of valid pre-target data-phase traffic;
5. make the i2pd driver wait for the exact target receive and exact target asynchronous send completion;
6. repair canonical event correlation and evidence provenance;
7. commit all corrections before live execution;
8. run one fresh instrumented forward attempt;
9. run one fresh control forward attempt only after the instrumented attempt passes;
10. retain exact sanitized records and update Plan 087/088 authority;
11. hand off Plan 088 as the single next executable plan.

## Non-goals

Plan 093 must not:

- implement the Plan 092 proposed Branch A handshake read correction without new evidence of a pre-auth failure;
- expand the Plan 092 handshake observer merely to satisfy a checklist after the failure was reclassified as data phase;
- patch pinned i2pd Noise, frame encryption, AEAD, key derivation, RouterInfo validation, or normal transport semantics;
- suppress i2pd's automatic local RouterInfo send;
- require the target DeliveryStatus to be the first data-phase frame or first data-phase block;
- accept unlimited unrelated frames while waiting for the target;
- ignore malformed, unauthenticated, wrong-peer, wrong-message-ID, duplicate-target, or termination input;
- increase timeouts as a substitute for a sequencing correction;
- add retries or automatic repeated live attempts;
- activate Plan 088 before both forward records pass;
- advance Plan 079 or Plan 072 directly;
- alter `specs/support.toml` to advertise NTCP2 support.

## Mandatory invariants

```text
RouterInfo signature validation       = unchanged and strict
peer Router Hash binding              = exact
NTCP2 static-key binding              = exact
endpoint binding                      = exact
network ID                            = exact 99
DeliveryStatus envelope ID            = exact configured message ID
DeliveryStatus payload ID             = exact configured message ID
authenticated frame requirement       = mandatory
reference observer                    = passive and compile-time gated
control build                         = no observer call sites
instrumented/control source revision  = identical pinned revision
instrumented/control behavior         = equivalent
receive loop                          = bounded by one absolute deadline
frame count                           = bounded
byte count                            = bounded
observer waits                        = predicate-based and generation-bound
cleanup                               = clean and bounded
binary/source provenance              = exact and nonzero
Plan 088                              = blocked until Plan 093 pass
NTCP2                                 = experimental and non-advertised
```

## Work package 1: correct status and ownership authority

Before implementation, rewrite the current authority sections in:

```text
plans/087-status.md
plans/088-status.md
plans/091-status.md
plans/092-status.md
README.md
AGENTS.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

Required current state:

```text
plan_086 = host-loopback-development-ready
plan_090 = routerinfo-correction-landed
plan_091 = partial-historical-correction
plan_092 = partial-evidence-surface-landed-misclassification-corrected
plan_087 = open_post-auth-data-phase-sequencing-defect
plan_093 = planned_next_executable
plan_088 = blocked_pending_plan_093_instrumented_and_control_pass
plan_079 = blocked_pending_plan_088_two_way-pass
plan_072 = inactive_pending_plan_088_ambiguity
ntcp2 = experimental_non_advertised
```

Required corrections:

1. State that `NTCP2: Receive length read error` originates in `HandleReceivedLength` and is a data-phase read failure.
2. Remove or explicitly supersede the assertion that i2pd read EOF on the first SessionRequest prefix.
3. Withdraw Plan 092 Branch A as an ownership conclusion.
4. Record the new dominant ownership:

   ```text
   owner = mixed corrective surface
   i2pr = one-frame target-DeliveryStatus receive oracle
   i2pd driver = observer reset and target-send predicate lifecycle
   ```

5. Preserve Plan 092's record digest as diagnostic history only.
6. Keep the zero i2pr binary digest labeled non-authoritative.
7. Do not claim the handshake passed solely from the i2pr counter. State that the pinned log location plus i2pr authentication/data counters jointly place the observed EOF in the post-auth data phase; the next instrumented run must still emit both-sided authentication evidence.

Acceptance criteria:

- no current status section describes `Receive length read error` as SessionRequest failure;
- no current status section authorizes a handshake-state-machine correction;
- Plan 093 is the only next executable plan;
- Plan 088 remains blocked;
- historical text is clearly labeled and cannot override the current authority.

## Work package 2: lock the pinned source classification

Add a focused source-verification record and tests that prevent recurrence of the Plan 092 misclassification.

Required source assertions:

1. `NTCP2Session::ReceiveLength()` reads exactly two encrypted data-phase length bytes.
2. `NTCP2Session::HandleReceivedLength()` owns the exact string `Receive length read error`.
3. `NTCP2Session::ServerLogin()` or the equivalent accepted-session path reads SessionRequest and binds `HandleSessionRequestReceived`.
4. SessionRequest failure diagnostics are distinct from data-phase length diagnostics.
5. `NTCP2Session::Established()` calls `transports.PeerConnected()`.
6. Incoming `Transports::PeerConnected()` calls `session->SendLocalRouterInfo()`.
7. `HandleI2NPMsgsSent()` runs only after the asynchronous socket write callback and is the target send-completion seam.

Add `tests/integration/ntcp2/harness/test_plan093.py` cases that read the pinned source-verification metadata and fail if any symbol/string/call relationship changes without an explicit source-lock revision update.

Do not grep an arbitrary current upstream branch. All checks must bind the exact pinned revision and source tree digest used by the driver build.

Acceptance criteria:

- the test deterministically distinguishes handshake-read and data-phase-read diagnostics;
- the automatic inbound RouterInfo send is represented in the test model;
- a future source revision change fails closed until reviewed;
- no runtime network access is needed by the test suite.

## Work package 3: fix i2pd observer reset ownership

The observer sink must be reset before any listener-mode transport thread can emit an event.

Required changes:

1. Move `ResetObserverSink()` to the start of `run_listen()` before `initialise_i2pd_runtime()` starts transports.
2. Do not reset the sink after `listener_ready`.
3. Add a process-generation value owned by the listener invocation, or establish an equivalent immutable baseline token before transport startup.
4. Attach the generation to every observer slot and every event emitted from a slot.
5. A wait must reject metadata from a different generation.
6. Keep the observer callback `noexcept`, nonblocking, allocation-bounded, and free of raw bytes.
7. `ObserverDropCount()` must remain queryable and must be zero for a passing record.

Required regression:

```text
reset observer
start transport
emit listener_ready
emit tcp_accepted immediately
wait for tcp_accepted
=> current-generation event retained
```

Negative regression:

```text
old-generation tcp_accepted exists
start new listener generation
wait for tcp_accepted
=> stale event rejected
```

Acceptance criteria:

- no code path resets current listener observations after readiness;
- a connection accepted immediately after readiness cannot be erased;
- inspect-mode observations cannot satisfy listener-mode waits;
- instrumented and control lifecycle ordering remains equivalent except for passive observer operations.

## Work package 4: replace generic observer waits with baseline-and-predicate waits

The current observer has one cumulative counter and one last slot per category. This is insufficient when i2pd automatically sends RouterInfo before the driver's target DeliveryStatus.

Implement one bounded solution:

### Preferred design: bounded sequence ring

Use a fixed-capacity process-local ring for sent and received I2NP observations.

Required fields per entry:

```text
generation
observation_sequence
i2np_type
i2np_envelope_message_id
delivery_status_message_id
peer_router_hash_sha256
bytes_transferred
frame_sequence
monotonic_ms
```

Capacity must be small and fixed. Overflow increments `ObserverDropCount()` and fails a passing run.

Provide predicate waits:

```text
WaitForReceivedDeliveryStatusAfter(
    generation,
    baseline_sequence,
    expected_peer_router_hash,
    expected_message_id,
    timeout_ms,
)

WaitForSentDeliveryStatusAfter(
    generation,
    baseline_sequence,
    expected_peer_router_hash,
    expected_message_id,
    timeout_ms,
)
```

Both predicates require:

```text
i2np_type = DeliveryStatus
I2NP envelope message ID = expected message ID
DeliveryStatus payload message ID = expected message ID
peer Router Hash = expected peer
observation_sequence > baseline
```

### Acceptable alternative: generation-bound matched slots

A fixed set of generation-bound slots may be used instead of a ring only if it proves that automatic RouterInfo/DatabaseStore sends cannot overwrite or satisfy the target DeliveryStatus wait.

Driver sequence:

1. capture receive baseline before waiting for i2pr's target DeliveryStatus;
2. wait for exact received DeliveryStatus after baseline;
3. emit `frame_authenticated_and_decrypted` and `i2np_message_decoded` only from that exact matched observation;
4. capture sent baseline immediately before submitting the reply;
5. submit the reply through `Transports::SendMessage`;
6. wait for exact sent DeliveryStatus after baseline;
7. emit `frame_emitted` only after the successful socket-write observer callback;
8. only then permit listener shutdown.

Acceptance criteria:

- an automatic RouterInfo/DatabaseStore send cannot satisfy the reply wait;
- an unrelated I2NP message cannot satisfy either target wait;
- a stale target from a prior generation cannot satisfy the wait;
- a wrong message ID cannot satisfy the wait;
- the listener does not shut down until the exact reply write is observed;
- observer overflow/drop prevents a pass.

## Work package 5: make the i2pr DeliveryStatus oracle bounded and multi-frame

Replace the one-frame `receive_delivery_status()` behavior with a bounded receive loop that recognizes valid pre-target data-phase traffic.

Required API shape:

```text
receive_correlated_delivery_status(
    link,
    cancellation,
    absolute_deadline,
    maximum_frames,
    maximum_plaintext_bytes,
    expected_message_id,
    expected_peer_router_hash,
)
```

Required behavior:

1. Create one absolute deadline before the first `recv`.
2. Do not restart or extend the deadline after each non-target frame.
3. Bound:

   ```text
   maximum frames
   cumulative plaintext bytes
   cumulative decoded blocks
   cumulative unrelated I2NP messages
   ```

4. For every authenticated frame:

   - verify frame authentication through the existing runtime;
   - parse blocks with existing strict bounds;
   - process every block in order;
   - reject malformed frames or blocks;
   - reject explicit termination before the target;
   - allow valid padding, options, datetime, and RouterInfo blocks according to the existing decoder and NTCP2 specification;
   - validate a received RouterInfo block as the same peer Router Hash and a valid signature before accepting it as non-target traffic;
   - allow non-target I2NP messages within the configured count bound;
   - do not classify a valid non-target frame as `receiver_delivery_status_missing`.

5. For a DeliveryStatus I2NP block:

   - decode the short transport header;
   - require both envelope and payload IDs to equal the configured message ID;
   - require exactly one matching target;
   - reject a wrong-ID DeliveryStatus with `receiver_delivery_status_id_mismatch` rather than silently skipping it;
   - reject a duplicate matching target;
   - update target correlation counters only after the exact match.

6. Stop with typed results:

```text
receiver_delivery_status_deadline
receiver_delivery_status_frame_limit
receiver_delivery_status_byte_limit
receiver_delivery_status_block_limit
receiver_peer_router_info_invalid
receiver_peer_router_info_mismatch
receiver_termination_before_target
receiver_frame_authentication_failed
receiver_frame_parse_failed
receiver_i2np_decode_failed
receiver_delivery_status_id_mismatch
receiver_delivery_status_duplicate
```

7. Preserve counters:

```text
frames_received = total authenticated frames consumed
non_target_frames_received
non_target_i2np_received
router_info_blocks_received
i2np_received = 1 only when exact target decoded
delivery_status_message_id = exact target ID
expected_peer_router_hash_sha256 = exact peer
```

The normal runtime may expose only the counters needed by the test launcher. Do not add unbounded telemetry or production logging.

Acceptance criteria:

- deterministic test: RouterInfo frame first, exact DeliveryStatus frame second -> pass;
- deterministic test: padding/options frame first, exact target second -> pass;
- wrong-ID DeliveryStatus before target -> typed rejection;
- valid unrelated I2NP before target within bound -> continue;
- frame/byte/block limit -> typed rejection;
- deadline remains absolute and cannot be extended by traffic;
- termination before target -> typed rejection;
- no security validation is weakened.

## Work package 6: correct canonical runner event authority

The runner must classify the corrected forward attempt from current-run evidence rather than fallback prose.

Required changes:

1. Use one shared event-ingestion function for live polling and all final drains.
2. Bind each event to:

```text
run_id
scenario_id
direction
source_side
process invocation ID
event sequence
event SHA-256
binary SHA-256
reference revision
```

3. Do not reuse `scenario_id` as a synthetic invocation ID.
4. Preserve the exact i2pr terminal reason.
5. Wait a bounded post-i2pr drain interval for the i2pd target-send and terminal events before reaping the listener.
6. Reap order:

```text
i2pr terminal observed
bounded drain of i2pd current-generation events
i2pr process reap
i2pd listener natural terminal or bounded reap
cleanup verification
```

7. Classify the stage as data phase when both sides authenticate, even if the target DeliveryStatus does not complete.
8. `reference-events-missing` may be used only after all current-run streams are drained and no more specific terminal exists.
9. A pass requires both:

```text
i2pr terminal result = passed
i2pd terminal result = clean
```

and exact target receive/send events from i2pd.

Acceptance criteria:

- a late exact target send event is retained;
- automatic RouterInfo send does not promote target DeliveryStatus success;
- a missing exact target event fails closed;
- no event with a zero digest contributes to pass;
- stage/prose/counters agree.

## Work package 7: bind exact binary and build provenance

The current wrapper hardcodes the i2pr binary digest to zero. Correct this before another live attempt.

Required wrapper contract:

```text
--i2pr-binary <absolute path>
--i2pd-driver-binary <absolute path>
```

The wrapper must:

1. require both files for a live attempt;
2. reject symlinks or non-regular files under the existing file policy;
3. calculate SHA-256 immediately before execution;
4. pass measured nonzero digests into the canonical runner;
5. verify the i2pr binary was built from the declared clean source commit using the repository's available build identity record or a new bounded build manifest;
6. bind instrumented/control i2pd binary digests to their build manifests and pinned source-tree digest;
7. bind observer patch/source/header digests for the instrumented attempt;
8. prohibit zero placeholders in any attempted live record;
9. allow zero placeholders only in explicit `not_started` or environment-blocker records.

Required live provenance fields:

```text
source_commit
reference_revision
i2pr_binary_sha256
i2pd_binary_sha256
i2pd_build_manifest_sha256
reference_source_tree_sha256
driver_source_sha256
observer_patch_sha256
observer_header_sha256
observer_source_sha256
placement_record_sha256
scenario_sha256
```

Acceptance criteria:

- the Plan 093 instrumented record contains no zero provenance digest;
- the control record identifies the control binary and pristine/control build manifest;
- both records bind the same i2pr corrective commit and pinned i2pd revision;
- a binary rebuilt from another commit is rejected.

## Work package 8: add focused Plan 093 tests

Create:

```text
tests/integration/ntcp2/harness/test_plan093.py
```

Required cases:

1. `Receive length read error` maps to data phase, not SessionRequest.
2. Pinned `HandleSessionRequestReceived` is the handshake-read path.
3. Pinned inbound `PeerConnected` sends local RouterInfo.
4. Plan 092 Branch A is marked superseded in current status authority.
5. Observer reset occurs before transport startup and never after `listener_ready`.
6. Current-generation immediate TCP accept survives readiness.
7. Stale-generation TCP accept is rejected.
8. Automatic RouterInfo send cannot satisfy target DeliveryStatus send wait.
9. An unrelated received I2NP cannot satisfy target receive wait.
10. Exact target predicate requires type, envelope ID, payload ID, peer hash, generation, and post-baseline sequence.
11. Listener shutdown cannot occur before exact target send completion.
12. RouterInfo-first then DeliveryStatus passes the i2pr receive oracle.
13. Valid padding/options before target passes.
14. Wrong-ID DeliveryStatus rejects.
15. Duplicate target rejects.
16. Absolute deadline cannot be refreshed by non-target traffic.
17. Frame limit rejects.
18. Byte limit rejects.
19. Block limit rejects.
20. Invalid or wrong-peer RouterInfo rejects.
21. Termination before target rejects.
22. Runner retains late i2pd target send and terminal events.
23. Runner uses real invocation ID rather than scenario ID aliasing.
24. Zero i2pr binary digest rejects attempted live execution.
25. Instrumented/control mismatch rejects Plan 087 closure.
26. Plan 088 gate requires both exact passing forward records.
27. Plan 079 remains blocked before Plan 088.
28. Plan 072 remains inactive before Plan 088 ambiguity.
29. NTCP2 remains experimental and non-advertised.

Add Rust tests at the smallest owning surface for the bounded multi-frame receive behavior. Do not rely only on Python source-string assertions.

Add i2pd driver tests that execute the instrumented observer implementation against deterministic synthetic observer metadata without opening a public-network socket.

Live binary tests must report executed, passed, failed, and skipped counts. A skipped required live test does not satisfy closure.

## Work package 9: implement and commit before live execution

Required commit boundary:

1. status/source-classification corrections;
2. pinned source-lock tests;
3. i2pd observer reset/generation/predicate changes;
4. i2pr bounded multi-frame receive correction;
5. runner event/provenance corrections;
6. Plan 093 focused tests and static enforcement;
7. all required validation green;
8. clean worktree;
9. commit.

Do not execute the closure attempt from a dirty worktree. Do not combine uncommitted protocol/harness changes with the retained live record.

## Work package 10: execute the instrumented forward attempt

From the exact committed corrective head:

1. rebuild `i2pr-interop`;
2. rebuild the instrumented i2pd driver from the pinned source and observer patch;
3. measure all binaries and manifests;
4. use a fresh run root, identities, ports, invocation IDs, observer generation, and nonzero DeliveryStatus ID;
5. run one `i2pr -> i2pd` attempt;
6. retain one sanitized compact record;
7. delete secret-bearing run state after validation and digest extraction;
8. stop if the attempt does not pass.

Instrumented pass requires all of:

```text
i2pd RouterInfo exact endpoint verified
i2pr TCP connected
i2pd TCP accepted
current-generation observer events only
i2pr NTCP2 authenticated
i2pd NTCP2 authenticated
i2pd automatic RouterInfo/pre-target traffic accepted within bounds
i2pr exact DeliveryStatus frame write completed
i2pd exact DeliveryStatus frame authenticated and decoded
i2pd exact DeliveryStatus reply write completed after reply baseline
i2pr exact DeliveryStatus reply authenticated and decoded
exact envelope and payload message ID correlation
exact peer Router Hash correlation
observer drop count = 0
i2pr terminal = passed
i2pd terminal = clean
cleanup = clean
all binaries/processes terminated
record SHA-256 exact and nonzero
all provenance digests exact and nonzero
```

A handshake-only result, one-sided target observation, stale send observation, or target received without exact reply completion does not pass.

## Work package 11: execute the control forward attempt

Run only after the instrumented attempt passes.

Requirements:

- same i2pr corrective source commit;
- same pinned i2pd revision;
- control driver built from the pristine/control source contract without observer call sites;
- fresh state, ports, message ID, and run root;
- same topology, network ID, deadline classes, receive bounds, and data-phase profile;
- externally visible exact bidirectional DeliveryStatus success;
- clean teardown;
- exact nonzero control record digest.

The control oracle cannot rely on internal observer events. It must prove equivalent external behavior through:

```text
i2pr terminal passed
exact returned DeliveryStatus ID
expected peer Router Hash
control process clean exit
bounded timing and process counters
clean cleanup
```

Instrumented/control disagreement blocks Plan 088.

## Work package 12: retain sanitized evidence and continue the roadmap

After both attempts pass:

1. Create `plans/093-status.md` with:

```text
status = passed
classification = post-auth-data-phase-sequencing-corrected
correction_commit = <exact 40-hex>
forward_instrumented_record_sha256 = <exact 64-hex>
forward_control_record_sha256 = <exact 64-hex>
reference_revision = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
```

2. Retain sanitized records under the repository-approved `target/interop/evidence/` policy or the existing approved tracked evidence-index mechanism.
3. Do not retain raw logs, RouterInfo bytes, identities, keys, configurations, socket captures, payload bytes, or private run paths.
4. Record exact binary, source-tree, manifest, patch, placement, scenario, RouterInfo, Router Hash, and message-ID digests.
5. Delete secret-bearing run roots after evidence validation.
6. Rewrite `plans/087-status.md` to:

```text
status = passed
plan_093 = passed
```

7. Rewrite `plans/088-status.md` to:

```text
decision = not-yet-run
plan_087 = passed
plan_093 = passed
plan_088 = next_executable
```

8. Update README, AGENTS, and the interop skill so Plan 088 is the single next executable plan.
9. Keep Plan 079 blocked and Plan 072 inactive until Plan 088 records its actual decision.
10. Keep NTCP2 experimental, disabled, and non-advertised.

Plan 088 becomes executable only after this reconciliation commit lands.

## Stop and escalation conditions

### Instrumented attempt fails before authentication

If the corrected run lacks both-sided authentication evidence:

- preserve the sanitized record;
- do not apply the old Plan 092 Branch A correction automatically;
- wire the existing metadata-only observed handshake path end-to-end under a separate narrowly scoped diagnostic commit;
- execute one bounded ownership reproduction;
- keep Plan 088 blocked.

### Instrumented attempt authenticates but target exchange fails

Use the exact target predicate and bounded receive reason to assign ownership. Implement no broad retry or fallback.

### Instrumented passes but control fails

Classify as observer-induced behavior or control-driver lifecycle divergence. Do not accept the instrumented pass as closure.

### Both forward attempts pass

Close Plan 087 and hand off Plan 088. Do not run Plan 079 or activate Plan 072 before the Plan 088 decision.

## Required validation commands

At minimum:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan093.py'
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

The rootless and Multipass checks remain static boundary checks only. Plan 093 must not attempt to make either blocked/redundant lane executable.

## Explicit closure criteria

Plan 093 closes only when every item is true:

- [ ] Current status authority classifies `Receive length read error` as data phase.
- [ ] Plan 092 Branch A is explicitly superseded as unsupported by the retained evidence.
- [ ] Pinned source tests bind the diagnostic and automatic inbound RouterInfo behavior.
- [ ] Observer reset occurs before transport startup and never after readiness.
- [ ] Observer events are generation-bound.
- [ ] Target receive waits require exact type, IDs, peer hash, generation, and post-baseline sequence.
- [ ] Target send waits require exact type, IDs, peer hash, generation, and post-baseline sequence.
- [ ] Automatic RouterInfo send cannot satisfy the target reply wait.
- [ ] Listener shutdown waits for actual target asynchronous write completion.
- [ ] i2pr's target receive oracle consumes bounded valid pre-target frames.
- [ ] The receive deadline is absolute and frame/byte/block counts are bounded.
- [ ] Wrong-ID, duplicate, malformed, wrong-peer, termination, timeout, and limit negatives are covered.
- [ ] Live/final runner drains share one current-run ingestion path.
- [ ] Real invocation IDs and event sequences are retained.
- [ ] Attempted live records reject zero provenance digests.
- [ ] The i2pr binary digest is measured and nonzero.
- [ ] Plan 093 focused tests and full required gates pass.
- [ ] All corrections were committed before live execution.
- [ ] A fresh instrumented forward attempt passed the exact bidirectional DeliveryStatus predicate.
- [ ] A fresh control forward attempt produced equivalent external success.
- [ ] Both records bind the same corrective source commit and pinned i2pd revision.
- [ ] Both record digests and all required provenance digests are exact and nonzero.
- [ ] Both attempts have clean teardown and no owned process remains.
- [ ] Secret-bearing run roots were deleted after evidence sanitation.
- [ ] `plans/093-status.md` records `status = passed`.
- [ ] `plans/087-status.md` records `status = passed`.
- [ ] `plans/088-status.md` records Plan 088 as `next_executable` without claiming a reverse result.
- [ ] Plan 079 remains blocked and Plan 072 remains inactive.
- [ ] NTCP2 remains experimental, disabled, and non-advertised.

If any checkbox is false, Plan 088 remains blocked.

## Smaller-model execution sequence

Execute in this exact order:

1. Correct Plan 092's diagnostic classification in all current status authority.
2. Add pinned source-classification tests.
3. Move observer reset before i2pd transport startup.
4. Add observer generation and exact predicate waits.
5. Add stale-event and automatic-RouterInfo-send regressions.
6. Replace i2pr's one-frame receive with the bounded multi-frame target oracle.
7. Add Rust tests for RouterInfo-first and other pre-target traffic.
8. Correct runner event ingestion and real invocation IDs.
9. Measure the i2pr binary and reject zero live provenance.
10. Add `test_plan093.py` and static enforcement.
11. Run focused and full validation.
12. Commit the complete correction.
13. Rebuild all binaries from the exact clean commit.
14. Run one fresh instrumented forward attempt.
15. Stop if it does not pass.
16. Run one fresh control forward attempt.
17. Stop if it does not pass equivalently.
18. Sanitize and validate evidence; delete raw run roots.
19. Reconcile Plans 087, 088, 092, and 093.
20. Commit closure/status updates.
21. Hand off Plan 088 as the single next executable plan.

Do not infer success from listener readiness, TCP connection, authentication alone, one frame received, a generic send counter, a clean process exit, or a green suite with skipped live binaries.

## Handoff state

Until Plan 093 closes:

```text
plan_086 = host-loopback-development-ready
plan_090 = routerinfo-correction-landed
plan_091 = partial-historical-correction
plan_092 = partial-evidence-surface-landed-diagnostic-misclassification
plan_087 = open-post-auth-data-phase-sequencing-defect
plan_093 = planned-next-executable
plan_088 = blocked-pending-plan093-instrumented-and-control-pass
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```
