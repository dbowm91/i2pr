# Plan 246 status — M6 Java Streaming delayed-ACK timer enqueue/fire and second-scheduler attribution

Status: **registered-ready-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution**.

Plan of record:
`plans/implementation/mixed-router-interop/246-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution.md`

## Registration basis

Plan 245 closed as:

```text
passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary
```

All three counted Plan-245 attempts established Direction A, bound the response
epoch, and observed the exact-pinned `SchedulerReceived` reschedule branch
without the send branch.

Exact-pinned Java I2P 2.13.0 source review narrows the successor:

- the frozen helper does not set `i2p.streaming.initialAckDelay`;
- `ConnectionOptions.DEFAULT_INITIAL_ACK_DELAY = 500` ms;
- `Connection.setNextSendTime()` clamps future deadlines to no later than
  `now + getSendAckDelay()`;
- `SchedulerReceived.reschedule()` delegates through
  `Connection.scheduleConnectionEvent()`;
- the transition `SimpleTimer2.addEvent(SimpleTimer.TimedEvent, timeoutMs)`
  creates a fresh one-shot wrapper whose `timeReached()` delegates to the
  Connection `ConEvent`;
- `ConEvent.timeReached()` re-enters `Connection.eventOccurred()` and
  therefore `SchedulerChooser`.

Plan 246 owns only attribution of the numeric delayed-ACK deadline, matching
timer enqueue/fire, second scheduler outcome, and transient observation
eviction. It retains the 45-second outer lane unchanged and adds only bounded
short-interval polling.

No Java source/jar patch, ACK-delay change, topology/publication/timing
correction, or production i2pr change is authorized.

## Current authority

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
