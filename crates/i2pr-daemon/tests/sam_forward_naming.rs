//! Plan 139 loopback forwarding and naming tests.

use std::net::SocketAddr;
use std::sync::Arc;

use i2pr_api::sam::limits::SamLimits;
use i2pr_daemon::config::SamConfig;
use i2pr_daemon::sam::{ForwardBridgeError, SamServiceState};
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

fn config() -> SamConfig {
    SamConfig {
        enabled: true,
        bind_address: "127.0.0.1".parse().unwrap(),
        port: 0,
        limits: SamLimits::loopback_test_profile(),
    }
}

fn scope(parent: &CancellationToken) -> ChildScope {
    ChildScope::for_test(parent, ChildFailurePolicy::FailParent)
}

async fn start() -> (
    Arc<SamServiceState>,
    SocketAddr,
    ChildScope,
    CancellationToken,
) {
    let state = Arc::new(SamServiceState::new(config()).unwrap());
    let (listener, address) = state.bind(state.bind_address()).await.unwrap();
    let parent = CancellationToken::new();
    let children = scope(&parent);
    let task_state = Arc::clone(&state);
    let task_children = children.clone();
    let task_parent = parent.clone();
    children
        .clone()
        .spawn(move |_| async move {
            let _ = task_state.serve(listener, task_children, task_parent).await;
            Ok(())
        })
        .unwrap();
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    (state, address, children, parent)
}

async fn line(stream: &mut TcpStream) -> String {
    let mut output = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        if stream.read_exact(&mut byte).await.is_err() {
            break;
        }
        output.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    String::from_utf8(output).unwrap()
}

fn base32(bytes: &[u8; 32]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut output = String::with_capacity(52);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in bytes {
        accumulator = (accumulator << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            output.push(ALPHABET[((accumulator >> bits) & 0x1f) as usize] as char);
        }
        if bits > 0 {
            accumulator &= (1_u32 << bits) - 1;
        } else {
            accumulator = 0;
        }
    }
    if bits != 0 {
        output.push(ALPHABET[((accumulator << (5 - bits)) & 0x1f) as usize] as char);
    }
    output
}

async fn hello(stream: &mut TcpStream) {
    stream
        .write_all(b"HELLO VERSION MIN=3.1 MAX=3.1\n")
        .await
        .unwrap();
    assert!(line(stream).await.starts_with("HELLO REPLY RESULT=OK"));
}

async fn create(stream: &mut TcpStream) -> String {
    stream
        .write_all(b"SESSION CREATE STYLE=STREAM ID=forward DESTINATION=TRANSIENT\n")
        .await
        .unwrap();
    let reply = line(stream).await;
    let start = reply.find("DESTINATION=").unwrap() + "DESTINATION=".len();
    let end = reply[start..]
        .find([' ', '\r', '\n'])
        .map_or(reply.len(), |index| start + index);
    reply[start..end].to_owned()
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn forward_registers_real_loopback_target_and_owner_close_removes_it() {
    let (state, address, children, parent) = start().await;
    let mut control = TcpStream::connect(address).await.unwrap();
    hello(&mut control).await;
    let _public = create(&mut control).await;
    let session = i2pr_api::SamSessionId::new("forward").unwrap();

    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    let mut forward = TcpStream::connect(address).await.unwrap();
    hello(&mut forward).await;
    let command = format!(
        "STREAM FORWARD ID=forward PORT={} HOST=127.0.0.1\n",
        target_address.port()
    );
    forward.write_all(command.as_bytes()).await.unwrap();
    assert_eq!(line(&mut forward).await, "STREAM STATUS RESULT=OK\n");
    assert!(state.forward_registration(&session).is_some());

    let target_task = tokio::spawn(async move {
        let (target, _) = target_listener.accept().await.unwrap();
        let mut reader = BufReader::new(target);
        let mut metadata = Vec::new();
        reader.read_until(b'\n', &mut metadata).await.unwrap();
        assert_eq!(metadata, b"DESTINATION=peer-public\n");
        let mut payload = [0_u8; 5];
        reader.read_exact(&mut payload).await.unwrap();
        assert_eq!(&payload, b"hello");
        let mut target = reader.into_inner();
        target.write_all(b"world").await.unwrap();
        target.shutdown().await.unwrap();
    });

    let source_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source_address = source_listener.local_addr().unwrap();
    let mut source_client = TcpStream::connect(source_address).await.unwrap();
    let (source, _) = source_listener.accept().await.unwrap();
    let bridge_state = Arc::clone(&state);
    let bridge_session = session.clone();
    let bridge_cancel = CancellationToken::new();
    let bridge = tokio::spawn(async move {
        bridge_state
            .bridge_forwarded_stream(&bridge_session, source, Some("peer-public"), bridge_cancel)
            .await
    });
    source_client.write_all(b"hello").await.unwrap();
    let mut reply = [0_u8; 5];
    source_client.read_exact(&mut reply).await.unwrap();
    assert_eq!(&reply, b"world");
    target_task.await.unwrap();
    bridge.await.unwrap().unwrap();

    // QUIT closes this owner connection through the normal SAM lifecycle.
    // Waiting for the peer EOF synchronizes with the server task's teardown
    // instead of relying on a scheduler-dependent number of yields.
    forward.write_all(b"QUIT\n").await.unwrap();
    let mut eof = [0_u8; 1];
    assert_eq!(forward.read(&mut eof).await.unwrap(), 0);
    assert!(state.forward_registration(&session).is_none());
    assert_eq!(
        state.stream_registry().inbound_mode(&session).unwrap(),
        i2pr_api::InboundMode::Idle
    );

    drop(control);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = children.shutdown().await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn silent_forward_passes_application_bytes_without_metadata() {
    let (state, address, children, parent) = start().await;
    let mut control = TcpStream::connect(address).await.unwrap();
    hello(&mut control).await;
    let _public = create(&mut control).await;
    let session = i2pr_api::SamSessionId::new("forward").unwrap();

    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    let mut forward = TcpStream::connect(address).await.unwrap();
    hello(&mut forward).await;
    forward
        .write_all(
            format!(
                "STREAM FORWARD ID=forward PORT={} HOST=127.0.0.1 SILENT=true\n",
                target_address.port()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    assert_eq!(line(&mut forward).await, "STREAM STATUS RESULT=OK\n");

    let target_task = tokio::spawn(async move {
        let (mut target, _) = target_listener.accept().await.unwrap();
        let mut payload = [0_u8; 5];
        target.read_exact(&mut payload).await.unwrap();
        assert_eq!(&payload, b"hello");
        target.write_all(b"world").await.unwrap();
        target.shutdown().await.unwrap();
    });
    let source_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source_address = source_listener.local_addr().unwrap();
    let mut source_client = TcpStream::connect(source_address).await.unwrap();
    let (source, _) = source_listener.accept().await.unwrap();
    let bridge = tokio::spawn({
        let state = Arc::clone(&state);
        let cancellation = CancellationToken::new();
        async move {
            state
                .bridge_forwarded_stream(&session, source, Some("peer-public"), cancellation)
                .await
        }
    });
    source_client.write_all(b"hello").await.unwrap();
    let mut reply = [0_u8; 5];
    source_client.read_exact(&mut reply).await.unwrap();
    assert_eq!(&reply, b"world");
    target_task.await.unwrap();
    bridge.await.unwrap().unwrap();

    drop(forward);
    drop(control);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = children.shutdown().await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn naming_me_is_session_scoped_and_unknown_i2p_is_not_found() {
    let (state, address, children, parent) = start().await;
    let mut session = TcpStream::connect(address).await.unwrap();
    hello(&mut session).await;
    let _private = create(&mut session).await;
    session.write_all(b"NAMING LOOKUP NAME=ME\n").await.unwrap();
    let reply = line(&mut session).await;
    assert!(reply.contains("NAMING REPLY RESULT=OK"), "{reply}");
    let public = reply
        .split_whitespace()
        .find_map(|token| token.strip_prefix("VALUE=").map(str::to_owned))
        .expect("NAMING LOOKUP NAME=ME returns public destination");
    assert!(!public.is_empty());

    session
        .write_all(format!("NAMING LOOKUP NAME={public}\n").as_bytes())
        .await
        .unwrap();
    let reply = line(&mut session).await;
    assert!(reply.contains("NAMING REPLY RESULT=OK"), "{reply}");
    assert!(reply.contains(&format!("VALUE={public}")));

    let entry = state
        .session_registry()
        .get(&i2pr_api::SamSessionId::new("forward").unwrap())
        .unwrap();
    let b32_name = format!("{}.b32.i2p", base32(entry.destination_id().as_bytes()));
    session
        .write_all(format!("NAMING LOOKUP NAME={b32_name}\n").as_bytes())
        .await
        .unwrap();
    let reply = line(&mut session).await;
    assert!(reply.contains("NAMING REPLY RESULT=OK"));
    assert!(reply.contains(&format!("VALUE={public}")));

    let mut utility = TcpStream::connect(address).await.unwrap();
    hello(&mut utility).await;
    utility.write_all(b"NAMING LOOKUP NAME=ME\n").await.unwrap();
    assert!(line(&mut utility).await.contains("RESULT=INVALID_NAME"));
    utility
        .write_all(b"NAMING LOOKUP NAME=unknown.i2p\n")
        .await
        .unwrap();
    assert!(line(&mut utility).await.contains("RESULT=KEY_NOT_FOUND"));
    utility
        .write_all(b"NAMING LOOKUP NAME=not-a-destination\n")
        .await
        .unwrap();
    assert!(line(&mut utility).await.contains("RESULT=INVALID_KEY"));

    drop(session);
    drop(utility);
    // Session teardown is performed by the per-connection child task; keep
    // this deterministic and bounded while allowing slower CI schedulers to
    // run that task after both control sockets close.
    for _ in 0..256 {
        tokio::task::yield_now().await;
    }
    assert_eq!(state.session_registry().session_count(), 0);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = children.shutdown().await;
}

/// Plan 151 §10 matrix helpers: register one loopback FORWARD owned
/// by a fresh control socket and return that socket.
async fn register_forward(address: SocketAddr, target: SocketAddr, silent: bool) -> TcpStream {
    let mut forward = TcpStream::connect(address).await.unwrap();
    hello(&mut forward).await;
    let command = format!(
        "STREAM FORWARD ID=forward PORT={} HOST={} SILENT={}\n",
        target.port(),
        target.ip(),
        if silent { "true" } else { "false" },
    );
    forward.write_all(command.as_bytes()).await.unwrap();
    assert_eq!(line(&mut forward).await, "STREAM STATUS RESULT=OK\n");
    forward
}

/// Serves `exchanges` forwarded streams on `listener` with the exact
/// non-silent metadata + hello/world trajectory.
async fn run_echo_target(listener: TcpListener, exchanges: usize) {
    for _ in 0..exchanges {
        let (target, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(target);
        let mut metadata = Vec::new();
        reader.read_until(b'\n', &mut metadata).await.unwrap();
        assert_eq!(metadata, b"DESTINATION=peer-public\n");
        let mut payload = [0_u8; 5];
        reader.read_exact(&mut payload).await.unwrap();
        assert_eq!(&payload, b"hello");
        let mut target = reader.into_inner();
        target.write_all(b"world").await.unwrap();
        target.shutdown().await.unwrap();
    }
}

/// Bridges one already-accepted Streaming byte socket through the
/// live registration and proves exact hello/world bytes.
async fn bridge_one_exchange(state: &Arc<SamServiceState>, session: &i2pr_api::SamSessionId) {
    let source_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source_address = source_listener.local_addr().unwrap();
    let mut source_client = TcpStream::connect(source_address).await.unwrap();
    let (source, _) = source_listener.accept().await.unwrap();
    let bridge = tokio::spawn({
        let state = Arc::clone(state);
        let session = session.clone();
        let cancellation = CancellationToken::new();
        async move {
            state
                .bridge_forwarded_stream(&session, source, Some("peer-public"), cancellation)
                .await
        }
    });
    source_client.write_all(b"hello").await.unwrap();
    let mut reply = [0_u8; 5];
    source_client.read_exact(&mut reply).await.unwrap();
    assert_eq!(&reply, b"world");
    bridge.await.unwrap().unwrap();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn forward_second_stream_reuses_live_registration() {
    // Plan 151 §10 item 3: the registration outlives one bridged
    // stream; a second independent forwarded stream succeeds.
    let (state, address, children, parent) = start().await;
    let mut control = TcpStream::connect(address).await.unwrap();
    hello(&mut control).await;
    let _public = create(&mut control).await;
    let session = i2pr_api::SamSessionId::new("forward").unwrap();

    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    let mut forward = register_forward(address, target_address, false).await;
    assert!(state.forward_registration(&session).is_some());

    let target_task = tokio::spawn(run_echo_target(target_listener, 2));
    bridge_one_exchange(&state, &session).await;
    assert!(
        state.forward_registration(&session).is_some(),
        "registration must survive a completed forwarded stream"
    );
    bridge_one_exchange(&state, &session).await;
    target_task.await.unwrap();

    forward.write_all(b"QUIT\n").await.unwrap();
    let mut eof = [0_u8; 1];
    assert_eq!(forward.read(&mut eof).await.unwrap(), 0);
    assert!(state.forward_registration(&session).is_none());

    drop(control);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = children.shutdown().await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn forward_target_refusal_is_typed_and_registration_survives() {
    // Plan 151 §10 item 4: a refused loopback target surfaces a typed
    // I/O error within the test deadline, leaks no attachment, keeps
    // the registration, and a later live target bridges fine.
    let (state, address, children, parent) = start().await;
    let mut control = TcpStream::connect(address).await.unwrap();
    hello(&mut control).await;
    let _public = create(&mut control).await;
    let session = i2pr_api::SamSessionId::new("forward").unwrap();

    // Reserve a loopback port, then release it so nothing listens.
    let held = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let refused = held.local_addr().unwrap();
    drop(held);

    let mut forward = register_forward(address, refused, false).await;
    let attachments_before = state.stream_registry().attachment_count();
    let outcome = state.connect_forward_target(&session).await;
    assert!(
        matches!(outcome, Err(ForwardBridgeError::Io(_))),
        "refused target must surface I/O, not timeout: {outcome:?}"
    );
    assert!(
        state.forward_registration(&session).is_some(),
        "refusal must not unregister the forward"
    );
    assert_eq!(
        state.stream_registry().attachment_count(),
        attachments_before,
        "refusal must leak no attachment"
    );

    // Owner close, re-register against a live target, bridge exactly.
    forward.write_all(b"QUIT\n").await.unwrap();
    let mut eof = [0_u8; 1];
    assert_eq!(forward.read(&mut eof).await.unwrap(), 0);
    assert!(state.forward_registration(&session).is_none());

    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    let mut forward = register_forward(address, target_address, false).await;
    let target_task = tokio::spawn(run_echo_target(target_listener, 1));
    bridge_one_exchange(&state, &session).await;
    target_task.await.unwrap();

    forward.write_all(b"QUIT\n").await.unwrap();
    assert_eq!(forward.read(&mut eof).await.unwrap(), 0);
    assert!(state.forward_registration(&session).is_none());

    drop(control);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = children.shutdown().await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn forward_unresponsive_target_times_out_within_policy() {
    // Plan 151 §10 item 5: a target that never completes the TCP
    // handshake (full backlog, SYNs dropped) terminates with the
    // configured 3 s policy timeout — no hang, no attachment leak.
    // The paused clock advances deterministically past the policy.
    let (state, address, children, parent) = start().await;
    let mut control = TcpStream::connect(address).await.unwrap();
    hello(&mut control).await;
    let _public = create(&mut control).await;
    let session = i2pr_api::SamSessionId::new("forward").unwrap();

    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    // Saturate the accept backlog and leave SYNs unanswered; the
    // later forward connect then pends instead of refusing.
    let mut fillers = Vec::new();
    for _ in 0..400 {
        let address = target_address;
        fillers.push(tokio::spawn(async move {
            let socket = TcpStream::connect(address).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            drop(socket);
        }));
    }
    for _ in 0..1024 {
        tokio::task::yield_now().await;
    }

    let _forward = register_forward(address, target_address, false).await;
    let attachments_before = state.stream_registry().attachment_count();
    let connect = tokio::spawn({
        let state = Arc::clone(&state);
        let session = session.clone();
        async move { state.connect_forward_target(&session).await }
    });
    tokio::time::advance(std::time::Duration::from_secs(5)).await;
    let outcome = connect.await.unwrap();
    assert!(
        matches!(outcome, Err(ForwardBridgeError::Timeout)),
        "hung target must surface the policy timeout: {outcome:?}"
    );
    assert!(
        state.forward_registration(&session).is_some(),
        "timeout must not unregister the forward"
    );
    assert_eq!(
        state.stream_registry().attachment_count(),
        attachments_before,
        "timeout must leak no attachment"
    );

    for filler in fillers {
        filler.abort();
    }
    drop(control);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = children.shutdown().await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn forward_rejects_non_loopback_target() {
    // Plan 151 §10 item 8: non-loopback literals and hostnames are
    // rejected at parse time (no resolver, no packet leaves
    // loopback) and register nothing.
    let (state, address, children, parent) = start().await;
    let mut control = TcpStream::connect(address).await.unwrap();
    hello(&mut control).await;
    let _public = create(&mut control).await;
    let session = i2pr_api::SamSessionId::new("forward").unwrap();

    for host in ["93.184.216.34", "example.com", "0.0.0.0"] {
        let mut forward = TcpStream::connect(address).await.unwrap();
        hello(&mut forward).await;
        let command = format!("STREAM FORWARD ID=forward PORT=1234 HOST={host}\n");
        forward.write_all(command.as_bytes()).await.unwrap();
        let reply = line(&mut forward).await;
        assert!(
            reply.starts_with("STREAM STATUS RESULT=INVALID_KEY"),
            "non-loopback HOST={host} must be rejected: {reply:?}"
        );
        assert!(
            state.forward_registration(&session).is_none(),
            "rejected HOST={host} must register nothing"
        );
    }

    drop(control);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = children.shutdown().await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn forward_and_accept_stay_mutually_exclusive() {
    // Plan 151 §10 item 7: an active FORWARD rejects ACCEPT and a
    // pending ACCEPT rejects FORWARD, both with I2P_ERROR.
    let (state, address, children, parent) = start().await;
    let mut control = TcpStream::connect(address).await.unwrap();
    hello(&mut control).await;
    let _public = create(&mut control).await;
    let session = i2pr_api::SamSessionId::new("forward").unwrap();

    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    let mut forward = register_forward(address, target_address, false).await;

    let mut accepter = TcpStream::connect(address).await.unwrap();
    hello(&mut accepter).await;
    accepter
        .write_all(b"STREAM ACCEPT ID=forward\n")
        .await
        .unwrap();
    let reply = line(&mut accepter).await;
    assert!(
        reply.starts_with("STREAM STATUS RESULT=I2P_ERROR"),
        "ACCEPT during active FORWARD must be rejected: {reply:?}"
    );

    // Release the forward, then park an ACCEPT and prove FORWARD is
    // rejected while the accept is pending.
    forward.write_all(b"QUIT\n").await.unwrap();
    let mut eof = [0_u8; 1];
    assert_eq!(forward.read(&mut eof).await.unwrap(), 0);
    assert!(state.forward_registration(&session).is_none());

    let mut accepter = TcpStream::connect(address).await.unwrap();
    hello(&mut accepter).await;
    accepter
        .write_all(b"STREAM ACCEPT ID=forward\n")
        .await
        .unwrap();
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    let mut forward = TcpStream::connect(address).await.unwrap();
    hello(&mut forward).await;
    let command = format!(
        "STREAM FORWARD ID=forward PORT={} HOST=127.0.0.1\n",
        target_address.port()
    );
    forward.write_all(command.as_bytes()).await.unwrap();
    let reply = line(&mut forward).await;
    assert!(
        reply.starts_with("STREAM STATUS RESULT=I2P_ERROR"),
        "FORWARD during pending ACCEPT must be rejected: {reply:?}"
    );

    drop(accepter);
    drop(forward);
    drop(control);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = children.shutdown().await;
}
