# Current dependency amendment — Plan 247 registered; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is
not reopened or downgraded.

Plan 246 closed the Java Streaming timer pass at an observation gap.
Post-closure review found a timer-stats parser schema-count defect, timer
polling placed after the retained 45-second response post-snapshot, and
SchedulerReceived polling derived from frozen Plan-245 state.

Plan 247 is registered-ready to correct only those observer defects and
re-run the delayed-ACK attribution with live response/timer polling inside
the response epoch. It does not change Java behavior, ACK delay, topology,
publication, the 45-second outer lane, or production i2pr.

Plan 204 remains convergence-only and blocked on independent M6 Java
second-family closure.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan247
plan_246 = observability-gap-observed-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution
plan_247 = registered-ready-m6-java-streaming-plan246-observation-window-parser-corrective
milestone10_final_acceptance = closed
next_executable_plan = 247-m6-java-streaming-plan246-observation-window-parser-corrective
```

# Current dependency amendment — Plan 246 closed at observation-gap; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed
and is not reopened or downgraded.

Plan 246 is closed as
`observability-gap-observed-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution`
(see `plans/closure/mixed-router-interop/246-status.md`): on three
counted same-SHA hosted executions of the frozen Plan-242 Streaming
lane, Direction A was established 3/3, but the Plan-245 baseline
gate was satisfied on 0/3 counted attempts and the Plan-246
contradiction guard fired on every attempt; all three counted
terminals were `P246-OBSERVABILITY-GAP`. No Java defect proven; no
production i2pr change; no ACK-delay override; 45-second lane
frozen; polling cadence 50 ms × 40 = 2 s attribution horizon
(observational only).

Plan 204 remains convergence-only and blocked on independent M6 Java
second-family closure (now pending the Plan-246 successor
observation-gap corrective; no double unblock).

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan246-successor-observation-gap-corrective
plan_245 = passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary
plan_246 = observability-gap-observed-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution
milestone10_final_acceptance = closed
next_executable_plan = none-pending-plan246-successor-observation-gap-corrective
```

# Current dependency amendment — Plan 246 registered; M10 product authority remains closed

# Current dependency amendment — Plan 245 closed at scheduler-rescheduled; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is not reopened or downgraded.

Plan 245 closed the stock-response construction-signal attribution corrective on `c866967f114ce15012d3256e38eebdd1f7e9f6f5` (with the Plan-237-stats extract committed as `685a59f` and counted attempts run on that head; see `plans/closure/mixed-router-interop/245-status.md`). Three counted same-SHA executions of the frozen Plan-242 Streaming lane established Direction A 3/3 with fully bound response epochs, and the Stage A.0 classifier proved the scheduler reschedule-only branch on every attempt with identical deltas (`scheduler_delta=1`, `scheduler_send_branch_delta=0`, `scheduler_reschedule_branch_delta=1`, `receiver_packet_built_delta=0`, `connection_resend_timer_delta=0`, failures zero). The Plan-244 `Resend in` retransmit-timer proxy was **not** a false negative on this lane — construction genuinely did not occur because the scheduler rescheduled before the response window opened. Plan 245 §17 authorizes a new narrow-timer/state-attribution plan-of-record to own the bounded investigation into why `con.getNextSendTime() - _context.clock().now() > 0` holds for the frozen 45-second response window on every counted attempt.

Plan 204 remains convergence-only and blocked on independent M6 Java second-family closure (now pending the Plan-245 successor-narrow-timer/state-attribution + publication / final-closure axis; no double unblock).

plan_204 = blocked-on-m6-java-second-family-closure-pending-plan245-successor-narrow-timer-state-attribution
plan_244 = passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary
plan_245 = passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary
milestone10_final_acceptance = closed
next_executable_plan = none-pending-successor-narrow-timer-state-attribution

# Current dependency amendment — Plan 245 registered; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is not reopened or downgraded.

Plan 244 closed the Java Streaming reverse-direction continuous response attribution at a repeatable response-construction boundary. Exact-pinned stock-Java review shows the retained retransmit-timer construction proxy is conditional, so Plan 245 is registered-ready to observe `ConnectionDataReceiver.buildPacket` directly and determine whether the Plan-244 boundary is an observer false negative or a genuine stock-response suppression state.

Plan 204 remains convergence-only and blocked on independent M6 Java second-family closure. Plan 245 pre-authorizes no production-i2pr, publication, topology, timing, or Java-source corrective.

plan_204 = blocked-on-m6-java-second-family-closure-pending-plan245
plan_244 = passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary
plan_245 = registered-ready-m6-java-streaming-stock-response-construction-signal-attribution-corrective
milestone10_final_acceptance = closed
next_executable_plan = 245-m6-java-streaming-stock-response-construction-signal-attribution-corrective

# Current dependency amendment — Plan 244 closed; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is
not reopened or downgraded.

Plan 244 closed with Direction A (i2pr → Java Streaming) established on
3/3 counted same-SHA hosted executions at
`154e92d8436dbad0b020a0b19d848d2d227a22af`, every response epoch fully
bound, and the Java → i2pr Streaming response path bounded repeatably at
`P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` (see
`plans/closure/mixed-router-interop/244-status.md`). No production i2pr
corrective is authorized before exact expected reverse TunnelData
reaches i2pr.

Plan 204 remains convergence-only and blocked on independent M6 Java
second-family closure (now pending the §20 stock-response corrective +
publication / final-closure axis; no double unblock).

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-successor-after-plan244
plan_243 = passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established
plan_244 = passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary
milestone10_final_acceptance = closed
next_executable_plan = none-pending-stock-response-corrective-plan-of-record
```

# Current dependency amendment — Plan 244 registered; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is
not reopened or downgraded.

Plan 243 closed with Direction A (i2pr → Java Streaming) established on hosted
execution. Plan 244 is registered-ready to attribute the remaining Java → i2pr
Streaming response path by correlating the retained Plan-237/238/239/240
observers in one response epoch.

Plan 204 remains convergence-only and blocked on independent M6 Java
second-family closure. Plan 244 does not reopen M10 product authority and does
not pre-authorize publication or production-i2pr changes.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan244
plan_243 = passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established
plan_244 = registered-ready-m6-java-streaming-reverse-direction-continuous-response-attribution
milestone10_final_acceptance = closed
next_executable_plan = 244-m6-java-streaming-reverse-direction-continuous-response-attribution
```

# Current dependency amendment — Plan 243 closed; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is not reopened or downgraded.

Plan 243 closed as
`passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established`
(see `plans/closure/mixed-router-interop/243-status.md`): Direction A
established on 2/3 counted same-SHA attempts at
`f359baba57fc7d952ab7f5a5367d34671c590b18`, one earlier
stock-Java `I2PSession.connect()` handshake ceiling stop, reverse
direction bounded at the retained Plan-236 response-emission
gap. No production i2pr corrective is authorized before the
reverse direction reaches i2pr.

Plan 204 remains convergence-only and blocked on independent M6
Java second-family closure (now pending the streaming reverse
direction + publication / final-closure axis; no double unblock).

plan_204 = blocked-on-m6-java-second-family-closure-pending-plan243-reverse-direction-and-publication-closure
plan_242 = passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate
plan_243 = passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established
milestone10_final_acceptance = closed
next_executable_plan = none-pending-m6-java-streaming-reverse-direction-and-publication-corrective

# Current dependency amendment — Plan 243 registered; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is not reopened or downgraded.

Plan 242 closed the corrected stock selector/bootstrap/pair-gate harness semantics, but its closure host lacked the Java reference cache plus i2pr daemon pair, so live counted external attempts remain outstanding.

Plan 243 is registered-ready to qualify a capable execution host and run the frozen Plan-242 Streaming lane three times on one SHA with no tuning. It pre-authorizes no topology, selector, publication, timing, Java-source, or production-i2pr corrective.

Plan 204 remains convergence-only and blocked on independent M6 Java second-family closure.

plan_204 = blocked-on-m6-java-second-family-closure-pending-plan243
plan_242 = passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate
plan_243 = registered-ready-m6-java-streaming-hosted-stock-client-build-qualification
milestone10_final_acceptance = closed
next_executable_plan = 243-m6-java-streaming-hosted-stock-client-build-qualification

# Current dependency amendment — Plan 242 closed; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is not reopened or downgraded.

Plan 241 removed the forced zero-hop Streaming fixture but stopped at the client-build gate because the harness required exact-via-C tunnels. Exact-pinned Java source now proves explicitPeers is sampled only one build in four; ordinary client tunnel builds may legitimately choose another stock fast peer.

Plan 242 closes the corrected harness semantics on the implementation head: the §5 stock-candidate-population gate replaces the Plan-241 §6 C-profile gate, the §7 non-zero-hop pair gate replaces the Plan-241 §7 exact-via-C gate, the §6 extended `P242-CLIENT-TUNNELS` row records role/path facts and `contains_c` as diagnostic only, source lock grew 35→38 with TunnelPeerSelector / ClientPeerSelector needles, and the routine + focused + full serial workspace floors all pass. Live counted external attempts remain unexecuted on this host (no Java cache + i2pr daemon pair available); the §20 stock-client-build successor is the next executable lane.

Plan 204 remains convergence-only and blocked on independent M6 Java second-family closure pending the §20 stock-client-build successor.

plan_204 = blocked-on-m6-java-second-family-closure-pending-stock-client-build-successor-after-plan242
plan_242 = passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate
plan_241 = passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary
milestone10_final_acceptance = closed
next_executable_plan = none-pending-stock-client-build-attribution-successor-plan-of-record

# Current dependency amendment — Plan 241 closed; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is
not reopened or downgraded.

Plan 241 closed the Streaming one-hop fixture corrective at the exact A/B
build-stage boundary (see
`plans/closure/mixed-router-interop/241-status.md`): the forced zero-hop
fixture is removed, the lane stops at typed bootstrap/pair terminals, and
the §20 successor (bootstrap repeatability / stock client-build
corrective) requires its own plan-of-record.

Plan 204 remains convergence-only and blocked on independent M6 Java
second-family closure.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-successor-after-plan241
plan_241 = passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary
milestone10_final_acceptance = closed
next_executable_plan = none-pending-section-20-successor-plan-of-record
```

# Current dependency amendment — Plan 241 registered; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is
not reopened or downgraded.

Plan 240 proved the exact Streaming response lookup stops in stock Java before
query dispatch because the controlled Streaming helper forces a zero-hop client
pool and the client-facade unknown-RI guard rejects Router B.

Plan 241 is registered-ready to correct only that test fixture using the
already-proven one-hop-via-C public SessionConfig, require genuine non-zero
client tunnels, and resume the exact lookup/reply/OCMOSJ chain.

Plan 204 remains convergence-only and blocked on independent M6 Java
second-family closure.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan241
plan_241 = registered-ready-m6-java-streaming-one-hop-client-tunnel-fixture-corrective
milestone10_final_acceptance = closed
next_executable_plan = 241-m6-java-streaming-one-hop-client-tunnel-fixture-corrective
```

# Current dependency amendment — Plan 240 closed; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is
not reopened or downgraded.

Plan 240 closed the Streaming lookup-failure attribution at
`P240-C-B-ZERO-HOP-UNKNOWN-RI` (see
`plans/closure/mixed-router-interop/240-status.md`): exact streaming ISJ
correlated, Router B eligible and in `toTry`, zero-hop-unknown sendQuery
skip proven on `37025f9` x3, B never queried.

Plan 204 remains convergence-only and blocked on independent M6 Java
second-family closure, now pending the Plan-240-authorized
zero-hop-tunnel/RI-availability successor (no plan-of-record yet).

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-zero-hop-successor-after-plan240
plan_240 = passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary
milestone10_final_acceptance = closed
next_executable_plan = none-pending-zero-hop-successor-plan-of-record
```

# Current dependency amendment — Plan 240 registered; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed and is
not reopened or downgraded.

Plan 239 closed the current Streaming branch at
`P239-D-TARGET-LEASESET-LOOKUP-FAILED`: Router A admits the Java response
client messages, but the helper client NetDB has no local target LS and the
remote lookup fails before OCMOSJ can select a target lease or dispatch.

Plan 240 is registered-ready to reuse the existing Plan-225/226 exact
target-job lookup trace in the Streaming epoch and identify the earliest
Router-B selection/query/reply/install boundary.

Plan 204 remains convergence-only and blocked on independent M6 Java
second-family closure.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan240
plan_240 = registered-ready-m6-java-streaming-target-leaseset-lookup-failure-attribution
milestone10_final_acceptance = closed
next_executable_plan = 240-m6-java-streaming-target-leaseset-lookup-failure-attribution
```

# Current dependency amendment — Plan 239 tightened; M10 product authority remains closed

M10 product/application authority through Plans 213–215 remains closed.

Plan 238 proved Router-A I2CP admission but zero dispatch progress for the
Streaming response epoch. Plan 239 now owns the exact post-admission OCMOSJ
attribution: local-vs-remote target LeaseSet state, lease/tunnel preparation,
branch-specific `client.dispatchNoTunnels`, and dispatchOutbound.

Plan 204 remains convergence-only and blocked on independent M6 Java
second-family closure.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan239
plan_239 = registered-ready-m6-java-streaming-router-a-dispatch-observer
milestone10_final_acceptance = closed
next_executable_plan = 239-m6-java-streaming-router-a-dispatch-observer
```

# Current dependency amendment — Plan 238 closed at dispatch boundary; Plan 239 registered; M10 product authority remains closed

Plan 204 remains convergence-only. M10 product/application authority through
Plans 213–215 remains closed and is not reopened or downgraded.

Plan 238 closed at `P237-D-CLIENT-MESSAGE-NOT-ADMITTED` on `08df6ea`
(three identical counted executions; distribute delta exactly 3 with
dispatch deltas 0 and rates known; full serial workspace floor green:
2710 passed, 18 ignored, 103 suites). Router-A dispatch on the
streaming response path remains unobserved, so M6 Java second-family
closure is still outstanding. Plan 239 is registered-ready for the
narrow Router-A dispatch observer.

Plan 204 therefore remains blocked until the independent M6 Java
second-family closure gates are satisfied (now pending Plan 239).

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan239
plan_238 = passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary
plan_239 = registered-ready-m6-java-streaming-router-a-dispatch-observer
milestone10_final_acceptance = closed
next_executable_plan = 239-m6-java-streaming-router-a-dispatch-observer
```

# Current dependency amendment — Plan 237 closed at Router-A boundary; Plan 238 registered; M10 product authority remains closed

Plan 204 remains convergence-only. M10 product/application authority through
Plans 213–215 remains closed and is not reopened or downgraded.

Plan 237 closed at `P237-D-ROUTER-I2CP-NOT-OBSERVED` on `a1065d1` (three
identical counted executions; scheduler/construction/sendMessage proven
with deltas 2/1/3, zero failures; full serial workspace floor green
including `sam_stream_final_acceptance` 10/10). Router-A admission on the
streaming response path remains unobserved, so M6 Java second-family
closure is still outstanding. Plan 238 is registered-ready for the narrow
Router-A admission observer.

Plan 204 therefore remains blocked until the independent M6 Java
second-family closure gates are satisfied (now pending Plan 238).

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan238
plan_237 = passed-m6-java-streaming-stock-response-observability-corrective-with-router-i2cp-not-observed-boundary
plan_238 = registered-ready-m6-java-streaming-router-a-admission-observer
milestone10_final_acceptance = closed
next_executable_plan = 238-m6-java-streaming-router-a-admission-observer
```

# Current dependency amendment — Plan 237 registered; M10 product authority remains closed

Plan 204 remains convergence-only. M10 product/application authority through
Plans 213–215 remains closed and is not reopened or downgraded.

Plan 236 closed at the Java response-emission observability gap. Plan 237 is
registered-ready to replace the literal response placeholders with measured
stock-Java scheduler/ACK/sendMessage evidence and continue only from the first
proven stage.

Plan 204 remains blocked until independent M6 Java second-family closure.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan237
plan_236 = passed-m6-java-streaming-response-emission-observability-gap
plan_237 = registered-ready-m6-java-streaming-stock-response-observability-corrective
milestone10_final_acceptance = closed
next_executable_plan = 237-m6-java-streaming-stock-response-observability-corrective
```

# Current dependency amendment — Plan 236 closed at response-emission observability gap; M10 product authority remains closed

Plan 204 remains convergence-only. M10 product/application authority through
Plans 213–215 remains closed and is not reopened or downgraded.

Plan 235 closed at the Java socket-surface-ready / no-i2pr-inbound boundary.
Plan 236 closed at
`P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`: the exact stock-Java
response path was source-locked, but response emission remained unobservable
on two same-SHA attempts. Router-A and i2pr-owned stages were not reached.

Plan 204 therefore remains blocked until the independent M6 Java second-family
closure gates are satisfied.

```text
plan_204 = blocked-on-m6-java-second-family-closure-after-plan236
plan_235 = passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound
plan_236 = passed-m6-java-streaming-response-emission-observability-gap
milestone10_final_acceptance = closed
next_executable_plan = none-registered
```

# Current dependency amendment — Plan 235 closed; M6 dependency remains unresolved

Plan 204 remains convergence-only. M10 product/application authority through
Plans 213–215 remains closed and is not reopened or downgraded.

Plan 234 closed at
`passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary`.
Plan 235 closed at
`passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound`:
Java's public socket surface was ready and i2pr outbound admission succeeded,
but no inbound TunnelData arrived. Plan 204 therefore remains blocked until
the independent M6 Java second-family closure gates are satisfied.

```text
plan_204 = blocked-on-m6-java-second-family-closure-after-plan235
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
plan_235 = passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound
milestone10_final_acceptance = closed
next_executable_plan = none
```

# Current dependency amendment — Plan 234 registered; M10 product authority remains closed

Plan 204 remains convergence-only. M10 product/application authority through
Plans 213–215 remains closed and is not reopened or downgraded by M6 work.

Plan 232 retained the raw-Destination Java-family pass and exposed the
Streaming SYN-ACK boundary. Plan 233 was never executed and is superseded by
Plan 234 because its family-pass outcome did not reconcile the still-required
Plan-200/201 client-LS2 lifecycle rows in the fail-closed Java harness.

Plan 234 now owns both the narrow Streaming continuation and the final-closure
authority reconciliation. Plan 204 remains blocked until Plan 234 either closes
Java-family M6 on one exact green head or records the next exact remaining
boundary.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan234
plan_232 = passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
plan_233 = superseded-before-execution-by-plan234-final-closure-authority-corrective
plan_234 = registered-ready-m6-java-streaming-syn-ack-and-client-ls2-final-closure-authority-corrective
milestone10_final_acceptance = closed
next_executable_plan = 234-m6-java-streaming-syn-ack-and-client-ls2-final-closure-authority-corrective
```

# Current dependency amendment — Plan 232 closed at Outcome B; convergence pending Plan 233; M10 product authority remains closed

Plan 204 remains convergence-only. M10 product/application authority through
Plans 213–215 remains closed and is not reopened or downgraded by this M6 work.

Plan 232 closed at Outcome B
(`passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary`,
see `plans/closure/mixed-router-interop/232-status.md`): the M6-only
route-derived lease correction landed, raw-Destination reverse delivery
passes digest-matched, and the Streaming continuation stops at the new
exact SYN-ACK-never-established boundary (two consecutive runs). Plan 233
is the registered narrow Streaming SYN-ACK corrective. Convergence still
waits on independent M6 Java second-family closure.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan233
plan_232 = passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
plan_233 = registered-ready-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix
milestone10_final_acceptance = closed
next_executable_plan = 233-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix
```

# Current dependency amendment — Plan 231 closed with boundary; M10 product authority remains closed

Plan 204 remains convergence-only. M10 product/application authority through
Plans 213–215 remains closed and is not reopened or downgraded by the M6 Java
work. Plan 231 closed as
`passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary`
(see `plans/closure/mixed-router-interop/231-status.md`): the post-`ACCEPTED`
reverse path is attributed end to end (A enqueue proven, C OBEP processing
proven, exact target IBGW on Router B proven absent, i2pr wire counters
honestly zero), root-caused to the test-driver lease fixture (advertised
gateway Router B vs actual inbound gateway Router A), with a narrow
lease-gateway fixture corrective recommended and no production change
authorized. Convergence still waits on that corrective plus actual reverse
delivery; no successor is registered here.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-lease-gateway-corrective-after-plan231
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
milestone10_final_acceptance = closed
next_executable_plan = none (narrow M6 lease-gateway fixture corrective recommended)
```

# Current dependency amendment — Plan 231 registered; M10 product authority remains closed

Plan 204 remains convergence-only. M10 product/application authority through
Plans 213–215 remains closed and is not reopened or downgraded by the M6 Java
work. Plan 231 is now the registered-ready attribution for the remaining
Java→i2pr reverse-delivery boundary after Plan 230: it traces the exact
post-`ACCEPTED` path from Java A's outbound gateway through Router C and the
selected target inbound gateway to i2pr's exact TunnelData/recovery/Garlic/
Destination stages.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan231
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = registered-ready-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective
milestone10_final_acceptance = closed
next_executable_plan = 231-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective
```

# Current dependency amendment — Plan 230 closed; convergence still blocked on the reverse-delivery boundary

Plan 204 remains convergence-only. M10 product/application authority through Plans 213–215 is already closed and is not reopened by this work. Plan 230 closed as `passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary` (see `plans/closure/mixed-router-interop/230-status.md`): predicate proven, stock fixture corrected, natural bootstrap proven, forward destination delivery digest-matched — with the reverse Java→i2pr payload reproducing the retained Plan-218 signature. The remaining dependency is therefore narrowed from generic Plan-230 pendency to that exact reverse-delivery boundary; no reverse-delivery corrective is registered yet.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-reverse-delivery-corrective
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
milestone10_final_acceptance = closed
```

# Current dependency amendment — revised Plan 230; M10 product authority remains closed

Plan 204 remains convergence-only. M10 product/application authority through Plans 213–215 is already closed and is not reopened by this work. The remaining dependency is the independent M6 Java second-family lane. Revised Plan 230 replaces the broad profile-population attribution with an exact reachability-capability/profile-bootstrap corrective: prove Java's `heardAbout()` creation predicate, conditionally repair only the synthetic loopback fixture, then continue through the retained Java qualification gates.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan230
plan_230 = registered-ready-m6-java-reachability-capability-profile-bootstrap-corrective
milestone10_final_acceptance = closed
```

# Current dependency amendment — Plan 229 closed; Plan 230 registered

Plan 204 remains convergence-only and blocked on independent M6 Java-family
closure. Plan 229 closed at `P229-C-NOT-EXPLORATORY-ELIGIBLE` (roles and
A-only small-router profile proven; tier population empty) and registered
Plan 230 for the profile-population-path attribution. Neither changes M10
product authority.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan230
plan_229 = passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary
plan_230 = registered-ready-m6-java-profile-population-path-attribution
```

# Current dependency amendment — Plan 229 registered

Plan 204 remains convergence-only and blocked on independent M6 Java-family
closure. Plan 229 changes only the controlled Java test topology needed for
ordinary paired exploratory/client tunnel construction; it does not alter M10
product authority.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan229
plan_229 = registered-ready-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective
```

# Current dependency amendment — Plan 228 closed with NO-PAIRED-TUNNEL boundary

Plan 204 remains convergence-only and blocked on independent M6 Java-family
closure. Plan 228 closed as
`passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary`
(see `plans/closure/mixed-router-interop/228-status.md`): attribution-only at
paired-tunnel selection, no workaround, no production change. It does not
alter M10 product authority.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-paired-tunnel-corrective
plan_228 = passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary
```

# Current dependency amendment — Plan 228 registered

Plan 204 remains convergence-only and blocked on independent M6 Java-family
closure. Plan 228 is attribution-only for the controlled Java client-tunnel
build path and does not alter M10 product authority.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan228
plan_228 = registered-ready-m6-java-client-tunnel-build-path-attribution
```

# Current dependency amendment — Plan 227 closed with NOT-BUILT boundary

Plan 204 remains convergence-only and blocked on independent M6 Java-family
closure. Plan 227 closed as
`passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary`
(see `plans/closure/mixed-router-interop/227-status.md`): Router C selectable
on all 3 counted attempts, no one-hop tunnels built within the five-minute
ceiling. It does not alter M10 product authority.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-build-path-investigation
plan_227 = passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary
```

# Current dependency amendment — Plan 227 registered

Plan 204 remains convergence-only and blocked on independent M6 Java-family
closure. Plan 227 changes only the controlled Java raw-reference helper tunnel
profile through stock I2CP test/debug options; it does not alter M10 product
authority.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan227
plan_227 = registered-ready-m6-java-explicit-one-hop-client-tunnel-corrective
```

# Current dependency amendment — Plan 226 closed at exact non-IP boundary

M10 product authority remains closed through Plans 213–215. Plan 204 remains
convergence-only and blocked because Plan 226 observed Java's exact
`P226-BASELINE-B-ZERO-HOP-UNKNOWN` boundary without qualifying the Java
second-family destination lane. The conditional distinct-topology correction
was correctly not admitted.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan226-baseline-zero-hop-boundary
plan_226 = passed-m6-java-loopback-peer-diversity-corrective-with-exact-baseline-non-ip-pre-dispatch-boundary
milestone10_final_acceptance = closed
next_executable_plan = none
```

# Current dependency amendment — Plan 226 registered

Plan 204 remains convergence-only and blocked on independent M6 Java-family
closure. Plan 226 is a controlled Java harness/topology corrective and does not
alter M10 product authority.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan226
plan_226 = registered-ready-m6-java-loopback-peer-diversity-corrective
```

# Current dependency amendment — Plan 225 closed with exact lookup attribution

Plan 224 closed as
`passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap`.
Its exact-clean-head destination evidence proves Router-B LS2 answerability
and a persistent empty Java helper client DB, but the lookup-path trace is not
observable. Plan 225 closed the resulting diagnostic corrective with the exact
terminal `P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B`: the Java
helper search started and exhausted without dispatching the target lookup to
Router B. The M6 Java second-family convergence gate remains open because this
is diagnostic evidence, not a product correction or final qualification.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan201-after-plan225-attribution
plan_224 = passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap
plan_225 = passed-m6-java-no-leaseset-lookup-path-observability-corrective-with-exact-attribution
plan_201 = blocked-pending-m6-java-second-family-closure-after-plan225-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
milestone10_final_acceptance = closed
next_executable_plan = none
```

# Current dependency amendment — Plan 224 attribution registered

M10 product authority remains closed through Plans 213–215. Plan 204 remains
convergence-only and blocked on independent M6 Java-family closure.

Plan 224 is now the active M6 attribution pass for the
`ACCEPTED -> NO_LEASESET (21)` boundary. It does not authorize a production
corrective and does not change M10 product authority.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan224-attribution
plan_224 = registered-ready-m6-java-no-leaseset-lookup-path-attribution
```

# Current dependency amendment — Plan 223 closed with NEXT-BOUNDARY

M10 product authority remains closed through Plans 213–215. Plan 204 remains
convergence-only and blocked on independent M6 Java-family closure.

Plan 223 closed as
`passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset`
(see `plans/closure/mixed-router-interop/223-status.md`): the early identity
guard is removed (type-0/256, LS2 type-4/32, status 17 gone), but the reverse
path now stops at `NO_LEASESET (21)` after `ACCEPTED`. M6 Java-family closure
still pending; Plan 204 stays blocked pending the Plan-224 successor. Plan 223
does not reopen M10 product closure.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan224-after-plan223
plan_223 = passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset
```

# Current dependency amendment — Plan 223 registered

M10 product authority remains closed through Plans 213–215. Plan 204 remains
convergence-only and blocked on independent M6 Java-family closure.

Plan 223 is the active M6 corrective after Plan 222 narrowed the reverse
failure to status 17. Plan 223 does not reopen M10 product closure and does
not itself close M6.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan223
plan_223 = registered-ready-m6-java-destination-identity-crypto-separation-corrective
```

# Current dependency amendment — Plan 222 closed with OCMOSJ narrowing

M10 product authority remains closed through Plans 213–215. Plan 204 is still
convergence-only and blocked on independent M6 Java-family closure.

Plan 222 closed as `passed-m6-java-client-netdb-ocmosj-narrowing-corrective`:
the M6 Java reverse failure is narrowed to
`P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION` (exact client
lookup has candidates; nonce-tracked send draws OCMOSJ status 17; no i2pr
TunnelData/payload in the frozen 45-second window). The lane is narrowed,
not closed. Plan 204 stays blocked pending the dedicated OCMOSJ corrective
and M6 Java second-family closure.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-ocmosj-corrective-after-plan222
plan_222 = passed-m6-java-client-netdb-ocmosj-narrowing-corrective
```

# Current dependency amendment — Plan 222 narrowing

M10 product authority remains closed through Plans 213–215. Plan 204 is still
convergence-only and blocked on independent M6 Java-family closure.

Plan 221 was superseded before execution after exact-pinned source review found
that the inherited selector probe did not reproduce Java's real routing-key and
search-width semantics. Plan 222 is the dependency-ready corrected narrowing.

```text
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan222-narrowing
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = registered-ready-m6-java-client-netdb-ocmosj-narrowing-corrective
```

# Plan 204 status — final M6/M10 evidence and authority convergence

Status: **`blocked-on-m6-java-second-family-closure-pending-plan201-after-plan225-attribution`**.

## 2026-09-18 follow-up — Plan 220 passed, convergence still blocked

Plan 220 closed as
`passed-m6-java-plan219-diagnostic-attribution-corrective` (see
[`220-status.md`](../../mixed-router-interop/220-status.md)): the
M6 Java lane is narrowed to `P220-OBSERVABILITY-GAP-CLIENT-NETDB`
but not closed. Plan 204 stays blocked with an unchanged token
pending M6 Java second-family closure via the Plan 221 → Plan 201
chain.

```text
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective
plan_221 = registered-ready-m6-java-client-netdb-ocmosj-narrowing
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan220-diagnostic-corrective
```

## 2026-09-18 dependency correction — Plan 219 attribution superseded

M10 product/application closure through Plans 213–215 remains authoritative.
Plan 204 stays convergence-only and blocked on independent M6 Java-family
closure.

Plan 219's J219-B attribution is superseded. Plan 220 now owns the diagnostic
correction, not a bootstrap/topology fix.

```text
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = registered-ready-m6-java-plan219-diagnostic-attribution-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan220-diagnostic-corrective
```

## Retained prior Plan 204 status narrative

# Plan 204 status — final M6/M10 evidence and authority convergence

Status: **`blocked-on-m6-java-second-family-closure-pending-plan220-j219-b-corrective`**.

## 2026-09-18 dependency amendment

M10 product/application closure is already authoritative through Plans 213, 214
and the hosted Plan 215 re-verification. Plan 204 is now a convergence-only
normalization pass and MUST NOT reopen or downgrade those M10 results.

The remaining hard dependency is independent M6 Java second-family closure:

```text
Plan 217 — Java closure harness/evidence corrective (closed)
    -> Plan 218 — direct-I2CP Java final qualification (stopped)
    -> Plan 219 — reverse-delivery root-cause investigation (closed: J219-B-A-STORED-B-RI-NOT-F)
    -> Plan 220 — J219-B bidirectional-bootstrap corrective (next executable plan; not registered by Plan 219)
    -> later corrective / Java-family closure
    -> Plan 204 — cross-milestone authority/docs convergence
```

Plan 205 was retained as a conditional fallback if Plan 218, after Plan 217
closed, proved a genuine stock-Java direct-I2CP public-client boundary. Plan
218 ran the corrected harness and recorded a reproducible inbound-delivery
boundary at the helper's outbound tunnel endpoint
(`java-floodfill-candidate=0` on Router A); Plan 205's SAM-bridge helper
pivot does not address this primitive (Plan 217 §6 disposition; Java SAM
ultimately routes through the same
`ClientConnectionRunner → I2PSessionImpl → FloodfillNetworkDatabaseFacade`
machinery as direct-I2CP). Plan 205 stays
`retained-deferred-conditional-after-plan218-direct-i2cp-requalification`.

Plan 219 — M6 Java reverse-delivery root-cause investigation — closed with the `J219-B-A-STORED-B-RI-NOT-F` attribution on commit `9ce32a9c8e860aa1c9dd19b8ae53a42da3d9a2c2` (see `plans/closure/mixed-router-interop/219-status.md`). It owned attribution of the Plan 218 boundary before any corrective successor is authorized. Plan 204 remains blocked until the Java second-family actually closes; the Plan 219 classification alone does not unblock convergence — the documented next executable Plan 220 owns the J219-B corrective (not registered by Plan 219).

Current authority:

```text
plan_213 = passed-m10-router-backed-generic-external-qualification
plan_214 = passed-m10-product-only-remote-http-and-irc-application-closure
plan_215 = passed-m10-hosted-plan214-tunnel-config-generation-corrective-and-exact-head-reverification
milestone10_final_acceptance = closed

plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = passed-m6-java-reverse-delivery-root-cause-attribution (J219-B-A-STORED-B-RI-NOT-F on commit 9ce32a9c8e860aa1c9dd19b8ae53a42da3d9a2c2)
plan_220 = registered-ready-m6-java-j219-b-bootstrap-bidirectional-corrective (next executable plan)
plan_205 = retained-deferred-conditional-after-plan219-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed (Plan 219 typed the attribution to J219-B-A-STORED-B-RI-NOT-F on commit 9ce32a9c…; Plan 220 owns the corrective on the corrected Plan 217 harness; the seven §11 stop rows past criteria 10/22/24/25 remain bounded on the inbound-delivery primitive until Plan 220 closes J219-B and re-reaches J219-J)

plan_204 = blocked-on-m6-java-second-family-closure-pending-plan220-j219-b-corrective
```

The prior Plan 204 narrative is retained below for traceability; its older
references to Plans 213/214 as unfinished and Plan 205 as the active Java branch
are superseded by this amendment.

## Retained prior status narrative

Plan of record: [`204-m10-final-closure-evidence-authority-and-documentation-normalization.md`](204-m10-final-closure-evidence-authority-and-documentation-normalization.md).

Plan 204 remains a later cross-milestone convergence/normalization pass. It must not hide product/evidence defects, and it must not force the independently testable M10 product to remain open solely because the Java M6 second-family branch is unresolved.

## Current execution graph

```text
Java / M6 branch:
  independent Plan 205 / evidence-driven successor
    -> M6 Java second-family closure

M10 branch:
  Plan 212 source closure (retained)
    -> router-backed per-service Destination state
    -> real per-service outbound/inbound tunnel material
    -> real local LS2 from real inbound lease metadata
    -> production receive-id ownership
    -> per-service target LS2 lookup
    -> server LS2 publication
    -> Garlic -> pop_payload -> canonical Streaming receive

  Plan 213 qualification
    -> replace immutable-false generic A/B scaffold with real application I/O
    -> independent exact-pinned i2pd SAM STREAM service + initiator
    -> command/state-derived evidence only
    -> generic Direction A + Direction B hosted exact-head proof twice

  Plan 214 final application qualification
    -> harden retained Plan 211 HTTP/IRC evidence path
    -> system curl HTTP row with target fixture + production counters
    -> exact-pinned jaraco IRC row with target fixture/privacy/DCC + counters
    -> hosted full-lane repeatability twice
    -> M10 closure

parallel outcomes:
  Plan 214 may close Milestone 10 independently
  Java branch may close Milestone 6 independently

later convergence:
  closed Java M6 branch + already-closed M10 -> Plan 204 authority/docs convergence
```

## Why Plans 213/214 were required after Plan 212 source closure

Plan 212 fixed the product seam, but its external generic driver is not yet executable proof: Direction A/B success booleans are immutable `false`, no counted application byte exchange occurs, the default M10 runner skips generic destination provisioning, and several mandatory qualification facts are unconditional literals.

The retained Plan 211 application driver also needs final evidence hardening: it directly constructs a placeholder manager, stamps pin/privacy facts, uses insufficient target-side/DCC derivation, and does not guarantee continuous `ServiceProduct::poll_inbound()` progress while blocking external clients execute.

Those are qualification defects. They do not invalidate the retained Plan 212 source architecture, but they prevent M10 closure until corrected and executed.

## Current authority

```text
plan_200 = retained diagnostic/evidence pass
plan_201 = retained/in-progress Java corrective history
plan_204 = blocked-on-independent-java-m6-branch-and-m10-plan213-plan214-qualification
plan_205 = registered/in-progress Java second-family branch

plan_202 = retained-partial-routing-capability-surface
plan_203 = retained-partial-application-observation-scaffolding
plan_206 = retained-partial-executable-backend-seams
plan_207 = retained-partial-real-application-client-harness
plan_208 = retained-partial-production-call-graph-corrective
plan_209 = retained-partial-black-box-composition-harness
plan_210 = retained-partial-structural-corrective-superseded-by-plan212
plan_211 = retained-source-harness-superseded-for-final-evidence-by-plan214
plan_212 = source-closure-landed-qualification-owned-by-plan213
plan_213 = registered-executable-after-plan212-source-closure
plan_214 = registered-blocked-by-plan213

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
m10_remote_transport_core = not-yet-passed
m10_generic_remote_product = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

The retained M10 local matrix stays passed. Remote rows remain non-authoritative until Plan 213 and Plan 214 pass.

## M10 closure authority

After Plan 213 passes:

```text
plan_212 = passed-source-and-generic-external-qualification-via-plan213
plan_213 = passed-m10-router-backed-generic-external-qualification
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
plan_214 = executable
```

After Plan 214 then passes its two consecutive hosted full-lane runs on one exact source SHA:

```text
plan_214 = passed-m10-http-irc-product-only-external-requalification-and-final-closure
m10_remote_application_interop = passed-via-plan214
milestone10_remote_service_interop = passed-via-plan213-and-plan214
milestone10_final_acceptance = closed-via-plan214
next_product_layer = milestone11-planning
```

This M10 transition does **not** require Java M6 second-family closure.

Plan 204 later consumes independently closed M10 authority plus independently closed Java M6 authority and normalizes cross-milestone documentation. It must not rerun or downgrade valid M10 closure merely because Java closure lands later.
