//! Plan 286 Proposal 170 / I2PControl runtime-neutral contract foundation.
//!
//! This crate owns the bounded wire/domain contract for the Proposal 170
//! workstream: JSON-RPC 2.0 envelope semantics, API version 1 authentication
//! vocabulary, the exact method/action/type/selector inventories, tunnel
//! option metadata with secret classification, wire-level ceilings, and the
//! machine-readable public contract inventory.
//!
//! It owns no sockets, no Tokio tasks, no timers, no filesystem access, no
//! token storage, no clocks, no router state, no transport internals, no
//! NetDB stores, no tunnel pools, and no service runtimes. `i2pr-daemon`
//! adapts this contract to router state; later capability plans own the
//! listeners, persistence, and backends.
//!
//! Frozen references (see `docs/provenance/proposal-170-manifest.md`):
//! - Proposal 170 revision 2026-05-20.
//! - Base I2PControl API version 1 documentation as of the 2026-07-10 update.
//! - eggstack/emissary fork master
//!   `6885a945d25a5ae61bc68191d27c5816bc3df4c9`.
//! - eepnet/emissary upstream master
//!   `9b43484a21d5a1291c4881cdae62a36c527f8c0f` (no `i2pcontrol` subtree).
//! - Java I2PControl PR 6 head `45bb593000408071dd376b78848fdc246dccd964`.
//! - i2pd openssl head `2d57d3f6783efbfebde6c5b03f29e6c231a84d6b`.
//!
//! Dependency review (Plan 286 §B): `serde` (derive) provides the bounded
//! structural codec shared with the rest of the workspace; `serde_json`
//! (`default-features = false`, `std`) provides deterministic JSON envelope
//! parsing/serialization for the JSON-RPC 2.0 surface. Both are maintained
//! (`serde-rs`), MIT/Apache-2.0 licensed, pure-Rust with no `unsafe` exposure
//! in the enabled feature set, and process untrusted input only through the
//! bounded decoders in [`jsonrpc`] (body ceiling enforced by the daemon via
//! [`limits::MAX_HTTP_BODY_BYTES`] before parsing). `thiserror` provides the
//! typed error enums required by the workspace `anyhow` policy. No other
//! external dependencies are introduced.

#![forbid(unsafe_code)]

pub mod address_book;
pub mod auth;
pub mod client_services;
pub mod conformance;
pub mod errors;
pub mod jsonrpc;
pub mod limits;
pub mod methods;
pub mod router_info;
pub mod tunnel;
pub mod tunnel_options;

pub use address_book::{
    ADDRESS_BOOK_FIELDS, AddressBookField, BOOK_TYPES, BookType, SET_CONFIG_KEYS,
};
pub use auth::{
    AUTHENTICATE_METHOD, AuthErrorCode, MAX_LIVE_TOKENS, MAX_PRESENTED_TOKEN_LEN, TOKEN_BYTES,
    TOKEN_LIFETIME_SECS,
};
pub use client_services::{CLIENT_SERVICES, ClientService};
pub use conformance::ContractInventory;
pub use errors::ContractError;
pub use jsonrpc::{JSONRPC_VERSION, JsonRpcErrorCode, JsonRpcRequest, RequestId};
pub use limits::{
    MAX_BATCH_ELEMENTS, MAX_DESTINATION_LEN, MAX_HOSTNAME_LEN, MAX_HTTP_BODY_BYTES,
    MAX_ID_STRING_LEN, MAX_INFLIGHT_REQUESTS, MAX_LIST_ITEMS, MAX_MAP_ENTRIES, MAX_MAP_KEY_LEN,
    MAX_METHOD_NAME_LEN, MAX_OPTION_NAME_LEN, MAX_OPTION_VALUE_LEN, MAX_OPTIONS_PER_TUNNEL,
    MAX_PARAMS_KEYS, MAX_PASSWORD_LEN, MAX_SELECTOR_LEN, MAX_STRING_LEN, MAX_SUBSCRIPTION_URL_LEN,
    MAX_SUBSCRIPTION_URLS, MAX_TUNNEL_DEFS, MAX_TUNNEL_NAME_LEN,
};
pub use methods::{METHODS, Method};
pub use router_info::{ROUTER_INFO_SELECTORS, ReturnType, RouterInfoSelector};
pub use tunnel::{TUNNEL_ACTIONS, TUNNEL_TYPES, TunnelAction, TunnelStatus, TunnelType};
pub use tunnel_options::{
    OptionSensitivity, OptionValueType, SECRET_OPTIONS, TUNNEL_OPTIONS, TunnelOption,
};
