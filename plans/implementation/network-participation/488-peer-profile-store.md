# Plan 488 — Peer profile store: capacity tiers, failure detection, and the bounded selection policy

**Status:** registered, ready to execute (phase 1 of
[network participation](../../subsystems/network-participation-roadmap.md))
**Owner:** `i2pr-netdb` (the store, the tiers, the failure model, the
selection policy), `i2pr-daemon` (the driver and the receipts)
**Baseline:** `work/plans-477-480-router-recovery@c43f3404`
**Authoritative predecessor:** `plans/closure/netdb/106-status.md` (the NetDb
handoff) and `plans/closure/core-router-recovery/487-status.md`
**Research input:** the I2P specification's NetDb and transport pages (the
date-keyed search key, the floodfill-indexed keyspace), the pinned i2pd
2.61.0 and Java I2P 2.13.0 peer/profile behaviour, and the Emissary
router's `ProfileStorage` (as a design reference only — ADR 0028 §7 excludes
its source from reuse on this branch)
**Predecessor lineage:** 102–106 (NetDb foundation) → 487 (client product and
§6 recovery, closed 2026-10-20) → **488**

## Objective

Replace the flat, undifferentiated peer map with a **bounded peer profile
store**: a tiered set of known peers that records each peer's capacity
class, tracks whether it is failing or unreachable, carries a score, and
supplies the **selection inputs** the later phases need — which peers to
build tunnels through, which floodfills to query, and which peers to prefer.

One bounded outcome: **the router keeps a bounded, tiered peer store with
honest failure detection, and a pure selection policy draws bounded,
diverse candidate sets from it.** The store is internal and non-advertised.

### Why this is phase 1

The reference Rust router reached its DHT only *after* it had a peer store,
a profile store with capacity tiers, capabilities parsing, and tunnel-hop
selection rules; its commit history shows the Kademlia table arriving after
the floodfill mode, the exploration job, the profile store and the selection
rules — not before. The same order is correct here, for a second reason that
is specific to I2P: **the Kademlia keyspace is indexed by floodfills**, so a
routing table over "all peers" is not merely premature, it is the wrong
structure. A router needs to know *which peers are floodfills* and *which
peers are worth building through* before it can index anything. This plan
builds that substrate; Plan 489 builds the table on top of it.

## Why ready

- **Hard dependencies are closed.** The NetDb foundation (Plans 102–106) is
  closed: the validated RouterInfo store, the routing primitives, the daily
  routing-key derivation (`format_daily_key`), the floodfill filtering
  helper, and the bounded nearest-N selection all passed.
- **Interface dependencies are stable.** `RouterInfoStore`/`RouterInfoStoreConfig`,
  `ValidatedRouterInfo`, `RouterHash`, `BandwidthClass`, and the
  `RouterInfo` capability parsing are closed surfaces.
- **No external topology is needed.** The whole phase is offline- and
  private-mesh-testable.

## Current implementation evidence

Present today:

- `crates/i2pr-netdb/src/store.rs` — `RouterInfoStore`: a **flat** bounded
  map of validated RouterInfos with a record ceiling (default 4096) and a
  byte ceiling (default 4 MiB), a `f`-capability advertiser count, and
  `insert` as the only public insertion path (Plan 103 §3 forbids an
  unchecked bypass).
- `crates/i2pr-netdb/src/routing.rs` — XOR distance, deterministic
  tie-breaking, floodfill filtering, bounded nearest-N selection, and the
  daily routing-key derivation.
- `crates/i2pr-netdb/src/local.rs` — `BandwidthClass` and the tiered `caps`
  construction (the `f` capability is refused below the pinned Java floor).
- `crates/i2pr-proto/src/common/mod.rs` — the `RouterInfo` option and
  capability parsing.

Missing today:

- **no capacity tiers** — every retained peer is equal; the store has no
  notion of a high-capacity peer, a standard peer, or an untracked one;
- **no failure or unreachability tracking** — a peer that never answers is
  indistinguishable from one that was never tried;
- **no per-peer score** — nothing encodes "responsive but empty" versus
  "dead", so the store cannot prefer a peer that answers over one that does
  not;
- **no selection policy** — the tunnel-hop selection rules (no two peers
  from the same /16 in one tunnel, a shuffle before picking, a per-peer
  participation cap) do not exist in the tree;
- **no maintenance pass** — nothing ages the store, and nothing is
  periodically re-evaluated or dropped.

## Invariants that must not regress

1. **Runtime neutrality.** `i2pr-netdb` gains no `tokio::*`, no `std::net`,
   no `std::fs`, no `async fn`; the store, the tiers, the failure model and
   the selection policy are pure.
2. **Bounded everything.** The total peer count, the per-tier count, the
   per-bucket count, the total encoded bytes, the outstanding-probe table
   and the maintenance interval are all bounded by named constants, with a
   release path on every drop path.
3. **Validated insertion only.** The only insertion path consumes a
   `ValidatedRouterInfo`; an unvalidated RouterInfo can never enter the
   store, and the existing `insert_unchecked` prohibition is preserved.
4. **The store's existing ceilings hold.** The record ceiling and the byte
   ceiling keep their current defaults and their current semantics; the
   tiering is layered on top, not instead.
5. **No secret in receipts.** The store's receipts carry counts, tier
  names, and hashes only — never a peer's address, key, or session state.
6. **No advertisement.** Nothing here changes `router.version`, `caps`, the
   published RouterInfo, or any `specs/support.toml` advertisement.
7. **Determinism.** The same store contents and the same policy input
   produce the same selection, byte for byte, on every run.

## Scope

**In:**

- the **profile** type: a peer's capacity tier, its last-seen and
  last-answered state, its failure counters, and its score;
- the **tiering policy**: how a peer is classified (high capacity, standard,
  untracked) and how it is promoted or demoted;
- the **failure model**: a peer that does not answer is distinguished from a
  peer that was never tried, with bounded counters and age thresholds;
- the **score**: a bounded, explainable number, where a peer that answered
  but did not have the requested data ranks *above* a peer that did not
  answer at all;
- the **selection policy**: a pure function from the store to a bounded,
  diverse candidate set — no two peers from the same /16 in one selection,
  a shuffle before picking, and a per-peer participation cap;
- the **maintenance pass**: the periodic re-evaluation that ages peers and
  applies the drop rules;
- persistence of the profile across a restart, in bounded and sanitized
  form;
- the offline controls and the private-mesh lane.

**Explicitly out:**

- the **Kademlia routing table**, the exploration job and the lookup
  (Plan 489);
- the **SSU2 introducer role** (Plan 490);
- **API-version floors and peer eligibility** (Plan 491);
- **transit participation** (Plan 492);
- **floodfill** (Plan 494);
- changing the *tunnel* selection behaviour of the client's own tunnel
  building — this plan delivers the policy and its inputs; wiring it into
  the client is the consumer's business, and the wiring must not change
  behaviour until the policy has its own evidence.

## Required production changes

- **`crates/i2pr-netdb/src/profile.rs`** (new): `Profile` (tier, state,
  counters, score, last-seen), `ProfileTier`, `ProfileState`, the bounded
  score type, and the aging rules.
- **`crates/i2pr-netdb/src/profile_store.rs`** (new): `ProfileStore` — the
  tiered, bounded store over the existing `RouterInfoStore` contents, with
  `observe_*` events (answered, unanswered, refused, unreachable), the
  maintenance pass, and the persistence projection.
- **`crates/i2pr-netdb/src/selection.rs`** (new): the pure selection
  policy — `select_candidates(count, constraints) -> Vec<Candidate>` with
  the /16 diversity rule, the shuffle, and the participation cap.
- **`crates/i2pr-daemon/src/netdb_participation.rs`** (new): the
  daemon-side driver that feeds the store with observed outcomes and runs
  the maintenance pass on the daemon's clock, and emits the receipts.

## Ordered work packages

1. **WP1 — vocabulary and bounds.** The tier, state, score and counter
   types; the named bound constants; the invariant set as executable
   assertions. Negative tests: a score out of range, a counter overflow, a
   tier promotion without evidence.
2. **WP2 — the profile and the tiering.** Classification on insert, the
   promotion and demotion rules, and the store's integration with the
   existing validated-RouterInfo path. A peer must never be classified
   without a validated RouterInfo.
3. **WP3 — the failure model.** The distinction between "never tried",
   "tried and did not answer", and "tried and refused"; the bounded
   counters; the age thresholds; the drop rules.
4. **WP4 — the score.** The score's inputs, its bounds, and its
   explanation. The rule that a peer which answered a lookup without having
   the data is ranked **above** a peer which did not answer is a
   requirement, not an implementation detail.
5. **WP5 — the selection policy.** The pure function, the /16 diversity
   rule, the shuffle, the participation cap, and the bounded output size.
   The policy must be deterministic given the same input.
6. **WP6 — the maintenance pass and persistence.** The periodic
   re-evaluation, the drop rules applied, the bounded persisted projection,
   and the restart behaviour (a corrupt or oversized projection is
   discarded with a counted receipt, never a startup failure).
7. **WP7 — the lane and the evidence.** The private-mesh lane: a peer that
   answers, a peer that stops answering, and a peer that answers with the
   wrong answer; the store's observed behaviour must be visible in
   source-originated receipts, and the selection must demonstrably prefer
   the responsive peer.

## Failure, cancellation, restart, and contention semantics

- **A malformed or invalid peer record** is rejected at the existing
  validated-insertion boundary and never reaches the profile store.
- **A peer that stops answering** ages out through the maintenance pass
  and is demoted, then dropped at the threshold — it is never silently
  forgotten, and the transition is counted.
- **A cancelled maintenance pass** leaves the store at a consistent size;
  the pass is idempotent and re-runnable.
- **A restart** reloads a bounded projection, discards a corrupt one with a
  count, and starts with an empty peer set if the projection is unusable —
  it must never start with an unbounded or partially-applied state.
- **Contention**: the store has a single writer (the daemon-side driver);
  readers take a bounded snapshot rather than holding a lock across an
  `.await`. The recorded lock order (**bridge → coordinator**) still holds.

## Compatibility and migration

- **No wire change.** No new message, no new option, no new field.
- **No `router.version` change.** The constant stays pinned to the
  implemented I2NP surface.
- **No migration.** The profile store is new state layered over the
  existing store; the existing store's behaviour is unchanged, and the
  profile store is populated from what the router already learns.
- **Persistence is additive and optional**, in the same sanitized shape the
  NetDb persistence already uses.

## Required tests

**Offline controls** (in `crates/i2pr-netdb` and `crates/i2pr-daemon`):

- tier classification on insert; a peer cannot be classified without a
  validated RouterInfo;
- the failure model's three states and the transitions between them;
- the score's bounds and its ordering rule (answered-but-empty outranks
  did-not-answer);
- the selection policy: capacity 1, exact load, max+1; the /16 diversity
  rule (no two peers from the same /16 in one selection); the participation
  cap; determinism (the same input yields the same output);
- the maintenance pass: an aged peer is demoted and then dropped, and every
  drop path releases its state;
- persistence: a bounded projection round-trips; a corrupt or oversized one
  is discarded with a count; a restart starts from a consistent state;
- an inverted probe for each invariant (each must fail when the invariant
  is broken).

**Private-mesh lane**: a controlled mesh where one peer answers, one peer
stops answering, and one peer answers with the wrong data; the store's
observed state and the selection's preference must be visible in
source-originated receipts.

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
bash scripts/check-netdb-tunnel-evidence.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
python3 scripts/check-tooling-inventory.py
```

## Documentation updates

- `docs/architecture/i2pr-netdb.md` — the profile store, the tiers, the
  failure model, the score, the selection policy;
- `docs/architecture/i2pr-daemon.md` — the driver and the maintenance pass;
- `specs/support.toml` — this plan's status key, and the peer-store entry
  once the lane has passed;
- `specs/CONFORMANCE.md` — the NetDb evidence rows;
- `plans/subsystems/network-participation-roadmap.md` and
  `plans/registry.md` — the phase's row.

## Acceptance criteria

1. Every retained peer carries a tier, a state, bounded counters and a
   bounded score, and a peer is never classified without a validated
   RouterInfo.
2. A peer that stops answering is distinguished from one that was never
   tried, is demoted through the maintenance pass, and is dropped at the
   threshold with a count.
3. The score ranks a peer that answered without the data above a peer that
   did not answer, and the score stays within its bounds.
4. The selection policy returns a bounded, deterministic candidate set that
   honours the /16 diversity rule and the participation cap.
5. A restart reloads a bounded projection, discards a corrupt one with a
   count, and never starts in an inconsistent state.
6. The offline controls pass, and each invariant has an inverted probe.
7. The private-mesh lane passes with source-originated receipts; the
   evidence file holds counts and digests only.
8. The routine floor is green on the closing tree, with the step table in
   the closure record.
9. The closure record states that this phase adds **no** capability claim:
   the router is still not a participant until the later phases land.

## Stop conditions

Stop and record a `stopped-*` closure when:

- the tiers or the score cannot be made bounded and explainable;
- the failure model cannot distinguish "never tried" from "did not answer"
  without unbounded state;
- the selection policy cannot be made deterministic, or the /16 diversity
  rule cannot be satisfied within the store's bounds;
- the persisted projection cannot be made safe against a corrupt or
  oversized file;
- the lane would need a public bind, an external topology, or a public
  network;
- any step would require editing `router.version`, `caps`, the reference
  pins, or a boundary script to go green;
- the attempt budget is spent without a material change.

## Closure evidence required

- `plans/closure/network-participation/488-status.md` with the full closure
  content the repository requires (commits, requirement-to-evidence matrix,
  command outcomes, invariant/failure/migration/security reviews,
  documentation updates, limitations, findings by severity, roadmap
  disposition, unblock audit);
- the lane's evidence file;
- the registry and roadmap rows advanced to closed;
- the unblock audit recorded, with any newly ready plan moved to `ready` in
  the same commit.

## Handoff notes

- **Do not skip this phase because the routing table is the visible goal.**
  The table is phase 2 and it indexes floodfills; without the store's tiers
  and failure model, the table would index peers the router should not
  build through.
- **Do not wire the selection policy into the client's tunnel building in
  this phase.** The policy and its evidence come first; the wiring is the
  consumer's change and belongs to the transit phase's evidence.
- **Reference comparison.** The reference Rust router's `ProfileStorage`
  (capacity tiers, failure and unreachability thresholds, a maintenance
  interval, a backup interval) is the closest precedent and is worth
  studying for the *shape* of the store; its source is out of bounds on this
  branch under ADR 0028 §7, and i2pd 2.61.0 and Java I2P 2.13.0 remain the
  behavioral oracles for what a peer profile means to the network.
