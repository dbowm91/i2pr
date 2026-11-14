# Plan 527 — the router's reachability has one owner and one reader

**Closure record.** The plan of record is
`plans/implementation/network-participation/527-the-routers-reachability-has-one-owner-and-one-reader.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 527 is **closed**. The router's reachability now has **one owner and one
reader**: the runtime owns the only reachability tracker and the product is
its only reader, and a contradicting observation can now reach the product's
state.

Classification: **corrective** (the SSU2 introducer composition's acquisition
half was unreachable because the router's reachability could never be
contradicted). Plan 490 gave the router the introducer role, Plan 498 made
the role owner reachable in a live daemon, Plan 499 made it work, Plan 523
corrected the **form** of the introducer set, Plan 524 delivered the
**acquisition**, Plan 525 the **publication** and Plan 526 the
**advertisement**. This plan fixes the state those five depend on.
**Subsystem:** network-participation. **Closure date:** 2026-11-10.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The defect.** The router's reachability had **two** trackers:

- the **runtime's**, in `ServiceState.reachability`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:1371`), built with the runtime's
  policy — the one the **product** read;
- the **peer-relay service's own** (`crates/i2pr-runtime/src/ssu2_peer_relay.rs`),
  built with the **default** policy.

Every **contradicting** signal was written only into the second one.
`RelayFirewalledSignal` was written at exactly one place,
`AddressMismatch` and `FirewalledLikely` at exactly two. The runtime's
tracker received only `LocalConfiguredBind`, `PeerTestResult{Confirmed}`,
`ValidatedPath` and `AuthenticatedPeerObservedExternalAddress` — **none** of
which contradicts. So `contradicting_classes()` was permanently 0 and
`ReachabilityState::Firewalled` and `ReachabilityState::Unreachable` were
**unreachable states** in the running router. The peer-relay service's
snapshot had **no reader in the daemon at all**.

**The consequence.** The daemon's firewalled gate
(`crates/i2pr-daemon/src/service_product.rs:4796`) reads the runtime's
tracker, so it was **permanently false**. `IntroducerRequestOwner::decide`
(`crates/i2pr-daemon/src/introducer_request.rs:314`) always refused with
`NotFirewalled`, so the router **never** asked a peer for a relay tag, never
obtained an authenticated introducer, and the introducer set it publishes
stayed empty — leaving Plan 525's publication with nothing to publish and
Plan 526's `"C"` advertisement riding a role whose acquisition half could
never run in the field.

Plan 524's lane drove the owner with the flag **injected**
(`crates/i2pr-daemon/tests/ssu2_introducer_request.rs:498`:
`owner.decide(&eligible, true, wall_ms())`). It proved the consumer honours
the flag. It never exercised the producer.

**The second defect.** The transport **decoded** the data-phase Address
block, which carries a validated endpoint, and then **discarded** it:
`SessionEvent::AddressObserved` was a unit variant. The router was told that
a peer observed an external address but never **which** address, so it could
never compare an observation with its own bound address.

**What the tree does now.**

- The peer-relay service's tracker is **removed**, not reconciled. The
  service keeps its counters and returns the typed facts it computes; the
  **runtime** records them.
- Every typed peer-test outcome is recorded, **not only the confirming
  one** — recording only the confirming kind is precisely what kept the
  tracker from ever being contradicted. The runtime's single recorder is
  `record_peer_test_outcome_locked`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3926`), and the controlled
  mirror hands its outcome to it (`:3892`).
- The runtime owns the **relay-success recorder**,
  `Ssu2RuntimeService::note_relay_firewalled`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3196`). The transport's state
  machine needs **two distinct contradicting classes** before it calls the
  router firewalled, and the two are the peer-test verdict (class 3) and the
  relay-firewalled indication (class 4), so the recorder is part of the
  deliverable and not an optional extra.
- The transport keeps the endpoint it already decoded:
  `SessionEvent::AddressObserved { observed }`
  (`crates/i2pr-transport-ssu2/src/session.rs:353`, decode site `:1936`).
  **No codec, block or option changes.**
- The runtime compares the observation with the router's **own bound
  socket** — `bound_endpoint` (`:2843`) and
  `note_observed_external_address_locked` (`:2798`) — not with a configured
  value the socket may not honour. An agreement corroborates; a
  disagreement contradicts; a family with no bound socket **refuses** the
  comparison. One observation is one fact: a contradiction never also
  corroborates.
- The router's evidence is **readable**:
  `Ssu2RuntimeService::reachability_evidence` (`:1893`) returns the state,
  the number of **distinct** corroborating classes behind it, the family's
  expiry and the family itself. Without it a contradiction could not be
  *shown* to support nothing: the state alone cannot distinguish "one
  contradicting class" from "one corroborating class", because both read
  `ObservedUnconfirmed`.

**The transport's reachability model is untouched** — the five signal kinds,
the corroboration classes, the policy floor and the state machine are
byte-identical. The plan changes what is **recorded**, not how a signal is
interpreted.

### The registered plan's corrections

**Plan 524's closed claim about the firewalled state.** Plan 524's record
states, as its invariant 5, that "**the firewalled state must be earned**:
the router asks only while its own reachability state is `Firewalled`".
That claim is **true of the consumer and false of the producer**: the owner
did ask only while firewalled, but nothing in the tree could ever *earn*
the state, because the tracker the product read could never be
contradicted. The state was unreachable in every configuration.

Plan 524's record is **not rewritten**. The correction lives here and in
Plan 527's registry row, per the house rule that a closed closure record is
not edited.

**The scope question, settled by the transport's own tests.** The
registration kept the **requester** side out of scope and asked whether the
peer-test route alone could reach `Firewalled`. It cannot. The transport's
own tests say so
(`crates/i2pr-transport/src/reachability.rs`,
`contradictory_observation_keeps_state_unconfirmed` and
`corroborated_failures_mark_unreachable`): a contradicting observation must
come from **two distinct classes** before the state machine calls the router
firewalled. The runtime's relay-success recorder is therefore part of the
deliverable. The plan is **not re-scoped**: §14's criterion 4 is met as
written, because the lane drives the recorder and the gate reads the real
tracker.

**The citation drift.** This plan's own production change moved lines in
four files and removed the items the registration quoted, so the citations
that pointed at them no longer land on the text they quoted. §3 of the plan
is evidence and is **not** rewritten; the additive pointer table is §17.3 of
the plan, and every pointer in it is verified against the closing tree.

## What the plan did not close

- The **requester side** of the SSU2 relay protocol — the part that
  produces the relay-success indication in the field — stays **unwired**.
  `start_relay_request` and `on_relay_response` have no production caller.
  The runtime's **recorder** for that indication exists and the lane drives
  it, so `Firewalled` is reachable; only the field producer is missing. Its
  owner is named in the plan's §5.2 and §5.3.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology. The plan's evidence is a local lane.
- The advertisement stays confined to the **controlled loopback profile**,
  because `ssu2.advertise` is refused outside it. Under that profile the
  router binds loopback, so the comparison is between two loopback
  addresses and the contradiction is a **qualification** claim.
- The runtime's **direct** form still needs three corroboration classes and
  no lane in this tree earns them. This plan does not change that; it makes
  the tracker's **negative** half reachable.

## Implementation commits

| Commit | Subject |
| --- | --- |
| `a93426c3` | `fix(ssu2): the router's reachability has one owner and one reader` |
| `ca1245f5` | `test(daemon): Plan 527's lane and guard for the reachability contradiction` |
| `b22785b2` | `docs(ssu2): Plan 527's documentation, the inventory and the amendment log` |

The registration commit is `ec717ef6`.

## Requirement-to-evidence matrix

Every acceptance criterion of the plan's §14, the file and line that
satisfies it, and the test that proves it.

| # | Criterion | Satisfied by | Proven by |
| --- | --- | --- | --- |
| 1 | the router's reachability has **one** tracker, and the peer-relay service owns none | `crates/i2pr-runtime/src/ssu2_runtime.rs:1371` (the only tracker field); `crates/i2pr-runtime/src/ssu2_peer_relay.rs` (no tracker field, no tracker construction) | guard rules 1 and 2: `the_router_has_one_reachability_tracker`, `the_peer_relay_service_owns_no_tracker`; lane row `the_product_reads_the_one_tracker_and_sees_the_contradiction` |
| 2 | the runtime records **every** typed peer-test outcome, and a contradicting outcome reaches the product's state | `crates/i2pr-runtime/src/ssu2_runtime.rs:3926` (`record_peer_test_outcome_locked`, all five kinds), `:3892` (the controlled mirror hands its outcome over) | guard rule 6 `every_typed_outcome_is_recorded`; lane rows `an_observation_that_differs_from_the_bound_address_contradicts` and `two_contradicting_classes_reach_firewalled` |
| 3 | an observation that differs from the router's own bound address is a contradiction, and an agreement corroborates | `crates/i2pr-runtime/src/ssu2_runtime.rs:2798` (`note_observed_external_address_locked`), `:2843` (`bound_endpoint` — the **bound** socket, not a configured value) | guard rule 4 `the_runtime_compares_with_the_bound_address`; lane rows `an_observation_that_agrees_with_the_bound_address_corroborates`, `an_observation_that_differs_from_the_bound_address_contradicts`, `a_contradiction_is_never_also_a_corroboration`, `a_router_with_no_bound_socket_refuses_the_comparison`, `an_unknown_family_records_nothing` |
| 4 | the product's reachability state can reach `Firewalled`, and Plan 524's gate fires on it **without any injected flag** | `crates/i2pr-runtime/src/ssu2_runtime.rs:3196` (`note_relay_firewalled`), `:1893` (`reachability_evidence`); the gate reads the tracker at `crates/i2pr-daemon/src/service_product.rs:4796` | lane rows `two_contradicting_classes_reach_firewalled` and `the_firewalled_gate_asks_only_on_the_state_the_tracker_really_holds`; guard rules 7 and 8 (`the_product_reads_the_runtime_s_tracker`, `the_gate_stays_fail_closed`) |
| 5 | the transport's codec, its blocks and its options are untouched | `crates/i2pr-transport-ssu2/src/session.rs:353` (the event variant) and `:1936` (the decode site) — no codec, block or option change | guard rules 3 and 5: `the_transport_keeps_the_observed_endpoint`, `the_transport_model_is_untouched` |
| 6 | no configuration key is added and no default moves | `crates/i2pr-daemon/src/config.rs` is untouched by this plan | guard rule 10 `no_new_key_and_no_infrastructure` and its inverted probe `the_guard_reports_a_new_configuration_key` |
| 7 | the lane and the guard are green, every guard rule has an inverted probe that fires it, and every rule fails closed on an absent item | `crates/i2pr-daemon/tests/ssu2_reachability_contradiction.rs` (9 rows), `crates/i2pr-daemon/tests/ssu2_reachability_contradiction_boundary.rs` (10 rules, 12 inverted probes, 1 missing-item probe) | the floor's `plan527-reachability-contradiction-lane` and `plan527-reachability-contradiction-boundary` steps |
| 8 | the routine floor is green on the closing tree | the floor table below | the floor run |
| 9 | `specs/support.toml` records the surface as `experimental` and `advertised = false`; the advertised count stays **zero** | `specs/support.toml`, surface `daemon.reachability-contradiction` | `scripts/check-tooling-inventory.py`; the counts are 120 surfaces, 115 experimental, 4 deferred, 1 ready, **0 advertised** |
| 10 | `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` are byte-identical | `crates/i2pr-netdb/src/local.rs:172` (`CONTROLLED_ROUTER_VERSION`); neither manifest is touched | `git status` on both manifests is empty; the floor's `fmt`, `check` and `clippy` steps |
| 11 | no boundary script is weakened | the diff touches **no** script under `scripts/`; Plan 521's peer-test guard is **extended**, with the original branch kept, the delegation branch checked, the absent-recorder case failing closed and two inverted probes added (plan's §17.4) | the floor's `plan521-peer-test-role-boundary` step (27 rows) |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS — no warnings |
| `cargo test --locked -p i2pr-daemon --test ssu2_reachability_contradiction -- --test-threads=1` | PASS — 9 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_reachability_contradiction_boundary -- --test-threads=1` | PASS — 23 rows |
| `cargo test --locked -p i2pr-daemon --test peer_test_role_boundary -- --test-threads=1` | PASS — 27 rows (was 25; two probes added) |
| `cargo test --locked -p i2pr-daemon --all-targets --no-fail-fast -- --test-threads=1` | 153 binaries green, 1 failure — `the_controlled_recorder_is_untouched`, the guard rule this plan extends. After the extension the guard is 27/27 and the floor's `test-workspace` step is the authority. |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-license-metadata.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests, OK |

## The routine floor

The floor ran on the **closing** tree (`b22785b2`), and every step is recorded in
`/tmp/527-floor.log` with its own log under `/tmp/527f-<step>.log`. The floor is
a plain sequence of `step` runs that records each step's exit status; no step is
skipped, no `|| true`, no `continue-on-error` and no filename filtering. The
workspace step runs with `--no-fail-fast`, so every test binary's result is
reported rather than the run stopping at the first failure.

| Step | Result |
| --- | --- |
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

**FLOOR PASS 93 — FLOOR FAIL 0.**

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be
reported. Every probe mutates an **in-memory copy** of the source, so **no
file on disk is written**. Every rule **fails closed** when the item it
names is absent — the guard reports the missing item rather than passing.

### Plan 527's own guard — `crates/i2pr-daemon/tests/ssu2_reachability_contradiction_boundary.rs`

Ten rules, twelve inverted probes, one missing-item probe. All 23 rows green.

| Rule | What it forbids | Its inverted probe |
| --- | --- | --- |
| 1 | a second reachability tracker | `the_guard_reports_a_second_reachability_tracker` |
| 2 | the peer-relay service owning a tracker | `the_guard_reports_a_peer_relay_service_that_owns_a_tracker` |
| 3 | the transport discarding the observed endpoint | `the_guard_reports_an_observation_that_discards_the_endpoint` |
| 4 | a changed transport reachability model | `the_guard_reports_a_changed_transport_model` |
| 5 | a comparison that is not against the **bound** address | `the_guard_reports_a_comparison_that_is_not_against_the_bound_address` |
| 6 | a recorder that drops the contradicting outcomes | `the_guard_reports_a_recorder_that_drops_the_contradicting_outcomes` |
| 7 | a second relay-success recorder | `the_guard_reports_a_second_relay_success_recorder` |
| 8 | the product reading another tracker | `the_guard_reports_a_gate_that_reads_another_tracker` |
| 9 | a gate that is not fail-closed | `the_guard_reports_a_gate_that_is_not_fail_closed` |
| 10 | a new configuration key or new infrastructure | `the_guard_reports_a_new_configuration_key`, `the_guard_reports_new_infrastructure` |

Plus `the_guard_reports_a_missing_named_item`, which proves the
missing-item branch, and `the_guard_reports_a_comparison_that_never_contradicts`.

### Plan 521's peer-test guard — the extension

Plan 521's rule 5 asserted that the controlled floodfill mirror "keeps its
own mirror", by requiring the literal `ReachabilitySignal::PeerTestResult`
inside the mirror's **own** body. This plan moves that recording into the
runtime's single recorder, so the rule is **extended**, not weakened:

- the original branch is kept — a mirror that records nothing, neither
  inline nor by delegation, is still reported;
- the delegation branch is **checked**, not assumed — the recorder the
  mirror delegates to must be present and must file the outcome as a
  `PeerTestResult` under its own kind, the **contradicting** kinds
  included;
- the absent-recorder case **fails closed** — a missing recorder is a
  report, not a pass;
- the "the production recorder must not delegate to the controlled mirror"
  branch is untouched, and the dependency now points the safe way: the
  controlled mirror calls the runtime's recorder, never the reverse;
- two inverted probes are added: `the_guard_reports_a_controlled_mirror_that_records_nothing`
  and `the_guard_reports_a_controlled_mirror_that_delegates_to_a_recorder_that_files_nothing`.

The guard's probe count goes from 13 to 15 and its rule count stays at 12.

## Acceptance criteria

The plan's §14 lists eleven criteria and six stop conditions.

1. the router's reachability has **one** tracker, and the peer-relay service
   owns none — **met**;
2. the runtime records **every** typed peer-test outcome, and a contradicting
   outcome reaches the product's state — **met**;
3. an observation that differs from the router's own bound address is
   recorded as a contradiction, and an observation that agrees corroborates —
   **met**;
4. the product's reachability state can reach `Firewalled`, and Plan 524's
   gate fires on it **without any injected flag** — **met**;
5. the transport's codec, its blocks and its options are untouched — **met**;
6. no configuration key is added and no default moves — **met**;
7. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, and every rule fails closed on an absent item — **met**;
8. the routine floor is green on the closing tree — **met**;
9. `specs/support.toml` records the surface as `experimental` and
   `advertised = false`; the advertised count stays **zero** — **met**;
10. `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` are
    byte-identical — **met**;
11. no boundary script is weakened — **met**.

**No stop condition fired.** Removing the peer-relay service's tracker
needed no change to the transport's reachability policy or to the state
machine; carrying the observed endpoint needed no new block, no new option
and no codec change; the comparison needed no configuration key and no
second policy; it needed no new task, no new timer and no new listener; and
the plan did **not** turn out to need the requester side.

## Invariant review

The plan's §4 lists twelve invariants. Each is reviewed against the closing
tree.

1. **One owner, one reader.** The runtime owns the only tracker
   (`ssu2_runtime.rs:1371`) and the product is its only reader. The
   peer-relay service owns none. — **held**.
2. **The service returns, the runtime records.** `on_peer_test`,
   `mark_peer_test_inconclusive` and `mark_firewalled` return the typed
   outcome; nothing writes a reachability signal. — **held**.
3. **Every typed outcome is recorded under its own kind.** Five outcomes,
   five kinds, one recorder. — **held**.
4. **A contradiction never also corroborates.** One observation is one fact,
   filed under one family; the corroboration count is unchanged by a
   contradicting signal. — **held**.
5. **The comparison is against the bound socket.** `bound_endpoint` reads the
   **bound** endpoint and refuses a zero port or an unspecified address. —
   **held**.
6. **No bound socket means no comparison.** The family is refused and
   nothing is recorded. — **held**.
7. **An unknown family records nothing.** `note_relay_firewalled` returns
   early on `AddressFamily::Unknown`. — **held**.
8. **The gate stays fail-closed.** `IntroducerRequestOwner::decide` still
   refuses with `NotFirewalled` and still counts the refusal under its own
   reason. — **held**.
9. **The transport's model is untouched.** The five signal kinds, the
   corroboration classes, the policy floor and the state machine are
   byte-identical. — **held**.
10. **No new key, no new default, no new infrastructure.** No configuration
    key is added and no task, timer or listener is created. — **held**.
11. **No wire change.** The Address block already carried the endpoint; the
    plan stops discarding it. — **held**.
12. **No promotion.** `router.version` stays `0.9.62` and the advertised
    count stays **zero**. — **held**.

## Failure, cancellation, restart, and contention review

- **Failure.** Every refusal is counted under its own reason and leaves no
  state behind. A poisoned state mutex degrades to "no comparison", never to
  a fabricated agreement: `note_observed_external_address` returns
  `ReachabilityState::Unknown` on a poisoned lock, and
  `reachability_evidence` returns an empty snapshot rather than a claim.
- **Cancellation.** The plan adds **no task**. The comparison happens on the
  path that already decodes the observation, and the runtime's material
  builders are synchronous and non-awaiting.
- **Restart.** The router's reachability is recomputed from live evidence, so
  a restart with no observation is `Unknown` and asks nobody. There is no
  persisted reachability claim.
- **Contention.** The plan adds no queue, no task and no lock of its own.
  The comparison happens under the runtime's existing state lock. The lock
  **order** was audited: the only nesting is `state` → `sockets` inside
  `bound_endpoint`, and no code path holds `sockets` while taking `state`,
  so the order is not inverted and no deadlock can form.

## Migration and compatibility review

- **No wire change.** The Address block already carried the endpoint; the
  plan stops discarding it. The daemon's peer-test message format, the
  transport's blocks and the transport's options are untouched.
- **A reading change for the router.** The router's reachability can now
  reach `Firewalled` and `Unreachable`. A router that was `Reachable` can
  become `Firewalled`, and it will then ask an introducer for a relay tag.
  That is the intended behaviour, and it is the behaviour Plan 524's and
  Plan 525's compositions were written for.
- **A removed API.** `Ssu2PeerRelayService::reachability_state`,
  `::note_validated_path` and `::note_relay_firewalled` are gone, and
  `Ssu2PeerRelaySnapshot::reachability` is gone. The two runtime test sites
  that used them are rewritten with the reason inline. No external crate
  used them.
- **No configuration migration.** No key is added, renamed or removed.

## Security review

- **No secret is logged.** The observation is an **address** the peer
  already saw, and the comparison happens under the state lock without
  logging, printing or serialising either endpoint.
- **No one local condition is written into a peer's profile.** The
  comparison is between the router's own bound address and what a peer
  reported about **this** router. Nothing is written into any peer's record.
- **A contradiction is never also a corroboration.** A single observation
  cannot both support and deny the router's reachability, so a peer cannot
  inflate the router's claim.
- **The contradiction is earned, not asserted.** The disagreement must come
  from the router's **bound** socket, and a family with no bound socket
  refuses the comparison. A router never invents a contradiction it cannot
  support.
- **Fail closed throughout.** An unknown family records nothing, a poisoned
  lock degrades to "no comparison", the gate refuses with `NotFirewalled`
  and counts the refusal, and the guard's absent-item case is a report.
- **The peer-relay service no longer holds a reachability claim at all.**
  Removing the second tracker removes a place where a reachability verdict
  could be computed from a *default* policy instead of the runtime's.

## Limitations

- The **requester side** stays unwired, so the relay-success route to a
  contradiction is not taken in the field. The peer-test route is the one
  this plan delivers, and the recorder for the relay route exists so the
  producer has exactly one place to write to.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology. The plan's evidence is a local lane.
- The advertisement stays confined to the **controlled loopback profile**:
  under that profile the router binds loopback, so the comparison is
  between two loopback addresses and the contradiction is a qualification
  claim.
- The runtime's **direct** form still needs three corroboration classes and
  no lane in this tree earns them.
- The lane drives the recorder; it does not drive a **peer** that produces
  the field indication. A stock-peer observation is not claimed.

## Findings by severity

### Medium — none open

### Low — recorded, not fixed

1. **The requester side of the SSU2 relay protocol has no production
   caller.** `start_relay_request` and `on_relay_response`
   (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:481` and `:497`) are
   unwired, so the relay-success indication is never produced in the field.
   The **recorder** exists and the lane drives it, so the state is
   reachable; only the producer is missing. **Owner:** the successor that
   implements the requester side. **Not this plan's scope.**
2. **The runtime's direct form still needs three corroboration classes** and
   no lane in this tree earns them. Plan 526 recorded this and it is
   unchanged: the `"C"` rides the direct form and the direct form is not
   yet earned in the field. **Owner:** the plan that earns the direct form.

### Informational

1. Plan 521's peer-test guard needed an **extension**, not a weakening, and
   the extension is recorded in the plan's §17.4.
2. The plan's own production change moved lines in four files, so the plan's
   citations were corrected by an **additive** pointer table in §17.3; §3
   itself is evidence and was not rewritten.
3. The peer-relay service's `relay_responses` counter survives the
   tracker's removal through `count_relay_response`
   (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:474`).

## Roadmap disposition

The **network-participation** roadmap gains this row:

| Plan | Subject | Status | Outcome |
| --- | --- | --- | --- |
| 527 | the router's reachability has one owner and one reader | closed | passed-the-router-s-reachability-has-one-owner-and-one-reader |

The roadmap's SSU2 introducer composition now reads, in order: Plan 490 gave
the router the role, Plan 523 corrected the **form**, Plan 524 delivered the
**acquisition**, Plan 525 the **publication**, Plan 526 the
**advertisement**, and Plan 527 made the state those five depend on
**earnable**. The composition's own row in the roadmap is unchanged, because
the stock-peer half of every one of those plans remains blocked on Plan
493's authorized two-host topology.

## Unblock audit

| Item | State |
| --- | --- |
| Plan 527 | **closed** — all eleven acceptance criteria met, the floor green on the closing tree |
| The SSU2 introducer composition's stock-peer half | **blocked** on Plan 493's authorized two-host topology |
| The requester side of the SSU2 relay protocol | **unwired** — the recorder exists, the producer does not; its owner is named |
| The runtime's direct form | **not yet earned** — three corroboration classes are needed and no lane in this tree earns them |
| Plans 433, 435-439, 493, 494 | **blocked** on external environments; untouched by this plan |

No blocked work was done under this plan's number.

## Handoff notes

- The plan's subject is the router's **own reachability**: one owner, one
  reader, and a state that can be **contradicted**.
- The plan completes the chain the SSU2 introducer composition needs. Without
  a reachable `Firewalled` state, the acquisition half can never run.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **requester side**, the **`B` advertisement**, the **`caps=f`
  advertisement** and the **stock-peer half** all stay out of scope, with
  their owners named.
- The next free plan number is **528**.
