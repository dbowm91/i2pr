# i2pr plans

Plan-of-record and closure records for `i2pr`. Closure records
(`NNN-status.md`) are authoritative; per-plan narratives
(`NNN-name.md`) are historical context, not live contracts.

The current **Milestone 6 local closure authority** is
[**Plan 134**](134-status.md) (`passed-milestone6-recv-window-ack-ceiling-closure`).
Independent-router interoperability is not claimed and is tracked as
external acceptance debt.

The current **Milestone 7 SAM 3.1 corrective authority** is
[**Plan 145**](145-status.md) (`active-m7-sam31-remaining-gap-corrective-roadmap`).
The post-Plan-144 audit retains the correct Plan 142 I2P Base64 change and
the useful Plan 143/144 local-delivery plus in-process handshake work, but
reclassifies the unsupported closure claims. The sequence is
**Plan 146 (closed) → Plan 147 (closed) → Plan 148 (blocked)**:

- Plan 146 has closed as
  [`passed-m7-sam31-private-destination-reference-requalification`](146-status.md) —
  bidirectional reference evidence against the pinned Java I2P 2.12.0
  (`2800040deee9bb376567b671ef2e9c34cf3e30b6`) and i2pd 2.60.0
  (`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`) references proves the
  canonical SAM private-destination / PrivateKeyFile representation;
  Plan 146 also relaxed the reconstruction invariant (`from_imported`
  preserves the destination's embedded encryption public field verbatim)
  to match the standard Java I2P `PrivateKeyFile` and i2pd
  `IdentityEx` layouts;
- Plan 147 implements the missing dedicated TCP↔Streaming raw socket driver,
  ACK/retransmit runtime ownership, backpressure, SILENT, and lifecycle/fault
  acceptance;
 - Plan 148 drives two independent SAM clients through the real listener and
   performs final FORWARD/naming/resource/privacy/M6 regression closure; it
   is blocked because the pinned i2plib/libsam3 sources and build cache are not
   available in this checkout (`plans/148-status.md`).

Milestone 7 remains open until Plan 148 passes. The Plan 147 raw product lane
is valid local product evidence, but does not substitute for the required
independent-client gate.

Milestone 8 is not the next product layer until Plan 148 passes.

## MVP roadmap

- [`000-mvp-roadmap.md`](000-mvp-roadmap.md) — milestone sequence from
  empty repository to the first feature-complete MVP (CLI router,
  NTCP2/SSU2, NetDB + floodfill, tunnel participation, destinations,
  streaming, SAM/I2CP, HTTP/SOCKS5 proxies, service tunnels).
  **Independent-router interoperability is MVP acceptance debt, not a
  prerequisite for the localhost SAM product work in Milestone 7.**
- [`145-m7-sam31-remaining-gap-corrective-roadmap.md`](145-m7-sam31-remaining-gap-corrective-roadmap.md) —
  current Milestone 7 remaining-gap sequence after the Plan 142–144
  acceptance audit.
- [`141-m7-sam31-corrective-roadmap.md`](141-m7-sam31-corrective-roadmap.md) —
  previous Milestone 7 corrective sequence after the blocked Plan 140 audit;
  superseded for next-action authority by Plan 145.
- [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md) —
  destination / garlic / LeaseSet2 / Streaming router construction.
- [`126-129-milestone6-final-corrective-roadmap.md`](126-129-milestone6-final-corrective-roadmap.md)
  and
  [`126-130-milestone6-final-corrective-roadmap.md`](126-130-milestone6-final-corrective-roadmap.md) —
  Milestone 6 final corrective roadmaps (each plan superseded by a
  later closure record).

## Milestone 7 plan hierarchy

| Plan | Current authority status | Record |
| --- | --- | --- |
| 135 | superseded by Plan 140 audit / later corrective roadmaps | [`135-status.md`](135-status.md) |
| 136 | foundation landed; SAM encoding/private-destination evidence superseded by later corrective work | [`136-status.md`](136-status.md) |
| 137 | passed loopback server/session lifecycle | [`137-status.md`](137-status.md) |
| 138 | implementation landed; product acceptance superseded by later raw-stream corrective work | [`138-status.md`](138-status.md) |
| 139 | local FORWARD/naming implementation landed; final byte-path acceptance deferred to Plan 148 | [`139-status.md`](139-status.md) |
| 140 | blocked closure audit; historical authority | [`140-status.md`](140-status.md) |
| 141 | previous corrective roadmap; superseded for next-action authority by Plan 145 | [`141-status.md`](141-status.md) |
| 142 | **Base64 correction retained as passed**; private-destination external compatibility requalified by Plan 146 | [`142-status.md`](142-status.md) |
| 143 | local delivery seam / Plan 129 bridge work retained; **full raw STREAM acceptance superseded by Plan 147** | [`143-status.md`](143-status.md) |
| 144 | **partial local evidence** — in-process bidirectional Streaming handshake; independent-client/raw-socket final closure not passed | [`144-status.md`](144-status.md) |
| 145 | **active corrective roadmap** — current Milestone 7 authority | [`145-status.md`](145-status.md) |
| 146 | **passed** bidirectional private-destination reference requalification; relaxed `from_imported` invariant | [`146-status.md`](146-status.md) |
| 147 | **passed** dedicated raw STREAM socket driver | [`147-status.md`](147-status.md) |
| 148 | next executable — two-independent-client final Milestone 7 closure | [`148-m7-sam31-independent-client-final-closure.md`](148-m7-sam31-independent-client-final-closure.md) |

Do not restore the historical Plan 136/138/142/143 broad `passed` labels as
final M7 authority without satisfying the superseding Plan 146–148 acceptance
criteria.

## Milestone 6 plan hierarchy

Closure records are authoritative; `superseded-by-*` rows are
historical evidence, retained for audit.

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

Milestones 0–5 live under the same `NNN-closure.md` / `NNN-status.md`
naming. The active development interop lane is **closed**; NTCP2 stays
experimental and non-advertised. The historical NTCP2 interop apparatus
lives under `docs/architecture/interop-apparatus.md` and the closed
Plan 038–100 closure records.

The constrained-host lane (Plan 086/099/100 host-loopback development),
the Plan 046 rootless sealed-namespace lane, and the Plan 048–051
Multipass recovery guest are documented in the architecture skill
([`.opencode/skills/i2pr-architecture/SKILL.md`](../.opencode/skills/i2pr-architecture/SKILL.md))
and the dedicated interop skills.

## What's implemented

- Bounded wire protocol codecs (`i2pr-proto`)
- Cryptographic wrappers: Ed25519, X25519, AES, ChaCha20-Poly1305,
  HMAC, SipHash, HKDF-SHA256, ECIES-X25519-AEAD-Ratchet (`i2pr-crypto`)
- Versioned private-identity persistence + NTCP2 static key/IV record
  (`i2pr-storage`)
- Runtime-neutral transport + NTCP2 codec (`i2pr-transport`,
  `i2pr-transport-ntcp2`)
- Tokio-owned runtime with supervision, cancellation, and bounded
  channels (`i2pr-runtime`)
- Deterministic testkit with seeded randomness and fault injection
  (`i2pr-testkit`)
- RouterInfo validation, bounded local NetDB store, lookup and
  publication state machines, signed Standard LeaseSet2 carrier and
  validation (`i2pr-netdb`)
- Persistent RouterInfo cache + bounded SU3 reseed ingestion
  (`i2pr-netdb-persist`)
- Daemon composition root + Plan 117 outbound/inbound NetDB dispatch
  (`i2pr-daemon`)
- Exploratory tunnel substrate, runtime-neutral ECIES-X25519 short
  tunnel-build cryptography, `ShortBuildI2npBridge`, runtime-neutral
  tunnel data plane, and outbound/inbound exploratory NetDB composition
  (`i2pr-tunnel`)
- Local destination runtime: identity, dedicated tunnel pools, signed
  Standard LeaseSet2 generation/lifecycle, ECIES-X25519-AEAD-Ratchet
  session layer, destination routing, I2P Streaming core with
  `StreamingManager` + `StreamingDestinationAdapter` (`i2pr-client`)
- CLI daemon with config validation, identity generation, and dry-run
  (`i2pr-daemon`)
- SAM protocol/session/FORWARD/naming foundations through Plan 139.
- Plan 142's canonical SAM I2P Base64 correction (`-`/`~`, `=` padding).
- Plan 143/144's reusable Plan 129 local-delivery seam and in-process
  bidirectional Streaming SYN/SYN-response handshake regression.
- Plan 147's dedicated raw STREAM socket driver (same-socket
  TCP↔Streaming byte pump, bounded ACK/retransmit runtime driver,
  and localhost byte-product evidence).

## What's not implemented yet

- Live NTCP2 or SSU2 transport (NTCP2 is experimental and
  non-advertised; the production service graph never activates it).
- Live mixed-router tunnel-build execution (depends on a qualified
  external delivery lane).
- Network-transport-bound NetDB lookup / publication (Plan 117
  §8/§10 composition is local-only; the exploratory outbound path is
  wired through `DataPlaneRegistry` + `OutboundGatewayRole`, but the
  network transport adapter still owns the NTCP2/SSU2 handshake
  surface).
- I2NP message handling and router dispatch.
- Milestone 7 SAM closure:
  - two independent SAM clients have not yet moved application bytes through
    the real listener; FORWARD/naming final byte-path/resource/privacy closure
    remains open (Plan 148).
- Client proxies (HTTP, SOCKS5) and service tunnels.
- Any public/network-facing router behavior.

The NTCP2 development interoperability result is
`protocol-defect-localized` at `noise_authenticated`. No passed
mixed-router NTCP2 result exists.

## Working with plans

Before editing or claiming behavioral conformance, load
[`.opencode/skills/i2pr-local-dev/SKILL.md`](../.opencode/skills/i2pr-local-dev/SKILL.md)
for the routine development seam and SAM baseline planning stub, or
[`.opencode/skills/i2pr-architecture/SKILL.md`](../.opencode/skills/i2pr-architecture/SKILL.md)
to navigate the architecture documentation, ADRs, and this directory.

When a closure record and a per-plan narrative disagree, the newest explicit
superseding status record wins. Plans retain their narrative as audit history,
not as live contracts.
