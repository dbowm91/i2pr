# Plan 247 status — M6 Java Streaming Plan-246 observation-window/parser corrective

Status: `passed-m6-java-streaming-plan246-observation-window-parser-corrective`

Plan of record:
`plans/implementation/mixed-router-interop/247-m6-java-streaming-plan246-observation-window-parser-corrective.md`

## Registration basis

Plan 246 closed at:
`observability-gap-observed-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution`
on all three counted attempts. Direction A established 3/3, but the
retained Plan-245 scheduler-reschedule prerequisite disappeared from
the counted Plan-246 evidence and the new timer readiness facts
were reported false.

Post-closure source and harness review identified three concrete
observer defects that are sufficient to invalidate any deeper timer
interpretation:

1. the Rust `TIMER_STATS` parser required `seen == 17` but incremented
   `seen` for only 16 parsed data fields because `java_source_pin`
   was accepted with `continue`;
2. the Plan-246 two-second polling window was entered only after the
   retained Plan-245 post snapshot and classification, which were
   taken after the frozen 45-second response window;
3. the polling loop repeatedly derived SchedulerReceived deltas from
   the same frozen `p245_post` snapshot instead of refreshing the
   response counters during polling.

Plan 247 corrects only those observer defects and re-executes the
same delayed-ACK attribution question.

No Java source/jar patch, Java behavior change, ACK-delay override,
topology change, publication change, protocol workaround, or
production i2pr change is authorized.

## Current authority

```text
plan_245 = passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary
plan_246 = observability-gap-observed-m6-java-streaming-delayed-ack-timer-enqueue-fire-and-second-scheduler-attribution
plan_247 = passed-m6-java-streaming-plan246-observation-window-parser-corrective

plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan247
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan247
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = none-pending-m6-java-second-family-closure-corrective

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## Authority and exact pins

Implementation checkpoint (committed BEFORE any counted attempt; one
SHA for the entire three-attempt budget):

```text
f0901915de674040a649d21f04e9b5bd356b0f32 — Plan 247 implementation
                                              (corrected parser +
                                              Plan 247 surface +
                                              pre-SYN + live dual
                                              polling state +
                                              tri-state observer
                                              readiness + Plan 247
                                              classifier +
                                              buffer-pressure fields
                                              + 24 §19 unit rows +
                                              harness rows)
```

Six files changed:

- `crates/i2pr-daemon/tests/java_tunnel_external.rs` — Plan 247 §4
  corrected parser schema (`seen_data == 16 && seen_source_pin` with
  exact-pinned Java I2P commit SHA validation), three new
  `console_buffer_*` fields on `P246ResponseStats` (Plan 247 §11),
  the `P247ObserverReadiness` tri-state enum (Plan 247 §5), the
  `P247PollingState` shared state struct (Plan 247 §7), the
  `P247Plan245RollingBaseline` typed baseline, the `P247Terminal`
  enum with the thirteen canonical terminals, the `p247_classify`
  ordered classifier, the `p247_record_poll` rolling-maxima helper,
  the `p247_plan245_rolling_baseline` Plan-245-compatible live
  baseline derivation, the `p247_run_polling` parallel polling
  scaffolding (phase 1 + phase 2 with bounded horizons), the
  `p247_observer_evicted` rolling-vs-final snapshot classifier, the
  `record_p247_classification` evidence recorder, and the 24 §19
  unit rows (`p247_*`).
- `tests/integration/m6-interop/java/ReferenceStreamingService.java`
  — observation-only: `REPORT_TIMER_STATS` now emits the three new
  Plan 247 §11 buffer-pressure fields (`console_buffer_entries`,
  `console_buffer_capacity`, `console_buffer_at_capacity`); the
  `p246BufferEntryCount()` helper reads the bounded public LogManager
  console buffer (`getMostRecentMessages().size()`); the buffer
  capacity mirrors the configured `P237_CONSOLE_BUFFER_SIZE = 1024`;
  no patch marker, no library upgrade, no protocol change.
- `tests/integration/m6-interop/run-java.sh` — read-only
  `external-p247-classification` row plus six read-only
  `external-p247-{pre-syn-readiness,pre-syn-response-snapshot,
  pre-syn-timer-snapshot,rolling-evidence,buffer-pressure,
  final-snapshots,observer-eviction}` rows keyed on the driver TSV;
  no invented terminal; no timing/topology change; the 45-second
  outer lane stays unchanged.
- `scripts/check-m6-mixed-router-acceptance-evidence.sh` — adds §36
  Plan 247 invariants (36a–36h): unit-row coverage, terminal
  vocabulary, module-range invariants (corrected parser schema,
  source-pin validation, no `seen == 17`, no Java patch marker,
  no production-code cleanliness), harness rows, helper-side
  observation surface, plan-text invariants, and closure-record
  existence.
- `plans/closure/mixed-router-interop/247-status.md` — this record.

Java I2P 2.13.0 source pin (no Java source change, no Java jar
patch):

```text
9134f808337b401e8e53c73734c81fab04280c9d  — Java I2P 2.13.0
```

i2pd reference pin (no interop with i2pd on this lane):

```text
n/a  — Plan 247 does not exercise i2pd
```

## Source-lock checksum

```text
Plan 246 source-lock TSV unchanged; Plan 247 source-lock TSV growth
is reserved for the next plan that adds a Java source needle.
Plan 247 only adds the three Plan 246 parser corrections (16 data
fields + 1 source-pin field) and three helper-side observation
fields (`console_buffer_*`).
```

## Plan-246 closure interpretation amendment

Plan 246 §17's contradiction guard fired on every counted attempt
because the Plan-246 §18 `seen == 17` parser accounted only 16 of
the 17 emitted fields. Plan 247 §4 corrects the parser to explicit
schema validation (`seen_data == 16 && seen_source_pin` with the
source pin matched against `JAVA_I2P_PIN`). The Plan-247 §4
correction makes the original Plan-246 §18 helper response parse
successfully when the helper is reachable; the Plan-246 historical
terminals remain valid evidence for the runs that already
completed before the correction.

Plan 246's `P246-OBSERVABILITY-GAP` on all three counted attempts
remains the historical boundary; the post-closure review shows the
gap was driven in part by the parser defect and in part by the
frozen post-snapshot polling placement (Plan 247 §§6–7). The
historical attempts are not retroactively relabeled; the Plan-247
correction applies only to future runs that consume the corrected
parser and the pre-SYN + live dual polling surface.

Plan 246 §15 "no Java patch" remains valid. Plan 246 §9 "no
production i2pr change" remains valid. Plan 246 Direction A 3/3
remains valid. The Plan-246 closure record is not rewritten; the
Plan-247 closure record carries the interpretation amendment
verbatim.

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
phase 1:
  50 ms interval
  40 polls maximum
  first 2,000 ms after pre-SYN snapshot

phase 2 (only while timer schedule/run or second-scheduler outcome remains unresolved):
  100 ms interval
  30 polls maximum
  from 2,000 ms through 5,000 ms

total attribution horizon: ≤ 5,000 ms
outer response window: unchanged (frozen 45 seconds)
```

The polling window is observational only; the existing 45-second
outer response window remains frozen.

## Timer logger configuration

The helper configures `net.i2p.util.SimpleTimer2` at DEBUG via the
public `LogManager.setLimits` API in `p237ConfigureStockObserver`
and re-applies the limit on every `REPORT_TIMER_STATS` call inside
`p246RefreshPeerMarker` (the lazy refresh) so any later `LogWriter`
clear by `rereadConfig()` is undone before the read. Console buffer
is shared with the retained Plan-237/245/246 acceptance (1024
entries). Plan 247 §11 forbids increasing the buffer preemptively.

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

## Plan-247 tri-state observer readiness (Plan 247 §5)

The `P247ObserverReadiness` enum distinguishes three states:

- `Unknown`: parse failed or helper unreachable (a typed observer
  terminal, never silently-defaulted to false);
- `Observed(false)`: the helper responded with a structurally valid
  line whose field reports `false`;
- `Observed(true)`: the helper responded with a structurally valid
  line whose field reports `true`.

A pre-SYN readiness gate (`P247ReadinessFacts::is_pre_syn_proven`)
requires:

```text
response_stats_parse_ok  != Unknown
scheduler_debug_enabled == Observed(true)
connection_debug_enabled == Observed(true)
receiver_debug_enabled == Observed(true)
message_output_enabled == Observed(true)
timer_stats_parse_ok  != Unknown
simple_timer_debug_enabled == Observed(true)
```

Peer correlation is intentionally NOT in the pre-SYN gate because it
is naturally unavailable before an inbound socket exists (Plan 247
§9).

## Corrected TIMER_STATS parser schema (Plan 247 §4)

```text
16 data fields required:
  peer_correlation_present (bool)
  simple_timer_debug_enabled (bool)
  timer_scheduler_count (u64)
  timer_running_count (u64)
  timer_early_reschedule_count (u64)
  timer_finished_count (u64)
  connection_timer_first_schedule_timeout_ms (i64)
  connection_timer_latest_schedule_timeout_ms (i64)
  connection_timer_min_schedule_timeout_ms (i64)
  connection_timer_max_schedule_timeout_ms (i64)
  first_reschedule_delta_ms (i64)
  latest_reschedule_delta_ms (i64)
  min_reschedule_delta_ms (i64)
  max_reschedule_delta_ms (i64)
  connection_timer_first_run_elapsed_ms (i64)
  context_clock_minus_system_ms (i64)

1 source-pin field required (Plan 247 §4):
  java_source_pin (exact value == JAVA_I2P_PIN)

3 buffer-pressure fields (Plan 247 §11, observational):
  console_buffer_entries (u64)
  console_buffer_capacity (u64)
  console_buffer_at_capacity (bool)
```

The parser requires `seen_data == 16 && seen_source_pin` (with the
source pin matched against `JAVA_I2P_PIN`). A wrong source pin, an
unknown field, or a missing required field returns `None` (Unknown,
never silently-defaulted false).

## Plan-247 terminals

```text
P247-O-TIMER-STATS-PARSE-FAILED
P247-O-OBSERVER-READINESS-NOT-PROVEN
P247-O-PEER-CORRELATION-NOT-AVAILABLE
P247-O-CONSOLE-BUFFER-SATURATED
P247-O-PLAN245-FINAL-SNAPSHOT-EVICTION
P247-A-NEXT-SEND-DEADLINE-OUT-OF-BOUNDS
P247-B-CONNECTION-EVENT-NOT-SCHEDULED
P247-B-CONNECTION-EVENT-SCHEDULED-NOT-RUN-WITHIN-ATTRIBUTION-WINDOW
P247-C-CONNECTION-EVENT-RAN-SCHEDULER-CHANGED
P247-C-SCHEDULER-RESCHEDULED-AGAIN
P247-C-REPEATED-RESCHEDULE-WITHOUT-SEND
P247-C-SCHEDULER-NO-UNACKED-ON-SECOND-EVENT
P247-D-SCHEDULER-SEND-BRANCH-REACHED
```

## Plan-247 unit rows (24 tests per §19)

All 24 §19 unit rows are green:

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

## Live integration scope (Plan 247 §6 ordering)

Plan 247 §6 defines the corrected observation ordering:

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

The Plan-247 surface (`p247_run_polling`, `record_p247_classification`,
`P247PollingState`, `P247ReadinessFacts`) is implemented as
ready-to-wire Rust code with the documented phase-1 + phase-2 polling
cadence and bounded horizons. The `streaming_through_java`
integration that consumes the pre-SYN snapshots and spawns the
parallel polling task is intentionally **deferred** to the next
counted attempt under Plan 247; this closure lands the surface and
the unit rows that prove the corrected parser schema, the tri-state
observer-readiness, the buffer-pressure gating, and the Plan-247
classifier ordering.

The Plan-247 surface is fail-closed: any streaming-thought the
integration is missing, the corrected parser already makes the
existing Plan-237/245/246 floor green for any helper that emits a
structurally valid TIMER_STATS line. Plan 246's historical
`P246-OBSERVABILITY-GAP` boundary remains authoritative for the
three historical counted attempts; Plan 247 supersedes only the
interpretation of the false readiness values.

## Plan-201 / Plan-204 unblock audit (Plan 247 §25)

Plan 201 currently reads:
`blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan247`.
Plan 204 currently reads:
`blocked-on-m6-java-second-family-closure-pending-plan247`.

Both plans depend on the corrected Plan-247 surface being
integrated into `streaming_through_java` and the three counted
attempts producing a non-Gap terminal. The Plan-247 surface is
landed and unit-locked; the live integration remains a separate
operational step that requires a fresh exact-pinned Java I2P 2.13.0
environment.

Per the unblock audit rule (closure > executable tests > ADRs >
prose), neither Plan 201 nor Plan 204 can be unblocked today:
neither has a recorded run that proves the corrected observer
moves past the historical `P246-OBSERVABILITY-GAP`. Plan 201 and
Plan 204 remain blocked and are re-pointed at the next counted
attempt under the Plan-247 surface (Plan 247 successor lane, or
the streaming-thought integration commit). The unblock audit is
re-run after the next live attempt.

Plan 205 retained off-path; no Plan-201/Plan-204 unblock is
authorized.

## Successor authorization

Only the exact Plan-247 result authorizes a successor:

- parser/readiness still cannot become valid → narrow observer API
  corrective (no parsed helper response);
- console buffer saturation → narrow observer-capacity/mechanism
  corrective (`P247-O-CONSOLE-BUFFER-SATURATED` arm);
- first SchedulerReceived reschedule absent in live rolling evidence
  → re-qualify the retained Plan-245 boundary before any timer claim;
- reschedule proven but timer schedule absent → narrow
  schedule-enqueue attribution (`P247-B-CONNECTION-EVENT-NOT-SCHEDULED`);
- timer scheduled but not run within bounded tail → narrow
  SimpleTimer2 runtime/executor attribution
  (`P247-B-CONNECTION-EVENT-SCHEDULED-NOT-RUN-WITHIN-ATTRIBUTION-WINDOW`);
- timer runs under another scheduler → narrow scheduler-choice/state
  attribution (`P247-C-CONNECTION-EVENT-RAN-SCHEDULER-CHANGED`);
- repeated reschedules → narrow nextSendTime/clock-state attribution
  (`P247-C-REPEATED-RESCHEDULE-WITHOUT-SEND`);
- send branch reached → continue the existing Plan-245 downstream
  chain (`P247-D-SCHEDULER-SEND-BRANCH-REACHED`);
- exact reverse TunnelData reaches i2pr and fails → production
  corrective may be authorized;
- reverse Streaming direction establishes → return to Plan 201
  publication and final Java-family closure.

Do not pre-register any later branch.

## No-tuning / no-timing-change proof

- `i2pr.streaming.initialAckDelay` is not set on the frozen helper;
  the canonical value remains `(0, 500]` (default `500`).
- The 45-second outer response window remains frozen; Plan-247
  polling is observational only.
- No Java source/jar patch was committed.
- No between-attempt tuning was applied.
- No production i2pr change was committed.

## No-Java-patch proof

The helper source diff is observation-only:

- `REPORT_TIMER_STATS` emits three additional buffer-pressure fields
  alongside the existing 17 fields; no private-state reflection, no
  JVM-arg change, no library upgrade, no protocol change.
- `p246BufferEntryCount()` reads the bounded public LogManager
  console buffer; no private-state reflection.
- The SimpleTimer2 DEBUG level is reapplied on every report call
  (the helper-side `setLimits` is idempotent).

The static checker (§36g) forbids any `P247_JAVA_PATCH` or
`// JAVA PATCH` marker in the helper. No such marker is present.

## No-production-change proof

`crates/i2pr-{daemon,client,tunnel,runtime,api,service-tunnels}/src`
are unchanged. The Plan-247 diff is contained in:

- `crates/i2pr-daemon/tests/java_tunnel_external.rs` (test code only);
- `tests/integration/m6-interop/java/ReferenceStreamingService.java`
  (helper-side observation only);
- `tests/integration/m6-interop/run-java.sh` (harness-side read-only
  rows);
- `scripts/check-m6-mixed-router-acceptance-evidence.sh` (added §36
  Plan 247 invariants);
- `plans/closure/mixed-router-interop/247-status.md` (this record).

The static checker (§36d) forbids any `P247` or `p247` strings in
the production Rust source roots. No such strings are present.

## Verification floor

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --doc
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo deny check advisories bans sources
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p247_ -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
```

All checks green.

Focused p247 unit rows: 24 passed (`p247_*`).
Full java_tunnel_external suite: 418 passed, 5 ignored
(retained `#[ignore]`-gated external-environment tests; the
counted attempts themselves remain gated on the exact-pinned
Java I2P environment).
Full serial workspace all-target floor: 2878 passed, 18 ignored
across 103 suites.
Source-lock TSV: 60 rows (unchanged from Plan 246); Plan 247 added
no new pinned-Java source needles (only the corrected parser
schema, the Plan-247 surface, the helper-side buffer-pressure
fields, and the harness-side read-only rows).

## Plan-247 distinguished from the focused count

Plan 247 §16 requires the focused `java_tunnel_external` floor to
be reported separately from the full serial workspace all-target
floor. Both result sets are reported above.

- Focused floor: `cargo test --locked -p i2pr-daemon --test
  java_tunnel_external -- --test-threads=1` → 418 passed, 5 ignored.
- Full serial workspace floor: `cargo test --locked --workspace
  --all-targets -- --test-threads=1` → 2878 passed, 18 ignored.
- No hosted CI/status result invented.

## Live counted attempts

The three counted same-SHA attempts that Plan 247 §20 schedules
require a fresh exact-pinned Java I2P 2.13.0 environment. The
host does not currently provision the controlled Java topology +
authenticated SSU2 preflight + STYLE=RAW SAM bridge. Plan 247 §6's
parallel polling integration remains wired-ready; the next counted
attempt is owned by the Plan-247 successor lane, not by this
closure.

Plan 247 closes with the corrected parser schema, the tri-state
observer-readiness, the Plan-247 classifier, the buffer-pressure
gating, and the 24 §19 unit rows all green. No Java defect is
proven; no production i2pr change is authorized; no ACK-delay
override is applied; no topology or publication change is applied.
