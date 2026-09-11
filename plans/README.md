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
plan_181 = blocked-by-m6-mixed-router-streaming-blocker
plan_182 = passed-m10-local-delivery-corrective
plan_183 = registered-m6-mixed-router-streaming-interop-program
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication

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
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
m6_mixed_router_program = registered-via-plan183
m6_authenticated_i2np_preflight = passed-via-plan184
m6_exploratory_one_hop_tunnels = passed-via-plan185
m6_netdb_lookup_publication = passed-via-plan186
next_product_layer = m6-mixed-router-leaseset2
next_executable_plan = 187
m9_sequence = 164 -> 165 -> 166 -> 167 -> 168 -> 169 -> 171 -> 170 -> 172
m10_sequence = 173 -> 174 -> 175 -> 176 -> 177 -> 178 -> 179 -> 180 -> 182 -> 181(blocked) -> 183
m6_sequence = 183(registered) -> 184(passed) -> 185(passed) -> 186(passed) -> 187(next)
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
- M10 service-tunnel foundation is landed via Plan 174 (runtime-neutral `i2pr-service-tunnels` crate, strict `[service_tunnels]` surface, shared Streaming pump reused by SAM) plus the Plan 175 generic client/server tunnels, Plan 176 HTTP proxy, Plan 177 SOCKS5 proxy, Plan 178 IRC client profile, Plan 179 IRC server profile, Plan 180 composition/reconcile/hardening, and Plan 182 local-delivery corrective. Local product + byte round-trip are passed via Plans 180-and-182 (9 round-trip + 6 wire-surface tests); Plan 181 local independent-application-client rows pass with remote rows blocked.
- M6 mixed-router program is registered via Plan 183; Plan 184 passed the authenticated I2NP runtime and reference preflight (strict daemon SSU2 activation, central dispatcher, narrow delivery, exact-pinned i2pd bidirectional control, 10 fail-closed rows). Plan 185 passed the live one-hop exploratory tunnel lane: the bounded Plan 185 exploratory build coordinator + the bounded creator-side liveness scheduler drive the existing `i2pr-tunnel::short::ShortBuildStateMachine` and `i2pr-tunnel::pool::ExploratoryPool` / `i2pr-tunnel::data_plane_registry::DataPlaneRegistry` seams end-to-end, with one real outbound and one real inbound one-hop build accepted by the exact-pinned i2pd 2.61.0 reference (`TransitTunnel: endpoint N created` and `TransitTunnel: gateway N created` log evidence). Plan 186 passed the live mixed-router NetDB lookup / publication program: the daemon-owned `NetDbTunnelCoordinator` drives the existing lookup/publication state machines over the Plan 185 pair through the ordinary authoritative store (exact-pinned i2pd floodfill, 12-row lane).

## What's not yet accepted

- Non-loopback/remote I2CP, TLS/authentication, or broad historical I2CP feature compliance.
- Live/public NTCP2 or SSU2 router transport activation and broad mixed-router interoperability.
- Public I2P participation and network-transport-bound NetDB/public router behavior.
- Milestone 6 independent-router destination/Streaming/tunnel interoperability (Plan 183 program registered, Plan 184 preflight passed with no tunnel/NetDB/Streaming claim; Plan 181 remote rows blocked on the remaining debt).
- Milestone 10 remote service interop / final acceptance (local rows passed via Plan 181; remote HTTP/IRC rows not-yet-passed).
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
Plan 181 = blocked-by-m6-mixed-router-streaming-blocker (local rows passed; remote gate pending plan183)
Plan 182 = passed M10 local-delivery corrective
Plan 183 = registered M6 mixed-router streaming interop program
Plan 184 = passed M6 authenticated I2NP runtime and reference preflight
Milestone 10 foundation = passed-via-plan174
Milestone 10 generic tunnels = passed-via-plan175
Milestone 10 HTTP proxy = passed-via-plan176
Milestone 10 SOCKS5 = passed-via-plan177
Milestone 10 IRC client = passed-via-plan178
Milestone 10 IRC server = passed-via-plan179
Milestone 10 local product = passed-via-plan180-and-plan182
Milestone 10 local round-trip = passed-via-plan182
Milestone 10 independent application clients = local-rows-passed-plan181-not-closed
Milestone 10 remote service interop = not-yet-passed
Milestone 10 final acceptance = not-yet-closed
M6 authenticated I2NP preflight = passed-via-plan184
M6 exploratory one-hop tunnels = passed-via-plan185
M6 NetDB lookup/publication = passed-via-plan186
next_executable_plan = 187
```
