//! Deterministic SSU2 RouterInfo address-publication snapshots (Plan 159).
//!
//! This module builds validated SSU2 address-publication material from
//! a reachability snapshot — never from mutable packet/session state.
//! Inputs:
//!
//! ```text
//! static SSU2 public key
//! intro key
//! version = 2
//! validated local/configured endpoint
//! reachability state
//! MTU/caps
//! introducer list (empty until Plan 160 owns validated introducers)
//! ```
//!
//! Rules enforced here:
//!
//! - private static/intro key material is never an input: only the
//!   public 32-byte values are accepted;
//! - direct host/port appears only when the reachability snapshot says
//!   `Reachable` and the policy explicitly opts into direct publication;
//! - the firewalled form never fabricates a direct address (it emits
//!   the unpublished static-only form while introducers are absent);
//! - introducers are included only when the policy explicitly allows
//!   validated introducers (Plan 160 owns that evidence; the default
//!   denies non-empty lists rather than silently dropping them);
//! - output option entries are canonical (sorted by key) and
//!   deterministic for identical inputs;
//! - the snapshot carries the evidence expiry: publication must
//!   withdraw once the underlying reachability evidence expires;
//! - building a snapshot never publishes anything: actual network
//!   publication stays disabled in production (the daemon `[ssu2]`
//!   surface rejects advertisement; Plan 160+ owns activation).
//!
//! Normative traceability: `plans/159-m8-ssu2-path-validation-`
//! `publication-and-transport-selection.md` §7. No sockets, no Tokio,
//! no NetDB mutation, no async.

use std::fmt;

use i2pr_transport::{ReachabilitySnapshot, ReachabilityState};
use thiserror::Error;

use crate::address::{
    IntroKey as AddressIntroKey, Ssu2AddressClass, Ssu2Capabilities, Ssu2Endpoint, Ssu2Introducer,
    Ssu2RouterAddress, StaticPublicKey, encode_i2p_base64,
};
use crate::constants;

/// Publication policy for one snapshot build.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PublicationPolicy {
    allow_direct: bool,
    allow_introducers: bool,
}

impl PublicationPolicy {
    /// Builds an explicit publication policy.
    ///
    /// `allow_direct` opts into direct host/port publication (default
    /// denial keeps this plan free of advertisement); `allow_introducers`
    /// opts into publishing validated introducer groups (Plan 160 owns
    /// that evidence; this plan denies non-empty lists by default).
    pub const fn new(allow_direct: bool, allow_introducers: bool) -> Self {
        Self {
            allow_direct,
            allow_introducers,
        }
    }

    /// Returns whether direct host/port publication is permitted.
    pub const fn allow_direct(self) -> bool {
        self.allow_direct
    }

    /// Returns whether validated introducers may be published.
    pub const fn allow_introducers(self) -> bool {
        self.allow_introducers
    }
}

/// Typed publication-build failures.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PublicationError {
    /// A public key input was all zero.
    #[error("SSU2 publication key material is invalid")]
    InvalidKey,
    /// The MTU is outside the SSU2 `1280..=9000` range.
    #[error("SSU2 publication MTU is out of range")]
    InvalidMtu,
    /// Non-empty introducers were supplied without validated
    /// introducer evidence (Plan 160 owns that surface).
    #[error("SSU2 publication introducers are not validated")]
    IntroducersUnvalidated,
    /// The supplied introducers exceed the protocol bound.
    #[error("SSU2 publication carries too many introducers")]
    TooManyIntroducers,
}

/// Why a snapshot build withheld publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WithholdReason {
    /// The reachability evidence expired; withdraw the address.
    EvidenceExpired,
    /// Direct publication is not permitted by policy/state.
    DirectNotAllowed,
}

impl fmt::Display for WithholdReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EvidenceExpired => formatter.write_str("reachability evidence expired"),
            Self::DirectNotAllowed => formatter.write_str("direct publication not permitted"),
        }
    }
}

/// The deterministic publication decision for one snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicationOutcome {
    /// Publish a direct SSU2 address (host/port present).
    Direct(Ssu2PublicationSnapshot),
    /// Publish the firewalled form (no fabricated direct address).
    Firewalled(Ssu2PublicationSnapshot),
    /// Publish nothing for the supplied inputs.
    Withheld(WithholdReason),
}

/// One deterministic validated SSU2 address-publication snapshot.
///
/// Option entries are canonical (sorted by key) so identical inputs
/// always render identical bytes. The snapshot carries the evidence
/// expiry: callers must treat an expired snapshot as withdrawn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ssu2PublicationSnapshot {
    class: Ssu2AddressClass,
    options: Vec<(String, String)>,
    evidence_expires_secs: u64,
}

impl Ssu2PublicationSnapshot {
    /// Returns the structural address class of the snapshot.
    pub const fn address_class(&self) -> Ssu2AddressClass {
        self.class
    }

    /// Returns the canonical sorted option entries.
    pub fn option_entries(&self) -> &[(String, String)] {
        &self.options
    }

    /// Returns the monotonic evidence-expiry second.
    pub const fn evidence_expires_secs(&self) -> u64 {
        self.evidence_expires_secs
    }

    /// Returns whether the underlying evidence expired at `now_secs`.
    pub const fn is_expired(&self, now_secs: u64) -> bool {
        now_secs >= self.evidence_expires_secs
    }

    /// Returns whether a direct endpoint is present.
    pub const fn has_direct_endpoint(&self) -> bool {
        matches!(
            self.class,
            Ssu2AddressClass::Direct | Ssu2AddressClass::DirectWithIntroducers
        )
    }
}

/// One publication-snapshot request: policy plus validated inputs.
///
/// Bundling keeps the build entry point to a single argument as Plan
/// 160 extends the surface (introducers, caps negotiation).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationRequest<'a> {
    /// Direct/introducer publication opt-ins.
    pub policy: PublicationPolicy,
    /// Public SSU2 static key bytes (never secret material).
    pub static_public: [u8; 32],
    /// Public SSU2 intro key bytes (never secret material).
    pub intro_public: [u8; 32],
    /// Validated local/configured endpoint, when one may be claimed.
    pub endpoint: Option<Ssu2Endpoint>,
    /// Conservative reachability snapshot consumed by value.
    pub reachability: ReachabilitySnapshot,
    /// Router MTU to publish (`1280..=9000`).
    pub mtu: u16,
    /// Capability flags to publish.
    pub caps: &'a Ssu2Capabilities,
    /// Validated introducer groups (empty until Plan 160).
    pub introducers: &'a [Ssu2Introducer],
    /// Wall-clock second of the build.
    pub now_secs: u64,
    /// Wall-clock second the reachability evidence expires.
    pub evidence_expires_secs: u64,
}

/// Builds the deterministic publication decision for one request.
///
/// `introducers` must be empty until Plan 160 produces validated live
/// introducer records, unless `policy.allow_introducers` explicitly
/// opts in with validated material. `now_secs`/`evidence_expires_secs`
/// use the wall-clock seconds domain of the reachability snapshot.
pub fn build_publication_snapshot(
    request: PublicationRequest<'_>,
) -> Result<PublicationOutcome, PublicationError> {
    let PublicationRequest {
        policy,
        static_public,
        intro_public,
        endpoint,
        reachability,
        mtu,
        caps,
        introducers,
        now_secs,
        evidence_expires_secs,
    } = request;
    let static_key =
        StaticPublicKey::new(static_public).map_err(|_| PublicationError::InvalidKey)?;
    let _intro = AddressIntroKey::new(intro_public).map_err(|_| PublicationError::InvalidKey)?;
    if !(constants::SSU2_MIN_MTU..=constants::SSU2_MAX_MTU).contains(&mtu) {
        return Err(PublicationError::InvalidMtu);
    }
    if introducers.len() > constants::MAX_SSU2_INTRODUCERS {
        return Err(PublicationError::TooManyIntroducers);
    }
    if !introducers.is_empty() && !policy.allow_introducers() {
        return Err(PublicationError::IntroducersUnvalidated);
    }
    if now_secs >= evidence_expires_secs {
        return Ok(PublicationOutcome::Withheld(
            WithholdReason::EvidenceExpired,
        ));
    }
    let direct_allowed = policy.allow_direct()
        && reachability.state == ReachabilityState::Reachable
        && endpoint.is_some();
    if direct_allowed {
        let endpoint = endpoint.expect("checked");
        return Ok(PublicationOutcome::Direct(direct_snapshot(
            static_key,
            intro_public,
            endpoint,
            mtu,
            caps,
            introducers,
            evidence_expires_secs,
        )));
    }
    if reachability.state == ReachabilityState::Reachable || endpoint.is_some() {
        // Reachable-but-not-opted-in, or an endpoint without confirmed
        // reachability: the firewalled form, never a fabricated direct.
        return Ok(PublicationOutcome::Firewalled(firewalled_snapshot(
            static_key,
            evidence_expires_secs,
        )));
    }
    Ok(PublicationOutcome::Firewalled(firewalled_snapshot(
        static_key,
        evidence_expires_secs,
    )))
}

fn direct_snapshot(
    static_key: StaticPublicKey,
    intro_public: [u8; 32],
    endpoint: Ssu2Endpoint,
    mtu: u16,
    caps: &Ssu2Capabilities,
    introducers: &[Ssu2Introducer],
    evidence_expires_secs: u64,
) -> Ssu2PublicationSnapshot {
    let mut options = Vec::new();
    if !caps.as_str().is_empty() {
        options.push(("caps".to_owned(), caps.as_str().to_owned()));
    }
    options.push(("host".to_owned(), endpoint.ip().to_string()));
    options.push(("i".to_owned(), encode_i2p_base64(&intro_public)));
    for (index, introducer) in introducers.iter().enumerate() {
        options.push((format!("ihost{index}"), introducer.ip().to_string()));
        options.push((
            format!("ikey{index}"),
            encode_i2p_base64(introducer.intro_key().as_bytes()),
        ));
        options.push((format!("iport{index}"), introducer.port().to_string()));
        options.push((format!("itag{index}"), introducer.relay_tag().to_string()));
    }
    options.push(("mtu".to_owned(), mtu.to_string()));
    options.push(("port".to_owned(), endpoint.port().to_string()));
    options.push(("s".to_owned(), encode_i2p_base64(static_key.as_bytes())));
    options.push(("v".to_owned(), constants::SSU2_VERSION.to_string()));
    options.sort_by(|left, right| left.0.cmp(&right.0));
    let class = if introducers.is_empty() {
        Ssu2AddressClass::Direct
    } else {
        Ssu2AddressClass::DirectWithIntroducers
    };
    Ssu2PublicationSnapshot {
        class,
        options,
        evidence_expires_secs,
    }
}

fn firewalled_snapshot(
    static_key: StaticPublicKey,
    evidence_expires_secs: u64,
) -> Ssu2PublicationSnapshot {
    // Unpublished static-only form: no host/port fabrication, no intro
    // key without an endpoint (the parser forbids it), version pinned.
    let options = vec![
        ("s".to_owned(), encode_i2p_base64(static_key.as_bytes())),
        ("v".to_owned(), constants::SSU2_VERSION.to_string()),
    ];
    Ssu2PublicationSnapshot {
        class: Ssu2AddressClass::UnpublishedStatic,
        options,
        evidence_expires_secs,
    }
}

/// Parses a snapshot back through the strict production parser.
///
/// Test and composition helper proving the snapshot is well-formed
/// without duplicating parser rules here.
pub fn parse_snapshot(
    snapshot: &Ssu2PublicationSnapshot,
) -> Result<Ssu2RouterAddress, crate::address::Ssu2AddressError> {
    let borrowed: Vec<(&str, &str)> = snapshot
        .options
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    Ssu2RouterAddress::from_option_entries("SSU2", &borrowed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_transport::{AddressFamily, ReachabilitySnapshot};
    // `core::net` path per the static boundary script: pure endpoint
    // literals for snapshots, never sockets.
    use core::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    const STATIC: [u8; 32] = [0x42; 32];
    const INTRO: [u8; 32] = [0x24; 32];

    fn endpoint() -> Ssu2Endpoint {
        Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 12345).expect("endpoint")
    }

    fn reachable_snapshot() -> ReachabilitySnapshot {
        ReachabilitySnapshot {
            state: ReachabilityState::Reachable,
            corroboration: 3,
            expires_at: Duration::from_secs(3600),
            family: AddressFamily::Ipv4,
        }
    }

    fn unconfirmed_snapshot() -> ReachabilitySnapshot {
        ReachabilitySnapshot {
            state: ReachabilityState::ObservedUnconfirmed,
            corroboration: 1,
            expires_at: Duration::from_secs(3600),
            family: AddressFamily::Ipv4,
        }
    }

    fn build(
        policy: PublicationPolicy,
        state: ReachabilitySnapshot,
        endpoint: Option<Ssu2Endpoint>,
        now: u64,
        expires: u64,
    ) -> Result<PublicationOutcome, PublicationError> {
        build_publication_snapshot(PublicationRequest {
            policy,
            static_public: STATIC,
            intro_public: INTRO,
            endpoint,
            reachability: state,
            mtu: 1280,
            caps: &Ssu2Capabilities::empty(),
            introducers: &[],
            now_secs: now,
            evidence_expires_secs: expires,
        })
    }

    #[test]
    fn direct_snapshot_is_deterministic_and_round_trips() {
        let policy = PublicationPolicy::new(true, false);
        let first =
            build(policy, reachable_snapshot(), Some(endpoint()), 100, 3600).expect("build");
        let second =
            build(policy, reachable_snapshot(), Some(endpoint()), 100, 3600).expect("build");
        assert_eq!(first, second);
        let PublicationOutcome::Direct(snapshot) = first else {
            panic!("expected direct");
        };
        assert_eq!(snapshot.address_class(), Ssu2AddressClass::Direct);
        assert!(snapshot.has_direct_endpoint());
        // Canonical order: keys sorted.
        let keys: Vec<&str> = snapshot
            .option_entries()
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
        // Strict production parse accepts the snapshot unchanged.
        let parsed = parse_snapshot(&snapshot).expect("parse");
        assert_eq!(parsed.address_class(), Ssu2AddressClass::Direct);
        assert_eq!(parsed.endpoint(), Some(endpoint()));
        assert_eq!(parsed.mtu(), Some(1280));
        assert!(!snapshot.is_expired(3599));
        assert!(snapshot.is_expired(3600));
    }

    #[test]
    fn unconfirmed_state_yields_firewalled_form_without_direct_address() {
        let outcome = build(
            PublicationPolicy::new(true, false),
            unconfirmed_snapshot(),
            Some(endpoint()),
            100,
            3600,
        )
        .expect("build");
        let PublicationOutcome::Firewalled(snapshot) = outcome else {
            panic!("expected firewalled");
        };
        assert!(!snapshot.has_direct_endpoint());
        assert_eq!(
            snapshot.address_class(),
            Ssu2AddressClass::UnpublishedStatic
        );
        let parsed = parse_snapshot(&snapshot).expect("parse");
        assert_eq!(parsed.endpoint(), None);
        assert_eq!(parsed.address_class(), Ssu2AddressClass::UnpublishedStatic);
    }

    #[test]
    fn direct_opt_out_never_fabricates_direct_address() {
        let outcome = build(
            PublicationPolicy::default(),
            reachable_snapshot(),
            Some(endpoint()),
            100,
            3600,
        )
        .expect("build");
        assert!(matches!(outcome, PublicationOutcome::Firewalled(_)));
    }

    #[test]
    fn expired_evidence_withdraws() {
        let outcome = build(
            PublicationPolicy::new(true, false),
            reachable_snapshot(),
            Some(endpoint()),
            3600,
            3600,
        )
        .expect("build");
        assert_eq!(
            outcome,
            PublicationOutcome::Withheld(WithholdReason::EvidenceExpired)
        );
        // And a live snapshot expires exactly at its evidence second.
        let PublicationOutcome::Direct(snapshot) = build(
            PublicationPolicy::new(true, false),
            reachable_snapshot(),
            Some(endpoint()),
            100,
            3600,
        )
        .expect("build") else {
            panic!("expected direct");
        };
        assert!(snapshot.is_expired(3600));
    }

    #[test]
    fn unvalidated_introducers_are_rejected_not_dropped() {
        let introducer =
            Ssu2Introducer::new(endpoint(), AddressIntroKey::new(INTRO).expect("intro"), 7)
                .expect("introducer");
        let outcome = build_publication_snapshot(PublicationRequest {
            policy: PublicationPolicy::new(true, false),
            static_public: STATIC,
            intro_public: INTRO,
            endpoint: Some(endpoint()),
            reachability: reachable_snapshot(),
            mtu: 1280,
            caps: &Ssu2Capabilities::empty(),
            introducers: &[introducer],
            now_secs: 100,
            evidence_expires_secs: 3600,
        });
        assert_eq!(outcome, Err(PublicationError::IntroducersUnvalidated));
    }

    #[test]
    fn zero_keys_and_bad_mtu_fail_closed() {
        let bad_static = build_publication_snapshot(PublicationRequest {
            policy: PublicationPolicy::new(true, false),
            static_public: [0_u8; 32],
            intro_public: INTRO,
            endpoint: Some(endpoint()),
            reachability: reachable_snapshot(),
            mtu: 1280,
            caps: &Ssu2Capabilities::empty(),
            introducers: &[],
            now_secs: 100,
            evidence_expires_secs: 3600,
        });
        assert_eq!(bad_static, Err(PublicationError::InvalidKey));
        let bad_mtu = build_publication_snapshot(PublicationRequest {
            policy: PublicationPolicy::new(true, false),
            static_public: STATIC,
            intro_public: INTRO,
            endpoint: Some(endpoint()),
            reachability: reachable_snapshot(),
            mtu: 1279,
            caps: &Ssu2Capabilities::empty(),
            introducers: &[],
            now_secs: 100,
            evidence_expires_secs: 3600,
        });
        assert_eq!(bad_mtu, Err(PublicationError::InvalidMtu));
    }

    #[test]
    fn snapshots_expose_no_private_material() {
        let PublicationOutcome::Direct(snapshot) = build(
            PublicationPolicy::new(true, false),
            reachable_snapshot(),
            Some(endpoint()),
            100,
            3600,
        )
        .expect("build") else {
            panic!("expected direct");
        };
        let rendered = format!("{snapshot:?}");
        // Only public option values appear; labels stay structural.
        assert!(rendered.contains("Direct"));
        assert!(!rendered.contains("private"));
        assert!(!rendered.contains("secret"));
    }
}
