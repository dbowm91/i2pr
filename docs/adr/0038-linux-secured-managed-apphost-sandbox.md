# ADR 0038: Linux secured managed-apphost sandbox

Status: Accepted (Plan 407)

## Context

`LaunchProfile::Secured` needs kernel enforcement before an application receives
its inherited managed-app channel. A partial setup must never report readiness
or fall back to `UnsafeDirect`. The router remains experimental and this ADR
does not make managed-app v1 a released or advertised feature.

## Decision

The first secured backend is limited to Linux x86_64 and aarch64 hosts whose
kernel enforces Landlock ABI v3 and seccomp-BPF. It requires no root, user or
mount namespace, privileged helper, systemd, container, or external sandbox
program. Missing features and setup errors refuse launch before exec.

The apphost resolves a static, native-architecture ELF executable and canonical
package/data roots before confinement. ELF interpreters (`PT_INTERP`) are
refused; this avoids granting a host-wide dynamic-loader tree. The verified
package tree is read/traverse/execute only. A manager-owned per-publisher and
per-AppId data directory is read/write/create/remove/truncate and never
executable. No home, router state, policy, sibling roots, `/proc`, `/sys`, or
arbitrary temporary path is granted.

The apphost installs hard `RLIMIT_AS` and `RLIMIT_NOFILE` ceilings, then a
Landlock ruleset under `no_new_privs`, then a default-kill seccomp filter. The
filter denies networking, process creation, process inspection, namespace and
mount operations, and privileged kernel interfaces. Static Rust startup may
query limits with `prlimit64`; the filter permits that syscall only when its
new-limit pointer is null. It cannot change a limit. Seccomp also forbids
subprocess creation, so the secured-v1 child-tree claim does not depend on
cgroups or PID namespaces. The inherited stdin/stdout manager channel is kept
and the apphost replaces itself with the application after the Ready reply.

Readiness attestation is emitted only after all setup layers report success and
includes the eight required `SandboxProperty` values. These values attest to
installed mechanisms, not to anonymity, privacy, production readiness, or
general application compatibility. `UnsafeDirect` remains a separate explicit
operator-selected mode with ordinary host networking.

## Consequences

Secured applications must be static ELF binaries on supported Linux hosts and
must operate without threads, subprocesses, direct sockets, or host filesystem
access. Existing application data survives package replacement and manager
restart, while trust and launch policy remain outside that directory. macOS,
Windows, unsupported Linux architectures/kernels, and failed setup return a
typed refusal before application exec. No support inventory, RouterInfo, SAM,
or I2CP advertisement changes.

The boundary checker and black-box qualification are required to preserve the
syscall/property mapping. Any relaxation, including dynamic loading or child
processes, requires a new plan and independent evidence.
