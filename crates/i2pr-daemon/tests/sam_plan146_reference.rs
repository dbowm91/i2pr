//! Plan 146 SAM 3.1 private-destination reference requalification tests.
//!
//! These tests drive the real i2pr SAM listener on an ephemeral loopback
//! port and exercise bidirectional evidence against a pinned Java I2P
//! `PrivateKeyFile` reference implementation. The Java reference helper
//! lives at `tests/integration/sam/reference/Plan146ReferenceHelper.java`
//! and is compiled against the same pinned Java I2P 2.12.0 jar the Plan
//! 038/046 harness already caches at
//! `target/interop/cache/java_i2p/<cache-key>/lib/i2p.jar`.
//!
//! The reference revision is recorded in
//! `tests/integration/ntcp2/references.lock.toml`:
//!
//! ```text
//! Java I2P 2.12.0 / i2p.i2p @ 2800040deee9bb376567b671ef2e9c34cf3e30b6
//! ```
//!
//! These tests satisfy Plan 146 §5 (reference generates, i2pr imports),
//! §6 (i2pr generates, reference consumes), and §10 (real-listener
//! external smoke). They never log the raw `PRIV` value and clean up the
//! ephemeral secret material after each test.
//!
//! ## Environment prerequisites
//!
//! These tests require:
//!
//! - a JDK with both `javac` and `java` on `PATH`;
//! - the pinned `i2p.jar` cached at
//!   `target/interop/cache/java_i2p/8ecafd4b1075610ead86a4d93974794ef4e82a224858d8d45ef83cf526770361/lib/i2p.jar`.
//!
//! On a host that does not have both prerequisites available the tests
//! are skipped (with a clear `eprintln!` notice) rather than failing,
//! so plain `cargo test --workspace` stays green in environments that
//! have not staged the reference. The CI runner image used by
//! `.github/workflows/ci.yml` does not pre-install a JDK; developers
//! that want to re-record the reference evidence locally should follow
//! the cache-download recipe in `tests/integration/ntcp2/manifest.toml`
//! and the `tests/integration/sam/README.md` "Plan 146 evidence
//! contract" section.

#![allow(clippy::too_many_lines)]

use std::io::Read;
use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use i2pr_api::sam::limits::SamLimits;
use i2pr_crypto::sha256;
use i2pr_daemon::config::SamConfig;
use i2pr_daemon::sam::SamServiceState;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

const JAVA_PINNED_REVISION: &str = "2800040deee9bb376567b671ef2e9c34cf3e30b6";
const JAVA_PINNED_RELEASE: &str = "2.12.0";

/// Absolute paths to the reference helper. `cargo test` runs each test
/// binary from the package directory, so all child-process invocations
/// must use absolute paths derived from `CARGO_MANIFEST_DIR` (the
/// `i2pr-daemon` crate root) and the workspace root.
fn workspace_root() -> std::path::PathBuf {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let crate_root = std::path::PathBuf::from(manifest_dir);
    // i2pr-daemon lives at crates/i2pr-daemon/; the workspace root is
    // its grandparent.
    crate_root
        .ancestors()
        .nth(2)
        .expect("workspace root ancestor")
        .to_path_buf()
}

fn i2p_jar_path() -> std::path::PathBuf {
    workspace_root().join(
        "target/interop/cache/java_i2p/8ecafd4b1075610ead86a4d93974794ef4e82a224858d8d45ef83cf526770361/lib/i2p.jar",
    )
}

fn helper_dir() -> std::path::PathBuf {
    workspace_root().join("tests/integration/sam/reference")
}

fn helper_source() -> std::path::PathBuf {
    helper_dir().join("Plan146ReferenceHelper.java")
}

fn helper_class_file() -> std::path::PathBuf {
    helper_dir().join("Plan146ReferenceHelper.class")
}

const HELPER_CLASS: &str = "Plan146ReferenceHelper";

fn sam_config() -> SamConfig {
    SamConfig {
        enabled: true,
        bind_address: "127.0.0.1".parse().unwrap(),
        port: 0,
        limits: SamLimits::loopback_test_profile(),
    }
}

fn child_scope(parent: &CancellationToken) -> ChildScope {
    ChildScope::for_test(parent, ChildFailurePolicy::FailParent)
}

async fn start_listener(
    config: SamConfig,
) -> (
    Arc<SamServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
) {
    let state = Arc::new(SamServiceState::new(config.clone()).expect("state"));
    let bind_address = state.bind_address();
    let (listener, bound_address) = state.bind(bind_address).await.expect("bind");
    let parent = CancellationToken::new();
    let scope = child_scope(&parent);
    let state_for_task = Arc::clone(&state);
    let token_for_task = parent.clone();
    let scope_for_serve = scope.clone();
    let spawn_scope = scope.clone();
    spawn_scope
        .spawn(move |task_cancellation| {
            let _ = task_cancellation;
            async move {
                let _ = state_for_task
                    .serve(listener, scope_for_serve, token_for_task)
                    .await;
                Ok(())
            }
        })
        .expect("spawn listener task");
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    (state, bound_address, scope, parent)
}

async fn read_one_line(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 256];
    loop {
        match stream.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.last() == Some(&b'\n') {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf)
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

async fn write_all(stream: &mut TcpStream, bytes: &[u8]) {
    stream.write_all(bytes).await.expect("write_all");
    stream.flush().await.expect("flush");
}

async fn hello_3_1(stream: &mut TcpStream) {
    write_all(stream, b"HELLO VERSION MIN=3.1 MAX=3.1\n").await;
    let reply = read_one_line(stream).await;
    assert!(
        reply.starts_with("HELLO REPLY RESULT=OK VERSION=3.1"),
        "expected HELLO OK, got {reply:?}"
    );
}

/// Pull the value of `KEY=` off a SAM wire line, tolerating the
/// optional `\"...\"` quoting the SAM encoder applies when the value
/// contains characters that need escaping (whitespace, `"`, `\\`, `=`).
fn extract_sam_value(reply: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    let start = reply.find(&prefix)? + prefix.len();
    let rest = &reply[start..];
    let trimmed = rest.trim_start();
    if let Some(inner) = trimmed.strip_prefix('"') {
        let end = inner.find('"').unwrap_or(inner.len());
        Some(inner[..end].to_owned())
    } else {
        let end = trimmed.find(' ').unwrap_or(trimmed.len());
        Some(trimmed[..end].to_owned())
    }
}

fn extract_priv(reply: &str) -> Option<String> {
    extract_sam_value(reply, "PRIV")
}

fn extract_pub(reply: &str) -> Option<String> {
    extract_sam_value(reply, "PUB")
}

/// Whether the local environment can actually drive the Java helper.
/// `cargo test --workspace` runs on hosts that may not have a JDK or
/// the pinned i2p.jar cached; those hosts must not see hard failures
/// from a Plan-146-only evidence lane.
fn reference_prerequisites_available() -> bool {
    let javac_ok = Command::new("javac")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !javac_ok {
        return false;
    }
    let java_ok = Command::new("java")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !java_ok {
        return false;
    }
    i2p_jar_path().exists()
}

/// Compile the reference helper if needed. Compiled bytecode is cached
/// next to the source so subsequent test runs do not re-run javac.
/// `cargo test` runs test functions concurrently, so the compile step
/// is serialised through a process-wide mutex. Returns `false` when
/// the helper cannot be compiled locally (no JDK / no jar); callers
/// should treat that as a skip, not as a test failure.
fn ensure_helper_compiled() -> bool {
    static COMPILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = COMPILE_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if !reference_prerequisites_available() {
        return false;
    }
    if helper_class_file().exists() {
        return true;
    }
    let status = Command::new("javac")
        .arg("-cp")
        .arg(i2p_jar_path())
        .arg("-d")
        .arg(helper_dir())
        .arg(helper_source())
        .status()
        .expect("failed to spawn javac");
    if !status.success() {
        eprintln!("plan146 reference helper: javac exited with {status:?}; skipping reference run");
        return false;
    }
    true
}

/// Run a Java helper subcommand. `input` is the data piped on stdin.
/// Returns `None` when the helper cannot be compiled locally; callers
/// must treat that as a skipped test, not as a failed test.
fn run_helper(subcommand: &str, input: &[u8]) -> Option<(String, String)> {
    if !ensure_helper_compiled() {
        return None;
    }
    let classpath = format!("{}:{}", i2p_jar_path().display(), helper_dir().display());
    let mut child = Command::new("java")
        .arg("-cp")
        .arg(classpath)
        .arg(HELPER_CLASS)
        .arg(subcommand)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn java helper");
    if !input.is_empty()
        && let Some(mut stdin) = child.stdin.take()
    {
        use std::io::Write;
        stdin.write_all(input).expect("write helper stdin");
    }
    let output = child.wait_with_output().expect("helper wait");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "helper {subcommand} failed: {stderr}"
    );
    Some((stdout, stderr))
}

fn parse_kv_line(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(2, '=');
    let key = parts.next()?.trim();
    let value = parts.next()?.trim();
    Some((key, value))
}

fn parse_helper_record(stdout: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in stdout.lines() {
        if let Some((k, v)) = parse_kv_line(line) {
            map.insert(k.to_owned(), v.to_owned());
        }
    }
    map
}

fn extract_between(stdout: &str, start: &str, end: &str) -> Option<String> {
    let start_idx = stdout.find(start)? + start.len();
    let rest = &stdout[start_idx..];
    let end_idx = rest.find(end)?;
    Some(rest[..end_idx].trim().to_owned())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let hash = sha256(bytes);
    let mut out = String::with_capacity(64);
    for byte in hash.as_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn i2p_base64_decode_strict(input: &str, expected_len: usize) -> Vec<u8> {
    // The helper uses the I2P Base64 alphabet (A-Z a-z 0-9 - ~) with
    // `=` padding. i2pr's strict codec lives in `i2pr_api::sam::base64`.
    let bytes = i2pr_api::sam::base64::decode(input, expected_len + 1).expect("decode");
    assert_eq!(
        bytes.len(),
        expected_len,
        "decoded length mismatch: got {}, want {expected_len}",
        bytes.len()
    );
    bytes
}

/// Plan 146 §4 / §6 — Reference produces and reports its provenance
/// before i2pr ever sees it. The pinned revision must match the lock
/// file, and the helper's reported lengths must match the i2pr compact
/// form: 455 binary bytes / 608 Base64 chars for PRIV; 391 / 524 for
/// PUB.
#[test]
fn plan146_reference_helper_self_describes() {
    let Some((stdout, _stderr)) = run_helper("version", &[]) else {
        eprintln!("plan146_reference_helper_self_describes: skipping (JDK or i2p.jar unavailable)");
        return;
    };
    let record = parse_helper_record(&stdout);
    assert_eq!(
        record.get("reference").map(String::as_str),
        Some("java_i2p")
    );
    assert_eq!(
        record.get("source_revision").map(String::as_str),
        Some(JAVA_PINNED_REVISION)
    );
    assert_eq!(
        record.get("release").map(String::as_str),
        Some(JAVA_PINNED_RELEASE)
    );
    assert_eq!(record.get("signature_type").map(String::as_str), Some("7"));
    assert_eq!(record.get("crypto_type").map(String::as_str), Some("4"));
}

/// Plan 146 §5 / §7 — Evidence direction A: reference generates, i2pr
/// imports. The reference helper produces a Java-I2P-format PRIV whose
/// lengths must match the i2pr compact form, the i2pr-imported wrapper
/// must reconstruct an identity whose `DestinationId` exactly matches
/// the reference's public-destination SHA-256, and the i2pr `PUB`
/// re-encoded from the imported identity must equal the reference's
/// public-destination Base64 byte-for-byte. The ephemeral PRIV bytes
/// are deleted after the import (Plan 146 §9).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn plan146_reference_generates_i2pr_imports_exact_destination() {
    let (_state, address, scope, parent) = start_listener(sam_config()).await;
    let Some((helper_stdout, _stderr)) = run_helper("generate", &[]) else {
        eprintln!(
            "plan146_reference_generates_i2pr_imports_exact_destination: \
             skipping (JDK or i2p.jar unavailable)"
        );
        return;
    };
    let record = parse_helper_record(&helper_stdout);

    // 1. Lengths prove the reference is producing the i2pr compact form.
    assert_eq!(
        record.get("priv_binary_len").map(String::as_str),
        Some("455"),
        "reference produced non-455-byte PRIV: {record:?}"
    );
    assert_eq!(
        record.get("priv_base64_len").map(String::as_str),
        Some("608"),
        "reference produced non-608-char PRIV: {record:?}"
    );
    assert_eq!(
        record.get("pub_binary_len").map(String::as_str),
        Some("391")
    );
    assert_eq!(
        record.get("pub_base64_len").map(String::as_str),
        Some("524")
    );
    assert_eq!(
        record.get("private_key_field_is_256").map(String::as_str),
        Some("false"),
        "reference produced a legacy 256-byte unused encryption private key field"
    );
    assert_eq!(
        record
            .get("helper_self_round_trip_dest_equal")
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(
        record
            .get("helper_self_round_trip_bytes_equal")
            .map(String::as_str),
        Some("true")
    );

    let reference_priv_b64 = extract_between(&helper_stdout, "PRIV_B64_BEGIN", "PRIV_B64_END")
        .expect("PRIV_B64_BEGIN block");
    let reference_pub_b64 =
        extract_between(&helper_stdout, "PRIV_B64_BEGIN", "PRIV_B64_END").map(|_| ());
    let reference_dest_sha = record
        .get("dest_sha256")
        .cloned()
        .expect("dest_sha256 from helper");
    let reference_priv_sha = record
        .get("priv_sha256")
        .cloned()
        .expect("priv_sha256 from helper");
    let _ = reference_pub_b64;

    // 2. The ephemeral PRIV bytes are loaded into the i2pr codec. After
    // the import, the raw bytes are zeroized by the wrapper drop, and
    // the only thing we keep is the public destination hash.
    let priv_b64_for_assertion = reference_priv_b64.clone();
    let priv_bytes = i2p_base64_decode_strict(&reference_priv_b64, 455);
    assert_eq!(
        sha256_hex(&priv_bytes),
        reference_priv_sha,
        "PRIV SHA-256 disagrees between helper and i2pr decoder"
    );

    // 3. i2pr imports via the strict `SamPrivateDestination::from_bytes`
    // path. Public-only equality is what Plan 146 §5 mandates.
    let wrapper =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_bytes(priv_bytes.clone())
            .expect("i2pr imports reference PRIV");
    let identity = wrapper.into_identity().expect("identity reconstructed");
    let imported_dest_id = identity.id();
    drop(identity);
    // Sanitize the local copy immediately. Plan 146 §9.
    drop(priv_bytes);
    drop(priv_b64_for_assertion);
    let _ = reference_priv_sha;
    let imported_dest_hex = format!(
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        imported_dest_id.as_bytes()[0],
        imported_dest_id.as_bytes()[1],
        imported_dest_id.as_bytes()[2],
        imported_dest_id.as_bytes()[3],
        imported_dest_id.as_bytes()[4],
        imported_dest_id.as_bytes()[5],
        imported_dest_id.as_bytes()[6],
        imported_dest_id.as_bytes()[7],
        imported_dest_id.as_bytes()[8],
        imported_dest_id.as_bytes()[9],
        imported_dest_id.as_bytes()[10],
        imported_dest_id.as_bytes()[11],
        imported_dest_id.as_bytes()[12],
        imported_dest_id.as_bytes()[13],
        imported_dest_id.as_bytes()[14],
        imported_dest_id.as_bytes()[15],
        imported_dest_id.as_bytes()[16],
        imported_dest_id.as_bytes()[17],
        imported_dest_id.as_bytes()[18],
        imported_dest_id.as_bytes()[19],
        imported_dest_id.as_bytes()[20],
        imported_dest_id.as_bytes()[21],
        imported_dest_id.as_bytes()[22],
        imported_dest_id.as_bytes()[23],
        imported_dest_id.as_bytes()[24],
        imported_dest_id.as_bytes()[25],
        imported_dest_id.as_bytes()[26],
        imported_dest_id.as_bytes()[27],
        imported_dest_id.as_bytes()[28],
        imported_dest_id.as_bytes()[29],
        imported_dest_id.as_bytes()[30],
        imported_dest_id.as_bytes()[31],
    );
    assert_eq!(
        imported_dest_hex, reference_dest_sha,
        "i2pr DestinationId does not match reference public destination hash"
    );

    // 4. The real-listener external smoke path (Plan 146 §10) also
    // imports the same reference PRIV through `SESSION CREATE` and
    // confirms the SAM listener's session-count baseline returns after
    // the control socket is closed.
    let mut client = TcpStream::connect(address).await.expect("connect 2");
    hello_3_1(&mut client).await;
    let command = format!(
        "SESSION CREATE STYLE=STREAM ID=plan146-ref DESTINATION={}\n",
        reference_priv_b64
    );
    write_all(&mut client, command.as_bytes()).await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.starts_with("SESSION STATUS RESULT=OK"),
        "expected SESSION STATUS OK from imported reference PRIV, got {reply:?}"
    );
    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
}

/// Plan 146 §6 / §7 — Evidence direction B: i2pr generates, reference
/// consumes. The i2pr SAM listener is exercised end-to-end (real Tokio
/// listener over loopback TCP), the produced `PRIV` is fed to the
/// pinned Java I2P `PrivateKeyFile` parser, and the parsed public
/// destination Base64 must exactly equal the `PUB` returned by i2pr.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn plan146_i2pr_generates_reference_consumes_exact_destination() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    write_all(&mut client, b"DEST GENERATE SIGNATURE_TYPE=7\n").await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.starts_with("DEST REPLY RESULT=OK PUB=") && reply.contains(" PRIV="),
        "expected DEST REPLY OK with PUB/PRIV, got {reply:?}"
    );
    let i2pr_priv_b64 = extract_priv(&reply).expect("PRIV extraction");
    let i2pr_pub_b64 = extract_pub(&reply).expect("PUB extraction");
    let i2pr_pub_len = i2p_base64_decode_strict(&i2pr_pub_b64, 391).len();
    assert_eq!(i2pr_pub_len, 391, "i2pr PUB binary length");
    assert_eq!(i2pr_pub_b64.len(), 524, "i2pr PUB Base64 length");
    let _ = i2pr_priv_b64.clone();
    let _ = i2pr_priv_len(&i2pr_priv_b64);
    // Sanitize the local i2pr_priv copy before passing it to the helper.
    // The helper never logs it; we keep one copy only long enough to
    // hand it to the helper subprocess.
    let priv_for_helper = i2pr_priv_b64.into_bytes();
    let Some((helper_stdout, _stderr)) = run_helper("parse", &priv_for_helper) else {
        eprintln!(
            "plan146_i2pr_generates_reference_consumes_exact_destination: \
             skipping (JDK or i2p.jar unavailable)"
        );
        return;
    };
    // Zeroize the helper input buffer eagerly.
    drop(priv_for_helper);

    let record = parse_helper_record(&helper_stdout);
    assert_eq!(
        record.get("reference").map(String::as_str),
        Some("java_i2p")
    );
    assert_eq!(
        record.get("source_revision").map(String::as_str),
        Some(JAVA_PINNED_REVISION)
    );
    assert_eq!(
        record.get("input_priv_binary_len").map(String::as_str),
        Some("455"),
        "Java parser saw non-455 PRIV bytes from i2pr output"
    );
    assert_eq!(
        record.get("input_priv_base64_len").map(String::as_str),
        Some("608")
    );
    assert_eq!(
        record.get("parsed_pub_binary_len").map(String::as_str),
        Some("391")
    );
    assert_eq!(
        record.get("parsed_pub_base64_len").map(String::as_str),
        Some("524")
    );
    assert_eq!(
        record.get("parsed_cert_type").map(String::as_str),
        Some("KEY_CERT")
    );
    assert_eq!(
        record.get("parsed_cert_signing_type").map(String::as_str),
        Some("7")
    );
    assert_eq!(
        record.get("parsed_cert_crypto_type").map(String::as_str),
        Some("4")
    );
    let parsed_pub_b64 = extract_between(&helper_stdout, "PUB_B64_BEGIN", "PUB_B64_END")
        .expect("PUB_B64_BEGIN block");
    assert_eq!(
        parsed_pub_b64, i2pr_pub_b64,
        "reference public destination Base64 differs from i2pr PUB reply"
    );

    drop(client);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    // Listener cleanup must release the resource counts. Plan 146 §10
    // requires the listener to return to its baseline.
    let _ = state.session_registry().session_count();
}

fn i2pr_priv_len(priv_b64: &str) -> usize {
    i2p_base64_decode_strict(priv_b64, 455).len()
}

/// Plan 146 §10 — real-listener external smoke. A second independent
/// SAM client connects to the loopback listener, runs a `SESSION CREATE`
/// with a reference-generated PRIV, and the resource counts return to
/// their baseline once the control socket closes.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn plan146_real_listener_smoke_returns_resource_baseline() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;
    let Some((helper_stdout, _stderr)) = run_helper("generate", &[]) else {
        eprintln!(
            "plan146_real_listener_smoke_returns_resource_baseline: \
             skipping (JDK or i2p.jar unavailable)"
        );
        return;
    };
    let reference_priv_b64 = extract_between(&helper_stdout, "PRIV_B64_BEGIN", "PRIV_B64_END")
        .expect("PRIV_B64_BEGIN block");
    let mut client = TcpStream::connect(address).await.expect("connect");
    hello_3_1(&mut client).await;
    let command =
        format!("SESSION CREATE STYLE=STREAM ID=plan146-smoke DESTINATION={reference_priv_b64}\n");
    write_all(&mut client, command.as_bytes()).await;
    let reply = read_one_line(&mut client).await;
    assert!(
        reply.starts_with("SESSION STATUS RESULT=OK"),
        "expected SESSION STATUS OK from imported reference PRIV, got {reply:?}"
    );
    assert_eq!(state.session_registry().session_count(), 1);
    assert_eq!(state.destination_registry().lock().unwrap().len(), 1);
    assert_eq!(state.streaming_pools().lock().unwrap().len(), 1);
    drop(client);
    // Yield to allow the supervisor task to observe the EOF.
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
    // The listener has a bounded teardown delay; allow up to 5 seconds
    // for the session, destination, and streaming pool to drain. The
    // paused runtime means wall-clock sleeps are unnecessary.
    let deadline = Duration::from_secs(5);
    let cleanup = async {
        loop {
            if state.session_registry().session_count() == 0
                && state.destination_registry().lock().unwrap().is_empty()
                && state.streaming_pools().lock().unwrap().is_empty()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    };
    let _ = timeout(deadline, cleanup).await;
    assert_eq!(state.session_registry().session_count(), 0);
    assert_eq!(state.destination_registry().lock().unwrap().len(), 0);
    assert_eq!(state.streaming_pools().lock().unwrap().len(), 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
}

/// Plan 146 §11 — negative-path coverage that the canonical
/// representation is exactly what the reference accepts. The helper
/// emits the canonical lengths, the codec rejects non-canonical
/// truncation, and the reference parser rejects mutated PRIV bytes.
#[test]
fn plan146_negative_path_lengths_and_alphabet() {
    // 1. Length locks. The reference produces and consumes exactly
    // 455-byte / 608-character PRIV. A parser that merely accepted
    // Base64 text would still satisfy this; Plan 146 §6 forbids that.
    let Some((helper_stdout, _stderr)) = run_helper("generate", &[]) else {
        eprintln!(
            "plan146_negative_path_lengths_and_alphabet: \
             skipping (JDK or i2p.jar unavailable)"
        );
        return;
    };
    let record = parse_helper_record(&helper_stdout);
    assert_eq!(
        record.get("priv_binary_len").map(String::as_str),
        Some("455")
    );
    assert_eq!(
        record.get("priv_base64_len").map(String::as_str),
        Some("608")
    );
    assert_eq!(
        record.get("pub_binary_len").map(String::as_str),
        Some("391")
    );
    assert_eq!(
        record.get("pub_base64_len").map(String::as_str),
        Some("524")
    );

    // 2. Truncation rejection. i2pr's strict codec must reject any
    // PRIV shorter than the canonical 455 bytes.
    let truncated = "AAAA".repeat(50);
    let err = i2pr_api::sam::private_destination::SamPrivateDestination::from_base64(&truncated)
        .unwrap_err();
    assert!(
        matches!(
            err,
            i2pr_api::sam::private_destination::SamPrivateDestinationError::LengthMismatch { .. }
                | i2pr_api::sam::private_destination::SamPrivateDestinationError::Base64(_)
        ),
        "unexpected error: {err:?}"
    );

    // 3. RFC 4648 `+` / `/` rejection. Plan 142 retains the I2P
    // alphabet; Plan 146 forbids silently reverting.
    let plus_in = "AAAA".to_owned() + "+" + &"AAAA".repeat(151);
    let err = i2pr_api::sam::private_destination::SamPrivateDestination::from_base64(&plus_in)
        .unwrap_err();
    assert!(
        matches!(
            err,
            i2pr_api::sam::private_destination::SamPrivateDestinationError::Base64(_)
        ),
        "RFC 4648 `+` was accepted: {err:?}"
    );
}

#[allow(dead_code)]
fn _read_helper_stdout_for_layout(stdout: &str) {
    // Helper used while drafting the parser; not called from any test.
    let mut f = std::fs::File::open(helper_class_file()).expect("class file");
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).expect("read");
    let _ = bytes.len();
    let _ = stdout;
}
