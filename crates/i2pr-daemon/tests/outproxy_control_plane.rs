//! Plan 342: the outproxy block at the control boundary and in the request
//! paths.
//!
//! Two halves, deliberately in one file because they are one property:
//!
//! - the **configuration** rows ask whether a tunnel can be told to use an
//!   I2P-routed outproxy, and what happens to a partial block, a credential,
//!   or a clearnet entry;
//! - the **request-path** rows ask what the daemon then does with a request
//!   that names a clearnet authority.
//!
//! The request-path rows are black-box over loopback `TcpStream`s: they drive
//! the real `handle_connect` / SOCKS5 executors through the real manager and
//! assert on what the local client receives. They never call a provider
//! directly, because "the provider is reachable" is not the property under
//! test — "the request either carries an outproxy or is refused" is.
//!
//! No row here opens a direct clearnet socket, and none can: the only socket
//! in this file is a loopback listener the test itself binds on `127.0.0.1:0`.

use std::collections::BTreeMap;
use std::sync::Arc;

use i2pr_daemon::i2pcontrol_tunnels::{
    ControlError, build_control_spec, normalize_definition_with_filter_root,
};
use i2pr_service_tunnels::outbound_secret::{NoOutboundSecrets, RouterSecretOwner};

/// A real Base32 destination, built with the service-tunnel crate's encoder.
fn b32(byte: u8) -> String {
    format!(
        "{}.b32.i2p",
        i2pr_service_tunnels::encode_b32_label(&[byte; 32])
    )
}

/// A router-bound secret owner, so a credential row cannot pass against a
/// stub that stores plaintext.
fn router_bound_store() -> Arc<dyn RouterSecretOwner> {
    let mut rng = i2pr_crypto::OsRng;
    let bundle = i2pr_crypto::RouterIdentityBundle::generate(&mut rng).expect("identity bundle");
    Arc::new(
        i2pr_daemon::outbound_secret::RouterBoundOutboundSecrets::from_router_identity(&bundle)
            .expect("router-bound secret owner"),
    )
}

/// The complete seven-field outproxy block on a `connectclient`.
fn block(credential: Option<(&str, &str)>) -> BTreeMap<String, String> {
    let mut options = BTreeMap::new();
    options.insert(
        "target_destination".to_owned(),
        format!("{}.b32.i2p", "a".repeat(52)),
    );
    options.insert("listen_port".to_owned(), "0".to_owned());
    options.insert(
        "proxy_list".to_owned(),
        format!("{},staging.example.i2p", b32(0x5a)),
    );
    options.insert("use_outproxy_plugin".to_owned(), "true".to_owned());
    options.insert("outproxy_type".to_owned(), "http".to_owned());
    options.insert("ssl_proxies".to_owned(), "staging.example.i2p".to_owned());
    match credential {
        None => {
            options.insert("outproxy_auth".to_owned(), "false".to_owned());
            options.insert("outproxy_username".to_owned(), String::new());
            options.insert("outproxy_password".to_owned(), String::new());
        }
        Some((user, password)) => {
            options.insert("outproxy_auth".to_owned(), "true".to_owned());
            options.insert("outproxy_username".to_owned(), user.to_owned());
            options.insert("outproxy_password".to_owned(), password.to_owned());
        }
    }
    options
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[test]
fn the_complete_block_reaches_a_connect_client_spec() {
    let definition = normalize_definition_with_filter_root(
        "outproxy",
        i2pr_i2pcontrol::TunnelType::ConnectClient,
        &block(None),
        false,
        None,
        &NoOutboundSecrets,
    )
    .expect("the complete block normalizes");
    let spec = build_control_spec(&definition).expect("the block maps to a spec");
    let connect = spec
        .connect_options
        .as_ref()
        .expect("connect options are present");
    let outproxy = connect
        .outproxy
        .as_ref()
        .expect("the route policy is present");
    assert_eq!(outproxy.list.len(), 2);
    assert_eq!(outproxy.tunnelled.len(), 1);
    assert!(!outproxy.present_credential);
    // The whole point of the plan's ordering rule: the option surface is only
    // accepted once the route exists, so a spec carrying a policy must be
    // able to name an outproxy.
    let target = i2pr_service_tunnels::OutproxyTarget::parse_authority("example.com:443")
        .expect("a clearnet authority parses");
    let route = outproxy.route(&target);
    assert!(
        route.is_via_outproxy(),
        "a spec carrying a policy must be able to select an outproxy, got {route:?}"
    );
}

#[test]
fn a_partial_block_is_refused_by_name() {
    let full = block(None);
    for key in [
        "proxy_list",
        "use_outproxy_plugin",
        "outproxy_auth",
        "outproxy_username",
        "outproxy_password",
        "outproxy_type",
        "ssl_proxies",
    ] {
        let mut partial = full.clone();
        partial.remove(key);
        let error = normalize_definition_with_filter_root(
            "partial",
            i2pr_i2pcontrol::TunnelType::ConnectClient,
            &partial,
            false,
            None,
            &NoOutboundSecrets,
        )
        .expect_err("a partial block must be refused");
        match error {
            ControlError::OutproxyBlockRejected(reason) => {
                assert!(reason.contains("all-or-none"), "{reason}");
                assert!(
                    reason.contains(key),
                    "{reason} should name the missing {key}"
                );
            }
            other => panic!("unexpected error for missing {key}: {other:?}"),
        }
    }
}

#[test]
fn a_credential_is_sealed_and_a_missing_owner_refuses_the_tunnel() {
    let store = router_bound_store();
    let definition = normalize_definition_with_filter_root(
        "sealed",
        i2pr_i2pcontrol::TunnelType::ConnectClient,
        &block(Some(("operator", "s3cret!"))),
        false,
        None,
        store.as_ref(),
    )
    .expect("normalizes with an installed owner");

    let sealed = definition
        .options
        .get("outproxy_password")
        .expect("the key is present");
    assert_ne!(sealed, "s3cret!");
    assert!(!format!("{definition:?}").contains("s3cret!"));
    // Round-trips through the owner that sealed it, which is what makes the
    // stored form usable at request time rather than merely opaque.
    assert_eq!(
        store
            .open(sealed)
            .expect("the owner opens its own stored form")
            .expose_str()
            .expect("utf-8"),
        "s3cret!"
    );

    // Without an owner the same tunnel is refused, before anything is
    // allocated.
    let error = normalize_definition_with_filter_root(
        "nosecret",
        i2pr_i2pcontrol::TunnelType::ConnectClient,
        &block(Some(("operator", "s3cret!"))),
        false,
        None,
        &NoOutboundSecrets,
    )
    .expect_err("no owner means no credential");
    match error {
        ControlError::OutproxyBlockRejected(reason) => {
            assert!(reason.contains("no outbound secret owner"), "{reason}");
            assert!(reason.contains("was not created"), "{reason}");
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // And a credential-free block needs no owner at all: the fail-closed
    // default must not make the feature unusable without an identity.
    normalize_definition_with_filter_root(
        "nosecret",
        i2pr_i2pcontrol::TunnelType::ConnectClient,
        &block(None),
        false,
        None,
        &NoOutboundSecrets,
    )
    .expect("a credential-free block needs no owner");
}

#[test]
fn clearnet_and_executable_outproxy_entries_are_refused() {
    for entry in [
        "proxy.example.com",
        "127.0.0.1",
        "proxy.example.com:8888",
        "/usr/bin/curl",
    ] {
        let mut options = block(None);
        options.insert("proxy_list".to_owned(), entry.to_owned());
        assert!(
            normalize_definition_with_filter_root(
                "bad",
                i2pr_i2pcontrol::TunnelType::ConnectClient,
                &options,
                false,
                None,
                &NoOutboundSecrets
            )
            .is_err(),
            "{entry} must not reach a definition"
        );
    }
    for value in ["/usr/bin/curl", "sh -c id", "socks6", "HTTP_PLUGIN"] {
        let mut options = block(None);
        options.insert("outproxy_type".to_owned(), value.to_owned());
        assert!(
            normalize_definition_with_filter_root(
                "bad",
                i2pr_i2pcontrol::TunnelType::ConnectClient,
                &options,
                false,
                None,
                &NoOutboundSecrets
            )
            .is_err(),
            "outproxy type {value} must be refused"
        );
    }
}

#[test]
fn a_provider_flag_of_false_is_refused_rather_than_inertly_stored() {
    let mut options = block(None);
    options.insert("use_outproxy_plugin".to_owned(), "false".to_owned());
    let error = normalize_definition_with_filter_root(
        "disabled",
        i2pr_i2pcontrol::TunnelType::ConnectClient,
        &options,
        false,
        None,
        &NoOutboundSecrets,
    )
    .expect_err("UseOutproxyPlugin:false must be refused");
    match error {
        ControlError::OutproxyBlockRejected(reason) => {
            assert!(reason.contains("no provider"), "{reason}");
            assert!(reason.contains("stored and never routed"), "{reason}");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Request paths
//
// The live request-path rows are NOT here.
//
// `handle_connect` and `run_socks5_connection` need a live `ServiceRuntime`
// with an established destination, which is what Plan 342's self-composed
// loopback lane exists to provide. Until that lane lands, a test file could
// only assert on the classifier — which is already covered by
// `i2pr_service_tunnels::outproxy::tests::classify_*` — and a test that
// re-asserts the classifier from outside proves nothing new.
//
// What this file covers instead is the half that can be exercised now and
// that a future lane depends on: that the option surface is all-or-none,
// sealed, bounded, and closed, and that it produces a spec whose policy can
// actually select an outproxy. The request-path guarantee is pinned statically
// by `scripts/check-outproxy-request-path.sh`, which asserts that all three
// request paths classify before they open anything.
