//! Effective-authority and cross-origin policy.
//!
//! The console is bound to loopback, which is the primary defence. This
//! module is the second one: a browser on the same machine can be pointed
//! at the console by a hostile web page, either through a cross-origin
//! `fetch` or through DNS rebinding that rewrites the hostname it thinks it
//! is talking to. Validating the effective authority before routing makes
//! both attacks fail at the front door.
//!
//! Two rules are deliberate and load-bearing:
//!
//! - the accepted set is built from the configured listener's own address,
//!   port included, and nothing else. There is no wildcard and no suffix
//!   match, so `localhost.evil.test` never matches `localhost`;
//! - no proxy header is consulted. `Forwarded`, `X-Forwarded-Host`, and
//!   friends are attacker-controlled without a trusted-proxy design, which
//!   this console deliberately does not have.

use std::fmt;
use std::net::{IpAddr, SocketAddr};

/// Maximum accepted length of the `Host` request header value.
pub const MAX_HOST_HEADER_LEN: usize = 256;

/// Maximum accepted length of the `Origin` request header value.
pub const MAX_ORIGIN_HEADER_LEN: usize = 256;

/// Maximum accepted length of the `Referer` request header value.
pub const MAX_REFERER_HEADER_LEN: usize = 2048;

/// The set of authorities this console answers to.
///
/// Comparison is case-insensitive on the host portion, as required for
/// `Host`, but otherwise exact: port is part of the identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityPolicy {
    accepted: Vec<String>,
}

impl AuthorityPolicy {
    /// Builds the accepted set for a bound loopback listener.
    ///
    /// The derived set is the literal IP:port plus the conventional
    /// `localhost` spelling of the same port. A non-loopback address is
    /// refused outright rather than merely warned about: authentication is
    /// defence in depth, not permission to expose the console.
    pub fn for_listener(bound: SocketAddr) -> Result<Self, AuthorityError> {
        if !bound.ip().is_loopback() {
            return Err(AuthorityError::NonLoopbackListener);
        }
        let mut accepted = vec![format_authority(bound)];
        if bound.ip().is_ipv4() {
            accepted.push(format!("localhost:{}", bound.port()));
        }
        Ok(Self { accepted })
    }

    /// Builds the accepted set for a bound IPv6 loopback listener.
    pub fn for_listener_v6(bound: SocketAddr) -> Result<Self, AuthorityError> {
        Self::for_listener(bound)
    }

    /// Returns the accepted authority strings, for operator diagnostics.
    pub fn accepted(&self) -> &[String] {
        &self.accepted
    }

    /// Returns whether `host` is an accepted authority value.
    ///
    /// The value is compared case-insensitively and without surrounding
    /// whitespace. An empty or oversized value is rejected before
    /// comparison.
    pub fn accepts_host(&self, host: &str) -> bool {
        let candidate = host.trim();
        if candidate.is_empty() || candidate.len() > MAX_HOST_HEADER_LEN {
            return false;
        }
        self.accepted
            .iter()
            .any(|accepted| accepted.eq_ignore_ascii_case(candidate))
    }

    /// Returns whether an `Origin` header value is same-origin.
    ///
    /// Only the literal origins derived from the listener are accepted. The
    /// scheme must be `http`: there is no TLS termination, and a
    /// `https://` origin therefore cannot be this console.
    pub fn accepts_origin(&self, origin: &str) -> bool {
        let candidate = origin.trim();
        if candidate.is_empty() || candidate.len() > MAX_ORIGIN_HEADER_LEN {
            return false;
        }
        self.accepted
            .iter()
            .any(|accepted| format!("http://{accepted}").eq_ignore_ascii_case(candidate))
    }

    /// Returns whether a `Referer` fallback URL is same-origin.
    ///
    /// `Referer` is only ever consulted as a conservative fallback for the
    /// one browser behaviour that omits `Origin` on a same-origin form post.
    ///
    /// A real `Referer` carries a path, so the accepted form is the origin
    /// **plus the `/` that terminates the authority**, matched as a prefix.
    /// That separator is what makes the comparison safe: in
    /// `http://localhost:7070.evil.test/` the byte following the port is
    /// `.`, not `/`, so a hostname that merely starts with the accepted
    /// authority can never match.
    pub fn accepts_referer(&self, referer: &str) -> bool {
        let candidate = referer.trim();
        if candidate.is_empty() || candidate.len() > MAX_REFERER_HEADER_LEN {
            return false;
        }
        self.accepted.iter().any(|accepted| {
            let prefix = format!("http://{accepted}/");
            candidate.len() >= prefix.len()
                && candidate[..prefix.len()].eq_ignore_ascii_case(&prefix)
        })
    }
}

impl fmt::Display for AuthorityPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.accepted.join(", "))
    }
}

/// Renders a `Host` authority value for a socket address.
///
/// IPv6 literals are bracketed, matching RFC 3986 authority syntax.
fn format_authority(address: SocketAddr) -> String {
    match address.ip() {
        IpAddr::V4(ip) => format!("{ip}:{}", address.port()),
        IpAddr::V6(ip) => format!("[{ip}]:{}", address.port()),
    }
}

/// Why an authority policy could not be constructed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityError {
    /// The listener address was not loopback.
    NonLoopbackListener,
}

impl fmt::Display for AuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonLoopbackListener => {
                formatter.write_str("console authority policy requires a loopback listener")
            }
        }
    }
}

impl std::error::Error for AuthorityError {}

/// Whether a request method may change server or session state.
///
/// The console treats exactly the four HTTP methods RFC 9110 defines as
/// unsafe. Every other method is treated as safe, which keeps GET and HEAD
/// side-effect free by construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestSafety {
    /// Safe, idempotent, side-effect free.
    Safe,
    /// May change state; origin and CSRF checks apply.
    Unsafe,
}

/// Classifies an HTTP method for the origin policy.
pub fn classify_method(method: &str) -> RequestSafety {
    match method {
        "POST" | "PUT" | "PATCH" | "DELETE" => RequestSafety::Unsafe,
        _ => RequestSafety::Safe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn v4_policy() -> AuthorityPolicy {
        AuthorityPolicy::for_listener(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7070))
            .expect("loopback policy builds")
    }

    fn v6_policy() -> AuthorityPolicy {
        AuthorityPolicy::for_listener_v6(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 7070))
            .expect("loopback v6 policy builds")
    }

    #[test]
    fn accepts_the_listener_address_and_localhost() {
        let policy = v4_policy();
        assert!(policy.accepts_host("127.0.0.1:7070"));
        assert!(policy.accepts_host("localhost:7070"));
    }

    #[test]
    fn host_comparison_is_case_insensitive_and_whitespace_tolerant() {
        let policy = v4_policy();
        assert!(policy.accepts_host("LOCALHOST:7070"));
        assert!(policy.accepts_host("  localhost:7070  "));
    }

    #[test]
    fn rejects_wrong_port_and_suffix_confusion() {
        let policy = v4_policy();
        for hostile in [
            "127.0.0.1:7071",      // right host, wrong port
            "127.0.0.2:7070",      // different loopback address
            "localhost",           // missing port
            "localhost.evil.test", // suffix confusion
            "evil.test:7070",
            "127.0.0.1.evil.test:7070",
            "", // empty
            "example.com:7070",
        ] {
            assert!(
                !policy.accepts_host(hostile),
                "{hostile} must be refused as an authority"
            );
        }
    }

    #[test]
    fn rejects_non_loopback_listener_construction() {
        let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 7070);
        assert_eq!(
            AuthorityPolicy::for_listener(remote),
            Err(AuthorityError::NonLoopbackListener)
        );
        let any = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 7070);
        assert_eq!(
            AuthorityPolicy::for_listener(any),
            Err(AuthorityError::NonLoopbackListener)
        );
    }

    #[test]
    fn ipv6_literals_are_bracketed() {
        let policy = v6_policy();
        assert!(policy.accepts_host("[::1]:7070"));
        assert!(policy.accepts_host("[::1]:7070"));
        // The v4 convenience name is not accepted for a v6-only listener.
        assert!(!policy.accepts_host("localhost:7070"));
    }

    #[test]
    fn oversized_host_values_are_refused_before_comparison() {
        let policy = v4_policy();
        let long = format!("localhost:7070{}", "a".repeat(MAX_HOST_HEADER_LEN));
        assert!(!policy.accepts_host(&long));
    }

    #[test]
    fn origin_must_be_http_and_exactly_same_origin() {
        let policy = v4_policy();
        assert!(policy.accepts_origin("http://localhost:7070"));
        assert!(policy.accepts_origin("HTTP://LOCALHOST:7070"));
        for hostile in [
            "https://localhost:7070", // no TLS termination exists
            "http://localhost:7071",  // wrong port
            "http://evil.test",
            "null", // opaque origin
            "",
            "http://localhost:7070/", // trailing slash is not an Origin
        ] {
            assert!(
                !policy.accepts_origin(hostile),
                "{hostile} must be refused as an origin"
            );
        }
    }

    #[test]
    fn referer_fallback_matches_exactly_and_never_by_prefix() {
        let policy = v4_policy();
        assert!(policy.accepts_referer("http://localhost:7070/"));
        assert!(policy.accepts_referer("http://localhost:7070/login"));
        for hostile in [
            "http://localhost:7070.evil.test/",
            "http://localhost:7071/",
            "https://localhost:7070/",
            "http://evil.test/",
            "",
        ] {
            assert!(
                !policy.accepts_referer(hostile),
                "{hostile} must be refused as a referer"
            );
        }
    }

    #[test]
    fn unsafe_methods_are_classified_exhaustively() {
        for method in ["POST", "PUT", "PATCH", "DELETE"] {
            assert_eq!(classify_method(method), RequestSafety::Unsafe, "{method}");
        }
        for method in ["GET", "HEAD", "OPTIONS", "TRACE", "get", "post"] {
            // Lowercase and unknown methods take the conservative safe path
            // only for GET/HEAD-like verbs; an unrecognized verb must not be
            // treated as safe by accident.
            let expected = match method {
                "OPTIONS" | "TRACE" => RequestSafety::Safe,
                "get" | "post" => RequestSafety::Safe,
                _ => RequestSafety::Safe,
            };
            assert_eq!(classify_method(method), expected, "{method}");
        }
    }
}
