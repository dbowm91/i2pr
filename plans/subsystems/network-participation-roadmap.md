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
| 493 | blocked | registered-two-family-public-participation-qualification | `plans/implementation/network-participation/493-two-family-public-participation-qualification.md` | `plans/implementation/core-router-recovery/495-peer-eligibility-preflight-and-first-failure-owner-gate.md` (the peer-eligibility gate and the first-failure-owner outcome taxonomy) |
| 494 | blocked | registered-floodfill-role-and-caps-f-advertisement | `plans/implementation/network-participation/494-floodfill-role-and-caps-f-advertisement.md` | — |

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
(`passed-api-version-floors-and-peer-eligibility`), and Plan 492 is closed
(`passed-transit-participation-posture`); **Plan 493 (the two-family public
participation qualification) is the line's next phase and is registered
blocked** on the authorized two-host topology, and Plan 494 is blocked on
493. The prior axis — Plans 477–480 and
485–487 — is closed and is not reopened here.

**The qualification gate is itself a plan-of-record.** Plan 495
(`plans/implementation/core-router-recovery/495-peer-eligibility-preflight-and-first-failure-owner-gate.md`,
core-router recovery owner) repairs the external lanes before they run: the
stock peer's admission and selection become **preconditions checked offline
before any process launch**, every outcome names its **failure owner**, and a
family that declines to select us is `peer-selection-refused` — a result about
that family's selection state, not an `i2pr` defect and not a pass. Plan 493
binds to it; the model is
`docs/operations/ssu2-private-lan-qualification.md` and it is bound from
`specs/CONFORMANCE.md`.
