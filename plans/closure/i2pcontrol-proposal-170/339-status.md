# Plan 339 — Per-family network status, error, and testing owners disposition

Status: **passed-per-family-network-condition-owners-with-pinned-i2pd-enumeration**

Implementation commits: see the Plan 339 entries in the repository history; the
registration commit is `53ed79e` ("plans(i2pcontrol): register Plan 339
per-family network condition owners"). This plan reopened Plan 322 for five of
its eight unavailable selectors and closed them.

## What this plan did

Plan 322 closed blocked with eight unavailable canonical RouterInfo selectors.
Re-auditing the source showed those eight were **two different problems**:

- five per-family condition selectors that project state i2pr already
  maintains, and
- three transit selectors that require turning on production transit
  participation.

This plan took the first group only. Plan 322 remains blocked on the transit
trio, and its closure record carries a dated correction saying so.

The decisive input was the Proposal text itself. All five selectors are marked
*"(adopted from i2pd)"*, so the integer vocabulary was never missing — it is
i2pd's, pinned at `2c69414` `libi2pd/RouterContext.h:44-72`. Reading the pinned
Proposal bytes (SHA-256 re-verified in-repo, third independent read) rather than
inferring from a reference implementation is what made the emission policy
reviewable instead of a guess.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| `i2p.router.net.status.v6` has a truthful owner. | `proposal_per_family_network_condition_over_wire`; `effective_reachability_never_inherits_another_family`; `status_never_leaves_unknown_without_corroborated_evidence` | Passed. `0 OK` only from a fresh IPv6-qualified snapshot; `1 Firewalled` only from corroborated IPv6 unreachability; otherwise `2 Unknown`. |
| `i2p.router.net.error` / `.error.v6` have truthful owners. | `proposal_per_family_error_tracks_the_attested_netdb`; `error_code_is_offline_only_when_configured_and_unbound`; `an_empty_netdb_reports_no_descriptors_on_both_families`; `undetectable_error_codes_are_never_derived` | Passed. `5` on an attested empty NetDB, `2` only for a configured-but-unbound family, `0` otherwise. |
| `i2p.router.net.testing` / `.testing.v6` have truthful owners. | `testing_flag_is_one_only_while_a_determination_is_pending`; `an_untested_router_is_not_reporting_itself_as_testing`; `proposal_per_family_network_condition_over_wire` | Passed. `1` only while a determination is in progress; a never-observed router reports `0`. |
| The wire enumeration is the adopted one, not an invention. | `enumeration_matches_the_pinned_i2pd_definitions`; `adopted_enumerations_round_trip_exactly`; `out_of_range_wire_values_are_rejected_not_clamped` | Passed. Values outside `0..=5` are rejected, never clamped. |
| No unobserved condition is fabricated. | `status_never_leaves_unknown_without_corroborated_evidence`; `proxy_mesh_and_stan_are_never_derived`; `effective_reachability_rejects_stale_and_absent_snapshots`; `an_untested_router_is_not_reporting_itself_as_testing` | Passed. |
| The prior fail-closed behavior is preserved, and gating is per key. | `proposal_per_family_condition_fails_closed_without_a_transport_owner`; `proposal_per_family_error_fails_closed_without_an_attested_netdb`; `proposal_unavailable_sources_fail_closed_with_field_and_plan_over_wire` | Passed. No row infers a value from the requested selector. |
| The gap census is honest. | `plan322_source_matrix_covers_all_canonical_additions_and_marks_gaps` | Passed. 8 gaps -> 3, and the remaining three are asserted to be exactly the transit selectors. |
| Evidence rows have teeth. | Inverted the status mapping and removed the family match; measured failures; restored with an empty diff | Passed. 4/13 transport, 2/2 runtime, 2/4 wire rows failed. |

## Verification

All commands below are **local** runs on this branch, not CI truth.

```text
cargo test --locked -p i2pr-transport network_status                     13 passed
cargo test --locked -p i2pr-runtime --lib network_condition              2 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection           19 passed
cargo test --locked -p i2pr-i2pcontrol --test contract                    16 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_differential          1 passed, 1 ignored (provisioned external target)
cargo test --locked --workspace --all-targets -- --test-threads=1        3984 passed / 0 failed / 35 ignored / 147 suites
python3 -m unittest discover -s tests/planning -p 'test_*.py'             6 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness           18 passed
bash scripts/check-i2pcontrol-acceptance-evidence.sh                     ok
python3 scripts/check-global-plan-number-uniqueness.py                   passed
```

The workspace count is +19 rows and +0 suites against the `36f48ba` baseline
(3 965 -> 3 984, 147 suites), which is exactly the 13 transport + 2 runtime + 4
wire rows this plan added.

The rest of the `AGENTS.md` routine floor also passes at this head: `cargo fmt
--all --check`, `cargo check --locked --workspace --all-targets`, `cargo clippy
--locked --workspace --all-targets --all-features -- -D warnings`,
`RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`,
`cargo test --locked --workspace --doc`, `cargo deny check advisories bans
sources`, and the check-dependency-direction, runtime-boundaries,
service-tunnel-boundaries, constrained-host-lane-boundary, m11-transit-boundaries,
m11-transit-qualification-evidence, i2pcontrol/service-tunnel/exploratory-tunnel/
netdb-tunnel/destination-tunnel/streaming-tunnel/sam/ssu2/i2cp/
m6-mixed-router acceptance-evidence, ntcp2-interoperability,
m12-floodfill-qualification-evidence (self-test), fixture-manifest,
ntcp2-vectors, ssu2-vectors, and i2cp-vectors checkers.

One boundary failure was found and fixed rather than waived: the new module's doc
comment contained the literal text `async fn`, which
`scripts/check-runtime-boundaries.sh` greps for across the transport crates. The
comment was reworded; the script is unchanged and now passes.

Ordering note, stated so the evidence is not overstated: the 3 984-row serial run
was executed **before** two cosmetic edits — `cargo fmt --all` reformatting the
new test helpers, and the doc-comment reword above. Neither changes behavior, and
after both edits `cargo fmt --all --check`, `cargo check --locked --workspace
--all-targets`, `cargo clippy --locked --workspace --all-targets --all-features
-- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace
--no-deps`, `cargo test --locked --workspace --doc`, both boundary checkers, and
all four affected suites (13 / 2 / 19 / 16 rows) were re-run green. A full
serial re-run at the exact final commit was not repeated.

## Invariants, failure, and migration review

- **Boundaries.** `i2pr-transport` stays runtime-neutral: the new module has no
  I/O, no sockets, no Tokio, and no `async fn`. The socket, reachability, and
  composition state stays in `i2pr-runtime` and the daemon; the control
  projection stays in `i2pr-daemon`. `check-dependency-direction.sh` and
  `check-runtime-boundaries.sh` pass.
- **No unbounded structure.** The snapshot is a fixed-size `Copy` value with two
  booleans and one enum per family. No channel, queue, task, or timer was added.
- **Failure semantics.** A poisoned lock returns `None` and the row fails closed
  naming the field and Plan 339. An unregistered transport service fails all
  five rows closed. An unattested NetDB fails only the two `error` rows closed.
- **Secrets.** The new accessor exposes no address, port, endpoint, key, peer
  identity, or payload. `ServiceSockets` gained two booleans recording the
  *requested* families; it already held the bound addresses and still does, but
  they are never read through this accessor. No `Debug`/`Display` surface was
  added over secret material, and no secret is logged.
- **Compatibility / migration.** No wire change to any existing field, no
  persisted format change, no listener or route change, no configuration key. The
  three transit rows keep their existing fail-closed behavior.
- **Advertisement.** None. `advertised` stays false; no capability, version,
  RouterInfo, SAM, or I2CP behavior is advertised beyond the tested subset.

## Findings by severity

- Critical: 0; high: 0; medium: 0; low: 0.
- **Informational — a stale record corrected.** Plan 322's closure said the
  per-family rows needed "typed IPv4/IPv6 lifecycle/peer-test owners" and its
  implementation plan said to "add a bounded snapshot at the owning subsystem."
  Both understated the position: the state existed, and the Proposal had already
  deferred the vocabulary to i2pd. Corrected by a dated addendum; the original
  text is preserved.
- **Informational — a stale dependency named.** Plan 326's closure still names
  Plan 325 as its sole blocker although Plan 329 superseded 325's forward
  architecture and Plans 330/332/333 delivered the ELS2 and authorization scope
  under the clean-room path. Corrected by a dated addendum that re-points the
  re-audit; Plan 326's status token is deliberately **not** changed here.

## Limitations

- `ClockSkew` (1), `SymmetricNAT` (3), and `FullConeNAT` (4) are **never
  emitted**: i2pr owns no clock-skew or NAT-type detector, and
  `router.clockskew` is still a neutral constant, so claiming skew would be
  fabrication. The variants exist for wire completeness and round-tripping only.
- `Proxy` (3), `Mesh` (4), and `Stan` (5) are **never emitted**: i2pr has no such
  posture, and `Proxy` would also contradict the no-direct-clearnet rule.
- No row can currently report `0 OK` in ordinary production composition, because
  nothing in an ordinary profile establishes a qualified reachability snapshot.
  That is the honest result, not a missing wiring: the code path is exercised by
  the transport unit table and would answer `0` the moment a snapshot is
  qualified for the queried family.
- The three transit selectors remain gaps. This plan does not enable transit
  participation and does not touch M11 or the constrained-host lane.
- No interoperability is claimed for anything in this plan. The evidence is
  local and loopback-only.

## Docs

- `specs/references/proposal-170-network-status-error-testing.md` — normative
  vocabulary authority and emission policy (new).
- `specs/CONFORMANCE.md` — new "Per-family network condition codes (Plan 339)"
  section stating the honest baseline and the never-emitted codes.
- `plans/closure/i2pcontrol-proposal-170/322-status.md` — dated correction
  splitting the eight gaps into Group A (transit, still blocked) and Group B
  (closed here).
- `plans/closure/i2pcontrol-proposal-170/326-status.md` — dated correction to the
  named dependency.
- `plans/subsystems/i2pcontrol-proposal-170-roadmap.md` — Plan 339 in the
  sequence plus the Group A/B split rationale.
- `plans/registry.md` — Plan 339 registered, then closed passed.

## Unblock audit

Plan 339's hard dependencies (Plan 322 closed, Plans 320/321/323/324 passed) were
all satisfied at registration, so no plan was moved to `ready` by this closure
beyond the row it already owns.

Audited and **not** unblocked:

- **Plan 328** (full-conformance gate) lists Plans 322, 326, and 327 as hard
  dependencies. All three are still blocked or stopped, so it stays blocked.
- **Plan 322** remains blocked on the transit trio. Closing five of its eight
  rows does not satisfy it; its own record now says so explicitly.
- **Plan 326** requires a re-audit against Plans 330/332/333 before it can move;
  this closure does not move it.
- **Plan 327** is untouched. Its evidence-fixture decision is now recorded (a
  self-composed in-tree loopback outproxy fixture, labelled loopback and not
  interoperability) so the next executor does not re-derive it.

## Roadmap disposition

Plan 339 is **closed passed**. Plan 322's Group B is closed; its Group A (three
transit selectors) is the remaining Plan 322 work and is a production
transit-participation posture change, not a snapshot addition. The next
independent Proposal 170 gaps are unchanged in order: Plan 322 Group A, then
Plan 327, then the Plan 328 conformance gate. No M12/mainline readiness and no
full-Proposal conformance claim changes.
