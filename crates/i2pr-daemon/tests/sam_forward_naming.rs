//! Plan 139 loopback forwarding and naming tests.

use std::net::SocketAddr;
use std::sync::Arc;

use i2pr_api::sam::limits::SamLimits;
use i2pr_daemon::config::SamConfig;
use i2pr_daemon::sam::SamServiceState;
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
    let public = create(&mut session).await;
    session.write_all(b"NAMING LOOKUP NAME=ME\n").await.unwrap();
    let reply = line(&mut session).await;
    assert!(reply.contains("NAMING REPLY RESULT=OK"), "{reply}");
    assert!(reply.contains(&format!("VALUE={public}")));

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
