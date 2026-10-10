//! Runtime-neutral router readiness and advertisement decision contracts.
//!
//! These values describe evidence gathered by owning services. A successful
//! [`evaluate_advertisement`] result is a decision result, not an authorization
//! token; RouterInfo builders must continue to require their owner-specific
//! permits and validated records.

/// Deployment profile used to interpret readiness and claim evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RouterProfile {
    /// Isolated tests with controlled endpoints and a non-production network ID.
    IsolatedTest,
    /// Controlled interoperation with explicitly pinned reference peers.
    ControlledInterop,
    /// Ordinary router profile with qualified authenticated transports.
    NormalRouter,
    /// An explicitly enabled optional network role such as transit or floodfill.
    OptionalNetworkRole,
}

/// Ordered router-level readiness milestones.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReadinessStage {
    /// The daemon process and local control plane are serving.
    ProcessServing,
    /// A validated, usable peer database is available.
    PeerDatabaseUsable,
    /// At least one qualified authenticated transport is usable.
    TransportUsable,
    /// Required exploratory and client tunnel pools are usable.
    TunnelPoolsUsable,
    /// Application traffic can be routed end to end.
    ApplicationRoutingUsable,
    /// A subsystem is degraded or a required stage has been lost.
    Degraded,
}

impl ReadinessStage {
    /// Returns whether this stage satisfies the requested minimum stage.
    pub fn satisfies(self, required: Self) -> bool {
        match self {
            Self::Degraded => false,
            _ => self >= required,
        }
    }
}

/// Public-facing claim that a composition root may consider publishing.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AdvertisementClaim {
    /// A reachable, qualified transport address.
    TransportAddress,
    /// Participation in normal-router transit forwarding.
    Transit,
    /// Floodfill lookup and publication service.
    Floodfill,
}

/// Independent facts that must all be true before a claim is considered.
///
/// The caller must derive these facts from current owner state and verified
/// evidence. Configuration values and loopback probes alone are insufficient.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AdvertisementEvidence {
    /// The owning service is healthy in the current generation.
    pub owner_healthy: bool,
    /// The advertised address has current reachability evidence.
    pub address_reachable: bool,
    /// The exact protocol/profile has independent qualification evidence.
    pub protocol_qualified: bool,
    /// The operator explicitly enabled this exposure or role.
    pub operator_authorized: bool,
    /// The evidence belongs to the currently active owner generation.
    pub generation_current: bool,
}

/// Reason a public-facing claim was withheld.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AdvertisementRejection {
    /// The selected deployment profile cannot publish this kind of claim.
    ProfileDisallowsClaim,
    /// The router has not reached the required network readiness stage.
    ReadinessInsufficient,
    /// The owning service is not healthy.
    OwnerUnhealthy,
    /// Reachability has not been established for the address.
    AddressUnreachable,
    /// The protocol/profile lacks independent qualification evidence.
    ProtocolUnqualified,
    /// The operator has not authorized this exposure or role.
    OperatorNotAuthorized,
    /// Evidence belongs to an expired owner generation.
    StaleGeneration,
}

/// Evaluates public claim evidence in a stable, fail-closed order.
///
/// Controlled test profiles never authorize public claims. A normal-router
/// profile may consider only transport addresses; optional roles require the
/// distinct `OptionalNetworkRole` profile. This function deliberately does
/// not create a capability or bypass owner-specific publication checks.
pub fn evaluate_advertisement(
    profile: RouterProfile,
    claim: AdvertisementClaim,
    readiness: ReadinessStage,
    required_readiness: ReadinessStage,
    evidence: AdvertisementEvidence,
) -> Result<(), AdvertisementRejection> {
    let profile_allows = match (profile, claim) {
        (RouterProfile::NormalRouter, AdvertisementClaim::TransportAddress)
        | (RouterProfile::OptionalNetworkRole, _) => true,
        _ => false,
    };
    if !profile_allows {
        return Err(AdvertisementRejection::ProfileDisallowsClaim);
    }
    if !readiness.satisfies(required_readiness) {
        return Err(AdvertisementRejection::ReadinessInsufficient);
    }
    if !evidence.owner_healthy {
        return Err(AdvertisementRejection::OwnerUnhealthy);
    }
    if !evidence.address_reachable {
        return Err(AdvertisementRejection::AddressUnreachable);
    }
    if !evidence.protocol_qualified {
        return Err(AdvertisementRejection::ProtocolUnqualified);
    }
    if !evidence.operator_authorized {
        return Err(AdvertisementRejection::OperatorNotAuthorized);
    }
    if !evidence.generation_current {
        return Err(AdvertisementRejection::StaleGeneration);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qualified() -> AdvertisementEvidence {
        AdvertisementEvidence {
            owner_healthy: true,
            address_reachable: true,
            protocol_qualified: true,
            operator_authorized: true,
            generation_current: true,
        }
    }

    #[test]
    fn process_serving_does_not_imply_network_readiness() {
        assert_eq!(
            evaluate_advertisement(
                RouterProfile::NormalRouter,
                AdvertisementClaim::TransportAddress,
                ReadinessStage::ProcessServing,
                ReadinessStage::TransportUsable,
                qualified(),
            ),
            Err(AdvertisementRejection::ReadinessInsufficient)
        );
    }

    #[test]
    fn controlled_profiles_cannot_authorize_public_claims() {
        for profile in [
            RouterProfile::IsolatedTest,
            RouterProfile::ControlledInterop,
        ] {
            assert_eq!(
                evaluate_advertisement(
                    profile,
                    AdvertisementClaim::TransportAddress,
                    ReadinessStage::ApplicationRoutingUsable,
                    ReadinessStage::TransportUsable,
                    qualified(),
                ),
                Err(AdvertisementRejection::ProfileDisallowsClaim)
            );
        }
    }

    #[test]
    fn optional_roles_require_the_optional_role_profile() {
        assert_eq!(
            evaluate_advertisement(
                RouterProfile::NormalRouter,
                AdvertisementClaim::Floodfill,
                ReadinessStage::ApplicationRoutingUsable,
                ReadinessStage::PeerDatabaseUsable,
                qualified(),
            ),
            Err(AdvertisementRejection::ProfileDisallowsClaim)
        );
    }

    #[test]
    fn every_independent_evidence_fact_is_required() {
        let cases = [
            (
                AdvertisementEvidence {
                    owner_healthy: false,
                    ..qualified()
                },
                AdvertisementRejection::OwnerUnhealthy,
            ),
            (
                AdvertisementEvidence {
                    address_reachable: false,
                    ..qualified()
                },
                AdvertisementRejection::AddressUnreachable,
            ),
            (
                AdvertisementEvidence {
                    protocol_qualified: false,
                    ..qualified()
                },
                AdvertisementRejection::ProtocolUnqualified,
            ),
            (
                AdvertisementEvidence {
                    operator_authorized: false,
                    ..qualified()
                },
                AdvertisementRejection::OperatorNotAuthorized,
            ),
            (
                AdvertisementEvidence {
                    generation_current: false,
                    ..qualified()
                },
                AdvertisementRejection::StaleGeneration,
            ),
        ];
        for (evidence, expected) in cases {
            assert_eq!(
                evaluate_advertisement(
                    RouterProfile::OptionalNetworkRole,
                    AdvertisementClaim::Floodfill,
                    ReadinessStage::ApplicationRoutingUsable,
                    ReadinessStage::PeerDatabaseUsable,
                    evidence,
                ),
                Err(expected)
            );
        }
    }

    #[test]
    fn readiness_stages_have_a_stable_order() {
        assert!(ReadinessStage::TunnelPoolsUsable.satisfies(ReadinessStage::TransportUsable));
        assert!(!ReadinessStage::PeerDatabaseUsable.satisfies(ReadinessStage::TransportUsable));
        assert!(!ReadinessStage::Degraded.satisfies(ReadinessStage::ProcessServing));
    }
}
