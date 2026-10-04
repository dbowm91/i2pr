//! Plan 289 TunnelManager request envelope: exact wire shape for the
//! seven lifecycle actions.
//!
//! Wire form (named params beside `Token`) uses the canonical Proposal
//! names `Action`, `Name`, `Type`, `NewName`, and option fields at the
//! top level. The previous lowercase envelope and nested `options` map
//! are not accepted on the default endpoint.
//! - `Action`: required, exact [`TunnelAction`] spelling;
//! - `Name`: required for every named operation, including `get`;
//!   validated by
//!   [`crate::tunnel::validate_tunnel_name`];
//! - `Type`: required for `create`, forbidden otherwise (tunnel types
//!   are immutable after creation; rename travels as `new_name` on
//!   `edit`, preserving the pinned seven-action vocabulary instead of
//!   inventing a rename action);
//! - `NewName`: allowed only on `edit` (validated like `Name`);
//! - option fields are top-level and mapped through an explicit canonical
//!   alias table into existing typed domain option names.
//!
//! `edit` requires `NewName` or at least one option; otherwise there is
//! nothing to change. Unknown top-level keys are rejected: the envelope
//! is closed.

use std::collections::{BTreeMap, BTreeSet};

use crate::errors::ContractError;
use crate::limits::{MAX_OPTION_NAME_LEN, MAX_OPTION_VALUE_LEN, MAX_OPTIONS_PER_TUNNEL};
use crate::tunnel::{TunnelAction, TunnelType, validate_tunnel_name};
use crate::tunnel_options::find_option;

/// Decoded TunnelManager request.
#[derive(Eq, PartialEq)]
pub struct TunnelManagerRequest {
    /// Lifecycle action.
    pub action: TunnelAction,
    /// Apply start/stop/restart to the complete control-owned set.
    pub all: bool,
    /// Tunnel name (absent only for an `All` lifecycle operation).
    pub name: Option<String>,
    /// Tunnel type (`create` only).
    pub tunnel_type: Option<TunnelType>,
    /// Rename target (`edit` only).
    pub new_name: Option<String>,
    /// Options map (`create`/`edit` only, normalized values).
    pub options: BTreeMap<String, String>,
}

impl core::fmt::Debug for TunnelManagerRequest {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("TunnelManagerRequest")
            .field("action", &self.action)
            .field("all", &self.all)
            .field("name", &self.name)
            .field("tunnel_type", &self.tunnel_type)
            .field("new_name", &self.new_name)
            .field("option_keys", &self.options.keys().collect::<Vec<_>>())
            .finish()
    }
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
    /// Two aliases named the same domain option.
    DuplicateAlias(String),
    /// Proposal field is valid, but its owner is registered to a later plan.
    UnavailableOption(String),
    /// `All` selected a capability not implemented by the current owner.
    AllUnavailable,
    /// `edit` carried neither `NewName` nor options.
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
            Self::DuplicateAlias(_) => write!(formatter, "duplicate TunnelManager aliases"),
            Self::UnavailableOption(_) => write!(formatter, "TunnelManager field is unavailable"),
            Self::AllUnavailable => write!(formatter, "All action is not available"),
        }
    }
}

fn canonical_option(key: &str) -> Option<&'static str> {
    match key {
        "TargetHost" | "Host" => Some("target_host"),
        "TargetPort" => Some("target_port"),
        "Port" => Some("listen_port"),
        "ReachableBy" => Some("listen_host"),
        "Description" => Some("description"),
        "MaxConcurrentConns" => Some("max_streams"),
        "ProxyAuth" => Some("proxy_auth"),
        "MultiHoming" => Some("multihoming"),
        // These canonical Proposal controls already have bounded idle
        // lifecycle owners in the service-tunnel runtime.
        "Close" => Some("close_on_idle"),
        "Reduce" => Some("reduce_on_idle"),
        "TargetDestination" | "Destination" => Some("target_destination"),
        "UseSSL" => Some("use_ssl"),
        "UniqueLocalAddressPerClient" => Some("unique_local_address"),
        _ => crate::tunnel_options::TUNNEL_OPTIONS
            .iter()
            .find(|option| pascal_option_name(option.name) == key)
            .map(|option| option.name),
    }
}

fn pascal_option_name(key: &str) -> String {
    let mut output = String::with_capacity(key.len());
    let mut uppercase = true;
    for character in key.chars() {
        if character == '_' {
            uppercase = true;
        } else if uppercase {
            output.extend(character.to_uppercase());
            uppercase = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn scalar_string(key: &str, value: &serde_json::Value) -> Result<String, TunnelRequestError> {
    let option = find_option(key).map_err(TunnelRequestError::BadOption)?;
    let text = match (option.value_type, value) {
        (crate::tunnel_options::OptionValueType::String, serde_json::Value::String(text)) => {
            text.clone()
        }
        (crate::tunnel_options::OptionValueType::Integer, serde_json::Value::Number(number))
            if number.is_i64() || number.is_u64() =>
        {
            number.to_string()
        }
        (crate::tunnel_options::OptionValueType::Boolean, serde_json::Value::Bool(flag)) => {
            flag.to_string()
        }
        _ => return Err(TunnelRequestError::BadValue(key.to_owned())),
    };
    if text.len() > MAX_OPTION_VALUE_LEN {
        return Err(TunnelRequestError::ValueOverBound(key.to_owned()));
    }
    Ok(text)
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
    let mut all = false;
    let mut unavailable_option = None;
    let mut seen_aliases = BTreeSet::new();

    for (key, value) in params {
        match key.as_str() {
            "Token" => continue,
            "Action" => {
                let text = value
                    .as_str()
                    .ok_or(TunnelRequestError::BadAction(ContractError::Malformed))?;
                action = Some(TunnelAction::parse(text).map_err(TunnelRequestError::BadAction)?);
            }
            "Name" => {
                let text = value.as_str().ok_or(TunnelRequestError::BadName)?;
                validate_tunnel_name(text).map_err(|_| TunnelRequestError::BadName)?;
                name = Some(text.to_owned());
            }
            "Type" => {
                let text = value
                    .as_str()
                    .ok_or(TunnelRequestError::BadType(ContractError::Malformed))?;
                tunnel_type = Some(TunnelType::parse(text).map_err(TunnelRequestError::BadType)?);
            }
            "NewName" => {
                let text = value.as_str().ok_or(TunnelRequestError::BadName)?;
                validate_tunnel_name(text).map_err(|_| TunnelRequestError::BadName)?;
                new_name = Some(text.to_owned());
            }
            "All" => {
                all = value
                    .as_bool()
                    .ok_or(TunnelRequestError::BadValue("All".to_owned()))?;
            }
            key => {
                if key.len() > MAX_OPTION_NAME_LEN {
                    return Err(TunnelRequestError::BadOption(ContractError::OverBound));
                }
                if crate::proposal_wire::proposal_tunnel_value_type(key).is_none() {
                    return Err(TunnelRequestError::UnknownKey(truncated_key(key)));
                }
                if matches!(
                    crate::proposal_wire::proposal_tunnel_value_type(key),
                    Some(crate::proposal_wire::ProposalTunnelValueType::String)
                ) && value
                    .as_str()
                    .is_some_and(|text| text.len() > MAX_OPTION_VALUE_LEN)
                {
                    return Err(TunnelRequestError::ValueOverBound(key.to_owned()));
                }
                crate::proposal_wire::validate_proposal_tunnel_value(key, value)
                    .map_err(|_| TunnelRequestError::BadValue(key.to_owned()))?;
                let alias = canonical_wire_alias(key);
                if !seen_aliases.insert(alias) {
                    return Err(TunnelRequestError::DuplicateAlias(alias.to_owned()));
                }
                if key == "Description" {
                    let text = value
                        .as_str()
                        .ok_or_else(|| TunnelRequestError::BadValue(key.to_owned()))?;
                    if text.len() > MAX_OPTION_VALUE_LEN {
                        return Err(TunnelRequestError::ValueOverBound(key.to_owned()));
                    }
                    options.insert("description".to_owned(), text.to_owned());
                    options_seen = true;
                    continue;
                }
                if key == "MaxConcurrentConns" {
                    let value = value
                        .as_u64()
                        .ok_or_else(|| TunnelRequestError::BadValue(key.to_owned()))?;
                    options.insert("max_streams".to_owned(), value.to_string());
                    options_seen = true;
                    continue;
                }
                if key == "ProxyAuth" {
                    let enabled = value
                        .as_bool()
                        .ok_or_else(|| TunnelRequestError::BadValue(key.to_owned()))?;
                    options.insert("proxy_auth".to_owned(), enabled.to_string());
                    options_seen = true;
                    continue;
                }
                let Some(option_key) = canonical_option(key) else {
                    unavailable_option.get_or_insert_with(|| key.to_owned());
                    options_seen = true;
                    continue;
                };
                let option = find_option(option_key).map_err(TunnelRequestError::BadOption)?;
                let wire_type = crate::proposal_wire::proposal_tunnel_value_type(key)
                    .expect("Proposal field type was checked");
                let adapter_type_matches = matches!(
                    (wire_type, option.value_type),
                    (
                        crate::proposal_wire::ProposalTunnelValueType::String,
                        crate::tunnel_options::OptionValueType::String
                    ) | (
                        crate::proposal_wire::ProposalTunnelValueType::Integer,
                        crate::tunnel_options::OptionValueType::Integer
                    ) | (
                        crate::proposal_wire::ProposalTunnelValueType::Boolean,
                        crate::tunnel_options::OptionValueType::Boolean
                    )
                );
                if !adapter_type_matches {
                    unavailable_option.get_or_insert_with(|| key.to_owned());
                    options_seen = true;
                    continue;
                }
                let text = scalar_string(option_key, value)?;
                if options.insert(option_key.to_owned(), text).is_some() {
                    return Err(TunnelRequestError::DuplicateAlias(option_key.to_owned()));
                }
                options_seen = true;
                if options.len() > MAX_OPTIONS_PER_TUNNEL {
                    return Err(TunnelRequestError::TooManyOptions);
                }
            }
        }
    }

    let action = action.ok_or(TunnelRequestError::MissingField("action"))?;
    match action {
        TunnelAction::Get => {
            if all {
                return Err(TunnelRequestError::BadValue("All".to_owned()));
            }
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
        TunnelAction::Create => {
            if all {
                return Err(TunnelRequestError::BadValue("All".to_owned()));
            }
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
            if all {
                return Err(TunnelRequestError::BadValue("All".to_owned()));
            }
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
            if all
                && !matches!(
                    action,
                    TunnelAction::Start | TunnelAction::Stop | TunnelAction::Restart
                )
            {
                return Err(TunnelRequestError::BadValue("All".to_owned()));
            }
            if all && name.is_some() {
                return Err(TunnelRequestError::UnexpectedField("name"));
            }
            if !all && name.is_none() {
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
    if let Some(option) = unavailable_option {
        return Err(TunnelRequestError::UnavailableOption(option));
    }
    Ok(TunnelManagerRequest {
        action,
        all,
        name,
        tunnel_type,
        new_name,
        options,
    })
}

fn canonical_wire_alias(key: &str) -> &'static str {
    match key {
        "MaxConcurrentConns" => "MaxConcurrentConns",
        "TargetHost" | "Host" => "TargetHost",
        "TargetDestination" | "Destination" => "TargetDestination",
        "WebsiteHostname" | "SpoofedHost" => "WebsiteHostname",
        _ => crate::proposal_wire::PROPOSAL_TUNNEL_MANAGER_FIELDS
            .iter()
            .copied()
            .find(|candidate| *candidate == key)
            .unwrap_or("invalid"),
    }
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
