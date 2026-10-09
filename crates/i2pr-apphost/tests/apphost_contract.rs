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
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use std::process::{Command, Stdio};
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

/// A process-unique suffix, not a bare timestamp.
///
/// Two threads can observe the same `clock_gettime` value, and two tests that then
/// build the same path race one test's write against the other's `execve` — which
/// surfaces as `ETXTBSY` inside the code under test. The sequence number makes the
/// name unique regardless of clock resolution.
fn unique_suffix() -> String {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{nanos:x}-{sequence:x}")
}

/// A scratch directory that cleans itself up, so a failing test cannot leave an
/// executable behind for the next one.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("i2pr-apphost-{label}-{}", unique_suffix()));
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

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn linux_secured_refuses_interpreter_elf_before_ready_or_exec() {
    use std::io::{Read, Write};

    let scratch = Scratch::new("secured-dynamic-elf");
    let package_root = scratch.path().join("package");
    let data_root = scratch.path().join("app-data");
    std::fs::create_dir(&package_root).expect("package root");
    std::fs::create_dir(&data_root).expect("app data root");
    let executable = package_root.join("dynamic");
    std::fs::copy(
        std::env::current_exe().expect("current test executable"),
        &executable,
    )
    .expect("copy dynamically linked test executable");
    let request = LaunchRequest {
        principal: i2pr_app_manager_proto::ManagerPrincipal {
            app_id: i2pr_app_proto::AppId::parse("dynamic-probe").unwrap(),
            instance_id: i2pr_app_manager_proto::ManagerInstanceId::new(1),
            publisher_id: None,
        },
        launch_profile: i2pr_app_proto::LaunchProfile::Secured,
        root: i2pr_app_manager_proto::apphost::LaunchRoot::new(package_root.to_str().unwrap())
            .unwrap(),
        data_root: i2pr_app_manager_proto::apphost::AppDataRoot::new(data_root.to_str().unwrap())
            .unwrap(),
        entrypoint: i2pr_app_manager_proto::apphost::Entrypoint::new("dynamic").unwrap(),
        argv: vec!["should-not-run".to_owned()],
        environment: i2pr_app_manager_proto::apphost::SanitizedEnvironment::new(BTreeMap::new())
            .unwrap(),
        resources: i2pr_app_manager_proto::apphost::DescriptiveResourceRequest {
            requested_memory_bytes: 128 * 1024 * 1024,
            requested_open_files: 32,
        },
    };
    let mut host = Command::new(env!("CARGO_BIN_EXE_i2pr-apphost"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn production apphost");
    let mut manager_in = host.stdin.take().unwrap();
    let mut manager_out = host.stdout.take().unwrap();
    manager_in.write_all(&ApphostHandshake::encode()).unwrap();
    let payload = request.encode().unwrap();
    manager_in
        .write_all(&(payload.len() as u32).to_be_bytes())
        .unwrap();
    manager_in.write_all(&payload).unwrap();
    manager_in.flush().unwrap();
    let mut reply_bytes = Vec::new();
    let reply = loop {
        let mut byte = [0_u8; 1];
        manager_out
            .read_exact(&mut byte)
            .expect("typed refusal reply");
        reply_bytes.push(byte[0]);
        if let Ok(reply) = serde_json::from_slice::<ApphostReply>(&reply_bytes) {
            break reply;
        }
        assert!(
            reply_bytes.len() < 4096,
            "apphost refusal exceeded its bound"
        );
    };
    assert!(matches!(
        reply,
        ApphostReply::Failed {
            reason: ApphostFailureReason::SecuredUnavailable,
            ..
        }
    ));
    drop(manager_in);
    let status = host.wait().expect("reap refused apphost");
    assert!(
        !status.success(),
        "unsupported dynamic ELF must fail before exec"
    );
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
        data_root: {
            let path = scratch.path().with_extension("data");
            std::fs::create_dir_all(&path).expect("data root");
            i2pr_app_manager_proto::apphost::AppDataRoot::new(path.to_str().expect("utf-8"))
                .expect("data root")
        },
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

    // Wire acceptance does not grant launch authority; the host still refuses
    // until a platform backend returns complete evidence.
    assert!(request.encode().is_ok());

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
    let ApphostReply::Ready { instance_id, .. } = &reply else {
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

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn linux_secured_exec_enforces_filesystem_environment_fds_network_and_fork() {
    let scratch = Scratch::new("secured-linux");
    let package_root = scratch.path().join("package");
    std::fs::create_dir(&package_root).expect("package root");
    let source = package_root.join("probe.c");
    let startup = package_root.join("start.S");
    let executable = package_root.join("probe");
    std::fs::write(
        &source,
        r#"#if defined(__x86_64__)
static long sc3(long n,long a,long b,long c) { long r; __asm__ volatile("syscall":"=a"(r):"a"(n),"D"(a),"S"(b),"d"(c):"rcx","r11","memory"); return r; }
static long sc4(long n,long a,long b,long c,long d) { register long r10 __asm__("r10")=d; long r; __asm__ volatile("syscall":"=a"(r):"a"(n),"D"(a),"S"(b),"d"(c),"r"(r10):"rcx","r11","memory"); return r; }
static long sc6(long n,long a,long b,long c,long d,long e,long f) { register long r10 __asm__("r10")=d; register long r8 __asm__("r8")=e; register long r9 __asm__("r9")=f; long r; __asm__ volatile("syscall":"=a"(r):"a"(n),"D"(a),"S"(b),"d"(c),"r"(r10),"r"(r8),"r"(r9):"rcx","r11","memory"); return r; }
#define NR_READ 0
#define NR_WRITE 1
#define NR_CLOSE 3
#define NR_FCNTL 72
#define NR_OPENAT 257
#define NR_SOCKET 41
#define NR_CLONE 56
#define NR_PTRACE 101
#define NR_PROCESS_VM_READV 310
#define NR_PIDFD_OPEN 434
#define NR_IO_URING_SETUP 425
#define NR_SETRLIMIT 160
#define NR_PRLIMIT64 302
#define NR_EXIT_GROUP 231
#define NR_MMAP 9
#define NR_MUNMAP 11
#elif defined(__aarch64__)
static long sc3(long n,long a,long b,long c) { register long x0 __asm__("x0")=a; register long x1 __asm__("x1")=b; register long x2 __asm__("x2")=c; register long x8 __asm__("x8")=n; __asm__ volatile("svc 0":"+r"(x0):"r"(x1),"r"(x2),"r"(x8):"memory"); return x0; }
static long sc4(long n,long a,long b,long c,long d) { register long x0 __asm__("x0")=a; register long x1 __asm__("x1")=b; register long x2 __asm__("x2")=c; register long x3 __asm__("x3")=d; register long x8 __asm__("x8")=n; __asm__ volatile("svc 0":"+r"(x0):"r"(x1),"r"(x2),"r"(x3),"r"(x8):"memory"); return x0; }
static long sc6(long n,long a,long b,long c,long d,long e,long f) { register long x0 __asm__("x0")=a; register long x1 __asm__("x1")=b; register long x2 __asm__("x2")=c; register long x3 __asm__("x3")=d; register long x4 __asm__("x4")=e; register long x5 __asm__("x5")=f; register long x8 __asm__("x8")=n; __asm__ volatile("svc 0":"+r"(x0):"r"(x1),"r"(x2),"r"(x3),"r"(x4),"r"(x5),"r"(x8):"memory"); return x0; }
#define NR_READ 63
#define NR_WRITE 64
#define NR_CLOSE 57
#define NR_FCNTL 25
#define NR_OPENAT 56
#define NR_SOCKET 198
#define NR_CLONE 220
#define NR_PTRACE 117
#define NR_PROCESS_VM_READV 270
#define NR_PIDFD_OPEN 434
#define NR_IO_URING_SETUP 425
#define NR_SETRLIMIT 164
#define NR_PRLIMIT64 261
#define NR_EXIT_GROUP 94
#define NR_MMAP 222
#define NR_MUNMAP 215
#else
#error unsupported architecture
#endif
static int eq(const char *a,const char *b) { while (*a && *a==*b) { ++a; ++b; } return *a==*b; }
static const char *env(char **values,const char *key) { for (;*values;++values) { const char *v=*values,*k=key; while (*k && *v==*k) { ++v; ++k; } if (!*k && *v=='=') return v+1; } return 0; }
static long openat_(const char *p,long flags,long mode) { return sc4(NR_OPENAT,-100,(long)p,flags,mode); }
static int write_all(long fd,const char *p,long n) { return sc3(NR_WRITE,fd,(long)p,n)==n ? 0 : 1; }
static int main_probe(long argc,char **argv,char **envp) {
  if (argc!=2) return 40;
  if (eq(argv[1],"env-fd-data")) {
    if (env(envp,"PATH") || !env(envp,"I2PR_APP_DATA_DIR")) return 41;
    for (long fd=3;fd<128;++fd) if (sc3(NR_FCNTL,fd,1,0)>=0) return 42;
    const char *data=env(envp,"I2PR_APP_DATA_DIR"); char path[512]; long i=0;
    while (data[i] && i<480) { path[i]=data[i]; ++i; } const char suffix[]="/persistent"; for(long j=0;suffix[j];++j) path[i++]=suffix[j]; path[i]=0;
    long file=openat_(path,1|64|512,0600); if(file<0) return 43; if(write_all(file,"ok",2)) return 44; sc3(NR_CLOSE,file,0,0); return 0;
  }
  if (eq(argv[1],"persistent")) {
    const char *data=env(envp,"I2PR_APP_DATA_DIR"); char path[512],bytes[2]; long i=0; while(data[i]&&i<480){path[i]=data[i];++i;} const char suffix[]="/persistent"; for(long j=0;suffix[j];++j) path[i++]=suffix[j]; path[i]=0;
    long file=openat_(path,0,0); if(file<0 || sc3(NR_READ,file,(long)bytes,2)!=2) return 45; sc3(NR_CLOSE,file,0,0); return bytes[0]=='o'&&bytes[1]=='k' ? 0:46;
  }
  if (eq(argv[1],"package-write")) return openat_("probe",1,0)<0 ? 0:47;
  if (eq(argv[1],"proc")) return openat_("/proc/self/status",0,0)<0 ? 0:48;
  if (eq(argv[1],"sibling")) return openat_(env(envp,"PROBE_SIBLING"),0,0)<0 ? 0:49;
  if (eq(argv[1],"filesystem")) return openat_("/etc/passwd",0,0)<0 ? 0:50;
  if (eq(argv[1],"memory-limit")) { long p=sc6(NR_MMAP,0,8*1024*1024,3,0x22,-1,0); if(p<0) return 54; sc3(NR_MUNMAP,p,8*1024*1024,0); p=sc6(NR_MMAP,0,96*1024*1024,3,0x22,-1,0); return p<0 ? 0:55; }
  if (eq(argv[1],"open-file-limit")) { const char *data=env(envp,"I2PR_APP_DATA_DIR"); if(!data) return 57; long fds[64],count=0; for(long i=0;i<64;++i) { char path[512]; long j=0; while(data[j]&&j<500){path[j]=data[j];++j;} path[j++]='/'; path[j++]='f'; path[j++]=(char)('A'+i); path[j]=0; long fd=openat_(path,1|64,0600); if(fd<0) break; fds[count++]=fd; } for(long i=0;i<count;++i) sc3(NR_CLOSE,fds[i],0,0); return count==29 ? 0:(100+count); }
  if (eq(argv[1],"echo")) { char bytes[4]; if(sc3(NR_READ,0,(long)bytes,4)!=4) return 57; return write_all(1,bytes,4); }
  if (eq(argv[1],"network") || eq(argv[1],"loopback")) { sc3(NR_SOCKET,2,1,0); return 51; }
  if (eq(argv[1],"network6")) { sc3(NR_SOCKET,10,1,0); return 58; }
  if (eq(argv[1],"udp")) { sc3(NR_SOCKET,2,2,0); return 59; }
  if (eq(argv[1],"ptrace")) { sc4(NR_PTRACE,0,0,0,0); return 60; }
  if (eq(argv[1],"process-vm")) { sc6(NR_PROCESS_VM_READV,1,0,0,0,0,0); return 61; }
  if (eq(argv[1],"pidfd")) { sc3(NR_PIDFD_OPEN,1,0,0); return 62; }
  if (eq(argv[1],"io-uring")) { sc3(NR_IO_URING_SETUP,1,0,0); return 63; }
  if (eq(argv[1],"raise-rlimit")) { sc4(NR_SETRLIMIT,9,0,0,0); return 64; }
  if (eq(argv[1],"prlimit")) { sc4(NR_PRLIMIT64,0,9,1,0); return 65; }
  if (eq(argv[1],"fork")) { sc3(NR_CLONE,17,0,0); return 52; }
  return 53;
}
int main(long argc,char **argv,char **envp) { return main_probe(argc,argv,envp); }
"#,
    )
    .expect("write static fixture source");
    std::fs::write(
        &startup,
        r#"#if defined(__x86_64__)
.global _start
_start:
  xor %rbp,%rbp
  mov (%rsp),%rdi
  lea 8(%rsp),%rsi
  lea 16(%rsp,%rdi,8),%rdx
  and $-16,%rsp
  call main
  mov %eax,%edi
  mov $231,%eax
  syscall
#elif defined(__aarch64__)
.global _start
_start:
  mov x29,xzr
  ldr x0,[sp]
  add x1,sp,#8
  add x2,x1,x0,lsl #3
  add x2,x2,#8
  bl main
  mov x0,x0
  mov x8,#94
  svc #0
#endif
 .section .note.GNU-stack,"",@progbits
"#,
    )
    .expect("write static startup");
    let status = Command::new("cc")
        .arg("-nostdlib")
        .arg("-static")
        .arg("-fno-stack-protector")
        .arg("-fno-pie")
        .arg("-no-pie")
        .arg("-O2")
        .arg(&startup)
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .status()
        .expect("Linux qualification runner must provide a C compiler");
    assert!(
        status.success(),
        "static C fixture compilation failed: {status}"
    );
    let rust_source = package_root.join("rust-probe.rs");
    let rust_executable = package_root.join("rust-probe");
    std::fs::write(
        &rust_source,
        "fn main() { assert!(std::env::var_os(\"PATH\").is_none()); let p = std::path::PathBuf::from(std::env::var_os(\"I2PR_APP_DATA_DIR\").unwrap()).join(\"rust-state\"); std::fs::write(p, b\"ok\").unwrap(); }",
    ).expect("write Rust app fixture");
    let rust_status = Command::new("rustc")
        .arg(&rust_source)
        .arg("-C")
        .arg("target-feature=+crt-static")
        .arg("-C")
        .arg("panic=abort")
        .arg("-o")
        .arg(&rust_executable)
        .status()
        .expect("Linux qualification runner must provide rustc");
    assert!(
        rust_status.success(),
        "static Rust fixture compilation failed: {rust_status}"
    );

    for (mode, data_label, should_be_sigsys, memory_limit) in [
        ("env-fd-data", "persistent", false, 256 * 1024 * 1024),
        ("persistent", "persistent", false, 256 * 1024 * 1024),
        ("filesystem", "filesystem", false, 256 * 1024 * 1024),
        ("package-write", "package-write", false, 256 * 1024 * 1024),
        ("proc", "proc", false, 256 * 1024 * 1024),
        ("sibling", "sibling", false, 256 * 1024 * 1024),
        ("memory-limit", "memory-limit", false, 64 * 1024 * 1024),
        (
            "open-file-limit",
            "open-file-limit",
            false,
            256 * 1024 * 1024,
        ),
        ("echo", "echo", false, 256 * 1024 * 1024),
        ("rust-static", "rust-static", false, 256 * 1024 * 1024),
        ("network", "network", true, 256 * 1024 * 1024),
        ("network6", "network6", true, 256 * 1024 * 1024),
        ("udp", "udp", true, 256 * 1024 * 1024),
        ("loopback", "loopback", true, 256 * 1024 * 1024),
        ("ptrace", "ptrace", true, 256 * 1024 * 1024),
        ("process-vm", "process-vm", true, 256 * 1024 * 1024),
        ("pidfd", "pidfd", true, 256 * 1024 * 1024),
        ("io-uring", "io-uring", true, 256 * 1024 * 1024),
        ("raise-rlimit", "raise-rlimit", true, 256 * 1024 * 1024),
        ("prlimit", "prlimit", true, 256 * 1024 * 1024),
        ("fork", "fork", true, 256 * 1024 * 1024),
    ] {
        let data_root = scratch.path().join(format!("data-{data_label}"));
        std::fs::create_dir_all(&data_root).expect("app data directory");
        let sibling_data = scratch.path().join("sibling-data");
        std::fs::create_dir_all(&sibling_data).expect("sibling data directory");
        std::fs::write(sibling_data.join("private"), b"secret").expect("sibling marker");
        let request = LaunchRequest {
            principal: i2pr_app_manager_proto::ManagerPrincipal {
                app_id: i2pr_app_proto::AppId::parse("secured-probe").unwrap(),
                instance_id: i2pr_app_manager_proto::ManagerInstanceId::new(17),
                publisher_id: None,
            },
            launch_profile: i2pr_app_proto::LaunchProfile::Secured,
            root: i2pr_app_manager_proto::apphost::LaunchRoot::new(package_root.to_str().unwrap())
                .unwrap(),
            data_root: i2pr_app_manager_proto::apphost::AppDataRoot::new(
                data_root.to_str().unwrap(),
            )
            .unwrap(),
            entrypoint: i2pr_app_manager_proto::apphost::Entrypoint::new(
                if mode == "rust-static" {
                    "rust-probe"
                } else {
                    "probe"
                },
            )
            .unwrap(),
            argv: vec![mode.to_owned()],
            environment: i2pr_app_manager_proto::apphost::SanitizedEnvironment::new(
                BTreeMap::from([(
                    "PROBE_SIBLING".to_owned(),
                    sibling_data.join("private").to_str().unwrap().to_owned(),
                )]),
            )
            .unwrap(),
            resources: i2pr_app_manager_proto::apphost::DescriptiveResourceRequest {
                requested_memory_bytes: memory_limit,
                requested_open_files: 32,
            },
        };
        let mut host = Command::new(env!("CARGO_BIN_EXE_i2pr-apphost"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn production apphost binary");
        let mut manager_in = host.stdin.take().unwrap();
        let mut manager_out = host.stdout.take().unwrap();
        use std::io::{Read, Write};
        manager_in.write_all(&ApphostHandshake::encode()).unwrap();
        let payload = request.encode().unwrap();
        manager_in
            .write_all(&(payload.len() as u32).to_be_bytes())
            .unwrap();
        manager_in.write_all(&payload).unwrap();
        manager_in.flush().unwrap();
        let mut reply_bytes = Vec::new();
        let reply = loop {
            let mut byte = [0_u8; 1];
            if let Err(error) = manager_out.read_exact(&mut byte) {
                let status = host.wait().expect("reap failed apphost");
                panic!("apphost closed before reply ({error}; {status:?})");
            }
            reply_bytes.push(byte[0]);
            if let Ok(reply) = serde_json::from_slice::<ApphostReply>(&reply_bytes) {
                break reply;
            }
            assert!(reply_bytes.len() < 4096, "apphost reply exceeded its bound");
        };
        let ApphostReply::Ready {
            attestation: Some(attestation),
            ..
        } = reply
        else {
            panic!("Linux secured apphost must return a complete attestation: {reply:?}");
        };
        attestation.validate_secured().unwrap();
        if mode == "echo" {
            manager_in.write_all(b"I2PR").unwrap();
            let mut response = [0_u8; 4];
            manager_out.read_exact(&mut response).unwrap();
            assert_eq!(
                &response, b"I2PR",
                "secured exec must retain the broker channel"
            );
        }
        let status = host.wait().expect("application process is reaped");
        if should_be_sigsys {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                status.signal(),
                Some(31),
                "{mode} must be denied by seccomp: {status:?}"
            );
        } else {
            assert_eq!(
                status.code(),
                Some(0),
                "{mode} did not complete under the sandbox: {status:?}"
            );
        }
        if mode == "env-fd-data" {
            assert_eq!(std::fs::read(data_root.join("persistent")).unwrap(), b"ok");
        }
    }
}
