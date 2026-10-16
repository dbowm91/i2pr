# Plan 480 status — bounded runtime observer landed; process consumer open

**Status token:** `passed-local-observer-and-receipt-interface-process-consumer-with-476`
**Disposition (2026-10-15 close-out):** Plan 480's owned scope is
complete and verified (matrix below). The Plan 476 per-host process
consumer (spawning, per-host collection, receipt exchange) and the
live two-host Tier B remain 476-owned and environment-blocked; per
the plan's own pass gate they do not hold 480 open, and passing 480
does not mark 476 complete. No dependent capability is unblocked.
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

**Roadmap disposition:** superseded by the closure below (was active at the time).

## 2026-10-15 floodfill-served fixture, production receipt export, layering finding

In-plan scope, no new plan number. The Plan 476 process consumer and
Tier B remain 476-owned/environment-blocked (unchanged).

- `crates/i2pr-daemon/tests/ssu2_qualification_receipts.rs` (now 9
  tests): `daemon_product_hook_accepts_floodfill_served_store_over_live_session`
  drives a serving coordinator to Active through the real
  eligibility/role transitions (documentation-address material, all
  measured signals true; no install/publish, no traffic), delivers a
  genuine third-party signed netId-2 store over a live authenticated
  session, and asserts the exact owner entry point the composition
  calls (`handle_authenticated_decoded`, the callee behind
  `handle_decoded_inbound`) returns `Store(Stored { .. })` — the
  composition's served arm. The exact hook tuple is then accepted and
  the collector projects `product_effect_observed` in the same epoch.
  The shared store fixture now carries `router.version` + `netId=2`;
  the bootstrap-insert test still passes on the same bytes.
- `crates/i2pr-daemon/src/ssu2_qualification_receipts.rs` (new
  production module, `pub` for the 476 consumer): `LOCAL_PHASES`,
  sole-variant `LocalQualificationProfile::ControlledLoopback`,
  `LocalQualificationPermit`, `LocalObservedReceipt` (phase labels +
  counts only, `live_qualification` always `"not-run"`),
  `LocalReceiptCollector::new/collect` (moved verbatim from the test;
  consent re-checked at collect; overflow/empty/non-monotonic/
  product-without-ingress/no-session fail closed; transient IDs never
  leave `collect`). Pure projection: no I/O, sockets, clock, timers,
  or tasks — runtime/dependency boundaries pass. The integration
  tests now exercise the production path (9/9 green); the Python
  `LOCAL_PHASES` comment repoints at the module.
- Plan item 6 delivered in the implementation plan: receipt contract
  plus the 476 continuation pointer (drain through
  `Ssu2DaemonHandle::qualification_observer` + this collector;
  spawning/collection/exchange/revocation stay 476-owned; Tier B
  needs the authorized pair).
- Layering finding (no new scope): no owner-level record-content
  network gate exists at any owner — a correctly signed foreign-
  `netId` store is `Stored` by bootstrap and floodfill serving alike
  (probed live, then reverted to a documented finding, not a test).
  Network enforcement lives at the session codec (`SSU2_NETWORK_ID`
  byte-14 check,
  `long_header_rejects_bad_version_network_flags_and_ids`). A
  record-content gate would be a NetDB plan-of-record, not 480.
  Restarted-identity rejection stays covered by the observer's
  `fresh_epoch_rejects_prior_epoch_tuples`.

Still open after this turn: the 476 process consumer, Tier B (no
pair), full floor on the closing tree, then narrow infrastructure
close-out. No dependent capability is unblocked.

## Closure (2026-10-15): requirement-to-evidence matrix

| Requirement (implementation plan §§1–6) | Evidence | Result |
|---|---|---|
| Authenticated session/product events from the real daemon, exact active link generation | Runtime observer hooks at promotion/removal/ingress/write; real-UDP session tests; daemon preflight through the owned handle | Passed |
| Product receipt only after actual owner effect, exact preceding tuple | Bootstrap-insert and floodfill-served fixtures over live sessions (exact `(peer, link, message_id)` bound to ingress commitment/generation); fabricated/replay/cancelled/link-loss claims refused | Passed (9 receipt tests) |
| Bounded private readout, no ID/endpoint/payload material | 512-event cap + sticky overflow, salted per-epoch commitments, sequence checks; export struct provably message-ID-free (id-independence test); `live_qualification` always `"not-run"` | Passed |
| Negative provenance matrix | Fabricated peer/link/ID, product-before-ingress, replay, cancelled-no-ingress, live link-loss clearing, live 600-send overflow, revoked consent, default-off refusal, epoch-boundary restart rejection (`fresh_epoch_rejects_prior_epoch_tuples`); wrong-network-identity covered at the enforcing layer (session codec `long_header_rejects_bad_version_network_flags_and_ids` — owners have no record-content net gate, recorded finding, not a gap) | Passed |
| Interface (not second executor) to the 476 runner + fake producer | Production `ssu2_qualification_receipts` module (pure, profile-gated); `LOCAL_PHASES`/`validate_local_observed` + 10-case executor negatives (6 tests); fake adapter has no socket/subprocess by construction | Passed |
| No default/public capability change | Default-off construction refused (test-pinned); floodfill/SSU2 stays non-advertised; `specs/support.toml` untouched | Passed |

Commands (closing tree): `cargo test --locked --workspace --all-targets -- --test-threads=1` — 4922 passed, 39 ignored, 188 suites; `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — clean; `cargo fmt --all --check` — clean; `RUSTDOCFLAGS="-D warnings" cargo doc` — clean; doctests 0/29 suites; `tests/planning` 51 OK; number-uniqueness passed; runtime/dependency/service-tunnel boundaries passed; SSU2 (15 rows) + SAM (28 rows) evidence integrity passed; `cargo deny` red only on the known external rustls advisories (no dep change).

Limitations: hook fires on owner dispatch for the exact ingress tuple (does not re-judge owner acceptance — composition's served arms own that mapping); no record-content network gate at any owner (NetDB plan-of-record if ever wanted); Tier B + process consumer are 476's.

Unblock audit: no registered plan lists 480 as a hard/interface dependency (registry-wide search); the designated consumer is the same Plan 476, which stays `blocked` (executor + pair). Nothing promoted.

**Roadmap disposition:** closed-narrowly (infrastructure); no live SSU2 capability is unblocked.
