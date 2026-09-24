# Plan 246 — M6 Java Streaming delayed-ACK timer enqueue/fire and second-scheduler attribution

Status: **registered-ready-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution**

## 1. Objective

Investigate the exact Plan-245 terminal:

```text
P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH
```

without widening the protocol scope or changing Java timing.

Plan 245 proved, on three counted same-SHA hosted attempts, that:

```text
Direction A established
scheduler_delta = 1
scheduler_send_branch_delta = 0
scheduler_reschedule_branch_delta = 1
scheduler_no_unacked_delta = 0
receiver_packet_built_delta = 0
```

Exact-pinned Java I2P 2.13.0 source now constrains the next question much more tightly:

```text
ConnectionPacketHandler
  -> incrementUnackedPacketsReceived()
  -> setNextSendTime(now + ACK delay)
  -> con.eventOccurred()

SchedulerReceived
  -> timeTillSend > 0
  -> reschedule(timeTillSend, con)

SchedulerImpl.reschedule
  -> Connection.scheduleConnectionEvent(ms)

Connection.scheduleConnectionEvent
  -> SimpleTimer2.addEvent(ConEvent, ms)

ConEvent.timeReached
  -> Connection.eventOccurred()
  -> SchedulerChooser.getScheduler(con)
```

The helper does not set `i2p.streaming.initialAckDelay`; therefore the exact-pinned default is 500 ms. `Connection.setNextSendTime()` also clamps any future deadline to no later than `now + getSendAckDelay()`.

Plan 246 must prove what happens to that one-shot delayed-ACK connection event: the numeric deadline, timer enqueue, timer execution, second scheduler outcome, and whether any transient send/build signal is lost by the existing end-of-window snapshot.

This is an attribution pass. It does not authorize Java source changes or production i2pr changes.

## 2. Exact retained authority

Retain unchanged:

- Java I2P 2.13.0 at `9134f808337b401e8e53c73734c81fab04280c9d`;
- i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`;
- Plan-232 route-derived lease-gateway fixture;
- Plans 237–240 downstream response/I2CP/lookup observers;
- Plan-242 non-zero client-pair semantics;
- Plan-243 host qualification;
- Plan-244 same-epoch response correlation;
- Plan-245 direct `ConnectionDataReceiver.buildPacket` observation;
- frozen A/B/C topology;
- Router-B publication target;
- frozen 45-second outer response window;
- fresh RouterContexts per counted attempt;
- no retry-until-C;
- no RNG manipulation;
- no zero-hop fallback;
- no Java source/jar patch;
- no production Rust change before exact reverse TunnelData reaches i2pr.

Plan-245 closure authority:

```text
ee7dc9d7d66adf3face504e80341b8f207a9db3d
```

Counted Plan-245 implementation/evidence authority:

```text
685a59f395685d7cb6fb2ff41181829327a71710
```

## 3. Exact-pinned timer facts to source-lock

### 3.1 ACK deadline source

`ConnectionOptions.cinit()` sets:

```text
setSendAckDelay(getInt(opts, PROP_INITIAL_ACK_DELAY, DEFAULT_INITIAL_ACK_DELAY))
```

and:

```text
DEFAULT_INITIAL_ACK_DELAY = 500
```

The frozen helper options do not set `i2p.streaming.initialAckDelay`.

### 3.2 Deadline clamp

`Connection.setNextSendTime(when)`:

- may move an already-scheduled deadline earlier but not later;
- if scheduled, clamps the result to:
  `context.clock().now() + options.getSendAckDelay()`.

Therefore, absent clock discontinuity, the initial delayed-ACK deadline must not remain more than 500 ms into the future.

### 3.3 Packet-handler ordering

For a new inbound packet:

```text
incrementUnackedPacketsReceived()
setNextSendTime(delay + context.clock().now())
con.eventOccurred()
```

The first `SchedulerReceived` event therefore sees an already-established ACK deadline.

### 3.4 SchedulerReceived ordering

When:

```text
getUnackedPacketsReceived() > 0
timeTillSend = getNextSendTime() - context.clock().now()
timeTillSend > 0
```

the exact branch logs:

```text
received con... time till next send: <N>
```

and then calls:

```text
reschedule(timeTillSend, con)
```

When the deadline is due and `getNextSendTime() > 0`, the exact branch logs:

```text
received con... send a packet
```

then calls:

```text
con.sendAvailable()
con.setNextSendTime(-1)
```

### 3.5 Connection timer wrapper

`SchedulerImpl.reschedule(ms, con)` delegates directly to
`con.scheduleConnectionEvent(ms)`.

`Connection.scheduleConnectionEvent(ms)` calls:

```text
_timer.addEvent(_connectionEvent, ms)
```

where `_connectionEvent` is a `SimpleTimer.TimedEvent`.

### 3.6 SimpleTimer2 transition-wrapper semantics

Exact-pinned `SimpleTimer2.addEvent(SimpleTimer.TimedEvent, timeoutMs)`
creates a new anonymous `SimpleTimer2.TimedEvent` wrapper for every call:

```text
new TimedEvent(this, timeoutMs) {
    timeReached() -> event.timeReached()
    toString() -> event.toString()
}
```

The wrapper schedules itself during construction.

This transition API is one-shot and uncancellable by the caller. It does not use the same `SimpleTimer2.TimedEvent` object's reschedule/coalescing state for repeated `ConEvent` submissions.

### 3.7 Timer lifecycle logs

With `net.i2p.util.SimpleTimer2` DEBUG enabled, exact source emits bounded lifecycle markers including:

```text
Scheduling: <event> timeout = <N> state: ...
Running: <event>
Early execution, Rescheduling for <N> later: <event>
Execution finished in <N>: <event>
```

The event's `toString()` delegates to `Connection.ConEvent.toString()`, which contains the Connection string. The raw line therefore MUST remain scratch-only and MUST NOT be copied into durable evidence.

### 3.8 Callback and scheduler chooser

`Connection.ConEvent.timeReached()` calls `Connection.eventOccurred()`.

`Connection.eventOccurred()` calls:

```text
TaskScheduler sched = _chooser.getScheduler(this)
sched.eventOccurred(this)
```

The exact chooser order is:

```text
SchedulerHardDisconnected
SchedulerPreconnect
SchedulerConnecting
SchedulerReceived
SchedulerConnectedBulk
SchedulerClosing
SchedulerClosed
SchedulerDead
NullScheduler
```

Plan 246 must not assume `SchedulerReceived` still owns the second event merely because it owned the first event.

## 4. Primary hypotheses

Plan 246 must distinguish these hypotheses without changing behavior:

```text
H1 deadline invalid:
  first timeTillSend is outside the source-compatible delayed-ACK bound

H2 timer enqueue absent:
  SchedulerReceived reschedule branch is observed but no matching
  Connection event schedule is observed

H3 timer enqueue succeeds but callback is not observed:
  matching schedule exists but no matching timer run appears

H4 timer runs under changed connection state:
  matching timer run appears but no second SchedulerReceived branch appears

H5 SchedulerReceived runs again and reschedules:
  second timeTillSend remains positive; clock/deadline evolution must be captured

H6 SchedulerReceived reaches send branch:
  continue immediately into Plan-245 direct build and downstream chain

H7 transient evidence eviction:
  timer/send/build evidence appears during short polling but is absent from
  the final 45-second snapshot
```

H7 is an observer-window result, not a stock-Java protocol defect.

## 5. Observation design

### 5.1 Do not alter the 45-second lane

The existing 45-second outer response window remains frozen.

Do not:

- increase or decrease the outer timeout;
- change `initialAckDelay`;
- change RTT;
- add Java sleeps intended to affect protocol behavior;
- manually invoke `sendAvailable()`;
- call `ackImmediately()`;
- inject timers or packets.

### 5.2 Add a short attribution polling window

Add read-only response-stat polling after the first Direction-A / response-epoch entry.

Target observation cadence:

```text
50 ms polling interval
2,000 ms attribution horizon
maximum 40 polls
```

Equivalent bounded cadence is acceptable only if the implementation explains why and preserves at least four observations across the source-max 500 ms ACK deadline.

The polling window is observational only. It MUST NOT shorten or replace the existing 45-second outer window.

Persist only:

- poll index;
- elapsed milliseconds from attribution-window start;
- sanitized counters;
- bounded numeric delays;
- sanitized connection-state booleans/integers.

Do not persist raw Java log lines.

### 5.3 Rolling maxima

The driver must maintain rolling maxima / first-seen timestamps for transient counters.

A signal observed at any poll remains proven even if its raw log later falls out of the LogManager console buffer.

The final 45-second snapshot remains recorded for compatibility, but it cannot erase a previously observed same-epoch signal.

## 6. Exact connection correlation

SimpleTimer2 is shared and may contain unrelated timer activity.

Plan 246 must count only the delayed-ACK `ConEvent` belonging to the accepted Streaming socket for the active response epoch.

Preferred method:

1. after `accept()` returns, use the public `I2PSocket` peer-Destination surface to derive the expected remote peer identifier in memory;
2. scan the bounded public LogManager buffer;
3. transiently match only `SimpleTimer2` lines for:
   `event on [Connection ... from <expected-peer> ...]`;
4. immediately reduce matching lines to sanitized numeric/count fields;
5. never write the peer string or raw line to durable evidence.

If the exact public socket surface cannot provide stable correlation, stop with:

```text
P246-OBSERVABILITY-GAP-CONNECTION-TIMER-CORRELATION
```

Do not weaken correlation to "any connection event".

## 7. New timer/scheduler snapshot

Prefer a dedicated control command such as:

```text
REPORT_TIMER_STATS <socket-id>
```

or a backwards-compatible extension to `REPORT_RESPONSE_STATS` with an explicit socket-id correlation argument.

At minimum expose sanitized fields:

```text
scheduler_reschedule_count
scheduler_send_count
scheduler_no_unacked_count

first_reschedule_delay_ms
latest_reschedule_delay_ms
min_reschedule_delay_ms
max_reschedule_delay_ms

connection_timer_schedule_count
connection_timer_run_count
connection_timer_early_reschedule_count
connection_timer_finished_count

connection_timer_first_schedule_timeout_ms
connection_timer_latest_schedule_timeout_ms
connection_timer_first_run_elapsed_ms

receiver_packet_built_count
send_message_event_count
send_failure_count
send_exception_count

socket_closed
context_clock_minus_system_ms
```

Where stable parsing is possible from the transient matched Connection string, also emit only sanitized state facts:

```text
unacked_in
unacked_out
sent_count
ack_through
reset_sent
reset_received
close_sent
close_received
```

Do not expose the peer Destination/b32 in the response.

## 8. Deadline validation

The first exact `SchedulerReceived` reschedule delay must satisfy:

```text
0 < first_reschedule_delay_ms <= active_send_ack_delay_ms
```

For the frozen helper, source/default configuration should prove:

```text
active_send_ack_delay_ms = 500
```

If the helper can read the active public socket options safely, record that value. Otherwise source-lock the frozen helper property set plus default 500 ms.

Canonical terminal:

```text
P246-A-NEXT-SEND-DEADLINE-OUT-OF-BOUNDS
```

Include the sanitized observed delay and allowed bound.

A value within bound proceeds to timer attribution.

## 9. Timer enqueue attribution

If the first SchedulerReceived reschedule branch is observed, require a matching Connection timer schedule within the same epoch.

If absent:

```text
P246-B-CONNECTION-EVENT-NOT-SCHEDULED
```

Before using this terminal, prove:

- SimpleTimer2 DEBUG is enabled before the inbound SYN;
- the buffer/polling window is active;
- exact connection correlation works;
- no matching schedule was observed in any poll.

Do not infer a stock-Java timer bug merely from an end-of-window zero.

## 10. Timer execution attribution

If a matching schedule is observed, require a matching timer run.

Classify:

```text
P246-B-CONNECTION-EVENT-SCHEDULED-NOT-RUN
```

only when:

- schedule count > 0;
- run count == 0 across the full two-second attribution horizon;
- no early-execution/reschedule lifecycle proves equivalent progress;
- no observer-capacity/correlation failure occurred.

If the timer runs, continue.

If `Early execution, Rescheduling...` is observed, record the sanitized reschedule duration and continue polling. That is not a failure by itself.

## 11. Second scheduler attribution

After a matching timer run, inspect same-epoch scheduler evidence.

### 11.1 SchedulerReceived send branch

If `scheduler_send_count` increases after the timer run:

```text
P246-D-SCHEDULER-SEND-BRANCH-REACHED
```

Immediately resume Plan 245 §8.

### 11.2 SchedulerReceived reschedules again

If the reschedule counter increases again after the timer run:

```text
P246-C-SCHEDULER-RESCHEDULED-AGAIN
```

Record:

- first and second reschedule delays;
- context-clock-minus-system-time at the nearest polls;
- whether the second delay is inside 0..500 ms;
- whether subsequent matching timer schedule/run occurs.

Do not stop at the first repeat if another bounded timer event is already proven queued inside the two-second attribution horizon. Continue until either the send branch is reached or the bounded horizon ends.

If repeated reschedules consume the full bounded horizon without send:

```text
P246-C-REPEATED-RESCHEDULE-WITHOUT-SEND
```

### 11.3 No-unacked state

If the second scheduler observation is the exact no-unacked warning:

```text
P246-C-SCHEDULER-NO-UNACKED-ON-SECOND-EVENT
```

This authorizes only a narrow inbound-ACK-state successor.

### 11.4 Timer ran but SchedulerReceived did not reappear

If a matching timer run occurs but no second SchedulerReceived send/reschedule/no-unacked signal is observed:

```text
P246-C-CONNECTION-EVENT-RAN-SCHEDULER-CHANGED
```

Record the sanitized public/socket and Connection-string state facts available at that timer run.

This terminal means the chooser selected another state or an observability gap remains. It does NOT by itself authorize a Java patch.

If state facts are insufficient to distinguish chooser candidates, the successor must be a narrow scheduler-choice attribution plan.

## 12. Observer-window eviction classification

If any short-poll signal is observed but its final 45-second snapshot counter later returns to a lower value because the bounded log line aged out, record:

```text
P246-O-END-OF-WINDOW-SNAPSHOT-EVICTION
```

alongside the deeper behavioral terminal reached through rolling-max evidence.

This is an observer correction only.

The short-poll rolling maximum becomes authoritative for Plan-246 timer/scheduler lifecycle facts; the retained final snapshot remains historical/compatibility evidence.

## 13. Resume the reverse path when send branch is reached

Once `P246-D-SCHEDULER-SEND-BRANCH-REACHED` is proven, do not create another timer plan.

Resume the existing Plan-245 chain in the same response epoch:

```text
sendAvailable
-> MessageOutputStream.flushAvailable
-> ConnectionDataReceiver.writeData
-> buildPacket
-> Connection.sendPacket
-> PacketQueue / I2PSession.sendMessage
-> Router-A I2CP admission
-> exact Streaming lookup
-> Router-B query/reply
-> helper client-subDB install
-> OCMOSJ/tunnel dispatch
-> exact reverse i2pr TunnelData
-> recovery
-> Garlic
-> Streaming
```

Reuse the Plan-245 canonical downstream ordering and typed terminals.

The deepest authoritative same-epoch boundary governs closure.

## 14. Clock-skew observation

`nextSendTime` calculations use `I2PAppContext.clock().now()`, while the ScheduledThreadPoolExecutor delay is ultimately based on system monotonic/wall scheduling from the relative timeout passed to SimpleTimer2.

Record, through public APIs only:

```text
context_clock_minus_system_ms =
  I2PAppContext.getGlobalContext().clock().now()
  - System.currentTimeMillis()
```

at each timer poll or at least:

- attribution start;
- first reschedule;
- first timer run;
- second scheduler signal;
- attribution end.

A stable non-zero offset is diagnostic only.

A material offset discontinuity correlated with repeated positive `timeTillSend` may authorize a narrow clock-state successor.

Do not adjust clocks.

## 15. Expected implementation surfaces

Prefer only:

```text
tests/integration/m6-interop/java/ReferenceStreamingService.java
crates/i2pr-daemon/tests/java_tunnel_external.rs
scripts/interop/check-m6-java-response-source-lock.sh
scripts/check-m6-mixed-router-acceptance-evidence.sh
tests/integration/m6-interop/run-java.sh
```

No production `src/` file may change.

Helper changes remain observation-only:

- enable bounded `SimpleTimer2` DEBUG;
- parse bounded public logs;
- correlate timer events to the accepted public socket;
- return sanitized counts/delays;
- no change to socket reads/writes, SessionConfig, ACK behavior, timer behavior, or connection state.

## 16. Source-lock additions

Extend the exact-pinned source lock with at least:

```text
ConnectionOptions:
  PROP_INITIAL_ACK_DELAY
  DEFAULT_INITIAL_ACK_DELAY = 500
  cinit -> setSendAckDelay(...)

Connection.setNextSendTime:
  earlier-only behavior
  now + getSendAckDelay() clamp

ConnectionPacketHandler:
  incrementUnackedPacketsReceived()
  setNextSendTime(...)
  con.eventOccurred() ordering

SchedulerReceived:
  timeTillSend calculation
  reschedule branch
  send branch

SchedulerImpl:
  reschedule -> scheduleConnectionEvent

Connection:
  scheduleConnectionEvent -> _timer.addEvent(_connectionEvent, ms)
  ConEvent.timeReached -> eventOccurred
  eventOccurred -> _chooser.getScheduler(this)

SchedulerChooser:
  exact scheduler precedence

SimpleTimer2:
  addEvent(SimpleTimer.TimedEvent, timeoutMs)
  new one-shot wrapper
  wrapper timeReached delegates to event.timeReached
  Scheduling / Running / Early execution / Execution finished lifecycle logs
```

The checker must reject claims that transition-wrapper `addEvent` deduplicates the same `ConEvent` object.

## 17. Static checker requirements

Reject:

- any change to Java I2P source/jars;
- any production Rust change;
- any modification of `initialAckDelay`;
- outer timeout changes;
- manual `sendAvailable` / `ackImmediately` calls;
- packet/timer injection;
- raw Connection/timer log lines in durable evidence;
- peer Destination/b32 in durable Plan-246 evidence;
- generic SimpleTimer2 counts not correlated to the active socket;
- classification from final snapshot alone when rolling polling observed deeper evidence;
- treating one reschedule as a 45-second delay;
- treating timer run as proof SchedulerReceived was selected;
- treating chooser change as a Java defect without state attribution;
- fail-open required checks.

## 18. Focused tests

Add at least equivalents of:

```text
p246_default_ack_delay_is_500_on_frozen_helper
p246_next_send_time_is_clamped_by_ack_delay
p246_packet_handler_sets_deadline_before_event
p246_received_reschedule_calls_connection_timer
p246_transition_add_event_uses_fresh_wrapper
p246_timer_wrapper_delegates_to_connection_event
p246_connection_event_reenters_scheduler_chooser
p246_scheduler_precedence_is_source_locked
p246_polling_preserves_transient_signal
p246_final_snapshot_cannot_erase_rolling_max
p246_timer_logs_require_exact_socket_correlation
p246_peer_identity_not_persisted
p246_deadline_out_of_bounds_precedes_timer_terminal
p246_schedule_precedes_run
p246_timer_run_precedes_second_scheduler_terminal
p246_second_reschedule_records_second_delay
p246_no_unacked_second_event_is_typed
p246_scheduler_changed_is_not_java_defect
p246_send_branch_resumes_plan245_chain
p246_i2pr_defect_requires_exact_reverse_tunneldata
p246_no_timing_change
p246_no_java_patch
p246_no_production_change
```

Retain the focused P237–P245 floor.

## 19. Counted execution discipline

After implementation is committed:

1. run the unchanged Plan-243 host qualification against the exact implementation SHA;
2. run the full source-lock/static/focused verification floor;
3. execute three counted hosted attempts on one SHA;
4. fresh A/B/C RouterContexts per attempt;
5. unique evidence directories;
6. no between-attempt tuning;
7. no retry-until-C;
8. no timeout/ACK-delay changes;
9. no Java source/jar patch;
10. no production i2pr change unless the existing exact-reverse-TunnelData gate is reached.

Command remains:

```bash
I2PR_M6_JAVA_DRIVER=streaming bash tests/integration/m6-interop/run-java.sh
```

## 20. Per-attempt evidence

Record:

```text
Direction-A prerequisite
response-epoch binding
active ACK-delay authority
first/second/etc reschedule delay ms
poll interval and poll count
timer schedule count + first timeout
timer run count + first run elapsed
early-reschedule count
timer-finished count
rolling scheduler send/reschedule/no-unacked maxima
rolling direct-build/sendMessage maxima
final-snapshot values
observer-eviction flag
clock-offset samples
sanitized socket/connection state when scheduler changes
deepest P246 terminal
```

Raw logs remain scratch-only.

## 21. Verification floor

Run:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
focused P237-P246 tests
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>
bash scripts/interop/check-p243-host-qualified.sh --expected-sha <implementation-sha>
javac <all staged Java helpers against exact-pinned jars>
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
cargo test --locked --workspace --all-targets -- --test-threads=1
```

## 22. Acceptance criteria

Plan 246 closes correctly only when:

1. the active delayed-ACK bound is proven for the frozen helper;
2. the first exact reschedule delay is captured numerically;
3. timer schedule/run observations are exact-socket correlated;
4. short polling prevents transient evidence from being lost by the final snapshot;
5. timer enqueue vs timer execution vs scheduler transition are distinguished;
6. a second SchedulerReceived invocation, if present, is ordered after timer execution;
7. repeated reschedules record their numeric delays and clock-offset context;
8. send-branch success immediately resumes the Plan-245 downstream chain;
9. three counted same-SHA hosted attempts run without tuning;
10. Java source/jars remain unchanged;
11. production i2pr remains unchanged unless exact reverse TunnelData reaches an i2pr-owned failure;
12. Plan 201 and Plan 204 are updated together at closure.

## 23. Successor authorization

Only the exact Plan-246 result may authorize a successor:

- deadline > active ACK-delay bound -> narrow deadline/clock-origin attribution;
- SchedulerReceived reschedules but matching timer schedule is absent -> narrow transition-timer enqueue/observer corrective;
- matching timer scheduled but never runs -> narrow SimpleTimer2/runtime attribution;
- timer runs but SchedulerReceived does not reappear -> narrow scheduler-choice/state attribution;
- second SchedulerReceived reports no unacked packets -> narrow inbound ACK-state attribution;
- repeated bounded reschedules correlate with clock discontinuity -> narrow clock-state attribution;
- repeated bounded reschedules with stable clock -> narrow nextSendTime writer/state attribution;
- send branch reached and write/build fails -> return to the exact Plan-245 downstream boundary;
- expected reverse TunnelData reaches i2pr then fails -> exact production i2pr corrective may be authorized;
- reverse direction establishes -> return to Plan 201 publication/final Java-family closure.

Do not pre-register later branches.

## 24. Closure record

`plans/closure/mixed-router-interop/246-status.md` must record:

- implementation SHA;
- exact pins;
- source-lock checksum/rows;
- active ACK-delay authority;
- poll cadence/horizon;
- timer logger configuration;
- exact-socket correlation proof;
- three counted evidence directories;
- first and subsequent reschedule delays per attempt;
- timer schedule/run/early-reschedule/finish counts;
- rolling maxima vs final snapshots;
- observer-eviction result;
- clock-offset samples;
- sanitized state if scheduler changed;
- downstream Plan-245 facts if send branch reached;
- exact terminal per attempt;
- no-tuning/no-timing-change proof;
- no-Java-patch proof;
- no-production-change proof;
- verification floor;
- Plan-201/204 unblock audit.

## 25. Registration disposition

```text
plan_245 = passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary

plan_246 = registered-ready-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution

plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan246
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan246
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 246-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## 26. Smaller-model handoff

Do not change the ACK delay or the 45-second timeout.

The first Plan-245 reschedule should be at most 500 ms on the frozen helper.

Add exact-socket-correlated observation of the SimpleTimer2 wrapper and poll the response counters for two seconds at about 50 ms cadence.

Prove:

```text
first SchedulerReceived reschedule
-> timer schedule
-> timer run
-> second scheduler outcome
```

If the second SchedulerReceived event reaches `send a packet`, immediately continue the existing Plan-245 build/send/Router-A/lookup/reverse-tunnel chain.

If the timer runs under another scheduler, record sanitized connection state and stop at that boundary.

Do not patch Java and do not touch production i2pr before exact reverse TunnelData reaches i2pr.
