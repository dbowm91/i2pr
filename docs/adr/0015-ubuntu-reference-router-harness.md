# ADR 0015: Ubuntu reference-router interoperability harness boundary

- Status: accepted for Plan 038 foundation
- Date: 2026-07-15
- Decision owners: repository maintainers

## Context

Plan 036/037 established a manual NTCP2 evidence contract but did not provide
source-pinned reference builds, isolated Linux topology, disposable router
state, or a process/evidence runner. A harness that starts Java I2P or i2pd
must not turn preparation-network access into public-router behavior, and it
must not move Tokio or socket ownership below `i2pr-runtime`.

## Decision

The harness has two security domains:

1. network-enabled preparation installs only the locked Ubuntu package set,
   fetches the exact Java I2P `2800040` and i2pd `f618e41` revisions, verifies
   the pinned IzPack 5.2.4 artifact, builds relocatable caches, and records
   source/tool/artifact hashes;
2. network-isolated execution creates two disposable namespaces joined only by
   a veth pair, rejects default/public routes, disables reseed/bootstrap,
   renders implementation-specific configuration, launches foreground
   processes, and sanitizes typed evidence before deleting raw state.

The dedicated `tools/i2pr-interop` binary is a non-production composition seam
depending on the runtime and protocol owners. Until the complete wire-level
adapter exists, `listen` and `dial` return `blocked_missing_driver`; they do
not activate `i2pr-daemon`, claim a handshake, or print arbitrary diagnostics.

## Consequences

- The first manual workflow is explicitly `ubuntu-24.04`, not a moving
  `ubuntu-latest` label.
- Namespace cleanup and evidence sanitation are part of scenario success; a
  protocol pass with failed cleanup is a failed result.
- Environment smoke and Java-I2P/i2pd reference crosscheck are harness
  validation only. Mixed-router i2pr evidence remains required before any
  support-ledger or RouterInfo advertisement change.
- Generated source trees, identities, keys, RouterInfo, logs, configurations,
  and raw result files stay under ignored `target/interop` paths.

## Rejected alternatives

- Public-network or shared-host execution: it violates the test boundary and
  cannot prove that peer discovery or route leakage is absent.
- Docker/Podman/systemd as a first dependency: it obscures Linux namespace and
  process ownership and is outside the narrow Ubuntu host contract.
- A launcher in `i2pr-daemon` or a Tokio dependency in transport crates: it
  would weaken the existing runtime ownership boundary before the adapter is
  complete.
