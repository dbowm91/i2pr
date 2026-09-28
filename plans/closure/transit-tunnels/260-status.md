# Plan 260 status — registered ready

Status:
**registered-m11-creator-owned-inbound-receipt-topology-and-planning-authority-corrective-ready**

Plan:
`plans/implementation/transit-tunnels/260-m11-creator-owned-inbound-receipt-topology-and-planning-authority-corrective.md`

Baseline:
`a545f09c463c03bfbac20a31c7f5bfa77d351da5`

Corrects current interpretation of:
- `plans/closure/transit-tunnels/259-status.md`

## Registration basis

Plan 259's retained topology inventory and transit-endpoint source reading remain useful, but
post-closure review identified an endpoint-class conflation. Exact-pinned i2pd constructs
creator-owned inbound tunnels differently from transit endpoints: the last remote inbound hop
forwards to the creator router with the endpoint flag cleared, the creator's local
`InboundTunnel` sets `msg->from` to the pool-owned tunnel, and LOCAL garlic dispatch then
selects `TunnelPool::ProcessGarlicMessage`.

The historical A-ending Plan 259 chain did not prove that its next tunnel id was such a
pool-owned receiver inbound tunnel. Therefore the IBGW receipt row cannot be retired as
unexhibitable on Plan 259 evidence.

Plan 260 is registered to test the actual creator-owned inbound topology, restore the receipt
gate, harden fragmented IBGW message ids, and reconcile planning authority.

## No implementation claim

Registration does not claim Plan 260 implementation or M11 qualification.

M11 remains non-advertised/unclaimed. M12 remains deferred.
