# Plan 300 closure — Destination isolation and tunnel-path qualification

Status: stopped-target-isolation-and-reference-diversity-owner-gap

## Completed changes

- `ShortBuildPath::validate` now rejects a RouterHash repeated within a path. A deterministic unit regression proves rejection before build-state creation.
- Added `DestinationConfig::service_compatibility_profile` and wired service-tunnel manager composition to it. It uses three hops, matching exact-pinned Java I2P 2.13.0 `TunnelPoolSettings` client inbound/outbound defaults and i2pd 2.61.0 `Config.cpp` HTTP/SOCKS proxy defaults. The i2pr quantity remains bounded at 2: it matches Java's `DEFAULT_QUANTITY = 2` and intentionally differs from i2pd's proxy default 5. Hop length has no random variance in this fixed profile.
- Client Destination configuration continues to use the existing `DestinationConfig::balanced` profile outside M10 service tunnels; this migration is scoped to the service manager.

## Target-fanout and isolation audit

| Service kind | Configured remote target fanout | Client Destination behavior | Evidence / status |
| --- | --- | --- | --- |
| Generic client | One configured `DestinationRef` | One dedicated service identity | `ServiceTunnelSpec::validate`; daemon composition is per service. |
| IRC client | One configured `DestinationRef` | One dedicated service identity | Same fixed-target composition; product policy is separately tested. |
| HTTP client | Arbitrary HTTP proxy targets | One service identity currently serves multiple remote targets | Runtime reuses the service's Destination bridge; target-scoped identities are not implemented. |
| SOCKS5 client | Arbitrary SOCKS5 CONNECT targets | One service identity currently serves multiple remote targets | Runtime reuses the service's Destination bridge; target-scoped identities are not implemented. |
| Generic server | One or more local server targets; accepts inbound remote clients | Server Destination persists under service lifecycle | Does not initiate remote Destination fanout. |
| IRC server | One or more local server targets; accepts inbound remote clients | Server Destination persists under service lifecycle | Does not initiate remote Destination fanout. |

Dedicated service identity remains the default. However, the required stronger property—unrelated targets through HTTP/SOCKS not reusing a client Destination identity—is not met. `SharedClientGroup` is explicit in the config model, but this audit did not establish runtime identity sharing/linkability semantics for it. No identity cache or target isolation was added because service runtime composition currently owns one Destination bridge per service; splitting it into per-target Destinations requires a new lifecycle/budget owner and cancellation/restart contract.

## Path policy and evidence

The repeat-router check is now enforced at the short-build path boundary. There is no peer-candidate selection owner in the current tunnel build API: callers supply an already-resolved `ShortBuildPath`, and that owner has no reference-backed family/network metadata. Consequently no family/prefix diversity, inbound/outbound endpoint reuse exclusion, typed candidate-rejection reasons, or explicit insufficient-diversity outcome could be wired without an architectural owner change. No arbitrary prefix rule was introduced.

Pinned reference source inspection at Java `9134f808337b401e8e53c73734c81fab04280c9d` and i2pd `635b013a612ff47278ef02acf8580a28e10e26c5` established the tunnel defaults cited above. It did not qualify i2pr path-selection diversity.

## Verification

- `cargo fmt --all --check`: passed.
- `cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1`: passed, 397 tests (including `repeated_router_is_rejected_before_build_state_is_created`).
- The combined service-tunnels/client/tunnel/NetDB/daemon package run at the host descriptor limit 256 failed in the existing `sam_forward_naming::forward_unresponsive_target_times_out_within_policy` test with `EMFILE`. The same suite passed with per-process `ulimit -n 1024`: `cargo test --locked -p i2pr-service-tunnels -p i2pr-client -p i2pr-tunnel -p i2pr-netdb -p i2pr-daemon --all-targets -- --test-threads=1`.
- Exact source pins verified with `git -C /tmp/java-plan296 rev-parse HEAD` and `git -C /tmp/i2pd-plan296 rev-parse HEAD`.

## Unblock audit

Plan 301 cannot pass: Plan 297 lacks executed HTTP family captures, Plan 299 lacks Plan 298 differential evidence, and Plan 300 lacks target isolation plus a reference-backed path-diversity/degraded-outcome contract. Plans 297–300 are not otherwise unblocked by this partial corrective. No M12/mainline dependency changes. A follow-on plan needs to define target-scoped Destination lifecycle ownership and path candidate metadata/selection before this qualification can resume.
