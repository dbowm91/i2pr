# `i2pr-apphost` — the direct-exec application supervisor

`i2pr-apphost` (Plan 369) is the **only** component in the workspace that execs
an application. It is a separate runtime trust zone whose only production
`i2pr-*` dependencies are `i2pr-app-manager-proto` and `i2pr-app-proto`; it may
not reach `i2pr-appd`, and `scripts/check-dependency-direction.sh` makes a
future `i2pr-apphost -> i2pr-appd` edge a hard failure.

## What this process is

A single, one-shot, bounded supervisor. It accepts exactly **one**
`LaunchRequest` over the inherited anonymous transport from `i2pr-appd`, execs
the named application directly, and then becomes a **byte-transparent relay**
between the manager and that application until one side closes. It launches
nothing else, ever, and serves nothing after the application exits.

Bootstrap is one-shot and then byte-transparent: the launch request is consumed
exactly once, and everything after it is forwarded without interpretation.

## Launch profiles

- **`Secured`** is supported only on qualified Linux x86_64/aarch64 hosts and
  only for static native ELF executables. Apphost enforces private package and
  app-data trees with Landlock, hard address-space/open-file limits, and a
  default-kill seccomp filter under `no_new_privs`. Network access,
  process-inspection operations, and child creation are denied. A complete
  attestation is sent only after all layers succeed; unsupported hosts and
  setup failures are refused before exec.
- **`UnsafeDirect`** remains an explicit unsandboxed host-networking profile.
  It does not receive a secured attestation.
- **There is no shell.** The application is exec'd directly. Nothing is passed
  to `sh -c`, and `PATH` is never consulted.
- **There is no second authority.** No discovery endpoint, no signal-based
  teardown, no control file. EOF on the inherited transport is the only shutdown
  signal.

`scripts/check-managed-app-process-boundary.py` rules 1 and 1b assert the spawn
site and forbid both shell launchers and any `PATH` lookup.

## Containment is checked twice

`LaunchRequest::validate` rejects `..`, `.`, absolute, and backslash forms
**structurally**, on strings, so it is testable without a filesystem. That is
necessary but not sufficient: a symlink inside the root can still point outside
it. `resolve_command` therefore canonicalises both paths and re-checks
containment on the resolved result.

The string check stops the obvious escape; the canonical check stops the
disguised one. A single check would leave one of the two open.

## Every wait is bounded

| Constant | Value |
| --- | --- |
| `APPHOST_BOOTSTRAP_GRACE` | 10 s |
| `MAX_APPHOST_STDERR_SNAPSHOT_BYTES` | 8 KiB retained |
| `APPHOST_CLOSE_GRACE` | 5 s |
| `APPHOST_FORCED_KILL_GRACE` | 500 ms |
| `RELAY_CHUNK_BYTES` | 8 KiB |

The bootstrap read, the stderr drain, the close grace, and the forced-kill grace
all have explicit ceilings. The direct child is owned by this process from
`spawn` until it is reaped, and it is **killed rather than leaked** if the
manager goes away first — Plan 369 §12 disclaims grandchild containment, so "the
pipe closed" must not mean "an application keeps running with nobody to talk
to".

stderr is drained continuously and bounded: the **retained prefix** is capped at
8 KiB while the **total byte count** keeps rising past it. A flooding application
completes normally; it just cannot make this process allocate without limit.

## Bootstrap framing

The bootstrap is a bounded length-prefixed JSON payload, decoded by
`i2pr-app-manager-proto::apphost`. A zero-length payload, a payload past
`MAX_BOOTSTRAP_PAYLOAD_BYTES`, malformed JSON, and unknown fields are all
refused as typed `ApphostBootstrapError` values rather than skipped. Identifier,
root, and environment sizes are separately bounded (`MAX_APPHOST_IDENTIFIER_BYTES`,
`MAX_ROOT_BYTES`, `MAX_ENV_ENTRIES`), and the environment handed to the child is
**constructed**, not inherited — the parent environment does not leak into an
application.

## Relay

After bootstrap, `serve` becomes a byte-transparent relay. Both directions are
chunked at `RELAY_CHUNK_BYTES` and forwarded without interpretation, so the
manager's app v1 stream ids are what the application sees. `RelayOutcome`
distinguishes a clean application exit from a forced kill from a transport
loss, so callers do not have to infer it from EOF.

## Limits

- **No secured subprocesses.** Seccomp denies process creation, so v1 does not
  depend on an unqualified descendant supervisor or host cgroup delegation.
- **Static ELF only.** `PT_INTERP` executables are refused rather than granting
  broad host loader paths.
- One launch per process. There is no restart, pooling, or reuse.
- The apphost's own executable is deliberately **not** resolved by sibling
  lookup: its target is not a sibling but the root/entrypoint of a launch
  request the manager already validated, so it is governed by the containment
  rules above instead.
