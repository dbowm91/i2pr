## Status

`passed-transit-owner-in-the-product-composition`

Plan 502 is closed. The daemon's transit data-plane owner is now **installed in
the product composition** behind the operator's existing default-off
`[transit] participation` opt-in, and the production inbound path dispatches
through it. An opted-in router admits and accounts for transit traffic
instead of relaying nothing; the default router is byte-for-byte the same
product as before.

**Plan 492 F2 is discharged.** Its closure record said, verbatim: *"a router
that has never been given a transit owner does not relay. Owner: the plan
that installs the transit owner in the product composition."* That is this
plan. Plan 492's own closure record is **not** rewritten.

**Plan 436 WP3 is advanced source-side only.** Plan 436 stays `blocked` on
Plan 433; its stock-peer qualification (WP5) and the Plan 433 ordering
dependency are untouched, and no transit success against an ordinary router
is claimed here. The registry's Plan 435 row is the precedent for splitting
a blocked plan this way: its source-side work packages were discharged by
Plans 456/461/496 while Plan 435 stayed blocked on the external evidence.

**Nothing is promoted.** The transit role stays non-advertised and
default-off. `specs/support.toml`, `Cargo.toml` and `Cargo.lock` are
byte-identical; `router.version` stays `0.9.62` (`i2pr-netdb`, unchanged);
`caps` and the published RouterInfo are unchanged.

## Commits

| Commit | What |
| --- | --- |
| `e944e10e` | Register Plan 502: the plan-of-record, the registry row, the roadmap row |
| `e9363784` | Correct Plan 502's WP6 — the probe bullet contradicted the plan's own first stop condition |
| `de796b73` | The implementation: the composition module, the production install and dispatch, the creator correlation, the publication, the strengthened boundary rule, the documentation |
| `cb30f55c` | The evidence for the peer reconcile and the creator correlation |
| `596ba7a8` | **Floor-caught fix:** drop the private intra-doc link the doc build rejected |
| `75b28e26` | **Correct the statements Plan 502 falsifies** — the source comments, the normative spec reference, `specs/CONFORMANCE.md` and the architecture deep-dives |
| *(closure)* | This record, the registry and roadmap dispositions |

Base: `1ad06b18` (Plan 501's closure).

## What changed

### The composition module — `crates/i2pr-daemon/src/transit_composition.rs` (new)

The composition is a pure, I/O-free derivation plus construction:

- **`derive_admission_policy`** derives the admission policy from the
  normalized configuration and the **enforced** bandwidth governor. The
  operator's counts are converted with a **checked** conversion
  (`config.transit.participating_tunnels` is `usize`, `max_active` is
  `u16`), and the tunnel crate's `TransitAdmissionPolicy::new` is the
  authority on the ceilings — the derivation never widens a configured
  value to make a policy fit, and a value the constructor rejects is a
  derivation bug, not a value to shrink.
- **`available_bandwidth_kbps` is not an invented number.** It is the
  participation owner's own arithmetic
  (`TransitParticipationOwner::from_config(...).share_bytes_per_second()`),
  so the admission policy and the accounting can never disagree.
- **No new configuration key.** The policy and the registry capacity are
  derived from the existing `[transit]` and `[bandwidth]` fields.
- **The hop material** comes from the persistent router identity bundle —
  the bundle's static private key and *this router's own* identity hash.
  The bundle is moved into the SSU2 service, so the material is taken from
  it before the move, exactly as `local_router_hash` already was.
- **The RNG is seeded from operating-system entropy.**
  `rand_core` 0.9's `OsRng` implements only `TryRngCore`/`TryCryptoRng`
  and cannot satisfy the owner's `R: TryCryptoRng + RngCore + CryptoRng +
  Send` bound; `ChaCha8Rng::try_from_os_rng()` does. A fixed or derived seed
  would not be acceptable, and none is used. `rand_chacha` was already in
  the daemon's dependency graph, so no manifest changed.
- **`build_transit_owner` returns `Ok(None)` for the default posture** and
  constructs **nothing at all** — no service, no counters, no owner, and no
  entropy drawn. A refusal is an `Err`, never a substituted value.

### The production install and the inbound dispatch — `crates/i2pr-daemon/src/lib.rs`

The owner is constructed inside `register_ssu2_service` — the same service
that owns the transport, the inbound pump and the `RouterDeliveryService`.
This is Plan 436's WP3 in its own words: *"the same normal transport /
I2NP pump, not an independent socket."* There is **no second socket, no
second event loop and no second I2NP decode**: the raw `Ssu2InboundI2np`
is handed to the owner, which performs its own single canonical decode.

The inbound branch that used to call `controlled_transit_disabled_probe`
now calls `transit_composition::dispatch_transit_inbound`, which makes the
whole owner/no-owner choice internally:

- owner present → `TransitLiveOwner::handle_inbound`;
- owner absent → the controlled disabled probe, **the fallback for the
  no-owner case only**, so the two can never run beside each other.

`lib.rs` no longer imports or calls the probe at all.

### The peer index and the expiry sweep

`reconcile_transit_peers` reconciles the owner's peer index against the
**live session set** (`Ssu2RuntimeService::active_peer_hashes()`, bounded by
`MAX_SSU2_ACTIVE_CEILING`) and sweeps expired registrations with
`expire(now_seconds)`. `PeerId` is a newtype over the router identity hash
(`crates/i2pr-transport/src/identity.rs:13`), so the mapping is exact.

The reconcile runs on the **existing** pump tick — bounded, never per
message, and never on a new task. Without it the owner would decide to
relay and then report `NoActiveSession`; with a stale entry it would
dispatch to a peer whose session is gone.

### The creator correlation

The owner's documented order is **creator/service first, transit second,
unknown fail-closed**. Without the correlation, the owner would claim the
router's **own** coordinator traffic as transit work and spend the
participation budget on it.

The daemon already had the producers; the plan wired them:

- `ExploratoryBuildCoordinator::reserved_creator_builds()` publishes the
  `(peer, message_id)` pairs of the router's own pending attempts, keyed by
  the attempt's **target peer** — the same key the coordinator itself
  matches an inbound reply against.
- `ServiceProduct::transit_creator_builds()` and
  `transit_creator_tunnel_ids()` expose them to the composition.
- `install_transit_creator_correlation` feeds them into the owner's narrow
  setters (`install_creator_build`, `install_creator_data`,
  `install_creator_gateway`) on the pump tick.

### The participation publication — `crates/i2pr-daemon/src/i2pcontrol_inspection.rs`

The composition root keeps publishing `TransitParticipation::Disabled` for
the default posture. When the owner is installed, the SSU2 service
publishes `TransitParticipation::Enabled(counters)` using the **service's
own** `volume_handle()` — the handle only a router that actually
constructed the service can hold, which is what keeps the product posture
and a controlled lane from being confused for one another. The publication
never happens before the owner exists.

The reader on the inspection surface is now **public**, so an out-of-crate
lane observes the published posture through the same value the
Proposal-170 selectors project. `publish_transit_participation` was already
public; the asymmetry is closed.

### The boundary rule — `scripts/check-m11-transit-boundaries.sh`

Rule 10 was two text searches:

```bash
rg -q 'transit_owner'                    lib.rs
rg -q 'controlled_transit_disabled_probe' lib.rs
```

Both were satisfiable without an installed owner. The second matched a
call to `controlled_transit_disabled_probe`, which is
`TransitOwner::is_transit_managed(outcome)` — a `matches!` with no
dispatch — whose result the caller discarded. **A composition that
installed the live owner *and kept the probe call beside it* satisfied the
literal text while the rule guarded nothing.**

The amended rule is a **function over the two production files**, so it can
be exercised by controls. Eight rules, nine controls, every rule with a
control that fires it, and both counts **derived**:

| Rule | Asserts |
| --- | --- |
| `lib-owner-module` | the legacy assertion, unchanged: `lib.rs` references the live-owner module |
| `lib-composition-dispatch` | `lib.rs` **calls** `dispatch_transit_inbound` — the load-bearing one |
| `lib-no-direct-disabled-probe` | `lib.rs` does not reach the probe at all |
| `lib-no-direct-owner-dispatch` | `lib.rs` does not call `handle_inbound` directly |
| `lib-owner-construction` | `lib.rs` constructs through `build_transit_owner` |
| `composition-disabled-probe` | the composition keeps the no-owner fallback |
| `composition-live-owner` | the composition reaches the live owner's `handle_inbound` |
| `composition-builder` | the composition owns the construction entry point |

The controls include the **vestigial shape the old rule accepted**. The
strictness comparison is itself a control: the pre-Plan-502 text search is
run against the same fixture and is required to **still accept it**, or the
control fails as stale. The old rule **passes** the vestigial shape; the
amended rule reports it on four independent rules.

### The fail-open fix in the script's own scan

The same amendment fixed a defect the plan did not anticipate. The tree
every pre-existing rule scanned was built by stripping each daemon source
file from its **first** `#[cfg(test)]` to EOF:

```bash
awk '/#\[cfg\(test\)\]/{exit} {print}' "$src" > "$dest"
```

`crates/i2pr-daemon/src/lib.rs` declares a test-only module at **line 14**,
so that convention left **13 of 5 770 lines** of `lib.rs` — and 136 of
8 831 lines of `service_product.rs` — visible to all seven pre-existing
rules. Across the daemon, **58 158 production lines were invisible**, and
a forbidden call below a file's first test module would not be seen.

The scan now uses a **scoped, per-item** strip (the Plan 366 house fix) for
the whole production tree. The pre-existing rule **bodies are
byte-identical** — only the tree construction changed — and a **drift
guard** asserts that every mirrored pattern still appears in the script's
shell body, so the mirror cannot rot.

The scanner is a text-level approximation, so it is validated three ways:
a brace-balance check over **70/70** daemon files, a **latching control**
that fails if the blunt convention is substituted, and an end-to-end proof
against an isolated copy of the tree (below).

### The documentation

`docs/architecture/overview.md` (the composition's place and the capability
snapshot), `docs/architecture/tooling.md` (the strengthened rule, the
scoped strip, the controls) and `docs/architecture/i2pr-daemon.md` (the new
module's row, the module list, the exported types).

### The statements Plan 502 falsified

Installing the owner made a set of statements false, and each of them said
the product **cannot** relay. Leaving them would have had the repository
describing a product that does not exist, so each was corrected in
`75b28e26`:

- `crates/i2pr-daemon/src/lib.rs` — the composition root's comment claimed
  *"production composition never constructs a transit data-plane owner … so
  this router relays nothing and says so … Enabling participation is a
  separate decision and is deliberately not reachable from here."*
- `crates/i2pr-daemon/src/transit_volume.rs` — the module doc claimed rule
  10 *"pins that shape"* and that a production router therefore relays
  nothing.
- `crates/i2pr-daemon/src/transit_compose.rs` — three places claimed that
  production profiles never construct a `TransitBuildService` and never
  call `TransitIngressGate::enable`.
- `crates/i2pr-daemon/src/transit_owner.rs` — the owner doc said only
  controlled tests and qualification enable it.
- `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` — the selector doc said
  a product router publishes `Disabled`.
- `specs/references/proposal-170-transit-volume-and-share.md` — the
  normative reference said *"ordinary product profiles never construct a
  transit data-plane owner"*, that rule 10 pinned that shape, and that
  `Enabled` was *"controlled lane only"*. **The normative semantics are
  unchanged**: the posture still decides what is reported, and the disabled
  posture is still structurally incapable of reporting a non-zero value.
- `specs/CONFORMANCE.md` — the honest-baseline bullet, plus a new section
  for Plan 502.
- `docs/architecture/i2pr-daemon.md`, `docs/architecture/i2pr-tunnel.md`
  and the Proposal-170 roadmap — the same corrections in prose.

Every corrected sentence now says what is actually true: the **default**
profile constructs no owner and relays nothing; an **opted-in** profile
installs one and reports measured counters; and the transit role stays
non-advertised and unclaimed for the public network in both.

## Requirement-to-evidence matrix

The plan's §13.1 acceptance criteria, each discharged by an executed command
rather than by reading the code.

| # | Criterion | Evidence |
| --- | --- | --- |
| 1 | The default configuration installs **no** owner, publishes `Disabled`, and the three Proposal-170 transit selectors report `participating = false` and zeros | `transit_composition::tests::the_default_configuration_constructs_no_owner`; `tests/transit_composition.rs::the_default_configuration_installs_no_transit_owner`; `::the_default_publication_reports_no_participation_and_zeros` — asserts the posture is `Disabled`, that `posture.counters()` is `None`, and that the three selectors project `0`, `0` and `0.0` |
| 2 | The opt-in installs an **enabled** owner in the production SSU2 service and publishes `Enabled` with the service's own volume handle | `::the_opted_in_configuration_constructs_an_enabled_owner`; `tests/transit_composition.rs::the_opted_in_configuration_installs_an_enabled_transit_owner` — asserts `owner.is_enabled()` and `owner.volume_counters().is_some()`; `lib.rs` publishes `Enabled(counters)` from `volume_handle()` only when the owner exists |
| 3 | The inbound path dispatches through the installed owner, and the probe is the no-owner fallback **only** — mutually exclusive branches | `dispatch_transit_inbound`'s single `match`; the boundary rule's `lib-composition-dispatch` + `lib-no-direct-disabled-probe` + `lib-no-direct-owner-dispatch`, and their three negative controls |
| 4 | The peer index is reconciled against the live session set and expired registrations are swept, on the existing pump tick | `transit_composition::tests::the_peer_index_is_reconciled_against_the_live_session_set` — a departed session is removed, a live one is installed, a second pass is idempotent; the tick arm in `lib.rs` |
| 5 | The router's own coordinator traffic is not claimed as transit | `exploratory_build::tests::the_reserved_creator_builds_are_the_pending_attempts_keyed_by_peer` — the producer publishes the pending attempts' `(peer, message_id)` pairs, keyed by the target peer; the owner's `CreatorBypass` disposition is covered by the pre-existing `plan254_c17_creator_bypass` |
| 6 | The owner is cancelled and drained deterministically on shutdown | `lib.rs` cancels the owner on **all three** shutdown paths: the outer token, the local-router-info channel closing, and the ingress channel closing |
| 7 | The amended rule 10 is **stronger** than the rule it replaces, and its negative control fires on the vestigial shape | The control set: the vestigial shape is reported on four rules, and the pre-Plan-502 text search is required to still accept the same fixture — the comparison is a control, not an assertion |
| 8 | The evidence asserts a **delivery outcome**, never `Forwarded(_)` alone and never `bytes_relayed` alone | `::the_build_outcome_names_the_delivery_outcome_explicitly` — asserts `RouterDeliveryOutcome::NoActiveSession` concretely, `admitted == 1`, and `bytes_relayed == 0` |
| 9 | `specs/support.toml`, `Cargo.toml`, `Cargo.lock` byte-identical; `router.version` `0.9.62`; `caps` and the published RouterInfo unchanged | `git diff --exit-code 1ad06b18 HEAD -- specs/support.toml Cargo.toml Cargo.lock` → clean; `i2pr-netdb` (which holds `CONTROLLED_ROUTER_VERSION = "0.9.62"`) is unchanged |
| 10 | The routine floor passes in full | The floor table below |

## Commands run

### Focused verification (before the floor)

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked -p i2pr-daemon --all-targets` | PASS |
| `cargo test --locked -p i2pr-daemon --test transit_composition -- --test-threads=1` | PASS — 3 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --lib transit_composition -- --test-threads=1` | PASS — 7 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --lib reserved_creator_builds -- --test-threads=1` | PASS — 1 passed, 0 failed |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | PASS |
| `bash scripts/check-m11-transit-boundaries.sh` | PASS — 8 rules, 9 controls exercised |
| `bash scripts/check-m11-transit-boundaries.sh --self-test` | PASS — tree-independent, 0.047 s |
| `python3 scripts/check-tooling-inventory.py` | PASS — every published figure matches the tree |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests, OK |
| the documentation link checker | PASS — no broken relative links, no out-of-range citations |

### The two measured proofs

**The amended rule rejects what the old rule accepted.** The pre-Plan-502
rule was run verbatim from `git show HEAD` against the same fixture:

```text
--- VESTIGIAL SHAPE (live owner beside a discarded probe) ---
  OLD RULE: PASS
  NEW RULE: FAIL (reported)
```

**The scoped scan sees what the blunt scan missed.** An isolated copy of the
tree was built, a forbidden call was injected into `lib.rs`'s production
region (below its line-14 test module), and both scripts were run against
the same tree:

```text
=== NEW script against the injected violation ===
NEW EXIT=1
/tmp/tmp.r6BymKAkEx/production/crates/i2pr-daemon/src/lib.rs:5775:    let _ = process_short_build_request();
check-m11-transit-boundaries: production daemon code must not call process_short_build_request; use process_short_build_message

=== ORIGINAL script, same tree ===
ORIGINAL EXIT=0
0 — STILL INVISIBLE to the original script
```

The isolated tree was deleted afterwards.

### The routine floor

Two floor runs, both on the closing tree's code, and the first one is
recorded here because **it found a real defect**.

**Run 1** — tree `de796b73` + `cb30f55c` — **99 PASS / 1 FAIL**:

```text
RESULT cargo-test-workspace PASS (1843s)
RESULT cargo-clippy       PASS (1s)
RESULT cargo-doc          FAIL (10s)
```

```text
error: public documentation for `transit_composition` links to private item `dispatch_transit_inbound`
  --> crates/i2pr-daemon/src/transit_composition.rs:23:8
   = note: `-D rustdoc::private-intra-doc-links` implied by `-D warnings`
error: could not document `i2pr-daemon`
```

This was a genuine defect, not a flake: the new module is **public** and its
module doc carried an intra-doc link to the **crate-private** dispatch seam,
so `RUSTDOCFLAGS="-D warnings" cargo doc` could not resolve it. Fixed in
`596ba7a8` — the link became plain code text and the seam stayed
crate-private, because it is an internal composition seam and not public
API.

**Run 2** — tree `596ba7a8` (the closing tree) — **100 PASS / 0 FAIL**,
every step of the `AGENTS.md` routine floor:

| Step | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | PASS |
| the static-fixture build / copy / plain rebuild | PASS |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS (1843 s) |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | PASS |
| `cargo test --locked --workspace --doc` | PASS |
| `check-dependency-direction` | PASS |
| `check-global-plan-number-uniqueness` | PASS |
| `check-adr-number-uniqueness` | PASS |
| `check-portable-service-tunnel-api` / `-consumer` | PASS |
| `planning-tests` (51) | PASS |
| `check-runtime-boundaries` | PASS |
| `check-console-boundaries` / `-browser-security` | PASS |
| `check-tooling-inventory` | PASS |
| `check-license-metadata` | PASS |
| `check-managed-app-private-client-seams` | PASS |
| `check-router-readiness-contract` (+ `--self-test`) | PASS |
| `check-core-router-recovery-contract` (+ `--self-test`) | PASS |
| `plan449-ssu2-preflight --self-test` | PASS |
| `plan495-peer-eligibility` (`--self-test`, `--profile-drift`) | PASS |
| `check-plan476-gate-binding` (+ `--self-test`) | PASS |
| `check-managed-app-package-boundary` / `-policy-boundary` (+ `--self-test`) | PASS |
| `check-managed-app-secured-sandbox` (+ `--self-test`) | PASS |
| `check-service-tunnel-boundaries` | PASS |
| `check-managed-app-gateway-boundary` / `-manager-boundary` | PASS |
| `check-m11-per-epoch-composition` | PASS |
| `check-service-anonymity-boundaries` | PASS |
| `check-fixture-manifest` | PASS |
| `check-ntcp2-vectors` / `check-ssu2-vectors` / `check-i2cp-vectors` | PASS |
| `check-ntcp2-interoperability` | PASS |
| `check-constrained-host-lane-boundary` | PASS |
| `check-m11-transit-boundaries` / `-transit-qualification-evidence` | PASS |
| `check-m11-transit-boundaries --self-test` | PASS |
| `check-els2-type11-transcript-boundary` / `-els2-live-lane-evidence` | PASS |
| `check-encrypted-service-consumer-caller` | PASS |
| `check-outproxy-request-path` / `-outproxy-wire-lane-evidence` | PASS |
| `check-config-secret-hygiene` | PASS |
| `check-workflow-validity` | PASS |
| `check-floodfill-type5-serve` | PASS |
| `check-service-tunnel-acceptance-evidence` | PASS |
| `check-exploratory-tunnel-evidence` / `-netdb-tunnel-evidence` | PASS |
| `check-destination-tunnel-evidence` / `-streaming-tunnel-evidence` | PASS |
| `check-m6-mixed-router-acceptance-evidence` | PASS |
| `check-m12-floodfill-qualification-evidence --self-test` | PASS |
| `check-m12-floodfill-boundaries --self-test` | PASS |
| the NTCP2 harness tests | PASS |
| the Plan 488 / 489 / 490 / 491 / 492 / 496 / 497 / 498 lanes and their evidence-determinism checks | PASS |
| `cargo deny check advisories bans sources` | PASS |

**All local.** No CI run was performed for this plan; the floor is the local
evidence and is labelled as such.

## Invariant review

The plan's §4 invariants, row by row.

| # | Invariant | Verdict |
| --- | --- | --- |
| 1 | Default-off preserved exactly | **Held.** The default posture returns `Ok(None)` before any construction, so no service, no counters, no owner and no entropy are drawn. The composition root still publishes `Disabled`, and the `Disabled` posture owns no counters — the selectors report zeros because there is nothing to read, not because a zero was substituted. Pinned by two rows. |
| 2 | The boundary rule gets stronger, never weaker | **Held, and demonstrated.** Eight rules against two, with nine controls; the old rule is required to still accept the vestigial shape, so the comparison is a control. Nothing was relaxed: the pre-existing rule bodies are byte-identical. |
| 3 | One pump, one decoder, one socket | **Held.** The owner lives in the existing SSU2 service and reuses its `RouterDeliveryService`. The raw inbound is handed to the owner, which performs the single canonical decode; the composition seam never decodes I2NP. |
| 4 | Fail closed | **Held.** An unknown, uncomparable, below-floor or refused peer is refused by the existing posture and counted by reason. A construction failure leaves the router running **without** an owner and never falls back to something that implies participation. |
| 5 | No advertisement, no promotion | **Held.** `specs/support.toml`, `Cargo.toml`, `Cargo.lock` byte-identical; `i2pr-netdb` (the `router.version` owner) unchanged; the transit role stays non-advertised and default-off. |
| 6 | Bounded | **Held for the structures this plan introduces** — the peer index, the registry, the reconcile. **One pre-existing structure is now populated and is not bounded over the router's lifetime; see Finding M1.** |
| 7 | Explicit ownership and cancellation | **Held.** The owner holds the **outer** owner's real `CancellationToken` and is cancelled on all three shutdown paths. No fresh token is created in `transit_owner.rs`. |
| 8 | The evidence asserts the delivery outcome | **Held.** `RouterDeliveryOutcome::NoActiveSession` is asserted concretely, and `bytes_relayed` is asserted as `0`, not as evidence of a relay. |
| 9 | Secrets stay secret | **Held.** The hop material's static private key is redacted in diagnostics and zeroized on drop; no `Debug`/`Display` was added to a secret type. |
| 10 | No dependency change | **Held.** `Cargo.toml` and `Cargo.lock` are byte-identical; `rand_chacha` was already in the graph. |

## Failure review

- **The owner cannot be constructed.** The construction returns an `Err`; the
  daemon logs a warning naming the reason and continues **without** a transit
  owner. It never falls back to a probe that implies participation, and the
  published posture stays `Disabled`.
- **The participation posture refuses.** Every refusal is counted by reason
  through the existing taxonomy; a refusal is never a silent skip.
- **The delivery fails.** `RouterDeliveryOutcome` says so, and the evidence
  asserts that outcome concretely. The share is charged **before** the
  forward, so `bytes_relayed` alone would be a false signal — which is why
  the evidence never asserts it alone.
- **Cancellation.** The owner holds the outer owner's token and is cancelled
  and drained on every shutdown path. `Drop` remains a fail-safe, not the
  plan.
- **Restart.** No stale forwarding state survives as active: the registry is
  not persisted and the owner is rebuilt from configuration.
- **Contention.** The reconcile and the sweep run on the existing pump tick
  and are bounded over the live session set and the transit registry's own
  capacity. They are not per-message and do not take a lock the inbound path
  holds for long.

## Migration review

No migration. No stored format changes. **No configuration key was added or
changed** — the admission policy and the registry capacity are derived from
the existing `[transit]` and `[bandwidth]` fields, and the existing
normalisation and validation are untouched.

The behaviour change is confined to the opt-in. A router configured with
`[transit] participation = true` now installs a transit owner and relays; a
router with the default configuration is the same product as before.

## Security review

- **The opt-in remains an explicit operator decision.** The participation
  posture is off by default, and the daemon's own comment that participation
  was "deliberately not reachable from here" is now a statement about the
  **default**, not about the product.
- **No advertisement.** The router does not claim the transit role in its
  published RouterInfo, and `caps` is unchanged. Nothing here makes the
  router a public transit participant.
- **No secrets leak.** The hop material's static private key is redacted in
  `Debug` and zeroized on drop. The identity bundle's key is taken before
  the bundle is moved into the SSU2 service and is never logged.
- **No new authority.** The owner holds the same narrow outbound delivery
  capability the SSU2 service already had. It gains no listener, no inbound
  path of its own, and no authority over other services.
- **The creator correlation is a safety property, not a privilege.** It makes
  the router *decline* to treat its own traffic as transit work.
- **The scanner change is a strengthening.** More daemon production code is
  now inside the boundary rules than before. It revealed **no** production
  violation: the one raw occurrence of a forbidden pattern outside a
  comment is inside a `#[cfg(test)]` region, which the scoped strip removes
  correctly.
- **No dependency change**, so no new transitive surface, no new `unsafe`
  exposure and no licence question.

## Documentation and operations

- `docs/architecture/overview.md` — the composition's place in the daemon's
  composition, and the M11 capability snapshot row, which now states that
  the role is reachable under an explicit operator opt-in and **still not
  advertised**, and that the build's *delivery* to a peer is still unproven.
- `docs/architecture/tooling.md` — the strengthened rule, the scoped strip,
  the controls and the fail-open that was closed.
- `docs/architecture/i2pr-daemon.md` — the new module's row, the module
  list and the exported types.
- **Operator-facing:** nothing new to configure. The opt-in already exists.
  A router that enables it now relays and its three Proposal-170 transit
  selectors report real values instead of zeros.

## Limitations

Stated plainly, because the temptation to over-claim is exactly what this
plan exists to prevent.

1. **The build's delivery to a peer is still unproven.** The evidence shows
   the composition-installed owner **admits** a peer's transit work and
   attempts the forward; the attempt reports `NoActiveSession`, because the
   private mesh has no established session to the responder. The share is
   charged and the bytes are accounted; the cell does not reach a peer.
   **Plan 492 F1 is unchanged.**
2. **No stock-peer evidence.** Nothing here was run against i2pd or a stock
   router. Plan 492 F3 and Plan 436 WP5 stay open and blocked on the
   authorized two-host topology.
3. **No LOCAL OBEP consumer.** The daemon has no production consumer for a
   LOCAL delivery action, so a LOCAL action refuses with the typed
   `TransitLiveError::NoLocalConsumer`. That is fail-closed and honest; this
   plan did not invent a consumer.
4. **No NTCP2 ingress.** `handle_inbound` takes an `Ssu2InboundI2np`.
   NTCP2-originated traffic is not routed into the owner.
5. **The creator correlation's facts are the coordinator's *pending* and
   *retained* sets.** A router that owns no build coordinator owns no
   coordinator traffic and the correlation stays empty.
6. **The scanner is a text-level approximation.** It understands
   `#[cfg(test)]` items, not `cfg(not(test))`, macros or `include!`d files.
   It is validated by a brace-balance check over 70/70 files and by the
   two strip controls.
7. **The rule set binds to the agreed seam names**
   (`transit_composition`, `dispatch_transit_inbound`, `build_transit_owner`).
   A different shape fails with a precise message naming what is expected.

## Findings by severity

### Medium

**M1 — The creator-correlation sets grow without bound over the router's
lifetime.** The owner's `creator_builds`, `creator_data` and
`creator_gateways` are `BTreeSet`s with only `insert`; the production
wiring calls `install_transit_creator_correlation` on **every** pump tick,
so each distinct build the router performs leaves one entry behind forever.
The code says so plainly (*"the number of facts is bounded by the
coordinator's pending-build and tunnel-retention ceilings times the number
of builds the router performs"*), which is not a bound over time.

Why this is Medium and not High: the sets are **pre-existing** structures of
`TransitLiveOwner` — this plan adds a *producer* for them, not the
structure — and the error direction is safe. A stale fact makes the owner
**decline** to treat traffic as transit work; it can never make the owner
claim the router's own traffic as transit work. Only an opted-in router is
affected, and the growth is slow (one entry per build, ≈40 bytes).

The fix is a retention window, not a cap: a fact should be dropped only
after the coordinator could no longer accept a reply for it, which means
deriving the window from the coordinator's attempt timeout. That is a
protocol decision, not a mechanical one, and it is not made here.

### Low

**L1 — The scanner's raw-string handling was wrong on the first pass.**
`news.rs` uses an `r#"…"#` raw string; the first masker mis-scanned it,
swallowed a `struct` line, unbalanced the brace walk and mis-detected the
test region. Found by the brace-balance validation, fixed with proper
raw-string handling (`r"…"`, `r#"…"#`, `br##"…"##`), and the scoped strip
now yields brace-balanced output for 70/70 files where the blunt strip
unbalanced 6.

**L2 — The legacy rule's first assertion is satisfiable by a substring.**
`transit_owner` is a substring of `build_transit_owner`, so
`lib-owner-module` can be satisfied without naming the module. It was kept
verbatim rather than tightened, because tightening it to a word boundary
risks a false failure if the finished `lib.rs` names the module only in a
comment.

**L3 — `lib-owner-construction` binds the install to `lib.rs`.** It is
grounded in the plan's WP2 (§6 places the install in
`register_ssu2_service`), but it is the one assertion that would fail if
the construction call were moved into another module. Its message names
exactly what is expected.

### Recorded, not defects

- The participation posture's **share is charged before the forward**, so
  `bytes_relayed` advances even when the delivery then fails. This is the
  pre-existing accounting order and it is the reason the evidence asserts a
  delivery outcome rather than a byte count.
- The **LOCAL OBEP consumer does not exist**; a LOCAL action refuses with
  `NoLocalConsumer`. Fail-closed, by design.

## Plan 492's F2 disposition

**F2 is discharged.** Plan 492's finding said: *"a router that has never
been given a transit owner does not relay. Owner: the plan that installs
the transit owner in the product composition."* The owner is now installed
in the product composition, behind the existing default-off opt-in, and the
production inbound path dispatches through it. The finding's own owner line
is satisfied by this plan.

Plan 492's **F1** (a private mesh cannot complete the build's *delivery*)
and **F3** (no stock-peer evidence) stay open. Plan 492's closure record is
not rewritten.

## Roadmap disposition

`plans/subsystems/network-participation-roadmap.md`'s §7 milestone row for
502 moves from `registered` to `closed`, with this record as its closure.
The §12 status summary gains the same disposition.

Plan 436's row in `plans/registry.md` is **not** moved: Plan 436 stays
`blocked`. The registry's Plan 492 row gains a note that its F2 is
discharged here, because the registry is the live index and a finding's
disposition belongs there; the closure record itself is untouched.

## Unblock audit

Audited `plans/registry.md`'s blocked work and the network-participation
roadmap's dependency graph for anything that lists the discharged work as a
dependency.

| Plan | Was | Now | Why |
| --- | --- | --- | --- |
| 433 — live NetDB/tunnel/client composition | blocked | **blocked** | Blocked on Plans 431/432. Unchanged. |
| 435 — two-family public qualification | blocked | **blocked** | External environments this host does not have. Its source-side work packages were already discharged by Plans 456/461/496. Unchanged. |
| 436 — normal-daemon transit participation opt-in | blocked | **blocked** | Its **source-side WP3** is discharged here, but WP5 (the stock-peer qualification) and its Plan 433 ordering dependency are untouched. A blocked plan does not become ready because one of its work packages landed. |
| 437 / 438 / 439 | blocked | **blocked** | External environments. Unchanged. |
| 493 — two-family public participation qualification | blocked | **blocked** | Blocked on the authorized two-host topology and Plan 435. Unchanged. |
| 494 — floodfill role and the `caps=f` advertisement | blocked | **blocked** | Blocked on Plan 493. Unchanged. |

**Nothing moved to `ready`.** Plan 502 closes a *composition* gap and
discharges one recorded finding; no registered plan was blocked on it.

No corrective plan is required by this closure. M1 is recorded rather than
fixed, because the fix is a protocol decision about the retention window
rather than a mechanical bound, and the current behaviour's error direction
is the safe one.
