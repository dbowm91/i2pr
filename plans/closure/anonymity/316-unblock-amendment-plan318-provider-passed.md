# Plan 316 status amendment — Plan 318 provider closure

Status: **ready — normal-daemon group provider is available**

Date: 2026-10-03

Plan of record: [`plans/implementation/anonymity/316-daemon-owned-service-group-lifecycle-integration.md`](../../implementation/anonymity/316-daemon-owned-service-group-lifecycle-integration.md).

Parent status authority: [`plans/closure/anonymity/316-status.md`](316-status.md).

Provider closure: [`plans/closure/anonymity/318-status.md`](318-status.md).

## Status correction

The original Plan 316 closure remains an accurate record of its failed attempt: at that time the normal daemon had no single-owner Destination-group provider. It is not rewritten. Plan 318 has since supplied that provider and closed the missing integration boundary.

The normal daemon now passes its existing SSU2 handle, runtime child scope, cancellation token, and a bounded validated-bootstrap snapshot to the group product. Group builds and Plan 315 pools use the same SSU2 owner and the sole normal inbound pump. Readiness gates service supervisors, and startup traffic consumed while awaiting build replies is replayed through the normal router/floodfill dispatcher. Plan 318's closure records the implementation and verification evidence.

## Dependency audit

- Plan 315: passed; the canonical group pool and Destination consumers are available.
- ADR 0030: accepted; the group/linkability and lifecycle boundaries are stable.
- Plan 311: its blocked closure is recorded and remains the lifecycle acceptance target.
- Plan 318: passed; the production single-owner group provider is now composed into the normal daemon.

Plan 316 is therefore `ready` to implement the remaining bounded graceful-drain and lifecycle requirements. Plan 311 remains blocked pending Plan 316. Plan 317 remains a historical blocked provider attempt, superseded for provider work by Plan 318. Plan 308 remains independently blocked; Plan 313 remains independently ready. No other plan is unblocked by this amendment.
