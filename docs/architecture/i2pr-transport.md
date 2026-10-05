# `i2pr-transport` — Deep Dive

Runtime-neutral, bounded transport contracts: link lifecycle FSM, owned
delivery requests, manager admission with RAII leases, deterministic
NTCP2/SSU2 selection, and conservative reachability policy. No Tokio, no
sockets, no filesystem, no `async fn`. The runtime adapter drives it
synchronously.

Path: `crates/i2pr-transport/`

## Purpose

`i2pr-transport` owns the contracts that any transport owner — NTCP2,
SSU2, or others later — must satisfy, plus the shared per-family network
condition vocabulary:

- `LinkState` lifecycle FSM with explicit, validated transitions.
- Owned `DeliveryRequest` / `QueuedDelivery` values with typed
  `DeliveryOutcome`s, caller-supplied monotonic `Deadline`s, and an
  optional `i2pr_core::CancellationToken`.
- Manager-level admission with double-checked locking, RAII leases
  (`PendingHandshake`, `TransportLease`, `TransportQueueLease`,
  `QueueAccounting`), and a deterministic duplicate-resolution policy.
- Deterministic, pure `select_peer_transport` over validated descriptors
  (Plan 159).
- Conservative router-level reachability with a structural corroboration
  floor of two (Plans 159–160).
- Per-family network status / error / testing code owners and their pure
  derivations (Plan 339, Proposal 170).
- Privacy-safe bounded observations and aggregate snapshots.

It does **not** own a runtime, sockets, timers, a clock, filesystem
access, NetDB state, tunnel state, or client delivery. "No I/O" here
means precisely: the crate never reads a clock (`Instant`/`SystemTime`),
never sleeps, never opens a socket or file, and never spawns. Time is
represented only as `std::time::Duration` values the caller supplies
monotonically to `record_reachability` / `enqueue_delivery` /
`ReachabilityTracker::record` / `TransportManager::snapshot`. Interiors
are guarded by `std::sync::Mutex` and never held across a suspension
point.

## Module layout

Flat — declared at `src/lib.rs:16-26`. No subdirectories. Line counts are
`wc -l` against current source.

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | ---: | --- | --- |
| (root) | `src/lib.rs` | 68 | Crate root; re-exports `i2pr_core` resource types; `type AddressObservation = ReachabilityObservation` (`lib.rs:62`) | `AddressObservation` |
| `types` | `src/types.rs` | 275 | Small bounded vocabulary shared by every contract; process-local ID generators | `LinkId`, `DeliveryId`, `Deadline`, `Confidence`, `TransportKind`, `Direction`, `LinkDirection` (alias), `TerminationCategory`, `AddressOrigin`, `AddressFamily`, `Reachability`, `ValidationState` + `LinkIdError`, `DeadlineError`, `ConfidenceError` |
| `identity` | `src/identity.rs` | 46 | Redacted `i2pr_proto::Hash` peer reference used as a map key | `PeerId` |
| `lifecycle` | `src/lifecycle.rs` | 83 | Finite link lifecycle transitions, no side effects | `LinkState`, `InvalidLinkTransition` |
| `delivery` | `src/delivery.rs` | 210 | Owned delivery requests, queue retention, typed outcomes | `DeliveryRequest`, `QueuedDelivery`, `DeliveryOutcome` |
| `payload` | `src/payload.rs` | 95 | Bounded, non-cloneable owned encoded I2NP message | `EncodedI2npMessage`, `PayloadError` |
| `resource` | `src/resource.rs` | 266 | Transport-shaped use of the shared `i2pr_core` resource governor | `TransportLimits`, `TransportResources`, `TransportLease`, `TransportQueueLease`, `TransportResourceLimitsError` |
| `manager` | `src/manager.rs` | 980 | Synchronous manager decisions, exact ownership accounting, RAII lease guards | `TransportManager`, `LinkCandidate`, `LinkDeliveryCapability`, `PendingHandshake`, `DuplicateLinkPolicy`, `DuplicateResolution`, `CandidateDecision`, `RegistrationOutcome` (alias), `CandidateAdmissionError` (alias), `RegistrationRejection`, `RegistrationError`, `CloseOutcome`, `DialBackoff`, `DialBackoffError`, `ReachabilityRecordOutcome` |
| `snapshot` | `src/snapshot.rs` | 104 | Privacy-safe bounded observations and aggregate snapshot types | `LinkResourceUsage`, `LinkSnapshot`, `ReachabilityObservation`, `TransportSnapshot`, `SnapshotError` |
| `selection` | `src/selection.rs` | 709 | Deterministic NTCP2/SSU2 selection and fallback (Plan 159) | `TransportCandidate`, `ExistingLink`, `SelectionPolicy`, `SelectionOutcome`, `select_peer_transport`, `MAX_SELECTION_CANDIDATES` |
| `reachability` | `src/reachability.rs` | 903 | Conservative router-level reachability state machine and publication input (Plans 159–160) | `ReachabilityState`, `ReachabilitySignal`, `PeerTestOutcomeKind`, `ReachabilityPolicy`, `ReachabilityPolicyError`, `ReachabilityTracker`, `ReachabilitySnapshot`, `corroboration_confidence`, `MIN_CORROBORATION_FLOOR`, `DEFAULT_OBSERVATION_TTL`, `MAX_TRACKED_OBSERVATIONS` |
| `network_status` | `src/network_status.rs` | 581 | Plan 339 / Proposal 170 per-family status, error, and testing code owners and their pure derivations | `NetworkStatusCode`, `NetworkErrorCode`, `FamilyNetworkCondition`, `effective_reachability`, `network_status_code`, `network_error_code`, `network_testing_flag` |
| `tests` | `src/tests.rs` | 488 | `#[cfg(test)]` **in-crate** unit tests, wired at `lib.rs:28-29` | — (private) |
| (integration) | `tests/contracts.rs` | 230 | Real integration-test target, black-box over the public API only | — (private) |

`src/` totals 4,808 lines. There is exactly **one** integration-test file
(`tests/contracts.rs`); `src/tests.rs` is a `#[cfg(test)]` module compiled
into the library, not a second integration target.

## Public surface

### Re-exports from `i2pr-core` (`lib.rs:11-14`)

```rust
pub use i2pr_core::{
    ResourceBudget, ResourceBundle, ResourceClass, ResourceError, ResourceLease, ResourceLimit,
    ResourceRequest, ResourceUsage,
};
```

`i2pr_core::CancellationToken` is also used (`delivery.rs:18,54,63`) but is
referenced by path rather than re-exported.

### `pub use` blocks from `src/lib.rs:31-68`

- `delivery` → `DeliveryOutcome`, `DeliveryRequest`, `QueuedDelivery`
- `identity` → `PeerId`
- `lifecycle` → `InvalidLinkTransition`, `LinkState`
- `manager` → `CandidateAdmissionError`, `CandidateDecision`, `CloseOutcome`,
  `DialBackoff`, `DialBackoffError`, `DuplicateLinkPolicy`,
  `DuplicateResolution`, `LinkCandidate`, `LinkDeliveryCapability`,
  `PendingHandshake`, `ReachabilityRecordOutcome`, `RegistrationError`,
  `RegistrationOutcome`, `RegistrationRejection`, `TransportManager`
- `network_status` → `FamilyNetworkCondition`, `NetworkErrorCode`,
  `NetworkStatusCode`, `effective_reachability`, `network_error_code`,
  `network_status_code`, `network_testing_flag`
- `payload` → `EncodedI2npMessage`, `PayloadError`
- `reachability` → `DEFAULT_OBSERVATION_TTL`, `MAX_TRACKED_OBSERVATIONS`,
  `MIN_CORROBORATION_FLOOR`, `PeerTestOutcomeKind`, `ReachabilityPolicy`,
  `ReachabilityPolicyError`, `ReachabilitySignal`, `ReachabilitySnapshot`,
  `ReachabilityState`, `ReachabilityTracker`, `corroboration_confidence`
- `resource` → `TransportLease`, `TransportLimits`, `TransportQueueLease`,
  `TransportResourceLimitsError`, `TransportResources`
- `selection` → `ExistingLink`, `MAX_SELECTION_CANDIDATES`,
  `SelectionOutcome`, `SelectionPolicy`, `TransportCandidate`,
  `select_peer_transport`
- `snapshot` → `LinkResourceUsage`, `LinkSnapshot`, `ReachabilityObservation`,
  `SnapshotError`, `TransportSnapshot`
- `types` → `AddressFamily`, `AddressOrigin`, `Confidence`, `ConfidenceError`,
  `Deadline`, `DeadlineError`, `DeliveryId`, `Direction`, `LinkDirection`,
  `LinkId`, `LinkIdError`, `MAX_DEADLINE`, `MAX_I2NP_MESSAGE_BYTES`,
  `MAX_LINK_ID`, `MAX_LINK_SNAPSHOT_ENTRIES`,
  `MAX_REACHABILITY_OBSERVATIONS`, `Reachability`, `TerminationCategory`,
  `TransportKind`, `ValidationState`
- crate-root alias → `pub type AddressObservation = ReachabilityObservation;`
  (`lib.rs:62`)

### Constants (all, with real values)

| Constant | Location | Value |
| --- | --- | --- |
| `MAX_LINK_ID` | `types.rs:8` | `0x7fff_ffff_ffff_ffff` |
| `MAX_I2NP_MESSAGE_BYTES` | `types.rs:10-11` | `62_724` (`i2pr_proto::MAX_I2NP_PAYLOAD_SIZE` 62,708 + `STANDARD_HEADER_SIZE` 16) |
| `MAX_DEADLINE` | `types.rs:13` | `604_800` s (7 days) |
| `MAX_LINK_SNAPSHOT_ENTRIES` | `types.rs:15` | `256` |
| `MAX_REACHABILITY_OBSERVATIONS` | `types.rs:17` | `64` |
| `MAX_TRANSPORT_RESOURCE_LIMIT` | `resource.rs:10` | `1 << 30` = `1_073_741_824` |
| `MAX_TRANSPORT_QUEUE_CAPACITY` | `resource.rs:12` | `4_096` |
| `MAX_SELECTION_CANDIDATES` | `selection.rs:39` | `16` |
| `MAX_TRACKED_OBSERVATIONS` | `reachability.rs:57` | `64` (aliases `MAX_REACHABILITY_OBSERVATIONS`) |
| `DEFAULT_OBSERVATION_TTL` | `reachability.rs:60` | `1_800` s (30 min) |
| `MIN_CORROBORATION_FLOOR` | `reachability.rs:66` | `2` |
| `NetworkStatusCode::MIN` / `::MAX` | `network_status.rs:66,68` | `0` / `5` |
| `NetworkErrorCode::MIN` / `::MAX` | `network_status.rs:130,132` | `0` / `5` |

`pub trait` count: **zero** — verified by grep across `src/` and `tests/`.
Every contract is a concrete struct, enum, type alias, or free function.

## Key contracts

### Link lifecycle (`lifecycle.rs`)

State set (7): `Candidate`, `Handshaking`, `Authenticated`, `Draining`,
`Closing`, `Closed`, `Failed` (`lifecycle.rs:7-22`).

- `LinkState::transition()` (`lifecycle.rs:26`) — pure; returns
  `Result<Self, InvalidLinkTransition>`. **Self-transitions are always
  legal** (`self == next`, `lifecycle.rs:27`). The complete legal edge
  set is:

  | From | Legal successors |
  | --- | --- |
  | `Candidate` | `Handshaking`, `Closing`, `Failed` |
  | `Handshaking` | `Authenticated`, `Closing`, `Failed` |
  | `Authenticated` | `Draining`, `Closing`, `Failed` |
  | `Draining` | `Closing`, `Closed`, `Failed` |
  | `Closing` | `Closed` |
  | `Failed` | `Closed` |
  | `Closed` | *(none — terminal)* |

  Authentication is therefore one-way: `Closed`/`Failed` → `Authenticated`
  is rejected, and `Handshaking` → `Authenticated` requires passing
  through `Handshaking` (no `Candidate` → `Authenticated` shortcut).
- `is_authenticated()` (`lifecycle.rs:54`) — true for `Authenticated`,
  `Draining`, `Closing`.
- `is_live()` (`lifecycle.rs:59`) — true for everything except `Closed`
  and `Failed`.
- `InvalidLinkTransition` (`lifecycle.rs:66`) is a struct with public
  `from` / `to` fields, not an enum.

### Delivery (`delivery.rs`)

- `DeliveryRequest` (`delivery.rs:13`) — owned, non-cloneable outbound
  request: `DeliveryId`, `PeerId`, `EncodedI2npMessage`, `Deadline`, and
  `Option<i2pr_core::CancellationToken>`. `new()` mints a process-local
  `DeliveryId`; `with_id()` accepts a caller-supplied one;
  `with_cancellation()` attaches the token. `Debug` prints id,
  `message_len`, deadline, and cancelled flag only — never bytes.
- Deadline enforcement is **caller-supplied and checked, not timer-driven**:
  `Deadline::is_elapsed(now)` / `remaining(now)` (`types.rs:148,153`)
  compare against a monotonic `Duration` the manager is handed, and
  `TransportManager::enqueue_on_link` rejects with
  `DeliveryOutcome::DeadlineElapsed` before any resource admission
  (`manager.rs:721-723`). The crate owns no clock and no timer.
- Cancellation is polled the same way: `DeliveryRequest::is_cancelled()`
  (`delivery.rs:60`) is checked before admission and yields
  `DeliveryOutcome::Cancelled` with no partial resource usage
  (`manager.rs:718-726`).
- `QueuedDelivery` (`delivery.rs:110`) — request retained by a link queue
  holding both the `TransportQueueLease` and a `QueueAccounting` guard.
  RAII: dropping it, or calling `into_request()` (`delivery.rs:155`),
  releases the exact grants.
- `DeliveryOutcome` (`delivery.rs:179`) — 12 variants: `Accepted{link_id}`,
  `NoActiveLink`, `QueueFull`, `LinkClosedBeforeWrite`, `LinkReplaced`,
  `ResourceDenied`, `DeadlineElapsed`, `Cancelled`,
  `ProtocolTerminated{category}`, `PeerIdentityMismatch`, `DialScheduled`,
  `DialAlreadyPending`. Deliberately **not** `impl Error`.

### Admission and locking discipline (`manager.rs`)

- `TransportManager::begin_handshake()` (`manager.rs:487`, alias
  `admit_handshake` at `manager.rs:502`) — admits exactly one
  `ResourceClass::PendingHandshakes` unit and returns a `PendingHandshake`.
- `TransportManager::register_authenticated()` (`manager.rs:510`, alias
  `resolve_candidate` at `manager.rs:520`) — requires
  `LinkState::Authenticated` (else
  `CandidateDecision::RejectIncompleteAuthentication`), rejects a
  duplicate `LinkId` with `RegistrationError::DuplicateLinkId`, then
  applies the supplied `DuplicateResolution`, the per-peer limit, the
  global limit, and finally the `ActiveLinks` lease.
- **Double-checked locking** lives in `enqueue_on_link`
  (`manager.rs:709`): the per-link counters are read under the state
  lock and checked (existence, `Authenticated`, per-link message and byte
  ceilings) at `manager.rs:728-743`; the lock is then **dropped**
  (`manager.rs:744`) before the `TransportResources::admit_queue` bundle
  call, so the shared governor is not held under the state mutex; the
  state lock is re-acquired and the **same three checks are repeated**
  (`manager.rs:760-773`) before `checked_add` on both counters. The
  queue lease is explicitly `drop(queue_lease)`d on every rejection path
  (`manager.rs:761,765,771,777,785`), and the item counter is rolled back
  if the byte addition would overflow (`manager.rs:784`). On overflow of
  either `checked_add` the outcome is `ResourceDenied`, never a wrapped
  value. The guard is released (`manager.rs:790`) before the
  `QueuedDelivery` is constructed.
- `TransportManager::enqueue_delivery()` (`manager.rs:699`) is
  `delivery_capability()` + `enqueue_on_link()`; the separate
  `enqueue_on_link()` entry point exists so a caller holding a
  previously-obtained (possibly stale) `LinkDeliveryCapability` gets the
  typed `LinkReplaced` / `LinkClosedBeforeWrite` re-check rather than a
  silent write to the wrong link.
- `PendingHandshake` (`manager.rs:941`) — RAII handshake lease.
  `.register(manager, candidate, now, duplicate)` (`manager.rs:958`)
  verifies the peer matches (else
  `CandidateDecision::RejectPeerIdentityMismatch`) and consumes the
  lease by delegating to `register_authenticated`; `.release()`
  (`manager.rs:953`) and plain drop both return the `PendingHandshakes`
  unit.
- **What the RAII leases guarantee on drop:**
  - `TransportLease` (`resource.rs:166`) wraps one `i2pr_core::ResourceLease`;
    dropping or `.release()` returns exactly `amount` units of `class`.
    `LinkRecord` holds one per active link (`manager.rs:378`), so closing,
    replacing, or dropping the record releases the `ActiveLinks` unit.
  - `TransportQueueLease` (`resource.rs:250`) wraps one
    `i2pr_core::ResourceBundle` covering `CommandQueueItems + 1` and
    `BufferedBytes + message_bytes` acquired atomically via
    `try_acquire_bundle` (`resource.rs:227-235`).
  - `QueueAccounting` (`manager.rs:396`, `pub(crate)`) holds a
    `Weak<ManagerInner>`, so it is safe even if the manager is dropped
    first; its `Drop` (`manager.rs:412-426`) `saturating_sub`s one message
    unit and `self.bytes` byte units from the link counters, and is a
    no-op if the manager, the lock, or the link is already gone.

### Duplicate resolution (`manager.rs`)

- `DuplicateLinkPolicy` (`manager.rs:44`) — constructed with the local
  router reference supplied by the runtime (this crate owns no identity).
  `decide()` (`manager.rs:55`) is a pure function with no mutation:
  different peers → `AcceptNew`; same direction → `RejectNew`; otherwise
  `outbound_wins = self.local_peer < candidate.peer()` and the winner is
  the side whose direction matches, giving `ReplaceExisting` for the
  winner and `RetainExistingDrainNew` for the loser. It never returns
  `AcceptNew` for a same-peer race, so a concurrent inbound/outbound pair
  deterministically collapses to one peer slot.
- `DuplicateResolution` (`manager.rs:25`) — 4 variants: `AcceptNew`,
  `ReplaceExisting`, `RejectNew`, `RetainExistingDrainNew`.
- `TransportManager::duplicate_resolution()` (`manager.rs:467`) is a
  read-only pre-check: it locks, looks at the peer's lowest link ID, and
  returns `AcceptNew` when no peer slot exists. The authoritative
  decision is applied inside `register_authenticated_inner`
  (`manager.rs:549-591`), which is where `ReplaceExisting` explicitly
  removes and drops the old `LinkRecord` (releasing its lease) before
  admitting the replacement.
- `PendingHandshake::register` also enforces a peer-identity check so a
  lease admitted for peer A cannot register peer B's candidate.

### Privacy-safe snapshots (`snapshot.rs`, `manager.rs`)

- `ReachabilityObservation` (`snapshot.rs:51`) — carries
  `transport`, `origin`, `family`, `reachability`, `observed_at`,
  `validation`, and `Option<Confidence>`. There is **no** endpoint, port,
  address, or raw-hash field; family is the coarse `AddressFamily`
  category only.
- `LinkSnapshot` (`snapshot.rs:26`) — process-local `link_id`, transport,
  direction, lifecycle, `authenticated` bool, queue counters, bounded
  `age` (`now - created_at`, clamped to `MAX_DEADLINE`), optional typed
  `last_termination`, and `LinkResourceUsage`. No peer reference at all.
- `TransportSnapshot` (`snapshot.rs:70`) — `links` (sorted by `LinkId`
  because they come from a `BTreeMap<LinkId, _>`, `manager.rs:872-875`),
  `observations` in insertion order, `resources` in core-class order, and
  `dial_backoff_entries: usize` — a **count**, not peer labels.
- `TransportManager::snapshot(now)` (`manager.rs:866`) caps links at
  `MAX_LINK_SNAPSHOT_ENTRIES = 256` via `.take(...)`; the `SnapshotError::TooManyLinks`
  variant exists for callers that build snapshots themselves.
- `PeerId` (`identity.rs:13`) `Debug` prints `PeerId(..)` and `Display`
  prints `peer`; `hash()` (`identity.rs:26`) is a deliberate typed accessor
  for map identity / protocol binding and is documented as never valid as
  a log label. `EncodedI2npMessage` `Debug` shows `len` only
  (`payload.rs:54`).
- `ReachabilityTracker::snapshot()` and `as_transport_observation()`
  (`reachability.rs:390,418`) hand out `Copy` publication inputs only —
  never packet or session objects.

### Resource accounting (`resource.rs`)

- `TransportResources::new()` (`resource.rs:198`) builds one
  `i2pr_core::ResourceBudget` with exactly four
  `i2pr_core::ResourceClass` values: `PendingHandshakes`, `ActiveLinks`,
  `BufferedBytes`, `CommandQueueItems`. No other `ResourceClass` variant
  is used by this crate.
- `TransportLimits` (`resource.rs:20`) — 7 ceilings validated in
  `const fn new()` (`resource.rs:40`): `max_pending_handshakes`,
  `max_active_links`, `max_buffered_bytes`, `max_queued_messages`,
  `max_links_per_peer`, `max_messages_per_link`, `max_bytes_per_link`.
  Mapping: the first four become the four `ResourceLimit`s above; the
  three scoped ceilings are enforced in manager code rather than in the
  budget — `max_links_per_peer` in `register_authenticated_inner`
  (`manager.rs:562`) and `peer_at_link_limit` (`manager.rs:695`),
  `max_messages_per_link` / `max_bytes_per_link` in the double-checked
  `enqueue_on_link` (`manager.rs:739-740,768-769`).
  Validation rules: zero → `Zero{class}`; above
  `MAX_TRANSPORT_RESOURCE_LIMIT` → `TooLarge`, except the two queue-item
  ceilings (`max_queued_messages`, `max_messages_per_link`) which are
  capped at `MAX_TRANSPORT_QUEUE_CAPACITY = 4_096` (`resource.rs:64-75`);
  then three scoped-vs-global cross-checks
  (`max_links_per_peer ≤ max_active_links`, `max_messages_per_link ≤
  max_queued_messages`, `max_bytes_per_link ≤ max_buffered_bytes`) →
  `ScopedExceedsGlobal{scoped, global}`.
- `for_test()` (`resource.rs:108`) is the deterministic small ceiling set
  used by the contract tests: `2 / 4 / 16384 / 4 / 2 / 2 / 8192`.

### Deterministic selection (`selection.rs`, Plan 159)

Inputs to the pure function `select_peer_transport` (`selection.rs:249`):
`existing: &[ExistingLink]`, `candidates: &[TransportCandidate]`,
`failed_tags: &[u64]`, `backed_off: &[TransportKind]`,
`policy: SelectionPolicy`, `peer_link_limit_reached: bool`.

Order of evaluation:

1. `candidates.len() > MAX_SELECTION_CANDIDATES` → `ResourceDenied`
   (`selection.rs:257`) — over-bound input is denied, never truncated.
2. Reuse: `select_reuse` (`selection.rs:327`) also denies over-bound
   `existing`; otherwise the winner is the lowest
   `transport_rank`, then the lowest `link().value()`.
3. `peer_link_limit_reached` → `ResourceDenied` (reuse still wins, so a
   limit never forces a redial of an already-usable link).
4. Filter (`selection.rs:268-289`), dropping: `!valid()`,
   `family() == AddressFamily::Unknown`, transports disabled by
   `policy.transport_enabled`, tags present in `failed_tags`,
   introducer-only candidates when `!policy.allow_introducer_only()`,
   and backed-off transports (which also set `saw_backed_off`).
5. Empty survivor set → `BackedOff` if anything was excluded for backoff,
   else `NoCompatibleAddress` (`selection.rs:290-295`).
6. **Tie-break** (`selection.rs:296-307`): direct before introducer-only,
   then `transport_rank` (policy `prefer_ssu2` decides whether SSU2 is
   rank 0 or 1, `selection.rs:347-353`), then lowest `tag`, then
   `AddressFamily` order.
7. Output: `DialFallback { primary, secondary }` when a survivor on a
   *different* transport exists (`selection.rs:309-312`) — primary and
   secondary always name different transports — otherwise `Dial`.

Determinism guarantee: no clock, no RNG, no iteration over a `HashMap`;
`SelectionPolicy` and `Default` are pure value construction
(`selection.rs:184-193`, default = both enabled, prefer SSU2,
introducer-only disallowed). The same inputs always yield the same
outcome. `Address-specific failures never poison a whole transport` —
only the exact `failed_tags` are excluded.

### Reachability policy (`reachability.rs`, Plans 159–160)

- `ReachabilityState` (`reachability.rs:70`, 6 states): `Unknown` (default),
  `ObservedUnconfirmed`, `CandidateReachable`, `Reachable`, `Firewalled`,
  `Unreachable`.
- `ReachabilitySignal` (`reachability.rs:119`, 5 variants) — family-only,
  no literal endpoint: `LocalConfiguredBind{family}`,
  `AuthenticatedPeerObservedExternalAddress{family}`,
  `ValidatedPath{family}`,
  `PeerTestResult{family, outcome: PeerTestOutcomeKind}`,
  `RelayFirewalledSignal{family}`.
- `PeerTestOutcomeKind` (`reachability.rs:99`, 5 variants): `Confirmed`,
  `AddressMismatch`, `FirewalledLikely`, `Inconclusive`, `Rejected`.
  `supports_reachability` (`reachability.rs:163`) is true only for
  `Confirmed`; `contradicts_reachability` (`reachability.rs:180`) is true
  only for `AddressMismatch` / `FirewalledLikely` — `Inconclusive` and
  `Rejected` are **neutral**, so an inconclusive test never flips state
  arbitrarily. Any `RelayFirewalledSignal` contradicts direct
  reachability.
- **Corroboration is counted per signal *class*, not per signal**
  (`corroboration_class()` at `reachability.rs:195` assigns 0..4), so one
  peer's repeated external-address observation can never reach
  `Reachable`. The floor is structural: `ReachabilityPolicy::validate()`
  (`reachability.rs:249`) rejects `min_corroboration < MIN_CORROBORATION_FLOOR`
  (= 2) and `> MAX_TRACKED_OBSERVATIONS` (= 64), and a zero or
  `> MAX_DEADLINE` TTL.
- `supporting_classes()` (`reachability.rs:475`) counts only signals of
  the **latest** family and skips `LocalConfiguredBind` unless
  `policy.configured_direct_allowed` — explicit configuration, never
  inference.
- `recompute()` (`reachability.rs:528`): empty → `Unknown`; contradicting
  classes ≥ floor → `Firewalled` (when relay evidence is present or
  firewalled classes reach the floor) else `Unreachable`; any single
  contradiction → `ObservedUnconfirmed`; supporting > floor →
  `Reachable`; supporting ≥ floor → `CandidateReachable`; otherwise
  `ObservedUnconfirmed`.
- Expiry: `record()` and `poll_expiry()` drop signals older than
  `policy.observation_ttl` (`expire_locked`, `reachability.rs:461`) and
  downgrade the state; the observation ring evicts oldest-first at
  `MAX_TRACKED_OBSERVATIONS` (`reachability.rs:369`). `snapshot()`
  (`reachability.rs:390`) reports `expires_at` as the **earliest**
  supporting-evidence expiry so direct addresses withdraw when evidence
  lapses.
- `corroboration_confidence(corroboration)` (`reachability.rs:580`) maps
  depth to a bounded score: `0 | 1 → None`, `2 → 50`, `≥3 → 75`.
- `as_transport_observation()` (`reachability.rs:418`) maps the state
  into the `snapshot` vocabulary for
  `TransportManager::record_reachability`, and sets
  `AddressOrigin::Configured` only when a `LocalConfiguredBind` signal is
  present.

### `network_status.rs` and `payload.rs`

- `network_status.rs` (Plan 339, Proposal 170) owns the **per-family
  condition owners and their pure derivations** for five canonical
  RouterInfo selectors, with the wire vocabulary pinned to i2pd
  `RouterContext.h:44-72` (read as a reference; nothing vendored):
  - `NetworkStatusCode` (`network_status.rs:49`): `Ok`=0,
    `Firewalled`=1, `Unknown`=2, `Proxy`=3, `Mesh`=4, `Stan`=5, with
    `as_i64`, `name`, and `try_from_i64` that **rejects** out-of-range
    values rather than clamping.
  - `NetworkErrorCode` (`network_status.rs:113`): `None`=0,
    `ClockSkew`=1, `Offline`=2, `SymmetricNat`=3, `FullConeNat`=4,
    `NoDescriptors`=5. `ClockSkew`, `SymmetricNat`, and `FullConeNat` are
    never produced — i2pr owns no such detector; the variants exist only
    so a decoded value round-trips.
  - `FamilyNetworkCondition` (`network_status.rs:177`): `configured`,
    `bound`, `reachability` — booleans and an enum only, plus
    `inert()`.
  - `effective_reachability(snapshot, family, now)` (`network_status.rs:210`):
    honours a snapshot only when it was recorded **for that family** and
    `expires_at > now`; otherwise `Unknown`. A snapshot qualified for the
    other family never leaks its claim across families.
  - `network_status_code` (`network_status.rs:226`): only `Reachable`
    → `Ok`; `Firewalled | Unreachable` → `Firewalled`; all three
    unconfirmed states → `Unknown`.
  - `network_testing_flag` (`network_status.rs:244`): `1` only for
    `ObservedUnconfirmed | CandidateReachable`; `0` otherwise, so a
    router that never observed anything does not claim to be testing.
  - `network_error_code` (`network_status.rs:260`): `NoDescriptors` when
    the caller attests no usable NetDB peer, else `Offline` when the
    family is configured but unbound, else `None`.
  - The honest production baseline is therefore
    `status = Unknown (2)`, `error = None (0)`, `testing = 0`.
- `payload.rs` owns exactly one value: `EncodedI2npMessage` — an owned,
  non-cloneable `Vec<u8>` bounded at construction by
  `MAX_I2NP_MESSAGE_BYTES` and rejected when empty (`payload.rs:20-31`).
  It is never decoded or re-encoded here, so the caller's
  canonical/authenticated representation survives the boundary; the
  owner moves onward with `into_bytes()`.

### Typed error enums — every variant

| Error | Location | Class | Variants |
| --- | --- | --- | --- |
| `LinkIdError` | `types.rs:86` | Protocol | `Zero`, `TooLarge`, `Exhausted` |
| `DeadlineError` | `types.rs:160` | Protocol | `TooFar` (only) |
| `ConfidenceError` | `types.rs:239` | Protocol | `TooLarge` (only) |
| `PayloadError` | `payload.rs:65` | Protocol | `Empty`, `TooLarge{actual, maximum}` |
| `InvalidLinkTransition` | `lifecycle.rs:66` | Protocol | struct `{ from, to }` |
| `TransportResourceLimitsError` | `resource.rs:123` | Configuration | `Zero{class}`, `TooLarge{class, maximum}`, `ScopedExceedsGlobal{scoped, global}` |
| `ReachabilityPolicyError` | `reachability.rs:275` | Configuration | `CorroborationTooLow`, `CorroborationTooHigh`, `InvalidTtl` |
| `RegistrationError` | `manager.rs:79` | Operational | `StatePoisoned`, `Resource(ResourceError)`, `MissingLink`, `DuplicateLinkId`, `InvalidTransition(InvalidLinkTransition)` |
| `DialBackoffError` | `manager.rs:234` | Operational | `TooFar` (only) |
| `SnapshotError` | `snapshot.rs:83` | Operational | `Resource(ResourceError)`, `TooManyLinks{maximum}` |
| `DeliveryOutcome` | `delivery.rs:179` | Ad-hoc typed result | 12 variants, **not** `impl Error` (see above) |

Supporting value enums (not errors): `DuplicateResolution`
(`manager.rs:25`, 4), `CandidateDecision` (`manager.rs:125`, 10),
`RegistrationRejection` (`manager.rs:178`, 6), `CloseOutcome`
(`manager.rs:195`, 2), `ReachabilityRecordOutcome` (`manager.rs:249`, 2),
`SelectionOutcome` (`selection.rs:197`, 6), `TerminationCategory`
(`types.rs:254`, 10: `LocalShutdown`, `RemoteTermination`,
`AuthenticationFailure`, `Timeout`, `ReplayOrSkewRejection`,
`MalformedFraming`, `QueueExhaustion`, `ResourceExhaustion`,
`DuplicateReplacement`, `IoClosure` — a fixed, closed vocabulary safe for
aggregate diagnostics).

`CandidateAdmissionError` (`manager.rs:121`) and `RegistrationOutcome`
(`manager.rs:174`) are `type` aliases for `RegistrationError` and
`CandidateDecision` respectively, kept so callers can name the handshake
and registration phases explicitly.

## Dependencies

`Cargo.toml:10-12` — the entire dependency list, no `[dev-dependencies]`:

```toml
[dependencies]
i2pr-core   = { path = "../i2pr-core" }
i2pr-proto  = { path = "../i2pr-proto" }
```

No third-party dependencies at all: this is a `std`-only crate.

- `i2pr-core` supplies the resource governor (`ResourceBudget`,
  `ResourceBundle`, `ResourceClass`, `ResourceError`, `ResourceLease`,
  `ResourceLimit`, `ResourceRequest`, `ResourceUsage`) and
  `CancellationToken`. It is runtime-neutral itself, so importing it does
  not cross the boundary.
- `i2pr-proto` supplies only `Hash` (`identity.rs:5`) and the two I2NP
  size constants used to derive `MAX_I2NP_MESSAGE_BYTES`
  (`types.rs:10-11`). Wire codecs stay in `i2pr-proto`; this crate never
  decodes a message.

Position: `i2pr-proto ← i2pr-core ← i2pr-transport ← {i2pr-transport-ntcp2,
i2pr-transport-ssu2, i2pr-runtime, i2pr-testkit}`.
`scripts/check-dependency-direction.sh:17` allowlists exactly
`{"i2pr-core", "i2pr-proto"}` for `i2pr-*` names; verified passing
(`bash scripts/check-dependency-direction.sh` → `dependency direction: ok`).

`#![forbid(unsafe_code)]` is present at `lib.rs:8`, joined by
`#![warn(missing_docs)]` at `lib.rs:9`.

### Runtime-neutrality — verified, not assumed

`scripts/check-runtime-boundaries.sh` greps this crate for
`tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|OpenOptions|File::`
(`:44-48`) and for `async[[:space:]]+fn|async_trait|i2pr-(netdb|tunnel|client)`
across the whole crate directory including tests (`:93-97`), plus
`i2pr-daemon|i2pr-runtime|i2pr-testkit` in the manifest (`:99-103`).
Verified by running the script (`runtime boundary checks passed`) and by
independent grep: the only matches for the word "async" anywhere in the
crate are two comments (`delivery.rs:12`, `selection.rs:30`). There is
**no** `async fn`, no `async_trait`, no `tokio` reference, no
`std::net`/`std::fs`, no `Instant`/`SystemTime`, no thread sleep, no
channel, and no `spawn`.

## Tests

57 tests total, all synchronous, all with explicit bounded inputs
(`Duration::ZERO` or a fixed `Duration`; no wall-clock waits).

### In-crate unit tests

`src/tests.rs` (11 tests, `#[cfg(test)] mod tests;` at `lib.rs:28-29`),
default manager limits `limits(1, 2, 1, 1, 64, 1, 64)` — i.e. capacity 1
queues and bytes per link:

| Test | Line | What it proves |
| --- | --- | --- |
| `payload_bounds_and_diagnostics_are_safe` | 74 | `LinkId` zero / `MAX_LINK_ID` / `MAX_LINK_ID + 1`; empty and `MAX_I2NP_MESSAGE_BYTES + 1` payloads rejected; `Debug` output contains neither payload bytes nor the peer byte |
| `first_link_limits_and_duplicate_decisions_are_typed` | 88 | `AcceptFirst`, `RejectNewDuplicate`, `RetainExistingDrainNew`, and replace all yield typed `CandidateDecision`s |
| `active_limit_and_stale_close_preserve_replacements` | 143 | Global `ActiveLinks` limit rejects the next candidate; a stale close of a replaced link returns `CloseOutcome::Stale` and does not release the replacement's lease |
| `pending_handshake_lease_releases_on_drop_and_completion` | 193 | `PendingHandshakes` usage returns to 0 on drop **and** on `.register()` |
| `queue_item_and_byte_leases_release_on_drop_and_handoff` | 236 | `CommandQueueItems` + `BufferedBytes` release on `QueuedDelivery` drop and on `into_request()` handoff |
| `queue_deadline_resource_and_closed_link_outcomes_are_typed` | 282 | `DeadlineElapsed`, `Cancelled`, `QueueFull`, `ResourceDenied`, `LinkClosedBeforeWrite` typed outcomes |
| `bounded_observations_and_snapshots_are_privacy_safe` | 358 | Ring buffer caps at `MAX_REACHABILITY_OBSERVATIONS`; snapshot exposes no hash bytes or endpoints |
| `snapshot_links_are_sorted_by_local_id` | 389 | Deterministic `LinkId`-ordered `links` |
| `lifecycle_authentication_is_one_way` | 417 | FSM directionality, no re-authentication after a terminal state |
| `duplicate_policy_is_deterministic_and_direction_aware` | 446 | Hash-ordering determinism and direction awareness |
| `manager_duplicate_policy_does_not_mutate_state` | 464 | `duplicate_resolution` is a pure read |

`src/selection.rs:355-709` — 15 tests: SSU2 and NTCP2 reuse
precedence, reuse tie-break by policy transport then lowest link ID,
backoff fallback, per-address failure isolation, ordered dual-transport
`DialFallback`, single-transport `Dial`, disabled-transport exclusion,
all-backed-off → `BackedOff` (not `NoCompatibleAddress`), peer limit
denying dials while keeping reuse, invalid/unknown/introducer-only
skipping, introducer-only dialing when allowed, input determinism, and
over-bound input denied rather than truncated.

`src/reachability.rs:594-903` — 12 tests: policy rejects
single-observation corroboration, one peer observation never reaches
`Reachable`, corroborated path + peer observation → `CandidateReachable`,
configured binding gated on `configured_direct_allowed`, a third class
promotes to `Reachable`, contradiction keeps `ObservedUnconfirmed`,
inconclusive never flips state, corroborated failures →
`Unreachable`, expiry withdraws support, snapshot expiry uses the
earliest supporting evidence, `Unknown`-family signals ignored, and the
transport-observation mapping staying redacted.

`src/network_status.rs:273-581` — 13 tests: exact round-trip of both
adopted enumerations, out-of-range wire values rejected rather than
clamped, the enumeration matching the pinned i2pd definitions, effective
reachability never inheriting another family, stale/absent snapshots
rejected, the status table covering every reachability state, status
never leaving `Unknown` without corroborated evidence, `Proxy`/`Mesh`/
`Stan` never derived, the testing flag `1` only while a determination is
pending, an untested router not reporting itself as testing, `Offline`
only when configured-and-unbound, an empty NetDB reporting
`NoDescriptors` on both families, and undetectable error codes never
derived.

### Integration test target

`tests/contracts.rs` — 6 tests, black-box over the public re-exports only
(no private `manager`/`resource` internals): `payload_bounds_and_debug_are_strict`,
`lifecycle_rejects_reauthentication_from_terminal_state`,
`first_duplicate_replace_and_stale_close_are_typed` (with an explicit
`ActiveLinks` usage assertion of 1 after a stale close),
`queue_and_pending_handshake_leases_release_exactly` (capacity-2 pending
handshakes: two admitted, third denied, then both drop back to 0; queue
bytes 8 → 0 on drop),
`cancelled_delivery_is_reported_without_resource_admission` (a
pre-cancelled `i2pr_core::CancellationToken` yields `Cancelled` with
`BufferedBytes` usage 0), and
`capacity_one_rejects_second_link_without_partial_usage`
(`TransportLimits::new(1, 1, 1024, 1, 1, 1, 1024)` → second link rejected
with `RejectGlobalLimit` and `ActiveLinks` usage still exactly 1).

### Resource / queue discipline

Capacity 1, exact load, and max+1 are all covered: the default unit-test
limits pin `max_messages_per_link = 1` and `max_bytes_per_link = 64`, so
exact-load and over-capacity paths both execute; the integration suite adds
a global capacity-1 link case. Lease release is asserted on every drop
path the crate has — pending-handshake drop and register, queue drop and
handoff, stale close, and link replacement.

## Distinctive design choices

- **Zero traits** — every contract is a concrete struct, enum, alias, or
  free function, pushing trait polymorphism down to
  `i2pr-transport-ntcp2` / `i2pr-transport-ssu2` and the runtime.
- **`std::sync::Mutex`, never held across a suspension point** — the
  manager is driven synchronously by an owning runtime service, and the
  state lock is deliberately dropped around resource-governor calls.
- **Double-checked admission** in `enqueue_on_link` closes the TOCTOU
  window between the shared governor and the per-link counters without one
  giant lock, and re-drops the queue lease on every rejection path.
- **RAII everywhere, `Weak` where it matters** — `QueueAccounting` holds a
  `Weak<ManagerInner>`, so a lease outliving its manager degrades to a
  no-op instead of resurrecting dead state.
- **The caller owns the clock** — `Duration` values are passed in for
  `now`; the crate never reads time, sleeps, or spawns, which is what
  makes every contract test deterministic.
- **Corroboration is counted per signal class, not per signal** — one
  peer's repeated external-address observation can never reach
  `Reachable`, and the floor of two is validation-enforced.
- **Relay success is firewalled-class** — a working introduction proves
  the requester needs help, never that it is directly reachable (Plan 160).
- **Selection without protocol imports** — the crate sits below both
  transport implementations, so `select_peer_transport` consumes validated
  descriptors, never `RouterAddress` types.
- **Redaction is enforced by construction and then tested** —
  `ReachabilitySignal` has no endpoint variant, `LinkSnapshot` has no peer
  field, and `Debug`/`Display` redaction of `PeerId` and
  `EncodedI2npMessage` is asserted against actual formatting output.
- **Denied, not truncated** — over-bound selection input and out-of-range
  network-status wire values both produce a typed refusal instead of a
  silent subset or a clamp.

## Cross-references

- [Architecture overview](overview.md)
- [Dependency graph](dependency-graph.md) · [Tooling](tooling.md)
- [`i2pr-core`](i2pr-core.md) — `ResourceBudget`, `ResourceClass`,
  `CancellationToken` owner.
- [`i2pr-proto`](i2pr-proto.md) — `Hash` and the I2NP size constants
  consumed at the transport boundary.
- [`i2pr-transport-ntcp2`](i2pr-transport-ntcp2.md) · [`i2pr-transport-ssu2`](i2pr-transport-ssu2.md)
  — the two transport owners that sit above these contracts.
- [`i2pr-runtime`](i2pr-runtime.md) — drives the manager and owns all
  sockets, timers, and task cancellation.
- [`i2pr-testkit`](i2pr-testkit.md) — synthetic helpers in
  `crates/i2pr-testkit/src/transport.rs` build on these contracts.
- ADR [0010 — Transport contracts and crate boundaries](../adr/0010-transport-contracts-and-crate-boundaries.md)
  (Accepted) — plan-of-record rationale for this crate's boundary.
- ADR [0002 — Tokio at runtime-facing boundaries](../adr/0002-tokio-runtime-boundary.md)
  (Accepted) — the runtime-neutrality rule this crate satisfies.
- Plan 031 — [`plans/implementation/ntcp2-transport/031-m3-transport-contracts-and-crate-boundaries.md`](../../plans/implementation/ntcp2-transport/031-m3-transport-contracts-and-crate-boundaries.md),
  closure [`plans/closure/ntcp2-transport/031-closure.md`](../../plans/closure/ntcp2-transport/031-closure.md).
- Plan 159 — [`plans/implementation/ssu2/159-m8-ssu2-path-validation-publication-and-transport-selection.md`](../../plans/implementation/ssu2/159-m8-ssu2-path-validation-publication-and-transport-selection.md),
  status [`plans/closure/ssu2/159-status.md`](../../plans/closure/ssu2/159-status.md) (`passed-m8-ssu2-path-validation-publication-and-transport-selection`) — selection and reachability policy.
- Plan 160 — [`plans/implementation/ssu2/160-m8-ssu2-peer-test-and-relay-reachability.md`](../../plans/implementation/ssu2/160-m8-ssu2-peer-test-and-relay-reachability.md),
  status [`plans/closure/ssu2/160-status.md`](../../plans/closure/ssu2/160-status.md) (`passed-m8-ssu2-peer-test-and-relay-reachability`) — typed peer-test / relay outcomes.
- Plan 339 — [`plans/implementation/i2pcontrol-proposal-170/339-per-family-network-status-error-and-testing-owners.md`](../../plans/implementation/i2pcontrol-proposal-170/339-per-family-network-status-error-and-testing-owners.md),
  status [`plans/closure/i2pcontrol-proposal-170/339-status.md`](../../plans/closure/i2pcontrol-proposal-170/339-status.md) (`passed-per-family-network-condition-owners-with-pinned-i2pd-enumeration`) — `network_status.rs`.
