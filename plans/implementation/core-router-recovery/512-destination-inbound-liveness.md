# Plan 512 — the destination inbound pool's liveness coverage

| Field | Value |
| --- | --- |
| Plan | 512 |
| Subsystem | core-router-recovery |
| Status | registered |
| Discharges | Plan 487 **F3** (the destination inbound pool has no transit-hop failure detection) |
| Predecessors | Plan 487 (the §6 lifecycle proof and its F3), Plan 504 (the tunnel liveness scheduler in the product composition), Plan 502 (the transit owner in the product composition), Plan 505 (the exploratory build driver) |
| Reference pins | i2pd `2.61.0` = `635b013a612ff47278ef02acf8580a28e10e26c5`; Java I2P `2.13.0` = `9134f808337b401e8e53c73734c81fab04280c9d` |
| Advertises | nothing. `router.version` stays `0.9.62`, `caps` and the published RouterInfo are unchanged, `Cargo.toml` and `Cargo.lock` stay byte-identical |

## 1. Objective

Give the router's **destination** inbound tunnel pool the failure detection it
does not have today, so that a destination tunnel whose transit hop has died
is **failed and replaced** — and the replacement is **published** — without
waiting for the lease to expire.

Plan 487 recorded the gap verbatim and left it open because it needed its own
plan-of-record. This is that plan-of-record.

The scope is deliberately the *reaction*, not the *diagnosis*: the tree cannot
say **which** hop died, and this plan does not pretend otherwise. What it
delivers is the reference's own mechanism, at the reference's own granularity:
a periodic probe that leaves through one tunnel and must come back through
the other, and a tunnel that stops answering is dropped from the pool,
replaced, and re-advertised.

## 2. Why ready

1. **The finding is recorded and unfixed.** `plans/closure/core-router-recovery/487-status.md:378-400`
   states it: *"its destination inbound pool has no transit-hop failure
   detection — a dead transit hop is invisible to the router until the tunnel
   expires"*, and *"a router whose destination tunnels die with a transit hop
   does not replenish them until the lease expires."*
2. **The mechanism already exists and is already live — for the exploratory
   role only.** Plan 504's `TunnelLivenessScheduler`
   (`crates/i2pr-daemon/src/tunnel_liveness.rs:274`) is installed in the
   product composition, driven on the existing tick, and wired to the existing
   inbound seam. Its *population* is the only thing that excludes the
   destination role.
3. **Every consequence of a failure already exists.** Failing a destination
   tunnel changes the pool's usable inbound set; the daemon's own tick
   notices the changed lease set and marks the destination for republication
   (`crates/i2pr-daemon/src/service_product.rs:4132-4144`); the replenishment
   deficit grows and the existing submit path builds the replacement
   (`crates/i2pr-daemon/src/service_product.rs:4396`, `:6126`).
4. **The reference does exactly this, at the pin.** i2pd `2.61.0` runs
   `TunnelPool::TestTunnels`, and on a repeated miss it fails the tunnel,
   replaces it, and republishes the lease set.

## 3. Current implementation evidence

### 3.1 The gap, in the tree's own words

`plans/closure/core-router-recovery/487-status.md:378-400`:

> The stream still did not resume: the stock peer only resends a stream when
> the **remote lease changes**, and the router did not republish, because its
> destination inbound pool has no transit-hop failure detection — a dead
> transit hop is invisible to the router until the tunnel expires.

and the finding's own conclusion:

> *a router whose destination tunnels die with a transit hop does not
> replenish them until the lease expires.*

That is a **product gap**, not a peer defect. Nothing in this plan attributes
the fault to a peer.

### 3.2 What the liveness scheduler already does, and for whom

`crates/i2pr-daemon/src/tunnel_liveness.rs:274` — `TunnelLivenessScheduler`.
Its contract is pair-level: a probe leaves through the **outbound** tunnel and
must come back through the **inbound** tunnel; the answer is correlated on the
**acknowledged** message id (`record_authenticated_response`, `:560`); and a
pair that misses **two consecutive** responses is reported as
`LivenessAction::MarkUnhealthy` (`:414`).

Two properties of the scheduler matter to this plan and are **inherited, not
relaxed**:

- **The scheduler is role-agnostic.** `register_pair` (`:342`) checks only the
  pair ceiling, the slot inequality and the duplicate key. There is no
  exploratory precondition in the scheduler at all.
- **The proven-pair gate.** `record_authenticated_response` sets
  `state.proven = true` (`:581`) only on an *authenticated* response, and only
  a **proven** pair may be torn down. An unproven pair is never torn down.

The **population** is what restricts the scheduler to the exploratory role:

- `crates/i2pr-daemon/src/exploratory_build.rs:1317` —
  `exploratory_liveness_pairs()` pairs the *exploratory pool's* outbound
  registrations with the inbound registration whose gateway is the **same
  peer**.
- `crates/i2pr-daemon/src/service_product.rs:3408` — `sync_liveness_pairs`
  registers exactly that population and unregisters everything else.
- `crates/i2pr-daemon/src/service_product.rs:3363` — `drive_tunnel_liveness`
  drives the scheduler on the existing tick.
- `crates/i2pr-daemon/src/service_product.rs:3474-3476` states the exclusion
  in the tree's own words: *"Only exploratory slots are removed: the pair
  came from `ExploratoryBuildCoordinator::exploratory_liveness_pairs`, which
  never reports a destination-role slot."*

The destination role's slots live in a **different** place — the shared data
plane registry, not the exploratory pool — and are discriminated
structurally, not textually:

- `crates/i2pr-daemon/src/exploratory_build.rs:908` —
  `DESTINATION_ROLE_SLOT_BASE = 0x8000_0000`.
- `crates/i2pr-daemon/src/exploratory_build.rs:917` —
  `is_destination_role_slot(slot)`.
- `crates/i2pr-daemon/src/exploratory_build.rs:1153` —
  `activate_destination_tunnel` allocates the destination-role slot and binds
  it in the registry.

### 3.3 What the destination role already has

| Capability | Where | State |
| --- | --- | --- |
| The destination pool | `crates/i2pr-client/src/pool.rs:101` `DestinationTunnelPool` | live |
| Failing a tunnel | `crates/i2pr-client/src/pool.rs:376` `mark_failed`, reached through `crates/i2pr-client/src/registry.rs:602` `mark_tunnel_failed` | live (used at `crates/i2pr-daemon/src/service_product.rs:4625`, `:4632`, `:4692`, `:4708`) |
| The replenishment deficit | `crates/i2pr-client/src/registry.rs:645` `inbound_replenishment_deficit`, called at `crates/i2pr-daemon/src/service_product.rs:4396` | live |
| The replacement build | `crates/i2pr-daemon/src/service_product.rs:6126` `submit_destination_replacement` | live |
| The lease set's regeneration on a changed tunnel set | `crates/i2pr-client/src/leaseset.rs:87-101` (`LeaseSetRotationCause::TunnelSetChanged`, whose own doc says *"an ordinary tunnel set change (a tunnel failed or was withdrawn)"*), evaluated at `crates/i2pr-client/src/leaseset.rs:370-397` | live |
| The republication of the changed lease set | `crates/i2pr-daemon/src/service_product.rs:4132-4144` — the tick notices `lease_changed` and inserts the destination into `publication_pending` | live |
| The inbound already routed through the liveness seam | `crates/i2pr-daemon/src/service_product.rs:4539`, `:6792`, `:7851`, `:8081` call `route_inbound_with_liveness` | live |

**So the consequences of a failure are already implemented.** The only thing
missing is the *detection* and the *routing of the failure*.

### 3.4 The one thing that is missing, in three parts

1. **A destination-role population.** No function reports the destination
   role's `(outbound, inbound)` pairs.
2. **A destination-role probe composition.** The exploratory composition
   (`crates/i2pr-daemon/src/exploratory_build.rs:1418`
   `compose_liveness_probe`) reads the **pool's** routing metadata
   (`:1436`) and enforces that the pair **shares one peer** (`:1450`). A
   destination-role inbound is not in the pool, and the two directions are
   built in **separate rounds** from **separate** peer selections
   (`crates/i2pr-daemon/src/service_product.rs:6139-6143`), so neither
   property holds for the destination role.
3. **A failure action routed to the destination.** `MarkUnhealthy` is
   handled at `crates/i2pr-daemon/src/service_product.rs:3396-3404` and
   always calls `remove_liveness_pair`, which removes the *exploratory* pair
   only.

### 3.5 What the reference does — derived from the pinned source

**i2pd 2.61.0**, at the exact pin (`/tmp/i2pd-ref`). Note that the file names
a successor might guess do **not** exist at this version: the equivalents are
`Tunnel.cpp`, `TunnelPool.cpp`, `TransitTunnel.cpp` and `Destination.cpp`.

| Step | Where | What it does |
| --- | --- | --- |
| The manage tick | `libi2pd/TunnelPool.cpp:462-475` `ManageTunnels`; `libi2pd/TunnelPool.h:32` `TUNNEL_POOL_MANAGE_INTERVAL = 10` (seconds) | runs `CreateTunnels` and `TestTunnels` |
| The pairing | `libi2pd/TunnelPool.cpp:400-430` | shuffles the established **outbound** list and the established **inbound** list and pairs them **one to one** — **no shared-hop requirement** |
| The probe | `libi2pd/TunnelPool.cpp:428-458` | a garlic-wrapped tunnel-test message, sent **out** through the **outbound** tunnel and **addressed to the inbound tunnel's gateway** (`:452`: `outbound->SendTunnelDataMsgTo(it.second->GetNextIdentHash(), it.second->GetNextTunnelID(), msg)`) |
| The return path | the same line | the gateway forwards the message into the inbound tunnel, and it comes back to the local destination. **No remote cooperation is required** — the gateway simply forwards |
| The miss | `libi2pd/TunnelPool.cpp:342-345` | a pending test that has not answered when the next tick swaps the table is a miss |
| The first miss | `libi2pd/TunnelPool.cpp:356`, `:369` | `eTunnelStateTestFailed` — **not** fatal |
| The second miss | `libi2pd/TunnelPool.cpp:357-366`, `:374-386` | `eTunnelStateFailed`, and the tunnel is **erased from the pool** — unless it is the **last** one, in which case a replacement is created immediately (`:360`, `:377`: *"don't fail last tunnel"*) |
| The republication | `libi2pd/TunnelPool.cpp:389-393`, `:394` | `m_LocalDestination->SetLeaseSetUpdated(true)` — the **lease set is republished** when the **inbound** fails |
| The local drop | `libi2pd/TunnelPool.cpp:439-451` | a probe dropped locally blames the **outbound** tunnel |
| The reply | `libi2pd/TunnelPool.cpp:493-537` `ProcessTunnelTest` | matches the reply and restores `eTunnelStateEstablished` from `eTunnelStateTestFailed` |
| The states | `libi2pd/Tunnel.h:57-66` | `TunnelState` includes `eTunnelStateTestFailed` (`:63`) and `eTunnelStateFailed` (`:64`) |

**Java I2P.** The pinned `2.13.0` artifact is **not** on this host; the local
install is `2.13.1` (`i2p-router 2.13.1-1~ubuntu4`), and the shipped jar
carries **no `.java` sources**, so the Java evidence is **class and method
from `javap`**, not `file:line`. That caveat is recorded wherever the Java
reading is relied upon, exactly as Plan 511 recorded it. The shape is the
same: `net.i2p.router.tunnel.pool.TestJob` is the analogue of
`TestTunnels`; `TunnelPool.tunnelFailed(TunnelInfo)` calls
`fail(TunnelInfo)`; and for an **inbound, non-exploratory** pool, `fail` calls
`locked_buildNewLeaseSet()` and then `requestLeaseSet(LeaseSet)` — the direct
analogue of i2pd's `SetLeaseSetUpdated(true)`.

**The reading the plan takes from both.** The failure is a **pair-level**
fact: the probe did not come back. Neither reference attributes it to a
specific hop, and neither reference asks the peer for anything. i2pd fails
**both** sides of the pair, subject to the "don't fail the last tunnel" rule.

## 4. Invariants that must not regress

1. **The exploratory role's behaviour is unchanged.** `exploratory_liveness_pairs()`
   keeps its exact shape and its exact same-peer rule; the exploratory
   composition keeps its exact same-peer precondition; the exploratory failure
   action keeps calling `remove_liveness_pair`.
2. **Plan 504's guard and lane stay byte-identical and green.**
   `crates/i2pr-daemon/tests/tunnel_liveness_boundary.rs` and
   `crates/i2pr-daemon/tests/tunnel_liveness_live.rs` are **not** edited by
   this plan — including the lane's
   `a_destination_tunnel_never_appears_in_the_tracked_exploratory_pairs`
   (`crates/i2pr-daemon/tests/tunnel_liveness_live.rs:672`), which stays true
   because the destination population is a **separate** function.
3. **The proven-pair gate is inherited.** A destination pair that has never
   answered is never torn down.
4. **No peer is blamed.** A failed pair writes nothing into any peer profile.
   The router's own counters carry the fact.
5. **The last tunnel of a direction is never failed** (i2pd's rule,
   `libi2pd/TunnelPool.cpp:360`, `:377`). Failing the last usable tunnel
   would leave the destination with no path at all.
6. **No unbounded state.** The population, the pending-test table, the pair
   table and the new binding table are all bounded by named ceilings.
7. **No new configuration key**, no new timer, no new task, no new listener,
   no new socket. The plan rides the existing tick.
8. **Nothing is advertised.** `router.version` stays `0.9.62`; `caps` and the
   published RouterInfo are unchanged; the new support entry is
   `experimental` with `advertised = false`; the inventory's advertised count
   stays zero; `Cargo.toml` and `Cargo.lock` stay byte-identical.

## 5. Scope

**In scope.**

- A destination-role liveness **population** reported from the shared registry.
- A destination-role **probe composition** that addresses the inbound
  tunnel's gateway the way i2pd does.
- A **failure action** that fails the destination's tunnel(s) through the
  existing `mark_tunnel_failed` seam, subject to the last-tunnel rule.
- The **binding** between a coordinator destination-role slot and the
  destination's pool slot, bounded and owned.
- A lane that proves the mechanism end to end over the tree's own surfaces.
- A static guard that pins the mechanism's shape.
- The specification, conformance, support-inventory and deep-dive records.

**Out of scope, and why.**

- **Per-hop attribution.** The tree has no per-hop delivery-success signal;
  the tunnel layer's IV and key state are end-to-end. Both references treat
  the fault as pair-level. A per-hop mechanism is a new protocol mechanism
  and needs its own plan-of-record.
- **Failing the destination's outbound tunnel.** i2pd fails both sides of the
  pair. This plan fails **the inbound only**, because the recorded gap is
  about the **inbound** pool and its lease, and because an inbound failure
  already produces a replacement and a republication while an outbound
  failure does not. The divergence is recorded in §7, D5.
- **A stock-peer qualification.** The mechanism is over the tree's own
  deterministic surfaces; no external host, no stock peer, no patch and no
  vendoring.
- **Any change to the transit owner's behaviour.** This plan observes
  nothing new about a transit peer; it reacts to its own tunnels.
- **Plan 487 F2 and F6**, the routing table's live reader (Plan 497
  limitation 5's read half), and the per-component width divergence. All are
  recorded for their own successors.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/exploratory_build.rs` | **new** `destination_liveness_pairs()` — reports the destination-role `(outbound, inbound)` pairs from the shared registry, filtered by `is_destination_role_slot`. **new** `compose_destination_liveness_probe()` — the destination-role composition, reading the registry's `InboundGatewayRoute` and addressing the inbound's gateway. `exploratory_liveness_pairs()` and `compose_liveness_probe()` are **not** modified. |
| `crates/i2pr-daemon/src/tunnel_liveness.rs` | **new** `LivenessRole` classification for a tracked pair, so the pump can route the failure by role. The scheduler's `register_pair`, `drive`, `record_authenticated_response` and the proven-pair gate are **not** modified. |
| `crates/i2pr-daemon/src/service_product.rs` | `sync_liveness_pairs` registers the **second** population beside the first. The `MarkUnhealthy` handler branches on the role: the exploratory arm is unchanged; the **new** destination arm fails the destination's inbound slot through `runtime.mark_tunnel_failed`, honours the last-tunnel rule, and marks the destination for republication. **new** bounded binding table from a coordinator destination-role slot to `(destination_id, pool_slot, direction)`, written where the binding is already created and removed where the role is removed. |
| `crates/i2pr-daemon/src/lib.rs` | the binding table's construction and teardown in the product composition. |

No other production file changes. No dependency changes.

## 7. Design decisions the plan must make explicit

### D1 — The destination population is a **separate** function.

`exploratory_liveness_pairs()` stays exactly as it is, and a new
`destination_liveness_pairs()` reports the destination role's pairs. The
alternative — widening the existing function and filtering by role at the
call site — would change Plan 504's function's contract and force an edit to
Plan 504's guard. The separate function keeps Plan 504's guard and lane
**byte-identical** (invariant 2) and makes the two populations independently
readable.

### D2 — The destination probe uses i2pd's addressing; the exploratory same-peer precondition is preserved.

i2pd addresses the probe to the inbound tunnel's **gateway** and lets the
outbound tunnel's exit peer forward it (`libi2pd/TunnelPool.cpp:452`). It does
**not** require the pair to share a hop. The tree's exploratory composition
**does** require it (`crates/i2pr-daemon/src/exploratory_build.rs:1450`),
which is correct for that population — both exploratory tunnels are built to
the same peer — but is not available for the destination role, whose two
directions are built in separate rounds from separate selections
(`crates/i2pr-daemon/src/service_product.rs:6139-6143`).

**Decision.** The destination composition addresses the inbound's gateway and
does **not** impose the same-peer precondition; the exploratory composition
keeps it, byte for byte.

**The cost, stated plainly.** A destination probe can fail because the
inbound's gateway was unreachable from the outbound's exit peer. The plan
does not claim per-hop attribution and does not attribute the failure to any
peer. The proven-pair gate (invariant 3) bounds the blast radius: a pair that
never proves is never torn down, so a systematically broken destination probe
path proves nothing and therefore fails nothing.

### D3 — The failure is a **pair-level** fact and is never written into a peer profile.

The plan mirrors both references: the probe's absence says *the path did not
answer*, not *this peer is dead*. The router's own counters carry the fact.
No peer profile is written, no peer is blamed, and the tree's existing rule —
a local condition must never be written into a peer's profile — is untouched.

### D4 — The proven-pair gate is the safety property, and it is inherited rather than re-derived.

Plan 504 already refuses to tear down a pair that has never answered
(`crates/i2pr-daemon/src/tunnel_liveness.rs:581`). The destination arm does
not get its own, looser, rule: the **same** gate governs both populations.
The plan's guard must pin that the destination arm cannot fail an unproven
pair.

### D5 — Only the inbound is failed; the divergence from i2pd is recorded.

i2pd fails both sides of the pair (`libi2pd/TunnelPool.cpp:357-366`,
`:374-386`). This plan fails **the inbound** only. The reasons are recorded
rather than argued away: the recorded gap is about the inbound pool and its
advertised lease; the inbound's failure already produces both a replacement
and a republication, while the outbound's failure does not; and the outbound
is the router's path to the remote service, whose failure is a **different**
gap. The cost is that a dead *outbound* produces repeated inbound rebuilds
rather than one outbound rebuild; the plan records that cost and bounds it
with the pair ceiling and the proven-pair gate.

### D6 — The last tunnel of a direction is never failed.

i2pd's rule (`libi2pd/TunnelPool.cpp:360`, `:377` — *"don't fail last
tunnel"*) is adopted verbatim: a destination with exactly **one** usable
inbound is not failed, because failing it would leave the destination with no
path and no advertisement. The refusal is **counted**, so the situation is
observable rather than silent.

### D7 — The binding between the coordinator's slot and the destination's pool slot is **owned and bounded**.

The coordinator allocates the destination-role slot
(`crates/i2pr-daemon/src/exploratory_build.rs:1159-1166`) and the destination
runtime allocates the pool slot; both are already returned together by
`register_destination_material_classified` as `ActivatedDestinationBinding`
(`crates/i2pr-daemon/src/service_product.rs:1368`, `:6117`). The plan keeps
that pairing in a **bounded table** in the product, written where the binding
is created and removed where the role is removed. The table is keyed by the
coordinator's role slot, because that is the key the scheduler tracks, and it
carries the destination id, the pool slot and the direction. Its ceiling is a
named constant, and the plan's guard pins it.

### D8 — No new configuration key, no new timer, no new task.

The scheduler's existing configuration governs both populations: the first
test delay, the repeat interval, the response timeout and the consecutive
failure threshold. The population is not configurable — it is derived from
the destination roles that are actually installed — so a router with no
destination tunnels tracks nothing, exactly as it does today.

### D9 — The republication on a failed tunnel is **used**, not re-implemented.

`LeaseSetRotationCause::TunnelSetChanged` already exists for exactly this
case (`crates/i2pr-client/src/leaseset.rs:87-101`), and the daemon's tick
already notices the changed lease set and marks the destination for
republication (`crates/i2pr-daemon/src/service_product.rs:4132-4144`). The
plan adds no publication path, no publication key and no second lease-set
builder. The guard must pin that the destination arm reaches the failure
through `mark_tunnel_failed`, so that the existing republication is the one
that fires.

## 8. Work packages

**WP1 — the population.** `ExploratoryBuildCoordinator::destination_liveness_pairs()`
reports the destination-role `(outbound, inbound)` pairs, built from the
shared registry and filtered by `is_destination_role_slot`. The pairing rule
is bounded and deterministic: the destination role's outbound roles are read
from the registry, the inbound roles are read from the registry, and the two
are paired in a stable order. The function is pure, allocation-bounded, and
returns nothing when no destination role is installed.

**WP2 — the probe composition.** `compose_destination_liveness_probe()`
composes the probe through the **registry's** installed outbound role, and
addresses it to the **registry's** `InboundGatewayRoute` for the paired
inbound (`crates/i2pr-tunnel/src/data_plane_registry.rs:74-90`). The payload
and the framing are the ones Plan 504 already uses
(`delivery_status_message`, the scheduler's response timeout, the same
`compose_outbound_build_deliveries` seam). **No private parallel pipeline is
introduced.**

**WP3 — the failure arm.** The `MarkUnhealthy` handler branches on the pair's
role. The exploratory arm is unchanged. The destination arm resolves the
binding, applies the last-tunnel rule, calls `runtime.mark_tunnel_failed`,
removes the coordinator's registry role, and lets the existing replenishment
and republication run. Every refusal and every failure is counted by reason.

**WP4 — the lane.** `crates/i2pr-daemon/tests/destination_liveness_live.rs`
proves, over the tree's own surfaces:

1. a destination-role pair is reported by the new population, and the
   exploratory population is unchanged;
2. the scheduler tracks the destination pair and emits a probe for it;
3. the probe is addressed to the **inbound's gateway** (i2pd's addressing),
   and the composition succeeds where the exploratory same-peer rule would
   have refused;
4. an authenticated response proves the pair;
5. a proven pair that misses two consecutive responses fails the
   destination's inbound slot, the pool's usable set changes, and the lease
   set's decision becomes `Regenerate(TunnelSetChanged)`;
6. an **unproven** pair that misses is **not** failed;
7. the **last** usable inbound is **not** failed, and the refusal is counted;
8. no peer profile is written.

**WP5 — the guard.** `crates/i2pr-daemon/tests/destination_liveness_boundary.rs`
pins: the population exists and is role-filtered; the exploratory population
and the exploratory composition are untouched; the destination composition
reads the registry and addresses the inbound's gateway; the failure arm
reaches `mark_tunnel_failed`; the last-tunnel rule holds; the proven-pair
gate holds; the binding table is bounded; the counters exist and carry the
refusal reasons; the lane is present, bounded and sleep-free. Every rule
carries an inverted probe.

**WP6 — the records.** `specs/QUALIFICATION-GATES.md` (a new section for the
references' failure-detection mechanism), `specs/CONFORMANCE.md`,
`specs/support.toml` (one new `experimental`, `advertised = false` surface),
`docs/architecture/i2pr-daemon.md`, `docs/architecture/i2pr-tunnel.md`,
`docs/architecture/i2pr-client.md`.

**WP7 — the controls, the floor, the closure.** At least eight negative
controls applied to the real tree and reverted byte-for-byte; the routine
floor; the closure record with the requirement-to-evidence matrix, the
reviews, the limitations, the findings by severity, the roadmap disposition
and the unblock audit.

## 9. Failure, cancellation, restart, and contention semantics

- **A probe that cannot be emitted is abandoned, not counted.** The existing
  `abandon_test` (`crates/i2pr-daemon/src/tunnel_liveness.rs:404`) semantics
  apply unchanged: the peer never saw the probe, so the peer is not at fault
  and the test must not count against the pair's threshold.
- **A pair that goes away between the decision and the emission** is
  unregistered without counting a failure, exactly as the exploratory arm
  does today.
- **A destination that is stopping** fails no tunnel. The failure arm checks
  the runtime's state through the existing manager seam; a refused failure is
  counted by reason.
- **The binding table is drained** when a destination is retired, so a stale
  binding can never fail a tunnel that has already gone.
- **A restart** resets the scheduler and the binding table, exactly as it
  resets the exploratory population today. The router re-learns the
  destination roles from the live composition.
- **Contention.** The failure arm takes the destination runtime's lock
  through the existing `with_destination_runtime` seam and never holds it
  across an await.

## 10. Compatibility and migration

- No configuration key changes meaning; no key is added.
- No wire format changes. The probe is Plan 504's existing `DeliveryStatus`
  payload with the existing correlation id.
- No persisted state changes: the binding table is in-memory and
  rebuilt on start.
- The published RouterInfo, `caps` and `router.version` are untouched.
- A router built before this plan and one built after it interoperate: the
  probe is an ordinary `DeliveryStatus`, and a peer that does not echo it
  simply never proves the pair, which is a no-op.

## 11. Required tests

| Test | Where | Asserts |
| --- | --- | --- |
| the destination population reports the destination role and not the exploratory role | lane | the population is role-filtered |
| the exploratory population is unchanged | lane | Plan 504's contract still holds |
| the probe is addressed to the inbound's gateway | lane | i2pd's addressing |
| the composition refuses a pair with no installed outbound role | lane | fail-closed |
| an authenticated response proves the pair | lane | the proven-pair gate |
| two consecutive misses fail the destination's inbound | lane | the gap is closed |
| an unproven pair is not failed | lane | the safety gate |
| the last usable inbound is not failed, and the refusal is counted | lane | i2pd's rule |
| the lease set's decision becomes `Regenerate(TunnelSetChanged)` | lane | the republication path is the existing one |
| no peer profile is written | lane | no peer is blamed |
| the guard's eleven-or-more rules, each with an inverted probe | guard | the shape is pinned |

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test destination_liveness_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_liveness_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test tunnel_liveness_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test tunnel_liveness_boundary -- --test-threads=1
cargo test --locked -p i2pr-client --all-targets -- --test-threads=1
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

The full routine floor from `AGENTS.md` is run before handoff, and its result
is recorded in the closure record.

## 13. Documentation updates

- `specs/QUALIFICATION-GATES.md` — a new section recording what the pinned
  references do when a tunnel stops answering, with the i2pd `file:line`
  citations and the Java caveat.
- `specs/CONFORMANCE.md` — the Plan 512 section, stating exactly what is and
  is not qualified.
- `specs/support.toml` — one new surface, `experimental`,
  `advertised = false`, with the inventory's advertised count unchanged.
- `docs/architecture/i2pr-daemon.md` — the destination role's place in the
  liveness scheduler, the failure arm and the binding table.
- `docs/architecture/i2pr-tunnel.md` — the registry's role in the destination
  composition.
- `docs/architecture/i2pr-client.md` — the destination pool's failure and
  replenishment path.
- `plans/subsystems/core-router-recovery-roadmap.md` — Plan 512's row and
  §12.
- `plans/registry.md` — Plan 512's row, and Plan 487's row carrying F3's
  discharge. **Plan 487's closure record is not rewritten.**

## 14. Acceptance criteria and stop conditions

**Accepted when** all of the following hold:

1. The floor is green, and the plan's own lane and guard are in it.
2. A destination-role pair is tracked, probed, and — once **proven** and then
   missed **twice** — the destination's inbound tunnel is failed, the pool's
   usable set changes, the lease set's decision becomes
   `Regenerate(TunnelSetChanged)`, and the existing republication path runs.
3. An unproven pair is never failed; the last usable inbound is never failed;
   both refusals are counted.
4. The exploratory role's behaviour, guard and lane are **byte-identical** and
   green.
5. The controls are applied to the real tree, reverted byte-for-byte, and
   each breaks an assertion.
6. Nothing is advertised; `Cargo.toml` and `Cargo.lock` are byte-identical.

**Stop conditions** — the plan stops and records, rather than continuing:

- if the destination composition cannot be built on the registry's route
  without a second, parallel delivery path;
- if the proven-pair gate cannot be shown to govern the destination arm;
- if the last-tunnel rule cannot be enforced without a race;
- if the binding table cannot be bounded;
- if any control leaves the tree changed, or the floor fails for a reason this
  plan does not own.

## 15. Closure evidence required

- The floor's full table, with the plan's two new steps.
- The requirement-to-evidence matrix.
- The commands run, with their exact results.
- The invariant, failure, migration and security reviews.
- The limitations, including the Java version caveat, the pair-level
  attribution limit, and the outbound-side divergence from i2pd.
- The findings by severity, with the controls' per-control failure counts.
- The roadmap disposition and the unblock audit.
- The **exact** future evidence that would close Plan 487 F3
  unconditionally, if the source-side work does not close it outright.

## 16. Handoff notes

- The plan's subject is a **reaction** gap. It does not diagnose a dead hop,
  and it does not claim it does.
- The reference reading is the primary source of the design, at the exact
  i2pd pin. The Java reading is one patch release off its pin and is labelled
  as such everywhere it is relied upon.
- Plan 504's guard and lane are **not** edited. If a later change finds that
  they must be, that is a signal the design was taken in the wrong direction,
  and it must be recorded as a deviation rather than absorbed.
- The next free plan number after this one is **513**.
