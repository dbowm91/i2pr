# Plan 521 — the router runs its own peer test

**Closure record.** The plan of record is
`plans/implementation/network-participation/521-the-router-runs-its-own-peer-test.md`.
This record is the authority on what the plan closed, what it did not, and
on what the evidence is.

## Status

Plan 521 is **closed**. The ordinary router can now run an I2NP peer test
against a peer it already holds a session with, and a **confirmed** exchange
records the router's **own** reachability in the transport's tracker — so the
published SSU2 address Plan 520 made qualified can be **earned in
production**, not only inside the controlled floodfill activation.

That discharges **Plan 520's open Medium finding M1**: the direct form needs
**three** distinct corroboration classes, and the third — a confirmed peer
test — was recorded in production only by the controlled activation, which
installs the *floodfill* record Plan 520's stand-down rule leaves alone.

**Class:** capability (the router can earn its own reachability evidence).
**Subsystem:** network-participation. **Closure date:** 2026-11-09.

Nothing is promoted: `router.version` stays `0.9.62`, `caps` and the
published RouterInfo's **content** stay unchanged, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

Plan 521 gave the ordinary router a **production** entry point that starts a
peer test and records the confirmed outcome about **this** router. The
machinery was already complete and unwired: the I2NP peer test — message
**231** (`crates/i2pr-proto/src/i2np/header.rs:61`) — was modelled by the
daemon and classified by the central dispatcher
(`crates/i2pr-daemon/src/router_i2np.rs:748`), and neither half had a
production caller.

What the plan closed, concretely:

- **The originator.** `crates/i2pr-daemon/src/peer_test.rs` gained
  `PeerTestOwner`, a bounded owner that starts one probe against a peer the
  router already holds a session with, records it in the outstanding table
  **before** the send, and completes it only on a **matched** echo. The
  table is keyed by the probed identifier **and** the probed timestamp, so
  an echo matches the probe that produced it; and a probe completes only
  when the answer came from the peer the probe was sent to. One probe at a
  time per peer; a second attempt is refused, not queued. The table is a
  ceiling that **refuses** rather than evicts.
- **The responder.** The responder moved onto the **live inbound path** and
  reads the dispatcher's **classified** outcome, so the inbound message is
  decoded **once** and the responder runs no decoder of its own. An echo of
  the router's own probe completes the probe and is owed **no** answer;
  anything else is owed an answer that repeats the probed identifier and
  timestamp unchanged, and it travels to the **authenticated** peer the test
  arrived from. The responder records **nothing** about itself.
- **The recorder.** `crates/i2pr-runtime/src/ssu2_runtime.rs` gained
  `note_confirmed_peer_test`, the production counterpart of the controlled
  table's outcome mirror. It is distinct from `map_controlled_outcome_locked`,
  it records only a `Confirmed` outcome, and it is refused after shutdown.
  The caller names the **link** the exchange travelled on and the runtime
  **derives** the address family from that session's own address, so the
  daemon can neither assert a family nor record an unmatched exchange.
- **The sealed proof.** `crates/i2pr-transport/src/reachability.rs` gained
  `ConfirmedPeerTest`, a proof with **no** constructor that takes a boolean
  or a claim. The only way to obtain one is
  `attest_matched_round_trip(elapsed_ms)`, which takes the elapsed local
  time a **matched** echo produced — so the unmatched and timed-out paths
  cannot mint one at all.
- **The opt-in.** `ssu2.peer_test` is an explicit, **default-off** opt-in,
  and it is refused unless the SSU2 service itself runs, because a peer test
  is an exchange with a peer the router already holds a session with. The
  owner is driven by the product's **existing** tick: it starts no task, no
  timer and no listener.
- **The bounded reply.** A second bounded table remembers the keys the
  router has already **answered**, so the reply traffic a peer can provoke
  is bounded by the router's own table and not by the peer's patience. A
  repeated key is **suppressed** rather than answered again; the table
  refuses rather than evicting, and it ages out on the owner's own sweep.

### The registered plan's four corrections

The implementation made four corrections to the registered text. They are in
the plan of record's amendment log, section 17; this record is not a rewrite
of the plan.

1. **§17.1 — the answered-key table.** The registered plan named no bound on
   the reply traffic a peer can provoke. `AGENTS.md` requires that network
   bytes be treated as hostile and bounded, so a second bounded table was
   added (§17.1).
2. **§17.2 — the runtime derives the family.** The registered §6.3 said the
   recorder "takes the typed confirmed outcome". The reachability tracker is
   **family-scoped** — the transport's `supporting_classes` counts the
   distinct classes of the **latest family** only
   (`crates/i2pr-transport/src/reachability.rs:511-528`) — so a daemon that
   named the family could file the evidence under a family the exchange
   never used. The runtime is the only component that knows the session's
   address, so the runtime derives it (§17.2).
3. **§17.3 — one probe at a time per peer.** The registered §9 named the
   contention rule but the registered §6.1 did not say how the table
   enforces it. `PeerTestTracker::has_outstanding_for(peer)` now answers the
   question directly (§17.3).
4. **§17.4 — the stock-peer half is narrower than §3.1 implied.** No stock
   router implements an I2NP 231 responder at the router level (§ below).

### §3's citation drift

§3 is evidence and is **not** rewritten. The implementation moved the tree
§3 cited, and the drift is recorded additively in §17.5 with a
registration-time to current pointer table. Eleven pointers moved, one
function was **retired**, and the rest are unchanged.

The retired function is the one §3.1 itself condemned:
`dispatch_router_i2np_with_peer_test` re-decoded the message the dispatcher
had already decoded, so §3.1 and the registered **D9** both required the
responder to use the classified outcome instead. Retiring it was the only
way §3.1's own requirement could be met.

## What the plan did not close

- **The stock-peer half.** It stays blocked on **Plan 493's** authorized
  two-host topology, and it is now blocked for a **second** reason besides
  the topology: **no stock router implements an I2NP 231 responder at the
  router level**. i2pd uses message 231 only for a **garlic-carried tunnel**
  test (`libi2pd/TunnelPool.cpp:436`, `:493-501` at the pin
  `635b013a612ff47278ef02acf8580a28e10e26c5`), and its
  `TunnelPool::ProcessTunnelTest` is the **initiator** half, matching the
  message id against the tunnel pool's own outstanding tests. Java I2P has
  **no** `TunnelTestMessage` class at all at the pin
  `9134f808337b401e8e53c73734c81fab04280c9d`; its peer test is the
  out-of-session `PeerTestManager`
  (`router/java/src/net/i2p/router/transport/udp/`). The router-level 231
  responder is therefore proven against **this router's own peer**, over a
  real SSU2 session on loopback.
- **The published record's qualification in the live network.** Plan 520's
  own stock-peer half — that a **stock** peer qualifies the router's
  published record — is untouched and still blocked on Plan 493.
- **The outbound peer-session pool.** The roadmap names it as missing. It
  is a separate, larger piece of work; this plan needed only *a* peer with
  a session, not a pool.
- **The transport's corroboration policy.** The threshold, the classes and
  the expiry are the transport's, and this plan changed none of them.
- **`caps=f`.** Plan 494's advertisement stays blocked on Plan 493, and the
  peer test touches no builder and no permit.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `cdfed225` | the plan of record, its registry row and its roadmap row |
| `3c91db86` | the implementation: the sealed proof, the runtime's recorder, the daemon's owner, the responder on the live inbound path, the opt-in and the wiring |
| `5390f015` | the lane, the guard, the documentation, the support inventory and the plan's amendment log |
| `8e487f3d` | the routine floor's two findings, fixed |

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the opt-in is **off** by default and a router with it off sends no probe | `crates/i2pr-daemon/tests/peer_test_role.rs`, `the_opt_in_is_off_by_default` |
| 2 | the opt-in requires the SSU2 service | `crates/i2pr-daemon/tests/peer_test_role.rs`, `the_opt_in_requires_the_ssu2_service` |
| 3 | the responder answers with the probed fields **unchanged**, back to the peer that sent the test | `crates/i2pr-daemon/tests/peer_test_role.rs`, `the_responder_answers_with_the_probed_fields_unchanged` |
| 4 | a probe that is answered is **matched** and completes with the locally measured elapsed time | `crates/i2pr-daemon/tests/peer_test_role.rs`, `the_responder_answers_with_the_probed_fields_unchanged` |
| 5 | a confirmed probe records the router's **own** reachability | `crates/i2pr-daemon/tests/peer_test_role.rs`, `a_confirmed_exchange_records_the_router_s_own_reachability` |
| 6 | the transport's tracker reaches `Reachable` once the peer test supplies the third class | `crates/i2pr-daemon/tests/peer_test_role.rs`, `the_transport_s_tracker_reaches_reachable_with_the_confirmed_peer_test` |
| 7 | a probe that is not answered times out, is counted, and records **no** signal | `crates/i2pr-daemon/tests/peer_test_role.rs`, `a_probe_that_is_not_answered_records_no_signal` |
| 8 | an unmatched echo matches nothing | `crates/i2pr-daemon/tests/peer_test_role.rs`, `an_unmatched_echo_matches_nothing` |
| 9 | the table's ceiling **refuses** a probe rather than evicting one | `crates/i2pr-daemon/tests/peer_test_role.rs`, `the_table_ceiling_refuses_a_probe_rather_than_evicting_one` |
| 10 | a repeated key is answered at most once | `crates/i2pr-daemon/tests/peer_test_role.rs`, `a_repeated_key_is_answered_at_most_once` |
| 11 | the responder records nothing about itself | `crates/i2pr-daemon/tests/peer_test_role.rs`, `the_responder_answers_with_the_probed_fields_unchanged` and `a_confirmed_exchange_records_the_router_s_own_reachability` |
| 12 | the probed timestamp is not the clock | `crates/i2pr-daemon/tests/peer_test_role.rs`, `the_probed_timestamp_is_not_the_clock` |
| 13 | the answer's identifier is fresh and never zero | `crates/i2pr-daemon/tests/peer_test_role.rs`, `the_answer_identifier_is_fresh_and_never_zero` |
| 14 | the owner records the probe **before** the send | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_probe_is_recorded_before_the_send` |
| 15 | the recorder's only production call site is the confirmed-match path | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_recorder_has_one_production_call_site` |
| 16 | the controlled recorder is untouched | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_controlled_recorder_is_untouched` |
| 17 | the owner adds no task, no timer and no listener | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_owner_adds_no_infrastructure` |
| 18 | the daemon records no reachability signal of its own | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_daemon_records_no_reachability_signal` |
| 19 | the responder derives the answer from the authenticated delivery | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_responder_uses_the_authenticated_delivery` |
| 20 | the inbound path decodes the message **once** | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_inbound_path_decodes_once` |
| 21 | the opt-in defaults off and needs the service | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_opt_in_defaults_off_and_needs_the_service` |
| 22 | the daemon asserts **no** address family | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_daemon_asserts_no_family` |
| 23 | the answered-key table is bounded | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_answered_key_table_is_bounded` |
| 24 | the transport's corroboration policy is untouched | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_corroboration_policy_is_untouched` |
| 25 | the publication is untouched | `crates/i2pr-daemon/tests/peer_test_role_boundary.rs`, `the_publication_is_untouched` |
| 26 | the owner's own unit behaviour | `cargo test --locked -p i2pr-daemon --lib peer_test` — 21 tests |
| 27 | the runtime's own recorder and the transport's own tracker | `cargo test --locked -p i2pr-runtime --lib ssu2_runtime` and `cargo test --locked -p i2pr-transport --lib reachability` |

## Focused verification

```text
cargo test --locked -p i2pr-daemon --test peer_test_role -- --test-threads=1
    12 passed; 0 failed
cargo test --locked -p i2pr-daemon --test peer_test_role_boundary -- --test-threads=1
    25 passed; 0 failed
cargo test --locked -p i2pr-daemon --lib peer_test -- --test-threads=1
    21 passed; 0 failed
cargo test --locked -p i2pr-runtime --lib ssu2_runtime -- --test-threads=1
    passed
cargo test --locked -p i2pr-transport --lib reachability -- --test-threads=1
    passed
cargo check --locked --workspace --all-targets
    clean
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    clean
```

## The routine floor

The routine floor ran on the committed tree at `8e487f3d4ba242b04469e4b84d50279c115bf61f` with a
**fresh** script (`/tmp/floor-521b.sh`) that was never edited while it ran:
bash reads a script incrementally, so rewriting one under a running instance
makes that instance resume at a byte offset in the new text and silently skip
whole blocks of steps. The log is `/tmp/floor-521b.log`.

**154 steps, 154 PASS, 0 FAIL.** The reconciliation is exact:
the script declares 154 steps and the log carries 154
results, the step names match one for one, and no step failed.

| Step | Result | Seconds |
| --- | --- | --- |
| `floor-tree-clean` | PASS | 0 |
| `cargo-fmt` | PASS | 3 |
| `cargo-check` | PASS | 8 |
| `cargo-build-siblings` | PASS | 0 |
| `cargo-build-static-fixture` | PASS | 0 |
| `cargo-copy-static-fixture` | PASS | 0 |
| `cargo-rebuild-plain-fixture` | PASS | 0 |
| `cargo-test-workspace` | PASS | 1249 |
| `cargo-clippy` | PASS | 15 |
| `cargo-doc` | PASS | 7 |
| `cargo-test-doc` | PASS | 4 |
| `check-dependency-direction` | PASS | 0 |
| `check-global-plan-number-uniqueness` | PASS | 0 |
| `check-adr-number-uniqueness` | PASS | 0 |
| `check-portable-service-tunnel-api` | PASS | 0 |
| `check-portable-service-tunnel-consumer` | PASS | 7 |
| `planning-tests` | PASS | 6 |
| `check-runtime-boundaries` | PASS | 1 |
| `check-console-boundaries` | PASS | 1 |
| `check-console-browser-security` | PASS | 0 |
| `check-tooling-inventory` | PASS | 0 |
| `check-license-metadata` | PASS | 0 |
| `check-managed-app-private-client-seams` | PASS | 1 |
| `check-router-readiness-contract` | PASS | 0 |
| `check-router-readiness-contract-self` | PASS | 0 |
| `check-core-router-recovery-contract` | PASS | 0 |
| `check-core-router-recovery-contract-self` | PASS | 0 |
| `plan449-ssu2-preflight-self` | PASS | 0 |
| `plan495-peer-eligibility-self` | PASS | 0 |
| `plan495-peer-eligibility-profile-drift` | PASS | 0 |
| `plan476-gate-binding` | PASS | 0 |
| `plan476-gate-binding-self` | PASS | 1 |
| `check-managed-app-package-boundary` | PASS | 0 |
| `check-managed-app-package-boundary-self` | PASS | 0 |
| `check-managed-app-policy-boundary` | PASS | 0 |
| `check-managed-app-policy-boundary-self` | PASS | 0 |
| `check-managed-app-secured-sandbox` | PASS | 0 |
| `check-managed-app-secured-sandbox-self` | PASS | 0 |
| `check-service-tunnel-boundaries` | PASS | 0 |
| `check-managed-app-gateway-boundary` | PASS | 0 |
| `check-managed-app-manager-boundary` | PASS | 0 |
| `check-m11-per-epoch-composition` | PASS | 0 |
| `check-service-anonymity-boundaries` | PASS | 1 |
| `check-fixture-manifest` | PASS | 0 |
| `check-ntcp2-vectors` | PASS | 0 |
| `check-ssu2-vectors` | PASS | 0 |
| `check-i2cp-vectors` | PASS | 0 |
| `check-ntcp2-interoperability` | PASS | 0 |
| `check-constrained-host-lane-boundary` | PASS | 0 |
| `check-m11-transit-boundaries` | PASS | 2 |
| `check-m11-transit-qualification-evidence` | PASS | 2 |
| `check-sam-acceptance-evidence` | PASS | 0 |
| `check-ssu2-acceptance-evidence` | PASS | 0 |
| `check-i2cp-acceptance-evidence` | PASS | 0 |
| `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| `check-els2-type11-transcript-boundary` | PASS | 0 |
| `check-els2-live-lane-evidence` | PASS | 2 |
| `check-encrypted-service-consumer-caller` | PASS | 0 |
| `check-outproxy-request-path` | PASS | 0 |
| `check-outproxy-wire-lane-evidence` | PASS | 0 |
| `check-config-secret-hygiene` | PASS | 1 |
| `check-workflow-validity` | PASS | 0 |
| `check-floodfill-type5-serve` | PASS | 0 |
| `check-service-tunnel-acceptance-evidence` | PASS | 0 |
| `check-exploratory-tunnel-evidence` | PASS | 1 |
| `check-netdb-tunnel-evidence` | PASS | 0 |
| `check-destination-tunnel-evidence` | PASS | 0 |
| `check-streaming-tunnel-evidence` | PASS | 5 |
| `check-m6-mixed-router-acceptance-evidence` | PASS | 3 |
| `check-m12-floodfill-qualification-evidence-self` | PASS | 1 |
| `check-m12-floodfill-boundaries-self` | PASS | 0 |
| `ntcp2-harness-tests` | PASS | 0 |
| `plan488-mesh-lane` | PASS | 0 |
| `plan488-evidence-deterministic` | PASS | 0 |
| `plan489-mesh-lane` | PASS | 1 |
| `plan489-evidence-deterministic` | PASS | 0 |
| `plan490-mesh-lane` | PASS | 0 |
| `plan490-evidence-deterministic` | PASS | 0 |
| `plan490-ssu2-transport-suite` | PASS | 7 |
| `plan490-transport-boundary-guard` | PASS | 0 |
| `plan491-mesh-lane` | PASS | 1 |
| `plan491-evidence-deterministic` | PASS | 0 |
| `plan491-boundary-guard` | PASS | 0 |
| `plan491-proto-suite` | PASS | 0 |
| `plan491-netdb-suite` | PASS | 88 |
| `plan491-client-suite` | PASS | 24 |
| `plan492-posture-suite` | PASS | 1 |
| `plan492-m11-lane` | PASS | 4 |
| `plan502-transit-composition-lane` | PASS | 1 |
| `plan502-transit-composition-unit` | PASS | 0 |
| `plan502-transit-boundary-self` | PASS | 0 |
| `plan496-private-lan-profile` | PASS | 0 |
| `plan496-ntcp2-activation-gate` | PASS | 0 |
| `plan497-routing-consumer-lane` | PASS | 1 |
| `plan497-netdb-preferred-order` | PASS | 1 |
| `plan497-daemon-participation-unit` | PASS | 2 |
| `plan498-introducer-handoff-lane` | PASS | 1 |
| `plan498-introducer-mesh-lane` | PASS | 0 |
| `plan498-bootstrap-resolver-row` | PASS | 25 |
| `plan498-runtime-relay-accessor` | PASS | 6 |
| `plan503-version-eligibility-live` | PASS | 1 |
| `plan503-version-eligibility-unit` | PASS | 0 |
| `plan504-liveness-boundary` | PASS | 1 |
| `plan504-liveness-live` | PASS | 0 |
| `plan504-liveness-unit` | PASS | 0 |
| `plan505-driver-boundary` | PASS | 1 |
| `plan505-driver-live` | PASS | 2 |
| `plan505-driver-unit` | PASS | 1 |
| `plan506-reconcile-boundary` | PASS | 5 |
| `plan506-reconcile-live` | PASS | 1 |
| `plan507-retention-feasibility` | PASS | 1 |
| `plan507-retention-boundary` | PASS | 0 |
| `plan507-retention-live` | PASS | 1 |
| `plan508-creator-retention-feasibility` | PASS | 0 |
| `plan508-creator-retention-boundary` | PASS | 1 |
| `plan508-creator-retention-live` | PASS | 1 |
| `plan509-lookup-target-decision` | PASS | 0 |
| `plan509-lookup-target-boundary` | PASS | 1 |
| `plan510-managed-staging-load-lane` | PASS | 0 |
| `plan510-managed-reason-boundary` | PASS | 1 |
| `plan510-tunnelmanager-black-box` | PASS | 2 |
| `plan511-version-ceiling-derivation` | PASS | 0 |
| `plan511-version-ceiling-boundary` | PASS | 1 |
| `plan512-destination-liveness-live` | PASS | 0 |
| `plan512-destination-liveness-boundary` | PASS | 0 |
| `plan489-peer-budget-receipt` | PASS | 1 |
| `plan513-discovery-live` | PASS | 2 |
| `plan513-discovery-boundary` | PASS | 0 |
| `plan513-discovery-unit` | PASS | 2 |
| `plan513-discovery-seam-unit` | PASS | 0 |
| `plan514-exploratory-target` | PASS | 8 |
| `plan514-exploratory-target-boundary` | PASS | 0 |
| `plan516-netdb-publication` | PASS | 1 |
| `plan515-publication` | PASS | 1 |
| `plan515-publication-boundary` | PASS | 0 |
| `plan516-confirmation` | PASS | 3 |
| `plan516-confirmation-boundary` | PASS | 0 |
| `plan517-publication-retry` | PASS | 4 |
| `plan517-publication-retry-boundary` | PASS | 0 |
| `plan518-publication-refresh` | PASS | 1 |
| `plan518-publication-refresh-boundary` | PASS | 0 |
| `plan519-publication-off-loop` | PASS | 2 |
| `plan519-publication-off-loop-boundary` | PASS | 0 |
| `plan520-publication-address` | PASS | 1 |
| `plan520-publication-address-boundary` | PASS | 0 |
| `plan520-netdb-addressed-builder` | PASS | 1 |
| `plan520-runtime-install-gate` | PASS | 0 |
| `plan521-peer-test-lane` | PASS | 1 |
| `plan521-peer-test-boundary` | PASS | 1 |
| `plan521-peer-test-owner-unit` | PASS | 0 |
| `plan521-runtime-recorder-unit` | PASS | 4 |
| `plan521-transport-reachability-unit` | PASS | 1 |
| `plan514-switch-unit` | PASS | 0 |
| `cargo-deny` | PASS | 1 |

The five Plan 521 steps are `plan521-peer-test-lane` (the lane),
`plan521-peer-test-boundary` (the guard), `plan521-peer-test-owner-unit` (the
owner's own unit behaviour), `plan521-runtime-recorder-unit` (the runtime's
recorder) and `plan521-transport-reachability-unit` (the transport's own
tracker).

## The floor's first run, and the fixes

The routine floor was first run on the implementation commit `3c91db86`, and it
reported **154 steps, 152 PASS, 2 FAIL**. Every failure came from this plan's
own changes and from nothing else, and each was fixed rather than tolerated:

1. **`cargo fmt --all --check`** — the transport's re-export list was ordered
   by hand rather than by rustfmt, so the check found a diff the author had
   not noticed. Fixed with `cargo fmt --all`. No behaviour changed: the list
   names the same seven items.
2. **`check-m12-floodfill-boundaries --self-test`** — the M12 checker's rule
   — corroboration recording stays inside the runtime service owner — scans
   `crates/` for `reachability.record(`, and its `--glob '!*/tests/*'`
   exclusion does not reach a nested `crates/*/tests/` path, because a
   ripgrep glob's `*` does not cross a directory separator. It therefore read
   this plan's own **probe fixture** — a string literal inside a test file
   that stands for a hypothetical violating runtime — as if it were a
   production corroboration write. The checker was **not** modified: widening
   its exclusion would be a forbidden broad exclusion, and the rule it owns
   is a real one. The **fixture** was rewritten instead, exactly as Plan 520
   rewrote its own probe fixture: the controlled mirror now records through
   `state.tracker.record(` rather than `state.reachability.record(`. The
   probe stays meaningful, because the guard's own rule 5 keys on the
   `ReachabilitySignal` token the fixture still names, and the delegated
   call the probe is about is unchanged.

The fixes are a **fix**, not a relaxation: no script's rule was weakened, no
rule was disabled, no exclusion was widened, and no production wire was
changed to go green. The floor's own script was **not** edited while it ran:
a fresh script (`/tmp/floor-521b.sh`) was written for the closing run, and the
first run's log is `/tmp/floor-521.log`.

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be
reported. A rule with no probe is a rule nobody has shown to fire. The
guard carries **12 rules** and **13 inverted probes**, and the control
proves that each mutation is reported:

| # | Rule | The inverted probe |
| --- | --- | --- |
| 1 | the opt-in defaults off and needs the service | an opt-in that defaults on |
| 2 | the probe is recorded before the send | a probe that is sent before it is recorded |
| 3 | the recorder has one production call site | a second production call site |
| 4 | the daemon asserts no family | a daemon that names `AddressFamily::Ipv4` |
| 5 | the controlled recorder is untouched | a recorder that delegates to the controlled mirror |
| 6 | the daemon records no reachability signal | a daemon that writes `ReachabilitySignal::` itself |
| 7 | the responder uses the authenticated delivery | a responder that decodes the message again |
| 8 | the inbound path decodes once | an inbound path that decodes twice |
| 9 | the owner adds no infrastructure | an owner that spawns a task |
| 10 | the answered-key table is bounded | an owner with no answered-key table |
| 11 | the corroboration policy is untouched | a daemon that names `MIN_CORROBORATION_FLOOR` |
| 12 | the publication is untouched | an owner that builds a published address |

The thirteenth probe is the **unit** that proves the guard's own
`impl_body` scoping is load-bearing: the tracker's `expire` and the owner's
`expire` share a signature, so the rule that is about the owner's expiry
scopes its lookup to the owner's own `impl` block.

## Invariant review

The plan's eight invariants, each against what the code does:

1. **No fabricated evidence.** The daemon records a reachability signal only
   when a **matched** echo completed an outstanding probe, and only with the
   transport's **sealed** proof — a type with no constructor that takes a
   boolean or a claim. A timeout, an unmatched echo, a peer mismatch and a
   refused probe all record **nothing**. *Proved by* the lane's
   `a_probe_that_is_not_answered_records_no_signal` and
   `an_unmatched_echo_matches_nothing`, and by the guard's
   `the_daemon_records_no_reachability_signal` and
   `the_recorder_has_one_production_call_site`.
2. **The transport's tracker remains the single authority.** The daemon
   never computes a reachability state, never reads one, and never writes a
   state into a published record. It hands the runtime a typed outcome and
   the runtime decides. The daemon's name of a family was removed for
   exactly this reason (§17.2). *Proved by* the guard's
   `the_daemon_records_no_reachability_signal`,
   `the_daemon_asserts_no_family` and `the_corroboration_policy_is_untouched`.
3. **The answer is derived from the authenticated delivery.** The answer
   carries the authenticated peer and the exact link the test arrived on,
   and it travels back to that peer over the router's own delivery seam. No
   answer is ever routed from a caller-supplied identity. *Proved by* the
   guard's `the_responder_uses_the_authenticated_delivery` and by the
   lane's live exchange over a real session.
4. **The bounded table stays bounded.** The outstanding-probe table keeps
   its capacity ceiling and its age ceiling; a probe is refused rather than
   evicting an older one, so an answer can never be matched against a
   different probe. The answered-key table has the same shape. *Proved by*
   the lane's `the_table_ceiling_refuses_a_probe_rather_than_evicting_one`
   and `a_repeated_key_is_answered_at_most_once`, and by the guard's
   `the_answered_key_table_is_bounded`.
5. **The publication is unchanged.** The period, the fan-out, the ladder's
   timing, the correlator, the freshness rule and the task's ownership are
   untouched, and Plan 520's owner still refreshes the record inside the
   cycle's own branch. *Proved by* the guard's `the_publication_is_untouched`.
6. **`caps=f` stays permit-gated.** The peer test touches no builder and no
   permit. *Proved by* the guard's `the_publication_is_untouched`.
7. **Nothing is advertised.** `router.version` stays `0.9.62`, the published
   RouterInfo's `caps` string is unchanged, the support inventory's
   advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
   byte-identical. *Proved by* the inventory's own count and by the diff.
8. **The controlled floodfill path is left alone.** Its `PeerTestResult`
   recording, its permit and its activation are untouched. *Proved by* the
   guard's `the_controlled_recorder_is_untouched`.

## Failure, cancellation, restart, and contention review

- **A refused probe** (the table is full) is counted under its own reason
  (`probes_refused_full`). The table is a ceiling, not a target: a probe is
  refused rather than evicting an older outstanding probe.
- **A refused probe** (the peer already has one) is counted under its own
  reason (`probes_refused_in_flight`). One probe at a time per peer.
- **A refused probe** (the owner is disabled) is counted under its own
  reason (`probes_refused_disabled`).
- **A refused answer** (the answered-key table is full) is counted under its
  own reason (`answers_refused_full`) and the answer is **not** sent.
- **A timeout** expires the probe and counts it. The tracker is left
  otherwise untouched: a failed test records **no** signal, so it can never
  poison later evidence.
- **An unmatched echo** is counted (`echoes_unmatched`) and matched to
  nothing. It is still **owed** an answer, because it is a probe from a
  peer the router does not have an outstanding exchange with.
- **A peer mismatch** is counted (`echoes_peer_mismatch`), completes
  nothing, and **releases no entry** — the real peer can still answer.
- **A late echo** is neither completed nor answered: the peer that sent it
  is not waiting for an answer.
- **A restart** rebuilds the owner with an empty table. No probe survives a
  restart, because a probe is only meaningful within the session that
  carried it.
- **Cancellation** — the owner holds no task of its own; it runs inside the
  service's existing loop and its cancellation path is the service's. The
  delivery seam checks the cancellation token before admission.
- **The daemon's own failure paths** — a refused delivery, a poisoned state
  lock and a closed runtime are each reported and counted; none of them
  fabricates evidence.

## Migration and compatibility review

- **No wire change.** Message 231 and its echo already exist in the proto
  crate and are already classified by the dispatcher.
- **No dependency change.** `Cargo.toml` and `Cargo.lock` stay
  byte-identical.
- **No configuration migration.** The opt-in is new and defaults to off, so
  the default configuration changes in no way. A configuration that asks for
  the peer test without the SSU2 service is **refused** at parse time rather
  than silently inert.
- **The published RouterInfo's `caps` string is unchanged**, and
  `router.version` stays `0.9.62`.
- **The default router's behaviour is unchanged**: with the opt-in off no
  probe is sent and no signal is recorded.
- **The support inventory grew by exactly one surface** — 112 → 113 — and
  the advertised count stayed zero.

## Security review

- **No fabricated evidence.** The sealed proof is the mechanism, and the
  guard proves there is exactly one production call site.
- **No unbounded reply traffic.** The answered-key table bounds the replies
  a peer can provoke, and the outstanding table bounds the probes. Both
  refuse rather than evict, and both age out.
- **The probed timestamp is drawn at random**, so a peer cannot make its
  own probe look like this router's echo.
- **The answer is derived from the authenticated delivery**, never from a
  caller-supplied identity, and it travels to that peer.
- **One local condition is never written into a peer's profile.** The
  recorded signal belongs to the **service** tracker — the router's own
  reachability. The guard proves the daemon writes **no** reachability
  signal of its own, and the peer-test owner touches **no** peer-profile
  store at all: it names no profile type and writes no profile entry.
- **No secrets are logged.** The counters are counts; the debug traces name
  a transition and a reason, and never a payload, a key or a peer.
- **The runtime refuses a session that is already gone** rather than
  recording the evidence under a guessed family.
- **The opt-in is off by default**, so a router that has not asked for
  reachability evidence never generates probe traffic.

## Limitations

1. **The stock-peer half is not proved and is not claimed**, and it is now
   blocked for a second reason besides Plan 493's topology: no stock router
   implements a router-level I2NP 231 responder. The lane drives the
   router-level responder against **this router's own peer** over a real
   SSU2 session on loopback.
2. **The probe's cadence is this router's own.** The plan chose
   `300 000 ms` between probes and a `30 000 ms` retry when no peer with a
   session was available. The transport's evidence lapses after thirty
   minutes, so the cadence refreshes it well inside that window. The
   reference's own rates are not matched, and no claim is made that they
   are.
3. **The owner probes one peer at a time** and takes the first peer with a
   live session in a stable order. It does not build a pool, does not dial,
   and does not rotate.
4. **The lane's live exchange is a two-router loopback exchange.** It
   proves the responder, the match, the recording and the transport's own
   threshold. It does not prove anything about a stock peer's behaviour,
   because no stock peer speaks this mechanism.
5. **The transport's corroboration classes 1 and 2 are not triggered by the
   lane.** The lane shows that the confirmed peer test supplies the missing
   third class by driving the transport's own `ReachabilityTracker`
   directly; the live runtime's classes 1 and 2 come from the session's
   address observation and its path validation, and are the runtime's own.

## Findings by severity

### Medium — none open

Plan 520's **M1** is **discharged**. The direct form's third corroboration
class — a confirmed peer test — now has a production entry point, and the
recorded outcome is the router's **own** reachability.

### Low — recorded, not fixed

- **L1 — the peer-test cadence is this router's own.** The interval
  (`300 000 ms`) and the retry delay (`30 000 ms`) are the daemon's
  constants, and no reference rate is matched. A successor that measures
  the reference's cadence would refine them.
- **L2 — the owner probes one peer at a time.** The selection takes the
  first peer with a live session in a stable order. It does not rotate and
  it does not spread the load, because the plan needs only *a* peer with a
  session and the outbound peer-session pool is a separate, larger piece of
  work.
- **L3 — the answered-key table's age window is the probe's timeout**
  (`10 000 ms`). A peer that repeats a key just after the window may be
  answered again. The replies are still bounded per window, and the bound
  is the router's own; a successor may widen the window if the traffic
  warrants it.

### Informational

- **I1 — the registered §3's citations drifted**, and one function was
  retired. The drift is recorded additively in §17.5; §3 itself is not
  rewritten.
- **I2 — the registered plan named no bound on the reply traffic**, and the
  implementation added one. The correction is §17.1.
- **I3 — the registered plan's §6.3 said the recorder takes the typed
  confirmed outcome**, and the implementation made the runtime derive the
  family. The correction is §17.2.

## Roadmap disposition

The roadmap's `network-participation` sheet now names **Plan 521** as its
authority plan, and the row it carries records the closure. The three
consequences that changed:

- the **authority line** moves from Plan 520 to Plan 521 — the ordinary
  router's own address can now be **earned** in production, because the third
  corroboration class is a confirmed peer test and the router has its own
  bounded peer test to supply it;
- the **RouterInfo** row's missing cell no longer says that the qualified
  form's production evidence route is Plan 521's: the route **exists** now,
  and only the **stock-peer** half — that a stock peer qualifies the
  published record — remains Plan 493's;
- the **Roles** row now lists the router's own peer test beside the
  controlled floodfill service, and the consequence paragraph no longer says
  that no production peer test exists.

The **Transports** row is unchanged, and its missing cell still names the
**outbound peer-session pool** as missing. That is deliberate: Plan 521 needs
only *a* peer with a session, and the pool is a separate, larger piece of
work. It is the next thing in this line that would let the router hold more
than one peer to ask.

The **stock-peer** half of this plan, Plan 520's stock-peer half, Plan 497's
M2, Plan 487's stock-peer half, and Plans 519 and 520's stock-peer halves
all stay blocked on **Plan 493's** authorized two-host topology.

## Unblock audit

The unblock audit asks, of every blocked item this line carries, whether
this plan changed its status. The answer is **no**, except for the one item
this plan was opened to discharge.

| Blocked item | Before Plan 521 | After Plan 521 |
| --- | --- | --- |
| Plan 520's **M1** — the ordinary record's address cannot be earned in production | open, Medium | **discharged** — the third corroboration class has a production entry point |
| Plan 520's stock-peer half — a stock peer qualifies the published record | blocked on Plan 493 | blocked on Plan 493 |
| Plan 521's stock-peer half | blocked on Plan 493 | blocked on Plan 493, **and** on the second ground that no stock router implements a router-level I2NP 231 responder |
| Plan 497's **M2** / limitation 1 — the stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 487's stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 519's and Plan 520's stock-peer halves | blocked on Plan 493 | blocked on Plan 493 |
| Plans 433, 435, 436, 437, 438, 439, 493, 494 | blocked on external environments | blocked on external environments — untouched |
| The **outbound peer-session pool** | missing | missing — Plan 521 needs only *a* peer with a session |
| The **`caps=f`** advertisement | Plan 494, blocked on Plan 493 | Plan 494, blocked on Plan 493 |
| The no-**NTCP2**-ingress and no-**OBEP**-consumer gaps | missing | missing — untouched |

Nothing was unblocked that this plan did not itself open, and nothing was
promoted. Plan 493's two-host topology is the gate on every remaining
stock-peer half, and Plan 521's own stock-peer half now waits on it for one
more reason than it did.

## Handoff notes

- The next free plan number after this one is **522**.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`). Neither reference is patched
  or vendored.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology, and it is now blocked on a second ground as well: no stock
  router implements a router-level I2NP 231 responder.
- Plan 520's **M1** is **discharged** by this plan. A successor does not
  need to reopen it.
- The transport's corroboration policy is untouched, and it remains the
  single authority on when three classes are enough.
- The **outbound peer-session pool** is still missing, and it is the next
  thing in this line that would let the router hold more than one peer to
  ask.
