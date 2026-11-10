# Plan 531 — the router must reach a peer through its introducer

**Closure record.** The plan of record is
`plans/implementation/network-participation/531-the-router-must-reach-a-peer-through-its-introducer.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 531 is **closed**. The router can reach a peer that publishes an
introducer set — the **requester** half of the SSU2 relay protocol, which was
the last half of it with no producer.

Classification: **corrective** (the machinery existed and nothing used it: the
transport's `RelayRequester` state machine had no production caller, the
runtime's `start_relay_request` seam had no caller, and the transport's
`RelayFirewalledSignal` — class 4, contradicting — had **no producer at all**,
so a firewalled peer was unreachable and the tree had no way to record that
fact either). Plan 490, 499 and 524 delivered the **introducer** half; Plans
525 and 526 delivered the published set and its advertisement. This plan
delivers the **requester** half.
**Subsystem:** network-participation. **Closure date:** 2026-11-10.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The defect — a requester half with no producer.** The SSU2 relay protocol
has two halves. The **introducer** half — the router that answers a relay
request and introduces a target — was already in the tree: Plan 490's owner,
Plan 499's live tag issuance and Plan 524's relay-tag request. The
**requester** half was not:

- the transport's `RelayRequester` state machine
  (`crates/i2pr-transport-ssu2/src/relay.rs`) existed and had **no production
  caller**;
- the runtime's `start_relay_request` seam existed and had **no caller**;
- the runtime's hole-punch handler existed and had **no producer and no
  route**;
- the transport's `RelayFirewalledSignal` — class 4, contradicting
  (`crates/i2pr-transport/src/reachability.rs:197`) — had **no producer at
  all**.

**The consequence.** A router behind a firewall could not reach a peer that
publishes an introducer set, and the tree had no way to say so: the one signal
that names the fact had no writer. Plan 528 recorded the gap as an open
finding and named its owner.

**What the tree does now.**

- The **policy** is the daemon's. `crates/i2pr-daemon/src/relay_requester.rs`
  holds a `RelayRequesterOwner` (`:545`) that selects a target from the
  **validated** RouterInfo store, resolves the introducer by **hash** from
  the same store, draws a fresh **nonzero** nonce from the router's own
  randomness, signs the request with the router's own identity, and starts it
  through the runtime's existing `start_relay_request` seam. The pure reads
  are `read_published_introducers` (`:233`) and `read_introducer_endpoint`
  (`:273`), and the selection is `drive_relay_request` (`:1025`).
- The **mechanics** stay in the runtime, which owns the sockets, the session,
  the bounded tables and the transport's verification. The transport's relay
  state machines are **used, not edited**: the only transport change is a
  `pub use RequesterState` re-export
  (`crates/i2pr-transport-ssu2/src/lib.rs:100`).
- The runtime adds an out-of-session **hole-punch route**
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:7635`) and a **bounded handoff of
  its own** (`:1709`, `:1044`), with the same ceiling as the relay handoff. A
  hole punch travels **out of session** — sealed under the *receiver's*
  introduction key, naming neither a session nor a peer hash — so it cannot
  ride the session-shaped relay handoff.
- A Relay Response is routed to the requester's handoff **iff** it correlates
  with one of the router's own live request nonces
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:7310`). Every other response keeps
  travelling the introducer's handoff exactly as before, and the existing
  `drain_relay_events` (`crates/i2pr-daemon/src/ssu2_relay_drain.rs:266`) is
  **byte-identical**.
- The daemon records **no reachability signal of its own**. It calls the
  **runtime's** `note_relay_firewalled`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3579`), so the transport's class-4
  contradiction is recorded through its own owner, and no corroborating class
  is ever claimed for a relayed peer.
- **One opt-in only**: `ssu2.relay_requester`, default **off**
  (`crates/i2pr-daemon/src/config.rs:648`), refused at parse time without
  `ssu2.enabled` (`:3536`).
- Every refusal is a **typed**, **counted** reason — fifteen of them, from
  `Disabled` to `TargetUnknown` (`crates/i2pr-daemon/src/relay_requester.rs:304`) —
  and **no refusal writes into a peer's record**.

**The exchange completes.** The lane drives three real routers over real
loopback UDP: the requester asks the introducer for the target, the
introducer verifies the request and introduces the target, the target accepts,
the introducer relays the response back, and the target's hole punch arrives
out-of-session. The response is **correlated**, the hole punch is **routed**,
the relay-firewalled signal is **recorded**, and the router's corroboration
count is **unchanged**.

## What the plan did not close

- The **target** side's response emission has **no product wiring** in this
  tree. The transport says the *caller* emits the accept and the hole punch,
  and no daemon role does that. The **requester** half is production code
  throughout; the **target** half is not. The lane completes that half over
  the transport's own preimage, with the target's own key, and the introducer
  verifies it for real.
- The **production** SSU2 peer-test driver stays unbuilt. Its choreography
  question is named: the tree's Alice role needs the helper's identity up
  front, while the specification has Bob select the helper.
- The **`B` advertisement** and the **`caps=f` advertisement** stay out of
  scope, with their owners named.
- The **public dial scope** stays out of scope; Plan 431 owns it.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology.
- The delivery is confined to the **controlled loopback profile**: the
  router's SSU2 dial target is loopback-only and the private-LAN variant
  accepts only RFC1918 IPv4.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `0e115a72` | the registration: the plan of record, the registry row and the roadmap row |
| `a38d7b38` | the production change: the daemon's requester owner, the runtime's requester seams and hole-punch route, the config opt-in, the composition, and the one mechanical line another plan's lane file needed |
| `4b2ab69e` | the daemon lane and the daemon guard |
| `fe4267de` | the documentation, the inventory and the plan's amendment log |
| `c4ec5923` | the two defects the first floor run caught: the in-crate unit test's identity material, and the guard's needle literal |

## Requirement-to-evidence matrix

Every acceptance criterion of the plan's §14, the file and line that
satisfies it, and the test that proves it.

| # | Criterion | Satisfied by | Proven by |
| --- | --- | --- | --- |
| 1 | the opt-in is **off** by default and refused without the SSU2 service | `crates/i2pr-daemon/src/config.rs:648` (the field), `:3536` (the refusal), `crates/i2pr-daemon/src/router_i2np.rs:1557` (the mapping into the runtime) | guard rule 4 `the_opt_in_defaults_off_and_needs_the_service`; the lane row `the_opt_in_is_off_by_default_and_refused_without_the_service` |
| 2 | the owner selects a target that publishes an introducer set, from **validated** material, and resolves the introducer by hash | `crates/i2pr-daemon/src/relay_requester.rs:233` (the published read), `:273` (the resolution), `:442` (`EligibleTarget`) | guard rule 3 `the_request_is_started_only_from_validated_material`; the lane rows `only_a_peer_that_publishes_an_introducer_set_is_a_target` and `the_reads_never_mutate_the_store` |
| 3 | the request carries the published tag and the target's own hash | `crates/i2pr-daemon/src/relay_requester.rs:721` (the builder), the transport's own preimage | the lane row `the_router_reaches_a_peer_through_its_introducer` |
| 4 | the exchange **completes** over real loopback UDP between three real routers, and the response is correlated and the hole punch routed | `crates/i2pr-runtime/src/ssu2_runtime.rs:7635` (the hole-punch route), `:7310` (the correlation), `crates/i2pr-daemon/src/ssu2_relay_drain.rs:167` (the drain) | the lane row `the_router_reaches_a_peer_through_its_introducer` |
| 5 | a completed exchange records the relay-firewalled signal and **never** a corroborating class | `crates/i2pr-daemon/src/relay_requester.rs:929` (the owner's call), `crates/i2pr-runtime/src/ssu2_runtime.rs:3579` (the runtime's recorder) | guard rule 6 `the_relay_firewalled_fact_is_a_contradiction`; the lane row `the_router_reaches_a_peer_through_its_introducer` |
| 6 | every refusal is typed and counted by reason, and no refusal writes into a peer's record | `crates/i2pr-daemon/src/relay_requester.rs:304` (the fifteen reasons), `:348` (one counter per reason) | the lane row `every_refusal_is_typed_and_counted_and_writes_nothing` |
| 7 | the transport's relay state machines and the reachability model are untouched, and both floors are unchanged | `crates/i2pr-transport-ssu2/src/lib.rs:100` (the only transport change, a re-export), `crates/i2pr-transport/src/reachability.rs:182`, `:197`, `:229` (the model, untouched) | guard rule 5 `the_transports_state_machines_and_model_are_untouched`; the lane row `the_transports_reachability_model_is_untouched` |
| 8 | the lane and the guard are green, every guard rule has an inverted probe that fires it, and every rule fails closed on an absent item | `crates/i2pr-daemon/tests/ssu2_relay_requester.rs` (8 rows), `crates/i2pr-daemon/tests/ssu2_relay_requester_boundary.rs` (8 rules, 18 inverted probes, 26 rows) | the floor's `plan531-relay-requester-lane` and `plan531-relay-requester-boundary` steps |
| 9 | the routine floor is green on the closing tree | the floor table below | the floor run |
| 10 | `specs/support.toml` records the surface as `experimental` and `advertised = false`; the advertised count stays **zero**; `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` stay byte-identical; no boundary script is weakened | `specs/support.toml`, surface `daemon.relay-requester`; `crates/i2pr-netdb/src/local.rs` (`CONTROLLED_ROUTER_VERSION`); neither manifest is touched; the diff touches **no** script under `scripts/` | `python3 scripts/check-tooling-inventory.py`; the counts are 124 surfaces, 119 experimental, 4 deferred, 1 ready, **0 advertised**; `git status` on both manifests is empty |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS — no warnings |
| `cargo test --locked -p i2pr-transport-ssu2 --all-targets` | PASS |
| `cargo test --locked -p i2pr-runtime --all-targets` | PASS |
| `cargo test --locked -p i2pr-daemon --lib relay_requester -- --test-threads=1` | PASS — 1 row |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_requester -- --test-threads=1` | PASS — 8 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_requester_boundary -- --test-threads=1` | PASS — 26 rows |
| `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | PASS — 11/11 negative cases each failed on their own gate |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests, OK |

## The routine floor

The floor ran **twice**. The first run, on the documentation tree `fe4267de`,
**failed** on two steps, and both failures were real defects in this plan's own
work. They are fixed in `c4ec5923`, and the second run, on that tree, is
green. Every step of both runs is recorded — the second in `/tmp/531-floor.log`
and the first in `/tmp/531-floor-first.log` — with each step's own log under
`/tmp/531f-<step>.log` and `/tmp/531-first/531f-<step>.log`. The floor is a
plain sequence of `step` runs that records each step's exit status; no step is
skipped, no `|| true`, no `continue-on-error` and no filename filtering. The
workspace step runs with `--no-fail-fast`, so every test binary's result is
reported rather than the run stopping at the first failure.

Plan 531's floor is Plan 530's floor plus two steps — the daemon lane and the
daemon guard for the relay requester. **No other step changed, and no other
plan's guard file was touched.**

**The first run's two failures, and what they were.**

| Step | The failure | The cause | The fix |
| --- | --- | --- | --- |
| `test-workspace` | `relay_requester::tests::a_disabled_owner_refuses_every_window` **FAILED** — `runtime: InvalidIdentity` | the in-crate unit test built the runtime with an **empty** RouterInfo, which the runtime refuses: it verifies the record's signature, checks that the identity hash matches the one it was handed, and requires an SSU2 address bound to the static and the introduction keys | the test now signs a real record bound to the router's own static and introduction keys, exactly as the lane does (`c4ec5923`) |
| `m12-floodfill-boundaries` | `corroboration recording must stay inside the runtime service owner` | the **guard's own source** carried the tracker's call literal inside a string needle, so the m12 boundary checker reported the *guard* as a violation | the needle is **split**, so the guard's source no longer carries the literal, and the rule the guard enforces is unchanged (`c4ec5923`) |

Neither failure was worked around, and **the checker was not touched**.
Widening the checker's `--glob '!*/tests/*'` exclusion would have *weakened*
the check, so it stands; the observation it produced is recorded as a finding
below. The first run's totals were **99 PASS and 2 FAIL**; the second run's
are below.

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


**FLOOR PASS 101 — FLOOR FAIL 0.**

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be
reported. Every probe mutates an **in-memory copy** of the source, so **no
file on disk is written**. Every rule **fails closed** when the item it
names is absent — the guard reports the missing item rather than passing.

### Plan 531's guard — `crates/i2pr-daemon/tests/ssu2_relay_requester_boundary.rs`

Eight rules, eighteen inverted probes. All 26 rows green.

| Rule | What it forbids | Its inverted probes |
| --- | --- | --- |
| 1 | an owner that does not use the runtime's seam, or a daemon-side copy of the transport's requester table | `the_guard_reports_an_owner_that_does_not_use_the_runtime_seam`, `the_guard_reports_a_daemon_side_requester_table` |
| 2 | an owner that owns a socket, spawns a task, or adds infrastructure of its own | `the_guard_reports_an_owner_that_owns_a_socket`, `the_guard_reports_an_owner_that_spawns_a_task` |
| 3 | a read that leaves the validated store, or a request that is not signed over the specification's preimage | `the_guard_reports_a_read_that_leaves_the_validated_store`, `the_guard_reports_a_request_that_is_not_signed_over_the_preimage` |
| 4 | an opt-in that defaults on, runs without the service, or is never mapped into the runtime | `the_guard_reports_an_opt_in_that_defaults_on`, `the_guard_reports_an_opt_in_that_runs_without_the_service`, `the_guard_reports_an_unmapped_opt_in` |
| 5 | a changed transport refusal order, or a moved corroboration class | `the_guard_reports_a_changed_transport_refusal_order`, `the_guard_reports_a_moved_corroboration_class` |
| 6 | a signal that supports reachability, a daemon that writes a reachability signal, or a completed exchange that records nothing | `the_guard_reports_a_signal_that_supports_reachability`, `the_guard_reports_a_daemon_that_writes_a_reachability_signal`, `the_guard_reports_a_completed_exchange_that_records_nothing` |
| 7 | a second configuration key beside the one opt-in | `the_guard_reports_a_second_configuration_key` |
| 8 | an owner that is never driven, or never constructed | `the_guard_reports_an_owner_that_is_never_driven`, `the_guard_reports_an_owner_that_is_never_constructed` |

Plus `the_guard_reports_a_drain_that_drops_the_hole_punch`, which proves that
the inbound handoff carries the hole punch and not only the response.

### The negative control against the **real** tree

The guard's probes are in-memory. The tree was therefore also mutated **on
disk** — one mutation at a time, each restored byte-identically — and the guard
and the lane were re-run.

| Mutation | The guard | The lane |
| --- | --- | --- |
| NC-1 — the daemon's owner writes a reachability signal of its own | **FAILED**, as it must (`the_relay_firewalled_fact_is_a_contradiction`) | green — the lane is a behaviour lane and does not read the owner's source |
| NC-2 — the transport's relay-firewalled signal is made to **support** reachability | **FAILED**, as it must (the same rule) | **FAILED**, as it must (`the_router_reaches_a_peer_through_its_introducer`) |
| NC-3 — the daemon re-implements the transport's requester table | **FAILED**, as it must (`the_owner_is_the_daemons_and_the_mechanics_are_the_runtimes`) | green — the lane is a behaviour lane |
| NC-4 — the runtime's hole-punch route is disabled | green — the route is the runtime's, and the guard pins the *daemon's* shape | **FAILED**, as it must (`the_router_reaches_a_peer_through_its_introducer`) |

The split is honest and is recorded rather than hidden. The **lane** is the
authority for the **behaviour**: the exchange completes, the response is
correlated, the hole punch is routed and the recorded signal contradicts.
The **guard** is the authority for the **shape**: no second requester table
in the daemon, no transport model change, no daemon-side reachability write,
exactly one configuration key, and the owner driven on the existing tick. The
two green rows above are the boundary of that division, and they are the
reason both surfaces exist: NC-1 and NC-3 are daemon-source defects that a
behaviour lane cannot see, and NC-4 is a runtime defect that a source guard
does not read.

Every mutation was restored **byte-identically** — the restore is verified by
a SHA-256 comparison of the file's bytes — and the working tree was verified
to be exactly as it was before each mutation.


The mutated file was restored byte-identically after every mutation, and the
restore is verified by a SHA-256 comparison of the file's bytes; the working
tree was verified clean after every restore.

## Acceptance criteria

The plan's §14 lists ten criteria and four stop conditions.

1. the opt-in is **off** by default and refused without the SSU2 service —
   **met**;
2. the owner selects a target that publishes an introducer set, from
   **validated** material, and resolves the introducer by hash — **met**;
3. the request carries the published tag and the target's own hash — **met**;
4. the exchange **completes** over real loopback UDP between three real
   routers, and the response is correlated and the hole punch routed — **met**;
5. a completed exchange records the relay-firewalled signal and never a
   corroborating class — **met**;
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

**No stop condition fired.** The requester did **not** need a change to the
transport's relay state machines; the target selection did **not** need a
**network** lookup that the authoritative store cannot serve; the delivery did
**not** need a new task, a new timer, a new listener or a new socket; and the
plan did **not** turn out to need the production peer-test driver or a public
dial scope.

## Invariant review

The plan's §4 lists twelve invariants. Each is reviewed against the closing
tree.

1. **The requester is opt-in and off by default.** `ssu2.relay_requester`
   defaults to `false` (`crates/i2pr-daemon/src/config.rs:648`), the owner is
   constructed only when the operator opted in
   (`crates/i2pr-daemon/src/lib.rs:4808`), and the runtime's hole-punch route
   is gated on the same flag
   (`crates/i2pr-runtime/src/ssu2_runtime.rs:7635`). A router that has not
   asked for it sends no relay request and drains nothing.
2. **The owner holds no task, no timer, no listener and no socket.** The guard
   rule 2 forbids `tokio::`, `UdpSocket`, `TcpListener`, `TcpStream`, `spawn(`,
   `sleep(`, `interval(` and `std::thread` in the owner's code. The owner's
   work is a single synchronous step on the daemon's **existing** relay tick
   (`crates/i2pr-daemon/src/lib.rs:5453`).
3. **Every table the owner holds is bounded and evicts deterministically.** The
   candidate pool is capped at `MAX_RELAY_REQUEST_CANDIDATES` (16), the live
   request table at `MAX_LIVE_RELAY_REQUESTS` (8), the nonce attempts at
   `MAX_NONCE_ATTEMPTS` (8), and the runtime's requester handoff at the same
   ceiling as the relay handoff (16). The runtime's routing table expires on
   the transport's own `RELAY_REQUEST_TIMEOUT_MS` deadline.
4. **A relay request is started only from validated material.** The reads go
   through `store.get(` and the transport's own strict
   `Ssu2RouterAddress::parse` (`crates/i2pr-daemon/src/relay_requester.rs:233`,
   `:273`); the request is signed over the specification's `relay_request_preimage`;
   and the target's signing key comes from the validated store
   (`crates/i2pr-daemon/src/ssu2_relay_drain.rs` resolves it through
   `NetDbSignerResolver`). The guard rule 3 pins all three.
5. **The tag presented is the one the published introducer set carries.** The
   tag and the target's hash are read from **one** introducer entry, so a
   request never names a target the tag does not identify. The lane proves the
   refusal: `a_request_is_refused_when_the_tag_does_not_identify_the_target`.
6. **A relay request is sent only over an authenticated session.** The request
   rides a session with the **introducer** — the relay blocks are session
   blocks — and the owner refuses with `NoSession` when the router holds none.
7. **Every refusal is typed and counted by reason, and no refusal writes into a
   peer's profile.** Fifteen reasons, one counter each
   (`crates/i2pr-daemon/src/relay_requester.rs:304`, `:348`). The counters are
   counts only: they name no peer, no address, no key and no tag. The lane
   proves it: `every_refusal_is_typed_and_counted_and_writes_nothing`.
8. **A completed exchange feeds the relay-firewalled signal and never a
   corroborating class.** The owner calls the **runtime's**
   `note_relay_firewalled`; the transport's signal stays class 4 and
   contradicting (`crates/i2pr-transport/src/reachability.rs:229`, `:197`); and
   the lane asserts the router's corroboration count is **unchanged** by the
   exchange.
9. **One local condition is never written into a peer's profile.** No refusal
   path touches the store: the reads are pure (`read_published_introducers`,
   `read_introducer_endpoint` take `&RouterInfoStore`), and the lane asserts
   `the_reads_never_mutate_the_store`.
10. **The transport's reachability model is untouched.** Its signal kinds, its
    corroboration classes, the corroboration floor of two and the contradiction
    floor of one are unchanged; `Reachable` still needs three distinct
    classes. The only transport change is a `pub use` re-export.
11. **No configuration key is added beside the one opt-in, and no default
    moves.** The guard rule 7 counts the requester's keys in the SSU2
    configuration surface and requires **exactly one**, and it forbids a
    free-form `HashMap` or `#[serde(flatten)]` escape.
12. **Nothing is promoted.** `router.version` stays `0.9.62`, the inventory's
    advertised count stays **zero**, and the new surface is `experimental` with
    `advertised = false`.

## Failure, cancellation, restart, and contention review

The plan's §9.

- **Failure.** Every refusal is typed and counted by reason and leaves no
  state behind: no target, no introducer, no tag, no session, no request. A
  request that cannot be started records nothing.
- **Cancellation.** The plan adds **no task**. The owner's work is a single
  synchronous step on the daemon's existing relay tick, and the runtime's
  `cancel_relay_request` seam releases the transport's own entry.
- **Restart.** Nothing is persisted. The owner's tables are live state, rebuilt
  from the store; a restart with no target finds nothing and asks nobody.
- **Contention.** The owner takes the daemon's own locks, in the order the
  daemon's other owners use, and introduces **no new lock** beyond the
  runtime's existing state lock. The runtime's routing table is written
  exactly where the transport's own table is written, so the two cannot
  disagree about a live nonce.

## Migration and compatibility review

The plan's §10.

- **No wire change.** The relay blocks, their types, their lengths and their
  signatures are exactly what the tree already sends and receives. The
  transport's relay state machines are used, not edited.
- **No configuration migration.** One boolean is added, off by default, and
  refused at parse time without the SSU2 service.
- **A new reading for the router.** A router that has asked for the requester
  may send relay requests to an introducer. It publishes nothing new and
  advertises nothing new.
- **No change to the transport's model.** The relay-firewalled signal already
  exists and already contradicts; the plan only gives it a producer.

## Security review

- **The request is signed.** The requester signs the specification's
  `relay_request_preimage` with the router's own identity key, and the
  introducer verifies it with the transport's own verification. A request whose
  signature does not verify is refused.
- **The response is verified before it is believed.** The target's signing key
  is resolved from the **validated** store
  (`NetDbSignerResolver` over the bootstrap's `RouterInfoStore`) and passed
  explicitly to the runtime's `*_with_signer` seams; a target the router does
  not hold yields `TargetUnknown` and the event is refused rather than
  guessed. The runtime's own seam verifies the signature and the timestamp's
  skew.
- **The hole punch is sealed.** It arrives out of session, sealed under the
  **receiver's** own introduction key, and the runtime parses it with the
  transport's own `parse_hole_punch` against the local intro key. A datagram
  that does not parse falls through to the cheap-drop.
- **Admission is checked before parsing.** The hole-punch route applies the
  peer-relay service's own `check_admission` before it parses anything.
- **The nonce is fresh and nonzero.** The transport refuses a zero nonce and a
  zero tag, and it refuses a duplicate correlation; the owner draws a fresh
  nonce per request and never reuses one while the request is live.
- **The counters are redacted by construction.** They carry counts and stable
  labels only — no peer hash, no address, no key, no tag, no version string.
- **The daemon writes no reachability signal.** The guard forbids the string
  `ReachabilitySignal::` in the owner's code, so the transport's own recorder
  stays the single writer of the router's reachability.
- **No secret is logged.** The owner's refusal paths log a transition label and
  counts; no key, tag, nonce or peer hash is written to the log.
- **The opt-in fails closed.** A misconfigured router is refused at parse time
  rather than running silently inert.

## Limitations

- The **target** side's response emission has **no product wiring** in this
  tree. The transport says the *caller* emits the accept and the hole punch,
  and no daemon role does that. The lane completes that half over the
  transport's own preimage, with the target's own key. A successor must wire
  the target role.
- The delivery is confined to the **controlled loopback profile**: the
  router's SSU2 dial target is loopback-only and the private-LAN variant
  accepts only RFC1918 IPv4, so the requester can only reach an introducer on
  a local or private network. That bound is Plan 431's.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology, and no stock router implements a router-level relay requester.
- The relay-firewalled signal **contradicts** reachability, so a router that
  uses the requester will find its own state fall to `Firewalled` or
  `Unreachable`. That is the transport's own model and is correct: the router
  is reachable **through the relay**, not directly.
- The candidate pool for the target selection is the **NetDB**, not the live
  sessions, because a peer that cannot be reached directly holds no session.
  The scan is O(store size) per window and the store is bounded.
- The **production** SSU2 peer-test driver stays unbuilt, and the
  **requester** side of the *relay* protocol's peer-test analogue stays out of
  scope, with their owners named.
- The daemon's requester owner is driven by the daemon's **existing relay
  tick** rather than by `ServiceProduct`'s product tick. The plan's §6.5 said
  "the product's existing tick"; §17.2 of the plan of record records the
  choice additively.

## Findings by severity

### Medium — none open

### Low — recorded, not fixed

1. **The target side's response emission has no product wiring.** The
   transport's target role says the caller emits the accept and the hole
   punch, and no daemon role does that, so a real target cannot yet answer a
   real request from this tree. **Owner:** the successor that wires the target
   role. **Not this plan's scope** — the plan's §6 scopes the production
   changes to the requester's side; the limit was found during the
   implementation and is recorded additively in §17.2 of the plan of record.
2. **The production SSU2 peer-test driver is unbuilt.** Its choreography
   question is named: the tree's Alice role needs the helper's identity up
   front, while the specification has Bob select the helper. **Owner:** the
   successor that builds the production driver. **Not this plan's scope.**
3. **Plan 527's Low finding 1 is discharged, and the corrections to Plans
   527, 528, 529 and 530 are recorded in this record and in the registry row.
   None of those closed records is rewritten.**
4. **The runtime's direct form is still loopback-only.** The publication
   profile that would carry it outside the controlled loopback profile is a
   separate decision. **Owner:** the plan that widens the publication profile.
5. **The delivery is confined to the controlled loopback profile.** The
   evidence is a local lane. **Owner:** the plan that exercises the model
   against a stock peer — Plan 493's authorized two-host topology.
6. **Plan 530's Low finding 1 — the explicit-bind class lapses with the
   30-minute observation TTL and no timer re-records it — remains open.** This
   plan did not touch it. **Owner:** the successor that earns the direct form
   in the field.

### Informational

1. **The floor caught two real defects in this plan's own work, and both are
   fixed.** The in-crate unit test built the runtime with an empty RouterInfo
   that the runtime refuses, and the guard's own needle literal read as a
   tracker call site to the m12 boundary checker. The first is in the
   workspace step and the second in the boundary checker; both are recorded
   in the floor table above.
2. **`scripts/check-m12-floodfill-boundaries.sh`'s `--glob '!*/tests/*'`
   exclusion is ineffective.** A single `*` does not cross a `/` in ripgrep's
   glob semantics, so the exclusion does not exclude `crates/*/tests/*`. The
   rule passes today only because no other file outside the runtime's two
   owners carries the literal — and Plan 531's guard did, until the floor
   caught it. **The script was not touched**: widening the exclusion would
   *weaken* the check, and the rule's intent — to forbid production
   corroboration recording outside the runtime's owner — is already enforced
   by the scan as it stands. **Owner:** a successor that records the
   observation and decides the checker's own intent; the fix is a plan of
   record, not a quiet edit.
3. **The relay protocol is now complete on the requester's side only.** A
   reader who took "the requester side is wired" to mean "the relay protocol
   is complete in this tree" would be wrong; the target's response emission
   is still unwired, and the limitation is recorded above and in §17.4 of the
   plan of record.
4. **The plan's own production change moved lines in three files**, so the
   closed records' citations were corrected by an **additive** pointer table
   in §17.3 of the plan of record; §3 itself is evidence and was not
   rewritten, and no closed record was rewritten.
5. **`crates/i2pr-daemon/tests/netdb_integration.rs` gained one line** —
   `relay_requester: false` — a mechanical struct-literal fix forced by the
   new runtime-config field. No behaviour in that file changed.
6. **The requester needed its own bounded handoff**, not the shared relay
   one, because a hole punch names neither a session nor a peer hash. The
   existing `drain_relay_events` is byte-identical, so no pinned artefact
   was rewritten.
7. **The lane and the guard divide the work** — the lane proves that the
   exchange **completes** and that the recorded signal **contradicts**; the
   guard proves the **shape**: no second requester table in the daemon, no
   transport model change, no daemon-side reachability write, one
   configuration key.

## Roadmap disposition

The **network-participation** roadmap gains this row:

| Plan | Subject | Status | Outcome |
| --- | --- | --- | --- |
| 531 | the router must reach a peer through its introducer | closed | passed-the-router-must-reach-a-peer-through-its-introducer |

The roadmap's SSU2 relay composition now reads, in order: Plan 490 gave the
router the introducer role, Plan 499 the live relay-tag issuance, Plan 524 the
router's own tag request, Plan 525 the **published** introducer set, Plan 526
the **advertised** introducer role — and Plan 531 the **requester** half, so
that the router can reach a peer that publishes an introducer set, and so that
the transport's contradicting relay-firewalled signal finally has a producer.
The composition's own row in the roadmap is unchanged, because the stock-peer
half of every one of those plans remains blocked on Plan 493's authorized
two-host topology.

## Unblock audit

| Item | State |
| --- | --- |
| Plan 531 | **closed** — all ten acceptance criteria met, the floor green on the closing tree |
| Plan 527's Low finding 1 (the requester side has no production caller) | **discharged** — the producer exists and the requester side is wired |
| Plan 528's, Plan 529's and Plan 530's repetitions of that finding | **discharged** — the corrections live in this record and in the registry row, never in the closed records |
| The relay-firewalled signal's producer | **present** — a completed exchange feeds it through the runtime's own recorder |
| The **target** side's response emission | **unwired** — the transport says the caller emits the accept and the hole punch, and no daemon role does |
| The production SSU2 peer-test driver | **unbuilt** — the model, the recorder and the verdict path exist; the driver is the successor's subject |
| The `B` advertisement and `caps=f` | **out of scope**, with their owners named |
| The public dial scope | **out of scope** — Plan 431 owns it |
| The stock-peer half | **blocked** on Plan 493's authorized two-host topology |
| Plan 530's Low finding 1 (the explicit bind lapses with its TTL) | **open** — a follow-up, named in the findings |
| Plans 433, 435-439, 493, 494 | **blocked** on external environments; untouched by this plan |

No blocked work was done under this plan's number.

## Handoff notes

- The plan's subject is the **requester** half of the SSU2 relay protocol: the
  router must be able to reach a peer that publishes an introducer set.
- The plan adds **no** capability to the transport. It composes machinery the
  tree already owns and gives the relay-firewalled signal the producer Plan
  528 named as missing.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **target** side's response emission, the **production peer-test driver**,
  the **public dial scope**, the **`B` advertisement**, the **`caps=f`
  advertisement** and the **stock-peer half** all stay out of scope, with
  their owners named.
- The next free plan number is **532**.
