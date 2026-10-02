# `i2pr-i2pcontrol` — Proposal 170 runtime-neutral contract

Path: `crates/i2pr-i2pcontrol`. Runtime-neutral Proposal 170 / I2PControl
wire and domain contract: exact method/action/type/selector inventories,
bounded JSON-RPC 2.0 semantics, API version 1 authentication vocabulary,
tunnel option metadata with secret classification, wire-level ceilings,
the machine-readable source matrix, and the frozen public inventory. No
I/O, no sockets, no Tokio, no timers, no filesystem, no token storage, no
clocks, no router state. `#![forbid(unsafe_code)]`.

## Purpose

Owns what is frozen: every wire spelling the control plane accepts or
emits, every ceiling it enforces, and — since Plan 288 — the availability
state of every readable selector. `i2pr-daemon` adapts this contract to
live router state; the crate itself never touches a live owner.

It must not own: listeners, TLS, tokens, throttle tables, snapshots,
persistence, service runtimes, NetDB stores, tunnel pools, or any
transport internals.

## Module layout

| Module | File | Responsibility | Key public types |
| --- | --- | --- | --- |
| `methods` | `methods.rs` | Exact 5-method inventory + token-gating classification | `Method`, `METHODS` |
| `router_info` | `router_info.rs` | Exact 30-selector inventory with return types | `RouterInfoSelector`, `ROUTER_INFO_SELECTORS`, `ReturnType` |
| `client_services` | `client_services.rs` | Exact 6-service inventory + constant/map classification | `ClientService`, `CLIENT_SERVICES` |
| `address_book` | `address_book.rs` | 4 book types, 6 fields, 13 `SetConfig` keys + path/inert classification | `BookType`, `AddressBookField`, `BOOK_TYPES`, `ADDRESS_BOOK_FIELDS`, `SET_CONFIG_KEYS` |
| `tunnel` | `tunnel.rs` | 7 actions, 12 types (all with Plan 291 backends), 6 statuses, name validation | `TunnelAction`, `TunnelType`, `TunnelStatus`, `TUNNEL_ACTIONS`, `TUNNEL_TYPES` |
| `tunnel_options` | `tunnel_options.rs` | 46 options with value types, sensitivity (4 secret), plan ownership | `TunnelOption`, `TUNNEL_OPTIONS`, `SECRET_OPTIONS`, `OptionSensitivity`, `OptionValueType` |
| `tunnel_request` | `tunnel_request.rs` | Plan 289: closed TunnelManager request envelope (action/name/type/new_name/options rules, frozen-universe option keys, scalar-only values) | `TunnelManagerRequest`, `TunnelRequestError`, `decode_tunnel_request` |
| `auth` | `auth.rs` | API-1 vocabulary: ceilings (32 B / 1 day / 1024 / 256) + six error codes | `AuthErrorCode`, `TOKEN_BYTES`, `TOKEN_LIFETIME_SECS`, `MAX_LIVE_TOKENS`, `MAX_PRESENTED_TOKEN_LEN` |
| `jsonrpc` | `jsonrpc.rs` | Bounded envelope decode, batch split, canonical envelopes | `JsonRpcRequest`, `RequestId`, `JsonRpcErrorCode`, `success_envelope`, `error_envelope` |
| `limits` | `limits.rs` | Every wire ceiling (body 1 MiB, batch 32, in-flight 64, names, maps, tunnels) | `MAX_*` constants, `check_len`, `check_str` |
| `errors` | `errors.rs` | Typed contract errors (no `anyhow`) | `ContractError` |
| `source_matrix` | `source_matrix.rs` | Plan 288: one machine-readable row per selector/service with owner, ceilings, sensitivity, freshness, availability, test id | `SourceRow`, `SourceAvailability`, `ROUTER_INFO_SOURCE_MATRIX`, `CLIENT_SERVICES_SOURCE_MATRIX`, `matrix_mirrors_inventories` |
| `conformance` | `conformance.rs` | Frozen Plan 286 inventory counts + canonical JSON | `ContractInventory`, `assert_frozen_counts` |

## Public surface

Re-exported from `lib.rs`: method/selector/service/action/type/status
inventories and parsers, auth vocabulary, JSON-RPC codecs and envelopes,
limits, errors, the source matrix and its index helpers, the TunnelManager
request envelope, and the conformance inventory.

## Key contracts

- Frozen counts (Plan 286, machine-checked): methods 5, RouterInfo 30,
  services 6, books 4, fields 6, `SetConfig` keys 13, actions 7, types 12,
  options 46, secret 4, auth errors 6, JSON-RPC errors 5, statuses 6.
- Source-matrix census (Plan 288, machine-checked): 5 available, 16
  publish-gated (1 in-plan `router.hash`, 15 residual-295), 9 unavailable
  (6 for Plan 294, 3 for Plan 295), 0 permitted-neutral.
- Availability is never derived from serializer existence. Unavailable
  rows fail whole-request; gated rows fail until their owner publishes;
  neutral rows require explicit protocol justification (none recorded).
- The Proposal-direct selector vocabulary (`router.*`, `netdb.*`, …) is
  structurally disjoint from the adopted base `i2p.*` vocabulary
  (verified read-only against pinned i2pd `2d57d3f6`), so base keys can
  never select Proposal serializers.

## Errors

`ContractError::{OverBound, UnknownLiteral, CaseMismatch, Malformed,
Duplicate, Truncated, SecretViolation}`. Case mismatch is distinct from
unknown literal so strictness is observable.

## Dependencies

`serde` (derive), `serde_json` (`default-features = false`, `std`),
`thiserror`. No `i2pr-*` production edge; only `i2pr-daemon` may depend
on this crate (enforced by `scripts/check-dependency-direction.sh`).

## Tests

`tests/contract.rs`: exact inventories, deterministic parsing, typed
literal failures, max/max+1 bounds, secret classification, matrix
mirror + availability census, Plan 289 envelope rules, Plan 290
ten-backend count, Plan 291 twelve-backend count.

## Distinctive design choices

- Contract-first: the matrix records availability before backends exist.
- Strictness by default: unknown keys, case drift, non-null select
  values, and base-compat keys are all invalid params, not skips.
- No neutral fabrications: empty/zero/map values require a live source.
- Counts serialize as one-element lists (the frozen shape is `List`).
- Base behavior is read for parity, never imported (see
  `docs/provenance/proposal-170-manifest.md`).

## Cross-references

- ADR 0028 (Proposal 170 control-plane authority).
- `plans/subsystems/i2pcontrol-proposal-170-roadmap.md` (Plans 286–295).
- `plans/closure/i2pcontrol-proposal-170/286-status.md` (foundation).
- `plans/closure/i2pcontrol-proposal-170/288-status.md` (inspection).
- `plans/closure/i2pcontrol-proposal-170/289-status.md` (TunnelManager control).
- `plans/closure/i2pcontrol-proposal-170/290-status.md` (composed families).
- `plans/closure/i2pcontrol-proposal-170/291-status.md` (repliable datagram + Streamr).
- `docs/provenance/proposal-170-manifest.md` (frozen pins, R/B/X).
- `specs/CONFORMANCE.md` §Proposal 170 support model (7 dimensions).
- `docs/architecture/i2pr-daemon.md` (`i2pcontrol.rs`,
  `i2pcontrol_inspection.rs` adapt this contract).
