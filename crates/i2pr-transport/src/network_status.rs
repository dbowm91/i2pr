//! Plan 339 per-family network condition codes.
//!
//! Proposal 170 declares five canonical RouterInfo selectors that are each
//! marked **"(adopted from i2pd)"**:
//!
//! - `i2p.router.net.status.v6` — IPv6 network status code, `int`
//! - `i2p.router.net.error` — IPv4 network error code, `int`
//! - `i2p.router.net.error.v6` — IPv6 network error code, `int`
//! - `i2p.router.net.testing` — IPv4 testing state, `0` or `1`, `int`
//! - `i2p.router.net.testing.v6` — IPv6 testing state, `0` or `1`, `int`
//!
//! Because the Proposal defers the vocabulary to i2pd, the enumerations here
//! are i2pd's, pinned to i2pd `2c694149fa6996eaeb23e378d5f83c9d3232c22f`
//! `libi2pd/RouterContext.h:44-72`, where `daemon/I2PControlHandlers.cpp:41-46`
//! projects them into exactly these keys as `(int)`. i2pd is read as a
//! reference only: nothing is vendored, patched, or reused.
//!
//! What is *i2pr's* is the emission policy — the conditions under which a code
//! is produced. That policy is deliberately conservative and lives in the pure
//! derivations below:
//!
//! - A reachability snapshot is qualified for exactly **one** address family.
//!   A snapshot qualified for IPv4 says nothing about IPv6, so the IPv6
//!   selectors report [`NetworkStatusCode::Unknown`] unless the qualified
//!   family is itself IPv6. This is what prevents a fabricated `OK`.
//! - A stale snapshot (`expires_at <= now`) supports no claim, exactly as in
//!   the publication path.
//! - i2pr owns no detector for [`NetworkErrorCode::ClockSkew`],
//!   [`NetworkErrorCode::SymmetricNat`], or [`NetworkErrorCode::FullConeNat`],
//!   and no Proxy/Mesh/Stan posture, so those codes are never produced. The
//!   variants exist only so the wire enumeration is complete and a decoded
//!   value round-trips.
//!
//! The honest ordinary-production baseline is therefore
//! `status = Unknown (2)`, `error = None (0)`, `testing = 0` — no claim made,
//! and no test running.
//!
//! This module is runtime-neutral and fully synchronous: no I/O, no sockets,
//! no Tokio, no executor, and no request-time router scan. Every value is
//! `Copy` and bounded.

use std::time::Duration;

use crate::reachability::{ReachabilitySnapshot, ReachabilityState};
use crate::types::AddressFamily;

/// i2pd `RouterStatus` network status code, as adopted by Proposal 170.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NetworkStatusCode {
    /// `0` — `eRouterStatusOK`.
    Ok,
    /// `1` — `eRouterStatusFirewalled`.
    Firewalled,
    /// `2` — `eRouterStatusUnknown`.
    Unknown,
    /// `3` — `eRouterStatusProxy`. Never produced by i2pr.
    Proxy,
    /// `4` — `eRouterStatusMesh`. Never produced by i2pr.
    Mesh,
    /// `5` — `eRouterStatusStan`. Never produced by i2pr.
    Stan,
}

impl NetworkStatusCode {
    /// Lowest value in the adopted enumeration.
    pub const MIN: i64 = 0;
    /// Highest value in the adopted enumeration.
    pub const MAX: i64 = 5;

    /// Returns the exact wire value.
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Ok => 0,
            Self::Firewalled => 1,
            Self::Unknown => 2,
            Self::Proxy => 3,
            Self::Mesh => 4,
            Self::Stan => 5,
        }
    }

    /// Decodes a wire value, rejecting anything outside the enumeration.
    ///
    /// A value outside `MIN..=MAX` is an error, never clamped into range: a
    /// caller must not silently widen a malformed source into a valid claim.
    pub const fn try_from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Ok),
            1 => Some(Self::Firewalled),
            2 => Some(Self::Unknown),
            3 => Some(Self::Proxy),
            4 => Some(Self::Mesh),
            5 => Some(Self::Stan),
            _ => None,
        }
    }

    /// Returns the i2pd spelling of this code, for diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Firewalled => "Firewalled",
            Self::Unknown => "Unknown",
            Self::Proxy => "Proxy",
            Self::Mesh => "Mesh",
            Self::Stan => "Stan",
        }
    }
}

/// i2pd `RouterError` network error code, as adopted by Proposal 170.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NetworkErrorCode {
    /// `0` — `eRouterErrorNone`.
    None,
    /// `1` — `eRouterErrorClockSkew`. Never produced by i2pr.
    ClockSkew,
    /// `2` — `eRouterErrorOffline`.
    Offline,
    /// `3` — `eRouterErrorSymmetricNAT`. Never produced by i2pr.
    SymmetricNat,
    /// `4` — `eRouterErrorFullConeNAT`. Never produced by i2pr.
    FullConeNat,
    /// `5` — `eRouterErrorNoDescriptors`.
    NoDescriptors,
}

impl NetworkErrorCode {
    /// Lowest value in the adopted enumeration.
    pub const MIN: i64 = 0;
    /// Highest value in the adopted enumeration.
    pub const MAX: i64 = 5;

    /// Returns the exact wire value.
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::None => 0,
            Self::ClockSkew => 1,
            Self::Offline => 2,
            Self::SymmetricNat => 3,
            Self::FullConeNat => 4,
            Self::NoDescriptors => 5,
        }
    }

    /// Decodes a wire value, rejecting anything outside the enumeration.
    pub const fn try_from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::ClockSkew),
            2 => Some(Self::Offline),
            3 => Some(Self::SymmetricNat),
            4 => Some(Self::FullConeNat),
            5 => Some(Self::NoDescriptors),
            _ => None,
        }
    }

    /// Returns the i2pd spelling of this code, for diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::ClockSkew => "ClockSkew",
            Self::Offline => "Offline",
            Self::SymmetricNat => "SymmetricNAT",
            Self::FullConeNat => "FullConeNAT",
            Self::NoDescriptors => "NoDescriptors",
        }
    }
}

/// Privacy-safe per-address-family network condition.
///
/// Carries no endpoint, address, key, peer identity, token, or payload — only
/// booleans and an enum, so it is safe to project onto the control surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FamilyNetworkCondition {
    /// Daemon configuration requested a socket for this family.
    ///
    /// A family that was never configured is **not** a fault, so it never
    /// produces [`NetworkErrorCode::Offline`].
    pub configured: bool,
    /// A socket for this family is currently bound, i.e. the transport for
    /// this family is running.
    pub bound: bool,
    /// Effective reachability **for this family**. A snapshot qualified for
    /// the other family, a stale snapshot, or the absence of a snapshot all
    /// yield [`ReachabilityState::Unknown`] — never the other family's state.
    pub reachability: ReachabilityState,
}

impl FamilyNetworkCondition {
    /// Returns the condition for a family that is neither configured nor
    /// bound and has made no claim.
    pub const fn inert() -> Self {
        Self {
            configured: false,
            bound: false,
            reachability: ReachabilityState::Unknown,
        }
    }
}

/// Returns the effective reachability of one family.
///
/// The snapshot is honoured only when it was observed **for that family** and
/// has not expired. `now` is monotonic and compared against
/// [`ReachabilitySnapshot::expires_at`], mirroring the publication path's
/// `expiry <= now` rejection.
pub fn effective_reachability(
    snapshot: Option<&ReachabilitySnapshot>,
    family: AddressFamily,
    now: Duration,
) -> ReachabilityState {
    match snapshot {
        Some(snapshot) if snapshot.family == family && snapshot.expires_at > now => snapshot.state,
        _ => ReachabilityState::Unknown,
    }
}

/// Derives the Proposal status code for one family.
///
/// Only corroborated evidence moves the code off
/// [`NetworkStatusCode::Unknown`]; a foreign-family, stale, or absent snapshot
/// reports `Unknown` rather than inheriting a claim it does not support.
pub const fn network_status_code(condition: &FamilyNetworkCondition) -> NetworkStatusCode {
    match condition.reachability {
        ReachabilityState::Reachable => NetworkStatusCode::Ok,
        ReachabilityState::Firewalled | ReachabilityState::Unreachable => {
            NetworkStatusCode::Firewalled
        }
        ReachabilityState::Unknown
        | ReachabilityState::ObservedUnconfirmed
        | ReachabilityState::CandidateReachable => NetworkStatusCode::Unknown,
    }
}

/// Derives the Proposal `testing` flag for one family: `1` while a
/// determination is genuinely in progress, `0` otherwise.
///
/// A router that has never observed anything is **not** testing, so it reports
/// `0`; only `ObservedUnconfirmed` and `CandidateReachable` mean evidence
/// exists and confirmation is still pending.
pub const fn network_testing_flag(condition: &FamilyNetworkCondition) -> i64 {
    match condition.reachability {
        ReachabilityState::ObservedUnconfirmed | ReachabilityState::CandidateReachable => 1,
        ReachabilityState::Unknown
        | ReachabilityState::Reachable
        | ReachabilityState::Firewalled
        | ReachabilityState::Unreachable => 0,
    }
}

/// Derives the Proposal error code for one family.
///
/// `netdb_usable` is the caller's attested NetDB peer observation; an empty
/// peer set is a router-global condition reported on both family selectors,
/// matching i2pd's per-family `m_Error`/`m_ErrorV6` structure. i2pr owns no
/// clock-skew or NAT-type detector, so those codes are never produced.
pub const fn network_error_code(
    condition: &FamilyNetworkCondition,
    netdb_usable: bool,
) -> NetworkErrorCode {
    if !netdb_usable {
        return NetworkErrorCode::NoDescriptors;
    }
    if condition.configured && !condition.bound {
        return NetworkErrorCode::Offline;
    }
    NetworkErrorCode::None
}

#[cfg(test)]
mod tests {
    use super::{
        AddressFamily, FamilyNetworkCondition, NetworkErrorCode, NetworkStatusCode,
        ReachabilitySnapshot, ReachabilityState, effective_reachability, network_error_code,
        network_status_code, network_testing_flag,
    };
    use std::time::Duration;

    const ALL_STATES: [ReachabilityState; 6] = [
        ReachabilityState::Unknown,
        ReachabilityState::ObservedUnconfirmed,
        ReachabilityState::CandidateReachable,
        ReachabilityState::Reachable,
        ReachabilityState::Firewalled,
        ReachabilityState::Unreachable,
    ];

    fn snapshot(
        state: ReachabilityState,
        family: AddressFamily,
        expires_at: Duration,
    ) -> ReachabilitySnapshot {
        ReachabilitySnapshot {
            state,
            corroboration: 2,
            expires_at,
            family,
        }
    }

    fn condition(state: ReachabilityState) -> FamilyNetworkCondition {
        FamilyNetworkCondition {
            configured: true,
            bound: true,
            reachability: state,
        }
    }

    #[test]
    fn adopted_enumerations_round_trip_exactly() {
        for value in NetworkStatusCode::MIN..=NetworkStatusCode::MAX {
            let code = NetworkStatusCode::try_from_i64(value).expect("in-range status decodes");
            assert_eq!(code.as_i64(), value, "status round trip at {value}");
        }
        for value in NetworkErrorCode::MIN..=NetworkErrorCode::MAX {
            let code = NetworkErrorCode::try_from_i64(value).expect("in-range error decodes");
            assert_eq!(code.as_i64(), value, "error round trip at {value}");
        }
    }

    #[test]
    fn out_of_range_wire_values_are_rejected_not_clamped() {
        for value in [-1_i64, 6, 7, i64::MAX, i64::MIN] {
            assert_eq!(
                NetworkStatusCode::try_from_i64(value),
                None,
                "status {value} must not clamp into the enumeration"
            );
            assert_eq!(
                NetworkErrorCode::try_from_i64(value),
                None,
                "error {value} must not clamp into the enumeration"
            );
        }
    }

    #[test]
    fn enumeration_matches_the_pinned_i2pd_definitions() {
        // i2pd RouterContext.h:44-52 and :64-72 at 2c69414.
        assert_eq!(NetworkStatusCode::Ok.as_i64(), 0);
        assert_eq!(NetworkStatusCode::Firewalled.as_i64(), 1);
        assert_eq!(NetworkStatusCode::Unknown.as_i64(), 2);
        assert_eq!(NetworkStatusCode::Proxy.as_i64(), 3);
        assert_eq!(NetworkStatusCode::Mesh.as_i64(), 4);
        assert_eq!(NetworkStatusCode::Stan.as_i64(), 5);

        assert_eq!(NetworkErrorCode::None.as_i64(), 0);
        assert_eq!(NetworkErrorCode::ClockSkew.as_i64(), 1);
        assert_eq!(NetworkErrorCode::Offline.as_i64(), 2);
        assert_eq!(NetworkErrorCode::SymmetricNat.as_i64(), 3);
        assert_eq!(NetworkErrorCode::FullConeNat.as_i64(), 4);
        assert_eq!(NetworkErrorCode::NoDescriptors.as_i64(), 5);

        assert_eq!(NetworkStatusCode::Unknown.name(), "Unknown");
        assert_eq!(NetworkErrorCode::FullConeNat.name(), "FullConeNAT");
    }

    #[test]
    fn effective_reachability_never_inherits_another_family() {
        let now = Duration::from_secs(1_000);
        let v4 = snapshot(
            ReachabilityState::Reachable,
            AddressFamily::Ipv4,
            now + Duration::from_secs(60),
        );
        assert_eq!(
            effective_reachability(Some(&v4), AddressFamily::Ipv4, now),
            ReachabilityState::Reachable,
            "a fresh same-family snapshot is honoured"
        );
        assert_eq!(
            effective_reachability(Some(&v4), AddressFamily::Ipv6, now),
            ReachabilityState::Unknown,
            "an IPv4-qualified snapshot says nothing about IPv6"
        );
    }

    #[test]
    fn effective_reachability_rejects_stale_and_absent_snapshots() {
        let now = Duration::from_secs(1_000);
        let stale = snapshot(ReachabilityState::Reachable, AddressFamily::Ipv4, now);
        assert_eq!(
            effective_reachability(Some(&stale), AddressFamily::Ipv4, now),
            ReachabilityState::Unknown,
            "expiry == now supports no claim, matching the publication path"
        );
        let expired = snapshot(
            ReachabilityState::Reachable,
            AddressFamily::Ipv4,
            now - Duration::from_secs(1),
        );
        assert_eq!(
            effective_reachability(Some(&expired), AddressFamily::Ipv4, now),
            ReachabilityState::Unknown,
            "an expired snapshot supports no claim"
        );
        assert_eq!(
            effective_reachability(None, AddressFamily::Ipv4, now),
            ReachabilityState::Unknown,
            "no snapshot supports no claim"
        );
    }

    #[test]
    fn status_code_table_covers_every_reachability_state() {
        let expected = [
            (ReachabilityState::Unknown, NetworkStatusCode::Unknown),
            (
                ReachabilityState::ObservedUnconfirmed,
                NetworkStatusCode::Unknown,
            ),
            (
                ReachabilityState::CandidateReachable,
                NetworkStatusCode::Unknown,
            ),
            (ReachabilityState::Reachable, NetworkStatusCode::Ok),
            (ReachabilityState::Firewalled, NetworkStatusCode::Firewalled),
            (
                ReachabilityState::Unreachable,
                NetworkStatusCode::Firewalled,
            ),
        ];
        for (state, code) in expected {
            assert_eq!(
                network_status_code(&condition(state)),
                code,
                "status for {state:?}"
            );
        }
    }

    #[test]
    fn status_never_leaves_unknown_without_corroborated_evidence() {
        for state in ALL_STATES {
            let code = network_status_code(&condition(state));
            if !matches!(
                state,
                ReachabilityState::Reachable
                    | ReachabilityState::Firewalled
                    | ReachabilityState::Unreachable
            ) {
                assert_eq!(
                    code,
                    NetworkStatusCode::Unknown,
                    "{state:?} must not claim OK or Firewalled"
                );
            }
        }
    }

    #[test]
    fn proxy_mesh_and_stan_are_never_derived() {
        for state in ALL_STATES {
            let code = network_status_code(&condition(state));
            assert!(
                !matches!(
                    code,
                    NetworkStatusCode::Proxy | NetworkStatusCode::Mesh | NetworkStatusCode::Stan
                ),
                "{state:?} must not produce a posture i2pr does not have ({code:?})"
            );
        }
    }

    #[test]
    fn testing_flag_is_one_only_while_a_determination_is_pending() {
        for state in ALL_STATES {
            let flag = network_testing_flag(&condition(state));
            match state {
                ReachabilityState::ObservedUnconfirmed | ReachabilityState::CandidateReachable => {
                    assert_eq!(flag, 1, "{state:?} has evidence and awaits confirmation")
                }
                _ => assert_eq!(
                    flag, 0,
                    "{state:?} is not an in-progress determination, got {flag}"
                ),
            }
            assert!(
                flag == 0 || flag == 1,
                "the Proposal declares a 0/1 testing flag, got {flag}"
            );
        }
    }

    #[test]
    fn an_untested_router_is_not_reporting_itself_as_testing() {
        let inert = FamilyNetworkCondition::inert();
        assert_eq!(network_testing_flag(&inert), 0);
        assert_eq!(
            network_status_code(&inert),
            NetworkStatusCode::Unknown,
            "no observation may not become OK"
        );
        assert_eq!(network_error_code(&inert, true), NetworkErrorCode::None);
    }

    #[test]
    fn error_code_is_offline_only_when_configured_and_unbound() {
        let running = FamilyNetworkCondition {
            configured: true,
            bound: true,
            reachability: ReachabilityState::Unknown,
        };
        assert_eq!(network_error_code(&running, true), NetworkErrorCode::None);

        let down = FamilyNetworkCondition {
            configured: true,
            bound: false,
            reachability: ReachabilityState::Unknown,
        };
        assert_eq!(
            network_error_code(&down, true),
            NetworkErrorCode::Offline,
            "a configured family with no bound socket is offline"
        );

        let never_configured = FamilyNetworkCondition {
            configured: false,
            bound: false,
            reachability: ReachabilityState::Unknown,
        };
        assert_eq!(
            network_error_code(&never_configured, true),
            NetworkErrorCode::None,
            "a family that was never configured is not a fault"
        );
    }

    #[test]
    fn an_empty_netdb_reports_no_descriptors_on_both_families() {
        let running = FamilyNetworkCondition {
            configured: true,
            bound: true,
            reachability: ReachabilityState::Reachable,
        };
        assert_eq!(
            network_error_code(&running, false),
            NetworkErrorCode::NoDescriptors
        );
        // Offline is subordinate: an empty NetDB is the stronger truth.
        let down = FamilyNetworkCondition {
            configured: true,
            bound: false,
            reachability: ReachabilityState::Unknown,
        };
        assert_eq!(
            network_error_code(&down, false),
            NetworkErrorCode::NoDescriptors
        );
    }

    #[test]
    fn undetectable_error_codes_are_never_derived() {
        for state in ALL_STATES {
            for configured in [false, true] {
                for bound in [false, true] {
                    for netdb in [false, true] {
                        let condition = FamilyNetworkCondition {
                            configured,
                            bound,
                            reachability: state,
                        };
                        let code = network_error_code(&condition, netdb);
                        assert!(
                            !matches!(
                                code,
                                NetworkErrorCode::ClockSkew
                                    | NetworkErrorCode::SymmetricNat
                                    | NetworkErrorCode::FullConeNat
                            ),
                            "i2pr owns no clock-skew or NAT-type detector, got {code:?}"
                        );
                    }
                }
            }
        }
    }
}
