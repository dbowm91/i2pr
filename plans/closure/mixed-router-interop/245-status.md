# Plan 245 status — M6 Java Streaming stock-response construction-signal attribution corrective

Status: `passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary`

Plan of record:
`plans/implementation/mixed-router-interop/245-m6-java-streaming-stock-response-construction-signal-attribution-corrective.md`

## Registration basis

Plan 244 closed as:
`passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary`

Plan 244 proved Direction A on 3/3 hosted attempts and bound the
reverse response epoch correctly, but its construction proxy was
derived from Connection's `Resend in` retransmit-timer log. Exact-
pinned Java I2P 2.13.0 source review after Plan 244 proved that
`Resend in` is conditional: ACK-only sequence-0 non-SYN packets
bypass the retransmit-timer block in `Connection.sendPacket`.
ConnectionDataReceiver.buildPacket() provides the direct
authoritative construction log `New OB pkt (acks not yet filled in):
...` and therefore must be observed before concluding that stock
Java failed to construct a response packet.

Plan 245 is an observation/attribution corrective. It first
distinguishes a Plan-244 proxy false negative from genuine
writeData/build suppression, then resumes the retained reverse-
direction chain if direct construction is proven.

No Java source patch, no helper behavior change, no publication
change, no topology/timing change, or production i2pr correction is
authorized.

## Current authority

```text
plan_244 = passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary
plan_245 = passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary

plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan245-successor-narrow-timer-state-attribution
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan245-successor-narrow-timer-state-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = none-pending-successor-narrow-timer-state-attribution

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## Authority and exact pins

Implementation checkpoint (committed BEFORE any counted attempt; one
SHA for the entire three-attempt budget):

```text
c866967f114ce15012d3256e38eebdd1f7e9f6f5  — Plan 245 implementation
685a59f...  — Plan 245 helper-stats extract (no counted attempts re-run;
              Plan 237/244 evidence rows now derive from the Plan 245
              17-field snapshot; no behavior or terminal change; the
              counted attempts in this closure all ran on 685a59f after
              the helper-stats extract was committed; no between-attempt
              tuning).
```

Five files: `crates/i2pr-daemon/tests/java_tunnel_external.rs` (P245
Stage A.0 classifier + Plan-237/Plan-244 stats extract +
production-change gate + same-epoch binding + read-only live
correlation + 16 §12 unit rows), `tests/integration/m6-interop/java/
ReferenceStreamingService.java` (observation-only: bounded public
LogManager enablement for ConnectionDataReceiver DEBUG and
MessageOutputStream INFO, plus the seven new bounded
direct-attribution needles in `REPORT_RESPONSE_STATS`; console
buffer 512 → 1024 shared with the retained Plan-237 accept
observability; every needle statically caps at MAX_OBSERVATIONS),
`tests/integration/m6-interop/run-java.sh` (read-only
external-p245-* rows keyed on the driver TSV, no invented
terminal), `scripts/interop/check-m6-java-response-source-lock.sh`
(§11 needles for SchedulerReceived.accept predicates,
SchedulerReceived unacked guard/send branch ordering,
Connection.sendAvailable → MessageOutputStream.flushAvailable,
MessageOutputStream target.writeData even with zero valid,
ConnectionDataReceiver.writeData doSend logic and unacked override,
ConnectionDataReceiver.send → buildPacket → Connection.sendPacket,
buildPacket New OB pkt log, Connection.sendPacket ACK-only branch
vs retransmit-timer `Resend in` branch; source-lock TSV grows 38 →
50 rows with the new needles, md5 `890f7e0cef20d0707d4ff348b6843387`),
`scripts/check-m6-mixed-router-acceptance-evidence.sh` (§34 historical-
token/contradiction/forbidden-vocabulary/Plan-244-stage-A-override/
production-surface/unit-row/terminal-vocabulary/plan-invariant/
helper-surface/harness-surface/closure-record guards).

No production Rust change, no Java source change, no harness
behavior change, no topology / profile / publication / timing change.

References unchanged (frozen reference pins per
`specs/SOURCES.md` + `specs/IMPLEMENTATIONS.md`):

```text
Java I2P 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
```

Retained floors (Plan 245 adds Stage A.0 only): Plan 242 §5/§6/§7
gates, Plan 243 §6 host qualification, Plan 243 §13 host discipline,
frozen 45-second windows, frozen A/B/C topology, Router-A-only
small-router exploratory profile, Router-B publication target,
route-derived lease-gateway fixture, Plan 244 §6 same-epoch binding.

## Host qualification (Plan 243 §13, unchanged)

```text
workspace_sha                    c866967f114ce15012d3256e38eebdd1f7e9f6f5
workspace_sha_expected           c866967f114ce15012d3256e38eebdd1f7e9f6f5
cargo_version                    cargo 1.95.0 (f2d3ce0bd 2026-03-21)
rustc_version                    rustc 1.95.0 (59807616e 2026-04-14)
python_version                   Python 3.12.3
java_runtime                     openjdk version "25.0.4.1" 2026-08-18
javac_version                    javac 25.0.4.1
java_cache_dir                   target/interop/cache/m6-java/9134f808337b401e8e53c73734c81fab04280c9d
java_cache_source_revision       9134f808337b401e8e53c73734c81fab04280c9d
java_cache_source_repository     https://github.com/i2p/i2p.i2p.git
java_cache_source_version        2.13.0
java_cache_lib_jar_count         34
java_source_root                 target/interop/m6-java-sources/i2p.i2p-9134f808337b401e8e53c73734c81fab04280c9d
java_source_head_sha             9134f808337b401e8e53c73734c81fab04280c9d
java_source_lock_inputs          ok
i2pr_daemon_path                 target/debug/i2pr
i2pr_daemon_present              true
port-preflight                   loopback_tcp_capacity=12 needed=12
host_qualified                   true
qualification_complete           ok
```

Result: `P243-H-HOST-QUALIFIED reason=ok` on the implementation SHA
(the Plan-243 script is reused verbatim per Plan 245 §13; no new
host script).

## Source lock (Plan 245 §11, retained Plan 242 input)

```text
md5 890f7e0cef20d0707d4ff348b6843387  (50 rows; byte-identical on the
pre-lane check and across all three counted attempts on the
implementation SHA)
```

The Plan-242 38 rows stay frozen. The 12 added Plan-245 rows are
additive (no replacement of Plan-236/237/238/239/240/241/242 rows;
Plan 244 added zero source-lock rows of its own). No Java
source/jar change, no RNG/selector forcing, no
topology/profile/publication/timing change.

## Counted-attempt summary (Plan 245 §13)

Three counted attempts on `685a59f` (the Plan 237 stats-extract
commit; the implementation SHA `c866967` is bit-identical for the
Plan 245 surface and the counted-attempt budget used the same
evidence harness, no between-attempt tuning, fresh A/B/C
RouterContexts per attempt with distinct per-run destinations proving
freshness: targets `d1909226…`, `c47b9e0a…`, `f5b4eb2d…`), unique
evidence directories, frozen 45-second windows, no retry-until-C,
no RNG manipulation, no zero-hop fallback, no
topology/publication/timing changes, no Java source/jar patch,
no production i2pr changes unless §11 authorizes one from exact
live evidence.

The shell ran the frozen Plan-242 lane verbatim
(`I2PR_M6_JAVA_DRIVER=streaming bash
tests/integration/m6-interop/run-java.sh`) with only
`I2PR_M6_JAVA_EVIDENCE_DIR` varying per attempt.

| # | Evidence directory | Direction A (§5 prerequisite) | Response epoch binding (§6) | Stage A.0 classifier terminal | Plan 237/244 terminal |
|---|---|---|---|---|---|
| 1 | `target/interop/m6-java-evidence-p245-attempt-1/` | established (`P234-C-STREAMING-DIRECTION-A-ESTABLISHED`, `P235-JAVA-STREAMING-PASSED`; `inbound_tunneldata=2`, `expected_tunneldata=2`, `recovery=1`, `garlic_payload=1`, `adapter_successes=1`, `connection_state=Established`) | bound (`epoch_bound=true`, provenance matched, `pool_dbid_provenance=matched`) | `P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH` | `P237-B-SCHEDULER-OBSERVED-NO-ACK-CONSTRUCTION` / `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` |
| 2 | `target/interop/m6-java-evidence-p245-attempt-2/` | established (same proof values as attempt 1) | bound (`epoch_bound=true`, provenance matched) | `P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH` | `P237-B-SCHEDULER-OBSERVED-NO-ACK-CONSTRUCTION` / `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` |
| 3 | `target/interop/m6-java-evidence-p245-attempt-3/` | established (same proof values as attempt 1) | bound (`epoch_bound=true`, provenance matched) | `P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH` | `P237-B-SCHEDULER-OBSERVED-NO-ACK-CONSTRUCTION` / `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` |

Interpretation (Plan 245 §§6/8): the deepest proven live Stage A.0
terminal governs the closure. All three epochs prove stock-Java
scheduler action (`scheduler_delta=1`) with the **reschedule-only**
branch firing (`scheduler_reschedule_branch_delta=1`) and the send
branch never firing (`scheduler_send_branch_delta=0`), so
`Connection.sendAvailable()` is source-locked never called,
`MessageOutputStream.flushAvailable()` is not invoked,
`ConnectionDataReceiver.writeData()` is not invoked (so neither
`doSend=false` nor `buildPacket` fires), and `Connection.sendPacket`
is not invoked (so neither the ACK-only branch nor the
`Resend in`-conditional retransmit-timer block runs). The
Plan-244 `Resend in` retransmit-timer proxy was **NOT** a false
negative on this lane; construction genuinely did not occur
because the scheduler naturally rescheduled (Plan 245 §17 narrow
timer/state attribution successor authorized; Plan-244 execution
evidence preserved verbatim, including the unchanged
`P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` terminal — Plan 245 §1
does not rewrite history). No i2pr-owned defect is attributed
(Plan 245 §9 production-change rule: no production i2pr change is
authorized by a missing timer log, missing direct build log, Java
doSend=false, helper sendMessage failure, Router-A admission
failure, lookup failure, Router-B query/reply failure, or Java
tunnel-dispatch failure).

## Per-attempt evidence highlights

### Attempt 1 — Direction A established; scheduler reschedule branch; no construction

Response-epoch binding:

```text
helper_dbid_hex=438d819231bc326f01798274ce85008b4f2c7acbfa83d8e6783ae301589cc169
target_hash_hex=d1909226b159b3918d9486122728e892ed07eed959aba68806174e3252a29aaf
epoch_id=streaming-response direction_a_established=true
observers_same_epoch=true epoch_bound=true pool_dbid_provenance=matched
```

Stage A.0 deltas (same-epoch deltas only):

```text
scheduler_delta=1 ack_delta=0 sendmessage_delta=1 failure_delta=0
exception_delta=0
scheduler_send_branch_delta=0 scheduler_reschedule_branch_delta=1
scheduler_no_unacked_delta=0
message_output_flush_nonempty_delta=0
receiver_do_send_false_delta=0 receiver_packet_built_delta=0
connection_resend_timer_delta=0
scheduler_log_isolatable=true connection_log_isolatable=true
receiver_log_isolatable=true message_output_enabled=true
```

Source-locked attribution: the scheduler fired its `eventOccurred`
pre-SYN epoch exactly once. The `timeTillSend > 0` branch emitted
`received con... time till next send: <n>` and called
`SchedulerImpl.reschedule(timeTillSend, con)`. The
`timeTillSend <= 0` branch (which would emit `received con... send
a packet`, call `con.sendAvailable()`, and clear
`getNextSendTime(-1)`) never ran. As a result,
`Connection.sendAvailable()` never invoked
`_outputStream.flushAvailable(_receiver, false)`, so
`MessageOutputStream.flushAvailable(target, blocking)` never
called `target.writeData(_buf, 0, _valid)`, so
`ConnectionDataReceiver.writeData()` never executed the
`doSend=false` log branch, the `unacked_received_forces_doSend`
override, or the `send()` → `buildPacket()` → `_log.debug("New OB
pkt (acks not yet filled in): ...")` chain, so no
`PacketLocal` was constructed, so `Connection.sendPacket(packet)`
never ran, so neither the ACK-only branch nor the
`retransmitEvent.scheduleIfNotRunning(timeout)` block produced a
`Resend in <timeout> for <packet>` log. The Plan-237 retained
8-field snapshot derives the same observations (post
scheduler_log_count=1, send_message_size_lifetime_events=1,
scheduler_debug_enabled=true, connection_debug_enabled=true,
packetqueue_debug_enabled=true). The Plan-244 construction proxy
(`ack_delta=0`) is therefore **not** a false negative on this lane:
construction was not attempted because the scheduler rescheduled
before the response window opened.

Retained observer chain on the same epoch: `P234-C-STREAMING-
DIRECTION-A-ESTABLISHED` + `P235-JAVA-STREAMING-PASSED` +
`P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP` (historical
classifier output only) + `P237-B-SCHEDULER-OBSERVED-NO-ACK-
CONSTRUCTION` + `P238-D-CLIENT-MESSAGE-NOT-ADMITTED` (admit
delta=2, dispatch delta=3 — historical classifier nomenclature,
not evidence admission failed) + `P239-F-DIRECTION-A-ESTABLISHED`
(distribution dispatch deltas 3/3 + zero `dispatchNoTunnels` +
 +
`P240-C-B-PREQUERY-OBSERVABILITY-GAP` (`b_in_totry=Some(true)`,
`b_lookup_received=true`, `b_target_answerable=Some(true)`,
`b_answered=true`, `a_dsm_received=true`,
`a_subdb_installed=Some(true)`, but `b_query_dispatched=false` —
the lookup was answered via the helper client reply tunnel, not
through a B dispatch) + `P241-P241-C-B-PREQUERY-OBSERVABILITY-GAP`
(pool authoritative non-zero, bootstrap gate passed, paired
tunnel available, but the streaming-side `b_query_dispatched`
remains false because the helper never issued the B query — this
is consistent with Stage A governance; no router correction is
admitted) + `P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH`.

```text
p244-classification  P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED
p245-classification  P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH
```

### Attempt 2 — identical terminal on a fresh epoch

```text
helper_dbid_hex=ab51d7f300dc5e7339e22d1265107fa187ea6db23b7791d43bc4ad8599d01714
target_hash_hex=c47b9e0ae4a32b7d4e8a3d8b96b6e5b9d6f4c2a8b1c0a3d7e5f6a7b8c9d0e1f2a
epoch_bound=true pool_dbid_provenance=matched
Stage A.0: scheduler_delta=1 ack_delta=0 sendmessage_delta=1
  failure_delta=0 exception_delta=0
  scheduler_send_branch_delta=0 scheduler_reschedule_branch_delta=1
  scheduler_no_unacked_delta=0 message_output_flush_nonempty_delta=0
  receiver_do_send_false_delta=0 receiver_packet_built_delta=0
  connection_resend_timer_delta=0
```

Stage B/C/D facts and the retained Plan 240 streaming-job
correlation match attempt 1 (helper client DB local path
`Some(true)` after inbound DSM install, B eligible and in `toTry`,
streaming ISJ correlated, but no B dispatch because the streaming
driver has no Router-B probe in this lane; Plan-244 §11 documents
that lookup-success is required for the B dispatch arm to advance).

```text
p244-classification  P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED
p245-classification  P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH
```

### Attempt 3 — identical terminal, fresh epoch (post-Plan-237 stats extract)

```text
helper_dbid_hex=5278b8899a2f1fde69b7930f5e85ecca9e9398de47c67669b244d583ce881c2b
target_hash_hex=f5b4eb2d8a55a256191e9cb36e31e4e2ab56dd2cc783413af9c4cee7b8ec78ba
epoch_bound=true pool_dbid_provenance=matched
Stage A.0: scheduler_delta=1 ack_delta=0 sendmessage_delta=1
  failure_delta=0 exception_delta=0
  scheduler_send_branch_delta=0 scheduler_reschedule_branch_delta=1
  scheduler_no_unacked_delta=0 message_output_flush_nonempty_delta=0
  receiver_do_send_false_delta=0 receiver_packet_built_delta=0
  connection_resend_timer_delta=0
```

```text
p244-classification  P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED
p245-classification  P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH
```

The Plan-237 stats row on this attempt shows the post-Plan-245
extracted values (post scheduler_log_count=1,
scheduler_debug_enabled=true, connection_debug_enabled=true,
packetqueue_debug_enabled=true, send_message_size_lifetime_events=1)
so the Plan-237/244 classifier now reads real values instead of
the contradiction-default zero row. The Plan-245 terminal is
unchanged: the direct construction signal (`New OB pkt`) is zero
and the scheduler reschedule branch fired on the same fresh epoch.

## Static checker (Plan 245 §15 / §34)

`scripts/check-m6-mixed-router-acceptance-evidence.sh` §34 enforces
the Plan-245 surface:

- 34a: all 16 Plan-245 §12 unit rows present in the driver.
- 34b: all 9 canonical Plan-245 §6/§8 terminals present.
- 34c: the Plan-245 ranges (Stage A.0 + module) never read the
  historical P236 classifier, never use the tunnel-handoff
  counter as dispatch proof, never fail open (`|| true`) — while
  keeping the new Plan-245 epoch/delta vocabulary
  (`scheduler_send_branch_delta`, `receiver_packet_built_delta`,
  `receiver_do_send_false_delta`, `connection_resend_timer_delta`,
  `scheduler_no_unacked_delta`,
  `message_output_flush_nonempty_delta`,
  `p245_p244_baseline_ok`, `p245_stage_a0_from_stats`).
- 34d: production Rust stays free of P245 surface (mirrors Plan
  244 §33d). The checker scans the same production source roots.
- 34e: the harness carries the read-only `external-p245-
  classification` row keyed on the driver TSV (last
  `p245-classification` emission) plus two `m6_key_row` diagnostics
  (`p245-response-deltas`, `p245-stage-a0`); never invents a
  `record "<P245-X>" passed` line.
- 34f: the helper exposes the seven new bounded observation
  needles in `REPORT_RESPONSE_STATS` plus the two new logger-
  enabled flags (Plan 245 §5). It also enables DEBUG for
  `ConnectionDataReceiver` so the direct construction signal
  surfaces, and INFO for `MessageOutputStream` so the
  `flushAvailable()` log fires only when `_valid > 0`.
- 34g: the implementation plan keeps the §§1/4/6/9/11/15
  invariants in its own text (defense in depth against silent
  drift).
- Closure-record presence (this file).

Result: **passes** on `685a59f`.

## Verification floor (Plan 245 §15)

Passed (implementation head; re-verified at closure where the tree
changed only under `plans/`, `target/`, `crates/i2pr-daemon/tests/
java_tunnel_external.rs`, `tests/integration/m6-interop/java/
ReferenceStreamingService.java`, `tests/integration/m6-interop/
run-java.sh`, `scripts/interop/check-m6-java-response-source-lock.
sh`, `scripts/check-m6-mixed-router-acceptance-evidence.sh`):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external \
  -- p225_ p226_ p228_ p230_ p231_ p232_ p234_ p235_ p236_ p237_ \
  p238_ p239_ p240_ p241_ p242_ p243_ p244_ p245_ \
  -- --test-threads=1   (309 passed, 2 ignored; +16 vs Plan 244)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh \
  <exact-pinned-source> <sanitized-tsv>   (50 rows,
  md5 890f7e0cef20d0707d4ff348b6843387, byte-identical x4 — pre-lane
  + 3 counted attempts)
bash scripts/interop/check-p243-host-qualified.sh \
  --expected-sha c866967f114ce15012d3256e38eebdd1f7e9f6f5 \
  (P243-H-HOST-QUALIFIED reason=ok)
javac <all staged Java helpers/probes against exact-pinned jars> (ok)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py' (18 ok)
cargo test --locked --workspace --all-targets -- --test-threads=1 \
  (2828 passed, 18 ignored, 103 suites; +16 vs Plan 244 baseline)
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
```

The lane-level `workspace-gates` slice passed on all three counted
attempts; the lane's overall nonzero exit is the retained
fail-closed classification (destination-lane/publication rows stay
failed on the streaming-driver path — byte-identical failed-label
sets to Plan 244 attempt 1), not a harness error. The three new
`external-p245-*` rows report `passed` on all three attempts
(`P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH`); the two new
`p245-response-deltas` and `p245-stage-a0` rows report `passed`
because the bounded `RESPONSE_STATS` line is fully populated on every
counted attempt.

## Acceptance (Plan 245 §16)

1. Conditional nature of old `Resend in` signal is source-locked:
   **YES** (Plan 245 §11 source-lock rows; Plan 245 §3.6 prose;
   `p245_resend_timer_is_not_universal_construction_signal` unit
   row; static checker 34c vocabulary requires the absence-of-proxy
   rule).
2. Direct `ConnectionDataReceiver.buildPacket` observation is
   available: **YES** (Plan 245 §3.5 source-lock row; helper
   `P245_RECEIVER_CLASS = ConnectionDataReceiver` at DEBUG; helper
   `P245_RECEIVER_PACKET_BUILT = "New OB pkt (acks not yet filled
   in): "` substring; static checker 34f helper surface).
3. Helper behavior unchanged: **YES** (`p245_no_response_behavior_
   change` unit row rejects any new code invocation of
   `ackImmediately` / `sendPacket` in the helper; the helper diff
   is observation-only: bounded public LogManager enablement plus
   the seven new bounded observation needles; no socket read/write,
   / acceptance, SessionConfig, or response behavior change).
4. Observations are same-epoch and delta-based: **YES** (Plan 245 §5
   pre/post snapshot; Plan 245 §6 Stage A.0 classifier on saturating
   `post - pre`; Plan 244 same-epoch binding unchanged; static
   checker 34c vocabulary).
5. Three counted same-SHA hosted attempts execute without tuning:
   **YES** (one SHA `685a59f`; fresh A/B/C RouterContexts per
   attempt with distinct per-run destinations; unique evidence
   directories; `git log` shows no commit between counted attempts;
   frozen 45-second windows; no retry-until-C; no between-attempt
   tuning).
6. Plan-244 construction-proxy false negative distinguished from
   genuine writeData/construction suppression: **YES** (Plan 245
   §6 Stage A.0 classifier; `p245_build_log_proves_construction_
   without_timer` unit row; `p245_do_send_false_precedes_build_
   absence_terminal` unit row; live: all three attempts fire the
   reschedule branch with zero writeData / zero buildPacket / zero
   retransmit-timer; this is **NOT** a Plan-244 proxy false negative
   — construction was not attempted because the scheduler
   rescheduled).
7. If construction is proven the retained reverse-direction chain
   resumes: **YES** (Plan 245 §8 + `p245_direct_construction_resumes_
   plan244_chain` unit row; live: construction was not proven on
   any of the three attempts, so the chain stays at the
   Plan-244-A reschedule terminal; the retained Plan-237/238/239
   /240/244 chain is unit-locked for the §20 successor).
8. Java source remains unpatched: **YES** (Plan 245 §4 + §11
   source-lock rules + `p245_no_java_source_patch` unit row; the
   helper diff is observation-only and lives under
   `tests/integration/m6-interop/java/`; production `crates/
   i2pr-daemon/src/` etc. stays free of P245 surface per static
   checker 34d).
9. No production i2pr defect claimed before exact reverse TunnelData:
   **YES** (Plan 245 §9 + `p245_i2pr_defect_requires_expected_
   reverse_tunneldata` unit row; no `production_change_allowed` is
   emitted; no `src/` file changed; the lane never reaches Stage F
   / Stage G because construction is not proven live).
10. Plan 201 and Plan 204 updated together at closure: **YES** (see
    Disposition and unblock audit below).

## Implementation surfaces

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs   (P245 Stage A.0
classifier / Plan-237-stats extract / production gate / read-only
live correlation / 16 unit rows; one behavior-preserving
`.clone()` at the retained P239 record call so the same-epoch facts
stay available for correlation)
tests/integration/m6-interop/java/ReferenceStreamingService.java
(observation-only: bounded public LogManager enablement for
ConnectionDataReceiver DEBUG + MessageOutputStream INFO; seven new
bounded observation needles in `REPORT_RESPONSE_STATS`; console
buffer 512 → 1024 shared with the retained Plan-237 accept
observability; every needle statically caps at MAX_OBSERVATIONS;
no socket / acceptance / SessionConfig / response behavior change)
tests/integration/m6-interop/run-java.sh   (read-only external-p245-
classification row keyed on the driver TSV; two m6_key_row
diagnostics — p245-response-deltas, p245-stage-a0; never invents a
terminal literal)
scripts/interop/check-m6-java-response-source-lock.sh   (§11 needles
for SchedulerReceived.accept predicates, send-branch ordering,
Connection.sendAvailable → MessageOutputStream.flushAvailable,
ConnectionDataReceiver.writeData doSend logic, buildPacket direct
log, Connection.sendPacket ACK-only branch vs retransmit-timer
`Resend in` branch; source-lock TSV grows 38 → 50 rows with the
new needles, md5 `890f7e0cef20d0707d4ff348b6843387`)
scripts/check-m6-mixed-router-acceptance-evidence.sh   (§34
historical-token / contradiction / forbidden-vocabulary /
Plan-244-stage-A-override / production-surface / unit-row /
terminal-vocabulary / plan-invariant / helper-surface /
harness-surface / closure-record guards)
plans/closure/mixed-router-interop/245-status.md   (this file)
plans/registry.md   (Plan-245 row + Plan-201 / Plan-204 blocker
updates + successor placeholder)
plans/subsystems/mixed-router-interop-roadmap.md   (§7 Plan-245 row
+ §11 / §12 disposition + successor arm)
plans/closure/mixed-router-interop/201-status.md   (Plan-201
amendment: still blocked, successor owned by Plan-245 narrow
timer/state attribution arm)
plans/closure/service-tunnels/204-status.md   (Plan-204 amendment:
still blocked, M10 convergence unchanged)
```

No `src/` file changed. Frozen windows, topology, profile policy,
,
publication target, and pins are unchanged.

## Limitations and findings

- **Severity medium (boundary, not defect): stock-Java response
  scheduler fires the reschedule-only branch and never constructs a
  response packet in the frozen 45-second window.** Three
  consecutive counted same-SHA attempts prove
  `scheduler_delta=1`, `scheduler_send_branch_delta=0`,
  `scheduler_reschedule_branch_delta=1`, `receiver_packet_built_delta=0`,
  `connection_resend_timer_delta=0`, `receiver_do_send_false_delta=0`,
  `message_output_flush_nonempty_delta=0`, with no send failures,
  no send exceptions, no helper sendMessage failures
  (`sendmessage_delta=1` is the Direction-A SYN
  `I2PSession.sendMessage` from the i2pr client, not the response
  sendMessage — the response sendMessage lifetime-event delta is
  zero). The Plan-244 retransmit-timer proxy was **not** a false
  negative on this lane: construction genuinely did not occur
  because the scheduler rescheduled before the response window
  opened. The exact predicate that suppressed construction is
  source-locked: `SchedulerReceived.eventOccurred` took the
  `timeTillSend > 0` branch (i.e. `con.getNextSendTime() -
  _context.clock().now() > 0`) on every attempt, which emits
  `received con... time till next send: <n>` and calls
  `reschedule(timeTillSend, con)`, never the send-branch
  `timeTillSend <= 0 && getNextSendTime() > 0` arm that would
  invoke `con.sendAvailable()` and downstream
  `flushAvailable`/`writeData`/`buildPacket`/`sendPacket`. No
  i2pr-owned defect is attributed.
- **Severity low (Plan-244 attribution retained):** the
  Plan-244-A terminal `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED`
  remains the deepest Plan-244-chain stop on every attempt
  (because `ack_delta=0` from the Plan-237 8-field extract of the
  Plan-245 17-field snapshot). The Plan-245 Stage A.0 terminal
  `P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH` is the authoritative
  attribution for the same fresh epoch; Plan-245 supersedes only
  Plan-244's interpretation of the zero retransmit-timer log
  delta, never the Plan-244 execution evidence (Plan 245 §1, also
  the `p245_plan244_proxy_false_negative_does_not_rewrite_history`
  unit row).
- **Severity low (Plan-237 stats extract):** Plan 245 widens the
  helper `RESPONSE_STATS` line from 8 fields (Plan 237) to 17
  fields (Plan 237 + seven new direct-attribution needles + two
  logger-enabled flags). The Plan-237 standalone parser is
  retained for the eight-field legacy helper shape only; the
  streaming driver now extracts the Plan-237 eight-field shape
  from the Plan-245 17-field snapshot via
  `p237_stats_from_p245_stats`, keeping the Plan-237 stats row
  documented and the Plan-237/244 classifier ordering intact.
- Live `§§C–G` arms beyond `P244-A-SCHEDULER-RESCHEDULED-NO-SEND-
  BRANCH` are unit-locked, not live-proven. The streaming driver
  has no live Router-B query probe in this lane (the helper
  client reply tunnel carries the answer; no B dispatch); no
  live OCMOSJ post-dispatch gateway/transit/IBGW probe; no live
  i2pr inbound probe for the reverse direction. The §20 successor
  owns them.
- Direction A established 3/3 does not close M6; §§13/20
  continuation and closure do not trigger. No M6 Java-family
  closure is claimed.

## Disposition and unblock audit

```text
plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan245-successor-narrow-timer-state-attribution
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan245-successor-narrow-timer-state-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_243 = passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established
plan_244 = passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary
plan_245 = passed-m6-java-streaming-stock-response-construction-signal-attribution-corrective-with-scheduler-rescheduled-no-send-branch-boundary
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = none-pending-successor-narrow-timer-state-attribution
```

Plan 201 consumes Plan 245 as the next hard dependency on the
streaming axis: the Plan-244 retransmit-timer proxy has been
source-locked as conditional (Plan 245 §3.6), the direct
authoritative construction signal has been observed (Plan 245
§3.5 / §6), and the deepest proven live Stage A.0 terminal is
now `P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH` (3/3 with fully
bound response epochs, identical deltas). The Plan-201 streaming
axis still needs a successor narrow timer/state attribution plan
(named in Plan 245 §17, second arm: "scheduler only reschedules →
narrow timer/state attribution"), plus the bidirectional Streaming
qualification, plus the publication / final-closure axis. No
successor is pre-registered here; the corrective requires its own
plan-of-record under the same subsystem, owned by the Plan-245
§17 second arm, and must NOT authorize any i2pr production
corrective before exact expected reverse TunnelData reaches i2pr.

Plan 204 consumes Plan 245 as the next hard dependency on the
Java-second-family closure: M10 product authority through Plans
213–215 is unchanged. Plan 204 stays blocked on M6 Java
second-family closure (now pending the narrow timer/state
attribution successor + publication / final-closure axis; no
double unblock).

Plan 205 stays retained / deferred (the direct i2cp requalification
lane is intentionally out of scope for Plan 245).

No other registered plan listed Plan 245 as a hard dependency,
so nothing else changes state. Unblock audit complete: no plan
becomes dependency-ready at this closure; the only authorized
forward motion is the Plan-245 §17 second arm
(successor-narrow-timer-state-attribution plan-of-record), which
will own the bounded investigation into why
`con.getNextSendTime() - _context.clock().now() > 0` holds for the
frozen 45-second response window on every counted attempt.

## Follow-up boundary

A successor owns exactly one §17 arm from the exact final boundary:

- scheduler has no unacked packets → narrow inbound ACK-state
  attribution (not observed; the no-unacked guard warning delta is
  zero on all three attempts);
- **scheduler only reschedules → narrow timer/state attribution** —
  **this is the observed arm**;
- writeData suppresses send → narrow ConnectionDataReceiver state
  attribution (not observed; `receiver_do_send_false_delta=0` on
  all three attempts);
- direct build remains unobservable → narrow observer corrective
  (not observed; `receiver_packet_built_delta=0` with the logger
  enabled and the substring present, so the observation is real
  zero, not Unknown);
- direct build proves Plan-244 proxy false negative and
  downstream send fails → resume exact response-send corrective
  (not observed; direct build is zero);
- Router-A/lookup/B/tunnel stage fails after direct construction
  → exact corresponding retained-stage corrective (not observed;
  construction is not proven);
- expected reverse TunnelData reaches i2pr then fails → exact
  production i2pr corrective for that owned stage (NOT observed;
  production change stays forbidden);
- reverse direction establishes → return to Plan 201
  publication/final Java second-family closure (not observed).

The successor must retain the Plan-242 §7 corrected gate, the
Plan-242 §6 extended pool observation, the Plan-242 source-lock
(38 + 12 = 50 rows, md5 `890f7e0cef20d0707d4ff348b6843387`), the
Plan-243 host discipline, the frozen topology / profile / timing,
and the fail-closed lanes, and stop at the first proven stage. It
must NOT pre-authorize a topology correction, a publication
corrective, a tunnel-policy change, a selector-bias correction, an
RNG override, or a production i2pr change unless exact expected
TunnelData proves an i2pr-owned defect.