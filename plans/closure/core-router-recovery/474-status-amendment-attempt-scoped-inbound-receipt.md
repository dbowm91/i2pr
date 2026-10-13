# Plan 474 status amendment — attempt-scoped inbound candidate receipt

This amendment records source-owned, per-attempt evidence added after the
earlier aggregate orphan counters. It does not change the original status or
the disposition of the SAM product gate.

## Source change

Each pending inbound build now retains bounded counters for authenticated
ShortTunnelBuild candidates: total candidates, candidates from the selected
terminal peer, message-ID matches, and same-peer message-ID offset classes.
The counters are held only for the lifetime of the pending attempt and are
emitted in its timeout receipt. Peer identities and message IDs are not added
to the receipt. Strict `(authenticated peer, message ID)` matching is
unchanged.

The Plan 458 receipt parser now requires and retains these bounded fields. A
focused coordinator regression covers peer-only, message-only, plus-one,
minus-one, and unrelated candidates across one pending attempt.

## Pinned replay

One exact-pinned normal SAM replay was run after the receipt change:

| Evidence | Result |
| --- | --- |
| Attempts selected/admitted/UDP-emitted | 2 / 2 / 2 |
| Authenticated reply matched and installed | 1 / 1 |
| Failed attempt terminal stage | `inbound-build` |
| Failed attempt authenticated candidates | 24 |
| Failed attempt candidates from selected terminal peer | 5 |
| Failed attempt exact message-ID candidates | 0 |
| Failed attempt same-terminal-peer ID offsets | 0 plus-one, 0 minus-one, 5 other |
| Stock aggregate | 98 short-build records; 40 transit-create events; 98 inventory delta across four peers; zero decrypt, local-next-hop, or reply-send failures |
| SAM owner summary / STREAM | Not reached |

The attempt-scoped receipt confirms that the failed attempt saw five
authenticated ShortTunnelBuild messages from its selected terminal peer, but
none had its expected message ID. It does not establish that stock accepted,
forwarded, or replied to this attempt: the five messages may be unrelated
builds. Aggregate stock tunnel and log counts cannot attribute a transition to
this attempt. The strict correlation rule remains supported by the pinned
i2pd source, which copies the short request's message ID into the forwarded
ShortTunnelBuild or endpoint ShortTunnelBuildReply.

## Verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all` | Passed |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` | Passed before focused daemon test |
| `cargo test --locked -p i2pr-daemon inbound_orphan_observations_are_attempt_scoped_and_identifier_free -- --test-threads=1` | Passed (1 test) |
| `python3 tests/integration/core-router-recovery/test_plan474_build_wait_receipt.py` | Passed (2 tests) |
| `python3 tests/integration/core-router-recovery/test_plan464_attempt_receipt.py` | Passed (6 tests) |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | Failed at `inbound-build`; receipt and stock aggregate retained above |

## Disposition

Plan 474 remains blocked before the owner-summary gate. The diagnostic change
provides attempt-scoped correlation evidence but identifies no source or
fixture defect and does not establish the historical 0/2 owner discrepancy.
Do not repeat this topology unchanged. Further work must identify a concrete
stock acceptance/forwarding or i2pr transport/ingress cause, then add a
deterministic regression and rerun the normal SAM product gate.
