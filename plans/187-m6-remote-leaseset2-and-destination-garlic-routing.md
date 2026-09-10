# Plan 187 — M6 remote LeaseSet2 and destination ECIES/Garlic routing

Status: **blocked until Plan 186 passes**.

## 1. Goal

Connect `i2pr-client::DestinationRouting` to the live Plan 185/186 tunnel and NetDB substrate. Against exact-pinned i2pd, resolve a reference-owned Standard LeaseSet2, send authenticated destination traffic through a real i2pr outbound tunnel to the selected remote lease, and receive/decrypt authenticated destination traffic through a real i2pr inbound tunnel.

This plan proves the destination message plane **before** Streaming.

## 2. Reuse mandatory existing ownership

Use only:

- `i2pr-client::DestinationRuntime` / `DestinationRouting`;
- existing destination-owned tunnel factories/pools;
- existing `LeaseSet2Store` and NetDB lookup actions;
- existing ECIES-X25519-AEAD-Ratchet implementation;
- existing Garlic/I2NP construction in the client layer;
- Plan 185 real tunnel pool/registry;
- Plan 186 real NetDB lookup/publication path.

No service-tunnel, SAM, or I2CP adapter is an internal shortcut. No second Garlic/decrypt/routing stack.

## 3. Reference service destination

Use exact-pinned unmodified i2pd and create a destination using its public SAM/client surface. Prefer a deterministic harness session with:

```text
inbound.length=0
outbound.length=0
```

where supported, so the reference side does not require extra controlled routers. Record only public Destination/LeaseSet2 facts and digests; never persist/reference private keys in repository evidence.

The reference destination must publish a real signed Standard LeaseSet2 containing at least one usable lease and X25519 encryption key.

## 4. Destination-owned tunnel integration

Replace any local-only factory assumptions on the counted route. A non-local i2pr destination must acquire real one-hop inbound/outbound tunnels through the existing factory/pool abstraction from Plan 185.

Requirements:

- no `LocalZeroHop` for counted i2pr remote evidence;
- remote `EstablishedMaterial` remains nonempty and cryptographically derived;
- destination pool ownership/rotation/expiry remains per existing limits;
- tunnel liveness from Plan 185 applies to destination tunnels or the shared underlying pool as designed;
- no fake lease gateway/tunnel IDs.

## 5. Remote LeaseSet2 lookup

Drive `DestinationRouting::begin_remote_lookup()` into the live Plan 186 `NetDbSeam` rather than leaving it as an unwired action.

Required trajectory:

```text
application/destination asks for remote peer
 -> bounded LeaseSet2 lookup
 -> real exploratory outbound tunnel
 -> reference floodfill
 -> reply via real exploratory inbound tunnel
 -> signed Standard LeaseSet2 validation
 -> LeaseSet2Store/cache
 -> destination routing retry
```

Validate destination binding, signature, expiry, lease count, lease gateway/tunnel IDs, supported encryption type, and cache freshness using existing validators.

## 6. Outbound destination message

After lookup success:

1. select a live lease from the reference Standard LeaseSet2;
2. wrap bounded application test bytes in the existing Data/ECIES/Garlic path;
3. select i2pr's real outbound destination tunnel;
4. set tunnel-delivery instructions for the remote lease gateway/tunnel ID;
5. send outer TunnelData to the i2pr first hop through Plan 184 RouterDelivery;
6. verify reference destination receives and authenticates the message through its normal client API.

Evidence must distinguish destination payload digest from routing metadata and never log plaintext content.

## 7. Inbound destination message

Reference destination sends a reply through its normal router/client path to i2pr's published LeaseSet2. i2pr must:

1. receive outer TunnelData on its real inbound destination tunnel;
2. recover the tunneled Garlic/I2NP message through existing endpoint roles;
3. route to the correct destination only;
4. decrypt/authenticate through existing ECIES session manager;
5. deliver the bounded application payload to the owning destination queue.

Sibling destination isolation is mandatory.

## 8. i2pr LeaseSet2 publication

Ensure the i2pr destination's own Standard LeaseSet2 describing the real inbound tunnel is available to the reference router through the controlled NetDB path. Reuse existing router-owned destination signing/publication behavior; do not synthesize a client-owned I2CP LeaseSet for this path.

Publication success must have protocol-derived evidence and expiry/rotation behavior.

## 9. Security and negative matrix

Test:

- destination hash mismatch;
- invalid/expired LeaseSet2;
- unsupported encryption type;
- zero/malformed leases;
- stale cached LeaseSet2 forces bounded refresh;
- selected lease expires between lookup and send;
- wrong inbound tunnel ID;
- Garlic authentication/decryption failure;
- replay/duplicate delivery behavior per existing ECIES policy;
- sibling destination cannot consume message;
- tunnel disappears during send;
- lookup/publication cancellation and shutdown.

No failure may fall back to a direct transport payload route.

## 10. Acceptance criteria

Plan 187 passes only when:

1. a reference-owned signed Standard LeaseSet2 is resolved through Plan 186's real mixed-router NetDB tunnel path;
2. the record validates and is cached through the existing LeaseSet2 store;
3. an i2pr destination owns real non-zero-hop inbound/outbound tunnel material;
4. i2pr's own Standard LeaseSet2 publishes a real inbound lease through the controlled NetDB path;
5. a bounded message from i2pr reaches the reference destination through existing ECIES/Garlic + real outbound tunnel + remote lease;
6. a reply from the reference destination reaches i2pr through its real inbound tunnel and existing ECIES decrypt path;
7. digests match in both directions and sibling isolation is proven;
8. no direct destination-over-SSU2 shortcut, fake lease, or local-zero-hop i2pr path is used;
9. malformed/stale/replay/tunnel-loss cases are bounded;
10. Plan 185 liveness and Plan 186 NetDB regressions remain green;
11. full workspace/static floor and exact-head routine CI pass;
12. `plans/187-status.md` advances `next_executable_plan = 188`.

## 11. Stop conditions

Stop for a narrow corrective if exact-pinned i2pd exposes an ECIES/Garlic or LeaseSet2 interoperability defect. Capture only sanitized message type/length/hash/session-state facts. Do not weaken authentication, accept unsupported crypto, or bypass the destination layer.

## 12. Handoff

Plan 188 may begin only after this raw destination message plane is proven. It then layers the existing Streaming protocol on the same path without changing routing semantics.
