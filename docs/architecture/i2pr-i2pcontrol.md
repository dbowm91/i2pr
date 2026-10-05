# `i2pr-i2pcontrol` — Proposal 170 Runtime-Neutral Contract

Path: `crates/i2pr-i2pcontrol`. Runtime-neutral Proposal 170 / I2PControl wire
and domain contract: exact method/action/type/selector inventories, bounded
JSON-RPC 2.0 envelope semantics, API version 1 authentication vocabulary,
tunnel option metadata with secret classification, wire-level ceilings, the
machine-readable source matrix, and the frozen public inventory. 18 modules,
6 531 source lines, `#![forbid(unsafe_code)]`, no I/O and no clock reads.

## Purpose

This crate owns what is frozen: every wire spelling the control plane accepts
or emits, every ceiling it enforces, and the availability state of every
readable selector. `i2pr-daemon` adapts this contract to live router state;
the crate itself never touches a live owner.

The crate doc comment (`src/lib.rs:9-13`) is explicit about what it owns **no**
of: no sockets, no Tokio tasks, no timers, no filesystem access, no token
storage, no clocks, no router state, no transport internals, no NetDB stores,
no tunnel pools, and no service runtimes. That is verifiable rather than
aspirational — a grep across `src/` and `tests/` for `std::time`, `Instant`,
`SystemTime`, `now(`, `std::fs`, `std::net`, `std::process`, `thread::`, and
`tokio` returns **no matches**. The only non-`crate::` import in the whole
crate is `std::collections::{BTreeMap, BTreeSet}` (`tunnel_request.rs:26`),
`std::fmt` (`proposal_leaseset_mode.rs:46`), and `thiserror::Error`.

It must not own: listeners, TLS, tokens, throttle tables, snapshots,
persistence, service runtimes, NetDB stores, tunnel pools, or any transport
internals.

It also must not own a **secret value**. The crate stores option *metadata* —
name, value type, sensitivity, applicability mask — and never the option's
value. That makes secret handling structural rather than filtered: there is no
key material in this crate to format, clone, or serialize. See
**Tunnel option metadata and secret classification** below.

## Module layout

Line counts are `wc -l` against current source (6 531 lines across 18 files).

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | ---: | --- | --- |
| `lib` | `lib.rs` | 106 | Module declarations, `#![forbid(unsafe_code)]`, the 15-group `pub use` public surface, the frozen provenance pins, and the Plan 286 dependency review | — |
| `proposal_leaseset_mode` | `proposal_leaseset_mode.rs` | 1 316 | Plan 334 executable form of the normative `EncryptLeaseSet` mapping: the ten exact Proposal strings resolved to eight protocol behaviors, the `OptionalLookup` / `LeaseSetClientAuths` pairing rules, and a bounded durable client-auth encoding | `EncryptLeaseSetMode`, `LeaseSetSecurityPlan`, `LeaseSetSecurityBehavior`, `LeaseSetClientAuthEntry`, `LeaseSetClientAuthScheme`, `LeaseSetAddressFlag`, `LeaseSetAddressFlags`, `EncryptLeaseSetModeError`, `LeaseSetClientAuthError`, `LeaseSetSecurityError` |
| `source_matrix` | `source_matrix.rs` | 1 098 | Plan 288 legacy source matrix (30 selectors + 6 services) and the Plan 322 canonical 43-row Proposal matrix, each row carrying owner, snapshot, ceilings, sensitivity, freshness, availability, and evidence id | `SourceRow`, `ProposalSourceRow`, `SourceAvailability`, `proposal_router_info_source_matrix`, `matrix_mirrors_inventories` |
| `tunnel_request` | `tunnel_request.rs` | 723 | Plan 289 closed TunnelManager request envelope: exact canonical `Action`/`Name`/`Type`/`NewName` rules, top-level option fields through an explicit alias table, scalar-only values, and the LeaseSet security block validator | `TunnelManagerRequest`, `TunnelRequestError`, `decode_tunnel_request`, `validate_lease_set_security_block` |
| `proposal_wire` | `proposal_wire.rs` | 625 | The canonical Proposal 170 wire inventory: 43 RouterInfo additions, 14 adopted base fields, 13 `SetConfig` keys, 75 TunnelManager fields, 22 integer ranges, and the 10 `EncryptLeaseSet` values | `ProposalRouterInfoField`, `ProposalValueType`, `ProposalTunnelValueType`, `ProposalTunnelIntegerRange`, `ProposalTunnelValueError`, `router_info_field`, `validate_proposal_tunnel_value` |
| `proposal_tunnel_matrix` | `proposal_tunnel_matrix.rs` | 433 | Generated field-by-tunnel-type ownership census over the complete canonical wire inventory (70 option fields × 12 types = 840 cells), distinguishing deep prerequisites and owner gaps | `ProposalTunnelCellDisposition`, `ProposalTunnelMatrixCell`, `proposal_tunnel_manager_matrix` |
| `tunnel_matrix` | `tunnel_matrix.rs` | 427 | Plan 292 exact type×option disposition matrix: 336 applicable cells, one disposition each, with Plan 293 determinations applied and the Plan 296/297 residuals closed | `CellDisposition`, `MatrixCell`, `MATRIX`, `find_cell`, `disposition_for` |
| `tunnel_options` | `tunnel_options.rs` | 383 | The 46-row typed option subset with value type, secret sensitivity, and a 12-bit applicability mask | `TunnelOption`, `TUNNEL_OPTIONS`, `SECRET_OPTIONS`, `OptionSensitivity`, `OptionValueType`, `find_option` |
| `tunnel` | `tunnel.rs` | 300 | TunnelManager domain: 7 actions, 12 types, 6 statuses, backend-availability predicates per plan scope, and name validation | `TunnelAction`, `TunnelType`, `TunnelStatus`, `TUNNEL_ACTIONS`, `TUNNEL_TYPES`, `validate_tunnel_name` |
| `router_info` | `router_info.rs` | 255 | The 30 historical normalized owner-routing selectors with declared return types. **Not** public wire spellings | `RouterInfoSelector`, `ROUTER_INFO_SELECTORS`, `ReturnType` |
| `jsonrpc` | `jsonrpc.rs` | 231 | Bounded JSON-RPC 2.0 envelope decode, batch split, and canonical success/error envelopes | `JsonRpcRequest`, `RequestId`, `JsonRpcErrorCode`, `JSONRPC_VERSION`, `success_envelope`, `error_envelope`, `split_body` |
| `address_book` | `address_book.rs` | 156 | 4 book types, 6 request fields, the 13-key `SetConfig` domain, and path-like / inert key classification | `BookType`, `AddressBookField`, `BOOK_TYPES`, `ADDRESS_BOOK_FIELDS`, `SET_CONFIG_KEYS`, `parse_set_config_key`, `is_path_like_config_key`, `is_inert_config_key` |
| `conformance` | `conformance.rs` | 113 | The machine-readable frozen public-contract inventory, derived from the frozen tables so counts cannot drift from prose | `ContractInventory`, `assert_frozen_counts`, `JSONRPC_ERROR_CODES` |
| `auth` | `auth.rs` | 107 | API version 1 authentication *vocabulary*: method/param/header spellings, the six standard error codes, and the bounded-behavior ceilings the daemon must enforce | `AuthErrorCode`, `AUTHENTICATE_METHOD`, `SUPPORTED_API_VERSION`, `TOKEN_BYTES`, `TOKEN_LIFETIME_SECS`, `MAX_LIVE_TOKENS`, `MAX_PRESENTED_TOKEN_LEN`, `TOKEN_HEADER`, `validate_presented_token` |
| `methods` | `methods.rs` | 90 | The exact 5-method inventory and the protected/public classification | `Method`, `METHODS` |
| `client_services` | `client_services.rs` | 73 | The exact 6-service inventory with constant / service-map classification | `ClientService`, `CLIENT_SERVICES` |
| `limits` | `limits.rs` | 66 | Every wire-level ceiling and the two length helpers that apply them | 21 `MAX_*` constants, `check_len`, `check_str` |
| `errors` | `errors.rs` | 29 | The typed contract error enum (no `anyhow` in this library crate) | `ContractError` |

## Public surface

`lib.rs` declares all 18 modules `pub` and then re-exports a curated set at
the crate root. The re-exports are exactly:

- `address_book` — `ADDRESS_BOOK_FIELDS`, `AddressBookField`, `BOOK_TYPES`,
  `BookType`, `SET_CONFIG_KEYS`
- `auth` — `AUTHENTICATE_METHOD`, `AuthErrorCode`, `MAX_LIVE_TOKENS`,
  `MAX_PRESENTED_TOKEN_LEN`, `TOKEN_BYTES`, `TOKEN_HEADER`,
  `TOKEN_LIFETIME_SECS`
- `client_services` — `CLIENT_SERVICES`, `ClientService`
- `conformance` — `ContractInventory`
- `errors` — `ContractError`
- `jsonrpc` — `JSONRPC_VERSION`, `JsonRpcErrorCode`, `JsonRpcRequest`,
  `RequestId`, `error_envelope`, `success_envelope`
- `limits` — all 21 `MAX_*` constants
- `methods` — `METHODS`, `Method`
- `proposal_leaseset_mode` — `EncryptLeaseSetMode`, `EncryptLeaseSetModeError`,
  `LEASESET_CLIENT_KEY_HEX_LEN`, `LEASESET_CLIENT_KEY_LEN`,
  `LeaseSetAddressFlag`, `LeaseSetAddressFlags`, `LeaseSetClientAuthEntry`,
  `LeaseSetClientAuthError`, `LeaseSetClientAuthScheme`,
  `LeaseSetSecurityBehavior`, `LeaseSetSecurityError`, `LeaseSetSecurityPlan`,
  `MAX_LEASESET_CLIENT_AUTHS`, `MAX_LEASESET_CLIENT_NAME_LEN`,
  `decode_encoded_client_auths`, `decode_lease_set_client_auths`,
  `encode_lease_set_client_auths`, `max_encoded_client_auths_bytes`,
  `resolve_encrypt_lease_set_mode`, `resolve_lease_set_security`
- `proposal_tunnel_matrix` — `ProposalTunnelCellDisposition`,
  `ProposalTunnelMatrixCell`, `proposal_tunnel_manager_matrix`
- `proposal_wire` — `BASE_ROUTER_INFO_FIELDS`,
  `PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS`, `PROPOSAL_ENCRYPT_LEASE_SET_VALUES`,
  `PROPOSAL_ROUTER_INFO_FIELDS`, `PROPOSAL_TUNNEL_INTEGER_RANGES`,
  `PROPOSAL_TUNNEL_MANAGER_FIELDS`, `PROPOSAL_TUNNEL_POLICY_INTEGER_RANGES`,
  `ProposalRouterInfoField`, `ProposalTunnelIntegerRange`,
  `ProposalTunnelValueError`, `ProposalTunnelValueType`, `ProposalValueType`,
  `proposal_tunnel_value_type`, `router_info_field`,
  `validate_proposal_tunnel_value`
- `router_info` — `ROUTER_INFO_SELECTORS`, `ReturnType`, `RouterInfoSelector`
- `source_matrix` — `CLIENT_SERVICES_SOURCE_MATRIX`, `ProposalSourceRow`,
  `ROUTER_INFO_SOURCE_MATRIX`, `SOURCE_MATRIX_NEUTRAL_COUNT`,
  `SourceAvailability`, `SourceRow`, `matrix_mirrors_inventories`,
  `proposal_router_info_source_matrix`, `selector_index`, `service_index`,
  `service_row`, `source_row`
- `tunnel` — `TUNNEL_ACTIONS`, `TUNNEL_TYPES`, `TunnelAction`, `TunnelStatus`,
  `TunnelType`
- `tunnel_options` — `OptionSensitivity`, `OptionValueType`, `SECRET_OPTIONS`,
  `TUNNEL_OPTIONS`, `TunnelOption`
- `tunnel_request` — `TunnelManagerRequest`, `TunnelRequestError`,
  `decode_tunnel_request`

Deliberately **module-public but not root-re-exported** (the daemon reaches
them through the module path): `conformance::assert_frozen_counts`,
`conformance::JSONRPC_ERROR_CODES`, `jsonrpc::split_body`,
`auth::SUPPORTED_API_VERSION`, `auth::PARAM_API`, `auth::PARAM_PASSWORD`,
`auth::PARAM_TOKEN`, `auth::validate_presented_token`,
`tunnel::validate_tunnel_name`, `tunnel_options::find_option`,
`tunnel_options::MASK_*`, `address_book::parse_set_config_key`,
`address_book::is_path_like_config_key`, `address_book::is_inert_config_key`,
`tunnel_matrix::MATRIX_CELLS`, `tunnel_matrix::find_cell`,
`tunnel_matrix::disposition_for`, `tunnel_request::validate_lease_set_security_block`.

## Key contracts

### JSON-RPC 2.0 envelope semantics (`jsonrpc.rs`)

`JSONRPC_VERSION` is `"2.0"` and must be present as a string on every envelope;
anything else is `Malformed` (`jsonrpc.rs:132-138`). The top-level member
vocabulary is **closed** to exactly `jsonrpc`, `method`, `params`, `id`; any
fifth key returns `Malformed` (`jsonrpc.rs:164-169`). A request object larger
than `MAX_PARAMS_KEYS + 2` (66) members is rejected as `OverBound` *before*
field access.

`params` is named-object only. An absent `params` decodes as an empty object; a
JSON array or any non-object value is `Malformed` — positional params are
rejected outright (`jsonrpc.rs:147-154`), which is stricter than base
JSON-RPC 2.0.

Id handling distinguishes three cases (`jsonrpc.rs:70-108`, `155-161`):

- **absent** `id` → `JsonRpcRequest::id == None` → `is_notification()` is
  `true` and no response body is produced;
- **explicit `null`** → `RequestId::Null`, a *real* request id that receives a
  response carrying a null id;
- string (bounded by `MAX_ID_STRING_LEN` = 128) or `i64` integer. A
  non-integral JSON number, a bool, an array, or an object is `Malformed`.

`success_envelope` emits `{jsonrpc, id, result}`; `error_envelope` emits
`{jsonrpc, id, error: {code, message}}`. **The error object has exactly two
members.** `error_envelope` never emits a `data` member, so the daemon cannot
attach structured detail through this helper without building the value itself.

The five standard codes (`JsonRpcErrorCode`) are `-32700` Parse error,
`-32600` Invalid Request, `-32601` Method not found, `-32602` Invalid params,
`-32603` Internal error. A typed capability-not-available condition maps to
`-32603` (`jsonrpc.rs:30`); the auth extension range `-32001`…`-32006` lives
in `auth` and is deliberately outside this enum.

`split_body` classifies a top-level value as batch (`Ok(true, elements)`,
including an empty array), single (`Ok(false, [body])`), or `Malformed`. An
empty array is valid JSON but must yield exactly one `Invalid Request`
response; the caller distinguishes it by the returned length being zero.
`split_body` does **not** apply `MAX_BATCH_ELEMENTS` — batch admission is the
daemon's, before this call.

### API version 1 authentication vocabulary (`auth.rs`)

Auth on the wire is a `Token` param beside the named params, with
`X-I2PControl-Token` recorded as the compatibility header spelling; the
daemon enforces that the two agree. `Authenticate` carries `API` (the version,
`SUPPORTED_API_VERSION = 1`) and `Password` (bounded by `MAX_PASSWORD_LEN` =
1 024).

The six standard codes are `-32001` Invalid password, `-32002` Missing token,
`-32003` Invalid token, `-32004` Expired token, `-32005` Missing API version,
`-32006` Unsupported API version.

The critical boundary: **this crate stores no tokens and reads no clock.**
`validate_presented_token` performs a length check only — empty is `Malformed`,
longer than `MAX_PRESENTED_TOKEN_LEN` (256) is `OverBound`
(`auth.rs:101-107`). There is no nonce, no timestamp, no expiry evaluation, no
signature, no constant-time comparison, and no token table anywhere in the
crate. The module doc assigns all of that to the daemon (`auth.rs:3-8`): token
generation, storage, expiry, the source-IP throttle, and constant-time
comparison. `TOKEN_LIFETIME_SECS` (86 400) and `MAX_LIVE_TOKENS` (1 024) are
*declared bounded behavior* the daemon must honor, not enforced values.

### Method, action, type, and selector inventories

- **Methods — exactly 5** (`methods.rs:13-19`): `Authenticate`,
  `RouterInfo`, `AddressBook`, `TunnelManager`, `ClientServicesInfo`.
  `Method::requires_token()` is `false` only for `Authenticate`. `parse` is
  case-sensitive and separates a case-only drift (`CaseMismatch`) from a
  genuinely unknown name (`UnknownLiteral`).
- **Tunnel actions — exactly 7** (`tunnel.rs:13-15`): `get`, `create`, `edit`,
  `delete`, `start`, `stop`, `restart`. `is_mutating()` is `false` only for
  `get`.
- **Tunnel types — exactly 12** (`tunnel.rs:18-31`), in canonical order that
  doubles as the applicability-mask bit position: `client`, `server`,
  `httpclient`, `socks`, `ircclient`, `ircserver`, `connectclient`, `socksirc`,
  `httpserver`, `httpbidirserver`, `streamrclient`, `streamrserver`. Three
  retained scope predicates record backend growth: `has_plan289_backend` (6),
  `has_plan290_backend` (10), `has_plan291_backend` (12).
- **Tunnel statuses — exactly 6**: `unsupported`, `stopped`, `starting`,
  `running`, `stopping`, `failed`.
- **RouterInfo selectors — exactly 30** (`router_info.rs:27-58`) with declared
  return types `String | Integer | Boolean | List | Map`. These normalized
  names are **internal owner-routing names, not public wire spellings**, and
  the canonical endpoint does not accept them (`router_info.rs:1-7`); the
  public vocabulary is `proposal_wire`.
- **Client services — exactly 6** (`client_services.rs:10`): `I2PTunnel`,
  `HTTPProxy`, `SOCKS`, `SAM`, `BOB`, `I2CP`. `is_service_map()` is `true` only
  for `I2PTunnel`; `is_constant()` is `true` only for `BOB`, which is a
  deliberate `enabled = false` capability result, not a missing-handler
  fallback (`client_services.rs:3-5`).
- **Address books — 4** (`private`, `local`, `router`, `published`); **request
  fields — 6** (`Type`, `Hostname`, `Destination`, `Delete`,
  `SetSubscriptions`, `SetConfig`); **`SetConfig` keys — 13**, aliased from
  `proposal_wire::PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS` so the two cannot drift.
  Eight keys are path-like administrative values that must stay confined
  beneath an owned root; `theme` is inert frontend-only metadata.
  `validate_tunnel_name` requires non-empty, ≤ 64 bytes, and no `/`, `\`, or
  control character.

`METHODS` intentionally does **not** absorb unrelated base methods: an unknown
method stays representable so the daemon can answer standard method-not-found.

### Tunnel option metadata and secret classification (`tunnel_options.rs`)

46 frozen rows, each `TunnelOption { name, value_type, sensitivity,
applies_mask }`. There is no unclassified option: every row carries an explicit
`OptionSensitivity`. Value types are `String`, `Integer`, `Boolean`.
Applicability is a 12-bit mask over `TUNNEL_TYPES` in canonical order, and
`applies_to(index)` returns `false` for any index ≥ 12 rather than indexing
past the mask.

**Four options are secret-bearing** (`SECRET_OPTIONS`): `proxy_password`,
`leaseset_password`, `leaseset_blinding_secret`, `leaseset_client_auth`.
`OptionSensitivity::Secret` carries the rule: never returned through
`rawConfig`, errors, logs, or `Debug` (`tunnel_options.rs:28`).

How secret options are typed and handled here:

- The crate stores **only the classification**, never a value. There is no
  secret-bearing type in this crate at all, so no secret can be formatted,
  cloned, or serialized. Redaction is the daemon's job and this crate gives it
  nothing to redact.
- `TunnelOption` does derive `Debug`, but that `Debug` formats a `&'static str`
  name plus three metadata fields — never an option value. It is not a
  secret-formatting path.
- Only two types in the whole crate derive `Serialize`: `ContractInventory`
  (a count snapshot) and `RequestId`. `SourceRow`, `ProposalSourceRow`,
  `TunnelManagerRequest`, and every LeaseSet type are **not** serializable, so
  `serde_json` provides no unrestricted serialization path for a secret.
- `LeaseSetClientAuthEntry` is the one type that holds key bytes. It is
  deliberately not `Clone`, and its hand-written `Debug` prints only the
  operator-metadata name and the literal `"<redacted>"` for the key
  (`proposal_leaseset_mode.rs:440-450`). Its bytes are decoded into the
  daemon's zeroizing `PskClientKey` / `AuthClientPublicKey` owners, which are
  the only layer with those types.
- Every `LeaseSetSecurityError` message names the *selected mode* and never a
  secret value; an in-crate test (`no_error_message_can_contain_a_secret`)
  enforces it.

`find_option` parses case-sensitively and returns `CaseMismatch` for a
case-only drift, mirroring the other literal parsers.

Two options carry Plan 334 corrections worth recording:

- `encrypt_lease_set` is typed **`String`, not `Boolean`**. Proposal 170 types
  it as one of ten enumeration strings, so the Plan 293 "Boolean"
  determination was wrong about the wire type (`tunnel_options.rs:317-325`).
- `leaseset_blinding_secret` is **retired as a duplicate spelling** but kept
  in the inventory so the closed envelope can refuse it by name. ELS2 defines
  exactly one lookup secret, so it must never be read as a second one.

### Wire-level ceilings (`limits.rs`)

Every bound is a hard compile-time maximum; the daemon may be stricter but
must never exceed these. All decoders validate length/count before allocation.

| Constant | Value | Governs |
| --- | ---: | --- |
| `MAX_METHOD_NAME_LEN` | 64 | JSON-RPC method name bytes |
| `MAX_SELECTOR_LEN` | 128 | RouterInfo selector bytes |
| `MAX_OPTION_NAME_LEN` | 128 | Option name bytes (aliases `MAX_SELECTOR_LEN`) |
| `MAX_STRING_LEN` | 4 096 | Generic string field bytes |
| `MAX_ID_STRING_LEN` | 128 | Request-id string bytes |
| `MAX_MAP_KEY_LEN` | 128 | Map key bytes |
| `MAX_PASSWORD_LEN` | 1 024 | `Authenticate` password bytes |
| `MAX_OPTION_VALUE_LEN` | 4 096 | Option value bytes |
| `MAX_HOSTNAME_LEN` | 255 | Hostname bytes (DNS-compatible) |
| `MAX_DESTINATION_LEN` | 4 096 | Full Destination text bytes |
| `MAX_SUBSCRIPTION_URL_LEN` | 2 048 | Subscription URL bytes |
| `MAX_TUNNEL_NAME_LEN` | 64 | Tunnel name bytes |
| `MAX_HTTP_BODY_BYTES` | 1 048 576 | HTTP request body (1 MiB) |
| `MAX_BATCH_ELEMENTS` | 32 | JSON-RPC batch elements |
| `MAX_INFLIGHT_REQUESTS` | 64 | Concurrent in-flight requests |
| `MAX_PARAMS_KEYS` | 64 | Named-param keys per request |
| `MAX_LIST_ITEMS` | 1 024 | Generic wire list items |
| `MAX_MAP_ENTRIES` | 256 | Generic wire map entries |
| `MAX_TUNNEL_DEFS` | 256 | Tunnel definitions per control generation |
| `MAX_OPTIONS_PER_TUNNEL` | 64 | Options on one tunnel definition |
| `MAX_SUBSCRIPTION_URLS` | 16 | Subscription URLs per request |

`check_len(len, max)` is a `const fn` returning `OverBound` past the ceiling;
`check_str` applies it to a `&str`. The body ceiling is enforced by the daemon
against `MAX_HTTP_BODY_BYTES` *before* parsing (`lib.rs:31-32`).

Module-local bounds live next to their owner: `MAX_LEASESET_CLIENT_AUTHS` (24),
`MAX_LEASESET_CLIENT_NAME_LEN` (64), `LEASESET_CLIENT_KEY_LEN` (32),
`LEASESET_CLIENT_KEY_HEX_LEN` (64). The 24-client ceiling is a
**control-surface** bound, not a protocol one: the ELS2 authorization block
permits 65 535 entries and `i2pr-netdb` accepts 255. The narrower number is a
consequence of every entry surviving a round trip through one bounded option
value, enforced by `max_encoded_client_auths_bytes` plus a worst-case test
(`proposal_leaseset_mode.rs:54-63`).

### Typed errors (`errors.rs`)

`ContractError` has exactly seven variants, all `Copy + Eq` with stable
`thiserror` messages:

| Variant | Message | Raised by |
| --- | --- | --- |
| `OverBound` | `value exceeds bound` | A value past a caller-visible ceiling |
| `UnknownLiteral` | `unknown literal` | Unknown method/action/type/selector/option |
| `CaseMismatch` | `literal case or spelling mismatch` | A known literal with wrong case |
| `Malformed` | `malformed value` | Bad envelope, params shape, or value encoding |
| `Duplicate` | `duplicate field` | Uniqueness violation |
| `Truncated` | `truncated input` | Truncated input |
| `SecretViolation` | `secret handling violation` | A secret-classified value presented where it must not appear |

`CaseMismatch` is deliberately distinct from `UnknownLiteral` so strictness is
observable in the error type rather than collapsed into one rejection.

The crate has three more typed error enums, each scoped to its domain:
`TunnelRequestError` (16 variants — `MissingField`, `UnexpectedField`,
`BadAction`, `BadName`, `BadType`, `BadOption`, `BadValue`, `ValueOverBound`,
`TooManyOptions`, `UnknownKey`, `DuplicateAlias`, `UnavailableOption`,
`RejectedOption`, `AllUnavailable`, `NothingToChange`), and
`EncryptLeaseSetModeError` / `LeaseSetClientAuthError` /
`LeaseSetSecurityError` (7 variants — `LegacyAesUnsupported`,
`LookupSecretNotPermitted`, `LookupSecretRequired`, `LookupSecretEmpty`,
`ClientAuthsNotPermitted`, `ClientAuthsRequired`, `ClientAuthsRejected`).
There is no `anyhow` in this crate.

### `proposal_wire.rs` — the canonical wire inventory

This is the default RouterInfo namespace and the authority for public
spellings, recording Proposal-declared JSON result shapes *independently from
current source availability* (a field can be valid wire vocabulary with no
truthful source yet). Contents: `PROPOSAL_ROUTER_INFO_FIELDS` (43 canonical
additions, each with a `ProposalValueType` and an optional existing
`RouterInfoSelector` adapter), `BASE_ROUTER_INFO_FIELDS` (14 adopted base API
fields, kept separate from the additions), `PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS`
(13), `PROPOSAL_TUNNEL_MANAGER_FIELDS` (75 — 5 envelope selectors plus 70
option fields), `PROPOSAL_TUNNEL_INTEGER_RANGES` (19) with
`PROPOSAL_TUNNEL_POLICY_INTEGER_RANGES` (3), and
`PROPOSAL_ENCRYPT_LEASE_SET_VALUES` (10). `ProposalValueType` has 8 variants
(`String`, `Integer`, `Boolean`, `Double`, `StringList`, `Object`,
`ObjectList`, `NestedObject`) — broader than the internal `ReturnType` because
the Proposal declares JSON shapes the internal vocabulary does not model.

### `proposal_tunnel_matrix.rs` — generated canonical ownership census

Unlike the frozen Plan 292 option inventory, this matrix is derived from the
complete canonical wire inventory: 70 option fields × 12 types = 840 cells,
generated by `proposal_tunnel_manager_matrix()` so inventory and coverage
cannot drift. The 5 envelope fields (`Name`, `Action`, `All`, `Type`,
`NewName`) are excluded as request-envelope rather than tunnel configuration.
`ProposalTunnelCellDisposition` has four variants — `Apply { owner }`,
`NotApplicable { reason }`, `DeepPrerequisite { plan, reason }`, and
`OwnerGap { owner_needed }`. The in-crate test asserts the `OwnerGap` set is
**empty**, that `SigType` and `EncType` are apply for all twelve types, and
that `MultiHoming` applies to `server`, `httpserver`, and `httpbidirserver`.
`OwnerGap` rows are the explicit "work remaining" marker and must be zero
before Plan 323 could close.

### `tunnel_matrix.rs` and `tunnel_options.rs` — the applied disposition

`tunnel_matrix` fixes 336 applicable cells (the 46-option × 12-type mask
intersection) with exactly one `CellDisposition` each: `Apply { owner }` names
the consuming struct/field/sweep (never a storage mirror), `NotApplicable
{ reason }` means the kind has no consuming layer, `ExplicitIncompatibility`
means the key names a capability i2pr deliberately does not provide, and
`CorrectivePending` is reserved for future correctives. The current
**test-pinned census is 281 apply / 37 not-applicable / 18 incompatible / 0
corrective-296 / 0 corrective-297** — Plan 334 moved 12 cells (the three
LeaseSet security options across the four publishing kinds) from incompatible
to apply once Plans 332/333 froze the real ELS2 owners.

### `source_matrix.rs` — availability, not serializer existence

`SourceAvailability` has four states, and the distinction is the whole point of
the module: `Available` (a live source answers, static constants and
configuration truth included), `PublishedGated { owner, owner_plan }` (typed
owner exists but must publish first; the whole request fails until it does),
`PermittedNeutral { reason }` (the only state that may emit a neutral value
without a live source, and each use needs a recorded protocol justification),
and `Unavailable { owner_plan, reason }` (no source; the whole request fails
explicitly, with no partial response and no fabricated zero/false/empty).

The legacy matrix has 30 `SourceRow` entries plus 6 client-service rows, each
recording key, return type, owner subsystem, snapshot method, cardinality and
encoded-byte ceilings, sensitivity/redaction rule, freshness semantics,
availability, and a fixture/test id. Its **test-pinned census is 27 available /
1 publish-gated (`router.hash`) / 1 unavailable (`news.feed`, by explicit
determination) / 1 permitted-neutral (`network.clock_skew`, justified from the
Proposal's null allowance)**, with `SOURCE_MATRIX_NEUTRAL_COUNT == 1`.

The canonical `proposal_router_info_source_matrix()` returns 43
`ProposalSourceRow` values in Proposal order. Its test asserts **zero
`Unavailable` rows** — Plan 339 closed the five per-family
`net.status.v6` / `net.error` / `net.error.v6` / `net.testing` /
`net.testing.v6` selectors (`PublishedGated { owner_plan: "339" }`) and Plan 340
closed the three transit selectors `net.total.transit.bytes`,
`net.bw.transit.15s`, and `net.tunnels.shareratio`
(`PublishedGated { owner_plan: "340" }`). Transit participation stays
**disabled** in production, so the honest baseline those owners report is
`0` / `0` / `0.0` — what a router relaying nothing truthfully reports. The
transit share ratio is the one key Proposal 170 does *not* mark
"(adopted from i2pd)", so i2pr defines it as observed transit bytes over
attested cumulative sent bytes, labelled as such, with a missing denominator
failing closed. `i2p.router.news` and the two total-byte keys remain
`PublishedGated { owner_plan: "322" }`. An `Unavailable` row must carry a
field-specific fail-closed wire test id; missing evidence is represented as
`None`, never replaced with an inventory-only check.

Base-compatibility separation is structural: the adopted base form selects with
`i2p.router.*` / `i2p.router.net.*` keys, while the Proposal-direct form uses
unprefixed `router.*` / `netdb.*` / `transport.*` / `tunnel.*` /
`addressbook.*` / `logs.*` / `news.*` / `network.*` keys. The two vocabularies
are disjoint by construction, so a base key can never select a Proposal
serializer, and unknown keys (including every `i2p.*` key) are rejected as
invalid params — stricter than i2pd's skip-and-log behavior, and deliberately
so (`source_matrix.rs:24-38`).

### `conformance.rs` — the frozen count inventory

`ContractInventory` exposes 16 count fields, each derived from the frozen
tables rather than prose: methods 5, RouterInfo selectors 30, Proposal
RouterInfo additions 43, base RouterInfo fields 14, client services 6, book
types 4, address-book fields 6, `SetConfig` keys 13, tunnel actions 7, tunnel
types 12, tunnel options 46, secret options 4, auth errors 6, JSON-RPC errors
5, tunnel statuses 6. `assert_frozen_counts` re-checks every one, plus
`PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS.len() == 13`, and is callable by the daemon
at startup for fail-fast contract alignment. `to_canonical_json` serializes
deterministically (struct field order is stable under `serde_json`).

### `address_book.rs` domain

Four administrative books, six request fields, and the 13-key `SetConfig`
domain. The behavior contract is Plan 294: a whole request validates before
any mutation, and mixed incompatible operation shapes fail rather than applying
partial changes (`address_book.rs:4-7`). Path-like config values are
administrative *logical* paths that must stay confined beneath an owned root —
never unrestricted filesystem selectors.

### `tunnel_request.rs` domain

The envelope uses the canonical Proposal names `Action`, `Name`, `Type`,
`NewName` beside `Token`, with option fields at the top level mapped through
an explicit canonical alias table. The previous lowercase envelope and nested
`options` map are not accepted on the default endpoint, and unknown top-level
keys are rejected — the envelope is closed. `Action` is required;
`Name` is required for every named operation including `get`; `Type` is
required for `create` and forbidden otherwise, because tunnel types are
immutable after creation and a rename travels as `NewName` on `edit` rather
than inventing an eighth action; `NewName` is allowed only on `edit`; and
`edit` requires `NewName` or at least one option or it is `NothingToChange`.
Option values must be scalars — null, array, and object are `BadValue`.

`TunnelManagerRequest` has a hand-written `Debug` that prints the action,
`all`, name, type, new name, and the **option keys only** — never a value
(`tunnel_request.rs:47-59`). That is the crate's structural secret-leak guard
at the decoded-request boundary. `validate_lease_set_security_block` also lives
here, so the whole block is validated as a unit before any mutation.

### `proposal_leaseset_mode.rs` domain

The largest module and the executable form of the normative mapping frozen in
`specs/references/proposal-170-encryptleaseset-mode-mapping.md`. It resolves
the ten exact Proposal strings (`disable`, `encrypted (aes)`, `blinded`,
`blinded with lookup password`, `encrypted (psk)`, `encrypted with lookup
password (psk)`, `encrypted with per-user key (psk)`, `encrypted with lookup
password and per-user key (psk)`, `encrypted with per-user key (dh)`,
`encrypted with lookup password and per-user key (dh)`) onto **eight** protocol
behaviors, and binds `OptionalLookup` and `LeaseSetClientAuths` to them.

The rules it owns: mode names are **identifiers** — never case-folded, trimmed,
or fuzzy-matched, because the Proposal spellings contain parentheses and
spaces; the two per-user spellings are recorded so an operator who typed the
longer form gets an answer they can reason about, without changing behavior;
`encrypted (aes)` is recognized and then refused *by name* as deprecated
(Proposal 121 was rejected and superseded by ChaCha20 encrypted LeaseSet2), so
the failure is actionable rather than mysterious; a supplied-but-unused
`OptionalLookup` or `LeaseSetClientAuths` is an **error, not a no-op**, and a
required one that is absent or empty is an error, so no mode silently
downgrades; and the whole block validates as a unit before any mutation and
re-validates on store load, so a generation written under an older rule fails
closed. The module owns no cryptography, no I/O, no persistence, and no router
state. `LeaseSetSecurityPlan` is the hand-off point; the daemon resolves it
into the real `i2pr_netdb::els2_auth` / `i2pr_client::encrypted_leaseset`
owners. The Proposal string parsing deliberately lives here rather than in
`i2pr-netdb`, because `els2_auth.rs` is the protocol owner and must not grow a
control-plane vocabulary dependency.

`LeaseSetAddressFlags` is a 4-state bitmask: `NONE` (0), `BLINDING_SECRET` (1),
`CLIENT_KEY` (2), `BLINDING_SECRET_AND_CLIENT_KEY` (3). Client authorizations
use a bounded durable encoding — `name:key` entries joined by newlines, with
the name charset excluding newline, carriage return, colon, and NUL — and
`max_encoded_client_auths_bytes` computes the worst case as a `const fn`.

## Dependencies

`Cargo.toml` declares exactly three production dependencies, all from the
workspace: `serde`, `serde_json`, and `thiserror`. There are **no
dev-dependencies**. The zero-`i2pr-*` production edge is enforced by
`scripts/check-dependency-direction.sh`, whose allowlist entry is literally
`"i2pr-i2pcontrol": set()` — an empty permitted-dependency set.

The dependency review is recorded in the crate doc comment
(`lib.rs:25-34`): `serde` (derive) provides the bounded structural codec,
`serde_json` (`default-features = false`, `std`) provides deterministic JSON
envelope handling for the JSON-RPC surface, and `thiserror` provides the typed
error enums the workspace `anyhow` policy requires. Untrusted input reaches
`serde_json` only through the bounded decoders in `jsonrpc`, with the body
ceiling applied by the daemon before parsing. `serde` and `serde_json` are
maintained, MIT/Apache-2.0, and pure-Rust with no `unsafe` in the enabled
feature set. The crate itself is `#![forbid(unsafe_code)]`.

## Tests

37 tests total, all local and deterministic. There are no `#[ignore]`-gated
external lanes in this crate and no fixture harness — the interop lanes belong
to `i2pr-daemon` and `i2pr-runtime`.

`cargo test -p i2pr-i2pcontrol` produces four targets:

| Target | Tests | What it proves |
| --- | ---: | --- |
| `src/lib.rs` unit tests | 13 | 12 in `proposal_leaseset_mode`, 1 in `proposal_tunnel_matrix` |
| `tests/contract.rs` (1 640 lines) | 16 | Inventories, parsing, bounds, and the two source matrices |
| `tests/tunnel_matrix.rs` (275 lines) | 7 | Matrix completeness and the exact disposition census |
| `tests/final_matrix.rs` (94 lines) | 1 | The Plan 295 final public-contract census in one place |

Doc-tests: 0.

The bounded negative paths are the point of the suite:
`max_and_max_plus_one_bounds` exercises every ceiling at max and max+1;
`unknown_and_case_methods_fail_typed` asserts `UnknownLiteral` and
`CaseMismatch` stay distinct; `secret_classification_is_total` asserts no
option is unclassified; `matrix_covers_every_applicable_cell` asserts the 336
cells equal the mask intersection and that no cell sits outside its mask;
`proposal_matrix_covers_every_canonical_option_for_all_twelve_types` asserts
the generated census is complete with an **empty** `OwnerGap` set;
`the_every_illegal_combination_is_rejected_before_any_mutation` and
`a_supplied_but_unused_secret_is_an_error_not_a_no_op` prove the LeaseSet block
fails closed as a unit; `mode_names_are_identifiers_and_are_never_normalized`
proves no case folding or trimming; `no_error_message_can_contain_a_secret`
proves no error string can leak a secret;
`client_entries_are_bounded_charset_checked_and_redacted` proves the charset,
count, and `<redacted>` `Debug` posture;
`the_durable_encoding_round_trips_and_fits_the_option_value_ceiling` proves the
worst-case encoding still fits the bounded option value;
`legacy_aes_is_recognized_then_refused_by_name` proves refusal is by name;
and `plan322_source_matrix_covers_all_canonical_additions_and_marks_gaps`
asserts all 43 canonical rows have a named owner, a non-zero byte ceiling, a
sensitivity and freshness string, and — for `Unavailable` rows — a
field-specific fail-closed wire test.

## Distinctive design choices

- Contract-first: the matrices record availability, owners, and dispositions
  before, and independently of, the backends that satisfy them.
- Availability is never derived from serializer existence — four distinct
  states, and a neutral value requires a recorded protocol justification.
- No neutral fabrications: an empty, zero, or map value requires a live
  authoritative source; an unowned row fails the whole request with no partial
  response.
- Strictness by default: unknown keys, case drift, non-null select values,
  positional params, and every base `i2p.*` key are invalid params, not skips —
  stricter than the reference router's skip-and-log, and deliberately so.
- The crate holds no secret value at all, only classification, so secret
  handling is structural rather than filtered; `serde` is not a serialization
  path for anything sensitive because only a count inventory and a request id
  derive `Serialize`.
- Refuse by name rather than degrade silently: a supplied-but-unused secret is
  an error, and a deprecated mode names its own deprecation in the message.
- Mode names are identifiers, never normalized — no case folding, trimming, or
  fuzzy matching, because the Proposal spellings contain parentheses and
  spaces.
- `CaseMismatch` is its own typed error, so case drift is observable rather
  than collapsed into a generic rejection.
- Frozen counts are derived from the frozen tables, never prose-counted, and
  re-asserted by `assert_frozen_counts` for daemon start-up fail-fast.
- Zero `i2pr-*` production edges: every value the crate needs (Proposal 170
  strings, ELS2 modes, secret-owner bindings) is modeled here precisely so the
  protocol owners never grow a control-plane dependency.

## Cross-references

- Architecture context: [overview.md](overview.md),
  [dependency-graph.md](dependency-graph.md), [tooling.md](tooling.md).
- Consumers and adaptors: [i2pr-daemon.md](i2pr-daemon.md) (listener, TLS,
  auth, composition), [i2pr-addressbook.md](i2pr-addressbook.md) (the canonical
  `SetConfig` owner), [i2pr-service-tunnels.md](i2pr-service-tunnels.md) (the
  shared `ServiceTunnelManager`), [i2pr-netdb.md](i2pr-netdb.md) (ELS2
  protocol owner), [i2pr-crypto.md](i2pr-crypto.md) (Red25519),
  [i2pr-client.md](i2pr-client.md) (encrypted LeaseSet2 publish/resolve).
- ADRs: [0028 — Proposal 170 control-plane
  authority](../adr/0028-i2pcontrol-proposal-170-control-plane.md),
  [0031 — one shared service-tunnel
  manager](../adr/0031-one-shared-service-tunnel-manager.md),
  [0005 — crypto dependency
  selection](../adr/0005-crypto-dependency-selection.md).
- Provenance: [proposal-170-manifest.md](../provenance/proposal-170-manifest.md)
  — the frozen pins the crate doc comment lists. Proposal 170 revision
  2026-05-20 (source SHA-256 `f13ae00b…`); base I2PControl API version 1
  documentation as of the 2026-07-10 update; eggstack/emissary fork master
  `6885a945d25a5ae61bc68191d27c5816bc3df4c9`; eepnet/emissary upstream master
  `9b43484a21d5a1291c4881cdae62a36c527f8c0f` (no `i2pcontrol` subtree); Java
  I2PControl PR 6 head `45bb593000408071dd376b78848fdc246dccd964`; i2pd openssl
  head `2d57d3f6783efbfebde6c5b03f29e6c231a84d6b` for adopted base behavior.
  All five Proposal/base pins and the i2pd pin match the manifest.
- Reference specifications:
  [encryptleaseset mode mapping](../../specs/references/proposal-170-encryptleaseset-mode-mapping.md)
  (the normative reading `proposal_leaseset_mode.rs` implements),
  [network status / error / testing](../../specs/references/proposal-170-network-status-error-testing.md),
  [outbound secret owner](../../specs/references/proposal-170-outbound-secret-owner.md),
  [transit volume and share](../../specs/references/proposal-170-transit-volume-and-share.md).
- Support and conformance: [support.toml](../../specs/support.toml) and
  [CONFORMANCE.md](../../specs/CONFORMANCE.md) §Proposal 170 support model
  (seven dimensions). **No Proposal 170 / I2PControl surface is advertised**:
  all fifteen `prop170.*` and `control.i2pcontrol-*` rows carry
  `advertised = false`.
- Plans: [registry.md](../../plans/registry.md) and the
  [i2pcontrol-proposal-170
  roadmap](../../plans/subsystems/i2pcontrol-proposal-170-roadmap.md). Closure
  records under
  [`plans/closure/i2pcontrol-proposal-170/`](../../plans/closure/i2pcontrol-proposal-170/):
  [286](../../plans/closure/i2pcontrol-proposal-170/286-status.md) (foundation),
  [288](../../plans/closure/i2pcontrol-proposal-170/288-status.md) (inspection
  plane), [289](../../plans/closure/i2pcontrol-proposal-170/289-status.md)
  (TunnelManager control state), [290](../../plans/closure/i2pcontrol-proposal-170/290-status.md)
  and [291](../../plans/closure/i2pcontrol-proposal-170/291-status.md)
  (composed and Streamr families), [292](../../plans/closure/i2pcontrol-proposal-170/292-status.md)
  and [293](../../plans/closure/i2pcontrol-proposal-170/293-status.md) (option
  matrix and deep determinations), [294](../../plans/closure/i2pcontrol-proposal-170/294-status.md)
  (AddressBook), [295](../../plans/closure/i2pcontrol-proposal-170/295-status.md)
  (source completion), [319](../../plans/closure/i2pcontrol-proposal-170/319-status.md)
  –[324](../../plans/closure/i2pcontrol-proposal-170/324-status.md) (canonical
  continuation), [329](../../plans/closure/i2pcontrol-proposal-170/329-status.md)
  –[334](../../plans/closure/i2pcontrol-proposal-170/334-status.md) (Red25519 /
  ELS2 successor), [337](../../plans/closure/i2pcontrol-proposal-170/337-status.md)
  and [338](../../plans/closure/i2pcontrol-proposal-170/338-status.md)
  (corrective passes that reclosed 334), [339](../../plans/closure/i2pcontrol-proposal-170/339-status.md)
  and [340](../../plans/closure/i2pcontrol-proposal-170/340-status.md)
  (per-family and transit owners that closed 322 Group A), and
  [335](../../plans/closure/i2pcontrol-proposal-170/335-status.md) (blocked).
- Boundary enforcement: [check-dependency-direction.sh](../../scripts/check-dependency-direction.sh)
  (the empty allowlist set) and
  [check-i2pcontrol-acceptance-evidence.sh](../../scripts/check-i2pcontrol-acceptance-evidence.sh).
