# Plan 251 status — Java source-lock test environment gating and ordinary-CI corrective

Status: **passed-java-source-lock-test-environment-gating-and-ordinary-ci-corrective**

Plan of record:
[`plans/implementation/mixed-router-interop/251-java-source-lock-test-environment-gating-and-ordinary-ci-corrective.md`](../../implementation/mixed-router-interop/251-java-source-lock-test-environment-gating-and-ordinary-ci-corrective.md)

Implementation SHA: `6cf441d0f4d52ed62e7a54363f4a1969d29539a5`
Date: 2026-09-24

## Outcome

The eight tests that read exact-pinned Java I2P source are individually ignore-gated.
Each uses `I2PR_M6_JAVA_SOURCE_ROOT` through one test-only helper that verifies the
checkout's Git revision before reading source. The explicit runner validates the checkout,
pin, and all source inputs, then invokes only the eight named tests with `--ignored --exact`.
It does not invoke `streaming_through_java`. Ordinary CI supplies no Java source root.

The final review found and corrected two source readers that had retained guessed `target/`
paths after the initial gate. The explicit lane was rerun after both were routed through the
common helper; all eight source assertions pass unchanged.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Inventory all exact-source readers | `java_tunnel_external.rs`: `p246_default_ack_delay_is_500_on_frozen_helper`, `p246_packet_handler_sets_deadline_before_event`, `p246_received_reschedule_calls_connection_timer`, `p246_transition_add_event_uses_fresh_wrapper`, `p246_timer_wrapper_delegates_to_connection_event`, `p246_connection_event_reenters_scheduler_chooser`, `p246_scheduler_precedence_is_source_locked`, `p247_no_ack_delay_override` | These eight tests require the pinned Java tree. Self-contained Rust parser/classifier tests and live topology tests retain their existing ordinary or explicit environment semantics. |
| Ordinary execution has no source dependency | Focused ordinary test binary | 410 passed, 13 ignored; source-root tests skipped. |
| Exact pin and original assertions retained | Explicit runner against Java I2P `9134f808337b401e8e53c73734c81fab04280c9d` | All eight exact tests passed. |
| Missing source root fails before tests | Runner with `I2PR_M6_JAVA_SOURCE_ROOT` unset | Exit 2; required-root diagnostic. |
| Wrong pin fails before assertions | Runner with the i2pr repository as source root | Exit 2; expected/found revision diagnostic. |
| Test gate, runner list, pin, and assertions statically guarded | `scripts/check-java-source-lock-gating.sh` | Passed; per-test ignore markers and all eight runner entries checked; live Streaming test excluded. |
| Ordinary workspace suite and quality floor pass on implementation SHA | GitHub Actions [run 36050553519](https://github.com/dbowm91/i2pr/actions/runs/36050553519), exact head `6cf441d0f4d52ed62e7a54363f4a1969d29539a5` | Quality (Ubuntu), Quality (macOS), MSRV (Ubuntu), and Dependency policy all succeeded. Workspace tests passed on both operating systems. |
| Production, Java source, dependency, and pin invariants preserved | Final diff review | Test/CI/planning only; no production changes, Java edits, dependency changes, or pin changes. |

## Verification record

Local after the final source-path corrections:

```text
cargo fmt --all --check                                       passed
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1
                                                               410 passed, 13 ignored
I2PR_M6_JAVA_SOURCE_ROOT=<exact checkout> scripts/run-java-source-lock-tests.sh
                                                               8 passed, each with --ignored --exact
runner with missing env                                       rejected (exit 2)
runner with wrong source revision                             rejected (exit 2)
bash scripts/check-java-source-lock-gating.sh                  passed
git diff --check                                               passed
```

The full local serial workspace floor, Clippy, docs, doctests, deny, dependency direction,
runtime boundaries, and existing M6 evidence checker passed on the immediately preceding
implementation commit `c4485d1`. The later `6cf441d` source-only test helper correction was
covered by the full current-SHA Ubuntu/macOS workflow above, including their workspace
test and quality steps.

GitHub Actions run `36050553519` completed successfully on
`6cf441d0f4d52ed62e7a54363f4a1969d29539a5`: both Quality jobs, MSRV, and Dependency policy
are green. No Java source checkout was injected into ordinary CI.

## Compatibility, security, and operations

- Java I2P remains pinned at `9134f808337b401e8e53c73734c81fab04280c9d`.
- No production/runtime/wire code, Java source or jar, dependency, or lockfile changed.
- No guessed sibling or `target/` source fallback remains in the eight source-lock tests.
- Exact source execution is explicit and fail-closed. The live Java Streaming lane is not
  part of the source-lock runner.
- M6 progression and Java compatibility interpretations are unchanged.

## Findings and limitations

- Critical/high/medium findings: none.
- Low findings: none.
- The live Java Streaming qualification remains deferred under its existing plan authority.

## Unblock audit and roadmap disposition

Plan 251 is closed. Plan 250 is ready and is the next executable plan in the requested
sequence. No registered blocked plan lists Plan 251 as its sole remaining prerequisite.
Plan 252 is still unregistered; its Plan 250 hard dependency remains open, so it cannot be
unblocked yet. Ordinary CI is green on the Plan 251 implementation SHA. M11 capability
remains unclaimed.
