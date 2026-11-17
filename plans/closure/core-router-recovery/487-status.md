# Plan 487 — Reverse-leg DATA recovery, receiver-ACK contract correction, and §6 lifecycle proof: `passed`

Status: **passed**
Date: 2026-10-19 (registered), 2026-10-20 (closed)
Subsystem: core-router-recovery
Predecessor: Plan 478 (active parent), Plans 485/486 (absorbed)
Owner: core-router-recovery
Disposition: passed — the three gates Plan 487 carried are closed (the reverse
external SAM STREAM DATA phase, the §6 live lifecycle recovery, and the Plan 478
product close-out). No support-inventory, conformance, capability, or
advertisement change; the Plan 433/439 gates are untouched.

## Scope

Plan 487 is the corrective pass registered on 2026-10-19 to carry the three
gates that Plan 478 could not close: the reverse external SAM STREAM **DATA**
phase, the **§6 live lifecycle recovery**, and the product close-out. It
corrects the receiver-ACK contract, adds the four-stage DATA-phase
attribution the reverse stall needs, repairs the two floor reds that blocked
any close-out, and settles the §6 governing semantics.

Plan 487 does **not** renumber or supersede Plan 478. Plan 478 remains the
active parent and closes only after Plan 487 passes, with its own closure
record, registry transition, and unblock audit.

## Baseline at registration

- Plan 478: `in-progress-forward-and-reverse-handshake-proven-reverse-data-and-lifecycle-recovery-open`.
- Reverse external SAM STREAM DATA: failing with stock
  `packet was not ACKed after 10 attempts, terminate` and driver
  `short stream read (0/2328)` after ~90.3 s; the close receipt was the
  all-zero torn-down shape (`bridge_present=0`).
- Floor reds on the registration tree:
  `check-els2-live-lane-evidence.sh` (Plan 393 cancellation count) and
  `cargo deny check advisories` (rustls 0.23.45: RUSTSEC-2026-0346/0347/0348).

## Implementation commits (branch `work/plans-477-480-router-recovery`)

- `c343d2e5` — implementation: the receiver-ACK contract correction, the
  four-stage DATA-phase attribution, the routing-peek fix, the margin-driven
  lease rotation, the Plan 393 publication-intent reservation, the rustls and
  zip advisory bumps, ADR 0040, the settled-decision entry in
  `specs/protocols/07-streaming.md`, the `docs/architecture/i2pr-client.md`
  update, the §6 lane, and the lane's offline controls.
- The closure commit — this record, Plan 478's closing entry, the registry
  rows, the roadmap §7 rows, the two plan documents' status lines, and the
  point-in-time research input. It carries no production change.
- Predecessor commits already on the branch: `f7403432` (Plan 486 closure),
  `d255a1ff` (Plan 486 implementation), plus the Plan 485/484/479/480/482/481/477 records' own commits.

## Requirement-to-evidence matrix

| Plan WP | Requirement | Evidence |
| --- | --- | --- |
| WP1 | The receiver-ACK contract matches both pinned references (acknowledge on transport state, never on local application drain), and the inbound bound survives the correction | `ack_is_emitted_while_the_application_has_not_drained` (fail-before), `over_cap_connection_still_emits_the_standalone_ack`, `send_data_piggybacks_the_real_ack_while_over_cap`, `over_cap_inbound_payload_is_dropped_and_the_peer_is_choked`, `immediate_ack_request_is_honoured`, `ack_only_packet_never_schedules_an_ack`; both probes executed and reverted (inverting the new assertion fails; re-introducing the Plan 152 D1 snooze fails) |
| WP2 | A four-stage DATA-phase attribution (manager arrival → ACK generation → ACK egress → delivered) that can classify a live stall | `DataPacketIngressAccounting::data_arrived` (manager arrival), `AckGenerationAccounting` (generated/transmitted/`no_ack_emitted`/`generated_after_drop`), `OutboundPacketKind` (producer-owned classification), `ack_egress_sent`/`ack_egress_dropped` at the send path and the single drop-before-send funnel, the live one-shot receipt `stream_accept_data_phase_stalled`, and the shared writer `log_data_phase_stall_attribution` used by both drivers |
| WP3 | A discriminating lane, and the reverse DATA root cause | The daemon's inbound routing peek reported `receive_stream_id` (the **peer's** id) where the manager resolves on `send_stream_id`; the peek now reports `peek.send_stream_id`. The first regression test was degenerate (both ids start at `0x8000_0000`); it now burns one originator id and asserts the ids differ, and it fails on the pre-fix field. Live: both directions digest-matched in the pinned lane |
| WP4 | The §6 semantics are settled, and the publish-and-acknowledge loop is real | ADR 0040 (`docs/adr/0040-stream-recovery-across-tunnel-rebuild-and-router-restart.md`) and `specs/protocols/07-streaming.md` S5; the coordinator's per-destination publication ledger (`admit_ls2_publication`, `note_ls2_publication_sent`, `correlate_ls2_publication_status` attributing the ack to the **inbound tunnel** it arrived on, `publication_acknowledgements`, `reap_unacknowledged_ls2_publications`); the publication's message id **is** the reply token and the reply route is the destination's own inbound tunnel plus its gateway; live proof: `ls2_publication_awaiting_acknowledgement` → `ls2_publication_acknowledged` on an inbound tunnel, 79 ms after the publish |
| WP5 | Plan 393's three-failure-class cancellation property is true (not relaxed in the script) | `reserve_ls2_publication()` (capacity reserved **before** the record is composed) and `attach_ls2_publication()` in `crates/i2pr-daemon/src/destination_tunnels.rs`; three distinct release sites in `crates/i2pr-daemon/src/service_product.rs` (compose, request-construction, delivery), each with its own `PublicationFailureClass`; `bash scripts/check-els2-live-lane-evidence.sh` is green with no script change |
| WP6 | The advisory reds are cleared | rustls `0.23.45 → 0.23.46` (lockfile-only, inside the existing caret range; clears RUSTSEC-2026-0346/0347/0348/0349); `zip 8.6.0 → 9.0.0` with the recorded **major**-bump deviation (RUSTSEC-2026-0367/0368/0369); `cargo deny check advisories bans sources` is green; the reference pins did not move |
| F1 | A LeaseSet2 published to one floodfill is not resolvable | The publication fans out to the three nearest floodfills under a hard ceiling with a unit test; live: the destination is resolvable after publication and after the restart |
| F4 | The approaching-expiry rotation must not re-sign an unchanged lease set, and a lease-refresh failure must not be service-fatal | `inbound_lease_covering_count` / `inbound_replenishment_deficit` (margin-aware, target+1 headroom clamped to 8), no re-sign of an unchanged lease set, destination-scoped lease-refresh failures with rate-limited `destination_inbound_replenishment_due` / `destination_build_submission_failed` receipts; ADR 0040 amended. Live: the deficit opened exactly 120 s before the lease end, the replacement was built and published, and the publication was acknowledged in 4 ms on an inbound tunnel with no `runtime.service.failed` |
| F5 | The attribution must run in the mode the product and the lane actually run | `crates/i2pr-daemon/src/service_tunnels.rs` records the established marker and emits the attribution; `crates/i2pr-daemon/src/sam.rs` emitters are refactored onto the shared writer; the accept site records the marker where the transition happens. Live: a fresh post-restart session produced the terminal receipt classified `data-never-arrived` |
| WP7 | The close-out is complete and auditable | This record; `plans/closure/core-router-recovery/478-status.md` (closing entry); `plans/registry.md` (rows 478/487 closed, recently-closed table); `plans/subsystems/core-router-recovery-roadmap.md` (§7 rows 478/487 advanced); the unblock audit below; the routine floor results in the Verification section |

## Work packages and outcomes

### WP1 — receiver-ACK contract correction — **done**

The Plan 152 D1 acknowledgement gate coupled the acknowledgement to local
application drain: a standalone ACK was snoozed while the connection held
more undrained bytes than its cap, and a piggybacked DATA carried
`FLAG_NO_ACK`. That diverges from the reference contract, read at the exact
pins:

- i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`),
  `libi2pd/Streaming.cpp:256-269` — an overflow packet is deleted, the peer is
  choked, and `SendQuickAck()` is **still** called; `:289-300` — the
  acknowledgement is scheduled on receipt.
- I2P Java 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`),
  `.../impl/ConnectionPacketHandler.java:145-161` — an inbound-buffer overflow
  drops the payload and calls `ackImmediately()`; `:180-236` — the
  acknowledgement is scheduled when `messageReceived` reports a new
  in-sequence packet.

Changes:

- `crates/i2pr-client/src/streaming/manager.rs` — the three gates removed; the
  standalone ACK is emitted on schedule, an immediate-ACK request is honoured,
  and a DATA packet always piggybacks the real `ackThrough`/NACK view.
- The bound is preserved by the reference's own shape: an in-order payload
  that would exceed `delivered_cap_bytes()` is **not admitted** to the receive
  window (so the acknowledgement ceiling does not advance and the peer
  retransmits), and the peer is **choked** with the reference's
  `DELAY_REQUESTED` vocabulary.
- `crates/i2pr-proto/src/streaming/packet.rs` — `DELAY_CHOKE = 61000`, the
  only value both references recognise as a choke (i2pd
  `DELAY_CHOKING_JAVA`; Java `SEND_DELAY_CHOKE`, with Java's choke threshold
  at `MIN_DELAY_CHOKE = 60001`).
- The delivered-byte accounting is retained as the **application-visible
  backpressure signal**. Runtime audit: the SAM raw-stream pump
  (`crates/i2pr-daemon/src/sam/raw_stream.rs`) and the service-tunnel pump
  (`crates/i2pr-daemon/src/service_tunnels.rs`) both drain eagerly through
  `drain_delivered_for`; `pending_bytes_for` is read only as a diagnostic
  receipt field (`crates/i2pr-daemon/src/sam.rs`). **No runtime caller relied
  on the acknowledgement being withheld.**

Tests: `ack_is_emitted_while_the_application_has_not_drained` (the fail-before
test), `over_cap_connection_still_emits_the_standalone_ack`,
`send_data_piggybacks_the_real_ack_while_over_cap`,
`over_cap_inbound_payload_is_dropped_and_the_peer_is_choked`,
`immediate_ack_request_is_honoured`, `ack_only_packet_never_schedules_an_ack`.
Both required probes were executed and reverted: inverting the new assertion
fails, and re-introducing the Plan 152 D1 snooze fails.

### WP2 — four-stage DATA-phase attribution — **done**

- `crates/i2pr-client/src/streaming/events.rs` —
  `DataPacketIngressAccounting::data_arrived` (the **manager-arrival** stage)
  and the new `AckGenerationAccounting` (`generated`, `transmitted`,
  `no_ack_emitted`, `generated_after_drop`).
- `crates/i2pr-client/src/streaming/transport.rs` — `OutboundPacketKind`, a
  **producer-owned** packet classification, so the runtime attributes an
  acknowledgement to the egress stage without re-decoding wire bytes.
- `crates/i2pr-daemon/src/service_tunnels.rs` and
  `crates/i2pr-daemon/src/sam/raw_stream.rs` — the **ACK-egress** stage:
  `ack_egress_sent` and `ack_egress_dropped`, counted at the transport send
  path and at the single drop-before-send funnel.
- `crates/i2pr-daemon/src/sam.rs` and `crates/i2pr-daemon/src/sam/streams.rs` —
  the receipt. The **close** receipt carries the four stages, but it cannot
  classify the stall: the session teardown removes the destination bridge
  before the pump finishes, so every delta reads zero by construction. A
  **live** one-shot receipt (`stream_accept_data_phase_stalled`) is therefore
  emitted from the destination driver tick while the state is still readable.

Each stage is counted by its own counter; no stage is inferred from another
stage's count. `ack_no_ack_emitted` is a permanent negative control.

### WP3 — discriminating lane and the reverse DATA root cause — **done**

The live attribution is in place, the root cause is a **wire-field
misread** in the daemon's inbound routing peek, and the reverse external
SAM STREAM DATA phase now carries a digest-matched payload in the pinned
lane. See the "Reverse DATA root cause" section below.

### WP4 — §6 lifecycle semantics and the publish-and-acknowledge loop — **done**

The governing semantics were an unresolved open decision
(`specs/protocols/07-streaming.md:132`). They are now settled:

- `docs/adr/0040-stream-recovery-across-tunnel-rebuild-and-router-restart.md`
  — ADR 0040.
- `specs/protocols/07-streaming.md` — a new "Settled decisions / S5" section
  recording the decision, and open decision 5 marked settled.

The **publish-and-acknowledge loop** is now implemented and proven live
against the pinned i2pd 2.61.0 floodfill:

- `crates/i2pr-daemon/src/destination_tunnels.rs` — the coordinator gained a
  per-destination **publication ledger** (`PublicationLedgerEntry`) with
  `admit_ls2_publication()` (the three publication bounds), `note_ls2_publication_sent()`,
  `correlate_ls2_publication_status()` (now attributing the acknowledgement to
  the **inbound tunnel** the status arrived on), `publication_acknowledgements()`
  and `reap_unacknowledged_ls2_publications()` (the bounded reaper).
- `crates/i2pr-daemon/src/service_product.rs` — the publication now carries the
  **reply token** and the **reply route** (one of the destination's own inbound
  tunnels plus that tunnel's gateway). The publication's message id *is* the
  reply token, so the floodfill's `DeliveryStatus` correlates back to exactly
  that publication. The coordinator entry is **kept** after the store reaches
  the wire and is released by the acknowledgement or by the bounded reaper.
- The inbound path correlates the status instead of dropping it: a
  `DeliveryStatus` on an inbound tunnel is matched against the pending
  publications and the acknowledgement is recorded with the tunnel it arrived
  on.

The live proof (pinned i2pd 2.61.0, four loopback floodfills):

```text
19:57:08.539916Z  LS2 publication emitted; awaiting the correlated DeliveryStatus
                  publication_reply_token=3258996996 published_seconds=1792439827
                  oldest_lease_expiry_ms=1792440367000
                  transition="ls2_publication_awaiting_acknowledgement"
19:57:08.618545Z  LS2 publication acknowledged by a correlated DeliveryStatus on an
                  inbound tunnel (counts only) publication_reply_token=3258996996
                  inbound_tunnel_id=4214374648 elapsed_ms=0
                  transition="ls2_publication_acknowledged"
```

The reference contract was read at the pin before the change: i2pd
`libi2pd/NetDb.cpp:804-848` answers a store that carries a nonzero reply
token with a `DeliveryStatus` whose message id **is** that token, addressed to
the supplied gateway and tunnel, so the status returns **through an inbound
tunnel**; `libi2pd/I2NPProtocol.cpp:124-132` writes the token into the
status's message-id field.

**The reply token is scoped to one recipient.** A floodfill answers a store
that carries a nonzero reply token, so a token on every copy would make one
publication produce one acknowledgement per recipient, and the surplus
`DeliveryStatus` messages would arrive as unmatched inbound messages. The
fan-out therefore puts the token and the reply route on the **nearest**
candidate only; the other copies carry no reply request.

The §6 **live lifecycle recovery** qualification is described in its own
section below.

### WP5 — Plan 393 publication-intent cancellation — **done**

`check-els2-live-lane-evidence.sh` was red because the tree had one
`.cancel_ls2_publication(request_id)` site where the checker requires three.
The Plan 478 record had noted the guard as "stale for the restructured code".
Plan 487 made the **semantic property true** rather than relaxing the script:

- `crates/i2pr-daemon/src/destination_tunnels.rs` — the coordinator gained
  `reserve_ls2_publication()` (capacity reserved **before** the record is
  composed) and `attach_ls2_publication()` (the same validation as
  `begin_ls2_publication`, applied to a reservation). The intent is an enum
  (`Reserved` / `Composed`).
- `crates/i2pr-daemon/src/service_product.rs` — the product publication path
  reserves the intent first, and the **compose**, **request-construction**,
  and **delivery** failure classes each release it at their own site, with an
  individually attributable `PublicationFailureClass`.

`bash scripts/check-els2-live-lane-evidence.sh` is green.

### WP6 — advisory reds — **done, with a recorded deviation**

- rustls: `cargo update -p rustls --precise 0.23.46`. Lockfile-only, inside
  the existing `version = "0.23"` caret range, no manifest change, no feature
  change, no new crate, licence unchanged. This clears RUSTSEC-2026-0346,
  0347, 0348 **and 0349** (the last one was not in the Plan 477 record).
- **Deviation, recorded for review:** the plan named only the rustls bump,
  but the live advisory set also carried RUSTSEC-2026-0367/0368/0369 on
  `zip 8.6.0` (all three concern archive *extraction*: SUID/SGID preservation,
  symlink targets outside the destination directory, and permissions applied
  through symlinks). The fix is `zip >= 9.0.0`, a **major** version bump, so
  the manifest was changed (`zip = "9"`, `zip-stored = { package = "zip",
  version = "9" }`) and the API change was applied: `ZipFile::name()` is now
  fallible, and `FileOptions` gained a second lifetime parameter. The bump
  was chosen over a `cargo deny` ignore because it removes the advisory
  rather than suppressing it. Review recorded: purpose (advisory fix);
  transitive impact (`nix 0.31.3` and `cfg_aliases 0.2.2` enter the lockfile
  but are **not activated** at the enabled feature set — `cargo tree -p zip`
  shows neither); `unsafe` exposure (unchanged for the enabled feature set);
  features (unchanged, `default-features = false`); licence (unchanged).
  The exact reference pins (i2pd, Java I2P, go-i2cp) did not move.

`cargo deny check advisories`, `cargo deny check bans` and
`cargo deny check sources` are green.

## Reverse DATA root cause

The first lane run carrying the live four-stage attribution produced the
receipt. The root cause is a **wire-field misread in the daemon's inbound
routing peek**, not a transport defect and not an acknowledgement defect.

The daemon classifies an inbound payload before routing it, using a bounded
peek over the authenticated inner I2NP message
(`inbound_payload_shape` in `crates/i2pr-daemon/src/sam/streams.rs`). The peek
distinguishes the originator SYN from an **established-phase** packet
(DATA, ACK, CLOSE, RESET, or a SYN-ACK) and, for the established phase,
reports the packet's stream id so the router can find the manager that
**owns** the connection.

The peek read the wrong wire field. It reported the packet's
`receive_stream_id` as the *local* receive stream id. But `receive_stream_id`
carries the **peer's** id; the id the peer uses to address **us** is the
packet's `send_stream_id`. The manager resolves an established-phase packet
on the **send** field — `inbound_by_stream` and `outbound_by_stream` are both
keyed by `local_receive_stream_id`, and the lookup is
`inbound_by_stream.get(&peek.send_stream_id)`. The reference agrees: a peer
sends `sendStreamId = <our receive id>` and `receiveStreamId = <the peer's own
id>`.

Every owner lookup therefore missed, every established-phase packet fell back
to profile routing, and the inbound DATA of an accepted (server-role)
connection was delivered to the **canonical** manager — where no application
reads it. The payload was never delivered, the connection was never
acknowledged, and the stock peer exhausted its ten retransmissions and
terminated the stream with `packet was not ACKed after 10 attempts`.

Fix (`crates/i2pr-daemon/src/sam/streams.rs`): the established-phase branch
reports `peek.send_stream_id`. The waiter-scoped rule of Plan 486 remains the
rule for packets that belong to no connection yet.

**The probe that distinguishes the two hypotheses.** The first version of the
regression test was degenerate and passed for the wrong reason: the test
built its DATA packet with the two stream ids **swapped**, and the mirror's
allocated receive id and the origin's chosen id both start at `0x8000_0000`,
so the swapped fields were indistinguishable. The test now burns one
originator stream id first, so the two ids differ, and it asserts that they
differ. With that in place:

- `established_phase_shape_reports_the_send_stream_id` fails on the
  pre-fix field (`left: 858997828` — the peer's id — `right: 286335522` — our
  local receive id).
- `established_phase_data_reaches_the_manager_that_owns_the_connection` fails
  on the pre-fix routing (`assertion failed: matches!(outcome,
  DequeuedPayloadOutcome::StreamingAccepted)` — the DATA is rejected outright)
  and passes after it; the bytes then drain through
  `receiver_streaming_mut().drain_delivered_for(connection_id)`.

**Live proof.** The pinned lane now passes in **both** directions:

```text
i2pr-to-i2pd=passed bytes=1816 sha256=e6426bb5b5027844baf5f4eec63d06f03cdbe7e748191b233abecfff60e8330e
i2pd-to-i2pr=passed bytes=2328 sha256=fbabac14c277d19f915326dc89009eec362175538f235299c21b146005a59232
```

and the stock peer's log records **no** `packet was not ACKed after 10
attempts` in the run (the count is 0). The daemon's close receipt now reads
real values instead of the all-zero torn-down shape:

```text
data_observed=4 data_accepted=4 data_rejected=0 data_payload_bytes=2328
canon_data_observed=0 canon_data_accepted=0 canon_data_rejected=0
garlic_authenticated=4 garlic_rejected=0
payloads_dequeued=4 streaming_accepted=4
connection_state_code=1 undrained_bytes=0 reset=0 bridge_present=1
manager_arrived=2 ack_generated=2 ack_transmitted=3 ack_no_ack_emitted=0
ack_egress_sent=1 ack_egress_dropped=0
```

`canon_data_observed=0` with `data_observed=4` is the routing fix, visible in
the receipt: the DATA reached the **mirror** that owns the accepted
connection, and no DATA was misrouted to the canonical manager.
`ack_generated=2` / `ack_transmitted=3` / `ack_egress_sent=1` is WP1's
acknowledgement contract, visible in the same receipt.

## Product findings the §6 qualification produced

The §6 qualification is a live external lane, so it produced findings that were
not visible from the source or the unit tests. Three were fixed in this plan
(F1, F4, F5); three are recorded for a future plan-of-record (F2, F3, F6).

### F1 — a LeaseSet2 published to one floodfill is not resolvable (fixed here)

A stock peer resolves a LeaseSet2 by asking the floodfills **it** selects for
the routing key, and i2pd 2.61.0 fails the lookup outright when none of the
floodfills it asked holds the record — `STREAM STATUS RESULT=CANT_REACH_PEER
MESSAGE="LeaseSet not found"`. The publication path selected a **single**
floodfill (`.first()` of the routing-key candidates), so the destination was
resolvable only by the peers that happened to ask that one floodfill.

Observed in the pinned lane (four loopback floodfills): the store landed at
one floodfill, and the peer asked three; all three answered "not found":

```text
22:57:57@028  d.log  NetDb: Store request: LeaseSet2 of type 3 for v4d3kp5…
22:58:06@322  f.log  NetDb: Requested LeaseSet not found for v4d3kp5…
22:58:06@632  n.log  NetDb: Requested LeaseSet not found for v4d3kp5…
22:58:06@847  c.log  NetDb: Requested LeaseSet not found for v4d3kp5…
```

**Fix.** `crates/i2pr-daemon/src/service_product.rs` — the publication now
fans the same store (same record, same reply token) out to the **nearest
`LS2_PUBLICATION_FLOODFILLS = 3`** floodfill candidates, the same shape the
creator-RouterInfo publication already used for the same reason. The nearest
candidate is required; the rest are best-effort, because one delivered copy
is enough for the record to exist in the network. The coordinator's ledger is
unchanged — it still records one publication, one reply token, one
acknowledgement — and the fan-out is bounded by a hard ceiling with its own
unit test (`ls2_publication_fanout_tests`).

### F2 — the receiver's session dies with its control socket's silence (recorded, not changed)

Plan 485 bounded a SAM session's life by the control socket's 60 s command
timeout: a control socket that sends no command for 60 s is treated as gone,
and the session — with every stream attached to it — is torn down with it.
The lane measured it exactly: the accepted stream's session was torn down
59.998 s after its last control command.

This is Plan 485's recorded decision, so it is **not** changed here; changing
a closed plan's session-lifetime policy needs its own plan-of-record. The
lane therefore keeps the control socket busy with a periodic no-op command
(`ControlSocketHeartbeat`) while it waits on a long-lived stream, which is
what the SAM convention already asks of a client that holds a session.

### F3 — a controlled transit-peer loss destroys the live path (recorded, not changed)

The Plan 478 §6 gate allows "one tunnel expiry/replenishment **or**
controlled peer loss". The controlled peer loss was measured first, and it is
**not survivable in this stack**:

- The lane identifies the peer that carries the destination's inbound tunnel
  from the router's own acknowledgement receipt (the stock peers log the
  tunnel ids they carry), and kills a **different** transit-only peer so the
  loss does not simply destroy the path. The killed peer's tunnels failed on
  the stock side (`Tunnel: Tunnel with id … expired or failed`) and the
  stock peer rebuilt them.
- The stream still did not resume: the stock peer only resends a stream when
  the **remote lease changes**, and the router did not republish, because its
  destination inbound pool has no transit-hop failure detection — a dead
  transit hop is invisible to the router until the tunnel expires.

So the §6 case-1 experiment uses the option the gate also allows: the
destination's **lease rotation / tunnel-pool replenishment**, which the
router performs itself. The peer-loss observation is recorded here so the
gap is not lost: *a router whose destination tunnels die with a transit hop
does not replenish them until the lease expires.* That is a named product
gap for a future plan-of-record, not a Plan 487 claim.

### F4 — the approaching-expiry rotation re-signed an unchanged lease set and failed the `ssu2-router` service (fixed here)

This is the finding that blocked the §6 rotation, and it is a **product**
defect with three linked parts. The live lane measured each of them.

**The symptom.** The lane's rotation phase never completed: the accept side
lost the stream with a short read, and the daemon's own log showed the router's
transport service going down in the middle of the wait:

```text
00:13:50.889735Z  WARN i2pr_daemon: destination-group pool advancement failed
                  reason=service router provisioning failed: group pool advance:
                  destination lease set rejected: lease end 1792455320 is not
                  after published 1792455320
                  transition="destination_group_pool_advancement_failed"
00:13:51.692134Z  INFO i2pr.runtime: event="runtime.service.failed"
                  service=ssu2-router classification=Optional lifecycle=Failed
                  restart_attempts=0 failure_category=Some(ServiceFailure)
```

**Part one — the rotation re-signed a lease set it could not improve.** The
rotation margin (120 s) was a *second* regeneration trigger: when the current
record's earliest advertised lease fell inside the margin, the lifecycle
re-signed **the same** lease set with a newer `published` timestamp. A
re-signed identical lease set cannot extend the record — the advertised lease
still dies at the same second — so the only effect was churn. The lane
measured five publications inside 21 s, **none** of which advanced the oldest
advertised lease, which is exactly what WP4's rotation gate rejects.

**Part two — the churn ended in a hard rejection.** `published` advances
monotonically, so it eventually reached the lease end. `build_signed_lease_set2`
rejects a record whose lease cannot outlive its own `published` timestamp
(`LeaseAlreadyExpired`), and that error propagated out of the destination's
`advance_time` → the group advance → `owner.advance_groups()` → the
`ssu2-router` service was **failed**. One destination's lease hiccup took the
whole router's transport down, mid-stream.

**Part three — the pool had no replacement to offer.** The root cause of part
one is that the destination's inbound pool was replenished on a **count**
deficit only: a tunnel was replaced when it *expired*, never before. The
advertised lease ends at `tunnel expiry − 60 s`, so at the rotation margin the
pool still held only the expiring tunnel — the rotation had nothing better to
publish, which is why it churned.

**The fix**, in three parts that belong together:

1. `crates/i2pr-client/src/leaseset.rs` — the lifecycle regenerates **only
   when the pool's usable inbound set changed**. The approaching-expiry margin
   is the deadline the pool's replenishment works against, not a second
   regeneration trigger; the rotation follows the changed tunnel set. The
   `LeaseSetRotationCause::ApproachingExpiry` cause now names the
   margin-driven rotation (the replacement that landed inside the window).
2. `crates/i2pr-client/src/pool.rs`, `registry.rs`, `config.rs` — the
   replenishment counts the tunnels whose advertised lease still **covers the
   rotation horizon** (`inbound_lease_covering_count`), so the replacement is
   built while the expiring tunnel is still alive, and the destination's
   inbound pool carries **one replacement headroom** past its target
   (`pool_inbound_capacity`) so that replacement can be admitted while the
   expiring tunnel is alive. The deficit is bounded by the pool's free
   capacity, so a build the pool would reject is never attempted.
3. `crates/i2pr-daemon/src/service_product.rs` — a lease-set refresh failure is
   **destination-scoped**: it is logged, recorded on the publication snapshot
   (`lease_refresh_failures`), retried on the next tick, and it never fails
   the router's transport service.

**The regression coverage** is deterministic and local:
`approaching_expiry_does_not_resign_an_unchanged_lease_set`,
`the_pool_replacement_inside_the_rotation_window_is_the_margin_rotation`,
`a_rotation_never_signs_a_record_whose_lease_cannot_outlive_published`
(`crates/i2pr-client/src/leaseset.rs`),
`inbound_covering_count_withdraws_a_tunnel_inside_the_rotation_window` and
`inbound_covering_count_ignores_withdrawn_and_failed_tunnels`
(`crates/i2pr-client/src/pool.rs`), and
`the_inbound_deficit_opens_when_the_lease_enters_the_rotation_window`
(`crates/i2pr-client/src/registry.rs`). The lane itself now asserts that no
router service failed during the run.

**The bounded-resource consequence is recorded, not hidden.** The
destination's inbound pool holds `effective target + 1` tunnels instead of
`effective target` (three instead of two for the balanced policy, and it
stays inside the shared pool's ceiling of eight). Plan 296's
"registration past base plus backup must fail" bound was therefore amended:
the pool now admits **one** tunnel past base plus backup — the replacement
headroom — and rejects the next. The standby accounting follows Plan 296's
own definition (tunnels held beyond the base target), so the headroom tunnel
counts as a standby and the promotion counter can be one higher than before.

### F5 — the four-stage DATA-phase attribution was dead in the mode the lane runs (fixed here)

**The symptom.** After F4 was fixed, the §6 lane's *first* data-phase round
trip failed: the accept side read 0 of 2328 bytes, and the router's own
receipts could not say why. The lane's diagnostic carried no classification
at all:

- no `stream_accept_data_phase_stalled` receipt — the live four-stage
  attribution never fired, although the lane waited far past the window it
  bounds;
- the close receipt read `data_observed=0 … bridge_present=0` — every stage
  counter was zero **by construction**, because the session teardown removes
  the destination bridge before the raw pump finishes.

A stall the router cannot classify is a stall the reader has to guess at, and
this plan's own §6 gate depends on that classification.

**The root cause.** Plan 487's WP2 instrument is wired into the **SAM-side**
destination driver (`run_destination_driver` in
`crates/i2pr-daemon/src/sam.rs`), and that driver only runs in **local-only**
mode. The product's SAM destinations are **router-network** destinations,
whose streaming state is owned by the **service-side** delivery driver
(`run_service_delivery_driver` in `crates/i2pr-daemon/src/service_tunnels.rs`).
Two consequences followed, and they are the same defect seen twice:

1. `note_established_at` was never called for a router-network destination, so
   `established_since_ms` stayed `None`. Both the live and the terminal
   four-stage snapshots return `None` without it, so the instrument was
   *structurally* dead in the production mode — not merely quiet.
2. The close receipt could not cover the gap: the teardown removes the bridge
   before the pump finishes, so the close receipt's counters are zero exactly
   in the case it was written to explain.

**The fix**, in three parts:

1. `crates/i2pr-daemon/src/service_tunnels.rs` — the service-side driver
   records the established marker for the destination it owns and emits the
   live four-stage attribution. The attribution is now emitted by the mode
   that owns the destination, in both modes.
2. `crates/i2pr-daemon/src/sam/streams.rs` — the receipt is written **once**
   (`log_data_phase_stall_attribution`) and called from both drivers, so the
   two modes cannot drift apart in field set, wording, or stage semantics.
3. `crates/i2pr-daemon/src/sam.rs` — the two SAM-side emitters call the
   shared logger instead of repeating the field list.

**The instrument is now proven live.** In the router-network mode the
receipt fires and classifies: a fresh session after a daemon restart whose
DATA never reached the router produced

```text
STREAM ACCEPT data phase stalled with no DATA at the owning manager; live
four-stage attribution (counts only) app_bytes_drained=0 mirror_observed=0
mirror_accepted=0 … canonical_observed=0 canonical_arrived=0 …
```

— the router's own classification, *data-never-arrived*, on the mode the
production lane runs. The close receipt for the same stream read
`bridge_present=0`, which is exactly the vacuity the live receipt exists to
cover.

**The lane harness** learned to use the instrument instead of guessing:
`read_data_phase_stall_receipts` reads the receipt's stage counts and
`classify_data_phase_stall` names the cause the router attributed — the DATA
never arrived, the DATA arrived in the wrong manager, the DATA arrived without
an acknowledgement, or the acknowledgement never left the router. A data-phase
round trip is now bounded (90 s) so a stall the router has already attributed
fails the lane instead of waiting it out, and a data-phase failure with **no**
receipt says exactly that instead of implying the router had spoken. Six
offline controls pin the reader, the classification, and both failure
messages (101 tests in the lane's offline suite).

### F6 — the accept socket is one bidirectional channel, so the raw driver can consume the application's inbound bytes (recorded, not changed)

**The symptom.** One §6 run in two failed its *first* data-phase round trip:
the accept side read 0 of 2328 bytes, while the stock peer received the
payload back from the router. The other run passed. The lane's own reading
was part of the cause — it posted its accept-side read up to 50 ms after the
accept, so the payload could arrive, and be consumed, before the lane's read
was posted. The lane now arms that read at the accept, in the accept worker
thread, exactly as the reference driver does.

**What the product does.** A SAM accept socket is **one bidirectional TCP
channel**. The raw driver reads the *application's writes* from it and writes
the *bridge's delivered bytes* to it, while the application reads the peer's
bytes from the same socket. Nothing in SAM 3.1 separates the two directions,
so the driver and the application compete for the application's inbound bytes:
whichever of the two is scheduled first when the bytes arrive takes them.

**The pinned reference has the same shape.** At i2pd 2.61.0
(`635b013a612ff47278ef02acf8580a28e10e26c5`),
`libi2pd_client/SAM.cpp`'s `SAMSocket::Receive` posts a socket read and
re-posts it after every completion, while `SAMSocket::HandleI2PReceive`
writes the stream's data to that same socket. i2pr is therefore not worse
than the pinned reference, and no divergence from it is claimed.

**Why it is recorded and not changed here.** Separating the directions would
need a second channel, which SAM 3.1 does not have; changing the protocol's
wire shape is outside this plan's scope and would diverge from the pinned
references. The observable consequence is bounded and is now *named*: on an
accepted stream, a payload can be consumed by the router's own raw driver
when the application's read is not outstanding when the bytes arrive. An
application that reads promptly — the reference driver's shape — is not
exposed, and the §6 lane now measures the rotation and the restart on such
an application.

## §6 live lifecycle recovery — qualified in the pinned lane

The §6 gate is ADR 0040's two recovery cases, measured by
`tests/integration/core-router-recovery/run_plan487_lifecycle_recovery.py`
against the exact-pinned stock peer (i2pd 2.61.0,
`635b013a612ff47278ef02acf8580a28e10e26c5`) in the controlled four-peer
mesh. The lane's evidence file is
`target/interop/plan487-lifecycle-evidence/result.json`; it carries counts,
decisions, and payload digests only.

**The reverse DATA phase, in the same qualification.** The Plan 458 stage
runs first and unchanged in the same mesh, so both directions are proven in
the same run that carries the §6 claim: `i2pr-to-i2pd` 1816 bytes
`sha256:e6426bb5…`, `i2pd-to-i2pr` 2328 bytes `sha256:fbabac14…`, both
digest-matched, with zero `packet was not ACKed after 10 attempts` in the
stock peer.

**Case 1 — the destination's lease rotation (ADR 0040 case 1).**

| Receipt | Value |
| --- | --- |
| The lease rotated | `rotated: true` |
| The republished record's `published` advanced | `published_delta_seconds: 540` |
| The republished record's oldest-lease expiry advanced | `oldest_lease_expiry_delta_ms: 420000` |
| The rotated record was acknowledged by a correlated `DeliveryStatus` **on an inbound tunnel** | `rotation_acknowledgements: 4`, `acknowledged_on_inbound_tunnel: 4` |
| The live stream survived the rotation | `stream_survived: true` — the same 2328-byte payload, digest `sha256:9fe9224e…`, before and after |
| The router's services stayed up | `service_failures: {}` |
| The session's control socket stayed busy across the wait | `heartbeats_sent: 26`, `heartbeat_failures: 0` |

The rotation is margin-driven end to end: the margin-aware replenishment
opened the deficit 120 s before the advertised lease ended, the replacement
tunnel was built and installed while the old one was still alive, and the
republication was acknowledged **4 ms** after it was emitted.

**Case 2 — the daemon restart (ADR 0040 case 2).**

| Receipt | Value |
| --- | --- |
| The SAM connection closed with the daemon | `sam_connection_closed: true` |
| The persisted identity was unchanged | `persisted_identity_unchanged: true` |
| The exported router **hash** was unchanged | `router_hash_unchanged: true` |
| The mesh settled after the restart | `mesh_settled_after_restart: {settled: true, window_s: 10.0}` |
| A **fresh** session and destination were reachable again | `destination_reachable_after_restart: true`, attempt 1, no error, 5 publication acknowledgements |
| The post-restart payload digest matched the pre-restart one | `sha256:9fe9224e…` on both sides |

**What is not claimed.** The lane proves the two ADR 0040 recovery cases for
one destination in the controlled mesh. It does not prove a capability
promotion, a support-inventory change, or any behaviour beyond the tested
subset; `specs/support.toml`, Plan 433/439, and the advertisement rules are
untouched. F2, F3, and F6 are recorded above and are **not** closed by this
run: the session's control-socket bound (F2), a controlled transit-peer loss
(F3), and the accept socket's single bidirectional channel (F6) are the
router's own behaviour, not the lane's.

## Lane harness defects the §6 qualification exposed

The qualification is only as good as its reading of the router's receipts, and
six defects in the lane's own reading were found and fixed. All six are
covered by the new offline controls in
`tests/integration/core-router-recovery/test_plan487_lifecycle_receipts.py`
(108 tests in the lane's offline suite).

- **The receipt readers never matched a real daemon log line.** The daemon
  emits terminal colour escapes even when its stdout is a pipe, and the
  escapes land *inside* the tracing field pairs, so the coloured text never
  contains the contiguous substring `transition="…"`. Every receipt count
  therefore read **zero** — a false "no publication happened" claim, not a
  cosmetic bug. The readers now strip the escapes with the Plan 458 lane's
  canonical `strip_ansi` before matching.
- **A failed exchange did not say which endpoint lost the stream.** The
  accept side is the router's inbound stream and the connect side is the
  stock peer's outbound stream; the two failures mean different things, and
  an unnamed "short stream read" hid that difference exactly when it
  mattered. `exchange` now names the endpoint.
- **The driver could not classify two real failure classes.** The daemon's
  typed "router-backed SAM destination product is unavailable" (a startup
  race between the listener binding and the product attaching) fell through
  to `unclassified`, and the failure message was dropped entirely, so an
  unclassified failure was undiagnosable. The class is now named and
  retried within a bounded budget, and every `SESSION CREATE` failure
  carries its message.
- **The lane stopped the control-socket heartbeat before the post-rotation
  round trip.** Plan 485 bounds a session by its control socket: a control
  socket that sends no command for 60 s is treated as gone, and the session
  — with every stream attached to it — is torn down with it. The lane
  stopped the heartbeat as soon as the rotation was observed, so the
  acknowledgement wait and the post-rotation round trip ran against a
  session the router had already torn down, and the lane measured its own
  teardown instead of the rotation. The heartbeat now stays alive across
  the whole rotation phase, including the acknowledgement wait and the
  round trip.
- **The lane posted its accept-side read too late.** A SAM accept socket is
  one bidirectional channel that the router's raw driver also reads, so the
  application's read has to be outstanding before the peer's bytes arrive; a
  read posted late can find the channel already drained by the driver, and
  the payload then never reaches the application. The lane posted its read
  from the main thread, up to 50 ms after the accept, and lost one run in
  two to exactly that. The accept worker now arms the read the moment the
  socket is bound, and the connect side sends afterwards — the reference
  driver's shape.
- **The lane dropped the result it was about to write.** The lifecycle
  produced its evidence, and the lane then dropped it on the floor: the
  lifecycle's `run` did not return the result, so the write of the evidence
  file failed with `'NoneType' object does not support item assignment` —
  after the whole §6 qualification had already passed. The lifecycle now
  returns its result, and a failed lane records the exception's type and
  traceback in its attempt diagnostic, so a harness defect can never again
  present as a product failure.
- **The rotation's acknowledgement was gated on a count.** The lane waited
  for an acknowledgement *newer* than the count it had already read at the
  moment it observed the rotation — but the rotation's own publication is
  usually acknowledged before the lane observes the rotation, so the gate
  waited for an acknowledgement that could not arrive for another rotation
  window. The gate now names the rotated publication: the wait ends only
  when an acknowledgement *for that publication* has arrived, and a count
  alone cannot satisfy it.

**The lane's evidence is now self-diagnosing.** A data-phase failure carries
the router's own four-stage attribution and the stock peer's streaming
counts (never the peer's raw log: only counts leave the reader), and a
failure with no attribution says exactly that, so an unclassified stall is
never mistaken for a classified one. The round trip is bounded at 90 s, so a
stall the router has already attributed fails the lane instead of waiting
it out.

## Lock acquisition order and deadlock review

Plan 486 required a deadlock review for the seam it touched; Plan 487
touches the same coordinator plus the destination bridge, so the order is
recorded here.

The locks in the touched paths are:

1. `ServiceTunnelManager`'s destination-bridge lock
   (`with_destination_bridge`).
2. `DestinationTunnelCoordinator`'s mutex
   (`Arc<Mutex<DestinationTunnelCoordinator>>`).
3. The per-connection streaming manager's own owner lock (never held
   across an `await`).

The order is always **bridge → coordinator**, and it is never inverted:

- The publication path takes the bridge lock only to *clone* the public
  LeaseSet2 (`router_ls2_for_publication`) and releases it before the
  coordinator lock is taken. The coordinator lock is then taken in
  short, non-overlapping scopes: `admit_ls2_publication`, then
  `attach_ls2_publication`, then `note_ls2_publication_sent`, each in its
  own block with the guard dropped before the next `await`.
- The inbound acknowledgement path takes the coordinator lock alone
  (`correlate_ls2_publication_status`); it never holds the bridge lock
  while the coordinator lock is taken.
- The reaper takes the coordinator lock alone.

No path holds a lock across an `.await` that can block on another lock
holder, and no path takes the coordinator lock while the bridge lock is
held. The bridge's own `with_destination_bridge` helper is not re-entrant
and is never called from inside a coordinator-locked scope.

## Verification

### Local commands (this turn; the routine floor is below)

- `cargo fmt --all --check` — clean (three hunks of this plan's own edits were
  reformatted before the close-out; the check is on the final tree).
- `cargo check --locked --workspace --all-targets` — clean.
- `cargo build --locked -p i2pr-daemon` — clean.
- `python3 -m unittest discover -s tests/integration/core-router-recovery -p 'test_*.py'`
  — **108 passed** (the lane's offline controls, including the seven
  harness-defect regressions and the rotated-publication gate).
- `I2PR_PLAN487_KEEP_SCRATCH=1 timeout 2400 python3 tests/integration/core-router-recovery/run_plan487_lifecycle_recovery.py`
  — the §6 qualification **passed**; the evidence file
  `target/interop/plan487-lifecycle-evidence/result.json` was written with the
  counts and digests quoted in the §6 section above.
- The §6 lane ran against the exact-pinned stock peer (i2pd 2.61.0,
  `635b013a612ff47278ef02acf8580a28e10e26c5`) in the controlled four-peer
  loopback mesh. No reference router or client was patched or vendored.

### The AGENTS.md routine floor, on the closing tree

Every floor line was executed locally on the closing tree: **65 steps, 65
PASS, 0 FAIL**. **CI was not run this turn** (no CI trigger is available in
this environment) — that is labelled truthfully, and no claim in this record
depends on it.

| Floor step | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS (4s) — silent on a clean tree |
| `cargo check --locked --workspace --all-targets` | PASS (7s) |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | PASS (0s) |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS (1750s) |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS (24s) |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | **PASS — re-run and verified** (see the note below) |
| `cargo test --locked --workspace --doc` | PASS (5s) |
| `bash scripts/check-dependency-direction.sh` | PASS (0s) |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS (0s) |
| `python3 scripts/check-adr-number-uniqueness.py` | PASS (0s) |
| `python3 scripts/check-portable-service-tunnel-api.py` | PASS (0s) |
| `bash scripts/check-portable-service-tunnel-consumer.sh` | PASS (7s) |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS (7s) |
| `bash scripts/check-runtime-boundaries.sh` | PASS (0s) |
| `bash scripts/check-console-boundaries.sh` | PASS (4s) |
| `bash scripts/check-console-browser-security.sh` | PASS (0s) |
| `python3 scripts/check-tooling-inventory.py` | PASS (0s) |
| `python3 scripts/check-license-metadata.py` | PASS (0s) |
| `python3 scripts/check-managed-app-private-client-seams.py` | PASS (0s) |
| `python3 scripts/check-router-readiness-contract.py` | PASS (0s) |
| `python3 scripts/check-router-readiness-contract.py --self-test` | PASS (0s) |
| `python3 scripts/check-core-router-recovery-contract.py` | PASS (0s) |
| `python3 scripts/check-core-router-recovery-contract.py --self-test` | PASS (0s) |
| `python3 scripts/plan449-ssu2-preflight.py --self-test` | PASS (0s) |
| `python3 scripts/check-managed-app-package-boundary.py` | PASS (1s) |
| `python3 scripts/check-managed-app-package-boundary.py --self-test` | PASS (0s) |
| `python3 scripts/check-managed-app-policy-boundary.py` | PASS (0s) |
| `python3 scripts/check-managed-app-policy-boundary.py --self-test` | PASS (0s) |
| `python3 scripts/check-managed-app-secured-sandbox.py` | PASS (0s) |
| `python3 scripts/check-managed-app-secured-sandbox.py --self-test` | PASS (0s) |
| `bash scripts/check-service-tunnel-boundaries.sh` | PASS (0s) |
| `python3 scripts/check-managed-app-gateway-boundary.py` | PASS (0s) |
| `python3 scripts/check-managed-app-manager-boundary.py` | PASS (0s) |
| `bash scripts/check-m11-per-epoch-composition.sh` | PASS (0s) |
| `bash scripts/check-service-anonymity-boundaries.sh` | PASS (0s) |
| `bash scripts/check-fixture-manifest.sh` | PASS (0s) — silent by design; it prints only on a mismatch |
| `bash scripts/check-ntcp2-vectors.sh` | PASS (0s) |
| `bash scripts/check-ssu2-vectors.sh` | PASS (0s) |
| `bash scripts/check-i2cp-vectors.sh` | PASS (11s) |
| `bash scripts/check-ntcp2-interoperability.sh` | PASS (0s) |
| `bash scripts/check-constrained-host-lane-boundary.sh` | PASS (0s) |
| `bash scripts/check-m11-transit-boundaries.sh` | PASS (2s) |
| `bash scripts/check-m11-transit-qualification-evidence.sh` | PASS (0s) |
| `bash scripts/check-sam-acceptance-evidence.sh` | PASS (1s) |
| `bash scripts/check-ssu2-acceptance-evidence.sh` | PASS (0s) |
| `bash scripts/check-i2cp-acceptance-evidence.sh` | PASS (0s) |
| `bash scripts/check-i2pcontrol-acceptance-evidence.sh` | PASS (0s) |
| `bash scripts/check-els2-type11-transcript-boundary.sh` | PASS (0s) |
| `bash scripts/check-els2-live-lane-evidence.sh` | PASS (1s) |
| `bash scripts/check-encrypted-service-consumer-caller.sh` | PASS (1s) |
| `bash scripts/check-outproxy-request-path.sh` | PASS (0s) |
| `bash scripts/check-outproxy-wire-lane-evidence.sh` | PASS (0s) |
| `bash scripts/check-config-secret-hygiene.sh` | PASS (0s) |
| `python3 scripts/check-workflow-validity.py` | PASS (0s) |
| `bash scripts/check-floodfill-type5-serve.sh` | PASS (0s) |
| `bash scripts/check-service-tunnel-acceptance-evidence.sh` | PASS (1s) |
| `bash scripts/check-exploratory-tunnel-evidence.sh` | PASS (0s) |
| `bash scripts/check-netdb-tunnel-evidence.sh` | PASS (0s) |
| `bash scripts/check-destination-tunnel-evidence.sh` | PASS (0s) |
| `bash scripts/check-streaming-tunnel-evidence.sh` | PASS (5s) |
| `bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | PASS (3s) |
| `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | PASS (0s) |
| `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | PASS (0s) |
| `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | PASS (1s) |
| `cargo deny check advisories bans sources` | PASS (`advisories ok, bans ok, sources ok`) |

**One floor step was re-run because the first execution proved to be a false
pass.** The floor wrapper invoked the documentation build as
`env RUSTDOCFLAGS="-D warnings" cargo doc …`, and on this host `env` resolves
to `~/.local/bin/env` — a PATH-prepending shim that **ignores its arguments
and never execs the command**. The step therefore exited 0 in 0 s without
building a single document, and the "PASS" it printed was vacuous. The step
was re-run as `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace
--no-deps` (the form `AGENTS.md` itself uses) and, to rule out a stale
artifact, every workspace crate's `src/lib.rs` was touched first so the
**whole** documentation set was rebuilt: 24.16 s, exit 0, no rustdoc warning
under `-D warnings`, 33 documents generated. The floor's other two
output-free steps are silent **by design** — `cargo fmt --all --check` prints
nothing on a clean tree, and `check-fixture-manifest.sh` prints only on a
mismatch — and both were confirmed independently. No repository script uses
the `env VAR=…` form (checked across `scripts/` and the whole tree), so the
defect was in the ad-hoc wrapper, not in the repo's floor. It is recorded
here because a floor that reports a vacuous pass is exactly the failure mode
this repository's evidence rules exist to prevent.

## Migration / compatibility

- **Wire protocol:** unchanged in shape. The receiver-ACK correction changes
  *when* the router acknowledges (on transport state, as both pinned
  references do), not the packet format. The only new wire value is the
  reference's own choke (`DELAY_CHOKE = 61000`), and the `DeliveryStatus`
  correlation reuses the existing message-id field as the reply token.
- **SAM/I2CP surface:** no new command, no new reply vocabulary, no new
  session kind. The close receipt keeps its existing shape; the DATA-phase
  attribution is a **new** WARN receipt with counts only, so a log consumer
  that ignores unknown WARN lines is unaffected.
- **Configuration:** no new key, no changed default, no changed ceiling. The
  lease rotation's margin is the existing replenishment deadline; the inbound
  pool's headroom is `target + 1` clamped to 8.
- **Compatibility with the pinned references:** the ACK contract now matches
  i2pd 2.61.0 and I2P Java 2.13.0 exactly, so a stock peer that withheld an
  acknowledgement until local drain now receives it on schedule; the
  reference's own retry-and-choke behaviour is unchanged.
- **Support inventory:** `specs/support.toml` is untouched; no conformance
  claim, version claim, RouterInfo advertisement, or SAM behaviour claim was
  added. The Plan 433/439 gates are unchanged.

## Security and contention

- The lock order is reviewed in the section above: **bridge → coordinator**,
  never inverted, and no lock is held across an `.await` that can block on
  another lock holder.
- The publication ledger is bounded (rate limit below 1/s, monotonic
  `published`, non-regressing oldest-lease expiry, a bounded reaper), so a
  misbehaving or hostile floodfill cannot make the coordinator accumulate
  unbounded state.
- The LeaseSet2 fan-out is bounded to the three nearest floodfills with a hard
  ceiling and a unit test; the reply token rides on **one** copy only, so a
  publication produces exactly one correlated acknowledgement and no surplus
  `DeliveryStatus` traffic is generated at the peer.
- No secret, identity, key, port, stream id, token, or payload material is
  written to any receipt, log line, or evidence file. The daemon's own
  acknowledgement receipt carries the local inbound tunnel id because the
  proof requires the status to have arrived *on an inbound tunnel*; that value
  never reaches an evidence file.
- The lane's evidence file carries counts, decisions, and payload digests
  only. The stock peer's raw log never becomes evidence — only sanitized
  counts leave the reader.

## Documentation / operational

- `docs/adr/0040-stream-recovery-across-tunnel-rebuild-and-router-restart.md`
  — the settled §6 semantics, with the F4 amendment.
- `specs/protocols/07-streaming.md` — the settled decision recorded, and the
  previously open decision marked settled.
- `docs/architecture/i2pr-client.md` — the client-side changes.
- `plans/diagnostics/2026-10-19-external-reference-streaming-ack-and-lease-lifecycle.md`
  — the point-in-time research input (not planning authority).
- **Operator action: none.** No configuration key was added, changed, or
  deprecated; no listener, port, or service was added; the router's default
  behaviour is unchanged for a deployment that does not use the SAM product.
  The lane's readers were updated in the same change as the receipts they
  read, so there is no window in which the daemon emits unparsable lines.

## Known limitations

- The §6 evidence is a **controlled four-peer loopback mesh** with one
  destination, not a public-network proof. It establishes the two ADR 0040
  recovery cases for the tested subset, nothing wider.
- The reverse DATA phase is proven for the payload sizes the lane uses
  (1816 and 2328 bytes). Larger payloads, and payloads that straddle a
  rotation *mid-transfer*, are not covered.
- The 18-key `stream_accept_syn_wait_expired` receipt still never fires on the
  lane (no timeout occurs there); its daemon↔parser agreement rests on the
  key-agreement unit test and the offline parser tests, not a live line.
- F2, F3, and F6 remain open properties of the router's own behaviour (see
  the findings below); this plan does not change them.
- The macOS CI lane, the Java fleet, and the Java I2P comparison were not run
  this turn.

## Findings by severity

- **Critical:** none.
- **High:** none remaining in scope.
- **Medium (recorded, not fixed — each needs a future plan-of-record):**
  - **F2** — a session dies with the silence of its control socket; the
    router's own product behaviour, not a lane artifact.
  - **F3** — a controlled transit-peer loss destroys the live path; a peer
    loss is not recoverable in the current tunnel design.
  - **F6** — the accept socket is a single bidirectional channel, so the
    router's own raw driver can consume the application's inbound bytes. The
    pinned i2pd 2.61.0 shares this shape, so no divergence from the reference
    is claimed and no protocol change was made here.
- **Low (fixed in this plan, recorded for the audit trail):** the routing
  peek's wire-field misread (fixed), the rotation's re-sign of an unchanged
  lease set (fixed), the dead four-stage attribution in router-network mode
  (fixed), and the seven lane-harness defects (fixed with offline controls).
- **Low (host environment, not a repository defect):** on this host `env`
  resolves to `~/.local/bin/env`, a PATH-prepending shim that ignores its
  arguments and never execs the command it is given. Any floor wrapper that
  spells a step as `env VAR=… cmd` therefore exits 0 **without running
  anything**, and reports a vacuous PASS. The repository's own floor uses the
  `VAR=… cmd` shell form, and no repository script uses the `env VAR=…` form
  (checked across `scripts/` and the whole tree), so nothing in the repo is
  affected — but the hazard is recorded here so a future run on this host
  does not repeat it. It was found because the step's output was audited:
  three floor steps produced no output, and two of the three are silent by
  design while the third was the false pass.

## Roadmap disposition

**Closed.** The core-router-recovery roadmap §7 rows for 478 and 487 are
advanced to `closed` / `passed`, the "Plans 477–480" paragraph records that
Plan 487 closed the three remaining gates, and the registry rows for 478 and
487 are closed and moved into the registry's recently-closed table. The
roadmap's dependency flow now reads 478 as closed. The Plan 433 public /
external client-router gate is **not** promoted by this closure.

## Unblock audit

Audited at this closure, over `plans/registry.md` (the "Blocked work",
"Dependency-ready and active plans", and "Current milestone authorities"
sections) and the core-router-recovery roadmap's dependency graphs:

- **No registered plan lists 478 or 487 as a hard or interface dependency.**
  The registry's blocked rows are 450, 371, 357, 358, 187/188/191, 280, 325,
  279, 306, 308, and 310 — none of them names 478 or 487 as its blocker.
- The roadmap's forward edge is 474 → 477 → 478 → **Plan 433**. Plan 433
  remains `blocked on 431,432`: Plan 431 is `stopped` (it needs an authorized
  independently addressed non-loopback reference topology) and Plan 432 is
  passed, so **not all** of Plan 433's hard dependencies are closed and
  **no row moves to `ready`**.
- Plan 439 stays blocked on 433/435/436/438, and 437/438 stay gated on
  433/436 — both unchanged by this closure.
- No corrective pass is required for the work this plan owned: F2, F3, and F6
  are recorded as router-behaviour properties for a **future** plan-of-record
  and are explicitly **not** unblocked by this closure, so nothing is
  silently unblocked.

## Open items

None for this plan. The three gates Plan 487 carried are closed:

- The reverse external SAM STREAM DATA phase is closed: the pinned lane
  produces a digest-matched reverse payload in both directions
  (`i2pr-to-i2pd` 1816 bytes, `i2pd-to-i2pr` 2328 bytes) with no
  retransmission exhaustion in the stock peer.
- The §6 live lifecycle recovery is closed: the lease rotation and the
  daemon restart are both qualified live, with the router's own receipts as
  the evidence (the table above).
- The product close-out of Plan 478 is done: Plan 478's record carries the
  closing entry and Plan 478 is closed.

The findings that are **recorded rather than fixed** (F2, F3, F6) are
deliberately left open for a future plan-of-record; they are properties of
the router's own behaviour, and this plan does not change them.

## Security-relevant decisions

- The receiver-ACK contract now matches the references: the acknowledgement
  is a function of transport state, never of local application drain.
- The inbound-buffer bound is preserved by dropping the overflowing payload
  and choking the peer, so memory stays bounded at every layer and the
  acknowledgement is never withheld.
- The choke uses the reference's own vocabulary and value; it introduces no
  new wire semantics beyond what both references already implement.
- The publication intent is reserved before the work that can fail, so every
  failure class releases coordinator capacity; no intent can leak.
- No identity, key, port, stream id, or payload material appears in any new
  receipt; the receipts are counts only.
- The LeaseSet2 fan-out is bounded (three nearest floodfills, hard ceiling with
  a unit test) and the reply token rides on one copy only, so a publication
  produces exactly one correlated acknowledgement and no surplus
  `DeliveryStatus` traffic.
- The lane's evidence file carries counts, decisions, and payload digests
  only: no identity, key, tunnel id, token, or payload material. The daemon's
  own receipts carry the local inbound tunnel id of a correlated
  acknowledgement, because WP4 requires the proof that the status arrived
  *on an inbound tunnel*; that value never reaches an evidence file.
- No capability, conformance, `specs/support.toml`, or advertisement change.
  Plan 433/439 gates untouched. No new crate edge. No boundary script
  weakened.
