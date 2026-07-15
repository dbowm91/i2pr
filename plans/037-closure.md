# Plan 037 corrective integration closure

## Status

**Blocked — local corrective integration is recorded; Milestone 3 is not
closed.**

This record documents the completed bounded corrections that can be expressed
under the current crate boundaries and the gates that remain unavailable. It
does not convert loopback tests, self-handshake code, vectors, or the checked-in
private-testnet manifest into Java I2P or i2pd evidence.

## Objective and disposition

Plan 037 was opened to correct ownership, deadline, queue-accounting, parser,
backoff, observability, and runtime-composition defects. Tracks A, C, D, E, G,
and H have local implementation or test coverage as described below. Track B
has a runtime-owned active-link lease path, but it is not connected to the
synchronous `TransportManager`. Track F remains incomplete: the workspace has
no production socket-to-NTCP2 state-machine adapter. Track I remains blocked:
the authorized Java I2P/i2pd private testnet has not been run and no sanitized
scenario artifacts, hashes, or run identifiers exist.

The dependency-direction guard is intentional: `i2pr-runtime` may consume
`i2pr-core` and `i2pr-transport`, while `i2pr-transport-ntcp2` consumes the
protocol/crypto/transport layers. Adding a direct runtime dependency on the
NTCP2 state-machine crate to force Track F would violate the repository's hard
boundary. A future composition plan must add a narrowly scoped adapter boundary
or place the composition in an approved root while preserving runtime socket
ownership.

## Implementation commits

- `690c895` — `feat: correct NTCP2 runtime ownership seams` (implementation
  and documentation change set).
- The closure record itself is committed separately after this SHA.

## Exact changed files

The implementation commit changed:

```text
AGENTS.md
CONTRIBUTING.md
README.md
crates/i2pr-runtime/src/lib.rs
crates/i2pr-runtime/src/ntcp2_runtime.rs
crates/i2pr-transport-ntcp2/src/block.rs
docs/adr/0013-ntcp2-data-phase-and-blocks.md
docs/adr/0014-ntcp2-runtime-link-manager-and-address-policy.md
docs/architecture.md
docs/architecture/i2pr-runtime.md
docs/architecture/i2pr-transport-ntcp2.md
docs/private-testnet.md
docs/protocol-support.md
docs/security-model.md
scripts/check-ntcp2-interoperability.sh
specs/CONFORMANCE.md
specs/protocols/03-ntcp2.md
specs/support.toml
```

This file, `plans/037-closure.md`, is the separate closure record.

## Corrected ownership diagrams

Inbound pending ownership:

```text
accept -> InboundPermit -> InboundChunk
       -> AdmittedInboundStream { stream + permit }
       -> handshake owner -> authenticated-link registration or typed failure
       -> permit drop/transition at the terminal boundary
```

Runtime link ownership:

```text
start_link -> ActiveLinkPermit + LinkHandle
           -> ChildScope reader/writer children
           -> QueuedFrame { bytes + item/byte release owner }
           -> write, cancellation, failure, receiver close, or scope teardown
           -> queue counters zero; LinkHandle drop releases active lease
```

The diagrams describe the corrected runtime seams. The missing handshake and
manager-registration arrows remain an explicit composition gap.

## Inbound permit lifetime

| Boundary | Owner and required result |
| --- | --- |
| Accept admission | `InboundAdmission::admit` returns one non-cloneable `InboundPermit`. |
| Listener handoff | `InboundChunk` contains the permit; cancellation-aware bounded send drops the whole chunk on failure. |
| Handshake handoff | `InboundChunk::into_stream` returns `AdmittedInboundStream`, which retains the permit. |
| Success | A future adapter must atomically replace pending ownership with its manager/active-link lease; that adapter is not present yet. |
| Failure, timeout, disconnect, cancellation | Dropping the wrapper releases global, exact-IP, and subnet counters once. |
| Runtime teardown | Child-scope cancellation drops queued chunks/wrappers and releases the permit. |

The local tests verify the handoff lifetime and repeated RAII release. They do
not prove the absent pending-to-manager atomic transition.

## Active-link and queue lease inventory

| Resource | Owner | Release boundary | Local evidence |
| --- | --- | --- | --- |
| Active link | `ActiveLinkPermit` retained by an admitted `LinkHandle` | Handle drop or failed child creation | Exact capacity/plus-one and 100-iteration teardown tests |
| Queue item | `QueuedFrame` | Write success/failure, send cancellation/timeout, receiver closure, or child teardown | Exact counter and underflow assertions |
| Queue bytes | Same `QueuedFrame` | Same as queue item | Exact counter and underflow assertions |
| Transport-manager active lease | `TransportLease` in `TransportManager::LinkRecord` | Manager close/removal | Existing synchronous manager tests; not composed with runtime link creation |
| Replay entry | `ReplayCache` | Bounded retention expiry | Existing deterministic replay tests; no handshake driver calls it end to end |

## Deadline and cancellation coverage

| I/O or wait stage | Current bounded path | Disposition |
| --- | --- | --- |
| TCP connect | `Ntcp2RuntimeService::dial_with_key` and configured connect deadline | Implemented locally |
| Handshake total deadline | No composed driver owns one absolute deadline | Blocked with Track F |
| Exact handshake field reads/writes | `read_exact`/`write_all_exact` helpers accept cancellation and an absolute deadline | Helper implemented; not composed |
| Data-frame length/ciphertext reads | No composed frame driver | Blocked with Track F |
| Link reader idle read | `read_once_bounded` with configured `read_idle` | Implemented in supervised child |
| Link writer | `write_all_exact` with a fresh configured write deadline | Implemented in supervised child |
| Queue admission | `send_with_deadlines` uses configured `queue_wait`; cancellation wins the select | Implemented locally |
| Duplicate drain/orderly termination | No manager/adapter driver | Blocked with Track F |

## General data-phase block conformance

The pinned NTCP2 specification is recorded in `specs/SOURCES.md` and
`specs/protocols/03-ntcp2.md`. The local policy now keeps general data-phase
parsing separate from the strict SessionConfirmed part-two parser.

| Sequence or rule | General data phase | SessionConfirmed part two |
| --- | --- | --- |
| Repeated Timestamp/Options/RouterInfo/I2NP/unknown non-padding blocks | Accepted when each block is bounded and authenticated | Not applicable to this parser |
| Termination | At most once and last non-padding block | Not a general data block |
| Padding | At most once and final | Strict handshake payload rules remain unchanged |
| Block after Termination | Only permitted final Padding | Strict handshake decoder rejects structural violations |
| Duplicate Padding or Termination | Rejected | Strict handshake decoder remains separate |

Exact-byte positive and malformed tests cover repeated non-padding blocks,
late Termination, final Padding, invalid post-Termination blocks, duplicate
Termination, duplicate Padding, and non-final Padding. No Java I2P- or i2pd-
produced block sequence is available in this checkout.

## Public API changes

- `AdmittedInboundStream` preserves an inbound permit through handshake work.
- `ActiveLinkAdmission`, `ActiveLinkPermit`, `ActiveLinkSnapshot`, and
  `LinkStartError` expose bounded active-link ownership for service-created
  links; `Ntcp2RuntimeService::start_link` uses it.
- `LinkHandle::start_with_deadlines` and `send_with_deadlines` connect the
  configured deadline policy to actual child I/O and queue waits.
- `LinkSnapshot` exposes a privacy-safe queue-release-underflow counter.
- `DialAttempt::mark_authenticated` clears backoff only after a complete
  handshake caller explicitly marks success; TCP connect no longer clears it.
- `Ntcp2EventKind` includes fixed categories for handshake, replay, skew,
  identity, frame, queue, deadline, termination, and cleanup outcomes.

## Secret-owner changes

None. Plan 037 does not change cryptographic secret ownership, NTCP2 static
key/IV storage, transcript ownership, or zeroization contracts. The runtime
types added here redact socket/permit internals and do not expose protocol
payloads or key material.

## Validation and evidence

The following commands were run for this change set. `rtk` is the repository's
output-filtering command wrapper.

```text
rtk cargo fmt --all --check
rtk cargo check -p i2pr-runtime --all-targets
rtk cargo test -p i2pr-runtime --all-targets
rtk cargo test -p i2pr-transport-ntcp2 --all-targets
rtk cargo check --workspace --all-targets
rtk cargo test --workspace
rtk bash scripts/check-dependency-direction.sh
rtk bash scripts/check-runtime-boundaries.sh
rtk bash scripts/check-fixture-manifest.sh
rtk bash scripts/check-ntcp2-vectors.sh
rtk bash scripts/check-ntcp2-interoperability.sh
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" rtk cargo doc --workspace --no-deps
rtk cargo deny check advisories bans sources
rtk cargo +1.85.0 check --workspace --all-targets
rtk cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
CARGO_NET_OFFLINE=true rtk bash scripts/fuzz-smoke.sh
rtk git diff --check
```

Results:

- `cargo fmt --all --check`: pass.
- `cargo check --workspace` and `cargo check --workspace --all-targets`: pass.
- `cargo test --workspace`: 205 passed across 26 suites.
- Focused tests: core 14, runtime 39, transport 17, NTCP2 34, testkit 25;
  the forced-child-cleanup serial test also passed (1 selected, 38 filtered).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass.
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`: pass.
- Dependency direction, runtime boundaries, fixture manifest, NTCP2 vectors,
  and the sanitized eight-scenario interoperability preflight: pass.
- `cargo deny check advisories bans sources`: pass with the existing duplicate
  `rand_core` 0.6/0.9 warning; advisories, bans, and sources passed.
- `cargo +1.85.0 check --workspace --all-targets`: pass.
- `cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets`
  and `CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh`: pass.
- `git diff --check`: pass.

The local deterministic repetition tests cover 100 active-link and queue
teardown iterations. No wall-clock sleeps, public-network sockets, private
keys, payload captures, or raw endpoint artifacts were retained. The fuzz
smoke run generated only temporary corpus inputs, which were removed before
commit. No Java I2P/i2pd execution occurred in this closure.

## Support ledger and unresolved deviations

All NTCP2 support rows remain `status = "experimental"` and
`advertised = false`. The eight-entry private-testnet manifest remains a
preflight contract, not execution evidence. The daemon remains disabled.

Unresolved blockers are:

1. no complete runtime-owned initiator/responder handshake and authenticated
   data-phase adapter;
2. no end-to-end composition with `TransportManager`, replay decisions,
   duplicate-link registration, or bidirectional I2NP delivery;
3. no authorized Java I2P/i2pd private-testnet run or sanitized evidence;
4. no fresh post-push CI run recorded in this repository.

## Milestone 4 readiness

**Not ready.** The aggregate Milestone 3 closure must remain blocked until the
complete adapter, local end-to-end cleanup matrix, authorized mixed-router
handshakes/data exchange, sanitized evidence, and fresh CI/MSRV/dependency/fuzz
gates all exist.
