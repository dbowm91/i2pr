# Plan 471 status: blocked — SAM inbound owner registry empty after installed builds

Closure token: `blocked-sam-owner-registration-empty-after-inbound-builds`

Plan: [`471-normal-sam-stock-short-build-acceptance-and-multihop-recovery.md`](../../implementation/core-router-recovery/471-normal-sam-stock-short-build-acceptance-and-multihop-recovery.md)

## Outcome

Source review confirmed pinned i2pd 2.61.0's short-build transit handler
accepts only when transit is enabled, connectivity is not limited, congestion
permits, requested bandwidth fits, the next-hop path is not duplicated, and
transit registration succeeds. It emits no rejection code for these gates.
Its existing loopback-only read-only `/?page=transit_tunnels` page exposes the
current transit inventory without a new stock helper. The runner now reads only
the number of transit table rows and bounded log-event counts; raw HTML and raw
reference logs remain private scratch data.

The first instrumented product replay reached SAM's postbuild summary after
four build attempts were selected, admitted, emitted, reply-matched, and
installed. The genuine signed LS2 contained two leases and the router bridge
held two inbound receive IDs, but the shared manager's inbound-owner map
contained **zero** corresponding owner registrations. SAM correctly returned
`destination-summary-incomplete`. This identifies the first product gate after
the four installs: inbound owner registration is absent at summary time. The
source's `provision_all_service_router_material` path registers those IDs
before it returns, so the observed zero conflicts with the apparent successful
path and the cause is not yet established. Do not weaken the SAM predicate or
paper over the missing owner.

Other bounded replays remained variable: some stopped at inbound or outbound
build before the summary. On the completed receipt with stock-side inventory
sampling, 14 short-build records were observed and transit inventory rose by
14 across four peers with zero decrypt/local-next-hop/reply-send failures. A
later run saw much larger background stock exploratory activity; that total
cannot be attributed to the i2pr attempts. Therefore this is evidence that
stock peers accepted some requests, but it is not attempt-correlated proof that
the unmatched Plan 466 request was accepted or replied to. The runner records
this limitation explicitly.

Plan 471 is **blocked**. No code change is justified until the owner-registry
discrepancy is source-localized. The external multihop signed-LS2 and
bidirectional SAM STREAM was not reached. Neither the diagnostic transit page
nor aggregate stock activity is represented as full SAM capability evidence.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Exact pinned stock i2pd and isolated local topology | Passed for each executed lane; pin `635b013a612ff47278ef02acf8580a28e10e26c5`, version 2.61.0, loopback-only temporary routers. |
| Stock eligibility/acceptance evidence | Partial: read-only transit inventory and sanitized short-build events show transit activity, but background stock exploratory traffic prevents exact attribution for unmatched attempts. |
| Plan 466 third unmatched attempt | Not correlated to a specific stock acceptance/reply event. |
| Four i2pr build attempts | One run selected/admitted/emitted/matched/installed 4/4; other bounded runs stopped earlier at inbound/outbound build. |
| Postbuild summary | Failed: 2 inbound receive IDs, 2 real leases, 0 of 2 inbound owner registrations at summary time. |
| Owner registration causal correction | Not made; source trace has not found why the map is empty after the path that should register the IDs. |
| Nonzero-hop LS2 plus external bidirectional SAM STREAM | Not reached. |
| Default hop policy, identity and role boundaries | Preserved; no synthetic tunnel, zero-hop path, modified stock process, or public role. |
| Capability/support promotion | None. |

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk python3 tests/integration/core-router-recovery/test_plan464_attempt_receipt.py` | Passed, 6 parser tests including owner-install and eviction count receipts. |
| `rtk python3 tests/integration/core-router-recovery/test_plan471_stock_receipt.py` | Passed, 3 read-only transit-row count tests. |
| `rtk python3 -m py_compile tests/integration/core-router-recovery/run_plan458_sam_product.py` | Passed. |
| `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` | Passed before focused daemon qualification. |
| `rtk cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1` | Passed, 4 tests. |
| `rtk cargo fmt --all --check` | Passed after formatting. |
| `rtk python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | Five bounded attempts. One reached 4/4 installs then failed at summary with owner count 0/2; other attempts failed at inbound-build or outbound-build. No attempt completed the external STREAM matrix. |
| `rtk git diff --check` | Passed. |

## Invariant, security, and compatibility review

- Stock i2pd remained exact-pinned and unmodified. The new observation uses its
  existing read-only loopback HTTP page and private debug logs; it sends no
  control command and exports only counts.
- Router identities, tunnel IDs, peer paths, raw HTML, RouterInfos, and raw
  logs are not retained in evidence. Stock log reads are capped at the last
  2 MB per peer.
- The production summary gained an owner count solely to make the already
  required all-owner predicate diagnosable. `RouterNetworkSummary::is_complete`
  still requires a positive inbound count, positive lease count, and the
  all-receives-registered predicate.
- No tunnel hop default, network behavior, public bind, protocol support, or
  dependency changed. No migration impact.

## Unblock audit and disposition

Plans 462 and 464–466 remain historical blocked records; this result adds
current evidence without rewriting them. Plan 433/439 remain blocked on their
independent full-router gates. Plans 470, 472, and 473 do not depend on Plan
471. No successor is registered yet: the next useful work must explain how the
inbound receive IDs reach the bridge while the matching owner entries are
absent, then add a deterministic regression and one causal replay. Do not
repeat the four-peer run without that source-level correction.

Disposition: **blocked at the postbuild inbound owner registry predicate**.
