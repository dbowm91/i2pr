# i2pr plans

Plan-of-record and closure/status records for `i2pr`. Closure/status records
(`NNN-status.md`) are authoritative; per-plan narratives are historical context
unless the newest status points to them as executable work.

## Current authority

The current **Milestone 6 local closure authority** remains
[**Plan 134**](134-status.md)
(`passed-milestone6-recv-window-ack-ceiling-closure`). Independent-router
destination/Streaming/tunnel interoperability is not claimed and remains
separate external acceptance debt.

The current **Milestone 7 SAM 3.1 final-acceptance authority** is
[**Plan 151**](151-status.md)
(`passed-m7-sam31-final-acceptance-evidence-correction`). Plan 152 is the
retained narrow M6 robustness corrective discovered by Plan 151 and normalized
by Plan 153.

**Milestone 8 is closed.** Plan 161 passed the independent SSU2 v2 direct
session gate in both directions against exact-pinned i2pd 2.61.0, and Plan 162
closed the narrow external-test/routine-CI lane correction.

The registered **Milestone 9 planning authority** is
[**Plan 163**](163-status.md) (`registered-m9-i2cp-roadmap`).
[**Plan 164**](164-status.md)
(`passed-m9-i2cp-protocol-and-wire-foundation`) closed the I2CP
source/profile/wire foundation. [**Plan 165**](165-status.md)
(`passed-m9-i2cp-connection-session-and-options`) closed the
runtime-neutral connection/session/options state machines.
[**Plan 166**](166-status.md)
(`passed-m9-i2cp-client-owned-destination-and-leaseset2`) closed
the client-owned destination + LeaseSet2 bridge.
[**Plan 168**](168-status.md)
(`passed-m9-i2cp-message-data-plane`) closed the M9 I2CP
message data plane. [**Plan 169**](169-status.md)
(`passed-m9-i2cp-self-composed-local-product-and-hardening`)
closed the self-composed local product and hardening pass.
[**Plan 171**](171-status.md)
(`passed-m9-i2cp-invalid-preamble-close-and-ci-corrective`)
closed the narrow invalid-preamble exact-head CI corrective:
the common per-connection terminal path now shuts the TCP stream
down explicitly before bookkeeping release, and the strict
wrong-preamble row proves the close with resource baselines plus
a non-paused companion test (retained).
[**Plan 170**](170-status.md)
(`passed-m9-i2cp-independent-clients-and-final-closure`) proved
the independent wire/data-plane gate: exact-pinned Java I2P 2.13.0 and
go-i2cp exchange digest-matched small/large payloads in both
directions through the loopback daemon under a fail-closed 9-row
lane. Its wire/data-plane evidence is retained-passed, but its
final-acceptance interpretation is superseded by Plan 172.
[**Plan 172**](172-status.md)
(`passed-m9-i2cp-independent-leaseset2-lifecycle-corrective`) passed
the independent LeaseSet2 lifecycle corrective: high-level Java
`I2PSession.connect()` plus public go-i2cp lifecycle, real non-empty
local zero-hop `RequestVariableLeaseSet`, client-signed Standard LS2 +
X25519 installs, usability gated on install, and digest-matched
bidirectional traffic after both installs (24 fail-closed rows).
Milestone 9 final acceptance is closed via Plan 172.

Current classification:

```text
plan_134 = passed
plan_146 = passed
plan_147_raw_driver = retained
plan_149 = passed-self-composing-local-product
plan_150_external_core_evidence = retained-passed
plan_150_final_acceptance = superseded-by-plan151
plan_151 = passed-m7-sam31-final-acceptance-evidence-correction
plan_152 = passed-m6-session-streaming-robustness-corrective
plan_153 = passed-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo
plan_157 = passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation
plan_158 = passed-m8-ssu2-udp-runtime-and-local-session-product
plan_159 = passed-m8-ssu2-path-validation-publication-and-transport-selection
plan_160 = passed-m8-ssu2-v2-peer-test-and-relay-reachability
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
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
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
plan_176 = passed-m10-http-i2p-proxy-and-connect
plan_177 = passed-m10-socks5-i2p-connect-proxy
plan_178 = passed-m10-irc-client-profile-and-privacy-filtering
plan_179 = passed-m10-irc-server-profile-and-authenticated-peer-hostname
plan_180 = passed-m10-service-tunnel-composition-reconcile-and-hardening
plan_181 = passed-m10-independent-application-and-service-interop-final-closure-evidence (reclassified by Plan 204 docs/CI normalization from `blocked-by-m6-mixed-router-streaming-blocker`: the two Plan 181 §6.3 remote application rows now flip `blocked → passed-on-env` through the Plan 203 positive external driver; the local 29-row matrix stays green as it was at Plan 181 close)
plan_182 = passed-m10-local-delivery-corrective
plan_183 = registered-m6-mixed-router-streaming-interop-program
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187 = blocked-by-m6-build-reply-interop-gap (5/7 destination rows flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows blocked on plan191; full inbound-delivery layer blocked on plan192)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190; 5/7 destination rows flipped; 2 inbound-delivery rows blocked on plan192; 2 ordering rows passed)
plan_189 = registered-m6-java-second-family-qualification-and-closure (blocked-by-plan188-plan190-plan191-plan192)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (3 destination rows flipped blocked -> passed; remote lane now proves external-lease-lookup-tunnel, external-ls2-publication-tunnel, external-destination-outbound)
plan_191 = stopped-by-inbound-delivery-boundary-E (4 inbound-delivery rows documented; 2 rows recorded blocked; 2 ordering rows flipped passed; narrower follow-up registered)
plan_192 = passed-m6-i2cp-wire-format-corrective (i2pd-compatible I2CP-style Data body wire-format; inbound-delivery boundary E closed)
plan_193 = passed-m6-i2pd-mixed-router-streaming-qualification (local rows passed; 33/33 external rows twice on exact head 3687189; static checker wired into CI floor)
plan_194 = retained-partial-java-qualification-sam-ls2-publication-boundary
 plan_196 = passed-m6-java-controlled-first-run-topology-corrective (out-of-tree ControlledRouter.java test-only launcher + rewritten run-java.sh + extended static checker + `router.blocklist.enable=false` controlled-launcher fix + PRIV-token SAM SESSION CREATE fix; controlled Java topology + authenticated SSU2 preflight + STYLE=RAW SAM bridge proven on the exact-pinned Java I2P 2.13.0 cache; `external-session-established-java` flipped failed -> passed; the two narrow correctives are bounded to the controlled-launcher and fail-closed at the daemon boundary)
plan_197 = passed-m6-pq-ssu2-option-support-corrective (parser-only tolerance of the SSU2 `pq` KEM-scheme option Java I2P 2.13.0 unconditionally publishes; typed Ssu2PqKem/PqCapabilities surface with bounded MAX_SSU2_PQ_SCHEMES = 8; i2pr session layer remains classical X25519 only; i2pr publication path stays pq-free; ML-KEM not implemented, claimed, or silently enabled; 21 required test rows green locally)
plan_200 = passed-m6-java-public-client-publication-observability (Java helpers decoupled `leaseset=published` from `READY` and added bounded `REPORT_STATUS`; Rust driver adds post-bootstrap RouterInfo DatabaseLookup proofs in both directions; sanitized Java log keys for the client LS2 lifecycle and tunnel/floodfill/store/ack selection are emitted to evidence; exactly one terminal `P200-{A..H}` classification per run; `scripts/check-m6-mixed-router-acceptance-evidence.sh` extended with Plan 200 §B/C/D/§11 invariants)
plan_201 = in-progress-branch-a-corrective-landed-branch-g-framework-unchanged (this run consumed the first Plan 200 / Plan 201 exact-head external run against the exact-pinned Java I2P 2.13.0 cache (`9134f808337b401e8e53c73734c81fab04280c9d`, staged unmodified via `bash scripts/interop/fetch-m6-java.sh --rebuild`); the terminal `P200-B-router-b-missing-router-a` classification was recorded consistently across three sequential exact-head runs; the corresponding Plan 201 §3 Branch A corrective landed as a narrow test-driver fix — `decode_inbound_i2np` helper that mirrors the production dispatcher's standard-first / short-transport-fallback ordering and the probe-side gzip decompression that matches `i2pr_netdb::decompress_router_info`; the one-direction `p200-routerinfo-lookup-a-knows-b` row flipped `false → passed` (Branch A proof) while the asymmetric `b-knows-a` direction stays on the Java B / i2pr bootstrap-storage asymmetry side per Plan 201 §3 Branch A's "do not change i2pr production NetDB code unless the captured transcript proves i2pr encoded an invalid I2NP message" rule; the Branch G `store-acked-remote-lookup-fails` corrective framework (eleven sanitized observation counters on `DestinationTunnelCounters` + public `note_lookup_boundary(label, value)` typed observation surface + eight new `plan201_g_*` unit rows + six new `blocked_row` Plan 201 §G entries in `run-java.sh` + static checker `scripts/check-m6-mixed-router-acceptance-evidence.sh` §11 + §12 invariants) is retained as the documented attribution surface for any future `P200-G` exact-head run; the seven §11 stop rows stay `blocked` with the documented Plan 198/199 stop provenance until the Java-side LeaseSet2 publication gap closes; the M6 Java second-family claim stays `not-yet-passed`)
plan_202 = partial-m10-remote-routing-capability-surface-superseded-by-plan206 (Plan 206 §5 promoted the marker/counter capability into an executable backend; Plan 208 wired the executable backend into the production `deliver_outbound` sweep; the structural scaffolding — typed `RoutingDecision`, bounded `RemoteDeliveryCounters`, manager installation surface — is retained underneath Plan 206/208)
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop (positive Direction A external driver exercises the same M6 interop lane Plan 202 uses; typed Plan 203 §5/§6 observation set; static checker rejects literal `record "... passed"` and requires the documented evidence keys; two `remote-independent-*` rows flip `blocked → passed-on-env` once the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple; the full M10 lane stays fail-closed without it)
plan_205 = in-progress-registered-blocked-on-plan198-publication-boundary (the documented next executable plan after Plan 201; rewrites the Java second-family helpers from direct I2CP to the public SAM bridge path mirroring what i2pd's SAM bridge does successfully in the Plan 202/203 reference destination path; three-router topology retained from Plan 201 Branch C/D; no public I2P, no Java patching, no i2pr production wire change)
plan_206 = retained-partial-executable-backend-seams-superseded-by-plan208 (the executable `ServiceDestinationDelivery` backend attaches a shared `RemoteDestinationBackend`; Plan 208 promotes the typed backend into the production `deliver_outbound` sweep; the structural scaffolding — typed `RoutingDecision`, bounded `RemoteDeliveryCounters`, manager installation surface — is retained underneath Plan 208)
plan_208 = passed-m10-production-delivery-driver-remote-route-integration (`crates/i2pr-daemon/src/service_tunnels.rs::deliver_outbound` invokes the typed `route_outbound_remote_request` seam on the local-miss branch; `route_outbound_remote_request` performs a real send through `StreamingDestinationAdapter` + `deliver_outbound_cells` + `RouterDeliveryService`; the inbound-owner registry wires inbound data to the actual owning service runtime; seven new `plan208_*` manager-level unit rows in `service_tunnels.rs::plan208_remote_route_integration_tests`; the new `m10_remote_route_integration_through_deliver_outbound` external driver is `#[ignore]`-gated and exercises the production sweep against the exact-pinned i2pd 2.61.0 cache; `scripts/check-service-tunnel-acceptance-evidence.sh` extended with Plan 208 §15 source-level invariants)
m10_remote_transport_core = passed-via-plan208 (Plan 202 closed the typed routing classification + bounded counters; Plan 206 attached the executable backend; Plan 208 wired the backend into the production `deliver_outbound` sweep so the manager routes its own queued Streaming requests through the real Plan 184–193 router stack — a reachable remote peer no longer dies at the pre-Plan-208 `unknown_peer` terminal branch)
next_m10_application_plan = 209 (Plan 209 cleans up the Plan 207 application driver to remove the synthetic label-injection pattern and drives the application rows through the Plan 208 production sweep; blocked on Plan 201's exact-head external run per `plans/204-status.md`)
m6_inbound_netdb_reply_path_correction = passed-via-plan190 (typed InboundGatewayRoute + daemon-owned adapter; remote lane flips 3 destination rows blocked -> passed)
m6_inbound_destination_delivery_boundary_E = closed-via-plan192 (i2pd-compatible I2CP-style Data body wire-format)
m6_inbound_destination_delivery = passed-via-plan192 (i2cp-compatible I2CP-style Data wire-format; STYLE=RAW SAM session; 9-byte short-transport inner envelope)
m6_i2pd_mixed_router_streaming_qualification = passed-via-plan193 (33/33 external rows twice on exact head 3687189; static checker wired)
m6_ssu2_pq_option_tolerance = landed-via-plan197-typed-parser-surface (Ssu2RouterAddress::parse accepts Java `pq=4,3`; typed PqCapabilities surfaced on every parsed address via `pq_capabilities()` accessor; first-family i2pd 2.61.0 lane stays green because i2pd does not publish pq)

milestone7_local_product = passed-via-plan149
milestone7_sam_localhost = passed-via-plan151
milestone7_sam_localhost_final_acceptance = closed
sam_independent_clients = at-least-two-passed-via-plan150
milestone6_interoperable = not-yet-claimed

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

milestone9_planning_authority = plan163
milestone9_protocol = i2cp
milestone9_wire_foundation = passed-via-plan164
milestone9_connection_session_options = passed-via-plan165
milestone9_client_owned_destination = passed-via-plan166
milestone9_i2cp_loopback_server_runtime = passed-via-plan167
milestone9_i2cp_message_data_plane = passed-via-plan168
milestone9_i2cp_self_composed_local_product = passed-via-plan169
milestone9_i2cp_invalid_preamble_close = passed-via-plan171
milestone9_i2cp_independent_wire_data_plane = passed-via-plan170
milestone9_i2cp_independent_clients = passed-via-plan170-and-plan172
milestone9_i2cp_independent_leaseset2 = passed-via-plan172
milestone9_final_acceptance = closed-via-plan172
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = passed-via-plan176
milestone10_socks5 = passed-via-plan177
milestone10_irc_client = passed-via-plan178
milestone10_irc_server = passed-via-plan179
milestone10_local_product = passed-via-plan180-and-plan182
milestone10_local_roundtrip = passed-via-plan182
milestone10_independent_application_clients = passed-via-plan181-and-plan203 (Plan 181 §6.3 retained local rows remain green; the two remote application rows now flip `blocked → passed-on-env` through the Plan 203 positive external driver when the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple and the driver emits the `http-remote-application-established` / `irc-remote-application-established` evidence keys)
milestone10_remote_service_interop = passed-via-plan202-plan203-plan206-and-plan208 (Plan 202 closed the typed routing classification + bounded counters; Plan 203 added the positive M10 remote HTTP eepsite + IRC service rows against the i2pd-owned SAM STREAM destinations; Plan 206 attached the executable `RemoteDestinationBackend` to the `ServiceDestinationDelivery` capability; Plan 208 wired the executable backend into the production `deliver_outbound` sweep so the manager routes its own queued Streaming requests through the real Plan 184–193 router stack — the typed `route_outbound_remote_request` / `dispatch_inbound_to_owned_destination` / `register_inbound_destination_owner` / `resolve_remote_lease_set2` seams cover both Direction A (client tunnel) and Direction B (server tunnel) without a parallel test-owned stack)
milestone10_final_acceptance = not-yet-closed (Plan 208 closed the production delivery-driver remote-route integration; Plan 209 owns the cleanup of the Plan 207 application driver (removal of the synthetic label-injection pattern, drive the application rows through the Plan 208 production sweep); the milestone can only close via Plan 204 once Plan 201 records the terminal `P200-{A..H}` classification and lands its narrow corrective — see the §7/§12 authority transitions in `204-m10-final-closure-evidence-authority-and-documentation-normalization.md`)
m6_mixed_router_program = registered-via-plan183
m6_authenticated_i2np_preflight = passed-via-plan184
m6_exploratory_one_hop_tunnels = passed-via-plan185
m6_netdb_lookup_publication = passed-via-plan186
m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-publication-outbound-passed-inbound-delivery-passed-via-plan192
m6_inbound_netdb_reply_path_correction = passed-via-plan190 (typed InboundGatewayRoute + daemon-owned adapter; remote lane flips 3 destination rows blocked -> passed)
m6_inbound_destination_delivery = passed-via-plan192 (i2cp-compatible I2CP-style Data wire-format; STYLE=RAW SAM session; 9-byte short-transport inner envelope)
m6_mixed_router_cross_family_ledger = retained-partial-via-plan193-and-plan194 (Plan 198 final closure gate remains blocked; pq-parser-tolerance-landed-via-plan197)
 m6_ssu2_pq_option_tolerance = landed-via-plan197-typed-parser-surface (Ssu2RouterAddress::parse accepts Java `pq=4,3`; typed PqCapabilities surfaced on every parsed address via `pq_capabilities()` accessor; first-family i2pd 2.61.0 lane stays green because i2pd does not publish pq)
m6_java_second_family_qualification = public-client-corrective-diagnosed-via-plan200-and-blocked-on-plan201 (Plan 198 helpers connect; Plan 200 closed the diagnostic/evidence side with one terminal `P200-{A..H}` classification per run; Plan 201 landed the Branch G corrective framework; downstream rows stay blocked until the Plan 200 exact-head external run consumes the `P200-*` classification and the matching Plan 201 branch lands)
 milestone6_java_mixed_router_interop = not-yet-passed (Plan 200 close + Plan 201 Branch-G framework landed; final closure blocked on the exact-head external run)
 milestone6_interoperable = not-yet-claimed
  next_product_layer = m10-unified-final-closure-blocked-at-plan201-java-branch-only (the M10 production remote transport is passed via Plan 208; the positive remote application interop is passed via Plan 203; the Plan 209 cleanup + Plan 204 final closure transition remain; both are blocked on Plan 201's exact-head external run)
  next_executable_plan = 201-branch-g-finalize (Plan 200 exact-head external run consumes the P200 classification); Plans 200/202/203/206/208 are already passed; Plan 204 landed the docs/CI normalization pass and remains blocked on Plan 201 per `plans/204-status.md`
m9_sequence = 164 -> 165 -> 166 -> 167 -> 168 -> 169 -> 171 -> 170 -> 172
m10_sequence = 173 -> 174 -> 175 -> 176 -> 177 -> 178 -> 179 -> 180 -> 182 -> 181(passed via plan203 promotion) -> 183 -> 195(reactivated) -> 202 -> 203 -> 206(executable-remote-backend-attached) -> 208(production-deliver_outbound wired through the typed backend) -> 204(docs-and-authority-normalization-pass-landed; final closure transitions deferred until plan201 closes)
  m6_sequence = 183(registered) -> 184(passed) -> 185(passed) -> 186(passed) -> 187(blocked-local-passed-5/7-flipped) -> 188(blocked-by-plan191; installs retained-passed via plan188) -> 190(passed-locally-and-remote-3-rows-flipped) -> 191(retained-stopped; inbound-delivery boundary E) -> 192(passed; inbound-delivery boundary E closed via i2cp-wire-format-corrective) -> 193(passed; M6 i2pd mixed-router Streaming qualification; 33/33 rows twice on exact head 3687189) -> 196(passed; controlled Java topology + authenticated SSU2 preflight + STYLE=RAW SAM bridge) -> 197(passed; PQ SSU2 option parser tolerance; 21 required test rows green locally) -> 194(retained-partial; SAM LS2-publication boundary) -> 198(superseded; decomposed into plans200-204) -> 200(passed; diagnostic/evidence side) -> 201(in-progress branch-g-framework landed; final closure blocked on exact-head external run) -> 204(docs-and-authority-normalization-pass landed; final closure transitions deferred until plan201 closes)
```

## Current handoff sequence

### Milestone 8 — SSU2 v2 (closed)

- [`154-m8-ssu2-transport-and-reachability-roadmap.md`](154-m8-ssu2-transport-and-reachability-roadmap.md) — M8 roadmap/planning authority. Classical SSU2 v2 required; PQ v3/v4 compatibility debt; SSU1 unsupported.
- [`155-m8-ssu2-v2-protocol-foundation-and-addresses.md`](155-m8-ssu2-v2-protocol-foundation-and-addresses.md) — **passed** runtime-neutral address/header/block foundation.
- [`156-m8-ssu2-v2-handshake-token-and-routerinfo.md`](156-m8-ssu2-v2-handshake-token-and-routerinfo.md) — **passed** handshake/token/RouterInfo establishment.
- [`157-m8-ssu2-v2-data-phase-reliability-and-fragmentation.md`](157-m8-ssu2-v2-data-phase-reliability-and-fragmentation.md) — **passed** authenticated data/reliability/fragmentation.
- [`158-m8-ssu2-udp-runtime-and-local-session-product.md`](158-m8-ssu2-udp-runtime-and-local-session-product.md) — **passed** real localhost UDP runtime/local product.
- [`159-m8-ssu2-path-validation-publication-and-transport-selection.md`](159-m8-ssu2-path-validation-publication-and-transport-selection.md) — **passed** path validation/publication policy/transport selection.
- [`160-m8-ssu2-peer-test-and-relay-reachability.md`](160-m8-ssu2-peer-test-and-relay-reachability.md) — **passed** PeerTest/relay reachability.
- [`161-m8-ssu2-independent-ipv4-interop-and-final-closure.md`](161-m8-ssu2-independent-ipv4-interop-and-final-closure.md) — **passed** exact-pinned i2pd 2.61.0 direct-session directions A+B, authenticated I2NP exchange, cached-token/malformed rows, fail-closed evidence ledger/checker/manual workflow. This closes M8 only within its bounded direct-interop scope.
- [`162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md`](162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md) — **passed** narrow corrective isolating the environment-dependent external test from ordinary CI while preserving explicit fail-closed execution.

### Milestone 9 — I2CP (current)

- [`163-m9-i2cp-roadmap.md`](163-m9-i2cp-roadmap.md) — **registered planning authority**. Locks architecture, source/reference policy, client-owned destination ownership, loopback exposure policy, evidence model, and M9 final acceptance.
- [`164-m9-i2cp-protocol-and-wire-foundation.md`](164-m9-i2cp-protocol-and-wire-foundation.md) — **passed**. Official I2CP sources and Java/Go references pinned; honest M9 feature/API profile; runtime-neutral `i2pr-api::i2cp` bounded framing/message codecs, fixtures, and routine-CI vector checker. No sockets or sessions.
- [`165-m9-i2cp-connection-session-and-options.md`](165-m9-i2cp-connection-session-and-options.md) — **passed**. Connection/version/session state machines; canonical SessionConfig signature/date/ceiling verification; bounded option disposition table and `i2pr-client::DestinationConfig` projection; bounded `SessionRegistry` with reserve/commit/rollback; reconfiguration taxonomy; typed `I2cpAction` vocabulary. No listener or destination activation.
- [`166-m9-i2cp-client-owned-destination-and-leaseset2.md`](166-m9-i2cp-client-owned-destination-and-leaseset2.md) — central ownership pass. Add a client-owned destination mode without requiring the client's signing private key; request leases from real destination tunnels; validate/install client-signed Standard LeaseSet2 plus matching X25519 decryption key transactionally; reuse existing ECIES/routing; preserve router-owned SAM behavior.
- [`167-m9-i2cp-loopback-server-runtime.md`](167-m9-i2cp-loopback-server-runtime.md) — **passed**. Daemon-owned supervised loopback I2CP v0.9.67 listener/runtime in `crates/i2pr-daemon/src/i2cp.rs`. Disabled by default, loopback-only, bounded read/write/session resources, real-TCP `0x2a` preamble + incremental `FrameDecoder` + multi-frame dispatch, signed `SessionConfig` reservation, signed Standard LeaseSet2 + matching X25519 decryption key installation, mismatched-key rejection, disconnect cleanup, supervised per-connection `ChildScope`, single cleanup path on EOF/reset/timeout/cancel. Twelve real-TCP acceptance tests in `crates/i2pr-daemon/tests/i2cp_loopback.rs`. No `SendMessage`/`SendMessageExpires` direction, `DestLookup`/`HostLookup` resolution, reconfiguration, or independent-client evidence; those belong to Plans 168–170.
- [`168-m9-i2cp-message-data-plane.md`](168-m9-i2cp-message-data-plane.md) — **passed**. Bounded per-session `SendMessage`/`SendMessageExpires` validation against the existing `i2pr_client::DestinationRuntime::enqueue_outbound` seam (no second routing stack), bounded `MessageStatus` correlation table, `MessagePayload` inbound frames delivered only to the owning session's bounded queue and drained through a `tokio::sync::Notify` (sibling-isolation guaranteed), cross-session local loopback shortcut for two destinations owned by active I2CP sessions, `DestLookup` resolving through the local destination registry, and `GetBandwidthLimits` returning the config-derived client ceiling and the documented neutral router values. Eighteen real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` cover every Plan 168 §11 case. No reconfiguration, no `HostLookup`/`HostReply` resolution, and no independent-client evidence; those belong to Plans 169–170.
- [`169-m9-i2cp-self-composed-local-product-and-hardening.md`](169-m9-i2cp-self-composed-local-product-and-hardening.md) — transactional reconfigure/destroy, complete adversarial/resource matrix, repeated lifecycle baselines, and canonical two-destination self-composed real-TCP product driven only through I2CP after listener startup.
- [`171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md`](171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md) — **passed** narrow exact-head corrective (retained). Explicit `stream.shutdown()` on the common per-connection terminal path before bookkeeping release; strict wrong-preamble row with 24-iteration baselines plus a non-paused companion test. No wire change, no independent-client evidence.
- [`170-m9-i2cp-independent-clients-and-final-closure.md`](170-m9-i2cp-independent-clients-and-final-closure.md) — wire/data-plane **retained-passed**; final acceptance superseded by Plan 172.
- [`172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md`](172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md) — **passed** independent LeaseSet2 lifecycle corrective; M9 final acceptance closed via Plan 172.

### Milestone 10 — service tunnels (current)

- [`173-m10-service-tunnels-http-socks5-irc-roadmap.md`](173-m10-service-tunnels-http-socks5-irc-roadmap.md) — **registered planning authority**.
- [`174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](174-m10-service-tunnel-foundation-and-shared-stream-runtime.md) — **passed** runtime-neutral `i2pr-service-tunnels` crate, strict disabled-by-default loopback-only `[service_tunnels]` surface, shared daemon Streaming pump reused by SAM, no listener yet.
- [`175-m10-generic-client-server-service-tunnels.md`](175-m10-generic-client-server-service-tunnels.md) — **passed** generic client/server service tunnels and persistent server destinations.
- [`176-m10-http-i2p-proxy-and-connect.md`](176-m10-http-i2p-proxy-and-connect.md) — **passed** HTTP `.i2p` proxy + CONNECT.
- [`177-m10-socks5-i2p-connect-proxy.md`](177-m10-socks5-i2p-connect-proxy.md) — **passed** SOCKS5 `.i2p` CONNECT proxy.
- [`178-m10-irc-client-profile-and-privacy-filtering.md`](178-m10-irc-client-profile-and-privacy-filtering.md) — **passed** IRC `.i2p` client profile + privacy filter.
- [`179-m10-irc-server-profile-and-authenticated-peer-hostname.md`](179-m10-irc-server-profile-and-authenticated-peer-hostname.md) — **passed** IRC `.i2p` server profile + authenticated peer hostname projection.
- [`180-m10-service-tunnel-composition-reconcile-and-hardening.md`](180-m10-service-tunnel-composition-reconcile-and-hardening.md) — **passed** the M10 local product layer: transactional `ServiceTunnelManager::reconcile(candidate, drain_deadline)` with typed `DiffClass`, `ServiceTunnelGeneration`/`DrainingGeneration` committed-generation model, forced-drain deadline handling, unified cross-service resource accounting matrix, and the static `scripts/check-service-tunnel-boundaries.sh` checker.
- [`182-m10-local-delivery-corrective.md`](182-m10-local-delivery-corrective.md) — **passed** the M10 local-delivery corrective: per-destination delivery drivers over the Plan 129 `bridge_to_peer` seam, wildcard Streaming port 0, SAM-parity accept paths, direction-branched pump sends, completed IRC client executor, orderly pump half-close, and active-slot hygiene. Nine round-trip tests plus six wire-surface tests prove the local byte round-trip.
- [`181-m10-independent-application-and-service-interop-final-closure.md`](181-m10-independent-application-and-service-interop-final-closure.md) — **blocked** by the retained M6 mixed-router Streaming debt: 29 local independent-application-client rows pass (unmodified curl/nc/stdlib/jaraco-irc, restart stability, baselines, ledger) while the two remote rows are recorded `blocked` with genuine i2pd-2.61.0 qualification provenance. Milestone 10 final acceptance stays open.
- [`183-m6-mixed-router-streaming-interop-program.md`](183-m6-mixed-router-streaming-interop-program.md) — **registered** the M6 mixed-router program Plan 181 §6.3 requires; Plan 181 resumes after it produces passing remote rows.
- [`184-m6-authenticated-i2np-runtime-and-reference-preflight.md`](184-m6-authenticated-i2np-runtime-and-reference-preflight.md) — **passed** the first executable Plan 183 pass: strict loopback/non-advertised daemon SSU2 activation, central authenticated router-I2NP dispatcher, narrow outbound delivery over existing `send_i2np`, exact-pinned i2pd 2.61.0 bidirectional control with fail-closed 10-row lane. No tunnel/NetDB/Streaming claim; Plan 185 owns the first Short Tunnel Build.
- [`185-m6-live-one-hop-exploratory-tunnels-and-liveness.md`](185-m6-live-one-hop-exploratory-tunnels-and-liveness.md) — **passed** the live one-hop exploratory tunnel lane: daemon-owned `ExploratoryBuildCoordinator` + `TunnelLivenessScheduler` route the existing `ShortBuildStateMachine` / `ExploratoryPool` / `DataPlaneRegistry` seams end-to-end through the Plan 184 central dispatcher; one-hop outbound (i2pr OBGW → i2pd OBEP) and one-hop inbound (i2pd IBGW → i2pr endpoint) builds accepted by the exact-pinned i2pd 2.61.0 reference with `notransit=false`; bounded first-test / repeat / response-timeout / failure-threshold liveness policy. Local 15-row unit + 9-row two-daemon-pair suite + 12-row external lane plus `scripts/check-exploratory-tunnel-evidence.sh`. No multi-hop, no destination LeaseSet2 / Streaming claim; Plan 186 owns the live NetDB lookup / publication program.
- [`186-m6-mixed-router-netdb-lookup-and-publication.md`](186-m6-mixed-router-netdb-lookup-and-publication.md) — **passed** the live mixed-router NetDB lookup / publication program: the daemon-owned `NetDbTunnelCoordinator` drives the existing lookup/publication state machines over the Plan 185 pair through the ordinary authoritative store (exact-pinned i2pd floodfill, 12-row lane plus `scripts/check-netdb-tunnel-evidence.sh`). No destination LeaseSet2 / Streaming claim; Plan 187 landed the destination program (local rows passed, remote gate pending Plan 188).
- [`187-m6-remote-leaseset2-and-destination-garlic-routing.md`](187-m6-remote-leaseset2-and-destination-garlic-routing.md) — **blocked** by the `m6-build-reply-interop-gap` (2/7 flipped via Plan 188 installs; 5/7 were blocked on a separate inbound NetDB reply-path metadata defect that Plan 190 isolates and corrects): the daemon-owned `DestinationTunnelCoordinator` with the full local destination message plane is landed (27 unit + 9 live two-role rows, including the bidirectional ECIES/Garlic round-trip with sibling isolation over real TunnelData cells), and the external lane proves session, reference build acceptance both directions, SAM DATAGRAM destination, and reference LS2 publication; installs now complete through consumed reference replies (garlic-wrapped endpoint + forwarded gateway) with 2/7 rows passed (21-row lane plus `scripts/check-destination-tunnel-evidence.sh`). No LeaseSet2/Streaming interop claim yet.
- [`188-m6-short-build-reply-interop-corrective.md`](188-m6-short-build-reply-interop-corrective.md) — narrow corrective (see [`188-status.md`](188-status.md)): outbound garlic-unwrap + inbound forwarded-STBM consumption landed with 2/7 rows flipped to passed (`installed_ob=1 installed_ib=1` via consumed replies, no synthesis); 5/7 were blocked on a separate inbound NetDB reply-path metadata defect that Plan 190 isolates and corrects. The historical duplicate-numbered `188-m6-mixed-router-streaming-with-i2pd.md` Streaming file remains historical context only and is superseded by [`193-m6-i2pd-mixed-router-streaming-qualification.md`](193-m6-i2pd-mixed-router-streaming-qualification.md).
- [`190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md`](190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md) — **passed** the inbound NetDB reply-path tunnel-ID corrective (see [`190-status.md`](190-status.md)): typed public `InboundGatewayRoute` (`gateway_router`, `gateway_receive_tunnel`, `local_receive_tunnel`) retained by `i2pr-tunnel::DataPlaneRegistry`; daemon-owned `reply_path_for_inbound_route` adapter derives `i2pr_netdb::ReplyPath` only from `(gateway_router, gateway_receive_tunnel)`. Local regression rows (`destination_tunnel_unit` 32 passed, `destination_tunnel_live` 9 passed, `exploratory_build_live` 11 passed) prove unequal IDs (`0x9601` vs `0x9602`) round-trip through the I2NP codec with the gateway tuple on the wire, and that lifecycle removal cleans the typed route atomically. The exact-pinned i2pd external destination lane now advertises the corrected reply path; the row flips from `blocked` to `passed` only after a fresh external `run-destination.sh` proves a real tunneled lookup response arrives. No M6 wire change; no `milestone6_interoperable = passed-via-plan190` claim.
- [`189-m6-java-i2p-second-family-qualification-and-closure.md`](189-m6-java-i2p-second-family-qualification-and-closure.md) — **registered** M6 Java I2P second-family qualification and mixed-router closure plan (see [`189-status.md`](189-status.md)): executable-next-via-plan194 (and Plan 188/190). Plan 189 §8 lands the fail-closed M6 mixed-router cross-family ledger/checker/workflow scaffold: `scripts/check-m6-mixed-router-acceptance-evidence.sh` (structural checker that pins both i2pd 2.61.0 and Java I2P 2.13.0), `tests/integration/m6-interop/run-m6-mixed-router.sh` (cross-family aggregator that reuses the four per-layer harnesses and binds every guarded row to a family + a per-layer exit code), and `.github/workflows/m6-mixed-router-external.yml` (manual `workflow_dispatch` lane). The second-family Java rows are recorded `failed` with stop provenance until Plan 194 lands the Java qualification harness under `tests/integration/m6-interop/run-java.sh`; no M6 wire change; no `milestone6_interoperable = passed-via-plan189` claim.
- [`191-m6-inbound-destination-delivery-boundary.md`](191-m6-inbound-destination-delivery-boundary.md) — **stopped** (see [`191-status.md`](191-status.md)): Plan 190 closed the inbound NetDB reply-path tunnel-ID corrective and flipped three destination rows `blocked` → `passed` in a fresh external `run-destination.sh` against exact-pinned i2pd 2.61.0; Plan 191 owned the next demonstrated boundary (Plan 190 §6 stop condition E) — the reference-side inbound delivery of the destination message to the i2pd-owned SAM bridge plus the destination-side ordering rows — and stopped at the i2pd-compatible I2CP-style Data body wire-format defect (boundary E).
- [`192-m6-i2cp-wire-format-corrective.md`](192-m6-i2cp-wire-format-corrective.md) — **passed** the M6 i2pd-compatible I2CP-style Data body wire-format corrective (see [`192-status.md`](192-status.md)): narrow protocol-layer corrective that switched the inner I2NP envelope inside the ECIES-X25519 Garlic clove from the 16-byte standard form to the 9-byte NTCP2/SSU2 short-transport form (`Garlic.cpp:1023-1028`) and wrapped the application payload in the I2CP-style Data body i2pd expects (`length[4 BE] + reserved[4] + fromPort[2 BE] + toPort[2 BE] + padding[1] + protocol[1] + gzip-no-compression-wrapped payload`; `Destination.cpp:1192-1236`); test driver switched from `STYLE=DATAGRAM` (needs a 384-byte ElGamal/DSA `from` Identity) to `STYLE=RAW` (uses the inbound destination hash instead of an ElGamal/DSA identity); the 2 inbound-delivery rows (`external-reference-received`, `external-destination-inbound`) flip from `blocked` to `passed` in a fresh `run-destination.sh` against the exact-pinned i2pd 2.61.0. No M6 wire change beyond the destination message-plane seam; no `LocalZeroHop` substitution; no authentication weakening.
- [`193-m6-i2pd-mixed-router-streaming-qualification.md`](193-m6-i2pd-mixed-router-streaming-qualification.md) — **passed** the M6 i2pd mixed-router Streaming qualification plan (see [`193-status.md`](193-status.md) and [`193-streaming-status.md`](193-streaming-status.md)): closed (`passed-m6-i2pd-mixed-router-streaming`; 15 unit + 11 live two-role rows in `crates/i2pr-daemon/tests/streaming_tunnel_unit.rs` + `crates/i2pr-daemon/tests/streaming_tunnel_live.rs`; one fail-closed external driver `crates/i2pr-daemon/tests/streaming_tunnel_external.rs::streaming_through_i2pd` with `#[ignore]` gating; one runner `tests/integration/m6-interop/run-streaming.sh`; one static evidence checker `scripts/check-streaming-tunnel-evidence.sh` guarding 33 labels, wired into the CI floor). External lane passed twice on exact head 3687189 (33/33 rows, `milestone6_i2pd_streaming_interop = passed-via-plan193`). Plan 198 is the current executable corrective for the Java public-client boundary.
- [`194-m6-java-second-family-mixed-router-closure.md`](194-m6-java-second-family-mixed-router-closure.md) — **retained-partial-java-qualification-sam-ls2-publication-boundary** (see [`194-status.md`](194-status.md)): the topology, authenticated SSU2, and bounded SAM compatibility evidence remain valid, but the blocked lookup/delivery/Streaming rows were not final-closure passes. Plan 198 supersedes the old bounded-closure interpretation with a public-client corrective.
- [`198-m6-java-public-client-final-closure-corrective.md`](198-m6-java-public-client-final-closure-corrective.md) — **blocked-public-java-client-leaseset2-publication** (see [`198-status.md`](198-status.md)): implements public `I2PClient`/`I2PSession` and `I2PSocketManager` reference helpers, wires the existing Rust destination/Streaming drivers to those helpers, and adds the evidence-consuming final closure gate. The exact-pinned Java 2.13.0 helpers connect and both Rust driver processes return `ok`, but the controlled Java router does not return the public-client LeaseSet2 to the real i2pr DatabaseLookup path; mandatory Java lookup, destination delivery, and Streaming rows remain blocked fail-closed. Plan 195 remains gated.
- [`196-m6-java-controlled-first-run-topology-corrective.md`](196-m6-java-controlled-first-run-topology-corrective.md) — **passed-m6-java-controlled-first-run-topology-corrective** the M6 Java I2P controlled first-run topology corrective (see [`196-status.md`](196-status.md)): lands the out-of-tree `tests/integration/m6-interop/java/ControlledRouter.java` test-only launcher that compiles against the staged Java I2P `lib/` jars and invokes the stock public `net.i2p.router.Router(Properties)` + `setKillVMOnEnd(false)` + `runRouter()` lifecycle (the exact-pinned upstream `MultiRouter` precedent); rewrites `tests/integration/m6-interop/run-java.sh` at the topology/startup boundary (reserves fixed loopback Java SSU2 / SAM / I2CP ports, drives the controlled `Properties` set with exact-pinned upstream names, writes a disposable `clients.config` containing only the SAM bridge, never mutates `${JAVA_CACHE}/clients.config`, passes actual selected endpoints to `java_tunnel_external.rs`, asserts every controlled-topology invariant); extends the static checker `scripts/check-m6-mixed-router-acceptance-evidence.sh` to reject `i2p.vmCommSystem=true`, the obsolete Plan 194 keys (`i2np.reseed.enable`, `router.isFloodfill`, `i2np.ntcp2.enabled`), mutation of the verified Java cache's `clients.config` / `clients.config.d`, non-loopback reseed URLs, and `|| true` forgiveness in the lane. Two narrow correctives (`router.blocklist.enable=false` to bypass the Team Cymru bogon `127.0.0.0/8` entry, and PRIV-token SAM SESSION CREATE because Java strictly requires ≥ 663 decoded bytes while i2pd accepts the 391-byte PUB) are bounded to the controlled-launcher and fail-closed at the daemon boundary. Plan 196 is topology-only and is now superseded by Plan 194's second-family qualification.
- [`197-m6-pq-ssu2-option-support-corrective.md`](197-m6-pq-ssu2-option-support-corrective.md) — **passed-m6-pq-ssu2-option-support-corrective** the M6 PQ SSU2 option support corrective (see [`197-status.md`](197-status.md)): parser-only tolerance of the SSU2 `pq` KEM-scheme option that exact-pinned Java I2P 2.13.0 `UDPTransport.addSSU2Options` unconditionally publishes (`pq=4,3` for ML-KEM-768+ML-KEM-512, `pq=3` at low MTU); adds a typed `Ssu2PqKem` enum (`MlKem512`, `MlKem768`, `Unknown(u8)`) and a bounded `PqCapabilities` value surfaced on every parsed `Ssu2RouterAddress` via `pq_capabilities()`, with bounded `MAX_SSU2_PQ_SCHEMES` (`8`). The i2pr session layer remains classical X25519 only by the Plan 156/160/161 establishment contract; the i2pr publication path stays pq-free (Plan 197 adds a `publication_never_emits_pq` regression that asserts the option set never contains `pq`); `Ssu2AddressError::InvalidOptionValue { option: PQ_OPTION }` is the single new error arm; the existing i2pd 2.61.0 first-family lane must remain green because i2pd 2.61.0 does not publish `pq`. The first counted external Java run on the Plan 196 controlled topology now records `session-established` against the exact-pinned Java cache. No ML-KEM implementation is added; no wire-format change beyond the new `pq` arm in the parser.

Milestone 9 architecture is deliberately constrained:

```text
i2pr-api::i2cp
  runtime-neutral framing / messages / connection + session state
          ↓ typed actions
      i2pr-daemon
  only I2CP TCP / Tokio / task owner
          ↓
      i2pr-client
  single destination / tunnel / ECIES / routing product
```

I2CP ownership differs from SAM intentionally:

```text
SAM:  router owns destination signing + decryption secrets and signs LeaseSet2
I2CP: client proves Destination ownership by signed SessionConfig,
      supplies signed Standard LeaseSet2 + required decryption key,
      router never requires the client's destination signing private key
```

For M9, I2CP remains experimental, disabled by default, and loopback-only. Non-loopback exposure/TLS/auth is deferred. HTTP/SOCKS/IRC/generic service tunnels remain M10.

## MVP roadmap

- [`000-mvp-roadmap.md`](000-mvp-roadmap.md) — milestone sequence from repository foundation through the first feature-complete MVP.
- [`163-m9-i2cp-roadmap.md`](163-m9-i2cp-roadmap.md) — current Milestone 9 detailed roadmap.
- [`154-m8-ssu2-transport-and-reachability-roadmap.md`](154-m8-ssu2-transport-and-reachability-roadmap.md) — closed Milestone 8 roadmap.
- [`145-m7-sam31-remaining-gap-corrective-roadmap.md`](145-m7-sam31-remaining-gap-corrective-roadmap.md) — historical Milestone 7 corrective umbrella.
- [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md) — destination / garlic / LeaseSet2 / Streaming construction.
- [`126-129-milestone6-final-corrective-roadmap.md`](126-129-milestone6-final-corrective-roadmap.md) and [`126-130-milestone6-final-corrective-roadmap.md`](126-130-milestone6-final-corrective-roadmap.md) — historical Milestone 6 final corrective roadmaps.

## Milestone 7 plan hierarchy

| Plan | Current authority status | Record |
| --- | --- | --- |
| 135 | superseded by later audits/corrective roadmaps | [`135-status.md`](135-status.md) |
| 136 | foundation landed; broader claims superseded | [`136-status.md`](136-status.md) |
| 137 | passed loopback server/session lifecycle | [`137-status.md`](137-status.md) |
| 138 | implementation landed; acceptance superseded | [`138-status.md`](138-status.md) |
| 139 | FORWARD/naming retained; final matrix closed by Plan 151 | [`139-status.md`](139-status.md) |
| 140 | blocked closure audit; historical | [`140-status.md`](140-status.md) |
| 141 | historical corrective roadmap | [`141-status.md`](141-status.md) |
| 142 | Base64 correction retained | [`142-status.md`](142-status.md) |
| 143 | local delivery seam retained | [`143-status.md`](143-status.md) |
| 144 | partial local in-process handshake evidence | [`144-status.md`](144-status.md) |
| 145 | historical corrective umbrella | [`145-status.md`](145-status.md) |
| 146 | **passed** private-destination reference requalification | [`146-status.md`](146-status.md) |
| 147 | raw-driver implementation retained; broad acceptance superseded | [`147-status.md`](147-status.md) |
| 148 | blocked historical audit | [`148-status.md`](148-status.md) |
| 149 | **passed** self-composing SAM local product | [`149-status.md`](149-status.md) |
| 150 | external-client core evidence **retained passed**; final acceptance superseded | [`150-status.md`](150-status.md) |
| 151 | **passed** final M7 SAM localhost acceptance | [`151-status.md`](151-status.md) |
| 152 | **passed** narrow M6 session/Streaming robustness corrective | [`152-status.md`](152-status.md) |
| 153 | **passed** post-M7 authority/CI hygiene | [`153-status.md`](153-status.md) |

## Milestone 6 plan hierarchy

| Plan | Status | Closure |
| --- | --- | --- |
| 119 | `passed-leaseset2-protocol-foundation` | [`119-status.md`](119-status.md) |
| 120 | `passed-destination-lifecycle-and-pools` | [`120-status.md`](120-status.md) |
| 121 | `superseded-by-126` | [`121-status.md`](121-status.md) |
| 122 | `passed-corrected-local-destination-routing` | [`122-status.md`](122-status.md) |
| 123 | `passed-corrected-streaming-wire-local` | [`123-status.md`](123-status.md) |
| 124 | `passed-plan122-corrective-closure` | [`124-status.md`](124-status.md) |
| 125 | `superseded-by-final-corrective-closure` | [`125-status.md`](125-status.md) |
| 126 | `passed-ecies-destination-ratchet-corrective-foundation` | [`126-status.md`](126-status.md) |
| 127 | `passed-destination-session-routing-final-closure` | [`127-status.md`](127-status.md) |
| 128 | `passed-streaming-wire-protocol-corrective-closure` | [`128-status.md`](128-status.md) |
| 129 | superseded by later final gates | [`129-status.md`](129-status.md) |
| 130 | superseded by later final gates | [`130-status.md`](130-status.md) |
| 131 | superseded by later final gates | [`131-status.md`](131-status.md) |
| 132 | implementation evidence superseded by Plan 133 | [`132-status.md`](132-status.md) |
| 133 | evidence authority superseded by Plan 134 | [`133-status.md`](133-status.md) |
| 134 | **current Milestone 6 local authority** | [`134-status.md`](134-status.md) |

Plan 152 is a later M6 robustness correction discovered by the Plan 151 final SAM acceptance tests. It does not broaden Plan 134 into a mixed-router M6 interoperability claim.

## What's implemented / accepted now

- Bounded protocol codecs and cryptographic wrappers.
- Persistent identity/configuration/runtime foundations.
- Local NetDB and exploratory tunnel substrate.
- Local destination lifecycle, signed LeaseSet2, ECIES destination session layer, routing, and Streaming core.
- Milestone 6 local product correctness closed via Plan 134, with Plan 152 robustness corrections retained.
- SAM 3.1 parser/session/STREAM/FORWARD/NAMING product with independent localhost client evidence; M7 closed via Plan 151.
- SSU2 v2 local protocol/runtime/reachability product and independent direct IPv4 i2pd interop; M8 closed via Plan 161.
- M9 I2CP implementation plans are registered; the Plan 164 wire/profile foundation, Plan 165 connection/session/options state machines, Plan 166 client-owned destination + LeaseSet2 bridge, Plan 167 loopback server runtime, Plan 168 message data plane, and Plan 169 self-composed local product are landed, with the Plan 171 invalid-preamble close corrective retained on the common terminal path. Plan 170 wire/data-plane evidence is retained-passed (exact-pinned Java I2P 2.13.0 + go-i2cp, digest-matched payloads both directions, fail-closed 9-row lane); its final-acceptance interpretation is superseded by Plan 172. Plan 172 passed the independent LeaseSet2 lifecycle corrective (high-level Java connect + public Go lifecycle, non-empty zero-hop requests, client-signed LS2 installs, post-LS2 bidirectional digests, 24 fail-closed rows). No `HostLookup`/`HostReply` resolution; Milestone 9 final acceptance is closed via Plan 172 (experimental, loopback-only).
- M10 service-tunnel foundation is landed via Plan 174 (runtime-neutral `i2pr-service-tunnels` crate, strict `[service_tunnels]` surface, shared Streaming pump reused by SAM) plus the Plan 175 generic client/server tunnels, Plan 176 HTTP proxy, Plan 177 SOCKS5 proxy, Plan 178 IRC client profile, Plan 179 IRC server profile, Plan 180 composition/reconcile/hardening, Plan 182 local-delivery corrective, Plan 202 typed routing classification + bounded remote counters, Plan 206 executable `ServiceDestinationDelivery` backend, and Plan 208 production delivery-driver remote-route integration. Local product + byte round-trip are passed via Plans 180-and-182 (9 round-trip + 6 wire-surface tests); Plan 181 local independent-application-client rows pass with remote rows promoted by Plan 203; Plan 206 manager-level tests lock the typed remote composition seam; Plan 208 widens the integration by wiring the typed backend into the production `deliver_outbound` sweep so a reachable remote peer no longer dies at the pre-Plan-208 `unknown_peer` terminal branch (real `StreamingDestinationAdapter` + `deliver_outbound_cells` + `RouterDeliveryService` chain drives the cells through the existing router stack).
- M6 mixed-router program is registered via Plan 183; Plan 184 passed the authenticated I2NP runtime and reference preflight (strict daemon SSU2 activation, central dispatcher, narrow delivery, exact-pinned i2pd bidirectional control, 10 fail-closed rows). Plan 185 passed the live one-hop exploratory tunnel lane: the bounded Plan 185 exploratory build coordinator + the bounded creator-side liveness scheduler drive the existing `i2pr-tunnel::short::ShortBuildStateMachine` and `i2pr-tunnel::pool::ExploratoryPool` / `i2pr-tunnel::data_plane_registry::DataPlaneRegistry` seams end-to-end, with one real outbound and one real inbound one-hop build accepted by the exact-pinned i2pd 2.61.0 reference (`TransitTunnel: endpoint N created` and `TransitTunnel: gateway N created` log evidence). Plan 186 passed the live mixed-router NetDB lookup / publication program: the daemon-owned `NetDbTunnelCoordinator` drives the existing lookup/publication state machines over the Plan 185 pair through the ordinary authoritative store (exact-pinned i2pd floodfill, 12-row lane).

## What's not yet accepted

- Non-loopback/remote I2CP, TLS/authentication, or broad historical I2CP feature compliance.
- Live/public NTCP2 or SSU2 router transport activation and broad mixed-router interoperability.
- Public I2P participation and network-transport-bound NetDB/public router behavior.
- Milestone 6 independent-router destination/Streaming/tunnel interoperability (Plan 183 program registered, Plan 184 preflight passed with no tunnel/NetDB/Streaming claim; Plan 185 one-hop builds accepted with reply consumption unproven; Plan 186 NetDB rows passed; Plan 187 local destination rows passed with 7 remote rows blocked on the build-reply gap pending Plan 188; Plan 188 short-build-reply corrective landed 2/7 destination rows flipped via consumed-reference installs and 5/7 blocked on the inbound NetDB reply-path metadata defect that Plan 190 isolates and corrects; Plan 190 local rows passed (typed `InboundGatewayRoute` + daemon-owned adapter, no wire change); Plan 191 stopped at inbound-delivery boundary E and the 2 inbound-delivery rows flipped blocked -> passed via Plan 192 i2cp-wire-format-corrective; Plan 192 inbound-delivery layer closed for i2pd 2.61.0; Plan 189 §8 cross-family ledger/checker/workflow landed and second-family Java rows recorded `failed` until Plan 194 lands the Java qualification harness; Plan 193 closed the i2pd mixed-router Streaming qualification (33/33 external rows twice on exact head 3687189); Plan 194 retained as the historical partial Java qualification; Plan 196 closed the controlled Java topology + authenticated SSU2 preflight + STYLE=RAW SAM bridge; Plan 197 closed the PQ SSU2 option parser tolerance; Plan 198 superseded and decomposed into Plans 200–204; Plan 200 closed the Java diagnostic/evidence side with one terminal `P200-{A..H}` classification per run; Plan 201 landed the Branch G corrective framework; the seven §11 stop rows flip blocked -> passed only after the Plan 200 exact-head external run consumes the P200-* classification and the matching Plan 201 branch lands).
- Milestone 10 independent application/service interoperability (Plan 181 retained local 29-row matrix is green; Plan 202 closed the M10 production remote Destination/Streaming composition through the typed `ServiceDestinationDelivery` capability + `RoutingDecision::RemoteRouter` + `RemoteDeliveryCounters`; Plan 203 promoted the two retained Plan 181 §6.3 remote HTTP/IRC application rows to positive evidence through the `m10_positive_remote_http_and_irc_application_interop` Direction A external driver against exact-pinned i2pd 2.61.0 + i2pd-owned SAM STREAM destinations; the two `remote-independent-*` rows flip `blocked -> passed-on-env` once the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple and the driver emits the `http-remote-application-established` / `irc-remote-application-established` evidence keys; Plan 195 is reactivated to the same status and Plan 204 owns the docs/CI normalization pass on top of Plans 200/202/203; the full M10 final acceptance is recorded by Plan 204's §12 authority transition once Plan 201 closes the Java branch).
- SSU2 IPv6 external interop, PQ SSU2, SSU1, encrypted/meta LeaseSets, or PQ destination encryption unless separately closed later.

The historical NTCP2 development interoperability result remains separate evidence; no passed broad mixed-router claim exists.

## Working with plans

Before editing or claiming conformance, read `AGENTS.md`, `GUARDRAILS.md`, the newest relevant status record, and the matching OpenCode skill. When records disagree, the newest explicit superseding status wins.

Current handoff:

```text
M8 closed via Plan 161; Plan 162 corrective passed
Plan 163 = registered M9 I2CP planning authority
Plan 164 = passed M9 I2CP wire foundation
Plan 165 = passed M9 I2CP connection/session/options
Plan 166 = passed M9 I2CP client-owned destination + LeaseSet2 bridge
Plan 167 = passed M9 I2CP loopback server runtime
Plan 168 = passed M9 I2CP message data plane
Plan 169 = passed M9 I2CP self-composed local product and hardening
Plan 171 = passed M9 I2CP invalid-preamble close corrective (retained)
Plan 170 external wire/data-plane = retained-passed; final acceptance superseded-by-plan172
Plan 172 = passed M9 I2CP independent LeaseSet2 lifecycle corrective
Milestone 9 final acceptance = closed-via-plan172 (experimental, loopback-only)
Plan 173 = registered M10 service-tunnels roadmap
Plan 174 = passed M10 service-tunnel foundation and shared stream runtime
Plan 175 = passed M10 generic client/server service tunnels
Plan 176 = passed M10 HTTP `.i2p` proxy and CONNECT
Plan 177 = passed M10 SOCKS5 `.i2p` CONNECT proxy
Plan 178 = passed M10 IRC `.i2p` client profile and privacy filtering
Plan 179 = passed M10 IRC `.i2p` server profile and authenticated peer hostname
Plan 180 = passed M10 service-tunnel composition, reconcile, and hardening
Plan 181 = passed-m10-independent-application-and-service-interop-final-closure-evidence (reclassified from `blocked-by-m6-mixed-router-streaming-blocker` by Plan 204 docs/CI normalization: the retained local 29-row matrix stays green; the two §6.3 remote application rows now flip `blocked → passed-on-env` through the Plan 203 positive external driver)
Plan 182 = passed M10 local-delivery corrective
Plan 183 = registered M6 mixed-router streaming interop program
Plan 184 = passed M6 authenticated I2NP runtime and reference preflight
Plan 185 = passed M6 live one-hop exploratory tunnels and liveness
Plan 186 = passed M6 mixed-router NetDB lookup and publication
Plan 187 = blocked-by-m6-build-reply-interop-gap (local rows passed; 5/7 flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows blocked on plan191)
Plan 188 = blocked-by-plan191-inbound-destination-delivery (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190)
Plan 189 = registered-m6-java-second-family-qualification-and-closure (blocked-by-plan188-plan190-plan191; §8 cross-family ledger/checker/workflow landed)
Plan 190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (local rows passed; remote lane flips 3 destination rows blocked -> passed in fresh external run)
Plan 191 = retained-stopped (Plan 190 §6 stop boundary E; narrower follow-up registered + closed via plan192)
Plan 192 = passed-m6-i2cp-wire-format-corrective (i2pd-compatible I2CP-style Data body; inbound-delivery layer closed for i2pd 2.61.0)
Plan 193 = passed-m6-i2pd-mixed-router-streaming-qualification (local rows passed; full Direction A + Direction B external matrix passed twice on exact head 3687189; static checker wired)
Plan 194 = retained-partial-java-qualification-sam-ls2-publication-boundary (historical Plan 196 topology blocker corrected; Plan 198 now owns the public-client final-closure corrective)
Milestone 10 foundation = passed-via-plan174
Milestone 10 generic tunnels = passed-via-plan175
Milestone 10 HTTP proxy = passed-via-plan176
Milestone 10 SOCKS5 = passed-via-plan177
Milestone 10 IRC client = passed-via-plan178
Milestone 10 IRC server = passed-via-plan179
Milestone 10 local product = passed-via-plan180-and-plan182
Milestone 10 local round-trip = passed-via-plan182
Milestone 10 independent application clients = passed-via-plan181-and-plan203 (Plan 181 retained local 29-row matrix is green; the two Plan 181 §6.3 remote application rows now flip `blocked -> passed-on-env` through the Plan 203 positive external driver when the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple and the driver emits the `http-remote-application-established` / `irc-remote-application-established` evidence keys)
Milestone 10 remote service interop = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization (Plan 202 transport layer + Plan 203 application rows are passed; the two `remote-independent-*` rows flip `blocked -> passed-on-env` once the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple; Plan 195 reactivated to the same status; Plan 204 landed the docs/CI normalization pass; full M10 closure deferred to Plan 204's §12 authority transition once Plan 201 closes the Java branch)
Milestone 10 final acceptance = not-yet-closed
M6 authenticated I2NP preflight = passed-via-plan184
M6 exploratory one-hop tunnels = passed-via-plan185
M6 NetDB lookup/publication = passed-via-plan186
M6 destination local product = passed-via-plan187
M6 destination remote interop = installs-proven-lookup-publication-outbound-passed-inbound-delivery-passed-via-plan192 (i2pd only; Java second family not yet run; Streaming passed-via-plan193)
M6 inbound NetDB reply-path correction = passed-via-plan190 (typed route + adapter; 3 destination rows flipped blocked -> passed in remote run)
M6 inbound destination delivery = passed-via-plan192 (i2cp-compatible I2CP-style Data wire-format; STYLE=RAW SAM session; 9-byte short-transport inner envelope)
M6 i2pd mixed-router Streaming qualification = passed-via-plan193 (full Direction A + Direction B matrix, two exact-head passes; static checker wired)
M6 mixed-router cross-family ledger = retained-partial-via-plan193-and-plan194 (i2pd-family-passed-via-plan193 on exact head 3687189; java-second-family-now-diagnosed-via-plan200-and-branch-g-framework-landed-via-plan201; final closure blocked on the Plan 200 exact-head external run + Plan 201 final branch implementation)
 Plan 187 = blocked-by-m6-build-reply-interop-gap (local rows passed; 5/7 flipped via plan188 installs + plan190 reply-path correction)
 Plan 188 = blocked-by-plan191-inbound-destination-delivery (real outbound/inbound i2pd installs retained-passed)
 Plan 189 = registered-m6-java-second-family-qualification-and-closure (executable-via-plan194-next; §8 cross-family ledger/checker/workflow landed)
 Plan 190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (local + remote run; 3 destination rows flipped blocked -> passed)
 Plan 191 = retained-stopped (Plan 190 §6 stop boundary E; narrower follow-up registered + closed via plan192)
 Plan 192 = passed-m6-i2cp-wire-format-corrective (i2pd-compatible I2CP-style Data body; inbound-delivery layer closed for i2pd 2.61.0)
 Plan 193 = passed-m6-i2pd-mixed-router-streaming-qualification (local rows passed; full Direction A + Direction B external matrix passed twice on exact head 3687189; static checker wired)
  Plan 194 = retained-partial-java-qualification-sam-ls2-publication-boundary
  Plan 196 = passed-m6-java-controlled-first-run-topology-corrective (out-of-tree ControlledRouter.java test-only launcher + rewritten run-java.sh + extended static checker + `router.blocklist.enable=false` controlled-launcher fix + PRIV-token SAM SESSION CREATE fix; controlled Java topology + authenticated SSU2 preflight + STYLE=RAW SAM bridge proven on the exact-pinned Java I2P 2.13.0 cache; `external-session-established-java` flipped failed -> passed; the two narrow correctives are bounded to the controlled-launcher and fail-closed at the daemon boundary)
  Plan 197 = passed-m6-pq-ssu2-option-support-corrective (parser-only tolerance of the SSU2 `pq` KEM-scheme option Java I2P 2.13.0 unconditionally publishes; typed Ssu2PqKem/PqCapabilities surface with bounded MAX_SSU2_PQ_SCHEMES = 8; i2pr session layer remains classical X25519 only; i2pr publication path stays pq-free; ML-KEM not implemented or claimed; 21 required test rows green locally)
  Plan 198 = superseded-execution-decomposed-and-closed-via-plans200-204 (Plan 198's original `blocked-public-java-client-leaseset2-publication` verdict is retained verbatim; the active Java branch is Plan 201 and Plan 204 owns the docs/CI normalization pass)
  Plan 199 = superseded-execution-decomposed-and-closed-via-plans200-204 (retained historical umbrella for the final M6/M10 requirements; decomposed into Plans 200/201/202/203/204)
  Plan 200 = passed-m6-java-public-client-publication-observability-and-verified-bootstrap (Java helpers decoupled `leaseset=published` from `READY` and added bounded `REPORT_STATUS`; Rust driver adds post-bootstrap RouterInfo DatabaseLookup proofs in both directions; sanitized Java log keys for the client LS2 lifecycle and tunnel/floodfill/store/ack selection emitted to evidence; exactly one terminal `P200-{A..H}` classification per run; `scripts/check-m6-mixed-router-acceptance-evidence.sh` extended with Plan 200 §B/C/D/§11 invariants)
  Plan 201 = in-progress-branch-a-corrective-landed-branch-g-framework-unchanged (this run consumed the first Plan 200 / Plan 201 exact-head external run against the exact-pinned Java I2P 2.13.0 cache (`9134f808337b401e8e53c73734c81fab04280c9d`, staged unmodified via `bash scripts/interop/fetch-m6-java.sh --rebuild`); the terminal `P200-B-router-b-missing-router-a` classification was recorded consistently across three sequential exact-head runs (`java-main-netdb-a-knows-b=true`, `java-main-netdb-b-knows-a=false`); the corresponding Plan 201 §3 Branch A corrective landed as a narrow test-driver fix — `decode_inbound_i2np` helper that mirrors the production dispatcher's standard-first / short-transport-fallback ordering and the probe-side gzip decompression that matches `i2pr_netdb::decompress_router_info` — so the bootstrap probe now decodes Java's standard-form, gzipped `RouterInfoCompressed` `DatabaseStore` responses correctly; the one-direction `p200-routerinfo-lookup-a-knows-b` row flipped `false → passed` (Branch A proof) while the asymmetric `b-knows-a` direction stays on the Java B / i2pr bootstrap-storage asymmetry side per Plan 201 §3 Branch A's "do not change i2pr production NetDB code unless the captured transcript proves i2pr encoded an invalid I2NP message" rule; the Branch G `store-acked-remote-lookup-fails` corrective framework (eleven sanitized observation counters on `DestinationTunnelCounters` + public `note_lookup_boundary(label, value)` typed observation surface + eight new `plan201_g_*` unit rows in `destination_tunnel_unit.rs` + six new `blocked_row` Plan 201 §G entries in `run-java.sh` + static checker `scripts/check-m6-mixed-router-acceptance-evidence.sh` §11 + §12 invariants) is retained as the documented attribution surface for any future `P200-G` exact-head run; the seven §11 stop rows stay `blocked` with the documented Plan 198/199 stop provenance until the Java-side LeaseSet2 publication gap closes; the M6 Java second-family claim stays `not-yet-passed`)
  Plan 202 = passed-m10-production-remote-destination-and-streaming-composition (`ServiceTunnelManager` now owns one shared `ServiceDestinationDelivery` capability; typed `RoutingDecision::LocalCoOwned` / `RemoteRouter` / `RemoteUnresolved`; bounded `RemoteDeliveryCounters` emits twelve positive observations on every counted path; `m10_remote_destination_streaming_composition_through_manager` Direction A external driver exercises `install_router_delivery_handle` / `routing_decision_for` against the exact-pinned i2pd 2.61.0 cache; static checker `scripts/check-service-tunnel-acceptance-evidence.sh` extended with Plan 202 §12 invariants; nine new unit rows in `service_delivery.rs` + five in `service_tunnels.rs` cover the routing-decision classification; `m10-remote-destination-streaming-composition` row is `blocked` in the M10 lane and flips to `passed` through `record_guarded` when the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple)
  Plan 203 = passed-m10-positive-remote-http-and-irc-application-interop (`m10_positive_remote_http_and_irc_application_interop` Direction A external driver declares `http-client` + `irc-client` specs whose destination is the i2pd-owned HTTP + IRC server-tunnel destination b64, asserts `RoutingDecision::RemoteRouter` after `install_router_delivery_handle`, advances the typed Plan 203 §5/§6 documented observation set through the new public `record_remote_application_observation` helper, exercises the underlying Plan 184–193 router stack with real one-hop builds + lease lookup + Streaming `Established`, and never logs peer key material; static checker `scripts/check-service-tunnel-acceptance-evidence.sh` rejects literal `record "... passed"` lines and requires the positive rows to flow through `record_guarded` + the documented evidence keys `http-remote-application-established` / `irc-remote-application-established` / `manager-routing-decision`; the two `remote-independent-*` rows flip from `blocked` to `passed` once the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple; the full M10 lane stays fail-closed without it)
  Plan 195 = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization (Plan 195 absorbed the Plan 203 positive application evidence; reactivated from the historical `registered-blocked-by-plan199` interpretation by the Plan 204 docs/CI normalization pass; the remaining work is the Plan 201 Java corrective and the Plan 204 final closure transition)
  Plan 181 = passed-m10-independent-application-and-service-interop-final-closure-evidence (Plan 181 §6.3 retained local rows remain green; the two `remote-independent-*` remote application rows now flip `blocked → passed-on-env` through the Plan 203 positive external driver; reclassified by the Plan 204 docs/CI normalization pass from `blocked-by-m6-mixed-router-streaming-blocker`)
  Plan 204 = in-progress-docs-and-authority-normalization-blocked-on-plan201-external-run (intentionally narrow docs/CI/evidence-authority normalization pass; the M6/M10 closed authority transitions in §7/§12 of `204-m10-final-closure-evidence-authority-and-documentation-normalization.md` stay deferred until Plan 201 records the terminal `P200-{A..H}` classification and lands its narrow corrective; no product bug hidden, no synthetic `passed` evidence introduced)
  next_executable_plan = 201-or-205-pivot-to-branch-{b..f} (Plan 200 / this Plan 201 exact-head run consumed the P200-B classification; Branch A corrective landed the i2pr test-driver decode gap and flipped the one-direction `p200-routerinfo-lookup-a-knows-b` row false → passed; the asymmetric `p200-routerinfo-lookup-b-knows-a` direction stays blocked on a Java-B-NetDB bootstrap-storage asymmetry on the Java side per Plan 201 §3 Branch A's rule; the remaining Plan 201 Branches B-F stay on the Java-side LeaseSet2 publication gap recorded in Plan 194 / Plan 200 §C / §D)
  remaining_m6_sequence = 201-or-205-pivot-to-branch-{b..f} -> 204-convergence (the Branch A narrow test-driver fix landed; the Java-side LeaseSet2 publication gap is the remaining unaddressed boundary)
  remaining_m10_sequence = plans 174-180-182-202-203 all passed; the Plan 181 + Plan 203 + Plan 195 remote application rows flip passed-on-env once the dedicated M6 interop lane provisions the SSU2 endpoint + bind tuple; full M10 closure deferred to Plan 204's §12 authority transition after Plan 201 records its terminal classification
```
