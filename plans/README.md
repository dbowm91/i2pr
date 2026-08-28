# i2pr plans

Plan-of-record and closure records for `i2pr`. Closure records
(`NNN-status.md`) are authoritative; per-plan narratives
(`NNN-name.md`) are historical context, not live contracts.

The current **Milestone 6 local closure authority** is
[**Plan 134**](134-status.md) (`passed-milestone6-recv-window-ack-ceiling-closure`).
Independent-router interoperability is not claimed and is tracked as
external acceptance debt. The current **Milestone 7 SAM 3.1 closure
authority** is the blocked [**Plan 140**](140-status.md)
(`blocked-independent-client-stream-path-not-ready`). Plan 139 remains
the last passed local SAM hardening closure; Plan 140's live STREAM and
independent-client evidence is not claimed.

## MVP roadmap

- [`000-mvp-roadmap.md`](000-mvp-roadmap.md) — milestone sequence from
  empty repository to the first feature-complete MVP (CLI router,
  NTCP2/SSU2, NetDB + floodfill, tunnel participation, destinations,
  streaming, SAM/I2CP, HTTP/SOCKS5 proxies, service tunnels).
  **Independent-router interoperability is MVP acceptance debt, not a
  prerequisite for beginning Milestone 7 in the constrained development
  environment.**
- [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md) —
  destination / garlic / LeaseSet2 / Streaming router construction.
- [`126-129-milestone6-final-corrective-roadmap.md`](126-129-milestone6-final-corrective-roadmap.md)
  and
  [`126-130-milestone6-final-corrective-roadmap.md`](126-130-milestone6-final-corrective-roadmap.md) —
  Milestone 6 final corrective roadmaps (each plan superseded by a
  later closure record).

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
- SAM 3.1 local product path is implemented through Plan 139; independent
  client interoperability and final Milestone 7 evidence remain in Plan 140.
- Client proxies (HTTP, SOCKS5) and service tunnels.
- Any network-facing behavior.

The NTCP2 development interoperability result is
`protocol-defect-localized` at `noise_authenticated`. No passed
mixed-router NTCP2 result exists.

## Working with plans

Before editing or claiming behavioral conformance, load
[`.opencode/skills/i2pr-local-dev/SKILL.md`](../.opencode/skills/i2pr-local-dev/SKILL.md)
for the routine development seam and SAM baseline planning stub, or
[`.opencode/skills/i2pr-architecture/SKILL.md`](../.opencode/skills/i2pr-architecture/SKILL.md)
to navigate the architecture documentation, ADRs, and this directory.

When a closure record and a per-plan narrative disagree, the closure
record wins. Plans retain their narrative as audit history, not as
live contracts.
