//! Exact ClientServicesInfo selectors (six) and result shapes.
//!
//! `BOB` is an explicit constant capability result (`enabled = false`):
//! i2pr has no BOB service and the proposal describes Java BOB as
//! deprecated. It is not a missing-handler fallback (Plan 288 rule).

use crate::errors::ContractError;

/// Exact frozen service inventory in canonical order.
pub const CLIENT_SERVICES: [&str; 6] = ["I2PTunnel", "HTTPProxy", "SOCKS", "SAM", "BOB", "I2CP"];

/// Typed service selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientService {
    /// Bounded map of configured service names (Plan 288 source).
    I2pTunnel,
    /// Actual HTTP service enabled/bind state.
    HttpProxy,
    /// Actual SOCKS service enabled/bind state.
    Socks,
    /// Actual SAM enabled state plus bounded session info.
    Sam,
    /// Deliberate constant: no BOB service exists.
    Bob,
    /// Actual I2CP enabled/bind state.
    I2cp,
}

impl ClientService {
    /// Exact wire spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::I2pTunnel => "I2PTunnel",
            Self::HttpProxy => "HTTPProxy",
            Self::Socks => "SOCKS",
            Self::Sam => "SAM",
            Self::Bob => "BOB",
            Self::I2cp => "I2CP",
        }
    }

    /// Whether the result shape is a bounded map of service entries
    /// (`true` only for `I2PTunnel`; all others report scalar state).
    pub const fn is_service_map(self) -> bool {
        matches!(self, Self::I2pTunnel)
    }

    /// Whether this selector is a deliberate constant (`true` only for
    /// `BOB`).
    pub const fn is_constant(self) -> bool {
        matches!(self, Self::Bob)
    }

    /// Parses an exact service spelling (case-sensitive).
    pub fn parse(name: &str) -> Result<Self, ContractError> {
        match name {
            "I2PTunnel" => Ok(Self::I2pTunnel),
            "HTTPProxy" => Ok(Self::HttpProxy),
            "SOCKS" => Ok(Self::Socks),
            "SAM" => Ok(Self::Sam),
            "BOB" => Ok(Self::Bob),
            "I2CP" => Ok(Self::I2cp),
            _ => {
                for known in CLIENT_SERVICES {
                    if known.eq_ignore_ascii_case(name) {
                        return Err(ContractError::CaseMismatch);
                    }
                }
                Err(ContractError::UnknownLiteral)
            }
        }
    }
}
