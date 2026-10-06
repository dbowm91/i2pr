//! Plan 342 — whether a request target must be an I2P name.
//!
//! # Why this is a policy value and not a property of the grammar
//!
//! Plan 176 (HTTP) and Plan 290 (SOCKS) both made their request-target
//! grammars `.i2p`-only, and that is right for what those plans were
//! building. But "is a clearnet name a *syntactically* acceptable
//! request-target?" turned out to be a **tunnel policy** question, not a
//! grammar question, and baking it into the grammar made Plan 342's whole
//! request-path integration unreachable.
//!
//! Concretely, before this module existed:
//!
//! - `parse_authority_form` and `parse_absolute_form` both called
//!   `validate_host`, which requires a `.i2p` / `.b32.i2p` suffix;
//! - `validate_domain_policy` (SOCKS5 and SOCKS4a) required `.i2p`;
//!
//! so a `CONNECT example.com:443` was answered `403` by the *parser*, and a
//! SOCKS5 `CONNECT example.com:80` was answered `HostUnreachable` from inside
//! the *negotiator*. Both returned before the request path reached
//! `classify_client_target`, so `ClientTargetClass::ViaOutproxy` and
//! `Refused` were unreachable in production while their unit rows — which
//! call the classifier directly — passed. The step-3 integration was
//! structurally correct and completely dead.
//!
//! The split this module draws:
//!
//! - **Grammar** (this crate, unchanged): lengths, delimiters, control bytes,
//!   userinfo, port range, zero port, empty authority, port-only authority.
//!   None of that depends on the tunnel's egress configuration.
//! - **Target policy** (this enum): whether a well-formed *non*-`.i2p` name
//!   is a parseable request at all. An outproxy-carrying tunnel answers
//!   "yes, and then the classifier decides"; everything else answers "no".
//!
//! # The default is the strict one
//!
//! Every pre-Plan-342 entry point keeps its exact previous behaviour: it is a
//! thin wrapper that passes [`TargetPolicy::I2pOnly`]. Existing rows are
//! therefore unchanged in meaning, and there is no way to reach the relaxed
//! policy by accident — a caller must name it.
//!
//! # This does not weaken Plan 176's or Plan 290's guarantees
//!
//! [`TargetPolicy::AllowsClearnet`] still refuses everything the strict
//! policy refused *except* the suffix requirement:
//!
//! - IP literals stay refused, so the tunnel can never become a numeric
//!   open relay that has no name to apply a `Host`-based policy to;
//! - `localhost` and `.localhost` stay refused, so a loopback name cannot be
//!   handed to an outproxy that would resolve it;
//! - userinfo stays refused (a credential in an authority would be
//!   indistinguishable from an outproxy credential);
//! - mixed-suffix confusion (`example.i2p.com`) is *not* this module's job —
//!   `outproxy::classify_client_target` refuses it through
//!   `OutproxyTarget::parse_authority`, which is the single authority on that
//!   decision.
//!
//! The one thing that genuinely changes is: a well-formed clearnet name now
//! parses, and it is the classifier — not the parser — that decides whether it
//! becomes a Direct route (never, for a clearnet name), an outproxy route, or
//! a refusal.

#![forbid(unsafe_code)]

/// Whether a request target must be an I2P name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TargetPolicy {
    /// The Plan 176 / Plan 290 default: only `.i2p` authorities parse. A
    /// well-formed clearnet name is `NonI2pAuthority` (HTTP) or
    /// `NonI2pTarget` (SOCKS), before any routing decision is made.
    I2pOnly,
    /// Plan 342: this tunnel carries outproxy requests, so a well-formed
    /// clearnet name parses and reaches `classify_client_target`, which
    /// decides Direct / ViaOutproxy / Refused.
    ///
    /// Choosing this says the tunnel has an egress route configured. It does
    /// not grant one: with no provider installed the classifier returns
    /// `Refused(NotConfigured)` and no socket is opened. That ordering is why
    /// this can be a parser input at all.
    AllowsClearnet,
}

impl TargetPolicy {
    /// Whether a well-formed non-`.i2p` name is a parseable request target.
    ///
    /// The single question every parser asks, so the HTTP and SOCKS grammars
    /// cannot disagree about it.
    pub const fn admits_clearnet(self) -> bool {
        matches!(self, Self::AllowsClearnet)
    }
}

impl Default for TargetPolicy {
    /// [`TargetPolicy::I2pOnly`].
    ///
    /// The strict default is the safe direction: a caller that forgets to
    /// thread a policy gets a parser that refuses clearnet names, which is a
    /// refusal, never a leak.
    fn default() -> Self {
        Self::I2pOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::target::parse_request_target;
    use crate::http::target::{
        parse_absolute_form_with_policy, parse_authority_form_with_policy,
        parse_request_target_with_policy,
    };
    use crate::socks5::Socks5Limits;
    use crate::socks5::config::SOCKS_VERSION;
    use crate::socks5::request::RequestParser;
    use crate::socks5::socks4a::{SOCKS4A_VERSION, Socks4aRequestParser};

    /// The row that would have caught Plan 342's unreachable request paths.
    ///
    /// Before this module, every one of these four grammars refused the clearnet
    /// authority, so no request path could ever hand a clearnet host to
    /// `classify_client_target`, and `ClientTargetClass::ViaOutproxy` was dead in
    /// production while its unit rows passed. Written as a *chain* on purpose:
    /// the individual parsers passing is not the property, the parser handing a
    /// usable host to the classifier is.
    #[test]
    fn plan342_clearnet_authority_reaches_the_classifier_in_every_grammar() {
        let open = TargetPolicy::AllowsClearnet;

        // HTTP absolute-form (the `httpclient` forward path).
        let absolute = parse_request_target_with_policy("http://example.com/a?b=1", open)
            .expect("an outproxy-carrying tunnel must parse a clearnet absolute-form target");
        assert_eq!(absolute.host, "example.com");
        assert_eq!(absolute.path, "/a");

        // HTTP authority-form (the `CONNECT` path).
        let authority = parse_authority_form_with_policy("example.com:8443", 512, open)
            .expect("an outproxy-carrying tunnel must parse a clearnet CONNECT authority");
        assert_eq!(authority.host, "example.com");
        assert_eq!(authority.port, Some(8443));

        // SOCKS5 CONNECT (the `socks` path).
        let socks5 = socks5_request("example.com", 443, open).expect("socks5 clearnet parses");
        assert_eq!(socks5.0, "example.com");
        assert_eq!(socks5.1, 443);

        // SOCKS4a CONNECT.
        let socks4a = socks4a_request("example.com", 443, open).expect("4a clearnet parses");
        assert_eq!(socks4a.0, "example.com");

        // And each host now classifies as an outproxy route rather than
        // stopping at the parser.
        for host in ["example.com", "a.b.example.org"] {
            let class =
                crate::outproxy::classify_client_target(Some(&configured_provider()), host, 443)
                    .expect("classify");
            assert!(
                matches!(class, crate::outproxy::ClientTargetClass::ViaOutproxy(_)),
                "{host} must classify as an outproxy route, got {class:?}"
            );
        }
    }

    /// The strict default is byte-for-byte unchanged.
    ///
    /// Every pre-Plan-342 entry point is a wrapper over `I2pOnly`, so an existing
    /// caller cannot have been widened. This row pins the wrapper half directly,
    /// because the guarantee is "you must name the relaxed policy", not "the
    /// relaxed policy is off somewhere less obvious".
    #[test]
    fn plan342_strict_default_refuses_the_same_authorities() {
        assert!(parse_request_target("http://example.com/").is_err());
        assert!(
            parse_absolute_form_with_policy("http://example.com/", TargetPolicy::I2pOnly).is_err()
        );
        assert!(
            parse_authority_form_with_policy("example.com:443", 512, TargetPolicy::I2pOnly)
                .is_err()
        );
        assert!(socks5_request("example.com", 443, TargetPolicy::I2pOnly).is_err());
        assert!(socks4a_request("example.com", 443, TargetPolicy::I2pOnly).is_err());
        // `.i2p` still parses under both, so the policy is not a blanket
        // loosening of the grammar.
        assert!(
            parse_request_target_with_policy("http://example.i2p/", TargetPolicy::I2pOnly).is_ok()
        );
        assert!(
            parse_request_target_with_policy("http://example.i2p/", TargetPolicy::AllowsClearnet)
                .is_ok()
        );
    }

    /// `Default` is the strict policy, and this row is what keeps it that way.
    ///
    /// Mutation-tested. Inverting `Default` to `AllowsClearnet` while leaving
    /// every wrapper intact passes every other row in this module, because they
    /// all name a policy explicitly — so the "a caller who forgets to thread a
    /// policy gets a parser that refuses" claim would have been a comment
    /// rather than a checked property. The claim is load-bearing (it is the
    /// reason a forgotten argument is a refusal and not a leak), so it is
    /// pinned here.
    #[test]
    fn plan342_default_policy_is_the_strict_one() {
        assert_eq!(TargetPolicy::default(), TargetPolicy::I2pOnly);
        assert!(!TargetPolicy::default().admits_clearnet());
        assert!(TargetPolicy::AllowsClearnet.admits_clearnet());
    }

    /// The relaxed policy relaxes *only* the suffix requirement.
    ///
    /// Everything refused for a structural reason stays refused, because an
    /// outproxy carrying it would be the thing Plan 342 exists to prevent: a
    /// numeric open relay, a loopback name handed to a foreign resolver, or a
    /// userinfo authority that would be indistinguishable from an outproxy
    /// credential.
    #[test]
    fn plan342_relaxed_policy_still_refuses_structurally_bad_authorities() {
        let open = TargetPolicy::AllowsClearnet;

        // IP literals: no name to apply a Host policy to.
        for bad in ["http://192.0.2.1/", "http://[2001:db8::1]/"] {
            assert!(
                parse_request_target_with_policy(bad, open).is_err(),
                "IP literal must stay refused: {bad}"
            );
        }
        assert!(parse_authority_form_with_policy("192.0.2.1:443", 512, open).is_err());
        assert!(socks5_request("192.0.2.1", 443, open).is_err());
        assert!(socks4a_request("192.0.2.1", 443, open).is_err());

        // localhost: a foreign resolver must not be asked for loopback.
        for bad in ["http://localhost/", "http://x.localhost/"] {
            assert!(
                parse_request_target_with_policy(bad, open).is_err(),
                "localhost must stay refused: {bad}"
            );
        }
        assert!(socks5_request("localhost", 80, open).is_err());
        assert!(socks4a_request("localhost", 80, open).is_err());

        // Userinfo: would be indistinguishable from an outproxy credential.
        assert!(parse_request_target_with_policy("http://user:pw@example.com/", open).is_err());
        assert!(parse_authority_form_with_policy("user:pw@example.com:443", 512, open).is_err());

        // Grammar bounds are policy-independent.
        assert!(parse_authority_form_with_policy("example.com:0", 512, open).is_err());
        assert!(parse_authority_form_with_policy(":443", 512, open).is_err());
        assert!(parse_authority_form_with_policy("443", 512, open).is_err());
        assert!(parse_authority_form_with_policy("example.com:443", 4, open).is_err());
        assert!(socks5_request("example.com", 0, open).is_err());
        assert!(socks4a_request("example.com", 0, open).is_err());

        // The scheme, form, and path grammar is untouched by the policy.
        assert!(parse_request_target_with_policy("https://example.com/", open).is_err());
        assert!(parse_request_target_with_policy("/relative", open).is_err());
    }

    /// Mixed-suffix confusion is refused — by the classifier, which is the
    /// single authority on that decision.
    ///
    /// Under the relaxed policy `example.i2p.com` is a syntactically fine
    /// clearnet label, so the parser accepts it and `classify_client_target`
    /// refuses it. That split is deliberate: the parser decides syntax, the
    /// classifier decides routes, and neither guesses at the other's job.
    #[test]
    fn plan342_mixed_suffix_is_refused_by_the_classifier_not_the_parser() {
        let parsed = parse_authority_form_with_policy(
            "example.i2p.com:443",
            512,
            TargetPolicy::AllowsClearnet,
        )
        .expect("syntactically a valid clearnet label");
        assert_eq!(parsed.host, "example.i2p.com");
        let refused =
            crate::outproxy::classify_client_target(None, &parsed.host, parsed.port.unwrap_or(443));
        assert!(
            refused.is_err(),
            "mixed suffix must be refused by the classifier, got {refused:?}"
        );
    }

    /// One valid configured provider, for the classification half of the rows.
    fn configured_provider() -> crate::outproxy::OutproxyConfig {
        let list = crate::outproxy::OutproxyList::parse("first.example.i2p, second.example.i2p")
            .expect("outproxy list");
        crate::outproxy::OutproxyConfig {
            tunnelled: list.clone(),
            list,
            kind: crate::outproxy::OutproxyType::HttpConnect,
            present_credential: false,
            username: None,
            policy: crate::outproxy::OutproxyPolicy::default(),
        }
    }

    /// Drives one SOCKS5 CONNECT request through the runtime-neutral parser.
    fn socks5_request(
        host: &str,
        port: u16,
        policy: TargetPolicy,
    ) -> Result<(String, u16), String> {
        let mut request = vec![
            SOCKS_VERSION,
            0x01,
            0x00,
            0x03,
            u8::try_from(host.len()).map_err(|_| "host too long".to_owned())?,
        ];
        request.extend_from_slice(host.as_bytes());
        request.extend_from_slice(&port.to_be_bytes());
        let mut parser = RequestParser::with_policy(policy);
        match parser
            .advance(&request, Socks5Limits::defaults())
            .map_err(|e| e.reason.to_owned())?
        {
            Some(crate::socks5::request::RequestOutcome::ReadyToConnect {
                destination, ..
            }) => Ok((destination.host, destination.port)),
            Some(crate::socks5::request::RequestOutcome::Rejected { .. }) => Err("rejected".into()),
            None => Err("incomplete".into()),
        }
    }

    /// Drives one SOCKS4a CONNECT request through the runtime-neutral parser.
    fn socks4a_request(
        host: &str,
        port: u16,
        policy: TargetPolicy,
    ) -> Result<(String, u16), String> {
        // SOCKS4a: version, CONNECT, port, the 4a extension marker
        // (0.0.0.1 — a plain 0.0.0.0 is a SOCKS4 IPv4 literal and is refused),
        // NUL-terminated userid, NUL-terminated domain.
        let mut request = vec![SOCKS4A_VERSION, crate::socks5::socks4a::SOCKS4A_CMD_CONNECT];
        request.extend_from_slice(&port.to_be_bytes());
        request.extend_from_slice(&[0, 0, 0, 1]);
        request.extend_from_slice(b"i2pr-test\x00");
        request.extend_from_slice(host.as_bytes());
        request.push(0x00);
        let mut parser = Socks4aRequestParser::with_policy(policy);
        match parser
            .advance(&request, Socks5Limits::defaults())
            .map_err(|e| e.to_string())?
        {
            Some(crate::socks5::socks4a::Socks4aOutcome::ReadyToConnect {
                destination, ..
            }) => Ok((destination.host, destination.port)),
            Some(crate::socks5::socks4a::Socks4aOutcome::Rejected) => Err("rejected".into()),
            None => Err("incomplete".into()),
        }
    }
}
