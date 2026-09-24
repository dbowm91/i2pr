# Plan 250 status — M11 transit foundation semantic and ownership corrective

Status: **registered-ready-m11-transit-foundation-semantic-and-ownership-corrective**

Plan of record:
`plans/implementation/transit-tunnels/250-m11-transit-foundation-semantic-and-ownership-corrective.md`

Baseline: `958c06171a6d60dc3d1866ed8b7d93937d001d6f`

## Registration basis

Post-Plan-249 source review found blocking semantic defects before any daemon consumer was
written: wrong previous-peer provenance, wrong accepted reply Mapping, no sealed policy
rejection outcome, reversed time window, fake pending accounting, Clone secret owners,
panic-capable removal, and proxy closure tests.

The architecture is retained. M11 capability remains unclaimed.

## Unblock rule

Plan 252 daemon composition remains unregistered until Plan 250 and independent Plan 251
ordinary-CI corrective both close and ordinary GitHub CI is green.

Compilation or documentation-only changes are not sufficient for closure.
