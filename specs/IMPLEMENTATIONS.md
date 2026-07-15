# Implementation cross-reference

This document identifies where the reviewed routers implement the protocol surfaces tracked by the `i2pr` MVP. It is a navigation aid and an interoperability checklist, not a substitute for reading the official specification.

All links are pinned to revisions listed in [SOURCES.md](SOURCES.md).

## How to use implementation evidence

For each protocol implementation, compare at least the following:

- parser acceptance and rejection boundaries;
- state transitions, retransmission and timeout behavior;
- maximum message, block, record, collection and queue sizes;
- version/capability advertisement and peer gating;
- clock-skew, replay and duplicate suppression;
- error handling and connection/tunnel teardown;
- persistence validation and corruption recovery;
- behavior under resource exhaustion;
- tests, vectors and comments that explain deviations from the prose specification.

Do not reproduce architecture merely because it exists in a mature router. `i2pr` should preserve wire behavior while retaining its own crate boundaries, bounded service model and explicit resource governor.

## Java I2P

Repository root: [`i2p/i2p.i2p`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0)

Java I2P is the official reference implementation. It is the first implementation to inspect where a specification is underspecified, but historical compatibility branches and JVM-specific architecture are not automatically requirements for `i2pr`.

| Surface | Primary source area |
|---|---|
| Common structures, RouterInfo, Destination, LeaseSets, I2NP types | [`core/java/src/net/i2p/data`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/core/java/src/net/i2p/data) and [`router/java/src/net/i2p/data/i2np`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/router/java/src/net/i2p/data/i2np) |
| Cryptographic wrappers and signature/encryption types | [`core/java/src/net/i2p/crypto`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/core/java/src/net/i2p/crypto) |
| NTCP2 | [`router/.../transport/ntcp`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/router/java/src/net/i2p/router/transport/ntcp), especially `NTCP2Options`, `NTCP2Payload`, establishment states and `NTCPConnection` |
| SSU2 | [`router/.../transport/udp`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/router/java/src/net/i2p/router/transport/udp) |
| NetDB and floodfill behavior | [`router/.../networkdb`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/router/java/src/net/i2p/router/networkdb) |
| Tunnel construction, participation and data forwarding | [`router/.../tunnel`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/router/java/src/net/i2p/router/tunnel) |
| Garlic routing and client message routing | [`router/.../message`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/router/java/src/net/i2p/router/message) and client-message components under `router` |
| Streaming | [`apps/streaming/java/src`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/apps/streaming/java/src) |
| SAM | [`apps/sam/java/src`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/apps/sam/java/src) |
| I2CP | Client types under `core` and router-side handlers under `router`; begin from the I2CP message classes and session handlers |
| I2PTunnel, HTTP, SOCKS, generic TCP and IRC adapters | [`apps/i2ptunnel/java/src`](https://github.com/i2p/i2p.i2p/tree/4e1822fcfafdf2b7de33fa14d71960a543a550e0/apps/i2ptunnel/java/src) |
| Reseed and signed containers | Router reseed components plus SU3 classes under `core` |

Java I2P contains long-lived compatibility behavior. Every copied constant or acceptance path must be tied to a current requirement, a deployed-compatibility need, or an explicit legacy-reject test.

## I2P+

Repository root: [`I2PPlus/i2pplus`](https://github.com/I2PPlus/i2pplus/tree/d45ad75157966bd80903af044d47f1ca14429726)

I2P+ is a soft fork of Java I2P and retains substantially the same package structure. Compare the same directories listed above at the I2P+ pin. It is especially valuable for:

- recent operational fixes and defensive checks;
- streaming congestion, pacing and queue behavior;
- NetDB and Kademlia edge cases;
- router-console-independent configuration choices;
- performance-oriented caching and allocation changes;
- differences in peer selection, floodfill policy, bandwidth handling and tunnel management.

Verified navigation examples include:

- [`router/java/src/net/i2p/router/transport/ntcp/OutboundNTCP2State.java`](https://github.com/I2PPlus/i2pplus/blob/d45ad75157966bd80903af044d47f1ca14429726/router/java/src/net/i2p/router/transport/ntcp/OutboundNTCP2State.java)
- [`router/java/src/net/i2p/router`](https://github.com/I2PPlus/i2pplus/tree/d45ad75157966bd80903af044d47f1ca14429726/router/java/src/net/i2p/router)
- [`apps/streaming/java/src`](https://github.com/I2PPlus/i2pplus/tree/d45ad75157966bd80903af044d47f1ca14429726/apps/streaming/java/src)

Because I2P+ shares lineage with Java I2P, agreement between the two is not independent interoperability evidence. A behavior found only in I2P+ must be labeled as a fork-specific policy or candidate hardening measure until verified elsewhere.

## i2pd

Repository root: [`PurpleI2P/i2pd`](https://github.com/PurpleI2P/i2pd/tree/361bee94803f0cfd9a3837acdd06a7cc457c4c4e)

The C++ implementation is the most important independent interoperability reference for the full router protocol stack.

| Surface | Primary source area |
|---|---|
| Common structures and RouterInfo | [`libi2pd/RouterInfo.cpp`](https://github.com/PurpleI2P/i2pd/blob/361bee94803f0cfd9a3837acdd06a7cc457c4c4e/libi2pd/RouterInfo.cpp) and related data/identity headers |
| I2NP | [`libi2pd/I2NPProtocol.h`](https://github.com/PurpleI2P/i2pd/blob/361bee94803f0cfd9a3837acdd06a7cc457c4c4e/libi2pd/I2NPProtocol.h) and corresponding implementation |
| NTCP2 | [`libi2pd/NTCP2.h`](https://github.com/PurpleI2P/i2pd/blob/361bee94803f0cfd9a3837acdd06a7cc457c4c4e/libi2pd/NTCP2.h) and [`NTCP2.cpp`](https://github.com/PurpleI2P/i2pd/blob/361bee94803f0cfd9a3837acdd06a7cc457c4c4e/libi2pd/NTCP2.cpp) |
| SSU2 | [`libi2pd/SSU2Session.cpp`](https://github.com/PurpleI2P/i2pd/blob/361bee94803f0cfd9a3837acdd06a7cc457c4c4e/libi2pd/SSU2Session.cpp) and neighboring SSU2 files |
| NetDB | [`libi2pd/NetDb.cpp`](https://github.com/PurpleI2P/i2pd/blob/361bee94803f0cfd9a3837acdd06a7cc457c4c4e/libi2pd/NetDb.cpp) |
| Tunnels | `libi2pd/Tunnel*`, tunnel-build and tunnel-gateway sources |
| Garlic/ECIES | `libi2pd/Garlic*`, `ECIESX25519AEADRatchet*`, and crypto helpers |
| Destinations, streaming, SAM, I2CP and service adapters | [`libi2pd_client`](https://github.com/PurpleI2P/i2pd/tree/361bee94803f0cfd9a3837acdd06a7cc457c4c4e/libi2pd_client) plus daemon configuration |
| Runtime/configuration behavior | [`daemon`](https://github.com/PurpleI2P/i2pd/tree/361bee94803f0cfd9a3837acdd06a7cc457c4c4e/daemon) and [`contrib/i2pd.conf`](https://github.com/PurpleI2P/i2pd/blob/361bee94803f0cfd9a3837acdd06a7cc457c4c4e/contrib/i2pd.conf) |

Use i2pd to detect assumptions accidentally inherited from Java object models, scheduling, persistence or API conventions. Where Java I2P and i2pd differ but both interoperate, prefer the smallest behavior set permitted by the specification and test both peers.

## Emissary/go-i2p

Repository root: [`go-i2p/go-i2p`](https://github.com/go-i2p/go-i2p/tree/be5ad3b9d6290943fb7906130d8a02061b8403c9)

This implementation is useful because it is a newer non-Java router with a more modular source layout. Its project status and protocol coverage must be verified from code and tests rather than assumed from README claims.

| Surface | Primary source area |
|---|---|
| Router composition | [`lib/router`](https://github.com/go-i2p/go-i2p/tree/be5ad3b9d6290943fb7906130d8a02061b8403c9/lib/router) |
| NTCP2 router integration | [`lib/router/router_ntcp2.go`](https://github.com/go-i2p/go-i2p/blob/be5ad3b9d6290943fb7906130d8a02061b8403c9/lib/router/router_ntcp2.go) |
| NTCP2 protocol implementation | [`lib/transport/ntcp2`](https://github.com/go-i2p/go-i2p/tree/be5ad3b9d6290943fb7906130d8a02061b8403c9/lib/transport/ntcp2), including address, block, framing, handshake/session and transport files |
| I2NP | [`lib/i2np/i2np.go`](https://github.com/go-i2p/go-i2p/blob/be5ad3b9d6290943fb7906130d8a02061b8403c9/lib/i2np/i2np.go), garlic-session and build-record crypto files |
| Other protocol packages | `lib` subpackages for NetDB, tunnels, common structures, crypto and client protocols |
| Transport diagnostics | [`docs/transport-observability.md`](https://github.com/go-i2p/go-i2p/blob/be5ad3b9d6290943fb7906130d8a02061b8403c9/docs/transport-observability.md) |

The implementation is particularly relevant to Rust design questions around explicit state, package boundaries, test seams and reduced coupling. Do not assume completeness for SSU2, streaming, SAM or all legacy crypto modes without examining current tests and successful mixed-router evidence.

## Required comparison record for implementation work

A detailed implementation plan for a protocol must add a table with at least these columns:

| Question | Official spec | Java I2P | i2pd | I2P+ | Emissary/go-i2p | `i2pr` decision/test |
|---|---|---|---|---|---|---|
| Accepted versions/capabilities | | | | | | |
| Maximum encoded sizes/counts | | | | | | |
| Timeout and skew policy | | | | | | |
| Duplicate/replay handling | | | | | | |
| Unknown-field/block behavior | | | | | | |
| Error/teardown behavior | | | | | | |
| Persistence validation | | | | | | |

A blank cell is an unresolved research item, not evidence that behavior is absent.