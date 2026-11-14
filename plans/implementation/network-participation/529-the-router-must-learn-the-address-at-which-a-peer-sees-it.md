# Plan 529 — the router must learn the address at which a peer sees it

**Status:** registered, 2026-11-10. The next free plan number after 528.

The SSU2 specification's Address block carries **"Alice's address, sent to
Alice by Bob, or Bob's address, sent to Bob by Alice"** — the address at
which the *recipient* sees the *sender*. The tree sends the block with the
**wrong value** and then throws the answer away on the receiving side, so
the router never learns the address at which a peer sees it, and the
corroboration class that depends on that fact is unreachable in the field.

## 1. Objective

Make the router learn the address at which an authenticated peer sees it,
and make it compare that address with the router's **own bound socket** —
through the seam Plan 527 already built.

Concretely:

1. the responder must put the **initiator's** observed source address in the
   Session Created's Address block, not its own;
2. the initiator must **read** the Address block out of the decrypted
   Session Created payload and deliver it as a typed observation;
3. the runtime must route that observation into Plan 527's comparison, so
   an agreement corroborates and a disagreement contradicts under Plan 528's
   model.

## 2. Why ready

- The **sender** side needs no new machinery: all three responder entry
  points already take the datagram's `source: SocketAddr`.
- The **receiver** side needs no new machinery either: the runtime already
  handles `SessionEvent::AddressObserved` and already compares it with the
  bound socket (Plan 527).
- The comparison, the policy, the firewalled verdict and the gate are all
  in place (Plans 527 and 528).
- The gap is a **value** that is wrong and an observation that is dropped,
  not a missing capability.

## 3. Current implementation evidence

### 3.1 The responder sends the wrong address

`crates/i2pr-transport-ssu2/src/state_machine.rs:1138` builds the Session
Created's payload:

```rust
        let mut blocks = Vec::with_capacity(3);
        blocks.push(PayloadBlock::Timestamp(Ts::new(params.timestamp)));
        blocks.push(PayloadBlock::Address(self.config.local_address));
```

and `self.config.local_address` is the **responder's own** bound address.
The responder's config is built at
`crates/i2pr-runtime/src/ssu2_runtime.rs:5635`-`:5645`:

```rust
    fn responder_config(&self, local_addr: SocketAddr) -> Option<ResponderConfig> {
        let endpoint = Ssu2Endpoint::from_socket_addr(local_addr).ok()?;
        Some(ResponderConfig {
            ...
            local_address: AddressBlock::new(endpoint),
        })
    }
```

So Bob tells Alice *"your address is 127.0.0.1:<Bob's port>"* — Bob's own
address, not Alice's.

The same value is in the **Retry** message, at
`crates/i2pr-transport-ssu2/src/state_machine.rs:992` and `:1251`.

The specification says the opposite. Its Address block (type 13) is
defined as: "2 byte port and 4 or 16 byte IP address. **Alice's address,
sent to Alice by Bob, or Bob's address, sent to Bob by Alice.**" The pinned
reference does the same: `libi2pd/SSU2Session.cpp:999` puts
`m_RemoteEndpoint` — the **initiator's** address as Bob sees it — in the
Session Created.

### 3.2 The initiator throws the answer away

The initiator decrypts the Session Created's payload at
`crates/i2pr-transport-ssu2/src/state_machine.rs:668`:

```rust
        let (transcript, created_payload) = match transcript.accept_session_created(
```

and uses it for exactly one thing, at `:736`:

```rust
        let peer_new_token = find_establishment_token(&created_payload)
```

The Address block inside that payload — Bob's authenticated observation of
the address at which Bob sees Alice — is **never read**.

The reference reads it in the very same message:
`libi2pd/SSU2Session.cpp:1110` calls
`HandlePayload (decryptedPayload.data (), decryptedPayload.size ());` on the
decrypted Session Created payload, and `HandleAddress`
(`libi2pd/SSU2Session.cpp:2067`) logs **"SSU2: Our external address is "**
and calls `i2p::context.UpdateAddress (ep.address ())` (`:2075`), then
compares the observed **port** with the router's own port to decide whether
it is behind a NAT (`:2078`-`:2103`).

### 3.3 The Retry's Address block is discarded too

`crates/i2pr-transport-ssu2/src/handshake.rs:536`:

```rust
            DecodedBlock::Address(_) => {
                seen_address = true;
            }
```

The presence is checked and the value is dropped.

### 3.4 The data-phase Address block *is* kept — so the seam exists

`crates/i2pr-transport-ssu2/src/session.rs:1936` keeps the endpoint
(Plan 527's change), and the runtime compares it at
`crates/i2pr-runtime/src/ssu2_runtime.rs:6962`:

```rust
                SessionEvent::AddressObserved { observed } => {
                    self.note_observed_external_address_locked(state, observed);
                }
```

⇒ **The receiver's seam exists and is proven. The handshake never reaches
it.**

### 3.5 The consequence: a corroboration class is unreachable

`AuthenticatedPeerObservedExternalAddress` is corroboration **class 1**
(`crates/i2pr-transport/src/reachability.rs:226`-`:228`). `Reachable`
needs three distinct supporting classes, and Plan 526's closure record
already recorded the open limitation:

> the runtime's **direct** form still needs three corroboration classes and
> no lane in this tree earns them.

⇒ **Class 1 is one of the three, and the tree has no producer for it
outside the controlled profile.** The controlled lane earns it by
hand-queuing the block
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3527`), which is not a field
producer. Plans 527 and 528 made the *negative* side reachable; this plan
makes a **positive** class reachable.

### 3.6 The initiator's own Address block is absent

The specification also has Alice tell Bob the address at which she sees
him ("Bob's address, sent to Bob by Alice"), in the Session Request's
payload. The tree's Session Request carries **no** Address block at all
(`PayloadBlock::Address` appears once in the transport, at
`state_machine.rs:1138`, in the Session Created).

⇒ The reference reads the Address block in **both** directions
(`libi2pd/SSU2Session.cpp:934` for the responder reading the Session
Request's payload, `:1110` for the initiator reading the Session Created's).
The tree reads it in neither.

## 4. Invariants

1. The Address block carries **the address at which the recipient sees the
   sender**, never the sender's own address.
2. The responder's value is the **observed source address** of the request
   it answers, never a configured or bound value.
3. The initiator's value is the address at which the responder sees it, and
   it is **authenticated** by the Noise transcript.
4. The observation is compared with the router's **own bound socket**,
   through Plan 527's seam — never with a configured value.
5. An agreement corroborates and a disagreement contradicts; one
   observation is one fact.
6. The comparison's refusal on a family with no bound socket is unchanged.
7. The **corroboration** floor is unchanged; `Reachable` still needs three
   distinct supporting classes.
8. The transport's reachability **model** — the signal kinds, the
   corroboration classes and the state machine — is untouched.
9. No configuration key is added and no default moves.
10. No wire format is added or changed: the block type, its length and its
    placement are exactly what the tree already sends.
11. No new task, no new timer and no new listener.
12. Nothing is promoted: `router.version` stays `0.9.62` and the inventory's
    advertised count stays **zero**.

## 5. Scope

### 5.1 In scope

- The responder's Session Created Address block: the initiator's observed
  source address.
- The responder's Retry Address block: the same observed source address.
- The initiator's read of the Session Created's Address block, and its
  delivery as a typed observation.
- The runtime's routing of that observation into Plan 527's comparison.
- The initiator's **own** Address block in the Session Request, carrying the
  address at which Alice sees Bob, and the responder's read of it.

### 5.2 Out of scope

- The **data phase**'s Address block. The tree already keeps it (Plan 527)
  and the reference does **not** send it in ordinary data packets either;
  the reference's data-phase sites are the Path Challenge and the Path
  Response.
- The **production SSU2 peer-test driver**, the **requester side**, the
  **`B` advertisement**, the **`caps=f` advertisement** and the
  **stock-peer** half. Their owners are named in the plan that owns them.

### 5.3 Out of scope, with the owner named

- **The production SSU2 peer-test driver** — Plan 528's §5.3 names the
  successor in this subsystem, and the choreography question is still open:
  the tree's Alice role requires the helper's identity up front, while the
  specification has Bob select the helper.
- **The requester side of the SSU2 relay protocol** — Plan 527's §5.3.
- **A public dial scope** — Plan 431 owns it.
- **The `B` advertisement and `caps=f`** — their own owners.

## 6. Required production changes

1. `crates/i2pr-transport-ssu2/src/state_machine.rs` — the responder's
   Address block carries the **observed source address** of the request it
   answers; `ResponderConfig::local_address` is no longer the source of
   that value.
2. `crates/i2pr-transport-ssu2/src/state_machine.rs` — the initiator reads
   the Address block out of the decrypted Session Created payload and
   carries it on the established session.
3. `crates/i2pr-transport-ssu2/src/handshake.rs` — the initiator builds the
   Session Request's Address block (the address at which it sees the
   responder), and the responder reads it.
4. `crates/i2pr-runtime/src/ssu2_runtime.rs` — the runtime routes the
   observed address into `note_observed_external_address_locked`, Plan 527's
   seam.

## 7. Design decisions

- **D1 — The value is the observed source address.** The responder knows
  the initiator's source address from the datagram's own header. It is
  authenticated by the Noise transcript, because the Session Created is
  sealed under keys derived from the handshake that carried that datagram.
- **D2 — The observation is delivered on the established session.** The
  Session Created's payload is read exactly once, at establishment, so the
  observation rides the same value that already carries the session's keys
  and the peer's identity. No new event type and no new queue.
- **D3 — The comparison is Plan 527's, unchanged.** The runtime routes the
  observation into `note_observed_external_address_locked`, which compares
  it with the **bound** socket and refuses a family with no bound socket.
- **D4 — The contradiction is Plan 528's, unchanged.** A disagreement
  against the router's own bound socket is the router's own **firewalled
  verdict**, and the corroboration floor is untouched.
- **D5 — The direction is symmetric.** The tree sends Alice's view of Bob
  in the Session Request and reads Bob's view of Alice in the Session
  Created, because the specification's block is symmetric and the
  reference reads it in both directions.
- **D6 — The Retry's Address block carries the same value.** The Retry is
  sent by the same responder for the same request, so it must not carry a
  different address. The Retry is **not** used as a reachability
  observation: it is sealed outside the Noise transcript, so it is a weaker
  fact, and the tree's `parse_retry` keeps discarding it.
- **D7 — No wire change.** The block type (13), its length (6 or 18) and
  its placement are exactly what the tree already sends; only the value
  changes, and it changes to what the specification says it must be.

## 8. Work packages

- **WP1 — the transport's sender side.** The responder's Address block
  carries the observed source address; the Session Request gains Alice's
  view of Bob.
- **WP2 — the transport's receiver side.** The initiator reads the
  Session Created's Address block; the responder reads the Session
  Request's.
- **WP3 — the runtime's seam.** The observation reaches Plan 527's
  comparison.
- **WP4 — the lane, the guard, the documentation and the inventory.**

## 9. Failure, cancellation, restart and contention semantics

- **Failure.** A missing or malformed Address block leaves the session
  **established** and records **nothing**: the observation is evidence, not
  a handshake requirement. The specification makes the block optional in
  the data phase, and the tree must not fail a handshake over it.
- **Cancellation.** The plan adds **no task**. The observation is read
  synchronously at establishment.
- **Restart.** Nothing is persisted: the observation lives in the live
  reachability tracker, exactly as Plan 527's.
- **Contention.** The comparison happens under the runtime's existing state
  lock, in the same order as Plan 527's, and no new lock is introduced.

## 10. Compatibility and migration

- **No wire format change.** The same block, the same type, the same
  length. Only the **value** changes, to the value the specification and
  the reference already use.
- **A reading change for the router.** A router that is behind a NAT now
  learns its external address from a peer, and its reachability tracker
  receives the observation. An agreement corroborates; a disagreement is
  the router's own firewalled verdict (Plan 528's model).
- **A removed field.** `ResponderConfig::local_address` goes away with the
  plan's change; no external crate used it.
- **No configuration migration.** No key is added, renamed or removed.

## 11. Required tests

### 11.1 The lane

- the responder's Session Created carries the **initiator's** address, not
  its own;
- the initiator **reads** it and the runtime compares it with the bound
  socket;
- an **agreement** corroborates;
- a **disagreement** is the router's own firewalled verdict;
- a session with **no** Address block is still established and records
  nothing;
- the Session Request carries Alice's view of Bob, and the responder reads
  it;
- the Retry's Address block carries the same observed address;
- the corroboration floor is unchanged.

### 11.2 The guard

- the responder's Address block carries the observed source address;
- the initiator reads the Address block out of the Session Created payload;
- the observation reaches Plan 527's comparison;
- the transport's reachability model is untouched;
- no new key and no new infrastructure;
- the Retry's Address block is still not a reachability observation.

Every guard rule needs an inverted probe that fires it, and every rule fails
closed when the item it names is absent.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --test ssu2_observed_address -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_observed_address_boundary -- --test-threads=1
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
- `docs/architecture/i2pr-transport-ssu2.md`,
  `docs/architecture/i2pr-runtime.md` and `docs/architecture/i2pr-daemon.md`
  — one section each.

## 14. Acceptance criteria and stop conditions

The plan passes when:

1. the responder's Address block carries the **initiator's** observed
   source address;
2. the initiator **reads** it and the runtime compares it with the router's
   own bound socket;
3. an agreement corroborates and a disagreement is the router's own
   firewalled verdict;
4. the corroboration floor is unchanged;
5. the transport's reachability model is untouched;
6. no configuration key is added and no default moves;
7. no wire format is added or changed;
8. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, and every rule fails closed on an absent item;
9. the routine floor is green on the closing tree;
10. `specs/support.toml` records the surface as `experimental` and
    `advertised = false`, the advertised count stays **zero**,
    `router.version` stays `0.9.62`, `Cargo.toml` and `Cargo.lock` stay
    byte-identical, and no boundary script is weakened.

**Stop conditions.** The plan stops and is re-scoped, rather than widened,
if:

- the observation turns out to need a change to the transport's reachability
  **model**;
- the comparison turns out to need a configuration key or a second policy;
- the delivery turns out to need a new task, a new timer or a new listener;
- the plan turns out to need the production peer-test driver, or the
  requester side, after all.

**Known limitations, recorded not hidden.**

- The delivery is confined to the **controlled loopback profile**: the
  comparison is between two loopback addresses, so the observation is a
  **qualification** claim.
- The **production peer-test driver** stays unbuilt, and the **stock-peer
  half** stays blocked on Plan 493's authorized two-host topology.
- The **requester side** stays unwired.

## 15. Closure evidence required

- The commits that carry each work package.
- A requirement-to-evidence matrix: every acceptance criterion above, the
  file and line that satisfies it, and the test that proves it.
- The exact commands run, with their outcomes.
- The invariant, failure, migration and security reviews of §4, §9 and §10.
- The negative controls: every mutation applied, the test it broke, and the
  proof that no file on disk was written.
- The findings by severity, each either fixed or recorded with its owner.
- The correction to Plan 526's and Plan 527's closed claims about the
  direct form's corroboration classes, in this plan's closure record and in
  the registry row — never in the closed record of the plan that made it.
- The roadmap disposition and the unblock audit.

## 16. Handoff notes

- The plan's subject is the router's **own** external address: it must
  learn the address at which a peer sees it.
- The plan corrects a **value** and stops a **discard**. It adds no
  capability of its own beyond the corroboration class it makes reachable.
- The plan is **never** a promotion: the inventory's advertised count stays
  zero and `router.version` stays pinned.
- The **production peer-test driver**, the **requester side**, the **`B`
  advertisement**, the **`caps=f` advertisement** and the **stock-peer
  half** all stay out of scope, with their owners named.

## 17. Amendment log

### 17.1 — 2026-11-10, registration

The plan is registered as **529**, the next free number. §3 records the
evidence as it stood at registration: the wrong value in the responder's
Address block, the discarded observation on the initiator side, the
reference's use of the very same block, and the corroboration class that
therefore has no field producer.

§3 is evidence and is **not** rewritten. Any later correction is added here
as an amendment, additively.

### 17.2 — 2026-11-10, implementation findings

The implementation settled five questions the registration left open. §3 is
**not** rewritten; the corrections live here and in the closure record.

**The responder reads the Session Request's block in a different transition
than the one that establishes the session.** §3.2 and §6.3 said that the
responder reads the block, without saying where. The answer is: the
responder decrypts the Session Request in `on_session_request`, and it
establishes the session in `on_session_confirmed` — two different
transitions, and the payload is not in scope in the second one. The
observation is therefore carried as **per-handshake state**, exactly like
`initiator_conn_id` and `initiator_eph`, and it is read in the transition
that decrypts the request. The initiator has no such problem: it reads the
Session Created's payload in the same transition that establishes the
session.

**The responder's configuration lost its address, and with it its only use
of the local address.** `ResponderConfig::local_address` is gone, so
`responder_config` no longer takes a local address and no longer returns an
`Option`. The `Option` existed only because the local address had to be
expressible as an SSU2 endpoint; with the block built from the request's
source, that constraint no longer applies. The runtime's three responder
entry points keep their **fail-closed** precondition unchanged — a router
with no bound socket address still refuses to answer — but the address is
no longer published.

**`build_retry`'s address became optional, and that is load-bearing.** The
tree's own `parse_retry` **refuses** a Retry with no Address block, so an
inexpressible source must not produce a *wrong* block: `build_retry` takes
`Option<AddressBlock>` and the two Retry sites pass
`observed_address_block(source)`. In practice a source with port 0 never
reaches the responder — the runtime drops it before the responder sees it —
so the Retry always carries the block. The `Option` is the honest
expression of "no block rather than a wrong one", and it is what lets the
plan's failure posture hold.

**A disagreement is unreachable without an address rewrite.** §11.1 asked
for a disagreement row, and the obvious way to make a peer report an address
the router does not own is to put something between them. A plain pair of
loopback routers cannot: the peer always sees the router's bound socket, so
the comparison always agrees. The lane therefore puts a **real UDP relay**
between the two routers — the tree's stand-in for a NAT. Behind it the
disagreement is the router's own firewalled verdict on **both** sides: the
initiator's view of the responder is the relay's address too, so neither
router can claim to be directly reachable.

**The wire-level rows live in the transport's own suite.** §11.1 put the
Session Created, Session Request and Retry rows in "the lane", and §12
named the lane as a daemon test. But the daemon holds **no direct
dependency** on the transport crate and `Cargo.toml` must stay
byte-identical, so a daemon test cannot read the transport's wire. The four
wire-level rows therefore live in
`crates/i2pr-transport-ssu2/tests/handshake.rs`, and the daemon lane
carries the five rows that need the runtime. **No row was dropped**: the
plan's §11.1 list is covered across the two files, and the gate table names
both.

**The observation is recorded only after the session exists.** §6.4 said the
runtime routes the observed address into `note_observed_external_address_locked`.
The implementation captures the value before the authenticated session is
consumed and records it **after** the data-phase session was successfully
constructed, so a corroboration class is never counted for a session that
does not exist. `construct_session` returns `None` when the session cannot be
built, and recording before that check would have counted a class for a
handshake that died.

### 17.3 — 2026-11-10, citation drift

The plan's own production change moved lines in four files, so the citations
in §3 no longer land on the text they quoted. The table is the **additive**
pointer to the current text; every citation not listed there still lands
where it did.

| Citation in §3 | Now | What it points at |
| --- | --- | --- |
| `crates/i2pr-transport-ssu2/src/state_machine.rs:1138` | `:1212` | the Session Created's Address block — the same block, now built from the observed source |
| `crates/i2pr-transport-ssu2/src/state_machine.rs:992` | `:1060` | the Retry's Address block — the same block, now built from the observed source |
| `crates/i2pr-transport-ssu2/src/state_machine.rs:1251` | `:1333` | the second Retry's Address block — the same |
| `crates/i2pr-transport-ssu2/src/state_machine.rs:851` | `:899` | `ResponderConfig` — the same struct, with the `local_address` field **removed** |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:5635` | `:5661` | `responder_config` — the same builder, now without an address and without an `Option` |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:5643` | `:5661`-`:5676` | the responder's configuration — the `local_address` field is **gone** |
| `crates/i2pr-transport-ssu2/src/state_machine.rs:668` | `:649` | `on_session_created` — the same transition |
| `crates/i2pr-transport-ssu2/src/state_machine.rs:736` | `:772` | the NewToken read — unchanged |
| `crates/i2pr-transport-ssu2/src/handshake.rs:536` | `:543` | the Retry parser's Address discard — unchanged |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:6962` | `:6987` | the data-phase `AddressObserved` handler — unchanged |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:6963` | `:6988` | the data-phase comparison call — unchanged |

The citations that still land where they did:
`crates/i2pr-transport-ssu2/src/session.rs:1936`,
`crates/i2pr-runtime/src/ssu2_runtime.rs:3527`,
`crates/i2pr-transport/src/reachability.rs:226`, and the five
`libi2pd/SSU2Session.cpp` citations (`:934`, `:999`, `:1110`, `:2067`,
`:2072`, `:2075`).

### 17.4 — 2026-11-10, the corrections to the closed plans' claims

Two closed records make claims this plan's production change **corrects** or
**discharges**. Neither closed record is rewritten; the corrections live
here, in the closure record and in the registry row.

1. **Plan 161's closure record named this defect and deferred it.**
   `plans/closure/ssu2/161-status.md` records it as a *cosmetic note
   (non-blocking, deferred)*: "our responder SessionCreated Address block
   carries our own endpoint rather than the peer's observed endpoint, so
   i2pd logs `Our external address is 127.0.0.1:44001` at info level; it
   proceeds and establishes. Fixing the observed-address value is follow-up
   work, not a closure blocker." **This plan is that follow-up work.** The
   symptom Plan 161 recorded from the reference's own log line is the same
   line the reference prints when it reads the block
   (`libi2pd/SSU2Session.cpp:2072`), and the reference's log then carried
   the **responder's own** address because the tree had sent it. Plan 161's
   record is not rewritten.

2. **Plan 282's closure record rejected recording handshake Address blocks
   as observations — against the tree's wrong value.**
   `plans/closure/floodfill/282-status.md` records the decision: "Recording
   handshake Address blocks as observations was considered and rejected: the
   peer's self-reported address adds no evidence independent of the dial
   target/source, so counting it would inflate corroboration without new
   facts." That objection was sound **against the tree as it stood**: the
   block carried the **sender's own self-report** — the responder's own
   bound address — so reading it back would have told the initiator nothing
   it did not already know, and counting it would have inflated the
   corroboration count with a fact the initiator already had. Plan 529
   changes the block's **value**, which is exactly what the objection was
   about. The initiator's side is now the **recipient's view of the
   sender** — the address at which the responder sees the initiator — which
   the initiator cannot derive by itself and which is independent of the
   dial target. That is the NAT fact. The responder's side is the
   initiator's view of the responder, which is the dial target; its
   corroboration is therefore weaker, and it is recorded as such: it
   certifies the published address against the bound socket, and its
   **disagreement** is the responder's own firewalled fact. Plan 282's
   record is not rewritten.
