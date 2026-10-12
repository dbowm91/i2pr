# Plan 465 status: blocked — build path passed; SAM summary predicate failed

Closure token: `blocked-sam-summary-incomplete-field-unlocalized`

Plan: [`465-sam-diagnostic-receipt-stream-correction.md`](../../implementation/core-router-recovery/465-sam-diagnostic-receipt-stream-correction.md)

## Outcome

The corrected runner reads daemon stdout and stderr, parses transition receipts,
and always writes a sanitized JSON result. Four offline parser tests cover a
complete attempt chain, admission without emission, fragmented-message ID
deduplication, and malformed/unattributed events.

The one corrected exact-pinned four-router run produced four selected
attempts, four bounded-delivery admissions, eight UDP datagrams carrying those
four I2NP message IDs, four authenticated reply matches, and four install
commits. The process then returned SAM `SESSION CREATE` failure
`destination-summary-incomplete`. The source reaches this failure after
`ensure_router_network_sam_destination_active` succeeds and the manager returns
a summary; the SAM owner rejects when any of these is false: inbound receive
count is nonzero, LS2 lease count is nonzero, and every receive ID has a
registered inbound owner.

The retained aggregate receipt proves that the original inbound-build timeout
is no longer the terminal result in this run. It does not expose which summary
predicate failed. No product correction is justified until that field is
identified. Plan 466 owns a bounded, identity-free field receipt and one
corrective run. The source trigger is proven through all four build attempts;
the external SAM capability remains blocked at the post-build summary gate.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Correct stdout/stderr parsing | Passed; sanitized diagnostic is retained in [`465-attempt.json`](465-attempt.json). |
| Attempt selected and admitted | Passed, four attempts. |
| Actual UDP emission correlated to attempt | Passed, four correlated attempts across eight datagrams. |
| Authenticated response matched | Passed, four attempts. |
| Pool install committed | Passed, four attempts. |
| SAM summary invariant | Failed at `destination-summary-incomplete`; the specific zero/missing field was not retained. |
| Bidirectional external STREAM exact-byte payload | Not reached. |
| Public SAM capability or support promotion | None. |

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk python3 tests/integration/core-router-recovery/test_plan464_attempt_receipt.py` | Passed, 4 tests. |
| `rtk python3 -m py_compile tests/integration/core-router-recovery/run_plan458_sam_product.py` | Passed. |
| `rtk python3 scripts/check-global-plan-number-uniqueness.py` | Passed before Plan 466 registration. |
| `rtk python3 scripts/check-adr-number-uniqueness.py` | Passed before Plan 466 registration. |
| `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | Passed before the single corrected daemon run. |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | One run; four build attempts passed through UDP emission, reply correlation, and install; SAM CREATE failed at `destination-summary-incomplete`. |

The runner removed all temporary raw logs, identities, and datadirs. Only
aggregate counts are committed.

## Security and compatibility

- The per-message receipt logs numeric correlation IDs only beside encrypted
  datagrams; retained evidence contains aggregate counts and no IDs.
- No stock source or binary was modified. The lane stayed loopback-only and
  used unique temporary datadirs.
- No SAM support or conformance claim is promoted.

Disposition: **blocked at the SAM post-build destination summary gate**. Plan
466 must identify the failed predicate before any product fix or further
acceptance claim.
