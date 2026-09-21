# Plan 234 — M6 Java Streaming SYN-ACK and client-LS2 final-closure authority corrective

Status: **passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary**

## 1. Objective

Supersede the unexecuted Plan 233 with one executable corrective that does two
things without reopening already-closed work:

1. attribute and correct the exact Java Streaming Direction-A
   `SYN -> SYN-ACK` boundary exposed by Plan 232 on the corrected route-derived
   lease fixture; and
2. reconcile the Java-family final-closure authority so M6 cannot be marked
   passed while `tests/integration/m6-interop/run-java.sh` still fails required
   Plan-200/201 public-client LeaseSet lifecycle rows.

Plan 234 must either:

- close the Streaming handshake and all retained Java-family closure gates on
  the same exact implementation head; or
- stop at one exact remaining boundary with the already-proven raw-Destination
  pass retained.

It MUST NOT declare Java-family/M6 closure merely because the Streaming data
plane passes if the authoritative Java harness still exits nonzero.

## 2. Registration basis

Plan 232 closed as:

```text
passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
```

On exact implementation SHA:

```text
236ccb63c056349bf784a19edf8a1c1d69064582
```

it proved:

- all three current local LS2 lease sites derive gateway+tunnel from the
  installed inbound route;
- Router B remains the independent NetDB publication target;
- raw i2pr -> Java Destination delivery is digest-matched;
- raw Java -> i2pr reverse delivery is now digest-matched inside the frozen
  45-second window;
- the exact advertised target IBGW exists on Router A;
- i2pr sees the exact reverse TunnelData, tunnel recovery succeeds, Garlic
  decodes, Destination dispatch succeeds, and the payload digest matches;
- Streaming initial lease route parity is live-proven on two counted runs;
- the Streaming SYN is emitted on both runs;
- both runs stop identically at:

```text
SYN-ACK never established
syn_accepted=false
established=false
pump_error=0
```

No production Rust behavior changed in Plan 232.

## 3. Why Plan 233 is superseded before execution

Plan 233's narrow Streaming attribution is retained conceptually, but its
closure semantics conflict with the current executable authority.

The current Java harness still records Plan-200 `C/D rows as required
`ref_row()` entries. `record()` sets `REQUIRED_FAILED=1` for every status other
than `passed`, and the harness exits 1 whenever `REQUIRED_FAILED != 0`.

The retained rows include:

### Plan-200 `C Java client LeaseSet lifecycle

```text
external-java-client-subdb-created
external-java-create-leaseset2-received
external-java-client-leaseset-stored-current
external-java-client-leaseset-publish-scheduled
external-java-client-leaseset-republish-job-ran
```

### Plan-200 `D tunnel/publication lifecycle

```text
external-java-client-inbound-tunnel-eligible
external-java-client-outbound-tunnel-eligible
external-java-floodfill-candidate-available
external-java-store-emitted
external-java-store-ack-observed
external-java-store-failure-reason
```

Plan 232 closure explicitly records these as still unresolved legacy
public-client publication rows. Plan 201's retained handoff rule likewise says
final Java-family closure cannot be claimed until the Java-side LeaseSet
publication boundary is closed.

Therefore Plan 233's `P233-JAVA-SECOND-FAMILY-PASSED` outcome was too broad:
Streaming success alone cannot satisfy current final-closure authority.

Plan 233 is now
`superseded-before-execution-by-plan234-final-closure-authority-corrective`.
There is no Plan-233 implementation SHA or counted execution to preserve.

## 4. Governing closure principle

Plan 234 distinguishes **external interoperability guarantees** from
**historical internal-observability rows**.

A historical row may be removed from the required final-closure set only when
Plan 234 proves, on executed evidence, that a newer external assertion is
strictly equivalent or stronger for the interoperability guarantee the row was
originally protecting.

No row may be converted from required to informational merely because it is
difficult to observe.

The allowed reconciliation outcomes are:

### R1 — row passes normally

Keep the row required and make the existing stock-Java lifecycle evidence pass.

### R2 — row is superseded by stronger executed end-to-end evidence

The plan may reclassify a historical internal-observability row as diagnostic
only when all of the following are documented:

1. the original invariant protected by that row is stated explicitly;
2. a newer executed external row proves the same invariant or a stronger one;
3. the newer row is mandatory and fail-closed in the final Java-family harness;
4. the mapping is one-to-one or many-to-one and recorded in the Plan-234
   closure matrix;
5. `run-java.sh`, the M6 checker, Plan 201 authority, and final closure ledger
   are updated together;
6. the change cannot turn a genuinely broken Java public-client lifecycle into
   a passing external result.

### Forbidden reconciliation

Do not:

- delete a required row without a replacement invariant;
- make `record()` tolerate blocked/failed status globally;
- add `|| true`;
- special-case Plan 234 to ignore `REQUIRED_FAILED`;
- claim publication merely from helper `READY`;
- use unexecuted source reasoning as a substitute for an external row.

## 5. Invariants

1. Java I2P remains exact-pinned at
   `9134f808337b401e8e53c73734c81fab04280c9d`.
2. i2pd remains exact-pinned at
   `635b013a612ff47278ef02acf8580a28e10e26c5`.
3. Plan-232 route-derived lease helper and all `p232_*` parity guards remain
   intact.
4. Plan-230 controlled topology and C1+C2 corrections remain unchanged.
5. Plan-231 read-only post-`ACCEPTED` observability remains intact.
6. Router B remains the NetDB publication target; local lease gateway remains
   route-derived.
7. No public I2P, reseed, VMComm, `netDb.alwaysQuery`, Java source patching,
   reflection, private-field mutation, direct tunnel/NetDB/profile injection,
   or topology expansion.
8. No timeout inflation. Existing Plan-230/231/232 windows remain frozen.
9. Raw Java logs remain scratch-only.
10. No production `src/` change is authorized until an exact i2pr-owned
    Streaming protocol defect is proven.
11. The Plan-232 raw-Destination reverse pass is retained and must not be
    relitigated as a prerequisite campaign.
12. M10 product authority remains closed.

## 6. Work package A — freeze and prove the corrected baseline

Before new Streaming attribution, run the focused Plan-232 guards and prove:

```text
destination route parity                  true
streaming initial route parity            true
publication target distinct from gateway  true
raw forward digest                        retained pass
raw reverse digest                        retained pass or fresh focused recheck
```

A full fresh raw external run is not required before every Streaming attempt if
the exact-head focused guard + retained Plan-232 closure evidence remains
unchanged. If any Plan-232 structural invariant regresses, stop:

```text
P234-A-PLAN232-BASELINE-REGRESSION
```

Do not proceed by retuning the fixture.

## 7. Work package B — exact Streaming SYN epoch

Define one typed Direction-A SYN epoch from:

```text
stream_control START_ACCEPT -> STARTED
i2pr StreamingManager.connect -> SynSent
exact SYN request drained
transport request composed and delivered
Java service-side receive/accept path
Java-generated SYN response / handshake output
i2pr inbound TunnelData
tunnel recovery
Garlic decode
StreamingDestinationAdapter::receive
connection state -> Established
```

Record at minimum:

```text
local_connection_id
local_destination_hash
remote_destination_hash
local_port
remote_port
syn_sequence_or_message_identity
syn_transport_request_emitted
java_accept_thread_started
java_accept_returned
java_stream_socket_count_delta
java_stream_receive_observed
java_stream_response_observed
i2pr_inbound_tunneldata_count
i2pr_expected_stream_tunneldata_count
i2pr_tunnel_recovery_count
i2pr_garlic_payload_count
i2pr_streaming_adapter_calls
i2pr_streaming_adapter_successes
i2pr_streaming_adapter_errors
i2pr_connection_state
pending_outbound_after_receive
ack_poll_emissions
retransmit_poll_emissions
```

Only bounded booleans/counts/ids/hashes/ports/stage tokens may become durable
evidence.

## 8. Work package C — Java helper-side accept observability

`ReferenceStreamingService` currently exposes control commands:

```text
START_ACCEPT
ACCEPT_STATUS
START_CONNECT
CONNECT_STATUS
WRITE
READ
EOF
CLOSE
```

Add the minimum read-only helper observability needed to distinguish:

1. accept worker requested;
2. accept worker entered `server.accept()`;
3. `server.accept()` returned a socket;
4. socket was stored in the helper table;
5. helper-side exception occurred before accept completed.

Do not expose packet contents, keys, tags, or private router internals.

A bounded command such as `REPORT_STREAM_STATE` or equivalent is acceptable if
it only reports counters/state already owned by the helper.

Required terminals include:

```text
P234-B-JAVA-ACCEPT-WORKER-NOT-STARTED
P234-B-JAVA-SYN-NOT-ACCEPTED
P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED
```

Do not assume `server.accept()` returning is equivalent to the exact wire stage;
document its pinned public API semantics if used as a stage boundary.

## 9. Work package D — i2pr inbound SYN-response attribution

Replace the current coarse
`syn_accepted=false established=false pump_error=0` stop with separate bounded
counters for:

```text
no inbound TunnelData at all
unrelated TunnelData only
expected inbound tunnel observed
tunnel recovery failure
Garlic decode failure
no queued destination payload
Streaming adapter rejected/errored
Streaming adapter dispatched but connection not Established
response accepted and Established
```

Current code silently continues on several recovery/decode paths. Plan 234 must
not collapse those paths into "SYN-ACK never established".

Terminals:

```text
P234-C-I2PR-NO-EXPECTED-TUNNELDATA
P234-C-I2PR-TUNNEL-RECOVERY-FAILED
P234-C-I2PR-GARLIC-DECODE-FAILED
P234-C-I2PR-NO-STREAMING-PAYLOAD
P234-C-I2PR-STREAMING-ADAPTER-FAILED
P234-C-I2PR-DISPATCHED-NOT-ESTABLISHED
P234-C-STREAMING-DIRECTION-A-ESTABLISHED
P234-C-OBSERVABILITY-GAP
```

If the exact inbound Streaming response reaches i2pr and an i2pr-owned
parser/state transition fails, stop and register a separate production
corrective. Do not repair production behavior inside this attribution plan.

## 10. Work package E — narrow corrective

If B/C attribution proves a test-helper or harness defect, correct only that
defect in Plan 234.

Examples that are in scope only if proven:

- helper accept worker readiness/serialization;
- control-plane race that starts accept too late;
- test pump ordering that fails to service timers;
- missing retransmit/ACK poll in the bounded test driver;
- incorrect test port/destination binding.

Do not alter production Streaming semantics to satisfy the test unless WP D
proves the production defect first; in that case Plan 234 stops and registers a
dedicated production corrective.

After a test-only corrective, rerun the exact same Streaming epoch without
changing topology/windows.

## 11. Work package F — retained Streaming qualification

Once Direction A establishes, continue through the already-defined Java
Streaming qualification:

### Direction A

- Java helper accepts the stream;
- small i2pr -> Java payload digest match;
- multi-packet i2pr -> Java digest match;
- Java -> i2pr reverse small payload digest match;
- Java -> i2pr reverse multi-packet digest match;
- sibling connection isolation;
- close/EOF;
- manager cleanup.

### Direction B

- refreshed local LS2 re-derives the installed inbound route live;
- refresh publication separation remains true;
- Java `START_CONNECT` -> i2pr listener accept;
- Java -> i2pr and i2pr -> Java payload digests match;
- close/EOF;
- cleanup.

The previously unit-only Streaming refresh parity MUST execute live before a
Streaming-complete result may be claimed.

Successful Streaming terminal:

```text
P234-F-JAVA-STREAMING-PASSED
```

This terminal does **not** by itself close Java-family M6.

## 12. Work package G — Plan-200/201 final-closure authority reconciliation

After `P234-F-JAVA-STREAMING-PASSED`, execute the full Java harness with the
default/full selector on the same exact implementation SHA.

Produce a row-by-row matrix for every Plan-200 `C/D required row:

```text
row
current harness status
original invariant
current evidence source
resolution = PASS | SUPERSEDED-BY-STRONGER-EXTERNAL-EVIDENCE | STILL-BLOCKING
replacement mandatory row(s), if superseded
justification
```

### Required external facts available for potential supersession

Plan 234 may use only executed mandatory evidence such as:

- Java public-client Destination was created through stock public I2CP;
- i2pr resolves the Java public client's current LS2 through the real NetDB
  path;
- the resolved Destination/hash/signature are valid;
- i2pr selects its advertised lease and sends through a real outbound tunnel;
- Java receives the bounded raw payload digest-matched;
- Java Streaming service LS2 resolves through the same external path;
- Java Streaming accepts an inbound stream;
- bidirectional Streaming payloads and close/EOF pass;
- live refresh/republication remains externally discoverable and usable.

These facts may supersede internal Java log-event rows only where they prove the
same interoperability invariant more strongly.

### Rows that must not be casually superseded

A row whose invariant covers a lifecycle behavior not implied by successful
external use, such as an actual republish lifecycle after expiry/refresh, must
either:

- be exercised and pass; or
- be replaced by an explicit external refresh/republication test that proves
  the lifecycle behavior.

### Final harness requirement

After reconciliation, `run-java.sh` MUST exit 0 on the full Java-family lane.

The fix may adjust which legacy rows are classified as required vs diagnostic
only, but only after the matrix above establishes replacement authority.
`REQUIRED_FAILED` itself remains fail-closed.

## 13. Work package H — cross-family final closure

Java-family M6 may close only after all of the following are green on one exact
head:

1. Plan-232 route-derived lease structural guards;
2. retained raw-Destination bidirectional external evidence;
3. Plan-234 Streaming Direction A+B external qualification;
4. live refresh/republication evidence;
5. full `tests/integration/m6-interop/run-java.sh` exit 0;
6. `bash scripts/check-m6-mixed-router-acceptance-evidence.sh`;
7. manual/cross-family M6 evidence generation produces a passing Java row set;
8. `bash scripts/check-m6-final-closure-evidence.sh` passes;
9. first-family i2pd authority remains retained-passed;
10. no mandatory row is blocked/failed.

Only then emit:

```text
P234-JAVA-SECOND-FAMILY-PASSED
```

and update:

```text
milestone6_java_mixed_router_interop = passed
milestone6_interoperable = passed
plan_201 = passed/closed according to final authority
plan_204 = dependency-ready for cross-milestone convergence
```

## 14. Required focused tests

Add at least equivalent coverage for:

```text
p234_plan232_route_parity_is_prerequisite
p234_start_accept_precedes_syn_send
p234_java_accept_worker_state_is_bounded
p234_no_expected_tunneldata_is_distinct_from_decode_failure
p234_tunnel_recovery_failure_is_distinct_from_no_wire
p234_garlic_failure_is_distinct_from_streaming_adapter_failure
p234_adapter_dispatch_without_established_is_distinct
p234_direction_a_pass_does_not_close_java_family
p234_direction_b_requires_live_refresh_parity
p234_plan200_c_d_rows_require_pass_or_explicit_supersession_mapping
p234_superseded_row_requires_stronger_mandatory_external_evidence
p234_unmapped_legacy_required_row_blocks_closure
p234_run_java_exit_zero_required_for_family_pass
p234_final_closure_checker_required
p234_no_global_required_failed_bypass
p234_no_production_surface_change_before_owned_defect
```

## 15. Static evidence guards

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh` to reject:

1. removal/regression of Plan-232 `25 guards;
2. any `P234` token under production `src/` before a separately registered
   production corrective;
3. `|| true` or equivalent forgiveness in the Java/final harness;
4. changes that make `record()` treat blocked/failed as success;
5. special-casing Plan 234 to ignore `REQUIRED_FAILED`;
6. deleting any Plan-200 `C/D row without an explicit Plan-234 mapping;
7. a "superseded" mapping without replacement mandatory external row(s);
8. a family-pass token while `run-java.sh` can still exit nonzero;
9. a family-pass token without the final closure checker;
10. lease-gateway/publication-target conflation;
11. timeout/window inflation;
12. raw-log promotion.

## 16. Attempt discipline

Implementation MUST be committed before counted external execution.

Maximum three counted attempts per implementation SHA.

No between-attempt tuning.

If attribution proves a test-only issue and a corrective commit is necessary,
a new implementation SHA gets its own maximum-three-attempt budget. Document
the reason for the second SHA explicitly.

Infrastructure death before the Plan-234 SYN epoch may be VOID only when no
Plan-234 terminal was emitted.

Natural profile/bootstrap stochasticity remains governed by the existing
frozen gates; do not extend timers.

## 17. Verification floor

The closing implementation SHA must pass:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-final-closure-evidence.sh

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'

cargo test --locked -p i2pr-daemon --test java_tunnel_external p234_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p232_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p231_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run

bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash -n scripts/check-m6-final-closure-evidence.sh
javac <all staged Java probes/helpers against exact pinned Java jars>
```

For final-family closure, also execute the full Java external harness and the
cross-family M6 harness/workflow-equivalent on the same exact head. A focused
`I2PR_M6_JAVA_DRIVER=streaming` pass is insufficient for final closure.

## 18. Acceptance criteria

Plan 234 closes successfully only when:

1. Plan 233 remains unexecuted and superseded;
2. Plan-232 route-derived lease correction remains intact;
3. the current SYN-ACK stop is attributed to exactly one earliest stage;
4. only the justified narrow test/harness corrective lands, unless an
   i2pr-owned production defect is proven and handed to a separate plan;
5. Java Streaming Direction A passes;
6. Java Streaming Direction B passes;
7. Streaming refresh/republication parity is live-proven;
8. every Plan-200 `C/D required row either passes or has an explicit
   stronger-external-evidence supersession mapping;
9. no required invariant is silently dropped;
10. full `run-java.sh` exits 0;
11. the M6 mixed-router checker passes;
12. the final closure evidence checker passes;
13. full exact-head routine verification passes;
14. only then may Java-family M6 be marked passed;
15. Plan 201/204/registry/roadmaps are updated together.

## 19. Closure outcomes

### Outcome A — full Java-family closure

```text
P234-JAVA-SECOND-FAMILY-PASSED
```

Requires all `18` criteria. No ceremonial successor.

### Outcome B — Streaming remains blocked

Emit the exact earliest `P234-B/C/F-*` terminal. Retain raw-Destination pass.
Register only the narrow successor justified by that stage.

### Outcome C — Streaming passes, final authority still blocked

```text
P234-STREAMING-PASSED-FINAL-AUTHORITY-BLOCKED
```

The closure record must enumerate the still-blocking Plan-200/201 rows. Do not
mark Java-family M6 passed.

### Outcome D — i2pr-owned Streaming defect

Record the exact i2pr-owned stage and register a dedicated production
corrective. Plan 234 itself makes no production fix.

## 20. Closure evidence

`plans/closure/mixed-router-interop/234-status.md` must contain:

- implementation SHA(s);
- exact pins;
- proof Plan 233 never executed;
- retained Plan-232 route parity;
- per-attempt SYN epoch facts;
- Java helper accept-state facts;
- i2pr receive/recovery/Garlic/Streaming stage counts;
- exact Plan-234 terminal per counted run;
- Direction A/B Streaming evidence if reached;
- live refresh/republication evidence if reached;
- complete Plan-200 `C/D reconciliation matrix;
- full `run-java.sh` exit/result summary;
- final M6 closure ledger/checker result;
- full exact-head verification;
- Plan-201/204 unblock audit;
- M10-unchanged confirmation.

## 21. Registration disposition

```text
plan_232 = passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
plan_233 = superseded-before-execution-by-plan234-final-closure-authority-corrective
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
plan_235 = registered-ready-m6-java-streaming-post-accept-response-boundary-corrective

plan_201 = blocked-pending-plan235-java-streaming-post-accept-response-boundary-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan235
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 235-m6-java-streaming-post-accept-response-boundary-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

## 22. Handoff notes for smaller-model execution

Do not touch the Plan-232 lease fixture.

Start with the existing Streaming-only run and add stage counters around the
current `START_ACCEPT -> SYN sent -> inbound pump` loop. Determine whether Java
ever accepts the stream and whether any response enters i2pr before changing
behavior.

If a test-driver race is proven, fix only that race and re-run.

If Streaming passes, do not stop. Run the full Java harness. The old Plan-200
`C/D` rows are currently required by executable harness semantics. Either make
them pass or produce an explicit replacement-invariant mapping backed by
stronger mandatory end-to-end evidence, then make the harness/Plan-201/final
closure authority agree.

A green focused Streaming test with a red full Java harness is not M6 closure.
