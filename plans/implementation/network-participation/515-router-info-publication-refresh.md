# Plan 515 — the router republishes its own RouterInfo

**Implementation plan.** The plan of record for the network-participation
subsystem. Registered 2026-11-02, immediately after Plan 514's closure. The
plan number is load-bearing and is never renumbered.

## 1. Objective

Make the router's **own** RouterInfo a **refreshed** network record rather
than a one-shot announcement.

Today `i2pr` publishes its own RouterInfo to the network **exactly once**,
from the service-tunnel provisioning path, and never again. Two consequences
follow, and both are core-router defects:

1. **The record goes stale.** A `RouterInfo` that is never republished ages
   out of the network's view. A peer that wants to reach `i2pr` cannot find
   it, however long the router has been running. The reference republishes
   on a timer for exactly this reason.
2. **A router without service tunnels never publishes at all.** The
   publication is not a router behaviour; it is a side effect of building
   service tunnels. A router whose only job is to be a DHT member is
   invisible.

The reference's model, verified at the pinned commit
(`635b013a612ff47278ef02acf8580a28e10e26c5`, i2pd 2.61.0):

- a **publication timer** fires every `ROUTER_INFO_PUBLISH_INTERVAL`
  seconds — `39 * 60` (`libi2pd/RouterContext.h:34`) — plus a random
  `ROUTER_INFO_PUBLISH_INTERVAL_VARIANCE` of up to `105` seconds
  (`libi2pd/RouterContext.h:36`, applied at `libi2pd/RouterContext.cpp:1461`),
- the timer's handler (`libi2pd/RouterContext.cpp:1470`) clears the
  per-cycle exclusion set, updates the RouterInfo's timestamp, and
  **publishes** (`libi2pd/RouterContext.cpp:1492`),
- each publish asks the **closest** floodfill that has not already been
  asked this cycle (`libi2pd/RouterContext.cpp:1502`,
  `libi2pd/NetDb.cpp:1296`), and adds it to the exclusion set,
- a **resend** timer then republishes every
  `ROUTER_INFO_CONFIRMATION_TIMEOUT` milliseconds — `1600`
  (`libi2pd/RouterContext.h:37`, `libi2pd/RouterContext.cpp:1547`) — so the
  next closest floodfills are asked, up to
  `ROUTER_INFO_MAX_PUBLISH_EXCLUDED_FLOODFILLS` — `15`
  (`libi2pd/RouterContext.h:38`).

`i2pr` today has **none** of this. Its one publication is a
best-effort precondition of the first service-tunnel build
(`crates/i2pr-daemon/src/service_product.rs:8122`, inside
`provision_all_service_router_material`), and it fans out to the three
closest version-gated floodfills
(`crates/i2pr-daemon/src/service_product.rs:7865`,
`:7953-7962`) — the right *targets*, once, and never again.

**Plan 515 closes the refresh.** Nothing is promoted: `router.version` stays
`0.9.62`, `caps` and the published RouterInfo stay unchanged, the
inventory's advertised count stays **zero**, and `Cargo.toml` and
`Cargo.lock` stay byte-identical.

## 2. Why ready

- Plan 514 closed the **aim** of the exploratory request and made the
  operator's switch over it real. The routing table's closest-first
  selection is now the live selection, and the table's reader is a
  production reader.
- The **publication machinery already exists**: the `DatabaseStore` is
  built (`creator_router_info_store`), the targets are selected through
  Plan 503's version gate (`version_gated_publication_floodfills`,
  `crates/i2pr-daemon/src/destination_tunnels.rs:1016`), the delivery goes
  through the router's own SSU2 delivery, and the outcome is counted.
- The reference's model is verified at the pin, with line-level citations
  in §1.
- The router already owns a **periodic owner** with a configurable period
  (`register_netdb_participation_service`,
  `crates/i2pr-daemon/src/lib.rs:2351`, using
  `config.netdb.participation_maintenance_period_ms`), so the shape of the
  refresh is the shape the tree already uses.

## 3. Current implementation evidence

**The publication happens once, and only from the service-tunnel path.**

- `crates/i2pr-daemon/src/service_product.rs:7932` —
  `async fn publish_creator_router_info(...)`: the only place the router's
  own RouterInfo is sent to the network. It reads the installed local
  RouterInfo (`:7940`), selects the closest version-gated floodfills
  (`:7953-7962`), builds the store (`:7970`), and delivers it to each
  target (`:7988-8033`), counting `stores_accepted` (`:8031`).
- `crates/i2pr-daemon/src/service_product.rs:8122` — its **only** call
  site, inside `provision_all_service_router_material` (`:8101`). That
  function is the service-tunnel provisioning path.
- The local RouterInfo is read in exactly two places in the whole tree:
  `crates/i2pr-daemon/src/service_product.rs:7940` (this publication) and
  `crates/i2pr-daemon/src/lib.rs:4525` (the SSU2 service installing it for
  handshakes). **There is no other publication of the router's own
  record.**
- `crates/i2pr-daemon/src/service_product.rs:7865` —
  `const CREATOR_RI_PUBLICATION_FLOODFILLS: usize = 3`: the fan-out bound
  of the one-shot publication.
- `crates/i2pr-daemon/src/destination_tunnels.rs:1016` —
  `version_gated_publication_floodfills`: the selection. It is
  closest-first, version-gated (Plan 503's DSM floor), excludes our own
  hash, and records a DSM receipt per selection.

**There is no timer.** No timer, no schedule, no refresh, and no
resend exists anywhere in the daemon for the router's own RouterInfo.

**The reference, at the pin.**

- `libi2pd/RouterContext.cpp:1456` — `SchedulePublish()`; the expiry is
  `ROUTER_INFO_PUBLISH_INTERVAL + rng % ROUTER_INFO_PUBLISH_INTERVAL_VARIANCE`
  seconds (`:1461-1462`).
- `libi2pd/RouterContext.cpp:1470` — `HandlePublishTimer()`; clears
  `m_PublishExcluded` (`:1477`), updates the timestamp, publishes, then
  schedules the resend.
- `libi2pd/RouterContext.cpp:1492` — `Publish()`; picks the closest
  floodfill not yet asked (`:1502`), sends a `DatabaseStore` with a fresh
  reply token — directly when the peer is connected or reachable, otherwise
  **through an exploratory tunnel** — and adds the peer to the exclusion
  set.
- `libi2pd/RouterContext.cpp:1547` — `SchedulePublishResend()`;
  `:1560` — `HandlePublishResendTimer()`, which republishes every
  `ROUTER_INFO_CONFIRMATION_TIMEOUT` milliseconds.
- `libi2pd/RouterContext.h:34`, `:36`, `:37`, `:38` — the four constants.

## 4. Invariants that must not regress

1. **The version gate.** The publication is a `DatabaseStore`, so Plan
   503's DSM floor applies to it: a floodfill that publishes no version,
   an uncomparable version, or a version below the floor is never a
   target, and every refusal is counted by reason. The refresh must use the
   same gated selection, not a raw store scan.
2. **Bounded fan-out.** The number of targets per publication stays
   bounded by an explicit constant. A refresh must not turn into an
   unbounded fan-out.
3. **No advertisement.** `router.version` stays `0.9.62`, `caps` stay
   unchanged, the published RouterInfo's content stays unchanged, and the
   inventory's advertised count stays zero.
4. **No direct clearnet.** The publication travels the router's own I2P
   transport, never a direct socket.
5. **Fail closed, counted.** Every outcome — published, no local record,
   no eligible target, refused by version, delivery refused, cancelled —
   is counted by reason. A refusal is never a silent success.
6. **Owned and cancellable.** The refresh has an owner, a period, and a
   cancellation path. It is not an unbounded loop and not a detached task.
7. **The one-shot publication still happens.** The creator's record must be
   resolvable *before* the first build, so the existing precondition stays.

## 5. Scope

**Lands:**

- a **runtime-neutral publication schedule**: the period, the due test, the
  per-cycle state, and the counters, with no I/O and no clock read of its
  own (the caller passes `now_ms`).
- a **periodic owner** that drives the publication on that schedule and
  owns its cancellation.
- the **wiring** that makes the router's own RouterInfo refresh without
  depending on a service tunnel being provisioned.
- the **records**: the inventory surface, the conformance note, the
  qualification gate, and the deep-dive.

**Does not land:**

- the reference's **resend burst** (the 1.6 s republish to the next
  closest floodfills). D7 decides against it, and the divergence is
  recorded as a finding.
- any change to the **one-shot** publication's targets, its fan-out bound,
  or its best-effort character.
- any change to the RouterInfo's **content**.
- any **live** stock-peer evidence; that remains Plan 493's topology.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/router_info_publication.rs` | **New.** The runtime-neutral schedule, the outcome type, and the counters. |
| `crates/i2pr-daemon/src/lib.rs` | The periodic owner, its registration, and the call into the existing publication. |
| `crates/i2pr-daemon/src/service_product.rs` | The one-shot publication delegates to the shared owner instead of doing its own selection, so there is **one** publication path. |
| `crates/i2pr-daemon/src/config.rs` | The period's configuration key, if the period is configurable. |

**No change** to `crates/i2pr-netdb` is planned: the gated selection
already exists and is correct.

## 7. Design decisions the plan must make explicit

- **D1 — The period.** The reference's interval is `39 * 60` seconds with
  up to `105` seconds of variance. `i2pr` should default to the reference's
  interval and **not** add the variance: a deterministic period is
  testable, and the variance exists to de-synchronise a flood with the
  network, which `i2pr` has no need for. The decision is recorded either
  way, and the reason is recorded.
- **D2 — The variance.** If the variance is added, it must come from an
  injected source and must be bounded; a wall-clock read inside the
  schedule is forbidden.
- **D3 — The clock.** The schedule takes `now_ms` from its caller. It
  never reads a clock itself. This is the tree's rule for state machines
  and it is what makes the lane deterministic.
- **D4 — The coupling. Answered: the refresh is a router-level owner.**
  The publication's **transport** is router-wide. The service product
  attaches to the router's *existing* SSU2 daemon
  (`crates/i2pr-daemon/src/lib.rs:4235`, `start_over_existing_daemon`),
  and the router's own owner reads the installed local RouterInfo from
  that daemon (`crates/i2pr-daemon/src/lib.rs:4525`). A router-level
  owner can therefore reach both the transport and the local record
  without a service tunnel existing.
  The **selection** needs the router's own store of floodfills. The
  service product's `DestinationTunnelCoordinator` holds one
  (`crates/i2pr-daemon/src/service_product.rs:2766`), but that coordinator
  is service-product-scoped, so it is the wrong owner. The router-wide
  **bootstrap's** store is the right one: it is the store the live
  discovery spine already selects from
  (`crates/i2pr-daemon/src/lib.rs:2064`, `bootstrap.store()`), and the
  version gate is a **free function over a store**
  (`i2pr_netdb::select_version_gated_floodfills`, used at
  `crates/i2pr-daemon/src/destination_tunnels.rs:1004` and `:1021`), so
  Plan 503's DSM floor applies to it unchanged.
  **Therefore:** the refresh is a router-level periodic owner that
  selects through the same gated function over the router-wide store, and
  the coupling to service tunnels is **removed** — a router with no
  service tunnels publishes.
- **D5 — The counters.** The outcome type names every outcome, and the
  counters are per reason. A publication that refused is not a
  publication.
- **D6 — The first publication.** Whether the refresh publishes
  immediately on start or waits one period is a decision. The reference
  publishes on its first cycle after the router is up; `i2pr`'s existing
  one-shot already publishes at provisioning, so the refresh's first
  publication may wait a full period without a loss.
- **D7 — The resend. Answered: keep the bounded one-pass fan-out.** The
  reference publishes to **one** floodfill and then, every `1600`
  milliseconds, to the next closest ones — up to fifteen
  (`libi2pd/RouterContext.cpp:1560`, `libi2pd/RouterContext.h:37`, `:38`).
  `i2pr` asks the **three** closest in one pass
  (`crates/i2pr-daemon/src/service_product.rs:7865`). One pass is not
  weaker: it asks three peers immediately where the reference asks one
  and then waits. This plan keeps the one-pass shape, keeps the bound at
  three, and does **not** adopt the reference's resend burst. The
  divergence — three targets per period where the reference may reach
  fifteen — is recorded as a finding, not a defect.

## 8. Work packages

**WP1 — The runtime-neutral schedule.** A new module owning the period, the
due test, the per-cycle state and the counters. No I/O, no clock, no
transport. Its lane proves the cadence and the accounting with fixed seeds
and an explicit `now_ms`.

**WP2 — The placement. Decided by D4: a router-level owner.** The
transport and the local RouterInfo are router-wide; the selection comes
from the router-wide bootstrap's store through the same gated function
the coordinator uses. The owner is registered in the service graph beside
the other periodic owners, and it does not depend on a service tunnel
being provisioned.

**WP3 — The periodic owner.** An owned, cancellable, periodic owner that
calls the publication when the schedule says it is due, and records the
outcome. It is registered in the service graph like every other periodic
owner, and it stops on cancellation.

**WP4 — One publication path.** The one-shot publication and the refresh
share the same selection, the same store construction, and the same
delivery. The refresh adds a **trigger**, not a second implementation.

**WP5 — The guards and the records.** A structural guard for the new
surface, the inventory entry, the conformance note, the qualification gate
and the deep-dive.

**WP6 — The controls.** Negative controls that mutate the new surface and
prove the guard and the lane catch each mutation by an **assertion**, not
by a build failure. Every mutation reverted byte-for-byte, with the
SHA-256 asserted. **The restore must write the file's bytes, not copy it
with its metadata** — Plan 514's floor failure was caused by exactly that
(see `plans/closure/network-participation/514-status.md`, finding **M1**).

## 9. Failure, cancellation, restart, and contention semantics

- A **refused** publication is counted and the next period is unaffected.
- A **cancelled** publication stops at the next cancellation check and is
  counted as cancelled, not as published.
- The owner **stops** on cancellation; it does not hold the process open.
- Two publications may not overlap: the owner is single-flight, and a
  publication still in flight when the next period falls due is **skipped
  and counted**, never run concurrently.
- On **restart**, the schedule starts from the new process's own clock; no
  publication is resurrected from a persisted timestamp.

## 10. Compatibility and migration

- No configuration key is required: the default period must work with an
  existing configuration file. A new key, if there is one, defaults to the
  reference's interval.
- `Cargo.toml` and `Cargo.lock` stay byte-identical. No new dependency.
- The one-shot publication's behaviour is unchanged: the same targets, the
  same bound, the same best-effort character, and provisioning still
  continues when it fails.

## 11. Required tests

**The lane** (public API only, fixed seeds, an explicit `now_ms`, no
socket, no environment variable, no `#[ignore]`, no sleeping):

1. a publication is **due** exactly one period after the last one;
2. a publication is **not** due** before the period elapses;
3. the first publication is due at the schedule's own start;
4. every outcome is counted by its own reason;
5. a refused publication does not count as a published one;
6. a publication still in flight at the next period is skipped and counted.

**The guard** (structural, in the house style — a named function's own body
must contain the required call, comments stripped, `#[cfg(test)]` blocks
excluded, an absent function failing closed):

1. the schedule is driven by the periodic owner;
2. the owner owns its cancellation;
3. the one-shot publication and the refresh share the selection;
4. the refresh never bypasses the version gate;
5. the fan-out stays bounded;
6. the configuration key, if any, is consumed.

**The crate-internal tests**: the owner's single-flight rule and its
cancellation.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test router_info_publication -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

The full routine floor from `AGENTS.md` is run before handoff, and its
result is recorded in the closure record. **The floor must run on a clean
tree**: the floor script asserts `git status --porcelain` is empty before
its first step, because Plan 514's first floor run measured a stale
artifact instead of the tree.

## 13. Documentation updates

- `specs/support.toml` — a new surface for the refresh, **experimental**,
  advertised **false**.
- `specs/CONFORMANCE.md` — what the refresh does and does not claim.
- `specs/QUALIFICATION-GATES.md` — the gate the refresh must pass.
- `docs/architecture/i2pr-daemon.md` — the new module, the owner, and the
  re-pointed citations for every file this plan changes.

## 14. Acceptance criteria and stop conditions

**Acceptance:**

1. The schedule's lane and the guard are green, and the whole floor is
   green on a clean tree.
2. The router's own RouterInfo is republished on a period, and the
   publication's outcomes are counted by reason.
3. The refresh is reachable **without** a service tunnel being provisioned,
   or the limitation is recorded with the citation that shows why not.
4. Nothing is advertised: `router.version` stays `0.9.62`, `caps` are
   unchanged, the advertised count stays zero.
5. Plan 503's version gate still applies to every publication.
6. The closure record carries the requirement-to-evidence matrix, the
   commands run, the controls, the findings by severity, and the unblock
   audit.

**Stop conditions:**

- The refresh cannot be wired without changing the published RouterInfo's
  content, or without bypassing the version gate.
- The refresh would need a new dependency.
- The placement question (D4) cannot be answered from the tree with a
  citation, in which case the plan stops at the survey rather than
  guessing.

## 15. Closure evidence required

- The floor's table, on a clean tree, with the step count and the outcome.
- The lane's and the guard's results.
- The negative controls' results: every mutation caught by an assertion,
  every file reverted byte-for-byte with its SHA-256 asserted.
- The inventory's figures before and after.
- The unblock audit: what is still open, and who owns it.

## 16. Handoff notes

- Plan 514's closure record is **not** rewritten, and neither is Plan
  513's. A correction to either lives in **this** plan's closure record and
  in the registry row.
- Plan 514's finding **F1** — the exploratory request's rate — is a
  different defect and is not touched here.
- The next free plan number is **516**.
