# Plan 231 status — M6 Java reverse-delivery tunnel-dispatch attribution corrective

Status: **`passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary`**.

Plan of record:
[`231-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective.md`](../../implementation/mixed-router-interop/231-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective.md).

## Closure result

Plan 231 is closed as a completed, fail-closed post-`ACCEPTED`
attribution with one exact earliest missing stage. The deepest counted
run proved every link it could and stopped exactly where the plan
requires:

```text
tracked Java client send (nonce=1, len=27, digest-matched intent)
  -> OCMOSJ target lease (gateway=B, tunnel=38401) + outbound tunnel selected
  -> OCMOSJ DispatchJob called TunnelDispatcher.dispatchOutbound() and returned
  -> Java A outbound gateway accepted/enqueued (client.dispatchTime +1,
     window dispatchOutboundTunnel +11, overflow 0, no id-correlated
     no-matching row): P231-A-ENQUEUED
  -> Java C exact one-hop OBEP present (receive == A send id,
     receive-from == A) and processed 1 -> 5: first hop received
  -> B dispatchInbound 1 -> 7 (forward leg observed)
  -> selected target IBGW on B for (gateway=B, tunnel=38401): ABSENT
     pre and post (present=false twice, exact-ID queries)
  -> terminal P231-C-TARGET-IBGW-NOT-INSTALLED
  -> i2pr exact inbound TunnelData: none (all wire/recovery/Garlic/
     Destination counters honestly zero)
```

No `P231-REVERSE-DELIVERY-PASSED`. No later terminal. Exactly one
`p231-classification` row per counted attempt. No production Rust
behavior change. No new i2pr-visible wire/protocol defect exists to
correct: i2pr never received the wire message, so every i2pr-owned
stage is vacuous by measurement, not by assumption.

## Implementation commits and pinned inputs

Attribution was implemented before counted execution (Plan 231 §11);
each fix below is its own implementation SHA with its own attempt
budget and no between-attempt tuning:

```text
b3502d2 interop: implement Plan 231 reverse-delivery tunnel-dispatch attribution
6d7e22c interop: fix Plan 231 gateway snapshot field parity (8-field row)
5134331 interop: apply Plan 231 stat.full observability correction
e3a76a9 interop: emit Plan 231 gap row for destination-lane-not-entered runs
1f5a495 interop: correct Plan 231 A-stage epoch to window attribution
```

Full SHAs:

```text
b3502d2d189f4dbb04c9d4c2ee107571623bd856
6d7e22c5d74074a23d50b7f362e28f0817bb560d
5134331310636b9cd2d1bc5e8bc2d447b4c7501e
e3a76a963df5f11a391fd41facbf729968269a10
1f5a4953adad84a72a0c4d611df2c0062ddf584e
```

Closing implementation SHA is `1f5a495`; the working tree was clean
at every counted run and the full routine floor ran on that exact
tree (see Verification).

Reference inputs remained frozen: Java I2P `2.13.0` at
`9134f808337b401e8e53c73734c81fab04280c9d`; the i2pd reference pin
remained `2.61.0` at `635b013a612ff47278ef02acf8580a28e10e26c5`. No
dependency, fixture, production protocol, or Java reference source
changed. No Java source patch, reflection, private-field access,
direct queue/tunnel/NetDB/profile injection, paired-tunnel policy
override, VMComm, `netDb.alwaysQuery`, public I2P, distinct topology,
build/timeout change, streaming-helper change, or reverse-window
change (45-second payload acceptance and 70-second status-only
observation frozen; checker-pinned).

Test-only deltas (no production Rust code changed; enforced by the
new checker rule that rejects any `p231`/`P231` surface under
`crates/i2pr-daemon/src`, `crates/i2pr-client/src`,
`crates/i2pr-tunnel/src`, `crates/i2pr-runtime/src`):

```text
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P231Probe.java (new)
tests/integration/m6-interop/java/ControlledRouter.java (P231-GATEWAY/P231-CLIENT-OUTBOUND/P231-PARTICIPATING commands + stat.full correction)
tests/integration/m6-interop/run-java.sh (P231 probe compile, JAVA_C_LOG_DIR input, single-terminal guard)
crates/i2pr-daemon/tests/java_tunnel_external.rs (reverse epoch, snapshots, counters, classifier, 18 unit rows)
scripts/check-m6-mixed-router-acceptance-evidence.sh (Plan 231 section 24 invariants)
```

## Requirement-to-evidence matrix

| Plan-231 requirement | Evidence / result |
|---|---|
| Reference pins + Plan-230 topology/corrections unchanged (§4.1–2, §15.1) | Java `9134f80…` on all attempts; C1 (`i2np.udp.status=ok`) + C2 (transit-C `128/128`) retained in launcher; checker §23c + §24 green on closing SHA. |
| No production Rust changes (§4.3, §15.11) | `git diff` on closing SHA touches only the five test-only files above; production-surface checker rule green. |
| No Java patching/reflection/mutation, no injection, no timeout/topology changes (§4.4–10) | Probe uses `statManager().getRate` + `getLifetimeEventCount`, `getOutboundPool`/`listTunnels`/`getSendTunnelId(0)`, `tunnelDispatcher().listParticipatingTunnels` + `HopConfig` getters only; checker §24b forbids mutators/reflection/`heardAbout`/stores; windows frozen (45/70 checker-pinned). |
| Source-order lock (§5, §15.2) | Exact-pinned review: `ClientMessageEventListener.handleSendMessage` calls `distributeMessage` first, OCMOSJ runs inline, `DispatchJob.runJob` calls `dispatchOutbound(_msg, _outTunnel.getSendTunnelId(0), _lease.getTunnelId(), _lease.getGateway())` before returning, and only then `ackSendMessage` emits `ACCEPTED`. Locked in code (`p231_accepted_implies_dispatch_called`) + unit test + classifier pass-through. The lock proves call ordering only — never delivery. |
| Reverse epoch contract (§5, §15.3) | `p231-reverse-epoch` row per reverse run: nonce, payload_len, payload_sha256, tracked_send_start_ms, accepted_observed(+ms), target_ls_hash, target_lease_gateway_hash, target_lease_tunnel_id, java_outbound_client_send_tunnel_id, java_outbound_client_first_hop_hash. Single-lease LS2 ⇒ selected lease is exact (no first-lease assumption). |
| Java A gateway stage (§6, §15.4) | Pre/post-immediate/post-window `P231-GATEWAY` snapshots (8 lifetime counts) + pre `P231-CLIENT-OUTBOUND` (single send id) + pre P227 exact-via-C proof. Window attribution under documented lane-quiet premises (exactly one client dispatch; missing gateways always log). Attempt 2: `client.dispatchTime` 0→1, window `dispatchOutboundTunnel` 0→11, overflow 0, no id-correlated no-matching row ⇒ ENQUEUED. |
| Router C OBEP stage (§7, §15.5) | Exact OBEP config by receive-id == A send-id with receive-from == A, pre/post processed counts + endpoint context stat. Attempt 2: present, match_count=1, send_to=none (endpoint), processed 1→5. |
| Target IBGW stage (§8, §15.6) | Lease gateway resolves to role B (exact selected lease, never assumed); exact IBGW config (receive == lease tunnel 38401) queried pre/post on B with gateway stats. Attempt 2: present=false twice ⇒ `P231-C-TARGET-IBGW-NOT-INSTALLED`. send_to==i2pr check armed (unrelated colliding ids cannot satisfy). |
| i2pr exact-TunnelData/recovery/Garlic/Destination (§9, §15.7–8) | Pump counts every stage by exact owned inbound tunnel id (`receive_ids[0]`); unrelated TunnelData is context only. Attempt 2: all target counters zero (0 seen, 0 recovery, 0 Garlic, 0 dispatch, 0 queue, digest false) with `DestinationTunnelError` category buckets armed. Structured categories (`UnknownInboundTunnel`, `CellIncomplete`, `UnexpectedBodyType`, `Inbound`) recorded by bounded enum branch, never stringified secrets. |
| Exactly one earliest terminal (§10, §15.9) | Rust emits one `p231-classification` on every driver path (epoch + both early-stop gaps); shell emits the single gap row only when the driver never ran; infra death stays VOID with no row. Aggregation duplicates are copies of the single emission (last-occurrence reading, same practice as P230). |
| No timeout inflation or fixture tuning (§10, §15.10) | 30-second D poll, five-minute helper/tunnel ceiling, 45-second payload window, 70-second status-only window all frozen; no second correction inside any counted SHA. |
| Closure/registry/roadmap/unblock audit (§15.13) | This record plus registry/roadmap/201/204 updates in the same commit (see below). |

## External attempt history

### Closing SHA `1f5a495` (3 counted attempts, no tuning)

| Attempt | Router-C hex | P230 result | P231 result |
|---|---|---|---|
| 1 | `4cb4a963…` | `P230-D-ELIGIBLE-BUT-NO-PROFILE` (eligible; 30 s D poll exhausted with profile_present=false) | `P231-A-OBSERVABILITY-GAP reason=destination-lane-not-entered` (shell gap; destination lane not entered) |
| 2 | `b5cd7956…` | `P230-E-TUNNEL-CONTINUATION-PASSED` (eligible + natural bootstrap 1/1 + exploratory 3/3 nonzero both dirs + 1/1 exact one-hop client tunnels through C both dirs, zero-hop absent; forward digest-matched; reverse admitted-but-unarrived) | `P231-C-TARGET-IBGW-NOT-INSTALLED nonce=1 role=B lease_tunnel=38401 send_id=2846582670 expected_seen=false digest_match=false ordered_statuses=[1]` |
| 3 | `1ac62df3…` | `P230-D-ELIGIBLE-BUT-NO-PROFILE` (same shape as attempt 1) | `P231-A-OBSERVABILITY-GAP reason=destination-lane-not-entered` (shell gap) |

Attempt-2 authoritative per-stage facts (fresh disposable A/B/C
RouterContexts, `I2PR_M6_JAVA_DRIVER=destination`, exact pins):

```text
epoch: nonce=1 payload_len=27 payload_sha256=8a9e8146bb7d8c0b32b19b2483913d9f38aab1c7bfc925b1a73e7cd5aebb2271
  tracked_send_start_ms=1790026354248 accepted_observed=true accepted_observed_ms=1790026354253 (4 ms)
  target_ls_hash=68070c129c75814d0655731d971d44c828ff8e264df8c4d7eb63ae597353543d
  target_lease_gateway_hash=becbea5a5ac977c244e8c87d6ef6fca746db90588969308fd5445ef723e79c4c (role=B)
  target_lease_tunnel_id=38401 (0x9601 = IBGW_RECEIVE)
  java_outbound_client_send_tunnel_id=2846582670
  java_outbound_client_first_hop_hash=b5cd7956543ebcefacd1ddd2322802fab3a4a16e79fc1dbc847d3340577dcf70 (C)
forward: destination-outbound-delivered cells=1 payload_len=27
  reference-received payload_len=27 match=true digest=ec61e08da98b76b02ee2268d544b90da0c3b7cace6339d627cd3d40b85048ceb
reverse: destination-inbound-send-failed send_status=public-send-accepted ordered_statuses=[1]
  reverse_i2pr_payload_recovered_45s=false
A gateway (stat.full lifetime counts):
  pre:            dispatch_time=0 dispatch_send_time=0 dispatch_outbound_tunnel=0 drop_gateway_overflow=0
  post-immediate: dispatch_time=0 dispatch_send_time=0 dispatch_outbound_tunnel=0 drop_gateway_overflow=0
  post-window:    dispatch_time=1 dispatch_send_time=1 dispatch_outbound_tunnel=11 drop_gateway_overflow=0
  client-outbound pre: resolved=true count=1 send_id=2846582670 still_installed_exact_via_c=true
  => ENQUEUED (window attribution: exactly one client dispatch + gateway accept + no correlated no-match)
C exact OBEP (receive == send_id, receive-from == A, endpoint):
  pre: processed=1  post-window: processed=5 (delta +4)
B forward/target (role=B):
  dispatchInbound 1 -> 7 (delta +6); IBGW for 38401: present=false pre AND post
  id-correlated no-matching-IBGW receipt: 0 -> 0 (INFO-blind, supporting only)
  => exact IBGW absent => P231-C-TARGET-IBGW-NOT-INSTALLED
i2pr reverse epoch:
  ssu2_datagrams_received_delta=5 i2np_messages_received_delta=0
  tunneldata seen/expected/unexpected 0/0/0; decode/recovery/Garlic/dispatch/queue all 0; digest false
stages: stage_a_enqueued=true stage_b_forward_proven=true stage_c_emitted_proven=false
```

The enclosing legacy Plan-199 Java wrapper exited nonzero (install/
lookup/streaming rows remain unqualified beyond the attributed
path), as on every prior plan. Raw Java logs remained scratch-only
(fixed-substring counts only).

### Pre-closing SHAs (traceability; not counted toward closure)

- `b3502d2`: 1 execution, reverse epoch reached, but every gateway
  snapshot unparseable — the launcher emitted six stat fields while
  the parser required eight (implementation defect). No protocol
  terminal derivable; VOID for protocol purposes. Fixed by `6d7e22c`
  (parity proven live against a controlled router before any counted
  run on that SHA).
- `6d7e22c`: 2 executions. (a) lease-stalled early-stop gap
  (reference LS2 never resolved; epoch never reached). (b)
  reverse-epoch A-gap with every gateway count `-1`: root-caused to
  exact-pinned `StatManager.createRateStat` creating rates ONLY when
  `stat.full` is true (`ignoreStat` drops them otherwise, and every
  `addRateData` is then a silent no-op). The plan's counter surface
  never exists on the stock profile. Fixed by `5134331`.
- `5134331`: 1 VOID (Router A died before SAM listen, exit 2, no
  Java log, no terminal — same infra shape as Plan 230 attempt 3)
  + 1 P230-D stop whose run emitted no P231 row (single-terminal
  guard lived inside the helper-ok section). Fixed by `e3a76a9`.
- `e3a76a9`: 1 execution, full gates + reverse epoch, A-gap under
  the micro-epoch rule — which the run's own data refuted
  (post-immediate dispatch count 0 while the window advanced
  0→10): the helper returns before the router dispatches, so the
  micro-epoch systematically misses the enqueue. Fixed by `1f5a495`
  (window attribution with lane-quiet premises + correlated-log
  priority). Attempt-2 data above replays cleanly through the
  corrected rule (ENQUEUED → forward → NOT-INSTALLED).

Attempt discipline (§11): implementation committed before counted
execution on every SHA; maximum three counted attempts per SHA; no
between-attempt tuning inside any SHA (each fix is a new SHA);
destination-only lane throughout; all windows frozen.

## Verification

Successful verification on closing SHA `1f5a495` (local truth; the
tree was clean, so the tested tree is the committed tree):

```text
cargo fmt --all --check                                      PASS (closing SHA)
cargo check --locked --workspace --all-targets                PASS (closing SHA)
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                                   PASS (2629 passed, 18 ignored; +18 are the P231 unit rows)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                                   PASS (closing SHA)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                                   PASS (closing SHA)
cargo test --locked --workspace --doc                         PASS (0 doc tests, closing SHA)
cargo deny check advisories bans sources                     PASS (closing SHA)
bash scripts/check-dependency-direction.sh                   PASS (closing SHA)
bash scripts/check-runtime-boundaries.sh                     PASS (closing SHA)
bash scripts/check-service-tunnel-boundaries.sh              PASS (closing SHA)
bash scripts/check-fixture-manifest.sh                       PASS (closing SHA)
bash scripts/check-ntcp2-vectors.sh                          PASS (closing SHA)
bash scripts/check-ssu2-vectors.sh                           PASS (closing SHA)
bash scripts/check-i2cp-vectors.sh                           PASS (closing SHA)
bash scripts/check-ntcp2-interoperability.sh                 PASS (closing SHA)
bash scripts/check-constrained-host-lane-boundary.sh         PASS (closing SHA)
bash scripts/check-sam-acceptance-evidence.sh                PASS (closing SHA)
bash scripts/check-ssu2-acceptance-evidence.sh               PASS (closing SHA)
bash scripts/check-i2cp-acceptance-evidence.sh               PASS (closing SHA)
bash scripts/check-service-tunnel-acceptance-evidence.sh     PASS (closing SHA)
bash scripts/check-exploratory-tunnel-evidence.sh            PASS (closing SHA)
bash scripts/check-netdb-tunnel-evidence.sh                  PASS (closing SHA)
bash scripts/check-destination-tunnel-evidence.sh            PASS (closing SHA)
bash scripts/check-streaming-tunnel-evidence.sh              PASS (closing SHA)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh    PASS (§24 invariants, closing SHA)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
                                                                   PASS (18 tests, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p231_ -- --test-threads=1
                                                                   PASS (18 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p230_ -- --test-threads=1
                                                                   PASS (21 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p229_ -- --test-threads=1
                                                                   PASS (25 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p228_ -- --test-threads=1
                                                                   PASS (21 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p227_ -- --test-threads=1
                                                                   PASS (14 passed, closing SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
                                                                   PASS (closing SHA)
bash -n tests/integration/m6-interop/run-java.sh             PASS
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh PASS
javac (all probes + launcher + helpers vs staged jars)       PASS (closing SHA)
```

The §24 integrity script additionally pins: the 8-field probe
surface with its read-only allowlist, the three launcher commands,
all classifier/parser/collector/record functions, all 17 terminal
tokens plus `P231-REVERSE-DELIVERY-PASSED`, the 18 unit rows, frozen
45/70 windows, no production `p231`/`P231` surface, scratch-only
logs, and the harness wiring without topology/timeout escapes.

Focused Plan-231 unit rows (18, all green on closing SHA):

```text
p231_accepted_is_ordered_after_inline_dispatch_call
p231_accepted_alone_does_not_prove_gateway_enqueue
p231_gateway_stat_delta_requires_target_epoch
p231_gateway_overflow_maps_to_enqueue_drop
p231_c_obep_requires_exact_installed_tunnel
p231_c_obep_count_delta_proves_first_hop_processing
p231_target_gateway_role_comes_from_selected_lease
p231_ibgw_requires_exact_target_tunnel_id
p231_ibgw_processed_delta_precedes_tunneldata_emitted
p231_unrelated_tunneldata_cannot_satisfy_i2pr_stage
p231_expected_tunnel_id_required_for_recovery_stage
p231_tunnel_recovery_failure_is_distinct_from_no_wire_receive
p231_garlic_failure_is_distinct_from_tunnel_recovery_failure
p231_destination_queue_hit_requires_digest_match_for_pass
p231_status_only_after_45s_cannot_pass_payload_delivery
p231_first_unknown_stage_maps_to_observability_gap
p231_exactly_one_terminal_per_counted_run
p231_secret_bearing_log_rows_rejected
```

## Security, compatibility, and operational decisions

- No production crate or protocol behavior changed; all deltas are
  in the external Java driver probes/launcher, the `run-java.sh`
  harness inputs and single-terminal guard, the
  `java_tunnel_external.rs` epoch/snapshot/counter/classifier/unit
  rows, and the static evidence checker. The new checker rule fails
  the build if any `p231`/`P231` token ever appears under
  production `src/` directories.
- The `stat.full=true` correction is observability-only: it is a
  stock Java I2P router property (same precedent as the retained
  `logger.defaultLevel=DEBUG`), sets no topology/profile/tunnel/
  NetDB/timeout/publication state, and changes no wire behavior.
  Without it the plan's own counter surface provably does not exist
  (`ignoreStat` drops every `createRateStat`); with it a mature
  idle router reports genuine lifetime counts (proven live before
  any counted run on the correction SHA).
- The P231 probes read through public accessors only
  (`statManager().getRate()` + `RateStat.getLifetimeEventCount()`,
  `tunnelManager().getOutboundPool()` + `listTunnels()` +
  `getSendTunnelId(0)`, `tunnelDispatcher()`
  `.listParticipatingTunnels()` + `HopConfig` getters). No
  reflection, no private-field or queue access, no `set*TunnelId`
  mutators (checker-forbidden), no `heardAbout`/`addProfile`/
  store/publish calls, no profile/tier/connection forcing.
- Durable evidence contains only bounded booleans, counts, tunnel
  ids, hex hashes, message ids, lengths, digests, and enumerated
  stage tokens. Raw Java logs, peer paths, keys, tags, SessionConfig
  contents, and payloads remain scratch-only; the sanitizers use
  fixed-substring matching and only bounded counts reach evidence
  (including the id-correlated no-matching-gateway rows, which carry
  the tunnel id already present in the epoch row).
- Java and i2pd pins, SAM/I2CP/diagnostic loopback policy, and all
  frozen windows remain unchanged. The lane stays destination-only.

## Findings and limitations

No security finding was introduced. Severity: no critical/high
findings.

High (harness-fixture defect, root cause of the reverse stop — owned
by the recommended successor, not by this attribution): the Rust
destination driver builds the published local LS2 with
`InboundLeaseSource::from_parts(slot, Hash(java_hash),
IBGW_RECEIVE, …)` — gateway Router B — while the inbound tunnel it
built (and proved in-registry: `gateway_router == service_hash`,
`gateway_receive_tunnel == IBGW_RECEIVE`, `local_receive ==
IBGW_NEXT`) terminates at Router A. The advertised lease therefore
addresses the reverse payload to a router that holds no inbound
gateway for the tunnel (B-side exact-IBGW absence proven twice),
while the true gateway (A) is never addressed. Java behaves
correctly given the wrong lease; i2pr behaves correctly given an
absent wire message. The corrective belongs to the controlled
fixture only (advertise the inbound peer's hash in the test
driver's lease source); it must not change i2pr production code.
This also explains why forward delivery (which uses Java's own
reference LS2, not the local one) passes while reverse fails.

Medium: the C→B forward for the exact target message is supported
but not isolated — B's `dispatchInbound` window advance (+6) is
background-plausible on a floodfill router, and the id-correlated
B receipt row is INFO-blind on this lane's file logging — so the
classifier leans on the exact-ID IBGW absence (delivery-necessary
under every sub-case: forwarded-then-dropped and never-forwarded
both terminate at B's missing gateway, and i2pr's zero-wire
counters corroborate non-delivery). A successor that fixes the
lease will re-prove the forward leg with per-id B signals or not
need to (delivery passing subsumes it).

Medium: profile-bootstrap stochasticity across fresh contexts —
attempts 1 and 3 exhausted the frozen 30-second D poll with C
outside the organizer (`P230-D-ELIGIBLE-BUT-NO-PROFILE`, count=1);
eligibility itself was 3/3 deterministic. Same family as Plan 230
attempt 3b; no timing inflation authorized to chase it.

Low: the post-teardown P227 pool snapshot is empty
(`client_resolved=false`, counts 0); the pre-send installed-pool
snapshot is authoritative, matching the Plan-230 observation.

Low: OCMOSJ `Dispatching message to` INFO never appears in the
scanned files (file-log INFO blindness); the `client.dispatchTime`
lifetime count carries the dispatch proof instead, and scratch
stays supporting-only.

Info: the helper returns before the router dispatches (post-
immediate gateway counts stay zero while the window advances),
which the first implementation SHA mis-modeled as an isolated
micro-epoch; corrected to window attribution with lane-quiet
premises plus correlated-log priority, with the micro advance kept
as an airtight fast path.

## Roadmap and unblock audit

Plan 231 is formally closed at the exact
`P231-C-TARGET-IBGW-NOT-INSTALLED` boundary. The unblock audit
examined every registered plan listing Plan 231 as a dependency:

```text
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = passed-m6-java-reverse-delivery-tunnel-dispatch-attribution-with-target-ibgw-not-installed-boundary
plan_201 = blocked-pending-lease-gateway-corrective-after-plan231-target-ibgw-attribution
plan_204 = blocked-on-m6-java-second-family-closure-pending-lease-gateway-corrective-after-plan231
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = none (narrow lease-gateway fixture corrective recommended;
  registers under this lane when approved; no production change authorized)
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

- Plan 201 remains blocked: its Plan-231 hard dependency is closed
  (post-`ACCEPTED` attribution complete with an exact stage), but
  Java second-family closure still requires actual reverse
  delivery, which needs the narrow lease-gateway fixture corrective
  identified above. Its blocker is narrowed from generic Plan-231
  pendency to that exact corrective. Re-running 201's lane without
  it would only reproduce the stop, so no successor is registered
  here; the corrective registers under this lane when approved.
- Plan 204 remains blocked on that same independent M6 closure; its
  M10 product/application authority (Plans 213–215) is unchanged
  and closed. Plan 231 is reference-topology-only and does not
  alter M10 authority.
- Plan 205 remains retained/deferred: the observed boundary is at
  the Java-side lease/forwarding layer below exploratory
  establishment and below client-tunnel paired selection — still
  below any SAM bridge.
- No plan is unblocked to ready: every ready-gated item still has
  an unclosed hard dependency (reverse delivery itself).

## Registration basis (retained)

Plan 230 closed as:

```text
passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
```

Its deepest run proved the corrected Java profile/bootstrap path,
genuine non-zero exploratory tunnels, one-hop client tunnels through
C, target LS2 resolution, and digest-matched i2pr -> Java raw
Destination delivery. The tracked Java -> i2pr reverse send produced
`STATUS_SEND_ACCEPTED` but no matching payload in the frozen
45-second window and no later terminal status.

Exact-pinned Java I2P 2.13.0 source review further narrows that
boundary:
`ClientMessageEventListener.handleSendMessage()` calls
`distributeMessage()`, the client message pool runs OCMOSJ inline,
OCMOSJ runs its `DispatchJob` inline, and `DispatchJob` calls
`TunnelDispatcher.dispatchOutbound(...)` before returning. Only after
`distributeMessage()` returns does `ackSendMessage()` emit
`STATUS_SEND_ACCEPTED`.

Therefore Plan 231 did not investigate whether OCMOSJ was entered.
It attributed the post-dispatch path:

```text
Java A outbound gateway enqueue
 -> Java C one-hop outbound endpoint
 -> selected target lease gateway / inbound gateway
 -> emitted TunnelData
 -> i2pr exact inbound tunnel
 -> tunnel recovery
 -> Garlic decode
 -> Destination payload
```

## Current authority (superseded by closure above)

```text
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = registered-ready-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective

plan_201 = blocked-pending-plan231-reverse-delivery-tunnel-dispatch-attribution-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan231
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 231-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

## Authorized work (executed; retained for traceability)

Plan 231 added read-only test/harness observability sufficient to
classify:

- exact Java A outbound gateway enqueue/drop;
- exact Router-C outbound-endpoint processing for the installed
  one-hop client tunnel;
- exact target lease gateway / inbound-gateway processing;
- next-hop TunnelData emission toward i2pr;
- i2pr exact target TunnelData receipt;
- tunnel recovery, Garlic decoding, Destination dispatch, and
  payload digest.

It did not tune the Plan-230 fixture (beyond the two documented
observability corrections: 8-field snapshot parity and stock
`stat.full`, neither of which changes protocol, topology, or
timing windows) and changed no production behavior.

## Prohibited shortcuts (all honored; retained for traceability)

No Java patching/reflection/private mutation; no direct queue/
tunnel/NetDB/profile injection; no timeout inflation; no topology/
profile/tunnel-policy changes; no public I2P/reseed/VMComm; no
`netDb.alwaysQuery`; no global counter used as the sole
target-message proof (exact tunnel-ID matches required at every
stage); no unrelated TunnelData accepted as the target; raw logs
remain scratch-only.
