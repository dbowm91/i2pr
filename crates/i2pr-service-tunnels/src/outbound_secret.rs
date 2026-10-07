//! Plan 341: the runtime-neutral capability that owns outbound proxy
//! secrets.
//!
//! [`crate::auth::ProxyCredentials`] is deliberately **one-way**: it keeps
//! `SHA-256(username:realm:password)` and drops the password at construction,
//! because an inbound listener only ever needs to *verify* a presented pair.
//! An I2P-routed outproxy is the opposite case — the router must *send* the
//! credential upstream in a `Proxy-Authorization: Basic` header — so a
//! verifier cannot produce it, and after a restart no plaintext exists to
//! send. That asymmetry is why Plan 327 was closed blocked instead of
//! partially built.
//!
//! This module is the policy half of the answer and holds no cryptography:
//! `i2pr-service-tunnels` is permitted only `i2pr-client` and `i2pr-proto`
//! internally, so it cannot depend on an AEAD. It defines the
//! [`OutboundSecretStore`] trait, the stored-form framing that a generation
//! file carries, and a fail-closed default. The daemon supplies the concrete
//! router-bound implementation, exactly as it already injects
//! `RouterDeliveryService` and the local sink.
//!
//! # What crosses this boundary
//!
//! A *sealed* form: opaque bytes whose meaning no reader of a generation file
//! can recover. What must never cross is plaintext in a `Debug`, a `Display`,
//! an error, or a log. The `open` signature therefore returns a
//! zeroize-on-drop value that the caller consumes while building a header and
//! then drops; it is not a `String` that can be logged by accident.

#![forbid(unsafe_code)]

use zeroize::Zeroizing;

use crate::errors::ServiceTunnelError;

/// Marker prefix on every persisted sealed form.
///
/// A distinct marker from [`crate::auth::PROXY_VERIFIER_MARKER`] on purpose:
/// an inbound *verifier* and an outbound *sealed secret* are different kinds
/// of value, and neither may ever be mistaken for the other. A stored form
/// missing this marker is rejected rather than reinterpreted.
pub const OUTBOUND_SECRET_MARKER: &str = "$i2pr1o$";

/// Marker prefix on every persisted ELS2 consumer-credential form.
///
/// Plan 380 adds a second sealed secret under the same router identity, and it
/// gets its own marker for the same reason [`OUTBOUND_SECRET_MARKER`] is
/// distinct from [`crate::auth::PROXY_VERIFIER_MARKER`]: three kinds of value
/// that must never be mistaken for one another. A stored form carrying this
/// marker is a consumer credential and cannot be opened by the outproxy
/// domain, because the two are derived under different labels and authenticate
/// different associated data.
pub const ELS2_CONSUMER_CREDENTIAL_MARKER: &str = "$i2pr1e$";

/// Longest plaintext this interface will seal, in bytes.
///
/// Matches the inbound password ceiling in [`crate::auth`] so a credential
/// that is legal to configure is legal to seal, and no larger value can force
/// an unbounded allocation on the way in.
pub const MAX_OUTBOUND_SECRET_LEN: usize = 512;

/// Longest stored form this interface will accept, in bytes.
///
/// A sealed form is a hex-encoded nonce, ciphertext, and Poly1305 tag, so the
/// stored length is a deterministic function of the plaintext length. This
/// ceiling is a backstop that rejects an absurd frame before any decode work
/// is attempted.
pub const MAX_OUTBOUND_SECRET_STORED_LEN: usize = 2 * (MAX_OUTBOUND_SECRET_LEN + 64) + 16;

/// A plaintext outbound credential, zeroized when it leaves scope.
///
/// This is the only type in the crate that carries outbound secret bytes, and
/// it deliberately does not implement `Debug`, `Display`, or `Clone`: there is
/// no way to print it, copy it, or put it in a log line by accident. A caller
/// that needs the bytes builds its header and lets the value drop.
pub struct OutboundSecret {
    bytes: Zeroizing<[u8; MAX_OUTBOUND_SECRET_LEN]>,
    len: usize,
}

impl OutboundSecret {
    /// Wraps plaintext after bounds and framing checks.
    pub fn new(plaintext: &str) -> Result<Self, ServiceTunnelError> {
        if plaintext.is_empty() || plaintext.len() > MAX_OUTBOUND_SECRET_LEN {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "outbound_secret",
                reason: "outbound secret must be non-empty and bounded",
            });
        }
        if plaintext.contains('\0') {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "outbound_secret",
                reason: "outbound secret must be NUL-free",
            });
        }
        // A fixed-size zeroizing buffer, not a `String` and not a `Vec`:
        // this crate does not enable `zeroize/alloc`, and more importantly a
        // secret should not sit in a heap allocation at all. The ceiling is
        // already a compile-time constant, so the type carries its own bound
        // and no length check can be forgotten at a call site.
        let mut bytes = Zeroizing::new([0_u8; MAX_OUTBOUND_SECRET_LEN]);
        bytes[..plaintext.len()].copy_from_slice(plaintext.as_bytes());
        Ok(Self {
            bytes,
            len: plaintext.len(),
        })
    }

    /// Borrows the plaintext bytes for immediate header construction.
    ///
    /// The borrow cannot outlive the `OutboundSecret`, and the whole buffer is
    /// zeroized when it drops, so the credential's lifetime is the caller's
    /// scope rather than the process's.
    pub fn expose(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Borrows the plaintext as UTF-8 for callers building text.
    pub fn expose_str(&self) -> Result<&str, ServiceTunnelError> {
        core::str::from_utf8(self.expose()).map_err(|_| ServiceTunnelError::ContradictoryOptions {
            id: String::new(),
            reason: "outbound secret is not valid UTF-8",
        })
    }

    /// Plaintext length in bytes. Non-secret, and safe to log.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the plaintext is empty. Always `false` for a constructed
    /// value; present so the type satisfies the usual length convention.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// The capability the tunnel-policy layer uses to persist and recover an
/// outbound proxy credential.
///
/// Implementations are supplied by the composition root, which is the only
/// layer holding key material. Every method is total and fallible: there is no
/// "best effort" path, because a partially recovered credential would be sent
/// to an upstream outproxy.
///
/// `Send + Sync` is a requirement of the design, not a convenience (Plan
/// 342). One store is shared as an `Arc` between the control plane, which
/// seals a credential into a generation file, and the tunnel runtime, which
/// opens it while building a header — and the runtime's work happens inside a
/// spawned task. A store that could not cross that boundary would have to be
/// re-derived per task, which is precisely the "second key" Plan 342 rules out.
/// Every method takes `&self` and an implementation needs no interior
/// mutability, so the bound costs nothing to honour.
pub trait OutboundSecretStore: Send + Sync {
    /// Seals `secret` into a stored form for a generation file.
    fn seal(&self, secret: &OutboundSecret) -> Result<String, ServiceTunnelError>;

    /// Opens a stored form back into plaintext.
    ///
    /// Returns an error — never a partial or empty value — when the form is
    /// unmarked, malformed, truncated, tampered, or was sealed by a different
    /// router.
    fn open(&self, stored: &str) -> Result<OutboundSecret, ServiceTunnelError>;

    /// Whether this store can seal at all. A `false` here must make tunnel
    /// creation fail **before** any listener or destination is allocated,
    /// rather than failing later at connect time.
    fn is_available(&self) -> bool;
}

/// A router-bound owner of **both** sealed secret domains (Plan 380).
///
/// Plan 341 established one router-bound key for the outbound proxy credential.
/// Plan 380 needs a second sealed secret — the ELS2 consumer client credential
/// — under the same router identity, but deliberately **not** under the same
/// key: the concrete implementation derives each domain from a distinct HKDF
/// label and authenticates a distinct marker, so a stored form from one domain
/// fails to open in the other rather than being silently interchangeable.
///
/// This is a supertrait rather than a second field on the composition root's
/// configuration so that "derive once, share one `Arc`" stays true: one owner,
/// two capabilities, no second key and no second `Arc` to keep in step. The
/// `OutboundSecretStore` half is inherited unchanged, which is what lets the
/// outproxy runtime keep holding a plain `Arc<dyn OutboundSecretStore>`.
///
/// Every method is total and fallible, exactly as on the parent trait. There is
/// no best-effort path: a credential that cannot be opened is refused, because
/// the alternative is presenting a client with a service it is not authorized
/// for.
pub trait RouterSecretOwner: OutboundSecretStore {
    /// Seals an ELS2 consumer credential into its stored form.
    fn seal_credential(&self, secret: &OutboundSecret) -> Result<String, ServiceTunnelError>;

    /// Opens an ELS2 consumer credential's stored form back into plaintext.
    fn open_credential(&self, stored: &str) -> Result<OutboundSecret, ServiceTunnelError>;
}

/// The default store: refuses everything.
///
/// Used wherever no composition root installed a real owner — tests, and any
/// profile that has not wired one. It is not a stub that returns a default
/// value; it is the fail-closed answer, and it makes "no owner installed" a
/// state the type system can carry instead of an `Option` a caller forgets to
/// check.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOutboundSecrets;

impl OutboundSecretStore for NoOutboundSecrets {
    fn seal(&self, _secret: &OutboundSecret) -> Result<String, ServiceTunnelError> {
        Err(unavailable("no outbound secret owner is installed"))
    }

    fn open(&self, _stored: &str) -> Result<OutboundSecret, ServiceTunnelError> {
        Err(unavailable("no outbound secret owner is installed"))
    }

    fn is_available(&self) -> bool {
        false
    }
}

/// Plan 380: the fail-closed default refuses the consumer credential too, and
/// the reason is deliberately the same one an operator sees for an outproxy
/// credential. "No secret owner is installed" is a composition-root fact, and
/// the two domains are not distinguishable to an operator who has not fixed the
/// router identity — naming the domain here would tell them a secret was
/// involved when the real problem is that nothing can seal one.
impl RouterSecretOwner for NoOutboundSecrets {
    fn seal_credential(&self, _secret: &OutboundSecret) -> Result<String, ServiceTunnelError> {
        Err(unavailable("no outbound secret owner is installed"))
    }

    fn open_credential(&self, _stored: &str) -> Result<OutboundSecret, ServiceTunnelError> {
        Err(unavailable("no outbound secret owner is installed"))
    }
}

/// The single error every unavailable-owner path returns.
fn unavailable(reason: &'static str) -> ServiceTunnelError {
    ServiceTunnelError::ContradictoryOptions {
        id: String::new(),
        reason,
    }
}

/// Validates a stored form's framing without decrypting it.
///
/// Generation loading calls this before handing a value to a store, so an
/// unmarked or absurdly long frame is rejected with a bounded, non-secret
/// error instead of reaching a decoder. The check is intentionally
/// conservative: it proves the shape, never the contents.
pub fn validate_stored_form(stored: &str) -> Result<(), ServiceTunnelError> {
    validate_sealed_form(stored, OUTBOUND_SECRET_MARKER)
}

/// The Plan 380 counterpart of [`validate_stored_form`] for the ELS2 consumer
/// credential domain.
///
/// Separate rather than parameterized at the call sites so that the marker is
/// never a value a caller can choose: passing the wrong domain's marker would
/// accept a frame belonging to the other domain.
///
/// The reason strings are deliberately identical to the outproxy domain's. The
/// two are told apart by which marker they accept, not by what they say, so a
/// stored form cannot be probed for its domain by comparing error text.
pub fn validate_els2_credential_form(stored: &str) -> Result<(), ServiceTunnelError> {
    validate_sealed_form(stored, ELS2_CONSUMER_CREDENTIAL_MARKER)
}

/// The shared shape check both domains apply.
fn validate_sealed_form(stored: &str, marker: &str) -> Result<(), ServiceTunnelError> {
    if stored.is_empty() || stored.len() > MAX_OUTBOUND_SECRET_STORED_LEN {
        return Err(ServiceTunnelError::ExceedsCeiling {
            field: "outbound_secret_stored",
            reason: "stored outbound secret must be non-empty and bounded",
        });
    }
    let Some(body) = stored.strip_prefix(marker) else {
        return Err(ServiceTunnelError::ContradictoryOptions {
            id: String::new(),
            reason: "stored outbound secret must carry the sealed-secret marker",
        });
    };
    if body.is_empty() || body.len() % 2 != 0 {
        return Err(ServiceTunnelError::ContradictoryOptions {
            id: String::new(),
            reason: "stored outbound secret body must be non-empty hex",
        });
    }
    if !body
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ServiceTunnelError::ContradictoryOptions {
            id: String::new(),
            reason: "stored outbound secret body must be lowercase hex",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_is_bounded_and_nul_free() {
        assert!(OutboundSecret::new("").is_err());
        assert!(OutboundSecret::new("a\0b").is_err());
        let over = "p".repeat(MAX_OUTBOUND_SECRET_LEN + 1);
        assert!(OutboundSecret::new(&over).is_err());
        let at = "p".repeat(MAX_OUTBOUND_SECRET_LEN);
        assert_eq!(
            OutboundSecret::new(&at).expect("at ceiling").len(),
            MAX_OUTBOUND_SECRET_LEN
        );
    }

    #[test]
    fn stored_form_framing_is_checked_before_any_decode() {
        assert!(validate_stored_form("").is_err());
        assert!(
            validate_stored_form("s3cret").is_err(),
            "plaintext is not a sealed form"
        );
        assert!(
            validate_stored_form("$i2pr1$0011223344").is_err(),
            "an inbound verifier is not an outbound sealed form"
        );
        assert!(validate_stored_form(OUTBOUND_SECRET_MARKER).is_err());
        assert!(validate_stored_form(&format!("{OUTBOUND_SECRET_MARKER}abc")).is_err());
        assert!(
            validate_stored_form(&format!("{OUTBOUND_SECRET_MARKER}ABCD")).is_err(),
            "uppercase hex is not canonical"
        );
        assert!(validate_stored_form(&format!("{OUTBOUND_SECRET_MARKER}00ff")).is_ok());
    }

    #[test]
    fn oversize_stored_form_is_rejected_without_work() {
        let huge = format!(
            "{OUTBOUND_SECRET_MARKER}{}",
            "00".repeat(MAX_OUTBOUND_SECRET_STORED_LEN)
        );
        assert!(validate_stored_form(&huge).is_err());
    }

    #[test]
    fn the_default_store_fails_closed() {
        let store = NoOutboundSecrets;
        assert!(!store.is_available());
        let secret = OutboundSecret::new("s3cret").expect("secret");
        assert!(store.seal(&secret).is_err());
        assert!(
            store
                .open(&format!("{OUTBOUND_SECRET_MARKER}00ff"))
                .is_err()
        );
    }
}
