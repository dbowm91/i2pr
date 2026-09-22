# Plan 237 status — M6 Java Streaming stock-response observability corrective

Status: `passed-m6-java-streaming-stock-response-observability-corrective-with-router-i2cp-not-observed-boundary`

Plan 237 replaced the Plan-236 literal response placeholders with real,
bounded stock-Java observations and identified the first missing response
stage on three identical counted executions: the helper JVM proved
scheduler action, response-packet construction, and `I2PSession.sendMessage`
return, then stopped at the first Router-A stage, for which no observer
exists yet on the streaming path.

```text
P237-D-ROUTER-I2CP-NOT-OBSERVED
```

No Java source was patched, no production Rust changed, and no Router-A or
i2pr delivery is claimed.

## Authority and exact pins

Implementation checkpoints:

```text
7506d06 (void: first observer implementation; counted attempt 1 exposed
         two stock-signal defects; evidence retained but void as closure
         proof per §13)

a1065d1 (corrective observer implementation; three counted attempts,
         identical terminal, no between-attempt tuning)
```

References unchanged:

```text
Java I2P 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
```

Retained: Plan 232 route-derived lease-gateway correction, Plan 232
bidirectional raw-Destination pass, Plan 234 Java accept-returned boundary,
Plan 235 usable `I2PSocket` surface, Plan 236 source-lock ordering, frozen
topology, profile policy, publication target, and timing windows
(`DATAGRAM_WAIT`/`STREAM_WAIT`/`SYN_ACK_WAIT` all still 45 s).

## Source lock

The Plan-236 structural source lock remains required and passes; Plan 237
extended it with the seven pinned stock signals the helper observer counts
(`scripts/interop/check-m6-java-response-source-lock.sh`):

```text
SchedulerReceived "received con... send a packet"
SchedulerReceived "received con... time till next send: "
SchedulerImpl logger getLog(SchedulerImpl.class)   (corrective: scheduler
  events log via the superclass logger)
Connection.sendPacket "Resend in "                  (corrective: active
  first-send timer log; ackImmediately fires only on dup/fast-ack paths)
PacketQueue "stream.con.sendMessageSize"
PacketQueue "Unable to send the packet" / "Send failed for " /
  "ms to sendMessage(...)"
```

Durable source-lock TSVs (12 rows) are retained per counted attempt under
`target/interop/m6-java-evidence-plan237b-attempt{1,2,3}/java-response-source-lock.tsv`.

## Observation mechanism

One tiny public helper command (`REPORT_RESPONSE_STATS` in the out-of-tree
`ReferenceStreamingService.java`, compiled against the exact-pinned jars):

- `stream.con.sendMessageSize` lifetime events via public
  `StatManager.getRate(...).getLifetimeEventCount()`;
- exact source-locked DEBUG/WARN substrings scanned from the public
  `LogManager.getBuffer().getMostRecentMessages()` console buffer
  (scheduler / sendPacket-construction / send-failure / send-exception);
- logger-enablement proof via public `LogManager.getLimits()`.

Only the four exact-pinned Streaming classes are set to DEBUG
(`SchedulerImpl` actual + `SchedulerReceived` intent + `Connection` +
`PacketQueue`), via public `setLimits`/`setConsoleBufferSize` before any
SYN can arrive. No Java source patching, no reflection, no private-field
mutation, no packet injection, no forced scheduler execution. Counts are
bounded (console buffer 20+4 slots; epoch adds ~6 messages; counters cap at
`MAX_OBSERVATIONS = 1024`). No packet contents, keys, tags, payloads, or
private state cross into evidence — sanitized counts/booleans only, raw
logs stay scratch-only.

The Rust driver (`streaming_through_java`) snapshots the helper immediately
before the Direction-A SYN and at the end of the frozen 45 s response
window, then classifies the earliest B/C/D/E/F stage from deltas (never
absolutes, never the Plan-236 placeholders, never `accept_returned` or
`socket_surface_ready` alone). Router-A/C/B stages have no probe on the
streaming path in this plan, so they stay false (Unknown); a proven
sendMessage therefore stops at the first D stage with a narrow successor
registered — never a later terminal.

## Logger enablement proof

All three counted attempts prove all three loggers enabled at both snapshots:

```text
scheduler_debug_enabled=true connection_debug_enabled=true
packetqueue_debug_enabled=true   (pre and post, attempts 1-3)
```

Per §8, absence would not have proven non-execution; presence lets the
positive deltas below count as executed evidence.

## Counted evidence (SHA a1065d1, no tuning between attempts)

| Attempt | Evidence dir | Pre (sched/ack/send/fail/exc) | Post | Deltas | Terminal |
| --- | --- | --- | --- | --- | --- |
| 1 | `target/interop/m6-java-evidence-plan237b-attempt1` | 0/0/0/0/0 | 2/1/3/0/0 | 2/1/3/0/0 | `P237-D-ROUTER-I2CP-NOT-OBSERVED` |
| 2 | `target/interop/m6-java-evidence-plan237b-attempt2` | 0/0/0/0/0 | 2/1/3/0/0 | 2/1/3/0/0 | `P237-D-ROUTER-I2CP-NOT-OBSERVED` |
| 3 | `target/interop/m6-java-evidence-plan237b-attempt3` | 0/0/0/0/0 | 2/1/3/0/0 | 2/1/3/0/0 | `P237-D-ROUTER-I2CP-NOT-OBSERVED` |

Reading: `SchedulerReceived` reached its send branch twice;
`Connection.sendPacket` constructed the response (first-send timer log);
`PacketQueue` reached and returned from `I2PSession.sendMessage` three
times with zero stock failure/exception evidence; no Router-A I2CP
admission was observed (no probe exists yet — Unknown, not a proven
Router-A failure).

Retained baselines reproduced on all three attempts:

```text
P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED
P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND
P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP  (placeholders intact;
  P237 never consumes them)
```

Void pre-budget attempt (SHA `7506d06`, retained but not counted): post
`0/0/3/0/0`, terminal `P237-B-SCHEDULER-NOT-OBSERVED`. Post-run
pinned-source review proved the observer defects corrected above
(scheduler logger class; ackImmediately never fires on fresh SYN-ACK), so
the B terminal measured the broken observer, not the response path. The
budget restarted on `a1065d1` per §13; no tuning occurred between the
three counted attempts.

## Proof of no production change

`git status` on the closing head shows only the five harness surfaces
(§10); no `src/` file changed:

```text
tests/integration/m6-interop/java/ReferenceStreamingService.java
tests/integration/m6-interop/run-java.sh
crates/i2pr-daemon/tests/java_tunnel_external.rs
scripts/check-m6-mixed-router-acceptance-evidence.sh
scripts/interop/check-m6-java-response-source-lock.sh
```

The M6 checker §26 production-surface guard (no `P237`/`p237` in
`i2pr-daemon/src`, `i2pr-client/src`, `i2pr-tunnel/src`,
`i2pr-runtime/src`) is green.

## Focused verification (closing head a1065d1)

Passed locally:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external p236_ -- --test-threads=1   (18 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p237_ -- --test-threads=1   (12 passed)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>
javac <all staged Java helpers against exact-pinned jars>   (exit 0)
all 18 boundary/vector/evidence scripts in AGENTS.md
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'   (18 tests OK)
```

Full serial workspace floor: **passed** on the closing head
(`cargo test --locked --workspace --all-targets -- --test-threads=1`,
exit 0), including `sam_stream_final_acceptance` 10/10 in 372 s. The
recurring SAM hang Plan 236 recorded did not reproduce; no SAM or
production behavior was changed. No `P237-V-WORKSPACE-FLOOR-INCOMPLETE`
token was needed.

## Requirement-to-evidence matrix (§17)

1. Placeholders no longer treated as observations: `REPORT_STREAM_STATE`
   literals retained for P236 compat but never parsed by P237
   (`p237_parse_response_stats` rejects the `STREAM_STATUS` line;
   unit row `p237_placeholder_false_is_unknown_not_negative_evidence`).
2. Exact stock response path remains source-locked: extended lock green
   on all attempts (12-row TSVs in evidence).
3. Real stock log/stat evidence for the isolated SYN epoch: pre/post
   snapshots + deltas on all attempts (table above); fresh helper per
   counted attempt via `run-java.sh`.
4. Earliest response stage identified on repeated counted execution:
   `P237-D-ROUTER-I2CP-NOT-OBSERVED` ×3 identical, deltas 2/1/3/0/0.
5. No Java source or production Rust patched: see proof above.
6. Router-A attribution only after sendMessage-returned: classifier
   order + unit rows `p237_send_failure_precedes_router_attribution`,
   `p237_router_attribution_requires_sendmessage_returned`; live D
   terminal follows delta 3 with zero failures.
7. i2pr attribution only after exact expected TunnelData: classifier
   order + unit row
   `p237_i2pr_owned_terminal_requires_expected_tunneldata`; live run
   never reaches E/F (stops at first D).
8. Plan-232 raw-Destination authority intact: untouched; destination lane
   rows unchanged.
9. Verification honest: full floor green (no SAM-hang token needed);
   void attempt documented, not counted.
10. Registry/roadmap/Plan-201/Plan-204 state updated together: see
    disposition below.

## Limitations and findings

- The D terminal is an observability boundary, not a proven Router-A
  failure: no Router-A/C/B probe exists on the streaming path yet. Plan
  238 is registered for exactly that narrow observer.
- i2pr-side E/F stages (TunnelData/recovery/Garlic/adapter) were not
  reached; their ordering is implemented and unit-locked but live
  unproven.
- The helper console buffer holds 20+4 messages (`setConsoleBufferSize`
  does not resize the already-constructed buffer in the pinned
  implementation); epoch deltas are valid while epoch logs stay below
  capacity (counted epochs add ~6). A successor that enables broader
  logging must re-prove capacity or snapshot mid-epoch.
- Direction A did not establish; §16 continuation does not trigger. No
  M6 Java-family closure is claimed.

## Disposition and unblock audit

```text
plan_201 = blocked-after-plan237-router-i2cp-not-observed-pending-router-a-observer-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan238
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_236 = passed-m6-java-streaming-response-emission-observability-gap
plan_237 = passed-m6-java-streaming-stock-response-observability-corrective-with-router-i2cp-not-observed-boundary
plan_238 = registered-ready-m6-java-streaming-router-a-admission-observer
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = 238-m6-java-streaming-router-a-admission-observer
```

Plan 201's `blocked-pending-plan237` hard dependency is closed (the
stock-response corrective executed; placeholders are real observations
now), but Plan 201 stays blocked: the streaming axis needs the Plan 238
Router-A observer, and the publication/final-closure axis is unchanged.
Plan 204 stays blocked on M6 Java second-family closure (now pending Plan
238 instead of Plan 237); M10 product authority through Plans 213–215 is
unchanged. Plan 205 stays retained/deferred. No other registered plan
listed Plan 237 as a hard dependency, so nothing else changes state.

## Follow-up boundary

Plan 238 owns only the narrow Router-A admission observer for the
streaming response epoch (read-only, stock signals, no Java patching, no
production change unless exact expected TunnelData proves an i2pr-owned
defect). It must retain the Plan-237 deltas, source lock, frozen
topology/profile/timing, and fail-closed lanes, and stop at the first
proven D/E stage. Broader publication, tunnel-policy, or production
correctives remain out of scope until a plan-of-record proves them.
