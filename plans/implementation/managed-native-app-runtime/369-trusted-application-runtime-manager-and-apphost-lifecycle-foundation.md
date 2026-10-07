# Plan 369 — trusted application runtime/manager and apphost lifecycle foundation

Status: **passed-trusted-application-runtime-manager-and-apphost-lifecycle-foundation**.

Closure: `plans/closure/managed-native-app-runtime/369-status.md`.
Work packages landed: **WP1, WP2, WP3, WP4, WP5, WP6**.

## WP6 outcome — guards, docs, floor, closure

WP6 adds `scripts/check-managed-app-process-boundary.py` and its `--self-test`,
registers the fixture crate in every dependency/console/runtime map, wires the
new checker into the `AGENTS.md` floor and CI, writes the documentation set,
runs the complete routine floor, performs the unblock audit, and records
everything in `plans/closure/managed-native-app-runtime/369-status.md`.

The documentation pass closed a real drift finding rather than only adding
pages: `docs/architecture/i2pr-daemon.md` still described Plan 368's bridge as
having "no production caller" and its module list predated Plans 368 and 369
entirely. Two of the three crates Plan 369 introduced — `i2pr-appd` and
`i2pr-apphost` — had **no per-crate deep dive at all**, and `i2pr-app-fixture`
did not exist as one either; `AGENTS.md` promises one per workspace member.

The Plan-368 manager reference gains a normative **§3.1** for the concrete
inherited binding Plan 368 deliberately left open, and the managed-app v1
reference gains the normative `hello` identity-binding clarification its §"A
future trusted transport owner must bind that claim" sentence had been waiting
for since Plan 345.

## WP5 outcome — the first run of the whole chain, and the defect only a real process could find

WP5 adds the purpose-built native fixture application and drives the **entire**
chain black-box: `i2pr-daemon` → `i2pr-appd` → `i2pr-apphost` → the fixture
application → real SAM and I2CP backends.

### The product defect: the manager link never flushed

`manager_link::writer_task` wrote each frame with `write_all` and **never
flushed**. The manager's write half is an anonymous pipe whose daemon end is
`tokio::io::Stdout`, which is *line-buffered*; a length-prefixed control frame
contains no newline, so every manager→daemon frame sat in the buffer until
process exit. The effect was total: **every `CreateSession` timed out** with
`AppdError::ManagerTimeout`, and no launch could ever have succeeded.

The handshake escaped only because `Appd::write_handshake` flushes on its own.
This is the failure mode that in-memory testing structurally cannot see — every
WP2–WP4 test drives a `tokio::io::duplex` transport, which is unbuffered and
therefore already "flushed". The defect was invisible until a real process with a
real pipe was on the other end of the link. `writer_task` now flushes per frame,
and `every_frame_is_flushed_without_waiting_for_a_newline` asserts it directly
with a `FlushCounter` writer, so the regression fails the moment the flush is
removed rather than inferring it from a timeout.

### Why the fixture has two binaries, and why the manager is not a flag

`crates/i2pr-app-fixture` contains both the application and a **fixture manager**.
The manager is a separate binary because the shipped `i2pr-appd` refuses all
arguments (WP3), and adding a flag would reopen precisely the hole Plan 369 §4
closes: "the operator can make the router exec an arbitrary file". The fixture
manager runs the **real** `i2pr_appd::Appd::with_catalog(FixtureCatalog)` over the
**real** `i2pr_appd::inherited()` transport and gates every authority through the
real `LaunchAuthority::new`, so what is qualified is the product manager, not a
stand-in. The application binary depends only on the wire contracts, so it is a
genuinely separate program with no route into router state.

### Black-box qualification — 17 tests, ~2 minutes serially

`crates/i2pr-daemon/src/app_runtime_qualification.rs` is a `#[cfg(test)]` module
(in-crate because `AppManagerBridge` is `pub(crate)`). It spawns the real fixture
manager over real `std::io::pipe()` anonymous pipes, then drives the real
`AppManagerBridge`, the real `AppGatewaySession`, and the real SAM/I2CP
backends. It deliberately does **not** reuse `run_manager_service`, which spawns
with no arguments and a cleared environment and so cannot select a scenario.

Coverage: SAM and I2CP happy paths; denied capability; denied service;
wrong-identity hello; frame-before-hello; oversized frame; malformed frame; early
close; shutdown hang; stderr flood; data-before-open; foreign stream id;
duplicate stream id; two-instance isolation; duplicate SAM session id; and
"the shipped manager cannot name the fixture".

Two harness hazards are worth recording. **Stale siblings:** `cargo test -p
i2pr-daemon` does not rebuild `target/debug/i2pr-app-fixture-manager` or
`i2pr-apphost`, so a stale binary silently hides product changes — this is how
the flush defect survived several iterations. The harness now asserts binary
freshness against per-binary source lists (`assert_fresh`) rather than trusting
the caller. **Teardown:** `AppManagerBridge::cancel()` alone does *not* end the
read loop, and the manager's only exit signal is EOF on a write half the daemon
holds — so it is always killed. Teardown mirrors production `terminate_manager`
(cancel, drop/abort the bridge future, bounded 5 s grace, then `start_kill` and
reap), and assertions say "terminated and reaped", never "exited cleanly".

### The process-boundary checker, and two guards it made honest

`scripts/check-managed-app-process-boundary.py` polices the property the
crate-edge scripts cannot see: **which process execs what**. There are exactly
three blessed process edges (daemon→manager, appd→apphost, apphost→application);
the two distribution-owned ones resolve via `current_exe()`, never a shell and
never a `PATH` lookup; no production source may name the fixture; no crate may
depend on it; the shipped manager must refuse argv and must not reach
`with_catalog`.

Two guard weaknesses were found by its own controls and corrected rather than
documented around:

- **Rule 3 asserted a token, not a behaviour.** `"args_os" not in shipped`
  passes a manager that reads argv, prints it, and carries on — which is exactly
  the "silently tolerates arguments" failure the rule exists to prevent. It now
  asserts the *shape*: an argv guard whose body returns. The structural limit is
  stated in the script and recorded here: this cannot prove the guard's condition
  is always true.
- **A negative control that replaced whole files.** `expect_accepted` originally
  overwrote a real file, deleting the production `current_exe()` lookup so rule 1b
  fired; the control then passed by filtering on its own label while the tree it
  scanned was nonsense. Controls now append, so each tests exactly one thing.

### A coverage hole in `check-dependency-direction.sh`

Negative mutation N11 removed the `i2pr-app-fixture` map entry and the script
still printed `dependency direction: ok`. Its loop iterates the **map**, so a
member with no entry is invisible — deleting an entry makes that crate's
forbidden edges unreported rather than reported. `check-console-boundaries.sh`
rule 7 asserted the same set, but the check belongs in the script that owns the
map; a reader running only the dependency check was getting a false all-clear.
That script now fails closed on unmapped members, and both checks pass.

### Evidence

**12 mutations, all caught** (N1 by a named unit test, N2–N10 by the process
boundary checker, N11–N12 by the owning guard maps). N8 originally missed, which
is what exposed the rule-3 weakness above; the mutation was then rewritten to the
realistic shape (read argv, announce, carry on) and the strengthened rule catches
it.

## WP4 outcome — the app v1 runtime consumer, and a direction defect it exposed

WP4 adds the managed-app v1 consumer that WP2 deliberately lacked: launch
authority, the manager-protocol client, the bounded session registry, and the
session that speaks v1 to an application and maps its logical stream ids onto
daemon manager-protocol handles.

### The defect WP4 found in WP2: the manager protocol was read backwards

WP2's manager loop decoded inbound frames with
`decode_manager_to_daemon_control`, and WP2's test peer **also sent
manager-direction frames**. The two mistakes cancelled, so the test asserted the
inverted contract and passed.

The two control vocabularies share no tags, so a direction mistake is not a
mis-parse — it is `InvalidControl`. Against the real Plan-368 bridge, the first
reply the manager read would have ended the transport, and nothing before that
point would have revealed it. This is the most expensive mistake available in
this protocol: it is invisible to every test that shares the implementation's
assumption.

`manager_link` therefore decodes inbound frames as `DaemonToManagerMessage`, and
`tests/manager_contract.rs` now drives its peer in the **daemon's** direction.
`inbound_decodes_daemon_to_manager_vocabulary` and
`manager_direction_bytes_are_not_daemon_bytes` are the regression guards. The
manager also now **fails closed on any correlated reply for a request it never
sent**, which is the property a confused or hostile daemon would violate.

### What else WP4 adds

- **`authority`** — `LaunchAuthority` and `AuthorityRequest`, with private
  fields and **no decoder**. Effective capabilities can only be assembled
  through `GrantedCapability::from_administrator_policy`, so `BrokeredTcp` is
  ungrantable, and there is no `&mut` path to the capability set at all. Every
  gate that can be evaluated without a filesystem runs here, including the
  `Secured` refusal, so no authority value can exist for a launch apphost would
  refuse at exec time.
- **`catalog`** — the trusted source of authority. The shipped `i2pr-appd` owns
  `EmptyCatalog` and therefore launches nothing. There is **no**
  manager-receivable launch request in the protocol, so the daemon cannot ask
  either: the absence of a launch path is structural, not a policy.
- **`manager_link`** — the concurrent manager client: one reader, one writer, a
  single-writer queue, a bounded in-flight ledger, per-session routing. The
  reader performs route registration *before* delivering a reply, which removes
  the window in which a notification emitted between "reply delivered" and
  "caller registered its route" would be dropped.
- **`session`** — greeting, hello-first/exactly-once with an exact identity
  match, immutable capability presentation, deterministic permission denial,
  `SessionLimits`, SAM/I2CP mapping, exact-octet forwarding in both directions,
  backend close/reset mapping, and EOF teardown that releases the daemon session.
- **`runtime`** — the bounded instance registry and launch pipeline. The global
  ceiling is `MAX_MANAGER_SESSIONS`, the same constant the daemon enforces,
  rather than a third independently chosen number.

### Evidence

**15 mutations, all caught by a named test** (M1–M14 failing assertions; M15 a
build failure). M15 is the point worth reading twice: the authority seal is
asserted by **method resolution**, not by scanning for `#[derive(Deserialize)]`,
so adding a derive makes the crate stop compiling rather than merely failing a
test. Writing that probe took two attempts and **both earlier attempts were
vacuous** — an associated const is resolved to the trait fallback rather than
the inherent impl, and behind a generic helper function the receiver type is
unresolved so rustc must pick the candidate valid for every `T`. Both produced a
file that compiled, passed, and proved nothing. The positive control
(`the_probe_must_be_able_to_detect_a_real_decoder`) is what exposed them and is
kept for that reason.

### A test-harness defect found and fixed

WP4's negative-evidence harness restored mutated files with `shutil.copy2`,
which **preserves mtime**. Cargo fingerprints on mtime, so the run after a
mutation was tested against the *mutated* build. This surfaced as two session
tests failing with exactly M14's signature; re-applying M14 by hand reproduced
those two failures exactly, and restoring with a fresh mtime turned them green
again. The harness now stamps restored files. Worth recording because a negative
suite that cannot trust its own restores will eventually report a phantom
product defect — or hide a real one.

### `ETXTBSY` in the launcher tests

The apphost-launcher tests write a `#!/bin/sh` stub into `/tmp` and exec it
milliseconds later. On this host that fails roughly one spawn in fifty with
`ETXTBSY`, reproducibly, and **only** when several tests spawn concurrently. It
was reduced rather than papered over: it does not occur under
`--test-threads=1` (0/10), does not occur spawning a pre-existing binary
(0/320), does not occur from the same write-then-exec pattern driven from Python
(0/1740), is unaffected by `fsync` or by renaming the file into place, and
scanning `/proc` at the moment of failure finds no process holding the file
open. The mechanism is therefore an OS/kernel-level transient around exec of a
very recently written file, and the fixture materialises by rename and tolerates
that one errno with a bounded retry. The retry is in the **fixture, not in
`launch_apphost`**: in production the apphost is a distribution-owned sibling
that is installed rather than written-then-exec'd, so the condition cannot arise
there, and a retry in product code would be hiding something that cannot happen.

## WP2 outcome — and a stop condition this plan already predicted

WP2 landed the `i2pr-appd` crate, the inherited anonymous transport, and the
daemon supervisor. Building it surfaced the **Stop condition** this plan
already listed:

> existing supervisor semantics cannot own/restart the manager without a new
> generic process-supervision substrate.

`Supervisor::run` awaits initial readiness for **every** service in
`startup_order()` and returns `SupervisorError::StartupFailed` for any that
never signals it — *regardless of classification*. The `RestartExhaustion::Degrade`
path is reachable only for a service that fails **after** startup. So a manager
that is spawned and then rejected (wrong greeting, immediate crash loop) takes
the **whole router's startup down**, which contradicts this plan's invariant 1
("Router stays functional if app runtime is disabled or broken") and §5
("exhaustion degrades/disables the app-runtime feature rather than shutting
down the router").

This is not fixed inside Plan 369, because the fix is a generic `i2pr-runtime`
substrate change. It was registered as **corrective Plan 371**
(`plans/implementation/managed-native-app-runtime/371-optional-non-blocking-service-startup-corrective.md`)
and **Plan 371 has now passed**
(`plans/closure/managed-native-app-runtime/371-status.md`): the `app-runtime`
service is registered `StartupRequirement::Optional`, so a manager that is
spawned and then rejected degrades the app runtime and the router starts.
WP3–WP6 are unblocked.

### What WP2 does about it in the meantime

WP2 refuses to configure the router into a state it cannot start:

- the composition root **preflights** the sibling manager and returns an
  actionable `DaemonError` when it cannot be resolved, so a missing
  `i2pr-appd` is a configuration error rather than an opaque startup failure;
- the post-spawn rejection path was left explicitly unimplemented-safe and was
  asserted as the **current** behaviour by
  `crates/i2pr-daemon/tests/app_runtime_supervision.rs`
  (`a_manager_that_sends_the_wrong_magic_never_becomes_ready`). As that test's
  doc comment required, Plan 371 **replaced** it with
  `a_manager_that_sends_the_wrong_magic_degrades_the_feature_not_the_router`,
  which asserts the degradation rather than the gap.

The composition preflight is **retained**, and its original justification is no
longer true. The surviving reason is stronger: a *present but broken* manager
should degrade, whereas an operator who enabled a feature without installing its
manager should get an actionable configuration error rather than a router that
silently runs without the feature they asked for.

### Defects found and corrected inside WP2

1. **Readiness deadlock (production).** The service pinned the bridge future
   and then awaited the handshake channel without ever polling that future, so
   the handshake could never be performed and every startup hit the 30 s
   readiness deadline. Fixed by polling the bridge inside the same `select!`.
2. **Unbounded stderr-drain lifetime (production).** The drain ran to EOF, but
   Plan 369 §12 makes grandchild containment an explicit non-guarantee — a
   surviving grandchild inherits the stderr write end, so the drain never saw
   EOF and the service could not finish shutting down. The drain is now
   **cancellable** in addition to byte-bounded, with `truncated_by_cancel`
   reported so the truncation stays observable.
3. **Excessive reap grace on failure paths.** A manager that already failed the
   protocol was still given the full graceful-exit grace. Split into
   `MANAGER_EXIT_GRACE` (cancellation) and `MANAGER_REAP_GRACE` (already-broken
   transport).
4. **Reverted regression:** adding the handshake signal to
   `AppManagerBridge::run` initially dropped `teardown_all()`, breaking
   `manager_transport_eof_tears_down_every_gateway_session`. Caught by the
   existing bridge tests and fixed.

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
