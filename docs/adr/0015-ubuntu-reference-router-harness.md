# ADR 0015: Ubuntu reference-router interoperability harness boundary

- Status: accepted for Plan 040 corrective apparatus; extended by Plan 041
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
   fetches the exact Java I2P
   `2800040deee9bb376567b671ef2e9c34cf3e30b6` and i2pd
   `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` revisions, verifies
   the pinned IzPack 5.2.4 artifact, builds relocatable caches, and records
   source/tool/artifact hashes;
2. network-isolated execution creates two disposable namespaces joined only by
   a veth pair, rejects default/public routes, disables reseed/bootstrap,
   renders implementation-specific configuration, launches foreground
   processes, and sanitizes typed evidence before deleting raw state.

The dedicated `tools/i2pr-interop` binary is a non-production composition seam
depending on the runtime and protocol owners. The current checkout contains
the bounded runtime-owned handshake action executor, authenticated link owner,
listener/dial promotion, and DeliveryStatus smoke. It does not activate
`i2pr-daemon`, claim mixed-router interoperability, or print arbitrary
diagnostics.
The Plan 041 reference-only control path is owned by
`tests/integration/ntcp2/harness/reference_runner.py` and
`reference_topology.py`, not by the launcher or the normal daemon. The
launcher `inspect` operation may perform strict RouterInfo/signature/NTCP2
address validation and emit only bounded typed JSON.

## Plan 042 launcher and evidence amendment

The completed Plan 042 launcher protocol is versioned and typed. `listen` must
flush one listener-readiness record and later one authenticated terminal
result; `dial` must emit one terminal result; and `inspect` must emit only
redacted state metadata. Listener readiness proves only that the configured
literal listener is active. Authentication requires the complete handshake,
active-link admission, an authenticated data exchange, and bounded cleanup.
The status record is schema-1 `i2pr-interop-status` JSON with fixed phase,
result, reason-code, and aggregate counters. Readiness is separate from the
terminal result. Local success is driver validation only; the reference
runner still needs its isolated i2pr/reference adapter and authoritative
observations before evidence can be retained.

The selected initial smoke message is the existing fixed-size DeliveryStatus
I2NP message (type 10): 12-byte body, 21-byte NTCP2/SSU2 short transport
encoding, and 24-byte NTCP2 block before frame overhead and padding. A positive
Plan 042 record must show one valid outbound and one valid inbound message per
direction, in addition to authenticated handshake and cleanup results. No
reference echo behavior has been proven, so the selection does not yet support
an interoperability claim and padding/TCP readiness cannot substitute for it.

The runtime-owned driver remains in `i2pr-runtime`; it owns socket I/O, action
deadlines, cancellation, replay/admission, authenticated frame state, bounded
queues, and child joins. The launcher only supplies bounded scenario and
disposable-state capabilities. The current checkout has no sanitized i2pr-to-
reference record: Plan 041 reference-pair output is a harness control only,
and the current host blocker is `blocked_host_contract`.

## Consequences

- The first manual workflow is explicitly `ubuntu-24.04`, not a moving
  `ubuntu-latest` label.
- Namespace cleanup and evidence sanitation are part of scenario success; a
  protocol pass with failed cleanup is a failed result.
- Environment smoke and Java-I2P/i2pd reference crosscheck are harness
  validation only. Mixed-router i2pr evidence remains required before any
  support-ledger or RouterInfo advertisement change.
- Plan 041 uses a dedicated pair schema, one-way namespace firewall policy,
  explicit non-public network ID 99, staged RouterInfo exchange, and dual
  authenticated observations. A TCP connection, listener, RouterInfo file,
  or generic `NTCP2` log line is not a successful crosscheck.
- Generated source trees, identities, keys, RouterInfo, logs, configurations,
  and raw result files stay under ignored `target/interop` paths. Cache lookup
  is by canonical reference and current-cache summary; sanitized evidence is
  written under `target/interop/evidence/` only after cleanup, and the run
  root is deleted even when a failed record is retained.
- `blocked_host_contract`, launcher `result=blocked` with its fixed reason code
  (and `i2pr-mixed-router-profile-not-wired` from the incomplete reference
  adapter), rejected
  state/configuration, authentication failure, timeout, and cleanup failure
  remain typed outcomes.
  None may be converted to `passed`; an empty evidence directory is not
  evidence.

## Rejected alternatives

- Public-network or shared-host execution: it violates the test boundary and
  cannot prove that peer discovery or route leakage is absent.
- Docker/Podman/systemd as a first dependency: it obscures Linux namespace and
  process ownership and is outside the narrow Ubuntu host contract.
- A launcher in `i2pr-daemon` or a Tokio dependency in transport crates: it
  would weaken the existing runtime ownership boundary before the adapter is
  complete.
