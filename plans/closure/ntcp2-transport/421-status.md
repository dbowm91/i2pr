# Plan 421 status: stopped — reverse scenario rejected before wire

Closure token: stopped-reverse-launcher-identity-role-mismatch.

Plan: plans/implementation/ntcp2-transport/421-current-pin-reverse-initiator-attempt.md.
Implementation commit: 57a10df — add reverse-only NTCP2 runner selection.

Plan 421 added a fail-closed runner selector for forward-only, reverse-only,
and the existing sequential-both mode. Its self-test verifies each selected
direction and rejects an invalid selector; the source checker mutation suite
rejects a reverse mapping that selects forward.

The one reverse-only run did not start the helper and sent no wire traffic.
The i2pr listener reached listener_ready, then the launcher rejected the
scenario with sender_router_identity_mismatch (exit 2). The runner recorded
process-exited-before-ready, attempted=false, and cleanup passed. Source
inspection localizes the error to the reverse scenario's expected identity
mapping: scenario_text uses its local-hash argument as the expected I2NP
sender, and the i2pr responder correctly requires that identity to match its
own prepared RouterInfo. The runner supplied the i2pd hash there in reverse
mode. No authentication or protocol result is inferred.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Reverse-only selector | Passed; no forward attempt was selected or executed. |
| Source/helper build and focused checks | Passed; pinned helper rebuilt and focused Rust, parser, runner, checker, vector, planning, and inventory checks passed. |
| Reverse wire attempt | Not reached; scenario validation rejected the i2pr sender identity before helper startup. |
| Cleanup and sanitized evidence | Passed; no helper events or raw logs retained; sanitized record is 421-evidence.json. |
| Plan 434 acceptance | Not met; no reverse NTCP2 or DeliveryStatus evidence. |

## Commands and outcomes

- Runner/observer self-tests and checker plus checker self-test — passed.
- cargo fmt --all --check — passed.
- cargo test --locked -p i2pr-interop --all-targets — passed, 32 tests.
- cargo build --locked -p i2pr-interop — passed.
- Pristine pinned i2pd helper rebuild — passed.
- NTCP2 vectors and historical boundary, tooling inventory, plan uniqueness, 51 planning tests, and git diff --check — passed.
- Reverse-only invocation with --direction reverse — exited 2 before helper start; see 421-evidence.json.
- Full workspace routine floor — not run; non-production runner tooling and planning files only.

## Findings and unblock audit

- **Medium — reverse setup uses the wrong scenario sender identity.** This is a localized runner defect, not a protocol result. Plan 422 owns the direction-to-identity correction, no-process regression, and one reverse-only reattempt.
- Plan 434 remains blocked on two-way authenticated I2NP evidence. The forward attempt remains unsuccessful. Plans 433 and 435–439 remain blocked by their listed dependencies. No other work line became dependency-ready.
- Normal-daemon NTCP2 remains disabled and non-advertised.
