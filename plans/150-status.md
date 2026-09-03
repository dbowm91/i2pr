# Plan 150 status — reproducible external-client final Milestone 7 closure

Status: **`next-executable-on-plan149-pass`**.

Registered: **2026-09-02** (UTC).

Plan of record:
[`plans/150-m7-sam31-external-client-reproducible-final-closure.md`](150-m7-sam31-external-client-reproducible-final-closure.md).

## Current authority

Plan 149 has passed its self-composed localhost product closure. Its
black-box evidence covers transactional `SESSION CREATE` composition, one
shared `Arc<DestinationIdentity>`, local LeaseSet2 resolution, exact
CONNECT/ACCEPT `SILENT` behavior, same-read preservation, bounded
backpressure, bidirectional 2 MiB transfer, typed delivery degradation, and
terminal resource cleanup.

Plan 150 is now the next executable SAM plan. It must run correctly pinned
independent SAM clients through the real loopback listener and close the
remaining external-client, FORWARD, NAMING, slow-peer, fault-matrix, and
sibling-stream evidence. It must preserve the localhost-only, non-advertised
scope and must not claim router-to-router interoperability.

## Client pins

```text
libsam3 = 7d6e658798baec31394c5685f9583343cc00900b
i2psam  = b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
i2plib  = 6edf51cd5d21cc745aa7e23cb98c582144884fa8 (supplementary)
```

The live fetch/build guidance is in
[`tests/integration/sam/README.md`](../tests/integration/sam/README.md) and
`scripts/interop/fetch-sam-clients.sh`. The old Plan 148 libsam3 pin is
invalid and must not be restored.

## Handoff

Execute Plan 150 only. Start with the local Plan 149 suite and the Plan
127–134 regression floor, then use the pinned external clients and record
sanitized evidence in the Plan 150 lane. Do not move to Milestone 8 until
the Plan 150 acceptance criteria and current CI head are green.
