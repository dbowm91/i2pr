# ADR 0024: constrained-host NTCP2 execution-lane selection

- Status: Accepted for Plan 077.
- Date: 2026-07-31.
- Scope: test-only NTCP2 interoperability execution infrastructure.
- Supersedes: none.

## Context

The real pinned i2pd driver from Plan 076 is available, but the current
development host cannot assume rootless user/network namespaces or a reliable
Multipass guest. Plan 077 needs a deterministic way to identify an executable
lane without installing software or changing host security policy. A capability
probe must not turn tool presence, a loopback socket, or a remote workflow
definition into an interoperability result.

## Decision

The constrained-host probe inspects and records these capabilities:

```text
docker_cli_present
docker_daemon_accessible
qemu_system_present
qemu_tcg_usable
seccomp_no_new_privs_supported
remote_workflow_present
```

Lane selection is ordered:

1. an already accessible rootful Docker daemon, using one `--network none`
   container;
2. QEMU system emulation using TCG and `-nic none`;
3. inherited connected descriptors with `no_new_privs`/seccomp, explicitly
   reduced-scope and never full transport-manager qualification;
4. a manually triggered remote Linux workflow with documented isolation;
5. a typed no-full-runtime-lane result.

The probe selects only from observed, unambiguous capabilities. It does not
install Docker or QEMU, modify daemon configuration or group membership,
invoke privilege escalation, retry rootless/Multipass lanes, or run a router.
The common execution manifest is strict and contains only source, binary,
reference-build, direction, run, result-output, and timeout fields. Paths are
bounded and all artifact values are measured SHA-256 digests.

The qualification record is separate from the probe. A full-runtime lane may
be marked qualified only after it proves loopback-only communication, no
public interface or route, exact artifact digests, the two-process control,
bounded result export, and cleanup. A reduced-scope capability or a remote
workflow definition alone records `full_runtime_lane = unavailable` and
cannot authorize Plan 078.

## Consequences

- The current host can produce a reproducible typed no-lane record without
  mutating its security policy.
- Docker/QEMU packaging is added only after the corresponding capability is
  actually available; speculative lane scripts are not part of this change.
- The inherited-descriptor surface remains useful for protocol diagnostics,
  but its records carry a distinct reduced scope and cannot satisfy the
  normal listener/dial transport-manager requirement.
- Plan 078 remains blocked until a full-runtime qualification record exists.
- NTCP2 remains experimental and non-advertised.

## Records and enforcement

The implementation is in:

- `scripts/interop/probe-constrained-host-lanes.sh`;
- `tests/integration/ntcp2/harness/execution_lane.py`;
- `scripts/check-constrained-host-lane-boundary.sh`.

The qualification output is sanitized and bounded at
`target/interop/lane/qualification.json`. It contains no identity material,
RouterInfo, endpoints, private paths, raw logs, payloads, or protocol
transcripts.
