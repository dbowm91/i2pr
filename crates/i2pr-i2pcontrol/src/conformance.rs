//! Machine-readable public contract inventory.
//!
//! The inventory is derived from the frozen module tables so counts cannot
//! drift from prose: every cardinality assertion in tests reads these
//! constructors.

use serde::{Deserialize, Serialize};

use crate::address_book::{ADDRESS_BOOK_FIELDS, BOOK_TYPES, SET_CONFIG_KEYS};
use crate::auth::AuthErrorCode;
use crate::client_services::CLIENT_SERVICES;
use crate::jsonrpc::JsonRpcErrorCode;
use crate::methods::METHODS;
use crate::router_info::ROUTER_INFO_SELECTORS;
use crate::tunnel::{TUNNEL_ACTIONS, TUNNEL_TYPES, TunnelStatus};
use crate::tunnel_options::{SECRET_OPTIONS, TUNNEL_OPTIONS};

/// Machine-readable snapshot of the frozen Plan 286 public contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContractInventory {
    /// Frozen method count (5).
    pub methods: usize,
    /// Frozen RouterInfo selector count (30).
    pub router_info_selectors: usize,
    /// Frozen ClientServicesInfo selector count (6).
    pub client_services: usize,
    /// Frozen address-book type count (4).
    pub book_types: usize,
    /// Frozen address-book field count (6).
    pub address_book_fields: usize,
    /// Frozen `SetConfig` key count (13).
    pub set_config_keys: usize,
    /// Frozen tunnel-action count (7).
    pub tunnel_actions: usize,
    /// Frozen tunnel-type count (12).
    pub tunnel_types: usize,
    /// Frozen tunnel-option count (46).
    pub tunnel_options: usize,
    /// Frozen secret-classified option count (4).
    pub secret_options: usize,
    /// Frozen auth error count (6).
    pub auth_errors: usize,
    /// Frozen JSON-RPC error count (5).
    pub jsonrpc_errors: usize,
    /// Frozen tunnel-status count (6).
    pub tunnel_statuses: usize,
}

impl ContractInventory {
    /// Builds the inventory from the frozen tables.
    pub fn current() -> Self {
        Self {
            methods: METHODS.len(),
            router_info_selectors: ROUTER_INFO_SELECTORS.len(),
            client_services: CLIENT_SERVICES.len(),
            book_types: BOOK_TYPES.len(),
            address_book_fields: ADDRESS_BOOK_FIELDS.len(),
            set_config_keys: SET_CONFIG_KEYS.len(),
            tunnel_actions: TUNNEL_ACTIONS.len(),
            tunnel_types: TUNNEL_TYPES.len(),
            tunnel_options: TUNNEL_OPTIONS.len(),
            secret_options: SECRET_OPTIONS.len(),
            auth_errors: AuthErrorCode::ALL.len(),
            jsonrpc_errors: 5,
            tunnel_statuses: TunnelStatus::ALL.len(),
        }
    }

    /// Serializes the inventory deterministically (sorted keys not needed:
    /// struct field order is stable under `serde_json`).
    pub fn to_canonical_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("inventory serializes")
    }
}

/// Asserts the inventory matches the frozen Plan 286 counts. Called by
/// tests and by the daemon at startup for fail-fast contract alignment.
pub fn assert_frozen_counts(inventory: &ContractInventory) -> bool {
    inventory.methods == 5
        && inventory.router_info_selectors == 30
        && inventory.client_services == 6
        && inventory.book_types == 4
        && inventory.address_book_fields == 6
        && inventory.set_config_keys == 13
        && inventory.tunnel_actions == 7
        && inventory.tunnel_types == 12
        && inventory.tunnel_options == 46
        && inventory.secret_options == 4
        && inventory.auth_errors == 6
        && inventory.jsonrpc_errors == 5
        && inventory.tunnel_statuses == 6
}

/// Lists the five JSON-RPC error codes in canonical order.
pub const JSONRPC_ERROR_CODES: [JsonRpcErrorCode; 5] = [
    JsonRpcErrorCode::ParseError,
    JsonRpcErrorCode::InvalidRequest,
    JsonRpcErrorCode::MethodNotFound,
    JsonRpcErrorCode::InvalidParams,
    JsonRpcErrorCode::InternalError,
];
