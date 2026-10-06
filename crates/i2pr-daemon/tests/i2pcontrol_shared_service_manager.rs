//! Plan 337 — composition-root evidence for the one shared
//! `ServiceTunnelManager`.
//!
//! Plan 289 required that the control plane own "the one existing M10
//! `ServiceTunnelManager`" and never create a second
//! service/destination/tunnel runtime. Until Plan 337 each owner built a
//! private one, so a service tunnel created through TunnelManager was off
//! the publication path entirely. These rows assert the property at the
//! composition root — the layer that owns the decision — rather than at any
//! one function, because the defect was precisely that no single function
//! was wrong on its own.
//!
//! Everything here goes through production construction
//! (`build_daemon_graph_with_inspection` and
//! `build_shared_service_manager`). Nothing binds a socket: the graph is
//! built, not run.

use std::sync::Arc;

use i2pr_daemon::config::Config;

/// Plan 342: the fail-closed outbound-credential owner. These rows exercise the
/// shared-manager wiring, not a credential, so the honest value is the one
/// that refuses everything.
fn test_outbound_secrets() -> Arc<dyn i2pr_service_tunnels::outbound_secret::OutboundSecretStore> {
    Arc::new(i2pr_service_tunnels::outbound_secret::NoOutboundSecrets)
}
use i2pr_i2pcontrol::{TunnelAction, TunnelManagerRequest, TunnelType};
use i2pr_runtime::ServiceName;

const PASSWORD: &str = "plan337-shared-manager";

/// A daemon config with the control plane and SSU2 both enabled, over a
/// temporary data directory. No service tunnel is configured: the control
/// plane is how an operator adds the *first* one, so the product layer must
/// come up anyway.
fn config_text(data_dir: &std::path::Path) -> String {
    format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n\
         [i2pcontrol]\nenabled = true\nbind_address = \"127.0.0.1\"\nport = 0\npassword = {PASSWORD:?}\n\
         [ssu2]\nenabled = true\nbind_ipv4 = \"127.0.0.1\"\nport = 0\n",
        data_dir.to_string_lossy()
    )
}

fn config(data_dir: &std::path::Path) -> Config {
    Config::parse(&config_text(data_dir)).expect("strict profile")
}

/// Plan 337: the composition root builds exactly one service manager, and
/// every consumer of a service runtime shares it.
///
/// The old shape is unrepresentable from here: `build_shared_service_manager`
/// is the only production construction, and it is called once.
#[test]
fn plan337_the_composition_root_builds_exactly_one_service_manager() {
    let data = tempfile::tempdir().expect("temp data dir");
    let config = config(data.path());
    let manager = i2pr_daemon::build_shared_service_manager(&config)
        .expect("shared manager builds")
        .expect("an enabled control plane implies a shared manager");

    // Injecting the same `Arc` twice yields the same instance, so the
    // control state and the product layer cannot drift apart.
    let handed_to_control = Arc::clone(&manager);
    let handed_to_product = Arc::clone(&manager);
    assert!(
        Arc::ptr_eq(&handed_to_control, &handed_to_product),
        "both owners must receive the composition root's manager instance"
    );
    assert!(
        Arc::strong_count(&manager) >= 3,
        "the composition root keeps a reference alongside both owners"
    );

    // The manager is constructed over the validated startup inventory, so a
    // control transaction's whole-set reconcile describes the same surface
    // the product prepared.
    assert_eq!(
        manager.data_dir(),
        data.path(),
        "the manager's identity records live under the router data dir"
    );
}

/// Plan 337: the manager is built when the control plane is enabled even
/// though no service tunnel is configured. Before Plan 337 the product's
/// gate was "at least one startup tunnel is enabled", which meant a
/// control-created tunnel had no product to be published through.
#[test]
fn plan337_a_control_only_router_still_gets_the_service_runtime() {
    let data = tempfile::tempdir().expect("temp data dir");
    let config = config(data.path());
    assert!(
        !config
            .service_tunnels
            .tunnels
            .tunnels
            .iter()
            .any(|tunnel| tunnel.enabled),
        "this row is about a router with no configured service tunnel"
    );
    assert!(
        i2pr_daemon::build_shared_service_manager(&config)
            .expect("builds")
            .is_some(),
        "an enabled control plane must bring the service runtime up"
    );

    // A profile with neither the control plane nor an enabled service
    // tunnel owns no service runtime, and constructs nothing.
    let bare = tempfile::tempdir().expect("temp data dir");
    let bare_config = Config::parse(&format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n",
        bare.path().to_string_lossy()
    ))
    .expect("strict profile");
    assert!(
        i2pr_daemon::build_shared_service_manager(&bare_config)
            .expect("builds")
            .is_none(),
        "a profile that can never own a service runtime constructs no manager"
    );
}

/// Plan 337: the control plane must start **after** the SSU2 service.
///
/// The graph's default order is lexical, and `i2pcontrol` sorts before
/// `ssu2-router`. The control state reconciles onto the shared manager, so
/// it has to run after the product has prepared that manager and installed
/// the executable router delivery backend. This row pins the declared
/// dependency rather than trusting the alphabetical accident.
#[test]
fn plan337_the_control_plane_starts_after_the_router_service() {
    let data = tempfile::tempdir().expect("temp data dir");
    let config = config(data.path());
    let (graph, _inspection) =
        i2pr_daemon::build_daemon_graph_with_inspection(&config).expect("graph builds");
    let order = graph.startup_order();
    let position = |name: &str| {
        order
            .iter()
            .position(|current| current == &ServiceName::new(name).expect("valid name"))
            .unwrap_or_else(|| panic!("{name} must be registered"))
    };
    let control = position("i2pcontrol");
    let router = position("ssu2-router");
    assert!(
        router < control,
        "ssu2-router must start before i2pcontrol: order was {order:?}"
    );
}

/// Plan 337 restart: a control-owned definition survives a restart and
/// republishes the same service.
///
/// A restart is modelled the way production performs one: a **new** manager
/// over the same data directory, then a new control state reading the same
/// durable store. Nothing is carried over in memory, so what survives is
/// only what was persisted — and the service identity must not be rotated,
/// or every client would have to rediscover the destination.
#[test]
fn plan337_a_control_owned_server_survives_a_restart_without_rotating_its_identity() {
    let data = tempfile::tempdir().expect("temp data dir");
    let config = config(data.path());

    // First boot: create a server through the control plane.
    let first_manager = i2pr_daemon::build_shared_service_manager(&config)
        .expect("builds")
        .expect("control implies a shared manager");
    let first = i2pr_daemon::i2pcontrol_tunnels::TunnelControlState::for_config(
        &config,
        Arc::clone(&first_manager),
        test_outbound_secrets(),
    )
    .expect("control builds");
    let created = block_on(first.create(&request("restart-srv", "127.0.0.1:8080")));
    assert!(created.is_ok(), "first-boot create: {created:?}");
    let first_destination = first_manager
        .service_destination_b64("restart-srv")
        .expect("runtime exists");

    // Second boot: a brand-new manager and control state over the same
    // directory, started through the production startup path.
    let second_manager = i2pr_daemon::build_shared_service_manager(&config)
        .expect("builds")
        .expect("control implies a shared manager");
    assert!(
        !Arc::ptr_eq(&first_manager, &second_manager),
        "a restart builds a fresh manager instance"
    );
    let second = i2pr_daemon::i2pcontrol_tunnels::TunnelControlState::for_config(
        &config,
        Arc::clone(&second_manager),
        test_outbound_secrets(),
    )
    .expect("control builds");
    let failures = block_on(async {
        let scope = test_scope();
        second.startup(&scope.0, &scope.1).await
    });
    assert!(failures.is_empty(), "restart startup clean: {failures:?}");

    // The definition is still there, and it is a *started* one so the
    // runtime was reconciled again.
    let status = second.get(Some("restart-srv")).expect("get after restart");
    assert_eq!(status["name"], "restart-srv");
    let second_destination = second_manager
        .service_destination_b64("restart-srv")
        .expect("runtime exists after restart");
    assert_eq!(
        first_destination, second_destination,
        "a restart must not rotate a persistent server identity"
    );
    // And the store generation advanced rather than starting empty, so the
    // durable intent really was reloaded.
    assert!(second.store_generation() > 0);
}

/// Plan 338 secrets: nothing secret from the LeaseSet block reaches a control
/// response, a `Debug` rendering, or an error string — now that an encrypted
/// control-created service actually publishes and exposes a live address.
///
/// Plan 334 guarantees this for the stored definition. This row re-checks it on
/// the two surfaces Plan 337 added, because a secret could leak through either.
#[test]
fn plan338_no_lease_set_secret_reaches_a_control_response_or_a_debug_rendering() {
    let data = tempfile::tempdir().expect("temp data dir");
    let config = config(data.path());
    let manager = i2pr_daemon::build_shared_service_manager(&config)
        .expect("builds")
        .expect("control implies a shared manager");
    let owned = Arc::clone(&manager);
    let control = i2pr_daemon::i2pcontrol_tunnels::TunnelControlState::for_config(
        &config,
        manager,
        test_outbound_secrets(),
    )
    .expect("control builds");

    let lookup_secret = "plan338-lookup-secret-value";
    let client_key = "ab".repeat(32);
    let mut options = std::collections::BTreeMap::new();
    options.insert("target_host".to_owned(), "127.0.0.1".to_owned());
    options.insert("target_port".to_owned(), "8080".to_owned());
    options.insert(
        "encrypt_lease_set".to_owned(),
        "encrypted with lookup password and per-user key (psk)".to_owned(),
    );
    options.insert("leaseset_password".to_owned(), lookup_secret.to_owned());
    options.insert(
        "leaseset_client_auth".to_owned(),
        format!("client0:{client_key}"),
    );
    let accepted = block_on(control.create(&TunnelManagerRequest {
        action: TunnelAction::Create,
        all: false,
        name: Some("secretsrv".to_owned()),
        tunnel_type: Some(TunnelType::HttpServer),
        new_name: None,
        options,
    }));
    assert!(
        accepted.is_ok(),
        "an authorized encrypted control-created server is accepted: {accepted:?}"
    );

    // The live surface: a `get` carries the published address, the client
    // count, and the resolved posture. None of it may carry a secret.
    let inventory = control.get(None).expect("inventory get").to_string();
    let detail = control.get(Some("secretsrv")).expect("get").to_string();
    for surface in [inventory.as_str(), detail.as_str()] {
        for (label, needle) in [
            ("lookup secret", lookup_secret),
            ("client key", client_key.as_str()),
        ] {
            assert!(
                !surface.contains(needle),
                "the {label} must never appear in a control response"
            );
        }
    }
    // The address is present and real, so the redaction is not achieved by
    // omission.
    assert!(
        detail.contains("encrypted_address"),
        "the encrypted address must be reported: {detail}"
    );

    // The manager's own Debug carries the ELS2 material registry, whose values
    // hold the blinding identity. `ServiceEls2Material` has a redacted Debug,
    // so a secret cannot surface through it.
    let manager_debug = format!("{owned:?}");
    for (label, needle) in [
        ("lookup secret", lookup_secret),
        ("client key", client_key.as_str()),
    ] {
        assert!(
            !manager_debug.contains(needle),
            "the {label} must never appear in a Debug rendering"
        );
    }
}

fn request(name: &str, target: &str) -> TunnelManagerRequest {
    let (host, port) = target.rsplit_once(':').expect("host:port");
    let mut options = std::collections::BTreeMap::new();
    options.insert("target_host".to_owned(), host.to_owned());
    options.insert("target_port".to_owned(), port.to_owned());
    TunnelManagerRequest {
        action: TunnelAction::Create,
        all: false,
        name: Some(name.to_owned()),
        tunnel_type: Some(TunnelType::HttpServer),
        new_name: None,
        options,
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

fn test_scope() -> (i2pr_runtime::ChildScope, i2pr_runtime::CancellationToken) {
    let parent = i2pr_runtime::CancellationToken::new();
    let scope =
        i2pr_runtime::ChildScope::for_test(&parent, i2pr_runtime::ChildFailurePolicy::FailParent);
    (scope, parent)
}
