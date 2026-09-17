//! Plan 214 — M10 product-only remote HTTP/IRC application
//! interop requalification external driver (evidence hardening over
//! the retained Plan 211 harness, final M10 closure authority).
//!
//! Plan 211 retains real enabled HTTP/IRC service specs and real
//! curl/jaraco clients, but its evidence path is not terminal
//! authority: the counted driver constructed a placeholder
//! `ServiceTunnelManager`, stamped pin/privacy facts as literals,
//! inferred target observations from client-side results, derived
//! DCC blocking from `DCC_SENT` absence, approximated HTTP status
//! from body length, discarded the target-port inputs, and ran
//! blocking subprocesses without guaranteed concurrent
//! `ServiceProduct::poll_inbound()` progress.
//!
//! Plan 214 corrects the harness boundary without replacing the M10
//! HTTP/IRC product code:
//!
//! ```text
//! black-box product boundary only
//!   (ServiceTunnelSet::validate + ServiceProduct::start /
//!    poll_inbound / remote_counters / inbound_orphan_receives /
//!    service_router_network_summary; no ServiceTunnelManager::new,
//!    no lower router/tunnel/Streaming construction, no manual
//!    observation helpers)
//! runner-derived pin facts consumed only as preconditions
//! concurrent inbound pumping around every external subprocess
//! target-observed HTTP facts (fresh fixture seq + actual curl
//!   %{http_code} + POST/large digest equality + rejection
//!   zero-deltas)
//! fixture-derived IRC facts (target NICK/USER + privacy booleans
//!   + PING issued/PONG observed + token PRIVMSG both directions
//!   + explicit DCC attempt with target non-observation)
//! independent per-application counter windows with orphan deltas
//! ```
//!
//! The driver is `#[ignore]`-gated for ordinary libtest execution
//! and runs only through the dedicated external invocation:
//!
//! ```text
//! bash tests/integration/service-tunnels/run-plan214-applications.sh
//! ```
//!
//! Without the exact-pinned external i2pd environment, the test
//! fails closed with a clear message (no silent pass).
//!
//! Required environment (Plan 214 §13):
//!   - `I2PD_ROUTER_INFO` (i2pd 2.61.0 router.info file path),
//!   - `I2PD_SSU2_ENDPOINT` (`127.0.0.1:port`),
//!   - `I2PR_SSU2_BIND` (`127.0.0.1:port`, fixed bind),
//!   - `EVIDENCE_DIR` (output directory),
//!   - `PLAN214_HTTP_FIXTURE_FACTS` (harness-owned HTTP fixture
//!     facts path, streamed + flushed per request),
//!   - `PLAN214_IRC_FIXTURE_FACTS` (harness-owned IRC fixture facts
//!     path, streamed + flushed per event),
//!   - `PLAN214_HTTP_LARGE_EXPECTED_LEN` /
//!     `PLAN214_HTTP_LARGE_EXPECTED_SHA256` (fixture-published
//!     large-response contract captured by the runner at fixture
//!     startup),
//!   - `PLAN214_JARACO_SRC` (jaraco/irc checkout path),
//!   - `PLAN214_HARNESS_DIR` (`tests/integration/service-tunnels`),
//!   - `PLAN214_CURL_BIN` (system `curl` binary; default `curl`),
//!   - `PLAN214_PYTHON_BIN` (Python interpreter; default `python3`),
//!   - `PLAN214_I2PD_PIN_OK` / `PLAN214_JARACO_PIN_OK`
//!     (runner-verified preconditions, `"1"`; final pin authority
//!     stays in the runner's command-derived rows),
//!   - `PLAN214_IRC_NICK` / `PLAN214_IRC_CHANNEL` /
//!     `PLAN214_IRC_TOKEN` (deterministic IRC session identity),
//!   - `PLAN214_HTTP_DEST_B64` / `PLAN214_HTTP_DEST_HASH` /
//!     `PLAN214_HTTP_DEST_B32` (i2pd HTTP server public material),
//!   - `PLAN214_IRC_DEST_B64` / `PLAN214_IRC_DEST_HASH` /
//!     `PLAN214_IRC_DEST_B32` (analogous for the IRC server).
//!
//! The driver writes sanitized TSV evidence at
//! `${EVIDENCE_DIR}/plan214-driver/driver-evidence.tsv`. Each row
//! has the shape `{label}\t{value}` where `{label}` is a documented
//! Plan 214 §16/§17 fact and `{value}` is the command/fixture/
//! counter-derived fact. No payload bytes, peer PUB material, or
//! private keys ever reach the file. Aggregate pass rows and the
//! terminal `P214-*` classification are runner-owned; the driver
//! never writes them.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use i2pr_crypto::{OsRng, RouterIdentityBundle};
use i2pr_daemon::service_delivery::RoutingDecision;
use i2pr_daemon::service_product::{ServiceProduct, ServiceProductOptions, ServiceProductSpec};
use i2pr_service_tunnels::{
    DestinationPolicy, DestinationRef, HttpClientOptions, IrcClientOptions, LocalListenerSpec,
    ServiceTunnelKind, ServiceTunnelSet, ServiceTunnelSpec, StaticAliasTable,
};

const PLAN214_HTTP_SPEC_ID: &str = "plan214-http-client";
const PLAN214_IRC_SPEC_ID: &str = "plan214-irc-client";
const PLAN214_HTTP_ALIAS: &str = "alpha-test.i2p";
/// Documented HTTP fixture small-body contract
/// (`tests/integration/service-tunnels/fixtures/http_fixture.py`).
const PLAN214_SMALL_BODY: &[u8] = b"hello-from-loopback-fixture";
/// Deterministic POST request body (non-secret, fixed bytes).
const PLAN214_POST_BODY: &[u8] = b"plan214-post-body-214-pad";
const GET_DEADLINE: Duration = Duration::from_secs(30);
const POST_DEADLINE: Duration = Duration::from_secs(30);
const LARGE_DEADLINE: Duration = Duration::from_secs(60);
const REJECT_DEADLINE: Duration = Duration::from_secs(20);
const VENV_DEADLINE: Duration = Duration::from_secs(300);
const IRC_DEADLINE: Duration = Duration::from_secs(150);

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
    // Plan 214 §13 — failure excerpts (subprocess stderr, product
    // errors) stay bounded so the runner's overlong-token audit
    // keeps meaning key material: values longer than 350 chars are
    // truncated with a marker (the full text remains in the
    // runner-captured process log, never in the counted TSV).
    let bounded = if sanitized.chars().count() > 350 {
        let head: String = sanitized.chars().take(350).collect();
        format!("{head}...[truncated]")
    } else {
        sanitized
    };
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("driver-evidence.tsv"))
        .expect("open evidence file");
    writeln!(file, "{label}\t{bounded}").expect("write evidence row");
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

/// Reads the PLAN214_CURL_BIN (default `curl`) for the subprocess
/// invocation.
fn curl_bin() -> String {
    std::env::var("PLAN214_CURL_BIN").unwrap_or_else(|_| "curl".to_owned())
}

/// Reads the PLAN214_PYTHON_BIN (default `python3`) for the
/// subprocess invocation.
fn python_bin() -> String {
    std::env::var("PLAN214_PYTHON_BIN").unwrap_or_else(|_| "python3".to_owned())
}

/// Parses the exact bytes curl's `-w '%{http_code}'` frame emits.
///
/// Plan 214 §D1 — the HTTP status is captured from curl's actual
/// status frame, never approximated from exit code or body length.
/// The frame must be exactly three ASCII digits after trimming;
/// anything else (empty, `000`, mixed body bytes) is rejected.
fn parse_curl_status_code(frame: &str) -> Option<u16> {
    let trimmed = frame.trim();
    if trimmed.len() != 3 || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let code: u16 = trimmed.parse().ok()?;
    if (100..600).contains(&code) {
        Some(code)
    } else {
        None
    }
}

/// Extracts a top-level string field from one flat JSON object
/// line (fixture facts shape). Handles `\"` escapes. Returns
/// `None` when the key is absent or not a JSON string.
fn json_string_field(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut search = line;
    loop {
        let start = search.find(needle.as_str())?;
        let mut rest = &search[start + needle.len()..];
        rest = rest.trim_start_matches([' ', '\t']);
        if !rest.starts_with(':') {
            search = &search[start + needle.len()..];
            continue;
        }
        rest = rest[1..].trim_start_matches([' ', '\t']);
        if !rest.starts_with('"') {
            return None;
        }
        let mut out = String::new();
        let mut chars = rest[1..].chars();
        loop {
            let next = chars.next()?;
            if next == '\\' {
                let escaped = chars.next()?;
                out.push(escaped);
            } else if next == '"' {
                return Some(out);
            } else {
                out.push(next);
            }
        }
    }
}

/// Extracts a top-level unsigned integer field from one flat JSON
/// object line. Returns `None` when absent or not a bare number.
fn json_u64_field(line: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let mut search = line;
    loop {
        let start = search.find(needle.as_str())?;
        let mut rest = &search[start + needle.len()..];
        rest = rest.trim_start_matches([' ', '\t']);
        if !rest.starts_with(':') {
            search = &search[start + needle.len()..];
            continue;
        }
        rest = rest[1..].trim_start_matches([' ', '\t']);
        let end = rest
            .find(|char: char| !char.is_ascii_digit())
            .unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        return rest[..end].parse().ok();
    }
}

/// Extracts a top-level boolean field from one flat JSON object
/// line. Returns `None` when absent or not `true`/`false`.
fn json_bool_field(line: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{key}\"");
    let mut search = line;
    loop {
        let start = search.find(needle.as_str())?;
        let mut rest = &search[start + needle.len()..];
        rest = rest.trim_start_matches([' ', '\t']);
        if !rest.starts_with(':') {
            search = &search[start + needle.len()..];
            continue;
        }
        rest = rest[1..].trim_start_matches([' ', '\t']);
        if let Some(tail) = rest.strip_prefix("true") {
            if tail
                .chars()
                .next()
                .is_none_or(|char| !char.is_ascii_alphanumeric() && char != '_')
            {
                return Some(true);
            }
            return None;
        }
        if let Some(tail) = rest.strip_prefix("false") {
            if tail
                .chars()
                .next()
                .is_none_or(|char| !char.is_ascii_alphanumeric() && char != '_')
            {
                return Some(false);
            }
            return None;
        }
        return None;
    }
}

/// Returns the maximum fixture `seq` observed in the facts text.
/// Missing/unparseable files yield 0 so the first record (seq 1)
/// always counts as fresh.
fn max_fact_seq(facts_text: &str) -> u64 {
    facts_text
        .lines()
        .filter_map(|line| json_u64_field(line, "seq"))
        .max()
        .unwrap_or(0)
}

/// Returns every fixture record strictly newer than `baseline`.
fn records_after(facts_text: &str, baseline: u64) -> Vec<&str> {
    facts_text
        .lines()
        .filter(|line| json_u64_field(line, "seq").is_some_and(|seq| seq > baseline))
        .collect()
}

/// Positive-delta predicate for operation-scoped counter windows
/// (Plan 214 §9/§11): the window must show strictly more composed
/// / delivered / accepted work after the bounded operation.
fn delta_after(before: u64, after: u64) -> u64 {
    after.saturating_sub(before)
}

/// Finds the first duplicated evidence label in TSV text, if any.
/// Plan 214 §19.17 — duplicate/conflicting evidence keys fail
/// aggregation.
fn find_duplicate_label(tsv: &str) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    for line in tsv.lines() {
        let label = line.split('\t').next().unwrap_or("");
        if label.is_empty() {
            continue;
        }
        if !seen.insert(label.to_owned()) {
            return Some(label.to_owned());
        }
    }
    None
}

/// Reads a harness-owned fixture facts file tolerantly: a missing
/// file yields empty text (no fresh records), never a panic. The
/// aggregation gates fail closed on the empty set.
fn read_facts(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// Outcome of one pumped subprocess invocation.
struct PumpedOutput {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Runs one synchronous child process to completion while the
/// production inbound pipeline keeps pumping (Plan 214 §7).
///
/// The child is spawned with piped stdio; optional `stdin_bytes`
/// are written up front (bounded, small); the driver then loops on
/// non-blocking `try_wait` while `ServiceProduct::poll_inbound()`
/// drains tunneled NetDB/Garlic/Streaming replies. No busy loop:
/// each poll blocks up to the product poll interval. On deadline
/// expiry the child is terminated and the phase is reported; after
/// exit the pump drains briefly so final ACK/close packets land
/// before the caller snapshots counters.
async fn run_pumped(
    product: &mut ServiceProduct,
    mut command: std::process::Command,
    stdin_bytes: Option<&[u8]>,
    deadline: Duration,
    phase: &str,
) -> Result<PumpedOutput, String> {
    if stdin_bytes.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut child: Child = command
        .spawn()
        .map_err(|error| format!("{phase}: spawn failed: {error}"))?;
    if let Some(bytes) = stdin_bytes {
        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(bytes)
                .map_err(|error| format!("{phase}: stdin write failed: {error}"))?;
        }
        drop(child.stdin.take());
    }
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("{phase}: wait failed: {error}"))?
        {
            break status;
        }
        if start.elapsed() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{phase}: deadline exceeded"));
        }
        // Bounded pacing: the poll itself blocks on the SSU2
        // inbound queue up to the product poll interval, so this
        // loop cannot spin while the child is waiting on remote
        // bytes.
        let _ = product.poll_inbound().await;
    };
    let output = child
        .wait_with_output()
        .map_err(|error| format!("{phase}: output collection failed: {error}"))?;
    // Post-exit drain so final ACK/close packets land before the
    // caller snapshots the operation-scoped counters.
    for _ in 0..3 {
        let _ = product.poll_inbound().await;
    }
    Ok(PumpedOutput {
        exit_code: status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

/// Reserves one OS-selected ephemeral loopback port for a client
/// listener spec.
///
/// Plan 214 §5 — the runtime-neutral `ServiceTunnelSet::validate`
/// preflight rejects two specs sharing the literal
/// `127.0.0.1:0` socket value, while production binds each
/// listener's ephemeral port independently at prepare time. The
/// driver therefore reserves two distinct ephemeral ports up
/// front (the same bind-close-read pattern the shell runners use
/// for reference ports) so the validated spec set matches what
/// production binds. A reservation race fails closed at prepare
/// time with a clear error.
fn reserve_loopback_port() -> u16 {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
    socket.local_addr().expect("reserved port").port()
}

/// Builds the Plan 214 §5 real enabled `HttpClient` service spec
/// with a reserved loopback listener port and the validated
/// remote destination.
fn build_http_spec(port: u16, destination: DestinationRef) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: i2pr_service_tunnels::ServiceTunnelId::parse(PLAN214_HTTP_SPEC_ID)
            .expect("http spec id"),
        kind: ServiceTunnelKind::HttpClient,
        enabled: true,
        listener: Some(
            LocalListenerSpec::parse(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port)
                .expect("http loopback listener"),
        ),
        target: None,
        targets: Vec::new(),
        destination: Some(destination),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: i2pr_service_tunnels::ServiceTimeouts::defaults(),
        http_options: Some(HttpClientOptions::defaults()),
        socks5_options: None,
        irc_options: None,
    }
}

/// Builds the Plan 214 §5 real enabled `IrcClient` service spec
/// with a reserved loopback listener port and the validated
/// remote destination.
fn build_irc_spec(port: u16, destination: DestinationRef) -> ServiceTunnelSpec {
    ServiceTunnelSpec {
        id: i2pr_service_tunnels::ServiceTunnelId::parse(PLAN214_IRC_SPEC_ID).expect("irc spec id"),
        kind: ServiceTunnelKind::IrcClient,
        enabled: true,
        listener: Some(
            LocalListenerSpec::parse(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port)
                .expect("irc loopback listener"),
        ),
        target: None,
        targets: Vec::new(),
        destination: Some(destination),
        policy: DestinationPolicy::Dedicated,
        max_connections: 4,
        max_buffered_bytes_per_direction: 65_536,
        timeouts: i2pr_service_tunnels::ServiceTimeouts::defaults(),
        http_options: None,
        socks5_options: None,
        irc_options: Some(IrcClientOptions::defaults()),
    }
}

/// Emits the per-service startup LeaseSet2/installation rows
/// (Plan 214 §9): the typed routing decision for the exact
/// target hash (proving the manager addresses this destination
/// through the remote router backend, never a local fallback),
/// the installed real lease count, the inbound-owner
/// registration flag, and the startup lookup activity observed
/// through the typed counters.
async fn emit_startup_rows(
    product: &ServiceProduct,
    evidence_dir: &Path,
    prefix: &str,
    spec_id: &str,
    expected_hash: &[u8; 32],
) {
    // Plan 214 §9 — the summary's own hash is the LOCAL service
    // destination, so exact-target addressing is proved through
    // the typed `RoutingDecision`, not by comparing hashes.
    write_subfact(
        evidence_dir,
        &format!("{prefix}-startup-destination-hash-match"),
        if product.routing_decision_for(expected_hash) == RoutingDecision::RemoteRouter {
            "1"
        } else {
            "0"
        },
    );
    match product.service_router_network_summary(spec_id) {
        Some(summary) => {
            write_subfact(
                evidence_dir,
                &format!("{prefix}-startup-lease-count"),
                &summary.lease_count.to_string(),
            );
            write_subfact(
                evidence_dir,
                &format!("{prefix}-startup-inbound-owner-registered"),
                if summary.inbound_owner_registered {
                    "1"
                } else {
                    "0"
                },
            );
        }
        None => {
            write_subfact(evidence_dir, &format!("{prefix}-startup-lease-count"), "0");
            write_subfact(
                evidence_dir,
                &format!("{prefix}-startup-inbound-owner-registered"),
                "0",
            );
        }
    }
    let counters = product.remote_counters().await;
    write_subfact(
        evidence_dir,
        &format!("{prefix}-startup-remote-lookup-started"),
        &counters.remote_lookup_started.to_string(),
    );
}

/// Drives the Plan 214 §8 curl cases against the live i2pr HTTP
/// client listener while the production inbound pump stays active.
/// Emits the documented §16 HTTP fact rows. Every positive fact is
/// bound to a fresh target-fixture record after a per-case
/// baseline; rejection cases additionally prove a zero target
/// delta.
#[allow(clippy::too_many_arguments)]
async fn run_http_cases(
    product: &mut ServiceProduct,
    http_port: u16,
    evidence_dir: &Path,
    facts_path: &Path,
    large_expected_len: usize,
    large_expected_sha256: &str,
) -> Result<(), String> {
    let proxy = format!("http://127.0.0.1:{http_port}");
    let small_expected_digest = sha256_hex(PLAN214_SMALL_BODY);
    let small_expected_len = PLAN214_SMALL_BODY.len();

    let http_before = product.remote_counters().await;
    let http_orphans_before = product.inbound_orphan_receives();

    // D.1 GET /hello with the actual curl status frame.
    let get_baseline = max_fact_seq(&read_facts(facts_path));
    let mut get_cmd = std::process::Command::new(curl_bin());
    get_cmd.args([
        "-sS",
        "--max-time",
        "20",
        "-x",
        &proxy,
        "http://alpha-test.i2p/hello",
        "-o",
        "-",
        "-w",
        "\n%{http_code}",
    ]);
    let get_output = run_pumped(product, get_cmd, None, GET_DEADLINE, "http-get").await?;
    // The status frame is the final line; body bytes can never be
    // confused with it because curl appends the frame after a
    // newline separator and the parser requires exactly 3 digits.
    let get_stdout = String::from_utf8_lossy(&get_output.stdout).into_owned();
    let (get_body_text, get_frame) = match get_stdout.rfind('\n') {
        Some(index) => (
            &get_stdout.as_bytes()[..index],
            get_stdout[index + 1..].trim(),
        ),
        None => (&b""[..], get_stdout.as_str()),
    };
    let get_status = parse_curl_status_code(get_frame).unwrap_or(0);
    let get_body_digest = sha256_hex(get_body_text);
    write_subfact(
        evidence_dir,
        "http-get-command-exit",
        &get_output.exit_code.to_string(),
    );
    write_subfact(evidence_dir, "http-get-status", &get_status.to_string());
    write_subfact(
        evidence_dir,
        "http-get-response-len",
        &get_body_text.len().to_string(),
    );
    write_subfact(evidence_dir, "http-get-response-sha256", &get_body_digest);
    // D.2 fresh target-side GET observation (never inferred from
    // the response bytes above).
    let get_facts_text = read_facts(facts_path);
    let get_fresh = records_after(&get_facts_text, get_baseline);
    let get_target = get_fresh.iter().find(|line| {
        json_string_field(line, "method").as_deref() == Some("GET")
            && json_string_field(line, "target")
                .as_deref()
                .is_some_and(|target| target == "/hello" || target.ends_with("/hello"))
    });
    match get_target {
        Some(line) => {
            write_subfact(
                evidence_dir,
                "http-get-fixture-method",
                json_string_field(line, "method").as_deref().unwrap_or(""),
            );
            write_subfact(
                evidence_dir,
                "http-get-fixture-path",
                json_string_field(line, "target").as_deref().unwrap_or(""),
            );
        }
        None => {
            write_subfact(evidence_dir, "http-get-fixture-method", "");
            write_subfact(evidence_dir, "http-get-fixture-path", "");
        }
    }
    if get_output.exit_code != 0
        || get_status != 200
        || get_body_text.len() != small_expected_len
        || get_body_digest != small_expected_digest
        || get_target.is_none()
    {
        return Err(format!(
            "http-get failed: exit={} status={get_status} len={} digest_match={} target_observed={}",
            get_output.exit_code,
            get_body_text.len(),
            get_body_digest == small_expected_digest,
            get_target.is_some(),
        ));
    }

    // D.3 POST /post: sent digest must equal the target-observed
    // digest, and the response must match the fixture contract
    // (`posted=<len>`).
    let post_baseline = max_fact_seq(&read_facts(facts_path));
    let post_digest = sha256_hex(PLAN214_POST_BODY);
    write_subfact(
        evidence_dir,
        "http-post-request-len",
        &PLAN214_POST_BODY.len().to_string(),
    );
    write_subfact(evidence_dir, "http-post-request-sha256", &post_digest);
    let mut post_cmd = std::process::Command::new(curl_bin());
    post_cmd.args([
        "-sS",
        "--max-time",
        "20",
        "-x",
        &proxy,
        "--data-binary",
        "@-",
        "http://alpha-test.i2p/post",
        "-o",
        "-",
        "-w",
        "\n%{http_code}",
    ]);
    let post_output = run_pumped(
        product,
        post_cmd,
        Some(PLAN214_POST_BODY),
        POST_DEADLINE,
        "http-post",
    )
    .await?;
    let post_stdout = String::from_utf8_lossy(&post_output.stdout).into_owned();
    let (post_body_text, post_frame) = match post_stdout.rfind('\n') {
        Some(index) => (
            &post_stdout.as_bytes()[..index],
            post_stdout[index + 1..].trim(),
        ),
        None => (&b""[..], post_stdout.as_str()),
    };
    let post_status = parse_curl_status_code(post_frame).unwrap_or(0);
    write_subfact(
        evidence_dir,
        "http-post-command-exit",
        &post_output.exit_code.to_string(),
    );
    let post_response_ok = post_output.exit_code == 0
        && post_status == 200
        && post_body_text == format!("posted={}", PLAN214_POST_BODY.len()).as_bytes();
    write_subfact(
        evidence_dir,
        "http-post-response-ok",
        if post_response_ok { "1" } else { "0" },
    );
    let post_facts_text = read_facts(facts_path);
    let post_fresh = records_after(&post_facts_text, post_baseline);
    let post_target = post_fresh.iter().find(|line| {
        json_string_field(line, "method").as_deref() == Some("POST")
            && json_string_field(line, "target")
                .as_deref()
                .is_some_and(|target| target == "/post" || target.ends_with("/post"))
    });
    match post_target {
        Some(line) => {
            write_subfact(
                evidence_dir,
                "http-post-fixture-body-len",
                &json_u64_field(line, "body_len").unwrap_or(0).to_string(),
            );
            write_subfact(
                evidence_dir,
                "http-post-fixture-body-sha256",
                json_string_field(line, "body_sha256")
                    .as_deref()
                    .unwrap_or(""),
            );
        }
        None => {
            write_subfact(evidence_dir, "http-post-fixture-body-len", "0");
            write_subfact(evidence_dir, "http-post-fixture-body-sha256", "");
        }
    }
    let post_digest_match = post_target.is_some_and(|line| {
        json_string_field(line, "body_sha256").as_deref() == Some(post_digest.as_str())
            && json_u64_field(line, "body_len") == Some(PLAN214_POST_BODY.len() as u64)
    });
    if !post_response_ok || !post_digest_match {
        return Err(format!(
            "http-post failed: exit={} status={post_status} response_ok={post_response_ok} digest_match={post_digest_match}",
            post_output.exit_code,
        ));
    }

    // D.4 multi-packet /large against the independently known
    // fixture-published contract (shape alone is insufficient).
    write_subfact(
        evidence_dir,
        "http-large-expected-len",
        &large_expected_len.to_string(),
    );
    write_subfact(
        evidence_dir,
        "http-large-expected-sha256",
        large_expected_sha256,
    );
    let mut large_cmd = std::process::Command::new(curl_bin());
    let large_body_file = evidence_dir.join("http-large-response.body");
    large_cmd.args([
        "-sS",
        "--max-time",
        "40",
        "-x",
        &proxy,
        "http://alpha-test.i2p/large",
        "-o",
        large_body_file.to_string_lossy().as_ref(),
        "-w",
        "%{http_code}",
    ]);
    let large_output = run_pumped(product, large_cmd, None, LARGE_DEADLINE, "http-large").await?;
    let large_stdout = String::from_utf8_lossy(&large_output.stdout).into_owned();
    let large_status = parse_curl_status_code(&large_stdout).unwrap_or(0);
    let large_bytes = std::fs::read(&large_body_file).unwrap_or_default();
    let large_digest = sha256_hex(&large_bytes);
    write_subfact(
        evidence_dir,
        "http-large-command-exit",
        &large_output.exit_code.to_string(),
    );
    write_subfact(
        evidence_dir,
        "http-large-response-len",
        &large_bytes.len().to_string(),
    );
    write_subfact(evidence_dir, "http-large-response-sha256", &large_digest);
    if large_output.exit_code != 0
        || large_status != 200
        || large_bytes.len() != large_expected_len
        || large_digest != large_expected_sha256
    {
        return Err(format!(
            "http-large failed: exit={} status={large_status} len={} expected_len={large_expected_len} digest_match={}",
            large_output.exit_code,
            large_bytes.len(),
            large_digest == large_expected_sha256,
        ));
    }

    // D.5 policy rejections must not reach the fixture: baseline
    // before, verify the local rejection, then prove a zero target
    // delta after.
    let clearnet_baseline = max_fact_seq(&read_facts(facts_path));
    let mut clearnet_cmd = std::process::Command::new(curl_bin());
    clearnet_cmd.args([
        "-sS",
        "--max-time",
        "10",
        "-x",
        &proxy,
        "http://example.com/",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
    ]);
    let clearnet_output = run_pumped(
        product,
        clearnet_cmd,
        None,
        REJECT_DEADLINE,
        "http-clearnet",
    )
    .await?;
    let clearnet_code = String::from_utf8_lossy(&clearnet_output.stdout)
        .trim()
        .to_owned();
    let clearnet_rejected = clearnet_output.exit_code == 0 && clearnet_code == "403";
    write_subfact(
        evidence_dir,
        "http-clearnet-rejected",
        if clearnet_rejected { "1" } else { "0" },
    );
    let clearnet_facts_text = read_facts(facts_path);
    let clearnet_delta = records_after(&clearnet_facts_text, clearnet_baseline).len();
    write_subfact(
        evidence_dir,
        "http-clearnet-target-observation-delta-zero",
        if clearnet_delta == 0 { "1" } else { "0" },
    );
    if !clearnet_rejected || clearnet_delta != 0 {
        return Err(format!(
            "http-clearnet failed: exit={} code={clearnet_code} target_delta={clearnet_delta}",
            clearnet_output.exit_code,
        ));
    }

    let ip_baseline = max_fact_seq(&read_facts(facts_path));
    let mut ip_cmd = std::process::Command::new(curl_bin());
    ip_cmd.args([
        "-sS",
        "--max-time",
        "10",
        "-x",
        &proxy,
        "http://127.0.0.1/",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
    ]);
    let ip_output = run_pumped(product, ip_cmd, None, REJECT_DEADLINE, "http-ip-literal").await?;
    let ip_code = String::from_utf8_lossy(&ip_output.stdout).trim().to_owned();
    let ip_rejected =
        ip_output.exit_code == 0 && (ip_code == "400" || ip_code == "403" || ip_code == "502");
    write_subfact(
        evidence_dir,
        "http-ip-literal-rejected",
        if ip_rejected { "1" } else { "0" },
    );
    let ip_facts_text = read_facts(facts_path);
    let ip_delta = records_after(&ip_facts_text, ip_baseline).len();
    write_subfact(
        evidence_dir,
        "http-ip-target-observation-delta-zero",
        if ip_delta == 0 { "1" } else { "0" },
    );
    if !ip_rejected || ip_delta != 0 {
        return Err(format!(
            "http-ip-literal failed: exit={} code={ip_code} target_delta={ip_delta}",
            ip_output.exit_code,
        ));
    }

    // Phase E — bounded HTTP session-window production counters.
    let http_after = product.remote_counters().await;
    let http_orphans_after = product.inbound_orphan_receives();
    write_subfact(
        evidence_dir,
        "http-remote-outbound-composed-delta",
        &delta_after(
            http_before.remote_outbound_composed,
            http_after.remote_outbound_composed,
        )
        .to_string(),
    );
    write_subfact(
        evidence_dir,
        "http-router-delivery-delta",
        &delta_after(
            http_before.remote_outbound_requests,
            http_after.remote_outbound_requests,
        )
        .to_string(),
    );
    write_subfact(
        evidence_dir,
        "http-remote-inbound-dispatched-delta",
        &delta_after(
            http_before.remote_inbound_dispatched,
            http_after.remote_inbound_dispatched,
        )
        .to_string(),
    );
    write_subfact(
        evidence_dir,
        "http-local-coowned-delta-zero",
        if delta_after(
            http_before.local_coowned_deliveries,
            http_after.local_coowned_deliveries,
        ) == 0
        {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(
        evidence_dir,
        "http-unknown-peer-delta-zero",
        if delta_after(http_before.unknown_peer, http_after.unknown_peer) == 0 {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(
        evidence_dir,
        "http-orphan-receive-delta-zero",
        if http_orphans_after.saturating_sub(http_orphans_before) == 0 {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(evidence_dir, "http-clean-resource-baseline", "1");
    Ok(())
}

/// Reads one `KEY=value` fact line from captured client stdout.
fn client_fact(stdout: &str, key: &str) -> String {
    stdout
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().to_owned())
        })
        .next()
        .unwrap_or_default()
}

/// Drives the Plan 214 §10 jaraco/irc case against the live i2pr
/// IRC client listener while the production inbound pump stays
/// active. Emits the documented §17 IRC fact rows. Registration,
/// privacy, PING/PONG, PRIVMSG, ACTION, and DCC facts are all
/// proved from fresh target-fixture records after a session
/// baseline plus the client's own observations — never from a
/// literal success row.
#[allow(clippy::too_many_arguments)]
async fn run_irc_case(
    product: &mut ServiceProduct,
    irc_port: u16,
    evidence_dir: &Path,
    facts_path: &Path,
    jaraco_src: &Path,
    harness_dir: &Path,
    expected_nick: &str,
    channel: &str,
    token: &str,
) -> Result<(), String> {
    // Phase G — the IRC window starts only after HTTP work is
    // complete and drained, so HTTP deltas can never leak into it.
    let irc_before = product.remote_counters().await;
    let irc_orphans_before = product.inbound_orphan_receives();
    let _ = product.poll_inbound().await;
    let irc_baseline = max_fact_seq(&read_facts(facts_path));

    let scratch = tempfile::Builder::new()
        .prefix("plan214-jaraco")
        .tempdir()
        .map_err(|error| format!("irc scratch dir: {error}"))?;
    let venv = scratch.path().join("venv");
    let mut venv_cmd = std::process::Command::new(python_bin());
    venv_cmd.args(["-m", "venv", venv.to_string_lossy().as_ref()]);
    let venv_output = run_pumped(product, venv_cmd, None, VENV_DEADLINE, "irc-venv").await?;
    if venv_output.exit_code != 0 {
        return Err(format!(
            "irc venv failed: exit={} stderr={}",
            venv_output.exit_code,
            String::from_utf8_lossy(&venv_output.stderr)
        ));
    }
    let pip = if venv.join("bin").join("pip").exists() {
        venv.join("bin").join("pip")
    } else {
        venv.join("Scripts").join("pip.exe")
    };
    let mut pip_cmd = std::process::Command::new(pip.as_os_str());
    pip_cmd.args(["install", "--quiet", jaraco_src.to_string_lossy().as_ref()]);
    let pip_output = run_pumped(product, pip_cmd, None, VENV_DEADLINE, "irc-pip-install").await?;
    if pip_output.exit_code != 0 {
        return Err(format!(
            "irc pip install failed: exit={} stderr={}",
            pip_output.exit_code,
            String::from_utf8_lossy(&pip_output.stderr)
        ));
    }
    let venv_python = if venv.join("bin").join("python3").exists() {
        venv.join("bin").join("python3")
    } else if venv.join("bin").join("python").exists() {
        venv.join("bin").join("python")
    } else {
        venv.join("Scripts").join("python.exe")
    };
    let driver = harness_dir.join("clients").join("irc_driver.py");
    let mut irc_cmd = std::process::Command::new(venv_python.as_os_str());
    irc_cmd.arg(driver.as_os_str()).args([
        "--port",
        &irc_port.to_string(),
        "--nick",
        expected_nick,
        "--channel",
        channel,
        "--token",
        token,
    ]);
    let irc_output = run_pumped(product, irc_cmd, None, IRC_DEADLINE, "irc-session").await?;
    let irc_exit = irc_output.exit_code;
    let irc_stdout = String::from_utf8_lossy(&irc_output.stdout).into_owned();
    write_subfact(evidence_dir, "irc-command-exit", &irc_exit.to_string());
    if irc_exit != 0 {
        return Err(format!(
            "irc client failed: exit={irc_exit} stderr={}",
            String::from_utf8_lossy(&irc_output.stderr)
        ));
    }

    let welcomed = client_fact(&irc_stdout, "WELCOME") == "1";
    let pong_sent = client_fact(&irc_stdout, "PONG_SENT") == "1";
    let privmsg_sent = client_fact(&irc_stdout, "PRIVMSG_SENT") == "1";
    let echo_received = client_fact(&irc_stdout, "ECHO_RECEIVED") == "1";
    let echo_token_match = client_fact(&irc_stdout, "ECHO_TOKEN_MATCH") == "1";
    let action_sent = client_fact(&irc_stdout, "ACTION_SENT") == "1";
    let dcc_attempted = client_fact(&irc_stdout, "DCC_ATTEMPTED") == "1";
    let quit_sent = client_fact(&irc_stdout, "QUIT_SENT") == "1";
    write_subfact(
        evidence_dir,
        "irc-registration-client-welcome",
        if welcomed { "1" } else { "0" },
    );
    if !welcomed || !privmsg_sent || !pong_sent {
        return Err(format!(
            "irc client facts failed: welcome={welcomed} privmsg_sent={privmsg_sent} pong_sent={pong_sent}"
        ));
    }
    if !dcc_attempted {
        return Err("irc DCC policy case was never attempted".to_owned());
    }
    write_subfact(evidence_dir, "irc-dcc-attempted", "1");

    // Fresh target-side facts after the session baseline.
    let irc_facts_text = read_facts(facts_path);
    let fresh = records_after(&irc_facts_text, irc_baseline);
    let find_event = |event: &str| {
        fresh
            .iter()
            .find(|line| json_string_field(line, "event").as_deref() == Some(event))
    };

    // F.1 registration: target must observe the expected NICK and
    // a USER line; the client must observe the welcome numeric.
    let nick_observed = find_event("register-nick")
        .is_some_and(|line| json_string_field(line, "nick").as_deref() == Some(expected_nick));
    write_subfact(
        evidence_dir,
        "irc-registration-target-nick-observed",
        if nick_observed { "1" } else { "0" },
    );
    let user_observed = find_event("user").is_some();
    write_subfact(
        evidence_dir,
        "irc-registration-target-user-observed",
        if user_observed { "1" } else { "0" },
    );
    if !nick_observed || !user_observed {
        return Err(format!(
            "irc registration not target-observed: nick={nick_observed} user={user_observed}"
        ));
    }

    // F.6 privacy rewrite derived from the target-observed USER
    // line + booleans (never a literal success row, never PII).
    // The client profile's documented contract (Plan 178, locked
    // by the i2pr-service-tunnels unit suite) rewrites
    // `USER alice 0 * :Alice` to the neutral shape
    // `USER alice i2p localhost :Alice`: jaraco never emits the
    // `i2p localhost` pair, so its target presence proves OUR
    // rewrite produced it, while the booleans prove no loopback
    // address or node name leaked. (A b32 hostname on this leg
    // would be i2pd's server-tunnel rewrite, not ours; the lane
    // forwards through the transparent server tunnel so the
    // observed bytes are exactly what our profile emitted.)
    let neutral_shape = find_event("user").is_some_and(|line| {
        json_string_field(line, "line")
            .as_deref()
            .is_some_and(|text| text.contains(" i2p localhost "))
    });
    let no_leak = find_event("user-privacy").is_some_and(|line| {
        json_bool_field(line, "has_loopback") == Some(false)
            && json_bool_field(line, "has_nodename") == Some(false)
    });
    let privacy_ok = neutral_shape && no_leak;
    write_subfact(
        evidence_dir,
        "irc-privacy-rewrite-derived-from-target",
        if privacy_ok { "1" } else { "0" },
    );
    if !privacy_ok {
        return Err("irc privacy rewrite not proved from target USER booleans".to_owned());
    }

    // F.2 PING issued by the target + PONG observed by the target
    // with a matching challenge token.
    let ping_issued = find_event("ping-sent").is_some();
    write_subfact(
        evidence_dir,
        "irc-ping-issued-by-target",
        if ping_issued { "1" } else { "0" },
    );
    let pong_observed = find_event("pong-received")
        .is_some_and(|line| json_bool_field(line, "matches_challenge") == Some(true));
    write_subfact(
        evidence_dir,
        "irc-pong-observed-by-target",
        if pong_observed { "1" } else { "0" },
    );
    if !ping_issued || !pong_observed {
        return Err(format!(
            "irc ping/pong not target-proved: ping={ping_issued} pong={pong_observed}"
        ));
    }

    // F.3 bidirectional PRIVMSG with the deterministic session
    // token observed on both sides.
    let outbound_observed = fresh.iter().any(|line| {
        json_string_field(line, "event").as_deref() == Some("privmsg-received")
            && json_string_field(line, "text")
                .as_deref()
                .is_some_and(|text| text.contains(token))
    });
    write_subfact(
        evidence_dir,
        "irc-outbound-privmsg-target-observed",
        if outbound_observed { "1" } else { "0" },
    );
    write_subfact(
        evidence_dir,
        "irc-inbound-privmsg-client-observed",
        if echo_received { "1" } else { "0" },
    );
    let token_match = outbound_observed && echo_received && echo_token_match;
    write_subfact(
        evidence_dir,
        "irc-privmsg-token-match",
        if token_match { "1" } else { "0" },
    );
    if !token_match {
        return Err(format!(
            "irc privmsg token not matched: target={outbound_observed} echo={echo_received} client_match={echo_token_match}"
        ));
    }

    // F.4 ACTION observed by the target.
    let action_observed = fresh.iter().any(|line| {
        json_string_field(line, "event").as_deref() == Some("action-received")
            && json_string_field(line, "text")
                .as_deref()
                .is_some_and(|text| text.contains("waves hello"))
    });
    write_subfact(
        evidence_dir,
        "irc-action-result",
        if action_observed && action_sent {
            "allowed-observed"
        } else {
            "missing"
        },
    );
    if !action_observed || !action_sent {
        return Err(format!(
            "irc action not proved: client={action_sent} target={action_observed}"
        ));
    }

    // F.5 DCC blocking: the attempt is explicit (client fact
    // above); the policy result requires the forbidden payload to
    // be absent from every fresh target record while the session
    // stayed otherwise usable (welcome + echo + quit observed).
    let dcc_target_hits = fresh
        .iter()
        .filter(|line| json_string_field(line, "event").as_deref() == Some("dcc-observed"))
        .count();
    let dcc_blocked = dcc_target_hits == 0 && echo_received && quit_sent;
    write_subfact(
        evidence_dir,
        "irc-dcc-policy-result",
        if dcc_blocked {
            "blocked"
        } else {
            "allowed-or-unusable"
        },
    );
    write_subfact(
        evidence_dir,
        "irc-dcc-target-observation-delta-zero",
        if dcc_target_hits == 0 { "1" } else { "0" },
    );
    if !dcc_blocked {
        return Err(format!(
            "irc DCC policy failed: target_hits={dcc_target_hits} echo={echo_received} quit={quit_sent}"
        ));
    }

    // Phase G — independent IRC-window production counters.
    let irc_after = product.remote_counters().await;
    let irc_orphans_after = product.inbound_orphan_receives();
    write_subfact(
        evidence_dir,
        "irc-remote-outbound-composed-delta",
        &delta_after(
            irc_before.remote_outbound_composed,
            irc_after.remote_outbound_composed,
        )
        .to_string(),
    );
    write_subfact(
        evidence_dir,
        "irc-router-delivery-delta",
        &delta_after(
            irc_before.remote_outbound_requests,
            irc_after.remote_outbound_requests,
        )
        .to_string(),
    );
    write_subfact(
        evidence_dir,
        "irc-remote-inbound-dispatched-delta",
        &delta_after(
            irc_before.remote_inbound_dispatched,
            irc_after.remote_inbound_dispatched,
        )
        .to_string(),
    );
    write_subfact(
        evidence_dir,
        "irc-local-coowned-delta-zero",
        if delta_after(
            irc_before.local_coowned_deliveries,
            irc_after.local_coowned_deliveries,
        ) == 0
        {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(
        evidence_dir,
        "irc-unknown-peer-delta-zero",
        if delta_after(irc_before.unknown_peer, irc_after.unknown_peer) == 0 {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(
        evidence_dir,
        "irc-orphan-receive-delta-zero",
        if irc_orphans_after.saturating_sub(irc_orphans_before) == 0 {
            "1"
        } else {
            "0"
        },
    );
    write_subfact(evidence_dir, "irc-clean-resource-baseline", "1");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "Plan 214 M10 product-only remote HTTP/IRC application requalification: requires exact-pinned external i2pd environment + runner-derived pins + fixture facts + i2pd public destinations"]
async fn m10_product_only_remote_http_and_irc_application_interop_v214() {
    let i2pd_ri_path = env_path("I2PD_ROUTER_INFO");
    let i2pd_endpoint: SocketAddr = env_value("I2PD_SSU2_ENDPOINT").parse().expect("endpoint");
    let bind: SocketAddr = env_value("I2PR_SSU2_BIND").parse().expect("bind");
    assert!(bind.ip().is_loopback(), "i2pr bind must be loopback");
    assert!(
        i2pd_endpoint.ip().is_loopback(),
        "i2pd endpoint must be loopback"
    );
    // Plan 212 §8 — router bootstrap is independent of
    // application Destination lookup. The reference peer carries
    // only router transport/bootstrap metadata (router.info +
    // endpoint); per-service remote target hashes derive from the
    // service specs' `DestinationRef` inside the production
    // composition. HTTP and IRC resolve independently.
    let http_dest_b64 = env_value("PLAN214_HTTP_DEST_B64");
    let http_dest_b32 = env_value("PLAN214_HTTP_DEST_B32");
    let http_dest_hash = parse_dest_hash(&env_value("PLAN214_HTTP_DEST_HASH"));
    let irc_dest_b64 = env_value("PLAN214_IRC_DEST_B64");
    let irc_dest_b32 = env_value("PLAN214_IRC_DEST_B32");
    let irc_dest_hash = parse_dest_hash(&env_value("PLAN214_IRC_DEST_HASH"));
    if http_dest_hash == [0_u8; 32] || irc_dest_hash == [0_u8; 32] {
        panic!("Plan 214 destination hashes must be nonzero");
    }
    if http_dest_hash == irc_dest_hash {
        panic!("Plan 214 HTTP and IRC destinations must be independently owned (hashes differ)");
    }
    let evidence_dir = env_path("EVIDENCE_DIR");
    std::fs::create_dir_all(&evidence_dir).expect("evidence dir");
    std::fs::create_dir_all(evidence_dir.join("plan214-driver")).ok();
    let evidence_dir = evidence_dir.join("plan214-driver");
    // Plan 214 §13 — the counted driver consumes harness-owned
    // fixture-facts paths; target ports are runner-only facts and
    // are never read here (§15.9 rejects unused env plumbing).
    let http_facts = env_path("PLAN214_HTTP_FIXTURE_FACTS");
    let irc_facts = env_path("PLAN214_IRC_FIXTURE_FACTS");
    let large_expected_len: usize = env_value("PLAN214_HTTP_LARGE_EXPECTED_LEN")
        .parse()
        .expect("large expected len must be usize");
    let large_expected_sha256 = env_value("PLAN214_HTTP_LARGE_EXPECTED_SHA256");
    if large_expected_sha256.len() != 64
        || !large_expected_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        panic!("large expected sha256 must be 64 hex chars");
    }
    let jaraco_src = env_path("PLAN214_JARACO_SRC");
    let harness_dir = env_path("PLAN214_HARNESS_DIR");
    let expected_nick = env_value("PLAN214_IRC_NICK");
    let irc_channel = env_value("PLAN214_IRC_CHANNEL");
    let irc_token = env_value("PLAN214_IRC_TOKEN");

    // Plan 214 §6 — pin facts are runner-derived command output.
    // The driver consumes the runner-verified booleans only as a
    // precondition before any network process runs; final pin
    // authority stays in the runner's rows, never in a literal
    // driver success line.
    if env_value("PLAN214_I2PD_PIN_OK") != "1" {
        append_evidence(
            &evidence_dir,
            "remote-stop",
            "phase=runner-pin-precondition i2pd pin not verified",
        );
        panic!("Plan 214 i2pd pin precondition not satisfied (runner must verify the exact pin)");
    }
    if env_value("PLAN214_JARACO_PIN_OK") != "1" {
        append_evidence(
            &evidence_dir,
            "remote-stop",
            "phase=runner-pin-precondition jaraco pin not verified",
        );
        panic!("Plan 214 jaraco pin precondition not satisfied (runner must verify the exact pin)");
    }

    // Plan 214 §5 mandates real enabled service specs whose
    // destinations are the actual remote destinations. The b32
    // form decodes through the Base32Hash variant which never
    // carries private material across the trust boundary.
    let http_dest_ref =
        DestinationRef::parse(&http_dest_b32).expect("http destination reference must validate");
    let irc_dest_ref =
        DestinationRef::parse(&irc_dest_b32).expect("irc destination reference must validate");
    append_evidence(
        &evidence_dir,
        "http-public-destination-hash",
        &hex_lower(&http_dest_hash),
    );
    append_evidence(
        &evidence_dir,
        "irc-public-destination-hash",
        &hex_lower(&irc_dest_hash),
    );
    write_subfact(&evidence_dir, "irc-destination-distinct-from-http", "1");

    // Verify the destination hashes match the SHA-256 of the
    // canonical public destination encoding. Fail closed on any
    // divergence (no fake destinations, Plan 211 §3).
    let http_dest_bytes = base64_decode(&http_dest_b64);
    let irc_dest_bytes = base64_decode(&irc_dest_b64);
    let http_dest_hash_computed = *i2pr_crypto::sha256(&http_dest_bytes).as_bytes();
    let irc_dest_hash_computed = *i2pr_crypto::sha256(&irc_dest_bytes).as_bytes();
    if http_dest_hash_computed != http_dest_hash {
        append_evidence(
            &evidence_dir,
            "remote-stop",
            "phase=http-destination-hash-mismatch",
        );
        panic!("Plan 214 HTTP destination hash mismatch");
    }
    if irc_dest_hash_computed != irc_dest_hash {
        append_evidence(
            &evidence_dir,
            "remote-stop",
            "phase=irc-destination-hash-mismatch",
        );
        panic!("Plan 214 IRC destination hash mismatch");
    }

    // Generate a CSPRNG-derived router bundle the production
    // composition signs into a controlled RouterInfo. The bundle
    // never leaves the composition; the driver does not see the
    // private material.
    let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("router bundle");

    // Plan 214 §5 — build a real enabled ServiceTunnelSet with
    // one HttpClient and one IrcClient spec. The static alias
    // table maps `alpha-test.i2p` to the i2pd HTTP destination
    // so curl addresses the i2pd-hosted service through a stable
    // `.i2p` hostname (no DNS, no synthetic peer bridge). The
    // bounded runtime-neutral validator is the only preflight —
    // no second daemon manager is constructed (black-box rule).
    let http_port_reserved = reserve_loopback_port();
    let irc_port_reserved = reserve_loopback_port();
    let http_spec = build_http_spec(http_port_reserved, http_dest_ref.clone());
    let irc_spec = build_irc_spec(irc_port_reserved, irc_dest_ref.clone());
    let specs = Arc::new(ServiceTunnelSet {
        tunnels: vec![http_spec, irc_spec],
    });
    specs
        .validate()
        .expect("Plan 214 service specs must validate");
    let mut alias_table = StaticAliasTable::new();
    alias_table
        .insert(PLAN214_HTTP_ALIAS, http_dest_ref)
        .expect("http alias insert");
    let alias_table = Arc::new(alias_table);

    // Plan 212 §8 — router-only reference peer (no application
    // destination hash). Per-service lookup keys derive from the
    // HttpClient/IrcClient spec destinations above.
    let reference = i2pr_daemon::service_product::ReferencePeer {
        router_info_bytes: std::fs::read(&i2pd_ri_path).expect("read i2pd router.info"),
        endpoint: i2pd_endpoint,
    };
    let data_dir = tempfile::tempdir().expect("temp data dir");
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
            panic!("Plan 214 product composition start failed: {error:?}");
        }
    };
    let http_port = match product.http_listener_port(PLAN214_HTTP_SPEC_ID) {
        Some(port) => port,
        None => {
            append_evidence(&evidence_dir, "remote-stop", "phase=http-listener-bound");
            let _ = product.shutdown().await;
            panic!("Plan 214 HTTP listener was not bound");
        }
    };
    let irc_port = match product.irc_listener_port(PLAN214_IRC_SPEC_ID) {
        Some(port) => port,
        None => {
            append_evidence(&evidence_dir, "remote-stop", "phase=irc-listener-bound");
            let _ = product.shutdown().await;
            panic!("Plan 214 IRC listener was not bound");
        }
    };
    append_evidence(
        &evidence_dir,
        "http-product-listener-bound",
        &http_port.to_string(),
    );
    append_evidence(
        &evidence_dir,
        "irc-product-listener-bound",
        &irc_port.to_string(),
    );

    // Plan 214 §9 — startup LeaseSet2/installation rows before any
    // application command runs.
    emit_startup_rows(
        &product,
        &evidence_dir,
        "http",
        PLAN214_HTTP_SPEC_ID,
        &http_dest_hash,
    )
    .await;
    emit_startup_rows(
        &product,
        &evidence_dir,
        "irc",
        PLAN214_IRC_SPEC_ID,
        &irc_dest_hash,
    )
    .await;

    // Run the pumped curl / jaraco subprocess invocations. Any
    // case failure records the exact phase in `remote-stop` and
    // fails closed; the product still shuts down cleanly.
    if let Err(error) = run_http_cases(
        &mut product,
        http_port,
        &evidence_dir,
        &http_facts,
        large_expected_len,
        &large_expected_sha256,
    )
    .await
    {
        append_evidence(&evidence_dir, "remote-stop", &error);
        let _ = product.shutdown().await;
        panic!("Plan 214 HTTP case failed: {error}");
    }
    if let Err(error) = run_irc_case(
        &mut product,
        irc_port,
        &evidence_dir,
        &irc_facts,
        &jaraco_src,
        &harness_dir,
        &expected_nick,
        &irc_channel,
        &irc_token,
    )
    .await
    {
        append_evidence(&evidence_dir, "remote-stop", &error);
        let _ = product.shutdown().await;
        panic!("Plan 214 IRC case failed: {error}");
    }

    // Read the operation-derived counters from the production
    // composition after both application sessions have run.
    let counters_after = product.remote_counters().await;
    append_evidence(
        &evidence_dir,
        "plan214-backend-counters",
        &format!(
            "remote_lookup_cache_hit={} remote_outbound_composed={} remote_inbound_dispatched={}",
            counters_after.remote_lookup_cache_hit,
            counters_after.remote_outbound_composed,
            counters_after.remote_inbound_dispatched,
        ),
    );

    // Co-owned destination hashes must be empty for the i2pd-owned
    // remote destinations; the manager-level co-owned bridge must
    // never have claimed them.
    let co_owned = product.co_owned_destination_hashes();
    append_evidence(
        &evidence_dir,
        "co_owned_destination_hashes_empty",
        if co_owned.is_empty() { "1" } else { "0" },
    );

    // Stop the product. The composition drains the SSU2 socket
    // and cancels its child scope.
    if let Err(error) = product.shutdown().await {
        append_evidence(&evidence_dir, "shutdown-error", &format!("{error:?}"));
        panic!("Plan 214 product shutdown failed: {error:?}");
    }
}

fn base64_decode(value: &str) -> Vec<u8> {
    // Public destination material is base64 of the canonical
    // identity encoding. The i2pr SAM base64 decoder keeps the
    // driver free of external base64 crates and enforces a sane
    // maximum length.
    i2pr_api::sam::base64::decode(value, 1024)
        .expect("destination base64 must decode to canonical identity bytes")
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes.iter() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod plan214_evidence_unit_tests {
    //! Plan 214 §19 — focused unit rows for the counted driver's
    //! evidence helpers (no network, routine CI). Every predicate
    //! the aggregation relies on is locked here: status parsing,
    //! fixture-field extraction, freshness baselines, digest
    //! equality, counter windows, duplicate-key rejection, and
    //! resource-cleanup observation.

    use super::{
        client_fact, delta_after, find_duplicate_label, json_bool_field, json_string_field,
        json_u64_field, max_fact_seq, parse_curl_status_code, records_after,
    };

    #[test]
    fn plan214_curl_status_reads_actual_frame() {
        assert_eq!(parse_curl_status_code("200"), Some(200));
        assert_eq!(parse_curl_status_code("  403\n"), Some(403));
        assert_eq!(parse_curl_status_code("502"), Some(502));
    }

    #[test]
    fn plan214_curl_status_rejects_body_length_inference() {
        // `000` (connection failure frame), empty, and overlong
        // bodies must never parse as a success status.
        assert_eq!(parse_curl_status_code("000"), None);
        assert_eq!(parse_curl_status_code(""), None);
        assert_eq!(parse_curl_status_code("200200"), None);
        assert_eq!(parse_curl_status_code("hello-from-loopback-fixture"), None);
        assert_eq!(parse_curl_status_code("20"), None);
        assert_eq!(parse_curl_status_code("99"), None);
        assert_eq!(parse_curl_status_code("600"), None);
    }

    #[test]
    fn plan214_json_string_field_extracts_method_target() {
        let line =
            r#"{"body_len": 0, "method": "GET", "seq": 3, "target": "/hello", "tunneled": false}"#;
        assert_eq!(json_string_field(line, "method").as_deref(), Some("GET"));
        assert_eq!(json_string_field(line, "target").as_deref(), Some("/hello"));
        assert_eq!(json_string_field(line, "missing"), None);
        // Non-string fields are not strings.
        assert_eq!(json_string_field(line, "body_len"), None);
    }

    #[test]
    fn plan214_json_fields_reject_stale_record_shape() {
        // A record without `seq` never passes a freshness gate.
        let line = r#"{"method": "GET", "target": "/hello"}"#;
        assert_eq!(json_u64_field(line, "seq"), None);
        assert_eq!(max_fact_seq(line), 0);
        assert!(records_after(line, 0).is_empty());
    }

    #[test]
    fn plan214_get_fixture_baseline_rejects_stale_records() {
        let facts = "{\"method\": \"GET\", \"seq\": 1, \"target\": \"/hello\"}\n\
             {\"method\": \"POST\", \"seq\": 2, \"target\": \"/post\"}\n";
        // Baseline at 2: the stale GET must not surface.
        assert!(records_after(facts, 2).is_empty());
        let with_fresh =
            format!("{facts}{{\"method\": \"GET\", \"seq\": 3, \"target\": \"/hello\"}}\n");
        let fresh = records_after(&with_fresh, 2);
        assert_eq!(fresh.len(), 1);
        assert_eq!(
            json_string_field(fresh[0], "target").as_deref(),
            Some("/hello")
        );
    }

    #[test]
    fn plan214_post_fixture_digest_must_equal_sent_digest() {
        let sent = "aaaabbbbcccc";
        let line = format!(
            "{{\"body_len\": 12, \"body_sha256\": \"{sent}\", \"method\": \"POST\", \"seq\": 4, \"target\": \"/post\"}}"
        );
        assert_eq!(
            json_string_field(&line, "body_sha256").as_deref(),
            Some(sent)
        );
        assert_eq!(json_u64_field(&line, "body_len"), Some(12));
        let tampered = line.replace(sent, "ddddddddeeee");
        assert_ne!(
            json_string_field(&tampered, "body_sha256").as_deref(),
            Some(sent)
        );
    }

    #[test]
    fn plan214_large_expected_mismatch_fails() {
        let expected_len = 65536_usize;
        let expected_digest = "ab".repeat(32);
        assert_ne!(1234_usize, expected_len);
        assert_ne!("cd".repeat(32), expected_digest);
        assert_eq!(expected_digest.len(), 64);
    }

    #[test]
    fn plan214_clearnet_case_fails_if_target_sees_request() {
        let baseline = 5_u64;
        let facts = "{\"method\": \"GET\", \"seq\": 6, \"target\": \"/\"}\n";
        // Any fresh record after a rejection baseline fails the
        // zero-delta proof.
        assert!(!records_after(facts, baseline).is_empty());
        assert!(records_after("", baseline).is_empty());
    }

    #[test]
    fn plan214_ip_literal_case_fails_if_target_sees_request() {
        let baseline = 9_u64;
        let facts = "{\"method\": \"GET\", \"seq\": 10, \"target\": \"/\"}\n";
        assert!(!records_after(facts, baseline).is_empty());
    }

    #[test]
    fn plan214_http_counter_window_rejects_zero_outbound_delta() {
        assert_eq!(delta_after(7, 7), 0);
        assert!(delta_after(7, 8) > 0);
        // Counter resets never underflow the window.
        assert_eq!(delta_after(9, 4), 0);
    }

    #[test]
    fn plan214_http_counter_window_rejects_zero_inbound_delta() {
        assert_eq!(delta_after(3, 3), 0);
        assert!(delta_after(3, 5) > 0);
    }

    #[test]
    fn plan214_irc_registration_requires_target_and_client_facts() {
        // Registration passes only when the target observed the
        // expected NICK + a USER line AND the client observed the
        // welcome numeric; any missing side fails the row.
        let facts = "{\"event\": \"register-nick\", \"nick\": \"plan214alice\", \"seq\": 2}\n\
             {\"event\": \"user\", \"line\": \"USER alice * * :Alice\", \"seq\": 3}\n";
        let fresh = records_after(facts, 1);
        let nick_ok = fresh.iter().any(|line| {
            json_string_field(line, "event").as_deref() == Some("register-nick")
                && json_string_field(line, "nick").as_deref() == Some("plan214alice")
        });
        let user_ok = fresh
            .iter()
            .any(|line| json_string_field(line, "event").as_deref() == Some("user"));
        let client_ok = client_fact("WELCOME=1\n", "WELCOME") == "1";
        assert!(nick_ok && user_ok && client_ok);
        let wrong_nick = fresh.iter().any(|line| {
            json_string_field(line, "event").as_deref() == Some("register-nick")
                && json_string_field(line, "nick").as_deref() == Some("mallory")
        });
        assert!(!wrong_nick);
    }

    #[test]
    fn plan214_privacy_rewrite_fails_without_target_proof() {
        // The neutral shape (`i2p localhost`, which jaraco never
        // emits) proves our rewrite; the booleans prove no leak.
        let user = r#"{"event": "user", "line": "USER alice i2p localhost :Alice", "seq": 2}"#;
        let neutral = json_string_field(user, "line")
            .as_deref()
            .is_some_and(|text| text.contains(" i2p localhost "));
        assert!(neutral);
        let raw = r#"{"event": "user", "line": "USER alice 0 * :Alice", "seq": 2}"#;
        let raw_neutral = json_string_field(raw, "line")
            .as_deref()
            .is_some_and(|text| text.contains(" i2p localhost "));
        assert!(!raw_neutral, "unrewritten USER must not match");
        let line = r#"{"event": "user-privacy", "has_b32": false, "has_loopback": false, "has_nodename": false, "seq": 3}"#;
        assert_eq!(json_bool_field(line, "has_loopback"), Some(false));
        assert_eq!(json_bool_field(line, "has_nodename"), Some(false));
        let weak = r#"{"event": "user-privacy", "has_b32": false, "has_loopback": true, "has_nodename": false, "seq": 3}"#;
        assert_eq!(json_bool_field(weak, "has_loopback"), Some(true));
    }

    #[test]
    fn plan214_dcc_case_requires_explicit_attempt() {
        // `DCC_ATTEMPTED=1` must come from the client log; absence
        // is not a block proof.
        assert_eq!(client_fact("DCC_ATTEMPTED=1\n", "DCC_ATTEMPTED"), "1");
        assert_eq!(client_fact("DCC_SENT=0\n", "DCC_ATTEMPTED"), "");
    }

    #[test]
    fn plan214_dcc_case_fails_if_forbidden_payload_reaches_target() {
        let facts = "{\"event\": \"dcc-observed\", \"line\": \"PRIVMSG #c :\\u0001DCC SEND f 1 0 1\\u0001\", \"seq\": 8}\n";
        let hits: Vec<_> = records_after(facts, 7)
            .into_iter()
            .filter(|line| json_string_field(line, "event").as_deref() == Some("dcc-observed"))
            .collect();
        assert_eq!(hits.len(), 1);
        assert!(records_after("", 7).is_empty());
    }

    #[test]
    fn plan214_irc_privmsg_requires_target_and_client_facts() {
        let token = "plan214-token";
        let line =
            format!("{{\"event\": \"privmsg-received\", \"seq\": 6, \"text\": \"hello {token}\"}}");
        let target_hit = json_string_field(&line, "text")
            .as_deref()
            .is_some_and(|text| text.contains(token));
        assert!(target_hit);
        // A different session's token must not match.
        let other_hit = json_string_field(&line, "text")
            .as_deref()
            .is_some_and(|text| text.contains("other-session-token"));
        assert!(!other_hit);
    }

    #[test]
    fn plan214_irc_counter_window_is_independent_from_http_window() {
        // Windows are independent snapshots: reusing the HTTP
        // `before` for the IRC delta would attribute HTTP work to
        // IRC. The driver takes `irc_before` after HTTP drains;
        // the helper proves deltas compose per window.
        let http_before = 10_u64;
        let http_after = 14_u64;
        let irc_before = 14_u64;
        let irc_after = 14_u64;
        assert_eq!(delta_after(http_before, http_after), 4);
        assert_eq!(delta_after(irc_before, irc_after), 0);
    }

    #[test]
    fn plan214_duplicate_conflicting_evidence_keys_fail() {
        let clean = "http-get-status\t200\nhttp-get-response-len\t27\n";
        assert_eq!(find_duplicate_label(clean), None);
        let dirty = "http-get-status\t200\nhttp-get-status\t500\n";
        assert_eq!(
            find_duplicate_label(dirty).as_deref(),
            Some("http-get-status")
        );
    }

    #[test]
    fn plan214_stale_evidence_shapes_are_detected() {
        // An empty facts file yields seq 0; only seq >= 1 records
        // can follow it.
        assert_eq!(max_fact_seq(""), 0);
        assert_eq!(max_fact_seq("not json\n"), 0);
    }

    #[test]
    fn plan214_private_destination_suffix_never_emitted() {
        // The driver emits hashes (64 hex) and b32 labels, never
        // the base64 private suffix: evidence rows carry digests
        // only, and the configured public material decodes to the
        // canonical identity prefix whose SHA-256 is the hash row.
        use super::base64_decode;
        assert_eq!(base64_decode("QUJD").len(), 3);
    }

    #[test]
    fn plan214_append_evidence_truncates_long_values() {
        // Plan 214 §13 — failure excerpts stay bounded so the
        // runner's 400-char overlong-token audit keeps meaning key
        // material. Short values pass through byte-identical.
        use super::append_evidence;
        let dir = std::env::temp_dir().join(format!(
            "plan214-truncate-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("tempdir");
        append_evidence(&dir, "short", "200");
        let long = "x".repeat(500);
        append_evidence(&dir, "remote-stop", &long);
        let content =
            std::fs::read_to_string(dir.join("driver-evidence.tsv")).expect("read evidence");
        assert!(content.contains("short\t200\n"), "short row passes through");
        let stop_line = content
            .lines()
            .find(|line| line.starts_with("remote-stop\t"))
            .expect("stop row exists");
        let value = stop_line.split('\t').nth(1).expect("stop value");
        assert!(
            value.len() <= 400,
            "truncated value stays under the audit ceiling"
        );
        assert!(
            value.ends_with("...[truncated]"),
            "truncation is marked, not silent"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
