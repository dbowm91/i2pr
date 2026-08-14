# Plan 107: Milestone 5 exploratory tunnel substrate — closure record

- Status: **closed**.
- Date: 2026-08-14.
- Plan-of-record: `plans/107-milestone-5-exploratory-tunnel-substrate.md`.
- Authority parent: Plan 102 and the Plan 102 amendment.
- Milestone: 5 (Network tunnel data plane and exploratory tunnels).

## Summary

Plan 107 landed the first Milestone 5 implementation surface. The plan
delivered a new runtime-neutral crate `i2pr-tunnel` that owns the
typed tunnel identity, a bounded exploratory pool, the build-record
layout surface, a typed build-cryptography seam, and a reply-path
provider adapter. The Plan 106 NetDB seam now consumes the reply-path
provider and flips its
`ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable` to
`Available` once a real inbound tunnel is registered.

The plan did **not** activate NTCP2, advertise tunnels, or run a
live mixed-router build. The exploratory pool is filled through an
injected registration path; the production registrar that performs
real builds lands in Plan 108+.

## Work packages

| Package | Surface | Notes |
| --- | --- | --- |
| 1 | `crates/i2pr-tunnel/src/identity.rs` | `TunnelId`, `TunnelDirection`, `TunnelRole`, `TunnelLifetime`, `TunnelState`, `TunnelPeer` |
| 2 | `crates/i2pr-tunnel/src/config.rs` | `ExploratoryPoolConfig` with hard ceilings; `balanced()` constructor |
| 3 | `crates/i2pr-tunnel/src/pool.rs` | Deterministic `ExploratoryPool` with replacement/expiry/failure accounting |
| 4 | `crates/i2pr-tunnel/src/build.rs` | `BuildRecordLayout` (Short/Variable), `BuildRequestKind` |
| 5 | `crates/i2pr-tunnel/src/build_crypto.rs` | `BuildCryptography` trait, `LayerKeys` zeroizing wrapper, `NoBuildCryptography` default |
| 6 | `crates/i2pr-tunnel/src/provider.rs` | `ExploratoryPoolReplyPathProvider` adapter |
| 7 | `crates/i2pr-tunnel/src/lib.rs` | crate-root façade |
| 8 | `crates/i2pr-netdb/src/lookup_id.rs` | new `ReplyPathProvider` trait |
| 9 | `crates/i2pr-netdb/src/lookup_engine.rs` | `RouterInfoLookup::accept_reply_path` |
| 10 | `crates/i2pr-netdb/src/lib.rs` | re-export `ReplyPathProvider` |
| 11 | `crates/i2pr-daemon/src/netdb_seam.rs` | `set_reply_path_provider` / `clear_reply_path_provider` / `path_status` / `begin_lookup` consume the provider |
| 12 | `crates/i2pr-proto/src/i2np/tunnel.rs` | `DeferredBuildRecords::new` promoted to `pub` so external crates can construct it |
| 13 | `docs/architecture/i2pr-tunnel.md` | new crate deep-dive |
| 14 | `docs/architecture/overview.md`, `docs/architecture/i2pr-netdb.md`, `docs/architecture/i2pr-daemon.md` | updated to reflect Plan 107 |
| 15 | `docs/protocol-support.md`, `specs/support.toml` | support matrix bump; new `tunnel.identity-and-pool`, `tunnel.reply-path-provider`, `tunnel.build-cryptography-seam`, `tunnel.live-build` surfaces |
| 16 | `plans/107-milestone-5-exploratory-tunnel-substrate.md` | plan-of-record |
| 17 | `AGENTS.md`, `README.md` | Plan 107 section, support-status table, Plan 102 amendment sequence |

## Validation

The local validation matrix shown below is the result of the
focused Plan 107 closure pass on this host.

```text
cargo +1.95.0 fmt --all --check                  OK
cargo +1.95.0 check --locked --workspace --all-targets   OK
cargo +1.95.0 test --locked --workspace          30 i2pr-tunnel + 117 i2pr-netdb + 53 i2pr-runtime + 50 i2pr-daemon + 24 i2pr-daemon integration tests + 40 i2pr-storage + 12 i2pr-crypto + 14 i2pr-core + 7 i2pr-proto + 5 i2pr-transport + 31 i2pr-transport-ntcp2 + 28 i2pr-testkit + 5 i2pr-cli + 11 i2pr-storage + 6 i2pr-transport + 7 i2pr-storage + 5 i2pr-cli + 4 i2pr-runtime + 1 i2pr-crypto = ~458 tests pass
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps   OK
bash scripts/check-dependency-direction.sh        OK
bash scripts/check-runtime-boundaries.sh          OK
bash scripts/check-fixture-manifest.sh            OK
bash scripts/check-ntcp2-vectors.sh               OK
bash scripts/check-ntcp2-interoperability.sh      OK
bash scripts/check-multipass-interop-boundary.sh  OK
git diff --check                                  OK
```

The Plan 046 rootless checker continues to report a pre-existing
baseline failure (`tests/integration/ntcp2/harness/rootless_supervisor.py`
was retired by the Plan 099 harness-reduction commit). Plan 107 does
not modify any rootless-owned file and the baseline failure is
unrelated to Plan 107.

## Milestone 4A status after Plan 107

```text
routerinfo_validation             = implemented (Plan 103)
local_netdb                       = implemented (Plan 103)
persistent_routerinfo_cache       = implemented (Plan 104)
su3_reseed_verification           = implemented (Plan 104)
reseed_ingestion                  = implemented (Plan 104)
netdb_query_state_machine         = implemented (Plan 105)
routerinfo_publication_state      = implemented (Plan 105)
netdb_daemon_integration          = implemented (Plan 106)
exploratory_tunnel_substrate       = implemented (Plan 107)
reply_path_provider               = implemented (Plan 107)
build_cryptography_seam           = implemented (Plan 107)
live_ecies_x25519_build           = blocked-on-plan108
live_mixed_router_build           = blocked-on-plan108-and-qualified-transport
live_routerinfo_lookup            = blocked-on-plan108-and-qualified-transport
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
external_netdb_over_ntcp2         = blocked
milestone4_full_exit              = pending-cross-milestone-checkpoint
```

## Next executable plan

The next executable implementation is **Plan 108** (live
ECIES-X25519 build-encryption primitive and live mixed-router
tunnel build). Plan 108 will land the live cryptographic primitive
behind the `BuildCryptography` seam, the deterministic simulation
backed by the Plan 107 pool, and the integration with the Plan 098
forward direction in the host-loopback development lane.

## Handoff

Plan 107 closes the first Milestone 5 implementation surface.
The Plan 107 implementation surface is mandatory; any change that
removes or weakens the substrate, the seam wiring, or the static
boundary checks must be re-justified in a new plan-of-record and
must not silently weaken the Milestone 4 evidence gate.
