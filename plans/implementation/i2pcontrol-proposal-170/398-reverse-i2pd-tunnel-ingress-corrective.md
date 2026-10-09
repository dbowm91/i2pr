# Plan 398 — reverse i2pd tunnel ingress corrective

Status: **in-progress-reverse-i2pd-tunnel-ingress-corrective**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Diagnose why stock SAM reports a connection to the i2pr-published encrypted
address, but no inbound server SYN or fixture payload reaches the service after
the receive-owner key was corrected. Determine whether the reference emits a
reverse request, whether matching TunnelData reaches the local endpoint, and
where I2NP/Streaming dispatch stops. Correct only the first demonstrated
transition and rerun exact-pinned NONE.

## Evidence and constraints

Plan 397's healthy-mesh artifact
`target/interop/els2-evidence-plan397-none-owner-id-fix-20261009` reports:

- type-5 DatabaseStore local admission: accepted;
- stock SAM encrypted-address connect: returned;
- reverse payload: absent;
- transaction orphan delta: 0;
- server SYN observed/accepted: 0/0;
- successful loopback target dials: 0;
- inbound Streaming dispatch delta: 0;
- service provisioning: two inbound roles, two usable leases, two installed
  bridge projections, no owner or activation failures.

The post-start owner map now uses `local_inbound_receive`, distinct from the
pool creator id. The current evidence does not prove that the reference emitted
client TunnelData merely because SAM connect returned.

## Invariants

1. Stock i2pd 2.61.0 at exact SHA
   `635b013a612ff47278ef02acf8580a28e10e26c5`; every execution keeps
   `MAX_ATTEMPTS=1`.
2. Keep the reference unmodified and payload traffic on loopback/private mesh.
3. Raw reference logs never enter evidence. Extract only bounded counts/enums;
   no addresses, ids, keys, or payloads.
4. Preserve the Plan 397 local receive-id correction, type-5 key validation,
   tunnel ownership/cancellation, and existing Standard LS2 path.
5. Do not alter transcript, credentials, support, capability, or advertisement.

## Work packages

1. Add bounded diagnostics at the SSU2 inbound I2NP boundary, completed
   TunnelData/DatabaseStore/Garlic boundary, and server Streaming SYN queue.
   Capture baselines immediately before the reference connect and transaction
   deltas after the response window.
2. Add a source-locked/sanitized observation of the pinned reference's relevant
   ELS2 lookup, TunnelBuild, and streaming-send outcomes. Do not retain raw logs.
3. Identify whether the failure is absent reference emission, tunnel delivery,
   Garlic/ECIES dispatch, or server acceptance; fix the demonstrated owner only.
4. Add deterministic coverage for that transition and failure/teardown path.
5. Run focused tests/guards and one healthy-mesh exact-pinned NONE lane. Register
   a successor if the first missing transition is another bounded boundary.

## Acceptance

The pinned reverse NONE payload returns through type-5 publication, inbound
tunnel delivery, service Streaming acceptance, and loopback fixture response;
sanitized evidence proves the transition counters and existing authority controls
pass. No broader conformance claim is made.
