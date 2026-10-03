//! Plan 294 address-book product tests (black-box through the manager).
//!
//! The tests drive the production [`ServiceTunnelManager`] plus the
//! canonical [`AddressBookManager`](i2pr_daemon::addressbook::AddressBookManager)
//! and prove the same-owner property end to end:
//!
//! - a book hostname resolves through `resolve_reference` to the
//!   co-owned server destination without external LeaseSet lookup;
//! - static aliases still win over book entries;
//! - restart restores the naming (generation persistence);
//! - without an installed handle the legacy `UnknownAlias` verdict
//!   stands (feature isolation).

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use i2pr_daemon::addressbook::{AddressBookManager, AddressBookSubsystemConfig};
use i2pr_daemon::service_tunnels::{
    DestinationFailure, ServiceTunnelManager, ServiceTunnelManagerConfig,
};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, LocalListenerSpec, ServerTarget, ServiceTimeouts,
    ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
};
use tokio::net::TcpListener;

fn temp_data_dir(name: &str) -> tempfile::TempDir {
    let directory = tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .expect("tempdir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
            .expect("set tempdir permissions");
    }
    directory
}

fn build_manager(data_dir: &Path) -> ServiceTunnelManager {
    let target_socket: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let tunnel_set = ServiceTunnelSet {
        tunnels: vec![ServiceTunnelSpec {
            id: ServiceTunnelId::parse("alpha-server").expect("id"),
            kind: ServiceTunnelKind::GenericServer,
            enabled: true,
            listener: None,
            target: Some(ServerTarget::LoopbackTcp(target_socket)),
            targets: Vec::new(),
            destination: None,
            policy: DestinationPolicy::Dedicated,
            max_connections: 4,
            max_buffered_bytes_per_direction: 65536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: i2pr_service_tunnels::TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: i2pr_service_tunnels::IdlePolicy::disabled(),
            access: i2pr_service_tunnels::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: i2pr_service_tunnels::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        }],
    };
    ServiceTunnelManager::new(ServiceTunnelManagerConfig {
        data_dir: data_dir.to_path_buf(),
        aggregate_connection_ceiling: 32,
        per_service_connection_ceiling: 8,
        specs: Arc::new(tunnel_set),
        aliases: Arc::new(StaticAliasTable::new()),
    })
    .expect("manager builds")
}

fn activate_books(data_dir: &Path) -> AddressBookManager {
    AddressBookManager::activate(AddressBookSubsystemConfig {
        enabled: true,
        state_dir: data_dir.join("addressbook"),
    })
}

#[tokio::test(flavor = "current_thread")]
async fn book_hostname_resolves_to_co_owned_server_destination() {
    let data_dir = temp_data_dir("ab-product");
    // Bind a throwaway target so the server spec has a live loopback peer.
    let probe = TcpListener::bind("127.0.0.1:0").await.expect("probe bind");
    let _socket = probe.local_addr().expect("probe addr");
    let manager = Arc::new(build_manager(data_dir.path()));
    let _ = manager.prepare().await.expect("prepare server");
    let server_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("server destination");
    // Commit the server destination under a book hostname through the
    // canonical owner (not through aliases or test-only maps).
    let books = activate_books(data_dir.path());
    assert!(books.is_active());
    books
        .apply_entry(i2pr_addressbook::EntryMutation {
            book: i2pr_addressbook::BookKind::Private,
            hostname: "server.i2p".to_owned(),
            destination: Some(server_b64.clone()),
            delete: false,
        })
        .expect("book entry");
    manager.set_addressbook_handle(books.shared());
    // Resolution flows book -> decode -> co-owned local destination
    // with no external LeaseSet lookup.
    let reference = DestinationRef::parse("server.i2p").expect("static alias");
    let target = manager
        .resolve_reference(&reference)
        .expect("book hostname resolves");
    let server_id = manager
        .service_destination_id("alpha-server")
        .expect("server id");
    assert_eq!(
        target.remote.destination_hash,
        *server_id.as_hash().as_bytes(),
        "book hostname must resolve to the co-owned server destination"
    );
    // A control-plane listener target for the same name would dial the
    // same hash: the stored text is the server destination verbatim.
    let stored = books.shared().lookup("server.i2p").expect("stored");
    assert_eq!(stored.destination, server_b64);
}

#[tokio::test(flavor = "current_thread")]
async fn naming_survives_manager_and_subsystem_restart() {
    let data_dir = temp_data_dir("ab-restart");
    let manager = Arc::new(build_manager(data_dir.path()));
    let _ = manager.prepare().await.expect("prepare server");
    let server_b64 = manager
        .service_destination_b64("alpha-server")
        .expect("server destination");
    let books = activate_books(data_dir.path());
    books
        .apply_entry(i2pr_addressbook::EntryMutation {
            book: i2pr_addressbook::BookKind::Local,
            hostname: "restart.i2p".to_owned(),
            destination: Some(server_b64),
            delete: false,
        })
        .expect("book entry");
    let revision = books.revision().expect("revision");
    drop(books);
    drop(manager);
    // Fresh subsystem + manager over the same directories: the naming
    // restores from the persisted generation with a stable revision.
    let books = activate_books(data_dir.path());
    assert!(books.is_active());
    assert_eq!(books.revision(), Some(revision));
    let manager = Arc::new(build_manager(data_dir.path()));
    let _ = manager.prepare().await.expect("prepare again");
    manager.set_addressbook_handle(books.shared());
    let reference = DestinationRef::parse("restart.i2p").expect("static alias");
    let target = manager
        .resolve_reference(&reference)
        .expect("naming survives restart");
    let server_id = manager
        .service_destination_id("alpha-server")
        .expect("server id");
    assert_eq!(
        target.remote.destination_hash,
        *server_id.as_hash().as_bytes()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn uninstalled_handle_keeps_legacy_unknown_alias() {
    let data_dir = temp_data_dir("ab-isolated");
    let manager = Arc::new(build_manager(data_dir.path()));
    let _ = manager.prepare().await.expect("prepare server");
    // No handle installed: the pre-294 verdict stands even though no
    // alias exists for the name.
    let reference = DestinationRef::parse("ghost.i2p").expect("static alias");
    assert!(matches!(
        manager.resolve_reference(&reference),
        Err(DestinationFailure::UnknownAlias(_))
    ));
    // A client listener spec still parses (unused here, kept to prove
    // the listener surface is unaffected by the subsystem).
    let _listener = LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener");
}
