# Plan 466 status: blocked — outbound build emitted, no correlated reply

Closure token: `blocked-sam-outbound-build-no-correlated-stock-reply`

Plan: [`466-sam-summary-postbuild-cause-and-correction.md`](../../implementation/core-router-recovery/466-sam-summary-postbuild-cause-and-correction.md)

## Outcome

The source now exposes the three summary values on the rejection path without
destination or peer identifiers. `RouterNetworkSummary::is_complete` is the
single predicate used by SAM, and its test covers all eight combinations of
nonzero inbound count, nonzero lease count, and registered inbound owner.
The corrected runner parses those values into a sanitized record.

The one Plan 466 exact-pinned run did not reach the summary branch. It ended
at `outbound-build`. The retained attempt receipt in
[`466-attempt.json`](466-attempt.json) shows three selected attempts, three
bounded-delivery admissions, and UDP emission correlated to all three
attempts. Two authenticated responses matched and two installs committed. The
third attempt had no authenticated reply match before the bounded outbound
build deadline. This is the first missing transition established by the
attempt-scoped source receipts: **after actual UDP emission and before
authenticated response correlation**. Whether pinned stock accepted the
request or emitted a response was not observed.

The earlier Plan 465 run reached SAM's `destination-summary-incomplete` check
after four successful install commits. Plan 466's current run instead failed
earlier at an outbound reply wait, showing the four-peer product lane is not
stable across the bounded attempts. The diagnostic does not prove a code
defect, so no source workaround or retry is justified. Plan 462 remains
blocked; its nonzero-hop two-direction SAM STREAM acceptance was not met.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Summary predicates exposed without identity material | Implemented; SAM records only inbound count, lease count, and owner-registration boolean. |
| Summary predicate truth table | Passed, all eight input combinations. |
| Selection/admission/emission receipt | Passed, 3 attempts. |
| Authenticated reply match and install | Passed for 2 attempts. |
| First missing attempt transition | Localized after UDP emission and before authenticated reply correlation on the third attempt. |
| Stock accepted the unmatched request or generated a reply | Not established. |
| Real inbound/outbound install plus signed LS2 and bidirectional external SAM STREAM | Not established as a complete session; SAM returned `outbound-build`. |
| Product fix | None made; no source cause was proven. |

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk python3 tests/integration/core-router-recovery/test_plan464_attempt_receipt.py` | Passed, 5 parser tests. |
| `rtk python3 scripts/check-global-plan-number-uniqueness.py` | Passed after Plan 466 registration. |
| `rtk cargo check --locked -p i2pr-daemon --all-targets` | Passed after the summary receipt change. |
| `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | Passed before the focused daemon test and external run. |
| `rtk cargo test --locked -p i2pr-daemon --lib sam::tests::router_network_summary_requires_each_postbuild_predicate -- --exact --test-threads=1` | Passed, 1 test (8 combinations). |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | One Plan 466 run; 3 admitted and emitted, 2 replied and installed, then outbound-build timeout. |

Raw logs, identities, peer endpoints, and datadirs were removed by the runner.
The committed receipt is aggregate-only.

## Security and compatibility

- No message payload, RouterInfo, destination identity, or peer address is
  included in the trace or evidence.
- No external router was modified; all endpoints remained loopback and all
  fixture datadirs were private and temporary.
- No SAM capability or conformance claim is promoted.

Disposition: **blocked at stock response after emitted outbound build**. No
further unchanged fixture run is authorized without a source-supported way to
observe or establish the stock acceptance boundary.
