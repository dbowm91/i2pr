# Plan 480 status — bounded runtime observer landed; process consumer open

**Status token:** `in-progress-bounded-runtime-observer-landed-process-consumer-open`
**Disposition:** the SSU2 runtime now has an explicitly enabled, bounded in-memory observer. It records signed local identity attestation, successful bind, inbound/outbound authenticated session promotion and loss, queued I2NP, authenticated ingress, successful/failed UDP write, staging rejection, and shutdown. The normal daemon marks a product effect only after an inserted bootstrap RouterInfo or a successful floodfill owner effect, and only for the exact preceding SSU2 peer/link/message receipt. The process-facing Plan 476 collector and a full product-effect integration lane remain open; Plan 480 is not closed.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Authenticated session lifecycle events with exact active link generation | `Ssu2RuntimeService::insert_active_locked` / `remove_active_locked` emit peer-epoch commitment + exact local link generation; real UDP test observes outbound and inbound promotion | Implemented and exercised |
| Product receipt after actual router/NetDB effect | Normal daemon records only successful bootstrap insertion or floodfill `served`, matched against a preceding authenticated peer/link/message receipt | Source hook implemented; product-effect lane still needs direct integration coverage |
| Reply receipt only after actual socket write and matching request | Bounded session queue, staged datagram, successful `UdpSocket::send_to`, and failed write paths produce distinct events with the transient message ID | Implemented; real UDP test distinguishes queue admission from actual write |
| Bounded private readout and Plan 476 consumer interface | Observer has a 512-event cap, salted per-epoch peer commitment, sequence, sticky overflow, no serialization or endpoint/payload storage; exposed only through the explicitly enabled SSU2 qualification runtime | In-process API exists; Plan 476 process collector and file/pipe readout are still open |
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
| `cargo check --locked -p i2pr-runtime -p i2pr-daemon --all-targets` | Passed after observer changes |
| `cargo test --locked -p i2pr-runtime --lib ssu2_qualification_observer -- --test-threads=1` | Passed (3 observer contract tests) |
| `cargo test --locked -p i2pr-runtime --test ssu2_local qualification_observer_tracks_real_authenticated_ssu2_events -- --exact --test-threads=1` | Passed (real UDP session and I2NP receipt) |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` | Passed before focused daemon qualification |
| `cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight daemon_controlled_exchange_reaches_central_dispatcher -- --exact --test-threads=1` | Passed (daemon-owned session, dispatcher, write and ingress receipts) |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed after the observer integration: 4,864 passed, 39 ignored across 187 suites |

The existing Plan 476/472 checks remain offline-only and have not been connected to a process receipt collector. The new runtime tests exercise the actual producer hooks over localhost UDP, while the daemon preflight verifies the observer is reachable through the daemon-owned handle. The product-effect hook is source-integrated but has not yet been driven by a qualifying NetDB/floodfill product fixture.

## Invariants and handoff

No general network API, public bind, capability advertisement, raw packet logging, or live peer attempt was added. Default-off startup does not construct an SSU2 runtime or observer. Runtime events are in-memory only; raw message IDs are transient correlation values and are not serialized by the observer. Plan 476 remains incomplete independently: its local process/session executor is missing, and the two-host live gate is unavailable until an authorized pair is supplied.

**Next executable step:** the Plan 476 process consumer (local
permission-gated receipt collector feeding its per-host executor) and the
floodfill-served product path remain open; then run the negative
provenance matrix, including link loss/replay, cancellation, overflow,
wrong network identity and changed restart identity.

## 2026-10-15 local collector, NetDB product fixture, and negative matrix landed

In-plan scope reduction, honestly recorded: what landed is the
*local* collector and the *bootstrap-insert* product path, not the Plan
476 process consumer and not the floodfill-served path.

- `crates/i2pr-daemon/tests/ssu2_qualification_receipts.rs` (new, 8
  tests): a permission-gated `LocalReceiptCollector` (consent +
  controlled-loopback permit required; default-off construction refused)
  projects one drained observer epoch into phase labels plus counts —
  the export struct has no field capable of holding a transient message
  ID, and an id-independence check proves two distinct message IDs
  project identically. A live bootstrap-insert fixture delivers a genuine
  third-party signed RouterInfo store over an authenticated session,
  ingests it through the real `Bootstrap` owner (same call the
  composition loop makes), and asserts the exact hook tuple
  `(peer, link_id, message_id)` is accepted with the effect bound to the
  ingress commitment/generation. Negatives: fabricated peer/link/ID,
  product-before-ingress, replay, cancelled delivery emitting no
  ingress, link loss clearing pending claims (live), 600-send overflow
  failing the receipt (live), revoked consent, and default-off service
  refusal. The permit gate is inversion-probed (mutant fails, reverted).
- `ssu2_qualification_observer.rs` test module: session-close clears
  pending claims; a fresh epoch rejects prior-epoch tuples (restart
  identity boundary at owner level).
- `tests/integration/ssu2/plan476_executor.py`: `LOCAL_PHASES` +
  `validate_local_observed` — single-role locally observed sequences,
  counts only, `live_qualification` stays `"not-run"`; positive and
  10-case negative matrix pass (`test_plan476_executor.py`, 6 tests).

Still open, unchanged: the Plan 476 process consumer (spawning,
per-host collection, receipt exchange), the floodfill-served product
path through the composition hook, and the live two-host Tier B (no
authorized pair supplied). 480 stays active; no dependent capability is
unblocked.

**Roadmap disposition:** active; no live SSU2 capability is unblocked.
