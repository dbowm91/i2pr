# Plan 477 status — source corrections landed; first divergence remains unresolved

**Status token:** `in-progress-per-hop-and-header-id-corrections-first-divergence-unattributed`  
**Disposition:** retain the deterministic per-hop ID and separate outer-header/reply-ID corrections. The pinned product attempt improved to one of two inbound installs, but the remaining failure is not attributable to a source-owned stage. Plan 477 is not closed.  
**Implementation commit:** pending.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Give each real hop a distinct nonzero forwarding `SEND_MSG_ID`, keeping the terminal reply ID | `inbound_multi_record_trajectory_round_trips_with_one_originator_fake` decrypts all three sealed request records and checks unique IDs plus terminal correlation | Passed locally; the old shared-ID implementation fails the uniqueness assertion |
| Remove fixed-seed production tunnel/build identifiers | `service_product.rs` obtains allocator start material and build message IDs from `ChaCha8Rng::try_from_os_rng`; bounded helper rejects zero/colliding outer IDs | Implemented; same-process active/retiring ID registry collision accounting remains open |
| Separate initial short-transport I2NP header ID from terminal reply-correlation ID | `submit_destination` receives the reply ID in `DestinationBuildRequest` and the independent outer ID in `BridgeHeader::ShortTransport`; inbound matching remains strict on terminal authenticated peer and reply ID | Implemented at the composition boundary; no relaxation of reply checks |
| Distinguish queue admission from a transport write | `RouterDeliveryOutcome::Accepted` still means queue admission; no source-owned write receipt is wired to the pending build owner | Open; handed to the source-observer work in Plan 480 |
| Attribute the exact first stock return-path divergence | Two post-change Plan 458 attempts, below | Not established |
| Preserve Plan 474 owner-generation fix and avoid external SAM claims | Existing Plan 474 tests pass; both Plan 458 runs stopped before owner summary/STREAM | Preserved; no claim |

## Verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all` | Passed |
| `cargo test --locked -p i2pr-tunnel --all-targets` | Passed (401 tests) |
| `cargo check --locked -p i2pr-daemon --all-targets` | Passed |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` | Passed before focused daemon tests |
| `cargo test --locked -p i2pr-daemon --lib exploratory_build -- --test-threads=1` | Passed (13 tests) |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1` | Passed (15 tests) |
| `python3 tests/integration/core-router-recovery/test_plan474_build_wait_receipt.py` | Passed (2 tests) |
| `python3 tests/integration/core-router-recovery/test_plan464_attempt_receipt.py` | Passed (6 tests) |
| `bash scripts/check-service-tunnel-boundaries.sh` | Passed |
| `bash scripts/check-runtime-boundaries.sh` | Passed |
| `bash scripts/check-ssu2-vectors.sh` | Passed |
| `git diff --check` | Passed |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after the per-hop correction | Failed at SAM inbound-build; one authenticated reply matched and one attempt expired. Failed-attempt receipt: 29 authenticated candidates, 5 candidate peer matches, 0 expected message-ID matches; stock aggregate showed 86 records / 39 transit-create events / 4 peers |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after outer/reply ID separation | Failed at SAM inbound-build; two attempts were observed, one reply matched and install committed, the other expired. Failed-attempt receipt: 50 candidates, 8 candidate peer matches, 0 expected message-ID matches; stock aggregate showed 118 records / 58 transit-create events / 4 peers |

The stock aggregate counters do not attribute any particular request, peer, or return to a pending attempt. The one successful install demonstrates progress but does not identify why the second attempt failed. No unchanged replay is warranted without a new source-owned observation or correction.

## Invariants and limitations

Terminal correlation remains bound to the authenticated terminal peer and expected message ID. The initial I2NP transport header ID is now independently generated from the terminal reply ID. Per-hop IDs are nonzero and unique within the current path. The allocator's OS-seeded monotonic sequence avoids the previous fixed restart value; a bounded registry-aware collision search across active, pending, retiring, and replay-window scopes is still required before the ID allocation requirement can pass. Router delivery still reports admission before the transport writer confirms a write.

No identity, endpoint, raw message ID, raw RouterInfo, packet, or raw stock log is retained in this record. No protocol support or public capability is claimed. The full AGENTS.md routine floor was not run for this intermediate implementation.

## Next executable step

Plan 480 must supply source-owned authenticated session and write/product events, bound to the real peer and link generation. Use that observer to identify the first delivery or return transition for one finite exact-pinned attempt, then correct and replay only if the event proves a source-owned defect. Plan 478 may consume the written distinction between outer header ID, per-hop forwarding IDs, and terminal reply ID; this status does not qualify its tunneled service-destination path.

**Roadmap disposition:** active; no dependent capability is unblocked.
