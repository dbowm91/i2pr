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

### Compatibility profile

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
  multi-session subsession semantics (Plan 165 decides acceptance)

explicitly-unsupported
  BlindingInfo (42); EncryptedLeaseSet/MetaLeaseSet publication;
  PQ encryption types 5-7; offline-signed sections

spec-defined-ignore
  Proposal 171 outbound-tunnel-switching flag (draft; exact flag key
    verified at Plan 165); SendMessageExpires reliability-override
    bits 10-9 (unimplemented per specification)

legacy-deprecated
  CreateLeaseSet (4); ReceiveMessageBegin/End (6/7);
  RequestLeaseSet (21); ReportAbuse (29);
  abandoned preliminary CreateLeaseSet2 (40, treated as unknown)
```

### Wire rules recorded

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
- Later passes own behavior: Plan 165 (connection/session/options),
  Plan 166 (client-owned destinations + LeaseSet2), Plan 167
  (loopback listener), Plan 168 (data plane), Plan 169 (product +
  hardening), Plan 170 (independent clients + closure).

## Open decisions

1. Exact I2CP version and message subset advertised by the first server.
2. Whether multi-session I2CP is required at initial Milestone 9 exit.
3. Internal destination-message API shared by I2CP, SAM datagrams and future applications.
4. HTTP CONNECT policy for I2P destinations and header-filter baseline.
5. Address syntax used by SOCKS for raw destinations versus names/Base32.
6. IRC filter rules and their explicit privacy claims.
7. Configuration schema and key ownership for persistent server tunnels.
8. Whether service adapters share a destination by default or create isolated destinations/tunnel pools per service.