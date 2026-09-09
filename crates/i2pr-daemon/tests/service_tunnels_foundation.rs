//! Plan 174 §9 focused service-tunnel foundation tests.
//!
//! Black-box coverage for the daemon configuration surface:
//! - disabled-by-default leaves the daemon graph unchanged;
//! - non-loopback local listeners are rejected;
//! - non-loopback server TCP targets are rejected;
//! - `enabled = true` tunnels are rejected as not-yet-available
//!   (no listener starts in Plan 174).
//!
//! After listener startup, canonical M10 product tests must not call
//! private delivery/bridge APIs. These tests only parse
//! configuration and inspect the public service graph.

#![forbid(unsafe_code)]

use i2pr_daemon::config::Config;

fn minimal(data_dir: &std::path::Path) -> String {
    format!(
        "schema_version = 1\n[router]\ndata_dir = {:?}\n",
        data_dir.to_string_lossy()
    )
}

fn graph_names(config: &Config) -> Vec<String> {
    let graph = i2pr_daemon::build_daemon_graph(config).expect("graph builds");
    let mut names: Vec<String> = graph
        .startup_order()
        .iter()
        .map(|name| name.as_str().to_string())
        .collect();
    names.sort();
    names
}

fn canonical_b32() -> String {
    format!("{}.b32.i2p", "a".repeat(52))
}

#[test]
fn disabled_by_default_leaves_daemon_graph_unchanged() {
    let directory = tempfile::tempdir().expect("temp directory");
    let base = Config::parse(&minimal(directory.path())).expect("minimal");
    let base_names = graph_names(&base);
    assert!(base_names.contains(&"lifecycle".to_owned()));
    assert!(base_names.contains(&"netdb-bootstrap".to_owned()));
    assert!(
        !base_names.iter().any(|name| name.contains("service")),
        "no service-tunnel service may be registered, got: {base_names:?}"
    );
    assert!(
        !base_names.iter().any(|name| name.contains("http")),
        "no HTTP listener may be registered, got: {base_names:?}"
    );
    assert!(
        !base_names.iter().any(|name| name.contains("socks")),
        "no SOCKS listener may be registered, got: {base_names:?}"
    );
    assert!(
        !base_names.iter().any(|name| name.contains("irc")),
        "no IRC listener may be registered, got: {base_names:?}"
    );

    // An explicit disabled section with a disabled tunnel must not
    // change the graph either.
    let b32 = canonical_b32();
    let text = format!(
        "{}\n[service_tunnels]\nenabled = false\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"generic-client\"\nenabled = false\nlistener = \"127.0.0.1:8080\"\ndestination = \"{b32}\"\n",
        minimal(directory.path())
    );
    let with_disabled = Config::parse(&text).expect("disabled tunnel validates");
    assert!(!with_disabled.service_tunnels.enabled);
    assert_eq!(with_disabled.service_tunnels.tunnels.len(), 1);
    assert_eq!(graph_names(&with_disabled), base_names);
}

#[test]
fn non_loopback_local_listener_rejected_before_mutation() {
    let directory = tempfile::tempdir().expect("temp directory");
    let b32 = canonical_b32();
    for listener in ["0.0.0.0:8080", "192.168.1.10:8080"] {
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"generic-client\"\nlistener = \"{listener}\"\ndestination = \"{b32}\"\n",
            minimal(directory.path())
        );
        assert!(
            Config::parse(&text).is_err(),
            "non-loopback listener {listener} must be rejected"
        );
    }
}

#[test]
fn non_loopback_server_tcp_target_rejected_before_mutation() {
    let directory = tempfile::tempdir().expect("temp directory");
    for target in ["192.168.1.10:9090", "0.0.0.0:9090"] {
        let text = format!(
            "{}\n[service_tunnels]\n[[service_tunnels.tunnel]]\nid = \"srv\"\nkind = \"generic-server\"\ntarget = \"{target}\"\n",
            minimal(directory.path())
        );
        assert!(
            Config::parse(&text).is_err(),
            "non-loopback target {target} must be rejected"
        );
    }
}

#[test]
fn enabled_service_tunnel_is_not_yet_available_and_starts_no_listener() {
    let directory = tempfile::tempdir().expect("temp directory");
    let b32 = canonical_b32();
    let text = format!(
        "{}\n[service_tunnels]\nenabled = true\n[[service_tunnels.tunnel]]\nid = \"alpha\"\nkind = \"generic-client\"\nenabled = true\nlistener = \"127.0.0.1:8080\"\ndestination = \"{b32}\"\n",
        minimal(directory.path())
    );
    let err = Config::parse(&text).expect_err("enabled tunnel must be rejected in Plan 174");
    assert!(
        matches!(
            err,
            i2pr_daemon::config::ConfigError::Semantic {
                field: "service_tunnels.tunnel.enabled",
                ..
            }
        ),
        "enabled rejection must be explicit, got: {err:?}"
    );
    // Because validation fails, no graph (and therefore no
    // listener) can be constructed from the rejected config.
}
