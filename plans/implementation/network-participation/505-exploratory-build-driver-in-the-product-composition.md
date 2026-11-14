# Plan 505 — The exploratory build driver in the product composition

Status: **registered** · Registered: 2026-10-25 · Subsystem: `network-participation`

Predecessors: Plan 185 (the one-hop exploratory tunnels and the liveness
scheduler), Plan 186 (the NetDB lookup / LeaseSet2 publication), Plan 488
(the peer-profile store), Plan 491 (the API-version floors and peer
eligibility), Plan 504 (the liveness scheduler in the product composition).

## 1. Objective

Install an **exploratory build driver** in the production composition: the
router names a peer, builds and installs a one-hop exploratory tunnel toward
it, and records the outcome in the peer-profile store.

This closes the loop Plan 504 left open. Plan 504 installed the tunnel-liveness
scheduler and wired it to the live inbound seam, but the daemon installs **no
exploratory tunnels**, so the scheduler tracks **zero pairs** — installed and
inert. The driver is the population the scheduler watches.

It also lands the **write side** of the peer-profile store. Plan 488 built the
store; nothing in the live daemon ever calls `observe_answered`,
`observe_refused`, `observe_unanswered` or `observe_unreachable`, and nothing
ever calls `learn`, so the store is empty in a live router. The driver is the
component that produces exactly those four signals.

## 2. Why ready

The mechanism is **complete**; only its inputs are missing.

- Plan 185 delivered the build coordinator, the one-hop install path and the
  liveness scheduler, and proved them against a controlled mesh.
- Plan 504 installed the liveness scheduler in the live product, on the
  **existing** tick, with an **authenticated** correlation and a proven-pair
  gate.
- `ExploratoryBuildCoordinator::rebuild_exploratory`
  (`crates/i2pr-daemon/src/exploratory_build.rs:2679`) is already a **complete
  first-build driver with the peer-selection step removed**: it allocates the
  three tunnel ids from the coordinator's monotonic pool, mints two distinct
  message ids, builds the `BridgeHeader::ShortTransport`, constructs the
  `BuildRequest` with the direction-appropriate role and identity, and calls
  `submit`.
- `ExploratoryBuildCoordinator::install_established`
  (`crates/i2pr-daemon/src/exploratory_build.rs:2469`) already installs an
  exploratory build into the pool (`register_outbound_with_material` /
  `register_inbound_with_material`), marks it established, activates it, and
  activates it in the `DataPlaneRegistry`.
- The peer-profile store, its version-gated tunnel-build selection
  (`select_tunnel_build_peers`, `crates/i2pr-daemon/src/netdb_participation.rs:384`)
  and its four observation calls already exist.

**One new thing is needed: a peer.** Nothing in the live composition names a
peer to build a **first** exploratory tunnel with.

## 3. Current implementation evidence

### 3.1 The coordinator's exploratory `submit` has zero production callers

`ExploratoryBuildCoordinator::submit`
(`crates/i2pr-daemon/src/exploratory_build.rs:1599`) is production-reachable
code with **no production caller**. Every call site is an out-of-crate
integration lane:

```text
crates/i2pr-daemon/tests/exploratory_build_live.rs:492, 584, 1156, 1259
crates/i2pr-daemon/tests/streaming_tunnel_external.rs:1190, 1218
crates/i2pr-daemon/tests/netdb_tunnel_external.rs:279, 315
crates/i2pr-daemon/tests/destination_tunnel_external.rs:417, 445
crates/i2pr-daemon/tests/service_tunnels_application_genuine_remote_qualification.rs:950
```

The daemon calls only `submit_destination` and
`submit_destination_via_outbound_tunnel`
(`crates/i2pr-daemon/src/service_product.rs:7243`, `:7510`, `:7741`, `:7750`),
which route the build through an already-installed outbound tunnel.

### 3.2 The rebuild path is production, but unreachable

`crates/i2pr-daemon/src/service_product.rs:3337` `remove_liveness_pair` calls
`coordinator.rebuild_exploratory(...)` at `:3376`. That is the **production
rebuild** path, and it is complete: it recovers the peer hash from the pool
slot, resolves the peer's public static encryption key from the authoritative
store (`peer_static_encryption_key`, `:3398`), obtains the delivery service,
and submits.

It is unreachable, because it is reached only when a liveness pair is removed,
and **no liveness pair exists** — the daemon installs no exploratory tunnels.
So the whole exploratory half of the router is a loop with no entry point.

### 3.3 The peer-profile store is constructed but never fed

`NetDbParticipation::new` (`crates/i2pr-daemon/src/lib.rs:1872`) builds the
`PeerParticipationHandle` at `:1902` and the daemon holds it at `:1853`.

- `learn` (`crates/i2pr-daemon/src/netdb_participation.rs:262`) has **one**
  caller in the whole crate, and it is inside `#[cfg(test)]`
  (`crates/i2pr-daemon/src/lib.rs:5911`).
- `observe_answered` / `observe_refused` / `observe_unanswered` /
  `observe_unreachable` (`crates/i2pr-daemon/src/netdb_participation.rs:296`, `:308`, `:319`, `:324`)
  have **no** production caller.
- `select_tunnel_build_peers` (`crates/i2pr-daemon/src/netdb_participation.rs:384`) has **no**
  production caller. It is version-gated (`require_floodfill: false`) and is
  exactly the read side a driver needs.

The store is therefore **empty in a live router**, and the router has no live
reader of it either.

### 3.4 The reachable peer universe

The validated RouterInfo store the driver would draw from is
`DestinationTunnelCoordinator::store()`
(`crates/i2pr-daemon/src/destination_tunnels.rs:690`). It is seeded **once** at
startup from the bootstrap snapshot (`crates/i2pr-daemon/src/lib.rs:3940`) and
thereafter only by `bootstrap_reference_router_info`
(`crates/i2pr-daemon/src/service_product.rs:5101`). RouterInfos learned from
live NetDB lookups are **not** inserted into it.

That is a bounded, known population — the bootstrap snapshot, capped by
`config.netdb.max_records`. It is a **scope** input, not an implementation
detail: Plan 505's driver draws from the validated store, and the store's
refresh policy is a separate concern.

### 3.5 The transport's dial scope is loopback and private-LAN only

`Ssu2DialTarget::new` (`crates/i2pr-runtime/src/ssu2_runtime.rs:647`) rejects
any non-loopback address (`NotLoopback`, `:660`).
`Ssu2DialTarget::new_private_lan` (`:679`) accepts only RFC1918 IPv4 literals
(`NotPrivateLanIpv4`, `:692`). `daemon_dial_target_for_profile`
(`crates/i2pr-daemon/src/router_i2np.rs:2342`) chooses between
`ControlledLoopback` and `PrivateLan` and nothing else.

`RouterDeliveryService::deliver_with_ticket`
(`crates/i2pr-daemon/src/router_i2np.rs:1368`) delivers to an **established**
session and reports `NoActiveSession` otherwise; it does not dial.

**Consequence.** A driver can only build toward a peer it can already reach:
either a peer with an existing session, or a peer it may legally dial under
the configured qualification profile. This is the repository's existing
controlled-mesh evidence model, and Plan 505 must not widen it — a
public-internet dial scope would be a transport-boundary change with its own
plan-of-record.

### 3.6 The liveness scheduler has no population

Plan 504's closure record states it plainly:

> The production daemon installs **no exploratory tunnels** …
> `exploratory_liveness_pairs()` returns empty in production today and the
> scheduler tracks **zero pairs**. It is **installed and inert**, not broken.

### 3.7 Plan 185's closure record is honest about this

Plan 185 delivered the **mechanism** and proved it with out-of-crate lanes and
an exact-pinned i2pd external lane. It never claimed the production daemon
initiates exploratory builds; its "Known limitations" section says so. **No
correction to Plan 185's record is needed** — this is an open gap, not a false
claim. The Plan 503/504 precedent (correct a false claim in the successor's
record) does not apply here.

### 3.8 The mechanism is bounded by construction

- `MAX_PENDING_BUILDS = 16` (`crates/i2pr-daemon/src/exploratory_build.rs:104`),
  `BUILD_DEADLINE_MS = 60_000` (`:137`),
  `MAX_LIVENESS_PAIRS = 8`, `MAX_PENDING_LIVENESS_TESTS = 16`
  (`crates/i2pr-daemon/src/tunnel_liveness.rs:81`, `:84`).
- The exploratory pool has its own slot cap and `mark_failed` failure counter.
- The profile store is bounded by `ProfileStoreConfig` (`max_profiles`).

## 4. Invariants that must not regress

1. **The declared version is not edited** and no floor is moved. Plan 491's
   API-version floors stay where they are; the driver *uses* them.
2. **A peer below the floor is refused, never silently skipped.** The
   driver's peer selection goes through `select_tunnel_build_peers`, whose
   refusals are counted by reason.
3. **Every refusal is counted by reason.** "No peer available", "peer material
   absent", "peer not reachable", "delivery refused", "build timed out" — each
   is a distinct counter, never a silent no-op.
4. **No wire-format change.** The build wire is the existing
   `ShortBuildI2npBridge` output in the existing `BridgeHeader`; the reply is
   decoded by the existing strict OTBRM decoder.
5. **No new dependency.** `Cargo.toml` and `Cargo.lock` stay byte-identical.
6. **No unbounded structure.** The driver's own state is bounded (at most one
   in-flight attempt, a bounded counter set).
7. **No new task, no new timer, no new listener, no new socket.** The driver
   runs on the **existing** 250 ms group tick, beside `drive_tunnel_liveness`.
8. **One build at a time.** The coordinator's own pending table is the
   authority; the driver submits only when the table is empty.
9. **A destination tunnel is never touched.** The driver is exploratory-only;
   the destination pool and the destination build path are out of scope.
10. **The liveness proven-pair gate is unchanged.** A peer that never answers
    is never torn down.
11. **The driver's failure path is bounded and typed.** A failed build is
    counted, not retried in a loop.
12. **No advertisement.** `router.version` stays `0.9.62`; `caps` and the
    published RouterInfo are unchanged; the support inventory's advertised
    count stays zero.
13. **A local condition is never written into a peer's profile.** The
    router's own faults are counted by the driver's counters and never
    degrade a peer's profile score.

## 5. Scope

### 5.1 In scope

- A new module `crates/i2pr-daemon/src/exploratory_driver.rs` owning the
  driver: peer selection, the build attempt, the outcome observation, and the
  counters.
- The driver wired into the **live** product composition, driven on the
  **existing** tick beside `drive_tunnel_liveness`.
- The peer-profile store **fed** from the validated RouterInfo store, and
  **written** by the driver's outcome observations.
- The driver's counters published through the daemon's existing
  `InspectionHandles` seam.
- An **operator opt-in**, off by default, following the `[transit]
  participation` precedent (`crates/i2pr-daemon/src/config.rs:957`).
- The controls: a static guard with inverted probes, and an out-of-crate lane.

### 5.2 Explicitly out of scope

- **Refreshing the validated RouterInfo store from live NetDB lookups.** The
  store's population policy is a separate concern; the driver draws from the
  store as it is.
- **A public-internet dial scope.** The transport's loopback / private-LAN
  restriction stays exactly as it is.
- **The `NetDbTunnelCoordinator` composition.** That is Plan 433's, and Plan
  433 is registered `blocked`.
- **Destination tunnels.** The driver never builds, registers, removes or
  rebuilds a destination tunnel.
- **The lease-set publication path.** Plan 502/503 own it.
- **Multi-hop exploratory tunnels.** Plan 185 scoped the mechanism to one hop
  and this plan keeps that.
- **Any change to the liveness scheduler's semantics.** Plan 504's gate and
  its authenticated correlation are frozen.
- **Stock-peer evidence.** No external lane is run by this plan; the external
  question stays open and is recorded as such.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/exploratory_driver.rs` | **new** — the driver, its counters, its typed outcome, and the peer-material resolver seam |
| `crates/i2pr-daemon/src/service_product.rs` | the product owns a driver; `drive_exploratory_builds` runs on the existing tick; the driver observes the inbound build outcomes and the expiry sweep |
| `crates/i2pr-daemon/src/lib.rs` | the composition hands the `PeerParticipationHandle` to the product and publishes the driver's counters |
| `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` | the driver's counters are published, following the existing `publish_*` / `*()` pattern |
| `crates/i2pr-daemon/src/config.rs` | an operator opt-in section, off by default, with bounded limits |
| `crates/i2pr-daemon/tests/exploratory_driver_boundary.rs` | **new** — the static guard |
| `crates/i2pr-daemon/tests/exploratory_driver_live.rs` | **new** — the out-of-crate lane |

## 7. Work packages

### WP1 — the feasibility question, as an executable artefact

**Before any production code changes**, an executable artefact must answer one
question by driving the **real** composition:

> Can the driver, running against the real coordinator, the real profile
> store and the real delivery seam, name a peer, build a one-hop exploratory
> tunnel toward it, install it into the pool and the registry, and record the
> outcome — **without any wire change and without any new dependency**?

The artefact must use the **product-level driver**, not the coordinator's
`submit` directly; a feasibility proof that bypasses the driver proves
nothing about the driver. The controlled-mesh harness in
`crates/i2pr-daemon/tests/exploratory_build_live.rs` (`start_daemon_pair`,
`pump_responder_loop`) is the existing base for it.

**The artefact may stop the plan.** If the loop cannot be closed end to end,
Plan 505 does not proceed to WP2 and the gap is recorded with the exact
boundary.

### WP2 — the peer-material resolver

The driver needs, for a selected peer key, the peer's public static encryption
key and its dialability. The resolver seam resolves a key to
`PeerBuildMaterial`-grade material from the validated store, mirroring
`peer_static_encryption_key`
(`crates/i2pr-daemon/src/service_product.rs:3398`): the key is the RouterInfo
identity's public key (`router_identity().public_key()`), **not** the SSU2
address `s` option — Plan 185's external lane found that defect and fixed it.

A peer with no validated RouterInfo, or one whose identity key is the wrong
length, is **refused and counted**, never substituted.

### WP3 — the peer selection

The driver selects through `PeerParticipationDriver::select_tunnel_build_peers`
(`crates/i2pr-daemon/src/netdb_participation.rs:384`), which is Plan 491's version-gated selection
with `require_floodfill: false` — an exploratory tunnel is built toward an
ordinary peer, not necessarily a floodfill.

The selection's refusals (unknown version, uncomparable version, below floor)
are counted by reason. A selection that yields no candidate is counted as
"no eligible peer" and is **never** a silent skip.

The peer-profile store must be **fed** before it can select: the driver (or
the composition) learns the validated RouterInfos from the store the driver
draws its material from, so the store and the resolver can never disagree
about which peer exists.

### WP4 — the build attempt

The driver submits through the existing `rebuild_exploratory`
(`crates/i2pr-daemon/src/exploratory_build.rs:2679`) — the same path the liveness rebuild uses — so
there is exactly **one** code path that builds an exploratory tunnel, and the
first build and the rebuild cannot drift apart.

The driver submits only when:

- the operator has enabled the feature;
- the coordinator is not paused (`is_paused`, `crates/i2pr-daemon/src/exploratory_build.rs:1113`);
- the pending table is empty (`pending_len`, `:1191`);
- the pool has no usable exploratory pair;
- a peer was selected and its material resolved.

Every one of those refusals is counted.

### WP5 — the outcome observation

The driver keeps a **single bounded in-flight record** (direction, peer key,
attempt id). When the coordinator produces a terminal outcome — from the
inbound build reply (`InboundRouteOutcome::coordinator`,
`crates/i2pr-daemon/src/exploratory_build.rs:2784`) or from `expire_pending` (`:2588`) — the driver
maps it to the peer and records one observation:

| Outcome | Observation |
| --- | --- |
| `Installed` | `observe_answered` |
| `TimedOut` | `observe_unanswered` |
| `HopRejected` | `observe_refused` |
| `DeliveryFailed` | `observe_unreachable` |
| `InvalidReply` | `observe_misbehaving` — the peer sent a reply the strict decoder refused |
| `Cancelled`, `PoolRejected`, `InvalidRegistration`, `InvalidPath`, `CoordinatorRejected` | **none** — these are *this router's* condition |
| the four destination-scoped variants | never attributed to the driver |

**Every terminal outcome produces exactly one disposition**, and an outcome
with no in-flight record is counted as an orphan and produces none. The
mapping is total over all fourteen `BuildCoordinatorOutcome` variants: no
outcome is silently dropped.

**A local condition is never written into a peer's profile.** The local pool
refusing a registration, an invalid local registration, a local coordinator
rejection, an invalid path and a local cancellation are **this router's**
faults. Recording any of them as `observe_misbehaving` would blame the peer
for a condition the peer did not cause — and would let a local fault degrade
a good peer's profile score, which is the peer-side equivalent of
publishing a peer-side refusal as an `i2pr` defect. The driver counts those
outcomes in its own counters and leaves the peer's profile untouched. The
distinction is the same one the repo already draws between a peer that
**misbehaves** (it replied with data that does not match the requested key)
and a peer that is merely **unreachable**.

### WP6 — the counters and the inspection seam

The driver publishes counts through the existing inspection seam: attempts,
installed, timed out, refused, unreachable, plus "no peer available",
"peer material absent", "peer not reachable", "coordinator paused",
"pending busy", "orphan outcome". Counts only — no peer identity, no address,
no key material.

### WP7 — the controls

**The static guard** (`crates/i2pr-daemon/tests/exploratory_driver_boundary.rs`)
is **structural**, following the technique of Plan 503's
`version_floor_boundary.rs` and Plan 504's `tunnel_liveness_boundary.rs`: a
named function's **own body** must contain the required **call**, matched
with its open parenthesis; comments are stripped; `#[cfg(test)]` blocks are
excluded; an absent function fails closed. Every rule carries an **inverted
probe**.

The rules must assert, at minimum:

1. The live composition constructs the driver and the product owns it.
2. The live tick drives the driver beside `drive_tunnel_liveness`.
3. The driver selects through `select_tunnel_build_peers(` — the version-gated
   selection — and **not** through an ungated path.
4. The driver submits through `rebuild_exploratory(` — the single exploratory
   build path — and the module contains no second, private build path.
5. Every terminal outcome produces an observation; the mapping is total.
6. The driver never touches a destination tunnel.
7. The feature is **off by default** in the config.

**The live lane** (`crates/i2pr-daemon/tests/exploratory_driver_live.rs`)
drives the **public** daemon API: fixed seeds, an explicit `now_ms`, no
wall clock, no environment variable, no `#[ignore]`, no stock peer, no
sleeping. It proves the selection, the refusal counting, the build attempt,
the observation mapping, the boundedness, and the counters' reachability.

### WP8 — the documentation

`docs/architecture/i2pr-daemon.md`, `specs/CONFORMANCE.md`, `specs/support.toml`
(one `experimental` surface, `advertised = false`), `plans/registry.md` and the
roadmap. The current-fact surface tallies move accordingly; **closed** closure
records keep their own historical tallies.

## 8. Failure, cancellation, restart, and contention semantics

- **Failure.** A refused selection, an unresolvable peer, an undialable peer,
  a refused delivery and a timed-out build are each counted by reason. The
  driver never retries inside one tick and never spins: it is driven from the
  tick, so a failure costs one tick.
- **Cancellation.** The driver holds no task and no timer. A cancelled pump
  leaves the coordinator's pending table as it is; the coordinator's own
  deadline sweep (`expire_pending`) ends the attempt and the observation
  follows from that sweep.
- **Restart.** Nothing is persisted by the driver. The profile store's own
  projection (Plan 488 F4) persists the *observations*, not the driver's
  in-flight record.
- **Contention.** The driver borrows the coordinator mutably for the duration
  of one attempt and the profile handle under its own lock for the duration of
  one selection — the same lock the maintenance owner takes, and it is never
  held across an await.

## 9. Compatibility and migration

None. No wire format changed, no `router.version` changed, no option changed,
no configuration key is required (the new section has a default), nothing is
persisted by the driver.

The **behavioural** consequence is stated plainly: with the feature enabled,
the router builds one one-hop exploratory tunnel at a time toward a peer it
can reach, and tears it down only when Plan 504's liveness gate allows it.

## 10. Required tests

- The static guard, with an inverted probe per rule, plus a benign probe (a
  construct that appears only in a comment or a `#[cfg(test)]` block must not
  be reported) and a sibling-function probe (a token in a different function
  must not satisfy a rule).
- The live lane, over the public API only.
- The existing lanes must keep passing **unchanged**: `version_floor_boundary`,
  `version_eligibility_live`, `version_eligibility_mesh`,
  `exploratory_build_live`, `exploratory_driver_*` (new),
  `tunnel_liveness_boundary`, `tunnel_liveness_live`, `destination_tunnel_live`,
  `destination_tunnel_unit`, `netdb_tunnel_live`.

## 11. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Focused:

```text
cargo test --locked -p i2pr-daemon --test exploratory_driver_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_driver_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test tunnel_liveness_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test tunnel_liveness_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_floor_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_eligibility_live -- --test-threads=1
```

## 12. Documentation updates

- `docs/architecture/i2pr-daemon.md` — the driver's owner, the selection
  path, the observation mapping, the opt-in, and the measured limitation.
- `specs/CONFORMANCE.md` — the Plan 505 section, including the explicit
  statement that **no stock-peer evidence** of a driver-initiated exploratory
  build exists.
- `specs/support.toml` — one `experimental` surface with `advertised = false`.
- `plans/registry.md`, `plans/subsystems/network-participation-roadmap.md` —
  the Plan 505 rows, and the disposition of Plan 497 limitation 5.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

1. The driver is constructed by the **live** composition and driven on the
   **existing** tick.
2. The driver names a peer through Plan 491's **version-gated** selection, and
   a below-floor peer is refused and counted.
3. The driver builds through the **single** exploratory build path and
   installs the result into the pool and the registry.
4. **Every** terminal outcome produces exactly one **disposition**, and the
   mapping is total over all fourteen `BuildCoordinatorOutcome` variants. A
   **local** condition is counted and produces **no** peer-profile write; the
   contrast — a genuine peer fault **does** write one — is asserted in the
   same test, so the omission cannot be a broken recorder.
5. The peer-profile store is **fed** in a live daemon and is **written** by the
   driver — the write side that Plan 497 limitation 5 recorded as open.
6. The liveness scheduler tracks a **non-zero** pair count in the controlled
   mesh, and the driver and the scheduler agree on the population.
7. Every refusal is counted by reason; no silent skip exists.
8. The feature is **off by default**, and a router that has never enabled it
   behaves exactly as it does today.
9. Nothing is advertised; `Cargo.toml` and `Cargo.lock` are byte-identical.
10. The static guard and the live lane are green, and every existing lane
    still passes unchanged.

### 13.2 Stop conditions

The plan stops, and the gap is recorded with the exact boundary, if any of
these holds:

- WP1's artefact cannot close the loop end to end.
- Closing it would require a wire-format change.
- Closing it would require a new dependency.
- Closing it would require a public-internet dial scope.
- Closing it would require composing `NetDbTunnelCoordinator` (Plan 433's
  blocked scope).
- Closing it would require weakening Plan 491's version gate, Plan 504's
  proven-pair gate, or any boundary script.

### 13.3 What this plan does **not** close

- **Plan 497 limitation 5's read half.** The driver becomes a live *reader* of
  the profile store, but the **routing table's** only reader remains Plan
  433's never-constructed `NetDbTunnelCoordinator`. The limitation's
  "write side and read side must land together" constraint is honoured in the
  sense that the store is no longer empty and is no longer unread — but Plan
  433's composition stays blocked, and this plan does not pretend otherwise.
- **The validated store's refresh policy.** RouterInfos learned from live
  NetDB lookups still do not enter it.
- **The stock-peer question.** Whether an ordinary public router accepts a
  driver-initiated one-hop exploratory build, and whether it relays a liveness
  probe back, is **unproven** and needs the external lanes.
- **Plan 487 F3's destination-inbound half.** It needs tunnel rekeying, which
  the tree does not implement at all.
- **Plan 491 F2 and F4**, **Plan 490 Medium #3**, **Plan 502 M1**, and Plan
  503's lookup-target API-version gate.

## 14. Closure evidence required

1. The WP1 feasibility artefact, with its measured answer.
2. The requirement-to-evidence matrix, with every requirement mapped to a
   `file:line` and to an executable check.
3. The full routine floor, green, with the exact command list and the step
   count.
4. The focused verification table.
5. The invariant, failure, migration and security reviews.
6. The limitations, the findings by severity, and the disposition of Plan 497
   limitation 5 — stated as a **partial** discharge if that is what it is.
7. The unblock audit.

## 15. Handoff notes

**The next candidate after this one** depends on what WP1 measures. If the
driver closes the loop, the next candidate is the **validated store's refresh
from live NetDB lookups** — the population the driver draws from is currently
the bootstrap snapshot, and a router that learns a peer does not remember it.
That is a separate plan-of-record because it changes what the router stores.

Do not run the external lanes under this plan's number. The stock-peer
question stays open until Plan 493's authorized topology is available.
