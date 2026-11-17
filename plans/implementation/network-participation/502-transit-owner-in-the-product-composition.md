# Plan 502 — The transit data-plane owner in the product composition

| Field | Value |
| --- | --- |
| Plan | 502 |
| Subsystem | `network-participation` |
| State | `registered` (`registered-transit-owner-in-the-product-composition`) |
| Depends on | Plan 492 (`passed-transit-participation-posture`), Plan 254 (`passed-m11-live-ingress-body-threading-closure-corrective`), Plan 454 (the enforced bandwidth governor) |
| Discharges | **Plan 492 F2** — "a router that has never been given a transit owner does not relay"; and advances **Plan 436 WP3** (source-side slice only) |
| Closure record | `plans/closure/network-participation/502-status.md` |

## 1. Objective

Install the daemon's **existing** transit data-plane owner in the **production
composition**, behind the **existing default-off** `[transit] participation`
opt-in, so that an opted-in router **admits and accounts for** transit traffic
instead of relaying nothing — and so that the M11 boundary rule guards the
**installed owner** rather than a module name plus a no-op probe.

The daemon says this itself, in its own source, at
`crates/i2pr-daemon/src/lib.rs:842-850`:

> Plan 340: the product's transit participation posture. Production
> composition never constructs a transit data-plane owner — the inbound
> path consults `controlled_transit_disabled_probe` instead — so this router
> relays nothing and says so. … Enabling participation is a separate
> decision and is deliberately not reachable from here.

That sentence is the gap. This plan makes it reachable, and makes the
boundary rule insist on it.

## 2. Why ready

- **The owner exists, works, and is exercised.** `TransitLiveOwner`
  (`crates/i2pr-daemon/src/transit_owner.rs:525`) is a complete live
  authenticated SSU2/router-I2NP inbound owner: it admits a short build
  against a participation posture, transforms and forwards it, charges the
  enforced share, writes a receipt, and revokes. Plan 492's own lane
  (`crates/i2pr-daemon/tests/transit_participation_lane.rs:311-320`) drives a
  relay through it and asserts the forward, the share charge, the receipt and
  the revocation.
- **The posture exists and is closed.** Plan 492 delivered the whole
  operator surface: `[transit]` config (`crates/i2pr-daemon/src/config.rs:2192`),
  validation, the participation owner, the refusal taxonomy, the receipts and
  the three Proposal-170 selectors. It closed with the finding that the
  posture is honoured only where the controlled path installs an owner.
- **Plan 492 assigned this work forward, by name.** `F2` in
  `plans/closure/network-participation/492-status.md` ends: *"Owner: the
  plan that installs the transit owner in the product composition."* §15
  records it as an open item with no owner. This plan is that owner.
- **Plan 436's WP3 is exactly this work, and Plan 436's block does not
  apply.** WP3 reads: *"Integrate the proven `TransitLiveOwner` into **the
  same** normal transport / I2NP pump, not an independent socket."* Plan 436
  is registered `blocked on Plan 433` because a *genuine multihop
  originating/receiving client product* must pass Plan 433 first "so transit
  success does not mask ordinary router failure". That is an ordering rule
  about **claiming transit success**, not about **installing the machinery
  behind a default-off switch**.
- **There is a precedent for exactly this decomposition.** The registry's
  Plan 435 row records: *"**source-side work packages now discharged** — WP3
  … by Plan 456, WP4 … by Plan 461, WP2 … and WP6 … by Plan 496; the
  remaining gap is the external two-family public evidence."* Plan 435 stayed
  `blocked` on the external evidence while its source-side packages landed in
  their own plans. Plan 502 does the same for Plan 436's WP3.
- **Every input the owner needs has a production source in the daemon,
  except three, and all three are source-side.** They are enumerated in §3.3.

## 3. Current implementation evidence

### 3.1 The owner is never constructed in production

`TransitLiveOwner::new_disabled` and `TransitOwner::new` are called **only**
from test lanes:

| Site | Class |
| --- | --- |
| `crates/i2pr-daemon/tests/m11_transit_data_plane.rs:54`, `:60` | test lane |
| `crates/i2pr-daemon/tests/transit_participation_lane.rs:251`, `:304`, `:417` | test lane |
| `crates/i2pr-daemon/tests/m11_transit_live_owner.rs:106` | test lane |
| `crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs:604` and 8 more | external, `#[ignore]`-gated |

No `crates/*/src/` file constructs one.

### 3.2 The production inbound path consults a probe, not an owner

`crates/i2pr-daemon/src/lib.rs:4487-4493`, inside `register_ssu2_service`:

```rust
                                    // Controlled transit stays disabled in
                                    // ordinary product profiles: the live
                                    // transit owner is consulted only as a
                                    // disabled probe so the production
                                    // caller references the live-owner
                                    // module without dispatching.
                                    let _ = controlled_transit_disabled_probe(&outcome);
```

`controlled_transit_disabled_probe` (`transit_owner.rs:1500-1502`) is
`TransitOwner::is_transit_managed(outcome)` — a `matches!` with **no
dispatch**. Its result is discarded (`let _ =`).

### 3.3 The three inputs with no production source

Everything else the owner needs already has a production source in the
daemon:

| Need | Production source |
| --- | --- |
| `RouterDeliveryService` | `Ssu2DaemonService::delivery()` (`router_i2np.rs:2114-2117`), wrapping the runtime the SSU2 service already owns |
| outer `CancellationToken` | `ctx.cancellation()` in the service factory — rule 11 forbids a fresh token in `transit_owner.rs` |
| `TransitHopMaterial` | the persistent router identity bundle the SSU2 branch already loads (`lib.rs:3540-3557`) |
| `TransitParticipationBinding` | `TransitParticipationOwner::from_config(&config.transit, Some(&governor), share_percent)` (`transit_participation.rs:436`) |
| `TransitVolumeCounters` | `TransitBuildService::volume_handle()` (`transit_compose.rs:800`) |

The three that do not:

1. **`TransitAdmissionPolicy`** (`crates/i2pr-tunnel/src/transit.rs:424`) has
   no production constructor. Its `available_bandwidth_kbps` and
   `max_per_tunnel_allocation_kbps` arguments have **no `[transit]` config
   field** today, and neither does the registry capacity.
2. **The RNG.** `TransitLiveOwner<R>` requires
   `R: TryCryptoRng + RngCore + CryptoRng + Send`. `rand_core` 0.9's `OsRng`
   implements only `TryRngCore`/`TryCryptoRng`, so it does **not** satisfy
   the bound; `rand_chacha`'s `ChaCha8Rng` does, and must be **seeded from
   OS entropy** (`rand_chacha` 0.9 has no `from_rng`).
3. **The peer-index lifecycle.** `install_peer` / `note_session_closed` have no
   production caller, and the runtime exposes no session-established event —
   only the polled `Ssu2RuntimeService::active_peer_hashes()`
   (`crates/i2pr-runtime/src/ssu2_runtime.rs:1767`).

### 3.4 The boundary rule that must be strengthened, not satisfied

`scripts/check-m11-transit-boundaries.sh:166-175`, rule 10:

```bash
if ! rg -q 'transit_owner' "$root/crates/i2pr-daemon/src/lib.rs"; then
    fail "production lib.rs must reference the transit_owner live-owner module"
fi
if ! rg -q 'controlled_transit_disabled_probe' "$root/crates/i2pr-daemon/src/lib.rs"; then
    fail "production lib.rs must consult the controlled transit probe on the inbound path"
fi
```

Rule 10's own comment says the live-owner module *"must be referenced by the
actual inbound owner outside tests"* — but the reference it accepts is a
**no-op probe**. A composition that installs the owner *and keeps the probe
call beside it* satisfies the literal text while leaving the rule guarding
nothing. **That is the single most likely way this plan produces a green but
dishonest result, and the rule amendment in WP6 exists to make it
impossible.**

Rule 3's comment (`scripts/check-m11-transit-boundaries.sh:107-108`) is
written in anticipation of this work: *"`TransitBuildService` must have a
production caller outside its own test module **once the daemon composition
is enabled**."*

### 3.5 The honesty trap in the existing lane's assertion

`crates/i2pr-daemon/tests/transit_participation_lane.rs:311-320` asserts
`TransitDataDisposition::Forwarded(_)`. The payload is
`TransitDataForwardEvidence { receive_tunnel, next_router, next_tunnel,
outcome }` (`transit_owner.rs:226-242`), and `outcome` is a
`RouterDeliveryOutcome` with **seven** variants including
`NoActiveSession`, `QueueFull`, `Cancelled` and `DeadlineElapsed`. The
share is charged **before** the forward (`transit_owner.rs:1047-1071`), so
`bytes_relayed` advances even when the delivery then fails. **A lane that
asserts only `Forwarded(_)` — or only `bytes_relayed` — passes on a relay
that never reached a peer.** This plan's evidence asserts the **outcome**,
never the wildcard.

## 4. Invariants that must not regress

1. **Default-off is preserved exactly.** The default configuration installs
   **no** transit owner, publishes `TransitParticipation::Disabled`, and
   relays nothing. The three Proposal-170 transit selectors keep reporting
   `participating = false` and zeros **because the Disabled posture owns no
   counters** — not because a zero was substituted for a missing
   measurement. This property must not change.
2. **The boundary rule gets stronger, never weaker.** The amended rule must
   require **more** than the present rule requires, and the amendment must
   be accompanied by a **negative control** that fails on the vestigial
   shape (§3.4). A rule that a text search can satisfy without an installed
   owner is not an amendment.
3. **One pump, one decoder, one socket.** The owner is installed into the
   **existing** SSU2 service — the same transport, the same inbound pump,
   the same `RouterDeliveryService`. No second socket, no second event loop,
   no second I2NP decode (`check-m11-transit-boundaries.sh` rule 12 bans one
   in `transit_owner.rs`).
4. **Fail closed.** An unknown, uncomparable, below-floor or refused peer is
   **refused**, never skipped silently, and every refusal is counted by
   reason. The participation posture's refusal taxonomy is unchanged.
5. **No advertisement, no promotion, no version change.** The published
   capability string is unchanged; the transit role stays **non-advertised**.
   `router.version` stays `0.9.62`; `caps` and the published RouterInfo are
   unchanged; `specs/support.toml` stays at 94 surfaces (88 experimental, 5
   deferred, 1 ready) with 0 advertised; `specs/support.toml`, `Cargo.toml`
   and `Cargo.lock` stay byte-identical.
6. **Bounded.** No unbounded channel, queue or registry. The peer index stays
   under `MAX_TRANSIT_PEER_INDEX`; the transit registry stays under its
   derived capacity; the reconcile is a bounded pass over the live session
   set.
7. **Explicit ownership and cancellation.** The owner uses the **outer**
   owner's real `CancellationToken`; it is cancelled and drained
   deterministically on shutdown, not merely dropped.
8. **The evidence asserts the delivery outcome.** Never `Forwarded(_)` alone,
   never `bytes_relayed` alone.
9. **Secrets stay secret.** The hop material's static private key is
   redacted in diagnostics and zeroized on drop. No `Debug`/`Display` on a
   secret type is added.
10. **No dependency change.** No new crate, no new feature, no version bump.

## 5. Scope

### 5.1 In scope

- **WP1 — a composition module** that derives everything the owner needs
  from the normalized configuration and constructs the owner.
- **WP2 — the production install and the inbound dispatch.**
- **WP3 — the peer-index lifecycle and the expiry sweep.**
- **WP4 — the creator-correlation wiring**, so the router's own coordinator
  traffic is never claimed as transit.
- **WP5 — the participation publication**, so the three Proposal-170
  selectors report what the router actually relayed.
- **WP6 — the boundary-rule amendment and its controls.**
- **WP7 — the evidence.**
- **WP8 — the documentation.**

### 5.2 Explicitly out of scope

- **The LOCAL OBEP consumer.** The daemon has no production consumer for a
  LOCAL delivery action, so a LOCAL action refuses with the typed
  `TransitLiveError::NoLocalConsumer`. That is **fail-closed and honest**;
  this plan does not invent a consumer.
- **NTCP2 ingress.** `TransitLiveOwner::handle_inbound` takes an
  `Ssu2InboundI2np`. NTCP2-originated traffic is not routed into the owner
  by this plan.
- **Stock-peer and public-network qualification.** Plan 492's F3 and Plan
  436's WP5 stay open and blocked on the authorized two-host topology
  (Plan 433 / Plan 493). This plan claims **no** public transit readiness.
- **Any advertisement.** The transit role stays non-advertised.
- **New configuration fields.** The admission policy and the registry
  capacity are **derived** from the existing `[transit]` and `[bandwidth]`
  fields (§6, WP1). No config key is added, so there is no migration.
- **Plan 436's blocked scope.** Plan 436 stays `blocked`; this plan advances
  its source-side WP3 only (§15).
- **Plan 492 F1** (a private mesh cannot complete the build's *delivery*)
  and **Plan 492 F3** (no stock-peer evidence) stay open. See §13.3.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/transit_composition.rs` | **New.** The composition-owned derivation and construction: admission policy, hop material, participation binding, RNG seeding, and the owner's construction. Owns no socket and no task. |
| `crates/i2pr-daemon/src/lib.rs` | The SSU2 service installs the owner when the operator opted in; the production inbound path dispatches through it; the participation posture is published as `Enabled` with the service's own volume handle. |
| `crates/i2pr-daemon/src/transit_owner.rs` | Only the narrowest additions the composition needs (for example, reading the owner's participation snapshot for publication). No behavioural change to the admission order. |
| `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` | A **public, read-only** accessor for the published transit posture, so an out-of-crate lane can observe it. `publish_transit_participation` is already public; the reader is not, and the asymmetry is closed. |
| `scripts/check-m11-transit-boundaries.sh` | Rule 10 **strengthened** to require the installed owner on the production inbound path, with the disabled probe as the fallback only; plus a positive and a negative control. |

No file under `crates/i2pr-tunnel`, `crates/i2pr-runtime`,
`crates/i2pr-transport*`, `crates/i2pr-netdb`, `crates/i2pr-proto` or
`crates/i2pr-core` changes. `Cargo.toml` and `Cargo.lock` do not change.

## 7. Work packages

### WP1 — the composition module

A new `crates/i2pr-daemon/src/transit_composition.rs` that owns the
derivation. It is a pure function of the normalized configuration plus the
already-loaded identity bundle and the delivery seam — no socket, no task,
no timer, no I/O.

It derives:

- **The admission policy** (`i2pr_tunnel::TransitAdmissionPolicy::new`,
  `crates/i2pr-tunnel/src/transit.rs:424`) from `config.transit` and
  `config.bandwidth`, inside the documented ceilings
  (`MAX_TRANSIT_ACTIVE` 64, `MAX_TRANSIT_PENDING` 8,
  `MAX_TRANSIT_PEER_ACTIVE` 8, `MAX_TRANSIT_PEER_PENDING` 2). The natural
  mapping is `max_active = participating_tunnels`,
  `max_active_per_peer = peer_participation_cap`, with the pending counts
  bounded by their ceilings, and `registry_capacity = max_active`.
  `available_bandwidth_kbps` is derived from the **enforced** bandwidth
  share the governor permits the transit posture — the same share
  `share_limit` bounds — and `max_per_tunnel_allocation_kbps` is `None`
  (no per-tunnel allocation cap beyond the per-peer and share limits).
  **The exact validation in `TransitAdmissionPolicy::new` must be read
  before the values are chosen; a policy that the constructor rejects is
  wrong, and the derivation must not be widened to make it fit.**
- **The hop material** (`TransitHopMaterial::new`) from the router identity
  bundle the SSU2 branch already loads — the bundle's static private key
  and this router's own identity hash. The bundle is **moved** into the
  SSU2 service today, so the hop material must be taken from it **before**
  the move, exactly as `local_router_hash` already is (`lib.rs:3645`).
- **The participation binding** from
  `TransitParticipationOwner::from_config(&config.transit, Some(&governor),
  config.bandwidth.share_percent)` and the daemon's `BandwidthGovernor`.
- **The RNG**: a `ChaCha8Rng` seeded from **OS entropy**. `rand_core` 0.9's
  `OsRng` is `TryRngCore`-only and cannot satisfy the owner's bound; the
  seed must be drawn through `try_fill_bytes` and the RNG built with
  `ChaCha8Rng::from_seed`. **A fixed or derived seed is not acceptable.**

The module returns `Option<TransitLiveOwner<ChaCha8Rng>>`: `None` when
`config.transit.participation` is false, `Some(enabled)` when it is true.
**`None` is the default path and must construct nothing at all** — no
service, no counters, no owner.

### WP2 — the production install and the inbound dispatch

Inside `register_ssu2_service` (`crates/i2pr-daemon/src/lib.rs:3337`), after
the `Ssu2DaemonService` is constructed and before the inbound loop starts:

- When the operator opted in, construct the owner via WP1 and keep it in
  scope for the service's lifetime.
- In the inbound `!served` branch (`lib.rs:4487-4493`), dispatch through the
  **installed owner** when one exists, and keep
  `controlled_transit_disabled_probe` as the **fallback for the
  no-owner case only**. The two must be **mutually exclusive branches**, so
  that the probe can never run beside a live owner.

The dispatch must pass the **raw** `Ssu2InboundI2np` (already in scope as
`inbound` after `lib.rs:4190`), `now_ms` (already in scope) and
`now_seconds = now_ms / 1000`. `handle_inbound` performs the single
canonical decode (`transit_owner.rs:702`), so the production path must **not**
decode a second time.

### WP3 — the peer-index lifecycle and the expiry sweep

The peer index is the owner's record of which routers it can reach. Without
it, `deliver_tunnel_data_forward` refuses with `NoActiveSession`
(`transit_compose.rs:1028-1031`) — the owner would decide to forward and then
report that it reached nobody.

- The index is reconciled against the **live session set**
  (`Ssu2RuntimeService::active_peer_hashes()`, bounded by
  `MAX_SSU2_ACTIVE_CEILING`). For each live peer hash the router is
  installed as `install_peer(hash, PeerId::from_hash(hash))` — `PeerId` is a
  newtype over the router identity hash
  (`crates/i2pr-transport/src/identity.rs:13`), so the mapping is exact.
- Peers that are **no longer live** are removed with `note_session_closed`.
- Expired registrations are swept with `expire(now_seconds)`
  (`transit_owner.rs:1453`), whose own doc says *"the production daemon owns
  when sweeps run"*.

The reconcile and the sweep run on the **existing** pump tick, not on a new
task. The reconcile is a bounded pass over a bounded set; it must not be a
per-message operation, and it must not spin.

### WP4 — the creator-correlation wiring

The owner's documented order is **creator/service first, transit second,
unknown fail-closed** (`transit_owner.rs:946-950`). Without the creator
correlation, the owner may claim the router's **own** coordinator traffic as
transit work — consuming the participation budget on the router's own
builds.

The daemon already has the producer: the build coordinator publishes
`reserved_message_ids()` (`crates/i2pr-daemon/src/exploratory_build.rs:1021`)
and `reserved_tunnel_ids()` (`:1033`), and the owner's narrow setters are
`install_creator_build(peer, message_id)` (`transit_owner.rs:616`),
`install_creator_data(tunnel_id)` (`:622`) and
`install_creator_gateway(tunnel_id)` (`:628`). The owner module's own doc
says the production daemon *"installs its real coordinator/registry facts
through the same narrow setters"* (`transit_owner.rs:521-524`).

The wiring must feed the coordinator's facts into those setters. **If a
producer for a given fact does not exist, that is a finding to record, not a
reason to leave the path unguarded** — the owner must not be installed with
a known-unprotected path.

### WP5 — the participation publication

`crates/i2pr-daemon/src/lib.rs:850` publishes
`TransitParticipation::Disabled` unconditionally. Under the opt-in, the
composition must publish `TransitParticipation::Enabled(counters)` using the
**service's own** `volume_handle()` (`transit_compose.rs:800`) — the handle
that only a router which actually constructed the service can hold, which is
what keeps the product posture and the controlled lane from being confused
for one another.

The publication happens **when the owner is installed**, never before: a
router that has not installed an owner must not report that it is
participating.

### WP6 — the boundary-rule amendment

`scripts/check-m11-transit-boundaries.sh` rule 10 must be **strengthened**,
and the amendment must be made **testable**. The rule becomes a function
over the files so it can be exercised by a control:

- **Positive:** the production `lib.rs` dispatches the inbound path through
  the composition module's entry point, which makes the whole
  owner/no-owner choice internally.
- **Positive:** the production `lib.rs` reaches **neither** owner entry
  point directly — neither the live owner's `handle_inbound` nor the
  disabled probe. The owner is reached **only** through the composition.
- **Positive:** the composition module **keeps** the disabled probe as the
  no-owner fallback, **keeps** reaching the live owner's `handle_inbound`
  on the installed-owner branch, and **owns** the construction entry point.
- **Negative:** a synthetic `lib.rs` that calls the **probe only** — the
  vestigial shape of §3.4 — must be **reported**. A rule that a text
  search can satisfy without an installed owner is not an amendment.
- **Negative:** the same vestigial shape with a live owner **beside** the
  discarded probe — the exact shape the present rule accepts — must be
  **reported**. This is the control that proves the amendment is stronger
  than the rule it replaces.

**The disabled probe moves out of `lib.rs` and into the composition
module.** It is not removed: the no-owner fallback must exist, and the
composition module is where that choice belongs. Leaving a probe call in
`lib.rs` is precisely the vestigial shape the amendment must reject, so the
amendment is what *permits* the probe to move — the present rule's second
assertion ("`lib.rs` must consult the controlled transit probe") cannot be
satisfied by a composition that reaches the owner at all, so the rule has to
change for the honest shape to pass.

Nothing else in the script is relaxed. The rule count and the control count
must be **derived**, not hardcoded (the Plan 497 L1 lesson), and every rule
must have a control that fires it.

**A second, independent defect is in scope for this amendment.** The
script's existing production scan strips each daemon source file from its
first `#[cfg(test)]` to EOF, and `crates/i2pr-daemon/src/lib.rs` declares a
test-only module at line 14 — so the tree every pre-existing rule scans is
missing almost all of the daemon's production code, including the production
inbound path. Those rules are **fail-open** on `lib.rs`: a forbidden call
below line 14 is not seen. The amendment replaces that convention with a
**scoped** per-item strip (the Plan 366 house fix) for the whole
production tree, and every pre-existing rule must still pass. **If the
scoped strip reveals a genuine production violation, that is a defect to
report and fix — never a reason to weaken a rule.**

### 7.1 The fixed seam names

The implementation and the boundary rule are written against these names,
so neither can drift into a shape the other does not expect:

| Name | Owner |
| --- | --- |
| `crates/i2pr-daemon/src/transit_composition.rs` | the new composition module |
| `transit_composition::build_transit_owner` | the construction entry point |
| `transit_composition::dispatch_transit_inbound` | the inbound dispatch entry point; makes the whole owner/no-owner choice internally |
| `crate::transit_owner::controlled_transit_disabled_probe` | the no-owner fallback, owned by the composition module |

### WP7 — the evidence

Two layers, both required:

1. **In-crate unit tests** for the composition module: the default
   configuration constructs **no** owner; the opt-in configuration constructs
   an **enabled** owner; the derived policy's values are inside the
   documented ceilings; the hop material carries **this router's** identity
   hash; and a synthetic inbound through the constructed owner produces a
   participation-aware outcome whose **delivery outcome** is asserted
   explicitly — never `Forwarded(_)` alone, and never `bytes_relayed` alone.
2. **A public-API daemon test** that observes, through the published
   posture, that the composition installs the owner under the opt-in and does
   **not** under the default. It must additionally show that the three
   Proposal-170 transit selectors report `participating = false` and zeros
   under the default configuration.

### WP8 — the documentation

`docs/architecture/overview.md`, `docs/architecture/tooling.md`, and the
`network-participation` roadmap row. **`AGENTS.md` gets no plan ledger.**
If `AGENTS.md`'s hard-boundaries section states something about the transit
role that this plan falsifies, the sentence is corrected — a statement about
the product is not plan state.

## 8. Failure, cancellation, restart, and contention semantics

- **The owner cannot be constructed** (an invalid policy, a missing identity,
  an unavailable governor). The construction **fails closed**: the service
  reports the failure and the router continues **without** a transit owner.
  It must never fall back to a probe that implies participation.
- **The participation posture refuses.** Every refusal is counted by reason
  through the existing taxonomy; a refusal is never a silent skip.
- **The delivery fails.** `RouterDeliveryOutcome` says so
  (`NoActiveSession`, `QueueFull`, `Cancelled`, `DeadlineElapsed`, …), and
  the evidence asserts that outcome.
- **Cancellation.** The owner holds the **outer** owner's token. On
  shutdown the owner is cancelled and drained deterministically — the
  registry's role-bound `LayerKeys` are zeroized and the peer index is
  cleared (`transit_compose.rs:1111-1126`). Dropping the owner is a
  fail-safe, not the plan.
- **Restart.** No stale forwarding state survives as active. A restarted
  router rebuilds its owner from configuration; the registry is not
  persisted.
- **Contention.** The reconcile and the sweep run on the existing pump tick
  and are bounded. They must not starve the inbound path, and the inbound
  path must not take a lock the sweep holds for long.

## 9. Compatibility and migration

No migration. No stored format changes. **No configuration key is added or
changed** — the admission policy and the registry capacity are derived from
the existing `[transit]` and `[bandwidth]` fields.

The behaviour change is confined to the opt-in: a router configured with
`[transit] participation = true` now installs a transit owner and relays.
A router with the default configuration is **byte-for-byte the same
product** as before.

## 10. Required tests

- The composition module's unit tests (WP7.1).
- The public-API daemon test (WP7.2).
- `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1`.
- `cargo test --locked -p i2pr-tunnel --all-targets`.
- `bash scripts/check-m11-transit-boundaries.sh` — green on the finished
  tree, and the negative control fires.
- The routine floor in `AGENTS.md`, in full.

## 11. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked -p i2pr-tunnel --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-m11-transit-boundaries.sh
bash scripts/check-m11-transit-qualification-evidence.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
```

and the full routine floor from `AGENTS.md`.

## 12. Documentation updates

- `docs/architecture/overview.md` — the transit owner's place in the
  composition, and the default-off posture.
- `docs/architecture/tooling.md` — the strengthened M11 rule and its
  controls.
- `plans/subsystems/network-participation-roadmap.md` — the §7 milestone row
  and the §12 status summary.
- `plans/registry.md` — the plan row, and the disposition of Plan 492's F2.
- `AGENTS.md` — **only** a correction of a product statement that this plan
  falsifies. No plan numbers, no status tokens, no plan ledger.
- `specs/CONFORMANCE.md` — **only** if it states something about the transit
  role that this plan falsifies. Nothing is promoted.
- `specs/support.toml` — **unchanged**: 94 surfaces, 0 advertised.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

1. The default configuration installs **no** transit owner, publishes
   `Disabled`, and the three Proposal-170 transit selectors report
   `participating = false` and zeros.
2. The opt-in configuration installs an **enabled** owner in the production
   SSU2 service, and the participation posture is published as `Enabled`
   with the service's own volume handle.
3. The production inbound path dispatches through the **installed owner**
   when one exists, and the disabled probe is the **no-owner fallback
   only** — the two are mutually exclusive branches.
4. The peer index is reconciled against the live session set, and expired
   registrations are swept, on the existing pump tick.
5. The router's own coordinator traffic is not claimed as transit.
6. The owner is cancelled and drained deterministically on shutdown.
7. The amended rule 10 is **stronger** than the rule it replaces, and its
   **negative control fires** on the vestigial shape.
8. The evidence asserts a **delivery outcome**, never `Forwarded(_)` alone
   and never `bytes_relayed` alone.
9. `specs/support.toml`, `Cargo.toml` and `Cargo.lock` are byte-identical;
   `router.version` is `0.9.62`; `caps` and the published RouterInfo are
   unchanged.
10. The routine floor passes in full.

### 13.2 Stop conditions

Stop and record a finding, rather than working around it, if:

- the rule amendment could only be satisfied by **keeping a vestigial probe
  call beside the live owner** — then the rule must be strengthened further,
  not the code shaped to it;
- the negative control cannot be made to fire, so the rule cannot be
  distinguished from a text search;
- installing the owner would require a **second socket, a second event
  loop, or a second I2NP decode**;
- the creator-correlation cannot be wired, so the owner would claim the
  router's own coordinator traffic — then the owner is **not** installed, and
  the gap is recorded;
- the admission policy would need **new configuration fields** — then the
  derivation is recorded as a finding rather than the config surface grown
  silently;
- closing the gap would require an **advertisement, a capability promotion,
  a `caps` change, or a version change**;
- the peer-index reconcile could only be made to work by a **per-message
  unbounded operation**.

### 13.3 What this plan does **not** close

- **Plan 492 F1** — a private mesh cannot complete the build's *delivery*;
  this plan does not change that.
- **Plan 492 F3** — no stock-peer evidence for the posture; blocked on the
  authorized two-host topology (Plan 493).
- **Plan 436** — stays `blocked`; WP5 (the stock-peer qualification) and the
  Plan 433 ordering dependency are untouched.
- **Plan 493** — stays `blocked`.
- **Plan 437, 438, 439** — stay `blocked`.

## 14. Closure evidence required

The closure record must carry: the commits; a requirement-to-evidence matrix
naming each acceptance criterion and the test that discharges it; the exact
commands and their outcomes, labelled local or CI truthfully; an invariant
review against §4, row by row; a failure, cancellation, restart, migration
and security review; the documentation changes; the limitations — including
that this plan claims **no** public transit readiness and that the build's
*delivery* is still unproven; findings by severity; the disposition of Plan
492's F2; the roadmap disposition; and the unblock audit.

## 15. Handoff notes

- **Plan 492's closure record is not rewritten.** The disposition of its F2
  finding is recorded here and in the registry rows.
- **Plan 436 stays `blocked`.** This plan advances its **source-side WP3
  only**. Plan 436's own dependency on Plan 433 — that a genuine multihop
  client product must pass first, so transit success does not mask ordinary
  router failure — is **not** discharged here, and this plan claims no
  transit success against an ordinary router. The registry's Plan 435 row is
  the precedent for splitting a blocked plan this way.
- **The transit role stays non-advertised.** This plan makes the machinery
  reachable behind a default-off switch. It does **not** make the router a
  public transit participant, and nothing here may be described as public
  transit readiness.
- **No plan state is mirrored outside `plans/`.** `AGENTS.md` is a statement
  about checker coverage and product boundaries, not a ledger.
