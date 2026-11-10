# Plan 530 — the router must record its own bound address

**Status:** registered, 2026-11-10. The next free plan number after 529.

The transport's reachability model says `Reachable` needs **three distinct
supporting classes**. Two of them have ordinary producers. The third — the
router's own configured bind — has exactly **one** recorder, and that recorder
is called only by the controlled floodfill activation. So a router running in
ordinary operation can never publish a direct SSU2 address, no matter how many
peers observe it and no matter how many peer tests confirm. The daemon already
opted in; only the ordinary **recorder** is missing.

## 1. Objective

Make the router record its own bound address as the explicit-bind
corroboration class **in ordinary operation**, so the third class can be
earned outside the controlled floodfill activation.

Concretely:

1. the runtime records `ReachabilitySignal::LocalConfiguredBind` for every
   family it can **name** its own bound endpoint for, at **bind time**;
2. the record is gated on the **existing** `explicit_bind_corroboration`
   opt-in — no new configuration key and no default moved;
3. the transport's reachability model, the corroboration floor, the
   contradiction floor and the publication gate are all untouched;
4. the controlled activation's narrow recorder and its call site stay
   byte-identical — Plan 283's foundation is not rewritten.

## 2. Why ready

- The transport **already counts** class 0 when the policy lets an explicit
  local configuration corroborate direct reachability
  (`crates/i2pr-transport/src/reachability.rs:581`-`:585`).
- The daemon **already opts in**
  (`crates/i2pr-daemon/src/router_i2np.rs:1539`), and the runtime turns that
  opt-in into `configured_direct_allowed = true`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:1830`-`:1837`).
- The runtime already owns **both** the tracker and the sockets, so it is the
  one place that knows a bind succeeded.
- The other two classes have ordinary producers: Plan 529's handshake
  observation, and Plan 521's peer test on the product's own tick.
- The gap is a missing **recorder**, not a missing capability.

## 3. Current implementation evidence

### 3.1 The corroboration floor demands three classes

`crates/i2pr-transport/src/reachability.rs:66`:

```rust
pub const MIN_CORROBORATION_FLOOR: usize = 2;
```

and `crates/i2pr-transport/src/reachability.rs:639`-`:641`:

```rust
        let supporting = self.supporting_classes();
        if supporting > self.policy.min_corroboration {
            self.transition(ReachabilityState::Reachable, now);
```

`supporting` counts **distinct** classes
(`crates/i2pr-transport/src/reachability.rs:572`-`:589`), so `Reachable`
requires **three** distinct supporting classes, not three observations. The
transport's own canonical test says the same:
`crates/i2pr-transport/src/reachability.rs:796`-`:820` records
`LocalConfiguredBind`, `ValidatedPath` and `PeerTestResult{Confirmed}` and
only the third record reaches `Reachable`.

### 3.2 The four supporting classes

`crates/i2pr-transport/src/reachability.rs:223`-`:231`:

```rust
    const fn corroboration_class(self) -> u8 {
        match self {
            Self::LocalConfiguredBind { .. } => 0,
            Self::AuthenticatedPeerObservedExternalAddress { .. } => 1,
            Self::ValidatedPath { .. } => 2,
            Self::PeerTestResult { .. } => 3,
            Self::RelayFirewalledSignal { .. } => 4,
        }
    }
```

Class 4 contradicts; it never corroborates. So four classes can corroborate,
and three of them must be present for `Reachable`.

### 3.3 Class 0's only recorder is the controlled activation

`LocalConfiguredBind` is declared at
`crates/i2pr-transport/src/reachability.rs:132`, and the tree records it in
exactly **one** place:
`crates/i2pr-runtime/src/ssu2_runtime.rs:3210`.

```rust
                i2pr_transport::ReachabilitySignal::LocalConfiguredBind { family },
```

inside `note_explicit_bind_for_controlled_qualification`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3175`), whose own doc comment says:
*"Only the controlled M12 activation composition calls this; normal operation
never does."*

The one caller is the controlled floodfill activation:
`crates/i2pr-daemon/src/floodfill.rs:648`.

```rust
    service
        .note_explicit_bind_for_controlled_qualification()
        .map_err(ControlledActivationError::BindFailed)?;
```

So in ordinary operation the class is **counted** but never **earned**.

### 3.4 The other two classes do have ordinary producers

- **Class 1**, `AuthenticatedPeerObservedExternalAddress`: Plan 529's
  handshake observation, recorded by the runtime's comparison against the
  router's own bound socket
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:2879`-`:2888`).
- **Class 3**, `PeerTestResult{Confirmed}`: Plan 521's peer test, driven on
  the product's own tick (`crates/i2pr-daemon/src/service_product.rs:4688`,
  called from `:5130`), behind the explicit opt-in `ssu2.peer_test`, which is
  **off** by default (`crates/i2pr-daemon/src/config.rs:1441`-`:1443`).
- **Class 2**, `ValidatedPath`: recorded only on a path **migration**, so it
  is not an ordinary event.

### 3.5 The consequence: the direct form is unreachable in ordinary operation

The direct form's gate is
`crates/i2pr-transport-ssu2/src/publication.rs:244`-`:246`:

```rust
    let direct_allowed = policy.allow_direct()
        && reachability.state == ReachabilityState::Reachable
        && endpoint.is_some();
```

and the runtime's own publication path expects the direct outcome
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3140`). With class 0 unreachable in
ordinary operation, an ordinary router can reach at most **two** classes, so
its state stalls at `CandidateReachable` and it publishes the firewalled form
for ever.

### 3.6 The daemon already opted in

`crates/i2pr-daemon/src/router_i2np.rs:1539`:

```rust
        explicit_bind_corroboration: true,
```

which the runtime turns into the transport policy at
`crates/i2pr-runtime/src/ssu2_runtime.rs:1830`-`:1837`:

```rust
        let reachability_policy = if config.explicit_bind_corroboration {
            i2pr_transport::ReachabilityPolicy::new(
                2,
                i2pr_transport::MIN_CONTRADICTION_FLOOR,
                i2pr_transport::DEFAULT_OBSERVATION_TTL,
                true,
            )
```

The fourth argument is `configured_direct_allowed`
(`crates/i2pr-transport/src/reachability.rs:276`), and it is what makes the
transport count class 0 at all
(`crates/i2pr-transport/src/reachability.rs:581`-`:585`):

```rust
            if matches!(entry.signal, ReachabilitySignal::LocalConfiguredBind { .. })
                && !self.policy.configured_direct_allowed
            {
                continue;
            }
```

So the opt-in, the counting and the policy are all in place. **Only the
ordinary recorder is absent.**

### 3.7 The tree's own records name the missing triple

- Plan 526's closure record, finding 3
  (`plans/closure/network-participation/526-status.md:450`-`:452`): *"The
  runtime's direct form still needs three corroboration classes to be published
  at all, and no lane in this tree earns them: the explicit bind, the peer's
  observed external address and the confirmed peer test."*
- Plan 520's closure record, finding M1
  (`plans/closure/network-participation/520-status.md:594`): *"The mechanism
  is proved; the production route to it does not exist."* Its successor is to
  own *"a bounded production peer-test entry point (or the authenticated-peer
  observation that would supply the third class)"*
  (`plans/closure/network-participation/520-status.md:598`-`:599`) — Plan 521
  and Plan 529 delivered those two. **The explicit bind is the one that is
  still missing.**
- Plan 282's closure record
  (`plans/closure/floodfill/282-status.md:31`): the explicit bind plus the
  peer observation *"yields exactly `CandidateReachable`"*, locked by the
  transport's own test.
- Plan 283's closure record
  (`plans/closure/floodfill/283-status.md:33`): class 0 was kept as *"Plan 283
  foundation"* — the narrow controlled recorder.

### 3.8 The one existing assertion this plan must amend

`crates/i2pr-runtime/tests/ssu2_controlled_peer_test.rs:220`-`:223` asserts
that a started, opted-in service has **no** reachability evidence before the
controlled exchange:

```rust
    assert_eq!(
        fixture.service.snapshot().reachability,
        ReachabilityState::Unknown
    );
```

With the ordinary recorder in place that service holds **one** corroborating
class from bind time, so the state is `ObservedUnconfirmed` rather than
`Unknown`. The assertion's *material* claim — **no publication material** —
is untouched and still holds, because one class is far below the floor. The
label changes; the floor does not. This is the **only** existing assertion in
the tree that the plan moves, and §11.3 names it.

Every other reachability assertion in the tree runs on
`Ssu2RuntimeConfig::default()`, whose `explicit_bind_corroboration` is
`false` (`crates/i2pr-runtime/src/ssu2_runtime.rs:430`), or on a service that
bound nothing, or on a synthetic `Ssu2PublicationMaterial`. The tree's
existing lanes are therefore unaffected.

## 4. Invariants

1. The record is made at **bind time**, because that is when the fact becomes
   true: a locally configured bind address exists for the family from the
   moment the socket is bound.
2. A family is recorded only when the runtime can **name** its own bound
   endpoint for it — the same predicate `bound_endpoint` already applies
   (`crates/i2pr-runtime/src/ssu2_runtime.rs:2943`-`:2953`): a wildcard or a
   port-zero bind names no address and therefore records **nothing**.
3. The record is gated on the **existing** `explicit_bind_corroboration`
   opt-in. A runtime that has not asked for explicit-bind corroboration
   records nothing, and the transport's own `as_transport_observation` never
   sees a configured origin it did not earn.
4. The signal carries only the **family**, never a literal endpoint, so the
   record stays redacted by construction.
5. The recorder lives in the **runtime**, which owns the tracker and the
   sockets. The daemon records no reachability signal of its own — Plan 520's
   guard rule 10 stays green.
6. The controlled activation's narrow recorder and its call site are
   **byte-identical**. Plan 283's foundation is not rewritten.
7. The transport's reachability **model** — the five signal kinds, the five
   corroboration classes and the state machine — is untouched.
8. The **corroboration** floor is unchanged; `Reachable` still needs three
   distinct supporting classes.
9. The **contradiction** floor is unchanged (Plan 528's separation stands).
10. No configuration key is added, renamed or removed, and no default moves.
11. No new task, no new timer, no new listener and no new socket.
12. Nothing is promoted: `router.version` stays `0.9.62` and the inventory's
    advertised count stays **zero**.

## 5. Scope

### 5.1 In scope

- The runtime's bind-time recorder for the explicit-bind corroboration class.
- The gate that keeps the recorder silent without the existing opt-in.
- The lane that proves the class is earned in **ordinary** operation, and that
  the explicit bind plus the peer observation plus the confirmed peer test
  reach `Reachable` and publish the direct form.
- The guard that keeps the recorder inside the runtime, keeps the floor
  unchanged and keeps the controlled path untouched.

### 5.2 Out of scope

- The **transport's** reachability model: its signal kinds, its corroboration
  classes, its floors and its state machine are unchanged.
- The **publication** gate, the direct form's shape, the `caps` string and the
  published RouterInfo. This plan changes what the router **knows** about
  itself, not what it publishes.
- The **peer test** itself, Plan 529's handshake observation, and Plan 520's
  publication gate. This plan supplies the **third** class, not the other two.
- The **controlled** activation's narrow recorder.

### 5.3 Out of scope, with the owner named

- **The production SSU2 peer-test driver** in the general three-party sense
  (unsolicited tests, production RouterInfo plumbing, persistent helpers) —
  Plan 528's §5.3 names the successor, and
  `crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs:36`-`:38` names it.
- **The requester side of the SSU2 relay protocol** — Plan 527's §5.3.
- **A public dial scope** — Plan 431 owns it.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

1. `crates/i2pr-runtime/src/ssu2_runtime.rs` — a **private** recorder that
   records `ReachabilitySignal::LocalConfiguredBind` for every family the
   runtime can name its own bound endpoint for, gated on the existing
   `explicit_bind_corroboration` opt-in.
2. `crates/i2pr-runtime/src/ssu2_runtime.rs` — `start()` calls that recorder
   after the sockets are bound and the loop tasks are running, next to the
   existing `SocketsBound` qualification event
   (`crates/i2pr-runtime/src/ssu2_runtime.rs:2110`-`:2115`).
3. `crates/i2pr-runtime/tests/ssu2_controlled_peer_test.rs` — the one
   assertion named in §3.8 moves from `Unknown` to `ObservedUnconfirmed`, and
   its comment names the recorder. Its **no material** assertion is untouched.

Nothing else changes. The daemon, the transport, the netdb and the
configuration surface are **not** edited.

## 7. Design decisions

- **D1 — The recorder lives in the runtime.** The runtime owns the tracker
  and the sockets, and the daemon must not record reachability signals of its
  own (Plan 520's guard rule 10). A daemon-side call would also make the
  class depend on a composition rather than on the bind.
- **D2 — The record is made at bind time.** The fact is true from the moment
  the socket is bound; recording it later would misdate it, and recording it
  lazily inside a getter would put a side effect in a read.
- **D3 — The gate is the existing opt-in.** No new key, no moved default. A
  runtime crate default of `false` keeps the crate's conservative posture,
  and the daemon's existing mapping (`router_i2np.rs:1539`) is what turns the
  class on for the router.
- **D4 — A family is recorded only when the runtime can name its own bound
  endpoint.** The runtime already has that predicate
  (`bound_endpoint`), and the publication path already refuses an unspecified
  or port-zero bound address
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3077`). Reusing the predicate
  keeps one notion of "a bound address this router owns" in the tree instead
  of a second, hidden rule inside the recorder.
- **D5 — The recorder records only the family.** The signal is
  family-scoped by construction, so no address can leak into the tracker or
  into a snapshot.
- **D6 — The controlled recorder stays.** It is narrower — it refuses a
  non-loopback bind — and it owns a fail-closed refusal the controlled
  activation depends on
  (`crates/i2pr-daemon/src/floodfill.rs:648` maps the refusal to
  `BindFailed`). Removing it would rewrite Plan 283's foundation and weaken
  the controlled path's refusal. After this plan it is *redundant* for the
  class; that is a cost worth paying for an unchanged controlled path.
- **D7 — One class is one class.** A duplicate record of the same class
  cannot raise the corroboration count, because the count is over distinct
  classes (`supporting_classes`). A restart therefore cannot inflate the
  claim, and the bounded queue evicts the oldest
  (`crates/i2pr-transport/src/reachability.rs:466`-`:467`).
- **D8 — The floor does not move.** The plan supplies the **third** class; it
  does not lower the floor, shorten the contradiction rule, or touch the
  publication gate.

## 8. Work packages

- **WP1 — the runtime's recorder.** The private bind-time recorder and the
  `start()` call.
- **WP2 — the one amended assertion.** The controlled peer-test integration
  test's floor label, and nothing else.
- **WP3 — the lane and the guard.** The lane proves the class is earned in
  ordinary operation and that the triple reaches `Reachable`; the guard keeps
  the recorder inside the runtime and the floor unchanged.
- **WP4 — the documentation, the inventory and the amendment log.**

## 9. Failure, cancellation, restart and contention semantics

- **Failure.** A bind that does not happen records nothing: the recorder runs
  after the sockets are bound, so a failed bind leaves the tracker untouched.
  A family whose bound endpoint cannot be named — a wildcard or a port-zero
  bind — records nothing for that family.
- **Cancellation.** The plan adds **no task**. The record is made
  synchronously inside `start()`.
- **Restart.** A restart re-records the same class. Because the corroboration
  count is over **distinct** classes, the re-record cannot inflate the claim;
  the bounded queue evicts the oldest signal, so the tracker stays bounded.
- **Contention.** The recorder takes the runtime's existing state lock, in the
  same order as every other recorder (`sockets`, released, then `state`), and
  introduces **no new lock**. It runs before any loop task is spawned, so it
  cannot race the loops.

## 10. Compatibility and migration

- **No wire format change.** Nothing on the wire moves.
- **No configuration migration.** No key is added, renamed or removed, and no
  default moves. The daemon's existing `explicit_bind_corroboration: true`
  mapping is the whole of the opt-in.
- **A reading change for the router.** A started router now holds one
  corroborating class — its own bind — instead of none. The state moves from
  `Unknown` to `ObservedUnconfirmed`, which is still far below the floor: no
  direct address is published, and the publication material is still refused.
- **A correction to one closed plan's test.** The controlled peer-test
  integration test's floor label moves, and its **no material** claim stands.
  The correction lives in this plan's amendment log and closure record; the
  closed records of Plans 283, 520 and 526 are **not** rewritten.

## 11. Required tests

### 11.1 The lane

- the **ordinary** bind records the explicit-bind class: one corroborating
  class, and a state below the floor;
- the recorder is **silent** without the opt-in: no class, and the state stays
  `Unknown`;
- the bind and the peer observation yield **exactly** the floor:
  `CandidateReachable`, and never `Reachable`;
- the bind, the peer observation and the confirmed peer test together reach
  `Reachable`, and the direct publication material is produced;
- a **wildcard** or port-zero bind records nothing;
- the **corroboration** floor is unchanged;
- the transport's reachability model is untouched.

### 11.2 The guard

- the recorder exists in the **runtime**, and `start()` calls it;
- the recorder is gated on the existing `explicit_bind_corroboration` opt-in;
- the transport's signal kinds, corroboration classes and floors are
  unchanged;
- the configuration surface gains **no** key and the daemon's existing mapping
  is unchanged;
- the controlled activation's narrow recorder and its call site are unchanged;
- the daemon records no reachability signal of its own;
- no new task, timer, listener or socket.

Every guard rule needs an inverted probe that fires it, and every rule fails
closed when the item it names is absent.

### 11.3 The amended assertion

`crates/i2pr-runtime/tests/ssu2_controlled_peer_test.rs` — the floor label
moves from `Unknown` to `ObservedUnconfirmed` and the comment names the
recorder. The **no material** assertion below it is untouched.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --test ssu2_explicit_bind_corroboration -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_explicit_bind_corroboration_boundary -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor from `AGENTS.md`.

## 13. Documentation updates

- `specs/support.toml` — the new surface, `experimental` and
  `advertised = false`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the plan's gates.
- `specs/protocols/09-ssu2.md` — the plan's scope note.
- `docs/architecture/i2pr-runtime.md` and `docs/architecture/i2pr-daemon.md` —
  one section each. The transport's deep-dive is **not** edited, because the
  transport's model does not move.

## 14. Acceptance criteria and stop conditions

The plan passes when:

1. the **ordinary** bind records the explicit-bind class, and the recorder is
   silent without the existing opt-in;
2. the bind and the peer observation yield exactly `CandidateReachable`, and
   the confirmed peer test supplies the third class;
3. the corroboration floor and the contradiction floor are unchanged;
4. the transport's reachability model is untouched;
5. no configuration key is added and no default moves;
6. the controlled activation's narrow recorder and its call site are
   byte-identical, and the daemon records no reachability signal of its own;
7. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, and every rule fails closed on an absent item;
8. the routine floor is green on the closing tree;
9. `specs/support.toml` records the surface as `experimental` and
   `advertised = false`, the advertised count stays **zero**,
   `router.version` stays `0.9.62`, `Cargo.toml` and `Cargo.lock` stay
   byte-identical, and no boundary script is weakened.

**Stop conditions.** The plan stops and is re-scoped, rather than widened,
if:

- the recorder turns out to need a change to the transport's reachability
  **model**;
- the gate turns out to need a new configuration key or a second policy;
- the delivery turns out to need a new task, a new timer or a new listener;
- the plan turns out to need the production peer-test driver, or the
  requester side, after all.

**Known limitations, recorded not hidden.**

- The runtime's publication path is still **loopback-only**
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3106`), so the direct form the
  plan makes reachable is a **controlled-loopback** claim. The plan supplies
  the missing class; it does not widen the publication profile.
- The peer test remains behind the **`ssu2.peer_test`** opt-in, which is off
  by default, so the third class is earned only by a router whose operator
  asked for it.
- The **production SSU2 peer-test driver** stays unbuilt, the **requester
  side** stays unwired, and the **stock-peer** half stays blocked on Plan
  493's authorized two-host topology.
- The class is recorded at bind time and **expires** with the observation TTL
  (`DEFAULT_OBSERVATION_TTL`, 30 minutes). A router that binds once and never
  observes a peer loses its evidence, and the router's own bind is not
  re-recorded on a timer — this plan adds no timer. A router that keeps its
  sessions re-records nothing, so a long-lived router that never observes a
  peer will see its own class lapse. This is recorded, not hidden, and the
  **re-record** is a follow-up, not part of this plan.

## 15. Closure evidence required

- The commits that carry each work package.
- A requirement-to-evidence matrix: every acceptance criterion above, the
  file and line that satisfies it, and the test that proves it.
- The exact commands run, with their outcomes.
- The invariant, failure, migration and security reviews of §4, §9 and §10.
- The negative controls: every mutation applied, the test it broke, and the
  proof that no file on disk was written.
- The findings by severity, each either fixed or recorded with its owner.
- The correction to the controlled peer-test integration test's floor label,
  and the correction to Plan 520's finding M1 and Plan 526's finding 3, in
  this plan's closure record and in the registry row — never in the closed
  record of the plan that made the claim.
- The roadmap disposition and the unblock audit.

## 16. Handoff notes

- The plan's subject is the router's **own** bound address: it must record
  that fact as evidence, in ordinary operation, so the third corroboration
  class can be earned.
- The plan adds **no** capability. It supplies the missing **recorder** for a
  class the transport already counts and the daemon already opts into.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **requester side**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer**
  half all stay out of scope, with their owners named.

## 17. Amendment log

### 17.1 — 2026-11-10, registration

The plan is registered as **530**, the next free number. §3 records the
evidence as it stood at registration: the corroboration floor's demand for
three distinct classes, the four corroborating classes, the single
controlled-only recorder of class 0, the two classes that do have ordinary
producers, the direct form's gate, the daemon's existing opt-in, and the one
existing assertion the recorder moves.

§3 is evidence and is **not** rewritten. Any later correction is added here
as an amendment, additively.
