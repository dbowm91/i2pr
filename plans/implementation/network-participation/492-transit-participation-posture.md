# Plan 492 — Transit participation: an operator-opted, bounded, revocable relay posture

**Status:** registered, ready to execute (phase 5 of
[network participation](../../subsystems/network-participation-roadmap.md))
**Owner:** `i2pr-daemon` (the participation owner and its policy),
`i2pr-tunnel` (the transit state machine, unchanged), `i2pr-runtime` (the
socket and timer ownership)
**Baseline:** `work/plans-477-480-router-recovery@c43f3404`
**Authoritative predecessor:** `plans/closure/transit-tunnels/268-status.md`
(the passed one-family qualification), `plans/closure/core-router-recovery/454-status.md`
(the bandwidth governor and share accounting)
**Research input:** the I2P specification's tunnel-implementation and
transit pages, the pinned i2pd 2.61.0 and Java I2P 2.13.0 transit
behaviour, and the Emissary router's participation defaults and share
limits (as a design reference only — ADR 0028 §7 excludes its source from
reuse on this branch)
**Predecessor lineage:** 254/268 (transit ingress owner, one-family
qualification) → 488 (profile store, selection policy) → 491 (version
floors) → **492**

## Objective

Turn the **existing** transit machinery on for an operator-opted profile:
a router that deliberately **relays other routers' tunnel traffic**, under
an explicit, bounded, revocable participation policy — a share limit, a
tunnel-count limit, a selection policy that keeps the router's own tunnel
hops diverse, and a withdrawal path that takes effect promptly.

One bounded outcome: **the daemon can participate in transit tunnels under
an explicit opt-in with a bounded share, a bounded tunnel count, and a
revocation that is observed in the receipts.** The capability is internal,
opt-in, and non-advertised beyond what the evidence supports.

### The state of the machinery

This plan is deliberately mostly about **policy and ownership**, not about
new protocol code. The transit ingress owner exists
(`crates/i2pr-daemon/src/transit_owner.rs`, Plan 254's live owner) with the
dispatch order it documents — *creator/service first, transit second,
unknown fail-closed* — and the participation states exist
(`crates/i2pr-tunnel/src/transit.rs`: fully enabled, enabled but shut down,
disabled). The M11 lane qualified one family. What is missing is a
*posture*: who may turn it on, how much, for how long, and how it stops.

## Why ready

- **Hard dependencies are closed.** The transit ingress owner and the
  runtime-neutral transit state machine are closed; the M11 one-family
  qualification passed; Plan 454's governor provides the share accounting
  the posture needs; Plan 488's selection policy supplies the diversity
  rules for the router's own tunnel hops.
- **Interface dependencies are stable.** The transit dispatch order, the
  participation states, and the governor's permit are closed surfaces.
- **No external topology is needed for the posture itself.** The posture
  and its controls are testable in a private mesh; only the *public*
  participation claim needs the two-family lane (Plan 493).

## Current implementation evidence

Present today:

- `crates/i2pr-daemon/src/transit_owner.rs` — the transit ingress owner:
  it receives a decoded tunnel-build or tunnel-data message, decides by the
  documented dispatch order, and owns the transient per-request state.
- `crates/i2pr-tunnel/src/transit.rs` — the runtime-neutral transit state
  machine and the participation states (fully enabled; enabled but shut
  down; disabled), with the data-plane and envelope rollback/drain
  behavior.
- `crates/i2pr-daemon/src/transit_volume.rs` and `transit_compose.rs` —
  the volume accounting and the composition helpers.
- `crates/i2pr-daemon/src/bandwidth_eligibility.rs` and Plan 454's
  `BandwidthEnforcementPermit`/`BandwidthGovernor` — the share accounting
  and the revalidation at the point of use.
- `crates/i2pr-tunnel/src/pool.rs` — the tunnel pool that a participating
  router's own tunnels come from.

Missing today:

- **no participation posture** — no configuration surface decides whether
  the router participates, at what share, and with how many tunnels;
- **no revocation** — nothing observes a policy change, a health loss, or
  a governor permit withdrawal and takes the router out of participation
  promptly;
- **no participation evidence in the receipts** — the router cannot show
  what it relayed, how much, and why it stopped;
- **no selection diversity** — the router's own tunnel hops are not
  constrained by the /16 rule or a participation cap in the ordinary path;
- the public posture stays **disabled, non-advertised, and unclaimed**
  (`specs/support.toml`, `m11_transit_tunnels`).

## Invariants that must not regress

1. **Opt-in only, and default off.** A router that has never enabled
   participation does not participate, and the default configuration must
   not change.
2. **The dispatch order is unchanged.** *Creator/service first, transit
   second, unknown fail-closed.* The transit path may not be given
   priority over the router's own service traffic.
3. **The share limit is enforced, not advisory.** The governor's permit is
   revalidated at the point of use; exceeding the share stops the
   participation rather than growing it.
4. **Revocation is prompt and observable.** When participation is turned
   off, when the permit is withdrawn, or when health is lost, the router
   stops accepting transit work within a bounded time and the receipts show
   the stop.
5. **Bounded everything.** The participating tunnel count, the per-peer
   participation cap, the buffered volume, the outstanding transit
   requests, and the accounting windows are bounded by named constants.
6. **No secret in receipts.** Volume and counts only — no peer addresses, no
   tunnel ids, no payload, no keys.
7. **The role is not advertised** beyond the evidence; the published
   capability string must not claim more than the qualified subset.

## Scope

**In:**

- the **participation policy**: the configuration surface (enabled, share
  limit, participating tunnel count, the per-peer cap), default-off, with
  the bounded constants named and documented;
- the **owner**: the daemon-side participation owner that admits or refuses
  transit work, applies the share and tunnel-count limits, and keeps the
  volume accounting;
- the **revocation path**: a policy change, a permit withdrawal, or a
  health loss stops participation within a bounded time, and the stop is
  recorded;
- the **selection diversity** for the router's own tunnel hops (the /16
  rule and the per-peer participation cap, from Plan 488's policy);
- the **receipts**: what was relayed, how much, under which limits, and
  when participation stopped;
- the offline controls and the private-mesh lane.

**Explicitly out:**

- **new transit protocol code** — the state machine and the ingress owner
  are closed; this plan is the posture, not a rewrite;
- **the public-network participation claim** (Plan 493);
- **the floodfill role** and `caps=f` (Plan 494);
- **bandwidth metering** — the governor's share accounting is used as it
  stands; a real metering implementation is a separate, larger piece of
  work and is open in the reference implementation too;
- any change to the **client's** own tunnel building semantics beyond the
  diversity rule;
- **public-network** operation and any advertisement.

## Required production changes

- **`crates/i2pr-daemon/src/transit_participation.rs`** (new): the
  participation owner and policy — the admission decision, the share and
  tunnel-count enforcement, the revocation handling, and the receipts.
- **`crates/i2pr-daemon/src/config.rs`**: a `[transit]` section with
  `participation` (default **off**), `share_limit`, `participating_tunnels`,
  and `peer_participation_cap`, all bounded and documented; the section is
  `deny_unknown_fields` like every other section.
- **`crates/i2pr-daemon/src/transit_owner.rs`**: the dispatch to the
  participation owner, preserving the documented order.
- **`crates/i2pr-client`**: the selection diversity rule for the router's
  own tunnel hops, consuming Plan 488's policy.

## Ordered work packages

1. **WP1 — the policy and the configuration.** The keys, the bounds, the
   defaults, and the negative test that the default posture is *not*
   participating.
2. **WP2 — the owner.** The admission decision, the share limit, the
   tunnel-count limit, the per-peer cap, and the typed refusals.
3. **WP3 — revocation.** The policy-change path, the permit-withdrawal
   path, and the health-loss path; each stops participation within a
   bounded time and is counted.
4. **WP4 — the selection diversity.** The /16 rule and the participation
   cap applied to the router's own tunnel-hop selection, with the
   determinism and the bounded-output tests.
5. **WP5 — the receipts.** Relayed volume, refusals by reason, and the
   stop events; counts only.
6. **WP6 — the lane.** A private-mesh lane where a peer's transit traffic
   is actually relayed by this router, and where participation is then
   revoked and the stop is observed.
7. **WP7 — the controls and the docs.** The offline controls, the inverted
   probes, the support-inventory entry, and the architecture docs.

## Failure, cancellation, restart, and contention semantics

- **A transit request that exceeds the share** is refused with a typed
  reason and counted; the router does not grow past its limit.
- **A cancellation mid-relay** releases the transient state and the
  buffered volume; the accounting stays consistent.
- **A restart** starts with participation **off** unless the operator's
  configuration enables it, and it re-derives the accounting from zero —
  a restart never resumes a share counter mid-window.
- **A health loss** stops participation (the governor's permit is
  revalidated at the point of use) and the stop is recorded.
- **Contention**: the participation owner is single-writer; the volume
  accounting is updated under the governor's own synchronization; no lock
  is held across an `.await` that can block on another lock holder. The
  recorded lock order (**bridge → coordinator**) still holds.

## Compatibility and migration

- **No wire change.** The transit protocol is unchanged.
- **No `router.version` change.**
- **No migration.** The posture is new and off by default; a router that
  does not enable it behaves exactly as it does today.
- **No advertisement.** The published capability string is not changed by
  this plan; changing it is a separate, evidence-gated decision.

## Required tests

**Offline controls**:

- the default configuration does not participate;
- the share limit and the tunnel-count limit are enforced and counted;
- the per-peer cap is enforced;
- revocation by policy change, by permit withdrawal, and by health loss
  stops participation within the bound, and each stop is recorded;
- the /16 diversity rule and the determinism of the hop selection;
- a restart resets the accounting and starts from the configured posture;
- an inverted probe for each invariant.

**Private-mesh lane**: this router relays a peer's transit traffic (a
source-originated receipt proves the relay), then participation is revoked
and the stop is observed in the receipts.

## Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-m11-transit-boundaries.sh
bash scripts/check-m11-transit-qualification-evidence.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

## Documentation updates

- `docs/architecture/i2pr-daemon.md` — the participation owner, the
  configuration, the revocation semantics;
- `docs/architecture/i2pr-tunnel.md` — the participation states and the
  posture that sits on top of them;
- `docs/architecture/i2pr-runtime.md` — the socket and timer ownership for
  the participation owner;
- `specs/support.toml` — this plan's status key; the
  `m11_transit_tunnels` posture entry only after the lane has passed;
- `plans/subsystems/network-participation-roadmap.md` and
  `plans/registry.md` — the phase's row.

## Acceptance criteria

1. The default configuration does not participate, and the disabled path
   is provably inert.
2. With participation enabled, the router relays a peer's transit traffic
   and the relay is visible in a source-originated receipt.
3. The share limit, the participating tunnel count and the per-peer cap are
   enforced and counted; exceeding the share refuses rather than grows.
4. Revocation by policy change, permit withdrawal, or health loss stops
   participation within the bound, and the stop is recorded.
5. The router's own tunnel hops satisfy the /16 diversity rule and the
   participation cap, deterministically.
6. A restart starts from the configured posture with a reset accounting.
7. The offline controls pass, and each invariant has an inverted probe.
8. The routine floor is green on the closing tree, with the step table in
   the closure record.
9. The closure record states that participation is **opt-in and
   non-advertised**, and that no public-network claim is made.

## Stop conditions

Stop and record a `stopped-*` closure when:

- the share or tunnel-count limit cannot be enforced without the governor's
  permit being advisory;
- revocation cannot be made prompt and observable;
- the dispatch order would have to change, or the fail-closed path would
  have to become fail-open, to make the lane pass;
- the lane would need a public bind, an external topology, or a public
  network;
- any step would require editing the transport codecs, `router.version`,
  `caps`, the reference pins, or a boundary script to go green;
- the attempt budget is spent without a material change.

## Closure evidence required

- `plans/closure/network-participation/492-status.md` with the full closure
  content the repository requires;
- the lane's evidence file;
- the registry and roadmap rows advanced to closed;
- the unblock audit recorded, with any newly ready plan moved to `ready` in
  the same commit.

## Handoff notes

- **This is a posture plan, not a protocol plan.** If it turns out to
  require new transit protocol code, that is a scope discovery: record it
  as a finding and register a separate plan rather than widening this one.
- **Do not enable participation by default**, and do not change the
  published capability string here.
- **Reference comparison.** i2pd 2.61.0 and Java I2P 2.13.0 both
  participate by default and both enforce a share limit; the reference Rust
  router defaults to participation with a bounded tunnel count and a share
  limit. i2pr's default-off posture is a deliberate, recorded difference
  and must stay deliberate.
