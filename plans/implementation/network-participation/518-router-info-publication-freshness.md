# Plan 518 — the router's own record is re-dated and re-signed

**Implementation plan.** The plan of record for the network-participation
subsystem. Registered 2026-11-03, immediately after Plan 517's closure. The
plan number is load-bearing and is never renumbered.

## 1. Objective

Make the router's **own** RouterInfo carry a **fresh** publication date.

Plan 515 made the publication periodic. Plan 516 made it confirmable. Plan
517 made it retry. All three re-send the **same bytes**, and those bytes
carry the publication date the router signed at **startup**. The record
therefore ages: the netdb's acceptance window is
`DEFAULT_MAX_AGE` — 24 hours (`crates/i2pr-netdb/src/router_info.rs:20`),
mirrored by `LOCAL_ROUTER_INFO_MAX_AGE_SECS`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:130`) — and the reference's own
netdb expires routers between one and a half and twenty-seven hours
(`libi2pd/NetDb.hpp:49-51`).

So after a day of uptime the router keeps republishing a record the network
will not accept, every 39 minutes, with no counter and no diagnostic. The
publication is not slow: it is **dead**.

The reference avoids this by re-dating and re-signing its own record before
each publish. This plan makes the tree do the same.

## 2. Why ready

- The identity bundle is already alive in the composition root
  (`crates/i2pr-daemon/src/lib.rs:3809`, an `Arc<RouterIdentityBundle>`), so
  the daemon can re-sign without a new dependency.
- The crypto layer already signs a RouterInfo's unsigned region
  (`crates/i2pr-crypto/src/lib.rs:380`, `sign_router_info`) and verifies one
  (`verify_router_info`), so a re-sign is a small, well-placed addition and
  not a new mechanism.
- The proto layer already encodes the unsigned region from the record's own
  semantic parts (`crates/i2pr-proto/src/common/router_info.rs:187`,
  `encode_unsigned`), so a re-encode with a fresh date needs no new
  accessor and no lossy round trip.
- The record's **key** is derived from the router **identity**, not from the
  signed bytes (`crates/i2pr-netdb/src/router_info.rs:249`), so a re-dated
  record is the **same router** to the network. That is what makes
  re-signing mid-life safe.
- The runtime already accepts a **replacement** record
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3260`,
  `install_local_router_info`), and existing authenticated sessions keep the
  RouterInfo they already exchanged.

## 3. Current implementation evidence

> **The line numbers in this section are the survey-time ones**, recorded
> before this plan's implementation moved the code. After the implementation:
> `generate_identity_with_keys` is at
> `crates/i2pr-daemon/src/router_i2np.rs:2000` and its frozen startup date at
> `:2054`; the install is still at
> `crates/i2pr-daemon/src/lib.rs:4013`; and Plan 515's refresh now reads the
> **cycle's record** from the owner at `crates/i2pr-daemon/src/lib.rs:5089`,
> not the installed copy.

### The gap

- `crates/i2pr-daemon/src/router_i2np.rs:1951` (`generate_identity_with_keys`)
  signs the local record with `Date::from_millis(wall_secs() * 1000)` at
  `:2005`. That is the **startup** clock, and it is frozen for the life of
  the process.
- `crates/i2pr-daemon/src/lib.rs:4013` installs those bytes into the SSU2
  runtime, **once**, at the SSU2 service's startup.
- `crates/i2pr-daemon/src/lib.rs:5072` — Plan 515's refresh — reads
  `installed_local_router_info()` and republishes **those bytes** every 39
  minutes. The date does not advance.
- `crates/i2pr-netdb/src/router_info.rs:20` — the netdb refuses a remote
  record older than 24 hours, so the tree's **own** floodfill service
  refuses a DSM carrying the router's record once the router has been up for
  a day.

### The reference

- `libi2pd/RouterInfo.cpp:1218` — `LocalRouterInfo::CreateBuffer` calls
  `RefreshTimestamp()` (`:1164`, `m_Timestamp = GetMillisecondsSinceEpoch()`)
  and then **re-signs** the record with the router's own key.
- `libi2pd/RouterContext.cpp:262` — `UpdateRouterInfo` calls
  `CreateBuffer`, so every update re-dates and re-signs.
- `libi2pd/RouterContext.cpp:1125` — `UpdateTimestamp(ts)` calls
  `UpdateRouterInfo` when `ts > m_LastUpdateTime + ROUTER_INFO_UPDATE_INTERVAL`,
  and `ROUTER_INFO_UPDATE_INTERVAL` is `30 * 60` seconds
  (`libi2pd/RouterContext.h:33`).
- `libi2pd/RouterContext.cpp:1474` — `HandlePublishTimer` calls
  `UpdateTimestamp` **before** `Publish()`. The reference's **resend rungs**
  (`:1560`) do **not** re-date: one cycle publishes one record.
- `libi2pd/NetDb.hpp:49-51` — the reference's netdb expires routers after
  one hour (floodfill) to twenty-seven hours, so a record that is never
  re-dated does leave the network's view.

## 4. Invariants that must not regress

1. **Nothing is advertised.** `router.version` stays `0.9.62`; `caps` and
   the published RouterInfo's **content** are unchanged; the inventory's
   advertised count stays zero.
2. **The router's hash does not change.** A re-dated record is the same
   router; the key is derived from the identity.
3. **The record's content does not change.** Only the publication date and
   the signature may differ from the startup record.
4. **The version gate holds.** Every target of every batch is still selected
   through `select_version_gated_floodfills`.
5. **A stale record is never published.** A record older than the netdb's
   acceptance window is refused and counted, not published.
6. **A record that cannot be re-dated is refused, not published stale.**
7. **The cycle publishes one record.** Every rung of a cycle publishes the
   same bytes.
8. **No new timer.** The re-dating rides the publication tick that already
   begins the cycle.
9. **No secret is logged.** The re-dating logs a reason, never a key.

## 5. Scope

**In scope.**

- A runtime-neutral way to **re-date and re-sign** the router's own
  RouterInfo.
- The composition root's re-dating at the start of each publication cycle,
  and the **install** of the fresh record.
- The publication's **staleness rule**: a record older than the netdb's
  acceptance window is refused, and the refusal is counted.
- The lane, the guard, the records, the controls and the closure.

**Out of scope.**

- The **floodfill** role's separate `caps=f` snapshot
  (`crates/i2pr-daemon/src/floodfill.rs`). The controlled floodfill lane is
  a different profile with its own record and its own evidence; this plan
  covers the participation record the router publishes to the network.
- The **private-LAN** record, which the operator hands to a lab peer out of
  band and which is never published.
- The reference's **direct-or-tunnelled** transport choice, its publish
  interval's **variance**, and its **congestion** update. Plan 517's record
  already records all three as not adopted.
- Any change to the wire format, to `Cargo.toml` or to `Cargo.lock`.
- The stock-peer half, which stays blocked on Plan 493's authorized two-host
  topology.

## 6. Production changes

- `crates/i2pr-proto/src/common/router_info.rs` — a method that returns the
  signed region re-encoded with a **fresh** publication date, built from the
  record's own decoded parts so nothing else can drift.
- `crates/i2pr-crypto/src/lib.rs` — a function that re-dates and re-signs an
  encoded RouterInfo with the router's own key, and **verifies** the result
  before returning it.
- `crates/i2pr-daemon/src/router_info_publication.rs` — the publication's
  **staleness rule**, runtime-neutral: it takes the record's bytes and a
  clock reading, and says whether the record may be published.
- `crates/i2pr-daemon/src/lib.rs` — the composition root re-dates the record
  **once per cycle**, installs the fresh record, and publishes it. A
  re-dating failure is **refused and counted**, and the cycle ends.

## 7. Design decisions the plan must make explicit

- **D1 — The record is re-dated once per cycle, not once per rung.** The
  reference dates the record in `HandlePublishTimer`
  (`libi2pd/RouterContext.cpp:1474`), once per cycle, and its resend rungs
  (`:1560`) publish the same bytes. Our cycle must do the same, so every
  rung of a cycle publishes one record and the correlator's registered
  record never changes mid-cycle.
- **D2 — The composition root owns the re-dating.** The daemon holds the
  identity bundle and the clock. The publication module stays
  runtime-neutral: it takes a record and a reading, and never reads a clock
  or holds a key.
- **D3 — The fresh record is installed as well as published.** The runtime's
  installed copy is what the router hands out in `SessionConfirmed`, so a
  peer that learns the router through a handshake must receive the fresh
  date too. The reference keeps one record for both.
- **D4 — Only the date changes.** The signed region is re-encoded from the
  decoded record's own parts, so the addresses, the peers and the options
  are preserved exactly. The date is the only semantic change, and the
  signature necessarily follows it.
- **D5 — The router's hash does not change.** The RouterInfo's key is
  derived from the router **identity**
  (`crates/i2pr-netdb/src/router_info.rs:249`), not from the signed bytes.
  A re-dated record is the same router to the network, which is what makes
  the re-sign safe mid-life.
- **D6 — A record that cannot be re-dated is refused, never published
  stale.** The publication is refused under its own reason and counted; the
  cycle ends.
- **D7 — The staleness rule is explicit and measured.** A record older than
  the netdb's acceptance window is refused and counted, because publishing
  it wastes a delivery and hides the fault. The window is the netdb's own
  `DEFAULT_MAX_AGE`, not a second constant.
- **D8 — The re-dating rides the cycle's start.** No new timer: the
  publication tick that already begins the cycle is where the record is
  re-dated.
- **D9 — The record's freshness is checked against the same window the
  network uses.** The tree must not invent a second, more generous age: the
  refusal must fire exactly when a peer would refuse the record.

## 8. Work packages

- **WP1** — `i2pr-proto`: the signed region re-encoded with a fresh
  publication date.
- **WP2** — `i2pr-crypto`: the re-date-and-re-sign function, with the
  result verified before it is returned.
- **WP3** — `i2pr-daemon`: the publication's staleness rule and its
  counter.
- **WP4** — `i2pr-daemon`: the composition root's re-dating, install and
  publication.
- **WP5** — the lane, the guard, the records, the controls and the closure.

## 9. Failure, cancellation, restart, and contention semantics

- **A record that cannot be re-dated** is refused under its own reason, the
  refusal is counted, and the cycle ends. The router keeps running.
- **A record that is already stale** — older than the netdb's acceptance
  window — is refused under its own reason and counted. It is never
  published, because a peer would refuse it anyway.
- **A cancellation** during the re-dating leaves the previously installed
  record in place; the publication is refused and the cycle ends.
- **A restart** re-signs the record from the persisted identity, so the
  date starts again at the new process's clock.
- **Contention** — the re-dating happens before the cycle's first batch, so
  the correlator's registered record is the fresh one for every rung.
- **The install** is atomic: the runtime replaces the record under its lock,
  and existing sessions keep the RouterInfo they already exchanged.

## 10. Compatibility and migration

- **No wire-format change.** The record has the same structure; only its
  date and its signature differ.
- **No configuration key.** The cadence is the publication's own period.
- `Cargo.toml` and `Cargo.lock` stay byte-identical. No new dependency.
- The published RouterInfo's **content** is unchanged, so a peer that
  already holds the record holds the same router.
- Plan 515's, Plan 516's and Plan 517's closure records are **not**
  rewritten, and their lanes and guards stay green and byte-identical.

## 11. Required tests

**The lane** (`crates/i2pr-daemon/tests/router_info_publication_refresh.rs`):

- a record built at `T0` is **stale** at `T0 + 24h` and **publishable** at
  `T0`;
- the re-dated record carries the **new** date, the **same** router hash,
  and the **same** content — the addresses, the peers and the options;
- the re-dated record's **signature verifies**, and the original's does not
  cover the new date;
- the re-dated record is **accepted** by the netdb's validator at the new
  reading, and the original is **refused** at the same reading;
- a record that cannot be re-dated is **refused**, and the refusal is
  **counted**;
- a **stale** record is refused by the publication's staleness rule and the
  refusal is counted;
- the cycle's **rungs publish one record**: the record registered with the
  correlator does not change between rungs;
- the re-dating is **deterministic** in its content: two re-datings of the
  same record at the same reading produce the same content.
- the cycle's **one record** is held: the first batch and every rung publish
  the same bytes, and the record is the owner's rather than a copy read back
  out of the runtime;

**The guard** (`crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`):

- the re-dating happens **once per cycle**, at the cycle's start, and not
  per rung;
- the re-dated record is the one that is **published** — the publication
  path must not fall back to the installed startup bytes;
- the **staleness rule** uses the netdb's own acceptance window, and not a
  second constant;
- a stale record is **refused**, never published;
- the re-dating **never logs** a key, a signature or the record's bytes;
- the proto's re-encode preserves the record's **content**;
- the crypto's re-sign **verifies** its own result;
- the module stays **runtime-neutral**: it reads no clock and holds no key;
- the **rungs** publish the cycle's record and do not re-read the installed
  copy, and that record is the **owner's**.

## 12. Exact verification commands

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
python3 -m unittest discover -s tests/planning -p 'test_*.py'
python3 scripts/check-tooling-inventory.py
cargo test --locked -p i2pr-daemon --test router_info_publication_refresh -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_refresh_boundary -- --test-threads=1
```

## 13. Documentation updates

- `specs/support.toml` — a new surface for the re-dated publication, and the
  inventory's counts.
- `specs/CONFORMANCE.md` — the section on the re-dating, the two
  divergences, and the stock-peer limit.
- `specs/QUALIFICATION-GATES.md` — the new section for the re-dating's
  gates.
- `docs/architecture/i2pr-daemon.md` — the new section, and the
  `src/router_info_publication.rs`, `src/lib.rs`, `src/router_i2np.rs`,
  `crates/i2pr-crypto` and `crates/i2pr-proto` rows.
- `docs/architecture/i2pr-netdb.md` — the acceptance window's new role.
- `docs/architecture/i2pr-crypto.md` and `docs/architecture/i2pr-proto.md` —
  the new API.

## 14. Acceptance criteria and stop conditions

The plan is complete when:

1. the router's published record carries a **fresh** publication date, and
   the date advances from cycle to cycle;
2. the record's **content** and the router's **hash** are unchanged;
3. a record older than the netdb's acceptance window is **refused and
   counted**, never published;
4. a re-dating failure is **refused and counted**, never published stale;
5. every rung of a cycle publishes **one** record;
6. the lane, the guard and the negative controls are green;
7. the routine floor is green on the closing tree;
8. the records, the closure record, the registry row and the roadmap row are
   in place.

**Stop conditions.** The plan stops and a successor takes over if:

- the re-sign cannot preserve the record's content exactly;
- the runtime's install would invalidate an existing session's RouterInfo;
- the re-dating would need a **new timer** or a **new dependency**;
- the stock-peer evidence would be needed to close the plan.

## 15. Closure evidence required

- the lane and the guard, with their counts;
- the negative controls, each caught by an **assertion**, with the tree
  restored byte-for-byte;
- the routine floor's full table;
- the requirement-to-evidence matrix;
- the invariant, failure, migration and security reviews;
- the findings by severity, the roadmap disposition and the unblock audit.

## 16. Amendment log

**Amended during implementation, 2026-11-03.** Section 11's required tests
gained the cycle's **one record** — the lane test
`the_cycle_holds_one_record_for_every_rung` and the guard's two rules
`the_rungs_publish_the_cycle_record` and `the_cycle_record_is_the_owners`.

**Why.** The first implementation kept invariant 7 true only *by accident*.
The cycle's first batch published the fresh record the composition root had
just re-dated, but every **rung** read the record back out of the runtime's
installed copy. Those are the same bytes only while the install succeeded,
and the proof that a re-dated record always installs is a reading of the
runtime's code rather than a test. Had the install failed, a rung would have
published the **stale** record while the correlator held the **fresh** one,
so the acknowledgement the correlator matched would have named a record the
cycle had stopped publishing.

**What changed.** The cycle's record moved into the owner
(`set_cycle_record` / `cycle_record`), and `refresh_router_info` and
`advance_publication_cycle` read it from there rather than taking it as a
parameter — so there is one source of truth rather than two. The composition
root sets it once, at the cycle's start; the relay arm no longer re-reads the
installed copy at all.

The plan number, the objective, the nine invariants and the decisions
**D1**–**D9** are unchanged.

## 17. Handoff notes

- Plan 517's closure record is **not** rewritten. This plan closes the
  **next** defect in the same path: Plan 517 made the publication retry,
  and this plan makes the thing it retries **current**.
- Plan 491's **F2** — the stock-peer half — stays blocked on Plan 493's
  authorized two-host topology and is **not** reopened here.
- Plan 517's **M1** — the publication's delivery shares the SSU2 service's
  loop — stays open and is owned by a successor plan.
- The next free plan number after this one is **519**.
