# Plan 534 — the router must build a tunnel toward the peer it reached through the relay

**Closure record.** The plan of record is
`plans/implementation/network-participation/534-the-router-must-build-a-tunnel-toward-the-peer-it-reached-through-the-relay.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 534 is **closed**. The router that reached a peer through a relay now
**builds a tunnel toward that peer**: the peer becomes the route's **named**
build target, subject to exactly the same API-version floor every drawn
candidate passes, with its material read from the validated store and its
build submitted through the **existing** coordinator.

Classification: **corrective** (the machinery existed and nothing used it:
the exploratory driver already had a version-gated selection, the coordinator
already had a single build path, and the peer-profile store was already fed
— but nothing could name a peer the router had deliberately reached, so the
deliberate reach was dropped on the floor). Plan 531 delivered the
**requester** half, Plan 532 the **target** half, and Plan 533 the
**establishment**. This plan delivers the **route**.
**Subsystem:** network-participation. **Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The defect — a deliberate reach that counted for nothing.** Plan 533 closed
the SSU2 relay exchange: the requester holds an authenticated session with the
peer it was relayed to. What the router still could not do was **use** that
peer. The product's tunnel-build route draws its peer from the peer-profile
store's policy selection
(`crates/i2pr-daemon/src/exploratory_driver.rs:760`), under the store's own
constraints — the rank floor, the participation cap, the per-prefix diversity
rule and the version gate. The peer the router had deliberately reached
through a relay was in that store, but it was **one candidate among every peer
the router knew**, and nothing distinguished it. An expensive, explicit act —
the whole point of the relay — was diluted into a random draw.

**The observation is the daemon's, and it is a local fact.** 
`crates/i2pr-daemon/src/relay_session_peer.rs` owns the **local observation
table**: a bounded map of the peers this router reached through a relay, each
stamped with the time it was observed
(`MAX_RELAY_SESSION_PEERS = 8`, `:50`; `RELAY_SESSION_PEER_TTL_MS = 60000`,
`:58`; the owner at `:134`). It holds no socket, owns no task, starts no
timer and dials nothing. The observation says that **this router's** session
with the peer is established; it says nothing about the peer, writes nothing
into the peer's record, and claims no reachability.

**The establishment's own task records the observation.** The task that
Plan 533 added already knows the peer it dialled — the peer comes from the
dial target it built — so recording the observation needs no new identity to
cross a boundary. The public seam
`record_established_peer` (`crates/i2pr-daemon/src/relay_establishment_task.rs:88`)
is called when the handshake succeeds (`:166`), and it takes the peer from the
target's own expected router hash. The peer is therefore the peer the router
**actually reached**.

**The route names the peer.** The driver's `select_peer`
(`crates/i2pr-daemon/src/exploratory_driver.rs:760`) takes the **oldest** live
observation before it draws a candidate, and names that peer
(`:768`, the counter `named_targets_used`). The named target is the route's
**subject**, not one more candidate: the selection's rank floor, participation
cap and per-prefix diversity rule exist to **spread load**, and letting them
dilute a deliberate reach would defeat the point.

**The named target passes the same floor.** The driver gained a gate for the
named target (`relay_target_eligibility`,
`crates/i2pr-daemon/src/exploratory_driver.rs:836`) that calls the **same**
`tunnel_peer_eligibility` every drawn candidate passes
(`crates/i2pr-netdb/src/version.rs:87`; the selection's own call at
`crates/i2pr-netdb/src/selection.rs:300`). The gate's verdict is carried
faithfully: a below-floor peer is refused and counted
(`named_targets_refused`, `:350`), never admitted. The refusal falls back to
the store's own selection, so one ineligible peer never blocks the exploratory
posture.

**A missing record is a local condition, never the peer's fault.** A named
target whose **validated** record the router does not hold is counted as
`peer_material_absent` (`:331`) — a fact about **this router's** store — and
never as the peer's missing version. The guard pins this: the local condition
is counted under its own reason and the peer's profile is left untouched.

**The observation is consumed, and it expires.** Taking is consuming
(`crates/i2pr-daemon/src/relay_session_peer.rs:222`): the router builds toward
a peer it reached through a relay **once**, and a router that wants to reach
the peer again reaches it again through the relay. The observation expires
after a bounded lifetime (`:248`), because a session that died while the
router was not looking must not be built toward. The table is **bounded**
(`:207`), and a full table is refused under its own counted reason
(`refusals_table_full`).

**One build path.** The named target's build is submitted through the
**existing** `ExploratoryBuildCoordinator::rebuild_exploratory`
(`crates/i2pr-daemon/src/exploratory_driver.rs:644`), so Plan 504's liveness
scheduler, Plan 505's pairing and the coordinator's own state machine apply
unchanged. The driver owns no second build path and constructs no message of
its own.

**No configuration key is added.** The table rides the **existing**
`ssu2.relay_requester` opt-in and is created only when the operator asked for
the requester role (`crates/i2pr-daemon/src/lib.rs:4376`), and the route
rides the **existing** `[exploratory]` posture (`:4440`). An ordinary router
records nothing and names nothing.

**The transport and the reachability model are untouched.** This plan changes
**no file** under `crates/i2pr-transport-ssu2/`, and the peer profile gains no
state for a peer this router reached through a relay.

**The counts reach the operator.** The counters are counts only and are
published into the inspection seam
(`crates/i2pr-daemon/src/i2pcontrol_inspection.rs:874`, the snapshot at
`:887`); no peer, address or key is readable through it.

**The build installs.** The lane drives the real driver, the real coordinator,
the real peer-profile store and the real delivery seam over **real loopback
UDP**, with a controlled responder that answers the short tunnel build with
the peer's **identity** encryption key — the key the production
`peer_static_encryption_key` seam derives — and the build toward the relayed
peer **installs**.

## What the plan did not close

- The **production** SSU2 peer-test driver stays unbuilt; the `B` advertisement
  and `caps=f` stay with their owners; and the **stock-peer** half stays
  blocked on Plan 493's authorized two-host topology.
- The target role still needs the **authoritative store**, which the
  composition root takes from the bootstrap. Plan 532's **Low** finding 2 is
  therefore **unchanged**.
- The delivery is confined to the **controlled loopback profile**: the router's
  SSU2 dial target is loopback-only and the private-LAN variant accepts only
  RFC1918 IPv4. That bound is Plan 431's.
- Plan 530's Low finding 1 (the explicit-bind class lapses with the
  30-minute observation TTL and no re-record timer exists) remains open and is
  **not** this plan's subject.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `6f91b5dd` | the registration: the plan of record, the registry row and the roadmap row |
| `ba845e36` | the production change: the local observation table, the establishment's recording seam, the driver's named target and its gate, the composition, and the inspection seam |
| `ceccdc6f` | the daemon lane and the daemon guard |
| `fa0b7bf4` | the documentation, the inventory and the plan's amendment log |

## Requirement-to-evidence matrix

Every acceptance criterion of the plan's §14, the file and line that satisfies
it, and the test that proves it.

| # | Criterion | Satisfied by | Proven by |
| --- | --- | --- | --- |
| 1 | the relayed peer is observed as a **local** fact and the peer's profile carries no corroborating class | `crates/i2pr-daemon/src/relay_session_peer.rs:134` (the owner), `crates/i2pr-daemon/src/relay_establishment_task.rs:88` (the recording seam), `:166` (the call) | guard rule 5 `the_peer_profile_is_not_written_and_no_reachability_is_claimed`; the lane row `the_route_names_the_relayed_peer_and_the_build_installs` |
| 2 | the route builds toward the **relayed** peer and the build installs, through the **existing** coordinator | `crates/i2pr-daemon/src/exploratory_driver.rs:760` (the selection), `:768` (the named target), `:644` (the build) | guard rule 6 `the_build_goes_through_the_existing_coordinator`; the lane row `the_route_names_the_relayed_peer_and_the_build_installs` |
| 3 | the named target passes the **same** version floor, and a refusal is counted under the same reasons | `crates/i2pr-daemon/src/exploratory_driver.rs:836` (the named target's gate), `crates/i2pr-netdb/src/version.rs:87` (the shared gate), `crates/i2pr-netdb/src/selection.rs:300` (the selection's own call) | guard rule 1 `the_route_selects_through_the_gate_and_a_named_target_passes_the_same_gate`; the lane row `a_named_target_below_the_version_floor_is_refused_and_counted` |
| 4 | the observation is bounded, consumed and expiring, and every full-table refusal is counted | `crates/i2pr-daemon/src/relay_session_peer.rs:50` (the capacity), `:207` (the bound), `:222` (the consuming take), `:232` (the removal), `:248` (the expiry) | guard rules 3 and 7; the lane rows `the_observation_table_is_bounded_and_a_full_table_is_refused` and `the_observation_expires_and_the_expiry_is_counted` |
| 5 | every refusal is typed and counted and writes nothing into a peer's record | `crates/i2pr-daemon/src/relay_session_peer.rs:91` (the two reasons), `:117` (the counters), `crates/i2pr-daemon/src/exploratory_driver.rs:331` (the local condition), `:346`, `:350` | guard rule 5; the lane rows `a_named_target_the_store_does_not_hold_is_refused_as_a_local_condition` and `every_refusal_is_typed_and_counted` |
| 6 | no configuration key is added, `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | `crates/i2pr-daemon/src/lib.rs:4376` (the table rides the existing opt-in), `:4440` (the driver rides the existing posture); neither manifest is touched | guard rule 8 `no_configuration_key_is_added`; `python3 scripts/check-tooling-inventory.py`; the counts are 127 surfaces, 122 experimental, 4 deferred, 1 ready, **0 advertised** |
| 7 | the full floor is green, with the closure record's table complete | the floor table below | the floor run |
| 8 | the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | `crates/i2pr-daemon/tests/relay_session_peer_boundary.rs` (9 rules, 9 inverted probes, 9 fail-closed probes, 27 rows) | the negative controls below |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS — no warnings |
| `cargo test --locked -p i2pr-daemon --test relay_session_peer -- --test-threads=1` | PASS — 7 rows |
| `cargo test --locked -p i2pr-daemon --test relay_session_peer_boundary -- --test-threads=1` | PASS — 27 rows |
| `cargo test --locked -p i2pr-daemon --test exploratory_driver_live --test exploratory_driver_boundary -- --test-threads=1` | PASS — 8 rows and 16 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_establishment --test ssu2_relay_establishment_boundary -- --test-threads=1` | PASS — 7 rows and 36 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_target --test ssu2_relay_target_boundary -- --test-threads=1` | PASS — 8 rows and 41 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_requester --test ssu2_relay_requester_boundary -- --test-threads=1` | PASS — 8 rows and 26 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS |
| `python3 /tmp/534-negative.py` | PASS — 8 controls, each caught by the surface that owns the defect, each restored byte-identically |

## The routine floor

The floor ran on the fixed tree `fa0b7bf4` and it is **green**:
**107 PASS, 0 FAIL**. Every step is recorded in
`/tmp/534-floor.log`, with each step's own log under `/tmp/534f-<step>.log`.
The floor is a plain sequence of `step` runs that records each step's exit
status; no step is skipped, no `|| true`, no `continue-on-error` and no
filename filtering. The workspace step runs with `--no-fail-fast`, so every
test binary's result is reported rather than the run stopping at the first
failure.

Plan 534's floor is Plan 533's floor plus two steps — the daemon lane and the
daemon guard for the relayed peer's build route. **No other step changed, and
no other plan's guard file was touched.**

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

**No floor-caught defect.** The floor's first run for this plan was green, so
there is no caught defect to record and no other plan's guard was weakened to
make it pass. The two defects the negative controls found were in **this
plan's own** test files and are recorded in the plan's amendment log and
below: the lane's profile assertion and two guard rules were strengthened
before the floor ran.

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be reported.
Every probe mutates an **in-memory copy** of the source, so **no file on disk
is written**. Every rule **fails closed** when the item it names is absent —
the guard reports the missing item rather than passing.

### Plan 534's guard — `crates/i2pr-daemon/tests/relay_session_peer_boundary.rs`

Nine rules, nine inverted probes, nine fail-closed probes. All 27 rows green.

| Rule | What it forbids | Its inverted probe |
| --- | --- | --- |
| 1 | a named target that bypasses the version gate, or a gate whose verdict is not carried faithfully | `the_guard_reports_a_named_target_that_bypasses_the_gate` |
| 2 | a driver or an observation table that owns a task, a timer, a listener or a socket | `the_guard_reports_a_driver_that_owns_a_socket` |
| 3 | an unbounded observation table | `the_guard_reports_an_unbounded_observation_table` |
| 4 | material parsed from unvalidated bytes | `the_guard_reports_material_parsed_from_unvalidated_bytes` |
| 5 | a profile write for a local condition | `the_guard_reports_a_profile_write_for_a_local_condition` |
| 6 | a second build path | `the_guard_reports_a_second_build_path` |
| 7 | an observation that is never consumed, or never expires | `the_guard_reports_an_observation_that_is_never_consumed` |
| 8 | a configuration key of the observation table's own | `the_guard_reports_a_configuration_key_for_the_observation_table` |
| 9 | an edited transport rule | `the_guard_reports_an_edited_transport_rule` |

Nine further probes remove the item each rule names — the named gate, the
driver, the table's bound, the resolver, the observation owner, the drive
step, the take step, the requester's opt-in and the transport's relay seam —
and every rule reports the absence rather than passing. They are the
`the_guard_fails_closed_when_*` rows.

### The negative control against the **real** tree

The guard's probes are in-memory. The tree was therefore also mutated **on
disk** — one mutation at a time, each restored byte-identically — and the guard
and the lane were re-run. The script is `/tmp/534-negative.py`.

| Mutation | The guard | The lane |
| --- | --- | --- |
| NC-1 — the named target's below-floor verdict is mapped to eligible | **FAILED**, as it must (`the_route_selects_through_the_gate_and_a_named_target_passes_the_same_gate`) | **FAILED**, as it must (`a_named_target_below_the_version_floor_is_refused_and_counted`) |
| NC-2 — the observation table is unbounded | **FAILED**, as it must (`the_observation_table_is_bounded_and_every_full_table_refusal_is_counted`) | **FAILED**, as it must (`the_observation_table_is_bounded_and_a_full_table_is_refused`) |
| NC-3 — the route writes a peer observation for a local condition | **FAILED**, as it must (`the_peer_profile_is_not_written_and_no_reachability_is_claimed`) | **FAILED**, as it must (`the_route_names_the_relayed_peer_and_the_build_installs`) — after the lane was strengthened |
| NC-4 — the driver owns a second build path | **FAILED**, as it must (`the_build_goes_through_the_existing_coordinator`) | green — the lane drives the existing coordinator |
| NC-5 — the observation never expires | **FAILED**, as it must (`the_observation_is_consumed_and_it_expires`) | **FAILED**, as it must (`the_observation_expires_and_the_expiry_is_counted`) |
| NC-6 — a peer the store does not hold is treated as eligible | green — the guard is the authority for the shape of the gate | **FAILED**, as it must (`a_named_target_the_store_does_not_hold_is_refused_as_a_local_condition`) |
| NC-7 — the establishment's task never records the observation | **FAILED**, as it must (`the_observation_is_consumed_and_it_expires`) | green — the lane drives the recording seam directly |
| NC-8 — the observation table is created without the operator's opt-in | **FAILED**, as it must (`no_configuration_key_is_added`) | green — the lane wires the table itself |

**Two controls changed the plan's own tests, and the changes are recorded
rather than hidden.** NC-3 was the first to show a gap: the lane asserted
that the relayed peer's profile was untouched **before** the driver ran, which
proved only that the recording seam wrote nothing — a write during the drive
was not caught. The assertion now follows the drive, where such a write
would happen, and NC-3 fails the lane as well as the guard. NC-1 and NC-7
showed two guard rules that were weaker than they read: rule 1 checked only
that the gate was **called**, not that its verdict was carried faithfully, and
rule 7 checked only the **consuming** half of the observation's lifecycle,
not the **recording** half. Both rules were strengthened; no other plan's
guard was touched.

Every mutation was restored **byte-identically** — the restore is verified by
a SHA-256 comparison of the file's bytes — and the working tree was verified
to be exactly as it was before each mutation.

## Acceptance criteria

The plan's §14 lists eight criteria and four stop conditions.

1. the relayed peer is observed as a **local** fact and the peer's profile
   carries no corroborating class — **met**;
2. the route builds toward the **relayed** peer and the build installs, through
   the **existing** coordinator — **met**;
3. the named target passes the **same** version floor, and a refusal is
   counted under the same reasons — **met**;
4. the observation is bounded, consumed and expiring, and every full-table
   refusal is counted — **met**;
5. every refusal is typed and counted and writes nothing into a peer's record
   — **met**;
6. no configuration key is added, `router.version` stays `0.9.62`, and the
   inventory's advertised count stays **zero** — **met**;
7. the full floor is green, with the closure record's table complete —
   **met**;
8. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each — **met**.

**No stop condition fired.** The named target **could** be made to pass the
same version gate without changing the gate itself. The build toward the
relayed peer **could** be made to go through the existing coordinator — no
second build path was needed and none exists. The floor's
`cargo deny check advisories bans sources` passed. **No stock peer was
required** to prove any of the above: the stock-peer half stays blocked on
Plan 493 and was not attempted.

## Invariant review

The plan's §4 lists eleven invariants. Each is reviewed against the closing
tree.

1. **The version floor is never bypassed.** The named target is gated by the
   same `tunnel_peer_eligibility` the drawn candidates pass
   (`crates/i2pr-daemon/src/exploratory_driver.rs:836`,
   `crates/i2pr-netdb/src/version.rs:87`), and the gate's verdict is carried
   faithfully. The guard fails closed if the mapping is not.
2. **There is exactly one build path.** The named target's build goes through
   the existing `rebuild_exploratory`
   (`crates/i2pr-daemon/src/exploratory_driver.rs:644`), and the module holds
   exactly one such call. The guard fails closed if a second appears.
3. **The peer profile is not written for a local condition.** The observation
   table holds no peer-level write, the selection writes none, and the
   recording seam writes none. The relayed peer's reachability is not
   claimed and no corroborating class is added.
4. **The material is validated.** The named target's version is read from the
   **validated** store through the driver's existing resolver
   (`crates/i2pr-daemon/src/exploratory_driver.rs:840`), and no unvalidated
   bytes are parsed. The guard fails closed if a parse of raw bytes appears.
5. **Every bound is explicit.** The observation table's capacity is a literal
   (`MAX_RELAY_SESSION_PEERS = 8`), the expiry is a literal
   (`RELAY_SESSION_PEER_TTL_MS = 60000`), and every refusal under a full
   table is counted under its own reason.
6. **A local condition is never written into a peer's profile.** Every refusal
   in this plan is a typed, counted reason about **this router's** state.
7. **The transport's relay state machines are used, not edited.** The plan
   changes **no file** under `crates/i2pr-transport-ssu2/`.
8. **The reachability model is untouched.** No corroborating class is claimed,
   the peer profile gains no state, and the corroboration and contradiction
   floors are unchanged.
9. **No advertisement.** `router.version` stays `0.9.62`, the published
   RouterInfo and `caps` are unchanged, and the inventory's advertised count
   stays **zero**.
10. **No configuration key is added.** The table rides the existing
    `ssu2.relay_requester` opt-in and the existing `[exploratory]` posture.
    The guard fails closed if one appears.
11. **No boundary script is weakened.** No `|| true`, no
    `continue-on-error`, no filename filtering, no fake peer environment, no
    broad exclusion, no early-return-success, and no production wire change
    made to go green.

## Failure, cancellation, restart, and contention review

**Failure.** Every refusal is typed and counted by reason, and leaves no state
behind. A full observation table is refused under `TableFull` and grows
nothing. A named target that cannot be gated is refused under its own reason
and the route falls back to the store's own selection. A refused named target
has already **consumed** its observation: one deliberate reach yields one
build attempt, and a refused attempt leaves no stale observation behind.

**Cancellation.** The observation table holds no task and no await, so there
is nothing to cancel. The recording happens inside the establishment's own
task, which is cancelled with the service, and the table's lock is held for
the duration of a synchronous call and released before the caller can suspend.

**Restart.** Nothing is persisted. The observation table is live state and is
rebuilt empty; a restart names no relayed peer and builds nothing toward one.

**Contention.** The table is behind the same short-lived mutex the daemon's
other owners use, and no critical section awaits. The route's take and the
task's record are both single critical sections, so the table cannot grow
without bound and cannot deadlock against another owner.

## Migration and compatibility review

**No wire change.** The relay blocks, the short tunnel build and the
RouterInfo are exactly what the tree already sent and received. This plan
changes no codec and no transport behaviour.

**No configuration migration.** **No key is added.** The table rides the
existing `ssu2.relay_requester` opt-in and the route rides the existing
`[exploratory]` posture. A configuration that names neither behaves exactly
as before.

**A new reading for the router.** A router that has asked for the requester
role may now build a tunnel toward a peer it reached through a relay. It
publishes nothing new and advertises nothing new, and the peer gains no
corroborating class.

**No change to the transport's model.** The relay-firewalled signal already
exists and already contradicts; a peer reached through a relay gains no
corroborating class and the corroboration count is unchanged.

## Security review

**No secret is copied, logged or serialized.** The observation is a peer hash
and a timestamp. The counters carry counts only: no peer, no address, no key
and no token. The observation table's `Debug` prints the observation
**count**, not the observations.

**No unvalidated bytes are parsed.** The named target's version comes from
the **validated** store, and the build's material comes from the same store
through the driver's existing resolver. The guard fails closed if either
path parses raw bytes.

**No reachability is fabricated.** The daemon writes **no** reachability
signal of its own. A router that reached a peer through a relay is reachable
*through the relay*, not directly, and the tree records it that way: the
peer's profile carries no corroborating class and the corroboration count is
unchanged.

**The named target cannot widen the reach.** The named target is subject to
the same API-version floor and the same validated store as every drawn
candidate, and the build goes through the existing coordinator under the
existing qualification profile.

**No listener, no port, no discovery endpoint.** The observation table owns no
socket of its own, and the route opens no new connection: it names a peer for
the coordinator that already dials.

**No advertisement and no promotion.** The inventory's advertised count stays
zero and `router.version` stays pinned.

## Limitations

- The **production** SSU2 peer-test driver stays unbuilt; the `B` advertisement
  and `caps=f` stay with their owners.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology, and no stock router implements a router-level relay target.
- The target role still needs the **authoritative store**, which the
  composition root takes from the bootstrap. Plan 532's **Low** finding 2 is
  **unchanged**.
- The delivery is confined to the **controlled loopback profile**: the
  router's SSU2 dial target is loopback-only and the private-LAN variant
  accepts only RFC1918 IPv4. That bound is Plan 431's.
- Plan 530's Low finding 1 (the explicit-bind class lapses with the
  30-minute observation TTL and no re-record timer exists) remains open and
  is **not** this plan's subject.
- The observation's **lifetime** is this plan's own constant
  (`RELAY_SESSION_PEER_TTL_MS = 60000`), chosen so that a session that died
  while the router was not looking is not built toward. It is not derived
  from a reference implementation.

## Findings by severity

### None — nothing was found that blocks the plan

No High, Medium or Low finding remains open in the plan's own production
change. The guard's nine rules and the lane's seven rows are green, the floor
is green, and the negative controls all fire the surface that owns the
defect.

### Medium — discharged

**Plan 533's recorded limitation — the route by which a relayed peer is then
used for tunnel building — is discharged by this plan.** The relayed peer
becomes the route's **named** build target, and the build toward it installs
through the existing coordinator. The correction lives in this record and in
the registry's row; **Plan 533's closed closure record is not rewritten.**

**Plan 533's recorded limitation — the relayed peer's presence in the
peer-profile store — is discharged by this plan.** The store is fed by the
existing reconcile (`crates/i2pr-daemon/src/lib.rs:2192`,
`crates/i2pr-daemon/src/peer_reconcile.rs:363`), so the relayed peer's
validated record reaches the store the driver reads without any new work,
and the lane proves it with an **adopted** store.

### Low — unchanged

**Plan 532's Low finding 2 — the target role needs the bootstrap's store — is
unchanged.** It is the composition root's own line, and the requester has had
the same dependency since Plan 531.

### Low — recorded by this plan

1. **The observation is consumed by the take, and a refused named target
   consumes it too.** One deliberate reach yields one build attempt; a
   refused attempt does not leave a stale observation behind. **Owner:** the
   route's own line.
2. **The observation's lifetime is a local constant.** The 60-second window is
   this plan's own choice and is not derived from a reference implementation.
   **Owner:** the route's own line.

## Roadmap disposition

The `network-participation` roadmap gains one closed row, **534**, with this
record as its closure. The row states what the plan closed, what it did not,
and that it advertises nothing.

The roadmap's SSU2 relay phase now reaches **use**, not only reach. The three
roles of the relay protocol — introducer, requester and target — each have a
daemon-owned producer behind its own default-off opt-in, the requester
**establishes a session** with the peer it reached through the relay (Plan
533), and the router now **builds a tunnel toward that peer** (this plan).
What remains is **not** a producer gap: the production peer-test driver, the
`B` and `caps=f` advertisements, the public dial scope, and the stock-peer
half on Plan 493's topology.

## Unblock audit

| What was blocked | Disposition |
| --- | --- |
| Plan 533's **route** limitation (a relayed peer was never used for tunnel building) | **discharged** — the relayed peer is the route's named build target, and the build installs through the existing coordinator |
| Plan 533's **peer-profile store** limitation | **discharged** — the existing reconcile feeds the store, and the lane proves it with an adopted store |
| The **deliberate reach counted for nothing** | **closed** — the route names the peer the router reached, subject to the same floor |
| A **local condition written as a peer's fault** | **closed** — a missing record is counted as `peer_material_absent`, never as the peer's missing version |
| A **second build path** | **closed** — the named target's build goes through the existing coordinator, and the guard fails closed if a second appears |
| The transport's relay state machines and the reachability model | **present and unchanged** — the plan changes no transport file and no peer state |
| The **production** SSU2 peer-test driver | **unbuilt** — the model, the recorder and the verdict path exist; the driver is a successor's subject |
| The `B` advertisement and `caps=f` | **out of scope**, with their owners named |
| The public dial scope | **out of scope** — Plan 431 owns it |
| The stock-peer half | **blocked externally** on Plan 493's authorized two-host topology |
| The delivery's scope | **the controlled loopback profile** — Plan 431's bound |

## Handoff notes

- The plan's subject is the **route**: the router must build a tunnel toward
  the peer it reached through a relay.
- The plan adds **no** capability to the transport. It composes machinery the
  tree already owns — the version gate, the validated store, the coordinator
  and the peer-profile store — and gives them a way to name a peer. **No
  transport file is changed.**
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **public dial scope**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer half**
  all stay out of scope, with their owners named.
- The negative controls found two weaknesses in **this plan's own** tests; the
  lane and the guard were strengthened and the findings are recorded in the
  plan's amendment log, in this record, and in the registry's row.
