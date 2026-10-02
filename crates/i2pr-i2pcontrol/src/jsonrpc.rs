//! JSON-RPC 2.0 envelope domain: request ids, single/batch shapes,
//! success/error envelopes, strict named params.
//!
//! The daemon owns framing (HTTP/TLS/body admission); this module owns exact
//! envelope validation and canonical error mapping. Positional (array) params
//! are rejected: the I2PControl surface uses named-object params only.

use serde::{Deserialize, Serialize};

use crate::errors::ContractError;
use crate::limits::{
    MAX_ID_STRING_LEN, MAX_METHOD_NAME_LEN, MAX_PARAMS_KEYS, check_len, check_str,
};

/// Exact JSON-RPC version string required on every envelope.
pub const JSONRPC_VERSION: &str = "2.0";

/// Standard JSON-RPC 2.0 error codes plus the I2PControl auth extension
/// range (the `-32001/nginx-32006` codes live in [`crate::auth`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JsonRpcErrorCode {
    /// `-32700` malformed JSON.
    ParseError,
    /// `-32600` valid JSON but not a valid request object.
    InvalidRequest,
    /// `-32601` unknown method.
    MethodNotFound,
    /// `-32602` bad params.
    InvalidParams,
    /// `-32603` internal failure (typed capability-not-available maps here).
    InternalError,
}

impl JsonRpcErrorCode {
    /// Numeric wire code.
    pub const fn code(self) -> i64 {
        match self {
            Self::ParseError => -32_700,
            Self::InvalidRequest => -32_600,
            Self::MethodNotFound => -32_601,
            Self::InvalidParams => -32_602,
            Self::InternalError => -32_603,
        }
    }

    /// Canonical message for the code.
    pub const fn message(self) -> &'static str {
        match self {
            Self::ParseError => "Parse error",
            Self::InvalidRequest => "Invalid Request",
            Self::MethodNotFound => "Method not found",
            Self::InvalidParams => "Invalid params",
            Self::InternalError => "Internal error",
        }
    }

    /// Parses an exact numeric code.
    pub const fn from_code(code: i64) -> Option<Self> {
        match code {
            -32_700 => Some(Self::ParseError),
            -32_600 => Some(Self::InvalidRequest),
            -32_601 => Some(Self::MethodNotFound),
            -32_602 => Some(Self::InvalidParams),
            -32_603 => Some(Self::InternalError),
            _ => None,
        }
    }
}

/// JSON-RPC request id: string, integer, or explicit null.
///
/// An absent id marks a notification (no response). An explicit null is a
/// real request id and receives a response carrying a null id.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// String id, bounded by [`MAX_ID_STRING_LEN`].
    String(String),
    /// Integer id.
    Integer(i64),
    /// Explicit null id.
    Null,
}

impl RequestId {
    /// Validates a string id against the wire ceiling.
    pub fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::String(value) => check_str(value, MAX_ID_STRING_LEN),
            Self::Integer(_) | Self::Null => Ok(()),
        }
    }

    /// Parses a raw `serde_json` id value with strict typing.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, ContractError> {
        match value {
            serde_json::Value::String(text) => {
                check_str(text, MAX_ID_STRING_LEN)?;
                Ok(Self::String(text.clone()))
            }
            serde_json::Value::Number(number) => number
                .as_i64()
                .map(Self::Integer)
                .ok_or(ContractError::Malformed),
            serde_json::Value::Null => Ok(Self::Null),
            _ => Err(ContractError::Malformed),
        }
    }
}

/// A decoded single JSON-RPC request with strict named (object) params.
///
/// `id` is `None` for notifications. `params` is the validated named-param
/// object; positional arrays are rejected at decode time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonRpcRequest {
    /// Request id, or `None` for notifications.
    pub id: Option<RequestId>,
    /// Exact method name (validated against [`MAX_METHOD_NAME_LEN`]).
    pub method: String,
    /// Named params object (key count bounded by [`MAX_PARAMS_KEYS`]).
    pub params: serde_json::Map<String, serde_json::Value>,
}

impl JsonRpcRequest {
    /// Decodes one request object with exact-consumption, strict-shape rules.
    pub fn decode(value: &serde_json::Value) -> Result<Self, ContractError> {
        let object = value.as_object().ok_or(ContractError::Malformed)?;
        if object.len() > MAX_PARAMS_KEYS + 2 {
            return Err(ContractError::OverBound);
        }
        let version = object
            .get("jsonrpc")
            .and_then(serde_json::Value::as_str)
            .ok_or(ContractError::Malformed)?;
        if version != JSONRPC_VERSION {
            return Err(ContractError::Malformed);
        }
        let method = object
            .get("method")
            .and_then(serde_json::Value::as_str)
            .ok_or(ContractError::Malformed)?;
        check_str(method, MAX_METHOD_NAME_LEN)?;
        if method.is_empty() {
            return Err(ContractError::Malformed);
        }
        let params = match object.get("params") {
            None => serde_json::Map::new(),
            Some(serde_json::Value::Object(map)) => {
                check_len(map.len(), MAX_PARAMS_KEYS)?;
                map.clone()
            }
            Some(_) => return Err(ContractError::Malformed),
        };
        let id = match object.get("id") {
            None => None,
            Some(raw) => Some(RequestId::from_json(raw)?),
        };
        if let Some(id) = &id {
            id.validate()?;
        }
        // Reject duplicate-prone unknown top-level members beyond the exact
        // envelope vocabulary (`jsonrpc`, `method`, `params`, `id`).
        for key in object.keys() {
            match key.as_str() {
                "jsonrpc" | "method" | "params" | "id" => {}
                _ => return Err(ContractError::Malformed),
            }
        }
        Ok(Self {
            id,
            method: method.to_string(),
            params,
        })
    }

    /// Whether this request is a notification (no response body).
    pub const fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// Builds a canonical success envelope preserving the request id shape.
pub fn success_envelope(id: Option<&RequestId>, result: serde_json::Value) -> serde_json::Value {
    let id_value = match id {
        None => serde_json::Value::Null,
        Some(RequestId::String(text)) => serde_json::Value::String(text.clone()),
        Some(RequestId::Integer(number)) => serde_json::Value::from(*number),
        Some(RequestId::Null) => serde_json::Value::Null,
    };
    serde_json::json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id_value,
        "result": result,
    })
}

/// Builds a canonical error envelope for a known request id (or null).
pub fn error_envelope(id: Option<&RequestId>, code: i64, message: &str) -> serde_json::Value {
    let id_value = match id {
        None => serde_json::Value::Null,
        Some(RequestId::String(text)) => serde_json::Value::String(text.clone()),
        Some(RequestId::Integer(number)) => serde_json::Value::from(*number),
        Some(RequestId::Null) => serde_json::Value::Null,
    };
    serde_json::json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id_value,
        "error": {
            "code": code,
            "message": message,
        },
    })
}

/// Classifies a top-level body value as single, batch, or empty-batch.
///
/// Returns `Ok(true)` for batches (arrays, possibly empty) and `Ok(false)`
/// for single request objects. Non-object/non-array bodies are malformed.
/// Empty batches are valid JSON but must yield exactly one
/// `Invalid Request` response; the caller distinguishes them via the
/// returned array length (zero).
pub fn split_body(
    body: &serde_json::Value,
) -> Result<(bool, Vec<&serde_json::Value>), ContractError> {
    match body {
        serde_json::Value::Array(elements) => Ok((true, elements.iter().collect())),
        serde_json::Value::Object(_) => Ok((false, vec![body])),
        _ => Err(ContractError::Malformed),
    }
}
