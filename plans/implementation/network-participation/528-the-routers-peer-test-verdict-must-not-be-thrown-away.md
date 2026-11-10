# Plan 528 — the router's peer-test verdict must not be thrown away

## 1. Objective

The tree runs a **complete** three-party SSU2 peer test whose verdict carries
exactly what the third party observed — and then throws away every verdict
except "the observation matched the router's own bound address and two peers
corroborated". A **disagreement** becomes `ExchangeNotConfirmed`, an **error**,
not a **fact**.

The specification's peer-test result table has four outcomes — UNKNOWN, OK,
FIREWALLED, SYMNAT — and the pinned reference concludes **FIREWALLED from a
single peer test**. The tree's `ControlledPeerTestOutcome` has exactly **one**
variant, and it is `Confirmed`.

So the router's reachability can only ever be **confirmed**. It can never be
**contradicted**, and the product's firewalled gate — which Plan 527 fixed to
read the one tracker — can never fire.

This plan makes the verdict a **fact**: a disagreement is recorded as a
contradiction against the router's own bound address, and the transport's
state machine is corrected so that one contradicting fact is enough to take
the router off `ObservedUnconfirmed`, exactly as the reference's own
single-test rule does.

## 2. Why ready

- The transport already computes and types the verdicts: `PeerTestOutcome`
  carries `DirectReachabilityConfirmed`, `AddressMismatch`, `FirewalledLikely`,
  `Inconclusive` and `Rejected`
  (`crates/i2pr-transport-ssu2/src/peer_test.rs:111`-`:147`).
- The runtime already records **every** typed outcome into the one tracker
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3926`), and Plan 527 gave the
  runtime the comparison seam that turns an observed address into an agreement
  or a contradiction (`:2798`).
- The controlled exchange already computes the observation and compares it
  (`crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs:753`-`:765`) — and
  discards the disagreement.
- The transport's own reachability model already knows the difference between
  a corroboration and a contradiction
  (`crates/i2pr-transport/src/reachability.rs:163`-`:203`).

So the tree has every fact it needs. Each has exactly one missing production
caller, and the missing caller is a single `match` arm.

## 3. Current implementation evidence

### 3.1 The verdict is thrown away

`crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs:753`-`:766`:

```rust
    match outcome {
        PeerTestOutcome::DirectReachabilityConfirmed {
            family,
            observed: confirmed,
            evidence_peers,
        } if confirmed.socket_addr() == params.alice_addr && evidence_peers == 2 => {
            Ok(ControlledPeerTestOutcome::Confirmed {
                family,
                observed: confirmed.socket_addr(),
                evidence_peers,
            })
        }
        _ => Err(ControlledPeerTestError::ExchangeNotConfirmed),
    }
```

The guard arm requires the observation to **equal the router's bound
address**. Every other outcome — a **mismatch**, a **firewalled** verdict, an
**inconclusive** test, a **rejection** — collapses into one error. The
observation that *disagrees* is the fact the router most needs, and it is
discarded at the last step.

### 3.2 The outcome enum has one variant

`crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs:73`-`:87` declares
`ControlledPeerTestOutcome` with exactly one variant, `Confirmed`. There is
nowhere for a mismatch or a firewalled verdict to go.

### 3.3 The state machine cannot express the reference's verdict

`crates/i2pr-transport/src/reachability.rs:571`:

```rust
        if self.contradicting_classes() >= self.policy.min_corroboration {
```

`min_corroboration` is **2** (`crates/i2pr-transport/src/reachability.rs:302`;
the runtime uses the same floor at
`crates/i2pr-runtime/src/ssu2_runtime.rs:1798`-`:1804`). The policy's floor is
therefore a **corroboration** floor applied to **contradictions**.

The vocabulary has exactly **two** contradicting classes
(`crates/i2pr-transport/src/reachability.rs:180`-`:189`,
`:200`-`:201`):

| Class | Signal | Contradicts | Field producer |
| --- | --- | --- | --- |
| 3 | `PeerTestResult{AddressMismatch}` | yes | the controlled exchange, and the comparison — **the mismatch is discarded** |
| 3 | `PeerTestResult{FirewalledLikely}` | yes | `Ssu2PeerRelayService::mark_firewalled` (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:927`) — **no caller** |
| 4 | `RelayFirewalledSignal` | yes | `Ssu2RuntimeService::note_relay_firewalled` (`crates/i2pr-runtime/src/ssu2_runtime.rs:3196`) — **no caller** |

Every peer-test verdict is filed under **one** class, so two different
peer-test verdicts corroborate and contradict **once** between them — the
transport's own test says so in as many words
(`crates/i2pr-transport/src/reachability.rs:845`): "Same class twice still
contradicts once; add the relay class."

⇒ **In a live router the tracker can hold at most one contradicting class,
the contradiction floor is 2, and the state never leaves `ObservedUnconfirmed`
on a contradiction** (`crates/i2pr-transport/src/reachability.rs:589`-`:591`).
`Unreachable` and `Firewalled` are **unreachable states**, and the daemon's
firewalled gate (`crates/i2pr-daemon/src/service_product.rs:4796`) is
**permanently false**.

### 3.4 The reference concludes FIREWALLED from one peer test

The SSU2 specification's peer-test result table — "based on whether messages
4, 5, and 7 are received" — has the row:

| 4 | 5 | 7 | Result |
| --- | --- | --- | --- |
| y | n | n | **FIREWALLED** (unless currently SYMNAT) |
| y | n | y | **FIREWALLED** or SYMNAT |

and the specification adds that "Post-processing or additional logic to
confirm state transitions by requiring the same results on two or more peer
tests is **recommended**" — recommended, not required.

The pinned i2pd 2.61.0 does exactly that: one peer test sets the firewalled
status (`libi2pd/SSU2Session.cpp:2703`-`:2705`):

```cpp
else if (GetRouterStatus () != eRouterStatusFirewalled && addr->IsPeerTesting ())
{
    SetRouterStatus (eRouterStatusFirewalled);
```

and the reference then asks an introducer for a relay tag while firewalled
(`libi2pd/SSU2Session.cpp:753`).

⇒ **The tree is stricter than the reference on the negative side and weaker
on the positive side at the same time.** The corroboration floor of 2 is
right — the router still needs three distinct supporting classes for
`Reachable` (`crates/i2pr-transport/src/reachability.rs:594`) — but the same
floor applied to contradictions demands evidence the tree cannot produce.

### 3.5 The live peer test cannot contradict

The daemon's live peer test is the **I2NP**-level one, and an I2NP peer-test
message carries "a message identifier and a timestamp"
(`crates/i2pr-daemon/src/peer_test.rs:4`) and **no address**
(`crates/i2pr-daemon/src/router_i2np.rs:155`-`:157`). It can therefore only
ever produce `Confirmed`
(`crates/i2pr-daemon/src/service_product.rs:4628`-`:4641`).

The **SSU2** peer test — the reference's own route — is implemented in full in
the transport, and the runtime drives it end to end over real UDP in the
**controlled** path. Its module doc names the gap in as many words
(`crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs:37`): "The general
production peer-test driver (unsolicited tests, production RouterInfo
plumbing, persistent helpers) is explicitly out of scope and stays unbuilt."

⇒ The **controlled** exchange is the tree's only complete SSU2 peer test, and
it is where the verdict is thrown away.

### 3.6 The requester side is not the answer

The relay-firewalled indication would be the other contradicting class, but
its producer does not exist and has no trigger: `start_relay_request` and
`on_relay_response` (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:481`, `:497`)
have no production caller, the runtime exposes no requester entry point, and
the daemon has no component that would originate a relay request — every peer
selector in the tree picks a peer the router dials **directly**. The pinned
reference derives its firewalled verdict from the **peer test**, not from a
relay response, and the relay response is its mechanism for learning a
target's endpoint (`libi2pd/SSU2Session.cpp:2428`-`:2459`).

⇒ The peer test is the route. The requester side stays out of scope.

## 4. Invariants

1. The router's reachability has **one** owner — the runtime — and the
   product is its only reader. Plan 527's invariant holds unchanged.
2. A disagreement between an observed address and the router's **own bound
   socket** is a **fact**, never an error and never a silent omission.
3. The comparison is made against the **bound** socket, never a configured
   value, and a family with no bound socket refuses the comparison.
4. One observation is one fact: a contradiction never also corroborates.
5. The **corroboration** floor is unchanged. `Reachable` still needs three
   distinct supporting classes. The negative side is corrected, not the
   positive side.
6. The contradiction floor is **one** distinct contradicting class, which is
   the reference's own single-test rule.
7. The firewalled verdict is **first class**: a peer test that concludes the
   router is not directly reachable puts the router in `Firewalled`, exactly
   as the relay-firewalled indication does.
8. A **neutral** verdict — inconclusive or rejected — moves nothing. It is
   recorded and counted, and the state is unchanged.
9. The transport's signal **vocabulary**, its corroboration classes and its
   supporting floor are untouched.
10. No configuration key is added and no default moves.
11. No wire format is added or changed. The verdict rides the blocks the
    tree already sends.
12. Nothing is promoted: `router.version` stays `0.9.62` and the
    inventory's advertised count stays **zero**.

## 5. Scope

### 5.1 In scope

- **The transport**: the contradiction floor is separated from the
  corroboration floor, and the firewalled verdict is first class. The
  specification's peer-test result table is transcribed into the tree as an
  **executable, pinned** table.
- **The runtime**: the controlled peer test's outcome enum grows the missing
  verdicts, a disagreement is a **fact** rather than an error, and the fact
  reaches the **one** tracker through the runtime's own comparison and
  recorder.
- **The daemon**: the firewalled gate reads a state that can now be
  `Firewalled`. The gate's own refusal reasons are unchanged.
- The lane, the guard, the documentation and the support inventory.

### 5.2 Out of scope

- **The production peer-test driver.** The tree's Alice role requires the
  helper's identity up front
  (`crates/i2pr-transport-ssu2/src/peer_test.rs:662`-`:669`), while the
  specification has Bob select the helper. That choreography is a separate
  question with its own plan of record; the **recorder** the driver would
  call is built here.
- **The requester side**, exactly as in Plan 527.
- **The transport's signal vocabulary, its corroboration classes, its
  supporting floor and its policy default.**
- **The `B` advertisement**, the **`caps=f` advertisement** and the
  **stock-peer** half, exactly as in Plans 524, 525 and 526.
- **The live I2NP-level peer test**, which carries no address and is
  therefore confirm-only by construction.

### 5.3 Out of scope, with the owner named

- **The production SSU2 peer-test driver.** The tree runs the full exchange
  only in the controlled profile, and the module doc says so
  (`crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs:37`). **Owner:** a
  successor plan in this subsystem.
- **A public dial scope.** Plan 431 owns it, and the router's SSU2 dial
  target is loopback-only, so a public deployment's peer population is not
  dialable at all.
- **The relay-firewalled indication's producer.** The requester side is
  unwired and triggerless in this tree. **Owner:** a successor plan.

## 6. Required production changes

1. `crates/i2pr-transport/src/reachability.rs` — the policy gains a
   **contradiction floor** separate from the corroboration floor, and the
   firewalled verdict becomes first class. The `Reachable` and
   `CandidateReachable` thresholds are **unchanged**.
2. `crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs` — the outcome enum
   grows the missing verdicts; a **mismatch** is returned as a fact; the
   driver's error arm keeps only the genuinely **failed** cases.
3. `crates/i2pr-runtime/src/ssu2_runtime.rs` — the runtime's **verdict**
   path: the observed address is compared with the router's own bound
   address and the verdict reaches the one tracker.
4. `crates/i2pr-daemon/src/service_product.rs` — the gate reads the state
   that can now be `Firewalled`. The gate's **refusal reasons** are
   unchanged.

## 7. Design decisions

- **D1 — The contradiction floor is separate.** The policy carries
  `min_contradiction` beside `min_corroboration`. The corroboration floor
  stays 2, so `Reachable` still needs three distinct supporting classes and
  the **positive** claim is not weakened at all. The contradiction floor is
  1, which is the reference's own single-test rule.
- **D2 — The firewalled verdict is first class.** A peer test that concludes
  the router is not directly reachable puts the router in `Firewalled`,
  exactly as the relay-firewalled indication already does. A bare address
  disagreement puts it in `Unreachable`. The two are different facts and the
  reference's result table distinguishes them.
- **D3 — The specification's result table is transcribed.** The table is
  written into the tree as data and pinned by a test, so the tree's rule is
  checkable against the specification rather than only against prose.
- **D4 — The disagreement is compared against the bound socket.** The
  comparison reuses Plan 527's seam, so the bound socket, the refusal on no
  bound socket, and the "a contradiction never also corroborates" rule are
  the **same** rules, not a second copy.
- **D5 — Neutral verdicts move nothing.** Inconclusive and rejected are
  recorded and counted; the state is unchanged.
- **D6 — The verdict is not an error.** A mismatch is a **result**, and the
  driver's error type keeps only the genuinely failed cases. A caller can
  therefore distinguish "the test failed" from "the test succeeded and the
  router is behind NAT".
- **D7 — No wire change.** The verdict rides the blocks the tree already
  sends.
- **D8 — The delivery is confined to the controlled loopback profile**, as
  Plan 526's advertisement is: the comparison is between two loopback
  addresses, so the contradiction is a **qualification** claim.

## 8. Work packages

- **WP1 — the transport's model.** The policy, the contradiction floor, the
  firewalled verdict, the transcribed result table, and the transport's own
  tests.
- **WP2 — the runtime's verdict path.** The outcome enum, the fact-not-error
  arm, the comparison, the recording, and the evidence accessor.
- **WP3 — the daemon's gate.** The state the gate reads, and the unchanged
  refusal reasons.
- **WP4 — the lane, the guard, the documentation and the inventory.**

## 9. Failure, cancellation, restart and contention semantics

- **Failure.** A mismatch is a fact, not a failure. A genuinely failed
  exchange — a refused call, a missed deadline, a cancelled step — keeps
  its typed error and records nothing.
- **Cancellation.** The plan adds **no task**. The verdict is computed on
  the path that already resolves the exchange.
- **Restart.** The router's reachability is recomputed from live evidence,
  so a restart with no verdict is `Unknown` and asks nobody.
- **Contention.** The plan adds no queue, no task and no lock of its own.
  The comparison happens under the runtime's existing state lock, and the
  lock **order** is unchanged: the only nesting is `state` → `sockets`, and
  no code path holds `sockets` while taking `state`.

## 10. Compatibility and migration review

- **No wire change.** No block, option or codec changes.
- **A reading change for the router.** The reachability state can now reach
  `Unreachable` and `Firewalled` from the evidence the reference produces. A
  router that was `CandidateReachable` can become `Firewalled`, and it will
  then ask an introducer for a relay tag. That is the intended behaviour.
- **A widened enum.** `ControlledPeerTestOutcome` gains variants. The
  enum is internal to the runtime and the daemon's controlled path, and the
  daemon's gate is unchanged.
- **No configuration migration.** No key is added, renamed or removed.

## 11. Required tests

### 11.1 The lane

- a verdict that **disagrees** with the router's bound address is a
  contradiction, not an error;
- the disagreement reaches the **one** tracker;
- a contradiction alone takes the router off `ObservedUnconfirmed`;
- a **firewalled** verdict puts the router in `Firewalled`;
- the daemon's firewalled gate **fires** on the state the tracker really
  holds, with no injected flag;
- an **agreement** corroborates and never contradicts;
- a **neutral** verdict moves nothing;
- a failed exchange records nothing and is still an error;
- a router with no bound socket refuses the comparison.

### 11.2 The guard

- the corroboration floor is **unchanged**;
- the contradiction floor is **separate**;
- the firewalled verdict is first class;
- the neutral verdicts move nothing;
- the specification's result table is transcribed and pinned;
- the mismatch arm is a fact and not an error;
- the comparison is against the bound socket;
- the gate stays fail-closed;
- no new key and no new infrastructure;
- the transport's vocabulary and corroboration classes are untouched.

Every guard rule needs an inverted probe that fires it, and every rule fails
closed when the item it names is absent.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --test ssu2_peer_test_verdict -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_peer_test_verdict_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets --no-fail-fast -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

## 13. Documentation updates

- `specs/support.toml` — the new surface, `experimental` and
  `advertised = false`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the plan's gates.
- `specs/protocols/09-ssu2.md` — the plan's scope note.
- `docs/architecture/i2pr-transport.md`, `docs/architecture/i2pr-runtime.md`
  and `docs/architecture/i2pr-daemon.md` — one section each.

## 14. Acceptance criteria and stop conditions

The plan passes when:

1. a disagreement between the observed address and the router's **own bound
   socket** is a **fact**, not an error;
2. the fact reaches the **one** tracker and the product's state;
3. the reachability state can reach `Firewalled` from the evidence the
   reference's own single-test rule produces;
4. the daemon's firewalled gate fires on it **without any injected flag**;
5. the **corroboration** floor is **unchanged** and `Reachable` still needs
   three distinct supporting classes;
6. the specification's peer-test result table is transcribed into the tree
   and pinned;
7. a neutral verdict moves nothing;
8. the transport's signal vocabulary and corroboration classes are
   untouched;
9. no configuration key is added and no default moves;
10. the lane and the guard are green, every guard rule has an inverted probe
    that fires it, and every rule fails closed on an absent item;
11. the routine floor is green on the closing tree;
12. `specs/support.toml` records the surface as `experimental` and
    `advertised = false`, the advertised count stays **zero**,
    `router.version` stays `0.9.62`, `Cargo.toml` and `Cargo.lock` stay
    byte-identical, and no boundary script is weakened.

**Stop conditions.** The plan stops and is re-scoped, rather than widened, if:

- the contradiction floor turns out to need a change to the **supporting**
  floor, or to the corroboration classes;
- the firewalled verdict turns out to need a new signal kind, a new
  corroboration class, or a change to the supporting threshold;
- the comparison turns out to need a configuration key or a second policy;
- the verdict turns out to need a new task, a new timer or a new listener;
- the plan turns out to need the production peer-test driver, or the
  requester side, after all.

**Known limitations, recorded not hidden.**

- The delivery is confined to the **controlled loopback profile**: the
  comparison is between two loopback addresses, so the contradiction is a
  **qualification** claim, exactly as Plan 526's advertisement is.
- The **production peer-test driver** stays unbuilt, and the tree's live
  router therefore still cannot be contradicted in the field. The recorder
  and the model it needs are built here; the driver is the successor's
  subject, and its choreography question is named in §5.3.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology.

## 15. Closure evidence required

- The commits that carry each work package.
- A requirement-to-evidence matrix: every acceptance criterion above, the
  file and line that satisfies it, and the test that proves it.
- The exact commands run, with their outcomes.
- The invariant, failure, migration and security reviews of §4, §9 and §10.
- The negative controls: every mutation applied, the test it broke, and the
  proof that no file on disk was written.
- The findings by severity, each either fixed or recorded with its owner.
- The correction to the claim that the transport's reachability model is
  untouched, in this plan's closure record and in the registry row — never
  in the closed record of the plan that made it.
- The roadmap disposition and the unblock audit.

## 16. Handoff notes

- The plan's subject is the router's **own** peer-test verdict: a verdict
  that disagrees with the router's bound address is a **fact**, not an
  error.
- The plan corrects the **negative** side of the reachability model and
  leaves the **positive** side exactly as it was.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **requester side**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer**
  half all stay out of scope, with their owners named.

## 17. Amendment log

### 17.1 — 2026-11-10, registration

The plan is registered as **528**, the next free number. §3 records the
evidence as it stood at registration: the thrown-away verdict, the
one-variant outcome enum, the shared floor and the single peer-test
corroboration class, the reference's single-test firewalled rule, the
confirm-only live peer test, and the requester side that is not the answer.

§3 is evidence and is **not** rewritten. Any later correction is added here
as an amendment, additively.
