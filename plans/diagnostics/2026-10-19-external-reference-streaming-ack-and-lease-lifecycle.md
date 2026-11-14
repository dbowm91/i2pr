# External reference research: receiver-ACK contract and lease lifecycle (2026-10-19)

**Status:** point-in-time research report under `plans/diagnostics/`. Not planning
authority, not conformance evidence, not a plan of record. It informs the
remaining gates of Plan 478 (reverse DATA phase, §6 live lifecycle recovery,
product close-out) and closes the open inspection item recorded in
`specs/protocols/07-streaming.md:99`.

**Why this was run.** Plan 478's reverse external SAM STREAM leg completes the
handshake but stalls in the DATA phase: stock i2pd 2.61.0 reports
`packet was not ACKed after 10 attempts, terminate` on established-phase stream
ids, and the driver leg fails at `short stream read (0/2328)` after ~90 s
(`plans/closure/core-router-recovery/478-status.md`, 2026-10-19 entry). The
forward leg is green. The question is whether the receiver-side ACK contract
in `i2pr` matches the reference implementations, and what the reference lease
and tunnel lifecycle looks like for the §6 gate.

**Method.** Reference sources were fetched at the exact pins this repository
uses (`i2pd 2.61.0` = `635b013a612ff47278ef02acf8580a28e10e26c5`,
`Java I2P 2.13.0` = `9134f808337b401e8e53c73734c81fab04280c9d`) and read at
source, not from documentation summaries. The Emissary tree was enumerated from
its GitHub API rather than from its README, per the explicit instruction in
`specs/protocols/07-streaming.md:99`.

---

## 1. The reference receiver-ACK contract

**Both reference implementations ACK on transport state, never on local
application drain.** This is the central contract fact.

### i2pd 2.61.0 — `libi2pd/Streaming.cpp`

In `Stream::HandleDataPacket`, a newly received in-sequence packet advances
`m_LastReceivedSequenceNumber` and schedules an acknowledgement
(`Streaming.cpp:412`, `:255-299`):

```
LogPrint (eLogDebug, "Streaming: Received seqn=", receivedSeqn, ...);
if (m_ReceiveQueue.size () > m_MaxWindowSize*3)
{
    LogPrint (eLogDebug, "Streaming: ReceiveQueue is full, delete packet");
    m_LocalDestination.DeletePacket (packet);
    m_IsChoking3 = true;
    if (!m_IsAckSendScheduled)
    {
        SendQuickAck ();
        ...
    }
    return;
}
```

Two things follow, and both matter:

1. The ACK deadline is derived from the RTT (`m_RTT/10`, capped at
   `m_AckDelay`) and is driven by `m_AckSendTimer`. It fires on the
   acknowledgement timer, not on the application consuming the receive queue.
2. **When the receive queue overflows, i2pd drops the packet and still sends
   the ACK** (`SendQuickAck()` above). Overflow is handled by losing data and
   advancing the sequence space, never by withholding the acknowledgement.

The receive queue (`m_ReceiveQueue`) is the local application buffer. It is
populated at `Streaming.cpp:406` *after* the ACK decision, and its state does
not feed back into ACK emission.

Termination on the sender side is `Stream::ResendPacket`
(`Streaming.cpp:1740-1756`):

```
if (m_IsIncoming && m_SequenceNumber == 1 && m_NumResendAttempts > 0 && ...)
    // "SYNACK packet was not ACKed after N attempts, terminate"
if (m_NumResendAttempts >= m_MaxNumResendAttempts)
    // "packet was not ACKed after N attempts, terminate"
```

The first branch is reachable only on the *incoming* side with
`m_SequenceNumber == 1`. A stock peer acting as the **sender** of DATA can
therefore only reach the second branch — which is exactly the row observed in
the lane. The lane's "established-phase stream id" rows are DATA-phase
resend exhaustion, and the counter is i2pd's `m_MaxNumResendAttempts`
(`i2p.streaming.maxResends`, default 10).

### I2P Java 2.13.0 — `apps/streaming/java/src/net/i2p/client/streaming/impl/`

`ConnectionPacketHandler.java:180-236`:

```java
boolean isNew;
if (seqNum > 0 || isSYN) {
    isNew = con.getInputStream().messageReceived(seqNum, packet.getPayload()) && allowAck;
} else {
    isNew = false;
}
...
if (isNew) {
    con.incrementUnackedPacketsReceived();
    con.incrementBytesReceived(packet.getPayloadSize());
    if (delayReq && packet.getOptionalDelay() <= 0) {
        con.setNextSendTime(_context.clock().now() + Math.min(IMMEDIATE_ACK_DELAY, con.getOptions().getRTT() / 8));
    } else {
        int delay = delayReq ? packet.getOptionalDelay() : con.getOptions().getSendAckDelay();
        con.setNextSendTime(delay + _context.clock().now());
    }
}
```

`messageReceived` returns true only for a new, in-sequence packet. The
acknowledgement is scheduled on that event. The application reads later from
`MessageInputStream`; the read never gates the ACK.

The inbound-overflow path is explicit that the ACK still goes out
(`ConnectionPacketHandler.java:150-160`):

```java
if (... inbound buffer exceeded ...) {
    _log.warn("Inbound buffer exceeded on connection " + con + ", choking and dropping " + packet);
    // this will call ackImmediately()
    con.setChoking(true);
}
```

`ConnectionOptions.java` states the rule in prose: *"if we have packets we
haven't ACKed yet and we don't receive _receiveWindow messages before
(_lastSendTime+_sendAckDelay), send an ACK of what we have received so far."*
The ACK is a function of what was **received**.

### Consequence for `i2pr`

A conforming receiver may withhold an acknowledgement only when the *protocol*
window says so, never because the local application has not drained. Backpressure
in both references is expressed by dropping data and advancing the sequence
space, or by the congestion/choking mechanism (`PACKET_FLAG_DELAY_REQUESTED`
with `DELAY_CHOKING*`), not by suppressing the ACK.

---

## 2. The divergence in `i2pr` (Plan 152 D1) — real, and **not** the current stall

`crates/i2pr-client/src/streaming/manager.rs` couples acknowledgement
emission to local application drain:

- `manager.rs:807-809` — `delivered_over_cap(connection_id)` is true once a
  connection holds more undrained delivered bytes than
  `delivered_cap_bytes()`.
- `manager.rs:2189-2192` — `poll_acks` **snoozes** the coalesced standalone
  ACK while over cap.
- `manager.rs:1648` — an immediate-ACK request is only honoured while not over
  cap.
- `manager.rs:1799-1804` — a piggybacked view on an outgoing data packet is
  replaced by `FLAG_NO_ACK` while over cap.

`delivered_cap_bytes()` is `max_recv_window_packets * MAX_PACKET_PAYLOAD_BYTES`
(`config.rs:790-793`), with `MAX_RECV_WINDOW_PACKETS = 128`
(`config.rs:69`).

**This is a genuine divergence from the reference contract in §1.** It is
recorded here as a finding, not as a confirmed cause of the current stall, for
a concrete arithmetic reason: the lane's failing transfer is a single 2328-byte
payload, far below a 128-packet cap, so the gate does not engage for that
traffic. It will engage, and become a protocol-visible stall against a stock
peer, on any transfer large enough to fill the receive window before the
application drains it.

The durable fix belongs inside Plan 478 (an in-scope defect found mid-pass is
repaired and retested under that plan, per the plan's own acceptance clause at
`plans/implementation/core-router-recovery/478-normal-exploratory-backed-sam-tunnels-and-client-product.md:31`),
and the replacement must keep the memory bound that Plan 152 D1 was created to
enforce — i.e. express backpressure in the reference's own vocabulary
(choking/delay options, receive-window limits, bounded reorder buffers) rather
than by withholding acknowledgements.

---

## 3. The reverse DATA stall: what the evidence now excludes, and what it does not

The reference contract in §1 does **not** by itself explain the stall, because
in the failing lane the ACK gate should not have engaged. The remaining causal
candidates are two, and they are cleanly separable by the existing owned-layer
arrival mirrors:

- **(a) Arrival loss.** The DATA cells never reached the streaming manager.
  The stock DATA traverses the peer's outbound tunnel to our published inbound
  gateway. If the inbound path stops short of the streaming manager, no ACK is
  ever generated. Consistent with Plan 486's `bridge_present=0` and the
  all-zero torn-down close shape: at data-phase close the inbound bridge was
  not present.
- **(b) Egress loss.** The DATA reached the manager, an ACK was generated, and
  the ACK never reached the peer. The SYN-ACK proves a *reply* route existed at
  handshake time, but the standalone plain ACK is a different send on the
  accepted connection, and it travels to the peer's published inbound tunnel
  rather than back along the SYN path.

A third possibility must be excluded explicitly rather than assumed away:
the DATA may have arrived, been delivered into `pending_delivered`, and never
reached the SAM socket. `drain_delivered()` (`manager.rs:2261`) is the **only**
path by which delivered bytes reach an application, so a bridge that does not
call it for an accepted stream would produce exactly the observed
`short stream read (0/2328)` while the ACK path itself remains healthy.

**The discriminating measurement** is therefore a single bounded lane that
records, per stage: (i) arrival at the inbound gateway, (ii) arrival at the
streaming manager, (iii) the ACK the manager generated (its `ackThrough` and
whether it was `NO_ACK`), and (iv) the egress send of that ACK. The existing
owned-layer arrival mirrors (manager TunnelData / cell / owner / direct
counters) cover (i) and (ii); the send-side map covers (iii) and (iv). A run
that shows (i) and (ii) true with (iii) present and (iv) absent isolates (b);
(i) false isolates (a); (ii) true and (iii) absent isolates the drain path.
This is a material change of observation, so it satisfies the bounded-execution
requirement of a changed causal path rather than an unchanged-topology retry.

**Do not close this gate on attribution.** The gate is a digest-matched
payload across the reverse leg, not a counter.

---

## 4. §6 live lifecycle recovery: reference lease and tunnel lifecycle

From the I2P network-database documentation and the `LeaseSet2` proposal
(`i2p.net/en/docs/overview/network-database/`,
`beta.i2p.net/en/proposals/123-new-netdb-entries/`):

- **Tunnel lifetime is 10 minutes**; a LeaseSet is expected to live at most
  10 minutes, and `LeaseSet2`'s header `expires` offset is capped at about
  **660 seconds (11 minutes)**.
- **Publication is acknowledged.** "A router publishes a local LeaseSet by
  sending an I2NP DatabaseStoreMessage with a nonzero Reply Token over an
  outbound client tunnel for that Destination. The message is end-to-end garlic
  encrypted using the Destination's Session Key Manager, to hide the message
  from the tunnel's outbound endpoint. The floodfill router replies with an
  I2NP DeliveryStatusMessage, with the Message ID set to the value of the Reply
  Token. This message is sent back to one of the client's **inbound** tunnels."
  A publish is therefore only confirmed when a DSM correlated on the Reply
  Token arrives **on an inbound tunnel** — an inbound-path event, which is the
  same class of event as the §3 candidate (a).
- **Republish must advance two fields**, or the floodfill silently rejects
  the new record: the `published` timestamp must be at least one second later
  than the previous one, and the oldest lease's expiration must be incremented
  by at least 1 ms. Publishing more often than once per second per destination
  must be rate-limited.
- "As LeaseSets have (at most) a 10 minute lifetime, should a particular
  instance go down, the outage will be 10 minutes at most."

### What `i2pr` already has

`crates/i2pr-client/src/leaseset.rs` implements a `LeaseSetLifecycle` with:

- `refresh` / `refresh_client` / `evaluate_client_refresh` and the
  `client_refresh_pending` signal (`leaseset.rs:287-435`);
- a monotonic `published` timestamp on replacement, with a dedicated test
  `replacement_has_monotonic_published_time` (`leaseset.rs:1007`);
- approaching-expiry rotation, with a dedicated test
  `approaching_expiry_rotates_the_lease_set` (`leaseset.rs:1030`);
- `zero_usable_inbound_tunnels_is_not_publishable` (`leaseset.rs:1052`) and
  `expired_lease_is_rejected_by_the_builder` (`leaseset.rs:1095`).

The §6 gate therefore does not start from zero. What the research does **not**
show is a live proof that a lease rotation is *published and acknowledged* —
i.e. that the DSM/Reply-Token correlation described above is implemented on
our side and that an inbound-tunnel delivery of that DSM is observed after a
tunnel expiry or a controlled peer loss.

### A gap that sits directly under §6

`specs/protocols/07-streaming.md:132` lists, as an **open decision**:
"Stream persistence/recovery semantics across destination tunnel-pool rebuilds
and router restart." That is §6's subject, and it is unresolved in our own
protocol dossier. §6 cannot be closed as a live recovery proof while the
governing behaviour is still an open decision; the plan-of-record needs to
state the intended semantics (does a stream survive a tunnel-pool rebuild? does
a SAM session survive a daemon restart?) before the lane can be judged against
it.

---

## 5. Emissary: not a usable peer for this line, but a usable §6 reference

Enumerated from the GitHub API at `master` (not from the README, per
`specs/protocols/07-streaming.md:99`): **229 `.rs` files**, with
`emissary-core/src` modules `crypto, destination, error, i2cp, i2np, netdb,
primitives, router, runtime, sam, subsystem, transport, tunnel, util`.

- **There is no `streaming` (or `ministreaming`) module.** The only
  "stream" path in the tree is `emissary-core/src/sam`, which is SAM's own
  stream handling, not the I2P Streaming protocol. Emissary's advertised
  features are transports (NTCP2, SSU2), client protocols (I2CP, SAMv3) and
  proxies (HTTP, SOCKSv5).
- **Consequence for Plan 478.** Emissary cannot serve as the stock peer for
  the 478 lane, which requires exact binary STREAM CONNECT/ACCEPT bytes in
  both directions, and it is not a third reference for the §3 ACK question.
  Its status is *absent*, not merely incomplete — a stronger and cleaner
  answer than "check the README".
- **Consequence for §6.** Emissary does carry a full `LeaseSet2` publish state
  machine in `emissary-core/src/destination/lease_set.rs`, with an explicit
  state set (`Inactive → AwaitingLeaseSet → AwaitingTunnels →
  GetClosestFloodfills → PublishLeaseSet → AwaitingFlooding → VerifyStorage →
  Retry`) and concrete timers: `TUNNEL_BUILD_WAIT_TIMEOUT` 30 s,
  `RETRY_TIMEOUT` 1 s, `STORAGE_VERIFICATION_START_TIMEOUT` 10 s,
  `STORAGE_VERIFICATION_TIMEOUT` 2 s per `DatabaseStore`, and
  `STORAGE_VERIFICATION_TOTAL_TIMEOUT` 15 s, after which the lease set is
  republished to the NetDB. This is a legitimate design reference for a
  *router-owned* lease publish-and-verify loop, and it is the one concrete
  external model for the "publish → floodfill → verify → retry" shape §6 needs.
- Emissary is listed by I2P itself as **experimental / in development / not
  recommended for production**
  (`i2p.net/en/docs/overview/alternative-clients/`). It must not be vendored,
  patched, or used as a substitute for the pinned i2pd/Java references, and
  `AGENTS.md` forbids patching or vendoring external routers in any case.

---

## 6. What is needed to complete the 477–480 line

Derived from §§1–5. The line's only open plan is **478**.

**Gate A — reverse external SAM STREAM DATA phase (product-significant, 478).**

1. Run one bounded, exact-pin lane that records the four stages in §3
   (gateway arrival → manager arrival → ACK generated → ACK sent). No
   unchanged-topology retry; this is the material change the policy requires.
2. Fix the cause inside Plan 478. If it is (b), the accepted connection's ACK
   egress is the defect; if it is (a), the inbound gateway → manager hop is;
   if it is the drain path, the SAM bridge's `drain_delivered()` wiring for an
   accepted stream is.
3. **Separately and regardless of the outcome**, repair the Plan 152 D1
   divergence from §1: acknowledgement gating must not depend on local
   application drain. Keep the memory bound, express backpressure in the
   reference's vocabulary, and add a deterministic regression that asserts an
   acknowledgement is still emitted when the application has not drained.
4. Close the gate only on a digest-matched payload across the reverse leg.

**Gate B — §6 live lifecycle recovery (478).**

1. Settle the open decision at `specs/protocols/07-streaming.md:132` in a
   plan-of-record, so the lane has a defined expected behaviour to be judged
   against.
2. Prove a lease rotation end to end: republish with a `published` timestamp
   at least one second later and the oldest lease expiration incremented by at
   least 1 ms; observe the `DeliveryStatus` correlated on the Reply Token
   arriving **on an inbound tunnel**; and show the recovery through one tunnel
   expiry/replenishment or controlled peer loss plus daemon restart.
3. Use the Emissary publish-and-verify state machine as the external design
   reference for a router-owned publish→floodfill→verify→retry loop, and
   `LeaseSetLifecycle` as the local substrate. Do not add a parallel publish
   path.

**Gate C — product close-out (478, mechanical).**

Full `AGENTS.md` floor on the closing tree; the closure record's final form;
the registry transition; the roadmap §7 disposition; the unblock audit.

**Gate D — two floor reds, externally owned, blocking the close-out.** Both
re-verified open on this tree on 2026-10-19:

- `scripts/check-els2-live-lane-evidence.sh` → `FAIL: Plan 393: compose,
  request-construction, and delivery failures must cancel publication intent`
  (3 `.cancel_ls2_publication(request_id)` sites required, 1 present).
  Bisected to committed 478 change `5b874e86`. Needs a 478/ELS2
  plan-of-record; fix code, never weaken the script.
- `cargo deny check advisories` → FAILED: rustls 0.23.45 via tokio-rustls
  0.26.6 — RUSTSEC-2026-0346, RUSTSEC-2026-0347, and RUSTSEC-2026-0348. The
  third is **new since the Plan 477 record** (which lists only 0346/0347) and
  concerns the built-in Brotli certificate decompressor under the non-default
  `brotli` feature. Documented fix is `rustls >= 0.23.46`; it needs a reviewed
  dependency-update plan-of-record. `Cargo.lock` is untouched by this line.

---

## Sources

Reference implementations, read at this repository's exact pins:

- i2pd 2.61.0, `635b013a612ff47278ef02acf8580a28e10e26c5` —
  `libi2pd/Streaming.cpp`, `libi2pd_client/ClientContext.cpp`,
  `libi2pd_client/I2PTunnel.cpp`.
- I2P Java 2.13.0, `9134f808337b401e8e53c73734c81fab04280c9d` —
  `apps/streaming/java/src/net/i2p/client/streaming/impl/Connection.java`,
  `.../ConnectionPacketHandler.java`, `.../ConnectionOptions.java`.
- Emissary, `eepnet/emissary` @ `master` —
  `emissary-core/src/destination/lease_set.rs`; module inventory from the
  GitHub tree API.

Specifications and documentation:

- <https://i2p.net/en/docs/overview/network-database/>
- <https://i2p.net/en/docs/specs/common-structures/>
- <https://beta.i2p.net/en/proposals/123-new-netdb-entries/>
- <https://i2p.net/en/docs/specs/i2cp/>
- <https://i2p.net/en/docs/overview/alternative-clients/>
- <https://docs.i2pd.website/en/latest/user-guide/tunnels/>

Local sources consulted: `crates/i2pr-client/src/streaming/manager.rs`,
`crates/i2pr-client/src/streaming/config.rs`,
`crates/i2pr-client/src/leaseset.rs`, `specs/protocols/07-streaming.md`,
`specs/support.toml`, and the Plan 477/478/479/480/485/486 closure records.

**Nothing in this report is a capability, conformance, or advertisement claim,
and no lane was executed to produce it.**
