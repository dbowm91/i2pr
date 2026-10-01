# Plan 281 status — passed type-5 support-floor correction

- Plan: `plans/implementation/floodfill/281-m12-encrypted-leaseset-floor-correction.md`
- Parent: Plan 280 provider review, stopped without an acceptable candidate.
- Disposition: **passed**. ADR 0027 and Plan 272 now defer EncryptedLeaseSet type 5 until a
  future provider plan passes. No crypto, codec, wire, storage, runtime, or advertisement change.

## Evidence

The ADR and Plan 272 agree on RouterInfo and DatabaseStore types 1, 3, and 7 as the initial
validated record floor. Type 5 stays deferred/unsupported and cannot enter server-authority
storage, be answered, persisted, or replicated. The candidate comparison and official algorithm
source are recorded in Plan 280's closure and `specs/SOURCES.md`.

## Unblock audit

Plan 272 is the only newly ready plan, with both provenance Plan 271 and this support-floor
correction passed. Plans 273–279 remain blocked in order. Plan 280 remains stopped, not passed;
future type-5 support requires a separately registered vetted-provider plan. No other plan became
eligible.

## Verification

- `rtk git diff --check` — passed.
- Python `tomllib` parse of `specs/support.toml` — passed.
- No production code or dependency changes.
