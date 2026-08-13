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
