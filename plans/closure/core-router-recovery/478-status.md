# Plan 478 status — inbound service route and external SAM product remain open

**Status token:** `in-progress-outbound-tunnel-route-wired-product-qualification-open`
**Disposition:** Service build composition now supports router-directed TunnelData through an activated outbound role. Startup creates at most one initial direct outbound bootstrap per daemon, then routes inbound, replacement, and additional outbound service builds through an existing outbound role. No network product qualification has run against this change. Plan 478 is not closed.

## 2026-10-14 bounded-retry + send-path attribution (commits `d58b29b`, `0ece629`, `a4fee2a`, `6fddcd3`, `35972c9`)

Narrowly controlled work inside this active plan; no general verification loop.

**Source changes (all in-plan, no new plan number):**

- Session-preferring inbound terminal reselection (`service_product.rs`): up to 8 bounded reselects prefer a terminal with a live SSU2 session, fail-open to the last qualified path. No new dial; Plan 482 authority routes dials only through the outbound first hop. Outcome is now observable as a redacted `inbound_terminal_reselected` log (`session_backed`, `reselections`, no identities).
- Bounded outbound-bootstrap retry: an emitted-but-unanswered bootstrap (`OutboundBuildMissing` + `AllOriginatorDeliveriesEmitted`/`PartialEmission` + live token) is cancelled (pending slot freed, identifiers replay-retained) and resubmitted exactly once on a freshly selected path with fresh tunnel/message identifiers and a new dial authority, logged as `bootstrap_retry`. Explicit rejection, invalid replies, local delivery failure, unobserved writes, and cancellation never loop. Pure predicate has unit coverage.
- Redacted send-path diagnostics on every build submit: `delivery_family` (`ssu2`/`ntcp2`/`mixed`/`unobserved`, via the existing `RouterDeliveryTransport::as_str`) and ephemeral selected-link `delivery_links` generations (numbers only). The harness counts families into the attempt diagnostic.
- Harness: ANSI-escape stripping in receipt parsers (prior receipts collapsed to empty), `I2PR_PLAN458_KEEP_SCRATCH=1` to preserve lane scratch for post-failure window correlation.

**Live evidence (pinned i2pd 2.61.0, four loopback floodfills, ordinary `i2pr run`):**

- Outbound direct bootstrap installs first-try in most runs (25 ms reply). One run fired the new retry correctly: first attempt emitted-but-unanswered → `bootstrap_retry` → fresh path on a new link generation → second attempt also unanswered → failed closed with `outbound-build`. Mechanism validated; success not yet observed.
- Two tunneled-inbound runs: outbound installs, 2 TunnelData cells socket-accepted (`inbound:all-emitted`, SSU2 family), then `inbound_build_wait_expired` with peer matches but zero message-ID matches. Reselection found a session-backed terminal (`session_backed=true`, 4 selections) with no effect.
- Stock-source correlation (ephemeral, counts only): **zero** `TunnelData`/`TunnelGateway` (I2NP types 18/19) receipts on any of the four peers across whole runs. The pinned source logs every I2NP message unconditionally at debug (`I2NPProtocol.cpp` `HandleI2NPMessage`), so socket-accepted cells never reached any stock I2NP dispatcher. Zero decrypt failures, zero next-hop rejections, zero reply-send failures in the windows. This is a systematic send-side non-arrival, not the 477 RI-resolvability divergence (receipt needs no RI over the existing first-hop session).
- Mesh-churn finding: stock-to-stock builds also time out in this 4-peer mesh (5 pending-build timeouts on one peer in one window; per-peer ShortTunnelBuild rates vary 2–3× run to run; our RI reached 2/4, 2/4, then 0/4 peers across runs). Our non-transit presence is a likely churn amplifier (stock exploratory builds select us, time out, retry). Per-run variance is high; single-run outcomes are weak evidence either way.
- No LS2 owner summary, publication, STREAM, or recovery stage was reached in any run.

**Next (in-plan):** symmetric accounted inbound retry (same predicate shape; the inbound wait currently fails closed without one); pre-run mesh-churn observability in the lane receipt (no gating); then one bounded lane run reading tunneled link generations. Own-RI `DatabaseStore` publication stays a conditional separate plan-of-record, not smuggled into 478.

## 2026-10-14 endpoint-reply proof and creator-RI publication (commits `dec1a66`, `02831f0`, `ce9cfcc`)

**Breakthrough 1 — the outbound failure is an endpoint direct-reply RI miss, proven end to end.** In a quiet-mesh run (`mesh_quiet: true`, churn 0/0/0/0), both bootstrap attempts fully traversed their 3-hop paths (tunnel-id chains `f→d→c` and `n→f→c` reassembled from per-peer transit logs) while the endpoints created their roles and then went silent: zero type-26 replies mesh-wide after the builds. At both emit seconds the endpoint logged `Transports: RouterInfo for <creator> not found, requested` + `failed to send messages`, and the hash recomputed offline to the lane's fresh local RouterInfo (counts only retained). Pinned-source cause: `TransitTunnels::HandleShortTransitTunnelBuildMsg` sends the endpoint reply via `Transports::SendMessage(next_ident, TunnelGateway(...))`, and our Plan 114 builder records the creator as the outbound reply router, so the reply is a direct creator send requiring the creator RI. No decrypt failure, no next-hop rejection, no reply-send failure anywhere — the reply died before queueing for lack of our record.

**Fix landed:** each provisioning pass now publishes our signed RouterInfo (the same public bytes handshakes emit) to the 3 nearest known floodfills via explicit sessions under a narrow `CreatorRouterInfoPublication` dial purpose that never derives from a gateway or path role (additive to the closed Plan 482 surface; existing purpose mappings untouched). Best-effort and bounded: missing record, no floodfill, dial/store rejection, and cancellation only log redacted counts and provisioning continues. Next lane run: 3 stores accepted × 2 passes, all 4 peers added our RI, bootstrap reply matched in 2 ms, outbound installed. The `Store request` receipt counter stayed 0 while `RouterInfo added` hit 4/4 — propagation actually rode the publication dials' SessionConfirmed handshakes; the explicit stores are belt-and-braces.

**Breakthrough 2 — the tunneled inbound leg has an independent send-side defect.** Same run, same live session/link as the 2 ms bootstrap success: both tunneled cells socket-accepted, yet stock shows zero type-18/19 across all peers. First-hop logs prove partial arrival: one `First fragment (1033B)` + `Follow-on fragment (9B)` pair and nothing else — the second cell and any dispatch never appear, with no reassembly error or expiry inside the run window. Direct and tunneled sends through the identical session 30 ms apart behave differently, so session/endpoint/RI theories are dead for this leg; the defect is in our cell fragmentation/send accounting or the first hop's reassembly of it. Next instrument (not yet built): log per-cell SSU2 fragment counts at send and compare against received fragments to separate an our-side framing/drop bug from a stock-side reassembly silence.

Also validated this turn: the outbound bootstrap retry fires exactly once on a fresh link and fails closed (`bootstrap_retry` in receipts); the inbound retry fires and resubmits (`inbound_retry`); the mesh-quiescence gate holds i2pr start until the startup storm settles (bounded 10×30 s, recorded `mesh_quiet`, never fatal); session-preferring reselection finds live terminals (`session_backed=true`) without changing outcomes. No LS2 owner summary, publication, STREAM, or recovery stage reached yet.

## 2026-10-14 tunneled-inner expiry fix and first inbound install (commits `c67a40a`, `0c6492e`)

**Breakthrough 3 — the tunneled cell was fine; its inner was short-framed.** Per-cell accounting (new permanent `cell_count`/`cell_wire_lens`/`cell_message_ids` submit diagnostics, exact-ID matched against stock logs) proved each inbound submit emits exactly one 1037-byte TunnelData cell (882-byte short inner, 2 SSU2 fragments) and the first hop logs exactly that pair. A pinned-source re-read showed tunnel cells bypass the `Handling message with type 18` log by design (`PutNextMessage` routes TunnelData straight to the tunnel queue), so the missing type-18 line was never evidence of non-arrival. The cell traversed the whole outbound path twice (first-hop fragments, OBEP endpoint handling, clean tunnel checksum) and died as `TunnelMessage: Message expired` at the OBEP: the stock tunnel thread checks the inner `IsExpired` at the standard millisecond offset, and a short inner reads its seconds expiry plus record bytes as far-future milliseconds. Stock frames its own tunnel-carried builds standard (`FillI2NPMessageHeader`, ms expiry). The tunneled branch now wraps the identical records/identifiers/instant in a standard envelope (889 bytes, still one cell); the direct bootstrap keeps proven short framing. Next lane run: 3 installs, inbound owners 2/2, signed LS2 with 2 leases. Deterministic standard-framing composition test pinned.

**Breakthrough 4 — direct standard stores expire 6/6 on stock.** Exact-ID correlation showed all six creator-RI publication stores (`DatabaseStore`, standard envelope, direct session delivery) dying as `SSU2: Message <id> expired` on three peers, while handshake-embedded RIs propagated (the `creator_ri_added` counts rode `SessionConfirmed`, never the stores). Mechanism: the SSU2 session layer parses every outbound message as short `[type][id][expiration-seconds]`, so standard bytes expose the high half of millisecond time as seconds (year 1970). Same trap the floodfill path already fixed (Plan 302) by switching to short. Direct publication now uses a new `encode_store_envelope_short` (same 60 s lifetime, seconds units); tunnel-carried publication (LS2 via outbound role) was already standard and stays so — the rule is short across sessions, standard inside cells. Deterministic short-envelope round-trip test pinned.

**In-plan defect found and fixed: pool-advance refresh desynchronised owners.** Two runs reached `destination-summary-incomplete` with an exact signature (new permanent redacted attribution: `owners_table_len=2 any_key_present=false any_destination_match=true bridge_receive_count=2`): the bridge and the owner table held disjoint id sets for the same destination. The advance loop's `refresh_router_pool_snapshot` replaced the bridge LS2 + receive projection when the pool snapshot changed without touching `inbound_tunnel_owners`. New transactional `refresh_service_router_pool_snapshot_with_owners` (same owners-then-bridge lock order as install) unregisters dropped ids and registers added ids under the existing same-destination runtime, failing closed on conflicts; deterministic coherence test pinned.

**Remaining live variance (run-to-run, churn-coupled, not yet closed):** one run failed `leaseset-publication` fast after owners installed (cause not yet read — the Activate site now warns a static `publication_failure_stage` reusing the existing failure taxonomy instead of a collapsed label); one run failed `inbound-build` with peer-matching but id-mismatching replies (`candidate_peer_match_id_other`), consistent with stock transit builds routed through us during the wait plus a missing terminal reply. Run outcomes currently swing with mesh churn (first-hop/link selection and stock-through-us noise vary); single-run results stay weak evidence either way per the standing bounded-execution policy.

**Evidence hygiene:** `/tmp` lane scratch proved volatile (a reaper cleared kept scratch dirs mid-turn, including the analyzed runs). After each lane run, snapshot the scratch into the repo evidence root before analysis; status notes above are reconstructed from committed code, exact-ID receipts, and contemporaneous log extracts.

## 2026-10-14 LS2 publication, ECIES peer, and outbound STREAM resolution (commits `4fabc4f`–`51dc94b`, `666bff2`–`02b810d` and follow-ons)

**Breakthrough 5 — direct LS2 publication lands.** The tunneled LS2 path was abandoned after exact-ID tracing showed its cells arriving yet never surfacing (no tunnel effect, no warning, unattributed): LS2 now goes direct short-framed to the selected floodfill with a narrow `ServiceLeaseSetPublication` dial purpose, mirroring the proven creator-RI path. First run with it: `LeaseSet2 updated` for our key on the target floodfill. A duplicate-composition wart (provision + activate each publish) remains; stores are idempotent. The tunneled composition stays in-tree for future re-hardening.

**Breakthrough 6 — the lane peer must be ECIES (lane profile, not product).** i2pr→stock STREAM failed `INVALID_KEY`: stock SAM defaults to ElGamal destinations and the router product is ECIES-only by declared scope (ElGamal fails closed per the I2CP conformance note). The driver now requests `CRYPTO_TYPE=4` for the stock peer only. A redacted decode-failure taxonomy (`base64 invalid` / `structure invalid` / `not X25519`) replaced the catch-all message.

**Breakthrough 7 — on-demand peer LeaseSet2 fetch.** Outbound STREAM to a never-seen peer failed closed as `NotCached` with no lookup ever initiated, so SYNs never left and CONNECT timed out. The delivery sweep now files a bounded cooldown-gated fetch on cache miss; the product advance loop fulfills it through standard bounded remote resolution; the sweep requeues (instead of terminating) while the fetch stays fresh; an atomic take-and-refresh closes a drain race where the sweep terminated instead of requeueing. A registry-fallback probes any registry-routed tunnel when bridge/registry id sets diverge, and the lookup first-hop session is ensured like builds. Deterministic table tests pinned.

**Current frontier (one direction complete, reverse open):** i2pr→stock binary STREAM now completes end to end (payload digest-matched). Stock→us times out: stock never sends SYN, and its CONNECT never appears in stock logs. Our LS2 is stored on one floodfill but visibly not replicated mesh-wide within the window, and stock never looks our key up. Also observed: per-run variance is high (bootstrap/inbound/outbound legs each fail closed on different runs under churn; duplicate publications; stock transit noise through us polluting reply correlation). Single-run outcomes stay weak evidence; the remaining work is the reverse STREAM leg plus §6 recovery, not the (now green) build/install/owner/publication machinery.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
| --- | --- | --- |
| Preserve Plan 474 transactional owner publication | Existing Plan 474 owner-generation regressions and status | Retained; not requalified by the failed SAM lane |
| Use a stable Plan 477 build-ID boundary | `DestinationBuildRequest.message_id` is the terminal reply ID; `BridgeHeader::ShortTransport.message_id` is independently generated for the outer header; per-hop IDs are assigned by the runtime-neutral builder | Interface available |
| Compose normal inbound build over an established outbound tunnel | `provision_all_service_router_material` currently direct-submits inbound and defers outbound builds; `RouterDeliveryService` resolves direct authenticated links. Existing `outbound_lookup` TunnelData composition is used for NetDB, not service build creation | Not implemented |
| Compose normal inbound build over an established outbound tunnel | `submit_destination_via_outbound_tunnel` wraps the short build as router-directed `TunnelPayloadHeader` content, transforms it through `OutboundGatewayRole::forward_cells`, and sends bounded TunnelData cells to that role's first hop. Startup now establishes the first outbound role before submitting inbound builds; replacements and remaining outbound builds select a usable role and fail closed if none exists | Implemented at source composition; live interop not yet run |
| Keep a bounded bootstrap exception separate from ordinary Destination construction | The first outbound build of the daemon may use direct delivery when no outbound role exists. It must install before ordinary inbound construction; no route means service startup fails closed. | Implemented in startup ordering; no independent direct inbound fallback |
| Produce genuine nonzero-hop signed LS2, complete owner generation, bidirectional external SAM STREAM and lifecycle recovery | The two Plan 458 attempts stopped at inbound-build before owner summary and STREAM | Not established |

## Verification

| Command | Outcome |
| --- | --- |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd` | Passed before focused daemon tests |
| `cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1` | Passed (4 tests) |
| `cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed (9 tests) |
| `cargo test --locked -p i2pr-daemon --lib exploratory_build -- --test-threads=1` | Passed (14 tests), including the TunnelData first-hop versus build-gateway target check |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed after the final startup ordering and outbound TunnelData changes: 4,864 passed, 39 ignored across 187 suites, including service-product tests |
| `bash scripts/check-service-tunnel-boundaries.sh` | Passed after the outbound role routing change |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | Failed at inbound-build after the new route. The direct bootstrap outbound build installed, then i2pd observed 27 ShortTunnelBuild records / 10 transit-create events and the inbound wait saw 3 expected-peer candidates but no expected message-ID match. No LS2 owner summary or STREAM exchange was reached. |
| `bash tests/integration/m6-interop/run-preflight.sh` | Passed against exact-pinned i2pd 2.61.0; Plan 184 control |
| `bash tests/integration/m6-interop/run-tunnels.sh` | Passed against exact-pinned i2pd 2.61.0; Plan 185 control |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` after Plan 477 changes | Failed at inbound-build on both runs; one run had one observed attempt and no reply match, the next had two attempts with one authenticated reply/install and one timeout |

The M6 controls prove their existing lower-tier contract only. They do not prove outbound-backed service-destination builds or SAM application traffic.

## Invariants and remaining work

No fake lease, zero-hop tunnel, or summary readiness was introduced. Inbound owner publication remains transactional. No public network, support inventory, RouterInfo advertisement, transit, or anonymity claim changed. The external SAM failure did not produce an attributable stock route and cannot justify loosening correlation.

The source path now exposes the outbound role before ordinary inbound service builds and composes the IBGW-targeted request through that role as TunnelData. The pinned lane confirms the first outbound bootstrap installs and the stock network creates transit tunnels, but the subsequent inbound return still fails strict message-ID matching. Remaining work includes source attribution for that return, signed usable leases, complete owner generation, bidirectional binary SAM STREAM, and expiry/replenishment or restart recovery. Do not claim those gates from the local composer test.

## 2026-10-15 timeout-advance rotation landed; forward-leg handshake divergence localized

Source change (this plan's peer-lease-fetch owner, no new plan number):

- `i2pr-netdb/src/lookup_engine.rs`: `advance_after_attempt_timeout` shares
  the `advance_active` body with `handle_pending_after_path`, so a retry
  emits the next unqueried floodfill for the same active lookup; exhaustion
  terminates honestly. No signature changed.
- `i2pr-daemon/src/netdb_seam.rs`: `advance_lease_set2_after_timeout_with_store`
  plus an extracted shared outcome mapping.
- `i2pr-daemon/src/destination_tunnels.rs`: `advance_lease_lookup_after_timeout`
  fails closed on unknown ids, refreshes the pending deadline, and frees the
  slot on exhaustion. A first-cut `begin_lease_lookup_excluding` approach was
  discarded before wiring: re-beginning would collide with the seam's single
  active-lookup slot, while advance reuses it.
- `i2pr-daemon/src/service_product.rs`: resolve attempts after the first
  advance instead of re-sending the same action, with a redacted `rotate`
  stage warn (counts only).
- Regression `lookup_timeout_advance_rotates_floodfill_then_terminates_honestly`:
  two seeded floodfills, distinct peers across advance, honest exhaustion,
  stale id fails closed.

Verification: `i2pr-netdb` 282 passed; daemon lib 764 passed;
`destination_tunnel_unit` 44 passed; `netdb_tunnel_unit` 22 passed; clippy
clean on both crates; fmt clean; runtime and service-tunnel boundary scripts
pass. (Scoped battery; full floor not run.)

One bounded exact-pin lane after the change (pinned i2pd 2.61.0, quiet mesh,
scratch `eunfpd5r` preserved then snapshotted). The rotation did **not** fire:
the forward peer lease fulfilled on attempt 1. New information instead came
from the forward STREAM leg, which failed where earlier runs completed:

- Our CONNECT was received, stock's LS2 fetched and fulfilled in 38 ms, our
  SYNs emitted and retransmitted on a 5 s cadence. Stock accepted immediately
  (`Stream accept`, `STREAM STATUS RESULT=OK`), had stored and updated our LS2
  six seconds earlier, added our remote LeaseSet, then logged `SYNACK packet
  was not ACKed after 1 attempts, terminate` 18 s later while our duplicate
  SYNs kept arriving (`already exists`). Our CONNECT timed out at 60 s with
  zero inbound stream-processing lines on our side. A `Garlic: Incoming
  sessions come too often` throttle warn appears on the accepting peer in the
  same window.
- The first divergence is therefore bounded to the SYN-ACK return: emitted
  (at most once observed) and never ACKed. Two sub-causes remain open:
  transit loss toward our inbound gateway versus silent drop on our ingress/
  streaming path. Our LS2 and leases are proven on the accepting floodfill,
  so this is not a publication or lookup failure.

**Disposition:** no unchanged-topology retry. The next lane run requires the
material change first: redacted inbound-handshake accounting on our side
(SYN-ACK observed at the streaming manager, garlic-to-stream dispatch
outcome), so arrival and processing are distinguished by source-owned counts.
Rotation stays landed and unproven-live; the reverse leg, §6 recovery, and
product close-out remain open.

## 2026-10-15 SYN-ACK ingress accounting landed; streaming discard ruled out

Source change (this plan's handshake owner, no new plan number):

- `i2pr-client/src/streaming/events.rs`: `SynResponseIngressAccounting`
  `{ observed, accepted, rejected }` (counts only) with `delta_since`.
- `i2pr-client/src/streaming/manager.rs`: one counter set per manager,
  incremented at the SYN-response peek classification (`observed`) with
  exactly one of `accepted` / `rejected` on every exit (port-tuple
  mismatch, decode failure, handler outcome). New round-trip test
  (SYN → accept → SYN response → Established records 1/1/0) and
  negative test (SYN-shaped packet with no outbound state records
  1/0/1; data packets leave the counters untouched).
- `i2pr-daemon/src/sam/streams.rs`: `RouterInboundDispatchReport` gains
  `synack_observed` / `synack_accepted`, captured as target-manager
  deltas around the payload drain (server mirror vs canonical honored).
- `i2pr-daemon/src/service_product.rs`: `InboundTrafficSnapshot` gains
  `service_synack_observed` / `service_synack_accepted` plus delta
  support, fed at both router-inbound dispatch sites.
- `i2pr-daemon/src/sam.rs`: CONNECT timeout now emits
  `transition="stream_connect_handshake_wait_expired"` with the wait
  deltas and appends them to the SAM TIMEOUT message (prefix kept).

Verification: `i2pr-client --lib` 115 passed; daemon lib 764 passed;
`destination_tunnel_unit` 44 passed; `i2pr-netdb` 282 passed; clippy
clean on both crates; fmt clean; runtime and service-tunnel boundary
scripts pass; receipt parsers OK; `cargo doc` clean. (Scoped battery;
full floor not run.)

Bounded exact-pin lane with the accounting live (pinned i2pd 2.61.0,
quiet mesh, scratch `lmiggsap` snapshotted). Forward CONNECT failed,
and the timeout message now carries the attribution directly:

`synack_observed=0 synack_accepted=0 synack_rejected=0` over the full
60 s wait, while stock floodfill `f` accepted our SYN three times
(`Incoming stream`, duplicate SYNs `already exists`), held our LS2
(`LeaseSet2 updated` on four peers), emitted a SYN-ACK once, and
terminated it for no-ACK (`SYNACK packet was not ACKed after 1
attempts`). Our outbound retransmits (2×1037B cells every ~5 s) and
our inbound path were stable (`owner_eviction_events=0`, builds
installed); our stdout shows zero garlic/service-inbound processing
lines in the window.

**Disposition:** streaming-layer or ingress-dispatch discard is ruled
out — no SYN-response-shaped packet reached either of our managers.
The loss is bounded to stock-emission → our-inbound-gateway → us
transit (or stale-lease addressing by the peer). The next material
change before any further lane is garlic-level arrival evidence on
the timeout path (product inbound-traffic snapshot deltas: TunnelData
received, owner matches, garlic auth outcomes), which splits "nothing
reached our daemon" from "garlic arrived but ECIES-rejected".

2026-10-15 replication (Plan 477's bounded post-change attempt, scratch
`df2wrjie`): same shape, second consecutive run. Builds/installs fully
green (3 installs, 4/4 replies matched, owners 2/2, no inbound expiry);
forward SYNs reach stock `f` repeatedly; stock emits the SYN-ACK and logs
`SYNACK packet was not ACKed after 1 attempts, terminate,
rSID=1497887931`; our wait records `synack_observed=0`. The divergence
is characterized across runs, not single-run weak evidence — but per
the bounded policy the next lane still moves only on the garlic-level
arrival material change above, not on repetition.

## 2026-10-15 garlic-arrival evidence landed; below-bridge loss proven

Source change (this plan's handshake owner, no new plan number):

- `sam/streams.rs`: `BridgeDiagnostics` gains cumulative outcome
  counters (`garlic_authenticated`, `garlic_rejected`,
  `payloads_dequeued`, `streaming_accepted`) folded from every
  router-inbound dispatch report via `record_dispatch_report`. Legacy
  `inbound_dispatched` / `inbound_observations` call sites untouched.
  New unit test pins all three report shapes (rejected, authenticated
  unattributed, authenticated drained) and the legacy-counter
  non-interference; it fails under a swapped-branch inversion (probe
  reverted, zero markers).
- `sam.rs`: CONNECT timeout snapshots bridge diagnostics before/after
  the wait (`bridge_garlic_totals`, same fail-closed pattern as the
  synack totals) and emits the four deltas in the warn and the TIMEOUT
  message (prefix kept; lane driver captures MESSAGE to the quote, so
  additive keys are safe).

Verification: daemon lib 769 passed; `destination_tunnel_unit` 44;
`sam_stream_self_composed` 4; receipt parsers OK; daemon clippy clean;
fmt/diff clean; runtime + service-tunnel boundaries pass. (Scoped
battery; full floor not run — full floor ran green on the 477 closing
tree two commits back except the two recorded external reds.)

Bounded exact-pin lane with the evidence live (pinned i2pd 2.61.0,
quiet mesh, scratch `1obo_t0p` snapshotted). Third consecutive run of
the same shape, now with the complete owned-layer chain:

`synack_observed=0 synack_accepted=0 synack_rejected=0
garlic_authenticated=0 garlic_rejected=0 payloads_dequeued=0
streaming_accepted=0` over the full 60 s wait, while stock accepted
our SYNs repeatedly (`Incoming stream rSID=2674265016`, retransmits
`already exists`), emitted the SYN-ACK, and terminated it for no-ACK.
Builds/installs green again (3 installs, 4/4 replies, owners 2/2).

**Disposition:** the arrival-vs-processing split is now exhaustive at
every layer this router owns: streaming (synack=0), garlic bridge
(all-zero), with ECIES rejection affirmatively ruled out
(`garlic_rejected=0` — nothing arrived to reject). The loss is below
the bridge: stock→our-inbound-gateway transit, the IBGW→us leg, or
owner routing. TunnelData-received/owner-match granularity stays
product-side (`InboundTrafficSnapshot`, not reachable from SAM state
without a new cross-owner handle) and is the only finer split left;
adding it is a bigger seam change, recorded here, not done here. No
further lane without it or another material change. Reverse STREAM,
§6 recovery, and product close-out remain open; 478 stays active.

Note for the ELS2 line (floor red, not this plan's to fix): the
`check-els2-live-lane-evidence.sh` Plan 393 count guard (needs 3
`.cancel_ls2_publication(request_id)` sites, tree has 1) was bisected
to committed 478 change `5b874e86` (direct-short LS2 publication).
Verified semantically equivalent, not a leak: the single coordinator
entry is created at `begin_ls2_publication` and unconditionally
cancelled after the direct send on every outcome; earlier failures
return before any entry exists; the direct helper creates no entry.
The guard's literal count is stale for the restructured code — updating
it to assert the semantic property belongs to the ELS2 owner with its
own plan-of-record. Fix code, never weaken the script; no contrived
call-site duplication was added to game the count.

**Roadmap disposition:** active; no dependent capability is unblocked.

## 2026-10-15 §6 below-bridge arrival mirrors landed; lane stopped pre-STREAM under churn

Source change (this plan's product/SAM seam owner, no new plan number):

- `service_tunnels.rs`: `ServiceTunnelManager` gains five lock-free
  `AtomicUsize` arrival mirrors (`inbound_tunnel_data_received`,
  `inbound_tunnel_data_cells_accepted`,
  `inbound_garlic_owner_matches`, `inbound_garlic_owner_misses`,
  `inbound_direct_garlic_received`) with `note_*` methods and an
  `inbound_arrival_totals` snapshot, following the established
  `note_inbound_orphan_receive` pattern. No lock, no allocation,
  no identity or payload material.
- `service_product.rs`: the four existing inbound update sites
  (TunnelData observed, cell accepted, owner match/miss, direct
  garlic) mirror into the manager alongside the `inner` fold.
  Ownership unchanged: the product stays the sole inbound owner;
  the manager only counts.
- `sam.rs`: CONNECT timeout snapshots the attached manager's
  arrival totals before/after the wait
  (`product_inbound_arrival_totals`, fail-closed to zeros when no
  manager is attached) and emits the five deltas in the warn and
  the TIMEOUT message (prefix kept; driver captures MESSAGE to
  the quote, so additive keys are safe).
- New unit test `plan478_inbound_arrival_mirrors_accumulate` pins
  per-counter independence and legacy-counter non-interference;
  it fails under a wrong-counter inversion (probe reverted, zero
  markers).

Verification: daemon lib 770 passed; `destination_tunnel_unit` 44;
`sam_stream_self_composed` 4; receipt parsers OK; daemon clippy
clean; fmt/diff clean; runtime + service-tunnel boundaries pass.
(Scoped battery; full floor last ran green on the 477 closing
tree except the two recorded external reds.)

Bounded exact-pin lane with the mirrors live (pinned i2pd 2.61.0,
quiet-mesh gate passed, scratch `vilzzd8q` snapshotted). This run
stopped **before** STREAM at `inbound-build`: bootstrap needed one
retry, then two inbound waits expired with peer-matching but
zero message-ID matches (`candidate_message_id_matches=0`,
orphans 17/25 with `peer_match` but `message_match=0`). Mesh was
hot despite the quiet gate (71 short-build records, 235 transit
inventory delta, 30 transit creates, 7 creator-RI store
requests): the recorded churn signature — stock transit builds
routed through us during the wait polluting correlation. The new
arrival keys were not exercised (no STREAM attempt); they stay
covered by unit test plus fail-closed defaults and will report
on the next run that reaches the forward leg.

**Disposition:** no defect, no fix, no second run on this
topology — the outcome is the known variance arm, and the
bounded policy spends attempts on changed causal paths only.
The arrival-vs-processing chain is now complete end to end for
the next STREAM-reaching run: manager TunnelData/cell/owner/
direct counters → bridge garlic outcomes → streaming SYN-ACK
accounting. Reverse STREAM, §6 live recovery, and product
close-out remain open; 478 stays active.

## Closure checklist (pass gate, refreshed 2026-10-15)

| Criterion | State |
| --- | --- |
| Normal router-owned exploratory→destination build route (outbound-backed TunnelData, one explicit direct bootstrap, Plan 482 dial authority consumed) | Implemented + live-proven across runs; deterministic target/generation tests green |
| Genuine nonzero-hop signed LS2 + stable complete owner registry (N/N leases, transactional generation, no aliasing) | Live-proven (3 installs, 4/4 replies, owners 2/2 across replicated runs); deterministic owner/lifecycle tests green |
| Forward external SAM STREAM (i2pr→stock binary, digest-matched) | Proven once end to end; later runs vary at build/STREAM legs under churn |
| Reverse external SAM STREAM (stock→us, SYN-ACK return completes) | Open, externally blocked: 3 replicated runs show stock emitting the SYN-ACK with zero arrival below our bridge; owner-routing vs transit loss now instrumented to the finest layer this router owns |
| §6 lifecycle recovery (tunnel expiry/replenishment or controlled peer loss + daemon restart) | Deterministic rotation/retry/replenishment-order tests green; live proof gated behind a working return leg |
| Executed lifecycle negative controls (bootstrap-exception refusal, wrong-terminal/generation/damaged-garlic fail-closed, cancellation-to-baseline, no cross-destination aliasing) | Deterministic tests green (`inbound_route_miss_fails_closed_and_bootstrap_remains_explicit`, `replacement_builds_use_the_existing_outbound_route_first_hop`, 477 five-way taxonomy + cancel/replay regressions, 474 owner-generation tests) |
| Full AGENTS.md floor on the closing tree | Open: scoped batteries green; full floor + ELS2 Plan 393 handoff belong to the closing pass, not this instrumentation turn |

## 2026-10-15 handshake-wait receipts machine-readable; bounded lane rerun

Source change (this plan's lane-evidence owner, no new plan number):

- `tests/integration/core-router-recovery/run_plan458_sam_product.py`:
  `parse_attempt_receipt_lines` now extracts
  `stream_connect_handshake_wait_receipts` — the full 12-key
  attribution (synack ×3, bridge garlic ×4, manager arrival ×5) from
  the daemon TIMEOUT warn — into `attempt-diagnostic.json`. A line
  missing any key yields no receipt (same fail-closed shape as the
  existing inbound-wait receipts); the daemon warn carries no
  `build_attempt_id`, so it contributes no stage count by design.
- `tests/integration/core-router-recovery/test_plan478_handshake_wait_receipt.py`
  (new, 4 tests): full-12-key parse, missing-key refusal,
  ANSI-colored lines, counts-only shape. All lane receipt suites
  green (`test_plan464/474/478/481`).

This is the material change licensing the next bounded lane: the §6
mirror keys have never reported on a STREAM-reaching run (run
`vilzzd8q` stopped at `inbound-build`), and they are now
machine-readable in the diagnostic. One exact-pin run authorized; no
unchanged-topology retry beyond it. Outcome recorded below.

## 2026-10-15 arrival proven, owner-miss localized (run `51yb1g49`)

Bounded exact-pin lane with the machine-readable receipts live
(pinned i2pd 2.61.0, `mesh_quiet: true`, scratch `51yb1g49`
snapshotted to `target/interop/plan458-scratch-snaps/` with its
`attempt-diagnostic.json`). Builds/installs fully green again (4/4
replies matched, 3 installs, owners 2/2). The forward CONNECT timed
out, and its wait receipt is the first below-bridge arrival ever
recorded:

`synack 0/0/0, garlic 0/0/0/0, tunnel_data_received=4,
cells_accepted=1, owner_matches=0, owner_misses=3, direct=0`

**Disposition:** the no-arrival theory is dead — 4 TunnelData
messages reached the daemon and 1 cell was accepted, but zero
envelopes resolved to a service owner. The loss is now bounded to
owner routing: the return arrives addressed to tunnel ids with no
registered destination owner (stale lease, own-exploratory traffic,
or foreign addressing — previously indistinguishable). Streaming,
bridge, and ECIES theories stay ruled out. Per the bounded policy,
the next lane moves only on the taxonomy material change below, not
on repetition.

## 2026-10-15 owner-miss taxonomy landed (this plan's miss owner)

Source change (no new plan number):

- `service_tunnels.rs`: `InboundOwnerMissClass`
  (`ExploratoryKnown`/`RetiredService`/`Unknown`) with a pure
  `classify` (exploratory wins over retired, retired over unknown);
  three class counters beside the aggregate (class counts always sum
  to it); bounded retired-id memory (`MAX_RETIRED_RECEIVE_IDS=64`,
  ephemeral local ids only, never logged/exported, never a decision
  input) fed by both unregister paths and cleared on re-register.
- `service_product.rs`: the miss arm classifies from already-held
  registries (exploratory endpoint table, retired memory) and folds
  the class into `InboundTrafficSnapshot` (3 new fields + delta
  arms); `classify` keeps the arm branch-free.
- `sam.rs`: CONNECT timeout captures the taxonomy deltas and emits
  `miss_exploratory_known/_retired_service/_unknown` in the warn and
  the TIMEOUT message (prefix kept).
- Lane parser: the handshake-wait receipt grows 12→15 keys with a
  sum-check (`exploratory+retired+unknown == misses`).

Verification: taxonomy sum/cap/roundtrip/clear/eviction test,
`classify` 4-arm test (arm-swap inversion fails, probe reverted),
snapshot delta test extended, mirror accumulation green; fmt/clippy/
service-tunnel + runtime boundaries pass. (Scoped battery; full
floor belongs to the closing pass.)

Next: one bounded exact-pin lane reading the taxonomy; then §6 live
recovery behind a working return leg, or the next causal step the
taxonomy dictates. Reverse STREAM, §6 recovery, and product
close-out remain open; 478 stays active.

## 2026-10-15 taxonomy reads exploratory; stock-side forensics

Run `ou1v6wii` (taxonomy live, `mesh_quiet: true`, snapshotted with
diagnostic): builds/installs/owners green again (4/4 replies, 3
installs, owners 2/2); forward CONNECT timed out with

`synack 0/0/0, garlic 0/0/0/0, received=6, cells=2, matches=0,
misses=4, direct=0, exploratory=4, retired=0, unknown=0`

(classes sum to the aggregate ✓). Both taxonomy runs replicate:
every miss is own-exploratory traffic, none is a stale lease.

Offline forensics on the preserved snaps (counts and shapes only,
no id material retained) kill two theories and bound the third:

- Stale-lease addressing is dead: `retired=0`, no replenishment or
  replacement lines anywhere in the run, and the timeline is tight
  (installs :46:56–58, LS2 published :46:58, CONNECT :47:07,
  SYN-ACK ~:47:07, timeout :47:42) — the leases were seconds old.
- Lease/gateway mismatch is dead by construction: the LS2 compose
  path (`i2pr-client/src/leaseset.rs::evaluate`) rejects any
  advertised lease that is not an owned `InboundLeaseSource`
  (`ForeignLease`), so the published leases provably named owned
  gateways and receive ids at publish time.
- What remains is stock-send-side: stock `f` accepts our SYNs
  repeatedly (:07/:12/:17/:22, plus a new-rSID stream at :27),
  re-resolves our lease mid-wait (:07 and :16 `RemoteLease
  changed`), emits exactly one SYN-ACK attempt, terminates it for
  no-ACK at :25 — while a `Garlic: Incoming sessions come too
  often` throttle warn fires on `f` in the accept second (:07) and
  no SYN-ACK send trace appears in any of the four peer logs. Our
  IBGW→us leg demonstrably carries cells (exploratory arrivals),
  and `f` holds outbound tunnels throughout, so the plausible
  locus is `f`'s send (suppressed once, concurrently with its
  throttle) rather than our reception.

Next bounded experiment (material change, in-plan): fresh-handshake
recovery in the lane driver — after a zero-service-arrival forward
timeout, one fresh STREAM CONNECT (new stream/SYN ids after the
peer-side termination, not a retry of the same send) tests whether
the loss was transient (throttle window) or structural. Bounded to
exactly one fresh attempt on the forward leg; reverse keeps
single-attempt shape; both attempts land in the evidence JSON.
`plan458_sam_driver.py::run_forward_leg_with_recovery` plus
`test_plan478_driver_recovery.py` (3 tests: no-retry on success,
exactly-once on first failure, raise after double failure).

## 2026-10-15 fresh-handshake recovery fails identically (run `v7ap08nx`)

Bounded lane with the recovery live (pinned i2pd 2.61.0,
`mesh_quiet: true`, snapshotted with diagnostic). Builds/installs/
owners green again (4/4 replies, 3 installs, owners 2/2). Two
handshake waits, both timed out:

- Attempt 1: `received=6, cells=2, matches=0, misses=4 exploratory,
  synack 0` — the now-familiar shape; recovery triggered.
- Attempt 2 (fresh CONNECT, new stream/SYN): `received=0, cells=0,
  misses=0, synack 0` — absolute zero, not even exploratory traffic.

Stock-side (counts only): first SYN accepted :27 with the recurring
`Garlic: Incoming sessions come too often` throttle warn in the same
second, SYN-ACK #1 terminated :45 for no-ACK; the fresh SYN accepted
:46 as a new stream, SYN-ACK #2 terminated :04 for no-ACK. Two
independent handshakes, two stock sends, zero arrival both times —
while build replies and exploratory cells demonstrably flow down our
tunnels and `f` holds outbound capacity throughout.

**Disposition — live campaign concluded for this pass.** Five
arrival-characterized runs plus one recovery run, all identical in
shape. Our-side hypotheses are exhausted with evidence: builds,
installs, owners, LS2 publication, forward delivery, inbound
dispatch, taxonomy (stale-lease dead by `retired=0` + tight
timeline; gateway-mismatch dead by the `ForeignLease` compose
self-check; transient/throttle dead by the fresh-handshake twin
failure). What remains is stock-send-side (single SYN-ACK attempt
with no send trace in any peer log, concurrent with `f`'s
per-accept garlic throttle) — not addressable from this tree, and
no our-side material change remains to license another lane. 478
stays active with reverse STREAM and §6 open; the next step needs a
genuinely new causal hypothesis (new plan-of-record), not another
attempt. No dependent capability is unblocked; 433/439 gates
unchanged.

## 2026-10-15 pinned-source forensics: f HAS our lease and sends (run `ou1v6wii` re-read)

Read-only archaeology against the exact pinned i2pd commit
(`635b013a`, fetched to scratch, never vendored) plus the
preserved snaps, answering where the SYN-ACK dies:

- `oilmu74...` is f's own SAM peer destination (its LS2
  republishes at :47:06/:47:36 on c/f/n), not ours. Our LS2
  (`gwbun5...`) was stored only on `d` at :46:58 (+ :47:49) —
  f's NetDb never holds it, by design.
- At :47:07 f receives TWO garlics on separate tunnels: a 662B
  block carrying a DatabaseStore of OUR LS2 (`Remote LeaseSet /
  New remote LeaseSet added` — our daemon garlic-sends its LS2
  to the peer destination alongside the SYN, working as
  designed) and a 580B block carrying the SYN (`Incoming stream`,
  `RemoteLease changed`, `OutboundTunnel changed`, `SAM:
  Incoming I2P connection`). f HAD our current lease.
- f's send path (`Stream::SendPackets`) queues the SYN-ACK via
  `m_CurrentOutboundTunnel->SendTunnelDataMsgs` to
  `(our-IBGW, our-T_gw)` with no error branch taken (no `No
  outbound tunnels`, no `Remote lease is not available`, no
  `All remote leases are expired` anywhere in-window). The
  `SYNACK ... terminate after 1 attempt` fires only because the
  stream has no shared garlic routing path — i.e. the ACK never
  came back, not because the send failed.
- The `Garlic: Incoming sessions come too often` throttle is
  inbound-only (drops a third same-second new ECIES session
  message, after the SYN was fully processed) — it cannot block
  f's sends. Throttle theory dead.
- So the loss is bounded to f's out-queue → our-IBGW transit,
  our IBGW handling, or our owner dispatch — with f's send
  proven queued and our IBGW→us leg proven working for other
  cells.

## 2026-10-15 taxonomy vacuity found and fixed (this plan's miss owner)

Code proof (no lane needed): the dispatch registry is SHARED —
exploratory pool roles and destination roles activate into the
same `DataPlaneRegistry` (`activate_destination_tunnel` writes
only the registry, never the exploratory pool). The old
`exploratory_known` arm tested that shared registry, and every
dispatch-accepted cell is registry-known by construction — so
the old label was tautological for accepted misses. The prior
"every miss is own-exploratory traffic" conclusion is retracted
as unproven; stale-lease stays dead (`retired=0` + tight
timeline hold independently).

Rework (material change licensing the next bounded lane):

- `exploratory_build.rs`: `DESTINATION_ROLE_SLOT_BASE`
  (`0x8000_0000`, single source of truth, replaces the init
  literal) + `is_destination_role_slot` (exploratory pool slots
  allocate from zero; disjoint by construction) +
  `exploratory_pool_holds_inbound` (tests the pool, never the
  shared registry). Namespace-boundary unit test pinned.
- `service_tunnels.rs`: new `ServiceUnowned` class — id bound
  to a live destination role slot but with no live service
  owner (owner-table divergence alarm, never expected
  nonzero). Precedence retired > service-unowned >
  exploratory > unknown; 8-arm test (arm-swap probe kills,
  reverted); counter + 4-tuple totals; sum test extended.
- `service_product.rs`: miss arm computes
  `destination_role_bound` (registry slot namespace) and
  genuine `exploratory_pool_known`; snapshot field + delta
  arms; delta test extended.
- `sam.rs`: TIMEOUT warn + message gain
  `miss_service_unowned` (additive, prefix kept).
- Lane parser: receipt grows 15→16 keys with a 4-class
  sum-check (`service+exploratory+retired+unknown == misses`);
  receipt tests (parse, sum-mismatch refusal, ANSI,
  counts-only).

Verification: daemon lib 779 passed (`--test-threads=1`;
one parallel-run flake, green on both reruns);
`destination_tunnel_unit` 44; `sam_stream_self_composed` 4;
receipt suites 8; clippy clean; fmt clean; service-tunnel +
runtime boundaries pass. (Scoped battery; full floor belongs
to the closing pass.)

Next: ONE bounded exact-pin lane reading the fixed taxonomy.
`service_unowned > 0` proves our-side divergence (fix under
478); all-exploratory proves background and bounds the loss
upstream of our daemon (record, no further lane without a new
hypothesis). Reverse STREAM, §6 recovery, and product
close-out remain open; 478 stays active.

## 2026-10-15 lane reads `service_unowned=1` (run `msmizqup`)

Bounded exact-pin lane with the fixed taxonomy live (pinned
i2pd 2.61.0, `mesh_quiet: true`, scratch
`msmizqup-service-unowned-run` snapshotted with diagnostic).
Builds/installs/owners green (4/4 replies, 3 installs, owners
2/2, no evictions). Attempt-1 wait receipt:

`tunnel_data_received=1, cells_accepted=0, matches=0,
misses=1, service_unowned=1, exploratory=0, retired=0,
unknown=0`

The alarm fired on its first live run: a garlic envelope
arrived on a tunnel id bound to a LIVE destination role slot
with no service owner — not background, not stale, not
foreign. Attempt 2 (fresh recovery CONNECT) ended in 35 ms
with all-zero: our daemon failed fast (no forward path; f
never saw a second SYN), so the recovery leg contributed no
information this run.

Timeline: installs :46:24 (owners coherent:
`owners_table_len=2 any_key_present=true`), CONNECT :46:30,
peer LS2 fetch fulfilled :47:00, f accepted :47:00, our wait
expired :47:05 — the SYN-ACK had ~5 s and one cell made it,
dropped by our owner table.

## 2026-10-15 root cause: creator/local id namespace swap in the advance loop

Code-proof (no lane needed), three distinct allocator-drawn
ids per inbound tunnel confirmed in `short.rs` + allocator:

- LS2 advertises the gateway id (`first_hop_receive_tunnel`).
- Registry roles + owner table + bridge key on the LOCAL
  endpoint id (`local_inbound_receive`).
- Destination pool registrations key on the CREATOR id
  (`register_inbound_with_material` uses
  `established.creator_tunnel_id()`).

The advance loop built its refresh/eviction id lists from
`registration.tunnel_id()` (creator) and fed them to owner and
bridge state (local): `owners_changed` was therefore ALWAYS
true, so the first poll after every install inserted creator
ids as owners, direct-removed the real local owners (without
retiring — `owners.remove`, not unregister), and swapped the
bridge projection to creator ids. The summary stayed masked
(`any_key_present` compares bridge vs owners — both creator
after the swap). Every later service arrival hit a live role
with no owner: total inbound deafness from seconds after
install, in every run. The eviction path had the same
confusion (removing creator ids that own nothing, leaving
role + owner + bridge behind).

Fix (this plan's product owner, no new plan number):

- `i2pr-client/src/pool.rs`: `DestinationTunnelPool` keeps a
  durable slot→local map, maintained on every insert and every
  removal path (register, zero-hop, mark_failed, remove,
  advance_time evictions, release_all) + `inbound_local_receive`
  accessor; `registry.rs` facade. Pool tests (map returns
  local ≠ creator on genuine short-build fixtures; cleared on
  all four removal paths).
- Daemon advance loop: eviction resolves slots to locals
  (fail-closed honest receipts when unresolvable); refresh
  list built by pure `local_inbound_receive_ids` (live
  registrations → locals minus evicted; `None` on any gap →
  tick skips refresh+replenishment, error-logged, practically
  unreachable); helper unit-tested incl. fail-closed +
  eviction-skip arms.
- `refresh_service_router_pool_snapshot_with_owners`:
  dropped ids now retire into taxonomy memory (lock order
  matches unregister); stability no-op + retire tests pinned.
- Probes: creator-fallback in helper, retire removal, map
  insert removal — each kills its test (all reverted, zero
  markers).

Verification: daemon lib 783 passed (`--test-threads=1`);
i2pr-client lib 117 passed; `destination_tunnel_unit` 44;
`sam_stream_self_composed` 4; receipt suites 8; clippy clean
both crates (one missing `#[cfg(test)]` caught and fixed);
fmt clean; service-tunnel + runtime + dependency boundaries
pass. (Scoped battery; full floor belongs to the closing
pass.)

Next: ONE bounded exact-pin lane with the fix, expecting
owner matches and handshake progress. The persistent upstream
cell loss (most SYN-ACK sends never arriving) is a separate,
still-open observation — this fix removes our self-inflicted
drop so the next lane measures only that.

## 2026-10-15 fix verified: forward full pass + reverse handshake (run `qr06eib6`)

Bounded exact-pin lane with the fix live (pinned i2pd 2.61.0,
`mesh_quiet: true`, scratch `qr06eib6-handshake-complete-run`
snapshotted). Builds/installs/owners green (4/4 replies, 3
installs, owners 2/2, no evictions, no handshake-wait
receipts at all):

- FORWARD leg (i2pr→stock) completed end to end, 2328-byte
  echo digest-matched — first forward full pass since the
  refresh path existed.
- REVERSE leg (stock→us): stock's CONNECT got `STREAM STATUS
  RESULT=OK`; our SAM ACCEPT bound the inbound stream (no
  wait-expired). Reverse HANDSHAKE completed — first time ever.
  Owner deafness is fixed.

New frontier — reverse DATA: stock resent its 2328-byte
payload 10 times over ~100 s (cycling both our leases and its
tunnels, `terminate after 10 attempts` at :22:17) while our
accept worker read EOF (0/2328); our daemon closed our SAM
socket with no error/warn logged. Forward echo arrived in
the same window, so f→us delivery works in principle.

Standing hypotheses for the next material change (NOT
conclusions — the current evidence cannot split them):

1. Payload cells never arrived (upstream/lease/session
   scope, same family as the old SYN-ACK loss; note f's
   forward-stream resends also went unacked, so us→f ACKs
   were lost too).
2. Cells arrived but our streaming/server-side path dropped
   them silently (no data-phase attribution is emitted on
   the ACCEPT path — handshake attribution exists only on
   CONNECT timeout — so silence proves nothing).

What the evidence rules out: owner-table drops (matches
flow; no misses logged), LS2/lease staleness (f cycled both
leases), our ACCEPT plumbing direction (server profiles feed
the receiver mirror end to end, code-verified), our ACK
generation as the zero-byte cause (2328 B fits the initial
window; first flight needs no ACKs).

Next material change (in-plan): data-phase attribution on
the ACCEPT path — per-accepted-stream ingress counts
(dequeued/accepted/rejected + close reason, counts-only) and
surfacing of accept-worker failures into the lane
diagnostic. That makes the next lane decisive between (1)
and (2). No further lane without it. Reverse STREAM data,
§6 recovery, and product close-out remain open; 478 stays
active.

## 2026-10-15 material change: ACCEPT data-phase attribution (no lane yet)

Implemented the in-plan change the `qr06eib6` frontier
requires. Per-accepted-stream ingress accounting plus the
close reason, counts-only throughout:

- `i2pr-client` (`streaming/events.rs`, `manager.rs`,
  `streaming/mod.rs`): new `DataPacketIngressAccounting
  { observed, accepted, rejected, payload_bytes_admitted }`
  with `delta_since`, mirroring the SYN-response arm.
  `process_inbound_packet`'s established-traffic branch notes
  observed at peek classification and exactly one of
  accepted/rejected on every exit (unknown connection,
  port-tuple mismatch, decode failure, handler outcome —
  including a provably-unreachable lookup arm kept counted
  so the invariant holds structurally).
  `handle_data_packet` counts the arriving packet's wire
  payload once on receive-window admission (Delivered or
  Buffered; duplicates, plain ACKs, terminated: nothing).
  7 new manager tests (accept/unknown/tuple/buffered/
  close-without-signature/handshake-isolation/delta); the
  accepted-arm probe kills and is reverted with zero
  markers. Client lib 117 → 124 green.
- `i2pr-daemon` (`sam.rs`, `sam/raw_stream.rs`):
  `RawStreamCleanup` carries an `AcceptDataPhaseBaseline`
  (bridge garlic 4-tuple + receiver-mirror and canonical
  data ingress) captured at the command→raw handoff for
  inbound streams only (ACCEPT command path + both FORWARD
  sites); outbound CONNECT closes stay unattributed.
  `finish_raw_stream` emits
  `transition="stream_accept_data_phase_closed"` with 15
  counts-only keys (mirror 4 + canonical 4 + bridge 4 +
  `connection_state_code`/`undrained_bytes`/`reset`) BEFORE
  removing the connection. State code: 0 absent (removed
  out from under the pump), 1 Established, 2 closing,
  3 closed/reset, 4 handshake states — exhaustive over the
  7 `ConnectionState` variants. New lib test pins the
  no-baseline and missing-bridge fail-closed arms (probe
  kills, reverted). Daemon lib 783 + self-composed 4 +
  tunnel-unit 44 green; clippy/fmt clean.
- Lane plumbing: parser gains
  `stream_accept_data_phase_close_receipts` (15 keys,
  fail-closed on either manager's
  accepted+rejected≠observed); new offline receipt suite
  (12 tests: full line, both invariant refusals, missing
  keys, ANSI, counts-only, handshake/close independence,
  sidecar round-trip/bound/schema/leg/range); driver
  accept worker records elapsed-to-failure (fast EOF vs
  timeout EOF) and `main()` prints per-leg start/fail
  lines plus a `plan458-sam-leg-failure-v1` sidecar the
  lane merges into `attempt-diagnostic.json` (fail-closed
  allowlist reader). Core-recovery offline suite 36
  green; planning suite 51 green.

Decision table for the next lane (read the reverse-leg
close receipt + the driver sidecar):
- `data_observed == 0` → payload never arrived (upstream
  scope; hypothesis 1). Canon zeros confirm no path
  confusion.
- `data_observed > 0, data_accepted == 0` → arrived but
  our manager rejected everything (hypothesis 2,
  streaming path).
- `data_accepted > 0, data_payload_bytes == 0` → only
  control arrived (ACK/CLOSE), no payload.
- `data_payload_bytes > 0` with lane 0 bytes →
  admitted but never reached TCP: split by
  `connection_state_code` (1 = pump exited while peer
  alive; 0 = connection removed under the pump) and
  `undrained_bytes` (>0 = never drained).
- `canon_data_* > 0` on an ACCEPT close → unexpected
  path (data landing on the wrong manager).

## 2026-10-15 lane `sy7cqc9a`: forward CONNECT fails before the frontier (plumbing live-proven)

Bounded exact-pin lane with the attribution live (pinned
i2pd 2.61.0, `mesh_quiet: true`, scratch
`sy7cqc9a-forward-zero-arrival-run` snapshotted). Builds
were rockier than `qr06eib6` (2 inbound-build waits
expired with orphans peer-matching but message-missing,
`bootstrap_retry: 1`, `inbound_retry: 2`; still 3 installs
committed, owners 2/2, no evictions). The forward leg
failed on BOTH attempts before any ACCEPT existed, so the
reverse-DATA frontier was not reached:

- Attempt 1: CONNECT :21:43, peer-LS2 fulfillment took
  30 s (:22:13), 8 SYN cells composed and tunnel-delivered,
  then a `delivery_failed=1` sweep at :22:18 removed the
  SYN connection and the wait exited at 35 s (NOT the 60 s
  deadline) with all-zero attribution.
- Attempt 2 (fresh-handshake recovery): CONNECT :22:23,
  `delivery_failed=1` at :22:23.28, wait exited in 55 ms
  with all zeros — the `msmizqup` fast-fail shape.
- f's log shows no incoming SYN at all (only its own
  parked SAM ACCEPT + session teardown), despite our
  `delivered=8`.

The new plumbing is live-proven by this failure:
`driver_leg_failure` (`i2pr-to-i2pd`, 40.2 s, CONNECT
TIMEOUT) merged into `attempt-diagnostic.json` via the
sidecar; `stream_accept_data_phase_close_receipts: []`
parsed correctly alongside 2 handshake-wait receipts —
old formats unbroken.

What this run measures (NOT conclusions):
1. `wait_for_established` returns false immediately on a
   removed connection (`sam.rs` early-exit arm), so a
   delivery-side removal reads as a handshake timeout with
   all-zero ingress. The 35 s / 55 ms exits (vs the 60 s
   deadline) are the readable signature — the sidecar
   elapsed makes it visible for the first time.
2. Our-side `deliver_outbound → bridge_to_peer` failure
   for the SYN is a first-order open: `delivered=8` at
   :22:13, then `delivery_failed=1` at :22:18 and again
   at :22:23. The sweep counter conflates LS2/install/
   bridge causes and the typed reason is debug-gated, so
   the cause is unrecorded. Outbound-role churn between
   :13 and :18 is one candidate; the refresh path from
   the creator/local fix is inbound-scoped and qr06eib6
   passed forward with it, so it is not the suspect —
   but the suspicion is recorded, not assumed away.
3. The 8 tunnel-delivered SYN cells never appeared in
   f's log: tunnel-path loss downstream of our
   `bridge_to_peer Ok` is also still open (same family
   as the old SYN-ACK loss, now on the forward leg).

Next material change (in-plan, no lane until it lands):
split `delivery_failed` by typed cause (LS2 vs install
vs bridge-subcause, counts-only) so a SYN-send failure
attributes like the handshake wait does. Reverse STREAM
data, §6 recovery, and product close-out remain open;
478 stays active.

## 2026-10-16 send-side cause-split landed (this plan's product/SAM seam owner)

Source change (no new plan number):

- `sam/fabric.rs`: `DeliverySweepCounters` gains four split
  fields (`delivery_failed_ls2_missing/_ls2_invalid/
  _ls2_install/_bridge`); new `SendFailureCause` enum +
  `note_split_failure` (aggregate + exactly one split arm,
  saturating) + `split_failure_sum`; `saturating_add_assign`
  and `clean` cover the new fields. Two unit tests (4-arm
  exactness + accumulate/clean); the arm-swap probe kills
  and is reverted with zero markers.
- `sam/raw_stream.rs`: the four `deliver_outbound`
  `delivery_failed` sites now call `note_split_failure`
  with `Ls2Missing` (peer holds no LS2), `Ls2Invalid`
  (validation), `Ls2Install` (install), `Bridge`
  (`bridge_to_peer` seam). Factory/unknown-peer arms
  untouched.
- `service_tunnels.rs`: product-sweep literal gains four
  zeros (SAM-path only; aggregate increments there keep
  the sum invariant trivially).
- `sam.rs`: new `delivery_send_totals` (9-tuple, fail-closed
  zeros, `u64::try_from` widening); CONNECT timeout
  captures before/after deltas and emits 9 additive keys
  (`send_delivered/_unknown_peer/_missing_factory/
  _factory_exhausted/_delivery_failed/
  _failed_ls2_missing/_ls2_invalid/_ls2_install/_bridge`)
  in the warn and the TIMEOUT message (prefix kept).
  `run_destination_driver` degradation debug carries the
  split fields. New lib test pins the helper mapping
  (seeded counters + absent-destination zeros); the
  tuple-swap probe kills and is reverted.
- Lane parser: handshake-wait receipt grows 16→25 keys with
  a second sum-check (split sum == `send_delivery_failed`);
  pre-split 16-key lines now refuse fail-closed (recorded,
  same shape as the 12→15→16 transitions). Receipt tests:
  full-25 parse, both sum-mismatch refusals, legacy
  refusal, ANSI, counts-only; the accept-data independence
  line updated to 25 keys.

Verification: daemon lib 787 passed (`--test-threads=1`;
784 + 2 fabric + 1 totals); `destination_tunnel_unit` 44;
`sam_stream_self_composed` 4; `i2pr-client` lib 124;
core-recovery offline 38 (36 + 2 new); planning 51;
clippy `-D warnings` clean (one `field_reassign_with_
default` caught and fixed); fmt clean; service-tunnel +
runtime boundaries pass. (Scoped battery; full floor
belongs to the closing pass.)

Next: ONE bounded exact-pin lane. Read the forward
handshake-wait receipt first: the send keys name the
owner immediately if forward fails again
(`send_failed_bridge` vs LS2 arms vs zeros-with-delivery).
Only if forward passes does the reverse close receipt
become reachable. Reverse STREAM data, §6 recovery, and
product close-out remain open; 478 stays active.

## 2026-10-16 lane `0h6f38y_`: pre-STREAM stop, match-without-install (variance arm)

Bounded exact-pin lane with the split live (pinned i2pd
2.61.0, `mesh_quiet: true`, scratch
`0h6f38y_-inbound-build-no-install-run` snapshotted).
Stopped at SESSION CREATE (`inbound-build`), 0 installs —
the send-split keys were not exercised (no STREAM
attempt; they stay unit-tested):

- Outbound bootstrap reply `authenticated_reply_matched`
  with NO `install_committed` (match-without-install —
  first time observed; all prior runs installed on
  match), then 2 all-emitted inbound submits, both waits
  expired with zero peer/message matches at every level
  (orphans, gateway, candidates).
- Stock mesh showed normal background (13 build records,
  7 transit creates, 0 decrypt failures, 0 rejections,
  creator RI 4/4). Run-to-run build variance is the
  standing characterization (clean → rocky → failed
  across the last three runs); this tree's build/install
  paths are untouched by the split change (counters +
  warn/message keys + parser/driver only), and
  deterministic suites stayed green, so no product fix
  is indicated by a pre-STREAM stop.
- Evidence defect found live and fixed (in-plan,
  lane-evidence owner): the previous run's driver
  leg-failure sidecar (`result.json.leg-failure.json`)
  is NOT cleaned at lane startup, so this run's
  diagnostic merged the `sy7cqc9a` sidecar (40.2 s
  forward CONNECT TIMEOUT) as its `driver_leg_failure`.
  Fixed via shared `driver.sidecar_path` helper +
  `clean_lane_evidence` covering the sidecar, with
  stale-sidecar-removal and path-parity tests (removal
  probe kills, reverted). Note for archaeology: the
  `0h6f38y_` diagnostic's `driver_leg_failure` is stale
  (belongs to `sy7cqc9a`); the stale file has since been
  removed from the evidence dir.

**Disposition:** known variance arm (cf. `vilzzd8q`):
no defect, no product fix, no unchanged-topology retry
on this outcome alone. The split keys still await their
first live report.

## 2026-10-16 lane `djv2qtdv`: frontier re-reached, close receipt lost to a lane/daemon race

Bounded exact-pin lane with split + sidecar-clean live
(pinned i2pd 2.61.0, `mesh_quiet: true`, scratch
`djv2qtdv-reverse-data-no-close-receipt-run`
snapshotted). Builds/installs fully green again (4/4
replies, 3 installs, owners 2/2, no build waits) —
variance confirmed, no systematic build regression.
Forward leg passed with no timeout (send-split keys
correctly absent: they emit only on CONNECT timeout).
Reverse leg reached the `qr06eib6` frontier exactly:
driver `short stream read (0/2328)` after 99.0 s on
`i2pd-to-i2pr` (fresh sidecar — the startup clean is
verified live, no stale carryover). But
`stream_accept_data_phase_close_receipts: []` — our
stdout (91 lines) ends at the forward SYN sends with
zero inbound lines and no close line.

Root cause (design gap in the `dc81898` attribution,
no lane needed to see it): the daemon emits the close
receipt when its raw pump FINISHES (`finish_raw_stream`
runs even on cancellation), but in the accept-timeout
shape the pump is still alive — blocked in read — when
the driver gives up. The lane then writes the
diagnostic milliseconds after the driver subprocess
exits and SIGTERMs the daemon in `finally`, so a
receipt that would flush on socket close is lost. The
missing receipt is a timing artifact, not a product
finding; the decision table has still never fired.

Fix (in-plan, lane-evidence owner): bounded late-flush
grace in the lane's failure path — if the sidecar shows
a started reverse leg (`i2pd-to-i2pr`), poll the
still-running daemon's stdout up to 30 s for the close
marker before snapshotting the diagnostic, and record
`accept_close_wait_s` / `accept_close_found`. Any other
failure parses at once (no ACCEPT pump can be
outstanding). Offline tests: arm/skip conditions,
present-at-once, budget-exhausted absence, late flush
inside budget (marker probe kills 2, reverted). If the
grace expires with no receipt on a future run, that is
itself the next finding (pump never finished), not a
reason to wait longer.

Next: ONE verification lane reading the reverse close
receipt through the grace window (firewall: no further
lane this turn beyond it). Reverse STREAM data, §6
recovery, and product close-out remain open; 478 stays
active.

## 2026-10-16 lane `kzn0a8gt`: pre-STREAM stop at outbound-build; live campaign paused

Verification lane with split + sidecar-clean + grace
live (pinned i2pd 2.61.0, `mesh_quiet: true`, scratch
`kzn0a8gt-outbound-build-session-fail-run`
snapshotted). Failed at SESSION CREATE
(`terminal_failure_stage: outbound-build`, 2 installs
committed of 3 matches). No STREAM attempt: send-split
keys still never reported live; grace correctly skipped
(`accept_close_wait_s: 0.0`, `accept_close_found:
false` — no reverse leg, no wasted budget);
`driver_leg_failure: null` (startup clean holds, no
stale carryover). The evidence-plumbing fixes are all
live-proven; the product frontier was not reached.

**Disposition — live campaign paused for this turn.**
Four lanes, four different early-stage signatures
(`sy7cqc9a` forward delivery_failed; `0h6f38y_`
match-without-install with zero matches everywhere;
`djv2qtdv` full-green builds + forward pass + reverse
DATA frontier; `kzn0a8gt` outbound-build session fail),
while `djv2qtdv` proves the whole path still composes
end to end. That is run-to-run mesh variance, not a
tree regression (build/install paths untouched since
`dc81898`; deterministic suites green throughout).
But per the bounded policy an immediate fourth lane
would be an unchanged-topology retry — nothing has
changed since `djv2qtdv`, so no further lane is
licensed. The next lane needs either a new material
change or a genuinely new causal hypothesis (new
plan-of-record), not repetition. Concretely still
open: first live send-split report (needs a forward
timeout run), first close receipt through the grace
window (needs a frontier run), reverse DATA decision,
§6 live recovery, product close-out. 478 stays active;
no dependent capability unblocked; 433/439 gates
unchanged.
