# Plan 310 status — Destination-group multi-hop pools and peer selection

Status: `blocked-service-product-has-no-bounded-multipath-candidate-owner`

## Authority and disposition

Plan 310 was made active by `be04a79` after Plan 309 passed. Investigation confirmed
that the acceptance path still cannot construct a complete qualified path from validated
NetDB candidates or retain those paths in group-owned pools. No production implementation
or anonymity/path-diversity claim is closed by this status. The blocker is the exact
architecture boundary identified by Plan 310's stop conditions, and remains actionable
within the repository.

## Requirement-to-evidence matrix

| Requirement | Repository evidence | Result |
|---|---|---|
| Bounded ordered multi-hop request distinct from exploratory one-peer semantics | `BuildRequest` in `crates/i2pr-daemon/src/exploratory_build.rs` carries one `PeerBuildMaterial`; `ExploratoryBuildCoordinator::submit` constructs `hops: vec![request.peer.hop_spec()]`. `ShortBuildPath` supports a bounded ordered vector and rejects repeated RouterHash values, but the production coordinator does not accept such a vector. | Not implemented |
| Candidate projection and selector sourced from validated NetDB | `RouterInfoStore` validates and exposes iteration over stored `ValidatedRouterInfo` records. However, `ServiceProduct::start` creates a fresh store and `dial_and_bootstrap_router_only` inserts only its single configured reference RouterInfo. No service candidate provider, Java-profile filters, CSPRNG selection, or typed diversity exhaustion is connected to that store. | Not implemented |
| Candidate facts sufficient for selected Java profile | The retained Plan 305 source matrix defines unique RouterHash, Java family-label exclusion (unverified in Java), IPv4 /16 and IPv6 /32 proximity, and role/port exclusions when metadata is available. No projection translates validated RouterInfo into these bounded facts for service selection. | Not implemented |
| Real three-hop inbound and outbound service builds | `provision_all_service_router_material` accepts one `RouterPeerMaterial` and submits one peer for each direction. `DestinationConfig::service_compatibility_profile()` reports three hops, but this is not the submitted path. | Not implemented; acceptance fails closed |
| Group-owned pools, replenish/expire/replace and accounting | Plan 309 gives each Destination group identity/bridge/registry composition. `ServiceProduct::ProductInner` still owns one process-wide `ExploratoryBuildCoordinator`; no group-keyed multi-hop pool lifecycle is installed. | Not implemented |
| Group LeaseSet, lookup, and publication use usable group tunnels | Current service provisioning derives network state from the one-hop coordinator roles. No group pool owner provides multiple usable inbound/outbound paths for LeaseSet or lookup/publication selection. | Not implemented |
| Scarcity, cancellation, resource bounds, and restart | Existing single-build coordinator limits pending builds and the short-path validator rejects repeated routers. There is no selected complete-path attempt lifecycle or group-pool replenishment/restart behavior to test. | Not demonstrated |
| Focused/full Plan 310 acceptance floor | No code implementing the required path/pool owner landed, so the required matrix was not run and no acceptance result is inferred from unrelated green suites. | Not run |

## Investigation commands and outcomes

- `cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1` — passed, 11 tests; this exercises the existing build owner and does not establish multi-hop service paths.
- `cargo test --locked -p i2pr-daemon --lib shared_client_group_members_share_one_destination -- --test-threads=1` — zero tests matched; not counted as evidence.
- `bash scripts/check-service-anonymity-boundaries.sh` — passed.
- `bash scripts/check-service-tunnel-boundaries.sh` — passed.
- `bash scripts/check-dependency-direction.sh` — passed.
- `git diff --check` — passed for the status update.
- Full workspace, Clippy, docs, and Plan 310 required focused suites — not run because no implementation was made and the hard acceptance boundary remains absent.

## Security and compatibility review

No production code, wire behavior, dependency, persistent format, configuration behavior,
or reference pin changed. In particular, no fallback that silently shortens a configured
path was added. The configured three-hop profile remains unqualified, and no group path
diversity or anonymity claim is made.

| Severity | Finding |
|---|---|
| Critical | None identified in this investigation. |
| High | The network-visible service path remains one-hop despite carrying a three-hop compatibility profile. |
| Medium | Validated RouterInfo storage exists, but the service product's candidate population and group-owned multipath lifecycle do not. |
| Low | None. |

## Unblock audit

- **Plan 311:** remains blocked on Plan 310. Its required startup gate and graceful
  retirement operate on usable group pools and LeaseSet lifecycle; existing one-hop
  material is not a substitute.
- **Plan 312:** remains blocked on Plan 310. Its directional baseline requires real
  group-owned Streaming paths and cannot treat the one-hop service product as the
  qualified endpoint.
- **Plan 313:** remains blocked on Plan 312's executed directional evidence.
- **Plan 308:** remains independently blocked on controlled ordinary-HTTP topology and
  three-family captures; Plan 310 has no effect on it.
- No downstream plan became ready. No other Plan 310-dependent plan is eligible under
  its registered hard-dependency graph.

Disposition: blocked at the service-product multi-candidate and group-pool ownership
boundary. Resume Plan 310 by implementing a bounded candidate-to-submitted-path owner,
connecting validated NetDB candidate material to the existing short-build stack, and
installing/replenishing the resulting tunnels under Destination-group ownership. Do not
resume Plans 311–313 until their registered dependency conditions are met.
