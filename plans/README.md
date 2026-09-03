# i2pr plans

Plan-of-record and closure/status records for `i2pr`. Closure/status records
(`NNN-status.md`) are authoritative; per-plan narratives are historical context
unless the newest status points to them as executable work.

## Current authority

The current **Milestone 6 local closure authority** remains
[**Plan 134**](134-status.md)
(`passed-milestone6-recv-window-ack-ceiling-closure`). Independent-router
interoperability is not claimed and remains separate external acceptance debt.

The current **Milestone 7 SAM 3.1 final-acceptance authority** is
[**Plan 151**](151-status.md)
(`passed-m7-sam31-final-acceptance-evidence-correction`). Plan 152 is the
retained narrow M6 robustness corrective discovered by Plan 151; its
authoritative closure record is [`plans/152-status.md`](152-status.md),
normalized by Plan 153.

The current **next executable plan** is
[**Plan 153**](153-status.md)
(`active-post-m7-authority-and-ci-hygiene`).

The registered **Milestone 8 planning authority** is
[**Plan 154**](154-status.md). Milestone 8 implementation is blocked until
Plan 153 passes. After that, execute Plans **155 → 156 → 157 → 158 → 159 →
160 → 161** in order.

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
plan_153 = active-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap-blocked-by-plan153

milestone7_local_product = passed-via-plan149
milestone7_sam_localhost = passed-via-plan151
milestone7_sam_localhost_final_acceptance = closed
sam_independent_clients = at-least-two-passed-via-plan150
milestone6_interoperable = not-yet-claimed

next_executable_plan = 153
milestone8_first_implementation_after_153 = 155
next_product_layer = milestone8-ssu2-v2
```

## Current handoff sequence

### Post-M7 hygiene

- [`153-m7-closure-authority-and-ci-hygiene.md`](153-m7-closure-authority-and-ci-hygiene.md) — **next executable**. Create the missing authoritative Plan 152 status record, remove stale Plan 151/152 prose, add the Plan 152 closure pointer to the support ledger, and make the Plan 151 SAM evidence-integrity checker a routine Linux-CI and manual-SAM-workflow invariant. No `crates/` or `Cargo.lock` changes are allowed.

### Milestone 8 — SSU2 v2

- [`154-m8-ssu2-transport-and-reachability-roadmap.md`](154-m8-ssu2-transport-and-reachability-roadmap.md) — Milestone 8 roadmap/planning authority. Classical SSU2 v2 is required; PQ SSU2 v3/v4 is explicit compatibility debt; SSU1 remains unsupported.
- [`155-m8-ssu2-v2-protocol-foundation-and-addresses.md`](155-m8-ssu2-v2-protocol-foundation-and-addresses.md) — refresh SSU2 source authority, add runtime-neutral `i2pr-transport-ssu2`, integrate `TransportKind::Ssu2`, and implement strict v2 RouterAddress/header/block foundations plus vectors.
- [`156-m8-ssu2-v2-handshake-token-and-routerinfo.md`](156-m8-ssu2-v2-handshake-token-and-routerinfo.md) — Noise XK establishment, header protection, TokenRequest/Retry, bounded one-use token lifecycle, RouterInfo fragmentation/validation, replay/deadline state.
- [`157-m8-ssu2-v2-data-phase-reliability-and-fragmentation.md`](157-m8-ssu2-v2-data-phase-reliability-and-fragmentation.md) — authenticated data packets, packet-number/replay window, ACK scheduling/ranges, bounded loss/congestion/retransmission, I2NP fragmentation/reassembly, duplicate suppression, termination/rekey.
- [`158-m8-ssu2-udp-runtime-and-local-session-product.md`](158-m8-ssu2-udp-runtime-and-local-session-product.md) — production UDP ownership in `i2pr-runtime`, existing `TransportManager` integration, central bounded scheduler, and real localhost i2pr↔i2pr UDP product tests.
- [`159-m8-ssu2-path-validation-publication-and-transport-selection.md`](159-m8-ssu2-path-validation-publication-and-transport-selection.md) — authenticated path migration, conservative reachability/address publication, IPv4/IPv6 structural separation, and deterministic NTCP2/SSU2 selection/fallback.
- [`160-m8-ssu2-peer-test-and-relay-reachability.md`](160-m8-ssu2-peer-test-and-relay-reachability.md) — PeerTest and relay requester/introducer/target roles, anti-amplification/resource policy, introducer records, and real-loopback NAT-like acceptance without namespaces.
- [`161-m8-ssu2-independent-ipv4-interop-and-final-closure.md`](161-m8-ssu2-independent-ipv4-interop-and-final-closure.md) — final independent direct-session gate. Exact-pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`) must interoperate in both directions over real localhost UDP and exchange authenticated I2NP messages. Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`) is a preferred secondary lane, not a reason to rebuild privileged/VM harness infrastructure.

Milestone 8 architecture is deliberately constrained:

```text
i2pr-transport-ssu2   runtime-neutral protocol/state machines
          ↓ actions/events
      i2pr-runtime     only production UDP/Tokio/timer owner
          ↓
     i2pr-transport    existing generic link/resource/delivery manager
```

Do not create a second SSU2-specific transport manager or a task/timer per packet.

## MVP roadmap

- [`000-mvp-roadmap.md`](000-mvp-roadmap.md) — milestone sequence from repository foundation through the first feature-complete MVP.
- [`154-m8-ssu2-transport-and-reachability-roadmap.md`](154-m8-ssu2-transport-and-reachability-roadmap.md) — current Milestone 8 detailed roadmap.
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
| 139 | FORWARD/naming implementation retained; final matrix closed by Plan 151 | [`139-status.md`](139-status.md) |
| 140 | blocked closure audit; historical | [`140-status.md`](140-status.md) |
| 141 | historical corrective roadmap | [`141-status.md`](141-status.md) |
| 142 | Base64 correction retained | [`142-status.md`](142-status.md) |
| 143 | local delivery seam retained | [`143-status.md`](143-status.md) |
| 144 | partial local in-process handshake evidence | [`144-status.md`](144-status.md) |
| 145 | historical corrective umbrella | [`145-status.md`](145-status.md) |
| 146 | **passed** private-destination reference requalification | [`146-status.md`](146-status.md) |
| 147 | raw-driver implementation/local byte pump retained; broad original acceptance superseded | [`147-status.md`](147-status.md) |
| 148 | blocked historical audit | [`148-status.md`](148-status.md) |
| 149 | **passed** self-composing SAM local product | [`149-status.md`](149-status.md) |
| 150 | external-client core evidence **retained passed**; final acceptance superseded | [`150-status.md`](150-status.md) |
| 151 | **passed** final M7 SAM localhost acceptance | [`151-status.md`](151-status.md) |
| 152 | **passed** narrow M6 session/streaming robustness corrective | [`152-status.md`](152-status.md) |
| 153 | **active** post-M7 authority/CI hygiene; next executable | [`153-status.md`](153-status.md) |

## Milestone 6 plan hierarchy

Closure records remain authoritative for Milestone 6.

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
- SAM 3.1 parser/session/STREAM/FORWARD/NAMING product.
- Plan 146 reference-compatible private destinations.
- Plan 149 self-composed SAM `SESSION CREATE` product path.
- Plan 150 external SAM-client core interoperability evidence on localhost.
- Plan 151 executable sibling/backpressure/fault/lifecycle/FORWARD evidence and final localhost SAM closure.

## What's not yet accepted

- Live/public NTCP2 or SSU2 router transport activation and broad mixed-router interoperability.
- SSU2 implementation: Milestone 8 is planned but no Plan 155+ implementation has landed yet.
- Public I2P participation.
- Network-transport-bound NetDB/public router behavior.
- Milestone 6 independent-router destination/Streaming/tunnel interoperability.
- Client proxies and service tunnels.

The historical NTCP2 development interoperability result remains separate evidence; no passed broad mixed-router claim exists.

## Working with plans

Before editing or claiming conformance, read `AGENTS.md`, `GUARDRAILS.md`, the newest relevant status record, and the matching OpenCode skill.

When records disagree, the newest explicit superseding status wins.

Current handoff:

```text
execute Plan 153
then execute Plans 155 -> 161 in order
Plan 154 is the Milestone 8 roadmap authority
```
