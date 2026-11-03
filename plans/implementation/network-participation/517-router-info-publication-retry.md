# Plan 517 — an unconfirmed publication is retried at the next closest floodfill

**Implementation plan.** The plan of record for the network-participation
subsystem. Registered 2026-11-03, immediately after Plan 516's closure. The
plan number is load-bearing and is never renumbered.

## 1. Objective

Make the router's RouterInfo publication **retry**.

Plan 515 made the publication periodic. Plan 516 made it **confirmable**:
the store message carries the correlator's minted token, the floodfill
answers with a `DeliveryStatus`, and the router records the answer. What is
still missing is the other half of the reference's mechanism: the reference
does not publish once and wait 39 minutes — it publishes, and then, while
the answer has not arrived, it **asks the next closest floodfill**.

Today a publication that is delivered but never confirmed is simply
unconfirmed. The next attempt is 39 minutes away. The reference's retry
ladder is the difference between a router that reaches the network within
seconds and one that reaches it within the hour.

## 2. Why ready

- Plan 516 put the correlation token **on the wire** and joined the inbound
  owner to the correlator, so the confirmation exists and is observable.
- The selection already takes an **exclusion slice**
  (`select_version_gated_floodfills(store, key, excluded, policy)`), so an
  exclusion set is a supported input rather than a new mechanism.
- The correlator already holds a **bounded attempt book** with the
  `Delivered`, `DeliveryFailed` and `Acknowledged` states, so an attempt
  that is abandoned can be recorded rather than forgotten.
- The SSU2 service's loop already carries a **100 millisecond** tick
  (`crates/i2pr-daemon/src/lib.rs:4106`), so a confirmation deadline can be
  observed without introducing a new timer.

## 3. Current implementation evidence

**The reference's cycle**, at the pin
(`635b013a612ff47278ef02acf8580a28e10e26c5`, i2pd 2.61.0):

- `libi2pd/RouterContext.h:34` — `ROUTER_INFO_PUBLISH_INTERVAL = 39*60`
  seconds; `:35` — `ROUTER_INFO_INITIAL_PUBLISH_INTERVAL = 10` seconds;
  `:37` — `ROUTER_INFO_CONFIRMATION_TIMEOUT = 1600` milliseconds; `:38` —
  `ROUTER_INFO_MAX_PUBLISH_EXCLUDED_FLOODFILLS = 15`.
- `libi2pd/RouterContext.cpp:1470` `HandlePublishTimer` — the cycle starts:
  the exclusion set is **cleared**, the reply token zeroed, `Publish()` is
  called, and the resend timer is armed.
- `libi2pd/RouterContext.cpp:1492` `Publish` — picks the closest eligible
  floodfill **not in the exclusion set** (`:1502`), sends it with a fresh
  reply token, and **inserts it into the exclusion set** (`:1540`). If the
  set is larger than the bound it is cleared and the cycle restarts
  (`:1495-1498`).
- `libi2pd/RouterContext.cpp:1560` `HandlePublishResendTimer` — every
  `1600` milliseconds, `Publish()` again and re-arm. A **drop** also
  triggers it (`:1516-1521`).
- `libi2pd/RouterContext.cpp:1321` `PostDeliveryStatusMessage` — a
  matching token reports *"Publishing confirmed"*, **clears the exclusion
  set** and schedules the next publish at the full interval.

**The tree today.** The refresh asks the closest three eligible floodfills
**in one pass**, once per period (Plan 515), and Plan 516 records the
confirmation when it arrives. There is no retry: an unconfirmed
publication waits for the next period.

**The selection takes an exclusion slice.** The refresh's own selection
passes only the local hash
(`crates/i2pr-daemon/src/router_info_publication.rs`, `publication_targets`),
so a cycle's exclusion set is the same mechanism with a longer slice.

## 4. Invariants that must not regress

1. **A publication is never retried before its deadline.** The cycle's
   deadline is the reference's `1600` milliseconds from the moment the
   batch was sent.
2. **A target is never asked twice in one cycle.** The exclusion set holds
   every target the cycle has asked.
3. **The cycle is bounded.** The reference's bound of fifteen asked
   targets holds, and a cycle that reaches it ends and counts the targets
   it asked.
4. **A confirmation ends the cycle.** A confirmed publication is not
   retried, and the next cycle starts at the full period.
5. **The fan-out stays bounded.** A batch is at most the tree's three
   targets, and a cycle is at most the reference's fifteen.
6. **The version gate still holds.** Every target is selected through the
   same version-gated function.
7. **Nothing is advertised.** `router.version` stays `0.9.62`; the
   published RouterInfo is unchanged; the advertised count stays zero.

## 5. Scope

**Lands:**

- a **cycle**: a bounded ladder of **batches**, each batch being the
  closest eligible targets the cycle has not asked;
- the cycle's **exclusion set**, so a target is never asked twice;
- the **confirmation deadline**, the reference's `1600` milliseconds,
  observed on the loop's existing 100 millisecond tick;
- the **retry**: a batch that is delivered but unconfirmed when its
  deadline passes is followed by the next batch, up to the bound;
- a **delivery refusal** advances immediately rather than waiting for the
  deadline;
- a **confirmation ends the cycle**;
- the records: the inventory surface, the conformance note, the
  qualification gate, and the deep-dives.

**Does not land:**

- the reference's **direct-or-tunnelled** choice of the publication's
  transport: this tree publishes over its direct SSU2 session, and its
  non-loopback binds are forbidden, so the tunnelled branch is unreachable
  here;
- the reference's **variance** on the publish interval; Plan 515 decided
  against it and that decision stands;
- the reference's **congestion** update; that is the bandwidth-metering
  line, which the roadmap records as genuinely hard and still open
  elsewhere;
- any change to the published RouterInfo's content;
- any **live** stock-peer evidence, which remains Plan 493's topology.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/router_info_publication.rs` | the cycle: its exclusion set, its deadline, its bound, and the counted outcomes |
| `crates/i2pr-daemon/src/lib.rs` | the publication tick starts a cycle; the 100 ms tick advances it; the inbound confirmation ends it |
| `crates/i2pr-daemon/tests/` | the new lane and the new boundary guard |

## 7. Design decisions the plan must make explicit

- **D1 — A cycle is a ladder of batches, not of single targets.** The
  reference asks one floodfill at a time. This tree asks the closest
  **three** in one pass, which Plan 515 already justified as not weaker in
  reach — it reaches three peers immediately where the reference reaches
  one and then waits. The tree's batch is therefore the ladder's rung, and
  the divergence is recorded rather than imitated.
- **D2 — The exclusion set is the cycle's.** Every target the cycle has
  asked is excluded from the next selection, so a target is never asked
  twice in one cycle. The selection already takes the exclusion slice; the
  local hash stays excluded throughout.
- **D3 — The deadline is the reference's 1600 milliseconds**, measured
  from the moment the batch was sent. The tree observes it on the loop's
  existing 100 millisecond tick, so the deadline is honoured to within
  that granularity.
- **D4 — The cycle is bounded by the reference's fifteen asked targets.**
  A cycle that reaches the bound without a confirmation ends, and the next
  period starts a fresh cycle — the same outcome as the reference's "clear
  the set and try again".
- **D5 — A delivery refusal advances immediately.** A target that refused
  the store will not answer, so waiting out its deadline wastes the
  window.
- **D6 — A confirmation ends the cycle.** The reference's behaviour
  exactly: the exclusion set is cleared and the next publish is scheduled
  at the full interval.
- **D7 — The cycle's clock is the loop's existing tick.** No new timer is
  introduced, and the loop's wake-up cadence does not change.
- **D8 — The cycle's state is runtime-neutral.** It takes `now_ms` from
  its caller and never reads a clock, like the schedule it extends, so the
  whole ladder is provable without a wall clock, a socket or a sleep.

## 8. Work packages

- **WP1** — the cycle's state: the exclusion set, the awaited deadline,
  the bound, and the counted outcomes.
- **WP2** — the selection with the cycle's exclusion set.
- **WP3** — the composition: the publication tick starts a cycle, the
  100 ms tick advances it, the inbound confirmation ends it.
- **WP4** — the lane and the boundary guard, the negative controls, and
  the records.

## 9. Failure, cancellation, restart, and contention semantics

- A cycle that is cancelled mid-flight is `Cancelled`, never confirmed.
- A router that restarts loses its cycle, and a confirmation for a previous
  process is never counted: the token space is per-process.
- Two cycles cannot overlap: a cycle in progress holds the publication, and
  the periodic tick does not start a second one.
- The exclusion set is bounded by the reference's fifteen, and the
  attempt book is bounded by the correlator's own ceiling.

## 10. Compatibility and migration

- **No wire-format change.** The retry asks the **same** store message at
  a different peer; nothing about the message changes.
- **No configuration key.** The cadence is a constant, like the period.
- `Cargo.toml` and `Cargo.lock` stay byte-identical. No new dependency.

## 11. Required tests

**The lane** (`crates/i2pr-daemon/tests/router_info_publication_retry.rs`):

- a cycle asks the closest eligible targets, and the **second** batch
  excludes the ones already asked;
- a cycle is **not** retried before the reference's deadline;
- a cycle **is** retried when the deadline passes with no confirmation;
- a confirmation **ends** the cycle;
- a delivery refusal advances **immediately**;
- the cycle is **bounded** by the reference's fifteen;
- a target is **never** asked twice in one cycle;
- the fan-out stays bounded at three;
- the cycle never reads a clock.

**The boundary guard**
(`crates/i2pr-daemon/tests/router_info_publication_retry_boundary.rs`):

- the retry is driven by the loop's **existing** tick, and no new timer is
  introduced;
- the deadline is the reference's constant;
- the cycle's exclusion set reaches the selection;
- the bound is the reference's constant;
- a confirmation ends the cycle;
- the cycle never reads a clock;
- the fan-out stays bounded.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test router_info_publication_retry -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_retry_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_publication_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

The AGENTS.md routine floor, on the closing tree, plus this plan's two steps.

## 13. Documentation updates

- `specs/support.toml` — the surface's notes gain the retry, and the
  advertised count stays zero.
- `specs/CONFORMANCE.md` — the retry is stated as an implemented,
  structurally proved capability, and the **stock-peer** half is stated as
  still blocked on Plan 493's topology.
- `specs/QUALIFICATION-GATES.md` — a new section: *how the router retries a
  publication the network did not acknowledge*.
- `docs/architecture/i2pr-daemon.md` — the cycle, the composition, and
  re-pointed citations.

## 14. Acceptance criteria and stop conditions

**Accepted when:**

1. a cycle asks the closest eligible targets, and the next batch excludes
   the ones already asked;
2. a cycle is not retried before the reference's deadline, and is retried
   once it passes with no confirmation;
3. a confirmation ends the cycle, and a delivery refusal advances
   immediately;
4. the cycle is bounded by the reference's fifteen, and the fan-out by the
   tree's three;
5. the retry rides the loop's existing tick, and no new timer is
   introduced;
6. the routine floor is green on the closing tree, and the negative
   controls are caught by an assertion;
7. nothing is advertised: `router.version` stays `0.9.62`, the inventory's
   advertised count stays zero, and `Cargo.toml` and `Cargo.lock` are
   byte-identical.

**Stop and record when:**

- no peer confirms and the cycle reaches its bound — the cycle ends, the
  targets are counted, and the next period starts a fresh cycle;
- no stock peer answers — the **stock-peer** half stays blocked on Plan
  493's topology, and no live claim is made.

## 15. Closure evidence required

- the lane and the guard, with their exact outcomes;
- the negative controls, each caught by an **assertion**;
- the routine floor's table on the closing tree;
- the requirement-to-evidence matrix, the invariant review, the failure and
  migration reviews, the limitations, the findings by severity, the roadmap
  disposition and the unblock audit;
- the registry and roadmap rows.

## 16. Handoff notes

- Plan 516's closure record is **not** rewritten. Plan 516's finding **F1**
  is discharged by this plan, and the disposition is recorded here and in
  the registry row.
- Plan 515's and Plan 516's closure records are not rewritten, and their
  guards stay green and byte-identical unless this plan explicitly
  extends them.
- The reference pins are frozen: i2pd `635b013a612ff47278ef02acf8580a28e10e26c5`,
  Java I2P `9134f808337b401e8e53c73734c81fab04280c9d`.
- No boundary script is weakened. If a rule needs to change, the boundary
  is fixed and the rule is left alone.
