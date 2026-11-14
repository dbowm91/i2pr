# Plan 519 — the publication's delivery leaves the transport's loop

**Status:** registered. **Subsystem:** network-participation. **Class:**
capability (the publication stops stalling the transport's service loop). **Predecessor:**
Plan 515 (the periodic publication), Plan 516 (the acknowledgement), Plan 517 (the retry
ladder), Plan 518 (the fresh record). This plan closes the **Medium** finding **M1** that
Plan 517's closure record and Plan 518's closure record both carry open.

## 1. Objective

The router's own RouterInfo publication must not be able to stall the SSU2 service's
`select!` loop.

One bounded outcome: the publication runs on **its own task**, with explicit ownership,
explicit cancellation and its own bounded delivery timeout, and the SSU2 service's loop
never awaits it. Nothing else about the publication changes — not the ladder, not the
correlator, not the cycle, not the record, not the counters.

## 2. Why ready

- **Plan 515** closed the periodic publication (`plans/closure/network-participation/515-status.md`).
- **Plan 516** closed the acknowledgement and the correlator (`516-status.md`).
- **Plan 517** closed the retry ladder and the cycle (`517-status.md`).
- **Plan 518** closed the record's freshness and the cycle's one record (`518-status.md`).
- The interface this plan needs is **stable and written**: the SSU2 runtime service is
  `Clone` over shared `Arc` state
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:1400-1401`), so a task other than the
  service's loop can hold a clone and use the dial and send seams
  (`:3480` `dial_ssu2`, `:3722` `send_i2np`) concurrently with the loop. The established
  peers are readable at `:1767` `active_peer_hashes`.
- No ADR is needed: the shape is settled by the two pinned references (§3) and by the
  I2P documentation, and it is recorded in the decisions below.

## 3. Current implementation evidence

### The defect

`crates/i2pr-daemon/src/lib.rs:5056-5102` is the publication arm of the SSU2 service's
`select!` loop. It awaits `refresh_router_info(...)`, which awaits
`publish_router_info(...)`, which awaits
`ensure_peer_session_for_record(...)` (`crates/i2pr-daemon/src/service_product.rs:7438`),
which awaits `Ssu2DaemonHandle::dial` (`crates/i2pr-daemon/src/router_i2np.rs:2349`),
which awaits `Ssu2RuntimeService::dial_ssu2`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3480`).

While that await is pending the `select!` loop cannot run its other arms. The loop also
carries the **relay drain** and the **inbound acknowledgement owner**
(`crates/i2pr-daemon/src/lib.rs:4752-4754`), so a slow dial delays the relay queue and
the correlation of an inbound `DeliveryStatus`. That is the **Medium** finding **M1**,
recorded open in `plans/closure/network-participation/517-status.md` and carried in
`518-status.md`.

The transport's **socket** I/O is not the victim: `i2pr-runtime` owns the sockets, the
timers and the handshake machinery, so a handshake completes while the daemon's service
loop is stalled. What is lost is the **service layer's** other work.

### What the references do

Both pinned references never block the transport's service loop on a handshake.

**i2pd 2.61.0** (pin `635b013a612ff47278ef02acf8580a28e10e26c5`):

- The publish timer lives on the router's **main service thread** —
  `libi2pd/RouterContext.cpp:64` binds `m_PublishTimer` to `m_Service->GetService()` —
  and `HandlePublishTimer` (`:1470`) calls `Publish()` on that thread.
- `Publish()` does **not** wait. `libi2pd/Transports.cpp:483-496`:
  `Transports::SendMessage` does
  `boost::asio::post(*m_Service, boost::asio::use_future(...))` and returns a future
  that the publisher **ignores**. The handshake is the transport's, and the main thread
  is never stalled on it.
- The publish **prefers a peer it is already connected to**:
  `libi2pd/RouterContext.cpp:1502-1521` takes the direct route when
  `i2p::transport::transports.IsConnected(floodfill->GetIdentHash())`, and otherwise
  sends the store **out an exploratory tunnel**. It never dials on demand for a publish.
- When neither route is available it logs and retries — it does not block:
  `libi2pd/RouterContext.cpp:1511` arms the resend timer for
  `ROUTER_INFO_CONFIRMATION_TIMEOUT` (1600 ms, `libi2pd/RouterContext.h:37`).

**Java I2P 2.13.0** (pin `9134f808337b401e8e53c73734c81fab04280c9d`):

- The publication is a **job**, not a service-loop arm.
  `router/java/src/net/i2p/router/networkdb/PublishLocalRouterInfoJob.java` extends
  `JobImpl`, and `net/i2p/router/JobQueue.java` runs jobs on its own **runner threads**
  (`:163` `JobQueueRunner`, `:352-354` the pumper thread). The job **requeues** with a
  delay; it never blocks.
- The store is handed to **another job with its own timeout**:
  `FloodfillNetworkDatabaseFacade.java:246-257` `sendStore` adds a
  `FloodfillStoreJob` to the job queue, and
  `FloodfillNetworkDatabaseFacade.java:174` `PUBLISH_TIMEOUT = 90*1000` bounds it.
- The delivery is **fire-and-forget**: `FloodfillStoreJob` extends `StoreJob`, and
  `router/java/src/net/i2p/router/networkdb/kademlia/StoreJob.java:415-441`
  `sendDirect` builds an `OutNetMessage` and calls
  `getContext().commSystem().processMessage(m)`. The transport owns the dial; the reply
  arrives later and is matched by the message registry
  (`getContext().messageRegistry().registerPending(m)`).
- The direct-or-tunnel choice is explicit:
  `StoreJob.java:392-410` takes the direct route when
  `ctx.commSystem().isEstablished(h)` or the peer is connectable, and otherwise
  `sendStoreThroughExploratory(...)`.

**The I2P documentation** states the same rule in prose.
<https://i2p.net/en/docs/overview/network-database/> ("RouterInfo Storage to
Floodfills"):

> A router publishes its own RouterInfo by directly connecting to a floodfill router and
> sending it a I2NP DatabaseStoreMessage with a nonzero Reply Token. […] In some
> circumstances, a router may also send the RouterInfo DatabaseStoreMessage out an
> exploratory tunnel; for example, due to connection limits, connection incompatibility,
> or a desire to hide the actual IP from the floodfill.

The same page also fixes the **cadence** the plan must not break: "Floodfills use a short
expiration time (1 hour) for all local RouterInfos", and Java's own comment on
`PUBLISH_DELAY = 43*60*1000` reads "Too long and the floodfill will drop us — timeout is
60 minutes". i2pr's period is `39 * 60` seconds (`RouterContext.h:34`'s value), which is
inside that window with margin. **No cadence change is needed.**

### What this tree already has

- `Ssu2SendOutcome::Closed` (`crates/i2pr-runtime/src/ssu2_runtime.rs:770-771`) is the
  typed "no active session for this peer" signal.
- `Ssu2RuntimeService::active_peer_hashes` (`:1767`) reads the established peers.
- `Ssu2RuntimeService::send_i2np` (`:3722`) **queues** a message onto the current
  session and returns immediately — the exact shape of Java's `processMessage` and
  i2pd's `post`.

### What this tree does not have

**Exploratory tunnels.** i2pr installs none in production, so the reference's
*tunnel* route is unreachable here. The *direct* route is the only one, and it is the
one that must not block.

## 4. Invariants that must not regress

1. **The SSU2 service's loop never awaits the publication.** No `.await` on the
   publication's work may appear inside the SSU2 service's `select!` arms.
2. **The transport's socket I/O is never blocked by the publication.** The publication
   owns its own task; the runtime keeps owning the sockets.
3. **One cycle publishes one record.** Plan 518's invariant 7 holds: the first batch and
   every rung publish the same bytes, and the record is the owner's.
4. **The ladder's timing is unchanged.** The confirmation deadline is still the
   reference's 1600 ms, measured from the moment the batch went out.
5. **The acknowledgement is still correlated.** Plan 516's correlator still matches the
   inbound `DeliveryStatus` to the attempt, and an unmatched token is still refused and
   counted.
6. **Every refusal is counted under its own reason.** Moving the publication must not
   lose a counter, and must not merge two reasons.
7. **The publication's record is still fresh.** Plan 518's staleness rule still refuses a
   stale or too-far-future record before any attempt is begun.
8. **Nothing is advertised.** `router.version` stays `0.9.62`; `caps` and the published
   RouterInfo's content are unchanged; the inventory's advertised count stays zero.

## 5. Scope

### In scope

- The publication's **execution context**: its own task, its own ownership, its own
  cancellation, its own bounded delivery timeout.
- The handoff of the inbound `DeliveryStatus` from the SSU2 service's loop to the
  publication's task, over a **bounded** channel.
- The publication's **target selection**: prefer a peer with an established session.
- The SSU2 service's loop: the publication arm is removed from it.
- The publication's own ticks (the period tick and the ladder tick) move with it.

### Explicitly out of scope

- **Exploratory tunnels.** The reference's tunnel route is unreachable in this tree, and
  installing tunnels is a different plan's work.
- **The cadence.** The publication period stays `39 * 60` seconds. The reference's
  variance is not adopted.
- **The congestion update.** The bandwidth-metering line is unresolved here and in both
  references.
- **The selection's Kademlia rule.** Plan 497's version gate and the closeness metric
  stay as they are; "prefer an established session" is an **additional** preference
  applied on top, exactly as i2pd's `IsConnected` and Java's `isEstablished` are.
- **Any wire change.** The store message, the reply token and the confirmation are
  unchanged.
- **Any advertisement.** Nothing is promoted.

## 6. Required production changes

- `crates/i2pr-daemon/src/router_info_publication.rs` — the publication's owner, its
  schedule, its cycle, its correlator and its cycle record move into a **publication
  task**; the task owns the ticks and the delivery timeout; the selection gains the
  established-session preference.
- `crates/i2pr-daemon/src/lib.rs` — the composition root spawns the publication task
  with explicit ownership and cancellation; the SSU2 service's loop loses the
  publication arm and gains a **bounded** handoff for the inbound `DeliveryStatus`.
- `crates/i2pr-daemon/src/router_i2np.rs` — the SSU2 service's loop hands the inbound
  acknowledgement to the publication's handoff instead of calling the owner directly.
- `crates/i2pr-runtime/src/ssu2_runtime.rs` — **no change is expected**: the cloneable
  service, `active_peer_hashes` and `send_i2np` already exist.

## 7. Design decisions the plan must make explicit

- **D1 — The publication runs on its own task.** Java's model: the publication is a job
  on its own pool, not an arm of the transport's service loop. i2pd's model is
  equivalent in effect, because i2pd's send never blocks the main thread either.
- **D2 — The publication owns its state.** The schedule, the cycle, the correlator and
  the cycle record live with the task. The SSU2 service's loop holds no publication
  state, so there is exactly one owner of the cycle's state.
- **D3 — The inbound acknowledgement crosses a bounded channel.** The SSU2 loop hands
  the `DeliveryStatus`'s message id to the publication's task over a bounded channel
  and **never awaits** it. The channel is bounded, and a full channel counts the
  acknowledgement as **unmatched** — the same refusal Plan 516 already defines — rather
  than blocking the loop or dropping it silently.
- **D4 — The delivery timeout belongs to the publication.** Java's `PUBLISH_TIMEOUT`
  is 90 s per store attempt. i2pr's publication owns a bounded per-attempt delivery
  timeout of its own, so a hung dial can hold neither the transport's loop nor the
  publication's task indefinitely.
- **D5 — The selection prefers an established session.** i2pd's `IsConnected` and
  Java's `isEstablished` both prefer a peer the router is already connected to, because
  that is the peer whose send needs no handshake. i2pr applies the same preference on
  top of the version-gated closeness selection, and **records** how many targets were
  skipped because they had no session.
- **D6 — No exploratory-tunnel route.** i2pr installs none, so the direct route is the
  only one. When no target has a session, the batch is **refused under its own reason**
  and counted — never blocked on.
- **D7 — The ticks move with the publication.** The 39-minute period tick and the
  ladder's tick belong to the publication's task. The SSU2 service's ticks are
  untouched, and **no new timer** is added to the transport's loop.
- **D8 — The task is owned and cancelled.** The publication's task is spawned by the
  composition root with an explicit child scope and an explicit `CancellationToken`;
  a shutdown cancels it and joins it. No ownerless `spawn`, no detached task.
- **D9 — Nothing is advertised and nothing about the wire changes.**

## 8. Work packages

- **WP1 — The publication's task.** The owner, its schedule, its cycle, its correlator
  and its cycle record move into a task that the composition root spawns with explicit
  ownership and cancellation. The task runs the period tick and the ladder tick.
- **WP2 — The bounded acknowledgement handoff.** The SSU2 service's loop hands the
  inbound `DeliveryStatus`'s message id to the publication's task over a bounded
  channel; a full channel counts the acknowledgement as unmatched.
- **WP3 — The established-session preference.** The selection prefers a peer with an
  established session and counts the targets it skipped for want of one.
- **WP4 — The publication's delivery timeout.** A bounded per-attempt delivery timeout
  owned by the publication, in the shape of Java's `PUBLISH_TIMEOUT`.

## 9. Failure, cancellation, restart, and contention semantics

- **A cancelled publication** stops at its next await, the cycle is halted, and the
  counters record the cancellation under its own reason.
- **A restart** rebuilds the publication's task and its state; the record is re-dated
  and re-signed by Plan 518's path, and the first publication is due immediately
  (`is_due` returns true while `last_attempt_ms` is `None`).
- **A full acknowledgement channel** counts the acknowledgement as unmatched. The loop
  is never blocked and the acknowledgement is never dropped silently.
- **A delivery timeout** ends the attempt, counts it under its own reason, and lets the
  ladder move to the next target. It never holds the publication's task.
- **Contention** — one publication task, one cycle at a time; a period tick that finds
  a cycle already running leaves it alone (Plan 517's rule).

## 10. Compatibility and migration

- No wire change; no configuration key; no dependency. `Cargo.toml` and `Cargo.lock` stay
  byte-identical.
- The published RouterInfo's content is unchanged.
- Plan 515's, Plan 516's, Plan 517's and Plan 518's lanes and guards stay green, and
  their guard files stay byte-identical.

## 11. Required tests

**The lane** (`crates/i2pr-daemon/tests/router_info_publication_off_loop.rs`):

- the SSU2 service's loop keeps running its other arms while a publication's dial is
  slow — the loop's relay drain and its ticks must still make progress;
- the publication's task is cancelled and joined on shutdown, and no task is left
  behind;
- the selection prefers a peer with an established session, and counts the targets it
  skipped for want of one;
- a full acknowledgement channel counts the acknowledgement as unmatched and does not
  block the loop;
- the delivery timeout ends the attempt under its own reason;
- the ladder's timing, the correlator's matching and Plan 518's staleness rule are
  unchanged.

**The guard** (`crates/i2pr-daemon/tests/router_info_publication_off_loop_boundary.rs`):

- the SSU2 service's loop does **not** await the publication;
- the publication's state is **not** owned by the SSU2 service's loop;
- the publication's task is spawned with **explicit ownership and cancellation**;
- the acknowledgement handoff is **bounded**;
- the selection prefers an **established session**;
- the publication's delivery timeout is **bounded** and owned by the publication;
- no new timer is added to the transport's loop;
- each rule has an **inverted probe**.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test router_info_publication_off_loop -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_off_loop_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_refresh -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_retry -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
```

Plus the routine floor in `AGENTS.md`, verbatim.

## 13. Documentation updates

- `specs/support.toml` — the publication's surface gains the off-the-loop property; the
  inventory's counts are re-derived and the advertised count stays **zero**.
- `specs/CONFORMANCE.md` — what is proved here, and what is **not**: the live
  stock-peer half stays blocked on Plan 493's topology.
- `specs/QUALIFICATION-GATES.md` — a new section with each rule's gating test.
- `docs/architecture/i2pr-daemon.md` — the publication's new execution context.
- `docs/architecture/i2pr-runtime.md` — if the runtime's seam changes at all; it is
  expected not to.
- `plans/closure/network-participation/517-status.md` and `518-status.md` are **not**
  rewritten. The disposition of their **M1** finding lives in this plan's closure record
  and in the registry's rows.

## 14. Acceptance criteria and stop conditions

**Accepted when:**

1. the SSU2 service's loop never awaits the publication, and a lane test proves the
   loop's other arms progress while a publication's dial is slow;
2. the publication runs on an owned, cancellable task that is joined on shutdown;
3. the acknowledgement handoff is bounded and a full channel is counted, not dropped;
4. the selection prefers an established session and counts the targets it skipped;
5. the delivery timeout is bounded and owned by the publication;
6. the ladder's timing, the correlator's matching and the staleness rule are unchanged;
7. Plans 515, 516, 517 and 518's lanes and guards are green and their guard files are
   byte-identical;
8. the routine floor is green and the negative controls are all caught by an assertion.

**Stop, do not continue, when:**

- the publication cannot be moved off the loop without a wire change;
- the SSU2 runtime's cloneable-service seam proves insufficient, and the plan would
  have to weaken the runtime's ownership boundary;
- the ladder's timing or the correlator's matching would have to change;
- any advertisement would be required;
- the reference's tunnel route would have to be installed to make the publication
  succeed — that is a different plan's scope, and this plan must not smuggle it in.

## 15. Closure evidence required

- The commits, in order.
- A requirement-to-evidence matrix naming the lane test and the guard test for each
  invariant.
- The exact commands run, with their outcomes, labelled local or CI.
- The routine floor's step count and its PASS/FAIL summary.
- The negative controls: each mutation, the detector, and the assertion it failed on.
- Invariant, failure/cancellation/restart/contention, migration and security reviews.
- Limitations, findings by severity, the roadmap disposition, and the unblock audit.

## 16. Handoff notes

- The next free plan number after this one is **520**.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`).
- Do not patch or vendor either reference.
- The **Medium** finding **M1** is owned by this plan. If this plan closes, the
  publication no longer shares the transport's service loop.
- The **stock-peer** half of the publication's evidence stays blocked on Plan 493's
  authorized two-host topology. This plan does not and cannot unblock it.

## 17. Amendment log

The plan of record is amended as the implementation lands. Every amendment below
records a **decision that changed**, the reason, and where the change is
visible. The plan's own text is not rewritten: an amendment that reverses a
decision says so explicitly.

### 17.1 The publication's task body lives in the composition root, not in a new module

**Changed.** §6 said `crates/i2pr-daemon/src/router_info_publication.rs` would
gain the publication task, and that the composition root would spawn it. The
**task body** is in `crates/i2pr-daemon/src/lib.rs`; only the **bounded
acknowledgement handoff** is in the new module
`crates/i2pr-daemon/src/router_info_publication_task.rs`.

**Why.** Plan 515's guard (`crates/i2pr-daemon/tests/router_info_publication_boundary.rs`,
rule 1) requires the composition root to contain
`crate::router_info_publication::refresh_router_info(`; Plan 517's guard
requires it to contain `crate::router_info_publication::advance_publication_cycle(`;
and Plan 516's requires it to contain `publication_owner.confirm(`. All three
read `crates/i2pr-daemon/src/lib.rs`, and §10 and acceptance criterion 7
require those guard files to stay **byte-identical**. A task body in a new
module would have taken every one of those literals out of the composition
root and broken three earlier guards. The task's `select!` therefore stays in
the composition root, where its calls are visible, and the handoff — which
needs no guard anchor — is a separate module.

**Visible in.** `crates/i2pr-daemon/src/lib.rs`
(`run_router_info_publication_task`), `crates/i2pr-daemon/src/router_info_publication_task.rs`.

### 17.2 The publication holds a narrow, cloneable capability

**Added.** §6 did not mention it. The publication's task cannot hold the SSU2
service's handle: that handle owns the runtime's inbound handoff and is not
cloneable. `crates/i2pr-daemon/src/router_i2np.rs` therefore gains
`Ssu2PublicationHandle` — a cloneable capability over the **same** shared
runtime state, carrying the service, the bounded delivery seam and the dial
seam, and nothing else. `refresh_router_info`, `advance_publication_cycle` and
`publish_router_info` take it instead of the service handle.

**Why.** The plan requires the publication to run on a task of its own, and the
runtime's service is already `Clone` over shared `Arc` state — that is the
enabler the plan named in §2. The capability grants nothing the daemon did not
already hold, so no authority widens.

**Visible in.** `crates/i2pr-daemon/src/router_i2np.rs`,
`crates/i2pr-daemon/src/router_info_publication.rs`,
`crates/i2pr-daemon/src/service_product.rs`.

### 17.3 Decision **D6** is corrected: the batch falls back to the direct route

**Changed.** D6 read: *"When no target has a session, the batch is **refused
under its own reason** and counted — never blocked on."* Read literally, the
publication would publish **nothing** whenever the router holds no session
with a floodfill — and i2pr installs no exploratory tunnel, so the direct
route would be the only one, and it would be closed. A router that cannot
publish its own RouterInfo cannot be found by any peer, which contradicts
this plan's own objective and Plan 491's line.

**As implemented.** The selection **prefers** a peer the router is already
connected to, and the targets it drops for want of one are counted under
their own reason (`skipped_no_session`). When the batch holds **no** session
at all, the batch is asked anyway and the **direct route** establishes the
session — under the publication's own bounded delivery timeout, so it is
never blocked on. The skip count is then zero, because nothing was skipped.

**Why.** The plan's intent — the publication must never block the transport's
loop, and a peer whose send needs no handshake is preferred — is kept
verbatim. What changes is the refusal: a batch with no session is not refused
outright, it is attempted on the only route this tree has.

**Visible in.** `crates/i2pr-daemon/src/router_info_publication.rs`
(`prefer_established_sessions`, `publish_once_inner`).

### 17.4 The lane proves the public behaviour; the guard proves the wiring

**Changed.** §11 listed every acceptance case under the lane, including the
wiring: that the transport's loop loses its publication arm, that the
publication's task is spawned with ownership and cancellation, and that no
new timer is added to the loop. Those are **structural** claims about the
production files, and the tree's house pattern puts them in the **guard**
(`crates/i2pr-daemon/tests/router_info_publication_off_loop_boundary.rs`),
which reads `crates/i2pr-daemon/src/lib.rs`,
`crates/i2pr-daemon/src/router_info_publication.rs` and
`crates/i2pr-daemon/src/router_info_publication_task.rs` directly. Plan 513's
lane records the same split in its own header.

The lane (`crates/i2pr-daemon/tests/router_info_publication_off_loop.rs`)
therefore proves the publication's **public** behaviour through the public API
only: the established-session preference, the delivery timeout's bound and its
own counting reason, the bounded handoff and its unmatched counting, the
cancellation's counting, and the unchanged ladder, correlator and staleness
rules.

**Not claimed.** The **live-dial** half is not re-measured: the lane drives
the publication's behaviour without a socket, so a *live* slow dial inside
the transport's loop is not observed end to end. The **stock-peer** half stays
blocked on Plan 493's authorized two-host topology.

### 17.5 The handoff's capacity and the ladder's tick

**Added.** The acknowledgement handoff's capacity is `16`
(`PUBLICATION_ACKNOWLEDGEMENT_CAPACITY`) and the publication's ladder tick is
`100` milliseconds (`PUBLICATION_LADDER_TICK_MS`) — the transport's loop's own
cadence, moved with the ladder. The period tick stays the reference's
`39 * 60` seconds. The transport's loop gains **no** timer.

### 17.6 The composition root creates no ownerless task

**Added.** The guard's third rule also pins that
`crates/i2pr-daemon/src/lib.rs` contains no `tokio::spawn(` in production
code: the publication's task is spawned under the service's own child scope,
with an explicit cancellation, and the composition root creates no detached
task of its own.
