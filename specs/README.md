# i2pr protocol specification index

## Purpose

This directory is the protocol research and conformance entry point for `i2pr`. It does not copy the upstream I2P specifications or convert implementation behavior into a new protocol. It records:

- the authoritative specification for each protocol surface required by the MVP;
- the exact upstream revisions reviewed by the project;
- accepted proposals and external standards incorporated by those specifications;
- implementation entry points in Java I2P, I2P+, i2pd, and Emissary/go-i2p;
- the subset `i2pr` must implement, defer, reject, or investigate;
- interoperability, malformed-input, and resource-bound tests required before support is claimed.

Research snapshot: **2026-07-14**.

Start with [SOURCES.md](SOURCES.md), [IMPLEMENTATIONS.md](IMPLEMENTATIONS.md), and [CONFORMANCE.md](CONFORMANCE.md). Protocol implementation plans must cite the relevant dossier below and pin any newer upstream source they use.

## Authority model

The project uses this precedence order:

1. Current English-language specifications published by the official I2P project.
2. Accepted or completed I2P proposals explicitly incorporated by those specifications.
3. External standards normatively referenced by the I2P specification, at the named revision.
4. The official Java I2P implementation where the written specification is ambiguous.
5. Independent interoperable behavior in i2pd.
6. I2P+ behavior where it differs from, tightens, or optimizes Java I2P.
7. Emissary/go-i2p behavior as an independent implementation and architecture reference.

Implementation source is evidence, not permission to contradict a clear specification. When sources disagree, `i2pr` must record the conflict, produce a minimal interoperability test, and avoid silently choosing one behavior.

## MVP protocol map

| Dossier | Roadmap milestone | Required MVP outcome | Initial priority |
|---|---:|---|---|
| [Common structures, identity, and cryptography](protocols/01-common-identity-crypto.md) | 1 | Strict codecs, RouterIdentity, Destination, RouterInfo, LeaseSet types, signing and key handling | Foundation |
| [I2NP](protocols/02-i2np.md) | 1, 3–6 | Router-to-router message envelope and messages needed by transports, NetDB, tunnels, and garlic routing | Foundation |
| [NTCP2](protocols/03-ntcp2.md) | 3 | First authenticated interoperable router transport | First network transport |
| [Reseeding and NetDB](protocols/04-reseed-netdb.md) | 4 | Bootstrap, validate and store network data, query and publish RouterInfo and LeaseSets | Join and operate |
| [Tunnel construction and tunnel messages](protocols/05-tunnels.md) | 5 | Build, maintain, participate in, and forward through unidirectional tunnels | Router data plane |
| [Garlic routing, ECIES, and LeaseSets](protocols/06-garlic-ecies-leasesets.md) | 6 | Destination-to-destination encryption, garlic messages, LeaseSet publication and lookup | End-to-end layer |
| [Streaming](protocols/07-streaming.md) | 6 | Minimal reliable byte streams over I2P messages | First usable destination |
| [SAM](protocols/08-sam.md) | 7 | Interoperable application-facing SAM endpoint | Primary external API |
| [SSU2](protocols/09-ssu2.md) | 8 | Current UDP transport, reachability testing and relay support | Second network transport |
| [I2CP and service tunnels](protocols/10-i2cp-service-tunnels.md) | 9–10 | Low-level client protocol plus HTTP, SOCKS5, generic TCP and IRC adapters | MVP completion |

The ordering is architectural, not a claim that each dossier can be implemented independently. Common structures, I2NP, cryptography, transport blocks, RouterInfo fields, and capability/version policy are cross-cutting.

## Support labels

Every protocol feature tracked by `i2pr` should use one of these labels:

- **required** — necessary for the current MVP and interoperability target;
- **required-later** — required by the MVP but intentionally sequenced after the current milestone;
- **compatibility** — accepted or emitted only to interoperate with deployed routers;
- **experimental** — behind an explicit non-default feature and never advertised as ordinary support;
- **deferred** — valid protocol surface outside the MVP;
- **legacy-reject** — parsed only far enough to reject safely, or not accepted at all;
- **open** — specification or interoperability question that must be resolved before implementation.

A feature is not “supported” merely because its constants or structures exist. Support requires encode/decode coverage, state-machine behavior, negative tests, resource bounds, and mixed-router interoperability where applicable.

## Update procedure

Before implementing a dossier:

1. Compare the pinned official specification commit in [SOURCES.md](SOURCES.md) with the current upstream revision.
2. Review specification metadata such as `lastUpdated` and `accurateFor`; these are guidance, not a substitute for the source diff.
3. Search accepted proposals newer than the pinned snapshot for changes to formats, required features, deprecations, and advertised versions.
4. Compare the relevant Java I2P, i2pd, I2P+, and Emissary/go-i2p paths.
5. Add unresolved differences to the dossier and create executable interoperability vectors where possible.
6. Update the pin ledger and research date in the same commit as any resulting implementation change.

Do not vendor upstream prose into this directory. Preserve links and concise engineering summaries so source licensing, provenance, and future updates remain clear.