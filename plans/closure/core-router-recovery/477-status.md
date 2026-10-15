# Plan 477 status — closed: canonical short-build IDs, collision retention, writer-receipt consumption

**Status token:** `passed-canonical-short-build-ids-collision-retention-and-writer-receipt-consumption`
**Disposition:** closed 2026-10-15. Every requirement has landed code, failing-before/passing-after tests, and live exact-pin evidence; the remaining product-path divergence (STREAM SYN-ACK return) is owned by active Plan 478. Floor reds are recorded as externally owned follow-ups below, not 477 defects.
**Implementation commits:** `623ceb5` (per-hop and outer/header ID separation), `eb8cdca` (live pending-ID exclusion and log redaction), and `1371c04` (writer-receipt lifecycle regressions with inversion proofs, `os_seeded` allocator centralization, restart-seed tests, stale-comment repair, and this closure with the registry/roadmap transition).

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

**Roadmap disposition:** closed; 478 retains the product gates. No dependent capability is unblocked.

## 2026-10-15 closure

Requirement-to-evidence close-out against the implementation plan's pass gate
(changed production contract; failing-before/passing-after tests; all
denial/collision/restart cases; an executed attributable finite reference
attempt; no external SAM STREAM or Plan 433 claim):

- Changed contract: per-hop forwarding IDs, separate outer/reply IDs,
  OS-CSPRNG ID allocation with bounded active/pending/retiring/replay
  collision retention, and Plan 481 writer-receipt consumption with the
  five-way terminal taxonomy — all landed and live-exercised.
- Tests: the ID/collision suites fail pre-fix behavior and pass after
  (retained predecessor evidence); the four closing tests each failed a
  targeted temporary production inversion and pass after its revert
  (cancel-leaks-tickets, disabled retained-set expiry, fixed allocator
  start; same-start replay pins the defect mechanism deterministically).
- Denial/collision/restart: invalid-reply rejection keeps the exact
  attempt id, orphan observations are attempt-scoped and identifier-free,
  zero/over-max rejected, cancel-by-owner counted, replay retained, and
  restarted processes draw independent allocator starts through the single
  `os_seeded()` production constructor.
- Attributable attempt: the 2026-10-15 bounded exact-pin run committed 3
  installs with 4/4 authenticated replies matched and owners 2/2 — every
  short-build leg attributed, no divergence at this plan's scope.
- Strict terminal peer/ID matching untouched throughout; no SAM STREAM,
  anonymity, advertisement, or Plan 433 claim.

Floor: full AGENTS.md floor run on the closing tree — green except two
items proven outside this plan (recorded as external follow-ups, not
defects): the ELS2 Plan 393 count guard, broken by committed 478 change
`5b874e86` (needs a 478/ELS2 plan-of-record; fix code, never weaken the
script), and `cargo deny` advisories for HEAD-pinned rustls 0.23.45
(RUSTSEC-2026-0346/0347; needs a reviewed dependency-update plan).
`Cargo.lock` and the ELS2 guard are untouched by this plan's diff.

Unblock audit: Plan 478 already consumes this plan's interface and is
active — no state change. No other registered plan lists 477 as a hard
dependency blocking readiness; nothing moves to ready. Findings:
critical — none; high — none; medium — the two external floor items
above, each with a named owner class; low — no hosted-CI run for the
closing commit.

## 2026-10-15 writer-receipt lifecycle closure and bounded post-change attempt

Narrowly controlled work inside this active plan; no new plan number.

**Source changes (all in-plan):**

- Stale-contract comment repair in `exploratory_build.rs`: the pending-attempt
  field claimed SSU2 queue admission stayed unobserved for lack of a
  writer-ticket producer. Plan 481 has since landed that producer
  (`send_i2np_with_receipt` / `deliver_with_link_and_receipt`), and the
  coordinator attaches a ticket per direct request and per TunnelData cell,
  polls them in `advance_time`, and exposes `emission_disposition`. The
  comment now states the true contract; no behavior change.
- Two lifecycle regressions pinning the handoff's late/cancel cases:
  `cancelled_build_drops_live_writer_tickets_and_never_resurrects` (cancel
  drops a live ticket; the late send fails instead of flipping any outcome;
  disposition goes `None`; identifiers stay replay-retained) and
  `retained_delivery_set_settles_without_resurrecting_its_attempt` (a
  retained set records a late write with no pending attempt, then expires
  without resurrecting anything). Cancellation already fails closed at the
  ticket level (`TryRecvError::Closed` settles `false`); these tests lock
  the attempt-level consequence.

**Verification:** `i2pr-daemon --lib exploratory_build` 19 passed (17 + 2
new); `exploratory_build_unit` 15; `exploratory_build_live` 11;
`i2pr-tunnel` 401; receipt parsers 5 + 6; runtime/service-tunnel/SSU2-vector
scripts pass; daemon clippy `-D warnings` clean; fmt and `git diff --check`
clean. (Scoped battery; full floor not run.)

**One bounded exact-pin attempt after the change** (pinned i2pd 2.61.0,
quiet mesh, scratch `df2wrjie` snapshotted). Short-build layer fully
attributed, no divergence left at this plan's scope:

- 3 installs committed, 4/4 authenticated replies matched, owners 2/2, no
  evictions, creator RI on 4/4 peers, zero decrypt/next-hop/reply-send
  failures stock-side. No inbound-build expiry this run
  (`inbound_build_wait_receipts` empty).
- Forward STREAM SYNs reach stock repeatedly (first accepted as `Incoming
  stream rSID=1497887931`, retransmits deduplicated as `already exists`).
  Stock `f` emitted the SYN-ACK and terminated it for no-ACK
  (`SYNACK packet was not ACKed after 1 attempts`); our managers observed
  zero (`synack_observed=0` over the whole wait). A second stock-side
  stream died on `Acceptor for incoming stream is not set` /
  `Pending incoming timeout expired` — stock SAM-accept scope, not ours.

**Disposition:** every Plan 477 requirement now has landed code, failing-
before/passing-after tests, and live exact-pin evidence: per-hop IDs,
outer/reply separation, CSPRNG/collision retention, writer-receipt
consumption with `delivery-accepted` / `partial` / `all-emitted` /
`not-emitted` / `unobserved` terminal reasons, late/cancel/replay
regressions, and strict terminal peer/ID matching untouched. The remaining
product-path divergence is the STREAM SYN-ACK return leg, owned by active
Plan 478 — two consecutive runs now replicate the shape (stock emits,
we observe zero), so it is characterized, not merely single-run weak
evidence. No unchanged-topology retry from this plan; the next lane moves
only on a 478-owned material change. Plan 477 stays active until the
planning flow closes it; the registry token update is a registry action,
not taken here.

## 2026-10-15 gap closure: inversion proofs, restart regression, full floor

All three gaps named for closure are now closed inside this plan.

**Failing-before proofs for the lifecycle tests.** Each new test was run
against a targeted temporary production inversion and failed, then the
probe was reverted (zero markers remain; suite green after):

- `cancelled_build_drops_live_writer_tickets_and_never_resurrects` fails
  when `cancel()` retains the delivery set in the settling map instead
  of dropping it (late send succeeds; disposition stays `Some`).
- `retained_delivery_set_settles_without_resurrecting_its_attempt` fails
  when retained-set expiry is disabled in `advance_time` (disposition
  never goes `None`).

**Restart/forked-seed regression (plan step 3).** Production
`Plan212TunnelIdAllocator` construction is centralized in one named
`os_seeded()` constructor (both provisioning call sites unified, no
behavior change); deterministic starts stay test-only and the historical
fixed constants remain `#[allow(dead_code)]` documentation. New tests:
`same_start_replays_same_sequence_so_fixed_starts_stay_test_only`
(pins the retired `0x51A7_9300` replay mechanism and the
exclude-to-stay-disjoint restart contract) and
`os_seeded_allocator_allocates_valid_disjoint_sets` (independent draws
give independent starts; sets are nonzero and disjoint). The latter
fails under a fixed-start inversion. All assertions are deterministic
except one 2^-32 independent-draw inequality, stated inline.

**Full AGENTS.md floor** (bash 5.2 host; `cargo`/`python3` run verbatim):

- Pass: `fmt --check`, `check --locked --workspace --all-targets`,
  managed-app sibling build, `test --locked --workspace --all-targets`
  (187 suites, 4897 passed, 0 failed, 39 ignored),
  `clippy --all-targets --all-features -D warnings` (no warnings),
  `cargo doc -D warnings`, workspace doc-tests (29 suites, 0 tests,
  0 failures), and every boundary/evidence/self-test script including
  dependency direction, plan/ADR uniqueness, portable-tunnel API +
  consumer, planning tests (51), runtime/console/browser/managed-app
  (process with `--self-test` via `python3`; the `bash` invocation of
  that `.py` file is a misuse, exit 2, not a failure), readiness and
  core-router-recovery contracts + self-tests, SSU2 preflight,
  M11/transit/anonymity, fixture manifest (exit 0 silent),
  NTCP2/SSU2/I2CP vectors + interop static check, SAM/SSU2/I2CP/
  I2PControl acceptance evidence, ELS2 type-11 boundary,
  encrypted-service consumer, outproxy path + wire lane, secret hygiene,
  workflow validity, floodfill type-5, service-tunnel acceptance (29
  rows), exploratory/NetDB/destination/streaming/M6 evidence, M12
  qualification + boundaries self-tests, NTCP2 harness tests (18).
  `bans`/`sources` ok.
- Two failures, both proven outside this plan's scope, neither touched
  by its diff:
  1. `check-els2-live-lane-evidence.sh` Plan 393 row (needs 3
     `.cancel_ls2_publication(request_id)` sites, tree has 1).
     Bisected to committed 478 change `5b874e86` (direct-short LS2
     publication), which removed the tunneled path's three
     failure-path cancellations; at Plan 481's commit the count was 3.
     Whether the new direct path preserves cancellation intent is a
     478/ELS2-owned semantic question — fix code, never weaken the
     script, and not inside 477.
  2. `cargo deny check advisories`: rustls 0.23.45
     RUSTSEC-2026-0346/0347 (fixed in >=0.23.46). `Cargo.lock` is
     untouched by this tree and HEAD pins the same version; the
     advisories postdate Plan 481's green floor. A dependency update
     needs review and its own plan-of-record, not a 477 drive-by.

**Remaining for closure (mechanics, not evidence):** focused commits
separating 477-owned files (`exploratory_build.rs`,
`service_product.rs` allocator/test sections, `477-status.md`) from
478-owned files, then the `i2pr-planning` registry transition
(`in-progress-*` → `passed-*`), roadmap disposition, and unblock audit.
No history rewrite, no `specs/support.toml` change, no SAM STREAM or
Plan 433 claim.

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

## 2026-10-15 re-verification on the current branch head (no new source divergence)

No lane run: the bounded stop stands (no unchanged-topology retry, no new
causal divergence to fix inside this plan). A half-finished Plan 478
floodfill-rotation edit (`begin_lease_lookup_excluding`, unwired, tree left
non-compiling) was found in the working tree and shelved to
`stash@{0}` ("WIP-478 ...") untouched for Plan 478; it is out of scope here
and no part of it was consumed. One whitespace-only repair inside this
plan's owned file: `fbc453d7` left a rustfmt drift at
`exploratory_build.rs:1914` (retired-ID orphan counter wrap); `cargo fmt -p
i2pr-daemon` restored `cargo fmt --all --check` clean with no behavior
change.

Battery on the current head (includes all later Plan 478 commits):

| Command | Outcome |
| --- | --- |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | Passed (fresh) |
| `cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1` | Passed (401) |
| `cargo test --locked -p i2pr-daemon --lib exploratory_build -- --test-threads=1` | Passed (17) |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1` | Passed (15) |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1` | Passed (11) |
| `cargo test --locked -p i2pr-daemon --lib plan477_message_id_tests` / `wrapped_outbound_invalid_reply_keeps_its_exact_attempt_id` | Passed (1 + 1) |
| `python3 tests/integration/core-router-recovery/test_plan474_build_wait_receipt.py` | Passed (5 tests) |
| `python3 tests/integration/core-router-recovery/test_plan464_attempt_receipt.py` | Passed (6 tests) |
| `bash scripts/check-service-tunnel-boundaries.sh` / `check-runtime-boundaries.sh` / `check-ssu2-vectors.sh` | Passed |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | Passed |
| `cargo fmt --all --check`, `git diff --check` | Passed after the whitespace repair above |

**Disposition:** the per-hop ID, outer/reply separation, CSPRNG/collision,
writer-disposition, and strict terminal peer/ID corrections hold under the
current tree; deterministic tests still fail pre-fix behavior and pass after
(no production contract change in this pass). The registry token (`first
divergence unresolved`) postdates the 2026-10-14 attribution section above;
updating it is a planning-registry action, not taken here. Plan 477 stays
active owning strict terminal reply attribution until the planning flow
closes it; the full AGENTS.md floor was not run (scoped battery only).
