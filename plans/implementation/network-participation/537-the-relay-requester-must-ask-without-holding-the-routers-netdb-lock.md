# Plan 537 — the relay requester must ask without holding the router's NetDB lock

## 1. Objective

The router's **requester** role — the peer that asks an introducer to relay
toward a target it cannot reach directly — must ask **outside** the router's
NetDB lock, and its refusal to ask must never be **skipped silently**.

Two facts about the tree force this. First, the requester's outbound step runs
with the bootstrap's mutex held for the **whole** step: the step scans the
**entire** validated store for candidates, reads up to sixteen candidates'
published introducer sets, resolves an introducer's endpoint, **signs** the
RelayRequest with the router's own key, writes the transport's requester table
and queues the request onto the introducer's session — all of it under the
router's NetDB lock. Second, and worse, the last two of those steps take the
**transport's own mutexes** while the NetDB lock is held, so the router's NetDB
lock **wraps** the runtime's state lock.

Second, the step is **gated on the daemon holding a `Bootstrap`**. A router that
does not hold one does not ask, does not refuse and does not count: the request
that could have been made leaves no trace at all.

The plan makes the requester's reads **short, bounded** reads, moves the
signature and the transport's writes **outside** them, and makes the absence of
a store a **typed, counted refusal** rather than a silent skip.

## 2. Why ready

- The requester role's machinery is **complete**: the owner, the transport's
  requester table, the specification's RelayRequest preimage and its signature,
  the nonce discipline, the session gate and the counters are all in the tree
  and green (`crates/i2pr-daemon/src/relay_requester.rs`).
- The **precedent** for the fix is already in the tree. Plan 536's target role
  reads its material under a short lock and answers outside it
  (`crates/i2pr-daemon/src/relay_target.rs:460`), and Plan 533's establishment
  task reads the same store under a short lock and releases it **before** the
  dial (`crates/i2pr-daemon/src/relay_establishment_task.rs:131`). This plan
  makes the requester follow the pattern its own siblings already follow.
- The defect is **internal**. No stock peer, no second host and no external
  topology is needed to find it, to fix it or to prove it.
- The **owner is named** and the finding is **open**: Plan 536's closure
  record's **Low** finding 3 assigned the requester's outbound step to "the
  requester's line" (`plans/closure/network-participation/536-status.md`).

## 3. Current implementation evidence

### 3.1 The lock is held across the whole outbound step

The composition root's relay tick drives the requester inside the bootstrap's
guard (`crates/i2pr-daemon/src/lib.rs:5658` and
`crates/i2pr-daemon/src/lib.rs:5672`):

```rust
if let Some(bootstrap) = bootstrap.as_ref()
    && let Ok(guard) = bootstrap.lock()
    && let Some(rng) = requester_rng.as_mut()
{
    let decision = crate::relay_requester::drive_relay_request(
        requester,
        guard.store(),
        &live,
        rng,
        local_endpoint,
        now_ms,
        now_secs,
    );
    // ...
}
```

The guard is held for the whole call, and the call does the work.

### 3.2 The step scans the entire store

`drive_relay_request` (`crates/i2pr-daemon/src/relay_requester.rs:1333`) builds
its candidate pool from the **whole** store
(`crates/i2pr-daemon/src/relay_requester.rs:1361`):

```rust
let pool: Vec<PeerId> = store
    .iter()
    .map(|(hash, _)| PeerId::from_hash(*hash.as_hash()))
    .collect();
```

`RouterInfoStore::iter` is a plain `HashMap` iteration
(`crates/i2pr-netdb/src/store.rs:241`), so the scan is O(the router's peer
population). The window on top of it is bounded at
`MAX_RELAY_REQUEST_CANDIDATES = 16`
(`crates/i2pr-daemon/src/relay_requester.rs:60`), but the **scan** is not.

### 3.3 The step signs the request under the lock

The candidates are read under the same guard
(`crates/i2pr-daemon/src/relay_requester.rs:1368` and
`crates/i2pr-daemon/src/relay_requester.rs:1384`), the request is
built at `crates/i2pr-daemon/src/relay_requester.rs:1423`, and the build
**signs** it: `build_request`
(`crates/i2pr-daemon/src/relay_requester.rs:842`) builds the specification's
preimage at `crates/i2pr-daemon/src/relay_requester.rs:850` and calls
`sign_relay_request` at `crates/i2pr-daemon/src/relay_requester.rs:861`. A
signature is a
private-key operation, and it happens under the router's NetDB lock.

### 3.4 The step takes the transport's own mutexes under the lock

`start_request` (`crates/i2pr-daemon/src/relay_requester.rs:881`) calls the
runtime's requester seam at `crates/i2pr-daemon/src/relay_requester.rs:905`
and the runtime's carriage at `crates/i2pr-daemon/src/relay_requester.rs:920`.

- `Ssu2RuntimeService::start_relay_request`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3130`) delegates to the peer-relay
  service, whose requester table is behind its own mutex
  (`crates/i2pr-runtime/src/ssu2_peer_relay.rs:134`) and is locked at
  `crates/i2pr-runtime/src/ssu2_peer_relay.rs:501`.
- `Ssu2RuntimeService::queue_relay_request`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:2670`) reaches
  `queue_relay_block`, which takes the runtime's **state** lock at
  `crates/i2pr-runtime/src/ssu2_runtime.rs:2765`.

So the router's NetDB lock is held while the runtime's requester-table mutex
**and** the runtime's state mutex are acquired. The runtime's own documented
lock order is **sockets → state**
(`crates/i2pr-runtime/src/ssu2_runtime.rs`), and the composition root's own
tick takes the state lock — and releases it — *before* it takes the NetDB lock
(`crates/i2pr-daemon/src/lib.rs:5649` and
`crates/i2pr-daemon/src/lib.rs:5658`). The requester is the one place
where the router's NetDB lock **wraps** the runtime's state lock instead of
sitting beside it.

### 3.5 The same lock is the router's NetDB

The bootstrap's mutex guards the router's only validated `RouterInfoStore`
(`crates/i2pr-daemon/src/bootstrap.rs:215`), and it is taken by the NetDB
discovery tick (`crates/i2pr-daemon/src/lib.rs:2063`), by the relay target's
material read (Plan 536 moved the target's **answer** out of the lock, but
the read itself still takes the mutex), by the publication tick, by the peer
reconcile and by the peer pool.

### 3.6 The consequence

A router that asks for a relay holds the router's NetDB lock across an
O(population) scan, up to sixteen strict address parses, a private-key
signature, and two of the transport's own mutex acquisitions. Every other
NetDB user — the discovery tick, the publication tick, the peer reconcile, the
peer pool, and the target role's own drain — waits for all of it. A router that
holds no bootstrap never asks, and says nothing about it.

## 4. Invariants

1. **The request is produced with no lock held.** Every read the requester
   makes of the router's NetDB is a **short, self-contained** read whose guard
   is released before the read returns, and the signature, the transport's
   requester-table write and the carriage happen with **no** NetDB lock held.
2. **The requester never nests the NetDB lock inside the transport's.** No
   code path may hold the router's NetDB lock while acquiring the runtime's
   requester-table mutex or the runtime's state mutex.
3. **The refusal to ask is never skipped silently.** The outbound step runs
   whenever the requester role exists. A missing store is a **typed, counted
   refusal**, not an absent pass.
4. **The reads stay read-only.** The material reads touch no store state,
   mutate nothing and write no peer profile.
5. **The requester's machinery is untouched.** The RelayRequest preimage, the
   signature, the nonce discipline, the session gate, the transport's requester
   table and the counters are exactly what the tree already had.
6. **The refusals stay typed.** Every outcome the outbound step can reach is a
   typed fact, and each is counted under its own reason.
7. **No transport file changes.** The plan touches no file under
   `crates/i2pr-transport/` and no file under `crates/i2pr-runtime/`.
8. **No configuration key is added.** The daemon's configuration surface is
   unchanged.
9. **No new task, timer, listener or socket.** The outbound step rides the
   existing tick, as it does today.
10. **Plan 531's lane and guard stay byte-identical.** This plan changes the
    requester's **material source**, not the requester's semantics, and the
    predecessor's evidence must remain exactly as it was.
11. **No advertisement.** `router.version` stays `0.9.62` and the inventory's
    advertised count stays **zero**.

## 5. Scope

### 5.1 In scope

- The relay requester's **material source**: a local trait the requester reads
  through, with one implementation for the validated store itself and one for
  the router's NetDB.
- The composition root's outbound step: no longer gated on the bootstrap, and
  no longer holding the NetDB lock across the signature or the transport's
  writes.
- A typed refusal for a store that is **absent** or **unavailable**.
- The counter that makes the refusal visible.

### 5.2 Out of scope

- The **introducer** role and the **target** role. The target's answer was
  Plan 536's subject and is already outside the lock; the introducer's role
  needs no peer's record and is not gated on the store.
- The requester's **semantics**: its window, its rotation, its nonce, its
  preimage, its signature, its session gate, its refusals and its counters are
  unchanged.
- The **transport's** requester table and the transport's refusal order.
- The stock-peer half, which stays blocked on Plan 493.

### 5.3 Out of scope, with the owner named

- **The router's validated store is still the bootstrap's store.** The
  bootstrap owns the only `RouterInfoStore` in the daemon
  (`crates/i2pr-daemon/src/bootstrap.rs:215`). Separating the router's NetDB
  from the bootstrap's lifetime is a larger change, and this plan does not
  attempt it: it removes the **lock scope** and the **silent skip**, and leaves
  the ownership alone. **Owner:** the successor that gives the router a NetDB
  owner of its own — the same successor Plan 536 named.
- **The lock scope is proven statically, not behaviourally.** Plan 536's
  closure record recorded the same gap for the target role: the composition
  root's drain is not reachable from a test without a second production
  composition. This plan does not build that seam either. **Owner:** the same
  successor that makes the composition root's relay arm reachable.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

### 6.1 `crates/i2pr-daemon/src/relay_requester.rs`

1. A **local** trait:

   ```rust
   pub trait RelayRequesterMaterialSource {
       fn peer_pool(&self) -> Result<Vec<PeerId>, RelayRequesterRefusal>;
       fn published_introducers(
           &self,
           peer: PeerId,
       ) -> Result<PublishedIntroducerSet, RelayRequesterRefusal>;
       fn introducer_endpoint(
           &self,
           introducer: [u8; 32],
       ) -> Result<IntroducerEndpoint, RelayRequesterRefusal>;
   }

   It is a **daemon-local** trait, so implementing it for the netdb crate's
   own `RouterInfoStore` is inside the orphan rule.

2. `impl RelayRequesterMaterialSource for i2pr_netdb::RouterInfoStore`, whose
   methods delegate to the **existing free functions**
   `read_published_introducers(&RouterInfoStore, PeerId)`
   (`crates/i2pr-daemon/src/relay_requester.rs:252`) and
   `read_introducer_endpoint(&RouterInfoStore, [u8; 32])`
   (`crates/i2pr-daemon/src/relay_requester.rs:292`). The free functions are
   **unchanged** — their signatures, their bodies, their `store.get(` reads and
   their strict address parses are exactly what they are today.

3. The **production** source, `ValidatedStoreRequesterSource`, holding
   `Option<&Arc<Mutex<crate::bootstrap::Bootstrap>>>`. Each of its three reads
   takes the bootstrap's lock, performs **one** read, and **releases the guard
   before the read returns**. Without a bootstrap, or with a poisoned lock, it
   returns `RelayRequesterRefusal::StoreUnavailable`.

4. `drive_relay_request` takes `source: &dyn RelayRequesterMaterialSource` in
   place of `store: &i2pr_netdb::RouterInfoStore`, and reads through it. The
   **rest of the function is unchanged**: the same window, the same rotation,
   the same refusals, the same preimage, the same signature, the same session
   gate and the same counters.

5. A new refusal, `RelayRequesterRefusal::StoreUnavailable`, for a source
   that cannot answer at all, and its own counter
   `refusals_store_unavailable`. It is a **local condition** — this router has
   no usable validated store — and it is never written into a peer's record.

### 6.2 `crates/i2pr-daemon/src/lib.rs`

The composition root's outbound step:

1. Builds the **production source** over the router's bootstrap, by reference.
2. Calls `crate::relay_requester::drive_relay_request(` with the source in
   place of the store.
3. Is **no longer gated** on the bootstrap: it runs whenever the requester role
   exists, and the absence of a store is a typed refusal counted under its own
   reason.

The literal `crate::relay_requester::drive_relay_request(` stays in the relay
tick, because Plan 531's guard looks for it there.

## 7. Design decisions

### 7.1 The signature and the transport's writes happen outside the lock

The lock is a **short, bounded** read of one thing at a time, exactly as Plan
536's target role does it
(`crates/i2pr-daemon/src/relay_target.rs:460`). The private-key signature and
the transport's two mutex acquisitions — the expensive part, and the part that
can block — happen after the guard is dropped.

### 7.2 The NetDB lock never wraps the runtime's locks

This is the defect's sharpest edge. Today the requester's outbound step holds
the router's NetDB lock while it acquires the transport's requester-table mutex
and the runtime's state mutex. After the plan, the NetDB lock is taken only for
reads, and every read returns before the transport is touched, so **no** code
path nests the router's NetDB lock inside the runtime's locks. The runtime's
own order — sockets → state — is left the only order that the transport's
locks observe.

### 7.3 The pool scan is one read, not the whole step

The candidate pool is the router's peer population, and the plan keeps that
need: the requester genuinely needs to know which peers the NetDB holds. What
changes is that the scan is **one self-contained read** whose guard is released
before it returns, instead of the prelude to a signature and two transport
writes under the same guard. The scan's own cost is O(the store's size) and
copies only hashes; it performs no crypto, no I/O and no record parse.

The window's rotation is unchanged, so the requester still examines every peer
over the following windows and never examines one it has already asked.

### 7.4 The refusal to ask is never skipped silently

A router that cannot ask because it holds no validated store is a **fact about
this router**. The plan makes that fact **typed and counted** rather than an
absent pass. A skipped outbound step is indistinguishable from a router that
had no eligible target.

### 7.5 The trait keeps Plan 531's evidence exactly as it was

The trait is implemented **for the store itself**, so Plan 531's lane — which
passes `&store` — still compiles and still passes, and Plan 531's guard — which
reads `read_published_introducers`'s body for `store.get(`, reads
`read_introducer_endpoint`'s body for `store.get(`, reads `build_request`'s body
for `relay_request_preimage(` and looks for
`crate::relay_requester::drive_relay_request(` in the relay tick — still reads
all four.

This is the reason the plan does not introduce a second, differently named
entry point, and the reason the two free functions keep their names and their
bodies. A second entry point would duplicate the signing path, and two copies
of a signing path are two places for them to diverge.

### 7.6 The reads stay read-only

The reads touch no store state, mutate nothing and write no peer profile. The
plan changes **where** the reads happen, not what they do.

### 7.7 The refusal is typed, counted, and never a peer's fault

`StoreUnavailable` says that **this router** has no usable validated store. It
is counted under its own reason. It is never written into a peer's record, and
it never counts as a peer fault.

### 7.8 No second store, no second lock

The plan adds no store, no lock and no owner. It reuses the bootstrap's mutex
for the reads it already needed, and it stops holding it for the work it did
not need it for. The larger question — the router's NetDB should not live
inside the bootstrap — is named in §5.3 and left to its successor.

## 8. Work packages

| Work package | What it carries |
| --- | --- |
| WP1 | the material-source trait, the store's implementation, the production source, the new refusal, the counter, and the requester's reads through the source |
| WP2 | the composition root's outbound step: the source in place of the store, and the ungated step |
| WP3 | the lane and the guard |
| WP4 | the documentation, the inventory and the plan's amendment log |
| WP5 | the closure record and the registry's closed rows |

## 9. Failure, cancellation, restart and contention semantics

**Failure.** A source that cannot answer returns
`RelayRequesterRefusal::StoreUnavailable`, the outbound step counts it under
its own reason, and the step ends. A poisoned bootstrap lock is the same typed
refusal. Nothing is skipped without a trace.

**Cancellation.** The outbound step is a synchronous step inside the product's
existing tick. It awaits nothing and holds no lock across an await, so a
cancellation cannot strand it.

**Restart.** Nothing is persisted. The requester's live table is live state and
is rebuilt empty; a restart asks nobody until the next window.

**Contention.** The bootstrap's mutex is now taken for one self-contained read
at a time and released before the next. The NetDB discovery tick, the
publication tick, the peer reconcile and the peer pool can no longer be held
behind an O(population) scan, a private-key signature and two transport
mutexes. The reads are **read-only**, so they cannot deadlock against a
writer: the mutex is taken once per read and released before the next. And
because the reads are the only thing that holds the NetDB lock, the router's
NetDB lock no longer wraps the runtime's locks.

## 10. Compatibility and migration

**No wire change.** The RelayRequest's bytes, the preimage, the signature, the
transport's requester table and the carriage are exactly what the tree already
produced.

**No configuration migration.** **No key is added.** The daemon's
configuration surface is unchanged.

**A refusal that did not exist before.** A router that holds no validated
store now **counts** `StoreUnavailable` where it used to say nothing. That is
a new observation, not a new behaviour: the request was not made before either.

**No change to the requester's semantics.** The same candidates are examined,
the same requests are signed and the same requests are carried.

**No change to the transport's model.** No file under `crates/i2pr-transport/`
and no file under `crates/i2pr-runtime/` is touched.

## 11. Required tests

### 11.1 The lane

`crates/i2pr-daemon/tests/ssu2_relay_requester_lock.rs` — the requester over
the **production** source, and the outbound step's behaviour with and without a
store. The lane must prove:

1. the request is produced while the router's NetDB lock is **free** — the
   production source's reads release it, and the lane measures that;
2. the outbound step **runs** when the router holds no store, and the request
   is refused with the **typed** reason, counted under its own reason, and not
   skipped;
3. the request is still produced correctly when the store is present — the
   requester's behaviour is unchanged;
4. the material reads mutate nothing and write no peer profile;
5. every refusal the outbound step can reach is typed and counted.

### 11.2 The guard

`crates/i2pr-daemon/tests/ssu2_relay_requester_lock_boundary.rs` — a static
guard that is the authority for the **shape**. It must have an inverted probe
for every rule and a fail-closed probe for every item a rule names. The
rules:

1. the outbound step is **not** gated on the bootstrap, and it holds no NetDB
   lock across the request;
2. the production source's reads **release** the lock before they return, and
   the signature and the transport's writes happen after the reads return;
3. the reads are **read-only** and mutate no store state;
4. the missing store is a **typed** refusal, and it is **counted**;
5. the requester's machinery — the preimage, the signature, the nonce, the
   session gate and the transport's requester table — is **untouched**;
6. the transport's requester table and its refusal order are **untouched**;
7. **no** configuration key is added, **no** task, timer, listener or socket is
   added, and the outbound step rides the **existing** tick;
8. Plan 531's lane and guard stay **byte-identical**, and Plan 531's guard
   still finds the strings it looks for.

Plan 531's guard (`crates/i2pr-daemon/tests/ssu2_relay_requester_boundary.rs`)
and Plan 531's lane (`crates/i2pr-daemon/tests/ssu2_relay_requester.rs`) must
stay **byte-identical**: this plan changes the requester's material **source**,
not its semantics, and the predecessor's evidence must remain exactly as it
was.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test ssu2_relay_requester_lock -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_relay_requester_lock_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_relay_requester --test ssu2_relay_requester_boundary --test ssu2_relay_target_lock --test ssu2_relay_target_lock_boundary --test ssu2_relay_target --test ssu2_relay_target_boundary --test ssu2_relay_establishment --test ssu2_relay_establishment_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor in `AGENTS.md`, with this plan's two steps added.

## 13. Documentation updates

- `specs/support.toml` — a new experimental, non-advertised surface
  `daemon.relay-requester-ask-lock`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the gates this plan's lane and guard
  discharge.
- `specs/protocols/09-ssu2.md` — the relay-requester scope, extended to the
  request's lock scope.
- `docs/architecture/i2pr-daemon.md` — the relay requester role's shape.
- The plan's own amendment log.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when all of the following hold:

1. the request is produced with **no** lock held, and the lane measures it;
2. the outbound step runs without a store and **counts** a typed refusal;
3. the requester's semantics are unchanged, and Plan 531's lane and guard are
   byte-identical;
4. the transport and the runtime are untouched, and no configuration key, task,
   timer, listener or socket is added;
5. `router.version` stays `0.9.62`, and the inventory's advertised count stays
   **zero**;
6. the full floor is green, with the closure record's table complete;
7. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the request cannot be produced outside the lock without changing the
  requester's semantics — the semantics are not this plan's to move;
- the predecessor's lane or guard would have to change — the plan's design
  exists precisely so that it does not;
- the floor's `cargo deny check advisories bans sources` fails;
- a stock peer would be required to prove any of the above — the stock-peer
  half stays blocked on Plan 493 and is **not** attempted here.

## 15. Closure evidence required

- The implementation commit, the lane-and-guard commit, the documentation
  commit and the closure commit, each named in the closure record.
- The **complete** floor table, every row with its result.
- The lane's rows, each with its result.
- The guard's rows, each with its result.
- The negative controls: each mutation, what it was expected to break, what it
  did break, and the SHA-256 proof that the tree was restored byte-identically.
- The requirement-to-evidence matrix, one row per requirement.
- The invariant review, the failure review, the migration review and the
  security review.
- The limitations, the findings by severity, the roadmap disposition and the
  unblock audit.

## 16. Handoff notes

- The plan is **experimental** and default-off. Do not describe the change as
  released, supported or advertised.
- The delivery is confined to the controlled loopback profile.
- The stock-peer half stays blocked on Plan 493's authorized two-host
  topology.
- Plan 531's closed closure record is **not** rewritten. The discharge lives
  in this plan's closure record and in the registry's row.
- Plan 536's closed closure record is **not** rewritten. The discharge of its
  Low finding 3 lives in this plan's closure record and in the registry's row.
- The plan **extends** no other plan's guard: Plan 531's guard and lane stay
  byte-identical.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 537 is registered as the successor to Plan 536 and the named owner of
Plan 536's recorded **Low** finding 3: the requester's outbound step keeps its
bootstrap gate, so the requester still contends with the router's NetDB.

The scope is the requester's **outbound step**, not its semantics. Today the
step holds the router's NetDB lock across an O(population) store scan, up to
sixteen strict address parses, a private-key signature, the transport's
requester-table mutex, and the runtime's state mutex; and a router that holds
no bootstrap does not ask at all, so a request that could have been made leaves
no trace.

The plan makes each read a short, self-contained read that releases the guard
before it returns, and it makes the absence of a store a typed, counted refusal
rather than a silent skip. It adds no store, no lock, no owner, no
configuration key and no timer, and it changes no transport file and no runtime
file.

No closed closure record is rewritten by this plan. The corrections this plan
finds, if any, live in this plan's closure record and in the registry's row.

### 17.2 — 2026-11-11, the implementation's own findings

This log is **additive**. No closed closure record is rewritten by this plan,
and the plan's own §3 is evidence and is not rewritten. The rows below record
what the implementation found and what it did about it.

| # | What the implementation found | What this plan does about it |
| --- | --- | --- |
| 17.2.1 | §11.2's rule 8 asks for Plan 531's lane and guard to stay **byte-identical**. "Byte-identical" is only checkable as a digest, so the guard now records the SHA-256 of both predecessor files and compares against it. | The digests live in `crates/i2pr-daemon/tests/ssu2_relay_requester_lock_boundary.rs` as `PREDECESSOR_LANE_SHA256` and `PREDECESSOR_GUARD_SHA256`. They are a **tripwire**: a future plan that needs to change Plan 531's evidence must change these two constants **and** say why. |
| 17.2.2 | Plan 537 touches the **same composition root** as Plan 536, so Plan 536's own evidence must remain exactly as it was too — not only Plan 531's. | The guard records Plan 536's lane and guard digests as well (`PLAN536_LANE_SHA256` and `PLAN536_GUARD_SHA256`), and the rule compares all four. |
| 17.2.3 | §11.2's rule 1 asks that the outbound step not be gated on the bootstrap. The gate Plan 537 removed lived in the `if let` **condition**, not in the block's body: the old code read `if let Some(bootstrap) = bootstrap.as_ref() && let Ok(guard) = bootstrap.lock() && let Some(rng) = requester_rng.as_mut() { … }`. A guard that read only the block's **body** would not have seen it. | The guard reads the **enclosing statement** — the condition that opens the innermost enclosing block and the block's own body — through `enclosing_statement`. The rule therefore sees a gate wherever it is put. |
| 17.2.4 | §11.1's item 2 names **one** local condition: the router holds no store. The implementation found a **second** one with the same shape: a **poisoned** NetDB lock. | The lane discharges both. The plan's §6.1 already required the poisoned lock to be the same typed refusal, so this is an addition to the **evidence**, not a change to the **scope**. |
| 17.2.5 | The lane's control signer originally took the router's NetDB lock with `lock()`. A regression that already holds the lock would then block for ever, and a lane that hangs proves nothing. | The control takes the lock with `try_lock()` and the lane's own snapshots do too, so a regression that holds the lock **fails a row** instead of hanging the suite. The control's meaning is unchanged: a lock that cannot be taken is already held, which is exactly what the probe must observe. |
| 17.2.6 | The lane drives the **production source** directly, so it cannot see a regression in the **composition root**. The composition root's shape is therefore proven by the **guard**, not by the lane. | This is Plan 536's recorded **Low** finding 2, carried forward unchanged and named in §5.3 with its owner. The lane's claim is the source's release semantics, and the guard's claim is the composition root's shape; neither claims the other's. |
| 17.2.7 | Plan 536's recorded **Low** finding 3 reads: *"The requester's outbound step keeps its bootstrap gate. That is deliberate — asking a peer for a relay genuinely needs the router's peer population — but it means the requester still contends with the router's NetDB."* That statement is **still true** after this plan in its first clause: the requester still needs the router's peer population, and it still reads the pool from the validated store. | The finding is discharged **in its recorded terms**: the *need* for the peer population is unchanged, and the **contention** — the lock scope and the silent skip — is gone. The **ownership** question — the router's NetDB should not live inside the bootstrap — stays with the successor named in §5.3. |
| 17.2.8 | The daemon deep-dive's Plan 531 section describes the requester's outbound step as it was **before** this plan. | The deep-dive gains a **Plan 537 section** that states the new shape, and the Plan 531 prose is left exactly as it was. The deep-dive is prose; the closure record and the registry's row carry the finding's disposition. |
