# Plan 494 — Floodfill role and the `caps=f` advertisement

**Status:** registered, **blocked** on Plan 493's two-family evidence —
phase 7 (the last) of
[network participation](../../subsystems/network-participation-roadmap.md))
**Owner:** `i2pr-daemon` (the role owner and its lifecycle),
`i2pr-netdb` (the store, the replication, the expiration), `i2pr-netdb-persist`
(the persistence), `i2pr-client` (the publication target selection)
**Baseline:** `work/plans-477-480-router-recovery@c43f3404`
**Authoritative predecessor:** `plans/closure/floodfill/285-status.md` (the
controlled floodfill record and its pinned `router.version`), Plan 283's
third-class evidence strategy, Plan 278's exact-pinned lane, and Plan 306's
reachability-caps boundary
**Research input:** the I2P specification's NetDb and floodfill requirements
(the store/lookup/expire rules, the `f` capability, the health-loss
withdrawal), the pinned i2pd 2.61.0 and Java I2P 2.13.0 floodfill behavior
**Predecessor lineage:** 270–285 (the controlled floodfill service) → 488 →
489 → 491 → 492 → 493 → **494**

## Objective

Turn the **controlled** floodfill service into a **role**: a router that
stores, serves, replicates and expires the network database, advertises the
`f` capability in its RouterInfo, and **withdraws that capability when its
health fails** — with the advertisement gated on the two-family evidence
Plan 493 produces.

One bounded outcome: **the daemon runs the floodfill role under an explicit
opt-in, serves correct bounded NetDb data, and advertises `caps=f` only
while healthy and only after the two-family evidence exists.**

### The `f` capability is the last thing, not the first

`caps=f` is the network's statement that this router is a database server.
It is a promise about *other routers' data*. It is therefore the most
expensive claim in the repository, and it stays last:

- the controlled floodfill service already exists and passed its
  controlled-evidence strategy (Plan 283) and an exact-pinned lane (Plan
  278);
- the `f` capability is currently **refused** by the builder unless a
  controlled activation permit exists, and the `router.version` declaration
  is pinned to the implemented surface by a test (Plan 285);
- the advertisement may only follow the two-family evidence (Plan 493), and
  the M12 completion definition in the floodfill roadmap lists the whole
  set of conditions, including "health loss removes caps=f and stops
  admitting new floodfill work" and "normal-daemon floodfill remains
  opt-in and default-off".

## Why ready (when 493 passes)

- **Hard dependencies:** Plan 493 closed (two-family evidence for the
  advertised surface), Plan 285 closed (the controlled record and the pinned
  version), Plan 306's caps boundary closed, Plan 454's governor landed
  (the tier derivation), and Plans 488/489/491 for the store, the table and
  the DSM target selection.
- **Interface dependencies:** the `f`-capability builder path, the
  controlled-activation permit, the reachability tracker, and the
  `BandwidthClass` tiering are closed surfaces.

## Current implementation evidence

Present today:

- `crates/i2pr-netdb/src/floodfill_service.rs` — the controlled floodfill
  service, including the exploration-request handling.
- `crates/i2pr-netdb/src/server_store.rs` — the server-side NetDb store.
- `crates/i2pr-netdb/src/replication.rs`, `store_message.rs`,
  `floodfill_role.rs` (the eligibility and advertisement authority
  vocabulary: `Disabled`, `Eligible`, `Activating`, `Active`, `Draining`,
  `Failed`), `provenance.rs` (the namespace/provenance rules).
- `crates/i2pr-daemon/src/floodfill.rs` — the daemon's floodfill surface,
  currently an **intent** with a default-off configuration and no
  advertisement.
- `crates/i2pr-netdb/src/local.rs` — the `f`-capability builder path, the
  tiered `caps` construction with the pinned Java floor, and the refusal of
  a below-floor class; `CONTROLLED_ROUTER_VERSION = "0.9.62"` with its
  pinning test.
- `crates/i2pr-daemon/src/bandwidth_eligibility.rs` — the tier derivation
  from the governor's permit and its revalidation at the point of use.

Missing today:

- **no role**: no lifecycle from `Eligible` through `Activating` and
  `Active` to `Draining` and `Failed` in the daemon;
- **no advertisement**: `caps=f` is refused, and the support inventory
  records `m12_caps_f_advertised = false`;
- **no health-driven withdrawal**: nothing removes `f` when the router's
  health fails;
- **no store/lookup/expire/replication evidence** at the level the M12
  completion definition requires (the controlled lane covered a controlled
  subset);
- **no bandwidth metering** — the tier is derived from the governor's
  permit rather than from a measured capacity, which is the same gap the
  reference implementations still carry.

## Invariants that must not regress

1. **Opt-in and default-off.** The floodfill role is never on by default.
2. **The advertisement is gated on evidence.** `caps=f` may not be
   published unless the two-family evidence exists; the builder's refusal
   path stays in place and stays tested.
3. **The declared version is not edited.** `CONTROLLED_ROUTER_VERSION` stays
   pinned to the implemented I2NP surface.
4. **Health loss withdraws the capability**, promptly and observably, and
   stops admitting new floodfill work.
5. **Bounded store, replication and expiry.** The stored database's size,
   the replication volume, the expiry windows, the throttling, and the
   amplification controls are all bounded, with a release path on every
   drop path.
6. **Provenance and namespace rules hold.** The router must not disclose a
   client's NetDb data, and the stored records are validated (signature and
   freshness) before they are served or replicated.
7. **No secret in receipts.** Counts, classes, and hashes only.
8. **The two enforcement points from Plan 491 apply**: DSMs are sent only
   to floodfills at or above the DSM floor.

## Scope

**In:**

- the **role lifecycle**: `Eligible → Activating → Active → Draining →
  Failed`, driven by the health signals the router already has;
- the **store/lookup/expire/replication** behavior at the level the M12
  completion definition requires, verified in a controlled mixed-router
  testnet;
- the **advertisement**: `caps=f` published while healthy, withdrawn on
  health loss, with the withdrawal observable;
- the **tier derivation** and its refusal below the pinned Java floor;
- the **receipts** and the evidence;
- the offline controls and the controlled-mesh lane.

**Explicitly out:**

- **bandwidth metering** — the tier is derived from the governor's permit;
  a real metering implementation is a separate, larger piece of work and
  is open in the reference implementations too. If the derivation cannot
  honestly produce a class, the role must not claim one;
- **the DSM floor constants** (Plan 491 owns them);
- **transit participation** (Plan 492);
- **public-network** operation;
- any change to the reference routers, the pins, or the boundary scripts;
- **the client's** NetDb disclosure boundaries beyond the provenance rules
  already in place.

## Required production changes

- **`crates/i2pr-daemon/src/floodfill.rs`**: the role owner — the
  lifecycle, the activation and withdrawal decisions, the health
  evaluation, and the receipts; the current intent-only surface becomes a
  gated owner.
- **`crates/i2pr-daemon/src/config.rs`**: the opt-in key (default off) and
  the bounded store/replication/expiry constants, named and documented; the
  section stays `deny_unknown_fields`.
- **`crates/i2pr-netdb/src/floodfill_role.rs`**: the state machine's
  transition rules, made total and bounded.
- **`crates/i2pr-netdb/src/replication.rs` / `store_message.rs`**: the
  validation-before-serve and validation-before-replicate rules, the
  throttling, and the amplification controls.
- **`crates/i2pr-netdb-persist`**: the floodfill store's persistence
  (Plan 285's `floodfill_records` surface extended to the role's needs).

## Ordered work packages

1. **WP1 — the lifecycle.** The state machine's transitions, the health
   inputs, the activation and withdrawal decisions, and the timing bound on
   the withdrawal. The refusal path (no evidence ⇒ no `f`) is tested.
2. **WP2 — the store and the serving rules.** Validation before serving,
   provenance and namespace rules, the bounded store, and the
   amplification controls.
3. **WP3 — replication and expiry.** The replication volume, the throttling,
   the expiry windows, the rollover, and the persistence and restart
   continuity.
4. **WP4 — the advertisement and the withdrawal.** The `caps=f` publication
   while healthy, the withdrawal on health loss, and the receipts for both.
5. **WP5 — the tier derivation.** The class derivation from the governor's
   permit, the refusal below the pinned floor, and the honest "unknown
   class" path when the derivation cannot be made.
6. **WP6 — the lane.** The controlled mixed-router testnet: another router
   looks up a resource through this floodfill, and the served data is
   correct and bounded.
7. **WP7 — the evidence and the docs.** The lane's evidence file, the
   support-inventory entries, the conformance rows, and the roadmap and
   registry rows.

## Failure, cancellation, restart, and contention semantics

- **A record that fails validation is never served and never replicated**;
  it is counted and dropped.
- **A health loss** withdraws the capability within the bound, stops
  admitting new floodfill work, drains what is in flight, and records the
  stop.
- **A cancellation** during replication leaves the store consistent and
  releases the replication budget.
- **A restart** reloads a bounded store, discards a corrupt one with a
  count, and starts **not** advertising until it has re-derived its health
  — a restart must not re-advertise `f` blindly.
- **Contention**: the role owner is single-writer; the store and the
  replication budget are updated under the owner's own synchronization; no
  lock is held across an `.await` that can block on another lock holder.
  The recorded lock order (**bridge → coordinator**) still holds.

## Compatibility and migration

- **No wire change** to the NetDb message formats.
- **No `router.version` change.**
- **No migration**: the role is new and off by default; the controlled
  service keeps behaving exactly as it does.
- **The advertisement is additive and reversible**: a router that enables
  the role publishes `f`; a router that disables it, or loses health,
  withdraws it.

## Required tests

**Offline controls**:

- the role does not start when the key is off, and the `f` capability is
  refused;
- the lifecycle's transitions, including the withdrawal within its bound;
- validation before serving and before replication; a malformed record is
  dropped and counted;
- the store, replication and expiry bounds, and the amplification controls;
- the provenance rules: a client's NetDb data is never served;
- a restart reloads a bounded store, discards a corrupt one with a count,
  and does not re-advertise blindly;
- the DSM floor applies to the publication targets;
- an inverted probe for each invariant.

**Controlled mixed-router testnet**: another router resolves a resource
through this floodfill, the served data is correct, and the withdrawal is
observed when the role is disabled.

## Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh --self-test
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test
bash scripts/check-floodfill-type5-serve.sh
bash scripts/check-els2-live-lane-evidence.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

## Documentation updates

- `docs/architecture/i2pr-netdb.md` — the role, the store, the
  replication, the expiry, the provenance rules;
- `docs/architecture/i2pr-daemon.md` — the role owner, the configuration,
  the lifecycle, the withdrawal;
- `specs/support.toml` — the `f`-capability entry, the floodfill status
  entry, and this plan's status key — **only** once the evidence exists;
- `specs/CONFORMANCE.md` — the floodfill evidence rows and the M12
  completion status;
- `plans/subsystems/floodfill-roadmap.md`,
  `plans/subsystems/network-participation-roadmap.md`, and
  `plans/registry.md` — the phase's rows.

## Acceptance criteria

1. The role is opt-in and default-off, and the `f` capability is refused
   without the activation evidence.
2. The lifecycle runs, and a health loss withdraws the capability within
   the bound, stops admitting new work, and is recorded.
3. Served and replicated records are validated; a client's NetDb data is
   never disclosed; the store, replication and expiry bounds hold under
   churn.
4. A restart does not re-advertise blindly, and a corrupt store is
   discarded with a count.
5. The controlled mixed-router testnet passes: another router resolves a
   resource through this floodfill, correctly.
6. The offline controls pass, and each invariant has an inverted probe.
7. The routine floor is green on the closing tree, with the step table in
   the closure record.
8. The support inventory and the conformance record state the achieved
   position truthfully — including the honest "unknown class" outcome if
   the tier cannot be derived.

## Stop conditions

Stop and record a `stopped-*` closure when:

- the store, replication or amplification controls cannot be made bounded;
- the withdrawal cannot be made prompt and observable;
- the role would have to advertise a class the tier derivation cannot
  honestly produce;
- the controlled testnet would need a public bind, an external topology, or
  a public network;
- any step would require editing `router.version`, the pins, the reference
  routers, or a boundary script to go green;
- the attempt budget is spent without a material change.

## Closure evidence required

- `plans/closure/network-participation/494-status.md` with the full closure
  content the repository requires;
- the lane's evidence file;
- the registry and both roadmaps' rows advanced to closed;
- the unblock audit recorded.

## Handoff notes

- **The last phase is the one that can hurt the network.** A floodfill
  that serves stale, unvalidated, or excessive data is worse than no
  floodfill at all. When in doubt, refuse to serve, and record it.
- **Do not claim a bandwidth class you cannot derive.** The reference
  implementations still have open metering work; `i2pr` is not behind on
  that, and an honest "unknown class" is a legitimate outcome.
- **Do not edit the declared version** to satisfy an admission gate; that
  gate failing is a finding.
- **The M12 completion definition is the checklist.** The floodfill
  roadmap's §11 already lists it — use it as the acceptance list, and do
  not close the milestone while any of its bullets is unproven.
