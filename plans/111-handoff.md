# Plan 111 handoff

- Status: **ready for implementation**
- Date: 2026-08-15
- Plan-of-record: `plans/111-short-build-final-local-conformance-correction.md`
- Scope: final local short-build conformance correction
- External network gate: **none**

## Current authority

A post-Plan-110 specification audit found that the architecture and most short-record/multi-record structure landed, but the active conformance claims must remain reopened until Plan 111 corrects the remaining deterministic protocol defects.

Known remaining corrections:

```text
Noise null-prologue MixHash         = missing
request es HKDF split               = incorrect-second-HKDF
record-slot nonce/IV                = incorrect-byte11-must-be-byte4
OBEP garlic reply tag               = incorrect-16-must-be-8
inbound creator ephemeral plaintext = missing
per-hop receive/next tunnel IDs     = missing-or-synthesized
hop responder role                  = flattened-to-participant
fixed independent conformance oracle = insufficient
```

The next executable implementation is **Plan 111 only**.

Do not start the external-delivery checkpoint until Plan 111 closes successfully.

## Scope guard

Plan 111 does not authorize:

- NTCP2 activation/repair;
- SSU2;
- live i2pd/Java/Emissary execution as a closure gate;
- public I2P access;
- namespaces/containers/root;
- Python interoperability machinery;
- generic I2NP dispatch;
- transit tunnels or tunnel data plane.

## Expected successful handoff state

```text
plan_111                           = passed-final-local-short-build-conformance
noise_n_request_transcript         = locally-conformant-fixed-vectors
record_slot_nonce_iv               = locally-conformant-byte4
obep_garlic_material               = locally-conformant-32-key-8-tag
inbound_creator_ephemeral          = locally-conformant
per_hop_tunnel_ids                 = explicit-and-validated
short_build_multirecord_processing = locally-conformant-fixed-vectors
complete_stbm_payload              = locally-conformant-fixed-vectors
external_build_delivery            = next-checkpoint
live_mixed_router_build            = blocked-on-qualified-delivery
normal_daemon_ntcp2                 = disabled-and-unenableable
```

If inbound creator-key placement cannot be resolved from the final spec plus a current reference implementation, Plan 111 must stop as `blocked-inbound-layout-ambiguity` rather than inventing a wire layout.
