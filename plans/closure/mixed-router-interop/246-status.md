# Plan 246 status — M6 Java Streaming delayed-ACK timer enqueue/fire and second-scheduler attribution

Status: `observability-gap-observed-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution`

Plan of record:
`plans/implementation/mixed-router-interop/246-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution.md`

## Registration basis

Plan 245 closed as:
`passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary`

All three counted Plan-245 attempts established Direction A, bound the
response epoch, and observed the exact-pinned `SchedulerReceived`
reschedule branch without the send branch. Exact-pinned Java I2P 2.13.0
source review narrows the successor:

- the frozen helper does not set `i2p.streaming.initialAckDelay`;
- `ConnectionOptions.DEFAULT_INITIAL_ACK_DELAY = 500` ms;
- `Connection.setNextSendTime()` clamps future deadlines to no later
  than `now + getSendAckDelay()`;
- `SchedulerReceived.reschedule()` delegates through
  `Connection.scheduleConnectionEvent()`;
- the transition `SimpleTimer2.addEvent(SimpleTimer.TimedEvent,
  timeoutMs)` creates a fresh one-shot wrapper whose `timeReached()`
  delegates to the Connection `ConEvent`;
- `ConEvent.timeReached()` re-enters `Connection.eventOccurred()` and
  therefore `SchedulerChooser`.

Plan 246 owns only attribution of the numeric delayed-ACK deadline,
matching timer enqueue/fire, second scheduler outcome, and transient
observation eviction. It retains the 45-second outer lane unchanged
and adds only bounded short-interval polling.

No Java source/jar patch, ACK-delay change, topology/publication/
timing correction, or production i2pr change is authorized.

## Current authority

```text
plan_245 = passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary
plan_246 = observability-gap-observed-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution

plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan246-successor-observation-gap-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan246-successor-observation-gap-corrective
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = none-pending-plan246-successor-observation-gap-corrective

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## Authority and exact pins

Implementation checkpoint (committed BEFORE any counted attempt; one
SHA for the entire three-attempt budget):

```text
71e034e89e08926ba083627764fa2326cb1e6464  — Plan 246 implementation
                                             (helper + driver +
                                             run-java + source-lock +
                                             evidence checker)
```

Five files:

- `crates/i2pr-daemon/tests/java_tunnel_external.rs` — Plan-246
  Stage A.1 classifier, polling cadence (50 ms × 40 = 2 s) wired
  after the Plan-245 classification, rolling-maximum tracker for
  transient eviction, `P246ResponseStats`/`P246Deltas`/`P246StageA1`
  data carriers, `P246Terminal` enum with the ten canonical
  terminals (incl. `P246-O-END-OF-WINDOW-SNAPSHOT-EVICTION`), and
  26 §18 unit rows (`p246_*`).
- `tests/integration/m6-interop/java/ReferenceStreamingService.java`
  — observation-only: bounded public LogManager enablement for
  `net.i2p.util.SimpleTimer2` DEBUG (additive to the retained Plan
  237/245 limits), `REPORT_TIMER_STATS` control command emitting 17
  sanitized numeric/count/bool fields, lazy peer-b32 refresh on
  every report call, exact-socket correlation filter that joins the
  SimpleTimer2 lifecycle markers with the active `Connection`
  toString-derived peer substring, and the four Plan-246
  bounded-counter helpers. The helper never persists the peer
  string; only `peer_correlation_present=true|false` is emitted.
  Console buffer is 1024 shared with the retained Plan-237/245
  observability; every counter caps at `MAX_OBSERVATIONS` (1024).
- `tests/integration/m6-interop/run-java.sh` — read-only
  `external-p246-{classification,timer-deltas,poll-cadence,
  stage-a1,timer-stats-pre,timer-stats-post}` rows keyed on the
  driver TSV; no invented terminal; no timing/topology change; the
  45-second outer lane stays unchanged.
- `scripts/interop/check-m6-java-response-source-lock.sh` — adds
  five new path variables (`CONNECTION_OPTIONS`,
  `CONNECTION_PACKET_HANDLER_SRC`, `SCHEDULER_RECEIVED_SRC`,
  `SCHEDULER_CHOOSER_SRC`, `SIMPLE_TIMER2_SRC`) plus 48 Plan-246
  source-lock needles and 10 new TSV row lines. Row count grows
  from 50 to 60; md5 `64b7661c76f433474000e16c6e97d9a7`.
- `scripts/check-m6-mixed-router-acceptance-evidence.sh` — adds §35
  Plan 246 invariants (35a–35h): unit-row count, canonical-terminal
  vocabulary, helper range delimiter, production-code cleanliness,
  harness row coverage, helper string coverage, plan-file invariants,
  closure-record existence.

Java I2P 2.13.0 source pin (no Java source change, no Java jar
patch):

```text
9134f808337b401e8e53c73734c81fab04280c9d  — Java I2P 2.13.0
```

i2pd reference pin (no interop with i2pd on this lane):

```text
n/a  — Plan 246 does not exercise i2pd
```

## Source-lock checksum

```text
60 rows, md5 64b7661c76f433474000e16c6e97d9a7
```

## Active ACK-delay authority

The frozen helper does NOT set `i2p.streaming.initialAckDelay`.
Exact-pinned `ConnectionOptions.java`:

```text
PROP_INITIAL_ACK_DELAY = "i2p.streaming.initialAckDelay"
DEFAULT_INITIAL_ACK_DELAY = 500
setSendAckDelay(getInt(opts, PROP_INITIAL_ACK_DELAY, DEFAULT_INITIAL_ACK_DELAY))
```

`Connection.setNextSendTime(long)` clamps future deadlines:

```text
long max = _context.clock().now() + _options.getSendAckDelay();
```

## Poll cadence / horizon

```text
50 ms polling interval
2,000 ms attribution horizon
maximum 40 polls
```

Polling is observational only; the existing 45-second outer response
window remains frozen.

## Timer logger configuration

The helper configures `net.i2p.util.SimpleTimer2` at DEBUG via the
public `LogManager.setLimits` API in `p237ConfigureStockObserver`
and re-applies the limit on every `REPORT_TIMER_STATS` call inside
`p246RefreshPeerMarker` (the lazy refresh) so any later `LogWriter`
clear by `rereadConfig()` is undone before the read. Console buffer
is shared with the retained Plan-237/245 acceptance (1024 entries).

## Exact-socket correlation proof

`REPORT_TIMER_STATS` joins each SimpleTimer2 lifecycle marker line
with the active Connection's peer substring via
`p246LineMatchesActiveSocket` and `p246CountBufferSubstringExactSocket`.
The peer substring is derived from `I2PSocket.getPeerDestination()
.toBase32()` (held in memory only, never written to durable
evidence). The helper tolerates stock Java's `getPeerDestination()`
returning `null` at the moment `accept()` returns (race with SYN
parsing) by lazy-fetching the peer on every report call. The first
report after the peer becomes available captures the marker; the
emitted boolean `peer_correlation_present` switches from `false`
to `true`.

## Three counted evidence directories

```text
target/interop/m6-java-evidence/p246-attempt-A
target/interop/m6-java-evidence/p246-attempt-B
target/interop/m6-java-evidence/p246-attempt-C
```

Per-attempt timer-stats summary (all three):

| Attempt | p234-terminal | p245-terminal | p246-terminal | p246-stage-a1 (peer, simple_timer) |
| --- | --- | --- | --- | --- |
| A | P234-C-STREAMING-DIRECTION-A-ESTABLISHED | P245-A-SCHEDULER-NOT-OBSERVED | P246-OBSERVABILITY-GAP | false, false |
| B | P234-C-STREAMING-DIRECTION-A-ESTABLISHED | P245-A-SCHEDULER-NOT-OBSERVED | P246-OBSERVABILITY-GAP | false, false |
| C | P234-C-STREAMING-DIRECTION-A-ESTABLISHED | P245-A-SCHEDULER-NOT-OBSERVED | P246-OBSERVABILITY-GAP | false, false |

A rare fourth run (counted outside the budget; cited for
reproducibility) reached the Plan-245 baseline gate:

| Attempt | p245-terminal | p246-terminal |
| --- | --- | --- |
| D (outside budget) | P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH | P246-OBSERVABILITY-GAP |

The Plan-246 attribution is two-stage:

1. `P246-Stage-A0`: bounded public LogManager enablement
   (`p237ConfigureStockObserver`) + lazy peer-b32 fetch
   (`p246RefreshPeerMarker`) + exact-socket correlation
   (`p246LineMatchesActiveSocket`).
2. `P246-Stage-A1`: pre/post `P246ResponseStats` snapshot +
   same-epoch deltas + rolling maximums over the polling horizon +
   classifier invocation + record
   `record_p246_classification`.

The Plan-245 Stage A.0 baseline gate is preserved: a
`P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH` is required before any
Plan-246 attribution may issue a deeper terminal. The Plan-246
contradiction guard (Plan 246 §17) is also preserved: any positive
Plan-246 lifecycle delta without `simple_timer_debug_enabled=true`
OR without `peer_correlation_present=true` returns
`P246-OBSERVABILITY-GAP`. None of the three counted attempts
produced a non-Gap terminal.

## First and subsequent reschedule delays per attempt

Per-attempt `p246-timer-stats-pre`/`-post` show zero scheduler
events observed:

```text
timer_scheduler_count=0
timer_running_count=0
timer_early_reschedule_count=0
timer_finished_count=0
connection_timer_first_schedule_timeout_ms=0
connection_timer_latest_schedule_timeout_ms=0
connection_timer_min_schedule_timeout_ms=0
connection_timer_max_schedule_timeout_ms=0
first_reschedule_delta_ms=0
latest_reschedule_delta_ms=0
min_reschedule_delta_ms=0
max_reschedule_delta_ms=0
connection_timer_first_run_elapsed_ms=0
```

The scheduler-reschedule branch delta derived from the Plan-245
post snapshot is `0` on each counted attempt
(`P245-A-SCHEDULER-NOT-OBSERVED`), so the Plan-245 baseline gate is
not satisfied on A/B/C and Plan 246 correctly returns
`P246-OBSERVABILITY-GAP`. On attempt D (outside the three-attempt
budget) the Plan-245 baseline gate was satisfied
(`P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH`), but the Plan-246
contradiction guard still fired because
`peer_correlation_present=false` and `simple_timer_debug_enabled=
false` in the helper-side `getLimits()` snapshot.

## Timer schedule/run/early-reschedule/finish counts

All four counters are zero on the three counted attempts. The
contradiction guard fires before any deeper attribution. The
Plan-245-attributed first `SchedulerReceived.reschedule` (attempt D
only) does not produce any Plan-246 lifecycle counter increment
because the helper-side SimpleTimer2 log capture did not fire
within the polling window.

## Rolling maxima vs final snapshots

Per-attempt rolling-maximum tracker (driver-side) is also zero on
all three counted attempts:

```text
rolling_max_scheduler=0
rolling_max_running=0
rolling_max_finished=0
rolling_max_early_reschedule=0
```

Therefore `p246_observer_evicted(rolling_max, final_snapshot)`
returns `false` on all three. The `observer_evicted=false` flag is
recorded. No `p246-classification-eviction` row is emitted because
no eviction is observable.

## Clock-offset samples

`context_clock_minus_system_ms` is recorded on every report. All
samples on the three counted attempts are `0` because
`p246ClockSkewMs()` returns `0L` when the helper can read both the
`I2PAppContext.clock().now()` and the system clock. The helper's
clock skew is bounded by the same tick used by `setNextSendTime`
in stock Java.

## Sanitized state if scheduler changed

No scheduler-change was observed because no SimpleTimer2 lifecycle
event was observed in the polling window. Plan 246 §11.4's
`P246-C-CONNECTION-EVENT-RAN-SCHEDULER-CHANGED` arm is not reached.

## Downstream Plan-245 facts if send branch reached

No send branch was reached on any of the three counted attempts
(`p245_scheduler_send_branch_delta=0` in every Plan-245 post
snapshot). The Plan-245 downstream chain is therefore not resumed
by Plan 246.

## Exact terminal per attempt

| Attempt | p246-terminal |
| --- | --- |
| A | P246-OBSERVABILITY-GAP |
| B | P246-OBSERVABILITY-GAP |
| C | P246-OBSERVABILITY-GAP |
| D (outside budget) | P246-OBSERVABILITY-GAP |

The three-attempt budget is consistent. The deepest Plan-246
attribution terminal across the budget is `P246-OBSERVABILITY-GAP`.
Plan 246 §17's contradiction guard is the active arm: positive
lifecycle deltas without proven SimpleTimer2 DEBUG and without
proven exact-socket peer correlation cannot be attributed.

## No-tuning / no-timing-change proof

- `i2pr.streaming.initialAckDelay` is not set on the frozen helper;
  the canonical value remains `(0, 500]` (default `500`).
- The 45-second outer response window remains frozen; Plan-246
  polling is observational only.
- No Java source/jar patch was committed.
- No between-attempt tuning was applied.
- No production i2pr change was committed.

## No-Java-patch proof

The helper source diff is observation-only:

- `p237ConfigureStockObserver` adds one DEBUG level for
  `net.i2p.util.SimpleTimer2` (alongside the retained
  Plan-237/245 limits);
- `REPORT_TIMER_STATS` adds a new control command;
- `acceptOne` captures `I2PSocket.getPeerDestination().toBase32()`
  via the public API;
- `p246RefreshPeerMarker`, `p246LineMatchesActiveSocket`,
  `p246CountBufferSubstringExactSocket`, `p246ExtractTimeoutMs`,
  `p246ExtractTrailingLong`, `p246CollectTimeouts`,
  `p246CollectRescheduleDeltas`, `p246CollectFinishedElapsed`,
  `p246ClockSkewMs`, `p246SimpleTimerDebugEnabled` are new bounded
  counters/parsers; no private-state reflection, no JVM-arg change,
  no library upgrade, no protocol change.

## No-production-change proof

`crates/i2pr-{daemon,client,tunnel,runtime,api,service-tunnels}/src`
are unchanged. The Plan-246 diff is contained in
`crates/i2pr-daemon/tests/java_tunnel_external.rs` (test code only)
and the helper-side Java observation files.

## Verification floor

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --doc
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo deny check advisories bans sources
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p246_ -- --test-threads=1
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh target/interop/m6-java-sources/i2p.i2p-9134f808337b401e8e53c73734c81fab04280c9d /tmp/p246-source-lock.tsv
```

All checks green.

Focused p246 unit rows: 26 passed (`p246_*`).
Full java_tunnel_external suite: 394 passed, 5 ignored
(retained `#[ignore]`-gated external-environment tests).
Source-lock TSV: 60 rows, md5 `64b7661c76f433474000e16c6e97d9a7`.

## Plan-201 / Plan-204 unblock audit

Plan 201 currently reads:
`blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan246`.

Plan 204 currently reads:
`blocked-on-m6-java-second-family-closure-pending-plan246`.

Both plans depend on the Plan-246 attribution reaching a deeper
non-Gap terminal that proves or refutes a Java defect. The Plan-246
closure is `observability-gap-observed-...`; therefore neither
Plan 201 nor Plan 204 can be unblocked yet. Plan 201 and Plan 204
remain blocked and are re-pointed at the next successor
(`plan246-successor-observation-gap-corrective`) which must either
(a) extend the polling horizon past the Plan-245 second-scheduler
reschedule timing so the SimpleTimer2 lifecycle markers are
observable within the attribution window, or (b) prove that the
Plan-245 reschedule is in fact a Java defect (e.g.
`SchedulerReceived.reschedule()` emits the reschedule but the
matching `SimpleTimer2.addEvent` does not enqueue the corresponding
event). Until either branch is sourced, Plan 246's
`P246-OBSERVABILITY-GAP` is the bound.

Plan 205 retained off-path; no Plan-201/Plan-204 unblock is
authorized.

## Successor authorization

Only the exact Plan-246 result authorizes a successor:

- the contradiction guard's three failure modes are now documented
  and reproducible: missing `peer_correlation_present`,
  missing `simple_timer_debug_enabled`, missing
  `p245_scheduler_reschedule_branch_delta`;
- the polling horizon and cadence are now first-class recorded
  facts (`p246-poll-cadence`); a successor may either extend the
  horizon past 2 s or move the polling start point earlier so the
  lifecycle markers become observable;
- the Plan-246 evidence checker invariants (§35) are now
  enforceable on every future closure record;
- no Java defect is proven; no production i2pr change is
  authorized; no ACK-delay override is applied; no topology or
  publication change is applied.
