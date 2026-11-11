
# Plan 535 — the router must keep its own bound address counted as reachability evidence

**Closure record.** The plan of record is
`plans/implementation/network-participation/535-the-router-must-keep-its-own-bound-address-counted-as-reachability-evidence.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 535 is **closed**. The router's own bound address stays **counted** as
reachability evidence for as long as the router runs: the class is
re-asserted on the runtime's existing maintenance pass, strictly before the
observation lifetime can lapse it, and the expiry rule keeps exactly one
owner — the transport's own tracker.

Classification: **corrective** (the machinery existed and nothing used it:
Plan 530's recorder put the class into the tracker once, at bind time, and
the transport's model holds every signal as a timed observation, so the class
lapsed after the observation lifetime and nothing re-asserted it).
**Subsystem:** network-participation. **Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The defect — a class the router earned and then lost by sitting still.**
The transport's reachability model needs **three distinct supporting
classes** before it will say `Reachable`
(`crates/i2pr-transport/src/reachability.rs:66`). Two of them arrive from the
network: an authenticated peer's observation of this router's address (Plan
529) and a confirmed peer test (Plan 521). The third is the router's **own**
bound socket — class 0, the one Plan 530 recorded.

That class is a **standing** fact. The socket is bound and stays bound. But
the model holds every signal as a *timed observation*, and the class was
recorded exactly once, at bind time
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3756`). The observation lifetime is
the transport's own thirty minutes
(`crates/i2pr-transport/src/reachability.rs:60`), and the runtime polls the
expiry on each of the paths that read the evidence. So the lapse was not
theoretical: after one observation lifetime the router's corroboration count
could never exceed two again, and `Reachable` was unreachable for ever.
Plan 530's own closure record named the limitation — "the class lapses with
the observation TTL" — and named this plan's line as its owner.

**The rule is public and pure.** `explicit_bind_refresh_due`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:467`) answers one question: is the
router's own bound-address fact due for re-assertion? It is due once the
record has aged past **half** the observation lifetime.

Half, not the whole. The re-assertion must happen **while the previous
record is still live**, because the expiry poll runs on the **read** paths: a
rule of "re-assert once expired" would let a read between the record's expiry
and the next maintenance pass find the class already gone. The margin is a
factor of two on a thirty-minute lifetime, against a maintenance pass that
runs at least every 200 milliseconds.

The rule **fails closed**. A fact that was never asserted is due, and a zero
lifetime is due on every pass — because a class that is re-asserted too
often is harmless and a class that lapses is not.

**The lifetime has one owner.** The runtime does not keep its own number for
the observation lifetime. It reads the tracker's own policy back
(`crates/i2pr-runtime/src/ssu2_runtime.rs:8276`, over
`crates/i2pr-transport/src/reachability.rs:448`), so the expiry rule has
exactly one owner and the runtime cannot drift from the model that enforces
it. The runtime's own parameter
(`Ssu2RuntimeReachability`, `crates/i2pr-runtime/src/ssu2_runtime.rs:424`)
exists so the *policy* can be configured and the behaviour **proved**: a
router configured with a short lifetime must keep its own class counted past
that lifetime, and the lane proves exactly that. Its production default is
the transport's own, and the daemon's mapping names that default
(`crates/i2pr-daemon/src/router_i2np.rs:1539`) rather than restating a
number.

**The re-assertion rides the existing maintenance.** It is one step of the
runtime's existing `drive_timeouts_locked`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:8297`). No timer, no task, no
listener and no socket is added, and the daemon's product tick is not
involved.

**The standing fact is captured at the bind.** The recorder stores the
families the router's own sockets can be named for, and the time the fact was
last asserted (`crates/i2pr-runtime/src/ssu2_runtime.rs:3781`), in the shared
state. The runtime binds once and does not rebind, so the families never
change and the re-assertion needs neither the sockets lock nor a second
clock — the runtime's lock order (sockets -> state) is never inverted.

**The re-assertion touches the router's own tracker and nothing else.**
`refresh_explicit_bind_locked`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:8268`) records the router's own
`LocalConfiguredBind` class. It writes no peer profile, claims no
corroborating class for any peer, and records no reachability signal about
anything but this router's own socket. A local condition is never written
into a peer's profile.

**Every re-assertion is counted.** The counter is `explicit_bind_refreshes`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:1277`), an `AtomicU64` in the
runtime's own counters (`crates/i2pr-runtime/src/ssu2_runtime.rs:1739`)
published in the snapshot as a plain count. Nothing publishes it but the
snapshot, and nothing mutates it from the outside.

**The transport's model is untouched.** The plan changes **no file** under
`crates/i2pr-transport/`: the signal vocabulary, the corroboration classes,
both floors, the observation lifetime and the tracker's public surface are
exactly what the tree already had, and the tracker gains no method — the
re-assertion *records* through the method the transport already offers.

## What the plan did not close

- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology. Nothing in this plan needs a stock peer, and none was attempted.
- The **production** SSU2 peer-test driver stays unbuilt. The `B`
  advertisement and `caps=f` stay with their owners.
- Plan 530's **Low** finding 1 — the explicit-bind class lapses with the
  observation TTL and no re-record timer exists — is **discharged** by this
  plan. **Plan 530's closed closure record is not rewritten**; the correction
  lives in this record and in the registry's row.
- The delivery stays confined to the **controlled loopback profile** (Plan
  431's bound).
- The router still cannot reach `Reachable` on the strength of its own bind
  alone: three **distinct** classes are needed, and the other two are
  refreshed by their peers. This plan keeps one of the three; it does not
  manufacture the other two.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `1f0fc501` | the plan of record, the registry's registered row and the roadmap's registered row |
| `cb2565a5` | the production change: the runtime's reachability policy parameter, the rule, the re-assertion in the existing maintenance pass, the counter, and the daemon's mapping |
| `5ebb0ff6` | the lane and the guard |
| `df992cbd` | the inventory, the conformance record, the qualification gates, the protocol scope, the runtime deep-dive and the plan's amendment log |

The closure record and the closed rows are carried by the commit that adds
this file; its hash is not named here, because a record cannot name the
commit that contains it.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| the class is counted past the observation lifetime | `crates/i2pr-daemon/tests/ssu2_explicit_bind_refresh.rs`, `a_router_keeps_its_own_class_counted_past_the_observation_lifetime` |
| the rule is pure and its truth table is the stated one | the same lane file, `the_rule_re_asserts_strictly_before_the_record_can_lapse` |
| the re-assertion is counted and the counter is counts only | the same lane file, `the_re_assertions_are_counted_and_the_counter_carries_counts_only`; the guard's `every_re_assertion_is_counted` |
| a runtime without the opt-in re-asserts nothing | the same lane file, `a_runtime_without_the_opt_in_re_asserts_nothing`; the guard's `the_re_assertion_is_gated_on_the_same_opt_in` |
| the transport's model is untouched | the same lane file, `the_transports_reachability_model_is_untouched`; the guard's `the_transports_model_is_untouched` |
| the lifetime is read from the tracker's own policy | the guard's `the_lifetime_comes_from_the_trackers_policy` |
| the re-assertion rides the existing maintenance and adds no timer | the guard's `the_re_assertion_rides_the_existing_maintenance` |
| the rule re-asserts strictly before the record can lapse | the guard's `the_rule_re_asserts_before_the_lapse` |
| the re-assertion touches only the router's own tracker | the guard's `the_re_assertion_touches_only_the_routers_own_tracker` |
| no configuration key is added | the guard's `no_configuration_key_is_added` |
| Plan 530's guard stays in force | the guard's `the_predecessors_guard_stays_in_force` |
| every rule has an inverted probe and fails closed | the guard's `the_guard_reports_*` and `the_guard_fails_closed_on_*` rows |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test ssu2_explicit_bind_refresh -- --test-threads=1` | PASS -- 5 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_explicit_bind_refresh_boundary -- --test-threads=1` | PASS -- 28 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_explicit_bind_corroboration --test ssu2_explicit_bind_corroboration_boundary -- --test-threads=1` | PASS -- 7 rows and 20 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |
| `python3 /tmp/535-negative.py` | PASS — 11 controls, each firing the surface that owns the defect, each restored byte-identically |

## The routine floor

The floor ran on the committed tree `df992cbd` and it is **green**. Every
step is recorded in `/tmp/535-floor.log`, with each step's own log under
`/tmp/535f-<step>.log`. The floor is a plain sequence of `step` runs that
records each step's exit status; no step is skipped, no `|| true`, no
`continue-on-error` and no filename filtering. The workspace step runs with
`--no-fail-fast`, so every test binary's result is reported rather than the
run stopping at the first failure.

Plan 535's floor is Plan 534's floor plus two steps — the daemon lane and
the daemon guard for the router's own explicit-bind re-assertion. **No other
step changed, and no other plan's guard file was touched.**

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

**Total: 109 PASS, 0 FAIL.**

**One environment note, recorded rather than hidden.** The floor's first run
failed its `test-workspace` step with a **linker** crash — `rust-lld` died
with a bus error while linking the daemon's test binaries — because this
machine's disk was at 100% with 161 MiB free. The disk was full because the
build cache of this repository had grown to 60 GiB alongside other
sessions' caches on the same filesystem, and an agent session in another
project on this machine ran a cleanup that removed this repository's
`target/` directory mid-run. No source, no test and no check was at fault:
the failure was the operating system's, in the linker, on a full disk. The
floor was re-run in full on the same committed tree with the build cache
moved outside the repository, with incremental compilation off, and with
cargo's automatic cache collection disabled. The **steps, the commands and
the tree** are unchanged.

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be
reported. Every probe mutates an **in-memory copy** of the source, so **no
file on disk is written**. Every rule **fails closed** when the item it
names is absent — the guard reports the missing item rather than passing.

### Plan 535's guard — `crates/i2pr-daemon/tests/ssu2_explicit_bind_refresh_boundary.rs`

Eight rules, an inverted probe for every rule, and a fail-closed probe for
every item a rule names. All 28 rows green.

| Rule | What it forbids | Its inverted probe |
| --- | --- | --- |
| 1 | a re-assertion that ignores the opt-in the recorder is, or a recorder that captures no standing fact | `the_guard_reports_an_ungated_re_assertion`, `the_guard_reports_a_recorder_that_captures_nothing` |
| 2 | a restated observation lifetime, or a policy built from a literal | `the_guard_reports_a_restated_lifetime` |
| 3 | a second call site — a second timer — or a re-assertion that adds infrastructure of its own | `the_guard_reports_a_second_call_site`, `the_guard_reports_a_re_assertion_that_adds_a_timer` |
| 4 | a rule that waits for the record to lapse | `the_guard_reports_a_rule_that_waits_for_the_lapse` |
| 5 | a re-assertion that claims another signal, or writes a peer profile | `the_guard_reports_a_re_assertion_that_claims_another_signal`, `the_guard_reports_a_re_assertion_that_writes_a_peer_profile` |
| 6 | a changed transport model, or a tracker that gains a method | `the_guard_reports_a_changed_transport_model`, `the_guard_reports_a_tracker_that_gains_a_method` |
| 7 | a new configuration key, or a mapping that names a lifetime of its own | `the_guard_reports_a_new_configuration_key`, `the_guard_reports_a_mapping_that_names_its_own_lifetime` |
| 8 | a re-assertion that is not counted, or a counter that carries more than a count | `the_guard_reports_an_uncounted_re_assertion`, `the_guard_reports_a_counter_that_is_not_a_count` |

Five further probes remove the item each rule names — the re-assertion, the
rule, the recorder, the maintenance pass and every rule's named source — and
every rule reports the absence rather than passing. They are the
`the_guard_fails_closed_on_*` rows.

### The negative control against the **real** tree

The guard's probes are in-memory. The tree was therefore also mutated **on
disk** — one mutation at a time, each restored byte-identically — and the
guard and the lane were re-run. The script is `/tmp/535-negative.py`.

| Mutation | The guard | The lane |
| --- | --- | --- |
| NC-1 — the maintenance pass no longer re-asserts the standing fact | **FAILED**, as it must (`the_re_assertion_rides_the_existing_maintenance`) | **FAILED**, as it must (`a_router_keeps_its_own_class_counted_past_the_observation_lifetime`, `the_re_assertions_are_counted_and_the_counter_carries_counts_only`) |
| NC-2 — the recorder captures no standing fact | **FAILED**, as it must (`the_re_assertion_is_gated_on_the_same_opt_in`) | **FAILED**, as it must (the same two rows) |
| NC-3 — the re-assertion restates the observation lifetime | **FAILED**, as it must (`the_lifetime_comes_from_the_trackers_policy`, `the_re_assertion_touches_only_the_routers_own_tracker`) | green — the lane drives the runtime's own policy |
| NC-4 — the rule waits for the record to lapse | **FAILED**, as it must (`the_rule_re_asserts_before_the_lapse`) | **FAILED**, as it must (`the_rule_re_asserts_strictly_before_the_record_can_lapse`) |
| NC-5 — the re-assertion claims a peer-sourced class | **FAILED**, as it must (`the_re_assertion_touches_only_the_routers_own_tracker`) | green — the lane drives the router's own class |
| NC-6 — the transport's corroboration floor is lowered | **FAILED**, as it must (`the_transports_model_is_untouched`) | **FAILED**, as it must (`the_transports_reachability_model_is_untouched`) |
| NC-7 — the daemon's configuration surface gains a key | **FAILED**, as it must (`no_configuration_key_is_added`) | green — the lane wires the runtime itself |
| NC-8 — the re-assertion is not counted | **FAILED**, as it must (`every_re_assertion_is_counted`) | **FAILED**, as it must (the same two rows) |
| NC-9 — the re-assertion ignores the opt-in | **FAILED**, as it must (`the_re_assertion_is_gated_on_the_same_opt_in`) | green — the lane drives the runtime's own gate |
| NC-10 — the daemon's mapping names a lifetime of its own | **FAILED**, as it must (`no_configuration_key_is_added`) | green — the lane does not use the daemon's mapping |
| NC-11 — the daemon's configuration surface gains an explicit-bind key | **FAILED**, as it must (Plan 530's own `no_new_key_and_the_existing_mapping`) | green — Plan 530's lane drives its own wiring |

**Two controls were first written wrong, and the correction is recorded
rather than hidden.** NC-7 and NC-8 were written with mutations that did not
compile — a struct field with no construction site, and a counter
increment removed from the middle of a statement — so they proved only that
the build breaks, not that the guard fires. They were rewritten so that the
mutations compile, and the script now treats a compile error as an
**inconclusive** control rather than as a firing rule. Every control in the
table above compiles and fires the row it names.

Every mutation was restored **byte-identically** — the restore is verified
by a SHA-256 comparison of the file's bytes — and the working tree was
verified to be exactly as it was before each mutation.

## Acceptance criteria

| Criterion | Met by |
| --- | --- |
| the explicit-bind class is **counted** past the observation lifetime, and the rule is proved rather than described | the lane's first two rows, over a fine millisecond grid, with the transport's own expiry as the control |
| the re-assertion is **gated** on the existing opt-in, rides the existing maintenance, and adds no timer and no configuration key | the guard's rules 1, 3 and 7 |
| the transport's reachability model is **untouched**, and Plan 530's guard file is byte-identical | the guard's rule 6 and `the_predecessors_guard_stays_in_force`; `git diff` over the tree shows no transport file |
| the counter is counts only, and every re-assertion is counted | the lane's counter row and the guard's rule 8 |
| `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | the inventory holds 128 surfaces and 0 advertised; the published RouterInfo is unchanged |
| the full floor is green, with the closure record's table complete | the floor table above |
| the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | the negative-control table above |

## Invariant review

The plan's §4 lists ten invariants. Each is reviewed against the closing
tree.

1. **The expiry rule has one owner.** The runtime reads the observation
   lifetime from the tracker's own policy
   (`crates/i2pr-runtime/src/ssu2_runtime.rs:8276`) and never restates it.
   The guard fails closed if a literal appears in the re-assertion.
2. **The class is re-asserted strictly before it can lapse.** The rule is
   due at half the lifetime, and the lane proves the first due moment is at
   or before that half over a fine grid. A read between an expiry and the
   next maintenance pass can never find the class already gone.
3. **The re-assertion adds no timer.** It rides the runtime's existing
   maintenance pass, and the guard checks that the re-assertion has exactly
   one call site in the whole runtime — a second call site would be a second
   timer.
4. **The recorder stays the runtime's, and stays gated.** The re-assertion
   is the same class, on the same opt-in, from the same owner. A runtime that
   did not ask for explicit-bind corroboration re-asserts nothing.
5. **The transport's reachability model is untouched.** No file under
   `crates/i2pr-transport/` is changed; the signal vocabulary, the
   corroboration classes, both floors, the states and the expiry semantics
   are exactly what the tree already had, and the tracker gains no method.
6. **No configuration key is added.** The daemon's configuration surface is
   unchanged, and the daemon's mapping names the runtime's own default
   policy. The guard fails closed if a key appears in either.
7. **The re-assertion is counted.** Every re-assertion increments the
   counter, and the counter carries counts only: a plain `u64` in the
   snapshot, published by the snapshot alone.
8. **A local condition is never written into a peer's profile.** The
   re-assertion touches the router's own tracker and nothing else. The guard
   checks that the only signal it records is `LocalConfiguredBind`, and that
   no peer profile, peer identity or peer class appears in it.
9. **No advertisement.** `router.version` stays `0.9.62`, the published
   RouterInfo and `caps` are unchanged, and the inventory's advertised count
   stays **zero**.
10. **No boundary script is weakened.** No `|| true`, no
    `continue-on-error`, no filename filtering, no fake peer environment, no
    broad exclusion, no early-return-success, and no production wire change
    made to go green. No other plan's guard file was touched: Plan 530's
    guard is byte-identical, and the negative control NC-11 shows it is
    still **live** rather than merely present.

## Failure, cancellation, restart, and contention review

**Failure.** A runtime that cannot take the state lock re-asserts nothing,
and the class lapses on its own rule — the same outcome as before this
plan, and never a wrong claim. A runtime without the opt-in re-asserts
nothing at all. The rule fails closed: a fact that was never asserted is
due, and a zero lifetime is due on every pass.

**Cancellation.** The re-assertion is a synchronous step inside the runtime's
existing maintenance pass. It awaits nothing and holds no lock across an
await, so a cancellation cannot strand it and cannot leave a lock held.

**Restart.** Nothing is persisted. The class is recorded again at the next
bind, exactly as before this plan, and the standing fact's timestamp is
taken at that bind.

**Contention.** The re-assertion takes the runtime's existing state lock, in
the existing order (sockets -> state), and **never** the sockets lock while
the state lock is held: the families are captured at bind time precisely so
that the re-assertion needs neither lock order inverted nor a second clock.
The two socket loops share the state, so the second loop's pass sees the
first's timestamp and does not assert the fact twice.

## Migration and compatibility review

**No wire change.** The reachability signal, its class and the publication
that consumes them are exactly what the tree already produced. The
re-assertion records the same signal the recorder always recorded.

**No configuration migration.** **No key is added.** The daemon's
configuration surface is unchanged and a configuration that names nothing new
behaves exactly as before. The runtime's own parameter is an internal Rust
struct, not a daemon configuration key, and its production default is the
transport's own.

**A new reading for the router.** A router that has asked for explicit-bind
corroboration now keeps that class for as long as it runs, instead of
holding it for one observation lifetime. It publishes nothing new and
advertises nothing new.

**No change to the transport's model.** The signal vocabulary, the
corroboration classes, the floors, the states and the expiry semantics are
unchanged, and the tracker gains no method.

**A new counter in the snapshot.** `Ssu2Snapshot` gains one field,
`explicit_bind_refreshes`, a plain count. A consumer that reads the snapshot
by name is unaffected; a consumer that enumerates its fields sees one more.

## Security review

**No new exposure.** The re-assertion records the router's own
`LocalConfiguredBind` class — the same redacted signal Plan 530 recorded:
it carries the **family**, never a literal endpoint. Nothing about a peer,
a peer's address, a peer's key or a peer's profile is read or written.

**No new trust boundary.** The re-assertion runs inside the runtime, under
the runtime's existing state lock, in the runtime's existing maintenance
pass. It opens no socket, starts no task, adds no listener and reads no
file.

**No secret is logged or exposed.** The counter is a count. The snapshot
publishes counts only. No `Debug`, `Display` or serialization surface is
added, and no private identity material is copied, cloned or minted.

**Fail closed.** The rule fails closed in both directions that matter: a
fact that was never asserted is due (so a missed record is corrected), and a
zero lifetime is due on every pass (so a misconfigured lifetime cannot
silently stop the re-assertion). A class that is re-asserted too often is
harmless; a class that lapses is not.

**No boundary weakened.** No boundary script was touched, and the transport's
own model — the enforcement point for the corroboration and contradiction
floors — is exactly as it was.

## Limitations

- The **production** SSU2 peer-test driver stays unbuilt; the `B`
  advertisement and `caps=f` stay with their owners.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology. No stock peer was needed to prove anything in this plan, and
  none was attempted.
- The delivery is confined to the **controlled loopback profile**: the
  router's SSU2 dial target is loopback-only and the private-LAN variant
  accepts only RFC1918 IPv4. That bound is Plan 431's.
- The router still needs the other two classes — the peer's observed
  external address and a confirmed peer test — before it can say
  `Reachable`. This plan keeps one of the three; it does not earn the other
  two.
- The **half-lifetime** margin is this plan's own choice. It is a factor of
  two on the transport's thirty-minute lifetime, against a maintenance pass
  that runs at least every 200 milliseconds, and it is not derived from a
  reference implementation.
- The runtime's own policy parameter exists to make the behaviour provable.
  Its production value is the transport's own, and the daemon's mapping
  names that default rather than a number of its own.

## Findings by severity

### None — nothing was found that blocks the plan

No High, Medium or Low finding remains open in the plan's own production
change. The guard's eight rules and the lane's five rows are green, the
floor is green, and the negative controls all fire the surface that owns the
defect.

### Medium — discharged

**Plan 530's recorded limitation — the explicit-bind class lapses with the
observation TTL, and no re-record timer exists — is discharged by this
plan.** The class is re-asserted on the runtime's existing maintenance
pass, strictly before the lapse, under a rule that is pure, public and
proved. The correction lives in this record and in the registry's row;
**Plan 530's closed closure record is not rewritten.**

### Low — recorded by this plan

1. **The half-lifetime margin is this plan's own constant.** It is not
   derived from a reference implementation, and a different observation
   lifetime would want a different review of the margin. **Owner:** the
   runtime's reachability line.
2. **The re-assertion is counted, not exposed as a decision.** The counter
   says that the class was re-asserted; it does not say whether the router's
   reachability changed, because it did not. **Owner:** the runtime's
   reachability line.
3. **The runtime's reachability parameter is an internal Rust struct.** It
   is not a daemon configuration key, so an operator cannot change the
   observation lifetime from the configuration file. That is deliberate—
   the lifetime is the transport's own — but it is a limit on what the
   parameter can be used for. **Owner:** the runtime's reachability line.

## Roadmap disposition

The network-participation roadmap's row for this plan moves from `registered`
to `closed`, with the plan's own closure record as the authority and this
plan's implementation, lane-and-guard and documentation commits named
beside it.

The roadmap's own completion clause for the SSU2 lane is **not** met by this
plan and is not claimed: the router must reach the **public** network, and
the **stock-peer** half of every SSU2 plan in this line remains blocked on
Plan 493's authorized two-host topology. This plan removes a defect that
would have kept the router's own class from surviving a long run; it does
not carry the router to the public network.

The next free plan number is **536**.

## Unblock audit

| Blocked item | Still blocked on | Changed by this plan |
| --- | --- | --- |
| Plan 431 — the SSU2 independent lane on a non-loopback reference topology | the reference topology | unchanged |
| Plan 493 — the authorized two-host topology | the operator's authorization | unchanged |
| Plan 493 and the stock-peer halves of Plans 519-535 | Plan 493 | unchanged |
| Plan 521's stock-peer half | Plan 493 **and** the absence of a stock router that implements a router-level I2NP 231 responder | unchanged |
| Plan 530's Low finding 1 — the explicit-bind class lapses with the observation TTL | — | **discharged** |

## Handoff notes

- The plan is **experimental** and default-off. Do not describe the
  re-assertion as released, supported or advertised.
- The delivery is confined to the controlled loopback profile. A public
  deployment is unaffected: the class is re-asserted, not widened.
- The stock-peer half stays blocked on Plan 493's authorized two-host
  topology.
- Plan 530's closed closure record is **not** rewritten. The discharge lives
  in this record and in the registry's row.
- The plan **extends** no other plan's guard: Plan 530's guard stays
  byte-identical and is still live.
- The floor was re-run in full after an **environment** failure (a full
  disk, a linker crash, and an external cleanup that removed the build
  cache). The steps, the commands and the tree are unchanged; only the
  build cache's location moved, outside the repository.
