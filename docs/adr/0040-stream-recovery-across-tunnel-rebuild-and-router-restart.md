# ADR 0040: Stream Recovery Across Tunnel-Pool Rebuild and Router Restart

Status: **accepted**  
Date: 2026-10-19  
Plan: 487

## Context

`specs/protocols/07-streaming.md` listed "Stream persistence/recovery
semantics across destination tunnel-pool rebuilds and router restart" as an
**open decision**. That left the semantics undefined, and a recovery proof
judged against undefined semantics is not evidence: the §6 live
lifecycle-recovery gate of the core-router-recovery line could not be
scored, and two different implementations could each claim conformance.

The question has two distinct cases that are easy to conflate:

- A **destination tunnel-pool rebuild** is routine. A published LeaseSet2
  names a set of inbound tunnels; tunnels live 10 minutes and the
  LeaseSet2 `expires` ceiling is about 660 s (11 minutes), so a healthy
  destination rotates its lease set continuously. The rotation changes the
  *transport path* under a stream, not the stream.
- A **router restart** is not a rotation. The ECIES session, the streaming
  connection, the send/receive windows, the retransmission state, the SAM
  session registry, and the per-destination tunnel pools are process state.

The references make the first case explicit: "Do not equate tunnel loss with
peer failure prematurely. Tunnel rotation, garlic session changes and path
latency can alter RTT without changing the destination."

## Decision

The recovery semantics are split by what is being recovered, and the
client-observable result is defined for each case.

**Case 1 — destination tunnel-pool rebuild (lease rotation, tunnel expiry,
tunnel-pool replenishment).** The stream **survives**. A stream is bound to
a *destination*, never to a tunnel. The ECIES session key, the local and
remote stream ids, the send and receive windows, and the retransmission
state are unaffected by a lease rotation. Bytes already handed to the
streaming layer are retransmitted across the new path; the peer observes
neither a reset nor a reordering.

Client-observable result: **nothing**. No SAM reply, no stream event, no
error, no session attribute change. The only observable effect is latency
across the rebuild, and a transfer in progress continues to completion.

**Case 2 — router (daemon) restart.** The **stream does not survive**, and
neither does the SAM session. The client-observable result is that the SAM
TCP connection is closed: the client observes EOF or an I/O error and must
reconnect and re-create the session.

A reconnect establishes a **new** stream — a new ECIES session, new stream
ids, and a new retransmission state.

What survives the restart is the **router's own persisted identity**: the
router identity and the transport static key are written under the data
directory, so the router's hash is unchanged and the peers' published view
of this router stays valid.

A **persistent destination** — a configured service, whose identity record
is persisted by the storage layer — keeps its destination hash across the
restart, so a reconnect can target the same destination and the peer's
published LeaseSet2 for it remains valid at the floodfill until that
record's own expiry.

A **SAM session's destination is not persisted**: `DEST GENERATE` mints an
ephemeral destination that lives only for the session. A client that
reconnects after a restart therefore creates a new session and reaches a
new destination, resolved through the ordinary LeaseSet2 lookup exactly
like any first connection. Nothing is lost by this: the ephemeral
destination carried no state worth keeping.

**What is explicitly not claimed.** No stream resumption, no byte-offset
replay, and no cross-restart session continuity. A restarted router does not
resume a transfer; it offers the same router, and the same destination
where that destination is persistent.

**Rotation mechanics.** A rotation advances the LeaseSet2 `published`
timestamp by at least one second, so the floodfill replaces the record
instead of silently keeping the older one, and the advertised expiry is
derived from the newest lease so the record never outlives the
destination's own tunnels. The rotation is driven by the approaching-expiry
margin, not by a wall-clock timer: the margin is the deadline the
destination's tunnel-pool replenishment works against, so the replacement
tunnel is built **while the expiring tunnel is still alive**, and the
rotation is published when that replacement changes the pool's usable
tunnel set.

A rotation never re-signs an unchanged lease set. A record whose oldest
advertised lease cannot outlive its own `published` timestamp is no better
than the record it replaces, and re-signing it is not a rotation at all: the
`published` timestamp keeps advancing while the advertised lease keeps
dying, and the re-sign eventually fails outright when `published` reaches
the lease end. Plan 487's live qualification measured both halves of that
failure and the correction is part of this decision: the margin-driven
rotation publishes a **changed** lease set, the replacement tunnel is
admitted while the expiring tunnel is still alive, and a lease-set refresh
failure is destination-scoped — it degrades that destination and is retried,
and it never fails the router's transport service.

## Consequences

- The §6 live lifecycle-recovery gate becomes scoreable: a tunnel-pool
  rebuild must show the transfer continuing with no client-observable
  event, and a router restart must show the SAM connection closing while
  the router comes back with the same persisted identity and the
  destination is reachable again.
- A reference peer that resets a stream on a lease rotation is
  non-conforming, and that is a reference-side observation — it does not
  license a production wire change here.
- The router-owned publication path is measured against the same rotation
  mechanics the `LeaseSetLifecycle` substrate implements, so the
  publish-and-acknowledge loop and the rotation rule cannot drift apart.
- No persistence of live stream state is introduced. A future decision to
  resume a transfer across a restart would be a separate ADR; this one
  fixes the observable contract that such a change would have to preserve.
