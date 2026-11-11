# Plan 536 — the relay target role must answer without holding the router's NetDB lock

## 1. Objective

The router's **target** role — the peer that an introducer reached, which must
answer the forwarded introduction with an accept — must produce that answer
**outside** the router's NetDB lock, and its drain must never be **skipped
silently**.

Two facts about the tree force this. First, the target's drain runs with the
bootstrap's mutex held for the whole pass, and a pass is bounded at
`MAX_RELAY_DRAIN_PER_PASS = 16` answers, each of which parses an address,
signs an accept and stages it on a session. The same mutex is taken by the
router's NetDB discovery tick, by the requester's outbound step, by the
RouterInfo publication tick, by the peer reconcile and by the peer pool. So
the relay answer path and the router's NetDB contend, and the loser is
whichever arrives second.

Second, the drain is **gated on the daemon holding a `Bootstrap`**. A router
that does not hold one does not drain, does not refuse and does not count:
the introductions sit in the handoff and the operator sees nothing. That is
the coupling Plan 532 recorded as its **Low** finding 2 and Plan 534 carried
forward unchanged: *"the target role needs the bootstrap's store"*
(`plans/closure/network-participation/532-status.md:604`,
`plans/closure/network-participation/534-status.md:130`).

The plan makes the target role's material read a **short, bounded** read and
moves the answer outside it, and it makes the absence of a store a **typed,
counted refusal** rather than a silent skip.

## 2. Why ready

- The target role's machinery is **complete**: the owner, the transport's
  target table, the accept's signature, the hole punch and the drain are all
  in the tree and green (`crates/i2pr-daemon/src/relay_target.rs`,
  `crates/i2pr-daemon/src/ssu2_relay_drain.rs`).
- The **precedent** for the fix is already in the tree. Plan 533's
  establishment task reads the same validated store under a short lock and
  releases it **before** the dial, so a slow handshake can never hold the
  router's NetDB (`crates/i2pr-daemon/src/relay_establishment_task.rs:129`).
  This plan makes the target role follow the pattern its own sibling already
  follows.
- The defect is **internal**. No stock peer, no second host and no external
  topology is needed to find it, to fix it or to prove it.
- The **owner is named** and the finding is **open**: Plan 532's Low finding
  2, carried forward by Plan 534.

## 3. Current implementation evidence

### 3.1 The drain is gated on the bootstrap

The composition root's relay tick holds the target's drain inside the
bootstrap's guard (`crates/i2pr-daemon/src/lib.rs:5735`):

```rust
if let Some(target) = target.as_mut()
    && let Some(bootstrap) = bootstrap.as_ref()
    && let Ok(guard) = bootstrap.lock()
{
    // ...
    let drained = crate::ssu2_relay_drain::drain_target_events(
        &mut owner,
        target,
        guard.store(),
        now_secs,
        now_ms,
    );
}
```

Without a bootstrap the whole block is skipped. There is no drain, no
refusal, no count and no trace.

### 3.2 The lock is held across the answer

`drain_target_events` (`crates/i2pr-daemon/src/ssu2_relay_drain.rs:293`) is
bounded at `MAX_RELAY_DRAIN_PER_PASS = 16`
(`crates/i2pr-daemon/src/ssu2_relay_drain.rs:47`), and each event drives
`drive_relay_target` (`crates/i2pr-daemon/src/relay_target.rs:394`), which
parses the requester's SSU2 address, reads the requester's signing key,
**signs** the accept and stages it on the session. All of that happens while
the caller still holds the bootstrap's mutex.

### 3.3 The same lock is the router's NetDB

The bootstrap's mutex guards the router's only validated `RouterInfoStore`,
and it is taken by the NetDB discovery tick
(`crates/i2pr-daemon/src/lib.rs:2063`), by the requester's outbound step
(`crates/i2pr-daemon/src/lib.rs:5658`), by the publication tick, by the peer
reconcile and by the peer pool. The introducer's drain
(`crates/i2pr-daemon/src/lib.rs:5593`) and the requester's **inbound** drain
(`crates/i2pr-daemon/src/lib.rs:5686`) are **not** gated on it, because
neither needs a peer's record to be answered.

The target's drain is the only relay drain that needs a store, and it needs
it for **one named peer** — the requester named in the introduction.

### 3.4 The consequence

A router that relays for others holds the NetDB lock across up to sixteen
signature operations, on every tick. A router that does not hold a bootstrap
never answers an introduction at all, and says nothing about it.

## 4. Invariants

1. **The answer is produced with no lock held.** The requester's material is
   read under a short, bounded lock, and the lock is **released** before the
   accept is signed or staged.
2. **The drain is never skipped silently.** The drain runs whenever the
   target role exists. A missing store is a **typed, counted refusal**, not
   an absent pass.
3. **The read stays read-only.** The material read touches no store state,
   mutates nothing and writes no peer profile.
4. **The target's machinery is untouched.** The transport's target table, the
   accept's signature preimage, the hole punch and the dial profile are
   exactly what the tree already had.
5. **The refusals stay typed.** Every outcome the drain can reach is a typed
   fact, and each is counted under its own reason.
6. **No transport file changes.** The plan touches no file under
   `crates/i2pr-transport/`.
7. **No configuration key is added.** The daemon's configuration surface is
   unchanged.
8. **No new task, timer, listener or socket.** The drain rides the existing
   tick, as it does today.
9. **Plan 532's lane and guard stay byte-identical.** This plan changes the
   target role's **material source**, not the target role's semantics, and
   the predecessor's evidence must remain exactly as it was.
10. **No advertisement.** `router.version` stays `0.9.62` and the
    inventory's advertised count stays **zero**.

## 5. Scope

### 5.1 In scope

- The relay target role's **material source**: a local trait the target role
  reads through, with one implementation for the validated store itself and
  one for the router's NetDB.
- The composition root's target drain: no longer gated on the bootstrap, and
  no longer holding the NetDB lock across the answer.
- A typed refusal for a store that is **absent** or **unavailable**.
- The counters that make the refusal visible.

### 5.2 Out of scope

- The **introducer** role and the **requester** role. Both are already
  ungated where they need to be; the requester's outbound step keeps its
  bootstrap gate, because asking a peer for a relay genuinely needs the
  router's peer population.
- The target role's **semantics**: its table, its signature, its hole punch,
  its refusals and its counters are unchanged.
- The **transport's** target table and the transport's refusal order.
- The stock-peer half, which stays blocked on Plan 493.

### 5.3 Out of scope, with the owner named

- **The router's validated store is still the bootstrap's store.** The
  bootstrap owns the only `RouterInfoStore` in the daemon
  (`crates/i2pr-daemon/src/bootstrap.rs:215`). Separating the router's NetDB
  from the bootstrap's lifetime is a larger change, and this plan does not
  attempt it: it removes the **lock scope** and the **silent skip**, and
  leaves the ownership alone. **Owner:** the successor that gives the router
  a NetDB owner of its own.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

### 6.1 `crates/i2pr-daemon/src/relay_target.rs`

1. A **local** trait:

   ```rust
   pub trait RelayTargetMaterialSource {
       fn read_requester_material(
           &self,
           requester: [u8; 32],
       ) -> Result<RequesterMaterial, RelayTargetRefusal>;
   }
   ```

   It is a **daemon-local** trait, so implementing it for the netdb crate's
   own `RouterInfoStore` is inside the orphan rule.

2. `impl RelayTargetMaterialSource for i2pr_netdb::RouterInfoStore`, whose
   body **calls the existing free function**
   `read_requester_material(&RouterInfoStore, [u8; 32])`
   (`crates/i2pr-daemon/src/relay_target.rs:351`). The free function is
   **unchanged** — its signature, its body and its four refusals are exactly
   what they are today.

3. `drive_relay_target` takes `source: &dyn RelayTargetMaterialSource` in
   place of `store: &i2pr_netdb::RouterInfoStore`, and reads through it. The
   **rest of the function is unchanged**: the same refusals, the same
   signature, the same hole punch, the same counters.

4. A new refusal, `RelayTargetRefusal::StoreUnavailable`, for a source that
   cannot answer at all. It is a **local condition** — this router has no
   validated store — and it is never written into a peer's record.

### 6.2 `crates/i2pr-daemon/src/ssu2_relay_drain.rs`

`drain_target_events` takes the source in place of the store, and is
otherwise unchanged: the same bound, the same loop, the same counters, and
the same handoff to `drive_relay_target`.

### 6.3 `crates/i2pr-daemon/src/lib.rs`

The composition root gains a **source** that reads the router's validated
store under a **short** lock:

1. The source holds `Option<&Arc<Mutex<crate::bootstrap::Bootstrap>>>`.
2. Its `read_requester_material` takes the bootstrap's lock, reads the one
   named peer, and **drops the guard before returning**. The answer is
   therefore produced with **no lock held**.
3. Without a bootstrap, or with a poisoned lock, it returns
   `RelayTargetRefusal::StoreUnavailable`.
4. The tick's target drain is **no longer gated** on the bootstrap: it runs
   whenever the target role exists, and the absence of a store is a typed
   refusal counted under its own reason.

## 7. Design decisions

### 7.1 The answer is produced outside the lock

The lock is a **short, bounded** read of one named peer, exactly as Plan
533's establishment task already does it
(`crates/i2pr-daemon/src/relay_establishment_task.rs:129`). The accept's
signature and the hole punch's staging — the expensive part, and the part
that can fail — happen after the guard is dropped.

### 7.2 The drain is never skipped silently

A relay introduction that arrives and cannot be answered is a **fact about
this router**: it has no validated record of the requester, or no store at
all. The plan makes that fact **typed and counted** rather than an absent
pass. A skipped drain is indistinguishable from a router that was never
reached.

### 7.3 The trait keeps Plan 532's evidence exactly as it was

The trait is implemented **for the store itself**, so Plan 532's lane — which
passes `&RouterInfoStore` — still compiles and still passes, and Plan 532's
guard — which reads `drive_relay_target`'s body for `read_requester_material(`
and `owner.runtime.on_target_relay_intro(` — still reads both. The method is
**named** `read_requester_material` for exactly that reason: the body keeps
the literal the predecessor's guard looks for.

This is the reason the plan does not introduce a second, differently named
entry point. A second entry point would duplicate the accept's signing body,
and two copies of a signing path are two places for them to diverge.

### 7.4 The read stays read-only

`read_requester_material` reads one record and derives two values from it.
It mutates nothing, writes no peer profile and claims no corroborating
class. The plan changes **where** the read happens, not what it does.

### 7.5 The refusal is typed, counted, and never a peer's fault

`StoreUnavailable` says that **this router** has no usable validated store.
It is counted under its own reason. It is never written into the requester's
record, and it never counts as a peer fault.

### 7.6 No second store, no second lock

The plan adds no store, no lock and no owner. It reuses the bootstrap's
mutex for the read it already needed, and it stops holding it for the work
it did not need it for. The larger question — the router's NetDB should not
live inside the bootstrap — is named in §5.3 and left to its successor.

## 8. Work packages

| Work package | What it carries |
| --- | --- |
| WP1 | the material-source trait, the store's implementation, the new refusal, and the target role's read through the source |
| WP2 | the drain's source, the composition root's source, and the ungated drain |
| WP3 | the lane and the guard |
| WP4 | the documentation, the inventory and the plan's amendment log |
| WP5 | the closure record and the registry's closed rows |

## 9. Failure, cancellation, restart and contention semantics

**Failure.** A source that cannot answer returns
`RelayTargetRefusal::StoreUnavailable`, the drain counts it under its own
reason, and the pass continues with the next event. A poisoned bootstrap
lock is the same typed refusal. Nothing is skipped without a trace.

**Cancellation.** The drain is a synchronous step inside the product's
existing tick. It awaits nothing and holds no lock across an await, so a
cancellation cannot strand it.

**Restart.** Nothing is persisted. The target's handoff is live state and is
rebuilt empty; a restart answers no introduction until a new one arrives.

**Contention.** The bootstrap's mutex is now taken for a single-key read and
released before the answer. The NetDB discovery tick, the requester's
outbound step, the publication tick, the peer reconcile and the peer pool can
no longer be held behind sixteen signature operations. The read is
**read-only**, so it cannot deadlock against a writer: the mutex is taken
once per event and released before the next.

## 10. Compatibility and migration

**No wire change.** The accept's bytes, the introduction's bytes, the
transport's target table and the hole punch are exactly what the tree already
produced.

**No configuration migration.** **No key is added.** The daemon's
configuration surface is unchanged.

**A refusal that did not exist before.** A router that holds no validated
store now **counts** `StoreUnavailable` where it used to say nothing. That is
a new observation, not a new behaviour: the introduction was not answered
before either.

**No change to the target's semantics.** The same introductions are answered,
the same accepts are signed and the same hole punches are staged.

**No change to the transport's model.** No file under `crates/i2pr-transport/`
is touched.

## 11. Required tests

### 11.1 The lane

`crates/i2pr-daemon/tests/ssu2_relay_target_lock.rs` — the target role over
the **production** source, and the drain's behaviour with and without a
store. The lane must prove:

1. the answer is produced while the router's NetDB lock is **free** — the
   production source's read releases it before the answer, and the lane
   measures that;
2. the drain **runs** when the router holds no store, and the introduction
   is refused with the **typed** reason, counted under its own reason, and
   not skipped;
3. the answer is still produced correctly when the store is present — the
   target role's behaviour is unchanged;
4. the material read mutates nothing and writes no peer profile;
5. every refusal the drain can reach is typed and counted.

### 11.2 The guard

`crates/i2pr-daemon/tests/ssu2_relay_target_lock_boundary.rs` — a static
guard that is the authority for the **shape**. It must have an inverted
probe for every rule and a fail-closed probe for every item a rule names. The
rules:

1. the target's drain is **not** gated on the bootstrap;
2. the production source's read **releases** the lock before it returns, and
   the answer is produced after the read returns;
3. the read is **read-only** and mutates no store state;
4. the missing store is a **typed** refusal, and it is **counted**;
5. the target role's machinery — the transport seam, the signature, the hole
   punch and the dial profile — is **untouched**;
6. the transport's target table and its refusal order are **untouched**;
7. **no** configuration key is added, **no** task, timer, listener or socket
   is added, and the drain rides the **existing** tick;
8. Plan 532's lane and guard stay **byte-identical**, and Plan 532's guard
   still finds the strings it looks for.

Plan 532's guard (`crates/i2pr-daemon/tests/ssu2_relay_target_boundary.rs`)
and Plan 532's lane (`crates/i2pr-daemon/tests/ssu2_relay_target.rs`) must
stay **byte-identical**: this plan changes the target role's material
**source**, not its semantics, and the predecessor's evidence must remain
exactly as it was.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test ssu2_relay_target_lock -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_relay_target_lock_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_relay_target --test ssu2_relay_target_boundary --test ssu2_relay_establishment --test ssu2_relay_establishment_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor in `AGENTS.md`, with this plan's two steps added.

## 13. Documentation updates

- `specs/support.toml` — a new experimental, non-advertised surface
  `daemon.relay-target-answer-lock`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the gates this plan's lane and guard
  discharge.
- `specs/protocols/09-ssu2.md` — the relay-target scope, extended to the
  answer's lock scope.
- `docs/architecture/i2pr-daemon.md` — the relay target role's shape.
- The plan's own amendment log.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when all of the following hold:

1. the answer is produced with **no** lock held, and the lane measures it;
2. the drain runs without a store and **counts** a typed refusal;
3. the target role's semantics are unchanged, and Plan 532's lane and guard
   are byte-identical;
4. the transport is untouched, and no configuration key, task, timer,
   listener or socket is added;
5. `router.version` stays `0.9.62`, and the inventory's advertised count
   stays **zero**;
6. the full floor is green, with the closure record's table complete;
7. the guard's inverted probes each fail as intended and the tree is
   restored byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the answer cannot be produced outside the lock without changing the target
  role's semantics — the semantics are not this plan's to move;
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
- The negative controls: each mutation, what it was expected to break, what
  it did break, and the SHA-256 proof that the tree was restored
  byte-identically.
- The requirement-to-evidence matrix, one row per requirement.
- The invariant review, the failure review, the migration review and the
  security review.
- The limitations, the findings by severity, the roadmap disposition and
  the unblock audit.

## 16. Handoff notes

- The plan is **experimental** and default-off. Do not describe the change
  as released, supported or advertised.
- The delivery is confined to the controlled loopback profile.
- The stock-peer half stays blocked on Plan 493's authorized two-host
  topology.
- Plan 532's closed closure record is **not** rewritten. The discharge lives
  in this plan's closure record and in the registry's row.
- The plan **extends** no other plan's guard: Plan 532's guard and lane stay
  byte-identical.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 536 is registered as the successor to Plan 535 and the named owner of
Plan 532's recorded **Low** finding 2: the target role needs the bootstrap's
store, and the composition root drains the target's handoff only when the
daemon holds a `Bootstrap`.

The scope is the target role's **answer**, not its semantics. The accept is
signed and staged today while the router's NetDB lock is held, across up to
sixteen answers per pass; and a router that holds no bootstrap does not drain
at all, so an introduction that arrives and cannot be answered leaves no
trace.

The plan makes the material read a short, bounded read and moves the answer
outside it, and it makes the absence of a store a typed, counted refusal
rather than a silent skip. It adds no store, no lock, no owner, no
configuration key and no timer, and it changes no transport file.

No closed closure record is rewritten by this plan. The corrections this plan
finds, if any, live in this plan's closure record and in the registry's row.

### 17.2 — 2026-11-11, the implementation's own findings

This log is **additive**. No closed closure record is rewritten by this plan,
and the plan's own §3 is evidence and is not rewritten. The rows below record
what the implementation found and what it did about it.

| # | What the implementation found | What this plan does about it |
| --- | --- | --- |
| 17.2.1 | §11.2's rule 8 asks for Plan 532's lane and guard to stay **byte-identical**. "Byte-identical" is only checkable as a digest, so the guard now records the SHA-256 of both predecessor files and compares against it. | The digests live in `crates/i2pr-daemon/tests/ssu2_relay_target_lock_boundary.rs` as `PREDECESSOR_LANE_SHA256` and `PREDECESSOR_GUARD_SHA256`. They are a **tripwire**: a future plan that needs to change Plan 532's evidence must change these two constants **and** say why. |
| 17.2.2 | §11.2's rule 1 asks that the drain not be gated on the bootstrap. The gate Plan 536 removed lived in the `if let` **condition**, not in the block's body: the old code read `if let Some(target) = target.as_mut() && let Some(bootstrap) = bootstrap.as_ref() && let Ok(guard) = bootstrap.lock() { … }`. A guard that read only the block's **body** would not have seen it. | The guard reads the **enclosing statement** — the condition that opens the innermost enclosing block and the block's own body — through `enclosing_statement`. The rule therefore sees a gate wherever it is put. |
| 17.2.3 | §11.1's item 2 names **one** local condition: the router holds no store. The implementation found a **second** one with the same shape: a **poisoned** NetDB lock. | The lane discharges both. The plan's §6.3 already required the poisoned lock to be the same typed refusal, so this is an addition to the **evidence**, not a change to the **scope**. |
| 17.2.4 | Plan 532's recorded **Low** finding 2 reads: *"the target role needs the bootstrap's store"*. That statement is **still true** after this plan: the production source reads the bootstrap's store. What changed is the **lock scope** (a single-key read, released before the answer) and the **silent skip** (a typed, counted refusal). | The finding is discharged **in its recorded terms** and **not** in the terms the prose might suggest. The target role's *need* for the bootstrap's store is unchanged; the coupling that remains is bounded, typed and counted. The **ownership** question — the router's NetDB should not live inside the bootstrap — stays with the successor named in §5.3. |
| 17.2.5 | The daemon deep-dive's Plan 532 section says: *"with no bootstrap the target's introductions are never drained."* That sentence describes the tree **before** this plan. | The deep-dive gains a **Plan 536 section** that states the new shape, and the Plan 532 prose is left exactly as it was. The deep-dive is prose; the closure record and the registry's row carry the finding's disposition. |
