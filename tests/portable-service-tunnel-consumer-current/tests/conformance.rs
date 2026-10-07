//! Plan 379 WP C — external-consumer conformance against the current revision.
//!
//! This fixture is a **supplement** to `tests/portable-service-tunnel-consumer/`,
//! not a replacement for it. The Plan 351 fixture stays pinned to the Plan 350
//! revision `fa082497…` because it is the evidence that the portable boundary
//! held *before* the `i2pr-proto` edge existed. Repointing it would destroy the
//! comparison. This one is pinned to the Plan 379 head, so it exercises the same
//! policy/filter matrix against today's package **plus** the one workspace edge
//! Plan 359 added.
//!
//! The eight Plan 351 tests below are carried forward byte-for-byte. They are
//! duplicated rather than shared deliberately: a shared `include!` would couple
//! the two fixtures' sources, and the whole point of the historical one is that
//! it is a snapshot of what an external consumer could do at a fixed revision.
//! Divergence between the copies is a real signal -- it means the current public
//! surface no longer matches what Plan 351 proved.
//!
//! What is genuinely new here is `encrypted_service_destination_is_usable_from
//! _outside_the_workspace` and its two neighbours. The permitted `i2pr-proto`
//! edge is what makes `DestinationRef::EncryptedService` reachable, and until
//! now no external consumer had ever compiled against it.

use std::collections::BTreeMap;

use i2pr_service_tunnels::http::{
    HttpLimits, HttpServerPolicy, filter_server_request, filter_server_request_with_policy,
    parse_request_head, parse_request_target, rewrite_headers,
};
use i2pr_service_tunnels::irc::{
    FilterOutcome, IrcClientOptions, IrcLimits, IrcLineParser, LineDirection, LineParserOutcome,
    PrivacySubstitutions, ReasonRewritePolicy, classify_core, decide_client_to_server,
};
use i2pr_service_tunnels::socks5::{RequestOutcome, RequestParser, Socks5ErrorKind, Socks5Limits};
use i2pr_service_tunnels::{
    DestinationGroupId, DestinationGroupKey, DestinationPolicy, DestinationRef, DiffClass,
    HttpClientOptions, IdlePolicy, LocalListenerSpec, ServerAccessPolicy,
    ServerConnectionRateLimiter, ServerConnectionRateLimits, ServerTarget, ServiceDiff,
    ServiceTimeouts, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec,
    StaticAliasTable, TunnelShaping, diff_sets,
};

fn client_spec(id: &str, policy: DestinationPolicy) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("bounded client id"),
        kind: ServiceTunnelKind::GenericClient,
        enabled: false,
        listener: Some(LocalListenerSpec::parse("127.0.0.1".parse().unwrap(), 0).unwrap()),
        target: None,
        targets: Vec::new(),
        destination: Some(DestinationRef::parse("example.i2p").unwrap()),
        policy,
        inbound_port: None,
        max_connections: 16,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: IdlePolicy::disabled(),
        access: ServerAccessPolicy::default(),
        unique_local_address: false,
        multihoming: false,
        reply_bundling: false,
        use_ssl: false,
        http_policy: Default::default(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

fn server_spec(id: &str, policy: DestinationPolicy) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: ServiceTunnelId::parse(id).expect("bounded server id"),
        kind: ServiceTunnelKind::GenericServer,
        enabled: false,
        listener: None,
        target: Some(ServerTarget::parse("127.0.0.1:8080").unwrap()),
        targets: Vec::new(),
        destination: None,
        policy,
        inbound_port: Some(80),
        max_connections: 16,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: ServiceTimeouts::defaults(),
        shaping: TunnelShaping::balanced(),
        streaming_interactive: false,
        idle: IdlePolicy::disabled(),
        access: ServerAccessPolicy::default(),
        unique_local_address: false,
        multihoming: false,
        reply_bundling: false,
        use_ssl: false,
        http_policy: Default::default(),
        http_options: None,
        socks5_options: None,
        irc_options: None,
        connect_options: None,
        streamr_options: None,
    }
}

#[test]
fn generic_client_and_server_validate_through_public_values() {
    let client = client_spec("client", DestinationPolicy::Dedicated);
    let server = server_spec("server", DestinationPolicy::Dedicated);
    assert!(client.validate().is_ok());
    assert!(server.validate().is_ok());
    let set = ServiceTunnelSet {
        tunnels: vec![client.clone(), server],
    };
    assert!(set.validate().is_ok());
    assert_eq!(client.id.as_str(), "client");

    let invalid_id = "x".repeat(65);
    assert!(ServiceTunnelId::parse(&invalid_id).is_err());
    assert!(ServerTarget::parse("192.0.2.8:8080").is_err());
}

#[test]
fn explicit_destination_groups_preserve_dedicated_and_shared_identity_domains() {
    let dedicated_a = client_spec("dedicated-a", DestinationPolicy::Dedicated);
    let dedicated_b = client_spec("dedicated-b", DestinationPolicy::Dedicated);
    assert_ne!(
        dedicated_a.policy.group_key(&dedicated_a.id),
        dedicated_b.policy.group_key(&dedicated_b.id)
    );

    let group = DestinationGroupId::parse("application").unwrap();
    let client = client_spec(
        "shared-client",
        DestinationPolicy::SharedGroup(group.clone()),
    );
    let server = server_spec("shared-server", DestinationPolicy::SharedGroup(group));
    let set = ServiceTunnelSet {
        tunnels: vec![client.clone(), server.clone()],
    };
    assert!(set.validate().is_ok());
    assert_eq!(
        client.policy.group_key(&client.id),
        server.policy.group_key(&server.id)
    );
    assert!(matches!(
        client.policy.group_key(&client.id),
        DestinationGroupKey::Explicit(_)
    ));
}

// The adapter's identity type can only be constructed from its authenticated
// I2P transport event. Local socket addresses and display names have no API here.
struct AuthenticatedPeer([u8; 32]);

fn admit_authenticated_peer(
    policy: &ServerAccessPolicy,
    limiter: &mut ServerConnectionRateLimiter,
    peer: &AuthenticatedPeer,
    monotonic_ms: u64,
) -> bool {
    policy.allows(&peer.0) && limiter.admit(peer.0, monotonic_ms)
}

#[test]
fn access_and_rate_policy_use_authenticated_hash_and_adapter_clock() {
    let peer = AuthenticatedPeer([7; 32]);
    let denied = AuthenticatedPeer([9; 32]);
    let policy = ServerAccessPolicy {
        allow: vec![peer.0],
        deny: vec![denied.0],
        connection_rates: ServerConnectionRateLimits::default(),
    };
    let mut limiter = ServerConnectionRateLimiter::new(ServerConnectionRateLimits {
        client_per_minute: 1,
        ..Default::default()
    });
    assert!(admit_authenticated_peer(&policy, &mut limiter, &peer, 100));
    assert!(!admit_authenticated_peer(&policy, &mut limiter, &peer, 101));
    assert!(!admit_authenticated_peer(
        &policy,
        &mut limiter,
        &denied,
        101
    ));
    assert_eq!(limiter.tracked_peers(), 1);
}

#[test]
fn http_client_privacy_and_server_filters_are_reused() {
    let raw = b"GET http://example.i2p/a HTTP/1.1\r\nHost: example.i2p\r\nReferer: https://local.invalid/\r\nX-Forwarded-For: 192.0.2.1\r\nUser-Agent: custom\r\n\r\n";
    let head = parse_request_head(raw, HttpLimits::defaults()).unwrap();
    let target = parse_request_target(&head.line.target).unwrap();
    let rewritten = rewrite_headers(
        &head.headers,
        &target,
        &HttpClientOptions::default().privacy,
    );
    assert!(!rewritten.iter().any(|h| h.name_str() == "referer"));
    assert!(!rewritten.iter().any(|h| h.name_str() == "x-forwarded-for"));
    assert!(
        rewritten
            .iter()
            .any(|h| { h.name_str() == "user-agent" && h.value == "MYOB/6.66 (AN/ON)" })
    );

    let server_raw = b"GET /index HTTP/1.1\r\nHost: public.example\r\nForwarded: for=192.0.2.1\r\nConnection: keep-alive\r\n\r\n";
    let server_head = parse_request_head(server_raw, HttpLimits::defaults()).unwrap();
    let filtered = filter_server_request_with_policy(
        &server_head,
        "127.0.0.1:8080",
        &HttpServerPolicy::default(),
    )
    .unwrap();
    let output = String::from_utf8(filtered.head_bytes)
        .unwrap()
        .to_ascii_lowercase();
    assert!(output.contains("host: 127.0.0.1:8080\r\n"));
    assert!(!output.contains("forwarded:"));
    assert!(filter_server_request(&server_head, "127.0.0.1:8080").is_ok());
}

#[test]
fn socks_connect_parser_and_irc_privacy_filter_are_publicly_usable() {
    let domain = b"example.i2p";
    let mut request = vec![5, 1, 0, 3, domain.len() as u8];
    request.extend_from_slice(domain);
    request.extend_from_slice(&[1, 187]);
    let parsed = RequestParser::new()
        .advance(&request, Socks5Limits::defaults())
        .unwrap()
        .unwrap();
    assert!(
        matches!(parsed, RequestOutcome::ReadyToConnect { destination, .. }
        if destination.host == "example.i2p" && destination.port == 443)
    );

    let parsed = classify_core(b"USER alice localhost localserver :real name").unwrap();
    let outcome = decide_client_to_server(
        &parsed,
        ReasonRewritePolicy::Keep,
        &PrivacySubstitutions::default(),
        IrcClientOptions::default().user_realname_max_bytes,
    );
    assert!(matches!(outcome, FilterOutcome::Rewrite { line }
        if String::from_utf8_lossy(&line).contains("USER alice i2p localhost :real name")));

    let mut line_parser = IrcLineParser::new(IrcLimits::defaults(), IrcClientOptions::default());
    assert!(matches!(
        line_parser.advance(b"NICK user\r\n", LineDirection::ClientToServer),
        LineParserOutcome::Complete {
            outcome: FilterOutcome::Allow,
            ..
        }
    ));
}

#[test]
fn malformed_and_max_plus_one_inputs_keep_typed_bounded_failures() {
    assert!(parse_request_head(b"G\r\n\r\n", HttpLimits::defaults()).is_err());

    let mut limits = Socks5Limits::defaults();
    limits.retained_buffer_max_bytes = 4;
    let err = RequestParser::new()
        .advance(&[5, 1, 0, 3, 1], limits)
        .unwrap_err();
    assert_eq!(err.kind, Socks5ErrorKind::BufferCeilingExceeded);
}

#[test]
fn generation_diff_is_deterministic_and_adapter_lifecycle_is_explicit() {
    let old = client_spec("alpha", DestinationPolicy::Dedicated);
    let mut changed = old.clone();
    changed.max_connections = 17;
    let diffs = diff_sets(std::slice::from_ref(&old), std::slice::from_ref(&changed));
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].class, DiffClass::MutableInPlace);

    let mut adapter_state = BTreeMap::new();
    adapter_state.insert(old.id.as_str().to_owned(), "running");
    apply_lifecycle(&mut adapter_state, &diffs);
    assert_eq!(adapter_state.get("alpha"), Some(&"running"));

    let removed = diff_sets(std::slice::from_ref(&changed), &[]);
    assert_eq!(removed[0].class, DiffClass::Remove);
    apply_lifecycle(&mut adapter_state, &removed);
    assert!(adapter_state.is_empty());
}

fn apply_lifecycle(state: &mut BTreeMap<String, &'static str>, diffs: &[ServiceDiff]) {
    for diff in diffs {
        match diff.class {
            DiffClass::Add | DiffClass::ReplaceListener | DiffClass::ReplaceDestination => {
                state.insert(diff.id.clone(), "running");
            }
            DiffClass::Remove => {
                state.remove(&diff.id);
            }
            DiffClass::MutableInPlace | DiffClass::Unchanged => {}
        }
    }
}

#[test]
fn failed_adapter_start_is_visible_and_does_not_change_policy_generation() {
    let policy_generation = 4_u64;
    let mut adapter_state = BTreeMap::new();
    let start = Result::<(), &'static str>::Err("bind failed");
    assert_eq!(start, Err("bind failed"));
    assert_eq!(policy_generation, 4);
    assert!(adapter_state.is_empty());

    // Restart derives a fresh adapter instance from the still-valid public spec.
    let spec = client_spec("restart", DestinationPolicy::Dedicated);
    assert!(spec.validate().is_ok());
    adapter_state.insert(spec.id.as_str().to_owned(), "running");
    assert_eq!(adapter_state.get("restart"), Some(&"running"));
}

// ---------------------------------------------------------------------------
// Plan 379 WP C -- everything below is new relative to the Plan 351 fixture.
// ---------------------------------------------------------------------------

/// Valid narrow (56-character) b33 encrypted-service address.
///
/// Generated with `EncryptedServiceAddress::new(7, 11, [0x5a; 32], false, false)`
/// and embedded as a **literal on purpose**. The fixture depends on
/// `i2pr-service-tunnels` alone; it does not name `i2pr-proto`, so it cannot
/// construct an address through the permitted edge and then quietly depend on
/// that edge for the *assertions* too. A literal proves the whole path --
/// parse, validate, expose -- runs on data this crate never had to build.
const B33_NARROW: &str = "epldews2ljnfuws2ljnfuws2ljnfuws2ljnfuws2ljnfuws2ljnfuws2.b32.i2p";

/// Valid wide (60-character) b33 address, both unblinded signing key types.
const B33_WIDE: &str = "b2j2ocyaljnfuws2ljnfuws2ljnfuws2ljnfuws2ljnfuws2ljnfuws2ljna.b32.i2p";
const B33_NARROW_RED25519: &str =
    "epndews2ljnfuws2ljnfuws2ljnfuws2ljnfuws2ljnfuws2ljnfuws2.b32.i2p";

/// A 52-character Base32 destination hash -- the ordinary kind, for contrast.
/// `i2pr_proto::base32_encode(&[0x62; 32])`, verified to decode back to 32 bytes.
const B32_ORDINARY: &str =
    "mjrgeytcmjrgeytcmjrgeytcmjrgeytcmjrgeytcmjrgeytcmjra.b32.i2p";

#[test]
fn encrypted_service_destination_is_usable_from_outside_the_workspace() {
    // The permitted `i2pr-proto` edge makes this variant reachable at all.
    for spelling in [B33_NARROW, B33_WIDE, B33_NARROW_RED25519] {
        let parsed = DestinationRef::parse(spelling).expect("valid b33 parses");
        let address = parsed
            .encrypted_service()
            .expect("a b33 parses to the encrypted-service variant");
        // The address is a validated, structured value -- not an opaque string,
        // and not a destination hash.
        // Both documented unblinded signing types (7 = Ed25519, 11 = Red25519)
        // are accepted, over a 32-byte key.
        assert_eq!(address.public_key().len(), 32);
        assert_eq!(address.blinded_sigtype(), 11);
        assert!(matches!(address.unblinded_sigtype(), 7 | 11));
        assert!(!address.requires_blinding_secret());
        assert!(!address.requires_client_key());
        // Canonical spelling round-trips, so an adapter can persist and reload
        // the reference without re-deriving it.
        assert_eq!(parsed.canonical_string(), spelling);
        assert_eq!(parsed.as_str(), spelling.trim_end_matches(".b32.i2p"));
    }
}

#[test]
fn an_encrypted_service_address_is_not_reported_as_a_malformed_base32_label() {
    // The precise misdiagnosis the `i2pr-proto` edge exists to prevent. Without
    // the b33 branch ahead of the 52-character Base32 branch, a valid 56- or
    // 60-character address would be rejected with "Base32 label must be exactly
    // 52 characters" -- telling an adapter its address is malformed when it is a
    // different kind of value entirely.
    let ordinary = DestinationRef::parse(B32_ORDINARY).expect("ordinary b32");
    assert!(ordinary.encrypted_service().is_none());

    let parsed = DestinationRef::parse(B33_NARROW).expect("a valid b33 is not a Base32 length error");
    assert!(parsed.encrypted_service().is_some());

    // And a b33-*shaped* value that genuinely fails validation must be reported
    // as an encrypted-service problem, not as a Base32 length problem. The
    // corrupted body keeps its 56-character length so it still dispatches to the
    // encrypted branch; it fails on content, not on shape.
    let body = B33_NARROW.trim_end_matches(".b32.i2p");
    let last = body.chars().last().expect("non-empty body");
    let corrupted = format!(
        "{}{}.b32.i2p",
        &body[..body.len() - 1],
        if last == 'a' { 'b' } else { 'a' }
    );
    let rendered = format!(
        "{:?}",
        DestinationRef::parse(&corrupted).expect_err("a corrupted b33 must be rejected")
    );
    assert!(
        rendered.contains("encrypted-service"),
        "a b33-shaped failure must name the encrypted-service policy: {rendered}"
    );
    assert!(
        !rendered.contains("exactly 52 characters"),
        "a b33 must never be misdiagnosed as a Base32 length error: {rendered}"
    );
}

#[test]
fn an_encrypted_service_target_is_contained_outside_static_aliases() {
    // Plan 351 Gate 1. The containment rule is per-service; an alias is global.
    // If an alias could name a b33, a service without `delay_open` would reach
    // the encrypted path by naming the alias instead of the address, and the
    // rule would be enforced on the *spelling* rather than on what the service
    // actually resolves to.
    let mut aliases = StaticAliasTable::new();
    let encrypted = DestinationRef::parse(B33_NARROW).expect("valid b33");

    let err = aliases
        .insert("shared.i2p", encrypted)
        .expect_err("an encrypted-service target must not be aliasable");
    assert!(
        format!("{err:?}").contains("encrypted-service"),
        "the refusal must name the reason it exists: {err:?}"
    );

    // The ordinary kinds still work, so the containment is narrow and not a
    // blanket rejection of the table.
    aliases
        .insert("ordinary.i2p", DestinationRef::parse(B32_ORDINARY).unwrap())
        .expect("a base32 target is aliasable");
    aliases
        .insert(
            "alias-to-alias.i2p",
            DestinationRef::parse("ordinary.i2p").unwrap(),
        )
        .expect("an alias may point at another alias");
    assert_eq!(aliases.len(), 2);
    // The table stores the reference as inserted and does not resolve it.
    // Resolution is the adapter's job, and doing it here would hide exactly the
    // substitution the containment rule exists to forbid.
    assert_eq!(
        aliases.get("alias-to-alias.i2p"),
        Some(&DestinationRef::StaticAlias("ordinary.i2p".to_owned()))
    );
    assert_eq!(
        aliases.get("ordinary.i2p"),
        Some(&DestinationRef::parse(B32_ORDINARY).unwrap())
    );
}
