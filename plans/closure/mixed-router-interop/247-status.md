# Plan 247 status — M6 Java Streaming Plan-246 observation-window/parser corrective

Status: **registered-ready-m6-java-streaming-plan246-observation-window-parser-corrective**.

Plan of record:
`plans/implementation/mixed-router-interop/247-m6-java-streaming-plan246-observation-window-parser-corrective.md`

## Registration basis

Plan 246 closed at:

```text
observability-gap-observed-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution
```

Post-closure review found three concrete observer defects that must be corrected before any Java timer inference:

1. `p246_parse_timer_stats()` requires `seen == 17` although only sixteen data fields increment `seen`; `java_source_pin` is accepted with `continue`.
2. Plan-246 timer polling begins after the retained Plan-245 post snapshot/classification, which are taken after the frozen 45-second response window.
3. The Plan-246 polling loop derives SchedulerReceived deltas repeatedly from the same frozen `p245_post` snapshot rather than polling fresh response stats.

Exact-pinned Java source remains unchanged and still proves the requested initial delayed-ACK deadline is bounded by the frozen 500 ms default. Java executor semantics do not guarantee that an eligible delayed task executes immediately when the requested delay expires, so Plan 247 keeps requested delay and actual executor latency as separate facts.

Plan 247 is therefore an observation corrective only.

## Current authority

```text
plan_245 = passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary
plan_246 = observability-gap-observed-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution
plan_247 = registered-ready-m6-java-streaming-plan246-observation-window-parser-corrective

plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan247
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan247
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 247-m6-java-streaming-plan246-observation-window-parser-corrective

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

No Java source/jar patch, ACK-delay change, 45-second outer-window change, topology/publication change, or production i2pr change is authorized by registration.
