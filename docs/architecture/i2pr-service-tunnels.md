# `i2pr-service-tunnels` — Deep Dive

Runtime-neutral Milestone 10 service-tunnel configuration, destination references, and policy.

Path: `crates/i2pr-service-tunnels/`

## Purpose

`i2pr-service-tunnels` owns the Plan 174 foundation required by every M10 application tunnel without adding an HTTP, SOCKS, IRC, or generic service listener yet:

- bounded typed identifiers and service kinds;
- strict destination reference policy (Base32 / static alias / configured public material);
- loopback-only listener/target shapes;
- central resource and deadline ceilings;
- validated service-tunnel sets;
- typed errors and events carrying no sockets or secrets.

It must not own:

- Tokio, sockets, listeners, tasks, timers;
- filesystem access;
- transport or tunnel-build internals;
- NetDB mutation;
- Garlic/I2NP construction;
- SAM or I2CP protocol parsing.

The daemon remains the sole M10 socket/task/composition owner. Future HTTP/SOCKS/IRC parsers and policies belong here (Plans 176–179); generic client/server composition belongs to the daemon (Plan 175).

## Module layout

| Module | File | Responsibility | Key public types |
| --- | --- | --- | --- |
| `lib` | `src/lib.rs` | Crate root, re-exports, architecture pointer | `ServiceTunnelId`, `DestinationRef`, `ServiceTunnelError` |
| `config` | `src/config.rs` | Typed kinds, policy, listener/target, limits, timeouts, set validation | `ServiceTunnelKind`, `DestinationPolicy`, `LocalListenerSpec`, `ServerTarget`, `ServiceResourceLimits`, `ServiceTimeouts`, `ServiceTunnelSpec`, `ServiceTunnelSet` |
| `destination` | `src/destination.rs` | Base32/alias/configured parsing, alias table | `DestinationRef`, `StaticAliasTable` |
| `errors` | `src/errors.rs` | Typed structural errors, no secrets | `ServiceTunnelError` |
| `events` | `src/events.rs` | Value-only lifecycle events and snapshots | `ServiceTunnelEvent`, `ServiceTunnelSnapshot` |

Line counts (approximate at Plan 174 close): `lib.rs` ~40, `config.rs` ~600, `destination.rs` ~400, `errors.rs` ~90, `events.rs` ~110.

## Public surface

```text
pub use config::{LocalListenerSpec, ServerTarget, ServiceClientGroupId,
  ServiceResourceLimits, ServiceTimeouts, ServiceTunnelId,
  ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec,
  DestinationPolicy, MAX_*};
pub use destination::{DestinationRef, StaticAliasTable};
pub use errors::ServiceTunnelError;
pub use events::{ServiceTunnelEvent, ServiceTunnelSnapshot};
```

## Key contracts

- `#![forbid(unsafe_code)]` in every module.
- Every count/length/deadline has a hard typed ceiling:
  32 services, 64 aliases, 64-byte IDs, 128 conns per service,
  1024 aggregate, 1024–1048576 buffered bytes per direction,
  8 configured targets, connect 1–120 s, read/write 1–600 s,
  shutdown 1–30 s, 52-char Base32, 67-byte alias, 255-byte Unix path.
- `ServiceTunnelSet::validate()` rejects duplicate IDs, duplicate binds, contradictory options, and aggregate overflow before daemon state changes.
- `DestinationRef::parse()` is structural only: no DNS, filesystem, network, or clearnet fallback; IP literals rejected.
- `StaticAliasTable` rejects duplicates, conflicts, malformed targets, and ceiling overflow.
- Client kinds require listener + destination and forbid server targets; server kinds require target(s), forbid listener and remote destination, and require dedicated policy.
- Events and snapshots carry counts and identifiers only.

## Dependencies

From `Cargo.toml`: `i2pr-proto`, `thiserror`. No Tokio, no socket libraries, no HTTP/SOCKS/parser frameworks.

From `scripts/check-dependency-direction.sh`: allowed workspace deps are `i2pr-client` and `i2pr-proto`. The Plan 174 implementation uses `i2pr-proto` only; the `i2pr-client` edge is explicitly allowed for future destination/Streaming-facing reuse without becoming an accidental omission.

## Tests

- `src/config.rs` unit tests: all six kinds parse, duplicate IDs/listeners rejected, non-loopback listener/target rejected, contradictory options rejected, bounds enforced.
- `src/destination.rs` unit tests: canonical Base32 parses, wrong length/alphabet rejected, mixed-suffix tricks rejected, NUL/control/whitespace rejected, IP literals rejected, overlong/duplicate/malformed aliases rejected, alias ceiling enforced.
- `src/events.rs` unit tests: empty snapshot, value-only events.
- Daemon-side: `crates/i2pr-daemon/src/config.rs` unit tests (disabled-by-default, unknown fields, loopback, enabled rejection, duplicates, bounds) and `crates/i2pr-daemon/tests/service_tunnels_foundation.rs` (4 black-box graph tests).
- Pump-side: `crates/i2pr-daemon/src/destination_streaming.rs` (5 deterministic pump tests).

## Distinctive design choices

1. Runtime-neutral by construction; the boundary script proves no Tokio or listener ownership.
2. Kinds are typed enum values, not strings, after parsing.
3. Destination references never touch the network during validation.
4. Static aliases are lower-case and bounded; b32 spellings are never aliases.
5. Server targets distinguish loopback TCP from Unix path values.
6. Shared client destinations require an explicit group; sharing is never implicit.
7. Contradictory client/server options fail before daemon mutation.
8. The crate never decides execution availability; the daemon rejects `enabled = true` until Plan 175.
9. Errors are typed and carry truncated values only, never secrets.
10. Snapshots are point-in-time counters, never memory-backed queues.

## Cross-references

- Plans 173 (roadmap authority) and 174 (foundation implementation).
- `docs/architecture/i2pr-daemon.md` (shared pump + config surface).
- `docs/architecture/i2pr-client.md` (destination/Streaming reuse).
- `specs/protocols/11-service-tunnels.md` (M10 dossier).
- ADRs 0001 (modular monolith), 0002 (Tokio boundary), 0010 (transport contracts).
