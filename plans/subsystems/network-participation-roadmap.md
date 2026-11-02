# Network Participation Roadmap

Status: active (registered 2026-10-20)

**What this roadmap is.** `i2pr` is currently a *client-side* router: it builds
its own tunnels, publishes a signed RouterInfo, and serves SAM/I2CP/Streaming
locally — but it is not a participant in the I2P network. It has no DHT
routing table, no exploration job, no SSU2 introducer role, no floodfill role,
and its transit (tunnel-participation) machinery is implemented but closed to
ordinary profiles. This roadmap owns the phased work that turns `i2pr` into a
**network participant**, in dependency order, with every role gated on
evidence rather than on intent.

**What this roadmap is not.** It does not reopen the closed
[core-router-recovery](core-router-recovery-roadmap.md) line, does not re-run
the client-product work that Plan 487 closed, and does not own the
public-network *topology* gate (Plan 431/472/476/480) or the transport
activation line (Plan 434/435). It references them as hard dependencies.

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)
- `specs/protocols/` (protocol dossiers; the DHT, ELS2 and transport dossiers)

Related ADRs:

- `docs/adr/0041-network-participation-phasing-and-role-gating.md` — the
  sequencing and role-gating policy this roadmap implements.
- `docs/adr/0028-emissary-implementation-neutrality.md` — Emissary source is
  excluded from reuse on this branch and is a post-freeze behavioral oracle
  only.
- `docs/adr/0030-...` (reachability caps) and `docs/adr/0040-...` (stream
  recovery) — the advertisement and recovery authorities that the later phases
  must not contradict.

Research inputs (point-in-time, not planning authority):

- The I2P specification: [I2NP](https://i2p.net/en/docs/specs/i2np/)
  (message catalogue and the `router.version` obligation table, accurate for
  API 0.9.69), [RouterInfo](https://i2p.net/en/docs/specs/routerinfo/),
  [tunnel implementation](https://i2p.net/en/docs/specs/tunnel-implementation/),
  [ECIES tunnel creation](https://i2p.net/en/docs/specs/tunnel-creation-ecies/),
  [ELS2](https://i2p.net/en/docs/specs/els2/), [NTCP2](https://i2p.net/en/docs/specs/ntcp2/),
  [SSU2](https://i2p.net/en/docs/specs/ssu2/).
- Reference routers: [i2pd](https://github.com/PurpleI2P/i2pd) (pinned
  2.61.0) and [Java I2P](https://github.com/i2p/i2p.i2p) (pinned 2.13.0).
- [Emissary](https://github.com/altonen/emissary) — the closest production Rust
  I2P router, used here as a sequencing and design reference only (ADR 0028
  §7 excludes its source from reuse on this branch).

### What the reference sequence says (why the phases are in this order)

The closest Rust precedent reached its DHT only *after* it had a peer store
with capacity tiers, capabilities parsing and tunnel-hop selection rules:
its history shows the Kademlia table, the exploration job, the profile
store and the selection rules arriving in that order, with a
**floodfill-only** table and an **optimized** (never recursive) lookup.
Two things follow for this roadmap, and both are already reflected above:

1. the **profile store precedes the routing table** (Plan 488 before
   Plan 489), because a table of peers the router should not build through
   is worse than no table;
2. the **Kademlia keyspace is indexed by floodfills**, and the lookup is
   optimized and iterative on top of the repository's *existing* iterative
   engine — the closed `RouterInfoLookup` state machine is extended, never
   replaced, and no recursive lookup is introduced.

The same reference also shows what is genuinely hard and still open
elsewhere: real **bandwidth metering** is unresolved, which is why Plan
494's tier derivation is allowed to end in an honest "unknown class"; and
its **ELS2** support is absent, where `i2pr` is ahead. Where `i2pr`
diverges deliberately — the default-off participation posture, the
no-ML-KEM transport posture, the exhaustive-`unsafe` discipline, and the
role gating — the divergence is recorded rather than "corrected".

## 1. Purpose and ownership boundary

This roadmap owns the **router-role axis**: the parts of a router that exist
for the *network's* benefit rather than for the operator's applications.

Owned here:

- the **peer profile store** — a tiered, bounded set of known peers with
  honest failure detection and a score, and the selection policy that
  draws diverse candidate sets from it;
- the **DHT spine** — a Kademlia routing table indexed by *floodfills*, the
  date-keyed search key, the exploration job, and the optimized iterative
  `DatabaseLookup` path that uses them;
- the **SSU2 introducer (relay) role** — running the introducer side of the
  SSU2 relay sub-protocol, so that a firewalled peer can be reached through
  this router, and publishing the validated introducer set in the
  RouterInfo address;
- **peer eligibility by published API version** — honoring the minimum-peer
  floors the specification attaches to the `router.version` this router
  declares, and the per-peer version evidence those floors need;
- **transit participation** — turning the existing M11 machinery on for an
  operator-opted profile, with an explicit participation posture, share
  policy, and withdrawal semantics;
- the **two-family public qualification** that a role claim requires;
- the **floodfill role** and the `caps=f` advertisement, last.

Not owned here (referenced as dependencies, never duplicated):

- the public-network *topology* gate — Plan 431 (stopped), Plan 472/476/480
  (the authorized two-host runner), Plan 449 (the portable LAN lane);
- transport activation — Plans 434/435 (NTCP2/SSU2 public activation);
- the client-product surface — Plan 487 closed it (2026-10-20);
- ELS2 live cross-router evidence — the red25519/Proposal-170 lines
  (Plans 374/375/377/406/412/413);
- IPv6 interop — the SSU2 line's `ssu2_ipv6_interop` debt;
- managed-app runtime, console, service tunnels, outproxy — parallel
  subsystems with their own owners.

Ownership boundary in crates: `i2pr-netdb` and `i2pr-netdb-persist` own the
routing/exploration state; `i2pr-proto` owns the wire codecs; `i2pr-daemon`
owns the role owners and the runtime wiring; `i2pr-runtime` owns sockets and
timers. No phase may introduce a socket, a timer, or an unbounded queue into
a protocol crate.

## 2. Work classification

- **Capability** — the router can do something a network depends on: find a
  peer, answer a Relay Request as the introducer, participate in a tunnel, answer as a
  floodfill.
- **Infrastructure** — the routing table, the profile store, the eligibility
  evaluator, the participation governor: runtime-neutral, bounded, tested
  offline.
- **Invariant** — bounded queues, exact-consumption decodes, no unbounded
  retention, fail-closed role gates, no advertisement without evidence.
- **Polish** — documentation alignment, tooling inventory, lane hygiene.

Every phase in §7 is a **capability** phase resting on **infrastructure**
delivered inside the same phase (the repo's "vertical slice over horizontal
refactor" rule).

## 3. Non-goals

- **No anonymity or privacy claim.** `i2pr` remains experimental; a
  participating router is a *more* visible network position, not a safer one.
- **No `caps=f` before the two-family evidence exists.** The advertisement is
  the last phase, not the first, and `caps=f` stays refused by the builder
  until its gate passes (the invariant Plan 285 pinned).
- **No version declaration edited to satisfy a peer's admission gate.** The
  `router.version` constant is pinned to the implemented I2NP surface by a
  test; a peer that will not admit it is a finding, not a licence to edit.
- **No public bind, no non-loopback listener, no public-I2P use, no
  root/sudo/namespaces/containers/VM** in routine acceptance.
- **No Emissary source reuse** on this branch (ADR 0028 §7); it is a
  post-freeze behavioral oracle only.
- **No clearnet path.** The outproxy boundary (Proposal 170) is unchanged;
  no phase may add a direct socket, resolver, TLS client, plugin load, or
  process spawn.
- **No SSU1** and **no ML-KEM** in the transport posture; the declared
  transport surface is SSU2 (classical X25519) with NTCP2 owned by Plan 435.
- **No rewriting of closed records.** Historical closure records are
  immutable; corrections are made forward.

## 4. Current state

**Authority plan:** Plan 487 `passed` (2026-10-20), with Plan 478 closed
alongside it. `active_plan = none` in the registry.

The support inventory states the position precisely: of **89 surface
entries, 0 are advertised** — 83 `experimental`, 5 `deferred`, 1 `ready`
(`specs/support.toml`). The five deferred entries are
`i2np.tunnel-framing`, `i2np.garlic-data-framing`, `i2np.initial-bodies`,
`netdb.live-routerinfo-lookup`, and `tunnel.live-build`.

What exists today, and what is missing from it:

| Layer | Present | Missing |
|---|---|---|
| RouterInfo | signed build, publication to floodfills, `router.version` 0.9.62 pinned to the implemented I2NP surface (`crates/i2pr-netdb/src/local.rs`) | per-peer version tracking and the minimum-peer floors that 0.9.62 attaches |
| I2NP | the specification's 14 messages plus `TunnelTest` (231) (`crates/i2pr-proto/src/i2np/header.rs`) | nothing on the wire; the gaps are role-level, not codec-level |
| Peers | bounded in-memory store of validated RouterInfos, daily routing-key derivation, bounded nearest-N selection (`crates/i2pr-netdb/src/{store,routing}.rs`) | **no capacity tiers, no failure/unreachability detection, no score, no selection policy, no maintenance pass** |
| NetDb | iterative lookup state machine with a deadline, a peer budget, duplicate coalescing and suggestion merging (`crates/i2pr-netdb/src/lookup_engine.rs`) | **no bucket set, no exploration job, no optimized selection, no live lookup** (`netdb.live-routerinfo-lookup` is deferred) |
| Transports | SSU2 v2 with i2pd 2.61.0 direct interop; NTCP2 experimental and non-advertised | NTCP2 public activation (Plan 435); IPv6 interop debt |
| Roles | peer test (231), reachability caps, controlled floodfill service, transit ingress owner (`crates/i2pr-daemon/src/{peer_test,transit_owner,floodfill}.rs`) | **no SSU2 introducer role** (`ssu2.introducer_service` is refused at parse time; the transport and runtime state machines exist, the daemon never runs the role), **no ordinary-profile transit participation**, **no floodfill role** |
| Clients | SAM 3.1/3.3, I2CP, Streaming, I2PControl — all loopback-only and default-off | non-loopback/remote I2CP, TLS/auth (deferred, not owned here) |

The consequence: `i2pr` can be *in* the network as a client of it, and it can
be *tested* against the network in a controlled mesh, but it cannot yet be
*relied upon* by the network — it cannot be found by a peer, it cannot enter
without a reseed, and it cannot carry another router's traffic.

## 5. Target architecture

The participant state, in the order a router must acquire it:

```text
reseed / cache            -> a seed set of RouterInfos
peer profile store        -> tiers, failure state, score; the selection inputs
routing table (bucket set)-> floodfills, indexed by distance to a date-keyed search key
exploration job           -> fills the bucket set with the closest known routers
lookup                    -> a DatabaseLookup over the table, answered by a peer
introducer (relay) role   -> answers a Relay Request so a firewalled peer is reachable
peer eligibility          -> the published API version decides who may be used
transit participation     -> the router relays TunnelData for others, under a policy
floodfill role            -> the router stores and serves the NetDb, last
```

Layering rules that each phase must respect:

- the **routing table is a protocol-layer object**: bounded buckets, bounded
  per-bucket size, bounded total, deterministic eviction, no runtime, no
  sockets, no timers;
- **exploration is a bounded job** with a deadline and a peer budget, driven
  by the daemon, and it may never grow the table without bound;
- **role owners are daemon-side** and each one is separately gated, separately
  observable, and separately revocable — a floodfill that loses health must
  drop `caps=f` and stop serving, and a transit participant that loses its
  share policy must stop relaying;
- **advertisement follows evidence**: a role may be *implemented* before it is
  *advertised*, and the advertisement is the last step of the last phase.

## 6. Dependency graph

```text
487 (passed) -> 488 peer profile store (passed)
                   -> 489 floodfill-indexed routing table + exploration (passed)
                        -> 490 SSU2 introducer (relay) role (passed)
                             -> 491 API-version floors + peer eligibility (passed)
                                  -> 492 transit participation posture (passed)
                                       -> 493 two-family public qualification
                                            -> 494 floodfill role + caps=f

503 is a corrective on 491 and consumes 497's routing-table consumer; it
   does not change the substrate order below.

493 additionally depends on the core-router-recovery chain:
   431/472/476/480 (authorized two-host topology) -> 433 (live composition)
   435 (NTCP2/SSU2 public activation) -> 436/437/438 (selection + role classes)
492 consumes M11's passed one-family qualification (Plan 268) and the
   transit posture in Proposal 170/322.
490 consumes the SSU2 session owner (Plans 161/162) and the RouterInfo
   publication path (Plan 285's controlled record).
489 consumes the NetDb foundation (Plans 102–106) and Plan 488's store.
488 consumes the NetDb foundation (Plans 102–106) and the bounded
   validated-RouterInfo store.
```

488–492 are **offline-testable** in a private mesh: they need no external
topology and no public bind. 493 is **environment-blocked** on the authorized
two-host topology that Plan 431 requires; 494 is blocked on 493's evidence.

## 7. Milestones

One row per plan. `i2pr token` is the authoritative classification (newest
status wins on conflict); `state` is the codegg-registry projection.
Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 488 | closed | passed-peer-profile-store | `plans/implementation/network-participation/488-peer-profile-store.md` | `plans/closure/network-participation/488-status.md` (implementation `c796bb0f`) |
| 489 | closed | passed-floodfill-indexed-routing-table | `plans/implementation/network-participation/489-floodfill-indexed-routing-table-and-exploration.md` | `plans/closure/network-participation/489-status.md` (implementation `40a15a87`) |
| 490 | closed | passed-ssu2-introducer-relay-role | `plans/implementation/network-participation/490-ssu2-introducer-relay-role.md` | `plans/closure/network-participation/490-status.md` (implementation `8a170ae4`; floor-caught fixes `0f5ba473` and `431ece6e` committed separately) |
| 491 | closed | passed-api-version-floors-and-peer-eligibility | `plans/implementation/network-participation/491-api-version-floors-and-peer-eligibility.md` | `plans/closure/network-participation/491-status.md` (implementation `dc36e2d7`; floor-caught fixes `c195f363` and `27c20b9c` committed separately) |
| 492 | closed | passed-transit-participation-posture | `plans/implementation/network-participation/492-transit-participation-posture.md` | `plans/closure/network-participation/492-status.md` (implementation `d287635e`; floor-caught fixes `279f6ae4` and `f0e44798` committed separately) |
| 493 | blocked | registered-two-family-public-participation-qualification | `plans/implementation/network-participation/493-two-family-public-participation-qualification.md` | `plans/closure/core-router-recovery/495-status.md` (`passed-peer-eligibility-gate-and-first-failure-owner`) — the peer-eligibility gate and the first-failure-owner outcome taxonomy; the gate dependency is discharged and Plan 493 remains blocked on the authorized two-host topology and Plan 435 |
| 494 | blocked | registered-floodfill-role-and-caps-f-advertisement | `plans/implementation/network-participation/494-floodfill-role-and-caps-f-advertisement.md` | — |
| 497 | closed | passed-netdb-participation-consumer-phase | `plans/implementation/network-participation/497-netdb-participation-consumer-phase.md` | `plans/closure/network-participation/497-status.md` (implementation `7183f0fb`; the coordinator-level consumer is proven by the discriminator, and the production composition of the coordinator into the daemon's service graph stays Plan 433's blocked scope. **Plan 513 discharges M2 and limitation 1's read half:** the shared routing-table consumer now has a production reader — the live RouterInfo-kind lookup consults the table as a **preference**, never narrowing it, with every fallback counted by reason. The **stock-peer** half stays Plan 493's. **Plan 497's closure record is not rewritten.**) |
| 498 | closed | passed-ssu2-introducer-role-owner-in-the-live-inbound-loop | `plans/implementation/network-participation/498-ssu2-introducer-role-owner-in-the-live-inbound-loop.md` | `plans/closure/network-participation/498-status.md` (implementation `b362108c`; Plan 490 Medium #1 discharged, and Plan 490's prescription corrected — the live drain in `lib.rs` is the mandatory half; Plan 490 Medium #2 stays open) |
| 499 | closed | passed-introducer-live-tag-request-and-carriage | `plans/implementation/network-participation/499-ssu2-introducer-live-tag-request-and-relay-block-carriage.md` | `plans/closure/network-participation/499-status.md` (implementation `99bc4603`; **Plan 490 Medium #2 discharged** — the relay tag is now issued to the **handshake-authenticated** session peer and the produced block is carried onto a live session; the **live three-party UDP mesh** shows the introduction completing end to end) |
| 500 | closed | passed-lookup-peer-budget-enforcement | `plans/implementation/network-participation/500-lookup-peer-budget-enforcement.md` | `plans/closure/network-participation/500-status.md` (implementation `6b7fe052`; **Plan 497 L2 discharged** — the declared per-lookup peer ceiling is now enforced in the engine's single attempt site, narrowing **attempts, never candidates**) |
| 501 | closed | passed-transport-runtime-boundary-section | `plans/implementation/network-participation/501-transport-runtime-boundary-section.md` | `plans/closure/network-participation/501-status.md` (implementation `ac7c7897`; **Plan 490 Low #4 and Plan 497 L1 discharged** — the transport crates get a **normalised**, socket-keyed boundary section with an explicit address-value allow-set; the raw grep was fail-open for a grouped `use` and over-broad for an ungrouped one) |
| 502 | closed | passed-transit-owner-in-the-product-composition | `plans/implementation/network-participation/502-transit-owner-in-the-product-composition.md` | `plans/closure/network-participation/502-status.md` (implementation `de796b73`; **Plan 492 F2 discharged** — the daemon's existing transit data-plane owner is installed in the production composition behind the existing default-off `[transit] participation` opt-in, so an opted-in router admits and accounts for transit traffic instead of relaying nothing; advances **Plan 436 WP3** source-side only, and Plan 436 stays blocked on Plan 433. The M11 boundary rule is **strengthened** to guard the installed owner rather than a module name plus a no-op probe, and the script's own production scan is **unfail-opened**: 58 158 production lines across the daemon had been invisible to all seven pre-existing rules) |
| 503 | closed | passed-live-dsm-version-gate-and-peer-eligibility-receipt | `plans/implementation/network-participation/503-live-dsm-version-gate-and-peer-eligibility-receipt.md` | `plans/closure/network-participation/503-status.md` — **Plan 491 F1 discharged**. Plan 491's *second* obligation (*minimum floodfill peers will send DSM to*) is enforced **nowhere in production**: the router's two live `DatabaseStore` sends select floodfills by XOR distance alone (`crates/i2pr-daemon/src/service_product.rs:6513-6528` and `destination_tunnels.rs:912` → `i2pr-netdb/src/lookup_policy.rs:257`) and never consult `dsm_target_eligibility`, because the coordinator that enforces it (`NetDbTunnelCoordinator`) is never constructed — Plan 433 owns that composition and is blocked. The plan delivers one shared, runtime-neutral version-gated floodfill selection, gates both live DSM paths on it, and makes the live build gate and the live DSM gate each produce Plan 491's `VersionEligibilityReceipt` in a bounded live-owned log published through `InspectionHandles`. Advertises nothing |
| 504 | closed | passed-live-tunnel-liveness-in-the-product-composition | `plans/implementation/network-participation/504-live-tunnel-liveness-in-the-product-composition.md` | `plans/closure/network-participation/504-status.md` — **closed**. Plan 185's tunnel liveness scheduler had **never been constructed by the production daemon**: it was the only public module in the daemon crate with zero production consumers, so the router did not notice that a tunnel pair it depended on was dead and did not rebuild it. Plan 504 installs the scheduler in the product composition, drives it on the existing tick, routes the live inbound path through the existing liveness seam, and recovers a failed pair. The correlation is **authenticated** — the dispatcher carries the acknowledged-message id and the scheduler credits exactly the probe that carried it — and the **proven-pair gate** means a pair that has never answered is never torn down. Plan 185's closure record's claim that the daemon pump already calls that seam was false of the tree it described; Plan 185's record is **not** rewritten and the correction lives here and in the registry's row. The daemon installs **no exploratory tunnels** yet, so the scheduler tracks zero pairs today: installed and inert, not broken. Advertises nothing |
| 505 | closed | passed-exploratory-build-driver-in-the-product-composition | `plans/implementation/network-participation/505-exploratory-build-driver-in-the-product-composition.md` | `plans/closure/network-participation/505-status.md` — **closed**. Plan 504 installed the tunnel-liveness scheduler in the live product, but the **population it watches did not exist**: `ExploratoryBuildCoordinator::submit` had **zero production callers**, so the daemon installed no exploratory tunnels and the scheduler tracked zero pairs — installed and inert. The mechanism was complete; only its inputs were missing: `rebuild_exploratory` was already a complete first-build driver with the peer-selection step removed. Plan 505 installs the driver that names a peer through Plan 491's version-gated selection, builds through the single existing build path, installs the result, and records the outcome into the peer-profile store — which is the write side Plan 497 limitation 5 recorded as open, and which also makes the store non-empty for the first time in a live router. The outcome map is total over all fourteen coordinator variants and separates a **peer fault** from a **local condition**: the router's own faults are counted and produce no peer-profile write. It runs on the **existing** tick behind an operator opt-in that is **off by default** — because the transport cannot dial a public deployment's peers — and it does not widen the transport's loopback / private-LAN dial scope. Plan 497 limitation 5 is **partially** discharged: the store is fed, written and read, but the routing table's only reader remains Plan 433's blocked coordinator. Advertises nothing |
| 506 | closed | passed-peer-knowledge-reconcile | `plans/implementation/network-participation/506-peer-knowledge-reconcile.md` | `plans/closure/network-participation/506-status.md` -- **closed**. A peer the router accepts on a **live** session is known **only to the bootstrap NetDB**: the destination coordinator's authoritative store and the peer-profile store are seeded once at startup and never again, so the population Plan 505's driver draws from does not grow. The router does **not** learn peers from a NetDB lookup (a `DatabaseSearchReply` carries hashes only, and the router runs no RouterInfo-kind lookup) -- the live-learned RouterInfos are exactly the unsolicited `DatabaseStore`s. Plan 506 installs one bounded reconcile **owned by the product**, on the **existing** tick, that snapshots the bootstrap under its lock, releases it, then writes through the canonical `insert_validated_router_info` and `learn_all` -- counting every outcome by reason, with a capacity refusal counted and never fatal, and with the three locks taken in one order and never nested. It also removes the **duplicate ingest** (the unsolicited store was written twice per accepted message), and records the **selection-set change** as a finding: the destination projection is a hash-ordered 256-prefix, so a refresh can change *which* peers the build path sees. The cadence reuses the **existing** NetDB participation maintenance period, so there is one source of truth and no second configuration knob. Advertises nothing
| 507 | closed | passed-netdb-retention-policy | `plans/implementation/network-participation/507-netdb-retention-policy.md` | `plans/closure/network-participation/507-status.md` -- **closed** — the router's peer records must not outlive the router's own acceptance window, and the NetDB must turn over instead of freezing. `RouterInfoStore::prune` — the tree's own *explicit prune over silent eviction* operation — has **zero production callers**: its only caller is its own unit test. Both live stores grow without bound and freeze at their quota: the bootstrap NetDB is fed by every accepted unsolicited store and the destination coordinator's store by Plan 506's reconcile, and `RouterInfoStore::insert` refuses at the quota and never evicts — so once a store is full the router refuses every new peer **permanently**. The retention rule also **disagrees** with the acceptance rule: the router refuses a RouterInfo older than `DEFAULT_MAX_AGE` (24 h) yet retains one indefinitely. Plan 507 installs one bounded, product-owned prune on the **existing** participation cadence, reusing the tree's own acceptance window — **no new configuration key** — with the ordering *prune the bootstrap and snapshot under one lock, prune the destination store, then reconcile*, every prune counted by reason. It closes Plan 506's recorded *the destination store refuses at capacity and never evicts* limitation. **The plan's own ordering wording was corrected against what landed:** the plan of record said *prune the bootstrap and snapshot under one lock*; what shipped is *prune the bootstrap, prune the destination store, snapshot, insert, learn*, because Plan 506's reconcile takes its three locks one at a time and never nested. The window is the tree's **own** acceptance window, derived through a `const fn` so it cannot drift from the policy; there is **no new configuration key**; the prune **rides** the reconcile's existing cadence rather than adding a second step; the destination store's lock is **tried, never waited for**, and a prune that cannot run is **counted** under the stable reason label `prune-unavailable`; the peer-profile store is **not** pruned; and the router's **own RouterInfo** is never a victim. `i2pr-netdb` is **not** changed -- the plan adds a caller, not a mechanism. **The controls are not vacuous, and that is measured:** four mutations of the real tree, each applied, run against the real lanes, and reverted, failing nine, seven, seven and five tests; the tree was restored byte-for-byte after each. Advertises nothing |
| 508 | closed | passed-transit-creator-correlation-retention | `plans/implementation/network-participation/508-transit-creator-correlation-retention.md` | `plans/closure/network-participation/508-status.md` -- **closed** -- the transit owner's creator-correlation facts never expire: the three sets are `BTreeSet`s with only `insert`, and the production wiring installs them on **every** pump tick, so each build the router performs leaves one entry behind for ever. Plan 502 recorded this as Finding M1 and deferred the fix as *a protocol decision*. Plan 508 makes that decision on the record and implements it: the window is `BUILD_DEADLINE_MS + BUILD_TUNNEL_ID_RETENTION_MS` -- derived from the tree's own constants, so there is one source of truth -- and the expiry is checked at use time and reclaimed by a sweep on the existing tick. The router's own traffic is never claimed as transit work: that invariant is the reason the facts exist, and the window is exactly the bound that preserves it. The expiry is checked **at use time** and the memory is reclaimed by a **sweep** on the pump tick that already installs the facts, so the owner gains no second timer and no second task; the window is stored in seconds and every comparison rounds in the safe direction, because two of the four use sites carry seconds and two carry milliseconds. The coordinator's own fact set is **untouched** -- it was already bounded, and only the owner's copy was unbounded. **The controls are not vacuous, and that is measured:** four mutations of the real tree, each applied, run against the real lanes, and reverted, failing six, eleven, eleven and eight tests; the tree was restored byte-for-byte after each. Advertises nothing | **The closing floor failed, and the fix was made in the design, not in another plan's guard.** The creator-correlation check was first written as a raw map lookup, which removed the literal `creator_builds.contains` that Plan 492's dispatch-order guard anchors on, and the floor caught it on that plan's guard. The facts are now held by a `CreatorFacts<K>` wrapper whose membership test is clock-aware **and** is spelled `contains`, so Plan 492's guard is left **byte-identical** and green, and the expiry semantics live in one place instead of being re-implemented at four use sites; Plan 508's guard was rewritten against the wrapper and gained a **rule 11** that asserts the Plan 492 anchor, so a change that would break that guard is reported here first. The plan of record's production-change table describes the facts as `BTreeMap`s; what landed is the wrapper over a `BTreeMap` of expiries -- the correction lives in the closure record and here, and the plan of record is not rewritten.
| 509 | closed | passed-lookup-target-api-version-decision | `plans/implementation/network-participation/509-lookup-target-api-version-decision.md` | `plans/closure/network-participation/509-status.md` -- Plan 503 recorded as a Medium finding that *the router still asks a below-floor floodfill for a lookup*, and left it open because extending the floor to the lookup path would change which floodfills the router can ask at all. Plan 509 **decides** it: the lookup target is **not** version-gated. No floor is derivable -- the specification's table attaches exactly two duties to the `0.9.62` row, a lookup is neither, and a floor that cannot be derived may not be invented; the two existing floors encode peer-side duties that do not transfer to a lookup; the lookup's answer is a `DatabaseStore` the router **validates itself**, so the responder's version buys no safety; the lookup's rotation skips an unhelpful candidate at the cost of one round trip; and the two reference implementations **disagree** about a version rule for floodfills and neither gates a lookup. The plan delivers the decision on the record, a feasibility artefact that proves the three load-bearing claims against the real lookup path, and a static guard that enforces the decision and checks its premise (the lookup path stays ungated; the obligation table still carries no lookup row; `VersionGate` still has exactly two variants). **No production behaviour changes**, nothing is advertised, and Plan 503's closure record is not rewritten -- the finding is discharged in Plan 509's closure record and in this row. The decision is **the lookup target is not version-gated**, and the plan delivers it three ways: the decision on the record with six reasons traced to the tree; a feasibility artefact that drives the real lease-set2 lookup path and establishes that the responder's version decides nothing; and a static guard that makes the decision structural and checks its premise. **No production behaviour changed** and nothing is advertised. Plan 503's recorded finding is **discharged**, and Plan 503's closure record is not rewritten -- the discharge lives in Plan 509's closure record and here. |
| 513 | closed | passed-live-netdb-discovery-spine | `plans/implementation/network-participation/513-live-netdb-discovery-spine.md` |
| 514 | closed | passed-live-exploratory-request | `plans/implementation/network-participation/514-live-exploratory-request.md` | `plans/closure/network-participation/514-status.md` (implementation `ccc0a029`; **the exploratory request asks the closest floodfill, and the operator's switch over it is real** -- the peer the request is addressed to is the routing table's **closest** verified floodfill for the search key's daily routing key, not the flat store's first floodfill advertiser, which was the **lowest-hash** one and the same peer for every exploration; a table that names no floodfill **refuses** the request and counts it under its own reason, and the request is never redirected to an arbitrary peer. `netdb.exploration_enabled` is now **read** -- it was parsed, defaulted and cross-validated and read by nothing, so the exploration ran whenever the routing table was on. The switch gates the **exploratory request** only, so the RouterInfo-kind resolution stays above it and the routing table keeps its production reader; the switch's default does **not** move, because the key's own documentation already said the exploration is off by default. Plan 509's lookup-target decision is extended to the two live lookup paths Plan 513 added and to this plan's own selection, so it is enforced and not merely recorded. **Advertises nothing** -- `router.version` stays `0.9.62` and the inventory's advertised count stays zero. Plan 497's closure record is not rewritten and Plan 497's **M2** is not reopened) | |
| 515 | closed | passed-router-info-publication | `plans/implementation/network-participation/515-router-info-publication-refresh.md` | `plans/closure/network-participation/515-status.md` | The router publishes its own RouterInfo **once**, from the service-tunnel provisioning path, and never again; a router with no service tunnels never publishes at all, so its record goes stale and no peer can find it. The reference republishes on a timer (`ROUTER_INFO_PUBLISH_INTERVAL = 39 * 60` seconds, `libi2pd/RouterContext.h:34`), to the closest floodfill not yet asked (`libi2pd/RouterContext.cpp:1502`), with a `1600` ms resend burst (`libi2pd/RouterContext.h:37`). The plan adds a runtime-neutral publication schedule and a periodic owner, shares the existing version-gated selection with the one-shot publication, and counts every outcome by reason. **Advertises nothing** -- `router.version` stays `0.9.62` and the inventory's advertised count stays zero. Plan 514's closure record is not rewritten and Plan 514's **F1** is not reopened |
| 516 | registered: the router's publication asks for the network's acknowledgement | `plans/implementation/network-participation/516-router-info-publication-confirmation.md` | The publication's **correlation token never reaches the wire**: the correlator mints it (`crates/i2pr-netdb/src/publication.rs:253`), records it (`:294`), documents that the peer can correlate the acknowledgement with it (`:228-232`), and writes `reply_token: 0` into the message (`:276`). The acknowledgement itself already exists -- the floodfill service answers a direct-peer store that carries a nonzero token (`crates/i2pr-netdb/src/floodfill_service.rs:559`, `:611`) -- so this router's own floodfill service answers an acknowledgement this router's publication never asks for. The plan puts the token on the wire in the reference's direct-reply form, joins the live inbound path to the correlator, and counts a **delivery admission** and a **network acknowledgement** separately. **Advertises nothing** -- `router.version` stays `0.9.62` and the inventory's advertised count stays zero. Plan 515's closure record is not rewritten and Plan 515's **F1** stays open |

### Predecessor and dependency plans owned elsewhere

These are **not** owned by this roadmap; they are the hard dependencies its
phases consume, and their records remain authoritative where they live.

| Plan | Owner | Relationship to this roadmap |
|---|---|---|
| 102–106 (NetDb) | `netdb` | the store, lookup state machine and routing primitives Plans 488 and 489 extend |
| 161, 162 (SSU2) | `ssu2` | the session owner Plan 489's introducer role runs on |
| 268, 269 (M11 transit) | `transit-tunnels` | the passed one-family qualification Plan 491 turns into an opt-in posture |
| 270–285 (M12 floodfill) | `floodfill` | the controlled floodfill service Plan 493 promotes to a role |
| 431, 472, 476, 480, 449 | `core-router-recovery` | the authorized two-host topology Plan 493 needs; Plan 431 remains stopped |
| 433, 435, 436, 437, 438, 439 | `core-router-recovery` | live composition, transport activation, selection and role classes Plan 493/494 consume |
| 454 | `core-router-recovery` | the enforced bandwidth governor Plan 491's share policy and Plan 493's tier derivation use |
| 444 | `core-router-recovery` | the Java candidate-selection and store/lookup controls Plan 493's class matrix needs |
| 161–166 (ELS2 / red25519) | `red25519-encrypted-leaseset` | the encrypted-publication path Plan 489 and 493 serve |
| 487, 478 | `core-router-recovery` | passed 2026-10-20; the client-product and §6 recovery authority this roadmap does not reopen |

## 8. Cross-cutting requirements

- Storage, protocol, security, concurrency, observability and docs constraints
  per `GUARDRAILS.md`; the evidence model per `specs/CONFORMANCE.md`.
- Service/transport/API crates stay runtime-neutral; `i2pr-runtime` owns
  Tokio, sockets and timers; protocol crates gain no `tokio::*`, no
  `std::net`, no `std::fs`.
- Every queue, table and cache introduced by a phase is **bounded** in both
  size and age, with a named release path on every drop path.
- Every secret-bearing type keeps its no-`Debug`/no-`Display` discipline; no
  key material, token, tunnel id or payload enters a receipt or an evidence
  file.
- Listeners stay loopback by default; a non-loopback bind requires explicit
  configuration **and** an authorization design, which is Plan 431's gate, not
  a phase's decision.
- No capability, version, RouterInfo, I2NP, SAM, I2CP or transport
  advertisement beyond the tested subset; `specs/support.toml` is updated
  only by the phase that earns the entry.
- Reference pins are frozen: i2pd `635b013a612ff47278ef02acf8580a28e10e26c5`,
  Java I2P `9134f808337b401e8e53c73734c81fab04280c9d`, i2psam
  `b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac`, i2plib
  `6edf51cd5d21cc745aa7e23cb98c582144884fa8`.

## 9. Verification strategy

Every phase carries its own evidence lane, and the lanes are the same shape
the closed lines use: a **private controlled mesh** first, then an
**exact-pinned single-family** lane, then the **two-family matrix** last.

- **Offline controls** (every phase): unit and property tests for the
  bounded structures — capacity 1, exact load, max+1, and lease/release on
  every drop path; deterministic golden vectors; malformed, truncated,
  oversized and semantically invalid input tests; fail-closed assertions with
  inverted probes.
- **Private mesh lane** (488–492): a controlled multi-router loopback mesh
  where `i2pr` is a full participant alongside a stock peer, asserting
  *source-originated* receipts (not aggregate counters) for: table growth
  under exploration, a lookup answered by a peer, a Relay Request answered as the introducer, an eligibility decision taken from a peer's published version, and
  a relayed tunnel's traffic.
- **Exact-pinned single-family lane** (488–491): the pinned i2pd 2.61.0 as
  the peer, with a deterministic selection (no randomized peer choice), and
  only sanitized counts and hashes in evidence.
- **Two-family matrix** (493–494): Java I2P 2.13.0 *and* i2pd 2.61.0, as
  `specs/CONFORMANCE.md` item 7 requires for a router-to-router claim.
  Missing environment must **fail**, never silently pass.
- **Routine floor** on the closing tree of every phase, exactly as
  `AGENTS.md` specifies, with the step table recorded in the closure record.
- No `|| true`, no `continue-on-error`, no filename filtering, no fake peer
  environment, no broad exclusions, no early-return-success, and no
  production wire change made to go green.

## 10. Risks and decision points

- **The topology gate may never open.** Plan 493 depends on an authorized,
  independently addressed two-host topology that Plan 431 stopped awaiting.
  If it does not open, 493 and 494 stay blocked and the honest position is
  "a participating router in a controlled mesh", not "a router on the public
  network". The roadmap must not degrade that claim to keep the sequence
  moving.
- **Two-family evidence is the true gate, not the code.** A complete
  implementation with one family is still a conformance claim the repository
  forbids; expect the last phases to be slower than the first.
- **A participating router is a more exposed position.** Role work increases
  what an observer can see (a floodfill is queried by everyone; a transit
  participant's peers are visible). The anonymity roadmap's ADR 0030
  constraints continue to bind, and no phase may weaken them for the sake of
  a capability claim.
- **A peer that refuses our RouterInfo is a finding, not a version edit.**
  The `router.version` constant stays pinned to the implemented surface.
- **Bucket-set and exploration tuning is a load-bearing risk**: a table that
  grows unbounded, an exploration job that never stops, or an eligibility
  rule that is too strict (excluding legitimate peers) all produce a router
  that looks alive and is not reachable. Each is a named invariant in the
  phase that owns it.
- **Emissary's designs are a reference, not a source.** Its sequencing is
  evidence about *order*; its code is out of bounds on this branch.

## 11. Completion definition

This roadmap is closed when all of the following hold:

- the router keeps a bounded, tiered peer store with honest failure
  detection, and draws diverse candidate sets from it;
- the router maintains a bounded, date-keyed, floodfill-indexed routing table
  and an exploration job, and a peer can find `i2pr` through the DHT and
  `i2pr` can find a peer;
- the router answers a peer’s SSU2 Relay Request as the introducer, so that a
  firewalled peer is reachable through it, and publishes the validated
  introducer set in its RouterInfo address;
- the router honors the minimum-peer floors its published `router.version`
  attaches, with per-peer version evidence;
- the router relays tunnels for other routers under an operator-opted,
  bounded, revocable participation policy, with a measured share;
- two independent router families interoperate with `i2pr` in its
  participant roles, as `specs/CONFORMANCE.md` item 7 requires;
- the floodfill role serves the NetDb correctly under churn, and `caps=f` is
  advertised only after that evidence exists, and is withdrawn on health loss;
- `specs/support.toml` and `specs/CONFORMANCE.md` state the achieved
  position truthfully, and no advertisement exceeds the tested subset.

A roadmap that reaches a *controlled-mesh* participant — with 488–492 passed
and 493 blocked on the topology gate — is a legitimate intermediate closure
under the same rules the rest of the repository uses, provided it is called
exactly that.

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 488 is closed
(`passed-peer-profile-store`), Plan 489 is closed
(`passed-floodfill-indexed-routing-table`), Plan 490 is closed
(`passed-ssu2-introducer-relay-role`), Plan 491 is closed
(`passed-api-version-floors-and-peer-eligibility`), Plan 492 is closed
(`passed-transit-participation-posture`), and **Plan 502 is closed**
(`passed-transit-owner-in-the-product-composition` — the transit data-plane
owner is installed in the product composition behind the operator's
default-off `[transit] participation` opt-in, which discharges Plan 492's
F2; the build's *delivery* to a peer and the stock-peer evidence remain
unproven, so nothing is advertised); **Plan 503 is closed**
(`passed-live-dsm-version-gate-and-peer-eligibility-receipt` — it
discharges **Plan 491 F1** and closes the live half of Plan 491's *second*
API-version obligation: the two live `DatabaseStore` sends used to publish to
floodfills that publish no version, an uncomparable version, or a version
below the floor, because the coordinator that enforces the DSM floor is never
composed; Plan 503 puts the floor on both live paths and makes the
peer-eligibility receipt a live product); **Plan 504 is closed** (`passed-live-tunnel-liveness-in-the-product-composition` — Plan 185's tunnel-liveness scheduler is now **constructed by the production daemon**, driven on the existing tick and wired to the existing inbound seam, so a failed exploratory tunnel pair is detected and rebuilt instead of lingering until the lease expires; the correlation is **authenticated** on the acknowledged-message id, and the **proven-pair gate** means a pair that has never answered is never torn down). Plan 185's closure record claimed the daemon pump already called the liveness seam; that was false of the tree it described and Plan 185's record is **not** rewritten — the correction lives in Plan 504's closure record and in the registry's row. The daemon still installs **no exploratory tunnels**, so the scheduler tracks zero pairs today: installed and inert, not broken. Plan 487 F3 is **advanced, not closed** — the destination-inbound half needs tunnel rekeying, which the tree does not implement at all. **Plan 505 is closed** (`passed-exploratory-build-driver-in-the-product-composition` — the router now names a peer through Plan 491's version-gated selection, builds and installs a one-hop exploratory tunnel toward it through the **existing** build path, and records the outcome in the peer-profile store, which is **fed** for the first time in a live router and so gives Plan 504's liveness scheduler a population to watch). The outcome map is total over all fourteen coordinator variants and separates a **peer fault** from a **local condition**: the router's own faults are counted and produce **no** peer-profile write. The posture is **off by default** — the SSU2 dial target is loopback-only and the private-LAN variant accepts only RFC1918 IPv4, so a public deployment's peer population is not dialable at all. Plan 497 limitation 5 is **partially** discharged: the store is fed, written and read, but the routing table's only reader remains Plan 433's blocked coordinator. Plan 487 F3 stays open — its destination-inbound half needs tunnel rekeying, which the tree does not implement at all. **Plan 508 is closed** (`passed-transit-creator-correlation-retention` — Plan 502's Finding M1: the transit owner's creator-correlation facts were insert-only sets that grew with every build the router performed, for ever. The protocol decision Plan 502 deferred is **made on the record**: the window is the coordinator's own bound, `BUILD_DEADLINE_MS + BUILD_TUNNEL_ID_RETENTION_MS`, so the router's own traffic is never claimed as transit work while a reply could still arrive, and the facts are bounded over the router's lifetime with **no new configuration key** and **no second timer**). **Plan 509 is closed** (`passed-lookup-target-api-version-decision` — it discharges **Plan 503's recorded lookup-target finding**. The decision is that the lookup target is **not** version-gated: no floor is derivable, because the specification's API-version table attaches exactly two duties to the `0.9.62` row and a `DatabaseLookup` is neither; the two existing floors encode duties *on the peer* that do not transfer to a lookup; the router **validates the answer itself**, so the responder's version buys no safety; the rotation absorbs an unhelpful candidate at the cost of one round trip; and the two reference implementations **disagree** about a version rule for floodfills and neither gates a lookup. The decision is proved over the tree's own surfaces and enforced by a static guard that checks its own premise, and it changes **no production behaviour**) **Plan 507 is closed** (`passed-netdb-retention-policy` — the router's peer records no longer outlive the router's **own** acceptance window, and the NetDB turns over instead of freezing: `RouterInfoStore::prune`, the tree's own *explicit prune over silent eviction* operation, had **zero production callers** -- only its own unit test -- while `insert` refuses at the quota and never evicts, so a full store refused every new peer **permanently**. The prune **rides** Plan 506's reconcile on the existing cadence, uses the tree's own acceptance window through a `const fn`, adds **no configuration key**, and changes `i2pr-netdb` **not at all**; every prune is counted by reason and a prune that cannot run is counted too). **Plan 493 (the two-family public
participation qualification) remains registered blocked** on the authorized
two-host topology, and Plan 494 is blocked on 493. The prior axis — Plans 477–480 and
485–487 — is closed and is not reopened here.

**The qualification gate is itself a plan-of-record.** Plan 495
(`plans/closure/core-router-recovery/495-status.md`, `passed-peer-eligibility-gate-and-first-failure-owner`;
core-router recovery owner) repairs the external lanes before they run: the
stock peer's admission and selection become **preconditions checked offline
before any process launch**, every outcome names its **failure owner**, and a
family that declines to select us is `peer-selection-refused` — a result about
that family's selection state, not an `i2pr` defect and not a pass. Plan 493
binds to it; the model is
`docs/operations/ssu2-private-lan-qualification.md` and it is bound from
`specs/CONFORMANCE.md`. Plan 513 is **registered** (`registered-live-netdb-discovery-spine`): it discharges Plan 497's **M2** and **limitation 1** -- the shared routing-table consumer has no production reader, and the table is populated in a live daemon while no live lookup consults it. The router runs **only** a `LeaseSet2` lookup today; the exploratory and RouterInfo kinds have no production caller. The plan lands the RouterInfo-kind resolution first, because the exploration's answers are peer *hashes* and a peer the router cannot resolve is only a name, and then the exploratory lookup, with Plan 102's exploratory-tunnel authority honoured.
