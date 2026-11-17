# Plan 499 — The SSU2 introducer role's live path: the relay-tag request and the block carriage

| Field | Value |
| --- | --- |
| Plan | 499 |
| Subsystem | `network-participation` |
| State | `closed` (`passed-introducer-live-tag-request-and-carriage`) |
| Depends on | Plan 490 (`passed-ssu2-introducer-relay-role`), Plan 498 (`passed-ssu2-introducer-role-owner-in-the-live-inbound-loop`) |
| Discharges | Plan 490 finding Medium #2 — no live socket carriage of a Relay Request |
| Closure record | `plans/closure/network-participation/499-status.md` |

## 1. Objective

Make the SSU2 introducer (Bob) role **complete on the wire**: a target can
obtain a relay tag from this router, a requester can present that tag, and
the Relay Intro and Relay Response this router produces actually cross a
session to their peers.

Plan 490 built the role owner and the transport's relay machinery. Plan 498
made the role owner **reachable** in a live daemon. Neither of them made the
role **work**: the produced block is still dropped, and no tag is ever
issued, so no introduction can complete. This plan closes the gap end to end.

It delivers five things:

1. the **transport's two missing outbound relay-tag controls**, so a tag
   request can be asked and a tag can be answered on a session;
2. the **runtime's live relay-tag request handling**, which binds a freshly
   minted tag to the *authenticated* peer that asked for it;
3. the **runtime's outbound relay carriage API**, which places a produced
   Relay Intro or Relay Response on an established session;
4. the **daemon's carriage seam**, so the live drain carries a produced block
   instead of dropping it;
5. a **live three-party mesh over real UDP** in which the introduction
   completes end to end.

Nothing is advertised, published or promoted. The role stays behind its
existing default-off opt-in.

## 2. Why ready

- The role owner, its policy, its receipt and the private mesh exist
  (Plan 490, `crates/i2pr-daemon/tests/ssu2_introducer_mesh.rs`).
- The live drain exists and is bounded (Plan 498,
  `crates/i2pr-daemon/src/ssu2_relay_drain.rs`).
- The transport's relay **block vocabulary is complete**: the block types, the
  encoders and the decoders for `RelayTagRequest` (block 15) and `RelayTag`
  (block 16) already exist
  (`crates/i2pr-transport-ssu2/src/block.rs:1467-1468`, `:1581-1582`,
  `:2020-2035`). Only the session's *outbound control queue* lacks them.
- The transport's relay block types already carry a **destination-free**
  payload, so the destination is the session — the same shape the existing
  `queue_relay_intro` / `queue_relay_response` already assume.
- The runtime already owns the session registry, the peer→link index and the
  scheduler wakeup, and already exposes a production-shaped precedent for
  exactly this operation (`queue_controlled_peer_test`,
  `crates/i2pr-runtime/src/ssu2_runtime.rs:2386-2425`).
- The runtime already has an OS CSPRNG (`random_u32_nonzero`,
  `crates/i2pr-runtime/src/ssu2_runtime.rs:1277-1280`).
- Two **live runtime instances already handshake over real loopback UDP**
  (`crates/i2pr-runtime/tests/ssu2_local.rs`), so a three-party live mesh is
  reachable on this host. No external environment is needed.

## 3. Current implementation evidence

### 3.1 The role owner's decision path is live; its carriage path is not

Plan 498 made the drain deliver a relay block to the role owner and the role
owner's receipt move. But the drain **drops the produced block**:

```rust
// crates/i2pr-daemon/src/ssu2_relay_drain.rs:98-105
// The role owner produced a block for a peer. Carrying it is
// Plan 490 Medium #2's scope; the count is what keeps the
// block from being a silent drop.
Ssu2IntroducerOutcome::IntroRequired { .. }
| Ssu2IntroducerOutcome::ResponseRequired { .. } => {
    outcome.admitted = outcome.admitted.saturating_add(1);
    outcome.awaiting_carriage = outcome.awaiting_carriage.saturating_add(1);
}
```

The `..` discards `target`, `intro`, `requester` and `response`. The module
header says so in its own words
(`crates/i2pr-daemon/src/ssu2_relay_drain.rs:22-26`).

### 3.2 The transport can queue a relay block — but only from test code

`Ssu2Session` already owns the outbound controls this plan needs:

| Method | Site |
| --- | --- |
| `queue_relay_request` | `crates/i2pr-transport-ssu2/src/session.rs:1003` |
| `queue_relay_response` | `crates/i2pr-transport-ssu2/src/session.rs:1016` |
| `queue_relay_intro` | `crates/i2pr-transport-ssu2/src/session.rs:1029` |

A repo-wide sweep finds **no production caller** of any of them: the only
call sites are `crates/i2pr-transport-ssu2/tests/peer_relay.rs:257`, `:321`,
`:390` and `crates/i2pr-runtime/tests/ssu2_peer_relay.rs:786`, `:833`.

The transport crate is otherwise complete for this role: the block types
(`block.rs:841-849`, `:948-956`), the encoder (`block.rs:1581-1582` for the
two tag blocks, and the relay blocks beside them), the decoder
(`block.rs:2020-2035`), the single-shot bounded control queue
(`session.rs:573`, bound `DATA_MAX_SENT_PACKETS` = 256 at
`constants.rs:269`) and the transmit path (`session.rs:1159`, `:1200-1202`).

### 3.3 The runtime has no outbound relay queueing API

`Ssu2RuntimeService` exposes the whole session registry
(`state.active: HashMap<LinkId, ActiveSession>` at `ssu2_runtime.rs:1117`,
`state.peer_links: HashMap<PeerId, HashSet<LinkId>>` at `:1118`) and already
reaches a session's control queue from four places —
`queue_controlled_peer_test` (`:2419`), `queue_controlled_address` (`:2467`),
`close_ssu2` (`:3540`) and `queue_to_session` (`:3477`).

**None of them queues a relay block.** The runtime's own module header names
the gap:

```rust
// crates/i2pr-runtime/src/ssu2_peer_relay.rs:31
//! APIs (`Ssu2Session::queue_relay_request` et al.)
```

The destination identity the runtime needs is available: the role owner
returns the destination as a raw 32-byte router hash
(`crates/i2pr-daemon/src/ssu2_introducer.rs:162`, `:170`), and the runtime's
session peer id **is** the peer's router hash
(`PeerId::from_hash(auth.peer().router_hash)`, `ssu2_runtime.rs:4184`).
`PeerId::from_bytes` (`crates/i2pr-transport/src/identity.rs:31`) is the
conversion.

### 3.4 No tag is ever issued, so the introduction leg can never fire

The transport **decodes** a relay-tag request and a relay tag and surfaces
both as session events:

| Session event | Site |
| --- | --- |
| `SessionEvent::RelayTagRequest` | `crates/i2pr-transport-ssu2/src/session.rs:312`, emitted at `:1774-1775` |
| `SessionEvent::RelayTag(u32)` | `crates/i2pr-transport-ssu2/src/session.rs:314`, emitted at `:1777-1779` |

The runtime **never mentions `RelayTag` at all** — a grep over
`crates/i2pr-runtime/src/` returns nothing. The two events fall into the
catch-all arm of the runtime's session-event loop:

```rust
// crates/i2pr-runtime/src/ssu2_runtime.rs:5712
_ => {}
```

And the tag-issuing API has **no production caller**:

| API | Site | Production callers |
| --- | --- | --- |
| `Ssu2RuntimeService::issue_relay_tag` | `ssu2_runtime.rs:1955-1967` | none |
| `Ssu2PeerRelayService::issue_introducer_tag` | `ssu2_peer_relay.rs:665-673` | none |
| `Ssu2IntroducerService::issue_tag` | `crates/i2pr-daemon/src/ssu2_introducer.rs:471-485` | none |

Every call site is test code
(`crates/i2pr-transport-ssu2/tests/peer_relay.rs:246`,
`crates/i2pr-runtime/tests/ssu2_peer_relay.rs:754`,
`crates/i2pr-daemon/tests/ssu2_introducer_handoff.rs:158`,
`crates/i2pr-daemon/tests/ssu2_introducer_mesh.rs:104`).

**This is the load-bearing correction of this plan.** The transport's own
documentation describes the intended flow:

> `crates/i2pr-runtime/src/ssu2_runtime.rs:1949-1953` — "The specification
> binds a relay tag to the introduction target: **the target asks the
> introducer for a tag and publishes it**, and a requester presents that tag
> to name the target."

A carriage API on its own would therefore be **dead code in production**: no
tag is ever bound to a live peer, so the role owner can never produce a Relay
Intro for one, so the carriage would never fire. This plan closes the tag
path **and** the carriage path together, because only the combination makes
the role work.

### 3.5 The tag request cannot even be asked

`Block::RelayTagRequest` (block 15) and `Block::RelayTag` (block 16) exist
and encode (`block.rs:1467-1468`, `:1581-1582`), and the session decodes both
(`block.rs:2020-2035`). But `QueuedControl`
(`crates/i2pr-transport-ssu2/src/session.rs:491-522`) has **no** variant for
either, and there is no `queue_relay_tag_request` / `queue_relay_tag`. A peer
therefore cannot ask this router for a tag, and this router cannot answer.

## 4. Invariants that must not regress

1. **Default-off.** `ssu2.introducer_service` stays `false` by default. With
   it off: the runtime answers no tag request, the transport's tag controls
   are never queued, the daemon's role owner refuses every event with
   `Disabled`, and the router behaves exactly as it does today. The runtime's
   gate is `Ssu2RuntimeConfig::introducer_service`, which the daemon already
   maps from the operator's opt-in
   (`crates/i2pr-daemon/src/router_i2np.rs:1506`).
2. **A tag is bound only to an authenticated peer.** The tag's owner is the
   router hash of the peer on whose **handshake-authenticated session** the
   request arrived. The runtime never issues a tag for a caller-supplied,
   unauthenticated identity, and this plan adds **no** public API that lets a
   caller request a tag for an arbitrary peer.
3. **The tag value is unpredictable and nonzero.** It comes from the OS
   CSPRNG (`random_u32_nonzero`), never from a counter or a clock, and the
   transport rejects zero (`RelayTagBlock::new`,
   `crates/i2pr-transport-ssu2/src/block.rs:1192-1199`).
4. **Tag issuance is bounded.** The transport's introducer enforces
   `MAX_RELAY_TAGS_GLOBAL` = 16, `MAX_RELAY_TAGS_PER_PEER` = 4 and
   `RELAY_TAG_LIFETIME_SECS` = 120
   (`crates/i2pr-transport-ssu2/src/relay.rs:67`, `:71`, `:73`, `:75`). The
   plan adds no table of its own and no unbounded structure.
5. **The signature path is unchanged.** The requester's signature is still
   verified by the transport against the key the daemon's resolver supplies
   only for a peer whose RouterInfo the NetDB validated. A carriage API
   cannot make an unverifiable request verifiable: the carriage happens
   **after** the role owner admitted and verified the request.
6. **Fail closed.** A block that cannot be carried is counted by reason and
   never silently dropped. A refusal is a counted outcome, not an error; the
   drain never fails the caller's loop.
7. **Bounded.** The transport's control queue stays bounded by
   `DATA_MAX_SENT_PACKETS`; the runtime's drain stays bounded by
   `MAX_RELAY_DRAIN_PER_PASS` = 16; no unbounded queue or channel is
   introduced anywhere.
8. **No await under a lock.** The runtime's state mutex is held for the
   lookup and the queueing only, and the daemon's bootstrap mutex only for
   one map lookup. Neither crosses an await.
9. **No dependency, no advertisement, no promotion.** `Cargo.toml` and
   `Cargo.lock` stay byte-identical; `specs/support.toml` stays at 94
   surfaces with 0 advertised; `router.version` stays `0.9.62`;
   `ssu2.advertise` stays refused; `caps` and the published RouterInfo are
   unchanged.

## 5. Scope

### 5.1 In scope

- **WP1 — the transport's two missing outbound relay-tag controls.**
  - `QueuedControl::RelayTagRequest` and `QueuedControl::RelayTag(RelayTagBlock)`.
  - `Ssu2Session::queue_relay_tag_request()` and
    `Ssu2Session::queue_relay_tag(tag)`, with the same two failure modes as
    every other queued control: `Terminated` on a terminated session and
    `LocalPolicyDenied` at the control-queue ceiling.
  - The two transmit arms that map them onto `Block::RelayTagRequest` and
    `Block::RelayTag`.
- **WP2 — the runtime's live relay-tag request handling.**
  - The runtime's session-event loop handles `SessionEvent::RelayTagRequest`
    instead of swallowing it.
  - When the introducer is enabled: mint a tag, bind it to the
    session's authenticated peer, and queue the `RelayTag` answer **on that
    same session**.
  - The transport's introducer quota is the bound; a refused issuance is
    counted by reason.
  - `SessionEvent::RelayTag(u32)` is counted as an observed tag.
- **WP3 — the runtime's outbound relay carriage API.**
  - The **peer-facing** methods on `Ssu2RuntimeService`, all resolving the
    peer's session through the same chain `queue_controlled_peer_test` uses
    and all waking the scheduler:
    `queue_relay_request`, `queue_relay_tag_request`, `queue_relay_intro`
    and `queue_relay_response`.
  - `queue_relay_request` and `queue_relay_tag_request` are the **peers'
    side** of the same mechanism. They are in scope only because the live
    three-party mesh needs its requester and its target to be **real live
    routers** rather than transport-level stubs; no requester or target
    *role* is built, and neither method carries any policy.
  - A typed error that distinguishes *no session*, *ambiguous session*,
    *session terminated* and *control queue full*.
- **WP4 — the daemon's carriage seam and the live drain.**
  - A `RelayCarriage` seam beside the existing `RelayEventSource`, so the
    drain's behaviour can be exercised without a socket.
  - The production carriage over the runtime.
  - The drain **carries** a produced block instead of dropping it, and
    `awaiting_carriage` is replaced by a carried count and a refused count
    with reasons.
- **WP5 — the live three-party UDP mesh and the documentation.**

### 5.2 Explicitly out of scope

- **The RouterInfo publication step.** The transport's documentation says the
  target "asks the introducer for a tag **and publishes it**". Publishing the
  tag in the target's RouterInfo is a NetDB/RouterInfo mechanism, not an
  SSU2 relay-block mechanism. This plan delivers the *ask and answer*; it
  does not publish anything, and it does not advertise the tag.
- **A stock-peer lane.** A lane against the pinned i2pd needs the Plan 493
  topology.
- **The requester and target *roles*.** The transport keeps the requester's
  block-emission API and this plan exposes it on the runtime, because the
  live mesh needs its requester and its target to be real live routers. But
  this router builds **no requester role** — no target selection, no
  correlation table, no response verification on the requester's side — and
  **no target role** beyond answering a tag request.
- **Link selection between two concurrent sessions with one peer.** The plan
  refuses an ambiguous peer and records it as a finding; a link-selection
  policy is a separate change.
- **Any capability promotion, advertisement or version change.**

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-transport-ssu2/src/session.rs` | `QueuedControl::RelayTagRequest`, `QueuedControl::RelayTag(RelayTagBlock)`, `Ssu2Session::queue_relay_tag_request()`, `Ssu2Session::queue_relay_tag(tag)`, and the two transmit arms. The block encoders already exist; no wire-format change. |
| `crates/i2pr-runtime/src/ssu2_runtime.rs` | The session-event loop handles `SessionEvent::RelayTagRequest` and `SessionEvent::RelayTag`; the tag is minted, bound to the session peer and answered on that session. New public `queue_relay_intro` / `queue_relay_response` with a typed error; the daemon-side handle gains the matching passthroughs. |
| `crates/i2pr-daemon/src/ssu2_relay_drain.rs` | A `RelayCarriage` seam; the production `RuntimeRelayCarriage` over the runtime that owns the sessions; the drain **carries** a produced block and counts carried / carriage-refused by reason. `awaiting_carriage` is retired. |
| `crates/i2pr-daemon/src/lib.rs` | The live loop's drain arm passes the production carriage and logs `carried` / `carriage_refused` counts instead of `awaiting_carriage`. |
| `crates/i2pr-daemon/src/ssu2_introducer.rs` | The role owner refuses the new `Ssu2RelayEvent::Tag(_)` as not its role, exactly as it already refuses `Ssu2RelayEvent::Intro`. |
| `crates/i2pr-transport-ssu2/tests/peer_relay.rs` | The transport's own rows for the two new controls, including the two failure modes. |
| `crates/i2pr-runtime/tests/ssu2_local.rs` | The runtime's rows for the tag request and the carriage API. |
| `crates/i2pr-daemon/tests/ssu2_introducer_handoff.rs` | The daemon's carriage rows. |
| `crates/i2pr-daemon/tests/ssu2_introducer_live_mesh.rs` (new) | The live three-party UDP mesh. |

No new workspace dependency. `i2pr-daemon` already depends on `i2pr-runtime`,
`i2pr-netdb`, `i2pr-proto` and `i2pr-transport`.

### 6.1 The tag is minted and answered inside the runtime, not by the daemon

The daemon's role owner must **not** gain an API that issues a tag for a
caller-named peer. If it did, the tag's owner would be whatever the caller
said, and the requester-leg proof ("the tag names the target") would rest on
the caller's honesty.

Instead the runtime answers the request on the session it arrived on. The
peer's router hash comes from the handshake-authenticated session
(`ssu2_runtime.rs:4184`), so the binding is a fact rather than a claim. The
daemon's role owner then *resolves* the target through the runtime's
existing `relay_tag_owner` (`ssu2_runtime.rs:1969-1974`), exactly as it
already does today.

### 6.2 The carriage is a pull-of-a-session, not a new send path

The transport's `queue_relay_intro` / `queue_relay_response` take only the
block: the destination is **the session**. So the runtime's carriage API
resolves the destination peer to one live session and queues the block
there. It reuses the resolution chain `queue_controlled_peer_test` already
uses (`state.peer_links` → `state.active` → `record.session.…`) and the same
scheduler wakeup (`self.shared.notify.notify_one()`), so there is exactly one
way for a control block to reach the wire.

### 6.3 The carriage seam is not an authentication bypass

The daemon-side seam is a **pull** seam in the same shape as
`RelayEventSource`: it exists so the drain's behaviour can be exercised
without binding a socket. It cannot enqueue into the runtime's relay
channel — that channel's sender is private to the runtime and is fed only by
the authenticated transport path — and it cannot make the role owner admit a
block it would otherwise refuse, because the admission and the signature
verification happen **before** the carriage.

## 7. Work packages

### WP1 — the transport's two missing outbound relay-tag controls

- `QueuedControl::RelayTagRequest` and `QueuedControl::RelayTag(RelayTagBlock)`.
- `queue_relay_tag_request()` — empty body, block 15.
- `queue_relay_tag(tag)` — four-byte body, block 16; a zero tag is rejected
  by the block constructor before it reaches the queue.
- Both bounded by `DATA_MAX_SENT_PACKETS` and refused on a terminated
  session, exactly like `queue_relay_intro`.
- Both single-shot: emitted once on the next transmit, not retransmitted.

**Acceptance:** a queued tag request and a queued tag both appear in the
next Data packet's block sequence and decode back on the peer; a terminated
session and a full control queue both refuse.

### WP2 — the runtime's live relay-tag request handling

- The session-event loop handles `SessionEvent::RelayTagRequest`: mint a tag
  (`random_u32_nonzero`), bind it to the session's peer through the existing
  `issue_introducer_tag`, and queue the `RelayTag` answer on the same
  session.
- The runtime's introducer gate (`Ssu2RuntimeConfig::introducer_service`)
  is checked first; with it off the request is ignored and counted.
- The transport's introducer quota is the bound; a refused issuance is
  counted by reason.
- `SessionEvent::RelayTag(u32)` is counted as an observed tag and creates no
  state. The **observed tag is surfaced on the runtime's existing relay
  handoff** as a new `Ssu2RelayEvent::Tag(u32)` variant, because the peer
  that asked for a tag is the peer that has to learn it. The daemon's role
  owner refuses that variant as not its role, exactly as it already refuses
  `Ssu2RelayEvent::Intro`.

**Acceptance:** a live peer that asks for a tag over a real session receives
a `RelayTag` block; the tag resolves through `relay_tag_owner` to that
peer's router hash; with the introducer disabled no tag is issued and no
answer is queued.

### WP3 — the runtime's outbound relay carriage API

- `queue_relay_intro(target: PeerId, block: RelayIntroBlock)` and
  `queue_relay_response(requester: PeerId, block: RelayResponseBlock)`.
- Resolution: the peer's live links; exactly one link is required; an
  ambiguous peer is refused rather than guessed.
- A typed error distinguishing `NoSession`, `AmbiguousSession`,
  `SessionTerminated` and `ControlQueueFull`.
- The scheduler is woken so the block leaves on the next tick.

**Acceptance:** a queued block is encoded into the next Data packet on that
session; each of the four refusals is distinguishable; the peer identity the
API takes is the same identity the session registry is keyed by.

### WP4 — the daemon's carriage seam and the live drain

- A `RelayCarriage` seam beside `RelayEventSource`.
- The production carriage over the runtime the role owner already owns.
- The drain carries a produced `IntroRequired` / `ResponseRequired` block
  and counts it; a carriage that cannot be performed is counted by reason.
- `awaiting_carriage` is removed once the carriage is live — a counter that
  says "waiting for something that now happens" is a stale counter.

**Acceptance:** the drain's counters show a carried block; the produced
block is no longer dropped; a carriage refusal is counted by reason and the
drain continues.

### WP5 — the live three-party UDP mesh and the documentation

- Three live runtime instances over real loopback UDP: a requester, this
  router as the introducer, and a target.
- The target asks the introducer for a tag and receives one.
- The requester presents the tag; the introducer admits it, verifies the
  requester's signature, and produces a Relay Intro.
- The intro is carried onto the introducer→target session and the target
  receives it.
- The target answers with a Relay Response; the introducer verifies it and
  carries it onto the introducer→requester session; the requester receives
  it and verifies the target's signature.
- The architecture deep-dives, `specs/CONFORMANCE.md`, the registry and the
  roadmap.

## 8. Failure, cancellation, restart, and contention semantics

- **The role owner refuses a block.** Counted by reason; no state created;
  the drain continues with the next event.
- **The tag request is refused** (introducer disabled, quota exhausted, or
  the CSPRNG unavailable). Counted; no tag is bound; no answer is queued.
- **The carriage is refused** (no session, ambiguous session, terminated
  session, control queue full). Counted by reason; the block is dropped
  **with a count**, never silently.
- **The drain never fails the loop.** Every refusal is a counted outcome.
- **Cancellation.** The live loop's cancellation arm shuts the owner down;
  the role owner holds no task of its own and the transport's tag and relay
  controls are session state that dies with the session.
- **Restart.** The role owner is reconstructed from configuration at start.
  A tag lives in the transport's introducer table and expires with
  `RELAY_TAG_LIFETIME_SECS`; a restart loses the table, which is safe — a
  requester re-asks for a tag.
- **Contention.** The runtime's state mutex is taken for the peer→link
  lookup and the queueing, with no await held. The transport's control queue
  is bounded at 256 and the drain's per-pass bound at 16, so a burst cannot
  grow an unbounded queue.

## 9. Compatibility and migration

- No migration. No stored format changes. No data-directory changes.
- Default-off: with `ssu2.introducer_service = false` the router answers no
  tag request, carries no relay block and behaves exactly as today.
- No configuration key is added or changed.
- The transport's **wire format is unchanged**: blocks 15 and 16 already
  exist in the vocabulary and already encode and decode. This plan adds the
  session's ability to *emit* them, not a new block type.
- The runtime's public surface grows by two queueing methods and one typed
  error.
- The daemon's public surface grows by one seam and the removal of the
  `awaiting_carriage` counter.

## 10. Required tests

- **The transport's tag rows.** A queued tag request and a queued tag both
  cross a session and decode on the peer; a terminated session refuses; a
  full control queue refuses; a zero tag never reaches the queue.
- **The runtime's tag rows.** A live peer that asks for a tag over a real
  session receives a `RelayTag` block; the tag resolves to that peer's
  router hash; with the introducer disabled no tag is issued.
- **The runtime's carriage rows.** A queued intro and a queued response are
  encoded into the next Data packet on the resolved session; each of the
  four refusals is distinguishable.
- **The daemon's carriage rows.** The drain carries a produced block; the
  counters show it; a carriage refusal is counted by reason; the produced
  block is not dropped.
- **The live three-party mesh.** The introduction completes over real UDP:
  the tag is issued, the request is admitted, the intro reaches the target,
  the response reaches the requester, and the requester verifies the
  target's signature. The evidence artifact holds counts and digests only.
- **The default-off row.** With `ssu2.introducer_service = false`, no tag is
  issued and no relay block is carried.
- The routine floor in `AGENTS.md` passes in full.

## 11. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --test ssu2_introducer_handoff -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_introducer_live_mesh -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

and the full routine floor from `AGENTS.md`.

## 12. Documentation updates

- `docs/architecture/i2pr-transport-ssu2.md` — the two new controls and the
  relay-tag vocabulary.
- `docs/architecture/i2pr-runtime.md` — the live tag request, the carriage
  API and the typed error.
- `docs/architecture/i2pr-daemon.md` — the carriage seam and the live mesh.
- `specs/CONFORMANCE.md` — the exact claim and the exact non-claims. Nothing
  is promoted; the support inventory is unchanged.
- `plans/registry.md` and `plans/subsystems/network-participation-roadmap.md`
  — the row.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

1. The transport's tag rows pass: a tag request and a tag cross a session
   and decode; the two failure modes refuse.
2. The runtime's tag rows pass: a live peer obtains a tag bound to itself;
   the introducer-disabled row passes.
3. The runtime's carriage rows pass: a queued block is encoded on the
   resolved session; each of the four refusals is distinguishable.
4. The daemon's carriage rows pass: the drain carries the produced block and
   counts it; the block is no longer dropped.
5. The live three-party mesh passes: the introduction completes over real
   UDP and the requester verifies the target's signature.
6. The default-off row passes.
7. The full routine floor passes.
8. `specs/support.toml`, `Cargo.toml` and `Cargo.lock` are byte-identical.

### 13.2 Stop conditions

Stop and record a finding, rather than working around it, if:

- the tag request could only be answered by trusting a caller-supplied peer
  identity instead of the authenticated session's peer;
- the carriage would require the daemon to reach into the runtime's private
  relay channel, or any unbounded queue or channel;
- the live mesh could only be completed by weakening a signature check, a
  quota, a bound, or the default-off gate;
- closing the finding would require a capability promotion, an
  advertisement, or a version change;
- the mesh could only be completed with an external environment — in that
  case the source-side work lands and the mesh lane is recorded as blocked,
  exactly as Plan 493 records its own external dependency.

## 14. Closure evidence required

The closure record must carry:

- the commits;
- a requirement-to-evidence matrix, with the **live three-party mesh** named;
- the exact commands and their outcomes;
- an invariant review against §4, row by row;
- a failure review, a migration review and a security review;
- the limitations, stated plainly — including that the RouterInfo
  publication step and a stock-peer lane are **not** closed here;
- findings by severity;
- the roadmap disposition;
- the unblock audit, recording what moved to `ready` — and what did not.

## 15. Handoff notes

- Plan 490's closure record is **not** rewritten. The disposition of its
  Medium #2 finding is recorded in this plan's closure record and in the
  registry rows.
- Plan 498's closure record is **not** rewritten either; its `awaiting_carriage`
  counter is superseded by this plan's carried / refused counters.
- The role stays **available but not enabled** by default, and the truthful
  status string is unchanged
  (`crates/i2pr-daemon/src/config.rs:2034-2040`).
- The `Ssu2DaemonHandle::next_relay_dispatched` seam
  (`crates/i2pr-daemon/src/router_i2np.rs:2249`) remains superseded and
  retained; Plan 490's closure record names it as that plan's delivered
  surface.
