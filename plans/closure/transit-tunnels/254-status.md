# Plan 254 status — M11 live ingress/body-threading closure corrective

Status: **registered-ready-m11-live-ingress-body-threading-closure-corrective**

Plan of record:
`plans/implementation/transit-tunnels/254-m11-live-ingress-body-threading-closure-corrective.md`

Baseline:
`6a53efaeed30087b3ef133028f7243897d72afba`

## Registration basis

Post-closure audit of Plan 253 found that several corrective pieces landed, but the live
owner acceptance boundary did not:

- `TransitOwner` is not called by the actual authenticated SSU2/router-I2NP inbound owner;
- `plan253_short_build_payload` returns an empty slice;
- the new integration test constructs `TransitOwner` directly instead of traversing the
  real inbound owner;
- OBEP semantic delivery and IBGW live TunnelGateway ingress are not completed through the
  live daemon owner;
- `TransitOwner::dispatch_short_build` creates a fresh cancellation token rather than using
  the owner token;
- current-SHA Actions run `36089834582` failed formatting on Ubuntu and macOS;
- registry/support projections still name Plan 253 as next executable despite its claimed
  closure.

The successful Plan 253 runtime-neutral data-plane, envelope, rollback, bounded-peer,
cancellation-drain, and secret-ownership work is retained.

## Gate

Plan 255 exact-pinned i2pd qualification remains unregistered until this corrective closes
with direct production-owner tests and green exact-SHA CI.

M11 capability remains unclaimed and `advertised=false`.
