//! Plan 290 strict HTTP CONNECT client profile options.
//!
//! The `connect-client` Proposal 170 family (`CONNECT` in Java
//! I2PTunnel) builds a TCP tunnel with the HTTP `CONNECT` method,
//! typically for TLS/HTTPS. Unlike `http-client` it never forwards
//! ordinary proxy requests: any non-`CONNECT` method is rejected
//! with a bounded `405` error response before any I2P work starts.
//!
//! This module owns only the runtime-neutral option surface (the
//! bounded CONNECT allowed-port policy). Header parsing, authority
//! validation, and the CONNECT executor stay shared with the
//! `http` module and the daemon HTTP executor; this crate only
//! validates configuration structurally.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use crate::errors::ServiceTunnelError;

/// Maximum CONNECT-specific options-set entries (allowed ports).
pub const CONNECT_OPTIONS_MAX_PORTS: usize = 16;

/// Default CONNECT target port (HTTPS port 443, identical to the
/// M10 HTTP/SOCKS CONNECT default policy).
pub const CONNECT_DEFAULT_PORT: u16 = 443;

/// Strict CONNECT profile options attached to a
/// `ServiceTunnelSpec` whose `kind` is `ConnectClient`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectClientOptions {
    /// Allowed CONNECT target ports. Empty set is rejected at
    /// validation; port 443 is the default.
    pub connect_allowed_ports: BTreeSet<u16>,
    /// Listener proxy authentication (Plan 292). When set, the
    /// listener answers unauthenticated CONNECT requests with 407;
    /// when unset, the listener stays open (pre-292 behavior).
    pub proxy_auth: Option<crate::auth::ProxyCredentials>,
    /// Plan 342: the I2P-routed outproxy provider policy.
    ///
    /// A CONNECT client is the kind that most needs it: every one of its
    /// targets is a `host:port` authority and is either an I2P destination
    /// (bypasses the outproxy) or a clearnet authority that has no other
    /// route. Route policy only, never the credential.
    pub outproxy: Option<crate::outproxy::OutproxyConfig>,
}

impl ConnectClientOptions {
    /// Returns the Plan 290 default policy (HTTPS only).
    pub fn defaults() -> Self {
        Self::default()
    }

    /// Validates the CONNECT profile options structurally.
    pub fn validate(&self) -> Result<(), ServiceTunnelError> {
        if let Some(outproxy) = &self.outproxy
            && let Err(_error) = outproxy.validate()
        {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "ProxyList is malformed, empty, or its SSLProxies subset is not within it",
            });
        }
        if self.connect_allowed_ports.is_empty() {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "CONNECT allowed ports must contain at least one port",
            });
        }
        if self.connect_allowed_ports.len() > CONNECT_OPTIONS_MAX_PORTS {
            return Err(ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "CONNECT allowed ports exceed the per-tunnel ceiling",
            });
        }
        Ok(())
    }

    /// Returns `true` when the supplied port is allowed by the
    /// CONNECT port policy.
    pub fn allows_connect_port(&self, port: u16) -> bool {
        self.connect_allowed_ports.contains(&port)
    }
}

impl Default for ConnectClientOptions {
    fn default() -> Self {
        let mut ports = BTreeSet::new();
        ports.insert(CONNECT_DEFAULT_PORT);
        Self {
            connect_allowed_ports: ports,
            proxy_auth: None,
            outproxy: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_allow_https_only() {
        let options = ConnectClientOptions::defaults();
        assert!(options.allows_connect_port(443));
        assert!(!options.allows_connect_port(80));
        options.validate().expect("defaults validate");
    }

    #[test]
    fn empty_port_set_rejected() {
        let options = ConnectClientOptions {
            connect_allowed_ports: BTreeSet::new(),
            proxy_auth: None,
            outproxy: None,
        };
        assert!(options.validate().is_err());
    }

    #[test]
    fn oversized_port_set_rejected() {
        let options = ConnectClientOptions {
            connect_allowed_ports: (1..=(CONNECT_OPTIONS_MAX_PORTS as u16 + 1)).collect(),
            proxy_auth: None,
            outproxy: None,
        };
        assert!(options.validate().is_err());
    }
}
