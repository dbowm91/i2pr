# Plan 489 — Floodfill-indexed Kademlia routing table, exploration, and the optimized lookup

**Status:** **closed as `passed-floodfill-indexed-routing-table`** — see
`plans/closure/network-participation/489-status.md` (implementation
`40a15a87`). The body below is preserved as registered.
**Owner:** `i2pr-netdb` (the bucket set, the exploration vocabulary, the
optimized lookup), `i2pr-daemon` (the exploration driver, the lookup
composition, the receipts)
**Baseline:** `work/plans-477-480-router-recovery@c43f3404`
**Authoritative predecessor:** `plans/closure/netdb/106-status.md`,
`plans/closure/netdb/105-status.md` (the lookup state machine), and Plan
488's peer profile store
**Research input:** the I2P specification's NetDb page (the date-keyed
search key, the floodfill-indexed keyspace, the lookup and search-reply
semantics), the pinned i2pd 2.61.0 and Java I2P 2.13.0 NetDb behaviour, and
the Emissary router's routing table and query implementation (as a design
reference only — ADR 0028 §7 excludes its source from reuse on this branch)
**Predecessor lineage:** 102–106 (NetDb foundation) → 487 → 488 (peer
profile store) → **489**

## Objective

Give the router the **DHT spine**: a bounded Kademlia **bucket set** that
indexes *floodfill* peers by their distance to a date-keyed search key, an
**exploration job** that grows and refreshes that set, and an **optimized
iterative lookup** that draws its candidates from the set — so that the
router can find a peer, and a peer can find the router, through the
network's own database rather than only through a reseed, a cache, or a
test harness.

One bounded outcome: **the router maintains a floodfill-indexed routing
table, explores it, and completes a DHT lookup through it.** The table is a
protocol-layer object; the exploration job and the lookup are daemon-driven;
the capability is internal and non-advertised.

### The three design decisions this plan must get right

1. **The keyspace is indexed by floodfills.** In I2P the Kademlia structure
   is over the *floodfill* set, not over every router; a table of "all
   peers" is the wrong structure, not merely a premature one. The table is
   seeded from Plan 488's store, filtered to the floodfill set.
2. **The search key is date-keyed.** The key is
   `SHA256(key || UTC_yyyyMMdd)`, so the keyspace rotates daily. The
   repository already has the derivation — `format_daily_key` in
   `crates/i2pr-netdb/src/routing.rs` — so this plan reuses it rather than
   inventing a second key derivation.
3. **The lookup is optimized and iterative, never recursive.** The
   repository's existing `RouterInfoLookup` engine is already an iterative
   state machine with a deadline, a peer budget, duplicate coalescing and
   suggestion merging; this plan supplies the *optimized* selection on top
   of it (query the closest known floodfills first; resolve a peer's
   RouterInfo before querying it when it is not yet known), and does **not**
   introduce a recursive lookup that would later have to be removed.

## Why ready

- **Hard dependencies are closed.** The NetDb foundation (Plans 102–106) is
  closed: the routing primitives, the daily routing-key derivation, the
  bounded store, and the iterative lookup state machine. Plan 488 supplies
  the peer store, the tiers and the score this plan's table and selection
  consume.
- **Interface dependencies are stable.** `LookupAction`, `LookupPolicy`,
  `ReplyPathSink`, `RouterInfoStore`, `format_daily_key`, and the
  `DatabaseLookup`/`DatabaseSearchReply`/`DatabaseStore` codecs are closed
  surfaces; this plan extends them additively.
- **No external topology is needed.** The phase is offline- and
  private-mesh-testable.

## Current implementation evidence

Present today:

- `crates/i2pr-netdb/src/routing.rs` — XOR distance, deterministic
  tie-breaking, floodfill filtering, bounded nearest-N selection, and
  `format_daily_key` (the date-keyed search key).
- `crates/i2pr-netdb/src/store.rs` — the bounded validated-RouterInfo store
  (which Plan 488 layers the profile store over).
- `crates/i2pr-netdb/src/lookup_engine.rs` — the iterative lookup state
  machine: `RouterInfoLookup` as a step-wise driver, `handle_response` and
  `handle_delivery_outcome` ingestion, deadline, peer budget, duplicate
  coalescing, suggestion merging, and the typed `LookupAction` output.
- `crates/i2pr-netdb/src/lookup_policy.rs`, `lookup_id.rs`,
  `databaselookup.rs`, `lookup_action.rs`.
- `crates/i2pr-daemon/src/netdb_tunnels.rs` — `NetDbTunnelCoordinator`:
  `begin_tunnel_lookup`, `compose_lookup_via_tunnel`, `ingest_search_reply`,
  `ingest_tunnel_store`, `lookup_delivery_outcome`, `candidate_hashes`,
  `floodfill_count`.
- `crates/i2pr-daemon/src/outbound_lookup.rs` — the
  `LookupAction::SendDatabaselookup` → I2NP envelope → outbound OBGW
  dispatch path.
- `crates/i2pr-netdb/src/floodfill_service.rs` — the controlled floodfill
  service, which already *answers* exploration requests and lookups.

Missing today:

- **no bucket set** — the distance primitives exist, the table that uses
  them does not; "Kademlia" appears in the tree only in a Java interop
  test;
- **no exploration job** — `exploration` appears in `i2pr-netdb` only
  inside the floodfill *service* (the answering side), never as a
  client-side job;
- **no live lookup** — `specs/support.toml` records
  `netdb.live-routerinfo-lookup = "deferred"`; the lookup engine has never
  been driven by a table the router maintains itself;
- **no optimized selection** — the engine's candidate set comes from the
  caller, not from a table.

## Invariants that must not regress

1. **Runtime neutrality.** `i2pr-netdb` gains no `tokio::*`, no `std::net`,
   no `std::fs`, no `async fn`.
2. **Bounded everything.** The bucket count, the per-bucket capacity, the
   total table size, the exploration peer budget, the exploration deadline,
   the outstanding-request table and the reply cache are bounded by named
   constants, with a release path on every drop path.
3. **The existing lookup contract.** `LookupAction`'s surface, the
   deadline, the peer budget, the duplicate coalescing and the suggestion
   merging are not weakened; a lookup that used to fail closed must still
   fail closed.
4. **Exact-consumption decodes and checked arithmetic** for every
   message-derived value; a malformed reply is a typed error, never a panic
   and never a silent drop.
5. **Only floodfills are indexed.** A peer that does not advertise the `f`
   capability never enters the table, even if it is otherwise well
   connected.
6. **The answering side does not change.** The controlled floodfill service
   stays exactly as it is; making `i2pr` answer lookups as a general
   participant is Plan 494's decision.
7. **No advertisement.** Nothing here changes `router.version`, `caps`, or
   any `specs/support.toml` advertisement.

## Scope

**In:**

- the **bucket set**: the Kademlia structure over the floodfill set, with
  XOR-distance ordering within a bucket, deterministic eviction, and a
  bounded total;
- the **date-keyed rotation**: the daily key's table and the retention rule
  that drops the previous day's table;
- the **exploration job**: request construction, response ingestion, the
  peer budget, the deadline, and the insertion it drives;
- the **optimized lookup**: the candidate selection over the table, the
  "resolve the RouterInfo first when it is unknown" split, and the
  continuation of the existing engine;
- the daemon-side driver and the receipts;
- persistence of the table across a restart, bounded and sanitized;
- the offline controls and the private-mesh lane.

**Explicitly out:**

- the **peer profile store** (Plan 488) — this plan consumes it;
- the **SSU2 introducer role** (Plan 490);
- **API-version floors and peer eligibility** (Plan 491);
- **transit participation** (Plan 492);
- the **floodfill role** and any change to the answering side (Plan 494);
- **public-network** operation, any public bind, and any advertisement;
- **NTCP2** activation (Plan 435) and **IPv6**;
- a **recursive** lookup, and the replacement of the closed
  `RouterInfoLookup` engine.

## Required production changes

- **`crates/i2pr-netdb/src/bucket_set.rs`** (new): `BucketSet` over
  `RouterHash` keys, with the bucket count and per-bucket capacity as
  named constants, XOR-distance ordering, deterministic eviction
  (furthest-first with a documented tie-break), and `insert`, `select`
  and `retain` operations that cannot exceed their bounds.
- **`crates/i2pr-netdb/src/exploration.rs`** (new): the exploration
  vocabulary — the exploration request key, the reply's peer suggestions,
  the budget/deadline type, and the bounded outstanding-request table.
- **`crates/i2pr-netdb/src/optimized_lookup.rs`** (new): the optimized
  selection over the bucket set and Plan 488's store, including the
  "queryable" (RouterInfo known) versus "pending" (RouterInfo must be
  resolved first) split, and the closest-first target choice.
- **`crates/i2pr-daemon/src/netdb_participation.rs`** (new, or extended
  from Plan 488's module): the runtime-facing driver that runs the
  exploration job, drives the lookup through the existing
  `NetDbTunnelCoordinator` and `outbound_lookup` paths, and emits the
  receipts.
- **`crates/i2pr-daemon/src/config.rs`**: a `[netdb]` section with the
  routing-table and exploration keys, both defaulting to **off**, plus the
  bounded size and budget constants as named, documented keys. The section
  is `deny_unknown_fields` like every other section.
- **`crates/i2pr-netdb-persist`**: optional persistence of the table, in
  the same bounded and sanitized shape the NetDb persistence already uses.

## Ordered work packages

1. **WP1 — vocabulary, bounds and invariants.** The bound constants, the
   typed errors, the wiring to the existing `format_daily_key`, and the
   invariant set as executable assertions. Negative tests: a bucket over
   capacity, a table over capacity, a non-floodfill peer offered for
   insertion, a reply with a peer that is not a RouterInfo.
2. **WP2 — the bucket set.** Insertion, distance ordering, deterministic
   eviction, and the bounded select. Offline controls cover capacity 1,
   exact load, max+1, and release on every drop path.
3. **WP3 — the daily rotation.** The date-keyed table, the rotation at the
   UTC day boundary, and the retention rule. A restart must not resurrect an
   expired table; a clock jump must not grow the table past its bound.
4. **WP4 — the exploration job.** Request construction, response
   ingestion, the peer budget, the deadline, and the insertion it drives.
   The job is cancel-safe: a cancelled exploration leaves no half-inserted
   state and no dangling outstanding entry.
5. **WP5 — the optimized lookup.** The candidate set from the table, the
   queryable/pending split, the closest-first choice, and the continuation
   of the existing engine — with the table's suggestions feeding the
   engine's existing suggestion merging.
6. **WP6 — restart continuity.** The table survives a restart in bounded,
   sanitized form, and a restart mid-lookup leaves no outstanding entry that
   a later generation would match.
7. **WP7 — the lane and the evidence.** The private-mesh lane: `i2pr` as a
   participant against a stock peer, asserting source-originated receipts
   for table growth, an exploration answered, a lookup answered by a peer,
   and the answered lookup's result reaching the caller.

## Failure, cancellation, restart, and contention semantics

- **A malformed or hostile reply** is a typed error and a counted drop; it
  never inserts a peer, never grows the table, and never panics.
- **A cancelled exploration** releases its outstanding-request entry and
  leaves the table at a consistent size; the budget is consumed, not
  refunded.
- **A restart** reloads a bounded table, drops the outstanding-request
  table entirely, and issues **no** reply-matching across the restart
  boundary (the lookup id generation must be per-process, or the
  outstanding table must carry a generation tag — the plan must choose one
  and prove it).
- **Contention**: the table has a single writer (the daemon-side driver);
  readers take a bounded snapshot rather than holding a lock across an
  `.await`. The recorded lock order (**bridge → coordinator**) still holds.
- **A transport that cannot deliver** produces a delivery outcome, not a
  retry loop; the lookup's peer budget and deadline remain the only bounds.

## Compatibility and migration

- **No wire change.** The messages used are the existing
  `DatabaseLookup`/`DatabaseSearchReply`/`DatabaseStore` codecs; the
  exploration request is an existing `DatabaseLookup` with the exploration
  flag and a random search key, exactly as the specification describes.
- **No `router.version` change.**
- **No migration.** The table is new state; a router that never runs
  exploration starts with an empty table and behaves exactly as it does
  today.
- **Persistence is additive and optional**: a corrupt or oversized persisted
  table is discarded with a counted receipt, never a startup failure.

## Required tests

**Offline controls** (in `crates/i2pr-netdb` and `crates/i2pr-daemon`):

- bucket-set capacity 1, exact load, max+1; eviction determinism; a
  release path on every drop path;
- only floodfill peers are indexed; a non-floodfill peer offered for
  insertion is rejected and counted;
- daily rotation; an expired table is not used;
- exploration budget and deadline: a job that exceeds either stops and is
  counted;
- the optimized selection: the closest-first choice, the queryable/pending
  split, duplicate coalescing and suggestion merging still hold with
  table-sourced candidates;
- malformed, truncated, oversized and semantically invalid replies: typed
  error, no insertion, no growth;
- restart continuity: a bounded table reloads, a corrupt one is discarded
  with a count, and no reply matches across a restart;
- an inverted probe for each invariant (each must fail when the invariant is
  broken).

**Private-mesh lane**: `i2pr` and a stock peer in a controlled loopback
mesh, asserting source-originated receipts — not aggregate counters — for
table growth, an exploration answered, a lookup answered by the peer, and
the result reaching the caller.

**Exact-pinned single-family lane**: the pinned i2pd 2.61.0, with a
deterministic peer selection and only sanitized counts and hashes in the
evidence.

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

- `specs/support.toml` — the `netdb.live-routerinfo-lookup` entry moves from
  `deferred` only when the lane has passed; this plan's status key is added;
- `specs/CONFORMANCE.md` — the NetDb evidence entries and the
  mixed-router-interoperability status;
- `docs/architecture/i2pr-netdb.md` — the bucket set, the daily rotation,
  the exploration job, the optimized selection;
- `docs/architecture/i2pr-daemon.md` — the new `[netdb]` configuration and
  the driver;
- `docs/architecture/overview.md` — the capability snapshot line for the
  routing table;
- `plans/subsystems/network-participation-roadmap.md` and
  `plans/registry.md` — the phase's row.

## Acceptance criteria

1. The bucket set honours its bounds under adversarial insertion, with the
   release path proven on every drop path, and indexes **only** floodfill
   peers.
2. The daily rotation happens at the UTC day boundary and never retains an
   expired table.
3. The exploration job completes under its budget and deadline, and stops —
   counted — when either is exhausted.
4. A lookup is issued whose candidates come from the table, is answered by
   a peer, and the answer reaches the caller; the reply path is the
   existing engine, unmodified in shape, and no recursive lookup exists.
5. A restart preserves a bounded table, discards a corrupt one with a
   count, and matches no reply across the restart boundary.
6. The offline controls pass, and each invariant has an inverted probe.
7. The private-mesh lane passes with source-originated receipts, and the
   evidence file contains only counts and digests.
8. The routine floor is green on the closing tree, with the step table in
   the closure record.
9. The closure record states that the router is still **not** a public-network
   participant: a routing table and a lookup are the *means*, not the
   participation.

## Stop conditions

Stop and record a `stopped-*` closure when:

- the routing table cannot be made bounded without weakening the existing
  lookup contract;
- the exploration job cannot be made cancel-safe and budget-bounded;
- the optimized selection cannot be built on the existing engine without
  replacing it;
- the private-mesh lane cannot distinguish a table-driven lookup from a
  hand-supplied peer, i.e. the evidence would not prove the table is used;
- the lane would need a non-loopback bind, an external topology, or a public
  network;
- any step would require editing `router.version`, `caps`, the reference
  pins, or a boundary script to go green;
- the attempt budget is spent without a material change.

## Closure evidence required

- `plans/closure/network-participation/489-status.md` with the full closure
  content the repository requires;
- the lane's evidence file;
- the registry and roadmap rows advanced to closed;
- the unblock audit recorded, with any newly ready plan moved to `ready` in
  the same commit.

## Handoff notes

- **The table is not the lookup engine, and the engine is not to be
  replaced.** Do not introduce a recursive lookup; the specification's
  lookup semantics and the repository's closed iterative engine stand.
- **Do not touch the floodfill service.** The controlled service already
  answers lookups and exploration requests; making `i2pr` answer them as a
  general participant is Plan 494's decision.
- **Do not advertise.**
- **Reference comparison.** The reference Rust router's routing table
  (`NUM_BUCKETS = 256`, libp2p-derived) and its optimized query
  implementation are the closest precedent for *shape*; its source is out
  of bounds on this branch under ADR 0028 §7. i2pd 2.61.0 and Java I2P 2.13.0
  remain the behavioral oracles for the protocol semantics, and the daily
  key derivation this plan reuses is already in the repository.
