# Protocol support

`i2pr` is an experimental Rust I2P router, not production-ready. Nothing here is
a privacy, anonymity, censorship-resistance, or general compatibility guarantee.

This page summarizes the scope of current evidence. The closure records and
[`plans/registry.md`](../plans/registry.md) carry current status;
[`specs/CONFORMANCE.md`](../specs/CONFORMANCE.md) defines what evidence supports
a claim. The structured inventory is in [`specs/support.toml`](../specs/support.toml).
A local test proves local behavior only. Public advertisement and router-to-router
claims require separate evidence for the exact protocol path.

## Current evidence

| Area | Evidence and limits |
| --- | --- |
| Common codecs, cryptography, NetDB | Selected bounded codecs, cryptographic wrappers, identity handling, and local NetDB/reseed paths have tests. This is a subset, not complete router compatibility. See the [protocol dossiers](../specs/README.md). |
| Destinations, LeaseSet2, Streaming | The local destination and Streaming product is closed. Selected NetDB, destination-delivery, and Streaming paths passed against pinned i2pd. Full two-family M6 router conformance is not claimed. Encrypted LeaseSet2 type 5 remains non-advertised and live end-to-end interoperability is not claimed. [Plan 134](../plans/closure/destination-streaming/134-status.md), [Plan 193](../plans/closure/mixed-router-interop/193-status.md) |
| SAM 3.1 and 3.3 | SAM 3.1 final acceptance and the experimental SAM 3.3 PRIMARY/subsession profile passed. Plan 368 includes pinned Java I2P 2.13.0 evidence. The SAM service is loopback-only, disabled by default, and not advertised for public or remote-router use. [Plan 151](../plans/closure/sam/151-status.md), [Plan 368](../plans/closure/sam/368-status.md) |
| SSU2 v2 | Direct IPv4 interoperability passed in a bounded loopback test against pinned i2pd 2.61.0. This does not claim public-network support. IPv6 interoperability remains infrastructure-limited, SSU1 is not implemented, and post-quantum session establishment is deferred. [Plan 161](../plans/closure/ssu2/161-status.md) |
| I2CP | Independent Java and Go clients passed within the loopback qualification scope. No remote-I2CP or public-network claim is made; `HostLookup` is not implemented. [Plan 172](../plans/closure/i2cp/172-status.md) |
| Service tunnels | The M10 product and scoped independent application qualification passed. Listeners are disabled by default and bind to loopback by default. The outproxy policy and route owner have no reachable request path, so there is no working outproxy. [Plan 215](../plans/closure/service-tunnels/215-status.md) |
| NTCP2 | Local protocol foundations exist, but there is no router-to-router interoperability claim and the production daemon does not activate the transport. NTCP2 remains experimental and non-advertised. See the [NTCP2 dossier](../specs/protocols/03-ntcp2.md) and [roadmap](../plans/subsystems/ntcp2-transport-roadmap.md). |
| Transit and floodfill | One-family experimental M11 progression passed. Public transit remains disabled, non-advertised, and unclaimed. M12 floodfill has no second-family qualification or broad `caps=f` advertisement. See [Plan 268](../plans/closure/transit-tunnels/268-status.md), [Plan 306](../plans/closure/floodfill/306-status.md). |
| Proposal 170 / I2PControl | Partial implementation and scoped evidence exist, but full Proposal 170 conformance and broad capability advertisement are not claimed. The outproxy request path is not usable. See the [current registry](../plans/registry.md) and [roadmap](../plans/subsystems/i2pcontrol-proposal-170-roadmap.md). |

## More detail

- [Protocol dossiers and source index](../specs/README.md)
- [Conformance and evidence policy](../specs/CONFORMANCE.md)
- [Machine-readable support inventory](../specs/support.toml)
- [Current plan and closure index](../plans/registry.md)
- [Architecture overview](architecture.md)
