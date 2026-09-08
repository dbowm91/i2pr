# Plan 171 status — Milestone 9 I2CP invalid-preamble close and exact-head CI corrective

Status: **`active-m9-i2cp-invalid-preamble-close-and-ci-corrective`**.

Registered: **2026-09-08**.

Plan of record:
[`plans/171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md`](171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md).

## Current authority

```text
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
plan_165 = passed-m9-i2cp-connection-session-and-options
plan_166 = passed-m9-i2cp-client-owned-destination-and-leaseset2
plan_167 = passed-m9-i2cp-loopback-server-runtime
plan_168 = passed-m9-i2cp-message-data-plane
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
plan_170 = blocked-by-plan171
plan_171 = active-m9-i2cp-invalid-preamble-close-and-ci-corrective

milestone9_i2cp_local_product = passed-via-plan169
milestone9_final_acceptance = not-yet-closed
next_executable_plan = 171
resume_after_plan171 = 170
next_product_layer = milestone9-i2cp
```

## Trigger evidence

Current head when Plan 171 was registered:

```text
4274b8a6b8198c1daad6caea020fdbffa530840a
```

Routine CI run:

```text
34202991918
Quality (ubuntu-latest) = success
Quality (macos-latest)  = failure
MSRV (Ubuntu)           = success
Dependency policy       = success
```

The macOS failure is:

```text
crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs
wrong_protocol_byte_is_closed ... FAILED
expected close, got timeout
```

The test sends `0x00` instead of the required I2CP protocol byte `0x2a`. The server's inner connection state correctly returns `InvalidProtocolByte`; the missing proof is deterministic peer-visible TCP termination and cleanup on the common terminal path.

## Prior correction audit

Plan 169 closing commit `2fecc64f6f7143ac6ef560ee31264b2cfa78b841` was titled `accept immediate-close or timeout in adversarial tests`, but its patch only changed:

- `oversized_frame_length_rejected_before_body_allocation`;
- `destroy_session_before_create_session_is_rejected`.

It did not change `wrong_protocol_byte_is_closed`. The current failure is therefore not contradicted by that patch; the wrong-preamble close contract remains unresolved on the current authoritative head.

## Handoff

Execute **Plan 171**. Do not execute Plan 170 or begin Milestone 10 planning until this corrective has an explicit passing closure record with exact-head Ubuntu/macOS CI green.

Expected post-closure authority:

```text
plan_171 = passed-m9-i2cp-invalid-preamble-close-and-ci-corrective
plan_170 = ready-m9-i2cp-independent-clients-and-final-closure
next_executable_plan = 170
milestone9_final_acceptance = not-yet-closed
```