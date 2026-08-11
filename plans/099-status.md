# Plan 099 implementation status: 2026-08-11

## Active correction — Plan 100 exit cleanup and router handoff

Plan 099's pruning, one-job workflow conversion, and router-buildout
authority landed, but the final Phase F/G development-interop exit was
**not yet complete**. The post-implementation audit (Plan 100) found
deterministic exit-readiness defects in the compact summary
API/classifier, the i2pd observer-proof path, and the divergent
source-tree digest fallback. Plan 100 is the one-time active cleanup
authority for those defects and the final bounded run:

```text
plan_099 = implementation-landed-exit-cleanup-complete-pending-live-run
plan_100 = exit-ready-awaiting-one-manual-run
plan_095 = historical-superseded-by-plan099-single-job-lane
plan_087 = historical-development-sequence-superseded-by-plan100
plan_088 = historical-development-sequence-superseded-by-plan100
plan_079 = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
ntcp2 = experimental-non-advertised
normal_daemon_activation = disabled
router_construction = authorized-after-plan100-outcome
```

Plan 100 is recorded at
`plans/100-plan099-exit-gate-cleanup-and-router-handoff.md`. It
does **not** create a new interoperability architecture. It repairs
the Plan 099 exit-gate API/classification (D1, D2: reduce the result
vocabulary to three outcomes, make the classifier stage-aware),
hardens the existing instrumented/pristine i2pd proof (D3: remove the
undeclared `rtk rg` dependency and add hard observer-count
assertions), removes the divergent Python-vs-shell tracked-tree digest
fallback (D4: require Python 3 and reject the divergent encoding),
performs focused validation, and dispatches the existing one-job
Plan 099 workflow exactly once. A bounded replacement run is allowed
only under Plan 100's explicit pre-TCP/post-TCP failure policy.

### Plan 100 D1/D2 — exit gate vocabulary and classifier

The development-only exit gate vocabulary is reduced to exactly three
values:

```text
passed
protocol-defect-localized
environment-or-harness-blocked
```

- `passed` — all four per-attempt records carry
  `terminal_result = passed` and `cleanup_result = clean`.
- `protocol-defect-localized` — at least one executed primary
  direction reached `tcp_connected` (or any later wire stage) and
  then failed before the required correlated DeliveryStatus pass. A
  skipped downstream attempt cannot erase that classification.
- `environment-or-harness-blocked` — the earliest nonpassing path
  occurs before authentic TCP/protocol evidence: build failure,
  missing executable, reference startup failure, state preparation
  failure, workflow/API error, or loopback placement failure before
  TCP.

The classifier is implemented in
`tests/integration/ntcp2/harness/plan099_exit_gate.py` and is
covered by the focused functional test set
(`Plan099ExitGateTests` in `test_minimal_i2pd_probe.py`).

### Plan 100 D3 — i2pd observer proof

`tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`
now:

- uses plain `rg` from the workflow's installed `ripgrep` package
  instead of the undeclared `rtk rg` command;
- requires both `rg` and `nm` to be present, failing the build when
  either is missing;
- hard-asserts that the pristine archive carries exactly zero
  `i2pr::i2pdinterop::Observe` references and that the instrumented
  archive carries at least one; a violated invariant fails closed.

### Plan 100 D4 — source-tree digest simplification

The Python-vs-shell divergent tracked-tree digest fallback is
removed. `build-driver.sh` now requires Python 3 (already a
declared dependency of the development lane) and fails closed when
it is unavailable.

### Plan 100 focused checks

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
git diff --check
```

The full historical plan-specific Python matrix, rootless checker,
Multipass checker, and release-certificate validator are not
required for Plan 100 closure; they remain available via git history
for forensic archaeology.

Regardless of whether the final development result is `passed`,
`protocol-defect-localized`, or `environment-or-harness-blocked`,
production daemon composition and local/offline RouterInfo/NetDB
work are the next product-development line. NTCP2 remains disabled
and non-advertised; external NetDB-over-NTCP2 remains blocked
unless the independent two-way smoke passes.

The older status/prose below is retained as implementation history.
Any statement below that says Plan 099 is already fully passed,
Plan 095 is the next executable plan, or Plan 088 is the active
reverse-execution gate is superseded by this active correction.

## Implementation baseline and outcome

| Stage              | Tracked Python LOC | Tracked Rust LOC |
|--------------------|--------------------|-------------------|
| Pre-Plan 099       | 50,260             | 35,224            |
| Plan 099 c04da77   | 14,231             | 37,599            |
| Plan 099 e7cad5f   | 14,356             | 37,599            |
| Plan 100 (this commit) | 14,356 (unchanged)  | 37,599 (unchanged) |

The Plan 099 c04da77 implementation commit deleted ~36,000 lines of
historical plan-specific Python test and runner machinery. The
e7cad5f commit added the bounded WP1, WP2, and WP4 implementation
surface:

- 185-line `plan099_exit_gate.py` (untracked at this point; added to
  the harness by the same commit) defining the bounded development
  exit gate vocabulary and the compact summary builder/validator.
- 95-line `Plan099ExitGateTests` class in `test_minimal_i2pd_probe.py`
  exercising the four-value classifier and the build/validate
  round-trip.
- 50-line additions to `test_i2pd_direct_driver.py` covering the
  split-library CMake cache variables, the patched-tree library
  build path, and the object-level `nm -C` proof.

Plan 100 (this commit) reduces the vocabulary to three values and
makes the classifier stage-aware, removes the undeclared `rtk rg`
dependency, hard-asserts the observer invariants, and removes the
divergent source-tree digest fallback. The Python LOC count remains
14,356 because the focused functional tests in
`test_minimal_i2pd_probe.py` and `test_i2pd_direct_driver.py` are
added in place of the prior `Plan099ExitGateTests` block.

## Work packages completed

### WP1 — i2pd library build correction (Plan 099 finding)

The critical Plan 099 implementation finding — the instrumented
i2pd transport libraries were never actually compiled from the
patched source tree — is fixed. The build script now performs
three ordered library builds:

1. Pristine pinned archives (`libi2pd.a`, `libi2pdclient.a`,
   `libi2pdlang.a`) compiled from the untouched pinned tree.
2. Instrumented archives compiled from a private copy of the
   pinned tree with the observer patch applied; the patched
   `NTCP2.cpp` translation unit sees `-DI2PD_INTEROP_OBSERVER=1`
   during the `libi2pd` compile.
3. The driver CMake consumes the two archive sets via the explicit
   `I2PD_INSTRUMENTED_LIB_DIR` and `I2PD_PRISTINE_LIB_DIR` cache
   variables. The legacy single `I2PD_LIB_DIR` variable is
   rejected by a hard assertion in the CMake file.

The object-level proof via `nm -C` is emitted as
`object-level-proof.txt`. Plan 100 D3 adds hard invariants:

- pristine `libi2pd.a` references `i2pr::i2pdinterop::Observe*`
  must equal zero;
- instrumented `libi2pd.a` references must be greater than zero.

On this host the proof shows:

```text
pristine libi2pd.a: 0 references to i2pr::i2pdinterop::Observe*
instrumented libi2pd.a: 4 references to i2pr::i2pdinterop::Observe*
```

A live build of the i2pd driver confirms the instrumented binary
contains 28 observer references and the control binary contains
zero.

### WP2 — Pristine control uses native i2pd APIs

The pristine control `run_listen` and `run_dial` paths are no
longer gated on observer APIs the control build cannot emit.
They are compile-time branched behind `#ifdef I2PD_INTEROP_OBSERVER`:

- The instrumented path retains the bounded observer predicate
  waits (`WaitForTcpAccepted`, `WaitForAuthenticated`,
  `WaitForReceivedDeliveryStatusAfter`,
  `WaitForSentDeliveryStatusAfter`).
- The control path uses native `Transports::IsConnected`,
  `Transports::SendMessage`, and `TransportSession::IsEstablished`
  through the asynchronous send future returned by
  `SendMessage`.

The control proves a non-null established `TransportSession`
without depending on any observer call site.

### WP4 — Bounded development exit gate (Plan 100 reduction)

The development-only exit gate vocabulary is reduced to exactly
three values (was four):

```text
passed
protocol-defect-localized
environment-or-harness-blocked
```

Implemented in `tests/integration/ntcp2/harness/plan099_exit_gate.py`.
The classifier is stage-aware: a skipped downstream attempt cannot
mask a real post-TCP protocol defect; a forward post-TCP failure
with downstream attempts skipped classifies as
`protocol-defect-localized`, not as an environment/harness blocker.

The compact summary the workflow writes to disk carries the
canonical `summary_sha256` digest over the bounded fields only,
is validated on write, and rejects unknown status values.

The CI workflow delegates the compact summary through
`plan099_exit_gate.build_summary` and `validate_summary`.

### WP3 / WP6 / WP7 — previously completed by c04da77

The Plan 099 c04da77 implementation commit already landed:

- WP3.1 (single-job workflow, no cross-job artifact transfer);
- WP6.1–WP6.5 (deletion of historical plan-specific Python
  tests/runners, removal of `scripts/check-plan095-workflow.sh`
  and `scripts/check-ntcp2-loopback-smoke-boundary.sh`,
  trimming of `scripts/check-ntcp2-interoperability.sh` from
  1870 to 124 lines);
- WP7 (router-build continuation authority recorded in
  `README.md`, `AGENTS.md`, `docs/architecture/interop-apparatus.md`,
  `docs/protocol-support.md`, and the
  `.opencode/skills/i2pr-ntcp2-interop/SKILL.md`).

## Validation

```text
cargo fmt --all --check                                           ok
cargo check --locked --workspace --all-targets                    ok
cargo test --locked --workspace                                  ok (all crate tests passing)
bash scripts/check-dependency-direction.sh                        ok
bash scripts/check-runtime-boundaries.sh                          ok
bash scripts/check-ntcp2-interoperability.sh                      ok
bash scripts/check-fixture-manifest.sh                            ok
bash scripts/check-ntcp2-vectors.sh                              ok
python3 -m unittest discover -s tests/integration/ntcp2/harness
  -p 'test_*.py'                                                  ok (153 tests)
git diff --check                                                  ok
```

## Status authority

```text
plan_099 = implementation-landed-exit-cleanup-complete-pending-live-run
plan_100 = exit-ready-awaiting-one-manual-run
plan_095 = historical-superseded-by-plan099-single-job-lane
plan_087 = historical-development-sequence-superseded-by-plan100
plan_088 = historical-development-sequence-superseded-by-plan100
plan_079 = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
normal_daemon_activation = disabled
router_construction = authorized-after-plan100-outcome
```

Plan 100 (this commit) is the one-time active cleanup authority.
Exactly one manual Plan 099 GitHub Actions dispatch follows the
Plan 100 correction commit. The Plan 095 historical sequence is
fully superseded; Plan 087 and Plan 088 remain historical audit
records but no longer direct active execution. Plan 079 remains
deferred to the pre-normal-NTCP2-activation and pre-public-network
integration checkpoint. NTCP2 stays experimental and non-advertised.