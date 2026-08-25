# Plan 121 status — final local closure (restored by Plans 126/127)

## Current authority

- Status: **`passed-corrected-ecies-destination-session-layer-local`**.
- Final closure: **2026-08-25** by the Plan 127 destination-session
  routing final closure, confirmed by the Plan 129 integrated
  Milestone 6 local-product gate.
- Original plan: `121-m6-ecies-garlic-session-layer.md`.
- Corrective plan: `126-m6-ecies-destination-ratchet-corrective-foundation.md`.
- Destination-layer closure: `127-m6-destination-session-routing-final-closure.md`.
- Integrated gate: `plans/129-status.md`.
- Milestone 6 authority: `126-129-milestone6-final-corrective-roadmap.md`.

## What carries the `local` qualifier

The original Plan 121 i2pr-internal ECIES dialect (Noise-NK
initializer, `0xE0`/`0xE2` marker bytes, clear static-key field) was
superseded and replaced by the Plan 126 normative
ECIES-X25519-AEAD-Ratchet contract. Plan 127 then proved bound sender
LeaseSet2 validation, reverse routing, NSR, and bidirectional Existing
Session traffic over actual destination tunnels. The word `local` is
mandatory: mixed-router destination ECIES interoperability remains
separate external acceptance debt.

## Current classification

```text
plan_121 = passed-corrected-ecies-destination-session-layer-local
plan_122 = passed-corrected-local-destination-routing
plan_124 = passed-corrected-destination-routing-local-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
next_product_layer = SAM baseline planning (Milestone 7)
```
