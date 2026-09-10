# Plan 184 — M6 authenticated I2NP runtime and independent-router preflight

Status: **first executable Plan 183 pass**.

Prerequisite: Plan 183 scoped roadmap registered.

## 1. Goal

Connect the already-proven SSU2 v2 runtime to normal daemon supervision and establish one authenticated router-I2NP dispatch spine. Prove the exact-pinned i2pd peer can exchange bounded authenticated I2NP with the normal daemon runtime. Do not build exploratory tunnels yet.

## 2. Read first

- `plans/183-m6-mixed-router-streaming-interop-program.md`
- `plans/161-status.md`
- `crates/i2pr-runtime/src/ssu2_runtime.rs`
- `crates/i2pr-daemon/src/config.rs`
- `crates/i2pr-daemon/src/lib.rs`
- `crates/i2pr-daemon/src/inbound_dispatch.rs`
- `crates/i2pr-tunnel/src/bridge.rs`
- `specs/protocols/08-ssu2.md`
- `GUARDRAILS.md`

## 3. Configuration activation

The daemon currently parses a bounded `[ssu2]` surface but rejects `enabled = true`. Replace that blanket rejection with a strict controlled profile:

- `enabled = false` remains the default;
- when enabled for this program, listen address must be loopback;
- `advertise = false` is mandatory;
- introducer/relay publication remains disabled;
- no wildcard/non-loopback bind in this plan;
- existing token/session/datagram ceilings remain authoritative;
- reject configuration that attempts broader publication/exposure.

Do not alter M8 protocol semantics merely to activate the existing runtime.

## 4. Daemon-owned SSU2 service

Register `Ssu2RuntimeService` under the existing daemon child/supervision model. The daemon must own:

- UDP socket/runtime lifetime;
- local RouterInfo/identity inputs;
- session establishment notifications;
- the bounded inbound I2NP receiver;
- orderly cancellation/shutdown.

No hidden standalone runtime may coexist with the daemon-owned instance in counted tests.

## 5. Authenticated router-I2NP dispatcher

Add a narrow daemon module/capability, e.g. `router_i2np.rs`, with these properties:

```text
Ssu2InboundI2np { authenticated peer/link metadata, encoded message }
 -> bounded standard/short I2NP decode
 -> expiration/size/type validation
 -> typed dispatch outcome
```

Required initial destinations:

- ShortTunnelBuild / OutboundTunnelBuildReply: typed hook reserved for Plan 185;
- TunnelData: typed hook into the existing data-plane registry/`inbound_dispatch` seam;
- DatabaseStore / DatabaseSearchReply / DeliveryStatus: typed router-control/NetDB hooks;
- unsupported/unknown I2NP: bounded explicit disposition, never panic/unbounded retain.

The dispatcher must preserve authenticated peer identity where routing/ownership checks need it. Do not accept arbitrary caller-supplied peer identity for the live path.

Keep `inbound_dispatch.rs` focused on recovered tunnel payloads; do not duplicate its NetDB normalization logic.

## 6. Outbound router delivery capability

Add one narrow transport-neutral capability used by future plans:

```text
RouterDelivery { peer_router_hash, encoded_i2np, deadline }
 -> established authenticated SSU2 link
 -> Ssu2RuntimeService::send_i2np(...)
```

Requirements:

- bounded pending delivery count/bytes;
- explicit unknown/not-established peer failure;
- deadline/cancellation;
- no task per delivery;
- no generic global router context passed into tunnel/NetDB/client crates.

Plan 184 may keep routing to one configured reference peer; general peer selection belongs to later normal router development.

## 7. Exact-pinned i2pd preflight

Use unmodified:

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
```

Create a new small runner under `tests/integration/m6-interop/` or extend the existing Plan 161 provisioning helpers without copying the whole historical M3 harness.

Ephemeral i2pd profile:

- fresh temp datadir;
- no reseed/public-network dependence;
- SSU2 loopback and published only inside the isolated reference RouterInfo used by the test;
- no unrelated services except optional loopback SAM for later plans;
- clean checkout verified before run.

The runner must extract/verify i2pd RouterInfo through a documented command/file path and pass only public RouterInfo material into i2pr.

## 8. Preflight evidence

Counted test must prove:

1. i2pr daemon starts with the strict SSU2 controlled profile.
2. exact-pinned i2pd starts unmodified.
3. i2pr parses/verifies the reference RouterInfo.
4. authenticated SSU2 session establishes.
5. one benign router-I2NP control message traverses i2pr -> i2pd through `send_i2np`.
6. one authenticated inbound I2NP traverses i2pd -> i2pr and reaches the central dispatcher.
7. malformed/expired/oversized inbound I2NP is rejected boundedly.
8. stopping either peer releases session, queue, socket, and dispatch resources.

Use a protocol-appropriate control message such as DeliveryStatus/DatabaseStore only if the exact reference behavior makes it observable without pretending NetDB success. Do not fabricate a tunnel-build pass in this plan.

## 9. Tests

Add focused tests for:

- config enable/advertise constraints;
- no non-loopback activation;
- dispatcher standard vs short I2NP classification;
- expiration and size limits;
- peer metadata preservation;
- unknown peer outbound failure;
- queue saturation and cancellation;
- daemon shutdown with active SSU2 session;
- retained Plan 161 direct SSU2 suite.

External preflight must be ignored/gated in routine workspace CI if it requires the reference checkout, but explicit invocation must fail closed if prerequisites are missing.

## 10. Acceptance criteria

Plan 184 passes only when:

1. normal daemon supervision can activate the existing SSU2 runtime under the strict loopback/non-advertised profile;
2. no broad/public SSU2 advertisement is enabled;
3. one central authenticated router-I2NP dispatcher exists and receives `Ssu2InboundI2np` from the live runtime;
4. one bounded outbound router-delivery seam uses existing SSU2 `send_i2np()`;
5. exact-pinned unmodified i2pd establishes the authenticated session against the normal daemon;
6. command-derived evidence proves authenticated I2NP in both directions reaches the intended live seams;
7. malformed/oversized/expired input and queue saturation are bounded;
8. shutdown returns all counters/resources to baseline;
9. Plan 161, M9 and M10 local regressions remain green;
10. full workspace/static/dependency floor and exact-head routine CI pass;
11. `plans/184-status.md` records exact head/run/evidence and advances `next_executable_plan = 185`.

## 11. Stop conditions

Stop for a narrow corrective if:

- daemon activation requires weakening Plan 161 authentication/token semantics;
- i2pd cannot establish the previously proven SSU2 session when driven by normal daemon ownership;
- the only proposed design uses per-I2NP tasks or an unrestricted global context;
- authenticated peer identity is lost before dispatch;
- a test needs public I2P or patched i2pd.

## 12. Handoff

On pass, Plan 185 owns the first real one-hop Short Tunnel Build and TunnelData exchange. Plan 184 must not claim NetDB, destination, Streaming, or M10 remote-service interoperability.
