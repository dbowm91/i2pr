# Plan 484 — Bandwidth default burst vs NTCP2 wire-frame minimum: closure

**Status:** passed 2026-10-15. Closure token
`passed-burst-default-fits-ntcp2-frame-no-silent-clamp`. Branch (a) from the
WP1 decision record in the implementation plan: the daemon default is now the
smallest burst that fits one maximum NTCP2 data frame above the default
control reserve. No silent default change beyond the decided value, no lowered
wire minimum, no live lane.

## Commits

- Implementation + guards + operator doc: `crates/i2pr-daemon/src/config.rs`
  (default `64 * 1024` → `72_819` with the pinned derivation comment; two new
  config tests), `docs/operations/bandwidth-governor.md` (default value,
  derivation, reserve interaction, explicit-override spelling).
- Planning records: implementation plan WP1 decision, `plans/registry.md`
  row, `plans/subsystems/core-router-recovery-roadmap.md` §7 row, this file.

## Requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| Decision (a) vs (b) with arithmetic, operator impact, migration | WP1 record in the implementation plan: `ceil(65_537 * 100 / 90) = 72_819`; no working daemon affected (default-off + construction previously failed); no wire/peer/state migration; (b) rejected as a new error path for a never-working configuration; no ADR per the Plan 454 precedent |
| Default fits a maximum frame above the reserve | `bandwidth_default_burst_fits_ntcp2_frame_above_default_reserve`: parsed default is `72_819` at reserve 10, and a governor built from the parsed defaults constructs `Ntcp2RuntimeService::new_with_bandwidth` successfully |
| Default-drift guard (rejected-branch shape) | Same test asserts `72_819 - 1` is still rejected with `BandwidthBurstTooSmall { minimum: 72_819 }`; fails if the default silently drifts below the minimum again |
| Explicit-small-burst semantics guard | `bandwidth_explicit_small_burst_keeps_fail_closed_semantics`: explicit `65_536` parses to the exact value (no clamp) and is still rejected by the unchanged runtime gate; fails if a small burst is ever silently accepted |
| Inversion probes, mutants reverted | One-line mutant lowering the default to `65_536` fails the fits-test; one-line mutant flipping the runtime `<` to `>` fails the explicit-small test; both reverted and the tree re-verified green |
| Plan 454 baseline green unchanged | `shared_shaper_must_fit_a_maximum_data_frame_above_the_control_reserve` passes unmodified (default-sized 64 KiB rejected, 128 KiB accepted) |
| No support/capability/advertisement change | No RouterInfo, `specs/support.toml`, tier, or publication edit; bandwidth stays default-off |

## Commands run with outcomes (local; CI not yet run at closure)

- `cargo test --locked -p i2pr-daemon --lib config::tests::bandwidth -- --test-threads=1` — 4 passed (2 existing + 2 new).
- `cargo test --locked -p i2pr-runtime --lib ntcp2_runtime::tests::shared_shaper_must_fit_a_maximum_data_frame_above_the_control_reserve -- --exact --test-threads=1` — 1 passed, unchanged.
- `cargo fmt --all --check` — clean (after formatting the new tests).
- `cargo check --locked --workspace --all-targets` — clean.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — 4919 passed, 39 ignored, 188 suites.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — No issues.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` — clean; `cargo test --locked --workspace --doc` — 0 passed, 29 suites.
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'` — 51 tests OK; `python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `bash scripts/check-runtime-boundaries.sh`, `bash scripts/check-dependency-direction.sh` — passed.
- `cargo deny check advisories bans sources` — advisories red on the known external rustls family (`RUSTSEC-2026-0346/0347/0348/0349`-class finding recorded at the Plan 483 close, no dependency change in this plan); bans/sources ok.

## Invariant / failure / migration / security review

- Wire-frame minimum and control reserve untouched: the runtime gate, the 454 regression, and the transport constants are byte-identical; only the daemon default literal changed.
- No silent clamp introduced anywhere: explicit values keep exact semantics through `normalize_bandwidth` into the runtime gate.
- Construction-time decision only: no task, queue, retry, or restart path added; `[bandwidth]` reload (if any) flows through the same `normalize_bandwidth`.
- No new secret surfaces, channels, listeners, or advertisement.
- Migration: operator-facing default change only, documented in `docs/operations/bandwidth-governor.md` with the override spelling.

## Limitations

- A non-default `control_reserve_percent` above 10 can push the required minimum above the new default; that configuration remains fail-closed at construction (documented, not solved — per-transport tuning is out of scope).
- The runtime `BandwidthBurstTooSmall` diagnostic still names only the minimum, not the configured burst; enriching it would reshape the 454-pinned variant and belongs to a transport plan-of-record, not this policy pass.

## Roadmap disposition

Closed; the 479 finding is resolved and the 483 limitation is lifted. No live SSU2/NTCP2 capability is unblocked.

## Unblock audit

No registered plan lists 484 as a hard or interface dependency (registry-wide search 2026-10-15). Plans 437/438/433/435/436/439 keep their existing blockers unchanged, as predicted: the bandwidth floor is necessary, not sufficient, for the normal role. 478/480 live legs untouched. Nothing promoted to ready by this closure.
