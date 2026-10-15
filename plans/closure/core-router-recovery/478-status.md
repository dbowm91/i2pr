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
