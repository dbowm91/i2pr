# i2pr-api

`i2pr-api` is the application-protocol adapter layer for the `i2pr`
router. Plan 136 lands it as the smallest trustworthy SAM 3.1
foundation on which the SAM server can be built; Plan 137 adds the
loopback-server / session-lifecycle surface; Plan 138 adds the
STREAM CONNECT / ACCEPT transport-bridge surface; Plan 139 adds
loopback-only STREAM FORWARD and local NAMING LOOKUP policy. Plan 146
requalifies the private-destination wire representation against the
reference implementations. Plan 149 composes these runtime-neutral
surfaces in `i2pr-daemon`; `SESSION STATUS DESTINATION=` carries the
private destination and `NAMING LOOKUP NAME=ME` returns its public
counterpart. Plan 150 retains the localhost independent-client layer
with exact i2psam and qualified i2plib.sam evidence; Plan 151 passed
the final acceptance and Plan 152 is the retained narrow M6 corrective
underneath it; router-to-router interoperability remains unclaimed.

Plan 164 adds the runtime-neutral I2CP wire/profile foundation
(`src/i2cp/`): strict bounded framing, structural message codecs,
and the explicit M9 compatibility profile.

Plan 165 extends `src/i2cp/` with the connection state machine,
canonical SessionConfig verification (signature, date, options,
ceiling checks), the bounded option disposition table and
projection into `i2pr-client::DestinationConfig`, the bounded
runtime-neutral session registry (reserve / commit / rollback),
the reconfiguration taxonomy, and the typed `I2cpAction`
vocabulary the Plan 167 daemon projects into runtime state. No
listener, destination activation, or client-interoperability
claim follows; those belong to Plans 166–170.

> [Plan 136](../plans/136-m7-sam31-protocol-private-destination-foundation.md):
> create the `i2pr-api` crate at the intended application-adapter
> layer; implement a strict bounded SAM v3.1 line/command/reply model;
> implement exact version negotiation for the declared baseline;
> reconcile and implement the SAM private-destination representation
> required by `DEST GENERATE` and `SESSION CREATE`; expose a narrow
> destination construction/import/export seam without weakening
> Milestone 6 secret ownership.
>
> [Plan 137](../plans/137-m7-sam31-loopback-server-session-lifecycle.md):
> add the bounded session registry, line reader, server state
> machine, and per-session resource counters that the daemon's
> loopback listener composes.
>
> [Plan 138](../plans/138-m7-sam31-stream-connect-accept-bridge.md):
> add the bounded per-session STREAM socket registry, the typed
> `STREAM CONNECT` / `STREAM ACCEPT` request parsers, the new
> `RequireStreamConnect` / `RequireStreamAccept` dispatch outcomes,
> and the `apply_stream_connect_outcome` / `apply_stream_accept_outcome`
> appliers.

> [Plan 139](../plans/139-m7-sam31-forward-naming-hardening.md):
> adds bounded FORWARD and naming request models, atomic ACCEPT/FORWARD
> inbound-mode ownership, canonical local naming outcomes, and explicit
> unsupported-feature paths.

The crate owns **no** sockets, timers, channels, or Tokio tasks. It
is a pure runtime-neutral surface that Plans 137–139 wire into the
supervised loopback listener through `i2pr-daemon`. The Plan 164
`i2cp` module keeps the same boundary for the Milestone 9 I2CP
passes: framing and structural codecs live here, while TCP/Tokio
ownership stays in `i2pr-daemon` (Plan 167) and destination behavior
stays in `i2pr-client` (Plan 166).

## What this crate owns

- The bounded SAM v3.1 line parser and the typed command surface.
- The typed reply model and the canonical encoder for `HELLO REPLY`,
  `DEST REPLY`, `SESSION STATUS`, `STREAM STATUS`, `NAMING REPLY`,
  and `PONG`.
- The strict **I2P Base64** SAM codec (`A-Z a-z 0-9 - ~`, `=`
  padding — the spelling every Java I2P / i2pd / independent Python
  SAM client reference implementation emits; see
  [`specs/references/sam31-private-destination.md`](../../specs/references/sam31-private-destination.md)
  for the corroborating references). This is **not** RFC 4648, and
  it is also **not** the I2P Base64 variant that uses `~` for
  padding (the router-hash codec in `i2pr-netdb::base64` uses that
  variant — SAM uses `=` for padding).
- The `SamPrivateDestination` wrapper that owns the standard Java
  `PrivateKeyFile` concatenation and provides the SAM-compatible
  `PRIV` encoding.
- The `dest_generate` runtime-neutral core operation that produces
  `SamPrivateDestination` from an injected CSPRNG.
- The `parse_session_create` typed request parser that accepts
  `STYLE=STREAM` with either `DESTINATION=TRANSIENT` or a verified
  imported `PRIV`.
- The `parse_stream_connect` / `parse_stream_accept` typed request
  parsers and their mapped failure enums (Plan 138).
- The loopback-only `parse_stream_forward` request/host policy and the
  local `parse_naming_lookup`, canonical public-Destination, and b32
  validation helpers (Plan 139).
- The bounded per-session `SamStreamRegistry` (FIFO pending-accept
  queue, per-session stream-socket ceiling, per-session pending-accept
  ceiling), its atomic `InboundMode`, and `SamStreamRegistryError`
  (Plans 138–139).
- Exact SAM version negotiation (server advertises only `3.1`).

## What this crate does not own

- No sockets, listeners, or accept loops (Plan 137).
- No Tokio runtime, no channels, no timers (Plan 137).
- No session lifetime or SAM control-socket ownership (Plan 137).
- No socket ownership or raw byte movement: Plan 139 supplies the
  runtime-neutral FORWARD/naming seam, while `i2pr-daemon` owns local
  sockets and the bounded bridge.
- No independent-client interoperability claim. The lightweight Plan 140
  provenance/evidence lane is in `tests/integration/sam/`; it does not
  import this crate's internals or promote the local capture seam to live
  STREAM evidence.

## Layering

```text
i2pr-api
  -> i2pr-client     (DestinationIdentity, from_private_bytes (generator), from_imported (SAM import), signing_seed_bytes)
  -> i2pr-crypto     (used only via i2pr-client; no direct crypto access)
  -> i2pr-proto      (Destination::decode for padding extraction, MAX_COMMON_STRUCTURE_SIZE)
```

`i2pr-api` does **not** depend on `i2pr-daemon` and does **not**
depend on `i2pr-runtime`. `i2pr-client` must never depend on
`i2pr-api`. The boundary is enforced by
`scripts/check-dependency-direction.sh`.

## Module layout

```text
crates/i2pr-api/
├── Cargo.toml            workspace member, depends on i2pr-client/i2pr-crypto/i2pr-proto/i2pr-tunnel
└── src/
    ├── lib.rs            facade and re-exports
    ├── i2cp/               Plans 164+165 runtime-neutral I2CP foundation
    │   ├── mod.rs            module facade, M9 compatibility profile
    │   ├── frame.rs          0x2a preamble, common frame, incremental decoder
    │   ├── message.rs        type IDs, dispositions, structural body codecs
    │   ├── ids.rs            SessionId/MessageId/ClientNonce/HostRequestId
    │   ├── payload.rs        payload wrapper, gzip metadata contract
    │   ├── mapping.rs        strict/lenient Mapping postures, signing bytes
    │   ├── error.rs          I2cpError typed classification
    │   ├── connection.rs     Plan 165 ConnectionStateMachine + version handshake
    │   ├── verify.rs         Plan 165 SessionConfig signature/date/ceiling verification
    │   ├── config.rs         Plan 165 option disposition table + DestinationConfig projection
    │   ├── session.rs        Plan 165 bounded session registry
    │   └── actions.rs        Plan 165 typed I2cpAction vocabulary
    └── sam/
        ├── mod.rs            module facade and named byte ceilings
        ├── version.rs        SamVersion, parse_version, negotiate, is_advertised
        ├── base64.rs         SAM I2P Base64 codec (encode/decode, strict; `-`/`~`, `=` padding)
        ├── command.rs        Command, CommandKind, OptionPair, CommandOutcome,
        │                      malformed/unknown/unsupported enums,
        │                      stream request parsers (Plans 138–139)
        ├── parser.rs        parse_line, tokenise, recognise_* per command family
        ├── reply.rs         ReplyLine, Reply, HelloReply, DestReply, SessionStatus,
        │                     StreamStatus (with `result()` accessor), NamingReply, PongReply
        ├── private_destination.rs  SamPrivateDestination wrapper, from_identity/from_base64/from_bytes, into_identity
        ├── dest_generate.rs        DestGenerateRequest, DestGenerateSignatureType, dest_generate core op
        ├── session_create.rs       SessionCreateRequest, parse_session_create
        ├── limits.rs               SamLimits + loopback_test_profile (Plan 137)
        ├── session.rs              SamSessionId, SamSessionCounters (Plan 137)
        ├── registry.rs             SamSessionRegistry, reserve/commit/rollback (Plan 137)
        ├── line_reader.rs          LineReader, LineEvent (Plan 137)
        ├── server_state.rs         ServerConnectionState, dispatch, stream/naming appliers,
        │                           Require* dispatch outcomes (Plans 138–139),
        │                           and StreamRawMode (Plan 143)
        ├── streams.rs              SamStreamRegistry, SamStreamAttachment,
                                    InboundMode, SamStreamRegistryError,
                                    SamStreamRegistryHandle (Plans 138–139)
        ├── forward.rs              loopback-only STREAM FORWARD request/host policy (Plan 139)
        └── naming.rs               local NAME=ME/public-Destination/.b32 validation (Plan 139)
```

## Plan 164 I2CP surface

`src/i2cp/` owns the M9 wire/profile foundation with no runtime
behavior:

- The `0x2a` preamble check and the common frame (`u32` length +
  `u8` type + body) with an exact 64 KiB ceiling enforced before
  allocation, plus a capped incremental decoder for partial reads.
- Exact type IDs, directions, and M9 dispositions for all 25
  assigned types; deprecated (4/6/7/21/29), unsupported
  (BlindingInfo 42), and unknown (including abandoned 40) types are
  classified without body parsing.
- Structural codecs for the implemented profile (GetDate/SetDate,
  session create/reconfigure/destroy/status, variable-lease
  request, Standard-LeaseSet2 publication with ordered redacted
  zeroized decryption keys, send/expires, payload/status,
  bandwidth, destination and host lookup, disconnect) with strict
  trailing-byte rejection and typed malformed errors.
- Canonical Mapping reuse: strict sorted posture for SessionConfig
  (with the exact received signed region retained for Plans
  165–166) and lenient normalize posture for GetDate auth and
  HostReply options.
- The payload/gzip metadata contract (ports, protocol number,
  flags) with a recorded 64 KiB expansion ceiling and no
  decompression yet (Plan 168).
- Committed vectors under `tests/fixtures/i2cp/` pinned by
  `scripts/check-i2cp-vectors.sh` and asserted in
  `crates/i2pr-api/tests/i2cp_vectors.rs`.

The full feature table lives in `src/i2cp/mod.rs` and
`specs/protocols/10-i2cp-service-tunnels.md`; the support ledger
rows are `i2cp.wire-foundation`, `i2cp.message-codecs`, and
`i2cp.connection-session-options` in `specs/support.toml`. No
support row is advertised.

## Plan 165 I2CP connection / session / option surface

`src/i2cp/connection.rs`, `verify.rs`, `config.rs`, `session.rs`,
and `actions.rs` extend `src/i2cp/` with runtime-neutral state,
verification, registry, option projection, and typed action
vocabulary; they still own no sockets, timers, channels, or task
runtimes:

- **Connection state machine.** `ConnectionStateMachine` tracks
  `AwaitProtocolByte → AwaitGetDate → ReadyForSession →
  SessionPending → Active → Closing → Closed`. The M9 profile
  accepts only API version `0.9.67` and rejects `GetDate`
  authentication. Illegal message families are rejected with
  `I2cpError::IllegalInState`; the machine never silently
  resynchronizes.
- **SessionConfig verification.** `verify_session_config` consumes
  a parsed `SessionConfig` plus the raw body bytes and verifies
  (in order) canonical bytes, supported signing/encryption types,
  certificate agreement, mapping shape (≤ 64 entries, ≤ 96-byte
  keys/values, ≤ 64 KiB total), creation timestamp within ±30 s of
  the injected `Clock` (`FixedClock` in tests, `SystemClock` in
  production), and signature over the retained signed region. The
  only output is `VerifiedSessionConfig`, the typed value the
  Plan 166 destination reservation may see.
- **Option disposition table.** `project_options` parses every
  numeric option strictly (unsigned, no whitespace, no overflow);
  rejects unknown signing/encryption types, zero-hop tunnels,
  guaranteed reliability, and non-fast-receive; records the
  Proposal 171 outbound-tunnel-switching flag as ignored;
  classifies unknown keys. The result is a `ProjectedPolicy` whose
  `DestinationConfig` is built via the existing
  `DestinationConfig::try_new` constructor so router-wide ceilings
  cannot be bypassed.
- **Session registry.** `SessionRegistry` exposes
  `reserve → commit → rollback` for sessions, tracks per-connection
  and per-router ceilings, prevents duplicate Destination ownership
  across active/reserved sessions, and assigns monotonic session
  IDs (skipping `0xffff`). `ReconfigurationClass` classifies every
  known option as `MutableWithRebuild`, `MutableImmediate`,
  `ImmutableAfterCreate`, or `Unsupported`; `validate_reconfigure_classifications`
  enforces all-or-nothing application.
- **Typed actions.** `I2cpAction::ReserveClientDestination`,
  `ReconfigureClientDestination`, `DestroyClientDestination`,
  `RequestBandwidthSnapshot`, and `RequestDestinationLookup` carry
  verified typed values only. The Plan 167 daemon is the sole
  translator from these actions to runtime state.

## Plan 166 — I2CP client-owned LeaseSet2 request action

The typed action vocabulary in `src/i2cp/actions.rs` grows by one
variant for Plan 166:

- **`I2cpAction::RequestVariableLeaseSet`** — emits the lease
  material the M9 client must include in its next Standard
  LeaseSet2. The action payload carries the connection capability, the
  session identifier, the verified destination hash, a typed
  [`LeaseRefreshCause`] (`InitialGeneration` /
  `ApproachingExpiry`), and the deterministic ordered lease list
  sourced from the destination's real inbound tunnel pool. No raw
  client bytes ever appear in the action: the lease material is
  produced by `i2pr_client::DestinationRuntime::take_client_refresh_request`,
  which the Plan 167 daemon hands to the typed `I2cpAction`
  envelope.

The action is non-secret: it carries only typed destination
metadata and the `(gateway, tunnel_id, end_date)` triple the client
should sign. The corresponding inbound-decryption capability stays
inside `i2pr_client`; the api layer never sees a `DestinationIdentity`
or a private key. The `LeaseRefreshCause` enum and the `LeaseRequestLease`
struct are exported from `i2pr_api::i2cp` alongside `I2cpAction` so
downstream consumers never have to translate an integer status.

## Plan 167 — I2CP daemon runtime boundary

The Plan 167 daemon in `crates/i2pr-daemon/src/i2cp.rs` is the
single composition root for the I2CP wire. It owns:

- the loopback `TcpListener` (one per `[i2cp]` block, port `7654`
  by default, `0` for ephemeral tests, IPv4/IPv6 loopback only);
- the per-connection `ChildScope` that owns every accepted socket;
- one `i2pr_api::i2cp::SessionRegistry` for I2CP session IDs;
- one `i2pr_client::DestinationRegistry` populated through the
  Plan 166 client-owned destination runtime.

The api layer owns **no** Tokio, sockets, timers, channels, or
destination private material. The Plan 167 daemon is the only place
where the typed `I2cpAction` vocabulary is projected into runtime
state: `reserve_client_destination` constructs a Plan 166
`DestinationPublic` from the verified `SessionConfig`, runs it
through `DestinationRuntime::new_client_owned`, registers it in
`DestinationRegistry`, commits the `SessionRegistry` reservation, and
hands the assigned session id back to the per-connection task.
`install_client_lease_set2` cross-checks the supplied
`InboundDecryptionCapability` against the destination's static X25519
public key and delegates to `DestinationRuntime::install_client_lease_set2`,
which is the single atomic Plan 166 install path.

The api layer is deliberately ignorant of the daemon. Every
traversal (read, write, frame decode) goes through the runtime-neutral
codecs the api already owns. Plan 168 lands the
`SendMessage` / `SendMessageExpires` / `MessagePayload` /
`MessageStatus` adapters behind the same boundary; Plan 169 lands
the self-composed local I2CP product behind the same boundary;
Plan 170 will land the independent Java/Go client evidence without
touching the api.

## Plan 169 — I2CP reconfiguration transaction surface

Plan 169 closes the M9 I2CP local self-composed product by
implementing the Plan 165 reconfiguration model against the real
client-owned destination runtime. The Plan 169 surface is layered
on top of the existing Plan 165 disposition table and Plan 168
data plane; nothing in the api grows new sockets, timers, or
secrets:

- **`apply_reconfigure` outcome vocabulary** — every Plan 169
  transaction returns a closed classification
  (`Accepted`, `RebuildStaged`, `RebuildRequired`,
  `InvalidOptions`, `ImmutableChange`, `UnsupportedChange`,
  `BadSignature`, `BadDate`, `ShapeRejected`, `Unchanged`,
  `Refused`). The daemon maps the outcome onto a typed
  `SessionStatusCode` (`Updated`, `Invalid`, or `Refused`) for
  the wire reply; tests assert the variant, never the
  status code alone, so regressions cannot silently overclaim.
- **`classify_reconfigure_diff` + `validate_reconfigure_classifications`**
  — the Plan 165 helpers classify each option-key change as
  `MutableWithRebuild`, `MutableImmediate`,
  `ImmutableAfterCreate`, or `Unsupported`. Plan 169 reuses
  them without modification; the helpers are the canonical
  all-or-nothing rule.
- **Atomic baseline replacement** — the runtime-neutral
  `I2cpSessionState::last_options` (`Mapping`) carries the
  previous verified SessionConfig mapping. The reconfigure
  handler replaces the baseline atomically inside a single
  mutex critical section, so the diff is computed against a
  stable view of the previous options regardless of any
  concurrent destroy path.
- **No second secret allocation** — Plan 169 never holds the
  client's destination signing private key. The reconfigure
  handler validates the supplied SessionConfig signature
  against the destination's embedded public key, accepts the
  new options, and lets the existing client-owned
  `DestinationRuntime` observe the staged replacement through
  its existing tunnel pool plumbing. The router never copies,
  clones, or re-derives any private identity material.
- **Bounded outcome enum** — `ReconfigurationOutcome` is
  `#[derive(Debug, Eq, PartialEq)]` and finite; the daemon
  adds no fallthrough branches, so wire-reply mapping cannot
  drift between Plan 169 and the test surface.

Plan 169 §5/§6/§7 evidence lives in three narrowly named
acceptance suites:

```text
crates/i2pr-daemon/tests/i2cp_final_acceptance.rs
crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs
crates/i2pr-daemon/tests/i2cp_resource_matrix.rs
```

The Plan 167 listener/runtime regression in
`crates/i2pr-daemon/tests/i2cp_loopback.rs` and the Plan 168
data-plane suite in
`crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` remain
green; Plan 169 never weakens the Plan 165 disposition table
or the Plan 168 bounded outcome vocabulary to satisfy a
reconfigure case.

## Plan 168 — I2CP message data plane surface

Plan 168 closes the M9 I2CP message/data-plane scope. The new
runtime-neutral surface lives in
`crates/i2pr-api/src/i2cp/data_plane.rs` and is re-exported from
`i2pr-api::i2cp`:

- `I2cpMessageOutcome` — bounded router-side outcome vocabulary
  (`Accepted`, `BadLocalLeaseSet`, `NoLocalTunnels`, `Overflow`,
  `DestinationStopping`, `BadSession`, `BadMessage`,
  `MessageExpired`, `BadExpirationHorizon`, `UnsupportedFlags`,
  `SessionError`). `status_code()` maps every variant to the
  matching `MessageStatusCode`; `Accepted` is the only success
  class in the vocabulary so the daemon never claims stronger
  delivery than the destination runtime observed.
- `I2cpDataPlaneAction::{EnqueueOutboundPayload,
  DeliverInboundPayload, ResolveDestinationLookup}` — the three
  typed data-plane actions the daemon projects into runtime state.
  Every variant carries only typed values; raw client bytes never
  appear in a payload.
- `PendingStatusTable` / `PendingStatusEntry` — bounded per-session
  correlation bookkeeping with explicit count ceiling
  (`MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION = 128`) and
  duplicate/late-idempotent `take()` semantics.
- `InboundPayloadQueue` / `InboundPayloadFrame` — bounded per-session
  inbound frame buffer with explicit frame
  (`MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION = 64`) and byte
  (`MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION = 64 KiB`) ceilings.
  The `InboundPayloadFrame::WIRE_OVERHEAD_BYTES = 14` constant is
  the only per-frame overhead the daemon and tests rely on.
- `DataPlaneError::CapacityExceeded` — single bounded-failure type
  shared by the correlation table and inbound queue.
- Bounded per-session ceilings
  (`MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION`,
  `MAX_CONCURRENT_DESTINATION_LOOKUPS_PER_CONNECTION`,
  `MAX_DESTINATION_LOOKUP_HORIZON = 10 s`,
  `MAX_MESSAGE_EXPIRATION_HORIZON = 1 h`) plus the helper accessors
  `max_pending_messages_per_session`,
  `max_pending_status_correlations`,
  `max_inbound_payload_bytes_per_session`,
  `max_destination_lookup_horizon`.

Plan 168 §3 pins the I2CP payload format: the four-byte length
prefix, the ten-byte gzip header (magic `0x1f 0x8b`, deflate method
`0x08`, no flag bits, source/destination ports in MTIME, xflags `2`,
I2P protocol number in OS), and the existing 64 KiB expansion
ceiling. The structural codec already rejects malformed gzip
metadata before routing; Plan 168 adds the runtime-neutral typed
outcome vocabulary the daemon maps into `MessageStatus` replies.

Configuration surface lives in the daemon: `[i2cp]` adds `enabled`
(default `false`), `bind_address` (default `127.0.0.1`), `port`
(default `7654`), `max_clients`, `max_sessions_per_connection`,
`max_sessions_router`, `max_buffered_bytes_per_connection`,
`max_pending_writes_per_connection`, `protocol_byte_timeout_ms`,
`command_timeout_ms`, and `shutdown_timeout_ms`. Non-loopback bind
addresses fail semantic validation; oversized ceilings fail at the
same step.

## Public surface

The crate re-exports the most commonly used types from
`crates/i2pr-api/src/lib.rs`:

```rust
pub use sam::{
    command::{
        Command, CommandKind, CommandOutcome, MissingOption, SessionStyle, Silently,
        StreamAcceptId, UnknownCommand, UnknownOption, UnsupportedStyle,
    },
    dest_generate::{
        DEST_GENERATE_SIGNATURE_TYPE_ED25519, DestGenerate, DestGenerateError,
        DestGenerateOutcome, DestGenerateRequest, DestGenerateSignatureType, dest_generate,
    },
    forward::{ForwardHost, StreamForwardError, StreamForwardRequest,
        normalize_forward_host, parse_stream_forward},
    parser::{ParseError, parse_line},
    naming::{NamingLookupError, NamingLookupRequest, decode_b32_destination_hash,
        parse_naming_lookup, resolve_public_destination},
    private_destination::{
        PUB_LENGTH, PRIV_LENGTH, SamPrivateDestination, SamPrivateDestinationError,
    },
    reply::{
        DestReply, HelloReply, NamingReply, PongReply, Reply, ReplyLine, SessionStatus,
        StreamStatus,
    },
    session_create::{
        SessionCreateError, SessionCreateRequest, SessionCreateStyle, parse_session_create,
    },
    version::{
        MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION, NegotiatedVersion, SamVersion,
        SamVersionParseError, negotiate, parse_version,
    },
};
```

## Key contracts

### SAM private-destination format

The standard Java `PrivateKeyFile` concatenation:

```text
PUB     = canonical Destination encoding (391 bytes for SIGNATURE_TYPE=7 / CRYPTO_TYPE=4)
PRIV    = Base64(Destination || X25519_static_secret [32] || Ed25519_signing_seed [32])
Length  = 455 bytes binary, 608 characters Base64 (with `=` padding)
```

The i2pr codec uses the existing
`DestinationIdentity::from_private_bytes(signing_seed, static_secret,
padding)` reconstruction path for the `DEST GENERATE` generator and
uses the new `DestinationIdentity::from_imported(destination,
signing_seed, static_secret)` constructor (Plan 146) for the
`SESSION CREATE` import path. `from_imported` preserves the
destination's embedded encryption public field verbatim and only
checks `signing_public == EdDSA(signing_seed)`; a mismatch reports
`DestinationIdentityError::ImportSigningKeyMismatch`. The standard
Java I2P `PrivateKeyFile` (pinned at
`2800040deee9bb376567b671ef2e9c34cf3e30b6`) and i2pd `IdentityEx`
(pinned at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`) populate the
destination encryption field with random bytes for destinations, so
the new constructor's tolerance matches the reference behavior. The
narrow SAM-specific accessor on `DestinationIdentity` is
`signing_seed_bytes()`, which is documented in
`crates/i2pr-client/src/identity.rs` as the sole documented narrow
path for the SAM codec. Provenance is recorded in
[`specs/references/sam31-private-destination.md`](../specs/references/sam31-private-destination.md);
bidirectional evidence lives at
`crates/i2pr-daemon/tests/sam_plan146_reference.rs`.

### SAM Base64

**I2P Base64** alphabet (`A-Z a-z 0-9 - ~`) with `=` padding. This
is the spelling every Java I2P / i2pd / independent Python SAM
client reference implementation emits (see
[`specs/references/sam31-private-destination.md`](../specs/references/sam31-private-destination.md)
for the four independent corroborating references). Plan 142
corrected the prior RFC 4648 alphabet; the SAM Base64 codec now
rejects `+` and `/` as `InvalidCharacter`. This is **not** the I2P
Base64 variant that uses `~` for padding (the router-hash codec in
`i2pr-netdb::base64` uses that variant — SAM uses `=` for padding).
The codec rejects:

- characters outside the I2P Base64 alphabet (RFC 4648 `+`/`/`
  surface as `InvalidCharacter`);
- inputs that are not a multiple of four characters;
- invalid padding positions;
- decoded outputs exceeding the caller-supplied ceiling.

### Version negotiation

The server advertises exactly `[3.1, 3.1]`. Negotiation
intersects the client's `MIN`/`MAX` with the server support set and
returns `NegotiatedVersion::Agreed(SamVersion { major: 3, minor: 1 })`
for the canonical SAM 3.1 client range. Any non-overlapping range
returns `NegotiatedVersion::NoOverlap` rather than accepting a
nearest version.

### Secret ownership

`SamPrivateDestination` is:

- `#![forbid(unsafe_code)]` (inherited from the crate root);
- non-`Clone` (would require copying secrets);
- `Zeroize` on drop via the inner `Zeroizing<[u8; PRIV_LENGTH]>`;
- non-`Debug` for secrets — the manual `Debug` implementation only
  emits `<redacted>` placeholders;
- `PartialEq`/`Eq` compare only the public destination portion of the
  concatenated bytes (timing-side-safe and meaningful — two wrappers
  are equal iff they describe the same destination).

`DestinationIdentity::signing_seed_bytes()` is the **single** new
accessor added to the identity model. It is reserved for the SAM
codec; the comment in `crates/i2pr-client/src/identity.rs` records
this as the narrow exception. No generic public accessor for raw
destination private keys was added.

## Plan 139 policy

`STREAM FORWARD` is an experimental loopback-only surface. `PORT` is
mandatory; an omitted `HOST` uses the forwarding socket's loopback peer;
explicit hosts are numeric loopback literals or `localhost`; no resolver,
Unix socket, TLS, or clearnet pivot is available. The forward control socket
owns the registration, and `InboundMode` makes it mutually exclusive with
pending `STREAM ACCEPT` waiters.

`NAMING LOOKUP` is deliberately local. `ME` is available only on a session
control connection, complete public Destinations are strictly decoded and
canonicalized, and a valid locally-owned `.b32.i2p` hash is looked up through
the existing session registry. Other b32 and human-readable `.i2p` names
return `KEY_NOT_FOUND`; malformed values return `INVALID_KEY`. No system DNS
or second address book is introduced.

## Errors

Each module exposes its own typed error enum:

- `version::SamVersionParseError` — empty, control-byte, extra-component, signed, overflow, whitespace-contaminated inputs.
- `base64::SamBase64Error` — invalid length, character, padding, decoded-too-large.
- `command::{MalformedReason, UnknownCommand}` — malformed line/unknown command.
- `parser::ParseError` — line too long, control bytes, invalid quoting, trailing escape, malformed command.
- `private_destination::SamPrivateDestinationError` — length mismatch, codec rejection, identity rejection, public/private mismatch, Base64 failure.
- `dest_generate::DestGenerateError` — randomness unavailable, private-destination failure.
- `session_create::SessionCreateError` — missing/unsupported option, invalid destination.

All errors are typed enums; no `anyhow` is used.

## Dependencies

From `Cargo.toml` (workspace members only):

```text
i2pr-client
i2pr-crypto
i2pr-proto
rand_core       (CSPRNG injection)
thiserror       (error derives)
zeroize         (Zeroizing wrapper)
```

The SAM Base64 codec does not pull in `base64ct`; it is implemented
directly per Plan 136 §8 ("Reuse an existing repository implementation
if present. Otherwise add the smallest explicit adapter"). The
narrow scope (SAM's standard alphabet, ≤1024-byte input ceiling)
justifies the small explicit implementation.

Dev dependency:

```text
rand_chacha      (deterministic seeded RNG for tests)
```

## Tests

The crate has unit tests inside each module:

| Test group | Coverage |
| --- | --- |
| Version parsing | canonical `3.1`, malformed inputs (empty, extra components, signed, control bytes, overflow) |
| Version negotiation | overlap agrees, disjoint rejects, major mismatch rejects |
| Advertised versions | only `3.1`; rejects `3.0`/`3.2`/`3.3` |
| Base64 | round-trip for each tail length, `==` padding rules, character/length/ceiling/padding rejection |
| Parser | canonical HELLO 3.1, command case normalization, oversized line, NUL/control rejection, duplicate critical options, unsupported style, escaped quote, trailing escape, unknown command, unknown action, NAMING LOOKUP OPTIONS=true, STREAM CONNECT FROM_PORT/TO_PORT, STREAM ACCEPT SILENT |
| Private destination | PRIV round-trip through Base64, exact `PUB`/`PRIV` lengths, truncation rejection, private-key mutation rejection, wrong-length rejection, Debug redaction |
| DEST GENERATE | type-7 round-trip, absent signature-type rejection, signature-type parser accepts known forms |
| SESSION CREATE | TRANSIENT construction, imported reconstruction, unsupported style, invalid PRIV, missing destination, style normalisation |

## Distinctive design choices

1. **`SamPrivateDestination` is a thin wrapper, not a new identity
   type.** It composes the existing `DestinationIdentity` without
   inventing a new key-management model.
2. **No generic private-key getter.** The only new accessor on
   `DestinationIdentity` is the narrow
   `signing_seed_bytes()` for the SAM codec; the accessor is
   documented as the sole narrow path.
3. **Server advertises exactly one version** (3.1). The server
   support set is a literal `MIN_SUPPORTED_VERSION ==
   MAX_SUPPORTED_VERSION == SamVersion::const_new(3, 1)`. A future
   plan may widen the set.
4. **Manual quote unescape.** The parser strips outer quotes and
   unescapes `\"` and `\\` after tokenisation, not during. This keeps
   the tokeniser simple and the option validator narrow.
5. **Reply encoding is centralised.** Hand-formatted strings are
   forbidden in socket tasks (Plan 137). Every reply kind is encoded
   by one canonical encoder in `reply.rs`.
6. **Public-only `PartialEq` on secret-owning types.** The
   `SamPrivateDestination` and `DestinationSource` `PartialEq`
   implementations compare only the public destination bytes —
   never the secret material.
7. **No `tokio`.** The crate is runtime-neutral; Plan 137 owns the
   socket tasks.

## Cross-references

- ADR 0002 (Tokio runtime boundary): `i2pr-api` owns no Tokio
  resources.
- ADR 0010 (Transport contracts): `i2pr-api` is the application
  adapter above the transport boundary.
- Plan 135 (Milestone 7 SAM 3.1 implementation roadmap): defines the
  broader Phase 7 sequence; Plan 136 implements the foundation.
- Plan 136 (SAM 3.1 protocol and private-destination foundation):
  the plan-of-record for this crate; closed as
  `passed-m7-sam31-protocol-private-destination-foundation`.
- Plan 137 (SAM 3.1 loopback server and session lifecycle): the
  loopback listener, session registry, line reader, and
  per-destination `StreamingManager` pool live in `i2pr-daemon`;
  closed as `passed-m7-sam31-loopback-server-session-lifecycle`.
  The Plan 137 runtime-neutral surface (`SamLimits`,
  `SamSessionId`, `SamSessionRegistry`, `LineReader`,
  `ServerConnectionState`, and `dispatch`) is owned by this
  crate; the daemon only owns the Tokio listener and the
  supervised per-socket loop.
- Plan 149 (SAM 3.1 self-composing local product): the daemon now
  composes this crate's session and stream registries with one shared
  destination identity and supervised local STREAM drivers. This
  crate remains runtime-neutral; it does not own the local fabric,
  sockets, or raw byte pump.
- Plan 150 (SAM 3.1 external-client closure): the real loopback listener
  passes the exact pinned i2psam client and qualified i2plib.sam substitute
  for CONNECT/ACCEPT, SILENT, private-destination, FORWARD, NAMING, and
  negative behavior. The reproducible harness lives under
  `tests/integration/sam/`; it is application-layer localhost evidence only.
- `specs/references/sam31-private-destination.md`: provenance for the
  private-destination format and the standard Java `PrivateKeyFile`
  concatenation.
- `specs/protocols/08-sam.md`: the SAM dossier and authoritative
  sources.
- `docs/architecture/dependency-graph.md`: the full dependency
  allowlist including the new `i2pr-api` edges.
