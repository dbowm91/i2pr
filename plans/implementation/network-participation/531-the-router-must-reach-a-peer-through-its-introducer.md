# Plan 531 — the router must reach a peer through the peer's introducer

**Status:** registered, 2026-11-10. The next free plan number after 530.

The tree implements the SSU2 relay protocol's **introducer** side and the
**target's** tag acquisition, but the **requester** side is unwired. The
requester's transport state machine, the runtime's relay-request seam and the
runtime's hole-punch handler all exist and all have **zero production
callers**, so a router cannot reach a peer that is only reachable through an
introducer — and the relay-firewalled indication, the transport's contradicting
class, has no producer at all.

## 1. Objective

Make the router able to **reach a peer through the peer's published
introducer set**, and give the relay-firewalled indication its producer.

Concretely:

1. a **daemon-owned requester** selects a target peer whose validated
   RouterInfo publishes an introducer set, resolves the introducer, and
   starts one bounded relay request through the runtime's existing seam;
2. the runtime routes the inbound **Relay Response** and the inbound **Hole
   Punch** to that owner;
3. the outcome is recorded under the transport's own model: a completed
   exchange feeds the **relay-firewalled** signal, and a refusal is counted
   by reason.

## 2. Why ready

- The **transport** owns the requester state machine
  (`crates/i2pr-transport-ssu2/src/relay.rs:446`), the response correlation
  (`:527`) and the hole-punch handling (`:569`).
- The **runtime** owns the peer-relay service and already exposes
  `start_relay_request` (`:481` in
  `crates/i2pr-runtime/src/ssu2_peer_relay.rs`), `on_relay_response` (`:497`)
  and `on_hole_punch` (`:572`).
- The **daemon** already owns the introducer role
  (`crates/i2pr-daemon/src/ssu2_introducer.rs`) and already consumes the
  runtime's relay events (`:318`).
- The **introducer set** is already the specification's form (Plan 523) and is
  already parsed: `Ssu2RouterAddress::introducers()`
  (`crates/i2pr-transport-ssu2/src/address.rs:676`) returns the published
  `ih`/`iexp`/`itag` tuples, and `Ssu2Introducer` (`:485`) exposes the
  introducer's hash, expiry and relay tag.
- The **authoritative** validated-RouterInfo store is reachable in production:
  `ProfileStore::peers()` (`crates/i2pr-netdb/src/profile_store.rs:738`) hands
  out the `RouterInfoStore`, whose `get()`
  (`crates/i2pr-netdb/src/store.rs:177`) resolves a hash to a validated
  record.
- The gap is a missing **composition**, not a missing capability.

## 3. Current implementation evidence

### 3.1 The transport's requester state machine exists

`crates/i2pr-transport-ssu2/src/relay.rs:446`:

```rust
pub struct RelayRequester {
```

and its `start` (`:477`):

```rust
    pub fn start(
        &mut self,
        nonce: u32,
        tag: u32,
        bob_hash: [u8; 32],
        charlie_hash: [u8; 32],
        alice_endpoint: Ssu2Endpoint,
        now_ms: u64,
    ) -> Result<(), RelayError> {
```

It enforces the nonce and tag's nonzero-ness, the duplicate-correlation
refusal, the global request ceiling (`MAX_RELAY_REQUESTS_GLOBAL`) and the
per-peer quota (`MAX_RELAY_REQUESTS_PER_PEER`) before it records anything.

### 3.2 The runtime's relay-request seam exists and has no caller

`crates/i2pr-runtime/src/ssu2_peer_relay.rs:481`:

```rust
    pub fn start_relay_request(
        &self,
        nonce: u32,
        tag: u32,
        bob_hash: [u8; 32],
        charlie_hash: [u8; 32],
        alice_endpoint: Ssu2Endpoint,
        now_ms: u64,
    ) -> Result<(), i2pr_transport_ssu2::RelayError> {
```

A search of the whole workspace finds **no production caller**: the only
references are the runtime's own definition and the transport's own tests.

### 3.3 The hole punch has no producer and no route

`crates/i2pr-runtime/src/ssu2_peer_relay.rs:572` is `on_hole_punch`, and the
transport's own codec and state machine live at
`crates/i2pr-transport-ssu2/src/relay.rs:569`. Neither has a production
caller, and the runtime's session loop contains **no** hole-punch handling at
all: the inbound path recognises a Relay Request, a Relay Intro and a Relay
Response
(`crates/i2pr-runtime/src/ssu2_runtime.rs:7056`, `:7063`, `:7070`) and
nothing else.

### 3.4 The introducer side is complete, and it resolves the target by the tag

The daemon's module doc states the protocol's shape
(`crates/i2pr-daemon/src/ssu2_introducer.rs`):

```text
requester ──Relay Request──▶ this router ──Relay Intro──▶ target
requester ◀─Relay Response── this router ◀─Relay Response── target
```

and the owner resolves the target through the tag before it verifies the
requester's signature
(`crates/i2pr-daemon/src/ssu2_introducer.rs`, `on_relay_request`):

```rust
        let Some(target) = self.runtime.relay_tag_owner(block.relay_tag(), now_secs) else {
            return self.refuse(Ssu2IntroducerRefusal::UnknownTag);
        };
```

So the tag names the target, and the introducer needs **no** network
lookup to forward the introduction. The requester is the side that must
carry the target's hash and the tag.

### 3.5 The requester has everything it needs, and nothing uses it

The requester needs five values: a nonce, the **tag**, the introducer's
hash, the target's hash and its own bound endpoint. Every one of them is
already reachable in production:

| Value | Where it comes from |
|---|---|
| nonce | the router's own randomness — nonzero, never reused |
| **tag** | the target's **published** introducer set: `Ssu2Introducer::relay_tag()` (`crates/i2pr-transport-ssu2/src/address.rs:522`) |
| introducer's hash | the same published set: `Ssu2Introducer::peer_hash()` (`:511`) |
| target's hash | the peer's own validated RouterInfo |
| own bound endpoint | the runtime's `bound_endpoint` |

The published set is read from the target's validated RouterInfo through the
transport's own **strict** parser:
`Ssu2RouterAddress::parse(&RouterAddress)`
(`crates/i2pr-transport-ssu2/src/address.rs:570`) and
`introducers()` (`:676`). The introducer's **endpoint** — which the
requester must reach — is resolved by hash from the authoritative store:
`ProfileStore::peers()` → `RouterInfoStore::get()`.

### 3.6 The consequence: a firewalled peer is unreachable, and the contradicting class has no producer

A peer that publishes an introducer set is a peer the network cannot reach
directly. The router has the **publication** half (Plan 525 publishes the
introducer set it learned) and the **introducer** half (Plan 490's role), but
not the **acquisition** half that would let it use another peer's published
introducer set.

The transport's model already has a signal for the fact
(`crates/i2pr-transport/src/reachability.rs:155`):

```rust
    RelayFirewalledSignal {
```

which is corroboration class 4 (`:229`), **contradicts** reachability
(`:197`) and never supports it (`:182`). The runtime already has the
recorder for it — `note_relay_firewalled`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3349`) — and it has **no
producer**: Plan 528's §5.3 named this exactly: *"The relay-firewalled
indication's producer. The requester side is unwired and triggerless in this
tree."*

### 3.7 The tree's own records name this gap

- Plan 527's §5.3: *"**The requester side of the SSU2 relay protocol.** The
  router implements the introducer side; a router that wants to be *relayed
  for* needs the requester side. **Owner:** a successor plan in this
  subsystem."*
- Plan 528's §5.3: *"**The relay-firewalled indication's producer.** The
  requester side is unwired and triggerless in this tree. **Owner:** a
  successor plan."*
- Plan 529's §5.3 and Plan 530's §5.3 both repeat it, with the owner named
  as *"a successor plan in this subsystem"*.

## 4. Invariants

1. The requester is **opt-in** and **off by default**; a router that has not
   asked for it sends no relay request.
2. The owner holds **no task, no timer, no listener and no socket**. The
   product's **existing** tick drives it, exactly as Plan 524's
   `IntroducerRequestOwner` is driven.
3. Every table the owner holds is **bounded** and evicts deterministically.
4. A relay request is started only from **validated** material: the target's
   validated RouterInfo, the transport's own strict address parser, and the
   introducer's validated RouterInfo resolved by hash.
5. The tag presented is the one the **published** introducer set carries, and
   the target's hash is the one the same set belongs to. A request never
   names a target the tag does not identify.
6. A relay request is sent only to a peer the router holds an **authenticated
   session** with, or one it can establish; the relay blocks are session
   blocks.
7. Every refusal is **typed** and counted **by reason**, and no refusal
   writes into a peer's profile.
8. A completed exchange feeds the transport's **relay-firewalled** signal and
   **never** a corroborating class: the relay's success proves that the
   router is reachable **through the relay**, not directly.
9. One local condition is never written into a peer's profile.
10. The transport's reachability **model**, its signal kinds, its
    corroboration classes and both floors are untouched.
11. No configuration key is added beside the one opt-in, and no default
    moves.
12. Nothing is promoted: `router.version` stays `0.9.62` and the inventory's
    advertised count stays **zero**.

## 5. Scope

### 5.1 In scope

- The daemon-owned **requester owner**: the selection of a target, the
  resolution of the introducer, and the start of one bounded relay request.
- The runtime's routing of the inbound **Relay Response** and the inbound
  **Hole Punch** to that owner.
- The recording of the outcome: the relay-firewalled signal on a completed
  exchange, and the counted refusals everywhere else.
- The opt-in's parse-time and refuse-time behaviour.

### 5.2 Out of scope

- The **introducer** role. Plan 490's owner and Plan 499's live tag issuance
  are unchanged.
- The **target**'s tag acquisition and publication. Plans 524 and 525 are
  unchanged.
- The **transport's** relay state machines, its block codec and its
  signatures. They are used, not edited.
- The **reachability model**. The relay-firewalled signal already exists and
  already contradicts; the plan only gives it a producer.
- The **dial scope**. The router's SSU2 dial target is loopback-only and the
  private-LAN variant accepts only RFC1918 IPv4; that bound is Plan 431's.

### 5.3 Out of scope, with the owner named

- **The production SSU2 peer-test driver** — Plan 530's §5.3 names the
  successor in this subsystem.
- **A public dial scope** — Plan 431 owns it.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

1. `crates/i2pr-daemon/src/relay_requester.rs` — a **new**, bounded,
   daemon-owned requester. It holds no task, no timer, no listener and no
   socket; it selects a target from the authoritative validated-RouterInfo
   store, resolves the introducer by hash, draws a **nonzero** nonce from
   the router's own randomness, and starts one relay request through the
   runtime's existing `start_relay_request` seam.
2. `crates/i2pr-daemon/src/relay_requester.rs` — the owner consumes the
   inbound **Relay Response** and the inbound **Hole Punch**, and records the
   outcome: a completed exchange calls the runtime's `note_relay_firewalled`,
   and every refusal is a typed, counted outcome.
3. `crates/i2pr-runtime/src/ssu2_runtime.rs` — the session loop recognises an
   inbound **Hole Punch** and routes it, with the relay blocks, over the
   existing bounded relay handoff.
4. `crates/i2pr-daemon/src/config.rs` — one **explicit opt-in**, defaulting
   to **off**, refused unless the SSU2 service runs.
5. `crates/i2pr-daemon/src/lib.rs` (or the composition root) — the owner is
   constructed and its tick is driven on the product's **existing** tick.

Nothing else changes. The transport, the netdb and the runtime's reachability
model are **not** edited.

## 7. Design decisions

- **D1 — The owner is the daemon's, the mechanics are the runtime's.** The
  daemon owns the *policy* — which target, which introducer, when — and the
  runtime owns the *mechanics*: the sockets, the session, the bounded tables
  and the transport's verification. The seam between them is the runtime's
  existing `start_relay_request`, so the transport's state machine is used,
  not re-implemented.
- **D2 — The tick is the product's existing one.** The owner holds no task
  and no timer, exactly as Plan 524's `IntroducerRequestOwner` does. A
  requester that is not on the tick cannot be bounded by the product's own
  cadence.
- **D3 — The target is chosen from validated material only.** A peer's
  published introducer set is read out of its **validated** RouterInfo, which
  the netdb checked for signature, identity hash, static-key binding and
  freshness before it reached the store. Unvalidated bytes are never parsed.
- **D4 — The tag and the target come from the same published set.** A
  request never names a target the presented tag does not identify, because
  both values are read from one introducer entry.
- **D5 — The introducer is resolved by hash, from the same store.** The
  published set names the introducer by **hash** (`ihN`), never by endpoint,
  so the requester must resolve it. The resolution is a store read, not a
  network lookup, and it is bounded by the store's own ceilings.
- **D6 — The relay's success is a *contradiction*, never a corroboration.**
  The transport's `RelayFirewalledSignal` is class 4 and contradicts
  (`crates/i2pr-transport/src/reachability.rs:197`, `:229`). A router that
  reaches a peer only through a relay is **not** directly reachable, and the
  plan records it that way. The corroboration floor is untouched.
- **D7 — The request is sent on an authenticated session.** The relay blocks
  are session blocks; a request over an unauthenticated path would not be
  verifiable by the introducer, and the introducer verifies the requester's
  signature over the specification's preimage.
- **D8 — No new key beyond the one opt-in.** The switch follows the
  `[ssu2] peer_test` precedent: a boolean, `deny_unknown_fields` already, off
  by default, and refused at parse time when the SSU2 service is off.
- **D9 — The nonce is the router's own and never zero.** The transport
  refuses a zero nonce and a zero tag
  (`crates/i2pr-transport-ssu2/src/relay.rs:486`), and it refuses a duplicate
  correlation, so the owner draws a fresh nonce per request and never reuses
  one while the request is live.

## 8. Work packages

- **WP1 — the daemon's requester owner.** The selection, the resolution, the
  start, the outcome and the counted refusals.
- **WP2 — the runtime's hole-punch route.** The session loop recognises an
  inbound Hole Punch and hands it to the owner with the other relay blocks.
- **WP3 — the lane and the guard.** The lane drives three real routers over
  loopback and proves the exchange completes and records the relay-firewalled
  signal; the guard pins the shape.
- **WP4 — the documentation, the inventory and the amendment log.**

## 9. Failure, cancellation, restart and contention semantics

- **Failure.** Every refusal is typed and counted by reason, and leaves no
  state behind: no target, no introducer, no tag, no session, no request.
  A request that cannot be started records nothing.
- **Cancellation.** The plan adds **no task**. The owner's work is a single
  synchronous step on the product's existing tick.
- **Restart.** Nothing is persisted. The owner's tables are live state and
  are rebuilt from the store; a restart with no target finds nothing and
  asks nobody.
- **Contention.** The owner takes the daemon's own locks, in the order the
  daemon's other owners use, and introduces **no new lock** beyond the
  runtime's existing state lock.

## 10. Compatibility and migration

- **No wire change.** The relay blocks, their types, their lengths and their
  signatures are exactly what the tree already sends and receives.
- **No configuration migration.** One boolean is added, off by default and
  refused without the SSU2 service.
- **A new reading for the router.** A router that has asked for the
  requester may send relay requests to an introducer. It publishes nothing
  new and advertises nothing new.
- **No change to the transport's model.** The relay-firewalled signal already
  exists and already contradicts; the plan only gives it a producer.

## 11. Required tests

### 11.1 The lane

- the opt-in is **off** by default and refused without the SSU2 service;
- the owner selects a target that **publishes** an introducer set, and never
  one that does not;
- the request carries the **published** tag and the target's own hash, and
  the introducer is resolved by hash from the store;
- the exchange **completes**: the response is correlated and the hole punch
  is routed, over real loopback UDP between three real routers;
- a completed exchange records the **relay-firewalled** signal and **never** a
  corroborating class;
- every refusal is typed and counted by reason, and writes **nothing** into
  a peer's profile;
- a request is refused when the tag does not identify the target.

### 11.2 The guard

- the owner is in the **daemon** and the mechanics stay in the **runtime**;
- the owner holds no task, no timer, no listener and no socket;
- the request is started only from **validated** material;
- the opt-in is **off** by default and refused without the SSU2 service;
- the transport's relay state machines and the reachability model are
  untouched;
- the relay-firewalled signal is recorded as a **contradiction**, never as a
  corroboration;
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
cargo test --locked -p i2pr-daemon --test ssu2_relay_requester -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_relay_requester_boundary -- --test-threads=1
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
2. the owner selects a target that publishes an introducer set, from
   **validated** material, and resolves the introducer by hash;
3. the request carries the published tag and the target's own hash;
4. the exchange **completes** over real loopback UDP between three real
   routers, and the response is correlated and the hole punch routed;
5. a completed exchange records the **relay-firewalled** signal and never a
   corroborating class;
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

- the requester turns out to need a change to the transport's relay state
  machines;
- the target selection turns out to need a **network** lookup that the
  authoritative store cannot serve — that is Plan 433's blocked composition,
  and this plan must not do it;
- the delivery turns out to need a new task, a new timer, a new listener or a
  new socket;
- the plan turns out to need the production peer-test driver, or a public
  dial scope, after all.

**Known limitations, recorded not hidden.**

- The delivery is confined to the **controlled loopback profile**: the
  router's SSU2 dial target is loopback-only and the private-LAN variant
  accepts only RFC1918 IPv4, so the requester can only reach an introducer
  on a local or private network. That bound is Plan 431's.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology, and no stock router implements the router-level relay requester.
- The relay-firewalled signal **contradicts** reachability, so a router that
  uses the requester will find its own state fall to `Firewalled` or
  `Unreachable`. That is the transport's own model and is correct: the
  router is reachable **through the relay**, not directly.
- The **production SSU2 peer-test driver** stays unbuilt and the
  **requester** side of the *relay* protocol's peer-test analogue stays out
  of scope, with their owners named.

## 15. Closure evidence required

- The commits that carry each work package.
- A requirement-to-evidence matrix: every acceptance criterion above, the
  file and line that satisfies it, and the test that proves it.
- The exact commands run, with their outcomes.
- The invariant, failure, migration and security reviews of §4, §9 and §10.
- The negative controls: every mutation applied, the test it broke, and the
  proof that no file on disk was written.
- The findings by severity, each either fixed or recorded with its owner.
- The correction to Plan 527's, Plan 528's, Plan 529's and Plan 530's closed
  claims about the requester side, in this plan's closure record and in the
  registry row — never in the closed record of the plan that made the claim.
- The roadmap disposition and the unblock audit.

## 16. Handoff notes

- The plan's subject is the **requester** half of the SSU2 relay protocol:
  the router must be able to reach a peer that publishes an introducer set.
- The plan adds **no** capability to the transport. It composes machinery the
  tree already owns and gives the relay-firewalled signal the producer Plan
  528 named as missing.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **public dial scope**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer half**
  all stay out of scope, with their owners named.

## 17. Amendment log

### 17.1 — 2026-11-10, registration

The plan is registered as **531**, the next free number. §3 records the
evidence as it stood at registration: the requester's transport state machine
and the runtime's seam with no production caller, the hole punch with no
producer and no route, the introducer side complete and resolving the target
through the tag, the five values the requester needs and where each of them
already comes from, and the relay-firewalled signal with no producer.

§3 is evidence and is **not** rewritten. Any later correction is added here
as an amendment, additively.

### 17.2 — 2026-11-10, implementation findings

The implementation settled eight questions the registration left open. §3 is
**not** rewritten; the corrections live here and in the closure record.

**The tick is the daemon's existing relay tick.** §6.5 said the owner is
"constructed and its tick is driven on the product's **existing** tick", and
§7's D2 said "the tick is the product's existing one". The implementation
drives the daemon's **existing relay tick** — the 100 ms tick in
`crates/i2pr-daemon/src/lib.rs:5453` that already drains the introducer's
handoff — and not `ServiceProduct`'s product tick. The intent of both
sentences holds: the owner adds **no task**, **no timer** and **no cadence** of
its own, and both halves of an exchange share one clock. The choice is
recorded here so a reader of §6.5 does not have to guess which tick it is.

**The requester needed its own bounded handoff, not the shared one.** §6.3
said the session loop "recognises an inbound Hole Punch and routes it, with
the relay blocks, over the existing bounded relay handoff". The implementation
gave the requester a **separate** bounded handoff instead, and the reason is
structural: a hole punch travels **out of session** — it is sealed under the
*receiver's* introduction key and it names neither a session nor a peer hash —
so it cannot ride a handoff whose entries are session-shaped; and a Relay
Response that correlates with one of the router's own live requests belongs to
the **requester** role, not to the introducer's. The new handoff carries the
same ceiling as the relay handoff (`max_inbound_relay_queue`, default 16). The
existing `drain_relay_events` is byte-identical and the transport's state
machines are untouched, so nothing else moved. This narrows §6.3 in mechanism
and widens no scope: the plan's intent — the owner receives the inbound Relay
Response and the inbound Hole Punch — holds exactly.

**A response is routed by nonce correlation, against a routing table.** The
runtime decides which role owns a Relay Response by asking the peer-relay
service whether the response's nonce is one of **this router's own** live
request nonces. That table is a **routing** table — a nonce and a deadline and
nothing else — and it is written exactly where the transport's own requester
table is written, so the two expire on the same `RELAY_REQUEST_TIMEOUT_MS`
deadline. It is deliberately **not** a second copy of the transport's table.

**The signal is recorded when the hole punch is ready, and the family comes
from the router's own bound endpoint.** §6.2 said a completed exchange calls
the runtime's `note_relay_firewalled`. The implementation does that at the
point the transport's own comment names as the requester's firewalled fact —
the hole punch being **ready** — and not on a rejection. The family is the one
the router's own bound endpoint carries, the same endpoint the request itself
presents; a router that can name no endpoint makes no request and records no
signal.

**The candidate pool is the NetDB, and the session requirement is on the
introducer.** §3.5 named the validated RouterInfo store as the source of the
tag and the introducer's hash, and §7's D3 required validated material. The
implementation draws the **target** candidates from the validated store, not
from the live sessions, because a peer that cannot be reached directly holds no
session — the session requirement is on the **introducer**, and the request
rides a session the router already holds with the introducer. The scan is
O(store size) per window and the store is bounded.

**The target's signing key is resolved from the validated store and passed
explicitly.** The runtime has no production signer-registration path, so the
daemon resolves the **target's** signing key through `NetDbSignerResolver` over
the bootstrap's `RouterInfoStore` and passes it to the runtime's
`*_with_signer` seams. A target this router does not hold yields
`TargetUnknown`, and the owner refuses the event rather than guessing. The
router's own Relay Request is signed with the router's own identity key over
the specification's preimage.

**The target side's response emission has no product wiring — the honest
limit.** The transport's target role says the *caller* emits the accept and
the hole punch, and no daemon role does that. The **requester** half is
production code throughout; the **target** half is not. The lane completes the
target's half over the transport's own preimage, with the target's own key,
and the introducer verifies it for real. A successor must wire the target
role; this plan does not, and does not claim to.

**One line in another plan's lane file moved.**
`crates/i2pr-daemon/tests/netdb_integration.rs` gained exactly one line —
`relay_requester: false` — because the new runtime-config field has no
default at that call site. It is a mechanical struct-literal fix; no behaviour
in that file changed. It is recorded here so the drift is not mistaken for a
change in scope.

### 17.3 — 2026-11-10, citation drift

The implementation inserted the requester into
`crates/i2pr-runtime/src/ssu2_runtime.rs`, into
`crates/i2pr-runtime/src/ssu2_peer_relay.rs` and into
`crates/i2pr-daemon/src/service_product.rs`, so the line numbers the **closed
records** cite into those files moved. §3 is **not** rewritten, and no closed
record is rewritten; this table is the additive pointer, and it is the whole of
the drift.

| The closed record's citation | after Plan 531 | the line it names |
| --- | --- | --- |
| `ssu2_runtime.rs:1830` | `:1913` | `let reachability_policy = if config.explicit_bind_corroboration {` |
| `ssu2_runtime.rs:2120` | `:2219` | `self.note_bound_endpoints_as_explicit_bind();` |
| `ssu2_runtime.rs:2819` | `:3049` | the address-agreement comment |
| `ssu2_runtime.rs:2849` | `:3079` | `/// Records one typed peer-test outcome into the router's reachability.` |
| `ssu2_runtime.rs:2884` | `:3114` | `pub fn note_observed_external_address(` |
| `ssu2_runtime.rs:2948` | `:3178` | `fn bound_endpoint(&self, family: AddressFamily)` |
| `ssu2_runtime.rs:3111` | `:3341` | the loopback refusal |
| `ssu2_runtime.rs:3196` | `:3426` | `fn note_bound_endpoints_as_explicit_bind(&self) {` |
| `ssu2_runtime.rs:3228` | `:3458` | `pub fn note_explicit_bind_for_controlled_qualification(` |
| `ssu2_runtime.rs:3263` | `:3493` | the controlled class-0 record |
| `ssu2_runtime.rs:5216` | `:5446` | the data-phase session builder's comment |
| `ssu2_runtime.rs:6987` | `:7218` | the bounded-candidate comment |
| `ssu2_peer_relay.rs:474` | `:485` | `fn count_relay_response(&self) {` |
| `ssu2_peer_relay.rs:481` | `:492` | `pub fn start_relay_request(` |
| `ssu2_peer_relay.rs:497` | `:535` | `pub fn on_relay_response(` |
| `service_product.rs:4796` | `:4808` | the Plan 528 reachability-gate comment |
| `service_product.rs:4804` | `:4816` | `== i2pr_transport::ReachabilityState::Firewalled;` |

No citation into any other file moved. The transport's, the daemon's `lib.rs`
and `config.rs`, the netdb's and the closure records' own citations are
unchanged.

### 17.4 — 2026-11-10, the corrections to the closed plans' claims

No closed record is rewritten. The corrections live here, in this plan's
closure record and in the registry row.

1. **Plan 527's** Low finding 1
   (`plans/closure/network-participation/527-status.md:512`-`:518`) said *"The
   requester side of the SSU2 relay protocol has no production caller.
   `start_relay_request` and `on_relay_response`
   (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:481` and `:497`) are unwired,
   so the relay-success indication is never produced in the field. The
   **recorder** exists and the lane drives it, so the state is reachable; only
   the producer is missing."* That was accurate for the tree it describes.
   **Plan 531 is the successor it named, and the producer now exists**: the
   daemon's `RelayRequesterOwner` drives the runtime's `start_relay_request`
   seam, the runtime routes the correlated Relay Response and the inbound Hole
   Punch back to it, and a completed exchange records the relay-firewalled
   signal through the runtime's own recorder. The two cited lines moved to
   `ssu2_peer_relay.rs:492` and `:535`; the pointer is in §17.3.
2. **Plan 528's** Low finding 2
   (`plans/closure/network-participation/528-status.md:545`-`:550`) repeated
   Plan 527's finding verbatim and named the same owner. **It is discharged**,
   with the same caveat as above: the **requester** half is wired, and the
   **target** half's response emission is not, so the relay-success route
   reaches the contradiction from the requester's side only.
3. **Plan 529's** Low finding 2
   (`plans/closure/network-participation/529-status.md:495`-`:497`) repeated
   the same finding. **It is discharged.**
4. **Plan 530's** Low finding 3
   (`plans/closure/network-participation/530-status.md:506`-`:508`) repeated
   the same finding. **It is discharged.**
5. **The limitations of Plans 528, 529 and 530** that said *"the **requester
   side** stays unwired, so the relay-success route is not taken in the field"*
   (`528-status.md:522`, `529-status.md:471`, `530-status.md:476`) are
   **superseded on that point**: the relay-success route is now taken in the
   field, behind the default-off `ssu2.relay_requester` opt-in. The
   surrounding limitations — the **production** SSU2 peer-test driver unbuilt,
   the loopback-only publication profile, the **stock-peer** half blocked on
   Plan 493 — are **unchanged**.

One correction runs the other way, and it is recorded for the same reason: the
**target** half of the exchange still has no product wiring. A reader who took
"the requester side is wired" to mean "the relay protocol is complete in this
tree" would be wrong. The lane completes the target's half over the
transport's own preimage; no daemon role emits the accept or the hole punch.
