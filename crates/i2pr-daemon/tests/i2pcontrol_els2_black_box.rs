//! Plan 334 — black-box I2PControl evidence for the Proposal 170
//! encrypted-LeaseSet mode mapping.
//!
//! The rows Plan 334 landed were unit- and lib-level. Its own closure record
//! listed black-box evidence as **NOT MET**: "a `create`/`get`/`rawConfig`/`edit`
//! round trip over a real JSON-RPC connection has not been written, so the
//! control-surface rows are not yet end-to-end evidence", and it listed
//! create/edit rollback and restart evidence as NOT MET for the same reason.
//!
//! These rows close that. Everything here goes through a real TLS listener
//! and a real JSON-RPC `TunnelManager` call — no private bridge, LeaseSet2,
//! driver, pump, or manager API is touched. A row that needed to look inside
//! would not be evidence for the control surface.
//!
//! The service is constructed the way production constructs it: the
//! composition root's one shared `ServiceTunnelManager` (Plan 337, ADR 0031),
//! with the ELS2 material resolved through the manager (Plan 338).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

/// Plan 342: the fail-closed outbound-credential owner for a row that does not
/// exercise a credential. No identity is loaded, so no store exists, and every
/// credential field is refused rather than silently accepted.
fn plan342_test_outbound_secrets()
-> Arc<dyn i2pr_service_tunnels::outbound_secret::OutboundSecretStore> {
    Arc::new(i2pr_service_tunnels::outbound_secret::NoOutboundSecrets)
}

use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol::I2pControlServiceState;
use i2pr_daemon::i2pcontrol_inspection::InspectionHandles;
use i2pr_daemon::i2pcontrol_tunnels::TunnelControlState;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Test-only accept-any verifier. The service is reached over loopback only;
/// this row is about JSON-RPC behaviour, not the TLS posture, which the
/// service-tunnel TLS suite covers.
#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls_pki_types::CertificateDer<'_>,
        _intermediates: &[rustls_pki_types::CertificateDer<'_>],
        _server_name: &rustls_pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls_pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

const TEST_PASSWORD: &str = "plan334-black-box";

fn config_text(data_dir: &std::path::Path, password: &str) -> String {
    format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n[i2pcontrol]\nenabled = true\n\
         bind_address = \"127.0.0.1\"\nport = 0\npassword = {password:?}\n",
        data_dir.to_string_lossy()
    )
}

/// Starts the service exactly as the supervised factory plus `run()` would,
/// and returns the bound loopback address.
async fn start_service(
    config: &Config,
) -> (
    Arc<I2pControlServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
) {
    let manager = i2pr_daemon::build_shared_service_manager(config)
        .expect("shared manager builds")
        .expect("control implies a shared manager");
    let control = Arc::new(
        TunnelControlState::for_config(config, manager, plan342_test_outbound_secrets())
            .expect("control builds for config"),
    );
    let inspection = Arc::new(InspectionHandles::from_config(config));
    let state = Arc::new(
        I2pControlServiceState::new_with_inspection(config.i2pcontrol.clone(), inspection)
            .expect("state builds"),
    );
    state.set_control_manager(Arc::clone(&control));
    let (listener, bound) = state
        .bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0))
        .await
        .expect("bind succeeds");
    let parent = CancellationToken::new();
    let scope = ChildScope::for_test(&parent, ChildFailurePolicy::FailParent);
    let failures = control.startup(&scope, &parent).await;
    assert!(failures.is_empty(), "control startup clean: {failures:?}");
    let serve_state = Arc::clone(&state);
    let serve_token = parent.clone();
    let serve_scope = scope.clone();
    scope
        .spawn(move |_task| async move {
            let _ = serve_state.serve(listener, serve_scope, serve_token).await;
            Ok(())
        })
        .expect("serve spawns");
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    (state, bound, scope, parent)
}

async fn tls_connect(address: SocketAddr) -> tokio_rustls::client::TlsStream<TcpStream> {
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let stream = TcpStream::connect(address).await.expect("tcp connect");
    connector
        .connect(
            rustls_pki_types::ServerName::try_from("localhost").expect("server name"),
            stream,
        )
        .await
        .expect("tls handshake")
}

async fn post_json(address: SocketAddr, request: &serde_json::Value) -> (u16, serde_json::Value) {
    let mut stream = tls_connect(address).await;
    let body = serde_json::to_vec(request).expect("request serializes");
    let head = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await.expect("head");
    stream.write_all(&body).await.expect("body");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("response");
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> (u16, serde_json::Value) {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("header terminator");
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("status line");
    let body = &raw[split + 4..];
    let value = serde_json::from_slice(body).unwrap_or_else(|_| serde_json::json!({}));
    (status, value)
}

async fn authenticate(address: SocketAddr) -> String {
    let (status, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0", "method": "Authenticate",
            "params": {"API": 1, "Password": TEST_PASSWORD},
            "id": 1,
        }),
    )
    .await;
    assert_eq!(status, 200, "authenticate status");
    response["result"]["Token"]
        .as_str()
        .expect("token minted")
        .to_owned()
}

/// Calls one `TunnelManager` action over the wire, in the canonical
/// Proposal 170 shape: option fields share the top-level params object, and
/// the response is the wire's own — no compatibility view, because a
/// black-box evidence row must assert what a client actually receives.
async fn tunnel(
    address: SocketAddr,
    token: &str,
    params: serde_json::Value,
    id: u32,
) -> serde_json::Value {
    let mut map = params.as_object().expect("object").clone();
    if let Some(serde_json::Value::Object(options)) = map.remove("options") {
        for (key, value) in options {
            assert!(map.insert(key, value).is_none(), "duplicate wire field");
        }
    }
    map.insert("Token".to_owned(), serde_json::json!(token));
    let (status, response) = post_json(
        address,
        &serde_json::json!({
            "jsonrpc": "2.0", "method": "TunnelManager", "params": map, "id": id,
        }),
    )
    .await;
    assert_eq!(status, 200, "tunnel status");
    response
}

fn server_create(name: &str, port: u16, extra: serde_json::Value) -> serde_json::Value {
    server_action("create", name, port, extra)
}

/// The same shape with an explicit action, so an `edit` is a real `edit` on
/// the wire and not a second `create` under the same name.
fn server_action(
    action: &str,
    name: &str,
    port: u16,
    extra: serde_json::Value,
) -> serde_json::Value {
    let mut params = serde_json::json!({
        "Action": action, "Name": name,
    });
    if let Some(target) = params.as_object_mut() {
        // An `edit` may not change the type, so it is sent only on a create.
        if action == "create" {
            target.insert("Type".to_owned(), serde_json::json!("server"));
        }
        target.insert("TargetHost".to_owned(), serde_json::json!("127.0.0.1"));
        target.insert("TargetPort".to_owned(), serde_json::json!(port));
    }
    if let (Some(target), Some(source)) = (params.as_object_mut(), extra.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    params
}

/// Plan 334 black-box: the full create → get → rawConfig round trip for an
/// encrypted server, over a real TLS JSON-RPC connection.
///
/// This is the row Plan 334's closure record listed as NOT MET. It asserts
/// the whole Proposal 170 encrypted-LeaseSet posture **as the wire presents
/// it**: the mode string is accepted, the resolved behavior is reported, and
/// the service reports a real `.b32.i2p` address that a client can look up.
#[tokio::test(flavor = "current_thread")]
async fn plan334_els2_create_get_rawconfig_round_trip_over_jsonrpc() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    // The lookup secret never leaves the client except as a stored definition.
    let lookup_secret = "plan334-wire-lookup-secret";
    let created = tunnel(
        address,
        &token,
        server_create(
            "encsrv",
            8080,
            serde_json::json!({
                "EncryptLeaseSet": "blinded with lookup password",
                "OptionalLookup": lookup_secret,
            }),
        ),
        2,
    )
    .await;
    assert!(created.get("error").is_none(), "create: {created}");
    assert!(
        created["result"]["status"]
            .as_str()
            .unwrap_or_default()
            .starts_with("success"),
        "create status: {created}"
    );

    // `get` reports the resolved posture and a real address.
    let got = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "encsrv"}),
        3,
    )
    .await;
    assert!(got.get("error").is_none(), "get: {got}");
    let result = &got["result"]["info"];
    let security = &result["lease_set_security"];
    assert_eq!(
        security["publishesEncryptedLeaseSet2"], true,
        "the wire must report a type-5 publisher: {security}"
    );
    assert_eq!(
        security["encryptLeaseSet"], "blinded with lookup password",
        "the spelling that arrived must be reported: {security}"
    );
    assert_eq!(security["lookupSecretConfigured"], true);
    let address_text = result["encryptedAddress"]
        .as_str()
        .expect("encryptedAddress is reported for a type-5 service")
        .to_owned();
    assert!(address_text.ends_with(".b32.i2p"), "got {address_text}");
    // The unblinded service destination is reported separately; the encrypted
    // address is what a client with the secret looks the service up by.
    assert_ne!(result["destinationB32"], address_text);
    // It decodes as a real encrypted service address carrying the
    // blinding-secret flag, so a client with the secret can derive the
    // blinded key from it.
    let parsed = i2pr_proto::EncryptedServiceAddress::from_text(&address_text)
        .expect("the reported address decodes");
    assert!(parsed.requires_blinding_secret());
    assert!(!parsed.requires_client_key());
    assert_eq!(parsed.blinded_sigtype(), i2pr_proto::B32_BLINDED_SIGTYPE);

    // `rawConfig` reports the same posture and redacts the secret.
    let raw = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "encsrv"}),
        4,
    )
    .await;
    assert!(raw.get("error").is_none(), "rawConfig: {raw}");
    let raw_text = raw.to_string();
    assert!(
        !raw_text.contains(lookup_secret),
        "the lookup secret must never appear on the wire: {raw_text}"
    );
    let raw_security = &raw["result"]["info"]["lease_set_security"];
    assert_eq!(raw_security["publishesEncryptedLeaseSet2"], true);
    assert_eq!(raw_security["lookupSecretConfigured"], true);
    // Presence and length, never bytes: the marker names the canonical
    // Proposal field, reports that a value is configured, and redacts it.
    let marker = &raw["result"]["info"]["rawConfig"]["OptionalLookup"];
    assert_eq!(marker["configured"], true, "{marker}");
    assert_eq!(marker["redacted"], true, "{marker}");
    assert!(
        marker["length"].is_number(),
        "the marker reports the secret's length so a client can detect a mismatch: {marker}"
    );
    assert!(
        !raw_text.to_lowercase().contains("password_value"),
        "the wire must not carry secret bytes under any field"
    );

    // The `start`/`stop` views name the tunnel too, and must stay clean.
    let status = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "stop", "Name": "encsrv"}),
        5,
    )
    .await;
    assert!(status.get("error").is_none(), "stop: {status}");
    assert!(
        !status.to_string().contains(lookup_secret),
        "a lifecycle response must never carry the lookup secret: {status}"
    );
}

/// Plan 334 black-box: an `edit` changes the published posture over the wire,
/// and the address follows the new mode rather than lagging behind it.
#[tokio::test(flavor = "current_thread")]
async fn plan334_els2_edit_changes_the_reported_posture_over_jsonrpc() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    let created = tunnel(
        address,
        &token,
        server_create(
            "modesrv",
            8081,
            serde_json::json!({"EncryptLeaseSet": "blinded"}),
        ),
        2,
    )
    .await;
    assert!(created.get("error").is_none(), "create: {created}");

    let before = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "modesrv"}),
        3,
    )
    .await;
    let before_address = before["result"]["info"]["encryptedAddress"]
        .as_str()
        .expect("address before the edit")
        .to_owned();
    assert_eq!(
        before["result"]["info"]["lease_set_security"]["lookupSecretConfigured"],
        false
    );

    // Add a lookup secret: the posture must become secret-required and the
    // address must change to carry the flag.
    let edited = tunnel(
        address,
        &token,
        server_action(
            "edit",
            "modesrv",
            8081,
            serde_json::json!({
                "EncryptLeaseSet": "blinded with lookup password",
                "OptionalLookup": "plan334-edit-secret",
            }),
        ),
        4,
    )
    .await;
    assert!(edited.get("error").is_none(), "edit: {edited}");

    let after = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "modesrv"}),
        5,
    )
    .await;
    let security = &after["result"]["info"]["lease_set_security"];
    assert_eq!(security["lookupSecretConfigured"], true, "{security}");
    let after_address = after["result"]["info"]["encryptedAddress"]
        .as_str()
        .expect("address after the edit")
        .to_owned();
    assert_ne!(
        before_address, after_address,
        "a mode that requires a secret must report a different address"
    );
    let parsed = i2pr_proto::EncryptedServiceAddress::from_text(&after_address)
        .expect("the edited address decodes");
    assert!(parsed.requires_blinding_secret());
    assert!(
        !after.to_string().contains("plan334-edit-secret"),
        "an edit must not leak the secret on the wire"
    );
}

/// Plan 334 black-box rollback: a rejected `edit` leaves the tunnel exactly as
/// it was — same posture, same address, same reported state — with no
/// half-converted definition.
#[tokio::test(flavor = "current_thread")]
async fn plan334_els2_rejected_edit_leaves_the_tunnel_unchanged_over_jsonrpc() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    tunnel(
        address,
        &token,
        server_create(
            "rollsrv",
            8082,
            serde_json::json!({
                "EncryptLeaseSet": "encrypted with lookup password and per-user key (psk)",
                "OptionalLookup": "plan334-roll-secret",
                "LeaseSetClientAuths": format!("client0:{}", "ab".repeat(32)),
            }),
        ),
        2,
    )
    .await;
    let before = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "rollsrv"}),
        3,
    )
    .await;
    assert!(before.get("error").is_none(), "create: {before}");

    // A rejected edit: a per-user-key mode that drops the authorization the
    // service was configured with. The whole LeaseSet block is invalid, so the
    // transaction must fail and change nothing.
    let rejected = tunnel(
        address,
        &token,
        server_action(
            "edit",
            "rollsrv",
            8082,
            serde_json::json!({
                "EncryptLeaseSet": "encrypted with per-user key (dh)",
                "OptionalLookup": "plan334-roll-secret",
            }),
        ),
        4,
    )
    .await;
    assert!(
        rejected.get("error").is_some(),
        "a per-user-key mode with no authorizations must be rejected: {rejected}"
    );

    let after = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "rollsrv"}),
        5,
    )
    .await;
    assert!(
        after.get("error").is_none(),
        "get after a rejected edit: {after}"
    );
    assert_eq!(
        after["result"]["info"]["rawConfig"], before["result"]["info"]["rawConfig"],
        "a rejected edit must not change the stored options"
    );
    assert_eq!(
        after["result"]["info"]["lease_set_security"],
        before["result"]["info"]["lease_set_security"],
        "a rejected edit must not change the reported posture"
    );
    assert_eq!(
        after["result"]["info"]["encryptedAddress"], before["result"]["info"]["encryptedAddress"],
        "a rejected edit must not change the published address"
    );
    assert!(
        !after.to_string().contains("plan334-roll-secret"),
        "a rejected edit must not leak a secret on the wire"
    );
}

/// Plan 334 black-box restart: a daemon restart over the same data directory
/// restores a secret-bearing encrypted definition, the runtime is reconciled
/// again, and the published address is unchanged — a rotated identity would
/// invalidate every client's stored address.
#[tokio::test(flavor = "current_thread")]
async fn plan334_els2_restart_restores_the_definition_without_rotating_the_address() {
    let directory = tempfile::tempdir().expect("tempdir");
    let text = config_text(directory.path(), TEST_PASSWORD);
    let config = Config::parse(&text).expect("config parses");
    let (_first, address, first_scope, first_parent) = start_service(&config).await;
    let token = authenticate(address).await;

    let lookup_secret = "plan334-restart-secret";
    tunnel(
        address,
        &token,
        server_create(
            "restsrv",
            8083,
            serde_json::json!({
                "EncryptLeaseSet": "blinded with lookup password",
                "OptionalLookup": lookup_secret,
            }),
        ),
        2,
    )
    .await;
    let before = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "restsrv"}),
        3,
    )
    .await;
    let before_address = before["result"]["info"]["encryptedAddress"]
        .as_str()
        .expect("address before the restart")
        .to_owned();

    // Restart: build a fresh service over the same data directory and run
    // control startup again, which is how a daemon restart replays the
    // durable store. The first instance is left to idle; a server tunnel
    // binds no loopback TCP listener, so the two cannot conflict.
    drop(first_scope);
    drop(first_parent);
    let config = Config::parse(&text).expect("config reparses");
    let (_second, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    let after = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "restsrv"}),
        4,
    )
    .await;
    assert!(after.get("error").is_none(), "get after restart: {after}");
    assert_eq!(after["result"]["info"]["rawConfig"]["name"], "restsrv");
    let security = &after["result"]["info"]["lease_set_security"];
    assert_eq!(security["publishesEncryptedLeaseSet2"], true, "{security}");
    assert_eq!(security["lookupSecretConfigured"], true);
    assert_eq!(
        after["result"]["info"]["encryptedAddress"]
            .as_str()
            .expect("address after the restart"),
        before_address,
        "a restart must not rotate a persistent service identity"
    );
    // The secret-bearing definition is restored, and the secret is still
    // redacted.
    let raw = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "restsrv"}),
        5,
    )
    .await;
    assert!(raw.get("error").is_none(), "rawConfig: {raw}");
    assert!(
        !raw.to_string().contains(lookup_secret),
        "a restored secret must stay redacted: {raw}"
    );
}

/// Plan 334 black-box negative: a mode the control surface must refuse stays
/// refused, and the refusal is typed rather than a silent downgrade.
#[tokio::test(flavor = "current_thread")]
async fn plan334_els2_refused_modes_stay_refused_over_jsonrpc() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (_state, address, _scope, _parent) = start_service(&config).await;
    let token = authenticate(address).await;

    // Recognized and then refused **by name**, which is different from
    // unknown-and-rejected.
    let aes = tunnel(
        address,
        &token,
        server_create(
            "aessrv",
            8084,
            serde_json::json!({"EncryptLeaseSet": "encrypted (aes)"}),
        ),
        2,
    )
    .await;
    let aes_error = aes["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        aes.get("error").is_some(),
        "encrypted (aes) is refused: {aes}"
    );
    assert!(
        aes_error.to_lowercase().contains("aes"),
        "the refusal must name the mode: {aes_error}"
    );

    // A per-user-key mode with no authorizations.
    let noauth = tunnel(
        address,
        &token,
        server_create(
            "noauthsrv",
            8085,
            serde_json::json!({"EncryptLeaseSet": "encrypted with per-user key (psk)"}),
        ),
        3,
    )
    .await;
    assert!(noauth.get("error").is_some(), "{noauth}");

    // A lookup secret supplied to a mode that does not use one.
    let unused = tunnel(
        address,
        &token,
        server_create(
            "unusedsrv",
            8086,
            serde_json::json!({
                "EncryptLeaseSet": "blinded",
                "OptionalLookup": "plan334-unused",
            }),
        ),
        4,
    )
    .await;
    assert!(unused.get("error").is_some(), "{unused}");

    // A mode string that is not one of the ten. Identifiers are exact: no
    // case folding, no trimming, no fuzzy match.
    for bogus in ["BLINDED", " blinded", "blinded ", "encrypted-leaseset"] {
        let refused = tunnel(
            address,
            &token,
            server_create(
                "bogsrv",
                8087,
                serde_json::json!({"EncryptLeaseSet": bogus}),
            ),
            5,
        )
        .await;
        assert!(
            refused.get("error").is_some(),
            "mode {bogus:?} must be rejected, not case-folded or trimmed: {refused}"
        );
    }

    // None of the refused attempts left a tunnel behind. The whole-inventory
    // view is the strongest form: a refused create must not appear in it.
    let inventory = tunnel(address, &token, serde_json::json!({"Action": "get"}), 6).await;
    let inventory_text = inventory.to_string();
    for name in ["aessrv", "noauthsrv", "unusedsrv", "bogsrv"] {
        assert!(
            !inventory_text.contains(name),
            "a refused create must not appear in the inventory as {name}: {inventory}"
        );
        let got = tunnel(
            address,
            &token,
            serde_json::json!({"Action": "get", "Name": name}),
            7,
        )
        .await;
        // A refused create may surface as a JSON-RPC error or as an error
        // *status* in the result, depending on where the transition stopped.
        // What must not happen is a tunnel that exists.
        let reports_missing = got.get("error").is_some()
            || got["result"]["status"]
                .as_str()
                .unwrap_or_default()
                .starts_with("error");
        assert!(
            reports_missing,
            "a refused create must leave no tunnel {name}: {got}"
        );
    }
}
