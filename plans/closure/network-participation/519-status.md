# Plan 519 — the publication's delivery leaves the transport's loop

**Closure record.** The plan of record is
`plans/implementation/network-participation/519-router-info-publication-off-the-transport-loop.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 519 is **closed**. The router's own RouterInfo publication runs on its
**own task**, with explicit ownership, explicit cancellation and its own
bounded delivery timeout, and the SSU2 service's `select!` loop never awaits
it.

Four facts, one gap:

1. **The publication was already periodic.** Plan 515 made the router's own
   RouterInfo a refreshed record on the reference's interval.
2. **The publication was already confirmable.** Plan 516 put the correlator's
   minted token on the wire and joined the inbound owner to the correlator.
3. **The publication was already retrying.** Plan 517 made an unacknowledged
   publication follow the next batch, at the next closest floodfills.
4. **The publication's record was already current.** Plan 518 re-dated and
   re-signed it once per cycle, so the network no longer refuses it.

All four ran the publication **inside** the SSU2 service's `select!` loop,
where the delivery awaited a session establishment
(`crates/i2pr-daemon/src/service_product.rs:7442` ->
`crates/i2pr-daemon/src/router_i2np.rs:2438` ->
`crates/i2pr-runtime/src/ssu2_runtime.rs:3480`). While that await was pending
the loop could not run its other arms: the relay drain and the inbound
acknowledgement owner both waited.

The transport's **socket** I/O was not the victim: `i2pr-runtime` owns the
sockets, the timers and the handshake machinery, so a handshake completed
while the daemon's service loop was stalled. What was lost was the **service
layer's** other work. This was the **Medium** finding `M1` that Plan 517's and
Plan 518's closure records both carry open.

**Neither pinned reference blocks its transport's loop on a handshake.** i2pd
2.61.0's `Transports::SendMessage` posts the send and the publisher **ignores**
the returned future (`libi2pd/Transports.cpp:483-496`), so the main thread is
never stalled on a handshake. Java I2P 2.13.0 runs the publication as a **job
on its own pool** (`PublishLocalRouterInfoJob`, `JobQueue.java:163`), hands
the store to a `FloodfillStoreJob` with its own `PUBLISH_TIMEOUT = 90 s`
(`FloodfillNetworkDatabaseFacade.java:174`, `:246-257`), and delivers it
through `commSystem().processMessage(m)`, so the transport owns the dial
(`StoreJob.java:415-441`). The I2P documentation states the same rule in prose:
a router publishes its RouterInfo by directly connecting to a floodfill
router, and only in some circumstances out an exploratory tunnel.

**Plan 519 closes the coupling.** Nothing is promoted: `router.version` stays
`0.9.62`, `caps` and the published RouterInfo's **content** stay unchanged,
the inventory's advertised count stays **zero**, and `Cargo.toml` and
`Cargo.lock` stay byte-identical.

## What the plan closed

- The publication runs on its **own task** (decision **D1**). The owner — the
  schedule, the cycle, the correlator and the cycle's record — the transport
  capability, the identity and the acknowledgement inbox all move into it.
- The task is **owned and cancellable** (decision **D8**): the composition
  root spawns it under the SSU2 service's own child scope, with an explicit
  cancellation token, and the service's shutdown cancels and joins it. There
  is no ownerless task and no detached one.
- The SSU2 service's loop holds **none** of the publication's state and never
  awaits it (decision **D2**). The publication's work is confined to the
  publication's task.
- The inbound `DeliveryStatus` crosses a **bounded** channel (decision
  **D3**). The loop's hand-over never waits for room; a full channel counts
  the acknowledgement as **unmatched** — the refusal Plan 516 already defines
  — rather than blocking the loop or dropping it silently.
- The publication owns a **bounded per-attempt delivery timeout** of 90
  seconds, in the shape of the reference's `PUBLISH_TIMEOUT` (decision
  **D4**). A delivery that did not finish inside it is refused under **its
  own reason** (`deliveries_timed_out`) and counted, and the ladder moves on.
- The selection **prefers a peer the router is already connected to** (decision
  **D5**), on top of the version-gated closeness selection, and the targets it
  skips for want of one are counted under their own reason
  (`skipped_no_session`).
- The publication's ticks **moved with it** (decision **D7**): the
  `39`-minute period tick and the ladder's `100`-millisecond tick belong to
  the publication's task. The transport's service loop gains **no** timer, and
  its own wake-up cadence does not change.
- Nothing is advertised and nothing about the wire changes (decision **D9**).
- **Plan 517's and Plan 518's `M1` is closed.** The publication's delivery no
  longer shares the transport's service loop. Both closure records are **not**
  rewritten; the disposition lives here and in the live index's rows.

## What the plan did not close

- The **stock-peer** half. That a pinned stock floodfill accepts the router's
  record in the live network, needs Plan 493's authorized two-host topology,
  and no live claim is made.
- The **live-dial** half. The lane drives the publication's public behaviour
  **without a socket**, so a live slow dial inside the transport's loop is not
  re-measured end to end. What stands between the publication and the loop is
  the structural guard, which reads the production files.
- The reference's **exploratory-tunnel** route. i2pr installs none in
  production, so the direct route is the only one.
- The publication's **cadence**. The period stays `39 * 60` seconds; the
  floodfill's RouterInfo timeout is 60 minutes, so no cadence change is
  needed.
- The reference's **variance** on the publish interval and its **congestion**
  update. Plan 515 decided against the variance and that decision stands; the
  congestion line is the bandwidth-metering line, still open elsewhere.
- Any change to the published RouterInfo's content, and any advertisement.

## Implementation commits

| Commit | Subject |
| --- | --- |
| `3d9174c8` | docs(plans): register Plan 519, the publication's delivery off the transport's loop |
| `6924e92f` | feat(publication): the router's own publication runs on its own task (Plan 519) |

The closing commit adds this record and the registry and roadmap rows.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| the transport's loop **never awaits** the publication | `crates/i2pr-daemon/tests/router_info_publication_off_loop_boundary.rs`, `the_loop_never_awaits_the_publication` |
| the publication's **state is not owned** by the loop | the same guard, `the_publications_state_is_not_owned_by_the_loop` |
| the task is spawned with **explicit ownership and cancellation** | the same guard, `the_task_is_spawned_with_ownership_and_cancellation` |
| the acknowledgement handoff is **bounded** | the same guard, `the_acknowledgement_handoff_is_bounded`; and `crates/i2pr-daemon/tests/router_info_publication_off_loop.rs`, `a_full_handoff_counts_the_acknowledgement_as_unmatched` and `the_handoff_never_blocks_the_loop` |
| the selection **prefers an established session** | the same guard, `the_selection_prefers_an_established_session`; and the lane, `the_selection_prefers_an_established_session`, `the_preference_keeps_the_selection_order` and `the_preference_never_adds_a_target` |
| the skipped targets are **counted** | the lane, `the_skipped_targets_are_counted_under_their_own_reason` and `a_batch_with_no_established_session_yields_an_empty_preference` |
| the delivery timeout is **bounded** and the publication's own | the same guard, `the_delivery_timeout_is_bounded_and_owned`; and the lane, `the_delivery_timeout_is_bounded_and_is_the_references_own` and `a_delivery_that_timed_out_is_refused_under_its_own_reason` |
| the transport's loop carries **no publication timer** | the same guard, `the_loop_carries_no_publication_timer` |
| a cancelled publication is **counted** and does not move the period | the lane, `a_cancelled_publication_is_counted_under_its_own_reason` |
| the ladder's timing is **unchanged** | the lane, `the_ladders_timing_is_unchanged` |
| the correlator's matching is **unchanged** | the lane, `the_correlators_matching_is_unchanged` |
| Plan 518's staleness rule is **unchanged** | the lane, `the_staleness_rule_is_unchanged` |
| the period and the fan-out are **unchanged** | the lane, `the_period_and_the_fan_out_are_unchanged` |
| every rule fires | the same guard's **fourteen inverted probes**, and the negative controls below |
| Plan 515's, Plan 516's, Plan 517's and Plan 518's evidence does not regress | Plan 515's lane (`crates/i2pr-daemon/tests/router_info_publication.rs`, 8) and guard (`.../router_info_publication_boundary.rs`, 15), Plan 516's lane (`.../router_info_publication_confirmation.rs`, 8) and guard (`.../router_info_publication_confirmation_boundary.rs`, 15), Plan 517's lane (`.../router_info_publication_retry.rs`, 14) and guard (`.../router_info_publication_retry_boundary.rs`, 17), Plan 518's lane (`.../router_info_publication_refresh.rs`, 11) and guard (`.../router_info_publication_refresh_boundary.rs`, 20) — all green on the closing tree, and the four guard files byte-identical |

## Focused verification

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo check --locked --workspace --all-targets` | clean |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_off_loop -- --test-threads=1` | 15 passed; 0 failed |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_off_loop_boundary -- --test-threads=1` | 21 passed; 0 failed (7 rules + 14 inverted probes) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_refresh -- --test-threads=1` | 11 passed; 0 failed (Plan 518's lane) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_refresh_boundary -- --test-threads=1` | 20 passed; 0 failed (Plan 518's guard) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_retry -- --test-threads=1` | 14 passed; 0 failed (Plan 517's lane) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_retry_boundary -- --test-threads=1` | 17 passed; 0 failed (Plan 517's guard) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation -- --test-threads=1` | 8 passed; 0 failed (Plan 516's lane) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation_boundary -- --test-threads=1` | 15 passed; 0 failed (Plan 516's guard) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication -- --test-threads=1` | 8 passed; 0 failed (Plan 515's lane) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_boundary -- --test-threads=1` | 15 passed; 0 failed (Plan 515's guard) |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 51 tests, OK |
| `python3 scripts/check-global-plan-number-uniqueness.py` | passed |
| `python3 scripts/check-tooling-inventory.py` | passed |
| `python3 scripts/check-adr-number-uniqueness.py` | passed |

Plan 515's, Plan 516's, Plan 517's and Plan 518's **guard files** are
byte-identical to their committed state: Plan 519 extends the publication's own
guard and touches none of theirs.

## The routine floor

The routine floor was run on the closing tree at commit `6924e92f`. It is
**145 steps, 145 PASS, 0 FAIL**, and the per-step record is the `RESULT` block
of the run's log.

**The first run of this floor was short by thirty-eight steps, and the record
says so.** The first log recorded 107 results against the script's 145 steps:
`cargo clippy`, `cargo doc`, `cargo test --doc` and thirty-four checker scripts
never ran. The cause was procedural and not a property of the tree: bash reads a
script **incrementally**, so the floor script was rewritten while an instance of
it was still executing, and that instance resumed at a byte offset that landed
past the block it had not yet run. A floor that silently skips two thirds of
its checks is worse than a floor that fails, because it reports a green it never
measured.

The floor was therefore **re-run from scratch** on the same commit, from a
script that was written before the run and was not touched while it ran, and the
numbers above are that run. The reconciliation that found the gap is mechanical:
each script step is matched against the log's `RESULT` lines, and every earlier
floor log in `/tmp` reconciles exactly — Plan 518's 143 of 143 included — so the
anomaly belongs to this plan's own run and not to the house.

The closing commit adds this closure record and the two live-index rows. Both
are prose under `plans/`; no production file, no test file and no configuration
changes after the floor ran, so no step's outcome can differ.

## The negative controls

Fourteen controls were applied to the real tree, each reverted byte-for-byte with its SHA-256 asserted. **All fourteen were caught by an assertion**: none was missed, and none was reported as a build failure. Every mutation compiled, because a control that breaks the build is a *missed* control rather than a caught one.

| ID | Mutation | Detector | Assertion it failed on |
| --- | --- | --- | --- |
| A | the publication's work moves out of the task | the guard | `the_loop_never_awaits_the_publication` |
| B | the transport's loop binds the publication's owner | the guard | `the_publications_state_is_not_owned_by_the_loop` |
| C | the composition root creates an ownerless task | the guard | `the_task_is_spawned_with_ownership_and_cancellation` |
| D | the publication's task never observes its cancellation | the guard | `the_task_is_spawned_with_ownership_and_cancellation` |
| E | the acknowledgement handoff becomes unbounded | the guard | `the_acknowledgement_handoff_is_bounded` |
| F | the hand-over waits for room | the guard | `the_acknowledgement_handoff_is_bounded` |
| G | the preference ignores the established peers | the guard | `the_selection_prefers_an_established_session` |
| H | the batch does not count the skipped targets | the guard | `the_selection_prefers_an_established_session` |
| I | the delivery is not bounded by a timeout | the guard | `the_delivery_timeout_is_bounded_and_owned` |
| J | the delivery timeout is unbounded | the guard | `the_delivery_timeout_is_bounded_and_owned` |
| K | the delivery timeout is no longer the reference's | the lane | `the_delivery_timeout_is_bounded_and_is_the_references_own` |
| L | the preference does not prefer | the lane | `the_selection_prefers_an_established_session` and three more |
| M | a refused acknowledgement is dropped silently | the lane | `a_full_handoff_counts_the_acknowledgement_as_unmatched` |
| N | a cancellation is not counted | the lane | `a_cancelled_publication_is_counted_under_its_own_reason` |

Every control's mutation **compiles**: a control that breaks the build is
weaker than one that breaks an assertion, and the harness reports a build
failure as a *missed* control. Every mutation is reverted byte-for-byte and
the SHA-256 is asserted; the restore writes the file's **bytes**, so cargo
rebuilds the crate after every control.

## Invariant review

| The plan's invariant | State | Evidence |
| --- | --- | --- |
| 1. The SSU2 service's loop never awaits the publication | **held** | the guard's `the_loop_never_awaits_the_publication`, and the control that moves the publication's work out of the task |
| 2. The transport's socket I/O is never blocked by the publication | **held** | the runtime is unchanged; the publication reaches the transport only through the cloneable service and the bounded delivery seam |
| 3. One cycle publishes one record | **held** | Plan 518's `the_cycle_holds_one_record_for_every_rung` and `the_cycle_record_is_the_owners`, green and byte-identical |
| 4. The ladder's timing is unchanged | **held** | the lane's `the_ladders_timing_is_unchanged` — the reference's 1600 ms, armed from the moment the batch went out |
| 5. The acknowledgement is still correlated | **held** | the lane's `the_correlators_matching_is_unchanged` |
| 6. Every refusal is counted under its own reason | **held** | the new counters (`deliveries_timed_out`, `skipped_no_session`, `unmatched`) are separate, and the lane asserts that a timeout is not merged with a refusal |
| 7. The publication's record is still fresh | **held** | the lane's `the_staleness_rule_is_unchanged` |
| 8. Nothing is advertised | **held** | `router.version` stays `0.9.62`; the inventory's advertised count stays zero; `Cargo.toml` and `Cargo.lock` are byte-identical |

## Failure, cancellation, restart, and contention review

- **A cancelled publication** stops at its next await, halts the cycle, and
  counts the cancellation under **its own reason** (`cancelled`). The
  cancellation is not an attempt: the publication's period is not moved.
- **A restart** rebuilds the task and its state with the service. The owner is
  fresh, so `last_attempt_ms` is `None` and the first publication is due
  immediately; the record is re-dated and re-signed by Plan 518's path at the
  cycle's start.
- **A full acknowledgement channel** counts the acknowledgement as unmatched.
  The loop is never blocked, the acknowledgement is never dropped silently,
  and the count is folded into the publication's own counters on the task's
  next wake.
- **A delivery timeout** ends the attempt, counts it under its own reason,
  abandons the awaited batch, and lets the ladder move to the next target. It
  never holds the publication's task, because the timeout bounds it.
- **A failed spawn** fails the SSU2 service closed: the owner is shut down
  and the service returns `Failed`. A publication that cannot be owned is not
  started.
- **Contention** — one publication task, one cycle at a time. A period tick
  that finds a cycle already running leaves it alone, exactly as Plan 517
  decided.

## Migration and compatibility review

- No wire change, no configuration key, no dependency. `Cargo.toml` and
  `Cargo.lock` are byte-identical.
- The published RouterInfo's content is unchanged.
- The publication's period, its fan-out, its ladder, its correlator and
  Plan 518's staleness rule are unchanged.
- The only **signature** change is the publication's three entry points, which
  now take the narrow `Ssu2PublicationHandle` instead of the service handle.
  The capability carries the same shared runtime state and grants nothing the
  daemon did not already hold.
- Plans 515's, Plan 516's, Plan 517's and Plan 518's lanes and guards are green
  on the closing tree, and their guard files are byte-identical.

## Security review

- The publication's task holds the router's identity bundle — the same
  `Arc<RouterIdentityBundle>` the service closure already holds. The bundle
  has **no** `Debug` implementation and is not cloned; the task's own `Debug`
  prints **no field**, so the bundle cannot be printed by accident.
- The handoff carries a **message id** — a correlation token, not a secret and
  not a peer identity. It is bounded, and a full channel counts rather than
  queues without bound.
- The counters carry **counts only**: no peer identity, no payload, no key.
- The logs carry a reason and a correlation id. No key, no signature, no
  record's bytes, and no peer identity.
- The narrow capability opens **no** new authority: it carries the same
  `Ssu2RuntimeService` the service handle carries, over the same shared state.
- No new listener, no new socket, no new configuration surface, and no change
  to what the router advertises.

## Limitations

- The **stock-peer** half is blocked on Plan 493's authorized two-host
  topology: that a pinned stock floodfill accepts the router's record in the
  live network is not proved here and is not claimed.
- The **live-dial** half is not re-measured. The lane drives the publication's
  public behaviour without a socket, so the structural guard is what proves
  the separation.
- The reference's **direct-or-tunnelled** choice is not implemented; the
  tunnelled branch is unreachable in this tree, so the batch falls back to
  the direct route when no target holds a session.
- The reference's **variance** on the publish interval is not adopted, and its
  **congestion** update is not implemented.
- The publication's task now carries its **own** `100`-millisecond ladder
  tick, so the router wakes at that cadence for the publication even when no
  cycle is running. The transport's loop carried the same cadence before, so
  this is one additional timer rather than a change of rate; a successor could
  sleep until the owner's deadline instead of ticking.
- The cycle's record lives in the **owner**, so the owner holds one copy of
  the router's own public record. It is public data, not a secret, and it is
  bounded by the runtime's own encoded-length ceiling.

## Findings by severity

### Medium — closed by this plan

- **M1 (Plan 517's and Plan 518's) — The publication's delivery shared the
  SSU2 service's loop.** Closed here. The publication's owner, its ticks, its
  correlator and its cycle record now live in a task of their own, spawned
  under the service's child scope with an explicit cancellation; the service's
  loop holds none of that state and never awaits it; and the inbound
  acknowledgement crosses a bounded channel that the loop never waits on. The
  delivery is further bounded by the publication's own 90-second per-attempt
  timeout, so a hung handshake can hold neither the loop nor the task.

### Low — recorded, not fixed

- **L1 — The daemon deep-dive's published counts have drifted from the tree,
  and six of the daemon's modules are not documented at all.**
  `docs/architecture/i2pr-daemon.md` claims **130** integration test files
  while the tree holds **132**; its acceptance-suite inventory's count column
  sums to **82** while its own prose describes "72 of the 112 files", so the
  prose, the table and the tree disagree three ways. Thirty-two of the
  sixty-five module rows carry a stale line count (`src/service_tunnels.rs`
  9 342 against a real 12 336; `src/sam/streams.rs` 2 075 against 3 423;
  `src/floodfill.rs` 3 089 against 3 941), the header's "69 `.rs` files, 119 860
  lines of `src`" against a real **76 files and 133 168 lines**, and **six**
  modules are absent from the document entirely:
  `src/app_runtime.rs`, `src/app_runtime_qualification.rs`,
  `src/bandwidth_eligibility.rs`, `src/encrypted_service_resolver.rs`,
  `src/outproxy_options.rs` and `src/tests/support/reseed_fixture.rs`.
  **Not fixed here**: this plan's own deep-dive edits are correct for the files
  it touched, the drift is pre-existing and spans most of the crate, and a
  mechanical re-derivation of every row belongs to a plan of its own — one that
  derives the numbers from the tree instead of letting a hand-kept count rot
  again, the mechanism Plan 372 built for the tooling inventory. It is named
  as successor work.

### Informational

- **I1 — The guard's first boundedness check was not value-based, and a
  control proved it.** The guard's rule 6 read the delivery timeout's
  *declaration* and refused only the spelling `u64::MAX`, so a control that
  raised the constant to `90_000_000` — an unbounded timeout written as an
  ordinary literal — was **missed** (13 of 14 controls caught). The rule now
  parses the constant's **value** and bounds it, and the control is caught.
  The lesson is the one Plan 514 and Plan 518 recorded: a control that changes
  the code but not the observable is **vacuous**, and only a control that
  changes the *value* can show the rule is load-bearing.
- **I2 — A lane that pins a constant must not compare it as a constant.**
  `clippy::assertions_on_constants` refused the lane's four range assertions
  on `PUBLICATION_DELIVERY_TIMEOUT_MS` and
  `PUBLICATION_ACKNOWLEDGEMENT_CAPACITY`. The lane binds each constant to a
  local first, so the assertion reads a value rather than a constant
  expression. The assertions are unchanged in what they pin.
- **I3 — A controls harness measures the editor, not the control, when the
  tree moves under it.** One harness run reported control C as "the mutation
  broke the build". The cause was that the guard file was being **edited while
  the harness was running**: the mutation was fine, and the concurrent edit
  broke the test target's build. Re-run with no concurrent edit, the same
  control is caught. The harness's own lesson is recorded in its header: the
  tree must not move under a controls run.
- **I4 — The plan of record's §6 named the wrong file for the publication's
  task.** It said the task would live in `router_info_publication.rs`, and it
  lives in the composition root instead, because Plan 515's, Plan 516's and
  Plan 517's guards bind the publication's calls to
  `crates/i2pr-daemon/src/lib.rs` and those guard files stay
  byte-identical. The plan's **§17** records the amendment, and the handoff
  — which needs no guard anchor — is the new module
  `crates/i2pr-daemon/src/router_info_publication_task.rs`.
- **I5 — Decision `D6` was corrected, because read literally it would have
  left the router unable to publish.** It said that when no target has a
  session the batch is *refused*. i2pr installs no exploratory tunnel, so the
  direct route is the only route it has, and refusing the batch would have
  meant the router never publishes its own RouterInfo whenever it holds no
  floodfill session — which contradicts this plan's objective and Plan 491's
  line. The implementation **prefers** an established session and, when the
  batch holds none, asks it anyway on the direct route, under the
  publication's own bounded timeout. The plan's **§17.3** records the
  correction.
- **I6 — The lane and the guard split the wiring from the behaviour, as
  Plan 513's lane does.** The plan of record's §11 listed every case under the
  lane; the wiring cases are structural claims about the production files,
  and the tree's house pattern puts them in the **guard**. The lane's header
  says so, and §18.8 of `specs/QUALIFICATION-GATES.md` names what is **not**
  gated.
- **I7 — A mistake was made and repaired: `git checkout` discarded an
  uncommitted file.** Restoring `crates/i2pr-daemon/src/lib.rs` with
  `git checkout` reverted it to `HEAD` and discarded the plan's
  implementation in it. It was recovered by re-applying the recorded patch
  scripts, and the restored file was re-verified by the whole floor. The
  lesson is procedural: on a tree whose work is uncommitted, `git checkout
  <path>` is destructive, and a restore must **re-apply** the change, never
  check it out.
- **I8 — The publication's task carries its own ladder tick.** The ladder's
  `100`-millisecond tick used to ride the transport's loop's relay tick; it
  is now the publication's own, so the router wakes at that cadence for the
  publication even when no cycle is running. The transport's loop carries no
  publication timer, so the loop's own cadence is unchanged; the cost is one
  additional timer in the router.

- **I9 — A floor script that is edited while it runs reports a green it never
  measured.** The first floor run for this plan recorded 107 results against the
  script's 145 steps, and the thirty-eight steps it skipped included
  `cargo clippy`, `cargo doc`, `cargo test --doc` and thirty-four checker
  scripts. The cause is bash's incremental script reading: a script rewritten
  under a running instance makes that instance resume at a byte offset in the
  new text, so whole blocks of steps are never executed. The reconciliation is
  mechanical — every script step is matched against the log's `RESULT` lines —
  and it found the gap before this record was written. The floor was re-run
  from scratch on the same commit with a script that was not touched while it
  ran, and the closing record's floor table is that run. The lesson is the one
  I3 records for the controls harness: a harness that moves under the
  measurement measures the movement, not the thing.

## Roadmap disposition

- The network-participation roadmap's row for Plan 519 moves from
  *registered* to **closed**, with the closure record named.
- The registry's Plan 519 row moves from *registered* to **closed**, in the
  house shape every other closed row uses.
- The registry's and the roadmap's **Plan 518 rows** now record that the `M1`
  Plan 517 carried — the publication's delivery sharing the SSU2 service's
  loop — is **closed by Plan 519**. Plan 517's and Plan 518's **closure
  records are not rewritten**.
- The roadmap's open item for a **live** stock-peer confirmation stays open:
  it is Plan 493's topology.
- Plan 491's `F2` stays open for the same reason.

## Unblock audit

| Item | State |
| --- | --- |
| Plan 491 **F1** — the receipt log is not produced by a live product loop | **closed** by Plan 503 |
| Plan 491 **F2** — no stock-peer evidence for the floors | **still blocked** on Plan 493's authorized two-host topology |
| Plan 491 **F3** — `check-runtime-boundaries.sh` has no `i2pr-netdb` section | **closed** by Plan 497's WP5 and verified by Plan 503 |
| Plan 491 **F4** — the intermittent `i2pcontrol_tunnels` staging failure | **conditionally discharged** by Plan 510; the failure itself belongs to the I2PControl / service-tunnel line |
| Plan 491 **F5** — the comparator's four-component ceiling is an unstated assumption | **closed** by Plan 511 |
| Plan 515 **F1** — the reference's resend burst | **closed** by Plan 517 |
| Plan 516 **F1** — the reference's resend burst | **closed** by Plan 517 |
| Plan 517 **M1** — the publication's delivery shares the SSU2 service's loop | **closed** by Plan 519 |
| Plan 518 **M1** — the same finding, carried forward | **closed** by Plan 519 |
| Plan 497 **M2** — the routing table's live reader | **not reopened**; Plan 519's selection is the router-wide store's, exactly as Plan 515's, Plan 517's and Plan 518's were |

## Handoff notes

- The next free plan number after this one is **520**.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`).
- Do not patch or vendor either reference.
- The **stock-peer** half of the publication's evidence stays blocked on
  Plan 493's authorized two-host topology. This plan does not and cannot
  unblock it.
- The publication is now **off the transport's loop**. The next thing this
  line needs is a peer the router is **already connected to**, so that the
  publication's ordinary path is the no-handshake path; that is a
  participation question, not a publication question.
