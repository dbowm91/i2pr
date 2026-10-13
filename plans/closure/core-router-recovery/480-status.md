# Plan 480 status — existing source seam verified; observer not implemented

**Status token:** `in-progress-authenticated-session-and-product-receipt-owner-missing`  
**Disposition:** the existing SSU2 runtime hands the daemon an authenticated `Ssu2InboundI2np { peer, link_id, bytes }`, and router dispatch preserves peer/link on typed outcomes. Those are usable source inputs, not a bounded qualification observer. Plan 480's actual observer and producer-owned event contract have not been implemented; Plan 480 is not closed.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Authenticated session lifecycle events with exact active link generation | Runtime owns promotion/removal and exposes aggregate counters plus point-in-time session lookup | No observer event contract |
| Product receipt after actual router/NetDB effect | Daemon dispatcher exposes typed outcomes; downstream owners remain separate | No immutable observer receipt tied to owner effect |
| Reply receipt only after actual socket write and matching request | SSU2 send result is queue acceptance; socket send loop increments datagram counters without a build-scoped receipt | Not implemented |
| Bounded private readout and Plan 476 consumer interface | No qualification observer profile/readout or Plan 476 local process collector exists | Not implemented |
| Pair-bound Tier B qualification | User confirmed no private-LAN host pair is available; no live attempt was run | Environment unavailable |

## Verification

| Command | Outcome |
| --- | --- |
| `cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed (136 tests, 1 ignored) |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` | Passed before focused daemon tests |
| `cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1` | Passed (6 tests, 1 ignored) |
| `cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed (9 tests) |
| `python3 tests/integration/ssu2/test_plan476_executor.py` | Passed (4 offline executor tests) |
| `python3 tests/integration/ssu2/plan472_live.py --self-test` | Passed (2 positive / 8 negative cases) |
| `python3 scripts/plan449-ssu2-preflight.py --self-test` | Passed (8 positive/negative cases) |
| `bash scripts/check-ssu2-acceptance-evidence.sh` | Passed integrity check (15 command-derived rows) |
| `bash scripts/check-runtime-boundaries.sh` | Passed |

The tests validate existing runtime/product behavior and offline authorization checks. They do not supply Plan 480's source-owned observer.

## Invariants and handoff

No general network API, public bind, capability advertisement, packet logging, or live peer attempt was added. Plan 476 remains incomplete independently: its local process/session executor is missing, and the two-host live gate is unavailable until an authorized pair is supplied. Plan 480 must first provide bounded producer-origin session, authenticated I2NP, product-effect, write-confirmation, withdrawal, and shutdown events; then the existing Plan 476 process owner must consume those receipts without treating script-authored phases as evidence.

**Roadmap disposition:** active; no live SSU2 capability is unblocked.
