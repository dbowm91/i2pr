# Plan 490 — SSU2 introducer (relay) role

**Status:** `passed-ssu2-introducer-relay-role`
**Date closed:** 2026-10-23
**Subsystem:** network participation (phase 3 of 7)
**Implementation plan:** [`plans/implementation/network-participation/490-ssu2-introducer-relay-role.md`](../../implementation/network-participation/490-ssu2-introducer-relay-role.md)
**Roadmap:** [`plans/subsystems/network-participation-roadmap.md`](../../subsystems/network-participation-roadmap.md)
**Implementation commits:** `8a170ae4` — *feat(network-participation): plan 490 SSU2 introducer (Bob) relay role*; `b4363112` — *test(network-participation): cover the plan 490 introducer role, publication form, and boundary guard*; `f88fcee7` — *docs(network-participation): document the plan 490 introducer role*; `35f8e647` — *fix(network-participation): refine the plan 490 refusal mapping and the rate-table eviction* (two refinements the focused runs prompted, committed separately so this record can report the tree honestly); `0f5ba473` — *fix(network-participation): resolve the plan 490 floor findings* and `431ece6e` — *fix(deps): update tokio for RUSTSEC-2026-0386* (the defects the routine floor caught, committed separately, for the same reason, so the closing tree's floor is green; neither is part of the plan's deliverable — see §7.5)
**Predecessor:** Plan 489 (`passed-floodfill-indexed-routing-table`), whose floodfill-indexed routing table gives the router a stable peer membership. Plan 490 does **not** depend on it: the relay role is a transport-level helper, and the plan's own ordering note says the two phases may execute in either order.
**Owner boundary:** `crates/i2pr-daemon` (the role owner and its configuration), `crates/i2pr-runtime` (the socket-level integration and the bounded handoff), `crates/i2pr-transport-ssu2` (the codecs, unchanged).

---

## 1. Scope

Plan 490 gave the router the **introducer (Bob) role** of the SSU2 relay
sub-protocol: the specification's NAT-traversal helper. A firewalled peer
(Charlie) asks this router for a relay tag and publishes it; a requester
(Alice) that can reach this router presents that tag to name the target;
the router forwards a Relay Intro to the target and answers the requester
with a Relay Response.

Delivered:

- the **daemon-side role owner** (`crates/i2pr-daemon/src/ssu2_introducer.rs`):
  it admits an inbound Relay Request, resolves the introduction target
  through the relay tag, verifies the requester's signature against a key
  resolved from a *validated* RouterInfo, produces the Relay Intro, and
  forwards the target's Relay Response to the requester;
- the **configuration gate**: `ssu2.introducer_service` is now *available*
  and still **default off**, and the disabled-path status is truthful
  ("available but not enabled", not the older and now false "unavailable in
  this milestone");
- the **inbound dispatch seam**: the runtime surfaces the decoded,
  session-authenticated relay blocks to the daemon over a **second bounded
  channel**, and `Ssu2DaemonHandle::next_relay_dispatched` delivers them to
  the role owner in the order *service first, transit second, unknown
  fail-closed*;
- the **transport's Bob-side response path**: `RelayIntroducer::on_response`
  correlates Charlie's Relay Response against the live request table by
  nonce, requires the sender to be the peer the request named, refuses a
  Bob-coded rejection, verifies Charlie's signature, and consumes the
  tracked request so a replay is refused;
- the **specification's introducer-only address form** in the publication
  builder: an address that publishes introducers omits `host`/`port` and
  carries the `4`/`6` family capability alongside the introducer groups,
  at most three introducers in the deterministic order;
- the **counts-only receipt** and the **private controlled-mesh lane** with
  its sanitized evidence artifact;
- a **crate-local static boundary guard** for the transport crate, with
  inverted probes.

Deliberately **not** delivered, by design:

- no advertisement of any kind. `router.version`, `caps`, the published
  RouterInfo, and the support inventory's advertisement flags are
  unchanged; `ssu2.advertise` stays refused;
- no public-network participation claim. The role is exercised against a
  private controlled mesh; the router's live RouterInfo still carries no
  introducer options;
- the **requester** side and the **target** side (both exist; the target
  side belongs to the peer's own address);
- **NAT hole punching for our own address** — that is the requester side;
- **IPv6 introducers** — the IPv4 family only in this phase, matching the
  `ssu2_direct_ipv4` posture;
- **tunnel participation**, the **version floors**, the **transit posture**,
  the **two-family qualification**, and the **floodfill role** — those are
  Plans 491, 492, 493 and 494.

---

## 2. Baseline at registration

At registration (`55fa39c4`, the Plan 489 closing tree) the daemon had no
introducer role at all:

- `ssu2.introducer_service` was **refused at configuration-parse time**
  with the reason "SSU2 introducer service is unavailable in this
  milestone". The key existed, but the role could not be switched on.
- `Ssu2DaemonService::new_with_bandwidth` and `ssu2_socket_config_from`
  refused the same key again at the service boundary, with the message
  "SSU2 controlled profile forbids advertise/introducer".
- The runtime's `Ssu2PeerRelayService` owned a `RelayIntroducer`, but only
  the **requester-shaped** request path existed: `on_relay_request` checked
  that the relay tag was bound to the *requesting* peer, and there was no
  Bob-side handler for Charlie's Relay Response at all — `RelayIntroducer`
  could admit a request but could never finish one.
- The transport's publication builder emitted a `DirectWithIntroducers`
  shape: host/port **and** introducer groups together. Neither reference
  router publishes that shape; the specification's rule is that an address
  publishing introducers omits host/port and carries the family capability
  instead.
- The runtime dropped the relay session events (`SessionEvent::RelayRequest`,
  `RelayIntro`, `RelayResponse`) on the floor of its `match`: they fell into
  the catch-all arm and vanished. There was no path from a decoded relay
  block to any owner.
- The daemon had no module, no configuration status, and no receipt for the
  role.

The specification's flow was confirmed against the primary source during
implementation (the SSU2 specification's Relay section): Alice sends a Relay
Request to Bob; Bob forwards a Relay Intro to Charlie; Charlie answers with
a Relay Response; Bob forwards it to Alice. Charlie's RouterInfo publishes
the router hash, relay tag and expiry of each introducer, so **the relay tag
identifies the target** — it is the only in-band link between the requester
and the target.

---

## 3. Implementation commits

| Commit | Subject | Content |
| --- | --- | --- |
| `8a170ae4` | feat(network-participation): plan 490 SSU2 introducer (Bob) relay role | `i2pr-transport-ssu2`: `RelayIntroducer::enabled()`, `on_response()`, `tag_owner()`, the introducer-only publication form, and the `IntroducerResponseOutcome` vocabulary. `i2pr-runtime`: the bounded relay handoff channel, `Ssu2RuntimeConfig::introducer_service`, the target-shaped request path, and `introducer_publication_material()`. `i2pr-daemon`: `src/ssu2_introducer.rs`, the configuration gate and its truthful status, and `Ssu2DaemonHandle::next_relay_dispatched`. |
| `b4363112` | test(network-participation): cover the plan 490 introducer role, publication form, and boundary guard | The crate-local static boundary guard `crates/i2pr-transport-ssu2/tests/profile_boundary.rs`, the private controlled-mesh lane `crates/i2pr-daemon/tests/ssu2_introducer_mesh.rs`, the sanitized evidence artifact, and the update of the closed Plan 160 publication assertion to the introducer-only form. |
| `f88fcee7` | docs(network-participation): document the plan 490 introducer role | The four architecture deep-dives, `specs/CONFORMANCE.md`, `specs/support.toml`, and the plan document's stale title, closure path and baseline. |
| `35f8e647` | fix(network-participation): refine the plan 490 refusal mapping and the rate-table eviction | The transport's refusal vocabulary is now mapped exhaustively rather than by catch-all, and the rate-limiter source table evicts the oldest window instead of clearing the table wholesale, so a flood that cycles many peers cannot reset every other peer's allowance. Two tests cover the plan's required rate-limiter behaviour. |
| `0f5ba473` | fix(network-participation): resolve the plan 490 floor findings | **Not part of the deliverable.** The six defects the routine floor caught on the closing tree: the runtime's stale publication expectation, the runtime handle's lint ceiling, the crate-local guard's collision with the transport boundary scan, the readiness contract's pre-plan control, and the stale architecture-deep-dive line counts. §7.5. |
| `431ece6e` | fix(deps): update tokio for RUSTSEC-2026-0386 | **Not part of the deliverable.** The advisory the floor's `cargo deny` step caught against a dependency the plan never touched: `tokio` 1.53.1 → 1.53.3, the smallest patched release in the same minor line. No manifest changed. §7.3. |

---

## 4. Requirement-to-evidence matrix

| # | Requirement | Evidence | Where |
| --- | --- | --- | --- |
| R1 | **Default-off and loopback-first.** The role is opt-in, disabled by default, and its ordinary profile binds loopback only. | `ssu2.introducer_service` defaults to `false`; `introducer_service = true` without `ssu2.enabled = true` is refused at parse time; the SSU2 bind posture is unchanged (loopback, or RFC1918 IPv4 under the explicit private-LAN profile). The role refuses every event with `Disabled` and creates no state while the key is off. | `config.rs` — `ssu2_introducer_service_is_available_and_default_off`, `ssu2_advertise_and_introducer_are_rejected`; `ssu2_introducer.rs` — `the_role_does_not_start_when_the_key_is_off` |
| R2 | **Admission before crypto, exactly as the runtime service already enforces.** | The role owner's per-source rate limit runs *before* the tag resolution and before any signature work; the transport's 3× response budget is enforced before its own crypto (`on_relay_request_with_signer`). A request that never passes the rate limit never reaches the signature check. | `ssu2_introducer.rs` — `the_per_source_rate_limit_refuses_a_flood`; `ssu2_peer_relay.rs` — `on_relay_request_with_signer`'s budget check |
| R3 | **Bounded everything; every drop path releases its entry.** | The rate-window table (≤ 64 sources), the tracked introductions (≤ 64), the transport's relay-tag table (`MAX_RELAY_TAGS_GLOBAL` = 16, `MAX_RELAY_TAGS_PER_PEER` = 4), the transport's request table (`MAX_RELAY_REQUESTS_GLOBAL` = 8, `MAX_RELAY_REQUESTS_PER_PEER` = 2), the runtime's relay handoff queue (≤ 16) and the transport's per-bucket introducer bound (≤ 3 published) all have fixed ceilings. A tracked introduction is released when it is answered, when the response is refused, or when its deadline passes. | `ssu2_introducer.rs` — `Ssu2IntroducerConfig`, `MAX_INTRODUCER_TRACKED_REQUESTS`, `MAX_INTRODUCER_RATE_SOURCES`; `relay.rs` — `poll_expired`, `shutdown` |
| R4 | **The signature preimages do not change.** | `relay_request_preimage` and `relay_response_preimage` are byte-for-byte unchanged; `RELAY_REQUEST_PROLOGUE` and `RELAY_RESPONSE_PROLOGUE` are untouched; no block format changed. The transport's closed preimage test (`request_preimage_matches_spec_order`) still passes. | `relay.rs` — `request_preimage_matches_spec_order`; the transport's existing 145 unit tests |
| R5 | **No advertisement beyond the tested subset.** | `router.version`, `caps` and the published RouterInfo are unchanged; `ssu2.advertise` is still refused at parse time and at the service boundary. The new support entry is `experimental` with `advertised = false`, and the inventory still has zero advertised surfaces. The publication builder never emits `pq` and never emits `caps=f`. | `specs/support.toml` — `ssu2.introducer-relay-role` (92 surfaces, 0 advertised); `publication.rs` — `the_introducer_only_form_never_publishes_a_floodfill_capability`, `publication_never_emits_pq` |
| R6 | **No secret in receipts.** | The receipt carries `requests_admitted`, `intros_produced`, `responses_forwarded`, `tags_issued` and refusals by typed reason — nothing else. The lane asserts that the rendered receipt and the rendered evidence file contain no peer hash, no address, and no tag value. | `ssu2_introducer.rs` — `Ssu2IntroducerReceipt`, `the_receipt_carries_no_peer_hash_no_tag_value_and_no_address`; `ssu2_introducer_mesh.rs` — the evidence assertions |
| R7 | **The requester side does not regress.** | `RelayRequester`, the peer-test path, and the requester-shaped `on_relay_request` are untouched in behaviour: the transport's own requester test (`requester_correlates_response_and_hole_punch_to_exact_request`) and the runtime's replay test (`relay_request_replay_does_not_reamplify`) still pass unchanged. The new target-shaped entry point is an *addition*. | `relay.rs` — `requester_correlates_response_and_hole_punch_to_exact_request`; `ssu2_peer_relay.rs` — `relay_request_replay_does_not_reamplify` |
| R8 | **The configuration gate makes the role available and keeps the disabled message truthful.** | The parse-time refusal is gone; the key parses, defaults to off, and reports `SSU2_INTRODUCER_DISABLED_REASON` — "the SSU2 introducer service is available but not enabled". The enabled status is `SSU2_INTRODUCER_ENABLED_REASON`. The in-crate test asserts the disabled reason contains "available but not enabled", so the old false wording cannot come back. | `config.rs` — `Ssu2Config::introducer_service_status`, `SSU2_INTRODUCER_DISABLED_REASON`, `SSU2_INTRODUCER_ENABLED_REASON`, `ssu2_introducer_service_is_available_and_default_off` |
| R9 | **The inbound Relay Request path reaches the new owner, in the fail-closed order.** | The runtime surfaces the decoded relay blocks over the second bounded channel; `Ssu2DaemonHandle::next_relay_dispatched` hands them to the role owner; the owner treats a Relay Request and a Relay Response as the *service* role and refuses a Relay Intro as the *transit* role. The runtime never surfaces a block it could not classify, so "unknown" is unreachable by construction rather than by a catch-all. | `ssu2_runtime.rs` — `Ssu2InboundRelay`, `Ssu2RelayEvent`, `next_relay_event`; `router_i2np.rs` — `next_relay_dispatched`; `ssu2_introducer.rs` — `the_transit_role_is_refused_by_the_service_owner` |
| R10 | **Every failure mode is a typed error and a counted drop.** | The refusal vocabulary is closed: `disabled`, `not_our_role`, `rate_limited`, `unknown_tag`, `expired_tag`, `unknown_requester`, `target_unknown`, `invalid_signature`, `stale_timestamp`, `duplicate_request`, `budget_exhausted`, `transport_refused`, `unknown_correlation`, `sender_mismatch`, `service_disabled`. Each refusal is counted under its stable key and creates no state. | `ssu2_introducer.rs` — `Ssu2IntroducerRefusal::receipt_key`; the refusal tests: `an_unknown_relay_tag_is_refused_without_state`, `a_request_from_an_unknown_requester_is_refused`, `a_request_with_a_bad_signature_is_refused`, `a_stale_request_is_refused`, `a_response_from_another_peer_is_refused`, `a_replayed_response_is_refused` |
| R11 | **The publication path honours the specification's rules.** | An address that publishes introducers omits `host`/`port`, carries the `4`/`6` family capability, publishes at most three introducers in the deterministic order, and round-trips through the strict production parser. The builder adds only the family flag implied by the reachability snapshot; it never invents a role capability. | `publication.rs` — `the_introducer_only_form_omits_host_and_port`, `the_introducer_only_form_carries_the_family_capability`, `the_introducer_only_form_round_trips_through_the_strict_parser`, `the_introducer_only_form_refuses_more_than_three_introducers`, `the_introducer_only_form_is_deterministic` |
| R12 | **A stale or unauthenticated introducer record is never published, and a restart rebuilds the set from live records only.** | These are the closed Plan 160 `IntroducerTable` behaviours, unchanged by this plan: only live, authenticated records are published, stale and failed records are dropped, and the table is rebuilt from live records on restart. | `introducer.rs` — `introducer.rs`'s own expiry and provenance tests; `peer_relay.rs` — `introducer_records_feed_publication_and_expire_cleanly` |
| R13 | **The private-mesh lane: the introduction completes and the receipt proves the role ran.** | The lane drives the daemon's *production* role owner through the same entry point the runtime handoff uses, with three peers and real signatures. It asserts that the tag names the target, that the Relay Intro is produced for it, that the target's Relay Response verifies and is forwarded to the requester, that the requester verifies the target's signature, and that the receipt proves the role ran. The evidence artifact holds counts and digests only. | `crates/i2pr-daemon/tests/ssu2_introducer_mesh.rs`; `tests/integration/network-participation/evidence/plan490/result.json`; the floor's `plan490-mesh-lane` and `plan490-evidence-deterministic` steps |
| R14 | **The offline controls, with an inverted probe per guard.** | The new crate-local static guard scans the transport's production sources and forbids `tokio::`, `std::net`, `std::fs`, the asynchronous-function construct, `async move`, `JoinHandle`, `std::process`, `std::thread`, and — the Plan 490 invariant — `std::time::Instant` and `std::time::SystemTime`. It allows the deliberate `core::net` address path. Its inverted probes: a synthetic socket use, a synthetic clock read, a synthetic task owner and a synthetic asynchronous function must all be reported, and a source that only *mentions* a forbidden construct in a comment or a `#[cfg(test)]` block must not. The asynchronous-function token is assembled at compile time so the guard's own source stays clean for the repository's whole-directory transport scan (§7.5, failure 3); the probe proves the assembly still detects it. | `crates/i2pr-transport-ssu2/tests/profile_boundary.rs` — `the_crate_sources_carry_no_forbidden_construct`, `the_guard_scans_the_plan_490_modules`, `the_guard_reports_a_socket_use`, `the_guard_reports_a_clock_read`, `the_guard_reports_a_violating_source`, `the_guard_reports_an_asynchronous_function`, `the_guard_allows_the_core_net_address_path`, `the_guard_ignores_comments_and_test_blocks`, `the_guard_reports_a_forbidden_construct_outside_a_test_block` |

---

## 5. Work packages and outcomes

| Work package | Outcome | State |
| --- | --- | --- |
| WP1 — the config gate and the refusal semantics | `ssu2.introducer_service` is available, default off, and refused only when the SSU2 service itself is off. The disabled-path status is truthful. | done |
| WP2 — the inbound Relay Request path | The runtime surfaces the relay blocks over a second bounded channel; the daemon's dispatch delivers them to the role owner in the *service first, transit second, unknown fail-closed* order. | done |
| WP3 — the role | The owner drives `RelayIntroducer`, produces the Relay Intro and forwards the Relay Response, and returns a typed outcome. Every failure mode is a typed refusal and a counted drop. | done |
| WP4 — the receipts | `Ssu2IntroducerReceipt` carries requests received, intros produced, responses forwarded, tags issued, and refusals by typed reason. No address, no tag value, no key, no payload. | done |
| WP5 — publication | The validated introducer set reaches the address builder and produces the specification's introducer-only form: no host/port, the family capability alongside the introducer groups, at most three introducers in the deterministic order. | done |
| WP6 — the lane | `crates/i2pr-daemon/tests/ssu2_introducer_mesh.rs`: a requester asks this router to introduce a firewalled peer, and the introduction completes. The receipt proves the role ran. | done |
| WP7 — the evidence and the docs | The sanitized artifact, the support-inventory entry, the four architecture deep-dives, `specs/CONFORMANCE.md`, and the registry/roadmap rows. | done |

### Design decisions

1. **The relay tag identifies the *target*, and the target's hash is what the
   requester's signature binds.** The specification is explicit: Charlie's
   RouterInfo publishes the router hash, relay tag and expiry of each
   introducer, and a requester presents that tag "to identify Charlie". The
   tag is therefore the only in-band link between the requester and the
   target, and the role owner must resolve the target **before** it can
   verify anything: the requester's signature is over a preimage that binds
   the target's hash.

   The transport's `RelayIntroducer` already took the tag owner and the
   preimage hash as separate parameters, but its *requester-shaped* entry
   point bound the tag to the requesting peer. Rather than change the
   transport's semantics, the runtime gained a second, **target-shaped**
   entry point (`on_relay_request_for_target`) that binds both to the tag
   owner. The transport's closed surface, its preimages and its block
   formats are unchanged; only the *caller* now supplies the peer the
   specification says the tag names.

2. **The requester's signing key comes from a validated RouterInfo, never
   from the wire.** The role owner resolves it through the
   `IntroducerSignerResolver` seam. A request whose requester is unknown is
   refused (`unknown_requester`) and counted. This is the same discipline
   the Plan 488/489 tables used: the *evidence* comes from a validated
   record, never from the message being validated.

3. **The rate limit runs before the tag resolution and before the
   signature work.** A flood must not reach the cryptographic path at all.
   The rate window is keyed by the peer hash — no address is retained, so a
   receipt can never leak one. When the source table reaches its ceiling it
   evicts the **oldest** window (the smallest `window_start_ms`, with the
   lower peer hash breaking a tie), so a flood that cycles many peers
   cannot reset every other peer's allowance. Clearing the table wholesale
   would have done exactly that, and was rejected.

4. **The publication rule is applied unconditionally, as the plan states it.**
   "An address that publishes introducers omits host/port and carries the
   `4`/`6` capability" appears in the plan's scope, in its invariant 5 and
   in its acceptance criterion 4. The builder therefore selects the
   introducer-only form whenever the published introducer set is non-empty.
   This **changed the builder's output** for that case: the old
   `DirectWithIntroducers` shape (host/port *and* introducer groups
   together) is no longer produced. The closed Plan 160 assertion was
   updated accordingly.

   The old shape matched neither reference router: i2pd and Java I2P publish
   host/port for a reachable router and the introducer groups plus the
   family capability for a firewalled one, never both. The new shape is
   therefore the specification's, and the plan mandated it.

   The builder adds **only** the family flag implied by the reachability
   snapshot (`4` for IPv4, `6` for IPv6). It never invents a role
   capability (`B`, `C`) and never drops a flag the caller asked for.

5. **The receipt carries counts only.** Not "hashes only" as the plan's
   invariant 6 allows, but the narrower "counts only": a peer hash, a
   tag value, an address, a key, or a payload would all be a disclosure,
   and none of them is needed to prove the role ran. The lane asserts this
   against the rendered receipt *and* against the rendered evidence file.

6. **The runtime hands the relay blocks over a second bounded channel
   rather than widening the I2NP one.** `Ssu2InboundI2np` is a closed
   shape that the daemon's I2NP dispatcher and several services already
   consume; widening it would have rippled through more than a dozen files
   for no benefit. A separate `mpsc` channel with its own ceiling
   (`max_inbound_relay_queue`, default 16) keeps the change contained and
   keeps the two flows independently bounded.

7. **The relay blocks are decoded and authenticated by the runtime, but the
   runtime never interprets the role.** The runtime's job is to hand the
   daemon a block it has already authenticated; the daemon's job is to
   decide. That split is why the role owner's tests need no socket and no
   runtime.

8. **The role owner is single-writer and holds no lock across an
   `.await`.** It owns plain maps; the runtime's tables are reached through
   the runtime service's own synchronization. The recorded lock order
   (bridge → coordinator) is untouched: this plan adds no bridge and no
   coordinator.

9. **The plan document carried three defects and they are fixed in place.**
   The title said "Plan 489", the closure path pointed at
   `489-status.md`, and the baseline pointed at a pre-Plan-489 commit. Plan
   numbers are load-bearing, so the plan document is now titled Plan 490,
   names `plans/closure/network-participation/490-status.md`, and records
   the Plan 489 closing tree as its baseline. The plan body was otherwise
   preserved.

---

## 6. Invariant review

The plan's seven invariants, each checked against the closing tree:

1. **Default-off and loopback-first.** Held. The key defaults to off; the
   role refuses everything with `Disabled` while off and creates no state;
   the SSU2 bind posture is unchanged (loopback, or RFC1918 IPv4 under the
   explicit private-LAN profile).
2. **Admission before crypto.** Held. The role's per-source rate limit
   precedes the tag resolution and the signature work; the transport's 3×
   response budget precedes its own crypto.
3. **Bounded everything.** Held. Every table has a fixed ceiling
   (rate sources 64, tracked introductions 64, relay tags 16/4, relay
   requests 8/2, handoff queue 16, published introducers 3), and every
   removal releases its accounting: an answered introduction is removed, a
   refused response leaves the introduction for its deadline, an expired
   introduction is dropped, and `shutdown` clears the transport's tables.
4. **The signature preimages do not change.** Held. Both preimages are
   byte-identical; the transport's own preimage test still passes.
5. **No advertisement beyond the tested subset.** Held. `router.version`,
   `caps` and the published RouterInfo are unchanged; `ssu2.advertise` is
   still refused; the new support entry is `experimental` with
   `advertised = false`; the inventory still has zero advertised surfaces.
6. **No secret in receipts.** Held, and narrower than the invariant: the
   receipt carries counts only.
7. **The requester side does not regress.** Held. `RelayRequester` and the
   peer-test path are untouched; the requester-shaped
   `on_relay_request` keeps its original tag binding, and the transport's
   and runtime's own requester tests pass unchanged.

The plan's **failure, cancellation, restart, and contention semantics** were
checked as well:

- a malformed or unauthenticated Relay Request is refused before any state
  change and never creates a relay-request entry;
- a Relay Request for an unknown tag is a typed refusal and a counted drop;
- the rate limiter's state is released on expiry (the window resets when it
  rolls over, and the source table evicts its **oldest** window at its
  ceiling, never clearing every other peer's allowance);
- a restart drops the outstanding relay-request table, and the published
  introducer set is rebuilt from live, authenticated records only;
- the role owner is single-writer, and no lock is held across an `.await`.

---

## 7. Verification

### 7.1 Focused runs

| Command | Result |
| --- | --- |
| `cargo test --locked -p i2pr-transport-ssu2 --all-targets -- --test-threads=1` | **212 passed**, 0 failed (145 lib + 19 data_phase + 22 handshake + 9 path_validation + 8 peer_relay + 9 profile_boundary) |
| `cargo test --locked -p i2pr-runtime --lib -- --test-threads=1` | **123 passed**, 0 failed |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` | **858 passed**, 0 failed, 3 ignored |
| `cargo test --locked -p i2pr-daemon --lib ssu2_introducer -- --test-threads=1` | **15 passed**, 0 failed (the role owner's tests and its configuration test) |
| `cargo test --locked -p i2pr-daemon --test ssu2_introducer_mesh -- --test-threads=1` | **1 passed**, 0 failed |
| `cargo test --locked -p i2pr-daemon --lib config:: -- --test-threads=1` | **99 passed**, 0 failed |
| `python3 scripts/check-global-plan-number-uniqueness.py` | passed |

### 7.2 The AGENTS.md routine floor

The floor was run **locally** on the closing tree (no CI run is claimed
for this closure). It is the AGENTS.md list verbatim, with four Plan 490
steps appended to the Plan 488/489 ones (the Plan 490 steps are
**additions**, not replacements):

- `plan490-mesh-lane` — the private controlled-mesh lane;
- `plan490-evidence-deterministic` — the evidence artifact is byte-identical
  across runs;
- `plan490-ssu2-transport-suite` — the whole transport crate's suites;
- `plan490-transport-boundary-guard` — the new static guard on its own.

**76 steps, 76 PASS, 0 FAIL.** The step-by-step table is in
§7.4 below; the raw log is `/tmp/i2pr-floor-490-final.log`.

**The closing tree was floored twice.** The first run caught six failures,
each reported in §7.5 with the fix that closed it. The fixes that are not
part of the plan's deliverable were committed separately, as Plan 489 did,
and the floor was then re-run **from the top** — not resumed — so every
step in the table below ran against the closing tree.

### 7.3 The advisories the floor caught

**One.** `cargo deny check advisories` flagged `RUSTSEC-2026-0386` against
`tokio` 1.53.1: the safe `AsyncFd` constructors accept any `T: AsRawFd`, so
safe code can register an arbitrary — or already closed — file descriptor.
The advisory was published after this host's advisory database was last
updated, so it surfaced only when `cargo deny` fetched the database during
the floor run; it is not a Plan 490 regression, because the plan added no
dependency and the baseline carried the same `tokio` 1.53.1.

The fix is the smallest patched release in the same minor line: `tokio`
1.53.3 (the advisory's patched set is `>=1.51.6, <1.52.0` or `>=1.53.3`).
The lockfile moved from 1.53.1 to 1.53.3 and **no manifest changed** — the
workspace pins `tokio = "1.48"` with caret semantics, so the patch release
was already permitted. The change is a lockfile-only dependency bump, and
the whole floor was re-run from the top afterwards rather than only the
advisory step.

Plan 489's closing tree needed a `ctutils` bump for `RUSTSEC-2026-0385`;
that fix (`c39da7cd`) was already in this plan's baseline.

### 7.4 The floor step table

| # | Step | Outcome | Seconds |
|---|---|---|---|
| 1 | `cargo-fmt` | PASS | 4 |
| 2 | `cargo-check` | PASS | 19 |
| 3 | `cargo-build-siblings` | PASS | 6 |
| 4 | `cargo-build-static-fixture` | PASS | 4 |
| 5 | `cargo-copy-static-fixture` | PASS | 1 |
| 6 | `cargo-rebuild-plain-fixture` | PASS | 0 |
| 7 | `cargo-test-workspace` | PASS | 1950 |
| 8 | `cargo-clippy` | PASS | 1 |
| 9 | `cargo-doc` | PASS | 12 |
| 10 | `cargo-test-doc` | PASS | 4 |
| 11 | `check-dependency-direction` | PASS | 0 |
| 12 | `check-global-plan-number-uniqueness` | PASS | 0 |
| 13 | `check-adr-number-uniqueness` | PASS | 0 |
| 14 | `check-portable-service-tunnel-api` | PASS | 0 |
| 15 | `check-portable-service-tunnel-consumer` | PASS | 6 |
| 16 | `planning-tests` | PASS | 7 |
| 17 | `check-runtime-boundaries` | PASS | 1 |
| 18 | `check-console-boundaries` | PASS | 1 |
| 19 | `check-console-browser-security` | PASS | 0 |
| 20 | `check-tooling-inventory` | PASS | 0 |
| 21 | `check-license-metadata` | PASS | 0 |
| 22 | `check-managed-app-private-client-seams` | PASS | 0 |
| 23 | `check-router-readiness-contract` | PASS | 0 |
| 24 | `check-router-readiness-contract-self` | PASS | 0 |
| 25 | `check-core-router-recovery-contract` | PASS | 0 |
| 26 | `check-core-router-recovery-contract-self` | PASS | 0 |
| 27 | `plan449-ssu2-preflight-self` | PASS | 0 |
| 28 | `check-managed-app-package-boundary` | PASS | 0 |
| 29 | `check-managed-app-package-boundary-self` | PASS | 1 |
| 30 | `check-managed-app-policy-boundary` | PASS | 0 |
| 31 | `check-managed-app-policy-boundary-self` | PASS | 0 |
| 32 | `check-managed-app-secured-sandbox` | PASS | 0 |
| 33 | `check-managed-app-secured-sandbox-self` | PASS | 0 |
| 34 | `check-service-tunnel-boundaries` | PASS | 0 |
| 35 | `check-managed-app-gateway-boundary` | PASS | 0 |
| 36 | `check-managed-app-manager-boundary` | PASS | 0 |
| 37 | `check-m11-per-epoch-composition` | PASS | 0 |
| 38 | `check-service-anonymity-boundaries` | PASS | 0 |
| 39 | `check-fixture-manifest` | PASS | 0 |
| 40 | `check-ntcp2-vectors` | PASS | 0 |
| 41 | `check-ssu2-vectors` | PASS | 0 |
| 42 | `check-i2cp-vectors` | PASS | 0 |
| 43 | `check-ntcp2-interoperability` | PASS | 0 |
| 44 | `check-constrained-host-lane-boundary` | PASS | 0 |
| 45 | `check-m11-transit-boundaries` | PASS | 2 |
| 46 | `check-m11-transit-qualification-evidence` | PASS | 1 |
| 47 | `check-sam-acceptance-evidence` | PASS | 0 |
| 48 | `check-ssu2-acceptance-evidence` | PASS | 0 |
| 49 | `check-i2cp-acceptance-evidence` | PASS | 0 |
| 50 | `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| 51 | `check-els2-type11-transcript-boundary` | PASS | 0 |
| 52 | `check-els2-live-lane-evidence` | PASS | 2 |
| 53 | `check-encrypted-service-consumer-caller` | PASS | 0 |
| 54 | `check-outproxy-request-path` | PASS | 0 |
| 55 | `check-outproxy-wire-lane-evidence` | PASS | 1 |
| 56 | `check-config-secret-hygiene` | PASS | 0 |
| 57 | `check-workflow-validity` | PASS | 0 |
| 58 | `check-floodfill-type5-serve` | PASS | 0 |
| 59 | `check-service-tunnel-acceptance-evidence` | PASS | 1 |
| 60 | `check-exploratory-tunnel-evidence` | PASS | 0 |
| 61 | `check-netdb-tunnel-evidence` | PASS | 0 |
| 62 | `check-destination-tunnel-evidence` | PASS | 0 |
| 63 | `check-streaming-tunnel-evidence` | PASS | 5 |
| 64 | `check-m6-mixed-router-acceptance-evidence` | PASS | 3 |
| 65 | `check-m12-floodfill-qualification-evidence-self` | PASS | 1 |
| 66 | `check-m12-floodfill-boundaries-self` | PASS | 0 |
| 67 | `ntcp2-harness-tests` | PASS | 1 |
| 68 | `plan488-mesh-lane` | PASS | 0 |
| 69 | `plan488-evidence-deterministic` | PASS | 0 |
| 70 | `plan489-mesh-lane` | PASS | 0 |
| 71 | `plan489-evidence-deterministic` | PASS | 0 |
| 72 | `plan490-mesh-lane` | PASS | 0 |
| 73 | `plan490-evidence-deterministic` | PASS | 0 |
| 74 | `plan490-ssu2-transport-suite` | PASS | 7 |
| 75 | `plan490-transport-boundary-guard` | PASS | 0 |
| 76 | `cargo-deny` | PASS | 1 |

**76 steps, 76 PASS, 0 FAIL.**

### 7.5 The failures the floor caught, and what each cost

**The floor was run three times, and the first run caught six failures.**
All six were real defects on the tree as the four plan commits left it.
Each is reported here with what it cost and how it was closed. Two of the
fixes are not part of the plan's deliverable and were committed separately,
exactly as Plan 489 did, so that this record can report a green floor
honestly.

**1. The runtime's publication test still expected the pre-plan form**
(`cargo-test-workspace`). The runtime's
`publication_integration_and_privacy_over_real_udp` asserted that a
firewalled address with one validated introducer publishes
`PublicationOutcome::Firewalled`. Under the plan's rule — an address that
publishes introducers omits host/port and takes the introducer-only form —
the outcome is `PublicationOutcome::IntroducerOnly`. The transport's own
closed Plan 160 assertion had been updated in `b4363112`, but the runtime's
mirror of it had not: the runtime's integration suite was never run as part
of the plan's focused runs, which used `--lib`. The fix updates the
assertion to the introducer-only form **and strengthens it** — it now
asserts that `host` and `port` are absent and that the introducer groups
are present, so it can no longer pass on a direct address. *Cost: one test
assertion; no production change.*

**2. The runtime handle's relay entry point exceeded the lint's argument
ceiling** (`cargo-clippy`). `Ssu2ServiceHandle::on_relay_request` has nine
parameters (the block, three peer hashes, the signer's key, the size budget
and the two clocks), and `clippy::too_many_arguments` fires at eight. The
peer-relay service it forwards to already carried the allowance for the same
shape; the handle wrapper did not. The fix adds the same allowance with its
reason, matching the eight existing sites in the transport, client and core
crates. *Cost: one attribute.*

**3. The new crate-local guard tripped the transport boundary scan**
(`check-runtime-boundaries`). `scripts/check-runtime-boundaries.sh` greps
the whole transport crate directory for the asynchronous-function construct.
The new `crates/i2pr-transport-ssu2/tests/profile_boundary.rs` names that
construct three times — in the module prose, in its own deny-list, and
inside the synthetic "benign" source it uses to prove that a comment naming
a forbidden construct is not reported. The scan is a whole-directory grep, so
the guard's own deny-list read as a violation. The fix assembles the token
at compile time (`concat!("async", " fn")`), which leaves the detection
identical and the guard's own source clean for the scan. An **inverted
probe** was added at the same time, so the compile-time assembly is itself
now proven to still detect the construct it forbids. *Cost: one constant and
one probe; no change to the script and no exclusion.*

**4. The readiness contract still described the pre-plan config gate**
(`check-router-readiness-contract`, and the same script's `--self-test`).
The checker asserted that `crates/i2pr-daemon/src/config.rs` contains
`if raw.introducer_service {` — the source shape of the pre-plan refusal,
where the key was refused unconditionally. Plan 490 deliberately changed
that gate: the role became *available* behind an explicit opt-in, so the
gate is now `if raw.introducer_service && !raw.enabled {`. **The checker's
control was the stale artifact, not the product.** It was updated to the
plan's contract rather than the product being bent back to the checker's
shape:

- "introducer publication must remain rejected" now points at the place
  where the invariant is actually enforced —
  `crates/i2pr-daemon/src/router_i2np.rs`, where the SSU2 service refuses
  to start when advertisement is permitted. The introducer role runs inside
  that service, so this refusal is what keeps the role unreachable in any
  advertising configuration;
- "introducer role must remain rejected unless the SSU2 service runs" pins
  the new opt-in gate;
- "introducer publication refusal test must remain" pins a **new**
  regression test, `socket_config_refuses_advertisement_even_with_the_introducer_role_on`,
  which drives the daemon's own socket builder with `advertise = true` and
  asserts the refusal names the advertisement it forbids.

The control count did not drop: three controls replace one, and the
checker's mutation self-test still proves every one of them has teeth. No
script was weakened and no exclusion was added. *Cost: one checker's
control set, one new regression test.*

**5. The dependency advisory** (`cargo-deny`) — see §7.3.

**6. The documentation's module line counts were stale** (not a floor
failure, but found while auditing the tree the floor ran against). The
architecture deep-dives count the lines of every module they describe, and
the counts for the nine modules Plan 490 touched were wrong — the plan's
own new module was counted at 890 instead of 1 011. The counts for those
nine modules were refreshed. The audit also found **37 further modules**
across the same three documents whose counts are stale; none of them is in
this plan's blast radius, so they are recorded as an open finding (§12)
rather than fixed here.

Two further failures were caught **before** the closing tree, by the
focused runs rather than by the floor itself:

1. **The requester-shaped request path regressed.** The first version of the
   runtime's refactor routed the requester-shaped `on_relay_request` through
   the new target-shaped helper, which bound the tag to the *target*. The
   transport's tag check therefore compared the tag owner against the wrong
   peer, and the runtime's own `relay_request_replay_does_not_reamplify`
   failed with `InvalidTag`. The fix restored the requester-shaped call and
   extracted the shared body into `on_relay_request_with_signer`. The
   requester path now behaves exactly as it did before the plan.
2. **The closed publication assertion failed.** The transport's
   `introducer_records_feed_publication_and_expire_cleanly` expected
   `PublicationOutcome::Direct` for a builder call with introducers. Under
   the plan's rule the outcome is `IntroducerOnly`. The assertion was
   updated to the new form and strengthened: it now checks that `host` and
   `port` are absent and that the introducer groups are present.

### 7.6 The false-pass audit

Every "passed" claim in this record was checked for the way it could have
passed without proving anything:

- **The role owner is not a hand-fed counter.** The lane drives the *real*
  production entry point (`Ssu2IntroducerService::on_relay_event`) with real
  Ed25519 signatures over the specification's preimages, and the transport
  verifies them. A receipt could not be produced by feeding the owner a
  fabricated outcome.
- **The signature verification is not stubbed.** The lane's request is
  signed by the requester's key and the response by the target's key, and
  the transport's `verify_relay_request` / `verify_relay_response` check
  them. A request signed by the wrong key is refused
  (`a_request_with_a_bad_signature_is_refused`), and a response from the
  wrong peer is refused (`a_response_from_another_peer_is_refused`).
- **The target resolution is not a constant.** The lane asserts that the
  Relay Intro names the target the *tag* identifies, and the role owner
  refuses a request carrying an unknown tag. A target that were hard-coded
  would still have to satisfy the tag check.
- **The evidence file cannot leak.** The lane asserts, before writing it,
  that the rendered evidence contains no peer hash, no address and no tag
  value; and the floor's `plan490-evidence-deterministic` step proves the
  artifact is byte-identical across runs, so it cannot have been
  hand-edited between runs.
- **The static guard cannot have gone blind.** The guard's own inverted
  probes fail if the scanner stops reporting a socket use, a clock read, a
  task owner or an asynchronous function. They are not the same assertion as
  the production scan. The asynchronous-function token is assembled at
  compile time so the guard's own source stays clean for the repository's
  whole-directory transport scan, and its probe proves that assembly did not
  blind the scanner.
- **The advertisement refusal is executable, not only declarative.** The
  daemon's socket builder refuses an advertising configuration even with
  the introducer role switched on, and the new regression test drives that
  path directly. The readiness contract pins both the refusal and the test
  by name, and its mutation self-test proves each control still has teeth.
- **The boundary guard covers the new modules.** `the_guard_scans_the_plan_490_modules`
  asserts that `relay.rs` and `publication.rs` are among the files the
  guard reads, so a rename cannot silently reduce coverage.
- **No `|| true`, no `continue-on-error`, no filename filtering, no fake
  peer environment, no broad exclusion, and no early return-success** was
  added to any script, and no production wire behaviour was changed to
  make a test pass. The floor script itself is a plain sequence of
  `run` steps that records each step's exit status.

---

## 8. Migration and compatibility

- **No wire change.** The Relay Request / Relay Intro / Relay Response and
  HolePunch formats are the closed transport codecs; both signature
  preimages are byte-identical.
- **No `router.version` change.** The constant stays pinned to the
  implemented I2NP surface.
- **No migration.** The role is new and off by default. A router that does
  not enable it behaves exactly as it does today.
- **One output-shape change, deliberate and documented:** the publication
  builder no longer produces the `DirectWithIntroducers` shape. An address
  that publishes introducers is now the introducer-only form. In ordinary
  operation the published introducer set is empty (the role is off and no
  validated introducer record exists), so **no ordinary address changes**;
  the change is visible only when introducers are actually published.
- **The daemon's transport dependency direction is unchanged.** The daemon
  still does not depend on `i2pr-transport-ssu2`; the relay vocabulary it
  needs is re-exported by `i2pr-runtime`, which is the pattern the
  existing Plan 184 re-exports established.
- **The reference pins are untouched**: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`), i2psam
  (`b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac`), i2plib
  (`6edf51cd5d21cc745aa7e23cb98c582144884fa8`). No external router or
  client was patched or vendored.
- **No boundary script was weakened.** The hard boundaries — the dependency
  direction, the runtime boundaries, the console boundaries and the console
  browser security — are byte-identical to the baseline. One *contract
  checker* changed: `scripts/check-router-readiness-contract.py`, whose
  introducer control still described the pre-plan config gate. Its controls
  were re-pointed at the plan's contract and **three controls replace one**
  (§7.5, failure 4); the mutation self-test still proves every control has
  teeth, and the script now pins an executable regression test for the
  advertisement refusal that it previously only pinned by source shape.

---

## 9. Security and contention

**Security**

- The role never trusts a key that arrived on the wire. The requester's and
  the target's signing keys are resolved from validated RouterInfos through
  the `IntroducerSignerResolver` seam, and the transport verifies the
  signature over the specification's preimage.
- A request whose signature does not verify, whose timestamp is outside the
  permitted skew, or whose relay tag is unknown is refused *before* any
  state is created. The refusal does not reveal whether the tag exists
  beyond the specification's own behaviour.
- The 3× anti-amplification budget is enforced before the transport's
  crypto, and the role owner supplies the protocol's upper bound for the
  request size, so the budget can never be under-counted for a short
  request.
- The per-source rate limit runs before the tag resolution and before any
  signature work, so a flood cannot reach the cryptographic path.
- A Relay Response is accepted only from the peer the request named, and
  only under that peer's signature; a Bob-coded rejection (which Charlie
  never sends) is refused as the wrong role.
- A replayed Relay Response is refused: the tracked introduction is
  consumed when it is answered, so the second response is an unknown
  correlation.
- No secret is logged, receipted or evidenced: no session key, no static or
  intro key, no tag value, no peer address, no payload. The receipt and the
  evidence artifact are asserted to carry none of them.
- The role is **not advertised**: the router publishes no introducer
  options, `caps=f` is never emitted, and `ssu2.advertise` stays refused.

**Contention**

- The role owner is single-writer and holds plain maps; it is not shared
  across tasks.
- The transport's tables are reached through the runtime service's own
  synchronization; the runtime takes and releases the lock inside one
  call, and no lock is held across an `.await` that can block on another
  lock holder.
- The recorded lock order (**bridge → coordinator**) is untouched: this
  plan adds no bridge and no coordinator.
- The relay handoff is a bounded `mpsc` channel with `try_send`; a full
  queue counts an `inbound_queue_drops` and drops the block rather than
  growing the queue.

---

## 10. Documentation and operational changes

- `docs/architecture/i2pr-transport-ssu2.md` — the introducer-only
  publication form and its rule; the Bob-side response path; the tag
  identifies the target; the new static boundary guard and the updated test
  inventory (212 tests).
- `docs/architecture/i2pr-runtime.md` — the relay handoff channel and its
  ceiling; the two request shapes; the daemon-facing role API.
- `docs/architecture/i2pr-daemon.md` — the new role-owner section; the
  module table entry; the `ssu2.introducer_service` default.
- `docs/architecture/overview.md` — the role in the crate summaries.
- `specs/CONFORMANCE.md` — the "SSU2 introducer (Bob) relay role (Plan
  490)" section, with what is claimed and what is not.
- `specs/support.toml` — `plan_490_status`, and the new
  `ssu2.introducer-relay-role` surface (`experimental`,
  `advertised = false`). The inventory is **92 surfaces, 0 advertised**.
- `plans/implementation/network-participation/490-ssu2-introducer-relay-role.md`
  — the stale title, closure path and baseline corrected; the body
  preserved.

**Operational notes**

- The new configuration key is `ssu2.introducer_service`, default `false`.
  It requires `ssu2.enabled = true`; the parse-time refusal says so
  explicitly.
- The role's status is visible through
  `Ssu2Config::introducer_service_status()`.

---

## 11. Known limitations

1. **The product's poll loop does not yet drive the relay channel.** The
   dispatch seam exists (`Ssu2DaemonHandle::next_relay_dispatched`) and the
   role owner is fully functional, but `ServiceProduct::poll_inbound` does
   not drain the runtime's relay handoff. A router that enables the role
   therefore admits the role's *configuration* and the runtime's
   *transport*, but the product's inbound pump does not yet deliver live
   relay blocks to the owner. This is the same shape as Plan 489's
   "the routing-table driver has no production consumer" finding.
2. **There is no live socket carriage of a Relay Request.** The runtime
   exposes no API to queue a relay block on an established session, so the
   role is proven against real blocks through the production entry point
   rather than over a UDP socket. The runtime's socket loop *does* deliver
   a relay block that arrives in-session to the bounded handoff — the
   missing piece is the *outbound* queueing API and a live three-party
   mesh.
3. **No stock-peer lane.** The plan's "Required tests" name a private mesh,
   and the delivered lane is that private mesh. A lane against the pinned
   i2pd `2.61.0` would need the authorized two-host topology that Plan 493
   owns; it cannot be fabricated here.
4. **The published introducer set is not exercised end-to-end.** The
   builder's introducer-only form is proven directly, and
   `Ssu2RuntimeService::introducer_publication_material` builds it from the
   validated set, but the router's *live* RouterInfo still publishes no
   introducer options: `ssu2.advertise` is refused, and the reachability
   state that feeds the publication path is still produced only by the
   controlled qualification profile.
5. **The transport's per-peer quota is keyed by the tag owner in the
   target-shaped path.** The transport's requester-quota check uses the
   parameter that also carries the tag binding, so in the target-shaped
   path the quota is per *target* rather than per *requester*. A single
   target therefore has a smaller effective admission budget than the
   specification's per-requester reading implies. The daemon's own
   per-source rate limit is unaffected.
6. **The transport's historical parameter names do not match the
   specification's roles.** `RelayIntroducer::issue_tag` binds the peer the
   tag *identifies* (the specification's target), and `on_request`'s first
   peer parameter is that same peer in the target-shaped path. The names
   are historical and are documented as such; no behaviour depends on them.

---

## 12. Findings by severity

### Medium — open

1. **The product's poll loop does not deliver live relay blocks to the role
   owner.** The role is complete and proven, but the daemon's ordinary
   inbound pump does not yet drain the runtime's relay handoff. Closing this
   means adding the relay drain to `ServiceProduct::poll_inbound` and
   resolving the requester's signing key from the product's validated store;
   it needs its own plan-of-record, because it touches the product
   composition.
2. **No live socket carriage of a Relay Request.** The runtime cannot queue
   a relay block on an established session, so no UDP-level proof of the
   role exists. Closing this needs the outbound queueing API and a
   three-party live mesh.
3. **No stock-peer (pinned i2pd 2.61.0) introducer lane.** The lane the
   plan names is the private controlled mesh, and the delivered lane is
   that private mesh. A stock-peer lane needs the Plan 493 topology.

### Low — open

4. **`scripts/check-runtime-boundaries.sh` has no section for the
   transport crate's clock rule.** The script scans the transport crates for
   the runtime-ownership rules, but the Plan 490 clock invariant (no
   `std::time::Instant` / `std::time::SystemTime` in the role or the
   publication builder) is enforced only by the crate-local guard
   `crates/i2pr-transport-ssu2/tests/profile_boundary.rs`. Adding a section
   to a published CI surface needs its own plan-of-record.
5. **The transport's per-peer quota is per target in the target-shaped
   path** (limitation 5 above). The fix would change the transport's
   requester-quota dimension, which is a closed surface.
6. **The transport's role parameter names are historical** and do not match
   the specification's Alice/Bob/Charlie roles (limitation 6 above).
7. **37 further architecture-deep-dive module line counts are stale.** The
   three deep-dives this plan touched count the lines of every module they
   describe. The counts for the nine modules in this plan's blast radius
   were wrong and are now refreshed; the same audit found 37 modules in the
   same three documents that are still stale — none of them touched by
   Plan 490. The drift is pre-existing and repo-wide, so it is recorded
   rather than fixed here: refreshing 37 unrelated modules would make this
   commit unfocused. It needs its own plan-of-record, or a checker that
   fails closed on a stale count.

### Informational

8. The plan document carried three defects (a stale "Plan 489" title, a
   `489-status.md` closure path, and a pre-Plan-489 baseline). They are
   fixed in place, because plan numbers are load-bearing.
9. The publication builder's output shape for a published introducer set
   changed deliberately (§5, decision 4). The closed Plan 160 assertion was
   updated and strengthened.
10. The floor's `CARGO_BUILD_JOBS=4` is a **host accommodation**, not a
    check change: this host has 15 GiB of RAM and 16 cores, and the
    daemon's ~60 test binaries do not fit when 16 rustc processes run at
    once. The variable is exported inside the floor script, so no step's
    command line changes.
11. The readiness contract's introducer control was **re-pointed at the
    plan's new contract** rather than the plan being bent back to the
    control's old source shape (§7.5, failure 4). Three controls replace
    one, the invariant is now asserted where it is actually enforced (the
    daemon's socket builder refuses an advertising configuration), and the
    checker's mutation self-test still proves every control has teeth. No
    script was weakened and no exclusion was added.

### No findings

- No secret was logged, receipted or evidenced.
- No unbounded channel, queue or table was introduced.
- No boundary script, reference pin or support-inventory advertisement flag
  was weakened. One *contract* checker changed — its controls were
  re-pointed at this plan's contract and their count increased
  (§7.5, failure 4) — but no hard boundary was relaxed and no exclusion was
  added.
- No capability, version or RouterInfo advertisement was added.

---

## 13. Roadmap disposition

Plan 490 is the network-participation roadmap's **phase 3**. Its row moves
from *ready* to *closed*, with the status token
`passed-ssu2-introducer-relay-role`.

The line now reads:

```text
488 (passed-peer-profile-store)
  -> 489 (passed-floodfill-indexed-routing-table)
  -> 490 (passed-ssu2-introducer-relay-role)
  -> 491 (ready) -> 492 (ready) -> 493 (blocked) -> 494 (blocked)
```

The next actionable phase is **Plan 491 — API-version floors and peer
eligibility**, which needs no external topology and no public bind.

---

## 14. Unblock audit

The audit is required at every closure: for each registered plan that lists
the just-closed milestone as a hard or interface dependency, check whether
**all** other hard dependencies are closed and the interface dependencies
have stable written contracts.

| Plan | Depends on 490? | Other hard deps | Interface deps | Disposition |
| --- | --- | --- | --- | --- |
| 491 — API-version floors and peer eligibility | No | the pinned `router.version` declaration (285) and the caps/tier boundary (306), plus Plan 488's per-peer evidence — all closed | the version comparator and the eligibility policy are Plan 491's own deliverable | **stays ready**; it becomes the next actionable phase |
| 492 — Transit participation posture | No | the transit ingress owner and the runtime-neutral transit state machine (254), the passed M11 one-family qualification (268), Plan 454's governor, and Plan 488's selection policy — all closed | Plan 488's selection policy is closed and stable | **stays ready** |
| 493 — Two-family public participation qualification | No | the authorized two-host topology (Plan 431, stopped) and Plan 435's transport activation — **not closed** | the three participant legs | **stays blocked**; Plan 490's role is one of the three legs the qualification exercises, and its availability does not unblock the topology |
| 494 — Floodfill role and `caps=f` | No | Plan 493 — **not closed** | Plan 491's DSM floor | **stays blocked**; `caps=f` stays refused |

No plan listed Plan 490 as a hard or interface dependency, so nothing is
unblocked by this closure. Plan 490's own dependencies were the closed SSU2
transport relay roles (Plan 160), the SSU2 interop and lane isolation
(Plans 161/162), and the runtime's peer/relay coordination service — all
closed before this plan started.

Plan 489 was **not** a hard dependency: the relay role is a transport-level
helper, and the plan's own ordering note says the two phases may execute in
either order. Plan 490 does not consume the routing table.

**No corrective pass is required.** The three open findings above are
recorded, not absorbed; each names the surface it touches and needs its own
plan-of-record.

---

## 15. Open items

1. Wire the relay handoff into `ServiceProduct::poll_inbound` and resolve the
   requester's signing key from the product's validated store (finding M1).
2. Add the runtime API that queues a relay block on an established session,
   and a live three-party mesh that proves the role over UDP (finding M2).
3. Own a stock-peer introducer lane against the pinned i2pd `2.61.0` once
   the authorized two-host topology exists (finding M3).
4. Add a `i2pr-transport-ssu2` section to `scripts/check-runtime-boundaries.sh`
   for the clock rule, or record a decision that the crate-local guard is
   the permanent home for it (finding L4).
5. Decide whether the transport's per-requester quota dimension should be
   restored in the target-shaped path (finding L5).
6. Decide whether the transport's historical role parameter names should be
   renamed to the specification's Alice/Bob/Charlie vocabulary (finding L6).
7. Refresh the 37 stale architecture-deep-dive module line counts outside this
   plan's blast radius, or add a checker that fails closed on a stale count
   (finding L7).
