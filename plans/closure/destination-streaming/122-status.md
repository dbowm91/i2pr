# Plan 122 status — final local closure (restored by Plans 126/127)

## Current authority

- Status: **`passed-corrected-local-destination-routing`**.
- Final closure: **2026-08-25** by the Plan 127 destination-session
  routing final closure, confirmed by the Plan 129 integrated
  Milestone 6 local-product gate.
- Milestone 6 authority: `126-129-milestone6-final-corrective-roadmap.md`.
- Corrective closures: Plan 124 (Garlic-carrier composition fix) and
  Plan 127 (bound NS/NSR/ES lifecycle over destination tunnels).

## Retained and completed work

The full Plan 122/124 product surface is closed under the corrected
ECIES protocol/session lifecycle:

- typed Standard LeaseSet2 lookup/cache/selection;
- destination-owned tunnel pools;
- canonical outbound I2NP Data construction (`OutboundRequest::new`
  remains the single Data-envelope construction owner; Plan 129
  removed the adapter's redundant duplicate);
- I2NP Garlic carrier composition (`garlic_i2np_bytes` through the
  tunnel data plane, never plaintext);
- destination dispatcher ownership mapping;
- explicit bounded local router-link seam;
- reverse routing via `install_remote_lease_set2`.

## Current classification

```text
plan_122 = passed-corrected-local-destination-routing
plan_124 = passed-corrected-destination-routing-local-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
next_product_layer = SAM baseline planning (Milestone 7)
```

The Plan 124 plaintext-tunnel regression remains a required invariant
and must stay green.
