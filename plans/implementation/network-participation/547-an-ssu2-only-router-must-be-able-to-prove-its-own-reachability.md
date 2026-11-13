# Plan 547 — an SSU2-only router must be able to prove its own reachability,
and a re-dated record must be installable

**Status:** registered. **Subsystem:** network-participation.
**Registered:** 2026-11-13. **Successor to:** Plan 546.

## 1. Objective

Two gates stand between Plan 546's relaxation and a router that can actually
advertise a reachable address. Neither is a predicate, and both are the same
kind of defect: a capability that belongs to the SSU2 service is owned by
something else, or is handed the wrong arguments.

1. **A router that runs only SSU2 cannot corroborate its own reachability.**
   The peer-test owner lives inside `ServiceProduct`, which the daemon builds
   only when `service_runtime_active` holds — so on the posture this tree
   supports best, no corroboration exists and `publication_material` refuses
   before any address predicate is consulted.
2. **A re-dated dual-transport record cannot be installed.**
   `refresh_local_router_info_record` installs through
   `install_local_router_info`, which passes no NTCP2 endpoint, and the install
   gate then requires the record to carry no NTCP2 address. A record that names
   both transports has therefore never been installable after a re-date.

This plan closes both, so that `[ssu2] publish_bound_address` (Plan 546) has
something to publish and the floodfill role can activate.

## 2. Why ready

**Both defects have named, located causes.** The first is an ownership
question with an exact call site: `ServiceProduct::handle_inbound_peer_test`
already calls `Ssu2RuntimeService::note_confirmed_peer_test`, which records
`ReachabilitySignal::PeerTestResult`; the recorder works, and the thing that
feeds it is simply built in the wrong place. The second is an argument list:
the scoped installer exists (`install_scoped_dual_router_info`), is already
used on the record-change path, and the re-dating path does not reach it.

**The measured evidence exists.** On the three-host LAN, `192.168.182.20` with
Plan 546's key set logs:

```text
normal floodfill opted in; evaluating eligibility requested=true role="Disabled"
the re-dated RouterInfo was not installed reason=InvalidPublication
router RouterInfo publication transition="router_info_publication" outcome=RefusedNoTarget
```

**And the precedent is in this repository.** Plan 542 found the same shape: the
NetDB's retention prune was unreachable because its only caller lived inside
`ServiceProduct` behind the same `service_tunnels_active` gate, and the fix was
a second caller that did not depend on the destination-group product. This plan
is the same move for reachability.

## 3. Current implementation evidence

### 3.1 The peer test exists and records reachability

```text
// crates/i2pr-daemon/src/service_product.rs
crate::peer_test::PeerTestInbound::Completed(round_trip) => {
    let service = self.inner.ssu2_handle.service();
    service.note_confirmed_peer_test(round_trip.link_id, round_trip.proof)
```

and `note_confirmed_peer_test` records `ReachabilitySignal::PeerTestResult`
(`crates/i2pr-runtime/src/ssu2_runtime.rs`). So the evidence path is real and
the runtime already derives the address family from the session the exchange
travelled on — the daemon never asserts one.

### 3.2 The owner is behind the destination-group gate

```text
// crates/i2pr-daemon/src/lib.rs
let has_enabled_groups = service_runtime_active;
let mut owner = if has_enabled_groups { ... }
```

`PeerTestOwner` is constructed only inside `ServiceProduct`
(`build_peer_test_owner`, called from two `ServiceProductSpec` sites), and
`ssu2.peer_test` is the switch that decides whether it exists at all. With only
SSU2 enabled there is no `ServiceProduct`, so no owner and no probe.

### 3.3 The re-dating path cannot install what it re-dates

```text
// crates/i2pr-daemon/src/router_i2np.rs
if let Err(error) = handle.service().install_local_router_info(fresh.clone(), now_ms)
```

`install_local_router_info` passes `controlled_ntcp2_endpoint: None`, and
`install_local_router_info_inner` then requires `ntcp2_addresses.is_empty()`.
The lab's record carries one SSU2 and one NTCP2 address, so the install is
refused and the publication then reports `RefusedNoTarget`.

### 3.4 What the plan does not claim

- It does not claim the floodfill **serves** correctly against stock peers. That
  is Plan 493's question.
- It does not claim a live-network session. Two `i2pr` routers on a private LAN
  are the evidence available; stock-router admission is external.
- It does not change any address predicate. Plan 546 already decided what may be
  published; this plan gives that decision a truthful input.

## 4. Invariants

1. **`ssu2.peer_test` stays off by default.** A default router runs no probe,
   records no reachability, and publishes nothing but loopback.
2. **The daemon never asserts reachability.** The only writer is the runtime's
   recorder, fed by a completed exchange. A `Firewalled`/contradicted outcome
   must be able to *veto* an operator's `publish_bound_address`, which is the
   property that makes the key safe to set on a host the operator believes is
   directly reachable.
3. **No new listener, socket or dependency.** The owner rides a loop that
   already exists.
4. **The install gate keeps its meaning.** Admitting a dual-transport record
   must not weaken the check that the NTCP2 address names the separately bound
   endpoint and falls inside the scope; the plumbing supplies the endpoint, it
   does not skip the test.
5. **The default posture is unchanged** and the predecessors' evidence stays
   byte-identical.

## 5. Scope

### 5.1 In scope

- `crates/i2pr-daemon/src/service_product.rs` and `crates/i2pr-daemon/src/lib.rs`
  — where the peer-test owner is constructed.
- `crates/i2pr-daemon/src/router_i2np.rs` — the re-dating install path.
- The daemon wiring that supplies the NTCP2 endpoint and scope.

### 5.2 Out of scope

- Any address predicate or publication policy (Plan 546 owns those).
- The floodfill's serving behaviour.
- NAT traversal, hole punching, introducers.
- Stock-router admission.

### 5.3 Out of scope, with the owner named

- **Whether the floodfill serves a stock peer correctly.** Plan 493.
- **Live-network admission evidence.** The external lanes and Plan 493.
- **`note_observed_external_address`'s missing production caller.** Still
  unowned; a successor either wires it from the handshake or retires it.

## 6. Required production changes

### 6.1 Carry the peer-test owner where an SSU2-only router can reach it

1. Construct the owner on the SSU2 service path under `ssu2.peer_test`, so it
   exists whenever the SSU2 service does.
2. Route the inbound peer-test handling to it there. If `ServiceProduct` also
   needs it (for its own inbound handling), the owner must be shared or moved —
   not duplicated, because two owners would answer the same probe twice.
3. Keep the counters readable through the existing inspection seam.

### 6.2 Let the re-dating path install a dual-transport record

`refresh_local_router_info_record` gains the NTCP2 endpoint and its scope, and
installs through `install_scoped_dual_router_info` when the record carries an
NTCP2 address — the same call the record-change branch already makes.

## 7. Design decisions

### 7.1 The owner moves; the recorder does not

The runtime stays the only writer of reachability. Moving the daemon-side owner
is a composition change; moving the recorder would let the daemon make a
claim, which is exactly what invariant 2 forbids.

### 7.2 One owner, not two

If both `ServiceProduct` and the SSU2 loop handle inbound peer-test messages,
a probe is answered twice and the counters double-count. The plan requires a
single owner with one consumer, and the guard pins that.

### 7.3 The endpoint is supplied, not inferred

The install gate's NTCP2 test is the authority on whether the record's NTCP2
address is the router's own. Passing the endpoint in keeps that test; inferring
it from the record would make the test compare the record with itself.

### 7.4 A contradicted probe must be able to stop publication

With the key set on a host that is *not* directly reachable, the honest outcome
is that the peer test contradicts the address and the runtime refuses
publication. The plan's lane proves that direction, because a corroboration
that can only say yes is an assertion with extra steps.

## 8. Work packages

1. **WP1 — the owner's construction and single consumer.**
2. **WP2 — the re-dating install path.**
3. **WP3 — the lane**: an SSU2-only composition reaches the peer-test owner; a
   completed exchange corroborates; a contradicted one refuses; a dual-transport
   record re-dates and installs.
4. **WP4 — the guard**: one owner, the default off, the install test intact.
5. **WP5 — the negative controls.**
6. **WP6 — the documentation and the inventory.**
7. **WP7 — the three-host measurement**, with `.20` as the floodfill.
8. **WP8 — the floor and the closure record.**

## 9. Failure, cancellation, restart and contention semantics

**Failure.** A refused install stays typed and categorical. A failed probe is
an outcome, not an error, and is counted by its own kind (Plan 528's rule).

**Cancellation.** The owner rides the existing loop's cancellation; moving it
adds no await and no task.

**Restart.** No state is persisted; reachability is re-derived.

**Contention.** One owner, one consumer. The plan explicitly forbids a second
consumer, which is the only contention this could introduce.

## 10. Compatibility and migration

**No wire change.** The record's encoding and signature are untouched.

**No new configuration key.** `ssu2.peer_test` and Plan 546's
`publish_bound_address` are the switches.

**No persisted state. No dependency change. No manifest change.**

## 11. Required tests

### 11.1 The lane

- An SSU2-only composition (no service tunnel, no control surface) has a
  peer-test owner when `ssu2.peer_test` is set, and none when it is not.
- A completed exchange records corroboration; a contradicted one records a
  refusal that keeps `publication_material` refusing.
- A dual-transport record re-dates and installs through the scoped form, and a
  record whose NTCP2 address names a different endpoint is still refused.

### 11.2 The guard

One owner; the default posture unchanged; the install gate's NTCP2 test intact;
no new listener, socket or dependency; nothing advertised beyond the operator's
own keys; the predecessors' evidence byte-identical.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test ssu2_only_reachability -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_only_reachability_boundary -- --test-threads=1
```

plus the routine floor, with this plan's steps added.

## 13. Documentation updates

`specs/support.toml` (surface `ssu2.ssu2-only-reachability`),
`specs/CONFORMANCE.md`, `specs/QUALIFICATION-GATES.md`,
`docs/architecture/i2pr-daemon.md`, and this plan's amendment log.

## 14. Acceptance criteria and stop conditions

Accepted when an SSU2-only router builds the peer-test owner from
`ssu2.peer_test`, a completed exchange makes its bound address publishable,
a contradicted exchange does not, a dual-transport record re-dates and
installs, and `.20` then serves as a floodfill on the LAN with a peer's
publication confirmed by a `DeliveryStatus`.

**Stops** if the fix would need a second owner, would let the daemon write
reachability itself, or would relax the install gate's NTCP2 test.

## 15. Closure evidence required

Commits; the floor table; lane and guard rows; negative controls with the
restoration proof; the requirement-to-evidence matrix; the invariant, failure,
migration and security reviews; limitations, findings, roadmap disposition and
the unblock audit.

## 16. Handoff notes

- **One owner.** Two consumers answer a probe twice and double the counters.
- **The daemon must not write reachability.** If a change needs it to, the
  change is wrong.
- A contradicted probe is a **feature**: it is what stops a NAT'd host with the
  operator's key set from publishing an address peers cannot reach.
- The predecessors' closure records are **not** rewritten.

## 17. Amendment log

### 17.1 — 2026-11-13, registration

Registered directly from Plan 546's measurement, which located both gates and
left them unfixed deliberately. Plan 546 stays registered; this plan is what
makes its relaxation meaningful.
