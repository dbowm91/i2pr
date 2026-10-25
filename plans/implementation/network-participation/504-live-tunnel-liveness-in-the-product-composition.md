# Plan 504 — The tunnel liveness scheduler in the product composition

| Field | Value |
| --- | --- |
| Plan | 504 |
| Subsystem | `network-participation` |
| State | `registered` (`registered-live-tunnel-liveness-in-the-product-composition`) |
| Depends on | Plan 185 (`passed-m6-live-one-hop-exploratory-tunnels-and-liveness`), Plan 487 (`passed` — whose F3 is the gap this plan advances), Plan 502 (`passed-transit-owner-in-the-product-composition`), Plan 503 (`passed-live-dsm-version-gate-and-peer-eligibility-receipt`) |
| Discharges | **Plan 185's unwired liveness seam** — the mechanism Plan 185 delivered, which the production daemon has never constructed. Advances **Plan 487 F3** on the exploratory path |
| Closure record | `plans/closure/network-participation/504-status.md` |

## 1. Objective

Make the router **notice that a tunnel pair it depends on is dead**, and
recover, instead of silently carrying the failure until the tunnel expires.

Plan 185 delivered a complete, bounded, unit-tested tunnel-liveness scheduler
for exactly this. It is `crates/i2pr-daemon/src/tunnel_liveness.rs`, and it
has **never been constructed by the production daemon**. This plan installs
it.

## 2. Why ready

- The mechanism exists, is bounded, and is tested: `TunnelLivenessScheduler`
  with a bounded pair table (`MAX_LIVENESS_PAIRS = 8`), a bounded pending
  table (`MAX_PENDING_LIVENESS_TESTS = 16`), a single central owner with no
  per-tunnel task and no per-tunnel timer, and a typed `LivenessAction` the
  pump acts on.
- The routing seam exists too: `route_inbound_with_liveness` is the one
  helper that threads the inbound dispatcher into both the build coordinator
  and the scheduler.
- The live pump already owns everything the scheduler needs: the coordinator
  (`ServiceProduct::inner.coordinator`), the outbound gateway role, and an
  existing tick.
- **This is a composition change, not new mechanism.** The line's established
  way to close a "complete but unwired" gap is Plan 502's: install the owner
  in the product composition.

## 3. Current implementation evidence

### 3.1 The scheduler has zero production users

An exhaustive audit of every `pub mod` in `crates/i2pr-daemon/src/lib.rs` —
for each module, the number of in-crate production references to
`crate::<module>::` outside the module's own file, against the number of
out-of-crate test references — produced exactly one result:

```text
TEST-ONLY IN DAEMON: tunnel_liveness (prod=0 tests=11)
```

`tunnel_liveness` is the **only** public module in the daemon crate with no
production consumer and with test consumers. It is not an accident of
naming.

### 3.2 The live inbound path bypasses the seam

The scheduler's routing helper exists and is public:

```rust
// crates/i2pr-daemon/src/tunnel_liveness.rs:551
pub fn route_inbound_with_liveness(
    coordinator: &mut ExploratoryBuildCoordinator,
    scheduler: &mut TunnelLivenessScheduler,
    inbound: &Ssu2InboundI2np,
    now_ms: u64,
) -> Result<InboundLivenessOutcome, crate::router_i2np::RouterI2npError>
```

The live daemon calls the coordinator's dispatcher **directly** in four
places, and never the liveness variant:

- `crates/i2pr-daemon/src/service_product.rs:3957` —
  `route_inbound_i2np_classified(inbound, outcome)`
- `crates/i2pr-daemon/src/service_product.rs:6177` —
  `route_inbound_i2np(&inbound, wall_ms())`
- `crates/i2pr-daemon/src/service_product.rs:7227` —
  `route_inbound_i2np(&inbound_msg, wall_ms())`
- `crates/i2pr-daemon/src/service_product.rs:7450` —
  `route_inbound_i2np(&inbound_msg, wall_ms())`

Every inbound `DeliveryStatus` echo the router receives is therefore
**discarded**: nothing correlates it with a pending test, because there is
no pending test.

### 3.3 A closure record states the opposite

`plans/closure/mixed-router-interop/185-status.md:71-73` says, verbatim:

> `route_inbound_with_liveness` is the single helper the **daemon pump
> calls** to thread the Plan 184 central dispatcher into both the build
> coordinator and the liveness scheduler.

**That is false of this tree.** The helper exists; the daemon pump does not
call it. Plan 185's closure record is **not** rewritten — the correction
lives in this plan's closure record and in the registry's row, the precedent
Plan 503 set for Plan 491.

### 3.4 The consequence is a core-router defect

Plan 487 F3 recorded the product gap in one line:

> *a router whose destination tunnels die with a transit hop does not
> replenish them until the lease expires*
> — `plans/closure/core-router-recovery/487-status.md:398-400`

The exploratory path has the same shape. The router builds a paired set of
exploratory tunnels and uses them. When the far end dies, the router finds
out only when a delivery attempt fails or the tunnel expires. It does not
**notice**, and it does not **rebuild** — it keeps a dead pair in the pool
and keeps routing through it.

The scheduler's own doc says what the router is missing
(`crates/i2pr-daemon/src/tunnel_liveness.rs:1-16`):

> Current I2P tunnel-routing guidance requires creators to test tunnels
> because participants may remove tunnels after roughly two minutes of no
> traffic. The scheduler pairs each active outbound exploratory tunnel with
> an active inbound exploratory tunnel, sends a DeliveryStatus test through
> the outbound path, and matches the response that arrives through the
> inbound path.

### 3.6 The response half has never been exercised over a wire

This is the fact that shapes the whole plan, and it must be stated before
the implementation, not discovered during it.

**Every lane that uses the scheduler stops at the request half.**

- `crates/i2pr-daemon/tests/exploratory_build_live.rs:753-782` registers a
  pair, drives the scheduler to a `SendTest`, and then calls
  `scheduler.record_response(test_id)` **directly**. The response is
  *simulated*; no message crosses a wire.
- `crates/i2pr-daemon/tests/exploratory_tunnel_external.rs:358-369`, against
  the exact-pinned i2pd, asserts only that the scheduler produces a
  `SendTest` and that `counters().tests_sent == 1`. It does **not** assert
  that any peer answers.
- The same shape holds in `netdb_tunnel_external.rs`,
  `destination_tunnel_external.rs`, `netdb_tunnel_live.rs`,
  `destination_tunnel_live.rs`, `java_tunnel_external.rs` and the
  service-tunnel qualification lanes: they drive the scheduler, and none of
  them observes a correlated response.

So the **request** half is proven; the **response** half is proven **nowhere**
— not against a stock peer, and not against another `i2pr` router.

### 3.7 The correlation is currently unauthenticated, and the plan says so

`TunnelLivenessScheduler::record_inbound_outcome`
(`crates/i2pr-daemon/src/tunnel_liveness.rs:432-456`) matches an inbound
`DeliveryStatus` **without** an authenticated message id. Its own doc comment
says:

> Without an authenticated message id the scheduler cannot match the inbound
> echo to a pending test id; the daemon pump must call
> [`Self::record_response`] directly when it has a confirmed test id. The
> helper returns the next outstanding test id when exactly one pending test
> exists, otherwise `None`.

That is a **weak** correlation: *any* inbound `DeliveryStatus` refreshes the
pending test whenever exactly one test is pending, and nothing correlates
when more than one is pending.

The reason is visible in the dispatcher
(`crates/i2pr-daemon/src/router_i2np.rs:700-708`): the `DeliveryStatus`
classification keeps the **transport** message id and **discards** the
`DeliveryStatusMessage::message_id` — the id of the message being
acknowledged. The id that would identify the test is thrown away.

**This is why the plan cannot simply wire the scheduler in.** With an
unauthenticated correlation and a response half that has never been observed,
a live router could tear down a healthy pair it cannot prove is unhealthy,
and would do so **repeatedly**. That is a regression, not a capability.

The plan therefore does three things it did not originally say it would do:

1. it makes the correlation **authenticated** — the acknowledged-message id
   is exposed by the dispatcher and matched against the pending test;
2. it gates the **teardown** on positive evidence that the pair has
   answered at least once, so a router that has never seen a response can
   never tear anything down (invariant 9);
3. it makes the **feasibility question executable** (WP1) rather than
   assumed, so the plan can stop honestly if the response half cannot be
   produced.

### 3.8 The mechanism is bounded by construction

| Bound | Value | Where |
| --- | --- | --- |
| First test delay after a pair becomes active | 30 s | `FIRST_LIVENESS_DELAY_MS` |
| Repeat test interval while the pair stays active | 60 s | `REPEAT_LIVENESS_INTERVAL_MS` |
| Response timeout | 60 s | `LIVENESS_RESPONSE_TIMEOUT_MS` — below the two-minute idle-deletion boundary |
| Consecutive-failure threshold | 2 | `LIVENESS_FAILURE_THRESHOLD` |
| Active pairs | 8 | `MAX_LIVENESS_PAIRS` |
| Pending tests | 16 | `MAX_PENDING_LIVENESS_TESTS` |

The error direction is the safe one: a **false** "dead" verdict tears down a
good tunnel pair, so the threshold is two consecutive failures and the
response timeout is a minute. A **missed** verdict costs the router the
liveness window — the status it has today, which is no worse.

## 4. Invariants that must not regress

1. **The declared version is not edited** and no floor is moved. This plan
   touches no version policy.
2. **Fail closed and bounded.** The scheduler's pair table, pending table and
   failure counters keep their explicit ceilings; a capacity refusal is
   typed and counted, never a silent drop.
3. **No new task, no new timer, no new listener, no new socket.** The
   scheduler runs on the **existing** pump tick and the **existing**
   coordinator. It opens no socket, spawns no task, and creates no timer.
4. **No unbounded structure.** The scheduler's tables are the bounded ones
   above. Nothing in this plan adds a growing collection.
5. **No new dependency.** `Cargo.toml` and `Cargo.lock` stay byte-identical.
6. **No advertisement, no wire change, no new option, no new capability.**
   `router.version` stays `0.9.62`; `caps` and the published RouterInfo are
   unchanged; the support inventory's advertised count stays **zero**.
7. **The liveness seam is the only inbound route.** The plan does not add a
   second, parallel inbound path; it routes the existing one through the
   existing helper.
8. **A false liveness verdict may not take down a good destination.** The
   scheduler's verdict removes an **exploratory** pair; a destination
   tunnel is not touched by this plan (see §5.2).
9. **A verdict may only be acted on when the signal is distinguishable from
   its opposite.** A missing response means "dead" **only** if the router has
   already seen that peer answer. Until a pair has produced at least one
   correlated response, a missing response is counted and **never** acted on.
   This is the same rule the rest of this repo applies to a refusal: refuse
   to act on a signal that cannot be distinguished from its negation.
10. **The correlation must be authenticated.** The scheduler may only
   refresh a pair from a response that carries **that pair's** test id in
   the acknowledged-message field. A response that cannot be attributed to a
   pending test is not evidence, and is not counted as one.

## 5. Scope

### 5.1 In scope

- **The feasibility question is answered by running code** (WP1), and the
  plan stops honestly if the answer is no.
- The live daemon owns a `TunnelLivenessScheduler`.
- The coordinator's installed **exploratory** tunnel pairs are registered with
  it, and unregistered when the pair goes away.
- The scheduler is driven on the existing tick; a `SendTest` action emits the
  `DeliveryStatus` test through the paired outbound role, and the response is
  correlated through the inbound path.
- **The correlation is authenticated**: the acknowledged-message id is exposed
  by the dispatcher and matched against the pending test, so a response is
  attributed to exactly one test.
- **A teardown requires positive evidence**: a pair that has never answered
  is never torn down.
- A `MarkUnhealthy` action removes the pair from the coordinator and asks the
  coordinator to rebuild — the recovery the router cannot do today.
- The live inbound route goes through `route_inbound_with_liveness`.
- The static guard gains rules that the live composition constructs the
  scheduler and that the live inbound path routes through the liveness seam,
  each with an inverted probe; an out-of-crate lane proves the live
  behaviour.
- The documentation, including the correction of Plan 185's false statement.

### 5.2 Explicitly out of scope

- **Plan 487 F3's destination-inbound half.** The scheduler is
  **creator-side**: it pairs the router's own outbound and inbound
  *exploratory* tunnels. The **destination's inbound pool** — the tunnels the
  router advertises in its LeaseSet2 — is a different pool, and detecting a
  dead transit hop *there* needs a signal the tree does not have: the only
  in-protocol signal for a tunnel you are the endpoint of is **tunnel
  rekeying**, and the tree implements **no rekeying at all** (no `rekey`
  symbol anywhere in `i2pr-proto`, `i2pr-tunnel` or `i2pr-client`, and
  `TunnelDataMessage` has no rekey payload). That is a new I2NP mechanism and
  needs its own plan-of-record. **Recorded as an open finding**, not silently
  taken.
- **The lookup-target API-version gate** (Plan 503's finding). It is API
  hygiene, not functionality, and gating lookups can only *reduce* whom the
  router asks.
- **Plan 497 limitation 5** — the peer-profile store stays empty in a live
  daemon, and the routing table's only reader is the never-constructed
  `NetDbTunnelCoordinator` (Plan 433's blocked composition). Wiring the
  profile store alone would wire a store into a path that is still not
  consulted — the Plan 491/503 trap one level down. The read side and the
  write side must land together, and the read side sits behind Plan 433.
- **Plan 487 F2** — the SAM session-lifetime policy is Plan 485's recorded
  decision; changing it needs its own plan-of-record.
- **Plan 502 M1** — the creator-correlation retention window is a
  protocol-derived window, not a mechanical cap.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/service_product.rs` | the product owns the scheduler; the tick drives it; the inbound route goes through the liveness seam; a `MarkUnhealthy` action removes the pair and asks for a rebuild |
| `crates/i2pr-daemon/src/exploratory_build.rs` | the coordinator reports the installed exploratory pairs the scheduler must track, and can drop a pair on request |
| `crates/i2pr-daemon/src/tunnel_liveness.rs` | the module is wired in; its public API is unchanged |
| `crates/i2pr-daemon/src/lib.rs` | the composition's public surface records the liveness posture, like the transit posture |
| `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` | the liveness counters are published for observation |

## 7. Work packages

### WP1 — the feasibility question, as an executable artefact

**This is the first work package, and it can stop the plan.** It answers, in
running code rather than in prose:

1. Can a probe emitted through the **real** `OutboundGatewayRole` produce a
   response that the **real** inbound dispatcher classifies as a
   `DeliveryStatus`?
2. Can that response be attributed to **one specific pending test** — the
   acknowledged-message id the dispatcher currently discards?
3. What exactly does the response look like on the wire, and which path does
   it arrive on?

The artefact is a local, deterministic lane that drives the real
composition: emit the probe through the real outbound role, deliver the
resulting cell back through the real inbound dispatch path exactly as the
peer's answer would arrive, and assert the scheduler correlates it **by the
acknowledged id**.

**If the answer to (1) or (2) is no**, the plan **stops** and records the
precise reason, the gap stays open, and the closure record says what would
be needed. A plan that stops honestly with a measured reason is worth more
than a plan that installs a mechanism whose response half has never been
observed.

### WP2 — the live owner

`ServiceProduct` owns a `TunnelLivenessScheduler` beside the coordinator it
watches. It is created with `LivenessConfig::plan_185_defaults()` — the
same bounded defaults Plan 185 fixed, so the live policy and the tested
policy are one. The scheduler's counters are published through the existing
`InspectionHandles` seam, so the running router's liveness posture is
observable.

### WP3 — registering the pairs

When the coordinator installs an **exploratory** outbound tunnel and its
paired inbound tunnel, the pair is registered with the scheduler
(`register_pair(outbound_slot, inbound_slot)`). When the pair goes away — a
slot is removed, or the tunnel expires — it is unregistered
(`unregister_pair(outbound_slot)`), which drops the pair's pending tests
**without counting them as failures**.

The coordinator must be able to say which slots are exploratory and which are
destination tunnels, because only the exploratory pairs are tracked. The
plan's implementation must make that distinction explicit rather than
inferring it from a slot number.

### WP4 — driving the scheduler

The scheduler is driven on the **existing** product tick — the same tick
that already advances the coordinator's clock and expires its pending
builds. `advance_time(now_ms)` then `drive()` yields one typed action:

- `LivenessAction::SendTest { outbound_slot, inbound_slot, test_id }` — the
  pump emits a `DeliveryStatus` test through the pair's outbound role, using
  the canonical `i2pr_tunnel::roles::OutboundGatewayRole`, never a private
  parallel pipeline. The test id is remembered so the response can be
  correlated.
- `LivenessAction::MarkUnhealthy { outbound_slot, inbound_slot, .. }` — the
  pump removes the pair from the coordinator and asks the coordinator to
  rebuild. **This is the recovery the router cannot do today.**
- `LivenessAction::Idle { .. }` — nothing to do.

The scheduler's `drive()` returns **one** action per tick. If several pairs
are due, the pump must keep driving until the scheduler returns `Idle`,
with a **bounded** number of drives per tick so a large backlog cannot turn
one tick into an unbounded burst. The bound is a named constant.

### WP5 — correlating the response, authentically

The live inbound route goes through `route_inbound_with_liveness`, which
correlates an inbound `DeliveryStatus` with a pending test and refreshes the
pair. The four direct `route_inbound_i2np` call sites become the liveness
variant. The helper is **existing**: the plan wires it, it does not rewrite
it.

**But the correlation must be made authenticated first.** Today the
dispatcher discards the acknowledged-message id
(`crates/i2pr-daemon/src/router_i2np.rs:700-708`), and the scheduler's
`record_inbound_outcome` therefore matches "any `DeliveryStatus`, when
exactly one test is pending". That is not a correlation.

The plan's change:

- the `DeliveryStatus` classification **exposes the acknowledged-message
  id** — the `DeliveryStatusMessage::message_id` the body already carries,
  which is the id the remote echoes for the message we tunneled. It is
  added to the outcome in a way that does not weaken any decoder and does
  not change the classification of any other body;
- the probe's tunneled message carries a **test-derived id** in that field,
  so the remote's acknowledgement echoes it back;
- the scheduler matches on **that id**, so a response is attributed to
  exactly one pending test, and a response that carries no pending id is
  **not** evidence and is not counted as one.

The scheduler's existing `record_inbound_outcome` behaviour — the
"exactly one pending test" fallback — **stays** for the lanes that use it,
but the live path uses the authenticated match. The plan must not weaken
the existing helper's contract to make the live path work.

The helper's own doc says it "never weakens any decoder; it only correlates
inbound observations with pending tests". That property is the reason the
plan prefers it over a new correlation path, and it is why the authenticated
id is *added* to the outcome rather than smuggled in through a side channel.

### WP6 — the removal and the rebuild

On `MarkUnhealthy`, the pump removes the pair's slots from the coordinator
(`remove_slot`) and asks the coordinator to rebuild. The rebuild request is
the coordinator's existing submit path; the plan must not add a second
build mechanism.

**The safety direction matters.** A false verdict tears down a *good*
pair, so:

- the threshold is two consecutive failures, not one;
- the response timeout is 60 s, below the two-minute idle-deletion boundary;
- the verdict is **counted** (`LivenessCounters`), so an operator can see a
  router that is tearing down healthy pairs — and the counter is published;
- the removed slots are exploratory only. **A destination tunnel is never
  removed by this plan**;
- **a pair that has never produced a correlated response is never torn
  down.** Until the router has seen that pair answer, a missing response is
  indistinguishable from "this peer does not answer", and the router must
  not act on a signal it cannot distinguish from its negation. The
  "unproven pair" state is itself **counted** and published, so an operator
  can see that the liveness signal is not working rather than inferring it
  from silence.

### WP7 — the controls

**The static guard** gains rules, each with an inverted probe:

1. the live composition constructs the scheduler;
2. the live inbound path routes through `route_inbound_with_liveness`, not
   through the bare coordinator dispatcher;
3. the live tick drives the scheduler.

**The out-of-crate lane** proves, over the public daemon API:

- an installed exploratory pair is registered with the scheduler;
- the scheduler emits a `DeliveryStatus` test after the first-test delay;
- a matching response refreshes the pair;
- a missing response twice reaches the failure threshold and the pair is
  removed and a rebuild is requested;
- the scheduler's counters report the failures;
- the scheduler's pair and pending tables stay within their ceilings.

### WP8 — the documentation

`docs/architecture/i2pr-daemon.md` gains the liveness section;
`specs/CONFORMANCE.md` gains a Plan 504 section; `specs/support.toml` gains
one `experimental` surface with `advertised = false`; the registry's Plan
185 row and the network-participation roadmap carry the correction and the
disposition.

**Plan 185's closure record is not rewritten.** Its false sentence about the
daemon pump is corrected in this plan's closure record and in the registry's
row — the precedent Plan 503 set for Plan 491.

## 8. Failure, cancellation, restart, and contention semantics

- **Failure.** A capacity refusal (`PairCapacityReached`,
  `PendingCapacityReached`) is typed and counted; the pump skips that pair
  and continues. A dead pair that cannot be tracked is not worse than today,
  where no pair is tracked at all.
- **Cancellation.** The scheduler holds no task and no timer, so there is
  nothing to cancel inside it. A cancelled pump leaves the scheduler's
  tables as they are; the next tick continues from the coordinator's clock.
- **Restart.** Nothing is persisted. The scheduler starts empty and the
  coordinator's tunnels are rebuilt by the normal path.
- **Contention.** The scheduler lives beside the coordinator inside the
  product's existing state; it adds no lock of its own beyond the ones the
  coordinator already takes. The pump's tick and the inbound route take the
  same coordinator borrow the existing code takes, in the same order, so no
  new lock order is introduced.

## 9. Compatibility and migration

- **No wire change.** The `DeliveryStatus` test is an existing I2NP message
  the router already encodes.
- **No `router.version` change, no `caps` change, no RouterInfo change.**
- **No migration, nothing persisted, no new configuration key.**
- **Behavioural consequence, stated plainly.** A router whose exploratory
  pair is healthy now emits one `DeliveryStatus` test per pair per minute.
  That is new outbound traffic, bounded by the pair ceiling (8) and the
  repeat interval (60 s). A router that tears down a good pair because of a
  false verdict loses that pair and rebuilds it — which is why the threshold
  is two consecutive failures and the verdict is counted.

## 10. Required tests

- The scheduler's own unit tests stay unchanged and keep passing.
- A new in-crate unit test proves the coordinator distinguishes an
  exploratory pair from a destination tunnel.
- A new out-of-crate lane proves the behaviours listed in WP6.
- The static guard's new rules and their inverted probes.
- The existing exploratory lanes (`exploratory_build_live.rs`,
  `exploratory_build_unit.rs`, `destination_tunnel_live.rs`,
  `destination_tunnel_unit.rs`) keep passing **unchanged**.

## 11. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_floor_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test tunnel_liveness_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- --test-threads=1
bash scripts/check-runtime-boundaries.sh
bash scripts/check-dependency-direction.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the full routine floor from `AGENTS.md` before the closure.

## 12. Documentation updates

- `docs/architecture/i2pr-daemon.md` — the live liveness owner, the
  registration, the tick, the correlation, the removal and rebuild, and the
  counters.
- `specs/CONFORMANCE.md` — the Plan 504 section: what is now detected, what
  is not, the bounds, and the counted verdicts.
- `specs/support.toml` — one `experimental` surface with `advertised = false`.
- `plans/registry.md` — the Plan 504 row, and Plan 185's row corrected.
- `plans/subsystems/network-participation-roadmap.md` — the Plan 504 row and
  the §12 status sentence.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

1. **The feasibility artefact answers (1), (2) and (3) in running code**, and
   its answer is recorded in the closure record whichever way it goes. If it
   is "no", the plan stops with the measured reason and the gap stays open.
2. The live daemon constructs a `TunnelLivenessScheduler` and publishes its
   counters.
2. An installed exploratory pair is registered; a removed pair is
   unregistered, and its pending tests are dropped without counting a
   failure.
3. The scheduler is driven on the **existing** tick, and a due pair produces
   a `DeliveryStatus` test through the canonical outbound role.
4. A matching response refreshes the pair; two consecutive missing responses
   reach the threshold and the pair is removed and a rebuild is requested.
5. The live inbound path routes through `route_inbound_with_liveness`, so an
   inbound echo is correlated rather than discarded.
6. The pair and pending tables never exceed their ceilings, and the drives
   per tick are bounded.
7. The static guard proves each new rule fires, with an inverted probe per
   rule.
8. The out-of-crate lane proves the live behaviour over the public daemon
   API.
9. **No new task, no new timer, no new listener, no new socket, no new
   dependency, no advertisement, no wire change.**
10. The routine floor is green on the closing tree.

### 13.2 Stop conditions

- If registering a pair would require the coordinator to expose a
  destination tunnel as an exploratory one, stop: the two must stay
  distinguishable.
- If the liveness seam cannot be used without changing the decoder's
  behaviour, stop: the helper's "never weakens any decoder" property is the
  reason to prefer it.
- If the rebuild request would need a new build mechanism, stop: the plan
  uses the coordinator's existing submit path.
- If the false-verdict direction cannot be bounded — that is, if one missed
  response could tear down a pair — stop: the threshold is two consecutive
  failures and the verdict is counted.

### 13.3 What this plan does **not** close

- **No stock-peer evidence of the liveness response.** The response half has
  never been observed over a wire, with any peer (see §3.6). The plan's
  feasibility artefact is a **local** one. Whether an ordinary router
  answers the probe is **unproven**, and is recorded as such — the same
  treatment Plan 491 F2 and Plan 493 give stock-peer evidence. The teardown
  gate (invariant 9) exists precisely because that question is open.

- **Plan 487 F3's destination-inbound half.** Detecting a dead transit hop
  in the destination's **inbound** pool needs **tunnel rekeying**, which the
  tree does not implement at all — there is no `rekey` symbol in
  `i2pr-proto`, `i2pr-tunnel` or `i2pr-client`, and `TunnelDataMessage` has
  no rekey payload. That is a new I2NP mechanism and needs its own
  plan-of-record.
- **Plan 487 F2** — the SAM session-lifetime policy is Plan 485's recorded
  decision.
- **Plan 497 limitation 5** — the peer-profile store stays empty in a live
  daemon, and the routing table's only live reader would be Plan 433's
  blocked coordinator. The write side and the read side must land together.
- **Plan 502 M1** — the creator-correlation retention window.
- **No stock-peer evidence of any kind**, and therefore **no advertisement**
  of any kind.

## 14. Closure evidence required

- A requirement-to-evidence matrix naming, for every acceptance criterion,
  the executable command and the file and line that satisfy it.
- The exact floor step table with pass/fail per step.
- The false-pass audit: for every "passed" claim, the way it could have
  passed without proving anything.
- The invariant, failure, migration and security reviews.
- The findings by severity, with the medium and low findings **recorded**
  rather than absorbed.
- The roadmap disposition and the unblock audit.
- The commit list, and the floor-caught fixes committed separately.

## 15. Handoff notes

- The plan number is **504**; it is never renumbered.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`) and Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`). Nothing is patched or
  vendored.
- `specs/support.toml` gains exactly one `experimental` surface with
  `advertised = false`; the inventory's advertised count stays **zero**.
- `Cargo.toml` and `Cargo.lock` stay byte-identical: this plan adds **no
  dependency**.
- No boundary script is weakened. If a new rule is needed, it comes with a
  positive control.
- The next free plan number after this one is **505**.
