//! Plan 342: the whole-block rule for the seven canonical outproxy options.
//!
//! # Why this is a block and not seven options
//!
//! Plan 342 records the sequencing constraint that decides this module's
//! shape: *"the ordering is forced — store plumbing first, then all seven
//! fields, then the request path, then the wire lane. Nothing in the option
//! surface is accepted until the route behind it exists."*
//!
//! The corollary is that the seven fields must not be independently
//! acceptable. Accepting `ProxyList` alone produces a tunnel that parses,
//! validates, persists, and reports success while nothing ever opens a route
//! — an operator reasonably reading `get` output as egress capability. That
//! is the inert-acceptance trap, the same failure Plan 344 found in the
//! LeaseSet mode table and the same one Plan 326 forbids with "no mode may
//! pass from parser acceptance or inert storage".
//!
//! So the rule is exactly two admissible shapes:
//!
//! - **none** of the seven is present, or
//! - **all seven** are present and the block resolves.
//!
//! Anything in between is refused by name, listing what is missing, before
//! any listener or destination is allocated.
//!
//! # Why the block owns the credential and the policy separately
//!
//! [`ParsedOutproxyBlock`] splits what the seven options mean into two
//! things with very different lifetimes:
//!
//! - `config` is the **route policy** — which outproxies, in which order,
//!   speaking which dialect. It is `Clone`, `Debug`, and `Eq`, rides on the
//!   `ServiceTunnelSpec`, and is safe to project into a status surface.
//! - `credential` is the **plaintext password**, which exists only inside
//!   [`parse`]'s return and is consumed immediately by the seal step. It is
//!   `Zeroizing`, has no `Debug`, and no `Display` anywhere.
//!
//! Splitting them is what lets the definition file carry ciphertext while
//! the spec carries policy, with no value that is both `Debug` and
//! sensitive.
//!
//! # The parse is shared by two callers with different password contents
//!
//! `normalize_definition` parses the block from a **fresh request**, where
//! `outproxy_password` is plaintext, and seals it. `build_control_spec`
//! parses the block from the **stored definition**, where the same key holds
//! the Plan 341 sealed form. The parser therefore never reads the password's
//! *content* — only whether it is present and non-empty, which is true in
//! both cases. The only reader of the plaintext is the seal step, and it
//! lives in `normalize_definition`, not here.

use std::collections::BTreeMap;

use zeroize::Zeroizing;

use i2pr_service_tunnels::outproxy::{
    OutproxyConfig, OutproxyError, OutproxyList, OutproxyPolicy, OutproxyType,
};

use crate::i2pcontrol_tunnels::ControlError;

/// The seven Proposal 170 outproxy option slots, in canonical i2pr spelling.
///
/// Membership of this list is what admits a key to a definition
/// (`SUPPORTED_342_OPTIONS` mirrors it); the *block* rule below is what
/// decides whether an admitted set is usable. They are deliberately the same
/// seven — a key that could be stored but never block-validated would be
/// exactly the inert acceptance this module exists to prevent.
pub const OUTPROXY_BLOCK_KEYS: [&str; 7] = [
    "proxy_list",
    "use_outproxy_plugin",
    "outproxy_auth",
    "outproxy_username",
    "outproxy_password",
    "outproxy_type",
    "ssl_proxies",
];

/// The tunnel kinds that may declare an outproxy provider.
///
/// `httpclient`, `socks`, `connectclient`, `socksirc` — the four kinds with
/// a *clearnet request target* an outproxy can carry. `client` and
/// `ircclient` are deliberately excluded: `client` is a raw byte tunnel to
/// one I2P destination and has no authority to route, and the IRC client
/// path has no outproxy-aware handshake. Accepting the block on a kind with
/// no request path would store a policy nothing reads, which is the inert
/// acceptance this module refuses.
pub fn kind_accepts_outproxy_block(kind: &str) -> bool {
    matches!(kind, "httpclient" | "socks" | "connectclient" | "socksirc")
}

/// One resolved outproxy block: the route policy plus the plaintext password
/// that has not yet been sealed.
pub struct ParsedOutproxyBlock {
    /// The validated route policy. No credential.
    pub config: OutproxyConfig,
    /// The plaintext password, present only when a credential is configured.
    ///
    /// `Zeroizing` and with no `Debug`/`Display`/`Clone`. The caller must
    /// seal it into the definition's stored form and then drop it; there is
    /// no path that keeps it.
    pub credential: Option<Zeroizing<String>>,
}

/// A `Debug` that shows the policy and the *presence* of a credential, never
/// its value.
///
/// Hand-written rather than derived: the derived form would require a `Debug`
/// on the credential, which is exactly the bound this type exists to avoid.
/// The username is printed because it is an identifier, not a secret — but
/// only after the same length bound that keeps it from being a covert
/// channel for arbitrary bytes.
impl core::fmt::Debug for ParsedOutproxyBlock {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ParsedOutproxyBlock")
            .field("config", &self.config)
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// The block, resolved once from one options map.
///
/// `kind` is the Proposal 170 type spelling (`"httpclient"`, …) so the
/// applicability rule and the failure messages name the same vocabulary the
/// control client sent.
pub fn parse_outproxy_block(
    definition_name: &str,
    kind: &str,
    options: &BTreeMap<String, String>,
) -> Result<Option<ParsedOutproxyBlock>, ControlError> {
    let present: Vec<&str> = OUTPROXY_BLOCK_KEYS
        .iter()
        .copied()
        .filter(|key| options.contains_key(*key))
        .collect();
    if present.is_empty() {
        return Ok(None);
    }
    // The all-or-none rule. Both halves of the message matter: "incomplete"
    // alone leaves an operator guessing which half to add.
    if present.len() != OUTPROXY_BLOCK_KEYS.len() {
        let missing: Vec<&str> = OUTPROXY_BLOCK_KEYS
            .iter()
            .copied()
            .filter(|key| !options.contains_key(*key))
            .collect();
        return Err(ControlError::OutproxyBlockRejected(format!(
            "{definition_name}: the outproxy fields are all-or-none; {} present, missing {}",
            present.join(", "),
            missing.join(", "),
        )));
    }
    if !kind_accepts_outproxy_block(kind) {
        return Err(ControlError::OutproxyBlockRejected(format!(
            "{definition_name}: the outproxy fields apply only to httpclient, socks, \
             connectclient, and socksirc; type {kind} has no clearnet request target an \
             outproxy could carry"
        )));
    }

    // Plan 342 invariant 3: `UseOutproxyPlugin` is the declaration that the
    // tunnel uses a configured provider. i2pr loads no plugins, ever, so the
    // flag can only mean "use the static configured provider path".
    //
    // `false` is refused rather than ignored. Ignoring it would store a
    // ProxyList that no request path consults — the inert acceptance trap in
    // its most literal form — and a control client that set both fields and
    // got an accepted response would reasonably conclude it has egress.
    let use_plugin = parse_bool("use_outproxy_plugin", options)?;
    if !use_plugin {
        return Err(ControlError::OutproxyBlockRejected(format!(
            "{definition_name}: UseOutproxyPlugin:false declares no provider, so a ProxyList \
             would be stored and never routed; omit the whole block to configure no outproxy"
        )));
    }

    let outproxy_type = value_of(definition_name, "outproxy_type", options)?;
    let kind_value =
        OutproxyType::parse(outproxy_type).map_err(|error| ControlError::InvalidOption {
            option: "OutproxyType".to_owned(),
            reason: static_outproxy_reason(error),
        })?;

    let list = OutproxyList::parse(value_of(definition_name, "proxy_list", options)?).map_err(
        |error| ControlError::InvalidOption {
            option: "ProxyList".to_owned(),
            reason: static_outproxy_reason(error),
        },
    )?;

    // `SSLProxies` is parsed with the same grammar as `ProxyList` and is
    // checked for subset membership by `OutproxyConfig::validate`, so an
    // empty value is legal and means "no tunnelled requests", which is the
    // fail-closed default in `permits_tunnelled`.
    let tunnelled = match options.get("ssl_proxies") {
        Some(value) if !value.trim().is_empty() => {
            OutproxyList::parse(value).map_err(|error| ControlError::InvalidOption {
                option: "SSLProxies".to_owned(),
                reason: static_outproxy_reason(error),
            })?
        }
        _ => OutproxyList::default(),
    };

    // The credential triple, with the same completeness rule Plan 292
    // applies to the local listener's `ProxyAuth`.
    let outproxy_auth = parse_bool("outproxy_auth", options)?;
    let username = options
        .get("outproxy_username")
        .map(|value| value.as_str())
        .unwrap_or("");
    let password = options
        .get("outproxy_password")
        .map(String::as_str)
        .unwrap_or("");
    let username_present = !username.is_empty();
    let password_present = !password.is_empty();
    if outproxy_auth != (username_present && password_present) {
        return Err(ControlError::OutproxyBlockRejected(format!(
            "{definition_name}: OutproxyAuth must match the complete \
             OutproxyUsername/OutproxyPassword pair"
        )));
    }
    if username_present
        && username.len() > i2pr_service_tunnels::outproxy::MAX_OUTPROXY_USERNAME_LEN
    {
        return Err(ControlError::InvalidOption {
            option: "OutproxyUsername".to_owned(),
            reason: "outproxy username exceeds the bounded length",
        });
    }

    let config = OutproxyConfig {
        list,
        kind: kind_value,
        present_credential: outproxy_auth,
        username: if username_present {
            Some(username.to_owned())
        } else {
            None
        },
        tunnelled,
        policy: OutproxyPolicy::default(),
    };
    config
        .validate()
        .map_err(|error| ControlError::InvalidOption {
            option: outproxy_option_for(error).to_owned(),
            reason: static_outproxy_reason(error),
        })?;

    Ok(Some(ParsedOutproxyBlock {
        config,
        // `Zeroizing` here so the plaintext's lifetime is bounded even if
        // the caller takes an early return before sealing it. The value is
        // never formatted: `ControlError` messages are constructed from
        // static strings and key names only.
        credential: password_present.then(|| Zeroizing::new(password.to_owned())),
    }))
}

/// The stored-form marker for an outproxy password, written into the
/// definition in place of the plaintext.
///
/// Not a prefix but a key-name convention: the daemon's persistence mask
/// keys off `SECRET_OPTIONS`, and this function is the only writer.
pub const OUTPROXY_PASSWORD_KEY: &str = "outproxy_password";

/// Reads one required block value.
fn value_of<'a>(
    definition_name: &str,
    key: &'a str,
    options: &'a BTreeMap<String, String>,
) -> Result<&'a str, ControlError> {
    options.get(key).map(String::as_str).ok_or_else(|| {
        ControlError::OutproxyBlockRejected(format!(
            "{definition_name}: the outproxy block is missing {key}"
        ))
    })
}

/// Parses one of the block's boolean fields.
///
/// A value that is not `true`/`false` is refused rather than defaulted: a
/// misspelled `UseOutproxyPlugin` must not silently read as the safe value.
fn parse_bool(key: &str, options: &BTreeMap<String, String>) -> Result<bool, ControlError> {
    match options.get(key).map(String::as_str) {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(ControlError::InvalidOption {
            option: key.to_owned(),
            reason: "outproxy boolean field must be exactly \"true\" or \"false\"",
        }),
    }
}

/// Which canonical field a config-level cross-field failure belongs to.
fn outproxy_option_for(error: OutproxyError) -> &'static str {
    match error {
        OutproxyError::TunnelledNotInList => "SSLProxies",
        OutproxyError::MalformedCredential { .. } => "OutproxyAuth",
        OutproxyError::MalformedList { .. } | OutproxyError::DuplicateEndpoint => "ProxyList",
        _ => "ProxyList",
    }
}

/// A static, non-secret reason for an outproxy parse failure.
///
/// Every arm is a literal. No arm interpolates the offending value, and the
/// input is never a secret on any of these paths.
fn static_outproxy_reason(error: OutproxyError) -> &'static str {
    match error {
        OutproxyError::UnknownType => "outproxy type is outside the finite vocabulary",
        OutproxyError::MalformedType { .. } => "outproxy type is malformed or empty",
        OutproxyError::MalformedList { .. } => "outproxy list entry is malformed or empty",
        OutproxyError::DuplicateEndpoint => "outproxy list names the same destination twice",
        OutproxyError::NotAnI2pDestination => {
            "outproxy entries must be I2P destinations or .i2p names; a clearnet host, an IP \
             literal, and a host:port authority are all refused"
        }
        OutproxyError::TunnelledNotInList => "SSLProxies must be a subset of ProxyList",
        OutproxyError::MalformedCredential { .. } => {
            "OutproxyAuth requires a complete OutproxyUsername/OutproxyPassword pair"
        }
        OutproxyError::MalformedTarget { .. } => "outproxy target is malformed",
        OutproxyError::MalformedResponse { .. } => "outproxy response is malformed",
        OutproxyError::CredentialRejected => "outproxy rejected the presented credential",
        OutproxyError::UpstreamRefused => "outproxy refused the upstream request",
        OutproxyError::ExceedsCeiling { .. } => "outproxy value exceeds a bounded ceiling",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real Base32 destination, built with the service-tunnel crate's own
    /// encoder so the fixture cannot drift from the grammar the parser
    /// enforces. A hand-typed `.b32.i2p` label would be a test of my
    /// typing, not of the code.
    fn b32(byte: u8) -> String {
        format!(
            "{}.b32.i2p",
            i2pr_service_tunnels::encode_b32_label(&[byte; 32])
        )
    }

    /// A real static alias in the `.i2p` namespace, resolved through the same
    /// validator an operator's alias would pass.
    const ALIAS: &str = "second.example.i2p";

    fn block(pairs: &[(&str, String)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    /// The complete seven-field block, with the two destination placeholders
    /// resolved against real fixtures.
    fn complete_pairs() -> [(&'static str, String); 7] {
        [
            ("proxy_list", format!("{},{ALIAS}", b32(0x5a))),
            ("use_outproxy_plugin", "true".to_owned()),
            ("outproxy_auth", "false".to_owned()),
            ("outproxy_username", String::new()),
            ("outproxy_password", String::new()),
            ("outproxy_type", "http".to_owned()),
            ("ssl_proxies", ALIAS.to_owned()),
        ]
    }

    #[test]
    fn absent_block_is_no_block() {
        let options = block(&[("proxy_username", "u".to_owned())]);
        assert!(
            parse_outproxy_block("t", "httpclient", &options)
                .expect("no block")
                .is_none()
        );
    }

    #[test]
    fn partial_block_is_refused_with_the_missing_half() {
        let options = block(&[
            ("proxy_list", b32(0x11)),
            ("outproxy_type", "http".to_owned()),
        ]);
        let error = parse_outproxy_block("t", "httpclient", &options)
            .expect_err("partial block must be refused");
        match error {
            ControlError::OutproxyBlockRejected(reason) => {
                // The message must name what is missing, not just that
                // something is: an operator cannot act on "incomplete".
                assert!(reason.contains("missing"), "{reason}");
                assert!(reason.contains("outproxy_username"), "{reason}");
                assert!(reason.contains("ssl_proxies"), "{reason}");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn every_single_key_alone_is_refused() {
        for key in OUTPROXY_BLOCK_KEYS {
            let value = if key.contains("plugin") || key.contains("auth") {
                "true".to_owned()
            } else if key == "outproxy_type" {
                "http".to_owned()
            } else {
                b32(0x22)
            };
            assert!(
                parse_outproxy_block("t", "httpclient", &block(&[(key, value)])).is_err(),
                "{key} alone must be refused"
            );
        }
    }

    #[test]
    fn complete_block_resolves_without_a_credential() {
        let options = block(&complete_pairs());
        let parsed = parse_outproxy_block("t", "httpclient", &options)
            .expect("complete block")
            .expect("present");
        assert!(!parsed.config.present_credential);
        assert!(parsed.config.username.is_none());
        assert!(parsed.credential.is_none());
        assert_eq!(parsed.config.list.len(), 2);
        assert_eq!(parsed.config.tunnelled.len(), 1);
        assert_eq!(parsed.config.kind, OutproxyType::HttpConnect);
    }

    #[test]
    fn complete_block_with_a_credential_yields_a_zeroizing_password() {
        let mut pairs = complete_pairs();
        pairs[2].1 = "true".to_owned();
        pairs[3].1 = "operator".to_owned();
        pairs[4].1 = "s3cret!".to_owned();
        let parsed = parse_outproxy_block("t", "httpclient", &block(&pairs))
            .expect("complete block")
            .expect("present");
        assert!(parsed.config.present_credential);
        assert_eq!(parsed.config.username.as_deref(), Some("operator"));
        assert_eq!(
            parsed.credential.as_deref().map(String::as_str),
            Some("s3cret!")
        );
    }

    #[test]
    fn half_a_credential_is_refused() {
        for (auth, username, password) in [
            ("true", "operator", ""),
            ("false", "operator", "s3cret!"),
            ("true", "", "s3cret!"),
        ] {
            let mut pairs = complete_pairs();
            pairs[2].1 = auth.to_owned();
            pairs[3].1 = username.to_owned();
            pairs[4].1 = password.to_owned();
            assert!(
                parse_outproxy_block("t", "httpclient", &block(&pairs)).is_err(),
                "auth={auth} username={username:?} password={password:?} must be refused"
            );
        }
    }

    #[test]
    fn plugin_false_is_refused_rather_than_inertly_stored() {
        let mut pairs = complete_pairs();
        pairs[1].1 = "false".to_owned();
        let error = parse_outproxy_block("t", "httpclient", &block(&pairs))
            .expect_err("UseOutproxyPlugin:false must be refused");
        match error {
            ControlError::OutproxyBlockRejected(reason) => {
                assert!(reason.contains("no provider"), "{reason}");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn a_non_boolean_plugin_flag_is_refused_not_defaulted() {
        let mut pairs = complete_pairs();
        pairs[1].1 = "yes".to_owned();
        assert!(parse_outproxy_block("t", "httpclient", &block(&pairs)).is_err());
    }

    #[test]
    fn a_clearnet_outproxy_entry_is_refused() {
        for entry in ["proxy.example.com", "127.0.0.1", "proxy.example.com:8888"] {
            let mut pairs = complete_pairs();
            pairs[0].1 = entry.to_owned();
            let error = parse_outproxy_block("t", "httpclient", &block(&pairs))
                .expect_err("clearnet entry must be refused");
            match error {
                ControlError::InvalidOption { option, .. } => assert_eq!(option, "ProxyList"),
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }

    #[test]
    fn an_unknown_outproxy_type_is_refused() {
        let mut pairs = complete_pairs();
        pairs[5].1 = "/usr/bin/curl".to_owned();
        assert!(parse_outproxy_block("t", "httpclient", &block(&pairs)).is_err());
    }

    #[test]
    fn ssl_proxies_outside_the_proxy_list_is_refused() {
        let mut pairs = complete_pairs();
        pairs[6].1 = b32(0x33);
        let error = parse_outproxy_block("t", "httpclient", &block(&pairs))
            .expect_err("tunnelled outproxy outside the list must be refused");
        match error {
            ControlError::InvalidOption { option, .. } => assert_eq!(option, "SSLProxies"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn kinds_without_a_clearnet_request_target_refuse_the_block() {
        let options = block(&complete_pairs());
        for kind in ["client", "ircclient", "server", "httpclient", "socks"] {
            let parsed = parse_outproxy_block("t", kind, &options);
            assert_eq!(
                parsed.is_ok(),
                kind_accepts_outproxy_block(kind),
                "{kind} disagreeed with kind_accepts_outproxy_block"
            );
        }
    }

    #[test]
    fn the_block_key_list_and_the_applicability_mask_agree() {
        // One source of truth: if these ever drift, a key could be admitted
        // to a definition on a kind whose block rule refuses it, which is
        // the inert acceptance the module exists to prevent.
        assert_eq!(OUTPROXY_BLOCK_KEYS.len(), 7);
        for key in OUTPROXY_BLOCK_KEYS {
            let option = i2pr_i2pcontrol::tunnel_options::find_option(key)
                .expect("block key is a known option");
            assert!(option.applies_to(6), "{key} does not reach connectclient");
            assert!(option.applies_to(2), "{key} does not reach httpclient");
            assert!(option.applies_to(3), "{key} does not reach socks");
            assert!(option.applies_to(7), "{key} does not reach socksirc");
        }
    }
}
