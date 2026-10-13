# Plan 477 status — source corrections landed; first divergence remains unresolved

**Status token:** `in-progress-per-hop-collision-scopes-landed-first-divergence-unattributed`
**Disposition:** retain the deterministic per-hop ID and separate outer-header/reply-ID corrections. The pinned product attempt improved to one of two inbound installs, but the remaining failure is not attributable to a source-owned stage. Plan 477 is not closed.  
**Implementation commits:** `623ceb5` (per-hop and outer/header ID separation) and `eb8cdca` (live pending-ID exclusion and log redaction).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Give each real hop a distinct nonzero forwarding `SEND_MSG_ID`, keeping the terminal reply ID | `inbound_multi_record_trajectory_round_trips_with_one_originator_fake` decrypts all three sealed request records and checks unique IDs plus terminal correlation | Passed locally; the old shared-ID implementation fails the uniqueness assertion |
| Remove fixed-seed production tunnel/build identifiers | The production coordinator and service allocator start from `ChaCha8Rng::try_from_os_rng`; message allocation rejects zero and boundedly retries collisions | Implemented; deterministic constructor remains test-owned |
| Prevent collisions across pending, active, retiring, and replay-retained build IDs | Coordinator tracks pending tunnel IDs and retains completed/cancelled tunnel IDs for the 30-minute maximum tunnel lifetime plus 15-minute I2NP replay window. Outer-header and terminal message IDs are both retained for the replay window. Service provisioning excludes coordinator reservations and IDs allocated earlier in the same startup pass. The validation permits only the protocol-required adjacent `next_tunnel == following.receive_tunnel` alias; all other path collisions and registry overflow fail closed. | Implemented locally; coordinator regression checks pending/retained inventory, cancellation, expiry, and adjacent chain aliases |
| Separate initial short-transport I2NP header ID from terminal reply-correlation ID | `submit_destination` receives the reply ID in `DestinationBuildRequest` and the independent outer ID in `BridgeHeader::ShortTransport`; inbound matching remains strict on terminal authenticated peer and reply ID | Implemented at the composition boundary; no relaxation of reply checks |
| Distinguish queue admission from a transport write | Plan 480 records source-owned UDP write outcomes in the bounded runtime observer, but the short-build attempt owner does not yet consume those receipts | Open; a queued delivery can still be confused with a sent-but-unanswered build |
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
| `cargo test --locked -p i2pr-daemon --lib plan477_message_id_tests -- --test-threads=1` | Passed (1 test, after `eb8cdca`) |
| `cargo test --locked -p i2pr-daemon --lib wrapped_outbound_invalid_reply_keeps_its_exact_attempt_id -- --test-threads=1` | Passed (1 test, after `eb8cdca`; also checks pending-ID inventory and lookup) |
| `cargo test --locked -p i2pr-daemon --lib exploratory_build -- --test-threads=1` | Passed (15 tests after collision retention, adjacent chain-alias regression, and tunnel-route composition changes) |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1` | Passed (15 tests after the current source pass) |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1` | Passed (11 tests after updating the synthetic responder to return the terminal ID independently from the outer transport ID) |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed after the current ID, route, and observer changes: 4,864 passed, 39 ignored across 187 suites; includes daemon service-product and exploratory-build coverage |
| `cargo test --locked -p i2pr-daemon --lib exploratory_build -- --test-threads=1` | Passed (13 tests, after `eb8cdca`) |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1` | Passed (15 tests, after `eb8cdca`) |
| `cargo check --locked -p i2pr-daemon --all-targets` | Passed (after `eb8cdca`) |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | Passed (after `eb8cdca`) |
| `git diff --check` | Passed |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after the per-hop correction | Failed at SAM inbound-build; one authenticated reply matched and one attempt expired. Failed-attempt receipt: 29 authenticated candidates, 5 candidate peer matches, 0 expected message-ID matches; stock aggregate showed 86 records / 39 transit-create events / 4 peers |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after outer/reply ID separation | Failed at SAM inbound-build; two attempts were observed, one reply matched and install committed, the other expired. Failed-attempt receipt: 50 candidates, 8 candidate peer matches, 0 expected message-ID matches; stock aggregate showed 118 records / 58 transit-create events / 4 peers |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after collision-scope and Plan 478 route changes | Failed at inbound-build. The bounded first outbound bootstrap reply matched and installed; the following inbound attempt expired. Sanitized receipt: 5 authenticated candidates from the expected peer, 3 peer matches, 0 expected message-ID matches; i2pd saw 27 ShortTunnelBuild records and 10 transit-create events across four peers. No raw IDs or logs retained. |

The stock aggregate counters do not attribute any particular request, peer, or return to a pending attempt. The new attempt's daemon-owned count receipt localizes the first observed failed transition to authenticated candidate matching: peer matches were observed but the expected terminal message ID never matched. It still cannot prove which stock request produced those candidates or confirm its outbound UDP write. No protocol acceptance or Java/i2pd family pass is claimed.

## Invariants and limitations

Terminal correlation remains bound to the authenticated terminal peer and expected message ID. The initial I2NP transport header ID is independently generated from the terminal reply ID. Per-hop IDs are nonzero and unique within the current path. The OS-seeded tunnel allocator avoids the previous fixed restart value. Coordinator-owned active and retiring IDs remain reserved through maximum tunnel lifetime plus the I2NP replay window. Router delivery still reports admission before the transport writer confirms a write.

No identity, endpoint, raw message ID, raw RouterInfo, packet, or raw stock log is retained in this record. No protocol support or public capability is claimed. The complete AGENTS.md routine floor passed after the current source pass: 4,864 workspace tests passed (39 ignored), with Clippy, rustdoc/doc-tests, repository boundary/evidence guards, and `cargo deny` green. The source-owned write receipt and registry-wide active/retiring/replay collision accounting remain open, so the finite reference failure still has no attributable first divergence.

## Next executable step

Connect the Plan 480 source-owned write events to each pending build's peer/link generation and terminal outcome, so one finite exact-pinned attempt can tie candidate mismatches to a specific emitted request. The immediate failure transition is now bounded and count-attributed; the request-to-return attribution remains open. Plan 478 has an outbound TunnelData source route but its full SAM product path remains unqualified.

**Roadmap disposition:** active; no dependent capability is unblocked.
