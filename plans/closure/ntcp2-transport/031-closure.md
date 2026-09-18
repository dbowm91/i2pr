# Plan 031 closure: transport contracts and crate boundaries

## Scope and result

Plan 031 is complete as a structural Milestone 3 foundation. The workspace
now contains runtime-neutral transport-manager contracts in `i2pr-transport`
and a Tokio-free, filesystem-free `i2pr-transport-ntcp2` ownership skeleton.
No handshake cryptography, encrypted data frames, sockets, DNS, live address,
NetDB, tunnel, client, capability, or interoperability behavior was added.

## Final crate graph

```text
i2pr-proto <- i2pr-crypto <- i2pr-storage
     ^              ^               ^
     |              |               |
i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon
     ^             ^       ^          ^
     |             |       |          |
     +-------------+-------+  i2pr-transport-ntcp2

i2pr-testkit may depend on transport crates for synthetic tests only.
```

`i2pr-runtime` remains the only production owner of Tokio tasks, timers,
channels, sockets, and wakeable cancellation. The daemon remains the
composition root. The dependency and runtime boundary scripts enforce these
relationships.

## ADR decision summary

ADR 0010 records the final boundaries, explicit action/state driving instead of
async traits, canonical encoded-I2NP ownership, runtime confinement, and the
rejected plugin, raw-`Vec<u8>`, raw-channel/socket, and all-in-runtime
alternatives. The transport-neutral surface is intentionally limited to
immediate NTCP2 and future SSU2 link-management needs.

## Public type inventory

- Transport identity and local ownership: `TransportKind`, `LinkId`,
  `Direction`/`LinkDirection`, `PeerId`, `Deadline`.
- Payload and delivery: `EncodedI2npMessage`, `DeliveryRequest`,
  `QueuedDelivery`, `DeliveryOutcome`, and `LinkDeliveryCapability`.
- Lifecycle and admission: `LinkState`, `LinkCandidate`,
  `CandidateDecision`, `DuplicateResolution`, `RegistrationOutcome`,
  `RegistrationRejection`, `PendingHandshake`, and `TransportManager`.
- Observations and diagnostics: `AddressOrigin`, `AddressFamily`,
  `Reachability`, `ValidationState`, `Confidence`, `TerminationCategory`,
  `ReachabilityObservation`, `LinkSnapshot`, and `TransportSnapshot`.
- Resource ownership: `TransportLimits`, `TransportResources`,
  `TransportLease`, `TransportQueueLease`, and the reused core
  `PendingHandshakes`, `ActiveLinks`, `BufferedBytes`, and
  `CommandQueueItems` classes.

The NTCP2 crate exposes no protocol implementation types in this plan. Its
private modules reserve ownership for address, constants, crypto, handshake,
frame, block, and state-machine work in Plans 032–034.

## Payload ownership decision

The transport boundary carries canonical encoded I2NP bytes in a bounded,
owned, non-cloneable `EncodedI2npMessage`. Empty values and values above
`MAX_I2NP_MESSAGE_BYTES` are rejected at construction before ownership is
accepted. `Debug` reports only the byte count, and `into_bytes` is the explicit
consuming handoff.
Delivery requests borrow bytes only for an owning write operation; queued
requests retain an atomic item/byte lease until drop or handoff.

## Lifecycle and delivery outcomes

| Area | Contract |
| --- | --- |
| Link lifecycle | `Candidate -> Handshaking -> Authenticated`; authenticated links may drain, close, or fail; terminal links cannot authenticate again. |
| Candidate admission | First link, additional link, replacement, duplicate rejection, retain-and-drain, peer/global limit, incomplete authentication, resource denial, and identity mismatch are typed decisions. |
| Delivery acceptance | `Ok(QueuedDelivery)` means the request owns a bounded queue reservation and has been admitted to an authenticated capability. |
| Delivery rejection | `NoActiveLink`, `QueueFull`, `LinkClosedBeforeWrite`, `LinkReplaced`, `ResourceDenied`, `DeadlineElapsed`, `Cancelled`, `PeerIdentityMismatch`, and typed protocol/dial outcomes contain no dynamic remote text. |
| Closure | A close report removes exactly its link owner; a stale report returns `CloseOutcome::Stale` and cannot remove a replacement. |

Duplicate winner policy is represented but not guessed; final policy evidence is
deferred to Plan 035.

## Resource classes and ceilings

No new `ResourceClass` was needed. `TransportResources` reuses
`PendingHandshakes`, `ActiveLinks`, `BufferedBytes`, and
`CommandQueueItems`. The deterministic `for_test` ceilings are pending
handshakes 2, active links 4, buffered bytes 16 KiB, queued messages 4,
links per peer 2, messages per link 2, and bytes per link 8 KiB. The
infrastructure bounds are 1 GiB per general class and 4,096 queue items.
Capacity-one, exact-limit, plus-one, deadline, cancellation, replacement,
drop, and teardown paths are covered by tests.

## Documentation and support state

Updated `README.md`, `AGENTS.md`, `CONTRIBUTING.md`, `docs/architecture.md`,
`docs/security-model.md`, and `docs/protocol-support.md`. The support ledger
remains unchanged because transport contracts are not protocol support
evidence; NTCP2 remains `Not implemented` and no capability is advertised.
No repository-local skill files exist under `.codex` or `.agents`; no skill
update was applicable.

## Validation evidence

The final local validation lane is:

```text
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p i2pr-transport --all-targets
cargo test -p i2pr-transport-ntcp2 --all-targets
cargo test -p i2pr-testkit --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
git diff --check
```

Results: all workspace tests passed (150 tests), the
focused transport suite passed 15 tests, the NTCP2 skeleton suite passed with
0 tests, and the testkit suite passed 19 tests. Formatting, clippy, docs,
dependency/runtime/fixture gates, and the Rust 1.85 check passed. `cargo deny`
reported advisories, bans, and sources as OK with the pre-existing duplicate
`rand_core` 0.6/0.9 lock entries warning. No CI run was available in this
local session.

## Deviations and known limits

- `i2pr-transport` uses the existing protocol `Hash` type and I2NP maximum
  constants so peer references and payload bounds do not create parallel
  identity or wire limits.
- Runtime and daemon manifests now declare the transport contract dependency,
  but no runtime adapter or live service was added.
- Queue delivery is returned as an owned `QueuedDelivery`; a later runtime
  response wrapper can translate that handoff to its one-shot channel without
  exposing a channel in this crate.

## Plan 032 prerequisites

Plan 032 may implement NTCP2 cryptographic transcript wrappers in the reserved
NTCP2 crate modules. It must preserve the runtime-free boundary, use reviewed
cryptographic dependencies and zeroizing secret owners, record exact vector
provenance, and feed only authenticated typed candidates into the transport
manager. It must not expand this closure into sockets, runtime tasks, duplicate
winner policy, or protocol capability claims.
