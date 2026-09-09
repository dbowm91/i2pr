//! Plan 176 header rewrite policy.
//!
//! Implements the conservative first-profile hop-by-hop + privacy
//! rewrite. The output is the sequence of header lines the proxy
//! forwards to the remote service.
//!
//! ## Rules (Plan 176 §6)
//!
//! 1. parse the `Connection` field value as a bounded list of
//!    case-insensitive field names;
//! 2. remove every header named by `Connection`;
//! 3. remove/replace the `Connection` header itself with
//!    `Connection: close` (forced for the first profile);
//! 4. remove known hop-by-hop headers when not already named by
//!    `Connection`: `Proxy-Connection`, `Keep-Alive`, `TE`,
//!    `Trailer`, `Upgrade`, `Transfer-Encoding` (we never forward
//!    dechunked bodies in the first profile);
//! 6. normalize the `Host` authority to the validated destination
//!    authority (canonical host\[:port\]);
//! 7. force `Connection: close` (mandatory for the first profile);
//! 8. apply the privacy policy (User-Agent / Referer / From /
//!    `Via` / `Forwarded` / `X-Forwarded-*` / `Proxy-Authorization`).
//!
//! The output sequence is byte-exact, lower-cased names preserved
//! per RFC 9110 §5.1 (case-insensitive but we always emit lower
//! case), and never contains CRLF injection (verified by the
//! caller through the input validation in the parser).

#![forbid(unsafe_code)]

use super::config::{DEFAULT_USER_AGENT_VALUE, PrivacyPolicy, UserAgentPolicy};
use super::parser::HeaderEntry;
use super::target::RequestTarget;

/// Returns the validated authority host for the supplied target,
/// rejecting hosts that fail the strict `.i2p` grammar.
pub fn validate_authority_host(target: &RequestTarget) -> Result<(), &'static str> {
    super::target::validate_host(&target.host).map_err(|_| "host is not a valid .i2p authority")
}

/// Rewrites the supplied header list against the validated target
/// and privacy policy. Returns the new header list in canonical
/// (lower-case) order. Input headers are not mutated; the output is
/// a fresh allocation.
pub fn rewrite_headers(
    headers: &[HeaderEntry],
    target: &RequestTarget,
    privacy: &PrivacyPolicy,
) -> Vec<HeaderEntry> {
    // Step 1: collect the `Connection` value list.
    let mut connection_list: Vec<String> = Vec::new();
    for entry in headers {
        if entry.name.as_str() == "connection" {
            for token in entry.value.split(',') {
                let trimmed = token.trim();
                if !trimmed.is_empty() {
                    connection_list.push(trimmed.to_ascii_lowercase());
                }
            }
        }
    }

    let mut output: Vec<HeaderEntry> = Vec::with_capacity(headers.len());
    let mut host_inserted = false;
    for entry in headers {
        let name = entry.name.as_str();
        let mut drop = false;
        // Always drop Connection itself; it is replaced.
        if name == "connection" {
            drop = true;
        }
        // Drop anything named by the Connection list (case-insensitive).
        if connection_list.iter().any(|n| n == name) {
            drop = true;
        }
        // Drop known hop-by-hop fields unconditionally.
        if matches!(
            name,
            "proxy-connection" | "keep-alive" | "te" | "trailer" | "upgrade" | "transfer-encoding"
        ) {
            drop = true;
        }
        // Privacy rewrite.
        match name {
            "via"
            | "forwarded"
            | "x-forwarded-for"
            | "x-forwarded-host"
            | "x-forwarded-proto"
            | "proxy-authorization" => {
                drop = true;
            }
            "referer" if privacy.strip_referer => {
                drop = true;
            }
            "from" if privacy.strip_from => {
                drop = true;
            }
            "user-agent" => match privacy.user_agent {
                UserAgentPolicy::Keep => {}
                UserAgentPolicy::Strip => drop = true,
                UserAgentPolicy::ReplaceStable => {}
            },
            _ => {}
        }
        if drop {
            continue;
        }
        // Always replace `Host` with the normalized target authority.
        if name == "host" {
            if !host_inserted {
                output.push(HeaderEntry {
                    name: super::parser::HeaderName("host".to_owned()),
                    value: super::target::canonical_authority(target),
                });
                host_inserted = true;
            }
            continue;
        }
        output.push(entry.clone());
    }
    if !host_inserted {
        output.push(HeaderEntry {
            name: super::parser::HeaderName("host".to_owned()),
            value: super::target::canonical_authority(target),
        });
    }
    // Step 7: force Connection: close.
    output.push(HeaderEntry {
        name: super::parser::HeaderName("connection".to_owned()),
        value: "close".to_owned(),
    });
    // Privacy rewrite step 8: handle User-Agent here so we can do
    // it last (or first), regardless of input ordering.
    match privacy.user_agent {
        UserAgentPolicy::Keep => {}
        UserAgentPolicy::Strip => {
            output.retain(|entry| entry.name.as_str() != "user-agent");
        }
        UserAgentPolicy::ReplaceStable => {
            let mut replaced = false;
            for entry in output.iter_mut() {
                if entry.name.as_str() == "user-agent" {
                    entry.value = DEFAULT_USER_AGENT_VALUE.to_owned();
                    replaced = true;
                }
            }
            if !replaced {
                output.push(HeaderEntry {
                    name: super::parser::HeaderName("user-agent".to_owned()),
                    value: DEFAULT_USER_AGENT_VALUE.to_owned(),
                });
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::parser::{HeaderEntry, HeaderName};
    use crate::http::target::{RequestTarget, TargetKind};

    fn entry(name: &str, value: &str) -> HeaderEntry {
        HeaderEntry {
            name: HeaderName(name.to_ascii_lowercase()),
            value: value.to_owned(),
        }
    }

    fn target(host: &str, port: Option<u16>) -> RequestTarget {
        RequestTarget {
            kind: TargetKind::Absolute,
            host: host.to_owned(),
            port,
            path: "/".to_owned(),
            query: String::new(),
            scheme: Some("http"),
        }
    }

    #[test]
    fn connection_nominated_field_removed() {
        let headers = vec![
            entry("Host", "example.i2p"),
            entry("Connection", "X-Custom, Keep-Alive"),
            entry("X-Custom", "stale"),
        ];
        let rewritten = rewrite_headers(
            &headers,
            &target("example.i2p", None),
            &PrivacyPolicy::default(),
        );
        let names: Vec<&str> = rewritten.iter().map(|h| h.name_str()).collect();
        assert!(!names.contains(&"x-custom"));
        assert!(!names.contains(&"keep-alive"));
        assert!(names.contains(&"connection"));
    }

    #[test]
    fn hop_by_hop_headers_stripped() {
        let headers = vec![
            entry("Host", "example.i2p"),
            entry("Proxy-Connection", "close"),
            entry("Keep-Alive", "timeout=5"),
            entry("TE", "trailers"),
            entry("Trailer", "X-Foo"),
            entry("Upgrade", "h2c"),
            entry("Transfer-Encoding", "chunked"),
        ];
        let rewritten = rewrite_headers(
            &headers,
            &target("example.i2p", None),
            &PrivacyPolicy::default(),
        );
        let names: Vec<&str> = rewritten.iter().map(|h| h.name_str()).collect();
        for forbidden in [
            "proxy-connection",
            "keep-alive",
            "te",
            "trailer",
            "upgrade",
            "transfer-encoding",
        ] {
            assert!(!names.contains(&forbidden), "{forbidden} must be stripped");
        }
        assert!(names.contains(&"connection"));
    }

    #[test]
    fn host_is_normalized() {
        let headers = vec![entry("Host", "OTHER.i2p:80")];
        let rewritten = rewrite_headers(
            &headers,
            &target("example.i2p", Some(80)),
            &PrivacyPolicy::default(),
        );
        let host = rewritten
            .iter()
            .find(|h| h.name_str() == "host")
            .expect("host present");
        assert_eq!(host.value, "example.i2p");
    }

    #[test]
    fn force_connection_close() {
        let headers = vec![entry("Host", "example.i2p")];
        let rewritten = rewrite_headers(
            &headers,
            &target("example.i2p", None),
            &PrivacyPolicy::default(),
        );
        let connection = rewritten
            .iter()
            .find(|h| h.name_str() == "connection")
            .expect("connection present");
        assert_eq!(connection.value, "close");
    }

    #[test]
    fn privacy_headers_stripped() {
        let headers = vec![
            entry("Host", "example.i2p"),
            entry("Via", "1.1 proxy"),
            entry("Forwarded", "for=1.2.3.4"),
            entry("X-Forwarded-For", "1.2.3.4"),
            entry("X-Forwarded-Host", "example.com"),
            entry("X-Forwarded-Proto", "https"),
            entry("Proxy-Authorization", "Basic Zm9vOmJhcg=="),
            entry("Referer", "http://example.com/"),
            entry("From", "user@example.com"),
        ];
        let rewritten = rewrite_headers(
            &headers,
            &target("example.i2p", None),
            &PrivacyPolicy::default(),
        );
        let names: Vec<&str> = rewritten.iter().map(|h| h.name_str()).collect();
        for forbidden in [
            "via",
            "forwarded",
            "x-forwarded-for",
            "x-forwarded-host",
            "x-forwarded-proto",
            "proxy-authorization",
            "referer",
            "from",
        ] {
            assert!(!names.contains(&forbidden), "{forbidden} must be stripped");
        }
    }

    #[test]
    fn user_agent_replace_stable() {
        let headers = vec![
            entry("Host", "example.i2p"),
            entry("User-Agent", "Mozilla/5.0"),
        ];
        let rewritten = rewrite_headers(
            &headers,
            &target("example.i2p", None),
            &PrivacyPolicy::default(),
        );
        let ua = rewritten
            .iter()
            .find(|h| h.name_str() == "user-agent")
            .expect("user-agent present");
        assert_eq!(ua.value, DEFAULT_USER_AGENT_VALUE);
    }

    #[test]
    fn user_agent_strip() {
        let policy = PrivacyPolicy {
            user_agent: UserAgentPolicy::Strip,
            ..PrivacyPolicy::default()
        };
        let headers = vec![
            entry("Host", "example.i2p"),
            entry("User-Agent", "Mozilla/5.0"),
        ];
        let rewritten = rewrite_headers(&headers, &target("example.i2p", None), &policy);
        let names: Vec<&str> = rewritten.iter().map(|h| h.name_str()).collect();
        assert!(!names.contains(&"user-agent"));
    }

    #[test]
    fn user_agent_keep() {
        let policy = PrivacyPolicy {
            user_agent: UserAgentPolicy::Keep,
            ..PrivacyPolicy::default()
        };
        let headers = vec![
            entry("Host", "example.i2p"),
            entry("User-Agent", "Mozilla/5.0"),
        ];
        let rewritten = rewrite_headers(&headers, &target("example.i2p", None), &policy);
        let ua = rewritten
            .iter()
            .find(|h| h.name_str() == "user-agent")
            .expect("user-agent present");
        assert_eq!(ua.value, "Mozilla/5.0");
    }

    #[test]
    fn duplicate_host_uniqueness_left_to_parser() {
        // The parser is responsible for rejecting duplicate
        // conflicting Host headers. Rewrite here preserves the
        // first host occurrence when multiple equal entries are
        // present, otherwise replaces with the canonical target.
        let headers = vec![entry("Host", "example.i2p"), entry("Host", "example.i2p")];
        let rewritten = rewrite_headers(
            &headers,
            &target("example.i2p", None),
            &PrivacyPolicy::default(),
        );
        let count = rewritten.iter().filter(|h| h.name_str() == "host").count();
        assert_eq!(count, 1);
    }
}
