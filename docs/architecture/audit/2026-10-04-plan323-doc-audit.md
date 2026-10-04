# 2026-10-04 Plan 323 architecture documentation update

Scope: targeted doc-vs-source check for the new Proposal 170
`PersistentClientKey` owner in `i2pr-service-tunnels`.

| Surface | Result |
|---|---|
| `crates/i2pr-service-tunnels/src/config.rs` | `DestinationPolicy::PersistentClient` and `PersistentSharedClientGroup` select persistent identity policy; server-only and bidirectional-server constraints remain enforced. |
| `crates/i2pr-service-tunnels/src/config.rs` | `ServiceTunnelSet::destination_groups` marks client groups persistent when their policy requires persistent identity. |
| `crates/i2pr-daemon/src/service_tunnels.rs` | `create_bridge_for_group` reads/writes the existing `ServiceDestinationStore`; restart evidence covers a shared HTTP/generic client Destination. |
| `docs/architecture/i2pr-service-tunnels.md` | Updated the Destination-group contract to distinguish default ephemeral client groups from explicitly persistent client groups. |

Focused evidence: `cargo test --locked -p i2pr-daemon --lib persistent_client_identity_survives_manager_restart -- --test-threads=1`; workspace all-target check and all-feature Clippy; dependency-direction, runtime-boundary, and service-tunnel-boundary scripts.

This is a scoped update for Plan 323, not a full crate documentation audit.
