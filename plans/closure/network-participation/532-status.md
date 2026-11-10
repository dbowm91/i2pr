# Plan 532 — the router must answer a relay introduction

**Closure record.** The plan of record is
`plans/implementation/network-participation/532-the-router-must-answer-a-relay-introduction.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 532 is **closed**. The router can answer a relay introduction when it is
the peer an introducer named — the **target** half of the SSU2 relay protocol,
which was the last of its three roles with no producer.

Classification: **corrective** (the machinery existed and nothing used it: the
transport's `RelayTarget` state machine was complete and the runtime held the
table and exposed the seam that drives it, but nothing in production called the
seam, and the introduction was routed to the **introducer's** handoff and
refused there as not its role). Plan 490, 499 and 524 delivered the
**introducer** half; Plans 525 and 526 delivered the published set and its
advertisement; Plan 531 delivered the **requester** half. This plan delivers
the **target** half.
**Subsystem:** network-participation. **Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The defect — a target half with no producer.** The SSU2 relay protocol has
three roles. The **introducer** answers a relay request and forwards an
introduction. The **requester** asks an introducer to reach a peer it cannot
reach directly. The **target** is the peer that is reached: it receives the
introduction, verifies it, and **answers** it.

The target half existed as machinery with no caller. The transport's
`RelayTarget` state machine
(`crates/i2pr-transport-ssu2/src/relay.rs:1025`) verifies the introducer
context against the specification's preimage, enforces the global quota,
suppresses a replayed `(nonce, alice_hash)` pair, expires a pending
introduction deterministically and clears on shutdown. The runtime held that
table and exposed the seam that drives it
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2881`). Nothing in production called
the seam: its only caller was Plan 531's own lane.

Worse, the introduction was delivered to the **wrong** role. The session loop
routed a Relay Intro to the **introducer's** handoff unconditionally
(`crates/i2pr-runtime/src/ssu2_runtime.rs:7484`), and the introducer owner
refused it as `NotOurRole` (`crates/i2pr-daemon/src/ssu2_introducer.rs:345`),
so the target's transport table was never fed. And the target's **outbound**
half had no producer either: nothing emitted the Relay Response accept and
nothing emitted the hole punch outside Plan 531's lane.

**The division of ownership.** The daemon owns the *policy*; the runtime owns
the *mechanics*.

`crates/i2pr-daemon/src/relay_target.rs` holds the policy. It refuses every
introduction while the operator's opt-in is off. It reads the requester's
**validated** RouterInfo out of the authoritative store
(`crates/i2pr-daemon/src/relay_target.rs:336`) and takes the requester's
**signing** key and **introduction** key from that validated record, through
the transport's strict `Ssu2RouterAddress::parse` — unvalidated bytes are
never parsed. It drives the runtime's existing `on_target_relay_intro` seam, so
the transport's own state machine is used and not re-implemented. It signs the
accept with the router's **own** identity key, over the **introducer-bound**
preimage `relay_response_preimage(bob_hash, …)`, and it hands the hole punch's
pieces to the runtime's outbound seam. Every outcome is a typed, counted
reason.

The runtime owns the sockets, the session, the bounded tables and the
transport's verification. The transport's relay state machines are **used, not
edited**: **Plan 532 changes no transport file at all.**

**The introduction has its own bounded handoff.** The runtime adds a **fourth**
channel for the target's events
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2231`) with the same ceiling as the
relay handoff (`max_inbound_relay_queue`, default 16), and the session loop
routes an inbound Relay Intro to it **iff** the operator's opt-in is on. With
the opt-in off — the default — the introduction keeps travelling the
introducer's handoff exactly as before, and the introducer owner refuses it as
not its role. The lane proves **both** halves.

**The hole punch is staged, never sessioned.** The outbound seam
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2923`) builds the datagram with the
transport's own codec, seals it under the **requester's** introduction key,
draws a nonzero packet number, and stages it through the runtime's existing
bounded queue as `BandwidthTrafficClass::Control`. It travels out of session.
Its destination is checked against the **union** of the two dial profiles the
runtime offers (`crates/i2pr-runtime/src/ssu2_runtime.rs:5427`) — a loopback
address of either family, or an RFC1918 IPv4 literal — so the target role
cannot widen the router's reach beyond what the router already dials.

**The transport's own table is the authority on a replay.** The daemon keeps
**no** introduction table. After the answer is emitted, the runtime
**consumes** the pending introduction
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1035`), and the transport's `seen`
set suppresses a replayed `(nonce, alice_hash)` pair. A replay produces no
second accept and no second hole punch.

**The answer gains no corroborating class.** A router that answers an
introduction is reachable **through the relay**, not directly. The daemon
writes **no** reachability signal of its own and the target's owner touches no
tracker; the transport's class-4 contradiction stays class 4 and contradicting,
and the corroboration floor (2) and the contradiction floor (1) are untouched.

**The exchange completes.** The lane drives three **real** routers over real
loopback UDP: Alice asks Bob for Charlie, Bob introduces Charlie, **Charlie
verifies the introduction through the transport's own target state machine and
answers**, Bob verifies the accept and forwards it, Alice correlates it, and
Charlie's hole punch arrives out-of-session. The relay-firewalled fact is
recorded and the router's corroboration count is **unchanged**.

**One opt-in only.** `ssu2.relay_target`, default **off**, refused at parse
time without `ssu2.enabled` (`crates/i2pr-daemon/src/config.rs:3566`). Every
refusal is a typed, counted reason — sixteen of them, from `Disabled` to
`StagingRefused` — and **no refusal writes into a peer's record**: a requester
this router does not hold is a **local** condition, never a peer's profile.

## What the plan did not close

- The **requester's establishment dial** that would consume the accept's token
  is **not in this tree**. The target mints a fresh nonzero token with the
  router's own randomness and the transport refuses a zero token, so the accept
  is well formed, but the handshake the hole punch initiates is performed by the
  transport's normal session path and the token's consumer does not exist yet.
  The token's value is therefore opaque to the tree today.
- The **requester owner has no deferred-punch buffer.** The transport's own
  requester table requires the accept to be correlated **before** a hole punch
  is processed, while the hole punch travels one hop and the accept two, so on
  loopback the punch reaches the requester's handoff first. The lane holds one
  punch aside until the accept is correlated and presents it afterwards; the
  production drain and the production requester owner are unchanged. This is a
  **Medium** finding with a named owner.
- The target role needs the **authoritative store**, which the composition
  root takes from the bootstrap
  (`crates/i2pr-daemon/src/lib.rs:5631`). With no bootstrap the target's
  introductions are never drained. The requester has the same dependency.
- The delivery is confined to the **controlled loopback profile**: the router's
  SSU2 dial target is loopback-only and the private-LAN variant accepts only
  RFC1918 IPv4, so the target can only answer an introduction for a requester
  on a local or private network. That bound is Plan 431's.
- The **production** SSU2 peer-test driver stays unbuilt; the `B` advertisement
  and `caps=f` stay with their owners; and the **stock-peer** half stays
  blocked on Plan 493's authorized two-host topology.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `e695d0a4` | the registration: the plan of record, the registry row and the roadmap row |
| `65c0baca` | the production change: the daemon's target owner, the runtime's target route and outbound hole-punch seam, the config opt-in, the composition, and the one mechanical line another plan's lane file needed |
| `5e63b83d` | the daemon lane and the daemon guard |
| `efe9fe74` | the documentation, the inventory and the plan's amendment log |

## Requirement-to-evidence matrix

Every acceptance criterion of the plan's §14, the file and line that
satisfies it, and the test that proves it.

| # | Criterion | Satisfied by | Proven by |
| --- | --- | --- | --- |
| 1 | the opt-in is **off** by default and refused without the SSU2 service | `crates/i2pr-daemon/src/config.rs:650` (the field), `:1461` (the default), `:3566` (the refusal), `crates/i2pr-daemon/src/router_i2np.rs:1558` (the mapping into the runtime) | guard rule 4 `the_opt_in_defaults_off_and_needs_the_service`; the lane row `the_opt_in_is_off_by_default_and_refused_without_the_service` |
| 2 | an inbound Relay Intro reaches the **target's** handoff, and the introducer's handoff no longer carries it | `crates/i2pr-runtime/src/ssu2_runtime.rs:7484` (the route), `:2231` (the target's own bounded channel), `crates/i2pr-runtime/src/ssu2_runtime.rs:1843` (the pull seam) | guard rule 6 `the_introduction_reaches_the_targets_handoff`; the lane rows `the_introduction_reaches_the_targets_own_handoff` and `the_introduction_keeps_travelling_the_introducer_handoff_without_the_opt_in` |
| 3 | the target verifies the introduction through the transport's own target state machine, and a forged, stale, unknown or replayed introduction is refused and counted | `crates/i2pr-daemon/src/relay_target.rs:379` (the drive step), `crates/i2pr-runtime/src/ssu2_runtime.rs:2881` (the seam), `crates/i2pr-transport-ssu2/src/relay.rs:1068` (the transport's own verification) | the lane row `the_target_verifies_the_introduction` |
| 4 | the target **answers**: the accept reaches the introducer, which verifies it and forwards it, and the requester correlates it | `crates/i2pr-daemon/src/relay_target.rs:438` (the answer step), `crates/i2pr-daemon/src/ssu2_relay_drain.rs:264` (the drain) | the lane row `the_target_answers_the_introduction_and_punches_the_hole` |
| 5 | the target **punches the hole** out of session, the requester receives it, and the relay-firewalled signal is recorded with **no** corroborating class | `crates/i2pr-runtime/src/ssu2_runtime.rs:2923` (the outbound seam), `:5427` (the dial profile), `crates/i2pr-daemon/src/relay_requester.rs:940` (the requester's recorder call) | guard rule 7 `the_hole_punch_is_staged_and_never_sessioned`; the lane row `the_target_answers_the_introduction_and_punches_the_hole` |
| 6 | every refusal is typed and counted by reason, and no refusal writes into a peer's profile | `crates/i2pr-daemon/src/relay_target.rs:62` (the sixteen reasons), `:132` (one counter per reason) | the lane row `every_refusal_is_typed_and_counted_and_writes_nothing` |
| 7 | the transport's relay state machines and the reachability model are untouched, and both floors are unchanged | `crates/i2pr-transport-ssu2/src/relay.rs:1068` (the target verification, untouched), `crates/i2pr-transport/src/reachability.rs` (the model, untouched); **the plan changes no transport file** | guard rule 5 `the_transports_state_machines_and_model_are_untouched` |
| 8 | the lane and the guard are green, every guard rule has an inverted probe that fires it, and every rule fails closed on an absent item | `crates/i2pr-daemon/tests/ssu2_relay_target.rs` (8 rows), `crates/i2pr-daemon/tests/ssu2_relay_target_boundary.rs` (9 rules, 24 inverted probes, 8 fail-closed probes, 41 rows) | the floor's `plan532-relay-target-lane` and `plan532-relay-target-boundary` steps |
| 9 | the routine floor is green on the closing tree | the floor table below | the floor run |
| 10 | `specs/support.toml` records the surface as `experimental` and `advertised = false`; the advertised count stays **zero**; `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` stay byte-identical; no boundary script is weakened | `specs/support.toml`, surface `daemon.relay-target`; `crates/i2pr-netdb/src/local.rs` (`CONTROLLED_ROUTER_VERSION`); neither manifest is touched; the diff touches **no** script under `scripts/` | `python3 scripts/check-tooling-inventory.py`; the counts are 125 surfaces, 120 experimental, 4 deferred, 1 ready, **0 advertised**; `git status` on both manifests is empty |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS — no warnings |
| `cargo test --locked -p i2pr-transport-ssu2 --all-targets` | PASS |
| `cargo test --locked -p i2pr-runtime --all-targets` | PASS |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_target -- --test-threads=1` | PASS — 8 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_target_boundary -- --test-threads=1` | PASS — 41 rows |
| `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | PASS — 11/11 negative cases each failed on their own gate |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests, OK |

## The routine floor

The floor ran **once**, on the documentation tree `efe9fe74`, and it is
**green**: **103 PASS, 0 FAIL**. Every step is recorded in
`/tmp/532-floor.log`, with each step's own log under `/tmp/532f-<step>.log`.
The floor is a plain sequence of `step` runs that records each step's exit
status; no step is skipped, no `|| true`, no `continue-on-error` and no
filename filtering. The workspace step runs with `--no-fail-fast`, so every
test binary's result is reported rather than the run stopping at the first
failure.

Plan 532's floor is Plan 531's floor plus two steps — the daemon lane and the
daemon guard for the relay target. **No other step changed, and no other
plan's guard file was touched.**

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

**No floor-caught defect.** Unlike Plan 531, whose first floor run caught two
real defects in that plan's own work, this plan's first floor run was green.
The defect that Plan 531's floor caught in the guard — the tracker's call
literal inside a string needle, which the m12 boundary checker reported as a
violation — is **structurally avoided** here: Plan 532's guard splits the same
needle with `concat!(".reachability", ".record(")`, and the
`m12-floodfill-boundaries` step is green on the closing tree without the
checker being touched. **The checker was not weakened**: widening its
`--glob '!*/tests/*'` exclusion would have *weakened* the check, so it stands.

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be reported.
Every probe mutates an **in-memory copy** of the source, so **no file on disk
is written**. Every rule **fails closed** when the item it names is absent —
the guard reports the missing item rather than passing.

### Plan 532's guard — `crates/i2pr-daemon/tests/ssu2_relay_target_boundary.rs`

Nine rules, twenty-four inverted probes, eight fail-closed probes. All 41 rows
green.

| Rule | What it forbids | Its inverted probes |
| --- | --- | --- |
| 1 | an owner that does not use the runtime's seam, a daemon-side copy of the transport's target table, or a runtime that skips the peer-relay service | `the_guard_reports_an_owner_that_does_not_use_the_runtime_seam`, `the_guard_reports_a_daemon_side_target_table`, `the_guard_reports_a_runtime_that_does_not_drive_the_peer_relay_service` |
| 2 | an owner that owns a socket, spawns a task, adds infrastructure of its own, or writes a reachability signal | `the_guard_reports_an_owner_that_owns_a_socket`, `the_guard_reports_an_owner_that_spawns_a_task`, `the_guard_reports_an_owner_that_writes_a_reachability_signal` |
| 3 | a read that leaves the validated store, an owner that parses unvalidated bytes, an accept that is not signed over the specification's preimage, an accept that is not bound to the introducer, a hole punch sealed under the wrong key, or a target with its own signing seam | `the_guard_reports_a_read_that_leaves_the_validated_store`, `the_guard_reports_an_owner_that_parses_unvalidated_bytes`, `the_guard_reports_an_accept_that_is_not_signed_over_the_preimage`, `the_guard_reports_an_accept_that_is_not_bound_to_the_introducer`, `the_guard_reports_a_hole_punch_sealed_under_the_wrong_key`, `the_guard_reports_a_target_that_signs_with_its_own_seam` |
| 4 | an opt-in that defaults on, runs without the service, or is never mapped into the runtime | `the_guard_reports_an_opt_in_that_defaults_on`, `the_guard_reports_an_opt_in_that_runs_without_the_service`, `the_guard_reports_an_unmapped_opt_in` |
| 5 | a changed transport refusal order, or a moved corroboration class | `the_guard_reports_a_changed_transport_refusal_order`, `the_guard_reports_a_moved_corroboration_class` |
| 6 | a route that keeps the introduction on the introducer's handoff, a route that never fills the target's handoff, or a datagram path with no target batch | `the_guard_reports_a_route_that_keeps_the_introduction_on_the_introducer`, `the_guard_reports_a_route_that_keeps_both_handoffs` |
| 7 | a hole punch that ignores the dial profile, or a hole punch that rides a session | `the_guard_reports_a_hole_punch_that_ignores_the_dial_profile`, `the_guard_reports_a_hole_punch_that_rides_a_session` |
| 8 | a second configuration key beside the one opt-in | `the_guard_reports_a_second_configuration_key` |
| 9 | an owner that is never constructed, or never driven on the existing tick | `the_guard_reports_an_owner_that_is_never_constructed`, `the_guard_reports_an_owner_that_is_never_driven` |

Eight further probes remove the item each rule names — the owner, the
material read, the opt-in's default, the transport's verification, the route,
the outbound seam, the configuration surface and the drain — and every rule
reports the absence rather than passing. They are the
`the_guard_fails_closed_when_*` rows.

### The negative control against the **real** tree

The guard's probes are in-memory. The tree was therefore also mutated **on
disk** — one mutation at a time, each restored byte-identically — and the guard
and the lane were re-run. The script is `/tmp/532-negative.py`.

| Mutation | The guard | The lane |
| --- | --- | --- |
| NC-1 — the runtime's route sends the introduction back to the introducer's handoff | **FAILED**, as it must (`the_introduction_reaches_the_targets_handoff`) | **FAILED**, as it must (`the_introduction_reaches_the_targets_own_handoff`) |
| NC-2 — the daemon's target owner writes a reachability signal of its own | **FAILED**, as it must (`the_owner_adds_no_infrastructure`) | green — the lane is a behaviour lane and does not read the owner's source |
| NC-3 — the daemon re-implements the transport's target table | **FAILED**, as it must (`the_owner_is_the_daemons_and_the_mechanics_the_runtimes`) | green — the lane is a behaviour lane |
| NC-4 — the runtime's hole punch ignores the dial profile | **FAILED**, as it must (`the_hole_punch_is_staged_and_never_sessioned`) | green — the lane's destinations are inside the profile |
| NC-5 — the accept carries a zero token | green — the token's value is opaque to the tree today | **FAILED**, as it must (`the_target_answers_the_introduction_and_punches_the_hole`) |
| NC-6 — the accept is not bound to the introducer | green — the binding is the guard's shape | **FAILED**, as it must (`the_target_answers_the_introduction_and_punches_the_hole`) |

The split is honest and is recorded rather than hidden. The **lane** is the
authority for the **behaviour**: the target verifies, answers, forwards and
punches, and the recorded signal contradicts. The **guard** is the authority
for the **shape**: no second target table in the daemon, no transport model
change, no daemon-side reachability write, the introduction routed to the
target's own handoff, the hole punch staged rather than sessioned, exactly one
configuration key, and the owner driven on the existing tick.

Every mutation was restored **byte-identically** — the restore is verified by
a SHA-256 comparison of the file's bytes — and the working tree was verified
to be exactly as it was before each mutation. The only entries `git status`
reported after the run were the two **new** untracked test files, which are
this plan's own work.

## Acceptance criteria

The plan's §14 lists ten criteria and four stop conditions.

1. the opt-in is **off** by default and refused without the SSU2 service —
   **met**;
2. an inbound Relay Intro reaches the **target's** handoff, and the
   introducer's handoff no longer carries it — **met** (with the opt-in on;
   with the opt-in off the introduction keeps travelling the introducer's
   handoff, as §17.2 of the plan of record records);
3. the target verifies the introduction through the transport's own target
   state machine, and a forged, stale, unknown or replayed introduction is
   refused and counted — **met**;
4. the target **answers**: the accept reaches the introducer, which verifies
   it and forwards it, and the requester correlates it — **met**;
5. the target **punches the hole** out of session, the requester receives it,
   and the relay-firewalled signal is recorded with **no** corroborating class
   — **met**;
6. every refusal is typed and counted by reason, and no refusal writes into a
   peer's profile — **met**;
7. the transport's relay state machines and the reachability model are
   untouched, and both floors are unchanged — **met**;
8. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, and every rule fails closed on an absent item — **met**;
9. the routine floor is green on the closing tree — **met**;
10. `specs/support.toml` records the surface as `experimental` and
    `advertised = false`; the advertised count stays **zero**;
    `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` stay
    byte-identical; no boundary script is weakened — **met**.

**No stop condition fired.** The target did **not** turn out to need a change
to the transport's relay state machines; the requester's record did **not**
turn out to need a **network** lookup that the authoritative store cannot
serve; the delivery did **not** turn out to need a new task, a new timer, a
new listener or a new socket; and the plan did **not** turn out to need the
production peer-test driver or a public dial scope.

## Invariant review

The plan's §4 lists twelve invariants. Each is reviewed against the closing
tree.

1. **The target is opt-in and off by default.** `ssu2.relay_target` defaults
   to `false` (`crates/i2pr-daemon/src/config.rs:1461`), the owner is
   constructed only when the operator opted in
   (`crates/i2pr-daemon/src/lib.rs:4854`), and the runtime's route is gated on
   the same flag (`crates/i2pr-runtime/src/ssu2_runtime.rs:7484`). A router
   that has not asked to answer a relay introduction answers no introduction
   and emits no hole punch.
2. **The owner holds no task, no timer, no listener and no socket.** The
   owner is a plain struct holding the runtime, the signer and the counters
   (`crates/i2pr-daemon/src/relay_target.rs:220`); the product's **existing**
   relay tick drives it (`crates/i2pr-daemon/src/lib.rs:5642`). The guard
   fails closed if a socket, a task or a timer appears.
3. **Every table the owner holds is bounded, and the daemon adds no second
   table.** The owner holds **no** table at all: the transport's own target
   table is the authority on pending introductions, and the guard fails closed
   if a daemon-side copy appears.
4. **An introduction is verified only from validated material.** The
   requester's keys come from the requester's validated RouterInfo through the
   transport's strict parser (`crates/i2pr-daemon/src/relay_target.rs:336`),
   and the verification itself is the transport's own
   (`crates/i2pr-runtime/src/ssu2_runtime.rs:2881`).
5. **The reply is one per introduction.** The transport's own replay
   suppression decides, and the daemon keeps no introduction table; the
   pending introduction is **consumed** after the answer
   (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1035`).
6. **The accept is signed with this router's own identity key and is sent to
   the introducer, on the session the introduction arrived on.** The signature
   is over the introducer-bound preimage, and the accept is carried by
   `queue_relay_response` on the introducer's live session.
7. **The hole punch is sealed under the requester's introduction key and
   travels out of session.** It is staged through the runtime's existing
   bounded queue (`crates/i2pr-runtime/src/ssu2_runtime.rs:2923`) and never
   through a session.
8. **The hole punch's destination obeys the dial profile.** The check is the
   union of the two dial profiles the runtime offers
   (`crates/i2pr-runtime/src/ssu2_runtime.rs:5427`), so the target role cannot
   widen the router's reach.
9. **Every refusal is typed and counted by reason, and no refusal writes into
   a peer's profile.** Sixteen reasons, one counter each
   (`crates/i2pr-daemon/src/relay_target.rs:62`, `:132`); a requester the
   router does not hold is a **local** condition.
10. **The transport's relay state machines, the reachability model and both
    floors are untouched.** The plan changes **no transport file**; the
    corroboration floor (2), the contradiction floor (1) and the class-4
    contradiction are unchanged.
11. **No configuration key is added beside the one opt-in, and no default
    moves.** The `[ssu2]` surface gains exactly one boolean; the guard fails
    closed if a second key appears.
12. **Nothing is promoted.** `router.version` stays `0.9.62` and the
    inventory's advertised count stays **zero**.

## Failure, cancellation, restart, and contention review

**Failure.** Every refusal is typed and counted by reason, and leaves no state
behind: no accept, no hole punch, no pending introduction. An introduction that
cannot be verified records nothing. The transport's own target table is the
authority on a pending introduction, and the pending entry is consumed after
the answer, so a failure **after** the verification but **before** the
staging leaves the transport's own pending entry to expire on the transport's
own deadline (`RELAY_REQUEST_TIMEOUT_MS`), and the daemon keeps nothing.

**Cancellation.** The plan adds **no task**. The owner's work is a single
synchronous step on the product's existing tick.

**Restart.** Nothing is persisted. The transport's target table is live state
and is rebuilt empty; a restart answers no introduction and punches no hole.

**Contention.** The owner takes the daemon's own locks, in the order the
daemon's other owners use, and introduces **no new lock** beyond the runtime's
existing state lock. The target's handoff is a separate bounded channel with
the same ceiling as the relay handoff, so the target's events cannot starve
the introducer's.

## Migration and compatibility review

**No wire change.** The relay blocks, their types, their lengths and their
signatures are exactly what the tree already sends and receives. The accept is
signed over the specification's existing preimage, and the hole punch is built
by the transport's own codec.

**No configuration migration.** One boolean is added, off by default and
refused without the SSU2 service. A configuration that named no such key
behaves exactly as before.

**A new reading for the router.** A router that has asked for the target role
may answer an introduction. It publishes nothing new and advertises nothing
new.

**No change to the transport's model.** The relay-firewalled signal already
exists and already contradicts; a target that answers gains no corroborating
class.

## Security review

**No secret is copied, logged or serialized.** The router's signing seed never
leaves the identity bundle: the seam signs one preimage and returns the
signature bytes. The target's owner holds a `Arc<dyn RelayTargetSigner>` and
never sees a seed. The counters carry counts only: no peer hash, no address,
no key, no nonce, no tag and no payload.

**No unvalidated bytes are parsed.** The requester's signing and introduction
keys come from the requester's **validated** RouterInfo, read out of the
authoritative store and parsed with the transport's strict
`Ssu2RouterAddress::parse`. The guard fails closed if the owner parses
unvalidated bytes.

**No reachability is fabricated.** The target's owner writes **no** reachability
signal and touches no tracker. A router that answers an introduction is
reachable **through the relay**, not directly, and the tree records it that
way.

**The hole punch cannot widen the reach.** The destination is checked against
the union of the two dial profiles before the datagram is built, so the target
role can only reach where the router already dials.

**The transport's verification is used, not re-implemented.** The daemon adds
no second table and no second decision, so there is no second place where an
introduction could be admitted.

**No listener, no port, no discovery endpoint.** The target role owns no
socket of its own and adds no task.

## Limitations

- The **requester's establishment dial** that would consume the accept's token
  is **not in this tree**. The token is minted fresh and nonzero, and the
  transport refuses a zero token, but nothing consumes it yet; the handshake
  the hole punch initiates is performed by the transport's normal session path.
- The **requester owner has no deferred-punch buffer.** The transport's own
  requester table requires the accept to be correlated before a hole punch is
  processed, while the punch travels one hop and the accept two. The lane
  holds one punch aside until the accept is correlated and presents it
  afterwards; the production drain and the production requester owner are
  unchanged. **Owner:** the successor that completes the requester's
  establishment path.
- The target role needs the **authoritative store**, which the composition
  root takes from the bootstrap (`crates/i2pr-daemon/src/lib.rs:5631`); with
  no bootstrap the target's introductions are never drained. The requester has
  the same dependency, so this is a property of the composition root.
- The delivery is confined to the **controlled loopback profile**: the
  router's SSU2 dial target is loopback-only and the private-LAN variant
  accepts only RFC1918 IPv4, so the target can only answer an introduction for
  a requester on a local or private network. That bound is Plan 431's.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology, and no stock router implements a router-level relay target.
- The **production** SSU2 peer-test driver stays unbuilt; the `B` advertisement
  and `caps=f` stay with their owners.
- Plan 530's Low finding 1 (the explicit-bind class lapses with the
  30-minute observation TTL, and no re-record timer exists) remains open and
  is **not** this plan's subject.

## Findings by severity

### None — nothing was found that blocks the plan

No High, Medium or Low finding was found in the plan's own production change.
The guard's nine rules and the lane's eight rows are green, the floor is
green, and the negative controls all fire the surface that owns the defect.

### Medium — recorded, with its owner

1. **The requester owner has no deferred-punch buffer.** The transport's own
   requester table moves a request to `AwaitingHolePunch` only when the
   accept has been correlated, and it refuses a hole punch that arrives
   before that. The hole punch travels **one** hop and the accept travels
   **two** — the introducer must receive, verify and forward it — so on
   loopback the punch reaches the requester's handoff **first**. In the
   production wiring the requester owner would refuse that punch as
   `WrongRole` and lose it. The lane holds the punch aside until the accept
   is correlated, so the lane's positive result is earned; the production
   path's behaviour under that ordering is **not** fixed by this plan.
   **Owner:** the successor that completes the requester's establishment path
   (the token's consumer and the deferred punch). **Not this plan's scope** —
   the plan's §6 scopes the production changes to the target role, and the
   ordering hazard was found during the implementation and is recorded
   additively in §17.2 of the plan of record.

### Low — recorded, with its owner

1. **The accept's token has no consumer.** The target mints a fresh nonzero
   token and the transport refuses a zero token, so the accept is well formed,
   but the requester's establishment dial that would consume it is not in the
   tree. **Owner:** the same successor.
2. **The target role needs the bootstrap's store.** The composition root
   drains the target's handoff only when the daemon holds a `Bootstrap`
   (`crates/i2pr-daemon/src/lib.rs:5631`). **Owner:** the composition root's
   own line; the requester has the same dependency, so this is not new to
   this plan.

## Roadmap disposition

The `network-participation` roadmap gains one closed row, **532**, with this
record as its closure. The row states what the plan closed, what it did not,
and that it advertises nothing.

The roadmap's SSU2 relay phase is now complete on the **producer** side: the
three roles of the relay protocol — introducer, requester and target — each
have a daemon-owned producer behind its own default-off opt-in, and the
transport's contradicting *relay-firewalled* class has a producer on the
requester's side. What remains is **not** a producer gap: the requester's
establishment path (the token's consumer and the deferred punch), the
production peer-test driver, the `B` and `caps=f` advertisements, the public
dial scope, and the stock-peer half on Plan 493's topology.

## Unblock audit

| What was blocked | Disposition |
| --- | --- |
| The **target** side's response emission, recorded unwired by Plan 531 | **wired** — the daemon's `RelayTargetOwner` signs the accept and stages the hole punch; the correction lives in this record and in the registry row, never in the closed record |
| Plan 527's Low finding 1 (the relay-success indication had no producer) | **discharged** — the requester half by Plan 531, the target half by this plan |
| Plan 528's, Plan 529's and Plan 530's repetitions of that finding | **discharged** — the corrections live in this record and in the registry row |
| The transport's `RelayFirewalledSignal` producer | **present** — recorded through the runtime's own recorder on the requester's side |
| The relay protocol's three roles | **all three have producers** — introducer, requester and target |
| The requester's establishment path (the token's consumer, the deferred punch) | **open** — the Medium and Low findings above, with the successor named |
| The target role's store dependency | **open** — the composition root's line; the requester has the same dependency |
| The production SSU2 peer-test driver | **unbuilt** — the model, the recorder and the verdict path exist; the driver is the successor's subject |
| The `B` advertisement and `caps=f` | **out of scope**, with their owners named |
| The public dial scope | **out of scope** — Plan 431 owns it |
| The stock-peer half | **blocked externally** on Plan 493's authorized two-host topology |
| The delivery's scope | **the controlled loopback profile** — Plan 431's bound |

## Handoff notes

- The plan's subject is the **target** half of the SSU2 relay protocol: the
  router must be able to answer a relay introduction when it is the peer
  being reached.
- The plan adds **no** capability to the transport. It composes machinery the
  tree already owns — the transport's target state machine, the runtime's
  target seam, the transport's hole-punch codec and the runtime's bounded
  staging queue — and gives them a caller. **No transport file is changed.**
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **public dial scope**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer half**
  all stay out of scope, with their owners named.
- The **requester's establishment path** is the next gap: the token's consumer
  and the deferred-punch buffer. It is a **requester** concern and is named
  in the findings above.
