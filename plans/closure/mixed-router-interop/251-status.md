# Plan 251 status — Java source-lock test environment gating and ordinary-CI corrective

Status: **in-progress-java-source-lock-test-environment-gating-and-ordinary-ci-corrective**

Plan of record:
[`plans/implementation/mixed-router-interop/251-java-source-lock-test-environment-gating-and-ordinary-ci-corrective.md`](../../implementation/mixed-router-interop/251-java-source-lock-test-environment-gating-and-ordinary-ci-corrective.md)

Implementation commit: pending final source-root correction and fresh hosted CI.
Prior implementation commit: `c4485d1269be6e61bba315b5c512d1fe34f9381a`
Date: 2026-09-24

## Implementation outcome (closure pending)

The eight tests that read Java I2P source are individually ignore-gated. They now use
`I2PR_M6_JAVA_SOURCE_ROOT`, verify the exact Java I2P 2.13.0 commit before reading source,
and retain their original source assertions. Ordinary CI does not set the source-root
environment variable. `scripts/run-java-source-lock-tests.sh` validates the checkout,
pin, and all source inputs, then runs only the eight named tests with `--ignored --exact`.
It does not invoke `streaming_through_java`.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Inventory source-dependent tests | `p246_default_ack_delay_is_500_on_frozen_helper`, `p246_packet_handler_sets_deadline_before_event`, `p246_received_reschedule_calls_connection_timer`, `p246_transition_add_event_uses_fresh_wrapper`, `p246_timer_wrapper_delegates_to_connection_event`, `p246_connection_event_reenters_scheduler_chooser`, `p246_scheduler_precedence_is_source_locked`, `p247_no_ack_delay_override` | All eight are exact-pinned Java source locks; adjacent p246/p247 classifier/parser tests remain ordinary Rust tests. |
| Ordinary focused run skips only source locks | Focused `java_tunnel_external` run | 410 passed, 13 ignored; no missing-file failures. |
| Keep source assertions and exact source contract | `scripts/check-java-source-lock-gating.sh`; explicit exact-pin run | All eight explicit tests pass against `9134f808337b401e8e53c73734c81fab04280c9d`. |
| Missing environment fails closed | `bash scripts/run-java-source-lock-tests.sh` with `I2PR_M6_JAVA_SOURCE_ROOT` unset | Exit 2 before test execution. |
| Wrong source revision fails closed | Explicit runner with repository root as source root | Exit 2 with expected/found revision diagnostic, before source assertions. |
| Runner is narrow and exact | Static source-lock checker and runner invocation | Eight `--ignored --exact` invocations; no `streaming_through_java`. |
| Ordinary workspace test no longer depends on Java source | Full local serial workspace test | 2,915 passed, 26 ignored across 103 suites. |
| Quality floor and policy are green locally | Commands below | Passed. |
| Previous hosted CI | GitHub Actions run [36047569335](https://github.com/dbowm91/i2pr/actions/runs/36047569335), head `c4485d1269be6e61bba315b5c512d1fe34f9381a` | All four jobs succeeded, but a final p247 source-root routing omission was subsequently found. A fresh run is required. |
| Pin unchanged; no production or Java source change | Diff review; source-lock constants and checker | Java pin unchanged; only test infrastructure, CI guard, and planning records changed. |

## Commands and results

Local results below were collected before the final p247 source-root correction and must be rerun:

```text
cargo fmt --all --check                                      passed
cargo check --locked --workspace --all-targets               passed
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1
                                                              410 passed, 13 ignored
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                              2,915 passed, 26 ignored; 103 suites
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                              passed
RUSTDOCFLAGS=-Dwarnings cargo doc --locked --workspace --no-deps
                                                              passed
cargo test --locked --workspace --doc                          passed (0 doctests)
cargo deny check advisories bans sources                      passed
bash scripts/check-dependency-direction.sh                    passed
bash scripts/check-runtime-boundaries.sh                       passed
bash scripts/check-m6-mixed-router-acceptance-evidence.sh      passed
bash scripts/check-java-source-lock-gating.sh                 passed
```

Explicit lane against the exact-pinned local Java source checkout passed all eight named
tests. Missing-root and wrong-revision probes both failed before assertions as required.
The explicit lane is not part of ordinary CI.

## Compatibility, security, and operational review

- No Rust production behavior, protocol wire behavior, reference router source, or Java
  source changed.
- No dependency or lockfile change.
- The source root is explicit and revision-checked; there is no sibling/`target/` fallback.
- Ordinary CI remains independent of a Java source checkout. External-source tests remain
  available via a narrow fail-closed runner.
- The change does not reopen M6 Java Streaming qualification or change any M6 evidence
  interpretation.

## Findings and limitations

- Critical/high/medium findings: none.
- Low findings: none.
- The live `streaming_through_java` lane remains governed by its existing environment gate
  and is deliberately outside the source-lock runner.

## Unblock audit and roadmap disposition

Closure and the final unblock audit remain pending the corrected implementation's hosted
CI. Plan 250 remains ready but will start only after Plan 251 formally closes. Plan 252
remains unregistered because Plan 250 has not closed. M11 capability remains unclaimed.
