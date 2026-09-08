# I2CP and service tunnels

Status: **required**  
Primary roadmap milestones: **9–10**  
Dependencies: destinations, NetDB, streaming and router lifecycle

> Plan 164 note: the M9 I2CP wire/profile foundation is landed
> (`crates/i2pr-api/src/i2cp/`, fixtures under
> `tests/fixtures/i2cp/`). All service-tunnel material in this
> dossier (HTTP, SOCKS5, generic TCP, IRC) is **Milestone 10** scope
> and is unchanged by Plans 164–170.
>
> Plan 165 note: the connection state machine, canonical SessionConfig
> signature/date/ceiling verification, bounded option disposition
> table and projection into `i2pr-client::DestinationConfig`,
> bounded session registry with reserve/commit/rollback,
> reconfiguration taxonomy, and typed `I2cpAction` vocabulary are
> landed (`crates/i2pr-api/src/i2cp/{connection,verify,config,
> session,actions}.rs`). No listener, no destination activation, and
> no interoperability claim; those belong to Plans 166–170.
>
> Plan 166 note: `DestinationOwnership::{RouterOwned, ClientOwned}`,
> `DestinationPublic`, `InboundDecryptionCapability`, atomic
> `install_client_lease_set2`, typed `LeaseRequest` /
> `ClientRefreshCause`, and `I2cpAction::RequestVariableLeaseSet`
> are landed (`crates/i2pr-client/src/{identity,leaseset,
> registry}.rs` and `crates/i2pr-api/src/i2cp/actions.rs`).
> SAM router-owned regressions remain green; no listener, no
> interoperability claim.
>
> Plan 167 note: the loopback I2CP v0.9.67 listener/runtime is
> landed (`crates/i2pr-daemon/src/i2cp.rs` plus
> `crates/i2pr-daemon/tests/i2cp_loopback.rs`). Disabled by default,
> loopback-only, bounded read/write/session resources, real-TCP
> `0x2a` preamble, fragmented frame reads, multiple frames per
> write, signed `SessionConfig` reservation, signed Standard
> LeaseSet2 + matching X25519 decryption key installation,
> mismatched-key rejection, disconnect cleanup, and graceful
> daemon cancellation. Application-message transport
> (`SendMessage` / `SendMessageExpires` / `MessagePayload` /
> `MessageStatus`), `DestLookup` / `DestReply` / `HostLookup` /
> `HostReply`, reconfiguration, and independent-client evidence
> remain in Plans 168–170.
> Normative M9 sources: official I2CP specification and overview at
> `i2p/i2p.website @ 26467e4b275e3a58280b9d4e6d4745d58bb8c499`
> (accurate for API 0.9.67); Java I2P 2.13.0
> (`i2p/i2p.i2p @ 9134f808337b401e8e53c73734c81fab04280c9d`) and
> go-i2cp (`go-i2p/go-i2cp @ b529ee1c10a6011558b4d69fc9436a4afc489eac`)
> as unmodified inspected references. See [SOURCES.md](../SOURCES.md)
> and the M9 section below.

## Scope

This dossier covers two related but distinct layers:

1. I2CP, the low-level client-to-router protocol for destination sessions, LeaseSets, messages, lookups and bandwidth information.
2. Local service adapters that map ordinary application protocols onto I2P streaming: HTTP client proxying, SOCKS5, generic TCP client/server tunnels and IRC-specific profiles/filters.

Service adapters must consume the shared destination/streaming APIs. They must not bypass destination lifecycle, tunnel pools, naming policy or resource accounting.

## Authoritative sources

- [I2CP specification](https://i2p.net/en/docs/specs/i2cp/), pinned in [SOURCES.md](../SOURCES.md), updated 2025-07 and accurate for 0.9.67.
- [I2PTunnel documentation](https://i2p.net/en/docs/api/i2ptunnel/) for adapter behavior and terminology.
- [SOCKS documentation](https://i2p.net/en/docs/api/socks/) and current Java/i2pd implementations for supported command/address behavior.
- HTTP proxy, streaming, naming and IRC application documentation in the official I2P site and implementations.
- Common structures and LeaseSet specifications for private keys and session messages.

The official I2CP document describes a low-level protocol primarily implemented by Java I2P and also externally by i2pd. It recommends SAM for most non-Java applications. `i2pr` still includes I2CP because the MVP roadmap requires parity for lower-level clients and existing software.

## I2CP required MVP subset

### Connection and framing

- Loopback-only TCP listener by default.
- Initial protocol byte and version/date negotiation exactly as specified.
- Length-prefixed typed messages with strict maximum size.
- Explicit connection/session states and legal message sequencing.
- Deadlines for initial negotiation, session creation and idle control connections.
- Unknown/unsupported message types rejected without desynchronizing framing.

### Sessions

Implement router-side behavior for:

- Create Session and Session Status;
- Request/Create LeaseSet exchange for supported LeaseSet variants;
- Reconfigure Session where safely supported;
- Destroy Session and disconnect cleanup;
- one or more sessions per connection only after current multi-session semantics are verified;
- destination-key ownership and offline signatures;
- validated tunnel/streaming option allowlist.

The I2CP specification states that sessions are not recoverable after connection loss. Close must destroy owned session state according to protocol semantics and release destinations/tunnels unless a future explicit persistence mode is designed.

### Messaging and lookup

Implement the messages required for:

- destination lookup and reply;
- outgoing client message submission;
- incoming message notification/retrieval or current delivery form;
- message status/reliability semantics selected by the MVP;
- bandwidth limits;
- LeaseSet creation/publication callbacks.

I2CP exposes lower-level destination messages, not only streaming. Keep message protocol/port fields and size limits explicit, and do not allow clients to bypass router-wide egress/resource policy.

## I2CP security policy

- Authenticate remote/non-loopback deployments before they are supported; baseline is loopback only.
- Bound connections, sessions, pending messages, destination lookups and queued inbound payloads.
- Never log private destination keys or full message payloads.
- Validate every option and reject unknown values unless the spec defines safe forwarding.
- Prevent one connection/session from referring to another client’s session ID or message state.
- Destroy sessions and sensitive state on malformed sequences according to a documented error matrix.
- Avoid Java object-serialization assumptions; implement only the wire protocol.

## Service-tunnel architecture

Each adapter should be a small supervised service with:

- validated listener/bind configuration;
- explicit destination/session ownership;
- naming/address resolution through the shared client layer;
- bounded accepts, connects, buffers and idle lifetime;
- cancellation and graceful shutdown;
- privacy-aware logs;
- protocol-specific sanitization before forwarding.

### HTTP client proxy

Required behavior:

- HTTP proxy request parsing with strict request-line/header limits;
- resolve `.i2p`, Base32 and configured names without clearnet DNS leakage;
- map requests to I2P streaming destinations and appropriate destination ports;
- remove or rewrite proxy-only and privacy-sensitive headers according to documented policy;
- avoid forwarding absolute-form URLs or `Host` values that expose local proxy details incorrectly;
- return bounded local error pages/statuses;
- support ordinary HTTP/1.x proxying first.

HTTPS CONNECT to arbitrary clearnet destinations and clearnet outproxy operation are outside the MVP. CONNECT behavior for I2P destinations should be added only with explicit semantics and tests.

### SOCKS5 client proxy

Required behavior:

- version/method negotiation;
- no-auth method for loopback baseline; authentication only through a designed extension/configuration;
- CONNECT command for I2P destinations;
- domain-name, Base32 and supported destination-address forms;
- correct reply codes and address fields;
- strict rejection of unsupported BIND, UDP ASSOCIATE, clearnet addresses and commands;
- no clearnet DNS fallback.

### Generic TCP tunnels

Client tunnel:

- local listener maps each accepted TCP connection to a configured I2P destination/port;
- bounded concurrent connections and buffers;
- clear failure behavior for lookup/connect/stream reset.

Server tunnel:

- local I2P destination/listener maps inbound streams to a configured local TCP target;
- local target allowlist and loopback default;
- no arbitrary target selection from remote input;
- destination-key storage and rotation policy.

### IRC profiles

IRC client/server adapters may reuse generic tunnels but require explicit privacy filters for commands/fields known to expose local host, IP, username or client metadata. Filter behavior must be documented and tested against fragmented/multi-line input. The adapter must not claim complete IRC anonymization beyond its defined transformations.

## Implementation references

- Java I2P: I2CP message classes/session handlers, `apps/i2ptunnel/java/src`, HTTP proxy, SOCKS and IRC tunnel classes.
- I2P+: corresponding packages; compare parser hardening, header/privacy filters and operational behavior.
- i2pd: I2CP and client/service implementations under `libi2pd_client`, plus tunnel configuration.
- Emissary/go-i2p: current I2CP integration and companion client libraries; verify server-side message coverage and tests.

For service adapters, behavior may be policy rather than I2P wire protocol. Record which transformations are required for interoperability, which prevent privacy leaks and which are optional convenience.

## Required tests

### I2CP

- Initial protocol byte/version/date negotiation, partial frames and oversized lengths.
- Valid and illegal message sequences for each connection/session state.
- Session create/reconfigure/destroy and connection-loss cleanup.
- Session-ID isolation and multi-session behavior if enabled.
- LeaseSet request/create/publication flow.
- Message send/receive/status and lookup/bandwidth messages.
- Interoperability with at least Java I2P/i2pd-compatible I2CP clients, including a real existing client where feasible.
- Slow client, queue saturation, cancellation and malformed-message fuzzing.

### Service adapters

- HTTP absolute/origin forms, malformed headers, privacy-header stripping, `.i2p`/Base32 resolution and no DNS leak.
- SOCKS negotiation, supported/unsupported commands and address types, correct reply codes and no DNS leak.
- Generic client/server tunnel success, local target refusal, backpressure and teardown.
- IRC filter fixtures split across arbitrary TCP boundaries and adversarial line lengths.
- Cross-adapter global resource limits so local clients cannot exhaust destination/stream capacity.
- End-to-end service tests through Java I2P and i2pd destinations.

## Deferred and excluded behavior

- I2CP over non-loopback interfaces without authentication/TLS: excluded.
- Session recovery across disconnect: excluded by current protocol semantics.
- Clearnet outproxying: explicit MVP non-goal.
- SOCKS4, SOCKS BIND and UDP ASSOCIATE: deferred/legacy-reject.
- Arbitrary HTTP CONNECT, transparent proxying and browser-specific helper features: deferred.
- Full I2PTunnel UI/configuration parity: deferred; the MVP is CLI/config driven.
- Every specialized Java tunnel type: deferred unless required by HTTP, SOCKS5, generic TCP or IRC MVP profiles.

## M9 I2CP wire/profile foundation (Plan 164)

Plan 164 lands structural codecs only: no listener, no sessions, no
destination activation, and no interoperability claim.

## M9 I2CP connection/session/options (Plan 165)

Plan 165 lands the runtime-neutral session control semantics. No TCP
listener, no destination private key installation, no LeaseSet2
semantic validation: those belong to Plan 166.

## M9 I2CP client-owned destination + LeaseSet2 bridge (Plan 166)

Plan 166 adapts the existing `i2pr-client` destination runtime to
host an I2CP client-owned destination through one explicit ownership
mode and one inbound-decryption capability surface. The router never
receives the destination signing private key; it only stores the
public `Destination`, the matching X25519 inbound decryption secret
(after atomic validation), and the client-signed Standard LeaseSet2.

Ownership modes are capability-oriented rather than `Option<secret>`:

- `DestinationOwnership::RouterOwned` retains the existing
  `Arc<DestinationIdentity>` allocation so SAM continues to share
  one private identity allocation between the runtime and the SAM
  bridge.
- `DestinationOwnership::ClientOwned` keeps no destination signing
  secret; the runtime exposes `DestinationPublic` and rejects any
  attempt to read the private identity.

`DestinationRuntime::new_client_owned(public, config)` is the only
client-owned constructor. Inbound traffic is rejected until the
Plan 167 daemon drives
`install_client_lease_set2(record, capability, now)` through to a
successful commit. The installer runs the full Plan 166 §6
checklist atomically:

1. structural decode within Plan 164 ceilings;
2. Standard LeaseSet2 type (LS2 type 3);
3. signature verifies against the embedded destination's signing
   public key;
4. destination/session binding matches the runtime's
   `DestinationPublic`;
5. every advertised lease is owned by the destination runtime's
   real inbound pool;
6. no lease is expired, duplicate, foreign, or excessively long
   lived;
7. encryption public-key type is X25519 / type 4;
8. the supplied private decryption capability matches the LS2's
   advertised encryption public key (Plan 166 §6 step 9) — a
   mismatched capability is a hard session error before any state
   mutates;
9. publication and decryption-key installation commit atomically;
10. cancellation at any step restores the prior baseline.

LeaseSet2 refresh does not synthesize a replacement locally. When
leases approach the configured rotation margin the lifecycle emits
`LeaseSetDecision::RequestClientRefresh(cause)`; the daemon pulls
the typed `LeaseRequest` material through
`take_client_refresh_request()` and ships it to the client through
the typed `I2cpAction::RequestVariableLeaseSet` action. The lease
material is sourced from the destination's real inbound pool; no
synthetic gateways are ever produced.

`InboundDecryptionCapability` is the smallest possible
secret-bearing wrapper for the inbound decryption secret: non-`Clone`,
manual `Debug` that redacts the secret bytes, zeroized on drop, no
serialization, no equality over the secret bytes, no raw accessor
exposed through `i2pr-api`, no logging/evidence. The Plan 166
trajectory test `crates/i2pr-client/tests/plan166_trajectory.rs`
covers every Plan 166 §11 case (client-owned construction without a
signing secret, valid install with matching decryption key,
mismatched decryption key, foreign lease, unknown-pool lease after
eviction, duplicate lease, lease rotation requesting refresh,
refresh via install, shutdown releasing every resource, install
while stopping, and router-owned rejection of client install). SAM
router-owned product regressions remain green.

No listener, no socket ownership, no interoperability claim: those
belong to Plans 167–170.

## M9 I2CP loopback server runtime (Plan 167)

Plan 167 composes the runtime-neutral Plan 164/165/166 surface into
a real Tokio-owned loopback I2CP v0.9.67 listener owned by the
production daemon (`crates/i2pr-daemon/src/i2cp.rs`). The service is
the single composition root for the I2CP wire:

```text
i2pr-daemon::i2cp::I2cpServiceState
  -> loopback TcpListener (127.0.0.1, port 7654, ephemeral in tests)
  -> tokio::sync::Semaphore (max_clients admission)
  -> per-connection ChildScope task
       -> i2pr_api::i2cp::FrameDecoder  (Plan 164 incremental framing)
       -> ConnectionStateMachine       (Plan 165 version/state)
       -> SessionRegistry               (Plan 165 reserve/commit/rollback)
       -> DestinationRegistry           (Plan 120)
            -> DestinationRuntime::new_client_owned (Plan 166)
            -> DestinationRuntime::install_client_lease_set2 (Plan 166)
       -> i2pr_api::i2cp::I2cpAction dispatch
```

### Configuration surface

The daemon `[i2cp]` block is the only place the I2CP listener is
controlled. Defaults and validation live in
`crates/i2pr-daemon/src/config.rs`:

```text
[i2cp]
enabled             = false            # disabled by default
bind_address        = "127.0.0.1"      # non-loopback rejected
port                = 7654             # 0 selects ephemeral
max_clients         = 16               # bounded admission semaphore
max_sessions_per_connection = 1
max_sessions_router = 16
max_buffered_bytes_per_connection = 65536
max_pending_writes_per_connection = 64
protocol_byte_timeout_ms = 10000        # 1..=60 seconds
command_timeout_ms       = 60000        # 5..=3600 seconds
shutdown_timeout_ms      = 5000         # 1..=30 seconds
```

Non-loopback bind addresses (`bind_address` outside `127.0.0.0/8` or
`::1`) fail semantic validation before the listener binds. The
router-wide `limits.max_tasks` and `limits.max_buffered_bytes` budgets
remain authoritative: an `[i2cp]` block whose aggregate buffered
bytes exceeds the router budget is rejected.

### Runtime boundary

The api layer (`crates/i2pr-api/src/i2cp/`) owns no Tokio, sockets,
timers, channels, or destination private material. The Plan 167
daemon is the only place where the typed `I2cpAction` vocabulary
projects into runtime state. The action envelope is the single
hand-off point: the per-connection task builds it, the daemon
projects it, and the typed result is the only thing the api layer
ever sees from runtime state.

### Frame and read behavior

The runtime enforces every Plan 164 frame rule:

- `0x2a` protocol byte first; non-magic bytes close the connection.
- `FrameDecoder::push` handles partial header/body reads; the
  per-connection read buffer is bounded near the frame ceiling.
- Multiple complete frames in one read are dispatched in order.
- A second `GetDate`, premature `CreateSession`, and any
  state-machine violation produce a typed rejection.
- The read deadline is `command_timeout_ms` (default 60s, set to
  `Duration::MAX` in the test profile so the paused
  `tokio::time::test-util` harness cannot race a finite timer).
- Connection exit converges on one cleanup path: drop the
  connection admission, cancel the per-connection task, run
  `teardown_connection` which removes every owned destination and
  releases the session registry slot. There is no second source of
  truth for owned resources.

### Action composition

The per-connection task builds one `I2cpAction` per accepted frame:

```text
GetDate             -> SetDate (only router-to-client reply)
CreateSession       -> ReserveClientDestination + verified SessionConfig
                        + projected DestinationConfig
                    -> reply SessionStatus{Created|Invalid|Refused}
CreateLeaseSet2     -> install_client_lease_set2
                        (Plan 166 atomic install_external path)
DestroySession      -> destroy owned destination
                    -> release session registry slot
ReconfigureSession  -> SessionStatus{Refused}  (deferred to Plan 169)
Disconnect          -> close the connection
GetBandwidthLimits  -> BandwidthLimits (neutral zeros in M9)
DestLookup/HostLookup -> typed not-found reply (deferred to Plan 168)
SendMessage/SendMessageExpires -> deferred to Plan 168
```

`CreateSession` and `CreateLeaseSet2` failure paths return a
`SessionStatus{Invalid}` reply instead of closing the connection,
so a client can recover from a single malformed frame and continue
using the same control socket. Wire-level framing errors and
timeout/cancellation close the connection.

### Evidence

The canonical real-TCP evidence lives in
[`crates/i2pr-daemon/tests/i2cp_loopback.rs`](../../crates/i2pr-daemon/tests/i2cp_loopback.rs).
Every Plan 167 §8 case is exercised through raw `tokio::net::TcpStream`
bytes against the real listener on `127.0.0.1:0`:

- disabled config / non-loopback config rejection;
- valid protocol byte / `GetDate` / `SetDate`;
- fragmented protocol byte + frame reads (byte-by-byte and
  multi-frame writes);
- valid signed `CreateSession` activates a client-owned destination;
- tampered signature / stale `creation_ms` produce
  `SessionStatus{Invalid}` without closing the connection;
- `DestroySession` releases the destination and the session slot;
- `Disconnect` tears down every owned resource and closes the
  connection;
- client admission ceiling drops the extra socket after the
  `max_clients` permits are exhausted;
- `GetBandwidthLimits` returns a neutral 64-byte reply;
- `CreateLeaseSet2` with a mismatched decryption key is rejected
  and the destination is cleaned up on connection exit.

No application-message transport, `SendMessage`/`SendMessageExpires`
direction, `MessageStatus` correlation, or independent-client
evidence is claimed in Plan 167. Application-message data plane
lands in Plan 168, self-composed local product in Plan 169,
independent Java/Go client evidence in Plan 170.

### Connection state machine

```text
AwaitProtocolByte
  -> AwaitGetDate
  -> ReadyForSession
  -> SessionPending
  -> Active
  -> Closing
  -> Closed
```

- The M9 profile advertises only `I2CP API version 0.9.67`. Any other
  syntactically-valid version string returns `SessionStatus::Refused`.
- `GetDate` `i2cp.username`/`i2cp.password` and any other auth mapping
  are rejected with `SessionStatus::Refused` (M9 has no
  authentication surface).
- Duplicate `GetDate` and any message family the protocol does not
  permit in the current state are rejected with a typed
  `IllegalInState` error; the state machine never silently
  resynchronizes.
- M9 enforces one primary session per connection; a second
  `CreateSession` after `Active` is rejected.

### SessionConfig verification (before any destination reservation)

Required pre-checks (any failure becomes `SessionStatus::Invalid`):

1. Destination decodes strictly and uses supported signing/encryption
   types (`SigningKeyType::EdDsaSha512Ed25519` and
   `CryptoKeyType::X25519` only).
2. The `KeyCertificate` signing/encryption types agree with the
   embedded keys.
3. The options mapping uses the canonical sorted-key byte
   representation defined in Plan 164.
4. The total SessionConfig body fits inside the 64 KiB I2CP frame
   ceiling; option count, key byte length, and value byte length
   stay within the named ceilings (64 entries, 96 bytes per key,
   96 bytes per value).
5. Creation timestamp is within ±30 seconds of the injected router
   clock (`FixedClock` in tests, `SystemClock` in production).
6. The signature verifies over the exact received signed region
   (Destination || Mapping || creation timestamp bytes retained by
   the structural decoder). The verifier never re-serializes the
   SessionConfig.

The verifier produces a single typed `VerifiedSessionConfig`. No raw
unverified SessionConfig may reach the destination-reservation
action.

### Option disposition table

| Key | Disposition | Notes |
| --- | --- | --- |
| `inbound.length` | applied | mapped to `DestinationConfig::length_hops` |
| `outbound.length` | applied | fallback if `inbound.length` is absent |
| `inbound.quantity` | applied | must be `1..=MAX_DESTINATION_INBOUND` |
| `outbound.quantity` | applied | must be `1..=MAX_DESTINATION_OUTBOUND` |
| `inbound.backupQuantity` | applied (informational) | target quantity drives pool sizing |
| `outbound.backupQuantity` | applied (informational) | target quantity drives pool sizing |
| `inbound.lengthVariance` | applied (informational) | target length drives pool sizing |
| `outbound.lengthVariance` | applied (informational) | target length drives pool sizing |
| `inbound.allowZeroHop` | rejected | no zero-hop support in M9 |
| `outbound.allowZeroHop` | rejected | no zero-hop support in M9 |
| `i2cp.messageReliability` | applied (`BestEffort` only) | `Guaranteed` rejected |
| `i2cp.fastReceive` | applied (`true` only) | `false` rejected (only fast-receive is delivered) |
| `i2cp.leaseSetType` | applied (`3` only) | classic/encrypted/meta/PQ rejected |
| `i2cp.leaseSetEncType` | applied (`4` only) | PQ types 5–7 rejected |
| `i2cp.outbound.tunnel.switch` | ignored | Proposal 171 draft; spec-defined ignore |
| _unknown key_ | recorded | logged as `Unknown` note; never alters policy |

Parsing rules for every numeric option:

- Empty value rejected.
- Leading `+`/`-` rejected (unsigned only).
- ASCII whitespace rejected.
- `u8`/`u16`/`u32` overflow rejected.
- Mixed-case `True`/`TRUE` rejected; literal `true`/`false` only for
  boolean options.

Backup quantity and length variance are recorded as informational
notes; they never silently replace target quantity/length. Router-wide
ceilings (`MAX_DESTINATION_INBOUND`, `MAX_DESTINATION_OUTBOUND`,
`MAX_DESTINATION_BUILD_CONCURRENCY`, `MAX_DESTINATION_FAILURE_THRESHOLD`,
`MAX_PENDING_DESTINATION_MESSAGES`, `MAX_PENDING_DESTINATION_BYTES`,
`MAX_LEASE_PUBLICATION_MARGIN_SECONDS`,
`MAX_LEASE_ROTATION_MARGIN_SECONDS`) are authoritative.

### Session registry

Bounded per-connection and per-router counters; default M9 limits are
`max_per_connection = 1`, `max_per_router = 16`. Sessions use a
reserve → commit → rollback transaction shape. Session IDs are
monotonically assigned by the registry, skip the reserved `0xffff`,
and never reuse a stale ID while a reservation still exists. Duplicate
Destination ownership across active/reserved sessions is rejected.

### Reconfiguration classes

| Class | Behaviour |
| --- | --- |
| `MutableWithRebuild` | `inbound.quantity`, `outbound.quantity`, `inbound.length`, `outbound.length` |
| `MutableImmediate` | `inbound.backupQuantity`, `outbound.backupQuantity`, `inbound.lengthVariance`, `outbound.lengthVariance` |
| `ImmutableAfterCreate` | `i2cp.leaseSetType`, `i2cp.leaseSetEncType` |
| `Unsupported` | `i2cp.messageReliability`, `i2cp.fastReceive`, all unknown keys |

Reconfigure requests are validated all-or-nothing before any
mutation; any `ImmutableAfterCreate` or `Unsupported` classification
rejects the whole request.

### Typed actions

```text
ReserveClientDestination { verified_session, projected_config, connection }
ReconfigureClientDestination { verified_session, projected_config, connection }
DestroyClientDestination { connection, session, destination_hash }
RequestBandwidthSnapshot { connection }
RequestDestinationLookup { connection, key }
```

Action payloads contain only verified/typed values; raw client
strings never appear in an `I2cpAction`. `i2pr-api` owns no sockets,
timers, or Tokio tasks; the Plan 167 daemon is the sole translator
from these actions to runtime state.

### M9 compatibility profile (Plan 164)

i2pr does not claim blanket API 0.9.67 compliance. The M9 profile
targets the modern Standard LeaseSet2 + Ed25519/X25519 path:

```text
implemented-m9
  GetDate / SetDate (32/33)
  CreateSession / ReconfigureSession / DestroySession (1/2/3)
  SessionStatus (20; codes 0-4)
  RequestVariableLeaseSet (37; at most 16 leases)
  CreateLeaseSet2 (41; Standard LeaseSet2 only)
  SendMessage / SendMessageExpires (5/36)
  MessagePayload / MessageStatus (31/22; codes 0-23, higher reserved)
  GetBandwidthLimits / BandwidthLimits (8/23; sixteen integers, 64 bytes)
  DestLookup / DestReply (34/35; empty/hash/destination shapes)
  HostLookup / HostReply (38/39; structural; client probing at Plan 170)
  Disconnect (30)

planned-later
  multi-session subsession semantics (Plan 165 decided: unsupported)

explicitly-unsupported
  BlindingInfo (42); EncryptedLeaseSet/MetaLeaseSet publication;
  PQ encryption types 5-7; offline-signed sections; zero-hop tunnels;
  guaranteed reliability; non-fast receive

spec-defined-ignore
  Proposal 171 outbound-tunnel-switching flag (draft; ignored
    per Plan 165 disposition table);
  SendMessageExpires reliability-override bits 10-9 (unimplemented
    per specification)

legacy-deprecated
  CreateLeaseSet (4); ReceiveMessageBegin/End (6/7);
  RequestLeaseSet (21); ReportAbuse (29);
  abandoned preliminary CreateLeaseSet2 (40, treated as unknown)
```

- Connection preamble: single protocol byte `0x2a`.
- Common frame: big-endian `uint32` body length, `uint8` type, body.
  The official "about 64 KB" limit is enforced as exactly 64 KiB,
  rejected before allocation on decode and on encode.
- Session ID: two bytes; `0xffff` means "no session".
- Message ID and client nonce: four bytes each; nonce zero
  suppresses status replies.
- Strings: one-byte length prefix, at most 255 UTF-8 bytes.
- Mapping: two-byte body length with canonical sorted-key order;
  SessionConfig options must arrive sorted, while GetDate
  authentication and HostReply option mappings are accepted unsorted
  and normalized to canonical form.
- SessionConfig: Destination, sorted Mapping, eight-byte creation
  Date, signature sized by the destination signing type; the exact
  received signed region is retained for Plan 165/166.
- Payload: four-byte length prefix (64 KiB ceiling); gzip content
  uses the ten-byte header with source/destination ports in MTIME,
  XFL 2, and the protocol number in OS (6 Streaming, 17 datagram,
  18 raw datagram, 224–254 experimental, 255 reserved). No
  decompression exists yet; the 64 KiB expansion ceiling is recorded
  for Plan 168.
- `SendMessageExpires`: nonce, two-byte flags (bits 15–11 must be
  zero), six-byte expiration instant (48-bit milliseconds).
- `CreateLeaseSet2` private keys: one per LeaseSet2 encryption key
  in order, at most eight keys and 8 KiB aggregate; secret-bearing
  values are non-`Clone`, redacted, and zeroized.

### Evidence

- `crates/i2pr-api/src/i2cp/` (`mod`, `frame`, `message`, `ids`,
  `payload`, `mapping`, `error`); `#![forbid(unsafe_code)]`, no
  Tokio, sockets, timers, or async ownership.
- `tests/fixtures/i2cp/` (22 positive, 7 malformed) with
  `manifest.tsv`, enforced by `scripts/check-i2cp-vectors.sh` in
  routine Linux CI; `crates/i2pr-api/tests/i2cp_vectors.rs` pins
  field expectations and typed rejections.
- Plan 165 extends `crates/i2pr-api/src/i2cp/` with `connection`,
  `verify`, `config`, `session`, and `actions` modules; unit tests in
  each module cover the plan §9 cases (legal/illegal transitions,
  SessionConfig signature/date/option verification, registry
  reserve/commit/rollback, duplicate-destination rejection, exact
  SessionStatus mapping, reconfiguration all-or-nothing).
- Plan 166 extends `crates/i2pr-client/src/identity.rs`,
  `leaseset.rs`, `registry.rs` with the explicit
  `DestinationOwnership`, `DestinationPublic`, and non-`Clone`
  redacted zeroized `InboundDecryptionCapability` surface; the new
  `install_client_lease_set2` transaction validates Standard
  LeaseSet2 signature, lease ownership, expiry, encryption-key
  type, and decryption-key match atomically; the typed
  `LeaseRequest` material and the `I2cpAction::RequestVariableLeaseSet`
  action carry lease refresh requests from real inbound tunnels
  through the `i2pr-api::i2cp` boundary. The 12-test
  `crates/i2pr-client/tests/plan166_trajectory.rs` covers every
  Plan 166 §11 case.
- Later passes own behavior: Plan 167 (loopback listener), Plan 168
  (data plane), Plan 169 (product + hardening), Plan 170
  (independent clients + closure).

## Open decisions

1. Exact I2CP version and message subset advertised by the first server.
2. Whether multi-session I2CP is required at initial Milestone 9 exit.
3. Internal destination-message API shared by I2CP, SAM datagrams and future applications.
4. HTTP CONNECT policy for I2P destinations and header-filter baseline.
5. Address syntax used by SOCKS for raw destinations versus names/Base32.
6. IRC filter rules and their explicit privacy claims.
7. Configuration schema and key ownership for persistent server tunnels.
8. Whether service adapters share a destination by default or create isolated destinations/tunnel pools per service.