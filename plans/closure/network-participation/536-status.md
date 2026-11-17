
# Plan 536 — the relay target role must answer without holding the router's NetDB lock

**Closure record.** The plan of record is
`plans/implementation/network-participation/536-the-relay-target-role-must-answer-without-holding-the-routers-netdb-lock.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 536 is **closed**. The router's relay **target** role answers an
introduction with **no lock held**: the requester's material is read under a
short, bounded lock, the guard is released before the read returns, and the
accept is signed and staged afterwards. And the target's drain is **never
skipped silently**: it runs whenever the target role exists, and a router that
holds no usable validated store refuses the introduction under a **typed**,
**counted** reason.

Classification: **corrective** (the target role's machinery was complete and
the composition root drained it inside the router's NetDB lock, across up to
sixteen signature operations per pass; and the drain was gated on the daemon
holding a `Bootstrap` at all, so an introduction that arrived and could not be
answered left no trace).
**Subsystem:** network-participation. **Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The defect — the answer was produced inside the router's NetDB lock.**
Plan 532 built the target role: the peer an introducer names receives the
Relay Intro, verifies it through the transport's own target state machine,
signs the accept and stages the hole punch. But the composition root drained
that handoff **inside the bootstrap's mutex** — the router's NetDB lock — for
the whole pass, and the pass is bounded at
`MAX_RELAY_DRAIN_PER_PASS = 16` answers
(`crates/i2pr-daemon/src/ssu2_relay_drain.rs:47`). Each answer parses an
address, signs an accept and stages it on a session, and all of it happened
while the caller still held the lock.

The same mutex is the router's NetDB. It is taken by the NetDB discovery tick
(`crates/i2pr-daemon/src/lib.rs:2063`), by the requester's outbound step
(`crates/i2pr-daemon/src/lib.rs:5658`), by the RouterInfo publication tick,
by the peer reconcile and by the peer pool. So the relay answer path and the
router's NetDB contended, and the loser was whichever arrived second: a router
that relays for others held the NetDB lock across up to sixteen signature
operations, on every tick.

**The second defect — the silent skip.** The drain was gated on the daemon
**holding** a `Bootstrap`. A router that did not hold one did not drain, did
not refuse and did not count: the introductions sat in the handoff and the
operator saw nothing. A skipped drain is indistinguishable from a router that
was never reached. This is the coupling Plan 532 recorded as its **Low**
finding 2 — "the target role needs the bootstrap's store"
(`plans/closure/network-participation/532-status.md:604`) — carried forward
unchanged by Plan 534 (`plans/closure/network-participation/534-status.md:568`).

**The material source is the seam.** `crates/i2pr-daemon/src/relay_target.rs:413`
gains a **daemon-local** trait, `RelayTargetMaterialSource`, with one method,
`read_requester_material`. It is implemented **for the validated store
itself** (`crates/i2pr-daemon/src/relay_target.rs:426`), so the free function
`read_requester_material(&RouterInfoStore, [u8; 32])`
(`crates/i2pr-daemon/src/relay_target.rs:368`) is unchanged — its signature,
its body and its four refusals are exactly what they were — and Plan 532's
lane still compiles and still passes. The target role reads through the source
(`crates/i2pr-daemon/src/relay_target.rs:504`) in its own drive step
(`crates/i2pr-daemon/src/relay_target.rs:489`), and verifies the introduction
through the runtime's own seam (`crates/i2pr-daemon/src/relay_target.rs:521`).

**The guard is released before the read returns.** The **production** source,
`ValidatedStoreMaterialSource` (`crates/i2pr-daemon/src/relay_target.rs:447`),
holds the router's bootstrap by reference, takes the bootstrap's mutex for the
one named peer, and lets the guard drop when the function returns
(`crates/i2pr-daemon/src/relay_target.rs:460`), so the material read itself
(`crates/i2pr-daemon/src/relay_target.rs:474`) returns with no lock held. The accept is therefore signed
and staged with **no lock held** — the pattern Plan 533's establishment task
already follows for the same store
(`crates/i2pr-daemon/src/relay_establishment_task.rs:131`).

**The drain is never skipped silently.** The composition root's relay tick
(`crates/i2pr-daemon/src/lib.rs:5581`) builds the source from
`bootstrap.as_ref()` (`crates/i2pr-daemon/src/lib.rs:5755`) and drains the
target's handoff (`crates/i2pr-daemon/src/lib.rs:5758`) **whenever the target
role exists**. A missing store — or a poisoned lock — refuses the introduction
under the typed reason `StoreUnavailable`
(`crates/i2pr-daemon/src/relay_target.rs:62`, the variant itself at
`crates/i2pr-daemon/src/relay_target.rs:81`), counted under its own reason key
(`crates/i2pr-daemon/src/relay_target.rs:125`) and its own counter
(`crates/i2pr-daemon/src/relay_target.rs:179`) on the owner's counters
(`crates/i2pr-daemon/src/relay_target.rs:148`), recorded by the owner's own
refusal recorder (`crates/i2pr-daemon/src/relay_target.rs:293`).

**The refusal is a fact about this router.** It says that **this** router has
no usable validated store — not that the requester is unknown, not that the
requester is hostile, not that the peer is at fault. It is never written into
the requester's profile, and it never counts as a peer fault.

**The read stays read-only.** One named peer is read, one address is parsed
with the transport's strict parser, two values are derived, and nothing is
mutated and no peer profile is written. The plan changes **where** the read
happens, not what it does.

**The target's semantics are untouched.** The accept's signature preimage, the
hole punch, the dial profile, the transport's target table and the transport's
the transport's refusal order are exactly what the tree already had
(`crates/i2pr-daemon/src/relay_target.rs:547`), and the target's drain
(`crates/i2pr-daemon/src/ssu2_relay_drain.rs:293`) still hands the
introduction to that owner. Plan 532's lane and guard stay
**byte-identical**, so the predecessor's evidence remains exactly as it was,
and Plan 532's guard still finds the strings it looks for.

## What the plan did not close

- The **router's validated store is still the bootstrap's store**
  (`crates/i2pr-daemon/src/bootstrap.rs:215`). Separating the router's NetDB
  from the bootstrap's lifetime is a larger change and is **not** attempted
  here: this plan removes the **lock scope** and the **silent skip**, and
  leaves the ownership alone. **Owner:** the successor that gives the router a
  NetDB owner of its own.
- Plan 532's recorded **Low** finding 2 — "the target role needs the
  bootstrap's store" — is **discharged in its recorded terms and not in the
  terms its prose might suggest**. The target role's *need* for the
  bootstrap's store is unchanged; the coupling that remains is bounded to a
  single-key read with a typed, counted refusal. **Plan 532's closed closure
  record is not rewritten**; the correction lives in this record and in the
  registry's row.
- The **production** SSU2 peer-test driver stays unbuilt. The `B`
  advertisement and `caps=f` stay with their owners.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology. Nothing in this plan needs a stock peer, and none was attempted.
- The delivery stays confined to the **controlled loopback profile** (Plan
  431's bound).

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `268d1639` | the plan of record, the registry's registered row and the roadmap's registered row |
| `b2f045c4` | the production change: the material-source trait, the store's implementation, the production source, the typed refusal and its counter, and the ungated drain |
| `4de3ac96` | the lane and the guard |
| `ea490cac` | the inventory, the conformance record, the qualification gates, the protocol scope, the daemon deep-dive and the plan's amendment log |

The closure record and the closed rows are carried by the commit that adds
this file; its hash is not named here, because a record cannot name the
commit that contains it.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| the answer is produced with **no** lock held, and the lane measures it | `crates/i2pr-daemon/tests/ssu2_relay_target_lock.rs`, `the_answer_is_signed_with_the_netdb_lock_free` |
| the measurement is **not vacuous** | the same lane file, `the_lock_measurement_detects_a_lock_held_across_the_answer` |
| the drain **runs** with no store and counts the typed refusal | the same lane file, `the_drain_runs_without_a_store_and_counts_the_typed_refusal` |
| a **poisoned** lock is the same typed refusal | the same lane file, `a_poisoned_netdb_lock_is_the_same_typed_refusal` |
| the answer is still produced correctly with a store | the same lane file, `the_answer_is_still_produced_correctly_with_a_store` |
| the read mutates nothing and writes no peer profile | the same lane file, `the_read_mutates_nothing_and_writes_no_peer_profile` |
| every refusal is typed and counted under its own reason | the same lane file, `every_refusal_is_typed_and_counted_and_writes_nothing` |
| the drain is **not** gated on the bootstrap | `crates/i2pr-daemon/tests/ssu2_relay_target_lock_boundary.rs`, `the_target_drain_is_not_gated_on_the_bootstrap` |
| the source **releases** the lock before the answer | the same guard file, `the_source_releases_the_lock_before_the_answer` |
| the read is **read-only** | the same guard file, `the_read_is_read_only` |
| the missing store is typed and **counted** | the same guard file, `the_missing_store_is_a_typed_and_counted_refusal` |
| the target's machinery is **untouched** | the same guard file, `the_target_roles_machinery_is_untouched` |
| the transport's target table and refusal order are **untouched** | the same guard file, `the_transports_target_table_is_untouched` |
| **no** new surface, and the drain rides the **existing** tick | the same guard file, `no_new_surface_and_the_drain_rides_the_existing_tick` |
| Plan 532's lane and guard stay **byte-identical** | the same guard file, `the_predecessors_evidence_is_untouched` |
| every rule has an inverted probe and fails closed | the same guard file, the `the_guard_reports_*` and the `the_guard_fails_closed_when_*` rows |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_target_lock -- --test-threads=1` | PASS — 7 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_target_lock_boundary -- --test-threads=1` | PASS — 29 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_target --test ssu2_relay_target_boundary --test ssu2_relay_establishment --test ssu2_relay_establishment_boundary -- --test-threads=1` | PASS — 7 rows, 36 rows, 8 rows and 41 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-license-metadata.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |
| `python3 /tmp/536-negative.py` | PASS — 11 controls, each firing the surface that owns the defect, each restored byte-identically |

## The routine floor

The floor ran on the committed tree `ea490cac` and it is **green**. Every
step is recorded in `/tmp/536-floor.log`, with each step's own log under
`/tmp/536f-<step>.log`. The floor is a plain sequence of `step` runs that
records each step's exit status; no step is skipped, no `|| true`, no
`continue-on-error` and no filename filtering. The workspace step runs with
`--no-fail-fast`, so every test binary's result is reported rather than the
run stopping at the first failure.

Plan 536's floor is Plan 535's floor plus two steps — the daemon lane and the
daemon guard for the relay target role's lock scope. **No other step changed,
and no other plan's guard file was touched.**

| Step | Outcome |
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
| `plan529-observed-address-lane` | PASS |
| `plan529-observed-address-boundary` | PASS |
| `plan530-explicit-bind-lane` | PASS |
| `plan530-explicit-bind-boundary` | PASS |
| `plan531-relay-requester-lane` | PASS |
| `plan531-relay-requester-boundary` | PASS |
| `plan532-relay-target-lane` | PASS |
| `plan532-relay-target-boundary` | PASS |
| `plan533-relay-establishment-lane` | PASS |
| `plan533-relay-establishment-boundary` | PASS |
| `plan534-relay-session-peer-lane` | PASS |
| `plan534-relay-session-peer-boundary` | PASS |
| `plan535-explicit-bind-refresh-lane` | PASS |
| `plan535-explicit-bind-refresh-boundary` | PASS |
| `plan536-relay-target-lock-lane` | PASS |
| `plan536-relay-target-lock-boundary` | PASS |
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

**Total: 111 PASS, 0 FAIL.**

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be
reported. Every probe mutates an **in-memory copy** of the source, so **no
file on disk is written**. Every rule **fails closed** when the item it names
is absent — the guard reports the missing item rather than passing.

### Plan 536's guard — `crates/i2pr-daemon/tests/ssu2_relay_target_lock_boundary.rs`

Eight rules, an inverted probe for every rule, and a fail-closed probe for
every item a rule names. All 29 rows green.

| Rule | What it forbids | Its inverted probe |
| --- | --- | --- |
| 1 | a drain that is gated on the bootstrap, or that holds the NetDB lock across the answer | `the_guard_reports_a_drain_gated_on_the_bootstrap`, `the_guard_reports_a_drain_skipped_when_the_router_holds_no_store` |
| 2 | a source that holds the guard past the material read, an answer that reads a store directly, or an answer that takes the NetDB lock | `the_guard_reports_a_source_that_holds_the_guard_past_the_read`, `the_guard_reports_an_answer_that_reads_a_store_directly`, `the_guard_reports_an_answer_that_takes_the_netdb_lock` |
| 3 | a material read that mutates the router's NetDB | `the_guard_reports_a_read_that_mutates_the_netdb` |
| 4 | an uncounted refusal, or a refusal under the wrong reason | `the_guard_reports_an_uncounted_store_refusal`, `the_guard_reports_a_store_refusal_with_the_wrong_reason` |
| 5 | an accept signed over another preimage | `the_guard_reports_an_accept_signed_over_another_preimage` |
| 6 | a changed transport refusal order | `the_guard_reports_a_changed_transport_refusal_order` |
| 7 | a second configuration key, or an owner that owns a socket | `the_guard_reports_a_second_configuration_key`, `the_guard_reports_an_owner_that_owns_a_socket` |
| 8 | a predecessor file that is not byte-identical | `the_guard_reports_a_changed_predecessor_lane` |

Eight further probes remove the item each rule names — the target's drain, the
production source, the material read, the refusal vocabulary, the answer step,
the transport's verification, the configuration surface and the predecessor's
guard — and every rule reports the absence rather than passing. They are the
`the_guard_fails_closed_when_*` rows.

### The negative control against the **real** tree

The guard's probes are in-memory. The tree was therefore also mutated **on
disk** — one mutation at a time, each restored byte-identically — and the
guard and the lane were re-run. The script is `/tmp/536-negative.py`.

| Mutation | This plan's guard | The lanes |
| --- | --- | --- |
| NC-1 — the target's drain is gated on the bootstrap again (compiled=True) | **FAILED**, as it must (`the_target_drain_is_not_gated_on_the_bootstrap`) | green -- the lane drives the production source directly |
| NC-2 — the target's drain is skipped when the router holds no store (compiled=True) | **FAILED**, as it must (`the_target_drain_is_not_gated_on_the_bootstrap`) | green -- the lane drives the production source directly |
| NC-3 — the production source holds the guard past the material read (compiled=True) | **FAILED**, as it must (`the_source_releases_the_lock_before_the_answer`) | green -- the lane drives the production source directly |
| NC-4 — the material read mutates the router's NetDB (compiled=True) | **FAILED**, as it must (`the_read_is_read_only`) | green -- the lane drives the production source directly |
| NC-5 — the missing store is refused under the wrong reason (compiled=True) | **FAILED**, as it must (`the_missing_store_is_a_typed_and_counted_refusal`) | **FAILED**, as it must (`a_poisoned_netdb_lock_is_the_same_typed_refusal`, `every_refusal_is_typed_and_counted_and_writes_nothing`, `the_drain_runs_without_a_store_and_counts_the_typed_refusal`) |
| NC-6 — the missing store's refusal is not counted (compiled=True) | **FAILED**, as it must (`the_missing_store_is_a_typed_and_counted_refusal`) | **FAILED**, as it must (`a_poisoned_netdb_lock_is_the_same_typed_refusal`, `every_refusal_is_typed_and_counted_and_writes_nothing`, `the_drain_runs_without_a_store_and_counts_the_typed_refusal`) |
| NC-7 — the accept is bound to the requester instead of the introducer (compiled=True) | **FAILED**, as it must (`the_target_roles_machinery_is_untouched`) | **FAILED**, as it must (`the_target_answers_the_introduction_and_punches_the_hole`) |
| NC-8 — the transport's target refusal order is changed (compiled=True) | **FAILED**, as it must (`the_transports_target_table_is_untouched`) | **FAILED**, as it must (`the_transports_state_machines_and_model_are_untouched`) |
| NC-9 — the daemon's configuration surface gains a key (compiled=True) | **FAILED**, as it must (`no_new_surface_and_the_drain_rides_the_existing_tick`) | **FAILED**, as it must (`no_configuration_key_besides_the_one_opt_in`) |
| NC-10 — the target's owner gains a socket (compiled=True) | **FAILED**, as it must (`no_new_surface_and_the_drain_rides_the_existing_tick`) | green -- the lane drives the production source directly |
| NC-11 — Plan 532's guard is neutered: it no longer looks for the string it looked for (compiled=True) | **FAILED**, as it must (`the_predecessors_evidence_is_untouched`) | **FAILED**, as it must (`the_owner_is_the_daemons_and_the_mechanics_the_runtimes`) |

Every mutation was restored **byte-identically** — the restore is verified by
a SHA-256 comparison of the file's bytes — and the working tree was verified
to be exactly as it was before each mutation.

**One control was first written wrong, and the correction is recorded rather
than hidden.** NC-6 was first written by *removing* the refusal's match arm
from the owner's recorder. That mutation did not compile — the match became
non-exhaustive — so it proved only that the build breaks, not that the guard
fires. It was rewritten so that the mutation compiles: the arm now increments
the wrong counter. The script treats a compile error as an **inconclusive**
control rather than as a firing rule, and every control in the table above
compiles and fires the row it names.

**Two controls are static-only, and that is honest.** The lock scope is a
property of *who holds the lock and when*, and the tree-level mutation that
restores the old shape — the composition root taking the bootstrap's mutex
across the drain — is a change in the composition root, which the lane does
not drive. The lane's own **control row**
(`the_lock_measurement_detects_a_lock_held_across_the_answer`) is what proves
the measurement is not vacuous: a signer that deliberately holds the lock
while it signs is observed as holding it. NC-1, NC-2 and NC-3 are therefore
static controls, and the lane's positive and control rows are what carry the
behaviour.

## Acceptance criteria

| Criterion | Met by |
| --- | --- |
| the answer is produced with **no** lock held, and the lane measures it | the lane's first row, with the lane's control row as the non-vacuity control |
| the drain runs without a store and **counts** a typed refusal | the lane's third and fourth rows; the guard's rule 4 |
| the target role's semantics are unchanged, and Plan 532's lane and guard are byte-identical | the guard's rules 5 and 8; the recorded SHA-256 of Plan 532's two files |
| the transport is untouched, and no configuration key, task, timer, listener or socket is added | the guard's rules 6 and 7; `git diff` over the tree shows no transport file and no manifest |
| `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | the inventory holds 129 surfaces and 0 advertised |
| the full floor is green, with the closure record's table complete | the floor table above |
| the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | the negative-control table above |

## Invariant review

The plan's §4 lists ten invariants
(`plans/implementation/network-participation/536-the-relay-target-role-must-answer-without-holding-the-routers-netdb-lock.md:104`).
Each is reviewed against the closing tree.

1. **The answer is produced with no lock held.** The material is read under a
   short, bounded lock — one named peer, one strict address parse, two derived
   values — and the guard is released before the read returns. The lane probes
   the lock at the moment the accept is signed, and the guard fails closed if
   the guard escapes the read.
2. **The drain is never skipped silently.** The drain runs whenever the target
   role exists. A missing store is a typed, counted refusal, and the guard
   fails closed if the drain's condition names the bootstrap at all.
3. **The read stays read-only.** The read touches no store state, mutates
   nothing and writes no peer profile. The guard fails closed if a mutating
   call appears in either the read or the production source.
4. **The target's machinery is untouched.** The transport seam, the accept's
   signature, the hole punch and the dial profile are exactly what the tree
   already had, and the guard checks each of them.
5. **The refusals stay typed.** Every outcome the drain can reach is a typed
   fact and each is counted under its own reason. The lane checks that the
   reason keys are distinct.
6. **No transport file changes.** The plan touches no file under
   `crates/i2pr-transport/`; `git diff` over the closing tree shows none, and
   the transport's target refusal order is checked by the guard.
7. **No configuration key is added.** The daemon's configuration surface is
   unchanged: the guard counts the `relay_target` keys and fails closed on a
   second one.
8. **No new task, timer, listener or socket.** The drain rides the existing
   relay tick, which is created once and ticked once, and the drain has
   exactly one call site. The target's owner module adds no infrastructure.
9. **Plan 532's lane and guard stay byte-identical.** The guard pins both
   files by SHA-256 and also checks that Plan 532's guard still looks for the
   strings it looks for and that the tree still satisfies them.
10. **No advertisement.** `router.version` stays `0.9.62`, the published
    RouterInfo and `caps` are unchanged, and the inventory's advertised count
    stays **zero**. No boundary script is weakened: no `|| true`, no
    `continue-on-error`, no filename filtering, no fake peer environment, no
    broad exclusion and no early-return-success, and no other plan's guard
    file was touched — NC-8, NC-9 and NC-11 show Plan 532's guard is still
    **live** rather than merely present.

## Failure, cancellation, restart, and contention review

**Failure.** A source that cannot answer returns
`RelayTargetRefusal::StoreUnavailable`, the drain counts it under its own
reason, and the pass continues with the next event. A poisoned bootstrap lock
is the same typed refusal. Nothing is skipped without a trace.

**Cancellation.** The drain is a synchronous step inside the product's
existing tick. It awaits nothing and holds no lock across an await, so a
cancellation cannot strand it and cannot leave the router's NetDB lock held.

**Restart.** Nothing is persisted. The target's handoff is live state and is
rebuilt empty; a restart answers no introduction until a new one arrives.

**Contention.** The bootstrap's mutex is now taken for a single-key read and
released before the answer. The NetDB discovery tick, the requester's
outbound step, the publication tick, the peer reconcile and the peer pool can
no longer be held behind sixteen signature operations. The read is
**read-only**, so it cannot deadlock against a writer: the mutex is taken once
per event and released before the next.

## Migration and compatibility review

**No wire change.** The introduction's bytes, the accept's bytes, the hole
punch, the transport's target table and the transport's refusal order are
exactly what the tree already produced.

**No configuration migration.** **No key is added.** The daemon's
configuration surface is unchanged and a configuration that names nothing new
behaves exactly as before.

**A refusal that did not exist before.** A router that holds no validated
store now **counts** `StoreUnavailable` where it used to say nothing. That is
a new observation, not a new behaviour: the introduction was not answered
before either.

**No change to the target's semantics.** The same introductions are answered,
the same accepts are signed and the same hole punches are staged.

**No change to the transport's model.** No file under `crates/i2pr-transport/`
is touched.

**A new trait and a new counter in the daemon.** The daemon's relay target
module gains one public trait, one public struct and one counter field. A
consumer that reads the counters by name is unaffected; a consumer that
enumerates them sees one more.

## Security review

**No new exposure.** The read touches one named peer's **validated** record,
through the transport's strict address parser. It reads no unvalidated bytes,
it writes no peer profile, and it records no reachability signal. The counters
carry counts only: no peer hash, no address, no key, no tag and no payload.

**No new trust boundary.** The read is taken under the router's existing
NetDB lock, for the whole of a single-key read, and released before the
answer. It opens no socket, starts no task, adds no listener and reads no
file.

**No secret is logged or exposed.** No `Debug`, `Display` or serialization
surface is added, and no private identity material is copied, cloned or
minted. The accept is still signed with the router's own identity key through
the existing seam.

**A local condition is never a peer's fault.** `StoreUnavailable` says that
**this router** has no usable validated store. It is never written into the
requester's profile, and it never counts as a peer fault — the symmetry of the
rule that a peer's fault is never written into this router's own condition.

**Fail closed.** A missing or poisoned store refuses rather than skipping, and
the refusal is counted. The read is read-only, so it cannot corrupt the
NetDB it reads.

**No boundary weakened.** No boundary script was touched, and the transport's
own target table and refusal order — the enforcement point for the target's
verification — are exactly as they were.

## Limitations

- The **router's validated store is still the bootstrap's store**. The plan
  removes the **lock scope** and the **silent skip**; it does not give the
  router a NetDB owner of its own.
- The **production** SSU2 peer-test driver stays unbuilt; the `B`
  advertisement and `caps=f` stay with their owners.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology. No stock peer was needed to prove anything in this plan, and none
  was attempted.
- The delivery is confined to the **controlled loopback profile**: the
  router's SSU2 dial target is loopback-only and the private-LAN variant
  accepts only RFC1918 IPv4. That bound is Plan 431's.
- The lock scope is a **static** property. The lane proves the *good* case and
  carries its own control row for the measurement; the tree-level controls
  that restore the old shape are static, because the composition root is not
  reachable from a test.
- The reason key `store-unavailable` and the counter
  `refusals_store_unavailable` are new names in the daemon's own vocabulary.
  They are internal, they carry counts only, and nothing outside the daemon
  reads them by name today.

## Findings by severity

### None — nothing was found that blocks the plan

No High, Medium or Low finding remains open in the plan's own production
change. The guard's eight rules and the lane's seven rows are green, the
floor is green, and the negative controls all fire the surface that owns the
defect.

### Medium — discharged

**Plan 532's recorded limitation — the target role needs the bootstrap's
store, and the composition root drains the target's handoff only when the
daemon holds a `Bootstrap` — is discharged by this plan.** The lock scope is
now a single-key read whose guard is released before the answer, and the
drain is never skipped silently: a router with no usable store refuses the
introduction under a typed, counted reason instead of saying nothing. The
correction lives in this record and in the registry's row; **Plan 532's closed
closure record is not rewritten.**

### Low — recorded by this plan

1. **The router's validated store is still the bootstrap's store.** The plan
   removes the lock scope and the silent skip and leaves the ownership
   alone, so the target role still reads the bootstrap's store. The coupling
   that remains is bounded, typed and counted, but it is a coupling.
   **Owner:** the successor that gives the router a NetDB owner of its own.
2. **The lock scope is proven statically and by a positive lane row, not by
   a tree-level behavioural control.** The composition root's lock scope is
   not reachable from a test, so the tree-level controls for it are static;
   the lane's own control row is what proves the measurement is not vacuous.
   **Owner:** the same successor, if the composition root's drain is ever
   refactored into a reachable seam.
3. **The requester's outbound step keeps its bootstrap gate.** That is
   deliberate — asking a peer for a relay genuinely needs the router's peer
   population — but it means the requester still contends with the router's
   NetDB. **Owner:** the requester's line.

The daemon deep-dive records the new shape at
`docs/architecture/i2pr-daemon.md:4301`.

## Roadmap disposition

The network-participation roadmap's row for this plan moves from `registered`
to `closed`, with the plan's own closure record as the authority and this
plan's implementation, lane-and-guard and documentation commits named beside
it.

The roadmap's own completion clause for the SSU2 lane is **not** met by this
plan and is not claimed: the router must reach the **public** network, and
the **stock-peer** half of every SSU2 plan in this line remains blocked on
Plan 493's authorized two-host topology. This plan removes a defect that
would have made the router's relay answer contend with its own NetDB and
that would have left an unanswerable introduction with no trace; it does not
carry the router to the public network.

The next free plan number is **537**.

## Unblock audit

| Blocked item | Still blocked on | Changed by this plan |
| --- | --- | --- |
| Plan 431 — the SSU2 independent lane on a non-loopback reference topology | the reference topology | unchanged |
| Plan 493 — the authorized two-host topology | the operator's authorization | unchanged |
| Plan 493 and the stock-peer halves of Plans 519-536 | Plan 493 | unchanged |
| Plan 521's stock-peer half | Plan 493 **and** the absence of a stock router that implements a router-level I2NP 231 responder | unchanged |
| Plan 532's Low finding 2 — the target role needs the bootstrap's store, and the drain was gated on holding one | — | **discharged** |

## Handoff notes

- The plan is **experimental** and default-off. Do not describe the change as
  released, supported or advertised.
- The delivery is confined to the controlled loopback profile. A public
  deployment is unaffected: the answer is moved, not widened.
- The stock-peer half stays blocked on Plan 493's authorized two-host
  topology.
- Plan 532's closed closure record is **not** rewritten. The discharge lives
  in this record and in the registry's row.
- The plan **extends** no other plan's guard: Plan 532's guard stays
  byte-identical and is still live — NC-11 shows it.
- The guard's rule 8 pins Plan 532's two files by SHA-256. A future plan that
  needs to change Plan 532's evidence must change those two constants **and**
  say why.
