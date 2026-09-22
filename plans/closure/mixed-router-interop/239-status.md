# Plan 239 status — M6 Java Streaming Router-A pre-dispatch / OCMOSJ attribution

Status: `passed-m6-java-streaming-router-a-dispatch-observer-with-target-leaseset-lookup-failed-boundary`

Plan 239 wired the first real Router-A pre-dispatch observation into the
Plan-238 proven I2CP-admission epoch and moved the terminal exactly one
D stage forward on three identical counted executions: the helper JVM
proved scheduler action (delta 2), response-packet construction (delta
1), and `I2PSession.sendMessage` return (lifetime-event delta 3, zero
failures) exactly as in Plans 237/238, Router A proved I2CP admission
of the response client messages (`client.distributeTime` delta exactly
3 on all three runs) with zero dispatch
(`client.dispatchTime`/`client.dispatchSendTime` deltas 0), and the new
P239-DISPATCH observer proved the earliest post-admission OCMOSJ stage:
no usable local target LeaseSet in the helper client sub-DB
(pre/post `target_ls_local_present=false`, type `-1`), zero remote
lookup successes (`lease_lookup_found_remote_delta 0` on all runs),
and positive remote lookup failures (`failed_remote_delta 3/4/4`),
stopping at the first D stage:

```text
P239-D-TARGET-LEASESET-LOOKUP-FAILED
```

No Java source was patched, no production Rust changed, and no
tunnel-preparation, dispatch, transit, target-IBGW, or i2pr delivery is
claimed. The retained historical classifier token
`P237-D-CLIENT-MESSAGE-NOT-ADMITTED` remains the post-admission /
pre-dispatch boundary label; Plan 239 does not re-litigate admission.

## Authority and exact pins

Implementation checkpoint:

```text
d82cf06 (Router-A pre-dispatch OCMOSJ observer; three counted full-lane
         attempts 1/2/3 with identical terminal and no
         between-attempt tuning)
```

References unchanged:

```text
Java I2P 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
```

Retained: Plan 232 route-derived lease-gateway correction and
bidirectional raw-Destination pass (code untouched), Plan 234 Java
accept-returned boundary, Plan 235 usable `I2PSocket` surface +
outbound-admission prerequisite, Plan 236 source-lock ordering, Plan 237
stock deltas (2/1/3/0/0) and logger proof, Plan 238 Router-A admission
deltas (distribute 3, dispatch 0/0), frozen topology, profile policy,
publication target, and timing windows (`DATAGRAM_WAIT`/`STREAM_WAIT`/
`SYN_ACK_WAIT` all still 45 s).

## Source lock (WP1)

Exact-pinned source review identified the minimum stock Router-A
signals for post-admission OCMOSJ attribution, in pinned order:

```text
OCMOSJ constructor local lookupLeaseSetLocally(toHash)
  (before any runJob remote lookup; zero found-remote delta never
  means no LS)
client.leaseSetFoundRemoteTime / client.leaseSetFailedRemoteTime
  (remote lookup success / failure)
client.dispatchNoTunnels (two distinct branches)
  Could not find any outbound tunnels to send the payload through
    (selectOutboundTunnel branch)
  Unable to create the garlic message (no tunnels left or too lagged)
    (garlic-construction branch)
client.dispatchPrepareTime (send() after inline DispatchJob returns)
tunnelDispatcher().dispatchOutbound (DispatchJob.runJob inline)
client.dispatchTime / client.dispatchSendTime (after dispatchOutbound)
```

Local-LS rejection logs (`Lookup locally didn't find the leaseSet`,
`Only have RAP LS`, `Got the lease but can't send to it`,
`No leases found`) are locked as source facts for the D1 log
counters; execution raw log lines never enter durable evidence.

The source lock
(`scripts/interop/check-m6-java-response-source-lock.sh`) is extended
with the twelve P239 needles above; the TSV gains nine additive rows
(25 rows total, 16 retained shapes frozen). The TSV is byte-identical
across the three counted attempts:

```text
md5 7803b9e909693541e4e56b61e99f18f9  (attempts 1/2/3)
```

## Observation mechanism (WP2–WP3)

One tiny public Router-A command (`P239-DISPATCH <client-dbid-hex>
<target-hash-hex>` in the out-of-tree `ControlledRouter.java`,
observed read-only through the new out-of-tree `P239Probe.java`
compiled against the exact-pinned jars):

- target LS via retained P224 client-subDB semantics
  (`lookupLeaseSetLocally` validated presence, type, current,
  RAP/RAR; local reads only, never primes the sub-DB);
- `client.leaseSetFoundRemoteTime`,
  `client.leaseSetFailedRemoteTime`, `client.dispatchNoTunnels`,
  `client.dispatchPrepareTime`, `client.dispatchTime`,
  `client.dispatchSendTime` lifetime events via public
  `StatManager.getRate(...).getLifetimeEventCount()`;
- installed inbound/outbound client-tunnel counts + single ids via
  public `TunnelManager.getOutboundPool/getInboundPool` +
  `TunnelPool.listTunnels()` read-only (send id only when exactly one
  outbound, receive id only when exactly one inbound, otherwise 0);
- sanitized exact-source OCMOSJ log counts via public
  `LogManager.getBuffer().getMostRecentMessages()` substring counts
  (bounded, capped; counts only, never raw lines);
- `-1` for a never-created rate or unreadable buffer (Unknown, never
  zero-as-fact); `0` tunnel/log counts are measured zeros, never
  Unknown-as-zero.

No Java source patching, no reflection, no private-field mutation, no
rate creation, no packet injection, no NetDB/tunnel/profile/stat
writes. All facts are bounded. No packet contents, keys, tags,
payloads, destinations, hashes, queue contents, or private state cross
into evidence — sanitized counts/booleans/type codes/tunnel ids only,
raw logs stay scratch-only.

The Rust driver (`streaming_through_java`) snapshots Router A
(`JAVA_DIAGNOSTIC_A_PORT`, additive env only, client hex = helper
destination, target hex = i2pr destination) immediately before the
Direction-A SYN and at the end of the frozen 45 s response window,
then maps isolated-epoch deltas onto the ordered D1/D2/D3 classifier
(never absolutes): local-LS path precedes any remote interpretation;
remote failure precedes tunnel attribution; any `dispatchNoTunnels`
event requires a branch discriminator (never tunnel counts alone);
dispatch requires both `dispatchTime` and `dispatchSendTime`
(`prepare` is corroboration only); post-dispatch stages require proven
dispatch; i2pr-owned terminals require expected TunnelData. Plan-238
admission (`distributeDelta > 0`) is a prerequisite; without it no D
stage is claimed.

Router A is long-lived across the destination + streaming sub-runs
(Plan 217 §6.D), so pre snapshots may be non-zero when the
destination lane dispatched first (attempt 1 pre `1/0`) and pristine
otherwise (attempts 2/3 pre `0/0`). Deltas isolate the SYN epoch under
the lane-quiet premise (the driver issues no other SendMessage during
the response window; distribute delta exactly 3 on all runs proves
exactly three client sends, matching helper sendMessage 3);
absolutes never attribute.

## Stat/logger enablement proof

All three counted attempts prove helper loggers enabled at both
snapshots and Router-A dispatch rates known (non-`-1`) at both
snapshots; target-LS facts are measured absent (not Unknown), lookup
rates are known, tunnel pools resolve with 1/1 installed each, and log
buffers are readable (0 counts are measured, not Unknown):

```text
scheduler_debug_enabled=true connection_debug_enabled=true
packetqueue_debug_enabled=true   (pre and post, attempts 1/2/3)
found/failed/noTunnels/prepare/dispatch/send known pre and post
(attempt 1: 1/0/0/1/1/1 -> 1/3/0/1/1/1; attempts 2/3: 0/0/0/0/0/0 -> 0/4/0/0/0/0)
tunnels 1/1 with stable single ids per attempt
logs readable (all five OCMOSJ counts 0 post, i.e. no branch log in window)
```

Per §8, absence of a log in the window does not prove
non-execution; D1 is proven by the positive `failed_remote` delta
with local absent and `found` zero, not by logs. Known-zero dispatch
deltas count as proven dispatch absence for the epoch (not Unknown).

## Counted evidence (SHA d82cf06, no tuning between attempts)

| Attempt | Evidence dir | Router-A P239 pre (found/failed/noT/prep/disp/send) | Post | Deltas (found/failed/noT/prep/disp/send) | Terminal |
| --- | --- | --- | --- | --- | --- |
| 1 | `target/interop/m6-java-evidence-plan239-attempt1` | 1/0/0/1/1/1 | 1/3/0/1/1/1 | 0/3/0/0/0/0 | `P239-D-TARGET-LEASESET-LOOKUP-FAILED` |
| 2 | `target/interop/m6-java-evidence-plan239-attempt2` | 0/0/0/0/0/0 | 0/4/0/0/0/0 | 0/4/0/0/0/0 | `P239-D-TARGET-LEASESET-LOOKUP-FAILED` |
| 3 | `target/interop/m6-java-evidence-plan239-attempt3` | 0/0/0/0/0/0 | 0/4/0/0/0/0 | 0/4/0/0/0/0 | `P239-D-TARGET-LEASESET-LOOKUP-FAILED` |

Full per-attempt P239 snapshots (all three):

```text
attempt 1 pre:  local=false current=None type=-1 found=1 failed=0 out=1 in=1 sendId=3889796347 recvId=203616491 noT=0 prep=1 disp=1 send=1 logs=0/0/0/0/0
attempt 1 post: local=false current=None type=-1 found=1 failed=3 out=1 in=1 sendId=3889796347 recvId=203616491 noT=0 prep=1 disp=1 send=1 logs=0/0/0/0/0
attempt 2 pre:  local=false current=None type=-1 found=0 failed=0 out=1 in=1 sendId=2952576737 recvId=2350708776 noT=0 prep=0 disp=0 send=0 logs=0/0/0/0/0
attempt 2 post: local=false current=None type=-1 found=0 failed=4 out=1 in=1 sendId=2952576737 recvId=2350708776 noT=0 prep=0 disp=0 send=0 logs=0/0/0/0/0
attempt 3 pre:  local=false current=None type=-1 found=0 failed=0 out=1 in=1 sendId=1239093091 recvId=2432987993 noT=0 prep=0 disp=0 send=0 logs=0/0/0/0/0
attempt 3 post: local=false current=None type=-1 found=0 failed=4 out=1 in=1 sendId=1239093091 recvId=2432987993 noT=0 prep=0 disp=0 send=0 logs=0/0/0/0/0
```

Reading: Router A admitted all three response client messages
through I2CP (`distributeTime` +3, matching helper sendMessage delta
3 on every run), found no local target LS in the helper client sub-DB
pre/post, performed no successful remote lookup (found delta 0), and
failed the remote lookup (`failed_remote` +3/+4/+4) before any
tunnel selection — so no `dispatchNoTunnels` event (delta 0 with
rates known), no `dispatchPrepare`/`dispatchTime`/`dispatchSend`
advance (deltas 0 with rates known), and no branch-log attribution is
needed or claimed. The `failed` 4 vs `distribute` 3 mismatch on
attempts 2/3 is honestly recorded: Router A is long-lived, and a
stale destination-lane lookup-timeout job can fire its
`failedRemoteTime` record inside the streaming window without a new
client send (distribute proves exactly three sends). The terminal is
unchanged on all runs (failed > 0 with local absent and found 0),
so no `P239-A-ROUTER-A-EPOCH-NOT-ISOLATABLE` is warranted; the quiet-
for-sends premise holds via distribute.

Retained baselines reproduced identically on all three attempts:

```text
P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED
P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND
P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP
P237 stock post 2/1/3/0/0 with all loggers enabled, deltas 2/1/3/0/0
P237-D-CLIENT-MESSAGE-NOT-ADMITTED (distribute 3, dispatch 0/0)
p237-router-stages router_i2cp_observed=true, all later stages false
```

Destination-lane context (full `both` lane, retained not re-litigated):
attempt 1 reproduced the Plan-232 shape (`P231-REVERSE-DELIVERY-PASSED`
role=A with the route-derived lease fixture, `P232-D-REVERSE-DELIVERY-
PASSED`); attempts 2/3 took the retained P230 early-stop path
(`P230-D-ELIGIBLE-BUT-NO-PROFILE`, `P231-A-OBSERVABILITY-GAP
reason=destination-lane-not-entered`). All three are retained
prior-plan shapes; none implicate the P239 observer, and the
streaming-epoch deltas isolate the response epoch regardless.

## Proof of no production change

`git status` on the implementation head is clean; `git show --stat
HEAD` shows only the six harness surfaces (§10); no `src/` file
changed:

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
scripts/check-m6-mixed-router-acceptance-evidence.sh
scripts/interop/check-m6-java-response-source-lock.sh
tests/integration/m6-interop/java/ControlledRouter.java
tests/integration/m6-interop/run-java.sh
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P239Probe.java  (new)
```

The M6 checker §28 production-surface guard (no `P239`/`p239` in
`i2pr-daemon/src`, `i2pr-client/src`, `i2pr-tunnel/src`,
`i2pr-runtime/src`) is green. The streaming helper
(`ReferenceStreamingService.java`) carries no P239 surface by checker
guard — the observer lives on Router A, never in the client helper.

## Focused verification (implementation head d82cf06)

Passed locally:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external p236_ -- --test-threads=1   (18 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p237_ -- --test-threads=1   (12 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p238_ -- --test-threads=1   (12 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p239_ -- --test-threads=1   (12 passed: 11 §10 rows + token lock)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>
javac <all staged Java helpers incl. P239Probe against exact-pinned jars>   (exit 0)
all 18 boundary/vector/evidence scripts in AGENTS.md
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'   (18 tests OK)
```

Full serial workspace floor: **passed** on the implementation head
(`cargo test --locked --workspace --all-targets -- --test-threads=1`,
exit 0: 2722 passed, 18 ignored, 103 suites, 607.65 s). No
`P239-V-WORKSPACE-FLOOR-INCOMPLETE-SAM-HANG` token was needed.

## Requirement-to-evidence matrix (§15)

1. Plan-238 Router-A admission remains proven (distribute delta
   exactly 3 with dispatch deltas 0 on all three runs, rates known).
2. Target-LS local vs remote-lookup state is measured without
   zero-counter inference (constructor local path locked; local
   present+current checked pre/post; found delta 0 never means no LS;
   Unknown `-1` never zero).
3. Any `dispatchNoTunnels` event is branch-disambiguated or explicitly
   left ambiguous (delta 0 here with rates known, so preparation
   absence is proven and no branch is claimed; unit rows lock the
   discriminator requirement and the two distinct branches).
4. Dispatch is claimed only from exact `dispatchTime` /
   `dispatchSendTime` evidence (both deltas 0 here with rates known,
   so dispatch absence is proven; `prepare` alone never proves
   dispatch by unit row).
5. The earliest D/E/F terminal repeats on counted execution
   (`P239-D-TARGET-LEASESET-LOOKUP-FAILED` on all three same-SHA runs,
   no tuning, unique evidence dirs).
6. No production Rust or Java source changes occur (clean status +
   §28 guard green).
7. Plan-232 raw-Destination authority remains intact (attempt 1
   reproduces the reverse pass shape; attempts 2/3 take retained
   early-stop paths; streaming deltas isolate regardless).
8. Verification is represented honestly (full floor green; 3/4 vs 3
   mismatch documented, not concealed; void/exploratory none beyond
   the three counted runs).
9. Plan-201/204 authority is updated together at closure (see
   disposition; no silent unblock).

## Limitations and findings

- Severity medium (boundary, not defect): the D terminal advanced
  exactly one stage past admission. Remote lookup failure
  (`failed_remote` +3/+4/+4 with local absent, found 0) is a proven
  epoch observation, not a proven Router-A failure cause — why the
  helper client lookup never succeeds (B answerability, search
  dispatch, DSM receipt, sub-DB install) remains unobserved. A
  successor owns that narrow lookup-failure attribution; no i2pr
  defect is claimed.
- Severity low (measurement note): attempts 2/3 record `failed` 4
  against `distribute` 3. The extra failure is a stale timeout job
  from the long-lived Router A firing inside the window, not an
  extra client send (distribute proves exactly three sends). The
  terminal is unaffected (failed > 0 with local absent). Future
  observers that equate counts exactly should snapshot tighter or
  prove quiet-for-timeouts separately; Plan 239 does not.
- i2pr-side D2/D3/E/F stages (tunnel preparation, dispatch, transit,
  IBGW, TunnelData, recovery, Garlic, adapter) were not reached;
  their ordering is implemented and unit-locked but live unproven.
- Log-buffer counts are 0 on all runs (no OCMOSJ branch log in the
  512-entry window). D1 does not need them (stat deltas prove the
  stage); the two `dispatchNoTunnels` branch discriminators and LS
  rejection logs remain unit-locked but live unexercised.
- Direction A did not establish; §14 continuation does not trigger.
  No M6 Java-family closure is claimed.

## Disposition and unblock audit

```text
plan_201 = blocked-after-plan239-target-leaseset-lookup-failed-pending-lookup-failure-attribution-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-lookup-failure-followup-after-plan239
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_238 = passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary
plan_239 = passed-m6-java-streaming-router-a-dispatch-observer-with-target-leaseset-lookup-failed-boundary
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = none-pending-lookup-failure-followup-plan-of-record
```

Plan 201's streaming axis consumed its `pending-plan239` hard
dependency (the Router-A pre-dispatch observer executed; D1
lookup-failure proven with dispatch absence proven for the epoch),
but Plan 201 stays blocked: the streaming axis now needs a narrow
lookup-failure attribution follow-up (no plan-of-record yet), and the
publication/final-closure axis is unchanged. Plan 204 stays blocked
on M6 Java second-family closure (now pending that follow-up instead
of Plan 239); M10 product authority through Plans 213–215 is
unchanged. Plan 205 stays retained/deferred. No other registered plan
listed Plan 239 as a hard dependency, so nothing else changes state.
No new plan is registered here; the next follow-up requires its own
plan-of-record under the same subsystem.

## Follow-up boundary

A successor owns only the narrow Router-A lookup-failure attribution
for the streaming response epoch (read-only, stock signals, no Java
patching, no production change unless exact expected TunnelData
proves an i2pr-owned defect). It must retain the Plan-239 deltas,
source lock, frozen topology/profile/timing, and fail-closed lanes,
and stop at the first proven D/E stage. Broader publication,
tunnel-policy, or production correctives remain out of scope until a
plan-of-record proves them.
