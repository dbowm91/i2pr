# Plan 534 — the router must build a tunnel toward the peer it reached through the relay

**Plan of record.** Subsystem: `network-participation`. Registered 2026-11-11.
The successor to Plan 533, and the named owner of the two limitations Plan 533
recorded as out of scope with an owner: *"the route by which a relayed peer is
then used for tunnel building"* and *"the relayed peer's presence in the
peer-profile store"*.

## 1. Objective

One bounded outcome: **the router that reached a peer through a relay must
build a tunnel toward that peer.**

The SSU2 relay exchange is complete. Plan 531 gave the requester its role,
Plan 532 gave the target its role, and Plan 533 gave the requester the
**establishment**: the requester now holds an authenticated session with the
peer it reached, and that session is the runtime's ordinary SSU2 handshake
carrying the target's own one-use token.

What the router still cannot do is **use** that peer. The product's
tunnel-build route draws its peer from the peer-profile store's policy
selection, so a peer the router deliberately reached through a relay is one
candidate among every peer the router knows. The deliberate reach is not
honoured: the session is established, the router knows the peer, and no build
ever names it.

This plan makes the deliberate reach **count**. The peer the router reached
through a relay becomes the route's **named** build target, subject to exactly
the same API-version floor every other build target is subject to, with its
material read from the validated store and its build submitted through the
**existing** build path.

## 2. Why ready

- Plan 533 is **closed** (`plans/closure/network-participation/533-status.md`).
  The establishment exists, the session is real, and the tree already names a
  peer's session through the runtime's own live-session set.
- The build machinery is **closed and proven**: Plan 505's exploratory build
  driver, Plan 504's liveness scheduler and Plan 507's reconcile are all
  closed. This plan adds no build path — it names a different peer on the
  **existing** one.
- The peer-profile store is **fed** in production
  (`crates/i2pr-daemon/src/lib.rs:2191`, `crates/i2pr-daemon/src/peer_reconcile.rs:363`),
  so the relayed peer's validated record reaches the store the driver reads.
- No interface dependency is open. The version floor, the selection and the
  coordinator are all closed contracts.

## 3. Current implementation evidence

### 3.1 The relayed peer is not named by the route

The exploratory build driver names its peer in exactly one place
(`crates/i2pr-daemon/src/exploratory_driver.rs:710`):

```rust
fn select_peer(&mut self, now_ms: u64) -> Option<SelectedPeer> {
    let seed = self.selection_seed(now_ms);
    let Some(selection) = self.profiles.with(|profiles| {
        profiles.advance_time(now_ms);
        profiles.select_tunnel_build_peers(1, seed)
    }) else {
        ...
```

`select_tunnel_build_peers` draws **one** candidate from the whole
peer-profile store (`crates/i2pr-daemon/src/netdb_participation.rs:393`), under
the store's own constraints: the rank floor, the participation cap, the
per-prefix diversity rule, and the version gate
(`crates/i2pr-netdb/src/selection.rs:297`).

The peer the router reached through a relay is in that store, but it is one
candidate among every peer the router knows, and nothing distinguishes it.
The deliberate reach — an expensive, explicit act — is dropped on the floor.

### 3.2 The session exists and is live

Plan 533's establishment ends with a real, authenticated session: the runtime
holds it, the requester holds the matching session, and the runtime's
live-session set names the peer. Plan 521's peer-test owner and Plan 524's
introducer request already name peers from exactly that set
(`crates/i2pr-daemon/src/service_product.rs:4786`). The product knows how to
name a peer it holds a session with; the build route simply never asks.

### 3.3 The material is available and validated

The relayed peer's validated RouterInfo is in the authoritative store — the
requester resolved the target from it to start the relay request, and Plan
533's establishment read the target's static and introduction keys from it
through the transport's strict parser
(`crates/i2pr-daemon/src/relay_establishment.rs:243`). The reconcile copies
the store's records into the peer-profile store on its cadence
(`crates/i2pr-daemon/src/lib.rs:2191`), so the driver's resolver
(`crates/i2pr-daemon/src/exploratory_driver.rs:140`) can resolve the peer
once the reconcile has run.

### 3.4 The tree's own records name this gap

Plan 533's closure record, under *"What the plan did not close"*:

> The **route** by which a relayed peer is then used for tunnel building.
> This plan establishes the session; it does not route traffic over it.
> **Owner:** a successor in the transit line.

and, under *"What the plan did not close"*, the second item:

> The **relayed peer's** presence in the peer-profile store. The
> establishment records a **local** fact about this router's session; it
> writes nothing into the peer's record, and a successor may add a local
> session observation.

This plan is that successor.

## 4. Invariants

1. **The version floor is never bypassed.** A named build target is subject to
   exactly the gate every selected target is subject to
   (`crates/i2pr-netdb/src/version.rs:87`), and a refusal is counted under the
   same reasons the selection counts.
2. **There is exactly one build path.** The named target's build is submitted
   through the **existing** `ExploratoryBuildCoordinator::rebuild_exploratory`,
   so the coordinator's state machine, the pairing and Plan 504's liveness
   scheduler are unchanged.
3. **The peer profile is not written for a local condition.** The relayed
   peer's reachability is not claimed, no corroborating class is added, and no
   peer-level observation is recorded for a peer the route did not actually
   use. The peer the router reached is recorded as a **local** observation of
   **this router's** session, in a table of this router's own.
4. **The material is validated.** The named target's build material is read
   from the **validated** store through the transport's strict parser. No
   unvalidated bytes are parsed and no material is substituted.
5. **Every bound is explicit.** The local observation table, the pending
   establishment table and the establishment handoff each have a stated
   capacity, and each refusal under a full bound is counted.
6. **A local condition is never written into a peer's profile.** Every refusal
   in this plan is a typed, counted reason about **this router's** state.
7. **The transport's relay state machines are used, not edited.** This plan
   changes no file under `crates/i2pr-transport-ssu2/`.
8. **The reachability model is untouched.** No corroborating class is claimed
   and the corroboration and contradiction floors are not changed.
9. **No advertisement.** `router.version` stays `0.9.62`, the published
   RouterInfo and `caps` are unchanged, and the inventory's advertised count
   stays **zero**.
10. **No configuration key is added.** The relayed target rides the **existing**
    `ssu2.relay_requester` opt-in and the **existing** `[exploratory]`
    posture.
11. **No boundary script is weakened.** No `|| true`, no `continue-on-error`, no
    filename filtering, no fake peer environment, no broad exclusion, no
    early-return-success, and no production wire change made to go green.

## 5. Scope

### 5.1 In scope

1. A **local session observation** of the peer the router reached through a
   relay: a bounded, expiring table in the daemon that names the peer and
   records when the observation was made. It is a fact about **this router's**
   session, not about the peer.
2. The **establishment task** records the observation when the establishment
   succeeds, and the observation is **consumed** by the route that uses it.
3. The **exploratory build route** gains a **named target**: when the local
   observation names a peer, the route builds toward **that** peer, subject to
   the same API-version floor, with the material resolved from the validated
   store and the build submitted through the existing coordinator.
4. The counters for every step, exposed as counts only.
5. A lane that proves the build toward the relayed peer over **real loopback
   UDP** with **three real routers**, and a static guard that is the authority
   for the shape.

### 5.2 Out of scope

- The **introducer's** half, the **target's** half and the **requester's**
  establishment. Plans 490, 499, 524, 531, 532 and 533 own them, and nothing
  here changes them.
- The **stock-peer** half. The delivery stays confined to the controlled
  loopback profile, and the stock-peer evidence stays blocked on Plan 493's
  authorized two-host topology.
- Any change to the transport's relay state machines, the reachability model,
  the corroboration floor or the contradiction floor.
- Any change to `Cargo.toml`, `Cargo.lock` or the dependency graph.
- The **outproxy** request path, which has no reachable caller and stays that
  way.

### 5.3 Out of scope, with the owner named

- The **production SSU2 peer-test driver** and its choreography question (the
  tree's Alice role needs the helper's identity up front, while the
  specification has Bob select the helper). The helper is a **stock** peer, so
  this half is blocked on Plan 493's authorized two-host topology. **Owner:**
  the plan that runs the production driver with a stock helper.
- Plan 530's Low finding 1 (the explicit-bind class lapses with the
  30-minute observation TTL and has no re-record timer). It is unrelated to
  the relay and stays with its owner.
- Plan 532's Low finding 2 (the target role's dependence on the bootstrap's
  store). The establishment's task has the same dependency and reads the same
  store; the composition root's line owns it.

## 6. Required production changes

### 6.1 `crates/i2pr-daemon/src/relay_session_peer.rs` (new)

- **New**: `RelaySessionPeer { peer: RouterHash, observed_at_ms: u64 }` — one
  local observation of one peer this router reached through a relay.
- **New**: `MAX_RELAY_SESSION_PEERS = 8` — the table's stated capacity.
- **New**: `RelaySessionPeerRefusal` — the typed reasons: `Disabled`,
  `TableFull`, `UnknownPeer`, `BelowVersionFloor`, `UncomparableVersion`,
  `UnknownVersion`, `MaterialAbsent`, `AlreadyPending`.
- **New**: `RelaySessionPeerOwner` — the bounded table, its expiry, and the
  counters. It owns **no** task, **no** timer, **no** listener and **no**
  socket; the product's existing tick advances it.
- **New**: `RelaySessionPeerCounterCell` — the shared cell the owner publishes
  counts into.

### 6.2 `crates/i2pr-daemon/src/relay_establishment_task.rs`

- The task records the **local observation** when the establishment succeeds.
  The peer hash comes from the dial target the task already built, so no
  identity is carried across a boundary that did not already have it.

### 6.3 `crates/i2pr-daemon/src/exploratory_driver.rs`

- The driver gains an optional **named target**: the peer the local
  observation names.
- `select_peer` (`crates/i2pr-daemon/src/exploratory_driver.rs:710`) honours
  the named target **and applies the same version gate to it** as the
  selection applies to a drawn candidate. A refusal is counted under the
  driver's **existing** version-refusal counters, so a named target that is
  below the floor is refused exactly as a drawn candidate would be.
- The driver otherwise changes **nothing**: the material resolution
  (`:571`), the build submission, the pairing and the observation path
  (`:644`) are untouched.

### 6.4 `crates/i2pr-daemon/src/service_product.rs`

- The product reads the local observation and hands the named target to the
  driver, on the **existing** tick, in the **existing** exploratory drive step
  (`crates/i2pr-daemon/src/service_product.rs:4253`).
- The product owns no new task and no new timer.

### 6.5 `crates/i2pr-daemon/src/lib.rs`

- The local observation's cell is constructed in the composition and handed to
  the establishment task and to the product.

### 6.6 `crates/i2pr-daemon/src/i2pcontrol_inspection.rs`

- The local observation's counters are published beside the establishment's
  counters, as **counts only**. The `i2pr-i2pcontrol` domain contract is
  **not** widened.

## 7. Design decisions

### 7.1 The named target is a peer the router deliberately reached

The relayed peer is not a *selection* candidate and must not be treated as
one. The selection's rank floor, participation cap and per-prefix diversity
rule exist to spread load across the peer population; a peer the router
reached **on purpose**, through an introducer, at the cost of a relay
exchange, is a different kind of choice. Treating it as an ordinary candidate
would let the diversity rule dilute the deliberate reach, which is the whole
point of the exercise.

So the route's peer source becomes **two** sources with a clear precedence:
the named target when the local observation names one, and the store's
selection otherwise. The named target is not "one more candidate"; it is the
route's **subject**.

### 7.2 The version floor applies to the named target too

The temptation is to let the deliberately-reached peer bypass the gate,
because the router already knows it is reachable. That would be wrong: the
floor exists because a peer below it cannot complete the build, and a build
that cannot complete is a build that burns a tunnel slot and a coordinator
lease. The named target is gated exactly like a drawn candidate, and a refusal
is counted under the same reasons.

### 7.3 The observation is local, and the profile is not written

The observation is a fact about **this router's** session with a peer. It is
recorded in the router's own table, not in the peer's profile. The peer's
reachability is not claimed: the router knows it reached the peer, not that the
peer is reachable *by others*. Plan 533's invariant 5 is carried forward
unchanged.

### 7.4 The observation is consumed

A named target that has been used is **consumed**: the route takes it out of
the table. A peer the router reached once is built toward once; the router
does not keep re-targeting the same relayed peer out of a stale observation.
The observation also **expires**, so a session that died while the router was
not looking cannot be built toward.

### 7.5 The build goes through the existing coordinator

There is **one** build path in this tree and this plan does not add a second.
The named target's build is an ordinary exploratory build toward a different
peer, so Plan 504's liveness scheduler watches it and Plan 505's pairing and
observation machinery apply unchanged.

### 7.6 The material comes from the validated store

The named target's build material is resolved by the driver's **existing**
resolver (`crates/i2pr-daemon/src/exploratory_driver.rs:140`), which reads the
peer-profile store's validated records. If the reconcile has not yet carried
the peer's record into that store, the route refuses under
`MaterialAbsent` and retries on a later tick. The route never substitutes
material and never parses unvalidated bytes.

## 8. Work packages

| Package | Subject | Files |
| --- | --- | --- |
| WP1 | the local observation table, its refusals and its counters | `crates/i2pr-daemon/src/relay_session_peer.rs` (new) |
| WP2 | the establishment task records the observation | `crates/i2pr-daemon/src/relay_establishment_task.rs` |
| WP3 | the driver honours the named target, under the same version gate | `crates/i2pr-daemon/src/exploratory_driver.rs` |
| WP4 | the composition and the product's tick | `crates/i2pr-daemon/src/lib.rs`, `crates/i2pr-daemon/src/service_product.rs` |
| WP5 | the counters' publication | `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` |
| WP6 | the lane and the guard | `crates/i2pr-daemon/tests/relay_session_peer.rs`, `crates/i2pr-daemon/tests/relay_session_peer_boundary.rs` |
| WP7 | the documentation and the inventory | `specs/support.toml`, `specs/CONFORMANCE.md`, `specs/QUALIFICATION-GATES.md`, `specs/protocols/09-ssu2.md`, `docs/architecture/i2pr-daemon.md` |

## 9. Failure, cancellation, restart and contention semantics

**Failure.** Every refusal is typed and counted by reason and leaves no state
behind: no build, no session, no observation. A named target whose material
cannot be resolved is refused under `MaterialAbsent` and the observation is
**kept**, so the route retries when the reconcile has caught up. A named target
below the version floor is refused under its own reason and the observation is
**kept** for the same reason: the peer's published version can change, and the
observation is about the session, not about the version.

**Cancellation.** This plan adds **no task**. The observation table is a plain
value the product's existing tick advances.

**Restart.** Nothing is persisted. The observation table is live state and is
rebuilt empty; a restart names no relayed peer and builds nothing toward it.

**Contention.** The observation table is owned by the driver, which is advanced
on the product's tick; the product reads the peer-profile store under its own
lock, and the establishment task writes the observation under the driver owner's
lock. No new lock is introduced and no lock is held across an await.

## 10. Compatibility and migration

**No wire change.** The relay blocks, the build messages and their signatures
are exactly what the tree already sends and receives.

**No configuration migration.** **No key is added.** The relayed target rides
the existing `ssu2.relay_requester` opt-in and the existing `[exploratory]`
posture. A configuration that named neither behaves exactly as before.

**A new reading for the router.** A router that has asked for the requester
role and the exploratory posture may build toward the peer it reached through a
relay. It publishes nothing new and advertises nothing new.

**No change to the transport's model.** The reachability floors are unchanged
and no corroborating class is claimed.

## 11. Required tests

### 11.1 The lane

`crates/i2pr-daemon/tests/relay_session_peer.rs` — real loopback UDP, **three
real routers**, and **no** private-API driving from the test. The lane must
prove:

1. the relayed peer is observed once the establishment succeeds, and the
   observation is a **local** fact — the peer's profile carries no
   corroborating class and the corroboration count is unchanged;
2. the route builds toward the **relayed** peer, and the build installs;
3. the relayed peer's build goes through the **existing** coordinator, so the
   route's liveness pair appears;
4. a named target below the API-version floor is **refused** and counted, and
   **no** build is submitted;
5. a named target whose material the store does not hold is **refused** and
   counted, and **no** build is submitted;
6. the observation is **consumed** by the route that used it, and it **expires**;
7. the observation table is **bounded**, and a full table is refused under its
   own reason;
8. every refusal is typed and counted and writes nothing into a peer's
   profile.

### 11.2 The guard

`crates/i2pr-daemon/tests/relay_session_peer_boundary.rs` — a static guard that
is the authority for the **shape**. It must have an inverted probe for every
rule and a fail-closed probe for every item a rule names. The rules:

1. the route selects through the **version-gated** selection and never around
   it, and a named target is gated by the **same** function;
2. the driver owns no task, no timer, no listener and no socket;
3. the observation table is bounded and every full-table refusal is counted;
4. the material is read from the **validated** store and unvalidated bytes are
   never parsed;
5. the peer profile is not written for a local condition, and no reachability
   signal is written;
6. the build goes through the **existing** coordinator and no second build
   path exists;
7. the observation is consumed and it expires;
8. no configuration key is added;
9. the transport's relay state machines and the reachability model are
   untouched.

Plan 505's guard (`crates/i2pr-daemon/tests/exploratory_driver_boundary.rs`)
is **extended** with one rule — that a named target passes the same version
gate — and is otherwise left byte-identical. Its existing rule
`check_driver_selects_through_the_gate`
(`crates/i2pr-daemon/tests/exploratory_driver_boundary.rs:758`) still holds:
the driver's `select_peer` still selects through `select_tunnel_build_peers(`,
and that call still occurs exactly once in the module.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test relay_session_peer -- --test-threads=1
cargo test --locked -p i2pr-daemon --test relay_session_peer_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_driver_live --test exploratory_driver_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_relay_establishment --test ssu2_relay_establishment_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor in `AGENTS.md`, with this plan's two steps added.

## 13. Documentation updates

- `specs/support.toml` — a new experimental, non-advertised surface
  `daemon.relay-session-peer`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the gates this plan's lane and guard
  discharge.
- `specs/protocols/09-ssu2.md` — the relay scope, extended to the route.
- `docs/architecture/i2pr-daemon.md` — the route's shape and its owner.
- The plan's own amendment log.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when all of the following hold:

1. the relayed peer is observed as a **local** fact and the peer's profile
   carries no corroborating class;
2. the route builds toward the **relayed** peer and the build installs, through
   the **existing** coordinator;
3. the named target passes the **same** version floor, and a refusal is
   counted under the same reasons;
4. the observation is bounded, consumed and expiring, and every full-table
   refusal is counted;
5. every refusal is typed and counted and writes nothing into a peer's record;
6. no configuration key is added, `router.version` stays `0.9.62`, and the
   inventory's advertised count stays **zero**;
7. the full floor is green, with the closure record's table complete;
8. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the named target cannot be made to pass the same version gate without
  changing the gate itself — the floor is not this plan's to move;
- the build toward the relayed peer cannot be made to go through the existing
  coordinator — a second build path would be a new plan, not a shortcut;
- the floor's `cargo deny check advisories bans sources` fails;
- a stock peer would be required to prove any of the above — the stock-peer
  half stays blocked on Plan 493 and is **not** attempted here.

## 15. Closure evidence required

- The implementation commit, the lane-and-guard commit, the documentation
  commit and the closure commit, each named in the closure record.
- The **complete** floor table, every row with its result.
- The lane's rows, each with its result.
- The guard's rows, each with its result.
- The negative controls: each mutation, what it was expected to break, what it
  did break, and the SHA-256 proof that the tree was restored byte-identically.
- The requirement-to-evidence matrix, one row per requirement.
- The invariant review, the failure review, the migration review and the
  security review.
- The limitations, the findings by severity, the roadmap disposition and the
  unblock audit.

## 16. Handoff notes

- The plan is **experimental** and default-off. Do not describe the relayed
  build route as released, supported or advertised.
- The delivery is confined to the controlled loopback profile. A public
  deployment builds nothing toward a relayed peer.
- The stock-peer half stays blocked on Plan 493's authorized two-host topology.
- The plan extends Plan 505's guard by **one** rule and leaves the rest of it
  byte-identical.
- Plan 533's closed closure record is **not** rewritten. The corrections this
  plan finds live in this plan's closure record and in the registry's row.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 534 is registered as the successor to Plan 533 and the named owner of the
two limitations Plan 533 recorded as out of scope with an owner: the route by
which a relayed peer is used for tunnel building, and the relayed peer's
presence in the peer-profile store.

The scope is the **route**, not the reachability: the peer is reachable after
Plan 533, and this plan makes that reachability **usable**. The relayed peer
becomes the route's named build target, subject to exactly the same
API-version floor every other build target is subject to, with its material
read from the validated store and its build submitted through the existing
coordinator. The observation is a **local** fact about this router's session;
no peer profile is written and no corroborating class is claimed.

No closed closure record is rewritten by this plan. The corrections this plan
finds, if any, live in this plan's closure record and in the registry's row.

### 17.2 — 2026-11-11, implementation

The implementation is complete and the lane and the guard are green. Six
findings came out of it, and each is recorded here rather than by rewriting
the sections above, which are the plan as it was registered.

1. **The observation is consumed by the take, and a refused named target
   consumes it too.** The route takes the observation *before* it applies the
   gate, so a named target that is refused under the version floor or under
   the local condition is **not** kept for a later attempt. The plan's
   —7.4 states that taking is consuming; the implementation makes the
   consequence explicit: one deliberate reach yields **one** build attempt,
   and a refused attempt does not leave a stale observation behind. This is
   the honest behaviour — the peer was reached once, so the router reaches
   it again through the relay if it wants to try again — and it keeps the
   table free of observations that name a peer the router has already tried.

2. **A refused named target falls back to the store's own selection.** The
   exploratory posture is never blocked by one ineligible peer: the refusal
   is counted under its own reason and the route draws a candidate as usual.
   The plan's —7.1 names the named target as the route's subject, not as a
   replacement for the selection, and this is that distinction in the
   implementation.

3. **The relayed peer's presence in the peer-profile store is already
   satisfied.** Plan 533 recorded this limitation with an owner. The store is
   fed by the existing reconcile (`crates/i2pr-daemon/src/lib.rs:2191` and
   `crates/i2pr-daemon/src/peer_reconcile.rs:363`), so a relayed peer's
   validated record reaches the store the driver reads without any new work.
   The limitation is discharged, and the lane proves it: the lane's store is
   **adopted**, so the selection, the resolver and the record the driver reads
   are the same record.

4. **The relayed build's installation is proven.** Plan 533 recorded that the
   establishment's session existed while the *build* toward the relayed peer
   was unproven. Plan 534's lane proves it end to end: the route names the
   relayed peer, the coordinator submits the build, the real inbound path
   classifies the reply, the coordinator installs the tunnel, and the
   outbound pool holds it.

5. **The lane's profile assertion moved after the drive.** The first draft
   asserted that the relayed peer's profile was untouched *before* the driver
   ran, which proved only that the recording seam wrote nothing. The
   negative control NC-3 — a route that writes a peer observation for a
   local condition — passed the lane as written. The assertion now follows
   the drive, where such a write would happen, and NC-3 fails the lane as
   well as the guard. The lane is stronger than it was, and the finding is
   recorded here because it is a defect in the plan's own test.

6. **Two guard rules were strengthened by the negative controls.** NC-1
   (a below-floor verdict mapped to eligible) showed that rule 1 checked only
   that the gate was *called*, not that its verdict was carried faithfully;
   rule 1 now pins the mapping. NC-7 (a task that never records the
   observation) showed that rule 7 checked only the consumption half of the
   observation's lifecycle; rule 7 now pins the recording half as well. Both
   strengthen the guard rather than weaken it, and both are in this plan's own
   guard file, so no other plan's guard is affected.

The negative controls are eight in total. Each mutation, what it was expected
to break, what it did break, and the SHA-256 proof that the tree was restored
byte-identically are recorded in this plan's closure record.
