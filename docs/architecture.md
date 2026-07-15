# Architecture

This document records the current modular-monolith boundaries and ownership.
The implemented common-structure codecs remain structural data handling, not
router behavior or an interoperability claim.

## Four planes

The intended modular monolith is organized into four conceptual planes:

| Plane | Responsibility | Milestone 0 status |
| --- | --- | --- |
| Data | Protocol representations, authenticated links, messages, and network tunnel traffic | Bounded common-structure model plus primitive codecs; no crypto, socket, or network behavior |
| Control | Configuration, lifecycle, health, cancellation, supervision, and resource budgets | CLI validation plus runtime-neutral core contracts |
| Client | Destinations, LeaseSets, streaming, SAM, and I2CP adapters | Not implemented |
| Service | HTTP, SOCKS5, IRC, generic TCP, and local service-tunnel composition | Not implemented |

Network tunnels carry router-to-router I2P traffic and are distinct from
application service tunnels, which eventually connect a local application to a
destination. The latter must not import transport internals or peer-profile
storage.

## Initial crate graph

The bootstrap has only four crates:

```text
i2pr-proto   (sha2 for fixed hash derivation; no workspace dependencies)
      ^                 ^
      |                 |
i2pr-core  <------ i2pr-testkit
      ^
      |
i2pr-daemon  (composition root; also depends on i2pr-proto)
```

The arrows show dependency direction. `i2pr-proto` owns protocol-facing names,
bounds, and typed codec error categories. It now also owns immutable Mapping,
certificate/key-type, RouterIdentity, Destination, RouterAddress, RouterInfo,
Lease, and classic LeaseSet values. Parsed signed records retain the exact
signed region. Its cursor borrows input, its encoder requires caller-visible
output limits, and strict top-level decoders reject trailing bytes. It has no
runtime, filesystem, CLI, transport, or tracing-subscriber dependency; the
only external dependency is the reviewed `sha2` crate for SHA-256 hash
derivation. `i2pr-core` owns runtime-neutral service,
health, lifecycle, cancellation, and resource-domain types. `i2pr-testkit`
provides deterministic clocks, randomness, and bounded fault vocabulary for
tests. The daemon owns CLI/configuration and is the future composition root.

The direction is mechanically checked by
`scripts/check-dependency-direction.sh`. Production crates do not depend on
`i2pr-testkit`.

### Common-structure boundary

`i2pr-proto::common` validates wire shape, bounded sizes, canonical mapping
order, algorithm-specific public-material lengths, and exact signed-byte
boundaries. It does not verify signatures, generate secrets, decide timestamp
freshness, interpret transport options, publish RouterInfo, or construct
LeaseSet2-family records. Those responsibilities belong to later crypto,
storage, NetDB, and client plans.

### Cancellation scope

The current `i2pr-core::CancellationToken` is runtime-neutral bootstrap
machinery: an atomic cancellation flag for cooperative polling. It records a
cancellation request but does not provide async wake semantics or async wait
and selection operations. Runtime-specific cancellation integration remains at
runtime-facing service boundaries; this bootstrap does not introduce a
generalized runtime abstraction.

## Composition and communication

The daemon will eventually compose supervised services and pass each service
only the narrow handles or capabilities it needs. A global mutable router
context or unrestricted service locator is not an architectural default.

The planned service model classifies work as essential, restartable,
degradable, or optional. Each long-lived service will declare startup
dependencies, readiness, health signals, owned resources, cancellation, and
graceful/forced shutdown behavior. Milestone 0 defines the lifecycle and
resource vocabulary but does not implement a supervisor or asynchronous
service graph.

## External boundaries

Future `synvoid` integration belongs behind a local Unix-socket or loopback
service boundary. It is not a routing-core dependency. Future `eggsec`
integration belongs in `i2pr-testkit`, private-testnet orchestration, and
stable fixtures; production routing code must not expose unrestricted testing
hooks.

The project is a modular monolith, not a runtime plugin platform. Compile-time
components or authenticated out-of-process interfaces are preferred to
in-process Rust plugins.
