# Plan 190 status — M6 inbound NetDB reply-path tunnel-ID corrective

Status: **`registered-executable-m6-inbound-netdb-reply-path-tunnel-id-corrective`**.

Plan of record:
[`plans/190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md`](190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md).

This status supersedes older handoff prose that still says Plan 188 is directly executable. Plan 188's authenticated i2pd one-hop tunnel installs remain retained-passed, but its five destination rows are blocked on the reply-path defect isolated here. Plan 189 remains dependency-blocked and has not started Java second-family qualification.

## Current authority

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187 = blocked-destination-remote-interop (local product passed; 2/7 remote rows flipped via plan188 installs)
plan_188 = blocked-by-plan190-reply-path-corrective (real outbound/inbound i2pd installs retained-passed)
plan_189 = registered-blocked-by-plan188-and-plan190 (cross-family scaffold only; Java qualification not started)
plan_190 = registered-executable-m6-inbound-netdb-reply-path-tunnel-id-corrective

m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-pending-plan190
m6_second_family_java = not-yet-started
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 190
resume_after_plan190 = 188
```

## Why Plan 190 exists

Plan 188 resolved the short-build reply half of the first-family i2pd interop program. Against exact-pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`), both real one-hop directions now install through consumed authenticated reference replies. The external destination lane then reaches:

```text
installed_ob = 1
installed_ib = 1
outbound-lookup-via-tunnel cells = 1
reference LeaseSet2 never resolved within the bounded wait
```

Research and source inspection isolate a concrete reply-path metadata defect before any need to change NetDB, Garlic, Streaming, or the reference topology.

### Protocol/reference finding

For a tunneled `DatabaseLookup`, the I2NP contract requires:

```text
from            = RouterHash of the inbound tunnel gateway
reply_tunnelId  = receive tunnel ID on that gateway
```

Exact-pinned i2pd implements this literally in `libi2pd/NetDb.cpp`: it reads the lookup's `replyIdent` and `replyTunnelID`, then sends `CreateTunnelGatewayMsg(replyTunnelID, replyMsg)` to `replyIdent` for a tunneled response. `libi2pd/Tunnel.cpp` resolves an arriving TunnelGateway by that tunnel ID. `libi2pd/Transports.cpp` explicitly loopbacks self-addressed transport messages, so the controlled topology where the same i2pd instance is tunnel participant and floodfill is valid and is not the blocker.

Pinned reference source:

```text
https://github.com/PurpleI2P/i2pd/blob/635b013a612ff47278ef02acf8580a28e10e26c5/libi2pd/NetDb.cpp
https://github.com/PurpleI2P/i2pd/blob/635b013a612ff47278ef02acf8580a28e10e26c5/libi2pd/Tunnel.cpp
https://github.com/PurpleI2P/i2pd/blob/635b013a612ff47278ef02acf8580a28e10e26c5/libi2pd/Transports.cpp
```

Current I2NP source used for the correction:

```text
https://beta.i2p.net/en/docs/specs/i2np/
```

### i2pr defect

`i2pr-tunnel::EstablishedTunnel` already retains the two independent inbound IDs correctly:

```text
inbound_gateway       = (remote IBGW router, remote IBGW receive tunnel ID)
local_inbound_receive = local creator endpoint receive tunnel ID
```

In the controlled one-hop test:

```text
IBGW_RECEIVE = 0x9601 = 38401  # remote i2pd gateway receive ID
IBGW_NEXT    = 0x9602 = 38402  # local i2pr endpoint receive ID
```

`DataPlaneRegistry::activate_inbound` retains the gateway router hash and the local receive ID but drops the remote gateway receive tunnel ID. `destination_tunnel_external.rs` then builds `ReplyPath` from the exact-pinned i2pd RouterHash plus `registry.inbound_receive_ids()[0]`, which is the local `0x9602` ID. The resulting lookup therefore tells i2pd to inject the reply into tunnel `0x9602` on i2pd, while the installed i2pd inbound gateway expects `0x9601`.

`i2pr-netdb::ReplyPath` and `build_databaselookup()` already have the correct semantics and are not the wire defect.

## Executable correction

Plan 190 owns only this boundary:

1. preserve a typed public inbound-gateway route in `DataPlaneRegistry`, retaining:
   - gateway RouterHash;
   - remote gateway receive tunnel ID;
   - local endpoint receive tunnel ID;
2. clean that metadata up atomically with role/slot lifecycle;
3. add a daemon-owned adapter that derives `i2pr_netdb::ReplyPath` from `(gateway RouterHash, gateway receive tunnel ID)`;
4. remove the manual `RouterHash(i2pd) + inbound_receive_ids()[0]` reconstruction from the external destination driver;
5. prove unequal IDs locally (`0x9601 != 0x9602`) and round-trip the resulting `DatabaseLookup` through the existing I2NP codec;
6. rerun the existing exact-pinned external destination lane without timeout relaxation, direct delivery, `LocalZeroHop`, synthetic replies, or reference-log-derived success.

The plan deliberately does not modify short-build reply handling, SSU2, lookup wire codecs, reply encryption, Streaming, or Java second-family qualification.

## Acceptance boundary

Plan 190 is not closed from source inspection. It passes only when the exact-pinned i2pd lane proves a real lookup response returns through the corrected inbound gateway tuple and the existing LeaseSet2 validation/store path resolves the reference Standard LeaseSet2.

Minimum external transition required:

```text
reply path gateway        = exact-pinned i2pd
reply path gateway tunnel = 0x9601
local receive tunnel      = 0x9602
ids_distinct              = true
outbound lookup cells     >= 1
real inbound TunnelData   = observed through the installed inbound tunnel
reference LeaseSet2       = validated and cached
external-lease-lookup-tunnel = passed
```

All already-passed Plan 188 rows must remain passed.

If the corrected reply reaches a later boundary and fails there, Plan 190 stops and records the first newly demonstrated boundary. A new narrow follow-up then owns that defect. The correction must not absorb publication, destination ECIES/Garlic, inbound destination dispatch, or Streaming defects merely because they become visible after lookup starts working.

## Required closure record

When executed, update this file with:

- exact implementation closing SHA;
- exact routine hosted CI run on that SHA;
- exact external destination-lane run/artifact/evidence location;
- local regression counts;
- proof that `reply_tunnelId` is the remote gateway receive ID and is distinct from the local receive ID;
- `external-lease-lookup-tunnel` disposition;
- first remaining downstream blocker, if any;
- authority transition back to Plan 188 only after this corrective passes.

## Handoff

Read/execute in this order:

1. `plans/190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md`
2. `plans/190-status.md` (this file; newest authority)
3. `plans/188-status.md` for retained authenticated build/install evidence and the remaining destination rows
4. `crates/i2pr-tunnel/src/established.rs`
5. `crates/i2pr-tunnel/src/data_plane_registry.rs`
6. `crates/i2pr-daemon/src/destination_tunnels.rs`
7. `crates/i2pr-daemon/tests/destination_tunnel_unit.rs`
8. `crates/i2pr-daemon/tests/destination_tunnel_external.rs`
9. `tests/integration/m6-interop/run-destination.sh`
10. `plans/189-status.md` only after Plan 188 first-family destination + Streaming work genuinely closes

Until closure:

```text
next_executable_plan = 190
resume_after_plan190 = 188
plan_189 = blocked
milestone6_interoperable = not-yet-claimed
```
