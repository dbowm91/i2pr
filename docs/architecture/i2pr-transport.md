# `i2pr-transport` — Deep Dive

Runtime-neutral, bounded transport contracts. No Tokio, no I/O, no
async. Pure data and decision types that the runtime adapter drives
synchronously.

Path: `crates/i2pr-transport/`

## Purpose

`i2pr-transport` owns the contracts that any runtime — NTCP2 today,
SSU2 or others in future — must satisfy:

- Link lifecycle FSM with explicit, validated transitions.
- Owned delivery requests with typed outcomes, deadlines, and
  cancellation tokens.
- Manager-level admission with double-checked locking, RAII leases,
  and a deterministic duplicate-resolution policy.
- Privacy-safe bounded observations and aggregate snapshots.
- Transport-shaped resource accounting (pending handshakes, active
  links, buffered bytes, queue items) layered atop `i2pr-core`'s
  shared `ResourceBudget`.

It does **not** own a runtime, sockets, timers, NetDB state, tunnel
state, or client delivery.

## Module layout

Flat — declared in `src/lib.rs:16-23`. No subdirectories.

| File | Responsibility | Main public types |
| --- | --- | --- |
| `src/lib.rs` | Crate root, re-exports core resource types, `type AddressObservation = ReachabilityObservation` | `AddressObservation` |
| `src/types.rs` | Small bounded vocabulary shared across contracts | `LinkId`, `DeliveryId`, `Deadline`, `Confidence`, `TransportKind`, `Direction`, `TerminationCategory` + error enums |
| `src/identity.rs` | Redacted peer references used as map keys | `PeerId` |
| `src/lifecycle.rs` | Explicit finite link lifecycle transitions | `LinkState`, `InvalidLinkTransition` |
| `src/delivery.rs` | Owned delivery requests and typed outcomes | `DeliveryRequest`, `QueuedDelivery`, `DeliveryOutcome` |
| `src/payload.rs` | Bounded owned encoded I2NP messages | `EncodedI2npMessage`, `PayloadError` |
| `src/resource.rs` | Transport-shaped use of the shared resource governor | `TransportLimits`, `TransportResources`, `TransportLease`, `TransportQueueLease`, `TransportResourceLimitsError` |
| `src/manager.rs` | Synchronous transport-manager decisions and accounting | `TransportManager`, `LinkCandidate`, `PendingHandshake`, `DuplicateLinkPolicy`, `LinkDeliveryCapability`, + enums |
| `src/snapshot.rs` | Privacy-safe bounded transport observations | `TransportSnapshot`, `LinkSnapshot`, `ReachabilityObservation`, `LinkResourceUsage`, `SnapshotError` |
| `src/tests.rs` | `#[cfg(test)]` synchronous unit tests | — |

## Public surface

### Re-exports from `i2pr-core` (`lib.rs:11-14`)
```rust
pub use i2pr_core::{
    ResourceBudget, ResourceBundle, ResourceClass, ResourceError,
    ResourceLease, ResourceLimit, ResourceRequest, ResourceUsage,
};
```

### Constants
- `MAX_LINK_ID`, `MAX_I2NP_MESSAGE_BYTES`, `MAX_DEADLINE`,
  `MAX_LINK_SNAPSHOT_ENTRIES`, `MAX_REACHABILITY_OBSERVATIONS`,
  `MAX_TRANSPORT_RESOURCE_LIMIT`, `MAX_TRANSPORT_QUEUE_CAPACITY`.

### Crate-root alias
- `type AddressObservation = ReachabilityObservation;` (`lib.rs:46`).

### Per-module highlights
- `types.rs`: `LinkId`, `DeliveryId`, `Deadline`, `AddressOrigin`,
  `AddressFamily`, `Reachability`, `ValidationState`, `Confidence`,
  `TransportKind`, `Direction`, `LinkDirection` (alias),
  `TerminationCategory`, plus `LinkIdError`, `DeadlineError`,
  `ConfidenceError`.
- `identity.rs`: `PeerId` — `Debug` prints `"PeerId(..)"`; `Display`
  prints `"peer"`.
- `lifecycle.rs`: `LinkState` (Candidate → Handshaking →
  Authenticated → Draining → Closing → Closed; `Failed` sink),
  `InvalidLinkTransition`.
- `delivery.rs`: `DeliveryRequest`, `QueuedDelivery`,
  `DeliveryOutcome` (10 variants).
- `payload.rs`: `EncodedI2npMessage` (bounded, `Debug` shows length
  only), `PayloadError`.
- `resource.rs`: `TransportLimits`, `TransportResources`,
  `TransportLease`, `TransportQueueLease`,
  `TransportResourceLimitsError`.
- `manager.rs`: `DuplicateResolution`, `DuplicateLinkPolicy`,
  `RegistrationError`, `CandidateDecision`, `RegistrationRejection`,
  `CloseOutcome`, `DialBackoff`, `DialBackoffError`,
  `ReachabilityRecordOutcome`, `LinkCandidate`,
  `LinkDeliveryCapability`, `TransportManager`, `PendingHandshake`.
- `snapshot.rs`: `LinkResourceUsage`, `LinkSnapshot`,
  `ReachabilityObservation`, `TransportSnapshot`, `SnapshotError`.

`pub trait` count: **zero.** Every contract is a concrete struct/enum.

## Key contracts

### Link lifecycle (`lifecycle.rs`)
- `LinkState::transition()` (`lifecycle.rs:26`) — pure state machine
  method; returns `Result<Self, InvalidLinkTransition>`. Rejects
  illegal transitions; **all transitions are sync — no `async fn`**.
- `is_authenticated()` (`lifecycle.rs:54`) — true for `Authenticated`
  | `Draining` | `Closing`.
- `is_live()` (`lifecycle.rs:59`) — true for everything except
  `Closed` | `Failed`.

### Delivery (`delivery.rs`)
- `DeliveryRequest` (`delivery.rs:13`) — owned, non-cloneable
  outbound request carrying `DeliveryId`, `PeerId`,
  `EncodedI2npMessage`, `Deadline`, and optional
  `CancellationToken`. No channel, no future, no async trait.
- `QueuedDelivery` (`delivery.rs:110`) — request retained by a link
  queue with exact resource grants. RAII: drops release accounting.
- `DeliveryOutcome` (`delivery.rs:179`) — 10-variant typed outcome.
  Notably **not** `impl Error` — callers match on variants rather
  than reading error strings.

### Admission (`manager.rs`)
- `TransportManager::begin_handshake()` (`manager.rs:487`) — admits
  one `PendingHandshake` lease from the `PendingHandshakes` class.
- `TransportManager::register_authenticated()` (`manager.rs:510`) —
  admits an authenticated `LinkCandidate`, enforcing peer and global
  limits plus the duplicate policy. Returns `CandidateDecision`.
- `TransportManager::enqueue_delivery()` (`manager.rs:665`) —
  enqueues against the first authenticated link for the target peer,
  with a double-lock admission pattern (check → lease → re-check →
  insert).
- `PendingHandshake` (`manager.rs:906`) — RAII handshake lease;
  consumed by `.register()` or released by drop.

### Duplicate resolution (`manager.rs`)
- `DuplicateLinkPolicy` (`manager.rs:44`) — deterministic,
  direction-aware winner selection via local/remote hash ordering.
  Pure function, no mutation.
- `DuplicateResolution` (`manager.rs:25`) — 4-variant policy:
  `AcceptNew`, `ReplaceExisting`, `RejectNew`,
  `RetainExistingDrainNew`.

### Observations / snapshots
- `ReachabilityObservation` (`snapshot.rs:51`) — transport-neutral
  address observation with no raw endpoint. Bounded ring buffer of
  `MAX_REACHABILITY_OBSERVATIONS = 64`.
- `TransportSnapshot` (`snapshot.rs:70`) — deterministic aggregate:
  links sorted by `LinkId`, observations in insertion order,
  resource usage in class order.
- `TransportManager::snapshot()` (`manager.rs:831`) — privacy-safe:
  capped at `MAX_LINK_SNAPSHOT_ENTRIES = 256`; no raw hash bytes,
  no endpoints.

### Resource accounting (`resource.rs`)
- `TransportResources` (`resource.rs:192`) wraps
  `i2pr_core::ResourceBudget` with transport-specific classes:
  `PendingHandshakes`, `ActiveLinks`, `BufferedBytes`,
  `CommandQueueItems`.
- `TransportLease` / `TransportQueueLease` — RAII wrappers.
  `TransportQueueLease` performs an atomic two-class bundle
  acquisition (item count + bytes).
- `TransportLimits` (`resource.rs:20`) — 7 bounded ceilings
  validated at construction; scoped limits (`max_links_per_peer`,
  `max_messages_per_link`, `max_bytes_per_link`) are cross-checked
  against their global counterparts.

## Errors

All error types implement `Display + Error + Clone + Copy + Debug + Eq
+ PartialEq`. No protocol-vs-operational mixing within a single enum.

| Error | Module | Semantics |
| --- | --- | --- |
| `LinkIdError` | `types.rs:81` | Protocol: zero, too-large, exhausted |
| `DeadlineError` | `types.rs:155` | Protocol: monotonic deadline > 7-day bound |
| `ConfidenceError` | `types.rs:234` | Protocol: score > 100 |
| `PayloadError` | `payload.rs:65` | Protocol: empty or oversized encoded I2NP |
| `InvalidLinkTransition` | `lifecycle.rs:66` | Protocol: illegal FSM transition |
| `TransportResourceLimitsError` | `resource.rs:123` | Configuration: zero, oversized, scoped > global |
| `RegistrationError` | `manager.rs:79` | Operational: poisoned, resource exhausted, missing link, duplicate ID, invalid transition |
| `DialBackoffError` | `manager.rs:234` | Operational: retry time exceeds monotonic bound |
| `SnapshotError` | `snapshot.rs:83` | Operational: snapshot failure or too many links |
| `DeliveryOutcome` | `delivery.rs:179` | Ad-hoc typed result, **not** `impl Error` |

## Dependencies

`Cargo.toml:11-12`:
```toml
[dependencies]
i2pr-core   = { path = "../i2pr-core" }
i2pr-proto  = { path = "../i2pr-proto" }
```
That is the entire dependency list. No dev-dependencies.

Position: `i2pr-proto ← i2pr-core ← i2pr-transport ← i2pr-transport-ntcp2`,
and `i2pr-runtime` above transport. Confirmed compliant.

## Tests

Inline `src/tests.rs` (`lib.rs:25-26`). 11 synchronous tests:

| Test | Line | Coverage |
| --- | --- | --- |
| `payload_bounds_and_diagnostics_are_safe` | 74 | `LinkId`/`EncodedI2npMessage` bounds, `PeerId` redaction |
| `first_link_limits_and_duplicate_decisions_are_typed` | 88 | All 4 `DuplicateResolution` variants |
| `active_limit_and_stale_close_preserve_replacements` | 143 | Global link limit, stale close idempotency |
| `pending_handshake_lease_releases_on_drop_and_completion` | 193 | RAII semantics |
| `queue_item_and_byte_leases_release_on_drop_and_handoff` | 236 | RAII + handoff |
| `queue_deadline_resource_and_closed_link_outcomes_are_typed` | 282 | `DeliveryOutcome` variants |
| `bounded_observations_and_snapshots_are_privacy_safe` | 358 | Ring-buffer + redaction |
| `snapshot_links_are_sorted_by_local_id` | 389 | Deterministic ordering |
| `lifecycle_authentication_is_one_way` | 417 | FSM directionality |
| `duplicate_policy_is_deterministic_and_direction_aware` | 446 | Hash-ordering determinism |
| `manager_duplicate_policy_does_not_mutate_state` | 464 | `duplicate_resolution` is a pure read |

## Distinctive design choices

- **No traits at all** — pushes trait polymorphism downstream to
  `i2pr-transport-ntcp2` and `i2pr-runtime`. This crate is a pure
  data/decision layer.
- **`std::sync::Mutex` interior mutability** — not `tokio::sync::Mutex`.
  The manager is driven synchronously by the runtime service, never
  held across `.await`.
- **RAII resource accounting everywhere** — no way to "forget" a
  grant; `QueueAccounting` uses `Weak<ManagerInner>` so it is safe
  even if the manager is dropped first.
- **Double-lock admission pattern** (`enqueue_on_link`,
  `manager.rs:675`) prevents TOCTOU between the resource governor
  and per-link counters without one giant lock.
- **Process-local atomic IDs** — `LinkId` and `DeliveryId` use
  `AtomicU64` with `fetch_update`. They share `MAX_LINK_ID` as a
  shared ceiling across both ID spaces.
- **Debug redaction is tested** — `PeerId` formatting is asserted to
  not leak the raw hash; `EncodedI2npMessage::Debug` shows length
  only.
- **Privacy-safe snapshots** — no raw hash bytes, no endpoints,
  bounded card 256.

## Cross-references

- [Overview](overview.md)
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — concrete
  transport that satisfies these contracts via `LinkHandle` /
  `BoundNtcp2Listener` in `i2pr-runtime`.
- [i2pr-runtime](i2pr-runtime.md) — drives the manager from
  supervised services.
- [i2pr-testkit](i2pr-testkit.md) — uses synthetic helpers in
  `src/transport.rs` for contract tests.
- Plan-of-record: `plans/031-m3-transport-contracts-and-crate-boundaries.md`.
- Related closure: `plans/031-closure.md`.
