# Plan 172 status — Milestone 9 independent LeaseSet2 lifecycle corrective

Status: **`active-m9-i2cp-independent-leaseset2-lifecycle-corrective`**.

Registered: **2026-09-08**.

Plan of record:
[`plans/172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md`](172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md).

## Current authority

This status intentionally supersedes the final-closure interpretation of Plan 170 while retaining Plan 170's successfully executed independent wire/data-plane evidence.

```text
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
plan_171 = passed-m9-i2cp-invalid-preamble-close-and-ci-corrective
plan_170_external_wire_data_plane = retained-passed
plan_170_final_acceptance = superseded-by-plan172
plan_172 = active-m9-i2cp-independent-leaseset2-lifecycle-corrective

milestone9_i2cp_local_product = passed-via-plan169
milestone9_i2cp_independent_wire_data_plane = passed-via-plan170
milestone9_i2cp_independent_leaseset2 = not-yet-proven
milestone9_final_acceptance = reopened-by-plan172
next_executable_plan = 172
next_product_layer = milestone9-i2cp-corrective
```

Do not begin Milestone 10 planning until this status is replaced by a passing Plan 172 closure record.

## Trigger evidence

Plan 170's plan-of-record requires both exact-pinned independent clients to create modern client-owned Standard LeaseSet2/X25519 sessions through normal public client APIs. Its Java trajectory explicitly requires the client to respond to the router LeaseSet request with Standard LeaseSet2/decryption material and reach usable state.

The landed Plan 170 status instead records:

- Java `I2PSession.connect()` was bypassed;
- the Java counted driver uses lower-level `net.i2p.data.i2cp` wire primitives;
- no LeaseSet2 install occurs for the external sessions.

The Plan 170 external run remains valid evidence for independent framing/version/session-message/data-plane compatibility and digest-matched Java↔Go application traffic. It is not sufficient evidence for the stronger LeaseSet2 lifecycle criterion.

## Research disposition

The corrective is feasible without changing the environment contract.

Official I2CP behavior requires the router to request a LeaseSet after inbound tunnels exist; the client responds with a signed LeaseSet and only then is the destination usable. Official I2P tunnel documentation defines a zero-hop tunnel as one where gateway and endpoint are the same router. I2CP exposes `inbound.allowZeroHop` / `outbound.allowZeroHop` and allows client tunnel length zero.

The exact-pinned Java I2P 2.13.0 client waits in `I2PSession.connect()` for a LeaseSet and its normal RequestLeaseSet handler constructs/signs Standard LeaseSet2 from the router-supplied leases. The exact-pinned Java router implements zero-hop as a local fallback rather than a remote tunnel build.

The exact-pinned go-i2cp client transitions from SessionStatus Created to awaiting RequestVariableLeaseSet; its normal ProcessIO handler constructs/signs Standard LeaseSet2 and sends CreateLeaseSet2. Its `CreateSessionSync()` convenience return is therefore not itself proof of LeaseSet completion.

Current i2pr Plan 166 already has the required atomic client-signed LeaseSet2 + X25519 decryption-capability validation/install path, including non-empty real-pool lease ownership checks. The missing component is a legitimate local zero-hop destination tunnel representation and composition into the I2CP session lifecycle.

Current i2pr remote tunnel types deliberately reject empty hop lists and Plan 165 rejects zero-hop options. Plan 172 must preserve those remote invariants and add an explicit local zero-hop kind; it must not make empty remote `EstablishedMaterial` legal.

## Source floor

Plan registered from `main` source floor:

```text
8000ad23e033dc261a98395d6f808551c05fc078
```

That head was routine-CI green. Plan 170's retained exact-head external evidence was run `34305769352` against `d9fff42439d86d8b0c04400f463bde710103b2b7`; the later `8000ad23...` change was status-documentation only.

## Required closure transition

Only after Plan 172's criteria pass may authority become:

```text
plan_170_external_wire_data_plane = retained-passed
plan_172 = passed-m9-i2cp-independent-leaseset2-lifecycle-corrective
milestone9_i2cp_independent_clients = passed-via-plan170-and-plan172
milestone9_i2cp_independent_leaseset2 = passed-via-plan172
milestone9_final_acceptance = closed-via-plan172
next_executable_plan = none (milestone10-planning next)
next_product_layer = milestone10-planning
```

## Handoff

Execute [`plans/172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md`](172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md).

The narrow implementation sequence is:

```text
explicit local zero-hop tunnel kind
  -> destination pool / zero-hop option projection
  -> non-empty real RequestVariableLeaseSet
  -> existing Plan 166 CreateLeaseSet2 atomic install
  -> high-level Java I2PSession + public go-i2cp lifecycle proof
  -> post-install cross-client application traffic
  -> exact-head routine + manual external closure
```

No Milestone 10 work belongs in Plan 172.