//! Exact method inventory and protected/public classification.
//!
//! The frozen Plan 286 surface is exactly five methods: the base public
//! `Authenticate` plus the four Proposal 170 capability methods
//! (`RouterInfo`, `AddressBook`, `TunnelManager`, `ClientServicesInfo`).
//! No unrelated base method is added. Unknown methods remain representable
//! so the daemon can answer standard method-not-found.

use crate::auth::AUTHENTICATE_METHOD;
use crate::errors::ContractError;

/// Exact frozen method inventory in canonical order.
pub const METHODS: [&str; 5] = [
    "Authenticate",
    "RouterInfo",
    "AddressBook",
    "TunnelManager",
    "ClientServicesInfo",
];

/// Typed method handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    /// Base public authentication entry point.
    Authenticate,
    /// Proposal 170 router inspection.
    RouterInfo,
    /// Proposal 170 canonical address-book operations.
    AddressBook,
    /// Proposal 170 tunnel lifecycle.
    TunnelManager,
    /// Proposal 170 service inspection.
    ClientServicesInfo,
}

impl Method {
    /// Exact wire spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Authenticate => "Authenticate",
            Self::RouterInfo => "RouterInfo",
            Self::AddressBook => "AddressBook",
            Self::TunnelManager => "TunnelManager",
            Self::ClientServicesInfo => "ClientServicesInfo",
        }
    }

    /// Whether the method requires a live token (`false` only for
    /// `Authenticate`).
    pub const fn requires_token(self) -> bool {
        !matches!(self, Self::Authenticate)
    }

    /// Parses an exact method name (case-sensitive).
    pub fn parse(name: &str) -> Result<Self, ContractError> {
        match name {
            "Authenticate" => Ok(Self::Authenticate),
            "RouterInfo" => Ok(Self::RouterInfo),
            "AddressBook" => Ok(Self::AddressBook),
            "TunnelManager" => Ok(Self::TunnelManager),
            "ClientServicesInfo" => Ok(Self::ClientServicesInfo),
            _ => {
                // Distinguish a case-only mismatch (typed classification)
                // from a genuinely unknown method.
                for known in METHODS {
                    if known.eq_ignore_ascii_case(name) {
                        return Err(ContractError::CaseMismatch);
                    }
                }
                Err(ContractError::UnknownLiteral)
            }
        }
    }

    /// Canonical deterministic ordering index.
    pub const fn index(self) -> usize {
        match self {
            Self::Authenticate => 0,
            Self::RouterInfo => 1,
            Self::AddressBook => 2,
            Self::TunnelManager => 3,
            Self::ClientServicesInfo => 4,
        }
    }

    /// Proves the base authenticate spelling matches the auth vocabulary.
    pub fn check_auth_spelling() -> bool {
        matches!(AUTHENTICATE_METHOD, "Authenticate")
    }
}
