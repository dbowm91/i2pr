# Plan 459 status: passed at controlled loopback scope — initiator lookup and correlated reply proven

Closure token: `passed-controlled-daemon-initiator-product-lookup-and-correlated-reply`

Plan: [`459-live-ntcp2-router-i2np-dispatch-and-reply-owner.md`](../../implementation/ntcp2-transport/459-live-ntcp2-router-i2np-dispatch-and-reply-owner.md)

## Outcome

The parse-and-drop sink is removed from both NTCP2 link roles. Authenticated
frames enter one bounded, byte-budgeted daemon ingress queue with the original
peer and link generation. The normal owner performs one canonical bounded I2NP
decode and shares its message and classification with floodfill, destination
groups, tunnel handling, and the common control path. Stale NTCP2 generations
are rejected before state mutation. A validated unsolicited RouterInfo
DatabaseStore is applied to the daemon's existing Bootstrap NetDB; unsupported
store payloads receive the existing typed rejection. NTCP2-only controlled
configuration also starts the router-I2NP owner, so the ingress queue has a
consumer without an SSU2 listener.

The controlled `i2pr run` test authenticated a local NTCP2 client, sent a valid
compressed RouterInfo DatabaseStore, and observed the normal daemon report the
store application. Wrong-network admission and identity-bound key persistence
after restart also passed. This proves the responder-role product effect and
the bounded source handoff.

At the time of this original status, normal daemon-initiated product receipt
and a correlated live reply were still open. The 2026-10-12 completion
addendum below records the causal correction and executed evidence; the earlier
assessment and its commands remain preserved as history.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Both NTCP2 parse-and-drop sinks replaced | Implemented: outgoing and incoming links call the same bounded forwarding helper; the shared owner decodes once. |
| Bounded transport-neutral ingress and lease lifecycle | Implemented: channel capacity 128, buffered-byte `ResourceBudget`, cancellation-aware admission, and byte lease held through dispatch. |
| Real product effect | Passed for a controlled local peer sending RouterInfo DatabaseStore to normal `i2pr run`; Bootstrap NetDB snapshot mutation has a unit test and the black-box process emits the accepted-store marker. |
| NTCP2-only owner lifecycle | Passed: the shared owner is registered whenever SSU2 or NTCP2 is enabled, and NTCP2 startup depends on owner readiness. |
| Correct NTCP2 delivery fallback and link-generation isolation | Unit tests passed for peer-bound bounded queue delivery, duplicate/current generation behavior, stale removal, and SSU2-first fallback. |
| Required response over eligible authenticated link | Partial only: floodfill lookup effects preserve reply route and the NTCP2 peer registry is used by `RouterDeliveryService`; no live DatabaseLookup/reply correlation was exercised. |
| Actual normal daemon initiator-role product effect | Not established. The responder-side black-box result does not prove the daemon's outbound link receiver. |
| Wrong network, malformed/expired input, cancellation, restart, bounds, SSU2 regression | Existing focused tests and full workspace tests passed; external stock-load negatives remain open. |
| Public capability, advertisement, or support-inventory promotion | None made; NTCP2 remains explicitly enabled controlled-loopback, default-off and non-advertised. |

## Commands and outcomes

All commands ran locally on Linux. The focused daemon qualification was preceded
by the required sibling build.

| Command | Outcome |
| --- | --- |
| `rtk cargo fmt --all --check` | Passed. |
| `rtk cargo check --locked --workspace --all-targets` | Passed. |
| `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | Passed. |
| `rtk cargo test --locked -p i2pr-daemon --lib bootstrap::tests::authenticated_router_store_updates_the_bootstrap_netdb_snapshot -- --test-threads=1` | Passed, 1 test. |
| `rtk cargo test --locked -p i2pr-daemon --lib router_i2np -- --test-threads=1` | Passed, 20 passed, 1 ignored. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed, 9 tests. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness controlled_ntcp2_binds_loopback_and_reuses_identity_bound_keys_after_restart -- --exact --test-threads=1` | Passed; authenticated NTCP2 DatabaseStore reached the daemon NetDB owner, wrong network was refused, restart retained identity-bound transport keys. |
| `rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed, 132 passed, 1 ignored. |
| `rtk cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Passed, 35 tests. |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed, 4,828 passed, 39 ignored across 187 suites (1,061.43s). |
| `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed. |
| `RUSTDOCFLAGS="-D warnings" rtk cargo doc --locked --workspace --no-deps` | Passed; 33 crates documented. |
| `rtk cargo test --locked --workspace --doc` | Passed, 29 suites. |
| `rtk bash tests/integration/ntcp2/plan455_direct_stock_attempt.sh` | Passed retained Plan 455 Gate A: 1 authenticated session, 1 I2NP message sent, 1 stock own-RouterInfo DatabaseStore receipt marker, 0 DeliveryStatus markers (none required). This is not stock-to-i2pr evidence. |
| `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed, 51 tests. |
| `rtk python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | Passed, 18 tests. |
| `rtk python3 scripts/check-router-readiness-contract.py` and `--self-test` | Passed after updating the guard to require the canonical `decode_authenticated_router_i2np` path and whitespace-insensitive live-endpoint binding. |
| Remaining `AGENTS.md` boundary/evidence scripts, `rtk cargo deny check advisories bans sources`, and `rtk git diff --check` | Passed. Negative-control self-test diagnostics and pre-existing cargo-deny duplicate-version warnings were expected; the commands exited successfully. |

Implementation is in commits `8f177603`, `aa163c4d`, and `3799725e` on the
working branch. The latter is pushed. No dependency, protocol wire format,
configuration default, external router source, or support inventory changed.

## Security, contention, and compatibility

- The channel and byte budget are bounded; forwarding waits are cancellation-aware and fail closed on queue closure or admission failure.
- Peer and link-generation attribution remain tied to the authenticated NTCP2 link; a superseded link cannot mutate product state.
- Replies use the existing authenticated transport delivery path and can only target the requested peer's established eligible link. No inbound-supplied address is dialed.
- SSU2 continues to feed the same owner and retains its existing product flow; the workspace suite passed with serial daemon execution.
- The normal NTCP2 profile remains loopback-only and non-advertised. No capability or version claim changed.

## Findings by severity

- Critical: none.
- High: none localized.
- Medium: normal daemon-initiated NTCP2 product receipt and an end-to-end correlated DatabaseLookup reply remain unproven; these are acceptance blockers.
- Low: the controlled black-box peer is a local harness client, not pinned stock i2pd.

## Unblock audit and disposition

- Plan 460's stated interface dependency, the live authenticated ingress and
  product consumer, is now implemented and exercised by a real `i2pr run`
  responder. Move Plan 460 to `ready` for its separate pinned-stock source and
  trigger investigation; do not treat this as completion of Plan 459.
- Plan 461 remains blocked on Plan 459's missing initiator/reply evidence and
  Plan 460's real stock-peer receipt and fresh stock-initiator gates.
- Plan 462 remains independently ready but deferred; no NTCP2 dependency is
  added to it.
- Plans 455 and 457 retain their historical blocked outcomes. This record does
  not rewrite their acceptance evidence.

Disposition at initial closure: **blocked** pending normal daemon
initiator-role product receipt and correlated live reply. Superseded by the
2026-10-12 completion addendum below.

## Completion addendum — 2026-10-12

The normal daemon initiator path authenticated pinned stock NTCP2 peers, but the
first replay exposed that `Ntcp2RouterDeliveryRegistry` keyed one route only by
peer. Stock peers already had an inbound link after sending their RouterInfo
DatabaseStore, so the successfully authenticated daemon-initiated link was
discarded as a duplicate before product delivery. The registry now retains
bounded routes by `(peer, link generation)`. Runtime active-link admission
bounds entries; ingress and reply correlation continue to require the exact
authenticated generation.

The strict source-owned private fixture passed with pinned i2pd
`635b013a612ff47278ef02acf8580a28e10e26c5`: one stock owner selected and
confirmed the candidate; the ordinary daemon accepted four RouterInfo stores,
submitted and correlated four reply-token probes, established one fresh
daemon-initiated NTCP2 link, submitted one product DatabaseLookup, and
correlated its DatabaseSearchReply. The outbound request and reply used the
same peer and link generation. The fixture also observed four inbound links,
36 forwarded I2NP messages, and four accepted stores with four product replies.
No identity, lookup key, reply token, or raw message bytes were emitted.

| Requirement | Result |
| --- | --- |
| Normal daemon-initiated authenticated NTCP2 product delivery | Passed: one fresh outbound link submitted a DatabaseLookup. |
| Real product effect and reply correlation | Passed: one DatabaseSearchReply correlated against the exact pending peer/link/key/deadline tuple. |
| Concurrent inbound/outbound links for one peer | Passed: registry stores exact-generation routes; focused registry test verifies independent generations and stale removal. |
| Existing stock-to-daemon product flow | Passed in the same execution: four RouterInfo stores accepted and four product replies submitted; four daemon probes correlated. |
| Default-off, loopback, non-advertised posture | Unchanged; no `specs/support.toml` promotion. |

Commands run after the correction:

| Command | Outcome |
| --- | --- |
| `rtk cargo fmt --all` | Passed. |
| `rtk cargo test --locked -p i2pr-daemon --lib router_i2np::tests::ntcp2_delivery_registry_is_peer_bound_generation_safe_and_bounded -- --exact --test-threads=1` | Passed, 1 test. |
| `rtk python3 tests/integration/ntcp2/plan468_private_netdb.py` | Passed; sanitized receipts recorded above, including `plan459_fresh_i2pr_ntcp2_outbound=1`, `plan459_database_lookup_submitted=1`, and `plan459_database_lookup_correlated=1`. |
| `rtk proxy bash -c 'export I2PR_PLAN467_REPLY_TOKEN=4670105; bash tests/integration/ntcp2/plan455_direct_stock_attempt.sh'` | Passed retained direct stock comparison: one authenticated session, one I2NP message sent, one stock self-RouterInfo receipt, and the Plan467 DeliveryStatus token correlated. |

## Combined-tree verification addendum — 2026-10-12

Implementation commit: `257f1eb`.

The required routine floor has now run against the combined Plan 459/461 tree,
including the request-driven outbound-owner change and real-link continuity test.
`cargo check --locked --workspace --all-targets`, the managed-app sibling build,
and the serialized workspace tests passed. The tests reported 4,849 passed and
39 ignored across 187 suites (1,100.49s). Clippy initially identified one type-complexity
warning in the new pending-lookup map; factoring its map type into a named alias
resolved it. Formatting, clippy, rustdoc with warnings denied, and workspace
doc tests then passed on the final tree. This change has no runtime or wire
effect.

Plan 460 remains passed at its controlled scope through Plan 469. Plan 461 is
active and owns the separate dual-family request-driven chooser and
failure-continuity acceptance.

Disposition: **passed at controlled loopback scope** for daemon-initiated
NTCP2 product receipt and correlated reply. Broader public and two-family
qualification remains outside this plan.
