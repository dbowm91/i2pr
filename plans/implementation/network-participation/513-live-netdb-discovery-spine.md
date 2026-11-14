# Plan 513 — the live NetDB discovery spine

| Field | Value |
| --- | --- |
| Plan | 513 |
| Subsystem | network-participation |
| Status | registered |
| Discharges | Plan 497 **M2** and **limitation 1** (the shared routing-table consumer has no production reader; the table is populated and persisted in a live daemon and no live lookup consults it), and the `netdb.live-routerinfo-lookup` deferred surface |
| Predecessors | Plan 489 (the exploration driver), Plan 497 (the routing table and its consumer), Plan 505 (the exploratory build driver), Plan 102's authority amendment (the exploratory-tunnel dependency), Plan 506 (the peer-knowledge reconcile) |
| Reference pins | i2pd `2.61.0` = `635b013a612ff47278ef02acf8580a28e10e26c5`; Java I2P `2.13.0` = `9134f808337b401e8e53c73734c81fab04280c9d` |
| Advertises | nothing. `router.version` stays `0.9.62`, `caps` and the published RouterInfo are unchanged, `Cargo.toml` and `Cargo.lock` stay byte-identical |

## 1. Objective

Give the router a **live** DHT discovery spine.

Today the router runs **only one** kind of NetDB lookup — `LeaseSet2` — and
only against peers it already knows. The **exploratory** lookup (wire type
`3`) and the **RouterInfo** lookup (wire type `2`) exist in the source and
have **no production caller**. The routing table is populated and persisted
in a live daemon, and **no live lookup consults it**.

That is the core of what "core router functionality" means for a DHT
member: a router that cannot *find* a peer can only talk to the peers it
was handed.

This plan makes both missing kinds live, in the order their dependency
demands.

## 2. Why ready

1. **The gap is recorded and unfixed.**
   `plans/closure/network-participation/497-status.md:394-398` (**M2**):
   *"The shared routing-table consumer has no production reader. The daemon
   populates and persists the table, and the coordinator asks it, but the
   coordinator is not in the daemon's service graph. Plan 433 owns that
   composition and is blocked."* And `:342-348` (**limitation 1**): *"the
   table is populated and persisted in a live daemon, and no live lookup
   consults it yet."*
2. **The blocker is plan ownership, not an environment.** Plan 433 owns the
   **coordinator's** composition, and it is blocked on Plan 431/432. The
   **live lookup path** is not the coordinator: it is Plan 490/491/503's
   `version_gated_candidate_hashes` path, which is live today. The repo's
   precedent — Plan 496, the *"source-side activation substrate Plan 435 is
   waiting on"*, and Plan 502's advance of Plan 436's source-side work —
   permits a source-side successor that does not duplicate blocked work.
3. **The inventory already names the missing capability.**
   `specs/support.toml:1372-1379` carries
   `netdb.live-routerinfo-lookup`, scope `required-later`, status
   **`deferred`**, structure *"live-standards-conformant-routerinfo-lookup-over-exploratory-tunnels"*.
   It is the only **deferred capability** surface left, and it names the
   exact work.
4. **The substrate is landed.** The lookup engine's `RouterInfo` and
   `Exploratory` kinds, the routing table's driver and its exploration job,
   the exploratory build driver, the reply-path adapter, the I2NP message
   codecs, and the transport seam all exist.

## 3. Current implementation evidence

### 3.1 What the live router runs today

- `crates/i2pr-daemon/src/netdb_seam.rs:362` — the live lookup is
  `LookupKind::LeaseSet2`. It is the only kind with a production caller.
- `crates/i2pr-daemon/src/netdb_seam.rs:637` — the **RouterInfo**-kind path
  exists and is complete (`LookupKind::RouterInfo`,
  `start_with_preferred`), but it is reachable only through
  `NetDbTunnelCoordinator`, which is constructed **only** under `#[cfg(test)]`
  (`crates/i2pr-daemon/src/netdb_tunnels.rs:951-958`).
- `crates/i2pr-netdb/src/lookup_id.rs:18-31` — the three kinds and their
  wire codes: `RouterInfo` = `2`, `LeaseSet2` = `1`, `Exploratory` = `3`.

### 3.2 The exploration has no production caller

- `crates/i2pr-daemon/src/netdb_participation.rs:1496` —
  `RoutingTableDriver::begin_exploration`. Its only callers are in
  `crates/i2pr-daemon/tests/` and in the module's own `#[cfg(test)]` block.
- The driver that owns the `ExplorationJob` is **private inside**
  `RoutingTableConsumerState` (`crates/i2pr-daemon/src/netdb_participation.rs:661-666`),
  and the consumer's public surface
  (`:718-830`: `new`, `is_enabled`, `index`, `index_suggested`,
  `advance_time`, `len`, `is_empty`, `candidates`, `counters`) exposes **no
  exploration entry point**. The exploration is therefore *structurally*
  unreachable from production code, not merely uncalled.
- `crates/i2pr-daemon/src/lib.rs:2008-2010` — the daemon says so itself:
  *"The daemon has no suggestion source yet — Plan 489's exploration is not
  composed into the service graph — so the list is empty. The rule is
  enforced here so that a suggestion is never mistaken for evidence when
  that source arrives."*

### 3.3 Why the RouterInfo-kind resolution comes first

- `crates/i2pr-proto/src/i2np/netdb.rs:223-230` —
  `DatabaseSearchReplyMessage` carries `key`, **`peer_hashes`**, and `from`.
  It carries **peer hashes only**, not inline `RouterInfo`s.
- `crates/i2pr-daemon/src/netdb_participation.rs:1689-1691` — so a peer the
  exploration names is not yet usable: `begin_lookup` returns
  `RoutingLookupOutcome::NeedsPeerResolution` — *"a peer without an address
  cannot be asked"*.
- The router's peer universe is therefore exactly *the seed file's floodfills
  plus unsolicited `DatabaseStore`s*
  (`plans/closure/network-participation/506-status.md:92-97`).

**The dependency is real:** an exploratory lookup whose answers cannot be
resolved adds names the router cannot talk to. The **RouterInfo-kind
resolution is the enabling half**, and it is also the half the inventory
names as deferred.

### 3.4 The exploratory-tunnel requirement is an authority, not a preference

`plans/implementation/netdb/102-amendment-exploratory-tunnel-dependency.md`
is a **closed authority amendment**:

> Current I2P RouterInfo `DatabaseLookup` operation uses an outbound
> exploratory tunnel and requests the response through an inbound exploratory
> tunnel.

> A direct `DatabaseLookup` over NTCP2 is not accepted as a substitute for
> the standard exploratory-tunnel path.

So a standards-conformant **RouterInfo** lookup (wire type `2`) must ride
an **outbound** exploratory tunnel and take its reply through an **inbound**
exploratory tunnel. The reply-path seam exists:
`crates/i2pr-tunnel/src/provider.rs:41` —
`ExploratoryPoolReplyPathProvider::provide_reply_path`, which asks the
exploratory pool for an established, unexpired **inbound** tunnel.

An **exploratory** lookup (wire type `3`) is different in the
specification: it is addressed **directly** to the floodfill being asked,
and the floodfill answers **directly** with a `DatabaseSearchReply`. The
direct transport seam exists:
`crates/i2pr-runtime/src/ssu2_runtime.rs:3722` — `send_i2np`.

### 3.5 What the routing table would gain

- `crates/i2pr-daemon/src/netdb_participation.rs:787` — the consumer's
  `candidates(store, routing_key, excluded)` returns the table's
  closest-first answer, or a **counted** fallback
  (`Disabled`, `NoCandidate`, `Unavailable`).
- The consumer is called in `src/` **only** at
  `crates/i2pr-daemon/src/netdb_tunnels.rs:454`, inside the
  never-constructed coordinator.
- The **no-narrowing rule** is already established and must be inherited:
  the table may only **reorder** the flat store's selection, never remove a
  candidate the flat store could serve and never add one it would not have
  served (`plans/closure/network-participation/497-status.md:388-393`).

## 4. Invariants that must not regress

1. **The live LeaseSet2 lookup's behaviour is unchanged.** Its version gate
   (Plan 491/503) and its candidate selection stay exactly as they are.
2. **The no-narrowing rule is inherited.** The table may only reorder.
3. **The version gate is inherited.** Every candidate, table-sourced or not,
   is still version-gated (Plan 491/503). A table-sourced candidate that
   fails the gate is refused, and the refusal is counted by reason.
4. **Plan 102's authority amendment is honoured.** The RouterInfo-kind
   lookup rides an **outbound** exploratory tunnel and takes its reply
   through an **inbound** exploratory tunnel. A direct `DatabaseLookup` over
   NTCP2 is **not** a substitute.
5. **Every fallback is counted.** A table that cannot answer falls back to
   the flat store's selection, and the reason is counted.
6. **No peer is blamed.** Nothing here writes a peer's fault into a profile.
7. **No unbounded state.** The exploration's budget and deadline, the
   lookup's own bounds, and every new table are bounded.
8. **No advertisement.** `router.version` stays `0.9.62`; `caps` and the
   published RouterInfo are unchanged; the inventory's advertised count
   stays zero; `Cargo.toml` and `Cargo.lock` stay byte-identical.

## 5. Scope

**In scope.**

- **WP1 — the live RouterInfo-kind lookup.** A production path that starts a
  `LookupKind::RouterInfo` lookup for a named peer, addresses the request
  through an **outbound** exploratory tunnel, and takes the reply through an
  **inbound** exploratory tunnel, per Plan 102's amendment. The reply's
  `DatabaseStore` is ingested into the router's NetDB.
- **WP2 — the live exploratory lookup.** A production path that asks a known
  floodfill for the peers closest to a random search key (wire type `3`),
  ingests the `DatabaseSearchReply`, and indexes the named peers into the
  routing table **as suggestions**, under Plan 489's rule that a suggestion
  is never evidence.
- **WP3 — the routing table's live read half.** The live lookup's candidate
  list consults the table as a **preference**, never narrowing, with
  counted fallbacks.
- The lane, the guard, the records, the controls, the floor and the closure.

**Out of scope, and why.**

- **A stock-peer qualification.** The mechanism is proven over the tree's
  own surfaces; the external half stays Plan 493's.
- **The `NetDbTunnelCoordinator`'s composition.** Plan 433 owns it and it
  is blocked. This plan does not construct the coordinator and does not
  duplicate blocked work: the live lookup path is a different path, and it
  is reachable without the coordinator.
- **The `caps=f` advertisement.** Plan 494's, gated on Plan 493.
- **Rekeying, tunnel failure detection beyond Plan 512, and the
  per-component width divergence.** Each is recorded for its own successor.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/netdb_participation.rs` | **new** a production entry point that starts an exploration on the consumer — the driver is private inside `RoutingTableConsumerState` today — and **new** the peer-resolution path that answers `RoutingLookupOutcome::NeedsPeerResolution` by starting a `LookupKind::RouterInfo` lookup. |
| `crates/i2pr-daemon/src/netdb_seam.rs` | the live seam gains the RouterInfo-kind and Exploratory-kind starts, beside the existing LeaseSet2 start. |
| `crates/i2pr-daemon/src/lib.rs` | the daemon's participation owner starts an exploration on its existing tick, and the suggestion list stops being empty. |
| `crates/i2pr-daemon/src/service_product.rs` | the lookup's candidate selection consults the routing table as a preference. |

No other production file changes. No dependency changes.

## 7. Design decisions the plan must make explicit

### D1 — The RouterInfo-kind resolution is the enabling half, and it comes first.

The exploration's answers are **peer hashes**, not records
(`crates/i2pr-proto/src/i2np/netdb.rs:223-230`). A peer hash the router
cannot resolve is a name, not a peer. Landing the exploration first would
produce a table full of suggestions the router still cannot ask. The order
is therefore fixed: **WP1, then WP2**.

### D2 — The RouterInfo-kind lookup rides the exploratory tunnels, because the authority says so.

Plan 102's amendment is a **closed authority amendment**, and it says a
direct `DatabaseLookup` over NTCP2 is **not** accepted as a substitute. The
plan therefore composes the lookup with the exploratory pool's **outbound**
role for the request and the pool's **inbound** role for the reply, using
the existing `ExploratoryPoolReplyPathProvider` and the existing outbound
role's cell composition. No second delivery path is introduced.

### D3 — The exploratory lookup is addressed directly, because the specification says so.

An exploratory lookup (wire type `3`) is sent **directly** to the floodfill
being asked, and the answer is a `DatabaseSearchReply` sent **directly**
back. The direct transport seam (`send_i2np`) is the conforming path here.
This is not a contradiction of D2: the exploratory-tunnel requirement is
specific to the **RouterInfo** and **LeaseSet2** kinds.

### D4 — The table may only reorder, and the version gate still applies.

The live candidate list is the flat store's selection **reordered** by the
table's preference. It is never narrowed, and every candidate — table-sourced
or not — is still version-gated. The rule Plan 497's M1 established is
inherited verbatim, and the guard must pin it.

### D5 — Every fallback is counted, and a fallback is never silent.

The consumer already counts `Disabled`, `NoCandidate` and `Unavailable`. The
plan adds no uncounted path: if the table cannot answer, the flat store's
selection is used and the reason is counted.

### D6 — The exploration is bounded and one-at-a-time.

The exploration's peer budget and deadline gate it
(`crates/i2pr-netdb/src/netdb_participation.rs:1539-1547`), and the job is
cancelled when it expires or when the budget is exhausted. One job at a
time; a new job replaces a finished one, never a running one.

### D7 — Nothing is advertised, and the inventory's figure moves honestly.

The `netdb.live-routerinfo-lookup` surface moves from `deferred` to
`experimental` with `advertised = false` **only when** the lane has proved
the mechanism. The `deferred` → `experimental` move is the inventory
telling the truth about what the tree now does; it is not an advertisement.

## 8. Work packages

**WP1 — the live RouterInfo-kind lookup.** A production path that resolves a
named peer to a `RouterInfo`: the request is composed as a
`DatabaseLookup` with `lookup_type = 2`, the request is delivered through
the exploratory pool's **outbound** role, the reply path is the pool's
**inbound** role, and the `DatabaseStore` reply is ingested into the
router's NetDB. The path is bounded by the lookup engine's own bounds and by
the pool's capacities.

**WP2 — the live exploratory lookup.** A production path that starts an
exploration on the consumer's driver, composes a `DatabaseLookup` with
`lookup_type = 3` and a random search key, delivers it **directly** to the
target floodfill, ingests the `DatabaseSearchReply`, and indexes the named
peers into the routing table **as suggestions** under Plan 489's rule.

**WP3 — the routing table's live read half.** The live lookup's candidate
list consults the table as a preference, never narrowing, with the version
gate preserved and every fallback counted.

**WP4 — the lane.** A new lane that drives the mechanism end to end over the
tree's own surfaces: the exploration starts, its dispatch names a peer, the
probe reaches the transport seam, a `DatabaseSearchReply` is ingested, the
named peer is indexed as a suggestion, and the table's preference reorders
— but never narrows — the live selection.

**WP5 — the guard.** A static guard pinning the plan's structure: the
exploration has a production entry point, the RouterInfo-kind start exists
in the live seam, the reply path comes from an **inbound** exploratory
tunnel, the table's read is a **preference**, the version gate survives it,
and every fallback is counted.

**WP6 — the records, the controls, the floor and the closure.**

## 9. Failure, cancellation, restart, and contention semantics

- **An exploration that expires** is cancelled honestly and counted; it is
  never retried in a loop.
- **A lookup that cannot find a reply path** terminates with the engine's
  own reason and is counted.
- **A peer that cannot be resolved** stays unresolved: the suggestion is
  indexed, and the router does not pretend to know the peer.
- **A restart** resets the exploration and the lookup state, as the rest of
  the participation state resets.
- **Contention.** The routing table's driver is behind the consumer's own
  lock; the plan adds no lock that is held across an await.

## 10. Compatibility and migration

No configuration key is added. No persisted state changes. The published
RouterInfo, `caps` and `router.version` are untouched. A router built
before this plan and one built after it interoperate: the new lookups are
ordinary I2NP messages.

## 11. Required tests

| Test | Where | Asserts |
| --- | --- | --- |
| the exploration starts from a production entry point | lane | the gap is closed |
| the exploratory lookup's dispatch names a peer and the transport seam | lane | the request is really sent |
| a `DatabaseSearchReply` is ingested and its peers indexed as suggestions | lane | the answer is used |
| a suggestion is never indexed as evidence | lane | Plan 489's rule |
| the RouterInfo-kind lookup takes its reply path from an **inbound** exploratory tunnel | lane | Plan 102's amendment |
| the table's preference **reorders** and never **narrows** the live selection | lane | Plan 497's M1 rule |
| the version gate survives the table's preference | lane | Plan 491/503's rule |
| every fallback is counted by reason | lane | no silent fallback |
| the guard's rules, each with an inverted probe | guard | the shape is pinned |

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test netdb_discovery_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_discovery_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_routing_consumer -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_routing_mesh -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_peer_budget_receipt -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

The full routine floor from `AGENTS.md` is run before handoff, and its result
is recorded in the closure record.

## 13. Documentation updates

- `specs/QUALIFICATION-GATES.md` — what the pinned references do for a
  RouterInfo lookup and for an exploratory lookup, with the i2pd
  `file:line` citations at the exact pin and Java's caveat.
- `specs/CONFORMANCE.md` — the Plan 513 section.
- `specs/support.toml` — the `netdb.live-routerinfo-lookup` surface's status,
  and one new surface for the exploratory lookup.
- `docs/architecture/i2pr-daemon.md`, `docs/architecture/i2pr-netdb.md`,
  `docs/architecture/i2pr-tunnel.md` — the discovery spine.
- `plans/subsystems/network-participation-roadmap.md` — Plan 513's row and
  §12.
- `plans/registry.md` — Plan 513's row, and Plan 497's row carrying M2's
  discharge. **Plan 497's closure record is not rewritten.**

## 14. Acceptance criteria and stop conditions

**Accepted when** the lane proves the discovery spine end to end, the guard's
rules hold with their inverted probes, the floor is green, the controls are
applied and reverted byte-for-byte, and the inventory's figure moves only
because the tree now does the work.

**Stop conditions** — the plan stops and records, rather than continuing:

- if the RouterInfo-kind lookup cannot be composed on the exploratory
  tunnels' existing roles without a second delivery path;
- if the table's preference cannot be shown not to narrow the selection;
- if the version gate cannot be shown to survive the preference;
- if any control leaves the tree changed, or the floor fails for a reason
  this plan does not own.

## 15. Closure evidence required

The floor's full table; the requirement-to-evidence matrix; the commands run
with their exact results; the invariant, failure, migration and security
reviews; the limitations; the findings by severity; the roadmap disposition;
and the unblock audit — including the **exact** future evidence that would
close the **stock-peer** half.

## 16. Handoff notes

- The subject is **discovery**: a router that can find a peer. The two
  halves are ordered by a real dependency — the exploration's answers are
  peer *hashes*, and a hash the router cannot resolve is a name.
- Plan 102's authority amendment is binding: the RouterInfo-kind lookup
  rides the exploratory tunnels, and a direct `DatabaseLookup` over NTCP2 is
  not a substitute.
- The next free plan number after this one is **514**.
