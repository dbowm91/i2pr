## Status

`passed-exploratory-build-driver-in-the-product-composition`

Plan 505 is closed. The router now **names a peer, builds and installs a
one-hop exploratory tunnel toward it, and records the outcome** — and the
peer-profile store is **fed** for the first time in a live router, so Plan
504's tunnel-liveness scheduler finally has a population to watch.

**The mechanism was complete; only its inputs were missing.** Plan 185
delivered the build coordinator, the one-hop install path and the liveness
scheduler, and proved them against a controlled mesh. What never existed was
a **peer**: `ExploratoryBuildCoordinator::submit` had **zero production
callers**, so the daemon installed no exploratory tunnels, the production
rebuild path was unreachable (no pair existed to be removed), and the
peer-profile store — constructed in production — was **never fed**.

**Plan 185's closure record needs no correction.** It delivered the mechanism
and never claimed the production daemon initiates exploratory builds, so this
is an **open gap**, not a false claim. The Plan 503/504 precedent (correct a
false claim in the successor's record) does not apply here, and the successor
says so rather than manufacturing a correction.

**The feasibility question was answered by running code.** Plan 505's first
work package was an executable artefact that could stop the plan. It drove a
controlled mesh of two real SSU2 daemons against the **real** coordinator,
the **real** peer-profile store and the **real** delivery seam. The answer
was **yes** — and the artefact is **not vacuous**, which was measured rather
than asserted (see below).

**Plan 497 limitation 5 is partially discharged.** The store is no longer
empty in a live router, and the router has a live **writer** of the four
observation signals. The store's other reader — the routing table's only
reader, Plan 433's never-constructed `NetDbTunnelCoordinator` — remains
blocked, so the limitation's "write side and read side must land together"
constraint is honoured in the sense that the store is neither empty nor
unread, but the routing-table reader is **not** discharged and this plan does
not pretend otherwise.

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised count
stays **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical.

## Commits

| Commit | Subject |
| --- | --- |
| `93cd0b82` | docs(plans): register Plan 505, the exploratory build driver in the product composition |
| `b2222746` | docs(plans): qualify Plan 505's citations and correct one line number |
| `d8eaede2` | docs(plans): correct Plan 505's outcome-to-observation table |
| `2ea35299` | feat(daemon): the exploratory build driver, and the WP1 feasibility gate |
| `774312f5` | feat(daemon): the Plan 505 exploratory build posture, off by default |
| `098d9b8e` | feat(daemon): wire the exploratory build driver into the live composition |
| `705a6ea7` | docs(plans): the Plan 505 conformance, deep-dive and support entry |
| `e9c0bdd8` | test(daemon): the Plan 505 driver guard and the live driver lane |
| `<closure>` | docs(plans): close Plan 505 and advance the network-participation roadmap |

## What changed

### The gap

Three facts, each measured rather than asserted:

1. **`ExploratoryBuildCoordinator::submit` had zero production callers.**
   Every call site was an out-of-crate integration lane:
   `tests/exploratory_build_live.rs`, `tests/streaming_tunnel_external.rs`,
   `tests/netdb_tunnel_external.rs`, `tests/destination_tunnel_external.rs`
   and `tests/service_tunnels_application_genuine_remote_qualification.rs`.
   The daemon called only `submit_destination` and
   `submit_destination_via_outbound_tunnel`, which route the build through an
   already-installed outbound tunnel.
2. **The production rebuild path was complete but unreachable.** The liveness
   rebuild — `remove_liveness_pair` → `coordinator.rebuild_exploratory(...)` —
   is the whole recovery mechanism, and it is reached only when a liveness
   pair is removed. No pair existed, so it never ran.
3. **The peer-profile store was constructed and never fed.** `learn` had one
   caller and it was inside `#[cfg(test)]`; `select_tunnel_build_peers` — Plan
   491's version-gated selection — had no production caller at all; the four
   `observe_*` calls had none either. The store was empty in a live router,
   and the router had no live reader of it.

Plan 504's closure record states the consequence plainly: the liveness
scheduler tracked **zero pairs** — *installed and inert*, not broken.

### The mechanism was complete; only its inputs were missing

`rebuild_exploratory` is already a complete first-build driver with the
peer-selection step removed: it allocates the three tunnel ids from the
coordinator's monotonic pool, mints two distinct message ids, builds the
`BridgeHeader::ShortTransport`, constructs the `BuildRequest` with the
direction-appropriate role and identity, and submits. `install_established`
already installs an exploratory build into the pool
(`register_outbound_with_material` / `register_inbound_with_material`), marks
it established, and activates it in the `DataPlaneRegistry`.

**One new thing was needed: a peer.**

### The feasibility answer, measured

Plan 505's first work package was an executable artefact that could stop the
plan. It drives a controlled mesh of two real SSU2 daemons against the real
coordinator, the real peer-profile store and the real delivery seam, and
asserts that the tunnel is **installed in the pool and the registry**, that
the **liveness scheduler sees the pair**, that the **profile store received
the observation**, and that the **counters moved**.

**The answer was yes.**

**The artefact is not vacuous, and that is measured.** The responder answers
with the peer's **identity** encryption key — `bundle.encryption_key()
.secret_bytes()` — which is what the production derivation uses. Swapping
that for the SSU2 static key, the shortcut the pre-existing
`tests/exploratory_build_live.rs` harness uses on **both** sides, makes the
build fail to install: the mutated tree fails with *"the outbound build must
install"*, and the real tree is green. Verified here, and the real tree was
never left modified.

### The peer selection is Plan 491's version-gated selection

The driver selects through `PeerParticipationDriver::select_tunnel_build_peers`
with `require_floodfill: false` — an exploratory tunnel is built toward an
**ordinary** peer, not necessarily a floodfill. The gate's three refusals
(unknown version, uncomparable version, below floor) are added to the
driver's counters **by reason**, so a below-floor peer is **refused and
counted**, never silently skipped.

The live lane proves this with a below-floor peer and an at-floor peer **in
the same store**: the at-floor peer is chosen, the below-floor refusal is
counted, the other two refusal counters are zero, and the below-floor peer's
profile is **never touched**.

### One build path, and the identity key

The driver submits through the **existing** `rebuild_exploratory` — the same
path the liveness rebuild uses — so there is exactly **one** code path that
builds an exploratory tunnel, and the first build and the rebuild cannot
drift apart.

The peer's static encryption key is the **RouterInfo identity public key**,
exactly as the production rebuild path uses it, and **not** the SSU2 address
`s` option that Plan 185's external lane found to be the wrong key. The peer
selection and the peer-material resolver read the **same** store, so they
can never disagree about which peer exists.

### The outcome map is total, and it separates a peer fault from a local condition

The map covers **all fourteen** `BuildCoordinatorOutcome` variants — the
variants are enumerated from the source, not counted from a hard-coded
number.

| Outcome | Observation |
| --- | --- |
| `Installed` | answered |
| `TimedOut` | unanswered |
| `HopRejected` | refused |
| `DeliveryFailed` | unreachable |
| `InvalidReply` | misbehaving |
| `Cancelled`, `PoolRejected`, `InvalidRegistration`, `InvalidPath`, `CoordinatorRejected` | **none** — this router's own condition |
| the four destination-scoped variants | never attributed to the driver |

**The plan's own table was wrong, and it was corrected before the code
followed it.** The plan mapped five outcomes to `observe_misbehaving`, but
`observe_misbehaving` means *the peer replied with data that does not match
the requested key*. Four of those five are **this router's** condition. The
correction is recorded in Plan 505's own plan correction (`d8eaede2`) and in
the driver; no earlier closure record was rewritten.

A local condition is **counted by the driver's own counters** and produces
**no peer-profile write. The router must never blame a peer for a fault of
its own** — writing a local failure into a peer's profile would degrade a
good peer's score, which is the peer-side equivalent of publishing a
peer-side refusal as an `i2pr` defect. The contrast — a local condition
writes nothing, a genuine peer fault **does** — is asserted in one test, so
the omission can never be a broken recorder.

An outcome with no in-flight attempt, a wrong direction, or a mismatched
attempt id is an **orphan**: counted, and credited to nothing.

### The peer-profile store is now fed in production

`NetDbParticipation::learn_bootstrap_peers` learns the router's **real**
validated peers from the bootstrap snapshot — the same population the routing
table is populated from, so the driver's selection and its resolver can never
disagree. The batch is the bootstrap's own bounded snapshot and is learned
with **one** receipt, so a large NetDB cannot churn the bounded receipt log.
The snapshot is taken under the bootstrap's lock while the batch is learned
outside it, so the two locks are never held at once.

A learned peer does **not** reset its restored observation state:
`ProfileStore::insert` only re-derives the version, the capacity class and
the floodfill flag for a peer it already knows, so the persisted projection's
observations survive the feed.

### The posture is off by default, for a concrete reason

A new `[exploratory]` configuration section carries one switch, `builds`,
following the `[transit] participation` precedent, and it is
`deny_unknown_fields`, so a key that looks like an advertisement is **refused**
rather than ignored.

**There is deliberately no count knob.** The number of exploratory pairs is
bounded by the coordinator's own pool and by Plan 504's `MAX_LIVENESS_PAIRS`.
A configuration key that could disagree with those ceilings would be a second
source of truth rather than a limit, so a `max_pairs` key is refused by the
parser and a test asserts that refusal.

**Off by default because the transport cannot dial the population.** The
daemon's SSU2 dial target is loopback-only and the private-LAN variant
accepts only RFC1918 IPv4, so a public deployment's peer population is **not
dialable at all**. A default-on driver would be a router that tries and fails.
Enabling the posture is an explicit operator decision that this router should
build exploratory tunnels inside the mesh it can actually reach.

### The live wiring

- `crates/i2pr-daemon/src/exploratory_driver.rs` — the driver, its counters,
  the peer-material resolver seam, and the total outcome map.
- `crates/i2pr-daemon/src/service_product.rs` — the product owns the driver;
  `drive_exploratory_builds` runs on the **existing** 250 ms group tick beside
  `drive_tunnel_liveness`; `observe_exploratory_outcome` records the terminal
  outcomes on **both** the expiry sweep and the inbound build-reply path.
- `crates/i2pr-daemon/src/lib.rs` — the composition builds the driver only
  when the operator has enabled the posture, feeds the peer-profile store, and
  publishes the counters.
- `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` — the counters are
  published through the existing inspection seam.
- `crates/i2pr-daemon/src/config.rs` — the `[exploratory]` section.

**No new task, no new timer, no new listener, no new socket, no new
dependency.** The driver runs on the tick that already existed, and when the
posture is off the product owns **no** driver and the tick's exploratory step
costs nothing.

## Requirement-to-evidence matrix

The guard's rule numbers are the **landed** numbering in
`crates/i2pr-daemon/tests/exploratory_driver_boundary.rs`.

| Requirement | Evidence |
| --- | --- |
| The composition builds the driver only on the operator posture, and the posture is off by default | `crates/i2pr-daemon/src/lib.rs`; `crates/i2pr-daemon/src/config.rs`; guard rule 1 |
| The product owns the driver and the spec carries it | `crates/i2pr-daemon/src/service_product.rs`; guard rule 2 |
| The live tick drives the driver, and the driver owns no task and no timer | `crates/i2pr-daemon/src/service_product.rs`; guard rule 3 |
| The driver selects through Plan 491's version-gated selection, never through an ungated path | `crates/i2pr-daemon/src/exploratory_driver.rs`; guard rule 4 |
| The driver submits through the single exploratory build path | `crates/i2pr-daemon/src/exploratory_driver.rs`; guard rule 5 |
| The outcome mapping is total over every variant, and a local condition is distinct from a peer fault | `crates/i2pr-daemon/src/exploratory_driver.rs`; guard rule 6 |
| The driver never touches a destination tunnel | `crates/i2pr-daemon/src/exploratory_driver.rs`; guard rule 7 |
| The peer-profile store is fed in production | `crates/i2pr-daemon/src/lib.rs`; guard rule 8 |
| The terminal outcomes reach the driver on **both** paths | `crates/i2pr-daemon/src/service_product.rs`; guard rule 9 |
| The counters are published, and the seam reports a gap rather than a zero | `crates/i2pr-daemon/src/i2pcontrol_inspection.rs`; guard rule 10 |
| The feasibility question is answered by running code | `crates/i2pr-daemon/src/exploratory_driver.rs` — the in-module feasibility artefact |
| The feasibility artefact is not vacuous | the identity-key / SSU2-key negative control, verified on the real tree |
| The below-floor peer is refused and counted by reason | `crates/i2pr-daemon/tests/exploratory_driver_live.rs` |
| The tunnel is installed in the pool and the registry | `crates/i2pr-daemon/tests/exploratory_driver_live.rs` |
| A local condition writes no peer profile; a peer fault does | `crates/i2pr-daemon/tests/exploratory_driver_live.rs`; `crates/i2pr-daemon/src/exploratory_driver.rs` |
| An orphan is counted and credited to nothing | `crates/i2pr-daemon/tests/exploratory_driver_live.rs` |
| Nothing is advertised | `specs/support.toml` — 97 surfaces, 0 advertised |
| No new dependency | `git diff --exit-code <base> HEAD -- Cargo.toml Cargo.lock` |

## Commands run

### The routine floor

108 steps, run on the closing tree. See the table below.

| # | Step | Result | Seconds |
| --- | --- | --- | --- |
| 1 | `cargo-fmt` | PASS | 3 |
| 2 | `cargo-check` | PASS | 5 |
| 3 | `cargo-build-siblings` | PASS | 1 |
| 4 | `cargo-build-static-fixture` | PASS | 0 |
| 5 | `cargo-copy-static-fixture` | PASS | 0 |
| 6 | `cargo-rebuild-plain-fixture` | PASS | 0 |
| 7 | `cargo-test-workspace` | PASS | 1222 |
| 8 | `cargo-clippy` | PASS | 11 |
| 9 | `cargo-doc` | PASS | 8 |
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
| 28 | `plan495-peer-eligibility-self` | PASS | 0 |
| 29 | `plan495-peer-eligibility-profile-drift` | PASS | 0 |
| 30 | `plan476-gate-binding` | PASS | 1 |
| 31 | `plan476-gate-binding-self` | PASS | 0 |
| 32 | `check-managed-app-package-boundary` | PASS | 0 |
| 33 | `check-managed-app-package-boundary-self` | PASS | 0 |
| 34 | `check-managed-app-policy-boundary` | PASS | 1 |
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
| 46 | `check-i2cp-vectors` | PASS | 0 |
| 47 | `check-ntcp2-interoperability` | PASS | 1 |
| 48 | `check-constrained-host-lane-boundary` | PASS | 0 |
| 49 | `check-m11-transit-boundaries` | PASS | 2 |
| 50 | `check-m11-transit-qualification-evidence` | PASS | 1 |
| 51 | `check-sam-acceptance-evidence` | PASS | 0 |
| 52 | `check-ssu2-acceptance-evidence` | PASS | 0 |
| 53 | `check-i2cp-acceptance-evidence` | PASS | 0 |
| 54 | `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| 55 | `check-els2-type11-transcript-boundary` | PASS | 0 |
| 56 | `check-els2-live-lane-evidence` | PASS | 2 |
| 57 | `check-encrypted-service-consumer-caller` | PASS | 0 |
| 58 | `check-outproxy-request-path` | PASS | 0 |
| 59 | `check-outproxy-wire-lane-evidence` | PASS | 0 |
| 60 | `check-config-secret-hygiene` | PASS | 1 |
| 61 | `check-workflow-validity` | PASS | 0 |
| 62 | `check-floodfill-type5-serve` | PASS | 0 |
| 63 | `check-service-tunnel-acceptance-evidence` | PASS | 0 |
| 64 | `check-exploratory-tunnel-evidence` | PASS | 0 |
| 65 | `check-netdb-tunnel-evidence` | PASS | 0 |
| 66 | `check-destination-tunnel-evidence` | PASS | 1 |
| 67 | `check-streaming-tunnel-evidence` | PASS | 5 |
| 68 | `check-m6-mixed-router-acceptance-evidence` | PASS | 4 |
| 69 | `check-m12-floodfill-qualification-evidence-self` | PASS | 0 |
| 70 | `check-m12-floodfill-boundaries-self` | PASS | 1 |
| 71 | `ntcp2-harness-tests` | PASS | 0 |
| 72 | `plan488-mesh-lane` | PASS | 0 |
| 73 | `plan488-evidence-deterministic` | PASS | 0 |
| 74 | `plan489-mesh-lane` | PASS | 1 |
| 75 | `plan489-evidence-deterministic` | PASS | 0 |
| 76 | `plan490-mesh-lane` | PASS | 0 |
| 77 | `plan490-evidence-deterministic` | PASS | 0 |
| 78 | `plan490-ssu2-transport-suite` | PASS | 6 |
| 79 | `plan490-transport-boundary-guard` | PASS | 0 |
| 80 | `plan491-mesh-lane` | PASS | 1 |
| 81 | `plan491-evidence-deterministic` | PASS | 0 |
| 82 | `plan491-boundary-guard` | PASS | 0 |
| 83 | `plan491-proto-suite` | PASS | 0 |
| 84 | `plan491-netdb-suite` | PASS | 87 |
| 85 | `plan491-client-suite` | PASS | 25 |
| 86 | `plan492-posture-suite` | PASS | 0 |
| 87 | `plan492-m11-lane` | PASS | 5 |
| 88 | `plan502-transit-composition-lane` | PASS | 0 |
| 89 | `plan502-transit-composition-unit` | PASS | 0 |
| 90 | `plan502-transit-boundary-self` | PASS | 0 |
| 91 | `plan496-private-lan-profile` | PASS | 0 |
| 92 | `plan496-ntcp2-activation-gate` | PASS | 0 |
| 93 | `plan497-routing-consumer-lane` | PASS | 1 |
| 94 | `plan497-netdb-preferred-order` | PASS | 1 |
| 95 | `plan497-daemon-participation-unit` | PASS | 2 |
| 96 | `plan498-introducer-handoff-lane` | PASS | 1 |
| 97 | `plan498-introducer-mesh-lane` | PASS | 0 |
| 98 | `plan498-bootstrap-resolver-row` | PASS | 25 |
| 99 | `plan498-runtime-relay-accessor` | PASS | 5 |
| 100 | `plan503-version-eligibility-live` | PASS | 1 |
| 101 | `plan503-version-eligibility-unit` | PASS | 0 |
| 102 | `plan504-liveness-boundary` | PASS | 0 |
| 103 | `plan504-liveness-live` | PASS | 0 |
| 104 | `plan504-liveness-unit` | PASS | 1 |
| 105 | `plan505-driver-boundary` | PASS | 0 |
| 106 | `plan505-driver-live` | PASS | 2 |
| 107 | `plan505-driver-unit` | PASS | 1 |
| 108 | `cargo-deny` | PASS | 1 |

**108 steps, 108 PASS, 0 FAIL.**


### The focused verification

| Command | Result |
| --- | --- |
| `cargo test --locked -p i2pr-daemon --test exploratory_driver_boundary -- --test-threads=1` | 15 passed |
| `cargo test --locked -p i2pr-daemon --test exploratory_driver_live -- --test-threads=1` | 8 passed |
| `cargo test --locked -p i2pr-daemon --lib exploratory_driver -- --test-threads=1` | 6 passed |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` | 957 passed, 3 ignored |
| `cargo test --locked -p i2pr-daemon --test version_floor_boundary -- --test-threads=1` | 28 passed |
| `cargo test --locked -p i2pr-daemon --test version_eligibility_live -- --test-threads=1` | 11 passed |
| `cargo test --locked -p i2pr-daemon --test tunnel_liveness_boundary -- --test-threads=1` | 26 passed |
| `cargo test --locked -p i2pr-daemon --test tunnel_liveness_live -- --test-threads=1` | 9 passed |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1` | 11 passed |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --all --check` | clean |
| `git diff --exit-code <base> HEAD -- Cargo.toml Cargo.lock crates/i2pr-daemon/Cargo.toml` | byte-identical |

### The two negative controls, verified on the real tree

| Mutation | Result |
| --- | --- |
| `InvalidPath` dropped from the outcome mapping behind a catch-all arm | guard rule 6 fails: *"the outcome mapping must handle every `BuildCoordinatorOutcome` variant"* |
| the observation call removed from the expiry sweep | guard rule 9 fails: *"both the expiry sweep and the inbound build reply must report to the driver"* |
| the responder's identity key replaced by the SSU2 static key | the feasibility artefact fails: *"the outbound build must install"* |
| the local-condition guard removed from the recorder | the peer-profile test fails: *"a local condition must not write an observation receipt"* |

Every mutation was reverted and the tree re-verified green.

## Invariant review

1. **The declared version is not edited** and no floor is moved.
2. **Fail closed and bounded.** Every refusal is counted by reason; the
   driver's own state is bounded (one in-flight attempt, a bounded counter
   set); the coordinator's pending table and the liveness scheduler's own
   ceilings bound the rest.
3. **No new task, no new timer, no new listener, no new socket.** The driver
   runs on the existing tick; the guard asserts the module contains no
   `tokio::spawn` and no `tokio::time::interval`.
4. **No unbounded structure.** Nothing in this plan adds a growing
   collection. The peer-profile store's population is bounded by the
   bootstrap's own bounded snapshot and by the store's own ceilings.
5. **No new dependency.** `Cargo.toml` and `Cargo.lock` stay byte-identical.
6. **No advertisement, no wire change, no new option, no new capability.**
7. **One exploratory build path.** The driver submits through the existing
   `rebuild_exploratory`; the module contains no second, private build path.
8. **A destination tunnel is never touched.** The driver is exploratory-only.
9. **A verdict may only be acted on when the signal is distinguishable from
   its opposite.** Plan 504's proven-pair gate is unchanged.
10. **The correlation must be authenticated.** Unchanged from Plan 504.
11. **A local condition is never written into a peer's profile.** The
    router's own faults are counted by the driver's counters and never
    degrade a peer's profile score.
12. **The failure path is bounded and typed.** A failed build is counted, not
    retried in a loop: one attempt per tick, and the coordinator's own
    deadline sweep ends it.
13. **No advertisement.** `router.version` stays `0.9.62`; `caps` and the
    published RouterInfo are unchanged; the support inventory's advertised
    count stays zero.

## Failure review

- **Failure.** A refused selection, an unresolvable peer, an unreachable
  peer, a refused delivery and a timed-out build are each counted by reason.
  The driver never retries inside one tick and never spins: it is driven
  from the tick, so a failure costs one tick.
- **Cancellation.** The driver holds no task and no timer, so there is
  nothing to cancel inside it. A cancelled pump leaves the coordinator's
  pending table as it is; the coordinator's own deadline sweep ends the
  attempt and the observation follows from that sweep.
- **Restart.** Nothing is persisted by the driver. The peer-profile store's
  own projection (Plan 488 F4) persists the *observations*, not the driver's
  in-flight record.
- **Contention.** The driver holds the coordinator mutably for the duration
  of one attempt and the peer-profile handle under its own lock for the
  duration of one selection. The two locks are never held at once: the
  bootstrap snapshot is taken under the bootstrap's lock while the batch is
  learned outside it.
- **Interaction with Plan 504's rebuild path.** When the liveness scheduler
  tears a dead pair down, Plan 504's rebuild asks the coordinator to rebuild
  toward the same peer. The coordinator's pending table is then non-empty, so
  the driver's own `pending_busy` check refuses to submit a duplicate on the
  next tick. The two mechanisms do not race.

## Migration review

None. No wire format changed, no `router.version` changed, no option
changed, no configuration key is required (the new section has a default),
nothing is persisted by the driver.

The **behavioural** consequence is stated plainly: with the posture enabled,
the router names one peer through the version-gated selection, builds a
one-hop exploratory tunnel toward it, builds the matching inbound tunnel
toward the **same** peer, and watches the pair with Plan 504's liveness
scheduler.

## Security review

- The probe carries **no peer identity** beyond the delivery instruction the
  tunnel already requires, and no key material.
- The peer's **identity public key** is used for the build's encryption, and
  it is resolved from the **authoritative** validated store — never from the
  SSU2 address `s` option, which Plan 185's external lane showed to be the
  wrong key.
- The peer selection is **version-gated**: a below-floor, unknown-version or
  uncomparable-version peer is refused and counted, never built toward.
- The driver only ever builds **exploratory** tunnels. It never touches a
  destination tunnel, so it cannot interfere with the router's service
  destinations.
- The **local-condition carve-out** narrows what is written into a peer's
  profile: only a genuine peer fault degrades a peer's score.
- No new configuration knob beyond the one switch; no new listener; no new
  socket; no new dependency; the counters are published through the existing
  loopback-only inspection seam and carry counts and stable reason labels
  only — no peer, address, key or payload.

## Documentation and operations

- `docs/architecture/i2pr-daemon.md` — the Plan 505 deep-dive section, the
  `[exploratory]` configuration reference, and a module-table row for
  `src/exploratory_driver.rs`.
- `specs/CONFORMANCE.md` — the Plan 505 section, including the explicit
  statement that **no stock-peer evidence** exists, and what the plan does
  **not** close.
- `specs/support.toml` — one `experimental` surface with `advertised = false`;
  Plan 504's now-stale forward-reference is updated.
- `plans/registry.md`, `plans/subsystems/network-participation-roadmap.md` —
  the Plan 505 rows and the Plan 497 limitation 5 disposition.
- The current-fact surface tallies move from 96/90 to 97/91 in the registry,
  `specs/CONFORMANCE.md` and the daemon deep-dive. The **closed** closure
  records keep their own historical tallies.

## Limitations

- **The driver's peer population is the bootstrap snapshot.** The validated
  store is still **not refreshed from live NetDB lookups**: RouterInfos
  learned from a live lookup are not inserted into it. The driver therefore
  names peers the router knew at startup. That is a **bounded, known**
  population, not an unbounded one, and refreshing the store is a separate
  plan-of-record because it changes what the router stores.
- **The default posture is off**, so a router that has never set
  `[exploratory] builds = true` owns no driver and builds nothing. That is
  the designed behaviour, and it is also why the "the liveness scheduler
  tracks a non-zero pair count" criterion is a **controlled-mesh** result,
  not a default-deployment one.
- **The transport's dial scope bounds the driver to the mesh it can reach.**
  `Ssu2DialTarget::new` is loopback-only and `new_private_lan` accepts only
  RFC1918 IPv4. A public deployment's peers are not dialable, so the driver
  would refuse them. This plan does **not** widen the dial scope; that would
  be a transport-boundary change with its own plan-of-record.
- **No stock-peer evidence.** Whether an ordinary public router accepts a
  driver-initiated one-hop exploratory build, and whether it relays a liveness
  probe back, is **unproven**. The external lanes stay open until Plan 493's
  authorized topology is available.
- **Plan 497 limitation 5's read half is open.** The driver is a live reader
  of the peer-profile store, but the routing table's only reader remains Plan
  433's never-constructed `NetDbTunnelCoordinator`.
- **Plan 487 F3's destination-inbound half is open.** It needs tunnel
  rekeying, which the tree does not implement at all — no `rekey` symbol in
  `i2pr-proto`, `i2pr-tunnel` or `i2pr-client`, and `TunnelDataMessage` has no
  rekey payload. That is a new I2NP mechanism with its own plan-of-record.
- **The receipts are not persisted by the driver.** A restart empties the
  in-flight record; the profile store's own projection persists the
  observations.

## Findings by severity

### Medium

- **The driver is inert by default.** The posture is off, so the mechanism
  exists but does not run unless an operator enables it. This is the
  designed posture, and the reason is concrete — the transport cannot dial a
  public deployment's peers — but it means the *observable* capability is
  bounded by the operator's choice and by the controlled mesh.
- **The peer population is the bootstrap snapshot.** The driver can only
  name peers the router knew at startup, because the validated store is not
  refreshed from live NetDB lookups.
- **No stock-peer evidence of the whole loop.** A controlled mesh proves the
  mechanism; it does not prove that an ordinary public router accepts a
  driver-initiated exploratory build, nor that it relays a liveness probe
  back.

### Low

- **The plan's own outcome table was wrong.** It mapped five outcomes to
  `observe_misbehaving`, four of which are the router's own condition. The
  correction landed as a plan correction **before** the code followed it,
  and the driver now distinguishes the two. Recorded rather than fixed in
  place, because the plan-of-record is the artifact that carried the error.
- **The guard's rule 6 enumerates the outcome variants from the source
  rather than from a hard-coded list.** The enumeration is a small parser,
  and it is coupled to the shape of `exploratory_build.rs`'s enum: a
  reformatting of that enum would make the rule fail closed with a clear
  message rather than silently pass.
- **The guard's rule 9 carries three probes, one per path.** Each proves its
  own path specifically, so a rule that accepted either path would not pass.
  The cost is three mutations per run instead of one.
- **The guard's weakened-copy probes are coupled to the landed source
  text.** Each probe replaces an exact string from the real source and
  asserts the replacement applied. A future refactor that reformats those
  strings makes the probe fail loudly with *"the … mutation must have
  applied"* — fail-closed and explicit, but a coupling a reviewer should
  know about.
- **The driver's per-tick cost when the posture is on** is one
  `ChaCha8Rng::try_from_os_rng()` and one delivery-handle clone per tick,
  because the driver's own refusals live behind its `drive()` call. When the
  posture is off the product owns no driver and the step costs nothing. The
  cost is bounded and small, but it is a per-tick cost that the disabled
  case avoids entirely.
- **`MAX_LIVENESS_PAIRS` (8) is smaller than `MAX_PENDING_LIVENESS_TESTS`
  (16).** Carried over from Plan 504 and unchanged here: the pending
  ceiling is a backstop the pair ceiling already bounds.

### Recorded, not defects

- The daemon deep-dive's per-module unit-test distribution is stale
  (pre-existing).
- The daemon deep-dive carries four bare-filename `file:line` citations a
  link checker cannot resolve (pre-existing).

## Plan 497 limitation 5, and the other open items

| Item | Disposition |
| --- | --- |
| **Plan 497 limitation 5 — the peer-profile store is empty in a live daemon** | **Partially discharged.** The store is now **fed** from the daemon's own NetDB, and the driver is a live **writer** of the four observation signals and a live **reader** for its selection. The limitation's "write side and read side must land together" constraint is honoured in the sense that the store is neither empty nor unread. The **routing table's** only reader remains Plan 433's never-constructed `NetDbTunnelCoordinator`, so that half stays open. |
| **Plan 487 F3 — the destination's inbound pool has no transit-hop failure detection** | **Open.** The creator side now has a live, authenticated liveness mechanism with a proven-pair gate and a population. The **destination-inbound** half needs **tunnel rekeying**, which the tree does not implement at all. |
| **Plan 487 F2** | Open — Plan 485's recorded session policy. |
| **Plan 491 F1, F3** | Discharged by Plan 503. |
| **Plan 491 F2, F4** | Open — external evidence and the I2PControl line. |
| **Plan 433 (the NetDB / destination-tunnel coordinator composition)** | Still `blocked` on the external acquisition contract. Untouched. |
| **Plan 436 WP3** | Already discharged by Plan 502. Plan 436 stays `blocked`. |
| **Plan 437, 438, 439, 493, 494** | Unchanged; all `blocked` on external environments. |
| **The validated store's refresh from live NetDB lookups** | **New, open.** Needs a plan-of-record: it changes what the router stores. |
| **The stock-peer loop** | **New, open.** An external-lane question. |

## Roadmap disposition

The `network-participation` roadmap's Plan 505 row moves to `closed`, and its
§12 status sentence names Plan 505 as closed.

**The next candidate on this line is the validated store's refresh from live
NetDB lookups.** It is the population the driver draws from, and it is what
makes three other things live at once: the driver's peer selection, the
liveness scheduler's verdicts, and the peer-profile store's own maintenance.
It needs its own plan-of-record, because it changes what the router stores.

Other open source-side candidates, unchanged by this plan: Plan 487 F2 and
F3's destination-inbound half, Plan 497 limitation 5's read half, Plan 491
F2 and F4, Plan 490 Medium #3 (the stock-peer introducer lane), Plan 502 M1,
and Plan 503's lookup-target API-version gate.

## Unblock audit

| Item | State after Plan 505 |
| --- | --- |
| Plan 185's exploratory build driver | **Live**, behind the operator's default-off `[exploratory]` posture. |
| Plan 185's liveness seam | **Live** (Plan 504). |
| Plan 504's scheduler population | **Live.** The driver is the population; with the posture enabled in the controlled mesh the scheduler tracks a non-zero pair count. |
| Plan 497 limitation 5 | **Partially discharged** — the store is fed, written and read; the routing table's reader stays blocked. |
| Plan 487 F3 | **Open** — the destination-inbound half needs tunnel rekeying. |
| Plan 487 F2 | Open — Plan 485's recorded session policy. |
| Plan 491 F1, F3 | Discharged by Plan 503. |
| Plan 491 F2, F4 | Open. |
| Plan 433 | Still `blocked`. Untouched. |
| Plan 436 WP3 | Already discharged by Plan 502. Plan 436 stays `blocked`. |
| Plan 437, 438, 439, 493, 494 | Unchanged; all `blocked`. |
| The validated store's refresh | **New, open.** Needs a plan-of-record. |
| The stock-peer loop | **New, open.** An external-lane question. |
