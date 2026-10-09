# Plan 386 — Linux Secured Managed-App Sandbox and Resource Enforcement

Status: **blocked on Plan 385**

Date: 2026-10-09

Roadmap: `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependencies:

- Managed native app runtime/369–371 closed.
- Plans 382–383 closed.
- Plan 385 closed — canonical package/data/runtime filesystem inputs.

Classification: security invariant + platform capability.

## 1. Objective

Make `LaunchProfile::Secured` real on Linux by enforcing and attesting the
properties already frozen in managed-app v1:

- no direct host networking, including loopback;
- private filesystem boundary;
- contained process tree;
- installed memory/open-file ceilings;
- sanitized environment;
- inherited managed-app capability channel retained;
- no router/admin credential exposed.

Unsupported platforms remain typed `SecuredUnavailable`; this plan does not
fake cross-platform parity.

## 2. Scope and platform policy

The first qualified backend is Linux. It must support the repository's Linux
release targets for which the required kernel primitives are available.

macOS and Windows remain explicit unsupported Secured backends until separate
plans choose supported OS mechanisms. `UnsafeDirect` remains available only
under the existing conspicuous administrator opt-in and receives no containment
claim.

Do not use undocumented/private OS APIs merely to make a platform row green.

## 3. Backend boundary

Add a small apphost-owned sandbox abstraction. Router/runtime crates must not
learn platform sandbox syscalls.

Conceptually:

```text
validated LaunchDecision
  + verified package entrypoint
  + Plan-385 app data/cache/run roots
  + resource ceilings
  + inherited managed-app IPC endpoints
        |
        v
SandboxBackend::prepare()
        |
        +-> install filesystem/network/process/resource policy
        +-> produce complete SandboxAttestation
        |
        v
exec application
```

A backend that cannot install every required property returns a typed failure
before application code executes.

## 4. Linux security requirements

Implementation may combine supported kernel primitives, but acceptance is
property-based rather than tied to one syscall name.

Required observable properties:

- application cannot create/connect/bind ordinary IPv4 or IPv6 network sockets;
- loopback is denied as part of the same rule;
- inherited managed-app IPC remains usable;
- application can read its selected package resources as required to execute;
- application can read/write only its Plan-385 data/cache/run writable set plus
  the minimum read-only runtime/library set required by the executable;
- sibling app data, router data, policy/store metadata, home-directory secrets,
  and arbitrary host files are denied;
- descendants cannot escape the boundary and are terminated when the launch
  owner terminates;
- `open_files` and `memory_bytes` policy values become enforced limits, not
  descriptive metadata;
- environment is the explicit allow-set only;
- privilege-gaining mechanisms are disabled/fail closed.

If the dynamically linked GNU target cannot satisfy the filesystem property
without an unsafe broad read grant, stop and record that platform/profile
boundary rather than redefining "private filesystem".

## 5. Attestation

Only trusted apphost composition creates
`SandboxAttestation`. It must record backend kind/version and the actual
installed property set.

`Secured` launch proceeds only if the attestation validates all required v1
properties. An app message, package manifest, or persisted policy cannot supply
or modify attestation.

A probe that merely says a kernel feature exists is not attestation; the launch
must prove policy installation succeeded for that process.

## 6. Resource and process semantics

- Clamp requested limits to administrator ceilings already resolved by Plan
  383.
- Zero/unrepresentable limits fail before exec.
- Do not silently substitute an unenforced memory "hint".
- Child/grandchild lifetime must be tied to the manager launch owner; shutdown
  and crash cleanup are bounded.
- Thread creation required by ordinary Rust programs must remain possible; a
  simplistic process ban that also prevents required threads is not acceptable.
- Application attempts to execute arbitrary sibling/host binaries must fail
  unless those binaries are within the explicit executable/read-only policy.

## 7. Work packages

### WP1 — platform primitive freeze

Document the exact supported Linux primitives, minimum kernel/runtime
requirements, dynamic-loader policy, and unavailable-host behavior. Add an ADR
if the choice is durable.

### WP2 — backend abstraction and probe

Implement fail-closed backend selection and capability probing without changing
application authority.

### WP3 — filesystem + environment containment

Install the Plan-385 writable roots and minimal package/runtime read set; prove
negative access outside them.

### WP4 — network denial

Deny direct TCP/UDP/socket access for IPv4/IPv6 including loopback while keeping
the inherited app protocol functional.

### WP5 — process/resource containment

Install process-tree cleanup and enforced memory/open-file ceilings.

### WP6 — attestation and production gate

Replace the Plan-383 `SecuredUnavailable` pre-exec gate only for a backend that
produces a complete validated attestation.

### WP7 — adversarial qualification

Run a hostile fixture attempting host networking, loopback, DNS/socket creation,
filesystem escape, sibling-data access, child persistence, environment leakage,
and resource exhaustion.

## 8. Verification

In addition to the routine floor, require black-box Linux tests from the
actual packaged/apphost path. Unit mocks cannot close this plan.

Mutation/negative evidence must show that independently removing network,
filesystem, process-tree, resource, or environment enforcement causes the
corresponding hostile fixture to succeed and the acceptance gate to fail.

## 9. Acceptance criteria

Plan 386 closes when a real Linux `Secured` app:

1. launches through the existing Plan-369/383 process path;
2. uses managed SAM/I2CP logical capabilities while direct host networking and
   loopback fail;
3. cannot read/write outside its allowed filesystem profile;
4. cannot outlive manager ownership through a child process escape;
5. is constrained by enforced memory/open-file ceilings;
6. carries a complete trusted attestation;
7. fails before exec when any required property cannot be installed;
8. leaves existing `UnsafeDirect` behavior explicitly unqualified and
   unchanged;
9. passes routine plus adversarial qualification.

## 10. Stop conditions

Stop if closure would require claiming unsupported macOS/Windows containment,
using an undocumented/private host API, allowing broad host filesystem access
in the name of dynamic loading, permitting loopback, or treating descriptive
limits as enforced limits.

## 11. Closure evidence

Create `plans/closure/managed-native-app-runtime/386-status.md` with kernel and
target matrix, exact backend primitives, hostile-fixture results, attestation
evidence, process/resource cleanup matrix, mutation evidence, unsupported
platform disposition, and downstream unblock audit.
