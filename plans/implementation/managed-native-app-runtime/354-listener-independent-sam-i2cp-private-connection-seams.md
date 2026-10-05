# Plan 354 — listener-independent SAM/I2CP private connection seams

Status: **registered-managed-app-private-client-transport-seams**.

Classification: **infrastructure + invariant**. This plan extracts private, listener-independent connection drivers from the existing daemon-owned SAM and I2CP services so a later managed-app gateway can reuse the exact protocol implementations without granting an application a loopback socket.

Roadmap:
- `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependencies:
- Plan 352 closed the managed-app v1 policy contract.
- Plan 353 closed branch integration/planning-authority reconciliation.
- Existing SAM and I2CP product authorities remain unchanged.

Successor:
- Plan 355 consumes these seams to build the router-side app-principal gateway.

## Objective

Create one reusable daemon-owned connection-driving seam for SAM and one for I2CP that can operate over an injected bounded asynchronous byte stream without binding or connecting a host network socket.

The existing loopback listeners must become adapters over those same connection drivers rather than remaining the only way to exercise the protocol implementations.

The managed-app private SAM profile must additionally reject `STREAM FORWARD` and any equivalent host-target operation. A hostile app must not be able to ask the trusted router process to connect to a loopback/LAN service that the app sandbox itself cannot reach.

This plan does **not** expose the managed-app v1 channel, authenticate an app principal, launch a process, add an AppManager, or make SAM/I2CP newly user-visible.

## Why this plan is ready

The required protocol owners already exist:

- `i2pr-api` owns runtime-neutral SAM 3.1 and I2CP state/wire contracts.
- `i2pr-daemon::sam::SamServiceState` owns the live SAM composition and destination/streaming state.
- `i2pr-daemon::i2cp::I2cpServiceState` owns the live I2CP composition.
- both services already have bounded listener admission and supervised connection tasks;
- Plan 345 explicitly forbids treating the existing loopback listeners as the managed-app transport;
- Plans 349/352 froze the managed-app contract sufficiently for the later gateway.

The missing substrate is transport ownership: both daemon connection loops are still typed around `tokio::net::TcpStream`, and SAM's raw transition transfers a `TcpStream` into the raw-stream driver.

## Current implementation evidence

At the planning baseline:

- `sam.rs::handle_connection`, command dispatch, reply writing, STREAM connect/accept, DEST generation, and raw transition receive a concrete `TcpStream`;
- SAM reads `peer_addr()` for connection metadata and transfers stream ownership on `ConnectionDisposition::RawTransition`;
- the SAM listener and per-connection driver share one `SamServiceState`;
- `STREAM FORWARD` causes the daemon to connect to a local target with `TcpStream::connect`;
- `i2cp.rs::handle_connection`, frame dispatch, chunk reads, and message writes receive a concrete `TcpStream`;
- both services already supervise accepted connections through `ChildScope` and cancellation tokens;
- neither service currently exposes a no-listener private connection entry point.

## Architecture decisions frozen by this plan

### 1. The daemon remains the runtime owner

No Tokio or I/O ownership moves into `i2pr-api`, `i2pr-app-proto`, `i2pr-client`, or another runtime-neutral crate.

The private connection seams live under `i2pr-daemon` and may depend on Tokio because the daemon is already the composition/runtime adapter.

### 2. One protocol implementation, two transport origins

SAM and I2CP each get one connection-driving implementation used by:

- the existing loopback TCP listener adapter; and
- an injected private bidirectional stream used by future managed-app composition.

Do not fork command/frame state machines into "socket" and "app" implementations.

The seam should accept an owned async bidirectional byte stream satisfying the minimum required Tokio `AsyncRead + AsyncWrite + Unpin + Send + 'static` behavior, or an equivalent reviewed internal abstraction.

### 3. Transport origin is explicit

Introduce a small daemon-local connection-origin/profile value rather than inferring policy from the I/O type.

At minimum distinguish:

- ordinary loopback SAM/I2CP listener connection;
- managed-app/private connection.

Logging/diagnostics may carry optional peer metadata for listener clients, but protocol correctness must not require `peer_addr()`.

### 4. Managed-app SAM forbids host-target forwarding

The managed-app/private SAM profile must reject `STREAM FORWARD` before any host connection attempt or forwarding registration is created.

Reason: `STREAM FORWARD` is an instruction to the trusted router to open a host-local TCP connection. Allowing it would create an indirect loopback/LAN capability that bypasses the secured app network model.

The existing ordinary loopback SAM profile retains its current tested FORWARD behavior.

If review discovers any other SAM command that causes router-owned host networking outside I2P, classify it under the same managed-app deny policy before closure.

### 5. Private connection seams do not imply external listener enablement

A private connection must not require:
- `sam.enabled = true`;
- `i2cp.enabled = true`;
- a bound TCP port;
- a localhost connection;
- an ephemeral listener/socket-pair fallback.

Managed-app access is a future capability path independent from public/loopback listener configuration.

### 6. Existing service semantics remain authoritative

The private seam reuses the current SAM/I2CP service state, session accounting, destination ownership, protocol limits, and cancellation semantics.

This plan does not create a second SAM implementation, second I2CP implementation, or new destination/tunnel stack.

## Invariants that must not regress

1. `i2pr-api` remains runtime-neutral.
2. Existing SAM 3.1 and I2CP loopback listeners remain disabled by default and retain their existing support claims.
3. Existing loopback listener wire behavior and acceptance evidence remain unchanged.
4. Managed-app/private SAM cannot perform `STREAM FORWARD`.
5. Private connection execution binds no listener and opens no host connection as part of admission.
6. I2CP private connections have no new host-network capability.
7. Every connection task is owned by `ChildScope` or an equivalent existing supervisor owner; no detached `tokio::spawn`.
8. Cancellation closes the connection and releases session/destination resources.
9. Per-service connection/session ceilings remain bounded.
10. No router support advertisement changes.

## Scope

### In scope

- refactor SAM connection driving away from concrete `TcpStream` where required;
- refactor SAM raw-stream handoff to work over the injected bidirectional transport;
- retain concrete TCP only where the ordinary SAM FORWARD target genuinely needs it;
- add explicit SAM connection policy/origin and managed-app FORWARD denial;
- refactor I2CP connection driving away from concrete `TcpStream`;
- expose crate-visible/private connection entry points suitable for Plan 355;
- separate listener binding/admission from protocol connection execution;
- add no-listener service-state/context constructors if listener config currently blocks private use;
- focused regression/in-memory transport tests;
- architecture/checker documentation necessary to freeze the seam.

### Out of scope

- `i2pr-app-proto` framing/session handling;
- app identity authentication;
- capability checks;
- app-principal namespace ownership;
- package lifecycle or AppManager APIs;
- process launch/sandbox/resource enforcement;
- Proposal 170 / `control_scoped`;
- clearnet broker;
- embedded UI;
- changing SAM/I2CP protocol breadth or support claims.

## Required production changes

### A. Define daemon-local connection origin/policy

Add an explicit type, name chosen by implementation, carrying only policy-relevant facts.

For SAM it must make host-target forwarding authorization explicit. Do not branch on a magic peer address, listener port, or whether the stream happens to be a `TcpStream`.

I2CP may share the origin type if useful, but must not acquire SAM-specific semantics.

### B. Extract one SAM connection driver

Refactor the accepted-socket path so the listener adapter performs only:

1. bind/accept;
2. bounded client admission;
3. origin metadata creation;
4. supervised invocation of the shared SAM connection driver.

The connection driver owns all existing handshake/command/session behavior.

SAM raw STREAM transition must accept the generic/private transport. Prefer `tokio::io::split` or another generic split over constructing an internal TCP socket pair.

Do **not** use a transient localhost listener as the managed-app bridge.

### C. Enforce managed-app SAM command policy

Before a `STREAM FORWARD` registration or `TcpStream::connect` can occur:

- inspect the explicit connection profile;
- reject the command deterministically for managed-app/private origin;
- return the existing closest protocol-level error without widening SAM wire vocabulary solely for this plan;
- prove zero host connect attempts and zero forwarding registrations on the denied path.

If another host-target command exists, add it to the same closed deny set.

### D. Extract one I2CP connection driver

The listener adapter performs only:

1. bind/accept;
2. bounded admission and connection-id allocation;
3. supervised invocation of the shared I2CP connection driver.

Generalize read/write helpers to the injected async transport while preserving:
- protocol byte handling;
- timeouts;
- frame decoder behavior;
- inbound-notify wakeups;
- connection teardown;
- session rollback.

### E. Add private no-listener test constructors/seams

Tests must be able to instantiate the service state and drive one private connection through `tokio::io::duplex` or an equivalent in-memory transport without binding a TCP listener.

The production API should remain narrow enough that only daemon composition/future gateway code can use it.

## Work packages

### WP1 — Freeze transport-seam architecture

Update the managed-app roadmap/architecture notes before production refactoring:
- one connection driver per protocol;
- listener as adapter;
- private origin;
- managed SAM FORWARD denial;
- no loopback fallback.

### WP2 — SAM private connection seam

Refactor SAM connection/read/write/raw transition and retain ordinary listener behavior.

Add the managed-app command policy.

### WP3 — I2CP private connection seam

Refactor I2CP connection/read/write execution and retain ordinary listener behavior.

### WP4 — Focused private-transport tests

Drive both protocol implementations through an in-memory transport.

### WP5 — Boundary and regression guards

Update static checks/documentation so a later change cannot replace the private seam with an app-visible localhost listener.

## Failure, cancellation, restart, and contention semantics

- admission failure allocates no session/destination state;
- parse/protocol failure closes only that connection and performs existing teardown;
- connection cancellation propagates through the existing service cleanup;
- raw SAM transition remains owned by the same supervised task lifetime;
- managed FORWARD denial occurs before forwarding registration/host connect side effects;
- listener and private connections use the same existing bounded service/session limits;
- no retry loop is introduced by this plan;
- daemon restart retains no new persistence because these are live connection seams only.

## Compatibility and migration

No wire migration is expected.

Existing SAM/I2CP loopback endpoints continue to use the same protocol versions and tested subset.

The private connection seam is an internal Rust API and is not itself the managed-app public ABI.

If genericizing SAM requires externally observable wire changes, stop and register a narrower SAM corrective rather than folding a protocol behavior change into this infrastructure plan.

## Required tests

At minimum:

### SAM

- existing loopback listener tests remain green;
- private in-memory connection completes SAM HELLO VERSION negotiation;
- private connection can execute a non-host-target command through the same dispatcher;
- managed-app/private `STREAM FORWARD` is rejected;
- rejected FORWARD leaves forwarding count/state unchanged and performs no target connect;
- ordinary loopback FORWARD behavior remains unchanged;
- raw STREAM transition works over the generic private transport or a focused equivalent proves the generic handoff;
- cancellation/EOF releases connection/session-owned state.

### I2CP

- existing loopback listener tests remain green;
- private in-memory connection accepts the protocol byte and GetDate/SetDate handshake path;
- malformed preamble/frame behavior matches listener behavior;
- cancellation/EOF tears down the connection and owned sessions;
- private execution binds no listener.

### Structural

- no new Tokio/runtime dependency enters runtime-neutral crates;
- no localhost socket-pair bridge is introduced for managed-app use;
- no detached task ownership;
- support inventory unchanged.

## Exact verification commands

Focused:

```text
cargo fmt --all --check
cargo check --locked -p i2pr-daemon --all-targets
cargo test --locked -p i2pr-daemon sam -- --test-threads=1
cargo test --locked -p i2pr-daemon i2cp -- --test-threads=1
cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-daemon --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Closure must also run the full routine floor from `AGENTS.md`.

## Documentation updates

Required:
- `docs/architecture/i2pr-api.md`;
- daemon architecture documentation covering listener vs private connection ownership;
- `plans/subsystems/managed-native-app-runtime-roadmap.md`;
- `plans/registry.md`;
- `plans/closure/managed-native-app-runtime/354-status.md` at closure.

Update the normative managed-app v1 reference only if needed to clarify that its future SAM/I2CP streams transport protocol bytes; do not add router-gateway authority semantics there until Plan 355.

## Acceptance criteria

Plan 354 passes only when:

1. SAM and I2CP each have one listener-independent connection-driving implementation;
2. existing TCP listeners are adapters over that implementation;
3. both protocols can be driven through a no-listener in-memory transport;
4. no localhost/ephemeral TCP bridge is used for managed-app/private execution;
5. managed-app/private SAM rejects `STREAM FORWARD` before host-network side effects;
6. ordinary loopback SAM FORWARD behavior is preserved;
7. SAM raw transition no longer requires the managed-app transport itself to be a `TcpStream`;
8. I2CP private execution preserves existing handshake/session teardown semantics;
9. supervision, bounds, and cancellation remain explicit;
10. runtime-neutral crate boundaries remain green;
11. support claims/config defaults remain unchanged;
12. full routine floor passes.

## Stop conditions

Stop and register a corrective/sub-plan if:

- SAM raw transition cannot be generalized without changing SAM wire semantics;
- another existing SAM command is discovered that exposes host networking and cannot be cleanly policy-gated;
- private execution requires a localhost listener/socket pair;
- protocol-state code would have to move into a new duplicate implementation;
- a new cross-platform OS IPC decision is required;
- existing loopback acceptance evidence regresses for reasons other than a localized bug that can be fixed without semantic expansion.

## Closure evidence required

The closure must include:

- before/after connection ownership diagram;
- exact production entry points for loopback and private connections;
- proof both origins call the same protocol driver;
- managed FORWARD negative test and zero-side-effect evidence;
- private SAM and I2CP in-memory handshake evidence;
- existing listener regression evidence;
- dependency/runtime-boundary results;
- full routine-floor results;
- support/config diff confirming no capability promotion;
- unblock audit for Plan 355.

## Handoff notes

This is intentionally an internal substrate plan.

Do not implement the app protocol, an app principal, process launch, package handling, or a sandbox here. The only goal is to make the existing SAM/I2CP implementations safely reusable without giving a managed app a host socket.
