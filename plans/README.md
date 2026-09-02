# i2pr plans

Plan-of-record and closure records for `i2pr`. Closure/status records
(`NNN-status.md`) are authoritative; per-plan narratives
(`NNN-name.md`) are historical context unless the newest status points to them as executable work.

The current **Milestone 6 local closure authority** is
[**Plan 134**](134-status.md) (`passed-milestone6-recv-window-ack-ceiling-closure`).
Independent-router interoperability is not claimed and is tracked as external acceptance debt.

The current **Milestone 7 SAM 3.1 corrective authority** is
[**Plan 145**](145-status.md), with the newest closed authority
[**Plan 149 status**](149-status.md) (`passed-m7-sam31-self-composing-local-product-corrective`).
The active sequence is now:

**Plan 146 (closed) → Plan 147 raw-driver implementation retained → Plan 148 blocked audit → Plan 149 (closed) → Plan 150 (next executable).**

- Plan 146 closed private-destination compatibility with bidirectional Java I2P/i2pd reference evidence.
- Plan 147 landed the real same-socket raw TCP↔Streaming owner/byte pump and supervised ACK/retransmit driver. Its implementation evidence is retained, but its broad original acceptance label is superseded where mandatory SILENT/backpressure/fault/lifecycle criteria were deferred.
- Plan 148 correctly rejected one Rust helper used twice as “independent clients,” but its original `blocked-external-client-build-failure` diagnosis was incomplete. The Plan 147 canonical test manually installs bridges, peer LeaseSet2 routing, inbound-tunnel factories, and destination drivers after `SESSION CREATE`; production `SESSION CREATE` does not currently self-compose those prerequisites. Plan 148 is therefore historical blocked-audit evidence.
- [Plan 149](149-m7-sam31-self-composing-local-product-corrective.md) **closed the self-composed local STREAM product**. `SESSION CREATE` now self-composes the full product: one `Arc<DestinationIdentity>` allocation, OS-CSPRNG `SamLocalProductFabric` for LeaseSet2 / outbound role / inbound-tunnel factory, automatic per-destination runtime driver spawn, local peer LeaseSet2 directory, byte-exact `STREAM STATUS RESULT=OK`/`DESTINATION=<peer-pub-b64>` raw-transition semantics, typed `DeliverySweepCounters`. The canonical evidence lives in `crates/i2pr-daemon/tests/sam_stream_self_composed.rs`.
- [Plan 150](150-m7-sam31-external-client-reproducible-final-closure.md) is **next executable**. It runs correctly pinned external clients through the Plan 149 self-composed listener to close independent-client FORWARD / NAMING / final M7 evidence.

`sam_independent_clients = 0-passed` until Plan 150 succeeds. Milestone 7 local product is closed via Plan 149; Milestone 8 is not the next product layer until Plan 150 closes Milestone 7.

## MVP roadmap

- [`000-mvp-roadmap.md`](000-mvp-roadmap.md) — milestone sequence from empty repository to the first feature-complete MVP (CLI router, NTCP2/SSU2, NetDB + floodfill, tunnel participation, destinations, streaming, SAM/I2CP, HTTP/SOCKS5 proxies, service tunnels). **Independent-router interoperability is MVP acceptance debt, not a prerequisite for the localhost SAM product work in Milestone 7.**
- [`145-m7-sam31-remaining-gap-corrective-roadmap.md`](145-m7-sam31-remaining-gap-corrective-roadmap.md) — Milestone 7 corrective umbrella; newest execution authority is Plan 149 status.
- [`149-m7-sam31-self-composing-local-product-corrective.md`](149-m7-sam31-self-composing-local-product-corrective.md) — active production-composition and deferred raw-path acceptance corrective.
- [`150-m7-sam31-external-client-reproducible-final-closure.md`](150-m7-sam31-external-client-reproducible-final-closure.md) — final independent-client closure after Plan 149.
- [`141-m7-sam31-corrective-roadmap.md`](141-m7-sam31-corrective-roadmap.md) — previous Milestone 7 corrective sequence after the blocked Plan 140 audit; superseded for next-action authority.
- [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md) — destination / garlic / LeaseSet2 / Streaming router construction.
- [`126-129-milestone6-final-corrective-roadmap.md`](126-129-milestone6-final-corrective-roadmap.md) and [`126-130-milestone6-final-corrective-roadmap.md`](126-130-milestone6-final-corrective-roadmap.md) — Milestone 6 final corrective roadmaps.

## Milestone 7 plan hierarchy

| Plan | Current authority status | Record |
| --- | --- | --- |
| 135 | superseded by later audits/corrective roadmaps | [`135-status.md`](135-status.md) |
| 136 | foundation landed; encoding/private-destination evidence superseded by later corrective work | [`136-status.md`](136-status.md) |
| 137 | passed loopback server/session lifecycle | [`137-status.md`](137-status.md) |
| 138 | implementation landed; product acceptance superseded by later raw-stream corrective work | [`138-status.md`](138-status.md) |
| 139 | local FORWARD/naming implementation landed; final byte-path acceptance now belongs to Plan 150 | [`139-status.md`](139-status.md) |
| 140 | blocked closure audit; historical authority | [`140-status.md`](140-status.md) |
| 141 | previous corrective roadmap; superseded for next-action authority | [`141-status.md`](141-status.md) |
| 142 | **Base64 correction retained as passed**; private-destination external compatibility requalified by Plan 146 | [`142-status.md`](142-status.md) |
| 143 | local delivery seam / Plan 129 bridge work retained; full raw STREAM acceptance superseded | [`143-status.md`](143-status.md) |
| 144 | **partial local evidence** — in-process bidirectional Streaming handshake | [`144-status.md`](144-status.md) |
| 145 | **active corrective umbrella** | [`145-status.md`](145-status.md) |
| 146 | **passed** bidirectional private-destination reference requalification | [`146-status.md`](146-status.md) |
| 147 | raw-driver implementation and local byte-pump **retained**; full original acceptance superseded by Plan 149 | [`147-status.md`](147-status.md) |
| 148 | **blocked audit; superseded for execution by Plans 149–150** | [`148-status.md`](148-status.md) |
| 149 | **passed** self-composing SAM local product + deferred raw acceptance | [`149-status.md`](149-status.md) |
| 150 | **next executable** — reproducible external-client final M7 closure | [`150-m7-sam31-external-client-reproducible-final-closure.md`](150-m7-sam31-external-client-reproducible-final-closure.md) |

Do not restore historical broad `passed` labels as final M7 authority without satisfying the newest superseding acceptance criteria.

## Milestone 6 plan hierarchy

Closure records are authoritative; `superseded-by-*` rows are historical evidence retained for audit.

| Plan | Status | Closure |
| --- | --- | --- |
| 119 | `passed-leaseset2-protocol-foundation` | [`119-status.md`](119-status.md) |
| 120 | `passed-destination-lifecycle-and-pools` | [`120-status.md`](120-status.md) |
| 121 | `superseded-by-126` | [`121-status.md`](121-status.md) |
| 122 | `passed-corrected-local-destination-routing` | [`122-status.md`](122-status.md) |
| 123 | `passed-corrected-streaming-wire-local` (restored by 128) | [`123-status.md`](123-status.md) |
| 124 | `passed-plan122-corrective-closure` | [`124-status.md`](124-status.md) |
| 125 | `superseded-by-final-corrective-closure` | [`125-status.md`](125-status.md) |
| 126 | `passed-ecies-destination-ratchet-corrective-foundation` | [`126-status.md`](126-status.md) |
| 127 | `passed-destination-session-routing-final-closure` | [`127-status.md`](127-status.md) |
| 128 | `passed-streaming-wire-protocol-corrective-closure` | [`128-status.md`](128-status.md) |
| 129 | `superseded-by-130` | [`129-status.md`](129-status.md) |
| 130 | `superseded-by-131` | [`130-status.md`](130-status.md) |
| 131 | `superseded-by-132-and-133` | [`131-status.md`](131-status.md) |
| 132 | `implementation-landed-evidence-superseded-by-plan133` | [`132-status.md`](132-status.md) |
| 133 | `passed-evidence-authority-superseded-by-plan134` | [`133-status.md`](133-status.md) |
| 134 | `passed-milestone6-recv-window-ack-ceiling-closure` (current authority) | [`134-status.md`](134-status.md) |

## Earlier milestones and lanes

Milestones 0–5 live under the same plan/status naming. The active development interop lane is closed; NTCP2 stays experimental and non-advertised. The historical NTCP2 interop apparatus lives under `docs/architecture/interop-apparatus.md` and the closed Plan 038–100 records.

The constrained-host lane, the Plan 046 rootless sealed-namespace lane, and the Plan 048–051 Multipass recovery guest are documented in the architecture skill and dedicated interop skills.

## What's implemented

- Bounded wire protocol codecs (`i2pr-proto`).
- Cryptographic wrappers: Ed25519, X25519, AES, ChaCha20-Poly1305, HMAC, SipHash, HKDF-SHA256, ECIES-X25519-AEAD-Ratchet (`i2pr-crypto`).
- Versioned private-identity persistence + NTCP2 static key/IV record (`i2pr-storage`).
- Runtime-neutral transport + NTCP2 codec (`i2pr-transport`, `i2pr-transport-ntcp2`).
- Tokio-owned runtime with supervision, cancellation, and bounded channels (`i2pr-runtime`).
- Deterministic testkit with seeded randomness and fault injection (`i2pr-testkit`).
- RouterInfo validation, bounded local NetDB store, lookup/publication state machines, signed Standard LeaseSet2 carrier and validation (`i2pr-netdb`).
- Persistent RouterInfo cache + bounded SU3 reseed ingestion (`i2pr-netdb-persist`).
- Daemon composition root + local NetDB dispatch (`i2pr-daemon`).
- Exploratory tunnel substrate, ECIES-X25519 short tunnel-build cryptography, runtime-neutral tunnel data plane, and local exploratory composition (`i2pr-tunnel`).
- Local destination runtime: identity, dedicated tunnel pools, signed Standard LeaseSet2 generation/lifecycle, ECIES-X25519-AEAD-Ratchet session layer, destination routing, I2P Streaming core (`i2pr-client`).
- SAM bounded parser/session/FORWARD/naming foundations.
- Plan 142 canonical I2P Base64 correction (`-`/`~`, `=` padding).
- Plan 146 reference-compatible private-destination import/generation.
- Plan 143/144 reusable Plan 129 local-delivery seam and in-process Streaming handshake regression.
- Plan 147 dedicated raw STREAM socket owner, TCP↔Streaming byte pump, and supervised ACK/retransmit runtime driver.
- Plan 149 self-composed `SESSION CREATE` (one `Arc<DestinationIdentity>` allocation, OS-CSPRNG `SamLocalProductFabric`, automatic per-destination runtime driver, local peer LeaseSet2 directory, byte-exact raw transition, typed `DeliverySweepCounters`).

## What's not implemented / not yet accepted

- Live NTCP2 or SSU2 transport activation (NTCP2 remains experimental/non-advertised).
- Live mixed-router tunnel-build execution.
- Network-transport-bound NetDB lookup/publication.
- Full router I2NP/network dispatch.
- Milestone 7 SAM closure:
  - **Plan 149** closed the self-composed local STREAM product
    (canonical black-box test passes; `sam31_self_composing_product
    = passed`).
  - exact SILENT plus raw-path resource/fault/lifecycle acceptance is
    covered by `sam_stream_self_composed`.
  - two correctly pinned independent SAM clients have not yet moved
    application bytes through the real self-composed listener; this
    is now Plan 150.
  - final FORWARD/NAMING external closure is Plan 150.
- Client proxies (HTTP, SOCKS5) and service tunnels.
- Any public/network-facing router behavior.

The NTCP2 development interoperability result remains `protocol-defect-localized` at `noise_authenticated`; no passed mixed-router NTCP2 result exists.

## Working with plans

Before editing or claiming behavioral conformance, load [`.opencode/skills/i2pr-local-dev/SKILL.md`](../.opencode/skills/i2pr-local-dev/SKILL.md) for the routine development seam, or the architecture skill for documentation/ADR navigation.

When a closure/status record and a per-plan narrative disagree, the newest explicit superseding status record wins. The current handoff is **execute Plan 150** (Plan 149 already closed).
