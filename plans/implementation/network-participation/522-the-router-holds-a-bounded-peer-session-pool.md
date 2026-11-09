# Plan 522 — the router holds a bounded pool of outbound peer sessions

## 1. Objective

The ordinary router holds a **bounded pool of outbound SSU2 sessions** to
peers it already knows, so a need arising between publications finds an
**established** session instead of a handshake — and the publication's
ordinary path stops being the handshake path.

One bounded outcome: **a production owner that keeps a small set of peer
sessions alive between needs.** Not a second dial path, not a second peer
store, not a new advertisement.

The gap is named in the network-participation roadmap's **Transports** row:
*"no outbound peer-session pool — the router dials per need and holds no
session between needs, so the publication's ordinary path is still the
handshake path"*
(`plans/subsystems/network-participation-roadmap.md:190`). It is the one
local gap that row names, and Plan 521's closure record names it as the next
thing in this line
(`plans/closure/network-participation/521-status.md:686`).

## 2. Why ready

- The dial seam **already exists** and is already used in production:
  `ensure_peer_session_for_record`
  (`crates/i2pr-daemon/src/service_product.rs:7737`) is the one narrow
  capability the router's own publication and the service product share.
  The pool needs no new dial path.
- The peer's material — the validated RouterInfo, its SSU2 address, its
  static key and its introduction key — is **already** resolved from a
  validated record by that same seam
  (`crates/i2pr-daemon/src/service_product.rs:7746-7760`). The pool needs no
  new peer-material resolver.
- The router's **own** peer population is already selectable through a
  version-gated selection the publication already uses:
  `select_version_gated_floodfills`
  (`crates/i2pr-netdb/src/lookup_policy.rs:381`,
  `crates/i2pr-daemon/src/router_info_publication.rs:539`). The pool needs no
  new selection policy and inherits Plan 491's version floor for free.
- The **publication** needs no change at all. Its delivery already reuses an
  existing session when there is one:
  `session_peer_links(peer_id).is_empty()`
  (`crates/i2pr-daemon/src/router_info_publication.rs:1329`). The pool's only
  job is to make that session **exist** when the publication needs it.
- The product already has a tick that drives bounded, cadence-gated owners
  with no task and no timer of their own
  (`crates/i2pr-daemon/src/service_product.rs:4744`, `:4959`). The pool is
  driven from there, exactly as Plan 521's peer test and Plan 506's
  reconcile are.

The **stock-peer** half — that a stock peer tolerates a held session and
that a stock floodfill is served over it — stays blocked on Plan 493's
authorized two-host topology. It is not claimed here.

## 3. Current implementation evidence

### 3.1 The router dials per need and holds nothing between needs

- `crates/i2pr-daemon/src/router_info_publication.rs:72` —
  `PUBLICATION_INTERVAL_MS = 39 * 60 * 1_000`. The publication runs every
  39 minutes.
- `crates/i2pr-daemon/src/router_info_publication.rs:82` —
  `PUBLICATION_FLOODFILLS = 3`. One batch names three floodfills.
- `crates/i2pr-daemon/src/router_info_publication.rs:116` —
  `PUBLICATION_CYCLE_TARGETS = 15`. A cycle names fifteen floodfills, in
  five batches of three.
- `crates/i2pr-daemon/src/router_info_publication.rs:1329` — the delivery's
  gate: `if ssu2_handle.service().session_peer_links(peer_id).is_empty()`.
  When there is no session, the delivery **dials**.
- `crates/i2pr-daemon/src/service_product.rs:7737` —
  `ensure_peer_session_for_record` is the dial the gate falls into.
- `crates/i2pr-daemon/src/router_info_publication.rs:590` —
  `prefer_established_sessions` already **prefers** a target that holds a
  session and counts the targets it skipped for want of one.

### 3.2 A session cannot outlive the interval between two publications

- `crates/i2pr-daemon/src/config.rs:1475` — `default_ssu2_idle_timeout_ms`
  returns `300_000`. The data-phase idle timeout is five minutes.
- `crates/i2pr-transport-ssu2/src/session.rs:198` — `SessionConfig`'s
  `idle_timeout_ms`.
- `crates/i2pr-transport-ssu2/src/session.rs:2551` — the session terminates
  itself locally when `now_ms - last_activity >= idle_timeout_ms`.

The publication's period is **39 minutes** and the session's idle timeout is
**5 minutes**, so a session established by one publication is always gone
before the next one. The publication's ordinary path is therefore the
handshake path, exactly as the roadmap's cell says.

### 3.3 The router has no way to hold a session open

- `crates/i2pr-transport-ssu2/src/session.rs:2632` — `is_ack_eliciting`
  treats `ACK`, `Address`, `DateTime`, `Padding` and `Termination` as
  **non**-ack-eliciting. A padding-only packet therefore never provokes an
  ACK on its own.
- `crates/i2pr-transport-ssu2/src/session.rs:1501` —
  `should_request_immediate_ack` derives the immediate-ACK flag from the
  congestion window alone; nothing else can request one.
- `crates/i2pr-transport-ssu2/src/session.rs:1399` — `seal_packet` is the
  single seal path, and `crates/i2pr-transport-ssu2/src/session.rs:1137` —
  `poll_transmit_fragments_traced` is the single transmit path. There is no
  keepalive, no ping, and no periodic session traffic of any kind.
- `crates/i2pr-proto/src/i2np/message.rs:7` — the proto crate's
  `I2npBody` registry has **no** `Date` message and no other no-op message
  type. There is no I2NP message the router could send as a keepalive
  without adding one.

**Consequence.** The only two ways to hold a session open today are (a) send
real payload traffic, which the router has no reason to do, and (b)
re-handshake, which is what the router already does. The second is
expensive: at a 15-peer pool, a re-handshake every idle timeout is **15
handshakes per 5 minutes** — three per minute, forever, to hold sessions a
peer test could hold with fifteen small packets.

### 3.4 The references hold sessions between needs

- `libi2pd/SSU2Session.cpp:269` — `void SSU2Session::SendKeepAlive ()`. The
  body is a **padding-only** data packet:

  ```cpp
  void SSU2Session::SendKeepAlive ()
  {
      if (IsEstablished ())
      {
          uint8_t payload[20];
          size_t payloadSize = CreatePaddingBlock (payload, 20, 8);
          SendData (payload, payloadSize, SSU2_FLAG_IMMEDIATE_ACK_REQUESTED);
      }
  }
  ```

  Twenty bytes of padding, the immediate-ACK flag set, and **no I2NP
  message at all**.
- `libi2pd/SSU2Session.h:273` — the declaration.
- `libi2pd/SSU2.cpp:1338` and `libi2pd/SSU2.cpp:1401` — the two call sites,
  both on the introducer-maintenance path: a router that wants to **keep** a
  session to a peer it has decided to keep sends it a keepalive.
- `libi2pd/SSU2.h:192` — `m_SessionsByRouterHash`, the router's live
  sessions keyed by the **peer's** router hash.
- `libi2pd/SSU2.cpp:576` — `SSU2Server::FindSession` looks a peer up by that
  hash and erases the entry when the session is gone
  (`libi2pd/SSU2.cpp:521-524`). The map is the router's live-session truth,
  not a history.
- `router/java/src/net/i2p/router/transport/udp/EstablishmentManager.java:118`
  — Java's `_outboundByHash`, the outbound establishment states keyed by the
  peer's hash. Java keeps the same kind of per-peer outbound state.

### 3.5 The transport's own dial scope bounds the pool

- `crates/i2pr-runtime/src/ssu2_runtime.rs:647` — `Ssu2DialTarget::new`
  refuses a non-loopback address (`:660`).
- `crates/i2pr-runtime/src/ssu2_runtime.rs:679` —
  `Ssu2DialTarget::new_private_lan` accepts only an RFC1918 IPv4 literal
  (`:692`).
- `crates/i2pr-daemon/src/router_i2np.rs:2480` —
  `daemon_dial_target_for_profile` chooses between those two constructors
  from the configured qualification profile.

**Consequence.** The pool can only dial a peer the transport is already
allowed to dial. In the controlled-loopback profile that is a loopback
peer; in the private-LAN profile it is an RFC1918 IPv4 peer. A public
deployment's peer population is **not dialable at all**, so the pool finds
no candidate and refuses — the same bound Plan 505 recorded for the
exploratory build driver. This is a **transport-boundary** limit with its
own plan-of-record, not something this plan may widen.

### 3.6 What already exists to reuse

- `crates/i2pr-runtime/src/ssu2_runtime.rs:3592` — `dial_ssu2`, the one
  dial.
- `crates/i2pr-runtime/src/ssu2_runtime.rs:1763` — `session_peer_links`, the
  live links for one peer.
- `crates/i2pr-runtime/src/ssu2_runtime.rs:1786` — `active_peer_hashes`, the
  peers holding at least one live session.
- `crates/i2pr-runtime/src/ssu2_runtime.rs:4415` — `construct_session`, the
  one place a `SessionConfig` is built; `:4438` sets `idle_timeout_ms`.
- `crates/i2pr-runtime/src/ssu2_runtime.rs:1131` — `ActiveSession`, whose
  `:1134` `direction` field already distinguishes an outbound session from
  an inbound one.
- `crates/i2pr-daemon/src/bootstrap.rs:339` —
  `validated_router_info_snapshot`, the bounded validated-peer snapshot.
- `crates/i2pr-daemon/src/config.rs:2005` — `Ssu2Config`, where the opt-in
  belongs.
- `crates/i2pr-daemon/src/config.rs:1447` —
  `default_ssu2_max_active_sessions` returns `64`, well above any pool
  ceiling this plan may choose.

## 4. Invariants that must not regress

1. **No new dial path.** The pool dials through `ensure_peer_session_for_record`
   and nothing else. A second dial seam would be a second authority over
   who may be contacted.
2. **No new task, no new timer, no new listener, no new socket.** The pool
   is driven by the product's existing tick; the keepalive is driven by the
   transport's existing session scheduler.
3. **No unbounded channel, queue or table.** Every table the pool owns has
   a ceiling that **refuses** rather than evicts.
4. **The default router's wire behaviour is unchanged.** With the opt-in
   off, the transport sends no keepalive and the daemon dials nothing it did
   not already dial.
5. **Fail closed.** A peer the router cannot validate, cannot dial, or has
   no session for is **refused and counted by reason** — never skipped
   silently, and never reported as a success.
6. **The publication's semantics are unchanged.** The pool does not alter
   the publication's selection, its cycle bound, its period, its
   confirmation, or its retry. It only makes a session exist.
7. **Nothing is advertised.** `router.version` stays `0.9.62`, the published
   RouterInfo's content is unchanged, `caps` is unchanged, and the
   inventory's advertised count stays zero.
8. **No peer profile is written.** The pool touches no peer-profile store:
   a dial outcome is counted in the pool's own counters and nowhere else.
   A local condition must never be written into a peer's profile.
9. **No new dependency.** `Cargo.toml` and `Cargo.lock` stay byte-identical.
10. **The inbound path is untouched.** The keepalive is an outbound-session
    behaviour; an inbound session's lifetime is unchanged.

## 5. Scope

### In scope

- A transport-level **session keepalive** for **outbound** sessions: a
  padding-only data packet that requests an immediate ACK, sent when the
  session has been idle for a bounded interval, with its own unanswered
  deadline.
- A daemon-level **bounded outbound peer-session pool**: a bounded set of
  peers the router keeps sessions to, selected through the router's own
  version-gated floodfill selection, dialled through the existing narrow
  seam, and maintained on the product's existing tick.
- The wiring: an explicit, default-off opt-in, and the counters that make
  the pool's posture observable.

### Out of scope

- **Any change to the publication.** Its selection, cycle, period,
  confirmation, retry and delivery path are untouched.
- **Any change to the peer-profile store** or to Plan 491's version gate.
- **Any change to the inbound session lifetime** or to the transport's
  handshake.
- **Any widening of the transport's dial scope.** A public-internet dial
  scope is a transport-boundary change with its own plan-of-record; Plan
  505 recorded the same bound.
- **Any advertisement.** No `caps`, no version change, no published
  capability.
- **The stock-peer half.** That a stock peer tolerates a held session, and
  that a stock floodfill is served over one, is Plan 493's blocked scope.

## 6. Required production changes

1. `crates/i2pr-transport-ssu2/src/session.rs` — `SessionConfig` gains an
   optional keepalive interval; the session's `poll` emits the keepalive
   and enforces its unanswered deadline.
2. `crates/i2pr-runtime/src/ssu2_runtime.rs` — the runtime's session
   configuration carries the keepalive interval; `construct_session` passes
   it for **outbound** sessions only.
3. `crates/i2pr-daemon/src/peer_pool.rs` — **new**: the bounded pool owner.
4. `crates/i2pr-daemon/src/service_product.rs` — the pool is installed in
   the product composition and driven on the existing tick.
5. `crates/i2pr-daemon/src/config.rs` — the opt-in and its refusal rules.
6. `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` — the pool's counter
   cell, so the posture is observable.
7. `crates/i2pr-daemon/src/lib.rs` — the composition wiring.

## 7. Design decisions the plan must make explicit

**D1 — The pool's population is the router's own version-gated floodfill
peers.** The pool selects through `select_version_gated_floodfills` over the
router's own NetDB store — the **same** selection the publication uses
(`crates/i2pr-daemon/src/router_info_publication.rs:539`, `:553`). The pool therefore
never names a peer the router's own version gate would refuse, and the
pool's population and the publication's targets are derived from **one**
order and cannot drift apart.

**D2 — The pool's ceiling is one publication cycle's worth of peers.** The
cycle names fifteen floodfills (`PUBLICATION_CYCLE_TARGETS = 15`), so a pool
of fifteen serves **every** batch of a cycle from an established session,
and the cycle's next round finds the same fifteen still held. A smaller pool
would serve only the nearest batch and leave the rest of the cycle on the
handshake path, which is the defect this plan removes. Fifteen is well under
the runtime's own session ceiling of sixty-four
(`crates/i2pr-daemon/src/config.rs:1447`).

**D3 — The pool records intent; the transport owns the sessions.** The pool
holds the set of peers it wants held. It never holds a session, a socket or
a link; it asks the runtime whether a peer already holds one
(`session_peer_links`) and dials only when it does not.

**D4 — The pool dials only through `ensure_peer_session_for_record`.** No
second dial seam, no second peer-material resolver, no second authority
over who may be contacted.

**D5 — The keepalive is a transport capability, and it is off unless the
pool is on.** The default router's wire behaviour is unchanged: with the
opt-in off the transport sends no keepalive at all.

**D6 — The keepalive is padding-only and requests an immediate ACK**, which
is i2pd's exact shape (`libi2pd/SSU2Session.cpp:269`). No I2NP message, no
new I2NP type, no new SSU2 block type, and the proto crate is untouched.
The immediate-ACK request is what makes the keepalive *observable*: the
peer's ACK refreshes this router's receive timestamp, so a keepalive that
goes unanswered is distinguishable from one that was merely lost.

**D7 — An unanswered keepalive terminates the session, because the idle
timeout cannot.** The keepalive refreshes the session's **send** timestamp,
so the idle timeout — which reads the *maximum* of send and receive
(`crates/i2pr-transport-ssu2/src/session.rs:2543-2551`) — never fires while
keepalives flow. The keepalive therefore carries its own deadline, and the
session terminates locally under its own reason when that deadline passes.
Without this rule a pool would hold a dead session forever.

**D8 — The keepalive applies to outbound sessions only.** The runtime's
`ActiveSession` already records the direction
(`crates/i2pr-runtime/src/ssu2_runtime.rs:1134`), so the inbound path is
untouched and an inbound session's lifetime is exactly what it is today.

**D9 — The pool writes no peer profile.** A dial outcome is this router's
condition as often as the peer's, and the pool cannot tell them apart, so
the pool counts its outcomes in its own counters and touches no
peer-profile store at all. This is Plan 521's discipline: a local condition
must never be written into a peer's profile.

**D10 — The publication is not changed.** The publication's existing gate
already reuses a session when one exists
(`crates/i2pr-daemon/src/router_info_publication.rs:1329`); the pool's only
contribution is that the session is there.

**D11 — The opt-in is off by default.** `ssu2.peer_pool` follows the
`ssu2.peer_test` precedent (Plan 521): an explicit opt-in, off by default,
refused without the SSU2 service, inside a `deny_unknown_fields` section.
There is deliberately **no** count knob: a key that could disagree with the
pool's ceiling would be a second source of truth, exactly as Plan 505
decided for its pair count.

**D12 — The pool is refused when the idle timeout is too short for the
keepalive.** The keepalive's unanswered deadline must fall before the
session's idle timeout, or the two would disagree about when a session
dies. The configuration parser refuses the combination by name.

## 8. Work packages

### WP1 — the transport's outbound session keepalive

- `SessionConfig` gains `keepalive_interval_ms: Option<u64>`; `None`
  disables the keepalive.
- `Ssu2Session::poll` emits a **padding-only** data packet with the
  immediate-ACK flag set when the session has been idle for the interval.
- The session records the keepalive's send time and clears it when any
  inbound packet arrives; when the unanswered deadline passes, the session
  terminates locally under its own reason code.
- The keepalive is bounded: one packet per interval, no queue, no
  retransmission (a lost keepalive is answered by the deadline, not by a
  retransmit storm).
- The keepalive respects the existing congestion gate: it is not sent while
  the window is full, and it does not bypass the payload budget.
- Tests in the transport crate: the keepalive fires at the interval, not
  before; it is padding-only; it requests an immediate ACK; an answer
  clears the deadline; an unanswered keepalive terminates the session; a
  disabled keepalive sends nothing; the idle timeout still fires on a
  session with no keepalive.

### WP2 — the runtime's keepalive wiring

- The runtime's session configuration carries the keepalive interval.
- `construct_session` takes the direction and passes the interval for
  outbound sessions only.
- No new task, no new socket, no new timer: the transport's existing
  session scheduler drives the keepalive, and the existing
  `SessionAction::Transmit` path carries it out.
- The runtime refuses a keepalive interval that is not strictly positive,
  and refuses one that is not below the idle timeout.

### WP3 — the daemon's bounded outbound peer-session pool

- **New** `crates/i2pr-daemon/src/peer_pool.rs` with a `PeerPoolOwner`:
  - a bounded set of peers it wants held, drawn from the router's own
    version-gated floodfill selection over the NetDB store;
  - a bounded set of peers it is currently establishing, so a peer is
    dialled once at a time;
  - per-peer state: last dial attempt, last established session, outcome;
  - counters by reason: candidates examined, admitted, dialed, established,
    lapsed, re-dialed, and every refusal reason.
- The pool **refuses rather than evicts**: a candidate that cannot be
  validated, cannot be dialled, or is already being dialled is refused and
  counted.
- The pool never dials a peer that already holds a session.
- The pool owns no task, no timer, no listener and no socket.

### WP4 — the wiring and the default-off opt-in

- `ssu2.peer_pool`: an explicit opt-in, **off** by default, refused unless
  `ssu2.enabled` is true, inside the `deny_unknown_fields` `[ssu2]`
  section.
- The keepalive interval is derived, not configured: the transport's
  constant is the single source of truth, and the parser refuses the
  combination of a pool and an idle timeout that is too short for it.
- The pool is installed in the product composition and driven on the
  product's existing tick, beside Plan 521's peer-test drive.
- The pool's counters are published into the inspection seam.

### WP5 — the lane, the guard, the documentation and the inventory

- A black-box lane that drives two real daemon-owned SSU2 services on
  loopback and proves the pool's behaviour end to end.
- A static guard that proves the pool's boundaries: no task, no timer, no
  listener, no socket, no second dial path, no peer-profile write, the
  opt-in off by default, the ceilings in force, and the transport's
  keepalive bounded.
- `specs/support.toml` gains the new surfaces, **experimental**, with
  `advertised = false`.
- `specs/CONFORMANCE.md`, `specs/QUALIFICATION-GATES.md`,
  `specs/protocols/09-ssu2.md` and the affected
  `docs/architecture/i2pr-*.md` deep-dives are updated.

## 9. Failure, cancellation, restart, and contention semantics

- **The pool's dial is bounded** by the publication's own delivery timeout
  shape and by the transport's own handshake timeout. A dial that does not
  finish is refused under its own reason and counted; it is never retried
  inside the tick that started it.
- **Cancellation** flows from the product's existing cancellation token into
  the dial. A cancelled dial releases the pool's "being dialled" slot, so a
  cancelled pool cannot wedge itself.
- **Restart** re-establishes the pool from scratch: the pool holds no
  durable state, so a restart costs one re-dial of the population and
  nothing else. The runtime's own session state is the only durable thing
  and it is not durable.
- **Contention** between the pool's dial and a need-driven dial of the same
  peer is resolved by the transport's own per-address pending guard
  (`pending_outbound_addrs`), which refuses a second concurrent dial to the
  same address. The pool counts that refusal; it does not retry inside the
  tick.
- **The keepalive never outlives its session.** A session that is
  terminated stops emitting keepalives at once, because the emit path is
  inside the session's own `poll` and the session is dropped with it.
- **The pool's tables are bounded and refuse rather than evict.** A router
  under flood cannot grow the pool past its ceiling.

## 10. Compatibility and migration

- **Wire compatibility.** The keepalive is a padding-only SSU2 data packet
  with the immediate-ACK flag set — the exact shape i2pd 2.61.0 already
  sends (`libi2pd/SSU2Session.cpp:269`). No I2NP message type is added, no
  SSU2 block type is added, and the proto crate is untouched. With the
  opt-in off the router sends nothing it did not already send.
- **Configuration compatibility.** The new key is additive and defaults to
  off. A configuration that does not mention it parses exactly as before.
- **No migration.** The pool holds no durable state; a restart is a
  re-establishment, not a migration.
- **No capability claim.** The published RouterInfo, `caps` and
  `router.version` are unchanged, and the inventory's advertised count
  stays zero.

## 11. Required tests

### The transport's keepalive

1. The keepalive fires at the configured interval and not before it.
2. The keepalive packet carries a padding block and **no** I2NP message.
3. The keepalive requests an immediate ACK.
4. An inbound answer clears the keepalive's deadline.
5. An unanswered keepalive terminates the session under its own reason.
6. A disabled keepalive sends nothing at all.
7. The idle timeout still fires on a session with no keepalive configured.
8. The keepalive does not bypass the congestion gate or the payload budget.

### The pool's behaviour

9. The pool is off by default and dials nothing.
10. The pool admits at most its ceiling of peers and **refuses** the rest.
11. A peer that already holds a session is not dialled again.
12. A peer whose session lapsed is re-dialed, and the re-dial is counted.
13. A candidate with no dialable SSU2 address is refused and counted.
14. A candidate the version gate refuses is never admitted.
15. A dial that fails is counted under its own reason and **writes no peer
    profile**.
16. The pool never exceeds its per-tick dial budget.
17. The pool's own dial budget is bounded by its cadence, and the cadence
    is bounded by the product's tick.

### The negative controls

18. The publication's delivery path is **unchanged** by the pool's
    existence: with the pool off, the router dials exactly as it did.
19. With the pool on and a session present, the publication's delivery
    performs **no** handshake.
20. With the pool on and **no** session, the publication's delivery dials
    exactly as it did before.

### The guard's inverted probes

Each guard rule has an inverted probe that must fail: a pool that owns a
task, a timer, a listener, a socket, a second dial path, a peer-profile
write, an unbounded table, or a default-on opt-in must be **caught**.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --test peer_pool -- --test-threads=1
cargo test --locked -p i2pr-daemon --test peer_pool_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-console-boundaries.sh
bash scripts/check-console-browser-security.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-config-secret-hygiene.sh
python3 scripts/check-tooling-inventory.py
python3 scripts/check-license-metadata.py
python3 scripts/check-managed-app-private-client-seams.py
python3 scripts/check-router-readiness-contract.py
python3 scripts/check-core-router-recovery-contract.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

## 13. Documentation updates

- `specs/support.toml` — the new surfaces, **experimental**, `advertised =
  false`; the advertised count stays zero.
- `specs/CONFORMANCE.md` — the inventory paragraph and a section for this
  plan's scope and its limits.
- `specs/QUALIFICATION-GATES.md` — the gate this plan's evidence opens, and
  the ones it does not.
- `specs/protocols/09-ssu2.md` — the keepalive's scope: a padding-only
  outbound keepalive, its cadence, and the fact that it is off by default.
- `docs/architecture/i2pr-transport-ssu2.md` — the keepalive and its
  unanswered deadline.
- `docs/architecture/i2pr-runtime.md` — the wiring.
- `docs/architecture/i2pr-daemon.md` — the pool, its ceilings, its opt-in
  and its counters.
- `plans/subsystems/network-participation-roadmap.md` — the **Transports**
  row's missing cell, the milestone row, and the consequence paragraph.

## 14. Acceptance criteria and stop conditions

The plan is **complete** when all of the following hold:

1. The transport emits a padding-only, immediate-ACK keepalive on an idle
   outbound session at a bounded interval, and terminates a session whose
   keepalive goes unanswered.
2. The daemon holds a bounded pool of outbound sessions to peers drawn from
   its own version-gated floodfill selection, dialled through the one
   existing seam, and re-establishes a peer whose session lapsed.
3. The publication's delivery path is **byte-for-byte unchanged**, and with
   the pool on and a session present the publication performs no handshake.
4. The opt-in is off by default, the default router's wire behaviour is
   unchanged, and the parser refuses a pool whose idle timeout is too short
   for the keepalive.
5. Every refusal is counted by reason and nothing is skipped silently.
6. No peer profile is written by the pool.
7. The lane and the guard pass, and each guard rule has an inverted probe
   that fails.
8. `router.version` stays `0.9.62`; the published RouterInfo and `caps` are
   unchanged; the advertised count stays zero; `Cargo.toml` and
   `Cargo.lock` are byte-identical.
9. The full routine floor is green.

**Stop conditions.** The plan stops and records the finding if any of these
holds:

- The keepalive cannot be made to request an immediate ACK without a new
  SSU2 block type or a new I2NP message type. The plan does not add one.
- The pool cannot be made to hold its ceiling without a second dial path.
- The publication's delivery path would have to change for the pool to
  mean anything. It must not.
- The pool would have to write a peer profile to be useful. It must not.
- The pool's population would have to be widened past the transport's own
  dial scope. It must not.

## 15. Closure evidence required

- The closure record names the **requirement-to-evidence matrix**, the
  focused verification commands and their outcomes, the routine floor's
  row-by-row result, the negative controls, the invariant review, the
  failure/cancellation/restart/contention review, the migration and
  compatibility review, the security review, the limitations, the findings
  by severity, the roadmap disposition, and the unblock audit.
- The lane's evidence is a **black-box** result through the daemon's own
  service surface, not a private-API call.
- The guard's evidence is each rule **plus** its inverted probe.
- The stock-peer half is recorded as **blocked**, with its ground.

## 16. Handoff notes

- The next free plan number after this one is **523**.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`). Neither reference is
  patched or vendored.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology.
- The **dial-scope** limit is a transport boundary and needs its own
  plan-of-record; it is not discharged here.
- The **outbound peer-session pool** is discharged by this plan; the
  roadmap's Transports cell must be rewritten to say so, in the same form
  the RouterInfo row was rewritten for Plan 520.

## 17. Amendment log

> The amendment log is **additive**. A closed closure record is never
> rewritten; a correction lives here and in the successor's closure record
> plus the registry row.

### 17.1 — 2026-11-08, registration

The plan is registered as written. No amendment.
