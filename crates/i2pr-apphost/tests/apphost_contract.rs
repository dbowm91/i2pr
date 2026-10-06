//! Plan 369 WP3 — executable apphost contract.
//!
//! These tests drive `i2pr_apphost::serve` over an in-process duplex transport
//! and a real generated application, so every assertion is about behaviour a
//! manager would actually observe rather than about internal state.
//!
//! Every wait is bounded by an explicit deadline. A supervisor whose whole
//! purpose is to be bounded must not be able to hang its own test suite.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use i2pr_app_manager_proto::apphost::{
    APPHOST_BOOTSTRAP_BYTES, ApphostBootstrapError, ApphostFailureReason, ApphostHandshake,
    ApphostReply, LaunchRequest, serde_json,
};
use i2pr_apphost::{
    APPHOST_BOOTSTRAP_GRACE, ApphostError, DuplexTransport, LENGTH_PREFIX_BYTES,
    MAX_BOOTSTRAP_PAYLOAD_BYTES, read_launch_request, serve, write_reply,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(20);

/// A scratch directory that cleans itself up, so a failing test cannot leave an
/// executable behind for the next one.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("i2pr-apphost-{label}-{unique}"));
        std::fs::create_dir_all(&path).expect("scratch directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write_executable(&self, name: &str, body: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, body).expect("write application");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("make executable");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn launch_request(
    scratch: &Scratch,
    entrypoint: &str,
    profile: i2pr_app_proto::LaunchProfile,
) -> LaunchRequest {
    LaunchRequest {
        principal: i2pr_app_manager_proto::ManagerPrincipal {
            app_id: i2pr_app_proto::AppId::parse("fixture-app").expect("app id"),
            instance_id: i2pr_app_manager_proto::ManagerInstanceId::new(7),
            publisher_id: None,
        },
        launch_profile: profile,
        root: i2pr_app_manager_proto::apphost::LaunchRoot::new(
            scratch.path().to_str().expect("utf-8"),
        )
        .expect("root"),
        entrypoint: i2pr_app_manager_proto::apphost::Entrypoint::new(entrypoint).expect("entry"),
        argv: Vec::new(),
        environment: i2pr_app_manager_proto::apphost::SanitizedEnvironment::new(BTreeMap::new())
            .expect("environment"),
        resources: i2pr_app_manager_proto::apphost::DescriptiveResourceRequest {
            requested_memory_bytes: 0,
            requested_open_files: 0,
        },
    }
}

/// Serialises a request without the gate, so a test can put bytes on the wire
/// that `LaunchRequest::encode` would never produce.
///
/// This is how the host's own refusal is exercised: the point is that the
/// *host* says no even if a manager-side bug managed to say yes.
fn encode_without_validation(request: &LaunchRequest) -> Vec<u8> {
    serde_json::to_vec(request).expect("serialise")
}

/// Writes the framed bootstrap exactly as `i2pr-appd` will.
async fn write_framed<W>(writer: &mut W, payload: &[u8]) -> Result<(), String>
where
    W: AsyncWrite + Unpin,
{
    assert!(
        payload.len() <= MAX_BOOTSTRAP_PAYLOAD_BYTES,
        "the payload must be inside the envelope the host accepts"
    );
    writer
        .write_all(&ApphostHandshake::encode())
        .await
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await
        .map_err(|error| error.to_string())?;
    writer
        .write_all(payload)
        .await
        .map_err(|error| error.to_string())
}

async fn write_bootstrap<W>(writer: &mut W, request: &LaunchRequest) -> Result<(), String>
where
    W: AsyncWrite + Unpin,
{
    let payload = request.encode().map_err(|error| error.to_string())?;
    write_framed(writer, &payload).await
}

/// Starts `serve` on one end of a duplex and returns the manager end.
fn start_host() -> (
    tokio::task::JoinHandle<Result<(), ApphostError>>,
    tokio::io::DuplexStream,
) {
    let (host_side, manager_side) = tokio::io::duplex(64 * 1024);
    let (read_half, write_half) = tokio::io::split(host_side);
    let host =
        tokio::spawn(async move { serve(DuplexTransport::new(read_half, write_half)).await });
    (host, manager_side)
}

/// Reads exactly one typed reply.
///
/// The reply is an unframed JSON value, so a naive "read until `from_slice`
/// succeeds" loop deadlocks: a value split across two reads looks exactly like a
/// malformed one until the rest arrives, and the second read then blocks
/// forever because the host already sent everything. The streaming
/// deserialiser is the tool that distinguishes those cases -- it reports "end of
/// input" rather than "malformed" when the buffer merely holds a prefix of a
/// complete value.
async fn read_reply<R>(reader: &mut R) -> ApphostReply
where
    R: AsyncRead + Unpin,
{
    let mut raw = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let mut stream = serde_json::Deserializer::from_slice(&raw).into_iter::<ApphostReply>();
        match stream.next() {
            Some(Ok(reply)) => return reply,
            Some(Err(error)) => panic!("the host sent a malformed reply: {error}"),
            // Either an eof-class error or no value at all: read more bytes.
            None => {}
        }
        assert!(
            raw.len() <= MAX_BOOTSTRAP_PAYLOAD_BYTES,
            "a reply must stay inside the same envelope as the request"
        );
        let read = timeout(DEADLINE, reader.read(&mut chunk))
            .await
            .expect("a reply must arrive rather than hang")
            .expect("read reply");
        assert_ne!(read, 0, "the host closed without sending a complete reply");
        raw.extend_from_slice(&chunk[..read]);
    }
}

#[tokio::test]
async fn a_secured_launch_is_refused_by_the_host_before_anything_is_executed() {
    let scratch = Scratch::new("secured");
    // If this ever ran it would leave a marker behind. The assertion that the
    // marker is absent is the real evidence: no exec happened at all.
    scratch.write_executable("app", "#!/bin/sh\ntouch ran-marker\nsleep 30\n");

    let request = launch_request(&scratch, "app", i2pr_app_proto::LaunchProfile::Secured);

    // First gate: the contract refuses to even encode it.
    assert_eq!(
        request.encode(),
        Err(i2pr_app_manager_proto::apphost::ApphostBootstrapError::SecuredUnavailable)
    );

    // Second gate: put the bytes on the wire anyway and let the *host* decide.
    let (host, mut manager) = start_host();
    write_framed(&mut manager, &encode_without_validation(&request))
        .await
        .expect("bootstrap");

    let reply = read_reply(&mut manager).await;
    assert_eq!(
        reply,
        ApphostReply::Failed {
            reason: ApphostFailureReason::SecuredUnavailable,
            diagnostic: Some(error_text_for(ApphostFailureReason::SecuredUnavailable)),
        }
    );

    let outcome = timeout(DEADLINE, host)
        .await
        .expect("a refused launch must fail fast")
        .expect("host joined");
    assert!(
        matches!(outcome, Err(ApphostError::Bootstrap(_))),
        "{outcome:?}"
    );
    assert!(
        !scratch.path().join("ran-marker").exists(),
        "a Secured launch must be refused before exec, not merely reported afterwards"
    );
}

/// The diagnostic the host sends for a given refusal.
///
/// Pinned here on purpose: the diagnostic is derived from the typed error, so a
/// change in what an operator is told is a visible change in this test rather
/// than a silent one.
fn error_text_for(reason: ApphostFailureReason) -> String {
    match reason {
        ApphostFailureReason::SecuredUnavailable => {
            ApphostBootstrapError::SecuredUnavailable.to_string()
        }
        other => panic!("no pinned diagnostic for {other:?}"),
    }
}

#[tokio::test]
async fn an_entrypoint_outside_the_root_is_refused_by_the_host() {
    let scratch = Scratch::new("escape");
    let outside = scratch
        .path()
        .parent()
        .expect("temp parent")
        .join(format!("i2pr-apphost-outside-{}", std::process::id()));
    let _ = std::fs::remove_file(&outside);
    std::fs::write(&outside, b"#!/bin/sh\nsleep 30\n").expect("outside file");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::os::unix::fs::symlink(&outside, scratch.path().join("app")).expect("symlink");

    let request = launch_request(&scratch, "app", i2pr_app_proto::LaunchProfile::UnsafeDirect);
    let (host, mut manager) = start_host();
    write_bootstrap(&mut manager, &request)
        .await
        .expect("bootstrap");
    let reply = read_reply(&mut manager).await;
    let outcome = timeout(DEADLINE, host)
        .await
        .expect("a refused launch must fail fast")
        .expect("host joined");
    let _ = std::fs::remove_file(&outside);

    assert!(
        matches!(
            reply,
            ApphostReply::Failed {
                reason: ApphostFailureReason::EntrypointEscapesRoot,
                ..
            }
        ),
        "a symlinked entrypoint must be refused, got {reply:?}"
    );
    assert!(
        matches!(outcome, Err(ApphostError::EntrypointEscapesRoot)),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_missing_entrypoint_is_refused_by_name() {
    let scratch = Scratch::new("missing");
    let request = launch_request(
        &scratch,
        "absent",
        i2pr_app_proto::LaunchProfile::UnsafeDirect,
    );
    let (host, mut manager) = start_host();
    write_bootstrap(&mut manager, &request)
        .await
        .expect("bootstrap");
    let reply = read_reply(&mut manager).await;
    let _ = timeout(DEADLINE, host)
        .await
        .expect("a refused launch must fail fast")
        .expect("host joined");

    assert!(
        matches!(
            reply,
            ApphostReply::Failed {
                reason: ApphostFailureReason::EntrypointNotFound,
                ..
            }
        ),
        "got {reply:?}"
    );
}

#[tokio::test]
async fn a_launched_application_is_ready_and_then_relays_bytes_verbatim() {
    let scratch = Scratch::new("relay");
    scratch.write_executable("app", "#!/bin/sh\ncat\n");
    let request = launch_request(&scratch, "app", i2pr_app_proto::LaunchProfile::UnsafeDirect);
    let (host, mut manager) = start_host();

    write_bootstrap(&mut manager, &request)
        .await
        .expect("bootstrap");
    let reply = read_reply(&mut manager).await;
    let ApphostReply::Ready { instance_id } = &reply else {
        panic!("expected a ready reply, got {reply:?}");
    };
    assert_eq!(
        instance_id, "7",
        "the canonical instance id must round-trip"
    );

    // After `Ready` the channel is byte-transparent: the application sees
    // exactly what the manager wrote, and its output comes back verbatim.
    const PAYLOAD: &[u8] = b"I2PA\x01\x00payload-bytes";
    manager
        .write_all(PAYLOAD)
        .await
        .expect("application-facing write");
    let mut echoed = vec![0_u8; PAYLOAD.len()];
    timeout(DEADLINE, manager.read_exact(&mut echoed))
        .await
        .expect("application bytes must return rather than hang")
        .expect("read echo");
    assert_eq!(echoed, PAYLOAD);

    // Closing the transport is the only shutdown signal; the host must reap
    // the application rather than leave it attached to pipes nobody reads.
    drop(manager);
    timeout(DEADLINE, host)
        .await
        .expect("the host must not outlive its transport")
        .expect("host joined")
        .expect("clean relay");
}

#[tokio::test]
async fn an_application_that_never_exits_is_killed_rather_than_leaked() {
    let scratch = Scratch::new("stubborn");
    // Ignores stdin EOF, so only a forced kill can end it.
    scratch.write_executable("app", "#!/bin/sh\nwhile true; do sleep 1; done\n");
    let request = launch_request(&scratch, "app", i2pr_app_proto::LaunchProfile::UnsafeDirect);
    let (host, mut manager) = start_host();

    write_bootstrap(&mut manager, &request)
        .await
        .expect("bootstrap");
    let reply = read_reply(&mut manager).await;
    assert!(matches!(reply, ApphostReply::Ready { .. }), "{reply:?}");

    // Dropping the manager side must terminate the host. The relay's close
    // grace is finite and escalates to a kill, so this is the assertion that
    // the direct child is owned to the end rather than leaked.
    drop(manager);
    timeout(DEADLINE, host)
        .await
        .expect("a host whose manager vanished must not linger")
        .expect("host joined")
        .expect("clean termination");
}

#[tokio::test]
async fn a_bad_magic_is_refused_without_a_reply() {
    let (_host, mut manager) = start_host();
    manager
        .write_all(b"XXXX\x01\x00\x01\x00\x00")
        .await
        .expect("write bad magic");

    let mut silence = Vec::new();
    let mut probe = [0_u8; 16];
    if let Ok(Ok(read)) = timeout(Duration::from_millis(200), manager.read(&mut probe)).await {
        silence.extend_from_slice(&probe[..read]);
    }
    assert!(
        silence.is_empty(),
        "a peer that cannot present framing is not answered in protocol, got {silence:?}"
    );
}

#[tokio::test]
async fn a_truncated_bootstrap_is_refused_without_hanging() {
    let (host, mut manager) = start_host();
    // Valid handshake, valid length prefix, then nothing.
    manager
        .write_all(&ApphostHandshake::encode())
        .await
        .expect("handshake");
    manager
        .write_all(&64_u32.to_be_bytes())
        .await
        .expect("length");

    let outcome = timeout(DEADLINE, host)
        .await
        .expect("a truncated bootstrap must fail fast, not hang")
        .expect("host joined");
    assert!(
        matches!(
            outcome,
            Err(ApphostError::Bootstrap(
                ApphostBootstrapError::Truncated | ApphostBootstrapError::Malformed
            ))
        ),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn an_oversize_payload_is_refused_before_it_is_allocated() {
    let (host, mut manager) = start_host();
    let mut frame = ApphostHandshake::encode().to_vec();
    frame.extend_from_slice(&(MAX_BOOTSTRAP_PAYLOAD_BYTES as u32 + 1).to_be_bytes());
    manager.write_all(&frame).await.expect("write");

    let outcome = timeout(DEADLINE, host)
        .await
        .expect("an oversize payload must fail fast")
        .expect("host joined");
    assert!(
        matches!(
            outcome,
            Err(ApphostError::Bootstrap(
                ApphostBootstrapError::LimitExceeded("bootstrap payload")
            ))
        ),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn the_bootstrap_grace_bounds_a_manager_that_says_nothing() {
    // A manager that opens the transport and never writes must not be able to
    // keep a supervisor alive forever.
    let (_writer, mut reader) = tokio::io::duplex(1024);
    let outcome = read_launch_request(&mut reader, Duration::from_millis(100))
        .await
        .expect_err("a silent manager yields no request");
    assert_eq!(outcome, ApphostBootstrapError::Truncated);

    // The framing constants are part of the contract with `i2pr-appd`; pinning
    // them here means a change to either side breaks a test rather than a
    // handshake at runtime.
    const _: () = assert!(MAX_BOOTSTRAP_PAYLOAD_BYTES >= 4096);
    const _: () = assert!(APPHOST_BOOTSTRAP_GRACE.as_secs() >= 1);
    assert_eq!(LENGTH_PREFIX_BYTES, 4);
    assert_eq!(APPHOST_BOOTSTRAP_BYTES, 9);
}

#[tokio::test]
async fn a_reply_is_flushed_before_the_channel_turns_transparent() {
    let (mut host, mut manager) = tokio::io::duplex(8 * 1024);
    write_reply(
        &mut host,
        &ApphostReply::Failed {
            reason: ApphostFailureReason::SecuredUnavailable,
            diagnostic: None,
        },
    )
    .await
    .expect("reply");
    assert_eq!(
        read_reply(&mut manager).await,
        ApphostReply::Failed {
            reason: ApphostFailureReason::SecuredUnavailable,
            diagnostic: None,
        }
    );
}
