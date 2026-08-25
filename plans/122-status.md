# Plan 122 status — provisional pending final destination-session correction

## Current authority

- Status: **`provisional-blocked-on-plan126-plan127`**.
- Milestone 6 authority: `126-129-milestone6-final-corrective-roadmap.md`.
- Lower-layer corrective prerequisite: Plan 126.
- Destination-routing final closure: Plan 127.

## Retained work

Retain the useful Plan 122/124 product surface:

- typed Standard LeaseSet2 lookup/cache/selection;
- destination-owned tunnel pools;
- outbound I2NP Data construction;
- I2NP Garlic carrier composition;
- destination dispatcher ownership mapping;
- explicit bounded local router-link seam;
- Plan 124 correction that makes the outbound tunnel carry `garlic_i2np_bytes` rather than plaintext `inner_envelope_bytes`.

## Remaining closure defects

The stronger destination-routing closure depends on the corrected ECIES protocol/session lifecycle.

Current production gaps include:

- no spec-correct bound NS/NSR/ES manager lifecycle;
- bundled sender LeaseSet2 is not yet correctly bound to the authenticated NS static key;
- accepted sender LS2 bookkeeping/reverse-routing handoff is incomplete;
- the full B -> A NSR and bidirectional Existing Session trajectory over the destination tunnel path is not established.

## Current classification

```text
plan_121 = corrective-reopened-plan126
plan_122 = provisional-blocked-on-plan126-plan127
plan_124 = primary-composition-fix-retained-full-closure-reopened-plan127
milestone6_local_product = not-closed
next = plans/126-m6-ecies-destination-ratchet-corrective-foundation.md
then = plans/127-m6-destination-session-routing-final-closure.md
```

The Plan 124 plaintext-tunnel regression remains a required invariant and must stay green.