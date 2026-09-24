# Plan 247 — M6 Java Streaming Plan-246 observation-window/parser corrective

Status: **registered-ready-m6-java-streaming-plan246-observation-window-parser-corrective**

## 1. Objective

Correct the Plan-246 observer before making any further claim about stock Java
Streaming delayed-ACK behavior.

Plan 246 closed at:

```text
P246-OBSERVABILITY-GAP
```

on all three counted attempts. Direction A established 3/3, but the retained
Plan-245 scheduler-reschedule prerequisite disappeared from the counted
Plan-246 evidence and the new timer readiness facts were reported false.

Post-closure source and harness review identified three concrete observer
defects that are sufficient to invalidate any deeper timer interpretation:

1. the Rust `TIMER_STATS` parser requires `seen == 17` but increments
   `seen` for only 16 parsed data fields because `java_source_pin` is
   accepted with `continue`;
2. the Plan-246 two-second polling window is entered only after the retained
   Plan-245 post snapshot and classification, which are taken after the frozen
   45-second response window;
3. the polling loop repeatedly derives SchedulerReceived deltas from the same
   frozen `p245_post` snapshot instead of refreshing the response counters
   during polling.

Plan 247 corrects only those observer defects and re-executes the same
delayed-ACK attribution question.

No Java source/jar patch, Java behavior change, ACK-delay override, topology
change, publication change, protocol workaround, or production i2pr change is
authorized.

## 2. Research conclusion

The correct next action is an observer corrective, not a Java timer corrective.

### 2.1 Exact Plan-246 parser defect

The helper emits:

```text
peer_correlation_present
simple_timer_debug_enabled
timer_scheduler_count
timer_running_count
timer_early_reschedule_count
timer_finished_count
connection_timer_first_schedule_timeout_ms
connection_timer_latest_schedule_timeout_ms
connection_timer_min_schedule_timeout_ms
connection_timer_max_schedule_timeout_ms
first_reschedule_delta_ms
latest_reschedule_delta_ms
min_reschedule_delta_ms
max_reschedule_delta_ms
connection_timer_first_run_elapsed_ms
context_clock_minus_system_ms
java_source_pin
```

The first sixteen fields are counted by `p246_parse_timer_stats()`.
`java_source_pin` is accepted and ignored with `continue`, so `seen`
remains 16.

The parser then requires:

```rust
(seen == 17).then_some(stats)
```

A structurally valid helper response can therefore collapse to `None`.
Downstream `P246StageA1::default()` then reports:

```text
simple_timer_isolatable=false
peer_correlation_present=false
```

Those false values are not reliable evidence that the helper actually reported
both facts false.

### 2.2 Plan-246 observation window starts too late

The retained Plan-245 code takes `p245_post` only after the frozen response
window. Plan 246 inserts its 50 ms × 40 polling loop after the Plan-245
classification derived from that post snapshot.

The effective observation ordering is therefore:

```text
pre-SYN response snapshot
-> SYN / Direction-A response epoch
-> frozen 45-second response window
-> Plan-245 post snapshot/classification
-> Plan-246 TIMER_STATS pre snapshot
-> Plan-246 2-second polling
```

Exact-pinned Java schedules the initial delayed ACK from the inbound packet
path. An observer that begins roughly 45 seconds later cannot be treated as a
reliable observer of that one-shot lifecycle.

### 2.3 Plan-246 does not refresh SchedulerReceived during polling

Inside the new polling loop, the code recomputes:

```text
p245_post.scheduler_reschedule_branch_log_count
-
p245_pre.scheduler_reschedule_branch_log_count
```

on every iteration.

`p245_post` is immutable. Therefore the loop cannot discover a second
SchedulerReceived event that appears after the Plan-245 post snapshot.

Plan 247 must poll both the response stats and timer stats live.

### 2.4 Exact Java delayed-ACK source remains valid

Retain exact-pinned Java I2P 2.13.0:

```text
9134f808337b401e8e53c73734c81fab04280c9d
```

Source facts remain:

```text
ConnectionPacketHandler
  -> incrementUnackedPacketsReceived()
  -> setNextSendTime(delay + context.clock().now())
  -> con.eventOccurred()

ConnectionOptions.DEFAULT_INITIAL_ACK_DELAY = 500 ms

Connection.setNextSendTime()
  -> clamps the future deadline to now + getSendAckDelay()

SchedulerReceived
  -> timeTillSend > 0
  -> reschedule(timeTillSend, con)

SchedulerImpl.reschedule
  -> Connection.scheduleConnectionEvent

Connection.scheduleConnectionEvent
  -> SimpleTimer2.addEvent

ConEvent.timeReached
  -> Connection.eventOccurred
  -> SchedulerChooser
```

### 2.5 Requested delay is not a real-time execution guarantee

Java's `ScheduledThreadPoolExecutor` contract says delayed tasks execute no
sooner than their requested delay, but provides no real-time guarantee for how
soon after becoming eligible they actually commence.

Therefore:

- `timeTillSend > 500 ms` under the frozen default remains a meaningful
  deadline/source contradiction;
- a matching event that executes after 500 ms is not by itself a Java defect;
- Plan 247 must distinguish requested delay from actual timer execution
  latency.

This is why the correct corrective is early continuous observation with
explicit scheduling/runtime terminals, not a hard 500 ms execution deadline.

## 3. Retained authority

Retain unchanged:

- Plan 232 route-derived lease-gateway fixture;
- Plans 237–245 response and downstream observers;
- Plan 242 stock one-hop selector semantics;
- Plan 243 hosted qualification rules;
- Plan 244 response-epoch correlation;
- Plan 245 direct buildPacket signal;
- Plan 246 exact-pinned timer/source-lock facts that are source-correct;
- A/B/C controlled topology;
- one-hop non-zero client-tunnel policy;
- Router-B publication target;
- frozen 45-second outer response window;
- fresh RouterContexts per counted attempt;
- maximum three counted attempts per implementation SHA;
- no retry-until-C;
- no RNG manipulation;
- no zero-hop fallback;
- no Java patch;
- no production i2pr change before exact reverse TunnelData reaches i2pr.

Plan 246 remains historical authority for the fact that its attempted timer
attribution closed at an observation gap. Plan 247 supersedes only the
interpretation of the Plan-246 helper readiness booleans and timing window.

## 4. Correct the TIMER_STATS parser first

Replace the ambiguous magic field count with explicit schema validation.

Preferred shape:

```rust
let mut seen_data = 0u8;
let mut seen_source_pin = false;

match key {
    ... 16 data keys => {
        parse...
        seen_data += 1;
    }
    "java_source_pin" => {
        if value != EXPECTED_JAVA_SOURCE_PIN { return None; }
        seen_source_pin = true;
    }
    _ => return None,
}

(seen_data == 16 && seen_source_pin).then_some(stats)
```

Do not simply change `17` to `16` while continuing to ignore the source
pin. The parser must validate the pin and prove complete schema coverage.

Add a round-trip unit fixture copied from the helper's exact emitted
`TIMER_STATS` field shape.

Required tests:

```text
p247_timer_stats_exact_helper_shape_parses
p247_timer_stats_requires_all_16_data_fields
p247_timer_stats_requires_source_pin
p247_timer_stats_rejects_wrong_source_pin
p247_timer_stats_rejects_unknown_field
p247_timer_stats_does_not_default_parse_failure_to_false_readiness
```

## 5. Preserve Unknown separately from false

Plan 246 collapses a missing/failed timer-stats response into default false
booleans.

Plan 247 must represent:

```text
Unknown
Observed(false)
Observed(true)
```

for observer readiness.

A parse failure or unavailable helper response must emit a typed observer
terminal such as:

```text
P247-O-TIMER-STATS-PARSE-FAILED
```

It must not be recorded as:

```text
simple_timer_debug_enabled=false
peer_correlation_present=false
```

unless those false values were actually parsed from a valid helper response.

## 6. Move timer observation into the response epoch

The corrected ordering must be:

```text
configure observers
-> validate timer-stats schema/readiness
-> pre-SYN RESPONSE_STATS snapshot
-> pre-SYN TIMER_STATS snapshot
-> initiate SYN
-> immediately begin rolling RESPONSE_STATS + TIMER_STATS polling
-> retain short-poll rolling facts
-> continue existing 45-second outer response lane
-> final retained Plan-245/244 snapshots
```

The 45-second acceptance window remains unchanged.

Plan 247 does not extend or shorten protocol timing. It only moves the
observation window to the event being observed.

## 7. Live dual polling

During the attribution window, every poll must refresh both:

```text
REPORT_RESPONSE_STATS
REPORT_TIMER_STATS
```

The polling loop must maintain rolling maxima / first-seen timestamps for:

```text
SchedulerReceived generic signal
scheduler send branch
scheduler reschedule branch
scheduler no-unacked warning
MessageOutputStream flush
writeData doSend=false
buildPacket
sendMessage lifetime-event delta

SimpleTimer2 schedule
SimpleTimer2 run
SimpleTimer2 early-reschedule
SimpleTimer2 finish
```

Do not reuse the frozen Plan-245 `p245_post` inside the live loop.

A second scheduler event is proven only from a fresh live response snapshot
whose count exceeds the first event's rolling baseline.

## 8. Polling cadence and horizons

Use an early high-resolution phase followed by a bounded low-rate tail.

Recommended observational schedule:

```text
phase 1:
  50 ms interval
  first 2,000 ms after SYN

phase 2:
  100 ms interval
  from 2,000 ms through 5,000 ms
  only while timer schedule/run or second scheduler outcome remains unresolved
```

This is still wholly inside the existing 45-second outer lane.

The 500 ms ACK setting bounds the requested `timeTillSend`; it does not
guarantee executor dispatch by 500 ms.

If a matching event is scheduled but not run by 5 seconds, classify that as a
bounded runtime attribution result, not automatically as a Java defect.

Do not poll for 45 seconds at 50 ms cadence.

## 9. Observer readiness before a counted attempt

Before the SYN of each counted attempt, require:

```text
REPORT_RESPONSE_STATS parses
scheduler_debug_enabled=true
connection_debug_enabled=true
receiver_debug_enabled=true
message_output_enabled=true

REPORT_TIMER_STATS parses
simple_timer_debug_enabled=true
java_source_pin matches
console buffer available
```

Peer correlation is naturally unavailable before an inbound socket exists and
must not fail pre-SYN readiness.

After Direction A accept/socket establishment, require peer correlation to
become available within a bounded observation interval before using any
exact-socket timer terminal.

If it does not:

```text
P247-O-PEER-CORRELATION-NOT-AVAILABLE
```

This is an observer terminal only.

A run that fails observer readiness before SYN is not a counted protocol
attempt. Correct the observer and start the three-attempt budget only after
readiness passes.

## 10. Exact-socket correlation

Retain the Plan-246 privacy requirement:

- the peer Destination/b32 may exist only in helper memory;
- raw Connection/SimpleTimer2 lines remain scratch-only;
- durable evidence contains only sanitized counts/delays/booleans.

Retain lazy `I2PSocket.getPeerDestination()` correlation after accept.

Do not require the peer marker before SYN.

If exact correlation becomes available after the timer lifecycle line was
written, the helper may scan the still-bounded console buffer retroactively for
matching lines.

Do not weaken the terminal to "any SimpleTimer2 event".

## 11. Bound and observe console-buffer pressure

`net.i2p.util.SimpleTimer2` DEBUG is class-wide and can emit unrelated timer
lifecycle records.

Plan 247 must record public bounded buffer pressure during the short polling
window:

```text
console_buffer_entries
console_buffer_capacity
console_buffer_at_capacity
```

If the public API does not expose capacity directly, use the configured
`P237_CONSOLE_BUFFER_SIZE` constant as the documented capacity and count the
returned messages.

Do not increase the buffer preemptively.

If the buffer reaches capacity before exact-socket correlation and required
signals are captured:

```text
P247-O-CONSOLE-BUFFER-SATURATED
```

Only that terminal authorizes a later observer-capacity change.

This prevents global SimpleTimer2 DEBUG traffic from silently erasing the
retained SchedulerReceived evidence.

## 12. Correct Plan-245 rolling baseline semantics

Define the first observed SchedulerReceived response event inside the live
window.

Track:

```text
first_scheduler_count
first_reschedule_count
first_send_count
first_no_unacked_count
```

A Plan-245-compatible reschedule boundary exists when the live rolling delta
proves:

```text
scheduler_reschedule_branch_delta >= 1
scheduler_send_branch_delta == 0
```

The timer chain can then be correlated against that first reschedule.

Do not require the 45-second final snapshot to preserve the first scheduler
line.

If live rolling evidence proves the Plan-245 reschedule but the final
45-second snapshot does not, record:

```text
P247-O-PLAN245-FINAL-SNAPSHOT-EVICTION
```

as a companion observer fact, while the live rolling evidence remains
authoritative.

## 13. Correct delayed-ACK attribution chain

Once the first live SchedulerReceived reschedule is observed:

```text
SchedulerReceived reschedule
-> matching SimpleTimer2 schedule
-> matching SimpleTimer2 run
-> next Connection.eventOccurred / scheduler outcome
```

Classify the earliest proven boundary.

### A. Requested deadline contradiction

If the exact SchedulerReceived `timeTillSend` is:

```text
<= 0 before the reschedule branch
or
> 500 ms under the frozen helper default
```

emit:

```text
P247-A-NEXT-SEND-DEADLINE-OUT-OF-BOUNDS
```

### B. Reschedule observed, no timer schedule

With valid observer readiness and unsaturated buffer:

```text
P247-B-CONNECTION-EVENT-NOT-SCHEDULED
```

### C. Timer scheduled, not run inside bounded tail

If a matching timer schedule is observed but no matching run is observed by
the end of the 5-second attribution tail:

```text
P247-B-CONNECTION-EVENT-SCHEDULED-NOT-RUN-WITHIN-ATTRIBUTION-WINDOW
```

This is a runtime scheduling boundary, not by itself a Java correctness defect.

### D. Timer runs, scheduler changes

If matching run is observed but SchedulerReceived does not own the next
connection event:

```text
P247-C-CONNECTION-EVENT-RAN-SCHEDULER-CHANGED
```

Capture sanitized socket/connection state only.

### E. SchedulerReceived reschedules again

```text
P247-C-SCHEDULER-RESCHEDULED-AGAIN
```

Record the fresh second delay and clock offset.

If repeated reschedules consume the bounded tail:

```text
P247-C-REPEATED-RESCHEDULE-WITHOUT-SEND
```

### F. No-unacked branch

```text
P247-C-SCHEDULER-NO-UNACKED-ON-SECOND-EVENT
```

### G. Send branch

```text
P247-D-SCHEDULER-SEND-BRANCH-REACHED
```

Immediately resume the existing Plan-245 downstream chain.

## 14. Resume existing downstream attribution, do not create a new gap

If the send branch is reached, continue in the same response epoch:

```text
sendAvailable
-> MessageOutputStream.flushAvailable
-> ConnectionDataReceiver.writeData
-> buildPacket
-> Connection.sendPacket
-> PacketQueue / I2PSession.sendMessage
-> Router-A I2CP admission
-> exact Streaming target lookup
-> Router-B query/reply
-> helper client sub-DB
-> OCMOSJ/tunnel dispatch
-> exact reverse i2pr TunnelData
-> recovery
-> Garlic
-> Streaming
```

Use existing Plan-237/238/239/240/244/245 typed terminals.

Do not stop merely because Plan 247's timer question is answered if deeper
same-epoch evidence is already available.

## 15. Correct Plan-246 closure interpretation in successor evidence

Do not rewrite Plan 246's historical result.

Plan 247 closure must explicitly state:

- Plan 246's `P246-OBSERVABILITY-GAP` remains historical;
- the recorded false readiness values cannot be treated as authoritative
  helper facts where `TIMER_STATS` parsing failed;
- Plan 247 supersedes only that interpretation;
- Direction A 3/3 and the no-production-change facts remain valid.

## 16. Verification-floor correction

Plan 246's closure described:

```text
394 passed, 5 ignored
```

as a "full serial workspace floor", but the recorded command is the
`java_tunnel_external` test target.

Plan 247 must distinguish:

```text
focused java_tunnel_external floor
full workspace all-target serial floor
```

and must actually run:

```bash
cargo test --locked --workspace --all-targets -- --test-threads=1
```

before closure.

The closure record must report both result sets separately.

No hosted CI/status result may be invented if GitHub has none.

## 17. Expected implementation surfaces

Prefer only:

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
tests/integration/m6-interop/java/ReferenceStreamingService.java
tests/integration/m6-interop/run-java.sh
scripts/check-m6-mixed-router-acceptance-evidence.sh
scripts/interop/check-m6-java-response-source-lock.sh
```

No production `src/` file may change.

No Java I2P source file or jar may change.

## 18. Static checker requirements

Extend the acceptance checker to reject:

- `seen == 17` timer parser logic without explicit source-pin accounting;
- timer-stats parse failure silently becoming false readiness;
- Plan-247 polling after the retained 45-second post snapshot;
- live scheduler attribution derived only from frozen `p245_post`;
- a counted attempt beginning before observer readiness passes;
- peer-correlation readiness required pre-SYN;
- raw peer Destination/b32 in durable evidence;
- raw Connection/SimpleTimer2 log lines in durable evidence;
- classification of a >500 ms actual callback latency as a defect solely
  because the requested ACK delay is 500 ms;
- buffer saturation ignored while relying on absence of a log signal;
- production Rust changes;
- Java source/jar patches;
- ACK-delay overrides;
- outer 45-second response-window changes.

## 19. Required focused tests

Add at least:

```text
p247_timer_stats_exact_helper_shape_parses
p247_timer_stats_requires_all_data_fields
p247_timer_stats_requires_matching_source_pin
p247_parse_failure_is_unknown_not_false
p247_observer_readiness_precedes_syn
p247_peer_correlation_not_required_pre_syn
p247_peer_correlation_required_before_exact_timer_terminal
p247_polling_starts_inside_response_epoch
p247_polling_refreshes_response_stats_each_iteration
p247_polling_refreshes_timer_stats_each_iteration
p247_live_scheduler_delta_not_derived_from_frozen_post
p247_rolling_scheduler_evidence_survives_final_snapshot_eviction
p247_buffer_pressure_is_recorded
p247_buffer_saturation_blocks_absence_inference
p247_requested_delay_bound_distinct_from_execution_latency
p247_timer_schedule_requires_first_reschedule
p247_timer_run_requires_schedule
p247_second_scheduler_requires_timer_run
p247_send_branch_resumes_plan245_chain
p247_no_java_patch
p247_no_ack_delay_override
p247_no_outer_window_change
p247_no_production_change
p247_full_workspace_floor_is_distinct_from_focused_floor
```

Retain the focused P237–P246 floor.

## 20. Counted execution discipline

After implementation is committed:

1. run Plan-243 host qualification;
2. run parser/schema/readiness tests;
3. run source-lock and static/evidence checker;
4. prove pre-SYN observer readiness on the exact implementation SHA;
5. only then begin the three counted attempts;
6. use one implementation SHA for all three counted attempts;
7. fresh A/B/C RouterContexts per attempt;
8. unique evidence directory per attempt;
9. no between-attempt tuning;
10. no retry-until-C;
11. no ACK-delay or outer-window change;
12. no Java patch;
13. no production change before the exact reverse-TunnelData gate.

A readiness failure before SYN does not consume a counted protocol attempt.

## 21. Per-attempt evidence

Record:

```text
implementation SHA
Java source pin
source-lock checksum
observer-readiness result
timer-stats parser/schema result
pre-SYN response snapshot
pre-SYN timer snapshot
poll cadence and actual poll count
rolling response maxima + first-seen timestamps
rolling timer maxima + first-seen timestamps
peer-correlation first-available timestamp
buffer entries/capacity/at-capacity samples
requested first/second reschedule delay
timer schedule/run timestamps
clock-offset samples
final 45-second response snapshot
final timer snapshot
final-snapshot eviction companion flags
deepest Plan-247 terminal
downstream Plan-245 terminal if resumed
no-tuning proof
no-Java-patch proof
no-production-change proof
```

## 22. Verification floor

Run and record separately:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --doc
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo deny check advisories bans sources

cargo test --locked -p i2pr-daemon --test java_tunnel_external p247_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>
bash -n tests/integration/m6-interop/run-java.sh

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'

cargo test --locked --workspace --all-targets -- --test-threads=1
```

The final command is the full serial workspace all-target floor and must not be
replaced in the closure record by the focused `java_tunnel_external` count.

## 23. Acceptance criteria

Plan 247 closes correctly only when:

1. the TIMER_STATS parser round-trips the exact helper schema;
2. the exact Java source pin is validated from the parsed response;
3. parse failure remains Unknown rather than false readiness;
4. observer readiness is proven before SYN;
5. live timer/response polling starts inside the response epoch;
6. RESPONSE_STATS and TIMER_STATS are refreshed on every attribution poll;
7. rolling scheduler evidence can survive final-snapshot eviction;
8. buffer pressure is measured and gates absence inference;
9. requested ACK delay is kept distinct from actual executor latency;
10. exact-socket correlation is proven before exact timer terminals;
11. three counted same-SHA attempts run only after readiness;
12. the deepest exact timer/scheduler/downstream boundary is recorded;
13. Java source/jars remain unchanged;
14. ACK delay and 45-second outer window remain unchanged;
15. production i2pr remains unchanged unless exact reverse TunnelData reaches
    an i2pr-owned failure;
16. focused and full-workspace verification results are separately reported;
17. Plan 201 and Plan 204 are updated together at closure.

## 24. Successor authorization

Only the exact Plan-247 result may authorize later work:

- parser/readiness still cannot become valid -> narrow observer API corrective;
- console buffer saturation -> narrow observer-capacity/mechanism corrective;
- first SchedulerReceived reschedule absent in live rolling evidence ->
  re-qualify the retained Plan-245 boundary before any timer claim;
- reschedule proven but timer schedule absent -> narrow schedule-enqueue
  attribution;
- timer scheduled but not run within bounded tail -> narrow SimpleTimer2
  runtime/executor attribution;
- timer runs under another scheduler -> narrow scheduler-choice/state
  attribution;
- repeated reschedules -> narrow nextSendTime/clock-state attribution;
- send branch reached -> continue the existing Plan-245 downstream chain;
- exact reverse TunnelData reaches i2pr and fails -> production corrective may
  be authorized;
- reverse Streaming direction establishes -> return to Plan 201 publication
  and final Java-family closure.

Do not pre-register any later branch.

## 25. Closure record

`plans/closure/mixed-router-interop/247-status.md` must include:

- implementation SHA;
- exact Java/i2pd authority;
- Plan-246 interpretation amendment;
- parser schema accounting;
- observer readiness evidence;
- poll placement proof relative to SYN and 45-second final snapshot;
- live response/timer rolling evidence;
- buffer pressure evidence;
- three counted evidence directories;
- exact terminal per attempt;
- deepest common boundary;
- no-tuning/no-timing-change proof;
- no-Java-patch proof;
- no-production-change proof;
- focused verification count;
- full workspace all-target serial verification count;
- GitHub status/workflow truth if any;
- Plan-201/204 unblock audit.

## 26. Registration disposition

```text
plan_245 =
  passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary

plan_246 =
  observability-gap-observed-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution

plan_247 =
  registered-ready-m6-java-streaming-plan246-observation-window-parser-corrective

plan_201 =
  blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan247

plan_204 =
  blocked-on-m6-java-second-family-closure-pending-plan247

plan_205 =
  retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan =
  247-m6-java-streaming-plan246-observation-window-parser-corrective

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## 27. Smaller-model handoff

Do not patch Java.

Do not change `initialAckDelay`.

Do not change the 45-second outer response window.

Fix the timer-stats parser first: it has sixteen counted data fields plus one
required source-pin field.

Then move the Plan-247 rolling observation before/inside the SYN response
epoch. Poll fresh RESPONSE_STATS and TIMER_STATS; do not reuse frozen
`p245_post`.

Require the observer configuration to parse and report enabled before the
counted SYN. Peer correlation is allowed to become available only after
accept.

Treat the 500 ms value as the maximum requested delayed-ACK deadline, not as a
guarantee that the ScheduledThreadPoolExecutor executes the callback within
500 ms.

If the send branch is reached, immediately continue the existing Plan-245
downstream chain.

Run the actual full workspace all-target serial test before closure.
