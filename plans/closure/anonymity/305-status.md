# Plan 305 closure — target-scoped Destination and peer-diversity ownership

Status: stopped-service-destination-multihop-target-owner-missing

Implementation commits: none. Plan 305's implementation attempt stopped after confirming
that its two required production owners cannot be attached to the current service product
path as a bounded local correction. The exact reference source matrix is retained at
`tests/integration/anonymity/reference-diversity-matrix.md`. No target-isolation or
path-diversity capability is claimed.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Exact reference diversity contract | Source matrix completed for the locked Java I2P 2.13.0 and i2pd 2.61.0 revisions. Java's client selector is the single selected profile for future service-path work; its /16 IPv4, /32 IPv6, family-label, port, and endpoint-role behavior is distinguished from i2pd's /24, /56 and certificate-verified family behavior. No i2pr selector was added. |
| Target-scope key and bounded lifecycle owner | Not implemented. HTTP/SOCKS currently resolve each remote target and continue using the single `ServiceRuntime.destination_id`; no child Destination identity/lease/driver owner exists. |
| Target-scoped HTTP and SOCKS identities | Not implemented. Distinct canonical remote hashes still enter Streaming through the service's one Destination bridge. Same-hash aliases/ports do not have a per-target lifecycle contract. |
| Capacity, cancellation, and reconcile semantics | Not implemented. `ServiceTunnelManager` stages one Destination per service runtime and its generation-owned indexes/drivers are keyed by that runtime. Dynamic children would require a manager lifecycle owner that can transactionally install and drain all those indexes. |
| Explicit `SharedClientGroup` behavior | Runtime composition rejects non-`Dedicated` policies in `ServiceTunnelGeneration::prepare`; it does not implement a shared Destination group. No new configuration behavior was introduced. |
| Candidate metadata/provider | Not implemented. `RouterInfoStore` retains validated records but the active service provisioning path receives one `RouterPeerMaterial`; it has no bounded candidate enumeration/metadata projection feeding the service build owner. |
| Destination peer selector | Not implemented. The current `BuildRequest` contains one `PeerBuildMaterial`, and `ExploratoryBuildCoordinator::submit` constructs `hops: vec![request.peer.hop_spec()]`. |
| Three-hop service build and typed diversity exhaustion | Not demonstrated. `DestinationConfig::service_compatibility_profile()` records the Java-aligned three-hop policy, but `provision_all_service_router_material` builds each service's outbound and inbound tunnel from one `RouterPeerMaterial`. Adding an unused selector would not make the real path three-hop. |
| Plan 300 repeated-router guard | Existing `ShortBuildPath::validate` repeated-router rejection remains in place. It is not a substitute for candidate selection. |
| NetDB namespace/provenance, server persistence, fixed-target regressions | No code changed, so existing owners and storage paths were not modified. Required regression suites were not rerun in this stopped pass. |
| Full workspace/security floor | Not run; no implementation landed and the service path ownership mismatch stopped acceptance. |
| Broad anonymity/path claim | None added. |

## Architecture boundary found

The current service path is not a target-scoped multi-hop build owner:

1. `ServiceRuntime` owns one `destination_id`; `service_tunnels_http.rs` and
   `service_tunnels_socks5.rs` resolve a remote `ClientTarget` but open Streaming using
   that service id.
2. `provision_all_service_router_material` in `service_product.rs` accepts one
   `RouterPeerMaterial` and uses it for both direction builds.
3. `BuildRequest` in `exploratory_build.rs` owns one `PeerBuildMaterial`; `submit` creates
   a one-element `ShortBuildPath`.
4. Generation indexes, inbound dispatch owners, delivery drivers, router-backed state,
   and Destination registry entries are provisioned and drained together as service
   generation state, not dynamically per target.

Plan 305 cannot truthfully claim per-target identities or a Java-derived three-hop path by
adding only a cache/selector at the HTTP/SOCKS connection boundary. The follow-on needs a
new architecture decision and an integrated service-Destination pool owner before it can
be scoped as a dependency-ready corrective. No fallback to the existing shared identity or
one-hop path is authorized by this closure.

## Reference and source audits

- i2pd exact source: `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Java exact source: `9134f808337b401e8e53c73734c81fab04280c9d`.
- `git -C <Java checkout> rev-parse HEAD` — returned `9134f808337b401e8e53c73734c81fab04280c9d`.
- `git -C <i2pd checkout> rev-parse HEAD` — returned `635b013a612ff47278ef02acf8580a28e10e26c5`.
- `rg` inspection covered the exact Java `ClientPeerSelector`, `TunnelPoolSettings`,
  `MaskedIPSet`, and `TunnelPeerSelector` source files and the exact i2pd NetDB,
  RouterInfo, config, and TunnelPool source files.
- Repository source inspection covered HTTP/SOCKS target resolution, `ServiceRuntime`,
  `ServiceTunnelManager` registry/generation ownership, `provision_all_service_router_material`,
  `BuildRequest`, and `ExploratoryBuildCoordinator::submit`.

No Cargo tests were run for Plan 305 because no production or test code changed. These
source/boundary checks passed locally on the stopped implementation head:

- `bash scripts/check-service-anonymity-boundaries.sh` — passed.
- `bash scripts/check-service-tunnel-boundaries.sh` — passed.
- `git diff --check` — passed.

These checks guard existing boundaries; they are not represented as Plan 305 implementation
tests.

## Compatibility, security, and findings

No production code, configuration semantics, dependency, server persistence, or identity
storage changed. The following issues remain visible and unqualified:

| Severity | Finding |
| --- | --- |
| Critical | None. |
| Medium | The M10 service Destination configuration carries a three-hop compatibility profile, but the current service provisioning request constructs one-hop paths. No three-hop anonymity property is established. |
| Medium | Multi-target HTTP/SOCKS continues to reuse one service Destination identity; target-scoped identity ownership is absent. |
| Medium | The exact-reference candidate/diversity policy is not connected to the service build path; no family/network scarcity result exists. |
| Low | `SharedClientGroup` is modeled but current generation preparation rejects it rather than composing a shared runtime. No runtime linkability behavior is claimed. |

## Unblock audit and roadmap disposition

- Plan 300 remains stopped. No fresh Plan-300 qualification corrective is dependency-ready because neither target-scoped ownership nor the production path selector exists.
- Plan 301 remains stopped on missing passing HTTP, Streaming, and Destination/path qualification successors.
- Plan 304's HTTP/Streaming successors remain unready for the independent hostile-reference packet-boundary reason in `304-status.md`.
- No registered downstream plan became ready through Plan 305; no plan status was changed downstream.
- There is no other dependency-ready anonymity implementation plan after 304 and 305. A follow-on production plan must first record the service multi-hop/target-owner architecture decision.
- No anonymity, unlinkability, or path-diversity claim is authorized.

Disposition: stopped at the service-Destination multi-hop and dynamic target-owner boundary; retain this record and the exact source matrix for a future architecture decision.
