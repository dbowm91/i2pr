# Plan 529 — the router must learn the address at which a peer sees it

**Closure record.** The plan of record is
`plans/implementation/network-participation/529-the-router-must-learn-the-address-at-which-a-peer-sees-it.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 529 is **closed**. The router learns the address at which a peer sees
it: the SSU2 handshake carries the specification's Address block with the
right **value**, the tree **reads** it instead of discarding it, and the
runtime compares it with the router's **own bound socket**.

Classification: **corrective** (the tree sent the specification's Address block
with the **wrong** value — the responder published its own bound address as
the initiator's — and then **threw the answer away**, so the class-1
corroboration had **no field producer** and `Reachable` could never be
earned). Plan 527 gave the router's reachability one owner and one reader;
Plan 528 made the negative verdict a fact; this plan gives the positive side
a fact to be made of.
**Subsystem:** network-participation. **Closure date:** 2026-11-10.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The defect — the block carried the wrong value.** The SSU2 specification's
Address block (type 13) is defined as "Alice's address, sent to Alice by Bob,
or Bob's address, sent to Bob by Alice" — the address at which the
**recipient** sees the **sender**. The responder sent its **own** bound
address:

```rust
        blocks.push(PayloadBlock::Address(self.config.local_address));
```

The field is **gone** from `ResponderConfig`
(`crates/i2pr-transport-ssu2/src/state_machine.rs:899`), and the block is
built from the observed source now (`:1212`). So Bob told Alice that
**Alice's** address was **Bob's own**. The pinned i2pd 2.61.0 does the
opposite: it puts the **initiator's** remote endpoint in the Session Created
(`libi2pd/SSU2Session.cpp:999`).

**The second defect — the answer was thrown away.** The initiator decrypted
the Session Created's payload and read only the establishment token from it:

```rust
        let peer_new_token = find_establishment_token(&created_payload)
```

The Address block beside it was **decoded and discarded**; the read is
there now (`crates/i2pr-transport-ssu2/src/state_machine.rs:779`). The
reference reads the whole decrypted payload in the very same message
(`libi2pd/SSU2Session.cpp:1110`), and its `HandleAddress`
(`libi2pd/SSU2Session.cpp:2067`) logs **"SSU2: Our external address is "**
(`libi2pd/SSU2Session.cpp:2072`) and hands the address to the router
(`libi2pd/SSU2Session.cpp:2075`).

**The third defect — the corroboration class had no producer.**
`AuthenticatedPeerObservedExternalAddress`
(`crates/i2pr-transport/src/reachability.rs:226`) is corroboration **class 1**
(`:223`), and the receiver's seam already existed: Plan 527 made the runtime
compare an observed address with the router's own bound socket
(`crates/i2pr-runtime/src/ssu2_runtime.rs:6987`). But the **handshake never
reached that seam**, so the class had no field producer outside the controlled
profile. The corroboration floor needs **three** distinct supporting classes
before `Reachable` (`crates/i2pr-transport/src/reachability.rs:66`), so the
router could never claim that it was directly reachable: Plan 526's recorded
open limitation, still open after Plans 527 and 528.

**What the tree does now.**

- The responder's Address block carries the **observed source address** of the
  request it answers, built from the datagram's own header and never from a
  configured or bound value
  (`observed_address_block`, `crates/i2pr-transport-ssu2/src/state_machine.rs:892`,
  used at `:1212` for the Session Created, `:1060` and `:1333` for the two
  Retry sites). `ResponderConfig::local_address` is **gone**
  (`crates/i2pr-transport-ssu2/src/state_machine.rs:899`) — the value it held
  was never the right one, and keeping a field for it would keep the wrong
  possibility open.
- The initiator's Session Request carries **Alice's view of Bob**, from
  `InitiatorConfig::remote_address`
  (`crates/i2pr-transport-ssu2/src/state_machine.rs:351`), and the responder
  reads it. The specification's block is symmetric, and the reference reads
  it in **both** directions (`libi2pd/SSU2Session.cpp:934`).
- The initiator **reads** the Session Created's Address block out of the
  decrypted payload (`crates/i2pr-transport-ssu2/src/state_machine.rs:779`,
  through `find_observed_address`,
  `crates/i2pr-transport-ssu2/src/handshake.rs:1083`) and carries it on the
  established session (`observed_address`,
  `crates/i2pr-transport-ssu2/src/state_machine.rs:233`, read through the
  accessor at `:314`).
- The responder reads the Session Request's Address block in the transition
  that **decrypts** the request
  (`crates/i2pr-transport-ssu2/src/state_machine.rs:1254`) and carries it
  across to the transition that **establishes** the session, because those are
  two different transitions.
- The runtime routes the observation into Plan 527's seam
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:5216` and `:5223`), which compares
  it with the router's **own bound socket** (`note_observed_external_address_locked`,
  `:2888`, against `bound_endpoint`, `:2943`). An agreement corroborates and a
  disagreement is Plan 528's firewalled verdict, unchanged.
- The comparison is recorded only once a session was really established, so a
  corroboration class is never counted for a session that does not exist.

## What the plan did not close

- The **production** SSU2 peer-test driver stays unbuilt. Its choreography
  question is named: the tree's Alice role needs the helper's identity up
  front, while the specification has Bob select the helper.
- The **requester** side of the SSU2 relay protocol stays unwired. Its owner
  is named in Plan 527's §5.3.
- The **`B` advertisement** and the **`caps=f` advertisement** stay out of
  scope, with their owners named.
- The **public dial scope** stays out of scope; Plan 431 owns it.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `c2b57f32` | the registration: the plan of record, the registry row and the roadmap row |
| `103ea515` | the production change: the transport's sender side, the transport's receiver side and the runtime's seam |
| `862c121d` | the transport's wire-level rows, the daemon lane and the daemon guard |
| `756ce1f2` | the documentation, the inventory and the plan's amendment log |

## Requirement-to-evidence matrix

Every acceptance criterion of the plan's §14, the file and line that
satisfies it, and the test that proves it.

| # | Criterion | Satisfied by | Proven by |
| --- | --- | --- | --- |
| 1 | the responder's Address block carries the **initiator's** observed source address | `crates/i2pr-transport-ssu2/src/state_machine.rs:892` (`observed_address_block`), `:1212` (the Session Created), `:1060` and `:1333` (the two Retry sites) | guard rule 1 `the_responder_reports_the_observed_source_address`; the transport's own test `the_session_created_carries_the_initiator_address_not_the_responder_own` |
| 2 | the initiator **reads** it and the runtime compares it with the router's own bound socket | `crates/i2pr-transport-ssu2/src/state_machine.rs:779` (the initiator's read), `:314` (the accessor); `crates/i2pr-runtime/src/ssu2_runtime.rs:5216`-`:5223` (the routing), `:2888` (the comparison), `:2943` (the bound socket) | guard rules 2 and 3 `the_initiator_reads_the_address_block` and `the_observation_reaches_the_comparison`; the lane rows `a_handshake_corroborates_the_routers_own_reachability` and `the_comparison_is_against_the_routers_own_bound_socket` |
| 3 | an agreement corroborates and a disagreement is the router's own firewalled verdict | `crates/i2pr-runtime/src/ssu2_runtime.rs:2888` (the comparison, Plan 528's verdict unchanged) | the lane rows `a_handshake_corroborates_the_routers_own_reachability` and `a_disagreement_is_the_routers_own_firewalled_verdict` |
| 4 | the corroboration floor is unchanged | `crates/i2pr-transport/src/reachability.rs:66` (`MIN_CORROBORATION_FLOOR`, still 2), `:223` (the five classes, unchanged) | guard rule 4 `the_reachability_model_is_untouched`; the lane row `the_corroboration_floor_is_unchanged` |
| 5 | the transport's reachability model is untouched | `crates/i2pr-transport/src/reachability.rs:130` (`ReachabilitySignal`, five kinds), `:223` (`corroboration_class`, five classes) | guard rule 4 and its inverted probe `the_guard_reports_a_changed_reachability_model`; the lane row `the_transports_reachability_model_is_untouched` |
| 6 | no configuration key is added and no default moves | `crates/i2pr-daemon/src/config.rs` is untouched by this plan | guard rule 5 `no_new_key_and_no_infrastructure` and its inverted probe `the_guard_reports_a_new_configuration_key` |
| 7 | no wire format is added or changed | the block type (13), its length (6 or 18) and its placement are exactly what the tree already sent; only the value changes | the transport's own tests `the_session_created_carries_the_initiator_address_not_the_responder_own` and `the_retry_carries_the_observed_source_address` |
| 8 | the lane and the guard are green, every guard rule has an inverted probe that fires it, and every rule fails closed on an absent item | `crates/i2pr-daemon/tests/ssu2_observed_address.rs` (5 rows), `crates/i2pr-daemon/tests/ssu2_observed_address_boundary.rs` (6 rules, 8 inverted probes, 1 missing-item probe), `crates/i2pr-transport-ssu2/tests/handshake.rs` (4 wire-level rows) | the floor's `plan529-observed-address-lane` and `plan529-observed-address-boundary` steps |
| 9 | the routine floor is green on the closing tree | the floor table below | the floor run |
| 10 | `specs/support.toml` records the surface as `experimental` and `advertised = false`; the advertised count stays **zero**; `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` stay byte-identical; no boundary script is weakened | `specs/support.toml`, surface `daemon.observed-address`; `crates/i2pr-netdb/src/local.rs` (`CONTROLLED_ROUTER_VERSION`); neither manifest is touched; the diff touches **no** script under `scripts/` | `python3 scripts/check-tooling-inventory.py`; the counts are 122 surfaces, 117 experimental, 4 deferred, 1 ready, **0 advertised**; `git status` on both manifests is empty |

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS — no warnings |
| `cargo test --locked -p i2pr-transport --all-targets` | PASS — 52 rows and 6 rows |
| `cargo test --locked -p i2pr-transport-ssu2 --all-targets` | PASS — 159, 19, **26**, 9, 13 and 9 rows |
| `cargo test --locked -p i2pr-runtime --all-targets` | PASS — 133, 3, 19 and 7 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_observed_address -- --test-threads=1` | PASS — 5 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_observed_address_boundary -- --test-threads=1` | PASS — 15 rows |
| `cargo test --locked -p i2pr-daemon --test ssu2_peer_test_verdict -- --test-threads=1` | PASS — 10 rows (Plan 528's lane, unchanged) |
| `cargo test --locked -p i2pr-daemon --test ssu2_peer_test_verdict_boundary -- --test-threads=1` | PASS — 23 rows (Plan 528's guard, unchanged) |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-license-metadata.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests, OK |

## The routine floor

The floor ran on the **closing** tree (`756ce1f2`), and every step is recorded in
`/tmp/529-floor.log` with its own log under `/tmp/529f-<step>.log`. The floor is
a plain sequence of `step` runs that records each step's exit status; no step is
skipped, no `|| true`, no `continue-on-error` and no filename filtering. The
workspace step runs with `--no-fail-fast`, so every test binary's result is
reported rather than the run stopping at the first failure.

Plan 529's floor is Plan 528's floor plus two steps — the daemon lane and the
daemon guard for the observed address. **No other step changed, and no other
plan's guard file was touched.**

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
| `plan529-observed-address-lane` | PASS |
| `plan529-observed-address-boundary` | PASS |
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

**FLOOR PASS 97 — FLOOR FAIL 0.**

## The negative controls

Every guard rule has an **inverted probe**: a mutation that must be
reported. Every probe mutates an **in-memory copy** of the source, so **no
file on disk is written**. Every rule **fails closed** when the item it
names is absent — the guard reports the missing item rather than passing.

### Plan 529's guard — `crates/i2pr-daemon/tests/ssu2_observed_address_boundary.rs`

Six rules, eight inverted probes, one missing-item probe. All 15 rows green.

| Rule | What it forbids | Its inverted probe |
| --- | --- | --- |
| 1 | a responder that publishes its own bound address, or a block built from anything but the observed source | `the_guard_reports_a_responder_that_publishes_its_own_address`, `the_guard_reports_a_block_built_from_something_else` |
| 2 | an initiator that discards the Address block | `the_guard_reports_an_initiator_that_discards_the_address_block` |
| 3 | an observation that never reaches Plan 527's comparison | `the_guard_reports_an_observation_that_never_reaches_the_comparison` |
| 4 | a changed reachability model | `the_guard_reports_a_changed_reachability_model` |
| 5 | a new configuration key, or new infrastructure | `the_guard_reports_a_new_configuration_key`, `the_guard_reports_new_infrastructure` |
| 6 | a Retry that became a reachability observation | `the_guard_reports_a_retry_that_becomes_a_reachability_observation` |

Plus `the_guard_reports_a_missing_named_item`, which proves the
missing-item branch.

### The neutered seam — the lane proves the producer

The lane is only worth something if it drives the **producer**. The runtime's
seam was therefore **neutered** in a scratch copy of the tree — the call to
`note_observed_external_address_locked` was replaced with a discard — and the
lane was re-run. **All five rows failed**:

| Lane row | Why it failed with the seam neutered |
| --- | --- |
| `a_handshake_corroborates_the_routers_own_reachability` | no observation reached the tracker, so the corroboration count stayed 0 |
| `a_disagreement_is_the_routers_own_firewalled_verdict` | no observation reached the tracker, so the state stayed `Unknown` |
| `the_corroboration_floor_is_unchanged` | the same — the tracker held no class at all |
| `the_comparison_is_against_the_routers_own_bound_socket` | the same |
| `the_transports_reachability_model_is_untouched` | the same |

The seam was restored and the lane returned to green. The lane therefore tests
the **producer**, not the recorder.

### The transport's own negative rows

The transport's suite carries the four wire-level rows, and each one fails if
the value is wrong: the Session Created must carry the **initiator's**
address and not the responder's own, the Session Request must carry Alice's
view of Bob, the Retry must carry the same observed source address, and a
session with no Address block must be established with nothing observed.

## Acceptance criteria

The plan's §14 lists ten criteria and five stop conditions.

1. the responder's Address block carries the **initiator's** observed source
   address — **met**;
2. the initiator **reads** it and the runtime compares it with the router's
   own bound socket — **met**;
3. an agreement corroborates and a disagreement is the router's own firewalled
   verdict — **met**;
4. the corroboration floor is unchanged — **met**;
5. the transport's reachability model is untouched — **met**;
6. no configuration key is added and no default moves — **met**;
7. no wire format is added or changed — **met**;
8. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, and every rule fails closed on an absent item — **met**;
9. the routine floor is green on the closing tree — **met**;
10. `specs/support.toml` records the surface as `experimental` and
    `advertised = false`; the advertised count stays **zero**;
    `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` stay
    byte-identical; no boundary script is weakened — **met**.

**No stop condition fired.** The observation did **not** need a change to the
transport's reachability **model**; the comparison did **not** need a
configuration key or a second policy; the delivery did **not** need a new
task, a new timer or a new listener; and the plan did **not** turn out to
need the production peer-test driver or the requester side.

## Invariant review

The plan's §4 lists twelve invariants. Each is reviewed against the closing
tree.

1. **The Address block carries the address at which the recipient sees the
   sender.** The responder's block is built from the request's source and the
   initiator's from the dial target's address; neither is a configured or
   published value. — **held**.
2. **The responder's value is the observed source address of the request it
   answers.** `observed_address_block` reads the datagram's own source; the
   responder's bound address is no longer published at all. — **held**.
3. **The initiator's value is authenticated.** It rides the Session Request's
   sealed payload, so it is covered by the Noise transcript. — **held**.
4. **The comparison is against the router's own bound socket.** The observation
   reaches `note_observed_external_address_locked`, which reads
   `bound_endpoint`, and never a configured value. — **held**.
5. **An agreement corroborates and a disagreement contradicts; one observation
   is one fact.** The comparison is Plan 527's, unchanged; a contradiction is
   never also a corroboration. — **held**.
6. **The refusal on a family with no bound socket is unchanged.**
   `bound_endpoint` still refuses a zero port or an unspecified address. —
   **held**.
7. **The corroboration floor is unchanged.** `MIN_CORROBORATION_FLOOR` is
   still 2, and `Reachable` still needs three distinct supporting classes. —
   **held**.
8. **The transport's reachability model is untouched.** The five signal kinds,
   the five corroboration classes and the state machine are unchanged. —
   **held**.
9. **No configuration key is added and no default moves.** The daemon's
   configuration module is untouched. — **held**.
10. **No wire format is added or changed.** The block type (13), its length
    (6 or 18) and its placement are exactly what the tree already sent; only
    the value changes. — **held**.
11. **No new task, no new timer and no new listener.** The plan's own code
    carries none of the three, and the guard checks it. — **held**.
12. **Nothing is promoted.** `router.version` stays `0.9.62` and the
    advertised count stays **zero**. — **held**.

## Failure, cancellation, restart, and contention review

- **Failure.** Every refusal is counted under its own reason and leaves no
  state behind. A source that cannot be expressed as an SSU2 endpoint yields
  **no** block rather than a wrong one: `observed_address_block` returns
  `None`, and the tree's `parse_retry` refuses a Retry with no Address block,
  so the peer declines the Retry rather than being told a wrong address. A
  poisoned state mutex degrades to "no comparison", never to a fabricated
  agreement.
- **Cancellation.** The plan adds **no task**. The observation is captured and
  recorded inside the existing promotion seam, which is synchronous and
  non-awaiting.
- **Restart.** The router's reachability is recomputed from live evidence, so
  a restart with no handshake is `Unknown` and asks nobody. There is no
  persisted observation and no persisted reachability claim.
- **Contention.** The plan adds no queue, no task and no lock of its own. The
  comparison happens under the runtime's existing state lock, and the lock
  **order** is unchanged: the only nesting is `state` → `sockets` inside
  `bound_endpoint`, and no code path holds `sockets` while taking `state`.
- **The responder's per-handshake state.** The observation is carried on the
  responder's own machine, like `initiator_conn_id` and `initiator_eph`, and
  is **overwritten** by every new Session Request — so a handshake that sends
  no Address block clears a stale observation rather than inheriting one.

## Migration and compatibility review

- **No wire change.** The block type, its length and its placement are exactly
  what the tree already sent. The **value** changes, and it changes to what
  the specification and the pinned reference already use.
- **A corrected reading for the peer.** A peer that follows the specification
  now receives the address at which it is actually seen, instead of the
  responder's own address. The pinned i2pd 2.61.0 logs "SSU2: Our external
  address is " from that block; with this plan the tree's value is the one
  the reference's log line is about.
- **A widened API.** `ResponderConfig` **lost** a field and `InitiatorConfig`
  gained one. `build_retry`'s address became an `Option<AddressBlock>`, and
  `AuthenticatedSsu2Session` gained an `observed_address` accessor. Every
  in-tree caller was updated. No external crate used them.
- **No configuration migration.** No key is added, renamed or removed.

## Security review

- **No secret is logged.** The observation is an **address**, not a payload.
  The lane and the guard print states, counts and digests only; no key, tag,
  peer hash, signature or payload is ever printed.
- **No one local condition is written into a peer's profile.** The comparison
  is between the router's own bound address and what a peer reported about
  **this** router. Nothing is written into any peer's record.
- **The observation is authenticated.** It rides the sealed handshake
  payload, so a peer cannot put a value into the router's reachability state
  without completing the handshake.
- **A contradiction is never also a corroboration.** A single observation
  cannot both support and deny the router's reachability, so a peer cannot
  inflate the router's claim.
- **The disagreement is earned, not asserted.** The comparison must come from
  the router's **bound** socket, and a family with no bound socket refuses the
  comparison. A router never invents a contradiction it cannot support.
- **The Retry is not an observation.** The Retry's block is sealed outside the
  Noise transcript, so it is a weaker fact; `parse_retry` keeps discarding it
  and the runtime never records one. The guard checks both.
- **Fail closed throughout.** An inexpressible source yields no block, a
  missing block records nothing, a poisoned lock degrades to "no comparison",
  and the guard's absent-item case is a report.

## Limitations

- The delivery is confined to the **controlled loopback profile**: the
  comparison is between loopback addresses, so the observation is a
  **qualification** claim.
- The **production** SSU2 peer-test driver stays unbuilt, so the live router
  still cannot earn the peer-test corroboration class in the field.
- The **requester side** stays unwired, so the relay-success route is not
  taken in the field.
- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology. The plan's evidence is a local lane.
- The class-1 corroboration is now **earned**, but the corroboration floor
  still needs **three** distinct supporting classes, and the other two are
  not earned by this plan: the runtime's direct form is still not confirmed.
- The **responder's** half of the observation is weaker than the initiator's.
  The initiator's side is the recipient's view of the sender — the NAT fact,
  and independent of the dial target. The responder's side is the initiator's
  view of the responder, which **is** the dial target: it certifies the
  published address against the bound socket, and its **disagreement** is the
  responder's own firewalled fact. This is recorded, not hidden.

## Findings by severity

### Medium — none open

### Low — recorded, not fixed

1. **The production SSU2 peer-test driver is unbuilt.** Its choreography
   question is named: the tree's Alice role needs the helper's identity up
   front, while the specification has Bob select the helper. **Owner:** the
   successor that builds the production driver. **Not this plan's scope.**
2. **The requester side of the SSU2 relay protocol has no production
   caller.** Plan 527's finding, unchanged. **Owner:** the successor that
   implements the requester side. **Not this plan's scope.**
3. **The runtime's direct form still needs three corroboration classes.** This
   plan earns the **first** of them from a real handshake; the other two are
   not earned here. Plan 526 recorded the limitation and Plan 528 repeated it.
   **Owner:** the plan that earns the direct form.
4. **The delivery is confined to the controlled loopback profile.** The
   comparison is between loopback addresses, so the observation is a
   qualification claim. **Owner:** the plan that exercises the model against
   a stock peer.
5. **The responder's half of the observation is weak.** It is the dial
   target's address, so its corroboration is nearly implied by the session's
   establishment. It is recorded because its **disagreement** is the
   responder's own firewalled fact, and because it certifies the published
   address against the bound socket. **Owner:** the plan that earns the direct
   form, if it decides to weigh the two halves differently.

### Informational

1. **Plan 161's closure record named this defect and deferred it.** Its
   *cosmetic note (non-blocking, deferred)* recorded that "our responder
   SessionCreated Address block carries our own endpoint rather than the
   peer's observed endpoint, so i2pd logs `Our external address is
   127.0.0.1:44001` at info level". **This plan is that follow-up work.**
   Plan 161's record is not rewritten.
2. **Plan 282's closure record rejected recording handshake Address blocks
   as observations — against the tree's wrong value.** Its objection was
   sound against the tree as it stood: the block carried the **sender's own
   self-report**, so reading it back would have told the initiator nothing it
   did not already know. This plan changes the block's **value**, which is
   what the objection was about. Plan 282's record is not rewritten.
3. **The plan's own production change moved lines in four files**, so the
   plan's citations were corrected by an **additive** pointer table in §17.3;
   §3 itself is evidence and was not rewritten.
4. **`build_retry`'s address became an `Option<AddressBlock>`** because the
   tree's own Retry parser refuses a Retry with no Address block, so the
   honest answer for an inexpressible source is "no block", not "a wrong
   block".
5. **The responder reads the Session Request's block in a different
   transition** than the one that establishes the session, so the observation
   is carried as per-handshake state.

## Roadmap disposition

The **network-participation** roadmap gains this row:

| Plan | Subject | Status | Outcome |
| --- | --- | --- | --- |
| 529 | the router must learn the address at which a peer sees it | closed | passed-the-router-must-learn-the-address-at-which-a-peer-sees-it |

The roadmap's SSU2 introducer composition now reads, in order: Plan 490 gave
the router the role, Plan 523 corrected the **form**, Plan 524 delivered the
**acquisition**, Plan 525 the **publication**, Plan 526 the
**advertisement**, Plan 527 made the state those five depend on
**earnable**, Plan 528 made the **verdict** that earns it a fact, and Plan
529 gives that state a **fact** to be made of. The composition's own row in
the roadmap is unchanged, because the stock-peer half of every one of those
plans remains blocked on Plan 493's authorized two-host topology.

## Unblock audit

| Item | State |
| --- | --- |
| Plan 529 | **closed** — all ten acceptance criteria met, the floor green on the closing tree |
| The SSU2 introducer composition's stock-peer half | **blocked** on Plan 493's authorized two-host topology |
| The production SSU2 peer-test driver | **unbuilt** — the model, the recorder and the verdict path exist; the driver is the successor's subject |
| The requester side of the SSU2 relay protocol | **unwired** — the recorder exists, the producer does not; its owner is named |
| The runtime's direct form | **not yet earned** — this plan earns the class-1 corroboration from a real handshake; two more distinct classes are needed |
| Plans 433, 435-439, 493, 494 | **blocked** on external environments; untouched by this plan |

No blocked work was done under this plan's number.

## Handoff notes

- The plan's subject is the router's **own** external address: the address at
  which a peer sees it.
- The plan corrects the **value** of a block the tree already sent, and stops
  the tree from discarding the answer. It adds no wire format.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **requester side**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer half**
  all stay out of scope, with their owners named.
- The next free plan number is **530**.
