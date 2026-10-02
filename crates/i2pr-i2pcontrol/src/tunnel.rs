//! TunnelManager domain: seven actions, twelve tunnel types, names,
//! lifecycle/status/result vocabulary.
//!
//! The twelve types are the six families i2pr already implements (Plan 289),
//! the four composed families (Plan 290), and the two Streamr families
//! (Plan 291). Types without a runtime backend return explicit unsupported
//! and allocate no listener/destination/task.

use crate::errors::ContractError;
use crate::limits::{MAX_TUNNEL_NAME_LEN, check_str};

/// Exact frozen action inventory in canonical order.
pub const TUNNEL_ACTIONS: [&str; 7] = [
    "get", "create", "edit", "delete", "start", "stop", "restart",
];

/// Exact frozen twelve-type inventory in canonical order.
pub const TUNNEL_TYPES: [&str; 12] = [
    "client",
    "server",
    "httpclient",
    "socks",
    "ircclient",
    "ircserver",
    "connectclient",
    "socksirc",
    "httpserver",
    "httpbidirserver",
    "streamrclient",
    "streamrserver",
];

/// The seven lifecycle actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelAction {
    /// Read actual runtime state plus persisted intent.
    Get,
    /// Create a control-owned definition.
    Create,
    /// Mutate a control-owned definition.
    Edit,
    /// Remove a control-owned definition.
    Delete,
    /// Start runtime for a control-owned definition.
    Start,
    /// Stop runtime for a control-owned definition.
    Stop,
    /// Stop then start runtime for a control-owned definition.
    Restart,
}

impl TunnelAction {
    /// Exact wire spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Create => "create",
            Self::Edit => "edit",
            Self::Delete => "delete",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }

    /// Whether the action mutates durable control state.
    pub const fn is_mutating(self) -> bool {
        matches!(
            self,
            Self::Create | Self::Edit | Self::Delete | Self::Start | Self::Stop | Self::Restart
        )
    }

    /// Parses an exact action spelling (case-sensitive).
    pub fn parse(name: &str) -> Result<Self, ContractError> {
        match name {
            "get" => Ok(Self::Get),
            "create" => Ok(Self::Create),
            "edit" => Ok(Self::Edit),
            "delete" => Ok(Self::Delete),
            "start" => Ok(Self::Start),
            "stop" => Ok(Self::Stop),
            "restart" => Ok(Self::Restart),
            _ => {
                for known in TUNNEL_ACTIONS {
                    if known.eq_ignore_ascii_case(name) {
                        return Err(ContractError::CaseMismatch);
                    }
                }
                Err(ContractError::UnknownLiteral)
            }
        }
    }
}

/// The twelve Proposal tunnel types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelType {
    /// Generic streaming client.
    Client,
    /// Generic streaming server.
    Server,
    /// HTTP client proxy.
    HttpClient,
    /// SOCKS client proxy.
    Socks,
    /// IRC client filter.
    IrcClient,
    /// IRC server.
    IrcServer,
    /// Strict HTTP CONNECT client (Plan 290).
    ConnectClient,
    /// SOCKS + IRC filter composition (Plan 290).
    SocksIrc,
    /// Filtered HTTP server (Plan 290).
    HttpServer,
    /// Deprecated bidirectional HTTP server (Plan 290).
    HttpBidirServer,
    /// Repliable-datagram subscriber (Plan 291).
    StreamrClient,
    /// Repliable-datagram publisher (Plan 291).
    StreamrServer,
}

impl TunnelType {
    /// Exact wire spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Server => "server",
            Self::HttpClient => "httpclient",
            Self::Socks => "socks",
            Self::IrcClient => "ircclient",
            Self::IrcServer => "ircserver",
            Self::ConnectClient => "connectclient",
            Self::SocksIrc => "socksirc",
            Self::HttpServer => "httpserver",
            Self::HttpBidirServer => "httpbidirserver",
            Self::StreamrClient => "streamrclient",
            Self::StreamrServer => "streamrserver",
        }
    }

    /// Whether i2pr already has a real backend at Plan 289 scope (the six
    /// M10 families). Plan 290/291 families report explicit unsupported
    /// until their plans close. Retained as the Plan 289 floor pin.
    pub const fn has_plan289_backend(self) -> bool {
        matches!(
            self,
            Self::Client
                | Self::Server
                | Self::HttpClient
                | Self::Socks
                | Self::IrcClient
                | Self::IrcServer
        )
    }

    /// Whether i2pr has a real backend at Plan 290 scope (the six
    /// M10 families plus the four composed families). Only the two
    /// Streamr families report explicit unsupported until Plan 291
    /// closes.
    pub const fn has_plan290_backend(self) -> bool {
        matches!(
            self,
            Self::Client
                | Self::Server
                | Self::HttpClient
                | Self::Socks
                | Self::IrcClient
                | Self::IrcServer
                | Self::ConnectClient
                | Self::SocksIrc
                | Self::HttpServer
                | Self::HttpBidirServer
        )
    }

    /// Whether i2pr has a real backend at Plan 291 scope: the ten
    /// Plan 290 backends plus the two Streamr families. All twelve
    /// Proposal types map after Plan 291 closes.
    pub const fn has_plan291_backend(self) -> bool {
        matches!(
            self,
            Self::Client
                | Self::Server
                | Self::HttpClient
                | Self::Socks
                | Self::IrcClient
                | Self::IrcServer
                | Self::ConnectClient
                | Self::SocksIrc
                | Self::HttpServer
                | Self::HttpBidirServer
                | Self::StreamrClient
                | Self::StreamrServer
        )
    }

    /// Parses an exact type spelling (case-sensitive).
    pub fn parse(name: &str) -> Result<Self, ContractError> {
        match name {
            "client" => Ok(Self::Client),
            "server" => Ok(Self::Server),
            "httpclient" => Ok(Self::HttpClient),
            "socks" => Ok(Self::Socks),
            "ircclient" => Ok(Self::IrcClient),
            "ircserver" => Ok(Self::IrcServer),
            "connectclient" => Ok(Self::ConnectClient),
            "socksirc" => Ok(Self::SocksIrc),
            "httpserver" => Ok(Self::HttpServer),
            "httpbidirserver" => Ok(Self::HttpBidirServer),
            "streamrclient" => Ok(Self::StreamrClient),
            "streamrserver" => Ok(Self::StreamrServer),
            _ => {
                for known in TUNNEL_TYPES {
                    if known.eq_ignore_ascii_case(name) {
                        return Err(ContractError::CaseMismatch);
                    }
                }
                Err(ContractError::UnknownLiteral)
            }
        }
    }
}

/// Per-name supervisor snapshot classification (Plan 289 §Runtime state).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelStatus {
    /// No backend for this type yet.
    Unsupported,
    /// Committed but not running.
    Stopped,
    /// Transitioning to running.
    Starting,
    /// Running.
    Running,
    /// Transitioning to stopped.
    Stopping,
    /// Last transition failed.
    Failed,
}

impl TunnelStatus {
    /// Exact wire spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Failed => "failed",
        }
    }

    /// All six statuses in canonical order.
    pub const ALL: [&str; 6] = [
        "unsupported",
        "stopped",
        "starting",
        "running",
        "stopping",
        "failed",
    ];
}

/// Validates a tunnel name (non-empty, bounded, no path separators or
/// whitespace control characters).
pub fn validate_tunnel_name(name: &str) -> Result<(), ContractError> {
    if name.is_empty() {
        return Err(ContractError::Malformed);
    }
    check_str(name, MAX_TUNNEL_NAME_LEN)?;
    for character in name.chars() {
        if character == '/' || character == '\\' || character.is_control() {
            return Err(ContractError::Malformed);
        }
    }
    Ok(())
}
