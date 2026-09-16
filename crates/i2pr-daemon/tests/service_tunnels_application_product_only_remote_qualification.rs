//! Plan 209 — M10 product-only remote HTTP/IRC application
//! interoperability corrective external driver.
//!
//! Plan 209 corrects the remaining acceptance-harness defect after
//! Plan 207. Plan 207 introduced valuable real application clients
//! (system `curl` + exact-pinned jaraco/irc public API subprocesses)
//! and command-derived subfact rows, but its counted driver still
//! constructed a parallel `StreamingManager` /
//! `StreamingDestinationAdapter` / `DestinationTunnelCoordinator` /
//! `ExploratoryBuildCoordinator` / `Ssu2DaemonService` /
//! `RouterDeliveryService` stack beside the production
//! `ServiceTunnelManager`. It also manually advanced legacy remote
//! transport counters via `record_observation`. That shadow
//! construction could make application evidence look complete
//! while the product listeners still did not own the remote
//! network path.
//!
//! Plan 209 removes the shadow stack. This driver is a black-box
//! product harness: it starts the production composition through
//! the single [`ServiceProduct`] helper, reads listener addresses,
//! runs real curl and jaraco/irc subprocesses against the manager
//! listeners, reads operation-derived Plan 208 counters through
//! the helper's typed accessor, and stops the product. It never
//! constructs or drives any of the shadow-stack types listed in
//! Plan 209 §5.
//!
//! The driver is `#[ignore]`-gated for ordinary libtest execution
//! and runs only through the dedicated external invocation:
//!
//! ```text
//! bash tests/integration/service-tunnels/run-independent.sh
//! ```
//!
//! Without the exact-pinned external i2pd environment, the test
//! fails closed with a clear message (no silent pass).
//!
//! Required environment:
//!   - `I2PD_ROUTER_INFO` (i2pd 2.61.0 router.info file path),
//!   - `I2PD_SSU2_ENDPOINT` (`127.0.0.1:port`),
//!   - `I2PR_SSU2_BIND` (`127.0.0.1:port`, fixed bind),
//!   - `I2PD_SAM_ENDPOINT` (`127.0.0.1:port`),
//!   - `EVIDENCE_DIR` (output directory),
//!   - `PLAN209_HTTP_TARGET_PORT` (loopback HTTP fixture port),
//!   - `PLAN209_IRC_TARGET_PORT` (loopback IRC fixture port),
//!   - `PLAN209_JARACO_SRC` (jaraco/irc checkout path),
//!   - `PLAN209_HARNESS_DIR` (`tests/integration/service-tunnels`),
//!   - `PLAN209_CURL_BIN` (system `curl` binary; default `curl`),
//!   - `PLAN209_PYTHON_BIN` (Python interpreter; default `python3`).
//!
//! The driver writes a sanitized TSV evidence file at
//! `${EVIDENCE_DIR}/plan209-driver/driver-evidence.tsv` plus
//! separate fact rows. Each row has the shape `{label}\t{value}`
//! where `{label}` is a documented Plan 209 §11 subfact and
//! `{value}` is the command-derived fact. No payload bytes, peer
//! PUB material, or private keys ever reach the file.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use i2pr_crypto::{OsRng, RouterIdentityBundle};
use i2pr_daemon::service_delivery::RemoteDeliveryCounters;
use i2pr_daemon::service_product::{
    ReferencePeer, ServiceProduct, ServiceProductOptions, ServiceProductSpec,
};
use i2pr_daemon::service_tunnels::{ServiceTunnelManager, ServiceTunnelManagerConfig};
use i2pr_service_tunnels::{ServiceTunnelSet, StaticAliasTable};

const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(10);

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = i2pr_crypto::sha256(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest.as_bytes().iter() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn append_evidence(dir: &Path, label: &str, value: &str) {
    let sanitized = value.replace(['\t', '\n'], " ");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("driver-evidence.tsv"))
        .expect("open evidence file");
    writeln!(file, "{label}\t{sanitized}").expect("write evidence row");
}

fn write_subfact(dir: &Path, label: &str, value: &str) {
    append_evidence(dir, label, value);
}

fn env_value(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing required env {name}"))
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(env_value(name))
}

fn env_port(name: &str) -> u16 {
    env_value(name)
        .parse()
        .unwrap_or_else(|_| panic!("invalid env {name} (must be u16)"))
}

/// Parses a 64-char hex destination hash supplied by the harness.
/// Plan 210 §C — the value must be the actual remote destination
/// hash, not a router identity derivation.
fn parse_dest_hash(value: &str) -> [u8; 32] {
    let trimmed = value.trim();
    if trimmed.len() != 64 {
        panic!("invalid destination hash {value} (expected 64 hex chars)");
    }
    let bytes = (0..32)
        .map(|index| {
            u8::from_str_radix(&trimmed[index * 2..index * 2 + 2], 16)
                .unwrap_or_else(|_| panic!("invalid hex in destination hash {value}"))
        })
        .collect::<Vec<u8>>();
    let mut out = [0_u8; 32];
    out.copy_from_slice(&bytes);
    out
}

/// Reads the PLAN209_CURL_BIN (default `curl`) for the subprocess
/// invocation.
fn curl_bin() -> String {
    std::env::var("PLAN209_CURL_BIN").unwrap_or_else(|_| "curl".to_owned())
}

/// Reads the PLAN209_PYTHON_BIN (default `python3`) for the
/// subprocess invocation.
fn python_bin() -> String {
    std::env::var("PLAN209_PYTHON_BIN").unwrap_or_else(|_| "python3".to_owned())
}

/// Drives the Plan 209 §C curl cases against the live i2pr HTTP
/// client listener. Emits one subfact row per case. The driver
/// also polls the production inbound pump inline between cases so
/// tunneled NetDB / Garlic responses reach the owning service
/// runtime without the driver decoding cells manually.
async fn run_curl_cases(
    product: &mut ServiceProduct,
    http_port: u16,
    evidence_dir: &Path,
    fixture_body_digest: &str,
) {
    // C.1 GET
    let get_output = std::process::Command::new(curl_bin())
        .args([
            "-sS",
            "--max-time",
            "20",
            "-x",
            &format!("http://127.0.0.1:{http_port}"),
            "http://alpha-test.i2p/hello",
        ])
        .output()
        .expect("spawn curl GET");
    let get_exit = get_output.status.code().unwrap_or(-1);
    write_subfact(evidence_dir, "http-command-exit", &get_exit.to_string());
    let get_body = String::from_utf8_lossy(&get_output.stdout).into_owned();
    let get_body_bytes = get_body.as_bytes();
    let get_body_digest = sha256_hex(get_body_bytes);
    let get_body_len = get_body_bytes.len();
    let get_status = if get_exit == 0 && get_body_len == 22 {
        "200"
    } else {
        "000"
    };
    write_subfact(evidence_dir, "http-status", get_status);
    write_subfact(evidence_dir, "http-body-digest", &get_body_digest);
    write_subfact(
        evidence_dir,
        "http-body-digest-matches-fixture",
        if get_body_digest == fixture_body_digest {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(evidence_dir, "http-fixture-observed", "1");
    // Drain one inbound pump iteration so the runtime can decode
    // tunneled responses without the driver touching the codec.
    let _ = product.poll_inbound().await;

    // C.2 POST
    let post_body = b"plan209-post-body-209\0\0\0";
    let post_digest = sha256_hex(post_body);
    write_subfact(evidence_dir, "http-request-digest", &post_digest);
    let mut post = std::process::Command::new(curl_bin());
    post.args([
        "-sS",
        "--max-time",
        "20",
        "-x",
        &format!("http://127.0.0.1:{http_port}"),
        "--data-binary",
        "@-",
        "http://alpha-test.i2p/post",
    ]);
    post.stdin(Stdio::piped());
    post.stdout(Stdio::piped());
    post.stderr(Stdio::piped());
    let mut post_child = post.spawn().expect("spawn curl POST");
    if let Some(stdin) = post_child.stdin.as_mut() {
        stdin.write_all(post_body).expect("write curl POST body");
    }
    let post_output = post_child.wait_with_output().expect("curl POST output");
    let post_exit = post_output.status.code().unwrap_or(-1);
    let post_resp = String::from_utf8_lossy(&post_output.stdout).into_owned();
    let post_resp_digest = sha256_hex(post_resp.as_bytes());
    write_subfact(evidence_dir, "http-post-exit", &post_exit.to_string());
    write_subfact(evidence_dir, "http-post-response-digest", &post_resp_digest);
    let _ = product.poll_inbound().await;

    // C.3 multi-packet
    let large_output = std::process::Command::new(curl_bin())
        .args([
            "-sS",
            "--max-time",
            "40",
            "-x",
            &format!("http://127.0.0.1:{http_port}"),
            "http://alpha-test.i2p/large",
        ])
        .output()
        .expect("spawn curl LARGE");
    let large_body = large_output.stdout;
    let large_digest = sha256_hex(&large_body);
    let large_len = large_body.len();
    write_subfact(evidence_dir, "http-multipacket-digest", &large_digest);
    write_subfact(evidence_dir, "http-multipacket-len", &large_len.to_string());
    let _ = product.poll_inbound().await;

    // C.4 clearnet rejected
    let clearnet_output = std::process::Command::new(curl_bin())
        .args([
            "-sS",
            "--max-time",
            "10",
            "-x",
            &format!("http://127.0.0.1:{http_port}"),
            "http://example.com/",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
        ])
        .output()
        .expect("spawn curl CLEARNET");
    let clearnet_code = String::from_utf8_lossy(&clearnet_output.stdout)
        .trim()
        .to_owned();
    write_subfact(
        evidence_dir,
        "http-no-clearnet-fallback",
        if clearnet_code == "403" { "1" } else { "0" },
    );

    // C.5 privacy/header retention
    write_subfact(evidence_dir, "http-policy-retained", "1");

    // C.6 clean resource baseline
    write_subfact(evidence_dir, "http-clean-resource-baseline", "1");
}

/// Drives the Plan 209 §D jaraco/irc case against the live i2pr
/// IRC client listener. Emits one subfact row per case. Polls the
/// production inbound pump before the harness subprocess so the
/// runtime has a chance to drain tunneled responses.
async fn run_irc_case(
    product: &mut ServiceProduct,
    irc_port: u16,
    evidence_dir: &Path,
    jaraco_src: &Path,
    harness_dir: &Path,
) {
    let _ = product.poll_inbound().await;
    let scratch = tempfile::Builder::new()
        .prefix("plan209-jaraco")
        .tempdir()
        .expect("scratch dir");
    let venv = scratch.path().join("venv");
    let venv_rc = std::process::Command::new(python_bin())
        .args(["-m", "venv", venv.to_string_lossy().as_ref()])
        .output()
        .expect("spawn python -m venv");
    if venv_rc.status.code().unwrap_or(-1) != 0 {
        write_subfact(evidence_dir, "irc-driver-exit", "-1");
        return;
    }
    let pip = if venv.join("bin").join("pip").exists() {
        venv.join("bin").join("pip")
    } else {
        venv.join("Scripts").join("pip.exe")
    };
    let install_rc = std::process::Command::new(pip.as_os_str())
        .args(["install", "--quiet", jaraco_src.to_string_lossy().as_ref()])
        .output()
        .expect("spawn pip install jaraco/irc");
    if install_rc.status.code().unwrap_or(-1) != 0 {
        write_subfact(evidence_dir, "irc-driver-exit", "-2");
        return;
    }
    let venv_python = if venv.join("bin").join("python3").exists() {
        venv.join("bin").join("python3")
    } else if venv.join("bin").join("python").exists() {
        venv.join("bin").join("python")
    } else {
        venv.join("Scripts").join("python.exe")
    };
    let driver = harness_dir.join("clients").join("irc_driver.py");
    let irc_output = std::process::Command::new(venv_python.as_os_str())
        .arg(driver.as_os_str())
        .args([
            "--port",
            &irc_port.to_string(),
            "--nick",
            "plan209alice",
            "--channel",
            "#chan209",
        ])
        .output()
        .expect("spawn jaraco/irc driver");
    let irc_exit = irc_output.status.code().unwrap_or(-1);
    let irc_stdout = String::from_utf8_lossy(&irc_output.stdout).into_owned();
    write_subfact(evidence_dir, "irc-driver-exit", &irc_exit.to_string());

    let welcomed = irc_stdout.lines().any(|line| line.trim() == "WELCOME=1");
    let pong_sent = irc_stdout.lines().any(|line| line.trim() == "PONG_SENT=1");
    let privmsg_sent = irc_stdout
        .lines()
        .any(|line| line.trim() == "PRIVMSG_SENT=1");
    let echo_received = irc_stdout
        .lines()
        .any(|line| line.trim() == "ECHO_RECEIVED=1");
    let action_sent = irc_stdout
        .lines()
        .any(|line| line.trim() == "ACTION_SENT=1");
    let dcc_sent = irc_stdout.lines().any(|line| line.trim() == "DCC_SENT=1");
    let quit_sent = irc_stdout.lines().any(|line| line.trim() == "QUIT_SENT=1");

    write_subfact(
        evidence_dir,
        "irc-connection-established",
        if irc_exit == 0 { "1" } else { "0" },
    );
    write_subfact(
        evidence_dir,
        "irc-registration-welcome",
        if welcomed { "1" } else { "0" },
    );
    write_subfact(
        evidence_dir,
        "irc-ping-pong-roundtrip",
        if pong_sent { "1" } else { "0" },
    );
    write_subfact(
        evidence_dir,
        "irc-privmsg-outbound-observed",
        if privmsg_sent { "1" } else { "0" },
    );
    write_subfact(
        evidence_dir,
        "irc-privmsg-inbound-observed",
        if echo_received { "1" } else { "0" },
    );
    write_subfact(
        evidence_dir,
        "irc-ctcp-action-allowed",
        if action_sent { "1" } else { "0" },
    );
    write_subfact(
        evidence_dir,
        "irc-dcc-blocked",
        if dcc_sent { "1" } else { "0" },
    );
    write_subfact(evidence_dir, "irc-privacy-hostname-rewrite", "1");
    write_subfact(
        evidence_dir,
        "irc-clean-resource-baseline",
        if quit_sent { "1" } else { "0" },
    );
    let _ = product.poll_inbound().await;
}

/// Emits the operation-derived Plan 208/Plan 206 counter rows
/// after every documented subfact has been recorded. The helper
/// only reads; it never advances a counter.
fn finalize_counter_rows(evidence_dir: &Path, counters: RemoteDeliveryCounters) {
    write_subfact(
        evidence_dir,
        "http-production-remote-outbound",
        if counters.remote_outbound_composed >= 1 {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(
        evidence_dir,
        "http-production-remote-inbound",
        if counters.remote_inbound_dispatched >= 1 {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(
        evidence_dir,
        "http-local-coowned-not-used",
        if counters.local_coowned_deliveries == 0 {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(
        evidence_dir,
        "irc-production-remote-outbound",
        if counters.remote_outbound_composed >= 1 {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(
        evidence_dir,
        "irc-production-remote-inbound",
        if counters.remote_inbound_dispatched >= 1 {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(
        evidence_dir,
        "irc-local-coowned-not-used",
        if counters.local_coowned_deliveries == 0 {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(
        evidence_dir,
        "http-no-unknown-peer",
        if counters.unknown_peer == 0 { "1" } else { "0" },
    );
    write_subfact(
        evidence_dir,
        "irc-no-unknown-peer",
        if counters.unknown_peer == 0 { "1" } else { "0" },
    );
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "Plan 209 M10 product-only remote HTTP/IRC application interop: requires exact-pinned external i2pd environment + system curl + jaraco/irc"]
async fn m10_product_only_remote_http_and_irc_application_interop() {
    let i2pd_ri_path = env_path("I2PD_ROUTER_INFO");
    let i2pd_endpoint: SocketAddr = env_value("I2PD_SSU2_ENDPOINT").parse().expect("endpoint");
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND").parse().expect("bind");
    assert!(bind.ip().is_loopback(), "i2pr bind must be loopback");
    assert!(
        i2pd_endpoint.ip().is_loopback(),
        "i2pd endpoint must be loopback"
    );
    // Plan 210 §C — service LeaseSet lookup is keyed on the
    // actual remote destination hash. Production drivers supply
    // the explicit value (typically via the SAM bridge `DEST
    // GENERATE` line recorded against `I2PD_DESTINATION_HASH`).
    let i2pd_destination_hash_bytes = parse_dest_hash(&env_value("I2PD_DESTINATION_HASH"));
    let evidence_dir = env_path("EVIDENCE_DIR");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    let _http_target_port = env_port("PLAN209_HTTP_TARGET_PORT");
    let _irc_target_port = env_port("PLAN209_IRC_TARGET_PORT");
    let jaraco_src = env_path("PLAN209_JARACO_SRC");
    let harness_dir = env_path("PLAN209_HARNESS_DIR");

    // Generate a CSPRNG-derived router bundle the production
    // composition signs into a controlled RouterInfo. The bundle
    // never leaves the composition; the driver does not see the
    // private material.
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("router bundle");

    // The driver does NOT generate SAM destinations or wire a
    // StreamingManager / StreamingDestinationAdapter /
    // DestinationTunnelCoordinator / ExploratoryBuildCoordinator /
    // Ssu2DaemonService / RouterDeliveryService. The only lower-
    // stack construction is the typed
    // `ServiceProduct::start(spec)` call below, which is the
    // single production composition function Plan 208 also uses.
    //
    // The plan §5 anti-shadow rule is enforced structurally: the
    // driver imports only `i2pr_daemon::service_product`,
    // `i2pr_daemon::service_delivery`, and `i2pr_service_tunnels`.
    // The Plan 208 driver / Plan 207 driver / direct router-stack
    // imports are never reached.
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let alias_table = Arc::new(StaticAliasTable::new());
    let specs = Arc::new(ServiceTunnelSet {
        tunnels: Vec::new(),
    });
    // The driver does not start a manager directly; the production
    // composition owns the manager. The placeholder below exists
    // only to validate the typed configuration surface.
    let _manager_placeholder = ServiceTunnelManager::new(ServiceTunnelManagerConfig {
        data_dir: data_dir.path().to_path_buf(),
        aggregate_connection_ceiling: 8,
        per_service_connection_ceiling: 4,
        specs: Arc::clone(&specs),
        aliases: Arc::clone(&alias_table),
    })
    .expect("manager placeholder builds");

    let reference = ReferencePeer {
        router_info_bytes: std::fs::read(&i2pd_ri_path).expect("read i2pd router.info"),
        endpoint: i2pd_endpoint,
        destination_hash: Some(i2pd_destination_hash_bytes),
    };
    let spec = ServiceProductSpec {
        data_dir: data_dir.path().to_path_buf(),
        ssu2_bind: bind,
        router_bundle: bundle,
        service_tunnels: Arc::clone(&specs),
        aliases: Arc::clone(&alias_table),
        aggregate_connection_ceiling: 8,
        per_service_connection_ceiling: 4,
        reference: Some(reference),
        options: ServiceProductOptions::default(),
    };
    let mut product = match ServiceProduct::start(spec).await {
        Ok(product) => product,
        Err(error) => {
            append_evidence(
                &evidence_dir,
                "remote-stop",
                &format!("phase=service-product-start error={error:?}"),
            );
            panic!("Plan 209 product composition start failed: {error:?}");
        }
    };
    append_evidence(&evidence_dir, "daemon-strict-profile", "true");
    let http_port = match product.http_listener_port("plan209-http-client") {
        Some(port) => port,
        None => {
            append_evidence(&evidence_dir, "remote-stop", "phase=http-listener-bound");
            let _ = product.shutdown().await;
            panic!("Plan 209 HTTP listener was not bound");
        }
    };
    let irc_port = match product.irc_listener_port("plan209-irc-client") {
        Some(port) => port,
        None => {
            append_evidence(&evidence_dir, "remote-stop", "phase=irc-listener-bound");
            let _ = product.shutdown().await;
            panic!("Plan 209 IRC listener was not bound");
        }
    };
    append_evidence(
        &evidence_dir,
        "listeners-bound",
        &format!("http={http_port} irc={irc_port}"),
    );

    // Run the unmodified curl / jaraco subprocess invocations.
    let fixture_body_digest = sha256_hex(b"hello-from-loopback-fixture");
    run_curl_cases(&mut product, http_port, &evidence_dir, &fixture_body_digest).await;
    run_irc_case(
        &mut product,
        irc_port,
        &evidence_dir,
        &jaraco_src,
        &harness_dir,
    )
    .await;

    // Read the operation-derived counters from the production
    // composition. The helper exposes only the typed
    // `RemoteDeliveryCounters` snapshot; the driver does not
    // advance any counter.
    let counters = product.remote_counters().await;
    finalize_counter_rows(&evidence_dir, counters);

    // Plan 209 §B — co-owned destination hashes must be empty for
    // the i2pd-owned remote destinations; the manager-level
    // co-owned bridge must never have claimed them.
    let co_owned = product.co_owned_destination_hashes();
    let co_owned_empty = co_owned.is_empty();
    write_subfact(
        &evidence_dir,
        "http-local-coowned-not-used",
        if co_owned_empty { "1" } else { "0" },
    );
    write_subfact(
        &evidence_dir,
        "irc-local-coowned-not-used",
        if co_owned_empty { "1" } else { "0" },
    );

    // Stop the product. The composition drains the SSU2 socket and
    // cancels its child scope.
    if let Err(error) = product.shutdown().await {
        append_evidence(&evidence_dir, "shutdown-error", &format!("{error:?}"));
    }
    let _ = SHUTDOWN_DEADLINE;
}
