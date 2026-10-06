# Plan 369 — trusted application runtime/manager and apphost lifecycle foundation

Status: **in-progress-managed-app-runtime-manager-foundation-wp1-landed-no-gate**.

Classification: **infrastructure + process lifecycle + capability plumbing**.

Roadmap:
- plans/subsystems/managed-native-app-runtime-roadmap.md

Hard dependency:
- Plan 368 must close the private manager protocol and daemon bridge. **Met.**

Gating dependency:
- Corrective Plan 370
  (`plans/implementation/managed-native-app-runtime/370-managed-app-v1-hello-instance-id-codec-corrective.md`)
  gated **WP4 and acceptance criterion 7**. `AppToHostMessage::Hello`
  encoded and then failed to decode for every `AppInstanceId`, because a
  `u128` field inside a serde internally tagged enum cannot be reconstructed from
  serde's `Content` buffer, so §10 hello-first/exactly-once matching and criterion
  7 could not be satisfied or evidenced. **Plan 370 has closed**
  (`plans/closure/managed-native-app-runtime/370-status.md`): `AppInstanceId` is
  now a bounded canonical decimal-digit string and `hello` decodes at every valid
  value, with every non-canonical spelling still failing closed. **This plan is
  unblocked for every work package — WP2, WP3, WP4, WP5, and WP6.**

Interface dependencies:
- Plans 354/355 private SAM/I2CP gateway;
- ADR 0032 managed-app process/capability boundary;
- Plan 368 manager protocol/authority ADR and reference.

This plan does not require Proposal 170 completion.

## Objective

Add the first trusted application-runtime processes:

- i2pr-appd: trusted application runtime/manager;
- i2pr-apphost: minimal per-launch process bootstrap/host.

Wire i2pr-daemon to supervise i2pr-appd through a private inherited capability transport, and prove one fixture native application can:

1. be launched through i2pr-apphost;
2. bind its declared hello to manager-created launch authority;
3. receive immutable effective capabilities;
4. open SAM/I2CP logical streams through the Plan-368 daemon bridge and Plan-355 gateway;
5. exchange exact protocol bytes;
6. close/fail without leaking router resources or affecting sibling applications.

This is a lifecycle/runtime foundation only. It does not establish a secure sandbox, package trust, persistent grants, user-facing AppManager administration, brokered clearnet, UI hosting, or a third-party-app security claim.

## Why this plan is ready after Plan 368

Plan 355 closed the router authorization boundary but deliberately left app_gateway.rs without a production runtime caller.

Plan 368 turns that Rust-private boundary into a bounded private manager protocol with daemon-side ownership. Once that closes, the application runtime can be a separate process without linking router internals into the manager.

The remaining work is process ownership and application-channel execution.

## Target process architecture

    i2pr-daemon
       |
       | supervised child; inherited anonymous control/data transport
       v
    i2pr-appd
       |
       | one supervised i2pr-apphost per app launch
       v
    i2pr-apphost
       |
       | managed-app v1 over reserved child stdin/stdout
       v
    native app

Router service traffic follows:

    native app
      -> app v1 logical SAM/I2CP stream
      -> i2pr-appd
      -> Plan-368 private manager stream
      -> i2pr-daemon
      -> AppGatewaySession
      -> existing private SAM/I2CP driver

No process in this chain connects to a SAM/I2CP localhost port.

## Architecture decisions frozen by this plan

### 1. i2pr-appd is a separate trusted process

It owns:
- application launch-instance identity;
- outer managed-app v1 handshake/frame/session state;
- mapping app logical stream ids to daemon manager-protocol service handles;
- process lifecycle for apphost children;
- effective-capability presentation to apps;
- app health/termination state;
- bounded stderr/log drain;
- future attachment points for package/grant/policy owners.

It does not own router protocol internals, NetDB/tunnel/transport state, Proposal-170 administrator credentials, package signatures/store in this plan, or sandbox enforcement in this plan.

### 2. i2pr-apphost is a tiny per-launch bootstrap process

It exists to keep OS launch/sandbox setup out of the larger long-lived manager.

Plan 369 establishes the process role and bootstrap protocol, but no secured launch is allowed. LaunchProfile::Secured must fail closed before app exec because no qualified sandbox backend exists yet.

No code may fabricate a passing SandboxAttestation for this foundation.

Later Linux/Windows/macOS plans extend apphost launch preparation with qualified backends.

### 3. Daemon↔manager transport is inherited and anonymous

The daemon launches the distribution-matched sibling i2pr-appd binary directly, never via a shell or PATH lookup, and gives it an anonymous inherited full-duplex capability transport.

For the portable foundation, the concrete transport may be represented as two inherited anonymous pipes (daemon→manager and manager→daemon) while exposing one logical duplex byte stream to the Plan-368 codec.

The manager transport must not be TCP, loopback, a filesystem-discoverable Unix socket, a globally named pipe endpoint, or a user-configurable arbitrary command.

The daemon is the parent/owner. Possession of the inherited pipe ends is the manager-process authentication fact.

### 4. Manager process location is distribution-owned

When the feature is enabled, the daemon resolves i2pr-appd as a sibling of its own executable, with platform suffix rules applied.

No config field may select an arbitrary manager executable.

Tests may inject a fixture path through test-only composition seams; production config may not.

### 5. Manager service is disabled by default and restartable

Add an application-runtime activation switch, disabled by default.

When enabled:
- daemon starts i2pr-appd as a Restartable service;
- use the existing bounded RestartPolicy;
- exhaustion degrades/disables the app-runtime feature rather than shutting down the router;
- manager failure/EOF tears down every Plan-368 manager session before restart;
- no application state is restored automatically in Plan 369 because persistence is not yet owned.

The router must remain fully usable when the app runtime is disabled, absent, or degraded.

### 6. i2pr-appd and i2pr-apphost are a separate runtime trust zone

Add explicit dependency/runtime checks.

Expected dependency direction:

    i2pr-appd
      -> i2pr-app-proto
      -> i2pr-app-manager-proto
      -> reviewed runtime/process/fs dependencies

    i2pr-apphost
      -> i2pr-app-proto and/or narrow manager-host bootstrap contract
      -> reviewed OS/process primitives

Neither may depend on i2pr-daemon, i2pr-runtime, i2pr-client, i2pr-api, i2pr-i2pcontrol, NetDB, tunnel, or transport crates.

The existing router runtime boundary checker must be amended explicitly rather than bypassed through workspace dependency syntax. Add negative tests proving unauthorized crates still cannot gain Tokio/process ownership.

### 7. AppManager↔apphost bootstrap is a one-way authority handoff

Extend the runtime-neutral internal protocol with a distinct apphost bootstrap handshake, or add an equally narrow runtime-neutral contract module.

The trusted manager sends exactly one bounded launch request containing:
- AppPrincipal;
- selected LaunchProfile;
- prevalidated application root;
- package-relative executable entrypoint;
- bounded argv;
- bounded sanitized environment entries;
- resource request/ceiling values that are descriptive only until sandbox plans enforce them.

Apphost replies with a typed ready/failure result.

After successful bootstrap, the channel transitions to byte-transparent managed-app protocol proxying. Apphost no longer interprets app protocol bytes.

No shell string, command interpreter, script hook, or arbitrary environment expansion is allowed.

### 8. App process managed protocol uses reserved stdin/stdout

For the v1 native-process foundation:
- app stdin receives host→app managed-app protocol bytes;
- app stdout carries app→host managed-app protocol bytes;
- app stderr is application diagnostics only.

Stdout is protocol-owned and arbitrary app logging to stdout is invalid.

This is language-neutral and works across supported platforms without a discoverable local IPC endpoint.

A future SDK may wrap these streams but may not change v1 wire semantics.

### 9. Launch authority is manager-created, never app-created

Introduce a trusted manager-side type conceptually containing:
- principal;
- effective capabilities;
- launch profile;
- entrypoint/root;
- limits.

It must have no decoder from app v1 messages or manifest bytes.

Plan 369 has no package/grant owner, so production external input cannot create one. Integration tests may construct fixture authorities through a test/support seam. A later package/AppManager plan becomes the production constructor.

### 10. Hello is declaration matching, not authentication

Appd generates the AppInstanceId before launch.

After apphost reports exec success, appd expects:
1. application-role handshake;
2. exactly one hello as the first request;
3. exact AppId, AppInstanceId, and protocol version match.

Mismatch, duplicate hello, data-before-hello, or wrong role terminates the launch and closes router authority.

The authentication fact is that appd/apphost created the process and handed that launch's private stdio channel to it. The hello only confirms the process is speaking for the identity appd already selected.

### 11. Permission requests cannot raise authority

Until a persistent administrator/grant owner exists:
- permission_request receives deterministic denied;
- it causes no grant/persistence/policy mutation;
- effective capabilities cannot change in place.

brokered_tcp, ui_bridge, lifecycle administration, and control_scoped remain unavailable unless separately implemented and granted by later plans.

### 12. Process lifecycle is bounded but not yet sandboxed

Appd owns each direct apphost child and each apphost owns one direct application child.

Required:
- bounded start timeout;
- bounded hello timeout;
- bounded graceful close;
- forced direct-child kill after deadline;
- exit status capture;
- direct child handles retained;
- no detached process;
- no automatic restart of app launches in Plan 369;
- app stderr drained with bounded memory/rate/truncation accounting.

Important non-guarantee: without the later OS sandbox/process-tree backend, Plan 369 cannot prove containment of grandchildren or direct networking. Secured therefore fails closed.

## Invariants that must not regress

1. Router stays functional if app runtime is disabled or broken.
2. i2pr-appd cannot import router internals.
3. Apps cannot connect directly to router listeners through the managed path.
4. App hello/requested permissions cannot create effective authority.
5. One app instance cannot attach to sibling runtime/router streams.
6. control_scoped stays unavailable.
7. Secured launch is rejected until a qualified backend exists.
8. No fake sandbox attestation.
9. No arbitrary executable path for daemon→appd.
10. No shell execution for manager/apphost/app launch.
11. Every child, request, stream, frame, diagnostic buffer and timeout is bounded.
12. Manager/app failure does not become router availability failure.
13. No package/signature/persistence claim.
14. No router support/advertisement promotion.

## Scope

### In scope

- i2pr-appd workspace crate/binary;
- i2pr-apphost workspace crate/binary;
- explicit runtime/dependency-zone checks for both;
- daemon application-runtime config activation flag, default off;
- daemon supervised/restartable appd child;
- inherited anonymous daemon↔appd transport;
- appd Plan-368 protocol client;
- appd launch/session state machine;
- apphost typed bootstrap and direct child exec;
- app managed-app v1 over stdin/stdout;
- exact hello/principal matching;
- app SAM/I2CP open/data/close through daemon bridge;
- bounded stderr capture/drain;
- cancellation/EOF/child-exit cleanup;
- fixture native app and black-box lifecycle tests.

### Out of scope

- package signatures/archive format;
- package install/update/uninstall;
- publisher trust store;
- persistent grants/policy;
- normal user-facing launch API;
- autostart/restart-request semantics;
- secure filesystem boundary;
- direct-network denial;
- process-tree containment;
- Landlock/seccomp/AppContainer/App Sandbox;
- brokered clearnet/DNS;
- Proposal-170 adapter;
- UI bridge;
- production third-party application security claim.

## Required production changes

### A. Add explicit app-runtime trust-zone crates

Expected:
- crates/i2pr-appd/
- crates/i2pr-apphost/

i2pr-appd may be library + binary so integration tests can exercise lifecycle state directly.

Update dependency graph and runtime boundary guards with an explicit allowlist. Do not rely on checker blind spots.

### B. Add daemon manager-process supervisor

Add a daemon service that:
- exists only when app runtime is enabled;
- locates sibling i2pr-appd;
- creates inherited anonymous pipes;
- spawns without shell/PATH resolution;
- closes all unrelated child descriptors/handles;
- performs manager-protocol handshake;
- marks readiness only after handshake;
- drives the Plan-368 bridge;
- maps manager process exit to typed service failure;
- uses bounded restart/backoff with degrade-on-exhaustion;
- terminates the child during daemon shutdown.

### C. Add appd runtime state machine

At minimum:

    Starting
      -> ConnectedToRouter
      -> Idle
      -> Launching(instance)
      -> AwaitingHello(instance)
      -> Running(instance)
      -> Stopping(instance)
      -> Closed(instance)

Multiple instances may exist concurrently within a bounded global limit.

Invalid transitions fail closed.

### D. Add narrow apphost bootstrap

Appd resolves sibling i2pr-apphost, creates private pipes, and spawns directly.

Apphost:
1. decodes one bounded bootstrap request;
2. rejects Secured;
3. validates root/entrypoint relationship;
4. spawns the exact application executable with piped stdin/stdout/stderr;
5. returns ready/failure;
6. transitions to transparent app-protocol proxy;
7. owns the direct child until exit/shutdown.

No PATH lookup or shell.

### E. Bind application protocol session

Appd:
- validates the application handshake;
- enforces hello-first/exactly-once;
- checks app/instance/version against launch authority;
- sends effective capabilities;
- rejects permission escalation;
- maintains SessionLimits;
- maps open SAM/I2CP to manager bridge;
- forwards exact data frames;
- maps backend close/reset;
- closes all streams on app EOF/exit.

### F. Bounded diagnostics

App stderr must never grow unbounded.

Implement:
- bounded line/chunk size;
- bounded retained ring/snapshot;
- byte/rate ceiling or deterministic truncation/drop accounting;
- no interpretation as control;
- no secret-bearing full dump in ordinary logs.

### G. Fixture application

Add a purpose-built native fixture that can deterministically:
- send correct/wrong hello;
- request permitted/denied capability;
- open SAM;
- open I2CP;
- emit malformed/oversized frames;
- close early;
- hang during shutdown;
- flood stderr;
- attempt sibling stream-id misuse.

The fixture is evidence tooling, not a shipped application.

## Work packages

### WP1 — trust-zone and process-transport freeze
Update architecture docs/checkers and freeze inherited-pipe/stdin-stdout semantics before production wiring.

### WP2 — daemon↔appd supervision
Land appd binary, restartable daemon service, anonymous pipe transport, manager handshake/readiness/shutdown.

### WP3 — apphost and direct process lifecycle
Land apphost bootstrap, direct application exec, timeouts, stderr drain and direct-child cleanup. Secured remains fail-closed.

### WP4 — app v1 runtime consumer
Implement hello/capability/session limits and SAM/I2CP mapping through Plan 368.

### WP5 — black-box fixture qualification
Exercise daemon→appd→apphost→fixture→SAM/I2CP and failure/restart paths.

### WP6 — docs/checkers/closure
Update dependency graph, security model, roadmap/registry and close with no sandbox/package/support claim.

## Failure, cancellation, restart, and contention semantics

### Manager process
- appd crash/EOF tears down all manager sessions in daemon;
- daemon restart policy may restart appd after bounded backoff;
- restart starts empty; no app or grant recovery is claimed;
- exhaustion degrades app runtime only.

### Apphost/application
- launch timeout kills direct apphost child;
- hello timeout closes app and any provisional router session;
- apphost/app EOF closes all logical streams;
- sibling app instances remain alive;
- graceful stop has a bounded deadline, then direct child kill;
- no grandchild containment claim;
- app auto-restart is disabled in this plan.

### Resource contention
- global app instance ceiling;
- per-instance v1 128-stream ceiling;
- Plan-368 and Plan-355 backend ceilings remain additional independent checks;
- bounded pending launches;
- bounded stderr retention;
- no unbounded process queues.

## Compatibility and migration

Managed-app v1 remains unreleased.

Plan 369 becomes its first actual runtime consumer but does not promote it to a stable external SDK guarantee.

The daemon manager protocol remains internal/private.

No existing router configuration changes except a new disabled-by-default app-runtime section/switch.

No support advertisement changes.

## Required tests

### Daemon↔appd
- disabled config creates no process;
- enabled config launches only sibling appd;
- missing/invalid appd degrades app runtime, router remains healthy;
- manager handshake required before readiness;
- wrong manager magic/version rejected;
- EOF tears down daemon gateway state;
- restart/backoff bounded;
- shutdown reaps direct manager child.

### Apphost
- no shell/PATH launch;
- traversal/out-of-root entrypoint rejected;
- Secured rejected before child exec;
- fixture launch succeeds only through trusted prevalidated authority;
- bootstrap is one-shot then byte-transparent;
- malformed bootstrap/oversize rejected;
- direct child killed on timeout;
- stderr flood is bounded.

### App session
- hello must be first and exactly once;
- identity/version mismatch kills launch;
- effective capability message matches prevalidated authority;
- permission request is denied/no mutation;
- SAM/I2CP open requires effective capability;
- data-before-open rejected;
- duplicate stream/request max+1 rejected;
- backend EOF closes only mapped stream;
- app EOF tears every backend stream down.

### Multi-instance
- two fixture apps with identical SAM session ids remain isolated;
- one app cannot use another's logical stream id/manager handle;
- one app crash does not terminate sibling;
- manager crash terminates all descendant router authority before restart.

### End-to-end
At least one black-box path each:
- fixture → managed SAM HELLO/session path through private daemon gateway;
- fixture → managed I2CP protocol-byte/GetDate path;
- neither requires loopback listeners enabled.

## Exact verification commands

Focused:

    cargo fmt --all --check
    cargo check --locked -p i2pr-appd -p i2pr-apphost -p i2pr-daemon --all-targets
    cargo test --locked -p i2pr-appd --all-targets -- --test-threads=1
    cargo test --locked -p i2pr-apphost --all-targets -- --test-threads=1
    cargo test --locked -p i2pr-daemon app_manager -- --test-threads=1
    cargo test --locked -p i2pr-daemon app_gateway -- --test-threads=1
    cargo clippy --locked -p i2pr-appd -p i2pr-apphost -p i2pr-daemon --all-targets --all-features -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-appd -p i2pr-apphost -p i2pr-daemon --no-deps
    bash scripts/check-dependency-direction.sh
    bash scripts/check-runtime-boundaries.sh
    python3 scripts/check-managed-app-private-client-seams.py
    python3 scripts/check-managed-app-gateway-boundary.py
    python3 scripts/check-managed-app-manager-boundary.py
    python3 scripts/check-managed-app-process-boundary.py
    python3 scripts/check-global-plan-number-uniqueness.py
    python3 scripts/check-adr-number-uniqueness.py
    python3 -m unittest discover -s tests/planning -p 'test_*.py'

Run the fixture black-box lifecycle script registered by this plan, then the complete current routine floor from AGENTS.md.

## Documentation updates

Required:
- managed app/runtime architecture;
- daemon architecture;
- dependency graph;
- security model;
- managed-app v1 reference if stdin/stdout transport wording needs normative clarification;
- Plan-368 manager protocol reference for concrete inherited transport binding;
- managed-app roadmap;
- registry;
- closure record.

Do not claim secured containment.

## Acceptance criteria

Plan 369 passes only when:

1. i2pr-appd is a separately supervised trusted process;
2. daemon↔appd uses only anonymous inherited transport with no discoverable endpoint;
3. manager failure degrades only app runtime and bounded restart behavior is proven;
4. i2pr-apphost is the only component that directly execs app fixture processes;
5. Secured fails before exec because no backend is qualified;
6. app stdin/stdout carry managed-app v1 and stderr is bounded diagnostics;
7. hello identity exactly matches manager-created launch identity;
8. permission requests cannot change effective capabilities;
9. fixture SAM/I2CP traffic traverses Plan 368→355 without localhost listeners;
10. sibling app isolation/crash behavior is proven;
11. direct child cleanup and manager restart cleanup are bounded;
12. no package/grant persistence/admin/broker/UI/Proposal170 implementation lands;
13. runtime/dependency guards explicitly enforce the new trust zones;
14. no protocol support/advertisement promotion occurs;
15. focused and full routine floors pass.

## Stop conditions

Stop and register a new plan if:

- cross-platform anonymous child transport requires a discoverable endpoint;
- apphost needs a real sandbox backend to safely exercise the foundation;
- production launch requires package/signature/grant persistence;
- process-tree containment is needed for a claimed acceptance result;
- daemon must expose a general admin/control credential to appd;
- router-core crates must import app-runtime implementation crates;
- existing supervisor semantics cannot own/restart the manager without a new generic process-supervision substrate.

## Closure evidence required

The closure must include:
- process/authority diagram;
- exact executable-resolution rules;
- manager inherited-transport evidence;
- restart/degrade evidence;
- apphost Secured fail-closed evidence;
- hello/identity mismatch negative cases;
- fixture SAM and I2CP black-box transcripts;
- sibling-isolation and cleanup evidence;
- stderr/resource ceiling evidence;
- dependency/runtime guard mutation tests;
- support/config diff;
- full routine-floor results;
- unblock audit for the later package trust/local lifecycle milestone.

## Handoff notes

Do not add package installation, persistent grants, real sandbox policy, clearnet brokering, UI hosting, or Proposal-170 in this plan.

The next milestone after Plan 369 should be package trust and restart-safe local lifecycle: immutable versions, publisher-key identity, signed manifest/file inventory, transactional install/update/uninstall, grant persistence, and the production constructor for prevalidated launch authority.
