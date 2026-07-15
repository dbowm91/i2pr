# Specification source and revision ledger

Research snapshot: **2026-07-14**.

This file pins the upstream revisions used to prepare the `i2pr` MVP protocol dossiers. Canonical website links are convenient for reading; pinned repository links are the reproducible source of what was reviewed.

## Official I2P specification corpus

Repository: [`i2p/i2p.website`](https://github.com/i2p/i2p.website)

Pinned commit: [`88596022920bdf99f27db27688faf4f204792fcd`](https://github.com/i2p/i2p.website/commit/88596022920bdf99f27db27688faf4f204792fcd)

Primary documents:

| Subject | Canonical document | Pinned source | Snapshot metadata |
|---|---|---|---|
| Common structures | [Specification](https://i2p.net/en/docs/specs/common-structures/) | [`common-structures.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/common-structures.md) | Updated 2026-03; accurate for 0.9.68 |
| I2NP | [Specification](https://i2p.net/en/docs/specs/i2np/) | [`i2np.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/i2np.md) | Updated 2026-03; accurate for 0.9.69 |
| NTCP2 | [Specification](https://i2p.net/en/docs/specs/ntcp2/) | [`ntcp2.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/ntcp2.md) | Updated 2026-03; accurate for 0.9.69 |
| SSU2 | [Specification](https://i2p.net/en/docs/specs/ssu2/) | [`ssu2.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/ssu2.md) | Completed; updated 2026-03; accurate for 0.9.69 |
| Streaming | [Specification](https://i2p.net/en/docs/specs/streaming/) | [`streaming.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/streaming.md) | Updated 2023-10; accurate for 0.9.59 |
| I2CP | [Specification](https://i2p.net/en/docs/specs/i2cp/) | [`i2cp.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/i2cp.md) | Updated 2025-07; accurate for 0.9.67 |
| Legacy crypto summary | [Specification](https://i2p.net/en/docs/specs/cryptography/) | [`cryptography.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/cryptography.md) | Marked mostly obsolete; use the current component specs below |
| ECIES destination encryption | [Specification](https://i2p.net/en/docs/specs/ecies/) | [`ecies.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/ecies.md) | Current X25519/ChaCha20-Poly1305 ratchet |
| ECIES router encryption | [Specification](https://i2p.net/en/docs/specs/ecies-routers/) | [`ecies-routers.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/ecies-routers.md) | Router garlic and reply encryption |
| Hybrid/PQ ECIES | [Specification](https://i2p.net/en/docs/specs/ecies-hybrid/) | [`ecies-hybrid.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/ecies-hybrid.md) | Track for compatibility planning; not automatically in the initial subset |
| ECIES tunnel creation | [Specification](https://i2p.net/en/docs/specs/tunnel-creation-ecies/) | [`tunnel-creation-ecies.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/tunnel-creation-ecies.md) | Current tunnel-build encryption |
| Encrypted LeaseSet | [Specification](https://i2p.net/en/docs/specs/encryptedleaseset/) | [`encryptedleaseset.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/encryptedleaseset.md) | LeaseSet privacy format |
| Signed update/SU3 container | [Specification](https://i2p.net/en/docs/specs/updates/) | [`updates.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/specs/updates.md) | Used by signed reseed bundles |
| Reseeding | [Documentation](https://i2p.net/en/docs/misc/reseed/) | [`reseed.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/misc/reseed.md) | Bootstrap policy and bundle behavior |
| SAM v3 | [API specification](https://i2p.net/en/docs/api/samv3/) | [`samv3.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/api/samv3.md) | Application-facing socket protocol |
| I2PTunnel | [API documentation](https://i2p.net/en/docs/api/i2ptunnel/) | [`i2ptunnel.md`](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/content/en/docs/api/i2ptunnel.md) | Behavioral reference for service adapters |

Architecture/background documents are useful for intent but are not substitutes for wire specifications:

- [Protocol stack](https://i2p.net/en/docs/development/protocol-stack/)
- [Transport layer](https://i2p.net/en/docs/overview/transport/)
- [Network database](https://i2p.net/en/docs/overview/network-database/)
- [Tunnel routing](https://i2p.net/en/docs/overview/tunnel-routing/)
- [Garlic routing](https://i2p.net/en/docs/overview/garlic-routing/)
- [Threat model](https://i2p.net/en/docs/overview/threat-model/)

## Proposals directly relevant to the MVP

Proposal text is pinned to the same website commit. A proposal may explain design intent or define a completed feature, but its current specification page takes precedence if they differ.

| Proposal | Relevance |
|---|---|
| [111 — NTCP2](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/111-ntcp-2.txt) | NTCP2 design and migration background |
| [144 — ECIES-X25519-AEAD-Ratchet](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/144-ecies-x25519-aead-ratchet.txt) | Destination end-to-end encryption |
| [145 — ECIES](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/145-ecies.txt) | Encryption type framework and X25519 migration |
| [147 — transport network-ID check](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/147-transport-network-id-check.txt) | Cross-network/replay separation in transports and RouterInfo |
| [152 — ECIES tunnel creation](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/152-ecies-tunnels.txt) | X25519 tunnel build records |
| [157 — new tunnel build messages](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/157-new-tbm.txt) | Short/current tunnel build messages |
| [159 — SSU2](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/159-ssu2.txt) | SSU2 design, threat model and rollout |
| [161 — RouterInfo/Destination padding](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/161-ri-dest-padding.txt) | Canonical structure parsing and fingerprint resistance |
| [165 — SSU2 fixes](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/165-ssu2-fix.txt) | Post-deployment SSU2 corrections |
| [168 — tunnel bandwidth parameters](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/168-tunnel-bandwidth.txt) | Current tunnel-build capability and policy fields |
| [169 — post-quantum cryptography](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/169-pq-crypto.txt) | Hybrid key types now visible in common structures; compatibility watch item |

LeaseSet2, encrypted LeaseSet, MetaLeaseSet and service-record proposal identifiers must be re-verified when Milestones 4 and 6 receive detailed implementation plans. Do not rely on remembered proposal numbers.

## External standards

The I2P specifications normatively or substantially depend on external standards. Follow the exact revisions cited by the official document, not an assumed latest version. The main MVP dependencies include:

- Noise Protocol Framework, revision 33 for NTCP2 and SSU2 handshakes.
- RFC 7748 X25519.
- ChaCha20-Poly1305 as cited by the respective I2P specification.
- SHA-256 and HMAC/HKDF constructions specified by each protocol.
- SipHash-2-4 for NTCP2 frame-length obfuscation.
- QUIC RFC 9000/9001 concepts adapted by SSU2; SSU2 is not QUIC and must not inherit unspecified QUIC behavior.
- WireGuard packet-number/header design concepts adapted by SSU2; SSU2 is not WireGuard.

## Implementation revisions

These are code references, not normative specifications.

| Implementation | Repository | Pinned revision | Role in research |
|---|---|---|---|
| Java I2P | [`i2p/i2p.i2p`](https://github.com/i2p/i2p.i2p) | [`4e1822fcfafdf2b7de33fa14d71960a543a550e0`](https://github.com/i2p/i2p.i2p/commit/4e1822fcfafdf2b7de33fa14d71960a543a550e0) | Official reference implementation |
| I2P+ | [`I2PPlus/i2pplus`](https://github.com/I2PPlus/i2pplus) | [`d45ad75157966bd80903af044d47f1ca14429726`](https://github.com/I2PPlus/i2pplus/commit/d45ad75157966bd80903af044d47f1ca14429726) | Java soft fork; divergence, hardening and operational evidence |
| i2pd | [`PurpleI2P/i2pd`](https://github.com/PurpleI2P/i2pd) | [`361bee94803f0cfd9a3837acdd06a7cc457c4c4e`](https://github.com/PurpleI2P/i2pd/commit/361bee94803f0cfd9a3837acdd06a7cc457c4c4e) | Independent C++ interoperability reference |
| Emissary/go-i2p | [`go-i2p/go-i2p`](https://github.com/go-i2p/go-i2p) | [`be5ad3b9d6290943fb7906130d8a02061b8403c9`](https://github.com/go-i2p/go-i2p/commit/be5ad3b9d6290943fb7906130d8a02061b8403c9) | Independent Go architecture and partial/current implementation reference |

The public source used here is named `go-i2p/go-i2p`. This corpus uses “Emissary/go-i2p” to preserve the project terminology in the `i2pr` roadmap discussion without asserting that every upstream branch or release uses the Emissary name.

## Refresh triggers

Refresh this ledger when any of the following occurs:

- the official specification `accurateFor` version changes;
- a proposal affecting an MVP protocol is accepted, completed, superseded, or deployed;
- Java I2P raises the minimum I2NP feature/API version it will build tunnels through or exchange NetDB messages with;
- an implementation removes legacy behavior or enables a new transport/key type by default;
- mixed-router tests reveal behavior not explained by the pinned documents;
- `i2pr` begins a milestone whose dossier contains an **open** or **compatibility watch** item.