//! Local RouterInfo construction.
//!
//! Plan 103 §4 owns the local signed RouterInfo. The builder borrows
//! the persistent [`RouterIdentityBundle`] long enough to sign the
//! record; it never clones private key material and never retains the
//! bundle across calls. The Plan 101 NTCP2 activation guard is
//! observed: a normal daemon must not advertise NTCP2 or any other
//! unqualified transport, so the local RouterInfo carries zero
//! `RouterAddress` entries.

use std::collections::BTreeMap;
use std::fmt;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_proto::{Date, Mapping, RouterAddress, RouterInfo};
use thiserror::Error;

use crate::router_info::{RouterHash, RouterInfoValidationError, ValidatedRouterInfo, router_hash};
use crate::{FloodfillAdvertisementPermit, LoopbackReachabilityProof, is_qualified_ssu2_address};

/// Errors raised by [`LocalRouterInfoBuilder`].
#[derive(Debug, Error, Eq, PartialEq)]
pub enum LocalRouterInfoError {
    /// The supplied `RouterAddress` carries a transport style that is
    /// forbidden under the current Plan 101 daemon-activation guard.
    /// The Plan 103 local RouterInfo must not advertise any transport.
    #[error("forbidden transport style {style} for local router info")]
    ForbiddenTransport {
        /// The transport style string the caller attempted to add.
        style: String,
    },
    /// The supplied options mapping is malformed at the protocol
    /// layer.
    #[error("invalid mapping for local router info: {context}")]
    InvalidMapping {
        /// Static field category from the codec.
        context: &'static str,
    },
    /// The signature could not be produced for the constructed record.
    #[error("local router info signing failed")]
    SigningFailed,
    /// Controlled floodfill construction requires a qualified SSU2 address.
    #[error("floodfill RouterInfo requires one qualified SSU2 address")]
    UnqualifiedFloodfillAddress,
    /// The constructed record did not pass the standard validator.
    #[error("local router info failed validation: {0}")]
    Validation(#[from] RouterInfoValidationError),
}

/// Local signed RouterInfo builder.
///
/// The builder is a transient owner: callers construct it, ask for
/// the latest signed snapshot via [`Self::build`], and let it drop.
/// No state lingers across calls; the persistent identity stays in
/// [`RouterIdentityBundle`].
pub struct LocalRouterInfoBuilder<'a> {
    bundle: &'a RouterIdentityBundle,
}

/// The I2NP feature/API version the controlled profile declares.
///
/// Per `specs/protocols/02-i2np.md`, `router.version` is an I2NP feature/API
/// version, not a release string, and it may only be as high as the message
/// surface `i2pr` actually implements. `i2pr` implements every I2NP type the
/// pinned reference enumerates, including peer testing (`TunnelTest`, wire code
/// 231), so the declaration names that level rather than a lower one that would
/// make peers skip peer testing for no reason.
///
/// Raising this constant above the implemented surface is a false claim and is
/// forbidden. It is pinned by `controlled_router_version_matches_the_implemented_i2np_surface`,
/// which fails if the declaration outruns the surface, and it must never be
/// edited to satisfy a peer's admission gate.
pub const CONTROLLED_ROUTER_VERSION: &str = "0.9.62";

/// The network identifier the controlled profile declares.
///
/// `i2pr` implements the mainnet (2) address and network behaviour, so the
/// controlled record declares netId 2. A controlled record that omits `netId`
/// is marked unreachable by the reference at parse time
/// (`libi2pd/RouterInfo.cpp:508`), and a `netId` that disagrees with the
/// peer's configured network is also treated as unreachable.
pub const CONTROLLED_NET_ID: &str = "2";

/// Builds the RouterInfo option declaration shared by every controlled
/// publication path.
///
/// The controlled SSU2 identity path and the controlled floodfill
/// publication path must not drift: both records are ingested by the same
/// reference, which rejects a record lacking `netId` or `router.version`.
pub fn controlled_router_options() -> Result<Mapping, LocalRouterInfoError> {
    Mapping::from_entries(vec![
        (
            "router.version".to_owned(),
            CONTROLLED_ROUTER_VERSION.to_owned(),
        ),
        ("netId".to_owned(), CONTROLLED_NET_ID.to_owned()),
    ])
    .map_err(|_| LocalRouterInfoError::InvalidMapping {
        context: "controlled options",
    })
}

impl<'a> fmt::Debug for LocalRouterInfoBuilder<'a> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalRouterInfoBuilder")
            .finish_non_exhaustive()
    }
}

impl<'a> LocalRouterInfoBuilder<'a> {
    /// Creates a builder bound to the supplied identity bundle.
    pub const fn new(bundle: &'a RouterIdentityBundle) -> Self {
        Self { bundle }
    }

    /// Constructs the local signed RouterInfo using the supplied
    /// publication time and options mapping.
    ///
    /// Per Plan 103 §4.2 and Plan 101 authority, the local RouterInfo
    /// carries zero `RouterAddress` entries. Attempting to inject an
    /// address is rejected; the builder is intentionally not generic
    /// over addresses so callers cannot accidentally advertise an
    /// unqualified transport.
    pub fn build(
        &self,
        published: Date,
        options: Mapping,
    ) -> Result<LocalRouterInfo, LocalRouterInfoError> {
        Self::validate_options(&options)?;
        let peers: Vec<i2pr_proto::Hash> = Vec::new();
        let addresses: Vec<RouterAddress> = Vec::new();
        let info = self
            .bundle
            .sign_router_info(published, addresses, peers, options)
            .map_err(|_| LocalRouterInfoError::SigningFailed)?;
        let validated = ValidatedRouterInfo::from_router_info(
            info,
            None,
            crate::router_info::ValidationContext::new(published),
        )?;
        Ok(LocalRouterInfo { validated })
    }

    /// Convenience constructor that uses an empty options mapping.
    pub fn build_default(&self, published: Date) -> Result<LocalRouterInfo, LocalRouterInfoError> {
        self.build(published, Mapping::empty())
    }

    /// Builds a floodfill RouterInfo only from an opaque active-role permit and a qualified SSU2
    /// address. Ordinary `build` remains unable to produce `caps=f`.
    pub fn build_floodfill(
        &self,
        published: Date,
        options: Mapping,
        address: RouterAddress,
        permit: &FloodfillAdvertisementPermit,
    ) -> Result<LocalRouterInfo, LocalRouterInfoError> {
        self.build_floodfill_with_caps(published, options, address, permit, "f")
    }

    /// Builds a floodfill RouterInfo carrying `caps=fR` for a
    /// peer-test-confirmed loopback address (Plan 306, ADR 0030).
    ///
    /// Admission is identical to [`Self::build_floodfill`] plus the
    /// opaque [`LoopbackReachabilityProof`]: caller-supplied options
    /// still cannot contain `R` (or any other forbidden letter) —
    /// `validate_options` rejects them first — so the `R` is appended
    /// only here, only beside proof of a confirmed controlled
    /// peer-test exchange for the advertised address. Bandwidth tiers
    /// and every other letter stay forbidden. Ordinary `build` and
    /// the withdrawal form are unaffected.
    pub fn build_floodfill_reachable(
        &self,
        published: Date,
        options: Mapping,
        address: RouterAddress,
        permit: &FloodfillAdvertisementPermit,
        _proof: &LoopbackReachabilityProof,
    ) -> Result<LocalRouterInfo, LocalRouterInfoError> {
        self.build_floodfill_with_caps(published, options, address, permit, "fR")
    }

    fn build_floodfill_with_caps(
        &self,
        published: Date,
        options: Mapping,
        address: RouterAddress,
        _permit: &FloodfillAdvertisementPermit,
        caps_suffix: &str,
    ) -> Result<LocalRouterInfo, LocalRouterInfoError> {
        if !is_qualified_ssu2_address(&address) {
            return Err(LocalRouterInfoError::UnqualifiedFloodfillAddress);
        }
        Self::validate_options(&options)?;
        let mut entries: Vec<_> = options
            .entries()
            .iter()
            .map(|entry| (entry.key().to_owned(), entry.value().to_owned()))
            .filter(|(key, _)| key != "caps")
            .collect();
        let caps = options.get("caps").unwrap_or("");
        let mut floodfill_caps = caps.to_owned();
        for letter in caps_suffix.bytes() {
            floodfill_caps.push(letter as char);
        }
        entries.push(("caps".to_owned(), floodfill_caps));
        let options = Mapping::from_entries(entries)
            .map_err(|_| LocalRouterInfoError::InvalidMapping { context: "caps" })?;
        self.sign_addressed(published, options, address)
    }

    /// Builds the withdrawal RouterInfo for the same qualified SSU2 address without `caps=f`.
    ///
    /// Health-withdrawal counterpart to [`Self::build_floodfill`]: the address stays
    /// qualified and bound so peers keep a reachable route, but the floodfill flag is gone.
    /// Like `build_floodfill` this requires the opaque activation permit (threaded through
    /// the activation record), so withdrawing is only possible after activating; ordinary
    /// `build` still cannot carry any address at all.
    pub fn build_floodfill_withdrawal(
        &self,
        published: Date,
        options: Mapping,
        address: RouterAddress,
        _permit: &FloodfillAdvertisementPermit,
    ) -> Result<LocalRouterInfo, LocalRouterInfoError> {
        if !is_qualified_ssu2_address(&address) {
            return Err(LocalRouterInfoError::UnqualifiedFloodfillAddress);
        }
        // `validate_options` refuses any `caps` value containing `f`,
        // so the withdrawal record cannot re-advertise floodfill.
        Self::validate_options(&options)?;
        let options = Mapping::from_entries(
            options
                .entries()
                .iter()
                .map(|entry| (entry.key().to_owned(), entry.value().to_owned()))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| LocalRouterInfoError::InvalidMapping { context: "caps" })?;
        self.sign_addressed(published, options, address)
    }

    fn sign_addressed(
        &self,
        published: Date,
        options: Mapping,
        address: RouterAddress,
    ) -> Result<LocalRouterInfo, LocalRouterInfoError> {
        let info = self
            .bundle
            .sign_router_info(published, vec![address], Vec::new(), options)
            .map_err(|_| LocalRouterInfoError::SigningFailed)?;
        let validated = ValidatedRouterInfo::from_router_info(
            info,
            None,
            crate::router_info::ValidationContext::new(published),
        )?;
        Ok(LocalRouterInfo { validated })
    }

    /// Returns the local RouterHash for this bundle without
    /// constructing a full RouterInfo.
    pub fn local_router_hash(&self) -> Result<RouterHash, LocalRouterInfoError> {
        Ok(router_hash(self.bundle.identity())?)
    }

    fn validate_options(options: &Mapping) -> Result<(), LocalRouterInfoError> {
        // `Mapping` is already validated by the codec; the local
        // builder just needs to refuse forbidden capability flags.
        if let Some(caps) = options.get("caps") {
            // The Plan 101 authority forbids advertising floodfill,
            // bandwidth tiering, or unreviewed transport capability
            // letters. Refuse the obvious false-advertising flags.
            for forbidden in ['f', 'B', 'K', 'L', 'M', 'N', 'P', 'R', 'S', 'U', 'X'] {
                if caps.bytes().any(|byte| byte == forbidden as u8) {
                    return Err(LocalRouterInfoError::InvalidMapping { context: "caps" });
                }
            }
        }
        Ok(())
    }
}

/// A locally signed, validated RouterInfo snapshot.
///
/// The type owns a [`ValidatedRouterInfo`]; the previous snapshot is
/// retained until a fresh snapshot replaces it through composition
/// code so a signing failure does not silently clear the last valid
/// record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalRouterInfo {
    validated: ValidatedRouterInfo,
}

impl LocalRouterInfo {
    /// Returns the canonical local RouterHash.
    pub fn router_hash(&self) -> RouterHash {
        self.validated.key()
    }

    /// Borrows the validated `RouterInfo`.
    pub fn router_info(&self) -> &RouterInfo {
        self.validated.router_info()
    }

    /// Returns the validated wrapper directly.
    pub fn validated(&self) -> &ValidatedRouterInfo {
        &self.validated
    }

    /// Returns the canonical encoded RouterInfo bytes.
    pub fn encoded(&self, maximum: usize) -> Result<Vec<u8>, i2pr_proto::CodecError> {
        self.validated.encoded(maximum)
    }
}

/// Helper that produces a stable, ordered view of the local
/// options mapping suitable for serialization into the daemon
/// `RouterInfo` publication state.
///
/// The helper returns `None` for an empty mapping so callers can
/// branch without a separate check.
#[allow(dead_code)]
pub fn options_to_sorted_entries(options: &Mapping) -> Option<Vec<(&str, &str)>> {
    if options.entries().is_empty() {
        return None;
    }
    let mut pairs: Vec<(&str, &str)> = options
        .entries()
        .iter()
        .map(|entry| (entry.key(), entry.value()))
        .collect();
    pairs.sort_by(|left, right| left.0.cmp(right.0));
    Some(pairs)
}

/// Lightweight privacy-safe summary of the local RouterInfo.
///
/// The summary exposes only the local RouterHash, the number of
/// `RouterAddress` entries, and the publication timestamp. It is the
/// preferred value for `i2pr-daemon` health/snapshot output.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalRouterInfoSummary {
    /// Canonical local RouterHash.
    pub router_hash: RouterHash,
    /// Number of `RouterAddress` entries (always zero under Plan 103).
    pub address_count: usize,
    /// Publication timestamp carried by the local RouterInfo.
    pub published: Date,
}

#[allow(dead_code)]
impl LocalRouterInfoSummary {
    /// Builds a summary from a local snapshot.
    pub fn from_local(local: &LocalRouterInfo) -> Self {
        Self {
            router_hash: local.router_hash(),
            address_count: local.router_info().addresses().len(),
            published: local.router_info().published(),
        }
    }
}

/// Diagnostic helper: confirm a `Mapping` does not advertise
/// forbidden capability flags. The builder already enforces this
/// rule, but daemon composition code that produces options mappings
/// in a different path can call the helper to assert the same
/// invariant.
#[allow(dead_code)]
pub fn assert_no_forbidden_caps(options: &Mapping) -> Result<(), LocalRouterInfoError> {
    LocalRouterInfoBuilder::validate_options(options)
}

/// Sorted view of a mapping; provided for symmetry with
/// [`options_to_sorted_entries`]. The wrapper returns a `BTreeMap`
/// so callers can re-build mappings deterministically.
#[allow(dead_code)]
pub fn options_to_btree(options: &Mapping) -> BTreeMap<String, String> {
    options
        .entries()
        .iter()
        .map(|entry| (entry.key().to_owned(), entry.value().to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn active_role_permit() -> crate::FloodfillAdvertisementPermit {
        let mut role = crate::FloodfillRoleController::new();
        role.update(crate::FloodfillEligibilitySnapshot {
            controlled_qualification_permit: true,
            qualified_ssu2_address: true,
            direct_reachability: true,
            netdb_ready: true,
            storage_ready: true,
            maintenance_ready: true,
            resource_headroom: true,
            clock_sane: true,
            supervision_healthy: true,
        });
        assert!(role.begin_activation());
        role.complete_activation().expect("active role")
    }

    #[test]
    fn builder_emits_a_validated_router_info_with_zero_addresses() {
        let signer = bundle(0x400);
        let builder = LocalRouterInfoBuilder::new(&signer);
        let published = Date::from_millis(1);
        let local = builder.build_default(published).expect("build local");
        assert_eq!(local.router_info().addresses().len(), 0);
        assert_eq!(local.router_hash(), builder.local_router_hash().unwrap());
    }

    #[test]
    fn builder_rejects_floodfill_capability_advertisement() {
        let signer = bundle(0x401);
        let builder = LocalRouterInfoBuilder::new(&signer);
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), "f".to_owned()).unwrap();
        let error = builder
            .build(Date::from_millis(1), options.build().unwrap())
            .unwrap_err();
        assert!(matches!(error, LocalRouterInfoError::InvalidMapping { .. }));
    }

    /// The controlled declaration is a single source of truth: every
    /// controlled publication path must emit `netId` and `router.version`.
    /// The pinned reference marks a record lacking either one unreachable at
    /// parse time (`libi2pd/RouterInfo.cpp:508`), which is the boundary
    /// Plan 278 stopped at.
    #[test]
    fn controlled_router_options_declare_net_id_and_router_version() {
        let options = controlled_router_options().expect("controlled options");
        assert_eq!(options.get("netId"), Some(CONTROLLED_NET_ID));
        assert_eq!(
            options.get("router.version"),
            Some(CONTROLLED_ROUTER_VERSION)
        );
    }

    /// The declared level must track the implemented I2NP surface in both
    /// directions. If the surface loses a type the declaration claims, the
    /// claim is false. If the surface implements peer testing but the
    /// declaration stays below the level that introduced it, peers will skip
    /// peer testing for a router that answers it, which is the defect the
    /// M12 qualification exists to correct.
    #[test]
    fn controlled_router_version_matches_the_implemented_i2np_surface() {
        fn level(value: &str) -> u32 {
            let digits: String = value.chars().filter(char::is_ascii_digit).collect();
            digits.parse().expect("decimal version")
        }
        // The reference pairs the 0.9.62 level with peer testing, and
        // `RouterInfo::IsEligibleFloodfill` requires at least 0.9.62.
        const PEER_TESTING_LEVEL: u32 = 962;
        const PEER_TESTING_WIRE_CODE: u8 = 231;

        // The declared level must not outrun the peer-testing surface.
        assert!(
            i2pr_proto::MessageType::from_code(PEER_TESTING_WIRE_CODE)
                == i2pr_proto::MessageType::TunnelTest,
            "the declared level requires peer testing, so TunnelTest must exist"
        );
        // The declaration must reach the peer-testing level while the surface
        // has it, so a peer does not skip peer testing for this router.
        assert_eq!(level(CONTROLLED_ROUTER_VERSION), PEER_TESTING_LEVEL);
    }

    #[test]
    fn controlled_floodfill_builder_requires_role_permit_and_ssu2_address() {
        let signer = bundle(0x405);
        let builder = LocalRouterInfoBuilder::new(&signer);
        let options =
            Mapping::from_entries(vec![("router.version".into(), "0.9.69".into())]).unwrap();
        let address_options = Mapping::from_entries(vec![
            ("caps".into(), "4".into()),
            ("host".into(), "127.0.0.1".into()),
            ("i".into(), crate::base64::encode(&[2; 32]).unwrap()),
            ("mtu".into(), "1280".into()),
            ("port".into(), "1234".into()),
            ("s".into(), crate::base64::encode(&[1; 32]).unwrap()),
            ("v".into(), "2".into()),
        ])
        .unwrap();
        let address =
            RouterAddress::new(10, Date::from_millis(100), "SSU2".into(), address_options).unwrap();
        let mut role = crate::FloodfillRoleController::new();
        let eligible = crate::FloodfillEligibilitySnapshot {
            controlled_qualification_permit: true,
            qualified_ssu2_address: true,
            direct_reachability: true,
            netdb_ready: true,
            storage_ready: true,
            maintenance_ready: true,
            resource_headroom: true,
            clock_sane: true,
            supervision_healthy: true,
        };
        assert_eq!(
            role.update(eligible),
            crate::FloodfillRoleEffect::ReadyToActivate
        );
        assert!(role.begin_activation());
        let permit = role.complete_activation().unwrap();
        let local = builder
            .build_floodfill(Date::from_millis(101), options, address, &permit)
            .unwrap();
        assert_eq!(local.router_info().options().get("caps"), Some("f"));
        assert_eq!(local.router_info().addresses().len(), 1);
    }

    fn qualified_ssu2_test_address() -> RouterAddress {
        let address_options = Mapping::from_entries(vec![
            ("caps".into(), "4".into()),
            ("host".into(), "127.0.0.1".into()),
            ("i".into(), crate::base64::encode(&[2; 32]).unwrap()),
            ("mtu".into(), "1280".into()),
            ("port".into(), "1234".into()),
            ("s".into(), crate::base64::encode(&[1; 32]).unwrap()),
            ("v".into(), "2".into()),
        ])
        .unwrap();
        RouterAddress::new(10, Date::from_millis(100), "SSU2".into(), address_options).unwrap()
    }

    /// Plan 306/ADR 0030: the proof-gated builder emits exactly `fR` —
    /// the appended `R` is the only widening over the plain path.
    #[test]
    fn reachable_builder_emits_exactly_f_r_with_proof() {
        let signer = bundle(0x407);
        let builder = LocalRouterInfoBuilder::new(&signer);
        let options = controlled_router_options().expect("controlled options");
        let address = qualified_ssu2_test_address();
        let permit = active_role_permit();
        let proof = LoopbackReachabilityProof::attest_confirmed_peer_test();
        let local = builder
            .build_floodfill_reachable(Date::from_millis(101), options, address, &permit, &proof)
            .expect("reachable build");
        assert_eq!(local.router_info().options().get("caps"), Some("fR"));
        assert_eq!(local.router_info().addresses().len(), 1);
    }

    /// Plan 306/ADR 0030: the proof does not launder caller bytes. Any
    /// forbidden letter in the input options — including `R` itself —
    /// is rejected on both floodfill builders before anything is
    /// appended, so tiers and all other letters stay forbidden.
    #[test]
    fn reachable_builder_rejects_smuggled_letters() {
        for smuggled in ["R", "f", "L", "fR"] {
            let signer = bundle(0x408);
            let builder = LocalRouterInfoBuilder::new(&signer);
            let options =
                Mapping::from_entries(vec![("caps".to_owned(), smuggled.to_owned())]).unwrap();
            let permit = active_role_permit();
            let proof = LoopbackReachabilityProof::attest_confirmed_peer_test();
            assert!(
                matches!(
                    builder.build_floodfill(
                        Date::from_millis(101),
                        options.clone(),
                        qualified_ssu2_test_address(),
                        &permit,
                    ),
                    Err(LocalRouterInfoError::InvalidMapping { .. })
                ),
                "plain builder must reject caps={smuggled}"
            );
            assert!(
                matches!(
                    builder.build_floodfill_reachable(
                        Date::from_millis(101),
                        options,
                        qualified_ssu2_test_address(),
                        &permit,
                        &proof,
                    ),
                    Err(LocalRouterInfoError::InvalidMapping { .. })
                ),
                "reachable builder must reject caps={smuggled}"
            );
        }
    }

    /// Plan 306/ADR 0030: without proof there is no `R`. The
    /// reachable builder requires the proof argument by type, so this
    /// row pins the plain builder's output at exactly `f`.
    #[test]
    fn plain_builder_still_emits_f_only() {
        let signer = bundle(0x409);
        let builder = LocalRouterInfoBuilder::new(&signer);
        let options = controlled_router_options().expect("controlled options");
        let permit = active_role_permit();
        let local = builder
            .build_floodfill(
                Date::from_millis(101),
                options,
                qualified_ssu2_test_address(),
                &permit,
            )
            .expect("plain build");
        assert_eq!(local.router_info().options().get("caps"), Some("f"));
    }

    #[test]
    fn controlled_floodfill_builder_rejects_style_only_ssu2_addresses() {
        let signer = bundle(0x406);
        let builder = LocalRouterInfoBuilder::new(&signer);
        let options = Mapping::empty();
        let address =
            RouterAddress::new(10, Date::from_millis(100), "SSU2".into(), Mapping::empty())
                .unwrap();
        let role = active_role_permit();
        assert!(matches!(
            builder.build_floodfill(Date::from_millis(101), options, address, &role),
            Err(LocalRouterInfoError::UnqualifiedFloodfillAddress)
        ));
    }

    #[test]
    fn builder_rejects_unreviewed_capability_letters() {
        for forbidden in ['B', 'K', 'L', 'M', 'N', 'P', 'R', 'S', 'U', 'X'] {
            let signer = bundle(0x402);
            let builder = LocalRouterInfoBuilder::new(&signer);
            let mut options = Mapping::builder();
            let caps = format!("L{forbidden}");
            options.insert("caps".to_owned(), caps.clone()).unwrap();
            let error = builder
                .build(Date::from_millis(1), options.build().unwrap())
                .unwrap_err();
            assert!(
                matches!(error, LocalRouterInfoError::InvalidMapping { .. }),
                "expected rejection for caps={caps}, got {error:?}"
            );
        }
    }

    #[test]
    fn local_router_info_self_validates_through_normal_path() {
        let signer = bundle(0x403);
        let builder = LocalRouterInfoBuilder::new(&signer);
        let local = builder
            .build_default(Date::from_millis(1))
            .expect("build local");
        // Round-trip through the validator with the router's own hash as
        // the expected key. This proves the Plan 103 §4.4 contract: there
        // is no privileged local bypass.
        let info = local.router_info().clone();
        let validated = ValidatedRouterInfo::from_router_info(
            info,
            Some(local.router_hash()),
            crate::router_info::ValidationContext::new(Date::from_millis(1)),
        )
        .expect("self-validate");
        assert_eq!(validated.key(), local.router_hash());
    }

    #[test]
    fn local_router_info_summary_reports_zero_addresses() {
        let signer = bundle(0x404);
        let local = LocalRouterInfoBuilder::new(&signer)
            .build_default(Date::from_millis(2))
            .expect("build local");
        let summary = LocalRouterInfoSummary::from_local(&local);
        assert_eq!(summary.address_count, 0);
        assert_eq!(summary.published, Date::from_millis(2));
        assert_eq!(summary.router_hash, local.router_hash());
    }

    #[test]
    fn builder_succeeds_with_allowed_router_version_option() {
        let signer = bundle(0x405);
        let builder = LocalRouterInfoBuilder::new(&signer);
        let mut options = Mapping::builder();
        options
            .insert("router.version".to_owned(), "0.9.68".to_owned())
            .unwrap();
        let local = builder
            .build(Date::from_millis(1), options.build().unwrap())
            .expect("build with router.version");
        assert_eq!(
            local
                .router_info()
                .protocol_version()
                .unwrap()
                .unwrap()
                .as_str(),
            "0.9.68"
        );
    }
}
