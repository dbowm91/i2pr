# Plan 528 — the router's peer-test verdict must not be thrown away

**Closure record.** The plan of record is
`plans/implementation/network-participation/528-the-routers-peer-test-verdict-must-not-be-thrown-away.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 528 is **closed**. The router's peer-test verdict is a **fact**: a
disagreement with the router's own bound address is recorded, reaches the one
tracker the product reads, and puts the router in `Firewalled`.

Classification: **corrective** (the tree ran a **complete** three-party SSU2
peer test and threw every verdict away except "confirmed", and the transport
applied its **corroboration** floor to **contradictions**, so `Firewalled` and
`Unreachable` were unreachable states and the daemon's firewalled gate was
permanently false). Plan 527 gave the router's reachability one owner and one
reader; this plan gives that owner a verdict it can actually hold.
**Subsystem:** network-participation. **Closure date:** 2026-11-10.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The defect — the verdict was thrown away.** The tree runs a **complete**
three-party SSU2 peer test: seven messages cross real UDP datagrams between
three distinct bound sockets, and the verdict Msg 7 carries exactly what the
third party observed. The driver's final `match` then required the
observation to **equal the router's bound address** and two peers to
corroborate, and collapsed **every other outcome** into one error:

```rust
        _ => Err(ControlledPeerTestError::ExchangeNotConfirmed),
```

So a **mismatch**, a **firewalled** verdict, an **inconclusive** test and a
**rejection** were indistinguishable from a **failed** exchange. The
`ControlledPeerTestOutcome` enum had exactly **one** variant, `Confirmed`
(`crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs:81`), so there was
nowhere for the verdict to go. The observation that *disagrees* is the fact
the router most needs, and it was discarded at the last step.

**The second defect — the transport applied the corroboration floor to
contradictions.** `ReachabilityPolicy` carried a single floor, and the
contradiction branch compared the contradicting-class count with **that**
floor:

```rust
        if self.contradicting_classes() >= self.policy.min_corroboration {
```

Every peer-test verdict is filed under **one** corroboration class, so two
peer-test verdicts contradicted **once** between them — the transport's own
test says so in as many words
(`crates/i2pr-transport/src/reachability.rs:929`). The floor of two distinct
contradicting classes could therefore never be reached, one contradiction left
the router on `ObservedUnconfirmed` for ever, and `Firewalled` and
`Unreachable` were **unreachable states** in the running router. The daemon's
firewalled gate (`crates/i2pr-daemon/src/service_product.rs:4804`) read
`false` for ever.

**The third defect — the model could not express the reference's verdict.** The
SSU2 specification's peer-test result table has four outcomes, and the row
"message 4 arrived, message 5 did not" is **FIREWALLED**. The pinned i2pd
2.61.0 concludes FIREWALLED from a **single** peer test
(`libi2pd/SSU2Session.cpp:2703`-`:2705`); the specification's "two or more
tests" is *recommended*, not required. The tree had no way to state that
verdict at all.

**What the tree does now.**

- The transport's policy carries a **contradiction floor** beside the
  corroboration floor. `MIN_CONTRADICTION_FLOOR` is 1
  (`crates/i2pr-transport/src/reachability.rs:77`) and
  `ReachabilityPolicy::new` takes it as its second argument (`:292`), with
  `min_contradiction()` (`:312`) and its own validation (`:334`, refusing a
  floor of 0 and a floor above the observation budget). The **corroboration**
  floor is **untouched** at 2 (`MIN_CORROBORATION_FLOOR`, `:66`), so
  `Reachable` still needs three distinct supporting classes and the
  **positive** claim is not weakened at all.
- The **firewalled verdict** is first class.
  `ReachabilitySignal::is_firewalled_verdict` (`:209`) answers the question
  the state machine actually needs: is this signal a fact about the router's
  **own** inbound reachability? A peer test that concludes the router is not
  directly reachable, and the relay-firewalled indication, are; a bare
  disagreement between two observations is not. So a contradiction now lands in
  one of two states — `Firewalled` for a firewalled verdict, `Unreachable` for
  any other contradiction (`:620`).
- The specification's peer-test result table is **transcribed** into the tree
  as data and pinned by a test, so the tree's rule is checkable against the
  specification rather than only against prose:
  `PEER_TEST_RESULT_TABLE` (`crates/i2pr-transport-ssu2/src/peer_test.rs:153`),
  the total lookup `peer_test_result` (`:209`), and the two pinned tests
  (`:1064` and `:1089`). The specification's "FIREWALLED or SYMNAT" row is
  read **conservatively** as ambiguous.
- The runtime has a **verdict path**.
  `Ssu2RuntimeService::note_peer_test_verdict`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:2819`) consults the
  specification's own table and answers with a typed `Ssu2PeerTestVerdict`
  (`:1711`): the FIREWALLED row is the router's own firewalled fact and is
  recorded as such, an OK row corroborates **nothing** by itself (the
  corroboration is the address agreement, and that arrives through the
  comparison), and an unknown or ambiguous row records nothing at all.
  `note_peer_test_outcome` (`:2849`) is the one place a typed outcome becomes
  a reachability fact, and `record_peer_test_outcome_locked` (`:4026`) files
  it under its own kind.
- The runtime's comparison against the router's **own bound socket**
  (`note_observed_external_address_locked`, `:2888`, against `bound_endpoint`,
  `:2943`) records a **disagreement** as the **firewalled verdict**, not as a
  bare mismatch: a peer seeing the router at an address it does not own **is**
  the router behind a NAT, and it is the same fact the specification's
  FIREWALLED row reports, so both routes reach the same state.
- The controlled driver's outcome enum grew the verdicts it used to throw
  away — `ObservedElsewhere`, `ObservationsDisagree`, `FirewalledLikely`,
  `Inconclusive` and `Rejected` beside `Confirmed`
  (`crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs:81`-`:134`) — and the
  driver's error arm keeps only the genuinely **failed** cases (`:814`-`:857`).
  A caller can therefore tell "the test failed" from "the test succeeded and
  the router is behind NAT".
- The daemon's firewalled gate reads the state that can now be `Firewalled`
  (`crates/i2pr-daemon/src/service_product.rs:4804`). Its **refusal reasons**
  are unchanged, and the daemon's controlled activation still refuses any
  verdict that is not a confirmation
  (`crates/i2pr-daemon/src/floodfill.rs:696`), so the floodfill's reachability
  proof is still minted only from a real confirmation.

### The registered plan's corrections

**Plan 527's closed claim that the transport's reachability model is
untouched.** Plan 527's record states, as its invariant 9, that "**the
transport's model is untouched**: the five signal kinds, the corroboration
classes, the policy floor and the state machine are byte-identical". That was
true of Plan 527's change and is **no longer true of the tree**: the five
signal kinds and the corroboration classes are still untouched, but the
**policy** now carries a second floor and the `Firewalled` branch asks a new
question.

Plan 527's record is **not rewritten**. The correction lives here and in Plan
528's registry row, per the house rule that a closed closure record is not
edited.

**The kind correction — §3.3's table is wrong about one row.** The
registration listed the runtime's bound-socket comparison as the producer of
the class-3 `PeerTestResult{AddressMismatch}`. It is not. The transport's
**table-level** `AddressMismatch` — two observations that disagree with *each
other* — is a bare disagreement and stays `AddressMismatch`. The runtime's
comparison against the router's **own bound socket** is a different fact: a
peer sees the router at an address the router does not own, so the router is
behind a NAT. That is **exactly** the fact the specification's FIREWALLED row
reports, so it is recorded as `PeerTestOutcomeKind::FirewalledLikely` and both
routes reach the same state. Plan 527 recorded the comparison as
`AddressMismatch`; Plan 527's record is not rewritten.

**The citation drift.** This plan's own production change moved lines in four
files, so the citations that pointed at them no longer land on the text they
quoted. §3 of the plan is evidence and is **not** rewritten; the additive
pointer table is §17.3 of the plan, and every pointer in it is verified
against the closing tree.

## What the plan did not close

- The **production** SSU2 peer-test driver stays **unbuilt**, and the tree's
  live router therefore still cannot be contradicted in the field. The model
  and the recorder are built here; the driver is the successor's subject, and
  its choreography question is named in the plan's §5.3: the tree's Alice role
  needs the helper's identity up front, while the specification has Bob
  select the helper.
- The **requester side** of the SSU2 relay protocol — the part that produces
  the relay-success indication in the field — stays **unwired**. Plan 527's
  finding is unchanged.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology. The plan's evidence is a local lane.
- The delivery is confined to the **controlled loopback profile**: the
  comparison is between two loopback addresses, so the contradiction is a
  **qualification** claim, exactly as Plan 526's advertisement is.
- The runtime's **direct** form still needs three corroboration classes and
  no lane in this tree earns them. This plan does not change that; it corrects
  the **negative** side of the model.

## Implementation commits

| Commit | Subject |
| --- | --- |
| `418a4b95` | `feat(ssu2): make the peer-test verdict a fact, not an error (Plan 528)` |
| `50189f5e` | `test(ssu2): Plan 528's lane and guard for the peer-test verdict` |
| `fbeec627` | `docs(ssu2): Plan 528's documentation, the inventory and the amendment log` |

The registration commit is `b141af68`.

## Requirement-to-evidence matrix

Every acceptance criterion of the plan's §14, the file and line that
satisfies it, and the test that proves it.

| # | Criterion | Satisfied by | Proven by |
| --- | --- | --- | --- |
| 1 | a disagreement between the observed address and the router's **own bound socket** is a **fact**, not an error | `crates/i2pr-runtime/src/ssu2_runtime.rs:2888` (`note_observed_external_address_locked`), `:2943` (`bound_endpoint` — the **bound** socket, not a configured value); `crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs:814`-`:857` (the driver's verdict match, one fact per arm) | guard rule 6 `the_mismatch_arm_is_a_fact` and rule 7 `the_comparison_is_against_the_bound_socket`; lane rows `the_disagreement_is_the_routers_own_firewalled_verdict` and `a_comparison_with_no_bound_socket_is_refused` |
| 2 | the fact reaches the **one** tracker and the product's state | `crates/i2pr-runtime/src/ssu2_runtime.rs:2849` (`note_peer_test_outcome`), `:4026` (`record_peer_test_outcome_locked`), `:1927` (`reachability_evidence`); the product reads the state at `crates/i2pr-daemon/src/service_product.rs:4804` | lane rows `a_firewalled_row_is_the_routers_firewalled_fact` and `the_disagreement_is_the_routers_own_firewalled_verdict`; guard rule 7 |
| 3 | the reachability state can reach `Firewalled` from the evidence the reference's own single-test rule produces | `crates/i2pr-transport/src/reachability.rs:77` (`MIN_CONTRADICTION_FLOOR`), `:209` (`is_firewalled_verdict`), `:620` (the contradiction branch); the runtime's verdict path at `crates/i2pr-runtime/src/ssu2_runtime.rs:2819` | guard rules 2 and 3 `the_contradiction_floor_is_separate` and `the_firewalled_verdict_is_first_class`; the transport's own test `a_firewalled_verdict_alone_marks_the_router_firewalled`; lane row `a_firewalled_row_is_the_routers_firewalled_fact` |
| 4 | the daemon's firewalled gate fires on it **without any injected flag** | `crates/i2pr-daemon/src/service_product.rs:4804` (the gate reads the tracker), `crates/i2pr-daemon/src/introducer_request.rs:303` (`decide` — the refusal reasons are unchanged) | lane rows `the_firewalled_gate_asks_only_on_the_state_the_tracker_really_holds` and `a_router_with_no_verdict_asks_nobody`; guard rule 8 `the_gate_stays_fail_closed` |
| 5 | the **corroboration** floor is **unchanged** and `Reachable` still needs three distinct supporting classes | `crates/i2pr-transport/src/reachability.rs:66` (`MIN_CORROBORATION_FLOOR`), `:639` (the `supporting_classes()` call), `:572` (`supporting_classes` — unchanged) | guard rule 1 `the_corroboration_floor_is_unchanged`; the lane row `the_corroboration_floor_is_unchanged` |
| 6 | the specification's peer-test result table is transcribed into the tree and pinned | `crates/i2pr-transport-ssu2/src/peer_test.rs:153` (`PEER_TEST_RESULT_TABLE`), `:209` (`peer_test_result`), `:117` (`PeerTestResult`), `:137` (`PeerTestResultRow`) | guard rule 5 `the_result_table_is_transcribed_and_pinned`; the transport's own tests `the_specification_result_table_is_transcribed_and_pinned` (`:1064`) and `the_peer_test_result_lookup_is_total` (`:1089`); the lane row `the_specification_table_is_the_trees_authority` |
| 7 | a neutral verdict moves nothing | `crates/i2pr-transport/src/reachability.rs:174` (`supports_reachability`) and `:191` (`contradicts_reachability`) — neither names `Inconclusive` or `Rejected` | guard rule 4 `the_neutral_verdicts_move_nothing`; the lane rows `an_ok_row_proves_nothing_by_itself`, `an_ambiguous_row_claims_nothing` and `an_unknown_row_claims_nothing` |
| 8 | the transport's signal vocabulary and corroboration classes are untouched | `crates/i2pr-transport/src/reachability.rs:110` (`PeerTestOutcomeKind` — five kinds), `:573` (`supporting_classes`), `:591` (`contradicting_classes`) | guard rule 10 `the_transport_vocabulary_is_untouched` and its inverted probe `the_guard_reports_a_changed_peer_test_vocabulary` |
| 9 | no configuration key is added and no default moves | `crates/i2pr-daemon/src/config.rs` is untouched by this plan | guard rule 9 `no_new_key_and_no_infrastructure` and its inverted probe `the_guard_reports_a_new_configuration_key` |
| 10 | the lane and the guard are green, every guard rule has an inverted probe that fires it, and every rule fails closed on an absent item | `crates/i2pr-daemon/tests/ssu2_peer_test_verdict.rs` (10 rows), `crates/i2pr-daemon/tests/ssu2_peer_test_verdict_boundary.rs` (10 rules, 12 inverted probes, 1 missing-item probe) | the floor's `plan528-peer-test-verdict-lane` and `plan528-peer-test-verdict-boundary` steps |
| 11 | the routine floor is green on the closing tree | the floor table below | the floor run |
| 12 | `specs/support.toml` records the surface as `experimental` and `advertised = false`; the advertised count stays **zero**; `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` stay byte-identical; no boundary script is weakened | `specs/support.toml`, surface `daemon.peer-test-verdict`; `crates/i2pr-netdb/src/local.rs` (`CONTROLLED_ROUTER_VERSION`); neither manifest is touched; the diff touches **no** script under `scripts/` | `python3 scripts/check-tooling-inventory.py`; the counts are 121 surfaces, 116 experimental, 4 deferred, 1 ready, **0 advertised**; `git status` on both manifests is empty |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS — no warnings |
| `cargo test --locked -p i2pr-transport --all-targets` | PASS — 52 rows and 6 rows |
| `cargo test --locked -p i2pr-transport-ssu2 --all-targets` | PASS — 159, 19, 22, 9, 13 and 9 rows |
| `cargo test --locked -p i2pr-runtime --all-targets` | PASS — 133, 3, 19 and 7 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_peer_test_verdict -- --test-threads=1` | PASS — 10 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_peer_test_verdict_boundary -- --test-threads=1` | PASS — 23 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_reachability_contradiction -- --test-threads=1` | PASS — 9 rows (the corrected Plan 527 lane) |
| `cargo test --locked -p i2pr-daemon --test ssu2_reachability_contradiction_boundary -- --test-threads=1` | PASS — 23 rows (the extended Plan 527 guard) |
| `cargo test --locked -p i2pr-daemon --test peer_test_role_boundary -- --test-threads=1` | PASS — 27 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-license-metadata.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests, OK |

## The routine floor

The floor ran on the **closing** tree (`fbeec627`), and every step is recorded in
`/tmp/528-floor.log` with its own log under `/tmp/528f-<step>.log`. The floor is
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
| `plan528-peer-test-verdict-lane` | PASS |
| `plan528-peer-test-verdict-boundary` | PASS |
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

**FLOOR PASS 95 — FLOOR FAIL 0.**

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be
reported. Every probe mutates an **in-memory copy** of the source, so **no
file on disk is written**. Every rule **fails closed** when the item it
names is absent — the guard reports the missing item rather than passing.

### Plan 528's own guard — `crates/i2pr-daemon/tests/ssu2_peer_test_verdict_boundary.rs`

Ten rules, twelve inverted probes, one missing-item probe. All 23 rows green.

| Rule | What it forbids | Its inverted probe |
| --- | --- | --- |
| 1 | a weakened corroboration floor | `the_guard_reports_a_weakened_corroboration_floor` |
| 2 | the contradiction floor merged into the corroboration floor | `the_guard_reports_a_merged_contradiction_floor` |
| 3 | a firewalled verdict that is not first class | `the_guard_reports_a_firewalled_verdict_that_is_not_first_class` |
| 4 | a neutral verdict that moves the state | `the_guard_reports_a_neutral_verdict_that_moves_something` |
| 5 | a result table that loses a row, or loses its pin | `the_guard_reports_a_result_table_that_loses_a_row`, `the_guard_reports_an_unpinned_result_table` |
| 6 | a verdict reported as an error | `the_guard_reports_a_mismatch_arm_that_is_an_error` |
| 7 | a comparison against another address | `the_guard_reports_a_comparison_against_another_address` |
| 8 | a gate that is not fail-closed | `the_guard_reports_a_gate_that_is_not_fail_closed` |
| 9 | a new configuration key, or new infrastructure | `the_guard_reports_a_new_configuration_key`, `the_guard_reports_new_infrastructure` |
| 10 | a changed peer-test vocabulary | `the_guard_reports_a_changed_peer_test_vocabulary` |

Plus `the_guard_reports_a_missing_named_item`, which proves the
missing-item branch.

### Plan 527's guard — the extension

Plan 527's rule `the_runtime_compares_with_the_bound_address` required the
comparison to record an `AddressMismatch`. The comparison now records the
firewalled verdict, so the rule accepts **either** contradicting kind. The
rule is **extended, not weakened**: the branch that corroborates instead of
contradicting is still reported, and its inverted probe was extended with it,
so the probe still fires. The guard stays at 23 rows and 12 probes.

## Acceptance criteria

The plan's §14 lists twelve criteria and five stop conditions.

1. a disagreement between the observed address and the router's **own bound
   socket** is a **fact**, not an error — **met**;
2. the fact reaches the **one** tracker and the product's state — **met**;
3. the reachability state can reach `Firewalled` from the evidence the
   reference's own single-test rule produces — **met**;
4. the daemon's firewalled gate fires on it **without any injected flag** —
   **met**;
5. the **corroboration** floor is **unchanged** and `Reachable` still needs
   three distinct supporting classes — **met**;
6. the specification's peer-test result table is transcribed into the tree and
   pinned — **met**;
7. a neutral verdict moves nothing — **met**;
8. the transport's signal vocabulary and corroboration classes are untouched —
   **met**;
9. no configuration key is added and no default moves — **met**;
10. the lane and the guard are green, every guard rule has an inverted probe
    that fires it, and every rule fails closed on an absent item — **met**;
11. the routine floor is green on the closing tree — **met**;
12. `specs/support.toml` records the surface as `experimental` and
    `advertised = false`; the advertised count stays **zero**;
    `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` stay
    byte-identical; no boundary script is weakened — **met**.

**No stop condition fired.** The contradiction floor needed no change to the
**supporting** floor and no change to the corroboration classes; the firewalled
verdict needed no new signal kind, no new corroboration class and no change to
the supporting threshold; the comparison needed no configuration key and no
second policy; the verdict needed no new task, no new timer and no new
listener; and the plan did **not** turn out to need the production peer-test
driver or the requester side.

## Invariant review

The plan's §4 lists twelve invariants. Each is reviewed against the closing
tree.

1. **One owner, one reader.** The runtime owns the only tracker and the
   product is its only reader. Plan 527's invariant holds unchanged. —
   **held**.
2. **A disagreement is a fact.** It is recorded as
   `PeerTestOutcomeKind::FirewalledLikely` and reaches the tracker; it is
   never an error and never a silent omission. — **held**.
3. **The comparison is against the bound socket.** `bound_endpoint` reads the
   **bound** endpoint and refuses a zero port or an unspecified address. —
   **held**.
4. **One observation is one fact.** A contradiction never also corroborates;
   the corroboration count is unchanged by a contradicting signal. — **held**.
5. **The corroboration floor is unchanged.** `MIN_CORROBORATION_FLOOR` is
   still 2, and `Reachable` still needs three distinct supporting classes. —
   **held**.
6. **The contradiction floor is one.** `MIN_CONTRADICTION_FLOOR` is 1 and is
   validated separately from the corroboration floor. — **held**.
7. **The firewalled verdict is first class.** `is_firewalled_verdict` names
   the peer-test firewalled verdict and the relay-firewalled indication, and
   the state machine consults it. — **held**.
8. **A neutral verdict moves nothing.** `Inconclusive` and `Rejected` support
   nothing and contradict nothing; they are recorded and counted, and the
   state is unchanged. — **held**.
9. **The transport's vocabulary and corroboration classes are untouched.** The
   five signal kinds and the five corroboration classes are unchanged. — **held**.
10. **No configuration key is added and no default moves.** — **held**.
11. **No wire format is added or changed.** The verdict rides the blocks the
    tree already sends. — **held**.
12. **Nothing is promoted.** `router.version` stays `0.9.62` and the
    advertised count stays **zero**. — **held**.

## Failure, cancellation, restart, and contention review

- **Failure.** Every refusal is counted under its own reason and leaves no
  state behind. A poisoned state mutex degrades to "no comparison", never to
  a fabricated agreement: `note_observed_external_address` returns
  `ReachabilityState::Unknown` on a poisoned lock, `note_peer_test_outcome`
  does the same, and `reachability_evidence` returns an empty snapshot rather
  than a claim.
- **Cancellation.** The plan adds **no task**. The verdict path is
  synchronous and non-awaiting, and the controlled driver's own cancellation
  and timeout arms are unchanged — a cancelled exchange is still
  `ExchangeCancelled` and a missed deadline is still `ExchangeTimeout`.
- **Restart.** The router's reachability is recomputed from live evidence, so
  a restart with no verdict is `Unknown` and asks nobody. There is no
  persisted reachability claim and no persisted verdict.
- **Contention.** The plan adds no queue, no task and no lock of its own. The
  comparison happens under the runtime's existing state lock. The lock
  **order** was audited: the only nesting is `state` → `sockets` inside
  `bound_endpoint`, and no code path holds `sockets` while taking `state`,
  so the order is not inverted and no deadlock can form. The controlled
  driver's single-flight context is untouched.

## Migration and compatibility review

- **No wire change.** The verdict rides the peer-test messages the tree
  already sends. The transport's blocks, the transport's options and the
  daemon's peer-test message format are untouched.
- **A reading change for the router.** The router's reachability can now
  leave `ObservedUnconfirmed` on a **single** contradicting fact, and can
  reach `Firewalled` and `Unreachable`. A router that was `Reachable` can
  become `Firewalled`, and it will then ask an introducer for a relay tag.
  That is the intended behaviour, and it is the behaviour Plan 524's and
  Plan 525's compositions were written for.
- **A widened API.** `ReachabilityPolicy::new` takes a fourth argument — the
  contradiction floor — and `ControlledPeerTestOutcome` grew five variants.
  Every in-tree caller was updated. No external crate used them.
- **No configuration migration.** No key is added, renamed or removed.

## Security review

- **No secret is logged.** The verdict is a **classification**, not a payload.
  The lane and the guard print states, counts and digests only; no key, tag,
  peer hash, signature or payload is ever printed.
- **No one local condition is written into a peer's profile.** The comparison
  is between the router's own bound address and what a peer reported about
  **this** router. Nothing is written into any peer's record.
- **A contradiction is never also a corroboration.** A single observation
  cannot both support and deny the router's reachability, so a peer cannot
  inflate the router's claim.
- **The contradiction is earned, not asserted.** The disagreement must come
  from the router's **bound** socket, and a family with no bound socket
  refuses the comparison. A router never invents a contradiction it cannot
  support, and a neutral verdict never invents one at all.
- **The specification's ambiguous row is read conservatively.** "FIREWALLED or
  SYMNAT" is neither a confirmation nor a firewalled verdict, so the tree
  claims nothing for it. A router never claims to be firewalled on a row the
  specification itself calls ambiguous.
- **Fail closed throughout.** An unknown family records nothing, a poisoned
  lock degrades to "no comparison", the gate refuses with `NotFirewalled` and
  counts the refusal, the controlled activation refuses a non-confirming
  verdict, and the guard's absent-item case is a report.
- **The verdict is a fact, not a claim.** The driver no longer returns an
  error for a verdict it received, so a caller cannot mistake "the router is
  behind NAT" for "the test failed" — and the daemon's proof is still minted
  only from a real confirmation.

## Limitations

- The **production** SSU2 peer-test driver stays unbuilt, so the live router
  still cannot be contradicted in the field. The model, the recorder and the
  verdict path are built here.
- The delivery is confined to the **controlled loopback profile**: the
  comparison is between two loopback addresses, so the contradiction is a
  qualification claim.
- The **requester side** stays unwired, so the relay-success route to a
  contradiction is not taken in the field. The peer-test route is the one
  this plan delivers.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology. The plan's evidence is a local lane.
- The runtime's **direct** form still needs three corroboration classes and
  no lane in this tree earns them.
- The lane drives the runtime's **producer** over a real loopback socket; it
  does not drive a **peer** that produces the field indication. A stock-peer
  observation is not claimed.

## Findings by severity

### Medium — none open

### Low — recorded, not fixed

1. **The production SSU2 peer-test driver is unbuilt.** The tree's live router
   therefore still cannot be contradicted in the field; the model and the
   recorder are built, the driver is not. Its choreography question is named:
   the tree's Alice role needs the helper's identity up front, while the
   specification has Bob select the helper. **Owner:** the successor that
   builds the production driver. **Not this plan's scope.**
2. **The requester side of the SSU2 relay protocol has no production
   caller.** `start_relay_request` and `on_relay_response`
   (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:481` and `:497`) are
   unwired, so the relay-success indication is never produced in the field.
   Plan 527's finding, unchanged. **Owner:** the successor that implements the
   requester side. **Not this plan's scope.**
3. **The runtime's direct form still needs three corroboration classes** and
   no lane in this tree earns them. Plan 526 recorded this and it is
   unchanged. **Owner:** the plan that earns the direct form.
4. **The delivery is confined to the controlled loopback profile.** The
   comparison is between two loopback addresses, so the contradiction is a
   qualification claim. **Owner:** the plan that exercises the model against
   a stock peer.

### Informational

1. Plan 527's invariant 9 claimed the transport's reachability model was
   byte-identical; this plan changed the **policy** and the `Firewalled`
   branch's question, so the claim no longer holds of the tree. The
   correction lives here and in the registry row, never in Plan 527's record.
2. Plan 527's guard rule
   `the_runtime_compares_with_the_bound_address` needed an **extension**, not
   a weakening, and the extension is recorded in the plan's §17.4.
3. The plan's own production change moved lines in four files, so the plan's
   citations were corrected by an **additive** pointer table in §17.3; §3
   itself is evidence and was not rewritten.
4. The transport's `firewalled_classes` helper is **removed** and
   `is_firewalled_verdict` replaces it: the branch now asks whether any
   contradicting signal is a firewalled **verdict**, not how many relay
   indications there are.

## Roadmap disposition

The **network-participation** roadmap gains this row:

| Plan | Subject | Status | Outcome |
| --- | --- | --- | --- |
| 528 | the router's peer-test verdict must not be thrown away | closed | passed-the-routers-peer-test-verdict-is-a-fact-not-an-error |

The roadmap's SSU2 introducer composition now reads, in order: Plan 490 gave
the router the role, Plan 523 corrected the **form**, Plan 524 delivered the
**acquisition**, Plan 525 the **publication**, Plan 526 the
**advertisement**, Plan 527 made the state those five depend on
**earnable**, and Plan 528 made the **verdict** that earns it a fact. The
composition's own row in the roadmap is unchanged, because the stock-peer half
of every one of those plans remains blocked on Plan 493's authorized two-host
topology.

## Unblock audit

| Item | State |
| --- | --- |
| Plan 528 | **closed** — all twelve acceptance criteria met, the floor green on the closing tree |
| The SSU2 introducer composition's stock-peer half | **blocked** on Plan 493's authorized two-host topology |
| The production SSU2 peer-test driver | **unbuilt** — the model, the recorder and the verdict path exist; the driver is the successor's subject |
| The requester side of the SSU2 relay protocol | **unwired** — the recorder exists, the producer does not; its owner is named |
| The runtime's direct form | **not yet earned** — three corroboration classes are needed and no lane in this tree earns them |
| Plans 433, 435-439, 493, 494 | **blocked** on external environments; untouched by this plan |

No blocked work was done under this plan's number.

## Handoff notes

- The plan's subject is the router's **own** peer-test verdict: a verdict
  that disagrees with the router's bound address is a **fact**, not an
  error.
- The plan corrects the **negative** side of the reachability model and
  leaves the **positive** side exactly as it was.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **requester side**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer half**
  all stay out of scope, with their owners named.
- The next free plan number is **529**.
