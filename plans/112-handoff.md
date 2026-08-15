# Plan 112 handoff

- Status: **ready for implementation**
- Date: 2026-08-15
- Plan-of-record: `plans/112-outbound-short-build-pre-delivery-closure.md`
- Parent roadmap: `plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md`
- Scope: outbound short-build local pre-delivery closure
- External network gate: **none**

## Current authority

Plan 111's cryptographic core is retained. A post-closure audit confirmed a small set of outer-surface issues that must be corrected before the first outbound independent-router delivery attempt:

```text
request plaintext padding          = zero-filled; must use injected CSPRNG
reply plaintext padding            = zero-filled; must use injected CSPRNG
outbound role topology             = unvalidated
inbound role topology              = unvalidated
production inbound builder         = claimed-disabled-but-not-explicitly-gated
HopCryptoContext ephemeral accessor = wrong-envelope-slice
STBM action payload contract       = count-prefix docs/derivation inconsistent
OTBRM reply-event contract         = count-prefix docs inconsistent
fixed-vector generator provenance  = stale/non-reproducible
```

The next executable implementation is **Plan 112 only**.

Plan 113 is already written for the separate inbound standards/reference discrepancy and does not block outbound progress after Plan 112.

## Research-backed constraints

Pinned independent sources used for this handoff:

```text
Java I2P master = 498488b0d01d9f59efe906424e56ff5e25f58a4d (2026-08-14)
i2pd openssl    = dfcb8a8043c0c689e5681c5ae5da89df5643347e (2026-08-14)
```

Final-spec authority:

`https://i2p.net/en/docs/specs/tunnel-creation-ecies/`

Key conclusions:

- final spec requires random request and reply padding;
- Java I2P randomizes both;
- current i2pd zero-fills both, so i2pd is not authority for this detail;
- both Java I2P and i2pd agree on remote-hop role topology;
- both Java I2P and i2pd implement the inbound originator fake as `hash16 || fresh X25519 pub32 || remainder`;
- do not invent an extra inbound plaintext key field in Plan 112.

## Scope guard

Plan 112 does not authorize:

- NTCP2 activation or repair;
- SSU2;
- live Java/i2pd/Emissary execution;
- public I2P access;
- Python additions;
- Docker/Multipass/root/namespaces;
- generic I2NP dispatch;
- transit tunnel data plane;
- inbound production enablement.

## Required successful handoff state

```text
plan_112                       = passed-outbound-pre-delivery-closure
plan_111_crypto                = retained
request_padding                = random-injected-csprng
reply_padding                  = random-injected-csprng
outbound_topology              = validated
inbound_topology               = structurally-validated-production-disabled
production_inbound_builder     = typed-fail-closed-pending-plan113
hop_context_ephemeral_accessor = corrected-or-removed
stbm_payload_contract          = exact-count-prefixed
otbrm_payload_contract         = exact-count-prefixed
fixed_vector_reference         = reproducible-rust-only
outbound_short_build           = locally-conformant-pre-delivery
outbound_external_delivery     = next-qualified-checkpoint
inbound_short_build            = blocked-on-plan113
normal_daemon_ntcp2            = disabled-and-unenableable
```

Do not start the outbound external-delivery checkpoint until all Plan 112 acceptance criteria pass.