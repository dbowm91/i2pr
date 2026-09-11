# Plan 190 — M6 inbound NetDB reply-path tunnel-ID corrective

Status: **registered executable corrective**. This plan is the next executable work before resuming Plan 188. Plan 189 remains blocked.

## 1. Goal

Fix the remaining first-family i2pd LeaseSet2 lookup blocker by correcting the reply path advertised in the tunneled `DatabaseLookup`.

Plan 188 has already proved real one-hop outbound and inbound tunnel installation from consumed exact-pinned i2pd replies. The current external destination lane then emits one outbound TunnelData cell for the LeaseSet2 lookup but receives no usable response before the bounded deadline. Static protocol/reference/source inspection now identifies a concrete metadata defect: the driver advertises the **local creator endpoint receive tunnel ID** as the `DatabaseLookup.reply_tunnelId`, while the I2NP contract requires the **receive tunnel ID on the remote inbound gateway**.

This corrective must preserve the distinction between those two IDs in production metadata, derive `i2pr_netdb::ReplyPath` from the remote inbound-gateway tuple, prove the encoded lookup carries the correct tuple, and rerun the exact-pinned i2pd lane. It must not broaden into Streaming, Java second-family qualification, public-network participation, a new NetDB stack, or reply-encryption work unless fresh evidence after this correction proves a separate blocker.

## 2. Starting authority and observed failure

Starting repository authority:

```text
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187 = blocked-by-m6-build-reply-interop-gap (2/7 flipped via plan188 installs)
plan_188 = in-progress-m6-short-build-reply-installs-proven
plan_189 = registered-blocked-by-plan188-lookup-gap
m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-pending-plan188
milestone6_interoperable = not-yet-claimed
```

Plan 188's external lane currently proves:

```text
installed_ob = 1
installed_ib = 1
outbound-lookup-via-tunnel cells = 1
reference LeaseSet2 never resolved within the bounded wait
```

The controlled one-hop constants in `crates/i2pr-daemon/tests/destination_tunnel_external.rs` deliberately distinguish the two inbound IDs:

```text
IBGW_RECEIVE = 0x9601 = 38401   # tunnel ID accepted by the remote i2pd inbound gateway
IBGW_NEXT    = 0x9602 = 38402   # downstream/local i2pr receive tunnel ID
```

The currently recorded installed-material evidence exposes `receive=38402`, and the driver presently builds the lookup reply path from `coord.registry().inbound_receive_ids()[0]`, therefore advertising 38402 to i2pd.

## 3. Research findings that define the correction

### 3.1 Current I2NP specification

Current I2NP specification (updated 2026-03, accurate for API 0.9.69):

- `DatabaseLookup.from`, when `deliveryFlag == 1`, is the RouterHash of the **reply tunnel gateway**.
- `DatabaseLookup.reply_tunnelId` is the nonzero **tunnel ID of the tunnel to send the reply to** on that gateway.
- Tunnel delivery instructions likewise identify the gateway router plus the destination tunnel ID on that gateway.
- No-reply-encryption mode is explicitly valid (`flags 4,1 = 0,0`); this corrective must not add reply encryption merely to make the lane pass.

Source:

- <https://beta.i2p.net/en/docs/specs/i2np/>

### 3.2 Exact-pinned i2pd behavior

Reference remains exact-pinned i2pd 2.61.0:

```text
635b013a612ff47278ef02acf8580a28e10e26c5
```

At that revision:

- `libi2pd/NetDb.cpp::HandleDatabaseLookupMsg` reads `replyIdent` from bytes 32..64 and, when the delivery flag is set, reads `replyTunnelID` from the lookup payload.
- For a tunneled response it sends `CreateTunnelGatewayMsg(replyTunnelID, replyMsg)` to `replyIdent` directly when reachable, or through an outbound tunnel otherwise.
- It only applies lookup-reply encryption when the corresponding encryption flags request it; an unencrypted reply remains a supported path.
- `libi2pd/Tunnel.cpp` parses the TunnelGateway payload tunnel ID and resolves the receiving tunnel through `GetTunnel(tunnelID)` before calling the gateway handler. Therefore the ID must be the ID installed on the **remote inbound gateway**, not the downstream creator endpoint's ID.
- `libi2pd/Transports.cpp::PostMessages` explicitly loopbacks messages whose destination hash is i2pd's own RouterHash through `m_LoopbackHandler`. Therefore the controlled topology where the same exact-pinned i2pd instance is both one-hop tunnel participant and floodfill is valid; do not redesign the harness merely because the target router equals the OBEP/IBGW router.

Pinned sources:

- <https://github.com/PurpleI2P/i2pd/blob/635b013a612ff47278ef02acf8580a28e10e26c5/libi2pd/NetDb.cpp>
- <https://github.com/PurpleI2P/i2pd/blob/635b013a612ff47278ef02acf8580a28e10e26c5/libi2pd/Tunnel.cpp>
- <https://github.com/PurpleI2P/i2pd/blob/635b013a612ff47278ef02acf8580a28e10e26c5/libi2pd/Transports.cpp>

### 3.3 i2pr ownership defect

`i2pr-tunnel::EstablishedTunnel` already models the protocol correctly and keeps these concepts separate:

```text
inbound_gateway                = (remote IBGW router, remote IBGW receive tunnel ID)
local_inbound_receive          = local creator endpoint receive tunnel ID
```

For the controlled one-hop trajectory these are conceptually:

```text
inbound_gateway.tunnel = 0x9601
local_inbound_receive  = 0x9602
```

The loss occurs in `i2pr-tunnel::data_plane_registry::DataPlaneRegistry::activate_inbound`:

- it retains the remote IBGW router hash in `inbound_first_hop`;
- it retains the local receive ID as the inbound-role map key;
- it does **not** retain the remote IBGW receive tunnel ID even though `EstablishedTunnel::inbound_gateway()` still has it at activation time.

The Plan 187 external driver then reconstructs an incomplete tuple manually:

```text
reply gateway = exact-pinned i2pd RouterHash
reply tunnel  = registry.inbound_receive_ids()[0]  # local endpoint ID, currently 0x9602
```

`i2pr-netdb::ReplyPath` and `build_databaselookup()` are already semantically correct: `ReplyPath.gateway` is documented as the inbound exploratory gateway RouterHash and `ReplyPath.tunnel_id` as the inbound exploratory tunnel identifier; the builder copies those values into `DatabaseLookup.from` and `reply_tunnel_id`. Do **not** change the wire codec to compensate for bad caller metadata.

## 4. Root-cause classification

Treat the following as a **statically confirmed metadata defect whose end-to-end effect still requires external proof**:

```text
EstablishedTunnel retains remote IBGW tuple
    ↓
DataPlaneRegistry activation discards remote IBGW tunnel ID
    ↓
external destination driver selects local receive ID
    ↓
ReplyPath(from=i2pd, tunnel=0x9602)
    ↓
i2pd attempts TunnelGateway(0x9602, reply)
    ↓
0x9602 is not i2pd's inbound-gateway receive ID
    ↓
lookup response cannot enter the installed inbound tunnel
```

The corrective is successful only when the external lane proves the response returns through the corrected path. Do not close from source inspection alone.

## 5. Required implementation

### 5.1 Preserve typed inbound-gateway routing metadata in `i2pr-tunnel`

Primary file:

```text
crates/i2pr-tunnel/src/data_plane_registry.rs
```

Add a small runtime-neutral public metadata type. Exact naming is flexible, but it must make the two tunnel-ID roles impossible to confuse. Preferred shape:

```rust
pub struct InboundGatewayRoute {
    pub gateway_router: i2pr_proto::Hash,
    pub gateway_receive_tunnel: TunnelId,
    pub local_receive_tunnel: TunnelId,
}
```

Requirements:

1. During `activate_inbound`, read `EstablishedTunnel::inbound_gateway()` **before** moving the tunnel into `LocalInboundEndpointRole`.
2. Store the remote gateway RouterHash and its remote receive tunnel ID together.
3. Keep the local receive tunnel ID explicit and separate; it remains the key used by inbound i2pr TunnelData dispatch.
4. The registry metadata must remain public/non-secret. Do not copy or expose layer keys.
5. Lifecycle cleanup must remove the new metadata atomically with the inbound role in `remove_inbound` / `remove_slot`.
6. Preserve existing callers of `inbound_first_hop()` if practical by implementing it from the new typed metadata. Do not force unrelated churn.
7. Add a narrow accessor such as:

```rust
pub fn inbound_gateway_route(
    &self,
    local_receive: TunnelId,
) -> Option<InboundGatewayRoute>
```

or an equivalent borrowed/copy surface.

Do not add an `i2pr-netdb` dependency to `i2pr-tunnel`; dependency direction remains runtime-neutral. `ReplyPath` construction belongs in the daemon composition layer.

### 5.2 Add one daemon-owned adapter from installed tunnel metadata to `ReplyPath`

Primary file:

```text
crates/i2pr-daemon/src/destination_tunnels.rs
```

Add one narrow helper owned by the daemon composition layer that converts the selected installed inbound exploratory route into `i2pr_netdb::ReplyPath`:

```text
ReplyPath.gateway   = route.gateway_router
ReplyPath.tunnel_id = route.gateway_receive_tunnel
```

The helper must **not** use `route.local_receive_tunnel` for `ReplyPath.tunnel_id`.

Preferred behavior:

- input is the real `DataPlaneRegistry` plus a selected local inbound role ID (or equivalent selector);
- output is a typed `ReplyPath` or a typed `DestinationTunnelError`;
- zero/missing/expired/unknown metadata fails closed;
- no direct-transport fallback;
- no synthetic `LocalZeroHop` reply path.

Do not duplicate lookup state or add a second NetDB coordinator. Continue using `DestinationTunnelCoordinator::begin_lease_lookup`, `NetDbSeam`, and `i2pr-netdb::build_databaselookup` unchanged except for the correctly derived `ReplyPath` input.

### 5.3 Remove manual reply-path reconstruction from the external driver

Primary file:

```text
crates/i2pr-daemon/tests/destination_tunnel_external.rs
```

Replace the current composition:

```text
RouterHash(i2pd), registry.inbound_receive_ids()[0]
```

with the production daemon adapter from §5.2.

The test must continue selecting a real installed inbound role. It may use the local receive ID only as a registry selector; that local ID must never be copied into the `DatabaseLookup.reply_tunnelId` field.

Add privacy-safe evidence that records only public routing metadata/counts, for example:

```text
inbound-reply-path = gateway_matches_reference=true gateway_tunnel=38401 local_receive=38402 ids_distinct=true
```

A full public RouterHash is acceptable but not required; do not record secrets, reply keys, session keys, layer keys, SAM PRIV material, or application plaintext.

### 5.4 Add regression tests that intentionally make the IDs unequal

At minimum add/extend tests in:

```text
crates/i2pr-tunnel/src/data_plane_registry.rs
crates/i2pr-daemon/tests/destination_tunnel_unit.rs
```

Required rows:

1. `DataPlaneRegistry` activation preserves all three public facts for an inbound tunnel:
   - gateway RouterHash;
   - remote gateway receive tunnel ID;
   - local endpoint receive tunnel ID.
2. Use deliberately unequal values such as `0x9601` and `0x9602`; a test where they accidentally match is insufficient.
3. Removal by local receive ID and removal by pool slot both clear the gateway-route metadata.
4. The daemon adapter produces `ReplyPath.tunnel_id == gateway_receive_tunnel` and explicitly `!= local_receive_tunnel`.
5. Build a LeaseSet2 `DatabaseLookup` through the ordinary `begin_lease_lookup` path and assert the resulting message contains:

```text
from            == gateway RouterHash
reply_tunnel_id == remote gateway receive tunnel ID
```

6. Round-trip that lookup through the existing I2NP codec; do not assert only on an intermediate struct.
7. Missing registry metadata fails closed and does not synthesize a direct reply path.

### 5.5 Keep external evidence semantics strict

Use the existing lane:

```bash
bash tests/integration/m6-interop/run-destination.sh
```

Do not add a skip flag, direct-delivery shortcut, `LocalZeroHop`, fake LeaseSet, synthetic response, reference-log-derived success, or timeout relaxation.

The lane may add a new structural evidence label for the corrected reply path, but existing rows must retain their meaning.

## 6. Diagnostic order after the correction

After local tests are green, run the exact-pinned external lane once before changing any other protocol area.

Expected sequence:

```text
installed_ob = 1
installed_ib = 1
reply-path gateway == i2pd
reply-path gateway_tunnel == 0x9601
reply-path local_receive == 0x9602
outbound-lookup-via-tunnel cells = 1
inbound TunnelData arrives on local receive 0x9602
DatabaseStore(LeaseSet2) is recovered
existing LeaseSet2 validators accept it
existing LeaseSet2 store caches it
external-lease-lookup-tunnel = passed
```

If that sequence occurs, continue the existing Plan 188 lane unchanged through publication and bidirectional destination messaging. Do not preemptively modify those layers.

If a reply begins arriving but fails at a later layer, stop and record the exact first new boundary:

```text
A. TunnelGateway reaches i2pd but no TunnelData reaches i2pr
B. TunnelData reaches i2pr but tunnel decrypt/reassembly fails
C. I2NP DatabaseStore is recovered but LeaseSet2 parse/validation fails
D. LeaseSet2 validates/caches but destination ECIES/Garlic send fails
E. outbound destination message passes but inbound response fails
```

Create a new narrow follow-up only for the first demonstrated boundary. Do not fold a newly discovered independent defect into this plan.

## 7. Explicitly ruled-out work for this corrective

Do **not**:

- redesign the one-router controlled i2pd topology; exact-pinned i2pd explicitly supports self-addressed transport loopback;
- change short-build record counts, reply correlation, or Garlic build-reply handling already proven by Plan 188;
- change SSU2 handshake/data semantics;
- change `i2pr-netdb::ReplyPath` wire semantics or `build_databaselookup()` to reinterpret a local endpoint ID as a gateway ID;
- add ECIES lookup-reply encryption merely because modern I2P supports it; unencrypted lookup replies are spec-valid and exact-pinned i2pd supports the mode currently requested;
- extend timeouts as a correctness fix;
- parse raw i2pd logs as acceptance evidence;
- start Java I2P second-family qualification;
- start the deferred mixed-router Streaming pass;
- claim Milestone 6 interoperability from a corrected LeaseSet2 lookup alone.

## 8. Acceptance criteria

Plan 190 passes only when all of the following are true:

1. `DataPlaneRegistry` retains the remote inbound-gateway receive tunnel ID separately from the local endpoint receive tunnel ID.
2. The production daemon adapter constructs `ReplyPath` from `(gateway RouterHash, gateway receive tunnel ID)` only.
3. A local regression with unequal IDs proves the encoded/decoded `DatabaseLookup` advertises the remote gateway ID and not the local receive ID.
4. Registry removal/expiry paths remove the new public metadata without leaks or stale selection.
5. The exact-pinned i2pd external destination lane reaches a real tunneled lookup response and the existing LeaseSet2 validation/store path resolves the reference LeaseSet2.
6. `external-lease-lookup-tunnel` flips from blocked to passed from command-derived evidence; no synthetic or direct path counts.
7. Existing Plan 185/186/187/188 local tests remain green.
8. No existing Plan 188 passed row regresses (`external-outbound-tunnel`, `external-inbound-tunnel`, session/reference/floodfill/liveness/direct-rejected rows remain passed).
9. Full workspace/static quality floor and exact-head routine CI pass.
10. `plans/190-status.md` is updated with the exact implementation SHA, exact hosted CI run, exact external lane result, and the next demonstrated boundary.

If the same corrected run also makes all five remaining Plan 188 destination rows pass, update Plan 188 closure authority and proceed to its deferred Streaming handoff. Otherwise Plan 188 resumes at the first newly demonstrated downstream blocker; do not mark the whole destination plane passed from lookup success alone.

## 9. Validation commands

Focused local floor:

```bash
cargo fmt --all --check
cargo test --locked -p i2pr-tunnel -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- --test-threads=1
```

External first-family gate:

```bash
bash tests/integration/m6-interop/run-destination.sh
```

Then the repository floor used by current authority:

```bash
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo deny check advisories bans sources
```

Run the existing static evidence/boundary scripts required by CI; do not weaken them to accommodate this corrective.

## 10. Stop conditions

Stop this corrective and register a new narrow plan if any of the following is proven after the correct remote gateway tuple is on the wire:

- i2pd accepts the correct `(gateway, gateway_receive_tunnel)` but no response is generated;
- a response is generated and TunnelData reaches i2pr but fails tunnel transform/reassembly;
- the recovered reply is encrypted despite the unencrypted request mode and requires a protocol capability not currently implemented;
- the recovered DatabaseStore is structurally valid but the existing LeaseSet2 validator rejects a genuine exact-pinned reference record;
- the lookup succeeds and the next failure is publication, ECIES/Garlic destination delivery, or inbound destination dispatch.

In each case preserve the passing reply-path correction and escalate only the newly demonstrated layer.

## 11. Handoff / authority transition

Until execution:

```text
plan_190 = registered-executable-m6-inbound-netdb-reply-path-tunnel-id-corrective
plan_188 = blocked-by-plan190-reply-path-corrective (installs retained-passed)
plan_189 = blocked-by-plan188-and-plan190
m6_destination_remote_interop = installs-proven-lookup-pending-plan190
milestone6_interoperable = not-yet-claimed
next_executable_plan = 190
resume_after_plan190 = 188
```

After Plan 190 passes:

```text
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective
plan_188 = resume-at-first-remaining-destination-row
plan_189 = remains-blocked-until-plan188-and-streaming-close
next_executable_plan = 188
```

If the same external run closes all remaining Plan 188 destination rows, Plan 188 may close in that same evidence pass and hand off to the already-deferred mixed-router Streaming work. Plan 189 second-family qualification must still wait until the first-family destination + Streaming gates are genuinely green.
