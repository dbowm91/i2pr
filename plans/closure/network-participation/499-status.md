# Plan 499 — Status: the SSU2 introducer role's live path

## Status

**Closed** as `passed-introducer-live-tag-request-and-carriage`.

The SSU2 introducer (Bob) role now completes **on the wire**. Plan 490 built
the role owner and the transport's relay machinery; Plan 498 made the role
owner reachable in a live daemon. Plan 499 makes the role work: the
introducer can be asked for a relay tag, and a relay block it produces is
carried onto the session with the peer it is for.

The evidence is a **live three-party UDP mesh**: Alice (the requester), Bob
(this router, the introducer) and Charlie (the target) are three live
routers over real loopback UDP, and the introduction completes end to end —
the tag is issued and bound to the target, the request is admitted and its
signature verified, the Relay Intro reaches the target, the Relay Response
is verified and reaches the requester, and the requester verifies the
response under the target's public key over the transport's exact preimage.

The mesh needed **no external environment**: two live runtime instances
already handshake over real loopback UDP in
`crates/i2pr-runtime/tests/ssu2_local.rs`, so a third party is reachable on
this host.

Nothing was promoted. `specs/support.toml` stays at 94 surfaces (88
experimental, 5 deferred, 1 ready) with **0 advertised**; `router.version`
stays `0.9.62`; `caps` and the published RouterInfo are unchanged;
`ssu2.advertise` stays refused; `specs/support.toml`, `Cargo.toml` and
`Cargo.lock` are byte-identical. The role stays behind its existing
**default-off** `ssu2.introducer_service` opt-in.

**Plan 490 Medium #2 is discharged** ("no live socket carriage of a Relay
Request"). Plan 490's Medium #1 was discharged by Plan 498. Plan 490's
Medium #3 (no stock-peer introducer lane) **stays open** and is recorded
below — it needs the authorized two-host topology.

## Commits

| Commit | What |
| --- | --- |
| `6df6cf9d` | `docs(plans): register Plan 499, the SSU2 introducer role's live path` |
| `0c2339a8` | `docs(plans): amend Plan 499 for the mesh's peer-facing block emission` |
| `99bc4603` | `feat(network-participation): land the Plan 499 introducer live path` |
| `c128ab3f` | `docs(architecture): document the Plan 499 introducer live path` |
| (this commit) | `docs(plans): close Plan 499 and advance the network-participation roadmap` |

## What changed

### The transport: two missing outbound relay-tag controls

`crates/i2pr-transport-ssu2/src/session.rs`

The block vocabulary was **already complete**. `Block::RelayTagRequest`
(block 15, empty body) and `Block::RelayTag` (block 16, a nonzero 4-byte
tag) existed, already encoded (`block.rs:1581-1582`) and already decoded
(`block.rs:2020-2035`). What was missing was the session's ability to
**emit** them: `QueuedControl` had no variant for either, and there was no
`queue_*` method.

- `QueuedControl::RelayTagRequest` and `QueuedControl::RelayTag(RelayTagBlock)`
  (`session.rs:508-511`).
- `Ssu2Session::queue_relay_tag_request()` (`session.rs:1051`) and
  `Ssu2Session::queue_relay_tag(block)` (`session.rs:1067`), with exactly
  `queue_relay_intro`'s two failure modes: `SessionError::Terminated` on a
  terminated session and `SessionError::LocalPolicyDenied` at the
  `DATA_MAX_SENT_PACKETS` (256) ceiling.
- The two transmit arms (`session.rs:1243-1244`) and the two **rebuild** arms
  (`session.rs:1215-1216`) — the latter was a second exhaustive match over
  `QueuedControl` that the plan-of-record had not named; omitting it is a
  compile error, not a style issue.

**The wire format did not change.** This plan adds the session's ability to
emit two blocks that were already in the vocabulary.

### The runtime: the live relay-tag request

`crates/i2pr-runtime/src/ssu2_runtime.rs`

The runtime **never mentioned `RelayTag` at all**. Both session events fell
into the catch-all `_ => {}` arm of the session-event loop, and
`issue_relay_tag` had no production caller — so no tag was ever bound to a
live peer, and the role owner could never produce a Relay Intro for one.

The handler (`ssu2_runtime.rs:3039`) does five things, in this order:

1. **Gate.** `self.shared.peer_relay.introducer_enabled()` — derived from
   `Ssu2RuntimeConfig::introducer_service`, which the daemon maps from the
   operator's `ssu2.introducer_service` opt-in. With the introducer off, no
   tag is issued and no answer is queued.
2. **Mint.** `random_u32_nonzero()` (`ssu2_runtime.rs:1371`) — the OS
   CSPRNG, never a counter or a clock, and never zero (the block
   constructor rejects zero independently).
3. **Bind.** `*peer.hash().as_bytes()` — the router hash of the
   **handshake-authenticated session peer** — through
   `peer_relay.issue_introducer_tag`, whose transport-side quota is the
   bound: `MAX_RELAY_TAGS_GLOBAL` 16, `MAX_RELAY_TAGS_PER_PEER` 4,
   `RELAY_TAG_LIFETIME_SECS` 120.
4. **Answer.** The `RelayTag` block is queued **on that same session**.
5. **Count.** Every outcome is counted by reason.

`SessionEvent::RelayTag(tag)` is surfaced on the runtime's **existing**
bounded relay handoff as the new `Ssu2RelayEvent::Tag(u32)`
(`ssu2_runtime.rs:834`), because the peer that asked for a tag is the peer
that has to learn it. No new channel and no new capacity.

### The runtime: the outbound relay carriage

`crates/i2pr-runtime/src/ssu2_runtime.rs`

Four peer-facing methods on `Ssu2RuntimeService` — `queue_relay_request`
(`:2107`), `queue_relay_tag_request` (`:2121`), `queue_relay_intro`
(`:2128`) and `queue_relay_response` (`:2139`) — all resolve through the
single site `queue_relay_block` (`:2159`). The resolution chain mirrors
`queue_controlled_peer_test`: the peer's live links → `state.active` → the
session → the session's bounded control queue → the scheduler wakeup. **Exactly
one** live link is required; a peer with more than one is refused rather than
guessed.

The typed error `Ssu2RelayQueueError` (`:839`) distinguishes *no session*,
*ambiguous session*, *session terminated* and *control queue full*, with
stable count-only reason labels (`:855`): `"no_session"`,
`"ambiguous_session"`, `"session_terminated"`, `"control_queue_full"`.
`note_relay_refusal` (`:2203`) counts every refusal by reason. The
counts-only accessor `Ssu2RelayCounters` (`:959`) and five aggregate
counters on `Ssu2Snapshot` (`:925-937`) surface the same numbers. **No
counter carries a peer hash, a key, a tag, an endpoint or a payload.**

### The daemon: the carriage seam and the live drain

`crates/i2pr-daemon/src/ssu2_relay_drain.rs`

The drain **dropped** the block the role owner produced: it counted
`awaiting_carriage` and discarded the block. That counter named a step which
now happens, so it is retired.

- `RelayCarriage` (`:74`) — a pull-shaped seam beside the existing
  `RelayEventSource` (`:57`), so the drain's behaviour can be exercised
  without binding a socket. It cannot enqueue into the runtime's relay
  channel: that channel's sender is private to the runtime and is fed only
  by the authenticated transport path.
- `RuntimeRelayCarriage` (`:95`) — the production carriage over the runtime
  that owns the sessions.
- `RelayDrainOutcome` (`:132`) — `carried` plus
  `carriage_refusals: BTreeMap<&'static str, u64>` replace `awaiting_carriage`.
- `drain_relay_events` (`:160`) takes the carriage and carries the produced
  block; a carriage that cannot be performed is counted by reason and the
  drain continues.

The live inbound loop constructs the carriage from the same runtime that
owns the sessions (`lib.rs:3993`) and passes it to the drain
(`lib.rs:4571`) on the 100 ms `relay_tick`. The log line reports `carried`
and `carriage_refused` counts instead of `awaiting_carriage`.

The role owner refuses the new `Ssu2RelayEvent::Tag(_)` as **not its role**
(`ssu2_introducer.rs:349`), exactly as it already refuses
`Ssu2RelayEvent::Intro`.

### The live three-party UDP mesh

`crates/i2pr-daemon/tests/ssu2_introducer_live_mesh.rs` (new)

Three live routers over real loopback UDP, with the harness re-implemented
from `crates/i2pr-runtime/tests/ssu2_local.rs` — with one necessary change:
the identity bundle is **retained**, because Alice and Charlie must sign.
The upstream helper drops it, which would silently lose the signing keys.

## Requirement-to-evidence matrix

| Requirement (plan §4 / §7) | Evidence | Verdict |
| --- | --- | --- |
| §4.1 default-off; the introducer off issues no tag and carries no block | `the_introducer_role_is_off_by_default_so_no_tag_is_issued` (mesh); `relay_tag_request_is_refused_when_the_introducer_is_disabled` (`ssu2_local.rs`) | pass |
| §4.2 a tag is bound only to an authenticated peer; no API issues a tag for a caller-named peer | `answer_relay_tag_request` binds `*peer.hash().as_bytes()` (`ssu2_runtime.rs:3039`); `live_relay_tag_request_answers_with_a_tag_bound_to_the_requester`; `the_issued_tag_resolves_to_its_own_target_and_to_nothing_else` | pass |
| §4.3 the tag value is unpredictable and nonzero | `random_u32_nonzero()`; `zero_relay_tag_is_rejected_before_the_queue` (transport) | pass |
| §4.4 tag issuance is bounded | the transport introducer's quota (16 global / 4 per peer / 120 s) is the only tag table; `relay_tag_refused_quota` counted | pass |
| §4.5 the signature path is unchanged | the transport's `verify_relay_request` / `verify_relay_response` are untouched; the requester's key still comes from a validated RouterInfo through `NetDbSignerResolver` | pass |
| §4.6 fail closed; every refusal counted by reason | `note_relay_refusal`; `relay_queue_error_reason_keys_are_stable`; `the_drain_counts_a_carriage_refusal_by_reason` | pass |
| §4.7 bounded | the transport's control queue stays at `DATA_MAX_SENT_PACKETS` (256); the drain's per-pass bound stays 16; no new channel | pass |
| §4.8 no await under a lock | the runtime's state mutex is held for the lookup and the queueing only; the daemon's bootstrap mutex only for one map lookup | pass |
| §4.9 no dependency, no advertisement, no promotion | `Cargo.toml` / `Cargo.lock` / `specs/support.toml` byte-identical; `router.version` 0.9.62; `caps` unchanged; `ssu2.advertise` refused | pass |
| §7 WP1 the transport's two controls | `relay_tag_request_traverses_session_to_introducer`; `relay_tag_traverses_session_with_the_exact_tag`; `relay_tag_controls_are_refused_on_a_terminated_session`; `relay_tag_controls_are_refused_at_the_control_queue_ceiling`; `zero_relay_tag_is_rejected_before_the_queue` | pass |
| §7 WP2 the runtime's live tag handling | `live_relay_tag_request_answers_with_a_tag_bound_to_the_requester`; `relay_tag_request_is_refused_when_the_introducer_is_disabled` | pass |
| §7 WP3 the runtime's carriage API | `queued_relay_intro_reaches_the_peer_intact`; `relay_carriage_without_a_live_session_is_refused`; `relay_carriage_after_the_session_terminates_is_refused`; `relay_carriage_is_refused_once_the_control_queue_is_full`; `relay_queue_error_reason_keys_are_stable` | pass |
| §7 WP4 the daemon's carriage seam | `the_drain_carries_the_produced_intro_to_the_tags_target`; `the_drain_counts_a_carriage_refusal_by_reason`; the existing discriminator and control rows, updated | pass |
| §7 WP5 the live three-party mesh | `live_three_party_relay_introduction_completes_over_real_udp` | pass |
| §10 the default-off row | `the_introducer_role_is_off_by_default_so_no_tag_is_issued` | pass |
| §13.1.8 no promotion | `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock` empty | pass |
| §13.2 no stop condition triggered | the tag is bound to the authenticated session peer; no daemon API reaches into the runtime's private relay channel; no unbounded queue; no signature check, quota, bound or default-off gate was weakened; the mesh needed no external environment | pass |

## Commands run

### The routine floor (on the closing tree `c128ab3f`)

**97 steps, 97 PASS, 0 FAIL.** Log: `/tmp/i2pr-floor-499-run1.log`;
completion marker `=== FLOOR COMPLETE 05:41:29 ===`.

The floor ran in full on the closing tree `c128ab3f`
(`docs(architecture): document the Plan 499 introducer live path`). The
slowest steps were `cargo-test-workspace` (1140 s), `plan491-netdb-suite`
(87 s), `plan498-bootstrap-resolver-row` (25 s) and `plan491-client-suite`
(25 s). Every checker script in the floor passed, including
`check-dependency-direction`, `check-runtime-boundaries`,
`check-console-boundaries`, `check-console-browser-security`,
`check-tooling-inventory`, `check-managed-app-process-boundary`,
`check-managed-app-package-boundary`, `check-managed-app-policy-boundary`,
`check-managed-app-secured-sandbox`, `check-service-tunnel-boundaries`,
`check-m12-floodfill-boundaries`, `check-outproxy-request-path` and
`cargo-deny` (advisories ok, bans ok, sources ok).

**No boundary script was weakened, relaxed or skipped.** The floor's
`cargo-test-workspace` step is the one that carries the new evidence: it
includes `ssu2_introducer_live_mesh` and `ssu2_introducer_handoff`.

### Focused verification (before the floor)

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo test --locked -p i2pr-transport-ssu2 --all-targets --no-fail-fast -- --test-threads=1` | PASS — **217 passed, 0 failed** across 6 binaries (145 unit, 19 `data_phase`, 22 `handshake`, 9 `path_validation`, 13 `peer_relay`, 9 `profile_boundary`) |
| `cargo test --locked -p i2pr-runtime --all-targets --no-fail-fast -- --test-threads=1` | PASS — **161 passed, 0 failed, 1 ignored** (the ignored one is the env-gated external i2pd lane) across 5 binaries (132 lib, 3 `ssu2_controlled_peer_test`, 19 `ssu2_local`, 7 `ssu2_peer_relay`) |
| `cargo test --locked -p i2pr-daemon --all-targets --no-fail-fast -- --test-threads=1` | PASS — **2305 passed, 0 failed** across **103** test binaries |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS — no warnings |
| `python3 scripts/check-tooling-inventory.py` | PASS — "every published figure matches the tree" |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests, OK |
| link check over the edited docs | PASS — no broken relative links, no out-of-range citations |
| `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock` | empty — no promotion |

The daemon's live-mesh lane was additionally run **four times** and the
runtime's new rows **five times** with no flake.

## Invariant review

1. **Default-off — preserved.** `ssu2.introducer_service` stays `false` by
   default, and the runtime's gate is derived from the same opt-in through
   `Ssu2RuntimeConfig::introducer_service` → `Ssu2PeerRelayConfig::introducer_enabled`.
   With the introducer off, the runtime observes the tag request and refuses
   it (counted as `relay_tag_refused_disabled`), no tag is issued, no answer
   is queued, the role owner refuses every event with `Disabled`, and the
   router behaves exactly as it did before this plan. **Proved** by
   `relay_tag_request_is_refused_when_the_introducer_is_disabled` and by
   `the_introducer_role_is_off_by_default_so_no_tag_is_issued`.
2. **A tag is bound only to an authenticated peer — preserved.** The bound
   hash is the router hash of the peer on whose **handshake-authenticated**
   session the request arrived. This plan adds **no** public API that lets a
   caller request a tag for an arbitrary peer. **Proved** by
   `live_relay_tag_request_answers_with_a_tag_bound_to_the_requester` (the
   tag resolves to the requester and to nobody else) and by
   `the_issued_tag_resolves_to_its_own_target_and_to_nothing_else`.
3. **The tag value is unpredictable and nonzero — preserved.** OS CSPRNG
   only; the block constructor independently rejects zero.
4. **Tag issuance is bounded — preserved.** The transport introducer's quota
   is the only tag table; this plan adds no table of its own. A refused
   issuance is counted by reason.
5. **The signature path is unchanged — preserved.** The transport's
   `verify_relay_request` and `verify_relay_response` are untouched, and the
   requester's key still comes from a validated RouterInfo through the
   daemon's `NetDbSignerResolver`. The carriage runs **after** the role
   owner admitted and verified the block, so it cannot make an unverifiable
   request verifiable.
6. **Fail closed — preserved.** Every refusal is a counted outcome with a
   stable reason label. A poisoned runtime state mutex fails closed as
   `SessionTerminated`. The drain never fails its caller's loop.
7. **Bounded — preserved.** The transport's control queue stays at 256; the
   drain's per-pass bound stays 16; the runtime's relay handoff keeps its
   existing `max_inbound_relay_queue` (default 16). No new channel, no new
   capacity, no unbounded queue.
8. **No await under a lock — preserved.** The runtime's state mutex is held
   for the peer→link lookup and the queueing only; the daemon's bootstrap
   mutex only for one map lookup. Neither crosses an await.
9. **No dependency, no advertisement, no promotion — preserved.**
   `specs/support.toml`, `Cargo.toml` and `Cargo.lock` are byte-identical.

## Failure review

- **The role owner refuses a block.** Counted by reason; no state created;
  the drain continues with the next event.
- **The tag request is refused** (introducer disabled, quota exhausted, the
  CSPRNG unavailable, the session terminated, the control queue full).
  Counted by reason; no tag is bound; no answer is queued.
- **The carriage is refused** (no session, ambiguous session, terminated
  session, control queue full). Counted by reason; the block is dropped
  **with a count**, never silently.
- **The drain never fails the loop.** Every refusal is a counted outcome.
- **Cancellation.** The live loop's cancellation arm shuts the owner down;
  the role owner holds no task of its own, and the transport's tag and relay
  controls are session state that dies with the session.
- **Restart.** The role owner is reconstructed from configuration at start. A
  tag lives in the transport introducer's table and expires with
  `RELAY_TAG_LIFETIME_SECS`; a restart loses the table, which is safe — a
  requester re-asks for a tag.
- **Contention.** The runtime's state mutex is taken for the lookup and the
  queueing, with no await held. The transport's control queue is bounded at
  256 and the drain's per-pass bound at 16, so a burst cannot grow an
  unbounded queue.

**Two refusal branches are guards rather than live paths**, and are recorded
as findings rather than claimed as coverage: `SessionTerminated` and
`AmbiguousSession` are both unreachable end to end (see *Findings*).

## Migration review

No migration. No stored format changes. No data-directory changes. No
configuration key is added or changed. The transport's **wire format is
unchanged** — blocks 15 and 16 already existed in the vocabulary and already
encode and decode; this plan adds the session's ability to emit them. The
runtime's public surface grows by four queueing methods, one typed error,
one counts-only accessor, one handoff variant and five snapshot counters. The
daemon's public surface grows by one seam and the removal of the
`awaiting_carriage` counter.

## Security review

- **The tag is not attacker-chosen.** The value comes from the OS CSPRNG and
  the owner is the authenticated session peer. A peer cannot ask for a tag
  bound to somebody else.
- **The tag is not a credential.** It names an introduction target; it does
  not authenticate the requester. The requester's signature is still
  verified against a key resolved from a **validated** RouterInfo, and the
  request must be fresh (within ±120 s) and carry a fresh nonce.
- **The carriage cannot bypass verification.** It runs strictly after the
  role owner admitted and verified the block, and it can only queue a block
  on an existing session with the peer the block is for. It cannot enqueue
  into the runtime's relay channel.
- **No secret is logged.** The drain's counters, the runtime's counters and
  the live loop's log line carry counts and stable reason labels only. No
  peer hash, key, tag, endpoint or payload is logged or counted. The
  `reason()` labels are fixed strings.
- **The refusal path leaks nothing.** A refusal reveals only that the
  carriage could not be performed, and by which of four stable reasons.
- **The default-off gate is checked first**, before any tag is minted, so
  the disabled router does no work at all for a tag request.
- **No new authority was created.** The daemon gained no API that issues a
  tag for a caller-named peer — the live path binds the session peer, and
  the tag table remains the transport's.

## Documentation and operations

- `docs/architecture/i2pr-transport-ssu2.md` — the two outbound controls,
  the `QueuedControl` variants, the transmit and rebuild arms, the two
  failure modes, the explicit "the wire format did not change" statement,
  and the `alice_hash` naming trap.
- `docs/architecture/i2pr-runtime.md` — the live tag request, the
  `Ssu2RelayEvent::Tag` handoff, the four carriage methods, the typed error
  and its reason labels, the counters, and the explicit statement that
  `SessionTerminated` and `AmbiguousSession` are guards rather than live
  paths.
- `docs/architecture/i2pr-daemon.md` — the carriage seam, the retirement of
  `awaiting_carriage`, the live three-party mesh and what it does **not**
  prove, and the RouterInfo-publication and anti-amplification limitations.
- `specs/CONFORMANCE.md` — a new Plan 499 section stating the claim and the
  non-claims, with forward-pointers from the Plan 498 section so the file
  is not self-contradictory.
- Published figures were recomputed for every row these edits touch. The
  plan-of-record's production-changes table was corrected to match what was
  actually built.

**Operations.** The role needs no new configuration. An operator enables it
with `ssu2.introducer_service = true` **and** `ssu2.enabled = true`; the
daemon's live loop then constructs the role owner and the carriage. The
counters to watch are the runtime's `relay_tag_requests`,
`relay_tags_issued`, `relay_tags_observed`, `relay_blocks_queued` and the
per-reason refusal counters, and the drain's `carried` and
`carriage_refusals`.

## Limitations

1. **The RouterInfo publication step is NOT closed.** The transport's
   documentation says the target "asks the introducer for a tag **and
   publishes it**". Publishing the tag in the target's RouterInfo is a
   NetDB/RouterInfo mechanism, not an SSU2 relay-block mechanism. This plan
   delivers the ask and the answer; it publishes **nothing**.
2. **There is no stock-peer lane.** A lane against the pinned i2pd 2.61.0
   needs the authorized two-host topology Plan 493 owns. Plan 490 Medium #3
   stays open.
3. **The mesh exercises the role owner against a TEST signer resolver**, not
   the production `NetDbSignerResolver`. The production resolver needs a
   `Bootstrap` holding validated RouterInfos — a separate concern, and it
   has its own unit rows (`crates/i2pr-daemon/src/bootstrap.rs`).
4. **`SessionTerminated` is not reachable end to end.** The runtime removes a
   terminated session from both `state.active` and `state.peer_links` in the
   same locked operation, so the observable refusal after a termination is
   `NoSession`. The `SessionError` → `Ssu2RelayQueueError` mapping is
   asserted by a focused in-crate unit row instead.
5. **`AmbiguousSession` is not reachable end to end.** The transport
   manager's duplicate policy resolves a same-peer duplicate as
   `RejectNew`, `ReplaceExisting` or `RetainExistingDrainNew`
   (`crates/i2pr-transport/src/manager.rs:55-73`); `AcceptNew` applies only
   to a *different* peer. So `peer_links[peer]` never holds two links. The
   ambiguous branch is asserted through a focused in-crate unit row of the
   resolution helper.
6. **The daemon's anti-amplification check never fires.** The role owner
   passes the constant `MAX_INTRODUCER_REQUEST_BYTES` (128) rather than the
   real request length, so the transport's `request_bytes * 3 < 128` test is
   always satisfied. **Pre-existing**, not added or fixed here.
7. **The requester's router hash is not in the request's signature
   preimage.** `relay_request_preimage(bob_hash, charlie_hash, ...)` binds
   **Bob's** and **Charlie's** hashes. The requester's identity comes from
   the session and only selects **which key verifies** the signature, so a
   request is not cryptographically bound to the requester's hash. The
   *Intro* block Bob produces does carry the requester's hash — that is
   Bob's output, not a field Alice signed. **Pre-existing** transport
   design.
8. **The mesh proves the introduction between three instances of this
   router.** It proves nothing about a stock peer, and nothing about the
   RouterInfo publication step.
9. **The mesh's `drained == 1` per pass** assumes Bob's relay handoff
   carries only the relay block. That held across four runs, but a busier
   handoff would move more than one event per pass.
10. **The mesh binds ephemeral loopback ports** and is intended to run with
    `--test-threads=1`.
11. **Plan 490's `next_relay_dispatched` seam** (`router_i2np.rs:2249`)
    remains superseded and retained; Plan 490's closure record names it as
    that plan's delivered surface.

## Findings by severity

**Medium**

- **M1 — the daemon's amplification budget is unenforced.** The role owner
  passes the constant 128 rather than the real request length, so the
  transport's `request_bytes * 3 < 128` check is always satisfied
  (`crates/i2pr-daemon/src/ssu2_introducer.rs:390`,
  `crates/i2pr-transport-ssu2/src/relay.rs:864-866`). The runtime's
  peer-relay path carries the same unenforced shape
  (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:772`). **Pre-existing** (Plan
  490); **not fixed** by this plan, and recorded rather than worked around.
  Fixing it means passing the real block length, which is a source change
  outside this plan's scope.

**Low**

- **L1 — `SessionTerminated` is a guard, not a live path.** See
  *Limitations* 4. The mapping is unit-tested; the end-to-end observable is
  `NoSession`.
- **L2 — `AmbiguousSession` is a guard, not a live path.** See
  *Limitations* 5. The branch is unit-tested.
- **L3 — the transport's comment above the tag-binding check is misleading.**
  `crates/i2pr-transport-ssu2/src/relay.rs:877` says "bound to this Alice",
  but in the live target-shaped path the transport's `alice_hash` parameter
  carries the **tag owner's** hash — the target's. The `issue_tag` doc
  comment (`relay.rs:789-793`) correctly says the name is historical. The
  comment is **not rewritten here**; the deep-dives and CONFORMANCE now state
  the binding correctly and name the trap. A one-line comment correction in
  the transport crate is a clean follow-up.
- **L4 — the runtime still exposes a public
  `issue_relay_tag(tag, target_hash, now_secs)`** that binds a tag to a
  **caller-supplied** hash (pre-existing, Plan 490,
  `crates/i2pr-runtime/src/ssu2_runtime.rs:2237`). This plan's live path
  binds the authenticated session peer and adds no such API, but the surface
  remains available to a caller that could name an arbitrary target.
- **L5 — the request is not cryptographically bound to the requester's
  hash.** See *Limitations* 7. Pre-existing transport design; not exploitable
  in the live path (the session supplies the requester's peer id and selects
  the key), but a reader of the wire format could reasonably assume
  otherwise.

**Informational**

- **I1 — pre-existing doc drift** in the daemon deep-dive: four
  `file:line` citations name files by bare filename
  (`lookup_engine.rs:523`, `floodfill_service.rs:739`,
  `server_store.rs:31`, `store_message.rs:84`) with no directory prefix. The
  files exist under `crates/i2pr-netdb/src/`; a link checker cannot resolve
  the bare form. Not introduced by this plan and left as-is.

## Roadmap disposition

`plans/subsystems/network-participation-roadmap.md`: the Plan 499 row moves
from `registered` to `closed` with the token
`passed-introducer-live-tag-request-and-carriage` and this closure record.

The line's own ordering (ADR 0041: substrate-first, no advertisement) is
unchanged. Plan 499 is a **discharge** of Plan 490's Medium #2, in the same
way Plan 497 discharged Plan 488's findings and Plan 498 discharged Plan
490's Medium #1 — it does not add a phase to the sequence and it advertises
nothing.

## Unblock audit

Audited `plans/registry.md`'s blocked work and the network-participation
roadmap's dependency graph for anything that lists the discharged finding as
a dependency.

| Plan | Was | Now | Why |
| --- | --- | --- | --- |
| 493 — two-family public participation qualification | blocked | **blocked** | Its acceptance names the **introducer** as one of the three participant legs, and that leg's source-side gap (Plan 490 Medium #2, no live socket carriage) is now closed. But Plan 493 remains blocked on the **authorized two-host topology** (Plan 431 stopped at exactly that boundary; Plans 472/476/480 own the runner, Plan 449 the portable LAN lane) and on **Plan 435's transport activation**. Neither dependency moved. |
| 494 — floodfill role and the `caps=f` advertisement | blocked | **blocked** | Blocked on Plan 493. Unchanged. |
| 433 — live NetDB/tunnel/client composition | blocked | **blocked** | Blocked on Plans 431/432. Unchanged. |
| 435 / 439 | blocked | **blocked** | External environments this host does not have. Unchanged. |
| 436 — transit owner | blocked | **blocked** | Its only remaining item is its stock-peer lane. Unchanged. |

**Nothing moved to `ready`.** Plan 499 removes a source-side gap from the
introducer leg, but the binding blocker on that leg is an external
environment, not a missing source-side capability.

No corrective plan is required by this closure. L3 and L4 are recorded Low
findings with named future evidence, not unfulfilled requirements of this
plan.
