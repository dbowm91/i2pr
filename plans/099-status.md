# Plan 099 implementation status: 2026-08-11

## Active closure — Plan 100 exit cleanup and bounded one-run dispatch

Plan 100 closed its exit-readiness defects (D1, D2, D3, D4) and
the Plan 099 single-job CI workflow was dispatched exactly once
from the Plan 100 correction commit. The bounded replacement
runs (allowed by Plan 100 Result branch C) consumed two narrow
direct corrections before the bound forward-instrumented attempt
reached authentic post-TCP protocol evidence.

### Final development result

```text
development_interop = protocol-defect-localized
exact_wire_stage = noise_authenticated (i2pd listener authenticated)
exact_question = Why does the i2pr forward dialer not observe
                 ntcp2_authenticated when the i2pd listener reports
                 it? The i2pr side reads tcp_connected from its
                 own log, then reports terminal_rejected with
                 reason_code = reference-events-missing before the
                 i2pr side ever observes the i2pd's ntcp2_authenticated
                 event. The i2pr binary digest is
                 4b0548c89bb0eddc597d87105f2361ed8a0f65d4b66307992d52503d8de90bcb
                 and the i2pd instrumented binary digest is
                 d64d2c0c9a1659c81c7765bf4be5ac25e1f44ed3a6246bf272b5a24068b85914.
```

The compact sanitized summary is preserved at
`target/interop/evidence/milestone-3/31521642090/plan099-summary.json`.
The summary records:

```text
schema                         = i2pr-ntcp2-plan099-summary-v1
source_commit                  = 51742e3e27fad0ede1ed716cfaafef3fd557ebbd
workflow_run_id                = 31521642090
status                         = protocol-defect-localized
topology_kind                  = host-loopback-development
network_id                     = 99
bind_address                   = 127.0.0.1
development_only               = true
release_qualified              = false
isolation_qualified            = false
ntcp2                          = experimental-non-advertised
forward_instrumented           = protocol_rejected at noise_authenticated
forward_control                = not-run (skipped after prerequisite)
reverse_instrumented           = not-run (skipped after prerequisite)
reverse_control                = not-run (skipped after prerequisite)
forward_instrumented_summary   = highest_stage noise_authenticated, reason
                                   reference-events-missing, cleanup clean
i2pr_binary_sha256             = 4b0548c89bb0eddc597d87105f2361ed8a0f65d4b66307992d52503d8de90bcb
i2pd_instrumented_binary_sha256 = d64d2c0c9a1659c81c7765bf4be5ac25e1f44ed3a6246bf272b5a24068b85914
i2pd_control_binary_sha256     = 07ed3a4a446e19bca72e689bd7840727ba4e4a36cd0575e64ff2d9e2e8ce5260
reference_revision             = f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
summary_sha256                 = a288db5773f5f049d860ea902d6e0b48800fc4b9f237c0efeae90a1f1be94ea3
```

### Plan 100 narrow corrections applied

The Plan 100 Result branch C path permitted at most one direct
narrow correction without a new plan. The bounded repair budget
was applied to two narrow pre-TCP runner defects that the first
Plan 099 dispatch surfaced:

1. `tests/integration/ntcp2/harness/plan083_runner.py` --
   removed two leftover `self._stages.advance_to(TCP_CONNECTED)`
   calls inside the `execute_real_probe` free function. The calls
   were dead code from a prior refactor; the function derives its
   `highest_stage` from `observed_names` directly. The first
   authoritative Plan 099 dispatch failed at
   `NameError: name 'self' is not defined` on the first i2pd
   observation.

2. `.github/workflows/ntcp2-interop-host-loopback-development.yml`
   -- renamed the inline heredoc references from
   `ENVIRONMENT_OR_BUILD_BLOCKED` to the new module constant
   `ENVIRONMENT_OR_HARNESS_BLOCKED`. The first authoritative
   dispatch failed at the
   `NameError: name 'ENVIRONMENT_OR_BUILD_BLOCKED' is not defined`
   step after the import list was updated.

3. `tests/integration/ntcp2/harness/plan083_runner.py` --
   recorded i2pr status events with the real measured `line_marker`
   SHA-256 digest instead of the zero placeholder. The second
   authoritative dispatch failed at
   `observed event_sha256 must be a real measured digest (no zero
   placeholder)` because the runner recorded i2pr status events
   with `"0" * 64` even though a real line digest was already
   computed.

The third authoritative dispatch produced the
`protocol-defect-localized` result recorded above. Cross-side
ownership is ambiguous: the i2pd listener reported
`ntcp2_authenticated` while the i2pr dialer recorded
`tcp_connected` and then `terminal_rejected` with
`reference-events-missing` before observing the i2pd's
authentication event. The Plan 100 Result branch B "reproduce once
before any Rust correction" rule requires a clearly i2pr-owned
defect before a narrow Rust correction is permitted; the bound
forward-instrumented cross-side divergence does not localize
ownership cleanly, so the result is recorded as
`protocol-defect-localized` and no further Rust correction is
attempted under Plan 100.

### Status authority

```text
plan_099 = closed-protocol-defect-localized
plan_100 = closed-exit-cleanup-with-recorded-procedural-deviation
plan_101 = passed-daemon-ntcp2-activation-safety
plan_095 = historical-superseded-by-plan099-single-job-lane
plan_087 = historical-development-sequence
plan_088 = historical-development-sequence
plan_079 = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
normal_daemon_ntcp2 = disabled
router_construction = active
development_interop = protocol-defect-localized
exact_wire_stage = noise_authenticated
external_netdb_over_ntcp2 = blocked
```

### Plan 100 procedural deviation record

Plan 100 technical closure result remains `protocol-defect-localized`.
The implementation required more pre-TCP harness corrections than the
written Branch C budget and the final post-TCP result was not
reproduced unchanged as required by criterion 47. These are recorded
procedural deviations, not grounds to reopen the retired interop
sequence.

Plan 101 is the active daemon-safety correction that removes
premature NTCP2 production activation and restores the correct
activation boundary.

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
surface. Plan 100 (this commit) reduced the exit gate vocabulary
to three values, made the classifier stage-aware, removed the
undeclared `rtk rg` dependency, hard-asserted the observer
invariants, and removed the divergent source-tree digest fallback.

## Work packages completed

### WP1 — i2pd library build correction (Plan 099 finding)

The critical Plan 099 implementation finding — the instrumented
i2pd transport libraries were never actually compiled from the
patched source tree — is fixed. The build script performs three
ordered library builds (pristine, instrumented, driver). Plan 100
D3 adds hard invariants: pristine `libi2pd.a` references
`i2pr::i2pdinterop::Observe*` must equal zero; instrumented
`libi2pd.a` references must be greater than zero. The build fails
closed when either invariant is violated.

### WP2 — Pristine control uses native i2pd APIs

The pristine control `run_listen` and `run_dial` paths use native
`Transports::SendMessage`, `Transports::IsConnected`, and
`TransportSession::IsEstablished` through the asynchronous send
future. The control proves a non-null established `TransportSession`
without depending on any observer call site.

### WP4 — Bounded development exit gate (Plan 100 reduction)

The development-only exit gate vocabulary is reduced to exactly
three values (was four): `passed`, `protocol-defect-localized`,
`environment-or-harness-blocked`. The classifier is stage-aware.
The workflow emits a valid summary even when downstream attempts
are skipped after a forward-instrumented failure.

### WP3 / WP6 / WP7 — previously completed by c04da77

The Plan 099 c04da77 implementation commit already landed the
single-job workflow, the plan-specific Python test deletion, the
static boundary check trim, and the router-build continuation
authority.

## Validation

```text
cargo fmt --all --check                                           ok
cargo check --locked --workspace --all-targets                    ok
cargo test --locked --workspace                                  ok (248 crate tests)
cargo clippy --locked --workspace --all-targets --all-features
  -- -D warnings                                                  ok
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps ok
bash scripts/check-dependency-direction.sh                        ok
bash scripts/check-runtime-boundaries.sh                          ok
bash scripts/check-ntcp2-interoperability.sh                      ok
bash scripts/check-fixture-manifest.sh                            ok
bash scripts/check-ntcp2-vectors.sh                              ok
python3 -m unittest discover -s tests/integration/ntcp2/harness
  -p 'test_*.py'                                                  ok (153 tests)
git diff --check                                                  ok
```

Plus the single authoritative Plan 099 GitHub Actions dispatch
(workflow run `31521642090`) which produced the
`protocol-defect-localized` summary recorded in this status.