# Plan 423 status: stopped — reverse SessionConfirmed read closed

Closure token: stopped-reverse-session-confirmed-read-closed.

Plan: plans/implementation/ntcp2-transport/423-session-confirmed-part1-error-classification.md.
Evidence: plans/closure/ntcp2-transport/423-evidence.json.

The responder maps the seven existing bounded SessionConfirmed Part 1
`HandshakeError` variants to distinct fixed status reasons. Responder I/O is
reported separately as `responder_session_confirmed_part1_io_failed`. The
evidence projector accepts only the closed responder reason-code allowlist;
it does not preserve OS error text or dynamic status detail. Exhaustive
no-process mapping tests and checker mutations cover the new categories.

The one reverse-only attempt reached TCP and terminated with
`responder_session_confirmed_part1_io_failed`. The pinned helper exited 66
with `control-dialing-session-not-established`; its sanitized stock-log
observer found no SessionRequest or SessionConfirmed milestones, no I2NP
block, and no DeliveryStatus. Cleanup passed. The attempt budget is spent.
This identifies the responder's bounded read-closed result, but does not
localize why the pinned peer closed before establishing the session. No
cryptographic defect or i2pr-owned protocol correction is inferred.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Closed classification for Part 1 HandshakeError variants | Passed; seven protocol variants map to distinct fixed reasons. |
| Separate bounded I/O category | Passed; I/O maps to `responder_session_confirmed_part1_io_failed`. |
| Secret-safe evidence projection | Passed; only allowlisted responder reasons survive projection. |
| Exhaustive no-process tests and source-check mutations | Passed. |
| One reverse-only attempt | Reached TCP; responder read closed before SessionConfirmed; helper session was not established. |
| Plan 434 acceptance | Not met; no authenticated reverse link or I2NP DeliveryStatus. |

## Commands and outcomes

- `cargo fmt --all --check` — passed.
- `cargo test --locked -p i2pr-interop --all-targets` — passed, 32 tests.
- `python3 tools/i2pr-interop/reference/i2pd-current/run_plan414.py --self-test` — passed.
- `python3 tools/i2pr-interop/reference/i2pd-current/observe_decoded_delivery_status.py --self-test` — passed.
- `python3 scripts/check-current-pin-ntcp2-runner.py` and `--self-test` — passed.
- `cargo build --locked -p i2pr-interop` — passed.
- Rebuilt the helper using `tools/i2pr-interop/reference/i2pd-current/build.sh` against the pristine pinned i2pd source — passed.
- NTCP2 vector manifest, historical interoperability boundary, tooling inventory, global plan-number uniqueness, 51 planning tests, and `git diff --check` — passed.
- Reverse-only invocation with `--direction reverse` — exited 2; sanitized outcome is recorded in `423-evidence.json`.
- Full workspace routine floor — not run; no production source changed.

## Findings and unblock audit

- **Medium — pinned peer closes before establishing the reverse session.** The
  responder now exposes the read-closed outcome, but the helper's bounded
  terminal result and zero handshake-stage counters do not reveal why it
  closed. No safe i2pr-owned correction is localized. Plan 434 remains blocked
  until authenticated forward and reverse link evidence includes correlated
  I2NP DeliveryStatus. Plans 433 and 435–439 remain blocked on their registered
  prerequisites. Plan 431 remains stopped at the unavailable independent
  non-loopback topology.
- Normal-daemon NTCP2 remains disabled and non-advertised. No support claim
  changed.
