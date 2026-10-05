# Plan 355 — router app-principal gateway over private SAM/I2CP seams

Status: **in-progress-managed-app-principal-gateway**.

Classification: **infrastructure + invariant + bounded capability plumbing**. This plan creates the router-side managed-app gateway that binds a trusted app principal and effective capabilities to the private SAM/I2CP connection seams delivered by Plan 354.

Roadmap:
- `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependency:
- Plan 354 must close with listener-independent SAM/I2CP connection seams and managed-app SAM host-target denial.

Interface dependencies:
- Plan 352 closed the corrected managed-app v1 contract.
- ADR 0032 defines the separate-process/capability trust boundary.

This plan does not depend on Proposal 170 completion because `control_scoped` remains unavailable here.

## Objective

Provide one narrow daemon-owned router gateway API through which a future trusted application runtime/manager can request an app-scoped SAM or I2CP protocol connection without:

- handing the application a host socket;
- enabling the ordinary loopback SAM/I2CP listeners;
- exposing router internals;
- trusting application-supplied identity as authentication;
- sharing protocol/session namespaces across unrelated app launch instances;
- granting any capability the trusted runtime did not already supply.

The gateway is the router-side capability boundary. It is not the package/process manager and it does not yet own the full app-channel wire transport.

## Why this plan is ready after Plan 354

The public/application contract is already frozen:

- `AppPrincipal`, `Capability`, `EffectiveCapabilities`, `AppService`, stream ceilings, and owned-resource vocabulary exist in `i2pr-app-proto`;
- Plan 352 closed the last known network-policy identity defect;
- Plan 354 supplies protocol connections that do not depend on host TCP listeners;
- existing SAM/I2CP implementations remain the only protocol owners.

What is missing is the router authorization/composition layer between a trusted future runtime and those protocol connections.

## Current design constraints

The gateway must preserve this ownership split:

```text
untrusted native app process
        |
        | managed-app v1 frames
        v
future trusted app runtime / manager
        |
        | authenticated principal + immutable effective capabilities
        | + one private byte stream per router service connection
        v
i2pr-daemon AppPrincipalGateway
        |
        +--> existing SAM private connection seam
        |
        +--> existing I2CP private connection seam
        |
        +--> control_scoped: unavailable until a separate Proposal-170 adapter plan
```

The future runtime may live in another crate/repository or process architecture. Plan 355 therefore freezes the router-facing authorization API without choosing the final OS IPC or process-launch mechanism.

## Architecture decisions frozen by this plan

### 1. The router does not authenticate from app-supplied Hello fields

`AppToHostMessage::Hello` is a declaration, not proof.

The gateway accepts an authorization object only from trusted composition. That object contains:

- the authenticated `AppPrincipal`;
- immutable `EffectiveCapabilities` for that launch instance;
- bounded gateway limits no greater than the v1 ceilings.

The authorization object must not implement `Deserialize` or be constructible directly from managed-app wire bytes.

A later runtime is responsible for proving that the process/channel it owns corresponds to the supplied principal before it calls the gateway.

### 2. One gateway session equals one launch-instance authority domain

A gateway session is bound to exactly one `AppPrincipal` / `AppInstanceId`.

SAM/I2CP service contexts owned by one gateway session are not shared with:
- another app instance;
- another publisher;
- the public/loopback SAM listener;
- the public/loopback I2CP listener.

This makes application-selected SAM session identifiers and I2CP connection/session identifiers local to that app gateway authority domain and prevents cross-app attachment by identifier guessing.

### 3. Capability checks happen before backend allocation

Service authorization is exact:

- `AppService::Sam` requires effective `Capability::Sam`;
- `AppService::I2cp` requires effective `Capability::I2cp`;
- `AppService::ControlScoped` returns typed unsupported/unavailable in Plan 355 even if `Capability::ControlScoped` is present;
- `BrokeredTcp` remains non-openable in v1.

A denied/unsupported open allocates no backend protocol connection, destination/session state, task, or resource handle.

### 4. Router gateway input is trusted composition, not raw app control authority

Plan 355 does **not** make `i2pr-daemon` the owner of package permission requests, AppManager/admin commands, UI messages, install/update state, or process lifecycle.

The future manager owns the outer managed-app channel and maps an authorized logical stream to this gateway.

The router gateway API must therefore consume a trusted open request/service selection rather than interpreting an app's `Hello` as authority.

### 5. One logical app service stream maps to one protocol connection

Freeze the data-plane mapping:

- one managed-app logical stream opened for `sam` corresponds to one SAM protocol connection;
- one logical stream opened for `i2cp` corresponds to one I2CP protocol connection;
- data payload bytes are the exact underlying SAM/I2CP octets in order;
- no base64/JSON wrapping, protocol rewriting, or application-layer inspection is introduced by the gateway;
- close/reset tears down that one backend connection;
- backend EOF/failure closes/resets the corresponding future logical stream.

The future runtime owns multiplexing and stream-id correlation; the router gateway owns backend authorization and protocol connection lifetime.

### 6. No localhost fallback

If a private backend seam is unavailable, the gateway fails closed.

It must never:
- connect to the configured SAM/I2CP loopback port;
- bind an ephemeral listener;
- use a host proxy;
- enable the ordinary listener as a convenience fallback.

### 7. Canonical shared router services remain shared where semantically required

Private per-principal protocol contexts may own their own client session/destination registries, but canonical router-wide services must be injected/reused rather than duplicated.

At minimum:
- SAM naming uses the canonical address-book handle when active;
- protocol connections use the same daemon/router product owners as their ordinary counterparts;
- no second NetDB, transport manager, tunnel subsystem, or control plane is created.

## Invariants that must not regress

1. Apps cannot increase their own `EffectiveCapabilities`.
2. App principal identity is distinct from router, Destination, publisher, and administrator identity.
3. A service open without the required effective capability fails before backend allocation.
4. App instances cannot attach to each other's SAM/I2CP client state by guessing protocol identifiers.
5. Private app service use does not require or expose loopback listeners.
6. Managed SAM cannot use `STREAM FORWARD` or another router-owned host-target path.
7. `control_scoped` does not hand out an administrator Proposal-170 credential.
8. No package/process/sandbox capability is claimed.
9. No support inventory/RouterInfo advertisement changes.
10. Every connection/task/resource is bounded and owned.

## Scope

### In scope

- add `i2pr-app-proto` as a direct daemon dependency;
- add a daemon-local `app_gateway` module or equivalent;
- trusted authorization/session types;
- capability-to-service authorization;
- per-principal/private SAM and I2CP context ownership;
- bounded connection admission;
- supervised backend connection lifetime;
- canonical address-book injection into private SAM state;
- exact service-byte-stream mapping documentation;
- principal isolation/resource cleanup tests;
- static guards preventing localhost fallback and raw app identity from becoming authorization.

### Out of scope

- app process spawning;
- OS sandboxing;
- package archive/signature format;
- install/update/uninstall;
- persistent permission/grant store;
- AppManager administrator wire implementation;
- handling `permission_request` as a policy mutation;
- embedded UI;
- brokered clearnet;
- Proposal 170 scoped adapter implementation;
- cross-process router↔manager IPC selection;
- public SDK convenience APIs.

## Required production changes

### A. Add the router gateway authorization type

Introduce a daemon-owned type, conceptually:

```text
AppGatewayAuthorization {
    principal: AppPrincipal,
    effective_capabilities: EffectiveCapabilities,
    limits: AppGatewayLimits,
}
```

Requirements:
- no serde/wire decoder;
- no constructor from `RequestedCapability`;
- limits cannot exceed managed-app v1 ceilings;
- diagnostic/debug output must not expose secret material (none should be present).

The exact Rust names may differ.

### B. Add one gateway session owner

A gateway session:
- is created from one trusted authorization;
- lazily creates/owns its private SAM and I2CP service contexts;
- tracks bounded open backend connections;
- owns child cancellation for all backend tasks;
- tears all state down on drop/shutdown;
- cannot change principal or capabilities in-place.

Capability changes, when later implemented, require a manager-owned lifecycle decision; do not invent dynamic self-upgrade semantics here.

### C. Add exact service authorization

Provide one closed mapping from `AppService` to required effective capability.

The check occurs before:
- service-state lazy construction;
- connection-id/resource allocation;
- child task spawn;
- protocol byte consumption.

Return a typed gateway error that the future runtime can map to managed-app `RequestErrorCode` without string inspection.

### D. Compose Plan-354 private SAM/I2CP connections

For authorized SAM/I2CP opens:
- create one private protocol connection over the supplied bidirectional byte stream;
- supervise it under the gateway session;
- use managed-app/private connection origin;
- preserve existing protocol timeouts and resource limits;
- release all state on EOF/cancel/error.

No host socket is returned to the app/runtime.

### E. Principal isolation

Prove private service contexts are instance-scoped.

At minimum:
- two different app principals may use the same SAM session identifier without collision;
- one principal cannot attach STREAM CONNECT/ACCEPT to another principal's SAM session;
- I2CP connection/session identifiers are not shared across principal contexts;
- ordinary loopback listener state is not visible through an app gateway session.

Do not achieve isolation by rewriting application-visible SAM/I2CP identifiers on the wire; namespace at the service-context ownership boundary.

### F. Shared canonical router dependencies

Pass canonical router-owned handles explicitly where needed.

For SAM naming, use the same `SharedAddressBook` owner used by the daemon listener.

If implementation discovers another router-wide singleton required for correct protocol behavior, inject that owner rather than constructing an app-private duplicate.

### G. Freeze service-byte-stream semantics

Update `specs/references/managed-native-app-runtime-v1.md` and architecture docs to state:
- an opened SAM/I2CP logical stream represents one protocol connection;
- data frames carry exact underlying protocol bytes;
- the future runtime maps stream ids to gateway connection handles;
- `control_scoped` is reserved until its separately gated adapter exists.

This is a clarification of the unreleased v1 contract, not a wire-version change, provided no consumer is discovered.

## Work packages

### WP1 — Freeze router gateway ownership

Update spec/architecture before code:
- trusted authorization object;
- one instance per principal;
- exact service/capability map;
- no outer AppManager ownership;
- no localhost fallback.

### WP2 — Add daemon gateway types and dependency edge

Add `i2pr-app-proto` to `i2pr-daemon`, update dependency graph/checker, and add the narrow gateway API.

### WP3 — Compose private SAM/I2CP backends

Consume Plan-354 seams with lazy per-principal contexts and supervised connection tasks.

### WP4 — Prove principal/capability isolation

Add negative and multi-principal tests.

### WP5 — Document future-runtime handoff

Document exactly what a package/process runtime must prove before constructing gateway authorization and exactly what the gateway returns/owns.

## Failure, cancellation, restart, and contention semantics

- capability denial is side-effect free;
- unsupported `control_scoped` is side-effect free;
- backend protocol parse failure kills only that backend connection unless session-wide integrity is compromised;
- gateway-session cancellation cancels every owned connection and service context;
- one connection exhausting its bounded protocol resources does not grant more quota or mutate another principal;
- duplicate/open-limit exhaustion returns a typed resource-limit failure;
- no retries are automatic at the router gateway layer;
- daemon restart drops all gateway sessions; no persistent app grant state is introduced by this plan;
- capability/grant persistence is future AppManager ownership.

## Compatibility and migration

The managed-app protocol remains unreleased.

The added stream-to-protocol-connection clarification keeps v1 at `1.0` if the closure audit confirms no external consumer.

The gateway Rust API is internal infrastructure and may evolve under repository semver policy until a separately declared public SDK surface exists.

No SAM/I2CP wire version changes are allowed.

## Required tests

### Authorization

- SAM open with SAM capability succeeds;
- SAM open without SAM capability fails before state/task allocation;
- I2CP equivalent allow/deny;
- `control_scoped` returns typed unsupported and allocates nothing;
- requested-but-not-granted capability cannot authorize an open;
- effective capability set cannot contain reserved `BrokeredTcp` under existing v1 rules.

### Principal isolation

- two principals using identical SAM session ids remain independent;
- one principal cannot connect/accept against another principal's SAM destination/session;
- two principal I2CP contexts can reuse local connection/session numeric ids without collision;
- gateway state cannot inspect/attach ordinary listener-owned client sessions.

### Protocol composition

- authorized SAM private stream completes HELLO negotiation through the gateway;
- at least one SAM session/data path command reaches the existing implementation;
- managed SAM FORWARD remains denied;
- authorized I2CP private stream completes protocol byte + GetDate/SetDate path;
- malformed backend protocol produces bounded typed cleanup;
- no loopback listener must be enabled for any gateway test.

### Lifecycle/resources

- max+1 backend connection is rejected;
- cancellation closes all children;
- EOF removes connection ownership;
- repeated open/close returns counts to baseline;
- one backend failure does not corrupt sibling connection state.

### Static/boundary

- `i2pr-daemon` is the only new crate dependency on `i2pr-app-proto`;
- no app-proto/runtime boundary regression;
- no `TcpStream::connect` to SAM/I2CP loopback endpoints in `app_gateway`;
- no serde decoder for gateway authorization;
- no support inventory change.

## Exact verification commands

Focused:

```text
cargo fmt --all --check
cargo check --locked -p i2pr-app-proto -p i2pr-daemon --all-targets
cargo test --locked -p i2pr-app-proto --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon app_gateway -- --test-threads=1
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

Closure must also execute the full routine floor from `AGENTS.md`.

## Documentation updates

Required:
- `specs/references/managed-native-app-runtime-v1.md`;
- `docs/architecture/i2pr-app-proto.md`;
- daemon architecture documentation;
- `docs/architecture/dependency-graph.md`;
- `plans/subsystems/managed-native-app-runtime-roadmap.md`;
- `plans/registry.md`;
- `plans/closure/managed-native-app-runtime/355-status.md` at closure.

No `specs/support.toml` promotion is expected.

## Acceptance criteria

Plan 355 passes only when:

1. a trusted router-gateway authorization object binds exactly one `AppPrincipal` to immutable effective capabilities;
2. raw app-supplied Hello/identity bytes cannot construct that authorization;
3. SAM/I2CP service authorization is checked before any backend side effect;
4. one gateway session owns isolated private protocol contexts for one app launch instance;
5. cross-principal SAM/I2CP identifier attachment is impossible by construction and regression-tested;
6. authorized SAM/I2CP connections use Plan-354 private seams, never localhost fallback;
7. managed SAM FORWARD remains denied;
8. `control_scoped` remains unavailable pending its separate Proposal-170 adapter;
9. stream-to-protocol-connection byte semantics are frozen in the language-neutral reference;
10. canonical router-wide dependencies are reused rather than duplicated;
11. cancellation/resource ceilings are bounded and supervised;
12. existing SAM/I2CP listener behavior/support claims remain unchanged;
13. no package/process/sandbox/admin capability is claimed;
14. focused and full routine floors pass.

## Stop conditions

Stop and register a new architectural plan if:

- the gateway requires the future manager↔router OS IPC transport to be chosen now;
- correct principal isolation requires rewriting SAM/I2CP wire identifiers;
- a private service context cannot reuse canonical router owners without creating a second protocol/network stack;
- an application-visible compatibility change to managed-app v1 is required;
- `control_scoped` must be implemented to make SAM/I2CP gateway use viable;
- package/grant persistence becomes necessary for gateway correctness.

## Closure evidence required

The closure must include:

- spec-first commit freezing stream mapping/authority;
- authorization construction audit;
- capability allow/deny side-effect matrix;
- two-principal SAM isolation proof;
- two-principal I2CP isolation proof;
- private SAM/I2CP handshake evidence;
- managed FORWARD denial evidence inherited/rechecked from Plan 354;
- no-listener/no-loopback-fallback proof;
- resource/cancellation cleanup evidence;
- dependency graph diff;
- support inventory diff;
- full routine-floor results;
- unblock audit for the next managed-app runtime milestone.

## Handoff notes

Plan 355 is the router-side half of the managed-app foundation.

Do not turn it into the package/process manager. The future trusted runtime owns process identity proof, app-channel framing, permission-request workflow, persistence, sandboxing, and package lifecycle. It supplies already-authenticated principal/capability authority to this gateway.

Once this gateway is closed, a separate AppManager/package-lifecycle milestone can be planned against a concrete router service boundary rather than inventing its own SAM/I2CP integration.
