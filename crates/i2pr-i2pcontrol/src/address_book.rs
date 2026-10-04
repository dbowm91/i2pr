//! AddressBook domain: four book types plus mutation/subscription/config
//! request vocabulary and the thirteen-key `SetConfig` inventory.
//!
//! Wire behavior (Plan 294): a whole request validates before mutation;
//! mixed incompatible operation shapes fail rather than applying partial
//! changes. Path-like config values are administrative logical paths, never
//! unrestricted filesystem selectors.

use crate::errors::ContractError;

/// Exact frozen book-type inventory in canonical order.
pub const BOOK_TYPES: [&str; 4] = ["private", "local", "router", "published"];

/// Exact frozen request-field vocabulary in canonical order.
pub const ADDRESS_BOOK_FIELDS: [&str; 6] = [
    "Type",
    "Hostname",
    "Destination",
    "Delete",
    "SetSubscriptions",
    "SetConfig",
];

/// Exact Proposal 170 thirteen-key `SetConfig` inventory in specification order.
pub const SET_CONFIG_KEYS: [&str; 13] = crate::proposal_wire::PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS;

/// The four administrative books.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookType {
    /// Operator-private entries.
    Private,
    /// Locally pinned entries.
    Local,
    /// Router-distributed entries.
    Router,
    /// Published entries.
    Published,
}

impl BookType {
    /// Exact wire spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Local => "local",
            Self::Router => "router",
            Self::Published => "published",
        }
    }

    /// Parses an exact book spelling (case-sensitive).
    pub fn parse(name: &str) -> Result<Self, ContractError> {
        match name {
            "private" => Ok(Self::Private),
            "local" => Ok(Self::Local),
            "router" => Ok(Self::Router),
            "published" => Ok(Self::Published),
            _ => {
                for known in BOOK_TYPES {
                    if known.eq_ignore_ascii_case(name) {
                        return Err(ContractError::CaseMismatch);
                    }
                }
                Err(ContractError::UnknownLiteral)
            }
        }
    }
}

/// Request-field vocabulary for the `AddressBook` method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressBookField {
    /// Book selector (`Type`).
    Type,
    /// Entry hostname.
    Hostname,
    /// Full Destination text.
    Destination,
    /// Entry removal flag.
    Delete,
    /// Subscription-URL replacement.
    SetSubscriptions,
    /// Config-map replacement.
    SetConfig,
}

impl AddressBookField {
    /// Exact wire spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Type => "Type",
            Self::Hostname => "Hostname",
            Self::Destination => "Destination",
            Self::Delete => "Delete",
            Self::SetSubscriptions => "SetSubscriptions",
            Self::SetConfig => "SetConfig",
        }
    }

    /// Parses an exact field spelling (case-sensitive).
    pub fn parse(name: &str) -> Result<Self, ContractError> {
        match name {
            "Type" => Ok(Self::Type),
            "Hostname" => Ok(Self::Hostname),
            "Destination" => Ok(Self::Destination),
            "Delete" => Ok(Self::Delete),
            "SetSubscriptions" => Ok(Self::SetSubscriptions),
            "SetConfig" => Ok(Self::SetConfig),
            _ => {
                for known in ADDRESS_BOOK_FIELDS {
                    if known.eq_ignore_ascii_case(name) {
                        return Err(ContractError::CaseMismatch);
                    }
                }
                Err(ContractError::UnknownLiteral)
            }
        }
    }
}

/// Parses an exact `SetConfig` key (case-sensitive).
pub fn parse_set_config_key(name: &str) -> Result<usize, ContractError> {
    for (index, known) in SET_CONFIG_KEYS.iter().enumerate() {
        if *known == name {
            return Ok(index);
        }
    }
    for known in SET_CONFIG_KEYS {
        if known.eq_ignore_ascii_case(name) {
            return Err(ContractError::CaseMismatch);
        }
    }
    Err(ContractError::UnknownLiteral)
}

/// Whether a `SetConfig` key carries path-like administrative content that
/// must stay confined beneath an owned root.
pub fn is_path_like_config_key(name: &str) -> bool {
    matches!(
        name,
        "subscriptions"
            | "published_addressbook"
            | "router_addressbook"
            | "local_addressbook"
            | "private_addressbook"
            | "etags"
            | "last_modified"
            | "log"
    )
}

/// Whether a `SetConfig` key is inert frontend-only metadata (never a
/// router behavior switch).
pub fn is_inert_config_key(name: &str) -> bool {
    matches!(name, "theme")
}
