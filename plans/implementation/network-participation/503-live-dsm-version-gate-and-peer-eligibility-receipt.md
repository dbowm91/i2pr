# Plan 503 — The live DSM version gate and the live peer-eligibility receipt

| Field | Value |
| --- | --- |
| Plan | 503 |
| Subsystem | `network-participation` |
| State | `registered` (`registered-live-dsm-version-gate-and-peer-eligibility-receipt`) |
| Depends on | Plan 491 (`passed-api-version-floors-and-peer-eligibility`), Plan 497 (`passed-netdb-participation-consumer-phase`), Plan 502 (`passed-transit-owner-in-the-product-composition`) |
| Discharges | **Plan 491 F1** (the receipt log is not produced by a live product loop) and the **live half** of Plan 491's DSM obligation; records **Plan 491 F3** as already discharged by Plan 497 |
| Closure record | `plans/closure/network-participation/503-status.md` |

## 1. Objective

Make the router's **live** API-version obligations true, and make the
**decision** observable in the running router.

Plan 491 attached two duties to the `0.9.62` level this router declares:

```text
minimum peers will build tunnels through, as of 0.9.68
minimum floodfill peers will send DSM to, as of 0.9.68
```

The **first** duty is enforced on the live path. The **second** is enforced
nowhere at all: the router's two live `DatabaseStore` sends choose floodfills
by XOR distance and never consult the floor. This plan puts the floor on the
live DSM path, and turns Plan 491's peer-eligibility receipt into a product
the running router actually produces.

## 2. Why ready

- Plan 491 delivered the comparator, the derived floors, the counted refusal
  vocabulary, and **two** enforcement points. The comparator and the floors
  are closed, derived constants (`crates/i2pr-netdb/src/version.rs`); they
  need no external topology to apply.
- Plan 497 wired the participation owner and the routing table into the
  daemon's service graph, so the daemon now owns one peer-profile store and
  one floodfill-indexed routing table in production.
- Plan 502 installed the transit data-plane owner in the product
  composition, so the daemon's live composition is the place where a new
  owner belongs.
- The live DSM paths, the live build path, and the daemon's public
  observation seam (`InspectionHandles`) all already exist. **This plan
  changes no wire format, no option, no capability, and no advertisement.**

## 3. Current implementation evidence

### 3.1 The tunnel-build half is live; the DSM half is not

| obligation | enforcement point | live? | live counters | live receipt |
| --- | --- | --- | --- | --- |
| build peers | `crates/i2pr-daemon/src/destination_peers.rs:282` `project_validated_store` | **yes** | yes — `CandidateProjectionSummary`, dropped on the floor | **no** |
| DSM targets | `crates/i2pr-netdb/src/publication.rs:205` `nearest_eligible_floodfills`; `crates/i2pr-daemon/src/netdb_tunnels.rs:790` `begin_publication` | **no** | n/a | **no** |

The build half runs in production. The daemon's 250 ms group tick reaches
`service_product.rs:5494` → `guard.destination_peer_candidates()` →
`destination_tunnels.rs:695` → `project_validated_store`, and the version
decision is taken at `destination_peers.rs:152`
(`tunnel_peer_eligibility(published_version_of(record))`). The refusals are
counted in `CandidateProjectionSummary` (`destination_peers.rs:61-79`) and
then **dropped**: the summary is a local variable at
`service_product.rs:5493-5496` and `service_product.rs:6714-6717`.

The DSM half does not run in production at all.
`NetDbTunnelCoordinator::begin_publication` — the typed, counted, fail-closed
DSM refusal — has **zero production callers**. The coordinator itself is
never constructed: `lib.rs:944-948` says so explicitly,

> That coordinator is **not** created in this phase, and must not be: Plan
> 433 owns the composition of the NetDB / destination-tunnel coordinator into
> the service graph and is registered blocked on an external acquisition
> contract.

### 3.2 The live DSM path bypasses the floor entirely

The daemon **does** send DSMs in production — through two paths, neither of
which consults `dsm_target_eligibility`:

1. **The creator RouterInfo publication.**
   `service_product.rs:6497` `publish_creator_router_info`, called from
   `service_product.rs:6688` inside `provision_all_service_router_material`
   (`service_product.rs:6668`), which the live composition reaches through
   `lib.rs:3991` → `ServiceProduct::start_over_existing_daemon`
   (`service_product.rs:2319`) → `service_product.rs:2456`.
   Its target selection (`service_product.rs:6513-6528`) walks
   `guard.store().floodfill_advertisers()`, skips our own hash, sorts by
   `xor_distance`, and truncates to `CREATOR_RI_PUBLICATION_FLOODFILLS` (3).
   No version decision is taken anywhere in that function.

2. **The service LeaseSet2 publication.**
   `service_product.rs:8222` `publish_service_ls2_for_service`, called from
   `service_product.rs:3008`, `:3606` and `:7586`. Its target selection
   (`service_product.rs:8409-8415`) calls
   `destination_tunnels.rs:912` `candidate_hashes`, which calls
   `crates/i2pr-netdb/src/lookup_policy.rs:257` `select_floodfill_candidates`
   — a selection that filters on **nothing but XOR distance and exclusion**.

The consequence is precise: **the router publishes its RouterInfo and its
LeaseSet2 to floodfills that publish no API version, an uncomparable API
version, or an API version below `0.9.62`** — the exact under-compliance
Plan 491 exists to prevent. Both pinned references refuse such a peer:
Java I2P 2.13.0's `TunnelPeerSelector` applies `MIN_VERSION = "0.9.62"`, and
i2pd 2.61.0's `libi2pd/NetDb.hpp` applies
`NETDB_MIN_FLOODFILL_VERSION = 0.9.62`.

### 3.3 The closure record's live-enforcement claim is true for one half only

`plans/closure/network-participation/491-status.md` §11 states:

> The *decisions* are enforced live — the two enforcement points refuse at
> the point of use, and the daemon's own counters
> (`publications_refused_by_version_floor`, `CandidateProjectionSummary`)
> count them.

For the tunnel-build gate that is accurate. For the DSM gate it is not: the
counter `publications_refused_by_version_floor` lives in
`NetDbTunnelCoordinator`, which is never constructed, and the live DSM paths
never consult the gate at all. **Plan 503 corrects this**; §13.3 states
exactly how, and the correction is recorded in the registry row and in this
plan's closure record — the closed record is not rewritten.

### 3.4 The receipt's owner is the wrong owner for a live receipt

Plan 491's `VersionEligibilityReceipt` (`netdb_participation.rs:149`) is
produced only by `PeerParticipationDriver::select_tunnel_build_peers`
(`:434`) and `select_dsm_targets` (`:448`). Both have **only test callers**
(`crates/i2pr-daemon/tests/version_eligibility_mesh.rs`). The driver is
constructed in the live composition (`lib.rs:1902`), but:

- its **profile store is never populated in production** — the only
  `driver.learn(...)` calls are inside `lib.rs`'s `#[cfg(test)]` module; and
- its maintenance task starts only when `netdb.routing_table_enabled` is
  true, which **defaults to false** (`config.rs:1119`).

So the driver is structurally mismatched with the live path: the live build
path reads a **different** store (`DestinationTunnelCoordinator`'s
`RouterInfoStore`), and the two stores are connected by no production code
path. A receipt produced by that driver would report `examined == 0` in a
running router.

### 3.5 The observation seam exists

`crates/i2pr-daemon/src/i2pcontrol_inspection.rs:201` `InspectionHandles` is
the daemon's one already-`Arc`-shared, public, in-process observation
channel. It holds `Mutex<Option<Arc<T>>>` for each published live owner
(`:212-240`) and is created at `lib.rs:683` and passed to every registrar.
Plan 502 used the same seam for the transit posture
(`publish_transit_participation`, `:502`).

### 3.6 The boundary section Plan 491 F3 asks for already exists

Plan 491 F3 recorded that `scripts/check-runtime-boundaries.sh` had no
`i2pr-netdb` section. **Plan 497 WP5 added one** (`scripts/check-runtime-boundaries.sh:112-156`),
and it scans the **whole** `crates/i2pr-netdb/src` directory, so Plan 491's
new `version.rs` and `selection.rs` are inside its scope. The section's
rules — no Tokio, no async functions, no sockets, no filesystem, process or
threads, no channels or tasks, no Tokio dependency, no wrong-crate
dependency — plus the normalised scan that masks comments and literals and
sees through grouped `use` statements cover the comparator and the policy
module. **Plan 503 verifies this with a negative control** (a clock
injected into `version.rs` must be reported) and records the disposition;
the rule is *fix the boundary, never weaken the script*, and no script is
weakened here.

## 4. Invariants that must not regress

1. **The declared version is not edited.** `CONTROLLED_ROUTER_VERSION` stays
   `"0.9.62"` in `crates/i2pr-netdb/src/local.rs`; Plan 285's pinning test is
   untouched.
2. **The floors are derived, never transcribed.** The obligation rows in
   `i2pr-proto` stay the only copy of the truth; the floors stay derived.
3. **Fail closed.** An unknown, uncomparable or below-floor peer is never a
   build peer and never a DSM target; an empty eligible set fails closed with
   a reason, and every refusal is counted by reason. A refusal is never a
   silent skip.
4. **No operator knob.** The floors are specification constants.
5. **No peer identity in a receipt.** A receipt carries counts and stable
   reason labels only.
6. **No advertisement, no wire change, no new option, no new dependency.**
   `router.version` stays `0.9.62`; `caps` and the published RouterInfo are
   unchanged; the support inventory's advertised count stays **zero**.
7. **Bounded.** Every new structure has an explicit ceiling: the receipt log
   is bounded, the candidate ceiling is the existing policy's, and the
   refusal counters saturate.
8. **No unbounded channel, no new task, no new listener.** This plan adds
   no task and no socket; it changes a selection and adds a bounded log.

## 5. Scope

### 5.1 In scope

- One shared, version-gated floodfill selection in `i2pr-netdb`
  (runtime-neutral), returning the eligible targets **and** the counted
  refusals. `PublicationCoordinator::nearest_eligible_floodfills` is
  refactored onto it with **byte-identical behaviour**.
- Both live DSM paths use that selection, so a below-floor floodfill is
  never a DSM target and every refusal is counted.
- The live build gate and the live DSM gate each produce Plan 491's
  `VersionEligibilityReceipt`, into one live-owned bounded log, published
  through `InspectionHandles`.
- The receipt type, its digest and its constructors move to one owner module
  so the live gates and the participation driver share one shape.
- The static boundary guard gains rules that the live DSM paths consult the
  gate, each with an inverted probe; a new out-of-crate lane proves the live
  path's behaviour.
- Plan 491 F3's disposition, verified with a negative control.

### 5.2 Explicitly out of scope

- **Plan 433's composition** of `NetDbTunnelCoordinator` into the service
  graph. That plan is registered **blocked** on an external acquisition
  contract, and this plan does not do blocked work under another number.
- **The lookup-target gate.** The router still asks a below-floor floodfill
  for a *lookup*. A `DatabaseLookup` is not a DSM and Plan 491's two
  obligations do not name it. Extending the floor to the lookup path changes
  which floodfills the router can ask at all — a product decision beyond
  Plan 491's table. **Recorded as an open finding**, not silently taken.
- **No advertisement, no capability change, no version change, no new
  configuration key, no new dependency, no I2NP message, no RouterInfo
  option, no new listener, no new service.**
- The **peer-profile store's** population in a live daemon (Plan 497
  limitation 5) — a different owner, recorded separately.
- Plan 491 F2 (no stock-peer evidence) and F4 (an intermittent
  `i2pcontrol_tunnels` staging failure): F2 needs the authorized two-host
  topology Plan 493 owns; F4 belongs to the I2PControl / service-tunnel line.
  Neither is touched here.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-netdb/src/lookup_policy.rs` | the shared nearest-floodfill core, the version-gated selection and its result type |
| `crates/i2pr-netdb/src/publication.rs` | `nearest_eligible_floodfills` refactored onto the shared selection (behaviour unchanged) |
| `crates/i2pr-daemon/src/version_eligibility.rs` | **new** — the receipt type, the digest, the constructors, and the live bounded owner |
| `crates/i2pr-daemon/src/netdb_participation.rs` | the receipt type and the digest move to the new module; the driver keeps its API and re-exports the type |
| `crates/i2pr-daemon/src/destination_tunnels.rs` | the coordinator gains the gated selections and records the receipts |
| `crates/i2pr-daemon/src/service_product.rs` | both live DSM paths use the gated selection |
| `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` | the live receipt log is published for observation |
| `crates/i2pr-daemon/src/lib.rs` | the composition creates the owner, publishes it, and hands it to the destination coordinator |

## 7. Work packages

### WP1 — one shared version-gated floodfill selection

`crates/i2pr-netdb/src/lookup_policy.rs` owns the ordering; `version.rs` owns
the comparator and the floors. This plan adds:

- a private `nearest_floodfill_candidates(store, routing_key, excluded,
  policy)` that computes the nearest floodfill set — the body of today's
  `select_floodfill_candidates` with the target exclusion moved into the
  caller's `excluded` set;
- `select_floodfill_candidates`, which delegates to it with the target
  added to `excluded`. Its **behaviour and its output are unchanged**;
- `VersionGatedFloodfillSelection { targets, refusals, examined }` and
  `select_version_gated_floodfills(store, routing_key, excluded, policy)`,
  which apply `dsm_target_eligibility(published_version_of(record))` to each
  candidate and count every refusal by reason.

**The invariant:** `examined == accepted + refused`, exactly. A candidate
whose record cannot be read is not counted as examined, so the identity
cannot drift.

`PublicationCoordinator::nearest_eligible_floodfills` is refactored onto
`select_version_gated_floodfills` so the closed enforcement point and the
live paths share **one** implementation. The public shape
(`FloodfillTargetSelection`, `targets()`, `refusals()`) is unchanged.

### WP2 — the live DSM paths consult the floor

**The creator RouterInfo publication** (`service_product.rs:6497`) replaces
its hand-rolled XOR sort with the coordinator's gated selection:

- the routing key is our own router hash and our own hash is excluded —
  the same set the hand-rolled sort produced;
- the eligible set is then truncated to `CREATOR_RI_PUBLICATION_FLOODFILLS`.

The bound is unchanged in spirit and slightly stronger in effect: the
candidates are considered within the policy's ceiling (32) and the
**eligible** ones are kept, so a below-floor floodfill can no longer consume
one of the three publication slots.

**The service LeaseSet2 publication** (`service_product.rs:8222`) replaces
`candidate_hashes` with the gated equivalent. When no eligible floodfill
remains, the existing fail-closed path is taken: the reserved publication
intent is released and the typed `PublicationFailureClass::RequestConstruction`
/ `PublicationBeginRejection::NoEligibleFloodfill` rejection is returned.
**A refused floodfill is never counted as a delivery failure** — it is
counted as a refusal, by reason, in the receipt.

Both paths keep their existing "best-effort publication; provisioning
continues" semantics. A below-floor floodfill is a **refusal**, not an
error: the publication proceeds to the next eligible target, and if none
remains the publication is skipped with the refusal counts recorded.

### WP3 — the live receipt owner

`crates/i2pr-daemon/src/version_eligibility.rs` owns the peer-eligibility
decision surface:

- `VersionDecisionKind` and `VersionEligibilityReceipt` (moved from
  `netdb_participation.rs`);
- `version_receipt_digest` (moved, **byte-identical algorithm**): the gate
  label, the examined count, the accepted count, then the refusal counts in
  sorted label order, hashed with SHA-256 through `i2pr_proto`'s reviewed
  `Hash::digest`;
- the two constructors: one from a `CandidateProjectionSummary` (the build
  gate) and one from a `VersionGatedFloodfillSelection` (the DSM gate);
- `VersionEligibilityOwner` — the bounded log: a `VecDeque` with an explicit
  cap, evicting the oldest entry at the cap, holding an internal
  `std::sync::Mutex` so the shared handle is a plain `Arc`.

The owner's cap is a constant, not a configuration key: a bounded log of
recent decisions, sized for an operator to read the last decisions without
an unbounded structure. The receipt log carries **counts only** — no peer
hash, no address, no key, no version string — so it is safe to expose.

`netdb_participation.rs` re-exports the moved types, so Plan 491's public
API and its lane are unchanged.

### WP4 — the live gates record their decisions

`DestinationTunnelCoordinator` gains an optional
`Arc<VersionEligibilityOwner>` and three recording points:

- `destination_peer_candidates()` — the **build** gate. The receipt's
  `examined` is the number of records the projection considered and its
  `accepted` is the number of candidates that survived every refusal.
- the gated floodfill selections — the **DSM** gate. The receipt's
  `examined` is the number of candidates the gate examined and its
  `accepted` is the number of eligible targets.

Every one of the three is on a **live** path:

| decision | live call site |
| --- | --- |
| build gate | `service_product.rs:5494` (250 ms group tick) and `service_product.rs:6716` (startup provisioning) |
| DSM gate, creator RouterInfo | `service_product.rs:6513` (startup provisioning) |
| DSM gate, service LeaseSet2 | `service_product.rs:8409` (service publication) |

A coordinator without an owner records nothing — the controlled helper and
the integration lanes construct one without the owner and are unaffected.

### WP5 — the observation seam

`InspectionHandles` gains `version_live: Mutex<Option<Arc<VersionEligibilityOwner>>>`
with `publish_version_eligibility` and `version_eligibility`, following the
existing `publish_*` / `*()` pattern exactly. The composition root creates
the owner, publishes it, and hands a clone to the destination coordinator.

The getter is a cheap, non-async, lock-free-for-the-caller read of the
owner; the owner's own lock is held only for the length of a clone.

### WP6 — the controls

**The static guard** (`crates/i2pr-daemon/tests/version_floor_boundary.rs`)
gains rules, each with an **inverted probe** and the existing benign probe
retained:

1. the live creator-RouterInfo publication path consults the gated
   selection;
2. the live LeaseSet2 publication path consults the gated selection;
3. the live build path records a peer-eligibility receipt;
4. the receipt type still carries no peer identity — **retargeted** from
   `netdb_participation.rs` to the new module, with the probe unchanged in
   substance.

**The out-of-crate lane** (`crates/i2pr-daemon/tests/version_eligibility_live.rs`)
drives the **public** daemon API (`DestinationTunnelCoordinator`) and proves:

- a below-floor floodfill is **not** a DSM target, and the refusal is
  counted by reason;
- an unknown-version and an uncomparable-version floodfill are refused the
  same way;
- a floodfill at the floor is eligible;
- the live gate produces a receipt whose `examined`, `accepted` and refusal
  counts are the decision's own counts, and whose `examined == accepted +
  refused`;
- the receipt and the rendered log carry **no** peer hash, address, key or
  version string;
- the receipt log is **bounded** — at the cap the oldest entry is evicted.

### WP7 — Plan 491 F3's disposition

Verify with a negative control that `scripts/check-runtime-boundaries.sh`'s
`i2pr-netdb` section reports a clock, a socket and an async function
injected into `crates/i2pr-netdb/src/version.rs`, and that it passes on the
real tree. Record the disposition in the closure record and the registry row.
**No script is weakened.**

### WP8 — the documentation

`docs/architecture/i2pr-netdb.md`, `docs/architecture/i2pr-daemon.md`,
`specs/CONFORMANCE.md`, and `specs/support.toml` (one `experimental`
surface, `advertised = false`) — plus the correction of the registry's stale
support-inventory count (93/87 → **94/88**), which drifted when Plan 492
added a surface and was never propagated.

## 8. Failure, cancellation, restart, and contention semantics

- **Failure.** A below-floor floodfill is a **refusal**, not an error. The
  publication continues to the next eligible target; if none remains, the
  publication is skipped and the refusal counts are recorded. No path fails
  the router's startup because of a refusal.
- **Cancellation.** The gate is a synchronous, bounded operation over an
  already-bounded store. It has nothing to cancel inside it. A cancelled
  publication leaves the coordinator's ledger and the receipt log
  consistent: the receipt records the *decision*, which was taken before the
  send.
- **Restart.** Nothing is persisted. The receipts are a live observation
  log, not a durable ledger; a restart starts an empty log. The version
  evidence is re-derived from each stored RouterInfo, exactly as Plan 491
  specifies.
- **Contention.** The owner holds a `std::sync::Mutex` for the length of a
  `VecDeque` push or clone. The lock is taken **while** the destination
  coordinator's Tokio mutex is held (the coordinator records under its own
  lock) and is **never** held across an `.await`. The observation path takes
  only the owner's lock and never takes the coordinator's lock, so the order
  is strictly **coordinator → owner**, with no cycle.

## 9. Compatibility and migration

- **No wire change.** The `router.version` option was already in the
  RouterInfo.
- **No `router.version` change.** The declaration stays `0.9.62`.
- **No migration.** Nothing is persisted.
- **No new configuration key.**
- **Behavioural consequence, stated plainly.** A router that knows only
  floodfills publishing no API version, an uncomparable version, or a
  version below `0.9.62` will **not** publish its RouterInfo or its
  LeaseSet2 to them. That is the intended fail-closed behaviour and it
  matches both pinned references, which also refuse a peer whose version
  they cannot establish or whose version is below the floor. A publication
  fixture that publishes no version must publish one — the fixture is
  wrong, not the floor, and the fixture is corrected rather than the floor
  weakened.

## 10. Required tests

- The comparator, the floors and the selection policy keep their existing
  tests unchanged.
- `crates/i2pr-netdb`: the shared selection's ordering matches
  `select_floodfill_candidates` exactly; the gate's refusal counts; the
  `examined == accepted + refused` identity; the refactored
  `nearest_eligible_floodfills` produces the same targets and the same
  refusal counts as before the refactor.
- `crates/i2pr-daemon`: the receipt constructors map the decision's counts
  onto the receipt; the digest is unchanged by the move (a golden value
  pins it); the owner is bounded and evicts the oldest entry at the cap; the
  coordinator records one receipt per decision and none without an owner.
- `crates/i2pr-daemon/tests/version_eligibility_live.rs`: the live gate's
  behaviour, as listed in WP6.
- `crates/i2pr-daemon/tests/version_floor_boundary.rs`: the new rules and
  their inverted probes.

## 11. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_eligibility_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_floor_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_eligibility_mesh -- --test-threads=1
bash scripts/check-runtime-boundaries.sh
bash scripts/check-dependency-direction.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the full routine floor from `AGENTS.md` before the closure.

## 12. Documentation updates

- `docs/architecture/i2pr-netdb.md` — the shared gated selection, the
  `examined == accepted + refused` identity, and the refactor of the
  publication coordinator onto it.
- `docs/architecture/i2pr-daemon.md` — the live DSM gate on both
  publication paths, the live receipt owner, and the observation seam.
- `specs/CONFORMANCE.md` — the Plan 503 section: what is now enforced live,
  what is not, the fail-closed behaviour, and the counted refusals.
- `specs/support.toml` — one `experimental` surface with
  `advertised = false`.
- `plans/registry.md` — the Plan 503 row, the Plan 491 row's F1/F3
  disposition, and the corrected support-inventory count.
- `plans/subsystems/network-participation-roadmap.md` — the Plan 503 row and
  the §12 status sentence.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

1. The live creator-RouterInfo publication and the live LeaseSet2
   publication **never** target a floodfill whose published API version is
   unknown, uncomparable, or below the floor; every refusal is counted by
   reason in a receipt.
2. The live build gate and the live DSM gate each produce a
   `VersionEligibilityReceipt` in the running router, and the log is
   observable through the daemon's public inspection seam.
3. The receipt carries counts and stable labels only — no peer hash, no
   address, no key, no version string.
4. The receipt log is bounded; the coordinator records nothing when it holds
   no owner.
5. The static guard proves each new rule fires, with an inverted probe per
   rule and the benign probe retained.
6. The out-of-crate lane proves the live gate's behaviour over the public
   daemon API.
7. `CONTROLLED_ROUTER_VERSION` is unchanged, the declared-version pinning
   test passes, the support inventory stays at **zero advertised**, and
   `specs/support.toml`, `Cargo.toml` and `Cargo.lock` stay byte-identical
   except for the one `experimental` surface.
8. The routine floor is green on the closing tree, and every floor-caught
   defect is fixed and recorded.

### 13.2 Stop conditions

- If applying the floor to the live DSM path makes a **stock-peer** lane
  fail for a reason that is not the floor, stop and attribute it. A
  peer-side refusal is **never** published as an `i2pr` defect.
- If the eligible set becomes empty in a lane that previously published, the
  fixture is corrected to publish a version — **never** the floor.
- If the receipt's counts cannot be derived from the live decision's own
  counts, stop: the receipt must describe the decision, not approximate it.
- If the gate would require a new configuration key, a new capability, a new
  advertisement, or a wire change, stop: this plan delivers none of those.

### 13.3 What this plan does **not** close

- **Plan 491 F2** — no stock-peer evidence for the floors; that needs the
  authorized two-host topology Plan 493 owns.
- **Plan 491 F4** — the intermittent `i2pcontrol_tunnels` managed-tunnel
  staging failure; it belongs to the I2PControl / service-tunnel line.
- **The lookup-target gate** — the router still asks a below-floor floodfill
  for a *lookup*. Recorded as an open finding for a successor plan.
- **Plan 433's composition** of `NetDbTunnelCoordinator` — blocked, untouched.
- **Plan 497 limitation 5** — the peer-profile store stays empty in a live
  daemon. Different owner.
- **No stock-peer evidence of any kind**, and therefore **no advertisement**
  of any kind.

## 14. Closure evidence required

- A requirement-to-evidence matrix naming, for every acceptance criterion,
  the executable command and the file and line that satisfy it.
- The exact floor step table with pass/fail per step.
- The false-pass audit: for every "passed" claim, the way it could have
  passed without proving anything.
- The invariant, failure, migration and security reviews.
- The findings by severity, with the medium and low findings **recorded**
  rather than absorbed.
- The roadmap disposition and the unblock audit.
- The commit list, and the floor-caught fixes committed separately as Plans
  489, 490 and 491 did.

## 15. Handoff notes

- The plan number is **503**; it is never renumbered.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`) and Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`). Nothing is patched or
  vendored.
- `specs/support.toml` gains exactly one `experimental` surface with
  `advertised = false`; the inventory's advertised count stays **zero**.
- `Cargo.toml` and `Cargo.lock` stay byte-identical: this plan adds **no
  dependency**.
- No boundary script is weakened. The only script change is a *new* rule
  with a positive control, if one is needed at all.
- The next free plan number after this one is **504**.
