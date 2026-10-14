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

## 2026-10-13 exact-pin short-build source trace and writer-receipt correlation

The unmodified i2pd 2.61.0 source at
`635b013a612ff47278ef02acf8580a28e10e26c5` was checked out read-only and
traced from tunnel delivery through transit forwarding and both reply forms.
The source path is consistent with the current Plan 477 wire contract:

1. When a build is routed through an existing outbound tunnel, i2pd's outbound
   `TunnelEndpoint::HandleNextMessage` sends a router-delivery I2NP message to
   the selected first peer after the tunnel endpoint reconstructs it. That
   first peer's `Tunnels::HandleShortTunnelBuildMsg` hands a short build that
   does not match its own pending-build table to
   `TransitTunnels::PostTransitTunnelBuildMsg`.
2. `TransitTunnels::HandleShortTransitTunnelBuildMsg` finds the record addressed
   to its own truncated router hash, decrypts its request, applies its normal
   transit-acceptance checks, and installs the transit role when accepted. It
   seals its response record in place and re-encrypts the other records.
3. A non-endpoint hop forwards a `ShortTunnelBuild` to the decrypted
   `next_ident`, with the I2NP header ID copied from that hop's decrypted
   `SEND_MSG_ID`. For an inbound build whose creator is the final next router,
   the final real participant forwards the updated `ShortTunnelBuild` to the
   creator. For an outbound build, the endpoint instead creates a
   `ShortTunnelBuildReply` using its `SEND_MSG_ID`, wraps it with
   `RGarlicKeyAndTag`, and sends it in a `TunnelGateway` to the recorded next
   router/tunnel.
4. The i2pr inbound-build path accepts only the authenticated terminal peer
   and expected terminal `SEND_MSG_ID`. The outbound response path checks the
   authenticated terminal peer and recorded reply tunnel, unwraps Garlic, and
   checks the inner reply ID. The outer `TunnelGateway` ID is not used as the
   build reply ID.

Plan 481's source-owned receipt now distinguishes local writer completion from
queue admission. In the bounded exact-pin run recorded by Plan 481, the
originator disposition was `outbound:all-emitted` and the UDP writer emitted
the frame; the stock aggregate showed 86 ShortTunnelBuild records, 36
transit-create events, and four peers, but could not attribute a particular
record, acceptance, or reply to that build. The later Plan 482 attempt also
ended at `outbound-build` after its source-owned delivery disposition was
`outbound:all-emitted`. A local socket write establishes that the frame was
submitted to the local UDP socket. It does not establish remote receipt,
transit acceptance, terminal forwarding, or reply delivery.

**Disposition:** this source trace found no specific protocol or return-routing
defect to correct. The next Plan 458 run is therefore not justified on the
unchanged topology: no correlated stock terminal response or source-attributed
first divergence is available from the aggregate receipts. Plan 477 remains
active. A future causal replay must correlate the test-host's private attempt
metadata to a specific stock-owned first-hop/terminal event while persisting
only sanitized counts and categories; it must not patch i2pd or relax strict
peer/message-ID matching. No raw IDs, identities, endpoints, or stock logs
were retained by this source audit.

## 2026-10-14 narrow harness attribution and creator-RI propagation divergence

Narrowly controlled investigation using the existing Plan 458 harness only;
no general verification loop was run for this attribution.

Fixture correction: `run_plan458_sam_product.py` did not strip ANSI color
escapes the daemon emits even when piped, so `parse_attempt_receipt_lines`
and `router_network_sam_failure_stage` missed every `direction`,
`originator_write_disposition`, `transition`, `failure_stage`, and
`inbound_build_wait_expired` field. Sanitized receipts collapsed to
`attempts_observed=0`, empty dispositions, and `terminal_failure_stage=null`
despite stock showing ShortTunnelBuild traffic. The parser now strips
`\x1b\[[0-9;]*m` before every regex, with a failing-before/passing-after
regression (`test_ansi_colored_daemon_lines_still_parse`). Existing
`test_plan474_build_wait_receipt` (3 tests) and `test_plan464_attempt_receipt`
(6 tests) still pass.

Product seam: `await_destination_build_install` now logs
`inbound_gateway_messages`, `inbound_gateway_peer_match`,
`inbound_gateway_route_match`, `inbound_orphans`, `invalid_replies`, and
`pending_builds` alongside `originator_write_disposition` on
`build_wait_expired`, so outbound bootstrap timeouts carry the same
gateway-orphan attribution previously available only for inbound waits. No
wire format, matching rule, or advertisement changed.

One bounded exact-pin run after those two material changes (pinned i2pd
2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`, four loopback
floodfills, ordinary `i2pr run`):

- Outbound destination bootstrap (direct): `selected`, `delivery_admitted`,
  `udp_emitted`, then `authenticated_reply_matched` for attempt 1 and install.
  Direct short-build crypto, per-hop IDs, outer/reply separation, and garlic
  TunnelGateway correlation are proven for the direct path.
- Inbound destination via outbound TunnelData: `selected`,
  `delivery_admitted`, two `udp_emitted` (two TunnelData cells to the
  outbound first hop), disposition `inbound:all-emitted`, then
  `inbound_build_wait_expired` with `inbound_routed=1` (from the outbound
  success), `inbound_orphans=24`, `authenticated_candidates=24`,
  `candidate_peer_matches=5`, all five in `peer_id_other`, zero message-ID
  matches, `inbound_gateway_messages=1` with zero peer/route matches. Stock
  aggregate: 82 ShortTunnelBuild records, 38 transit-create events, zero
  decrypt failures, zero next-hop rejections, zero reply-send failures.
  Terminal stage `inbound-build`. No LS2 owner summary or STREAM was reached.

Stock-source correlation on the test host (ephemeral, not retained beyond
counts): the retained stock logs show peer `f` adding our creator RI at
`03:12:04.343` with immediate SSU2 session, peer `d` adding at `04.148` with
session at `04.461` (both before our inbound send at `04.568`), while peers
`c`/`n` log `RouterInfo for <our-hash> not found, requested` at `14.5xx`,
then `Store request`, `added`, `found, trying to connect`, and session
established 10s after our send. The five peer-matching orphans with `other`
IDs are therefore unattributed stock-owned builds through us, not our
terminal return; our terminal forward (P2=`c`/`n`) was dropped at
`Transports::SendMessage` for lack of our RI. The first unobservable
transition is stock P2->creator SendMessage requiring creator-RI resolution,
not per-hop IDs, outer/reply separation, collision retention, or writer
receipts (all proven emitted). No raw IDs, hashes, endpoints, or stock logs
are retained in this record beyond the categorical counts above.

**Disposition:** 477's ID, collision, and writer-receipt contracts remain as
implemented; this run supplies the missing source-owned first divergence with
explicit bounded stop. Do not retry unchanged topology for per-ID matching:
the next replay must ensure creator-RI resolvability by all transit
candidates before inbound (for example, wait for `RouterInfo added` on all
four stock logs before `i2pr run`, or constrain the inbound terminal to peers
with existing sessions without adding new direct dials), then re-run one
bounded Plan 458 attempt. That RI-propagation and TunnelData-delivery owner
is handed to active Plan 478; no new per-timeout plan number is created.
Plan 478 product gates (genuine inbound install, owner generation, signed
LS2, bidirectional binary SAM STREAM, expiry/replacement) remain open.
