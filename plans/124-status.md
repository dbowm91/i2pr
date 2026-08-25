# Plan 124 status — final local closure (restored by Plans 126/127)

## Current authority

- Status: **`passed-corrected-destination-routing-local-closure`**.
- Final closure: **2026-08-25** by the Plan 127 destination-session
  routing final closure, confirmed by the Plan 129 integrated
  Milestone 6 local-product gate.
- Milestone 6 authority: `126-129-milestone6-final-corrective-roadmap.md`.

## What remains authoritative

Plan 124 corrected a real and important defect:

```text
before: outbound tunnel received plaintext inner I2NP Data
now:    outbound tunnel receives standard-encoded I2NP Garlic carrying encrypted destination bytes
```

The byte-identity regression proving `garlic_i2np_bytes` crosses the
outbound tunnel and is not equal to `inner_envelope_bytes` remains
authoritative and must not regress. The complete reverse/session
lifecycle that the original acceptance required (bound NS, NSR,
bidirectional Existing Session over the destination tunnel path) was
subsequently proven by Plan 127 and re-proven with Streaming payloads
by Plan 129.

## Current classification

```text
plan_124 = passed-corrected-destination-routing-local-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
next_product_layer = SAM baseline planning (Milestone 7)
```

The only permitted network omission remains
`authenticated-router-link-bypassed-local-seam` after real OBEP
processing.
