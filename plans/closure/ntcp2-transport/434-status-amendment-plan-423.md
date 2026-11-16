# Plan 434 status amendment: reverse runner exercised, authenticated acceptance still blocked

Current disposition token: `blocked-reverse-attempt-did-not-establish-authenticated-link`.

This status amendment updates the blocker recorded in
[`434-status.md`](434-status.md) using the later corrective work and evidence
from Plans 421–423. The original blocked record remains unchanged as the
historical account of why Plan 434 could not proceed at that time.

Plan: `plans/implementation/ntcp2-transport/434-ntcp2-authenticated-link-discrepancy-recovery.md`.
Related implementation commits: `57a10df3`, `1ebf95a0`, `ea53ec27`.
Latest reverse evidence: `plans/closure/ntcp2-transport/423-evidence.json`.

## Updated disposition

The Plan 434 blocker that no current-pin reverse initiator runner existed is
discharged. Plans 421–423 added reverse-only selection, corrected the
scenario's direction-independent I2NP sender/receiver identities, and
exercised one reverse-only attempt using the pinned i2pd 2.61.0 helper on
network ID 2 over loopback.

That attempt reached TCP. The i2pr responder reported the bounded
`responder_session_confirmed_part1_io_failed` result; the helper reported
`control-dialing-session-not-established`. The sanitized observer recorded no
SessionRequest or SessionConfirmed milestones, no I2NP block, and no
DeliveryStatus. Cleanup passed. These outcomes do not establish a correlated
peer-side cause or an authenticated NTCP2 session.

Plan 434 remains **blocked** because its acceptance requires genuine
authenticated NTCP2 in both directions and correlated I2NP delivery. The
reverse attempt did not meet that requirement, and the current evidence does
not localize an i2pr-owned protocol/runtime correction. The forward evidence
recorded by Plans 410–420 also lacks correlated I2NP DeliveryStatus. Normal
daemon NTCP2 stays disabled and non-advertised.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Current-pin reverse initiator execution surface | Passed; the bounded Plan 410 runner can select a reverse-only attempt against the pinned helper. |
| Direction-independent expected I2NP identities | Passed by Plan 422 tests; the local i2pr identity is sender and pinned i2pd identity is receiver in either connection role. |
| Reverse authenticated NTCP2 | Not met; responder read closed during SessionConfirmed Part 1 and helper session was not established. |
| Correlated reverse I2NP delivery | Not met; zero I2NP blocks and DeliveryStatus. |
| Forward authenticated I2NP delivery | Not met by the evidence in Plans 410–420. |
| Safe normal-daemon activation | Preserved; no production or configuration source changed, and Plan 435 remains blocked. |

## Verification carried by the successor records

Plan 422 records its focused interop tests, runner and observer self-tests,
source checker mutations, pinned helper rebuild, vectors, historical boundary,
planning and tooling gates in `422-status.md`. Plan 423 records the matching
focused tests and gates plus the reverse-only outcome in `423-status.md`.
No additional router attempt was run for this amendment.

## Findings and unblock audit

- **Medium — authenticated-link recovery remains unresolved.** The runner
  availability blocker is resolved, but the reverse authenticated link and
  correlated I2NP requirements remain unmet. Do not infer peer-close
  causality from the two terminal statuses; they contain no shared session
  correlation token.
- Plans 433 and 435–439 remain blocked by their registered prerequisites;
  Plan 431 remains stopped at the independent non-loopback topology gate.
- No other plan becomes dependency-ready from this status amendment. A new
  bounded corrective plan is required before another wire attempt or protocol
  change.
