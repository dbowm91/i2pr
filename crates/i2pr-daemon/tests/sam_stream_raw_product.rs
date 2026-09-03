//! Plan 147 SAM 3.1 dedicated raw STREAM driver end-to-end test.
//!
//! This is the retained Plan 147 regression for the dedicated raw
//! TCP↔Streaming product path. Two SAM destinations are created
//! entirely through the daemon's SAM listener; the test
//!
//! 1. binds two TCP clients to the SAM listener,
//! 2. issues HELLO + SESSION CREATE on both,
//! 3. issues STREAM CONNECT on side A and STREAM ACCEPT on side B,
//! 4. lets the per-destination runtime drivers drive the SYN/SYN
//!    response handshake through the Plan 129 local seam,
//! 5. once both sides report `STREAM STATUS RESULT=OK`, exchanges
//!    application bytes through the dedicated raw driver,
//! 6. verifies byte-for-byte equality across the bridge.
//!
//! The test never invokes product-composition helpers or custom
//! in-process byte-moving loops. The canonical Plan 149 product
//! evidence is in `sam_stream_self_composed.rs`; this smaller test
//! remains as a focused raw-driver regression.

#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;

use i2pr_api::sam::limits::SamLimits;
use i2pr_daemon::config::SamConfig;
use i2pr_daemon::sam::SamServiceState;
use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
    loop {
        let mut byte = [0_u8; 1];
        match stream.read_exact(&mut byte).await {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&byte[..n]);
                if byte[0] == b'\n' {
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

fn destination_identity(seed: u64) -> i2pr_client::DestinationIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    i2pr_client::DestinationIdentity::generate(&mut rng).expect("identity")
}

#[tokio::test(flavor = "current_thread")]
async fn plan147_dedicated_raw_driver_exchanges_application_bytes() {
    let (state, address, scope, parent) = start_listener(sam_config()).await;

    // ----- TCP clients: A = CONNECT, B = ACCEPT -----
    let mut client_a = TcpStream::connect(address).await.expect("connect a");
    let mut client_b = TcpStream::connect(address).await.expect("connect b");
    hello_3_1(&mut client_a).await;
    hello_3_1(&mut client_b).await;

    // SESSION CREATE imports the supplied private destinations. The
    // production handler now self-composes the bridge, local
    // LeaseSet2 directory, inbound-tunnel factory, and runtime
    // driver before returning the success line.
    let identity_a = destination_identity(0xA1);
    let identity_b = destination_identity(0xB2);
    let priv_a =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(&identity_a)
            .expect("identity a round-trips")
            .encode_base64();
    let priv_b =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(&identity_b)
            .expect("identity b round-trips")
            .encode_base64();
    let pub_b =
        i2pr_api::sam::private_destination::SamPrivateDestination::from_identity(&identity_b)
            .expect("identity b round-trips")
            .encode_public_base64();

    write_all(
        &mut client_a,
        format!("SESSION CREATE STYLE=STREAM ID=alpha DESTINATION={priv_a}\n").as_bytes(),
    )
    .await;
    let reply_a = read_one_line(&mut client_a).await;
    assert!(
        reply_a.contains("SESSION STATUS RESULT=OK"),
        "session alpha A failed: {reply_a:?}"
    );
    write_all(
        &mut client_b,
        format!("SESSION CREATE STYLE=STREAM ID=beta DESTINATION={priv_b}\n").as_bytes(),
    )
    .await;
    let reply_b = read_one_line(&mut client_b).await;
    assert!(
        reply_b.contains("SESSION STATUS RESULT=OK"),
        "session beta B failed: {reply_b:?}"
    );

    // Issue STREAM ACCEPT on B; issue STREAM CONNECT on A.
    // Both calls happen in parallel tasks so the runtime driver
    // can drive the handshake while both clients are parked on
    // their `read_one_line` futures.
    let accept_task = tokio::spawn(async move {
        write_all(&mut client_b, b"STREAM ACCEPT ID=beta\n").await;
        let reply = read_one_line(&mut client_b).await;
        let peer = read_one_line(&mut client_b).await;
        (client_b, reply, peer)
    });
    let connect_task = tokio::spawn(async move {
        let cmd = format!("STREAM CONNECT ID=alpha DESTINATION={pub_b}\n");
        write_all(&mut client_a, cmd.as_bytes()).await;
        let reply = read_one_line(&mut client_a).await;
        (client_a, reply)
    });
    let (accept_result, connect_result) = tokio::join!(accept_task, connect_task);
    let (mut client_b, accept_reply, accept_peer) = accept_result.expect("accept task");
    let (mut client_a, connect_reply) = connect_result.expect("connect task");
    assert!(
        accept_reply.contains("STREAM STATUS RESULT=OK"),
        "ACCEPT did not return OK, got {accept_reply:?}"
    );
    assert!(
        accept_peer.starts_with("DESTINATION="),
        "ACCEPT peer destination missing, got {accept_peer:?}"
    );
    assert!(
        connect_reply.contains("STREAM STATUS RESULT=OK"),
        "CONNECT did not return OK, got {connect_reply:?}"
    );

    // ----- Drive raw byte exchange through the dedicated driver -----
    let payload_a: Vec<u8> = (0..1024_u32).map(|i| (i & 0xFF) as u8).collect();
    let payload_b: Vec<u8> = (0..2048_u32)
        .map(|i| ((i.wrapping_mul(7)) & 0xFF) as u8)
        .collect();
    let write_a = async {
        write_all(&mut client_a, &payload_a).await;
        let mut buf = Vec::with_capacity(payload_b.len());
        let mut chunk = [0_u8; 256];
        let mut received = 0_usize;
        while received < payload_b.len() {
            let n = client_a.read(&mut chunk).await.expect("client_a read");
            assert!(n > 0, "client_a EOF before receiving payload_b");
            buf.extend_from_slice(&chunk[..n]);
            received += n;
        }
        buf
    };
    let write_b = async {
        write_all(&mut client_b, &payload_b).await;
        let mut buf = Vec::with_capacity(payload_a.len());
        let mut chunk = [0_u8; 256];
        let mut received = 0_usize;
        while received < payload_a.len() {
            let n = client_b.read(&mut chunk).await.expect("client_b read");
            assert!(n > 0, "client_b EOF before receiving payload_a");
            buf.extend_from_slice(&chunk[..n]);
            received += n;
        }
        buf
    };
    let (received_a, received_b) = tokio::join!(write_a, write_b);
    assert_eq!(received_a, payload_b, "client_a did not receive payload_b");
    assert_eq!(received_b, payload_a, "client_b did not receive payload_a");

    drop(client_a);
    drop(client_b);
    parent.cancel(i2pr_core::CancellationReason::OperatorRequest);
    let _ = scope.shutdown().await;
    drop(state);
}
