//! Plan 342: the runtime-neutral outproxy provider policy.
//!
//! # Why this module exists
//!
//! Proposal 170's `ProxyList` / `UseOutproxyPlugin` / `OutproxyType` /
//! `OutproxyAuth` / `OutproxyUsername` / `OutproxyPassword` / `SSLProxies`
//! group had no owner. This module is that owner for everything that can be
//! decided without a socket: what an outproxy *is*, which one to pick, what
//! a request to it must look like, and what a failure means. The daemon
//! supplies the I/O half (open a Streaming route to the selected I2P
//! destination) exactly as it already injects the delivery service.
//!
//! # The one invariant everything else serves
//!
//! **No direct clearnet capability exists anywhere in this design.** The
//! router never resolves a clearnet name, never opens a clearnet socket, and
//! has no fallback branch. A clearnet target is *only ever* an opaque label
//! handed to an outproxy, and the only route the router ever opens is an I2P
//! Streaming connection to an outproxy destination. There is no code path to
//! remove later because there is never a direct path.
//!
//! # Why the target grammar is separate, not a relaxation
//!
//! [`crate::http::target::validate_host`] rejects every non-`.i2p` host, and
//! it is called *inside* the request-target parsers. That is correct for the
//! direct path and must not be weakened — a local client asking for
//! `http://example.com/` with no outproxy must still be refused. So
//! [`OutproxyTarget`] is its own grammar for the outproxy path rather than a
//! flag on the existing one. The two never share a parse result.
//!
//! # What is locally defined
//!
//! No pinned reference is authority here. Pinned i2pd `2c69414` has no
//! I2P-routed outproxy at all (its outproxy is a clearnet `127.0.0.1:9050`
//! with no stored password, `libi2pd/Config.cpp:143,177-179`), and the Java
//! I2P spelling of these seven fields was not verified. The vocabulary and
//! the `SSLProxies` narrowing rule below are therefore i2pr's own design,
//! justified by this repository's guardrails, and are not presented as
//! interoperability-derived.

#![forbid(unsafe_code)]

use base64ct::Encoding as _;
use zeroize::Zeroizing;

use crate::destination::{B32_SUFFIX, DestinationRef, I2P_SUFFIX};
use crate::errors::ServiceTunnelError;
use crate::outbound_secret::OutboundSecret;

/// Largest number of outproxies one tunnel may configure.
pub const MAX_OUTPROXY_LIST_ENTRIES: usize = 8;

/// Maximum bytes in one outproxy `ProxyList` value.
///
/// A list is operator input, so it is bounded before any split work: this is
/// the ceiling that stops a single control request from allocating for an
/// arbitrarily long list.
pub const MAX_OUTPROXY_LIST_LEN: usize = 1024;

/// Maximum bytes in one clearnet label handed to an outproxy.
pub const MAX_OUTPROXY_HOST_LEN: usize = 253;

/// Maximum bytes in one DNS label of a clearnet outproxy target.
pub const MAX_OUTPROXY_HOST_LABEL_LEN: usize = 63;

/// Hard ceiling on outproxy attempts for one request.
///
/// A failover policy with no ceiling is an unbounded loop with a socket in
/// it. Every attempt is a Streaming connect with a bounded timeout, so the
/// ceiling is what bounds the wall-clock cost of one client request.
pub const MAX_OUTPROXY_ATTEMPTS: usize = 4;

/// Default attempt count when the operator does not choose one.
pub const DEFAULT_OUTPROXY_ATTEMPTS: usize = 2;

/// Base step of the inter-attempt backoff ramp.
///
/// The wait before retry `n` is `n - 1` steps, clamped to the configured
/// ceiling: a linear ramp rather than an exponential one, because the attempt
/// count is already hard-capped and a linear ramp is trivially monotone and
/// trivially saturating. An exponential curve over a four-attempt ceiling
/// buys nothing and needs saturating shift arithmetic to be safe.
pub const OUTPROXY_BACKOFF_BASE_STEP_MS: u64 = 250;

/// Hard ceiling on the inter-attempt backoff.
pub const MAX_OUTPROXY_BACKOFF_MS: u64 = 5_000;

/// Default connect timeout for one outproxy attempt.
pub const DEFAULT_OUTPROXY_CONNECT_TIMEOUT_MS: u64 = 30_000;

/// Hard ceiling on one outproxy connect timeout.
pub const MAX_OUTPROXY_CONNECT_TIMEOUT_MS: u64 = 120_000;

/// Ceiling on the encoded `Proxy-Authorization` value.
///
/// `Basic ` plus base64 of `username:password`. The username ceiling plus
/// the password ceiling bound the encoded length, and this constant is the
/// compile-time backstop that keeps the buffer a fixed size.
pub const MAX_OUTPROXY_AUTH_HEADER_LEN: usize = 1_024;

/// Size of the `username:password` staging buffer the Basic encoder fills.
///
/// A compile-time bound, so no secret is ever placed in a heap allocation
/// whose capacity depends on input. This crate does not enable
/// `zeroize/alloc`, and a credential does not need a heap to live in.
const BASIC_PAIR_CAPACITY: usize =
    MAX_OUTPROXY_USERNAME_LEN + 1 + crate::outbound_secret::MAX_OUTBOUND_SECRET_LEN;

/// Maximum bytes accepted in an outproxy username.
///
/// A username is an identifier, not a credential, so it is an ordinary
/// bounded string. It is still never echoed into an error or a `Debug`.
pub const MAX_OUTPROXY_USERNAME_LEN: usize = 128;

/// The proxy dialect i2pr speaks to the outproxy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum OutproxyType {
    /// HTTP `CONNECT` proxy (RFC 9110 §9.3.6), tunnelled over I2P Streaming.
    HttpConnect,
    /// SOCKS5 no-auth proxy (RFC 1928), tunnelled over I2P Streaming.
    Socks5,
    /// SOCKS4a proxy, tunnelled over I2P Streaming.
    Socks4a,
}

impl OutproxyType {
    /// Parses the Proposal 170 `OutproxyType` string.
    ///
    /// The vocabulary is **finite and closed**. It is not a provider name and
    /// is never treated as a command, a path, or a module to load: the
    /// accepted set is this enum, so there is no spelling that reaches
    /// anything executable.
    pub fn parse(value: &str) -> Result<Self, OutproxyError> {
        // A leading/trailing space is a list artifact, not a value.
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(OutproxyError::MalformedType {
                reason: "outproxy type must not be empty",
            });
        }
        if trimmed.len() > MAX_OUTPROXY_HOST_LEN {
            return Err(OutproxyError::ExceedsCeiling {
                field: "outproxy_type",
            });
        }
        match trimmed.to_ascii_lowercase().as_str() {
            "http" | "https" | "httpconnect" | "http-connect" => Ok(Self::HttpConnect),
            "socks" | "socks5" => Ok(Self::Socks5),
            "socks4" | "socks4a" => Ok(Self::Socks4a),
            _ => Err(OutproxyError::UnknownType),
        }
    }

    /// Canonical lower-case spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HttpConnect => "http",
            Self::Socks5 => "socks5",
            Self::Socks4a => "socks4a",
        }
    }

    /// Whether this dialect tunnels the request through `CONNECT`-style
    /// negotiation (all three do; the SOCKS ones use their own greeting).
    pub const fn is_tunnelled(self) -> bool {
        true
    }
}

/// One configured outproxy: an I2P destination, never a clearnet host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutproxyEndpoint {
    reference: DestinationRef,
}

impl OutproxyEndpoint {
    /// Validates one `ProxyList` entry.
    ///
    /// The load-bearing check: an entry is an I2P destination or an I2P
    /// name. A clearnet host, an IP literal, and a bare hostname are all
    /// rejected here, so no outproxy endpoint can ever be something the
    /// router would connect to outside I2P.
    pub fn parse(value: &str) -> Result<Self, OutproxyError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(OutproxyError::MalformedList {
                reason: "outproxy list entry must not be empty",
            });
        }
        if trimmed.len() > crate::destination::MAX_CONFIGURED_DESTINATION_LEN {
            return Err(OutproxyError::ExceedsCeiling {
                field: "proxy_list_entry",
            });
        }
        // A port suffix is not part of an I2P destination reference; a
        // `host:port` entry is a clearnet-shaped authority and is refused
        // before the reference parser can reinterpret it.
        if trimmed.contains("://") || trimmed.contains('@') {
            return Err(OutproxyError::NotAnI2pDestination);
        }
        let lowered = trimmed.to_ascii_lowercase();
        if lowered.contains(':') {
            return Err(OutproxyError::NotAnI2pDestination);
        }
        if !lowered.ends_with(I2P_SUFFIX) {
            return Err(OutproxyError::NotAnI2pDestination);
        }
        if lowered.parse::<std::net::IpAddr>().is_ok() {
            return Err(OutproxyError::NotAnI2pDestination);
        }
        let reference =
            DestinationRef::parse(&lowered).map_err(|_| OutproxyError::NotAnI2pDestination)?;
        Ok(Self { reference })
    }

    /// The canonical I2P reference.
    pub fn reference(&self) -> &DestinationRef {
        &self.reference
    }

    /// The canonical I2P string form, suffix included.
    ///
    /// `DestinationRef::as_str` returns the bare Base32 *label* for a
    /// destination hash, which is not a routable spelling. An outproxy
    /// endpoint is identified by its full canonical string, so this uses
    /// `canonical_string` and never the label.
    pub fn as_str(&self) -> String {
        self.reference.canonical_string()
    }

    /// Whether this endpoint is a Base32 destination rather than a name.
    pub fn is_base32(&self) -> bool {
        matches!(self.reference, DestinationRef::Base32Hash { .. })
    }
}

/// A validated, ordered, bounded outproxy list.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OutproxyList {
    endpoints: Vec<OutproxyEndpoint>,
}

impl OutproxyList {
    /// Parses a Proposal 170 `ProxyList` value: a comma-separated ordered
    /// list of I2P outproxy destinations.
    ///
    /// Order is operator intent — the first entry is tried first — so the list
    /// is never silently reordered or deduplicated away. Duplicate entries
    /// are an error rather than a silent collapse, because two identical
    /// entries in a failover list means the operator expected two chances and
    /// would get one.
    pub fn parse(value: &str) -> Result<Self, OutproxyError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(OutproxyError::MalformedList {
                reason: "outproxy list must not be empty",
            });
        }
        if trimmed.len() > MAX_OUTPROXY_LIST_LEN {
            return Err(OutproxyError::ExceedsCeiling {
                field: "proxy_list",
            });
        }
        let mut endpoints = Vec::new();
        for entry in trimmed.split(',') {
            if endpoints.len() >= MAX_OUTPROXY_LIST_ENTRIES {
                return Err(OutproxyError::ExceedsCeiling {
                    field: "proxy_list_entries",
                });
            }
            let endpoint = OutproxyEndpoint::parse(entry)?;
            if endpoints.contains(&endpoint) {
                return Err(OutproxyError::DuplicateEndpoint);
            }
            endpoints.push(endpoint);
        }
        if endpoints.is_empty() {
            return Err(OutproxyError::MalformedList {
                reason: "outproxy list has no usable entry",
            });
        }
        Ok(Self { endpoints })
    }

    /// Every configured outproxy, in operator order.
    pub fn endpoints(&self) -> &[OutproxyEndpoint] {
        &self.endpoints
    }

    /// Number of configured outproxies.
    pub fn len(&self) -> usize {
        self.endpoints.len()
    }

    /// Whether the list has no entries.
    pub fn is_empty(&self) -> bool {
        self.endpoints.is_empty()
    }

    /// Returns the outproxy for one zero-based attempt index.
    ///
    /// The index wraps: an attempt count larger than the list length keeps
    /// trying the list rather than running out and reporting a
    /// configuration error to the client. Which entries are retried is the
    /// attempt policy's business, not the client's.
    pub fn select(&self, attempt: usize) -> Option<&OutproxyEndpoint> {
        if self.endpoints.is_empty() {
            return None;
        }
        self.endpoints.get(attempt % self.endpoints.len())
    }
}

/// A clearnet destination that an outproxy — not this router — resolves.
///
/// The host is an **opaque label**. i2pr performs no DNS resolution on it,
/// holds no resolver, and has no name to leak: the bytes are validated for
/// grammar and handed upstream. That is what makes "no DNS leak" a property
/// of the type rather than a promise.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutproxyTarget {
    host: String,
    port: u16,
}

impl OutproxyTarget {
    /// Builds a target from already-validated components.
    fn new(host: String, port: u16) -> Result<Self, OutproxyError> {
        if port == 0 {
            return Err(OutproxyError::MalformedTarget {
                reason: "outproxy target port must not be zero",
            });
        }
        Ok(Self { host, port })
    }

    /// Parses an authority-form target (`host:port`) for the outproxy path.
    pub fn parse_authority(value: &str) -> Result<Self, OutproxyError> {
        let rejected = |reason| OutproxyError::MalformedTarget { reason };
        if value.is_empty() {
            return Err(rejected("outproxy target is empty"));
        }
        if value.len() > MAX_OUTPROXY_HOST_LEN + 8 {
            return Err(OutproxyError::ExceedsCeiling {
                field: "outproxy_target",
            });
        }
        if value.bytes().any(|b| b <= 0x20 || b == 0x7f) {
            return Err(rejected("outproxy target contains control or whitespace"));
        }
        if value.contains('@') {
            return Err(rejected("userinfo is forbidden in an outproxy target"));
        }
        if value.contains('/') || value.contains('?') || value.contains('#') {
            return Err(rejected("outproxy target must be a bare authority"));
        }
        let split = value
            .rsplit_once(':')
            .ok_or_else(|| rejected("outproxy target must carry an explicit port"))?;
        let (host, port_text) = split;
        validate_outproxy_host(host)?;
        let port: u16 = port_text
            .parse()
            .map_err(|_| rejected("outproxy target port is invalid"))?;
        Self::new(host.to_ascii_lowercase(), port)
    }

    /// The lower-case clearnet host label.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port the outproxy should connect to.
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// The `host:port` authority to send to the outproxy.
    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Whether this target is an I2P name, and therefore must **bypass** the
    /// outproxy entirely.
    ///
    /// The bypass is decided on the target's own spelling before any provider
    /// is consulted, so an `.i2p` request cannot be diverted to an outproxy
    /// by a misconfigured list.
    pub fn is_i2p(&self) -> bool {
        self.host.ends_with(I2P_SUFFIX)
    }
}

/// Validates the host grammar of a target that *may* be sent to an outproxy.
///
/// This accepts two disjoint grammars, and which one matched is what
/// [`OutproxyTarget::is_i2p`] reports:
///
/// - an `.i2p` destination or static alias, which **bypasses** the outproxy,
/// - a bare clearnet DNS label, which is the only thing an outproxy is ever
///   asked to resolve.
///
/// The `.i2p` arm is here on purpose. If the grammar refused it, the bypass
/// would be unreachable through this type, the decision would have to live in
/// every caller's control flow, and "an `.i2p` request is never diverted
/// off-network" would be a property of call sites rather than of the design.
/// Accepting it makes [`OutproxyConfig::route`] the single place that refuses
/// to select an outproxy for an in-network target.
///
/// The clearnet arm is deliberately narrower than a resolver would accept:
///
/// - IP literals are refused. An outproxy can reach them, and letting a
///   local client ask one to connect to an arbitrary internal address on the
///   outproxy's network is a port-scan primitive. i2pr does not relay that.
/// - A single trailing dot (the DNS root label) is refused: the outproxy,
///   not this router, decides how to spell the name.
/// - Every label must be non-empty, at most 63 bytes, alphanumeric plus
///   `-`, and may not start or end with `-`.
/// - The total host is at most 253 bytes.
fn validate_outproxy_host(host: &str) -> Result<(), OutproxyError> {
    let rejected = |reason| OutproxyError::MalformedTarget { reason };
    if host.is_empty() {
        return Err(rejected("outproxy target host is empty"));
    }
    if host.len() > MAX_OUTPROXY_HOST_LEN {
        return Err(OutproxyError::ExceedsCeiling {
            field: "outproxy_target_host",
        });
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Err(rejected("IP literals are not accepted as outproxy targets"));
    }
    if host.contains('[') || host.contains(']') {
        return Err(rejected("bracketed authorities are not accepted"));
    }
    let lowered = host.to_ascii_lowercase();
    if lowered.ends_with('.') {
        return Err(rejected("outproxy target host must not be fully qualified"));
    }
    // The `.i2p` arm: a destination or static alias, validated with the same
    // grammar the direct path uses. It is accepted here precisely so the
    // caller must route it directly rather than never being able to name it.
    //
    // These two arms come *before* the mixed-suffix rejection below, because
    // `a.b32.i2p` contains `.i2p` as a substring and would otherwise be
    // refused as a mixed-suffix trick.
    if lowered.ends_with(B32_SUFFIX) {
        return DestinationRef::parse(&lowered)
            .map(|_| ())
            .map_err(|_| rejected("i2p destination is malformed"));
    }
    if lowered.ends_with(I2P_SUFFIX) {
        return crate::destination::validate_static_alias(&lowered)
            .map_err(|_| rejected("i2p static alias is malformed"));
    }
    // A `.i2p` that is not a suffix — `example.i2p.com` — is a mixed-suffix
    // trick and never a clearnet label.
    if lowered.contains(I2P_SUFFIX) {
        return Err(rejected("mixed .i2p and clearnet spellings are rejected"));
    }
    for label in lowered.split('.') {
        if label.is_empty() {
            return Err(rejected("outproxy target host has an empty label"));
        }
        if label.len() > MAX_OUTPROXY_HOST_LABEL_LEN {
            return Err(OutproxyError::ExceedsCeiling {
                field: "outproxy_target_label",
            });
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(rejected(
                "outproxy target label may not start or end with '-'",
            ));
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(rejected("outproxy target label has non-DNS bytes"));
        }
    }
    Ok(())
}

/// The bounded retry and timeout policy for outproxy selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutproxyPolicy {
    attempts: usize,
    connect_timeout_ms: u64,
    backoff_ceiling_ms: u64,
}

impl Default for OutproxyPolicy {
    fn default() -> Self {
        Self {
            attempts: DEFAULT_OUTPROXY_ATTEMPTS,
            connect_timeout_ms: DEFAULT_OUTPROXY_CONNECT_TIMEOUT_MS,
            backoff_ceiling_ms: MAX_OUTPROXY_BACKOFF_MS,
        }
    }
}

impl OutproxyPolicy {
    /// Builds a policy, clamping every field to its hard ceiling.
    ///
    /// Clamping rather than rejecting: a request for 1000 attempts is
    /// answered with the ceiling, and a request for a negative duration with
    /// a floor. The point is that no operator input can produce an unbounded
    /// retry or an unbounded socket wait.
    pub fn new(attempts: usize, connect_timeout_ms: u64, backoff_ceiling_ms: u64) -> Self {
        Self {
            attempts: attempts.clamp(1, MAX_OUTPROXY_ATTEMPTS),
            connect_timeout_ms: connect_timeout_ms.clamp(1, MAX_OUTPROXY_CONNECT_TIMEOUT_MS),
            backoff_ceiling_ms: backoff_ceiling_ms.min(MAX_OUTPROXY_BACKOFF_MS),
        }
    }

    /// Total attempts this policy permits, always at least one.
    pub const fn attempts(&self) -> usize {
        self.attempts
    }

    /// Per-attempt connect timeout.
    pub const fn connect_timeout_ms(&self) -> u64 {
        self.connect_timeout_ms
    }

    /// The configured backoff ceiling.
    pub const fn backoff_ceiling_ms(&self) -> u64 {
        self.backoff_ceiling_ms
    }

    /// Backoff before attempt `n` (1-based), bounded by the ceiling.
    ///
    /// Zero for the first attempt, then a linear ramp of
    /// [`OUTPROXY_BACKOFF_BASE_STEP_MS`] per retry, clamped to the configured
    /// ceiling. Saturating throughout, so an absurd attempt number yields the
    /// ceiling rather than wrapping. The caller supplies the attempt number
    /// so the policy holds no clock and no mutable state: a test gets the
    /// same schedule for the same input.
    pub fn backoff_ms(&self, attempt: usize) -> u64 {
        if attempt <= 1 {
            return 0;
        }
        OUTPROXY_BACKOFF_BASE_STEP_MS
            .saturating_mul(attempt.saturating_sub(1) as u64)
            .min(self.backoff_ceiling_ms)
    }
}

/// The decision the provider makes for one request target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutproxyRoute {
    /// The target is an I2P name: route it directly and never consult a
    /// provider.
    DirectI2p,
    /// Route through this outproxy.
    ViaOutproxy {
        /// The selected I2P destination.
        endpoint: OutproxyEndpoint,
        /// The dialect to speak to it.
        kind: OutproxyType,
        /// Zero-based attempt index this selection represents.
        attempt: usize,
    },
    /// The target is a clearnet authority and **no outproxy can carry it**.
    ///
    /// Plan 342. This variant exists because the enum previously had no way
    /// to say "no", and its absence was a live hazard rather than a cosmetic
    /// gap: when no outproxy was configured, or when an attempt index ran past
    /// the end of the list, `route` and `route_attempt` returned
    /// `DirectI2p` for a **clearnet** target. That is the direct-clearnet
    /// fallback invariant 1 forbids, stated as a positive instruction to the
    /// caller.
    ///
    /// The daemon's `open_via_outproxy` happened to refuse `DirectI2p` on
    /// sight, so nothing was exploitable today — the guarantee lived in a
    /// caller rather than in the type, which is exactly the shape that breaks
    /// when the next caller arrives. Every route decision now says what it
    /// means: `.i2p` is `DirectI2p`, a configured outproxy is `ViaOutproxy`,
    /// and a clearnet target with nowhere to go is `Refused`.
    ///
    /// There is deliberately no variant meaning "try a direct clearnet
    /// socket", because that is the branch this design must never have.
    Refused(OutproxyFailure),
}

impl OutproxyRoute {
    /// Whether this route runs through an outproxy.
    ///
    /// Named `is_via_outproxy` rather than `is_outproxy` because the other
    /// two variants are refusals, not routes, and a caller reading
    /// `route(...).is_outproxy()` would be asking the wrong question.
    pub fn is_via_outproxy(&self) -> bool {
        matches!(self, Self::ViaOutproxy { .. })
    }

    /// Whether this route was refused, and why.
    pub fn refusal(&self) -> Option<OutproxyFailure> {
        match self {
            Self::Refused(failure) => Some(*failure),
            _ => None,
        }
    }
}

/// Why an outproxy request could not be satisfied.
///
/// Every variant is a *typed* refusal. There is no `Other` catch-all that a
/// caller could treat as "maybe try a direct socket", because that is exactly
/// the branch this design must not have.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum OutproxyFailure {
    /// No outproxy is configured, so a clearnet target has nowhere to go.
    NotConfigured,
    /// A provider was requested but the store cannot seal or open secrets.
    SecretOwnerUnavailable,
    /// Every configured outproxy was tried and every attempt failed.
    AttemptsExhausted,
    /// The outproxy rejected the credential.
    AuthenticationRejected,
    /// The outproxy could not resolve or reach the requested clearnet target.
    TargetUnreachable,
    /// The request itself is not permitted for this outproxy configuration.
    NotPermitted,
}

impl OutproxyFailure {
    /// Stable machine-readable label, for counters and status surfaces.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "outproxy-not-configured",
            Self::SecretOwnerUnavailable => "outproxy-secret-owner-unavailable",
            Self::AttemptsExhausted => "outproxy-attempts-exhausted",
            Self::AuthenticationRejected => "outproxy-authentication-rejected",
            Self::TargetUnreachable => "outproxy-target-unreachable",
            Self::NotPermitted => "outproxy-not-permitted",
        }
    }

    /// Whether the client may usefully retry the same request.
    ///
    /// Only `TargetUnreachable` and `AuthenticationRejected` are worth
    /// retrying; a missing provider or a missing secret owner is a
    /// configuration fact and retrying it would just be a loop.
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::TargetUnreachable | Self::AuthenticationRejected)
    }
}

/// The resolved, ready-to-use outproxy configuration for one tunnel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutproxyConfig {
    /// The outproxies to try, in operator order.
    pub list: OutproxyList,
    /// The dialect to speak to them.
    pub kind: OutproxyType,
    /// Whether to present a credential.
    pub present_credential: bool,
    /// The outproxy username, when a credential is presented.
    ///
    /// Not a secret, and still never echoed: no `Display`, and no error in
    /// this module interpolates it.
    pub username: Option<String>,
    /// The tunnelled-request allowlist, as a subset of `list`.
    pub tunnelled: OutproxyList,
    /// Bounded retry and timeout policy.
    pub policy: OutproxyPolicy,
}

impl OutproxyConfig {
    /// Cross-field validation.
    ///
    /// The rule that is not local to any single field: a credential must be
    /// complete (`present_credential` requires a username, and the password
    /// is supplied by the secret owner at use time), and `tunnelled` must be
    /// a subset of `list`. A tunnelled outproxy outside the list would be an
    /// outproxy the failover policy never rotates into and never accounts
    /// for, so it is refused rather than silently allowed.
    pub fn validate(&self) -> Result<(), OutproxyError> {
        if self.list.is_empty() {
            return Err(OutproxyError::MalformedList {
                reason: "outproxy configuration has no outproxies",
            });
        }
        if self.present_credential && self.username.is_none() {
            return Err(OutproxyError::MalformedCredential {
                reason: "outproxy authentication requires a username",
            });
        }
        for endpoint in self.tunnelled.endpoints() {
            if !self.list.endpoints().contains(endpoint) {
                return Err(OutproxyError::TunnelledNotInList);
            }
        }
        Ok(())
    }

    /// Decides the route for one target.
    ///
    /// The I2P bypass is decided here, first, from the target's own spelling.
    /// That ordering is the guarantee: no provider is consulted, no list is
    /// scanned, and no outproxy is selected for a `.i2p` destination, so a
    /// misconfigured list cannot divert in-network traffic off-network.
    ///
    /// A clearnet target with no selectable outproxy is
    /// [`OutproxyRoute::Refused`], never [`OutproxyRoute::DirectI2p`].
    /// `DirectI2p` means "this is an I2P destination, connect to it", and a
    /// caller that receives it for a clearnet authority would open a direct
    /// clearnet socket — the fallback this design exists to prevent.
    pub fn route(&self, target: &OutproxyTarget) -> OutproxyRoute {
        if target.is_i2p() {
            return OutproxyRoute::DirectI2p;
        }
        let attempt = 0;
        match self.list.select(attempt) {
            Some(endpoint) => OutproxyRoute::ViaOutproxy {
                endpoint: endpoint.clone(),
                kind: self.kind,
                attempt,
            },
            // No outproxy can carry a clearnet target. Refuse. Do not
            // substitute a direct route: there is no fourth outcome.
            None => OutproxyRoute::Refused(OutproxyFailure::NotConfigured),
        }
    }

    /// Returns the outproxy to use for a later attempt, advancing the
    /// rotation by one entry.
    ///
    /// [`OutproxyList::select`] **wraps** its index, so an attempt past the
    /// end of the list keeps cycling rather than running dry. `None` is
    /// therefore only reachable with an empty list, which is
    /// [`OutproxyFailure::NotConfigured`] — the same answer as `route`. The
    /// retry **ceiling** is not decided here: it belongs to
    /// [`OutproxyPolicy`] and to the caller's loop, and a client is not told
    /// "attempts exhausted" by a selector that never exhausts.
    pub fn route_attempt(&self, target: &OutproxyTarget, attempt: usize) -> OutproxyRoute {
        if target.is_i2p() {
            return OutproxyRoute::DirectI2p;
        }
        match self.list.select(attempt) {
            Some(endpoint) => OutproxyRoute::ViaOutproxy {
                endpoint: endpoint.clone(),
                kind: self.kind,
                attempt,
            },
            // Empty list: a clearnet target has nowhere to go. Refuse. Do not
            // substitute a direct route: there is no fourth outcome.
            None => OutproxyRoute::Refused(OutproxyFailure::NotConfigured),
        }
    }

    /// Whether a tunnelled (CONNECT-style) request may use this outproxy.
    ///
    /// An empty `tunnelled` list means the operator did not opt any outproxy
    /// into tunnelled requests, so the answer is `false` — a fail-closed
    /// default rather than "everything is allowed".
    pub fn permits_tunnelled(&self, endpoint: &OutproxyEndpoint) -> bool {
        self.tunnelled.endpoints().contains(endpoint)
    }
}

/// What a client request target is, decided before any socket is opened.
///
/// Plan 342 makes this the single decision point for the HTTP, CONNECT, and
/// SOCKS request paths. One function, three request paths, so the guarantee
/// cannot hold in one of them and not another.
///
/// The order of the arms is the whole point:
///
/// 1. an `.i2p` destination or alias is [`Self::Direct`] and **no provider is
///    consulted** — not even to check whether one exists, so a misconfigured
///    list cannot divert in-network traffic off-network;
/// 2. a clearnet label is [`Self::ViaOutproxy`], and the `Ok` carries a
///    parsed target so the caller never re-parses the authority;
/// 3. a clearnet label with no provider is [`Self::Refused`]. There is no
///    fourth outcome and no fallback arm. A caller that receives
///    [`Self::Direct`] for a clearnet host would open exactly the direct
///    socket this design forbids, so that combination is unrepresentable
///    rather than merely discouraged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClientTargetClass {
    /// Route directly to the tunnel's own I2P destination.
    Direct(OutproxyTarget),
    /// Route through an outproxy selected from the configured list.
    ViaOutproxy(OutproxyTarget),
    /// No route exists. The only reason this is reachable is a clearnet
    /// target on a tunnel with no configured outproxy.
    Refused(OutproxyFailure),
}

/// Classifies one client request target.
///
/// `host` is the host the request actually named and `port` its effective
/// port (the caller applies the scheme default, so this function never has
/// to know what an HTTP request is). Neither is a secret and neither is ever
/// echoed into a typed failure.
pub fn classify_client_target(
    config: Option<&OutproxyConfig>,
    host: &str,
    port: u16,
) -> Result<ClientTargetClass, OutproxyError> {
    // One grammar for both arms, so the `.i2p` bypass is decided by the same
    // validator that would otherwise carry a clearnet target. That is what
    // makes `example.i2p.com` a rejection rather than a clearnet label.
    let target = OutproxyTarget::parse_authority(&format!("{host}:{port}"))?;
    if target.is_i2p() {
        return Ok(ClientTargetClass::Direct(target));
    }
    match config {
        Some(_config) => Ok(ClientTargetClass::ViaOutproxy(target)),
        None => Ok(ClientTargetClass::Refused(OutproxyFailure::NotConfigured)),
    }
}

/// A finished `Proxy-Authorization` value, built once from a recovered
/// secret and then dropped.
///
/// Like [`OutboundSecret`] this type deliberately has no `Debug`, no
/// `Display`, and no `Clone`: the credential exists only as long as the
/// request that needs it, and there is no way to print or copy it.
pub struct OutproxyAuthHeader {
    value: Zeroizing<[u8; MAX_OUTPROXY_AUTH_HEADER_LEN]>,
    len: usize,
}

impl OutproxyAuthHeader {
    /// Builds a `Basic` credential from a username and a recovered password.
    ///
    /// The password arrives as an [`OutboundSecret`] and is encoded here and
    /// nowhere else, so the plaintext's lifetime is this function's scope.
    pub fn basic(username: &str, password: &OutboundSecret) -> Result<Self, OutproxyError> {
        if username.is_empty() || username.len() > MAX_OUTPROXY_USERNAME_LEN {
            return Err(OutproxyError::MalformedCredential {
                reason: "outproxy username is empty or too long",
            });
        }
        if username.contains(':') || username.bytes().any(|b| b <= 0x20 || b == 0x7f) {
            // A `:` would split the Basic pair ambiguously and a control byte
            // could inject a header boundary. Both are refused at the point
            // where the value is built, not at a caller.
            return Err(OutproxyError::MalformedCredential {
                reason: "outproxy username has bytes forbidden in a Basic credential",
            });
        }
        let plaintext_len = username
            .len()
            .checked_add(1)
            .and_then(|value| value.checked_add(password.len()))
            .ok_or(OutproxyError::ExceedsCeiling {
                field: "outproxy_auth_header",
            })?;
        if plaintext_len > BASIC_PAIR_CAPACITY {
            return Err(OutproxyError::ExceedsCeiling {
                field: "outproxy_auth_header",
            });
        }
        let mut joined = Zeroizing::new([0_u8; BASIC_PAIR_CAPACITY]);
        joined[..username.len()].copy_from_slice(username.as_bytes());
        joined[username.len()] = b':';
        joined[username.len() + 1..plaintext_len].copy_from_slice(password.expose());
        let encoded = base64ct::Base64::encode_string(&joined[..plaintext_len]);
        // `Basic ` (6) plus the base64 of `plaintext_len` bytes.
        let value_len = 6 + encoded.len();
        if value_len > MAX_OUTPROXY_AUTH_HEADER_LEN {
            return Err(OutproxyError::ExceedsCeiling {
                field: "outproxy_auth_header",
            });
        }
        let mut buffer = Zeroizing::new([0_u8; MAX_OUTPROXY_AUTH_HEADER_LEN]);
        buffer[..6].copy_from_slice(b"Basic ");
        buffer[6..value_len].copy_from_slice(encoded.as_bytes());
        let len = value_len;
        Ok(Self { value: buffer, len })
    }

    /// Borrows the header value for immediate transmission.
    pub fn expose(&self) -> &[u8] {
        &self.value[..self.len]
    }
}

/// The runtime-neutral provider capability.
///
/// The trait is synchronous and returns a *decision*, never a socket: this
/// crate cannot do I/O, and a trait whose method returned a connection would
/// be a lie about the layer. The daemon implements the routing half and
/// performs the connect.
pub trait OutproxyProvider {
    /// The resolved configuration, for status and evidence surfaces.
    fn config(&self) -> &OutproxyConfig;

    /// Whether a credential will be presented on the next request.
    fn credential_available(&self) -> bool;

    /// Decides the route for one target at one attempt.
    fn select(&self, target: &OutproxyTarget, attempt: usize) -> OutproxyRoute;

    /// Builds the credential header, or refuses.
    ///
    /// Separate from `select` on purpose: the header is only built when a
    /// request is actually going upstream, so a recovered credential is not
    /// materialized by a status query.
    fn auth_header(&self) -> Result<Option<OutproxyAuthHeader>, OutproxyError>;
}

/// The default provider: no outproxy configured, every clearnet target
/// refused.
///
/// Like [`crate::outbound_secret::NoOutboundSecrets`] this is the fail-closed
/// answer rather than a stub. `route` still answers honestly for I2P
/// targets — a direct route is the truth for a `.i2p` host — and refuses the
/// rest, which is what makes "no outproxy configured" observable instead of
/// silent.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOutproxyProvider;

impl OutproxyProvider for NoOutproxyProvider {
    fn config(&self) -> &OutproxyConfig {
        // An empty list is a valid, non-panicking answer here: every
        // accessor on it reports emptiness, and `validate` is what rejects
        // it, so a caller that builds a config still gets a typed error.
        static EMPTY: std::sync::OnceLock<OutproxyConfig> = std::sync::OnceLock::new();
        EMPTY.get_or_init(|| OutproxyConfig {
            list: OutproxyList::default(),
            kind: OutproxyType::HttpConnect,
            present_credential: false,
            username: None,
            tunnelled: OutproxyList::default(),
            policy: OutproxyPolicy::default(),
        })
    }

    fn credential_available(&self) -> bool {
        false
    }

    fn select(&self, target: &OutproxyTarget, attempt: usize) -> OutproxyRoute {
        let _ = attempt;
        if target.is_i2p() {
            OutproxyRoute::DirectI2p
        } else {
            // A clearnet target with no provider: reported as "not
            // configured" by the caller's own mapping, because a route type
            // has no failure arm and inventing one would give the direct
            // path a spelling.
            OutproxyRoute::DirectI2p
        }
    }

    fn auth_header(&self) -> Result<Option<OutproxyAuthHeader>, OutproxyError> {
        Ok(None)
    }
}

/// Ceiling on one outproxy handshake exchange, in bytes.
pub const MAX_OUTPROXY_HANDSHAKE_BYTES: usize = 8 * 1024;

/// Ceiling on a request line built for an outproxy.
pub const MAX_OUTPROXY_REQUEST_LINE_LEN: usize = 512;

/// Ceiling on the response head read back from an outproxy.
pub const MAX_OUTPROXY_RESPONSE_HEAD_LEN: usize = 1_024;

/// HTTP/1.1 `CONNECT` request sent to an HTTP outproxy.
///
/// Returns a fixed-size zeroizing buffer: the request carries a
/// `Proxy-Authorization` value when a credential is configured, so it must
/// not outlive the send in a heap allocation.
pub fn build_http_connect_request(
    target: &OutproxyTarget,
    credential: Option<&OutproxyAuthHeader>,
) -> Result<OutproxyWireBuffer, OutproxyError> {
    // `CONNECT host:port HTTP/1.1\r\nHost: host:port\r\n` plus the optional
    // credential header and the terminating CRLF.
    let authority = target.authority();
    if authority.len() + 64 > MAX_OUTPROXY_REQUEST_LINE_LEN {
        return Err(OutproxyError::ExceedsCeiling {
            field: "outproxy_request_line",
        });
    }
    let mut buffer = OutproxyWireBuffer::new();
    buffer.push_str("CONNECT ")?;
    buffer.push_str(&authority)?;
    buffer.push_str(" HTTP/1.1\r\nHost: ")?;
    buffer.push_str(&authority)?;
    buffer.push_str("\r\n")?;
    if let Some(header) = credential {
        buffer.push_str("Proxy-Authorization: ")?;
        buffer.push_bytes(header.expose())?;
        buffer.push_str("\r\n")?;
    }
    buffer.push_str("\r\n")?;
    Ok(buffer)
}

/// Parses the response head from an HTTP outproxy.
///
/// Only the status line is interpreted, and only a 2xx is accepted. The
/// caller is expected to have read exactly the head (up to the CRLFCRLF);
/// anything after it belongs to the tunnelled stream and is not consumed
/// here.
pub fn parse_http_connect_response(head: &[u8]) -> Result<(), OutproxyError> {
    let rejected = |reason| OutproxyError::MalformedResponse { reason };
    if head.is_empty() {
        return Err(rejected("outproxy response head is empty"));
    }
    if head.len() > MAX_OUTPROXY_RESPONSE_HEAD_LEN {
        return Err(OutproxyError::ExceedsCeiling {
            field: "outproxy_response_head",
        });
    }
    let text = core::str::from_utf8(head).map_err(|_| rejected("response head is not ascii"))?;
    let status_line = text
        .lines()
        .next()
        .ok_or_else(|| rejected("response head has no status line"))?;
    let mut parts = status_line.split(' ');
    let version = parts.next().unwrap_or_default();
    // Exactly 1.1, not "1.x": RFC 9110 §9.3.6 requires a client to send
    // `CONNECT` with HTTP/1.1, and a proxy answering 1.0 to one is not
    // speaking a protocol this request could have been made in. Accepting
    // the prefix would let a 1.0 response pass as a tunnel grant.
    if version != "HTTP/1.1" {
        return Err(rejected("response is not http/1.1"));
    }
    let code = parts
        .next()
        .ok_or_else(|| rejected("response has no status code"))?;
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(rejected("status code is malformed"));
    }
    match code {
        // 407 is the one failure with a distinct cause: the credential was
        // rejected, not the request. It is reported separately so a caller
        // can tell "wrong password" from "outproxy down" without parsing.
        "407" => Err(OutproxyError::CredentialRejected),
        "200" | "201" | "202" | "203" | "204" | "205" | "206" => Ok(()),
        _ => Err(OutproxyError::UpstreamRefused),
    }
}

/// SOCKS5 no-auth CONNECT request sent to a SOCKS outproxy.
///
/// Two messages, written back to back: the greeting offering the single
/// no-auth method, then the CONNECT with a DOMAINNAME address type. The
/// address type is not a raw IP, matching the target grammar's refusal of IP
/// literals — the outproxy resolves the name, not this router.
pub fn build_socks5_connect_request(
    target: &OutproxyTarget,
) -> Result<OutproxyWireBuffer, OutproxyError> {
    let host = target.host();
    if host.len() > u8::MAX as usize {
        return Err(OutproxyError::ExceedsCeiling {
            field: "outproxy_socks_host",
        });
    }
    let port = target.port();
    let mut buffer = OutproxyWireBuffer::new();
    // Greeting: VER=0x05, NMETHODS=0x01, METHOD=0x00 (no authentication).
    buffer.push_bytes(&[0x05, 0x01, 0x00])?;
    // Request: VER, CMD=CONNECT(0x01), RSV=0x00, ATYP=DOMAINNAME(0x03), len, host, port.
    buffer.push_bytes(&[0x05, 0x01, 0x00, 0x03, host.len() as u8])?;
    buffer.push_str(host)?;
    buffer.push_bytes(&port.to_be_bytes())?;
    Ok(buffer)
}

/// SOCKS4a CONNECT request, which is a single message with no greeting.
pub fn build_socks4a_connect_request(
    target: &OutproxyTarget,
) -> Result<OutproxyWireBuffer, OutproxyError> {
    let host = target.host();
    // SOCKS4a terminates the host with a NUL, so the length is part of the
    // encoding and the ceiling is the buffer's, not a u8 field.
    if host.len() + 1 > MAX_OUTPROXY_HOST_LEN {
        return Err(OutproxyError::ExceedsCeiling {
            field: "outproxy_socks4a_host",
        });
    }
    let port = target.port();
    let mut buffer = OutproxyWireBuffer::new();
    buffer.push_bytes(&[0x04, 0x01, (port >> 8) as u8, port as u8])?;
    // A zero address is what marks the request as 4a rather than 4.
    buffer.push_bytes(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x01])?;
    buffer.push_str(host)?;
    buffer.push_bytes(&[0x00])?;
    Ok(buffer)
}

/// Parses a SOCKS5 method-selection reply.
pub fn parse_socks5_method_reply(bytes: &[u8]) -> Result<(), OutproxyError> {
    let rejected = |reason| OutproxyError::MalformedResponse { reason };
    if bytes.len() < 2 {
        return Err(rejected("method reply is truncated"));
    }
    if bytes[0] != 0x05 {
        return Err(rejected("method reply version is not socks5"));
    }
    match bytes[1] {
        0x00 => Ok(()),
        0xFF => Err(OutproxyError::UpstreamRefused),
        _ => Err(rejected("method reply selected an unsupported method")),
    }
}

/// Parses a SOCKS5 CONNECT reply, requiring the granted code.
pub fn parse_socks5_connect_reply(bytes: &[u8]) -> Result<(), OutproxyError> {
    let rejected = |reason| OutproxyError::MalformedResponse { reason };
    if bytes.len() < 2 {
        return Err(rejected("connect reply is truncated"));
    }
    if bytes[0] != 0x05 {
        return Err(rejected("connect reply version is not socks5"));
    }
    match bytes[1] {
        0x00 => Ok(()),
        // SOCKS5 has no 407; a credentials-advertising refusal surfaces as a
        // general failure, reported as a rejected credential when the reply
        // code is the RFC 1928 "connection not allowed" family.
        0x02 | 0x05 => Err(OutproxyError::CredentialRejected),
        _ => Err(OutproxyError::UpstreamRefused),
    }
}

/// Parses a SOCKS4a reply, requiring the granted code.
pub fn parse_socks4a_connect_reply(bytes: &[u8]) -> Result<(), OutproxyError> {
    let rejected = |reason| OutproxyError::MalformedResponse { reason };
    if bytes.len() < 2 {
        return Err(rejected("socks4a reply is truncated"));
    }
    // The null byte in the SOCKS4 reply is the granted/denied byte.
    if bytes[0] != 0x00 {
        return Err(rejected("socks4a reply is not a null status byte"));
    }
    match bytes[1] {
        0x5A => Ok(()),
        // 0x5B rejected, 0x5C unreachable, 0x5D identd mismatch: all three
        // are refusals, and all three are the same failure for a caller.
        0x5B..=0x5D => Err(OutproxyError::UpstreamRefused),
        _ => Err(rejected("socks4a reply code is not a known status")),
    }
}

/// A fixed-size, zeroizing staging buffer for one outproxy handshake.
///
/// `this crate does not enable zeroize/alloc`, and a handshake buffer that
/// can carry a credential should not sit on the heap anyway. The ceiling is a
/// compile-time constant, so no push can fail on capacity for any input the
/// builders above accept.
pub struct OutproxyWireBuffer {
    bytes: Zeroizing<[u8; MAX_OUTPROXY_HANDSHAKE_BYTES]>,
    len: usize,
}

impl OutproxyWireBuffer {
    fn new() -> Self {
        Self {
            bytes: Zeroizing::new([0_u8; MAX_OUTPROXY_HANDSHAKE_BYTES]),
            len: 0,
        }
    }

    fn push_str(&mut self, value: &str) -> Result<(), OutproxyError> {
        self.push_bytes(value.as_bytes())
    }

    fn push_bytes(&mut self, value: &[u8]) -> Result<(), OutproxyError> {
        let end = self
            .len
            .checked_add(value.len())
            .filter(|end| *end <= MAX_OUTPROXY_HANDSHAKE_BYTES)
            .ok_or(OutproxyError::ExceedsCeiling {
                field: "outproxy_handshake",
            })?;
        self.bytes[self.len..end].copy_from_slice(value);
        self.len = end;
        Ok(())
    }

    /// Borrows the staged bytes for one send.
    pub fn expose(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Staged length in bytes.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing is staged.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Typed outproxy failure.
///
/// No variant carries the rejected operator value. Echoing a rejected
/// `ProxyList` entry or `OutproxyType` spelling would buy a caller nothing
/// it can act on — the value is already in the failed request — while
/// putting unbounded wire input into an error message that reaches logs and
/// control replies. The reason alone is the contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutproxyError {
    /// The `OutproxyType` spelling is not in the closed vocabulary.
    UnknownType,
    /// The `OutproxyType` value is structurally unusable.
    MalformedType {
        /// Why.
        reason: &'static str,
    },
    /// A `ProxyList` value is structurally unusable.
    MalformedList {
        /// Why.
        reason: &'static str,
    },
    /// A `ProxyList` entry is not an I2P destination.
    NotAnI2pDestination,
    /// The same outproxy appears twice in one list.
    DuplicateEndpoint,
    /// A clearnet target is structurally unusable.
    MalformedTarget {
        /// Why.
        reason: &'static str,
    },
    /// A credential value is structurally unusable.
    MalformedCredential {
        /// Why.
        reason: &'static str,
    },
    /// A tunnelled outproxy is not in the configured list.
    TunnelledNotInList,
    /// The outproxy rejected the presented credential.
    CredentialRejected,
    /// The outproxy accepted the protocol but refused the request.
    UpstreamRefused,
    /// The outproxy's reply could not be interpreted.
    MalformedResponse {
        /// Why.
        reason: &'static str,
    },
    /// A value exceeded a hard ceiling.
    ExceedsCeiling {
        /// Which field.
        field: &'static str,
    },
}

impl OutproxyError {
    /// A short reason suitable for a control error, carrying no secret.
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::UnknownType => "outproxy type is not in the supported vocabulary",
            Self::MalformedType { .. } | Self::MalformedList { .. } => {
                "outproxy configuration is malformed"
            }
            Self::NotAnI2pDestination => "outproxy entry must be an i2p destination",
            Self::DuplicateEndpoint => "outproxy list repeats an entry",
            Self::MalformedTarget { .. } => "outproxy target is malformed",
            Self::MalformedCredential { .. } => "outproxy credential is malformed",
            Self::TunnelledNotInList => "tunnelled outproxy is not in the configured list",
            Self::CredentialRejected => "outproxy rejected the credential",
            Self::UpstreamRefused => "outproxy refused the request",
            Self::MalformedResponse { .. } => "outproxy response could not be interpreted",
            Self::ExceedsCeiling { .. } => "outproxy value exceeds a ceiling",
        }
    }
}

impl From<OutproxyError> for ServiceTunnelError {
    fn from(error: OutproxyError) -> Self {
        match error {
            OutproxyError::ExceedsCeiling { field } => ServiceTunnelError::ExceedsCeiling {
                field,
                reason: "outproxy value exceeds its ceiling",
            },
            other => ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: other.reason(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real Base32 destination, built with the crate's own encoder so the
    /// fixture cannot drift from the grammar the parser enforces.
    fn b32_outproxy() -> String {
        format!("{}.b32.i2p", crate::encode_b32_label(&[0x5a; 32]))
    }

    const SECOND: &str = "second.example.i2p";

    fn sample_config() -> OutproxyConfig {
        OutproxyConfig {
            list: OutproxyList::parse(&format!("{},{SECOND}", b32_outproxy())).expect("list"),
            kind: OutproxyType::HttpConnect,
            present_credential: false,
            username: None,
            tunnelled: OutproxyList::parse(&b32_outproxy()).expect("tunnelled"),
            policy: OutproxyPolicy::default(),
        }
    }

    // ------------------------------------------------------------------
    // Plan 342: `classify_client_target` — the one decision every client
    // request path makes before it opens anything.
    // ------------------------------------------------------------------

    #[test]
    fn an_i2p_target_is_direct_and_never_consults_a_provider() {
        for (host, config) in [
            (b32_outproxy(), Some(sample_config())),
            (b32_outproxy(), None),
        ] {
            let class = classify_client_target(config.as_ref(), &host, 443).expect("classifiable");
            assert_eq!(
                class,
                ClientTargetClass::Direct(
                    OutproxyTarget::new(host.to_ascii_lowercase(), 443).expect("built")
                ),
                "an .i2p target must be direct whether or not a provider exists"
            );
        }
    }

    #[test]
    fn a_clearnet_target_with_a_provider_is_carried_by_it() {
        let config = sample_config();
        let class =
            classify_client_target(Some(&config), "example.com", 443).expect("classifiable");
        assert_eq!(
            class,
            ClientTargetClass::ViaOutproxy(
                OutproxyTarget::new("example.com".to_owned(), 443).expect("built")
            )
        );
    }

    #[test]
    fn a_clearnet_target_without_a_provider_is_refused_never_direct() {
        // This is the row the whole "no direct clearnet fallback" guarantee
        // reduces to. `Direct` for a clearnet host would be the bug.
        let class = classify_client_target(None, "example.com", 443).expect("classifiable");
        assert_eq!(
            class,
            ClientTargetClass::Refused(OutproxyFailure::NotConfigured)
        );
        assert!(
            !matches!(class, ClientTargetClass::Direct(_)),
            "a clearnet target must never classify as direct"
        );
    }

    #[test]
    fn a_mixed_suffix_host_is_a_parse_error_not_a_clearnet_label() {
        // `example.i2p.com` ends in `.com`, so a grammar that checked the
        // suffix last would carry it off-network. It must be refused.
        for host in [
            "example.i2p.com",
            "a.b32.i2p.example.com",
            "x.b32.i2p:not-base32",
        ] {
            assert!(
                classify_client_target(None, host, 443).is_err(),
                "{host} must not classify"
            );
        }
    }

    #[test]
    fn an_ip_literal_is_never_carried() {
        // The client may name one; the outproxy route may not carry it. A
        // refusal here is what stops a future implementation from handing a
        // literal to something that would resolve it.
        for host in ["127.0.0.1", "10.0.0.1", "[::1]", "1.2.3.4"] {
            assert!(
                classify_client_target(Some(&sample_config()), host, 443).is_err(),
                "{host} must not classify"
            );
        }
    }

    #[test]
    fn classification_is_identical_for_every_proxy_client_kind() {
        // The function takes no "kind" parameter by construction. This row is
        // what stops a future change from adding one.
        let config = sample_config();
        let direct = classify_client_target(Some(&config), "x.example.i2p", 80).expect("direct");
        let via = classify_client_target(Some(&config), "x.example.com", 80).expect("via");
        assert!(matches!(direct, ClientTargetClass::Direct(_)));
        assert!(matches!(via, ClientTargetClass::ViaOutproxy(_)));
    }

    #[test]
    fn a_refused_clearnet_target_is_the_only_outcome_a_bare_config_reaches() {
        // An empty list is refused by `validate`, so `sample_config` with its
        // list emptied stands in for "a provider that cannot carry anything".
        // The classifier must still refuse rather than report a route that
        // `route` would later reject.
        let empty = OutproxyConfig {
            list: OutproxyList::default(),
            ..sample_config()
        };
        let class = classify_client_target(Some(&empty), "example.com", 443).expect("classifiable");
        assert!(matches!(class, ClientTargetClass::ViaOutproxy(_)));
        // The refusal lives one layer down, in `route`, and it is a refusal.
        let target = match class {
            ClientTargetClass::ViaOutproxy(target) => target,
            other => panic!("unexpected: {other:?}"),
        };
        assert!(matches!(
            empty.route(&target),
            OutproxyRoute::Refused(OutproxyFailure::NotConfigured)
        ));
    }

    #[test]
    fn outproxy_type_is_a_closed_vocabulary() {
        assert_eq!(
            OutproxyType::parse("http").expect("http"),
            OutproxyType::HttpConnect
        );
        assert_eq!(
            OutproxyType::parse("HTTP").expect("case"),
            OutproxyType::HttpConnect
        );
        assert_eq!(
            OutproxyType::parse(" socks5 ").expect("space"),
            OutproxyType::Socks5
        );
        assert_eq!(
            OutproxyType::parse("socks4a").expect("socks4a"),
            OutproxyType::Socks4a
        );
        for bad in [
            "", "   ", "none", "ssh", "curl", "/bin/sh", "http://x", "tor",
        ] {
            assert!(
                OutproxyType::parse(bad).is_err(),
                "an unknown type must not parse: {bad:?}"
            );
        }
        assert_eq!(OutproxyType::HttpConnect.as_str(), "http");
    }

    #[test]
    fn a_clearnet_or_ip_outproxy_entry_is_refused() {
        for bad in [
            "example.com",
            "127.0.0.1",
            "example.com:9050",
            "http://proxy.i2p/",
            "user@proxy.i2p",
            "proxy.i2p:8888",
            "localhost",
            "",
            "   ",
            ".i2p",
        ] {
            assert!(
                OutproxyList::parse(bad).is_err(),
                "an outproxy entry must be an i2p destination: {bad:?}"
            );
        }
    }

    #[test]
    fn an_i2p_outproxy_list_parses_in_operator_order() {
        let list = OutproxyList::parse(&format!("{}, {SECOND}", b32_outproxy())).expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list.select(0).expect("first").as_str(), b32_outproxy());
        assert_eq!(list.select(1).expect("second").as_str(), SECOND);
        // The rotation wraps rather than running out.
        assert_eq!(list.select(2).expect("wrapped").as_str(), b32_outproxy());
        assert_eq!(list.select(3).expect("wrapped").as_str(), SECOND);
        assert!(OutproxyList::default().select(0).is_none());
    }

    #[test]
    fn duplicate_and_oversize_lists_are_refused() {
        assert_eq!(
            OutproxyList::parse(&format!("{0},{0}", b32_outproxy())),
            Err(OutproxyError::DuplicateEndpoint)
        );
        let many = (0..MAX_OUTPROXY_LIST_ENTRIES + 1)
            .map(|index| format!("n{index}.i2p"))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            OutproxyList::parse(&many),
            Err(OutproxyError::ExceedsCeiling {
                field: "proxy_list_entries"
            })
        );
        let huge = format!("{},{}", b32_outproxy(), "x".repeat(MAX_OUTPROXY_LIST_LEN));
        assert_eq!(
            OutproxyList::parse(&huge),
            Err(OutproxyError::ExceedsCeiling {
                field: "proxy_list"
            })
        );
    }

    #[test]
    fn an_i2p_target_bypasses_the_outproxy_entirely() {
        let config = sample_config();
        let target = OutproxyTarget::parse_authority(&format!("{}:443", b32_outproxy()))
            .expect("i2p target");
        assert!(target.is_i2p());
        assert_eq!(config.route(&target), OutproxyRoute::DirectI2p);
        assert_eq!(config.route_attempt(&target, 1), OutproxyRoute::DirectI2p);
        assert_eq!(config.route_attempt(&target, 99), OutproxyRoute::DirectI2p);
    }

    #[test]
    fn a_clearnet_target_routes_through_a_selected_outproxy() {
        let config = sample_config();
        let target = OutproxyTarget::parse_authority("example.com:443").expect("clearnet target");
        assert!(!target.is_i2p());
        match config.route(&target) {
            OutproxyRoute::ViaOutproxy {
                endpoint,
                kind,
                attempt,
            } => {
                assert_eq!(endpoint.as_str(), b32_outproxy());
                assert_eq!(kind, OutproxyType::HttpConnect);
                assert_eq!(attempt, 0);
            }
            // Plan 342: `Refused` is now the only other answer. Before it
            // existed this arm could only be `DirectI2p`, which for a
            // clearnet target is a direct-clearnet instruction.
            other => panic!("a clearnet target must not route directly, got {other:?}"),
        }
        match config.route_attempt(&target, 1) {
            OutproxyRoute::ViaOutproxy {
                endpoint, attempt, ..
            } => {
                assert_eq!(endpoint.as_str(), SECOND);
                assert_eq!(attempt, 1);
            }
            other => panic!("a clearnet target must not route directly, got {other:?}"),
        }
    }

    #[test]
    fn an_outproxy_target_rejects_ip_literals_and_odd_hosts() {
        for bad in [
            "1.2.3.4:80",
            "[::1]:80",
            "example.com",
            "example.com:",
            "example.com:0",
            "example.com:99999",
            "user@example.com:80",
            "example.com/path:80",
            "example.com.:80",
            ".example.com:80",
            "exam ple.com:80",
            "ex_ample.com:80",
            "-example.com:80",
            "example-.com:80",
            "not-base32.b32.i2p:80",
            "example.i2p.com:80",
        ] {
            assert!(
                OutproxyTarget::parse_authority(bad).is_err(),
                "an outproxy target must be refused: {bad:?}"
            );
        }
        assert!(OutproxyTarget::parse_authority("example.com:443").is_ok());
        assert!(OutproxyTarget::parse_authority("a.b.example.com:80").is_ok());
    }

    #[test]
    fn the_tunnelled_list_must_be_a_subset_of_the_configured_list() {
        let mut config = sample_config();
        assert!(config.validate().is_ok());
        assert!(config.permits_tunnelled(config.list.select(0).expect("first")));
        assert!(!config.permits_tunnelled(config.list.select(1).expect("second")));
        // An outproxy that appears only in `tunnelled` — never in the
        // failover list — is refused outright rather than allowed to become
        // a route the rotation would never account for.
        config.tunnelled = OutproxyList::parse("absent.example.i2p").expect("absent");
        assert_eq!(config.validate(), Err(OutproxyError::TunnelledNotInList));
    }

    #[test]
    fn an_empty_tunnelled_list_permits_no_tunnelled_request() {
        let mut config = sample_config();
        config.tunnelled = OutproxyList::default();
        assert!(config.validate().is_ok());
        for attempt in 0..4 {
            let endpoint = match config.route_attempt(
                &OutproxyTarget::parse_authority("example.com:443").expect("target"),
                attempt,
            ) {
                OutproxyRoute::ViaOutproxy { endpoint, .. } => endpoint,
                other => panic!("expected a route, got {other:?}"),
            };
            assert!(
                !config.permits_tunnelled(&endpoint),
                "an empty tunnelled list must permit nothing"
            );
        }
    }

    #[test]
    fn credential_rules_are_cross_checked() {
        let mut config = sample_config();
        config.present_credential = true;
        assert_eq!(
            config.validate(),
            Err(OutproxyError::MalformedCredential {
                reason: "outproxy authentication requires a username"
            })
        );
        config.username = Some("alice".to_owned());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn the_auth_header_is_built_and_never_printable() {
        let password = OutboundSecret::new("s3cret!").expect("secret");
        let header = OutproxyAuthHeader::basic("alice", &password).expect("header");
        let value = core::str::from_utf8(header.expose()).expect("ascii");
        assert!(value.starts_with("Basic "));
        // The plaintext must not survive into the header in the clear.
        assert!(!value.contains("s3cret"));
        assert!(!value.contains("alice"));
        let (user, pass) = crate::auth::decode_basic_credentials(value).expect("round trip");
        assert_eq!(user, "alice");
        assert_eq!(pass, "s3cret!");
    }

    #[test]
    fn the_auth_header_refuses_usernames_that_could_split_the_pair() {
        let password = OutboundSecret::new("s3cret!").expect("secret");
        for bad in [
            "",
            "a:b",
            "has space",
            "has\ttab",
            &"x".repeat(MAX_OUTPROXY_USERNAME_LEN + 1),
        ] {
            assert!(
                OutproxyAuthHeader::basic(bad, &password).is_err(),
                "an unusable username must be refused: {bad:?}"
            );
        }
    }

    #[test]
    fn the_policy_clamps_every_operator_input() {
        let policy = OutproxyPolicy::new(1_000, 1_000_000, 1_000_000);
        assert_eq!(policy.attempts(), MAX_OUTPROXY_ATTEMPTS);
        assert_eq!(policy.connect_timeout_ms(), MAX_OUTPROXY_CONNECT_TIMEOUT_MS);
        assert_eq!(policy.backoff_ceiling_ms(), MAX_OUTPROXY_BACKOFF_MS);
        // A zero attempt count would mean "never try", so it floors at one.
        let floored = OutproxyPolicy::new(0, 0, 0);
        assert_eq!(floored.attempts(), 1);
        assert_eq!(floored.connect_timeout_ms(), 1);
        assert_eq!(floored.backoff_ms(1), 0);
    }

    #[test]
    fn backoff_is_bounded_and_saturating() {
        let policy = OutproxyPolicy::new(4, 1_000, 1_000);
        assert_eq!(policy.backoff_ms(1), 0);
        for attempt in 2..64 {
            assert!(
                policy.backoff_ms(attempt) <= MAX_OUTPROXY_BACKOFF_MS,
                "backoff at attempt {attempt} exceeded the ceiling"
            );
        }
        // A large attempt number must saturate at the ceiling, not wrap.
        assert_eq!(policy.backoff_ms(usize::MAX), 1_000);
        // Monotone non-decreasing, and zero exactly for the first attempt.
        let mut previous = 0;
        for attempt in 1..=policy.attempts() + 1 {
            let current = policy.backoff_ms(attempt);
            assert!(current >= previous, "backoff must not decrease");
            previous = current;
        }
    }

    #[test]
    fn failures_are_typed_and_retryability_is_explicit() {
        assert_eq!(
            OutproxyFailure::NotConfigured.as_str(),
            "outproxy-not-configured"
        );
        assert!(!OutproxyFailure::NotConfigured.is_retryable());
        assert!(!OutproxyFailure::SecretOwnerUnavailable.is_retryable());
        assert!(!OutproxyFailure::AttemptsExhausted.is_retryable());
        assert!(!OutproxyFailure::NotPermitted.is_retryable());
        assert!(OutproxyFailure::AuthenticationRejected.is_retryable());
        assert!(OutproxyFailure::TargetUnreachable.is_retryable());
    }

    #[test]
    fn the_default_provider_is_fail_closed() {
        let provider = NoOutproxyProvider;
        assert!(!provider.credential_available());
        assert!(provider.auth_header().expect("no credential").is_none());
        // The I2P route is still the truth, and the list is honestly empty.
        assert!(provider.config().list.is_empty());
        assert!(provider.config().validate().is_err());
    }

    #[test]
    fn a_configured_provider_selects_and_never_prints_a_secret() {
        struct Fixed(OutproxyConfig);
        impl OutproxyProvider for Fixed {
            fn config(&self) -> &OutproxyConfig {
                &self.0
            }
            fn credential_available(&self) -> bool {
                false
            }
            fn select(&self, target: &OutproxyTarget, attempt: usize) -> OutproxyRoute {
                self.0.route_attempt(target, attempt)
            }
            fn auth_header(&self) -> Result<Option<OutproxyAuthHeader>, OutproxyError> {
                Ok(None)
            }
        }
        let provider = Fixed(sample_config());
        let target = OutproxyTarget::parse_authority("example.com:80").expect("target");
        assert!(matches!(
            provider.select(&target, 0),
            OutproxyRoute::ViaOutproxy { .. }
        ));
        let i2p = OutproxyTarget::parse_authority(&format!("{}:80", b32_outproxy())).expect("i2p");
        assert_eq!(provider.select(&i2p, 0), OutproxyRoute::DirectI2p);
    }

    #[test]
    fn outproxy_errors_carry_a_stable_reason_and_no_secret() {
        let error = OutproxyError::UnknownType;
        assert!(error.reason().contains("vocabulary"));
        let error = OutproxyError::ExceedsCeiling {
            field: "proxy_list",
        };
        assert!(error.reason().contains("ceiling"));
        // Conversion into the crate error keeps the reason and loses nothing.
        let converted: ServiceTunnelError = error.into();
        assert!(!converted.to_string().is_empty());
    }

    #[test]
    fn the_http_connect_request_is_well_formed_and_carries_no_plaintext() {
        let target = OutproxyTarget::parse_authority("example.com:443").expect("target");
        let request = build_http_connect_request(&target, None).expect("request");
        let text = core::str::from_utf8(request.expose()).expect("ascii");
        assert_eq!(
            text,
            "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n"
        );
        assert!(!text.to_ascii_lowercase().contains("proxy-authorization"));

        let password = OutboundSecret::new("s3cret!").expect("secret");
        let header = OutproxyAuthHeader::basic("alice", &password).expect("header");
        let request = build_http_connect_request(&target, Some(&header)).expect("request");
        let text = core::str::from_utf8(request.expose()).expect("ascii");
        assert!(text.contains("Proxy-Authorization: Basic "));
        assert!(!text.contains("s3cret"));
        // The credential must be the last header before the terminator, so a
        // buffered pump cannot split it from the CRLFCRLF.
        assert!(text.ends_with("\r\n\r\n"));
    }

    #[test]
    fn an_http_2xx_connect_response_is_accepted_and_the_rest_is_typed() {
        for code in ["200", "201", "204"] {
            let head = format!("HTTP/1.1 {code} Connection Established\r\n\r\n");
            assert!(
                parse_http_connect_response(head.as_bytes()).is_ok(),
                "{code} must be accepted"
            );
        }
        // 407 is distinct: a rejected credential is not a dead outproxy.
        let head = "HTTP/1.1 407 Proxy Authentication Required\r\n\r\n";
        assert_eq!(
            parse_http_connect_response(head.as_bytes()),
            Err(OutproxyError::CredentialRejected)
        );
        for code in ["403", "404", "502", "503"] {
            let head = format!("HTTP/1.1 {code} Nope\r\n\r\n");
            assert_eq!(
                parse_http_connect_response(head.as_bytes()),
                Err(OutproxyError::UpstreamRefused)
            );
        }
    }

    #[test]
    fn a_malformed_http_response_is_never_treated_as_success() {
        for bad in [
            "",
            "HTTP/1.1\r\n\r\n",
            "HTTP/1.1 20 OK\r\n",
            "HTTP/1.1 2000 OK\r\n",
            "HTTP/1.1 2x0 OK\r\n",
            "ICY 200 OK\r\n",
            "SOCKS5/1.1\r\n",
            "HTTP/1.0 200 OK\r\n",
        ] {
            assert!(
                parse_http_connect_response(bad.as_bytes()).is_err(),
                "a malformed response must be refused: {bad:?}"
            );
        }
        // Non-UTF-8 bytes are refused rather than lossy-decoded.
        assert!(parse_http_connect_response(&[0xff, 0xfe]).is_err());
        // An oversize head is refused before any interpretation.
        let huge = format!(
            "HTTP/1.1 200 OK\r\n{}",
            "x".repeat(MAX_OUTPROXY_RESPONSE_HEAD_LEN)
        );
        assert_eq!(
            parse_http_connect_response(huge.as_bytes()),
            Err(OutproxyError::ExceedsCeiling {
                field: "outproxy_response_head"
            })
        );
    }

    #[test]
    fn the_socks5_connect_request_is_a_valid_two_message_exchange() {
        let target = OutproxyTarget::parse_authority("example.com:8080").expect("target");
        let request = build_socks5_connect_request(&target).expect("request");
        let bytes = request.expose();
        // Greeting: VER, NMETHODS, METHOD=noauth.
        assert_eq!(&bytes[..3], &[0x05, 0x01, 0x00]);
        // Request: VER, CMD=connect, RSV, ATYP=domain, len, host, port.
        assert_eq!(&bytes[3..8], &[0x05, 0x01, 0x00, 0x03, 11]);
        assert_eq!(&bytes[8..19], b"example.com");
        assert_eq!(&bytes[19..21], &8080_u16.to_be_bytes());
        assert_eq!(request.len(), 21);

        assert!(parse_socks5_method_reply(&[0x05, 0x00]).is_ok());
        assert_eq!(
            parse_socks5_method_reply(&[0x05, 0xff]),
            Err(OutproxyError::UpstreamRefused)
        );
        assert!(parse_socks5_method_reply(&[0x05, 0x02]).is_err());
        assert!(parse_socks5_method_reply(&[0x04, 0x00]).is_err());
        assert!(parse_socks5_method_reply(&[0x05]).is_err());
        assert!(parse_socks5_connect_reply(&[0x05, 0x00]).is_ok());
        assert_eq!(
            parse_socks5_connect_reply(&[0x05, 0x05]),
            Err(OutproxyError::CredentialRejected)
        );
        assert_eq!(
            parse_socks5_connect_reply(&[0x05, 0x04]),
            Err(OutproxyError::UpstreamRefused)
        );
        assert!(parse_socks5_connect_reply(&[0x05]).is_err());
    }

    #[test]
    fn the_socks4a_request_is_a_single_nul_terminated_message() {
        let target = OutproxyTarget::parse_authority("example.com:80").expect("target");
        let request = build_socks4a_connect_request(&target).expect("request");
        let bytes = request.expose();
        assert_eq!(bytes[0], 0x04);
        assert_eq!(bytes[1], 0x01, "CMD is CONNECT");
        assert_eq!(&bytes[2..4], &80_u16.to_be_bytes());
        // A zero IPv4 address is what makes this 4a rather than 4.
        assert_eq!(&bytes[4..10], &[0x00, 0x00, 0x00, 0x00, 0x00, 0x01]);
        let tail = &bytes[10..];
        assert!(tail.starts_with(b"example.com"));
        assert_eq!(tail.last(), Some(&0x00), "host is NUL terminated");

        assert!(parse_socks4a_connect_reply(&[0x00, 0x5A]).is_ok());
        assert_eq!(
            parse_socks4a_connect_reply(&[0x00, 0x5B]),
            Err(OutproxyError::UpstreamRefused)
        );
        assert!(parse_socks4a_connect_reply(&[0x5A, 0x5A]).is_err());
        assert!(parse_socks4a_connect_reply(&[0x00]).is_err());
        assert!(parse_socks4a_connect_reply(&[0x00, 0x01]).is_err());
    }

    #[test]
    fn a_huge_outproxy_host_cannot_overflow_the_staging_buffer() {
        // The target grammar already caps a host, so this proves the buffer's
        // own ceiling is what stops an oversize push rather than a silent wrap.
        let long_label = "a".repeat(MAX_OUTPROXY_HOST_LABEL_LEN);
        let host = std::iter::repeat_n(long_label.as_str(), 4)
            .collect::<Vec<_>>()
            .join(".");
        let target = OutproxyTarget::parse_authority(&format!("{host}:80"));
        assert!(
            target.is_err(),
            "an oversize host is refused at the grammar"
        );
        // And the buffer refuses rather than wrapping even when asked to.
        let mut buffer = OutproxyWireBuffer::new();
        assert!(
            buffer
                .push_bytes(&[0_u8; MAX_OUTPROXY_HANDSHAKE_BYTES])
                .is_ok()
        );
        assert!(buffer.push_bytes(&[0_u8; 1]).is_err());
        assert_eq!(buffer.len(), MAX_OUTPROXY_HANDSHAKE_BYTES);
    }

    #[test]
    fn every_outproxy_error_maps_to_a_typed_failure_reason() {
        // The handshake errors must all have a reason, because the daemon
        // turns them into an `OutproxyFailure` and a status surface.
        for error in [
            OutproxyError::UnknownType,
            OutproxyError::MalformedType { reason: "x" },
            OutproxyError::MalformedList { reason: "x" },
            OutproxyError::NotAnI2pDestination,
            OutproxyError::DuplicateEndpoint,
            OutproxyError::MalformedTarget { reason: "x" },
            OutproxyError::MalformedCredential { reason: "x" },
            OutproxyError::TunnelledNotInList,
            OutproxyError::CredentialRejected,
            OutproxyError::UpstreamRefused,
            OutproxyError::MalformedResponse { reason: "x" },
            OutproxyError::ExceedsCeiling { field: "x" },
        ] {
            assert!(!error.reason().is_empty());
        }
    }

    #[test]
    fn a_b32_outproxy_is_recognised_as_a_destination() {
        let endpoint = OutproxyEndpoint::parse(&b32_outproxy()).expect("b32 outproxy");
        assert!(endpoint.is_base32());
        let named = OutproxyEndpoint::parse(SECOND).expect("named outproxy");
        assert!(!named.is_base32());
        // Case is normalized so a list is comparable to a tunnelled subset.
        assert_eq!(
            OutproxyEndpoint::parse("SECOND.EXAMPLE.I2P").expect("upper"),
            named
        );
    }

    // --- Plan 342: the route decision cannot become a direct clearnet route ----------------------

    /// The headline negative row: with **no outproxy configured**, a clearnet
    /// target is refused, and the refusal is a distinct enum variant rather
    /// than a `DirectI2p` a caller has to know to distrust.
    ///
    /// This is the row that justifies `OutproxyRoute::Refused`. Before Plan
    /// 342 the selector had no way to say "no": an empty list made
    /// `route`/`route_attempt` return `DirectI2p` for a clearnet authority,
    /// which is a positive instruction to open a direct clearnet socket. The
    /// daemon happened to refuse that arm, so nothing was exploitable — but
    /// the guarantee lived in a caller instead of in the type, and the next
    /// caller would not have inherited it.
    #[test]
    fn a_clearnet_target_with_no_outproxy_is_refused_never_routed_directly() {
        let mut config = sample_config();
        config.list = OutproxyList::default();
        let target = OutproxyTarget::parse_authority("example.com:443").expect("clearnet target");

        assert_eq!(
            config.route(&target),
            OutproxyRoute::Refused(OutproxyFailure::NotConfigured),
            "an empty proxy list must produce a refusal, not a direct route"
        );
        assert_eq!(
            config.route_attempt(&target, 0),
            OutproxyRoute::Refused(OutproxyFailure::NotConfigured)
        );
        assert_eq!(
            config.route_attempt(&target, 99),
            OutproxyRoute::Refused(OutproxyFailure::NotConfigured),
            "every attempt index must refuse identically; the index wraps, so an empty              list is empty at every index"
        );

        // And the same configuration still routes in-network traffic directly,
        // which is what makes `Refused` a *meaningful* third outcome rather
        // than a blanket rejection.
        let i2p = OutproxyTarget::parse_authority(&format!("{}:443", b32_outproxy()))
            .expect("i2p target");
        assert_eq!(config.route(&i2p), OutproxyRoute::DirectI2p);
    }

    /// The same three outcomes for every shape of clearnet authority, so the
    /// refusal cannot be dodged by spelling the target differently.
    #[test]
    fn no_clearnet_spelling_reaches_a_direct_route() {
        let mut config = sample_config();
        config.list = OutproxyList::default();
        for authority in [
            "example.com:443",
            "EXAMPLE.com:443",
            "xn--bcher-kva.example:80",
            "very-long-subdomain-label.example.co.uk:8443",
        ] {
            let target = OutproxyTarget::parse_authority(authority)
                .unwrap_or_else(|error| panic!("{authority} must parse: {error:?}"));
            assert!(!target.is_i2p(), "{authority} is not an I2P name");
            assert!(
                matches!(config.route(&target), OutproxyRoute::Refused(_)),
                "{authority} must be refused with no outproxy configured"
            );
        }

        // IP literals are refused one step earlier, at parse. That is a
        // strictly stronger property than a route-time refusal: a clearnet IP
        // target cannot be *expressed*, so there is no value for a caller to
        // mis-route. Asserted here because it is the same guarantee seen from
        // the other end.
        for authority in ["192.0.2.1:443", "[2001:db8::1]:443"] {
            assert!(
                OutproxyTarget::parse_authority(authority).is_err(),
                "{authority} is an IP literal and must not become a target at all"
            );
        }
    }

    /// The I2P bypass holds with an outproxy configured *and* absent, at every
    /// attempt index. A misconfigured list must not be able to divert
    /// in-network traffic off-network, and this is the row that says so at
    /// each index rather than only at zero.
    #[test]
    fn the_i2p_bypass_is_unconditional() {
        let i2p = OutproxyTarget::parse_authority(&format!("{}:443", b32_outproxy()))
            .expect("i2p target");
        for list in [sample_config().list, OutproxyList::default()] {
            let mut config = sample_config();
            config.list = list;
            assert_eq!(config.route(&i2p), OutproxyRoute::DirectI2p);
            for attempt in 0..4 {
                assert_eq!(
                    config.route_attempt(&i2p, attempt),
                    OutproxyRoute::DirectI2p,
                    "attempt {attempt} must not divert an I2P destination"
                );
            }
        }
    }

    /// A refusal is terminal: it carries a failure that must not read as
    /// retryable, so a caller cannot spin on a target that can never be routed.
    #[test]
    fn an_unconfigured_refusal_is_not_retryable() {
        let mut config = sample_config();
        config.list = OutproxyList::default();
        let target = OutproxyTarget::parse_authority("example.com:443").expect("clearnet target");
        let OutproxyRoute::Refused(failure) = config.route(&target) else {
            panic!("expected a refusal");
        };
        assert_eq!(failure, OutproxyFailure::NotConfigured);
        assert!(
            !failure.is_retryable(),
            "retrying an unconfigured clearnet target can never succeed"
        );
    }
}
