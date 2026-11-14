# Plan 533 — the router must establish a session with the peer it reached through the relay

Status: registered 2026-11-11
Subsystem: network participation
Predecessor: Plan 532 (`passed-the-router-must-answer-a-relay-introduction`)
Plan number: 533 (assigned; the next free number at registration)

## 1. Objective

The SSU2 relay exchange is now complete in **shape** but not in **effect**. Plan 531
gave the requester (Alice) its role, Plan 532 gave the target (Charlie) its role, and
Plan 532's lane drove all three routers over real loopback UDP — the request was
signed, the introduction was forwarded, the introduction was verified, the accept was
signed and forwarded, and the hole punch arrived and recorded the relay-firewalled
fact. What the lane could not do is the last step, because the last step did not
exist: **Alice never opens a session with Charlie**.

Three concrete gaps stand between the tree and a firewalled peer being reachable:

1. **The accept's token is inert.** The target draws the accept's token from the
   daemon's own randomness and puts it in the accept, but the target's **responder**
   — the state machine that would consume that token — knows nothing about it. The
   token is not in the responder's token store, so the target would reject its own
   token as unknown. The value the accept carries is a number with no consumer.
2. **An early hole punch is refused and lost.** The transport's requester table moves
   a request to `AwaitingHolePunch` only when the accept has been correlated, and it
   refuses a hole punch that arrives before that. The hole punch travels **one** hop
   (Charlie → Alice) and the accept travels **two** (Charlie → Bob → Alice), so on
   loopback the punch reaches Alice's handoff **first**. The production path refuses
   it, maps the refusal to a generic transport refusal, and the event is consumed: the
   exchange then never completes. Plan 532's lane worked around this with lane-side
   scaffolding; the production path does not have that scaffolding.
3. **There is no establishment.** Nothing reads the accept's endpoint and token,
   nothing reads the target's validated static and introduction keys, and nothing
   dials the target. The transport's own documentation already names the intended
   design: *"the caller dials via the standard establishment path with the HolePunch
   token — never a relay-specific fake session"*. No such call exists.

This plan closes all three. The target issues a **real** establishment token from the
runtime's responder token store, bound to the requester's exact source address; the
requester **defers** an early hole punch and re-drives it once the accept is
correlated; the requester then **dials the target through the runtime's ordinary
establishment path**, presenting the accept's token.

The completion clause this advances is the roadmap's own: *"the router answers a
peer's SSU2 Relay Request as the introducer, so that a firewalled peer is reachable
through it"*. Plan 532 discharged the answering half. This plan discharges the
**reachable** half on the requester's side.

## 2. Why ready

- Plan 531 closed with the requester role in the product composition, behind the
  default-off `ssu2.relay_requester` opt-in, and its closure record named the
  requester's establishment as the honest limit.
- Plan 532 closed with the target role in the product composition, behind the
  default-off `ssu2.relay_target` opt-in, and its closure record named **two**
  limits: the establishment dial that would consume the accept's token, and the
  requester owner's missing deferred-punch buffer. This plan is the named owner of
  both.
- The transport's relay state machines are **complete**: `RelayRequester`,
  `RelayIntroducer` and `RelayTarget` all exist, all are tested, and none needs to
  change. The transport's `TokenStore` already implements exactly the semantics the
  relay establishment needs — a one-use token bound to an exact source address,
  issued by the responder and consumed by the initiator's Session Request.
- The runtime already owns the sockets, the sessions, the responder's token store,
  the cached-token dial path and the ordinary establishment dial. The daemon already
  owns the relay requester and target owners, the relay drain and the relay tick.
- The **controlled loopback profile** is the delivery vehicle, and Plan 431's bound
  (the loopback-only profile) is unchanged by this plan.

## 3. Current implementation evidence

### 3.1 The target's accept carries a token the target would reject

`crates/i2pr-daemon/src/relay_target.rs:467` draws the accept's token with
`draw_nonzero_token` (`crates/i2pr-daemon/src/relay_target.rs:522`), which reads the
**daemon's** randomness and returns a nonzero `u64`. The value goes straight into
`RelayResponseBlock::accept` (`crates/i2pr-daemon/src/relay_target.rs:471`).

The token's only possible consumer is the target's **responder**, and the responder's
authority is the runtime's `TokenStore`, held in the service state at
`crates/i2pr-runtime/src/ssu2_runtime.rs:1497`. A token the responder did not issue
is not in that table, so `TokenStore::consume`
(`crates/i2pr-transport-ssu2/src/token.rs:243`) returns `UnknownToken` for it.

The responder consumes a presented token in exactly one place:
`crates/i2pr-transport-ssu2/src/state_machine.rs:1122`, inside
`Responder::on_session_request`. The runtime calls that seam at
`crates/i2pr-runtime/src/ssu2_runtime.rs:6795` (the stateless Retry path) and at
`crates/i2pr-runtime/src/ssu2_runtime.rs:6532` (the admitted path). Both pass
`&mut state.token_store`.

`TokenStore::issue` (`crates/i2pr-transport-ssu2/src/token.rs:194`) binds a token to
an **exact** `SocketAddr`; `TokenStore::consume` refuses any other source
(`WrongSource`), any expired token (`ExpiredToken`) and any second use
(`UnknownToken`, because consumption removes the entry). The lifetime is
`TOKEN_LIFETIME_SECONDS = 30` (`crates/i2pr-transport-ssu2/src/constants.rs:211`).

**Finding.** The accept's token is a number with no consumer. The establishment the
protocol describes cannot happen with it.

### 3.2 An early hole punch is refused, mapped to a generic reason, and lost

`RelayRequester::on_hole_punch` (`crates/i2pr-transport-ssu2/src/relay.rs:569`)
requires the request to be in `RequesterState::AwaitingHolePunch`:

```rust
if entry.state != RequesterState::AwaitingHolePunch {
    return Err(RelayError::WrongRole);
}
```

(`crates/i2pr-transport-ssu2/src/relay.rs:582`.) The state is set only by
`RelayRequester::on_response` (`crates/i2pr-transport-ssu2/src/relay.rs:527`), which
is the accept's path.

The daemon's `RelayRequesterOwner::on_hole_punch`
(`crates/i2pr-daemon/src/relay_requester.rs:910`) maps that refusal through
`map_relay_error` (`crates/i2pr-daemon/src/relay_requester.rs:1143`), whose
catch-all arm is `RelayRequesterRefusal::TransportRefused`. The event has already
been consumed by the drain
(`crates/i2pr-daemon/src/ssu2_relay_drain.rs:198`), so the punch is **gone**.

The ordering is not incidental. The hole punch is staged out of session toward the
requester's endpoint by the target (`crates/i2pr-daemon/src/relay_target.rs:493`);
the accept is queued on the **introducer's** session
(`crates/i2pr-daemon/src/relay_target.rs:483`) and travels two hops. One hop beats
two hops.

**Finding.** On loopback — the only delivery profile this tree has — the production
path refuses the punch that carries the exchange.

### 3.3 The requester holds no establishment material and dials nothing

`RelayRequesterOwner`'s live table (`crates/i2pr-daemon/src/relay_requester.rs:574`)
holds a `RelayRequestContext` and a deadline. The context names the introducer, the
target and the address family — **not** the accept's endpoint, **not** the accept's
token. `RelayResponseBlock::endpoint()`
(`crates/i2pr-transport-ssu2/src/block.rs:931`) and
`RelayResponseBlock::token()` (`crates/i2pr-transport-ssu2/src/block.rs:941`) are
readable, but nothing reads them.

The runtime's dial already knows how to present a token: `dial_ssu2`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:4795`) consults
`take_cached_token_locked` (`crates/i2pr-runtime/src/ssu2_runtime.rs:5574`) and enters
`DialPhase::CachedToken` (`crates/i2pr-runtime/src/ssu2_runtime.rs:4866`), which
puts the token in the initiator's Session Request. The cache is filled by
`note_cached_token_locked` (`crates/i2pr-runtime/src/ssu2_runtime.rs:5589`) from a
peer's `NewToken` announcement.

But the cached-token path is the **wrong** vehicle for the relay establishment, for
two reasons. First, it is a FIFO: `take_cached_token_locked` removes the **oldest**
entry for the peer, so an older peer-issued token would be presented instead of the
accept's, and the target's responder would refuse it as unknown. Second, there is no
seam that puts a token into the cache on demand — the only writers are the peer's
`NewToken` announcement (`crates/i2pr-runtime/src/ssu2_runtime.rs:5815` and
`crates/i2pr-runtime/src/ssu2_runtime.rs:7447`).

**Finding.** The establishment has no material and no path.

### 3.4 The transport's own documentation names the intended design

`RelayRequester::on_hole_punch`'s documentation
(`crates/i2pr-transport-ssu2/src/relay.rs:561`) says:

> Correlates the HolePunch connection IDs and RelayResponse nonce to the exact
> request; validates Charlie's endpoint/signature/freshness; then reports readiness
> to transition into the normal SSU2 handshake (**the caller dials via the standard
> establishment path with the HolePunch token — never a relay-specific fake
> session**).

The design is therefore already decided by the tree, and this plan implements it
rather than inventing one.

### 3.5 The tree's own records name this gap

- `plans/closure/network-participation/532-status.md` §"Limitations" records that the
  **requester's establishment dial** that would consume the accept's token is not in
  the tree, and that the requester owner has **no deferred-punch buffer**.
- The same record's findings section names the missing deferred-punch buffer as a
  **Medium** finding, with the owner being the successor that completes the
  requester's establishment path, and the absent token consumer as a **Low** finding.
- `plans/subsystems/network-participation-roadmap.md` §11 keeps the clause *"so that
  a firewalled peer is reachable through it"* in the completion definition.

## 4. Invariants

1. **The token's issuer is the responder.** The accept's token is issued by the
   runtime's own responder token store, bound to the requester's exact source
   address, with the router's own randomness. No daemon-side random value is ever
   presented to a responder.
2. **The token is one-use and source-bound.** It is consumed once, by the responder
   that issued it, and only from the exact address it was issued to. A token
   presented from another address, after expiry, or a second time is refused by the
   transport's own table, not by a rule this plan writes.
3. **The establishment is the ordinary establishment path.** The requester reaches
   the target with the runtime's normal SSU2 handshake. No relay-specific session, no
   synthetic peer, no bypass of the responder's verification.
4. **The requester reads only validated material.** The target's static and
   introduction keys come from the target's **validated** RouterInfo, read through
   the transport's strict address parser, exactly as the floodfill service's
   `validated_dial_target` (`crates/i2pr-daemon/src/floodfill.rs:220`) does it.
5. **A local condition is never written into a peer's profile.** Every refusal in
   this plan is a typed, counted reason about **this router's** state. No peer's
   record, profile or reachability state is written, and a peer that is unknown,
   absent or unparsable is refused under its own reason.
6. **The reachability model is untouched.** A peer reached through a relay is
   reachable *through the relay*, not directly. No corroborating class is claimed
   for a relayed peer, and the transport's corroboration and contradiction floors
   are not changed.
7. **The transport's relay state machines are used, not edited.** This plan changes
   no file under `crates/i2pr-transport-ssu2/src/relay.rs`, and no transport file at
   all unless the closure record names the change and the reason.
8. **Every bound is explicit.** The deferred-punch buffer, the pending
   establishment table and the establishment handoff each have a stated capacity and
   an expiry, and each refusal under a full bound is counted.
9. **No advertisement.** `router.version` stays `0.9.62`, the published RouterInfo
   and `caps` are unchanged, and the inventory's advertised count stays **zero**.
10. **One opt-in per role.** The establishment rides the **existing**
    `ssu2.relay_requester` opt-in, and the target's token issuance rides the
    **existing** `ssu2.relay_target` opt-in. **No configuration key is added.**
11. **No boundary script is weakened.** No `|| true`, no `continue-on-error`, no
    filename filtering, no fake peer environment, no broad exclusion, no
    early-return-success, and no production wire change made to go green.

## 5. Scope

### 5.1 In scope

1. The runtime issues the accept's token from its **responder** token store, bound
   to the requester's source address, and reports the value to the target owner.
2. The target owner puts the **issued** token in the accept and drops its own
   random-token draw.
3. The requester owner holds a **bounded deferred-punch buffer**: a hole punch that
   arrives before its accept is held, not refused, and re-driven when the accept
   lands.
4. The requester owner records the accept's **endpoint** and **token** on the live
   request, and publishes a **bounded pending establishment** when the exchange
   completes.
5. The runtime gains a dial seam that presents an **explicit** establishment token,
   and the ordinary `dial_ssu2` delegates to it.
6. A **bounded, owned daemon task** performs the establishment: it reads the
   target's validated static and introduction keys from the authoritative store,
   builds the dial target under the qualification profile, and dials with the
   accept's token.
7. The counters for every step, exposed as counts only.
8. A lane that proves the establishment over **real loopback UDP** with **three real
   routers**, and a static guard that is the authority for the shape.

### 5.2 Out of scope

- The **introducer's** half of the reachability clause: Plan 490, Plan 499 and
  Plan 524 already own it, and nothing here changes the introducer.
- The **stock-peer** half. The delivery stays confined to the controlled loopback
  profile, and the stock-peer evidence stays blocked on Plan 493's authorized
  two-host topology.
- The **production SSU2 peer-test driver**, the `B` advertisement and `caps=f`.
- Any change to the transport's relay state machines, the reachability model, the
  corroboration floor or the contradiction floor.
- Any change to `Cargo.toml`, `Cargo.lock` or the dependency graph.
- The **outproxy** request path, which has no reachable caller and stays that way.

### 5.3 Out of scope, with the owner named

- The **route** by which a peer reached through a relay is then used for tunnel
  building. This plan establishes the session; it does not route traffic over it.
  The owner is a successor in the transit line.
- The **relayed peer's** presence in the peer-profile store. The establishment
  records a **local** fact about this router's session; it writes nothing into the
  peer's record, and a successor may add a local session observation.
- Plan 530's Low finding 1 (the explicit-bind class lapses with the 30-minute
  observation TTL and has no re-record timer). It is unrelated to the relay and
  stays with its owner.

## 6. Required production changes

### 6.1 `crates/i2pr-runtime/src/ssu2_runtime.rs`

- **New**: `Ssu2RuntimeService::issue_relay_establishment_token(&self, requester:
  SocketAddr, now_secs: u64) -> Result<u64, Ssu2RelayTargetError>`. The runtime
  draws eight random bytes, rejects zero, and issues the token from
  `state.token_store` with **no** responder connection id (the responder mints a
  fresh one at SessionCreated, exactly as it does for a data-phase `NewToken`).
  The randomness and the store are the runtime's; the daemon never sees the table.
- **New**: `Ssu2RuntimeService::dial_ssu2_with_token(&self, target: Ssu2DialTarget,
  token: u64, timeout: Duration, cancellation: &CancellationToken) -> Result<
  Ssu2EstablishedLink, Ssu2DialOutcome>`. The initiator presents the token in its
  Session Request. `dial_ssu2` delegates to it with **no** token.
- **Changed**: when an explicit token is presented, the **cached-token path is
  skipped**. An explicit establishment token is never shadowed by an older cached
  one, and the cached entry is left in place for its own dial.
- **New**: one refusal arm on `Ssu2RelayTargetError` for a token the store refused
  (distinct from "no randomness" and from "the transport refused").

### 6.2 `crates/i2pr-daemon/src/relay_target.rs`

- The accept's token comes from the runtime's new issuance seam, bound to the
  **requester's endpoint from the Relay Intro** — the address the target will
  observe as the datagram's source when the requester dials it.
- `draw_nonzero_token` is **removed**. The daemon's randomness no longer produces a
  token a responder could ever accept.
- The target's counters distinguish "the token was issued" from "the token was
  refused".

### 6.3 `crates/i2pr-daemon/src/relay_requester.rs`

- **New**: a bounded deferred-punch buffer. A hole punch that the transport refuses
  with `WrongRole` is **held**, with a stated capacity and an expiry, and is
  **re-driven** when the accept for the same nonce is correlated. `WrongRole` is the
  deferral signal, not a refusal reason.
- **New**: the live request records the accept's **endpoint** and **token**.
- **New**: a bounded pending-establishment table. When the exchange completes, the
  requester publishes the establishment material (the target, the endpoint and the
  token).
- **New**: one refusal reason for an establishment the requester could not prepare,
  counted under its own reason.

### 6.4 `crates/i2pr-daemon/src/relay_establishment.rs` (new)

- `RelayEstablishment` — the material: the target's peer reference, the accept's
  endpoint and the accept's token.
- `RelayEstablishmentOwner` — the counters and the bounded handoff.
- `RelayEstablishmentRefusal` — one typed variant per refusal.
- `drive_relay_establishment(...)` — reads the target's validated material from the
  authoritative store, builds the dial target under the qualification profile, and
  calls the runtime's token-bearing dial. Every refusal is typed and counted, and
  none of them writes into a peer's record.

### 6.5 `crates/i2pr-daemon/src/relay_establishment_task.rs` (new)

- The establishment's **own** task, spawned under the service's child scope, exactly
  as the peer pool's task is (`crates/i2pr-daemon/src/peer_pool_task.rs:74`). The
  daemon's `select!` loop never blocks on a handshake: the relay tick is a
  100 ms cadence and a handshake is an await.
- The task owns the handoff receiver, the store, the narrow transport capability,
  the dial timeout, the qualification profile and the counter cell. The bootstrap's
  lock is released before any dial.

### 6.6 `crates/i2pr-daemon/src/lib.rs`

- The requester owner publishes ready establishments to the task over a **bounded**
  handoff; a full handoff is a counted refusal and the material is returned to the
  owner's table, never dropped silently.
- The task is spawned only when `ssu2.relay_requester` is on.

### 6.7 `crates/i2pr-daemon/src/ssu2_relay_drain.rs`

- The drain counts a **deferred** punch as handled, not refused, and the outcome
  gains one counter for it. `drain_relay_events` (the introducer's) stays
  **byte-identical**.

## 7. Design decisions

### 7.1 The target's token comes from the responder's store, not from the daemon

The alternative — the daemon draws a token and hands it to the runtime to register —
was rejected. The store is the responder's authority; a token that is not in it is
not a token the responder will accept, and a daemon-side draw is exactly the defect
this plan closes. The runtime owns the randomness **and** the table, and the daemon
receives only the value it must put in the accept.

The binding address is the **requester's endpoint from the Relay Intro**. That is
the requester's own bound endpoint — the same one the requester put in its Relay
Request — and it is the address the target will observe as the datagram's source
when the requester dials it. The transport's `TokenStore` compares the source
**exactly**, including the port, so the binding is the whole security property of
the establishment, and it is enforced by the transport's own table.

### 7.2 The establishment presents the token explicitly, and the cache is skipped

Reusing the runtime's cached-token path would have been a smaller diff and a wrong
one. The cache is a FIFO keyed by peer, populated only by a peer's `NewToken`
announcement, and its consumer takes the **oldest** entry. An accept's token must be
presented on **this** dial, or the target's responder refuses the presented value as
unknown and the establishment fails for a reason that has nothing to do with the
peer's health. An explicit token skips the cache entirely and leaves the cached
entry untouched for the dial it was issued for.

### 7.3 The deferred punch is held by the requester owner, not by the transport

The transport's requester table is the **authority** on a request's state, and its
`WrongRole` refusal is correct: the transport refuses to correlate a hole punch
before the accept. Changing the transport to accept the punch early would move a
protocol rule into a state machine that must stay runtime-neutral and would lose the
one-use guarantee the transport's table provides.

The deferral therefore lives where the ordering problem actually is: the daemon's
requester owner, which sees the punch and the accept on the same handoff and can
hold the punch until its own accept lands. The transport's rule is unchanged and
still enforced; the owner simply presents the punch again once the transport's
precondition holds.

### 7.4 The establishment is a task of its own

A session establishment is an await, and the daemon's `select!` loop carries the
inbound drain, the relay drain and every other service arm. Blocking it on a
handshake would stall all of them — the same rule Plan 519 applied to the
publication and Plan 522 to the peer pool. The establishment therefore gets its own
task under the service's child scope, so it is owned, cancelled and joined with the
service.

The handoff between the service loop and the task is a **bounded** channel. When it
is full the material is **returned** to the requester owner's table and a refusal is
counted: an establishment is never dropped silently.

### 7.5 The dial target obeys the qualification profile

The establishment dials the **accept's** endpoint — the target's own bound endpoint,
authenticated by the target's signature over the preimage that carries it — and the
target's **keys** come from the validated store. The address is then checked
against the qualification profile exactly as every other dial in the tree is
(`daemon_dial_target_for_profile`, `crates/i2pr-daemon/src/router_i2np.rs:2508`),
so the relay establishment cannot widen the router's reach: the controlled loopback
profile stays loopback-only and the private-LAN variant stays RFC1918 IPv4.

### 7.6 The token's lifetime is the transport's, not this plan's

`TOKEN_LIFETIME_SECONDS` is 30 seconds. The requester receives the accept, defers
or re-drives the punch, and dials — all on the relay tick's 100 ms cadence. The
transport's own table is the expiry authority: a token presented after its
lifetime is refused as `ExpiredToken` by the responder. This plan adds no second
expiry and no second clock.

### 7.7 No new configuration key

The establishment is part of the requester role, so it rides `ssu2.relay_requester`.
The target's token issuance is part of the target role, so it rides
`ssu2.relay_target`. Two roles, two keys, unchanged from Plans 531 and 532. A
router that has not opted in constructs neither owner, spawns no task and dials
nothing.

### 7.8 The counters are counts only

Every counter is a count. No counter names a peer, an address, a key, a tag or a
token, and the establishment's counters are not published into the I2PControl
inspection plane — that plane's contract is not widened by this plan.

## 8. Work packages

- **WP1 — the runtime's establishment seams.** The token issuance and the
  token-bearing dial, with their refusals. *Owner: the runtime crate.*
- **WP2 — the target answers with a real token.** The target owner's accept carries
  the issued token; the daemon's random draw is removed. *Owner: the daemon crate.*
- **WP3 — the requester defers and completes.** The deferred-punch buffer, the
  recorded accept material, the pending-establishment table, and the drain's
  counting. *Owner: the daemon crate.*
- **WP4 — the establishment task.** The owner, the refusals, the task, and the
  composition's spawn. *Owner: the daemon crate.*
- **WP5 — the lane and the guard.** The lane drives three real routers over real
  loopback UDP and proves the session; the guard is the authority for the shape.
  *Owner: the daemon crate's test surface.*
- **WP6 — the documentation, the inventory and the amendment log.**
  *Owner: the documentation surface.*

## 9. Failure, cancellation, restart and contention semantics

- **The token cannot be drawn.** The target refuses the introduction under its own
  reason and emits no accept and no hole punch. Nothing is half-answered.
- **The store refuses the token.** Same: a typed refusal, no accept, no punch.
- **The punch arrives before the accept.** It is held. The hold is bounded by
  capacity and by the request's own deadline; a hold that outlives the request is
  dropped when the request expires and is counted.
- **The accept never arrives.** The request expires on the transport's own
  deadline; the held punch is released with it.
- **The handoff to the task is full.** The material is returned to the owner's
  table, a refusal is counted, and the establishment is retried on the next tick.
- **The dial fails.** The failure is counted by reason. The router does not retry in
  a loop: the requester's own cadence bounds the attempts.
- **The task is cancelled.** The cancellation is the service's own; the dial is
  awaited under it, the task exits, and the material is released with the owner.
- **The bootstrap's lock is contended.** The store is read under the lock and the
  lock is released before the dial, exactly as the peer pool's task does it.
- **A restart.** The token store is rotated on a key change, which invalidates every
  outstanding token; the requester's live table is in memory and does not survive
  the process. Neither leaves a stale grant behind.

## 10. Compatibility and migration

- **Wire compatibility.** No wire format changes. The accept carries the same
  fields it always carried; only the **provenance** of its token changes, from a
  value no responder could accept to one the responder issued.
- **Configuration compatibility.** No key is added, removed or renamed. A router
  with an existing configuration behaves exactly as before.
- **Behaviour compatibility.** With `ssu2.relay_requester` and `ssu2.relay_target`
  both off — the default — nothing in this plan runs: no owner is constructed, no
  task is spawned, no token is issued and no dial is made.
- **Migration.** None. There is no stored state to migrate.
- **The inventory.** One new surface, `daemon.relay-establishment`, experimental,
  `advertised = false`.

## 11. Required tests

### 11.1 The lane

`crates/i2pr-daemon/tests/ssu2_relay_establishment.rs` — three real routers over
real loopback UDP, with the requester, introducer and target roles all driven by
the production owners. At least these rows:

1. the opt-in is off by default, and the establishment task is not spawned;
2. the target's accept carries a token the target's **own responder** accepts (the
   token is consumed from the responder's table, not merely nonzero);
3. a hole punch that arrives before its accept is **held**, not refused, and the
   exchange still completes;
4. the accept is correlated and the punch is re-driven;
5. the requester establishes an **authenticated session** with the target, and the
   target's runtime holds the matching active session;
6. the token is **one-use**: a second presentation is refused;
7. a token presented from a **different source address** is refused;
8. every refusal is typed and counted and writes nothing into a peer's record;
9. the reads never mutate the store;
10. the lanes' routers are distinct.

### 11.2 The guard

`crates/i2pr-daemon/tests/ssu2_relay_establishment_boundary.rs` — a static guard
over the tree. Every rule has an **inverted probe** (a mutation that must be caught)
and a **fail-closed probe**. The rules are:

1. the owner is in the daemon and the mechanics are in the runtime;
2. the accept's token is issued by the runtime's responder store, and the daemon
   draws no token of its own;
3. the token is bound to the requester's source address;
4. the establishment is the ordinary establishment path — no relay-specific session
   and no bypass of the responder's verification;
5. the dial target's keys come from the validated store through the strict parser;
6. the dial target obeys the qualification profile;
7. the deferred punch is bounded and re-driven, and `WrongRole` is not a refusal;
8. the handoff to the task is bounded and a full handoff is not a silent drop;
9. the daemon's `select!` loop never awaits a handshake on the relay tick;
10. no configuration key is added;
11. no reachability signal is written by the daemon;
12. the transport's relay state machines are not edited.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
cargo test --locked -p i2pr-daemon --test ssu2_relay_establishment -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_relay_establishment_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib relay_requester -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib relay_target -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-console-boundaries.sh
bash scripts/check-console-browser-security.sh
bash scripts/check-service-tunnel-boundaries.sh
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

The **full** repository floor in `AGENTS.md` is the authority for handoff, and the
closure record must carry its complete table.

## 13. Documentation updates

- `specs/support.toml` — one new surface, `daemon.relay-establishment`,
  experimental, `advertised = false`.
- `specs/CONFORMANCE.md` — the inventory count paragraph and a Plan 533 section.
- `specs/QUALIFICATION-GATES.md` — a new gate section with the Plan 533 gates.
- `specs/protocols/09-ssu2.md` — the Plan 533 relay-establishment scope.
- `docs/architecture/i2pr-daemon.md` — the requester's establishment path.
- `docs/architecture/i2pr-runtime.md` — the establishment token and the
  token-bearing dial.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when all of the following hold:

1. the target's accept carries a token the target's own responder consumes, and the
   lane proves the consumption rather than inferring it;
2. a hole punch that arrives before its accept is held and re-driven, and the
   exchange completes on the **production** path with no lane-side scaffolding;
3. the requester holds an authenticated session with the target after the
   establishment, and the target's runtime holds the matching session;
4. the token is one-use and source-bound, and both properties are proven by a
   negative row;
5. every refusal is typed and counted, and no refusal writes into a peer's record;
6. no configuration key is added, `router.version` stays `0.9.62`, and the
   inventory's advertised count stays **zero**;
7. the full floor is green, with the closure record's table complete;
8. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the transport's responder cannot consume a token the runtime issued from its own
  store, bound to the requester's address — that would be a transport defect and
  **not** a licence to change the transport's table;
- the establishment cannot be made to obey the qualification profile;
- the floor's `cargo deny check advisories bans sources` fails;
- a stock peer would be required to prove any of the above — the stock-peer half
  stays blocked on Plan 493 and is **not** attempted here.

## 15. Closure evidence required

- The implementation commit, the lane-and-guard commit, the documentation commit and
  the closure commit, each named in the closure record.
- The **complete** floor table, every row with its result.
- The lane's rows, each with its result.
- The guard's rows, each with its result.
- The negative controls: each mutation, what it was expected to break, what it did
  break, and the SHA-256 proof that the tree was restored byte-identically.
- The requirement-to-evidence matrix, one row per requirement.
- The invariant review, the failure review, the migration review and the security
  review.
- The limitations, the findings by severity, the roadmap disposition and the
  unblock audit.

## 16. Handoff notes

- The plan is **experimental** and default-off. Do not describe the relay
  establishment as released, supported or advertised.
- The delivery is confined to the controlled loopback profile. A public deployment
  dials nothing.
- The stock-peer half stays blocked on Plan 493's authorized two-host topology.
- If the implementation finds that the establishment cannot be made to work without
  editing the transport's relay state machines, **stop** and report. The transport's
  rules are the protocol's rules, and this plan's invariant 7 forbids changing them
  to make a lane green.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 533 is registered as the successor to Plan 532 and the named owner of Plan
532's **Medium** finding (the requester owner's missing deferred-punch buffer) and
its **Low** finding (the accept's token has no consumer). The scope is the
requester's establishment path, because the roadmap's completion clause — *"so that
a firewalled peer is reachable through it"* — is not discharged by an exchange that
ends at a hole punch.

No closed closure record is rewritten by this plan. The corrections this plan
finds, if any, live in this plan's closure record and in the registry's row.

### 17.2 — 2026-11-11, implementation findings

The implementation found eight things the plan did not know. Each is recorded
here; **§3 is evidence and is not rewritten**, and every correction to a
closed plan's claim lives in §17.4 and in the registry's row.

1. **The target role needed no randomness of its own.** The daemon's
   `draw_nonzero_token` was the target owner's only use of entropy, so
   removing it left `drive_relay_target` and `drain_target_events` with a
   `rng` parameter they could not use. The plan's §6.2 said the draw would be
   removed but did not say what would happen to the parameter. Both
   signatures lost it, and Plan 532's lane was changed **mechanically** to
   match: one call site loses its argument, the fixture's `rng` field is
   removed, and two local bindings go with it. Plan 532's **guard** is
   untouched — it never named the randomness.

2. **The runtime needed no new flag.** The plan's §6.1 assumed the
   establishment would need its own "the token was used" signal. It does not:
   `Ssu2EstablishedLink::used_cached_token` already means *"a token was
   presented rather than the tokenless path"*, and the explicit token reuses
   the transport's existing `DialPhase::CachedToken` phase. The
   establishment's counters therefore count the sessions established **on the
   token** using the runtime's own flag.

3. **The responder's table cannot refuse for capacity.** `TokenStore::issue`
   **evicts deterministically** when a quota is full rather than refusing, so
   the only real refusals are a zero draw and a poisoned lock. The plan's
   §6.1 named a "the store refused" arm; the runtime's `EstablishmentRefused`
   is the poisoned-lock case, and the zero draw is the plan's existing
   `NoRandomness`.

4. **The token is reachable from one of the responder's two call sites.**
   `Responder::on_session_request` is entered from the admitted path and from
   the stateless-Retry path. A **nonzero** token never reaches the Retry
   path — that path answers a *tokenless* request — so the establishment's
   token is consumed by the admitted path only, and the Retry path is
   untouched.

5. **A refused token does not by itself prevent a session.** The runtime's
   cached-token **grace** is 1.5 seconds, and a dial that has made no progress
   within it falls back to the tokenless path, which obtains a fresh Retry.
   A token the responder refused therefore shows up as a dial that completes
   **without** the token, not as a dial that hangs. The lane's negative rows
   consequently assert the **responder's own refusal counter**
   (`Ssu2Snapshot::token_rejections`) and not merely the dial's failure —
   a failed dial alone would not distinguish a refused token from an
   unreachable peer.

6. **The runtime's snapshot already carries the decisive observable.**
   `token_rejections` is documented as *"Token validations rejected
   (unknown/expired/reused/source)"*. That single counter is the honest proof
   of the token's **one-use** and **source-bound** properties, because it is
   incremented by the responder that refused — the authority — and not by the
   initiator that offered.

7. **The establishment's task reads the router's authoritative store the way
   every other daemon service does.** The daemon's `Bootstrap` is the store,
   `Bootstrap::store_mut` exists, and the establishment's task takes the
   bootstrap exactly as the peer pool's task does. The lane therefore builds a
   real `Bootstrap` and runs the **production** task against it, rather than
   a stand-in.

8. **The establishment needed two refusal reasons, not one.** The plan's
   §6.3 named one reason for an establishment the requester could not
   prepare. The implementation distinguishes **two**: `EstablishmentUnavailable`
   for an accept that carried no endpoint or no token, or a full
   establishment table, and `EstablishmentQueueFull` for a handoff the task
   could not take. They are different conditions with different owners — the
   first is about the accept, the second about the handoff — and each is
   counted under its own reason.

### 17.3 — 2026-11-11, citation drift

The plan of record's §3 was written against the tree at the registration
commit `bfe80fd8`. The implementation added code ahead of some of the lines
§3 cites, so the **line numbers** drifted. §3 is evidence and is **not
rewritten**; the table below is the additive pointer from each original
citation to the line that now carries the same content. Three of the drifted
citations name code the plan itself **removed**, and those are marked as
such rather than pointed at.

| The plan's citation | The citation now | What the line is |
| --- | --- | --- |
| `crates/i2pr-daemon/src/relay_target.rs:467` | **removed** | `let Some(token) = draw_nonzero_token(rng) else {` |
| `crates/i2pr-daemon/src/relay_target.rs:522` | **removed** | `fn draw_nonzero_token(rng: &mut rand_chacha::ChaCha8Rng) -> Option<u64` |
| `crates/i2pr-daemon/src/relay_target.rs:471` | `crates/i2pr-daemon/src/relay_target.rs:504` | `let Ok(accept) = i2pr_runtime::RelayResponseBlock::accept(` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:1497` | `crates/i2pr-runtime/src/ssu2_runtime.rs:1505` | `token_store: TokenStore,` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:6795` | `crates/i2pr-runtime/src/ssu2_runtime.rs:6906` | `let (_, actions) = match responder.on_session_request(` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:6532` | `crates/i2pr-runtime/src/ssu2_runtime.rs:6643` | `&mut state.token_store,` |
| `crates/i2pr-daemon/src/relay_requester.rs:910` | `crates/i2pr-daemon/src/relay_requester.rs:1039` | `pub fn on_hole_punch(` |
| `crates/i2pr-daemon/src/relay_requester.rs:1143` | `crates/i2pr-daemon/src/relay_requester.rs:1417` | `fn map_relay_error(error: i2pr_runtime::RelayError) -> RelayRequesterR` |
| `crates/i2pr-daemon/src/ssu2_relay_drain.rs:198` | `crates/i2pr-daemon/src/ssu2_relay_drain.rs:223` | `i2pr_runtime::Ssu2InboundRequesterEvent::HolePunch(inbound) => {` |
| `crates/i2pr-daemon/src/relay_target.rs:493` | `crates/i2pr-daemon/src/relay_target.rs:526` | `match owner.runtime.stage_relay_hole_punch(` |
| `crates/i2pr-daemon/src/relay_target.rs:483` | `crates/i2pr-daemon/src/relay_target.rs:516` | `if owner` |
| `crates/i2pr-daemon/src/relay_requester.rs:574` | `crates/i2pr-daemon/src/relay_requester.rs:658` | `live: BTreeMap<u32, LiveRelayRequest>,` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:4795` | `crates/i2pr-runtime/src/ssu2_runtime.rs:4848` | `pub async fn dial_ssu2(` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:5574` | `crates/i2pr-runtime/src/ssu2_runtime.rs:5685` | `fn take_cached_token_locked(state: &mut ServiceState, peer: &PeerId, n` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:4866` | **removed** | `let cached = take_cached_token_locked(&mut state, &target.peer(), now_` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:5589` | `crates/i2pr-runtime/src/ssu2_runtime.rs:5700` | `fn note_cached_token_locked(` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:5815` | `crates/i2pr-runtime/src/ssu2_runtime.rs:5926` | `note_cached_token_locked(` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:7447` | `crates/i2pr-runtime/src/ssu2_runtime.rs:7558` | `note_cached_token_locked(state, &peer, token, u64::from(expires), now_` |

### 17.4 — 2026-11-11, the corrections to the closed plans' claims

**No closed closure record is rewritten.** The corrections live here and in
the registry's row.

- **Plan 532's closure record** (`plans/closure/network-participation/532-status.md`)
  recorded, under *"What the plan did not close"*, that *"The **requester's
  establishment dial** that would consume the accept's token is **not in this
  tree**"* and that *"The **requester owner has no deferred-punch buffer**"*.
  **Both are discharged by this plan.** The accept's token is now issued by
  the target's own responder table and consumed by it, and the requester
  owner holds an early hole punch and presents it again once the accept is
  correlated. Plan 532's **Medium** finding (the missing deferred-punch
  buffer) and its **Low** finding 1 (the token's absent consumer) are
  **closed**, and its **Low** finding 2 (the target role's dependence on the
  bootstrap's store) is **unchanged**: the establishment's task has the same
  dependency and reads the same store.
- **Plan 532's closure record** also recorded that *"the production drain and
  the production requester owner are unchanged"* and that the lane *"holds one
  punch aside until the accept is correlated"*. That remains true of Plan
  532's own lane, which keeps its hold as an **isolation of the target
  role**; what changes is that the **production** path no longer needs the
  hold, and Plan 533's lane proves it.
- **Plan 532's closure record** named the successor: *"the successor that
  completes the requester's establishment path (the token's consumer and the
  deferred punch)"*. That successor is this plan.
- **Plan 531's closure record** recorded that the requester role's honest
  limit was that the tree had *"no way to record that fact"* about a
  firewalled peer being reachable. That limit was discharged by Plan 531
  itself; this plan adds the **session** the fact refers to, and changes
  nothing Plan 531 recorded.

### 17.5 -- 2026-11-11, the floor's caught defects and their correction

The routine floor's first run was **not** green: three steps failed, from two
distinct defects, both in **this plan's own work**.

**The first defect -- the recorder left the body Plan 531's rule reads.**
Plan 531's boundary guard failed its rule
`the_relay_firewalled_fact_is_a_contradiction`, with the reason *"a completed
exchange must record the relay-firewalled fact"*. The cause was **this plan's
production change**, not Plan 531's guard. The correlation step had been split
into a shared `handle_hole_punch` so that the deferral could be switched off
for a re-drive, and that split moved the recorder call
`self.runtime.note_relay_firewalled(entry.context.family);` one level down,
out of the body Plan 531's rule reads. The recording itself was never lost --
the shared step still performed it -- but the rule's **shape** no longer held.

**The production code was fixed and Plan 531's guard file was left
byte-identical.** The shared step now hands the completed request back to its
caller, and each caller records the relay's success against it, so the literal
is back inside `on_hole_punch`'s body. Plan 531's inverted probe replaces that
**exact** literal, so it still fires. Plan 533's own guard was unaffected
throughout: the shared step still carries the wrong-role interception and the
deferral, and the re-drive still presents the held punch with the deferral
switched off.

**The second defect -- two lint findings in this plan's own test files.** The
`clippy` step failed on two helper functions in this plan's guard
(`item_text` and `arm_body`) that no rule uses, and then, once those were
removed, on a `clone_on_copy` in this plan's lane's counter read. The two
helpers are removed and the copy is a copy. Neither finding was in a
production file.

Section 3 is evidence and is **not** rewritten, and no closed closure record
is rewritten. The correction lives here, in this plan's closure record, and in
the registry's row.
