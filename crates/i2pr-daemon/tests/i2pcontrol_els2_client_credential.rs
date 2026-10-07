//! Plan 380 — black-box I2PControl evidence for the ELS2 **consumer client
//! credential**.
//!
//! Plan 380's production change is that a consumer of a `.b33` which declares
//! `B32_FLAG_REQUIRES_CLIENT_KEY` can now supply the PSK or DH key the service
//! published for it, instead of failing with "no lookup produced a record". The
//! value is the operator's, it arrives as one control option, and it is the only
//! client key material the router will ever hold.
//!
//! Every row here goes through a real TLS listener and a real JSON-RPC
//! `TunnelManager` call. No private bridge, control-plane, or manager API is
//! touched except where a row is explicitly about the manager's registry, which
//! is stated in that row's name.
//!
//! # What these rows do and do not establish
//!
//! They establish the **control surface**: the option's grammar, its pairing
//! rule, that the stored form is ciphertext, that the plaintext never crosses
//! the wire, and that a restart recovers the credential while a data directory
//! copied without the router secret cannot. They establish that a consumer with
//! the credential resolves and one without it does not — see
//! `encrypted_service_consumer_wiring.rs` for the resolution half.
//!
//! They do not establish a live cross-router `.b33` fetch. That is Plan 374's,
//! and it is still blocked for want of a driver; nothing here is offered in its
//! place.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::config::Config;
use i2pr_daemon::i2pcontrol::I2pControlServiceState;
use i2pr_daemon::i2pcontrol_inspection::InspectionHandles;
use i2pr_daemon::i2pcontrol_tunnels::TunnelControlState;
use i2pr_daemon::outbound_secret::RouterBoundOutboundSecrets;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use i2pr_service_tunnels::outbound_secret::RouterSecretOwner;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const TEST_PASSWORD: &str = "plan380-black-box";

/// The secret owner a row uses to seal and to open, built over a generated
/// router identity.
///
/// The identity is **not** in the data directory. That is deliberate for the
/// restart rows: a store derived from an identity the data directory does not
/// hold is exactly the "copied data directory" shape, so the recovery row needs
/// no filesystem surgery to prove the property — it holds the same bundle across
/// a restart to prove recovery, and drops it to prove refusal.
fn plan380_secret_owner() -> Arc<dyn RouterSecretOwner> {
    Arc::new(RouterBoundOutboundSecrets::from_router_identity(&plan380_identity()).expect("owner"))
}

/// A fresh router identity per call, so two rows cannot share a key by accident.
fn plan380_identity() -> RouterIdentityBundle {
    let mut rng = rand_core::OsRng;
    RouterIdentityBundle::generate(&mut rng).expect("identity bundle")
}

/// A real `.b33.i2p` that demands a client key, built the way a publisher builds one.
fn plan380_authorized_b33() -> String {
    use i2pr_netdb::BlindingIdentity;
    use i2pr_proto::{B32_BLINDED_SIGTYPE, B32_UNBLINDED_SIGTYPE_RED25519, SigningKeyType};
    let mut rng = rand_core::OsRng;
    let private: i2pr_crypto::red25519::Red25519PrivateScalar =
        i2pr_crypto::red25519::generate_private(&mut rng).expect("key");
    let public = i2pr_crypto::red25519::derive_public_key(&private);
    let identity = BlindingIdentity::new(
        public,
        SigningKeyType::RedDsaSha512Ed25519,
        Some("plan380-wire-lookup-secret"),
    )
    .expect("blinding identity");
    let schedule =
        i2pr_netdb::BlindingSchedule::new(identity, i2pr_netdb::BlindingScheduleConfig::default());
    let publisher = i2pr_client::encrypted_leaseset::EncryptedLeaseSet2Publisher::new(&schedule);
    let address = publisher
        .authorized_address(true)
        .expect("authorized address");
    assert_eq!(address.blinded_sigtype(), B32_BLINDED_SIGTYPE);
    assert_eq!(address.unblinded_sigtype(), B32_UNBLINDED_SIGTYPE_RED25519);
    address.to_text().expect("address text")
}

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

fn config_text(data_dir: &std::path::Path, password: &str) -> String {
    format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n[i2pcontrol]\nenabled = true\n\
         bind_address = \"127.0.0.1\"\nport = 0\npassword = {password:?}\n",
        data_dir.to_string_lossy()
    )
}

/// Starts the service with a **real** secret owner, so a sealing row cannot pass
/// against the fail-closed stub.
async fn start_service(
    config: &Config,
    owner: Arc<dyn RouterSecretOwner>,
) -> (
    Arc<I2pControlServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
    Arc<i2pr_daemon::service_tunnels::ServiceTunnelManager>,
) {
    let manager = i2pr_daemon::build_shared_service_manager(config)
        .expect("shared manager builds")
        .expect("control implies a shared manager");
    let control = Arc::new(
        TunnelControlState::for_config(config, Arc::clone(&manager), owner)
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
    (state, bound, scope, parent, manager)
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

async fn tunnel(
    address: SocketAddr,
    token: &str,
    params: serde_json::Value,
    id: u32,
) -> serde_json::Value {
    let mut map = params.as_object().expect("object").clone();
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

/// A client tunnel: a `.b33` target, `delay_open` so the destination resolves
/// at request time rather than at create time, and the credential under test.
fn client_create(name: &str, target: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut params = serde_json::json!({
        "Action": "create", "Type": "client", "Name": name,
        "TargetDestination": target,
        "DelayOpen": true,
    });
    if let (Some(target), Some(source)) = (params.as_object_mut(), extra.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    params
}

/// The credential as it crosses the wire: Proposal 170's `CustomOptions` carrying i2pr's typed
/// extension namespace. There is no Proposal field for a consumer credential, so this is the only
/// shape that can deliver one — see `i2pr_i2pcontrol::extension_options`.
fn credential_extension(credential: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "i2pr": {"LeasesetClientCredential": credential},
    })
}

fn ok(response: &serde_json::Value) -> bool {
    response.get("error").is_none()
        && response["result"]["status"]
            .as_str()
            .unwrap_or_default()
            .starts_with("success")
}

/// The whole client credential row over the wire: the option is accepted, the
/// plaintext is stored sealed, and no byte of it ever crosses the connection.
#[tokio::test(flavor = "current_thread")]
async fn plan380_a_credential_creates_and_never_appears_on_the_wire() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let owner = plan380_secret_owner();
    let (_state, address, _scope, _parent, _manager) = start_service(&config, owner).await;
    let token = authenticate(address).await;
    let target = plan380_authorized_b33();
    let credential = format!("psk:{}", "7c".repeat(32));

    let created = tunnel(
        address,
        &token,
        client_create(
            "consrv",
            &target,
            serde_json::json!({"CustomOptions": credential_extension(&serde_json::json!(credential))}),
        ),
        2,
    )
    .await;
    assert!(ok(&created), "create: {created}");

    // `get` answers the operator's question — is one configured — with a boolean.
    let got = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "get", "Name": "consrv"}),
        3,
    )
    .await;
    assert!(ok(&got), "get: {got}");
    let security = &got["result"]["info"]["lease_set_security"];
    assert_eq!(
        security["clientCredentialConfigured"], true,
        "the wire must report presence: {security}"
    );

    // And `rawConfig` redacts it rather than echoing it.
    let raw_text = got.to_string();
    assert!(
        !raw_text.contains(&credential),
        "the credential must never appear on the wire"
    );
    assert!(
        !raw_text.contains("7c7c7c7c"),
        "no run of the credential's bytes may appear either"
    );
    let raw_config = &got["result"]["info"]["rawConfig"];
    assert!(
        raw_config.get("leaseset_client_credential").is_none(),
        "the credential must not appear in rawConfig at all: {raw_config}"
    );
}

/// The stored definition carries ciphertext, never the option value.
///
/// This row reaches into the control state on purpose: the claim is about what
/// is *persisted*, and the persisted thing is a definition. It is the only row
/// in this file that is not a wire row.
#[tokio::test(flavor = "current_thread")]
async fn plan380_the_stored_definition_holds_a_sealed_credential() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let owner = plan380_secret_owner();
    let (_state, address, _scope, _parent, manager) =
        start_service(&config, Arc::clone(&owner)).await;
    let token = authenticate(address).await;
    let target = plan380_authorized_b33();
    let credential = format!("dh:{}", "3f".repeat(32));

    assert!(
        ok(&tunnel(
            address,
            &token,
            client_create(
                "sealedsrv",
                &target,
                serde_json::json!({"CustomOptions": credential_extension(&serde_json::json!(credential))}),
            ),
            2,
        )
        .await),
        "create must succeed"
    );

    let installed = manager
        .encrypted_target_credential("sealedsrv")
        .expect("the reconciliation installed the credential");
    let stored = installed.sealed_form();
    assert!(
        stored.starts_with("$i2pr1e$"),
        "the stored form must carry the consumer-credential marker: {stored}"
    );
    assert!(
        !stored.contains(&credential) && !stored.contains("3f3f3f3f"),
        "the stored form must be ciphertext"
    );
    // And it is usable: the manager's value opens to exactly what was supplied.
    assert_eq!(
        installed.open().expect("opens").to_option_value(),
        credential
    );
}

/// The pairing rule: a credential on an ordinary `.b32` target is refused, and
/// the reason names the rule rather than the value.
#[tokio::test(flavor = "current_thread")]
async fn plan380_a_credential_on_an_ordinary_target_is_refused() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (_state, address, _scope, _parent, _manager) =
        start_service(&config, plan380_secret_owner()).await;
    let token = authenticate(address).await;
    let credential = format!("psk:{}", "5a".repeat(32));

    let refused = tunnel(
        address,
        &token,
        client_create(
            "badsrv",
            // A syntactically valid ordinary destination, not a `.b33`.
            &format!("{}.b32.i2p", "a".repeat(52)),
            serde_json::json!({"CustomOptions": credential_extension(&serde_json::json!(credential))}),
        ),
        2,
    )
    .await;
    assert!(
        !ok(&refused),
        "a credential with no encrypted target must be refused: {refused}"
    );
    let reason = refused.to_string();
    assert!(
        reason.contains("leaseset_client_credential requires target_destination"),
        "the refusal must name the rule: {reason}"
    );
    assert!(
        !reason.contains(&credential),
        "the refusal must not echo the credential: {reason}"
    );
}

/// Grammar: an over-long value and an unknown scheme are both refused, by shape.
#[tokio::test(flavor = "current_thread")]
async fn plan380_a_malformed_credential_is_refused_by_shape() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (_state, address, _scope, _parent, _manager) =
        start_service(&config, plan380_secret_owner()).await;
    let token = authenticate(address).await;
    let target = plan380_authorized_b33();

    let cases: [(&str, serde_json::Value); 5] = [
        (
            "over bound",
            serde_json::json!(format!("psk:{}", "00".repeat(200))),
        ),
        ("no scheme", serde_json::json!("00".repeat(32))),
        (
            "unknown scheme",
            serde_json::json!(format!("rsa:{}", "00".repeat(32))),
        ),
        (
            "short key",
            serde_json::json!(format!("psk:{}", "00".repeat(31))),
        ),
        (
            "uppercase key",
            serde_json::json!(format!("psk:{}", "AA".repeat(32))),
        ),
    ];
    for (index, (label, credential)) in cases.iter().enumerate() {
        let refused = tunnel(
            address,
            &token,
            client_create(
                &format!("malformed{index}"),
                &target,
                serde_json::json!({"CustomOptions": credential_extension(&serde_json::json!(credential))}),
            ),
            2 + u32::try_from(index).expect("index fits"),
        )
        .await;
        assert!(!ok(&refused), "{label} must be refused: {refused}");
        assert!(
            !refused
                .to_string()
                .contains(credential.as_str().unwrap_or_default()),
            "{label}: the refusal must not echo the value"
        );
    }
}

/// A credential is refused outright when the router has no secret owner.
///
/// `NoOutboundSecrets` is the composition root's fallback when the router
/// identity cannot be loaded. Every credential field is then refused rather
/// than accepted and silently unusable — the same rule Plan 342 states for the
/// outproxy credential.
#[tokio::test(flavor = "current_thread")]
async fn plan380_a_credential_needs_a_secret_owner_to_exist() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (_state, address, _scope, _parent, _manager) = start_service(
        &config,
        Arc::new(i2pr_service_tunnels::outbound_secret::NoOutboundSecrets),
    )
    .await;
    let token = authenticate(address).await;
    let target = plan380_authorized_b33();

    let refused = tunnel(
        address,
        &token,
        client_create(
            "noowner",
            &target,
            serde_json::json!({
                "CustomOptions": credential_extension(&serde_json::json!(format!("psk:{}", "11".repeat(32))))
            }),
        ),
        2,
    )
    .await;
    assert!(
        !ok(&refused),
        "with no secret owner a credential must be refused: {refused}"
    );
}

/// Restart: the credential survives a fresh control state over the same data
/// directory, and the reconciliation reinstalls it.
#[tokio::test(flavor = "current_thread")]
async fn plan380_a_credential_survives_a_restart_over_the_same_data_directory() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let owner = plan380_secret_owner();
    let credential = format!("psk:{}", "6e".repeat(32));

    let stored_form = {
        let (_state, address, _scope, _parent, manager) =
            start_service(&config, Arc::clone(&owner)).await;
        let token = authenticate(address).await;
        assert!(
            ok(&tunnel(
                address,
                &token,
                client_create(
                    "restartsrv",
                    &plan380_authorized_b33(),
                    serde_json::json!({"CustomOptions": credential_extension(&serde_json::json!(credential))}),
                ),
                2,
            )
            .await),
            "create must succeed before the restart"
        );
        manager
            .encrypted_target_credential("restartsrv")
            .expect("installed")
            .sealed_form()
            .to_owned()
    };

    // A second control state over the same directory, as a restart would build.
    let manager = i2pr_daemon::build_shared_service_manager(&config)
        .expect("shared manager builds")
        .expect("control implies a shared manager");
    let control = Arc::new(
        TunnelControlState::for_config(&config, Arc::clone(&manager), Arc::clone(&owner))
            .expect("control rebuilds"),
    );
    let scope = ChildScope::for_test(&CancellationToken::new(), ChildFailurePolicy::FailParent);
    control.startup(&scope, &CancellationToken::new()).await;

    let recovered = manager
        .encrypted_target_credential("restartsrv")
        .expect("the reconciliation reinstalled the credential after the restart");
    assert_eq!(
        recovered.sealed_form(),
        stored_form,
        "a restart must re-adopt the same bytes, not re-seal"
    );
    assert_eq!(
        recovered
            .open()
            .expect("opens after restart")
            .to_option_value(),
        credential,
        "the recovered credential must be the one that was configured"
    );
}

/// A data directory copied to a router that did not produce it cannot recover
/// the credential.
///
/// The owner is the only difference, which is the whole point: the stored form
/// is portable, the key that opens it is not.
#[tokio::test(flavor = "current_thread")]
async fn plan380_a_credential_is_unrecoverable_without_the_router_secret() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let owner = plan380_secret_owner();
    let credential = format!("psk:{}", "2b".repeat(32));

    let stored_form = {
        let (_state, address, _scope, _parent, manager) =
            start_service(&config, Arc::clone(&owner)).await;
        let token = authenticate(address).await;
        assert!(
            ok(&tunnel(
                address,
                &token,
                client_create(
                    "copysrv",
                    &plan380_authorized_b33(),
                    serde_json::json!({"CustomOptions": credential_extension(&serde_json::json!(credential))}),
                ),
                2,
            )
            .await),
            "create must succeed before the copy"
        );
        manager
            .encrypted_target_credential("copysrv")
            .expect("installed")
            .sealed_form()
            .to_owned()
    };

    // The same stored bytes, a different router.
    assert!(
        i2pr_daemon::encrypted_target_credential::SealedEncryptedTargetCredential::from_sealed(
            &stored_form,
            plan380_secret_owner(),
        )
        .is_err(),
        "a stored form copied onto another router must not be adopted"
    );
}

/// An **unrelated** edit leaves the sealed credential byte-for-byte identical.
///
/// This is the row that justifies the seal step's idempotence arm. `edit` merges the stored
/// options with the request's, so the seal step runs again over a value that is already this
/// owner's stored form. Two things must hold, and neither is free:
///
/// - it must not *fail*, by trying to parse its own ciphertext as a plaintext credential;
/// - it must not *re-seal*, because a fresh nonce would make the stored bytes differ after an
///   edit that changed nothing about the credential, and a generation round trip would stop being
///   byte-stable.
#[tokio::test(flavor = "current_thread")]
async fn plan380_an_unrelated_edit_preserves_the_sealed_credential() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let owner = plan380_secret_owner();
    let (_state, address, _scope, _parent, manager) =
        start_service(&config, Arc::clone(&owner)).await;
    let token = authenticate(address).await;
    let target = plan380_authorized_b33();
    let credential = format!("psk:{}", "9a".repeat(32));

    assert!(
        ok(&tunnel(
            address,
            &token,
            client_create(
                "preservesrv",
                &target,
                serde_json::json!({"CustomOptions": credential_extension(&serde_json::json!(credential))}),
            ),
            2,
        )
        .await),
        "create"
    );
    let before = manager
        .encrypted_target_credential("preservesrv")
        .expect("installed")
        .sealed_form()
        .to_owned();

    // An edit that touches the description and nothing else. `Description` is a Proposal field,
    // so this is a real edit over the wire rather than a synthetic request.
    let edited = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "edit", "Name": "preservesrv", "Description": "changed",
        }),
        3,
    )
    .await;
    assert!(
        ok(&edited),
        "an edit that changes nothing about the credential must still succeed: {edited}"
    );
    let after = manager
        .encrypted_target_credential("preservesrv")
        .expect("still installed after the edit")
        .sealed_form()
        .to_owned();
    assert_eq!(
        after, before,
        "an unrelated edit must not re-seal the credential"
    );
    assert_eq!(
        manager
            .encrypted_target_credential("preservesrv")
            .expect("installed")
            .open()
            .expect("opens")
            .to_option_value(),
        credential
    );
}

/// `edit` cannot *remove* a credential, because it merges options rather than replacing them.
///
/// Pinned as a property rather than left to be discovered, because the natural assumption is the
/// opposite: an operator who edits a service to stop presenting a credential would otherwise
/// believe they had done so, and the router would keep presenting it.
///
/// This is the same merge limitation Plan 376 found and pinned for the outproxy block, and it has
/// the same remedy: `delete` the definition and `create` it again. The row below does exactly
/// that, so the limitation is never the only thing a reader has.
#[tokio::test(flavor = "current_thread")]
async fn plan380_an_edit_cannot_remove_a_credential_but_a_delete_can() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let owner = plan380_secret_owner();
    let (_state, address, _scope, _parent, manager) =
        start_service(&config, Arc::clone(&owner)).await;
    let token = authenticate(address).await;
    let target = plan380_authorized_b33();
    let credential = format!("psk:{}", "4d".repeat(32));

    assert!(
        ok(&tunnel(
            address,
            &token,
            client_create(
                "delsrv",
                &target,
                serde_json::json!({"CustomOptions": credential_extension(&serde_json::json!(credential))}),
            ),
            2,
        )
        .await),
        "create"
    );
    assert!(manager.encrypted_target_credential("delsrv").is_some());

    // An edit that simply does not mention the credential keeps it, because merge.
    let edited = tunnel(
        address,
        &token,
        serde_json::json!({
            "Action": "edit", "Name": "delsrv", "TargetDestination": target,
            "DelayOpen": true,
        }),
        3,
    )
    .await;
    assert!(ok(&edited), "edit: {edited}");
    assert!(
        manager.encrypted_target_credential("delsrv").is_some(),
        "an edit merges options, so omitting the credential does not remove it — this is the \
         documented limit, and delete + create is the remedy"
    );

    // Delete drops it outright: a definition removed from the map must not leave a live credential
    // installed against a spec id nothing owns any more.
    let deleted = tunnel(
        address,
        &token,
        serde_json::json!({"Action": "delete", "Name": "delsrv"}),
        4,
    )
    .await;
    assert!(ok(&deleted), "delete: {deleted}");
    assert!(
        manager.encrypted_target_credential("delsrv").is_none(),
        "a deleted definition must not leave a live credential installed"
    );

    // And the remedy is a plain create without the credential.
    assert!(
        ok(&tunnel(
            address,
            &token,
            client_create("delsrv", &target, serde_json::json!({})),
            5,
        )
        .await),
        "recreate"
    );
    assert!(
        manager.encrypted_target_credential("delsrv").is_none(),
        "a service recreated without the credential must present none"
    );
}

/// The extension seam's own refusals over the wire.
///
/// The untyped Proposal blob form is the case this repository refused before Plan 380 and the
/// reason it refused has not changed, so that refusal is asserted here as a live row rather than
/// left to the contract suite: an operator who reaches for the familiar `Key=Value` form must be
/// told no, over the same connection they would have used.
#[tokio::test(flavor = "current_thread")]
async fn plan380_the_extension_seam_refuses_everything_it_does_not_type() {
    let directory = tempfile::tempdir().expect("tempdir");
    let config =
        Config::parse(&config_text(directory.path(), TEST_PASSWORD)).expect("config parses");
    let (_state, address, _scope, _parent, _manager) =
        start_service(&config, plan380_secret_owner()).await;
    let token = authenticate(address).await;
    let target = plan380_authorized_b33();

    let refused: [(&str, serde_json::Value); 6] = [
        (
            "the untyped I2CP blob form",
            serde_json::json!("TargetHost=attacker.invalid"),
        ),
        (
            "an unknown extension name",
            serde_json::json!({"i2pr": {"NoSuchThing": "x"}}),
        ),
        (
            "a wrong namespace",
            serde_json::json!({"I2PR": {"LeasesetClientCredential": format!("psk:{}", "01".repeat(32))}}),
        ),
        (
            "two namespaces",
            serde_json::json!({
                "i2pr": {"LeasesetClientCredential": format!("psk:{}", "01".repeat(32))},
                "other": {"X": "y"},
            }),
        ),
        (
            "a non-string value",
            serde_json::json!({"i2pr": {"LeasesetClientCredential": 1}}),
        ),
        (
            "a namespace that is not an object",
            serde_json::json!({"i2pr": format!("psk:{}", "01".repeat(32))}),
        ),
    ];
    for (index, (label, shape)) in refused.iter().enumerate() {
        let response = tunnel(
            address,
            &token,
            client_create(
                &format!("seam{index}"),
                &target,
                serde_json::json!({"CustomOptions": shape}),
            ),
            2 + u32::try_from(index).expect("index fits"),
        )
        .await;
        assert!(!ok(&response), "{label} must be refused: {response}");
    }

    // And the historical reason string is still what a client sees for the untyped form, so an
    // operator upgrading gets the same answer they got before.
    let untyped = tunnel(
        address,
        &token,
        client_create(
            "seamuntyped",
            &target,
            serde_json::json!({"CustomOptions": "TargetHost=attacker.invalid"}),
        ),
        20,
    )
    .await;
    assert!(
        untyped.to_string().contains("no safe typed allowlist"),
        "the untyped form must still be refused for the reason it always was: {untyped}"
    );
}
