# Plan 532 — the router must answer a relay introduction when it is the target

**Status:** registered.
**Subsystem:** network-participation.
**Plan of record.** This document is the plan of record for the work; the
closure record will live at `plans/closure/network-participation/532-status.md`.

## 1. Objective

The SSU2 relay protocol has three roles. The **introducer** answers a relay
request and forwards an introduction. The **requester** asks an introducer to
reach a peer it cannot reach directly. The **target** is the peer that is
reached: it receives the introduction, verifies it, and **answers** it.

Plans 490, 499 and 524 delivered the introducer, Plans 525 and 526 the
published introducer set and its advertisement, and Plan 531 the requester.
The **target** is the last role with no producer.

Plan 532 delivers it: when this router is the target of a relay request, it
must receive the introducer's Relay Intro, verify it through the transport's
own target state machine, and **answer** it — send the Relay Response accept
back to the introducer, and punch the hole to the requester out of session.

## 2. Why ready

- The transport's target role is **complete**: `RelayTarget` verifies the
  introducer context, enforces the global quota, suppresses a replayed
  introduction, expires pending intros deterministically, and clears on
  shutdown.
- The runtime already holds the transport's target table and exposes the
  seam that drives it.
- The runtime's `on_target_relay_intro` seam was added by Plan 531 and has
  **zero production callers** — it is ready to be wired, not to be built.
- The transport's hole-punch codec and the runtime's bounded staging queue
  both exist, and the queue is already used for the relay blocks.
- The daemon already owns a router identity bundle and can sign the accept
  with its own key.
- The authoritative validated-RouterInfo store already holds the requester's
  record when the requester is a peer this router knows.

## 3. Current implementation evidence

### 3.1 The transport's target role exists and is complete

`crates/i2pr-transport-ssu2/src/relay.rs:1025` is `RelayTarget`, with a
`seen` set for replay suppression and a bounded `pending` map of live
introductions. Its `on_intro` (`:1068`) is the whole verification:

```rust
    pub fn on_intro(
        &mut self,
        block: &RelayIntroBlock,
        expected_bob_hash: &[u8; 32],
        expected_charlie_hash: &[u8; 32],
        alice_signer: &SigningPublicKey,
        now_secs: u64,
        now_ms: u64,
    ) -> Result<bool, RelayError> {
```

It checks freshness, reconstructs the forwarded Relay Request view, verifies
Alice's signature against the specification's preimage with the **expected
introducer** and the **expected target**, refuses a replayed
`(nonce, alice_hash)` pair, enforces `MAX_RELAY_REQUESTS_GLOBAL`, and inserts
the pending introduction. It returns `true` when the caller should emit the
HolePunch, and `false` for an idempotent replay. `consume` (`:1127`) removes
the pending introduction after the emission and `poll_expired` (`:1133`)
expires it on the transport's own deadline.

### 3.2 The runtime holds the target table and exposes the seam

`crates/i2pr-runtime/src/ssu2_peer_relay.rs:136` holds
`target: Mutex<RelayTarget>`, and the service exposes `on_relay_intro`
(`:998`) and `on_relay_intro_with_signer` (`:975`). The runtime's service
exposes `on_target_relay_intro`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2783`), which forwards to the
peer-relay service.

### 3.3 The seam has no production caller

`on_target_relay_intro` has exactly **one** caller in the tree, and it is
Plan 531's own lane (`crates/i2pr-daemon/tests/ssu2_relay_requester.rs:670`).
No daemon role calls it.

### 3.4 The introduction is delivered to the wrong role and refused

The runtime's session loop recognises a Relay Intro and routes it to the
**introducer's** handoff, unconditionally
(`crates/i2pr-runtime/src/ssu2_runtime.rs:7290`):

```rust
                SessionEvent::RelayIntro(block) => {
                    relay.push(Ssu2InboundRelay {
                        link_id,
                        peer,
                        event: Ssu2RelayEvent::Intro(block),
                    });
                }
```

The introducer owner then refuses it — correctly, for its own role
(`crates/i2pr-daemon/src/ssu2_introducer.rs:345`):

```rust
            Ssu2RelayEvent::Intro(_) => self.refuse(Ssu2IntroducerRefusal::NotOurRole),
```

So the introduction reaches the router, is refused as *not our role*, and the
target's transport table is never fed.

### 3.5 The target's outbound half has no producer

The target must emit two things, and nothing in production emits either:

- the **accept** — the Relay Response the target sends back to the
  introducer, signed with the target's own identity key over
  `relay_response_preimage(bob_hash, nonce, timestamp, version, endpoint)`;
- the **hole punch** — the out-of-session datagram sealed under the
  **requester's** introduction key, built by the transport's own
  `build_hole_punch`.

The only producer of either in the tree is Plan 531's lane.

### 3.6 The tree's own records name this gap

Plan 531's closure record records, in its **Limitations** section, that *"The
**target** side's response emission has **no product wiring** in this tree"*,
and names the successor: *"a successor must wire the target role."* Plan
531's plan of record records the same limit in its §17.2 amendment log.

## 4. Invariants

1. The target is **opt-in** and **off by default**; a router that has not
   asked for it answers no introduction and emits no hole punch.
2. The owner holds **no task, no timer, no listener and no socket**. The
   product's **existing** tick drives it.
3. Every table the owner holds is **bounded** and evicts deterministically.
   The transport's own target table is the authority on pending
   introductions; the daemon adds **no** second table.
4. An introduction is verified **only** from validated material: the
   transport's own target state machine, the requester's **validated**
   RouterInfo, and the transport's strict address parser.
5. The reply is **one** per introduction. The transport's own replay
   suppression is the authority; the daemon does not keep its own
   introduction table.
6. The accept is signed with this router's **own** identity key and is sent
   to the **introducer**, on the session the introduction arrived on.
7. The hole punch is sealed under the **requester's** introduction key and
   travels **out of session**; it is staged through the runtime's existing
   bounded queue and never through a session.
8. The hole punch's destination is checked against the **same profile** the
   dial target obeys, so the target role cannot widen the router's reach.
9. Every refusal is **typed** and counted **by reason**, and no refusal
   writes into a peer's profile.
10. The transport's relay state machines, the reachability model and both
    floors are untouched.
11. No configuration key is added beside the one opt-in, and no default
    moves.
12. Nothing is promoted: `router.version` stays `0.9.62` and the inventory's
    advertised count stays **zero**.

## 5. Scope

### 5.1 In scope

- The daemon-owned **target owner**: the introduction's policy, the accept's
  signature, the hole punch's pieces, and the counted refusals.
- The runtime's routing of the inbound **Relay Intro** to the target's own
  bounded handoff, instead of the introducer's.
- The runtime's outbound seam that stages one hole punch, applying the
  profile check and the bounded queue.
- The opt-in's parse-time and refuse-time behaviour.

### 5.2 Out of scope

- The **introducer** role, Plan 490's owner and Plan 499's live tag
  issuance. Unchanged.
- The **requester** role, Plan 531's owner. Unchanged.
- The **transport's** relay state machines, its block codec and its
  signatures. They are used, not edited.
- The **reachability model**. A target that answers an introduction gains no
  corroborating class; the relay's success still contradicts direct
  reachability.
- The **dial scope**. The router's SSU2 dial target is loopback-only and the
  private-LAN variant accepts only RFC1918 IPv4; that bound is Plan 431's,
  and the target role obeys it rather than widening it.
- The **handshake** the hole punch initiates. The transport's normal session
  path already establishes it; this plan adds no handshake machinery.

### 5.3 Out of scope, with the owner named

- **The production SSU2 peer-test driver** — Plan 530's §5.3 names the
  successor in this subsystem.
- **A public dial scope** — Plan 431 owns it.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

1. `crates/i2pr-daemon/src/relay_target.rs` — a **new**, bounded,
   daemon-owned target owner. It holds no task, no timer, no listener and no
   socket; it reads the requester's validated RouterInfo from the
   authoritative store, drives the runtime's existing `on_target_relay_intro`
   seam, signs the accept with the router's own key, and hands the hole
   punch's pieces to the runtime's new outbound seam.
2. `crates/i2pr-daemon/src/relay_target.rs` — the owner consumes the inbound
   **Relay Intro** and records the outcome: a verified introduction answers,
   and every refusal is a typed, counted outcome.
3. `crates/i2pr-runtime/src/ssu2_runtime.rs` — the session loop routes an
   inbound Relay Intro to the **target's** bounded handoff, and the
   introducer's handoff stops carrying it.
4. `crates/i2pr-runtime/src/ssu2_runtime.rs` — a **new**, bounded outbound
   seam that builds one hole punch with the transport's own codec and stages
   it, after checking the destination against the router's profile.
5. `crates/i2pr-daemon/src/config.rs` — one **explicit opt-in**, defaulting
   to **off**, refused unless the SSU2 service runs.
6. `crates/i2pr-daemon/src/lib.rs` (or the composition root) — the owner is
   constructed and its tick is driven on the product's **existing** tick.

Nothing else changes. The transport, the netdb and the runtime's reachability
model are **not** edited.

## 7. Design decisions

- **D1 — The owner is the daemon's, the mechanics are the runtime's.** The
  daemon owns the *policy* — which introduction, when, what the accept says
  — and the runtime owns the *mechanics*: the sockets, the session, the
  bounded queues and the transport's verification. The seam between them is
  the runtime's existing `on_target_relay_intro`, so the transport's state
  machine is used, not re-implemented.
- **D2 — The introduction is routed to the target's own handoff.** The
  introducer's handoff stops carrying it. The introducer owner already
  refuses an introduction as `NotOurRole`, so nothing is lost; but the
  refusal is no longer the *only* thing that happens to one.
- **D3 — The transport's target table is the authority on pending
  introductions.** The daemon keeps **no** introduction table. The transport
  already suppresses a replayed `(nonce, alice_hash)` pair and expires a
  pending introduction on its own deadline; a second table would be a second
  source of truth.
- **D4 — The requester is read from the validated store.** The requester's
  **signing** public key is needed to verify the forwarded signature, and its
  **introduction** key is needed to seal the hole punch. Both come from the
  requester's validated RouterInfo, through the transport's own strict
  `Ssu2RouterAddress::parse`. Unvalidated bytes are never parsed.
- **D5 — The accept is bound to the introducer.** The accept's signature is
  over `relay_response_preimage(bob_hash, …)`, where `bob_hash` is the
  **introducer's** hash — the session peer the introduction arrived on. The
  introducer forwards the accept verbatim, so the accept must be bound to the
  introducer that forwarded it.
- **D6 — The hole punch is staged, never sessioned.** It is sealed under the
  **requester's** introduction key and travels out of session. It goes
  through the runtime's existing bounded staging queue — the same queue the
  relay blocks already use — so it is bounded by the runtime's own limits and
  never becomes an unbounded send.
- **D7 — The destination obeys the dial profile.** The hole punch's
  destination is the requester's endpoint **as the introducer saw it**. It is
  checked against the same loopback / RFC1918-IPv4 profile the dial target
  obeys, so the target role cannot widen the router's reach beyond what the
  router already dials.
- **D8 — One opt-in only.** The switch follows the `[ssu2] peer_test` and
  `[ssu2] relay_requester` precedent: a boolean, `deny_unknown_fields`
  already, off by default, and refused at parse time when the SSU2 service
  is off.
- **D9 — One answer per introduction.** The transport's replay suppression
  decides, and the daemon adds no second decision. A replayed introduction
  produces **no** second accept and **no** second hole punch.

## 8. Work packages

- **WP1 — the daemon's target owner.** The introduction's policy, the
  accept's signature, the hole punch's pieces, and the counted refusals.
- **WP2 — the runtime's introduction route and the outbound seam.** The
  session loop routes the introduction to the target's handoff, and the
  outbound seam builds and stages one hole punch.
- **WP3 — the lane and the guard.** The lane drives three real routers over
  loopback and proves the target answers; the guard pins the shape.
- **WP4 — the documentation, the inventory and the amendment log.**

## 9. Failure, cancellation, restart and contention semantics

- **Failure.** Every refusal is typed and counted by reason, and leaves no
  state behind: no accept, no hole punch, no pending introduction. An
  introduction that cannot be verified records nothing.
- **Cancellation.** The plan adds **no task**. The owner's work is a single
  synchronous step on the product's existing tick.
- **Restart.** Nothing is persisted. The transport's target table is live
  state and is rebuilt empty; a restart answers no introduction and punches
  no hole.
- **Contention.** The owner takes the daemon's own locks, in the order the
  daemon's other owners use, and introduces **no new lock** beyond the
  runtime's existing state lock.

## 10. Compatibility and migration

- **No wire change.** The relay blocks, their types, their lengths and their
  signatures are exactly what the tree already sends and receives.
- **No configuration migration.** One boolean is added, off by default and
  refused without the SSU2 service.
- **A new reading for the router.** A router that has asked for the target
  role may answer an introduction. It publishes nothing new and advertises
  nothing new.
- **No change to the transport's model.** The relay-firewalled signal already
  exists and already contradicts; a target that answers gains no
  corroborating class.

## 11. Required tests

### 11.1 The lane

- the opt-in is **off** by default and refused without the SSU2 service;
- an introduction addressed to this router reaches the **target**, not the
  introducer, and the introducer's own refusal path is unchanged;
- the target **verifies** the introduction: a forged signature, a stale
  timestamp, an unknown requester and a replay are each refused and counted;
- the target **answers**: the accept reaches the introducer, the introducer
  verifies it and forwards it, and the requester correlates it;
- the target **punches the hole**: the hole punch arrives out of session at
  the requester, and the requester's state becomes `Firewalled`;
- a completed answer records the **relay-firewalled** signal and **never** a
  corroborating class;
- every refusal is typed and counted by reason, and writes **nothing** into
  a peer's profile.

### 11.2 The guard

- the owner is in the **daemon** and the mechanics stay in the **runtime**;
- the owner holds no task, no timer, no listener and no socket;
- the answer is produced only from **validated** material;
- the opt-in is **off** by default and refused without the SSU2 service;
- the transport's relay state machines and the reachability model are
  untouched;
- the introduction is routed to the **target's** handoff and the
  introducer's handoff no longer carries it;
- the hole punch is staged and never sessioned, and its destination obeys
  the dial profile;
- no configuration key beside the one opt-in.

Every guard rule needs an inverted probe that fires it, and every rule fails
closed when the item it names is absent.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --test ssu2_relay_target -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_relay_target_boundary -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor from `AGENTS.md`.

## 13. Documentation updates

- `specs/support.toml` — the new surface, `experimental` and
  `advertised = false`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the plan's gates.
- `specs/protocols/09-ssu2.md` — the plan's scope note.
- `docs/architecture/i2pr-runtime.md` and `docs/architecture/i2pr-daemon.md` —
  one section each.

## 14. Acceptance criteria and stop conditions

The plan passes when:

1. the opt-in is **off** by default and refused without the SSU2 service;
2. an inbound Relay Intro reaches the **target's** handoff, and the
   introducer's handoff no longer carries it;
3. the target verifies the introduction through the transport's own target
   state machine, and a forged, stale, unknown or replayed introduction is
   refused and counted;
4. the target **answers**: the accept reaches the introducer, which verifies
   it and forwards it, and the requester correlates it;
5. the target **punches the hole** out of session, the requester receives it,
   and the relay-firewalled signal is recorded with **no** corroborating
   class;
6. every refusal is typed and counted by reason, and no refusal writes into
   a peer's profile;
7. the transport's relay state machines and the reachability model are
   untouched, and both floors are unchanged;
8. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, and every rule fails closed on an absent item;
9. the routine floor is green on the closing tree;
10. `specs/support.toml` records the surface as `experimental` and
    `advertised = false`, the advertised count stays **zero**,
    `router.version` stays `0.9.62`, `Cargo.toml` and `Cargo.lock` stay
    byte-identical, and no boundary script is weakened.

**Stop conditions.** The plan stops and is re-scoped, rather than widened, if:

- the target turns out to need a change to the transport's relay state
  machines;
- the requester's record turns out to need a **network** lookup that the
  authoritative store cannot serve — that is Plan 433's blocked composition,
  and this plan must not do it;
- the delivery turns out to need a new task, a new timer, a new listener or a
  new socket;
- the plan turns out to need the production peer-test driver, or a public
  dial scope, after all.

**Known limitations, recorded not hidden.**

- The delivery is confined to the **controlled loopback profile**: the
  router's SSU2 dial target is loopback-only and the private-LAN variant
  accepts only RFC1918 IPv4, so the target can only answer an introduction
  for a requester on a local or private network. That bound is Plan 431's.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology, and no stock router implements a router-level relay target.
- The requester's **validated** RouterInfo must be in the store. A requester
  this router does not hold is a typed, counted refusal — a **local**
  condition — and is never written into the requester's profile.
- The hole punch **initiates** the handshake but does not perform it; the
  transport's normal session path already establishes it, and this plan adds
  no handshake machinery.

## 15. Closure evidence required

- The commits that carry each work package.
- A requirement-to-evidence matrix: every acceptance criterion above, the
  file and line that satisfies it, and the test that proves it.
- The exact commands run, with their outcomes.
- The invariant, failure, migration and security reviews of §4, §9 and §10.
- The negative controls: every mutation applied, the test it broke, and the
  proof that no file on disk was written.
- The findings by severity, each either fixed or recorded with its owner.
- The correction to Plan 531's recorded limitation, in this plan's closure
  record and in the registry row — never in the closed record of the plan
  that made the claim.
- The roadmap disposition and the unblock audit.

## 16. Handoff notes

- The plan's subject is the **target** half of the SSU2 relay protocol: the
  router must be able to answer a relay introduction when it is the peer
  being reached.
- The plan adds **no** capability to the transport. It composes machinery the
  tree already owns — the transport's target state machine, the runtime's
  target seam, the transport's hole-punch codec and the runtime's bounded
  staging queue — and gives them a caller.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **public dial scope**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer half**
  all stay out of scope, with their owners named.

## 17. Amendment log

### 17.1 — 2026-11-10, registration

The plan is registered as **532**, the next free number. §3 records the
evidence as it stood at registration: the transport's complete target state
machine, the runtime's target table and its seam with no production caller,
the introduction routed to the introducer's handoff and refused as not our
role, the target's two missing emissions, and Plan 531's own recorded
limitation that names this successor.

§3 is evidence and is **not** rewritten. Any later correction is added here
as an amendment, additively.

### 17.2 — 2026-11-11, implementation findings

The implementation settled six questions the registration left open. §3 is
**not** rewritten; the corrections live here and in the closure record.

**The introduction's route is gated on the opt-in — a correction to §6.3.**
§6.3 said the session loop "routes an inbound Relay Intro to the **target's**
bounded handoff, and the introducer's handoff stops carrying it", and §7's D2
said "The introducer's handoff stops carrying it". The implementation routes
the introduction to the target's handoff **iff** `ssu2.relay_target` is on;
with the opt-in off — the default — the introduction keeps travelling the
**introducer's** handoff exactly as before and is refused there as
`NotOurRole`. The reason is §4's first invariant: a router that has not asked
to answer a relay introduction answers no introduction, and moving the
introduction into the target's handoff on a router that never opted in would
change what that router *observes* — the introduction would leave the
introducer's handoff and the introducer owner would never see it — without the
router ever answering. The plan's intent holds exactly where the opt-in is on:
with it on, the introducer's handoff does not carry the introduction. The lane
proves **both** halves, and the correction is recorded here so a reader of
§6.3 does not have to guess.

**The target role needs the authoritative store, and the store lives in the
bootstrap.** The target's owner reads the requester's validated RouterInfo out
of the authoritative store, and in the composition root that store is the
bootstrap's (`crates/i2pr-daemon/src/lib.rs:5631`): the target's handoff is
drained only when the daemon holds a `Bootstrap`. With no bootstrap the
target's introductions are never drained. The requester has the same
dependency — Plan 531's wiring drains the requester's handoff under the same
bootstrap guard — so this is a property of the composition root and not of
this plan. It is recorded as a limitation rather than hidden.

**The accept's token has no consumer in this tree.** The target mints a fresh
nonzero token with the router's own randomness and the transport refuses a
zero token, so the accept is well formed. The **requester's establishment
dial** that would consume it — the handshake the hole punch initiates — is not
in the tree: the transport's normal session path establishes the session, and
the token's consumer does not exist yet. §5.2's "the transport's normal
session path already establishes it" holds; the token's consumer is a
successor's work, and the token's value is opaque to the tree today.

**The transport's own target table is the authority on a replay.** The daemon
keeps **no** introduction table, as §4's fifth invariant requires. After the
answer is emitted, the runtime **consumes** the pending introduction
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1035`), and the transport's `seen`
set suppresses a replayed `(nonce, alice_hash)` pair. The lane proves that a
replay produces no second accept and no second hole punch.

**The hole punch's destination obeys the union of the two dial profiles.** §7's
D7 said the destination is "checked against the same loopback / RFC1918-IPv4
profile the dial target obeys". The runtime offers two dial constructors — the
ordinary one is loopback-only and the private-LAN variant accepts only
RFC1918 IPv4 — so the implementation checks the **union** of the two
(`crates/i2pr-runtime/src/ssu2_runtime.rs:5427`): a loopback address of either
family, or an RFC1918 IPv4 literal. The bound is the same bound; the union is
the only reading that covers both dial profiles the router actually has. The
target role still cannot widen the router's reach: a public address is refused
under either profile.

**One line in another plan's lane file moved.**
`crates/i2pr-daemon/tests/netdb_integration.rs` gained exactly one line —
`relay_target: false` — because the new runtime-config field has no default at
that call site. It is a mechanical struct-literal fix; no behaviour in that
file changed. It is recorded here so the drift is not mistaken for a change in
scope.

### 17.3 — 2026-11-11, citation drift

The implementation inserted the target into
`crates/i2pr-runtime/src/ssu2_runtime.rs`, into
`crates/i2pr-runtime/src/ssu2_peer_relay.rs` and into
`crates/i2pr-daemon/src/service_product.rs`, so the line numbers the **closed
records** cite into those files moved. §3 is **not** rewritten, and no closed
record is rewritten; this table is the additive pointer, and it is the whole of
the drift. It continues Plan 531's own pointer table
(`plans/implementation/network-participation/531-the-router-must-reach-a-peer-through-its-introducer.md`,
§17.3) rather than replacing it.

| The closed record's citation | after Plan 531 | after Plan 532 | the line it names |
| --- | --- | --- | --- |
| `ssu2_runtime.rs:1830` | `:1913` | `:1994` | `let reachability_policy = if config.explicit_bind_corroboration {` |
| `ssu2_runtime.rs:2120` | `:2219` | `:2316` | `self.note_bound_endpoints_as_explicit_bind();` |
| `ssu2_runtime.rs:2819` | `:3049` | `:3225` | the address-agreement comment |
| `ssu2_runtime.rs:2849` | `:3079` | `:3255` | `/// Records one typed peer-test outcome into the router's reachability.` |
| `ssu2_runtime.rs:2884` | `:3114` | `:3290` | `pub fn note_observed_external_address(` |
| `ssu2_runtime.rs:2948` | `:3178` | `:3354` | `fn bound_endpoint(&self, family: AddressFamily)` |
| `ssu2_runtime.rs:3111` | `:3341` | `:3517` | the loopback refusal |
| `ssu2_runtime.rs:3196` | `:3426` | `:3602` | `fn note_bound_endpoints_as_explicit_bind(&self) {` |
| `ssu2_runtime.rs:3228` | `:3458` | `:3634` | `pub fn note_explicit_bind_for_controlled_qualification(` |
| `ssu2_runtime.rs:3263` | `:3493` | `:3669` | the controlled class-0 record |
| `ssu2_runtime.rs:5216` | `:5446` | `:5639` | the data-phase session builder's comment |
| `ssu2_runtime.rs:6987` | `:7218` | `:7412` | the bounded-candidate comment |
| `ssu2_peer_relay.rs:474` | `:485` | `:485` | `fn count_relay_response(&self) {` |
| `ssu2_peer_relay.rs:481` | `:492` | `:492` | `pub fn start_relay_request(` |
| `ssu2_peer_relay.rs:497` | `:535` | `:535` | `pub fn on_relay_response(` |
| `service_product.rs:4796` | `:4808` | `:4813` | the Plan 528 reachability-gate comment |
| `service_product.rs:4804` | `:4816` | `:4821` | `== i2pr_transport::ReachabilityState::Firewalled;` |

The peer-relay service's three cited lines did **not** move: Plan 532 added
`consume_target_intro` below them, not above. No citation into any other file
moved. The transport's, the daemon's `lib.rs` and `config.rs`, the netdb's and
the closure records' own citations are unchanged.

### 17.4 — 2026-11-11, the corrections to the closed plans' claims

No closed record is rewritten. The corrections live here, in this plan's
closure record and in the registry row.

1. **Plan 531's** limitation
   (`plans/closure/network-participation/531-status.md:100`-`:104` and
   `:521`-`:526`) said *"The **target** side's response emission has **no
   product wiring** in this tree. The transport says the *caller* emits the
   accept and the hole punch, and no daemon role does that ... A successor must
   wire the target role."* That was accurate for the tree it describes.
   **Plan 532 is the successor it named, and the wiring now exists**: the
   daemon's `RelayTargetOwner` drives the runtime's target seam, signs the
   accept with the router's own identity key over the introducer-bound
   preimage, and stages the hole punch through the runtime's bounded queue.
   The correction is recorded here and in this plan's closure record, never in
   the closed record.
2. **Plan 531's** Low finding 1
   (`plans/closure/network-participation/531-status.md:553`-`:559`) named the
   same gap and the same owner. **It is discharged.**
3. **Plan 531's** §17.4 closing paragraph said *"the **target** half of the
   exchange still has no product wiring"*. It is **superseded on that point**:
   all three roles of the relay protocol now have a producer in this tree.
4. **Plan 531's** unblock-audit row *"The **target** side's response emission |
   **unwired**"* (`plans/closure/network-participation/531-status.md:645`) is
   **superseded on that point** in the same way.
5. **Plan 527's** Low finding 1 and its repetitions in Plans 528, 529 and 530
   said the relay-success indication had no producer. Plan 531 discharged the
   **requester** half; **Plan 532 discharges the target half**, so the relay
   protocol's three roles all have producers and the transport's contradicting
   *relay-firewalled* class has a producer on the requester's side.

Two corrections run the other way, and they are recorded for the same reason.
The target role is **not** complete in this tree: the requester's establishment
dial that would consume the accept's token is not in the tree, and the
requester owner has no deferred-punch buffer, so a hole punch that arrives
before the accept is refused by the transport's own requester table. And the
target role needs the authoritative store, which the composition root takes
from the bootstrap, so with no bootstrap the target's introductions are never
drained. Both are recorded in this plan's closure record with their owners
named; neither is hidden, and neither is a promotion.
