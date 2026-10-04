# Plan 328 — Live external full-conformance gate disposition

Status: **blocked-prop170-full-conformance-gate-awaiting-322-326-327**

Implementation commits: none. This is a dependency disposition; no external reference lane was run.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Plan 322 canonical RouterInfo/news source completion. | `plans/closure/i2pcontrol-proposal-170/322-status.md` | Blocked on production transit and IPv6 source owners. |
| Plan 326 encrypted LeaseSet/client authorization. | `plans/closure/i2pcontrol-proposal-170/326-status.md` | Blocked on Plan 325 Red25519 provider qualification. |
| Plan 327 routed outproxy capability. | `plans/closure/i2pcontrol-proposal-170/327-status.md` | Blocked on provider and safe outbound credential owners. |
| Exact-pinned Emissary, Java Proposal 170, and i2pd differential runs. | Plan 328 reference-lane requirements | Not run because all hard dependencies are unresolved. |
| Full Proposal 170 support inventory/claim. | `specs/support.toml`; `specs/CONFORMANCE.md` | Not authorized. The repository continues to claim only its tested experimental subset. |

## Verification and security

No external tests, dependencies, product changes, or secret-bearing evidence were produced. The branch's workspace floor is local-only and cannot substitute for the external evidence gate. No findings were adjudicated; severity counts for this dependency disposition are critical 0, high 0, medium 0, low 0.

## Roadmap disposition

Plan 328 is closed as blocked, not passed. Reopen only after Plans 322, 326, and 327 each pass, then re-freeze Proposal 170 and execute every required pinned reference lane. No `full-proposal-conformant` claim is made.
