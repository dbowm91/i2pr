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
source/profile/wire foundation. The current **next executable plan is
Plan 165**, the I2CP connection/session/options pass. Execute Plans
165–170 sequentially; do not skip ahead based on aggregate workspace
green status.

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
plan_160 = passed-m8-ssu2-peer-test-and-relay-reachability
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_163 = registered-m9-i2cp-roadmap
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation

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
milestone9_final_acceptance = not-yet-closed
next_executable_plan = 165
m9_sequence = 164 -> 165 -> 166 -> 167 -> 168 -> 169 -> 170
next_product_layer = milestone9-i2cp
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
- [`165-m9-i2cp-connection-session-and-options.md`](165-m9-i2cp-connection-session-and-options.md) — connection/version/session state machines, canonical SessionConfig signature/date validation, bounded session registry, SessionStatus mapping, and explicit tunnel/session option projection into existing `DestinationConfig` policy.
- [`166-m9-i2cp-client-owned-destination-and-leaseset2.md`](166-m9-i2cp-client-owned-destination-and-leaseset2.md) — central ownership pass. Add a client-owned destination mode without requiring the client's signing private key; request leases from real destination tunnels; validate/install client-signed Standard LeaseSet2 plus matching X25519 decryption key transactionally; reuse existing ECIES/routing; preserve router-owned SAM behavior.
- [`167-m9-i2cp-loopback-server-runtime.md`](167-m9-i2cp-loopback-server-runtime.md) — daemon-owned supervised TCP listener/runtime. Disabled by default, loopback-only, bounded read/write/session resources, real-TCP session/LeaseSet2 activation and cleanup.
- [`168-m9-i2cp-message-data-plane.md`](168-m9-i2cp-message-data-plane.md) — SendMessage/Expires, bounded payload format, honest MessageStatus semantics, inbound MessagePayload, destination lookup, bandwidth replies, flags, backpressure, and bidirectional local application data over the existing destination routing/ECIES path.
- [`169-m9-i2cp-self-composed-local-product-and-hardening.md`](169-m9-i2cp-self-composed-local-product-and-hardening.md) — transactional reconfigure/destroy, complete adversarial/resource matrix, repeated lifecycle baselines, and canonical two-destination self-composed real-TCP product driven only through I2CP after listener startup.
- [`170-m9-i2cp-independent-clients-and-final-closure.md`](170-m9-i2cp-independent-clients-and-final-closure.md) — final independent-client gate. Exact-pinned Java I2P 2.13.0 plus target exact-pinned go-i2cp create modern client-owned sessions and exchange cross-client traffic both directions; fail-closed command-derived ledger/checker/manual workflow; exact-head routine + external CI; final M9 closure.

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
- M9 I2CP implementation plans are registered; the Plan 164 wire/profile foundation is landed with no behavior claim yet.

## What's not yet accepted

- Any M9 I2CP session/listener/behavior or independent-client result until Plans 165–170 execute.
- Non-loopback/remote I2CP, TLS/authentication, or broad historical I2CP feature compliance.
- Live/public NTCP2 or SSU2 router transport activation and broad mixed-router interoperability.
- Public I2P participation and network-transport-bound NetDB/public router behavior.
- Milestone 6 independent-router destination/Streaming/tunnel interoperability.
- Service tunnels, HTTP proxy, SOCKS5, and IRC (Milestone 10).
- SSU2 IPv6 external interop, PQ SSU2, SSU1, encrypted/meta LeaseSets, or PQ destination encryption unless separately closed later.

The historical NTCP2 development interoperability result remains separate evidence; no passed broad mixed-router claim exists.

## Working with plans

Before editing or claiming conformance, read `AGENTS.md`, `GUARDRAILS.md`, the newest relevant status record, and the matching OpenCode skill. When records disagree, the newest explicit superseding status wins.

Current handoff:

```text
M8 closed via Plan 161; Plan 162 corrective passed
Plan 163 = registered M9 I2CP planning authority
Plan 164 = passed M9 I2CP wire/profile foundation
execute Plan 165 next
then 166 -> 167 -> 168 -> 169 -> 170
```
