# Plan 238 status — M6 Java Streaming Router-A admission observer

Status: `passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary`

Plan 238 wired the first real Router-A observation into the Plan-237
proven sendMessage epoch and moved the terminal exactly one D stage
forward on three identical counted executions: the helper JVM proved
scheduler action (delta 2), response-packet construction (delta 1),
and `I2PSession.sendMessage` return (lifetime-event delta 3, zero
failures) exactly as in Plan 237, and Router A proved I2CP admission
of the response client message (`client.distributeTime` delta exactly
3 on all three runs) with zero dispatch
(`client.dispatchTime`/`client.dispatchSendTime` deltas 0), stopping
at the second D stage:

```text
P237-D-CLIENT-MESSAGE-NOT-ADMITTED
```

No Java source was patched, no production Rust changed, and no
Router-A dispatch, transit, target-IBGW, or i2pr delivery is claimed.

## Authority and exact pins

Implementation checkpoint:

```text
08df6ea (Router-A admission observer; three counted full-lane
         attempts b-1/b-2/b-4 with identical terminal and no
         between-attempt tuning)
```

References unchanged:

```text
Java I2P 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
```

Retained: Plan 232 route-derived lease-gateway correction and raw
reverse pass (code untouched), Plan 234 Java accept-returned boundary,
Plan 235 usable `I2PSocket` surface + outbound-admission prerequisite,
Plan 236 source-lock ordering, Plan 237 stock deltas (2/1/3/0/0) and
logger proof, frozen topology, profile policy, publication target, and
timing windows (`DATAGRAM_WAIT`/`STREAM_WAIT`/`SYN_ACK_WAIT` all still
45 s).

## Source lock (WP1)

Exact-pinned source review identified the minimum stock Router-A
signals for I2CP admission of the helper's response client message:

```text
ClientMessageEventListener.handleSendMessage  (I2CP SendMessage entry;
  runs for the best-effort boolean sendMessage overload the SYN-ACK
  response uses, so it fires where a nonce-correlated ack never will)
client.distributeTime  (added after distributeMessage returns; FIRST
  Router-A stage; works for best-effort Streaming)
client.dispatchTime / client.dispatchSendTime  (OCMOSJ dispatch path,
  added after tunnelDispatcher().dispatchOutbound(...) returns;
  immediate dispatch-admission corroboration)
tunnel.dispatchOutboundTunnel  (tunnel-handoff context; advances on
  ALL outbound tunnel traffic including background builds, so it is
  evidence-only in this plan and never satisfies a stage alone)
```

`client.sendMessageSize` (router-side) was reviewed and rejected as
the admission signal: it is added only on the guaranteed-success DSM
path, which the best-effort response never takes. The source lock
(`scripts/interop/check-m6-java-response-source-lock.sh`) is extended
with the seven P238 needles above; the TSV gains three additive rows
(15 rows total, 12 retained shapes frozen). The TSV is byte-identical
across the three counted attempts:

```text
md5 06157ae771d14d45abf9f36601ccdc76  (b-1, b-2, b-4)
```

## Observation mechanism (WP2–WP3)

One tiny public Router-A command (`P238-ADMISSION` in the out-of-tree
`ControlledRouter.java`, observed read-only through the new
out-of-tree `P238Probe.java` compiled against the exact-pinned jars):

- `client.distributeTime`, `client.dispatchTime`,
  `client.dispatchSendTime`, `tunnel.dispatchOutboundTunnel`
  lifetime events via public
  `StatManager.getRate(...).getLifetimeEventCount()`;
- `-1` for a never-created rate (Unknown, never zero-as-fact);
- one bounded `P238-EV kind=admission ...` line with counts only.

No Java source patching, no reflection, no private-field mutation, no
rate creation (creation would fabricate the observed counts), no
packet injection, no NetDB/tunnel/profile/stat writes. Counts are
bounded (four signed integers). No packet contents, keys, tags,
payloads, destinations, hashes, queue contents, or private state cross
into evidence — sanitized counts only, raw logs stay scratch-only.

The Rust driver (`streaming_through_java`) snapshots Router A
(`JAVA_DIAGNOSTIC_A_PORT`, additive env only) immediately before the
Direction-A SYN and at the end of the frozen 45 s response window,
then maps isolated-epoch deltas onto `P237RouterStages` (never
absolutes): `router_i2cp_observed` from a positive distribute delta,
`client_message_admitted` from a positive dispatch delta gated on the
admission above (a later true can never skip an earlier Unknown), all
seven later stages false (Unknown). The Plan-237 B/C ordering and the
no-later-terminal-while-earlier-Unknown rule are retained unchanged in
`p237_classify_response`, which still proves
scheduler/ack/sendMessage before any router attribution.

Router A is long-lived across the destination + streaming sub-runs
(Plan 217 §6.D), so pre snapshots are non-zero when the destination
lane dispatches first (b-1 pre `1/1/1/16`, b-2 pre `1/1/1/19`) and
pristine otherwise (b-4 pre `0/0/0/0`). Deltas isolate the SYN epoch
under the lane-quiet premise (the driver issues no other SendMessage
during the response window); absolutes never attribute.

## Stat/logger enablement proof

All three counted attempts prove all three helper loggers enabled at
both snapshots and all four Router-A rates known (non-`-1`) at both
snapshots:

```text
scheduler_debug_enabled=true connection_debug_enabled=true
packetqueue_debug_enabled=true   (pre and post, attempts b-1/b-2/b-4)
distribute/dispatch/dispatch_send/handoff known pre and post
(b-1: 1/1/1/16, b-2: 1/1/1/19, b-4: 0/0/0/0)
```

Per §8, absence would not have proven non-execution; presence lets
the positive deltas below count as executed evidence, and known-zero
dispatch deltas count as proven dispatch absence for the epoch (not
Unknown).

## Counted evidence (SHA 08df6ea, no tuning between attempts)

| Attempt | Evidence dir | Router-A pre (dist/disp/sent/hand) | Post | Deltas | Terminal |
| --- | --- | --- | --- | --- | --- |
| b-1 | `target/interop/m6-java-evidence-plan238b-attempt1` | 1/1/1/16 | 4/1/1/23 | 3/0/0/7 | `P237-D-CLIENT-MESSAGE-NOT-ADMITTED` |
| b-2 | `target/interop/m6-java-evidence-plan238b-attempt2` | 1/1/1/19 | 4/1/1/28 | 3/0/0/9 | `P237-D-CLIENT-MESSAGE-NOT-ADMITTED` |
| b-4 | `target/interop/m6-java-evidence-plan238b-attempt4` | 0/0/0/0 | 3/0/0/4 | 3/0/0/4 | `P237-D-CLIENT-MESSAGE-NOT-ADMITTED` |

Reading: Router A admitted all three response client messages through
I2CP (`distributeTime` +3, matching the helper `sendMessageSize`
delta 3 on every run) and dispatched none of them through OCMOSJ
(`dispatchTime`/`dispatchSendTime` +0 with rates known). The
`tunnel.dispatchOutboundTunnel` delta (+7/+9/+4) advances on
background tunnel traffic and is recorded as context only — it never
feeds the classifier (§15 guard).

Retained baselines reproduced identically on all three attempts:

```text
P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED
P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND
P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP
P237 stock post 2/1/3/0/0 with all loggers enabled, deltas 2/1/3/0/0
p237-router-stages router_i2cp_observed=true, all later stages false
```

Destination-lane context (full `both` lane, retained not re-litigated):
b-1 reproduced the Plan-237b shape (`P231-REVERSE-DELIVERY-PASSED`
role=A with the route-derived lease fixture); b-4 took the retained
P227-gate path (`P231-A-OBSERVABILITY-GAP
reason=destination-lane-not-entered`). Neither affects the streaming
epoch, whose Router-A deltas isolate it exactly.

Void attempt (SHA `08df6ea`, retained but not counted):
`plan238b-attempt3` stopped at the retained pre-epoch
`P227-EXPLICIT-ONE-HOP-NOT-BUILT` gate (no one-hop tunnels installed
at all on that run's routers; destination driver skipped by design;
streaming stopped at `phase=lease-lookup reference LeaseSet2 never
resolved`). The P238 observer path never executed, no observer defect
is implicated, and no tuning occurred — the run is void for lack of a
response epoch, not for a measurement defect. The budget counts only
response-epoch executions (b-1/b-2/b-4).

Exploratory shakedown (SHA `08df6ea`, retained but not counted):
`plan238-attempt1` ran the streaming-only sub-lane to prove the
mechanism end-to-end before spending full-lane budget (pre `0/0/0/0`,
post `3/0/0/8`, same `P237-D-CLIENT-MESSAGE-NOT-ADMITTED` terminal).
Pre-commit exploratory runs are never closure evidence; this run was
post-commit but wrong lane shape (`I2PR_M6_JAVA_DRIVER=streaming`,
destination baselines not exercised), so it stays exploratory.

## Proof of no production change

`git status` on the implementation head shows only the six harness
surfaces (§10); no `src/` file changed:

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
scripts/check-m6-mixed-router-acceptance-evidence.sh
scripts/interop/check-m6-java-response-source-lock.sh
tests/integration/m6-interop/java/ControlledRouter.java
tests/integration/m6-interop/run-java.sh
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P238Probe.java  (new)
```

The M6 checker §27 production-surface guard (no `P238`/`p238` in
`i2pr-daemon/src`, `i2pr-client/src`, `i2pr-tunnel/src`,
`i2pr-runtime/src`) is green. The streaming helper
(`ReferenceStreamingService.java`) carries no P238 surface by checker
guard — the observer lives on Router A, never in the client helper.

## Focused verification (implementation head 08df6ea)

Passed locally:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external p236_ -- --test-threads=1   (18 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p237_ -- --test-threads=1   (12 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p238_ -- --test-threads=1   (12 passed)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>
javac <all staged Java helpers incl. P238Probe against exact-pinned jars>   (exit 0)
all 18 boundary/vector/evidence scripts in AGENTS.md
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'   (18 tests OK)
```

Full serial workspace floor: **passed** on the implementation head
(`cargo test --locked --workspace --all-targets -- --test-threads=1`,
exit 0: 2710 passed, 18 ignored, 103 suites, 768.96 s). No
`P238-V-WORKSPACE-FLOOR-INCOMPLETE-SAM-HANG` token was needed.

## Requirement-to-evidence matrix (§17)

1. Minimum stock Router-A signals identified by exact-pinned source
   review: `client.distributeTime` (admission) +
   `client.dispatchTime`/`client.dispatchSendTime` (dispatch) +
   `tunnel.dispatchOutboundTunnel` (context only); router-side
   `client.sendMessageSize` explicitly rejected with reason (DSM-ack
   path only, never taken by best-effort responses).
2. Read-only diagnostic surface added on the existing
   `ControlledRouter` command pattern (`P238-ADMISSION`, bounded
   public facts only, P231Probe precedent reused not copied).
3. Streaming driver epoch feeds real observations into
   `P237RouterStages` (baseline + post snapshots, deltas where
   cumulative); Plan-237 B/C ordering and Unknown rule retained.
4. M6 checker §27 extended with the §11-shaped invariants (no
   literals, no placeholder satisfaction, no Router-A claim before
   sendMessage, no production surface, no timeout/topology/pin
   changes, no raw-log promotion).
5. Focused unit rows (12 `p238_*`) lock the new stage ordering:
   sendMessage-proven precedes any D claim; earliest-D wins; later
   cannot skip earlier Unknown; handoff context alone satisfies
   nothing; i2pr stages still require expected TunnelData;
   accept/socket-surface still satisfy nothing; no production change.
   Full `p236_`/`p237_` suites stay green (18 + 12).
6. i2pr attribution only after exact expected TunnelData: classifier
   order unchanged + unit row
   `p238_i2pr_owned_terminal_requires_expected_tunneldata`; live runs
   never reach E/F (stop at second D).
7. Plan-232 raw-Destination authority intact: untouched; destination
   lane rows unchanged (b-1 reproduces the reverse pass shape).
8. Verification honest: full floor green (no SAM-hang token needed);
   void + exploratory attempts documented, not counted.

## Limitations and findings

- Severity low (boundary, not defect): the D terminal advanced
  exactly one stage. Dispatch absence (`dispatchTime` +0 with rates
  known) is a proven epoch observation, not a proven Router-A
  failure cause — OCMOSJ lease selection / outbound-tunnel selection
  remain unobserved. Plan 239 is registered for exactly that narrow
  dispatch observer.
- i2pr-side E/F stages (transit/IBGW/TunnelData/recovery/Garlic/
  adapter) were not reached; their ordering is implemented and
  unit-locked but live unproven.
- The handoff-context delta varies per run (+7/+9/+4) with background
  tunnel traffic; it is recorded but never classified (§15 guard +
  unit row `p238_outbound_tunnel_context_never_satisfies_stage_alone`).
- The destination lane keeps its known run-to-run variance (P227
  gate family): b-1 passed reverse delivery, b-4 entered the
  observability gap, b-3 never built tunnels at all. All three are
  retained prior-plan shapes; none implicate the P238 observer, and
  the streaming-epoch deltas isolate the response epoch regardless.
- Direction A did not establish; §16 continuation does not trigger.
  No M6 Java-family closure is claimed.

## Disposition and unblock audit

```text
plan_201 = blocked-after-plan238-client-message-not-admitted-pending-router-a-dispatch-observer-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan239
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_237 = passed-m6-java-streaming-stock-response-observability-corrective-with-router-i2cp-not-observed-boundary
plan_238 = passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary
plan_239 = registered-ready-m6-java-streaming-router-a-dispatch-observer
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = 239-m6-java-streaming-router-a-dispatch-observer
```

Plan 201's streaming axis consumed its `pending-plan238` hard
dependency (the Router-A observer executed; admission proven,
dispatch-absence proven for the epoch), but Plan 201 stays blocked:
the streaming axis now needs the Plan 239 dispatch-stage observer,
and the publication/final-closure axis is unchanged. Plan 204 stays
blocked on M6 Java second-family closure (now pending Plan 239
instead of Plan 238); M10 product authority through Plans 213–215 is
unchanged. Plan 205 stays retained/deferred. No other registered plan
listed Plan 238 as a hard dependency, so nothing else changes state.

## Follow-up boundary

Plan 239 owns only the narrow Router-A dispatch observer for the
streaming response epoch (read-only, stock signals, no Java patching,
no production change unless exact expected TunnelData proves an
i2pr-owned defect). It must retain the Plan-238 deltas, source lock,
frozen topology/profile/timing, and fail-closed lanes, and stop at
the first proven D/E stage. Broader publication, tunnel-policy, or
production correctives remain out of scope until a plan-of-record
proves them.
