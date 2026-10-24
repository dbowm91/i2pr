//! Plan 496 — the profile-aware NTCP2 activation gate.
//!
//! Plan 101 refused every NTCP2 activation that was not the explicit
//! controlled-loopback profile. Plan 496 keeps that refusal for every
//! configuration the old guard rejected, and adds one narrowly-scoped
//! operator profile — a guarded, RFC1918-only, non-public private LAN —
//! whose refusals are named individually instead of being collapsed into a
//! single generic message.
//!
//! This module is deliberately **pure**: it takes an already-normalized
//! [`Ntcp2Config`] and returns either the address scope the transport may
//! dial or a typed refusal. Keeping the decision out of the service closure
//! makes every branch reachable by a unit test, which the in-closure form
//! was not: the configuration layer already refuses every shape the gate
//! refuses, so a gate branch could never be reached from a configuration
//! file.
//!
//! The gate never widens a scope. `ControlledLoopback` admits only loopback
//! addresses; `PrivateLan` admits only RFC1918 IPv4 literals. A peer
//! address outside the returned scope is refused by the runtime's scoped
//! dial admission.

use crate::config::{Ntcp2Config, Ntcp2QualificationProfile};
use i2pr_runtime::Ntcp2DialScope;
use std::net::IpAddr;

/// The reason an NTCP2 activation was refused.
///
/// Every variant is a distinct, named refusal. None of them is
/// recoverable by retrying; each names the configuration that must
/// change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ntcp2ActivationRefusal {
    /// The transport is disabled.
    Disabled,
    /// No explicit qualification profile was selected.
    NoExplicitProfile,
    /// The controlled-loopback profile was selected with a non-loopback
    /// bind address.
    NonLoopbackBind,
    /// The private-LAN profile was selected with an address that is not an
    /// RFC1918 IPv4 literal.
    NonPrivateLanBind,
    /// The selected port is privileged.
    PrivilegedPort,
}

impl Ntcp2ActivationRefusal {
    /// Returns the operator-facing reason text.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Disabled => "NTCP2 is disabled",
            Self::NoExplicitProfile => "NTCP2 requires an explicit qualification profile",
            Self::NonLoopbackBind => {
                "the NTCP2 controlled-loopback profile requires a fixed loopback bind address"
            }
            Self::NonPrivateLanBind => {
                "the NTCP2 private-LAN profile requires an RFC1918 IPv4 bind address"
            }
            Self::PrivilegedPort => "NTCP2 requires an explicit non-privileged listen port",
        }
    }
}

/// Returns the address scope the NTCP2 transport may dial under `config`.
///
/// The returned scope is the widest set of peer addresses the transport
/// may reach. The bind address itself must satisfy the same scope, so a
/// profile can never listen on an address it is not allowed to dial.
pub fn ntcp2_activation_scope(
    config: &Ntcp2Config,
) -> Result<Ntcp2DialScope, Ntcp2ActivationRefusal> {
    if !config.enabled {
        return Err(Ntcp2ActivationRefusal::Disabled);
    }
    let scope = match config.qualification_profile {
        Ntcp2QualificationProfile::Disabled => {
            return Err(Ntcp2ActivationRefusal::NoExplicitProfile);
        }
        Ntcp2QualificationProfile::ControlledLoopback => {
            if !config.bind_address.is_loopback() {
                return Err(Ntcp2ActivationRefusal::NonLoopbackBind);
            }
            Ntcp2DialScope::Loopback
        }
        Ntcp2QualificationProfile::PrivateLanQualification => {
            if !matches!(config.bind_address, IpAddr::V4(ip) if ip.is_private()) {
                return Err(Ntcp2ActivationRefusal::NonPrivateLanBind);
            }
            Ntcp2DialScope::PrivateLan
        }
    };
    if config.listen_port < MIN_NTCP2_ACTIVATION_PORT {
        return Err(Ntcp2ActivationRefusal::PrivilegedPort);
    }
    Ok(scope)
}

/// The lowest NTCP2 listen port the activation gate accepts.
///
/// The configuration layer enforces the same floor; the gate repeats it so
/// the refusal survives a caller that constructs a configuration directly.
pub const MIN_NTCP2_ACTIVATION_PORT: u16 = 1024;

#[cfg(test)]
mod tests {
    use super::*;

    fn config(
        enabled: bool,
        qualification_profile: Ntcp2QualificationProfile,
        bind_address: &str,
        listen_port: u16,
    ) -> Ntcp2Config {
        Ntcp2Config {
            enabled,
            qualification_profile,
            bind_address: bind_address.parse().expect("bind address"),
            listen_port,
            connect_timeout: std::time::Duration::from_millis(5_000),
            handshake_timeout: std::time::Duration::from_millis(30_000),
            read_idle_timeout: std::time::Duration::from_millis(120_000),
            write_timeout: std::time::Duration::from_millis(30_000),
            queue_wait_timeout: std::time::Duration::from_millis(5_000),
            drain_timeout: std::time::Duration::from_millis(5_000),
            max_active_links: 128,
            max_replay_entries: 256,
            ipv4_prefix: 24,
            ipv6_prefix: 64,
        }
    }

    #[test]
    fn the_controlled_profile_admits_only_loopback() {
        let scope = ntcp2_activation_scope(&config(
            true,
            Ntcp2QualificationProfile::ControlledLoopback,
            "127.0.0.1",
            40123,
        ))
        .expect("controlled profile");
        assert_eq!(scope, Ntcp2DialScope::Loopback);
    }

    #[test]
    fn the_private_lan_profile_admits_only_rfc1918_ipv4() {
        let scope = ntcp2_activation_scope(&config(
            true,
            Ntcp2QualificationProfile::PrivateLanQualification,
            "192.168.10.25",
            40124,
        ))
        .expect("private-LAN profile");
        assert_eq!(scope, Ntcp2DialScope::PrivateLan);
    }

    #[test]
    fn every_refusal_names_its_own_reason() {
        let cases: [(Ntcp2Config, Ntcp2ActivationRefusal); 5] = [
            (
                config(
                    false,
                    Ntcp2QualificationProfile::ControlledLoopback,
                    "127.0.0.1",
                    40123,
                ),
                Ntcp2ActivationRefusal::Disabled,
            ),
            (
                config(
                    true,
                    Ntcp2QualificationProfile::Disabled,
                    "127.0.0.1",
                    40123,
                ),
                Ntcp2ActivationRefusal::NoExplicitProfile,
            ),
            (
                config(
                    true,
                    Ntcp2QualificationProfile::ControlledLoopback,
                    "192.168.10.25",
                    40123,
                ),
                Ntcp2ActivationRefusal::NonLoopbackBind,
            ),
            (
                config(
                    true,
                    Ntcp2QualificationProfile::PrivateLanQualification,
                    "8.8.8.8",
                    40124,
                ),
                Ntcp2ActivationRefusal::NonPrivateLanBind,
            ),
            (
                config(
                    true,
                    Ntcp2QualificationProfile::ControlledLoopback,
                    "127.0.0.1",
                    443,
                ),
                Ntcp2ActivationRefusal::PrivilegedPort,
            ),
        ];
        for (config, expected) in cases {
            assert_eq!(
                ntcp2_activation_scope(&config),
                Err(expected),
                "{expected:?}"
            );
        }
    }

    #[test]
    fn the_private_lan_profile_refuses_every_non_rfc1918_ipv4_shape() {
        for address in [
            "127.0.0.1",
            "0.0.0.0",
            "8.8.8.8",
            "192.0.2.1",
            "172.32.0.1",
            "10.0.0.1",
            "fd00::1",
            "::1",
        ] {
            let config = config(
                true,
                Ntcp2QualificationProfile::PrivateLanQualification,
                address,
                40124,
            );
            let expected = if address == "10.0.0.1" {
                Ok(Ntcp2DialScope::PrivateLan)
            } else {
                Err(Ntcp2ActivationRefusal::NonPrivateLanBind)
            };
            assert_eq!(ntcp2_activation_scope(&config), expected, "{address}");
        }
    }

    #[test]
    fn the_refusals_are_not_vacuous() {
        // An inverted probe: if the gate accepted every shape, this table
        // would fail. Each case is the *nearest accepted* neighbour of the
        // refused one, so a weakened check shows up here as an unexpected
        // acceptance.
        assert!(
            ntcp2_activation_scope(&config(
                true,
                Ntcp2QualificationProfile::PrivateLanQualification,
                "10.0.0.1",
                40124,
            ))
            .is_ok(),
            "the nearest accepted neighbour must still be accepted"
        );
        assert!(
            ntcp2_activation_scope(&config(
                true,
                Ntcp2QualificationProfile::ControlledLoopback,
                "127.0.0.1",
                40123,
            ))
            .is_ok(),
            "the nearest accepted neighbour must still be accepted"
        );
        assert!(
            ntcp2_activation_scope(&config(
                true,
                Ntcp2QualificationProfile::ControlledLoopback,
                "127.0.0.1",
                1023,
            ))
            .is_err(),
            "a privileged port must still be refused"
        );
    }

    #[test]
    fn the_gate_never_widens_a_scope() {
        // The controlled profile must not admit a private-LAN address, and
        // the private-LAN profile must not admit a public address. The
        // returned scope is the only thing the runtime may dial inside, so
        // this is the boundary that matters.
        let controlled = ntcp2_activation_scope(&config(
            true,
            Ntcp2QualificationProfile::ControlledLoopback,
            "127.0.0.1",
            40123,
        ))
        .expect("controlled profile");
        assert!(!controlled.admits("192.168.10.25".parse().expect("address")));
        assert!(!controlled.admits("8.8.8.8".parse().expect("address")));

        let private_lan = ntcp2_activation_scope(&config(
            true,
            Ntcp2QualificationProfile::PrivateLanQualification,
            "192.168.10.25",
            40124,
        ))
        .expect("private-LAN profile");
        assert!(!private_lan.admits("127.0.0.1".parse().expect("address")));
        assert!(!private_lan.admits("8.8.8.8".parse().expect("address")));
        assert!(private_lan.admits("192.168.10.25".parse().expect("address")));
    }
}
