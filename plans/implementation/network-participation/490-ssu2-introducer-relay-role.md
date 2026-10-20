# Plan 489 — SSU2 introducer (relay) role: running the introducer side of the relay sub-protocol

**Status:** registered, ready to execute (phase 3 of
[network participation](../../subsystems/network-participation-roadmap.md))
**Owner:** `i2pr-daemon` (the role owner and its configuration),
`i2pr-runtime` (the socket-level integration of the existing state
machines), `i2pr-transport-ssu2` (the codecs, unchanged)
**Baseline:** `work/plans-477-480-router-recovery@c43f3404`
**Authoritative predecessor:** `plans/closure/ssu2/161-status.md` (the SSU2
acceptance authority) and `plans/closure/core-router-recovery/487-status.md`
**Research input:** the SSU2 specification's Relay section and Published
Router Info section (block types 7–9, message type 11 HolePunch; the
introducer address options), the pinned i2pd 2.61.0 and Java I2P 2.13.0
introducer behaviour
**Predecessor lineage:** 159/160 (SSU2 path validation, publication,
peer-test/relay) → 161/162 (interop and lane isolation) → 488 → 489 → **490**

## Objective

Run the **introducer (Bob) role** of the SSU2 relay sub-protocol in the
production daemon, and publish the validated introducer set in the router's
own RouterInfo address — so that a **firewalled peer can be reached through
this router**.

One bounded outcome: **the daemon answers an inbound SSU2 Relay Request as
the introducer, under an explicit opt-in, with bounded admission, and the
router's published address carries the introducer options the specification
requires.** The role is internal and non-advertised beyond the address
options themselves.

### What the role is, precisely (and what it is not)

The I2P specification's "introducer" is a **transport-level NAT-traversal
helper**, not a NetDb entry point:

- a firewalled peer (Charlie) publishes an address whose introducer options
  name a publicly reachable router (Bob) plus a relay tag and an expiration;
- Alice, who wants to reach Charlie, sends a **Relay Request** to Bob;
- Bob forwards a **Relay Intro** to Charlie and answers Alice with a
  **Relay Response**; the hole punch follows.

A router does **not** enter the network through an introducer: it enters
through a reseed (already implemented, Plan 432) and then through the DHT
spine (Plan 488). This plan is about the router **serving** the network by
relaying introductions on behalf of firewalled peers.

## Why ready

- **Hard dependencies are closed.** The SSU2 transport's relay roles and
  HolePunch codec are implemented and closed (Plan 160), and the SSU2
  session owner passed independent interop (Plan 161) with lane isolation
  (Plan 162). The runtime's peer/relay coordination service is closed
  (Plan 160's runtime side).
- **Interface dependencies are stable.** `i2pr-transport-ssu2::relay`
  (`RequesterState`, `IntroducerState`, `TargetState`, the signature
  preimages) and `i2pr-transport-ssu2::introducer` (the validated
  introducer table and its publication inputs) are closed surfaces.
- **Plan 489 is not a hard dependency** for the *protocol* work, but the
  phase order puts 488 and 489 first because the introducer set is only
  meaningful once the router knows peers. The plan may execute in either order; the
  registry's phase order expresses the *credible* order, not a hard
  inter-phase dependency.

## Current implementation evidence

Present today:

- `crates/i2pr-transport-ssu2/src/relay.rs` — the three v2 roles
  (requester/Alice, introducer/Bob, target/Charlie) as runtime-neutral
  state machines, with the specification's signature preimages verbatim
  (`RelayRequestData`, `RelayAgreementOK`, …) and the out-of-session
  HolePunch codec.
- `crates/i2pr-transport-ssu2/src/introducer.rs` — the validated introducer
  record set: at most `MAX_PUBLISHED_INTRODUCERS` (3), only live and
  authenticated records, deterministic eviction (oldest-expiring first,
  stable order by peer hash), stale/failed records never published, and
  "direct vs introducer-only publication follows the reachability state, not
  a caller boolean".
- `crates/i2pr-runtime/src/ssu2_peer_relay.rs` — the runtime service that
  owns `PeerTestTable`, `RelayRequester`, `RelayIntroducer`, `RelayTarget`,
  `IntroducerTable` and `ReachabilityTracker`, with per-source rate
  limiters and response-size budgets, and the explicit statement that
  "introducer public service remains disabled unless configuration
  explicitly enables it (runtime/daemon concern)".
- `crates/i2pr-transport-ssu2/src/address.rs` — the SSU2 address options,
  including the introducer options and the `4`/`6`/`B`/`C` capability
  parsing.

Missing today:

- **the daemon never runs the role.** `ssu2.introducer_service` is refused
  at configuration-parse time with the reason "SSU2 introducer service is
  unavailable in this milestone"
  (`crates/i2pr-daemon/src/config.rs`).
- **no production socket integration for the inbound Relay Request path**
  in the daemon: the runtime module states that production socket
  integration for out-of-session messages belongs to Plan 161, and the
  in-session relay/peer-test carriage over a live socket is the part the
  daemon does not own.
- **no published introducer options** in a live RouterInfo, because the
  role is not running and the reachability state that feeds the publication
  builder is not produced by the daemon.

## Invariants that must not regress

1. **Default-off and loopback-first.** The role is opt-in, disabled by
   default, and its ordinary profile binds loopback only.
2. **Admission before crypto, exactly as the runtime service already
   enforces.** A datagram is admitted (source address, length, rate limit)
   before any parse or signature work; a decode never mutates the NetDb or
   the RouterInfo.
3. **Bounded everything.** The per-source rate limiters, the outstanding
   relay-request table, the introducer table (≤ 3 published) and the
   response-size budgets stay bounded; every drop path releases its entry.
4. **The signature preimages do not change.** The transport's preimages are
   normative; this plan must not re-derive or "improve" them.
5. **No advertisement beyond the tested subset.** The address options the
   router publishes are exactly the ones the specification requires for an
   address with introducers — the introducer hash, the relay tag, the
   expiration — and the `4`/`6` capability the specification demands
   alongside them; no `caps=f`, no extra option.
6. **No secret in receipts.** A relay receipt carries counts, roles, and
   hashes only — never a session key, a tag value, or a peer address.
7. **The requester side does not regress.** The runtime's `RelayRequester`
   and the peer-test path keep working exactly as they do now.

## Scope

**In:**

- the daemon-side owner that runs the introducer role: inbound Relay
  Request → the runtime's `RelayIntroducer` → Relay Intro to the target →
  Relay Response to the requester;
- the configuration gate that makes the role available (default off), and
  its refusal message replaced by the real reason when disabled;
- the publication path: the validated introducer set feeding the RouterInfo
  address options under the reachability state;
- the receipts that prove the role ran (counts, roles, outcomes);
- the private-mesh lane and the offline controls.

**Explicitly out:**

- the **requester** side and the **target** side (both exist; the target
  side belongs to the peer's own address);
- **NAT hole punching for our own address** — the firewall-traversal
  *outbound* path is the requester side and stays as it is;
- **tunnel participation** (Plan 491) — the I2NP transit role is a
  different mechanism from the SSU2 relay role and must not be conflated;
- **floodfill** (Plan 493);
- **IPv6** introducers (`ih[1]`/`ih[2]`) — the IPv4 family only in this
  phase, matching the `ssu2_direct_ipv4` posture;
- any change to the transport codecs, the reference pins, or the boundary
  scripts.

## Required production changes

- **`crates/i2pr-daemon/src/ssu2_introducer.rs`** (new): the role owner —
  it receives a decoded, session-authenticated Relay Request, applies the
  runtime's admission and rate limits, drives `RelayIntroducer`, and emits
  the typed outcome and the receipt.
- **`crates/i2pr-daemon/src/config.rs`**: `ssu2.introducer_service` becomes
  available. It stays **default off**; when off, the refusal message must
  state that the role is available but not enabled (the current message
  says the role is unavailable, which will be false).
- **`crates/i2pr-daemon/src/router_i2np.rs` / the SSU2 inbound dispatch**:
  the Relay Request path reaches the new owner. The dispatch stays
  **creator/service first, transit second, unknown fail-closed** — the order
  the transit owner already documents.
- **`crates/i2pr-daemon/src/publication.rs` (or the existing SSU2
  publication owner)**: the validated introducer set is supplied to the
  address builder under the reachability state, honoring the
  specification's rules: no host/port in an address that publishes
  introducers, the `4`/`6` capability alongside them, and at most three
  introducers.

## Ordered work packages

1. **WP1 — the config gate and the refusal semantics.** Make the key
   available, keep it default-off, and make the disabled-path message
   truthful. A negative test asserts the role does not start when the key
   is off.
2. **WP2 — the inbound Relay Request path.** The daemon's SSU2 inbound
   dispatch delivers a decoded Relay Request to the new owner; the
   fail-closed ordering is asserted.
3. **WP3 — the role.** The owner drives `RelayIntroducer`, produces the
   Relay Intro and the Relay Response, and returns a typed outcome. Every
   failure mode (unknown tag, expired tag, target unknown, budget
   exhausted) is a typed error and a counted drop.
4. **WP4 — the receipts.** Source-originated receipts: requests received,
   intros forwarded, responses sent, refusals by reason. No addresses, no
   tags, no keys.
5. **WP5 — publication.** The validated introducer set reaches the address
   builder; the published options are exactly the specification's; the
   direct-vs-indirect choice follows the reachability state, not a
   caller-provided boolean.
6. **WP6 — the lane.** A private mesh lane where a requester asks this
   router to introduce a firewalled peer, and the introduction completes.
   The lane asserts a source-originated receipt, not an aggregate counter.
7. **WP7 — the evidence and the docs.** The lane's evidence file (counts
   and digests), the support-inventory entry, the architecture docs, and
   the roadmap/registry rows.

## Failure, cancellation, restart, and contention semantics

- **A malformed or unauthenticated Relay Request** is dropped before any
  state change and counted; it never creates a relay-request entry.
- **A Relay Request for an unknown or expired tag** is refused with a typed
  reason; the refusal is a *response*, not a silent drop, and it does not
  reveal whether the tag exists beyond the specification's behaviour.
- **Rate limiting** is per source address and is applied before crypto; a
  source over its limit is dropped and counted, and the limiter's state is
  released on expiry.
- **A restart** drops the outstanding relay-request table and the
  introducer table's *runtime* state; the published introducer set is
  rebuilt from live, authenticated records only — a record that cannot be
  re-authenticated is not published.
- **Contention**: the role owner is single-writer; the transport's tables
  are accessed through the runtime service's own synchronization, and no
  lock is held across an `.await` that can block on another lock holder.
  The recorded lock order (**bridge → coordinator**) still holds.

## Compatibility and migration

- **No wire change.** The Relay Request/Intro/Response and HolePunch
  formats are the closed transport codecs; the signature preimages are
  normative and unchanged.
- **No `router.version` change.** The constant stays pinned to the
  implemented I2NP surface.
- **No migration.** The role is new and off by default; a router that does
  not enable it behaves exactly as it does today, and its published address
  keeps its direct form.
- **Publication is additive**: an address that publishes introducers omits
  host/port and carries the `4`/`6` capability, which is the
  specification's requirement, not a new i2pr rule.

## Required tests

**Offline controls** (in `crates/i2pr-daemon`, `crates/i2pr-runtime`, and
the transport crate's existing suites):

- the role does not start when the configuration key is off, and the
  disabled-path message is truthful;
- a Relay Request with an unknown tag, an expired tag, and a malformed
  signature are each refused with a typed reason and counted, and none
  creates state;
- the per-source rate limiter drops over-limit traffic and releases its
  state on expiry;
- the published address contains exactly the specification's introducer
  options, carries the `4` capability, omits host/port, and lists at most
  three introducers in the deterministic order;
- a stale or unauthenticated introducer record is never published;
- a restart rebuilds the published set from live records only;
- an inverted probe for each invariant (each must fail when the invariant
  is broken).

**Private-mesh lane**: a requester asks this router to introduce a
firewalled peer; the Relay Intro and Relay Response complete, and the
receipt proves the role ran.

## Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-ssu2-acceptance-evidence.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

## Documentation updates

- `docs/architecture/i2pr-daemon.md` — the new role owner and the
  configuration key;
- `docs/architecture/i2pr-runtime.md` — the production socket integration
  the role adds;
- `docs/architecture/i2pr-transport-ssu2.md` — the role's runtime
  ownership (the transport stays runtime-neutral);
- `specs/support.toml` — the role's status entry, once the lane has passed;
- `specs/CONFORMANCE.md` — the SSU2 relay evidence row;
- `plans/subsystems/network-participation-roadmap.md` and
  `plans/registry.md` — the phase's row.

## Acceptance criteria

1. With the key off, the role does not start and the daemon's message says
   so truthfully.
2. With the key on (loopback profile), an inbound Relay Request is answered
   as the introducer, and the completion is visible in a source-originated
   receipt.
3. Every refusal path is typed, counted, and leaves no state behind.
4. The published address carries the specification's introducer options,
   the `4` capability, no host/port, and at most three introducers in the
   deterministic order.
5. The offline controls pass and each invariant has an inverted probe.
6. The private-mesh lane passes; the evidence file holds counts and digests
   only.
7. The routine floor is green on the closing tree, with the step table in
   the closure record.
8. The closure record states plainly that this is an **opt-in, non-advertised
   role** — not a public-network claim.

## Stop conditions

Stop and record a `stopped-*` closure when:

- the role cannot be made fail-closed and bounded in the daemon;
- answering a Relay Request would require weakening the transport's
  admission-before-crypto order or the fail-closed dispatch order;
- the published address cannot satisfy the specification's "no host/port
  with introducers" and `4`/`6` capability rules together;
- the lane would need a public bind, an external topology, or a public
  network;
- any step would require editing the transport codecs, the signature
  preimages, the reference pins, or a boundary script to go green;
- the attempt budget is spent without a material change.

## Closure evidence required

- `plans/closure/network-participation/489-status.md` with the full closure
  content the repository requires (commits, requirement-to-evidence matrix,
  command outcomes, invariant/failure/migration/security reviews,
  documentation updates, limitations, findings by severity, roadmap
  disposition, unblock audit);
- the lane's evidence file;
- the registry and roadmap rows advanced to closed;
- the unblock audit recorded, with any newly ready plan moved to `ready` in
  the same commit.

## Handoff notes

- **Do not confuse the three relay roles.** The requester (Alice) and the
  target (Charlie) exist; this plan owns the **introducer (Bob)** side.
- **Do not touch the preimages.** The transport's signature preimages are
  normative; a mismatch here is a protocol defect, not a style question.
- **Do not enable this by default**, and do not treat "the role runs" as
  "the router is on the public network".
- **Reference comparison.** i2pd 2.61.0 and Java I2P 2.13.0 both run the
  introducer role; their admission and rate-limit behaviour is the oracle
  for the private-mesh lane, and Emissary's Rust implementation is worth
  studying for the service's ownership shape (its source is out of bounds
  on this branch under ADR 0028 §7).
