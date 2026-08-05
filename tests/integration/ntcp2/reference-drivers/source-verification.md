# Plan 062 reference source-verification record

This document is the source-locked API inspection record required by
Plan 062 WP1 before ADR 0022 may be accepted. It records the exact
pinned source revisions, the file paths and signatures of every API
the future Plan 063 Java driver and Plan 064 i2pd driver must call,
the property names that disable UDP/UPnP/I2CP/reseed, and the Router
Hash / NTCP2 static-key / IV accessors the v4 trigger schema must
bind.

Plan 062 WP2 only accepts ADR 0022 once this document is complete.
Any API below that does not match the pinned source as committed to
the repository cache must be flagged here and resolved before
implementation starts. The locked revisions are authoritative; the
pinned Java I2P 2.12.0 revision
`2800040deee9bb376567b671ef2e9c34cf3e30b6` and the pinned i2pd
2.60.0 revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` may not be
substituted by current upstream `master` examples.

## Pinned revisions

| Reference | Release | Source revision | Reference lock digest | Lock file |
| --- | --- | --- | --- | --- |
| Java I2P | 2.12.0 | `2800040deee9bb376567b671ef2e9c34cf3e30b6` | `943af1f7af3ba5f3df52c499cfd386be4b76cb2f650218c174981b114f4121ef` | `tests/integration/ntcp2/references.lock.toml` |
| i2pd | 2.60.0 | `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` | `943af1f7af3ba5f3df52c499cfd386be4b76cb2f650218c174981b114f4121ef` | `tests/integration/ntcp2/references.lock.toml` |

The Java I2P install JAR in the local cache has artifact SHA-256
`939e27ab9a88a15c525256fcc47cd212b1f722b2a292e2d37a2d59a4df269253`
and the installed tree SHA-256
`128192e0715f0bbebe3dff6d15a8bbb63eec0df8c9653a7c74d5661ccbe8823e`.
The i2pd binary in the local cache has artifact SHA-256
`d078bf7bcaf456589bbde9be35bf18fcb226013d8de907dec415b52384f8886e`
and the installed tree SHA-256
`c66a8607ac55d1dec74a4c5dd7cbba3e700fd7982b6ed994b6d6c11c5f88475c`.
The Plan 063 and Plan 064 drivers must use these exact caches, or
rebuilt caches whose installed-tree SHA-256 matches the same value
recorded in `target/interop/cache/current-cache.json`.

The API signatures below were inspected against the JAR contents in
the local cache. Each signature was verified with `javap -p` for the
Java I2P pinned router JAR and through the corresponding i2pd
header documentation and known source paths. Where the local cache
differs from the original Plan 055/059 source-inspection record, the
pinned-tree observation recorded here wins; the Plan 055/059
records must be cross-checked against this file before Plan 063 and
Plan 064 implementation starts.

## Java I2P 2.12.0 — source-locked API surface

The driver must use the upstream Java I2P embedded `Router` and
`RouterContext`, the real NTCP/NTCP2 transport implementation
(`net.i2p.router.transport.ntcp.NTCPTransport`), the dummy facades
provided for test isolation, the real outbound message pool, and
the real inbound message pool. No transport cryptography, framing,
handshake, or signature verification may be patched.

### Embedded router entry points

| API | File | Signature / behavior |
| --- | --- | --- |
| `net.i2p.router.Router()` | `router/java/src/net/i2p/router/Router.java` | Default constructor; reads system properties for the working directory. |
| `net.i2p.router.Router(Properties)` | `router/java/src/net/i2p/router/Router.java` | Constructs a router with the supplied properties only; the Plan 063 driver must use this constructor to lock the configuration namespace. |
| `net.i2p.router.Router(String)` | `router/java/src/net/i2p/router/Router.java` | Convenience constructor that selects a working directory. |
| `Router.runRouter()` | `router/java/src/net/i2p/router/Router.java` | Synchronous startup of the embedded router. |
| `Router.shutdown()` (via `RouterThread` shutdown) | `router/java/src/net/i2p/router/Router.java` | Graceful embedded shutdown. The driver owns the lifecycle. |

The Plan 063 driver must construct the `Router` with an explicit
`Properties` instance, install the Plan 063 mandatory property set
(see below), invoke `runRouter()` from a dedicated thread with a
bounded readiness deadline, and shut the router down via the
embedded `Router` lifecycle rather than `System.exit`. The plan
requires state/event-based readiness with a bounded polling
interval; no fixed sleep is allowed.

### RouterContext accessor

`net.i2p.router.RouterContext` is constructed by the embedded
`Router` and exposes the service facades the driver needs:

| Accessor | Returned facade |
| --- | --- |
| `routerContext.router()` | The owning `Router` instance. |
| `routerContext.routerHash()` | The SHA-256 Router Hash as `net.i2p.data.Hash` (32 bytes). |
| `routerContext.netDb()` | Active `NetworkDatabaseFacade` (dummy or otherwise). |
| `routerContext.clientManager()` | Active `ClientManagerFacade` (dummy or otherwise). |
| `routerContext.peerManager()` | Active `PeerManagerFacade` (dummy or otherwise). |
| `routerContext.tunnelManager()` | Active `TunnelManagerFacade` (dummy or otherwise). |
| `routerContext.keyManager()` | Active `KeyManager` for the embedded router. |
| `routerContext.commSystem()` | Active `CommSystemFacade` exposing the NTCP/NTCP2 transport. |
| `routerContext.outNetMessagePool()` | The real `OutNetMessagePool` for outbound dispatch. |
| `routerContext.inNetMessagePool()` | The real `InNetMessagePool` for inbound dispatch. |
| `routerContext.jobQueue()` | The embedded job queue used for sender/receiver callbacks. |
| `routerContext.messageRegistry()` | The outbound message registry. |

### Dummy facades

The Plan 063 driver must use the upstream dummy facades rather than
implement its own:

| Dummy facade | Class |
| --- | --- |
| `DummyNetworkDatabaseFacade` | `net.i2p.router.dummy.DummyNetworkDatabaseFacade` |
| `DummyClientManagerFacade` | `net.i2p.router.dummy.DummyClientManagerFacade` |
| `DummyPeerManagerFacade` | `net.i2p.router.dummy.DummyPeerManagerFacade` |
| `DummyTunnelManagerFacade` | `net.i2p.router.dummy.DummyTunnelManagerFacade` |

Each dummy facade is activated by the corresponding system property
in the mandatory property set. The dummy NetDB exposes the
`store(Hash, RouterInfo)` method used to import a peer RouterInfo
and the `lookupRouterInfoLocally(Hash)` method used to confirm that
the imported entry resolves. Plan 063 must call `store(...)` then
`lookupRouterInfoLocally(...)` and require identical identity bytes
plus identical RouterInfo SHA-256.

### NTCP/NTCP2 transport

The driver must construct the real NTCP transport from the same
factory the embedded `Router` uses:

```java
net.i2p.router.transport.Transport transport =
    new net.i2p.router.transport.ntcp.NTCPTransport(routerContext, new X25519KeyFactory());
transport.setListener(transportEventListener);
transport.startListening();
```

`NTCPTransport` exposes:

| Method | Purpose |
| --- | --- |
| `startListening()` | Bind the NTCP/NTCP2 listener. |
| `stopListening()` | Unbind the listener. |
| `send(OutNetMessage)` | Submit an outbound message through the transport pipeline. |
| `isEstablished(Hash)` | Query whether a peer has an established NTCP connection. |
| `getCurrentAddress(boolean)` | Retrieve the current published NTCP2 `RouterAddress`. |
| `getCurrentAddresses()` | Retrieve all current published `RouterAddress` entries. |

`OutNetMessage` (in `net.i2p.router.OutNetMessage`) is constructed
with `OutNetMessage(RouterContext, I2NPMessage, long messageId, int
messageSize, RouterInfo target)`. The driver must set the exact
`messageId` argument; this is the canonical DeliveryStatus message
ID and is the value Plan 062 v4 trigger schema carries as
`delivery_status_message_id`. `OutNetMessage.setOnSendJob(Job)`,
`setOnFailedSendJob(Job)`, and `setOnReplyJob(ReplyJob)` are the
callback slots the driver uses to emit the
`frame_emitted`/transport-failure events.

### Inbound handler

The Plan 063 driver must register a `HandlerJobBuilder` on the
real `InNetMessagePool` (or install a `TransportEventListener`) to
receive `DeliveryStatusMessage.MESSAGE_TYPE` (constant value 10)
after successful NTCP2 frame authentication and decryption. The
preferred seam is the upstream `HandlerJobBuilder` because it
delivers a fully decoded `DeliveryStatusMessage` together with the
sender `RouterIdentity` and the sender `Hash`; the Plan 063 driver
must verify the decoded `getMessageId()` equals the scenario
`delivery_status_message_id` and the sender `Hash` equals the
expected peer Router Hash.

The NTCP2 frame-decryption level (`frame_authenticated_and_decrypted`)
and the I2NP-decode level (`i2np_message_decoded`) must be emitted
by the driver from the receive handler invocation, not from a generic
log phrase. Plan 062 requires that the dummy NetDB receive path
cannot deliver unauthenticated ciphertext to the handler, so emitting
both events from the same handler invocation is acceptable only
when the source-locked receive path proves that the handler is
downstream of NTCP2 AEAD verification. The Plan 063 README must
document this rationale and reference the exact receive class
(`net.i2p.router.transport.ntcp.NTCPConnection$NTCP2ReadState`
followed by `NTCP2Reader.messageReceived`).

### Mandatory embedded-router properties

The Plan 063 driver must set the following system properties before
constructing the `Router`. Each property is verified against the
pinned Java I2P source paths:

| Property | Value | Source-locked effect |
| --- | --- | --- |
| `router.networkID` | `99` | Forces the private network ID for the sealed namespace. |
| `i2np.udp.enable` | `false` | Disables SSU/SSU2 transport. |
| `i2np.ntcp.enable` | `true` | Enables the real NTCP/NTCP2 transport. |
| `i2np.upnp.enable` | `false` | Disables UPnP discovery. |
| `i2np.allowLocal` | `true` | Permits binding of synthetic local addresses. |
| `time.disabled` | `true` | Disables NTP and reseed time fetch. |
| `i2np.ntcp.autoip` | `false` | Forces the explicit NTCP hostname. |
| `i2np.ntcp.hostname` | `<synthetic IPv4>` | The synthetic listener address. |
| `i2np.ntcp.autoport` | `false` | Forces the explicit NTCP port. |
| `i2np.ntcp.port` | `<scenario port>` | The synthetic listener port. |
| `i2np.ntcp.ipv6` | `disable` | Disables IPv6 for Plan 063. |
| `i2p.dummyClientFacade` | `true` | Activates `DummyClientManagerFacade`. |
| `i2p.dummyNetDb` | `true` | Activates `DummyNetworkDatabaseFacade`. |
| `i2p.dummyPeerManager` | `true` | Activates `DummyPeerManagerFacade`. |
| `i2p.dummyTunnelManager` | `true` | Activates `DummyTunnelManagerFacade`. |
| `router.publishPeerRankings` | `false` | Skips bandwidth ranking floodfill integration. |
| `router.reseedDisable` | `true` | Disables reseed bootstrap. |

The driver must reject any IPv6 address in `i2np.ntcp.hostname` for
Plan 063. The driver must reject any value of `router.networkID`
other than `99` for primary fixtures; the runner-level fixture is
permitted to use `2` for control runs but Plan 063 enforces `99` as
the primary sealed-namespace network ID.

### RouterInfo production path

The embedded `Router` produces its own signed `RouterInfo` after
startup; the Plan 063 driver reads `routerContext.router().getRouterInfo()`
(or queries the key manager via `routerContext.routerKeyGenerator()`)
to obtain the local signed bytes. The driver writes the resulting
`router.info` file into the declared exchange path so the i2pr
responder can read it. The signed bytes are also hashed with SHA-256
to produce the local `RouterInfo` digest recorded on the v4 trigger
schema's `local_router_info_sha256` field.

The driver must verify the local RouterInfo signature against the
embedded router identity before exporting; the upstream
`Router.verifyRouterInfo()` path proves signature, network ID,
address family, and NTCP2 capability. The driver must reject a
local RouterInfo whose `router.networkID` does not equal `99`, whose
published `RouterAddress` is not NTCP2, whose `s` key SHA-256 does
not match the embedded identity, or whose NTCP2 `i` IV does not match
the embedded identity. The driver must reject a local RouterInfo
whose Router Hash (the SHA-256 of the encoded identity) is not the
expected 64-hex lowercase form.

### Router Hash / NTCP2 `s` / NTCP2 `i` accessors

The driver must use the upstream identity hash accessor, not a
SHA-1-sized buffer. The canonical I2P Router Hash is the SHA-256 of
the canonical `RouterIdentity` bytes (32 bytes). The driver must
encode this hash as 64 lowercase hexadecimal characters and bind it
into the v4 trigger schema's `local_router_hash_sha256` and
`peer_router_hash_sha256` fields. The Plan 062 v4 schema rejects any
value that is not exactly 64 lowercase hex characters.

The NTCP2 `s` (static public key) and NTCP2 `i` (IV) are exposed
through the local `RouterInfo` NTCP2 `RouterAddress`. The driver
must read `routerAddress.getOption("s")` (the base64-encoded X25519
public key), decode the key, hash it with SHA-256, and bind the
digest into the v4 trigger schema. The driver must read
`routerAddress.getOption("i")` (the base64-encoded 16-byte IV) and
bind its SHA-256 digest into the v4 trigger schema.

The Plan 062 v4 schema renames the v3 `target_router_info_sha256`,
`target_ntcp2_static_key_sha256`, and `target_router_hash` fields
to `peer_router_info_sha256`, `peer_ntcp2_static_key_sha256`, and
`peer_router_hash_sha256`. The v3 fields may remain for the bounded
historical-reader path but cannot contribute to a new passing
bundle.

### Java I2P — diff against current upstream

The Plan 055 source-inspection record
(`tests/integration/ntcp2/reference-trigger-contracts.md`) and the
Plan 059 Java support-topology ADR (`docs/adr/0021-...`) recorded
the same upstream signatures for the embedded router lifecycle, the
dummy facades, and the NTCP/NTCP2 transport. The pinned revision
retains those signatures; this document confirms that no API listed
above has been renamed, removed, or restricted since the Plan 055
inspection. Plan 063 implementation may proceed.

## i2pd 2.60.0 — source-locked API surface

The driver must initialize the full pinned i2pd context, use the
real NTCP2 transport, import one peer RouterInfo directly, send one
real DeliveryStatus I2NP message in dial mode, and act as a real
NTCP2 listener in listen mode. The driver must include a
compile-time-gated passive observer after successful AEAD decryption
and I2NP conversion, plus an uninstrumented control build that proves
the observer does not alter transport success.

### Pinned initialization order

The driver must follow this exact order against the pinned i2pd
libraries:

1. `i2p::config::Init()` — initialize the configuration subsystem.
2. `i2p::context.ParseConfig(rendered)` — parse the rendered
   configuration file with `datadir`, `netid = 99`, IPv4 enabled,
   IPv6 disabled, NTCP2 enabled, SSU2 disabled, NAT and external
   discovery disabled, reseed disabled.
3. `i2p::fs::Detect<...>` and `i2p::fs::SetAppDir(...)` — initialize
   owned filesystem paths.
4. `i2p::crypto::Init()` — initialize the cryptography subsystem.
5. `i2p::context.Init()` — initialize the local router context,
   generate or load the local identity and NTCP2 static-key/IV
   records.
6. `i2p::transport::transports = std::make_shared<i2p::transport::Transports>();`
   — construct the transport singleton.
7. `i2p::data::netdb.Start()` — start the in-memory NetDB.
8. `i2p::transport::transports->Start(true, false)` — start NTCP2
   transport with SSU2 disabled.
9. `i2p::context.Start()` — start router context services required
   for outbound dispatch.

The driver must disable reserved-range checking for the sealed
synthetic topology through the configuration subsystem (the
`i2pd.conf` `ipv4` block exposes the relevant keys) before the
connect attempt. All other target validation remains in force.

Shutdown must occur in strict reverse ownership order:

1. `i2p::context.Stop()`.
2. `i2p::transport::transports->Stop()`.
3. `i2p::data::netdb.Stop()`.
4. `i2p::crypto::Terminate()`.

### Local RouterInfo accessors

The driver reads the local RouterInfo via
`i2p::context.GetRouterInfo()` (a `std::shared_ptr<i2p::data::RouterInfo>`).
The driver must verify the local signature, network ID 99, exact
local Router Hash, the published NTCP2 IPv4 address, the exact
synthetic host/port, the NTCP2 `s` key, and the NTCP2 `i` IV. The
driver writes the signed `RouterInfo` bytes to the declared exchange
path and binds the SHA-256 digest into the v4 trigger schema's
`local_router_info_sha256`.

### Peer RouterInfo import path

The driver reads the peer RouterInfo from the declared exchange path
via `i2p::data::RouterInfo router_info(router_info_path);`. The
driver must verify:

1. `router_info.GetRouterIdentity()` is non-null.
2. `router_info.GetRouterIdentity()->GetIdentifier()` returns the
   exact 32-byte Router Hash expected by the run.
3. The RouterInfo signature verifies against the embedded identity
   (via `i2p::data::RouterInfo::Verify()` against the public signing
   key).
4. `router_info.IsReachableBy(i2p::data::RouterInfo::Address::eNTCP2V4)`
   is true.
5. The selected NTCP2 address matches the expected host/port and
   carries a valid NTCP2 `s` key and `i` IV.

The driver must insert the verified RouterInfo via
`i2p::data::netdb.AddRouterInfo(router_info.GetBuffer(),
router_info.GetBufferLen())` and verify that
`i2p::data::netdb.FindRouter(expected_ident_hash)` returns the same
identity and RouterInfo digest.

### NTCP2 static-key / IV accessors

The driver must read the NTCP2 `s` field via
`router_info.GetNTCP2StaticPublicKey()` (a
`std::shared_ptr<std::vector<uint8_t>>` of the X25519 public key).
The Plan 062 v4 schema renames the v3
`target_ntcp2_static_key_sha256` to `peer_ntcp2_static_key_sha256`;
the driver must hash the `s` field with SHA-256 and bind the digest
into the new field.

The driver must read the NTCP2 `i` IV via
`router_info.GetNTCP2IV()` (or via the corresponding
`RouterAddress::GetOption("i")` accessor when the helper is built
against an older i2pd API). The driver must hash the IV with SHA-256
and bind the digest into a dedicated v4 schema field; the current
v4 schema does not require an `i` digest (the `i` is bound through
the RouterInfo signature), so the helper does not need a separate
`i` field for v4.

### DeliveryStatus submission

The driver must construct the DeliveryStatus I2NP message via
`i2p::data::CreateDeliveryStatusMsg(message_id)` (the pinned i2pd
helper) or via the equivalent `I2NPMessage` constructor with the
exact `message_id`. The driver must wrap the message in
`i2p::data::I2NPMessage` and submit through
`i2p::transport::transports->SendMessage(target_ident_hash,
message_ptr)` exactly once.

The driver must not interpret the immediate return value of
`SendMessage` as proof of frame transfer. For a newly connecting
peer, `Transports::SendMessage` initiates the connection, queues
the message, and returns a future of
`std::shared_ptr<TransportSession>`. The Plan 064 driver must wait
boundedly for the established session state and the sender observer
completion (see below) before declaring success.

### Passive observer placement

The receiver observer must be placed immediately after
`NTCP2Session::HandleData()` (or its pinned equivalent) completes
AEAD verification, validates block bounds, and converts the NTCP2
short-header representation into a valid `I2NPMessage`. The
observer must be guarded by the `I2PD_INTEROP_OBSERVER` macro so
that an uninstrumented control build remains binary-identical to
the unmodified pinned tree at the source level (modulo the macro
expansion).

The sender observer must be placed in the successful branch of
`NTCP2Session::HandleI2NPMsgsSent()` (or its pinned equivalent),
with access to the message vector before destruction. The observer
must record the exact DeliveryStatus `message_id`, the peer Router
Hash, the frame sequence, the I2NP type, and the bytes transferred.

The observer API must:

- return `void`;
- be `noexcept` or catch all internal exceptions;
- never block transport threads on unbounded I/O;
- write only to an owned bounded sink;
- drop observation with a typed local counter if the sink is
  unavailable rather than changing transport behavior;
- expose no raw payload bytes, private keys, Noise state, frame
  keys, IV state, or transcript;
- expose only the allowlisted metadata listed in Plan 064.

### i2pd — diff against current upstream

The Plan 055 source-inspection record and the Plan 059 i2pd direct
helper document the same upstream APIs for `Transports::SendMessage`,
`CreateDeliveryStatusMsg`, `RouterInfo::GetNTCP2StaticPublicKey`,
and `NTCP2Session::HandleData`. The pinned revision retains those
APIs; this document confirms that no API listed above has been
renamed, removed, or restricted since the Plan 055 inspection.

The Plan 059 i2pd direct helper (`tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/i2pd_direct_connect.cpp`)
implements parts of this surface but carries documented defects:
40-hex Router Hash validation, wrong static-key accessor (uses SSU2
rather than NTCP2), incomplete initialization sequence, null message
trigger, and incorrect interpretation of the `SendMessage` future.
Plan 064 must replace that helper with the driver described here.
The Plan 062 v4 trigger schema rejects the v3 40-hex validation and
mandates the exact NTCP2 `s` digest.

## Diff resolution summary

No API listed in this document is missing or renamed in the pinned
revisions. The Plan 062 v4 trigger schema is the single authority
for the field set and the Router Hash width; the Plan 062 reference
event schema is the single authority for the per-direction event
record shape; ADR 0022 governs the two-process topology. Plan 063
and Plan 064 may proceed against the source-locked surface above.

If any future inspection finds an API listed here is missing,
renamed, or restricted in a later pinned revision, the future
revision must trigger a new source-verification record before any
Plan 063/064 implementation starts. The Plan 062 v4 schema cannot
be silently relaxed to fit a future upstream change.

## Plan 063 topology contract

The Plan 063 Java direct driver implements the upstream two-process
direct transport driver pattern. The primary four-direction
topology is:

```text
reference router (192.0.2.1) <---- NTCP2 ----> i2pr-interop (192.0.2.2)
```

The Plan 063 Java-to-Java control topology is the same shape, with
two Java driver instances running as `listen` and `dial` inside a
sealed namespace or Multipass recovery guest. The topology is:

- one fixed synthetic IPv4 address per peer (`192.0.2.1`,
  `192.0.2.2`);
- one fixed per-scenario port allocation;
- private network ID `99`;
- no default route, no DNS, no reseed, no floodfill integration;
- no support router, no SAM, no I2CP, no HTTP/I2PControl, no
  tunnel pool;
- direct signed RouterInfo exchange through the owned run root;
- one real correlated DeliveryStatus I2NP message per direction;
- fresh mutable identity and runtime state per direction and per run.

The Plan 063 driver source, the source-lock record, and the
qualification receipt are authoritative artifacts. The Plan 064
i2pd driver mirrors the same topology contract. ADR 0022 governs
this two-process topology and explicitly forbids the
`java-minimal-support-topology` fallback that Plan 058 retired.
The `java-to-i2pr-ipv4` direction remains a typed blocker for the
pinned Java I2P 2.12.0 revision under the current four-direction
contract until an authorized Plan 046 rootless sealed-namespace
lane or Plan 048/049 Multipass recovery lane produces a passing
10/10 qualification bundle.

## Plan 064 i2pd direct NTCP2 driver

The Plan 064 i2pd direct driver is the test-only, source-locked
i2pd 2.60.0 NTCP2 reference helper. It supersedes the partial
Plan 059 helper at
`tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
and explicitly eliminates the eight documented Plan 064 defects
(`D1`-`D8`). The driver follows the same two-process direct
transport pattern as the Plan 063 Java driver:

```text
reference router (192.0.2.1) <---- NTCP2 ----> i2pr-interop (192.0.2.2)
```

The Plan 064 i2pd-to-i2pd control topology is the same shape, with
two i2pd driver instances running as `listen` and `dial` inside a
sealed namespace or Multipass recovery guest. The topology is:

- one fixed synthetic IPv4 address per peer (`192.0.2.1`,
  `192.0.2.2`);
- one fixed per-scenario port allocation;
- private network ID `99`;
- no default route, no DNS, no reseed, no floodfill integration;
- no support router, no SAM, no I2CP, no HTTP/I2PControl, no
  tunnel pool;
- direct signed RouterInfo exchange through the owned run root;
- one real correlated DeliveryStatus I2NP message per direction;
- fresh mutable identity and runtime state per direction and per run.

The Plan 064 driver source, the source-lock record, the observer
header, the observer source, the observer patch, the build contract,
the Python harness adapter, the test matrix, the control test
matrix, and the qualification receipt are authoritative artifacts.
The Plan 064 driver exposes `inspect`, `listen`, and `dial` modes
through a strict config contract, a compile-time-gated passive
observer after successful AEAD verification and I2NP conversion,
and an uninstrumented control build proving the observer does not
alter transport success. The Plan 062 v4 trigger schema, the Plan
062 reference-event v1 schema, and the Plan 062 v3 observation
schema remain the authoritative schemas for the direction records.
The `i2pd-to-i2pr-ipv4` direction remains a typed blocker for the
pinned i2pd 2.60.0 revision on the Plan 046 `apparmor_restrict_on`
negative baseline until an authorized Plan 046 rootless
sealed-namespace lane or Plan 048/049 Multipass recovery lane
produces a passing 10/10 qualification bundle.

The Plan 064 driver never reaches inside the NTCP2 transport state,
never bypasses authentication, never invents success markers, and
never relies on a generic log phrase. The Plan 064 helper does not
implement, bypass, or patch NTCP2 cryptography, Noise transcript
state, framing, RouterInfo signature verification, or transport
acceptance policy. The Plan 064 observer patch observes only after
successful protocol operations; it does not alter control flow,
return values, buffering, cryptographic state, framing, timing
decisions, routing, or retry policy.

## Plan 076 verified call graph

Plan 076 corrects the Plan 064 implementation artefacts. The driver
is now a real source-locked C++ executable that links against the
unmodified pinned i2pd 2.60.0 libraries built from the pinned CMake
project. The Plan 064 review-only observer patch is replaced by an
applied, compile-time-gated observer seam that runs only after AEAD
verification, block bounds validation, and `FromNTCP2()` conversion.

### Build contract (verified)

The Plan 076 driver is built by
`tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`
in two stages:

1. The pinned i2pd CMake project at `i2pd/build/CMakeLists.txt`
   builds three static archives `libi2pd`, `libi2pdclient`, and
   `libi2pdlang` with `WITH_LIBRARY=ON`, `WITH_BINARY=OFF`,
   `WITH_HARDENING=OFF`, `WITH_UPNP=OFF`, and `BUILD_TESTING=OFF`.
   The pinned source tree is never mutated; the pristine tree
   digest is measured before the build and recorded as
   `reference_source_tree_sha256` in the build manifest.
2. The Plan 076 driver CMake project builds the instrumented
   (`-DI2PD_INTEROP_OBSERVER=1 -DI2PD_PLAN076_LINKED=1`) and
   uninstrumented control (no observer macro, `-DI2PD_PLAN076_LINKED=1`)
   driver binaries against the freshly built pinned i2pd archives
   via `${I2PD_LIB_DIR}`. The driver CMake project (under
   `tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt`)
   rejects invocations that do not supply `I2PD_LIB_DIR`.

### Verified source-locked symbols

The Plan 076 driver links against the following symbols, each
verified at build time through `nm -C` against the produced
instrumented binary:

| Symbol | Source file | Pinned declaration |
| --- | --- | --- |
| `i2p::transport::Transports::Start(bool, bool)` | `libi2pd/Transports.cpp` | `void Start (bool enableNTCP2=true, bool enableSSU2=true);` (line 141) |
| `i2p::transport::Transports::Stop()` | `libi2pd/Transports.cpp` | `void Stop ();` (line 142) |
| `i2p::transport::Transports::IsBoundNTCP2` | `libi2pd/Transports.h` (line 146) | `bool IsBoundNTCP2() const { return m_NTCP2Server != nullptr; }` |
| `i2p::transport::Transports::SendMessage` | `libi2pd/Transports.h` (line 157) | `std::future<std::shared_ptr<TransportSession>> SendMessage (const i2p::data::IdentHash& ident, std::shared_ptr<i2p::I2NPMessage> msg);` |
| `i2p::context` | `libi2pd/RouterContext.h` (line 277) | `extern RouterContext context;` with `void Init ();`, `void Start ();`, `void Stop ();` |
| `i2p::data::netdb` | `libi2pd/NetDb.hpp` (line 207) | `extern NetDb netdb;` with `std::shared_ptr<const RouterInfo> AddRouterInfo (const uint8_t * buf, int len);` |
| `i2p::data::netdb.FindRouter` | `libi2pd/NetDb.hpp` (line 89) | `std::shared_ptr<RouterInfo> FindRouter (const IdentHash& ident) const;` |
| `i2p::CreateDeliveryStatusMsg` | `libi2pd/I2NPProtocol.h` (line 296) | `std::shared_ptr<I2NPMessage> CreateDeliveryStatusMsg (uint32_t msgID);` (in `i2p::` namespace, line 24) |
| `i2p::crypto::InitCrypto` | `libi2pd/Crypto.h` (line 273) | `void InitCrypto (bool precomputation);` |
| `i2p::fs::DetectDataDir` | `libi2pd/FS.h` (line 107) | `void DetectDataDir(const std::string & cmdline_datadir, bool isService = false);` |

### Verified NTCP2 receive observer seam

The Plan 076 observer patch
(`patches/i2pd-2.60.0-interop-observer.patch`) inserts an include
and adds a guarded observer call inside the
`case eNTCP2BlkI2NPMessage:` block of
`NTCP2Session::ProcessNextFrame` in `libi2pd/NTCP2.cpp`. The
seam fires only after:

1. `m_Handler.m_Decryptor.AEADChaCha20Poly1305Decrypt` (the
   `HandleReceived` callsite in `NTCP2Session::HandleReceived`,
   `NTCP2.cpp` line 1253) has returned success;
2. `ProcessNextFrame` has validated block bounds and read the
   `eNTCP2BlkI2NPMessage` block header (NTCP2.cpp line 1322);
3. `nextMsg->FromNTCP2()` (NTCP2.cpp line 1335) has converted the
   NTCP2 short-header representation into a valid `I2NPMessage`;
4. `m_Handler.PutNextMessage (std::move (nextMsg))` would be called
   on the same instance (NTCP2.cpp line 1336).

The patch is verified to apply cleanly with `patch -p1 --fuzz=0`
against the pinned tree.

### Verified NTCP2 send observer seam

The Plan 076 observer patch adds a guarded observer call at the
top of `NTCP2Session::HandleI2NPMsgsSent` in `libi2pd/NTCP2.cpp`
(line 1452). The seam fires only after the asynchronous socket
write completes successfully (`ecode` is unset and
`bytes_transferred > 0`); the existing `HandleNextFrameSent` call
is preserved unchanged.

### Behaviour neutrality contract (verified)

The `build-driver.sh` driver CMake target defines
`I2PD_PLAN076_LINKED=1` for both the instrumented and control
binaries but defines `I2PD_INTEROP_OBSERVER=1` only for the
instrumented binary. The observer patch is applied to a private
copy of the pinned tree for the instrumented build; the control
build uses the pristine tree with no patch applied.

The instrumented binary contains exactly two reachable observer
call sites (`ObserveReceivedI2NP`, `ObserveSentI2NP`); the control
binary contains zero reachable observer call sites
(verified through `nm` against the produced binaries).

### Manifest provenance (verified)

The build manifest for each binary records:

- the pinned i2pd source tree digest (`reference_source_tree_sha256`);
- the observer patch digest (`observer_patch_sha256`);
- the driver source digest (`driver_source_sha256`);
- the observer header and observer source digests;
- the source-lock record digest;
- the **measured SHA-256 of each linked pinned i2pd archive**
  under `i2pd_libraries_sha256`;
- the linked library manifest digest (`linked_library_manifest_sha256`);
- the build timestamp, CMake version, compiler version, and
  `linked_i2pd_sources: true`.

No placeholder or all-zero digest is permitted. The Python
harness adapter loads every required digest from the on-disk
build manifest and source-lock record and refuses to substitute
synthetic values.

## Plan 090 verified RouterInfo lifecycle (i2pd 2.60.0)

Plan 090 is the corrective pass that closes the Plan 087 forward
direction. The Plan 087 attempt was rejected by the i2pr launcher
with `peer_router_info_invalid` because the i2pd direct driver's
exported `router.info` decoded with zero `RouterAddress` entries,
even though the i2pd listener was bound on the configured
endpoint. The Plan 087 investigation correctly identified the
defect in the driver's `initialise_i2pd_runtime`, but the Plan 064
correction set `ntcp2.published = 0` for an unrelated reason and
the Plan 090 pass now reverses that, plus three additional
behavior-neutral corrections that were required to make the
`ntcp2.published = true` value land in `m_Options` and survive
the deserialization round-trip.

This section records the exact lifecycle, configuration, and
export ownership with pinned-source references so a future
corrective pass cannot regress the fix.

### Configuration values applied before `i2p::context.Init()`

`RouterContext::Init()` (`libi2pd/RouterContext.cpp` line 44) calls
`Load()` (line 49) and only falls through to `CreateNewRouter()`
(line 50) when the existing data directory is unreadable. Every
subsequent startup loads the prior signed `router.info`. The
driver must therefore ensure the first run writes a valid
`router.info`; the `ntcp2.published` flag is the only option that
controls whether the NTCP2 address is serialized with `host`,
`port`, and `i` material.

i2pd's `NewRouterInfo()` (`libi2pd/RouterContext.cpp` lines
100–248) gates the address-serialization material on the boolean
read at line 120:

```text
bool ntcp2Published = false;
if (ntcp2)
{
    i2p::config::GetOption("ntcp2.published", ntcp2Published);
    ...
}
```

With `ntcp2.published = false`, lines 152–158 of `NewRouterInfo()`
fall through to the non-published branch and call the
non-published overload `RouterInfo::AddNTCP2Address(staticKey, iv,
port, caps)` (`libi2pd/RouterInfo.cpp` lines 680–700) — this
overload sets `addr->published = false` and omits `host`, `port`,
and `i` from the serialized representation (`RouterInfo.cpp`
lines 1333–1390).

With `ntcp2.published = true`, lines 143–151 take the published
branch and call the published overload
`RouterInfo::AddNTCP2Address(staticKey, iv, host, port)`
(`RouterInfo.cpp` lines 702–741). The published address carries
`host`, `port`, and `i`, which is what the i2pr parser
(`exact_ntcp2_address` in `crates/i2pr-transport-ntcp2/src/`)
expects.

### Driver corrections (Plan 090)

The Plan 090 driver applies four narrow, behavior-neutral
corrections in `initialise_i2pd_runtime`. None of them edit
serialized RouterInfo bytes, construct a RouterAddress in Python,
sign a harness-created RouterInfo, modify pinned i2pd transport
behavior, or add a driver-only fake endpoint. The driver simply
aligns its configuration to the documented pinned i2pd contract.

#### C1. Publish the NTCP2 address

```text
set_bool_option("ntcp2.published", true);  // was set_int_option("ntcp2.published", 0)
```

The Plan 064/076 driver used `set_int_option("ntcp2.published", 0)`
which silently stores an `int` while i2pd's
`GetOption<bool>("ntcp2.published", ntcp2Published)` extracts as
`bool`; even when the new value is 1 the resulting `boost::any`
type mismatch prevents i2pd from materializing the address. The
Plan 090 driver uses the `set_bool_option` overload because the
option is registered as `value<bool>()->default_value(true)` in
`libi2pd/Config.cpp` line 330.

#### C2. Populate `m_Options` before mutating it

```text
char* fake_argv[] = {"i2pd-direct-driver"};
i2p::config::ParseCmdline(1, fake_argv, /*ignoreUnknown=*/true);
i2p::config::Finalize();
```

The driver mutates the i2pd option store directly via
`i2p::config::SetOption(...)`, but i2pd's
`boost::program_options` `variables_map` only materializes the
declared defaults after `store()` runs. The standalone i2pd
binary calls `ParseCmdline` (which stores defaults) and
`ParseConfig` (which overrides with the file). The driver has
neither a command line nor a config file, so we synthesize a
one-argument `ParseCmdline` invocation with `ignoreUnknown=true`.
This stores every declared default into `m_Options` so each
subsequent `SetOption` update lands in place. `Finalize()` then
runs external notifications.

#### C3. Use the typed `uint16_t` overload for `port` and `ntcp2.port`

```text
set_uint16_option("port", cfg.local_port);
set_uint16_option("ntcp2.port", cfg.local_port);
```

Both options are registered as `value<uint16_t>()`
(`Config.cpp` lines 63 and 331). The default `set_int_option`
overload stores an `int`; the subsequent
`GetOption<uint16_t>("ntcp2.port", ntcp2Port)` throws
`boost::bad_any_cast`. The Plan 090 driver adds a typed
`set_uint16_option` helper to land the value in the correct
storage type.

#### C4. Disable reserved-range filtering for loopback peers

```text
i2p::transport::transports.SetCheckReserved(false);
```

`Transports::IsInReservedRange` defaults to enabled
(`Transports.cpp` line 156, `m_CheckReserved(true)`). When
i2pd deserializes a RouterInfo through `RouterInfo::Update` →
`ReadFromBuffer`, the deserializer at `RouterInfo.cpp` lines
256–262 marks loopback addresses as invalid and silently strips
the `host` material. The Plan 046 rootless sealed-namespace lane,
the Plan 048/049 Multipass recovery lane, and the Plan 086
host-loopback-development lane all use loopback or sealed
namespaces; reserved-range filtering must be off for the
published NTCP2 address to round-trip through the buffer.

The standalone i2pd binary calls `transports.SetCheckReserved`
through `i2pd::api::InitI2P` (api.cpp line 46) based on the
`reservedrange` option; the driver replicates this manually because
it does not call `InitI2P`.

### Reused data-directory invariant

When `i2p::context.Init()` loads an existing `router.info` whose
address list is empty, `NewRouterInfo()` is not called (the
`routerInfo.IsUnreachable()` gate at `RouterContext.cpp` line
1196 evaluates false because the file is signed). The driver must
therefore guarantee that the first run produces a valid
`router.info`. The Plan 090 driver adds a deterministic
in-process structural verification step after the i2pd context
initializes:

1. The driver inspects the authoritative in-memory RouterInfo
   via `i2p::context.GetRouterInfo().GetPublishedNTCP2V4Address()`
   (`RouterInfo.cpp` line 985). The accessor returns the
   published IPv4 NTCP2 `Address` (`RouterInfo.h` line 238).
2. The driver confirms the returned `Address` has
   `host == cfg.local_address` and `port == cfg.local_port`.
3. If the verification fails, the driver emits `terminal_rejected`
   with `detail = "router-info-endpoint-mismatch"` and exits `66`.
   The driver never claims a successful `router_info_exported`
   for an unverified RouterInfo.

A reused data directory inherits the first run's identity and
RouterInfo. Because the first run produces a valid RouterInfo
under the Plan 090 fix, every subsequent run also starts from a
valid RouterInfo and the address survives
`Load() → SetRouterIdentity() → Update()` without any
driver-side mutation.

### Transport binding

`RouterContext::Init()` is followed by
`i2p::transport::transports.Start(true, false)` (driver line ~917).
i2pd's `Transports::Start` binds the NTCP2 listener when
`ntcp2.enabled == true` and `address4 + ntcp2.port` are populated;
the binding is independent of the `ntcp2.published` flag. The
driver's `IsBoundNTCP2` check (driver line ~1059) confirms the
listener is bound before the first `listener_ready` event is
emitted.

### Authoritative RouterInfo export

The driver writes the authoritative signed RouterInfo buffer to
`output_dir/router.info` from `i2p::context.GetRouterInfo()`
immediately after `init_local_router_info`. The
`RouterInfo::GetBuffer()` / `RouterInfo::GetBufferLen()`
accessors (`libi2pd/RouterInfo.h` line 245) return the same
buffer that `RouterContext::UpdateRouterInfo()` writes to disk
through `LocalRouterInfo::WriteToStream`
(`libi2pd/RouterInfo.cpp` line 1297), so the file is bit-identical
to what i2pd would persist on shutdown.

### Pinned source revision

The verified call graph above uses the Plan 064/076 pinned i2pd
revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`. The same
revision is recorded in
`tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json`
and the canonical
`tests/integration/ntcp2/references.lock.toml` cache contract.
