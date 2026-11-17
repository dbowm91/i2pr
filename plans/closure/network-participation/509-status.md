## Status

**Plan 509 — the lookup-target API-version decision — is closed.**

Plan 503 recorded as a Medium finding that *the router still asks a
below-floor floodfill for a lookup*, and left it open: a `DatabaseLookup` is
neither a tunnel build nor a DSM, so the API-version gate was never extended
to the lookup path, and extending the floor to the lookup path *would change
which floodfills the router can ask at all* — which is a product decision,
and therefore a plan-of-record's work.

**Plan 509 makes that decision: the lookup target is NOT version-gated.** The
router asks the floodfills its own routing policy selects, and every answer
is validated by the router before it is trusted.

The plan delivers three things:

- **the decision on the record**, with six reasons, each traced to the tree;
- **the proof**, a feasibility artefact that drives the real lease-set2
  lookup path and establishes that the responder's version decides nothing;
- **the enforcement**, a static guard that makes the decision structural, so
  it cannot drift, and that checks its own **premise** so a change to the
  specification's table is reported rather than assumed away.

**The plan changes no production behaviour, and that is the outcome, not a
shortfall.** The finding was a question — *should the lookup be gated?* — and
the plan answers it with a sourced decision, an executable proof and an
enforcing guard. The diff proves the behavioural claim: the router asks the
same floodfills, in the same order, with the same messages.

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised count
stays **zero** (101 surfaces, 95 experimental, 5 deferred, 1 ready);
`Cargo.toml` and `Cargo.lock` are byte-identical.

**No stock-peer evidence is claimed.** The decision is proved over the tree's
own deterministic surfaces. Whether a real network's floodfills publish
versions the router can compare is **unproven**, and the external lanes stay
open until Plan 493's authorized two-host topology exists.

## Commits

| Commit | Subject |
| --- | --- |
| `93f579c0` | docs(plans): register Plan 509, the lookup-target API-version decision |
| `280288cb` | test(netdb): prove Plan 509's lookup-target decision and guard it |
| `bbff2aeb` | docs(specs): record Plan 509's lookup-target API-version posture |
| `e56d6dc9` | test(netdb): sharpen Plan 509's refusal rows and add the destination binding |
| `<closure>` | docs(plans): close Plan 509 and advance the network-participation roadmap |

## What changed

### The gap

Plan 503 closed Plan 491's two obligations — the minimum peer version for
tunnel building and the minimum floodfill version for a DSM — and made the
peer-eligibility receipt a live product. It recorded, and deliberately did
**not** discharge, a third question:

> **The lookup-target selection is not gated.** The router still asks a
> below-floor floodfill for a *lookup*. A `DatabaseLookup` is not a DSM and
> is not one of Plan 491's two obligations, so the gate is not extended to
> it here. This is an open finding, not an oversight: extending the floor to
> the lookup path would change which floodfills the router can ask at all,
> which is a product decision beyond this plan.

The finding was registered, owned, and named as *"the next candidate on this
line"*. Plan 509 is that plan-of-record.

### The decision, and why it is the right one

**No floor is derivable, and a floor that cannot be derived may not be
invented.** The specification's API-version table attaches exactly two
duties to the `0.9.62` row — *minimum peers will build tunnels through, as
of 0.9.68* and *minimum floodfill peers will send DSM to, as of 0.9.68*. A
`DatabaseLookup` is neither. `i2pr_proto::MinimumPeerObligation` carries
only `constrains_tunnel_peers` and `constrains_dsm_targets`;
`i2pr_proto::VersionFloors` carries only `min_peer_for_tunnel_build` and
`min_floodfill_for_dsm`; and `i2pr_netdb::VersionGate` has exactly two
variants, `TunnelBuildPeer` and `DsmTarget`, because the specification
imposes exactly two duties. A lookup floor would have to be **invented**,
and this repository's rule is that a floor is **derived**, never
hand-maintained.

**The two existing floors encode duties *on the peer*, and they do not
transfer.** Each floor says that a peer **will not** do something: a
below-floor peer will not build tunnels through the router, and will not
send the router's DSM onward. The router's refusal is therefore exactly
*do not ask a peer for something it will not do*. The specification imposes
no such duty for a lookup, so there is no peer-side behaviour to refuse
against.

**The responder's version buys no safety.** The lookup's answer is a
`DatabaseStore`, and the router validates it itself —
`handle_database_store_lease_set2` runs it through
`ValidatedLeaseSet2::from_lease_set2` with the destination it was asked
for — before it is trusted. A below-floor peer that answers correctly is
answered to; one that answers incorrectly is refused by the validation. The
refusal is the router's own answer check, not a verdict on the peer.

**The rotation absorbs an unhelpful candidate.** The lookup state machine
excludes already-queried peers and re-selects from the store on each
attempt, so a candidate that cannot answer costs one round trip and the
lookup moves on. The rotation is bounded by the policy's candidate ceiling
and the peer budget, and it terminates.

**The two reference implementations disagree, so the router does not pick a
side.** i2pd refuses to *file* a RouterInfo that advertises the floodfill
capability below `router.version` **0.9.62**
(`specs/QUALIFICATION-GATES.md:161`). Java's `0.9.62` is a **different**
rule — a *peer-exclusion* floor, `shouldExclude`, which asks whether the
peer will select *us* — and Java's floodfill-profile predicate
`shouldCreate` requires the `f` capability with **no version floor at all**
(`specs/QUALIFICATION-GATES.md:204`). Neither gates a lookup by version, and
both publish `router.version` **0.9.70**
(`plans/closure/network-participation/491-status.md:721`).

**The decision changes no production behaviour.** The router asks the same
floodfills, in the same order, with the same messages.

### The proof

`crates/i2pr-netdb/tests/lookup_target_version_decision.rs` drives the
**real** lease-set2 lookup path through its public surface only. Six rows:

| Row | What it establishes |
| --- | --- |
| `a_below_floor_floodfills_correct_answer_completes_the_lookup` | the responder's version is irrelevant: a below-floor peer that answers correctly completes the lookup |
| `an_incorrect_answer_is_refused_by_the_validation_not_by_the_version` | the peer was **asked** — a version gate would have stopped the request — and the answer was then refused on its own merits |
| `an_answer_for_another_destination_is_refused` | the answer is bound to the destination the lookup asked for |
| `a_silent_below_floor_floodfill_is_skipped_and_the_lookup_completes` | an unhelpful candidate costs one round trip, not the lookup, and it is never re-asked |
| `a_peer_that_publishes_no_version_is_asked_for_a_lookup_too` | the second unreachable class behaves the same way |
| `a_gated_selection_would_have_refused_the_peer_the_lookup_asked` | the inverted probe: the claim is not vacuous — a version-gated selection **would have refused** the very peer the lookup asked |

### The enforcement

`crates/i2pr-daemon/tests/lookup_target_version_boundary.rs` makes the
decision structural, so it cannot drift:

1. **the lookup path is ungated** — the daemon's `begin_lease_lookup_inner`
   and the state machine's `start`, `start_with_preferred` and
   `advance_active` all select through the nearest-floodfill selection, and
   the daemon's lookup body takes **no** version decision at all;
2. **the premise holds** — the specification's obligation table carries no
   lookup duty, the derived floors carry no lookup floor, and the gate
   vocabulary has exactly two variants;
3. **the answer is validated** — the answer path still runs through
   `ValidatedLeaseSet2::from_lease_set2`;
4. **the rotation skips** — the state machine still applies its
   already-queried and excluded sets.

Every rule carries its own inverted probe, the guard is fail-closed, and it
strips comments and `#[cfg(test)]` blocks before matching.

### Three things the change deliberately does not do

- **It does not add a third gate.** `VersionGate` and
  `VersionDecisionKind` keep exactly two variants, because the
  specification's table has two duties. A third variant would be a third
  duty that does not exist.
- **It does not change the store's notion of a floodfill advertiser.** The
  two reference implementations disagree about that, so the router must not
  invent a rule. The DSM gate already refuses to *send* to a below-floor
  floodfill (Plan 503), and the lookup's answer is validated.
- **It does not touch the peer-profile store's read half** (Plan 497
  limitation 5), which remains bound to Plan 433's blocked composition.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| The lookup target is not version-gated | `crates/i2pr-daemon/tests/lookup_target_version_boundary.rs`, rule 1 |
| No floor is derivable for a lookup | the same, rule 2 |
| The gate vocabulary has exactly two variants | the same, rule 2 |
| The answer is validated by the router | the same, rule 3; and `crates/i2pr-netdb/tests/lookup_target_version_decision.rs` |
| The rotation skips an unhelpful candidate | the same, rule 4; and the artefact |
| The decision changes no production behaviour | the commit's own diff: no production file is touched |
| Nothing is advertised | `specs/support.toml` — one experimental surface, `advertised = false`; the inventory's advertised count stays zero |
| The decision is on the record | `docs/architecture/i2pr-daemon.md`, `specs/CONFORMANCE.md`, `specs/support.toml` |

## Commands run

### The routine floor

The closing floor is **118 steps**, run on the closing tree, and it passed
**118 of 118**. The script is
`/tmp/i2pr-floor-509-run1.sh`; the log is `/tmp/i2pr-floor-509-run1.log`.
It is Plan 508's 116-step floor plus two Plan 509 steps
(`plan509-lookup-target-decision` and `plan509-lookup-target-boundary`).

| # | Step | Result | Seconds |
| --- | --- | --- | --- |
| 1 | `cargo-fmt` | PASS | 3 |
| 2 | `cargo-check` | PASS | 7 |
| 3 | `cargo-build-siblings` | PASS | 0 |
| 4 | `cargo-build-static-fixture` | PASS | 0 |
| 5 | `cargo-copy-static-fixture` | PASS | 1 |
| 6 | `cargo-rebuild-plain-fixture` | PASS | 0 |
| 7 | `cargo-test-workspace` | PASS | 1340 |
| 8 | `cargo-clippy` | PASS | 15 |
| 9 | `cargo-doc` | PASS | 11 |
| 10 | `cargo-test-doc` | PASS | 4 |
| 11 | `check-dependency-direction` | PASS | 0 |
| 12 | `check-global-plan-number-uniqueness` | PASS | 0 |
| 13 | `check-adr-number-uniqueness` | PASS | 0 |
| 14 | `check-portable-service-tunnel-api` | PASS | 0 |
| 15 | `check-portable-service-tunnel-consumer` | PASS | 6 |
| 16 | `planning-tests` | PASS | 7 |
| 17 | `check-runtime-boundaries` | PASS | 1 |
| 18 | `check-console-boundaries` | PASS | 1 |
| 19 | `check-console-browser-security` | PASS | 0 |
| 20 | `check-tooling-inventory` | PASS | 0 |
| 21 | `check-license-metadata` | PASS | 0 |
| 22 | `check-managed-app-private-client-seams` | PASS | 0 |
| 23 | `check-router-readiness-contract` | PASS | 0 |
| 24 | `check-router-readiness-contract-self` | PASS | 0 |
| 25 | `check-core-router-recovery-contract` | PASS | 0 |
| 26 | `check-core-router-recovery-contract-self` | PASS | 0 |
| 27 | `plan449-ssu2-preflight-self` | PASS | 0 |
| 28 | `plan495-peer-eligibility-self` | PASS | 1 |
| 29 | `plan495-peer-eligibility-profile-drift` | PASS | 0 |
| 30 | `plan476-gate-binding` | PASS | 0 |
| 31 | `plan476-gate-binding-self` | PASS | 1 |
| 32 | `check-managed-app-package-boundary` | PASS | 0 |
| 33 | `check-managed-app-package-boundary-self` | PASS | 0 |
| 34 | `check-managed-app-policy-boundary` | PASS | 0 |
| 35 | `check-managed-app-policy-boundary-self` | PASS | 0 |
| 36 | `check-managed-app-secured-sandbox` | PASS | 0 |
| 37 | `check-managed-app-secured-sandbox-self` | PASS | 0 |
| 38 | `check-service-tunnel-boundaries` | PASS | 0 |
| 39 | `check-managed-app-gateway-boundary` | PASS | 0 |
| 40 | `check-managed-app-manager-boundary` | PASS | 0 |
| 41 | `check-m11-per-epoch-composition` | PASS | 0 |
| 42 | `check-service-anonymity-boundaries` | PASS | 0 |
| 43 | `check-fixture-manifest` | PASS | 0 |
| 44 | `check-ntcp2-vectors` | PASS | 0 |
| 45 | `check-ssu2-vectors` | PASS | 0 |
| 46 | `check-i2cp-vectors` | PASS | 2 |
| 47 | `check-ntcp2-interoperability` | PASS | 1 |
| 48 | `check-constrained-host-lane-boundary` | PASS | 0 |
| 49 | `check-m11-transit-boundaries` | PASS | 1 |
| 50 | `check-m11-transit-qualification-evidence` | PASS | 1 |
| 51 | `check-sam-acceptance-evidence` | PASS | 0 |
| 52 | `check-ssu2-acceptance-evidence` | PASS | 0 |
| 53 | `check-i2cp-acceptance-evidence` | PASS | 0 |
| 54 | `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| 55 | `check-els2-type11-transcript-boundary` | PASS | 1 |
| 56 | `check-els2-live-lane-evidence` | PASS | 1 |
| 57 | `check-encrypted-service-consumer-caller` | PASS | 0 |
| 58 | `check-outproxy-request-path` | PASS | 1 |
| 59 | `check-outproxy-wire-lane-evidence` | PASS | 0 |
| 60 | `check-config-secret-hygiene` | PASS | 0 |
| 61 | `check-workflow-validity` | PASS | 0 |
| 62 | `check-floodfill-type5-serve` | PASS | 0 |
| 63 | `check-service-tunnel-acceptance-evidence` | PASS | 0 |
| 64 | `check-exploratory-tunnel-evidence` | PASS | 1 |
| 65 | `check-netdb-tunnel-evidence` | PASS | 0 |
| 66 | `check-destination-tunnel-evidence` | PASS | 0 |
| 67 | `check-streaming-tunnel-evidence` | PASS | 5 |
| 68 | `check-m6-mixed-router-acceptance-evidence` | PASS | 3 |
| 69 | `check-m12-floodfill-qualification-evidence-self` | PASS | 0 |
| 70 | `check-m12-floodfill-boundaries-self` | PASS | 0 |
| 71 | `ntcp2-harness-tests` | PASS | 0 |
| 72 | `plan488-mesh-lane` | PASS | 1 |
| 73 | `plan488-evidence-deterministic` | PASS | 0 |
| 74 | `plan489-mesh-lane` | PASS | 0 |
| 75 | `plan489-evidence-deterministic` | PASS | 0 |
| 76 | `plan490-mesh-lane` | PASS | 0 |
| 77 | `plan490-evidence-deterministic` | PASS | 0 |
| 78 | `plan490-ssu2-transport-suite` | PASS | 7 |
| 79 | `plan490-transport-boundary-guard` | PASS | 0 |
| 80 | `plan491-mesh-lane` | PASS | 0 |
| 81 | `plan491-evidence-deterministic` | PASS | 0 |
| 82 | `plan491-boundary-guard` | PASS | 0 |
| 83 | `plan491-proto-suite` | PASS | 1 |
| 84 | `plan491-netdb-suite` | PASS | 92 |
| 85 | `plan491-client-suite` | PASS | 27 |
| 86 | `plan492-posture-suite` | PASS | 1 |
| 87 | `plan492-m11-lane` | PASS | 5 |
| 88 | `plan502-transit-composition-lane` | PASS | 0 |
| 89 | `plan502-transit-composition-unit` | PASS | 1 |
| 90 | `plan502-transit-boundary-self` | PASS | 0 |
| 91 | `plan496-private-lan-profile` | PASS | 0 |
| 92 | `plan496-ntcp2-activation-gate` | PASS | 0 |
| 93 | `plan497-routing-consumer-lane` | PASS | 1 |
| 94 | `plan497-netdb-preferred-order` | PASS | 0 |
| 95 | `plan497-daemon-participation-unit` | PASS | 3 |
| 96 | `plan498-introducer-handoff-lane` | PASS | 1 |
| 97 | `plan498-introducer-mesh-lane` | PASS | 0 |
| 98 | `plan498-bootstrap-resolver-row` | PASS | 26 |
| 99 | `plan498-runtime-relay-accessor` | PASS | 5 |
| 100 | `plan503-version-eligibility-live` | PASS | 1 |
| 101 | `plan503-version-eligibility-unit` | PASS | 0 |
| 102 | `plan504-liveness-boundary` | PASS | 0 |
| 103 | `plan504-liveness-live` | PASS | 0 |
| 104 | `plan504-liveness-unit` | PASS | 0 |
| 105 | `plan505-driver-boundary` | PASS | 1 |
| 106 | `plan505-driver-live` | PASS | 2 |
| 107 | `plan505-driver-unit` | PASS | 1 |
| 108 | `plan506-reconcile-boundary` | PASS | 5 |
| 109 | `plan506-reconcile-live` | PASS | 1 |
| 110 | `plan507-retention-feasibility` | PASS | 1 |
| 111 | `plan507-retention-boundary` | PASS | 1 |
| 112 | `plan507-retention-live` | PASS | 0 |
| 113 | `plan508-creator-retention-feasibility` | PASS | 1 |
| 114 | `plan508-creator-retention-boundary` | PASS | 1 |
| 115 | `plan508-creator-retention-live` | PASS | 0 |
| 116 | `plan509-lookup-target-decision` | PASS | 1 |
| 117 | `plan509-lookup-target-boundary` | PASS | 0 |
| 118 | `cargo-deny` | PASS | 1 |

**118 of 118 steps passed, 0 failed.** The steps Plan 509 adds are the
feasibility artefact and the static guard.


### The focused verification

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked -p i2pr-netdb -p i2pr-daemon --all-targets` | PASS |
| `cargo clippy --locked -p i2pr-netdb -p i2pr-daemon --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-netdb --test lookup_target_version_decision -- --test-threads=1` | PASS — 6 |
| `cargo test --locked -p i2pr-daemon --test lookup_target_version_boundary -- --test-threads=1` | PASS — 10 |
| `cargo test --locked -p i2pr-netdb --test lookup_peer_budget --test lease_set2_integration -- --test-threads=1` | PASS — 7 + 8 |
| `cargo test --locked -p i2pr-daemon --test version_floor_boundary --test destination_tunnel_unit -- --test-threads=1` | PASS — 28 + 51 |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 |
| `bash scripts/check-dependency-direction.sh` | PASS |

### The four negative controls, verified on the real tree

Each control was applied to the **real** production source, run against the
**real** lanes, and reverted; the files were restored **byte-for-byte** after
each, and the restoration was asserted. All four broke an assertion; none
broke the build.

| Control | Mutation | Failing tests | Named failures |
| --- | --- | --- | --- |
| A | the rotation re-asks the silent candidate | 5 | `a_silent_below_floor_floodfill_is_skipped_and_the_lookup_completes`, `the_rotation_rule_fires_when_the_rotation_re_asks`, `the_landed_tree_holds_every_rule`, `every_rule_is_reachable_and_the_tree_holds`, `the_guard_ignores_comments_and_test_blocks` |
| B | the answer's destination binding is dropped | 1 | `an_answer_for_another_destination_is_refused` |
| C | the lookup path consults the version-gated selection | 5 | `the_ungated_rule_fires_when_the_lookup_falls_back_to_the_gated_selection`, `the_version_decision_rule_fires_when_the_lookup_asks_for_one`, `the_landed_tree_holds_every_rule`, `every_rule_is_reachable_and_the_tree_holds`, `the_guard_ignores_comments_and_test_blocks` |
| D | an expired record is no longer refused | 1 | `an_incorrect_answer_is_refused_by_the_validation_not_by_the_version` |

**Control D was vacuous on its first run, and that is recorded.** The expired
record the artefact used was refused by the *lease* expiry check as well as
by the record's own expiry check, so dropping the record-expiry check changed
the code and not the observable, and the control reported zero failures. The
detector caught it. The record now carries a **live** lease, so the record's
own expiry is the only check that fails, and the control fails exactly one
test.

### Five weaknesses found while building the controls and the guard

They are recorded because each one **would have made a control or a probe
pass vacuously**.

1. **The control detector's build-failure matcher was too loose.** It
   matched any line starting with `error`, and the test runner's own
   `error: test failed, to rerun pass …` footer matched it, so every control
   with failing tests reported one **spurious** build failure and its count
   was inflated by one. The matcher now accepts a compiler error or an abort
   only. This is Plan 508's weakness 5, carried forward and fixed here
   before the controls were run.
2. **A control that changes the code but not the observable is vacuous.**
   Control D was exactly that, on its first run. The detector's report of
   zero failures is what exposed it, and the fix was to make the record's own
   expiry the **only** failing check.
3. **Three of the guard's probes did not apply.** The daemon's selection is
   wrapped across two lines, so the probes' anchors did not match the real
   source and the mutations silently did nothing. The probes failed loudly,
   because the mutation is asserted, and were retargeted at the lookup's own
   arguments.
4. **The artefact's fourth row's name did not match what it did.** The row
   was named for the *unknown-version* class while the peer published a
   below-floor version. It was found by reading the row back, and the
   unknown-version class now has a row of its own.
5. **The support inventory's new surface did not parse.** The notes value
   was written without its quotes, so the TOML was invalid. The tooling
   inventory check and a `tomllib` parse caught it, and the value was
   repaired.

## Invariant review

| Invariant | Held? | Evidence |
| --- | --- | --- |
| The router's own traffic is never claimed as transit work, and every answer the router trusts is validated by the router | yes | the artefact's refusal rows; the guard's rule 3 |
| A lookup never asks a peer the router has no authority to ask, and never asks a peer twice | yes | the artefact's rotation row; the guard's rule 4 |
| The API-version floors remain **derived** from the specification's table; no floor is hand-maintained and no floor is added for the lookup path | yes | the guard's rule 2; no production file is touched |
| `VersionGate` and `VersionDecisionKind` keep exactly two variants | yes | the guard's rule 2 |
| The receipts keep carrying counts and stable reason labels only | yes | Plan 509 adds no receipt and no counter |
| The declared `router.version` stays `0.9.62`; `caps` and the published RouterInfo are unchanged; the advertised count stays zero | yes | the support inventory; no production file is touched |
| No new configuration key, dependency, task, timer, listener, socket, or production wire change | yes | the commit's own diff; `Cargo.toml` and `Cargo.lock` are byte-identical |

## Failure review

| Condition | Behaviour |
| --- | --- |
| The lookup finds no candidate | the daemon terminates honestly with `NoEligibleCandidates` and counts the failure — unchanged by Plan 509 |
| A candidate does not answer | the rotation skips it and moves on; the lookup is never re-asked the same peer — proved by the artefact and guarded by rule 4 |
| A candidate answers with a record that fails validation | the answer is refused, the record is never installed, and the lookup stays active so it may ask somebody else — proved by the artefact |
| A candidate answers with a record for another destination | the answer is refused on the destination binding — proved by the artefact |
| The specification's table gains a lookup duty | the guard's premise rule fires, and the decision must be re-made on the record |
| The lookup path starts taking a version decision | the guard's rule 1 fires, with the reason named |

## Migration review

None. Nothing is persisted, nothing is renamed, nothing is reinterpreted, and
no configuration key changes meaning. The plan adds no data format, no
protocol message, and no wire change: a `DatabaseLookup` is byte-identical
before and after, and the router asks the same floodfills in the same order.

## Security review

- **No new attack surface.** The plan changes no production behaviour, so the
  router's exposure is unchanged. The lookup path already asked below-floor
  floodfills before Plan 509 and still does; the difference is that the
  decision is now *made*, *proved* and *enforced* rather than deferred.
- **The decision does not weaken the answer path.** The gate's premise is
  that what the router trusts is the answer's validity. Plan 509 makes that
  premise explicit and proves it: the answer is validated, the destination
  binding holds, and an expired record is refused.
- **The refusal remains the router's own.** A below-floor peer that answers
  incorrectly is refused by the validation, so the peer's version buys it
  nothing.
- **No secret is logged, printed, or serialised.** The plan adds no
  diagnostic and no receipt. The artefact and the guard read source files and
  drive the engine; neither retains a peer identity beyond the test's own
  local variables.
- **The guard reads only the repository's own source.** It opens no socket
  and reaches no network.

## Documentation and operations

- `docs/architecture/i2pr-daemon.md` — the lookup path's API-version posture
  section, with the six reasons and the selection sites.
- `specs/CONFORMANCE.md` — the Plan 509 section, stating what is claimed and
  what is not.
- `specs/support.toml` — one `experimental` surface,
  `netdb.lookup-target-version-posture`, with `advertised = false`, and the
  plan's metadata keys. The inventory is now **101 surfaces, 95 experimental,
  5 deferred, 1 ready, 0 advertised**.
- `plans/registry.md` and `plans/subsystems/network-participation-roadmap.md`
  — the Plan 509 rows.

**No operator action.** There is nothing to configure, nothing to observe,
and nothing to roll back.

## Limitations

1. **The decision is proved over the tree's own surfaces, not against a
   real network.** Whether a real network's floodfills publish versions the
   router can compare, and whether a real below-floor floodfill would answer
   usefully, is **unproven**. The external lanes stay open until Plan 493's
   authorized two-host topology exists.
2. **The plan does not answer whether a real floodfill can *hold* the record
   the router wants.** The specification's feature rows — which record when
   LeaseSet2 support appeared — are **not** in the tree, and the reference
   implementations disagree about the version rule, so no capability floor
   could be derived. If the specification's feature table is ever brought
   into the tree, a *capability* question opens alongside this one; it is a
   different question from the *duty* question this plan decided.
3. **The reference implementations' disagreement is recorded, not resolved.**
   The router's own rule is capability plus validity. If i2pd and Java are
   brought into agreement, the question would need re-deciding.
4. **The plan does not touch the peer-profile store's read half** (Plan 497
   limitation 5), which stays bound to Plan 433's blocked composition.
5. **Plan 509's artefact drives the lease-set2 lookup.** The RouterInfo
   lookup path shares the same engine, the same selection and the same
   rotation, and is ungated for the same reasons, but it has no row of its
   own.

## Findings by severity

### Medium

- **The daemon deep-dive's module-table line counts are stale for 33 of the
  63 rows that carry one** (measured by comparing every row against the
  file). Plan 508 measured **36**, updated the **three** it changed, and
  left the rest: the drift is a separate concern with its own
  plan-of-record, and correcting thirty-three unrelated rows here would
  bury the change. **Not fixed.** Plan 509 changes no production file, so it
  changes no row.

### Low

- **Control D was vacuous on its first run**, and the detector caught it. The
  artefact's expired record was refused by the *lease* expiry check as well
  as by the record's own expiry check, so the control changed the code and
  not the observable. The record now carries a live lease, so the record's
  own expiry is the only failing check. Recorded above.
- **Three of the guard's probes did not apply** on their first run, because
  their anchors did not match the real source's line wrapping. The probes
  failed loudly — the mutation is asserted — and were retargeted.
- **The artefact's fourth row's name did not match what it did.** It was
  found by reading the row back, and the class it was named for now has a
  row of its own.
- **The support inventory's new surface did not parse** on the first write,
  because the notes value lost its quotes. The tooling inventory check and a
  `tomllib` parse caught it.

### Recorded, not defects

- **The plan changes no production behaviour, and that is the outcome.** The
  finding was a question, and the plan answers it. The plan's deliverable is
  the decision, its proof and its enforcement.
- **Plan 503's closure record is not rewritten.** The finding is discharged
  here and in the registry row, as the repository's rule requires.
- **The plan of record is not rewritten.** The correction — that the plan's
  §6 "no production change" table and its §8 work packages are the ones
  that shipped — lives here and in the registry row.

## Roadmap disposition

The network-participation roadmap's Plan 509 row is **closed** as
`passed-lookup-target-api-version-decision`, and its §12 status sentence
names the decision. Plan 503's recorded finding is **discharged**: it is
closed by a decision on the record, with the proof and the guard, and the
registry's row says so.

## Unblock audit

Plan 503's closure record listed the lookup-target gate as *"New, open. Needs
a plan-of-record."* Plan 509 is that plan-of-record, and the finding is now
**discharged**. No other registered plan lists Plan 503's finding as a hard
dependency, so no other row moves.

The plans that remain blocked are blocked on **external environments** and
are unaffected by this plan: Plans 433, 435, 436, 437, 438, 439, 493 and
494. Plan 436's source-side work was discharged by Plan 502.

The source-side candidates that remain on this line are named in the plan of
record's §16: Plan 491 **F4** (the intermittent `i2pcontrol_tunnels`
staging failure, which belongs to the I2PControl / service-tunnel line),
Plan 487 **F3**'s destination-inbound half (which needs tunnel rekeying — a
new I2NP mechanism with its own plan-of-record), and Plan 497 limitation 5's
**read** half (which is bound to Plan 433's blocked composition).
