# Plan 482 status: passed — Destination dial-authority policy guard

Closure token: `passed-destination-dial-authority-guard-with-stock-outbound-build-boundary`

Plan: [`482-destination-build-direct-dial-authority-and-linkability.md`](../../implementation/core-router-recovery/482-destination-build-direct-dial-authority-and-linkability.md)

## Outcome

Destination build setup now carries a private, typed dial authority. An inbound
gateway is an encrypted delivery target behind the outbound tunnel; it cannot
authorize a direct dial. Inbound provisioning and replacement require a usable
outbound role and select that route's actual first hop. Outbound bootstrap
retains an explicit first-hop exception; follow-on and replacement outbound
builds use the existing outbound route's first hop. Startup and replenishment
no longer eagerly establish a direct session to the selected inbound gateway.
An attempted target must match the authority before the dial callback runs.

The deterministic coverage includes policy mapping for inbound route miss,
bootstrap, replacement, and first-hop/gateway overlap; a dial-cause recorder
observes the actual authorized peer and purpose; and a mismatch test proves the
dial callback is not invoked. Authenticated inbound session admission and
return handling were not broadened or rejected by this policy.

This closes Plan 482's dial-authority guard. It does not establish anonymity,
public reachability, or SAM application traffic. Plan 478 still owns genuine
LS2 owner generation and bidirectional external SAM STREAM acceptance; Plans
477 and 478 retain the unresolved stock terminal-reply and product gates.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Inventory and route authority | `service_product.rs` now derives direct-dial authorization from build direction and the actual outbound first hop. Inbound gateway identity never supplies the authority. |
| Startup and replacement policy | Initial provisioning no longer eagerly dials the inbound gateway. Replacements and replenishment resolve the usable outbound role first and use its first hop; absence of that route fails closed. |
| Explicit bootstrap exception | Outbound router bootstrap has a distinct `RouterBootstrapFirstHop` purpose. Subsequent outbound builds use `OutboundTunnelFirstHop`. |
| Dial-cause regression | Six focused Plan 482 tests cover route/purpose selection and the dispatch boundary. The fixture records the peer and purpose passed to the actual dial callback; a route-authority mismatch returns before callback invocation. |
| Transport and boundary coverage | The routine runtime and service-tunnel boundary checks pass. The policy remains private to daemon composition and does not alter wire formats or global router background routing. |
| Bounded pinned product handoff | One exact-pinned i2pd 2.61.0 Plan-458 attempt after the policy change stopped at `outbound-build`. Sanitized receipt: `outbound:all-emitted=1`; stock aggregate: 96 ShortTunnelBuild records, 47 transit-create events, 4 peers. It produced no correlated terminal reply and did not reach the inbound stage. |

## Commands and outcomes

Executed locally on Linux; no hosted-CI result is claimed.

```text
rtk cargo fmt --all --check
rtk cargo check --locked --workspace --all-targets
rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
rtk cargo test --locked -p i2pr-daemon --lib plan482_destination_dial_authority_tests -- --test-threads=1
  passed (6 tests)
rtk cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
  passed (4 tests)
rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1
  passed (9 tests)
rtk cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
  passed (11 tests)
rtk bash scripts/check-runtime-boundaries.sh
rtk bash scripts/check-service-tunnel-boundaries.sh
  both passed
rtk python3 scripts/check-global-plan-number-uniqueness.py
rtk python3 scripts/check-core-router-recovery-contract.py
rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'
  passed (51 tests)
rtk git diff --check
  all passed
```

Before the final dial-callback fixture extraction, the serial workspace test
suite passed (4,872 passed, 39 ignored across 187 suites), along with the full
routine static/evidence floor documented in the Plan 481 closure. The focused
Plan 482 suite was rerun after the extraction and passed all six tests. The
final check, Clippy, focused product tests, and boundary results passed after
the extraction. No dependency was added; support inventory and
conformance declarations are unchanged.

## Compatibility, security, and limitations

The policy changes only which peer a Destination operation may directly dial.
It keeps direct outbound first-hop establishment needed by the outbound route,
fails closed when an inbound build has no usable outbound route, and leaves
independent router background sessions and authenticated inbound replies
outside the Destination dial decision. Overlap between an inbound gateway and
the outbound first hop is attributed to the independently selected outbound
route and is not treated as proof of acceptance. Diagnostics expose only a
purpose category, without stable peer identifiers.

The stock attempt failed before inbound construction, so it did not externally
exercise the IBGW policy or stock return behavior. No stock acceptance,
remote delivery, anonymity, public reachability, or SAM capability is claimed.
No support or advertisement posture changed.

Implementation commit: `3bb3dd8` (`fix(core-router-recovery): constrain destination dial authority`);
dial-cause fixture commit: `3e4a75f` (`test(core-router-recovery): record authorized destination dial attempts`).
No dependency was added.

## Unblock audit and disposition

Plan 482's source-owned dial policy is available to active Plan 478, but it
does not close Plan 478's route, LS2, or SAM product acceptance. Plan 477 still
owns strict terminal reply attribution and consumes Plan 481's write-completion
interface. Plans 479 and 480 remain independent work owners. No support
promotion follows from this closure.
