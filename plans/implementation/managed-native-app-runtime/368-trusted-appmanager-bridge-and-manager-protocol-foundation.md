# Plan 368 — trusted AppManager bridge and manager protocol foundation

Status: **registered-managed-app-manager-bridge-and-protocol-foundation**.

Classification: **invariant + infrastructure + capability boundary**.

Roadmap:
- `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependencies:
- Plan 354 closed the listener-independent private SAM/I2CP connection seams.
- Plan 355 closed the principal/capability router gateway over those seams.
- ADR 0032 remains the process/capability trust-boundary authority.

Successor:
- Plan 369 consumes this bridge to add the trusted `i2pr-appd` / `i2pr-apphost` runtime and supervised fixture-process lifecycle.

## Objective

Create the private router↔AppManager protocol and daemon-side bridge needed for a trusted out-of-process application manager to consume `AppGatewaySession` without exposing the daemon's Rust internals, loopback SAM/I2CP listeners, or a general Proposal-170 credential.

This plan freezes and implements the router-facing half only:

1. a runtime-neutral manager protocol contract;
2. a daemon-owned bridge that authenticates authority by trusted composition rather than app-controlled bytes;
3. bounded manager session/service-stream lifecycle over the existing `AppGatewaySession`;
4. exact service-byte forwarding for SAM/I2CP;
5. explicit exclusion of package lifecycle, process launch, OS sandboxing, AppManager administrator operations, brokered clearnet, UI hosting, and `control_scoped`.

No application process or AppManager process is launched by Plan 368.

## Why this plan is ready

The router-side capability boundary is now concrete:

- `i2pr-app-proto` owns the language-neutral application contract and identity/capability types.
- `AppGatewayAuthorization` already binds one trusted `AppPrincipal` to immutable `EffectiveCapabilities`.
- `AppGatewaySession` owns isolated private SAM/I2CP contexts for one app instance.
- capability checks occur before backend allocation;
- managed SAM denies host-target `STREAM FORWARD`;
- no localhost fallback exists.

The missing boundary is process-independent transport between a future trusted manager and the daemon.

## Current implementation evidence

At the planning baseline:

- `crates/i2pr-daemon/src/app_gateway.rs` is crate-private and explicitly has no production runtime caller;
- no manager protocol crate exists;
- no daemon service accepts manager requests;
- no manager process is spawned;
- no app process is spawned;
- app-facing `hello` remains a declaration, not authentication;
- `control_scoped` is typed unsupported at the gateway;
- package/admin operations remain reserved in `i2pr-app-proto`.

These absences are part of the baseline and must not be bypassed with a localhost bridge or direct daemon-library linkage.

## Architecture decisions frozen by this plan

### 1. Add a distinct private manager protocol

Introduce a runtime-neutral workspace crate, expected name:

```text
i2pr-app-manager-proto
```

It may depend on `i2pr-app-proto` for principal/capability/service value types but must not depend on `i2pr-daemon`, `i2pr-runtime`, router protocol/state crates, Tokio, sockets, process APIs, filesystem APIs, DNS, or sandbox backends.

The manager protocol is not:
- the application v1 protocol;
- SAM;
- I2CP;
- Proposal 170/I2PControl;
- an AppManager administrator API.

It is an internal trusted-component protocol for projecting already-authenticated app authority into the router gateway.

### 2. Freeze the protocol before daemon integration

Add a normative reference, expected:

```text
specs/references/managed-app-manager-protocol-v1.md
```

The protocol must be language-neutral and versioned independently from the application protocol.

At minimum it must define:
- fixed handshake magic/version/role;
- strict directional control vocabulary;
- bounded frame/control payloads;
- exact request/reply correlation;
- opaque manager-session handles assigned by the daemon;
- service-stream handles scoped to one manager session;
- session create/close;
- service open/close/reset for `sam` and `i2cp`;
- exact ordered service data bytes;
- backend close/reset notification;
- bounded health/shutdown semantics;
- deterministic unknown/duplicate/stale-handle rejection.

The manager sends one trusted `AppPrincipal`, immutable `EffectiveCapabilities`, and bounded gateway limits when requesting session creation. Possession of a future inherited manager transport authenticates the *manager process*; the daemon still validates every protocol message and enforces the maximum authority available through this bridge.

### 3. The bridge has a deliberately small authority ceiling

Plan 368 allows only:
- create app gateway session;
- close app gateway session;
- open SAM service connection when effective SAM is present;
- open I2CP service connection when effective I2CP is present;
- forward opaque protocol bytes;
- observe backend termination;
- health/shutdown.

It must reject or make unrepresentable:
- `control_scoped`;
- brokered clearnet;
- package install/update/uninstall;
- grant/revoke;
- network-policy mutation;
- launch-profile mutation;
- process launch/stop;
- router configuration;
- general I2PControl dispatch;
- arbitrary daemon method names.

A compromised manager must not turn this bridge into router-administrator authority.

### 4. Manager authority is distinct from app declarations

The daemon bridge never constructs authorization from:
- `AppToHostMessage::Hello`;
- `RequestedCapability`;
- app manifest bytes;
- service data bytes.

Only the trusted manager-protocol session-create request may supply the principal/effective grants, and only after the future manager transport itself is trusted by composition.

The bridge maps this request to the existing `AppGatewayAuthorization::from_trusted_composition` path.

### 5. One manager session maps to one gateway session

For every accepted manager session:
- allocate one daemon-owned opaque session handle;
- construct exactly one `AppGatewaySession`;
- bind it to one `AppInstanceId`;
- keep service handles local to that session;
- reject cross-session/stale handles;
- tear down every backend stream when the manager session closes or the manager transport dies.

No Rust pointer/address or daemon-internal identifier crosses the wire.

### 6. Exact byte-stream mapping

For an authorized service open:
- one manager service stream maps to one Plan-355 backend connection;
- manager data payloads are forwarded as exact SAM/I2CP protocol octets;
- no base64, JSON wrapping of service data, rewriting, or protocol interpretation is introduced;
- backpressure is bounded;
- a slow manager cannot cause unbounded buffering.

### 7. Transport ownership remains outside the contract

Plan 368 implements the bridge over an injected reliable bidirectional byte stream suitable for in-memory qualification.

It does not choose or open:
- Unix-domain paths;
- TCP loopback;
- named public endpoints;
- inherited pipe/socket handles;
- process stdio.

Plan 369 owns the concrete inherited daemon↔`i2pr-appd` transport.

No localhost transport may be used as a temporary fallback.

## Invariants that must not regress

1. `i2pr-app-proto` remains runtime/OS neutral.
2. `i2pr-app-manager-proto` is also runtime/OS neutral.
3. App-controlled identity/request bytes cannot construct router authorization.
4. Manager sessions cannot attach to another manager session's gateway or service handles.
5. Capability checks remain in the daemon before backend allocation.
6. `control_scoped` remains unavailable.
7. Managed SAM host-target forwarding remains denied.
8. No ordinary SAM/I2CP listener is required.
9. No package/process/sandbox/admin capability is claimed.
10. Every session, request, frame, queue, and service stream is bounded.
11. Manager disconnect tears down all descendant gateway state.
12. This work does not change RouterInfo/support advertisement or router protocol claims.

## Scope

### In scope

- ADR 0035 or the next free ADR documenting manager process/authority separation and inherited-capability intent;
- new runtime-neutral manager protocol crate;
- normative manager-protocol v1 reference;
- dependency/runtime guards for the new contract crate;
- daemon `app_manager_bridge` or equivalent;
- injected-stream manager bridge entry point;
- bounded manager session/stream registries;
- mapping to `AppGatewaySession`;
- exact SAM/I2CP data forwarding;
- close/reset/backend-completion propagation;
- cancellation, EOF, max+1, stale-handle, duplicate-handle and cross-session tests;
- static checker/CI-floor integration.

### Out of scope

- `i2pr-appd` process;
- `i2pr-apphost` process;
- OS process spawning;
- package inventory/signature/storage;
- grant persistence;
- AppManager administrator API;
- secured or unsafe app launch;
- OS sandbox;
- clearnet broker;
- UI bridge implementation;
- Proposal-170 scoped adapter.

## Required production changes

### A. Freeze ADR authority

Record:
- `i2pr-daemon` remains the router-side authority;
- `i2pr-appd` is a future separate trusted process;
- daemon↔manager authority travels over a private inherited capability transport, not a discoverable localhost endpoint;
- the private manager protocol has a smaller authority surface than Proposal 170;
- `i2pr-appd` authenticates app processes in a later plan; the daemon authenticates only the manager transport and validates the bounded authority it submits;
- manager failure is not a router-core availability dependency;
- no sandbox guarantee exists yet.

### B. Add `i2pr-app-manager-proto`

Expected properties:
- `#![forbid(unsafe_code)]`;
- no Tokio, sockets, filesystem, process, DNS, dynamic loading, or OS sandbox API;
- no serde ambiguity/unknown fields;
- strict limits and exact-consumption decoders;
- redacted diagnostics;
- no raw secret material.

Types should include validated opaque manager session/service IDs rather than exposing daemon IDs.

### C. Implement daemon bridge state

The bridge owns:
- bounded manager request tracking;
- bounded app-session map;
- bounded service-stream map per app session;
- one cancellation root for all bridge-owned gateway sessions;
- deterministic teardown.

The bridge must not own package/grant policy.

### D. Bind session creation to existing gateway authority

On accepted session creation:
1. validate principal and limits;
2. validate effective capabilities;
3. construct daemon-local gateway authorization through the existing trusted constructor;
4. construct one `AppGatewaySession`;
5. return only an opaque manager-session handle.

No backend service state is allocated until a later authorized service open.

### E. Compose service streams

`sam` and `i2cp` service opens call the existing Plan-355 gateway methods.

`control_scoped` returns typed unsupported before side effects.

Manager stream close/reset and manager EOF must cancel the associated backend connection.

### F. Add executable guards

Add a focused checker proving:
- only the daemon consumes manager protocol as a router-side implementation;
- manager protocol cannot depend on router/runtime/OS owners;
- no localhost socket/listener/connect path exists in the bridge;
- manager control vocabulary contains no admin/config/package operation;
- `control_scoped` remains denied;
- the app-facing `hello` is not referenced as authorization input.

## Work packages

### WP1 — ADR + language-neutral protocol freeze
Write ADR 0035 (or next free ADR) and the manager-protocol v1 reference before production integration.

### WP2 — Contract crate
Add `i2pr-app-manager-proto`, codec/golden vectors, limits, malformed/fuzz-smoke tests, dependency guards.

### WP3 — Daemon bridge
Add session/service ownership and exact gateway composition over an injected stream.

### WP4 — Isolation/lifecycle evidence
Prove multi-session isolation, stale-handle rejection, sibling survival, bounded admission and deterministic teardown.

### WP5 — Checker/docs/closure
Wire the boundary checker into routine CI/floor and update architecture/roadmap/registry.

## Failure, cancellation, restart, and contention semantics

- malformed/unsupported manager input closes or rejects only the offending manager request/session according to the frozen protocol;
- unauthorized service open is side-effect free;
- manager transport EOF cancels every manager-created gateway session and backend connection;
- one backend EOF closes only that service stream;
- one app session failure does not close sibling app sessions unless the manager transport itself fails;
- max+1 manager session/service/request is rejected synchronously;
- no unbounded queue or automatic retry loop;
- daemon restart loses all manager sessions; no persistence is introduced;
- manager restart recovery is Plan 369 scope.

## Compatibility and migration

Both managed-app contracts remain unreleased.

The new manager protocol is private infrastructure v1 and is not an external SDK/API compatibility promise.

No SAM/I2CP wire version or support state changes.

## Required tests

At minimum:

### Contract
- handshake golden bytes/version rejection;
- directional control decoder separation;
- duplicate/unknown field rejection;
- frame truncation, oversize, max+1 and malformed IDs;
- request/reply correlation;
- fuzz smoke/no panic.

### Authority
- session creation with valid trusted principal/effective grants;
- app `hello`/requested capability cannot construct manager authorization;
- unsupported `control_scoped` allocates nothing;
- SAM/I2CP capability denial allocates nothing.

### Isolation
- two manager sessions with identical application-level SAM/I2CP identifiers remain isolated;
- service handle from session A is invalid in B;
- closed/stale session and service handles fail deterministically;
- one sibling backend EOF does not close another.

### Lifecycle/resources
- manager EOF tears down all gateway sessions;
- max+1 sessions/streams/requests fail;
- repeated create/open/close returns counts to baseline;
- backpressure remains bounded.

### Structural
- no loopback fallback;
- no Proposal-170/admin operation vocabulary;
- no process/package/sandbox code;
- no support inventory change.

## Exact verification commands

Focused:

```text
cargo fmt --all --check
cargo check --locked -p i2pr-app-manager-proto -p i2pr-daemon --all-targets
cargo test --locked -p i2pr-app-manager-proto --all-targets
cargo test --locked -p i2pr-daemon app_manager_bridge -- --test-threads=1
cargo test --locked -p i2pr-daemon app_gateway -- --test-threads=1
cargo clippy --locked -p i2pr-app-manager-proto -p i2pr-daemon --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-app-manager-proto -p i2pr-daemon --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-managed-app-private-client-seams.py
python3 scripts/check-managed-app-gateway-boundary.py
python3 scripts/check-managed-app-manager-boundary.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Closure must also run the current full routine floor in `AGENTS.md`.

## Documentation updates

Required:
- ADR 0035 or next free ADR;
- `specs/references/managed-app-manager-protocol-v1.md`;
- `docs/architecture/i2pr-app-proto.md`;
- `docs/architecture/i2pr-daemon.md`;
- `docs/architecture/dependency-graph.md`;
- managed-app roadmap;
- registry;
- closure record.

## Acceptance criteria

Plan 368 passes only when:

1. a frozen private manager protocol exists as a runtime-neutral contract;
2. the daemon bridge consumes it over an injected reliable stream;
3. session creation maps only trusted manager authority to the existing gateway constructor;
4. manager/app declarations cannot bypass the authority path;
5. SAM/I2CP service streams use exact existing gateway connections;
6. `control_scoped` remains unsupported;
7. cross-session/stale-handle isolation is proven;
8. manager EOF tears down all descendant router resources;
9. all queues/counts/frames/requests are bounded;
10. no localhost/process/package/sandbox/admin implementation lands;
11. static guards enforce the boundary;
12. focused and full routine floors pass.

## Stop conditions

Stop and register a successor/corrective if:

- manager transport requires a discoverable localhost endpoint;
- bridge correctness requires general Proposal-170 access;
- package/grant persistence is required to create router sessions;
- cross-session isolation requires rewriting SAM/I2CP wire identifiers;
- the manager protocol needs OS/process types;
- daemon gateway authority must be widened beyond Plan 355.

## Closure evidence required

The closure must include:
- ADR/protocol freeze commit;
- complete message/authority matrix;
- dependency graph diff;
- two-session isolation evidence;
- capability-denial side-effect evidence;
- manager EOF teardown evidence;
- no-loopback/no-admin static guard evidence;
- full routine-floor results;
- support-inventory diff;
- unblock audit for Plan 369.

## Handoff notes

Plan 368 is the router-facing half of the trusted runtime milestone. Do not launch `i2pr-appd` or applications here. Plan 369 owns concrete process supervision and the inherited capability transport once this bridge contract is closed.
