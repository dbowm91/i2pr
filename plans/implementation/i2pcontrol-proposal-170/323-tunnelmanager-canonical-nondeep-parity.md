# Plan 323 — Canonical TunnelManager envelope and non-deep option parity

Status: **in-progress-prop170-tunnelmanager-canonical-parity**

Classification: capability + protocol reconciliation.

Hard dependency: Plan 320 closed. May execute in parallel with Plans 321 and 325.

## Objective

Make TunnelManager behavior match the canonical Proposal 170 envelope and implement every non-cryptographic parameter that can be supported through existing i2pr owners, while retaining the single M10 ServiceTunnelManager runtime.

## Canonical request model

Implement the exact top-level Proposal fields instead of the former lowercase/nested-options public shape.

Required behavior includes:
- Name and Action;
- All for start/stop/restart;
- Type on create and NewName on edit;
- common Port/TargetHost/Host/TargetPort/TargetDestination/Destination/StartOnLoad/Description/ReachableBy/Shared/UseSSL/tunnel-shaping fields;
- full client proxy, client management, HTTP filter and server option sets;
- LeaseSet fields routed to Plan 326;
- exact aliases only where the Proposal lists aliases.

Conflicting aliases in one request fail before mutation.

## Bulk action semantics

All=true is valid only for start, stop and restart.

Take one bounded snapshot of eligible control-owned/startup-owned tunnels under the ownership rules, apply deterministic ordering, and return per-tunnel results without unbounded fanout.

Define whether startup-owned tunnels are bulk-controllable. The decision must preserve the Plan 289 provenance invariant and be documented in the canonical result.

## Non-deep option owners

Implement or map the remaining Proposal fields that are not cryptographic/LeaseSet/outproxy work, including as applicable:
- Description, ReachableBy, Shared;
- ConnectDelay, DelayOpen;
- Reduce/ReduceCount/ReduceTime;
- Close/CloseTime;
- NewDest and PersistentClientKey lifecycle semantics;
- ProxyList routing input without outproxy-plugin activation;
- ProxyAuth/ProxyUsername/ProxyPassword;
- HTTP AllowUserAgent/AllowReferer/AllowAccept/AllowInternalSSL;
- WebsiteHostname/SpoofedHost;
- BlockAccessInProxies, BlockUserAgents/UserAgents, BlockReferers;
- AccessOption/AccessList/FilterFilePath;
- MaxConcurrentConns and per-client/total minute/hour/day/post/period/ban rate controls;
- SSLProxies/JumpList where they have safe real I2P semantics;
- UseSSL for every Proposal-applicable client/server family.

Each accepted field needs a named runtime owner. Persist-only/inert acceptance is prohibited.

## CustomOptions

Do not treat CustomOptions as an arbitrary bypass around typed validation.

Either:
- parse a documented bounded allowlist into the same typed owners; or
- reject unknown/unsafe keys explicitly.

CustomOptions must never introduce arbitrary filesystem, direct-clearnet, plugin loading, or secret logging capabilities.

## PrivKeyFile

Treat PrivKeyFile as a logical key reference inside an owned key root, not an arbitrary host path. Enforce path confinement/special-file/symlink rules and persistent identity ownership.

## Exact results

Get must emit the exact Proposal info/status structure while filtering secret rawConfig values. Create/edit/start/stop/restart/delete must use exact status/results shape.

## Evidence

Build a generated field×tunnel-type applicability matrix from the canonical inventory and require every cell to be:
- apply with named owner;
- not applicable by Proposal/type;
- deep prerequisite owned by Plans 324/326/327.

No generic “unsupported” bucket may contain non-deep fields at closure.

Test All actions, aliases, conflicts, ranges, rates, persistence/restart, secret redaction, transactional rollback, ownership collisions and all twelve existing backend data paths.

## Acceptance criteria

Plan 323 closes when the canonical TunnelManager public surface is operational for every non-deep field, all twelve types remain real backends, and only destination-algorithm, encrypted-LeaseSet/client-auth, or outproxy-provider cells remain dependency-gated.

Closure unblocks Plans 324 and 327.

## Current implementation progress

- The existing top-level canonical request decoder already validates `All`; Plan 323 now executes start/stop/restart over one sorted, bounded snapshot of control-owned definitions and returns per-tunnel results. Startup-owned TOML definitions are excluded to preserve provenance and mutation ownership. The daemon dispatch no longer substitutes an unavailable marker for these actions.
- Focused evidence: `cargo test --locked -p i2pr-daemon --lib plan323_all_lifecycle_uses_sorted_control_owned_snapshot -- --test-threads=1` covers start, restart, stop, sorted output, and startup-owned exclusion. `cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels tunnelmanager_emits_canonical_proposal_result_and_redacts_secrets -- --test-threads=1` verifies the canonical wire envelope and per-tunnel bulk results.
- This is one Plan 323 slice only. Canonical Get/result shapes, the field×type applicability matrix, and non-deep option ownership remain open; the plan stays in progress and Plans 324/327 stay blocked.
