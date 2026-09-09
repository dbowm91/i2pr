# `i2pr-service-tunnels` — Deep Dive

Runtime-neutral Milestone 10 service-tunnel configuration,
destination references, policy, plus the Plan 175 generic
client/server tunnel composition surface owned by the daemon.

Path:
- `crates/i2pr-service-tunnels/` (configuration + reference parsing).
- `crates/i2pr-storage/src/service_destination.rs` (persistent server destinations).
- `crates/i2pr-daemon/src/service_tunnels.rs` (manager + listener runtime).

## Purpose

Plan 175 lands the first complete Milestone 10 application service
product. It builds on the Plan 174 foundation (`ServiceTunnelSet`,
`LocalListenerSpec`, `ServerTarget`, bounded resource/timeouts) and
adds:

- bounded typed identifiers and service kinds;
- strict destination reference policy (Base32 / static alias /
  configured public material);
- loopback-only listener/target shapes;
- central resource and deadline ceilings;
- validated service-tunnel sets;
- typed errors and events carrying no sockets or secrets;
- a versioned, atomic, secret-safe persistent service-destination
  storage seam;
- a daemon-owned manager that wires loopback TCP, I2P Streaming, and
  the existing Plan 149 local destination product path together.

It must not own:

- Tokio, sockets, listeners, tasks, timers;
- filesystem access (lives in `i2pr-storage`);
- transport or tunnel-build internals;
- NetDB mutation;
- Garlic/I2NP construction;
- SAM or I2CP protocol parsing;
- HTTP, SOCKS, or IRC parsers (Plans 176-179).

The daemon remains the sole M10 socket/task/composition owner.
Higher protocol profiles (HTTP/SOCKS/IRC) belong to later plans.

## Module layout

| Module | File | Responsibility | Key public types |
| --- | --- | --- | --- |
| `lib` | `crates/i2pr-service-tunnels/src/lib.rs` | Crate root, re-exports, architecture pointer | `ServiceTunnelId`, `DestinationRef`, `ServiceTunnelError` |
| `config` | `crates/i2pr-service-tunnels/src/config.rs` | Typed kinds, policy, listener/target, limits, timeouts, set validation | `ServiceTunnelKind`, `DestinationPolicy`, `LocalListenerSpec`, `ServerTarget`, `ServiceResourceLimits`, `ServiceTimeouts`, `ServiceTunnelSpec`, `ServiceTunnelSet` |
| `destination` | `crates/i2pr-service-tunnels/src/destination.rs` | Base32/alias/configured parsing, alias table | `DestinationRef`, `StaticAliasTable` |
| `errors` | `crates/i2pr-service-tunnels/src/errors.rs` | Typed structural errors, no secrets | `ServiceTunnelError` |
| `events` | `crates/i2pr-service-tunnels/src/events.rs` | Value-only lifecycle events and snapshots | `ServiceTunnelEvent`, `ServiceTunnelSnapshot` |
| `service_destination` | `crates/i2pr-storage/src/service_destination.rs` | Versioned, atomic, secret-safe persistent service destination storage | `ServiceDestinationStore`, `ServiceDestinationRecord`, `ServiceDestinationStorageError` |
| `service_tunnels` | `crates/i2pr-daemon/src/service_tunnels.rs` | Generic client/server tunnel composition root | `ServiceTunnelManager`, `ServiceTunnelManagerConfig`, `ServiceRuntime`, `ServiceTunnelSnapshot`, `ClientTarget`, `DestinationFailure` |

Line counts (approximate at Plan 175 close): see files.

## Public surface

```text
i2pr_service_tunnels:
  pub use config::{ServiceTunnelId, ServiceClientGroupId, ServiceTunnelKind,
    DestinationPolicy, LocalListenerSpec, ServerTarget, ServiceResourceLimits,
    ServiceTimeouts, ServiceTunnelSpec, ServiceTunnelSet, MAX_*};
  pub use destination::{DestinationRef, StaticAliasTable};
  pub use errors::ServiceTunnelError;
  pub use events::{ServiceTunnelEvent, ServiceTunnelSnapshot};

i2pr_storage:
  pub ServiceDestinationStore, ServiceDestinationRecord,
  ServiceDestinationStorageError, decode_service_destination_bytes,
  MAX_SERVICE_DESTINATION_FILE_SIZE,
  SERVICE_DESTINATION_FILE_NAME, SERVICE_DESTINATIONS_SUBDIR,
  SERVICE_DESTINATION_FORMAT_VERSION.

i2pr_daemon::service_tunnels:
  pub ServiceTunnelManager, ServiceTunnelManagerConfig, ServiceRuntime,
  ServiceTunnelSnapshot, ClientTarget, DestinationFailure,
  register_service_tunnel_manager.
```

## Key contracts

### `i2pr-service-tunnels` (Plan 174 foundation, retained)

- `#![forbid(unsafe_code)]` in every module.
- Every count/length/deadline has a hard typed ceiling:
  32 services, 64 aliases, 64-byte IDs, 128 conns per service,
  1024 aggregate, 1024-1048576 buffered bytes per direction,
  8 configured targets, connect 1-120 s, read/write 1-600 s,
  shutdown 1-30 s, 52-char Base32, 67-byte alias, 255-byte Unix path.
- `ServiceTunnelSet::validate()` rejects duplicate IDs, duplicate
  binds, contradictory options, and aggregate overflow before daemon
  state changes.
- `DestinationRef::parse()` is structural only: no DNS, filesystem,
  network, or clearnet fallback; IP literals rejected.
- `StaticAliasTable` rejects duplicates, conflicts, malformed
  targets, and ceiling overflow.
- Client kinds require listener + destination and forbid server
  targets; server kinds require target(s), forbid listener and remote
  destination, and require dedicated policy.

### Plan 175 generic client/server execution

Plan 175 enables `enabled = true` for `generic-client` and
`generic-server` only. `http-client`, `socks5-client`, `irc-client`,
and `irc-server` remain rejected as not-yet-available until their
own plans.

### Persistent server destinations (`i2pr-storage`)

The Plan 175 persistent service destination format is independent of
Rust layout and serde:

| Region | Size | Contents |
| --- | ---: | --- |
| Magic | 8 | `I2PRSD\0\0` |
| Header | 16 | version (1), reserved (0), signing algorithm (7 = Ed25519), static algorithm (4 = X25519) |
| Payload | 448 | Ed25519 seed, X25519 static secret, derived signing public key, derived X25519 public key, destination padding |
| Integrity | 32 | SHA-256 over header+payload |

Version 1 accepts only Ed25519 signing (type 7) and X25519 static
keys (type 4). The format:

- is integrity-protected by SHA-256;
- never accepts truncation, trailing bytes, unsupported versions,
  checksum changes, or derived public-key mismatches;
- is permission-hardened (file `0o600`, directory `0o700`);
- is atomic and no-replace (same-directory temporary + `hard_link`
  install; `AlreadyExists` is fail-closed);
- secret material is non-`Clone`, redacted `Debug`, and zeroized on
  drop.

A reload/reconcile never generates a new identity because a
component restart occurred. Corruption or wrong-version files fail
closed; identity rotation is never a side effect of reload.

### Service tunnel manager (`i2pr-daemon`)

- `ServiceTunnelManager::new(config)` builds a manager from a
  validated `ServiceTunnelSet` and `StaticAliasTable` plus the router
  data directory.
- `prepare()` builds each enabled service's destination runtime,
  binds loopback TCP listeners for client tunnels, and installs the
  per-service Streaming listener for server tunnels.
- `start_supervisors(runtimes, children, cancellation)` spawns the
  per-service supervisor loops under the daemon's child scope.
- `shutdown()` cancels every per-service supervisor token.
- `lookup_local_service_destination(hash)` resolves a Base32 hash to
  a [`ClientTarget`] when the hash matches a service destination
  registered with the manager (cross-tunnel local delivery).
- `service_destination_b64(service_id)` returns the canonical SAM
  private-destination wrapper base64 for one service.
- `snapshot()` returns a sanitized accounting view (counts and
  identifiers only; no secrets).

The manager never logs private destination material, signing seeds,
static secrets, raw payloads, or base64 of the private destination.

## Dependencies

From `Cargo.toml`:
- `i2pr-service-tunnels` depends on `i2pr-proto` + `thiserror`.
- `i2pr-daemon` depends on `i2pr-service-tunnels` for the typed spec
  surface and on the SAM bridge infrastructure for destination
  composition.
- `i2pr-storage` adds `service_destination` module depending on
  `i2pr-crypto` (keys, hashing).

From `scripts/check-dependency-direction.sh`: the workspace graph is
unchanged. From `scripts/check-runtime-boundaries.sh`:
`i2pr-service-tunnels` is runtime-neutral (`#![forbid(unsafe_code)]`,
no Tokio, sockets, listeners, tasks, or timers); the daemon owns all
M10 sockets and tasks.

## Tests

- `crates/i2pr-service-tunnels/src/config.rs` unit tests: all six
  kinds parse, duplicate IDs/listeners rejected, non-loopback
  listener/target rejected, contradictory options rejected, bounds
  enforced.
- `crates/i2pr-service-tunnels/src/destination.rs` unit tests:
  canonical Base32 parses, wrong length/alphabet rejected, mixed-suffix
  tricks rejected, NUL/control/whitespace rejected, IP literals
  rejected, overlong/duplicate/mal mappings aliases rejected, alias
  ceiling enforced.
- `crates/i2pr-storage/src/service_destination.rs` unit tests:
  round-trip preserves secrets, existing destination never replaced,
  truncation at every boundary rejected, checksum / version / public
  mutation rejected, Unix permissions and symlink rejection,
  invalid service-id rejection.
- Daemon-side: `crates/i2pr-daemon/src/config.rs` unit tests
  (disabled-by-default, unknown fields, loopback, enabled rejection
  per kind, duplicates, bounds) and
  `crates/i2pr-daemon/tests/service_tunnels_foundation.rs`
  (Plan 174 black-box config/graph tests) plus
  `crates/i2pr-daemon/tests/service_tunnel_generic_product.rs`
  (Plan 175 manager-level tests: manager prepare, restart-stable
  identity, corrupt-identity rejection, missing-target rejection,
  Unix-target not-yet-supported, generic-server-only, client
  target lookup, snapshot accounting, disabled-service ignored).
- Pump-side: `crates/i2pr-daemon/src/destination_streaming.rs` (5
  deterministic pump tests, retained from Plan 174).

## Distinctive design choices

1. Runtime-neutral configuration by construction; the boundary
   script proves no Tokio or listener ownership in
   `i2pr-service-tunnels`.
2. Kinds are typed enum values, not strings, after parsing.
3. Destination references never touch the network during validation.
4. Static aliases are lower-case and bounded; b32 spellings are
   never aliases.
5. Server targets distinguish loopback TCP from Unix path values;
   Unix targets are explicit `not-yet-supported` rather than silently
   ignored.
6. Shared client destinations require an explicit group; sharing is
   never implicit.
7. Contradictory client/server options fail before daemon mutation.
8. Persistent service destinations follow the same versioned,
   permission-hardened, atomic, no-replace contract as the router
   identity (see ADR 0006).
9. Only `generic-client` and `generic-server` may be `enabled = true`
   in this plan; HTTP/SOCKS/IRC kinds remain rejected with an
   explicit field-level message.
10. The manager exposes only the public Destination b64; raw secrets
    stay inside `ServiceDestinationRecord` and `DestinationIdentity`.
11. Errors are typed and carry truncated values only, never secrets.
12. Snapshots are point-in-time counters, never memory-backed queues.
13. The cross-tunnel local-delivery path resolves through the manager
    itself so client tunnels do not need an external LeaseSet
    lookup to talk to a server tunnel owned by the same router.

## Cross-references

- Plans 173 (roadmap authority), 174 (foundation), 175 (this plan).
- `docs/architecture/i2pr-daemon.md` (manager + runtime surface).
- `docs/architecture/i2pr-storage.md` (persistent destination storage).
- `docs/architecture/i2pr-service-tunnels.md` (foundation deep dive).
- `specs/protocols/11-service-tunnels.md` (M10 dossier).
- ADRs 0001 (modular monolith), 0002 (Tokio boundary), 0006 (private
  identity storage), 0010 (transport contracts).