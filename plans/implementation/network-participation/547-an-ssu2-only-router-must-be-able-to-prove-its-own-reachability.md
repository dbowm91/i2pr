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

### 17.2 — 2026-11-13, implementation, the guard retarget, and the measurement

**The owner moved; the recorder did not.** `PeerTestRuntime`
(`crates/i2pr-daemon/src/peer_test_runtime.rs`) is the new owner, and it is
built in the SSU2 service closure from `ssu2_config.peer_test` **alone**. It
rides the daemon's **existing** group tick and it is fed by the daemon's own
inbound path, which is its **single** consumer: the seam reads the classified
dispatcher outcome, so the message is decoded once. `ServiceProduct` keeps no
peer test at all — `peer_test_owner`, `peer_test_rng`, `peer_test_next_at_ms`,
`peer_test_counters_cell`, `build_peer_test_owner`, `build_peer_test_rng`,
`handle_inbound_peer_test`, `deliver_peer_test_answer`, `drive_peer_test`,
`select_peer_test_target`, `publish_peer_test_counters` and
`peer_test_counters_cell` are all gone, and `ServiceProductOptions::peer_test`
is gone with them.

**The re-dating path now supplies the endpoint.** `refresh_local_router_info_record`
carries the separately bound NTCP2 endpoint and the scope it is admitted in,
and chooses the install form by the record's **own** content: a record that
names an NTCP2 address goes through `install_scoped_dual_router_info`, whose
gate still compares that address with the supplied endpoint; a record that
names none keeps `install_local_router_info`, whose gate requires exactly
that. The endpoint is never inferred from the record — inferring it would make
the test compare the record with itself.

**Plan 521's guard was retargeted and strengthened.** Its rules that named the
product's peer-test code now name the new module, and the two that named the
product's *inbound path* now name the daemon's composition root. The
retarget also closed a latent defect: several of those rules used `?` on an
`Option`, so an **absent** definition returned "no problem found" — the
guard's own header claimed "a function that is **absent** fails closed", and
that claim was false, so a rule could be satisfied by deleting the code it
policed. Every rule this plan touched now reports an absent definition, and a
new rule pins that the destination-group product owns no peer test at all.
Plan 521's closure record is **not** rewritten.

**Two of this plan's own negative controls were wrong and were corrected.** The
first run of NC-547-3 and NC-547-4 fired, but they fired on a **compile error**
— a struct field the constructor never set, and an undefined helper — and a
control that fires because the tree does not build proves nothing. Both were
rewritten to compile, and the guard's install rule was strengthened at the same
time to pin the **exact** scoped-install call, so the endpoint the path passes
is the supplied one. The corrected run fires all eleven controls for the
right reason.

**The measurement.** On the three-host private LAN — `192.168.182.20` (the
floodfill), `192.168.182.143` and `192.168.182.8` — with
`[ssu2] publish_bound_address = true` and `[ssu2] peer_test = true`:

```text
04:47:28  normal floodfill opted in; evaluating eligibility requested=true role="Disabled"
04:48:28  floodfill qualification activated requested=true role="Active"
```

Plan 546's measurement on the same host recorded `role="Disabled"` and never
left it. The role activated because the runtime produced **publication
material**, and `publication_material` requires `Reachable`, and `Reachable`
requires **three** distinct corroboration classes of which the confirmed peer
test is the only production source of the third: `LocalConfiguredBind` comes
from the bind, `ValidatedPath` comes from an authenticated inbound session,
and `AuthenticatedPeerObservedExternalAddress` still has no production caller
at all. The activation is therefore itself the evidence that the peer test
completed over a real SSU2 session — and the local node's log shows the
session it completed on (`ssu2 peer pool dial outcome outcome=Established`).

**What the measurement does not say.** The floodfill role activating on a
private LAN is not the floodfill **serving** correctly against stock peers —
Plan 493 owns that question — and the role's activation is not a claim that
the published address is reachable from the public Internet.

### 17.3 — 2026-11-13, the reporting build, the corrected attribution, and the blocked measurement

**Three commits followed 17.2, and the first of them was the floor's own
finding.** The 76-step floor failed at step 45, not in a test but in
`scripts/check-m11-transit-boundaries.sh`: rule 10 forbids the **word**
`handle_inbound` anywhere in production `lib.rs`, because that is the transit
owner's entry point, and 17.2's daemon-side seam was named `handle_inbound`.
`f430b453` renames the seam **`accept_inbound`** and leaves the checker
byte-identical — the rule's needle cannot distinguish two types' methods of the
same name, and tightening it would have required rewriting the rule's own
negative controls, whose sample texts call `owner.handle_inbound(`. The
over-broad needle is recorded as a finding, not papered over.

**`4139fc7d` gave the peer test and the transport a voice.** The earlier build
owned no log at all, which is exactly the trap 545 recorded one layer down: a
probe that was refused, a probe that went out and was never answered, and a
peer test that never ran were one indistinguishable silence. The peer test now
reports `peer_test_no_peer_to_ask`, `peer_test_probe_started`,
`peer_test_probe_refused`, `peer_test_round_trip_completed` (with the measured
elapsed time) and `peer_test_answer_sent`; the transport reports the typed
`terminate_reason` on both handshake paths, the `drop_category` on the inbound
path, and the typed `dial_outcome` at the dial seam. Counts, elapsed time and
stable labels only. `d4647de1` rebuilt the arm64 artifact for it.

**The measurement, on that build, over 3 h 04 m.** Three hosts, all with
`[ssu2] peer_test = true`; `.20` additionally with `publish_bound_address` and
`[floodfill] enabled`:

| Host | probes started | matched round trips | answers sent to peers | RTT | publication cycles | published |
| --- | --- | --- | --- | --- | --- | --- |
| `192.168.182.20` (floodfill) | 39 | **39** | 38 | 3–43 ms | 3, all `RefusedNoTarget` | **0** |
| `192.168.182.143` | 40 | 38 | 1 | 2–142 ms | 3, all `RefusedNoTarget` | **0** |
| `192.168.182.8` | 38 | **38** | 76 | — | 3, all `RefusedNoTarget` | **0** |

The peer test works, on every host, repeatedly, and it is what moved the gate:

```text
14:54:06.148  peer_test_probe_started
14:54:06.188  peer_test_round_trip_completed elapsed_millis=40
14:54:36.188  floodfill qualification activated requested=true role="Active"
```

**A correction to 17.2.** 17.2 said `AuthenticatedPeerObservedExternalAddress`
"still has no production caller at all", and concluded that the confirmed peer
test is the only production source of the third corroboration class. **That is
wrong.** `note_observed_external_address` is reached from the inbound
handshake's `Address` block and from `SessionEvent::AddressObserved`
(`crates/i2pr-runtime/src/ssu2_runtime.rs`), so class 1 is producible, and a
fresh inbound handshake supplies classes 1 and 2 together. The activation above
is therefore **not** proof that the peer test was *necessary* for it — the log
shows the round trip 40 s earlier, which proves the peer test ran, and the
corroboration arithmetic alone cannot separate the two. 17.2's sentence is left
in place as the record of what was believed at implementation time; this
paragraph is the correction.

**T4 did not happen, and the reason is not the peer test.** Every publication
cycle on every host ended `RefusedNoTarget`, because
`publication_targets_excluding` → `select_version_gated_floodfills` admits only
records that advertise floodfill, and **all three hosts hold `caps="4"` for each
other** — including the record each hands out about itself. The out-of-band
export is written at startup, before the gate runs, and is never rewritten
(`.20`'s export is dated `14:53`; activation was `14:54:36`), the floodfill's
own publication is refused for the same reason, and the production router runs
no RouterInfo-kind lookup. That is a bootstrap deadlock in an isolated network,
and it is Plan 548's.

**The role does not sustain, and that is a second blocker.** `.20` held
`role="Active"` for 29 m 30 s, withdrew, re-activated for 1 m 30 s, withdrew,
and stayed `Disabled` for the remaining 2 h 45 m. `Reachable` needs three
distinct classes; only `LocalConfiguredBind` and `PeerTestResult{Confirmed}`
are refreshed by traffic a long-lived session produces, so classes 1 and 2 lapse
at `DEFAULT_OBSERVATION_TTL` (30 min) and the state falls to the corroboration
floor, which publication refuses. The 29 m 30 s window matches that TTL. The
class composition itself is **not observable** — no surface reports it — so the
attribution is inferred from source plus the matching period and is recorded as
inferred.

**Closure.** `plans/closure/network-participation/547-status.md` is the
authority: both gates this plan was registered for are passed with executed
evidence, and the inherited measurement clause is `blocked` at target
selection. Plan 548 carries it.
