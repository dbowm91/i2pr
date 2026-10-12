# Plan 465 — SAM diagnostic receipt stream correction

**State:** blocked P2. Closure: [`465-status.md`](../../closure/core-router-recovery/465-status.md). Owner: core-router-recovery / destination-product. The corrected single pinned run proves all four build attempts were selected, admitted, emitted, reply-matched, and installed, but SAM then failed `destination-summary-incomplete`. The summary gate combines three independent predicates and the retained receipt does not identify which was false. Plan 466 owns one field-specific summary diagnostic and a bounded corrective run.

## Objective

Read both daemon output streams, retain only aggregate attempt transitions, and determine the earliest missing event after accepted bounded delivery. The source receipts correlate an opaque `BuildAttemptId` to its I2NP message ID and the runtime's UDP emission by that message ID. No identities, peer hashes, destination keys, RouterInfos, or payloads may enter the committed evidence.

## Work packages

1. Harden the runner's sanitized parser for normal formatted output on stdout and stderr. Always write a versioned diagnostic JSON, including an explicit zero-attempt result. Test its positive and negative parsing cases with representative lines, including reordered fields, missing IDs, no emission, and multiple fragments sharing one message ID.
2. Confirm the exact i2pd source pin and binary version, build the required managed-app siblings, and execute the existing four-router diagnostic once with unique datadirs. The previous Plan 464 attempt is the only attempt on the old parser; this plan's one run is the only corrected capture attempt.
3. Report selected/submitted, bounded admission, datagram staging, actual UDP emission, authenticated reply match, and install commit as aggregate counts. Derive the first missing transition only when the receipt proves it. Do not identify peers or retain raw logs.
4. If the receipt proves an i2pr source or fixture defect, amend this plan with the exact narrow correction, add deterministic regression, then spend exactly one fresh pinned acceptance run. If no source defect is proved, stop at the classified external/topology blocker and retain SAM capability as blocked.

## Invariants

- Do not alter i2pd source or binary, invent a response, bypass authenticated SSU2, reduce the three-hop requirement, use a public network, or advertise SAM capability.
- No unchanged reruns after this diagnostic. The only permitted later run is the one after an exact proven source correction.
- Raw logs, identities, datadirs and peer addresses remain temporary and are deleted.

## Verification and closure

Before focused daemon tests, build sibling binaries per `AGENTS.md`. Run parser self-tests, `cargo test --locked -p i2pr-transport-ssu2 --test data_phase transmit_receipt_preserves_bounded_i2np_message_ids -- --exact --test-threads=1`, `cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1`, and the one pinned diagnostic. Close blocked if the actual UDP emission/reply transition remains absent or external; close passed only after a proven corrective regression and the complete Plan 462 acceptance matrix. Run the full `AGENTS.md` floor for production source changes.
