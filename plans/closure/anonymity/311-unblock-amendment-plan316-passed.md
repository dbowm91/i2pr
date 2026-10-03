# Plan 311 status amendment — Plan 316 lifecycle integration passed

Status: `ready-lifecycle-acceptance-after-plan316`

Date: 2026-10-03

Plan of record: [`plans/implementation/anonymity/311-service-lifecycle-startup-and-graceful-drain.md`](../../implementation/anonymity/311-service-lifecycle-startup-and-graceful-drain.md).

Parent status authority: [`plans/closure/anonymity/311-status.md`](311-status.md).

Corrective integration closure: [`plans/closure/anonymity/316-status-amendment-lifecycle-passed.md`](316-status-amendment-lifecycle-passed.md).

## Status correction

The original Plan 311 blocked closure remains an accurate record of its missing production consumer and shutdown owner. It is not rewritten. Plan 316 has since integrated the normal-daemon group provider, readiness gate, inbound-first startup, separate admission/runtime cancellation, and bounded group retirement into the production daemon.

Plan 311 is now ready to execute its lifecycle acceptance target against that production path. Plan 316's local implementation and full workspace evidence do not make an external anonymity or hostile-timing claim.

## Dependency audit

- Plan 309: passed; Destination-group identity and ownership contract is stable.
- Plan 315: passed; canonical group pools and their consumers are available.
- ADR 0030: accepted; lifecycle separation and scope are defined.
- Plan 316: passed; the normal-daemon lifecycle integration prerequisite is available.
- Plan 311's blocked closure is preserved as historical evidence.

Plan 311 moves to `ready`. Plan 308 remains independently blocked on controlled ordinary-HTTP topology evidence. Plan 313 remains independently ready after Plan 312. Plan 317 remains the immutable blocked qualification-owner attempt. No other plan changes state.
