//! Plan 292 listener proxy authentication (runtime-neutral).
//!
//! Credential verifiers for the HTTP/CONNECT/SOCKS listeners. The
//! scheme is deliberately narrow:
//!
//! - one SHA-256 verifier per tunnel over
//!   `username:realm:password` (documented i2pr scheme; the realm
//!   binds the verifier to its listener family so a verifier stolen
//!   from one family does not verify on another);
//! - plaintext passwords never reach storage: persisted
//!   `proxy_password` values always carry [`PROXY_VERIFIER_MARKER`]
//!   and hold the hex verifier, never the password;
//! - verification is constant-time over the recomputed digest;
//! - no credential (username, password, verifier, or digest) ever
//!   appears in `Debug`, errors, logs, or control output.
//!
//! HTTP clients authenticate with `Proxy-Authorization: Basic`
//! (RFC 7617); SOCKS5 clients use RFC 1929 username/password
//! subnegotiation. Digest authentication is not implemented: Basic
//! over loopback plus a stored verifier (not a stored password) is
//! the documented mechanism.

#![forbid(unsafe_code)]

use crate::errors::ServiceTunnelError;
use base64ct::Encoding as _;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Realm binding HTTP proxy credential verifiers.
pub const PROXY_AUTH_REALM_HTTP: &str = "i2pr-http-proxy";
/// Realm binding CONNECT credential verifiers.
pub const PROXY_AUTH_REALM_CONNECT: &str = "i2pr-connect";
/// Realm binding SOCKS5 credential verifiers.
pub const PROXY_AUTH_REALM_SOCKS: &str = "i2pr-socks-proxy";
/// Stored-verifier marker: persisted `proxy_password` values always
/// start with this prefix (plaintext never reaches storage).
pub const PROXY_VERIFIER_MARKER: &str = "$i2pr1$";
/// Username ceiling in bytes.
pub const MAX_PROXY_USERNAME_LEN: usize = 128;
/// Password ceiling in bytes.
pub const MAX_PROXY_PASSWORD_LEN: usize = 512;

/// Listener credential verifier (SHA-256 over
/// `username:realm:password`; the plaintext password is dropped at
/// construction and never stored).
#[derive(Clone, Eq, PartialEq)]
pub struct ProxyCredentials {
    username: String,
    realm: &'static str,
    verifier: [u8; 32],
}

impl core::fmt::Debug for ProxyCredentials {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ProxyCredentials(<redacted>)")
    }
}

impl ProxyCredentials {
    /// Builds a verifier from explicit credentials. Both halves are
    /// required; the username must not contain `:` (Basic framing)
    /// and neither half may contain NUL.
    pub fn new(
        username: &str,
        password: &str,
        realm: &'static str,
    ) -> Result<Self, ServiceTunnelError> {
        check_credential_text("proxy_username", username, MAX_PROXY_USERNAME_LEN)?;
        if username.contains(':') {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "proxy_username",
                reason: "must not contain ':'",
            });
        }
        check_credential_text("proxy_password", password, MAX_PROXY_PASSWORD_LEN)?;
        Ok(Self {
            username: username.to_owned(),
            realm,
            verifier: digest_credentials(username, realm, password),
        })
    }

    /// Rebuilds credentials from a persisted marked verifier (see
    /// [`Self::stored_form`]). Rejects unmarked or malformed values
    /// so plaintext can never be mistaken for a verifier. The realm
    /// stays bound inside the stored digest; callers pass the tunnel
    /// family's realm so later verification mixes the same value.
    pub fn from_stored(
        username: &str,
        stored: &str,
        realm: &'static str,
    ) -> Result<Self, ServiceTunnelError> {
        check_credential_text("proxy_username", username, MAX_PROXY_USERNAME_LEN)?;
        if username.contains(':') {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "proxy_username",
                reason: "must not contain ':'",
            });
        }
        let hex = stored.strip_prefix(PROXY_VERIFIER_MARKER).ok_or_else(|| {
            ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "stored proxy verifier must carry the verifier marker",
            }
        })?;
        let verifier =
            decode_verifier_hex(hex).ok_or_else(|| ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "stored proxy verifier is malformed",
            })?;
        Ok(Self {
            username: username.to_owned(),
            realm,
            verifier,
        })
    }

    /// Returns the persisted form (marker plus hex verifier), safe
    /// for generation files and mirrors; filtered from control
    /// output by key like every secret-classified option.
    pub fn stored_form(&self) -> String {
        let mut out = String::with_capacity(PROXY_VERIFIER_MARKER.len() + 64);
        out.push_str(PROXY_VERIFIER_MARKER);
        for byte in self.verifier {
            out.push(HEX_DIGITS[(byte >> 4) as usize]);
            out.push(HEX_DIGITS[(byte & 0x0F) as usize]);
        }
        out
    }

    /// Returns the realm these credentials are bound to (non-secret
    /// family label, safe for challenges and diagnostics).
    pub fn realm(&self) -> &'static str {
        self.realm
    }

    /// Verifies a presented username/password pair in constant time
    /// over the recomputed digest (the username comparison is an
    /// early exact check; the digest comparison never short-circuits
    /// on content). The construction realm binds the check to its
    /// listener family.
    pub fn verify(&self, username: &str, password: &str) -> bool {
        if username != self.username {
            return false;
        }
        let candidate = digest_credentials(username, self.realm, password);
        self.verifier.ct_eq(&candidate).into()
    }
}

/// Bounds-checks one credential half (non-empty, ceiling, no NUL).
fn check_credential_text(
    field: &'static str,
    value: &str,
    ceiling: usize,
) -> Result<(), ServiceTunnelError> {
    if value.is_empty() || value.len() > ceiling || value.contains('\0') {
        return Err(ServiceTunnelError::ExceedsCeiling {
            field,
            reason: "credential half must be non-empty, bounded, and NUL-free",
        });
    }
    Ok(())
}

/// SHA-256 over `username:realm:password`.
fn digest_credentials(username: &str, realm: &str, password: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(username.as_bytes());
    hasher.update(b":");
    hasher.update(realm.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    hasher.finalize().into()
}

const HEX_DIGITS: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

/// Decodes exactly 64 lowercase hex characters into 32 bytes.
fn decode_verifier_hex(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0_u8; 32];
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i < 32 {
        let hi = hex_value(bytes[2 * i])?;
        let lo = hex_value(bytes[2 * i + 1])?;
        out[i] = (hi << 4) | lo;
        i += 1;
    }
    Some(out)
}

/// Single lowercase hex digit value (`None` otherwise; uppercase
/// rejected so stored forms are canonical).
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Decodes one `Basic <base64>` header value into its
/// `(username, password)` halves. Rejects malformed framing without
/// distinguishing cases (callers answer 407 either way).
pub fn decode_basic_credentials(value: &str) -> Option<(String, String)> {
    let encoded = value.strip_prefix("Basic ")?;
    if encoded.is_empty() || encoded.len() > 2048 {
        return None;
    }
    let decoded = base64ct::Base64::decode_vec(encoded).ok()?;
    let text = core::str::from_utf8(&decoded).ok()?;
    let (username, password) = text.split_once(':')?;
    if username.is_empty() || password.is_empty() {
        return None;
    }
    Some((username.to_owned(), password.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_stored_form() {
        let creds =
            ProxyCredentials::new("alice", "s3cret!", PROXY_AUTH_REALM_HTTP).expect("credentials");
        let stored = creds.stored_form();
        assert!(stored.starts_with(PROXY_VERIFIER_MARKER));
        assert!(!stored.contains("s3cret"));
        let rebuilt = ProxyCredentials::from_stored("alice", &stored, PROXY_AUTH_REALM_HTTP)
            .expect("rebuild");
        assert_eq!(creds, rebuilt);
        assert!(rebuilt.verify("alice", "s3cret!"));
        assert!(!rebuilt.verify("alice", "wrong"));
        assert!(!rebuilt.verify("bob", "s3cret!"));
        // A verifier built for another family does not verify here:
        // realms bind verifiers to their listener family.
        let foreign =
            ProxyCredentials::new("alice", "s3cret!", PROXY_AUTH_REALM_SOCKS).expect("foreign");
        assert_ne!(creds, foreign);
        assert_ne!(creds.stored_form(), foreign.stored_form());
        assert!(foreign.verify("alice", "s3cret!"));
    }

    #[test]
    fn plaintext_is_never_a_stored_form() {
        assert!(ProxyCredentials::from_stored("alice", "s3cret!", PROXY_AUTH_REALM_HTTP).is_err());
        assert!(
            ProxyCredentials::from_stored("alice", "$i2pr1$ZZZ", PROXY_AUTH_REALM_HTTP).is_err()
        );
        // Uppercase hex is rejected (canonical lowercase only).
        let creds =
            ProxyCredentials::new("alice", "s3cret!", PROXY_AUTH_REALM_HTTP).expect("credentials");
        let upper = creds.stored_form().to_uppercase();
        assert!(ProxyCredentials::from_stored("alice", &upper, PROXY_AUTH_REALM_HTTP).is_err());
    }

    #[test]
    fn bounds_and_framing_rejected() {
        assert!(ProxyCredentials::new("", "pw", PROXY_AUTH_REALM_HTTP).is_err());
        assert!(ProxyCredentials::new("user", "", PROXY_AUTH_REALM_HTTP).is_err());
        assert!(ProxyCredentials::new("a:b", "pw", PROXY_AUTH_REALM_HTTP).is_err());
        assert!(ProxyCredentials::new("u\0", "pw", PROXY_AUTH_REALM_HTTP).is_err());
        let long_user = "u".repeat(MAX_PROXY_USERNAME_LEN + 1);
        assert!(ProxyCredentials::new(&long_user, "pw", PROXY_AUTH_REALM_HTTP).is_err());
        let long_pass = "p".repeat(MAX_PROXY_PASSWORD_LEN + 1);
        assert!(ProxyCredentials::new("user", &long_pass, PROXY_AUTH_REALM_HTTP).is_err());
    }

    #[test]
    fn debug_never_leaks() {
        let creds =
            ProxyCredentials::new("alice", "s3cret!", PROXY_AUTH_REALM_HTTP).expect("credentials");
        let debug = format!("{creds:?}");
        assert!(!debug.contains("alice"));
        assert!(!debug.contains("s3cret"));
        assert!(!debug.contains(&creds.stored_form()[8..16]));
    }

    #[test]
    fn basic_header_decodes() {
        // "alice:s3cret!" in Base64.
        let (user, pass) = decode_basic_credentials("Basic YWxpY2U6czNjcmV0IQ==").expect("decodes");
        assert_eq!(user, "alice");
        assert_eq!(pass, "s3cret!");
        assert!(decode_basic_credentials("Basic !!!").is_none());
        assert!(decode_basic_credentials("Digest abc").is_none());
        assert!(decode_basic_credentials("Basic bm9jb2xvbg==").is_none());
        assert!(decode_basic_credentials("Basic OjEyMw==").is_none());
    }
}
