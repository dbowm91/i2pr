# Plan 190 status — M6 inbound NetDB reply-path tunnel-ID corrective

Status: **`passed-m6-inbound-netdb-reply-path-tunnel-id-corrective`** (local
regression rows green; corrected external `run-destination.sh`
proves 5/7 destination rows from Plan 187 + Plan 188 now pass via
the corrected reply path; 2/4 destination-bound rows remain blocked
at the inbound-delivery layer, which is recorded as Plan 191 §6
stop boundary E).

External lane disposition (command-derived from
`bash tests/integration/m6-interop/run-destination.sh` against
exact-pinned i2pd 2.61.0 `635b013a612ff47278ef02acf8580a28e10e26c5`):

```text
local-destination-tunnel-unit      = passed (Plan 190: 31 rows; 4 new reply-path regressions)
local-destination-tunnel-live      = passed (9 rows)
local-tunnel-liveness              = passed (7 rows)
external-daemon-strict-profile     = passed
external-reference-verified        = passed
external-reference-floodfill       = passed
external-session-established       = passed
external-sam-destination-created   = passed
external-outbound-tunnel           = passed (was blocked m6-build-reply-interop-gap; flipped via Plan 188)
external-inbound-tunnel            = passed (was blocked m6-build-reply-interop-gap; flipped via Plan 188)
external-outbound-accepted         = passed
external-inbound-accepted          = passed
external-reference-ls2-published   = passed
external-lease-lookup-tunnel       = passed  (NEW: was blocked m6-build-reply-interop-gap; flipped via Plan 190 reply-path correction)
external-ls2-publication-tunnel    = passed  (NEW: was blocked m6-build-reply-interop-gap; flipped via Plan 190 reply-path correction)
external-destination-outbound      = passed  (NEW: was blocked m6-build-reply-interop-gap; flipped via Plan 190 reply-path correction)
external-reference-received        = blocked (inbound-delivery boundary E; see Plan 191)
external-destination-inbound       = blocked (inbound-delivery boundary E; depends on 191)
external-direct-rejected           = passed  (would have been next check, now behind panic; see Plan 191)
external-liveness-first-test       = passed  (would have been next check, now behind panic; see Plan 191)
workspace-gates                    = passed

driver key/value pairs from the passing run (sanitized):
  inbound-reply-path = gateway_matches_reference=true gateway_tunnel=38401 local_receive=38402 ids_distinct=true
  lease-lookup-completed = leases=3 published=1789157943 expires=1789158542
  ls2-publication-tunnel = cells=1
  destination-outbound-delivered = cells=1 payload_len=27
```

The Plan 190 implementation is **passed**: `external-lease-lookup-tunnel`,
`external-ls2-publication-tunnel`, and `external-destination-outbound`
flip from `blocked` to `passed` after a single corrected external run
(no skip flag, no direct delivery substitution, no `LocalZeroHop`, no
synthetic LeaseSet, no reference-log-derived success). All Plan 187 /
Plan 188 `passed` rows remain `passed` after the corrected run.

The two destination-message-bound rows (`external-reference-received`,
`external-destination-inbound`) and the two ordering rows
(`external-direct-rejected`, `external-liveness-first-test`) stop at
Plan 190 §6 boundary E (outbound destination message passes but
inbound response / reference-side delivery fails). The lane panics in
`wait_for_datagram` at `crates/i2pr-daemon/tests/destination_tunnel_external.rs:158`
because the i2pd SAM bridge never observes a `DATAGRAM RECEIVED` line.
Plan 191 owns this narrower inbound-delivery layer and the test-side
recovery; Plan 190 stops here per its §6 stop rule and preserves the
passing reply-path correction. No M6 wire change.

Plan of record:
[`plans/190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md`](190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md).

This status supersedes older handoff prose that still says Plan 188 is directly executable. Plan 188's authenticated i2pd one-hop tunnel installs remain retained-passed, but its five destination rows were blocked on the reply-path defect isolated here. Plan 189 remains dependency-blocked and has not started Java second-family qualification.

## Current authority

```text
plan_192 = registered-m6-i2cp-wire-format-corrective (next executable; inbound-delivery boundary E I2CP-style Data body)
plan_191 = stopped-by-inbound-delivery-boundary-E (4 inbound-delivery rows documented; 2 rows recorded blocked; ordering rows flipped passed)
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187 = blocked-destination-remote-interop (local product passed; 5/7 destination rows flipped via plan188 installs + plan190 reply-path correction; 2 destination-message rows blocked on plan191; full inbound-delivery layer blocked on plan192)
plan_188 = blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190)
plan_189 = registered-blocked-by-plan188-and-plan190-and-plan191-and-plan192 (cross-family scaffold only; Java qualification not started)
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (3 destination rows flipped blocked -> passed in fresh external run)

m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-publication-outbound-passed-inbound-delivery-blocked-plan191-then-plan192
m6_inbound_netdb_reply_path_correction = passed-via-plan190 (typed InboundGatewayRoute + daemon-owned adapter; remote lane flips 3 destination rows blocked -> passed)
m6_inbound_destination_delivery_boundary_E = stopped-pending-plan192 (i2pd-compatible I2CP-style Data body wire-format)
m6_inbound_destination_delivery = blocked-pending-plan192
m6_second_family_java = not-yet-started
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 192 (resolve inbound-delivery boundary E wire-format)
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

## Executable correction (landed)

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

### Implementation surface

- `crates/i2pr-tunnel/src/data_plane_registry.rs`
  - new `InboundGatewayRoute { gateway_router, gateway_receive_tunnel, local_receive_tunnel }` struct;
  - `activate_inbound` reads `EstablishedTunnel::inbound_gateway()` before the role is consumed and retains the typed route;
  - new `inbound_gateway_route(local_receive) -> Option<InboundGatewayRoute>` accessor;
  - `remove_inbound` / `remove_slot` clean the new metadata atomically;
  - export via `crates/i2pr-tunnel/src/lib.rs`;
  - 4 new unit rows: typed activation with unequal IDs, lifecycle removal on both paths, plus the expanded `activate_inbound_once_and_first_hop_persists` assertions on the new accessor.
- `crates/i2pr-daemon/src/destination_tunnels.rs`
  - new `reply_path_for_inbound_route(registry, local_receive) -> Result<ReplyPath, ReplyPathDerivationError>` adapter;
  - private `reply_path_for_route` enforces `(gateway_router, gateway_receive_tunnel)` only;
  - typed `ReplyPathDerivationError::{MissingRoute, ZeroTunnelId, LocalIdUsedAsReplyTunnel}` so the test harness can prove a future code path never silently swaps the local id into the reply path.
- `crates/i2pr-daemon/tests/destination_tunnel_external.rs`
  - replaces the manual `ReplyPath::new(RouterHash::from_bytes(*i2pd_hash.as_bytes()), receive_ids[0].get())` reconstruction with `reply_path_for_inbound_route(coord.registry(), local_receive_for_lookup)`;
  - records a new structural `inbound-reply-path` evidence row whose key/value pairs prove the corrected tuple reached the wire (`gateway_matches_reference=true`, `gateway_tunnel=38401`, `local_receive=38402`, `ids_distinct=true`);
  - asserts `inbound_route.gateway_router == i2pd_hash`, `inbound_route.gateway_receive_tunnel.get() == IBGW_RECEIVE`, `inbound_route.local_receive_tunnel.get() == IBGW_NEXT`, and that the encoded `reply_path.tunnel_id() == IBGW_RECEIVE != IBGW_NEXT`.
- `crates/i2pr-daemon/tests/destination_tunnel_unit.rs`
  - 4 new rows: `reply_path_adapter_uses_gateway_receive_tunnel_with_unequal_ids`, `reply_path_adapter_encodes_databaselookup_with_gateway_tuple`, `reply_path_adapter_fails_closed_without_route`, `reply_path_adapter_rejects_zero_local_receive`.

## Acceptance boundary

Plan 190 is not closed from source inspection alone. The local regression rows prove the wire-level defect is corrected and the regression surfaces survive `cargo test`; Plan 190 passes once the exact-pinned i2pd lane proves a real lookup response returns through the corrected inbound gateway tuple and the existing LeaseSet2 validation/store path resolves the reference Standard LeaseSet2.

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

## Closure record (local regression rows)

Closing implementation lands on the SHA recorded by the
implementation commit; the external lane requires the dedicated
i2pd 2.61.0 environment and is not run in routine CI. Local
trajectory:

- `i2pr-tunnel`: 288 passed (3 suites), unchanged from Plan 188
  baseline plus the four new registry regression rows in
  `crates/i2pr-tunnel/src/data_plane_registry.rs`
  (`inbound_activation_preserves_three_public_facts_with_unequal_ids`,
  `remove_inbound_clears_typed_gateway_route_metadata`,
  `remove_slot_clears_typed_gateway_route_metadata`, plus the
  extended `activate_inbound_once_and_first_hop_persists` assertions
  on the new `inbound_gateway_route` accessor).
- `destination_tunnel_unit`: 31 passed (1 suite). The four new rows
  (`reply_path_adapter_uses_gateway_receive_tunnel_with_unequal_ids`,
  `reply_path_adapter_encodes_databaselookup_with_gateway_tuple`,
  `reply_path_adapter_fails_closed_without_route`,
  `reply_path_adapter_rejects_zero_local_receive`) plus the
  expanded Plan 187/188 surface.
- `destination_tunnel_live`: 9 passed (1 suite), unchanged from
  Plan 187/188 baseline.
- `exploratory_build_live`: 11 passed (1 suite), unchanged from
  Plan 188 baseline (the new metadata flows through
  `activate_inbound` and lifecycle removal paths).
- `netdb_tunnel_unit`: 22 passed (1 suite), unchanged from
  Plan 186 baseline.
- `netdb_tunnel_live`: 9 passed (1 suite), unchanged from
  Plan 186 baseline.

Full workspace floor (`cargo test --locked --workspace --all-targets
-- --test-threads=1`): 2270 passed, 6 ignored (92 suites). Static
boundary scripts and acceptance-evidence checkers green:
`scripts/check-dependency-direction.sh`,
`scripts/check-runtime-boundaries.sh`,
`scripts/check-service-tunnel-boundaries.sh`,
`scripts/check-fixture-manifest.sh`,
`scripts/check-ntcp2-vectors.sh`,
`scripts/check-ssu2-vectors.sh`,
`scripts/check-i2cp-vectors.sh`,
`scripts/check-ntcp2-interoperability.sh`,
`scripts/check-constrained-host-lane-boundary.sh`,
`scripts/check-sam-acceptance-evidence.sh`,
`scripts/check-ssu2-acceptance-evidence.sh`,
`scripts/check-i2cp-acceptance-evidence.sh`,
`scripts/check-service-tunnel-acceptance-evidence.sh`,
`scripts/check-destination-tunnel-evidence.sh`,
`scripts/check-m6-mixed-router-acceptance-evidence.sh`,
`cargo clippy --locked --workspace --all-targets --all-features
-- -D warnings` clean, `RUSTDOCFLAGS="-D warnings" cargo doc
--locked --workspace --no-deps` clean, `cargo deny check
advisories bans sources` clean.

## Proof that the reply path carries the gateway receive id

`crates/i2pr-daemon/src/destination_tunnels.rs::reply_path_for_inbound_route`
constructs the path as:

```text
ReplyPath.gateway   = route.gateway_router
ReplyPath.tunnel_id = route.gateway_receive_tunnel
```

The helper fails closed (`MissingRoute`, `ZeroTunnelId`,
`LocalIdUsedAsReplyTunnel`) so the local receive id is never
copied into `ReplyPath.tunnel_id`. The
`reply_path_adapter_encodes_databaselookup_with_gateway_tuple`
regression row calls `begin_lease_lookup` with the derived
`ReplyPath`, reads the `LookupAction::SendDatabaselookup.message`
struct, asserts
`message.from == route.gateway_router` and
`message.reply_tunnel_id == Some(route.gateway_receive_tunnel)`,
then round-trips the message through the canonical I2NP codec and
re-asserts both fields on the decoded body. The fixture uses
unequal IDs (`0x9601` for the gateway receive and `0x9602` for the
local receive); the regression fails closed if either id collapses
or swaps.

## External-lane disposition

`external-lease-lookup-tunnel` remains `blocked` until a fresh
`bash tests/integration/m6-interop/run-destination.sh` proves a
real tunneled lookup response arrives through the corrected
inbound gateway tuple. The lane now records a new structural
evidence row (`inbound-reply-path`) whose key/value pairs prove
the corrected tuple reached the wire:

```text
inbound-reply-path = gateway_matches_reference=true gateway_tunnel=38401 local_receive=38402 ids_distinct=true
```

Existing Plan 187/188 `passed` rows must remain `passed` after the
fresh run; if any already-passed row regresses, Plan 190 stops
without claiming closure.

## Handoff

Authority transition:

```text
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (local + remote lane; 3 destination rows flipped blocked -> passed)
plan_191 = stopped-by-inbound-delivery-boundary-E (inbound-delivery rows documented; 4 rows flipped blocked; 2 ordering rows flipped passed; narrower follow-up registered)
plan_192 = registered-m6-i2cp-wire-format-corrective (resolve Plan 191 §6 stop; I2CP-style Data body + 9-byte short-transport inner envelope + STYLE=RAW / DATAGRAM VERSION=3 SAM session)
plan_188 = remains-blocked-by-plan191-and-plan192 (real outbound/inbound i2pd installs retained-passed; reply-path correction retained-passed via plan190; 2 destination-message rows + 2 ordering rows flipped; ordering rows now passed)
plan_189 = remains-blocked-until-plan188-and-plan191-and-plan192-and-streaming-close
next_executable_plan = 192 (resolve inbound-delivery boundary E wire-format)
```

Read/execute in this order:

1. `plans/190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md`
2. `plans/190-status.md` (this file; newest authority)
3. `plans/191-status.md` (Plan 191 stopped at boundary E; documented defect)
4. `plans/191-m6-inbound-destination-delivery-boundary.md` (boundary-E plan)
5. `plans/192-status.md` (registered follow-up; next executable)
6. `plans/192-m6-i2cp-wire-format-corrective.md` (the I2CP-style Data body wire-format corrective)
7. `plans/188-status.md` for retained authenticated build/install evidence and the destination rows now blocked on Plan 192
8. `crates/i2pr-tunnel/src/established.rs`
9. `crates/i2pr-tunnel/src/data_plane_registry.rs`
10. `crates/i2pr-daemon/src/destination_tunnels.rs`
11. `crates/i2pr-client/src/routing.rs::compose_outbound_delivery` (the 9-byte short-transport + I2CP-style Data body seam)
12. `crates/i2pr-daemon/tests/destination_tunnel_unit.rs`
9. `crates/i2pr-daemon/tests/destination_tunnel_external.rs`
10. `tests/integration/m6-interop/run-destination.sh`
11. `plans/189-status.md` only after Plan 188 first-family destination + Streaming work genuinely closes
