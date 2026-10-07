//! Plan 369 §9 — manager-created launch authority.
//!
//! This is the trusted manager-side type that bundles everything one permitted
//! application launch needs: principal, effective capabilities, launch profile,
//! root/entrypoint, argv, environment, resource request, and gateway limits.
//!
//! # What this type deliberately cannot do
//!
//! It has **no decoder**. There is no `Deserialize`, no `from_bytes`, no
//! `from_app_message`, and no `from_manifest` — on this type *or* on
//! [`AuthorityRequest`], whose fields are public only so a Rust caller can
//! assemble one. Nothing turns wire bytes into either.
//!
//! [`AuthorityRequest`]'s fields are public because a launch owner in a later
//! plan will need to build one from a package record; that is a Rust call, not a
//! decode. The property that matters is asserted at compile time by
//! `tests/authority_seal.rs`, so a later `#[derive(Deserialize)]` cannot quietly
//! reopen it.
//!
//! # What the administrator gate does and does not prove
//!
//! `AdministratorPrincipal::from_authenticated_session` is a **typed** gate, not
//! an authorization gate: Plan 369 has no administrator, no grant store, and no
//! policy engine, so any caller can construct one. Stating that plainly matters
//! more than pretending otherwise. What the gate does buy is narrow and real:
//!
//! - effective capabilities can only be assembled through
//!   `GrantedCapability::from_administrator_policy`, which refuses
//!   `Capability::BrokeredTcp` outright, and through `EffectiveCapabilities`,
//!   which enforces the ceiling and canonical ordering;
//! - a capability the operator never listed is not in `EffectiveCapabilities`,
//!   so the session cannot open a service that requires it.
//!
//! The outer gate — that nothing in production ever calls [`LaunchAuthority::new`]
//! at all — is `LaunchCatalog::empty()` being the only value the shipped
//! `i2pr-appd` binary owns. See [`crate::catalog`].

use std::collections::BTreeSet;

use i2pr_app_manager_proto::apphost::{
    DescriptiveResourceRequest, Entrypoint, LaunchRequest, LaunchRoot, SanitizedEnvironment,
};
use i2pr_app_manager_proto::{EffectiveGrant, ManagerGatewayLimits, ManagerPrincipal};
use i2pr_app_proto::{
    AdministratorPrincipal, AppInstanceId, Capability, ContractError, EffectiveCapabilities,
    GrantedCapability, LaunchProfile, MAX_CAPABILITIES,
};

use crate::AppdError;

/// Everything a launch owner supplies when it asks for one launch.
///
/// This is a plain assembly struct, not a wire type. It has no serde derives and
/// `tests/authority_seal.rs` proves it never gains one.
#[derive(Clone, Debug)]
pub struct AuthorityRequest {
    pub principal: ManagerPrincipal,
    /// The capabilities this launch is *permitted* to hold. Every entry must
    /// survive the administrator grant path; anything reserved is refused here
    /// rather than at session time.
    pub capabilities: Vec<Capability>,
    pub launch_profile: LaunchProfile,
    pub root: LaunchRoot,
    pub entrypoint: Entrypoint,
    pub argv: Vec<String>,
    pub environment: SanitizedEnvironment,
    pub resources: DescriptiveResourceRequest,
    /// Gateway connections this session may hold. Clamped again by the daemon.
    pub max_connections: u32,
}

/// One trusted, immutable permission to launch one application.
#[derive(Clone, Debug)]
pub struct LaunchAuthority {
    principal: ManagerPrincipal,
    instance_id: AppInstanceId,
    effective: EffectiveCapabilities,
    launch_profile: LaunchProfile,
    root: LaunchRoot,
    entrypoint: Entrypoint,
    argv: Vec<String>,
    environment: SanitizedEnvironment,
    resources: DescriptiveResourceRequest,
    gateway_limits: ManagerGatewayLimits,
}

impl LaunchAuthority {
    /// Builds an authority from an administrator-origin decision.
    ///
    /// Every gate that can be evaluated without a filesystem or a process is
    /// evaluated here, including the Plan 369 §2 `Secured` refusal, so no
    /// authority value can exist for a launch that apphost would refuse at exec
    /// time. Failing later would mean a launch was admitted by the manager and
    /// then killed by the host, which reads as two decisions where there is one.
    pub fn new(
        administrator: &AdministratorPrincipal,
        request: AuthorityRequest,
    ) -> Result<Self, AppdError> {
        let AuthorityRequest {
            principal,
            capabilities,
            launch_profile,
            root,
            entrypoint,
            argv,
            environment,
            resources,
            max_connections,
        } = request;

        // Canonical form is resolved once, here, so the value the application
        // declares in `hello` and the value the daemon is told about cannot
        // differ in spelling.
        let instance_id = principal
            .instance_id
            .to_app_instance_id()
            .map_err(|_| AppdError::Authority(ContractError::InvalidOpaqueId))?;

        let grants = effective_grants(administrator, &capabilities)?;
        let effective =
            EffectiveCapabilities::from_grants(&grants).map_err(AppdError::Authority)?;
        let gateway_limits =
            ManagerGatewayLimits::new(max_connections).map_err(AppdError::Protocol)?;

        let authority = Self {
            principal,
            instance_id,
            effective,
            launch_profile,
            root,
            entrypoint,
            argv,
            environment,
            resources,
            gateway_limits,
        };

        // The bootstrap contract is the single launch gate. Running it here
        // means `Secured`, an escaping entrypoint, and oversize argv all fail
        // before an authority value exists at all.
        authority
            .to_launch_request()
            .validate()
            .map_err(AppdError::Bootstrap)?;

        Ok(authority)
    }

    pub const fn principal(&self) -> &ManagerPrincipal {
        &self.principal
    }

    /// The instance id the application must declare in `hello`.
    pub const fn instance_id(&self) -> &AppInstanceId {
        &self.instance_id
    }

    /// Immutable effective capabilities. There is deliberately no `&mut` path:
    /// §11 requires that an application cannot change them in place, and the
    /// cheapest way to keep that true is for the owner to be unreachable.
    pub fn capabilities(&self) -> &[Capability] {
        self.effective.as_slice()
    }

    pub const fn launch_profile(&self) -> LaunchProfile {
        self.launch_profile
    }

    pub const fn gateway_limits(&self) -> ManagerGatewayLimits {
        self.gateway_limits
    }

    /// True when this launch may open `capability`.
    pub fn permits(&self, capability: Capability) -> bool {
        self.effective.as_slice().contains(&capability)
    }

    /// The same capability set in the manager protocol's wire shape.
    ///
    /// This is what `create_session` sends to the daemon. The daemon re-derives
    /// authority through its own administrator path, so this is an assertion by
    /// the manager, not an authorization.
    pub fn effective_grants(&self) -> Vec<EffectiveGrant> {
        self.effective
            .as_slice()
            .iter()
            .map(|capability| EffectiveGrant {
                capability: *capability,
            })
            .collect()
    }

    /// The single bounded bootstrap request this authority authorizes.
    pub fn to_launch_request(&self) -> LaunchRequest {
        LaunchRequest {
            principal: self.principal.clone(),
            launch_profile: self.launch_profile,
            root: self.root.clone(),
            entrypoint: self.entrypoint.clone(),
            argv: self.argv.clone(),
            environment: self.environment.clone(),
            resources: self.resources,
        }
    }
}

/// Assembles effective capabilities through the administrator grant path.
///
/// The duplicate check is not cosmetic: `EffectiveCapabilities::from_grants`
/// silently deduplicates, so a caller asking for `Sam` twice would otherwise
/// receive one grant and could reasonably believe it had two.
fn effective_grants(
    administrator: &AdministratorPrincipal,
    capabilities: &[Capability],
) -> Result<Vec<GrantedCapability>, AppdError> {
    if capabilities.len() > MAX_CAPABILITIES {
        return Err(AppdError::Authority(ContractError::LimitExceeded(
            "capabilities",
        )));
    }
    let mut seen = BTreeSet::new();
    let mut grants = Vec::with_capacity(capabilities.len());
    for capability in capabilities {
        if !seen.insert(*capability) {
            return Err(AppdError::Authority(ContractError::InvalidControl));
        }
        grants.push(
            GrantedCapability::from_administrator_policy(administrator, *capability)
                .map_err(AppdError::Authority)?,
        );
    }
    Ok(grants)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use i2pr_app_manager_proto::ManagerInstanceId;
    use i2pr_app_manager_proto::ManagerProtocolError;
    use i2pr_app_manager_proto::apphost::{
        ApphostBootstrapError, Entrypoint, LaunchRoot, SanitizedEnvironment,
    };
    use i2pr_app_proto::{AdministratorPrincipal, AppId, LaunchProfile};

    use super::*;

    fn request(
        capabilities: Vec<Capability>,
        profile: LaunchProfile,
        entrypoint: &str,
    ) -> AuthorityRequest {
        AuthorityRequest {
            principal: ManagerPrincipal {
                app_id: AppId::parse("fixture.app").expect("app id"),
                instance_id: ManagerInstanceId::new(42),
                publisher_id: None,
            },
            capabilities,
            launch_profile: profile,
            root: LaunchRoot::new("/opt/apps/fixture").expect("root"),
            entrypoint: Entrypoint::new(entrypoint).expect("entrypoint"),
            argv: Vec::new(),
            environment: SanitizedEnvironment::new(BTreeMap::new()).expect("environment"),
            resources: DescriptiveResourceRequest {
                requested_memory_bytes: 0,
                requested_open_files: 0,
            },
            max_connections: 4,
        }
    }

    fn administrator() -> AdministratorPrincipal {
        AdministratorPrincipal::from_authenticated_session(1).expect("administrator")
    }

    #[test]
    fn an_authority_carries_exactly_the_granted_capabilities() {
        let authority = LaunchAuthority::new(
            &administrator(),
            request(
                vec![Capability::Sam, Capability::I2cp],
                LaunchProfile::UnsafeDirect,
                "app",
            ),
        )
        .expect("authority");

        assert!(authority.permits(Capability::Sam));
        assert!(authority.permits(Capability::I2cp));
        assert!(!authority.permits(Capability::ControlScoped));
        assert!(!authority.permits(Capability::Lifecycle));
        assert_eq!(
            authority.effective_grants(),
            vec![
                EffectiveGrant {
                    capability: Capability::Sam
                },
                EffectiveGrant {
                    capability: Capability::I2cp
                },
            ]
        );
    }

    #[test]
    fn a_reserved_capability_never_reaches_an_authority() {
        let outcome = LaunchAuthority::new(
            &administrator(),
            request(
                vec![Capability::Sam, Capability::BrokeredTcp],
                LaunchProfile::UnsafeDirect,
                "app",
            ),
        );
        assert_eq!(
            outcome.unwrap_err(),
            AppdError::Authority(ContractError::ReservedCapability),
            "brokered clearnet has no broker in Plan 369 and must not be grantable"
        );
    }

    #[test]
    fn secured_is_refused_before_an_authority_exists() {
        let outcome = LaunchAuthority::new(
            &administrator(),
            request(vec![Capability::Sam], LaunchProfile::Secured, "app"),
        );
        assert_eq!(
            outcome.unwrap_err(),
            AppdError::Bootstrap(ApphostBootstrapError::SecuredUnavailable),
            "the refusal must land on the authority, not at exec time"
        );
    }

    #[test]
    fn duplicate_and_oversize_grant_requests_are_refused() {
        assert_eq!(
            LaunchAuthority::new(
                &administrator(),
                request(
                    vec![Capability::Sam, Capability::Sam],
                    LaunchProfile::UnsafeDirect,
                    "app"
                ),
            )
            .unwrap_err(),
            AppdError::Authority(ContractError::InvalidControl),
            "a duplicate request is a caller bug, not two grants"
        );

        let every = vec![
            Capability::Sam,
            Capability::I2cp,
            Capability::ControlScoped,
            Capability::BrokeredTcp,
            Capability::UiBridge,
            Capability::Health,
            Capability::Lifecycle,
        ];
        let oversize = vec![Capability::Health; MAX_CAPABILITIES + 1];
        assert_eq!(every.len(), 7);
        let _ = every;
        assert_eq!(
            LaunchAuthority::new(
                &administrator(),
                request(oversize, LaunchProfile::UnsafeDirect, "app"),
            )
            .unwrap_err(),
            AppdError::Authority(ContractError::LimitExceeded("capabilities"))
        );
    }

    #[test]
    fn an_escaping_entrypoint_never_reaches_an_authority() {
        let outcome = LaunchAuthority::new(
            &administrator(),
            request(vec![Capability::Sam], LaunchProfile::UnsafeDirect, "app"),
        );
        assert!(outcome.is_ok());

        // `Entrypoint::new` already refuses `..`, absolute, and backslash forms,
        // so an escaping entrypoint cannot be constructed here at all. That is
        // the point of doing the check in the contract type rather than at exec
        // time; this assertion records it.
        assert!(Entrypoint::new("../escape").is_err());
        assert!(Entrypoint::new("/absolute").is_err());
        assert!(Entrypoint::new("nested/../../escape").is_err());
    }

    #[test]
    fn a_zero_gateway_connection_limit_is_refused() {
        let mut request = request(vec![Capability::Sam], LaunchProfile::UnsafeDirect, "app");
        request.max_connections = 0;
        assert_eq!(
            LaunchAuthority::new(&administrator(), request).unwrap_err(),
            AppdError::Protocol(ManagerProtocolError::LimitExceeded("gateway connections"))
        );
    }

    #[test]
    fn the_instance_id_is_resolved_to_canonical_form_once() {
        let authority = LaunchAuthority::new(
            &administrator(),
            request(vec![Capability::Health], LaunchProfile::UnsafeDirect, "app"),
        )
        .expect("authority");
        assert_eq!(authority.instance_id().as_str(), "42");
        assert_eq!(
            authority.principal().instance_id.to_app_instance_id(),
            Ok(authority.instance_id().clone())
        );
    }
}
