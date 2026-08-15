# Plan 111 handoff

- Historical implementation status: **landed**
- Current authority status: **superseded by post-closure audit**
- Date: 2026-08-15
- Plan-of-record: `plans/111-short-build-final-local-conformance-correction.md`
- Closure record: `plans/111-status.md`
- Active amendment: `plans/111-post-closure-audit-amendment.md`
- Successor roadmap: `plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md`
- Next executable plan: `plans/112-outbound-short-build-pre-delivery-closure.md`

## Historical Plan 111 result

Plan 111 landed the intended high-impact cryptographic corrections:

```text
Noise null-prologue MixHash         = corrected
request es HKDF split               = corrected-single-HKDF
record-slot nonce/IV                = corrected-byte4
OBEP garlic reply tag               = corrected-to-8
per-hop receive/next tunnel IDs     = explicit
hop responder role                  = authenticated-role-aware
fixed cryptographic vectors         = committed
```

These corrections remain retained.

## Post-closure audit

A subsequent audit against the current final I2P Tunnel Creation Specification plus current Java I2P and i2pd source found additional local pre-delivery issues:

```text
request plaintext padding           = zero-filled; Plan112
reply plaintext padding             = zero-filled; Plan112
direction/role topology             = not validated; Plan112
production inbound gate             = not explicit; Plan112
HopCryptoContext ephemeral accessor = wrong slice; Plan112
STBM/OTBRM payload API contract      = count-prefix inconsistency; Plan112
fixed-vector generator provenance   = stale/non-reproducible; Plan112
inbound creator-key interpretation  = spec/reference discrepancy; Plan113
```

Therefore the old instruction to proceed directly from Plan 111 to external delivery is superseded.

## Current handoff

The next executable implementation is **Plan 112 only**.

Plan 112 is the mandatory local blocker before the first outbound independent-router delivery attempt.

Plan 113 is a separate inbound semantics reconciliation and does not block outbound delivery after Plan 112.

Do not start a broad transport/harness program from this historical handoff.

## Current expected sequence

```text
Plan111 core retained
 -> Plan112 outbound pre-delivery closure
 -> narrow outbound qualified external-delivery checkpoint

Plan111 inbound ambiguity
 -> Plan113 inbound spec/reference reconciliation
 -> later inbound delivery checkpoint if enabled
```

See `plans/102-amendment-exploratory-tunnel-dependency.md` for current authority precedence.