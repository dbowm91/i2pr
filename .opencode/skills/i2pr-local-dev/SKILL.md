---
name: i2pr-local-dev
description: Work on the local product path of the i2pr Rust I2P router — Milestone 6 destinations/garlic/LeaseSet2/Streaming, Milestone 7 SAM 3.1, Milestone 8 SSU2, Milestone 9 I2CP, and Milestone 10 service tunnels execution. Plans 155–161 passed the SSU2 v2 stack including independent IPv4 interop against exact-pinned i2pd 2.61.0; Milestone 8 is closed within its bounded scope, Plans 163–169 passed the M9 I2CP roadmap through the self-composed local product, Plan 171 passed the invalid-preamble close corrective, and Plan 172 passed the independent LeaseSet2 lifecycle corrective closing Milestone 9; Plan 170 wire/data-plane is retained-passed with final acceptance superseded by Plan 172; Plans 174–180 passed the M10 service-tunnel foundation/profiles/reconcile, Plan 182 passed the M10 local-delivery corrective proving the local byte round-trip, Plan 181 local rows pass with remote rows blocked on retained M6 debt, Plan 183 registers the M6 mixed-router program, Plan 184 passed the authenticated I2NP preflight with no tunnel/NetDB/Streaming claim, Plan 185 passed the live one-hop exploratory tunnels + liveness lane with no multi-hop / LeaseSet2 / Streaming claim, and Plan 186 passed the mixed-router NetDB lookup / publication lane with no LeaseSet2 / Streaming claim; Plan 187 landed the local destination message plane with remote rows blocked on the build-reply gap, Plan 188 owns the narrow build-reply corrective (5/7 destination rows flipped), Plan 190 isolates and corrects the inbound NetDB reply-path metadata defect (3 more destination rows flipped blocked -> passed in fresh external run), Plan 191 ran the inbound-delivery layer and stopped at the i2pd-compatible I2CP-style Data body wire-format defect (boundary E), and Plan 192 passed the narrow corrective (9-byte short-transport inner envelope + i2cp I2CP-style Data body + STYLE=RAW SAM session + RAW RECEIVED SIZE=N digest equality; inbound-delivery layer closed for i2pd 2.61.0; 2 inbound-delivery rows flipped blocked -> passed). Plan 193 is the current executable M6 i2pd mixed-router Streaming qualification plan (local rows + external scaffold landed; external lane not yet run; static checker wired). Plan 189 (Java I2P second-family qualification) depends on Plan 193 closing first; Plan 194 is the registered-blocked-by-plan193 follow-up.
---

# I2PR Local Development

Use this skill for the local product/SAM/SSU2 execution side of the router.
Historical mixed-router NTCP2 work remains separate acceptance debt.

## Current authority

Milestone 6 local product closure remains Plan 134:

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
```

Current M7/M8 authority:

```text
plan_146_private_destination_reference = passed
plan_147_raw_driver_implementation = retained
plan_149 = passed-self-composing-local-product
plan_150_external_core_evidence = retained-passed
plan_150_final_acceptance = superseded-by-plan151
plan_151 = passed-final-acceptance-evidence-correction
plan_152 = passed-narrow-m6-corrective
plan_153 = passed-post-m7-authority-and-ci-hygiene
sam_independent_clients = at-least-two-passed-via-plan150
milestone7_local_product = passed-via-plan149
milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed

plan_154 = registered-m8-ssu2-v2-roadmap
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo
plan_157 = passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation
plan_158 = passed-m8-ssu2-udp-runtime-and-local-session-product
plan_159 = passed-m8-ssu2-path-validation-publication-and-transport-selection
plan_160 = passed-m8-ssu2-peer-test-and-relay-reachability
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration

milestone8_planning_authority = plan154
milestone8_foundation = passed-via-plan155
milestone8_handshake = passed-via-plan156
milestone8_data_phase = passed-via-plan157
milestone8_udp_runtime = passed-via-plan158
milestone8_path_publication_selection = passed-via-plan159
milestone8_peer_test_relay = passed-via-plan160
milestone8_ssu2_direction_a = passed-via-plan161
milestone8_ssu2_direction_b = passed-via-plan161
milestone8_ssu2_ledger = landed-via-plan161
milestone8_final_acceptance = closed-via-plan161

plan_163 = registered-m9-i2cp-roadmap
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
plan_165 = passed-m9-i2cp-connection-session-and-options
plan_166 = passed-m9-i2cp-client-owned-destination-and-leaseset2
plan_167 = passed-m9-i2cp-loopback-server-runtime
plan_168 = passed-m9-i2cp-message-data-plane
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
plan_170_external_wire_data_plane = retained-passed
plan_170_final_acceptance = superseded-by-plan172
plan_171 = passed-m9-i2cp-invalid-preamble-close-and-ci-corrective-retained
plan_172 = passed-m9-i2cp-independent-leaseset2-lifecycle-corrective

milestone9_planning_authority = plan163
milestone9_wire_foundation = passed-via-plan164
milestone9_connection_session_options = passed-via-plan165
milestone9_client_owned_destination = passed-via-plan166
milestone9_i2cp_loopback_server_runtime = passed-via-plan167
milestone9_i2cp_message_data_plane = passed-via-plan168
milestone9_i2cp_self_composed_local_product = passed-via-plan169
milestone9_i2cp_independent_wire_data_plane = passed-via-plan170
milestone9_i2cp_independent_clients = passed-via-plan170-and-plan172
milestone9_i2cp_independent_leaseset2 = passed-via-plan172
milestone9_i2cp_invalid_preamble_close = passed-via-plan171
milestone9_final_acceptance = closed-via-plan172

plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
plan_176 = passed-m10-http-i2p-proxy-and-connect
plan_177 = passed-m10-socks5-i2p-connect-proxy
plan_178 = passed-m10-irc-client-profile-and-privacy-filtering
plan_179 = passed-m10-irc-server-profile-and-authenticated-peer-hostname
plan_180 = passed-m10-service-tunnel-composition-reconcile-and-hardening
plan_181 = blocked-by-m6-mixed-router-streaming-blocker
plan_182 = passed-m10-local-delivery-corrective
plan_183 = registered-m6-mixed-router-streaming-interop-program
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187 = blocked-by-m6-build-reply-interop-gap (5/7 flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows blocked on plan191; full inbound-delivery layer blocked on plan192)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190; inbound-delivery layer closed via plan192; deferred Streaming pass becomes executable next)
plan_189 = registered-blocked-by-plan188-plan190-plan191-plan192-and-streaming (cross-family ledger/checker/workflow landed, second-family Java deferred)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (3 destination rows flipped blocked -> passed in fresh external run)
plan_191 = stopped-by-inbound-delivery-boundary-E-closed-via-plan192 (retained-passed; narrower follow-up registered + closed)
plan_192 = passed-m6-i2cp-wire-format-corrective (9-byte short-transport inner envelope + i2cp I2CP-style Data body + STYLE=RAW SAM session + RAW RECEIVED SIZE=N digest equality; 2 inbound-delivery rows flipped blocked -> passed; inbound-delivery layer closed for i2pd 2.61.0)
plan_193 = in-progress-m6-i2pd-mixed-router-streaming-qualification (local rows passed; external scaffold landed; external lane not yet run; static checker wired into CI floor)
plan_194 = registered-blocked-by-plan193 M6 Java I2P second-family qualification
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = passed-via-plan176
milestone10_socks5 = passed-via-plan177
milestone10_irc_client = passed-via-plan178
milestone10_irc_server = passed-via-plan179
milestone10_local_product = passed-via-plan180-and-plan182
milestone10_local_roundtrip = passed-via-plan182
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_product_layer = m6-mixed-router-streaming-with-i2pd (Plan 193 mixed-router Streaming qualification on top of the Plan 192 inbound-delivery layer)
next_executable_plan = 193 (then Plan 194 Java I2P second-family qualification)
m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-publication-outbound-passed-inbound-delivery-passed-via-plan192 (i2pd only; Java second family + Streaming not yet run)
m6_inbound_netdb_reply_path_correction = passed-via-plan190 (typed route + adapter; 3 destination rows flipped blocked -> passed in remote run)
m6_inbound_destination_delivery_boundary_E = closed-via-plan192
m6_inbound_destination_delivery = passed-via-plan192 (i2cp-compatible I2CP-style Data wire-format; STYLE=RAW SAM session; 9-byte short-transport inner envelope)
m6_i2pd_mixed_router_streaming_qualification = in-progress-via-plan193 (local rows passed; external scaffold landed; external lane not yet run; static checker wired)
m6_mixed_router_cross_family_ledger = landed-via-plan189 (i2pd-only-runs; java-second-family-deferred-until-plan193-streaming-passes)
```

Read in order for current SSU2 work:

1. `plans/162-status.md`
2. `plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md`
3. `plans/161-status.md`
4. `plans/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md`
5. `plans/160-status.md`
6. `plans/159-status.md`
7. `plans/158-status.md`
8. `plans/157-status.md`
9. `plans/156-status.md`
10. `plans/155-status.md`
11. `plans/154-status.md`

Read in order for M6 mixed-router preflight work:

1. `plans/193-status.md` (current authority: in-progress M6 i2pd mixed-router Streaming qualification; local rows passed, external scaffold landed, external lane not yet run; next executable)
2. `plans/193-m6-i2pd-mixed-router-streaming-qualification.md` (registered plan of record)
3. `plans/193-streaming-status.md` (Plan 193 M6 i2pd mixed-router Streaming qualification execution status)
4. `plans/192-status.md` (passed M6 i2pd-compatible I2CP-style Data body wire-format corrective; inbound-delivery layer closed for i2pd 2.61.0)
5. `plans/192-m6-i2cp-wire-format-corrective.md` (passed plan of record)
6. `plans/191-status.md` (Plan 191 stopped at inbound-delivery boundary E; documented I2CP-style Data body defect)
7. `plans/191-m6-inbound-destination-delivery-boundary.md` (boundary E plan)
8. `plans/190-status.md` (passed inbound NetDB reply-path corrective; 3 destination rows flipped blocked -> passed in fresh external run)
9. `plans/190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md` (corrective plan of record)
10. `plans/188-status.md` (5/7 destination rows flipped; reply-path correction closed by Plan 190; 2 inbound-delivery rows blocked on Plan 192; 2 ordering rows flipped passed)
11. `plans/188-m6-short-build-reply-interop-corrective.md` (passed install half; lookup half closed by Plan 190; inbound-delivery half blocked on Plan 191/192)
12. `plans/187-status.md` (local destination message plane landed; remote rows blocked on the Plan 188/190/191/192 sequence)
13. `plans/187-m6-remote-leaseset2-and-destination-garlic-routing.md`
14. `plans/186-status.md` (passed NetDB lookup/publication)
15. `plans/186-m6-mixed-router-netdb-lookup-and-publication.md`
16. `plans/185-status.md` (passed one-hop tunnels + liveness)
17. `plans/184-status.md` (passed authenticated I2NP preflight)
18. `plans/184-m6-authenticated-i2np-runtime-and-reference-preflight.md`
19. Do not claim destination remote interop, Streaming, or M10 remote-service interop. The destination lane is `bash tests/integration/m6-interop/run-destination.sh` + `bash scripts/check-destination-tunnel-evidence.sh` (22 guarded rows; inbound-delivery rows flipped blocked -> passed via Plan 192); the NetDB lane stays `bash tests/integration/m6-interop/run-netdb.sh` + `bash scripts/check-netdb-tunnel-evidence.sh`; the tunnel lane stays `bash tests/integration/m6-interop/run-tunnels.sh` + `bash scripts/check-exploratory-tunnel-evidence.sh`; the streaming lane is `bash tests/integration/m6-interop/run-streaming.sh` + `bash scripts/check-streaming-tunnel-evidence.sh` (22 guarded labels; external driver fail-closed when i2pd env is absent). Creator-known tunnel keys are never installed without a consumed build reply.
For SAM/local-product history, then read Plan 151, 150, 149 and Plans 146–148
as needed.

Read in order for Milestone 9 I2CP work:

1. `plans/170-status.md`
2. `plans/170-m9-i2cp-independent-clients-and-final-closure.md`
3. `plans/171-status.md`
4. `plans/171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md`
5. `plans/169-status.md`
6. `plans/169-m9-i2cp-self-composed-local-product-and-hardening.md`
7. `plans/168-status.md`
8. `plans/168-m9-i2cp-message-data-plane.md`
9. `plans/167-status.md`
10. `plans/167-m9-i2cp-loopback-server-runtime.md`
11. `plans/166-status.md`
12. `plans/166-m9-i2cp-client-owned-destination-and-leaseset2.md`
13. `plans/165-status.md`
14. `plans/165-m9-i2cp-connection-session-and-options.md`
15. `plans/164-status.md`
16. `plans/164-m9-i2cp-protocol-and-wire-foundation.md`
17. `plans/163-m9-i2cp-roadmap.md` (planning authority)
18. Milestone 9 is closed via Plan 172.

Read in order for Milestone 10 service-tunnel work:

1. `plans/181-status.md` (blocked M10 independent acceptance; current authority)
2. `plans/181-m10-independent-application-and-service-interop-final-closure.md`
3. `plans/182-status.md` (passed M10 local-delivery corrective)
4. `plans/182-m10-local-delivery-corrective.md`
5. `plans/183-status.md` (registered M6 mixed-router program; next)
6. `plans/183-m6-mixed-router-streaming-interop-program.md`
7. `plans/180-status.md` (passed composition/reconcile/hardening)
8. `plans/180-m10-service-tunnel-composition-reconcile-and-hardening.md`
9. `plans/179-status.md` (passed IRC `.i2p` server profile + authenticated peer hostname)
10. `plans/179-m10-irc-server-profile-and-authenticated-peer-hostname.md`
11. `plans/178-status.md` (passed IRC `.i2p` client profile + privacy filtering)
12. `plans/178-m10-irc-client-profile-and-privacy-filtering.md`
13. `plans/177-status.md` (passed SOCKS5 `.i2p` CONNECT)
14. `plans/177-m10-socks5-i2p-connect-proxy.md`
15. `plans/176-status.md` (passed HTTP `.i2p` proxy + CONNECT)
16. `plans/176-m10-http-i2p-proxy-and-connect.md`
17. `plans/175-status.md` (passed generic client/server tunnels)
18. `plans/175-m10-generic-client-server-service-tunnels.md`
19. `plans/174-status.md` (passed foundation)
20. `plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`
21. `plans/173-status.md` (roadmap authority)
22. `plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`
23. Do not claim M10 final closure: Plan 181 is blocked by the
    retained M6 mixed-router Streaming debt; Plan 183 owns the
    corrective program. Do not relabel the blocked remote rows.

Plans 155–160 passed the local SSU2 v2 protocol/runtime/reachability sequence.
Plan 161 has passed the final independent gate: directions A
(`i2pr initiator -> i2pd responder`) and B (`i2pd initiator -> i2pr
responder`), plus cached-token and malformed/resource rows, over real
loopback UDP against exact-pinned i2pd 2.61.0, with the fail-closed
15-row evidence ledger (`tests/integration/ssu2/run-independent.sh`),
its integrity checker (`scripts/check-ssu2-acceptance-evidence.sh`,
routine-CI-enforced), and the manual
`.github/workflows/ssu2-external.yml` lane green locally and hosted
(routine CI runs `34050058216`/`34053041778`, external runs
`34051298144`/`34053042857`); Java I2P is
recorded nonblocking debt. Plan 162 passed its
narrow corrective: routine CI now ignores the environment-dependent external
test while retaining all-target compilation, and explicit external selection
remains fail-closed. Milestone 8 is closed within this bounded scope.

Plan 163 registered the Milestone 9 I2CP roadmap (client-owned
destinations, `i2pr-api::i2cp` framing/session state, daemon-owned
loopback listener, reuse of the `i2pr-client` destination product,
Plans 164–170 in order). Plan 164 passed the wire/profile
foundation: pinned official I2CP sources with Java I2P 2.13.0 and
go-i2cp references, the explicit M9 compatibility profile,
runtime-neutral `i2pr-api::i2cp` bounded framing/message codecs with
committed fixtures (`tests/fixtures/i2cp/`) and the routine-CI
vector checker (`scripts/check-i2cp-vectors.sh`).
Plan 165 passed the connection/session/options state machines
(`ConnectionStateMachine` with explicit message-family transitions,
canonical `SessionConfig` signature/date/ceiling verification with
injected `Clock`, option disposition table and projection into
`i2pr-client::DestinationConfig`, bounded `SessionRegistry` with
reserve/commit/rollback, reconfiguration taxonomy, typed
`I2cpAction` vocabulary). Plan 166 passed the M9 client-owned
destination + LeaseSet2 bridge: `DestinationOwnership::RouterOwned`
/ `ClientOwned`, `DestinationPublic` (non-secret public destination),
`InboundDecryptionCapability` (non-`Clone`, redacted, zeroized
wrapper for the client-supplied X25519 inbound decryption secret),
atomic `install_client_lease_set2` (signature + lease ownership +
expiry + decryption-key match), typed `LeaseRequest` (sourced from
real inbound tunnels, never synthesized) with
`take_client_refresh_request`, and the
`I2cpAction::RequestVariableLeaseSet` action. SAM router-owned
product regressions remain green. Plan 167 passed the M9 I2CP
loopback server runtime in `crates/i2pr-daemon/src/i2cp.rs`:
`0x2a` preamble + incremental `FrameDecoder` + multi-frame
dispatch + atomic `install_client_lease_set2` + supervised
per-connection `ChildScope` + single `teardown_connection` cleanup
path, twelve real-TCP acceptance tests in
`crates/i2pr-daemon/tests/i2cp_loopback.rs`. Plan 168 passed the
M9 I2CP message data plane in `crates/i2pr-api/src/i2cp/data_plane.rs`
plus the daemon projection in `crates/i2pr-daemon/src/i2cp.rs`:
bounded per-session `SendMessage`/`SendMessageExpires` validation,
bounded `MessageStatus` correlation table, sibling-isolated
`MessagePayload` inbound delivery through a `tokio::sync::Notify`,
cross-session local loopback shortcut for two destinations owned
by active I2CP sessions, `DestLookup` resolution through the local
destination registry, and `GetBandwidthLimits` returning the
config-derived client ceiling and the documented neutral router
values; eighteen real-TCP black-box tests in
`crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` exercise
every Plan 168 §11 case. Plan 169 passed the M9 I2CP
self-composed local product and hardening: the
`handle_reconfigure_session` + `apply_reconfigure` +
`ReconfigurationOutcome` reconfigure transaction handler in
`crates/i2pr-daemon/src/i2cp.rs` (parses/verifies the full new
SessionConfig, classifies each diff entry using the Plan 165
`reconfiguration_class` table, and commits the new baseline
atomically through `I2cpSessionState::last_options`), the
synchronous `handle_destroy_session` data-plane drain, and three
new narrowly named acceptance suites
(`crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` — 5 tests,
`crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` — 20 tests,
`crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` — 6 tests)
that bind the listener to `127.0.0.1:0` and drive behavior only
through TCP/I2CP inputs. Plan 171 passed the narrow
invalid-preamble close corrective on the common terminal path
(`handle_connection` shuts the TCP stream down explicitly before
bookkeeping release; the strict wrong-preamble row proves a
24-iteration rejection trajectory plus a non-paused companion
test) (retained). Plan 170 closed the final
independent-client gate: exact-pinned Java I2P 2.13.0 and go-i2cp
exchange digest-matched small/large payloads both directions
through the loopback daemon under the fail-closed 9-row lane
(`tests/integration/i2cp/run-independent.sh`,
`scripts/check-i2cp-acceptance-evidence.sh`,
`.github/workflows/i2cp-external.yml`).

SAM stays experimental, loopback-only, disabled by default, and non-advertised.
SSU2 public advertisement/public-network participation and broad router
interoperability remain unclaimed. I2CP stays experimental,
loopback-only, disabled by default, and non-advertised throughout M9.

## Retain these working pieces

Do not rebuild them without a concrete defect:

- Plan 137 bounded loopback listener/session lifecycle;
- Plan 142 I2P Base64 correction;
- Plan 146 Java I2P/i2pd private-destination reference compatibility;
- `DestinationIdentity::from_imported` semantics;
- strict SAM parser/resource ceilings and secret hygiene;
- Plan 139 loopback-only FORWARD/NAMING implementation;
- `StreamingManager` and `StreamingDestinationAdapter` as the authoritative stream implementation;
- Plan 129 local destination/ECIES/Garlic/Streaming product path;
- Plan 147 owned raw `TcpStream` handoff, same-read preservation, actual `Established` wait, OS CSPRNG runtime path, byte pump, and supervised ACK/retransmit driver;
- Plan 149 transactional self-composed `SESSION CREATE`, one shared `Arc<DestinationIdentity>`, `SamLocalProductFabric`, local peer LeaseSet2 resolution, automatic destination driver, byte-exact SILENT/peer metadata, and typed delivery counters;
- Plan 150 external core evidence: pinned i2psam + qualified i2plib SAM surface, exact two-direction 2 MiB transfers, private destinations, SILENT, NAMING, negative matrix, and positive FORWARD;
- Plans 155–160 SSU2 local protocol/runtime/path/peer-test/relay architecture;
- Plan 161 direction-A handshake transcript corrections and regenerated vectors. Independent i2pd comparison exposed those defects; do not revert them to match older i2pr↔i2pr assumptions.
- Plan 164 I2CP framing/message codecs, the M9 compatibility profile, and the committed `tests/fixtures/i2cp/` vectors. Do not extend structural codecs into behavior/session/listener claims; those belong to Plans 165–170.
- Plan 165 I2CP `ConnectionStateMachine`/SessionConfig verification/option projection/session registry/typed `I2cpAction` vocabulary. Do not extend into a listener, destination activation, or interoperability claim; those belong to Plans 166–170.
- Plan 166 client-owned destination capability surface (`DestinationOwnership`, `DestinationPublic`, `InboundDecryptionCapability`, `install_client_lease_set2`, `LeaseRequest`, `take_client_refresh_request`) and the `I2cpAction::RequestVariableLeaseSet` action. Do not extend into a listener, socket ownership, or interoperability claim; those belong to Plans 167–170.

## Why Plan 151 exists

Plan 150's implementation/external-client work is useful, but its final
acceptance ledger overclaimed several deferred cases. The clearest example was
an unconditional `multiple-stream-lifecycle = passed` row referring to a Plan
149 sibling-stream test that did not exist.

Plan 151 made the deferred sibling/backpressure/fault/CLOSE-RESET/FORWARD and
focused M6 regression items executable through the real listener and required
every final `passed` row to derive from a command/test that actually ran.

That pass exposed one narrow M6 robustness defect family, closed by Plan 152
without a wire change: bounded receiver retention/ACK gating, coalesced
duplicate ACK behavior, and sender ECIES ratchet-key trimming.

## Evidence-integrity rule

No required final row may be marked passed merely because another plan/status
says it passed. `tests/integration/sam/run-independent.sh` derives required SAM
rows from executed commands/tests.

Plan 151 added:

```text
scripts/check-sam-acceptance-evidence.sh
```

The checker is enforced in routine Linux CI and the manual SAM external
workflow. Do not weaken it to make CI pass.

The same principle applies to current SSU2 work: Plan 161 final evidence must
come from explicitly executed local/external commands. An external test that
is skipped because no peer exists is **not** an external-interoperability pass.

## Plan 161 independent SSU2 provenance

Retain exact pins:

```text
i2pd
  version: 2.61.0
  repo: PurpleI2P/i2pd
  pin: 635b013a612ff47278ef02acf8580a28e10e26c5
  role: mandatory independent Plan 161 SSU2 reference

Java I2P
  version: 2.13.0
  repo: i2p/i2p.i2p
  pin: 9134f808337b401e8e53c73734c81fab04280c9d
  role: preferred secondary; nonblocking if narrow unprivileged orchestration is disproportionate
```

Do not patch or vendor external routers.

Direction A retained evidence:

```text
i2pr initiator -> i2pd responder
real loopback UDP
tokenless TokenRequest -> Retry -> SessionRequest -> SessionCreated -> SessionConfirmed
mutual authentication
small DatabaseStore i2pr -> i2pd
fragmented DatabaseStore i2pr -> i2pd
DeliveryStatus return for both stores
graceful session/resource teardown
```

Direction B is proven symmetrically (i2pd initiator -> i2pr responder
promotion through the normal token/Retry path, same small + fragmented
proof shape). The direction-B baseline predates the inter-direction
settle sleep so a redial landing inside the settle still counts;
see `plans/161-status.md`. Java I2P is recorded nonblocking
narrow-orchestration debt in every ledger artifact, not a silent gap.

## Plan 162 closure rule/result

Current routine CI run `33915994884` on head
`4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c` failed both Ubuntu and macOS
quality jobs because ordinary workspace execution automatically ran:

```text
crates/i2pr-runtime/tests/ssu2_independent.rs
```

without an external i2pd environment. Dependency policy and MSRV passed; the
observed error was `missing required env I2PD_ROUTER_INFO`.

Plan 162 implemented this shape:

```text
ordinary workspace test
  -> external test is compiled/discovered
  -> external test is ignored
  -> ordinary command exits 0

dedicated external invocation
  -> explicitly selects ignored test with --ignored --exact
  -> missing external environment still fails hard
  -> exact-pinned i2pd environment executes the real trajectory
```

Preferred mechanism: a descriptive Rust `#[ignore = "..."]` attribute on only
the environment-dependent external test.

Forbidden fixes:

- missing-env early return/success;
- CI executable-name filtering;
- `|| true`;
- `continue-on-error`;
- fake `I2PD_*` values;
- broad crate/integration-test exclusion;
- production SSU2 changes merely to make CI green.

Plan 162 re-ran direction A after gating and required routine Ubuntu/macOS CI
green on its exact closing commit. The implementation closing commit was
`624e8cce177040674376163160cfbda47e6a60fe`, verified by hosted CI run
`33941941145`; `next_executable_plan = 161` is restored.

## External SAM provenance

Retain exact pins:

```text
i2psam
  repo: https://github.com/i2p/i2psam
  pin: b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
  role: counted external client

i2plib
  repo: https://github.com/l-n-s/i2plib
  pin: 6edf51cd5d21cc745aa7e23cb98c582144884fa8
  role: counted qualified SAM-surface substitute

libsam3
  repo: https://github.com/i2p/libsam3
  pin: 7d6e658798baec31394c5685f9583343cc00900b
  role: built/probed, not counted
```

Do not patch or vendor external clients.

## Environment contract

```text
root/sudo                         = no
Linux namespaces                  = no
Docker                            = no
VM/Multipass                      = no
systemd                           = no
public I2P network                = no
localhost TCP                     = yes
localhost UDP                     = yes
exact-pinned external i2pd process= yes, Plan 161 dedicated lane only
routine CI external peer          = no
manual GitHub external lane       = yes
```

## Development commands

Routine floor:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
cargo deny check advisories bans sources
```

Focused SAM floor:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- --test-threads=1
```

Focused SSU2 floor:

```text
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --lib
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay -- --test-threads=1
bash scripts/check-ssu2-vectors.sh
```

Focused I2CP floor:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-api --test i2cp_vectors
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
bash scripts/check-i2cp-vectors.sh
bash scripts/check-i2cp-acceptance-evidence.sh
```

Focused M10 service-tunnel floor:

```text
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --lib destination_streaming
cargo test --locked -p i2pr-daemon --lib config
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_client_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_server_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_independent_application_clients -- --test-threads=1
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
```

Plan 162 ordinary no-peer regression:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent -- --test-threads=1
# expected after Plan 162 implementation: test ignored, exit 0
```

Plan 162 / Plan 161 explicit external invocation:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
```

With required environment absent, that explicit command must fail for missing
external configuration. With the exact-pinned i2pd lane provisioned, it must
execute and pass the full matrix (directions A+B, cached-token,
malformed/resource rows).

The full Plan 161 lane (local suites + matrix + gates, 15 command-derived
rows) is:

```text
bash tests/integration/ssu2/run-independent.sh
bash scripts/check-ssu2-acceptance-evidence.sh
```

## Coding rules

- No new unbounded channels/queues.
- Runtime/socket ownership stays in daemon/runtime layers.
- The SSU2 central scheduler replaces a handshake's resend deadline with each new arm batch; never min-merge with a stale past value (Plan 158 regression).
- SSU2 path migration must requeue unacked fragments through the bounded loss policy; never just clear sent provenance (Plan 159 regression).
- SSU2 peer-test correlation is by nonce plus role/state, never by source; unsigned out-of-session corroboration never confirms direct reachability (Plan 160).
- SSU2 relay success proves firewalled, never direct; verify HolePunch against nonce-derived connection IDs before touching request state (Plan 160).
- SSU2 path challenges/responses are single-shot minimum-MTU control datagrams; never migrate on source change alone.
- OS CSPRNG for runtime material; deterministic randomness is test-only.
- Never log private destination material, SSU2 static/session keys, tokens, or raw payloads.
- No second private identity copy for SAM bridge ownership.
- Do not weaken M6 Streaming semantics for SAM tests.
- Do not weaken SSU2 authentication/RouterInfo/token/replay semantics for external interop.
- Do not modify SSU2 production wire behavior to repair Plan 162 CI selection.
- I2CP framing/session state stays in `i2pr-api` with no sockets, Tokio, timers, or task ownership; TCP/Tokio ownership stays in `i2pr-daemon`; destination behavior stays in `i2pr-client` (which must never depend on `i2pr-api`).
- I2CP decryption material stays non-`Clone`, redacted, and zeroized; never log private keys, session secrets, tokens, or raw payloads.
- Do not claim I2CP behavior, sessions, listeners, or client interop from structural codecs alone.

## Final claim rules

- SAM stays disabled by default, loopback-only, experimental, and non-advertised.
- Milestone 7 final localhost acceptance is closed via Plan 151; Plan 152 is the retained narrow M6 corrective underneath it.
- Plan 153 passed post-M7 docs/CI hygiene.
- Plans 155–160 passed the local SSU2 v2 protocol/runtime/path/reachability sequence.
- Plan 161 passed the final independent gate against exact-pinned i2pd
  (directions A+B, cached-token/malformed rows, fail-closed
  ledger/checker/workflow lane green locally and hosted); Milestone 8 is
  closed within that bounded scope.
- Plan 162 passed the narrow external-test lane/CI corrective; it must not broaden or downgrade direction-A protocol evidence.
- Plan 163 registered the M9 I2CP roadmap (planning authority only).
- Plan 164 passed the M9 I2CP wire/profile foundation (structural codecs, fixtures, profile; no behavior claim).
- Plan 165 passed the M9 I2CP connection/session/options state machines (typed connection state, SessionConfig signature/date/ceiling verification with injected clock, option disposition table, bounded session registry, reconfiguration taxonomy, typed `I2cpAction` vocabulary; no listener, destination activation, or interoperability claim).
- Plan 166 passed the M9 I2CP client-owned destination + LeaseSet2 bridge: `DestinationOwnership`, `DestinationPublic`, `InboundDecryptionCapability`, atomic `install_client_lease_set2`, typed `LeaseRequest`, `take_client_refresh_request`, and the `I2cpAction::RequestVariableLeaseSet` action. SAM router-owned product regressions remain green; no listener, socket ownership, or interoperability claim.
- Plan 167 passed the M9 I2CP loopback server runtime in `crates/i2pr-daemon/src/i2cp.rs`: disabled-by-default `[i2cp]` block, supervised Tokio listener, per-connection `ChildScope`, typed `I2cpAction` dispatch, single `teardown_connection` cleanup path on EOF/reset/timeout/cancel, twelve real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_loopback.rs`. No application-message direction, no lookup, no reconfiguration, no independent-client interop claim.
- Plan 168 passed the M9 I2CP message data plane: bounded per-session `SendMessage`/`SendMessageExpires` validation against the existing `i2pr_client::DestinationRuntime::enqueue_outbound` seam, bounded `MessageStatus` correlation table, `MessagePayload` inbound frames delivered only to the owning session's bounded queue (sibling-isolation guaranteed), cross-session local loopback shortcut, `DestLookup` resolving through the local destination registry, and `GetBandwidthLimits` returning the config-derived client ceiling and the documented neutral router values. Eighteen real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` exercise every Plan 168 §11 case. SAM router-owned product regressions remain green. No reconfiguration, no `HostLookup`/`HostReply` resolution, and no independent-client interop claim.
- Plan 169 passed the M9 I2CP self-composed local product and hardening (reconfigure transaction handler, atomic reconfigure baseline, synchronous destroy drain; 5 + 19 + 6 black-box tests, all TCP/I2CP-driven; no `HostLookup`/`HostReply`, no independent-client evidence — those belong to Plan 170).
- Plan 171 passed the M9 I2CP invalid-preamble close and CI corrective (retained): explicit `stream.shutdown()` on the common per-connection terminal path before bookkeeping release (no wire change); the strict wrong-preamble row now proves 24-iteration rejection with zeroed baselines plus a subsequent valid client (paused test waits via a bounded yield-pump/`try_read` drain, no virtual-time timeout), with a non-paused companion test separating product-close evidence from paused-clock behavior. The adversarial matrix is now 20 tests.
- Plan 170 external wire/data-plane evidence is retained-passed: exact-pinned Java I2P 2.13.0 and go-i2cp exchange digest-matched 25 B/32 KiB payloads both directions through the loopback daemon under the fail-closed 9-row lane. Its final-acceptance interpretation is superseded by Plan 172 (counted Java driver bypassed `I2PSession.connect()`; no external LeaseSet2 install).
- Plan 172 passed the M9 I2CP independent LeaseSet2 lifecycle corrective: explicit local zero-hop tunnel kind, 44-byte Lease-compatible non-empty real `RequestVariableLeaseSet`, Plan 166 atomic install (ElGamal-slot skip, unpublished accepted, u8 LS2 key count), high-level Java `I2PSession.connect()` plus public go-i2cp async `ProcessIO` lifecycle proof, post-LS2 digest-matched cross-client traffic both directions, fail-closed 24-row evidence. Milestone 9 final acceptance is closed via Plan 172.
- Plan 173 registered the M10 service-tunnels roadmap (planning authority only).
- Plan 174 passed the M10 service-tunnel foundation and shared stream runtime: runtime-neutral `i2pr-service-tunnels` crate, strict disabled-by-default loopback-only `[service_tunnels]` surface, generic bounded socket<->Streaming pump reused by SAM, no listener yet. Do not implement generic, HTTP, SOCKS5, or IRC listeners until Plan 175.
- Plan 175 passed the first complete M10 application service product: persistent router-owned service destinations (`ServiceDestinationStore`; versioned, atomic, secret-safe) and a daemon-owned `ServiceTunnelManager` that binds loopback TCP for `generic-client` and a loopback Streaming listener for `generic-server`, reuses the Plan 149 local destination product path and the Plan 174 shared byte pump, and exposes a typed cross-tunnel local destination lookup so client/server tunnels owned by the same router do not need an external LeaseSet lookup. Plan 175 enables `enabled = true` only for `generic-client` and `generic-server`; HTTP/SOCKS/IRC remain rejected as not-yet-available until Plans 176-179.
- Plan 176 passed the first M10 application profile: the runtime-neutral `i2pr-service-tunnels::http` module (bounded HTTP/1.1 parser with smuggling rejection, hop-by-hop `Connection` removal, conservative privacy/header rewrite, `.i2p`-only target validation, bounded error response generation), the strict disabled-by-default `http-client` configuration surface, and the daemon HTTP proxy executor (`crates/i2pr-daemon/src/service_tunnels_http.rs`) that owns one loopback listener, parses headers under a bounded deadline, dispatches CONNECT to a port-policy-gated 2xx tunnel or rewrites/forwards ordinary proxy requests, and reuses the Plan 174 shared byte pump + Plan 149 destination product path. No new Garlic/I2NP/Streaming implementation is introduced; the full I2P Streaming byte round-trip over local TCP for the HTTP profile is owned by Plan 180 reconcile work. Plan 176 enables `enabled = true` for `generic-client`, `generic-server`, and `http-client`; SOCKS/IRC remain rejected as not-yet-available until Plans 177-179.
- Plan 177 passed the second M10 application profile: the runtime-neutral `i2pr-service-tunnels::socks5` module (RFC 1928 no-auth greeting negotiation; incremental CONNECT request parser with strict `.i2p`/DOMAINNAME-only target policy, rejecting IPv4/IPv6/BIND/UDP ASSOCIATE/unknown commands/zero domain/zero port/NUL/control/whitespace domain bytes; deterministic RFC 1928 reply generator with neutral `127.0.0.1:0` bind; bounded typed errors mapped to the eight RFC 1928 reply codes), the strict disabled-by-default `socks5-client` configuration surface, and the daemon SOCKS5 proxy executor (`crates/i2pr-daemon/src/service_tunnels_socks5.rs`) that owns one loopback listener, runs greeting + CONNECT negotiation under bounded deadlines, validates the CONNECT port against the per-service `ConnectPortPolicy` (default `{443}`), resolves the destination through the manager (Base32 / alias / local-delivery path), opens I2P Streaming, sends the success reply only after Streaming reaches `Established`, and runs the shared Plan 174 byte pump in opaque tunnel mode with same-read post-request bytes preserved as first tunnel bytes. No clearnet outproxy, SOCKS UDP, BIND, SOCKS4/4a, authentication, Tor RESOLVE, or arbitrary local/LAN target relay. Plan 177 enables `enabled = true` for `generic-client`, `generic-server`, `http-client`, and `socks5-client`; IRC remains rejected as not-yet-available until Plans 178-179.
- Plan 178 passed the third M10 application profile: the runtime-neutral `i2pr-service-tunnels::irc` module (bounded IRC/IRCv3 line parser with 512-byte core / 8191-byte tag-envelope / 4094-byte tag-data ceilings; structural IRCv3 message-tag framing; typed command classifier with an explicit per-direction allowlist; client-to-network privacy rewrites for USER/PING/QUIT/PART; CTCP/DCC policy allowing ACTION while dropping malformed/multi-delimiter messages, address-bearing DCC, and unsupported CTCP; bounded typed errors), the strict disabled-by-default `irc-client` configuration surface, and the daemon IRC client tunnel executor (`crates/i2pr-daemon/src/service_tunnels_irc_client.rs`) that owns one loopback listener per `irc-client` spec and reuses the Plan 174 shared byte pump + Plan 149 destination product path. No new Garlic/I2NP/Streaming implementation is introduced; unknown commands are dropped, never passed, and overlong lines are dropped without truncation. The full I2P Streaming byte round-trip over local TCP for the IRC client profile is owned by Plan 180 reconcile work. Plan 178 enables `enabled = true` for `generic-client`, `generic-server`, `http-client`, `socks5-client`, and `irc-client`; `irc-server` remains rejected as not-yet-available until Plan 179. No DCC tunnel support, WEBIRC, TLS termination, SASL credential management, bouncer state, or server-side filter claim.
- Plan 179 passed the fourth M10 application profile: the runtime-neutral `i2pr-service-tunnels::irc::server` registration interceptor (bounded pre-registration line / byte ceilings with a typed default of 10 lines / 8192 bytes; cross-protocol rejection of HTTP/BitTorrent first lines via a small fixed list; an authenticated peer Destination hash projection to `<52-char base32>.b32.i2p` that replaces the USER hostname and is bound to the streaming peer identity; RFC 2812 four-arg and legacy RFC 1459 USER shapes; IRCv3 tagged USER rewrite with envelope preserved; PASS / CAP / AUTHENTICATE / NICK passthrough; same-read post-USER bytes preserved as first raw-pump bytes; optional `SERVER` server-to-server handoff; typed `RegistrationOutcome::{Incomplete, Ready, Rejected, Eof}`), the strict disabled-by-default `irc-server` configuration surface that reuses the Plan 175 persistent server destination storage, and the daemon IRC server tunnel executor (`crates/i2pr-daemon/src/service_tunnels_irc_server.rs`) that owns one Streaming accept loop per `irc-server` spec, waits for the Streaming connection to reach `Established`, captures the peer Destination hash from authenticated Streaming metadata (the only acceptable source for the projected hostname), runs the bounded registration interceptor under a 30 s total deadline (with a 20 ms poll cadence), connects to the loopback target under a 10 s deadline, writes the rewritten prefix + leftover exactly once, and switches to the shared Plan 174 byte pump in opaque mode for the post-registration stream. The Plan 175 persistent server destination storage owns the IRC server destination identity so restart preserves both the public service Destination and the projected hostname algorithm. No new Garlic/I2NP/Streaming implementation is introduced; no WEBIRC, no cloaked hostnames, no DCC, no TLS termination, no IRC daemon implementation, and no post-registration server-side filter claim. Plan 179 enables `enabled = true` for `generic-client`, `generic-server`, `http-client`, `socks5-client`, `irc-client`, and `irc-server`; no remaining not-yet-available gate exists for the current kinds. The full I2P Streaming byte round-trip over local TCP for the IRC server profile is owned by Plan 180 reconcile work; Plan 179 does not silently weaken that criterion. The Plan 180 reconcile pass generalizes the SAM per-destination runtime driver loop to service tunnels so the Plan 179 §10 byte-round-trip matrix executes end-to-end without re-plumbing the manager surface.
 - Plan 180 passed the M10 service-tunnel composition, reconcile, and hardening: the runtime-neutral `i2pr_service_tunnels::generation::DiffClass` typed classification (`Unchanged`, `MutableInPlace`, `ReplaceListener`, `ReplaceDestination`, `Remove`, `Add`); the daemon-owned `ServiceTunnelGeneration` / `DrainingGeneration` committed-generation model with `GenerationCounters { active_current_generation, active_draining_generation, forced_drain_closes_total }`; the `ServiceTunnelManager::reconcile(candidate, drain_deadline) -> ReconcileOutcome` transactional algorithm that validates the candidate, diffs it against the committed generation, stages `Add` / `Replace*` entries without disturbing the old generation, then atomically publishes the new generation and pushes only replaced/removed old runtimes onto the draining list under a hard deadline; `reap_expired_drains -> ReapReport` for forced-drain close handling; `generation_snapshot -> GenerationSnapshot` for the Plan 180 §9 unified cross-service resource accounting matrix; the static `scripts/check-service-tunnel-boundaries.sh` checker enforcing the runtime-neutral constraint, no Garlic/I2NP construction in service-tunnels, the single shared `run_stream_pump` invariant, no unbounded Tokio channels, and exactly one `register_service_tunnel_manager` entry point. Stable server identities survive no-op or target-only reconciles because `Unchanged` / `MutableInPlace` entries copy the existing committed runtime + identity into the new per-generation directory. Two new narrowly named suites (`crates/i2pr-daemon/tests/service_tunnels_final_acceptance.rs` — 15 tests covering the Plan 180 §12 reconcile matrix; `crates/i2pr-daemon/tests/service_tunnels_adversarial_matrix.rs` — 12 tests covering the Plan 180 §13 cross-service adversarial matrix) bind the manager to a temp data directory and drive behavior only through the public API. Every Plan 174/175/176/177/178/179 product suite remains green. Plan 180 closes the M10 local product layer; Plan 181 owns the M10 independent acceptance gate.
- Plan 182 passed the M10 local-delivery corrective the profiles assumed but never had: per-destination delivery drivers reusing the Plan 129 `bridge_to_peer` seam, inbound-factory install, wildcard Streaming port 0 (SAM convention), SAM-parity accept paths with queued SYN responses, direction-branched pump sends with typed backpressure matching, orderly pump half-close (default no-op keeps SAM byte-identical), a completed line-filtering IRC client executor, permit-for-task-lifetime capture, and active-slot release on every exit path. Nine round-trip tests (`service_tunnels_local_roundtrip.rs`) plus six wire-surface tests (`service_tunnels_independent_application_clients.rs`) prove the local byte round-trip. No wire change.
- Plan 181 ran its full external lane to the §6.3 stop condition: 29 local independent-application-client rows pass (unmodified curl HTTP/SOCKS, nc, stdlib generic driver, exact-pinned jaraco/irc through the real manager; restart stability; resource baselines; unsupported-profile ledger) while the two remote rows are recorded `blocked` with genuine exact-pinned i2pd 2.61.0 qualification provenance (`unknown_peer>0`, `delivered=0`, no establishment). Self-composed rows are never substituted for interop. Milestone 10 final acceptance stays open.
- Plan 183 registered the M6 mixed-router destination/Streaming interop program Plan 181 §6.3 requires (registration only); Plan 181 resumes after it produces passing remote rows.
- Plan 193 is the current executable M6 i2pd mixed-router Streaming qualification plan (see `plans/193-m6-i2pd-mixed-router-streaming-qualification.md`, `plans/193-status.md`, and `plans/193-streaming-status.md`): in-progress. Plan 193 supersedes the historical `plans/188-m6-mixed-router-streaming-with-i2pd.md` Streaming file (which remains historical context only). The local rows (`streaming_tunnel_unit` 15 + `streaming_tunnel_live` 11) pass; the fail-closed external driver (`streaming_tunnel_external::streaming_through_i2pd`, `#[ignore]`-gated) plus `tests/integration/m6-interop/run-streaming.sh` plus `scripts/check-streaming-tunnel-evidence.sh` are landed and the static checker is wired into the build floor; the external lane is not yet run. No `milestone6_i2pd_streaming_interop = passed-via-plan193` claim. Plan 194 (Java I2P second-family qualification) is blocked-by-plan193 and resumes only after Plan 193 closes.
- Plan 184 passed the M6 authenticated I2NP preflight with no tunnel/NetDB/Streaming claim; Plan 185 passed the live one-hop exploratory tunnels + liveness lane; Plan 186 passed the mixed-router NetDB lookup/publication lane with no LeaseSet2/Streaming claim.
- Plan 187 landed the local destination message plane (daemon-owned `DestinationTunnelCoordinator`, 27 unit + 9 live two-role rows including the bidirectional ECIES/Garlic round-trip with sibling isolation, narrow additive seams, no wire change) with 2/7 remote rows now flipped via Plan 188 installs (see below); 5/7 were blocked on a separate inbound NetDB reply-path metadata defect that Plan 190 isolates and corrects.
- Plan 188 in-progress: garlic-wrapped endpoint + forwarded gateway installs proven both directions (`installed_ob=1 installed_ib=1`, no synthesis, no wire change) with 2/7 rows passed; lookup/publication/messaging rows pending. No LeaseSet2/Streaming interop claimed yet.
- Plan 190 passed the inbound NetDB reply-path tunnel-ID corrective: typed public `InboundGatewayRoute` (`gateway_router`, `gateway_receive_tunnel`, `local_receive_tunnel`) retained by `i2pr-tunnel::DataPlaneRegistry`; daemon-owned `reply_path_for_inbound_route` adapter derives `i2pr-netdb::ReplyPath` only from `(gateway_router, gateway_receive_tunnel)`. Local regression rows prove unequal IDs (`0x9601` vs `0x9602`) round-trip through the I2NP codec with the gateway tuple on the wire, and that lifecycle removal cleans the typed route atomically (`destination_tunnel_unit` 31 passed, `destination_tunnel_live` 9 passed, `exploratory_build_live` 11 passed). The exact-pinned i2pd external lane now advertises the corrected reply path; the row flips from `blocked` to `passed` only after a fresh external `run-destination.sh` proves a real tunneled lookup response arrives. No `milestone6_interoperable = passed-via-plan190` claim.
 - `milestone6_interoperable = not-yet-claimed` remains unchanged.
- SSU2 public-network participation, broad router interoperability, IPv6 external interop, PQ v3/v4, and SSU1 remain unclaimed/deferred as documented.
- Do not advance `advertised = true` without `specs/CONFORMANCE.md` evidence.

Current handoff: **Plan 192 passed the M6 i2pd-compatible I2CP-style
Data body wire-format corrective: the i2pd-compatible 9-byte
NTCP2/SSU2 short-transport inner envelope + i2cp I2CP-style
Data body (`length[4 BE] + reserved[4] + fromPort[2 BE] +
toPort[2 BE] + padding[1] + protocol[1] +
gzip-no-compression-wrapped payload`) landed in
`OutboundRequest::new` + `compose_outbound_delivery` +
`StreamingDestinationAdapter::send` /
`StreamingDestinationAdapter::receive` /
`crates/i2pr-proto/src/i2cp_data_body.rs`; the test driver
switched to `STYLE=RAW` (no ElGamal/DSA `from` Identity
required), waits for `RAW RECEIVED SIZE=N\n<payload>` on
the loopback SAM socket, asserts digest equality, sends the
reply as `RAW SEND ID=... DESTINATION=... SIZE=N\n<payload>`,
and unwraps the i2cp I2CP-style Data body via
`decode_i2cp_data_body` before recovering the application
payload. The 2 inbound-delivery rows
`external-reference-received` and
`external-destination-inbound` flip from `blocked` to
`passed` in a fresh `run-destination.sh` against the
exact-pinned i2pd 2.61.0. Local Plan 187/188/190/191 suites
remain green (`destination_tunnel_unit` 32 passed,
`destination_tunnel_live` 9 passed, `tunnel_liveness` 7
passed, `exploratory_build_live` 11 passed). Plan 191
retained-stopped; Plan 188 retains its
`installed_ob/installed_ib` retention-passed status and the
corrected reply-path rows retained-passed via Plan 190; the
deferred `plans/188-m6-mixed-router-streaming-with-i2pd.md`
Streaming pass is the next executable plan. Plan 189 (Java
I2P second-family qualification) depends on Plan 188 + Plan
192 + Streaming closing first. M10 final acceptance stays
open.**
