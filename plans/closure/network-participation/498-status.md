# Plan 498 — The SSU2 introducer (Bob) role owner in the daemon's live inbound loop

## Status

| Field | Value |
| --- | --- |
| Plan | 498 |
| Subsystem | `network-participation` |
| Closure token | `passed-ssu2-introducer-role-owner-in-the-live-inbound-loop` |
| Plan of record | `plans/implementation/network-participation/498-ssu2-introducer-role-owner-in-the-live-inbound-loop.md` |
| Closure record | `plans/closure/network-participation/498-status.md` |
| Predecessor | Plan 490 (`passed-ssu2-introducer-relay-role`) |
| Floor | 97 steps, 97 PASS, 0 FAIL, on tree `78f9e72e` |

Plan 498 discharges **Plan 490 Medium #1**: the daemon's ordinary inbound
pump never drained the runtime's relay handoff, so a Relay Request that a
peer really sent to this router was decoded, authenticated and then dropped.

The role owner is now **reachable in a live daemon**: the daemon's live
inbound loop drains the runtime's bounded relay handoff into it on a bounded
tick, and the role owner answers a requester's signing key **only** from the
NetDB's validated store.

**What is closed**

- **Plan 490 Medium #1** — the live drain, the production resolver, and the
  bounded, counted refusal path.

**What is not closed**

- **Plan 490 Medium #2** — "No live socket carriage of a Relay Request." The
  runtime still has no outbound relay queueing API, so a Relay Intro or
  Relay Response the role owner produces is **counted as
  `awaiting_carriage`** rather than carried to the wire. The finding stays
  open and is recorded in §13 (Unblock audit).

**Nothing was promoted.** `specs/support.toml`, `Cargo.toml` and `Cargo.lock`
are byte-identical; `router.version` stays `0.9.62`; `caps` and the published
RouterInfo are unchanged; `ssu2.advertise` stays refused. The role stays
behind its existing default-off `ssu2.introducer_service` opt-in.

## Commits

| Commit | Subject | Contents |
| --- | --- | --- |
| `b0a20c7d` | docs(plans): register Plan 498, the SSU2 introducer role owner in the live inbound loop | The plan of record, the registry row and the network-participation roadmap row. |
| `b362108c` | feat(network-participation): land the Plan 498 SSU2 introducer role owner in the live inbound loop | `crates/i2pr-runtime/src/ssu2_runtime.rs`, `crates/i2pr-daemon/src/router_i2np.rs`, `bootstrap.rs`, `ssu2_introducer.rs`, the new `ssu2_relay_drain.rs`, `lib.rs`, `service_product.rs`, the new `crates/i2pr-daemon/tests/ssu2_introducer_handoff.rs`, and the plan-of-record amendment. |
| `78f9e72e` | docs(architecture): document the Plan 498 SSU2 introducer relay handoff drain | `docs/architecture/i2pr-daemon.md`, `docs/architecture/i2pr-runtime.md`, `specs/CONFORMANCE.md`. |

## What changed

### `crates/i2pr-runtime/src/ssu2_runtime.rs`

One **non-blocking** accessor beside `next_relay_event()` (`:1384`):

```rust
pub fn try_next_relay_event(&mut self) -> Option<Ssu2InboundRelay>
```

It pulls from the **existing** bounded relay channel (`self.relay.try_recv()`).
No channel is added, the capacity is unchanged, and nothing about the
runtime's authentication path changes. `Empty` and `Disconnected` both yield
`None`.

`tokio::select!` over the handle's two receivers is impossible: `next_inbound`
and `next_relay_event` both take `&mut self` on the same handle, and `select!`
constructs both futures up front. That is why the drain runs on its own tick.

### `crates/i2pr-daemon/src/router_i2np.rs`

`Ssu2DaemonHandle::try_next_relay_event()` (`:2225`) delegates to the runtime's
accessor, beside `next_inbound()` (`:2216`).

`Ssu2DaemonHandle::next_relay_dispatched` (`:2249`) is **unchanged**. This
plan supersedes it — the drain reaches the role owner through
`try_next_relay_event` — but does not remove it: Plan 490's closure record
names it as part of that plan's delivered surface, so its removal is a
separate change.

### `crates/i2pr-daemon/src/bootstrap.rs`

`Bootstrap::validated_signing_key(&self, peer_hash) -> Option<SigningPublicKey>`
(`:276`) — a narrow, read-only accessor over the NetDB's validated store.
The answer is `None` for a peer this NetDB does not hold.

`RouterInfoStore` admits only `ValidatedRouterInfo`, and there is no
unchecked insert path, so the key can only come from a RouterInfo the NetDB
already validated.

### `crates/i2pr-daemon/src/ssu2_introducer.rs`

`NetDbSignerResolver` (`:246`) — the production
[`IntroducerSignerResolver`] implementation, over the shared
`Arc<Mutex<bootstrap::Bootstrap>>`.

A poisoned lock is fail-closed: the answer is `None`, so the role owner
refuses the block instead of admitting an unverifiable peer. The bootstrap's
lock is a `std::sync::Mutex`; the critical section is one map lookup, and the
guard is created and dropped inside the synchronous resolver call, so it never
crosses an await.

### `crates/i2pr-daemon/src/ssu2_relay_drain.rs` (new)

The drain.

- `MAX_RELAY_DRAIN_PER_PASS` — 16, the runtime channel's own capacity.
- `RelayEventSource` — the **pull** seam. It cannot enqueue into the
  runtime's relay channel; that channel's sender is private to the runtime
  and is fed only by the authenticated transport path.
- `RelayDrainOutcome { drained, admitted, refused, awaiting_carriage }` —
  counts only: no peer hash, no key, no tag, no payload.
- `drain_relay_events(...)` — pulls until the source is empty or the bound is
  reached, whichever comes first. A refusal is a counted outcome, not an
  error: the drain never fails the caller's loop.

### `crates/i2pr-daemon/src/lib.rs`

- `NormalSsu2Owner::try_next_relay_event()` (`:2320`) and
  `impl RelayEventSource for NormalSsu2Owner` (`:2357`) — the live owner is
  the drain's event source, whichever variant holds the handle.
- The role owner is constructed in the daemon's live inbound loop when
  `ssu2.introducer_service` is set, with the production resolver over the
  shared bootstrap handle.
- A `relay_tick` arm (`:4543`) on a **100 ms** bounded tick with
  `MissedTickBehavior::Skip`, matching the loop's existing tick style. The
  drain's counters are logged at `debug`; no peer identity is logged.

### `crates/i2pr-daemon/src/service_product.rs`

`ServiceProduct::try_next_relay_event()` (`:3104`) — the **pull** only. The
product does **not** own a role owner.

### `crates/i2pr-daemon/tests/ssu2_introducer_handoff.rs` (new)

Five rows: the discriminator, the control, the refusal, the bound, and the
live-wiring static guard.

## Requirement-to-evidence matrix

| # | §5.1 requirement | Evidence |
| --- | --- | --- |
| 1 | **WP1 — the production resolver.** A narrow read-only accessor on the bootstrap; a production `IntroducerSignerResolver` over the shared handle; the role owner's configuration from the existing opt-in and the role owner's own bounded policy defaults. | `crates/i2pr-daemon/src/bootstrap.rs:276` (`Bootstrap::validated_signing_key`), `crates/i2pr-daemon/src/ssu2_introducer.rs:246` (`NetDbSignerResolver`). Test: `bootstrap::tests::a_validated_peer_yields_its_signing_key_and_an_unknown_peer_yields_none` — a validated peer yields its own signing key, an unknown peer yields none. |
| 2 | **WP2 — the bounded drain on the live path.** A non-blocking relay accessor in the runtime, on the daemon's handle and on the live owner; the role owner in the live loop; a bounded drain on a dedicated tick; the product's pull; the daemon-side pull seam. | `crates/i2pr-runtime/src/ssu2_runtime.rs:1399` (`try_next_relay_event`), `crates/i2pr-daemon/src/router_i2np.rs:2225`, `crates/i2pr-daemon/src/lib.rs:2320` and `:2357` (the `RelayEventSource` impl), `crates/i2pr-daemon/src/lib.rs:4543` (the `relay_tick` arm), `crates/i2pr-daemon/src/service_product.rs:3104` (the pull), `crates/i2pr-daemon/src/ssu2_relay_drain.rs` (the drain and the seam). |
| 3 | **The discriminator.** A relay block in the handoff reaches the role owner and the receipt moves 0→1. | **`the_drain_delivers_a_relay_block_to_the_role_owner`** in `crates/i2pr-daemon/tests/ssu2_introducer_handoff.rs`. The role owner's own `requests_admitted` goes 0→1 and `intros_produced` goes 0→1. A drain that never reached the role owner would leave both counters at zero. |
| 4 | **The control.** An empty handoff delivers nothing and counts nothing. | **`the_drain_delivers_nothing_when_the_handoff_is_empty`** — the drain's outcome is the default and both receipt counters stay zero. This is what makes the discriminator sharp. |
| 5 | **The refusal.** An unresolvable requester is refused with `UnknownRequester`, the refusal is counted by reason, and no state is left behind. | **`the_drain_refuses_a_requester_the_netdb_does_not_hold`** — `refusals["unknown_requester"]` goes 0→1, `requests_admitted` stays 0, and the replay is refused for the **same** reason rather than as a duplicate, which can only happen if no tracked state was created. |
| 6 | **The bound.** One drain pass processes at most the per-pass bound. | **`the_drain_is_bounded_per_pass`** — a handoff of `MAX_RELAY_DRAIN_PER_PASS + 8` events yields exactly `MAX_RELAY_DRAIN_PER_PASS` processed, and the rest stay in the handoff. |
| 7 | **The produced block is counted, not silently dropped.** | `RelayDrainOutcome::awaiting_carriage` in `crates/i2pr-daemon/src/ssu2_relay_drain.rs`; asserted by the discriminator row (`awaiting_carriage == 1`). |
| 8 | **The runtime accessor.** `try_next_relay_event` returns `None` on an empty channel, returns a queued event, and does not consume a later event. | The runtime crate's own unit tests for `try_next_relay_event`: empty → `None` and nothing consumed; a queued event is returned; after the take the next call yields `None`; a disconnected channel yields `None`. |
| 9 | **The live-wiring static guard.** | **`the_daemon_live_loop_drains_the_relay_handoff`** — asserts the live owner is the drain's `RelayEventSource`, the live loop wakes on `relay_tick`, the live loop calls the drain, and the live role owner answers requester keys from the validated store. This is a **static** guard; see §9 (Limitations), item 2. |
| 10 | The routine floor in `AGENTS.md` passes in full. | The floor log. |

## Commands run

### Focused, before the floor

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `CARGO_BUILD_JOBS=4 cargo check --locked -p i2pr-runtime -p i2pr-daemon --all-targets` | clean |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-daemon --test ssu2_introducer_handoff -- --test-threads=1` | 5 passed; 0 failed |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-daemon --lib -- bootstrap` | 14 passed; 0 failed |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | PASS (exit 0) |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-daemon --all-targets --no-fail-fast -- --test-threads=1` | PASS (exit 0) |
| `CARGO_BUILD_JOBS=4 cargo clippy --locked -p i2pr-daemon -p i2pr-runtime --all-targets --all-features -- -D warnings` | clean |
| `bash scripts/check-runtime-boundaries.sh` | `runtime boundary checks passed` (exit 0) |
| `python3 scripts/check-tooling-inventory.py` | `check-tooling-inventory: every published figure matches the tree` |
| `python3 scripts/check-global-plan-number-uniqueness.py` | `Global plan-number ownership check passed.` |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 51 tests, OK |
| `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock` | **empty** |

### The routine floor

`bash /tmp/i2pr-floor-498-run1.sh` → `/tmp/i2pr-floor-498-run1.log`, on tree
`78f9e72e`. **97 steps, 97 PASS, 0 FAIL.** The floor is the
repository's own routine list from `AGENTS.md` plus four Plan 498 lanes:
`plan498-introducer-handoff-lane`, `plan498-introducer-mesh-lane`,
`plan498-bootstrap-resolver-row` and `plan498-runtime-relay-accessor`.

Every one of these steps ran **locally** on this host. Nothing in this closure
is a CI claim.

## Invariant review

The plan's §4 invariants, each with its evidence.

1. **Default-off.** `ssu2.introducer_service` stays `false` by default. With
   it off the daemon constructs no role owner, the `relay_tick` arm finds
   `None`, and the live loop's behaviour is identical to today.
   Evidence: the daemon's own `ssu2_introducer_service_is_available_and_default_off`
   config row (unchanged) and the construction site, which is gated on the
   opt-in.
2. **The key comes from a validated RouterInfo, never from the wire.** The
   resolver answers only from the bootstrap's validated store; an unknown peer
   yields `None` and the role owner refuses the block with
   `UnknownRequester`. Evidence:
   `a_validated_peer_yields_its_signing_key_and_an_unknown_peer_yields_none`,
   `the_drain_refuses_a_requester_the_netdb_does_not_hold`.
3. **Fail closed.** A relay block whose requester cannot be resolved is
   refused, never admitted, and every refusal is counted by reason.
   Evidence: the refusal row.
4. **No state left behind on a refusal.** A refused block creates no tracked
   request, so the replay is refused for the same reason rather than as a
   duplicate. Evidence: the refusal row's replay assertion and the
   `duplicate_request` counter staying absent.
5. **Bounded.** The drain is bounded by the channel's capacity (16) and by
   `MAX_RELAY_DRAIN_PER_PASS` (16). No unbounded queue or channel is
   introduced. Evidence: the bound row, and the runtime's unchanged
   `max_inbound_relay_queue` (default 16, ceiling
   `MAX_SSU2_INBOUND_QUEUE_CEILING`).
6. **No await under the bootstrap lock.** The resolver's guard is created and
   dropped inside a synchronous call, and the critical section is one map
   lookup. Evidence: the resolver's implementation and the
   `RouterInfoStore::get` accessor.
7. **The live path is the path that closes the finding.** The drain runs in
   the daemon's live inbound loop. Evidence: the static guard, and §3.3 of
   the plan-of-record, which records the correction of Plan 490's
   prescription.
8. **No dependency, no advertisement, no promotion.** `Cargo.toml` and
   `Cargo.lock` stay byte-identical; `specs/support.toml` is unchanged;
   `router.version` stays `0.9.62`; `ssu2.advertise` stays refused; `caps`
   and the published RouterInfo are unchanged.

## Failure review

| Failure | Behaviour | Evidence |
| --- | --- | --- |
| The requester is not in the NetDB | Refused with `UnknownRequester`; counted by reason; no state left behind. | `the_drain_refuses_a_requester_the_netdb_does_not_hold` |
| The relay tag is unknown or expired | Refused with `UnknownTag` / `ExpiredTag` by the role owner's own policy; counted. | Plan 490's role-owner rows, unchanged |
| The requester floods | Refused with `RateLimited` by the role owner's per-source window; the flood never reaches the cryptographic path. | Plan 490's role-owner rows, unchanged |
| The request's signature does not verify | Refused by the transport's verification inside `on_relay_request`; counted. | Plan 490's role-owner rows, unchanged |
| The bootstrap's lock is poisoned | The resolver answers `None`; the block is refused and counted. Fail-closed. | `NetDbSignerResolver::signing_key` |
| The role owner is switched off | Every event is refused with `Disabled`; counted. | Plan 490's role-owner rows, unchanged |
| The handoff is empty | The drain returns its default outcome; nothing is counted. | `the_drain_delivers_nothing_when_the_handoff_is_empty` |
| The handoff is full | The runtime's bounded channel drops the oldest entry; the drain never sees an unbounded queue. | The runtime's existing bounded-channel behaviour, unchanged |
| The role owner produces a block | Counted as `awaiting_carriage`. The block is not carried to the wire — Plan 490 Medium #2's scope. | The discriminator row's `awaiting_carriage == 1` |
| The live loop's tick is missed | `MissedTickBehavior::Skip`; the next tick drains. The handoff stays bounded. | The loop's existing tick style |

## Migration review

- **No migration.** No stored format changes. No data-directory changes.
- **Default-off.** With `ssu2.introducer_service = false`, the daemon
  constructs no role owner and the live loop's behaviour is identical to
  today.
- **No configuration change.** The key `ssu2.introducer_service` already
  existed and is already default-off; Plan 498 adds no key and changes no
  key's meaning.
- **No runtime surface change beyond one accessor.** The relay channel, its
  capacity and the runtime's authentication path are untouched.

## Security review

- **The key never comes from the wire.** The resolver answers only from the
  NetDB's validated store; `RouterInfoStore` admits only
  `ValidatedRouterInfo` and has no unchecked insert path. An unknown peer
  yields `None` and the block is refused.
- **The injection seam is not an authentication bypass.** The daemon's seam is
  a **pull** seam: it lets the drain take its event source as a parameter.
  It adds no API that lets anyone enqueue into the runtime's relay channel —
  that channel's sender is private to the runtime and is fed only by the
  authenticated transport path. An injected event still needs a valid
  signature from a key the resolver supplies only for a validated peer.
- **The drain does not widen the trust boundary.** It pulls what the runtime
  already authenticated and hands it to the role owner, which applies the
  role's own policy and the transport's verification.
- **No secret is logged or serialised.** The drain's counters carry counts
  only: no peer hash, no key, no tag, no payload. The live loop logs the
  counters at `debug`.
- **The bootstrap's lock is held for one lookup.** No await under it; a
  poisoned lock is fail-closed.
- **Bounded.** The handoff stays bounded at 16; one drain pass is bounded at
  16; the role owner's tracked requests and source windows are Plan 490's
  bounded tables.
- **No advertisement, no publication, no promotion.** `specs/support.toml`
  stays at 94 surfaces with 0 advertised; `router.version` stays `0.9.62`;
  `ssu2.advertise` stays refused; `caps` and the published RouterInfo are
  unchanged. The role stays behind the existing default-off opt-in.
- **No network behaviour change.** The runtime still decodes and
  authenticates; it still never interprets the role.

## Documentation and operations

- `docs/architecture/i2pr-daemon.md` — the live drain, the role owner's
  placement, the resolver, the correction of Plan 490's prescription, and the
  exact non-claims.
- `docs/architecture/i2pr-runtime.md` — the non-blocking accessor and the
  second bounded channel's consumer.
- `specs/CONFORMANCE.md` — the exact claim and the exact non-claims.
- No operator procedure is needed: the role is behind an existing,
  already-documented, default-off opt-in and Plan 498 adds no key.

## Limitations

Stated plainly, because the closure record is the authority a later reader
will check first.

1. **The produced Relay Intro or Relay Response is not carried to the wire.**
   The runtime has no outbound relay queueing API, so the drain counts the
   block as `awaiting_carriage` and it does not reach a peer. The role's
   **decision** path is now live; its **carriage** path is not. This is
   Plan 490 **Medium #2**, and it stays open.
2. **The live wiring is proved statically, not by a live daemon run.** The
   runtime's relay channel admits no external injection, and the role
   requires the non-strict operator profile, so no end-to-end relay-block
   row is reachable on this host. The drain's **behaviour** is proved
   behaviourally, in-process and socket-free; the live loop's **wiring** is
   proved by a static guard that asserts the drain call, the event source
   and the resolver are present in the live loop.
3. **No stock-peer or public-network evidence.** Every row is in-process.
4. **The role's tag issuance still needs a live target.** The tag is issued
   by the runtime's relay-tag owner; a live target must ask this router for
   one before a requester can name it.
5. **The relay tag is the only in-band link.** A requester can only present a
   tag this router issued for a target this router knows; that is Plan 490's
   design and it does not change.

## Findings by severity

### High

None open. No high-severity finding was recorded by this plan.

### Medium

- **M1 — Plan 490 Medium #2 stays open: the runtime cannot queue a relay
  block on an established session.** The transport already has the session
  queue API (`i2pr-transport-ssu2`'s `queue_relay_intro` and
  `queue_relay_response`); the **runtime** exposes no way to reach it. The
  role owner's produced block is therefore counted as `awaiting_carriage`
  rather than carried. This is recorded, not fixed: it needs its own
  plan-of-record, and a plan-of-record is the honest unit for "the runtime
  gains an outbound relay queueing API and a live three-party mesh proves
  it over UDP".
- **M2 — the live-wiring evidence is a static guard, not a live run.** This
  is a direct consequence of M1's missing injection seam and of the role's
  non-strict-profile requirement. It is stated in §9 so a later reader does
  not mistake the behavioural rows for an end-to-end proof.

### Low

- **L1 — `Ssu2DaemonHandle::next_relay_dispatched` is superseded but
  retained.** It has zero callers and the drain now reaches the role owner
  through `try_next_relay_event`. It is retained because Plan 490's closure
  record names it as part of that plan's delivered surface, so removing it
  would leave a closed plan's record pointing at nothing. Its removal is a
  separate change.
- **L2 — `docs/architecture/i2pr-daemon.md` still carries stale module-row
  line counts** outside the rows this plan touched. Pre-existing, and already
  recorded in `AGENTS.md`.

### Informational

- **I1 — `tokio::select!` over the runtime handle's two receivers is
  impossible.** Both take `&mut self` on the same handle. The daemon's live
  loop solves the same problem by selecting over two *different* objects
  (`lib.rs`: the SSU2 handle and a separate `router_ingress` mutex). The
  drain therefore runs on its own bounded tick. This shaped the design; it
  is recorded so the next plan does not rediscover it.

## Roadmap disposition

| Roadmap | Disposition |
| --- | --- |
| `plans/subsystems/network-participation-roadmap.md` | Plan 498's row moves to `closed` with the closure token `passed-ssu2-introducer-role-owner-in-the-live-inbound-loop` and the closure record path. No other row moves. |
| `plans/subsystems/core-router-recovery-roadmap.md` | Unchanged. |

## Unblock audit

The plan's §14 requires the unblock audit to record the disposition of the
finding this plan set out to discharge.

| Finding | Disposition |
| --- | --- |
| Plan 490 **Medium #1** — the product's poll loop does not deliver live relay blocks to the role owner. | **Closed.** The daemon's live inbound loop drains the runtime's bounded relay handoff into the role owner on a bounded tick; the production resolver answers requester keys only from the NetDB's validated store; every refusal is counted by reason and leaves no state behind. The finding's prescription was **corrected**: `ServiceProduct::poll_inbound` is not on the live path, so the live drain in `lib.rs` is the mandatory half. |
| Plan 490 **Medium #2** — no live socket carriage of a Relay Request. | **Stays open and recorded.** The runtime still has no outbound relay queueing API, so a produced Relay Intro or Relay Response is counted as `awaiting_carriage` instead of being carried. It needs its own plan-of-record. |

**Nothing was moved to `ready`.** The audit considered every registered plan
that lists Plan 498 as a dependency, and every one of them is blocked on an
external environment this host does not have:

| Plan | State after this closure | Why it stays blocked |
| --- | --- | --- |
| 435 | `blocked` | The external two-family public evidence on an authorized two-host topology. |
| 439 | `blocked` | Its external environment. |
| 493 | `blocked` | The authorized two-host topology; the stock-peer introducer lane and the `caps=f` advertisement wait on it. |
| 494 | `blocked` | Gated on Plan 493. |
| 433 | `blocked` | Its work packages 2 and 4 — composing the NetDB / destination-tunnel coordinator into the daemon's service graph — are untouched by this plan. It needs Plans 431 and 432. |
| 436 | `blocked` on its stock-peer lane | Its source-side work packages were delivered by Plan 492; only the exact-pinned i2pd stock lane (work package 5) remains, and it is environmental. |

Plan 490's own closure record is **left exactly as written**. It is the
historical record of that plan; the disposition of its findings is recorded
here and in the registry rows, which is where a later reader looks.
