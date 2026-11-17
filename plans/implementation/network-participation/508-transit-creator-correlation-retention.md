# Plan 508 — The transit creator-correlation retention window

Plan-of-record. Subsystem: **network participation** (the core-router
recovery line). Predecessor: Plan 502 (Finding M1). Successor: none yet.

**Objective.** Give the transit owner's creator-correlation facts a
**retention window**, so the three sets stop growing without bound over the
router's lifetime, while never dropping a fact that could still be needed to
keep the router from claiming its **own** traffic as transit work.

**Status.** registered. Not yet implemented.

---

## 1. Objective

Plan 502 gave the transit owner the creator-correlation facts it needs to
keep the router's own coordinator traffic out of the transit path. It
recorded, as Finding M1, that those facts **never expire**:

> **M1 — The creator-correlation sets grow without bound over the router's
> lifetime.** The owner's `creator_builds`, `creator_data` and
> `creator_gateways` are `BTreeSet`s with only `insert`; the production
> wiring calls `install_transit_creator_correlation` on **every** pump tick,
> so each distinct build the router performs leaves one entry behind forever.
> The code says so plainly (*"the number of facts is bounded by the
> coordinator's pending-build and tunnel-retention ceilings times the number
> of builds the router performs"*), which is not a bound over time.

Plan 502 closed with:

> The fix is a retention window, not a cap: a fact should be dropped only
> after the coordinator could no longer accept a reply for it, which means
> deriving the window from the coordinator's attempt timeout. That is a
> protocol decision, not a mechanical one, and it is not made here.

**This plan makes that decision, explicitly and on the record**, and then
implements it. The window is derived from the tree's **own** constants, so
there is one source of truth and no new configuration key.

## 2. Why ready

- **The finding is recorded, owned and unfixed.** Plan 502's closure record
  names M1 as a Medium finding and states that no corrective plan is
  required by that closure. The gap is not speculative.
- **The tree already carries the constants the window needs.**
  `BUILD_DEADLINE_MS` (`crates/i2pr-daemon/src/exploratory_build.rs:137`),
  `MAX_BUILD_TUNNEL_LIFETIME_MS` (`:112`) and
  `BUILD_MESSAGE_REPLAY_RETENTION_MS` (`:110`) already bound how long the
  coordinator can still recognise a build's identifiers. The window is a
  function of constants that are already there.
- **The coordinator's own fact set is already bounded and already
  expires.** Only the transit owner's **copy** is unbounded. The plan does
  not invent a mechanism; it bounds a copy.
- **The pattern is established.** Plan 507 closed one week of work ago with
  exactly this shape: a retention window derived from the tree's own
  acceptance bound, no new configuration key, every outcome counted by
  reason. The implementation style, the guard style and the closure
  conventions are all settled.
- **The error direction is safe.** A stale fact makes the owner **decline**
  to treat traffic as transit work; it can never make the owner claim the
  router's own traffic as transit work. A missing fact can. This plan
  therefore always errs towards keeping the fact.

## 3. Current implementation evidence

### 3.1 The three sets only ever grow

`crates/i2pr-daemon/src/transit_owner.rs:534-536`:

```rust
    creator_builds: BTreeSet<(i2pr_proto::Hash, u32)>,
    creator_data: BTreeSet<u32>,
    creator_gateways: BTreeSet<u32>,
```

The three install methods are insert-only
(`crates/i2pr-daemon/src/transit_owner.rs:621`, `:626`, `:632`):

```rust
    pub fn install_creator_build(&mut self, peer: PeerId, message_id: u32) {
        self.creator_builds.insert((peer.hash(), message_id));
    }
```

No method anywhere removes an entry. `TransitLiveOwner`'s own `Debug`
implementation reports the three lengths
(`crates/i2pr-daemon/src/transit_owner.rs:548-550`), so the growth is
observable — the debug output simply gets longer.

### 3.2 The production wiring installs on every pump tick

`crates/i2pr-daemon/src/lib.rs:2352`:

```rust
fn install_transit_creator_correlation(
    transit: &mut TransitLiveOwner<ChaCha8Rng>,
    owner: &NormalSsu2Owner,
) {
    let (builds, tunnel_ids) = owner.transit_creator_correlation();
    for (peer, message_id) in builds {
        transit.install_creator_build(peer, message_id);
    }
    for tunnel_id in tunnel_ids {
        transit.install_creator_data(tunnel_id);
        transit.install_creator_gateway(tunnel_id);
    }
}
```

The call site is the inbound pump tick
(`crates/i2pr-daemon/src/lib.rs:4724`):

```rust
                                    install_transit_creator_correlation(transit, &owner);
```

The function's own doc comment admits the shape
(`crates/i2pr-daemon/src/lib.rs:2348-2351`):

> The sets only grow: a fact that was true stays true for the router's
> lifetime, and the number of facts is bounded by the coordinator's
> pending-build and tunnel-retention ceilings times the number of builds
> the router performs.

That is a bound **per build**, not over time. It is the sentence Plan 502
quoted.

### 3.3 The coordinator's own facts are bounded and expire

The facts the owner is given come from the build coordinator:

- `reserved_creator_builds()` (`crates/i2pr-daemon/src/exploratory_build.rs:1077`)
  returns the `(peer, message_id)` pairs of the router's **own pending**
  build replies. The set is bounded by the coordinator's pending-build
  ceiling (`MAX_PENDING_BUILDS = 16`,
  `crates/i2pr-daemon/src/exploratory_build.rs:104`).
- `reserved_tunnel_ids()` (`crates/i2pr-daemon/src/exploratory_build.rs:1060`)
  returns the tunnel ids held by pending builds or still inside their
  active-tunnel/replay retention window.
- The coordinator's **own** memory of a retired build's identifiers is a
  `BTreeMap<u32, u64>` keyed by identifier with an **expiry**
  (`crates/i2pr-daemon/src/exploratory_build.rs:916-917`), and the expiry is
  set when the build is retired
  (`crates/i2pr-daemon/src/exploratory_build.rs:1557-1560`):

```rust
        let message_expiry = self
            .now_ms
            .saturating_add(BUILD_MESSAGE_REPLAY_RETENTION_MS);
        let tunnel_expiry = self.now_ms.saturating_add(BUILD_TUNNEL_ID_RETENTION_MS);
```

So the coordinator **knows exactly when each fact may be dropped** and
drops it. The transit owner's copy does not.

### 3.4 The safe window is derivable from the tree's own constants

The fact is installed while the coordinator's build is **pending**. The
coordinator's own memory of that build's identifiers starts when the build
is **retired**. A fact must therefore survive:

1. the longest a build attempt can stay **pending**, which is the build
   deadline `BUILD_DEADLINE_MS = 60_000`
   (`crates/i2pr-daemon/src/exploratory_build.rs:137`); and
2. the longest the coordinator retains a **retired** build's identifiers,
   which is the tunnel-id retention
   `BUILD_TUNNEL_ID_RETENTION_MS = MAX_BUILD_TUNNEL_LIFETIME_MS +
   BUILD_MESSAGE_REPLAY_RETENTION_MS` = 30 min + 15 min = 45 min
   (`crates/i2pr-daemon/src/exploratory_build.rs:112-114`).

The window is therefore **46 minutes**, and it is a function of constants
the tree already has. Nothing is invented and nothing is configured.

### 3.5 The four use sites, and their two clock units

The facts are consulted in four places, and the owner already has a clock at
each:

| Use site | Function | Clock parameter |
| --- | --- | --- |
| `crates/i2pr-daemon/src/transit_owner.rs:789` | `dispatch_router_garlic` | `now_seconds: u64` |
| `crates/i2pr-daemon/src/transit_owner.rs:942` | `dispatch_build_body` | `now_seconds: u64` |
| `crates/i2pr-daemon/src/transit_owner.rs:1022` | `handle_tunnel_data_inner` | `now_ms: u64` |
| `crates/i2pr-daemon/src/transit_owner.rs:1160` | `handle_gateway_inner` | `now_ms: u64` |

**Two of the four use seconds and two use milliseconds.** The existing code
already reconciles this by converting milliseconds down to seconds before
calling the dispatch builders (`crates/i2pr-daemon/src/transit_owner.rs:1116`
passes `now_ms / 1_000`, and `:1070` charges the relay with `now_ms / 1_000`).
The plan must be explicit about the unit, because a rounding error in the
wrong direction would drop a live fact.

### 3.6 The neighbour: Plan 507's retention window

Plan 507 closed the NetDB retention gap with the same shape: a window
**derived** from the tree's own bound, no new configuration key, and every
outcome counted. The two plans must not disagree about how a retention
window is derived, and they must not be confused with one another in the
documentation.

## 4. Invariants that must not regress

- **The router's own traffic is never claimed as transit work.** This is
  the invariant the facts exist for. A plan that bounds the facts must
  never shorten the window below the point at which a reply the coordinator
  could still recognise could arrive.
- **The error direction is safe.** An expired fact may cause the owner to
  treat traffic as transit work (a false negative on the bypass). A
  missing fact may cause the owner to **claim the router's own traffic as
  transit work** (a false positive on the bypass). The second is the
  dangerous one and the plan must never produce it.
- **No unbounded collection.** After this plan the three sets are bounded
  by the window, not by the number of builds the router performs.
- **No new configuration key.** The window is derived from the tree's own
  constants.
- **No new task, timer, listener or socket.** The sweep rides the pump
  tick that already installs the facts.
- **The transit participation posture (Plan 492) is unchanged.** The facts
  are a bypass aid; the posture decides whether transit happens at all.
- **Plan 492's M11 capability snapshot and the transit evidence lanes are
  unchanged.**
- **Nothing is advertised.** `router.version` stays `0.9.62`; `caps` and
  the published RouterInfo are unchanged; the support inventory's
  advertised count stays **zero**.
- **`i2pr-transport`, `i2pr-tunnel` and `i2pr-proto` are not changed.**
  The facts live in the daemon's own transit owner.

## 5. Scope

### In scope

- the three creator-correlation sets in `TransitLiveOwner`;
- the expiry representation, the install methods and the sweep;
- the four use sites' expiry comparison;
- the production wiring in `crates/i2pr-daemon/src/lib.rs`;
- the window constant and its derivation;
- the tests that prove the bound holds and that a live fact is never
  dropped early.

### Explicitly out of scope

- **The coordinator's own fact set.** It is already bounded and already
  expires. This plan does not touch it.
- **The transit participation posture, the relay accounting and the
  OBEP path.** Plan 492 owns them.
- **A cap instead of a window.** A count cap would drop the *oldest* fact,
  which is exactly the fact whose reply is most likely still to arrive. The
  plan uses a window.
- **Any new configuration key.** The window is derived.
- **The `i2pr-transport` crate's own tunnel-identity handling.** The facts
  are the daemon's, not the transport's.
- **Plan 487 F3's destination-inbound half, the tunnel rekeying
  mechanism, Plan 503's lookup-target gate, Plan 497 limitation 5's read
  half, and every externally-blocked plan.** They are listed in §14.3.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/exploratory_build.rs` | one **new** public constant, `CREATOR_CORRELATION_RETENTION_MS`, derived from `BUILD_DEADLINE_MS` and `BUILD_TUNNEL_ID_RETENTION_MS` |
| `crates/i2pr-daemon/src/transit_owner.rs` | the three sets become `BTreeMap<Key, u64>` (key → expiry); the three install methods take a clock and store an expiry; a new sweep method drops expired facts; the four use sites compare the expiry |
| `crates/i2pr-daemon/src/lib.rs` | `install_transit_creator_correlation` takes the pump tick's clock and sweeps after installing |

No other production file changes. `config.rs` is untouched.

## 7. Design decisions the plan must make explicit

### 7.1 The window is the tree's own bound

The window is

```
CREATOR_CORRELATION_RETENTION_MS = BUILD_DEADLINE_MS + BUILD_TUNNEL_ID_RETENTION_MS
                                   = 60_000 + 2_700_000
                                   = 2_760_000 ms  (46 minutes)
```

It is **not** a new number chosen for convenience. It is the sum of the two
bounds the tree already enforces:

- `BUILD_DEADLINE_MS` — how long a build attempt can stay pending, so the
  fact is installed at the earliest moment it can exist and must survive
  that long before the coordinator's own clock has even started; and
- `BUILD_TUNNEL_ID_RETENTION_MS` — how long the coordinator still remembers
  a retired build's identifiers, so the fact survives exactly as long as
  the coordinator could still recognise a reply.

**This is the protocol decision Plan 502 deferred, and it is made here on
the record.** The reasoning is: the coordinator's own memory of a build's
identifiers is the *definition* of "the coordinator could still accept a
reply for it". The transit owner's fact must therefore live at least as
long as the coordinator's memory, and because the fact is installed while
the build is pending rather than when it is retired, the pending deadline
must be added to it.

### 7.2 The expiry is checked at use time, and swept on the tick

Two things are needed, and they are different:

- **Correctness** is decided **at use time**: a fact is honoured only if it
  has not expired. This is the safe direction — an expired fact is
  declined, which is the benign error.
- **Memory** is reclaimed by a **sweep** that drops the expired entries.
  The sweep rides the pump tick that already installs the facts, so there
  is no second timer.

Checking at use time rather than only sweeping matters: a sweep alone would
leave a fact live until the next tick even if it had already expired, and a
use-time check alone would leave the map growing in memory.

### 7.3 The window is stored in seconds, and every comparison is conservative

The four use sites carry two clock units (§3.5). The expiry is stored in
**seconds**, and every comparison rounds in the **safe** direction:

- at a **seconds** site, the comparison is `expiry_seconds > now_seconds`;
- at a **milliseconds** site, the comparison is
  `expiry_seconds > now_ms / 1_000`. Integer division **floors**, so the
  clock is read as *earlier* than it really is, and a fact is therefore
  honoured for **longer** than it strictly should be. That is the safe
  direction.

The expiry itself is computed with a **ceiling** at install time, so the
stored second is never earlier than the true millisecond expiry:

```
expiry_seconds = (now_ms + RETENTION_MS + 999) / 1000
```

Without the ceiling, a fact could be dropped up to one second before it
expired. With it, a fact is never dropped early.

### 7.4 A fact is replaced, never duplicated

Re-installing a fact that is already present **refreshes** its expiry
rather than leaving the old one behind. A repeated pump tick therefore
does not extend a fact's life indefinitely: the expiry is the install
instant plus the window, and a refresh moves it forward by at most the
window. This is the behaviour that keeps the bound meaningful.

### 7.5 The direction of the error is stated, not assumed

- A fact that has **expired** is declined. The owner may then treat a
  message as transit work that was really the router's own. The coordinator
  has already forgotten the build, so the reply could not have been matched
  by the coordinator either.
- A fact that is **missing** makes the owner claim the router's own traffic
  as transit work. **This must never happen while a reply could still
  arrive**, and §7.1's window is exactly the bound that prevents it.

### 7.6 The Plan 507 lineage is explicit

Plan 507's window is the NetDB's **acceptance** window. This plan's window
is the build coordinator's **identifier-recognition** window. They are
different facts about different subsystems and must not be conflated in the
documentation, in the support inventory or in the counters.

## 8. Work packages

### WP1 — the feasibility artefact

A new out-of-crate test, `crates/i2pr-daemon/tests/transit_creator_retention_feasibility.rs`,
that answers the plan's own question with running code:

- does the owner today hold a fact for ever after it is installed? (the
  gap);
- does a fact installed at the window boundary still get honoured, and one
  installed one second past it get declined? (the mechanism);
- how many facts does the owner hold after N simulated builds under the
  proposed scheme, against the unbounded count today? (the bound).

The artefact must carry **at least two tree-level negative controls** of
its own, so a reader can see it is not vacuous.

**The artefact may stop the plan.** If the window cannot be derived, or if
the bound does not hold, the plan does not proceed.

### WP2 — the window constant

`CREATOR_CORRELATION_RETENTION_MS` in
`crates/i2pr-daemon/src/exploratory_build.rs`, derived from
`BUILD_DEADLINE_MS` and `BUILD_TUNNEL_ID_RETENTION_MS` with a comment that
states the derivation and the two bounds it comes from.

### WP3 — the expiry representation and the sweep

The three sets become `BTreeMap<Key, u64>`; the install methods take a
clock; a sweep method drops the expired entries; the four use sites
compare the expiry in the safe direction.

### WP4 — the production wiring

`install_transit_creator_correlation` takes the pump tick's clock and
sweeps after installing. The call site at
`crates/i2pr-daemon/src/lib.rs:4724` passes the tick's clock.

### WP5 — the static guard

`crates/i2pr-daemon/tests/transit_creator_retention_boundary.rs`, in the
established style: a named function's **own body** must contain the
required **call**, matched with its open parenthesis; comments stripped;
`#[cfg(test)]` blocks excluded; an absent function fails closed. Every rule
carries an **inverted probe** on a copy of the real source.

### WP6 — the live lane

`crates/i2pr-daemon/tests/transit_creator_retention_live.rs`: the
out-of-crate lane that drives the public API, proves the bound holds over
many simulated builds, and proves a live fact is never declined early.

### WP7 — the documentation

The support inventory, `specs/CONFORMANCE.md`, the daemon deep-dive, the
registry and the roadmap — and the closure record.

## 9. Failure, cancellation, restart, and contention semantics

| Situation | Behaviour |
| --- | --- |
| The pump tick does not run | no facts are installed and none are swept; the sets keep whatever they held. The window is not enforced without a tick, and the sets stay **bounded by the previous window's worth of facts**. |
| The sweep runs | expired facts are removed; live facts keep their expiry |
| Two ticks race | the owner is reached through a single `&mut` handle, so the install and the sweep are serialised by the borrow; no lock is added |
| The process restarts | the sets are empty again; the coordinator's own facts are rebuilt; nothing is persisted |
| A fact is installed twice | the second install **refreshes** the expiry |
| The clock jumps backwards | `saturating_add` is used everywhere; an expiry is never in the past because of an overflow |
| The clock jumps forwards | facts expire early, which is the safe direction, and the count of live facts shrinks |
| The coordinator reports no facts | nothing is installed; the sweep still runs, so the sets still shrink |

**Nothing fails open.** The bypass is a *decline*, so a lost fact can only
ever make the router claim its own traffic — which the window prevents —
and never the other way round.

## 10. Compatibility and migration

There is **no migration**:

- no configuration key changes;
- no on-disk format changes;
- the sets are in-memory only and start empty;
- the transit owner's public behaviour is unchanged for every fact that is
  still inside the window, which is every fact the coordinator could still
  recognise;
- the three install methods gain a clock parameter. The three call sites in
  `crates/i2pr-daemon/tests/m11_transit_live_owner.rs` (`:586`, `:673`,
  `:944`) are updated. This is a **source-level** signature change inside
  the workspace, not a wire change.

## 11. Required tests

- the WP1 feasibility artefact, with its own negative controls;
- the static guard, with one inverted probe per rule;
- the live lane, driving only the public API;
- the existing M11 transit lane unchanged:
  `cargo test --locked -p i2pr-daemon --test m11_transit_live_owner -- --test-threads=1`;
- the existing transit evidence checkers unchanged:
  `bash scripts/check-m11-transit-boundaries.sh` and
  `bash scripts/check-m11-transit-qualification-evidence.sh`;
- the workspace floor.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test transit_creator_retention_feasibility -- --test-threads=1
cargo test --locked -p i2pr-daemon --test transit_creator_retention_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test transit_creator_retention_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test m11_transit_live_owner -- --test-threads=1
bash scripts/check-m11-transit-boundaries.sh
bash scripts/check-m11-transit-qualification-evidence.sh
```

plus the routine floor from `AGENTS.md`.

## 13. Documentation updates

- `specs/support.toml` — one new `experimental` surface,
  `transit.creator-correlation-retention`, with `advertised = false`.
- `specs/CONFORMANCE.md` — a Plan 508 section, including the explicit
  statement that the window is a **protocol decision made on the record**
  and that no stock-peer evidence exists.
- `docs/architecture/i2pr-daemon.md` — the `src/transit_owner.rs` and
  `src/lib.rs` module rows, and a Plan 508 section.
- `plans/registry.md` and `plans/subsystems/network-participation-roadmap.md`
  — the Plan 508 rows.

## 14. Acceptance criteria and stop conditions

### 14.1 Acceptance

1. the three creator-correlation sets are bounded over the router's
   lifetime, and the bound is derived from the tree's own constants;
2. a fact is honoured for at least as long as the coordinator could still
   recognise a reply for it;
3. an expired fact is declined, and the decline is **counted**;
4. the sweep reclaims memory without a second timer;
5. the static guard and the live lane are green, and every rule carries an
   inverted probe;
6. the four negative controls fire on the real tree and are reverted
   byte-for-byte;
7. the M11 transit lane and the transit evidence checkers are unchanged and
   green;
8. nothing is advertised, no dependency changes, and no crate outside the
   daemon changes.

### 14.2 Stop conditions

The plan stops, and records the reason, if:

- the window cannot be derived from the tree's own constants;
- the bound cannot be made to hold over the router's lifetime;
- honouring the bound would require the owner to claim the router's own
  traffic as transit work at any point inside the window;
- the change would need a new configuration key;
- the change would need a second task, timer, listener or socket;
- any existing lane or evidence checker would have to be weakened.

### 14.3 What this plan does **not** close

- Plan 487 F3's **destination-inbound half**, which needs tunnel rekeying —
  a new I2NP mechanism the tree does not implement at all;
- Plan 497 limitation 5's **read** half, which needs Plan 433's
  never-constructed coordinator;
- Plan 503's **lookup-target API-version gate**, which is a product
  decision with no protocol basis;
- Plan 491 **F2** and **F4**, Plan 490 Medium #3, Plan 502 M1's siblings;
- every externally-blocked plan: 433, 435, 436, 437, 438, 439, 493, 494;
- the stock-peer question, which needs Plan 493's authorized topology.

## 15. Closure evidence required

- the WP1 feasibility artefact, with its negative controls;
- the static guard, with one inverted probe per rule;
- the live lane;
- **at least four negative controls applied to the real tree and reverted
  byte-for-byte** — removing the expiry check, removing the sweep, widening
  the window to zero, and dropping the expiry from one use site;
- the routine floor's result;
- the requirement-to-evidence matrix;
- the invariant, failure, migration and security reviews;
- the limitations, the findings by severity and the unblock audit.

## 16. Handoff notes

- **The window is the tree's own bound.** If `BUILD_DEADLINE_MS` or the
  tunnel-id retention changes, the derived window changes with it, and the
  derivation is the only place that has to be understood.
- **The expiry comparison must round in the safe direction.** The two
  seconds-unit use sites and the two milliseconds-unit use sites are
  different, and the plan's rule is that a fact is never declined before
  it truly expired.
- **The four use sites are the invariant.** If a fifth use site appears, it
  must compare the expiry too; the static guard must notice.
- **Nothing about the transit posture changes.** The facts are a bypass
  aid; Plan 492's posture decides whether transit happens at all.
- **Plan 507's window is a different window.** The NetDB's acceptance
  window and the build coordinator's identifier-recognition window are
  different facts about different subsystems.
