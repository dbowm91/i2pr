# Plan 353 status — branch integration and planning-authority reconciliation

Status: **passed-plan349-branch-integration-and-planning-authority-reconciliation**.

Classification: integration hygiene. Plan 352's pure policy correction is included;
Plan 353 itself changes no product/runtime semantics or support claims.

## Implementation commits and integration baseline

- `cf54055` — removes the 409 tracked portable-consumer Cargo `target/` files
  while landing Plan 352's policy correction.
- `d1be21e` — pins the address-representation regression in a focused test.
- `53ee560` — closes Plan 352 and records its unblock audit.
- The branch was rebased onto `origin/main` at `bf257b2` before implementation.
  Final fetch confirmed `origin/main` remained the same baseline and
  `git rev-list --left-right --count origin/main...HEAD` returned `0 33`:
  zero commits behind current main, 33 commits ahead. The Plan-352 result and
  all Plan-353 corrections are present on this integrated branch.
- Plan 352's spec-first sequence is `3e6bffc` before implementation `cf54055`.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Remove generated output and prevent recurrence | `git ls-files '*/target/*'`; `.gitignore` | Before: 409 tracked paths under `tests/portable-service-tunnel-consumer/target/`. After: zero tracked target paths in the repository. `/tests/**/target/` ignores nested test target trees; `git check-ignore -v tests/portable-service-tunnel-consumer/target/check` identifies that exact rule. |
| Rebuild the consumer from source | `bash scripts/check-portable-service-tunnel-consumer.sh` | Clean temporary target directory, exact pinned public package revision, all 8 integration tests passed. Run twice after deleting the tracked target tree; both runs passed. |
| Keep the Plan-349 collision exact | `plans/README.md`, collision ledger, checker, `tests/planning/test_global_plan_number_uniqueness.py` | The qualified managed-runtime/349 + portable-service-tunnels/349 collision and historical 296/297 pairs are named consistently. Exact known collision passes; a third Plan-349 owner fails; generic unrelated duplicate-owner test fails. `python3 scripts/check-global-plan-number-uniqueness.py` passes. No closed plan number was changed. |
| Give the portable decision a unique ADR number | `git mv` from the former portable ADR path to `docs/adr/0033-portable-service-tunnel-policy-core-and-adapters.md`; repository-wide reference search | Portable service-tunnel decision is ADR 0033; managed-app ADR 0032 remains at its original path and identity. Architecture, crate docs, roadmaps, and portable closure references use ADR 0033. No current reference to the former filename or current prose identifying the portable decision as ADR 0032 remains. |
| Prove ADR ordering | freeze commits `3bcce31` and `1eb1a7c` | Managed-app ADR 0032 was frozen at `2026-10-05 13:16:07 UTC`; portable policy was frozen at `2026-10-05 14:49:06 UTC`. Their histories were parallel, so commit timestamps, not ancestry, establish the order. |
| Preserve the pre-existing ADR 0030 limitation | `docs/adr/0030-*` and `git diff origin/main -- docs/adr/0030-*` | Both accepted ADR 0030 files remain present and unchanged. Their pre-existing collision remains out of scope as Plan 353 specifies. |
| Integrate current main and retain Plan 352 result | final fetch, branch ancestry/count, Plan 352 closure | Branch is zero commits behind fetched `origin/main`; Plan 352 is closed and its downstream planning unblock is reflected in the roadmap and registry. |
| Preserve product and support boundaries | source diff and complete local routine floor | No Plan-353 runtime change, protocol advertisement, capability claim, or support inventory change. Plan 352 remains a pre-runtime policy contract correction only. |

## Verification

All results below are local Linux results. Hosted CI is not claimed.

Full routine floor on the integrated implementation tree:

```text
cargo fmt --all --check                                                     passed
cargo check --locked --workspace --all-targets                              passed (175 crates)
cargo test --locked --workspace --all-targets -- --test-threads=1          passed (4071 passed, 35 ignored, 148 suites)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       passed
cargo test --locked --workspace --doc                                       passed (20 suites)
bash scripts/check-dependency-direction.sh                                  passed
python3 scripts/check-global-plan-number-uniqueness.py                      passed
python3 scripts/check-portable-service-tunnel-api.py                        passed (678 declarations)
bash scripts/check-portable-service-tunnel-consumer.sh                      passed (8 tests, clean build)
python3 -m unittest discover -s tests/planning -p 'test_*.py'               passed (7 tests)
bash scripts/check-runtime-boundaries.sh                                    passed
bash scripts/check-service-tunnel-boundaries.sh                             passed
bash scripts/check-m11-per-epoch-composition.sh                             passed
bash scripts/check-service-anonymity-boundaries.sh                          passed
bash scripts/check-fixture-manifest.sh                                      passed
bash scripts/check-ntcp2-vectors.sh                                         passed
bash scripts/check-ssu2-vectors.sh                                          passed
bash scripts/check-i2cp-vectors.sh                                          passed (15 vector tests)
bash scripts/check-ntcp2-interoperability.sh                                passed
bash scripts/check-constrained-host-lane-boundary.sh                       passed
bash scripts/check-m11-transit-boundaries.sh                               passed
bash scripts/check-m11-transit-qualification-evidence.sh                   passed
bash scripts/check-sam-acceptance-evidence.sh                              passed
bash scripts/check-ssu2-acceptance-evidence.sh                             passed
bash scripts/check-i2cp-acceptance-evidence.sh                             passed
bash scripts/check-i2pcontrol-acceptance-evidence.sh                       passed
bash scripts/check-service-tunnel-acceptance-evidence.sh                   passed
bash scripts/check-exploratory-tunnel-evidence.sh                          passed
bash scripts/check-netdb-tunnel-evidence.sh                                passed
bash scripts/check-destination-tunnel-evidence.sh                          passed
bash scripts/check-streaming-tunnel-evidence.sh                            passed
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                  passed
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test      passed, including negative-control probes
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
  passed (18 tests)
cargo deny check advisories bans sources                                    passed
```

Plan 352 focused lane, additionally:

```text
cargo check --locked -p i2pr-app-proto --all-targets                         passed
cargo test --locked -p i2pr-app-proto --all-targets -- --test-threads=1    passed (15 tests)
cargo clippy --locked -p i2pr-app-proto --all-targets --all-features -- -D warnings
  passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-app-proto --no-deps passed
bash scripts/fuzz-smoke.sh                                                  passed
```

After closure metadata updates, format/diff checks, the global plan-number
checker, all 7 planning tests, service-tunnel boundary/API checks, target ignore
and count checks, and ADR listing were rerun and passed.

## Planning authority and unblock audit

The two Plan-349 implementation and closure authorities remain under their
original subsystem-qualified paths. The global-number checker retains exact
path-set exceptions only; no third Plan-349 owner is allowed. Plan 352 is
closed, and the managed-app roadmap/registry now permit bounded planning of the
router app-principal gateway and package/lifecycle + AppManager owner. Plan 353
is moved from active work to recently closed. No other blocked registry row
lists Plan 352 as a dependency. Proposal 170 adapter work remains gated on its
separate canonical Proposal-170 dependency. OS sandbox and runtime capabilities
remain future planned work and are not inferred from this closure.

## Security, compatibility, and known findings

This is repository integration hygiene plus the separately closed pure policy
correction in Plan 352. The managed-app v1 contract still has no in-tree runtime
consumer; its version remains 1.0 and no migration is required. There are no
dependency changes. The portable service-tunnel consumer still builds from its
public pinned package and source fixture after deleting generated output.

Findings by severity: critical none; high none; medium none; low none. Two
existing diagnostic classes did not fail their checks: the streaming-evidence
checker prints its guarded-label wiring warnings before its final passed result,
and `cargo deny` prints duplicate transitive lockfile-version warnings while
reporting advisories, bans, and sources passed. These are recorded observations,
not Plan-353 regressions. The two accepted ADR 0030 records remain a known
pre-existing identifier collision and were not changed.

