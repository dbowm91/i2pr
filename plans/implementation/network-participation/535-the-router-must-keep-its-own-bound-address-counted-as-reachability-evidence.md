# Plan 535 — the router must keep its own bound address counted as reachability evidence

**Plan of record.** Subsystem: `network-participation`. Registered 2026-11-11.
The successor to Plan 534, and the named owner of the limitation Plan 530
recorded as out of scope with an owner: *"the class lapses with the
observation TTL"*.

## 1. Objective

One bounded outcome: **the router's own bound address must stay counted as
reachability evidence while the router is running.**

The router needs **three distinct** supporting classes before the transport's
reachability model will say `Reachable`
(`crates/i2pr-transport/src/reachability.rs:640`). Plan 530 gave the router
the first of them — the **explicit bind** — and it is the only one of the
three that is a fact about **this router** rather than about a peer.

Two of the three classes are refreshed by their peers: the peer's observed
external address (Plan 529) arrives when a peer sees the router, and the
confirmed peer test (Plan 521) arrives when a peer answers the router's test.
The third — the explicit bind — is refreshed by **nobody**, because it is the
router's own fact and the router never re-asserts it. It is recorded once,
at bind time, and the model holds every signal as a **timed observation**, so
after `DEFAULT_OBSERVATION_TTL` (thirty minutes) the class **lapses**.

After that, the router's corroboration count can never exceed two again, and
`Reachable` is unreachable **for ever** — not until the next restart. A
long-running router silently loses the one class it could have kept.

## 2. Why ready

- Plan 530 is **closed** (`plans/closure/network-participation/530-status.md`)
  and named the successor: the finding is *"The class lapses with the
  observation TTL"* (`:495`), with the owner named as the successor that earns
  the direct form in the field.
- The **recorder** exists and is the runtime's
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3655`), and it is already gated
  on the daemon's **existing** `explicit_bind_corroboration` opt-in.
- The runtime already has an **existing** maintenance pass
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:8151`), driven by the socket
  loop's own wake, so the re-assertion needs **no new timer**.
- The **expiry rule has one owner**: the transport's tracker publishes its own
  observation lifetime (`crates/i2pr-transport/src/reachability.rs:317`), so the
  runtime can read the lifetime it must respect instead of restating it.
- No interface dependency is open. The tracker, the policy and the recorder
  are all closed contracts.

## 3. Current implementation evidence

### 3.1 The class is recorded once, at bind time

The explicit-bind recorder is the runtime's
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3655`), and it is called from
exactly one place — the bind path
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2324`):

```rust
fn note_bound_endpoints_as_explicit_bind(&self) {
    if !self.shared.config.explicit_bind_corroboration {
        return;
    }
    ...
    state
        .reachability
        .record(ReachabilitySignal::LocalConfiguredBind { family }, now);
}
```

There is no second caller. The class is therefore asserted **once** for the
router's whole life.

### 3.2 The model expires every observation

The reachability tracker holds every signal as a timed observation
(`crates/i2pr-transport/src/reachability.rs:60`):

```rust
pub const DEFAULT_OBSERVATION_TTL: Duration = Duration::from_secs(30 * 60);
```

and drops the expired ones
(`crates/i2pr-transport/src/reachability.rs:558`):

```rust
fn expire_locked(&mut self, now: Duration) {
    while let Some(front) = self.signals.front() {
        if front.observed_at.saturating_add(self.policy.observation_ttl) > now {
            break;
        }
        self.signals.pop_front();
    }
}
```

### 3.3 The expiry is polled on every read

The runtime polls the expiry on each of the three paths that read the
evidence (`crates/i2pr-runtime/src/ssu2_runtime.rs:2112`, `:2401` and
`:3552`), so the lapse is not theoretical: the first read after the lifetime
has elapsed drops the class, and the router's corroboration count drops with
it.

### 3.4 Nothing re-asserts the class

There is no second recorder for the class anywhere in the tree. The
**controlled** recorder
(`crates/i2pr-daemon/src/floodfill.rs:648`) is a different, narrower seam:
it is called by the controlled M12 activation only, and it records the same
class for the same router. The ordinary router has no equivalent.

### 3.5 The consequence is the `Reachable` state

`Reachable` needs **more** than the corroboration floor
(`crates/i2pr-transport/src/reachability.rs:640`):

```rust
if supporting > self.policy.min_corroboration {
    self.transition(ReachabilityState::Reachable, now);
}
```

With the floor at two, three distinct classes are needed. After the bind class
lapses, only the two peer-sourced classes remain, so the router is
`CandidateReachable` at best — and its own publication path refuses with
`ReachabilityUnqualified`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3561`).

## 4. Invariants

1. **The expiry rule has one owner.** The runtime reads the observation
   lifetime from the tracker's **own policy**
   (`crates/i2pr-transport/src/reachability.rs:448`), and never restates it.
2. **The class is re-asserted strictly before it can lapse.** The re-assertion
   happens while the previous record is still live, so the class is never at
   risk of lapsing, whatever the read cadence.
3. **The re-assertion adds no timer.** It rides the runtime's **existing**
   maintenance pass.
4. **The recorder stays the runtime's, and stays gated.** The re-assertion is
   the same class, on the same opt-in, from the same owner. A runtime that
   did not ask for explicit-bind corroboration re-asserts nothing.
5. **The transport's reachability model is untouched.** Its signal vocabulary,
   its corroboration classes, its floors, its states and its expiry semantics
   are exactly what the tree already had. The tracker gains no method.
6. **No configuration key is added.** The daemon's configuration surface is
   unchanged, and the daemon's mapping into the runtime is unchanged.
7. **The re-assertion is counted.** Every re-assertion is counted under its
   own counter, and the counter carries counts only.
8. **A local condition is never written into a peer's profile.** The
   re-assertion touches the router's own reachability tracker and nothing
   else.
9. **No advertisement.** `router.version` stays `0.9.62`, the published
   RouterInfo and `caps` are unchanged, and the inventory's advertised count
   stays **zero**.
10. **No boundary script is weakened.** No `|| true`, no
    `continue-on-error`, no filename filtering, no fake peer environment, no
    broad exclusion, no early-return-success, and no production wire change
    made to go green.

## 5. Scope

### 5.1 In scope

1. A **re-assertion** of the router's own explicit-bind corroboration class,
   while the router is running and the bind holds.
2. The **rule** that decides when the re-assertion is due, expressed as a
   public, pure function so it can be proved rather than described.
3. The runtime's **observation lifetime** as a policy parameter of the
   runtime's own configuration struct, so the behaviour is provable and the
   production default is the transport's own.
4. A counter for the re-assertions, exposed as counts only.
5. A lane that proves the class survives past the observation lifetime, and a
   static guard that is the authority for the shape.

### 5.2 Out of scope

- The **peer's** two classes. The observed external address (Plan 529) and
  the confirmed peer test (Plan 521) are refreshed by their peers, and
  nothing here changes them.
- The **controlled** M12 activation's recorder
  (`crates/i2pr-daemon/src/floodfill.rs:648`), which is a different seam and
  is unchanged.
- The transport's reachability **model**, the corroboration floor and the
  contradiction floor.
- Any change to `Cargo.toml`, `Cargo.lock` or the dependency graph.
- The **stock-peer** half. The delivery stays confined to the controlled
  loopback profile, and the stock-peer evidence stays blocked on Plan 493's
  authorized two-host topology.

### 5.3 Out of scope, with the owner named

- The **production** SSU2 peer-test driver and its choreography question.
  **Owner:** the plan that runs the production driver with a stock helper.
- The runtime's **direct form is still loopback-only**. The publication
  profile that would carry it outside the controlled loopback profile is a
  separate decision. **Owner:** the plan that widens the publication profile.
- Plan 534's two carried-forward limits — the **production** peer-test driver
  and the target role's dependence on the bootstrap's store — are **not** this
  plan's subject.
- Plan 530's Low finding 2 — the **production** SSU2 peer-test driver — and
  finding 3 — the requester side's missing production caller — are
  **discharged by Plans 531 and 533** and are not reopened here.

## 6. Required production changes

### 6.1 `crates/i2pr-runtime/src/ssu2_runtime.rs`

1. A new **policy sub-struct** `Ssu2RuntimeReachability` carrying
   `observation_ttl: Duration`, with a manual `Default` of
   `i2pr_transport::DEFAULT_OBSERVATION_TTL`. It is a field of
   `Ssu2RuntimeConfig`, which therefore keeps its derived `Default`.
2. The runtime's **reachability policy** is built with that lifetime in both
   branches, so the policy the tracker publishes is the policy the runtime
   configured.
3. `ServiceState` gains `explicit_bind_recorded_at: Option<Duration>` — the
   time the standing fact was last asserted. It lives in the **shared** state
   so the second socket loop's pass does not assert it twice.
4. A **public, pure** function `explicit_bind_refresh_due(recorded_at, now,
   observation_ttl) -> bool` — the rule that decides when the standing fact
   must be re-asserted. It is pure so the lane can prove it exhaustively.
5. A **re-assertion** step in the runtime's existing maintenance pass
   (`drive_timeouts_locked`), which reads the lifetime from the tracker's own
   policy and re-records the class when the rule says so.
6. A counter `explicit_bind_refreshes` in the runtime's counters, published
   in the runtime's snapshot as counts only.

### 6.2 `crates/i2pr-daemon/src/config.rs`

**No change.** The daemon's configuration surface gains no key.

### 6.3 `crates/i2pr-daemon/src/router_i2np.rs`

**No change.** The daemon's mapping into the runtime keeps the runtime's
default.

## 7. Design decisions

### 7.1 The class is a standing fact, and the model is not

The transport's model holds every signal as a timed observation, and that is
right for a peer-sourced class: an observation of a peer says something about
the world at a moment, and it goes stale. The explicit bind says something
about **this router's own socket**, and that does not go stale while the
socket is bound.

The tree's own vocabulary already carries this distinction: the signal is
`LocalConfiguredBind`, not `ObservedBind`. The plan therefore does not change
the model — it makes the router **re-assert** the fact the model holds, so
the fact's own lifetime is renewed by the owner of the fact.

### 7.2 The re-assertion happens strictly before the lapse

The rule is: re-assert once the record has aged past **half** the observation
lifetime. Half is chosen so that the re-assertion always happens while the
previous record is still live, and so that a missed maintenance pass cannot
lapse the class — the margin is a factor of two on a lifetime of thirty
minutes, against a maintenance pass that runs at least every 200
milliseconds.

A rule of "re-assert once the record has expired" would be wrong: the
expiry poll runs on the **read** paths, so a read between the record's
expiry and the next maintenance pass would find the class already gone.

### 7.3 The lifetime is read from the tracker's policy, not restated

The runtime does not have its own number for the observation lifetime. It
reads the tracker's own policy
(`crates/i2pr-transport/src/reachability.rs:448`), so the expiry rule has
**one** owner and the runtime cannot drift from it. The runtime's
configuration parameter exists so the *policy* can be configured, and the
runtime then reads back what it configured.

### 7.4 The re-assertion rides the existing maintenance

The runtime already wakes its socket loop at least every
`scheduler_poll_max` (200 milliseconds) and runs its maintenance pass on
every wake (`crates/i2pr-runtime/src/ssu2_runtime.rs:8151`). The
re-assertion is one step of that pass. **No new timer** is introduced, and
the daemon's product tick is not involved.

### 7.5 The re-assertion records the class again, and the queue stays bounded

The re-assertion **records** the class rather than mutating the tracker's
queue, so the transport's tracker gains no method and its model is untouched.
At half the observation lifetime, the re-assertion happens twice an hour in
production, so the tracker's bounded queue grows by two entries an hour and
each entry expires with the tracker's own rule. The queue's bound is
unchanged and no eviction is provoked.

### 7.6 The families come from the bind, not from the sockets lock

The re-assertion names the families the router's own bound sockets can be
named for. Reading them from the sockets lock **inside** the state lock would
invert the runtime's lock order (sockets → state), so the families are
captured at bind time — the runtime binds once and does not rebind — and the
re-assertion reads them from the shared state.

### 7.7 The re-assertion is a fact about this router, and only this router

The re-assertion touches the router's own reachability tracker. It writes no
peer profile, claims no corroborating class for any peer, and records no
reachability signal about anything but this router's own socket.

## 8. Work packages

| Work package | What it carries |
| --- | --- |
| WP1 | the runtime's reachability policy parameter, the shared state field and the rule |
| WP2 | the re-assertion in the runtime's existing maintenance pass, and the counter |
| WP3 | the lane and the guard |
| WP4 | the documentation, the inventory and the plan's amendment log |
| WP5 | the closure record and the registry's closed rows |

## 9. Failure, cancellation, restart and contention semantics

**Failure.** A runtime that cannot take the state lock re-asserts nothing and
the class lapses on its own rule — the same outcome as before this plan, and
never a wrong claim. A runtime without the opt-in re-asserts nothing at all.

**Cancellation.** The re-assertion is a synchronous step inside the runtime's
existing maintenance pass. It awaits nothing and holds no lock across an
await, so a cancellation cannot strand it.

**Restart.** Nothing is persisted. The class is recorded again at the next
bind, exactly as before.

**Contention.** The re-assertion takes the runtime's existing state lock,
in the existing order (sockets → state), and never the sockets lock while
the state lock is held. The two socket loops share the state, so the second
loop's pass sees the first's timestamp and does not assert the fact twice.

## 10. Compatibility and migration

**No wire change.** The reachability signal, its class and the publication
that consumes them are exactly what the tree already produced.

**No configuration migration.** **No key is added.** The daemon's
configuration surface is unchanged and a configuration that names nothing new
behaves exactly as before.

**A new reading for the router.** A router that has asked for explicit-bind
corroboration now keeps that class for as long as it runs. It publishes
nothing new and advertises nothing new.

**No change to the transport's model.** The signal vocabulary, the
corroboration classes, the floors, the states and the expiry semantics are
unchanged, and the tracker gains no method.

## 11. Required tests

### 11.1 The lane

`crates/i2pr-daemon/tests/ssu2_explicit_bind_refresh.rs` — real loopback UDP
and a real runtime, and **no** private-API driving from the test. The lane
must prove:

1. the rule is **pure** and its truth table is exactly the stated one: the
   re-assertion is due strictly before the record can lapse, and never after;
2. a runtime whose observation lifetime is short **keeps** the explicit-bind
   class counted **past** that lifetime, where without the re-assertion the
   class would have lapsed;
3. the runtime's re-assertion counter moves, and the counter is counts only;
4. a runtime without the opt-in re-asserts **nothing** and records nothing;
5. the transport's reachability model is **untouched**: its vocabulary, its
   corroboration classes, its floors and its expiry semantics.

### 11.2 The guard

`crates/i2pr-daemon/tests/ssu2_explicit_bind_refresh_boundary.rs` — a static
guard that is the authority for the **shape**. It must have an inverted probe
for every rule and a fail-closed probe for every item a rule names. The
rules:

1. the re-assertion is gated on the **same** opt-in the recorder is, and a
   runtime that did not ask for it re-asserts nothing;
2. the observation lifetime is read from the tracker's **own policy** and is
   never restated in the runtime;
3. the re-assertion rides the runtime's **existing** maintenance pass and adds
   **no** timer;
4. the rule re-asserts **strictly before** the record can lapse;
5. the re-assertion touches the router's **own** tracker and writes no peer
   profile and no reachability signal;
6. the transport's reachability model is **untouched**, and the tracker gains
   no method;
7. **no** configuration key is added, in the daemon's surface or in the
   daemon's mapping;
8. every re-assertion is **counted**, and the counter carries counts only.

Plan 530's guard (`crates/i2pr-daemon/tests/ssu2_explicit_bind_corroboration_boundary.rs`)
must stay **byte-identical**: this plan changes the runtime's recorder's
**neighbour**, not the recorder, and the transport's model is untouched.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test ssu2_explicit_bind_refresh -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_explicit_bind_refresh_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_explicit_bind_corroboration --test ssu2_explicit_bind_corroboration_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor in `AGENTS.md`, with this plan's two steps added.

## 13. Documentation updates

- `specs/support.toml` — a new experimental, non-advertised surface
  `daemon.explicit-bind-refresh`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the gates this plan's lane and guard
  discharge.
- `specs/protocols/09-ssu2.md` — the explicit-bind scope, extended to the
  re-assertion.
- `docs/architecture/i2pr-runtime.md` — the reachability owner's shape.
- The plan's own amendment log.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when all of the following hold:

1. the explicit-bind class is **counted** past the observation lifetime, and
   the rule that keeps it counted is proved rather than described;
2. the re-assertion is **gated** on the existing opt-in, rides the existing
   maintenance, and adds no timer and no configuration key;
3. the transport's reachability model is **untouched**, and Plan 530's guard
   file is byte-identical;
4. the counter is counts only, and every re-assertion is counted;
5. `router.version` stays `0.9.62`, and the inventory's advertised count
   stays **zero**;
6. the full floor is green, with the closure record's table complete;
7. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the class cannot be kept counted without changing the transport's reachability
  **model** — the model is not this plan's to move;
- the re-assertion cannot be made to ride an **existing** pass — a second
  timer would be a new decision, not a shortcut;
- the floor's `cargo deny check advisories bans sources` fails;
- a stock peer would be required to prove any of the above — the stock-peer
  half stays blocked on Plan 493 and is **not** attempted here.

## 15. Closure evidence required

- The implementation commit, the lane-and-guard commit, the documentation
  commit and the closure commit, each named in the closure record.
- The **complete** floor table, every row with its result.
- The lane's rows, each with its result.
- The guard's rows, each with its result.
- The negative controls: each mutation, what it was expected to break, what
  it did break, and the SHA-256 proof that the tree was restored
  byte-identically.
- The requirement-to-evidence matrix, one row per requirement.
- The invariant review, the failure review, the migration review and the
  security review.
- The limitations, the findings by severity, the roadmap disposition and the
  unblock audit.

## 16. Handoff notes

- The plan is **experimental** and default-off. Do not describe the
  re-assertion as released, supported or advertised.
- The delivery is confined to the controlled loopback profile. A public
  deployment is unaffected: the class is re-asserted, not widened.
- The stock-peer half stays blocked on Plan 493's authorized two-host topology.
- Plan 530's closed closure record is **not** rewritten. The discharge lives in
  this plan's closure record and in the registry's row.
- The plan **extends** no other plan's guard: Plan 530's guard stays
  byte-identical.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 535 is registered as the successor to Plan 534 and the named owner of
Plan 530's recorded limitation: the explicit-bind class lapses with the
observation TTL, and the plan adds no timer to re-record it.

The scope is the router's **own** class: the peer-sourced classes are
refreshed by their peers, and the router's own class is refreshed by nobody.
The plan makes the router re-assert its own standing fact, with the expiry
rule owned by the transport's tracker and the re-assertion riding the
runtime's existing maintenance.

No closed closure record is rewritten by this plan. The corrections this plan
finds, if any, live in this plan's closure record and in the registry's row.
