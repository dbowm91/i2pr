# i2pr-api

`i2pr-api` is the application-protocol adapter layer for the `i2pr`
router. Plan 136 lands it as the smallest trustworthy SAM 3.1
foundation on which the SAM server can be built; Plan 137 adds the
loopback-server / session-lifecycle surface; Plan 138 adds the
STREAM CONNECT / ACCEPT transport-bridge surface.

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

The crate owns **no** sockets, timers, channels, or Tokio tasks. It
is a pure runtime-neutral surface that Plan 137–138 wires into the
supervised loopback listener through `i2pr-daemon`.

## What this crate owns

- The bounded SAM v3.1 line parser and the typed command surface.
- The typed reply model and the canonical encoder for `HELLO REPLY`,
  `DEST REPLY`, `SESSION STATUS`, `STREAM STATUS`, `NAMING REPLY`,
  and `PONG`.
- The strict RFC 4648 SAM Base64 codec (different from the I2P Base64
  variant used for router hashes).
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
- The bounded per-session `SamStreamRegistry` (FIFO pending-accept
  queue, per-session stream-socket ceiling, per-session pending-accept
  ceiling) and `SamStreamRegistryError` (Plan 138).
- Exact SAM version negotiation (server advertises only `3.1`).

## What this crate does not own

- No sockets, listeners, or accept loops (Plan 137).
- No Tokio runtime, no channels, no timers (Plan 137).
- No session lifetime or SAM control-socket ownership (Plan 137).
- No `STREAM CONNECT` / `STREAM ACCEPT` byte movement over the
  Milestone 6 `StreamingDestinationAdapter` (Plan 138 runtime path;
  the runtime-neutral `SamStreamRegistry` lives here).
- No `STREAM FORWARD` or `NAMING LOOKUP` byte movement (Plan 139).
- No `i2pd` / Java I2P interop harness (Plan 140).

## Layering

```text
i2pr-api
  -> i2pr-client     (DestinationIdentity, from_private_bytes, signing_seed_bytes)
  -> i2pr-crypto     (used only via i2pr-client; no direct crypto access)
  -> i2pr-proto      (RouterIdentity::decode for padding extraction, MAX_COMMON_STRUCTURE_SIZE)
```

`i2pr-api` does **not** depend on `i2pr-daemon` and does **not**
depend on `i2pr-runtime`. `i2pr-client` must never depend on
`i2pr-api`. The boundary is enforced by
`scripts/check-dependency-direction.sh`.

## Module layout

```text
crates/i2pr-api/
├── Cargo.toml            workspace member, depends on i2pr-client/i2pr-crypto/i2pr-proto
└── src/
    ├── lib.rs            facade and re-exports
    └── sam/
        ├── mod.rs            module facade and named byte ceilings
        ├── version.rs        SamVersion, parse_version, negotiate, is_advertised
        ├── base64.rs         RFC 4648 SAM Base64 codec (encode/decode, strict)
        ├── command.rs        Command, CommandKind, OptionPair, CommandOutcome,
        │                      malformed/unknown enums,
        │                      parse_stream_connect / parse_stream_accept (Plan 138)
        ├── parser.rs         parse_line, tokenise, recognise_* per command family
        ├── reply.rs          ReplyLine, Reply, HelloReply, DestReply, SessionStatus,
        │                      StreamStatus (with `result()` accessor), NamingReply, PongReply
        ├── private_destination.rs  SamPrivateDestination wrapper, from_identity/from_base64/from_bytes, into_identity
        ├── dest_generate.rs        DestGenerateRequest, DestGenerateSignatureType, dest_generate core op
        ├── session_create.rs       SessionCreateRequest, parse_session_create
        ├── limits.rs               SamLimits + loopback_test_profile (Plan 137)
        ├── session.rs              SamSessionId, SamSessionCounters (Plan 137)
        ├── registry.rs             SamSessionRegistry, reserve/commit/rollback (Plan 137)
        ├── line_reader.rs          LineReader, LineEvent (Plan 137)
        ├── server_state.rs         ServerConnectionState, dispatch, apply_session_outcome,
        │                           RequireStreamConnect / RequireStreamAccept dispatch outcomes
        │                           (Plan 138)
        └── streams.rs              SamStreamRegistry, SamStreamAttachment,
                                    SamStreamRegistryError, SamStreamRegistryHandle (Plan 138)
```

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
    parser::{ParseError, parse_line},
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
padding)` reconstruction path. It does not introduce a new identity
type or alter the secret-ownership model. The narrow SAM-specific
accessor added to `DestinationIdentity` is `signing_seed_bytes()`,
which is documented in `crates/i2pr-client/src/identity.rs` as the
sole documented narrow path for the SAM codec. Provenance is recorded
in [`specs/references/sam31-private-destination.md`](../specs/references/sam31-private-destination.md).

### SAM Base64

Standard RFC 4648 alphabet (`A-Z a-z 0-9 + /`) with `=` padding.
**Not** the I2P Base64 variant (`-`/`?` with `~` padding) used for
router hashes. The codec rejects:

- characters outside the RFC 4648 alphabet;
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
- `specs/references/sam31-private-destination.md`: provenance for the
  private-destination format and the standard Java `PrivateKeyFile`
  concatenation.
- `specs/protocols/08-sam.md`: the SAM dossier and authoritative
  sources.
- `docs/architecture/dependency-graph.md`: the full dependency
  allowlist including the new `i2pr-api` edges.