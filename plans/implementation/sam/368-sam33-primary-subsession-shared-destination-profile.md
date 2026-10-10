# Plan 368 — SAM 3.3 PRIMARY/Subsession Shared-Destination Profile

Status: **passed-sam33-primary-subsession-shared-destination-profile**

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
The matrix and complete routine floor pass, so production negotiation is
`[3.1, 3.3]`. Formal disposition is recorded in
`plans/closure/sam/368-status.md`.

Required compatibility policy:

- preserve every currently supported SAM 3.1 STREAM path;
- add the current normative PRIMARY/subsession behavior;
- use the current official SAM V3 specification and the pinned Java I2P
  implementation as the normative behavior sources; the current SAM version is
  3.3. Newer specification additions that do not change the negotiated version
  still require an explicit in-scope/out-of-scope disposition;
- inspect the pinned i2pd implementation as a compatibility diagnostic only.
  i2pd behavior must not override the current specification or Java I2P, and
  lack of support in i2pd must not block this plan;
- use `PRIMARY` as the normative style and support the specification's
  `MASTER` backward-compatibility alias; do not make the alias the internal or
  normative model;
- never advertise a 3.3 command that maps to an inert/no-op implementation.

A compatibility alias may share the same internal state model but must not
create a second semantic owner.

## 3. Existing substrate to reuse

Plan 368 must remain an adapter over existing production owners.

Already present in i2pr:

- local Destination lifecycle and tunnel pools in `i2pr-client`;
- Streaming protocol/runtime and `StreamingDestinationAdapter`;
- protocols 17/18 repliable Datagram1 and raw datagram substrate;
- protocol 19/20 Proposal 163 Datagram2/Datagram3 codecs and bounded receive handling;
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
- DATAGRAM (I2CP protocol 17), DATAGRAM2 (protocol 19), DATAGRAM3
  (protocol 20), and RAW (protocol 18) child send/receive without granting
  private apps host UDP forwarding;
- latest unversioned SAM additions in the selected command surface:
  per-datagram `SEND_TAGS`, `TAG_THRESHOLD`, `EXPIRES`, and `SEND_LEASESET`
  overrides, plus `NAMING LOOKUP OPTIONS=true` LeaseSet option retrieval with
  bounded, sanitized `OPTION:` response fields. The send controls are optional
  in the official SAM spec and may be ignored by a server; because the current
  canonical datagram request cannot carry them, this profile explicitly rejects
  UDP packets containing any of those fields and records a typed rejection in
  the loopback service diagnostics. It must never enqueue them as if honored;
- naming/NAME=ME behavior against the primary Destination;
- typed duplicate/invalid ID/style/port/result behavior, including exact 0–65535 bounds and precedence of per-stream `FROM_PORT`/`TO_PORT` over session defaults;
- inbound STREAM listener selection by `LISTEN_PORT`, defaulting to the
  configured local `FROM_PORT`; it must not use the outbound `TO_PORT`;
- deterministic teardown on primary control loss.

Do not include FORWARD as a managed-app requirement. Existing ordinary SAM
FORWARD behavior remains separate and private managed-app policy continues to
deny host-target forwarding.

Explicit disposition of other current SAM V3 features: authentication and TLS,
SSL STREAM FORWARD, optional QUIT/STOP/EXIT commands, and unrelated router
tunnel/I2CP option expansion remain outside this plan. Preserve existing
behavior where already implemented; otherwise return a typed unsupported/error
result. Negotiating 3.3 does not imply these optional facilities. `MASTER` is
accepted as a compatibility alias for `PRIMARY`.

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

For protocol 17 (Datagram1):

- preserve sender authentication/signature semantics owned by
  `i2pr-client::datagram`;
- preserve source/destination I2P ports;
- bound message size and receive queues;
- surface typed queue-full/invalid-signature/malformed errors.

For protocol 18:

- preserve raw datagram semantics;
- preserve source/destination I2P ports;
- support the official SAM RAW `PROTOCOL` and `LISTEN_PROTOCOL` ranges,
  defaulting outbound and inbound protocol to 18; reject 6 for either RAW
  protocol field and reject 17/19/20 for outbound `PROTOCOL` because those
  have defined datagram envelopes;
- route inbound RAW by `LISTEN_PROTOCOL` and `LISTEN_PORT`, including zero
  wildcards, and preserve the selected I2CP protocol in `HEADER=true` output;
- bind the receive path to the authenticated destination transport context
  available at the router layer;
- bound payloads and queues.

For protocol 19 (Datagram2):

- implement Proposal 163's flags/options/offline-signature/payload/signature
  layout and signing input exactly;
- verify the recipient-destination-hash prelude as part of the signature,
  rejecting invalid signatures before routing to the child;
- enforce replay resistance with a named, bounded replay cache and expiry policy;
  keep unexpired entries until expiry and reject new Datagram2 messages when
  the cache is full rather than evicting a live replay guard;
- preserve bounded options decoding and offline-signature expiry/key checks.

For protocol 20 (Datagram3):

- implement Proposal 163's source-hash/flags/options/payload layout exactly;
- expose the source hash as unauthenticated metadata, never as a verified sender;
- require application-layer authentication for any private app operation that
  depends on sender identity.

For DATAGRAM/RAW send controls introduced in SAM 3.3:

- The official SAM specification makes these controls optional. The current
  canonical destination send owner cannot carry or honor them, so this profile
  rejects any UDP packet containing `SEND_TAGS`, `TAG_THRESHOLD`, `EXPIRES`, or
  `SEND_LEASESET` before enqueue and records a payload-free diagnostic. It does
  not claim that these optional controls work, and it must never silently accept
  a control as honored.

For Proposal 167 naming options:

- `NAMING LOOKUP OPTIONS=true` may target a name or full Destination and
  returns only bounded valid LeaseSet option key/value pairs;
- filter keys containing `=` and keys/values containing line breaks, and
  return `LEASESET_NOT_FOUND` for an explicit options lookup with no LeaseSet;
- preserve ordinary lookup behavior when `OPTIONS` is absent/false.

SAM is only the application framing/control adapter. It must not reinterpret
I2P BitTorrent DHT payloads.

## 8. Datagram2/Datagram3 scope

The current official SAM specification includes DATAGRAM2 and DATAGRAM3 styles
without a new version number. They are in scope for Plan 368 alongside
DATAGRAM/RAW. Java I2P 2.13.0 implements them at the pinned reference revision.
The i2pd diagnostic is not an oracle and its lack of these features does not
change the requirement. Follow Proposal 163 and the current Datagram
specification; do not infer their formats from Datagram1 or RAW.

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

### Pinned Java I2P — normative implementation qualification

- 3.3 HELLO;
- primary creation;
- STREAM child;
- DATAGRAM child;
- DATAGRAM2 child;
- DATAGRAM3 child;
- RAW child;
- shared `NAME=ME` Destination across all children;
- child removal;
- primary close tears down children.

### Pinned i2pd — non-gating compatibility diagnostic

Run only the rows supported by the pinned i2pd revision and record unsupported
rows as such. A missing or divergent i2pd feature is not a Plan-368 failure and
must not change i2pr's specification-defined behavior. Record:

- PRIMARY versus MASTER spelling;
- SESSION ADD/REMOVE compatibility;
- STREAM behavior;
- DATAGRAM1/DATAGRAM2/DATAGRAM3/RAW framing where supported;
- shared Destination;
- any version/feature differences.

Initial source inspection at the pinned 2.61.0 commit found Datagram2/3 codecs,
but SAM `ProcessSessionAdd` and `ProcessSessionRemove` explicitly report
unsupported, and the bridge recognizes MASTER rather than PRIMARY. This is a
source diagnostic only; the bounded runtime diagnostic rows remain to be run,
and those differences do not reduce the Java/specification acceptance matrix.

### i2pr self-product

Use real loopback SAM sockets and the private managed-app raw-stream origin.
The private-origin matrix is mandatory because that is the consumer path.

## 12. Torrent-specific acceptance fixture without torrent coupling

Do not depend on i2pr-tc from the router repository.

Instead add a generic shared-Destination fixture:

1. create one primary;
2. add STREAM, DATAGRAM, DATAGRAM2, DATAGRAM3, and RAW children;
3. resolve `NAME=ME` and record one Destination hash;
4. exchange exact bytes over STREAM;
5. exchange protocol-17, protocol-19, protocol-20, protocol-18, and one
   application-defined RAW protocol datagrams;
6. prove all children are routed under the same local Destination;
7. remove one child and prove siblings remain usable;
8. drop primary control and prove all remaining children are torn down.

That fixture is the upstream contract C003 consumes.

## 13. Ordered work packages

### WP1 — specification/reference freeze

Pin the current official SAM V3 specification, Java I2P 2.13.0, and the
repository's i2pd 2.61.0 diagnostic revision. Record the latest negotiated
version, unversioned additions, and PRIMARY/MASTER differences before changing
code. The specification and Java I2P are normative; i2pd is diagnostic only.

### WP2 — runtime-neutral primary/child protocol model

Extend parser/replies/state transitions and strict bounds without sockets.
Match the pinned Java response shape for child operations:
`SESSION STATUS RESULT=OK ID="<child-id>"` on add/remove success. A child
operation must not return or re-encode the primary's private destination.

### WP3 — primary lifetime composition

Bind one control owner to one existing `DestinationRuntime`; deterministic
teardown and generation rules.

### WP4 — STREAM child composition and I2P port semantics

Reuse existing Streaming managers/registries without reopening the 3.1 stream
implementation. Implement and test the 3.2+ `FROM_PORT`/`TO_PORT` semantics carried
by SAM 3.3 so a managed application can target a nonzero remote I2P service port without
a localhost I2PTunnel proxy.

### WP5 — DATAGRAM1/2/3 and RAW child composition

Bridge child framing to the canonical destination data plane for protocols
17–20. Add Proposal 163 codecs, recipient-bound signature verification,
replay-cache bounds, Datagram3's explicitly unauthenticated sender metadata,
and protocol/port-based inbound child dispatch.

### WP6 — private managed-app datagrams and isolation

Run the same primary/child machinery through Plan 354/355 private streams and
prove cross-principal denial/revocation. Preserve the exact-byte `sam` stream
contract. Add a bounded private manager-protocol datagram operation so an app
can send/receive through protocols 17–20 without acquiring a host UDP socket or
host UDP forwarding authority. The operation is scoped to the app principal,
primary, child ID, destination, I2CP protocol, and ports; it cannot name a
host endpoint. Application-defined RAW protocol values remain opaque to SAM.

### WP7 — Java I2P qualification and i2pd diagnostics

Execute the required matrix against pinned Java I2P and correct only
specification-defined incompatibilities. Run the bounded i2pd diagnostic where
available; preserve its results without allowing them to redefine the profile
or gate completion.

### WP8 — support/docs/closure

Update the SAM roadmap, support inventory, protocol dossier, managed-app
reference documentation, and closure evidence. Preserve historical SAM 3.1
records.

### Qualification checkpoint — 2026-10-10

The staged SAM 3.3 profile now has a loopback TCP test that negotiates 3.3,
creates a PRIMARY, adds/removes a STREAM child with `FROM_PORT=25` and
`TO_PORT=110`, checks the exact quoted `SESSION STATUS ... ID="child"`
response, verifies the child shares the primary Destination, adds a RAW child,
and proves primary-control loss releases the session and Destination. The
test-only ceiling was used during staged qualification. After the full matrix
passed, production negotiation moved to `[3.1, 3.3]`. The profile is wired into
the SAM acceptance harness and its evidence-integrity checker. A private
origin test also proves that multiple SAM raw connections in one app instance
share the child's ID, a second app instance cannot resolve it, and primary
loss tears the private session down. A second loopback TCP test sends STREAM
traffic to ports 110 and 65535 through children of one PRIMARY. It also proves
that omitting per-stream `TO_PORT` leaves the zero default and does not reach
the port-110 listener. The private managed-app SAM origin runs the same
110/65535 and omitted-port matrix through inherited raw SAM streams.
The ordinary SAM datagram bridge also has a shared-PRIMARY fixture that adds
DATAGRAM, DATAGRAM2, DATAGRAM3, RAW protocol 18, and application-defined RAW
protocol 42 children. It sends through each child into the canonical
`DatagramManager`, verifies a shared Destination hash and selected protocol
and ports, then injects valid inbound envelopes and verifies protocol/port
specific child delivery. The same fixture drives bounded private manager-
protocol Send/Receive requests for each protocol, confirms they reach the same
per-primary canonical datagram owner, and confirms RAW child replies strip
sender identity metadata. This private fixture carries no host endpoint and
runs with no SAM UDP listener. These are local data-plane and process-boundary
tests, not Java interoperability evidence.
The new stream regressions exposed and fixed a lifecycle defect: ending the
last stream on a child used to tear down the shared Destination. Last-stream
cleanup still closes standalone sessions as before, while a child keeps its
shared Destination alive until SESSION REMOVE or primary close.

The first pinned Java 2.13.0 loopback run exposed two interoperability
requirements not covered by the local Rust client: Java sends
`HELLO VERSION MIN=1.0 MAX=3.3`, and its `NAMING REPLY` handler correlates on
the echoed `NAME`. Version negotiation now intersects ordered ranges across
major-version boundaries; successful and failed valid-name replies echo the
requested name. The unmodified pinned Java `SAMStreamSink` completes HELLO,
PRIMARY creation, STREAM/DATAGRAM/RAW child operations, `NAME=ME`, Datagram1
receive, and primary-control teardown. A separate Java wire peer checks
STREAM, DATAGRAM1/2/3, and RAW child operations on a single PRIMARY, exercises
DATAGRAM1/2/3 receive forwarding, and verifies a bidirectional STREAM byte
round trip through that PRIMARY's Java-created STREAM child. The evidence
checker records these rows separately from the unmodified client row.

The exact `FROM_PORT`/`TO_PORT` routing boundaries now pass on ordinary and
private origins. The bounded i2pd diagnostic also ran at the exact 2.61.0 pin
with SAM loopback-only, NTCP2 bound to loopback, SSU2 disabled, and reseed and
addressbook subscriptions disabled. Its runtime reports a 3.3 HELLO and rejects
`SESSION CREATE STYLE=PRIMARY`. Pinned-source rows record MASTER-only primary
spelling, STREAM-only subsession add, master-owned removal, and standalone
DATAGRAM1/2/3 plus RAW styles. This remains non-gating compatibility evidence;
it does not change the Java/specification profile. Sanitized evidence is checked
by `scripts/check-sam368-i2pd-evidence.sh`.

The ordinary shared-profile loopback test now combines the port-aware STREAM
exchange and DATAGRAM1/2/3, RAW 18, and custom RAW 42 children under one PRIMARY.
It checks shared Destination identity, canonical protocol/port routing, and
inbound forwarding to a loopback UDP client. The Java matrix likewise combines
its child operations and receive traffic with a same-PRIMARY STREAM exchange.
Neither lane proves a live remote I2P tunnel round trip.

## 14. Verification floor

Run the repository routine floor plus targeted:

- SAM parser/state tests;
- exact SESSION ADD/REMOVE response transcript tests, including absence of a
  DESTINATION field;
- SAM acceptance checker extensions;
- datagram manager tests;
- Proposal 163 golden-vector, malformed-input, offline-signature, recipient
  binding, replay, options, Datagram3 unauthenticated-source, and queue-limit
  tests;
- primary/child lifecycle property tests;
- private app-principal isolation tests;
- loopback shared-Destination integration;
- port-aware STREAM integration with a controlled nonzero destination port, including
  a max-boundary case and a negative proof that omitting `TO_PORT` does not accidentally
  reach the nonzero-port service;
- external Java I2P matrix;
- bounded i2pd diagnostic matrix; unsupported rows are recorded, not treated as
  failed required evidence.

Environment-gated external rows must fail explicitly when invoked without
required infrastructure; ordinary CI may keep them ignored according to the
existing evidence policy.

## 15. Acceptance criteria

Plan 368 closes only when:

1. i2pr negotiates and honestly advertises the implemented SAM 3.3 profile;
2. a long-lived primary owns one Destination and tunnel set;
3. STREAM, DATAGRAM1, DATAGRAM2, DATAGRAM3, and RAW children share exactly that
   Destination;
4. STREAM reuses the existing Streaming owner;
5. DATAGRAM1/2/3 and RAW reuse canonical protocol-17–20 data-plane owners;
6. latest SAM V3 unversioned send controls are honored or explicitly rejected
   before enqueue, and Proposal 167 naming options work with bounded sanitized
   responses; no control is silently accepted as honored;
7. child removal leaves siblings healthy;
8. primary loss tears down all children deterministically;
9. private managed-app SAM streams support the same semantics;
10. cross-app child/session attachment is impossible;
11. no private DATAGRAM/RAW path grants host UDP authority;
12. Java I2P interoperability is recorded;
13. pinned i2pd diagnostics and the PRIMARY/MASTER disposition are recorded;
    unsupported i2pd features do not gate completion or alter the normative
    profile;
14. SAM 3.1 regression tests remain green;
15. SAM 3.3 STREAM CONNECT honors a nonzero `TO_PORT` on both ordinary and private
    managed-app origins, with exact port-boundary evidence;
16. full routine/guard/documentation floor passes.

## 16. Stop conditions

Stop and register a successor/corrective if:

- the official 3.3 profile cannot be reconciled with current deployed Java I2P;
- protocol 17–20 delivery would bypass canonical destination routing;
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
- pinned i2pd diagnostic results and PRIMARY/MASTER decision;
- SAM 3.1 regression evidence;
- routine/guard results;
- residual findings and downstream unblock decisions for i2pr-tc C003 and
  i2pr-mail's port-aware managed-app transport.
