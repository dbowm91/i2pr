# ADR 0041: Network participation sequencing and role gating

Status: **accepted**
Date: 2026-10-20
Plan: 488 (and the network-participation roadmap 488–494)

## Context

`i2pr` is a working client-side router and a not-yet-participating
network router. The repository's own support inventory says this precisely:
of 89 surface entries, **none are advertised**; 83 are `experimental`, 5 are
`deferred`, and 1 is `ready`
(`specs/support.toml`). The router can build its own tunnels, publish a
signed RouterInfo, and serve SAM/I2CP/Streaming locally, but it cannot be
found by a peer, cannot be entered as a database server, and does not carry
other routers' traffic.

The core-router-recovery line (Plans 477–480, 485–487) closed on 2026-10-20
with the client-product and the §6 recovery gates proven. That line
answered "can the product use I2P"; it did not answer "is the router part of
I2P", and it was never scoped to.

Three questions had to be answered before the next work could be planned,
and answering them wrongly would have been expensive:

1. **What is the correct order of the participation capabilities?** The
   natural-looking order — build a DHT table, then a role, then advertise —
   is not obviously right, and getting it wrong means building a routing
   table over peers the router should not build through.
2. **What may be advertised, and when?** `caps=f` is a promise about other
   routers' data. Advertising it early damages the network; never
   advertising it leaves the work unfinished.
3. **Which of these capabilities are *routers'* duties and which are
   *client libraries'* duties?** The repository's crate boundaries already
   encode that split, but the participation work would cross them.

The reference material was consulted: the I2P specification (the NetDb and
transport pages, the I2NP API-version table), the pinned reference routers
i2pd 2.61.0 and Java I2P 2.13.0, and Emissary — the closest production Rust
I2P router, used as a sequencing and design reference only, since ADR 0028
§7 excludes its source from reuse on this branch.

## Decision

### 1. The participation capabilities are ordered substrate-first

The next work is the **network-participation** axis, owned by the new
`plans/subsystems/network-participation-roadmap.md`, and it is ordered:

```text
488 peer profile store        (tiers, failure detection, score, selection policy)
489 floodfill-indexed routing table + exploration + optimized lookup
490 SSU2 introducer (relay) role
491 API-version floors and peer eligibility
492 transit participation posture
493 two-family public participation qualification   [blocked: topology]
494 floodfill role and the caps=f advertisement      [blocked: 493]
```

The order is not a preference; it follows three facts:

- **The Kademlia keyspace is indexed by floodfills.** A table of "all
  peers" is the wrong structure. A router must therefore know which peers
  are floodfills and which are worth building through *before* it indexes
  anything — hence the profile store first.
- **The search key is date-keyed** (`SHA256(key || UTC_yyyyMMdd)`), so the
  keyspace rotates daily and the table's lifetime is bounded by that
  rotation. The repository already holds the derivation
  (`format_daily_key`); the work is to use it, not to invent a second one.
- **The lookup is optimized and iterative.** The repository's closed
  `RouterInfoLookup` engine is already iterative, with a deadline, a peer
  budget, duplicate coalescing and suggestion merging. The work extends it
  with a table-sourced candidate set and a queryable/pending split. A
  recursive lookup is not introduced, because it would later have to be
  removed.

### 2. Every role is gated on evidence, and the advertisement is last

- A role may be **implemented** before it is **advertised**. Implementation
  is invisible to the network; advertisement is a claim about other
  routers' data.
- `caps=f` is published only after the two-family evidence exists
  (Plan 493 → 494), and the builder's refusal path — a controlled
  activation permit plus a tier at or above the pinned floor — stays in
  place and stays tested.
- **Health loss withdraws the capability**, promptly and observably, and
  stops admitting new work.
- The `router.version` declaration stays pinned to the implemented I2NP
  surface. A peer that refuses the declaration is a **finding**, not a
  licence to edit it.

### 3. The version obligations are compliance, not an option

The specification's API-version table attaches two duties to the level
`i2pr` declares (0.9.62): minimum peers to build tunnels through, and
minimum floodfill peers to send DSMs to. `i2pr` had no peer-version floor
logic at all. Both enforcement points are added (Plan 491) and both fail
**closed**: a peer whose published version is unknown, unparseable, or
below the floor is not used for the gated purpose. An under-complying
router is a defect, not a safe default.

### 4. Participation stays opt-in, and stays off by default

Transit participation and the floodfill role are **operator opt-in**,
default-off, bounded, and revocable, and each has a distinct withdrawal
path. This is a deliberate divergence from the reference routers, which
participate by default; the divergence is recorded, not "corrected".

### 5. The crate boundary is the discipline, not an afterthought

- `i2pr-netdb` owns the routing table, the profile store and the selection
  policy — **runtime-neutral**, no sockets, no timers.
- `i2pr-daemon` owns the role owners, the configuration and the receipts.
- `i2pr-runtime` owns Tokio, sockets and timers.
- `i2pr-proto` owns the codecs, including the new version comparator.
- `i2pr-client` never depends on `i2pr-api`, and no participation work may
  create such an edge.

### 6. The topology gate is honored, not engineered around

The two-family qualification (Plan 493) needs an authorized, independently
addressed non-loopback topology. Plan 431 stopped at exactly that
boundary. The new plan is therefore registered **blocked**, and the
roadmap's completion definition allows an intermediate, honestly-named
closure: a router that participates in a controlled mesh is not "a router
on the public network", and the repository may not call it that.

## Consequences

- The repository gains a coherent, dependency-ordered next axis instead of
  a set of unrelated gaps, and every phase is small enough to be one
  focused commit (Plans 488–494).
- The first four phases need **no external topology** and no public bind,
  so progress is possible under the repository's existing acceptance rules.
- Nothing is advertised by this decision. `specs/support.toml` keeps its
  zero-advertised state until a phase's own evidence earns an entry.
- A router that completes the axis becomes a *more exposed* network
  position. ADR 0030's reachability constraints continue to bind, and no
  phase may weaken them for the sake of a capability claim.
- The reference Rust implementation remains out of bounds as a source
  (ADR 0028 §7). This ADR uses it as evidence about *order* and *shape*,
  which is what a reference is for.

## Alternatives considered

- **Build the Kademlia table first, over all peers.** Rejected: it is the
  wrong keyspace, and it would have to be rebuilt once the floodfill
  distinction existed.
- **Advertise `caps=f` as soon as the controlled floodfill service
  passes.** Rejected: the controlled service proves a *controlled*
  subset, not the role; a floodfill that serves stale or unvalidated data
  is worse than no floodfill.
- **Reuse Emissary's implementation.** Rejected by ADR 0028 §7. The
  reference is used for sequencing and shape only.
- **Use a recursive lookup for simplicity.** Rejected: the repository
  already has a closed iterative engine with the right bounds, and a
  recursive lookup is a later removal, not a later improvement.
- **Close the roadmap only on public-network participation.** Rejected: the
  repository's closure rules allow an honest intermediate closure, and
  refusing one would stall the axis on a topology nobody has authorized.
  The completion definition names that intermediate state exactly.
