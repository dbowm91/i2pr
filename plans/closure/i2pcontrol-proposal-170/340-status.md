# Plan 340 — Transit volume, bandwidth, and share owners: closure record

Status: **passed-transit-volume-owners-with-participation-posture-unchanged**

Plan of record:
[`plans/implementation/i2pcontrol-proposal-170/340-transit-volume-and-share-owners.md`](../../implementation/i2pcontrol-proposal-170/340-transit-volume-and-share-owners.md)

Registration commit: `e683eb2`. Implementation and records: this commit.

Date: 2026-10-05.

## Outcome

Plan 340 closes Plan 322 Group A. The three remaining canonical Proposal 170
`RouterInfo` selectors — `i2p.router.net.total.transit.bytes`,
`i2p.router.net.bw.transit.15s`, and `i2p.router.net.tunnels.shareratio` — now
have bounded production owners, and the canonical gap census reaches **zero**.

**The transit participation posture is unchanged.** Production profiles still
never construct a transit data-plane owner, transit is still disabled,
non-advertised, and unclaimed, and no M11, SSU2, NetDB, or advertisement
behavior changed. The honest product baseline for all three selectors is
`0`, `0`, and `0.0`, and that is what a router that relays nothing truthfully
reports.

A separate, pre-existing defect was found and corrected in its own commit
(`b453270`): the M11 lane's zero-production-diff guard was fail open. It is
recorded in full in the 2026-10-05 correction in
[`plans/closure/transit-tunnels/265-status.md`](../../closure/transit-tunnels/265-status.md).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Three transit selectors have production owners and the gap census reaches zero | `crates/i2pr-i2pcontrol/src/source_matrix.rs`; `plan322_source_matrix_covers_all_canonical_additions_and_marks_gaps` | Pass. Census 3 → 0, asserted by test. |
| Volume is counted where the pinned reference counts it | `transit_compose::tests::transit_volume_advances_only_on_a_forward`; i2pd `TransitTunnel.cpp:43`, `TunnelBase.h:29` | Pass. 1028 bytes per cell, `Forward` only. |
| A disabled posture cannot report non-zero volume | `transit_volume::tests::disabled_participation_reports_no_volume_and_no_counters`; `TransitParticipation::Disabled` holds no counters | Pass, structural. |
| A participating router reports measured volume over the wire | `proposal_transit_volume_reports_a_participating_router_measured_volume` | Pass. |
| The 15-second figure is a real trailing window | `transit_volume::tests::window_*` (7 rows) | Pass. |
| The share ratio never publishes a guess | `transit_volume::tests::share_ratio_*`; `proposal_transit_share_ratio_requires_an_attested_denominator_over_wire` | Pass. Missing denominator is a gap. |
| An absent owner is a gap, never a zero | `proposal_unavailable_sources_fail_closed_over_wire` | Pass. |
| The participation posture did not change | `git diff` shows no change to transit construction, gates, M11, or advertisement surfaces | Pass. |
| Every evidence row has teeth | Five inversions, below | Pass. |

## Invariants

1. No `Disabled` owner can report non-zero volume — the counters do not exist in
   that state, so there is no code path from a missing observation to a zero.
2. Volume advances at exactly one site, the `Forward` arm of
   `TransitBuildService::route_tunnel_data`. Drops, replays, rejected builds,
   and OBEP deliveries leave the counters untouched.
3. A poisoned lock, an unpublished owner, or a missing denominator fails closed
   to an `InspectionGap` and never degrades to `0`.
4. The window ring is fixed-size (16 slots). No unbounded container, no new
   queue, no new task, and no new timer.
5. Writer and reader share one clock base (`transit_volume::wall_seconds()`).
6. No secret, key, payload, peer identity, endpoint, or address crosses the
   surface; the snapshot is three bounded numbers and a boolean.

## Failure, cancellation, migration, and security review

- **Failure.** A poisoned volume lock is skipped rather than propagated: the
  cell has already been transformed and must still be forwarded, so the cost of
  a poisoned counter is a diagnostic undercount, never a delivery failure. That
  trade is recorded in the code comment at the call site.
- **Cancellation / restart.** The counters are process-local and hold no durable
  state. A restart returns every volume selector to zero, which is the truth
  after a restart. No lease, token, or registration is affected, and the change
  touches no cancellation or drain path.
- **Migration.** None. No wire field, no option, no RouterInfo content, and no
  configuration key changed. Nothing to migrate and nothing to deprecate.
- **Security.** The new surface exposes three aggregates and one boolean. No
  secret, private key, payload, peer identity, endpoint, or address is reachable
  from it. Nothing is logged. The `Debug` output of the new types is counts and
  flags only. No new dependency, no `unsafe`, no new listener, and no change to
  the loopback-only binding posture.

## Two defects found in this plan's own code, before closure

Both were caught by the plan's own rows and fixed at the source rather than
papered over in a test.

1. **The window ring stored epochs modulo the ring size.** Two seconds one full
   ring period apart landed on the same slot with an equal stored residue, so the
   second record accumulated into the first and the window over-counted by every
   recycled period. `cumulative_total_is_independent_of_the_window` read 2604
   where it expected 1028. Fixed by storing the **absolute** second; the ring is
   now 16 slots of an absolute epoch, and the read stays lock-free.
2. **The window's writer and reader used different clocks.** The forward path
   inherited the transit ingress clock while the request path inherited the
   I2PControl service's monotonic `now_ms`, measured from service construction.
   The two lived in different epochs, so a live window could only ever read
   empty. Fixed by deriving both stamps from one `wall_seconds()` helper. This is
   why the wire tests stamp with the real clock rather than a fixed epoch.

## Teeth

Every new evidence row was verified to fail when its fix is disabled. All
sources were restored from backup afterwards and re-verified green.

| # | Inversion | Layer | Rows failed |
|---|---|---|---|
| T1 | projection ignores the published owner and hard-codes the disabled posture | wire | 1 of 4 |
| T2 | forward-path volume accounting removed | unit | 1 |
| T3 | share ratio falls back to `0.0` without an attested denominator | unit + wire | 1 + 1 |
| T4 | window ring reverts to modulo-16 epochs | unit | 8 |
| T5 | a transit source-matrix row reverts to `Unavailable` | contract | 1 |

The first teeth pass also exposed that the harness could silently skip an edit
whose anchor did not match; the harness was rewritten to report a non-applied
edit and T5 was re-run until it genuinely failed. A dangling evidence pointer was
found in the same pass — the contract invariant required an
`Unavailable` row to name `proposal_unavailable_sources_fail_closed_over_wire`,
a test name that no longer existed — and was repointed at a test that does exist
rather than left to rot.

## Findings by severity

- Critical: 0; high: 0; medium: 0; low: 1.
- **Low — pre-existing, not introduced here:** the contract invariant's required
  evidence-test name was dangling (above). Corrected in this pass.
- Out of scope, deliberately not fixed here, already noted in earlier planning:
  the legacy `RouterInfoSelector::ClockSkew` path returns a neutral constant in
  one code path rather than null, and `i2p.router.net.status` (v4 integer) is
  declared in `proposal_wire.rs` with no canonical source row or dispatch.

## Limitations

- No transit capability is claimed or advertised. `specs/support.toml` is
  unchanged.
- The three selectors report zero on a product router by design. A non-zero value
  requires the controlled lane, and no external qualification was run here.
- `tunnels.shareratio` remains a local definition of an unspecified key. A future
  reference implementation may define it differently; the definition and its
  caveat are recorded in the reference dossier so that is a visible decision
  rather than a silent divergence.
- The trailing-window rate is computed at request time, so it is not bit-identical
  to i2pd's one-hertz sample.
- Verification is local. No external or interoperability lane was executed, and
  none is claimed.

## Verification

Local, on this branch, at the closure state:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo test --locked -p i2pr-daemon --lib
cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection -- --test-threads=1
cargo test --locked -p i2pr-daemon --test m11_transit_data_plane --test m11_transit_live_owner
cargo test --locked -p i2pr-i2pcontrol --all-targets
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-m11-transit-boundaries.sh
bash scripts/check-m11-transit-qualification-evidence.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-i2pcontrol-acceptance-evidence.sh
python3 -m unittest discover -s tests/planning -p 'test_*.py'
python3 scripts/check-global-plan-number-uniqueness.py
```

Full serial workspace floor on this branch:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
-> 4 003 passed / 0 failed / 35 ignored across 147 suites
```

That is **+19 rows and +0 suites** against the 3 984 / 147 baseline recorded by
Plan 339: 15 unit rows in `transit_volume`, 1 forward-path row in
`transit_compose`, and 4 wire rows in `i2pcontrol_inspection` (one of which
replaced the now-obsolete three-gap test), with the Plan 322 census row updated
3 → 0.

Ordering note, stated so the evidence is not over-read: this floor ran at the
implementation state. The record set (this file, the reference dossier, the
CONFORMANCE section, the 322 amendment, the registry and roadmap rows) was
written afterwards and touches no `crates/*/src`, so no source row depends on
it; `cargo fmt --all --check`, workspace check, Clippy, rustdoc, doctests,
dependency/runtime/service-tunnel/constrained-host boundaries, the three M11
checkers, and all sixteen evidence checkers were re-run green after the records
were final. A full serial re-run at the exact closure commit was not repeated.

## Docs

- New: `specs/references/proposal-170-transit-volume-and-share.md` (normative).
- Updated: `specs/CONFORMANCE.md`, "Transit volume, bandwidth, and share
  (Plan 340)".
- Updated: `plans/closure/transit-tunnels/265-status.md` (dated correction),
  `268-status.md` (addendum), `266/267-status.md` (cross-references), and
  `265-retained-plan264-evidence.tsv` (dated note).
- Updated: `plans/registry.md`, `plans/subsystems/i2pcontrol-proposal-170-roadmap.md`.

## Unblock audit

Plan 340 was registered to close Plan 322 Group A. Per the audit:

- **Plan 322** — all five of its requirement rows are now satisfied: the selector
  inventory is complete, the implemented sources are truthful and bounded, the
  three transit selectors have production owners (this plan), the five per-family
  selectors have production owners (Plan 339), and no unavailable field is
  fabricated as zero. Its disposition changes from blocked to passed by dated
  amendment in `322-status.md`.
- **Plan 328** (full-conformance gate) lists 322, 326, and 327 as hard
  dependencies. 322 is now closed, but **326 and 327 remain blocked**, so 328
  correctly stays blocked and does not move to `ready`. Not unblocked.
- No other registered plan lists Plan 340 or Plan 322 as a hard or interface
  dependency. Nothing else moves.

## Roadmap disposition

Plan 340 closed `passed` on 2026-10-05, taking Plan 322 Group A's ownership half
and leaving the participation posture exactly where Plans 268/269 left it. Plan
322's remaining Plan 340 work is nil: the canonical Proposal 170 RouterInfo gap
census is zero. Group A's *posture* half — enabling production transit
participation — remains explicitly out of scope and now requires M11
re-qualification bound to the current tree.
