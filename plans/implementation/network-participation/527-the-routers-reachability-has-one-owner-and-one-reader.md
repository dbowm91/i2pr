# Plan 527 — the router's reachability has one owner and one reader

**Plan of record.** Registered 2026-11-10. The next free number was 527.
Subsystem: `network-participation`. Status: **registered**.

## 1. Objective

The router's reachability has **two** trackers. The product reads one of them,
and that one only ever receives **confirming** evidence. The other tracker —
the one every **contradicting** signal is written into — is read by nothing.

The consequence is that `ReachabilityState::Firewalled` and
`ReachabilityState::Unreachable` are **unreachable states**: no sequence of
events in the running router can produce them. Plan 524's firewalled gate
therefore reads `false` forever, the router **never** asks a peer for a relay
tag, it never obtains an authenticated introducer, and the introducer set it
publishes stays empty.

This plan gives the router's reachability **one owner and one reader**: the
runtime owns the router's reachability, the product reads it, and every
signal — confirming **or** contradicting — reaches that one tracker.

## 2. Why ready

The tree holds every fact this plan needs, and each fact has exactly one
production caller that is missing.

- The transport's **reachability model** is complete: five typed signal
  classes, a per-character corroboration class, and a state machine that
  distinguishes `CandidateReachable`, `Reachable`, `Firewalled` and
  `Unreachable`
  (`crates/i2pr-transport/src/reachability.rs:119`-`:203`,
  `:564`-`:601`).
- The transport **decodes** the data-phase Address block and **throws the
  endpoint away**: `DecodedBlock::Address(AddressBlock)` carries a validated
  endpoint
  (`crates/i2pr-transport-ssu2/src/block.rs:1387`,
  `crates/i2pr-transport-ssu2/src/block.rs:692`-`:725`), and the session
  discards it
  (`crates/i2pr-transport-ssu2/src/session.rs:1927`-`:1929`).
- The runtime already records the **confirmed** peer test into its own
  tracker
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3024`-`:3066`), and the daemon
  already drives it
  (`crates/i2pr-daemon/src/service_product.rs:4608`-`:4649`).
- The daemon already refuses to ask for a relay tag unless the router is
  firewalled, and counts the refusal by its own reason
  (`crates/i2pr-daemon/src/introducer_request.rs:314`-`:315`).

So the gap is not a missing model, a missing codec or a missing consumer. It
is a **second source of truth**, and a signal that is decoded and discarded.

## 3. Current implementation evidence

### 3.1 The router has two reachability trackers

The **runtime** owns one, inside its service state
(`crates/i2pr-runtime/src/ssu2_runtime.rs:1370`):

```rust
    reachability: ReachabilityTracker,
```

and constructs it from the runtime's own policy
(`crates/i2pr-runtime/src/ssu2_runtime.rs:1805`):

```rust
        let reachability = ReachabilityTracker::new(reachability_policy)
```

The **peer-relay service** owns another
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:139`):

```rust
    reachability: Mutex<ReachabilityTracker>,
```

and constructs it from the **default** policy, in its own constructor
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:170`-`:171`):

```rust
        let reachability =
            ReachabilityTracker::new(ReachabilityPolicy::default()).map_err(|_| "policy")?;
```

They are different objects with different policies. The runtime's tracker is
gated by the operator's `explicit_bind_corroboration`; the peer-relay
service's tracker is always the default.

### 3.2 Every contradicting signal is written into the second tracker

The transport decides which signals contradict reachability
(`crates/i2pr-transport/src/reachability.rs:180`-`:189`):

```rust
    const fn contradicts_reachability(self) -> bool {
        match self {
            Self::PeerTestResult { outcome, .. } => matches!(
                outcome,
                PeerTestOutcomeKind::AddressMismatch | PeerTestOutcomeKind::FirewalledLikely
            ),
            Self::RelayFirewalledSignal { .. } => true,
            _ => false,
        }
    }
```

So exactly three signal shapes contradict: `PeerTestResult{AddressMismatch}`,
`PeerTestResult{FirewalledLikely}` and `RelayFirewalledSignal`. Every one of
them is written **only** into the peer-relay service's own tracker.

The peer-test outcome is mirrored there
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:466`-`:499`):

```rust
    fn mirror_peer_outcome(&self, outcome: Option<PeerTestOutcome>) {
        let Some(outcome) = outcome else {
            return;
        };
        let signal = match outcome {
            PeerTestOutcome::DirectReachabilityConfirmed { family, .. } => {
                ReachabilitySignal::PeerTestResult {
                    family,
                    outcome: PeerTestOutcomeKind::Confirmed,
                }
            }
            PeerTestOutcome::AddressMismatch { family, .. } => ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::AddressMismatch,
            },
            PeerTestOutcome::FirewalledLikely { family } => ReachabilitySignal::PeerTestResult {
                family,
                outcome: PeerTestOutcomeKind::FirewalledLikely,
            },
```

and the relay-success indication is recorded there too
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:505`-`:511`):

```rust
    pub fn note_relay_firewalled(&self, family: AddressFamily) {
        if let Ok(mut reachability) = self.reachability.lock() {
            reachability.record(
                ReachabilitySignal::RelayFirewalledSignal { family },
                Duration::from_secs(1),
            );
        }
```

A search of the whole workspace finds **no other writer** of any
contradicting signal. `RelayFirewalledSignal` is written at exactly one
place (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:508`), and
`AddressMismatch` and `FirewalledLikely` are written at exactly two
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:479`, `:483`).

### 3.3 The runtime's tracker receives only confirming signals

The runtime's tracker is written at four places, and **none** of them
contradicts:

| Signal | Class | Contradicts? | Written at |
| --- | --- | --- | --- |
| `LocalConfiguredBind` | 0 | no | `crates/i2pr-runtime/src/ssu2_runtime.rs:2996` |
| `PeerTestResult{Confirmed}` | 3 | no | `crates/i2pr-runtime/src/ssu2_runtime.rs:3050`, `:3762` |
| `ValidatedPath` | 2 | no | `crates/i2pr-runtime/src/ssu2_runtime.rs:6553` |
| `AuthenticatedPeerObservedExternalAddress` | 1 | no | `crates/i2pr-runtime/src/ssu2_runtime.rs:6692` |

The corroboration classes are fixed
(`crates/i2pr-transport/src/reachability.rs:195`-`:203`):

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

### 3.4 The state machine therefore cannot reach two of its states

The recomputation is fail-closed on contradiction
(`crates/i2pr-transport/src/reachability.rs:564`-`:601`):

```rust
        if self.contradicting_classes() >= self.policy.min_corroboration {
            let firewalled = self.firewalled_classes() >= self.policy.min_corroboration
                || self.signals.iter().any(|entry| {
                    matches!(
                        entry.signal,
                        ReachabilitySignal::RelayFirewalledSignal { .. }
                    )
                });
            self.transition(
                if firewalled {
                    ReachabilityState::Firewalled
                } else {
                    ReachabilityState::Unreachable
                },
                now,
            );
            return;
        }
```

`contradicting_classes()` counts only contradicting signals
(`crates/i2pr-transport/src/reachability.rs:530`-`:542`), and the runtime's
tracker never holds one (§3.3). The count is therefore **always zero**, so
the `Firewalled` and `Unreachable` branches are **dead code in the running
router**, and the states those branches produce are unreachable.

The runtime's tracker can only ever be `Unknown`, `ObservedUnconfirmed`,
`CandidateReachable` or `Reachable`.

### 3.5 The product reads the runtime's tracker, and nothing reads the other one

The daemon's firewalled gate reads the **runtime's** tracker
(`crates/i2pr-daemon/src/service_product.rs:4796`-`:4797`):

```rust
        let firewalled = self.inner.ssu2_handle.service().snapshot().reachability
            == i2pr_transport::ReachabilityState::Firewalled;
```

and `Ssu2RuntimeService::snapshot()` reads the runtime's own tracker and
nothing else (`crates/i2pr-runtime/src/ssu2_runtime.rs:2162`-`:2167`):

```rust
        let reachability = self
            .shared
            .state
            .lock()
            .map(|state| state.reachability.state())
            .unwrap_or(ReachabilityState::Unknown);
```

The peer-relay service's tracker is published in `Ssu2PeerRelaySnapshot`
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1062`-`:1110`), and **no
component of the daemon reads that snapshot**. A search of
`crates/i2pr-daemon/src` finds no reader of `Ssu2PeerRelaySnapshot` at all.

So the tracker that holds every contradicting signal is **written and never
read**, and the tracker the product reads holds only confirming signals and
is **read and never contradicted**. The router's reachability has two sources
of truth, and the product consults the wrong one.

### 3.6 The consequence reaches the whole introducer composition

Plan 524's owner refuses every request unless the router is firewalled
(`crates/i2pr-daemon/src/introducer_request.rs:314`-`:315`):

```rust
        if !firewalled {
            self.counters.requests_refused_not_firewalled += 1;
```

With `firewalled` permanently `false` (§3.5), the router **never** asks a
peer for a relay tag. No relay tag means no **authenticated introducer**, so
the introducer set stays empty, so:

- Plan 525's introducer-only publication has **nothing** to publish;
- Plan 526's `"C"` advertisement rides a role whose **acquisition** half can
  never run in the field; and
- the network-participation roadmap's completion requirement — *"publishes the
  validated introducer set in its RouterInfo address"* — is unreachable for
  every router that is not already directly reachable.

### 3.7 The lane never exercised the producer

Plan 524's lane drives the owner with the flag **injected**
(`crates/i2pr-daemon/tests/ssu2_introducer_request.rs:498`):

```rust
    let decision = owner.decide(&eligible, true, wall_ms());
```

and its companion at `:509` injects `false`. The lane therefore proves that
the **consumer** honours the flag. It never drives the **producer**, because
in the tree of the day there was no producer to drive: Plan 524's closure
record states the invariant as

> "The firewalled state must be earned. The router asks only while its own
> reachability state is `Firewalled`, and that state needs corroborated
> evidence."

(`plans/closure/network-participation/524-status.md`, *Findings by severity*,
item 5.)

The consumer is proven; the producer has no lane and, in the running router,
no path.

### 3.8 The transport decodes the observed address and discards it

The data-phase Address block carries a validated endpoint
(`crates/i2pr-transport-ssu2/src/block.rs:692`-`:725`), and the session
throws it away (`crates/i2pr-transport-ssu2/src/session.rs:1927`-`:1929`):

```rust
                DecodedBlock::Address(_) => {
                    outcome.events.push(SessionEvent::AddressObserved);
                }
```

`SessionEvent::AddressObserved` is a **unit** variant
(`crates/i2pr-transport-ssu2/src/session.rs:347`): the observed endpoint is
decoded, validated and then dropped on the floor. The runtime records the
family-only corroborating signal
(`crates/i2pr-runtime/src/ssu2_runtime.rs:6692`), which is right for a
**corroboration** and useless for a **contradiction**: the router is told
that a peer observed an external address, but never **which** address, so it
cannot compare it with its own bound address and cannot discover that it is
behind a NAT.

### 3.9 The requester side is not wired, and is out of scope

`Ssu2PeerRelayService::start_relay_request`
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:518`) and
`Ssu2PeerRelayService::on_relay_response`
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:534`) — the pair that would
produce the relay-success indication — have **no production caller**. The
relay-success route to "I am firewalled" is the **requester** side of the
SSU2 relay protocol, and the router implements the **introducer** side. §5.2
keeps that out of scope and §5.3 names its owner.

## 4. Invariants that must not regress

1. **The transport's reachability model is untouched.** The five signal
   classes, the corroboration classes, the policy floor and the state machine
   are not changed by this plan.
2. **The runtime owns the router's reachability.** Exactly one tracker
   exists for the router's own reachability, and the runtime owns it.
3. **The product reads that one tracker.** No component reads a second
   tracker, and no second tracker exists.
4. **A confirming signal is still confirming.** The `Confirmed` peer test
   still records, and the publication's corroboration floor is unchanged.
5. **A contradicting signal is never dropped.** A contradicting observation
   is recorded, not swallowed, and it is never written into a peer's profile.
6. **The firewalled gate stays fail-closed.** A router whose reachability is
   `Unknown`, `ObservedUnconfirmed` or `CandidateReachable` still asks
   nobody, and the refusal is still counted by its own reason.
7. **No peer profile is written.** The router's own reachability is never
   recorded into a peer's profile.
8. **The role stays default-off.** `ssu2.introducer_service` keeps its
   default and its parse-time refusals.
9. **No new configuration key.** The switches stay `ssu2.peer_test`,
   `ssu2.introducer_service` and `ssu2.advertise`.
10. **No new task, timer, listener or socket.** The plan adds no
    infrastructure of any kind.
11. **No secret crosses a boundary.** The comparison is between two
    addresses, never between keys.
12. **The wire vocabulary is untouched.** The plan changes no block, no
    option and no codec; it stops discarding a value that is already decoded.

## 5. Scope

### 5.1 In scope

- **The runtime**: the router's reachability has one owner. The peer-relay
  service's second tracker is removed, and the runtime records the typed
  outcome it already receives into the one tracker the product reads.
- **The runtime**: every typed peer-test outcome is recorded, not only the
  confirming one, so a contradicting outcome reaches the product.
- **The transport**: the observed endpoint the Address block already carries
  survives to the event that announces it, so the runtime can compare it
  with the router's own bound address.
- **The runtime**: the comparison is made, and a disagreement is recorded as
  a contradicting signal.
- **The daemon**: the firewalled gate reads a state that can now be
  `Firewalled`, and the gate's own refusal reasons are unchanged.
- The lane, the guard, the documentation and the support inventory.

### 5.2 Out of scope

- **The requester side.** `start_relay_request` and `on_relay_response` —
  the pair that would produce the relay-success indication — stay unwired.
  This plan delivers the *peer-test* route to a contradicting verdict; the
  *relay-success* route is a separate matter with its own plan-of-record.
- **The peer-test protocol's own address comparison.** The transport's
  controlled peer-test table already computes `AddressMismatch` from the
  observed endpoint it is given
  (`crates/i2pr-transport-ssu2/src/peer_test.rs:732`-`:744`). This plan does
  not change that table and does not change the daemon's peer-test message
  format.
- **The `B` advertisement**, the **`caps=f` advertisement** and the
  **stock-peer** half, exactly as in Plans 524, 525 and 526.
- **Any change to the transport's policy, its state machine, its signal
  vocabulary or the daemon's peer-test wire format.**

### 5.3 Out of scope, with the owner named

- **The requester side of the SSU2 relay protocol.** The router implements
  the introducer side; a router that wants to be *relayed for* needs the
  requester side. **Owner:** a successor plan in this subsystem.
- **A publication-capable profile.** The advertisement stays confined to the
  controlled loopback profile. **Owner:** Plan 493's two-family
  qualification and Plan 494's publication.

## 6. Required production changes

1. **`i2pr-transport-ssu2`** — `SessionEvent::AddressObserved` carries the
   observed endpoint the Address block already decoded, instead of discarding
   it. No codec, no block and no option changes.
2. **`i2pr-runtime`** — the peer-relay service no longer owns a
   `ReachabilityTracker`. It returns the typed outcome it already computes,
   and the **runtime** records it.
3. **`i2pr-runtime`** — the runtime records **every** typed peer-test
   outcome into its own tracker, including the contradicting ones.
4. **`i2pr-runtime`** — the runtime compares the observed endpoint with the
   router's **own bound address** for that family, and records a
   contradicting signal when they disagree and a corroborating one when they
   agree.
5. **`i2pr-runtime`** — the relay-success indication, where the peer-relay
   service produced one, is recorded by the runtime into the one tracker, so
   no contradicting signal is dropped.
6. **`i2pr-daemon`** — the firewalled gate is unchanged in shape; it now
   reads a state that can reach `Firewalled`.

## 7. Design decisions the plan must make explicit

**D1 — The router's reachability has one owner.**

Two trackers for one fact is a second source of truth, and the repository's
discipline forbids that. The runtime already owns the fact the product reads;
the peer-relay service's tracker is the intruder. The plan removes it rather
than reconciling the two.

**D2 — The peer-relay service returns; the runtime records.**

The peer-relay service already computes the typed outcome
(`PeerTestOutcome`). It must not keep a second tracker to hold it. The
service returns the typed outcome to its caller, and the **runtime** — which
owns the router's reachability and the wire — records it. This keeps the
repo's standing rule: the runtime owns the fact, the daemon never decides.

**D3 — The runtime records every typed outcome, not only the confirming one.**

`map_controlled_outcome_locked` records **only**
`DirectReachabilityConfirmed`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3744`-`:3750`). Keeping that
selection would preserve the defect: the tracker would still only ever be
confirmed. The plan records the whole typed outcome, so a contradiction is
evidence like any other.

**D4 — The observed endpoint is compared, not merely announced.**

`SessionEvent::AddressObserved` is a unit variant, so the decoded endpoint
is lost. Carrying it changes no wire format: the Address block already
carries it and the session already decodes it. With it, the runtime can
compare the address the peer observed against the router's **own bound
address** — the comparison that distinguishes "directly reachable" from
"behind a NAT".

**D5 — The comparison is made by the runtime, against the bound address.**

The runtime owns the sockets, so it alone knows the router's bound address.
The daemon must not assert one, and the comparison must not be made against a
configured value that the socket may not honour. The runtime looks up the
**bound** socket for the observed family and refuses the comparison when it
has none.

**D6 — The comparison is made once per observation, and only for the
family it concerns.**

A signal is filed under one family. The runtime compares within a single
family and refuses a cross-family comparison, so an IPv4 observation can
never contradict an IPv6 address.

**D7 — The contradiction is recorded as a contradicting signal, and a
disagreement is never a corroboration.**

The transport already distinguishes "contradicts" from "supports". A
disagreement between the observed endpoint and the bound address is a
contradiction. It must not also be recorded as a corroborating class, or one
observation would satisfy both.

**D8 — The plan changes no configuration and no default.**

The switches stay `ssu2.peer_test`, `ssu2.introducer_service` and
`ssu2.advertise`. No key is added and no default moves.

**D9 — The plan adds no task, no timer and no listener.**

The comparison happens on the path that already decodes the observation. No
poll, no periodic sweep, no new owner.

**D10 — The peer-relay service's counters are preserved.**

Removing its tracker must not remove its counts. The service's counters stay
where they are and keep their names; only the tracker moves.

**D11 — The daemon's firewalled gate is not weakened.**

Plan 524's gate stays exactly as it is: the router asks only while its own
reachability state is `Firewalled`, and every refusal is counted by its own
reason. The plan makes the state reachable; it does not relax the gate.

**D12 — The plan is not a promotion.**

The support inventory's new surface is `experimental` with
`advertised = false`, the inventory's advertised count stays **zero**, and
`router.version` stays `0.9.62`. Nothing about the router's public posture
changes.

## 8. Work packages

### WP1 — the transport keeps the observed endpoint

`SessionEvent::AddressObserved` carries the endpoint the Address block
already decoded. The transport's codec, its block set and its wire format
are untouched.

### WP2 — the runtime owns the router's reachability

The peer-relay service's tracker is removed. The runtime records the typed
outcome the service returns.

### WP3 — the runtime records every typed outcome

The selection that kept only the confirming outcome is replaced by the whole
typed outcome, so a contradicting outcome reaches the one tracker.

### WP4 — the runtime compares the observation with the bound address

The runtime looks up its bound socket for the observed family, compares the
observed endpoint with it, and records a contradicting signal on a
disagreement and a corroborating one on an agreement.

### WP5 — the lane, the guard, the documentation and the inventory

The lane drives the **producer**: a real observation moves the product's
state, and Plan 524's gate fires on it. The guard pins the shape of the
plan. The documentation and the support inventory record the new surface.

## 9. Failure, cancellation, restart, and contention semantics

- **Failure.** Every refusal is counted under its own reason and leaves no
  state behind. A poisoned state mutex degrades to "no comparison", never to
  a fabricated agreement.
- **Cancellation.** The plan adds no task. The comparison happens on the
  path that already decodes the observation, and the runtime's material
  builders are synchronous and non-awaiting.
- **Restart.** The router's reachability is recomputed from live evidence,
  so a restart with no observation is `Unknown` and asks nobody. There is no
  persisted reachability claim.
- **Contention.** The plan adds no queue, no task and no lock of its own.
  The comparison happens under the runtime's existing state lock, and the
  peer-relay service's own locks are untouched.

## 10. Compatibility and migration review

- **No wire change.** The Address block already carries the endpoint; the
  plan stops discarding it. The daemon's peer-test message format, the
  transport's blocks and the transport's options are all untouched.
- **A reading change for the router.** The router's reachability can now
  reach `Firewalled` and `Unreachable`. A router that was `Reachable` can
  become `Firewalled`, and it will then ask an introducer for a relay tag.
  That is the intended behaviour.
- **A behaviour change for the daemon.** Plan 524's gate can now fire. The
  gate's shape and its refusal reasons are unchanged.
- **A public API change in the runtime and the transport.** The event
  carries a value it already decoded, the peer-relay service loses a
  tracker, and the runtime gains a comparison. The crates are experimental
  and the lanes are updated with the plan; no external consumer exists.
- **No configuration migration.** No new key, no changed default, no changed
  meaning.
- **No persistence.** The reachability is recomputed from live evidence.

## 11. Required tests

### The one owner

- the peer-relay service owns **no** reachability tracker;
- the runtime owns exactly one tracker for the router's reachability;
- the product's snapshot and the product's network condition read that one
  tracker.

### The typed outcome

- a **confirming** outcome is recorded, and the state corroborates;
- an **`AddressMismatch`** outcome is recorded, and the state contradicts;
- a **`FirewalledLikely`** outcome is recorded, and the state contradicts;
- an **`Inconclusive`** outcome records nothing that contradicts and
  nothing that supports;
- a **`Rejected`** outcome records nothing that contradicts and nothing that
  supports;
- a contradicting outcome never also counts as a corroborating class.

### The comparison

- an observation that equals the router's bound address corroborates;
- an observation that differs from the router's bound address contradicts;
- a cross-family observation is refused rather than compared;
- the comparison is refused when the router has no bound socket for the
  observed family.

### The firewalled gate

- a router whose reachability is `Firewalled` asks **one** eligible peer;
- a router whose reachability is `Unknown`, `ObservedUnconfirmed` or
  `CandidateReachable` asks **nobody**, and the refusal is counted under
  its own reason;
- the gate's own reason strings are unchanged.

### The negative controls

Every guard rule needs an inverted probe that fires it, and every probe
mutates an **in-memory copy** of the source, so no file on disk is written.
Every guard rule must **fail closed** when the item it names is absent.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_reachability_contradiction -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_reachability_contradiction_boundary -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
python3 scripts/check-router-readiness-contract.py
python3 scripts/check-router-readiness-contract.py --self-test
```

## 13. Documentation updates

- `specs/support.toml` — a new surface for the contradiction, `experimental`
  and `advertised = false`.
- `specs/CONFORMANCE.md` — the section for this plan and the inventory counts.
- `specs/protocols/09-ssu2.md` — the reachability scope.
- `specs/QUALIFICATION-GATES.md` — the gates for this plan.
- `docs/architecture/i2pr-transport.md` — the reachability model's owner and
  reader.
- `docs/architecture/i2pr-transport-ssu2.md` — the observed endpoint.
- `docs/architecture/i2pr-runtime.md` — the one tracker and the comparison.
- `docs/architecture/i2pr-daemon.md` — the firewalled gate.

## 14. Acceptance criteria and stop conditions

The plan passes when:

1. the router's reachability has **one** tracker, and the peer-relay service
   owns none;
2. the runtime records **every** typed peer-test outcome, and a contradicting
   outcome reaches the product's state;
3. an observation that differs from the router's own bound address is
   recorded as a contradiction, and an observation that agrees corroborates;
4. the product's reachability state can reach `Firewalled`, and Plan 524's
   gate fires on it without any injected flag;
5. the transport's codec, its blocks and its options are untouched;
6. no configuration key is added and no default moves;
7. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, and every rule fails closed on an absent item;
8. the routine floor is green on the closing tree;
9. `specs/support.toml` records the surface as `experimental` and
   `advertised = false`; the advertised count stays **zero**;
10. `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` are
    byte-identical;
11. no boundary script is weakened.

**Stop conditions.** The plan stops and is re-scoped, rather than widened, if:

- removing the peer-relay service's tracker turns out to need a change to
  the transport's reachability policy or to the state machine;
- carrying the observed endpoint turns out to need a new block, a new option
  or a change to any codec;
- the comparison turns out to need a configuration key or a second policy;
- the comparison turns out to need a new task, a new timer or a new listener;
- the plan turns out to need the requester side after all.

**Known limitations, recorded not hidden.**

- The **requester side** stays unwired (§5.2), so the relay-success route to
  a contradiction is not taken. The peer-test route is the one this plan
  delivers.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology. The plan's evidence is a local lane.
- The advertisement stays confined to the **controlled loopback profile**,
  because `ssu2.advertise` is refused outside it. Under that profile the
  router binds loopback, so the comparison is between two loopback
  addresses and the contradiction is a qualification claim.
- The runtime's **direct** form still needs three corroboration classes,
  and no lane in this tree earns them. This plan does not change that; it
  makes the tracker's **negative** half reachable.

## 15. Closure evidence required

- The commits that carry each work package.
- A requirement-to-evidence matrix: every acceptance criterion above, the
  file and line that satisfies it, and the test that proves it.
- The exact commands run, with their outcomes.
- The invariant, failure, migration and security reviews of §4, §9 and §10.
- The negative controls: every mutation applied, the test it broke, and the
  proof that no file on disk was written.
- The findings by severity, each either fixed or recorded with its owner.
- The correction to Plan 524's closed claim about the firewalled state, in
  this plan's closure record and in the registry row — never in Plan 524's
  own record.
- The roadmap disposition and the unblock audit.

## 16. Handoff notes

- The plan's subject is the router's **own reachability**: one owner, one
  reader, and a state that can be **contradicted**.
- The plan completes the chain the SSU2 introducer composition needs:
  Plan 490 gave the role, Plan 523 the form, Plan 524 the acquisition, Plan
  525 the publication and Plan 526 the advertisement. Without a reachable
  `Firewalled` state, the acquisition half can never run.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **requester side**, the **`B` advertisement**, the **`caps=f`
  advertisement** and the **stock-peer half** all stay out of scope, with
  their owners named.

## 17. Amendment log

### 17.1 — 2026-11-10, registration

The plan is registered as **527**, the next free number. §3 records the
evidence as it stood at registration: the two trackers, the contradicting
signals and where they are written, the product's reader, the state
machine's dead branches, the consequence for Plan 524's gate, the lane that
injected the flag, the decoded-and-discarded endpoint, and the unwired
requester side.

§3 is evidence and is **not** rewritten. Any later correction is added here
as an amendment, additively.
