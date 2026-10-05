# i2pr-api

**Crate:** `i2pr-api` — **Path:** `crates/i2pr-api` — **Lines:** 15 858
across 33 Rust files: 15 494 in `src/` (31 files) plus 364 in `tests/`
(2 files).

Runtime-neutral application-protocol adapter layer for the `i2pr` router:
strict bounded SAM 3.1 and I2CP 0.9.67 (M9 profile) wire codecs, typed
command/reply models, connection and session state machines, and bounded
per-session resource accounting — composed into real listeners by
`i2pr-daemon`.

## Purpose

`i2pr-api` owns two application protocol surfaces, both **runtime-neutral**:

- **SAM 3.1** (`src/sam/`) — bounded line/command/reply parsing, exact
  version negotiation, the I2P Base64 codec, the `SamPrivateDestination`
  private-destination container, the bounded session/stream registries,
  the line reader, the server connection state machine, and the
  loopback-only FORWARD / NAMING policy.
- **I2CP** (`src/i2cp/`) — the `0x2a` preamble and common frame codec,
  the incremental `FrameDecoder`, structural message codecs for the
  implemented M9 profile, the `ConnectionStateMachine`, canonical
  `SessionConfig` verification, the option disposition table, the
  bounded session registry, and the typed `I2cpAction` / data-plane
  action vocabularies.

What the crate must **not** own:

- **No sockets, listeners, or accept loops.** The daemon owns every
  SAM and I2CP listener; see [i2pr-daemon.md](i2pr-daemon.md).
- **No Tokio runtime, no timers, no channels, no task spawning.**
  `#![forbid(unsafe_code)]` is set at the crate root
  (`crates/i2pr-api/src/lib.rs:51`); no `tokio` dependency appears in
  `crates/i2pr-api/Cargo.toml`, and `scripts/check-runtime-boundaries.sh`
  enforces the manifest-level ban.
- **No destination private material.** The api layer never sees a
  `DestinationIdentity` or a private key; inbound-decryption capability
  stays inside `i2pr-client`.
- **No clocks it did not receive.** `verify_session_config` takes an
  injected `Clock`; the crate never reads a wall clock on its own
  behalf.
- **No interoperability claim.** Both surfaces are **loopback-only,
  disabled by default, and non-advertised** (`DEFAULT_SAM_ENABLED =
  false`, `crates/i2pr-api/src/sam/limits.rs:44`; `[i2cp]` defaults to
  `enabled = false` in the daemon). Nothing is advertised beyond the
  tested subset per `specs/CONFORMANCE.md`.

## Module layout

### `src/i2cp/` — 7 935 lines across 13 files

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- | --- |
| `i2cp` | `src/i2cp/mod.rs` | 150 | Module facade, `pub use` re-exports, the documented M9 compatibility profile | (facade only) |
| `i2cp::frame` | `src/i2cp/frame.rs` | 333 | `0x2a` preamble check, common frame (`u32` length + `u8` type + body), capped incremental decoder | `PROTOCOL_BYTE`, `MAX_I2CP_BODY_BYTES`, `FRAME_HEADER_LEN`, `FrameHeader`, `RawFrame`, `FrameDecoder`, `check_protocol_byte`, `decode_header`, `decode_frame`, `encode_frame` |
| `i2cp::message` | `src/i2cp/message.rs` | 2 907 | Type IDs, dispositions, structural body codecs for the implemented profile | `MessageType` (25 assigned), `MessageDisposition`, `Message`, `GetDate`/`SetDate`, `CreateSession`, `ReconfigureSession`, `DestroySession`, `SessionStatus`, `RequestVariableLeaseSet`, `CreateLeaseSet2`, `SendMessage`, `SendMessageExpires`, `MessagePayload`, `MessageStatus`, `GetBandwidthLimits`, `BandwidthLimits`, `DestLookup`, `DestReply`, `HostLookup`, `HostReply`, `Disconnect`, `decode_typed` |
| `i2cp::ids` | `src/i2cp/ids.rs` | 125 | Typed 32-bit identifier newtypes | `SessionId`, `MessageId`, `ClientNonce`, `HostRequestId` |
| `i2cp::payload` | `src/i2cp/payload.rs` | 399 | Payload wrapper, gzip metadata contract, protocol numbers | `Payload`, `PayloadGzipHeader`, `split_payload`, `GZIP_*`, `PROTOCOL_*`, `MAX_I2CP_PAYLOAD_BYTES` |
| `i2cp::mapping` | `src/i2cp/mapping.rs` | 242 | Strict/lenient Mapping postures, signing bytes | `encode_mapping`, `split_mapping_strict`, `split_mapping_lenient` |
| `i2cp::error` | `src/i2cp/error.rs` | 169 | `I2cpError` typed classification | `I2cpError` |
| `i2cp::connection` | `src/i2cp/connection.rs` | 610 | `ConnectionStateMachine` + `0.x.y` version handshake | `ConnectionState`, `ConnectionStateMachine`, `M9_ADVERTISED_VERSION`, `MAX_VERSION_STRING_BYTES` |
| `i2cp::verify` | `src/i2cp/verify.rs` | 550 | Canonical `SessionConfig` signature/date/ceiling verification with injected clock | `verify_session_config`, `VerifiedSessionConfig`, `Clock`, `SystemClock`, `FixedClock`, `VERIFICATION_SKEW_MS` |
| `i2cp::config` | `src/i2cp/config.rs` | 1 128 | Option disposition table, `DestinationConfig` projection, reconfiguration taxonomy | `project_options`, `ProjectedPolicy`, `OptionDisposition`, `OptionNote`, `SessionConfigLimits`, `ReconfigurationClass`, `reconfiguration_class`, `classify_reconfigure_diff`, `validate_reconfigure_classifications`, `validate_mapping_shape`, `default_registry_config`, `default_tunnel_lifetime` |
| `i2cp::session` | `src/i2cp/session.rs` | 477 | Bounded session registry (reserve / commit / rollback) | `SessionRegistry`, `SessionRegistryLimits`, `SessionEntry`, `SessionReservation`, `MAX_REGISTRY_SESSION_ID` |
| `i2cp::actions` | `src/i2cp/actions.rs` | 159 | Typed `I2cpAction` vocabulary the daemon projects into runtime state | `I2cpAction`, `LeaseRefreshCause`, `LeaseRequestLease`, `DestinationLookupKey` |
| `i2cp::data_plane` | `src/i2cp/data_plane.rs` | 686 | Bounded message data plane: outcome vocabulary, action vocabulary, correlation table, inbound queue, per-session ceilings | `I2cpMessageOutcome`, `I2cpDataPlaneAction`, `PendingStatusTable`, `PendingStatusEntry`, `InboundPayloadQueue`, `InboundPayloadFrame`, `DataPlaneError` |

### `src/sam/` — 7 441 lines across 17 files

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- | --- |
| `sam` | `src/sam/mod.rs` | 50 | Module facade + named byte ceilings for one SAM line | `MAX_SAM_LINE_BYTES`, `MAX_SAM_TOKENS`, `MAX_SAM_OPTIONS`, `MAX_SAM_OPTION_VALUE_BYTES`, `MAX_SAM_SESSION_ID_BYTES`, `MAX_SAM_NAME_BYTES`, `MAX_SAM_PRIV_TEXT_BYTES`, `MAX_SAM_PUB_TEXT_BYTES` |
| `sam::version` | `src/sam/version.rs` | 268 | `SamVersion`, parsing, negotiation, advertisement | `SamVersion`, `parse_version`, `negotiate`, `NegotiatedVersion`, `SamVersionParseError`, `MIN_SUPPORTED_VERSION`, `MAX_SUPPORTED_VERSION` |
| `sam::base64` | `src/sam/base64.rs` | 369 | I2P Base64 codec (strict, `-`/`~` alphabet, `=` padding) | `SamBase64Error`, encode/decode |
| `sam::command` | `src/sam/command.rs` | 530 | `Command`, `CommandKind`, options, outcomes, STREAM request parsers | `Command`, `CommandKind`, `CommandOutcome`, `MissingOption`, `UnknownCommand`, `UnknownOption`, `SessionStyle`, `UnsupportedStyle`, `Silently`, `StreamConnectRequest`/`Error`, `parse_stream_connect`, `StreamAcceptRequest`/`Error`, `StreamAcceptId`, `parse_stream_accept` |
| `sam::parser` | `src/sam/parser.rs` | 630 | `parse_line`, tokenisation, per-command-family recognition | `parse_line`, `ParseError` |
| `sam::reply` | `src/sam/reply.rs` | 635 | Canonical reply model and single encoder | `Reply`, `ReplyLine`, `HelloReply`, `DestReply`, `SessionStatus`, `StreamStatus`, `NamingReply`, `PongReply` |
| `sam::private_destination` | `src/sam/private_destination.rs` | 355 | `SamPrivateDestination` container, `from_identity`/`from_base64`/`from_bytes`, `into_identity` | `SamPrivateDestination`, `SamPrivateDestinationError`, `PUB_LENGTH`, `PRIV_LENGTH` |
| `sam::dest_generate` | `src/sam/dest_generate.rs` | 219 | Runtime-neutral `dest_generate` core operation over an injected CSPRNG | `DestGenerateRequest`, `DestGenerateSignatureType`, `DestGenerateOutcome`, `DestGenerate`, `DestGenerateError`, `dest_generate`, `DEST_GENERATE_SIGNATURE_TYPE_ED25519` |
| `sam::session_create` | `src/sam/session_create.rs` | 209 | Typed `SESSION CREATE` request parser | `parse_session_create`, `SessionCreateRequest`, `SessionCreateStyle`, `SessionCreateError` |
| `sam::limits` | `src/sam/limits.rs` | 402 | Bounded capacity/timeout ceilings and the loopback test profile | `SamLimits`, `SamLimitsError`, `MAX_SAM_*`, `DEFAULT_SAM_*`, `loopback_test_profile` |
| `sam::session` | `src/sam/session.rs` | 188 | Session identifier and per-session counters | `SamSessionId`, `SamSessionCounters`, `SamSessionCountersError` |
| `sam::registry` | `src/sam/registry.rs` | 710 | Bounded session registry with reserve/commit/rollback; control-connection ownership | `SamSessionRegistry`, `SamSessionEntry`, `SamSessionReservation`, `SamSessionRegistryError`, `ControlOwnerState` |
| `sam::line_reader` | `src/sam/line_reader.rs` | 433 | Bounded line framing with control-byte and oversize rejection | `LineReader`, `LineEvent`, `LineReaderError` |
| `sam::server_state` | `src/sam/server_state.rs` | 1 186 | Per-connection state machine, dispatch, stream/naming appliers | `ServerConnectionState`, `DispatchOutcome`, `CloseReason`, `dispatch`, `apply_session_outcome`, `apply_stream_connect_outcome`, `apply_stream_accept_outcome`, `apply_stream_forward_outcome`, `apply_naming_lookup_outcome`, `SessionCreateApplied`/`Failed`, `StreamConnectApplied`/`Failed`, `StreamAcceptApplied`/`Failed`, `StreamForwardApplied`/`Failed`, `NamingLookupApplied`/`Failed` |
| `sam::streams` | `src/sam/streams.rs` | 911 | Per-session STREAM socket registry, pending-accept queue, atomic `InboundMode` | `SamStreamRegistry`, `SamStreamRegistryHandle`, `SamStreamRegistryError`, `SamStreamAttachment`, `SamOutboundAttachment`, `SamAcceptWaiter`, `SamStreamEntry`, `SamStreamState`, `SamStreamDirection`, `InboundMode` |
| `sam::forward` | `src/sam/forward.rs` | 190 | Loopback-only `STREAM FORWARD` request/host policy | `parse_stream_forward`, `StreamForwardRequest`, `ForwardHost`, `normalize_forward_host`, `StreamForwardError` |
| `sam::naming` | `src/sam/naming.rs` | 156 | Local `NAME=ME` / public-Destination / `.b32` validation (bridge to `i2pr-addressbook`) | `parse_naming_lookup`, `NamingLookupRequest`, `resolve_public_destination`, `decode_b32_destination_hash`, `NamingLookupError` |

## Public surface

`crates/i2pr-api/src/lib.rs` declares exactly two top-level modules —
`pub mod i2cp;` and `pub mod sam;` — and re-exports the SAM surface
verbatim (the I2CP surface is re-exported from `src/i2cp/mod.rs`):

```rust
pub use sam::{
    command::{
        Command, CommandKind, CommandOutcome, MissingOption, SessionStyle, Silently,
        StreamAcceptError, StreamAcceptId, StreamAcceptRequest, StreamConnectError,
        StreamConnectRequest, UnknownCommand, UnknownOption, UnsupportedStyle, parse_stream_accept,
        parse_stream_connect,
    },
    dest_generate::{
        DEST_GENERATE_SIGNATURE_TYPE_ED25519, DestGenerate, DestGenerateError, DestGenerateOutcome,
        DestGenerateRequest, DestGenerateSignatureType, dest_generate,
    },
    forward::{
        ForwardHost, StreamForwardError, StreamForwardRequest, normalize_forward_host,
        parse_stream_forward,
    },
    limits::{
        DEFAULT_SAM_BIND_ADDRESS, DEFAULT_SAM_COMMAND_TIMEOUT_MS, DEFAULT_SAM_ENABLED,
        DEFAULT_SAM_HELLO_TIMEOUT_MS, DEFAULT_SAM_MAX_CLIENTS, DEFAULT_SAM_MAX_SESSIONS,
        DEFAULT_SAM_PENDING_ACCEPTS_PER_SESSION, DEFAULT_SAM_PORT, DEFAULT_SAM_SHUTDOWN_TIMEOUT_MS,
        DEFAULT_SAM_STREAM_SOCKETS_PER_SESSION, MAX_SAM_BUFFERED_BYTES_PER_STREAM_DIRECTION,
        MAX_SAM_CLIENTS, MAX_SAM_COMMAND_TIMEOUT_SECS, MAX_SAM_HELLO_TIMEOUT_SECS,
        MAX_SAM_PENDING_ACCEPTS_PER_SESSION, MAX_SAM_SESSIONS, MAX_SAM_SHUTDOWN_TIMEOUT_SECS,
        MAX_SAM_STREAM_SOCKETS_PER_SESSION, SamLimits, SamLimitsError,
    },
    line_reader::{LineEvent, LineReader, LineReaderError},
    naming::{
        NamingLookupError, NamingLookupRequest, decode_b32_destination_hash, parse_naming_lookup,
        resolve_public_destination,
    },
    parser::{ParseError, parse_line},
    private_destination::{
        PRIV_LENGTH, PUB_LENGTH, SamPrivateDestination, SamPrivateDestinationError,
    },
    registry::{
        ControlOwnerState, SamSessionEntry, SamSessionRegistry, SamSessionRegistryError,
        SamSessionReservation,
    },
    reply::{
        DestReply, HelloReply, NamingReply, PongReply, Reply, ReplyLine, SessionStatus,
        StreamStatus,
    },
    server_state::{
        CloseReason, DispatchOutcome, NamingLookupApplied, NamingLookupFailed,
        ServerConnectionState, SessionCreateApplied, SessionCreateFailed, StreamAcceptApplied,
        StreamAcceptFailed, StreamConnectApplied, StreamConnectFailed, StreamForwardApplied,
        StreamForwardFailed, apply_naming_lookup_outcome, apply_session_outcome,
        apply_stream_accept_outcome, apply_stream_connect_outcome, apply_stream_forward_outcome,
        dispatch,
    },
    session::{SamSessionCounters, SamSessionCountersError, SamSessionId},
    session_create::{
        SessionCreateError, SessionCreateRequest, SessionCreateStyle, parse_session_create,
    },
    streams::{
        InboundMode, SamAcceptWaiter, SamOutboundAttachment, SamStreamAttachment,
        SamStreamDirection, SamStreamEntry, SamStreamRegistry, SamStreamRegistryError,
        SamStreamRegistryHandle, SamStreamState,
    },
    version::{
        MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION, NegotiatedVersion, SamVersion,
        SamVersionParseError, negotiate, parse_version,
    },
};
```

`i2pr_api::i2cp` re-exports, in addition to every submodule's public
items: `I2cpAction`, `LeaseRefreshCause`, `LeaseRequestLease`,
`DestinationLookupKey`, `I2cpError`, `ConnectionState`,
`ConnectionStateMachine`, `I2cpMessageOutcome`, `I2cpDataPlaneAction`,
`PendingStatusTable`/`PendingStatusEntry`,
`InboundPayloadQueue`/`InboundPayloadFrame`, `DataPlaneError`,
`SessionRegistry`/`SessionEntry`/`SessionReservation`,
`VerifiedSessionConfig`, `Clock`/`SystemClock`/`FixedClock`, and every
`MAX_*` / `M9_*` constant listed below.

## Key contracts

### I2CP framing

- `PROTOCOL_BYTE = 0x2a` (`frame.rs:21`), checked before any body work.
- `MAX_I2CP_BODY_BYTES = 64 * 1024` (`frame.rs:27`), enforced **before**
  allocation.
- `FRAME_HEADER_LEN = 5` (`frame.rs:30`) — `u32` length + `u8` type.
- `FrameDecoder` is incremental and capped, so a partial read never
  desynchronizes framing.

### I2CP message classification

`MessageType` carries exactly **25 assigned types**
(`message.rs:136`). `MessageDisposition` is `ImplementedM9`,
`LegacyDeprecated` (4/6/7/21/29), or `ExplicitlyUnsupported`
(`BlindingInfo` 42); the abandoned preliminary `CreateLeaseSet2`
(type 40) is treated as unknown. Deprecated, unsupported, and unknown
types are rejected at the classification layer without body parsing.

`HostLookup` / `HostReply` are **structural only**. No resolution
exists in this crate: `DestinationLookupKey::Hostname` is a
client-supplied value whose "resolution policy lives in" the daemon
(`actions.rs:128`). There is no `resolve_host_lookup` and no
`Hostname` resolution routine anywhere under `crates/i2pr-api/src/`.

### Connection state machine

`ConnectionState` (`connection.rs:41`) runs
`AwaitProtocolByte → AwaitGetDate → ReadyForSession → SessionPending →
Active → Closing → Closed`. `M9_ADVERTISED_VERSION = "0.9.67"`
(`connection.rs:37`); `MAX_VERSION_STRING_BYTES = 32`
(`connection.rs:34`). The Plan 170 posture accepts any well-formed
`0.x.y` client string and rejects a non-zero major with
`I2cpError::VersionNegotiation`. Illegal message families are rejected
with `I2cpError::IllegalInState` or `I2cpError::HandshakeOrdering`; the
machine never silently resynchronizes. Empty-auth `GetDate`
(I2CP 0.9.11+) is accepted; non-empty credentials are rejected with
`I2cpError::AuthNotSupported`.

### `SessionConfig` verification with an injected clock

`verify_session_config` consumes a parsed `SessionConfig` plus the raw
body bytes and verifies, in order: canonical bytes; supported
signing/encryption types; certificate agreement; mapping shape; the
creation timestamp; and the signature over the retained signed region.

- `SESSION_CONFIG_MAX_SKEW_MS = 30_000` (`message.rs:74`),
  re-exported as `VERIFICATION_SKEW_MS` (`verify.rs:42`).
- The time source is the **injected** `Clock` trait
  (`verify.rs:48`) with `fn now_ms(&self) -> u64`. The only production
  implementation is `SystemClock` (`verify.rs:55`), which the **daemon**
  constructs and passes in; `FixedClock` (`verify.rs:68`) is the
  deterministic test implementation. The crate has no ambient clock
  read — verification is `pub fn verify_session_config(..., clock: &dyn
  Clock, ...)`, so a caller fully controls the reference instant.
- `I2cpError::CreationTimestampOutOfRange` is raised when
  `creation_ms` falls outside `[now - skew, now + skew]`
  (`verify.rs:182-183`, using `saturating_sub`/`saturating_add`).
- The only success output is `VerifiedSessionConfig`, the typed value
  the Plan 166 destination reservation may see.

### Option disposition table and router-wide ceilings

`project_options` (`config.rs:267`) parses every numeric option
strictly (unsigned, no whitespace, no overflow), rejects unknown
signing/encryption types, zero-hop tunnels, guaranteed reliability, and
non-fast-receive, records the Proposal 171 outbound-tunnel-switching
flag as ignored, and classifies unknown keys.

- `OptionDisposition` is `Applied`, `Rejected`, `Ignored`, `Unknown`
  (`config.rs:49`), each recorded as an `OptionNote` whose `note` is
  redacted and never carries the rejected value bytes.
- `MAX_SESSION_CONFIG_OPTIONS = 64`, `MAX_SESSION_CONFIG_KEY_BYTES = 96`,
  `MAX_SESSION_CONFIG_VALUE_BYTES = 96` (`config.rs:27-33`).
- `M9_LEASE_SET_TYPE = 3`, `M9_LEASE_SET_ENC_TYPE = 4`,
  `M9_MESSAGE_RELIABILITY_BEST_EFFORT = "BestEffort"`,
  `M9_FAST_RECEIVE_DEFAULT = true` (`config.rs:36-45`).
- **Router-wide ceilings are authoritative.** The projected
  `DestinationConfig` is built through the existing
  `DestinationConfig::try_new` constructor (`config.rs:532`, `:612`), so
  a client-supplied value that exceeds a router ceiling is rejected
  rather than clamped (`config.rs:687`:
  `"destination policy rejected by router-wide ceiling"`). The
  `ceilings` submodule (`config.rs:792`) re-exposes the plan-level
  ceilings for tests without duplicating magic numbers.
- The ElGamal legacy-slot policy relocated under Plan 170: the
  Destination encryption-key slot is accepted at verification time, and
  X25519 enforcement lives at the Plan 166
  `install_client_lease_set2` capability↔LS2 match, which stays
  fail-closed with `DecryptionCapabilityKeyMismatch`.

### Bounded session registry

`SessionRegistry` (`session.rs:80`) exposes
`reserve → commit → rollback`, plus `destroy`,
`destroy_for_connection`, `destination_in_use`, `session_active`,
`get`, and `session_for_destination`.

- `MAX_REGISTRY_SESSION_ID = 0xfffe` (`session.rs:27`); the `0xffff`
  sentinel is skipped, and exhaustion returns
  `I2cpError::SessionIdExhausted` rather than wrapping.
- `reserve` / `commit` are fallible (`I2cpError::SessionRegistry`),
  `rollback` is infallible; the two-step shape is what makes the
  session-plus-destination insert transactional.
- Duplicate-destination ownership is prevented across **active and
  reserved** sessions via `destination_in_use` / the registry's
  destination index.
- Per-connection and per-router ceilings come from
  `SessionRegistryLimits`.

### Reconfiguration taxonomy

`ReconfigurationClass` (`config.rs:697`) is `MutableWithRebuild`,
`MutableImmediate`, `ImmutableAfterCreate`, or `Unsupported`.
`reconfiguration_class` maps a known option key to its class;
`classify_reconfigure_diff` classifies a key-by-key diff, and
`validate_reconfigure_classifications` enforces the canonical
**all-or-nothing** application rule. Plan 169 reuses these helpers
unmodified; the runtime-neutral `I2cpSessionState::last_options`
(`Mapping`) carries the previous verified mapping so the diff is
computed against a stable baseline.

### Typed action vocabulary

`I2cpAction` (`actions.rs`) has exactly six variants:
`ReserveClientDestination`, `ReconfigureClientDestination`,
`DestroyClientDestination`, `RequestBandwidthSnapshot`,
`RequestDestinationLookup`, and (Plan 166)
`RequestVariableLeaseSet`. All carry verified typed values only — never
raw client bytes, never a private key. `LeaseRefreshCause` is
`InitialGeneration` or `ApproachingExpiry`; `LeaseRequestLease` carries
the deterministic ordered `(gateway, tunnel_id, end_date)` triple, which
reference clients require to be 44 bytes (gateway + tunnel id + 8-byte
ms end date) so exact-pinned Java `Lease.readBytes` and go-i2cp
`NewLeaseFromStream` parse unmodified. The lease material is produced by
`i2pr_client::DestinationRuntime::take_client_refresh_request`; the api
layer never holds the inbound-decryption capability.

### I2CP message data plane

- `I2cpMessageOutcome` (`data_plane.rs:106`) is `#[non_exhaustive]`:
  `Accepted`, `BadLocalLeaseSet`, `NoLocalTunnels`, `Overflow`,
  `DestinationStopping`, `BadSession`, `BadMessage`, `MessageExpired`,
  `BadExpirationHorizon`, `UnsupportedFlags`, `SessionError`.
  `status_code()` is a one-to-one `const fn` map onto
  `MessageStatusCode`; `Accepted` is the only success class.
- `I2cpDataPlaneAction` (`data_plane.rs:184`) is `#[non_exhaustive]`:
  `EnqueueOutboundPayload`, `DeliverInboundPayload`,
  `ResolveDestinationLookup`.
- `PendingStatusTable` / `PendingStatusEntry` — bounded correlation
  bookkeeping; `record` returns `DataPlaneError::CapacityExceeded` at
  the ceiling, `take` is duplicate/late-idempotent, and `release_all`
  drains on teardown.
- `InboundPayloadQueue` / `InboundPayloadFrame` — bounded inbound frame
  buffer with separate frame and byte ceilings.
  `InboundPayloadFrame::WIRE_OVERHEAD_BYTES = 10 + 4 = 14`
  (`data_plane.rs:495`) — gzip header plus the I2CP payload length
  prefix — and `wire_size()` is the only sizing helper.
- `DataPlaneError::CapacityExceeded` is the single bounded-failure type
  shared by both containers.

**Per-session / per-connection ceilings** (`data_plane.rs:53-78`):

| Constant | Value |
| --- | --- |
| `MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION` | `64` |
| `MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION` | `128` |
| `MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION` | `64` |
| `MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION` | `64 * 1024` (64 KiB) |
| `MAX_CONCURRENT_DESTINATION_LOOKUPS_PER_CONNECTION` | `16` |
| `MAX_DESTINATION_LOOKUP_HORIZON` | `Duration::from_secs(10)` |
| `MAX_MESSAGE_EXPIRATION_HORIZON` | `Duration::from_secs(60 * 60)` (1 h) |

Accessors `max_pending_messages_per_session`,
`max_pending_status_correlations`, `max_inbound_payload_bytes_per_session`,
and `max_destination_lookup_horizon` are all `const fn`, so tests reserve
exactly a frame-sized slot below the byte ceiling without duplicating
the magic numbers.

### I2CP payload format

`GZIP_HEADER_LEN = 10`, `GZIP_TRAILER_LEN = 8`, `GZIP_MAGIC = [0x1f,
0x8b]`, `GZIP_METHOD_DEFLATE = 8`, `GZIP_XFLAGS_JAVA = 2`,
`MAX_I2CP_PAYLOAD_BYTES = MAX_I2CP_DECOMPRESSED_BYTES = 64 * 1024`
(`payload.rs:22-61`). The structural codec rejects malformed gzip
metadata before routing; there is no decompression in this crate.

### SAM line ceilings

From `src/sam/mod.rs:24-50`: `MAX_SAM_LINE_BYTES = 8192`,
`MAX_SAM_TOKENS = 64`, `MAX_SAM_OPTIONS = 32`,
`MAX_SAM_OPTION_VALUE_BYTES = 4096`, `MAX_SAM_SESSION_ID_BYTES = 256`,
`MAX_SAM_NAME_BYTES = 256`, `MAX_SAM_PRIV_TEXT_BYTES = 1024`,
`MAX_SAM_PUB_TEXT_BYTES = 1024`.

### SAM capacity and timeout ceilings

From `src/sam/limits.rs:16-61` — hard maxima and shipped defaults:

| Hard ceiling | Value | Default |
| --- | --- | --- |
| `MAX_SAM_CLIENTS` | `1024` | `DEFAULT_SAM_MAX_CLIENTS = 16` |
| `MAX_SAM_SESSIONS` | `1024` | `DEFAULT_SAM_MAX_SESSIONS = 16` |
| `MAX_SAM_STREAM_SOCKETS_PER_SESSION` | `1024` | `DEFAULT_SAM_STREAM_SOCKETS_PER_SESSION = 16` |
| `MAX_SAM_PENDING_ACCEPTS_PER_SESSION` | `1024` | `DEFAULT_SAM_PENDING_ACCEPTS_PER_SESSION = 16` |
| `MAX_SAM_BUFFERED_BYTES_PER_STREAM_DIRECTION` | `16 * 1024 * 1024` | `DEFAULT_SAM_BUFFERED_BYTES_PER_STREAM_DIRECTION = 64 * 1024` |
| `MAX_SAM_HELLO_TIMEOUT_SECS` | `60` | `DEFAULT_SAM_HELLO_TIMEOUT_MS = 10_000` |
| `MAX_SAM_COMMAND_TIMEOUT_SECS` | `3_600` | `DEFAULT_SAM_COMMAND_TIMEOUT_MS = 60_000` |
| `MAX_SAM_SHUTDOWN_TIMEOUT_SECS` | `60` | `DEFAULT_SAM_SHUTDOWN_TIMEOUT_MS = 5_000` |
| — | — | `DEFAULT_SAM_BIND_ADDRESS = "127.0.0.1"`, `DEFAULT_SAM_PORT = 7656`, `DEFAULT_SAM_ENABLED = false` |

`SamLimits::loopback_test_profile` is the ephemeral loopback profile the
integration suites bind; the daemon never enables SAM by default.

### SAM I2P Base64 codec

The SAM alphabet is `A-Z a-z 0-9 - ~` with **`=` padding**
(`base64.rs:169` treats only `b'='` as padding, and only at chunk index
2 or 3). This is **not** RFC 4648, and it is **not** the `~`-padded I2P
Base64 variant used by the router-hash codec in `i2pr-netdb`. The codec
rejects characters outside the alphabet (RFC 4648 `+` and `/` surface
as `SamBase64Error::InvalidCharacter`), inputs that are not a multiple
of four characters, padding at an invalid position or after a value
byte, and decoded outputs exceeding the caller-supplied ceiling.

### `SamPrivateDestination` format

The standard Java `PrivateKeyFile` concatenation
(see [`specs/references/sam31-private-destination.md`](../../specs/references/sam31-private-destination.md)):

```text
PUB     = canonical Destination encoding (391 bytes for SIGNATURE_TYPE=7 / CRYPTO_TYPE=4)
PRIV    = Base64(Destination || X25519_static_secret [32] || Ed25519_signing_seed [32])
Length  = 455 bytes binary (PUB_LENGTH=391, PRIV_LENGTH=455), 608 characters Base64 (with `=` padding)
```

The generator uses the existing
`DestinationIdentity::from_private_bytes(signing_seed, static_secret,
padding)` path; the import path uses
`DestinationIdentity::from_imported(destination, signing_seed,
static_secret)` (Plan 146), which preserves the destination's embedded
encryption public field verbatim and only checks
`signing_public == EdDSA(signing_seed)` — a mismatch reports
`DestinationIdentityError::ImportSigningKeyMismatch`. The reference
implementations populate the encryption field with random bytes, so the
tolerance matches reference behaviour. The narrow SAM-specific accessor
on `DestinationIdentity` is `signing_seed_bytes()`, documented in
`crates/i2pr-client/src/identity.rs` as the sole narrow path for the SAM
codec; bidirectional evidence lives at
`crates/i2pr-daemon/tests/sam_plan146_reference.rs`.

### SAM secret ownership

`SamPrivateDestination` is non-`Clone`; `Zeroize` on drop via the inner
`Zeroizing<[u8; PRIV_LENGTH]>`; the manual `Debug` emits only
`<redacted>` placeholders; and `PartialEq`/`Eq` compare **only** the
public destination portion, so two wrappers compare equal iff they
describe the same destination without ever touching secret bytes.

### SAM version negotiation

The server support set is the literal pair
`MIN_SUPPORTED_VERSION == MAX_SUPPORTED_VERSION ==
SamVersion::const_new(3, 1)` (`version.rs:17-20`), so the advertised
range is exactly `[3.1, 3.1]`. `negotiate` intersects the client's
`MIN`/`MAX` with that set and returns
`NegotiatedVersion::Agreed(SamVersion { major: 3, minor: 1 })` for the
canonical client range; any non-overlapping range returns
`NegotiatedVersion::NoOverlap` rather than accepting a nearest version.

### STREAM CONNECT / ACCEPT and FORWARD policy

`parse_stream_connect` / `parse_stream_accept` produce typed requests
(`StreamConnectRequest` / `StreamAcceptRequest`) with mapped failure
enums; the appliers `apply_stream_connect_outcome` /
`apply_stream_accept_outcome` return `DispatchOutcome::RequireStreamConnect`
/ `RequireStreamAccept` on success and `StreamRawMode` where the Plan 143
raw-mode path applies. The per-session `SamStreamRegistry` holds a FIFO
pending-accept queue under a per-session stream-socket ceiling and a
per-session pending-accept ceiling, and its atomic `InboundMode` makes
`STREAM FORWARD` registration mutually exclusive with pending
`STREAM ACCEPT` waiters. Failures are typed
(`SamStreamRegistryError`).

`STREAM FORWARD` is loopback-only: `PORT` is mandatory, an omitted
`HOST` uses the forwarding socket's loopback peer, and explicit hosts
are numeric loopback literals or `localhost` after
`normalize_forward_host`. There is no resolver, Unix socket, TLS, or
clearnet pivot.

### `NAMING LOOKUP` policy

`sam::naming` is deliberately local and is the bridge to
[i2pr-addressbook.md](i2pr-addressbook.md). `ME` is available only on a
session control connection; a complete public Destination is strictly
decoded and canonicalized by `resolve_public_destination`; a valid
locally-owned `.b32.i2p` hash is resolved by
`decode_b32_destination_hash` and looked up through the existing session
registry. Other b32 and human-readable `.i2p` names return
`KEY_NOT_FOUND`; malformed values return `INVALID_KEY`. No system DNS or
second address book is introduced.

### Error enums

Every module exposes its own typed error enum; no `anyhow` is used.

- `version::SamVersionParseError` — empty, control-byte, extra-component, signed, overflow, whitespace-contaminated inputs.
- `base64::SamBase64Error` — `InvalidLength`, `InvalidCharacter`, `InvalidPadding`, decoded-too-large.
- `command::UnknownCommand`, `command::MissingOption`, `command::UnknownOption`, `command::UnsupportedStyle`, `command::StreamConnectError`, `command::StreamAcceptError`.
- `parser::ParseError` — line too long, control bytes, invalid quoting, trailing escape, malformed command.
- `private_destination::SamPrivateDestinationError` — length mismatch, codec rejection, identity rejection, public/private mismatch, Base64 failure.
- `dest_generate::DestGenerateError` — randomness unavailable, private-destination failure.
- `session_create::SessionCreateError` — missing/unsupported option, invalid destination.
- `limits::SamLimitsError`, `session::SamSessionCountersError`, `line_reader::LineReaderError`.
- `registry::SamSessionRegistryError`, `streams::SamStreamRegistryError`, `forward::StreamForwardError`, `naming::NamingLookupError`.
- `i2cp::error::I2cpError` — `InvalidProtocolByte`, `BodyTooLarge`, `UnknownMessageType`, `DeprecatedMessageType`, `UnsupportedMessageType`, `Incomplete`, `Malformed`, `IllegalInState`, `VersionNegotiation`, `AuthNotSupported`, `HandshakeOrdering`, `SessionConfigLimit`, `SignatureRejected`, `CreationTimestampOutOfRange`, `UnsupportedSigningType`, `UnsupportedCryptoType`, `OptionRejected`, `OptionParseFailed`, `SessionRegistry`, `SessionIdExhausted`, `ReconfigureRejected`.
- `i2cp::data_plane::DataPlaneError` — `CapacityExceeded` (the single bounded-failure type shared by the correlation table and the inbound queue).

## Dependencies

`crates/i2pr-api/Cargo.toml` — production (all workspace members
except the three external crates):

```text
i2pr-client   DestinationIdentity, from_private_bytes, from_imported, signing_seed_bytes
i2pr-crypto   used only via i2pr-client; no direct crypto access
i2pr-proto    Destination::decode for padding extraction, MAX_COMMON_STRUCTURE_SIZE
i2pr-tunnel   TunnelLifetime (i2cp/config.rs:22, default_tunnel_lifetime)
rand_core     CSPRNG injection
thiserror     error derives
zeroize       Zeroizing wrapper
```

Dev dependencies:

```text
ed25519-dalek  signature fixtures for SessionConfig verification tests
rand_chacha    deterministic seeded RNG for tests
```

`scripts/check-dependency-direction.sh` records the allowlist as
exactly `{"i2pr-client", "i2pr-crypto", "i2pr-proto", "i2pr-tunnel"}`.
`i2pr-api` does **not** depend on `i2pr-daemon` or `i2pr-runtime`, and
`i2pr-client` must never depend on `i2pr-api`.

The SAM Base64 codec does not pull in `base64ct`; it is implemented
directly, justified by the narrow scope (standard alphabet, ≤ 1024-byte
input ceiling).

**Runtime-neutrality, verified.** `bash scripts/check-runtime-boundaries.sh`
reports `runtime boundary checks passed`, but that script has **no
`i2pr-api` section**: it only covers `i2pr-transport*`,
`i2pr-i2pcontrol`, and `i2pr-service-tunnels` source trees. What it
*does* guarantee for this crate is the manifest-level Tokio ban (it
scans every `crates/*/Cargo.toml` except `i2pr-runtime` and
`i2pr-testkit`). Direct source audit of `crates/i2pr-api/src/` and
`tests/` found:

- no `tokio::` path, no `async fn`, no `async_trait`, no channel or
  `spawn` call of any kind;
- no grouped `use std::{…}` block anywhere in the crate (the pattern
  that can hide an import from a literal grep);
- no `std::fs` in `src/`. One `std::net::IpAddr` import exists in
  `src/sam/forward.rs:7` (with `std::net::Ipv4Addr` /
  `std::net::Ipv6Addr` constructions at `forward.rs:25-26`) — this is
  `IpAddr` used as validated loopback **data** in the FORWARD host
  policy, not socket ownership, and matches the posture the checker
  explicitly permits for `i2pr-service-tunnels`. It is nonetheless a
  literal `std::net` use in a crate `AGENTS.md` describes as having no
  `std::net`, and no checker rule currently covers it.
- `tests/i2cp_vectors.rs:24` uses `std::fs::read_to_string` to read the
  committed fixture corpus. That is test-only, expected, and outside
  the production boundary.

## Tests

`cargo test --locked -p i2pr-api` → **253 tests, 0 failed, 0 ignored**:

| Suite | Count |
| --- | --- |
| `src/lib.rs` unit tests | 234 |
| `tests/i2cp_vectors.rs` | 15 |
| `tests/sam_plan139.rs` | 4 |

Unit tests per module (I2CP 116, SAM 118):

| Module | Tests | Module | Tests |
| --- | --- | --- | --- |
| `i2cp::message` | 30 | `sam::server_state` | 17 |
| `i2cp::config` | 19 | `sam::parser` | 15 |
| `i2cp::connection` | 13 | `sam::line_reader` | 14 |
| `i2cp::session` | 11 | `sam::base64` | 10 |
| `i2cp::verify` | 10 | `sam::registry` | 10 |
| `i2cp::frame` | 10 | `sam::streams` | 8 |
| `i2cp::data_plane` | 7 | `sam::limits` | 7 |
| `i2cp::payload` | 7 | `sam::reply` | 5 |
| `i2cp::mapping` | 6 | `sam::private_destination` | 6 |
| `i2cp::ids` | 2 | `sam::session_create` | 6 |
| `i2cp::actions` | 1 | `sam::version` | 6 |
| — | — | `sam::command` | 3 |
| — | — | `sam::dest_generate` | 3 |
| — | — | `sam::session` | 4 |
| — | — | `sam::forward` | 2 |
| — | — | `sam::naming` | 2 |

Malformed and adversarial coverage:

- **SAM parser** — canonical `HELLO` 3.1, command case normalization,
  oversized line, NUL/control-byte rejection, duplicate critical
  options, unsupported style, escaped quote, trailing escape, unknown
  command, unknown action, `NAMING LOOKUP OPTIONS=true`,
  `STREAM CONNECT FROM_PORT/TO_PORT`, `STREAM ACCEPT SILENT`.
- **SAM private destination** — `PRIV` Base64 round-trip, exact
  `PUB`/`PRIV` lengths, truncation rejection, private-key mutation
  rejection, wrong-length rejection, `Debug` redaction.
- **SAM Base64** — round-trip for each tail length, `==` padding rules,
  character/length/ceiling/padding-position rejection.
- **SAM version** — canonical `3.1`, empty/extra-component/signed/
  control-byte/overflow rejection, overlap agrees, disjoint rejects,
  advertisement is only `3.1` (`3.0`/`3.2`/`3.3` rejected).
- **I2CP frame / message** — oversize length, truncated body, unknown
  type, deprecated type, bad destination, short signature, trailing
  bytes; `I2cpError` mapping for each.
- **I2CP verify** — signature rejection, creation-timestamp just
  outside the ±30 s window, mapping-shape limits.
- **I2CP config** — ceiling-exceeding quantity rejected, supported
  inbound quantity capped by the router ceiling, `default_tunnel_lifetime`
  and `default_registry_config` within ceiling.

Per-session ceiling coverage follows the repo rule (capacity 1, exact
load, max+1, and lease release on drop/timeout/cancel/teardown) for the
`PendingStatusTable` and `InboundPayloadQueue`; the byte ceiling is
exercised through `InboundPayloadFrame::wire_size()` so the
`WIRE_OVERHEAD_BYTES` term is always accounted for. Session-id
exhaustion is covered (`session_id_exhaustion_returns_typed_error`).

Determinism: the crate has no wall-clock sleeps and no async runtime.
`verify_session_config` is driven by the injected `Clock`, with
`FixedClock` in tests; `SamLimits::loopback_test_profile` exists
specifically so the suite does not race `tokio::time::test-util`
auto-advance (`src/sam/limits.rs:178`, `:206`).

**Committed I2CP wire-vector corpus.** `tests/fixtures/i2cp/` holds 30
hex fixtures plus `manifest.tsv` and `README.md`, pinned by
`scripts/check-i2cp-vectors.sh` and asserted in
`crates/i2pr-api/tests/i2cp_vectors.rs`. The checker validates every
row's id/path uniqueness, `category ∈ {positive, malformed}`,
64-hex-character `expected_hash` against `sha256sum`, containment in
`tests/fixtures/i2cp/*`, and file existence; it also requires that no
fixture escapes the manifest, and that 22 ids are present (including
`malformed-oversize-length`, `malformed-truncated-body`,
`malformed-unknown-type`). The malformed set additionally covers
`malformed-bad-destination`, `malformed-deprecated-type`,
`malformed-short-signature`, and `malformed-trailing-session-status`.

## Distinctive design choices

1. **One canonical reply encoder.** Hand-formatted SAM reply strings are
   forbidden outside `reply.rs`, so the wire spelling cannot drift
   between the daemon's socket tasks and the tests.
2. **Server advertises exactly one version.** The SAM support set is the
   literal pair `MIN_SUPPORTED_VERSION == MAX_SUPPORTED_VERSION ==
   SamVersion::const_new(3, 1)`, so `[3.1, 3.1]` is a fact of the type,
   not a configuration.
3. **Manual quote unescape after tokenisation.** The parser strips outer
   quotes and unescapes `\"` and `\\` only after tokenising, keeping the
   tokeniser simple and the option validator narrow.
4. **Public-only `PartialEq` on secret-owning types.**
   `SamPrivateDestination` compares only the public destination bytes —
   never the secret material — which is both timing-side-safe and
   semantically the right question.
5. **`SamPrivateDestination` is a thin wrapper, not a new identity
   type.** It composes the existing `DestinationIdentity` and the only
   new identity accessor is the narrow, documented
   `signing_seed_bytes()`.
6. **Verification takes a clock; it never reads one.** `verify_session_config`
   requires an injected `Clock`, so the ±30 s skew is exactly
   reproducible in tests and the crate holds no ambient time source.
7. **Router-wide ceilings are authoritative.** The projected
   `DestinationConfig` always goes through `DestinationConfig::try_new`,
   so a client-supplied I2CP option can be rejected but never allowed to
   exceed a router ceiling.
8. **Two mapping postures, one signing region.** Strict sorted encoding
   for `SessionConfig` (retaining the exact received signed region for
   later verification) and lenient normalize for `GetDate` auth and
   `HostReply` options — one reuse point, two explicit strictnesses.
9. **Classify before parse.** Deprecated, unsupported, and unknown I2CP
   message types are rejected without body parsing, so framing can never
   desynchronize on a type the profile does not implement.
10. **Overhead is a named constant, not a magic number.**
    `InboundPayloadFrame::WIRE_OVERHEAD_BYTES` sits next to `wire_size()`
    so the daemon and the ceiling tests reserve the same frame-sized
    slot; the same reasoning drives `i2cp::config::ceilings`.

## Cross-references

**ADRs**

- [ADR 0001 (modular monolith)](../adr/0001-modular-monolith.md)
- [ADR 0002 (Tokio runtime boundary)](../adr/0002-tokio-runtime-boundary.md) — `i2pr-api` owns no Tokio resources.
- [ADR 0003 (bounded supervised services)](../adr/0003-bounded-supervised-services.md) — the per-session ceilings.
- [ADR 0006 (private identity storage)](../adr/0006-private-identity-storage.md) — why no private identity is duplicated here.
- [ADR 0010 (transport contracts and crate boundaries)](../adr/0010-transport-contracts-and-crate-boundaries.md) — `i2pr-api` sits above the transport boundary.

**Plans and closure records.** `plans/closure/*/*-status.md` wins over
`plans/registry.md`.

*I2CP (Milestone 9, closed):*

- Plan 163 — planning authority, `registered-m9-i2cp-roadmap` ([`plans/closure/i2cp/163-status.md`](../../plans/closure/i2cp/163-status.md)).
- Plan 164 — wire/profile foundation, `passed-m9-i2cp-protocol-and-wire-foundation` ([`164-status.md`](../../plans/closure/i2cp/164-status.md)).
- Plan 165 — connection/session/options, `passed-m9-i2cp-connection-session-and-options` ([`165-status.md`](../../plans/closure/i2cp/165-status.md)).
- Plan 166 — client-owned destination + LeaseSet2 bridge, `passed-m9-i2cp-client-owned-destination-and-leaseset2` ([`166-status.md`](../../plans/closure/i2cp/166-status.md)).
- Plan 167 — loopback server runtime, `passed-m9-i2cp-loopback-server-runtime` ([`167-status.md`](../../plans/closure/i2cp/167-status.md)).
- Plan 168 — message data plane, `passed-m9-i2cp-message-data-plane` ([`168-status.md`](../../plans/closure/i2cp/168-status.md)).
- Plan 169 — self-composed local product + hardening, `passed-m9-i2cp-self-composed-local-product-and-hardening` ([`169-status.md`](../../plans/closure/i2cp/169-status.md)).
- Plan 170 — independent clients (Java I2P 2.13.0 `9134f808…`, go-i2cp `b529ee1c…`), `passed-m9-i2cp-independent-clients-and-final-closure` ([`170-status.md`](../../plans/closure/i2cp/170-status.md)); final acceptance superseded by Plan 172.
- Plan 171 — invalid-preamble close corrective, `passed-m9-i2cp-invalid-preamble-close-and-ci-corrective` ([`171-status.md`](../../plans/closure/i2cp/171-status.md)).
- Plan 172 — final acceptance, `passed-m9-i2cp-independent-leaseset2-lifecycle-corrective`; Milestone 9 closed ([`172-status.md`](../../plans/closure/i2cp/172-status.md)).

*SAM 3.1 (Milestone 7, closed):*

- Plan 149 — self-composed local product, `passed-m7-sam31-self-composing-local-product-corrective` ([`plans/closure/sam/149-status.md`](../../plans/closure/sam/149-status.md)).
- Plan 150 — external-client core evidence retained, `external-client-core-passed-final-acceptance-superseded-by-plan151` ([`150-status.md`](../../plans/closure/sam/150-status.md)).
- Plan 151 — final localhost acceptance, `passed-m7-sam31-final-acceptance-evidence-correction` ([`151-status.md`](../../plans/closure/sam/151-status.md)). It records that the official `libsam3` snapshot was **built and probed but deliberately not counted**, because its public key-length API rejects i2pr's compact Ed25519 `PRIV` shape.
- Plan 148 — blocked independent-client closure audit, `blocked-audit-superseded-by-plan149-150-corrective-sequence`; historical only ([`148-status.md`](../../plans/closure/sam/148-status.md)).
- Plan 152 — narrow M6 session/streaming robustness corrective retained underneath Plan 151, `passed-m6-session-streaming-robustness-corrective` ([`152-status.md`](../../plans/closure/sam/152-status.md)).

*Specs and evidence:*

- [`specs/references/sam31-private-destination.md`](../../specs/references/sam31-private-destination.md) — private-destination format provenance and the standard Java `PrivateKeyFile` concatenation.
- [`specs/protocols/08-sam.md`](../../specs/protocols/08-sam.md) — the SAM dossier.
- [`specs/protocols/10-i2cp-service-tunnels.md`](../../specs/protocols/10-i2cp-service-tunnels.md) — the I2CP feature table.
- [`specs/support.toml`](../../specs/support.toml) — support rows `sam.v31-*` and `i2cp.wire-foundation`, `i2cp.message-codecs`, `i2cp.connection-session-options`, `i2cp.client-owned-destination`, `i2cp.loopback-server-runtime`, `i2cp.message-data-plane`, `i2cp.self-composed-local-product`, `i2cp.invalid-preamble-close`. No row is advertised.
- [`specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md) — what counts as evidence.
- `tests/integration/sam/` and `tests/integration/i2cp/` — reproducible loopback harnesses (application-layer localhost evidence only).
- `tests/fixtures/i2cp/` — committed wire vectors, pinned by `scripts/check-i2cp-vectors.sh`.

**Related deep dives**

- [i2pr-client.md](i2pr-client.md) — `DestinationIdentity`, destination config ceilings, client-owned LeaseSet2 runtime.
- [i2pr-tunnel.md](i2pr-tunnel.md) — `TunnelLifetime` and the inbound tunnel pool behind `RequestVariableLeaseSet`.
- [i2pr-proto.md](i2pr-proto.md) — `Destination` wire codec and structure ceilings.
- [i2pr-crypto.md](i2pr-crypto.md) — protocol crypto wrappers (used transitively only).
- [i2pr-netdb.md](i2pr-netdb.md) — the separate `~`-padded I2P Base64 router-hash codec.
- [i2pr-addressbook.md](i2pr-addressbook.md) — the bridge behind local `NAMING LOOKUP`.
- [i2pr-daemon.md](i2pr-daemon.md) — the sole owner of the SAM and I2CP listeners and the only projector of `I2cpAction` into runtime state.
- [overview.md](overview.md) and [dependency-graph.md](dependency-graph.md) — crate index, data flow, and the dependency allowlist.
