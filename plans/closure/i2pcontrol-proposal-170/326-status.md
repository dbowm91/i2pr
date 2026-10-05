# Plan 326 — Encrypted LeaseSet2 and client authorization disposition

Status: **blocked-prop170-encrypted-leaseset-awaiting-qualified-red25519-provider**

Implementation commits: none. Plans 323 and 324 are passed; Plan 325's provider qualification closed blocked.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Plan 323 canonical non-deep TunnelManager owner is closed. | `plans/closure/i2pcontrol-proposal-170/323-status.md` | Passed. |
| Plan 324 supplies typed SigType/EncType identity policy. | `plans/closure/i2pcontrol-proposal-170/324-status.md` | Passed for Ed25519 type 7 and active LeaseSet2 X25519 type 4. |
| I2P-compatible Red25519 provider supports required signing, verification, and key blinding operations. | `plans/closure/i2pcontrol-proposal-170/325-status.md` | Blocked: no qualified maintained provider was found. |
| Type-5 encrypted/blinded LeaseSet2, all ten encryption modes, and per-client PSK/DH authorization have real publication and lookup owners. | Plan 326 implementation requirements; current NetDB/client publication owners | Not implemented; cryptographic prerequisite is unsatisfied. |

## Verification, compatibility, and security

This is a dependency closure only; no implementation or dependencies were added, and no Plan 326 tests were run. The branch's current local workspace floor is green but does not provide encrypted LeaseSet evidence. Ordinary Ed25519 is not treated as a substitute for Red25519. No secrets, identity formats, or network behavior changed.

Findings by severity: critical 0; high 0; medium 0; low 0. No encrypted LeaseSet or client-authorization support is claimed.

## Roadmap disposition

Plan 326 is closed as blocked solely on Plan 325. Plans 323 and 324 no longer gate it. Reopen after a separately reviewed I2P-compatible Red25519 provider passes vectors, malformed-input, and secret-handling qualification. Plan 328 remains blocked on 322, 326, and 327.

## Correction, 2026-10-05 (dated; the original text above is preserved)

The disposition above is **stale in its named dependency**, though the status token
is not changed here.

Plan 325's provider survey was superseded by the clean-room path: Plan 329
froze the provenance and normative boundary, Plan 330 landed the independent
Red25519 implementation, Plan 332 landed the type-5 ELS2 foundation with daily
blinding, lookup secret, B33, opaque NetDB store/serve and client
publish/resolve, and Plan 333 landed PSK and DH client authorization. So
requirement 4's stated cause — "the cryptographic prerequisite is unsatisfied" —
is no longer accurate as written: the cryptographic work exists, and it exists
because the forward architecture this record was blocked on was deliberately
replaced.

What remains genuinely open is narrower and is **not** closed by this correction:
Plan 325's *provider* question, the "all ten encryption modes" breadth in
requirement 4 against what 332/333 actually cover, and the control-plane
publication evidence. Re-audit Plan 326 against Plans 330/332/333 rather than
reopening it on Plan 325. Plan 335's measured type-11 transcript incompatibility
is a separate defect on the interoperability axis and is not a Plan 326
dependency.

