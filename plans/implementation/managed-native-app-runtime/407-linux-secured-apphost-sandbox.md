# Plan 407 — Linux Secured managed-app sandbox and private data root

Status: **passed-linux-secured-apphost-sandbox-and-private-persistent-app-data**

Global number reconciliation: the source branch called this Plan 385, but Plans 385–388 are already owned by Proposal 170 on `main`. This proposal is Plan 407. It consolidates the former managed-app Plans 385 and 386; those drafts, plus the deferred ingress and SDK drafts 387 and 388, are preserved under `plans/archive/managed-native-app-runtime/`.

Classification: **invariant + capability + platform security**

Repository baseline: `main` at `928842f9` (Plan 375 merge); proposal reconciled from `plans/385-linux-secured-app-sandbox`.

Owning subsystem: `plans/subsystems/managed-native-app-runtime-roadmap.md`.

Hard predecessors:

- Managed native app runtime/368 — trusted AppManager bridge and private inherited manager protocol — closed.
- Managed native app runtime/369 — supervised `i2pr-appd` + `i2pr-apphost` + managed-app v1 consumer — closed.
- Managed native app runtime/370 — managed-app v1 instance-id wire corrective — closed.
- Managed native app runtime/371 — optional/non-blocking runtime startup corrective — closed.
- Managed native app runtime/382 — signed package/immutable store foundation — closed.
- Managed native app runtime/383 — persistent trust/grants/selection/profile/autostart and production catalog — closed.

Canonical constraints:

- `GUARDRAILS.md`
- `docs/adr/0032-managed-native-app-process-and-capability-boundary.md`
- `docs/adr/0035-private-manager-protocol-and-inherited-authority.md`
- `docs/adr/0036-managed-app-signed-package-and-immutable-store.md`
- `docs/adr/0037-managed-app-persistent-policy-and-production-catalog.md`
- `specs/references/managed-native-app-runtime-v1.md`
- `specs/references/managed-app-manager-protocol-v1.md`
- `specs/references/managed-app-policy-v1.md`

## Objective

Turn `LaunchProfile::Secured` from an intentional fail-closed placeholder into one
real, qualified **Linux** launch backend while preserving the existing trust/process
boundaries.

A successful secured launch must establish every property already required by
`SandboxAttestation::validate_secured()`:

1. direct network denied;
2. loopback denied;
3. private filesystem boundary;
4. host process inspection denied/contained;
5. child process tree contained;
6. resource limits installed;
7. environment sanitized;
8. the inherited managed-app broker/capability channel remains installed.

The first backend is deliberately strict: a secured application may not create child
processes. That is a stronger form of child-tree containment and avoids claiming a
rootless cgroup/PID-namespace guarantee the host may not actually provide. A future
plan may loosen this only with an independently qualified descendant-containment owner.

This plan does not make `Secured` portable. Non-Linux platforms and Linux hosts that
lack any required primitive continue to return a typed `SecuredUnavailable` before
application exec.

## Why ready

The work was previously premature because no production application process owner,
launch authority, package trust, or persistent grant/catalog path existed. That is no
longer true.

Plan 369 now supplies the exact exec owner (`i2pr-apphost`) and a real inherited
application channel. Plan 383 supplies a production launch catalog whose authority is
derived from verified installed packages and persistent operator policy. Black-box
qualification already proves real applications can reach the private SAM/I2CP gateway
through that chain under `UnsafeDirect`.

The remaining missing capability is therefore localized: the existing
`SecuredUnavailable` gates in `i2pr-app-state`, the apphost bootstrap contract, and
`i2pr-apphost` must be replaced by a platform-qualified Linux sandbox result, not by
another process/runtime redesign.

## Research decisions

### Linux first; unsupported is fail-closed

The first backend targets Linux x86_64 and aarch64. macOS and Windows remain
`SecuredUnavailable` and receive separate plans because their security mechanisms and
qualification evidence are materially different.

The implementation may not weaken `Secured` to "best effort". Missing kernel support,
a missing policy primitive, an unsupported architecture, or a sandbox setup failure is a
launch refusal.

### No dependency on unprivileged user namespaces for the first backend

The baseline design must not require user namespaces, mount namespaces, PID namespaces,
a privileged helper, root, sudo, systemd, containers, or an external sandbox executable.

User namespaces are deployment-policy dependent and may be disabled even on otherwise
supported Linux hosts. They can be evaluated later as an additional backend, but are not
the authority for the first `Secured` claim.

### Layered kernel enforcement

The Linux backend must use **independent layers**, each with a named property:

- Landlock for unprivileged filesystem confinement and, where supported, an additional
  network-deny layer;
- `no_new_privs` before application exec;
- seccomp-BPF for syscall classes that Landlock cannot completely express, including
  network/socket creation/connection, host-process inspection, namespace/mount escape,
  dangerous kernel interfaces, and process creation in this first no-subprocess profile;
- hard rlimits for the resource requests/ceilings already frozen by Plans 369/383;
- descriptor hygiene so the application inherits only stdin/stdout/stderr plus no hidden
  socket/file authority.

Official Linux documentation states that Landlock is designed for unprivileged
self-restriction and that restrictions inherit into future children; TCP restrictions
exist from ABI v4 and filesystem truncation from ABI v3. The implementation must probe
the running ABI and claim only a profile whose required rights are actually enforceable:
<https://docs.kernel.org/userspace-api/landlock.html>.

The repo forbids local unsafe code. Any syscall/filter dependency therefore needs a
specific dependency review: purpose, current maintenance status, MSRV 1.89,
x86_64+aarch64 coverage, transitive/unsafe surface, license, and whether it requires a
system C library or external binary. The current `landlock` crate is a plausible safe
filesystem-policy layer. The archived `seccompiler` project is **not** an acceptable
default solely because it is familiar. If no maintained safe seccomp abstraction meets
the constraints, stop and register a substrate plan rather than adding local unsafe or
silently dropping the seccomp layer.

### Private persistent application data is part of the sandbox contract

The immutable package root is not an application data directory. Secured mail and other
stateful applications need a writable location that survives package replacement and
manager restart without granting access to policy state or sibling applications.

Plan 407 therefore adds a manager-owned data root below the existing managed-app state
root, keyed by the trusted publisher identity plus `AppId`, not by package version or
instance id. The package root is read/execute-only; the app data root is read/write and
non-executable.

The exact layout is frozen by the implementation ADR, but it must remain under the
manager-owned state root and outside `packages/` and `policy/`. The application
receives a stable environment key such as `I2PR_APP_DATA_DIR`; the value is
manager/apphost-created authority, never manifest- or app-supplied input.

### Initial child policy is deny, not "best effort cleanup"

The first Linux secured profile denies `clone`, `clone3`, `fork`, and `vfork`
(or the architecture-equivalent complete process-creation set). This makes
`ChildTreeContained` mechanically true without depending on cgroup delegation or
PID-namespace availability.

A later plan may add subprocess support only after it owns both containment and
tree-wide teardown. Plan 407 must not invent a weak process-group approximation and
call it equivalent.

## ADR required

Implementation must add the next free ADR (expected ADR 0038 at registration time)
recording:

- Linux-first support boundary;
- layered Landlock + no-new-privs + seccomp + rlimit model;
- no-unprivileged-user-namespace dependency for this backend;
- no-subprocess secured-v1 policy;
- private package/data/system-runtime filesystem policy;
- attestation evidence semantics and unsupported-host behavior.

If the implementation research materially changes any of these decisions, stop before
production code and reconcile the ADR/plan rather than drifting silently.

## Ownership and production changes

### `i2pr-app-state`

Own the persistent per-application data directory derivation and creation because it
already owns the managed-app state root and trusted launch decision.

Required properties:

- identity key is publisher trust identity + `AppId`;
- package version changes do not change the data root;
- sibling apps/publishers cannot resolve to the same root;
- directory is private at creation and revalidated on reopen;
- package removal does not silently delete application data;
- policy/package transaction recovery cannot traverse into data;
- launch decision carries a typed data-root value, not an arbitrary string from
  manifest/application input.

### `i2pr-app-manager-proto` apphost bootstrap

Extend the private unreleased bootstrap contract with:

- trusted data root;
- sandbox result/attestation in the ready response;
- typed platform/setup failure reasons;
- the already-frozen resource ceilings, now as enforceable inputs rather than
  descriptive-only values.

The contract remains bounded, duplicate/unknown-field rejecting, and directional.
A `Secured` request is no longer rejected merely because of its profile; it is
rejected when the current platform/backend cannot produce a complete attestation.

### `i2pr-apphost`

Own the Linux sandbox setup immediately before the application exec.

For a secured launch the order is load-bearing:

1. decode/validate the trusted bootstrap;
2. canonicalize package and data roots and re-check their separation/containment;
3. resolve executable while filesystem authority is still sufficient;
4. construct the filesystem policy from trusted canonical paths;
5. close/mark-close-on-exec every descriptor not explicitly part of stdio/bootstrap;
6. install hard resource limits;
7. install Landlock rules;
8. install `no_new_privs`;
9. install seccomp filter;
10. produce/validate the exact `SandboxAttestation`;
11. acknowledge readiness;
12. exec the already-resolved application **in place** so the inherited manager pipes
    become the application's managed-app v1 stdin/stdout channel.

The secured path must not spawn an additional unsandboxed relay child. In-place exec
keeps the process edge simple and prevents a new supervisor/helper trust zone from being
invented solely to work around `pre_exec`.

The existing `UnsafeDirect` child/relay path remains separate and retains its explicit
no-containment semantics.

### Filesystem policy

At minimum:

- verified package root: read + directory traversal + execute, never write/create/remove;
- app data root: read/write/create/remove/truncate, never execute;
- only the minimum read-only system runtime paths required for the supported Linux
  executable model;
- only required device files (for example null/random) when demonstrated by tests;
- no user home, router data root, managed-app `policy/`, sibling package/data roots,
  `/proc`, `/sys`, arbitrary `/tmp`, or cwd inheritance.

If dynamic-loader requirements force a broad filesystem grant, stop and narrow the
supported executable model rather than claiming `PrivateFilesystem` over an
unreviewed host tree.

### Seccomp policy

The policy must fail closed and explicitly cover at least:

- socket/network creation and connection paths, including bypass-capable async/kernel
  interfaces such as io_uring;
- `ptrace`, `process_vm_*`, pidfd inspection/access, perf/BPF/keyring interfaces used
  to inspect or influence other host processes;
- mount/namespace/chroot/setns/unshare escape primitives;
- process creation for the initial no-subprocess profile;
- privilege-changing or module/kernel-control operations that an unprivileged process
  should never need.

Do not use a generic third-party default policy without mapping each
`SandboxProperty` to exact enforced rules and negative tests.

## Invariants

- `Secured` never degrades to `UnsafeDirect`.
- A partial sandbox never yields a complete `SandboxAttestation`.
- Application/manifest bytes cannot select a host path, filter profile, or attestation.
- The app receives no router/admin credential and no loopback fallback.
- stdin/stdout remain the sole managed-app capability channel.
- No direct network socket path exists after secured exec.
- Package tree cannot be modified by the app.
- App data cannot reach sibling app data, package-policy metadata, router state, or user
  home through ordinary filesystem operations.
- Resource limits are hard limits and cannot be raised by the application.
- No child process can be created in the first secured Linux profile.
- Sandbox setup failure occurs before application exec.
- `UnsafeDirect` behavior remains explicitly distinct and unchanged.
- No support.toml/RouterInfo/SAM/I2CP advertisement changes.

## Scope

In scope:

- Linux x86_64 + aarch64 secured backend;
- trusted persistent app data root;
- bootstrap/attestation changes necessary to prove the backend;
- dependency and boundary-guard changes;
- hostile black-box qualification using the real `i2pr-appd -> i2pr-apphost -> app`
  chain;
- routine CI lane on a Linux runner where required primitives are available.

Out of scope:

- macOS/Windows sandbox backends;
- brokered clearnet;
- subprocess support in Secured mode;
- live AppManager administrator IPC;
- UI hosting;
- remote repository/update/TUF;
- Proposal 170 scoped control;
- making managed-app v1 released/advertised;
- using `UnsafeDirect` as fallback.

## Work packages

### WP1 — mechanism/dependency freeze and ADR 0038

Review candidate safe Rust Landlock/seccomp/resource/descriptor APIs against MSRV,
architectures, licensing, maintenance and transitive unsafe. Write ADR 0038 and freeze
the exact kernel minimum/feature probes.

Acceptance: no unresolved sandbox mechanism remains; unsupported kernels have a typed
pre-exec refusal path.

### WP2 — private data-root authority

Add the persistent data-root owner in `i2pr-app-state`, thread it through
`LaunchDecision` and the private apphost bootstrap, and prove identity separation,
restart stability, permissions, symlink/canonicalization refusal, and package-update
stability.

Acceptance: two publishers with the same AppId cannot collide; two versions of one
trusted app reuse the same data root; sibling/root/policy traversal fails.

### WP3 — Linux sandbox setup and attestation

Implement the ordered apphost setup, remove the unconditional `SecuredUnavailable`
gate only on the qualified Linux path, and add a truthful attestation.

Acceptance: deleting/bypassing any one enforcement layer prevents a valid secured
attestation or fails a named negative test.

### WP4 — hostile fixture qualification

Extend the real managed-app fixture with probes for:

- IPv4/IPv6 public TCP;
- loopback TCP;
- UDP/ping-like direct network attempts;
- DNS/socket creation;
- user-home/router-state/sibling-data/package-write reads/writes;
- `/proc`/process inspection and ptrace/process_vm/pidfd attempts;
- child creation;
- resource-limit max+1;
- environment inheritance;
- descriptor enumeration/leak attempts;
- normal app-data read/write/restart persistence;
- normal SAM and I2CP managed streams after sandboxing.

Every forbidden row must fail for the **sandbox reason**, not because the target happens
to be absent or unreachable. Use local controlled listeners/files/fixtures where a
positive control is needed.

### WP5 — CI, guards, docs and closure

Add platform guards proving the secured path cannot be compiled into an unguarded
fallback, run the routine floor plus Linux-specific black-box lane, update architecture
and security docs, and write the closure/unblock audit.

## Failure, restart and contention semantics

- Missing/unsupported Landlock or seccomp capability: typed `SecuredUnavailable`;
  no exec.
- Data-root creation/canonicalization/permission error: launch fails before apphost
  authority is issued.
- Sandbox setup error after bootstrap but before ready: typed apphost failure and
  process teardown.
- Exec failure after ready: manager observes channel EOF/no valid app hello and tears the
  launch down; it must not retry indefinitely.
- Manager/router EOF: secured application loses its only capability channel and is
  terminated/reaped according to existing Plan-369 ownership.
- App restart receives the same persistent data root but a fresh instance id and fresh
  managed-app session.
- Concurrent launches of the same app may share the persistent data directory only if
  the existing policy allows multiple instances; Plan 407 must not introduce a hidden
  file-locking policy. If mail or another app requires single-instance storage, that is
  application-level policy unless a separate runtime invariant is registered.

## Compatibility and migration

The managed-app contracts are still experimental/unreleased. Bootstrap changes are
therefore a pre-release contract correction, not a public wire migration, but golden
fixtures and all four directional decoders must be updated together.

Persistent state gains an application-data subtree but does not reinterpret existing
policy generations or package identities. Existing `UnsafeDirect` selections keep
their current semantics.

No existing package may be silently switched to `Secured`; the operator-selected
launch profile remains authoritative.

## Required tests

At minimum:

- data-root identity/path/permission/restart/update matrix;
- bootstrap exact bytes, max+1, duplicate/unknown fields, missing data root, malformed
  attestation;
- kernel feature probe exact result mapping;
- each Landlock filesystem permission allowed/denied at both boundary and sibling paths;
- network denial positive-control matrix;
- process-inspection denial matrix;
- process-creation denial matrix;
- resource limits at exact ceiling and max+1;
- inherited descriptor census;
- app channel survives sandbox setup and carries exact SAM/I2CP bytes;
- secured restart uses fresh instance identity and persistent data;
- unsupported OS and unsupported Linux kernel refuse before exec;
- mutation/negative controls for every static guard and each claimed attestation
  property.

## Exact verification commands

Run the complete routine floor from `AGENTS.md`, including the explicit managed-app
binary build before daemon qualification.

Additionally on Linux:

```text
cargo test --locked -p i2pr-app-manager-proto --all-targets -- --test-threads=1
cargo test --locked -p i2pr-app-state --all-targets -- --test-threads=1
cargo test --locked -p i2pr-apphost --all-targets -- --test-threads=1
cargo test --locked -p i2pr-appd --all-targets -- --test-threads=1
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
cargo test --locked -p i2pr-daemon --lib app_runtime_qualification -- --test-threads=1
python3 scripts/check-managed-app-process-boundary.py
python3 scripts/check-managed-app-process-boundary.py --self-test
python3 scripts/check-managed-app-policy-boundary.py
python3 scripts/check-managed-app-policy-boundary.py --self-test
```

Add a dedicated secured-sandbox boundary checker with a `--self-test` mutation suite;
its exact filename becomes part of the routine floor and CI in this plan.

Hosted closure requires an exact-head Linux CI result for the secured black-box lane.
Local-only evidence is insufficient for a new OS security claim.

## Documentation updates

- new ADR 0038;
- `AGENTS.md` durable managed-app sandbox constraints, without copying plan status;
- `docs/architecture/i2pr-apphost.md`;
- `docs/architecture/i2pr-app-state.md`;
- `docs/architecture/i2pr-app-manager-proto.md`;
- managed-app security/threat-model documentation;
- `specs/references/managed-native-app-runtime-v1.md` only where the durable secured
  contract needs clarification;
- `specs/references/managed-app-policy-v1.md` for data-root ownership/layout;
- managed-native-app roadmap and registry;
- routine floor/tooling inventory for the new checker/lane.

Do not change `specs/support.toml` merely because local application sandboxing works.

## Acceptance criteria

Plan 407 may close only when:

- a production `Secured` launch succeeds on qualified Linux x86_64/aarch64 hosts;
- all eight required `SandboxProperty` values are backed by executed negative evidence;
- the application has persistent private data and cannot modify its package or inspect
  sibling/router/user-private state;
- direct public/LAN/loopback networking is denied with positive controls;
- process inspection and child creation are denied;
- requested memory/open-file ceilings are installed as hard limits;
- no ambient environment or unexpected descriptor authority reaches the app;
- the managed-app stdin/stdout channel still reaches real private SAM and I2CP paths;
- unsupported platforms/hosts still fail before exec;
- exact-head Linux CI is green;
- no high/medium security finding remains open.

## Stop conditions

Stop and register a narrower corrective/substrate plan if:

- satisfying the profile requires local `unsafe` in an i2pr crate;
- the only viable seccomp path requires an abandoned dependency, privileged daemon,
  root, sudo, external sandbox binary, or silently optional enforcement;
- filesystem isolation requires broad access to user/router state;
- a required property can only be inferred from configuration rather than negatively
  executed;
- child containment cannot be proven under the no-subprocess profile;
- the sandbox breaks the inherited app channel and the proposed fix introduces a
  loopback/socket fallback;
- a platform cannot meet all eight properties but would still receive a complete
  attestation;
- implementation scope expands into macOS/Windows, brokered clearnet, UI, or scoped
  control.

## Closure evidence required

The closure record must include:

- implementation/ADR/dependency commits and exact dependency review;
- kernel/architecture qualification matrix and feature-probe output;
- requirement-to-enforcement mapping for all eight sandbox properties;
- hostile fixture results with positive controls;
- resource/descriptor/process-tree evidence;
- data-root restart/update/separation evidence;
- routine floor and exact-head hosted Linux CI;
- security review and unresolved findings by severity;
- compatibility/non-claims;
- unblock audit for downstream managed-app consumers, including `dbowm91/i2pr-mail`
  M006 if its remaining local prerequisites are closed.

## Handoff notes

Do not optimize for a permissive general-purpose sandbox in this first pass. The useful
product boundary is a small, auditable secured profile that can run first-party Rust
applications such as i2pr-mail with private persistent data and only the inherited I2P
capability channel.

The no-subprocess rule is intentional. It reduces the number of kernel/process-lifecycle
claims Plan 407 must prove and can be relaxed later only with evidence.

Do not remove the current `SecuredUnavailable` gates piecemeal. They move together only
when the end-to-end apphost path can return a complete, validated attestation.
