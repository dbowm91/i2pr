# Plan 424 status: stopped — SessionConfirmed read closed, cause uncorrelated

Closure token: `stopped-session-confirmed-read-closed-unattributed-to-helper`.

Plan: `plans/implementation/ntcp2-transport/424-reverse-responder-io-operation-attribution.md`.
Sanitized evidence: `plans/closure/ntcp2-transport/424-evidence.json`.

The runtime now preserves a closed responder I/O operation alongside the
existing bounded `ExactIoError.kind`. At `await_confirmed`, completed write
counters distinguish the SessionCreated write from the SessionConfirmed read.
The non-production launcher maps each operation and `IoErrorKind` combination
to a distinct fixed terminal status, and the evidence projection admits only
those fixed codes. Protocol behavior, wire bytes, cancellation, deadlines,
normal-daemon NTCP2 policy, and support claims are unchanged.

The one reverse-only attempt reached TCP and terminated with
`responder_session_confirmed_read_closed`. The pinned i2pd helper exited 66
with `control-dialing-session-not-established`. Its sanitized observer found
no SessionRequest or SessionConfirmed milestones, I2NP block, or DeliveryStatus.
Cleanup passed. The responder operation is now identified, but the helper
result has no shared session token and cannot be attributed to that read
failure. No safe i2pr-owned protocol correction is localized, and no further
attempt is authorized by this plan.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Preserve responder I/O operation in bounded runtime error context | Passed; closed enum derived from completed counters for `await_confirmed`. |
| Preserve exact bounded `IoErrorKind` | Passed; existing `ExactIoError.kind` is retained. |
| Distinct operation/kind terminal statuses | Passed; exhaustive no-process test covers both operations and all four kinds. |
| Closed evidence projection and mutation checks | Passed; source checker verifies mapping pairs and rejects missing mappings/codes. |
| Protocol, part-two, and identity classifications unchanged | Passed; focused interop suite and full workspace suite passed. |
| Pinned helper build | Passed against pristine i2pd `635b013a612ff47278ef02acf8580a28e10e26c5`. |
| Exactly one reverse-only attempt | Passed; evidence records one attempt, reached TCP, then responder SessionConfirmed read closed. |
| Helper/session correlation | Not established; helper reported session-not-established without a shared token. |
| Plan 434 acceptance | Not met; no authenticated reverse link or correlated I2NP DeliveryStatus. |

## Commands and outcomes

- `cargo fmt --all --check` — passed.
- Focused runtime and interop suites — passed (115 runtime tests passed, 1
  ignored; 32 interop tests passed).
- Full AGENTS.md routine floor — passed: workspace tests 4,766 passed / 38
  ignored across 186 suites; Clippy, docs, doc tests, planning tests, boundary
  checks, evidence checkers, and `cargo deny check advisories bans sources`
  passed.
- `python3 scripts/check-current-pin-ntcp2-runner.py` and `--self-test` — passed.
- `python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --self-test`
  and `observe_decoded_delivery_status.py --self-test` — passed.
- `cargo build --locked -p i2pr-interop` — passed.
- `tools/i2pr-interop/reference/i2pd-current/build.sh` — passed against the
  pristine pinned source checkout.
- Reverse runner preflight initially failed before creating an attempt because
  its work parent did not exist. The work parent was created and the runner was
  invoked once more; the evidence file records exactly one reverse attempt.
  No reference attempt occurred during the failed preflight.
- `git diff --check` — passed before closure updates.

## Findings and unblock audit

- **Medium — reverse session remains unestablished.** The responder's terminal
  operation is specifically a closed SessionConfirmed read. The pinned helper
  reports session-not-established, but no common correlation identifies these
  as one session or one cause. Do not infer a cryptographic defect or modify
  wire behavior from this result.
- Plan 434 remains blocked until authenticated forward and reverse NTCP2 links
  and correlated I2NP DeliveryStatus evidence exist. Plans 433 and 435–439
  remain blocked on their registered predecessors. Plan 431 remains stopped at
  the unavailable authorized independent non-loopback topology. No other
  dependency-ready work in the 430–439 line was exposed by this diagnostic.
- Normal-daemon NTCP2 remains disabled and non-advertised. No support or
  anonymity claim changed.
