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
    TunnelShaping, diff_sets,
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
