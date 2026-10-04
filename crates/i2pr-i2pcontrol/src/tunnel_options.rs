//! Existing typed tunnel-option subset with value types, secret
//! sensitivity, and family applicability metadata.
//!
//! Every option carries an explicit [`OptionSensitivity`]: there is no
//! unclassified option. Secret values use redacted wrappers downstream and
//! are filtered from `rawConfig` output, errors, and logs (Plan 292 rule).
//! Applicability is a 12-bit mask over [`crate::tunnel::TUNNEL_TYPES`] in
//! canonical order (bit `i` = `TUNNEL_TYPES[i]`).

use crate::errors::ContractError;

/// Value type of one tunnel option.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionValueType {
    /// UTF-8 string (bounded by [`crate::limits::MAX_OPTION_VALUE_LEN`]).
    String,
    /// Signed 64-bit integer (range-checked per option by later plans).
    Integer,
    /// Boolean.
    Boolean,
}

/// Secret handling classification. Every option has exactly one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionSensitivity {
    /// Safe for control output, snapshots, and diagnostics.
    Public,
    /// Never returned through `rawConfig`, errors, logs, or `Debug`.
    Secret,
}

/// One frozen tunnel option row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TunnelOption {
    /// Exact wire key.
    pub name: &'static str,
    /// Value type.
    pub value_type: OptionValueType,
    /// Secret classification.
    pub sensitivity: OptionSensitivity,
    /// Applicability mask over the twelve types (bit `i` = type `i`).
    pub applies_mask: u16,
}

impl TunnelOption {
    /// Whether this option applies to the type at canonical `index`.
    pub const fn applies_to(self, index: usize) -> bool {
        if index >= 12 {
            return false;
        }
        (self.applies_mask & (1 << index)) != 0
    }
}

/// Bit helpers over the canonical type order
/// (`client, server, httpclient, socks, ircclient, ircserver,
/// connectclient, socksirc, httpserver, httpbidirserver,
/// streamrclient, streamrserver`).
pub const MASK_ALL: u16 = 0x0FFF;
pub const MASK_CLIENT_FAMILIES: u16 = (1 << 0) | (1 << 2) | (1 << 3) | (1 << 6) | (1 << 7);
pub const MASK_SERVER_FAMILIES: u16 = (1 << 1) | (1 << 8) | (1 << 9);
pub const MASK_STREAMR: u16 = (1 << 10) | (1 << 11);
pub const MASK_PROXY_AUTH: u16 = (1 << 2) | (1 << 3) | (1 << 6) | (1 << 7);
pub const MASK_PUBLISHING: u16 = (1 << 1) | (1 << 8) | (1 << 9) | (1 << 11);

/// Existing typed option subset: 46 rows. This is not the full Proposal
/// 170 TunnelManager vocabulary; canonical wire names are inventoried in
/// [`crate::proposal_wire::PROPOSAL_TUNNEL_MANAGER_FIELDS`].
pub const TUNNEL_OPTIONS: [TunnelOption; 46] = [
    TunnelOption {
        name: "target_host",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "target_port",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "listen_host",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "listen_port",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "target_destination",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_CLIENT_FAMILIES | (1 << 10),
    },
    TunnelOption {
        name: "target_i2p_port",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_CLIENT_FAMILIES | (1 << 10),
    },
    TunnelOption {
        name: "use_ssl",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_SERVER_FAMILIES,
    },
    TunnelOption {
        name: "local_udp_host",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_STREAMR,
    },
    TunnelOption {
        name: "local_udp_port",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_STREAMR,
    },
    TunnelOption {
        name: "remote_udp_host",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: (1 << 10),
    },
    TunnelOption {
        name: "tunnel_length",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "tunnel_quantity",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "tunnel_backup_quantity",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "tunnel_variance",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "inbound_length",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "outbound_length",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "inbound_quantity",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "outbound_quantity",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "profile",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "interactive",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "start_on_load",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "idle_timeout",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "close_on_idle",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "new_dest_on_idle",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "reduce_on_idle",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "max_streams",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "proxy_username",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_PROXY_AUTH,
    },
    TunnelOption {
        name: "proxy_password",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Secret,
        applies_mask: MASK_PROXY_AUTH,
    },
    TunnelOption {
        name: "access_list",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_SERVER_FAMILIES,
    },
    TunnelOption {
        name: "white_list",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_SERVER_FAMILIES,
    },
    TunnelOption {
        name: "black_list",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_SERVER_FAMILIES,
    },
    TunnelOption {
        name: "address_helper",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_SERVER_FAMILIES,
    },
    TunnelOption {
        name: "jump_list",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_SERVER_FAMILIES,
    },
    TunnelOption {
        name: "unique_local_address",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_SERVER_FAMILIES,
    },
    TunnelOption {
        name: "multihoming",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_SERVER_FAMILIES,
    },
    TunnelOption {
        name: "reply_bundling",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "streamr_subscribe_interval",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: (1 << 10),
    },
    TunnelOption {
        name: "streamr_expiry",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: (1 << 11),
    },
    TunnelOption {
        name: "streamr_max_subscribers",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: (1 << 11),
    },
    TunnelOption {
        name: "streamr_payload_limit",
        value_type: OptionValueType::Integer,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_STREAMR,
    },
    TunnelOption {
        name: "sig_type",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_ALL,
    },
    TunnelOption {
        name: "encrypt_lease_set",
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_PUBLISHING,
    },
    TunnelOption {
        name: "leaseset_password",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Secret,
        applies_mask: MASK_PUBLISHING,
    },
    TunnelOption {
        name: "leaseset_blinding_secret",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Secret,
        applies_mask: MASK_PUBLISHING,
    },
    TunnelOption {
        name: "leaseset_client_auth",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Secret,
        applies_mask: MASK_PUBLISHING,
    },
    TunnelOption {
        name: "use_outproxy_plugin",
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: (1 << 2) | (1 << 6),
    },
];

/// Exact frozen secret-classified option names (4 of 46).
pub const SECRET_OPTIONS: [&str; 4] = [
    "proxy_password",
    "leaseset_password",
    "leaseset_blinding_secret",
    "leaseset_client_auth",
];

/// Looks up an exact option by wire key (case-sensitive).
pub fn find_option(name: &str) -> Result<TunnelOption, ContractError> {
    for option in TUNNEL_OPTIONS {
        if option.name == name {
            return Ok(option);
        }
    }
    for option in TUNNEL_OPTIONS {
        if option.name.eq_ignore_ascii_case(name) {
            return Err(ContractError::CaseMismatch);
        }
    }
    Err(ContractError::UnknownLiteral)
}
