//! Wire-level string/list/map/request ceilings shared by later daemon code.
//!
//! Every bound is a hard compile-time maximum. The daemon may enforce a
//! stricter operational ceiling but must never exceed these values. All
//! decoders validate lengths/counts before allocation.

/// Maximum JSON-RPC method name length in bytes.
pub const MAX_METHOD_NAME_LEN: usize = 64;
/// Maximum RouterInfo selector / tunnel option name length in bytes.
pub const MAX_SELECTOR_LEN: usize = 128;
/// Maximum option name length in bytes (aliases [`MAX_SELECTOR_LEN`]).
pub const MAX_OPTION_NAME_LEN: usize = 128;
/// Maximum generic string field length in bytes.
pub const MAX_STRING_LEN: usize = 4_096;
/// Maximum request-id string length in bytes.
pub const MAX_ID_STRING_LEN: usize = 128;
/// Maximum map key length in bytes.
pub const MAX_MAP_KEY_LEN: usize = 128;
/// Maximum password length in bytes presented to `Authenticate`.
pub const MAX_PASSWORD_LEN: usize = 1_024;
/// Maximum option value length in bytes.
pub const MAX_OPTION_VALUE_LEN: usize = 4_096;
/// Maximum hostname length in bytes (DNS-compatible ceiling).
pub const MAX_HOSTNAME_LEN: usize = 255;
/// Maximum full Destination text length in bytes.
pub const MAX_DESTINATION_LEN: usize = 4_096;
/// Maximum subscription URL length in bytes.
pub const MAX_SUBSCRIPTION_URL_LEN: usize = 2_048;
/// Maximum tunnel name length in bytes.
pub const MAX_TUNNEL_NAME_LEN: usize = 64;

/// Maximum HTTP request body in bytes (Plan 287 initial ceiling: 1 MiB).
pub const MAX_HTTP_BODY_BYTES: usize = 1_048_576;
/// Maximum elements per JSON-RPC batch (Plan 287 initial ceiling).
pub const MAX_BATCH_ELEMENTS: usize = 32;
/// Maximum concurrent in-flight I2PControl requests (Plan 287 ceiling).
pub const MAX_INFLIGHT_REQUESTS: usize = 64;
/// Maximum named-params keys per request object.
pub const MAX_PARAMS_KEYS: usize = 64;
/// Maximum items in a generic wire list.
pub const MAX_LIST_ITEMS: usize = 1_024;
/// Maximum entries in a generic wire map.
pub const MAX_MAP_ENTRIES: usize = 256;
/// Maximum tunnel definitions in one control generation.
pub const MAX_TUNNEL_DEFS: usize = 256;
/// Maximum options carried on one tunnel definition.
pub const MAX_OPTIONS_PER_TUNNEL: usize = 64;
/// Maximum address-book subscription URLs in one request.
pub const MAX_SUBSCRIPTION_URLS: usize = 16;

use crate::errors::ContractError;

/// Validates `len` against `max`, returning [`ContractError::OverBound`]
/// when the value exceeds the ceiling. All arithmetic is checked by the
/// caller; this helper only compares.
pub const fn check_len(len: usize, max: usize) -> Result<(), ContractError> {
    if len > max {
        return Err(ContractError::OverBound);
    }
    Ok(())
}

/// Validates a string field against a byte ceiling.
pub fn check_str(value: &str, max: usize) -> Result<(), ContractError> {
    check_len(value.len(), max)
}
