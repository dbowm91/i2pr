# Plan 525 — the router publishes the introducer set it learned

**Status:** registered.
**Subsystem:** network-participation. **Registered:** 2026-11-09.
**Predecessor:** Plan 524 (the router asks a peer for a relay tag, and records
the introducer), Plan 523 (the SSU2 introducer set is the specification's
form), Plan 520 (the router's own qualified SSU2 address).

## 1. Objective

The network-participation roadmap's **completion definition** requires that the
router "publishes the validated introducer set in its RouterInfo address".

Plan 523 corrected the **form** of that set. Plan 524 delivered the
**acquisition** half — the router learns a peer's advertised introducer
capability, asks such a peer for a relay tag while it is firewalled, and
records the answer as an authenticated introducer. The **publication** half is
this plan's subject.

The router must be able to:

1. **build** the specification's introducer-only address from the introducer
   set it learned — an address that carries `ih[N]`/`iexp[N]`/`itag[N]` and
   **no** `host` and **no** `port`;
2. **choose** that form over the direct form when the direct form is not
   available, which is exactly the situation a firewalled router is in;
3. **sign** the result through the netdb's own second qualification rule and
   install it as the router's own published record.

The `C` capability this router publishes about **itself** is a third, separate
matter and is **not** this plan's subject.

## 2. Why ready

The tree holds every fact this plan needs, and each fact has exactly one
production caller that is missing.

- The runtime can already **build** the introducer-only address
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:2586`). The builder exists; no
  production path calls it.
- The netdb already owns the **second qualification rule** and the **second
  builder** the introducer-only form needs
  (`crates/i2pr-netdb/src/floodfill_role.rs:277`,
  `crates/i2pr-netdb/src/local.rs:292`). They exist; no production path calls
  them.
- The daemon already owns the **self-publication** path — the opt-in, the
  hand-off of the transport's address to the netdb's builder, the install, the
  withdrawal and the counters
  (`crates/i2pr-daemon/src/self_publication.rs:357`). It exists; it asks for
  the **direct** form only.
- The runtime already owns the **validated introducer set** and can hand it
  over (`crates/i2pr-runtime/src/ssu2_runtime.rs:2495`,
  `crates/i2pr-runtime/src/ssu2_peer_relay.rs:900`).

So the gap is not a missing capability. It is a missing **call** — and a
missing **decision**, because the daemon has no way to learn that the direct
form is unavailable *and* that an introducer-only form is available.

**The tree also holds a latent defect in the very function this plan needs.**
`introducer_publication_material` cannot succeed as written. See §3.1.

## 3. Current implementation evidence

### 3.1 The runtime's introducer-only builder can never succeed

`crates/i2pr-runtime/src/ssu2_runtime.rs:2586`:

```rust
    pub fn introducer_publication_material(
        &self,
        now_secs: u64,
    ) -> Result<Ssu2PublicationMaterial, Ssu2PublicationUnavailable> {
```

It selects the validated introducer set and then calls the transport's own
snapshot builder (`crates/i2pr-transport-ssu2/src/publication.rs:212`) with
**the same second** for both the build time and the evidence expiry
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2612`,
`crates/i2pr-runtime/src/ssu2_runtime.rs:2613`):

```rust
now_secs,
evidence_expires_secs: now_secs,
```

The transport's contract is that an expired evidence withdraws the address
(`crates/i2pr-transport-ssu2/src/publication.rs:239`):

```rust
if now_secs >= evidence_expires_secs {
    return Ok(PublicationOutcome::Withheld(
        WithholdReason::EvidenceExpired,
    ));
}
```

With `now_secs == evidence_expires_secs` the condition is **always** true, so
the builder returns `Withheld(EvidenceExpired)` — never `IntroducerOnly`. The
runtime then discards every outcome that is not `IntroducerOnly`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2617`):

```rust
        let i2pr_transport_ssu2::publication::PublicationOutcome::IntroducerOnly(snapshot) =
            snapshot
        else {
            return Err(Ssu2PublicationUnavailable::InvalidPublication);
        };
```

**The function therefore always fails**, with `InvalidPublication` — a refusal
that names a malformed address when the real cause is a withdrawn snapshot.
The transport's own test already pins the contract that breaks it
(`crates/i2pr-transport-ssu2/src/publication.rs:544`):

```rust
fn expired_evidence_withdraws() {
    let outcome = build(
        PublicationPolicy::new(true, false),
        reachable_snapshot(),
        Some(endpoint()),
        3600,
        3600,
    )
    .expect("build");
    assert_eq!(
        outcome,
        PublicationOutcome::Withheld(WithholdReason::EvidenceExpired)
    );
```

This is the same class of defect Plan 524 found in the recording path
(`crates/i2pr-runtime/src/ssu2_peer_relay.rs:1119`, whose sweep clock was
derived from the record's own expiry). It is invisible today **only** because
the function has no production caller.

The defect is a **failing gate**, not a missing one: the gate is correct and
tested, and the caller passes the wrong value into it.

### 3.2 The evidence a snapshot should carry is the introducer set's own

The introducer-only form's evidence is the introducer set itself. The
transport's table already knows each record's expiry
(`crates/i2pr-transport-ssu2/src/introducer.rs:137`):

```rust
pub const fn is_expired(&self, now_secs: u64) -> bool {
    now_secs >= self.expires_secs
}
```

and the table's selection reaps expired records and returns only live ones
(`crates/i2pr-transport-ssu2/src/introducer.rs:253`). Because a record with
`expires_secs == 0` is expired at every second, **every selected introducer
has an expiry strictly greater than `now_secs`**. The evidence expiry a
snapshot may therefore carry is derivable from the set itself, and the tree
already has both halves.

### 3.3 The netdb's second rule and second builder exist, uncalled

`crates/i2pr-netdb/src/local.rs:292`:

```rust
pub fn build_with_ssu2_introducer_address(
    &self,
    published: Date,
    options: Mapping,
    address: RouterAddress,
) -> Result<LocalRouterInfo, LocalRouterInfoError> {
    Self::validate_options(&options)?;
    if !is_qualified_ssu2_introducer_address(&address) {
        return Err(LocalRouterInfoError::UnqualifiedAddress);
    }
    self.sign_addressed_many(published, options, vec![address])
}
```

The rule it consults (`crates/i2pr-netdb/src/floodfill_role.rs:277`) accepts
exactly the specification's introducer-only form: the transport style is
`SSU2`, there is **no** `host` and **no** `port`, the MTU is in range, `v` is
`2`, the capability string carries only family letters, and every introducer
group is complete. The builder carries **no** floodfill permit, so it cannot
produce `caps=f`.

### 3.4 The daemon's self-publication path asks for the direct form only

`crates/i2pr-daemon/src/self_publication.rs:357`:

```rust
pub fn refresh(
    &self,
    bundle: &RouterIdentityBundle,
    service: &Ssu2RuntimeService,
    now_ms: u64,
) -> SelfPublicationOutcome {
    ...
    let material = service.publication_material(now_ms);
```

`publication_material` is the **direct** form's builder
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2654`). It refuses everything that is
not a `Reachable` router with a bound, loopback endpoint
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2696`):

```rust
if reachability != ReachabilityState::Reachable || expiry <= now {
    return Err(Ssu2PublicationUnavailable::ReachabilityUnqualified);
}
```

A firewalled router therefore never reaches the introducer-only form: the
daemon receives a refusal, and the refusal becomes the **withdrawal** — the
record returns to the netdb's ordinary **zero-address** form
(`crates/i2pr-daemon/src/self_publication.rs:397`, calling
`crates/i2pr-netdb/src/local.rs:224`, whose `build` signs **no** address).

### 3.5 The daemon's counter for the empty introducer set can never move

The counters already carry a reason for the empty set
(`crates/i2pr-daemon/src/self_publication.rs:157`):

```rust
pub withheld_no_introducers: u64,
```

and the mapping from the runtime's typed refusal to the counter exists
(`Ssu2PublicationUnavailable::NoIntroducers` →
`withheld_no_introducers`). But `publication_material` never returns
`NoIntroducers` — that reason belongs to the introducer-only builder, which
the daemon never calls. The counter is **permanently zero**, and it is the
counter that would say *"the router is firewalled and has nobody to introduce
it"*.

### 3.6 The reference's whole chain

The pinned i2pd `2.61.0`
(`635b013a612ff47278ef02acf8580a28e10e26c5`) publishes **one** SSU2 address
whose **shape** depends on reachability.

`libi2pd/RouterInfo.cpp:1333`:

```cpp
bool isPublished = address.published && !address.host.is_unspecified () && address.port;
```

- **Published** (`:1350`-`:1373`): `caps` carries the **role** letters —
  `B` when the router peer-tests and `C` when it is an introducer — and
  `host`/`port` are written (`:1377`, `:1444`).
- **Not published** (firewalled) (`:1360`-`:1365`): `caps` carries only the
  **family** letters — `4` and/or `6` — and `host`/`port` are **omitted**.
- The intro key `i` is written for SSU2 in **both** shapes (`:1384`).
- The introducer groups `iexp[N]`/`ih[N]`/`itag[N]` are written whenever the
  address carries introducers, in **both** shapes (`:1391`-`:1431`).
- `mtu` (`:1433`), `s` and `v` (`:1459`) are written for SSU2 in both shapes.

So the reference publishes the **direct** form when it is reachable and the
**introducer-only** form when it is not, and it publishes the introducer-only
form **with** introducers when it has them.

The tree's introducer-only renderer
(`crates/i2pr-transport-ssu2/src/publication.rs:326`) writes exactly `caps`
(family), `i`, the introducer groups, `mtu`, `s` and `v`, and omits `host` and
`port` — **the reference's unpublished SSU2 form, option for option**. Plan
523 put it there; nothing has ever rendered it.

The tree's *withdrawal* renderer
(`crates/i2pr-transport-ssu2/src/publication.rs:389`) writes `s` and `v` only,
where the reference's unpublished form carries `s`, `i` and `v`. That
divergence belongs to Plan 520's path and is **out of scope** here (§5.2).

### 3.7 The reference's condition for asking is the one the tree already uses

`libi2pd/SSU2Session.cpp:753`:

```cpp
if (GetRouterStatus () == eRouterStatusFirewalled && m_Address->IsIntroducer ())
```

and `libi2pd/RouterInfo.h:184`:

```cpp
bool IsIntroducer () const { return caps & eSSUIntroducer; };
```

The reference asks for a relay tag when it is **firewalled**, and it publishes
the introducer-only form in that same state. Plan 524 already implemented the
acquisition under that condition, so the publication lands in the same state
the acquisition uses, and the router's own posture is coherent: a firewalled
router asks for introducers, learns them, and publishes them.

## 4. Invariants that must not regress

1. **The direct form stays the preferred form.** A router whose reachability
   evidence admits the direct form publishes the **direct** form. The
   introducer-only form is a fallback, never a replacement.
2. **The netdb's two rules stay two rules.** The direct rule
   (`is_qualified_ssu2_address`) and the introducer rule
   (`is_qualified_ssu2_introducer_address`) are unchanged, and the
   introducer-only record is qualified by the introducer rule **only**.
3. **No floodfill permit.** The introducer-only record carries **no** `f`
   capability, and the builder that signs it takes no permit.
4. **No host, no port.** The introducer-only address publishes no endpoint.
   The transport's renderer omits them and the netdb's rule refuses them, and
   both must keep doing so.
5. **The publication is opt-in and default-off.** The router publishes nothing
   it did not publish before unless the operator asked for it.
6. **Nothing is advertised.** `router.version` stays `0.9.62`, the published
   RouterInfo's `caps` string is unchanged, and the support inventory's
   advertised count stays **zero**.
7. **Every refusal is counted under its own reason.** A refusal is never
   silent and never collapses into another reason.
8. **No new task, timer, listener or socket.** The publication rides the
   daemon's existing publication cycle.
9. **No secret crosses a boundary.** The published material carries the
   router's **public** keys only.
10. **The wire vocabulary is untouched.** The plan renders an address; it
    defines no block, no option and no codec.

## 5. Scope

### 5.1 In scope

- The runtime's introducer-only builder: its **evidence expiry**, its
  **family capability** and its **material**.
- The runtime's **composition** of the two published forms into one typed
  answer.
- The daemon's **hand-off**: choosing the netdb's builder that matches the
  form the runtime admitted.
- The daemon's **counters**: the two published forms, the withdrawal and the
  empty introducer set are each counted under their own reason.
- The daemon's **observability**: the self-publication counters become
  readable through the existing inspection seam.
- The lane, the guard, the documentation and the support inventory.

### 5.2 Out of scope

- **The `C` advertisement.** This router advertising **its own** introducer
  role is a third, separate matter. The reference publishes `C` on the
  **published** (reachable) form (`libi2pd/RouterInfo.cpp:1358`), so it is a
  different behaviour from this plan's and needs its own plan-of-record.
- **The `caps=f` advertisement.** Plan 494 owns it and it is blocked on Plan
  493's two-family evidence.
- **The withdrawal form.** A firewalled router with **no** introducer keeps
  publishing the netdb's ordinary zero-address record. Changing that into the
  reference's unpublished `s`+`i`+`v` form would change Plan 520's shipped
  path and needs its own plan-of-record; §3.6 records the divergence.
- **The acquisition.** Plan 524 owns it and it is closed.
- **The stock-peer half.** The plan's evidence is a local, in-process lane.
  Evidence against a stock peer needs the authorized two-host topology Plan
  493 owns, and it stays blocked there.
- **The private-LAN profile.** The introducer-only form publishes no endpoint
  and is therefore safe in any profile, but the `ssu2.advertise` switch stays
  refused in the private-LAN profile (§7, D9).
- **Any change to the transport's option vocabulary, the parser, or the
  netdb's rules.**

## 6. Required production changes

1. **`i2pr-runtime`** — `Ssu2RuntimeService::introducer_publication_material`
   derives its snapshot's evidence expiry from the **introducer set's own**
   expiry, derives its family capability from the router's **bound** socket
   rather than a literal, and reports the material's remaining lifetime
   truthfully.
2. **`i2pr-runtime`** — a new **typed composition** answers "which form does
   this router publish for itself now?", returning either the direct form or
   the introducer-only form, or a typed refusal. The composition is the
   runtime's, because the direct form's availability and the introducer
   set's liveness are both the runtime's facts.
3. **`i2pr-daemon`** — the self-publication owner consumes the **typed form**
   and hands the matching address to the netdb's matching builder. It never
   branches on a refusal code to infer a form.
4. **`i2pr-daemon`** — the counters distinguish the two published forms and
   the withdrawal, and the empty-introducer-set reason becomes reachable.
5. **`i2pr-daemon`** — the self-publication counters are published through the
   existing inspection seam, so the posture is observable on a running router.

## 7. Design decisions the plan must make explicit

**D1 — The introducer-only snapshot's evidence expiry is the *earliest* expiry
among the introducers it publishes.**

The transport's snapshot contract is that a caller must treat an expired
snapshot as withdrawn. The introducer-only form's evidence is the introducer
set, and the set's members expire individually. The **minimum** expiry is the
fail-closed choice: the form is withdrawn the moment its first listed
introducer dies, so a refresh re-selects and the published set never carries a
dead introducer for longer than one publication cycle. The **maximum** would
be the permissive choice and is rejected: it would let a dead introducer sit
in a published address until the whole set had expired.

**D2 — The composition lives in the runtime, and the daemon branches on a
typed form, never on a refusal code.**

The runtime owns the facts the decision needs: the router's reachability
evidence, its bound sockets and its validated introducer set. The daemon
owns the install and the counters. If the daemon branched on
`Err(ReachabilityUnqualified)` it would re-implement the transport's error
taxonomy in the daemon, and a new refusal reason would silently change which
form the router publishes. A typed answer — `Direct(material)` or
`IntroducerOnly(material)` — makes the two forms **values** the daemon can
match on.

**D3 — The direct form is tried first and the introducer-only form is the
fallback for a reachability refusal only.**

A refusal that is not about reachability — the runtime is closed, no socket is
bound, the state is poisoned, the endpoint is invalid — is a **broken**
router, not a firewalled one, and is propagated unchanged. Only
`ReachabilityUnqualified` means "this router cannot be reached directly", and
only then is the introducer-only form attempted.

**D4 — The family capability is derived from the router's bound socket.**

The current builder hardcodes `"4"` and reports `bound_family: Ipv4`. That is
a factual claim about a router that may have no IPv4 socket at all. The
family is therefore derived from the **bound** sockets — IPv4 when the IPv4
socket is bound, otherwise IPv6 when the IPv6 socket is bound, otherwise the
form is refused — and exactly one family letter is published. The tree never
claims a family it has no socket for.

**D5 — The withdrawal stays where the reference would publish an empty
unpublished form.**

A firewalled router with no introducer keeps publishing the netdb's ordinary
zero-address record. The plan does not introduce the reference's `s`+`i`+`v`
unpublished form, because that is Plan 520's shipped path (§5.2) and the
netdb's ordinary builder is the tree's own withdrawal form.

**D6 — The counters distinguish the forms.**

`published` currently counts **every** successful install, including the
withdrawal, which makes it a count of "an install happened" and not of "an
address was published". The plan splits it: the direct form and the
introducer-only form are counted separately, and the withdrawal gets its own
counter. `published` remains the **sum** of the two published forms, so a
reader who wants the total still has one.

**D7 — The counters become observable.**

The self-publication counters exist and, before this plan, **no production
reader could see them**. The plan publishes them through the daemon's
existing `InspectionHandles` seam, on the pattern Plan 522's pool already
uses. A counter that cannot be read is not a refusal that was not silent; it
is a refusal nobody can audit.

**D8 — No new configuration key.**

`ssu2.advertise` already means "the router publishes its own SSU2 address".
The introducer-only form is that address in its other shape, and splitting
one behaviour across two switches would make the operator reason about which
switch produces an address. The **acquisition** keeps its own switch
(`ssu2.introducer_request`, Plan 524); the two are independent, and a router
that learned introducers before this plan, or by another route, publishes them
under the publication switch alone.

**D9 — The private-LAN refusal stays.**

`ssu2.advertise` is refused in the private-LAN profile because the direct
form can never be earned there. The introducer-only form publishes no
endpoint and would be safe in that profile, but a private-LAN router's
introducers are typically its LAN peers, and publishing them to the public
network is an operator decision this plan does not make on the operator's
behalf. The refusal stays; §14 records it as a limitation with its owner.

**D10 — The plan names no peer and publishes no peer.**

The published address carries introducer **hashes**, expiries and relay
tags, exactly as the specification's form defines them, and nothing else. The
plan adds no peer profile, no peer identity and no peer score.

## 8. Work packages

### WP1 — the runtime's introducer-only builder is made to work

The evidence expiry is derived from the introducer set's own expiry (D1), the
family capability is derived from the bound socket (D4), and the material's
remaining lifetime is truthful. A regression test proves the builder now
succeeds where it previously always refused, and that the refusal it reports
names the *actual* cause.

### WP2 — the runtime composes the two forms

A new typed composition answers which form the router publishes, under D2 and
D3. The composition is **one** answer with **two** shapes, never two
independent answers the daemon must reconcile.

### WP3 — the daemon publishes the form the runtime admitted

The owner consumes the typed form, hands the address to the netdb's matching
builder, and counts each outcome under its own reason (D6). The empty
introducer set becomes a reachable refusal.

### WP4 — the counters become observable

The self-publication counters are published through the existing inspection
seam (D7), on the pattern the peer pool already uses.

### WP5 — the lane, the guard, the documentation and the inventory

The lane drives the real runtime and the real daemon. The guard pins the
shape of the plan. The documentation and the support inventory record the new
surface.

## 9. Failure, cancellation, restart, and contention semantics

- **Failure.** Every refusal is counted under its own reason and leaves no
  state behind. A poisoned state mutex degrades to "no material", never to a
  fabricated one. A netdb refusal never signs a record.
- **Cancellation.** The plan adds no task of its own. The publication rides
  the daemon's **existing** publication cycle
  (`crates/i2pr-daemon/src/lib.rs:3666`), and the runtime's material builders
  are synchronous and non-awaiting. There is nothing to cancel and nothing to
  await.
- **Restart.** The introducer table is in-memory and is lost on restart, the
  same as every other session-scoped table in the tree. A router that restarts
  publishes the **withdrawal** until it has learned introducers again, which
  is the honest posture: publishing a stale introducer set would be worse
  than publishing none.
- **Contention.** The publication is bounded by the transport's own table
  ceiling, which **evicts the oldest** record when it is full
  (`crates/i2pr-transport-ssu2/src/introducer.rs:185`) and by the transport's
  own published-introducer bound. The plan adds no second bound and no second
  selection policy: the set the runtime publishes is the set the transport
  already selected.

## 10. Compatibility and migration review

- **No wire change.** The plan renders an address in the form Plan 523 put
  there. The proto crate, the option vocabulary and the parser are untouched.
- **A reading change, not a writing change, for peers.** A peer that reads
  this router's record may now find an address with introducers and no
  endpoint. The tree's own parser already accepts that form (Plan 523), and
  the specification defines it.
- **A writing change for this router's own record**, behind the existing
  default-off opt-in. A router with the opt-in on, that is firewalled and
  that has learned introducers, publishes a different address shape than it
  did before. This is the plan's purpose.
- **A public API change in the daemon.** The self-publication owner's
  `decide` takes the typed form rather than the direct material, and the
  counters split. The crate is experimental and the lane is updated with the
  plan; no external consumer exists.
- **No configuration migration.** No new key, no changed default, no changed
  meaning. The existing `ssu2.advertise` switch keeps its meaning and its
  refusals.
- **No persistence.** The published form is recomputed from live state on
  every publication cycle, so a restart cannot leave a stale form installed.

## 11. Required tests

### The runtime's introducer-only builder

- a router holding one live introducer builds the introducer-only material;
- the built address carries `ih0`, `iexp0` and `itag0` and **no** `host` and
  **no** `port`;
- the built address is accepted by the netdb's introducer rule;
- the snapshot's evidence expiry is the earliest expiry among the published
  introducers, and the material's remaining lifetime is that minus now;
- an **empty** introducer set is refused with `NoIntroducers`, and with no
  other reason;
- a builder that cannot satisfy the transport's evidence gate reports the
  transport's own cause and not a fabricated `InvalidPublication`.

### The runtime's composition

- a `Reachable` router with a bound loopback endpoint yields the **direct**
  form;
- a router whose reachability is unqualified and that holds introducers
  yields the **introducer-only** form;
- a router whose reachability is unqualified and that holds **no** introducer
  is refused with `NoIntroducers`;
- a closed runtime, a runtime with no bound socket and a poisoned state are
  each propagated unchanged and are **not** turned into the introducer-only
  form;
- the composition never yields both forms and never yields neither silently.

### The daemon's hand-off

- the direct form is signed by the netdb's **direct** builder and the
  introducer-only form by the netdb's **introducer** builder, and each is
  refused by the other;
- the introducer-only record carries **no** `f` capability;
- the published record's address carries no host and no port;
- the withdrawal still yields a record with **no** address;
- the direct form is still chosen for a router the direct form admits.

### The daemon's counters

- the two published forms are counted separately;
- the withdrawal is counted under its own reason;
- the empty introducer set is counted under its own reason, and the counter
  **moves**;
- every refusal the owner can produce is counted under its own reason.

### The negative controls

Every guard rule needs an inverted probe that fires it, and every probe
mutates an **in-memory copy** of the source, so no file on disk is written.
Every guard rule must **fail closed** when the item it names is absent: a
rule whose named item is missing is **reported**, not passed (Plan 523's
precedent, Plan 524's proof).

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_introducer_publication -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_introducer_publication_boundary -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
```

## 13. Documentation updates

- `specs/support.toml` — a new surface for the publication, `experimental` and
  `advertised = false`.
- `specs/CONFORMANCE.md` — the section for this plan and the inventory counts.
- `specs/protocols/09-ssu2.md` — the publication scope.
- `specs/QUALIFICATION-GATES.md` — the gates for this plan.
- `docs/architecture/i2pr-transport-ssu2.md` — the introducer-only form and
  its evidence.
- `docs/architecture/i2pr-runtime.md` and `docs/architecture/i2pr-daemon.md` —
  the composition, the hand-off and the counters.

## 14. Acceptance criteria and stop conditions

The plan passes when:

1. the runtime's introducer-only builder **succeeds** where it previously
   always refused, and its evidence expiry comes from the introducer set's
   own expiry;
2. the built address carries the specification's introducer groups and **no**
   `host` and **no** `port`, and the netdb's introducer rule accepts it;
3. a router whose reachability is unqualified and that holds introducers
   publishes the introducer-only address through the netdb's introducer
   builder;
4. a router whose reachability admits the direct form still publishes the
   **direct** address, and the direct form is unchanged;
5. the empty introducer set is refused under its own reason, and the
   counter that names it **moves**;
6. the counters distinguish the two published forms and the withdrawal, and
   they are readable through the inspection seam;
7. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, every rule fails closed on an absent item, and no file on
   disk is written by a probe;
8. the routine floor is green on the closing tree;
9. `specs/support.toml` records the surface as `experimental` and
   `advertised = false`; the advertised count stays **zero**;
10. `router.version` stays `0.9.62`; the published RouterInfo's `caps` is
    unchanged; `Cargo.toml` and `Cargo.lock` are byte-identical;
11. no boundary script is weakened.

**Stop conditions.** The plan stops and is re-scoped, rather than widened, if:

- making the introducer-only builder work turns out to require weakening the
  transport's evidence gate rather than passing it the right value;
- the composition turns out to need a second answer the daemon must reconcile;
- the hand-off turns out to need the daemon to infer a form from a refusal
  code;
- the counters turn out to need a new task, a new timer or a new listener;
- publishing the introducer set turns out to require the `C` capability, in
  which case the plan is re-derived and the `C` advertisement is named as a
  separate successor.

**Known limitations, recorded not hidden.**

- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology. The plan's evidence is a local, in-process lane.
- The **withdrawal** remains the tree's zero-address record, where the
  reference publishes an unpublished `s`+`i`+`v` form (§3.6). That divergence
  is Plan 520's path.
- The **`C` advertisement** is not this plan's subject (§5.2).
- The **private-LAN profile** keeps its `ssu2.advertise` refusal (D9).

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

- The plan's subject is the **publication** of the introducer set the router
  learned. The **acquisition** is Plan 524 and it is closed.
- The **`C` advertisement** — this router advertising its own introducer role
  — is a **third**, separate matter and needs its own plan-of-record.
- The plan **fixes** a latent defect in a function that had no production
  caller. The fix is in the **caller** (the evidence expiry it passes), not
  in the transport's gate, and the transport's gate is untouched.
- The **withdrawal** is unchanged: a firewalled router with no introducer
  publishes the netdb's ordinary zero-address record.

## 17. Amendment log

### 17.2 — 2026-11-09, the install gate could not admit the form

The registered §3 recorded **one** latent defect. The implementation found a
**second** one, which §3 did not record. §3 is evidence and is **not**
rewritten; this amendment adds the second defect forward.

The runtime's install gate
(`crates/i2pr-runtime/src/ssu2_runtime.rs`,
`install_local_router_info_inner`) required **every** SSU2 address the
record carries to name the router's own bound **endpoint**: `parsed.endpoint()`
had to be `Some`, and that endpoint had to be one of the router's own bound
sockets. The introducer-only form has **no** endpoint — that is what the form
*is* (`libi2pd/RouterInfo.cpp:1377` and `:1444` omit `host` and `port` when
the router is not published directly). The gate therefore refused the form
with `InvalidPublication` before the record could be installed, and the
plan's publication could not have landed with the gate unchanged.

The fix binds the router's **own** identity and contact material **by the
address's form**. The transport's parser refuses an address carrying both a
direct endpoint and introducers (`Ssu2AddressError::HostPortWithIntroducers`),
so the two forms are exclusive and `Ssu2RouterAddress::address_class()` is a
total classification:

| Class | The gate requires |
|---|---|
| `Direct` | the router's own bound endpoint, with the router's own static and intro keys |
| `IntroducerOnly` | the router's own static and intro keys, and at least one introducer |
| `UnpublishedStatic` | refused — the address names no contact at all |

The gate's **subject** is unchanged: it binds the router's own address, and
the introducer-only form is bound by its keys and its introducers rather than
by an endpoint it does not have. The **introducer set** itself is not
re-verified by the gate: it is the runtime's own validated table's product,
the runtime's material builder is its only source, and the netdb's rule
verifies the set's **form**.

**Plan 520's guard is not weakened.** Its rule 12 — "the install gate binds
every SSU2 address the record carries to the router's own endpoint" — still
requires the gate to contain the `is_bound` binding, and it still does, for
the direct form. The rule's **subject** is unchanged; the gate now
distinguishes the two forms instead of demanding an endpoint of both.

**Plan 520's guard rule 2 is updated, and strengthened.** The owner must now
ask the runtime for the **composed** form
(`service.self_publication_material(`), which is strictly stronger than
asking for one form alone — an owner that asked for one form only would
publish the direct address and never the introducer-only one. A second
inverted probe reports exactly that owner. The rule's **subject** — "the
daemon holds no copy of the publication rule; it asks the runtime" — is
unchanged.

### 17.3 — 2026-11-09, the citation drift after the implementation

The implementation moved the lines the registered §3 cites. §3 is evidence and
is **not** rewritten; this table points each drifted citation at its current
site. The citations that did **not** move are named at the end.

| The registered §3 citation | Its current site | What moved |
|---|---|---|
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2586` | `:2618` | the `Ssu2PublicationForm` enum and the material's `form` field were added above |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2612` | `:2664` | the builder's body was rewritten |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2613` | `:2665` | the evidence expiry is now the set's own |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2617` | `:2669` | as above |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2636` | `:2689` | as above |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2654` | `:2772` | the composition and the bound-family helper were added above |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2696` | `:2814` | as above |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:1101` | `:1117` | the `Ssu2PublicationForm` enum was added above |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:1126` | `:1147` | as above |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:1848` | `:1869` | as above |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2495` | `:2516` | as above |
| `crates/i2pr-daemon/src/self_publication.rs:157` | `:181` | the counters gained a field |
| `crates/i2pr-daemon/src/self_publication.rs:302` | `:342` | the counters gained fields |
| `crates/i2pr-daemon/src/self_publication.rs:357` | `:397` | as above |
| `crates/i2pr-daemon/src/self_publication.rs:397` | `:459` | as above |
| `crates/i2pr-daemon/src/lib.rs:3666` | `:3667` | the counter snapshot is published beside the refresh |

The citations that did **not** move and remain valid as registered:
`crates/i2pr-runtime/src/ssu2_peer_relay.rs:900` and `:1119`;
`crates/i2pr-transport-ssu2/src/publication.rs:126`, `:212`, `:239`, `:326`,
`:389` and `:544`; `crates/i2pr-transport-ssu2/src/introducer.rs:137`, `:185`
and `:253`; `crates/i2pr-netdb/src/local.rs:268` and `:292`;
`crates/i2pr-netdb/src/floodfill_role.rs:277`;
`crates/i2pr-daemon/src/router_i2np.rs:2298`; and
`crates/i2pr-daemon/src/floodfill.rs:706`.

The **new** sites the implementation added, which §3 could not have cited:

| The construct | Its site |
|---|---|
| `Ssu2PublicationForm` | `crates/i2pr-runtime/src/ssu2_runtime.rs:1105` |
| `Ssu2RuntimeService::bound_family` | `crates/i2pr-runtime/src/ssu2_runtime.rs:2705` |
| `Ssu2RuntimeService::self_publication_material` | `crates/i2pr-runtime/src/ssu2_runtime.rs:2746` |
| the install gate's form-aware binding | `crates/i2pr-runtime/src/ssu2_runtime.rs:3872` |
| `SelfPublicationFailure::UnqualifiedIntroducerAddress` | `crates/i2pr-daemon/src/self_publication.rs:94` |
| `SelfPublicationOwner::build_record`'s form match | `crates/i2pr-daemon/src/self_publication.rs:459` |
| `SelfPublicationCounterCell` | `crates/i2pr-daemon/src/i2pcontrol_inspection.rs:94` |
| `InspectionHandles::publish_self_publication_counters` | `crates/i2pr-daemon/src/i2pcontrol_inspection.rs:791` |

### 17.1 — 2026-11-09, registration

Plan 525 registered. The plan's subject is the publication half of the SSU2
introducer composition, discharging the roadmap's completion definition's
**publishing** half. The acquisition half is Plan 524; the `C` advertisement
is a third, separate matter and is out of scope.

The registration records one defect that is **not** visible in production
today: `Ssu2RuntimeService::introducer_publication_material` cannot succeed,
because it passes the same second for the build time and the evidence expiry
and the transport's tested gate therefore always withdraws the snapshot
(§3.1). The defect is a **failing gate**, not a missing one: the transport's
gate is correct, and the caller passes the wrong value into it. The plan fixes
the caller.

D2 and D4 are recorded as the plan's two explicit decisions where the tree
held a shortcut: the composition lives in the runtime and the daemon branches
on a typed form rather than on a refusal code, and the family capability is
derived from the router's bound socket rather than hardcoded to `"4"`.

No configuration key is added (D8): `ssu2.advertise` already means "the
router publishes its own SSU2 address", and the introducer-only form is that
address in its other shape.
