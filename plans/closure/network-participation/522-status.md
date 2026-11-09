# Plan 522 — the router holds a bounded pool of outbound peer sessions

**Closure record.** The plan of record is
`plans/implementation/network-participation/522-the-router-holds-a-bounded-peer-session-pool.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 522 is **closed**. The router now holds a **bounded pool** of outbound peer
sessions to the peers it already knows, and the transport holds a pooled
session open with an **outbound-only** keepalive. The publication's ordinary
path is no longer the handshake path.

The pool's population is the router's **own** version-gated floodfill
selection — the same selection the publication reads, under the **default**
policy the publication's owner already holds — so there is no second policy
and no second source of truth. The pool's ceilings **refuse** rather than
evict: one publication cycle's worth of peers (`15`) and two dials per period.
A candidate beyond either bound is refused and counted under its own reason.

**Class:** capability (the router holds sessions between needs).
**Subsystem:** network-participation. **Closure date:** 2026-11-09.

Nothing is promoted: `router.version` stays `0.9.62`, `caps` and the published
RouterInfo's **content** stay unchanged, the inventory's advertised count stays
**zero**, and `Cargo.toml` and `Cargo.lock` stay byte-identical.

## What the plan closed

The router used to dial per need and hold nothing between needs. The
publication's period is `39 * 60` seconds
(`crates/i2pr-daemon/src/router_info_publication.rs:72`) and a session's idle
timeout is `300_000` milliseconds
(`crates/i2pr-daemon/src/config.rs:1485`), so a session established by one
publication was always gone before the next, and the publication's ordinary
path was the handshake path. The transport had **no way** to hold a session
open: no keepalive, no ping, and no periodic session traffic of any kind.

What the plan closed, concretely:

- **The transport's keepalive.**
  `crates/i2pr-transport-ssu2/src/session.rs` gained
  `SessionConfig::keepalive_interval_ms` (`:220`) and `seal_keepalive`
  (`:1603`). The keepalive is a **padding-only** packet — a
  `Block::Padding` of `DATA_KEEPALIVE_PAYLOAD_BYTES` (20) and **no** I2NP
  message — sealed by the transport's own `seal_packet`, so it respects the
  congestion window and the payload budget, and it asks for an **immediate
  ACK**. The shape is the pinned i2pd 2.61.0 reference's own
  (`libi2pd/SSU2Session.cpp:269`, at the pin
  `635b013a612ff47278ef02acf8580a28e10e26c5`).
- **The keepalive's own unanswered deadline.** The deadline is three
  intervals (`DATA_KEEPALIVE_TIMEOUT_INTERVALS`, `:340`) — the
  specification's own timeout code, **14**
  (`libi2pd/SSU2Session.h:169`) — and it is the keepalive's **own** because
  the keepalive refreshes the session's *send* timestamp while the idle
  timeout reads the **maximum** of the send and receive timestamps. The idle
  timeout could therefore never detect a dead peer. One keepalive is
  outstanding at a time, so an unanswered keepalive reaches its deadline
  instead of being refreshed by a second one.
- **The runtime's wiring.**
  `crates/i2pr-runtime/src/ssu2_runtime.rs` gained
  `Ssu2RuntimeDeadlines::keepalive` (`:346`), the `KeepaliveTooLong`
  refusal (`:401`), and the re-exported `KEEPALIVE_INTERVAL_MS` (`:126`).
  `construct_session` (`:4462`) takes the session's **direction** and passes
  the interval to an **outbound** session and to nothing else, so an inbound
  session's lifetime is exactly what it was before this plan.
- **The daemon's pool.**
  `crates/i2pr-daemon/src/peer_pool.rs` gained `PeerPoolOwner` (`:196`), a
  runtime-neutral owner that holds **no** session, **no** socket, **no** task
  and **no** timer, and reads its clock from its caller. Its population is
  the router's own selection; its ceilings are `MAX_POOLED_PEERS` (`:62`,
  one publication cycle's worth of peers) and `MAX_DIALS_PER_PERIOD` (`:76`,
  two); a peer that already holds a session is never dialled again; a peer
  whose session lapsed is planned again and the lapse is counted; and every
  refusal is counted by reason.
- **The pool's own task.**
  `crates/i2pr-daemon/src/peer_pool_task.rs` gained `PeerPoolTask` (`:40`)
  and `run_peer_pool_task` (`:74`), for the same reason the publication has
  one: a session establishment is an **await**, and the daemon's `select!`
  loop must never block on a handshake. The bootstrap's lock is taken to read
  the population and is **released** before any dial, so a slow handshake can
  never hold the router's NetDB.
- **One dial path.** The pool dials through the **one** existing narrow seam
  — `ensure_peer_session_for_record`
  (`crates/i2pr-daemon/src/service_product.rs:7744`) — with its own reason,
  `DestinationDialPurpose::PeerPoolMaintenance` (`:7358`). The publication's
  delivery path is **unchanged**: its existing
  `session_peer_links(peer_id).is_empty()` gate already reuses a session, so
  the pool's only job is to make the session exist.
- **The opt-in.** `ssu2.peer_pool` is an explicit, **default-off** opt-in
  (`crates/i2pr-daemon/src/config.rs:1445`), refused unless the SSU2 service
  itself runs, and refused when the session's idle timeout cannot hold the
  keepalive's unanswered deadline. The runtime's interval is derived from the
  pool's own switch and the transport's own constant
  (`crates/i2pr-daemon/src/router_i2np.rs:1528`), so a router without the pool
  sends no keepalive at all.
- **The runtime's own refusals.** The runtime refuses a zero keepalive, a
  keepalive longer than its own ceiling, and a keepalive whose unanswered
  deadline does not fall **strictly** before the session's idle timeout
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:391-402`). The daemon's
  parse-time refusal is the first line; the runtime's is the second, and it
  is the one a different caller of the public runtime crate would meet. It
  had no test of its own until this plan's floor audit found it, and it now
  has one.
- **The receipt.** The counters reach the inspection plane through
  `PeerPoolCounterCell` (`crates/i2pr-daemon/src/i2pcontrol_inspection.rs:82`).
  The cell holds counts only: it names no peer, no address and no key.

### The registered plan's corrections

The implementation made five corrections to the registered text. They are in
the plan of record's amendment log, section 17; this record is not a rewrite of
the plan.

1. **§17.2 — the immediate-ACK request is honoured on a non-eliciting
   packet.** The registered plan assumed the peer answers a padding-only
   packet that asks for an immediate ACK. `i2pr`'s own receiver did **not**:
   the immediate-ACK flag was consulted only inside the arm that handles an
   ack-eliciting packet, so a padding-only keepalive would never have been
   answered by an `i2pr` peer. The reference honours the flag
   unconditionally (`libi2pd/SSU2Session.cpp:1758`) and so does Java I2P
   2.13.0 (`router/java/src/net/i2p/router/transport/udp/PeerState2.java:628`,
   at the pin `9134f808337b401e8e53c73734c81fab04280c9d`), so the request is
   now honoured. A packet carrying no ack-eliciting block still never asks
   for an ACK of its own, which keeps the ACK exchange loop-free.
2. **§17.3 — the termination reason is the specification's own timeout code,
   14** (`libi2pd/SSU2Session.h:169`), the same code the consecutive-RTO
   ceiling uses; the cause is distinguished by the counters.
3. **§17.4 — `ack_scheduled` now reports what it says.** It reported
   `ack_eliciting`, which after §17.2 is a different question, so it now
   reports whether an ACK was actually **armed**.
4. **§17.6 — the admission counter now counts the pool's own dials.** The
   lane found that `peers_admitted` counted only a peer the pool found
   **already** holding a session, so a pool that dialled fifteen peers and
   established twelve reported **zero** admissions — the counter contradicted
   the pool's own peer list. The admission is now counted wherever the pool
   comes to hold a session with a peer, once per peer.
5. **§17.7 — the §17.5 pointer table moved again.** §17.5's runtime rows and
   the daemon rows it never covered are stale by the lines WP3, WP4 and WP5
   moved. The corrected pointers are recorded additively in §17.7; §3 itself
   is not rewritten.

### §3's citation drift

§3 is evidence and is **not** rewritten. The implementation moved the tree
§3 cited, and the drift is recorded additively in §17.5 and §17.7 with
registration-time to current pointer tables. Twenty pointers moved and seven
stayed exact. The publication's own file, the netdb, the proto crate and the
two pinned references are untouched, so §3's pointers into them are still
exact.

The one **functional** consequence of the drift is worth naming: the
implementation **added** `crates/i2pr-daemon/src/peer_pool.rs` and
`crates/i2pr-daemon/src/peer_pool_task.rs`, two files §3 could not cite
because they did not exist at registration.

## What the plan did not close

- **The stock-peer half.** It stays blocked on **Plan 493's** authorized
  two-host topology. The lane drives the pool against **this router's own
  peer**, over a real SSU2 session on loopback.
- **The dial scope.** The transport's own `Ssu2DialTarget::new` accepts a
  loopback endpoint and `new_private_lan` accepts only an RFC1918 IPv4
  literal, so a **public** deployment's peer population is not dialable at
  all and the pool finds no candidate and refuses. This is a
  **transport-boundary** limit with its own plan-of-record — the same bound
  Plan 505 recorded for the exploratory build driver — and this plan does
  not widen it.
- **The publication's delivery path.** It is **byte-for-byte** unchanged. The
  pool's only job is to make the session the publication's existing gate
  already looks for.
- **The transport's reachability policy.** The corroboration classes, the
  threshold and the expiry are the transport's, and this plan changed none of
  them.
- **`caps=f`.** Plan 494's advertisement stays blocked on Plan 493, and the
  pool touches no builder and no permit.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `94232fb5` | the plan of record, its registry row and its roadmap row |
| `5ece3d60` | the transport's outbound keepalive and the runtime's wiring |
| `22ce037b` | the daemon's bounded pool, its task and the default-off opt-in |
| `5396dab4` | the lane, the guard, the documentation, the support inventory, the admission-counter correction and the plan's amendment log |
| `94488d18` | the runtime's own keepalive-deadline refusals, proven by a test of their own |

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the keepalive fires at the configured interval and not before it | `crates/i2pr-transport-ssu2/src/session.rs`, the keepalive unit tests |
| 2 | the keepalive carries a padding block and **no** I2NP message | `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_keepalive_that_carries_an_i2np_message` |
| 3 | the keepalive requests an immediate ACK | `crates/i2pr-transport-ssu2/src/session.rs`, `seal_keepalive` and the immediate-ACK unit tests |
| 4 | an inbound answer clears the keepalive's deadline | `crates/i2pr-transport-ssu2/src/session.rs`, the keepalive unit tests |
| 5 | an unanswered keepalive terminates the session under its own reason | `crates/i2pr-transport-ssu2/src/session.rs`, the keepalive timeout unit test |
| 6 | a disabled keepalive sends nothing at all | `crates/i2pr-transport-ssu2/src/session.rs`, the disabled-keepalive unit test |
| 7 | the idle timeout still fires on a session with no keepalive | `crates/i2pr-transport-ssu2/src/session.rs`, the idle-timeout unit test |
| 8 | the keepalive does not bypass the congestion gate or the payload budget | `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_keepalive_that_bypasses_the_congestion_gate` |
| 9 | the pool is off by default and dials nothing | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_opt_in_is_off_by_default` |
| 10 | the pool admits at most its ceiling of peers and **refuses** the rest | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_pool_refuses_a_candidate_beyond_its_ceiling` and `the_pool_never_exceeds_its_ceiling` |
| 11 | a peer that already holds a session is not dialled again | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_pool_reuses_a_session_it_already_holds` |
| 12 | a peer whose session lapsed is re-dialled, and the re-dial is counted | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_pool_replans_a_peer_whose_session_lapsed` |
| 13 | a candidate with no dialable SSU2 address is refused and counted | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_pool_counts_a_peer_with_no_dialable_address` |
| 14 | a candidate the version gate refuses is never admitted | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_pool_refuses_a_candidate_the_version_gate_refuses` |
| 15 | a dial that fails is counted under its own reason and **writes no peer profile** | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_pool_counts_every_dial_outcome_under_its_own_reason` and `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_pool_that_writes_a_peer_profile` |
| 16 | the pool never exceeds its per-tick dial budget | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_pool_refuses_a_candidate_beyond_its_ceiling` and `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_pool_with_no_dial_budget` |
| 17 | the pool's dial budget is bounded by its cadence | `crates/i2pr-daemon/src/peer_pool.rs`, `POOL_PERIOD_MS` and `MAX_DIALS_PER_PERIOD` |
| 18 | the publication's delivery path is **unchanged** by the pool's existence | `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_publication_that_names_the_pool` |
| 19 | with the pool on and a session present, the publication performs **no** handshake | `crates/i2pr-daemon/src/router_info_publication.rs:1329` — the gate is unchanged and `session_peer_links` is read first |
| 20 | with the pool on and **no** session, the publication dials exactly as it did before | `crates/i2pr-daemon/src/service_product.rs:7744` — the one seam, unchanged |
| 21 | the pool **establishes a real session** to a peer it knows | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_pool_establishes_a_session_to_a_peer_it_knows` |
| 22 | a router with the opt-in off establishes **no** session | `crates/i2pr-daemon/tests/peer_pool.rs`, `a_router_with_the_opt_in_off_establishes_no_session` |
| 23 | the keepalive is **outbound-only** | `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_keepalive_on_an_inbound_session` |
| 24 | the keepalive is **off** unless the pool is on | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_keepalive_reaches_the_runtime_only_when_the_pool_is_on` and `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_keepalive_that_is_on_without_the_pool` |
| 25 | the opt-in is refused without the SSU2 service | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_opt_in_requires_the_ssu2_service` |
| 26 | the opt-in is refused when the idle timeout is too short | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_opt_in_is_refused_when_the_idle_timeout_cannot_hold_the_keepalive` |
| 27 | the pool's owner adds **no** task, no timer, no listener and no socket | `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_pool_that_spawns_a_task` and `the_guard_reports_a_pool_that_binds_a_socket` |
| 28 | the pool dials through the **one** seam and through no other | `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_pool_that_dials_the_transport_directly` |
| 29 | the pool's task is spawned **only** under the opt-in, and only once | `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_pool_task_that_runs_without_the_opt_in` |
| 30 | a rule whose named function is absent **fails closed** | `crates/i2pr-daemon/tests/peer_pool_boundary.rs`, `the_guard_reports_a_missing_named_item` |
| 31 | the counters reach the inspection plane | `crates/i2pr-daemon/tests/peer_pool.rs`, `the_counters_reach_the_inspection_plane` |
| 32 | a cancelled dial releases the peer | `crates/i2pr-daemon/tests/peer_pool.rs`, `a_cancelled_dial_releases_the_peer` |
| 33 | the pool's own unit behaviour | `cargo test --locked -p i2pr-daemon --lib peer_pool` — 13 tests |
| 34 | the transport's own keepalive behaviour | `cargo test --locked -p i2pr-transport-ssu2 --all-targets` — 155 lib tests |
| 35 | the runtime's own wiring | `cargo test --locked -p i2pr-runtime --all-targets` |
| 36 | the runtime's own keepalive-deadline refusals | `crates/i2pr-runtime/src/ssu2_runtime.rs`, `the_keepalive_deadline_is_refused_when_it_cannot_fall_before_the_idle_timeout` |

## Focused verification

```text
cargo test --locked -p i2pr-daemon --test peer_pool -- --test-threads=1
    17 passed; 0 failed
cargo test --locked -p i2pr-daemon --test peer_pool_boundary -- --test-threads=1
    16 passed; 0 failed
cargo test --locked -p i2pr-daemon --lib peer_pool -- --test-threads=1
    13 passed; 0 failed
cargo test --locked -p i2pr-transport-ssu2 --all-targets
    155 passed; 0 failed
cargo test --locked -p i2pr-runtime --all-targets
    133 lib tests passed; 0 failed
cargo check --locked --workspace --all-targets
    clean
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    clean
```

## The routine floor

**79 steps, 79 PASS, 0 FAIL.** The reconciliation is
exact: the script declares its steps and the log carries one result for each,
the step names match one for one, and no step failed.

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
| `plan522-keepalive-transport-unit` | PASS |
| `plan522-keepalive-runtime-unit` | PASS |
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

The floor ran on the committed tree at `94488d18` with a **fresh** script
(`/tmp/522-floor-final.sh`) that was never edited while it ran: bash reads a script
incrementally, so rewriting one under a running instance makes that instance resume
at a byte offset in the new text and silently skip whole blocks of steps. The log is
`/tmp/522-floor-final.log`.

The script's first step asserts the working tree is **clean**, so the floor is
evidence about a committed tree rather than about a dirty one. The managed-app
sibling binaries are built **before** the workspace test, because Plans 369 and
383's black-box qualification execs **real** sibling executables and its
`assert_fresh` staleness guard fails closed on a leftover binary.

The five Plan 522 steps are `plan522-peer-pool-lane` (the lane),
`plan522-peer-pool-boundary` (the guard), `plan522-peer-pool-unit` (the pool's own
unit behaviour), `plan522-keepalive-transport-unit` (the transport's own keepalive
behaviour) and `plan522-keepalive-runtime-unit` (the runtime's own wiring).

## The floor's first run, and the fix

The routine floor was first run on the implementation commit `5396dab4`, and it
reported **73 steps, 73 PASS, 0 FAIL**. The audit that followed it read the
runtime's own deadline validation and found a **coverage gap**, not a
failure: the runtime refuses a zero keepalive, a keepalive longer than its own
ceiling, and a keepalive whose unanswered deadline does not fall strictly
before the session's idle timeout
(`crates/i2pr-runtime/src/ssu2_runtime.rs:391-402`), and none of the three had
a test of their own. The daemon's parse-time refusal is the first line; the
runtime's is the second, and it is the one a different caller of the public
runtime crate would meet.

The gap was **fixed**, not recorded and tolerated: commit `94488d18` adds
`the_keepalive_deadline_is_refused_when_it_cannot_fall_before_the_idle_timeout`,
and the routine floor was re-run on the tree that carries it. The floor's
result above is that second run.

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be reported.
A rule with no probe is a rule nobody has shown to fire. The guard carries
**13 rules** and **15 inverted probes**, and the control proves that each
mutation is reported:

| # | Rule | The inverted probe |
| --- | --- | --- |
| 1 | the opt-in defaults off and needs the service | an opt-in that defaults on |
| 2 | the owner adds no infrastructure | an owner that spawns a task |
| 3 | the pool dials through the one seam | a pool that dials the transport directly |
| 4 | the pool writes no peer profile | a pool that writes a peer profile |
| 5 | the ceiling refuses rather than evicts | a pool that evicts a peer |
| 6 | the dial budget is bounded | a pool with no dial budget |
| 7 | the keepalive carries no I2NP message | a keepalive that carries an I2NP message |
| 8 | one outstanding keepalive at a time | a session that keeps two keepalives |
| 9 | the keepalive respects the transport policy | a keepalive that bypasses the congestion gate |
| 10 | the keepalive is outbound-only | a keepalive on an inbound session |
| 11 | the keepalive is off without the pool | a keepalive that is on without the pool |
| 12 | the publication is untouched | a publication that names the pool |
| 13 | the pool's task runs only under the opt-in | a pool task that runs without the opt-in |

The two further probes are the **unit** proofs of the guard's own machinery: a
pool that binds a socket (rule 2's second shape) and a rule whose named
function is **absent** — the last is the guard's own fail-closed property,
which the inherited scanner got wrong: `item_body(…) ?` returned "no
complaint" for an absent item, so a deleted function would have satisfied
every rule that named it.

## Invariant review

The plan's invariants, each against what the code does:

1. **One policy, one gate.** The pool's population is the router's **own**
   version-gated floodfill selection under the **default** policy the
   publication's owner already holds. The pool never names a peer the tree's
   version floor would refuse. *Proved by* the lane's
   `the_pool_refuses_a_candidate_the_version_gate_refuses`.
2. **The ceilings refuse rather than evict.** A candidate beyond the pool's
   ceiling or its per-period dial budget is refused and counted under its own
   reason. A router under flood cannot grow the pool past its ceiling, and a
   peer the pool already holds is never evicted to make room. *Proved by* the
   lane's `the_pool_refuses_a_candidate_beyond_its_ceiling` and
   `the_pool_never_exceeds_its_ceiling`, and by the guard's
   `the_guard_reports_a_pool_that_evicts_a_peer`.
3. **One dial path.** The pool dials through the **one** existing narrow
   seam and names no other dial entry point. *Proved by* the guard's
   `the_guard_reports_a_pool_that_dials_the_transport_directly`.
4. **No peer profile is written.** A dial outcome is this router's condition
   as often as the peer's, so the pool counts its outcomes in its own counters
   and nowhere else. *Proved by* the guard's
   `the_guard_reports_a_pool_that_writes_a_peer_profile`.
5. **The publication is unchanged.** The period, the fan-out, the ladder's
   timing, the correlator, the freshness rule, the delivery's gate and the
   task's ownership are all untouched. *Proved by* the guard's
   `the_guard_reports_a_publication_that_names_the_pool`.
6. **The owner adds no infrastructure.** The pool's owner holds no session,
   no socket, no task and no timer, and the dial is awaited on the pool's
   **own** task so the daemon's `select!` loop never blocks on a handshake.
   *Proved by* the guard's `the_guard_reports_a_pool_that_spawns_a_task` and
   `the_guard_reports_a_pool_that_binds_a_socket`.
7. **The keepalive is outbound-only and off by default.** The runtime passes
   the interval to an outbound session and to nothing else, and the daemon
   derives it from the pool's own switch. An inbound session's lifetime is
   exactly what it was. *Proved by* the guard's
   `the_guard_reports_a_keepalive_on_an_inbound_session` and
   `the_guard_reports_a_keepalive_that_is_on_without_the_pool`, and by the
   lane's `the_keepalive_reaches_the_runtime_only_when_the_pool_is_on`.
8. **Nothing is advertised.** `router.version` stays `0.9.62`, the published
   RouterInfo's `caps` string is unchanged, the support inventory's advertised
   count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay byte-identical.
   *Proved by* the inventory's own count and by the diff.
9. **Every rule fails closed.** A rule whose named function is absent reports
   the absence as a failure. *Proved by* the guard's
   `the_guard_reports_a_missing_named_item`.

## Failure, cancellation, restart, and contention review

- **A refused dial** (the qualification profile refuses the target) is
  counted under its own reason (`dials_refused`).
- **A failed dial** is counted under its own reason (`dials_failed`), and the
  peer is **released**, so a failed dial cannot wedge the pool. *Proved by*
  the lane's `the_pool_counts_every_dial_outcome_under_its_own_reason`.
- **A cancelled dial** is counted under its own reason (`dials_cancelled`)
  and the peer is released. *Proved by* the lane's
  `a_cancelled_dial_releases_the_peer`.
- **An absent peer** (the selection named a peer the store no longer holds) is
  counted under its own reason (`peers_absent`) and released.
- **A peer that is dialling** is not dialled again (`dial_in_flight`), so a
  second plan in the same period never starts a second dial for it.
- **A peer whose session lapsed** is planned again and the lapse is counted
  (`sessions_lapsed`). *Proved by* the lane's
  `the_pool_replans_a_peer_whose_session_lapsed`.
- **An unanswered keepalive** terminates the session under the
  specification's own timeout code (**14**) and is counted under
  `keepalive_timeouts`, so the cause is distinguishable from the
  consecutive-RTO ceiling.
- **Cancellation** flows from the product's existing cancellation token into
  the dial, and a cancelled pool cannot wedge itself.
- **Restart** re-establishes the pool from scratch: the pool holds no durable
  state, so a restart costs one re-dial of the population and nothing else.
- **Contention** between the pool's dial and a need-driven dial of the same
  peer is resolved by the transport's own per-address pending guard, which
  refuses a second concurrent dial to the same address; the pool counts that
  refusal and does not retry inside the tick.
- **The keepalive never outlives its session.** The emit path is inside the
  session's own `poll`, and the session is dropped with it.

## Migration and compatibility review

- **No wire change.** The keepalive is a padding-only SSU2 data packet with
  the immediate-ACK flag set — the exact shape i2pd 2.61.0 already sends. No
  I2NP message type is added, no SSU2 block type is added, and the proto
  crate is untouched.
- **No dependency change.** `Cargo.toml` and `Cargo.lock` stay byte-identical.
- **No configuration migration.** The new key is additive and defaults to off.
  A configuration that does not mention it parses exactly as before. A
  configuration that asks for the pool without the SSU2 service, or with an
  idle timeout that cannot hold the keepalive's deadline, is **refused** at
  parse time rather than silently inert.
- **The published RouterInfo's `caps` string is unchanged**, and
  `router.version` stays `0.9.62`.
- **The default router's wire behaviour is unchanged**: with the opt-in off
  the router sends nothing it did not already send, holds no session, and
  starts no pool task.
- **The support inventory grew by exactly two surfaces** — 113 → 115 — and
  the advertised count stayed zero.

## Security review

- **No peer profile is written.** The guard proves the pool touches no
  participation handle, no participation driver, no observe call and no
  profile store.
- **The pool's tables are bounded.** The population's ceiling is fifteen
  peers, and the pool's refusal counts saturate. A router under flood cannot
  grow the pool past its ceiling, and it refuses rather than evicts.
- **The dial rate is bounded.** Two dials per period, and the population is
  re-read on the same period, so a population that is entirely unreachable
  cannot turn the pool into a handshake storm.
- **The keepalive's traffic is bounded.** One outstanding keepalive at a time,
  on a bounded population, at the transport's own interval — fifteen small
  packets per minute against fifteen handshakes per five minutes.
- **The keepalive is off by default**, so a router that has not asked for the
  pool generates no keepalive traffic at all.
- **No secrets are logged.** The counters are counts; the cell that carries
  them to the inspection plane names no peer, no address and no key.
- **One local condition is never written into a peer's profile.** The pool's
  outcome counts are its own, and they are the router's own condition, not a
  statement about the peer.

## Limitations

1. **The stock-peer half is not proved and is not claimed.** The lane drives
   the pool against **this router's own peer** over a real SSU2 session on
   loopback. The stock half needs Plan 493's authorized two-host topology.
2. **The pool cannot dial a public peer.** The transport's own dial scope
   accepts a loopback endpoint or an RFC1918 IPv4 literal, so a public
   deployment's population is not dialable and the pool refuses every
   candidate. This is a transport boundary with its own plan-of-record; the
   same bound Plan 505 recorded for the exploratory build driver.
3. **The pool's cadence and budget are this router's own.** Thirty seconds
   between population reads and two dials per period. No reference rate is
   matched, and no claim is made that one is.
4. **The keepalive's interval is this router's own.** Sixty seconds, with a
   three-interval unanswered deadline. The interval is the transport's own
   constant rather than a measured reference value.
5. **The pool does not rotate.** It holds one population, drawn from the
   router's own selection, and it replaces a peer whose session lapsed
   rather than rotating through the population.
6. **The lane's live exchange is a two-router loopback exchange.** It proves
   the pool's selection, its ceilings, its counting and a real session
   establishment. It does not prove anything about a stock peer's behaviour.

## Findings by severity

### Medium — none open

The **outbound peer-session pool** — the one local gap the network-participation
roadmap's **Transports** row carried — is **discharged**.

### Low — recorded, not fixed

- **L1 — the pool cannot dial a public peer.** The transport's own dial scope
  is the limit. A successor that widens the transport's dial scope would
  widen the pool's reach; the pool itself must not.
- **L2 — the pool's cadence and budget are this router's own.** Thirty
  seconds and two dials. A successor that measures the reference's own
  session-maintenance rate would refine them.
- **L3 — the keepalive's interval is the transport's own constant.** Sixty
  seconds. The reference's own keepalive cadence was not measured against the
  specification's text, and no claim is made that the two agree.

### Informational

- **I1 — the registered §3's citations drifted**, and two files were added
  that §3 could not cite. The drift is recorded additively in §17.5 and
  §17.7; §3 itself is not rewritten.
- **I2 — the immediate-ACK request needed a correction** before the keepalive
  could work against an `i2pr` peer at all. The correction is §17.2.
- **I3 — the admission counter contradicted the pool's own peer list** until
  this plan's lane found it. The correction is §17.6.
- **I4 — the guard's inherited scanner failed open** for an absent named
  item. The correction is in the guard itself, and the probe for it is
  `the_guard_reports_a_missing_named_item`.
- **I5 — the runtime's own keepalive-deadline refusals had no test.** The
  daemon's parse-time refusal is the first line and the runtime's is the
  second, and the second was reachable and typed but unproven. The floor
  audit found it and it is now proven by
  `the_keepalive_deadline_is_refused_when_it_cannot_fall_before_the_idle_timeout`.

## Roadmap disposition

The roadmap's `network-participation` sheet now records **Plan 522** as
closed, and the row it carries names the closure. The three consequences that
changed:

- the **authority line** moves from Plan 521 to Plan 522 — the router now
  **holds** sessions between needs instead of dialling per need, so the
  publication's ordinary path is no longer the handshake path;
- the **Transports** row's missing cell no longer says that the outbound
  peer-session pool is missing: the pool **exists** now, behind the
  default-off `ssu2.peer_pool` opt-in, and only the **dial scope** — the
  transport's own loopback/RFC1918 profile — and the **stock-peer** half
  remain;
- the **consequence paragraph** no longer says the router holds no peer
  sessions between needs.

The **RouterInfo**, **I2NP**, **Peers**, **NetDb**, **Roles** and **Clients**
rows are unchanged.

The **stock-peer** half of this plan, Plan 520's stock-peer half, Plan 497's
M2, Plan 487's stock-peer half, and Plans 519, 520 and 521's stock-peer
halves all stay blocked on **Plan 493's** authorized two-host topology.

## Unblock audit

The unblock audit asks, of every blocked item this line carries, whether this
plan changed its status. The answer is **no**, except for the one item this
plan was opened to discharge.

| Blocked item | Before Plan 522 | After Plan 522 |
| --- | --- | --- |
| The **outbound peer-session pool** | missing | **delivered** — a bounded pool behind the default-off `ssu2.peer_pool` opt-in, with the outbound keepalive it needs |
| Plan 522's stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 520's stock-peer half — a stock peer qualifies the published record | blocked on Plan 493 | blocked on Plan 493 |
| Plan 521's stock-peer half | blocked on Plan 493, and on the router-level 231 responder | blocked on Plan 493, and on the router-level 231 responder |
| Plan 497's **M2** / limitation 1 — the stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 487's stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plans 519, 520 and 521's stock-peer halves | blocked on Plan 493 | blocked on Plan 493 |
| Plans 433, 435, 436, 437, 438, 439, 493, 494 | blocked on external environments | blocked on external environments — untouched |
| The **dial scope** — the transport's own loopback/RFC1918 profile | a transport boundary | a transport boundary — untouched, with its own plan-of-record needed |
| The no-**NTCP2**-ingress and no-**OBEP**-consumer gaps | missing | missing — untouched |
| The **`caps=f`** advertisement | Plan 494, blocked on Plan 493 | Plan 494, blocked on Plan 493 |

Nothing was unblocked that this plan did not itself open, and nothing was
promoted. Plan 493's two-host topology is the gate on every remaining
stock-peer half.

## Handoff notes

- The next free plan number after this one is **523**.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`). Neither reference is patched
  or vendored.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology.
- The **dial-scope** limit is a transport boundary and needs its own
  plan-of-record; it is not discharged here.
- The **outbound peer-session pool** is discharged by this plan. The roadmap's
  **Transports** cell is rewritten to say so.
- The pool is **off by default**, so a router that has not asked for it
  changes in no way on the wire.
- The next thing in this line is the **stock-peer** half of the pool, which
  needs Plan 493's authorized two-host topology, and the **dial scope**,
  which needs a transport-boundary plan-of-record.
