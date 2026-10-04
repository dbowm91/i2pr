//! Plan 290 runtime-neutral HTTP server profile filter.
//!
//! Java I2PTunnel's HTTP server mode ("Creates a destination to a
//! local HTTP server ip:port") filters inbound requests before they
//! reach the local webserver: header validation, header spoof
//! protection, header size checks, `Host` replacement, `X-I2P`
//! provenance, forced `Connection: close`, hop-by-hop stripping,
//! buffered headers, POST throttling, slowloris timeouts, response
//! privacy stripping, and bounded error responses.
//!
//! This module owns the runtime-neutral half of that contract:
//!
//! - request side: origin-form enforcement (the client proxy always
//!   rewrites to origin-form, so absolute/authority forms are a
//!   framing mismatch here, never silently reinterpreted), required
//!   `Host`, `Transfer-Encoding` rejection (this profile never
//!   forwards dechunked bodies), hop-by-hop + identifying-header
//!   stripping, `Host` replacement with the configured local target
//!   authority (spoof protection), forced `Connection: close`;
//! - response side: status-line validation, hop-by-hop +
//!   `Server`/`Via` stripping, framing preservation
//!   (`Content-Length` / `Transfer-Encoding` / `Content-Encoding`
//!   pass through so the transparent `x-i2p-gzip` cooperation and
//!   chunked framing keep working), forced `Connection: close`.
//!
//! Timeouts, slowloris bounds, POST chunk pacing, and socket
//! orchestration belong to the daemon: this module only validates
//! and rewrites bounded byte sections. Peer-identity headers are
//! deliberately NOT injected (privacy-strict deviation from Java:
//! the local webserver never learns the client's destination; see
//! the closure record).

#![forbid(unsafe_code)]

use super::error::{HttpError, HttpErrorKind};
use super::parser::{HeaderEntry, HeaderName, HttpRequestHead};
use super::target::parse_origin_form;
use std::collections::BTreeMap;

/// Maximum bytes for the validated local authority replacement
/// (`ip:port` text the daemon derives from the server target).
pub const SERVER_AUTHORITY_MAX_BYTES: usize = 256;
/// Maximum byte length of a Proposal `SpoofedHost` DNS host name.
pub const SPOOFED_HOST_MAX_BYTES: usize = 253;
/// Maximum authenticated peers retained for per-client POST limits.
pub const MAX_POST_LIMIT_PEERS: usize = 4096;
const MAX_POST_LIMIT_VALUE: u32 = 100_000;

/// Proposal HTTP POST throttle values. Zero disables the corresponding
/// limit; the configured window is shared by per-client and total counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HttpPostLimits {
    /// Fixed counting window duration in seconds (`PostLimit`).
    pub window_seconds: u32,
    /// Per-client ban duration in seconds (`PostLimitTime`).
    pub client_ban_seconds: u32,
    /// Maximum POST requests per peer within one window (`PerClientPeriod`).
    pub client_max: u32,
    /// Maximum POST requests from all peers within one window (`TotalPeriod`).
    pub total_max: u32,
    /// Aggregate ban duration in seconds (`TotalBanTime`).
    pub total_ban_seconds: u32,
}

impl HttpPostLimits {
    /// Checks the Proposal field bounds and action pairings.
    pub fn validate(self) -> Result<Self, crate::errors::ServiceTunnelError> {
        if [
            self.window_seconds,
            self.client_ban_seconds,
            self.client_max,
            self.total_max,
            self.total_ban_seconds,
        ]
        .iter()
        .any(|value| *value > MAX_POST_LIMIT_VALUE)
            || ((self.client_max != 0 || self.total_max != 0) && self.window_seconds == 0)
            || (self.client_ban_seconds != 0 && self.client_max == 0)
            || (self.total_ban_seconds != 0 && self.total_max == 0)
        {
            return Err(crate::errors::ServiceTunnelError::ContradictoryOptions {
                id: String::new(),
                reason: "HTTP POST limits require a bounded window and matching limits",
            });
        }
        Ok(self)
    }

    /// Whether any POST throttling is configured.
    pub fn enabled(self) -> bool {
        self.client_max != 0 || self.total_max != 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PostPeerWindow {
    epoch: u64,
    count: u32,
    banned_until: u64,
}

/// Bounded authenticated-peer and aggregate POST accounting.
#[derive(Debug)]
pub struct HttpPostLimiter {
    limits: HttpPostLimits,
    total_epoch: u64,
    total_count: u32,
    total_banned_until: u64,
    peers: BTreeMap<[u8; 32], PostPeerWindow>,
}

impl HttpPostLimiter {
    /// Creates a fresh limiter for one HTTP server generation.
    pub fn new(limits: HttpPostLimits) -> Self {
        Self {
            limits,
            total_epoch: u64::MAX,
            total_count: 0,
            total_banned_until: 0,
            peers: BTreeMap::new(),
        }
    }

    /// Admits one parsed POST request using process-monotonic seconds.
    /// At peer-table capacity, expired records are reclaimed and a new
    /// peer fails closed if the bounded table remains full.
    pub fn admit_post(&mut self, peer: [u8; 32], now_seconds: u64) -> bool {
        if !self.limits.enabled() {
            return true;
        }
        if self.total_banned_until > now_seconds {
            return false;
        }
        let window = u64::from(self.limits.window_seconds);
        let epoch = now_seconds / window;
        if self.total_epoch != epoch {
            self.total_epoch = epoch;
            self.total_count = 0;
        }
        if self.limits.total_max != 0 && self.total_count >= self.limits.total_max {
            self.total_banned_until =
                now_seconds.saturating_add(u64::from(self.limits.total_ban_seconds));
            return false;
        }

        let client_limit_enabled = self.limits.client_max != 0;
        if client_limit_enabled && !self.peers.contains_key(&peer) {
            if self.peers.len() >= MAX_POST_LIMIT_PEERS {
                self.peers
                    .retain(|_, state| state.epoch == epoch || state.banned_until > now_seconds);
            }
            if self.peers.len() >= MAX_POST_LIMIT_PEERS {
                return false;
            }
        }
        let mut client_state = self.peers.get(&peer).copied().unwrap_or_default();
        if client_state.banned_until > now_seconds {
            return false;
        }
        if client_state.epoch != epoch {
            client_state.epoch = epoch;
            client_state.count = 0;
            client_state.banned_until = 0;
        }
        if client_limit_enabled && client_state.count >= self.limits.client_max {
            client_state.banned_until =
                now_seconds.saturating_add(u64::from(self.limits.client_ban_seconds));
            self.peers.insert(peer, client_state);
            return false;
        }

        self.total_count = self.total_count.saturating_add(1);
        if client_limit_enabled {
            client_state.count = client_state.count.saturating_add(1);
            self.peers.insert(peer, client_state);
        }
        true
    }

    /// Number of currently retained peer records, exposed for boundedness tests.
    pub fn tracked_peers(&self) -> usize {
        self.peers.len()
    }
}

/// Whether `host` is a bounded ASCII DNS name suitable for an HTTP Host
/// field. Ports, userinfo, whitespace, and control bytes are not accepted.
pub fn valid_spoofed_host(host: &str) -> bool {
    if host.is_empty() || host.len() > SPOOFED_HOST_MAX_BYTES || !host.is_ascii() {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.as_bytes()[0].is_ascii_alphanumeric()
            && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

/// Presentation policy for the HTTP server profile (Plan 292
/// `address_helper` / `jump_list`). Both gates restrict which
/// request classes the server forwards to the local target; the
/// defaults preserve the historical forward-everything behavior
/// so enabling either gate only ever refuses more.
///
/// - `address_helper`: address-helper class requests (the
///   `/addresshelper` path family and the `i2paddresshelper`
///   query key) are forwarded when true, refused with 403 when
///   false. The local webserver is never used as an
///   addressbook oracle unless the operator opts in.
/// - `jump_list`: jump class requests (the `/jump` path family
///   and the `jump` query key) are forwarded when true, refused
///   with 403 when false.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpServerPolicy {
    /// Forward address-helper class requests.
    pub address_helper: bool,
    /// Forward jump class requests.
    pub jump_list: bool,
    /// Strip the inbound Referer header before forwarding.
    pub block_referers: bool,
    /// Reject requests carrying headers that identify an HTTP inproxy.
    pub block_access_in_proxies: bool,
    /// Reject requests whose User-Agent contains one of `user_agents`.
    pub block_user_agents: bool,
    /// Case-sensitive comma-separated User-Agent substring rules after
    /// parsing. The literal `none` matches a missing User-Agent header.
    pub user_agents: Vec<String>,
    /// Optional validated Host replacement for Proposal `SpoofedHost`.
    pub spoofed_host: Option<String>,
    /// Optional Proposal POST-count and ban policy.
    pub post_limits: HttpPostLimits,
}

impl Default for HttpServerPolicy {
    fn default() -> Self {
        Self {
            address_helper: true,
            jump_list: true,
            block_referers: true,
            block_access_in_proxies: false,
            block_user_agents: false,
            user_agents: Vec::new(),
            spoofed_host: None,
            post_limits: HttpPostLimits::default(),
        }
    }
}

impl HttpServerPolicy {
    /// Whether a request class may be forwarded to the local
    /// target under this policy.
    pub const fn admits(&self, class: PresentationClass) -> bool {
        match class {
            PresentationClass::Ordinary => true,
            PresentationClass::Helper => self.address_helper,
            PresentationClass::Jump => self.jump_list,
        }
    }
}

/// Presentation class of one origin-form server request target
/// (Plan 292 `address_helper` / `jump_list` gates).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationClass {
    /// Ordinary site content: always forwarded.
    Ordinary,
    /// Address-helper class: the `/addresshelper` path family or
    /// the `i2paddresshelper` query key. Forwarded only when the
    /// policy opens the helper gate.
    Helper,
    /// Jump class: the `/jump` path family or the `jump` query
    /// key. Forwarded only when the policy opens the jump gate.
    Jump,
}

/// Classifies one origin-form request target. Path families win
/// over query keys, and the helper class wins over jump when a
/// target carries both markers; ordinary content never matches.
pub fn classify_presentation(path: &str, query: &str) -> PresentationClass {
    if path == "/addresshelper" || path.starts_with("/addresshelper/") {
        return PresentationClass::Helper;
    }
    if path == "/jump" || path.starts_with("/jump/") {
        return PresentationClass::Jump;
    }
    let mut jump_seen = false;
    for pair in query.split('&') {
        let key = pair.split('=').next().unwrap_or("").trim();
        if key.eq_ignore_ascii_case("i2paddresshelper") {
            return PresentationClass::Helper;
        }
        if key.eq_ignore_ascii_case("jump") {
            jump_seen = true;
        }
    }
    if jump_seen {
        return PresentationClass::Jump;
    }
    PresentationClass::Ordinary
}

/// Filtered server-side request ready to forward to the local
/// target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilteredServerRequest {
    /// Serialized request head (`METHOD origin-form HTTP/1.1` +
    /// filtered headers + forced `Connection: close`).
    pub head_bytes: Vec<u8>,
    /// Declared `Content-Length`, if exactly one valid header was
    /// present. The daemon paces body forwarding against this
    /// bound.
    pub content_length: Option<u64>,
}

/// Filters one parsed inbound request head for the HTTP server
/// profile. `local_authority` is the daemon-derived `ip:port` text
/// of the loopback server target that replaces the inbound `Host`.
pub fn filter_server_request(
    head: &HttpRequestHead,
    local_authority: &str,
) -> Result<FilteredServerRequest, HttpError> {
    filter_server_request_with_spoofed_host(head, local_authority, None)
}

/// Filters a request using the configured `SpoofedHost` value when set;
/// the loopback target authority remains the default Host replacement.
pub fn filter_server_request_with_spoofed_host(
    head: &HttpRequestHead,
    local_authority: &str,
    spoofed_host: Option<&str>,
) -> Result<FilteredServerRequest, HttpError> {
    filter_server_request_with_policy(
        head,
        local_authority,
        &HttpServerPolicy {
            spoofed_host: spoofed_host.map(str::to_owned),
            ..HttpServerPolicy::default()
        },
    )
}

/// Filters a request using the committed HTTP server policy.
pub fn filter_server_request_with_policy(
    head: &HttpRequestHead,
    local_authority: &str,
    policy: &HttpServerPolicy,
) -> Result<FilteredServerRequest, HttpError> {
    let rejected = |kind, reason| HttpError::new(kind, reason);
    if head.line.method == "CONNECT" {
        return Err(rejected(
            HttpErrorKind::MethodNotAllowed,
            "server profile rejects CONNECT",
        ));
    }
    // Origin-form only: the client proxy rewrites absolute-form to
    // origin-form, so any other form here is a framing mismatch.
    // The client absolute-form parser is not reused: proxy
    // request-target semantics (absolute URIs naming remote
    // destinations) must not be conflated with server semantics
    // (a bare path on the local target).
    if !head.line.target.starts_with('/') {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "server profile requires origin-form",
        ));
    }
    let (_path, _query) = parse_origin_form(&head.line.target).map_err(HttpError::from)?;
    validate_local_authority(local_authority)?;
    if policy
        .spoofed_host
        .as_deref()
        .is_some_and(|host| !valid_spoofed_host(host))
    {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "configured SpoofedHost is not a valid DNS host",
        ));
    }
    // HTTP/1.1 requires Host; the parser already rejects
    // duplicate/conflicting Host headers, so at most one survives.
    let host_count = head
        .headers
        .iter()
        .filter(|entry| entry.name_str() == "host")
        .count();
    if host_count == 0 {
        return Err(rejected(
            HttpErrorKind::MalformedHeaders,
            "server profile requires a Host header",
        ));
    }
    if policy.block_access_in_proxies
        && head.headers.iter().any(|entry| {
            matches!(
                entry.name_str(),
                "x-forwarded-for" | "x-forwarded-server" | "forwarded" | "x-forwarded-host"
            )
        })
    {
        return Err(rejected(
            HttpErrorKind::PresentationRefused,
            "request appears to have passed through an HTTP inproxy",
        ));
    }
    if policy.block_user_agents && user_agent_is_blocked(&head.headers, &policy.user_agents) {
        return Err(rejected(
            HttpErrorKind::PresentationRefused,
            "request User-Agent is blocked by the server policy",
        ));
    }
    // This profile never forwards dechunked bodies: reject chunked
    // framing instead of stripping the header and corrupting the
    // body framing for the local target.
    if head
        .headers
        .iter()
        .any(|entry| entry.name_str() == "transfer-encoding")
    {
        return Err(rejected(
            HttpErrorKind::SmugglingAmbiguity,
            "server profile rejects Transfer-Encoding",
        ));
    }
    let mut connection_list: Vec<String> = Vec::new();
    for entry in &head.headers {
        if entry.name_str() == "connection" {
            for token in entry.value.split(',') {
                let trimmed = token.trim();
                if !trimmed.is_empty() {
                    connection_list.push(trimmed.to_ascii_lowercase());
                }
            }
        }
    }
    let mut content_lengths: Vec<u64> = Vec::new();
    let mut output: Vec<HeaderEntry> = Vec::with_capacity(head.headers.len());
    let mut host_inserted = false;
    for entry in &head.headers {
        let name = entry.name_str();
        if name == "connection" || connection_list.iter().any(|listed| listed == name) {
            continue;
        }
        if matches!(
            name,
            "proxy-connection" | "keep-alive" | "te" | "trailer" | "upgrade" | "transfer-encoding"
        ) {
            continue;
        }
        // Identifying-header strip (same set as the client
        // profile; the local target must not learn browser
        // provenance from a forwarded request).
        match name {
            "via"
            | "forwarded"
            | "x-forwarded-for"
            | "x-forwarded-host"
            | "x-forwarded-proto"
            | "proxy-authorization"
            | "from" => continue,
            _ => {}
        }
        if policy.block_referers && name == "referer" {
            continue;
        }
        if name == "content-length" {
            let length: u64 = entry.value.trim().parse().map_err(|_| {
                rejected(
                    HttpErrorKind::MalformedHeaders,
                    "Content-Length is not a valid integer",
                )
            })?;
            content_lengths.push(length);
            continue;
        }
        if name == "host" {
            if !host_inserted {
                output.push(HeaderEntry {
                    name: HeaderName("host".to_owned()),
                    value: policy
                        .spoofed_host
                        .as_deref()
                        .unwrap_or(local_authority)
                        .to_owned(),
                });
                host_inserted = true;
            }
            continue;
        }
        output.push(entry.clone());
    }
    // Duplicate Content-Length values are ambiguous even when
    // identical; fail closed.
    if content_lengths.len() > 1 {
        return Err(rejected(
            HttpErrorKind::SmugglingAmbiguity,
            "duplicate Content-Length",
        ));
    }
    let content_length = content_lengths.into_iter().next();
    if let Some(length) = content_length {
        output.push(HeaderEntry {
            name: HeaderName("content-length".to_owned()),
            value: length.to_string(),
        });
    }
    output.push(HeaderEntry {
        name: HeaderName("connection".to_owned()),
        value: "close".to_owned(),
    });
    let mut head_bytes = Vec::with_capacity(512);
    head_bytes.extend_from_slice(head.line.method.as_bytes());
    head_bytes.push(b' ');
    head_bytes.extend_from_slice(head.line.target.as_bytes());
    head_bytes.extend_from_slice(b" HTTP/1.1\r\n");
    for entry in &output {
        head_bytes.extend_from_slice(entry.name_str().as_bytes());
        head_bytes.extend_from_slice(b": ");
        head_bytes.extend_from_slice(entry.value.as_bytes());
        head_bytes.extend_from_slice(b"\r\n");
    }
    head_bytes.extend_from_slice(b"\r\n");
    Ok(FilteredServerRequest {
        head_bytes,
        content_length,
    })
}

fn user_agent_is_blocked(headers: &[HeaderEntry], rules: &[String]) -> bool {
    let user_agents: Vec<&str> = headers
        .iter()
        .filter(|entry| entry.name_str() == "user-agent")
        .map(|entry| entry.value.as_str())
        .collect();
    if user_agents.is_empty() {
        return rules.iter().any(|rule| rule == "none");
    }
    user_agents.iter().any(|agent| {
        !agent.starts_with("MYOB")
            && rules
                .iter()
                .any(|rule| rule != "none" && !rule.is_empty() && agent.contains(rule))
    })
}

/// Validates the bounded, case-sensitive HTTP-server User-Agent rules.
pub fn valid_user_agent_rules(rules: &[String]) -> bool {
    rules.len() <= super::config::HTTP_USER_AGENT_RULES_MAX
        && rules.iter().all(|rule| {
            !rule.is_empty()
                && rule.len() <= super::config::HTTP_USER_AGENT_RULE_MAX_BYTES
                && !rule.bytes().any(|byte| byte.is_ascii_control())
        })
}

/// Validates the daemon-supplied local authority replacement.
fn validate_local_authority(authority: &str) -> Result<(), HttpError> {
    let rejected = |kind, reason| HttpError::new(kind, reason);
    if authority.is_empty() || authority.len() > SERVER_AUTHORITY_MAX_BYTES {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "local authority exceeds the ceiling",
        ));
    }
    if authority.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "local authority contains control or whitespace bytes",
        ));
    }
    // A colon may only appear as the single host/port separator;
    // anything else (userinfo `@`, second colon) is rejected.
    if authority.contains('@') || authority.bytes().filter(|byte| *byte == b':').count() != 1 {
        return Err(rejected(
            HttpErrorKind::MalformedTarget,
            "local authority must be host:port",
        ));
    }
    Ok(())
}

/// Filters one raw response head section captured from the local
/// target. Returns the serialized filtered head
/// (`status-line + headers + forced Connection: close`).
pub fn filter_server_response(head_bytes: &[u8]) -> Result<Vec<u8>, HttpError> {
    use super::config::HTTP_TOTAL_HEADER_MAX_BYTES;
    let rejected = |kind, reason| HttpError::new(kind, reason);
    if head_bytes.len() > HTTP_TOTAL_HEADER_MAX_BYTES {
        return Err(rejected(
            HttpErrorKind::BufferCeilingExceeded,
            "response head exceeds the ceiling",
        ));
    }
    let head_text = std::str::from_utf8(head_bytes).map_err(|_| {
        rejected(
            HttpErrorKind::MalformedHeaders,
            "response head is not valid UTF-8",
        )
    })?;
    let mut lines = head_text.split("\r\n");
    let status_line = lines.next().ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedHeaders,
            "response head has no status line",
        )
    })?;
    validate_status_line(status_line)?;
    let mut connection_list: Vec<String> = Vec::new();
    let mut parsed: Vec<(String, String)> = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some(colon) = line.find(':') else {
            return Err(rejected(
                HttpErrorKind::MalformedHeaders,
                "response header has no colon",
            ));
        };
        let (name, value) = (&line[..colon], &line[colon + 1..]);
        validate_response_field_name(name)?;
        let value = value.trim();
        if value.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
            return Err(rejected(
                HttpErrorKind::MalformedHeaders,
                "response value contains control bytes",
            ));
        }
        let lower = name.to_ascii_lowercase();
        if lower == "connection" {
            for token in value.split(',') {
                let trimmed = token.trim();
                if !trimmed.is_empty() {
                    connection_list.push(trimmed.to_ascii_lowercase());
                }
            }
            continue;
        }
        parsed.push((lower, value.to_owned()));
    }
    let mut output: Vec<u8> = Vec::with_capacity(head_bytes.len());
    output.extend_from_slice(status_line.as_bytes());
    output.extend_from_slice(b"\r\n");
    for (name, value) in &parsed {
        if connection_list.iter().any(|listed| listed == name) {
            continue;
        }
        // Privacy-problematic response headers plus hop-by-hop
        // leftovers. Framing (`content-length`,
        // `transfer-encoding`) and content coding
        // (`content-encoding`, including `x-i2p-gzip`) pass
        // through untouched.
        if matches!(
            name.as_str(),
            "proxy-connection" | "keep-alive" | "te" | "trailer" | "upgrade" | "via" | "server"
        ) {
            continue;
        }
        output.extend_from_slice(name.as_bytes());
        output.extend_from_slice(b": ");
        output.extend_from_slice(value.as_bytes());
        output.extend_from_slice(b"\r\n");
    }
    output.extend_from_slice(b"connection: close\r\n\r\n");
    Ok(output)
}

/// Validates one response status line (`HTTP/1.0|HTTP/1.1 SP DDD SP reason`).
fn validate_status_line(line: &str) -> Result<(), HttpError> {
    let rejected = |kind, reason| HttpError::new(kind, reason);
    let (version, rest) = line.split_once(' ').ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedHeaders,
            "response status line is malformed",
        )
    })?;
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(rejected(
            HttpErrorKind::MalformedHeaders,
            "response version must be HTTP/1.x",
        ));
    }
    let (code, _) = rest.split_once(' ').ok_or_else(|| {
        rejected(
            HttpErrorKind::MalformedHeaders,
            "response status line is malformed",
        )
    })?;
    if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(rejected(
            HttpErrorKind::MalformedHeaders,
            "response status must be three digits",
        ));
    }
    Ok(())
}

/// Validates one response field name (visible ASCII token, no
/// spaces, no colon, no control bytes).
fn validate_response_field_name(name: &str) -> Result<(), HttpError> {
    let rejected = |kind, reason| HttpError::new(kind, reason);
    if name.is_empty() || name.len() > super::config::HTTP_FIELD_NAME_MAX_BYTES {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "response field name exceeds the ceiling",
        ));
    }
    if name
        .bytes()
        .any(|byte| byte <= 0x20 || byte == 0x7f || byte == b':' || byte == b'\r' || byte == b'\n')
    {
        return Err(rejected(
            HttpErrorKind::MalformedField,
            "response field name is not a valid token",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse_request_head;
    use super::*;
    use crate::http::HttpLimits;

    fn parse_head(raw: &str) -> HttpRequestHead {
        parse_request_head(raw.as_bytes(), HttpLimits::defaults()).expect("head parses")
    }

    #[test]
    fn post_limiter_enforces_client_total_bans_and_window_reset() {
        let mut limiter = HttpPostLimiter::new(HttpPostLimits {
            window_seconds: 300,
            client_ban_seconds: 20,
            client_max: 1,
            total_max: 3,
            total_ban_seconds: 10,
        });
        assert!(limiter.admit_post([1; 32], 1));
        assert!(!limiter.admit_post([1; 32], 2));
        assert!(!limiter.admit_post([1; 32], 21));
        assert!(limiter.admit_post([2; 32], 22));
        assert!(limiter.admit_post([3; 32], 23));
        assert!(!limiter.admit_post([4; 32], 24));
        assert!(!limiter.admit_post([2; 32], 30), "aggregate ban is shared");
        assert!(
            limiter.admit_post([1; 32], 300),
            "new fixed window resets counts"
        );
    }

    #[test]
    fn post_limiter_bounds_peer_tracking_and_fails_closed() {
        let mut limiter = HttpPostLimiter::new(HttpPostLimits {
            window_seconds: 300,
            client_max: 1,
            ..HttpPostLimits::default()
        });
        for index in 0..MAX_POST_LIMIT_PEERS {
            let mut peer = [0_u8; 32];
            peer[..4].copy_from_slice(&(index as u32).to_be_bytes());
            assert!(limiter.admit_post(peer, 1));
        }
        assert_eq!(limiter.tracked_peers(), MAX_POST_LIMIT_PEERS);
        assert!(!limiter.admit_post([0xff; 32], 2));
        assert!(limiter.admit_post([0xff; 32], 300));
        assert_eq!(limiter.tracked_peers(), 1);
    }

    #[test]
    fn origin_form_request_is_filtered() {
        let head =
            parse_head("GET /path?q=1 HTTP/1.1\r\nHost: example.i2p\r\nVia: 1.1 proxy\r\n\r\n");
        let filtered = filter_server_request(&head, "127.0.0.1:8080").expect("filters");
        let text = std::str::from_utf8(&filtered.head_bytes).expect("utf-8");
        assert!(text.starts_with("GET /path?q=1 HTTP/1.1\r\n"));
        assert!(text.contains("host: 127.0.0.1:8080\r\n"));
        assert!(!text.contains("example.i2p"));
        assert!(!text.to_ascii_lowercase().contains("via:"));
        assert!(text.contains("connection: close\r\n"));
        assert_eq!(filtered.content_length, None);
    }

    #[test]
    fn proposal_spoofed_host_overrides_only_the_forwarded_host() {
        let head = parse_head(
            "GET / HTTP/1.1\r\nHost: attacker.example\r\nX-Forwarded-Host: attacker.example\r\n\r\n",
        );
        let filtered = filter_server_request_with_spoofed_host(
            &head,
            "127.0.0.1:8080",
            Some("site.example.i2p"),
        )
        .expect("valid SpoofedHost filters");
        let text = std::str::from_utf8(&filtered.head_bytes).expect("utf-8");
        assert!(text.contains("host: site.example.i2p\r\n"));
        assert!(!text.contains("attacker.example"));
        assert!(!text.contains("127.0.0.1:8080"));
    }

    #[test]
    fn proposal_user_agent_blocklist_is_bounded_case_sensitive_and_keeps_myob() {
        let policy = HttpServerPolicy {
            block_user_agents: true,
            user_agents: vec!["crawler".to_owned(), "none".to_owned()],
            ..HttpServerPolicy::default()
        };
        let blocked = parse_head(
            "GET / HTTP/1.1\r\nHost: example.i2p\r\nUser-Agent: Example crawler\r\n\r\n",
        );
        assert_eq!(
            filter_server_request_with_policy(&blocked, "127.0.0.1:8080", &policy)
                .expect_err("matching substring is rejected")
                .kind,
            HttpErrorKind::PresentationRefused
        );
        let case_mismatch =
            parse_head("GET / HTTP/1.1\r\nHost: example.i2p\r\nUser-Agent: CRAWLER\r\n\r\n");
        assert!(
            filter_server_request_with_policy(&case_mismatch, "127.0.0.1:8080", &policy).is_ok()
        );
        let default_agent = parse_head(
            "GET / HTTP/1.1\r\nHost: example.i2p\r\nUser-Agent: MYOB/6.66 (AN/ON)\r\n\r\n",
        );
        assert!(
            filter_server_request_with_policy(&default_agent, "127.0.0.1:8080", &policy).is_ok()
        );
        let missing_agent = parse_head("GET / HTTP/1.1\r\nHost: example.i2p\r\n\r\n");
        assert_eq!(
            filter_server_request_with_policy(&missing_agent, "127.0.0.1:8080", &policy)
                .expect_err("none rule rejects missing User-Agent")
                .kind,
            HttpErrorKind::PresentationRefused
        );
        let duplicate_agent = parse_head(
            "GET / HTTP/1.1\r\nHost: example.i2p\r\nUser-Agent: browser\r\nUser-Agent: crawler\r\n\r\n",
        );
        assert_eq!(
            filter_server_request_with_policy(&duplicate_agent, "127.0.0.1:8080", &policy)
                .expect_err("duplicate headers cannot bypass a matching rule")
                .kind,
            HttpErrorKind::PresentationRefused
        );
        assert!(!valid_user_agent_rules(&["\r\n".to_owned()]));
        assert!(!valid_user_agent_rules(&["x".repeat(
            super::super::config::HTTP_USER_AGENT_RULE_MAX_BYTES + 1
        )]));
    }

    #[test]
    fn proposal_block_access_in_proxies_rejects_forwarding_headers() {
        let policy = HttpServerPolicy {
            block_access_in_proxies: true,
            ..HttpServerPolicy::default()
        };
        for header in [
            "X-Forwarded-For: 192.0.2.1",
            "X-Forwarded-Server: proxy.example",
            "Forwarded: for=192.0.2.1",
            "X-Forwarded-Host: example.com",
        ] {
            let request = format!("GET / HTTP/1.1\r\nHost: example.i2p\r\n{header}\r\n\r\n");
            let head = parse_head(&request);
            assert_eq!(
                filter_server_request_with_policy(&head, "127.0.0.1:8080", &policy)
                    .expect_err("proxy-identifying header is refused")
                    .kind,
                HttpErrorKind::PresentationRefused,
                "header: {header}"
            );
        }
        let direct = parse_head("GET / HTTP/1.1\r\nHost: example.i2p\r\n\r\n");
        assert!(filter_server_request_with_policy(&direct, "127.0.0.1:8080", &policy).is_ok());
    }

    #[test]
    fn block_referers_defaults_to_strip_and_can_be_disabled_explicitly() {
        let head = parse_head(
            "GET / HTTP/1.1\r\nHost: example.i2p\r\nReferer: https://source.example/path\r\n\r\n",
        );
        let filtered = filter_server_request_with_policy(
            &head,
            "127.0.0.1:8080",
            &HttpServerPolicy::default(),
        )
        .expect("default filter");
        assert!(
            !filtered
                .head_bytes
                .windows(7)
                .any(|window| window == b"referer")
        );

        let policy = HttpServerPolicy {
            block_referers: false,
            ..HttpServerPolicy::default()
        };
        let filtered = filter_server_request_with_policy(&head, "127.0.0.1:8080", &policy)
            .expect("explicit forwarding policy");
        assert!(
            filtered
                .head_bytes
                .windows(b"referer: https://source.example/path".len())
                .any(|window| window == b"referer: https://source.example/path")
        );
    }

    #[test]
    fn spoofed_host_validation_rejects_header_injection_and_bad_labels() {
        assert!(valid_spoofed_host("service.i2p"));
        for host in [
            "",
            "a..i2p",
            "-bad.i2p",
            "bad-.i2p",
            "x.i2p:80",
            "x.i2p\r\nX: y",
        ] {
            assert!(!valid_spoofed_host(host), "accepted {host:?}");
        }
    }

    #[test]
    fn absolute_form_is_rejected() {
        let head = parse_head("GET http://example.i2p/path HTTP/1.1\r\nHost: example.i2p\r\n\r\n");
        let error = filter_server_request(&head, "127.0.0.1:8080").expect_err("absolute");
        assert_eq!(error.kind, HttpErrorKind::MalformedTarget);
    }

    #[test]
    fn connect_method_is_rejected() {
        let head = parse_head("CONNECT example.i2p:443 HTTP/1.1\r\nHost: example.i2p\r\n\r\n");
        let error = filter_server_request(&head, "127.0.0.1:8080").expect_err("connect");
        assert_eq!(error.kind, HttpErrorKind::MethodNotAllowed);
    }

    #[test]
    fn missing_host_is_rejected() {
        let head = parse_head("GET /path HTTP/1.1\r\nX-Custom: yes\r\n\r\n");
        let error = filter_server_request(&head, "127.0.0.1:8080").expect_err("host");
        assert_eq!(error.kind, HttpErrorKind::MalformedHeaders);
    }

    #[test]
    fn transfer_encoding_is_rejected() {
        let head = parse_head(
            "POST /submit HTTP/1.1\r\nHost: example.i2p\r\nTransfer-Encoding: chunked\r\n\r\n",
        );
        let error = filter_server_request(&head, "127.0.0.1:8080").expect_err("te");
        assert_eq!(error.kind, HttpErrorKind::SmugglingAmbiguity);
    }

    #[test]
    fn duplicate_content_length_is_rejected() {
        let raw = "POST /submit HTTP/1.1\r\nHost: example.i2p\r\nContent-Length: 5\r\nContent-Length: 5\r\n\r\n";
        // Identical duplicates either fail at the shared parser
        // (conflicting-length rule) or reach the filter and fail
        // closed there; both outcomes are acceptable.
        match parse_request_head(raw.as_bytes(), HttpLimits::defaults()) {
            Err(_) => {}
            Ok(head) => {
                let result = filter_server_request(&head, "127.0.0.1:8080");
                assert!(
                    result.is_err(),
                    "duplicate Content-Length must fail, got {result:?}"
                );
            }
        }
    }

    #[test]
    fn content_length_is_reported() {
        let head =
            parse_head("POST /submit HTTP/1.1\r\nHost: example.i2p\r\nContent-Length: 11\r\n\r\n");
        let filtered = filter_server_request(&head, "127.0.0.1:8080").expect("filters");
        assert_eq!(filtered.content_length, Some(11));
        let text = std::str::from_utf8(&filtered.head_bytes).expect("utf-8");
        assert!(text.contains("content-length: 11\r\n"));
    }

    #[test]
    fn connection_listed_headers_are_dropped() {
        let head = parse_head(
            "GET / HTTP/1.1\r\nHost: example.i2p\r\nConnection: X-Custom\r\nX-Custom: evil\r\n\r\n",
        );
        let filtered = filter_server_request(&head, "127.0.0.1:8080").expect("filters");
        let text = std::str::from_utf8(&filtered.head_bytes).expect("utf-8");
        assert!(!text.to_ascii_lowercase().contains("x-custom"));
    }

    #[test]
    fn bad_local_authority_is_rejected() {
        let head = parse_head("GET / HTTP/1.1\r\nHost: example.i2p\r\n\r\n");
        assert!(filter_server_request(&head, "").is_err());
        assert!(filter_server_request(&head, "127.0.0.1:8080\r\nEvil: 1").is_err());
        assert!(filter_server_request(&head, "user@127.0.0.1:8080").is_err());
    }

    #[test]
    fn response_filters_privacy_headers() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nServer: secret/1.0\r\nVia: 1.1 proxy\r\nConnection: keep-alive\r\nContent-Length: 5\r\n\r\n";
        let filtered = filter_server_response(raw).expect("filters");
        let text = std::str::from_utf8(&filtered).expect("utf-8");
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("content-type: text/html\r\n"));
        assert!(text.contains("content-length: 5\r\n"));
        assert!(!text.to_ascii_lowercase().contains("server:"));
        assert!(!text.to_ascii_lowercase().contains("via:"));
        assert!(!text.contains("keep-alive"));
        assert!(text.contains("connection: close\r\n"));
    }

    #[test]
    fn response_preserves_framing_and_coding() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Encoding: x-i2p-gzip\r\n\r\n";
        let filtered = filter_server_response(raw).expect("filters");
        let text = std::str::from_utf8(&filtered).expect("utf-8");
        assert!(text.contains("transfer-encoding: chunked\r\n"));
        assert!(text.contains("content-encoding: x-i2p-gzip\r\n"));
    }

    #[test]
    fn malformed_response_status_is_rejected() {
        assert!(filter_server_response(b"NOT-HTTP 200 OK\r\n\r\n").is_err());
        assert!(filter_server_response(b"HTTP/1.1 99 OK\r\n\r\n").is_err());
        assert!(filter_server_response(b"HTTP/2 200 OK\r\n\r\n").is_err());
        assert!(filter_server_response(b"HTTP/1.1 200\r\n\r\n").is_err());
    }

    #[test]
    fn presentation_classes_match_exactly() {
        use super::{HttpServerPolicy, PresentationClass, classify_presentation};
        // Ordinary content never matches, including near-miss
        // paths and unrelated query keys.
        for (path, query) in [
            ("/", ""),
            ("/index.html", ""),
            ("/addresshelperish", ""),
            ("/jumpstart", ""),
            ("/Jump/X", ""),
            ("/page", "jumped=1&helper=2"),
            ("/page", "JUMPING=1"),
        ] {
            assert_eq!(
                classify_presentation(path, query),
                PresentationClass::Ordinary,
                "{path}?{query}"
            );
        }
        // Helper class: path family and query key (case-insensitive).
        for (path, query) in [
            ("/addresshelper", ""),
            ("/addresshelper/", ""),
            ("/addresshelper/add", "host=x.i2p"),
            ("/page", "i2paddresshelper=x.i2p"),
            ("/page", "a=1&I2PADDRESSHELPER=x.i2p"),
        ] {
            assert_eq!(
                classify_presentation(path, query),
                PresentationClass::Helper,
                "{path}?{query}"
            );
        }
        // Jump class: path family and query key (case-insensitive).
        for (path, query) in [
            ("/jump", ""),
            ("/jump/", ""),
            ("/jump/x.i2p", ""),
            ("/page", "jump=x.i2p"),
            ("/page", "a=1&JUMP=x.i2p"),
        ] {
            assert_eq!(
                classify_presentation(path, query),
                PresentationClass::Jump,
                "{path}?{query}"
            );
        }
        // Helper wins when both markers are present.
        assert_eq!(
            classify_presentation("/addresshelper/x", "jump=y"),
            PresentationClass::Helper
        );
        assert_eq!(
            classify_presentation("/page", "jump=y&i2paddresshelper=z"),
            PresentationClass::Helper
        );
        // Policy defaults open both gates; closing one refuses
        // only its class.
        let open = HttpServerPolicy::default();
        assert!(open.admits(PresentationClass::Ordinary));
        assert!(open.admits(PresentationClass::Helper));
        assert!(open.admits(PresentationClass::Jump));
        let no_helper = HttpServerPolicy {
            address_helper: false,
            ..HttpServerPolicy::default()
        };
        assert!(no_helper.admits(PresentationClass::Ordinary));
        assert!(!no_helper.admits(PresentationClass::Helper));
        assert!(no_helper.admits(PresentationClass::Jump));
        let no_jump = HttpServerPolicy {
            jump_list: false,
            ..HttpServerPolicy::default()
        };
        assert!(no_jump.admits(PresentationClass::Ordinary));
        assert!(no_jump.admits(PresentationClass::Helper));
        assert!(!no_jump.admits(PresentationClass::Jump));
    }
}
