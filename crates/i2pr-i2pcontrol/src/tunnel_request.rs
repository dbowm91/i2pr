//! Plan 289 TunnelManager request envelope: exact wire shape for the
//! seven lifecycle actions.
//!
//! Wire form (named params beside `Token`):
//! - `action`: required, exact [`TunnelAction`] spelling;
//! - `name`: required for every action except `get` (absent `name` on
//!   `get` selects all summaries); validated by
//!   [`crate::tunnel::validate_tunnel_name`];
//! - `type`: required for `create`, forbidden otherwise (tunnel types
//!   are immutable after creation; rename travels as `new_name` on
//!   `edit`, preserving the pinned seven-action vocabulary instead of
//!   inventing a rename action);
//! - `new_name`: allowed only on `edit` (validated like `name`);
//! - `options`: allowed only on `create`/`edit`; a bounded map whose
//!   keys must belong to the frozen 46-option universe ([`find_option`]
//!   rejects unknown keys, [`ContractError::CaseMismatch`] preserved) and
//!   whose values are strings, integers, or booleans normalized to
//!   strings (null/array/object rejected).
//!
//! `edit` requires `new_name` or at least one option; otherwise there is
//! nothing to change. Unknown top-level keys are rejected: the envelope
//! is closed.

use std::collections::BTreeMap;

use crate::errors::ContractError;
use crate::limits::{MAX_OPTION_NAME_LEN, MAX_OPTION_VALUE_LEN, MAX_OPTIONS_PER_TUNNEL};
use crate::tunnel::{TunnelAction, TunnelType, validate_tunnel_name};
use crate::tunnel_options::find_option;

/// Decoded TunnelManager request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TunnelManagerRequest {
    /// Lifecycle action.
    pub action: TunnelAction,
    /// Tunnel name (absent only for whole-inventory `get`).
    pub name: Option<String>,
    /// Tunnel type (`create` only).
    pub tunnel_type: Option<TunnelType>,
    /// Rename target (`edit` only).
    pub new_name: Option<String>,
    /// Options map (`create`/`edit` only, normalized values).
    pub options: BTreeMap<String, String>,
}

/// Envelope rejection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TunnelRequestError {
    /// A required field is absent.
    MissingField(&'static str),
    /// A field is present where the action forbids it.
    UnexpectedField(&'static str),
    /// The action spelling is unknown (contract error preserved).
    BadAction(ContractError),
    /// The name fails [`validate_tunnel_name`].
    BadName,
    /// The type spelling is unknown (contract error preserved).
    BadType(ContractError),
    /// An option key is outside the frozen universe (contract error
    /// preserved, including case mismatch).
    BadOption(ContractError),
    /// An option value is null, an array, or an object.
    BadValue(String),
    /// An option value exceeds [`MAX_OPTION_VALUE_LEN`].
    ValueOverBound(String),
    /// The options map exceeds [`MAX_OPTIONS_PER_TUNNEL`].
    TooManyOptions,
    /// An unknown top-level key.
    UnknownKey(String),
    /// `edit` carried neither `new_name` nor options.
    NothingToChange,
}

impl core::fmt::Display for TunnelRequestError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingField(field) => write!(formatter, "missing required field {field}"),
            Self::UnexpectedField(field) => {
                write!(formatter, "field {field} not allowed for this action")
            }
            Self::BadAction(error) => write!(formatter, "invalid action: {error:?}"),
            Self::BadName => write!(formatter, "invalid tunnel name"),
            Self::BadType(error) => write!(formatter, "invalid tunnel type: {error:?}"),
            Self::BadOption(error) => write!(formatter, "unknown tunnel option: {error:?}"),
            Self::BadValue(key) => write!(formatter, "invalid value type for option {key}"),
            Self::ValueOverBound(key) => write!(formatter, "value over ceiling for option {key}"),
            Self::TooManyOptions => write!(formatter, "options map over ceiling"),
            Self::UnknownKey(key) => write!(formatter, "unknown TunnelManager field {key}"),
            Self::NothingToChange => write!(formatter, "edit requires new_name or options"),
        }
    }
}

/// Returns the string form of a JSON scalar option value.
fn scalar_string(key: &str, value: &serde_json::Value) -> Result<String, TunnelRequestError> {
    match value {
        serde_json::Value::String(text) => Ok(text.clone()),
        serde_json::Value::Number(number) => Ok(number.to_string()),
        serde_json::Value::Bool(flag) => Ok(flag.to_string()),
        _ => Err(TunnelRequestError::BadValue(key.to_owned())),
    }
}

/// Decodes and validates one TunnelManager param map (without `Token`,
/// which the daemon authenticates separately).
pub fn decode_tunnel_request(
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<TunnelManagerRequest, TunnelRequestError> {
    let mut action: Option<TunnelAction> = None;
    let mut name: Option<String> = None;
    let mut tunnel_type: Option<TunnelType> = None;
    let mut new_name: Option<String> = None;
    let mut options: BTreeMap<String, String> = BTreeMap::new();
    let mut options_seen = false;

    for (key, value) in params {
        match key.as_str() {
            "Token" => continue,
            "action" => {
                let text = value
                    .as_str()
                    .ok_or(TunnelRequestError::BadAction(ContractError::Malformed))?;
                action = Some(TunnelAction::parse(text).map_err(TunnelRequestError::BadAction)?);
            }
            "name" => {
                let text = value.as_str().ok_or(TunnelRequestError::BadName)?;
                validate_tunnel_name(text).map_err(|_| TunnelRequestError::BadName)?;
                name = Some(text.to_owned());
            }
            "type" => {
                let text = value
                    .as_str()
                    .ok_or(TunnelRequestError::BadType(ContractError::Malformed))?;
                tunnel_type = Some(TunnelType::parse(text).map_err(TunnelRequestError::BadType)?);
            }
            "new_name" => {
                let text = value.as_str().ok_or(TunnelRequestError::BadName)?;
                validate_tunnel_name(text).map_err(|_| TunnelRequestError::BadName)?;
                new_name = Some(text.to_owned());
            }
            "options" => {
                let map = value
                    .as_object()
                    .ok_or(TunnelRequestError::BadOption(ContractError::Malformed))?;
                if map.len() > MAX_OPTIONS_PER_TUNNEL {
                    return Err(TunnelRequestError::TooManyOptions);
                }
                for (option_key, option_value) in map {
                    if option_key.len() > MAX_OPTION_NAME_LEN {
                        return Err(TunnelRequestError::BadOption(ContractError::OverBound));
                    }
                    find_option(option_key).map_err(TunnelRequestError::BadOption)?;
                    let text = scalar_string(option_key, option_value)?;
                    if text.len() > MAX_OPTION_VALUE_LEN {
                        return Err(TunnelRequestError::ValueOverBound(option_key.clone()));
                    }
                    options.insert(option_key.clone(), text);
                }
                options_seen = true;
            }
            _ => {
                return Err(TunnelRequestError::UnknownKey(truncated_key(key)));
            }
        }
    }

    let action = action.ok_or(TunnelRequestError::MissingField("action"))?;
    match action {
        TunnelAction::Get => {
            if tunnel_type.is_some() {
                return Err(TunnelRequestError::UnexpectedField("type"));
            }
            if new_name.is_some() {
                return Err(TunnelRequestError::UnexpectedField("new_name"));
            }
            if options_seen {
                return Err(TunnelRequestError::UnexpectedField("options"));
            }
        }
        TunnelAction::Create => {
            let _ = name
                .as_ref()
                .ok_or(TunnelRequestError::MissingField("name"))?;
            if tunnel_type.is_none() {
                return Err(TunnelRequestError::MissingField("type"));
            }
            if new_name.is_some() {
                return Err(TunnelRequestError::UnexpectedField("new_name"));
            }
        }
        TunnelAction::Edit => {
            if name.is_none() {
                return Err(TunnelRequestError::MissingField("name"));
            }
            if tunnel_type.is_some() {
                return Err(TunnelRequestError::UnexpectedField("type"));
            }
            if new_name.is_none() && !options_seen {
                return Err(TunnelRequestError::NothingToChange);
            }
        }
        TunnelAction::Delete | TunnelAction::Start | TunnelAction::Stop | TunnelAction::Restart => {
            if name.is_none() {
                return Err(TunnelRequestError::MissingField("name"));
            }
            if tunnel_type.is_some() {
                return Err(TunnelRequestError::UnexpectedField("type"));
            }
            if new_name.is_some() {
                return Err(TunnelRequestError::UnexpectedField("new_name"));
            }
            if options_seen {
                return Err(TunnelRequestError::UnexpectedField("options"));
            }
        }
    }
    Ok(TunnelManagerRequest {
        action,
        name,
        tunnel_type,
        new_name,
        options,
    })
}

/// Truncates an unknown key for error reporting (bounded diagnostics).
fn truncated_key(key: &str) -> String {
    const MAX_KEY_ECHO: usize = 128;
    if key.len() <= MAX_KEY_ECHO {
        key.to_owned()
    } else {
        key[..MAX_KEY_ECHO].to_owned()
    }
}
