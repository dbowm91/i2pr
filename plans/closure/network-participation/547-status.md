# Plan 547 — an SSU2-only router must be able to prove its own reachability

**Closure record.** The plan of record is
`plans/implementation/network-participation/547-an-ssu2-only-router-must-be-able-to-prove-its-own-reachability.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 547 is **closed as blocked on one acceptance clause**, token
`blocked-t4-publication-target-bootstrap`.

Both gates the plan was registered to close are **passed, with executed
evidence**:

1. **The owner moved.** `PeerTestRuntime`
   (`crates/i2pr-daemon/src/peer_test_runtime.rs`) is built from
   `ssu2_config.peer_test` **alone** in the SSU2 service closure, so a router
   that runs only SSU2 owns a peer test. On `.20` — a host whose own startup
   log reports `service group lifecycle changed lifecycle="no_groups"`, i.e. no
   destination group exists at all — the owner probed, matched its echo in
   **40 ms**, recorded the result as this router's confirmed reachability, and
   the floodfill qualification gate that Plan 546's measurement never moved off
   `role="Disabled"` reached `role="Active"` **30.0 seconds later**.
2. **The re-dating path installs a dual-transport record.** It chooses the form
   by the record's own content and supplies the endpoint; the runtime's gate
   still compares the record's address against the supplied endpoint.

The acceptance clause the plan **did not** meet is the measurement it inherited
from Plan 546: *"`.20` then serves as a floodfill on the LAN with a peer's
publication confirmed by a `DeliveryStatus`."* `.20` reached
`role="Active"` — twice — so the first half was observed. The second half was
not, and could not be, for a reason **outside this plan's two gates**: the
publication's target selection reads the **publisher's** NetDB for a record
that advertises floodfill, and no host in this three-node network holds one.
Every host's out-of-band export carries `caps="4"`.

That is a source-attributed missing prerequisite, not a first-run fixture
error, so `blocked` is the honest disposition under `plans/README.md` §4. The
successor scope is named in **Findings** and **Unblock audit** below.

The measurement window is
`2026-11-13T14:49:15Z → 17:53Z` (~3 h 04 m), on the arm64 artifact built from
`d4647de1` (CI run `43446216811`, extracted SHA-256
`d19683b3b3cceb1a467de0dbb9bfd6ff1b1ec3c5678f025a5fe423116bf135ff`) and on the
local debug binary
`44042515eb059887c69f524ef0d5df68f7355ea0e63a6895518c23115b47c707`.

## What the plan closed

**The owner is beside the transport it measures.** The peer test is no longer a
property of the destination-group product. `PeerTestRuntime` holds the bounded
`PeerTestOwner`, the entropy source and the probe cadence; it rides the
daemon's **existing** `group_tick` and is fed by the daemon's own inbound path,
which is its single consumer. `ServiceProduct` owns no peer test at all — the
builder, the rng, the inbound handler, the delivery, the drive step, the target
selection, the counters cell, the four fields, the two construction sites, the
three constants and `ServiceProductOptions::peer_test` were all removed, so two
owners cannot exist.

**The re-dating path installs what it re-dates.** `refresh_local_router_info_record`
now takes `ntcp2_endpoint: Option<SocketAddr>` and `ntcp2_scope`, and chooses
the install form by the **record's own content**: a record naming an NTCP2
address goes to `install_scoped_dual_router_info` with the supplied endpoint;
a record naming none goes to `install_local_router_info`. The endpoint is
supplied by the caller and never inferred from the record — inferring it would
make the gate compare the record with itself.

**The peers can see it.** The peer test and the transport now report
themselves. `peer_test_no_peer_to_ask`, `peer_test_probe_started`,
`peer_test_probe_refused`, `peer_test_round_trip_completed` (with the measured
elapsed time), `peer_test_answer_sent`; and, on the transport, the typed
`terminate_reason` on both handshake paths, the `drop_category` on the inbound
path, and the typed `dial_outcome` at the dial seam. Counts, elapsed time and
stable transition labels only: no peer, no address, no key, no identifier, no
payload. This is what made the measurement possible at all — the earlier build
could not distinguish a probe that was refused from a probe that went out and
was never answered.

**The floor's own finding was fixed in code.** The 76-step floor caught, at
step 45, that naming the daemon-side seam `handle_inbound` put that word in
production `lib.rs`, where `scripts/check-m11-transit-boundaries.sh` rule 10
reserves it for the transit owner. The rule's needle is a bare word, so it
cannot tell the two types apart. The seam was renamed **`accept_inbound`** and
the checker is **byte-identical**: no boundary script was weakened to make the
change pass.

## What the plan did not close

- **T4 — a peer's publication confirmed by a `DeliveryStatus`.** Blocked at
  target selection. See Findings (high).
- **A sustained floodfill role.** `.20` held `role="Active"` for 29 m 30 s,
  withdrew, re-activated for 1 m 30 s, withdrew, and stayed `Disabled` for the
  remaining 2 h 45 m. See Findings (high).
- The peer test's own **contradiction** path has no production caller.
  `note_observed_external_address` is reachable from the inbound handshake's
  `Address` block and from `SessionEvent::AddressObserved`, so class 1 is
  producible — but the lane exercises the typed contradiction route explicitly
  rather than through a live producer that vetoes a publication.
- Stock-router admission (Plan 493), live-network NAT traversal, and the
  floodfill's serving behaviour against reference peers. Out of scope, and
  still out.

## Implementation commits

| Commit | What it is |
| --- | --- |
| `673adaea` | Registration: the plan of record, the registry row and the roadmap row |
| `0d7fc09c` | The owner moves beside the SSU2 service; the re-dating path installs the scoped form; Plan 521's guard retargeted |
| `a558d37f` | The lane (`ssu2_only_reachability`, 6 tests), the guard (`ssu2_only_reachability_boundary`, 16 tests) and the inventory surface |
| `a17efbd1` | The gates, the conformance section and the daemon deep-dive |
| `2c6f8bdb` | The arm64 artifact rebuilt for the moved owner |
| `736739fb` | The guard strengthened to pin the **exact** scoped install call with the supplied endpoint |
| `f430b453` | The seam renamed `accept_inbound` so production `lib.rs` leaves the transit checker's needle alone |
| `4139fc7d` | The peer test and the SSU2 transport report themselves |
| `d4647de1` | The arm64 artifact rebuilt for the reporting |

## Requirement-to-evidence matrix

| Plan §14 clause | Evidence | Status |
| --- | --- | --- |
| builds the peer-test owner from `ssu2.peer_test` | guard rule 2; lane test 1 (`the_opt_in_is_off_by_default_and_the_owner_then_does_not_exist`); live: `.20` probed with `peer_test = true` and stayed silent in the plan's control posture | **met** |
| a completed exchange makes the bound address publishable | lane test 2 (`a_completed_exchange_makes_the_bound_address_publishable`): before → `CandidateReachable` + publication refused; after → `confirmed_peer_test_count() == 1`, `Reachable`, publication accepted | **met** |
| a contradicted exchange does not | lane test 4 (`a_contradiction_vetoes_the_publication`): a mismatching observation drives `Firewalled` and publication is refused | **met** |
| a dual-transport record re-dates and installs | lane test 5 (`a_dual_transport_record_re_dates_and_installs`); lane test 6 (`a_record_whose_ntcp2_address_names_another_endpoint_is_still_refused`); guard rule 3 pins the exact call | **met** |
| `.20` serves as a floodfill on the LAN | live: `floodfill qualification activated requested=true role="Active"` at `14:54:36.188`, 30.0 s after the first matched round trip; it answered **38** peers' peer tests | **met** |
| a peer's publication confirmed by a `DeliveryStatus` | not observed. All three hosts attempted publication 3× each over 3 h; every outcome was `RefusedNoTarget`, 0 published | **not met — blocked** |

## Focused verification

| Check | Command | Result |
| --- | --- | --- |
| lane | `cargo test --locked -p i2pr-daemon --test ssu2_only_reachability -- --test-threads=1` | 6/6 pass |
| guard | `cargo test --locked -p i2pr-daemon --test ssu2_only_reachability_boundary -- --test-threads=1` | 16/16 pass |
| Plan 521 guard, retargeted | `cargo test --locked -p i2pr-daemon --test peer_test_role_boundary -- --test-threads=1` | 32/32 pass |
| transit boundary | `bash scripts/check-m11-transit-boundaries.sh` | passes: 8 rules, 9 controls |
| negative controls | `python3 /tmp/547-controls.py` | 11/11 fired; tree restored byte-identically (executed at `736739fb`; the only later change to the owner is the seam rename `f430b453`, which retargets the guards' needles in the same commit and is covered by those guards' own unit tests) |

The full routine floor, with Plan 544's, Plan 545's, Plan 546's and this plan's
own steps appended. Plan 546's two are included because **Plan 546's own floor
run had omitted them**; closing that gap is part of this plan's floor:

| # | Command | Outcome |
| --- | --- | --- |
| 1 | `cargo fmt --all --check` | PASS (4.0 s) |
| 2 | `cargo check --locked --workspace --all-targets` | PASS (27.8 s) |
| 3 | `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | PASS (0.3 s) |
| 4 | `cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS (2591.5 s) |
| 5 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS (25.5 s) |
| 6 | `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | PASS (10.0 s) |
| 7 | `cargo test --locked --workspace --doc` | PASS (5.0 s) |
| 8 | `bash scripts/check-dependency-direction.sh` | PASS (0.0 s) |
| 9 | `python3 scripts/check-global-plan-number-uniqueness.py` | PASS (0.1 s) |
| 10 | `python3 scripts/check-adr-number-uniqueness.py` | PASS (0.0 s) |
| 11 | `python3 scripts/check-portable-service-tunnel-api.py` | PASS (0.0 s) |
| 12 | `bash scripts/check-portable-service-tunnel-consumer.sh` | PASS (6.7 s) |
| 13 | `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS (6.4 s) |
| 14 | `bash scripts/check-runtime-boundaries.sh` | PASS (1.0 s) |
| 15 | `bash scripts/check-console-boundaries.sh` | PASS (0.3 s) |
| 16 | `bash scripts/check-console-browser-security.sh` | PASS (0.0 s) |
| 17 | `python3 scripts/check-tooling-inventory.py` | PASS (0.1 s) |
| 18 | `python3 scripts/check-license-metadata.py` | PASS (0.1 s) |
| 19 | `python3 scripts/check-managed-app-private-client-seams.py` | PASS (0.0 s) |
| 20 | `python3 scripts/check-router-readiness-contract.py` | PASS (0.0 s) |
| 21 | `python3 scripts/check-router-readiness-contract.py --self-test` | PASS (0.2 s) |
| 22 | `python3 scripts/check-core-router-recovery-contract.py` | PASS (0.0 s) |
| 23 | `python3 scripts/check-core-router-recovery-contract.py --self-test` | PASS (0.0 s) |
| 24 | `python3 scripts/plan449-ssu2-preflight.py --self-test` | PASS (0.0 s) |
| 25 | `python3 scripts/plan495-peer-eligibility.py --self-test` | PASS (0.1 s) |
| 26 | `python3 scripts/check-plan476-gate-binding.py` | PASS (0.3 s) |
| 27 | `python3 scripts/check-plan476-gate-binding.py --self-test` | PASS (0.6 s) |
| 28 | `python3 scripts/check-managed-app-package-boundary.py` | PASS (0.1 s) |
| 29 | `python3 scripts/check-managed-app-package-boundary.py --self-test` | PASS (0.1 s) |
| 30 | `python3 scripts/check-managed-app-policy-boundary.py` | PASS (0.1 s) |
| 31 | `python3 scripts/check-managed-app-policy-boundary.py --self-test` | PASS (0.1 s) |
| 32 | `python3 scripts/check-managed-app-secured-sandbox.py` | PASS (0.0 s) |
| 33 | `python3 scripts/check-managed-app-secured-sandbox.py --self-test` | PASS (0.0 s) |
| 34 | `bash scripts/check-service-tunnel-boundaries.sh` | PASS (0.1 s) |
| 35 | `python3 scripts/check-managed-app-gateway-boundary.py` | PASS (0.0 s) |
| 36 | `python3 scripts/check-managed-app-manager-boundary.py` | PASS (0.1 s) |
| 37 | `bash scripts/check-m11-per-epoch-composition.sh` | PASS (0.1 s) |
| 38 | `bash scripts/check-service-anonymity-boundaries.sh` | PASS (0.1 s) |
| 39 | `bash scripts/check-fixture-manifest.sh` | PASS (0.1 s) |
| 40 | `bash scripts/check-ntcp2-vectors.sh` | PASS (0.0 s) |
| 41 | `bash scripts/check-ssu2-vectors.sh` | PASS (0.0 s) |
| 42 | `bash scripts/check-i2cp-vectors.sh` | PASS (0.2 s) |
| 43 | `bash scripts/check-ntcp2-interoperability.sh` | PASS (0.0 s) |
| 44 | `bash scripts/check-constrained-host-lane-boundary.sh` | PASS (0.0 s) |
| 45 | `bash scripts/check-m11-transit-boundaries.sh` | PASS (2.6 s) |
| 46 | `bash scripts/check-m11-transit-qualification-evidence.sh` | PASS (1.3 s) |
| 47 | `bash scripts/check-sam-acceptance-evidence.sh` | PASS (0.1 s) |
| 48 | `bash scripts/check-ssu2-acceptance-evidence.sh` | PASS (0.0 s) |
| 49 | `bash scripts/check-i2cp-acceptance-evidence.sh` | PASS (0.1 s) |
| 50 | `bash scripts/check-i2pcontrol-acceptance-evidence.sh` | PASS (0.0 s) |
| 51 | `bash scripts/check-els2-type11-transcript-boundary.sh` | PASS (0.1 s) |
| 52 | `bash scripts/check-els2-live-lane-evidence.sh` | PASS (1.9 s) |
| 53 | `bash scripts/check-encrypted-service-consumer-caller.sh` | PASS (0.1 s) |
| 54 | `bash scripts/check-outproxy-request-path.sh` | PASS (0.1 s) |
| 55 | `bash scripts/check-outproxy-wire-lane-evidence.sh` | PASS (0.2 s) |
| 56 | `bash scripts/check-config-secret-hygiene.sh` | PASS (0.1 s) |
| 57 | `python3 scripts/check-workflow-validity.py` | PASS (0.1 s) |
| 58 | `bash scripts/check-floodfill-type5-serve.sh` | PASS (0.0 s) |
| 59 | `bash scripts/check-service-tunnel-acceptance-evidence.sh` | PASS (0.5 s) |
| 60 | `bash scripts/check-exploratory-tunnel-evidence.sh` | PASS (0.1 s) |
| 61 | `bash scripts/check-netdb-tunnel-evidence.sh` | PASS (0.1 s) |
| 62 | `bash scripts/check-destination-tunnel-evidence.sh` | PASS (0.2 s) |
| 63 | `bash scripts/check-streaming-tunnel-evidence.sh` | PASS (5.0 s) |
| 64 | `bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | PASS (2.6 s) |
| 65 | `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | PASS (0.1 s) |
| 66 | `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | PASS (0.6 s) |
| 67 | `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | PASS (0.1 s) |
| 68 | `cargo deny check advisories bans sources` | PASS (0.9 s) |
| 69 | `cargo test --locked -p i2pr-daemon --test router_info_export -- --test-threads=1` | PASS (0.4 s) |
| 70 | `cargo test --locked -p i2pr-daemon --test router_info_export_boundary -- --test-threads=1` | PASS (0.6 s) |
| 71 | `cargo test --locked -p i2pr-daemon --test peer_pool_fallback -- --test-threads=1` | PASS (3.1 s) |
| 72 | `cargo test --locked -p i2pr-daemon --test peer_pool_fallback_boundary -- --test-threads=1` | PASS (0.6 s) |
| 73 | `cargo test --locked -p i2pr-daemon --test publication_bound_address -- --test-threads=1` | PASS (0.2 s) |
| 74 | `cargo test --locked -p i2pr-daemon --test publication_bound_address_boundary -- --test-threads=1` | PASS (0.6 s) |
| 75 | `cargo test --locked -p i2pr-daemon --test ssu2_only_reachability -- --test-threads=1` | PASS (1.7 s) |
| 76 | `cargo test --locked -p i2pr-daemon --test ssu2_only_reachability_boundary -- --test-threads=1` | PASS (0.5 s) |

FLOOR PASS 76 / FAIL 0 / TOTAL 76

## The negative controls

Eleven controls, each a single-source mutation, each expected to fire the guard
for its own reason, with the tree restored byte-identically afterwards
(`git status` clean and every digest re-verified):

| Control | Mutation | Guard rule |
| --- | --- | --- |
| NC-547-1 | the owner sits behind the destination-group gate again | 1 |
| NC-547-2 | a second consumer of the seam | 1 |
| NC-547-3 | the product owns a peer test again | 1 |
| NC-547-4 | the re-dating path installs an endpoint other than the supplied one | 3 |
| NC-547-5 | the re-dating path uses the plain form for a dual-transport record | 3 |
| NC-547-6 | the plain install starts admitting NTCP2 | 3 |
| NC-547-7 | the daemon writes a reachability signal itself | 4 |
| NC-547-8 | the owner spawns a task | 6 |
| NC-547-9 | the inventory advertises the surface | 6 |
| NC-547-10 | a predecessor's lane moves | 6 |
| NC-547-11 | the opt-in defaults on | 2 |

**Two of these were wrong on the first run.** NC-547-3 and NC-547-4 fired
because their mutation broke compilation, so the test binary aborted rather
than the rule rejecting anything — a control that fires for the wrong reason
proves nothing. Both were rewritten so the tree compiles, and the guard was
strengthened at the same time (rule 3 now pins the exact call
`install_scoped_dual_router_info(fresh.clone(), now_ms, endpoint, ntcp2_scope)`).
The corrected run fires for the right reason. The defect is recorded here
rather than hidden, because it is the failure mode this repository's control
culture exists to catch.

## Acceptance criteria

Plan 547's §14 named five clauses and one measurement. Four clauses and the
measurement's first half are met with executed evidence; the measurement's
second half is blocked. The plan's three **stop conditions** were never
approached: there is one owner, the daemon writes no reachability, and the
install gate's NTCP2 test is supplied rather than skipped.

## Invariant review

- **One owner, one consumer.** The guard asserts `PeerTestRuntime::new(`
  exactly once, `peer_test.accept_inbound(` exactly once, `peer_test.drive(`
  exactly once, and that the drive rides the `group_tick` arm. The product is
  asserted to name none of `PeerTestOwner`, `note_confirmed_peer_test`,
  `peer_test_owner`, `drive_peer_test`, `handle_inbound_peer_test`.
- **The runtime is the only writer of reachability.** The owner contains no
  `ReachabilitySignal::`, no `ReachabilityTracker` and no
  `AddressFamily::Ipv4|Ipv6`; `note_confirmed_peer_test(` appears exactly once,
  in the confirmed-match branch of the seam, and the runtime derives the family
  from the session the exchange travelled on.
- **The install gate keeps its meaning.** The scoped form still asserts
  `scope.admits(ntcp2_endpoint.ip())` and `ntcp2_endpoint.port() < 1024`; the
  plain form still requires `ntcp2_addresses.is_empty()`.
- **Nothing new runs and nothing is advertised.** `ssu2.peer_test` still
  defaults to `false` and is still refused without `ssu2.enabled`; the owner
  contains no `tokio::spawn`, no `std::thread`, no `TcpListener` and no
  `sleep(`; `specs/support.toml` keeps the surface at `advertised = false`.
- **The predecessors are untouched.** `Cargo.toml` (`39c44e33…`),
  `Cargo.lock` (`f0cec068…`), Plans 544/545/546's lanes and guards are pinned by
  SHA-256 in the guard and are byte-identical.

## Failure, cancellation, restart, and contention review

- A peer test is an **outcome**, not an error: a refused probe, an expired
  probe and an unmatched echo are each counted under their own reason and none
  of them stops the router. The live run confirms it — `.20` reported
  `peer_test_no_peer_to_ask` twice at startup and continued.
- A router with no peer to ask generates **no** traffic: the attempt is refused
  and `PEER_TEST_RETRY_MS` (30 s) defers the next one. The report of that
  condition is itself rate-limited to once per `PEER_TEST_INTERVAL_MS` (5 min),
  so an idle router cannot fill its own log.
- Cancellation is the daemon's own: the owner holds no task to cancel, so the
  service's cancellation covers it by construction. A refused delivery is
  counted by the transport and never retried blindly.
- Contention: the counters cell is a `Mutex` whose poisoning is a **gap**, not
  a zero — the cell keeps its last published value rather than fabricating one.

## Migration and compatibility review

No configuration key was added. `ssu2.peer_test` keeps its name, its default
(`false`) and its refusal without `ssu2.enabled`. `router.version` stays
`0.9.62`, the inventory's advertised count stays **zero**, and `Cargo.toml` and
`Cargo.lock` are byte-identical, so no dependency moved. The one behavioural
change a reader must know is the rename `PeerTestRuntime::handle_inbound` →
`accept_inbound`: the method existed only on this branch, in this plan, and
has no external consumer.

## Security review

- The reports carry **counts, elapsed time and stable transition labels only**.
  No peer hash, no address, no key, no identifier and no payload appears in any
  new log line. This was checked against the log lines themselves during the
  measurement.
- No secret type gained `Debug`, `Display`, a `Clone` or a serialization path.
  No secret leaves its process: the floodfill's `caps=f` record is signed inside
  the router that owns the key.
- The peer test is **unauthenticated at the I2NP layer by design**, as the
  specification has it; the response travels only to the authenticated peer the
  request arrived from, over the router's existing delivery seam, and the
  answered-key table is a **ceiling** (`MAX_RESPONDED_PEER_TESTS`) rather than a
  target, so the reply traffic a peer can provoke is bounded by the router's
  own table and not by the peer's patience.
- A rootless, non-privileged process on a private LAN was used. No root, no
  namespace, no container, no VM and no public I2P traffic were involved.

## Limitations

- The measurement is a **three-node private LAN**, not a public network. It
  establishes that a router can prove its own reachability on a real network
  path with real handshakes and real I2P messages; it establishes nothing about
  admission by a public network.
- The floodfill flap's **exact class composition was not observable**. The
  tracker's supporting classes are not reported by any surface, so the
  mechanism below is inferred from source plus a period that matches
  `DEFAULT_OBSERVATION_TTL` (30 min). It is marked as inferred, not proven.
- `.8` failed 378 of 380 outbound dials while local succeeded 4 of 5, so the
  lab's only reliably working pair is local ↔ `.20`. That is enough for the
  peer test and enough for a publication, but it makes the `.8` results weaker
  evidence than `.20`'s.
- The stock-peer question (whether a reference router accepts these messages)
  remains Plan 493's, untouched.

## Findings by severity

### High — T4 is blocked by a publication-target bootstrap gap

`publish_once_inner` (`crates/i2pr-daemon/src/router_info_publication.rs`)
selects its targets with `publication_targets_excluding` →
`select_version_gated_floodfills`, so a publisher can only publish **to a
floodfill**. A floodfill is recognised by the `caps` option of the record in
the publisher's store. All three hosts hold `caps="4"` for each other:

```text
via-seed-.20  caps=.4;host=...
via-seed-.8   caps=.4;host=...
local-export  caps=.4;host=...
```

So `cycle_step` returns `NoTarget`, and every cycle ends `RefusedNoTarget`: 3
attempts per host over 3 h, 0 published, on all three hosts.

Three source facts make this a **deadlock** in an isolated network rather than
an operator mistake:

1. `.20`'s own publication is `RefusedNoTarget` too, so a floodfill cannot
   announce itself through the ordinary path;
2. the production router runs no RouterInfo-kind lookup, so a peer cannot fetch
   the record it does not hold;
3. the out-of-band export (`export_local_router_info`, reached from the NTCP2
   shared-record path at `crates/i2pr-daemon/src/lib.rs:5308`) is written **at
   startup**, before floodfill activation, and is never rewritten — `.20`'s
   export is 812 bytes dated `14:53`, while activation happened at `14:54:36`.

The missing prerequisite is a **production path that hands a peer the
floodfill's post-activation record**, or a bootstrap rule for the publication
target. Neither exists. This is new architectural scope and needs its own plan.

### High — the floodfill role does not sustain

```text
14:54:36.188  floodfill qualification activated requested=true role="Active"
15:24:06.194  floodfill advertisement withdrew requested=true role="Disabled"
15:24:36.201  floodfill qualification activated requested=true role="Active"
15:26:06.191  floodfill advertisement withdrew requested=true role="Disabled"
```

`Reachable` requires `supporting > min_corroboration`, i.e. **three** distinct
classes (`crates/i2pr-transport/src/reachability.rs`). Only two are refreshed by
traffic this lab actually produces:

- class 0 `LocalConfiguredBind` — recorded once, at bind;
- class 3 `PeerTestResult{Confirmed}` — refreshed every 5 min, and it worked:
  39 probes, 39 matched round trips on `.20`, RTT 3–43 ms.

Classes 1 `AuthenticatedPeerObservedExternalAddress` and 2 `ValidatedPath` are
produced only by an inbound **handshake** or an authenticated path response. On
an established session no further handshake happens, so both lapse at
`DEFAULT_OBSERVATION_TTL` (30 min) and the state falls to bind + peer test =
the corroboration floor = `CandidateReachable`, which the publication gate
refuses. The observed 29 m 30 s Active window matches that TTL.

**Inferred, not proven**: the tracker's class composition is not reported by
any surface, so the attribution rests on source plus the matching period. The
consequence is nonetheless measured: a floodfill whose peers hold long-lived
sessions cannot stay `Reachable`.

### Medium — the reachability snapshot has no observability

The peer test now reports itself, but the **tracker** does not. A reader cannot
see which classes are supporting, which are expiring, or when the next one
lapses. That is exactly the gap the peer test had before this plan, one layer
down, and it is why the finding above had to be inferred. The `ssu2` roadmap
should own a counts-only reachability snapshot (class occupancy, `expires_at`,
state) before the next floodfill measurement.

### Medium — outbound dials fail with `RetriesExhausted` on two of three hosts

| Host | dials established | dials failed | typed reason |
| --- | --- | --- | --- |
| local | 4 | 1 | — |
| `.20` | — | 102 | `terminate_reason=RetriesExhausted` (101) |
| `.8` | 2 | 378 | `terminate_reason=RetriesExhausted` (424) |

`RetriesExhausted` means the **peer never answered the SessionRequest**; it is
not a local admission refusal (`ResourceDenied`) and not a timeout of our own
making. Two candidate causes were tested:

- **Stale seeded records falsified.** `.8`'s SSU2 static and intro keys in its
  current export are byte-identical to the copy seeded in `.20`, so the SSU2
  transport identity is stable across restarts and the seeded records are not
  stale.
- **A host-local network cause remains the leading hypothesis, unproven.** `.8`
  (a Raspberry Pi also running pihole) fails outbound in both directions while
  `.20` answers peers' peer tests over the sessions that do exist, so the
  fault is not in the protocol path this plan changed.

This is now **reportable** where it previously was not, which is the whole
value of `4139fc7d`; establishing the cause is a successor's work.

### Low — the transit checker's needle is a bare word

`scripts/check-m11-transit-boundaries.sh` rule 10 matches the **word**
`handle_inbound` anywhere in production `lib.rs`. Its intent is
`TransitLiveOwner::handle_inbound`, and it cannot distinguish that from any
other type's method of the same name — which is why this plan's seam collided
with it. The collision was resolved in the **code** (the seam is
`accept_inbound`) and the checker is byte-identical, because tightening the
needle would have required rewriting the rule's own negative controls, whose
sample texts call `owner.handle_inbound(`. The over-broad needle is recorded
here as a checker gap for a successor; it is not a licence to reuse the word.

## Roadmap disposition

The **network-participation** roadmap's row for Plan 547 moves from
`registered` to `closed`, with the verdict
`blocked-t4-publication-target-bootstrap` and this record's path. The row's
prose is **not** rewritten: it records what the plan set out to do at
registration. Plan 546 stays `registered`; its own acceptance is not met either,
and its relaxation now has the corroboration this plan supplied but still no
publication to carry it.

The successor is **Plan 548**, registered by this closure, which owns the
publication-target bootstrap named in Findings (high).

The **ssu2** roadmap gains no row: this plan changes no transport *behaviour*.
`4139fc7d` adds reporting to `crates/i2pr-runtime/src/ssu2_runtime.rs`, which is
where a reader looking for the handshake reason should be sent, and the daemon
deep-dive carries the narrative.

## Unblock audit

| Plan | State after 547 | Why |
| --- | --- | --- |
| 546 | unchanged (`registered`) | its acceptance needs a correlated publication, which this record shows is blocked at target selection |
| 431 | unchanged (`stopped`) | keeps its own plan-of-record |
| 449 | unchanged (blocked) | needs the SSU2 external lane's independent peer |
| 472 | unchanged (blocked) | the same |
| 476 | unchanged (blocked) | the gate binding keeps its requirement |
| 493 | **advanced one gate** | a router now proves its own reachability on a live LAN path, so stock-router admission no longer waits on corroboration; it still needs the reference pins and the ban-surface care in `specs/QUALIFICATION-GATES.md` §3 |
| 433, 435–439, 441–448, 451, 452, 455, 457–462, 464–466, 471–474, 494 | unchanged (externally blocked) | they need a public network or an authorized external peer |

**Newly registered:** Plan 548 — a publication must reach a floodfill in a
network that holds no floodfill advertisement.

## Handoff notes

- **One owner.** Two consumers answer a probe twice and double the counters.
  The guard pins the single call site; do not add a second.
- **The daemon must not write reachability.** If a change needs it to, the
  change is wrong.
- **The endpoint is supplied, never inferred.** A path that derives the
  endpoint from the record it is installing makes the gate compare the record
  with itself.
- **A contradicted probe is a feature.** It is what stops a NAT'd host with the
  operator's key set from publishing an address peers cannot reach.
- **Do not reuse the word `handle_inbound` in production `lib.rs`.** The transit
  checker reserves it; the seam here is `accept_inbound` for that reason.
- **Before measuring a floodfill again, give the publishers a `caps=f`
  record.** Without it every publication is `RefusedNoTarget` and the
  measurement cannot reach a `DeliveryStatus`, no matter how well the peer test
  works.
- The predecessors' closure records are **not** rewritten. Plan 521's guard was
  retargeted and strengthened; its latent fail-open defect (`?` on an absent
  definition, which silently passed) was closed for every rule this plan
  touched.
