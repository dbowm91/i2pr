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
/// Plan 342: the proxy client kinds that may declare an I2P-routed
/// outproxy provider (`httpclient`, `socks`, `connectclient`,
/// `socksirc`).
///
/// The membership is deliberately identical to [`MASK_PROXY_AUTH`] and is
/// defined separately anyway, because the two mean different things: that
/// mask is "this kind exposes a local authenticated listener", this one is
/// "this kind has a clearnet request target an outproxy can carry". The
/// kinds happen to be the same four today. Merging them would make a future
/// fifth proxy client silently inherit both meanings.
pub const MASK_OUTPROXY_PROVIDER: u16 = (1 << 2) | (1 << 3) | (1 << 6) | (1 << 7);
pub const MASK_PUBLISHING: u16 = (1 << 1) | (1 << 8) | (1 << 9) | (1 << 11);

/// Existing typed option subset: 52 rows. This is not the full Proposal
/// 170 TunnelManager vocabulary; canonical wire names are inventoried in
/// [`crate::proposal_wire::PROPOSAL_TUNNEL_MANAGER_FIELDS`].
pub const TUNNEL_OPTIONS: [TunnelOption; 52] = [
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
        // Plan 334: Proposal 170 types `EncryptLeaseSet` as one of ten
        // enumeration *strings*, so the Plan 293 "Boolean" determination was
        // wrong about the wire type and is corrected here. Plan 293 carried
        // this to Plan 295 as wire-shape divergence item 1.
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_PUBLISHING,
    },
    TunnelOption {
        name: "leaseset_password",
        // Plan 334: this slot is the Proposal 170 `OptionalLookup` field. The
        // ELS2 specification defines exactly one lookup secret, so this is
        // the single canonical spelling; `leaseset_blinding_secret` below is
        // refused as a duplicate rather than treated as a second secret.
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Secret,
        applies_mask: MASK_PUBLISHING,
    },
    TunnelOption {
        name: "leaseset_blinding_secret",
        // Plan 334: retired as a duplicate spelling. Kept in the inventory so
        // the closed envelope still names it in its refusal, but it has no
        // owner and must never be read as a second lookup secret.
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Secret,
        applies_mask: MASK_PUBLISHING,
    },
    TunnelOption {
        name: "leaseset_client_auth",
        // Plan 334: the Proposal 170 `LeaseSetClientAuths` array, carried
        // durably in the bounded encoding from
        // `proposal_leaseset_mode::encode_lease_set_client_auths`.
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Secret,
        applies_mask: MASK_PUBLISHING,
    },
    TunnelOption {
        name: "use_outproxy_plugin",
        // Plan 342. Plan 293 had typed this `String` because no provider
        // existed to give the flag a meaning; Proposal 170 spells it a
        // Boolean and the envelope's adapter check compares the wire type
        // against this one, so a `String` row would have refused a
        // correctly-formed boolean before any provider logic ran.
        //
        // The applicability mask widens from Plan 293's
        // `httpclient | connectclient` to the whole proxy-client block,
        // because the flag is the declaration that the tunnel uses the
        // configured provider and the block it declares is shared by all
        // four proxy client kinds.
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_OUTPROXY_PROVIDER,
    },
    TunnelOption {
        name: "proxy_list",
        // Plan 342: the Proposal 170 `ProxyList`. A bounded, comma or
        // whitespace separated list of I2P destinations or `.i2p` names.
        // Every entry is validated by `OutproxyEndpoint::parse`, which
        // refuses a clearnet host before the value reaches a definition.
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_OUTPROXY_PROVIDER,
    },
    TunnelOption {
        name: "outproxy_auth",
        // Plan 342: Proposal 170's `OutproxyAuth`, the credential
        // declaration for the *upstream outproxy*. Distinct from
        // `proxy_auth`, which authenticates the local listener.
        value_type: OptionValueType::Boolean,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_OUTPROXY_PROVIDER,
    },
    TunnelOption {
        name: "outproxy_username",
        // Plan 342: an identifier, not a credential. Never echoed into a
        // `Debug`, an error, or control output — `OutproxyConfig` has no
        // `Display` and no codec error interpolates it.
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_OUTPROXY_PROVIDER,
    },
    TunnelOption {
        name: "outproxy_password",
        // Plan 342: the outbound credential. Never stored in the clear:
        // `normalize_definition` replaces it with the Plan 341 sealed
        // stored form before the definition reaches a generation file, and
        // it is opened only inside `RouterOutproxyProvider::auth_header`.
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Secret,
        applies_mask: MASK_OUTPROXY_PROVIDER,
    },
    TunnelOption {
        name: "outproxy_type",
        // Plan 342: the finite `OutproxyType` vocabulary. It is a dialect
        // identifier, never a provider name, a path, or a command.
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_OUTPROXY_PROVIDER,
    },
    TunnelOption {
        name: "ssl_proxies",
        // Plan 342: the Proposal 170 `SSLProxies` subset. Parsed with the
        // same `OutproxyEndpoint` grammar as `proxy_list` and required to
        // be a subset of it, so a tunnelled outproxy can never be one the
        // failover policy never rotates into.
        value_type: OptionValueType::String,
        sensitivity: OptionSensitivity::Public,
        applies_mask: MASK_OUTPROXY_PROVIDER,
    },
];

/// Exact frozen secret-classified option names (5 of 52).
pub const SECRET_OPTIONS: [&str; 5] = [
    "proxy_password",
    "leaseset_password",
    "leaseset_blinding_secret",
    "leaseset_client_auth",
    // Plan 342: the upstream outproxy credential. Its presence here is
    // what the daemon's persistence mask uses to keep the plaintext out of
    // a generation file — the sealed stored form is *not* secret-classified
    // at the wire layer, because by the time a value is stored it is
    // ciphertext produced by Plan 341's owner.
    "outproxy_password",
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
