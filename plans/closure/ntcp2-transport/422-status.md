# Plan 422 status: stopped — reverse SessionConfirmed Part 1 failed

Closure token: stopped-reverse-session-confirmed-part1-stage-failure.

Plan: plans/implementation/ntcp2-transport/422-reverse-scenario-identity-correction.md.
Implementation commit: 1ebf95a — correct reverse NTCP2 scenario identities.

Plan 422 corrected the reverse scenario's expected I2NP identities. Both
forward and reverse now declare the local i2pr Router Hash as the expected
sender and the pinned i2pd Router Hash as the expected receiver, independent
of NTCP2 connection role. No-process tests cover both directions and reject
swapped or duplicate identities. The source checker mutation suite rejects a
swapped mapping.

The one reverse-only attempt reached TCP and the responder handshake, then
failed with the bounded status reason responder_session_confirmed_part1_failed.
The launcher exited 2; the pinned helper exited 66 with
control-dialing-session-not-established. Sanitized stock-log counters showed
no SessionRequest or SessionConfirmed milestones from i2pd, no I2NP block, and
no DeliveryStatus. Cleanup passed; reverse budget is spent. This reason groups
several bounded errors (including structure, deobfuscation, AEAD, transcript,
and key-agreement failures), so it does not yet identify the protocol
divergence. No success or specific cryptographic defect is inferred.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Direction-independent sender/receiver mapping | Passed; sender is local i2pr and receiver is i2pd in both roles. |
| Swapped/duplicate identity negative controls | Passed in the no-process runner self-test. |
| Reverse-only attempt | Reached TCP; failed at responder SessionConfirmed Part 1. |
| Sanitized evidence and cleanup | Passed; only fixed counts, codes, pin, and direction retained; cleanup passed. |
| Plan 434 acceptance | Not met; no authenticated reverse link or I2NP DeliveryStatus. |

## Commands and outcomes

- Runner and observer self-tests; current-pin checker and self-test — passed.
- cargo fmt --all --check — passed.
- cargo test --locked -p i2pr-interop --all-targets — passed, 32 tests.
- cargo build --locked -p i2pr-interop — passed.
- Pristine pinned i2pd helper rebuild — passed.
- NTCP2 vectors, historical boundary, tooling inventory, global plan uniqueness, 51 planning tests, and git diff --check — passed.
- Reverse-only invocation with --direction reverse — exited 2 after TCP; see 422-evidence.json.
- Full workspace routine floor — not run; no production source changed.

## Findings and unblock audit

- **Medium — reverse responder fails during SessionConfirmed Part 1.** The status category is too broad to distinguish the first exact cryptographic/structural error. Plan 423 owns a closed safe classification of the existing bounded HandshakeError variants and one reverse-only diagnostic attempt.
- Plan 434 remains blocked pending authenticated bidirectional NTCP2 and I2NP evidence. Plans 433 and 435–439 remain blocked on their recorded prerequisites. Plan 431 remains stopped at the independent topology boundary.
- Normal-daemon NTCP2 remains disabled and non-advertised.
