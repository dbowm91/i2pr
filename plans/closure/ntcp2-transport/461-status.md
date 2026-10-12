# Plan 461 status: passed at controlled loopback scope

Closure token: `passed-dual-family-request-driven-product-and-failure-continuity`

Plan: [`461-dual-transport-normal-process-i2np-continuity.md`](../../implementation/ntcp2-transport/461-dual-transport-normal-process-i2np-continuity.md)

## Outcome

Plan 461 depends on executed normal-daemon product receipt over authenticated
NTCP2 and a source-supported stock-to-i2pr trigger. Plan 459 implemented the
shared bounded ingress owner and exercised a responder-role RouterInfo effect,
but its normal-daemon initiator-role receipt and correlated live reply remain
unproven. Plan 460 traced the exact-pinned i2pd 2.61.0 send call graph and
found no eligible direct router-control target for the current controlled
non-floodfill/non-transit i2pr role; its useful tunnel send requires a live
transit consumer. Those gates remain authoritative in
[`459-status.md`](459-status.md) and [`460-status.md`](460-status.md).

Without eligible live product messages over both transports, Plan 461 cannot
demonstrate request-driven family selection, correlated responses, or either
single-family failure surviving through the other family. No dual-family
source correction or external qualification attempt was made. Plan 461 is
closed as blocked at its dependencies, not passed.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| One normal daemon product owner shared by NTCP2 and SSU2 | NTCP2 ingress wiring landed under Plan 459; both-family live product consumption not qualified. |
| Request-driven eligible authenticated transport chooser | Not implemented or tested. |
| Real I2NP receipt over NTCP2 and SSU2 with peer/family attribution | Not established; see Plans 459 and 460 status records. |
| Each single-family failure survives through the other transport | Not run because the positive dual-family product receipt gate is absent. |
| Wrong identity/network, stale route, cancellation, governor saturation and lifecycle matrix | Not run for the new dual-family path. Existing Plan 459 local negative and lifecycle results do not satisfy this matrix. |
| Public capability or support promotion | None. NTCP2 remains default-off and non-advertised. |

## Commands and outcomes

This dependency closure added no source changes and ran no new dual-family
qualification command. Evidence is inherited only as scoped by the immutable
predecessor records:

- Plan 459's focused and full workspace verification is recorded in
  [`459-status.md`](459-status.md).
- Plan 460's pinned source and trigger inspection is recorded in
  [`460-status.md`](460-status.md).
- The Plan 462 M6 preflight, tunnel, and NetDB controls are not dual-family
  normal-daemon product evidence.

No dependency, protocol, or configuration change was made.

## Security, compatibility, and findings

- Critical/high: none newly identified.
- Medium: normal-process dual-family routing and single-family failure
  continuity remain unproven.
- No stale RouterInfo route, false reachability, patched reference, public
  network, or additional socket owner was introduced.
- Existing non-advertised and default-off transport posture is unchanged.

## Unblock audit and disposition

Plan 461 can resume only when Plans 459 and 460 meet their live receipt gates.
Plan 462 is independent and remains blocked on its own first-divergence
evidence; it does not change Plan 461's dependency order. Plans 455, 457, 434,
and 435 keep their own historical and external qualification gates; this
closure does not rewrite them.

Disposition: **blocked on Plans 459 and 460 acceptance evidence**.

## Unblock amendment — 2026-10-12

The dependency blocker in this historical closure is cleared. Plan 459's
completion addendum records a live normal-daemon outbound NTCP2 DatabaseLookup
and same-generation correlated DatabaseSearchReply. Plan 460 passed at
controlled scope through Plan 469's source-owned fresh stock dial and product
reply. Those are the two stated Plan 461 prerequisites; the original blocked
assessment above remains intact as history.

Plan 461 is now **active**. No dual-family chooser or failure-continuity result
is inferred from the one-family Plan 459 proof. Remaining acceptance is exactly
the existing plan's request-driven eligible transport selection, actual
product receipt over NTCP2 and SSU2, then each isolated-family failure
surviving through the other family, including its bounded identity, lifecycle,
and resource-return matrix. Normal NTCP2 remains default-off and
non-advertised, and Plan 435's external/public qualification remains separate.

Disposition: **active; dependency audit passed; implementation and dual-family
acceptance remain open**.


## Execution progress — 2026-10-12

The common `RouterDeliveryService` now exposes the selected family and, for
NTCP2, the selected link generation. It uses an established SSU2 session first
and falls back to the newest live NTCP2 generation. The registry holds
concurrent authenticated generations by `(peer, link_id)` and removes only the
generation that closes. A local test with a real SSU2 session removes the
competing NTCP2 route and confirms SSU2 delivery continues; after stopping the
SSU2 owner, the surviving NTCP2 route accepts the next message. The NTCP2 route
in that chooser test is an admitted test queue; Plan 459 supplies the separate
normal-process live NTCP2 wire and correlated-reply proof.

The source-pinned stock fixture now has a bounded `--stock-ssu2-only` mode. It
retains the exact signed dual-address stock RouterInfos but starts publication
owners with only SSU2 active. One execution received four candidate publication
targets, confirmed three, and observed three normal floodfill DatabaseStores
accepted with product replies submitted; the route counters recorded six SSU2
stores and six selected SSU2 replies, with no NTCP2 links. This shows a real
source-owned request/reply path over SSU2 while the peer's NTCP2 listener is
disabled. The regular strict mode continues to show product replies over
NTCP2; its latest replay recorded ten NTCP2 floodfill replies and one
daemon-initiated DatabaseLookup/DatabaseSearchReply correlation.

The alternate SSU2-only run is not a Plan 469 selector-model pass: its
precomputed candidate selection was zero while live stock receipts selected
the candidate. The observed SSU2 product receipts are valid family evidence,
but no source-selector prediction is claimed for that mode. An attempt with
both stock transports active still selected NTCP2 for every measured product
reply, so the SSU2-only mode is retained to exercise actual SSU2 transport.

Focused evidence after these changes:

| Command | Outcome |
| --- | --- |
| `rtk cargo test --locked -p i2pr-daemon --lib router_i2np` | Passed, 26 tests, 1 ignored. |
| `rtk cargo test --locked -p i2pr-daemon --lib router_i2np::tests::router_delivery_survives_each_single_family_route_loss -- --exact --test-threads=1` | Passed; SSU2 survived NTCP2 route removal, then NTCP2 survived SSU2 owner shutdown. |
| `rtk cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight daemon_controlled_exchange_reaches_central_dispatcher -- --exact --test-threads=1` | Passed; real daemon-owned SSU2 session delivered one I2NP message. |
| `rtk python3 tests/integration/ntcp2/plan468_private_netdb.py` | Passed strict NTCP2 mode; sanitized live receipts above. |
| `rtk python3 tests/integration/ntcp2/plan468_private_netdb.py --stock-ssu2-only` | Passed SSU2 family row; three of four candidate-targeted publication attempts were confirmed, with product acceptance/reply over SSU2. The selector-model mismatch is retained above. |

Additional real-link continuity and request-driven owner — 2026-10-12:

Implementation commit: `257f1eb`.

`router_i2np::tests::router_delivery_uses_real_ntcp2_link_after_ssu2_owner_stops`
establishes real authenticated NTCP2 and SSU2 sessions between the same router
identities. It sends over SSU2, drops the NTCP2 socket and route while SSU2
continues, reauthenticates a replacement NTCP2 generation, then shuts down the
SSU2 owner and sends over that real NTCP2 link. The peer decodes the I2NP on
each family. The test passed.

`RouterDeliveryService` now places a peer-only request on a bounded queue after
both established-family paths report `NoActiveSession`. The NTCP2 owner wakes
for that request, obtains a fresh `validated_router_info_snapshot`, resolves
only the requested peer from the current eligible records, and applies the
existing signature/network/hash checks, loopback policy, active-link ceiling,
shared governor and dial backoff. A periodic peer sweep remains a health path.
Inbound routes no longer suppress the distinct outbound generation, as the
strict stock replay verified after a first pass exposed that bug.
`router_i2np::tests::router_delivery_miss_requests_a_bounded_ntcp2_dial` proves
the miss result, bounded peer request, and subsequent delivery using the newly
registered route.

The strict pinned stock replay passed again after the link-owner refactor with
one fresh daemon outbound NTCP2 session, one DatabaseLookup and its correlated
DatabaseSearchReply. The SSU2-only replay also passed with ten product replies
over SSU2 and zero NTCP2 product deliveries. The two stock modes demonstrate
real product receipt separately; live request-triggered target selection and
the full malformed/stale/restart/governor negative matrix have not yet run.

The final routine floor passed against this combined working tree: 4,849 tests
passed and 39 were ignored across 187 suites; clippy, rustdoc with warnings
denied, and doc tests passed. Planning unit tests (51), NTCP2 harness lane tests
(18), global plan/ADR uniqueness, dependency direction, runtime boundaries,
NTCP2 vectors/interoperability, and `git diff --check` also passed.

Plan 461 remains active for the normal-daemon fault matrix beyond the real-link
continuity test. In particular, the bounded miss queue and validated-snapshot
selector are unit-tested and source-pinned outbound traffic is live-tested, but
one live fixture has not yet forced the ordinary product into a no-session
request that wakes this selector. Malformed/stale address, changed-key,
half-open, cancellation, saturation, disabled-transport, and repeated
normal-process restart cases also remain to be reconciled as one acceptance
matrix. The controlled proof changes no public or broad support claim.

## Controlled dual-family completion — 2026-10-12

The request-triggered normal-daemon gate and remaining Plan 461 continuity and
fault controls now pass at controlled loopback scope. The request-driven pinned
fixture uses one source-built i2pd 2.61.0 floodfill. Its signed dual-address
RouterInfo is seeded into i2pr's actual `netdb/routers/<64-hex-hash>` cache;
the stock peer is deliberately started without i2pr's RouterInfo so it cannot
create an inbound route first. The daemon observed one `NoActiveSession`
route miss, queued bounded peer-only NTCP2 dial requests, established an
authenticated outbound generation, submitted one product DatabaseLookup, and
correlated the DatabaseSearchReply on that same generation. The sanitized
receipt was `route_misses=1`, `submitted=1`, `correlated=1`, generation `23`
for both send and reply, `outbound_links=1`, and `inbound_links=0`. This is the
request-driven proof; a separate strict four-stock-owner replay remains
nondeterministic and is not substituted for it.

Actual family and route-loss receipts are complementary and remain scoped:

- The pinned `--stock-ssu2-only` run received three confirmed candidate
  publication requests, accepted three product stores, submitted three
  product replies, and recorded six SSU2 product deliveries with zero NTCP2
  product deliveries or links.
- `router_delivery_survives_each_single_family_route_loss` passed with real
  daemon-owned SSU2 delivery and the competing NTCP2 route removed, then the
  SSU2 owner stopped while the surviving NTCP2 route continued.
- `router_delivery_uses_real_ntcp2_link_after_ssu2_owner_stops` passed using
  two authenticated real NTCP2 sessions and two authenticated SSU2 sessions.
  It decoded I2NP over SSU2 after NTCP2 loss, then decoded I2NP over a
  reauthenticated NTCP2 generation after SSU2 owner shutdown.

The fault and lifecycle rows were reconciled to executable controls. The
validated snapshot test drops stale RouterInfo; the runtime test refuses an
expired NTCP2 address before connect; the runtime and NTCP2 handshake suites
cover wrong network, identity/static-key mismatch, half-open handshakes, and
bounded backoff; the generation registry rejects wrong peer/link correlation,
stale generation responses, duplicate and expired tokens, and queue overflow.
The runtime bandwidth and NTCP2 link suites cover shared-governor capacity,
cancellation, queue return, and repeated teardown. Config tests preserve
NTCP2-disabled defaults and reject tuning as activation. The real-process
lifecycle test now performs three starts and stops with a matching-network
authenticated DatabaseStore each cycle, stable identity-bound keys, and a
wrong-network rejection on the first cycle. No support inventory or public
capability was promoted.

Focused commands and results:

| Command | Outcome |
| --- | --- |
| `rtk python3 tests/integration/ntcp2/plan468_private_netdb.py --request-driven` | Passed; one route miss, fresh outbound NTCP2 generation 23, one DatabaseLookup and same-generation correlated reply, no inbound route. |
| `rtk python3 tests/integration/ntcp2/plan468_private_netdb.py --stock-ssu2-only` | Passed; 6 SSU2 product deliveries, 3 accepted stores and replies, 0 NTCP2 links/deliveries. |
| `rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed, 135 tests, 1 ignored. |
| `rtk cargo test --locked -p i2pr-daemon --lib router_i2np -- --test-threads=1` | Passed, 28 tests, 1 ignored. |
| `rtk cargo test --locked -p i2pr-daemon --test netdb_tunnel_unit -- --test-threads=1` | Passed, 22 tests. |
| `rtk cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Passed, 35 tests. |
| `rtk cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight daemon_unknown_peer_delivery_and_cancellation_are_explicit -- --exact --test-threads=1` | Passed, 1 test. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness controlled_ntcp2_binds_loopback_and_reuses_identity_bound_keys_after_restart -- --exact --test-threads=1` | Passed; 3 daemon start/stop cycles, stable key/IV, authenticated store each cycle. |
| `rtk cargo test --locked -p i2pr-daemon --lib omitted_ntcp2_section_means_disabled` and `... ntcp2_tuning_fields_do_not_activate_when_disabled` | Passed, 1 test each. |
| `bash scripts/check-runtime-boundaries.sh`, `bash scripts/check-ntcp2-vectors.sh`, `bash scripts/check-ssu2-vectors.sh` | Passed. |

Final repository floor on the completed tree:

- `cargo fmt --all --check`, `cargo check --locked --workspace --all-targets`,
  managed-app sibling build, and `cargo build` passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` passed
  4,851 tests; 39 ignored across 187 suites.
- Workspace Clippy with warnings denied, rustdoc with warnings denied, and
  workspace doc tests passed.
- Planning tests passed (51); plan and ADR number ownership, dependency
  direction, runtime/console/service-tunnel boundaries, workflow validity,
  fixture and NTCP2/SSU2/I2CP vectors, acceptance-evidence guards, managed-app
  self-tests, M11/M12 checks, 18 NTCP2 harness tests, and `cargo deny check
  advisories bans sources` passed.

The strict four-owner `plan468_private_netdb.py` replay was also attempted. It
reached one same-generation DatabaseLookup reply over an existing inbound
NTCP2 route, but recorded no fresh outbound link in that execution and one of
three candidate-targeted publishes was unconfirmed. Its strict Plan 469 chain
therefore reported blocked. The dedicated request-driven run above is the
authoritative Plan 461 outbound receipt; this supplemental variability is
retained as a finding, not called a pass.

Disposition: **passed at controlled loopback scope**. Plan 435's separate
external/two-family public qualification, default-off posture, and no-support
promotion remain unchanged.
