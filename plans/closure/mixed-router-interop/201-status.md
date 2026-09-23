# Current dependency amendment — Plan 243 closed; Direction-A established; reverse direction + publication closure pending

Plan 243 is closed as
`passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established`
(see `plans/closure/mixed-router-interop/243-status.md`). On the
committed implementation SHA
`f359baba57fc7d952ab7f5a5367d34671c590b18`, the host qualification
gate ran green, three counted same-SHA executions of the frozen
Plan-242 Streaming lane produced `P243-G-DIRECTION-A-ESTABLISHED`
on 2/3 attempts (one earlier stock-Java `I2PSession.connect()`
handshake ceiling stop), and the streaming reverse direction
(Java → i2pr response path) bounded at the retained
`P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`. Direction A
(i2pr → Java Streaming) reaches i2pr's expected TunnelData path
(`inbound_tunneldata=1`, `expected_tunneldata=1`, `recovery=1`,
`garlic_payload=1`, `adapter_successes=1`, `connection_established=true`).

Plan 201's streaming axis now needs the narrow streaming reverse
direction (Java → i2pr) + the bidirectional Streaming qualification
+ the publication / final-closure axis. The publication/final-
closure axis remains independently unresolved. The successor plan
must own the Plan-236-boundary continuation and must NOT authorize
any i2pr production corrective before exact expected TunnelData
reaches i2pr for the reverse direction.

plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan243
plan_242 = passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate
plan_243 = passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established
next_executable_plan = none-pending-m6-java-streaming-reverse-direction-and-publication-corrective

# Current dependency amendment — Plan 243 registered for hosted stock-client-build qualification

Plan 242 is closed as passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate. Its source/static/unit/workspace qualification is green, but its closure host did not have the required Java reference cache plus i2pr daemon pair, so no live counted external attempts executed there.

Plan 243 is now dependency-ready. It does not redesign the harness. It first qualifies a capable Java+i2pr host, then executes the frozen Plan-242 Streaming lane three times on one committed SHA with no tuning. Exact-via-C and the one-in-four explicit branch remain diagnostic only.

The publication/final-closure axis remains independently unresolved.

plan_201 = blocked-after-plan242-corrected-gates-pending-plan243-and-publication-closure
plan_242 = passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate
plan_243 = registered-ready-m6-java-streaming-hosted-stock-client-build-qualification
next_executable_plan = 243-m6-java-streaming-hosted-stock-client-build-qualification

# Current dependency amendment — Plan 242 registered for stock one-hop selector-semantics correction

Plan 241 is closed at
passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary.

Its decisive run proved the corrected Streaming helper can install one inbound and one outbound non-zero-hop client tunnel with zero-hop absent, but the harness stopped because neither tunnel was exact-via-C.

Exact-pinned Java source review now proves that explicitPeers is not deterministic: TunnelPeerSelector.shouldSelectExplicit() honors it only when random.nextInt(4) == 0. Otherwise ClientPeerSelector uses ordinary stock fast-peer selection. Therefore exact-via-C is diagnostic, not a valid lookup prerequisite.

Plan 242 is dependency-ready to correct that harness gate, replace the C-specific profile prerequisite with actual stock candidate/pool readiness, accept any controlled non-zero-hop client pair, and resume the exact Plan-240/241 lookup continuation.

The publication/final-closure axis remains independently unresolved.

plan_201 = blocked-after-plan242-corrected-bootstrap-pair-gate-pending-stock-client-build-successor-and-publication-closure
plan_242 = passed-m6-java-streaming-stock-one-hop-selector-semantics-corrective-with-corrected-bootstrap-and-pair-gate
plan_241 = passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary
plan_242 = registered-ready-m6-java-streaming-stock-one-hop-selector-semantics-corrective
next_executable_plan = 242-m6-java-streaming-stock-one-hop-selector-semantics-corrective

# Current dependency amendment — Plan 241 closed at the A/B build-stage boundary; §20 successor pending

Plan 241 closed as
`passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary`
(see `plans/closure/mixed-router-interop/241-status.md`). The forced
zero-hop Streaming fixture is removed on the counted lane (one-hop
SessionConfig proven active live); the lane now stops at exact typed
build-stage terminals — `P241-A-TRANSIT-BOOTSTRAP-NOT-READY` (2/3,
known Plan-230 profile stochasticity) or
`P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT direction=both` (1/3: 1+1
non-zero-hop tunnels installed but not exact-via-C) — with the old
zero-hop terminal structurally unreachable. Live lookup continuation
(§§8–11) was not reached and is owned by the §20 successor, which
requires its own plan-of-record.

The publication/final-closure axis remains independently unresolved.

```text
plan_201 = blocked-after-plan241-a-b-build-stage-pending-successor-and-publication-closure
plan_240 = passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary
plan_241 = passed-m6-java-streaming-one-hop-client-tunnel-fixture-corrective-with-a-b-build-stage-boundary
next_executable_plan = none-pending-section-20-successor-plan-of-record
```

# Current dependency amendment — Plan 241 registered for the one-hop Streaming fixture corrective

Plan 240 closed at
`passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary`.
The exact Streaming ISJ selected Router B, but the current
`ReferenceStreamingService` explicitly requests zero-hop client tunnels; the
pinned Java `sendQuery()` zero-hop/unknown-RI guard therefore rejects B before
a DLM can be sent.

Plan 241 is now dependency-ready. It changes only the Streaming test helper to
the already-proven stock one-hop SessionConfig through controlled transit
Router C, requires the Plan-230 bootstrap gates and a genuine non-zero client
pair before the SYN epoch, records main-NetDB versus helper-client-NetDB
Router-B RI visibility, and then resumes the exact Plan-240 lookup chain.

The publication/final-closure axis remains independently unresolved.

```text
plan_201 = blocked-after-plan240-b-zero-hop-unknown-ri-pending-plan241-and-publication-closure
plan_240 = passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary
plan_241 = registered-ready-m6-java-streaming-one-hop-client-tunnel-fixture-corrective
next_executable_plan = 241-m6-java-streaming-one-hop-client-tunnel-fixture-corrective
```

# Current dependency amendment — Plan 240 closed at B-ZERO-HOP-UNKNOWN-RI; zero-hop successor pending

Plan 240 is closed at
`passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary`
(see `plans/closure/mixed-router-interop/240-status.md`). Its three
counted Streaming runs on `37025f9` correlate the exact streaming
target ISJ (2 jobs/attempt, both with Router B in `toTry`), prove B
lookup-candidate eligible, and prove the first B-specific boundary
inside pinned `sendQuery()`: the selected outbound tunnel was zero-hop
while A's main NetDB held no validated RI for B, so no target DLM was
ever dispatched to B. IP-close is false on all runs, so no topology
correction is admitted.

The streaming axis now needs the narrow zero-hop-tunnel/
RI-availability successor (no plan-of-record yet). The
publication/final-closure axis remains independently unresolved.

```text
plan_201 = blocked-after-plan240-b-zero-hop-unknown-ri-pending-zero-hop-successor-and-publication-closure
plan_240 = passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary
next_executable_plan = none-pending-zero-hop-successor-plan-of-record
```

# Current dependency amendment — Plan 240 registered for exact Streaming lookup-failure attribution

Plan 239 is closed at
`passed-m6-java-streaming-router-a-dispatch-observer-with-target-leaseset-lookup-failed-boundary`.
Its three counted Streaming runs prove Router-A admission, no local target LS,
zero remote lookup successes, positive remote lookup failures, and no dispatch.

Plan 240 is now dependency-ready. It reuses the retained Plan-225/226 exact
target-search machinery inside the Streaming epoch and must determine the first
supported cause among negative cache, Router-B candidate selection, B-specific
pre-query guards, actual B query/reply delivery, or helper client-subDB
installation. It may not change routing behavior before that attribution.

The publication/final-closure axis remains independently unresolved.

```text
plan_201 = blocked-after-plan239-target-leaseset-lookup-failed-pending-plan240-attribution-and-publication-closure
plan_239 = passed-m6-java-streaming-router-a-dispatch-observer-with-target-leaseset-lookup-failed-boundary
plan_240 = registered-ready-m6-java-streaming-target-leaseset-lookup-failure-attribution
next_executable_plan = 240-m6-java-streaming-target-leaseset-lookup-failure-attribution
```

# Current dependency amendment — Plan 239 tightened to post-admission OCMOSJ attribution

Plan 238's retained terminal label is historically named
`P237-D-CLIENT-MESSAGE-NOT-ADMITTED`, but its executed evidence proves
Router-A I2CP admission occurred: `client.distributeTime` advanced by exactly
3 on all three counted attempts, matching the helper's three sendMessage
events. The actual open Streaming boundary is post-admission / pre-dispatch:
`client.dispatchTime` and `client.dispatchSendTime` remained zero.

Plan 239 is therefore tightened to distinguish the exact OCMOSJ branches:
local target LeaseSet vs remote lookup, lease selection, outbound-tunnel
selection, garlic/tunnel-material failure, and only then
`dispatchOutbound(...)`. Zero `client.leaseSetFoundRemoteTime` alone is not
evidence that no target LeaseSet exists.

```text
plan_201 = blocked-after-plan238-post-admission-pre-dispatch-pending-plan239-and-publication-closure
plan_238 = passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary
plan_239 = registered-ready-m6-java-streaming-router-a-dispatch-observer
next_executable_plan = 239-m6-java-streaming-router-a-dispatch-observer
```

# Current dependency amendment — Plan 238 closed at Client-Message-Not-Admitted; Plan 239 registered

Plan 238 is now closed at
`passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary`
(see `238-status.md`): three identical counted executions on `08df6ea`
proved Router-A I2CP admission (distribute delta exactly 3, matching
the helper sendMessage delta 3) with proven dispatch absence
(dispatch deltas 0 with rates known) from real stock Router-A
observations, stopping at the second D stage. No Java source or
production Rust changed.

Plan 201's streaming axis therefore consumed its `pending-plan238`
hard dependency, but Plan 201 stays blocked: the streaming axis now
needs the Plan 239 narrow Router-A dispatch observer, and the
publication/final-closure axis is unchanged.

```text
plan_201 = blocked-after-plan238-client-message-not-admitted-pending-router-a-dispatch-observer-and-publication-closure
plan_238 = passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary
plan_239 = registered-ready-m6-java-streaming-router-a-dispatch-observer
next_executable_plan = 239-m6-java-streaming-router-a-dispatch-observer
```

# Current dependency amendment — Plan 237 closed at Router-A I2CP-not-observed; Plan 238 registered

Plan 237 is now closed at
`passed-m6-java-streaming-stock-response-observability-corrective-with-router-i2cp-not-observed-boundary`
(see `237-status.md`): three identical counted executions on `a1065d1`
proved scheduler action (delta 2), response-packet construction (delta 1),
and `I2PSession.sendMessage` return (lifetime-event delta 3, zero failures)
from real stock helper-JVM observations, then stopped at the first Router-A
stage, for which no probe exists yet. The Plan-236 literal placeholders are
real observations now; no Java source or production Rust changed.

Plan 201's `blocked-pending-plan237` hard dependency is therefore closed,
but Plan 201 stays blocked: the streaming axis needs the Plan 238 narrow
Router-A admission observer, and the publication/final-closure axis is
unchanged.

```text
plan_201 = blocked-after-plan237-router-i2cp-not-observed-pending-router-a-observer-and-publication-closure
plan_237 = passed-m6-java-streaming-stock-response-observability-corrective-with-router-i2cp-not-observed-boundary
plan_238 = registered-ready-m6-java-streaming-router-a-admission-observer
next_executable_plan = 238-m6-java-streaming-router-a-admission-observer
```

# Current dependency amendment — Plan 237 registered for real stock-Java response observation

Plan 236 remains closed at
`passed-m6-java-streaming-response-emission-observability-gap`.
Its exact source lock is retained, but its response-stage booleans were literal
Unknown placeholders rather than executed observations.

Plan 237 is now dependency-ready. It must replace those placeholders with
real isolated-epoch stock Java observations, preferring the pinned
`SchedulerReceived` / ACK DEBUG events and the public
`stream.con.sendMessageSize` `RateStat.getLifetimeEventCount()` delta.
Router-A attribution may resume only after `I2PSession.sendMessage` return is
proven.

Plan-200/201 publication and final-closure rows remain fail-closed.

```text
plan_201 = blocked-pending-plan237-stock-response-observability-corrective
plan_236 = passed-m6-java-streaming-response-emission-observability-gap
plan_237 = registered-ready-m6-java-streaming-stock-response-observability-corrective
next_executable_plan = 237-m6-java-streaming-stock-response-observability-corrective
```

# Current dependency amendment — Plan 236 closed at exact Java response-emission observability gap

Plan 236 is now closed at
`P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`: the exact pinned Java
response path was source-locked, but stock helper/router observations did not
prove scheduler/packet construction/PacketQueue/I2PSession emission on two
same-SHA attempts. Plan 201 remains blocked; Router-A and i2pr-owned stages
were not attributed.

Plan 235 remains closed at
`passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound`.
Its three same-SHA attempts proved Java socket-surface readiness and i2pr
outbound admission/dispatch, but no inbound TunnelData.

Plan 236 was the bounded continuation. It owned only the exact
missing interval from the pinned Java Streaming response scheduler through
`Connection.sendPacket`, `PacketQueue.enqueue`,
`I2PSession.sendMessage`, Router-A client-message admission/dispatch, the
route-derived target IBGW, and i2pr exact TunnelData.

A returned Java `I2PSocket` is not treated as proof that a response was
emitted. No production corrective is authorized until exact expected response
TunnelData reaches an i2pr-owned failing stage.

Plan-200/201 client-LS2 publication/final-closure rows remain fail-closed and
are not superseded by this diagnostic registration.

```text
plan_201 = blocked-after-plan236-java-response-emission-observability-gap
plan_235 = passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound
plan_236 = passed-m6-java-streaming-response-emission-observability-gap
next_executable_plan = none-registered
```

# Current dependency amendment — Plan 235 closed at the Java socket-surface boundary

Plan 234 is formally closed at Outcome B:
`passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary`.
Three same-SHA counted attempts emitted
`P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED`: the Java accept worker returned
and stored the socket, but i2pr observed no inbound TunnelData in the frozen
SYN epoch. Plan 232's raw-Destination reverse pass remains authoritative.

Plan 235 is now closed at
`passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound`.
It proved Java's public socket surface ready and i2pr outbound admission, but
no inbound TunnelData. Plan 201 remains blocked; no final publication/LeaseSet2
authority claim is unblocked.

```text
plan_201 = blocked-after-plan236-java-response-emission-observability-gap
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
plan_235 = passed-m6-java-streaming-post-accept-response-boundary-attributed-no-i2pr-inbound
next_executable_plan = none
```

# Current dependency amendment — Plan 233 superseded; Plan 234 owns Streaming plus final-closure authority reconciliation

Plan 232 remains closed at
`passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary`.
Its raw-Destination bidirectional pass is retained.

Plan 233 was never executed and is now
`superseded-before-execution-by-plan234-final-closure-authority-corrective`.
Its narrow Streaming SYN-ACK attribution remains part of Plan 234, but Plan 234
also corrects the closure-authority defect: a Streaming pass alone cannot close
Java-family M6 while the full Java harness still treats the Plan-200 §C/D
public-client LeaseSet lifecycle rows as required and exits nonzero for any
non-passed required row.

Plan 234 therefore owns:

1. exact SYN -> SYN-ACK attribution and the narrow justified test/harness
   corrective;
2. Direction A+B Streaming qualification including live refresh;
3. a row-by-row Plan-200 §C/D pass-or-stronger-external-evidence reconciliation;
4. full `run-java.sh` exit 0;
5. the final M6 closure ledger/checker.

No Plan-201 final pass may be recorded before all of those gates agree on one
exact head.

```text
plan_201 = blocked-pending-plan234-streaming-and-final-closure-authority-corrective
plan_232 = passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
plan_233 = superseded-before-execution-by-plan234-final-closure-authority-corrective
plan_234 = registered-ready-m6-java-streaming-syn-ack-and-client-ls2-final-closure-authority-corrective
next_executable_plan = 234-m6-java-streaming-syn-ack-and-client-ls2-final-closure-authority-corrective
```

# Current dependency amendment — Plan 232 closed at Outcome B; Plan 233 owns the Streaming SYN-ACK corrective

Plan 232 closed as
`passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary`
(see `232-status.md`): all three Java-driver local leases derive gateway and
gateway tunnel from the installed inbound route, Router B stays the
independent NetDB publication target, raw-Destination reverse delivery
flips to digest-matched `P232-D-REVERSE-DELIVERY-PASSED` (role B → A,
exact IBGW present), and the Streaming continuation proves initial route
parity live with the Direction-A SYN sent on two consecutive counted
runs, both stopping identically at `SYN-ACK never established
(syn_accepted=false established=false pump_error=0)`.

Plan 233 is the registered narrow Streaming SYN-ACK corrective on the
corrected fixture. Plan 201 remains blocked pending Plan 233; no 201-lane
rerun without it would be interpretable.

```text
plan_201 = blocked-pending-plan233-streaming-syn-ack-corrective-after-plan232-raw-reverse-pass
plan_232 = passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
plan_233 = registered-ready-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix
next_executable_plan = 233-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix
```

# Current dependency amendment — Plan 232 registered for route-derived lease-gateway fixture correction

Plan 231 remains closed at
`passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary`.
Its exact root cause is now executable: the Java external driver installs the
local inbound tunnel through `service_hash` (Router A) but advertises
`java_hash` (Router B) as the local LS2 lease gateway for the same gateway
tunnel id. The same hard-coded gateway assumption exists at all three current
local-lease construction sites (raw Destination, initial Streaming, Streaming
refresh).

Plan 232 owns the narrow fixture correction. It MUST derive the lease gateway
and gateway tunnel id from the installed inbound route while preserving Router B
as the independent NetDB publication target. After route parity is proven it
reruns raw-Destination qualification; if reverse delivery passes it continues
directly through the retained Java Streaming rows and may close Java second-
family M6 without another intermediate plan.

```text
plan_201 = blocked-pending-plan232-route-derived-lease-gateway-fixture-corrective
plan_231 = passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
plan_232 = registered-ready-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure
next_executable_plan = 232-m6-java-route-derived-lease-gateway-fixture-corrective-and-second-family-closure
```

# Current dependency amendment — Plan 231 closed with the exact target-IBGW boundary; narrow lease-gateway corrective pending

Plan 231 closed as
`passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary`
(see `231-status.md`): the tracked reverse send's post-`ACCEPTED` path
is fully attributed — Java A outbound-gateway enqueue proven
(`client.dispatchTime` +1 with a window gateway accept and no
correlated no-matching row), Router C exact one-hop OBEP processing
proven (receive == A send id, processed 1→5), and the selected target
IBGW on Router B for the exact lease tunnel proven absent pre and
post, with i2pr's exact-TunnelData/recovery/Garlic/Destination
counters honestly zero. The closure root-causes the stop to the
test-driver fixture (not to Java internals and not to i2pr
production): the published local LS2 advertises gateway Router B
while the inbound tunnel was built via Router A, so reverse traffic
is addressed to a router holding no inbound gateway for the tunnel.

Plan 201's Plan-231 hard dependency is therefore closed, but Java
second-family closure still requires actual reverse delivery, which
needs the narrow lease-gateway fixture corrective (advertise the
inbound peer's hash in the driver's lease source; no production
change). Re-running 201's lane without it would only reproduce the
stop, so no successor is registered here; the corrective registers
under this lane when approved.

```text
plan_201 = blocked-pending-lease-gateway-corrective-after-plan231-target-ibgw-attribution
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
next_executable_plan = none (narrow lease-gateway fixture corrective recommended)
```

# Current dependency amendment — Plan 231 registered for post-ACCEPT reverse-delivery attribution

Plan 230 remains closed at
`passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary`.
Exact-pinned Java I2P 2.13.0 source ordering now narrows the retained stop:
`ClientMessageEventListener.handleSendMessage()` calls `distributeMessage()`;
the client message pool runs OCMOSJ inline; OCMOSJ runs its `DispatchJob`
inline; and that job calls `TunnelDispatcher.dispatchOutbound(...)` before
returning. Only after `distributeMessage()` returns does
`ackSendMessage()` emit `STATUS_SEND_ACCEPTED`.

Plan 231 therefore owns the remaining post-`ACCEPTED` path, not another
lookup/bootstrap pass: Java A outbound-gateway enqueue -> Router C exact one-hop
OBEP processing -> selected target LeaseSet gateway/inbound-gateway processing
-> TunnelData emission -> exact i2pr inbound TunnelData -> tunnel recovery ->
Garlic decode -> Destination payload. Plan-230 topology, fixture corrections,
and timing windows remain frozen.

```text
plan_201 = blocked-pending-plan231-reverse-delivery-tunnel-dispatch-attribution-corrective
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = registered-ready-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective
next_executable_plan = 231-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective
```

# Current dependency amendment — Plan 230 closed with reverse-delivery boundary

Plan 230 closed as
`passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary`
(see `230-status.md`): the `heardAbout()` predicate was proven, the matching
stock C1+C2 fixture correction applied on baseline evidence, natural profile
bootstrap proven (2/3 counted attempts, RI-identity matched), the exploratory
+ client-tunnel continuation passed, and the frozen destination lane ran once
end-to-end with digest-matched forward delivery — stopping at the reverse
Java→i2pr payload, which reproduces the retained Plan-218 signature (send
admitted, no payload in 45 s, no terminal status). No new i2pr-visible
wire/protocol boundary was observed. Plan 201's Plan-230 hard dependency is
therefore closed, but Java second-family closure still requires the
reverse-delivery boundary, which has no registered corrective. Re-running
201's lane without one would only reproduce the stop, so no successor is
registered here; a narrow reverse-delivery corrective may be registered
separately under this lane when its trigger is identified.

```text
plan_201 = blocked-pending-reverse-delivery-corrective-after-plan230-profile-bootstrap
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
next_executable_plan = none (a narrow reverse-delivery corrective may be registered separately)
```

# Current dependency amendment — Plan 230 replaced by reachability-capability/profile-bootstrap corrective

Exact-pinned Java source review supersedes the original Plan-230 open-ended profile-population attribution. The historical Plan-229 `unreachable=false` fact was derived from `ProfileOrganizer.isFailing()`, which is deprecated and unconditionally false in the pinned Java 2.13.0 source. Ordinary `ProfileManagerImpl.heardAbout()` profile creation is instead gated by the peer RouterInfo capability predicate (`R` plus floodfill/bandwidth/congestion conditions). Revised Plan 230 first proves that predicate, conditionally corrects only the controlled loopback reachability / Router-C bandwidth configuration when justified, requires natural profile creation through the authenticated RI bootstrap, and then continues directly through the retained P229/P228/P201 gates.

```text
plan_201 = blocked-pending-plan230-reachability-capability-profile-bootstrap-corrective
plan_230 = registered-ready-m6-java-reachability-capability-profile-bootstrap-corrective
next_executable_plan = 230-m6-java-reachability-capability-profile-bootstrap-corrective
```

# Current dependency amendment — Plan 229 closed with NOT-EXPLORATORY-ELIGIBLE boundary; Plan 230 registered

Plan 201 remains blocked on Java second-family closure. Plan 229 closed as
`passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary`
(see `229-status.md`): roles restored and the A-only small-router profile
proven live on all 4 counted attempts, but every run stops at
`P229-C-NOT-EXPLORATORY-ELIGIBLE` — the ordinary wire bootstrap populates
NetDB entries without populating Router A's organizer tier population
(`profile_count=0`). Plan 230 now owns the bounded profile-population-path
attribution; no profile injection is authorized.

```text
plan_201 = blocked-pending-plan230-profile-population-path-attribution-after-plan229-not-eligible
plan_229 = passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary
plan_230 = registered-ready-m6-java-profile-population-path-attribution
next_executable_plan = 230-m6-java-profile-population-path-attribution
```

# Current dependency amendment — Plan 229 registered

Plan 201 remains blocked on Java second-family closure. Plan 228 localized the
client-build stop to missing non-zero paired tunnels before dispatch. Plan 229
now owns the bounded stock-Java corrective: restore Router C's intended
non-floodfill transit role, establish genuine non-zero exploratory paired
infrastructure on Router A using Java's supported small-router profile, then
rerun the unchanged client build path.

```text
plan_201 = blocked-pending-plan229-nonzero-exploratory-paired-tunnel-bootstrap-corrective
plan_229 = registered-ready-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective
next_executable_plan = 229-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective
```

# Current dependency amendment — Plan 228 closed with NO-PAIRED-TUNNEL boundary

Plan 201 remains blocked on Java second-family closure. Plan 228 closed as
`passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary`
(see `228-status.md`): Router C selectable, client configs through C created
in both directions, but neither direction obtains the paired tunnel Java
requires (`P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both` on all 4 counted
attempts; only zero-hop exploratory tunnels available). No dispatch ever
occurs, so no lookup/reply/install stage was reached. A narrow paired-tunnel
corrective plan-of-record is required before 201's qualification can proceed;
none is registered here.

```text
plan_201 = blocked-pending-paired-tunnel-corrective-after-plan228-no-paired-tunnel
plan_228 = passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary
next_executable_plan = none (narrow paired-tunnel successor may be registered separately)
```

# Current dependency amendment — Plan 228 registered

Plan 201 remains blocked on Java second-family closure. Plan 227 proved Router C
selectable but no stock-Java one-hop client tunnel installed within the
five-minute helper ceiling. Plan 228 now owns attribution of the exact client
tunnel build path and may not implement a workaround.

```text
plan_201 = blocked-pending-plan228-client-tunnel-build-path-attribution
plan_228 = registered-ready-m6-java-client-tunnel-build-path-attribution
next_executable_plan = 228-m6-java-client-tunnel-build-path-attribution
```

# Current dependency amendment — Plan 227 closed with NOT-BUILT boundary

Plan 201 remains blocked on Java second-family closure. Plan 227 closed as
`passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary`
(see `226-status.md` for the zero-hop baseline and `227-status.md` for the
explicit one-hop attempt): Router C proven selectable in A's main NetDB on
all 3 counted attempts, but stock Java built no one-hop client tunnels
within the five-minute ceiling, so the frozen destination lookup was never
re-observable. The next investigation is the tunnel build request/reply
path, not profile scoring.

```text
plan_201 = blocked-pending-build-path-investigation-after-plan227-selectable-c-not-built
plan_227 = passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary
next_executable_plan = none (narrow build-request/reply successor may be registered separately)
```

# Current dependency amendment — Plan 227 registered

Plan 201 remains blocked on Java second-family closure. Plan 226 proved the
current raw helper is stopped by Java's zero-hop lookup-to-unknown guard.
Plan 227 now owns the narrow stock-Java client-tunnel corrective: establish and
prove a genuine one-hop raw-helper tunnel through Router C using ordinary I2CP
`explicitPeers`, then rerun the frozen destination lane.

```text
plan_201 = blocked-pending-plan227-explicit-one-hop-client-tunnel-corrective
plan_227 = registered-ready-m6-java-explicit-one-hop-client-tunnel-corrective
next_executable_plan = 227-m6-java-explicit-one-hop-client-tunnel-corrective
```

# Current dependency amendment — Plan 226 closed at exact non-IP boundary

Plan 226 closed as
`passed-m6-java-loopback-peer-diversity-corrective-with-exact-baseline-non-ip-pre-dispatch-boundary`
(see [`226-status.md`](226-status.md)). Its exact target-job trace proved
`P226-BASELINE-B-ZERO-HOP-UNKNOWN`, not the hypothesized Router-B IP-close
skip; therefore no distinct SSU2 topology was admitted and Java second-family
closure remains outstanding.

```text
plan_201 = blocked-pending-m6-java-second-family-closure-after-plan226-baseline-zero-hop-boundary
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan226-baseline-zero-hop-boundary
plan_226 = passed-m6-java-loopback-peer-diversity-corrective-with-exact-baseline-non-ip-pre-dispatch-boundary
next_executable_plan = none
```

# Current dependency amendment — Plan 226 registered

Plan 201 remains blocked on Java second-family closure. Plan 225 proved that
Router A's helper search starts and exhausts without dispatching the target
lookup to answerable Router B. Plan 226 now owns the narrow controlled-loopback
peer-diversity corrective and must prove the exact target-job IP-close skip
before changing topology.

```text
plan_201 = blocked-pending-plan226-loopback-peer-diversity-corrective
plan_226 = registered-ready-m6-java-loopback-peer-diversity-corrective
```

# Current dependency amendment — Plan 225 closed with exact lookup attribution

Plan 224 closed as
`passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap` (see
[`224-status.md`](224-status.md)). It proved that Router B's main NetDB held a
current, `receivedAsPublished` target LS2 and that Router A's helper client DB
remained empty before and after the nonce-correlated `ACCEPTED -> NO_LEASESET`
send, but the exact A→B/B→A lookup trace was not observable from the permitted
diagnostics. No lookup-stage corrective is authorized from that gap.

Plan 225 is now closed as the narrow observability corrective with the exact
terminal `P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B`: Router A's
helper search started and exhausted without dispatching the target lookup to
Router B. This closes the observability dependency, but Plan 225 is diagnostic
only and does not complete the Java second-family closure or authorize a
production correction. Plan 201 remains blocked pending that closure.

```text
plan_201 = blocked-pending-m6-java-second-family-closure-after-plan225-attribution
plan_224 = passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap
plan_225 = passed-m6-java-no-leaseset-lookup-path-observability-corrective-with-exact-attribution
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan201-after-plan225-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
next_executable_plan = none
```

# Current dependency amendment — Plan 224 attribution registered

Plan 201 remains blocked. Plan 223 removed the Destination encryption guard,
but the tracked reverse send now stops at `ACCEPTED -> NO_LEASESET (21)`.

Exact-pinned Java source shows status 21 is only a bounded client-NetDB lookup
failure result. Plan 224 now owns attribution across Router-B main-NetDB
answerability, actual helper client lookup dispatch, Router-B answer, Router-A
client-tunnel DSM receipt, and helper client-subDB installation. The concrete
fix belongs to Plan 225 after that attribution.

```text
plan_201 = blocked-pending-plan224-no-leaseset-lookup-path-attribution
plan_224 = registered-ready-m6-java-no-leaseset-lookup-path-attribution
```

# Current dependency amendment — Plan 223 closed with NEXT-BOUNDARY

Plan 201 remains blocked. Plan 223 closed as
`passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset`
(see [`223-status.md`](223-status.md)): post-fix Destinations are ElGamal/type-0/256
with matching Rust/Java hashes, LS2 stays X25519/type-4/32, status 17 is gone,
and the tracked reverse send now draws `ACCEPTED` followed by `NO_LEASESET (21)`
with no payload in 45 s (`P223-NEXT-BOUNDARY [1,21]`). The identity guard is
removed; the new NO_LEASESET boundary (target LS absent in helper client DB,
source X25519-only) needs a dedicated successor. Plan 201 cannot target closure
until that successor lands.

```text
plan_201 = blocked-pending-plan224-no-leaseset-corrective-after-plan223
plan_223 = passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset
```

# Current dependency amendment — Plan 223 registered

Plan 201 remains blocked. Plan 222 narrowed the reverse failure to
`P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION`; exact-pinned
Java source then exposed an earlier status-17 guard on the target
Destination encryption type.

Plan 223 is now the sole executable corrective. It must prove that guard on
the exact i2pr target bytes before changing router-owned Destination
generation, keep LS2 X25519/type 4 unchanged, and rerun the destination-only
Java lane.

```text
plan_201 = blocked-pending-plan223-destination-identity-crypto-separation-corrective
plan_223 = registered-ready-m6-java-destination-identity-crypto-separation-corrective
```

# Current dependency amendment — Plan 222 closed with OCMOSJ narrowing

Plan 201 remains blocked. Plan 222 closed as
`passed-m6-java-client-netdb-ocmosj-narrowing-corrective` (see
[`222-status.md`](222-status.md)): one exact-clean-head destination-only run
emitted `P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION` — the
exact client lookup has candidates (helper client DBID resolves to a client
DB, Java routing key differs, width 6 = 5 + 1, nonempty selector containing
B) and the nonce-tracked helper send (nonce=1) draws the specific OCMOSJ
failure 17 with no i2pr TunnelData/payload in the frozen 45-second window.

Plan 201 cannot target a corrective until a dedicated plan-of-record owns
the `OCMOSJ-UNSUPPORTED-ENCRYPTION` fix. No bootstrap corrective is
authorized (J219-B stays refuted).

```text
plan_201 = blocked-pending-ocmosj-unsupported-encryption-corrective-after-plan222
plan_222 = passed-m6-java-client-netdb-ocmosj-narrowing-corrective
```

# Current dependency amendment — Plan 222 corrected narrowing

Plan 201 remains blocked. Plan 221 was superseded before execution because the
Plan 220 selector preflight was not production-equivalent. Plan 222 now owns
the exact client-NetDB/OCMOSJ narrowing using the helper client DBID, Java's
derived routing key/effective selector width, and nonce-correlated public send
status.

```text
plan_201 = blocked-pending-plan222-client-netdb-ocmosj-narrowing
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = registered-ready-m6-java-client-netdb-ocmosj-narrowing-corrective
```

# Plan 201 status — Java publication corrective and M6 second-family closure

Status: **`blocked-pending-m6-java-second-family-closure-after-plan225-attribution`**.

## 2026-09-18 authority amendment — Plan 220 corrected attribution closed

Plan 220 closed as
`passed-m6-java-plan219-diagnostic-attribution-corrective` (see
[`220-status.md`](220-status.md)): on the exact-clean-head
authoritative run (`a3d2571`) the corrected P220 classifier
emitted `P220-OBSERVABILITY-GAP-CLIENT-NETDB` with every earlier
stage `Known(pass)` — hash cross-check, exact A-stored-B with
current `f` RI, PeerManager indexing, live selector containing B.
The superseded J219-B claim ("Router A lacks Router B's RI") is
refuted on corrected evidence, so no bootstrap corrective is or
was authorized.

Plan 201 cannot target a corrective until Plan 221 narrows the
client-NetDB/OCMOSJ gap to an exactly observed layer.

```text
plan_201 = blocked-pending-plan221-client-netdb-narrowing
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective
plan_221 = registered-ready-m6-java-client-netdb-ocmosj-narrowing
```

## 2026-09-18 authority correction — Plan 219 attribution superseded

Plan 218's Java → i2pr reverse-delivery stop remains reproducible. Plan 219's
claimed J219-B root cause is superseded because its diagnostic measurements
were not taken/derived with the required hash, epoch, selector,
client-NetDB, and directional-dispatch guarantees.

Plan 220 is the dependency-ready diagnostic corrective. No remaining Plan 201
stop row may flip until Plan 220 produces a corrected exact-clean-head
classification and a later plan addresses that boundary if necessary.

```text
plan_201 = blocked-pending-plan221-client-netdb-narrowing
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective
```

## Retained prior Plan 201 status narrative

# Plan 201 status — Java publication corrective and M6 second-family closure

Status: **`blocked-pending-plan220-j219-b-corrective`**.

## 2026-09-18 authority amendment — Plan 219 typed attribution closed

Plan 218 consumed the corrected Plan 217 harness and proved a reproducible
Java→i2pr reverse-delivery failure with a coarse `java-floodfill-candidate=0`
attribution. Plan 219 closed the typed-facts investigation that narrowed the
attribution to the `J219-B-A-STORED-B-RI-NOT-F` boundary: Router B's live
RouterInfo advertises the `f` capability on every snapshot (5 timed
snapshots per A/B/C router across the lane), but Router A's authoritative
store does not have Router B's signed RouterInfo; consequently
`peerManager().getPeersByCapability('f')` indexes only Router A itself under
`f`, and `FloodfillPeerSelector` excludes Router B in the post-ranking
result set (selector_input=1, selector_result=0). The downstream cascade
the J219-B boundary produces — empty PeerManager indexing, empty floodfill
selection, helper `ClientPeerSelector.selectPeers → null`,
`OutboundClientMessageOneShotJob` lease loop unsatisfied, inbound
`TunnelData` never constructed — matches the inbound-delivery primitive
Plan 218 reached on commit `7762e13`.

Current authority:

```text
plan_201 = blocked-pending-plan220-j219-b-corrective
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = passed-m6-java-reverse-delivery-root-cause-attribution
plan_220 = registered-ready-m6-java-j219-b-bootstrap-bidirectional-corrective (next executable plan; not registered by Plan 219)
plan_205 = retained-deferred-conditional-after-plan219-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed (J219-B-A-STORED-B-RI-NOT-F attribution, Plan 220 owns the corrective)
milestone6_interoperable = not-yet-claimed
```

No Plan 201 stop row may flip until a registered Plan 220 closes the
J219-B-A-STORED-B-RI-NOT-F boundary. The typed-facts surface (12 mandatory
keys + 5-timed-snapshot timeline per router) lands in
`target/interop/m6-java-evidence/j219/` and is consumed by
`scripts/check-m6-mixed-router-acceptance-evidence.sh` §14 invariants.

## Retained prior authority narrative

## 2026-09-18 authority amendment (Plan 217 closure superseded)

Plan 216 exposed a test-driver panic, but subsequent source review showed the
recorded Java-side publication blocker was not trustworthy enough to drive the
next implementation. Plan 217 (now `passed`) closed the harness/evidence
corrective: the destination-driver duplicate-block regression was removed and
replaced with a transfer-once invariant assertion plus a
`plan217_outbound_role_transfer_once_invariant` unit row; the positive/negative
Java lifecycle evidence split was enforced; the controlled-launcher
`router.networkDatabase.dbDir` was switched to a relative path; the streaming
driver received a disjoint build/tunnel/message-id namespace; and the harness
gained an `I2PR_M6_JAVA_DRIVER` selector.

Plan 217 closure also confirmed that the Plan 216 destination driver reached
`LeaseStoreIngestOutcome::Completed` before the panic, so the Plan 216
"no LS2 reply arrives" interpretation is no longer authoritative; the
remote Java LS2 lookup did return a signature-valid DatabaseStore through a
real inbound tunnel. The prior Plan 201 Branch C/D three-router topology +
1-hop / zero-hop profile attempts remain as the documented Plan 201
narrow-attempt narrative; the corrected harness is the Plan 218 input.

Current authority:

```text
plan_201 = blocked-by-plan218-fresh-external-classification-on-corrected-harness (predecessor token; current token is `blocked-pending-plan220-j219-b-corrective`)
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = passed-m6-java-reverse-delivery-root-cause-attribution (J219-B-A-STORED-B-RI-NOT-F on commit 9ce32a9c8e860aa1c9dd19b8ae53a42da3d9a2c2)
plan_220 = registered-ready-m6-java-j219-b-bootstrap-bidirectional-corrective (next executable plan)
plan_205 = retained-deferred-conditional-after-plan219-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed (Plan 219 typed the attribution to J219-B-A-STORED-B-RI-NOT-F on commit 9ce32a9c…; Plan 220 owns the corrective on the corrected Plan 217 harness; the seven §11 stop rows remain blocked on the inbound-delivery primitive until Plan 220 closes J219-B)
milestone6_interoperable = not-yet-claimed
```

Plan 218 ran the corrected harness once on commit `7762e13` and
recorded a fresh terminal classification. Criteria 1-9 of Plan 218
§11 pass (authenticated sessions, RouterInfo bootstrap, public
helper session, real outbound and inbound installs, LS2 lookup via
the owned outbound tunnel, LS2 key/signature/destination validation,
local i2pr LS2 publication via the controlled tunnel path, raw
i2pr → Java reference payload digest match); criterion 10
(bidirectional raw Destination payloads match expected digests)
fails on the Java → i2pr direction at the helper's outbound
tunnel endpoint's peer-selection path; criteria 11-25 (Direction A /
Direction B Streaming acceptance matrix + cross-family final-closure
ledger + reference-side acceptance + the seven §11 stop rows) are
bounded by the same inbound-delivery primitive. The underlying
cause is observable in the Plan 218 evidence: `java-floodfill-candidate=0`
and `java-floodfill-candidate-empty=1` on Router A
(`target/interop/m6-java-evidence/reference-facts.tsv`); the helper's
`I2PSessionImpl._leaseSetWait` (`I2PSessionImpl.java:805`) never sees
a usable LeaseSet for the i2pr destination through Java's netDb, and
the helper's outbound tunnel endpoint has no real lease to pick.

Plan 205 stays `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`;
the Plan 218 boundary is on the helper-side outbound tunnel endpoint's
peer-selection path, which is the inverse axis from Plan 205's
SAM-bridge helper-local LeaseSet publication. Plan 205 would re-hit
the same boundary on the corrected harness and is not reactivated by
this Plan 218 outcome. A future plan-of-record (e.g. Plan 219 — M6
Java second-family inbound-delivery primitive) is the documented
next move and is not registered by Plan 218.

The prior Plan 201 execution narrative is retained below for
traceability; its earlier "blocked-by-plan217" disposition is
superseded by this amendment.

## Retained prior execution record

Plan of record: [`201-m6-java-public-client-publication-corrective-and-second-family-closure.md`](201-m6-java-public-client-publication-corrective-and-second-family-closure.md).

Plan 200 closed the diagnostic/evidence side of the lane. Plan 201 §G
observation framework (eleven sanitized counters on
`DestinationTunnelCounters` + public `note_lookup_boundary` helper +
eight `plan201_g_*` unit rows + six blocked_row entries + §11 + §12
invariants) is retained as the Branch G attribution surface for any
future `P200-G-store-acked-remote-lookup-fails` exact-head run.

## Branch A corrective (previous run, commit `2dc926f`)

The `P200-A` / `P200-B` classification was being driven by a test
driver decode gap, not by any i2pr production M6 wire / protocol
defect. Two narrow test-driver fixes closed it:

1. `decode_inbound_i2np` helper in `crates/i2pr-daemon/tests/java_tunnel_external.rs`
   that mirrors the production `router_i2np::dispatch_router_i2np`
   standard-first / short-transport-fallback ordering. Every
   inbound-pump loop in the second-family probe (bootstrap
   DatabaseLookup, destination TunnelData recovery, Streaming
   inbound cell, raw RECEIVED queued dispatch) calls the helper
   instead of `decode_short_transport` alone.
2. `flate2::read::GzDecoder` gunzip of `DatabaseStoreData::RouterInfoCompressed`
   before `RouterInfo::decode`, mirroring the production
   `i2pr_netdb::decompress_router_info` contract.

Both fixes are test-driver scope; production wire is unchanged; no
NetDB / client helper change; no Java-special lookup parser.

The exact-head bootstrap probe is now consistently:

```text
p200-routerinfo-lookup-a-knows-b               response_observed=true key_match=true \
                                                identity_match=true ssu2_addresses=1 \
                                                decoded_payload_match=true \
                                                decoded_payload_len=731 \
                                                expected_payload_len=731 pump_errors=0
p200-routerinfo-lookup-b-knows-a               response_observed=true key_match=true \
                                                identity_match=true ssu2_addresses=1 \
                                                decoded_payload_match=true \
                                                decoded_payload_len=731 \
                                                expected_payload_len=731 pump_errors=0
```

`java-main-netdb-a-knows-b` flipped from `false` (pre-fix) to `true`
(post-fix) consistently across all counted runs. The asymmetric
`b-knows-a` direction also flipped from `false` to `true` once
the same fix was applied — i2pr's inbound I2NP envelope decode
ordering and the gzip-decompress step were both required to round-trip
Java's standard-form, gzipped `DatabaseStore` responses.

## Branch C/D corrective (this run)

Plan 201 §3.3 step 3 names Router C as the documented fallback when
2 routers cannot simultaneously satisfy client-tunnel + floodfill
roles under stock Java I2P 2.13.0. This run implemented Router C
end-to-end:

### Router C implementation

- `tests/integration/m6-interop/java/ControlledRouter.java`:
  `writeClientsConfig()` and the I2CP server properties are
  conditional on the SAM / I2CP port. When the launcher is invoked
  with `samPort=0, i2cpPort=0` (Router C's role), no SAM bridge
  client app is written and no I2CP server is started. Router C
  is a pure router that only owns its SSU2 endpoint.
- `tests/integration/m6-interop/run-java.sh`: added
  `JAVA_TUNNEL_PARTICIPANT_SSU2_PORT`, `JAVA_TUNNEL_PARTICIPANT_DATA`,
  `JAVA_TUNNEL_PARTICIPANT_LOG`, `JAVA_TUNNEL_PARTICIPANT_RI`, plus a
  third `ControlledRouter` process start command and a
  `router.info`-publish wait loop. The bootstrap invocation
  (around line 622) now passes
  `JAVA_TUNNEL_PARTICIPANT_ROUTER_INFO` /
  `JAVA_TUNNEL_PARTICIPANT_SSU2_ENDPOINT` env vars to the test
  driver.
- `crates/i2pr-daemon/tests/java_tunnel_external.rs`:
  `bootstrap_java_router_peers()` now reads Router C's env, dials a
  third SSU2 session, exchanges RouterInfo between all three pairs
  (A↔B, A↔C, B↔C), and probes `c-knows-a` + `c-knows-b` lookups.
  Six `database_store_router_info_wire` invocations cover the
  complete 3-router DatabaseStore matrix.
- `scripts/check-m6-mixed-router-acceptance-evidence.sh`: §13
  invariants enforce (a) the helpers declare a bounded client
  tunnel profile (either `length=0` or `length=1`), (b) `run-java.sh`
  provisions three Java routers (`JAVA_TUNNEL_PARTICIPANT_SSU2_PORT`
  + `datadir-tunnel-participant`), and (c) the bootstrap driver
  handles Router C (`tunnel_participant` + `c-knows-a` +
  `c-knows-b` lookup probes).

### Branch C/D blocker (the honest finding)

With Router C in the topology, two parallel attempts were made to
activate the LS2 publication pipeline:

1. **Zero-hop helper profile** (`inbound.length=0 outbound.length=0
   quantity=1 backupQuantity=0 allowZeroHop=true`) — the original
   pre-Branch-C configuration. The helpers successfully emit
   `READY` (i2cp handshake completes; `I2PSession.connect()` returns
   once the LS2 is created locally with zero-hop leases). However,
   the 10 sanitized Java LS2 lifecycle keys in
   `target/interop/m6-java-evidence/reference-facts.tsv` stay at
   `0` for every key:
   ```
   java-client-subdb-created                    0
   java-create-leaseset2-received               0
   java-client-leaseset-stored-current          0
   java-client-leaseset-publish-scheduled       0
   java-client-leaseset-republish-job-ran       0
   java-client-inbound-tunnel-selectable        0
   java-client-outbound-tunnel-selectable       0
   java-store-emitted                           0
   java-store-ack-observed                      0
   java-store-failure-reason                    0
   ```
   Stock Java's `LeaseSetPublisher` (which is the
   `RepublishLeaseSetJob` queued by `KademliaNetworkDatabaseFacade.store`
   on lease creation) does not actually fire a `sendStore` for the
   helper's destination because the helper's LeaseSet contains only
   the helper's own router endpoint as the zero-hop lease; the
   publisher's "is this LS still useful?" check
   (`RepublishLeaseSetJob.runJob()` line 49) likely filters out the
   LS because there is no eligible client tunnel path.

2. **1-hop helper profile** (`inbound.length=1 outbound.length=1
   quantity=1 backupQuantity=1`) — Plan 201 §3.3 step 1
   corrective. `I2PSession.connect()` blocks for the full 5-minute
   timeout (`I2PSessionImpl.java:805`) and throws `IOException:
   No tunnels built after waiting 5 minutes`. Even with Router C
   added, stock Java's `TunnelManager` cannot build the required
   outbound client tunnel because the peer profile scoring
   (`ProfileOrganizer.java:1394` threshold calculation +
   `_fastPeers` promotion at line 913) does not promote any
   loopback peer into the `_fastPeers` set within the 5-minute
   window. `selectFastPeers()` falls back to
   `selectHighCapacityPeers()` (line 432) which also returns no
   peers; `ClientPeerSelector.selectPeers()` line 92 calls into
   the same `selectFastPeers` and falls through to `selectPeers`
   returning `null`. The `I2PSessionImpl._leaseSetWait` wait loop
   therefore never sees `_leaseSet != null` and exits at the
   5-minute `waitcount > 5*60` check.

Both attempts were made against the same exact-pinned Java I2P 2.13.0
cache (`9134f808337b401e8e53c73734c81fab04280c9d`). The Branch C/D
corrective's intended outcome — flipping the 10 LS2 lifecycle keys
from `0` to `≥1` and the 27 blocked destination / Streaming rows
from `blocked` to `passed` — was not reached.

### Per Plan 201 §3.3 step 3 rule on the Java side

Per Plan 201 §3.3 Branch A's standing rule:

> Do not change i2pr production NetDB code unless the captured
> transcript proves i2pr encoded an invalid I2NP message.

The captured transcripts for both the zero-hop and the 1-hop
attempts prove the i2pr side is encoding a valid standard-form,
gzipped `DatabaseStore` (or a valid zero-hop LS, respectively).
The blocking boundary lives entirely on the Java side: Java's
controlled-topology loopback profile scoring + zero-hop LS
filtering refuse to publish the helper's LeaseSet2 to the floodfill.
Per Plan 201 §3.3 step 3, the next move (a third independent Java
peer) was already taken as Router C in this run, and the
publication pipeline still does not activate.

### What Branch C/D retained as evidence

- Three-router topology is now first-class in the harness.
- A↔B, A↔C, B↔C DatabaseStore exchanges are proven ordinary
  (the bootstrap probe `p200-routerinfo-lookup-{a,b}-knows-{b,a}`
  rows flip `response_observed=true key_match=true
  identity_match=true decoded_payload_match=true`; c↔a/b rows
  show `response_observed=false` because Java's
  `PersistentDataStore` (`PersistentDataStore.java:643`) resolves
  `router.networkDatabase.dbDir` as `i2p.dir.router + dbDir` even
  when `dbDir` is absolute, so the NetDB files end up at
  `datadir/router/tmp/.../datadir/netDb/` and the lookup response
  path is the doubled absolute path; this is a Java-side
  `SecureDirectory` behavior documented at line 643, not an i2pr
  defect).
- The 1-hop helper profile is proven bounded at the 5-minute
  Java-side timeout.
- The zero-hop helper profile is the working configuration that
  keeps `I2PSession.connect()` returning.

### Current branch matrix

```text
branch_a_router_a_missing_router_b          corrective-landed-this-run (Branch A decode fix; one-direction proof a-knows-b=true; symmetric b-knows-a also flipped to true once decode fix is applied)
branch_b_router_b_missing_router_a          recorded-corrective-not-implemented (java-b-netdb-asymmetry-on-bootstrap-storage; matches Plan 194 retained-partial)
branch_c_client_ls2_not_created_or_current  attempted-then-blocked-on-java-loopback-peer-profile-scoring (1-hop profile throws IOException at the 5-min Java timeout even with Router C; zero-hop profile keeps LS2 local but the LeaseSetPublisher never publishes; this run's Branch C/D corrective is the documented failure)
branch_d_client_tunnel_publication_path     registered-corrective-not-implemented (java-zero-hop-client-tunnel-no-publication-path; matches Plan 194 retained-partial)
branch_e_no_eligible_floodfill_candidate    registered-corrective-not-implemented (java-floodfill-candidate-non-empty=1 passes; not the active boundary)
branch_f_store_sent_no_ack                  registered-corrective-not-implemented
branch_g_store_acked_remote_lookup_fails    implemented-framework-landed-blocked-on-exact-head-external-run (eleven sanitized counters + note_lookup_boundary + eight plan201_g_* unit rows + six blocked_row entries + §11/§12 invariants retained)
branch_h_publication_path_passed            registered-no-corrective-needed
```

## Current authority

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187 = blocked-by-m6-build-reply-interop-gap (retained-passed-via-plan188-and-plan190)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective
plan_191 = stopped-by-inbound-delivery-boundary-E
plan_192 = passed-m6-i2cp-wire-format-corrective
plan_193 = passed-m6-i2pd-mixed-router-streaming-qualification
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_198 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_199 = superseded-execution-decomposed-and-closed-via-plans200-204
plan_200 = passed-m6-java-public-client-publication-observability-and-verified-bootstrap
plan_201 = blocked-pending-plan220-j219-b-corrective
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_204 = in-progress-docs-and-authority-normalization-blocked-on-plan201-external-run

milestone6_i2pd_streaming_interop          = passed-via-plan193
milestone6_java_mixed_router_interop       = not-yet-passed (Plan 219 typed the attribution to J219-B-A-STORED-B-RI-NOT-F on commit 9ce32a9c…; Plan 220 owns the corrective on the corrected Plan 217 harness; the remaining Java-side LeaseSet2 publication gap is rooted on Router A's authoritative NetDB, not on the LeaseSetPublisher / profile-scoring axis Branch C/D retained)
milestone6_interoperable                   = not-yet-claimed
m6_java_publication_observability          = landed-via-plan200
m6_java_publication_branch_g_framework     = landed-via-plan201
m6_java_publication_branch_a_corrective    = landed-this-run (one-direction proof)
m6_java_publication_branch_c_d_corrective  = attempted-this-run-blocked-on-java-loopback-peer-profile-scoring (Router C retained; 1-hop profile retained as bounded attempt; zero-hop helpers reverted to working configuration; Plan 219 proved the inbound primitive root cause is upstream of Branch C/D on the J219-B boundary)

next_executable_plan = 220-m6-java-j219-b-bootstrap-bidirectional-corrective (Plan 219 typed the boundary at J219-B-A-STORED-B-RI-NOT-F on commit 9ce32a9c…; the corrective must submit Router B's signed RouterInfo into Router A's authoritative store through the existing authenticated DatabaseStore path so `peerManager().setCapabilities(b_hash, caps)` fires on the receiving side and `FloodfillPeerSelector` indexes Router B under `f`)
remaining_sequence = 220 -> 204-convergence (after Plan 220 closes J219-B on the corrected Plan 217 harness and re-reaches J219-J)
```

## Required validation on the closing head

```text
cargo fmt --all --check                                                OK
cargo check --locked --workspace --all-targets                         OK
cargo test --locked -p i2pr-daemon --test java_tunnel_external         1 passed, 3 ignored (fail-closed ordinary invocation)
cargo test --locked --workspace --all-targets -- --test-threads=1    2357 passed, 12 ignored
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps   OK
cargo test --locked --workspace --doc                                0 passed (16 suites)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh            OK (11 guarded labels + Plan 197 §8 pq parser tolerance + Plan 201 §G observation surface + Plan 201 §13 Branch C/D three-router topology + helper tunnel-profile len∈{0,1})
bash tests/integration/m6-interop/run-java.sh                        external-session-established-java PASSED; the seven Plan 201 §11 stop rows stay blocked with documented Plan 198/199 stop provenance; p200-routerinfo-lookup-{a,b}-knows-{b,a} now flipped to passed (Branch A decode fix); p200-routerinfo-lookup-c-knows-{a,b} still blocked on the Java-side doubled-netDb-path SecureDirectory behavior; the 10 Plan 200 §C/D LS2 lifecycle keys stay at 0 with zero-hop helpers (Branch C/D documented failure); the symmetric 1-hop helper attempt blocks at Java's 5-minute I2PSession.connect() IOException.
cargo deny check advisories bans sources                              OK
```

## Handoff rule

Plan 201 must not claim final closure until:

1. Plan 200 / this Plan 201 run has consumed exactly one
   unambiguous terminal `P200-*` classification against the
   exact-pinned Java I2P 2.13.0 cache (this run: `P200-H`
   classification at the bootstrap layer, because the
   `java-main-netdb-{a,b}-knows-{b,a}` rows both flip to true
   with the Branch A decode fix; the `P200-H` label is the
   "no specific bootstrap boundary failed" catch-all and is not
   itself a publication-path success);
2. the matching Plan 201 branch lands the narrowest corrective
   for that classification (this run: Branch A landed; Branch C/D
   was attempted as a separate run and proved bounded by Java's
   loopback peer profile scoring under the controlled-topology
   constraints);
3. the cross-family M6 checker (`bash scripts/check-m6-mixed-router-acceptance-evidence.sh`)
   stays green on the closing exact head;
4. the manual `M6 mixed-router external interoperability` workflow
   (`.github/workflows/m6-mixed-router-external.yml`) executes both
   the i2pd and Java second-family lanes and produces
   `target/interop/m6-mixed-router-evidence/evidence.json` whose
   `m6_mixed_router` field flips to `passed`;
5. the Plan 199 §8 final closure evidence ledger
   (`bash scripts/check-m6-final-closure-evidence.sh`) is green on
   the same exact head.

Plan 201 cannot record final `passed` status until the Java-side
LeaseSet2 publication boundary is closed by either:

(a) a follow-up Branch B / E / F corrective that uses a different
Java-side entry point (e.g. the SAM bridge path), or
(b) a Plan 205 / future plan that rewrites the helpers onto the
SAM bridge path whose LS2 publication lifecycle is different from
the direct I2CP path the current `ReferenceRawDestination.java`
and `ReferenceStreamingService.java` use.

Until either path closes the Java-side boundary, this status
remains the only authorized Plan 201 record, and Plan 204 stays
blocked on the Java second-family branch per its §1 preconditions.
Plan 218's `plans/closure/mixed-router-interop/218-status.md`
records the exact inbound-delivery boundary that bounds this
amendment.
