# Plan 099 implementation status: 2026-08-11

## Active correction — Plan 100 exit cleanup

Plan 099's pruning, one-job workflow conversion, and router-buildout authority landed, but the final Phase F/G development-interop exit is **not yet complete**. The post-implementation audit found deterministic exit-readiness defects in the compact summary API/classifier and i2pd observer-proof path. Plan 100 is the one-time active cleanup authority for those defects and the final bounded run:

```text
plan_099 = implementation-landed-exit-run-pending
plan_100 = active-exit-gate-cleanup-and-router-handoff
plan_095 = historical-superseded-by-plan099-single-job-lane
plan_087 = historical-development-sequence
plan_088 = historical-development-sequence
plan_079 = deferred-to-pre-activation-checkpoint
ntcp2 = experimental-non-advertised
normal_daemon_activation = disabled
router_construction = authorized-after-plan100-outcome
```

Plan 100 is recorded at `plans/100-plan099-exit-gate-cleanup-and-router-handoff.md`. It does **not** create a new interoperability architecture. It repairs only the Plan 099 exit-gate API/classification, hardens the existing instrumented/pristine i2pd proof, removes the divergent digest fallback, performs focused validation, and dispatches the existing one-job Plan 099 workflow exactly once. A bounded replacement run is allowed only under Plan 100's explicit pre-TCP/post-TCP failure policy.

The older status/prose below is retained as implementation history. Any statement below that says Plan 099 is already fully passed, Plan 095 is the next executable plan, or Plan 088 is the active reverse-execution gate is superseded by this active correction.

Regardless of whether the final development result is `passed`, `protocol-defect-localized`, or `environment-or-harness-blocked`, production daemon composition and local/offline RouterInfo/NetDB work are the next product-development line. NTCP2 remains disabled and non-advertised; external NetDB-over-NTCP2 remains blocked unless the independent two-way smoke passes.

## Implementation baseline and outcome

| Stage              | Tracked Python LOC | Tracked Rust LOC |
|--------------------|--------------------|-------------------|
| Pre-Plan 099       | 50,260             | 35,224            |
| Plan 099 c04da77   | 14,231             | 37,599            |
| Plan 099 +WP1/WP2/WP4 (this commit) | 14,356 (+125 tracked)  | 37,599 (unchanged) |

The Plan 099 c04da77 implementation commit deleted ~36,000 lines of
historical plan-specific Python test and runner machinery. This commit
adds the bounded WP1, WP2, and WP4 implementation surface:

- 185-line `plan099_exit_gate.py` (untracked at this point; added to
  the harness by the same commit) defining the bounded development
  exit gate vocabulary and the compact summary builder/validator.
- 95-line `Plan099ExitGateTests` class in `test_minimal_i2pd_probe.py`
  exercising the four-value classifier and the build/validate
  round-trip.
- 50-line additions to `test_i2pd_direct_driver.py` covering the
  split-library CMake cache variables, the patched-tree library
  build path, and the object-level `nm -C` proof.

Net Python LOC change from this commit: +125 tracked lines (plus +185
untracked lines for the new file). The total Python LOC across the
full Plan 099 implementation (c04da77 + this commit) remains
strictly lower than the pre-Plan 099 baseline; the WP4 acceptance
criteria require a small schema/builder module that is not
duplicable in any existing file (it is a validator for a record
schema that did not exist before this commit). The WP1/WP2/witness
tests stay inside the existing artifact files.

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
`object-level-proof.txt`. On this host the proof shows:

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

### WP4 — Bounded development exit gate

The development-only exit gate vocabulary is exactly four values:

```text
two-way-development-smoke-passed
forward-wire-defect
reverse-wire-defect
environment-or-build-blocked
```

Implemented in `tests/integration/ntcp2/harness/plan099_exit_gate.py`.
The compact summary the workflow writes to disk carries the
canonical `summary_sha256` digest over the bounded fields only,
is validated on write, and rejects unknown status values.

The CI workflow now delegates the compact summary through
`plan099_exit_gate.build_summary` and `validate_summary`; the
inline heredoc Python that produced the same payload in earlier
plans is removed.

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
cargo clippy --locked --workspace --all-targets --all-features
  -- -D warnings                                                  ok
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps ok
bash scripts/check-dependency-direction.sh                        ok
bash scripts/check-runtime-boundaries.sh                          ok
bash scripts/check-ntcp2-interoperability.sh                      ok
bash scripts/check-fixture-manifest.sh                            ok
bash scripts/check-ntcp2-vectors.sh                              ok
python3 -m unittest discover -s tests/integration/ntcp2/harness
  -p 'test_*.py'                                                  ok (144 tests)
git diff --check                                                  ok
```

## Status authority

```text
plan_099 = passed-pruning-and-exit
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_098 = passed-runner-provenance-boundary-correction (historical)
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = deferred-to-pre-activation-checkpoint
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
normal_daemon_activation = disabled
router_construction = next
```

Plan 099 WP1/WP2/WP4 lands at this commit. Plan 095 (one manual
Ubuntu single-job two-way i2pd smoke) remains the single next
executable plan; Plan 088 remains blocked on Plan 095 CI closure
and the gated Plan 072 ambiguity-activation path remains
inactive. NTCP2 stays experimental and non-advertised.
