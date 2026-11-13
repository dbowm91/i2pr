# Plan 546 — the loopback interlock on publication must become an operator
precondition

**Status:** registered. **Subsystem:** network-participation.
**Registered:** 2026-11-12. **Successor to:** Plan 545.

## 1. Objective

Let the router publish its **own bound SSU2 address**, and let the floodfill
role qualify on it, when the operator has explicitly accepted that the host is
directly reachable at that address.

Today the router can publish an SSU2 address only when it is bound to
**loopback**, so the address it publishes is `127.0.0.1`. Everything built on
publication therefore works only in a loopback lab: `ssu2.advertise` is inert,
the floodfill role evaluates to `Disabled` in every posture, and no router can
be a floodfill for another machine. Plans 544 and 545 made two hosts find each
other and hold sessions; they did not make either host *addressable*.

**This is a policy change, and it is the operator's.** The loopback-only
predicates were never an architectural invariant. They exist to stop the router
from publishing an address it cannot serve, because a router that advertises a
reachability it does not have is a router that gets **blocked** by the peers it
wastes — and once a public address is widely blocked, testing against the live
network becomes much harder. That reasoning is sound and it is retained: what
changes is **who holds the judgement**. An operator who knows a host is
directly reachable may say so; a default build may not.

## 2. Why ready

**Three predicates, each of them one line, and each already named.**

1. `Ssu2RuntimeService::publication_material` refuses every non-loopback bound
   socket (`crates/i2pr-runtime/src/ssu2_runtime.rs`, the `is_loopback` arm
   beside the reachability check), so on an RFC1918 or public bind there is no
   publication material at all.
2. `note_explicit_bind_for_controlled_qualification` refuses the same binds
   (`crates/i2pr-runtime/src/ssu2_runtime.rs`), and that is the **only**
   production source of a `LocalConfiguredBind` corroboration.
3. `floodfill::is_normal_qualified_address`
   (`crates/i2pr-daemon/src/floodfill.rs`) requires the published host to pass
   `is_publicly_routable_host`, which rejects `is_private()`.

**And the gate is load-bearing in a way worth stating.** In production
`ReachabilityState::Reachable` arrives by exactly one route:
`explicit_bind_corroboration` is set unconditionally by the daemon
(`crates/i2pr-daemon/src/router_i2np.rs`), and the corroboration is recorded
only through the loopback-gated bind note. `note_observed_external_address` has
**no production caller**, and `note_peer_test_outcome` is reached only from the
runtime's own **controlled** verdict path. So predicate 2 is not decoration: it
is the switch that decides whether the runtime will make a reachability claim
at all.

**A peer test exists, and it does not settle this.** `PeerTestOwner`
(`crates/i2pr-daemon/src/peer_test.rs`, Plan 521) is built from
`ssu2.peer_test`, but two facts keep it from being a reachability source here:
it is constructed inside `ServiceProduct`, which the daemon builds only when
`service_tunnels_active(config)` is true -- the same gate Plan 542 found -- and
its verdicts are never fed to the runtime's reachability, because
`note_peer_test_outcome` is called only from the runtime's controlled path. So
the successor named in §5.3 is **not** "build a peer test"; it is "carry the
existing owner's verdict into the runtime's reachability", which is a smaller
and better-defined job.

**What exists and is used.** The runtime already carries the whole
corroboration model (`i2pr-transport/src/reachability.rs`): a
`LocalConfiguredBind` observation, an expiry, and a `Reachable` state that
`publication_material` reads. The three predicates are the only thing between
the model and a record that names a real address.

## 3. Current implementation evidence

### 3.1 The clamp

```text
// crates/i2pr-runtime/src/ssu2_runtime.rs
if !bound.ip().is_loopback() {
    return Err(Ssu2PublicationUnavailable::InvalidPublication);
}
```

It sits **after** the reachability check, so reachability is not the only thing
refusing a non-loopback bind; the clamp refuses independently.

### 3.2 The corroboration is loopback-only too

```text
// crates/i2pr-runtime/src/ssu2_runtime.rs
if bound.port() == 0 || !bound.ip().is_loopback() {
    return Err(Ssu2PublicationUnavailable::InvalidPublication);
}
```

### 3.3 The floodfill re-checks routability

```text
// crates/i2pr-daemon/src/floodfill.rs
fn is_normal_qualified_address(material: &Ssu2PublicationMaterial) -> bool {
    ... is_publicly_routable_host(host)
}
```

The comment above `is_publicly_routable_host` records the intent plainly: the
loopback arm "already rejects every address the runtime can produce", so the
function is defence in depth over a runtime that cannot produce a public
address in the first place.

### 3.4 What this plan observed on the three-host LAN

With `[floodfill] enabled = true` and `qualification_profile` absent (that is,
`Normal`), `192.168.182.20` logs:

```text
normal floodfill opted in; evaluating eligibility requested=true role="Disabled"
bootstrap pipeline completed state=reseed-required record_count=2 floodfill_advertisers=0
```

and the router's own publication reports `outcome=RefusedNoTarget`. So on the
LAN every node is a client, and the correlated two-way reply (T4) has no target
to correlate with.

### 3.5 The precondition this plan cannot remove

`Reachable` from `LocalConfiguredBind` means "the operator configured this
address and the socket is bound to it". That is **true** for a host that is
directly reachable at its bound address — a lab LAN, or a public host with no
NAT in front of it — and **false** for a host behind NAT, where the bound
address is not the address peers see. A router behind NAT that publishes its
bound address is exactly the misbehaving router the interlock existed to
prevent.

This plan does not attempt the peer test that would settle the NAT case. That
is named as a successor in §5.3.

## 4. Invariants

1. **The default posture does not change.** With the new opt-in absent, every
   published address is still loopback, `Reachable` is still reached only
   through a loopback bind, and the floodfill still evaluates to `Disabled`.
   A build that does nothing new behaves exactly as it does today.
2. **The opt-in is explicit, typed and documented**, not inferred from a bind.
   A non-loopback bind alone changes nothing.
3. **The runtime remains the authority on what may be published.** The daemon
   does not decide that an address is publishable; it asks the runtime for
   material and accepts what it gets. The floodfill's routability re-check
   becomes "the material exists, because the runtime produced it".
4. **Reachability is still required.** The relaxation admits an address; it
   does not admit an unreachable one. `publication_material` keeps its
   `Reachable` and expiry tests unchanged.
5. **No new listener, no new port, no new dependency.** The change replaces a
   predicate; it does not add a surface.
6. **No advertisement beyond what the operator asked for.** `router.version`
   stays `0.9.62` and the inventory's advertised count stays **zero**; a
   floodfill record still carries `f` only when the operator enabled the role.
7. **The NAT precondition is documented as a precondition, not as a caveat.**
   The config key's own documentation says a NAT'd host must not set it.
8. **The predecessors' lanes and guards stay byte-identical.**

## 5. Scope

### 5.1 In scope

- `crates/i2pr-runtime/src/ssu2_runtime.rs` — the two loopback predicates and
  the config field that relaxes them.
- `crates/i2pr-daemon/src/config.rs` — the `[ssu2] publish_bound_address` key.
- `crates/i2pr-daemon/src/floodfill.rs` — the qualification arm.
- The daemon wiring that builds the runtime config.

### 5.2 Out of scope

- The production SSU2 peer test.
- Any NAT traversal, hole punching or introducer work.
- The stock-router admission question (Plan 493).
- Bandwidth classes, `R`, and the transit posture.
- Any change to `caps` beyond what the operator's existing keys already imply.

### 5.3 Out of scope, with the owner named

- **Feeding a peer test's verdict into runtime reachability.** Without it,
  `Reachable` for a NAT'd host cannot be established honestly, so live-network
  testing from behind NAT stays blocked. The pieces exist but are not joined:
  `PeerTestOwner` (Plan 521) is built from `ssu2.peer_test` inside
  `ServiceProduct`, behind `service_tunnels_active(config)`, and nothing calls
  `Ssu2RuntimeService::note_peer_test_outcome` with its verdict.
  **Owner:** a successor plan that joins them. It is a wiring job, not a build.
- **The floodfill's serving duties on a live network.** This plan makes the
  role *reachable*; whether the router's serving path is correct against real
  peers is Plan 493's question and the external lanes' evidence.
- **`note_observed_external_address`'s missing production caller.** Recorded as
  a finding in §16; a successor either wires it from the handshake or retires
  it.

## 6. Required production changes

### 6.1 `crates/i2pr-runtime/src/ssu2_runtime.rs`

1. `Ssu2RuntimeConfig::publish_bound_address: bool`, default `false`, documented
   with the NAT precondition and the blacklist consequence.
2. `publication_material`: the loopback arm becomes
   `!bound.ip().is_loopback() && !self.shared.config.publish_bound_address`.
3. `note_explicit_bind_for_controlled_qualification`: the same relaxation, so
   the corroboration that makes `Reachable` possible is available on the same
   terms.

### 6.2 `crates/i2pr-daemon/src/config.rs`

`[ssu2] publish_bound_address` — bool, default `false`, `deny_unknown_fields`
retained, with validation that it requires `ssu2.enabled`.

### 6.3 `crates/i2pr-daemon/src/floodfill.rs`

`is_normal_qualified_address` takes whether bound-address publication is
enabled. When it is, the material's own address is accepted (the runtime
produced it, and it is the authority); when it is not, the public-routability
arm is unchanged.

## 7. Design decisions

### 7.1 The opt-in is a separate key, not a new qualification profile

`ssu2.qualification_profile` names a **dial** posture (what the router is
willing to dial). Publication is the other direction. Conflating them would
mean that changing a dial profile silently widened publication, which is
precisely the coupling this plan exists to avoid.

### 7.2 The default is unchanged, and the guard proves it

The strongest evidence this plan can offer is that a build without the key
produces **byte-identical** records to today's. The guard pins the predicates'
shape and the lane pins the behaviour, including the default.

### 7.3 The floodfill stops re-checking routability when the operator opted in

Two authorities for one question is how they drift. The runtime already decides
whether an address may be published -- reachability, expiry, not-unspecified,
port bound -- and the daemon's re-check was defence in depth over a runtime that
could not produce a public address. Once the runtime can, the re-check either
agrees (and is dead code) or disagrees (and is a bug). It becomes "the runtime
said yes", which is one authority.

### 7.4 The key's documentation names the failure it can cause

A config key that can get the operator's address blocked carries that sentence
in its own documentation, next to the NAT precondition. Documentation that
lives only in a plan is documentation the operator will not read.

## 8. Work packages

1. **WP1 — the runtime predicates and the config field.**
2. **WP2 — the daemon key, validation and wiring.**
3. **WP3 — the floodfill qualification arm.**
4. **WP4 — the lane**: the default posture is unchanged; the opt-in publishes
   the bound address; a NAT'd-shaped configuration is documented as a
   precondition rather than tested for.
5. **WP5 — the guard**: the predicates, the default, the one-authority rule.
6. **WP6 — the negative controls.**
7. **WP7 — the documentation and the inventory.**
8. **WP8 — the three-host measurement**, with `.20` as the floodfill.
9. **WP9 — the floor and the closure record.**

## 9. Failure, cancellation, restart and contention semantics

**Failure.** A refused publication is unchanged: the failure is typed
(`Ssu2PublicationUnavailable`) and categorical, and the daemon counts it. The
relaxation changes which binds may succeed; it does not change what a failure
looks like.

**Cancellation.** No new await, no new task, no new channel. The predicates are
read during a call that already exists.

**Restart.** Nothing is persisted. A restart re-derives publication from the
config and the live bind.

**Contention.** The relaxed branch reads one bool from the config that the
service already holds. No new lock.

## 10. Compatibility and migration

**No wire change.** The RouterInfo encoding and signature are untouched; what
changes is which address may appear in one.

**One new configuration key**, default `false`. An existing config file parses
and behaves identically.

**No dependency change.** No manifest change.

**Reversibility.** Removing the key returns the router to today's posture with
no state to migrate.

## 11. Required tests

### 11.1 The lane

- With the key absent, `publication_material` still refuses a non-loopback bind
  and still refuses a loopback bind that is not `Reachable`.
- With the key present and the bind corroborated, material names the **bound**
  address.
- The floodfill's qualification accepts that address only when the opt-in is
  set, and the refusal reason is unchanged when it is not.

### 11.2 The guard

The default posture is unchanged; the two predicates are relaxed by one shared
condition and not two; the reachability and expiry tests are untouched; no
listener, socket or dependency is added; nothing is advertised; the
predecessors' evidence is byte-identical.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --test publication_bound_address -- --test-threads=1
cargo test --locked -p i2pr-daemon --test publication_bound_address_boundary -- --test-threads=1
```

plus the routine floor, with this plan's steps added.

## 13. Documentation updates

`specs/support.toml` (surface `ssu2.bound-address-publication`),
`specs/CONFORMANCE.md`, `specs/QUALIFICATION-GATES.md`,
`docs/architecture/i2pr-runtime.md`, `docs/architecture/i2pr-daemon.md`, and
this plan's amendment log.

## 14. Acceptance criteria and stop conditions

Accepted when the default posture is byte-identical, the opt-in publishes the
bound address, the floodfill qualifies on it, reachability and expiry still
gate, no surface or dependency is added, nothing is advertised, `.20` serves as
a floodfill on the LAN, and a peer's publication is confirmed by a
`DeliveryStatus`.

**Stops** if the change would require a second authority for "may this address
be published", if it would let a bind corroborate reachability without an
explicit operator key, or if it would advertise a capability beyond `f` on a
record the operator asked for.

## 15. Closure evidence required

Commits; the floor table; lane and guard rows; negative controls with the
restoration proof; the requirement-to-evidence matrix; the invariant, failure,
migration and security reviews; limitations, findings, roadmap disposition and
the unblock audit.

## 16. Handoff notes

- **The default must not move.** If the guard cannot prove that, the plan has
  failed regardless of whether the LAN works.
- **A NAT'd host must not set `publish_bound_address`.** The key's own
  documentation says so. Live-network testing from behind NAT needs the peer
  test, not this key.
- The interlock's original purpose -- not getting the address blocked -- is
  **retained as a precondition**, not deleted. Say it that way in every
  document this plan touches.
- `note_observed_external_address` still has no production caller when this
  plan closes; that is recorded, not fixed.
- The predecessors' closure records are **not** rewritten.

## 17. Amendment log

### 17.1 — 2026-11-12, registration

Registered after the three-host LAN measurement showed every node at
`role="Disabled"` with `floodfill_advertisers=0`, and after reading the three
loopback predicates that produce it. The operator's direction is recorded
because it is the plan's whole justification: the loopback gates were a
blacklist interlock, not an architectural invariant, and live-network testing
requires relaxing them.

### 17.2 — 2026-11-12, one predicate instead of two relaxed conditions

§6.1 asked for the loopback arm of `publication_material` to gain a condition.
The implementation goes one step further and extracts
`may_publish_bound_address`, which **both** publication entry points now call.

The reason is the guard's first rule. Two relaxed conditions are two places for
the rule "may this socket's address appear in a RouterInfo" to live, and the
second one is the load-bearing one: it is the only production source of a
`LocalConfiguredBind` corroboration, and therefore the only production route to
`Reachable`. Relaxing only the material arm would have left the runtime unable
to reach `Reachable` on a non-loopback bind at all, so the opt-in would have
done nothing — but that would have taken a lab run to discover. One predicate
makes the coupling visible in one place, and `the_predicate_is_one_authority`
fails if either site restates the loopback test.

`normal` is unchanged in every other respect: the `Reachable` and expiry tests
stay where they were, the unspecified/port checks move into the predicate, and
the default is still off.

### 17.3 — 2026-11-13, the measurement: the predicate works, the role does not

Measured on the three-host LAN after deploying this plan's binary to
`192.168.182.20` with `[ssu2] publish_bound_address = true`:

```text
normal floodfill opted in; evaluating eligibility requested=true role="Disabled"
the re-dated RouterInfo was not installed reason=InvalidPublication
router RouterInfo publication transition="router_info_publication" outcome=RefusedNoTarget
```

The relaxation is **necessary but not sufficient**, and §14's acceptance
criterion is **not met**. Two gates remain, and neither was identified when this
plan was registered.

**1. Nothing on the normal path records a configured-bind corroboration.**
`note_explicit_bind_for_controlled_qualification` is called only by
`activate_controlled`, which only the `controlled_loopback` floodfill profile
reaches. The `Normal` profile therefore never reaches
`ReachabilityState::Reachable`, and `publication_material` refuses on
reachability **before the address predicate is consulted at all**. Relaxing the
predicate changed which binds *could* be published; it did not create a caller.
This is the same shape this repository has closed three times now — mechanism
present, caller absent — and it is the reason the plan's own §3.2 warning about
the corroboration being load-bearing was correct but incomplete: being
load-bearing is not the same as being reachable.

**2. The re-dating path cannot install a dual-transport record.**
`crates/i2pr-daemon/src/router_i2np.rs` re-dates the installed record and calls
`install_local_router_info(fresh, now)`, which passes
`controlled_ntcp2_endpoint: None`; the install gate then requires
`ntcp2_addresses.is_empty()`, and the record this lab carries has one NTCP2
address beside its one SSU2 address. The refusal is **pre-existing** — it
predates this plan and appears in every earlier lab run — but it sits directly
on the path to a floodfill record, so this plan inherits it.

Both are recorded rather than fixed. Each needs its own decision — the first
about **what corroborates** reachability on a non-loopback bind, the second
about **which** install form the re-dating path should use — and neither is a
predicate, which is all this plan set out to change.

**The plan therefore stays `registered`.** What it delivered is real and proven
locally: one predicate, one operator key with the precondition in its own
documentation, one qualification arm, a lane of 13 rows and a guard of 14
(7 rules, 7 inverted probes), plus 9 negative controls that all fired. What it
did not deliver is a router that serves as a floodfill, and the plan says so
instead of closing.

**What a successor needs, in order.** First, decide what corroborates
reachability when the operator has declared the address directly reachable —
today's `LocalConfiguredBind` observation is the candidate, and it must be
recorded on the normal path, which means a caller and a gate rather than a
predicate. Second, decide how the re-dating path installs a record whose form
the install gate refuses: either plumb the NTCP2 endpoint through, or re-date
only the SSU2 address. Then the floodfill role has material and a record it can
install, and a peer's publication can be correlated.

### 17.4 — 2026-11-13, a closed guard's rule was retargeted, not deleted

The floor's workspace test step failed on
`ssu2_explicit_bind_corroboration_boundary`, rule
`the_controlled_path_is_unchanged`, with *"the controlled-qualification
recorder must keep refusing a non-loopback bind"*. That rule asserted the body
of `note_explicit_bind_for_controlled_qualification` contains `is_loopback()`,
which is precisely the predicate this plan moved into
`may_publish_bound_address`.

The rule is **retargeted, and it is now stronger**: the recorder must reach
`may_publish_bound_address(`, and it must **not** restate `is_loopback()`. So
the policy still cannot be bypassed, and it can no longer be duplicated. The
closed plan's record is not rewritten; this is the successor's correction, and
it is recorded here because a reader of that plan will not find it.

This is the third time in this repository's recent history that a deliberate
policy change has surfaced as a closed guard failing rather than as a compile
error — Plan 503's DSM rule and Plan 542's `PruneOutcome` link were the other
two — and in all three cases the guard was right to fail: it encoded a fact the
successor was changing.
