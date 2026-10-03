//! Plan 297: explicit local TLS identity and trust policy for
//! server `use_ssl`.
//!
//! The policy is operator-owned daemon configuration, never wire
//! data: where the tunnel endpoint's TLS identity comes from
//! (provisioned PEM, never silent self-signature), how the loopback
//! target's certificate is verified (exact-certificate pins and/or
//! explicit trust roots through the standard WebPKI verifier, never
//! ambient system roots and never a verification bypass), and how
//! rotation and expiry surface (restart to rotate; the provisioned
//! identity expiry and per-runtime handshake counters surface
//! through control state).
//!
//! Verification precedence is structural: pinned end-entity
//! certificates and explicit roots share one trust store, so either
//! acceptance path verifies. A policy with neither is rejected at
//! load, and `use_ssl` dials without any installed policy fail
//! before connecting. Verification failure fails the connection
//! (typed, counted by the caller); there is no plaintext fallback
//! on any path. Secret key bytes never appear in `Debug`, errors,
//! logs, or control output.
//!
//! Deliberately no `dangerous()` custom verifier: the
//! runtime-boundary guardrail forbids verification bypass in
//! production daemon source, so pinning is exact-certificate
//! trust anchors (not hash comparisons) and there is no
//! unauthenticated opt-in.

use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{ClientConfig, Error as TlsError};
use tokio::net::TcpStream;

/// Typed service-tunnel TLS failures. Messages name what failed,
/// never key material, certificate bytes, or pin values.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServiceTlsError {
    /// The identity section names only one of the pair.
    #[error("tls identity requires both certificate and private key paths")]
    IncompleteIdentity,
    /// A configured file could not be read.
    #[error("cannot read tls {what}")]
    CannotRead {
        /// Which file role failed (certificate, private key, pins, roots).
        what: &'static str,
    },
    /// A PEM block could not be parsed.
    #[error("cannot parse tls {what}")]
    CannotParse {
        /// Which material failed (certificate, private key, pins, roots).
        what: &'static str,
    },
    /// The certificate PEM holds no certificate.
    #[error("tls certificate PEM holds no certificate")]
    EmptyChain,
    /// A trust bundle PEM holds no certificate.
    #[error("tls trust bundle PEM holds no certificate")]
    EmptyRoots,
    /// The end-entity certificate does not parse for expiry.
    #[error("tls certificate does not parse as X.509")]
    BadCertificate,
    /// The certificate and key do not combine into a client identity.
    #[error("tls certificate and key do not combine")]
    IdentityMismatch,
    /// Neither pins, nor roots verify anything.
    #[error("tls policy verifies nothing: configure pinned certificates or trust roots")]
    VerifiesNothing,
    /// No TLS policy is installed on the manager.
    #[error("use_ssl requires a daemon TLS policy")]
    NoPolicy,
    /// The TLS handshake failed (typed, counted; never falls back
    /// to plaintext).
    #[error("tls handshake failed: {0}")]
    Handshake(String),
}

/// Loaded endpoint TLS identity (provisioned PEM, optional).
/// The private key zeroizes on drop through
/// `rustls-pki-types`; `Debug` is redacted unconditionally.
pub struct LoadedIdentity {
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
    /// End-entity expiry as Unix seconds, surfaced through control
    /// state so rotation deadlines are observable.
    expires_unix: u64,
}

impl LoadedIdentity {
    /// End-entity certificate expiry as Unix seconds.
    pub const fn expires_unix(&self) -> u64 {
        self.expires_unix
    }
}

impl core::fmt::Debug for LoadedIdentity {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("LoadedIdentity")
            .field("chain_len", &self.chain.len())
            .field("key", &"<redacted>")
            .field("expires_unix", &self.expires_unix)
            .finish()
    }
}

/// Explicit local TLS identity and trust policy. Built once from
/// daemon configuration and shared across dials; `Debug` reports
/// shapes and counts only.
pub struct ServiceTlsPolicy {
    identity: Option<LoadedIdentity>,
    pinned_count: usize,
    roots: Arc<rustls::RootCertStore>,
    has_explicit_roots: bool,
}

impl ServiceTlsPolicy {
    /// Builds a policy from loaded file bytes (the daemon
    /// configuration owns file I/O; this constructor owns
    /// validation). `identity` is the optional PEM pair
    /// `(certificate, private_key)`; `pins_pem` is an optional PEM
    /// bundle of exact end-entity certificates used as trust
    /// anchors; `roots_pem` is an optional PEM bundle of explicit
    /// trust roots. Ambient system roots are never consulted.
    pub fn from_parts(
        identity: Option<(Vec<u8>, Vec<u8>)>,
        pins_pem: Option<Vec<u8>>,
        roots_pem: Option<Vec<u8>>,
    ) -> Result<Self, ServiceTlsError> {
        let identity = identity
            .map(|(cert_pem, key_pem)| load_identity(&cert_pem, &key_pem))
            .transpose()?;
        let pinned: Vec<CertificateDer<'static>> = pins_pem
            .map(|pem| load_bundle(&pem, "pins"))
            .transpose()?
            .unwrap_or_default();
        let roots_certs: Vec<CertificateDer<'static>> = roots_pem
            .map(|pem| load_bundle(&pem, "roots"))
            .transpose()?
            .unwrap_or_default();
        if pinned.is_empty() && roots_certs.is_empty() {
            return Err(ServiceTlsError::VerifiesNothing);
        }
        let pinned_count = pinned.len();
        let has_explicit_roots = !roots_certs.is_empty();
        let mut store = rustls::RootCertStore::empty();
        store.add_parsable_certificates(pinned);
        store.add_parsable_certificates(roots_certs);
        if store.is_empty() {
            return Err(ServiceTlsError::VerifiesNothing);
        }
        Ok(Self {
            identity,
            pinned_count,
            roots: Arc::new(store),
            has_explicit_roots,
        })
    }

    /// Short verification-mode name for control state
    /// (`pin`, `roots`, `pin-and-roots`).
    pub fn verify_mode(&self) -> &'static str {
        match (self.pinned_count > 0, self.has_explicit_roots) {
            (true, true) => "pin-and-roots",
            (true, false) => "pin",
            (false, true) => "roots",
            (false, false) => "unconfigured",
        }
    }

    /// Provisioned identity expiry as Unix seconds, if an identity
    /// is configured.
    pub fn identity_expires_unix(&self) -> Option<u64> {
        self.identity.as_ref().map(LoadedIdentity::expires_unix)
    }

    /// Number of pinned end-entity certificates (shape only).
    pub const fn pin_count(&self) -> usize {
        self.pinned_count
    }

    /// Whether explicit trust roots are configured (shape only:
    /// pinned and root anchors share one store).
    pub const fn has_roots(&self) -> bool {
        self.has_explicit_roots
    }

    /// Builds one client configuration from the policy. A fresh
    /// configuration per dial keeps rotation (via restart) and
    /// per-policy isolation simple at loopback scale. Verification
    /// is always the standard WebPKI path over the operator's
    /// anchors; there is no bypass hook anywhere in this module.
    pub fn client_config(&self) -> Result<ClientConfig, ServiceTlsError> {
        let builder =
            ClientConfig::builder().with_root_certificates(Arc::clone(&self.roots));
        match &self.identity {
            Some(loaded) => builder
                .with_client_auth_cert(loaded.chain.clone(), loaded.key.clone_key())
                .map_err(|_| ServiceTlsError::IdentityMismatch),
            None => Ok(builder.with_no_client_auth()),
        }
    }
}

impl core::fmt::Debug for ServiceTlsPolicy {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ServiceTlsPolicy")
            .field("identity", &self.identity)
            .field("pin_count", &self.pinned_count)
            .field("has_roots", &self.has_roots())
            .field("verify_mode", &self.verify_mode())
            .finish()
    }
}

/// Shared explicit TLS policy handle for daemon configuration.
///
/// Equality compares the observable policy shape (verification
/// mode, identity expiry, pin count, roots presence) — which is
/// what configuration equality needs. Key bytes, pins, and
/// certificate bytes are never compared or printed.
#[derive(Clone, Debug)]
pub struct TlsPolicyHandle(Arc<ServiceTlsPolicy>);

impl TlsPolicyHandle {
    /// Wraps a loaded policy for sharing.
    pub fn new(policy: ServiceTlsPolicy) -> Self {
        Self(Arc::new(policy))
    }

    /// Shares the underlying policy.
    pub fn policy(&self) -> Arc<ServiceTlsPolicy> {
        Arc::clone(&self.0)
    }
}

impl PartialEq for TlsPolicyHandle {
    fn eq(&self, other: &Self) -> bool {
        self.0.verify_mode() == other.0.verify_mode()
            && self.0.identity_expires_unix() == other.0.identity_expires_unix()
            && self.0.pin_count() == other.0.pin_count()
            && self.0.has_roots() == other.0.has_roots()
    }
}

impl Eq for TlsPolicyHandle {}

/// Loads a PEM trust bundle (pinned end-entity certificates or
/// explicit roots): at least one parseable certificate, never
/// ambient roots.
fn load_bundle(pem: &[u8], what: &'static str) -> Result<Vec<CertificateDer<'static>>, ServiceTlsError> {
    use rustls::pki_types::pem::PemObject as _;
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ServiceTlsError::CannotParse { what })?;
    if certs.is_empty() {
        return Err(ServiceTlsError::EmptyRoots);
    }
    Ok(certs)
}

/// Loads and validates the provisioned endpoint identity: at least
/// one PEM certificate, a parseable private key, and an X.509
/// end-entity expiry for control state.
fn load_identity(cert_pem: &[u8], key_pem: &[u8]) -> Result<LoadedIdentity, ServiceTlsError> {
    use rustls::pki_types::pem::PemObject as _;
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ServiceTlsError::CannotParse {
            what: "certificate",
        })?;
    if chain.is_empty() {
        return Err(ServiceTlsError::EmptyChain);
    }
    let key = PrivateKeyDer::from_pem_slice(key_pem).map_err(|_| ServiceTlsError::CannotParse {
        what: "private key",
    })?;
    let expires_unix = certificate_expiry_unix(&chain[0])?;
    Ok(LoadedIdentity {
        chain,
        key,
        expires_unix,
    })
}

/// Extracts the end-entity X.509 expiry as Unix seconds.
fn certificate_expiry_unix(cert_der: &CertificateDer<'_>) -> Result<u64, ServiceTlsError> {
    use x509_parser::prelude::FromDer as _;
    let (_, parsed) = x509_parser::certificate::X509Certificate::from_der(cert_der.as_ref())
        .map_err(|_| ServiceTlsError::BadCertificate)?;
    let timestamp = parsed.tbs_certificate.validity().not_after.timestamp();
    u64::try_from(timestamp).map_err(|_| ServiceTlsError::BadCertificate)
}

/// Negotiates TLS over an established loopback TCP stream to the
/// target address using the policy. Verification failure fails the
/// connection (typed, counted by the caller); there is no plaintext
/// fallback on any path.
pub async fn tls_connect(
    stream: TcpStream,
    target: std::net::SocketAddr,
    policy: &ServiceTlsPolicy,
) -> Result<tokio_rustls::client::TlsStream<TcpStream>, ServiceTlsError> {
    let config = policy.client_config()?;
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let server_name = ServerName::from(target.ip());
    connector
        .connect(server_name, stream)
        .await
        .map_err(|error| ServiceTlsError::Handshake(error.to_string()))
}

/// Reads one file for the TLS policy loader, naming only the role
/// on failure (never the path contents, and paths stay out of
/// error strings by house rule).
pub fn read_tls_file(what: &'static str, path: &Path) -> Result<Vec<u8>, ServiceTlsError> {
    std::fs::read(path).map_err(|_| ServiceTlsError::CannotRead { what })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rcgen_self_signed() -> (Vec<u8>, Vec<u8>) {
        let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()])
            .expect("self-signed cert");
        let cert_pem = certified.cert.pem().into_bytes();
        let key_pem = certified.key_pair.serialize_pem().into_bytes();
        (cert_pem, key_pem)
    }

    #[test]
    fn policy_without_verification_is_rejected() {
        assert!(matches!(
            ServiceTlsPolicy::from_parts(None, None, None),
            Err(ServiceTlsError::VerifiesNothing)
        ));
        assert!(matches!(
            ServiceTlsPolicy::from_parts(None, Some(Vec::new()), None),
            Err(ServiceTlsError::EmptyRoots)
        ));
    }

    #[test]
    fn pinned_certificate_builds_pin_mode() {
        let (cert_pem, _) = rcgen_self_signed();
        let policy = ServiceTlsPolicy::from_parts(None, Some(cert_pem), None)
            .expect("pinned policy builds");
        assert_eq!(policy.verify_mode(), "pin");
        assert_eq!(policy.pin_count(), 1);
        assert!(policy.identity_expires_unix().is_none());
        policy.client_config().expect("client config builds");
        let debug = format!("{policy:?}");
        assert!(!debug.contains("-----BEGIN"), "no PEM in Debug");
    }

    #[test]
    fn identity_alone_verifies_nothing() {
        // Plan 297: an identity without trust anchors verifies
        // nothing (the endpoint identity authenticates us, never
        // the target).
        let (cert_pem, key_pem) = rcgen_self_signed();
        assert!(matches!(
            ServiceTlsPolicy::from_parts(Some((cert_pem, key_pem)), None, None),
            Err(ServiceTlsError::VerifiesNothing)
        ));
    }

    #[test]
    fn identity_with_pins_reports_expiry() {
        let (cert_pem, key_pem) = rcgen_self_signed();
        let policy = ServiceTlsPolicy::from_parts(
            Some((cert_pem.clone(), key_pem)),
            Some(cert_pem),
            None,
        )
        .expect("pinned identity builds");
        assert_eq!(policy.verify_mode(), "pin");
        let expires = policy.identity_expires_unix().expect("expiry surfaces");
        assert!(expires > 1_700_000_000, "expiry is a real Unix time");
        policy.client_config().expect("client config builds");
        let debug = format!("{policy:?}");
        assert!(!debug.contains("-----BEGIN"), "no PEM in Debug");
    }

    #[test]
    fn mismatched_identity_pair_is_rejected() {
        let (cert_pem, _) = rcgen_self_signed();
        let (_, other_key_pem) = rcgen_self_signed();
        let (pin_pem, _) = rcgen_self_signed();
        let policy = ServiceTlsPolicy::from_parts(
            Some((cert_pem, other_key_pem)),
            Some(pin_pem),
            None,
        )
        .expect("policy builds (pairing is checked at client-config build)");
        assert!(matches!(
            policy.client_config(),
            Err(ServiceTlsError::IdentityMismatch)
        ));
    }

    #[test]
    fn roots_bundle_loads_and_verifies_mode() {
        let (cert_pem, _) = rcgen_self_signed();
        let policy = ServiceTlsPolicy::from_parts(None, None, Some(cert_pem))
            .expect("roots policy builds");
        assert_eq!(policy.verify_mode(), "roots");
        policy.client_config().expect("client config builds");
    }

    #[test]
    fn garbage_bundles_are_rejected() {
        // Non-PEM bytes hold no certificates; a truncated PEM
        // fails parsing. Both are typed, never silent.
        assert!(matches!(
            ServiceTlsPolicy::from_parts(None, Some(b"not-pem".to_vec()), None),
            Err(ServiceTlsError::EmptyRoots)
        ));
        assert!(matches!(
            ServiceTlsPolicy::from_parts(
                None,
                Some(b"-----BEGIN CERTIFICATE-----\n!!!\n-----END CERTIFICATE-----\n".to_vec()),
                None
            ),
            Err(ServiceTlsError::CannotParse { .. })
        ));
    }
}
