# Plan 498 — The SSU2 introducer (Bob) role owner in the daemon's live inbound loop

| Field | Value |
| --- | --- |
| Plan | 498 |
| Subsystem | `network-participation` |
| State | `registered` (`registered-introducer-role-owner`) |
| Depends on | Plan 490 (`passed-ssu2-introducer-relay-role`) |
| Discharges | Plan 490 finding Medium #1 — the product's poll loop does not deliver live relay blocks to the role owner |
| Closure record | `plans/closure/network-participation/498-status.md` |

## 1. Objective

Make the SSU2 introducer (Bob) role owner **reachable in a live daemon**.

Plan 490 built the role owner, the runtime's relay handoff, the configuration
key and the privacy-safe receipt, and closed. It left one recorded finding
open, and this plan closes it: the daemon's ordinary inbound pump never
drains the runtime's relay handoff, so a Relay Request that a peer really
sends to this router is decoded, authenticated and then **dropped**.

This plan delivers three things:

1. a **production signing-key resolver** that answers a peer's signing key
   from a RouterInfo the NetDB already validated — never from the wire;
2. a **bounded drain** of the runtime's relay handoff into the role owner, on
   the daemon's **live** inbound path;
3. two **discriminator tests** that prove the wiring is real: one that shows a
   relay block reaching the role owner through the product loop, and one that
   shows an unknown requester being refused and counted.

Nothing is advertised, published or promoted. The role stays behind its
existing default-off opt-in.

## 2. Why ready

- The role owner is complete and proven by Plan 490's private controlled mesh
  (`crates/i2pr-daemon/tests/ssu2_introducer_mesh.rs`), which drives
  `on_relay_event` **directly** and therefore bypasses the handoff entirely.
- The runtime's relay handoff exists and is bounded
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:1705`, `max_inbound_relay_queue`
  default 16).
- The daemon already maps the operator's opt-in into the runtime
  (`crates/i2pr-daemon/src/router_i2np.rs:1506`), so the runtime decodes and
  authenticates relay blocks into a channel that **nobody drains**.
- The configuration key exists and is default-off:
  `ssu2.introducer_service` (`crates/i2pr-daemon/src/config.rs:637-638`,
  default `false` at `:1402-1404`), refused unless `ssu2.enabled = true`
  (`:3353-3358`).
- No external environment is needed: the discriminators are in-process.

## 3. Current implementation evidence

### 3.1 The relay handoff exists and is fed

The runtime decodes and authenticates relay blocks and puts them on a
**second bounded channel**, separate from the I2NP handoff
(`crates/i2pr-runtime/src/ssu2_runtime.rs:830-838`, `:1705`, `:6872-6887`).
The channel's capacity is `max_inbound_relay_queue`, default **16**, with a
ceiling of `MAX_SSU2_INBOUND_QUEUE_CEILING` (`:231`, `:289-292`).

The receiver is `Ssu2ServiceHandle::next_relay_event()`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:1384-1386`).

### 3.2 Nothing drains it

- `crates/i2pr-daemon/src/router_i2np.rs:2240-2249` —
  `RouterI2npRuntime::next_relay_dispatched` is the only production seam that
  delivers a handoff event to the role owner. It has **zero callers**
  anywhere in the tree, and it is **unreachable**: `NormalSsu2Owner`
  (`crates/i2pr-daemon/src/lib.rs:2288-2291`) exposes `next_inbound`,
  `advance_groups` and `process_group_inbound`, but never the relay seam.
- `crates/i2pr-daemon/src/service_product.rs:2782-2803` —
  `ServiceProduct::poll_inbound` awaits `self.next_inbound()` and never
  touches the relay handoff.
- `crates/i2pr-daemon/src/ssu2_introducer.rs:229-233` —
  `IntroducerSignerResolver` has **no production implementation**; the only
  implementations are the mesh test's `HashMap` and the module's own
  `#[cfg(test)]` helper.
- `Ssu2IntroducerService::new` (`crates/i2pr-daemon/src/ssu2_introducer.rs:255-268`)
  is called only from the module's test module and from the mesh test.

### 3.3 The finding's prescription is incomplete, and this plan corrects it

Plan 490's finding says:

> Closing this means adding the relay drain to `ServiceProduct::poll_inbound`
> and resolving the requester's signing key from the product's validated
> store.

The first half is **not sufficient**. The daemon's own source says
`poll_inbound` is not on the live path:

> `crates/i2pr-daemon/src/service_product.rs:3654-3655` — "advance runs via
> the daemon loop; **poll_inbound is not on the live path**".

The live inbound pump is the daemon loop in
`crates/i2pr-daemon/src/lib.rs:4022-4470`, whose `tokio::select!` (`:4090-4103`)
arms the SSU2 I2NP handoff and the NTCP2 ingress, and whose `group_tick` arm
(`:4449-4469`) advances the destination pools.

A plan that patched only `poll_inbound` would close the finding on paper while
a live router still dropped every relay block. **This plan drains on the live
path.**

### 3.3.1 The role owner has exactly one owner

The drain lives in the daemon's live inbound loop, and the role owner lives
with it. `NormalSsu2Owner` has two variants — `Router(Ssu2DaemonHandle)` when
no destination groups are configured, and `Groups(Box<ServiceProduct>)` when
they are — and the live loop drains through whichever holds the handle. The
product therefore exposes the **pull** only
(`ServiceProduct::try_next_relay_event`); it does not own a role owner.

Two owners would mean two sets of counters and an ambiguity about which one
is authoritative. The qualification drivers, which drive `ServiceProduct`
directly and never enter the daemon's live loop, are not the live path and do
not need a second owner.

### 3.4 Where the authoritative validated store lives

Four owners hold a `RouterInfoStore`:

| Owner | Site | Live? | Reachable from the daemon loop? |
| --- | --- | --- | --- |
| `bootstrap::Bootstrap` | `crates/i2pr-daemon/src/bootstrap.rs:215`, accessor `store()` at `:261` | **Yes** — mutated by `ingest_authenticated_router_store` (`:268-281`), called from `lib.rs:3483` and `lib.rs:4351` | **Yes** — the composition root holds `Arc<Mutex<Bootstrap>>` (`lib.rs:133`, `:688`, `:2180`, `:2681`, `:3321`; `std::sync::Mutex`, `lib.rs:97`) |
| `NetDbTunnelCoordinator` | `crates/i2pr-daemon/src/netdb_tunnels.rs:258` | Per-owner | No — Plan 433's blocked scope |
| `DestinationTunnelCoordinator` | `crates/i2pr-daemon/src/destination_tunnels.rs:620` | **Stale** — seeded once at `service_product.rs:2354`, never written again | Yes, but it is a start-time snapshot capped by `MAX_GROUP_PROVIDER_ROUTER_INFOS` (`bootstrap.rs:287-293`), a *destination-candidate* cap |
| `PeerParticipationDriver` | `crates/i2pr-daemon/src/netdb_participation.rs:217` | Starts empty (`lib.rs:1892`); only floodfills are indexed (`lib.rs:2003`) | No |

Only the bootstrap's store is authoritative, live and complete. The product's
own store is a stale snapshot, so a peer ingested after start would be
permanently `UnknownRequester`.

## 4. Invariants that must not regress

1. **Default-off.** `ssu2.introducer_service` stays `false` by default. With it
   off, the daemon constructs no role owner, drains nothing, and behaves
   exactly as it does today.
2. **The key comes from a validated RouterInfo, never from the wire.** The
   resolver answers only from the NetDB's validated store. `RouterInfoStore`
   admits only `ValidatedRouterInfo`
   (`crates/i2pr-netdb/src/router_info.rs:217-221`); there is no unchecked
   insert path. An unknown peer yields `None`, and the role owner refuses the
   block with `UnknownRequester`.
3. **Fail closed.** A relay block whose requester cannot be resolved is
   refused, never admitted. Every refusal is counted by reason.
4. **No state left behind on a refusal.** A refused block creates no tracked
   request and no source window, so replaying the same event is refused the
   same way rather than becoming a duplicate.
5. **Bounded.** The drain is bounded by the channel's capacity (16) and by an
   explicit per-pass cap. No unbounded queue or channel is introduced.
6. **No await under the bootstrap lock.** `Arc<Mutex<Bootstrap>>` is a
   `std::sync::Mutex` (`lib.rs:97`). The resolver's critical section is one
   map lookup and it never awaits.
7. **The live path is the path that closes the finding.** The drain runs in
   the daemon's live loop, not only in a qualification helper.
8. **No dependency, no advertisement, no promotion.** `Cargo.toml` and
   `Cargo.lock` stay byte-identical; `specs/support.toml` stays at 94 surfaces
   with 0 advertised; `router.version` stays `0.9.62`; `ssu2.advertise` stays
   refused (`config.rs:3345-3349`); `caps` and the published RouterInfo are
   unchanged.

## 5. Scope

### 5.1 In scope

- **WP1 — the production resolver.**
  - A narrow read-only accessor on `bootstrap::Bootstrap` that returns a
    peer's signing key from its validated RouterInfo.
  - A production `IntroducerSignerResolver` implementation over the shared
    `Arc<Mutex<Bootstrap>>`.
  - The role owner's configuration, derived from the existing opt-in and the
    role owner's existing bounded policy defaults
    (`max_requests_per_minute` 60, `max_tracked_requests` 64,
    `crates/i2pr-daemon/src/ssu2_introducer.rs:43-46`).
- **WP2 — the bounded drain on the live path.**
  - A **non-blocking** relay accessor in the runtime
    (`Ssu2ServiceHandle::try_next_relay_event`) and on the daemon's handle
    (`Ssu2DaemonHandle::try_next_relay_event`) and on `NormalSsu2Owner`.
  - The role owner constructed in the daemon's live loop when the opt-in is
    set, and a bounded drain on a dedicated tick.
  - The product exposes the **pull** only
    (`ServiceProduct::try_next_relay_event`), so the live loop drains
    through whichever owner holds the handle.
  - A daemon-side **pull seam** so the discriminators can drive the handoff
    without a socket.
- **WP3 — the discriminators and the documentation.**
  - The two discriminator tests.
  - The privacy-safe receipt surfaced on the daemon's status surface.

### 5.2 Explicitly out of scope

- **Plan 490 Medium #2 — "No live socket carriage of a Relay Request."** That
  finding needs the outbound queueing API *and* a live three-party mesh over
  UDP. It is a separate plan-of-record and is not touched here.
- **Plan 433** — the NetDB / destination-tunnel coordinator's composition
  into the daemon's service graph. This plan never constructs
  `NetDbTunnelCoordinator`.
- **Plan 436 work package 3** — the transit owner in the product composition.
  This plan installs the **introducer** role owner, which is a different role
  on a different path.
- **The `caps=f` advertisement** (Plan 494, gated on Plan 493) and the
  stock-peer introducer lane (Plan 493's topology). Neither is reachable here.
- **Advertisement of any kind.** `ssu2.advertise` stays refused.
- **The role's policy shape.** The admission policy, the per-source rate
  limit, the tracked-request bound and the refusal vocabulary are Plan 490's
  closed surface and do not change.
- **Any change to the runtime's relay authentication.** The runtime still
  decodes and authenticates; it still never interprets the role.

## 6. Required production changes

| Crate / file | Change |
| --- | --- |
| `crates/i2pr-runtime/src/ssu2_runtime.rs` | A **non-blocking** `Ssu2ServiceHandle::try_next_relay_event()` beside `next_relay_event()` (`:1384`). `try_recv` on the existing bounded relay channel; `Empty` and `Disconnected` both yield `None`. No new channel, no new capacity, no change to the authentication path. |
| `crates/i2pr-daemon/src/router_i2np.rs` | `Ssu2DaemonHandle::try_next_relay_event()` beside `next_inbound()` (`:2216`). The existing `RouterI2npRuntime::next_relay_dispatched` (`:2240`) keeps its contract; this plan makes it reachable or supersedes it, and says which. |
| `crates/i2pr-daemon/src/bootstrap.rs` | A narrow read-only accessor returning a peer's signing key from the validated store. |
| `crates/i2pr-daemon/src/ssu2_introducer.rs` | A production `IntroducerSignerResolver` implementation over the shared bootstrap handle. |
| `crates/i2pr-daemon/src/ssu2_relay_drain.rs` (new) | The bounded drain, the pull seam, and the counted outcome. |
| `crates/i2pr-daemon/src/lib.rs` | The role owner in the live loop, the bounded drain on a dedicated tick, and the `NormalSsu2Owner` relay accessor. |
| `crates/i2pr-daemon/src/service_product.rs` | The pull accessor, so the live loop drains through the product's handle when the product owns it. |
| `crates/i2pr-daemon/tests/ssu2_introducer_handoff.rs` (new) | The discriminator rows and the live-wiring static guard. |

No new workspace dependency. `i2pr-daemon` already depends on `i2pr-runtime`,
`i2pr-netdb` and `i2pr-proto` (`crates/i2pr-daemon/Cargo.toml:25`, `:27`,
`:28`).

### 6.1 Why a non-blocking accessor rather than a second `select!` arm

`Ssu2ServiceHandle::next_inbound` (`:1378`) and `next_relay_event` (`:1384`)
both take `&mut self` on the **same** handle. `tokio::select!` constructs both
futures up front, so two simultaneous `&mut` borrows of one handle do not
compile. The live loop solves the same problem by selecting over two
*different* objects (`lib.rs:4091-4103`: the SSU2 handle and a separate
`router_ingress` mutex).

This plan therefore adds a non-blocking accessor and drains on a dedicated
bounded tick. The alternative — splitting the handle's two receivers into two
public accessors so a `select!` becomes possible — would widen the runtime's
public surface for no functional gain, and the drain-on-tick keeps the
worst-case latency explicit and small.

### 6.2 The pull seam, and why it is not an authentication bypass

The runtime's relay channel is private: its sender is created inside the
runtime and cloned only into the runtime's own socket loops
(`crates/i2pr-runtime/src/ssu2_runtime.rs:1705`, `:1738`, `:1752`). This plan
adds **no** API that lets anyone enqueue into it.

The daemon-side seam is a **pull** seam: it lets the drain function take its
event source as a parameter, so a test can supply events without a socket.
It cannot inject into the runtime, and it cannot bypass the role owner's
verification: the role owner checks the requester's signature against the key
the resolver supplies, and the resolver supplies a key only for a peer whose
RouterInfo the NetDB already validated. An injected event still needs a valid
signature from that peer's private key.

## 7. Work packages

### WP1 — the production signing-key resolver

- A narrow, read-only accessor on `bootstrap::Bootstrap` that returns a
  peer's signing key from its validated RouterInfo, or `None`.
- A production `IntroducerSignerResolver` implementation over the shared
  `Arc<Mutex<Bootstrap>>`.
- The role owner's configuration derived from the existing opt-in and the
  role owner's own bounded policy defaults.
- The role owner's `local_hash` is the daemon's own router hash, taken from
  the same identity the runtime was started with.

**Acceptance:** with a validated peer in the bootstrap store, the resolver
returns that peer's signing key; with an unknown peer it returns `None`; and
the critical section never awaits.

### WP2 — the bounded drain on the live path

- `try_next_relay_event` in the runtime, on the daemon's handle, and on
  `NormalSsu2Owner`.
- The role owner constructed in the live loop when `ssu2.introducer_service`
  is set; absent otherwise, and the drain is a no-op.
- A bounded drain: pull until the channel is empty, capped by an explicit
  per-pass bound that is at most the channel's capacity.
- The drain runs on a dedicated tick with `MissedTickBehavior::Skip`, matching
  the loop's existing tick style (`lib.rs:3815-3816`, `:4008-4010`).
- The same drain in `ServiceProduct::poll_inbound`, before and after the
  I2NP wait.

**Acceptance:** a relay block in the handoff is delivered to the role owner by
the live loop; the role owner's receipt moves from 0 to 1.

### WP3 — the discriminators and the documentation

- The two discriminator rows in
  `crates/i2pr-daemon/tests/ssu2_introducer_handoff.rs`.
- The receipt surfaced on the daemon's status surface, counts only.
- The architecture deep-dives, the tooling inventory and `specs/CONFORMANCE.md`.

## 8. Failure, cancellation, restart, and contention semantics

- **The role owner refuses a block.** The refusal is counted by reason and no
  state is created. The drain continues with the next event.
- **The drain never fails the loop.** A refusal or a resolver miss is a
  counted outcome, not an error. The loop's health contract is unchanged.
- **Cancellation.** The live loop's cancellation arm (`lib.rs:4025-4028`)
  shuts the owner down and returns; the role owner is dropped with the loop
  and holds no task of its own.
- **Restart.** The role owner is reconstructed from configuration at start.
  It holds no durable state: the receipt is a counter that starts at zero.
- **Contention.** The bootstrap's `std::sync::Mutex` is taken for one lookup
  per relay event, inside the drain, with no await held. The channel is
  bounded at 16, so a burst cannot grow the queue.

## 9. Compatibility and migration

- No migration. No stored format changes. No data-directory changes.
- Default-off: with `ssu2.introducer_service = false`, the daemon constructs
  no role owner and the live loop's behaviour is identical to today.
- The configuration key is unchanged: `ssu2.introducer_service` already
  exists and is already default-off.
- The runtime's public surface grows by exactly one non-blocking accessor.
  The relay channel, its capacity and the authentication path are unchanged.

## 10. Required tests

- **The discriminator.** A relay block placed in the handoff is delivered by
  the drain to the role owner: the receipt's `requests_admitted` goes 0→1 and
  `intros_produced` goes 0→1. A drain that never reached the role owner
  would leave both counters at zero, so the row is sharp.
- **The control.** With an empty handoff, nothing is delivered and nothing is
  counted. The control is what makes the discriminator sharp.
- **The refusal.** With a resolver that has no key for the requester, the
  block is refused with `UnknownRequester`, the refusal counter goes 0→1,
  `requests_admitted` stays 0, and replaying the identical event is refused
  the same way rather than becoming a duplicate — which can only happen if
  no tracked state was created.
- **The bound.** A drain pass over a full channel processes at most the
  per-pass bound.
- **The produced block is counted, not silently dropped.** The role owner
  produces a Relay Intro or Relay Response that must still be carried to a
  peer. The runtime has no outbound relay queueing API — that is Plan 490
  Medium #2's scope — so the drain counts the block as
  `awaiting_carriage` rather than dropping it without a trace.
- **The resolver's own rows.** A validated peer in the NetDB yields its own
  signing key; an unknown peer yields none, so the role owner refuses it.
- **The live-wiring static guard.** The daemon's live inbound loop calls the
  drain, the live owner is the drain's event source, and the live role owner
  answers requester keys from the validated store. This is a **static**
  guard: it proves the wiring is present, not that a live daemon executed
  it, because the runtime's relay channel admits no external injection and
  the role requires the non-strict operator profile.
- **Default-off.** With `ssu2.introducer_service = false`, the daemon
  constructs no role owner.
- **The runtime accessor.** `try_next_relay_event` returns `None` on an empty
  channel, returns a queued event, and does not consume a later event.
- The routine floor in `AGENTS.md` passes in full.

Every one of these rows is socket-free. The existing private mesh
(`crates/i2pr-daemon/tests/ssu2_introducer_mesh.rs`) supplies the fixtures:
`MeshSigners`, `endpoint()`, `inbound()`, `SigningPrivateKey::from_bytes`,
`relay_request_preimage`, `RelayRequestBlock::new`,
`generate_controlled_identity`, `owner.issue_tag`.

## 11. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test ssu2_introducer_handoff -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_introducer_mesh -- --test-threads=1
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

and the full routine floor from `AGENTS.md`.

## 12. Documentation updates

- `docs/architecture/i2pr-daemon.md` — the live drain, the role owner's
  placement, the resolver, and the exact non-claims.
- `docs/architecture/i2pr-runtime.md` — the non-blocking accessor and the
  second bounded channel's consumer.
- `specs/CONFORMANCE.md` — the exact claim and the exact non-claims. Nothing
  is promoted; the support inventory is unchanged.
- `plans/registry.md` and `plans/subsystems/network-participation-roadmap.md`
  — the row.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

1. The discriminator row passes: a relay block reaches the role owner through
   the drain and the receipt moves 0→1.
2. The control row passes: with an empty handoff, nothing is delivered and
   nothing is counted.
3. The refusal row passes: an unresolvable requester is refused and counted,
   and no state is left behind.
4. The default-off row passes.
5. The bound row passes.
6. The produced-block row passes: a produced Relay Intro or Relay Response is
   counted as `awaiting_carriage` and never silently dropped.
7. The resolver's rows pass: a validated peer yields its key and an unknown
   peer yields none.
8. The live-wiring static guard passes.
9. The runtime's accessor rows pass, and the runtime's existing suite is
   unchanged.
10. The full routine floor passes.
11. `specs/support.toml`, `Cargo.toml` and `Cargo.lock` are byte-identical.

### 13.2 Stop conditions

Stop and record a finding, rather than working around it, if:

- closing the finding would require constructing `NetDbTunnelCoordinator`
  (Plan 433's scope) or installing the transit owner (Plan 436 WP3);
- the resolver would have to read a key from anything other than a validated
  RouterInfo;
- the drain would need an unbounded queue or channel;
- the live path cannot be reached without an external environment;
- the only way to prove the wiring would be a public API that lets a caller
  enqueue into the runtime's relay channel;
- closing the finding would require a capability promotion or an
  advertisement.

## 14. Closure evidence required

The closure record must carry:

- the commits;
- a requirement-to-evidence matrix, with the **discriminator** named;
- the exact commands and their outcomes;
- an invariant review against §4, row by row;
- a failure review, a migration review and a security review;
- the limitations, stated plainly — including that the live socket carriage
  of a Relay Request (Plan 490 Medium #2) is **not** closed here;
- findings by severity;
- the roadmap disposition;
- the unblock audit, recording what moved to `ready` — and what did not.

## 15. Handoff notes

- Plan 490's closure record is **not** rewritten. The disposition of its
  Medium #1 finding is recorded in this plan's closure record and in the
  registry rows, which is where a later reader looks.
- Plan 490 Medium #2 (no live socket carriage of a Relay Request) stays open
  and is recorded against the next plan-of-record. It needs the outbound
  queueing API and a live three-party mesh.
- The role stays **available but not enabled** by default, and the truthful
  status string is unchanged
  (`crates/i2pr-daemon/src/config.rs:2034-2040`).
