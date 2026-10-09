# Plan 368 — SAM 3.3 PRIMARY/Subsession Shared-Destination Profile

Status: **ready — parallel SAM extension; does not reopen Milestone 7 SAM 3.1 acceptance**

Date: 2026-10-06

Historical authority:

- Plan 135–153 SAM 3.1 line
- `plans/subsystems/sam-roadmap.md`
- `plans/closure/sam/151-status.md`
- `plans/closure/sam/153-status.md`

Primary consumers:

- `dbowm91/i2pr-tc` C003 SAM 3.3 primary/subsession transport corrective;
- `dbowm91/i2pr-mail` M006/M012 port-aware Postman transport: the reference Postman client tunnels target `pop.postman.i2p:110` and `smtp.postman.i2p:25`, so mail requires the SAM 3.2+ I2P port fields that are part of a truthful 3.3 implementation.

Primary class: capability + invariant + managed-app interface dependency

## 1. Purpose

Extend the closed SAM 3.1 product with the smallest SAM 3.3
PRIMARY/subsession profile required for applications that need multiple I2P
protocols over one Destination.

The immediate consumer is the I2P-native torrent client. Its future DHT needs
the same Destination for:

- Streaming peer/tracker connections;
- protocol 17 signed/repliable datagrams;
- protocol 18 raw datagrams.

Raw I2CP can express this directly, but requiring every managed application to
own I2CP destination/LeaseSet lifecycle plus an application-side Streaming
implementation would duplicate capabilities already implemented in
`i2pr-client`. SAM 3.3 PRIMARY/subsessions is the intended adapter layer for
this use case.

This plan extends SAM. It does not replace I2CP and does not weaken the managed
app capability boundary.

## 2. Compatibility and support posture

The existing SAM 3.1 claim remains closed and valid.

Plan 368 adds a separately evidenced SAM 3.3 profile. Do not change the
advertised maximum version until the full Plan-368 acceptance matrix passes.

Required compatibility policy:

- preserve every currently supported SAM 3.1 STREAM path;
- add the current normative PRIMARY/subsession behavior;
- inspect current Java I2P and current i2pd before implementation;
- current i2pd source uses `MASTER` terminology for its shared-session owner
  where current SAM documentation uses `PRIMARY`; if still true, decide
  explicitly whether i2pr accepts `MASTER` as a narrow compatibility alias;
- never advertise a 3.3 command that maps to an inert/no-op implementation.

A compatibility alias may share the same internal state model but must not
create a second semantic owner.

## 3. Existing substrate to reuse

Plan 368 must remain an adapter over existing production owners.

Already present in i2pr:

- local Destination lifecycle and tunnel pools in `i2pr-client`;
- Streaming protocol/runtime and `StreamingDestinationAdapter`;
- protocol 17 repliable datagram substrate;
- protocol 18 raw datagram substrate;
- bounded destination routing;
- SAM parser/session/stream server;
- listener-independent private SAM connection seam from Plan 354;
- app-principal/capability gateway from Plan 355.

Do not create:

- a second Destination registry;
- a second Streaming implementation;
- a SAM-only datagram router;
- a torrent-specific daemon API;
- direct host UDP as a requirement for managed apps.

## 4. Target ownership model

```text
SAM primary session
  owner: one control connection
  owns: one Destination + one tunnel set
  |
  +-- STREAM subsession
  |     -> existing StreamingManager / StreamingDestinationAdapter
  |
  +-- DATAGRAM subsession
  |     -> existing DatagramManager / protocol 17
  |
  +-- RAW subsession
        -> existing raw datagram path / protocol 18
```

The primary control connection is authoritative lifetime ownership.

Loss/close of that owner:

1. stops new child admission;
2. cancels pending child operations;
3. removes child listeners/receivers;
4. closes active child state according to protocol semantics;
5. removes the Destination only after children are quiesced;
6. releases session IDs/resources exactly once.

Loss of one child must not destroy sibling children or the primary unless the
protocol requires it.

## 5. Protocol surface

At implementation start, pin the official SAM 3.3 specification and exact
current Java I2P/i2pd revisions.

Implement the smallest complete profile required by the shared-Destination
consumer:

- HELLO negotiation through 3.3;
- primary shared-session creation;
- child/subsession add;
- child/subsession remove;
- STREAM child attachment sufficient for CONNECT/ACCEPT;
- DATAGRAM child send/receive without host UDP forwarding;
- RAW child send/receive without host UDP forwarding;
- naming/NAME=ME behavior against the primary Destination;
- typed duplicate/invalid ID/style/port/result behavior, including exact 0–65535 bounds and precedence of per-stream `FROM_PORT`/`TO_PORT` over session defaults;
- deterministic teardown on primary control loss.

Do not include FORWARD as a managed-app requirement. Existing ordinary SAM
FORWARD behavior remains separate and private managed-app policy continues to
deny host-target forwarding.

## 6. Private managed-app behavior

Plan 354 established one private app logical `sam` stream -> one raw router
SAM protocol connection with exact ordered octets.

Plan 368 must preserve that contract.

A managed app may open the multiple raw protocol connections required by one
SAM 3.3 primary/subsession session, but all those connections remain scoped to
the same app principal/session context through Plan 355.

Required negative guarantees:

- one app cannot attach a child to another app's primary ID;
- two app principals may reuse the same textual SAM IDs without collision if
  the gateway's isolation contract permits it;
- a private app cannot acquire host UDP forwarding by requesting DATAGRAM/RAW;
- private `STREAM FORWARD` remains denied;
- closing/revoking the app gateway tears down its primary and children;
- no administrator/Proposal-170 token is required.

## 7. Datagram semantics

Use the canonical destination data plane.

For protocol 17:

- preserve sender authentication/signature semantics owned by
  `i2pr-client::datagram`;
- preserve source/destination I2P ports;
- bound message size and receive queues;
- surface typed queue-full/invalid-signature/malformed errors.

For protocol 18:

- preserve raw datagram semantics;
- preserve source/destination I2P ports;
- bind the receive path to the authenticated destination transport context
  available at the router layer;
- bound payloads and queues.

SAM is only the application framing/control adapter. It must not reinterpret
I2P BitTorrent DHT payloads.

## 8. Datagram2/Datagram3 disposition

Current i2pd supports Datagram2/3 and newer I2P torrent UDP-tracker work may
benefit from them, but the immediate DHT requirement is protocol 17 + 18.

During Plan 368:

- research and document the exact current SAM 3.3 style names/wire behavior for
  Datagram2/3;
- design the internal child-style enum so adding them does not require another
  ownership rewrite;
- do not advertise/support them unless their complete router substrate and
  interoperability evidence are ready in the same plan.

Deferral is acceptable; accidental partial acceptance is not.

## 9. State model changes

Extend the runtime-neutral SAM state with explicit:

- primary session kind;
- primary owner connection token;
- primary Destination ID;
- bounded child ID map;
- child kind/style;
- child I2P source/destination/listen-port configuration, including inherited 3.2 `FROM_PORT`/`TO_PORT` semantics;
- child owner/attachment state;
- generation/teardown state.

A child must reference one live primary. No command may implicitly create a
Destination when it claims to attach to an existing primary.

The global and per-principal session registries must have named ceilings.

## 10. Parser and framing bounds

Retain all existing SAM line/token/value ceilings and add named bounds for:

- children per primary;
- concurrent primary sessions;
- DATAGRAM/RAW queued messages per child;
- DATAGRAM/RAW queued bytes per child/primary;
- child add/remove command rate or in-flight operations where needed;
- datagram payload size;
- receive dispatch work per turn;
- shutdown drain deadline.

Reject duplicate identity-bearing options deterministically.

No logs may contain private Destination material or application datagram
payloads.

## 11. Reference and interoperability requirements

Qualify against at least:

### Current Java I2P

- 3.3 HELLO;
- primary creation;
- STREAM child;
- DATAGRAM child;
- RAW child;
- shared `NAME=ME` Destination across all children;
- child removal;
- primary close tears down children.

### Current i2pd

Run the same matrix and record:

- PRIMARY versus MASTER spelling;
- SESSION ADD/REMOVE compatibility;
- STREAM behavior;
- DATAGRAM/RAW framing;
- shared Destination;
- any version/feature differences.

### i2pr self-product

Use real loopback SAM sockets and the private managed-app raw-stream origin.
The private-origin matrix is mandatory because that is the consumer path.

## 12. Torrent-specific acceptance fixture without torrent coupling

Do not depend on i2pr-tc from the router repository.

Instead add a generic shared-Destination fixture:

1. create one primary;
2. add STREAM, DATAGRAM, RAW children;
3. resolve `NAME=ME` and record one Destination hash;
4. exchange exact bytes over STREAM;
5. exchange one protocol-17 datagram;
6. exchange one protocol-18 datagram;
7. prove all three are routed under the same local Destination;
8. remove one child and prove siblings remain usable;
9. drop primary control and prove all remaining children are torn down.

That fixture is the upstream contract C003 consumes.

## 13. Ordered work packages

### WP1 — specification/reference freeze

Pin official SAM 3.3, Java I2P, i2pd, and current I2P datagram guidance.
Record PRIMARY/MASTER differences before changing code.

### WP2 — runtime-neutral primary/child protocol model

Extend parser/replies/state transitions and strict bounds without sockets.

### WP3 — primary lifetime composition

Bind one control owner to one existing `DestinationRuntime`; deterministic
teardown and generation rules.

### WP4 — STREAM child composition and I2P port semantics

Reuse existing Streaming managers/registries without reopening the 3.1 stream
implementation. Implement and test the 3.2+ `FROM_PORT`/`TO_PORT` semantics carried
by SAM 3.3 so a managed application can target a nonzero remote I2P service port without
a localhost I2PTunnel proxy.

### WP5 — DATAGRAM/RAW child composition

Bridge child framing to the existing protocol 17/18 destination data plane.

### WP6 — private managed-app isolation

Run the same primary/child machinery through Plan 354/355 private streams and
prove cross-principal denial/revocation.

### WP7 — Java I2P/i2pd interoperability

Execute the reference matrix and correct only evidenced incompatibilities.

### WP8 — support/docs/closure

Update the SAM roadmap, support inventory, protocol dossier, managed-app
reference documentation, and closure evidence. Preserve historical SAM 3.1
records.

## 14. Verification floor

Run the repository routine floor plus targeted:

- SAM parser/state tests;
- SAM acceptance checker extensions;
- datagram manager tests;
- primary/child lifecycle property tests;
- private app-principal isolation tests;
- loopback shared-Destination integration;
- port-aware STREAM integration with a controlled nonzero destination port, including
  a max-boundary case and a negative proof that omitting `TO_PORT` does not accidentally
  reach the nonzero-port service;
- external Java I2P matrix;
- external i2pd matrix.

Environment-gated external rows must fail explicitly when invoked without
required infrastructure; ordinary CI may keep them ignored according to the
existing evidence policy.

## 15. Acceptance criteria

Plan 368 closes only when:

1. i2pr negotiates and honestly advertises the implemented SAM 3.3 profile;
2. a long-lived primary owns one Destination and tunnel set;
3. STREAM, DATAGRAM, and RAW children share exactly that Destination;
4. STREAM reuses the existing Streaming owner;
5. DATAGRAM/RAW reuse canonical protocol-17/18 owners;
6. child removal leaves siblings healthy;
7. primary loss tears down all children deterministically;
8. private managed-app SAM streams support the same semantics;
9. cross-app child/session attachment is impossible;
10. no private DATAGRAM/RAW path grants host UDP authority;
11. Java I2P interoperability is recorded;
12. i2pd interoperability and PRIMARY/MASTER disposition are recorded;
13. SAM 3.1 regression tests remain green;
14. SAM 3.3 STREAM CONNECT honors a nonzero `TO_PORT` on both ordinary and private
    managed-app origins, with exact port-boundary evidence;
15. full routine/guard/documentation floor passes.

## 16. Stop conditions

Stop and register a successor/corrective if:

- the official 3.3 profile cannot be reconciled with current deployed Java I2P;
- i2pd requires incompatible ownership rather than a narrow syntax alias;
- protocol 17/18 delivery would bypass canonical destination routing;
- private managed-app streams cannot keep all child connections inside one
  principal-isolated session context;
- implementation requires a host UDP forwarding socket for the managed profile;
- existing Destination/Streaming/datagram APIs are insufficient and need a
  lower-layer corrective first.

## 17. Closure evidence

Create `plans/closure/sam/368-status.md` containing:

- implementation SHAs;
- pinned specs/reference revisions;
- primary/child state diagram;
- resource ceilings;
- same-Destination STREAM/DATAGRAM/RAW evidence;
- primary/child teardown matrix;
- private app-principal isolation matrix;
- Java I2P interoperability;
- i2pd interoperability and PRIMARY/MASTER decision;
- SAM 3.1 regression evidence;
- routine/guard results;
- residual findings and downstream unblock decisions for i2pr-tc C003 and
  i2pr-mail's port-aware managed-app transport.
