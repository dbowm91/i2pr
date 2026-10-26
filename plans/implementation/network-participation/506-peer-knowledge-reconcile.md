# Plan 506 — The peer-knowledge reconcile: the live NetDB in the stores the router builds with

Status: **registered** · Registered: 2026-10-26 · Subsystem: `network-participation`

Predecessors: Plan 185 (the one-hop exploratory tunnels), Plan 186 (the NetDB
lookup / LeaseSet2 publication), Plan 488 (the peer-profile store), Plan 491
(the API-version floors), Plan 503 (the live DSM version gate), Plan 504 (the
liveness scheduler), Plan 505 (the exploratory build driver).

## 1. Objective

Make a RouterInfo the router accepts **on a live session** known to the two
stores it actually builds with — the **destination coordinator's authoritative
store** and the **peer-profile store** — and not only to the bootstrap NetDB.

Today a peer is remembered in the bootstrap store and forgotten everywhere
else. The two stores the router builds with are seeded **once at startup** and
never see a peer the router learns afterwards.

This is the population Plan 505's driver draws from. Plan 505 made the driver
live and fed the profile store at startup; this plan makes that population
**live**.

## 2. Why ready

- **The source is already live.** Three production sites ingest an
  authenticated unsolicited `DatabaseStore` into the bootstrap store
  (`crates/i2pr-daemon/src/lib.rs:3620`, `:4462`, `:4675`), through the
  existing bounded, fail-closed validation pipeline.
- **The read side is already live.** The destination path projects its store
  on **every** build (`destination_tunnels.rs:711` → `destination_peers.rs:282`
  → `service_product.rs:5982`, `:7212`), and Plan 505's driver selects from
  the profile store on every attempt (`exploratory_driver.rs:712`). A refresh
  therefore takes effect on the next build; there is no snapshot to rebuild.
- **The write APIs exist and are bounded.**
  `DestinationTunnelCoordinator::insert_validated_router_info`
  (`destination_tunnels.rs:698`), `PeerParticipationDriver::learn_all`
  (`netdb_participation.rs:273`) and `Bootstrap::validated_router_info_snapshot`
  (`bootstrap.rs:307`).
- **A reconcile precedent exists in the same owner.** The routing table is
  re-populated from the bootstrap store on the participation owner's tick
  (`lib.rs:1988` → `populate_routing_table`, `lib.rs:2012`), with exactly the
  lock discipline a reconcile needs: snapshot under the bootstrap lock, apply
  outside it (`lib.rs:2050-2061`).

**Nothing about the wire changes.** A reconcile copies already-validated
`ValidatedRouterInfo` records between in-process stores.

## 3. Current implementation evidence

### 3.1 The three live ingest sites

All three are in `register_ssu2_service` (`crates/i2pr-daemon/src/lib.rs:3435`):

| Site | Trigger | Note |
| --- | --- | --- |
| `lib.rs:3620` | an **NTCP2** inbound carrying `DatabaseStore` | the `!ssu2_config.enabled` branch |
| `lib.rs:4462` | an **SSU2 or NTCP2** inbound carrying `DatabaseStore` | the SSU2-enabled select loop |
| `lib.rs:4675` | the **same message**, later in the same loop iteration | gated on `!served` |

Site `4675` is a **duplicate ingest** of the message site `4462` already
ingested, whenever the router is not serving the store as a floodfill. The
second insert yields `InsertOutcome::Idempotent`. It is harmless but it is a
real double write, and this plan owns it.

`UnsolicitedStorePolicy::Accept` admits **only**
`DatabaseStoreData::RouterInfoCompressed`; the LeaseSet family and `Deferred`
are refused with `UnsupportedPayload`
(`crates/i2pr-netdb/src/store_message.rs:81`). The ingest is bounded on every
axis: a decompressed-size ceiling (`:91`), a key-match check (`:98`), Plan 103
validation (`:103`) and the store's own capacity (`:104`).

### 3.2 The two build-with stores are startup-only

- `insert_validated_router_info` has exactly **one** production caller:
  `crates/i2pr-daemon/src/service_product.rs:2448`, at startup, from
  `bootstrap.validated_router_info_snapshot()`.
- `learn_bootstrap_peers` has exactly **one** production caller:
  `crates/i2pr-daemon/src/lib.rs:1946`, at startup.

Neither is called again. A peer the router accepts on a live session is
therefore known to the bootstrap and to **nothing else**.

### 3.3 The framing correction: the router does not learn peers from a lookup

`DatabaseStoreMessage` carries exactly **one** record
(`crates/i2pr-proto/src/i2np/netdb.rs:150`). `DatabaseSearchReplyMessage`
carries peer **hashes only** (`netdb.rs:223`). The production router runs
**no** RouterInfo-kind lookup: the only `begin_lookup` caller is the
never-constructed `NetDbTunnelCoordinator` (`netdb_tunnels.rs:466`, built only
inside its own `#[cfg(test)]` block), and the production live lookup is a
**LeaseSet2** lookup whose reply is a LeaseSet2
(`crates/i2pr-netdb/src/lookup_engine.rs:876`).

⇒ **The live-learned RouterInfos the router holds are exactly the unsolicited
`DatabaseStore`s of §3.1.** This plan's subject is therefore narrower than
"refresh from live NetDB lookups", and it says so rather than overstating.

### 3.4 The read sides are live, so a refresh takes effect immediately

- `DestinationTunnelCoordinator::destination_peer_candidates`
  (`destination_tunnels.rs:711`) reads `self.store` live, and the build path
  calls it on every build (`service_product.rs:5982`, `:7212`).
- `ProfileStorePeerResolver::resolve` (`exploratory_driver.rs:141`) reads the
  profile store's own validated records live, and `learn` makes a peer
  selectable immediately.

### 3.5 The destination store refuses at capacity; it does not evict

`RouterInfoStore::insert` returns `InsertOutcome::CapacityExceeded` when
`records.len() >= max_records` or the encoded-byte total would exceed
`max_total_encoded_bytes` (`crates/i2pr-netdb/src/store.rs:158-167`). It never
evicts. A reconcile must treat `CapacityExceeded` as **counted, not fatal** —
the startup seed's policy (`service_product.rs:2452-2468`), which treats it as
fatal, **must not** be copied.

The peer-profile store is different: it **evicts the worst** to make room and
reports `ProfileCapacityExceeded` only when eviction fails
(`crates/i2pr-netdb/src/profile_store.rs:798-804`).

### 3.6 A refresh changes *which* peers the build path sees

`RouterInfoStore.records` is a `BTreeMap<RouterHash, ValidatedRouterInfo>`
(`crates/i2pr-netdb/src/store.rs:84`), and `project_validated_store` takes the
**first 256 in hash order** (`destination_peers.rs:289`,
`MAX_DESTINATION_CANDIDATES = 256` at `:31`).

So adding peers to the destination store does not merely add candidates — it
can **change the visible set**, because the projection is a hash-ordered
prefix, not a recency list. This plan owns that behaviour explicitly rather
than discovering it later.

### 3.7 The version gate is applied at selection, so it is not weakened

`select_version_gated` applies Plan 491's gate at **selection**
(`netdb_participation.rs:406-424`), and `DestinationPeerCandidate::from_validated`
applies it per record (`destination_peers.rs:151-155`). More records in a store
means more records **for the gate to refuse**: a refreshed peer below the floor
is refused with `CandidateRefusal::BelowVersionFloor` and counted
(`destination_peers.rs:305`), exactly as a startup-seeded one is.

**The refresh must go through `learn_all` / `insert_validated_router_info`
and never feed a selection directly**, or the gate would be bypassed.

### 3.8 The stores are bounded by named configuration

`[netdb] max_records` (default 4 096, refused if `0` or above 65 536) and
`max_encoded_bytes` (default 4 MiB, refused if `0` or above 64 MiB)
(`crates/i2pr-daemon/src/config.rs:1126-1132`, `:3783`, `:3789`, `:3795`,
`:3801`). Both quotas are enforced inside `RouterInfoStore::insert`, and the
live ingest goes through it, so the ingest is bounded and **fails closed at
capacity**.

## 4. Invariants that must not regress

1. **The declared version is not edited** and no floor is moved.
2. **Plan 491's version gate is not weakened.** The refresh inserts into the
   stores; the gate stays at selection, and a refreshed peer below the floor is
   refused and counted.
3. **A refresh never bypasses a selection.** The refresh writes *peer
   records*, never a selection or a candidate list.
4. **No wire-format change.** The refresh copies already-validated records
   between in-process stores.
5. **No new dependency.** `Cargo.toml` and `Cargo.lock` stay byte-identical.
6. **Bounded.** Every store the refresh writes is bounded by a named
   configuration quota, and the refresh itself reads a **bounded** snapshot.
7. **Fail closed and counted.** A capacity refusal, a conflict, a stale
   replacement and a malformed record are each **counted by reason** and never
   fatal.
8. **No unbounded structure.** The reconcile holds no growing collection.
9. **No new task, no new timer, no new listener, no new socket.** The reconcile
   runs on the **existing** product tick, gated by a bounded cadence.
10. **No lock is held across an await, and the locks are taken in one order.**
    The bootstrap's lock is released before any other lock is taken; the
    destination coordinator's `tokio::sync::Mutex` is never held while the
    bootstrap's or the profile store's lock is held.
11. **Plan 505's local-condition carve-out is untouched.** The refresh writes
    *peer records*, not observations; the two are orthogonal.
12. **A destination tunnel is never touched.** The reconcile writes peer
    records only.
13. **No advertisement.** `router.version` stays `0.9.62`; `caps` and the
    published RouterInfo are unchanged; the support inventory's advertised
    count stays zero.

## 5. Scope

### 5.1 In scope

- A new bounded reconcile in the **product**: snapshot the bootstrap's
  validated RouterInfos, insert them into the destination store, learn them
  into the peer-profile store, and count every outcome by reason.
- The product gains the **bootstrap handle** and the **peer-profile handle**
  it needs, through the composition.
- The reconcile runs on the **existing** product tick, gated by a **bounded
  cadence** carried in the spec.
- The **duplicate ingest** at `lib.rs:4675` is removed: the same message must
  not be ingested twice.
- The counters published through the existing inspection seam.
- The controls: a static guard with inverted probes, and an out-of-crate lane.

### 5.2 Explicitly out of scope

- **A RouterInfo-kind NetDB lookup.** The router performs none in production
  (§3.3), and adding one is a protocol-composition change with its own
  plan-of-record.
- **The routing table's refresh.** It already runs on the participation
  owner's tick (`lib.rs:1988`); this plan does not move it.
- **Plan 433's `NetDbTunnelCoordinator` composition.** Still `blocked`.
- **The destination store's capacity policy.** It refuses at capacity; this
  plan does not add eviction.
- **The projection's hash-order prefix.** This plan **owns** the behaviour
  (§3.6) and records it; it does not change the ordering, because the
  ordering is Plan 103's store semantics and changing it is a separate
  decision.
- **A stock-peer lane.** The external question stays open.
- **Plan 487 F3's destination-inbound half.** It needs tunnel rekeying.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/peer_reconcile.rs` | **new** — the reconcile, its bounded counters and its cadence gate |
| `crates/i2pr-daemon/src/service_product.rs` | the product gains the bootstrap and peer-profile handles, the reconcile, and the cadence |
| `crates/i2pr-daemon/src/lib.rs` | the composition hands the product its handles; the duplicate ingest is removed |
| `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` | the reconcile's counters are published, following the existing `publish_*` / `*()` pattern |
| `crates/i2pr-daemon/tests/peer_reconcile_boundary.rs` | **new** — the static guard |
| `crates/i2pr-daemon/tests/peer_reconcile_live.rs` | **new** — the out-of-crate lane |

**Correction, recorded when the code landed (2026-10-26).** The table above
originally listed `crates/i2pr-daemon/src/config.rs` as a production change.
It is **not**: the reconcile reuses the **existing**
`netdb.participation_maintenance_period_ms` key, which the configuration
parser already bounds to `[1 000 ms, 3 600 000 ms]`, so **no new key** is
added and `config.rs` is untouched. The reconcile's own bound is the same
range, and the composition root copies the value out of the configuration
the way every other configuration value the `'static` service closure reads
is copied. The table also gained the `peer_reconcile.rs` row, because the
reconcile landed in its own module rather than inside `service_product.rs`.

## 7. Work packages

### WP1 — the feasibility question, as an executable artefact

**Before any production code changes**, an executable artefact must answer one
question by driving the **real** composition:

> Does a peer the router accepts on a live session become selectable — and
> buildable — **without a wire change, without a new dependency, and without
> weakening Plan 491's gate or Plan 505's carve-out**?

The artefact must use the **real** `Bootstrap`, the **real**
`DestinationTunnelCoordinator` and the **real** `PeerParticipationDriver`, and
must show a peer that is **not** in either store before the reconcile becoming
**selectable and insertable** after it — and a **below-floor** peer staying
**refused** after it.

**The artefact may stop the plan.** If the loop cannot be closed end to end,
Plan 506 does not proceed to WP2 and the gap is recorded with the exact
boundary.

### WP2 — the reconcile

The reconcile is one bounded operation:

1. **Snapshot** the bootstrap's validated RouterInfos under the bootstrap's
   lock — the bounded snapshot (`bootstrap.rs:307`), capped at
   `MAX_GROUP_PROVIDER_ROUTER_INFOS`.
2. **Release** the bootstrap's lock.
3. **Insert** each record into the destination store through
   `insert_validated_router_info`, counting every `InsertOutcome` by reason.
4. **Learn** the same records into the peer-profile store through
   `learn_all`, counting the `LearnSummary` by reason.

`CapacityExceeded` is **counted, not fatal** (§3.5).

### WP3 — the cadence

The reconcile runs on the **existing** product tick, gated by a bounded
cadence. The cadence is the **existing** participation maintenance period
(`config.netdb.participation_maintenance_period_ms`), so there is **one**
source of truth and no second knob: the router's peer knowledge reconciles on
the same cadence its peer profiles are maintained.

The cadence is bounded on both sides by the same validation the existing
period has, and a zero or out-of-range value is a configuration error.

### WP4 — the duplicate ingest

The second ingest of the same message (`lib.rs:4675`) is removed. The
unsolicited store must be ingested **once** per message; the floodfill-serving
branch keeps its own behaviour and the non-serving branch no longer re-ingests
what the loop already ingested.

This is a **product defect** in its own right — a double write on every
unsolicited store the router accepts — and it is fixed here rather than
recorded.

### WP5 — the counters and the inspection seam

The reconcile publishes counts through the existing inspection seam:
snapshotted, inserted, replaced, idempotent, conflict, stale, capacity
exceeded, learned, learned-capacity-exceeded, and the cadence skips. **Counts
and stable reason labels only** — no peer, address, key or payload.

### WP6 — the controls

**The static guard** (`crates/i2pr-daemon/tests/peer_reconcile_boundary.rs`)
is **structural**, following the technique of Plan 503's
`version_floor_boundary.rs`, Plan 504's `tunnel_liveness_boundary.rs` and
Plan 505's `exploratory_driver_boundary.rs`: a named function's **own body**
must contain the required **call**, matched with its open parenthesis;
comments stripped; `#[cfg(test)]` blocks excluded; an absent function failing
closed. Every rule carries an **inverted probe** on a copy of the real source,
and the probe asserts the mutation applied.

The rules must assert, at minimum:

1. The composition hands the product the **bootstrap handle** and the
   **peer-profile handle** it needs.
2. The product **reconciles** on its **existing** tick, and the reconcile
   method contains **no** `tokio::spawn` and **no** `tokio::time::interval`.
3. The reconcile **snapshots under the bootstrap lock and releases it before**
   any other lock is taken — the rule must assert the ordering structurally,
   not merely that both locks appear.
4. The reconcile writes through `insert_validated_router_info(` and
   `learn_all(` — the **canonical** write APIs — and contains **no** direct
   store manipulation and **no** selection-side write.
5. The reconcile is **cadenced**, and the cadence is bounded.
6. `CapacityExceeded` is **counted, not fatal**: the reconcile must not abort
   the batch on a capacity refusal.
7. The **unsolicited store is ingested once**: the duplicate ingest site is
   gone, and the surviving site still ingests.
8. The reconcile's counters are published through the inspection seam, and the
   seam's reader returns a gap rather than a fabricated zero.
9. Plan 491's version gate is still applied at **selection**, and the refresh
   does not write a selection.

**The live lane** (`crates/i2pr-daemon/tests/peer_reconcile_live.rs`) drives
the **public** daemon API only: fixed seeds, an explicit `now_ms`, no wall
clock, no environment variable, no `#[ignore]`, no stock peer, no sleeping.

### WP7 — the documentation

`docs/architecture/i2pr-daemon.md`, `specs/CONFORMANCE.md`,
`specs/support.toml` (one `experimental` surface, `advertised = false`),
`plans/registry.md` and the roadmap. The current-fact surface tallies move
accordingly; the **closed** closure records keep their own historical
tallies.

Plan 505's closure record and the daemon deep-dive state the startup-only
limitation in prose; those statements are **current fact** and must be updated
to describe the live reconcile. Plan 505's closure record is **not** rewritten
— the correction and the update live here, in this record and in the registry's
row, as the Plan 503/504 precedent requires.

## 8. Failure, cancellation, restart, and contention semantics

- **Failure.** A capacity refusal, a conflict, a stale replacement and a
  malformed record are each counted by reason and **never fatal**. One bad
  record does not abort the batch.
- **Cancellation.** The reconcile holds no task and no timer; it is a step on
  the existing tick. A cancelled pump leaves the stores as they are.
- **Restart.** Nothing is persisted by the reconcile. The peer-profile
  store's own projection (Plan 488 F4) persists the *observations*, not the
  peer records; the peer records are re-learned from the bootstrap at
  startup and then reconciled live again.
- **Contention.** The bootstrap's lock is taken for the snapshot and
  **released** before any other lock is taken; the destination coordinator's
  `tokio::sync::Mutex` is taken for the insert and released before the
  profile store's lock is taken for the learn. **The locks are taken in one
  order and never nested**, so no cycle is possible. The profile store's
  `with` closure never awaits.

## 9. Compatibility and migration

None. No wire format changed, no `router.version` changed, no advertisement
changed, nothing is persisted by the reconcile.

The **behavioural** consequence is stated plainly: the router's peer
population becomes **live**. A peer the router accepts on a live session
becomes a candidate for the next tunnel build and a peer the exploratory
driver may select.

## 10. Required tests

- The static guard, with an inverted probe per rule, plus a benign probe (a
  construct only in a comment or a `#[cfg(test)]` block must not be reported)
  and a sibling-function probe (a token in a different function must not
  satisfy a rule).
- The live lane, over the public API only.
- The existing lanes must keep passing **unchanged**:
  `exploratory_driver_boundary`, `exploratory_driver_live`,
  `tunnel_liveness_boundary`, `tunnel_liveness_live`,
  `version_floor_boundary`, `version_eligibility_live`,
  `version_eligibility_mesh`, `exploratory_build_live`,
  `destination_tunnel_live`, `destination_tunnel_unit`, `netdb_tunnel_live`.

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
cargo test --locked -p i2pr-daemon --test peer_reconcile_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test peer_reconcile_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_driver_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_driver_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test tunnel_liveness_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test tunnel_liveness_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_floor_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_eligibility_live -- --test-threads=1
```

## 12. Documentation updates

- `docs/architecture/i2pr-daemon.md` — the reconcile's owner, the lock
  discipline, the cadence, the capacity policy, the selection-set change, and
  the removal of the duplicate ingest.
- `specs/CONFORMANCE.md` — the Plan 506 section, including the explicit
  statement that the router learns peers from **unsolicited stores**, not from
  lookups, and that **no stock-peer evidence** exists.
- `specs/support.toml` — one `experimental` surface with `advertised = false`.
- `plans/registry.md`, `plans/subsystems/network-participation-roadmap.md` —
  the Plan 506 rows, and the update to the startup-only limitation that
  Plan 505's closure record and the deep-dive state.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

1. A peer the router accepts on a live session is known to the **destination
   store** and the **peer-profile store** after one reconcile, without a
   restart.
2. The reconcile is **bounded**: it reads a bounded snapshot and every store
   it writes is bounded by a named configuration quota.
3. Every outcome is **counted by reason**; a capacity refusal, a conflict and
   a stale replacement are counted and **not fatal**.
4. A **below-floor** peer stays **refused** after the reconcile — Plan 491's
   gate is not weakened.
5. The reconcile runs on the **existing** tick: the product gains **no** task,
   no timer, no listener and no socket.
6. The locks are taken in **one order** and never nested; the bootstrap's
   lock is released before any other lock is taken.
7. The **unsolicited store is ingested once**; the duplicate write is gone.
8. The reconcile's counters are published, and an unpublished or poisoned
   slot is a gap, never a fabricated zero.
9. The **selection-set change** (§3.6) is recorded as a finding, not left to
   be discovered.
10. Nothing is advertised; `Cargo.toml` and `Cargo.lock` are byte-identical.
11. The static guard and the live lane are green, and every existing lane
    still passes unchanged.

### 13.2 Stop conditions

The plan stops, and the gap is recorded with the exact boundary, if any of
these holds:

- WP1's artefact cannot close the loop end to end.
- Closing it would require a wire-format change.
- Closing it would require a new dependency.
- Closing it would require weakening Plan 491's version gate or Plan 505's
  local-condition carve-out.
- Closing it would require holding two locks at once, or a lock across an
  await.
- Closing it would require composing `NetDbTunnelCoordinator` (Plan 433's
  blocked scope).
- Closing it would require adding eviction to the destination store, or
  changing the projection's ordering.

### 13.3 What this plan does **not** close

- **A RouterInfo-kind NetDB lookup.** The router performs none in production
  (§3.3), so the refreshed population is the unsolicited stores the network
  sends. Adding a lookup is its own plan-of-record.
- **Plan 497 limitation 5's read half.** The reconcile makes the profile
  store live and populated; the routing table's only reader remains Plan
  433's never-constructed `NetDbTunnelCoordinator`.
- **The projection's ordering.** The hash-ordered 256-prefix stays as it is;
  this plan records the behaviour change, it does not redesign the ordering.
- **The stock-peer question.** Whether an ordinary public router sends
  unsolicited stores the router will accept is unproven.
- **Plan 487 F3's destination-inbound half.** It needs tunnel rekeying, which
  the tree does not implement at all.

## 14. Closure evidence required

1. The WP1 feasibility artefact, with its measured answer.
2. The requirement-to-evidence matrix, with every requirement mapped to a
   `file:line` and to an executable check.
3. The full routine floor, green, with the exact command list and the step
   count.
4. The focused verification table.
5. The invariant, failure, migration and security reviews.
6. The limitations, the findings by severity, and the disposition of the
   startup-only limitation Plan 505 recorded.
7. The unblock audit.

## 15. Handoff notes

**The duplicate ingest is a finding in its own right.** It is fixed here, but
a reviewer should know that the unsolicited store path wrote every accepted
store twice.

**The selection-set change is the risk to watch.** A refresh can change *which*
peers the build path sees, because the projection is a hash-ordered prefix. If
that proves undesirable in practice, the fix is a separate plan that changes
the projection's ordering — not a tweak to this one.

**Do not run the external lanes under this plan's number.** The stock-peer
question stays open until Plan 493's authorized topology is available.

**The next candidate after this one** depends on what WP1 measures. If the
reconcile closes the loop, the next candidate is a **RouterInfo-kind NetDB
lookup** — the router would then *ask* for peers instead of waiting to be
told about them, which is the other half of the population story.
