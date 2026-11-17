# Plan 530 — the router must record its own bound address

**Closure record.** The plan of record is
`plans/implementation/network-participation/530-the-router-must-record-its-own-bound-address.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 530 is **closed**. The router records its own bound address as the
explicit-bind corroboration class, in **ordinary** operation, at **bind
time** — so the third of the three distinct supporting classes the transport's
model demands before `Reachable` can be earned outside the controlled
floodfill activation.

Classification: **corrective** (the class was **counted** but never
**earned**: the daemon already opted in and the transport already counted
class 0, but the only recorder in the tree was called by the controlled
activation, so an ordinary router held at most two classes, its state stalled
at `CandidateReachable`, and it published the firewalled form for ever).
Plan 521 gave the router a production peer test, Plan 529 gave it the peer's
observation of its address; this plan gives it the third class.
**Subsystem:** network-participation. **Closure date:** 2026-11-10.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The defect — a corroboration class with no ordinary producer.** The
transport's reachability model says `Reachable` needs **more than** the
corroboration floor of two, so it needs **three distinct supporting classes**
(`crates/i2pr-transport/src/reachability.rs:66`, `:639`-`:641`):

```rust
        let supporting = self.supporting_classes();
        if supporting > self.policy.min_corroboration {
            self.transition(ReachabilityState::Reachable, now);
```

The transport names four classes that can corroborate
(`crates/i2pr-transport/src/reachability.rs:223`-`:231`): class 0
`LocalConfiguredBind`, class 1
`AuthenticatedPeerObservedExternalAddress`, class 2 `ValidatedPath` and class
3 `PeerTestResult`. Two of them had ordinary producers — Plan 529's handshake
observation (class 1) and Plan 521's peer test (class 3). The third did not.

**Class 0 had exactly one recorder, and it was controlled-only.** The tree
wrote `LocalConfiguredBind` in exactly one place
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3263`), inside
`note_explicit_bind_for_controlled_qualification` (`:3228`), whose own doc
comment says: *"Only the controlled M12 activation composition calls this;
normal operation never does."* The tree's only caller was the controlled
floodfill activation (`crates/i2pr-daemon/src/floodfill.rs:648`).

**The opt-in and the counting were already in place.** The daemon maps
`explicit_bind_corroboration: true`
(`crates/i2pr-daemon/src/router_i2np.rs:1539`), and the runtime turns it into
the transport's `configured_direct_allowed`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:1830`-`:1837`), which is what makes
the transport count class 0 at all
(`crates/i2pr-transport/src/reachability.rs:581`-`:585`). **Only the ordinary
recorder was missing.**

**The consequence.** A router in ordinary operation held at most two classes,
its state stalled at `CandidateReachable`, and it published the firewalled
form for ever: the direct form's gate
(`crates/i2pr-transport-ssu2/src/publication.rs:244`-`:246`) needs
`ReachabilityState::Reachable`.

**What the tree does now.**

- The runtime records the class at **bind time**, because that is when the
  fact becomes true (`note_bound_endpoints_as_explicit_bind`,
  `crates/i2pr-runtime/src/ssu2_runtime.rs:3196`, called from `start()` at
  `:2121`).
- The recorder is **private** and lives in the runtime, which owns the
  tracker and the sockets. The daemon records no reachability signal of its
  own, so Plan 520's guard rule 10 stays green.
- It is gated on the **existing** `explicit_bind_corroboration` opt-in, so no
  configuration key is added and no default moves.
- It records a family only when the runtime can **name** its own bound
  endpoint for it, reusing `bound_endpoint`'s predicate
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:2948`-`:2958`): a wildcard or a
  port-zero bind names no address and therefore records nothing.
- The signal carries only the **family**, never a literal endpoint, so the
  record stays redacted by construction.
- The controlled activation's narrow recorder and its call site are
  **byte-identical**: Plan 283's foundation is not rewritten.
- The transport's reachability **model**, the corroboration floor and the
  contradiction floor are **untouched**, and `Reachable` still needs three
  distinct classes.

**The triple is now earnable.** The bind, the peer's observed external
address and a confirmed peer test are three distinct classes, so the
corroboration floor is above them and the direct form becomes publishable.
The lane proves it over a real session between two real daemon-owned routers
on loopback, ending in the **direct** publication material.

## What the plan did not close

- The **production** SSU2 peer-test driver stays unbuilt. Its choreography
  question is named: the tree's Alice role needs the helper's identity up
  front, while the specification has Bob select the helper.
- The **requester** side of the SSU2 relay protocol stays unwired. Its owner
  is named in Plan 527's §5.3.
- The **`B` advertisement** and the **`caps=f` advertisement** stay out of
  scope, with their owners named.
- The **public dial scope** stays out of scope; Plan 431 owns it.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology.
- The runtime's publication path stays **loopback-only**
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3111`), so the direct form the
  plan makes reachable remains a **controlled-loopback** claim.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `3d3646c5` | the registration: the plan of record, the registry row and the roadmap row |
| `f318b5ae` | the production change: the runtime's bind-time recorder, and the one existing assertion it moves |
| `f1b395ce` | the daemon lane and the daemon guard |
| `2d6e6b33` | the documentation, the inventory and the plan's amendment log |

## Requirement-to-evidence matrix

Every acceptance criterion of the plan's §14, the file and line that
satisfies it, and the test that proves it.

| # | Criterion | Satisfied by | Proven by |
| --- | --- | --- | --- |
| 1 | the **ordinary** bind records the explicit-bind class, and the recorder is silent without the existing opt-in | `crates/i2pr-runtime/src/ssu2_runtime.rs:3196` (the recorder), `:2120` (the call), `:3228` (the controlled recorder, unchanged) | guard rules 1 and 2 `the_recorder_lives_in_the_runtime` and `the_recorder_is_gated`; the lane rows `the_ordinary_bind_records_the_explicit_bind_class` and `the_recorder_is_silent_without_the_opt_in` |
| 2 | the bind and the peer observation yield exactly `CandidateReachable`, and the confirmed peer test supplies the third class | `crates/i2pr-runtime/src/ssu2_runtime.rs:2884` (the observation, Plan 529's), `:3290` (the confirmed peer test, Plan 521's) | the lane rows `the_bind_and_the_peer_observation_yield_exactly_the_floor` and `the_bind_the_observation_and_the_peer_test_reach_reachable` |
| 3 | the corroboration floor and the contradiction floor are unchanged | `crates/i2pr-transport/src/reachability.rs:66` (`MIN_CORROBORATION_FLOOR`, still 2), `:77` (`MIN_CONTRADICTION_FLOOR`, still 1) | guard rule 3 `the_reachability_model_is_untouched`; the lane row `the_corroboration_floor_is_unchanged` |
| 4 | the transport's reachability model is untouched | `crates/i2pr-transport/src/reachability.rs:130` (the five signal kinds), `:223` (the five corroboration classes) | guard rule 3 and its inverted probes `the_guard_reports_a_changed_reachability_model` and `the_guard_reports_a_moved_corroboration_class`; the lane row `the_transports_reachability_model_is_untouched` |
| 5 | no configuration key is added and no default moves | `crates/i2pr-daemon/src/config.rs` is untouched by this plan; `crates/i2pr-daemon/src/router_i2np.rs:1539` keeps its existing mapping | guard rule 4 `no_new_key_and_the_existing_mapping` and its inverted probes `the_guard_reports_a_new_configuration_key` and `the_guard_reports_an_unmapped_opt_in` |
| 6 | the controlled activation's narrow recorder and its call site are byte-identical, and the daemon records no reachability signal of its own | `crates/i2pr-runtime/src/ssu2_runtime.rs:3228` (the narrow recorder), `crates/i2pr-daemon/src/floodfill.rs:648` (its call site) | guard rules 5 and 6 `the_controlled_path_is_unchanged` and `the_daemon_records_no_reachability_signal`; the inverted probe `the_guard_reports_a_changed_controlled_path` |
| 7 | the lane and the guard are green, every guard rule has an inverted probe that fires it, and every rule fails closed on an absent item | `crates/i2pr-daemon/tests/ssu2_explicit_bind_corroboration.rs` (7 rows), `crates/i2pr-daemon/tests/ssu2_explicit_bind_corroboration_boundary.rs` (7 rules, 12 inverted probes, 1 missing-item probe, 20 rows) | the floor's `plan530-explicit-bind-lane` and `plan530-explicit-bind-boundary` steps |
| 8 | the negative control is **non-vacuous** | the four mutations of `/tmp/530-negative.py`, each applied to the real tree and restored byte-identically | the negative-control table below |
| 9 | the routine floor is green on the closing tree | the floor table below | the floor run |
| 10 | `specs/support.toml` records the surface as `experimental` and `advertised = false`; the advertised count stays **zero**; `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` stay byte-identical; no boundary script is weakened | `specs/support.toml`, surface `daemon.explicit-bind-corroboration`; `crates/i2pr-netdb/src/local.rs` (`CONTROLLED_ROUTER_VERSION`); neither manifest is touched; the diff touches **no** script under `scripts/` | `python3 scripts/check-tooling-inventory.py`; the counts are 123 surfaces, 118 experimental, 4 deferred, 1 ready, **0 advertised**; `git status` on both manifests is empty |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS — no warnings |
| `cargo test --locked -p i2pr-transport --all-targets` | PASS — 52 rows and 6 rows |
| `cargo test --locked -p i2pr-runtime --all-targets` | PASS — 133, 3, 19 and 7 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_explicit_bind_corroboration -- --test-threads=1` | PASS — 7 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_explicit_bind_corroboration_boundary -- --test-threads=1` | PASS — 20 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-license-metadata.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests, OK |

## The routine floor

The floor ran on the **closing** tree (`2d6e6b33`), and every step is recorded in
`/tmp/530-floor.log` with its own log under `/tmp/530f-<step>.log`. The floor is
a plain sequence of `step` runs that records each step's exit status; no step is
skipped, no `|| true`, no `continue-on-error` and no filename filtering. The
workspace step runs with `--no-fail-fast`, so every test binary's result is
reported rather than the run stopping at the first failure.

Plan 530's floor is Plan 529's floor plus two steps — the daemon lane and the
daemon guard for the explicit-bind corroboration. **No other step changed, and
no other plan's guard file was touched.**

| `floor-tree-clean` | PASS |
| `fmt` | PASS |
| `check` | PASS |
| `app-siblings` | PASS |
| `app-fixture-static` | PASS |
| `app-fixture-copy` | PASS |
| `app-fixture-rebuild` | PASS |
| `plan522-peer-pool-lane` | PASS |
| `plan522-peer-pool-boundary` | PASS |
| `plan522-peer-pool-unit` | PASS |
| `transport-ssu2-unit` | PASS |
| `runtime-unit` | PASS |
| `netdb-unit` | PASS |
| `plan523-introducer-lane` | PASS |
| `plan523-introducer-boundary` | PASS |
| `plan524-introducer-request-lane` | PASS |
| `plan524-introducer-request-boundary` | PASS |
| `plan520-publication-lane` | PASS |
| `plan520-publication-boundary` | PASS |
| `plan525-introducer-publication-lane` | PASS |
| `plan525-introducer-publication-boundary` | PASS |
| `plan526-introducer-advertisement-lane` | PASS |
| `plan526-introducer-advertisement-boundary` | PASS |
| `plan521-peer-test-role-boundary` | PASS |
| `plan527-reachability-contradiction-lane` | PASS |
| `plan527-reachability-contradiction-boundary` | PASS |
| `plan528-peer-test-verdict-lane` | PASS |
| `plan528-peer-test-verdict-boundary` | PASS |
| `plan529-observed-address-lane` | PASS |
| `plan529-observed-address-boundary` | PASS |
| `plan530-explicit-bind-lane` | PASS |
| `plan530-explicit-bind-boundary` | PASS |
| `test-workspace` | PASS |
| `clippy` | PASS |
| `doc` | PASS |
| `test-doc` | PASS |
| `dependency-direction` | PASS |
| `plan-number-uniqueness` | PASS |
| `adr-number-uniqueness` | PASS |
| `portable-service-tunnel-api` | PASS |
| `portable-service-tunnel-consumer` | PASS |
| `planning-tests` | PASS |
| `runtime-boundaries` | PASS |
| `console-boundaries` | PASS |
| `console-browser-security` | PASS |
| `tooling-inventory` | PASS |
| `license-metadata` | PASS |
| `managed-app-private-client-seams` | PASS |
| `router-readiness-contract` | PASS |
| `router-readiness-self-test` | PASS |
| `core-router-recovery-contract` | PASS |
| `core-router-recovery-self-test` | PASS |
| `plan449-ssu2-preflight` | PASS |
| `plan495-peer-eligibility` | PASS |
| `plan476-gate-binding` | PASS |
| `plan476-gate-binding-self-test` | PASS |
| `managed-app-process-boundary` | PASS |
| `managed-app-process-boundary-self-test` | PASS |
| `managed-app-package-boundary` | PASS |
| `managed-app-package-boundary-self-test` | PASS |
| `managed-app-policy-boundary` | PASS |
| `managed-app-policy-boundary-self-test` | PASS |
| `managed-app-secured-sandbox` | PASS |
| `managed-app-secured-sandbox-self-test` | PASS |
| `service-tunnel-boundaries` | PASS |
| `managed-app-gateway-boundary` | PASS |
| `managed-app-manager-boundary` | PASS |
| `m11-per-epoch-composition` | PASS |
| `service-anonymity-boundaries` | PASS |
| `fixture-manifest` | PASS |
| `ntcp2-vectors` | PASS |
| `ssu2-vectors` | PASS |
| `i2cp-vectors` | PASS |
| `ntcp2-interoperability` | PASS |
| `constrained-host-lane-boundary` | PASS |
| `m11-transit-boundaries` | PASS |
| `m11-transit-qualification-evidence` | PASS |
| `sam-acceptance-evidence` | PASS |
| `ssu2-acceptance-evidence` | PASS |
| `i2cp-acceptance-evidence` | PASS |
| `i2pcontrol-acceptance-evidence` | PASS |
| `els2-type11-transcript-boundary` | PASS |
| `els2-live-lane-evidence` | PASS |
| `encrypted-service-consumer-caller` | PASS |
| `outproxy-request-path` | PASS |
| `outproxy-wire-lane-evidence` | PASS |
| `config-secret-hygiene` | PASS |
| `workflow-validity` | PASS |
| `floodfill-type5-serve` | PASS |
| `service-tunnel-acceptance-evidence` | PASS |
| `exploratory-tunnel-evidence` | PASS |
| `netdb-tunnel-evidence` | PASS |
| `destination-tunnel-evidence` | PASS |
| `streaming-tunnel-evidence` | PASS |
| `m6-mixed-router-acceptance-evidence` | PASS |
| `m12-floodfill-qualification-evidence` | PASS |
| `m12-floodfill-boundaries` | PASS |
| `ntcp2-harness` | PASS |
| `cargo-deny` | PASS |

**FLOOR PASS 99 — FLOOR FAIL 0.**

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be
reported. Every probe mutates an **in-memory copy** of the source, so **no
file on disk is written**. Every rule **fails closed** when the item it
names is absent — the guard reports the missing item rather than passing.

### Plan 530's guard — `crates/i2pr-daemon/tests/ssu2_explicit_bind_corroboration_boundary.rs`

Seven rules, twelve inverted probes, one missing-item probe. All 20 rows
green.

| Rule | What it forbids | Its inverted probe |
| --- | --- | --- |
| 1 | a recorder outside the runtime, a `start()` that never calls it, or a recorder that runs **before** the sockets are bound | `the_guard_reports_a_recorder_outside_the_runtime`, `the_guard_reports_a_start_that_never_calls_the_recorder`, `the_guard_reports_a_recorder_that_runs_before_the_bind` |
| 2 | a recorder that ignores the opt-in, or a transport that always counts the class | `the_guard_reports_an_ungated_recorder`, `the_guard_reports_a_transport_that_counts_an_ungated_class` |
| 3 | a changed reachability model — a moved floor or a renumbered class | `the_guard_reports_a_changed_reachability_model`, `the_guard_reports_a_moved_corroboration_class` |
| 4 | a new configuration key, or an unmapped opt-in | `the_guard_reports_a_new_configuration_key`, `the_guard_reports_an_unmapped_opt_in` |
| 5 | a controlled activation that lost its narrow recorder call | `the_guard_reports_a_changed_controlled_path` |
| 6 | a daemon that records a reachability signal of its own | `the_guard_reports_a_daemon_that_records_its_own_signal` |
| 7 | a recorder that opens a socket or adds infrastructure of its own | `the_guard_reports_a_recorder_that_adds_infrastructure` |

Plus `the_guard_reports_a_missing_named_item`, which proves the
missing-item branch for every rule that names an item.

### The negative control against the **real** tree

The guard's probes are in-memory. The tree was therefore also mutated **on
disk** — one mutation at a time, each restored byte-identically — and the guard
and the lane were re-run:

| Mutation | The guard | The lane |
| --- | --- | --- |
| the gate is removed from the recorder | **FAILED**, as it must | **FAILED**, as it must |
| the recorder is never called | **FAILED**, as it must | **FAILED**, as it must |
| the recorder writes the **path-validation** class | **FAILED**, as it must | green — the lane is class-agnostic there, by design |
| the recorder writes the **peer-test** class | **FAILED**, as it must | **FAILED**, as it must |

The third row is the honest limit of the split, and it is recorded rather than
hidden: the runtime's public reachability view carries a corroboration
**count**, never a class list, because the transport's reachability model is
the transport's own and this plan does not move it. The **guard** is the
authority for the class. The **lane** is the authority for the structure: the
fourth row shows it is not blind — when the recorder's class collides with the
peer test's, the triple collapses to two distinct classes and the lane fires.
The mutated file was restored byte-identically after every mutation, and the
restore is verified by a SHA-256 comparison of the file's bytes.

### The one existing assertion that moved

`crates/i2pr-runtime/tests/ssu2_controlled_peer_test.rs` asserted that a
started, opted-in service held **no** reachability evidence before the
controlled exchange. With the ordinary recorder in place that service holds
**one** corroborating class from bind time, so the state is
`ObservedUnconfirmed` rather than `Unknown`. The assertion's material claim —
**no publication material** — is untouched, and is still true: one class is far
below the floor.

## Acceptance criteria

The plan's §14 lists ten criteria and five stop conditions.

1. the **ordinary** bind records the explicit-bind class, and the recorder is
   silent without the existing opt-in — **met**;
2. the bind and the peer observation yield exactly `CandidateReachable`, and
   the confirmed peer test supplies the third class — **met**;
3. the corroboration floor and the contradiction floor are unchanged —
   **met**;
4. the transport's reachability model is untouched — **met**;
5. no configuration key is added and no default moves — **met**;
6. the controlled activation's narrow recorder and its call site are
   byte-identical, and the daemon records no reachability signal of its own —
   **met**;
7. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, and every rule fails closed on an absent item — **met**;
8. the negative control is **non-vacuous** — **met**;
9. the routine floor is green on the closing tree — **met**;
10. `specs/support.toml` records the surface as `experimental` and
    `advertised = false`; the advertised count stays **zero**;
    `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` stay
    byte-identical; no boundary script is weakened — **met**.

**No stop condition fired.** The recorder did **not** need a change to the
transport's reachability **model**; the gate did **not** need a new
configuration key or a second policy; the delivery did **not** need a new
task, a new timer or a new listener; and the plan did **not** turn out to
need the production peer-test driver or the requester side.

## Invariant review

The plan's §4 lists twelve invariants. Each is reviewed against the closing
tree.

1. **The record is made at bind time, because that is when the fact becomes
   true.** `start()` calls the recorder after the sockets are installed and
   the loop tasks are running (`crates/i2pr-runtime/src/ssu2_runtime.rs:2120`),
   and the guard pins the ordering. — **held**.
2. **A family is recorded only when the runtime can name its own bound
   endpoint for it.** The predicate is `bound_endpoint`'s
   (`:2948`-`:2959`), the one the publication path already uses. The lane's
   `a_wildcard_bind_names_no_address_and_records_nothing` proves a wildcard
   bind records nothing. — **held**.
3. **The record is gated on the existing `explicit_bind_corroboration`
   opt-in.** The recorder refuses without it, and the transport's own
   `configured_direct_allowed` gate still skips the class. — **held**.
4. **The signal carries only the family, never a literal endpoint.**
   `ReachabilitySignal::LocalConfiguredBind { family }` has one field, and it
   is the family. — **held**.
5. **The recorder lives in the runtime.** It is a private method on
   `Ssu2RuntimeService`, and the daemon records no reachability signal of its
   own. — **held**.
6. **The controlled activation's narrow recorder and its call site are
   byte-identical.** `note_explicit_bind_for_controlled_qualification` and
   `crates/i2pr-daemon/src/floodfill.rs:648` are untouched, and the guard
   checks both. — **held**.
7. **The transport's reachability model is untouched.** The five signal kinds,
   the five corroboration classes and the state machine are unchanged. —
   **held**.
8. **The corroboration floor is unchanged.** `MIN_CORROBORATION_FLOOR` is
   still 2, and `Reachable` still needs three distinct supporting classes. —
   **held**.
9. **The contradiction floor is unchanged.** `MIN_CONTRADICTION_FLOOR` is
   still 1, and Plan 528's separation stands. — **held**.
10. **No configuration key is added, renamed or removed, and no default
    moves.** The daemon's configuration module is untouched. — **held**.
11. **No new task, no new timer, no new listener and no new socket.** The
    record is made synchronously inside `start()`, and the guard checks the
    recorder's body for all four. — **held**.
12. **Nothing is promoted.** `router.version` stays `0.9.62` and the
    advertised count stays **zero**. — **held**.

## Failure, cancellation, restart, and contention review

- **Failure.** A bind that does not happen records nothing: the recorder runs
  after the sockets are bound. A family whose bound endpoint cannot be named
  — a wildcard or a port-zero bind — records nothing for that family. A
  poisoned state mutex degrades to "no record", never to a fabricated class.
- **Cancellation.** The plan adds **no task**. The record is made
  synchronously inside `start()`.
- **Restart.** A restart re-records the same class. Because the corroboration
  count is over **distinct** classes, the re-record cannot inflate the claim;
  the bounded queue evicts the oldest signal
  (`crates/i2pr-transport/src/reachability.rs:466`-`:467`), so the tracker
  stays bounded.
- **Contention.** The recorder takes the runtime's existing state lock, in the
  same order as every other recorder, and introduces **no new lock**. It runs
  before any loop task is spawned, so it cannot race the loops.
- **The controlled path's refusal is intact.** The controlled activation
  still maps a failed narrow record to `BindFailed`, and the narrow recorder
  still refuses a non-loopback bind.

## Migration and compatibility review

- **No wire change.** Nothing on the wire moves.
- **No configuration migration.** No key is added, renamed or removed, and no
  default moves. The daemon's existing `explicit_bind_corroboration: true`
  mapping is the whole of the opt-in.
- **A reading change for the router.** A started router now holds one
  corroborating class — its own bind — instead of none. The state moves from
  `Unknown` to `ObservedUnconfirmed`, which is still far below the floor: no
  direct address is published, and the publication material is still refused.
- **No public API change.** The recorder is a private method. The runtime's
  public surface is unchanged.
- **A corrected reading for one closed plan's test.** The controlled
  peer-test integration test's floor label moves, and its **no material**
  claim stands. The correction lives in this record and in the plan's
  amendment log; the closed records of Plans 283, 520 and 526 are **not**
  rewritten.

## Security review

- **No secret is logged.** The record is a **family**, not an address and not
  a payload. The lane and the guard print states, counts and forms only; no
  key, tag, peer hash, signature or payload is ever printed.
- **No one local condition is written into a peer's profile.** The record is
  about **this** router's own bound socket. Nothing is written into any
  peer's record.
- **The record is a local fact, not an assertion.** It is made from the
  runtime's own socket, and it cannot be asserted by a caller: the recorder is
  private and there is no public "record reachability" entry point.
- **A peer cannot inflate the router's claim.** The corroboration count is
  over **distinct** classes, so a peer repeating one observation — or a peer
  test repeating — cannot raise the count. The triple needs three distinct
  kinds of fact, not three signals.
- **The wildcard case is refused.** A bind that names no address records
  nothing, so the router never claims an address it cannot name.
- **Fail closed throughout.** A failed bind records nothing, an unnamed
  family records nothing, a poisoned lock degrades to "no record", and the
  guard's absent-item case is a report.

## Limitations

- The runtime's publication path stays **loopback-only**
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3111`), so the direct form the
  plan makes reachable is a **controlled-loopback** claim, not a
  public-internet one.
- The peer test stays behind the **`ssu2.peer_test`** opt-in, which is off by
  default, so the third class is earned only by a router whose operator asked
  for it.
- The **production** SSU2 peer-test driver stays unbuilt, so the live router
  still cannot earn the peer-test class in the field.
- The **requester side** stays unwired, so the relay-success route is not
  taken in the field.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology. The plan's evidence is a local lane.
- **The class expires with the observation TTL** (30 minutes) and the plan
  adds no timer, so a router that binds once and never observes a peer loses
  its evidence. This is recorded, not hidden, and the **re-record** is a
  follow-up, not part of this plan.
- The **lane is class-agnostic** by design: the runtime's public reachability
  view carries a count and never a class list. The guard is the authority for
  the class, and the negative-control table above is the honest limit of the
  split.

## Findings by severity

### Medium — none open

### Low — recorded, not fixed

1. **The class lapses with the observation TTL.** The record is made once, at
   bind time, and the tracker expires observations after
   `DEFAULT_OBSERVATION_TTL`. A long-lived router that never observes a peer
   therefore loses its own class, and the plan adds no timer to re-record it.
   **Owner:** the successor that earns the direct form in the field.
   **Not this plan's scope** — the plan adds no timer by design, and its §9
   records the expiry as a known consequence.
2. **The production SSU2 peer-test driver is unbuilt.** Its choreography
   question is named: the tree's Alice role needs the helper's identity up
   front, while the specification has Bob select the helper. **Owner:** the
   successor that builds the production driver. **Not this plan's scope.**
3. **The requester side of the SSU2 relay protocol has no production
   caller.** Plan 527's finding, unchanged. **Owner:** the successor that
   implements the requester side. **Not this plan's scope.**
4. **The runtime's direct form is still loopback-only.** The plan makes the
   direct form *reachable*; the publication profile that would carry it
   outside the controlled loopback profile is a separate decision.
   **Owner:** the plan that widens the publication profile.
5. **The delivery is confined to the controlled loopback profile.** The
   evidence is a local lane. **Owner:** the plan that exercises the model
   against a stock peer.

### Informational

1. **Plan 520's finding M1 is discharged for the explicit bind.** Its claim
   — that the ordinary published address "cannot be earned in production"
   because there is no production entry point — was accurate for the tree as
   it stood. Plan 521 delivered the production peer test, Plan 529 the
   production observation, and this plan the production explicit bind. Plan
   520's record is not rewritten.
2. **Plan 526's finding 3 named the triple** — the explicit bind, the peer's
   observed external address and the confirmed peer test — and said no lane
   earned them. All three now have ordinary producers. Plan 526's record is
   not rewritten.
3. **Plan 283's and Plan 282's records need no correction.** The narrow
   controlled recorder is unchanged, and the corroboration-floor pair still
   yields exactly `CandidateReachable` — which this plan's lane now proves in
   ordinary operation rather than only in the transport's own test.
4. **The plan's own production change moved lines in two files**, so the
   plan's citations were corrected by an **additive** pointer table in §17.3;
   §3 itself is evidence and was not rewritten.
5. **The recorder is redundant beside the controlled one** in the controlled
   profile, and is kept anyway: the narrow recorder owns a fail-closed refusal
   the controlled activation depends on, and removing it would have rewritten
   Plan 283's foundation.
6. **The lane and the guard divide the work** — the lane proves the
   corroboration structure and the guard proves the class. The negative
   control above is the evidence for that division, including the one case
   where the lane cannot see the difference.

## Roadmap disposition

The **network-participation** roadmap gains this row:

| Plan | Subject | Status | Outcome |
| --- | --- | --- | --- |
| 530 | the router must record its own bound address | closed | passed-the-router-must-record-its-own-bound-address |

The roadmap's SSU2 reachability composition now reads, in order: Plan 520 gave
the router a published address, Plan 521 the **peer test** that corroborates
it, Plan 527 gave the reachability **one owner and one reader**, Plan 528 made
the negative **verdict** a fact, Plan 529 gave the router the peer's
**observation** of its own address, and Plan 530 supplies the **third class**
— the router's own bound address — so that all three of the corroboration
classes the transport's model demands can be earned in ordinary operation.
The composition's own row in the roadmap is unchanged, because the stock-peer
half of every one of those plans remains blocked on Plan 493's authorized
two-host topology.

## Unblock audit

| Item | State |
| --- | --- |
| Plan 530 | **closed** — all ten acceptance criteria met, the floor green on the closing tree |
| Plan 520's finding M1 | **discharged** for the explicit bind; the production peer test (Plan 521) and the production observation (Plan 529) were already delivered |
| Plan 526's finding 3 | **discharged** — all three corroboration classes now have ordinary producers |
| The runtime's direct form | **earned, in the controlled loopback profile** — the triple reaches `Reachable` and the direct material is produced; the publication profile itself stays loopback-only |
| The production SSU2 peer-test driver | **unbuilt** — the model, the recorder and the verdict path exist; the driver is the successor's subject |
| The requester side of the SSU2 relay protocol | **unwired** — the recorder exists, the producer does not; its owner is named |
| The re-record of the explicit bind after its TTL | **open** — a follow-up, named in the findings |
| Plans 433, 435-439, 493, 494 | **blocked** on external environments; untouched by this plan |

No blocked work was done under this plan's number.

## Handoff notes

- The plan's subject is the router's **own** bound address: it must record
  that fact as evidence, in ordinary operation, so the third corroboration
  class can be earned.
- The plan adds **no** capability. It supplies the missing **recorder** for a
  class the transport already counts and the daemon already opts into.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **requester side**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer half**
  all stay out of scope, with their owners named.
- The next free plan number is **531**.
