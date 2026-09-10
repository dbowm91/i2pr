# Plan 185 — M6 live one-hop exploratory tunnels, TunnelData, and liveness

Status: **blocked until Plan 184 passes**.

## 1. Goal

Use the existing Short Tunnel Build and data-plane implementations against exact-pinned i2pd to create genuine one-hop inbound and outbound exploratory tunnels, move TunnelData through them in both directions, and add current-spec creator-side tunnel testing/liveness.

No NetDB lookup success or destination Streaming claim belongs here.

## 2. Reuse, do not replace

Mandatory existing production components:

- `i2pr-tunnel::short::ShortBuildStateMachine`;
- `i2pr-tunnel::bridge::ShortBuildI2npBridge`;
- existing `ExploratoryPool` and `DataPlaneRegistry`;
- inbound/outbound gateway/endpoint TunnelData roles;
- Plan 184 authenticated router-delivery/I2NP dispatcher;
- Plan 184 daemon-owned SSU2 session.

Do not introduce a second ShortTunnelBuild encoder, alternate TunnelData framing, relaxed reply decoder, or test-only established material.

## 3. Controlled i2pd role

Exact pin:

```text
PurpleI2P/i2pd
2.61.0
635b013a612ff47278ef02acf8580a28e10e26c5
```

For this isolated lane only, configure the ephemeral router to accept the one-hop exploratory builds required by i2pr (`notransit=false` or the exact equivalent), while keeping public reseed/network dependence disabled. Record the effective config in sanitized evidence.

The reference router may also be floodfill-capable for Plan 186, but Plan 185 must not count NetDB behavior yet.

## 4. Build coordinator

Add the smallest daemon-owned coordinator that translates existing short-build actions into Plan 184 router deliveries and feeds authenticated build replies back to the state machine.

Required ownership:

```text
request inbound/outbound exploratory build
 -> choose exact reference RouterInfo from authoritative bounded store/config
 -> ShortBuildStateMachine
 -> ShortBuildI2npBridge
 -> authenticated SSU2 RouterDelivery
 -> authenticated build reply via central I2NP dispatcher
 -> state machine verification
 -> install EstablishedMaterial into existing pool/registry
```

All request/reply IDs, deadlines, pending builds, and material must be bounded. No per-build task explosion; use one scheduler/coordinator.

## 5. Real one-hop paths

Build at least:

- one outbound exploratory tunnel: i2pr creator/gateway -> i2pd participant/endpoint;
- one inbound exploratory tunnel: i2pd participant/gateway -> i2pr creator/endpoint.

Prove that installed material was derived from successful authenticated build replies and contains one real remote hop. `LocalZeroHop` from Plan 172 is not valid for these counted rows.

## 6. TunnelData proof

After establishment, send bounded test I2NP/control data through the outbound tunnel and receive corresponding traffic through the inbound tunnel using existing TunnelData roles and `DataPlaneRegistry`.

Required evidence distinguishes:

- direct transport I2NP;
- ShortTunnelBuild/Reply;
- TunnelData outer message;
- recovered inner I2NP at the tunnel endpoint.

Do not count direct SSU2 delivery as tunnel delivery.

## 7. Creator-side tunnel liveness/testing

Current I2P tunnel-routing guidance requires creators to test tunnels because participants may remove tunnels after approximately two minutes of no traffic.

Implement one central bounded liveness scheduler that pairs an active outbound exploratory tunnel with an active inbound exploratory tunnel and sends a DeliveryStatus test through the outbound path with the reply routed down the inbound path.

Product defaults/ceilings:

- first test target <=30 seconds after establishment;
- repeat target <=60 seconds while active;
- hard ceiling remains safely below the two-minute idle deletion boundary;
- bounded pending test IDs;
- bounded consecutive-failure threshold;
- successful response refreshes health;
- threshold failure marks/removes the affected tunnel and requests replacement through existing pool policy;
- no task/timer per tunnel.

Tests may use injected clock/scheduler advancement, but the external lane must execute at least one real DeliveryStatus round trip and one repeated scheduled test without waiting two minutes.

## 8. Failure cases

Exercise:

- corrupted build reply record;
- wrong reply ID;
- expired reply;
- reference refusal;
- build deadline;
- one direction established while sibling direction fails;
- malformed TunnelData;
- unknown tunnel ID;
- duplicate/replayed liveness reply;
- missed liveness replies triggering bounded removal/rebuild;
- cancellation/shutdown during build and during liveness wait.

Strict decoder behavior from prior plans must remain unchanged.

## 9. Acceptance criteria

Plan 185 passes only when:

1. exact-pinned unmodified i2pd accepts a real ShortTunnelBuild from the normal i2pr daemon path;
2. one-hop inbound and outbound exploratory tunnels both establish;
3. successful material is installed through existing pool/registry ownership, never synthetic insertion;
4. outbound and inbound TunnelData paths are demonstrated independently of direct SSU2 I2NP;
5. recovered inner I2NP is routed through the Plan 184 dispatcher/Plan117 data-plane seam;
6. current-spec tunnel liveness scheduler exists with bounded first/repeat/failure policy;
7. a real paired DeliveryStatus tunnel test succeeds against i2pd and repeated scheduling is proven;
8. failure threshold removes/replaces unhealthy tunnels without leaks;
9. no per-tunnel task/timer design is introduced;
10. all malformed/refusal/deadline/replay paths are bounded;
11. Plan 184/M8/M9/M10 local regressions and full exact-head CI pass;
12. `plans/185-status.md` advances `next_executable_plan = 186`.

## 10. Stop conditions

Stop for a narrow corrective if exact-pinned i2pd rejects a standards-conformant request. Capture the sanitized build request/reply and compare with current Short Tunnel Build specification/reference behavior before changing protocol code. Do not loosen authentication/AEAD/record-count validation merely to get a reply.

## 11. Handoff

Plan 186 reuses the established exploratory pair for real mixed-router NetDB lookup/publication. Do not begin destination LeaseSet2 or Streaming work early.
