# Plan 524 — the router asks a peer for a relay tag, and records the introducer

**Status:** registered.
**Subsystem:** network-participation. **Registered:** 2026-11-09.
**Predecessor:** Plan 523 (the SSU2 introducer set is the specification's form),
Plan 499 (the live tag request and the relay block carriage), Plan 490 and
Plan 498 (the introducer role owner and its live inbound path).

## 1. Objective

Plan 523 corrected the **form** of the SSU2 introducer set and deliberately left
the **composition** out of scope. The composition is this plan's subject.

The router must be able to:

1. tell that a peer is an **introducer** — a peer that advertises the
   introducer capability, so that the router knows the peer is willing to
   introduce it;
2. **ask** such a peer for a relay tag, over an established session, when the
   router is firewalled;
3. **record** the tag that comes back as an **authenticated introducer**, bound
   to the peer that issued it.

That is the acquisition half of the roadmap's completion definition — *"publishes
the validated introducer set in its RouterInfo address"*. The **publication**
half — building the introducer-only address, choosing it over the direct form,
and signing it — is a successor's subject, and the `C` capability this router
publishes about itself is a third, separate matter.

## 2. Why ready

The tree holds every fact this plan needs, and each fact has exactly one
production caller that is missing.

- The transport can already **mint** a relay tag for a peer that asks
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3204`), and can already **queue**
  a RelayTagRequest toward a peer (`ssu2_runtime.rs:2187`). The request path
  exists; nothing calls it.
- The runtime already **receives** the answer: a `SessionEvent::RelayTag`
  (`ssu2_runtime.rs:6316`). The event is counted and handed on, and then
  dropped.
- The peer-relay service already owns the **introducer table** and the
  insertion path
  (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1101`). The insertion path
  exists; nothing calls it.
- The **daemon** already reads a peer's **validated** RouterInfo from the
  authoritative store — `Bootstrap::store()`
  (`crates/i2pr-daemon/src/bootstrap.rs:261`) → `RouterInfoStore::get`
  (`crates/i2pr-netdb/src/store.rs:177`) — and already does so for a peer's
  public static encryption key
  (`crates/i2pr-daemon/src/service_product.rs:4122`). The peer's advertised
  capabilities are therefore **authenticated**, and reading them needs no new
  trust.

  **Corrected by §17.2.** The registered text put this read in the transport's
  validated peer record. That is true of an **inbound** session and false of
  an **outbound** one, and the request travels outbound.

So the gap is not a missing capability. It is a missing **call**: four public
items with no production caller, and one event that arrives and dies.

## 3. Current implementation evidence

### 3.1 The transport can queue a RelayTagRequest, and nobody queues it

`crates/i2pr-runtime/src/ssu2_runtime.rs:2187`:

```rust
pub fn queue_relay_tag_request(&self, peer: PeerId) -> Result<(), Ssu2RelayQueueError> {
    self.queue_relay_block(peer, |session| session.queue_relay_tag_request())
}
```

The request body is empty by design: the peer is the destination, so the
session is the whole destination, and whether the *receiving* peer answers is
that peer's own introducer gate (`ssu2.introducer_service`), not the
requester's. Every refusal the queue can produce is already typed and counted
by `queue_relay_block` (`ssu2_runtime.rs:2225`).

### 3.2 The runtime receives the answer and drops it

`crates/i2pr-runtime/src/ssu2_runtime.rs:6316`:

```rust
SessionEvent::RelayTag(tag) => {
    self.shared
        .counters
        .relay_tags_observed
        .fetch_add(1, Ordering::Relaxed);
    relay.push(Ssu2InboundRelay {
        link_id,
        peer,
        event: Ssu2RelayEvent::Tag(tag),
    });
}
```

The event is counted and pushed onto the bounded relay handoff. On the daemon
side the drain feeds it to the introducer **role owner**
(`crates/i2pr-daemon/src/ssu2_relay_drain.rs:174`), and the role owner
**refuses** it:

`crates/i2pr-daemon/src/ssu2_introducer.rs:349`:

```rust
Ssu2RelayEvent::Tag(_) => self.refuse(Ssu2IntroducerRefusal::NotOurRole),
```

That refusal is **correct** — a `Tag` event is the answer to *this router's own*
request, not an inbound request the role owner should serve. But the effect is
that a legitimate event is counted as a refusal and dies there. Nothing
records the tag, so nothing learns the peer.

### 3.3 The peer-relay service owns the insertion path, and nobody inserts

`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1101`:

```rust
pub fn note_authenticated_introducer(
    &self,
    peer_hash: [u8; 32],
    endpoint: Ssu2Endpoint,
    intro_key: i2pr_transport_ssu2::address::IntroKey,
    relay_tag: u32,
    expires_secs: u64,
    provenance: IntroducerProvenance,
) -> Result<(), i2pr_transport_ssu2::IntroducerError> {
```

It builds an `IntroducerRecord` — which refuses a **zero** tag
(`crates/i2pr-transport-ssu2/src/introducer.rs:80`) — and inserts it into the
table. The table is bounded and its ceiling is a refusal, not an eviction.

### 3.4 The router cannot tell that a peer is an introducer

`crates/i2pr-transport-ssu2/src/address.rs:472`:

```rust
pub fn supports_relay(&self) -> bool {
    self.bits & Self::RELAY != 0
}
```

The `C` capability. It has **no production caller** — the only calls are the
transport's own unit tests (`address.rs:1443`). `Ssu2Capabilities::parse` has
exactly two production callers, and both are about *this* router's own
publication (`ssu2_runtime.rs:2392`, `:2514`).

So the router cannot answer the question the reference answers. The pinned
i2pd 2.61.0 answers it from the peer's **advertised capabilities**
(`libi2pd/RouterInfo.h:184`):

```cpp
bool IsIntroducer () const { return caps & eSSUIntroducer; };
```

### 3.5 The reference's whole chain

The pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`):

- the initiator asks for a tag when **it is firewalled** and **the peer
  advertises the introducer capability**
  (`libi2pd/SSU2Session.cpp:753`):

  ```cpp
  if (GetRouterStatus () == eRouterStatusFirewalled && m_Address->IsIntroducer ())
  ```

  and sends an **empty** RelayTagRequest block
  (`libi2pd/SSU2Session.cpp:759` — `eSSU2BlkRelayTagRequest`, size 0);

- the responder mints a tag **only if it is itself an introducer**, and binds
  the tag to the session (`libi2pd/SSU2Session.cpp:1878-1883`):

  ```cpp
  auto addr = FindLocalAddress ();
  if (addr && addr->IsIntroducer ())
  {
      RAND_bytes ((uint8_t *)&m_RelayTag, 4);
      m_Server.AddRelay (m_RelayTag, shared_from_this ());
  }
  ```

- the initiator stores the tag (`libi2pd/SSU2Session.cpp:1889`):

  ```cpp
  case eSSU2BlkRelayTag:
      LogPrint (eLogDebug, "SSU2: RelayTag");
      m_RelayTag = bufbe32toh (buf + offset);
  ```

- and the initiator **drops** the tag when the peer's later RouterInfo stops
  advertising the capability (`libi2pd/SSU2Session.cpp:1990-1991`):

  ```cpp
  if (IsOutgoing () && m_RelayTag && !address->IsIntroducer ())
      m_RelayTag = 0; // not longer introducer
  ```

**Java I2P 2.13.0** (`9134f808337b401e8e53c73734c81fab04280c9d`) reads the
same marker — `addr.getOption("itag0")`
(`router/java/src/net/i2p/router/transport/udp/UDPTransport.java:2335`) — and
the tree's `C` capability is the same role the reference calls
`eSSUIntroducer` (`libi2pd/RouterInfo.h:125`).

### 3.6 The peer-relay service knows the router's own reachability

`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1036`:

```rust
pub fn reachability_state(&self) -> ReachabilityState {
```

The reference's condition is *"I am firewalled"*, and this tree already knows
whether it is.

## 4. Invariants that must not regress

1. **The proto crate is untouched.** No new I2NP message, no new SSU2 block
   type, no new wire format, no change to `i2pr-proto`.
2. **The introducer role owner is untouched.** This plan changes what the
   *router* does with a `Tag` event it receives; it does not change how the
   role owner *serves* a peer, and the role owner's refusal of a `Tag` event
   stands.
3. **The router's published RouterInfo content does not change.** This plan
   publishes no introducer set and advertises no `C`; `router.version` stays
   `0.9.62`; the published record's `caps` string is unchanged; the inventory's
   advertised count stays **zero**.
4. **A tag is recorded only from an authenticated session, and only in
   answer to a request this router made.** The peer must have a **live
   session**, and the peer must be one the router **asked**. A tag from any
   other source is refused, not recorded. **Corrected by §17.2.**
5. **The capability read is authenticated.** The peer RouterInfo the plan reads
   is the one the **netdb validated on ingest** — signature, identity hash,
   static-key binding and freshness all checked — and the plan re-uses that
   validated record rather than parsing unvalidated bytes. **Corrected by
   §17.2.**
6. **A malformed or absent capability is refused, never defaulted.** A peer
   whose address carries no `caps`, an unparsable `caps`, or a `caps` without
   the introducer letter is **not** an introducer.
7. **The transport and the netdb stay runtime-neutral.** No `tokio::*`, no
   `std::net`, no `std::fs` in `i2pr-transport-ssu2` or `i2pr-netdb`.
8. **Every refusal is typed and countable.** A refusal to ask, a refusal to
   record, and a refused tag are each counted under their own reason.
9. **No new configuration key.** The plan changes no config surface, or
   exactly one, and the plan says which.
10. **The reference pins are untouched.** i2pd `635b013a…`, Java
    `9134f808…`; no external router or client is patched, vendored or
    modified.
11. **No boundary script is weakened.** No `|| true`, no
    `continue-on-error`, no filename filtering, no early-return-success, and
    no production wire change made to go green.
12. **The request is bounded.** The router asks at most one peer per cadence
    window, and never asks a peer it has no session with.

## 5. Scope

### 5.1 In scope

- The runtime's ability to answer **"does this peer advertise the introducer
  capability?"** for a peer it holds a session with.
- The daemon's **request policy**: when to ask, which peer to ask, how often.
- The runtime's **recording** of the tag that arrives, as an authenticated
  introducer.
- The counters that make each outcome visible.
- The lane, the guard, the documentation and the support inventory.

### 5.2 Out of scope

- **The publication.** Building the introducer-only address from the table,
  choosing it over the direct form, and signing it. `Ssu2RuntimeService::introducer_publication_material`
  (`ssu2_runtime.rs:2384`) and the netdb's
  `build_with_ssu2_introducer_address` stay uncalled by production after this
  plan; that is a successor's subject, and Plan 523's whole point was that the
  form must be right before it is used.
- **Advertising `C`.** This router's own published `caps` does not gain the
  introducer letter. The plan **reads** the capability; it does not **publish**
  it. Publishing it is an advertisement with its own evidence burden and its
  own default-off semantics, and it is a separate plan.
- **The public dial scope.** `Ssu2DialTarget::new` remains loopback-only and
  `new_private_lan` remains RFC1918-IPv4-only; that bound is Plan 431's.
- **Plan 433's scope** and **Plan 494's `caps=f`** are untouched.
- **The stock-peer lane.** Blocked on Plan 493's authorized two-host topology.
- **The role owner's refusal of a `Tag` event.** It is correct as it stands.
  This plan routes the event to its own destination; it does not make the role
  owner serve it.

## 6. Required production changes

1. **`crates/i2pr-transport-ssu2/`** — **nothing.** The transport is
   untouched by this plan. **Corrected by §17.2.**
2. **`crates/i2pr-daemon/src/`** — the daemon reads the peer's advertised
   capability from the authoritative store, drives the request on its own tick
   under a bounded policy, and publishes the counters to the inspection plane.
3. **`crates/i2pr-runtime/src/ssu2_runtime.rs`** — the runtime keeps the
   **bounded** set of outstanding relay-tag requests, and records the tag that
   arrives as an authenticated introducer.
4. **`crates/i2pr-runtime/src/ssu2_peer_relay.rs`** — the insertion path
   gains the production caller; the table and its ceiling are unchanged.

## 7. Design decisions the plan must make explicit

**D1 — The peer is an introducer when it advertises the `C` capability.**
The reference's own test is `caps & eSSUIntroducer`
(`libi2pd/RouterInfo.h:184`), and the tree's `Ssu2Capabilities::supports_relay`
is that test. The plan does **not** use the peer's *published introducer set*
(`UsesIntroducer()`, `libi2pd/RouterInfo.h:182`) as the test: a peer that
publishes introducers is a peer that *has* introducers, not a peer that is
willing to *be* one.

**D2 — The capability is read from the store's validated peer record.**
*(**Corrected by §17.2.** The registered text read it from the transport's
validated peer record, which is not implementable for an outbound session.)*
The daemon reads the peer's validated RouterInfo from the authoritative
store (`Bootstrap::store()` → `RouterInfoStore::get`), takes the peer's SSU2
address, and parses it with the transport's own strict parser,
`Ssu2RouterAddress::parse` (`crates/i2pr-transport-ssu2/src/address.rs:570`),
whose `capabilities()` (`:661`) answers the `C` question with the transport's
own vocabulary. The bytes are the ones the netdb validated on ingest; the
plan does not parse unvalidated bytes.

**D3 — The router asks only when it is firewalled.** The reference's condition
is `eRouterStatusFirewalled && m_Address->IsIntroducer()`
(`libi2pd/SSU2Session.cpp:753`). A reachable router does not need an
introducer, so it does not ask. The tree knows its own reachability
(`ssu2_peer_relay.rs:1036`).

**D4 — The router asks a peer it already has a session with.** The request
travels in-session; there is no new connection to make. The peer is chosen
from the peers the runtime already holds a session with
(`ssu2_runtime.rs:1833`), exactly as the router's own peer test chooses
(`crates/i2pr-daemon/src/service_product.rs:4709`).

**D5 — The tag is recorded only for a peer this router asked.**
*(**Corrected by §17.2.** The registered text checked the capability at the
moment the tag arrived, which the runtime cannot do for an outbound peer.)*
The runtime keeps a **bounded** set of outstanding relay-tag requests, and a
`SessionEvent::RelayTag` for a peer that is not in that set is **refused**: a
peer may send the block unsolicited, and recording an introducer nobody asked
for would put an unauthenticated peer into the router's introducer set. The
reference drops a tag when the peer's RouterInfo stops advertising the
capability (`libi2pd/SSU2Session.cpp:1990-1991`); this plan applies that
check **at the request**, in the daemon, from the store, and the check at
**publication** time — where staleness bites — belongs to the publication
successor.

**D6 — The record's `endpoint` and `intro_key` are this router's own.**
*(**Corrected by §17.2.** The registered text supplied the *peer's* own
introduction key, which the runtime cannot read for an outbound peer.)* The
SSU 1 shape published the **target's** contact material — the target's host,
port and introduction key — so that the introducer could reach the target;
the record's `endpoint` and `intro_key` are that material, and they are
**this router's own**: its bound endpoint and its own introduction key, both
of which the runtime already holds. The SSU2 published form carries neither
(Plan 523), so neither is load-bearing for this plan.

**D7 — The record's expiry is bounded and derived from the session's own
evidence.** The tag does not carry an expiry. The plan derives one from the
runtime's own clock and the table's own retention, and the record's
provenance is `IntroducerProvenance::AuthenticatedDirect`
(`crates/i2pr-transport-ssu2/src/introducer.rs:43`) — the session that carried
the tag *is* the authentication.

**D8 — A `Tag` event no longer reaches the role owner's refusal path.** The
event is consumed by the runtime's recording path before it reaches the
handoff, so a legitimate event is no longer counted as a refusal. The role
owner's refusal stands for any `Tag` event that still reaches it.

**D9 — The plan adds one configuration key, and it is default-off.**
Something like `ssu2.introducer_request`. A router whose operator has not
asked for it asks nobody. The plan says so in the acceptance criteria and the
guard proves the key exists and is default-off.

## 8. Work packages

### WP1 — the daemon reads the peer's advertised capability

The daemon reads the peer's validated RouterInfo from the authoritative store,
takes its SSU2 address, and parses it with the transport's own strict parser.
A peer with no stored record, an unparsable address, or an address without
the introducer letter is **not** an introducer, and each is a counted refusal
rather than an absence. **Corrected by §17.2:** the registered WP1 put this
read in the transport, which cannot answer it for an outbound session.

### WP2 — the runtime tracks the outstanding requests

The runtime keeps the **bounded** set of peers this router asked for a relay
tag, so that a tag arriving for a peer nobody asked about is refused rather
than recorded.

### WP3 — the daemon asks a peer for a tag

The daemon drives the request on its own tick: the router must be firewalled,
the opt-in must be on, a peer that advertises the capability must be found
among the peers with a live session, and at most one request goes out per
cadence window. Every refusal is counted under its own reason.

### WP4 — the runtime records the tag

The `SessionEvent::RelayTag` arm records an authenticated introducer, and the
`Tag` event no longer reaches the role owner's refusal path.

### WP5 — the lane, the guard, the documentation and the inventory

The lane drives the real runtime and the real daemon over real sessions. The
guard pins the shape of the plan. The documentation and the support inventory
record the new surface.

## 9. Failure, cancellation, restart, and contention semantics

- **Failure.** A refusal to ask, a refusal to record and a refused tag are
  each counted under their own reason, and none leaves state behind. A
  poisoned state mutex degrades to "no introducer", never to a fabricated one.
- **Cancellation.** The plan adds no task of its own; the request rides the
  daemon's existing tick and the daemon's existing product loop, and the
  runtime's queueing is synchronous and non-awaiting
  (`ssu2_runtime.rs:2225`). There is nothing to cancel and nothing to await.
- **Restart.** The introducer table is in-memory and is lost on restart, the
  same as every other session-scoped table in the tree. The plan does not add
  persistence: a tag learned from a session is meaningless after the session
  is gone.
- **Contention.** The request is bounded to one peer per cadence window, so a
  busy router does not ask every peer it knows. The table's own ceiling
  refuses rather than evicts.

## 10. Compatibility and migration review

- **No wire change.** The request block is the one the transport already
  builds, and the answer is the one the transport already parses. The proto
  crate and the block vocabulary are untouched.
- **A reading change, not a writing change.** The router reads a peer's
  advertised capability it did not read before. Nothing the router publishes
  changes.
- **The role owner's behaviour is unchanged** for every event it already
  served.
- **No configuration migration.** The one new key is default-off and has no
  meaning to an existing configuration.

## 11. Required tests

### The daemon's capability read

- a stored peer whose SSU2 address advertises `C` answers the capability
  question affirmatively;
- one that does not, answers negatively;
- a peer with no stored record answers negatively, and is counted;
- an address with no `caps`, or with an unparsable `caps`, answers negatively.

### The runtime's outstanding-request set

- a peer this router asked for a tag is in the set;
- a peer it did not ask is not;
- the set is bounded, and a peer beyond the bound is refused and counted.

### The daemon's request policy

- the opt-in is off by default and asks nobody;
- a firewalled router with an eligible peer asks exactly one peer per window;
- a reachable router asks nobody;
- a router with no eligible peer asks nobody, and the refusal is counted;
- the router never asks a peer it has no session with.

### The runtime's recording

- a tag arriving on an authenticated session, in answer to a request this
  router made, is recorded as an authenticated introducer;
- the recorded introducer is observable in the table;
- a zero tag is refused;
- a tag arriving for a peer this router **did not** ask yields **no** record;
- the table's ceiling refuses rather than evicts;
- the `Tag` event no longer reaches the role owner's refusal path.

### The negative controls

Every guard rule needs an inverted probe that fires it, and every probe
mutates an **in-memory copy** of the source, so no file on disk is written.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_introducer_request -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_introducer_request_boundary -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
```

## 13. Documentation updates

- `specs/support.toml` — a new surface for the acquisition, `experimental` and
  `advertised = false`.
- `specs/CONFORMANCE.md` — the section for this plan and the inventory counts.
- `specs/protocols/09-ssu2.md` — the acquisition scope.
- `specs/QUALIFICATION-GATES.md` — the gates for this plan.
- `docs/architecture/i2pr-transport-ssu2.md` — the peer record's capabilities.
- `docs/architecture/i2pr-runtime.md` and `docs/architecture/i2pr-daemon.md` —
  the request policy and the recording.

## 14. Acceptance criteria and stop conditions

The plan passes when:

1. the runtime can answer, for a peer it holds a session with, whether that
   peer advertises the introducer capability, and the answer is the
   transport's **validated** one;
2. a router with the opt-in on, firewalled, and holding a session with an
   eligible peer asks exactly one peer per cadence window;
3. a tag arriving on an authenticated session is recorded as an authenticated
   introducer and is observable in the table;
4. a peer that does not advertise the capability, a zero tag, a peer with no
   usable introduction key and a full table are each refused and counted;
5. the `Tag` event no longer reaches the role owner's refusal path;
6. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, and no file on disk is written by a probe;
7. the routine floor is green on the closing tree;
8. `specs/support.toml` records the surface as `experimental` and
   `advertised = false`; the advertised count stays **zero**;
9. `router.version` stays `0.9.62`; the published RouterInfo's `caps` is
   unchanged; `Cargo.toml` and `Cargo.lock` are byte-identical;
10. no boundary script is weakened.

**Stop conditions.** The plan stops and is re-scoped, rather than widened, if:

- reading the peer's advertised capability turns out to require re-parsing
  unvalidated bytes, or to weaken the transport's validation;
- the request policy turns out to need a new connection, a new task or a new
  listener;
- recording a tag turns out to require publishing anything;
- the peer's advertised capability turns out not to be the reference's own
  test, in which case the disagreement is recorded and the plan is re-derived
  against the specification, not against the reference.

## 15. Closure evidence required

- The commits that carry each work package.
- A requirement-to-evidence matrix: every acceptance criterion above, the
  file and line that satisfies it, and the test that proves it.
- The exact commands run, with their outcomes.
- The invariant, failure, migration and security reviews of §4, §9 and §10.
- The negative controls: every mutation applied, the test it broke, and the
  proof that no file on disk was written.
- The findings by severity, each either fixed or recorded with its owner.
- The roadmap disposition and the unblock audit.

## 16. Handoff notes

- The plan's subject is the **acquisition**. The **publication** — building
  the introducer-only address, choosing it over the direct form, and signing
  it — is a successor's subject, and the closure record must say so plainly
  so that a successor does not mistake the two.
- The `C` capability this router publishes about **itself** is a third
  matter, and is not this plan's.
- Plan 523's closure record states that the router still never asks a peer
  for a relay tag, never records an introducer and never publishes an
  introducer set. This plan discharges the **first two** of those. The closed
  record is **not** rewritten; the correction lives in this plan's closure
  record and in this plan's registry row.

## 17. Amendment log

### 17.2 — 2026-11-09, the capability read moves to the daemon

**The defect in the registered design.** §2 and D2 put the capability read in
the transport's validated peer record. The transport's `validate_router_info`
(`crates/i2pr-transport-ssu2/src/handshake.rs:924`) is called on exactly one
path — the **responder's** SessionConfirmed
(`crates/i2pr-transport-ssu2/src/state_machine.rs:1446`) — because it is the
**initiator** that carries a RouterInfo in its SessionConfirmed. The
initiator's own peer record is built with an **empty** RouterInfo
(`state_machine.rs:728-732` — `router_info: Vec::new()`), so an **outbound**
session never carries the responder's RouterInfo at all.

That is the direction this plan's request travels: the router dials a peer and
asks it. So the registered design could not answer the question for the
peers it needed to answer it for, and the runtime's session record — which
the registered §2 called "each session's validated peer RouterInfo" — is
empty on exactly the sessions that matter.

**The correction.** The capability read belongs to the **daemon**, which is
the tree's authority for peer records. The daemon reads the peer's validated
RouterInfo from the authoritative store — `Bootstrap::store()`
(`crates/i2pr-daemon/src/bootstrap.rs:261`) → `RouterInfoStore::get`
(`crates/i2pr-netdb/src/store.rs:177`) — takes the peer's SSU2 address, and
parses it with the transport's own strict parser, `Ssu2RouterAddress::parse`
(`crates/i2pr-transport-ssu2/src/address.rs:570`), whose `capabilities()`
(`:661`) answers the `C` question in the transport's own vocabulary. This is
the same authority and the same pattern the product already uses for a peer's
public static encryption key (`crates/i2pr-daemon/src/service_product.rs:4122`).

**What changed.**

- **D2 is replaced** — the read is from the store's validated record, not
  from the transport's peer record.
- **D5 is replaced** — the runtime cannot re-check the capability when the tag
  arrives, so the gate becomes **"a peer this router asked"**. The runtime
  keeps a bounded set of outstanding relay-tag requests, and a tag for a peer
  outside that set is refused. The reference's own staleness rule
  (`libi2pd/SSU2Session.cpp:1990-1991`) is applied at the **request**, and the
  check at **publication** time belongs to the publication successor.
- **D6 is replaced** — the record's `endpoint` and `intro_key` are **this
  router's own**, because the SSU 1 shape published the *target's* contact
  material and the runtime holds both facts without any peer lookup.
- **The transport is untouched.** The plan adds no transport surface at all.
  An earlier draft put the capability on the transport's validated peer
  record; the correction above is why it does not.

**What did not change.** The objective, the scope, the invariants' intent, the
acceptance criteria, the out-of-scope list, and the design decisions D1, D3,
D4, D7, D8 and D9 all stand as registered. No production code had been
written against the registered design when the correction was made, so nothing
was rewritten after the fact.

**The new evidence.** The asymmetry is the evidence for this amendment, and it
is in the tree:

| Site | What it shows |
|---|---|
| `crates/i2pr-transport-ssu2/src/handshake.rs:924` | `validate_router_info` — the transport's full validation |
| `crates/i2pr-transport-ssu2/src/state_machine.rs:1446` | the **only** call site — the responder's SessionConfirmed |
| `crates/i2pr-transport-ssu2/src/state_machine.rs:728-732` | the initiator's peer record, with `router_info: Vec::new()` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:1188` | the session's peer RouterInfo field — empty on an outbound session |
| `crates/i2pr-daemon/src/bootstrap.rs:261` | `Bootstrap::store()` — the authoritative store |
| `crates/i2pr-netdb/src/store.rs:177` | `RouterInfoStore::get` — the validated lookup |
| `crates/i2pr-daemon/src/service_product.rs:4122` | the product's existing use of that lookup |
| `crates/i2pr-transport-ssu2/src/address.rs:570` | `Ssu2RouterAddress::parse` — the strict address parser |
| `crates/i2pr-transport-ssu2/src/address.rs:661` | `capabilities()` — the `C` question |

### 17.1 — 2026-11-09, registration

Plan 524 is registered as the **acquisition** half of the SSU2 introducer
composition: the runtime learns a peer's advertised introducer capability from
the transport's validated peer record, the daemon asks such a peer for a relay
tag when the router is firewalled, and the runtime records the tag that
arrives as an authenticated introducer. The publication and the `C`
advertisement are out of scope.

The citations in §3 were verified against the tree at registration, against
the pinned i2pd checkout at `635b013a612ff47278ef02acf8580a28e10e26c5`,
against the pinned Java I2P checkout at
`9134f808337b401e8e53c73734c81fab04280c9d`, and against the SSU2
specification at <https://i2p.net/en/docs/specs/ssu2/>.
