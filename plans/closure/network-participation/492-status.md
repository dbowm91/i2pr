# Plan 492 — Transit participation: an operator-opted, bounded, revocable relay posture

**Status:** `passed-transit-participation-posture`
**Date closed:** 2026-10-24
**Subsystem:** network participation (phase 5 of 7)
**Branch:** `work/plans-477-480-router-recovery`
**Implementation plan:** [`plans/implementation/network-participation/492-transit-participation-posture.md`](../../implementation/network-participation/492-transit-participation-posture.md)
**Roadmap:** [`plans/subsystems/network-participation-roadmap.md`](../../subsystems/network-participation-roadmap.md)
**Implementation commits:** `d287635e` — *feat(network-participation): plan 492 transit participation posture*; `6ff44072` — *docs(network-participation): plan 492 architecture, conformance, and inventory*; `d7f4323c` — *fix(network-participation): bound the plan 492 receipt control's posture* (the workspace sweep's catch); `279f6ae4` and `f0e44798` — the two defects the routine floor caught (the floor-caught fixes are committed separately, as Plans 489, 490, and 491 did, so this record can report the tree honestly; neither is part of the plan's deliverable — see §7.3)
**Predecessor:** Plan 268 (`passed-m11-receipt-family-three-completions-zero-semantic-failures-path-divergence-falsified-exact-head-ci-green` — the passed one-family qualification) and the transit ingress owner Plan 254 closed (`passed-m11-live-ingress-body-threading-closure-corrective`); Plan 454's bandwidth governor (`plans/closure/core-router-recovery/454-status.md`), whose `BandwidthEnforcementPermit` the posture consumes as its enforced share; and Plan 488's selection policy, whose /16 diversity rule and per-peer participation cap the router's own build hops now satisfy.
**Owner boundary:** `crates/i2pr-daemon` (the participation owner and its policy, the `[transit]` configuration section, the transit ingress owner's dispatch, the build-hop selection, and the receipts), `crates/i2pr-netdb` (Plan 488's selection policy, consumed rather than restated), `crates/i2pr-runtime` (the governor whose permit is the enforced share). `crates/i2pr-tunnel` is **unchanged** by this plan, and `crates/i2pr-client` is unchanged (§1's deviations).
**Authority:** closure records outrank executable tests, which outrank ADRs, which outrank prose (`plans/README.md`).

---

## 1. Scope

Plan 492 turned the **existing** transit machinery on for an
**operator-opted** profile. The transit state machine
(`i2pr-tunnel`'s `transit.rs`) and the transit ingress owner
(`crates/i2pr-daemon/src/transit_owner.rs`) were already closed surfaces;
what was missing was a *policy* that answers four questions:

1. **Who may turn participation on?** Only the operator, through a new
   `[transit]` configuration section. The posture is **off by default** and
   the default configuration does not change.
2. **How much?** A bounded share of the router's *enforced* bandwidth
   share, a bounded number of participating tunnels, and a bounded per-peer
   participation cap.
3. **For how long?** Until the operator changes the posture, the governor's
   permit is withdrawn, or health is lost. Each stop is prompt and recorded.
4. **What happened?** The receipts carry what was relayed, how much, the
   refusals by reason, and when participation stopped.

### In scope

- The `[transit]` configuration section and its bounded, validated
  normalization.
- The daemon-side participation owner and its policy.
- The dispatch from the transit ingress owner into the participation owner,
  preserving the documented order.
- The selection diversity for the router's own build hops: Plan 488's /16
  rule and its per-peer participation cap.
- The offline controls, the inverted probes, and the private-mesh lane.
- The architecture, conformance, and inventory updates.

### Explicitly out of scope

- **Public-network operation and any advertisement.** `router.version`,
  `caps`, and the published RouterInfo are unchanged; the transit role stays
  disabled, non-advertised, and unclaimed for the public network. The
  two-family public participation claim is Plan 493's work.
- **New transit protocol code.** No I2NP message, no RouterInfo option, no
  wire change, and no new tunnel-crate code. The plan document made this a
  stop condition, and it was honoured: `i2pr-tunnel` is unchanged.
- **A second share accounting.** The governor's permit is the enforced
  share; the posture consumes it rather than adding a parallel meter.
- **A migration.** The posture is a new, optional section. A configuration
  without it parses unchanged and does not participate.
- **The reference pins.** i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`), i2psam
  (`b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac`), and i2plib
  (`6edf51cd5d21cc745aa7e23cb98c582144884fa8`) are unchanged and were not
  consulted for this plan: the posture is a local policy, and no reference
  implementation was patched or vendored.
- **The boundary scripts.** None was edited, relaxed, or reworded. The
  plan's stop condition named exactly that, and the floor proves it.

### Deviations from the plan document (recorded, not hidden)

| The plan document said | What landed | Why |
| --- | --- | --- |
| `crates/i2pr-client`: the selection diversity rule for the router's own tunnel hops | `crates/i2pr-daemon/src/destination_peers.rs` | `i2pr-client` owns destination tunnel **lifecycles** and consumes peers that were already chosen; it has no build-peer selection surface. Plan 491 established the same fact for the version floors. The rule landed in the daemon, consuming Plan 488's policy from `i2pr-netdb`, and `i2pr-client` is unchanged. |
| The participation owner keeps the participating-tunnel count | The owner **reads** the transit service's registration counts at each decision | A second count of live tunnels is a second ledger, and a ledger diverges from the data plane. The service's registry is the authority; the posture's limits are measured against it. |
| `TransitParticipation::Disabled` / `Enabled` remain the published posture | Unchanged; the new posture is a separate, internal surface | The Proposal 170 selectors still answer `0` / `0` / `0.0` for the default posture. Plan 492 does not change the published transit counters. |

---

## 2. Baseline at registration

| Item | Value |
| --- | --- |
| Branch | `work/plans-477-480-router-recovery` |
| Head at registration | `c43f3404` (Plan 487's and Plan 478's closure) — the plan document's own declared baseline |
| Head at implementation start | `02ccb197` (Plan 491's closure) — the first commit after this plan was registered, and the base of every commit below |
| Transit state machine | `crates/i2pr-tunnel/src/transit.rs` — closed surface, unchanged by this plan |
| Transit ingress owner | `crates/i2pr-daemon/src/transit_owner.rs` — Plan 254's live owner, installed only by the controlled opt-in path |
| Bandwidth governor | `crates/i2pr-runtime/src/bandwidth.rs` — Plan 454's governor, with `BandwidthEnforcementPermit` and `capacity_permit` |
| Selection policy | `crates/i2pr-netdb/src/selection.rs` — Plan 488's `SelectionConstraints`, `ParticipationLedger`, and `network_prefix16` |
| Ordinary build-hop selection | `crates/i2pr-daemon/src/destination_peers.rs` — the Java-profile selector with **no** /16 rule and **no** participation cap |
| Published transit posture | `TransitParticipation::Disabled`; the three Proposal 170 selectors answer `0` / `0` / `0.0` |
| Advertised surfaces | 0 |

---

## 3. Implementation commits

| Commit | What it carries |
| --- | --- |
| `d287635e` | `feat(network-participation): plan 492 transit participation posture` — the `[transit]` configuration section and its normalization; `crates/i2pr-daemon/src/transit_participation.rs` (the owner, the policy, the binding, the receipts); the transit ingress owner's dispatch into the posture; `TransitBuildService::active_for_peer`; the ordinary build-hop selection's `/16` rule and participation cap, and the coordinator's participation ledger; the M11 controlled lanes' posture installation; the three new test binaries; the sanitized lane artifact. |
| `6ff44072` | `docs(network-participation): plan 492 architecture, conformance, and inventory` — the daemon, runtime, and tunnel deep-dives; `specs/CONFORMANCE.md`; `specs/support.toml`; the plan document's status line. |
| `d7f4323c` | `fix(network-participation): bound the plan 492 receipt control's posture` — the sweep-caught fix (§7.3). |
| `279f6ae4` | `fix(deps): bump crypto-bigint to 0.7.6 for RUSTSEC-2026-0387` — **not part of the deliverable.** The floor's `cargo-deny` step caught a *new* RustCrypto advisory that Plan 492 did not cause; the two-line lockfile bump that resolves it (§7.3). |
| `f0e44798` | `fix(network-participation): rustfmt the plan 492 changes` — **not part of the deliverable.** The floor's `cargo-fmt` step caught 59 rustfmt hunks in the plan's own new code; every hunk is a line-wrapping change and no statement, binding, or assertion changed (§7.3). |

---

## 4. Requirement-to-evidence matrix

| # | Requirement | Evidence | Verdict |
| --- | --- | --- | --- |
| R1 | **The default configuration does not participate.** | `config.rs` — `transit_participation_is_disabled_by_default_with_bounded_defaults`; `transit_participation_posture.rs` — `the_default_configuration_does_not_participate`; the static guard's rule 1 (`the_posture_switch_defaults_off`). | Held. |
| R2 | **The participation posture is a single, bounded, validated surface.** | `config.rs` — `normalize_transit`; `transit_participation_is_disabled_by_default_with_bounded_defaults`, `transit_participation_accepts_an_explicit_bounded_opt_in`, `transit_participation_rejects_unbounded_or_contradictory_limits`, `transit_section_rejects_unknown_keys`; the posture controls — `the_policy_bounds_are_named_and_bounded`. | Held. |
| R3 | **An opt-in requires an enforced share.** | `config.rs` — `transit_participation_requires_an_enabled_bandwidth_policy`; the posture controls — `an_opt_in_requires_an_enforced_share` (`NoEnforcedPermit`). | Held. |
| R4 | **The share limit is enforced, not advisory.** | The owner's `revalidate` (point-of-use permit revalidation) and `charge_relay` (the charge precedes the forward); the posture controls — `the_share_limit_is_enforced_rather_than_growing`; the static guard's rule 3 (`the_share_is_enforced_at_the_point_of_use`); `transit_participation.rs` — `exceeding_the_share_refuses_rather_than_grows`. | Held. |
| R5 | **The participating-tunnel count is enforced and counted.** | The posture controls — `the_participating_tunnel_count_is_enforced_and_counted`; `transit_participation.rs` — `the_participating_tunnel_count_is_enforced_and_counted`. | Held. |
| R6 | **The per-peer participation cap is enforced and counted.** | The posture controls — `the_per_peer_participation_cap_is_enforced_and_counted`; `transit_participation.rs` — `the_per_peer_cap_is_enforced_and_counted`. | Held. |
| R7 | **Revocation by policy change, permit withdrawal, and health loss stops participation within a bound, and the stop is recorded.** | The posture controls — `a_policy_change_to_off_stops_participation_and_is_recorded`, `a_withdrawn_permit_stops_participation_and_is_recorded`, `a_health_loss_stops_participation_and_is_recorded`; `transit_participation.rs` — the same three; the static guard's rule 4 (`a_revocation_is_recorded`). | Held. |
| R8 | **A restart starts from the configured posture and re-derives the accounting from zero.** | The posture controls — `a_restart_re_derives_the_accounting_from_zero`; `transit_participation.rs` — `a_restart_re_derives_the_accounting_from_zero`. | Held. |
| R9 | **The receipts are bounded and carry counts only.** | The posture controls — `the_receipts_are_bounded_and_carry_counts_only`; `transit_participation.rs` — `the_receipts_are_bounded_and_carry_counts_only`; the static guard's rules 5 and 6 (`every_participation_surface_is_bounded`, `the_participation_receipt_carries_no_peer_identity`), each with its inverted probe. | Held. |
| R10 | **The dispatch order is unchanged, and the posture is consulted before any slot is allocated.** | The static guard's rule 2 (`the_dispatch_order_is_unchanged`) with its inverted probe; `transit_owner.rs` — `dispatch_build_body` and `refuse_unless_participating`. | Held. |
| R11 | **The router's own build hops satisfy the /16 rule deterministically.** | `destination_peers.rs` — `the_prefix16_rule_excludes_two_peers_from_one_network_prefix`, `the_prefix16_policy_is_honoured_explicitly`, `the_selection_is_deterministic_for_a_fixed_seed`; the static guard's rule 7. | Held. |
| R12 | **The per-peer participation cap is applied to the ordinary selection and a selection is recorded.** | `destination_peers.rs` — `the_participation_cap_removes_a_capped_peer_from_the_projection`; `destination_tunnels.rs` — `record_hop_selection`; `service_product.rs` — the three live call sites; the static guard's rule 8. | Held. |
| R13 | **A relayed peer's transit traffic is visible in a source-originated receipt, and the revocation's stop is observed.** | `transit_participation_lane.rs` — `plan492_relay_then_revoke_is_observed_in_the_receipts`; the sanitized artifact `tests/integration/network-participation/evidence/plan492/result.json`. | Held, with the mesh limitation recorded in §11 and §12 (F1). |
| R14 | **No wire change, no new option, no advertisement.** | `specs/support.toml` — one `experimental` surface with `advertised = false`; the inventory holds **94** surfaces and **0** advertised entries; `router.version`, `caps`, and the published RouterInfo are unchanged; `i2pr-tunnel` is unchanged. | Held. |
| R15 | **The transit role stays disabled, non-advertised, and unclaimed.** | The static guard's rule 9 (`the_transit_role_is_not_advertised`) with its inverted probe; `specs/support.toml`'s `m11_transit_tunnels` prose. | Held. |
| R16 | **No boundary script was weakened.** | The floor's `check-m11-transit-boundaries`, `check-runtime-boundaries`, `check-console-boundaries`, `check-dependency-direction`, `check-service-tunnel-boundaries`, and `check-service-anonymity-boundaries` steps are green (§7.4); `git diff` over `scripts/` is empty for this plan. | Held. |

---

## 5. Work packages and outcomes

### WP1 — the configuration surface

`crates/i2pr-daemon/src/config.rs` gained `RawTransitConfig`
(`deny_unknown_fields`, every field defaulted) and the normalized
`TransitConfig`, plus the named constants `DEFAULT_TRANSIT_SHARE_LIMIT`
(25), `DEFAULT_TRANSIT_PARTICIPATING_TUNNELS` (4),
`DEFAULT_TRANSIT_PEER_PARTICIPATION_CAP` (2),
`MAX_TRANSIT_SHARE_LIMIT_PERCENT` (100),
`MAX_TRANSIT_PARTICIPATING_TUNNELS` (32), and
`MAX_TRANSIT_PEER_PARTICIPATION_CAP` (8).

`normalize_transit` rejects, before any owner or listener exists:

- a share limit of zero or above 100 percent;
- a participating-tunnel count of zero or above 32;
- a per-peer cap of zero, above 8, or **larger than the router-wide
  count** — a per-peer cap above the router-wide cap is a contradiction
  rather than a looser policy;
- an opt-in **without an enabled bandwidth policy**.

The last one is the interesting rejection. The share limit is enforced
against the governor's permitted share; without a governor the limit could
not be enforced and would be advisory, and the plan forbids an advisory
limit. That is a configuration error, not a silent no-op.

### WP2 — the participation owner

`crates/i2pr-daemon/src/transit_participation.rs` is new (about 1 100 lines
with its tests). It owns:

- `TransitParticipationPolicy` — the bounded limits, validated by one
  constructor that the configuration path *and* a hand-built path both go
  through, so a policy cannot smuggle an unbounded limit past the named
  constants.
- `TransitParticipationOwner` — the accounting surface. It owns **no
  socket, no timer, and no task**; the runtime owner supplies the clock.
- `TransitParticipationBinding` — the owner plus the live governor, so the
  permit stays healthy while the owner is installed and the owner has the
  governor to revalidate against.
- `TransitRefusalReason`, `TransitRevocationReason`,
  `TransitRefusalCounts`, `TransitReceiptKind`,
  `TransitParticipationReceipt`, `TransitParticipationSnapshot`.

The admission decision takes the transit service's **authoritative**
registration counts:

```rust
pub fn admit(
    &mut self,
    peer: &Hash,
    active_tunnels: usize,
    active_for_peer: usize,
    now_seconds: u64,
) -> Result<(), TransitRefusalReason>
```

That is the load-bearing design decision of the plan. A second count of
live tunnels would be a second ledger, and a ledger diverges from the data
plane: an expired registration, a closed session, and a rolled-back build
would all have to be reflected by hand. Instead the transit service's
registry *is* the authority, and the posture's limits are measured against
it at the moment of the decision.

The share is a one-second window, matching the governor's per-second rate
accounting:

```rust
pub fn charge_relay(&mut self, bytes: u64, now_seconds: u64) -> Result<(), TransitRefusalReason>
```

A window may spend at most the permitted share, and the next window starts
from zero. Exceeding the share **refuses** the relay; it never grows the
budget.

### WP3 — the revocation paths

Three typed reasons stop participation, and each is prompt, counted, and
recorded:

| Reason | Trigger | How it is detected |
| --- | --- | --- |
| `PolicyChanged` | the operator changed the posture | `apply_config` re-derives the posture; turning participation off releases the participating tunnels and records the stop |
| `PermitWithdrawn` | the live governor is a different instance, or no live governor is offered | `permit.is_current_for(governor)` is false at the point of use |
| `HealthLost` | the issuing governor is gone or its accounting state is poisoned | `permit.is_healthy()` is false at the point of use |

Health is checked **before** the withdrawal check, because the two are
different facts: a dropped issuing governor is a health loss, and a
replaced governor is a withdrawal. Recording them under the same reason
would hide which one happened.

Revocation is idempotent: a second revocation of the same reason does not
double-count the stop.

### WP4 — the selection diversity

`crates/i2pr-daemon/src/destination_peers.rs` gained `HopDiversityPolicy`
— the /16 rule and the per-peer participation cap, taken from
`i2pr-netdb` (`DEFAULT_MAX_PER_PREFIX16`, `DEFAULT_MAX_PARTICIPATION`)
rather than restated. `DestinationPeerCandidate` carries its `/16`
prefix (`network_prefix16` over the peer's RouterHash), and the bounded
backtracking search honours `max_per_prefix16` with a prefix counter: the
rule is never relaxed to reach the requested hop count.

`crates/i2pr-daemon/src/destination_tunnels.rs` gave the
`DestinationTunnelCoordinator` a bounded per-peer participation ledger
(Plan 488's `ParticipationLedger`) and a `record_hop_selection` method, and
`crates/i2pr-daemon/src/service_product.rs` records every selection at the
three live call sites (the qualification-profile selection and the inbound
and outbound selections of the role build). A selection that is never recorded
would leave the cap advisory.

### WP5 — the receipts

A receipt carries the event kind, an optional typed refusal reason, the
participating-tunnel count, the admitted-peer count, the relayed bytes, the
refusal counters, and the stop count. It never carries a peer hash, an
address, a tunnel id, a key, or a payload — and the static guard proves
that by inspecting the receipt **type**, not merely its values.

The receipt log is bounded at 256 entries and the admitted-peer record at
4 096 peers.

### WP6 — the private-mesh lane

`crates/i2pr-daemon/tests/transit_participation_lane.rs` proves, in three
parts:

1. **The posture admits the peer's transit work.** The peer's short tunnel
   build reaches the transit data plane through the production live owner
   (`TransitLiveOwner::handle_inbound`), the posture admits it, and the
   receipt records the admission.
2. **The router relays the peer's transit traffic.** The peer's tunnel data
   reaches the transit path, the path forwards it, the share is charged
   **before** the cell leaves the router, and the receipt — this router's
   own accounting, not a value the peer asserted — shows the relayed bytes.
3. **The participation is then revoked and the stop is observed.** The
   operator posture changes to off, the participation stops, the stop is
   counted and recorded, and the next transit request is refused with a
   typed reason.

The lane writes a sanitized artifact:
`tests/integration/network-participation/evidence/plan492/result.json` —
counts and a receipt digest only.

**The mesh limitation is recorded, not hidden (§11, F1).** A private mesh
has no established session to the *next* router, so the transit service
cannot complete the build's delivery and rolls the participant
registration back — the correct fail-closed behaviour. The lane asserts
that the posture admitted the request *before* that happened, and then
establishes the registration through the transit service's own transaction
(the same one the live owner drives, minus the delivery step a private
mesh cannot complete). The **relay itself** — the work the posture
accounts — is then driven through the production live owner: the forward,
the share charge, the receipt, and the revocation are all production
paths.

### WP7 — the offline controls and the inverted probes

`crates/i2pr-daemon/tests/transit_participation_posture.rs` holds the
offline controls: the default posture, the enforced-share requirement, the
share limit, the participating-tunnel count, the per-peer cap, the three
revocation paths, the restart, the bounded count-only receipts, and the
named bounds.

`crates/i2pr-daemon/tests/transit_participation_boundary.rs` holds the
static guard. Nine rules, each with an **inverted probe**: a synthetic
violating source must be reported, so a rule that stops detecting its
violation fails its own probe. The guard strips comments before matching
(a doc comment that merely names a construct is not a violation) and
excludes `#[cfg(test)]` blocks, and one control asserts both.

### Design decisions

- **The posture is a separate surface from the published transit
  counters.** `TransitParticipation::Disabled` still answers the three
  Proposal 170 selectors with `0` / `0` / `0.0` for the default posture.
  Plan 492 does not change the published counters.
- **The transit owner has exactly one way to admit work.** `enable` now
  takes the participation binding, so installing the transit machinery
  without a posture cannot relay anything. The M11 controlled lanes
  install the loosest bounded posture; the service's own admission policy
  (`max_active = 4`, `max_active_per_peer = 2`) stays the governing limit.
- **The permit is revalidated, not consulted once.** The permit is
  revalidated at each point of use, so a governor replaced by a
  configuration change stops the participation rather than being ignored
  for the life of the process.
- **The counts are the data plane's.** See §1's deviations.
- **The share is charged before the forward.** Charging after the forward
  would let the router overspend the share by exactly one cell; charging
  first makes the refusal fail closed.

---

## 6. Invariant review

| Invariant | How it is held | Where it is proven |
| --- | --- | --- |
| I1 — The default posture does not participate and the default configuration does not change. | The switch is `#[serde(default)]` on a `bool`, so it is `false` unless the operator sets it. | `transit_participation_is_disabled_by_default_with_bounded_defaults`; the guard's rule 1 and its inverted probe. |
| I2 — The participation posture is bounded, validated, and default-off. | Named ceilings; `normalize_transit` runs before any owner or listener exists. | `transit_participation_rejects_unbounded_or_contradictory_limits`; `the_policy_bounds_are_named_and_bounded`. |
| I3 — The share limit is enforced, not advisory. | The permit is revalidated at the point of use; the share is charged before the forward; an over-share relay is refused. | `the_share_limit_is_enforced_rather_than_growing`; the guard's rule 3 and its inverted probe. |
| I4 — Revocation is prompt, bounded, counted, and recorded. | Three typed reasons; the stop is counted and receipted; revocation is idempotent. | The three revocation controls; the guard's rule 4 and its inverted probe. |
| I5 — No secret, no peer identity, and no payload in a receipt. | The receipt type carries counts only; the guard inspects the type. | The guard's rule 6 and its inverted probe. |
| I6 — The dispatch order is unchanged. | The posture is consulted after the creator correlation check and before the transit dispatch. | The guard's rule 2 and its inverted probe. |
| I7 — The router's own build hops are diverse and the cap is not advisory. | The /16 rule in the bounded search; every selection recorded. | The four `destination_peers` controls; the guard's rules 7 and 8. |
| I8 — The role is not advertised and the wire does not change. | No I2NP message, no RouterInfo option, no `caps` change; the inventory surface is `experimental` with `advertised = false`. | The guard's rule 9 and its inverted probe; the floor's `check-tooling-inventory`. |
| I9 — A restart re-derives the accounting from zero. | No persisted participation state exists. | `a_restart_re_derives_the_accounting_from_zero`. |
| I10 — No boundary script is weakened. | None was edited. | The floor's boundary steps; `git diff --stat -- scripts/` is empty for this plan. |
| I11 — Fail closed, never skip silently. | Every refusal is typed and counted, including the ones the revalidation itself produces. | `record_refusal`; the posture controls; the lane's refused-request assertion. |

---

## 7. Verification

### 7.1 Focused runs

| Command | Outcome |
| --- | --- |
| `cargo test --locked -p i2pr-daemon --lib transit -- --test-threads=1` | **75 passed, 0 failed** (the participation owner's unit tests and the transit owner's) |
| `cargo test --locked -p i2pr-daemon --lib destination_peers -- --test-threads=1` | **13 passed, 0 failed** |
| `cargo test --locked -p i2pr-daemon --test transit_participation_lane -- --test-threads=1` | **2 passed, 0 failed** |
| `cargo test --locked -p i2pr-daemon --test transit_participation_posture -- --test-threads=1` | **12 passed, 0 failed** |
| `cargo test --locked -p i2pr-daemon --test transit_participation_boundary -- --test-threads=1` | **19 passed, 0 failed** |
| `cargo test --locked -p i2pr-daemon --test m11_transit_live_owner -- --test-threads=1` | **41 passed, 0 failed** (the M11 lane, with the posture installed) |
| `cargo check --locked --workspace --all-targets` | clean |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | clean |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | clean |
| `cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1` | **197 test binaries, 0 failed** (§7.3) |

The sweep ran on the implementation tree (`d7f4323c`); the closing tree
(`f0e44798`) differs from it only by the rustfmt fix and the
`crypto-bigint` patch bump, and the floor's own `cargo-test-workspace`
step is the authoritative whole-workspace result on the closing tree
(§7.2, §7.4).

### 7.2 The AGENTS.md routine floor

The floor from `AGENTS.md` was run **twice**: once on the implementation
tree, which was **red**, and once on the fixed tree, which is **green** in
every step. The routine is exactly the one `AGENTS.md` specifies — 84
steps — plus the plan-focused suites the previous phases added.

| run | tree | outcome | what it caught |
| --- | --- | --- | --- |
| 1 | `d7f4323c` | **2 FAIL / 84** — `cargo-fmt`, `cargo-deny` | The plan's own new code was not rustfmt-clean, and the RustCrypto advisory database had gained `RUSTSEC-2026-0387` for `crypto-bigint` after Plan 491's floor run. |
| 2 | `f0e44798` | **0 FAIL / 84** | — |

The step table for the closing run is §7.4. The two failures are
itemised in §7.3, and both were fixed in code or in the lockfile — never
by relaxing a check.

Cargo stops the workspace test run at the first failing test binary, so
the first run's workspace step could not have reported every failure at
once. A separate `--no-fail-fast` sweep of the whole workspace resolved
every binary at once on the same tree: **197 test binaries ok, 0 failed**
(after the one sweep-caught fix below).

### 7.3 The failures the sweep and the floor caught, and what each cost

The plan's discipline is that a green floor is the *minimum*, and that the
checks are the mechanism. **Four** real defects were caught — three by the
workspace sweep and the routine floor, and one by the plan's own inverted
probes before the floor ever ran — and every one was fixed in code or in
the lockfile rather than by relaxing a check.

**The workspace sweep caught the first one.** The `--no-fail-fast` sweep
over all 197 test binaries reported exactly one failure:

```text
---- the_receipts_are_bounded_and_carry_counts_only stdout ----
thread 'the_receipts_are_bounded_and_carry_counts_only' panicked at
  crates/i2pr-daemon/tests/transit_participation_posture.rs:69:26:
valid transit posture: Semantic { field: "transit.peer_participation_cap",
  reason: "must be between 1, the bounded per-peer ceiling, and the
  participating-tunnel count" }
```

The cause was a **control** that built its posture with a per-peer
participation cap of 32 — above the bounded per-peer ceiling of 8 — so
the configuration was correctly refused and the control never ran. The
production code was right; the control was wrong. Fixed in `d7f4323c`: the
control now uses the bounded posture it means to exercise.

**The floor's `cargo-fmt` step caught the second one (run 1).** The plan's
own new code was not rustfmt-clean: **59 hunks across the ten files the
plan touched** — `config.rs`, `destination_peers.rs`,
`destination_tunnels.rs`, `transit_owner.rs`, `transit_participation.rs`,
and the five test files. Every hunk is a line-wrapping change; no
statement, no binding, and no test assertion changed, and no behaviour
changed with it. Fixed in `f0e44798` by `cargo fmt --all`. **Cost:** one
commit, and the diff is mechanical. This one *was* introduced by the
plan: the rest of the workspace was already clean.

**The floor's `cargo-deny` step caught the third one (run 1).** This one
is **not** the plan's fault, and the record says so plainly. Plan 492
added no dependency, and `git diff 02ccb197..HEAD -- Cargo.lock` was
**empty** before the fix. The RustCrypto advisory database gained a new
advisory on 2026-10-23 — after Plan 491's floor run, which is why that
run's `cargo-deny` step passed:

```text
error[vulnerability]: Multiple timing variability issues in operations
intended to be constant-time
   └── crypto-bigint v0.7.5
       └── crypto-primes v0.7.2
           └── sad-rsa v0.10.2
               └── i2pr-su3 v0.1.0
                   ├── i2pr-daemon v0.1.0
                   └── i2pr-netdb v0.1.0
```

`RUSTSEC-2026-0387` covers `crypto-bigint` 0.7.5: subtraction, zero
counting, division, and Montgomery multiplication could execute
secret-dependent branches, and `UintRef` equality could terminate at the
first unequal limb. The advisory is patched in 0.7.6.

This is a **production** path, not a dev-only one — `crypto-bigint` ←
`crypto-primes` ← `sad-rsa` ← `i2pr-su3` ← `i2pr-daemon` and
`i2pr-netdb` — so the floor was right to fail closed. Fixed in `279f6ae4`
with `cargo update -p crypto-bigint --precise 0.7.6`: a single transitive
**patch** release that satisfies the existing `^0.7` constraint, so no
manifest changed, no dependency was added, removed, or re-scoped, and the
lock diff is **two lines** (the version and its checksum). `cargo deny
check advisories bans sources` now reports `advisories ok, bans ok,
sources ok`. **Cost:** one commit, two lockfile lines.

The dependency review this bump required: **purpose** — a patch-level
security fix inside an existing crate; **transitive impact** — none
beyond the `crypto-bigint` → `crypto-primes` → `sad-rsa` chain, which
is the `rsa` crate's own arithmetic and is used by `i2pr-su3`'s SU3
signature verification; **`unsafe` exposure** — unchanged, the crate is
the same one with a smaller amount of secret-dependent branching;
**features** — unchanged, the resolution is selected by the patch
release's default features; **license** — unchanged, `MIT OR Apache-2.0`
in both releases.

**The plan's own inverted probes caught a fourth one**, before the floor
ever ran. The static guard's rules were written with an
`Option`-returning "violation or not" convention, and the first
implementation used `.ok_or_else(...)?` — which returns `None` (no
violation) on the *missing item* path, the opposite of what the rule
meant. The inverted probes for rules 1, 2, and 3 failed, which is exactly
what they exist for: a guard that cannot detect its own violation must
fail loudly rather than pass silently. The rules were rewritten with
explicit `match`/`let-else` handling, and the probes then passed.

The probes also caught a **false negative in the rule-9 check**: the rule
looked for the support-inventory surface, which did not exist yet, and
correctly reported it — which is how the inventory entry was found to be
missing before the docs commit rather than after.

### 7.4 The floor step table

| # | Step | Outcome | Seconds |
|---|---|---|---|
| 1 | `cargo-fmt` | PASS | 14 |
| 2 | `cargo-check` | PASS | 1 |
| 3 | `cargo-build-siblings` | PASS | 1 |
| 4 | `cargo-build-static-fixture` | PASS | 0 |
| 5 | `cargo-copy-static-fixture` | PASS | 0 |
| 6 | `cargo-rebuild-plain-fixture` | PASS | 0 |
| 7 | `cargo-test-workspace` | PASS | 1999 |
| 8 | `cargo-clippy` | PASS | 35 |
| 9 | `cargo-doc` | PASS | 28 |
| 10 | `cargo-test-doc` | PASS | 6 |
| 11 | `check-dependency-direction` | PASS | 0 |
| 12 | `check-global-plan-number-uniqueness` | PASS | 1 |
| 13 | `check-adr-number-uniqueness` | PASS | 0 |
| 14 | `check-portable-service-tunnel-api` | PASS | 0 |
| 15 | `check-portable-service-tunnel-consumer` | PASS | 13 |
| 16 | `planning-tests` | PASS | 18 |
| 17 | `check-runtime-boundaries` | PASS | 0 |
| 18 | `check-console-boundaries` | PASS | 9 |
| 19 | `check-console-browser-security` | PASS | 0 |
| 20 | `check-tooling-inventory` | PASS | 0 |
| 21 | `check-license-metadata` | PASS | 0 |
| 22 | `check-managed-app-private-client-seams` | PASS | 0 |
| 23 | `check-router-readiness-contract` | PASS | 0 |
| 24 | `check-router-readiness-contract-self` | PASS | 1 |
| 25 | `check-core-router-recovery-contract` | PASS | 0 |
| 26 | `check-core-router-recovery-contract-self` | PASS | 0 |
| 27 | `plan449-ssu2-preflight-self` | PASS | 0 |
| 28 | `check-managed-app-package-boundary` | PASS | 0 |
| 29 | `check-managed-app-package-boundary-self` | PASS | 0 |
| 30 | `check-managed-app-policy-boundary` | PASS | 0 |
| 31 | `check-managed-app-policy-boundary-self` | PASS | 0 |
| 32 | `check-managed-app-secured-sandbox` | PASS | 0 |
| 33 | `check-managed-app-secured-sandbox-self` | PASS | 0 |
| 34 | `check-service-tunnel-boundaries` | PASS | 0 |
| 35 | `check-managed-app-gateway-boundary` | PASS | 0 |
| 36 | `check-managed-app-manager-boundary` | PASS | 0 |
| 37 | `check-m11-per-epoch-composition` | PASS | 0 |
| 38 | `check-service-anonymity-boundaries` | PASS | 0 |
| 39 | `check-fixture-manifest` | PASS | 0 |
| 40 | `check-ntcp2-vectors` | PASS | 0 |
| 41 | `check-ssu2-vectors` | PASS | 0 |
| 42 | `check-i2cp-vectors` | PASS | 15 |
| 43 | `check-ntcp2-interoperability` | PASS | 0 |
| 44 | `check-constrained-host-lane-boundary` | PASS | 0 |
| 45 | `check-m11-transit-boundaries` | PASS | 1 |
| 46 | `check-m11-transit-qualification-evidence` | PASS | 1 |
| 47 | `check-sam-acceptance-evidence` | PASS | 0 |
| 48 | `check-ssu2-acceptance-evidence` | PASS | 0 |
| 49 | `check-i2cp-acceptance-evidence` | PASS | 0 |
| 50 | `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| 51 | `check-els2-type11-transcript-boundary` | PASS | 0 |
| 52 | `check-els2-live-lane-evidence` | PASS | 2 |
| 53 | `check-encrypted-service-consumer-caller` | PASS | 0 |
| 54 | `check-outproxy-request-path` | PASS | 0 |
| 55 | `check-outproxy-wire-lane-evidence` | PASS | 0 |
| 56 | `check-config-secret-hygiene` | PASS | 0 |
| 57 | `check-workflow-validity` | PASS | 1 |
| 58 | `check-floodfill-type5-serve` | PASS | 0 |
| 59 | `check-service-tunnel-acceptance-evidence` | PASS | 0 |
| 60 | `check-exploratory-tunnel-evidence` | PASS | 0 |
| 61 | `check-netdb-tunnel-evidence` | PASS | 0 |
| 62 | `check-destination-tunnel-evidence` | PASS | 1 |
| 63 | `check-streaming-tunnel-evidence` | PASS | 5 |
| 64 | `check-m6-mixed-router-acceptance-evidence` | PASS | 3 |
| 65 | `check-m12-floodfill-qualification-evidence-self` | PASS | 0 |
| 66 | `check-m12-floodfill-boundaries-self` | PASS | 0 |
| 67 | `ntcp2-harness-tests` | PASS | 0 |
| 68 | `plan488-mesh-lane` | PASS | 1 |
| 69 | `plan488-evidence-deterministic` | PASS | 0 |
| 70 | `plan489-mesh-lane` | PASS | 1 |
| 71 | `plan489-evidence-deterministic` | PASS | 0 |
| 72 | `plan490-mesh-lane` | PASS | 0 |
| 73 | `plan490-evidence-deterministic` | PASS | 0 |
| 74 | `plan490-ssu2-transport-suite` | PASS | 7 |
| 75 | `plan490-transport-boundary-guard` | PASS | 0 |
| 76 | `plan491-mesh-lane` | PASS | 1 |
| 77 | `plan491-evidence-deterministic` | PASS | 0 |
| 78 | `plan491-boundary-guard` | PASS | 0 |
| 79 | `plan491-proto-suite` | PASS | 0 |
| 80 | `plan491-netdb-suite` | PASS | 91 |
| 81 | `plan491-client-suite` | PASS | 37 |
| 82 | `plan492-posture-suite` | PASS | 4 |
| 83 | `plan492-m11-lane` | PASS | 4 |
| 84 | `cargo-deny` | PASS | 6 |


### 7.5 The false-pass audit

| Candidate false pass | Why it is not one |
| --- | --- |
| The guard's rules could pass vacuously (an empty or unparsable source). | Every rule's *missing item* path returns a violation, and each rule has an inverted probe that proves it. |
| The lane could pass without relaying. | The lane asserts the disposition is `Forwarded` and that `bytes_relayed` advanced by exactly the cell size. |
| The receipts could carry a peer identity and still pass. | The guard inspects the receipt **type**, and the lane's evidence file carries counts and a digest only. |
| The floor could pass with a stale binary. | The floor runs `cargo check --all-targets`, `cargo build` of the managed-app siblings, `cargo test --workspace --all-targets`, and `cargo clippy --all-targets` on the same tree, and the floor's own log is the record. |
| The share could be advisory. | The over-share control refuses and the byte counter does not grow; the guard's rule 3 asserts the charge precedes the forward. |
| A refusal could be a silent skip. | Every refusal path increments a counter and writes a receipt; the sweep caught the one path (the revalidation's own refusal) that did not, and it was fixed. |
| The transit role could have been advertised. | The guard's rule 9 checks the inventory's prose *and* the new surface's `advertised` flag, with an inverted probe. |
| The floor's green could hide a dependency advisory. | The floor's `cargo-deny` step is a real, failing-closed check: it caught `RUSTSEC-2026-0387` in the **production** graph, the fix is in the lockfile, and the step now reports `advisories ok`. |
| The re-run could have passed on a tree the floor did not test. | The two floor runs are on two different recorded commits (`d7f4323c` and `f0e44798`), and the closing tree is the second one. |

---

## 8. Migration and compatibility

- **No migration.** The posture is a new, optional configuration section.
  A configuration without `[transit]` parses unchanged and does not
  participate. Nothing is persisted, so there is no state to migrate and
  no projection format to change.
- **No wire change.** No I2NP message, no RouterInfo option, no change to
  `router.version`, `caps`, or the published RouterInfo. `i2pr-tunnel` is
  unchanged by this plan.
- **No advertisement.** The inventory holds **94** surfaces and **0**
  advertised entries; the new surface is `experimental` with
  `advertised = false`.
- **The M11 controlled lanes changed shape, not behaviour.** The transit
  ingress owner's `enable` now takes the participation binding, so the M11
  lanes install the loosest bounded posture. The transit service's own
  admission policy remains the governing limit, and the M11 lane's 41 tests
  pass unchanged.
- **The default configuration's meaning is unchanged.** A router that has
  never enabled participation does not relay another router's tunnel
  traffic.

---

## 9. Security and contention

- **Secrets.** The posture holds no secret. The permit it holds is a
  governor capability, not a key; the receipts carry counts only. No
  `Debug` or `Display` impl prints a peer's identity: the owner's `Debug`
  prints the policy, the counters, and the receipt count.
- **A dependency advisory was fixed on the way through.** The closing tree
  carries `crypto-bigint` 0.7.6, which removes the secret-dependent
  branches in `RUSTSEC-2026-0387` from the `sad-rsa` → `crypto-primes` →
  `crypto-bigint` chain that `i2pr-su3` pulls into the production graph.
  The posture's own code adds no key material and no new secret-bearing
  surface; the permit it holds is a governor capability, and a withdrawn
  or stale permit *revokes* participation rather than extending it.
- **Contention.** The owner is a single-writer, synchronous accounting
  surface guarded by the caller's lock. It holds no lock across an
  `.await`: the live owner consults it synchronously inside the inbound
  dispatch. `record_hop_selection` takes the coordinator's lock and
  releases it around the update only, and it is called *after* the
  selection, never while another lock is held.
- **Resource bounds.** The receipt log (256), the admitted-peer record
  (4 096), the participating-tunnel count (32), the per-peer cap (8), the
  share limit (100 percent), the share window (one second), and the
  candidate projection (256) are all bounded by named constants.
- **Anonymity.** Nothing this plan adds reveals a peer identity in a
  receipt, an evidence file, or a log line. The posture deliberately does
  **not** advertise the transit role, so the router's participation is not
  discoverable from its published RouterInfo.
- **Fail closed.** A posture that cannot be enforced refuses. There is no
  path where an unenforced limit degrades into an advisory one.

---

## 10. Documentation and operational changes

| Artifact | Change |
| --- | --- |
| `docs/architecture/i2pr-daemon.md` | A new "Transit participation posture (Plan 492)" section: the configuration surface, the owner, the binding, the point-of-use permit revalidation, the unchanged dispatch order, the count-only receipts, the /16 diversity rule, and the recorded deviation about `i2pr-client`. |
| `docs/architecture/i2pr-runtime.md` | A new "The governor's permit as an enforced share (Plan 492)" section: the three questions the permit answers and how the posture asks them at the point of use. |
| `docs/architecture/i2pr-tunnel.md` | A note that Plan 492 added no tunnel-crate code and that the posture's counts are read from the transit registry. |
| `specs/CONFORMANCE.md` | A new "Transit participation posture (Plan 492)" section stating exactly what is claimed and what is not. |
| `specs/support.toml` | One `experimental` surface, `daemon.transit-participation-posture`, with `advertised = false`; the `m11_transit_tunnels` prose extended with Plan 492's honest position; `plan_492_plan_of_record`, `plan_492_closure`, and `plan_492_status` added. The inventory holds **94** surfaces and **0** advertised entries. |
| `plans/implementation/network-participation/492-transit-participation-posture.md` | The status line now reads `closed as passed-transit-participation-posture`. |

**Operational note for the operator.** The posture is inert unless
`[transit] participation = true` **and** `[bandwidth]` is enabled. The
configuration refuses the first without the second, so an operator never
gets an unenforced share by accident.

---

## 11. Known limitations

1. **The private mesh cannot complete a build's delivery.** It has no
   established session to the next router, so the transit service rolls the
   participant registration back — correctly. The lane therefore
   establishes the registration through the transit service's own
   transaction and drives the *relay* through the production live owner.
   A lane that proves the whole loop against a live next router needs the
   authorized two-host topology (Plan 493) or a loopback SSU2 session pair.
2. **The product's public composition installs no transit data-plane
   owner.** Production `lib.rs` still consults
   `controlled_transit_disabled_probe`, which the M11 boundary script
   (`check-m11-transit-boundaries.sh` rule 10) requires on the inbound
   path. The posture is therefore honoured wherever the transit owner is
   installed — the controlled path — and the *public-network* installation
   of a transit data-plane owner is **not** part of this plan.
3. **The share is accounted at the router level, not metered at the
   socket.** The governor remains the socket-level enforcement; the posture
   accounts the transit role's spend inside the enforced share. A real
   bandwidth *metering* implementation is a separate and larger piece of
   work, and it is open in the reference implementations too.
4. **The participation owner's reported participating-tunnel count is the
   count observed at the last admission decision.** The authority is the
   transit service's registry; the owner reports the last observed value
   rather than keeping a ledger that could drift. The *limits* are always
   measured against a fresh count.
5. **`i2pr-client` is unchanged.** The plan named it for the selection
   diversity; the ordinary build-hop selection is the daemon's. See §1.

---

## 12. Findings by severity

### Medium — open

**F1 — The private mesh cannot complete the build's delivery, so the relay
is exercised against a registration the transit service's own transaction
established.** The *relay* — the forward, the share charge, the receipt,
the revocation — is a production path through
`TransitLiveOwner::handle_inbound`. The build's *delivery* is not: a
private mesh has no established session to the next router, and the transit
service correctly rolls the registration back. Owner: the two-family
public qualification (Plan 493), or a loopback SSU2 session pair.

**F2 — The product's public composition installs no transit data-plane
owner, so the posture is honoured only where the controlled path installs
one.** Production `crates/i2pr-daemon/src/lib.rs` never constructs a
transit data-plane owner; the inbound path consults
`controlled_transit_disabled_probe` instead, and `lib.rs` says so in a
comment at the publication site. `check-m11-transit-boundaries.sh`
**rule 10** requires production `lib.rs` to consult that probe on the
inbound path (and to reference the `transit_owner` module), and
**rule 3** requires a production caller of `TransitBuildService` outside
its own test module — a rule the `transit_owner` module's
`TransitOwner::dispatch_short_build` satisfies, without the composition
root instantiating anything. Replacing the probe with a live transit owner
in the composition root would therefore mean **editing a boundary
script**, which the plan named as a stop condition; it was not done. The
operator opt-in is complete as a *posture* — the configuration is real,
validated, and the single source of the policy — but a router that has
never been given a transit owner does not relay. Owner: the plan that
installs the transit owner in the product composition.

**F3 — There is no stock-peer evidence for the posture.** Neither the
pinned i2pd 2.61.0 nor Java I2P 2.13.0 exercises this router's posture,
because a stock-peer transit lane needs the authorized two-host topology
Plan 493 owns. The posture is proven against a private mesh only.

### Low — open

**F4 — The comparator-free posture has no equivalent of Plan 491's
version floor, so a peer that offers a transit build to a router with
participation off is refused with `ParticipationDisabled` rather than with
a reason derived from a published level.** This is correct behaviour, and
it is recorded here only so that the refusal vocabulary stays visible:
`participation-disabled`, `revoked`, `participating-tunnel-limit`,
`peer-participation-cap`, `share-limit-exceeded`, `no-enforced-permit`.

**F5 — The share window is one second, matching the governor's per-second
rate accounting, so a burst inside a window is refused rather than
smoothed.** A router with a low share limit can therefore drop a burst of
relay traffic it would have been willing to spread over two windows. The
governor's own burst allowance is not extended to the transit role.

### Informational

- The support inventory's `surface` list is a **list**, not a table, and
  the tally is derived with `tomllib` over `d['surface']`. The inventory
  holds 94 surfaces: 88 `experimental`, 5 `deferred`, 1
  `ready-control-plane-and-publication-complete-black-box-evidence-landed`,
  and **0** advertised.
- The signed-RouterInfo `ProtocolVersion` validation already rejects
  values that are not digits and dots, so the reference to Plan 491's
  `Uncomparable` state does not apply here: the posture never compares a
  version.
- `scripts/check-runtime-boundaries.sh` still has **no `i2pr-netdb`
  section** — Plan 491's F3, still open. Plan 492's new code is in
  `i2pr-daemon`, which the script polices, so this plan does not widen the
  gap; it does not close it either.
- **`crypto-bigint` is in the production graph through `i2pr-su3`.** The
  `sad-rsa` → `crypto-primes` → `crypto-bigint` chain that Plan 492's
  floor caught via `RUSTSEC-2026-0387` is a *production* dependency of
  `i2pr-daemon` and `i2pr-netdb`, not a dev-only one, so the advisory
  check was right to fail closed (§7.3). The bump to 0.7.6 is the fix, and
  it is recorded here because it is a dependency change that happened
  inside a plan that was not about dependencies.

### No findings

- No secret, key, token, or payload in a receipt, a log line, or an
  evidence file.
- No unbounded channel, queue, map, or log.
- No lock held across an `.await` by the new code.
- **No new dependency added by the plan.** The plan itself added no
  dependency and changed no manifest; the only dependency-graph change in
  the closing commits is the `crypto-bigint` **patch** bump that the
  floor's advisory check forced (§7.3), which adds, removes, and re-scopes
  nothing.
- No new I2NP message, RouterInfo option, or published field.
- No boundary script weakened, and no check skipped to go green.

---

## 13. Roadmap disposition

Plan 492 is **closed** as `passed-transit-participation-posture`. The
network-participation roadmap's §6 graph, §7 milestone table, and §12
status summary are advanced, and the plan document's status line is
updated.

The roadmap's prose carried an **off-by-one** in its plan numbers — it was
written when 492 was the two-family qualification — and the closure
corrected it, because a live roadmap that mis-states which phase is
topology-gated mis-states the blocker:

- the dependency table's "the authorized two-host topology Plan 492 needs"
  now names **Plan 493**;
- "selection and role classes Plan 492/493 consume" now names **493/494**;
- the verification strategy's "Two-family matrix (492–493)" now names
  **(493–494)**;
- the verification strategy's "Private mesh lane (488–491)" now names
  **(488–492)** — the bullet already enumerated Plan 492's own evidence
  item ("a relayed tunnel's traffic"), so the range was behind its own
  list;
- the topology-gate risk now names **Plan 493** and "493 and 494 stay
  blocked";
- the completion definition's "488–491 passed and 492 blocked on the
  topology gate" now reads "488–492 passed and 493 blocked".

The "Exact-pinned single-family lane (488–491)" range was deliberately
**left alone**: Plan 492's lane is a private mesh with no stock peer
(F3), so it does not belong to the exact-pinned lane's range.

The roadmap's completion definition (§11) is now satisfied up to the
two-family items: the router keeps a bounded tiered peer store, maintains
a bounded date-keyed floodfill-indexed routing table with an exploration
job, answers a peer's SSU2 Relay Request as the introducer, honors the
minimum-peer floors its published `router.version` attaches, and relays
tunnels for other routers under an operator-opted, bounded, revocable
participation policy with a measured share. The two-family interoperation
and the floodfill `caps=f` advertisement remain open.

---

## 14. Unblock audit

**Registered plans naming Plan 492.** The registry's network-participation
block lists Plans 488–494. **Plan 493** names Plan 492 directly — its hard
dependencies are "Plans 488–492 closed". **Plan 494** names it in its plan
document's predecessor lineage (`488 → 489 → 491 → 492 → 493 → **494**`)
while listing transit participation as **explicitly out of scope** for the
floodfill role; the registry's Plan 494 row itself names Plan 491 as its
last direct predecessor and states "Blocked on Plan 493". Plan 493's own
hard dependencies are unchanged and still unmet:

| Dependency | State | Effect on Plan 493 |
| --- | --- | --- |
| The authorized two-host topology (Plans 431, 472, 476, 480, 449) | Plan 431 remains **stopped** at exactly that boundary | Plan 493 stays **blocked** |
| Plan 435's transport activation | not closed | Plan 493 stays **blocked** |
| Plan 491's version floors | **closed** (`passed-api-version-floors-and-peer-eligibility`) | satisfied |
| Plan 490's introducer role | **closed** (`passed-ssu2-introducer-relay-role`) | satisfied |
| Plan 489's routing table | **closed** (`passed-floodfill-indexed-routing-table`) | satisfied |
| Plan 488's peer profile store | **closed** (`passed-peer-profile-store`) | satisfied |
| Plan 492's transit participation posture | **closed** (`passed-transit-participation-posture`) | satisfied |

**Conclusion: no plan is unblocked by this closure.** Plan 493 moves from
"blocked on Plan 492" to "blocked on the authorized two-host topology and
Plan 435", and Plan 494 stays blocked on Plan 493 — Plan 492's closure
removes a *transitive* predecessor from Plan 494's lineage, but Plan 494's
own blocker is Plan 493, which is still blocked. The registry's active
row now reports the line's honest state: **Plans 488–492 closed, 493–494
registered blocked.**

**A corrective pass is not required.** No plan in this line has an
identified source, fixture, or test defect that Plan 492 introduced. F1
and F2 are *scope* limitations of the phase, each owned by a named
successor, and each recorded rather than absorbed.

---

## 15. Open items

| Item | Owner | State |
| --- | --- | --- |
| The public-network transit installation (F2) | the plan that installs the transit owner in the product composition | open |
| The stock-peer transit lane (F3) and the whole two-family matrix | Plan 493 | blocked on the authorized two-host topology and Plan 435 |
| The floodfill role and the `caps=f` advertisement | Plan 494 | blocked on Plan 493 |
| A loopback SSU2 session pair for the private mesh (F1) | the lane owner | open |
| `scripts/check-runtime-boundaries.sh` has no `i2pr-netdb` section | Plan 491's F3 | open |
| Bandwidth metering | unresolved in the reference implementations too | open |

**The honest position after this closure**: `i2pr` is a **controlled-mesh
participant** with an operator-opted, bounded, revocable transit posture.
It is not a router on the public network, and it advertises nothing.
